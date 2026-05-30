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
    // AUDIT E P0-1: reqwest::blocking を tokio worker thread でブロックしないよう
    // spawn_blocking で別スレッド実行 (src/handlers/CLAUDE.md §2.3 準拠)
    let audit_clone = audit.clone();
    let accounts =
        match tokio::task::spawn_blocking(move || dao::list_accounts(audit_clone.turso(), 500))
            .await
        {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!("admin_users_list spawn_blocking join failed: {e}");
                Vec::new()
            }
        };
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
    // AUDIT E P0-1: 3 つの blocking DAO 呼出を 1 度の spawn_blocking にまとめる
    // (semantics 同一: 同期的に順次実行されるので合算 IO は変化なし)
    let audit_clone = audit.clone();
    let aid_clone = account_id.clone();
    let triple = tokio::task::spawn_blocking(move || {
        let acc = dao::find_account_by_id(audit_clone.turso(), &aid_clone);
        let sessions = dao::list_sessions_for_account(audit_clone.turso(), &aid_clone, 100);
        let activities = dao::list_activity_for_account(audit_clone.turso(), &aid_clone, 200);
        (acc, sessions, activities)
    })
    .await;
    let (acc_opt, sessions, activities) = match triple {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("admin_user_detail spawn_blocking join failed: {e}");
            (None, Vec::new(), Vec::new())
        }
    };
    let Some(acc) = acc_opt else {
        return Html(render::not_found(&account_id));
    };
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
    // AUDIT E P0-1: spawn_blocking で worker thread 解放
    let audit_clone = audit.clone();
    let failures = match tokio::task::spawn_blocking(move || {
        dao::list_recent_failures(audit_clone.turso(), 200)
    })
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("admin_login_failures spawn_blocking join failed: {e}");
            Vec::new()
        }
    };
    Html(render::login_failures_page(&failures))
}
