//! 職種カルテタブ (driver/職業情報) Axum handler
//!
//! ルート（call_quality 流儀: Router::merge() で組み込み）:
//!   GET /tab/driver                     : 一覧（カテゴリでフィルタ可: ?category=driver）
//!   GET /tab/driver/{jobtag_id}         : 個別カルテ（賃金センサス + JILPT 解説 + スコア）
//!   GET /api/driver/list                : 職業一覧 JSON
//!   GET /api/driver/{jobtag_id}         : 個別職業 JSON
//!   GET /api/driver/wage/{wage_code}    : 賃金センサス年齢別 JSON
//!
//! データ出典:
//!   * 賃金構造基本統計調査 令和7年 表5（厚生労働省、e-Stat 00450091）
//!   * 職業情報データベース 解説系 ver.7.01 / 簡易版数値系 ver.7.00（JILPT）

use askama::Template;
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json},
    routing::get,
    Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{error, warn};

use crate::AppState;

pub mod data;

use data::{
    fetch_age_distribution_by_pref, fetch_category_counts, fetch_category_stats,
    fetch_multiple_occupations, fetch_occupation_detail, fetch_occupation_list, fetch_wage_age,
    get_overall_stats, CategoryInfo, CategoryStats, DriverDataError, OccupationDetail,
    OverallStats, RelatedOrgRow,
};

/// driver タブのルーターを公開する。
///
/// `build_app()` の `protected_routes` チェーンに以下のように組み込む:
///   `.merge(handlers::driver::router())`
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tab/driver", get(tab_driver_index))
        .route("/tab/driver/{jobtag_id}", get(tab_driver_detail))
        .route("/api/driver/list", get(api_driver_list))
        .route("/api/driver/{jobtag_id}", get(api_driver_detail))
        .route("/api/driver/wage/{wage_code}", get(api_driver_wage))
        .route(
            "/api/driver/{jobtag_id}/age_distribution",
            get(api_age_distribution),
        )
        .route("/tab/driver/compare", get(tab_driver_compare))
}

// =========================================================================
// 一覧タブ HTML
// =========================================================================

#[derive(Deserialize, Default)]
pub struct ListQuery {
    /// category フィルタ ('driver' など)。未指定で全カテゴリ。
    pub category: Option<String>,
}

#[derive(Template)]
#[template(path = "tabs/driver_index.html")]
struct DriverIndexPage {
    occupations: Vec<OccupationCard>,
    selected_category: String,
    categories: Vec<CategoryInfo>,
}

#[derive(Serialize, Clone)]
pub struct OccupationCard {
    pub jobtag_id: i64,
    pub name: String,
    pub category: String,
    pub wage_census_code: String,
    pub wage_census_name: String,
    pub avg_age: Option<f64>,
    pub annual_salary_man_yen: Option<f64>,
    pub workers_count: Option<f64>,
    pub aliases: String,
}

