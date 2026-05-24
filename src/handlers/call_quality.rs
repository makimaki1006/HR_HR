//! 架電クオリティ Axum handler
//!
//! ルート:
//!   GET  /call-quality                          : メインタブ HTML
//!   GET  /api/call-quality/healthz              : ヘルスチェック (auth bypass 推奨)
//!   GET  /api/call-quality/dashboard            : P0 KPI + 月次推移 JSON
//!   GET  /api/call-quality/prefecture           : P4 都道府県別 アポ率 JSON
//!   GET  /api/call-quality/raw_list             : シート一覧
//!   GET  /api/call-quality/raw/{sheet_name}     : P7 生シート閲覧
//!   POST /api/call-quality/cache/clear          : キャッシュ全クリア
//!
//! 認証は HR_HR 既存の `auth_middleware` で全 protected_routes と一緒に被せる前提。
//! 本モジュールは Router::merge() で組み込む側で State<Arc<AppState>> 接続される。

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::{get, post},
    Router,
};
use serde::Serialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::cache::call_quality_cache::CacheStats;
use crate::models::call_quality::{
    parse_number, DashboardKpi, DashboardQuery, DashboardResponse, MonthlyPoint, PrefectureBar,
    PrefectureResponse, RawSheetListResponse, SheetResponse, RAW_SHEET_NAMES,
};
use crate::AppState;

// =========================================================================
// Router 公開関数
// =========================================================================

/// 架電クオリティ用ルーター。`Arc<AppState>` を State として扱う。
///
/// `build_app()` の `protected_routes` チェーンに `.merge(handlers::call_quality::router())`
/// として組み込む想定。認証 middleware は親側でまとめてかけられる。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/call-quality", get(page_main))
        .route("/api/call-quality/healthz", get(api_healthz))
        .route("/api/call-quality/dashboard", get(api_dashboard))
        .route("/api/call-quality/prefecture", get(api_prefecture))
        .route("/api/call-quality/raw_list", get(api_raw_list))
        .route("/api/call-quality/raw/{sheet_name}", get(api_raw_sheet))
        .route("/api/call-quality/cache/clear", post(api_cache_clear))
}

// =========================================================================
// メインタブ HTML
// =========================================================================

#[derive(Template)]
#[template(path = "tabs/call_quality.html")]
struct CallQualityPage {
    spreadsheet_id: String,
    sheet_names: Vec<&'static str>,
}

async fn page_main(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let spreadsheet_id = state
        .sheets_client
        .as_ref()
        .map(|c| c.spreadsheet_id().to_string())
        .unwrap_or_else(|| "(SheetsClient 未初期化)".to_string());

    let page = CallQualityPage {
        spreadsheet_id,
        sheet_names: RAW_SHEET_NAMES.to_vec(),
    };
    match page.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            error!("Askama render failed: {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
        }
    }
}

// =========================================================================
// ヘルスチェック
// =========================================================================

#[derive(Serialize)]
struct HealthzResp {
    status: &'static str,
    cache: CacheStats,
    spreadsheet_id: String,
    sheets_client_configured: bool,
}

async fn api_healthz(State(state): State<Arc<AppState>>) -> Json<HealthzResp> {
    let (spreadsheet_id, configured) = match &state.sheets_client {
        Some(c) => (c.spreadsheet_id().to_string(), true),
        None => ("".to_string(), false),
    };
    Json(HealthzResp {
        status: if configured { "ok" } else { "degraded" },
        cache: state.call_quality_cache.stats(),
        spreadsheet_id,
        sheets_client_configured: configured,
    })
}

// =========================================================================
// /api/call-quality/dashboard
// =========================================================================

