//! P13: 案件タイムライン (Zoom AI Companion 議事録)
//!
//! GAS 版の移植元:
//!   - 画面: `scripts/gas/call_quality_app/index.html` `#page-p13` (~1687-1776行)
//!   - 描画: `scripts/gas/call_quality_app/javascript.html` `renderP13MtgTimeline` 以下
//!            (~14894-15757行)
//!   - 取得: `scripts/gas/call_quality_app/Code.gs`
//!            `getConsultingMtgTimeline` (1916行) 他4関数
//!
//! 読むシート (5枚。全て `sheets::KNOWN_SHEETS` に登録済み):
//!   - コンサルMTGタイムライン   (主。1 MTG = 1 行。Deal セレクタ + タイムライン本体)
//!   - コンサル接触率_週次       (Deal カルテのEmail/Call推移。javascript.html 14957行)
//!   - コンサル健全性_月次       (定期NPS推移。nps_series_json を展開。14968行)
//!   - コンサル接触ロールアップ  (全関連Call/Email/MTG件数。14979行)
//!   - コンサルフェーズKPI       (契約フェーズKPIパネル + 担当者名フォールバック。14993行)
//!
//! 未実装パネル: なし。5シートとも取得可能なため GAS 版の Deal セレクタ/概要カード/
//! 接触×NPS推移/MTGタイムライン/契約フェーズKPIパネルを全て移植した。
//!
//! GAS 版と処理場所が違う点(値は同じになるよう再現、理由をここに明記):
//!   - **信頼度フィルタ**: source = "host_email_match" は「consultant 全 Deal に展開し
//!     誤紐付け多発」(javascript.html 15129行)のため GAS 版と同様に Deal 一覧・
//!     タイムライン双方から除外する。除外件数は `DealIndexData.skipped_low_confidence` /
//!     `DealDetail.meetings_total`(除外後)で可視化する。
//!   - **接触×NPS推移グラフ**: GAS 版は「期間絞込(3M/6M/12M)」の対象外で、常に Deal の
//!     全MTG履歴から週次MTG実施回数を算出する(javascript.html 15043行 `P13_CACHE.rows`
//!     参照 = 期間フィルタ前の全件)。本実装も `meetings_all`(期間フィルタ前)から
//!     週次集計する。タイムラインのカード一覧だけ期間/並び順の絞り込みを適用する。
//!   - **担当者名の解決**: MTGタイムライン側 `consultant_owner_name` が空 or
//!     数字5桁以上(ID)の場合、フェーズKPI側 `consultant_name` を fallback に使う
//!     (javascript.html 15264-15274行 `_p13ConsultantName`)。どちらも無ければ「（未設定）」。

use std::collections::{BTreeSet, HashMap};
use std::sync::Arc;

use anyhow::Result;
use chrono::{DateTime, Datelike, Duration as ChronoDuration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};
use crate::handlers::call_quality::query_audit::ValueAudit;

use super::{SourceInfo, TabPayload};

// ---------------------------------------------------------------- 共通パース

fn pf_opt(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok()
    }
}

fn pu32_opt(s: &str) -> Option<u32> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok().map(|v| v as u32)
    }
}

fn pu32(s: &str) -> u32 {
    pu32_opt(s).unwrap_or(0)
}

/// javascript.html `_p13ConsultantName`(15268行) の isId ガード。
/// 「数字5桁以上」は名前ではなく owner_id が誤って入っている行とみなす。
fn is_id_like(s: &str) -> bool {
    let t = s.trim();
    t.len() >= 5 && !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

/// javascript.html `_p13ConsultantName`(15267-15274行) を移植。
fn resolve_consultant_name(timeline_name: &str, phase_name: Option<&str>) -> String {
    let nm = timeline_name.trim();
    if !nm.is_empty() && !is_id_like(nm) {
        return nm.to_string();
    }
    if let Some(pk) = phase_name {
        let pk = pk.trim();
        if !pk.is_empty() && !is_id_like(pk) {
            return pk.to_string();
        }
    }
    "（未設定）".to_string()
}

/// javascript.html `_p13WeekStartFromIso`(15101-15119行) を移植。
/// ISO8601(UTC) → JST(+9h)換算後の月曜起点週開始日。
/// contact_weekly.week_start と同じ粒度(JST 月曜起点)に揃えるための変換
/// (UTC のまま丸めると午後MTGがJSTで翌日になり週がズレる、というコメントが原文にある)。
fn week_start_jst(iso: &str) -> Option<String> {
    let dt = DateTime::parse_from_rfc3339(iso.trim()).ok()?;
    let jst = dt.with_timezone(&Utc) + ChronoDuration::hours(9);
    let date = jst.date_naive();
    let offset_from_monday = date.weekday().num_days_from_monday(); // 月=0..日=6
    let monday = date - ChronoDuration::days(offset_from_monday as i64);
    Some(monday.format("%Y-%m-%d").to_string())
}

const CIRCLED: &str = "①②③④⑤⑥⑦⑧⑨⑩";

/// javascript.html `_p13IndexTrends`(15059-15070行) の定期ラベル (定期①..⑩、以降は数値)。
fn period_label(i: usize) -> String {
    match CIRCLED.chars().nth(i) {
        Some(c) => format!("定期{c}"),
        None => format!("定期{}", i + 1),
    }
}

/// JSON配列文字列 → Option<f64> の配列。パース不能な要素は None のまま保持する
/// (欠番があってもラベルの通し番号がズレないよう、フィルタで詰めずに保持するのが目的)。
fn parse_num_arr_opt(raw: &str) -> Vec<Option<f64>> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    v.as_array().map(|a| a.iter().map(|x| x.as_f64()).collect()).unwrap_or_default()
}