async fn tab_driver_index(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded(
                "Turso country-statistics 未接続のため職種カルテを表示できません",
            )
            .into_response();
        }
    };

    let category = q.category.clone();
    let (occupations, categories) = match tokio::task::spawn_blocking(move || {
        let cats = fetch_category_counts(&turso)?;
        let occs = fetch_occupation_list(&turso, category.as_deref())?;
        Ok::<_, String>((occs, cats))
    })
    .await
    {
        Ok(Ok(pair)) => pair,
        Ok(Err(e)) => {
            error!("fetch driver index data failed: {e}");
            return render_degraded("職業一覧の取得に失敗しました").into_response();
        }
        Err(e) => {
            error!("spawn_blocking failed: {e}");
            return render_degraded("内部エラー").into_response();
        }
    };

    let page = DriverIndexPage {
        occupations,
        selected_category: q.category.unwrap_or_default(),
        categories,
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
// 個別カルテ HTML
// =========================================================================

#[derive(Template)]
#[template(path = "tabs/driver_detail.html")]
struct DriverDetailPage {
    occupation: OccupationDetailView,
    wage_rows_json: String,
    interest_json: String,
    values_json: String,
    skills_json: String,
    /// ECharts 経験年数別給与用 JSON（テンプレに直接渡す）
    wage_age_exp_json: String,
}

#[derive(Serialize)]
pub struct OccupationDetailView {
    pub jobtag_id: i64,
    pub name: String,
    pub category: String,
    pub aliases: String,
    pub mhlw_classification: String,
    pub wage_census_code: String,
    pub wage_census_name: String,
    pub summary: String,
    pub what_is_the_job: String,
    pub how_to_become: String,
    pub working_conditions: String,
    pub qualifications: Vec<String>,
    pub total_avg_age: Option<f64>,
    pub total_scheduled_hours: Option<f64>,
    pub total_annual_salary_man_yen: Option<f64>,
    pub total_workers_count: Option<f64>,
    /// 同カテゴリ内ベンチマーク（中央値ベース）
    pub category_stats: CategoryStats,
    /// 全職種ベンチマーク（中央値ベース、起動時キャッシュ）
    pub overall_stats: OverallStats,
    /// 関連団体一覧（EX-D）
    pub related_orgs: Vec<RelatedOrgRow>,
}

async fn tab_driver_detail(
    State(state): State<Arc<AppState>>,
    Path(jobtag_id): Path<i64>,
) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded(
                "Turso country-statistics 未接続のため職種カルテを表示できません",
            )
            .into_response();
        }
    };

    let (detail, category_stats, overall_stats) =
        match tokio::task::spawn_blocking(move || -> Result<_, DriverDataError> {
            let d = fetch_occupation_detail(&turso, jobtag_id)?;
            let stats = fetch_category_stats(&turso, &d.occupation.category).unwrap_or_else(|e| {
                warn!("fetch_category_stats failed (treated as empty): {e}");
                CategoryStats::default()
            });
            let overall = get_overall_stats(&turso).clone();
            Ok((d, stats, overall))
        })
        .await
        {
            Ok(Ok(triple)) => triple,
            Ok(Err(DriverDataError::NotFound)) => {
                return (
                    StatusCode::NOT_FOUND,
                    format!("jobtag_id={jobtag_id} は未投入"),
                )
                    .into_response();
            }
            Ok(Err(e)) => {
                error!("fetch_occupation_detail failed: {e}");
                return render_degraded("職業詳細の取得に失敗しました").into_response();
            }
            Err(e) => {
                error!("spawn_blocking failed: {e}");
                return render_degraded("内部エラー").into_response();
            }
        };

    // ECharts 用 JSON を事前直列化
    let wage_rows_json = serde_json::to_string(&detail.wage_rows).unwrap_or_else(|_| "[]".into());
    let interest_json =
        serde_json::to_string(&detail.interest_scores).unwrap_or_else(|_| "[]".into());
    let values_json = serde_json::to_string(&detail.values_scores).unwrap_or_else(|_| "[]".into());
    let skills_json = serde_json::to_string(&detail.skills_scores).unwrap_or_else(|_| "[]".into());
    let wage_age_exp_json =
        serde_json::to_string(&detail.wage_age_exp_rows).unwrap_or_else(|_| "[]".into());

    let total = detail.wage_rows.iter().find(|w| w.age_range_order == 0);

    let view = OccupationDetailView {
        jobtag_id: detail.occupation.jobtag_id,
        name: detail.occupation.name.clone(),
        category: detail.occupation.category.clone(),
        aliases: detail.occupation.aliases.clone(),
        mhlw_classification: detail.occupation.mhlw_classification.clone(),
        wage_census_code: detail.occupation.wage_census_code.clone(),
        wage_census_name: detail.occupation.wage_census_name.clone(),
        summary: detail.description.summary.clone(),
        what_is_the_job: detail.description.what_is_the_job.clone(),
        how_to_become: detail.description.how_to_become.clone(),
        working_conditions: detail.description.working_conditions.clone(),
        qualifications: detail.qualifications.clone(),
        total_avg_age: total.and_then(|t| t.avg_age),
        total_scheduled_hours: total.and_then(|t| t.scheduled_hours),
        total_annual_salary_man_yen: total.and_then(|t| t.annual_salary_man_yen),
        total_workers_count: total.and_then(|t| t.workers_count_tenfold),
        category_stats,
        overall_stats,
        related_orgs: detail.related_orgs.clone(),
    };

    let page = DriverDetailPage {
        occupation: view,
        wage_rows_json,
        interest_json,
        values_json,
        skills_json,
        wage_age_exp_json,
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
// JSON API
// =========================================================================

async fn api_driver_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ListQuery>,
) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            warn!("api_driver_list: turso_db None");
            return Json(json!({"error": "turso not connected", "occupations": []}))
                .into_response();
        }
    };
    let category = q.category.clone();
    match tokio::task::spawn_blocking(move || fetch_occupation_list(&turso, category.as_deref()))
        .await
    {
        Ok(Ok(list)) => Json(json!({"occupations": list})).into_response(),
        Ok(Err(e)) => {
            error!("api_driver_list: {e}");
            // 内部エラー詳細は外部レスポンスに露出させない
            Json(json!({"error": "internal", "occupations": []})).into_response()
        }
        Err(e) => {
            error!("api_driver_list spawn_blocking: {e}");
            Json(json!({"error": "internal", "occupations": []})).into_response()
        }
    }
}

