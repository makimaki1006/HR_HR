//! 管理者向けハンドラ (HTML)
//!
//! 認可は lib.rs 側で require_admin ミドルウェアが処理するため、
//! ここに到達する時点で role=admin が保証されている。
//!
//! W8 (2026-09-29): 「応答 struct を作る (`data.rs`) → render (`render.rs`)」の 2 段に分けた。
//! JSON 版 (`json.rs`、`/api/admin/*`) は同じ struct を返す。HTML の出力は分割前と同一
//! (`snapshot_tests.rs` で固定)。

use axum::extract::{Path, State};
use axum::response::Html;
use std::sync::Arc;
use tower_sessions::Session;

use super::{data, render};
use crate::AppState;

/// GET /admin/users : アカウント一覧（直近ログイン順、最大 500 件）
pub async fn admin_users_list(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    Html(render::users_list_page(&data::load_users(audit).await))
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
    match data::load_user_detail(audit, &account_id).await {
        Some(detail) => Html(render::user_detail_page(&detail)),
        None => Html(render::not_found(&account_id)),
    }
}

/// GET /admin/login-failures : 最近の失敗ログ (最大 200 件)
pub async fn admin_login_failures(
    State(state): State<Arc<AppState>>,
    _session: Session,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    Html(render::login_failures_page(
        &data::load_login_failures(audit).await,
    ))
}

/// GET /admin/usage?days=30 : 利用状況（ユーザー別 / 機能別 / クロス）
///
/// 2026-08-10 追加。ユーザー決定により、記録対象は
/// タブ切替・検索実行・レポート生成・CSV取込などの「意味のある操作」のみ。
pub async fn admin_usage(
    State(state): State<Arc<AppState>>,
    _session: Session,
    axum::extract::Query(q): axum::extract::Query<UsageQuery>,
) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::no_audit_db());
    };
    // 想定外の値で全期間スキャンにならないよう 1〜365 日に丸める
    let days = data::clamp_days(q.days);
    Html(render::usage_page(&data::load_usage(audit, days).await))
}

#[derive(Debug, serde::Deserialize)]
pub struct UsageQuery {
    pub days: Option<i64>,
}