async fn api_dashboard(
    State(state): State<Arc<AppState>>,
    Query(q): Query<DashboardQuery>,
) -> Result<Json<DashboardResponse>, ApiError> {
    let cache_key = format!(
        "dashboard::{}::{}::{}::{}::{}",
        q.from.as_deref().unwrap_or("-"),
        q.to.as_deref().unwrap_or("-"),
        q.pipeline_filter().unwrap_or("-"),
        q.prefecture_filter().unwrap_or("-"),
        q.member_filter().join("|"),
    );

    // キャッシュ命中チェック
    if let Some(cached) = state.call_quality_cache.get(&cache_key) {
        if let Ok(mut resp) = serde_json::from_value::<DashboardResponse>(cached) {
            resp.from_cache = true;
            return Ok(Json(resp));
        }
    }

    // SheetsClient 未設定なら 502 にせず、空のレスポンスを返して UI を壊さない
    let sheets = state
        .sheets_client
        .as_ref()
        .ok_or_else(|| ApiError::Sheets("SheetsClient 未初期化 (GOOGLE_SA_KEY_B64 / SPREADSHEET_ID 未設定)".to_string()))?;

    // 月次明細をベースに集計
    let rows = sheets
        .get_sheet_as_rows("月次明細")
        .await
        .map_err(|e| ApiError::Sheets(format!("月次明細 取得失敗: {e}")))?;

    let resp = build_dashboard_response(&rows, &q);

    // キャッシュ保存
    if let Ok(v) = serde_json::to_value(&resp) {
        state.call_quality_cache.set(cache_key, v);
    }

    Ok(Json(resp))
}

/// 月次明細から DashboardResponse を生成
///
/// フィルタロジック:
///   - pipeline: 行の pipeline 列と一致
///   - prefecture: 月次明細には prefecture 列が無いため、prefecture フィルタ
///     指定時は「都道府県月次」を読むハンドラに切り替えるのが正攻法。
///     Phase 1 では prefecture が指定された場合は警告ログを出して無視する
///   - members: owner_id 列でフィルタ
///   - from/to: date (YYYY-MM-01) 列でフィルタ
fn build_dashboard_response(
    rows: &[std::collections::HashMap<String, String>],
    q: &DashboardQuery,
) -> DashboardResponse {
    if q.prefecture_filter().is_some() {
        warn!("Phase 1: prefecture フィルタは未対応 (Phase 2 で /api/.../prefecture-monthly に切替予定)");
    }

    let pipeline_filter = q.pipeline_filter();
    let member_filter = q.member_filter();
    let from = q.from.as_deref();
    let to = q.to.as_deref();

    let filtered: Vec<&std::collections::HashMap<String, String>> = rows
        .iter()
        .filter(|r| {
            // pipeline
            if let Some(p) = pipeline_filter {
                if r.get("pipeline").map(String::as_str).unwrap_or("") != p {
                    return false;
                }
            }
            // members
            if !member_filter.is_empty() {
                let owner = r.get("owner_id").map(String::as_str).unwrap_or("");
                if !member_filter.iter().any(|m| m == owner) {
                    return false;
                }
            }
            // date range
            let date = r
                .get("date")
                .map(String::as_str)
                .or_else(|| r.get("year_month").map(String::as_str))
                .unwrap_or("");
            if let Some(f) = from {
                if date < f {
                    return false;
                }
            }
            if let Some(t) = to {
                if date > t {
                    return false;
                }
            }
            true
        })
        .collect();

    // ---- KPI 集計 (全期間合算) ----
    let sum = |col: &str| -> f64 {
        filtered
            .iter()
            .filter_map(|r| r.get(col))
            .filter_map(|v| parse_number(v))
            .sum()
    };

    let call = sum("call_count");
    let apo = sum("apo_count");
    let na_due = sum("na_due");
    let na_ontime = sum("na_done_ontime");
    let high_call = sum("high_call_deals");
    let new_open = sum("new_open_deals");

    let apo_rate = if call > 0.0 { apo / call * 100.0 } else { 0.0 };
    let na_rate = if na_due > 0.0 {
        na_ontime / na_due * 100.0
    } else {
        0.0
    };

    let kpis = vec![
        kpi("総架電数", call, "件"),
        kpi("アポ数", apo, "件"),
        kpi("アポ率", round_2(apo_rate), "%"),
        kpi("NA遵守率", round_2(na_rate), "%"),
        kpi("新規 Open Deal", new_open, "件"),
        kpi("高頻度架電 Deal", high_call, "件"),
    ];

    // ---- 月次推移 (P3 ECharts) ----
    let monthly_call = aggregate_monthly(&filtered, "call_count");
    let monthly_apo_rate = aggregate_monthly_rate(&filtered, "apo_count", "call_count");

    DashboardResponse {
        kpis,
        monthly_call_trend: monthly_call,
        monthly_apo_rate_trend: monthly_apo_rate,
        updated_at: now_jst_str(),
        from_cache: false,
    }
}