async fn api_driver_detail(
    State(state): State<Arc<AppState>>,
    Path(jobtag_id): Path<i64>,
) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => return Json(json!({"error": "turso not connected"})).into_response(),
    };
    match tokio::task::spawn_blocking(move || fetch_occupation_detail(&turso, jobtag_id)).await {
        Ok(Ok(d)) => Json(serde_json::to_value(d).unwrap_or(Value::Null)).into_response(),
        Ok(Err(DriverDataError::NotFound)) => (
            StatusCode::NOT_FOUND,
            Json(json!({"error": "not found", "jobtag_id": jobtag_id})),
        )
            .into_response(),
        Ok(Err(e)) => {
            error!("api_driver_detail: {e}");
            // 内部エラー詳細は外部レスポンスに露出させない
            Json(json!({"error": "internal"})).into_response()
        }
        Err(e) => {
            error!("api_driver_detail spawn_blocking: {e}");
            Json(json!({"error": "internal"})).into_response()
        }
    }
}

async fn api_driver_wage(
    State(state): State<Arc<AppState>>,
    Path(wage_code): Path<String>,
) -> impl IntoResponse {
    // 入力検証: wage_code は最大10文字の英数字のみ（賃金センサス職種コードは4桁数字想定）
    if wage_code.len() > 10 || !wage_code.chars().all(|c| c.is_ascii_alphanumeric()) {
        return Json(json!({"error": "invalid wage_code", "rows": []})).into_response();
    }
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => return Json(json!({"error": "turso not connected", "rows": []})).into_response(),
    };
    match tokio::task::spawn_blocking(move || fetch_wage_age(&turso, &wage_code)).await {
        Ok(Ok(rows)) => Json(json!({"rows": rows})).into_response(),
        Ok(Err(e)) => {
            error!("api_driver_wage: {e}");
            // 内部エラー詳細は外部レスポンスに露出させない
            Json(json!({"error": "internal", "rows": []})).into_response()
        }
        Err(e) => {
            error!("api_driver_wage spawn_blocking: {e}");
            Json(json!({"error": "internal", "rows": []})).into_response()
        }
    }
}

