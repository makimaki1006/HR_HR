//! 架電クオリティ: メンバー比較（GAS 版 page-p1）
//!
//! 2026-08-14 移植。GAS 版の構成:
//!   選択メンバー比較 / メンバー特性レーダー(2-5名) / アポ率ランキング /
//!   架電数ランキング / 量×質マップ / 規模別アポ率 / メンバー別平均通話秒数
//!
//! GAS 版でこのタブに入っていた誤りを持ち込まないこと:
//!   - アポ率ランキングの足切りが HubSpot Call、率の分母が Zoom発信で**不一致**だった
//!   - `render()` が `o.call > 0` で担当者を落としており、
//!     HubSpot Call 未記録の担当者が KPI 合計には入るのにランキングから消えて
//!     **合計と内訳が一致しない**状態だった
//!   両方ともここでは最初から揃えてある。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

/// 率ランキングの最低分母。これ未満は少サンプルで率が不安定。
/// **率と同じ分母で足切りする**（GAS 版の不一致を再現しない）。
const MIN_DEN_FOR_RATE: f64 = 100.0;

#[derive(Debug, Default, Deserialize)]
pub struct MembersQuery {
    pub year_month: Option<String>,
    /// 指定すると分母が Zoom発信 → HubSpot Call に切り替わる
    pub prefecture: Option<String>,
    /// カンマ区切り owner_id。未指定なら role=sales のみ。
    pub owners: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MemberRow {
    pub owner_id: String,
    pub call_count: f64,
    pub zoom_dial_count: f64,
    pub apo_count: f64,
    /// 率の分母そのもの。足切りにもこれを使う。
    pub denominator: f64,
    pub apo_rate: Option<f64>,
    pub na_due: f64,
    pub na_done_ontime: f64,
    pub na_rate: Option<f64>,
    pub duration_ms_total: f64,
    /// 平均通話秒数。分母(架電数)0なら null。
    pub avg_talk_secs: Option<f64>,
    /// 分母が足切り未満か。画面で「※少サンプル」を出すための旗。
    /// **行を消さずに旗を立てる**（事実は見せる、順位だけ鵜呑みにさせない）
    pub thin: bool,
}

#[derive(Debug, Serialize)]
pub struct MembersData {
    /// 全メンバー（足切り前）。合計と内訳を一致させるため、ここから行を落とさない。
    pub members: Vec<MemberRow>,
    /// アポ率ランキング（足切り後・降順）
    pub apo_rank: Vec<MemberRow>,
    /// 架電数ランキング（足切りなし・降順。量の比較なので分母の縛りは不要）
    pub call_rank: Vec<MemberRow>,
    pub apo_denominator_label: String,
    /// 足切り閾値。画面に出して「なぜこの人が居ないのか」を説明できるようにする。
    pub min_denominator: f64,
}

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

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

/// 月次明細から owner 単位に畳む。
///
/// 使うシート: 「月次明細」
/// 使う列: owner_id / year_month / call_count / zoom_dial_count / apo_count /
///         na_due / na_done_ontime / duration_ms_total
pub fn collect(
    data: &SheetData,
    q: &MembersQuery,
    sales_owners: Option<&Vec<String>>,
) -> (Vec<MemberRow>, usize) {
    let pref_mode = q
        .prefecture
        .as_deref()
        .map(|p| !p.is_empty())
        .unwrap_or(false);
    let owner_filter: Option<Vec<String>> = q.owners.as_ref().map(|s| {
        s.split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect()
    });

    let mut acc: HashMap<String, [f64; 6]> = HashMap::new();
    let mut matched = 0usize;

    for row in &data.rows {
        let owner = data.get(row, "owner_id").to_string();
        if owner.is_empty() {
            continue;
        }

        // スコープ: 指定があればその人、無ければ role=sales のみ
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

        if let Some(ym) = q.year_month.as_deref() {
            if data.get(row, "year_month") != ym {
                continue;
            }
        }

        let e = acc.entry(owner).or_insert([0.0; 6]);
        e[0] += num(data.get(row, "call_count"));
        e[1] += num(data.get(row, "zoom_dial_count"));
        e[2] += num(data.get(row, "apo_count"));
        e[3] += num(data.get(row, "na_due"));
        e[4] += num(data.get(row, "na_done_ontime"));
        e[5] += num(data.get(row, "duration_ms_total"));
        matched += 1;
    }

    let mut members: Vec<MemberRow> = acc
        .into_iter()
        .map(|(id, v)| {
            let den = apo_denominator(v[0], v[1], pref_mode);
            MemberRow {
                owner_id: id,
                call_count: v[0],
                zoom_dial_count: v[1],
                apo_count: v[2],
                denominator: den,
                apo_rate: rate(v[2], den),
                na_due: v[3],
                na_done_ontime: v[4],
                na_rate: rate(v[4], v[3]),
                duration_ms_total: v[5],
                // 平均通話秒数は「架電1件あたり」。架電0なら null（0秒ではない）
                avg_talk_secs: if v[0] > 0.0 {
                    Some(v[5] / 1000.0 / v[0])
                } else {
                    None
                },
                thin: den < MIN_DEN_FOR_RATE,
            }
        })
        .collect();

    // owner_id で安定化（HashMap の反復順を返さない）
    members.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    (members, matched)
}

/// 率降順ランキング。**足切りは率と同じ分母**で行う。
fn rank_by_rate(members: &[MemberRow]) -> Vec<MemberRow> {
    let mut v: Vec<MemberRow> = members
        .iter()
        .filter(|m| m.denominator >= MIN_DEN_FOR_RATE)
        .cloned()
        .collect();
    v.sort_by(|a, b| {
        b.apo_rate
            .unwrap_or(0.0)
            .partial_cmp(&a.apo_rate.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    v
}

/// 架電数降順。量の比較なので足切りしない。
fn rank_by_calls(members: &[MemberRow]) -> Vec<MemberRow> {
    let mut v: Vec<MemberRow> = members.to_vec();
    v.sort_by(|a, b| {
        b.call_count
            .partial_cmp(&a.call_count)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    v
}

pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: MembersQuery,
    sales_owners: Option<Vec<String>>,
) -> Result<TabPayload<MembersData>> {
    let started = Instant::now();
    let (data, from_cache) = store.get(client, "月次明細").await?;
    let (members, matched) = collect(&data, &q, sales_owners.as_ref());

    let pref_mode = q
        .prefecture
        .as_deref()
        .map(|p| !p.is_empty())
        .unwrap_or(false);

    Ok(TabPayload {
        data: MembersData {
            apo_rank: rank_by_rate(&members),
            call_rank: rank_by_calls(&members),
            members,
            apo_denominator_label: denominator_label(pref_mode),
            min_denominator: MIN_DEN_FOR_RATE,
        },
        sources: vec![SourceInfo {
            sheet: "月次明細".to_string(),
            total_rows: data.rows.len(),
            matched_rows: matched,
            from_cache,
            age_secs: data.fetched_at.elapsed().as_secs(),
        }],
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: Vec<(&str, f64, f64, f64, f64)>) -> SheetData {
        let header = vec![
            "owner_id".to_string(),
            "year_month".to_string(),
            "call_count".to_string(),
            "zoom_dial_count".to_string(),
            "apo_count".to_string(),
            "na_due".to_string(),
            "na_done_ontime".to_string(),
            "duration_ms_total".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(o, c, z, a, dur)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(o),
                    Arc::from("2026-05"),
                    Arc::from(c.to_string().as_str()),
                    Arc::from(z.to_string().as_str()),
                    Arc::from(a.to_string().as_str()),
                    Arc::from("0"),
                    Arc::from("0"),
                    Arc::from(dur.to_string().as_str()),
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
    fn 足切りは率と同じ分母で行う() {
        // GAS 版は call>=100 で足切りしていたため、Zoom発信が少ない担当者が
        // 不安定な率のままランキング上位に出ていた
        let d = sheet(vec![
            ("thin", 500.0, 20.0, 3.0, 0.0),   // Call多いがZoom発信20
            ("normal", 400.0, 500.0, 5.0, 0.0),
        ]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        let r = rank_by_rate(&m);
        assert_eq!(r.len(), 1, "分母20の担当者は足切りで除外される");
        assert_eq!(r[0].owner_id, "normal");
    }

    #[test]
    fn 足切りされた人もmembersには残る() {
        // 合計と内訳を一致させるため、行そのものは落とさない。
        // GAS 版は o.call > 0 で落としており、KPI合計には入るのに
        // 担当者別チャートから消える不整合があった。
        let d = sheet(vec![("thin", 0.0, 20.0, 3.0, 0.0)]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        assert_eq!(m.len(), 1);
        assert!(m[0].thin, "少サンプルは旗を立てる（消さない）");
    }

    #[test]
    fn 分母0のアポ率はnull() {
        let d = sheet(vec![("x", 0.0, 0.0, 3.0, 0.0)]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        assert!(m[0].apo_rate.is_none());
    }

    #[test]
    fn 架電0の平均通話秒数はnull() {
        // 0秒ではない。「通話していない」と「平均0秒」は違う
        let d = sheet(vec![("x", 0.0, 10.0, 0.0, 0.0)]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        assert!(m[0].avg_talk_secs.is_none());
    }

    #[test]
    fn 平均通話秒数が計算される() {
        // 100件で 300,000ms → 1件あたり 3秒
        let d = sheet(vec![("x", 100.0, 100.0, 0.0, 300_000.0)]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        assert_eq!(m[0].avg_talk_secs, Some(3.0));
    }

    #[test]
    fn 営業以外は既定で除外される() {
        let d = sheet(vec![("sales1", 100.0, 200.0, 2.0, 0.0), ("bpo1", 900.0, 900.0, 1.0, 0.0)]);
        let sales = vec!["sales1".to_string()];
        let (m, matched) = collect(&d, &MembersQuery::default(), Some(&sales));
        assert_eq!(matched, 1);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].owner_id, "sales1");
    }

    #[test]
    fn 並びが安定する() {
        let d = sheet(vec![
            ("b", 100.0, 200.0, 2.0, 0.0),
            ("a", 100.0, 200.0, 2.0, 0.0),
        ]);
        let a = collect(&d, &MembersQuery::default(), None).0;
        let b = collect(&d, &MembersQuery::default(), None).0;
        let ids: Vec<&str> = a.iter().map(|x| x.owner_id.as_str()).collect();
        assert_eq!(ids, vec!["a", "b"]);
        assert_eq!(
            ids,
            b.iter().map(|x| x.owner_id.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn 同率でも順位が安定する() {
        let d = sheet(vec![
            ("b", 1000.0, 1000.0, 10.0, 0.0),
            ("a", 1000.0, 1000.0, 10.0, 0.0),
        ]);
        let (m, _) = collect(&d, &MembersQuery::default(), None);
        let r = rank_by_rate(&m);
        assert_eq!(r[0].owner_id, "a", "同率は owner_id で安定させる");
    }
}
