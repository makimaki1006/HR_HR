//! 公開ハンドラー（tab_analysis, analysis_subtab）

use axum::extract::{Path, State};
use axum::response::Html;
use serde_json::Value;
use std::sync::Arc;
use tower_sessions::Session;

use super::super::overview::{get_session_filters, make_location_label, render_no_db_data};
use super::helpers::ANALYSIS_SUBTABS;
use super::render::{
    render_subtab_1, render_subtab_2, render_subtab_3, render_subtab_4, render_subtab_5,
    render_subtab_6,
};
use crate::AppState;

/// HTMXパーシャル: V2独自分析（サブタブナビゲーション付き）
pub async fn tab_analysis(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(render_no_db_data("雇用形態別分析")),
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();
    let location = make_location_label(&pref, &muni);
    let industry = filters.industry_label();

    // サブタブ1のコンテンツを初期表示（spawn_blockingで実行）
    let pref2 = pref.clone();
    let muni2 = muni.clone();
    let subtab1_content = tokio::task::spawn_blocking(move || render_subtab_1(&db, &pref2, &muni2))
        .await
        .unwrap_or_else(|_| render_no_db_data("雇用形態別分析"));

    let mut html = String::with_capacity(16_000);

    html.push_str(&format!(
        r#"<div class="space-y-4">
        <h2 class="text-xl font-bold text-white">詳細分析 <span class="text-blue-400 text-base font-normal">{location} {industry}</span></h2>"#
    ));

    // グループナビゲーション（構造分析 / トレンド / 総合診断）
    html.push_str(r##"
        <div class="flex gap-2 mb-2">
            <button class="analysis-group active" onclick="setAnalysisGroup(this)"
                hx-get="/tab/analysis?group_only=1" hx-target="#analysis-group-content" hx-swap="innerHTML">構造分析</button>
            <button class="analysis-group" onclick="setAnalysisGroup(this)"
                hx-get="/tab/trend" hx-target="#analysis-group-content" hx-swap="innerHTML">トレンド</button>
            <button class="analysis-group" onclick="setAnalysisGroup(this)"
                hx-get="/tab/insight" hx-target="#analysis-group-content" hx-swap="innerHTML">総合診断</button>
        </div>
        <div id="analysis-group-content">
    "##);

    // グループA: 構造分析 サブタブナビゲーション
    html.push_str(
        r##"<p class="text-xs text-slate-500 mb-2">正社員/パートで分けた求人市場の構造指標</p>"##,
    );
    html.push_str(r##"<div class="flex gap-1 mb-4 border-b border-slate-700 overflow-x-auto">"##);
    for (id, label) in &ANALYSIS_SUBTABS {
        let active = if *id == 1 { " active" } else { "" };
        html.push_str(&format!(
            r##"<button class="analysis-subtab{active}" hx-get="/api/analysis/subtab/{id}" hx-target="#analysis-content" hx-swap="innerHTML" onclick="setAnalysisSubtab(this)">{label}</button>"##
        ));
    }
    html.push_str("</div>");

    // サブタブコンテンツ領域（初期はサブタブ1を表示）
    html.push_str(r##"<div id="analysis-content">"##);
    html.push_str(&subtab1_content);
    html.push_str("</div>");

    html.push_str("</div>"); // analysis-group-content

    // グループ切替+サブタブ切替用JS
    html.push_str(
        r#"<script>
function setAnalysisGroup(el) {
    document.querySelectorAll('.analysis-group').forEach(function(btn) {
        btn.classList.remove('active');
    });
    el.classList.add('active');
}
function setAnalysisSubtab(el) {
    document.querySelectorAll('.analysis-subtab').forEach(function(btn) {
        btn.classList.remove('active');
    });
    el.classList.add('active');
}
</script>"#,
    );

    html.push_str(r##"<div hx-get="/api/insight/widget/analysis" hx-trigger="load" hx-swap="innerHTML"></div>"##);
    html.push_str("</div>");

    Html(html)
}

/// サブタブAPIハンドラー（HTMX経由で呼ばれる）
pub async fn analysis_subtab(
    State(state): State<Arc<AppState>>,
    session: Session,
    Path(id): Path<u8>,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => {
            return Html(
                r#"<p class="text-slate-500 text-sm p-4">データベース未接続</p>"#.to_string(),
            )
        }
    };

    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();

    let cache_key = format!(
        "v2analysis_sub{}_{}_{}_{}",
        id,
        filters.industry_cache_key(),
        filters.prefecture,
        filters.municipality
    );
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let turso_db = state.turso_db.clone();
    let content = tokio::task::spawn_blocking(move || match id {
        1 => render_subtab_1(&db, &pref, &muni),
        2 => render_subtab_2(&db, &pref, &muni),
        3 => render_subtab_3(&db, &pref, &muni),
        4 => render_subtab_4(&db, &pref, &muni),
        5 => render_subtab_5(&db, turso_db.as_ref(), &pref, &muni),
        6 => render_subtab_6(&db, &pref, &muni),
        7 => super::render::render_subtab_7(&db, turso_db.as_ref(), &pref, &muni),
        _ => r#"<p class="text-slate-500 text-sm p-4">不明なサブタブです</p>"#.to_string(),
    })
    .await
    .unwrap_or_else(|_| r#"<p class="text-slate-500 text-sm p-4">処理エラー</p>"#.to_string());

    state.cache.set(cache_key, Value::String(content.clone()));
    Html(content)
}