/// GET /api/driver/{jobtag_id}/age_distribution?prefecture={prefecture}
///
/// 国勢調査 R2 職業中分類の都道府県別年齢分布を返す。
/// wage_census_code がマッピングにない職業は空配列を返す（エラーではない）。
async fn api_age_distribution(
    State(state): State<Arc<AppState>>,
    Path(jobtag_id): Path<i64>,
    Query(params): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let pref: String = params.get("prefecture").cloned().unwrap_or_default();
    if pref.is_empty() {
        return Json(json!({"error": "prefecture is required", "rows": []})).into_response();
    }
    // 都道府県名は最大10文字の日本語のみ許可（簡易バリデーション）
    if pref.chars().count() > 10 {
        return Json(json!({"error": "invalid prefecture", "rows": []})).into_response();
    }

    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => return Json(json!({"error": "turso not connected", "rows": []})).into_response(),
    };

    // jobtag_id から wage_census_code を取得
    let wage_code = match tokio::task::spawn_blocking({
        let turso2 = turso.clone();
        move || -> Result<String, String> {
            let rows = turso2.query(
                "SELECT COALESCE(wage_census_code,'') AS wage_census_code \
                 FROM v2_external_jobtag_occupation WHERE jobtag_id = ?",
                &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
            )?;
            Ok(rows
                .into_iter()
                .next()
                .map(|r| {
                    r.get("wage_census_code")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string()
                })
                .unwrap_or_default())
        }
    })
    .await
    {
        Ok(Ok(code)) => code,
        Ok(Err(e)) => {
            error!("api_age_distribution: wage_code lookup failed: {e}");
            return Json(json!({"error": "internal", "rows": []})).into_response();
        }
        Err(e) => {
            error!("api_age_distribution spawn_blocking: {e}");
            return Json(json!({"error": "internal", "rows": []})).into_response();
        }
    };

    match tokio::task::spawn_blocking(move || {
        fetch_age_distribution_by_pref(&turso, &wage_code, &pref)
    })
    .await
    {
        Ok(Ok(rows)) => Json(json!(rows)).into_response(),
        Ok(Err(e)) => {
            error!("api_age_distribution: fetch failed: {e}");
            Json(json!({"error": "internal", "rows": []})).into_response()
        }
        Err(e) => {
            error!("api_age_distribution spawn_blocking: {e}");
            Json(json!({"error": "internal", "rows": []})).into_response()
        }
    }
}

// =========================================================================
// 比較ビュー HTML
// =========================================================================

#[derive(Deserialize, Default)]
pub struct CompareQuery {
    /// カンマ区切りの jobtag_id（2〜3個）
    pub ids: String,
}

/// 比較ビュー用の1職業エントリ（テンプレートに渡す）
#[derive(Serialize)]
pub struct CompareEntry {
    pub view: OccupationDetailView,
    pub wage_rows_json: String,
    pub interest_json: String,
    pub values_json: String,
}

#[derive(Template)]
#[template(path = "tabs/driver_compare.html")]
struct DriverComparePage {
    entries: Vec<CompareEntry>,
    /// 全職業の年齢別年収をオーバーレイ表示用に直列化した JSON（Vec<Vec<WageAgeRow>>）
    all_wage_json: String,
    /// 各職業名の JSON 配列
    names_json: String,
}

fn parse_compare_ids(ids_str: &str) -> Result<Vec<i64>, (StatusCode, String)> {
    if ids_str.trim().is_empty() {
        return Err((
            StatusCode::BAD_REQUEST,
            "比較には2〜3職業を選んでください".into(),
        ));
    }
    let raw: Vec<&str> = ids_str.split(',').map(|s| s.trim()).collect();
    // 重複除去（順序維持）
    let mut seen = std::collections::HashSet::new();
    let mut unique: Vec<i64> = Vec::new();
    for token in raw {
        let id = token.parse::<i64>().map_err(|_| {
            (
                StatusCode::BAD_REQUEST,
                "IDのパースに失敗しました（整数のみ受け付けます）".to_string(),
            )
        })?;
        if seen.insert(id) {
            unique.push(id);
        }
    }
    if unique.len() < 2 {
        return Err((
            StatusCode::BAD_REQUEST,
            "比較には2〜3職業を選んでください".into(),
        ));
    }
    if unique.len() > 3 {
        return Err((StatusCode::BAD_REQUEST, "最大3職業まで比較できます".into()));
    }
    Ok(unique)
}

