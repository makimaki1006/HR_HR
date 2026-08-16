//! 架電クオリティ: 全社サマリ（GAS 版 page-p0）
//!
//! 2026-08-14 移植。GAS 版の構成:
//!   KPI カード / 営業日ペース進捗 / 月の接触規模 /
//!   月次3連（架電数・アポ率・NA遵守率、直近8ヶ月）/
//!   Recency 全社サマリ / Deal Health Critical Top20 /
//!   トップパフォーマー・要支援 Top3
//!
//! **このタブは経営が見る画面なので、定義のズレが最も高くつく。**
//! GAS 版でこの2ヶ月に潰した誤りを、ここに再度作り込まないこと:
//!   - アポ率の分母は Zoom発信（都道府県モードのときだけ HubSpot Call）
//!   - **足切りは率と同じ分母で行う**（HubSpot Call で足切りして Zoom分母で率を出さない）
//!   - 分母0のアポ率は 0% でなく null
//!   - 営業スコープの既定は role=sales（BPO/コンサルを混ぜない）

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

/// アポ率ランキングの最低分母。これ未満は少サンプルで率が不安定なため順位から外す。
/// GAS 版の `MIN_CALLS_FOR_RATE` と同値。
const MIN_DEN_FOR_RATE: f64 = 100.0;

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct OverviewQuery {
    /// 対象年月 (YYYY-MM)。未指定なら最新月。
    pub year_month: Option<String>,
    /// 都道府県。指定すると**分母が Zoom発信 → HubSpot Call に切り替わる**。
    /// GAS 版 `_apoDen()` と同じ挙動。
    pub prefecture: Option<String>,
    /// カンマ区切り owner_id。未指定なら role=sales のみ。
    pub owners: Option<String>,
}

crate::accepted_params!(OverviewQuery, overview_query_accepted =>
    "year_month", "prefecture", "owners");

#[derive(Debug, Serialize)]
pub struct OverviewData {
    pub kpi: Kpi,
    /// 直近8ヶ月の月次推移
    pub monthly: Vec<MonthlyPoint>,
    pub top_performers: Vec<OwnerRate>,
    pub needs_support: Vec<OwnerRate>,
    /// アポ率の分母が何だったかを画面に出すためのラベル。
    /// GAS 版で「単独表記禁止（分母2版がある）」と決めた運用ルールへの対応。
    pub apo_denominator_label: String,
}

