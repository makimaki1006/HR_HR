use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::{fetch, render};
use crate::AppState;

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
}

#[derive(Deserialize)]
pub struct BulkCsvQuery {
    /// カンマ区切りの corporate_number（最大100件）
    #[serde(default)]
    pub corps: String,
}

/// タブ: 企業分析（検索ボックスを表示）
pub async fn tab_company(State(_state): State<Arc<AppState>>, _session: Session) -> Html<String> {
    Html(render::render_search_page())
}

/// API: 企業名タイプアヘッド検索
pub async fn company_search(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<SearchQuery>,
) -> Html<String> {
    let query = params.q.trim().to_string();
    if query.len() < 2 {
        return Html(
            r#"<p class="text-slate-500 text-sm py-2">2文字以上入力してください</p>"#.to_string(),
        );
    }

    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => {
            return Html(
                r#"<p class="text-slate-500 text-sm py-2">企業データベース未接続</p>"#.to_string(),
            );
        }
    };

    let results = tokio::task::spawn_blocking(move || fetch::search_companies(&sn_db, &query))
        .await
        .unwrap_or_default();

    Html(render::render_search_results(&results))
}

/// API: 企業プロフィール（HTMXパーシャル）
/// キャッシュ: build_company_context は 15+ 個の Turso/SQLite クエリを直列実行するため
/// 初回生成に 30〜100秒かかるケースがある。同じ corporate_number への再アクセスは
/// 高速化のため生成済み HTML を AppCache に 15分 TTL で保持する。
pub async fn company_profile(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(corporate_number): Path<String>,
) -> Html<String> {
    // 監査: 企業プロフィール閲覧を記録
    crate::audit::record_event(
        &state.audit,
        &session,
        "view_company_profile",
        "company",
        &corporate_number,
        "",
    )
    .await;

    let cache_key = format!("company_profile_html_{}", corporate_number);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(s) = cached.as_str() {
            return Html(s.to_string());
        }
    }

    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => {
            return Html(r#"<p class="text-red-400">企業データベース未接続</p>"#.to_string());
        }
    };

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => {
            return Html(
                r#"<p class="text-red-400">求人データベースが読み込まれていません</p>"#.to_string(),
            );
        }
    };

    let ext_db = state.turso_db.clone();
    let corp = corporate_number.clone();
    let ctx = tokio::task::spawn_blocking(move || {
        fetch::build_company_context(&sn_db, ext_db.as_ref(), &db, &corp)
    })
    .await
    .unwrap_or(None);

    let html = match ctx {
        Some(ctx) => render::render_company_profile(&ctx),
        None => format!(
            r#"<div class="stat-card"><p class="text-slate-400 text-center py-8">企業が見つかりません: {}</p></div>"#,
            crate::handlers::helpers::escape_html(&corporate_number)
        ),
    };
    state
        .cache
        .set(cache_key, serde_json::Value::String(html.clone()));
    Html(html)
}

/// レポート: 印刷用フルHTML
pub async fn company_report(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Path(corporate_number): Path<String>,
) -> Html<String> {
    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => {
            return Html("<html><body><h1>企業データベース未接続</h1></body></html>".to_string());
        }
    };

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => {
            return Html(
                "<html><body><h1>求人データベースが読み込まれていません</h1></body></html>"
                    .to_string(),
            );
        }
    };

    let ext_db = state.turso_db.clone();
    let corp = corporate_number.clone();
    let ctx = tokio::task::spawn_blocking(move || {
        fetch::build_company_context(&sn_db, ext_db.as_ref(), &db, &corp)
    })
    .await
    .unwrap_or(None);

    match ctx {
        Some(ctx) => Html(render::render_company_report(&ctx)),
        None => Html(format!(
            "<html><body><h1>企業が見つかりません: {}</h1></body></html>",
            crate::handlers::helpers::escape_html(&corporate_number)
        )),
    }
}

