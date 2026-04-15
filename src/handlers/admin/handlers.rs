//! 管理者向けハンドラ
//!
//! 認可は lib.rs 側で require_admin ミドルウェアが処理するため、
//! ここに到達する時点で role=admin が保証されている。

use axum::extract::{Path, State};
use axum::response::Html;
use std::sync::Arc;
use tower_sessions::Session;

use super::render;
use crate::audit::dao;
use crate::AppState;

/// GET /admin/users : アカウント一覧（直近ログイン順、最大 500 件）
pub async fn admin_users_list(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    let accounts = dao::list_accounts(audit.turso(), 500);
    Html(render::users_list_page(&accounts))
}

/// GET /admin/users/{account_id} : 顧客詳細
/// - プロフィール
/// - ログイン履歴 (直近 100 件)
/// - 操作履歴    (直近 200 件)
pub async fn admin_user_detail(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Path(account_id): Path<String>,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    let Some(acc) = dao::find_account_by_id(audit.turso(), &account_id) else {
        return Html(render::not_found(&account_id));
    };
    let sessions = dao::list_sessions_for_account(audit.turso(), &account_id, 100);
    let activities = dao::list_activity_for_account(audit.turso(), &account_id, 200);
    Html(render::user_detail_page(&acc, &sessions, &activities))
}

/// GET /admin/login-failures : 最近の失敗ログ (最大 200 件)
pub async fn admin_login_failures(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    let failures = dao::list_recent_failures(audit.turso(), 200);
    Html(render::login_failures_page(&failures))
}