/// detail → CompareEntry に変換する（heavy clone を避けるため消費する）
fn detail_to_compare_entry(detail: OccupationDetail) -> CompareEntry {
    let total = detail.wage_rows.iter().find(|w| w.age_range_order == 0);
    let view = OccupationDetailView {
        jobtag_id: detail.occupation.jobtag_id,
        name: detail.occupation.name.clone(),
        category: detail.occupation.category.clone(),
        aliases: detail.occupation.aliases.clone(),
        mhlw_classification: detail.occupation.mhlw_classification.clone(),
        wage_census_code: detail.occupation.wage_census_code.clone(),
        wage_census_name: detail.occupation.wage_census_name.clone(),
        summary: detail.description.summary.clone(),
        what_is_the_job: detail.description.what_is_the_job.clone(),
        how_to_become: detail.description.how_to_become.clone(),
        working_conditions: detail.description.working_conditions.clone(),
        qualifications: detail.qualifications.clone(),
        total_avg_age: total.and_then(|t| t.avg_age),
        total_scheduled_hours: total.and_then(|t| t.scheduled_hours),
        total_annual_salary_man_yen: total.and_then(|t| t.annual_salary_man_yen),
        total_workers_count: total.and_then(|t| t.workers_count_tenfold),
        // 比較ビューではカテゴリベンチマーク不要のためデフォルト値を使用
        category_stats: CategoryStats::default(),
        // 比較ビューでは全職種中央値不要のためデフォルト値を使用
        overall_stats: OverallStats::default(),
        // 比較ビューでは関連団体は表示しないため空 Vec
        related_orgs: Vec::new(),
    };
    let wage_rows_json = serde_json::to_string(&detail.wage_rows).unwrap_or_else(|_| "[]".into());
    let interest_json =
        serde_json::to_string(&detail.interest_scores).unwrap_or_else(|_| "[]".into());
    let values_json = serde_json::to_string(&detail.values_scores).unwrap_or_else(|_| "[]".into());
    CompareEntry {
        view,
        wage_rows_json,
        interest_json,
        values_json,
    }
}

async fn tab_driver_compare(
    State(state): State<Arc<AppState>>,
    Query(q): Query<CompareQuery>,
) -> impl IntoResponse {
    // ids パース・バリデーション
    let ids = match parse_compare_ids(&q.ids) {
        Ok(v) => v,
        Err((code, msg)) => {
            let safe = msg.replace('<', "&lt;").replace('>', "&gt;");
            return (
                code,
                Html(format!(
                    r##"<div class="p-6 bg-red-900/30 border border-red-600 rounded text-red-200">
                        <h2 class="text-lg font-bold mb-2">入力エラー</h2>
                        <p>{safe}</p>
                        <a href="/tab/driver"
                           hx-get="/tab/driver" hx-target="#content" hx-swap="innerHTML"
                           class="inline-block mt-3 text-blue-300 underline">← 一覧に戻る</a>
                    </div>"##
                )),
            )
                .into_response();
        }
    };

    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded(
                "Turso country-statistics 未接続のため職種カルテを表示できません",
            )
            .into_response();
        }
    };

    let ids_clone = ids.clone();
    let details =
        match tokio::task::spawn_blocking(move || fetch_multiple_occupations(&turso, &ids_clone))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                error!("spawn_blocking failed (compare): {e}");
                return render_degraded("内部エラー").into_response();
            }
        };

    // None（取得失敗）をフィルタ；全件失敗なら degraded
    let valid_details: Vec<OccupationDetail> = details.into_iter().flatten().collect();
    if valid_details.is_empty() {
        return render_degraded("指定された職業が見つかりませんでした").into_response();
    }

    // 全職業の年齢別年収データをオーバーレイ用に収集（総計行除外）
    let all_wage: Vec<serde_json::Value> = valid_details
        .iter()
        .map(|d| {
            let rows: Vec<_> = d
                .wage_rows
                .iter()
                .filter(|r| r.age_range_order != 0)
                .collect();
            serde_json::to_value(&rows).unwrap_or(serde_json::Value::Array(vec![]))
        })
        .collect();
    let all_wage_json = serde_json::to_string(&all_wage).unwrap_or_else(|_| "[]".into());

    let names: Vec<&str> = valid_details
        .iter()
        .map(|d| d.occupation.name.as_str())
        .collect();
    let names_json = serde_json::to_string(&names).unwrap_or_else(|_| "[]".into());

    let entries: Vec<CompareEntry> = valid_details
        .into_iter()
        .map(detail_to_compare_entry)
        .collect();

    let page = DriverComparePage {
        entries,
        all_wage_json,
        names_json,
    };
    match page.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            error!("Askama render failed (compare): {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
        }
    }
}

// =========================================================================
// degraded 表示（Turso 未接続時など）
// =========================================================================

fn render_degraded(msg: &str) -> Html<String> {
    let safe = msg.replace('<', "&lt;").replace('>', "&gt;");
    Html(format!(
        r#"<div class="p-6 bg-yellow-900/30 border border-yellow-600 rounded text-yellow-200">
            <h2 class="text-lg font-bold mb-2">職種カルテ — 表示できません</h2>
            <p>{safe}</p>
        </div>"#
    ))
}
