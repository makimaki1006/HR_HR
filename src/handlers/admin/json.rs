//! `/api/admin/*` : React 管理画面 (`/app/admin`) 向けの JSON (W8、計画 §2.8 手順 4)。
//!
//! lib.rs の `admin_routes` に登録するので、HTML と同じく auth_middleware + require_admin_mw の内側。
//! 一般ユーザーは 403、未ログインは 303 /login (W2 で 401 JSON に変わる予定)。
//! 読み取りのみ。書き込み系の呼び出しは無い。

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;

use super::data;
use super::handlers::UsageQuery;
use crate::AppState;

/// require_admin_mw が先に 403 を返すので通常は到達しない (防御的)。
fn audit_unavailable() -> Response {
    (
        StatusCode::SERVICE_UNAVAILABLE,
        Json(serde_json::json!({"error": "監査DB未接続 (AUDIT_TURSO_URL / AUDIT_TURSO_TOKEN)"})),
    )
        .into_response()
}

/// GET /api/admin/users
pub async fn api_users(State(state): State<Arc<AppState>>) -> Response {
    let Some(audit) = &state.audit else {
        return audit_unavailable();
    };
    Json(data::load_users(audit).await).into_response()
}

/// GET /api/admin/users/{account_id} (無ければ 404 + `{"error":...}`)
pub async fn api_user_detail(
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
) -> Response {
    let Some(audit) = &state.audit else {
        return audit_unavailable();
    };
    match data::load_user_detail(audit, &account_id).await {
        Some(detail) => Json(detail).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(serde_json::json!({"error": "account not found", "account_id": account_id})),
        )
            .into_response(),
    }
}

/// GET /api/admin/login-failures
pub async fn api_login_failures(State(state): State<Arc<AppState>>) -> Response {
    let Some(audit) = &state.audit else {
        return audit_unavailable();
    };
    Json(data::load_login_failures(audit).await).into_response()
}

/// GET /api/admin/usage?days=30
pub async fn api_usage(
    State(state): State<Arc<AppState>>,
    Query(q): Query<UsageQuery>,
) -> Response {
    let Some(audit) = &state.audit else {
        return audit_unavailable();
    };
    Json(data::load_usage(audit, data::clamp_days(q.days)).await).into_response()
}