#[derive(Debug, Serialize, Default)]
pub struct Kpi {
    pub call_count: f64,
    pub zoom_dial_count: f64,
    pub apo_count: f64,
    /// 分母0なら null（0% にしない）
    pub apo_rate: Option<f64>,
    pub na_due: f64,
    pub na_done_ontime: f64,
    pub na_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct MonthlyPoint {
    pub year_month: String,
    pub call_count: f64,
    pub zoom_dial_count: f64,
    pub apo_count: f64,
    pub apo_rate: Option<f64>,
    /// 2026-08-17 追加: KPI カードで実数を出すために持たせる（率だけでは
    /// 「何件中の何件か」が分からず、少ない母数の率を過信させる）
    pub na_due: f64,
    pub na_done_ontime: f64,
    pub na_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct OwnerRate {
    pub owner_id: String,
    pub apo_count: f64,
    /// 率の分母そのもの。足切りにもこれを使う（GAS 版で不一致を起こした箇所）
    pub denominator: f64,
    pub call_count: f64,
    pub apo_rate: Option<f64>,
}

/// 都道府県が「実際に選ばれている」か。
///
/// 2026-08-16 追加。画面の「全都道府県」は **`__all__` という番兵**を送ってくる
/// （GAS 版 index.html の `<option value="__all__">`）。これを県名として扱うと
/// 「__all__ という県」を探しに行って **0件**になる。空文字と同じく
/// 「絞らない」を意味するので、ここで吸収する。
fn pref_selected(v: &Option<String>) -> Option<&str> {
    v.as_deref()
        .filter(|p| !p.is_empty() && *p != "__all__")
}

/// 進行中の当月か。GAS `_isPartialMonth` と同じで先頭7文字(YYYY-MM)で判定する。
fn is_partial_month(ym: &str, current_ym: &str) -> bool {
    ym.len() >= 7 && current_ym.len() >= 7 && ym[..7] == current_ym[..7]
}

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// アポ率の分母を返す。
///
/// GAS 版 `_apoDen(callSum, zoomSum)` と同じ規則:
///   都道府県モード → HubSpot Call
///   通常          → Zoom発信（無ければ HubSpot Call にフォールバック）
///
/// **注意**: GAS 版では「県を選んだ直後（データ未ロード）に、行は全国のまま
/// 分母だけ切り替わる」不具合があった。ここでは絞り込み後の行に対して
/// 常に同じ規則を適用するので、その齟齬は起きない。
fn apo_denominator(call: f64, zoom: f64, pref_mode: bool) -> f64 {
    if pref_mode {
        call
    } else if zoom > 0.0 {
        zoom
    } else {
        call
    }
}

fn denominator_label(pref_mode: bool) -> String {
    if pref_mode {
        "HubSpot Call（都道府県で絞り込み中）".to_string()
    } else {
        "Zoom発信（行動量分母）".to_string()
    }
}

/// 月次明細から owner×年月 の集計を作る。
///
/// 使うシート: 「月次明細」
/// 使う列: owner_id / year_month / call_count / zoom_dial_count / apo_count /
///         na_due / na_done_ontime
/// いずれも**列名で引く**（位置で決め打ちしない）。
pub fn collect(
    data: &SheetData,
    q: &OverviewQuery,
    sales_owners: Option<&Vec<String>>,
) -> (Vec<MonthlyPoint>, HashMap<String, OwnerRate>, usize) {
    // 2026-08-17 是正: ここだけ `!p.is_empty()` の旧判定が残っており、
    //   `__all__`(画面の「全都道府県」の番兵)で **分母だけ HubSpot Call に化けていた**。
    //   handle() 側は pref_selected() を使うのでシートは月次明細のまま・
    //   ラベルも「Zoom発信」のままなので、**表示と計算が食い違う**。
    //   実測(2026-05): 正 1032÷67,208=1.54% → 誤 1032÷49,659=2.08%（差 0.54pt）
    let pref_mode = pref_selected(&q.prefecture).is_some();

    let owner_filter: Option<Vec<String>> = q.owners.as_ref().map(|s| {
        s.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    });

    // 年月 → 合計
    let mut by_month: HashMap<String, [f64; 5]> = HashMap::new();
    // owner → 合計（当月分のみ）
    let mut by_owner: HashMap<String, [f64; 3]> = HashMap::new();
    let mut matched = 0usize;

    // 対象月。未指定なら最新月を後で決めるため、まず全月を集める
    let target = q.year_month.clone();

    for row in &data.rows {
        let owner = data.get(row, "owner_id").to_string();

        // スコープ: メンバー指定があればその人、無ければ role=sales のみ。
        // GAS 版でここを省いて 141名(sales30/bpo32/consultant29/other50)を
        // 混ぜ、アポ率が 0.93% → 0.63% に希釈された事故がある。
        match owner_filter.as_ref() {
            Some(ids) if !ids.is_empty() => {
                if !ids.contains(&owner) {
                    continue;
                }
            }
            _ => {
                if let Some(sales) = sales_owners {
                    if !sales.contains(&owner) {
                        continue;
                    }
                }
            }
        }

        let ym = data.get(row, "year_month").to_string();
        if ym.is_empty() {
            continue;
        }

        // 2026-08-16 追加: 都道府県で**行を絞る**。
        //   従来は分母を HubSpot Call に切り替えるだけで行を絞っておらず、
        //   「東京都を選んでも全国の集計が出る」状態だった。
        //   handle 側で「都道府県月次」シートへ差し替え、ここで列で絞る。
        if let Some(pref) = pref_selected(&q.prefecture) {
            if data.col("prefecture").is_some() && data.get(row, "prefecture") != pref {
                continue;
            }
        }

        let call = num(data.get(row, "call_count"));
        let zoom = num(data.get(row, "zoom_dial_count"));
        let apo = num(data.get(row, "apo_count"));
        let na_due = num(data.get(row, "na_due"));
        let na_ok = num(data.get(row, "na_done_ontime"));

        let e = by_month.entry(ym.clone()).or_insert([0.0; 5]);
        e[0] += call;
        e[1] += zoom;
        e[2] += apo;
        e[3] += na_due;
        e[4] += na_ok;
        matched += 1;

        if target.as_deref().map(|t| t == ym).unwrap_or(false) {
            let o = by_owner.entry(owner).or_insert([0.0; 3]);
            o[0] += call;
            o[1] += zoom;
            o[2] += apo;
        }
    }

    // 月次は年月昇順で安定させる（HashMap の反復順を返さない）
    let mut months: Vec<String> = by_month.keys().cloned().collect();
    months.sort();
    let monthly: Vec<MonthlyPoint> = months
        .iter()
        .map(|ym| {
            let v = by_month[ym];
            let den = apo_denominator(v[0], v[1], pref_mode);
            MonthlyPoint {
                year_month: ym.clone(),
                call_count: v[0],
                zoom_dial_count: v[1],
                apo_count: v[2],
                apo_rate: rate(v[2], den),
                na_due: v[3],
                na_done_ontime: v[4],
                na_rate: rate(v[4], v[3]),
            }
        })
        .collect();

    let owners: HashMap<String, OwnerRate> = by_owner
        .into_iter()
        .map(|(id, v)| {
            let den = apo_denominator(v[0], v[1], pref_mode);
            (
                id.clone(),
                OwnerRate {
                    owner_id: id,
                    apo_count: v[2],
                    denominator: den,
                    call_count: v[0],
                    apo_rate: rate(v[2], den),
                },
            )
        })
        .collect();

    (monthly, owners, matched)
}

/// トップ3 / ワースト3 を選ぶ。
///
/// **足切りは率と同じ分母（denominator）で行う。**
/// GAS 版では足切りが HubSpot Call、率の分母が Zoom発信で不一致になっており、
/// Zoom発信が少ない担当者が足切りを通過して不安定な率で上位に出ていた。
fn rank(owners: &HashMap<String, OwnerRate>) -> (Vec<OwnerRate>, Vec<OwnerRate>) {
    let mut q: Vec<&OwnerRate> = owners
        .values()
        .filter(|o| o.denominator >= MIN_DEN_FOR_RATE)
        .collect();
    // 率降順。同率は owner_id で安定化
    q.sort_by(|a, b| {
        b.apo_rate
            .unwrap_or(0.0)
            .partial_cmp(&a.apo_rate.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    let clone = |o: &&OwnerRate| OwnerRate {
        owner_id: o.owner_id.clone(),
        apo_count: o.apo_count,
        denominator: o.denominator,
        call_count: o.call_count,
        apo_rate: o.apo_rate,
    };
    let top: Vec<OwnerRate> = q.iter().take(3).map(clone).collect();
    let bottom: Vec<OwnerRate> = q.iter().rev().take(3).map(clone).collect();
    (top, bottom)
}

pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    mut q: OverviewQuery,
    sales_owners: Option<Vec<String>>,
) -> Result<TabPayload<OverviewData>> {
    let started = Instant::now();

    // 都道府県を選んだときは土台シートごと差し替える（GAS 版と同じ）。
    // 「月次明細」には prefecture 列が無いので、そのままでは絞りようがない。
    let pref_mode = pref_selected(&q.prefecture).is_some();
    let sheet = if pref_mode { "都道府県月次" } else { "月次明細" };
    let (data, from_cache) = store.get(client, sheet).await?;

    // 対象月未指定なら最新月にする（画面の既定挙動）
    if q.year_month.is_none() {
        let mut latest: Option<String> = None;
        for row in &data.rows {
            let ym = data.get(row, "year_month");
            if !ym.is_empty() && latest.as_deref().map(|l| ym > l).unwrap_or(true) {
                latest = Some(ym.to_string());
            }
        }
        q.year_month = latest;
    }

    let (monthly_all, owners, matched) = collect(&data, &q, sales_owners.as_ref());

    // 直近8ヶ月（GAS 版と同じ）
    // 2026-08-17 是正3点（検証チームがソース精査で発見）:
    //
    //  (a) **KPI を「8ヶ月に切り詰めた後」の配列から探していた**。
    //      データは9ヶ月あるため `?year_month=2025-12` のように窓外の月を指定すると
    //      find() が外れて `unwrap_or_default()` = **KPI 全項目0・アポ率 null** になっていた。
    //      実データでは 架電20,539 / Zoom33,643 / アポ252 がある月。
    //      → 切り詰め **前** の全月から探す。
    //
    //  (b) **進行中の当月（部分月）を推移から除外していなかった**。
    //      GAS `renderP0MiniCharts` は `_isPartialMonth` で除外している。
    //      実データの 2026-08 は Zoom発信 3,393 に対し HubSpot Call 9,712 で
    //      集計が途中のため、アポ率が **6.60%**（前月 1.62%）と出る。
    //      GAS が意図的に消していた誤表示そのものだった。
    //      → 推移からは部分月を除く。**KPI カードは当月を出す**（GAS も同じ。
    //        カードには「集計途中」の注記が付く運用）。
    //
    //  (c) **KPI の NA 実数が固定0**だった（率だけ正しく実数が出ない）。
    //      → 月次側に実数を持たせて渡す。
    // 2026-08-17 是正: 「KPIカードで見たい月」と「進行中で信用できない月」は
    //   別物。当初この2つを同じ変数にしてしまい、**2026-05 を選ぶと推移から
    //   2026-05 が消えて、進行中の 2026-08 は残る**という逆の挙動になっていた。
    //   GAS の `_isPartialMonth` は `_currentYM()`(実際の今月)と比べており、
    //   画面で選んだ月とは無関係。
    let selected_ym = q
        .year_month
        .clone()
        .filter(|s| !s.is_empty())
        .or_else(|| monthly_all.last().map(|m| m.year_month.clone()))
        .unwrap_or_default();
    let partial_ym = super::jst_current_ym();

    // KPI は切り詰め前の全月から探す（(a)）
    let kpi = monthly_all
        .iter()
        .find(|m| m.year_month == selected_ym)
        .map(|m| Kpi {
            call_count: m.call_count,
            zoom_dial_count: m.zoom_dial_count,
            apo_count: m.apo_count,
            apo_rate: m.apo_rate,
            na_due: m.na_due,
            na_done_ontime: m.na_done_ontime,
            na_rate: m.na_rate,
        })
        .unwrap_or_default();

    // 推移は部分月を除いて直近8ヶ月（(b)）
    let monthly: Vec<MonthlyPoint> = monthly_all
        .into_iter()
        .filter(|m| !is_partial_month(&m.year_month, &partial_ym))
        .rev()
        .take(8)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let (top, bottom) = rank(&owners);

    Ok(TabPayload {
        data: OverviewData {
            kpi,
            monthly,
            top_performers: top,
            needs_support: bottom,
            apo_denominator_label: denominator_label(pref_mode),
        },
        sources: vec![SourceInfo {
            sheet: sheet.to_string(),
            total_rows: data.rows.len(),
            matched_rows: matched,
            from_cache,
            age_secs: data.fetched_at.elapsed().as_secs(),
        }],
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: Vec<(&str, &str, f64, f64, f64)>) -> SheetData {
        let header = vec![
            "owner_id".to_string(),
            "year_month".to_string(),
            "call_count".to_string(),
            "zoom_dial_count".to_string(),
            "apo_count".to_string(),
            "na_due".to_string(),
            "na_done_ontime".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(o, ym, c, z, a)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(o),
                    Arc::from(ym),
                    Arc::from(c.to_string().as_str()),
                    Arc::from(z.to_string().as_str()),
                    Arc::from(a.to_string().as_str()),
                    Arc::from("0"),
                    Arc::from("0"),
                ]
            })
            .collect();
        SheetData {
            header,
            rows,
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn 分母はzoom発信が既定() {
        assert_eq!(apo_denominator(100.0, 300.0, false), 300.0);
    }

    #[test]
    fn 都道府県モードでは分母がhubspot_callになる() {
        // GAS 版 _apoDen と同じ規則
        assert_eq!(apo_denominator(100.0, 300.0, true), 100.0);
    }

    #[test]
    fn zoomが0ならcallにフォールバックする() {
        assert_eq!(apo_denominator(100.0, 0.0, false), 100.0);
    }

    #[test]
    fn 分母0のアポ率はnull() {
        let d = sheet(vec![("1", "2026-05", 0.0, 0.0, 3.0)]);
        let q = OverviewQuery {
            year_month: Some("2026-05".into()),
            ..Default::default()
        };
        let (monthly, _, _) = collect(&d, &q, None);
        assert!(
            monthly[0].apo_rate.is_none(),
            "架電0でアポ3件を 0% と表示してはいけない"
        );
    }

    #[test]
    fn 足切りは率と同じ分母で行う() {
        // Zoom発信が少ないが HubSpot Call は多い担当者。
        // GAS 版は call>=100 で足切りしていたため、この人が
        // 不安定な率(3/20=15%)のまま1位に出ていた。
        let mut owners = HashMap::new();
        owners.insert(
            "thin".to_string(),
            OwnerRate {
                owner_id: "thin".into(),
                apo_count: 3.0,
                denominator: 20.0, // Zoom発信20 → 足切り未満
                call_count: 500.0, // HubSpot Call は多い
                apo_rate: rate(3.0, 20.0),
            },
        );
        owners.insert(
            "normal".to_string(),
            OwnerRate {
                owner_id: "normal".into(),
                apo_count: 5.0,
                denominator: 500.0,
                call_count: 400.0,
                apo_rate: rate(5.0, 500.0),
            },
        );
        let (top, _) = rank(&owners);
        assert_eq!(top.len(), 1, "分母20の担当者は足切りで除外される");
        assert_eq!(top[0].owner_id, "normal");
    }

    #[test]
    fn 月次は年月昇順で安定する() {
        let d = sheet(vec![
            ("1", "2026-05", 100.0, 200.0, 2.0),
            ("1", "2026-03", 100.0, 200.0, 1.0),
            ("1", "2026-04", 100.0, 200.0, 3.0),
        ]);
        let (m, _, _) = collect(&d, &OverviewQuery::default(), None);
        let ys: Vec<&str> = m.iter().map(|x| x.year_month.as_str()).collect();
        assert_eq!(ys, vec!["2026-03", "2026-04", "2026-05"]);
    }

    #[test]
    fn 営業以外は既定で除外される() {
        let d = sheet(vec![
            ("sales1", "2026-05", 100.0, 200.0, 2.0),
            ("bpo1", "2026-05", 900.0, 900.0, 1.0),
        ]);
        let sales = vec!["sales1".to_string()];
        let (m, _, matched) = collect(&d, &OverviewQuery::default(), Some(&sales));
        assert_eq!(matched, 1, "BPO の行が混ざってはいけない");
        assert_eq!(m[0].zoom_dial_count, 200.0);
    }

    #[test]
    fn 分母ラベルがモードで変わる() {
        // 「アポ率」を単独表記しないための運用ルールへの対応
        assert!(denominator_label(false).contains("Zoom発信"));
        assert!(denominator_label(true).contains("HubSpot Call"));
    }
    #[test]
    fn 全都道府県の番兵は絞り込みとして扱わない() {
        // 画面の「全都道府県」は `__all__` を送る（GAS index.html の option value）。
        // これを県名として扱うと「__all__ という県」を探して **0件**になる。
        // 実データで実際に 0件になることを確認して見つけた不具合。
        assert_eq!(pref_selected(&Some("__all__".to_string())), None);
        assert_eq!(pref_selected(&Some(String::new())), None);
        assert_eq!(pref_selected(&None), None);
        assert_eq!(
            pref_selected(&Some("東京都".to_string())),
            Some("東京都"),
            "実在する県名はそのまま絞り込みに使う"
        );
    }

}