fn kpi(label: &str, value: f64, unit: &str) -> DashboardKpi {
    DashboardKpi {
        label: label.to_string(),
        value,
        unit: unit.to_string(),
        delta: None,
        delta_pct: None,
        delta_ppt: None,
    }
}

fn round_2(v: f64) -> f64 {
    (v * 100.0).round() / 100.0
}

/// year_month ごとに col を合算し、降順上位 8 ヶ月を昇順で返す
fn aggregate_monthly(
    rows: &[&std::collections::HashMap<String, String>],
    col: &str,
) -> Vec<MonthlyPoint> {
    let mut map: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for r in rows {
        let ym = match r.get("year_month") {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        let v = r.get(col).and_then(|s| parse_number(s)).unwrap_or(0.0);
        *map.entry(ym).or_default() += v;
    }
    // 昇順で全件 → 末尾 8 件取得 (BTreeMap は key 昇順)
    let mut all: Vec<MonthlyPoint> = map
        .into_iter()
        .map(|(m, v)| MonthlyPoint { month: m, value: v })
        .collect();
    let len = all.len();
    if len > 8 {
        all.drain(0..len - 8);
    }
    all
}

/// year_month ごとに numerator / denominator * 100 を集計
fn aggregate_monthly_rate(
    rows: &[&std::collections::HashMap<String, String>],
    numerator: &str,
    denominator: &str,
) -> Vec<MonthlyPoint> {
    let mut num: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    let mut den: std::collections::BTreeMap<String, f64> = std::collections::BTreeMap::new();
    for r in rows {
        let ym = match r.get("year_month") {
            Some(s) if !s.is_empty() => s.clone(),
            _ => continue,
        };
        let n = r.get(numerator).and_then(|s| parse_number(s)).unwrap_or(0.0);
        let d = r.get(denominator).and_then(|s| parse_number(s)).unwrap_or(0.0);
        *num.entry(ym.clone()).or_default() += n;
        *den.entry(ym).or_default() += d;
    }
    let mut all: Vec<MonthlyPoint> = num
        .into_iter()
        .map(|(m, n)| {
            let d = den.get(&m).copied().unwrap_or(0.0);
            let v = if d > 0.0 { n / d * 100.0 } else { 0.0 };
            MonthlyPoint {
                month: m,
                value: round_2(v),
            }
        })
        .collect();
    let len = all.len();
    if len > 8 {
        all.drain(0..len - 8);
    }
    all
}

// =========================================================================
// /api/call-quality/prefecture (P4)
// =========================================================================

async fn api_prefecture(
    State(state): State<Arc<AppState>>,
) -> Result<Json<PrefectureResponse>, ApiError> {
    const CACHE_KEY: &str = "prefecture_bars_v1";

    if let Some(cached) = state.call_quality_cache.get(CACHE_KEY) {
        if let Ok(mut resp) = serde_json::from_value::<PrefectureResponse>(cached) {
            resp.from_cache = true;
            return Ok(Json(resp));
        }
    }

    let sheets = state
        .sheets_client
        .as_ref()
        .ok_or_else(|| ApiError::Sheets("SheetsClient 未初期化".to_string()))?;

    let rows = sheets
        .get_sheet_as_rows("都道府県月次")
        .await
        .map_err(|e| ApiError::Sheets(format!("都道府県月次 取得失敗: {e}")))?;

    // prefecture ごとに call/apo を全月合算
    let mut agg: std::collections::HashMap<String, (f64, f64)> =
        std::collections::HashMap::new();

    for r in &rows {
        let pref = r.get("prefecture").map(String::as_str).unwrap_or("").trim();
        if pref.is_empty() || pref.starts_with('_') {
            continue;
        }
        if matches!(pref, "一" | "不明" | "unknown" | "UNKNOWN") {
            continue;
        }
        let call = r.get("call_count").and_then(|s| parse_number(s)).unwrap_or(0.0);
        let apo = r.get("apo_count").and_then(|s| parse_number(s)).unwrap_or(0.0);
        let entry = agg.entry(pref.to_string()).or_insert((0.0, 0.0));
        entry.0 += call;
        entry.1 += apo;
    }

    let mut bars: Vec<PrefectureBar> = agg
        .into_iter()
        .map(|(pref, (call, apo))| {
            let rate = if call > 0.0 { apo / call * 100.0 } else { 0.0 };
            PrefectureBar {
                prefecture: pref,
                call_count: call,
                apo_count: apo,
                apo_rate: round_2(rate),
            }
        })
        .collect();

    // call_count 降順 (上位 47 都道府県すべて返すが、フロントは Top 20 表示等で間引く)
    bars.sort_by(|a, b| {
        b.call_count
            .partial_cmp(&a.call_count)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let resp = PrefectureResponse {
        bars,
        updated_at: now_jst_str(),
        from_cache: false,
    };

    if let Ok(v) = serde_json::to_value(&resp) {
        state.call_quality_cache.set(CACHE_KEY, v);
    }
    Ok(Json(resp))
}

// =========================================================================
// /api/call-quality/raw_list and /raw/{sheet_name}
// =========================================================================

async fn api_raw_list() -> Json<RawSheetListResponse> {
    Json(RawSheetListResponse {
        sheets: RAW_SHEET_NAMES.to_vec(),
    })
}

async fn api_raw_sheet(
    State(state): State<Arc<AppState>>,
    Path(sheet_name): Path<String>,
) -> Result<Json<SheetResponse>, ApiError> {
    // ホワイトリスト検証
    if !RAW_SHEET_NAMES.iter().any(|s| *s == sheet_name) {
        return Ok(Json(SheetResponse::error(
            &sheet_name,
            "許可されていないシート名",
        )));
    }

    let cache_key = format!("raw::{sheet_name}");
    if let Some(cached) = state.call_quality_cache.get(&cache_key) {
        if let Ok(resp) = serde_json::from_value::<SheetResponse>(cached) {
            return Ok(Json(resp.mark_from_cache()));
        }
    }

    let sheets = match state.sheets_client.as_ref() {
        Some(c) => c,
        None => {
            return Ok(Json(SheetResponse::error(
                &sheet_name,
                "SheetsClient 未初期化",
            )));
        }
    };

    let resp = match sheets.get_sheet_as_rows(&sheet_name).await {
        Ok(rows) => SheetResponse::new(rows),
        Err(e) => {
            warn!("raw sheet 取得失敗: {sheet_name} → {e}");
            SheetResponse::error(&sheet_name, e)
        }
    };

    if let Ok(v) = serde_json::to_value(&resp) {
        state.call_quality_cache.set(cache_key, v);
    }
    Ok(Json(resp))
}

// =========================================================================
// /api/call-quality/cache/clear
// =========================================================================

async fn api_cache_clear(State(state): State<Arc<AppState>>) -> Json<Value> {
    let n = state.call_quality_cache.clear();
    info!("call_quality cache cleared: {n} entries");
    Json(json!({ "cleared": n }))
}

// =========================================================================
// エラー型
// =========================================================================

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("Sheets API error: {0}")]
    Sheets(String),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        let (status, msg) = match &self {
            ApiError::Sheets(m) => (StatusCode::BAD_GATEWAY, m.clone()),
        };
        error!("api_error: {self}");
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

// =========================================================================
// ユーティリティ
// =========================================================================

fn now_jst_str() -> String {
    use chrono::{FixedOffset, Utc};
    let jst = FixedOffset::east_opt(9 * 3600).expect("JST");
    Utc::now()
        .with_timezone(&jst)
        .format("%Y-%m-%d %H:%M")
        .to_string()
}
