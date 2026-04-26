use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;

use super::overview::{get_session_filters, make_location_label, render_no_db_data};

use std::fmt::Write as _;
/// 市場概況タブ: 概況セクションを即時表示、残りは即時遅延ロード
pub async fn tab_market(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("市場概況")),
    };

    let location_label = make_location_label(&filters.prefecture, &filters.municipality);
    let industry_label = filters.industry_label();

    // 概況セクションはキャッシュ済みなら高速
    let cache_key = format!(
        "overview_{}_{}_{}",
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    let overview_html = if let Some(cached) = state.cache.get(&cache_key) {
        cached.as_str().unwrap_or("").to_string()
    } else {
        let db1 = db.clone();
        let f1 = filters.clone();
        let html =
            tokio::task::spawn_blocking(move || super::overview::build_overview_html(&db1, &f1))
                .await
                .unwrap_or_default();
        state.cache.set(cache_key, Value::String(html.clone()));
        html
    };

    // ページ全体を構築
    let mut html = String::with_capacity(overview_html.len() + 4096);

    // ヘッダー
    write!(html,
        r#"<div class="space-y-6">
        <div class="flex items-center justify-between mb-4">
            <h2 class="text-xl font-bold text-gray-100">
                市場概況 &mdash; {location} {industry}
            </h2>
        </div>"#,
        location = location_label,
        industry = if industry_label == "全産業" {
            String::new()
        } else {
            format!("({})", industry_label)
        },
    ).unwrap();

    // セクションナビ（ページ内スクロール）
    html.push_str(r##"
        <nav class="flex gap-2 flex-wrap text-xs mb-4">
            <a href="#sec-overview" class="px-3 py-1.5 bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition">概況</a>
            <a href="#sec-workstyle" class="px-3 py-1.5 bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition">雇用条件</a>
            <a href="#sec-balance" class="px-3 py-1.5 bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition">企業分析</a>
            <a href="#sec-demographics" class="px-3 py-1.5 bg-gray-700 text-gray-300 rounded hover:bg-gray-600 transition">採用動向</a>
        </nav>
    "##);

    // セクション1: 概況（同期ロード済み）
    html.push_str(r##"<section id="sec-overview">"##);
    html.push_str(&overview_html);
    html.push_str("</section>");

    // 外部統計（人口コンテキスト）- 遅延ロード
    if !filters.prefecture.is_empty() {
        html.push_str(r##"<div hx-get="/api/market/population" hx-trigger="load" hx-swap="innerHTML"></div>"##);
    }

    // セクション2: 雇用条件（即時ロード）
    html.push_str(
        r##"<section id="sec-workstyle" class="mt-8">
        <div hx-get="/api/market/workstyle" hx-trigger="load" hx-swap="innerHTML">
            <div class="flex justify-center py-8"><div class="loading-spinner"></div></div>
        </div>
    </section>"##,
    );

    // セクション3: 企業分析（即時ロード）
    html.push_str(
        r##"<section id="sec-balance" class="mt-8">
        <div hx-get="/api/market/balance" hx-trigger="load" hx-swap="innerHTML">
            <div class="flex justify-center py-8"><div class="loading-spinner"></div></div>
        </div>
    </section>"##,
    );

    // セクション4: 採用動向（即時ロード）
    html.push_str(
        r##"<section id="sec-demographics" class="mt-8">
        <div hx-get="/api/market/demographics" hx-trigger="load" hx-swap="innerHTML">
            <div class="flex justify-center py-8"><div class="loading-spinner"></div></div>
        </div>
    </section>"##,
    );

    // 関連示唆ウィジェット
    html.push_str(r##"<div hx-get="/api/insight/widget/overview" hx-trigger="load" hx-swap="innerHTML"></div>"##);

    html.push_str("</div>");

    Html(html)
}

/// 市場概況タブ用: 人口コンテキスト遅延ロードAPI
pub async fn market_population(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    if let Some(turso) = &state.turso_db {
        let turso = turso.clone();
        let pref = filters.prefecture.clone();
        let muni = filters.municipality.clone();
        let html = tokio::task::spawn_blocking(move || {
            super::overview::build_population_context_html(&turso, &pref, &muni)
        })
        .await
        .unwrap_or_default();
        Html(html)
    } else {
        Html(String::new())
    }
}

/// 市場概況タブ用: 雇用条件セクション（キャッシュ対応）
pub async fn market_workstyle(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(String::new()),
    };
    let cache_key = format!(
        "workstyle_{}_{}_{}",
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }
    let f = filters.clone();
    let html = tokio::task::spawn_blocking(move || super::workstyle::build_workstyle_html(&db, &f))
        .await
        .unwrap_or_default();
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// 市場概況タブ用: 企業分析セクション（キャッシュ対応）
pub async fn market_balance(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(String::new()),
    };
    let cache_key = format!(
        "balance_{}_{}_{}",
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }
    let f = filters.clone();
    let html = tokio::task::spawn_blocking(move || super::balance::build_balance_html(&db, &f))
        .await
        .unwrap_or_default();
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// 市場概況タブ用: 採用動向セクション（キャッシュ対応）
pub async fn market_demographics(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(String::new()),
    };
    let cache_key = format!(
        "demographics_{}_{}_{}",
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }
    let f = filters.clone();
    let html =
        tokio::task::spawn_blocking(move || super::demographics::build_demographics_html(&db, &f))
            .await
            .unwrap_or_default();
    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}
