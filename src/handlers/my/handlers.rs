//! ユーザー自己サービス用ハンドラ (HTML)
//!
//! W8 (2026-09-29): 「応答型を作る (`data.rs`) → render (`render.rs`)」の 2 段に分けた。
//! JSON 版 (`json.rs`、`/api/my/*`) は同じ型を返し、POST の書き込み経路も共通。
//! HTML の出力は分割前と同一 (`snapshot_tests.rs` で固定)。

use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use std::sync::Arc;
use tower_sessions::Session;

use super::data::{self, MyProfileUpdateRequest, ProfileUpdateOutcome};
use super::render;
use crate::AppState;

/// GET /my/profile
pub async fn my_profile_get(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    Html(render::profile_page(
        &data::load_profile(&state, &session).await,
        None,
    ))
}

/// POST /my/profile
pub async fn my_profile_post(
    State(state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<MyProfileUpdateRequest>,
) -> impl IntoResponse {
    match data::apply_profile_update(&state, &session, &form).await {
        ProfileUpdateOutcome::AuditDisabled => Html(render::audit_disabled_page()).into_response(),
        ProfileUpdateOutcome::NoAccountInSession => Redirect::to("/login").into_response(),
        ProfileUpdateOutcome::Updated(Some(account)) => Html(render::profile_page(
            &data::MyProfileResponse::Ok { account },
            Some("プロフィールを更新しました"),
        ))
        .into_response(),
        ProfileUpdateOutcome::Updated(None) => Html(render::not_linked_page()).into_response(),
    }
}

/// GET /my/activity
pub async fn my_activity(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    Html(render::activity_page(
        &data::load_activity(&state, &session).await,
    ))
}
