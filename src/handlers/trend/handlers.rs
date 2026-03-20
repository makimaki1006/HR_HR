//! 公開ハンドラー（tab_trend, trend_subtab）

use axum::extract::{Path, State};
use axum::response::Html;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::super::overview::{get_session_filters, make_location_label};
use super::helpers::TREND_SUBTABS;
use super::render::{
    render_subtab_1, render_subtab_2, render_subtab_3, render_subtab_4,
};

/// HTMXパーシャル: 時系列トレンド分析（サブタブナビゲーション付き）
pub async fn tab_trend(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let pref = filters.prefecture.clone();
    let location = make_location_label(&pref, "");

    // サブタブ1のコンテンツを初期表示
    let turso_db = state.turso_db.clone();
    let pref2 = pref.clone();
    let subtab1_content = tokio::task::spawn_blocking(move || {
        render_subtab_1(turso_db.as_ref(), &pref2)
    }).await.unwrap_or_else(|_| r#"<p class="text-slate-500 text-sm">処理エラー</p>"#.to_string());

    let mut html = String::with_capacity(8_000);

    html.push_str(&format!(
        r#"<div class="space-y-4">
        <h2 class="text-xl font-bold text-white">時系列トレンド分析 <span class="text-blue-400 text-base font-normal">{location}</span></h2>
        <p class="text-xs text-slate-500">HW過去データ（20月次スナップショット）の時系列推移。都道府県フィルタのみ対応。</p>"#
    ));

    // サブタブナビゲーションバー
    html.push_str(r#"<div class="flex gap-1 mb-4 border-b border-slate-700 overflow-x-auto">"#);
    for (id, label) in &TREND_SUBTABS {
        let active = if *id == 1 { " active" } else { "" };
        html.push_str(&format!(
            r##"<button class="analysis-subtab{active}" hx-get="/api/trend/subtab/{id}" hx-target="#trend-content" hx-swap="innerHTML" onclick="setTrendSubtab(this)">{label}</button>"##
        ));
    }
    html.push_str("</div>");

    // サブタブコンテンツ領域
    html.push_str(r##"<div id="trend-content">"##);
    html.push_str(&subtab1_content);
    html.push_str("</div>");

    // サブタブ切替用JS
    html.push_str(r#"<script>
function setTrendSubtab(el) {
    el.closest('.flex').querySelectorAll('.analysis-subtab').forEach(function(btn) {
        btn.classList.remove('active');
    });
    el.classList.add('active');
}
</script>"#);

    html.push_str("</div>");

    Html(html)
}

/// サブタブAPIハンドラー（HTMX経由）
pub async fn trend_subtab(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<u8>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let pref = filters.prefecture.clone();

    let cache_key = format!("trend_sub{}_{}", id, pref);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso_db = state.turso_db.clone();
    let content = tokio::task::spawn_blocking(move || {
        match id {
            1 => render_subtab_1(turso_db.as_ref(), &pref),
            2 => render_subtab_2(turso_db.as_ref(), &pref),
            3 => render_subtab_3(turso_db.as_ref(), &pref),
            4 => render_subtab_4(turso_db.as_ref(), &pref),
            _ => r#"<p class="text-slate-500 text-sm p-4">不明なサブタブです</p>"#.to_string(),
        }
    }).await.unwrap_or_else(|_| r#"<p class="text-slate-500 text-sm p-4">処理エラー</p>"#.to_string());

    state.cache.set(cache_key, Value::String(content.clone()));
    Html(content)
}