/// API: 選択した企業のサマリーCSVダウンロード（複数比較用）
/// 最大100件までの corporate_number を受け取り、SalesNow基本情報をCSV化する。
/// build_company_context は使わず単一Tursoクエリで取得（高速）。
pub async fn bulk_csv(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(query): Query<BulkCsvQuery>,
) -> Response {
    let corps_raw = query.corps.trim();
    if corps_raw.is_empty() {
        return (StatusCode::BAD_REQUEST, "corps is required").into_response();
    }

    // corporate_number は数字のみ想定。安全のため英数字のみに絞る。
    let corp_list: Vec<String> = corps_raw
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty() && s.chars().all(|c| c.is_ascii_alphanumeric()))
        .take(100)
        .collect();
    if corp_list.is_empty() {
        return (StatusCode::BAD_REQUEST, "no valid corps").into_response();
    }

    // 監査: CSVダウンロードを記録 (何社DLしたか target_id に)
    crate::audit::record_event(
        &state.audit,
        &session,
        "download_csv",
        "csv",
        &format!("{}社", corp_list.len()),
        "",
    )
    .await;

    let sn_db = match &state.salesnow_db {
        Some(t) => t.clone(),
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "企業DB未接続").into_response();
        }
    };

    let corp_list_clone = corp_list.clone();
    let rows = tokio::task::spawn_blocking(move || {
        // IN句生成: ?1, ?2, ... （パラメータ化で安全）
        let placeholders: Vec<String> = (1..=corp_list_clone.len())
            .map(|i| format!("?{i}"))
            .collect();
        let sql = format!(
            "SELECT corporate_number, company_name, prefecture, address, \
             sn_industry, employee_count, employee_delta_1y, employee_delta_3m, \
             employee_delta_1m, sales_range, capital_stock_range, credit_score, \
             established_date, listing_category, company_url, tob_toc \
             FROM v2_salesnow_companies WHERE corporate_number IN ({})",
            placeholders.join(",")
        );
        let params: Vec<Box<dyn crate::db::turso_http::ToSqlTurso>> = corp_list_clone
            .iter()
            .map(|s| Box::new(s.clone()) as Box<dyn crate::db::turso_http::ToSqlTurso>)
            .collect();
        let param_refs: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            params.iter().map(|p| p.as_ref()).collect();
        sn_db.query(&sql, &param_refs).unwrap_or_default()
    })
    .await
    .unwrap_or_default();

    // UTF-8 BOM 付きCSV（Excel互換）
    use crate::handlers::helpers::{get_f64, get_i64, get_str};
    let mut csv = String::from("\u{FEFF}");
    csv.push_str("法人番号,企業名,都道府県,住所,業種,従業員数,従業員増減率(1年),従業員増減率(3ヶ月),従業員増減率(1ヶ月),売上レンジ,資本金レンジ,信用スコア,設立年月日,上場区分,URL,BtoB/BtoC\n");
    for r in rows.iter() {
        let line = format!(
            "\"{}\",\"{}\",\"{}\",\"{}\",\"{}\",{},{:.1},{:.1},{:.1},\"{}\",\"{}\",{:.0},\"{}\",\"{}\",\"{}\",\"{}\"\n",
            csv_esc(&get_str(r, "corporate_number")),
            csv_esc(&get_str(r, "company_name")),
            csv_esc(&get_str(r, "prefecture")),
            csv_esc(&get_str(r, "address")),
            csv_esc(&get_str(r, "sn_industry")),
            get_i64(r, "employee_count"),
            get_f64(r, "employee_delta_1y"),
            get_f64(r, "employee_delta_3m"),
            get_f64(r, "employee_delta_1m"),
            csv_esc(&get_str(r, "sales_range")),
            csv_esc(&get_str(r, "capital_stock_range")),
            get_f64(r, "credit_score"),
            csv_esc(&get_str(r, "established_date")),
            csv_esc(&get_str(r, "listing_category")),
            csv_esc(&get_str(r, "company_url")),
            csv_esc(&get_str(r, "tob_toc")),
        );
        csv.push_str(&line);
    }

    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        "text/csv; charset=utf-8".parse().unwrap(),
    );
    headers.insert(
        header::CONTENT_DISPOSITION,
        "attachment; filename=\"companies_compare.csv\""
            .parse()
            .unwrap(),
    );
    (headers, csv).into_response()
}

/// CSVセル内のダブルクォートをエスケープ（"  → "" ）
fn csv_esc(s: &str) -> String {
    s.replace('"', "\"\"")
}
