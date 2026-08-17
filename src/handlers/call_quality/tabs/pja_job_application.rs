//! 架電クオリティ P-pja: 「求人・応募」タブ(応募効率 / 顧客(Deal)健全性 / データ品質)
//!
//! 2026-08-16 移植。GAS 版の実データ集計(HubSpot 求人 LISTING × 応募 APPOINTMENT の
//! 突合)は Python `scripts/call_quality_monitor/job_application_dashboard.py` が
//! 日次で行い、結果を以下4シートへ書き込む(まっさら更新)。**このタブは4シートを
//! 読んで整形するだけ**で、LISTING/APPOINTMENT の再突合はしない
//! (Python 側が既に集計済みの値をそのまま信頼する)。
//!
//!   求人応募_KPI        指標/値              の key-value(10行)
//!   求人応募_媒体月次    媒体/年月(応募日基準)/応募数  の long 形式
//!   求人応募_Deal健全性  Deal/公開中求人数/直近30日応募数/最終応募日/最終応募からの経過日数/状態
//!   求人応募_データ品質  指標/値/母数/率/補足
//!
//! 参照元:
//!   画面   scripts/gas/call_quality_app/index.html   id="page-pja" (2025-2060行)
//!   取得   scripts/gas/call_quality_app/Code.gs       getJobAppKpi/Media/Health/Quality (1076-1079行)
//!   描画   scripts/gas/call_quality_app/javascript.html  _jaRenderKpi/_jaRenderMedia/_jaRenderHealth/_jaRenderQuality (2925-3080行付近)
//!   集計   scripts/call_quality_monitor/job_application_dashboard.py  build() / _tables()
//!
//! GAS 版との既知の重要な意味(落とすと数字の意味が変わる。依頼メッセージより):
//!   - 時系列は必ず**応募日(yingmuri)基準**(取込日 hs_createdate ではない)。
//!   - 「求人あたり応募」の**分子は求人紐付きの応募のみ**(AirWork未紐付けで水増ししない)。
//!     一方で**分母は「公開中求人数」と「応募実績のある求人数」の両方を併記する**仕様。
//!     片方だけにすると意味が変わるため、本ファイルでも両方を `PjaKpi` に残す。
//!   - Deal健全性は1求人が複数Dealに紐付く場合に**各Dealへ重複計上**されるため、
//!     `公開中求人数` 列を縦に合計しても全体の公開中求人数とは一致しない(意図的)。
//!
//! 本ファイルでの意図的な変更点(約束2「分母0はNone」に合わせるための調整):
//!   Python 側 `eff_per_public`/`eff_per_applied` はシートに 0 (denominator=0の場合)を
//!   書くが、ここでは KPI シートの生の件数(公開中求人数・応募実績のある求人数)から
//!   **自前で割り直し**、分母0を None にする。データ品質シートの「率」列も同様に、
//!   Python が書いた文字列("49.9%"等)をそのまま信用せず、値/母数から計算し直す。
//!   ただし「率」列が元々空欄の行(Python が意図的に%表示しないと決めた行)は
//!   None のまま据え置く(母数があっても率を作らない)。
//!
//! 未実装: なし(①応募効率 ②顧客(Deal)健全性 ③データ品質 の3パネルとも実装)。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

use super::{rate, SourceInfo, TabPayload};

const KPI_SHEET: &str = "求人応募_KPI";
const MEDIA_SHEET: &str = "求人応募_媒体月次";
const HEALTH_SHEET: &str = "求人応募_Deal健全性";
const QUALITY_SHEET: &str = "求人応募_データ品質";

/// フロントから渡す絞り込み。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct PjaQuery {
    /// true なら「応募途絶注意」(公開中求人ありで直近30日応募ゼロ)のDealのみ返す。
    /// GAS 版はチェックボックスでブラウザ側が再描画していたが(javascript.html
    /// `pja-only-silent`)、ここではサーバ側で絞ってから返す(約束6)。
    #[serde(default)]
    pub only_silent: bool,
}

crate::accepted_params!(PjaQuery, pja_query_accepted => "only_silent");

