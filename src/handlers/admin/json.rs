//! `/api/admin/*` : React 管理画面 (`/app/admin`) 向けの JSON (W8、計画 §2.8 手順 4)。
//!
//! lib.rs の `admin_routes` に登録するので、HTML と同じく auth_middleware + require_admin_mw の内側。
//! 一般ユーザーは 403、未ログインは 303 /login (W2 で 401 JSON に変わる予定)。
//! 読み取りのみ。ただし `api_change_role` (役割の変更、管理者の操作) だけは `accounts.role` を書き換える。

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use std::sync::Arc;
use tower_sessions::Session;

use crate::audit::dao;
use crate::crm::rbac::{self, CrmRole};

use super::data::{self, AdminRoleChangeRequest, AdminRoleChangeResponse};
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

fn role_error(status: StatusCode, kind: &str) -> Response {
    (status, Json(serde_json::json!({ "error_kind": kind }))).into_response()
}

/// POST /api/admin/users/{account_id}/role  `{"role": "bpo"}`
///
/// 管理者 (require_admin_mw を通った人) が他人の役割を変える。書き込みは `accounts.role` の 1 列だけ。
/// - 役割は admin / consultant / bpo / user (前後の空白・大文字小文字は吸収)。それ以外は 400 `invalid_role`
/// - 自分自身は変えられない (403 `cannot_change_self`。管理者が 0 人になる事故と、降格の取り違えを防ぐ)
/// - `ADMIN_EMAILS` の人を admin 以外にはできない (409 `env_admin`。次のログインで admin に戻るため)
/// - 対象が無ければ 404 `account_not_found`
/// - 変更したら operation log (`change_role`) に残し、このプロセスの役割キャッシュを即時に捨てる
pub async fn api_change_role(
    session: Session,
    State(state): State<Arc<AppState>>,
    Path(account_id): Path<String>,
    Json(req): Json<AdminRoleChangeRequest>,
) -> Response {
    let Some(audit) = &state.audit else {
        return audit_unavailable();
    };
    let Some(new_role) = CrmRole::parse_known(&req.role) else {
        return role_error(StatusCode::BAD_REQUEST, "invalid_role");
    };
    let actor_id: Option<String> = session
        .get(crate::SESSION_ACCOUNT_ID_KEY)
        .await
        .unwrap_or(None);
    if actor_id.as_deref() == Some(account_id.as_str()) {
        return role_error(StatusCode::FORBIDDEN, "cannot_change_self");
    }
    let target = {
        let audit = audit.clone();
        let id = account_id.clone();
        tokio::task::spawn_blocking(move || dao::find_account_by_id(audit.turso(), &id))
            .await
            .unwrap_or(None)
    };
    let Some(target) = target else {
        return role_error(StatusCode::NOT_FOUND, "account_not_found");
    };
    let is_env_admin = state
        .config
        .admin_emails
        .iter()
        .any(|a| a.trim().eq_ignore_ascii_case(target.email.trim()));
    if is_env_admin && new_role != CrmRole::Admin {
        return role_error(StatusCode::CONFLICT, "env_admin");
    }
    let write = {
        let audit = audit.clone();
        let id = account_id.clone();
        let role = new_role.as_str();
        tokio::task::spawn_blocking(move || dao::update_account_role(audit.turso(), &id, role))
            .await
    };
    match write {
        Ok(Ok(())) => {}
        Ok(Err(e)) => {
            tracing::warn!("change_role failed: {e}");
            return role_error(StatusCode::BAD_GATEWAY, "audit_write_failed");
        }
        Err(e) => {
            tracing::warn!("change_role join failed: {e}");
            return role_error(StatusCode::BAD_GATEWAY, "audit_write_failed");
        }
    }
    // 次のリクエストから新しい役割で判定する (このプロセス。別プロセスはキャッシュ期限まで)
    rbac::invalidate_role(&target.email);
    let meta = serde_json::json!({
        "email": target.email,
        "from": target.role,
        "to": new_role.as_str(),
    })
    .to_string();
    crate::audit::record_event(
        &state.audit,
        &session,
        "change_role",
        "account",
        &account_id,
        &meta,
    )
    .await;
    let after = {
        let audit = audit.clone();
        let id = account_id.clone();
        tokio::task::spawn_blocking(move || dao::find_account_by_id(audit.turso(), &id))
            .await
            .unwrap_or(None)
    };
    match after {
        Some(account) => Json(AdminRoleChangeResponse {
            account,
            previous_role: target.role,
        })
        .into_response(),
        None => role_error(StatusCode::BAD_GATEWAY, "audit_write_failed"),
    }
}
