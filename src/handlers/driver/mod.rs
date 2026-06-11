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
use std::sync::Arc;
use tracing::{error, warn};

use crate::AppState;

pub mod data;

use data::{
    fetch_category_counts, fetch_occupation_detail, fetch_occupation_list, fetch_wage_age,
    CategoryInfo, DriverDataError,
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
            return render_degraded("Turso country-statistics 未接続のため職種カルテを表示できません")
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
}

async fn tab_driver_detail(
    State(state): State<Arc<AppState>>,
    Path(jobtag_id): Path<i64>,
) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded("Turso country-statistics 未接続のため職種カルテを表示できません")
                .into_response();
        }
    };

    let detail = match tokio::task::spawn_blocking(move || fetch_occupation_detail(&turso, jobtag_id))
        .await
    {
        Ok(Ok(d)) => d,
        Ok(Err(DriverDataError::NotFound)) => {
            return (StatusCode::NOT_FOUND, format!("jobtag_id={jobtag_id} は未投入")).into_response();
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
    let interest_json = serde_json::to_string(&detail.interest_scores).unwrap_or_else(|_| "[]".into());
    let values_json = serde_json::to_string(&detail.values_scores).unwrap_or_else(|_| "[]".into());
    let skills_json = serde_json::to_string(&detail.skills_scores).unwrap_or_else(|_| "[]".into());

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
    };

    let page = DriverDetailPage {
        occupation: view,
        wage_rows_json,
        interest_json,
        values_json,
        skills_json,
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
            return Json(json!({"error": "turso not connected", "occupations": []})).into_response();
        }
    };
    let category = q.category.clone();
    match tokio::task::spawn_blocking(move || fetch_occupation_list(&turso, category.as_deref())).await {
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
        Ok(Err(DriverDataError::NotFound)) => {
            (StatusCode::NOT_FOUND, Json(json!({"error": "not found", "jobtag_id": jobtag_id})))
                .into_response()
        }
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