/// summary_details_json = `[{"label":..., "summary":...}, ...]`
fn parse_summary_details(raw: &str) -> Vec<SummaryDetail> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let label = obj.get("label").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            let summary = obj.get("summary").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
            if label.is_empty() && summary.is_empty() {
                None
            } else {
                Some(SummaryDetail { label, summary })
            }
        })
        .collect()
}

/// next_steps_json = 文字列配列、または `{"action"|"text"|"summary": ...}` の配列。
/// javascript.html `_p13RenderMtgCard`(15704-15706行) と同じ優先順で文字列を取り出す。
fn parse_next_steps(raw: &str) -> Vec<String> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let s = if let Some(s) = item.as_str() {
                s.to_string()
            } else if let Some(obj) = item.as_object() {
                obj.get("action")
                    .or_else(|| obj.get("text"))
                    .or_else(|| obj.get("summary"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                String::new()
            };
            let s = s.trim().to_string();
            if s.is_empty() { None } else { Some(s) }
        })
        .collect()
}

/// javascript.html `_p13RenderMtgCard`(15650-15661行) の信頼度分類。
/// host_email_match は呼び出し側で既に除外済みの想定(このシートには来ない)。
fn confidence_of(source: &str) -> &'static str {
    match source {
        "zoom_email_match" | "zoom_summary" | "zoom_direct_match" => "high",
        "zoom_topic_match" | "hubspot_only" => "mid",
        _ => "low",
    }
}

// ============================================================== Deal 一覧 (セレクタ用)

