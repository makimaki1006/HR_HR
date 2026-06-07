//! 地域×業界分析タブ: tab handler + 3 endpoint。
//!
//! ## エンドポイント
//! | route                                   | 内容                               |
//! |-----------------------------------------|------------------------------------|
//! | GET /tab/regional_analysis              | タブ本体 (フィルタバー + 3 パネル枠) |
//! | GET /api/regional/municipalities        | 市区町村カスケード (都道府県連動)  |
//! | GET /api/regional/job_types             | 業界カスケード (都道府県+市区町村連動) |
//! | GET /api/regional/salary_histogram      | 1) 給与分布ヒストグラム            |
//! | GET /api/regional/muni_ranking          | 2) 市区町村別ランキング            |
//! | GET /api/regional/emp_salary            | 3) 雇用形態別給与統計              |
//! | GET /api/regional/job_type_salary       | 4) 業界別 給与中央値比較          |
//! | GET /api/regional/population_pyramid     | 5) 人口ピラミッド (国勢調査)      |
//! | GET /api/regional/wage_comparison        | 6) 最低賃金 vs 給与中央値          |
//! | GET /api/regional/company_matrix         | 7) 企業成長マトリックス (外部企業) |
//!
//! 重い集計 (中央値算出のため市区町村ごとに salary_min を取得) は
//! spawn_blocking で実行する (company::handlers のパターン踏襲)。

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::fmt::Write as _;
use std::sync::Arc;
use tower_sessions::Session;

use super::fetch::{
    fetch_company_matrix, fetch_emp_salary_stats, fetch_foreign_residents, fetch_internet_usage,
    fetch_job_type_salary, fetch_job_types, fetch_muni_ranking, fetch_municipalities,
    fetch_occupation_distribution, fetch_population_pyramid, fetch_prefectures,
    fetch_salary_histogram, fetch_wage_comparison, RegionalFilter,
};
use super::render::{
    render_company_matrix, render_emp_salary, render_foreign_residents, render_internet_usage,
    render_job_type_salary, render_muni_ranking, render_occupation, render_population_pyramid,
    render_salary_histogram, render_wage_comparison,
};
use crate::handlers::competitive::escape_html;
use crate::handlers::overview::format_number;
use crate::AppState;

/// 市区町村別ランキングの表示上限。
const MUNI_RANK_LIMIT: usize = 50;

/// 業界別給与比較の表示上限 (業界数)。
const JOB_TYPE_SALARY_LIMIT: usize = 40;

/// 企業成長マトリックスの取得上限 (散布点数)。
const COMPANY_MATRIX_LIMIT: usize = 300;

/// タブ本体: フィルタバー (都道府県→市区町村→業界) + 3 パネル枠。
pub async fn tab_regional_analysis(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    // 都道府県一覧 (DB 接続なしでも空 option で描画継続)
    let prefs = fetch_prefectures(&state);
    let pref_options: String = prefs
        .iter()
        .map(|p| format!(r#"<option value="{v}">{v}</option>"#, v = escape_html(p)))
        .collect();

    let html = include_str!("../../../templates/tabs/regional_analysis.html")
        .replace("{{PREF_OPTIONS}}", &pref_options);
    Html(html)
}

/// カスケード用共通パラメータ。
#[derive(Deserialize)]
pub struct RegionalParams {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
    pub job_type: Option<String>,
}

impl RegionalParams {
    fn to_filter(&self) -> RegionalFilter {
        RegionalFilter {
            prefecture: self.prefecture.clone().unwrap_or_default(),
            municipality: self.municipality.clone().unwrap_or_default(),
            job_type: self.job_type.clone().unwrap_or_default(),
        }
    }
}

/// 市区町村 <option> 群 (都道府県連動)。
pub async fn regional_municipalities(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    if pref.is_empty() {
        return Html(r#"<option value="">全て</option>"#.to_string());
    }
    let munis = fetch_municipalities(&state, &pref);
    let mut html = String::from(r#"<option value="">全て</option>"#);
    for m in &munis {
        write!(
            html,
            r#"<option value="{v}">{v}</option>"#,
            v = escape_html(m)
        )
        .unwrap();
    }
    Html(html)
}

/// 業界(job_type) <option> 群 (都道府県+市区町村連動、件数付き)。
pub async fn regional_job_types(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let pref = params.prefecture.unwrap_or_default();
    let muni = params.municipality.unwrap_or_default();
    let jts = fetch_job_types(&state, &pref, &muni);
    let mut html = String::from(r#"<option value="">全業界</option>"#);
    for (jt, cnt) in &jts {
        write!(
            html,
            r#"<option value="{raw}">{disp} ({cnt})</option>"#,
            raw = jt.replace('"', "&quot;"),
            disp = escape_html(jt),
            cnt = format_number(*cnt),
        )
        .unwrap();
    }
    Html(html)
}

/// DB 未接続時の共通レスポンス (silent fallback 禁止)。
fn db_unavailable() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-red-300 text-sm">ローカル求人データベースに接続できません。</p></div>"#
            .to_string(),
    )
}

/// 都道府県未選択時の共通レスポンス。
fn pref_required() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-amber-300 text-sm">都道府県を選択してください。</p></div>"#
            .to_string(),
    )
}

/// 1) 給与分布ヒストグラム。
pub async fn regional_salary_histogram(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return db_unavailable(),
    };

    let f = filter.clone();
    let hist = tokio::task::spawn_blocking(move || fetch_salary_histogram(&db, &f))
        .await
        .ok();
    match hist {
        Some(h) => Html(render_salary_histogram(&filter, &h)),
        None => Html(
            r#"<div class="stat-card"><p class="text-red-300 text-sm">集計処理に失敗しました。</p></div>"#
                .to_string(),
        ),
    }
}

