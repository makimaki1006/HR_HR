use axum::extract::{Path, Query, State};
use axum::response::Html;
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
pub async fn company_profile(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Path(corporate_number): Path<String>,
) -> Html<String> {
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

    match ctx {
        Some(ctx) => Html(render::render_company_profile(&ctx)),
        None => Html(format!(
            r#"<div class="stat-card"><p class="text-slate-400 text-center py-8">企業が見つかりません: {}</p></div>"#,
            crate::handlers::helpers::escape_html(&corporate_number)
        )),
    }
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
