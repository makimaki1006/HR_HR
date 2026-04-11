//! 公開ハンドラー（tab_insight, insight_subtab, insight_widget, insight_report_json）

use axum::extract::{Path, State};
use axum::response::Html;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::super::overview::{get_session_filters, make_location_label, render_no_db_data};
use super::engine::generate_insights;
use super::fetch::build_insight_context;
use super::helpers::INSIGHT_SUBTABS;
use super::render::{
    render_subtab_hiring, render_subtab_forecast,
    render_subtab_regional, render_subtab_action,
    render_insight_widget_html,
};
use super::report::build_report_json;

/// HTMXパーシャル: 総合診断タブ（サブタブナビゲーション付き）
pub async fn tab_insight(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(render_no_db_data("総合診断")),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();
    let location = make_location_label(&pref, &muni);

    // キャッシュ確認
    let cache_key = format!("insight_tab_{}_{}", pref, muni);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    // サブタブ1のコンテンツを初期表示
    let turso = state.turso_db.clone();
    let pref2 = pref.clone();
    let muni2 = muni.clone();
    let subtab1_content = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref2, &muni2);
        let insights = generate_insights(&ctx);
        render_subtab_hiring(&insights)
    }).await.unwrap_or_else(|e| {
        tracing::error!("Insight tab render failed: {e}");
        render_no_db_data("総合診断")
    });

    let mut html = String::with_capacity(8_000);

    html.push_str(&format!(
        r#"<div class="space-y-4">
        <h2 class="text-xl font-bold text-white">総合診断 <span class="text-blue-400 text-base font-normal">{location}</span></h2>
        <p class="text-xs text-slate-500">全データソースを複合的に分析し、採用課題と改善アクションを示します</p>"#
    ));

    // サブタブナビゲーションバー
    html.push_str(r#"<div class="flex gap-1 mb-4 border-b border-slate-700 overflow-x-auto">"#);
    for (id, label) in &INSIGHT_SUBTABS {
        let active = if *id == 1 { " active" } else { "" };
        html.push_str(&format!(
            r##"<button class="analysis-subtab{active}" hx-get="/api/insight/subtab/{id}" hx-target="#insight-content" hx-swap="innerHTML" onclick="setInsightSubtab(this)">{label}</button>"##
        ));
    }
    html.push_str("</div>");

    // サブタブコンテンツ領域
    html.push_str(r##"<div id="insight-content">"##);
    html.push_str(&subtab1_content);
    html.push_str("</div>");

    // サブタブ切替用JS
    html.push_str(r#"<script>
function setInsightSubtab(el) {
    el.closest('.flex').querySelectorAll('.analysis-subtab').forEach(function(btn) {
        btn.classList.remove('active');
    });
    el.classList.add('active');
}
</script>"#);

    html.push_str("</div>");

    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

/// サブタブAPIハンドラー（HTMX経由）
pub async fn insight_subtab(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<u8>,
) -> Html<String> {
    // id範囲検証（1-4のみ有効、キャッシュ汚染防止）
    if !(1..=4).contains(&id) {
        return Html(r#"<p class="text-slate-500 text-sm p-4">不明なサブタブです</p>"#.to_string());
    }

    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(r#"<p class="text-slate-500 text-sm p-4">データベース未接続</p>"#.to_string()),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();

    let cache_key = format!("insight_sub{}_{}_{}", id, pref, muni);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso = state.turso_db.clone();
    let content = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref, &muni);
        let insights = generate_insights(&ctx);
        match id {
            1 => render_subtab_hiring(&insights),
            2 => render_subtab_forecast(&insights),
            3 => render_subtab_regional(&insights),
            4 => render_subtab_action(&insights),
            _ => r#"<p class="text-slate-500 text-sm p-4">不明なサブタブです</p>"#.to_string(),
        }
    }).await.unwrap_or_else(|_| r#"<p class="text-slate-500 text-sm p-4">処理エラー</p>"#.to_string());

    state.cache.set(cache_key, Value::String(content.clone()));
    Html(content)
}

/// 既存タブ用ウィジェット（HTMX遅延ロード: hx-trigger="load"）
pub async fn insight_widget(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(tab): Path<String>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(String::new()),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();

    let cache_key = format!("insight_widget_{}_{}_{}",  tab, pref, muni);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso = state.turso_db.clone();
    let content = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref, &muni);
        let insights = generate_insights(&ctx);

        // タブに関連する示唆のみ抽出（最大3件）
        let relevant: Vec<_> = insights.iter()
            .filter(|i| i.related_tabs.contains(&tab.as_str()))
            .take(3)
            .collect();

        if relevant.is_empty() { return String::new(); }
        render_insight_widget_html(&relevant)
    }).await.unwrap_or_default();

    if !content.is_empty() {
        state.cache.set(cache_key, Value::String(content.clone()));
    }
    Html(content)
}

/// レポートJSON API
pub async fn insight_report_json(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> axum::response::Json<Value> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return axum::response::Json(serde_json::json!({"error": "DB未接続"})),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();
    let turso = state.turso_db.clone();

    let report = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref, &muni);
        let insights = generate_insights(&ctx);
        build_report_json(&insights, &pref, &muni)
    }).await.unwrap_or_else(|_| serde_json::json!({"error": "処理エラー"}));

    axum::response::Json(report)
}

/// 統合レポートHTMLページ（PDF出力用、新しいタブで開く）
pub async fn insight_report_html(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html("<html><body><p>DB未接続</p></body></html>".to_string()),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();

    // キャッシュ確認（レポート生成は重いため）
    let cache_key = format!("report_html_{}_{}", pref, muni);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso = state.turso_db.clone();

    let html = tokio::task::spawn_blocking(move || {
        let ctx = build_insight_context(&db, turso.as_ref(), &pref, &muni);
        let insights = generate_insights(&ctx);
        super::render::render_insight_report_page(&insights, &ctx, &pref, &muni)
    }).await.unwrap_or_else(|e| {
        tracing::error!("Report HTML generation failed: {e}");
        "<html><body><p>レポート生成エラー</p></body></html>".to_string()
    });

    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}
