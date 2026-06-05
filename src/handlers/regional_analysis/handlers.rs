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
    fetch_emp_salary_stats, fetch_job_types, fetch_muni_ranking, fetch_municipalities,
    fetch_prefectures, fetch_salary_histogram, RegionalFilter,
};
use super::render::{render_emp_salary, render_muni_ranking, render_salary_histogram};
use crate::handlers::competitive::escape_html;
use crate::handlers::overview::format_number;
use crate::AppState;

/// 市区町村別ランキングの表示上限。
const MUNI_RANK_LIMIT: usize = 50;

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