#[derive(Debug, Serialize)]
pub struct PjaKpi {
    /// Python 集計時刻(「シートを読んだ時刻」ではない。GAS 版が両者を混同していた
    /// 反省があるため、この値は必ず「求人応募_KPI」シートの生成時刻をそのまま出す)。
    pub generated_at: String,
    pub listing_total: u64,
    pub n_pub: u64,
    pub total_appts: u64,
    pub linked_appts: u64,
    pub n_listings_with_apps: u64,
    pub active_deals: u64,
    /// 公開中求人あたり応募(露出効率)。分子=紐付き応募・分母=公開中求人数。
    /// 分母0なら None(約束2)。
    pub eff_per_public: Option<f64>,
    /// 応募実績求人あたり応募(反応の濃さ)。分子=紐付き応募・分母=応募実績のある求人数。
    pub eff_per_applied: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct PjaMediaRow {
    pub media: String,
    /// (年月 "YYYY-MM", 応募数) を months と同じ並びで持つ。
    pub monthly: Vec<(String, u64)>,
    pub total: u64,
}

#[derive(Debug, Serialize)]
pub struct PjaMediaTable {
    /// 全媒体を通じて登場した年月の一覧(昇順)。表の列見出しに使う。
    pub months: Vec<String>,
    /// 応募総数の多い順(GAS版 `_jaRenderMedia` の並びに合わせる)。
    pub rows: Vec<PjaMediaRow>,
}

#[derive(Debug, Serialize)]
pub struct PjaHealthRow {
    pub deal: String,
    pub pub_listing_count: u64,
    pub recent_apps_30d: u64,
    pub last_app_date: Option<String>,
    pub days_since_last_app: Option<i64>,
    pub status: String,
    /// status == "応募途絶注意" のショートカット(フロントの色分け用)。
    pub is_silent: bool,
}

#[derive(Debug, Serialize)]
pub struct PjaQualityRow {
    pub metric: String,
    pub value: f64,
    pub denominator: Option<f64>,
    /// パーセント(0-100)。分母0、または元シートがこの行を%表示しないと決めていた
    /// 場合は None(約束2)。
    pub rate_pct: Option<f64>,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct PjaData {
    pub kpi: PjaKpi,
    pub media: PjaMediaTable,
    /// only_silent フィルタ適用後の一覧。
    pub health: Vec<PjaHealthRow>,
    /// フィルタ前の「応募途絶注意」件数(フィルタ有無に関わらず一定)。
    pub health_silent_count: usize,
    pub quality: Vec<PjaQualityRow>,
}

fn num_f64(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn num_u64(s: &str) -> u64 {
    let v = num_f64(s);
    if v > 0.0 {
        v.round() as u64
    } else {
        0
    }
}

/// 「指標」列をキー、「値」列を値にした map を作る(KPI/データ品質シート共通の形)。
fn kv_map<'a>(data: &'a SheetData, key_col: &str, val_col: &str) -> HashMap<&'a str, &'a str> {
    let mut map = HashMap::new();
    if let (Some(ki), Some(vi)) = (data.col(key_col), data.col(val_col)) {
        for row in &data.rows {
            let k = row.get(ki).map(|s| s.as_ref()).unwrap_or("");
            if k.is_empty() {
                continue;
            }
            let v = row.get(vi).map(|s| s.as_ref()).unwrap_or("");
            map.insert(k, v);
        }
    }
    map
}

/// 「求人応募_KPI」シート(指標/値)を読む。
/// 参照: job_application_dashboard.py `_tables()` の kpi 配列(359-370行)。
fn parse_kpi(data: &SheetData) -> PjaKpi {
    let m = kv_map(data, "指標", "値");
    let get = |k: &str| m.get(k).copied().unwrap_or("");

    let n_pub = num_u64(get("公開中求人数"));
    let linked_appts = num_u64(get("うち求人紐付き応募"));
    let n_listings_with_apps = num_u64(get("応募実績のある求人数"));

    PjaKpi {
        generated_at: get("生成時刻").to_string(),
        listing_total: num_u64(get("求人LISTING総数")),
        n_pub,
        total_appts: num_u64(get("応募APPOINTMENT総数")),
        linked_appts,
        n_listings_with_apps,
        active_deals: num_u64(get("納品管理アクティブDeal数")),
        eff_per_public: if n_pub > 0 {
            Some(linked_appts as f64 / n_pub as f64)
        } else {
            None
        },
        eff_per_applied: if n_listings_with_apps > 0 {
            Some(linked_appts as f64 / n_listings_with_apps as f64)
        } else {
            None
        },
    }
}

/// 「求人応募_媒体月次」シート(媒体/年月(応募日基準)/応募数)を
/// 媒体×年月のピボットに直す。GAS版 `_jaRenderMedia` (javascript.html) のロジック移植。
fn parse_media(data: &SheetData) -> PjaMediaTable {
    let ci_media = data.col("媒体");
    let ci_ym = data.col("年月(応募日基準)");
    let ci_cnt = data.col("応募数");

    let mut months: Vec<String> = Vec::new();
    let mut per_media: HashMap<String, HashMap<String, u64>> = HashMap::new();
    let mut totals: HashMap<String, u64> = HashMap::new();
    let mut media_order: Vec<String> = Vec::new(); // 初出順(並び替え前の安定キー)

    if let (Some(mi), Some(yi), Some(ci)) = (ci_media, ci_ym, ci_cnt) {
        for row in &data.rows {
            let media = row.get(mi).map(|s| s.as_ref()).unwrap_or("");
            let ym_raw = row.get(yi).map(|s| s.as_ref()).unwrap_or("");
            let cnt = num_u64(row.get(ci).map(|s| s.as_ref()).unwrap_or(""));
            if media.is_empty() || ym_raw.len() < 7 {
                continue;
            }
            let ym = ym_raw[..7].to_string();
            if !months.contains(&ym) {
                months.push(ym.clone());
            }
            if !per_media.contains_key(media) {
                media_order.push(media.to_string());
            }
            *per_media
                .entry(media.to_string())
                .or_default()
                .entry(ym)
                .or_insert(0) += cnt;
            *totals.entry(media.to_string()).or_insert(0) += cnt;
        }
    }
    months.sort();

    // 応募総数の多い順。同数はメディア名で安定させる(約束4)。
    let mut medias = media_order;
    medias.sort_by(|a, b| {
        totals
            .get(b)
            .unwrap_or(&0)
            .cmp(totals.get(a).unwrap_or(&0))
            .then_with(|| a.cmp(b))
    });

    let rows = medias
        .into_iter()
        .map(|media| {
            let mm = per_media.get(&media).cloned().unwrap_or_default();
            let monthly = months
                .iter()
                .map(|m| (m.clone(), *mm.get(m).unwrap_or(&0)))
                .collect();
            let total = *totals.get(&media).unwrap_or(&0);
            PjaMediaRow {
                media,
                monthly,
                total,
            }
        })
        .collect();

    PjaMediaTable { months, rows }
}

/// 「求人応募_Deal健全性」シートを読む(フィルタ前の全件)。
/// 順序はシートの行順(python `_tables()` が -直近30日応募数, -公開中求人数 で
/// 事前ソート済)をそのまま保持する(約束4: 再ソートせず安定させる)。
fn parse_health(data: &SheetData) -> Vec<PjaHealthRow> {
    let ci_deal = data.col("Deal");
    let ci_pub = data.col("公開中求人数");
    let ci_recent = data.col("直近30日応募数");
    let ci_last = data.col("最終応募日");
    let ci_gap = data.col("最終応募からの経過日数");
    let ci_status = data.col("状態");

    let mut out = Vec::new();
    for row in &data.rows {
        let g = |ci: Option<usize>| -> &str {
            ci.and_then(|i| row.get(i)).map(|s| s.as_ref()).unwrap_or("")
        };
        let deal = g(ci_deal);
        if deal.is_empty() {
            continue;
        }
        let status = g(ci_status).to_string();
        let is_silent = status == "応募途絶注意";
        let last_raw = g(ci_last);
        let gap_raw = g(ci_gap);
        out.push(PjaHealthRow {
            deal: deal.to_string(),
            pub_listing_count: num_u64(g(ci_pub)),
            recent_apps_30d: num_u64(g(ci_recent)),
            last_app_date: if last_raw.is_empty() {
                None
            } else {
                Some(last_raw.to_string())
            },
            days_since_last_app: gap_raw.trim().parse::<i64>().ok(),
            status,
            is_silent,
        });
    }
    out
}

/// 「求人応募_データ品質」シート(指標/値/母数/率/補足)を読む。
fn parse_quality(data: &SheetData) -> Vec<PjaQualityRow> {
    let ci_metric = data.col("指標");
    let ci_value = data.col("値");
    let ci_denom = data.col("母数");
    let ci_rate = data.col("率");
    let ci_note = data.col("補足");

    let mut out = Vec::new();
    for row in &data.rows {
        let g = |ci: Option<usize>| -> &str {
            ci.and_then(|i| row.get(i)).map(|s| s.as_ref()).unwrap_or("")
        };
        let metric = g(ci_metric);
        if metric.is_empty() {
            continue;
        }
        let value = num_f64(g(ci_value));
        let denom_raw = g(ci_denom);
        let denominator = if denom_raw.trim().is_empty() {
            None
        } else {
            Some(num_f64(denom_raw))
        };
        // 「率」列が元々空欄かどうかで、この行を%表示すべき行かを判定する。
        // 母数があっても Python が意図的に率を空欄にしている行がある
        // (例:「求人紐付きだがDeal未到達の応募」。件数の文脈は出すが割合は出さない設計)。
        // その意図は保ちつつ、実際の数値は文字列("49.9%")を信用せず rate() で計算し直す。
        let rate_designated = !g(ci_rate).trim().is_empty();
        let rate_pct = if rate_designated {
            denominator.and_then(|d| rate(value, d))
        } else {
            None
        };
        out.push(PjaQualityRow {
            metric: metric.to_string(),
            value,
            denominator,
            rate_pct,
            note: g(ci_note).to_string(),
        });
    }
    out
}

/// ハンドラ本体。4シートを読み、整形して返す。
pub async fn handle(
    store: &SheetStore,
    client: &SheetsClient,
    q: PjaQuery,
) -> Result<TabPayload<PjaData>> {
    let started = Instant::now();

    let (kpi_sheet, kpi_cached) = store.get(client, KPI_SHEET).await?;
    let (media_sheet, media_cached) = store.get(client, MEDIA_SHEET).await?;
    let (health_sheet, health_cached) = store.get(client, HEALTH_SHEET).await?;
    let (quality_sheet, quality_cached) = store.get(client, QUALITY_SHEET).await?;

    let kpi = parse_kpi(&kpi_sheet);
    let media = parse_media(&media_sheet);
    let health_all = parse_health(&health_sheet);
    let health_silent_count = health_all.iter().filter(|r| r.is_silent).count();
    let health: Vec<PjaHealthRow> = if q.only_silent {
        health_all.into_iter().filter(|r| r.is_silent).collect()
    } else {
        health_all
    };
    let quality = parse_quality(&quality_sheet);

    let sources = vec![
        SourceInfo {
            sheet: KPI_SHEET.to_string(),
            total_rows: kpi_sheet.rows.len(),
            matched_rows: kpi_sheet.rows.len(),
            from_cache: kpi_cached,
            age_secs: kpi_sheet.fetched_at.elapsed().as_secs(),
        },
        SourceInfo {
            sheet: MEDIA_SHEET.to_string(),
            total_rows: media_sheet.rows.len(),
            matched_rows: media_sheet.rows.len(),
            from_cache: media_cached,
            age_secs: media_sheet.fetched_at.elapsed().as_secs(),
        },
        SourceInfo {
            sheet: HEALTH_SHEET.to_string(),
            total_rows: health_sheet.rows.len(),
            matched_rows: health.len(),
            from_cache: health_cached,
            age_secs: health_sheet.fetched_at.elapsed().as_secs(),
        },
        SourceInfo {
            sheet: QUALITY_SHEET.to_string(),
            total_rows: quality_sheet.rows.len(),
            matched_rows: quality_sheet.rows.len(),
            from_cache: quality_cached,
            age_secs: quality_sheet.fetched_at.elapsed().as_secs(),
        },
    ];

    Ok(TabPayload {
        data: PjaData {
            kpi,
            media,
            health,
            health_silent_count,
            quality,
        },
        sources,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(header: &[&str], rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(std::sync::Arc::from).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn kpi分母0はnoneになる() {
        // Python 側は 0 を書くが、ここでは公開中求人数0を渡して None を確認する。
        let s = sheet(
            &["指標", "値"],
            vec![
                vec!["生成時刻", "2026-08-16 07:00 JST"],
                vec!["求人LISTING総数", "500"],
                vec!["公開中求人数", "0"],
                vec!["応募APPOINTMENT総数", "1000"],
                vec!["うち求人紐付き応募", "700"],
                vec!["応募実績のある求人数", "300"],
                vec!["納品管理アクティブDeal数", "200"],
            ],
        );
        let kpi = parse_kpi(&s);
        assert_eq!(kpi.eff_per_public, None, "分母(公開中求人数)0は0.0でなくNone");
        assert_eq!(kpi.eff_per_applied, Some(700.0 / 300.0));
    }

    #[test]
    fn kpi通常値の読み取り() {
        let s = sheet(
            &["指標", "値"],
            vec![
                vec!["生成時刻", "2026-08-16 07:00 JST"],
                vec!["求人LISTING総数", "500"],
                vec!["公開中求人数", "200"],
                vec!["応募APPOINTMENT総数", "1,000"],
                vec!["うち求人紐付き応募", "700"],
                vec!["応募実績のある求人数", "300"],
                vec!["納品管理アクティブDeal数", "199"],
            ],
        );
        let kpi = parse_kpi(&s);
        assert_eq!(kpi.generated_at, "2026-08-16 07:00 JST");
        assert_eq!(kpi.total_appts, 1000, "カンマ区切りも数値として読める");
        assert_eq!(kpi.eff_per_public, Some(3.5));
        assert_eq!(kpi.active_deals, 199);
    }

    #[test]
    fn 媒体ピボットは総数降順で並ぶ() {
        let s = sheet(
            &["媒体", "年月(応募日基準)", "応募数"],
            vec![
                vec!["AirWork", "2026-06", "10"],
                vec!["AirWork", "2026-07", "20"],
                vec!["indeed", "2026-07", "50"],
                vec!["indeed", "2026-06", "5"],
            ],
        );
        let m = parse_media(&s);
        assert_eq!(m.months, vec!["2026-06", "2026-07"]);
        assert_eq!(m.rows[0].media, "indeed", "合計55が最多なので先頭");
        assert_eq!(m.rows[0].total, 55);
        assert_eq!(m.rows[1].media, "AirWork");
        assert_eq!(m.rows[1].total, 30);
        // 年月ごとの内訳も months と同じ並びで持つ
        assert_eq!(m.rows[0].monthly, vec![("2026-06".into(), 5), ("2026-07".into(), 50)]);
    }

    #[test]
    fn 応募途絶注意でフィルタできる() {
        let s = sheet(
            &["Deal", "公開中求人数", "直近30日応募数", "最終応募日", "最終応募からの経過日数", "状態"],
            vec![
                vec!["A社", "3", "5", "2026-08-10", "6", "応募あり"],
                vec!["B社", "2", "0", "2026-06-01", "76", "応募途絶注意"],
                vec!["C社", "0", "0", "", "", "掲載ゼロ・応募ゼロ"],
            ],
        );
        let all = parse_health(&s);
        assert_eq!(all.len(), 3);
        let silent_count = all.iter().filter(|r| r.is_silent).count();
        assert_eq!(silent_count, 1);
        let b = all.iter().find(|r| r.deal == "B社").unwrap();
        assert!(b.is_silent);
        assert_eq!(b.days_since_last_app, Some(76));
        let c = all.iter().find(|r| r.deal == "C社").unwrap();
        assert_eq!(c.last_app_date, None, "空文字はNoneに正規化する");
        assert_eq!(c.days_since_last_app, None);
    }

    #[test]
    fn データ品質は率列が空欄なら母数があってもnone() {
        let s = sheet(
            &["指標", "値", "母数", "率", "補足"],
            vec![
                vec!["未紐付け応募(求人に紐付かない)", "7", "1000", "0.7%", "media内訳"],
                // 母数はあるが率は空欄(Python側の意図的な設計)。ここもNoneのまま。
                vec!["求人紐付きだがDeal未到達の応募", "12", "1000", "", "LISTING→Deal未設定"],
                // 母数も率も無い行
                vec!["応募総数(取込進捗の目安)", "1000", "", "", "yingmuri応募日ベース"],
            ],
        );
        let rows = parse_quality(&s);
        // f64 の比較なので厳密な等価ではなく誤差許容で見る(7.0/1000.0*100.0 は
        // 浮動小数点演算の丸めにより 0.7 と厳密一致しないことがある)。
        let got = rows[0].rate_pct.expect("率が計算されているはず");
        assert!((got - 0.7).abs() < 1e-9, "0.7% 前後のはずが {got}");
        assert_eq!(rows[1].rate_pct, None, "率列が空欄の行は母数があってもNone");
        assert_eq!(rows[1].denominator, Some(1000.0), "母数自体は保持する");
        assert_eq!(rows[2].rate_pct, None);
        assert_eq!(rows[2].denominator, None);
    }

    #[test]
    fn データ品質は分母0でも率列が非空欄ならnoneになる() {
        // 実運用では python が n_pub==0 のとき率列を空にするが、
        // ここでは「率列が非空欄なのに母数0」という不整合値が来ても
        // rate() 経由でNoneになることを保証する(約束2の防御)。
        let s = sheet(
            &["指標", "値", "母数", "率", "補足"],
            vec![vec!["公開中求人でDeal未紐付け", "0", "0", "0.0%", "注記"]],
        );
        let rows = parse_quality(&s);
        assert_eq!(rows[0].rate_pct, None, "分母0はrate()がNoneを返す");
    }
}