/// 2) 市区町村別 求人数・給与中央値ランキング。
pub async fn regional_muni_ranking(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return db_unavailable(),
    };

    let f = filter.clone();
    let rows = tokio::task::spawn_blocking(move || fetch_muni_ranking(&db, &f, MUNI_RANK_LIMIT))
        .await
        .ok();
    match rows {
        Some(r) => Html(render_muni_ranking(&filter, &r)),
        None => Html(
            r#"<div class="stat-card"><p class="text-red-300 text-sm">集計処理に失敗しました。</p></div>"#
                .to_string(),
        ),
    }
}

/// 3) 雇用形態別 給与統計。
pub async fn regional_emp_salary(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return db_unavailable(),
    };

    let f = filter.clone();
    let rows = tokio::task::spawn_blocking(move || fetch_emp_salary_stats(&db, &f))
        .await
        .ok();
    match rows {
        Some(r) => Html(render_emp_salary(&filter, &r)),
        None => Html(
            r#"<div class="stat-card"><p class="text-red-300 text-sm">集計処理に失敗しました。</p></div>"#
                .to_string(),
        ),
    }
}

/// 集計処理失敗時の共通レスポンス。
fn aggregation_failed() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-red-300 text-sm">集計処理に失敗しました。</p></div>"#
            .to_string(),
    )
}

/// 4) 業界別 給与中央値比較。
pub async fn regional_job_type_salary(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return db_unavailable(),
    };

    let f = filter.clone();
    let rows =
        tokio::task::spawn_blocking(move || fetch_job_type_salary(&db, &f, JOB_TYPE_SALARY_LIMIT))
            .await
            .ok();
    match rows {
        Some(r) => Html(render_job_type_salary(&filter, &r)),
        None => aggregation_failed(),
    }
}

/// 5) 人口ピラミッド (外部統計: 国勢調査)。
pub async fn regional_population_pyramid(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    // turso_db / hw_db いずれも無い場合のみ DB 未接続扱い。
    if state.turso_db.is_none() && state.hw_db.is_none() {
        return db_unavailable();
    }

    let st = state.clone();
    let f = filter.clone();
    let py = tokio::task::spawn_blocking(move || fetch_population_pyramid(&st, &f))
        .await
        .ok();
    match py {
        Some(p) => Html(render_population_pyramid(&filter, &p)),
        None => aggregation_failed(),
    }
}

/// 6) 最低賃金 vs 給与中央値 (都道府県粒度)。
pub async fn regional_wage_comparison(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    if state.turso_db.is_none() && state.hw_db.is_none() {
        return db_unavailable();
    }

    let st = state.clone();
    let f = filter.clone();
    let cmp = tokio::task::spawn_blocking(move || fetch_wage_comparison(&st, &f))
        .await
        .ok();
    match cmp {
        Some(c) => Html(render_wage_comparison(&filter, &c)),
        None => aggregation_failed(),
    }
}

/// 7) 企業成長マトリックス (外部企業データ)。
pub async fn regional_company_matrix(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    // 外部企業データ (salesnow_db) 未接続時は明示メッセージ (silent fallback 禁止)。
    if state.salesnow_db.is_none() {
        return Html(
            r#"<div class="stat-card"><p class="text-amber-300 text-sm">外部企業データに接続できません。</p></div>"#
                .to_string(),
        );
    }

    let st = state.clone();
    let f = filter.clone();
    let pts =
        tokio::task::spawn_blocking(move || fetch_company_matrix(&st, &f, COMPANY_MATRIX_LIMIT))
            .await
            .ok();
    match pts {
        Some(p) => Html(render_company_matrix(&filter, &p)),
        None => aggregation_failed(),
    }
}

/// 8) 在留外国人 (外部統計: 住民基本台帳、都道府県粒度)。
pub async fn regional_foreign_residents(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    if state.turso_db.is_none() && state.hw_db.is_none() {
        return db_unavailable();
    }
    let st = state.clone();
    let f = filter.clone();
    let fr = tokio::task::spawn_blocking(move || fetch_foreign_residents(&st, &f))
        .await
        .ok();
    match fr {
        Some(d) => Html(render_foreign_residents(&filter, &d)),
        None => aggregation_failed(),
    }
}

/// 9) インターネット利用 (外部統計: 通信利用動向、都道府県粒度)。
pub async fn regional_internet_usage(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    if state.turso_db.is_none() && state.hw_db.is_none() {
        return db_unavailable();
    }
    let st = state.clone();
    let f = filter.clone();
    let iu = tokio::task::spawn_blocking(move || fetch_internet_usage(&st, &f))
        .await
        .ok();
    match iu {
        Some(d) => Html(render_internet_usage(&filter, &d)),
        None => aggregation_failed(),
    }
}

/// 10) 職業別就業者 (外部統計: 国勢調査・従業地ベース、市区町村/都道府県粒度)。
pub async fn regional_occupation(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
    if state.turso_db.is_none() && state.hw_db.is_none() {
        return db_unavailable();
    }
    let st = state.clone();
    let f = filter.clone();
    let occ = tokio::task::spawn_blocking(move || fetch_occupation_distribution(&st, &f))
        .await
        .ok();
    match occ {
        Some(d) => Html(render_occupation(&filter, &d)),
        None => aggregation_failed(),
    }
}
