//! 資格カルテタブ Axum handler
//!
//! ルート:
//!   GET /tab/license              : 資格一覧（五十音順、検索可能）
//!   GET /tab/license/{name}       : 資格詳細（URL エンコードされた資格名を Path で受け取る）
//!
//! データ出典:
//!   * 職業情報データベース 資格情報 ver.7.01（JILPT）
//!   * 賃金構造基本統計調査 令和7年 表5（厚生労働省、e-Stat 00450091）

use askama::Template;
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse},
    routing::get,
    Router,
};
use std::sync::Arc;
use tracing::error;

use crate::AppState;

pub mod data;

use data::{fetch_license_detail, fetch_license_summaries, LicenseDetail, LicenseSummary};

/// license タブのルーターを公開する。
///
/// `build_app()` の `protected_routes` チェーンに以下のように組み込む:
///   `.merge(handlers::license::router())`
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/tab/license", get(tab_license_index))
        .route("/tab/license/{name}", get(tab_license_detail))
}

// =========================================================================
// テンプレート構造体（Askama）
// =========================================================================

#[derive(Template)]
#[template(path = "tabs/license_index.html")]
pub struct LicenseIndexPage {
    pub summaries: Vec<LicenseSummary>,
}

#[derive(Template)]
#[template(path = "tabs/license_detail.html")]
pub struct LicenseDetailPage {
    pub detail: LicenseDetail,
    pub category_dist_json: String,
}

// =========================================================================
// 一覧タブ HTML
// =========================================================================

pub async fn tab_license_index(State(state): State<Arc<AppState>>) -> impl IntoResponse {
    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded(
                "Turso country-statistics 未接続のため資格カルテを表示できません",
            )
            .into_response();
        }
    };

    let summaries = match tokio::task::spawn_blocking(move || fetch_license_summaries(&turso)).await
    {
        Ok(Ok(v)) => v,
        Ok(Err(e)) => {
            error!("fetch_license_summaries failed: {e}");
            return render_degraded("資格一覧の取得に失敗しました").into_response();
        }
        Err(e) => {
            error!("spawn_blocking failed (license index): {e}");
            return render_degraded("内部エラー").into_response();
        }
    };

    let page = LicenseIndexPage { summaries };
    match page.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            error!("Askama render failed (license index): {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
        }
    }
}

// =========================================================================
// 詳細ページ HTML
// =========================================================================

pub async fn tab_license_detail(
    State(state): State<Arc<AppState>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    // Axum が URL デコードした文字列を受け取る。長さ上限のみ検証（XSS対策はテンプレ側 escape）
    if name.chars().count() > 200 {
        return (StatusCode::BAD_REQUEST, "資格名が長すぎます").into_response();
    }

    let turso = match state.turso_db.clone() {
        Some(db) => db,
        None => {
            return render_degraded(
                "Turso country-statistics 未接続のため資格カルテを表示できません",
            )
            .into_response();
        }
    };

    let name_clone = name.clone();
    let detail = match tokio::task::spawn_blocking(move || {
        fetch_license_detail(&turso, &name_clone)
    })
    .await
    {
        Ok(Ok(Some(d))) => d,
        Ok(Ok(None)) => {
            let escaped = html_escape(&name);
            let body = format!(
                "<div class=\"p-6 bg-yellow-900/30 border border-yellow-600 rounded text-yellow-200\">\
                    <h2 class=\"text-lg font-bold mb-2\">資格カルテ</h2>\
                    <p>「{escaped}」は未登録です。</p>\
                    <a href=\"/tab/license\" \
                       hx-get=\"/tab/license\" hx-target=\"#content\" hx-swap=\"innerHTML\" \
                       class=\"inline-block mt-3 text-blue-300 underline\">&#x2190; 一覧に戻る</a>\
                </div>"
            );
            return (StatusCode::NOT_FOUND, Html(body)).into_response();
        }
        Ok(Err(e)) => {
            error!("fetch_license_detail({name}) failed: {e}");
            return render_degraded("資格詳細の取得に失敗しました").into_response();
        }
        Err(e) => {
            error!("spawn_blocking failed (license detail): {e}");
            return render_degraded("内部エラー").into_response();
        }
    };

    // ECharts 用カテゴリ分布 JSON
    let category_dist_json =
        serde_json::to_string(&detail.category_distribution).unwrap_or_else(|_| "[]".into());

    let page = LicenseDetailPage {
        detail,
        category_dist_json,
    };
    match page.render() {
        Ok(body) => Html(body).into_response(),
        Err(e) => {
            error!("Askama render failed (license detail): {e}");
            (StatusCode::INTERNAL_SERVER_ERROR, "render failed").into_response()
        }
    }
}

// =========================================================================
// degraded 表示（Turso 未接続時など）
// =========================================================================

fn render_degraded(msg: &str) -> Html<String> {
    let safe = html_escape(msg);
    Html(format!(
        r#"<div class="p-6 bg-yellow-900/30 border border-yellow-600 rounded text-yellow-200">
            <h2 class="text-lg font-bold mb-2">資格カルテ — 表示できません</h2>
            <p>{safe}</p>
        </div>"#
    ))
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}
