use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use crate::handlers::overview::{format_number, get_session_filters};
use super::analysis::{calc_salary_stats, fetch_analysis, fetch_analysis_filtered};
use super::fetch::{
    count_postings, fetch_competitive, fetch_industry_raws, fetch_job_types,
    fetch_job_types_filtered, fetch_nearby_postings, fetch_postings,
    fetch_prefectures, fetch_salary_stats_sql,
};
use super::render::{
    render_analysis_html, render_analysis_html_with_scope, render_competitive,
    render_posting_table, render_report_html,
};
use super::utils::escape_html;

/// タブ8: 競合調査（ヘッダーフィルタ統合版）
pub async fn tab_competitive(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let industry_label = filters.industry_label();

    let cache_key = format!("competitive_{}", filters.industry_cache_key());
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let stats = fetch_competitive(&state, &filters);
    let pref_options = fetch_prefectures(&state, &filters);
    let ftype_options = fetch_job_types(&state, "");
    let stype_options = fetch_industry_raws(&state, "");
    let html = render_competitive(&industry_label, &stats, &pref_options, &ftype_options, &stype_options);
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// フィルタリクエストパラメータ
#[derive(Deserialize)]
pub struct CompFilterParams {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
    pub employment_type: Option<String>,
    pub service_type: Option<String>,
    pub facility_type: Option<String>,
    pub nearby: Option<bool>,
    pub radius_km: Option<f64>,
    pub page: Option<i64>,
}

/// フィルタ付き求人一覧API（HTMXパーシャル）
/// ヘッダーのjob_typeフィルタをセッションから取得
pub async fn comp_filter(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<CompFilterParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html("<p class=\"text-red-400\">ローカルDBが利用できません</p>".to_string()),
    };

    // クエリパラメータ優先、なければセッションから取得
    let pref = params.prefecture.as_deref().unwrap_or("");
    let pref = if pref.is_empty() { &filters.prefecture } else { pref };
    let muni = params.municipality.as_deref().unwrap_or("");
    let emp = params.employment_type.as_deref().unwrap_or("");
    let stype = params.service_type.as_deref().unwrap_or("");
    let ftype = params.facility_type.as_deref().unwrap_or("");
    let nearby = params.nearby.unwrap_or(false);
    let radius_km = params.radius_km.unwrap_or(10.0);
    let page = params.page.unwrap_or(1).max(1);
    let page_size: i64 = 50;

    if pref.is_empty() {
        return Html("<p class=\"text-slate-400\">都道府県を選択してください</p>".to_string());
    }

    let industry_label = filters.industry_label();
    let muni_opt = if muni.is_empty() { None } else { Some(muni) };

    // 近隣検索はhaversineフィルタがあるためRust側でページネーション
    if nearby && !muni.is_empty() {
        let postings = fetch_nearby_postings(db, &filters, pref, muni, radius_km, emp);
        let total = postings.len() as i64;
        let total_pages = if total == 0 { 1 } else { (total - 1) / page_size + 1 };
        let start = ((page - 1) * page_size) as usize;
        let start = start.min(postings.len());
        let end = (start + page_size as usize).min(postings.len());
        let page_data = &postings[start..end];
        let salary_stats = calc_salary_stats(&postings);
        return render_posting_table(
            &industry_label, pref, muni, page_data, &salary_stats,
            page, total_pages, total, nearby, radius_km, emp,
        );
    }

    // 通常検索: SQLレベルのページネーション + SQL給与統計
    let total = count_postings(db, &filters, pref, muni_opt, emp, stype, ftype);
    let total_pages = if total == 0 { 1 } else { (total - 1) / page_size + 1 };
    let postings = fetch_postings(db, &filters, pref, muni_opt, emp, stype, ftype, Some(page), Some(page_size));
    let salary_stats = fetch_salary_stats_sql(db, &filters, pref, muni_opt, emp, stype, ftype);

    render_posting_table(
        &industry_label, pref, muni, &postings, &salary_stats,
        page, total_pages, total, nearby, radius_km, emp,
    )
}

/// 市区町村一覧API / ドロップダウン絞り込み共通パラメータ
#[derive(Deserialize)]
pub struct MuniParams {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
}