#[derive(Debug, Clone, Serialize)]
pub struct DealIndexEntry {
    pub deal_id: String,
    pub customer_label: String,
    pub consultant_name: String,
    pub pipeline_label: String,
    pub stage_label: String,
    /// 信頼度フィルタ後(host_email_match除外後)のMTG件数
    pub meeting_count: usize,
    /// 同フィルタ後の最新MTG開始時刻(ISO8601)。1件もなければ None
    pub latest_start_time: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsultantOption {
    pub name: String,
    pub deal_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct DealIndexData {
    /// 取引先名の辞書順(Rust既定の文字列比較。GAS版の Intl 'ja' collator とは
    /// 厳密には一致しない場合があるが、実データはほぼ全角文字列で実用上の差は小さい)
    pub deals: Vec<DealIndexEntry>,
    /// 担当者フィルタのプルダウン用。Deal数降順
    pub consultants: Vec<ConsultantOption>,
    /// host_email_match で除外したMTG件数(全Deal合算)
    pub skipped_low_confidence: usize,
    /// 除外後の総MTG件数
    pub total_meetings: usize,
}

struct DealAgg {
    customer_label: String,
    consultant_owner_name: String,
    pipeline_label: String,
    stage_label: String,
    meeting_count: usize,
    latest_start_time: Option<String>,
}

/// javascript.html `_p13IndexByDeal`(15121-15164行) を移植。
/// phase_lookup: deal_id → フェーズKPI側 consultant_name (担当者名フォールバック用)。
pub fn build_deal_index(timeline: &SheetData, phase_lookup: &HashMap<String, String>) -> (Vec<DealIndexEntry>, usize) {
    let mut by: HashMap<String, DealAgg> = HashMap::new();
    let mut skipped = 0usize;

    for row in &timeline.rows {
        let deal_id = timeline.get(row, "deal_id").trim().to_string();
        if deal_id.is_empty() {
            continue;
        }
        let source = timeline.get(row, "source").trim();
        if source == "host_email_match" {
            skipped += 1;
            continue;
        }
        let start_time = timeline.get(row, "start_time").trim().to_string();
        let entry = by.entry(deal_id.clone()).or_insert_with(|| DealAgg {
            customer_label: timeline.get(row, "customer_label").trim().to_string(),
            consultant_owner_name: timeline.get(row, "consultant_owner_name").trim().to_string(),
            pipeline_label: timeline.get(row, "pipeline_label").trim().to_string(),
            stage_label: timeline.get(row, "stage_label").trim().to_string(),
            meeting_count: 0,
            latest_start_time: None,
        });
        entry.meeting_count += 1;
        // ISO8601 文字列は辞書順比較 = 時系列順になる(GAS版のlocaleCompareと同じ前提)
        if entry.latest_start_time.as_deref().unwrap_or("") < start_time.as_str() {
            entry.latest_start_time = Some(start_time);
        }
        // customer_label が空(初回行がたまたま欠損)なら以後の行で補完
        if entry.customer_label.is_empty() {
            entry.customer_label = timeline.get(row, "customer_label").trim().to_string();
        }
    }

    let mut deals: Vec<DealIndexEntry> = by
        .into_iter()
        .map(|(deal_id, a)| {
            let phase_name = phase_lookup.get(&deal_id).map(|s| s.as_str());
            DealIndexEntry {
                consultant_name: resolve_consultant_name(&a.consultant_owner_name, phase_name),
                customer_label: if a.customer_label.is_empty() {
                    format!("Deal {deal_id}")
                } else {
                    a.customer_label
                },
                pipeline_label: a.pipeline_label,
                stage_label: a.stage_label,
                meeting_count: a.meeting_count,
                latest_start_time: a.latest_start_time,
                deal_id,
            }
        })
        .collect();
    deals.sort_by(|a, b| a.customer_label.cmp(&b.customer_label));

    (deals, skipped)
}

pub fn build_phase_lookup(phase: &SheetData) -> HashMap<String, String> {
    phase
        .rows
        .iter()
        .filter_map(|row| {
            let deal_id = phase.get(row, "deal_id").trim().to_string();
            if deal_id.is_empty() {
                return None;
            }
            Some((deal_id, phase.get(row, "consultant_name").trim().to_string()))
        })
        .collect()
}

pub async fn get_deal_index(client: &SheetsClient, store: &SheetStore) -> Result<TabPayload<DealIndexData>> {
    let started = std::time::Instant::now();
    let (timeline, timeline_cached) = store.get(client, "コンサルMTGタイムライン").await?;
    let (phase, phase_cached) = store.get(client, "コンサルフェーズKPI").await?;

    let phase_lookup = build_phase_lookup(&phase);
    let (deals, skipped) = build_deal_index(&timeline, &phase_lookup);

    let mut consultant_counts: HashMap<String, u32> = HashMap::new();
    for d in &deals {
        *consultant_counts.entry(d.consultant_name.clone()).or_insert(0) += 1;
    }
    let mut consultants: Vec<ConsultantOption> = consultant_counts
        .into_iter()
        .map(|(name, deal_count)| ConsultantOption { name, deal_count })
        .collect();
    consultants.sort_by(|a, b| b.deal_count.cmp(&a.deal_count).then_with(|| a.name.cmp(&b.name)));

    let total_meetings: usize = deals.iter().map(|d| d.meeting_count).sum();

    Ok(TabPayload {
        data: DealIndexData { deals, consultants, skipped_low_confidence: skipped, total_meetings },
        sources: vec![
            SourceInfo {
                sheet: "コンサルMTGタイムライン".to_string(),
                total_rows: timeline.rows.len(),
                matched_rows: timeline.rows.len() - skipped,
                from_cache: timeline_cached,
                age_secs: timeline.fetched_at.elapsed().as_secs(),
            },
            SourceInfo {
                sheet: "コンサルフェーズKPI".to_string(),
                total_rows: phase.rows.len(),
                matched_rows: phase.rows.len(),
                from_cache: phase_cached,
                age_secs: phase.fetched_at.elapsed().as_secs(),
            },
        ],
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
        // 一覧は引数を取らないので常に空。キーごと消さない（古いサーバと区別するため）。
        invalid_values: Vec::new(),
    })
}

// ============================================================== Deal 詳細 (カルテ + タイムライン)

#[derive(Debug, Clone, Serialize)]
pub struct SummaryDetail {
    pub label: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct MeetingCard {
    pub meeting_id: String,
    pub start_time: String,
    pub topic: String,
    pub duration_min: Option<u32>,
    pub host_email: String,
    pub zoom_url: String,
    pub source: String,
    /// "high" | "mid" | "low"
    pub confidence: &'static str,
    /// source=hubspot_only(AI Companion要約未生成)のとき false
    pub has_summary: bool,
    pub summary_overview: String,
    pub summary_details: Vec<SummaryDetail>,
    pub next_steps: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct RollupCell {
    pub call_all: u32,
    pub email_all: u32,
    pub mtg_all: u32,
    pub call_post: u32,
    pub email_post: u32,
    pub mtg_post: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct PhaseKpiCell {
    pub overall_flag: String,
    pub contract_type: String,
    pub contract_period: String,
    pub phase_bucket: String,
    /// 0-1 の小数(GAS 版はここに *100 して % 表示。表示側で変換する)
    pub phase_pct: Option<f64>,
    pub latest_nps: Option<f64>,
    pub nps_base_line: Option<f64>,
    pub nps_flag: String,
    pub contact_flag: String,
    pub seika_status: String,
    pub alert_msg: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WeeklyContactPoint {
    pub week_start: String,
    pub email_count: u32,
    pub call_count: u32,
    /// Zoom MTG 実施件数。contact_weekly.mtg_count(ほぼ0)ではなく、
    /// タイムラインから週次に再集計した値(javascript.html 15036行の理由に同じ)。
    pub mtg_count: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct NpsPoint {
    pub label: String,
    pub nps: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct DealDetail {
    pub deal_id: String,
    pub customer_label: String,
    pub consultant_name: String,
    pub pipeline_label: String,
    pub stage_label: String,
    /// 期間絞込・並び順を適用した後のカード一覧
    pub meetings: Vec<MeetingCard>,
    /// 表示件数(=meetings.len())
    pub meetings_shown: usize,
    /// 信頼度フィルタ後・期間絞込前の全件数
    pub meetings_total: usize,
    /// 期間絞込前・最新のMTG開始時刻
    pub latest_meeting_start: Option<String>,
    pub rollup: Option<RollupCell>,
    pub phase_kpi: Option<PhaseKpiCell>,
    /// 期間絞込の影響を受けない(GAS版と同じく常に全期間から算出、コメント参照)
    pub contact_trend: Vec<WeeklyContactPoint>,
    pub nps_trend: Vec<NpsPoint>,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct DealDetailQuery {
    pub deal_id: String,
    /// "desc"(既定、新しい順) | "asc"(古い順)
    #[serde(default)]
    pub sort: Option<String>,
    /// "all"(既定) | "3m" | "6m" | "12m"
    #[serde(default)]
    pub period: Option<String>,
}

crate::accepted_params!(DealDetailQuery, deal_detail_query_accepted =>
    "deal_id", "sort", "period");

pub fn build_meeting_card(data: &SheetData, row: &[Arc<str>]) -> MeetingCard {
    let source = data.get(row, "source").trim().to_string();
    let overview = data.get(row, "summary_overview").trim().to_string();
    let details = parse_summary_details(data.get(row, "summary_details_json"));
    let next_steps = parse_next_steps(data.get(row, "next_steps_json"));
    let has_summary = !(source == "hubspot_only" && overview.is_empty() && details.is_empty() && next_steps.is_empty());
    MeetingCard {
        meeting_id: data.get(row, "meeting_id").trim().to_string(),
        start_time: data.get(row, "start_time").trim().to_string(),
        topic: data.get(row, "topic").trim().to_string(),
        duration_min: pu32_opt(data.get(row, "duration_min")),
        host_email: data.get(row, "host_email").trim().to_string(),
        zoom_url: data.get(row, "zoom_url").trim().to_string(),
        confidence: confidence_of(&source),
        has_summary,
        summary_overview: overview,
        summary_details: details,
        next_steps,
        source,
    }
}

/// javascript.html `_p13DrawDeal` の期間絞込 (15356-15364行) を移植。
/// GAS 側は Apps Script サーバのローカル日時基準。本実装は UTC 基準の月次減算で近似する
/// (数日〜1ヶ月末の境界で GAS 版とズレる可能性はあるが「直近nヶ月」の用途では実用上問題ない)。
fn months_ago(today: NaiveDate, months: i32) -> NaiveDate {
    let total = today.year() * 12 + (today.month() as i32 - 1) - months;
    let year = total.div_euclid(12);
    let month = (total.rem_euclid(12) + 1) as u32;
    let day = today.day();
    (1..=day).rev().find_map(|d| NaiveDate::from_ymd_opt(year, month, d)).unwrap_or_else(|| {
        NaiveDate::from_ymd_opt(year, month, 1).expect("月初は常に有効な日付")
    })
}

fn period_cutoff(period: &str, now: DateTime<Utc>) -> Option<NaiveDate> {
    let months = match period {
        "3m" => 3,
        "6m" => 6,
        "12m" => 12,
        _ => return None, // "all" 相当。絞り込まない
    };
    Some(months_ago(now.date_naive(), months))
}

/// deal_id・信頼度フィルタで絞ったタイムライン行(期間フィルタ前=推移グラフ用の全件)。
fn timeline_rows_for_deal<'a>(data: &'a SheetData, deal_id: &str) -> Vec<&'a Vec<Arc<str>>> {
    data.rows
        .iter()
        .filter(|row| {
            data.get(row, "deal_id").trim() == deal_id && data.get(row, "source").trim() != "host_email_match"
        })
        .collect()
}

/// `sort` / `period` の値を解釈し、実際に使う値と**解釈できなかった値**を返す。
///
/// 2026-08-17 追加。`get_deal_detail` から切り出してあるのはテストのため
/// （あちらは Sheets を叩くので単体テストから呼べない）。
///
/// - `sort` は `if sort == "asc" {…} else {降順}` なので `?sort=ascending` は黙って降順。
/// - `period` は `period_cutoff` が `_ => None`（＝絞らない）なので
///   **`?period=6M` は「6ヶ月」のつもりで全期間**になる。
///   動機になった `deals_status=NONSENSE`（絞ったつもりで全件）と同じ形。
pub fn resolve_detail_query(q: &DealDetailQuery) -> (&'static str, &'static str, ValueAudit) {
    let mut audit = ValueAudit::new();
    let sort = audit.choice(
        "sort",
        q.sort.as_deref(),
        "desc | asc",
        |v| match v.trim() {
            "desc" => Some("desc"),
            "asc" => Some("asc"),
            _ => None,
        },
        || ("desc", "desc".to_string()),
    );
    let period = audit.choice(
        "period",
        q.period.as_deref(),
        "all | 3m | 6m | 12m",
        |v| match v.trim() {
            "all" => Some("all"),
            "3m" => Some("3m"),
            "6m" => Some("6m"),
            "12m" => Some("12m"),
            _ => None,
        },
        || ("all", "all".to_string()),
    );
    (sort, period, audit)
}

pub async fn get_deal_detail(
    client: &SheetsClient,
    store: &SheetStore,
    q: &DealDetailQuery,
) -> Result<TabPayload<DealDetail>> {
    let started = std::time::Instant::now();
    let deal_id = q.deal_id.trim();
    // 2026-08-17 追加: 解釈できない値を黙って既定へ落とさない。
    //   `sort` は `if sort == "asc" {…} else {降順}` なので `?sort=ascending` は黙って降順。
    //   `period` は `period_cutoff` が `_ => None`（＝絞らない）なので
    //   **`?period=6M` は6ヶ月のつもりで全期間**になる。動機になった
    //   `deals_status=NONSENSE`（絞ったつもりで全件）と同じ形。
    let (sort, period, audit) = resolve_detail_query(q);

    let (timeline, timeline_cached) = store.get(client, "コンサルMTGタイムライン").await?;
    let (contact_weekly, contact_cached) = store.get(client, "コンサル接触率_週次").await?;
    let (health, health_cached) = store.get(client, "コンサル健全性_月次").await?;
    let (rollup, rollup_cached) = store.get(client, "コンサル接触ロールアップ").await?;
    let (phase, phase_cached) = store.get(client, "コンサルフェーズKPI").await?;

    let meetings_all = timeline_rows_for_deal(&timeline, deal_id);
    let meetings_total = meetings_all.len();

    // 概要フィールドは最初に出現した行から採る(customer_label等はDeal内で共通の想定)。
    // consultant_owner_name だけ空なら「最新MTG」の値でフォールバック
    // (javascript.html 15157-15161行 `_p13IndexByDeal` と同じ順序)。
    let mut sorted_desc = meetings_all.clone();
    sorted_desc.sort_by(|a, b| {
        timeline.get(b, "start_time").trim().cmp(timeline.get(a, "start_time").trim())
    });
    let latest_meeting_start = sorted_desc.first().map(|r| timeline.get(r, "start_time").trim().to_string());

    let (customer_label, mut consultant_owner_name, pipeline_label, stage_label) = match meetings_all.first() {
        Some(r) => (
            timeline.get(r, "customer_label").trim().to_string(),
            timeline.get(r, "consultant_owner_name").trim().to_string(),
            timeline.get(r, "pipeline_label").trim().to_string(),
            timeline.get(r, "stage_label").trim().to_string(),
        ),
        None => (String::new(), String::new(), String::new(), String::new()),
    };
    if consultant_owner_name.is_empty() {
        if let Some(r) = sorted_desc.first() {
            consultant_owner_name = timeline.get(r, "consultant_owner_name").trim().to_string();
        }
    }

    let phase_lookup = build_phase_lookup(&phase);
    let consultant_name = resolve_consultant_name(&consultant_owner_name, phase_lookup.get(deal_id).map(|s| s.as_str()));

    // ---- 期間絞込 + 並び順 (タイムラインのカード一覧のみに適用) ----
    let cutoff = period_cutoff(period, Utc::now());
    let mut filtered: Vec<&Vec<Arc<str>>> = meetings_all
        .iter()
        .copied()
        .filter(|row| match cutoff {
            None => true,
            Some(c) => DateTime::parse_from_rfc3339(timeline.get(row, "start_time").trim())
                .map(|dt| dt.with_timezone(&Utc).date_naive() >= c)
                .unwrap_or(false), // 日付が壊れている行は「対象外」扱い(黙って含めない)
        })
        .collect();
    filtered.sort_by(|a, b| {
        let av = timeline.get(a, "start_time").trim();
        let bv = timeline.get(b, "start_time").trim();
        if sort == "asc" { av.cmp(bv) } else { bv.cmp(av) }
    });
    let meetings: Vec<MeetingCard> = filtered.iter().map(|row| build_meeting_card(&timeline, row)).collect();
    let meetings_shown = meetings.len();

    // ---- ロールアップ ----
    let rollup_cell = rollup.rows.iter().find(|row| rollup.get(row, "deal_id").trim() == deal_id).map(|row| {
        RollupCell {
            call_all: pu32(rollup.get(row, "call_all")),
            email_all: pu32(rollup.get(row, "email_all")),
            mtg_all: pu32(rollup.get(row, "mtg_all")),
            call_post: pu32(rollup.get(row, "call_post")),
            email_post: pu32(rollup.get(row, "email_post")),
            mtg_post: pu32(rollup.get(row, "mtg_post")),
        }
    });

    // ---- 契約フェーズKPI ----
    let phase_cell = phase.rows.iter().find(|row| phase.get(row, "deal_id").trim() == deal_id).map(|row| {
        PhaseKpiCell {
            overall_flag: phase.get(row, "overall_flag").trim().to_string(),
            contract_type: phase.get(row, "contract_type").trim().to_string(),
            contract_period: phase.get(row, "contract_period").trim().to_string(),
            phase_bucket: phase.get(row, "phase_bucket").trim().to_string(),
            phase_pct: pf_opt(phase.get(row, "phase_pct")),
            latest_nps: pf_opt(phase.get(row, "latest_nps")),
            nps_base_line: pf_opt(phase.get(row, "nps_base_line")),
            nps_flag: phase.get(row, "nps_flag").trim().to_string(),
            contact_flag: phase.get(row, "contact_flag").trim().to_string(),
            seika_status: phase.get(row, "seika_status").trim().to_string(),
            alert_msg: phase.get(row, "alert_msg").trim().to_string(),
        }
    });

    // ---- 接触量推移 (週次。期間絞込の影響を受けない、コメント参照) ----
    let mut mtg_by_week: HashMap<String, u32> = HashMap::new();
    for row in &meetings_all {
        if let Some(w) = week_start_jst(timeline.get(row, "start_time")) {
            *mtg_by_week.entry(w).or_insert(0) += 1;
        }
    }
    let contact_rows: Vec<(String, u32, u32)> = contact_weekly
        .rows
        .iter()
        .filter(|row| contact_weekly.get(row, "deal_id").trim() == deal_id)
        .filter_map(|row| {
            let w = contact_weekly.get(row, "week_start").trim();
            if w.is_empty() {
                return None;
            }
            Some((
                w.to_string(),
                pu32(contact_weekly.get(row, "email_count")),
                pu32(contact_weekly.get(row, "call_count")),
            ))
        })
        .collect();
    let contact_trend = merge_weekly_trend(&contact_rows, &mtg_by_week);

    // ---- NPS推移 ----
    let nps_trend = health
        .rows
        .iter()
        .find(|row| health.get(row, "deal_id").trim() == deal_id)
        .map(|row| build_nps_trend(&health, row))
        .unwrap_or_default();

    let src = |sheet: &str, d: &SheetData, matched: usize, cached: bool| SourceInfo {
        sheet: sheet.to_string(),
        total_rows: d.rows.len(),
        matched_rows: matched,
        from_cache: cached,
        age_secs: d.fetched_at.elapsed().as_secs(),
    };

    Ok(TabPayload {
        data: DealDetail {
            deal_id: deal_id.to_string(),
            customer_label,
            consultant_name,
            pipeline_label,
            stage_label,
            meetings,
            meetings_shown,
            meetings_total,
            latest_meeting_start,
            rollup: rollup_cell,
            phase_kpi: phase_cell,
            contact_trend,
            nps_trend,
        },
        sources: vec![
            src("コンサルMTGタイムライン", &timeline, meetings_total, timeline_cached),
            src("コンサル接触率_週次", &contact_weekly, contact_rows.len(), contact_cached),
            src("コンサル健全性_月次", &health, if health.rows.iter().any(|r| health.get(r, "deal_id").trim() == deal_id) { 1 } else { 0 }, health_cached),
            src("コンサル接触ロールアップ", &rollup, rollup.rows.iter().filter(|r| rollup.get(r, "deal_id").trim() == deal_id).count(), rollup_cached),
            src("コンサルフェーズKPI", &phase, phase.rows.iter().filter(|r| phase.get(r, "deal_id").trim() == deal_id).count(), phase_cached),
        ],
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
        // こちらは**タブ側が詰める**（`sort` / `period` の解釈結果を知っているのはここ）。
        invalid_values: audit.into_vec(),
    })
}

/// javascript.html `_p13DrawTrends`(15486-15493行) の週集合マージを移植。
/// contact_weekly の週 ∪ MTG実施週。BTreeSet で昇順(=文字列昇順=時系列順)に揃える。
fn merge_weekly_trend(contact: &[(String, u32, u32)], mtg_by_week: &HashMap<String, u32>) -> Vec<WeeklyContactPoint> {
    let mut weeks: BTreeSet<String> = BTreeSet::new();
    let mut contact_map: HashMap<&str, (u32, u32)> = HashMap::new();
    for (w, email, call) in contact {
        weeks.insert(w.clone());
        contact_map.insert(w.as_str(), (*email, *call));
    }
    for w in mtg_by_week.keys() {
        weeks.insert(w.clone());
    }
    weeks
        .into_iter()
        .map(|w| {
            let (email_count, call_count) = contact_map.get(w.as_str()).copied().unwrap_or((0, 0));
            let mtg_count = mtg_by_week.get(&w).copied().unwrap_or(0);
            WeeklyContactPoint { week_start: w, email_count, call_count, mtg_count }
        })
        .collect()
}

/// javascript.html `_p13IndexTrends`(15059-15081行) の定期NPS展開 + 満了時NPS付与を移植。
pub fn build_nps_trend(data: &SheetData, row: &[Arc<str>]) -> Vec<NpsPoint> {
    let series = parse_num_arr_opt(data.get(row, "nps_series_json"));
    let mut points: Vec<NpsPoint> = series
        .into_iter()
        .enumerate()
        .filter_map(|(i, v)| v.map(|v| NpsPoint { label: period_label(i), nps: v }))
        .collect();

    let latest_period = data.get(row, "latest_nps_period").trim();
    if latest_period == "満了時" {
        if let Some(v) = pf_opt(data.get(row, "latest_nps")) {
            points.push(NpsPoint { label: "満了時".to_string(), nps: v });
        }
    }
    points
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    fn sheet(header: &[&str], rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows.into_iter().map(|r| r.into_iter().map(Arc::from).collect()).collect(),
            fetched_at: Instant::now(),
        }
    }

    const TIMELINE_HEADER: &[&str] = &[
        "deal_id", "customer_label", "consultant_owner_id", "consultant_owner_name", "pipeline_label",
        "stage_label", "meeting_id", "meeting_uuid", "zoom_meeting_id", "start_time", "topic",
        "duration_min", "host_email", "summary_overview", "summary_details_json", "next_steps_json",
        "zoom_url", "hubspot_meeting_id", "source",
    ];

    fn tl_row(deal: &str, name: &str, start: &str, source: &str) -> Vec<&'static str> {
        vec![
            Box::leak(deal.to_string().into_boxed_str()),
            "テスト商事",
            "999",
            Box::leak(name.to_string().into_boxed_str()),
            "リクロジ_納品管理",
            "定期1",
            "m1",
            "uuid1",
            "z1",
            Box::leak(start.to_string().into_boxed_str()),
            "定例",
            "30",
            "host@example.com",
            "",
            "[]",
            "[]",
            "",
            "",
            Box::leak(source.to_string().into_boxed_str()),
        ]
    }

    // ---- 担当者名解決 ----

    #[test]
    fn 担当者名がid形式ならフェーズkpiにフォールバックする() {
        assert_eq!(resolve_consultant_name("1867408508", Some("鶴見 亮介")), "鶴見 亮介");
        assert_eq!(resolve_consultant_name("", Some("鶴見 亮介")), "鶴見 亮介");
        assert_eq!(resolve_consultant_name("山田太郎", Some("別の名前")), "山田太郎", "有効な名前があればそちらを優先");
        assert_eq!(resolve_consultant_name("", None), "（未設定）");
    }

    // ---- Deal 一覧: host_email_match 除外 ----

    #[test]
    fn host_email_matchは除外されskipped件数に計上される() {
        let d = sheet(
            TIMELINE_HEADER,
            vec![
                tl_row("1", "山田", "2025-08-01T00:00:00Z", "zoom_email_match"),
                tl_row("1", "山田", "2025-08-08T00:00:00Z", "host_email_match"),
            ],
        );
        let (deals, skipped) = build_deal_index(&d, &HashMap::new());
        assert_eq!(skipped, 1);
        assert_eq!(deals.len(), 1);
        assert_eq!(deals[0].meeting_count, 1, "host_email_match の1件はカウントしない");
    }

    #[test]
    fn 最新mtg開始時刻は文字列時系列で最大を取る() {
        let d = sheet(
            TIMELINE_HEADER,
            vec![
                tl_row("1", "山田", "2025-08-01T00:00:00Z", "zoom_email_match"),
                tl_row("1", "山田", "2025-09-15T00:00:00Z", "zoom_email_match"),
                tl_row("1", "山田", "2025-08-20T00:00:00Z", "zoom_email_match"),
            ],
        );
        let (deals, _) = build_deal_index(&d, &HashMap::new());
        assert_eq!(deals[0].latest_start_time.as_deref(), Some("2025-09-15T00:00:00Z"));
        assert_eq!(deals[0].meeting_count, 3);
    }

    // ---- 週次接触量推移: マージと安定ソート ----

    #[test]
    fn 接触週とmtg週の和集合を昇順で返す() {
        let contact = vec![("2025-08-04".to_string(), 3u32, 1u32)];
        let mut mtg: HashMap<String, u32> = HashMap::new();
        mtg.insert("2025-07-28".to_string(), 1);
        mtg.insert("2025-08-04".to_string(), 2);
        let trend = merge_weekly_trend(&contact, &mtg);
        assert_eq!(trend.len(), 2, "和集合で2週");
        assert_eq!(trend[0].week_start, "2025-07-28", "昇順(時系列順)");
        assert_eq!(trend[1].email_count, 3);
        assert_eq!(trend[1].mtg_count, 2);
        assert_eq!(trend[0].email_count, 0, "接触データが無い週は0(欠損を捏造しない)");
    }

    #[test]
    fn 同じ週集計を2回呼んでも並びが安定する() {
        let contact = vec![("2025-08-04".to_string(), 1, 0), ("2025-07-28".to_string(), 2, 0)];
        let mtg = HashMap::new();
        let a = merge_weekly_trend(&contact, &mtg);
        let b = merge_weekly_trend(&contact, &mtg);
        assert_eq!(a.iter().map(|p| p.week_start.clone()).collect::<Vec<_>>(),
                   b.iter().map(|p| p.week_start.clone()).collect::<Vec<_>>());
    }

    // ---- 週開始日(JST月曜起点)変換 ----

    #[test]
    fn jst換算で正しい月曜起点週になる() {
        // UTC 2025-08-01T00:58:19Z → JST 2025-08-01 09:58 (金曜) → その週の月曜は 2025-07-28
        assert_eq!(week_start_jst("2025-08-01T00:58:19Z").as_deref(), Some("2025-07-28"));
    }

    #[test]
    fn utc深夜でjst日付が繰り上がるケース() {
        // UTC 2025-08-03T16:00:00Z (日曜) → JST +9h = 2025-08-04 01:00 (月曜) → 週開始は当日
        assert_eq!(week_start_jst("2025-08-03T16:00:00Z").as_deref(), Some("2025-08-04"));
    }

    // ---- NPS推移: 欠番があってもラベルの通し番号がズレない ----

    #[test]
    fn nps欠番があってもラベル番号は元の位置を保つ() {
        let d = sheet(&["nps_series_json", "latest_nps_period", "latest_nps"], vec![vec!["[5, null, 7]", "", ""]]);
        let row: Vec<Arc<str>> = vec![Arc::from("[5, null, 7]"), Arc::from(""), Arc::from("")];
        let trend = build_nps_trend(&d, &row);
        assert_eq!(trend.len(), 2, "null要素はスキップされるが件数は2件");
        assert_eq!(trend[0].label, "定期①");
        assert_eq!(trend[1].label, "定期③", "2番目(index=1)はnullなので③にジャンプ");
    }

    #[test]
    fn 満了時npsは系列の末尾に付与される() {
        let row: Vec<Arc<str>> = vec![Arc::from("[5]"), Arc::from("満了時"), Arc::from("8")];
        let d = sheet(&["nps_series_json", "latest_nps_period", "latest_nps"], vec![]);
        let trend = build_nps_trend(&d, &row);
        assert_eq!(trend.len(), 2);
        assert_eq!(trend[1].label, "満了時");
        assert_eq!(trend[1].nps, 8.0);
    }

    // ---- 期間絞込 (months_ago) ----

    #[test]
    fn 三ヶ月前の日付を計算できる() {
        let today = NaiveDate::from_ymd_opt(2026, 8, 16).unwrap();
        assert_eq!(months_ago(today, 3), NaiveDate::from_ymd_opt(2026, 5, 16).unwrap());
    }

    #[test]
    fn 年をまたぐ月数減算ができる() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 10).unwrap();
        assert_eq!(months_ago(today, 3), NaiveDate::from_ymd_opt(2025, 10, 10).unwrap());
    }

    // ---- MTGカードの信頼度分類 ----

    #[test]
    fn source別に信頼度が分類される() {
        assert_eq!(confidence_of("zoom_email_match"), "high");
        assert_eq!(confidence_of("zoom_topic_match"), "mid");
        assert_eq!(confidence_of("hubspot_only"), "mid");
        assert_eq!(confidence_of("host_email_match"), "low");
    }

    #[test]
    fn hubspot_onlyで要約が空ならhas_summaryはfalse() {
        let header = TIMELINE_HEADER;
        let row_vals = tl_row("1", "山田", "2025-08-01T00:00:00Z", "hubspot_only");
        let d = sheet(header, vec![row_vals]);
        let card = build_meeting_card(&d, &d.rows[0]);
        assert!(!card.has_summary, "AI Companion要約が無いhubspot_onlyはhas_summary=false");
    }

    // ---- sort / period の不正値を無音で既定にしない（2026-08-17 追加） ----

    fn detail_q(sort: Option<&str>, period: Option<&str>) -> DealDetailQuery {
        DealDetailQuery {
            deal_id: "1".into(),
            sort: sort.map(str::to_string),
            period: period.map(str::to_string),
        }
    }

    #[test]
    fn periodの不正値は全期間に落ちたことを応答に出す() {
        // `period_cutoff` は `_ => None`（＝絞らない）なので
        // **`?period=6M` は「6ヶ月」のつもりで全期間**になる。
        // 動機になった `deals_status=NONSENSE`（絞ったつもりで全件）と同じ形。
        let (_, period, audit) = resolve_detail_query(&detail_q(None, Some("6M")));
        assert_eq!(period, "all", "既定値へ落とす挙動は変えない");
        assert!(period_cutoff(period, Utc::now()).is_none(), "実際に絞られない");
        let v = audit.into_vec();
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].param, "period");
        assert_eq!(v[0].given, "6M");
        assert_eq!(v[0].used.as_deref(), Some("all"));
    }

    #[test]
    fn sortの不正値はdescに落ちたことを応答に出す() {
        let (sort, _, audit) = resolve_detail_query(&detail_q(Some("ascending"), None));
        assert_eq!(sort, "desc");
        let v = audit.into_vec();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].param, "sort");
        assert_eq!(v[0].used.as_deref(), Some("desc"));
    }

    #[test]
    fn 正しいsortとperiodでは何も報告しない() {
        // **陰性対照**
        for (s, p) in [
            (None, None),
            (Some("asc"), Some("3m")),
            (Some("desc"), Some("all")),
            (Some("desc"), Some("12m")),
            (Some(""), Some("")),
        ] {
            let (_, _, a) = resolve_detail_query(&detail_q(s, p));
            assert!(a.is_empty(), "sort={s:?} period={p:?} は正常なので黙る");
        }
    }
}
