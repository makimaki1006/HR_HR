//! 地域分析タブ: tab handler + エンドポイント群。
//!
//! ## エンドポイント
//! | route                                    | 内容                              |
//! |------------------------------------------|-----------------------------------|
//! | GET /tab/regional_analysis               | タブ本体 (フィルタバー + パネル枠) |
//! | GET /api/regional/municipalities         | 市区町村カスケード (都道府県連動) |
//! | GET /api/regional/job_openings_ratio     | 有効求人倍率 推移 (e-Stat)        |
//! | GET /api/regional/labor_stats            | 労働統計指標カード (e-Stat)        |
//! | GET /api/regional/industry_structure     | 産業構造テーブル (国勢調査)        |
//! | GET /api/regional/population_pyramid     | 人口ピラミッド (国勢調査)         |
//! | GET /api/regional/wage_comparison        | 最低賃金 (厚労省)                 |
//! | GET /api/regional/company_matrix         | 企業成長マトリックス (外部企業)   |
//! | GET /api/regional/foreign_residents      | 在留外国人 (住民基本台帳)         |
//! | GET /api/regional/internet_usage         | インターネット利用 (通信利用動向) |
//! | GET /api/regional/occupation             | 職業別就業者 (国勢調査)           |

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::fetch::{
    fetch_company_matrix, fetch_foreign_residents, fetch_industry_structure, fetch_internet_usage,
    fetch_job_openings_ratio, fetch_labor_stats, fetch_municipalities,
    fetch_occupation_distribution, fetch_population_pyramid, fetch_prefectures,
    fetch_wage_comparison, RegionalFilter,
};
use super::render::{
    render_company_matrix, render_foreign_residents, render_industry_structure,
    render_internet_usage, render_job_openings_ratio, render_labor_stats, render_occupation,
    render_population_pyramid, render_wage_comparison,
};
use crate::handlers::competitive::escape_html;
use crate::AppState;

/// 企業成長マトリックスの取得上限 (散布点数)。
const COMPANY_MATRIX_LIMIT: usize = 300;

/// 産業構造の表示上限 (産業数)。
const INDUSTRY_STRUCTURE_LIMIT: usize = 20;

/// タブ本体: フィルタバー (都道府県→市区町村) + パネル枠。
pub async fn tab_regional_analysis(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    // 都道府県一覧 (municipality_code_master ベース)
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

/// 市区町村 <option> 群 (都道府県連動、municipality_code_master ベース)。
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
        html.push_str(&format!(
            r#"<option value="{v}">{v}</option>"#,
            v = escape_html(m)
        ));
    }
    Html(html)
}

/// 都道府県未選択時の共通レスポンス。
fn pref_required() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-amber-300 text-sm">都道府県を選択してください。</p></div>"#
            .to_string(),
    )
}

/// 集計処理失敗時の共通レスポンス。
fn aggregation_failed() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-red-300 text-sm">集計処理に失敗しました。</p></div>"#
            .to_string(),
    )
}

/// DB 未接続時の共通レスポンス。
fn db_unavailable() -> Html<String> {
    Html(
        r#"<div class="stat-card"><p class="text-red-300 text-sm">外部統計データベースに接続できません。</p></div>"#
            .to_string(),
    )
}

/// 有効求人倍率 推移 (e-Stat)。
pub async fn regional_job_openings_ratio(
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
    let data = tokio::task::spawn_blocking(move || fetch_job_openings_ratio(&st, &f))
        .await
        .ok();
    match data {
        Some(d) => Html(render_job_openings_ratio(&filter, &d)),
        None => aggregation_failed(),
    }
}

/// 労働統計指標カード (e-Stat / 労働政策研究・研修機構)。
pub async fn regional_labor_stats(
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
    let data = tokio::task::spawn_blocking(move || fetch_labor_stats(&st, &f))
        .await
        .ok();
    match data {
        Some(d) => Html(render_labor_stats(&filter, d.as_ref())),
        None => aggregation_failed(),
    }
}

/// 産業構造テーブル (国勢調査)。
pub async fn regional_industry_structure(
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
    let data = tokio::task::spawn_blocking(move || {
        fetch_industry_structure(&st, &f, INDUSTRY_STRUCTURE_LIMIT)
    })
    .await
    .ok();
    match data {
        Some(d) => Html(render_industry_structure(&filter, &d)),
        None => aggregation_failed(),
    }
}

/// 人口ピラミッド (外部統計: 国勢調査)。
pub async fn regional_population_pyramid(
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
    let py = tokio::task::spawn_blocking(move || fetch_population_pyramid(&st, &f))
        .await
        .ok();
    match py {
        Some(p) => Html(render_population_pyramid(&filter, &p)),
        None => aggregation_failed(),
    }
}

/// 最低賃金 (都道府県粒度、厚生労働省)。
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

/// 企業成長マトリックス (外部企業データ)。
pub async fn regional_company_matrix(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<RegionalParams>,
) -> Html<String> {
    let filter = params.to_filter();
    if filter.prefecture.is_empty() {
        return pref_required();
    }
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

/// 在留外国人 (外部統計: 住民基本台帳、都道府県粒度)。
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

/// インターネット利用 (外部統計: 通信利用動向、都道府県粒度)。
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

/// 職業別就業者 (外部統計: 国勢調査・従業地ベース、市区町村/都道府県粒度)。
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