pub async fn comp_municipalities(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<MuniParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let pref = params.prefecture.as_deref().unwrap_or("");
    if pref.is_empty() {
        return Html(r#"<option value="">市区町村</option>"#.to_string());
    }

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(r#"<option value="">市区町村</option>"#.to_string()),
    };

    let mut sql = "SELECT DISTINCT municipality FROM postings WHERE prefecture = ?".to_string();
    let mut param_values: Vec<String> = vec![pref.to_string()];
    filters.append_industry_filter_str(&mut sql, &mut param_values);
    sql.push_str(" ORDER BY municipality");

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &params_ref).unwrap_or_default();

    let mut html = String::from(r#"<option value="">全て</option>"#);
    for row in &rows {
        let m = row.get("municipality")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        if !m.is_empty() {
            html.push_str(&format!(r#"<option value="{m}">{m}</option>"#));
        }
    }
    Html(html)
}

/// 事業所形態一覧API（job_typeのチェックボックスHTMLを返す）
pub async fn comp_facility_types(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<MuniParams>,
) -> Html<String> {
    let pref = params.prefecture.as_deref().unwrap_or("");
    let muni = params.municipality.as_deref().unwrap_or("");
    let job_types = fetch_job_types_filtered(&state, pref, muni);

    if job_types.is_empty() {
        return Html(r#"<div class="text-sm text-slate-400 p-2">データがありません</div>"#.to_string());
    }

    let mut html = String::new();
    for (i, (jt, cnt)) in job_types.iter().enumerate() {
        let raw = jt.replace('"', "&quot;");
        let disp = escape_html(jt);
        html.push_str(&format!(
            r#"<label class="flex items-center gap-2 py-1 px-2 hover:bg-slate-700 rounded cursor-pointer">
                <input type="checkbox" class="ftype-major-cb rounded" value="{raw}" data-group="g{i}"
                    onchange="onMajorToggle(this)">
                <span class="text-sm text-white flex-1">{disp}</span>
                <span class="text-xs text-slate-400">{cnt_s}</span>
            </label>"#,
            cnt_s = format_number(*cnt),
        ));
    }

    Html(html)
}

/// 産業分類一覧API（industry_rawのDISTINCT値を返す）
pub async fn comp_service_types(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<MuniParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(r#"<option value="">全て</option>"#.to_string()),
    };

    let pref = params.prefecture.as_deref().unwrap_or("");
    let muni = params.municipality.as_deref().unwrap_or("");
    let mut sql = "SELECT industry_raw, COUNT(*) as cnt FROM postings WHERE length(industry_raw) > 0".to_string();
    let mut param_values: Vec<String> = Vec::new();

    if !pref.is_empty() {
        sql.push_str(" AND prefecture = ?");
        param_values.push(pref.to_string());
    }
    if !muni.is_empty() {
        sql.push_str(" AND municipality = ?");
        param_values.push(muni.to_string());
    }
    filters.append_industry_filter_str(&mut sql, &mut param_values);
    sql.push_str(" GROUP BY industry_raw ORDER BY cnt DESC");

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &params_ref).unwrap_or_default();

    let mut html = String::from(r#"<option value="">全て</option>"#);
    for row in &rows {
        let name = row.get("industry_raw").and_then(|v| v.as_str()).unwrap_or("");
        let cnt = row.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
        if !name.is_empty() {
            // valueにはraw値を使い、表示名のみエスケープ
            html.push_str(&format!(
                r#"<option value="{raw}">{disp} ({cnt_s})</option>"#,
                raw = name.replace('"', "&quot;"),
                disp = escape_html(name),
                cnt_s = format_number(cnt),
            ));
        }
    }
    Html(html)
}

/// 求人データ分析API（HTMXパーシャル）
pub async fn comp_analysis(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let industry_label = filters.industry_label();

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html("<p class=\"text-red-400\">ローカルDBが利用できません</p>".to_string()),
    };

    let analysis = fetch_analysis(db, &industry_label);
    Html(render_analysis_html(&industry_label, &analysis))
}

/// 都道府県指定の分析API
#[derive(Deserialize)]
pub struct AnalysisParams {
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
}

pub async fn comp_analysis_filtered(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<AnalysisParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let industry_label = filters.industry_label();

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html("<p class=\"text-red-400\">ローカルDBが利用できません</p>".to_string()),
    };

    let pref = params.prefecture.as_deref().unwrap_or("");
    let muni = params.municipality.as_deref().unwrap_or("");
    let analysis = fetch_analysis_filtered(db, &industry_label, pref, muni);
    let scope_label = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };
    Html(render_analysis_html_with_scope(&industry_label, &scope_label, &analysis))
}

/// HTMLレポート生成API
pub async fn comp_report(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(params): Query<CompFilterParams>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let industry_label = filters.industry_label();

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html("<p>ローカルDBが利用できません</p>".to_string()),
    };

    let pref = params.prefecture.as_deref().unwrap_or("");
    let muni = params.municipality.as_deref().unwrap_or("");
    let emp = params.employment_type.as_deref().unwrap_or("");
    let nearby = params.nearby.unwrap_or(false);
    let radius_km = params.radius_km.unwrap_or(10.0);

    if pref.is_empty() {
        return Html("<p>都道府県を選択してください</p>".to_string());
    }

    let postings = if nearby && !muni.is_empty() {
        fetch_nearby_postings(db, &filters, pref, muni, radius_km, emp)
    } else {
        fetch_postings(db, &filters, pref, if muni.is_empty() { None } else { Some(muni) }, emp, "", "", None, None)
    };

    let stats = calc_salary_stats(&postings);
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();

    render_report_html(&industry_label, pref, muni, emp, &postings, &stats, &today, nearby, radius_km)
}
