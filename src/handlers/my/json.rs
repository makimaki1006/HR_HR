//! `/api/my/*` : React 個人設定画面 (`/app/my`) 向けの JSON (W8、計画 §2.8 手順 4)。
//!
//! protected_routes (auth_middleware + CSRF) の内側。応答は `status` タグ付きで、
//! 旧画面の 3 状態 (通常 / 監査無効 / アカウント未連携) をそのまま伝える。
//! POST は旧 `POST /my/profile` と同じ書き込み経路 (`data::apply_profile_update`) を通る。

use axum::extract::State;
use axum::Json;
use std::sync::Arc;
use tower_sessions::Session;

use super::data::{self, MyActivityResponse, MyProfileResponse, MyProfileUpdateRequest};
use crate::AppState;

/// GET /api/my/profile
pub async fn api_profile(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<MyProfileResponse> {
    Json(data::load_profile(&state, &session).await)
}

/// POST /api/my/profile (JSON body: `MyProfileUpdateRequest`)
pub async fn api_profile_post(
    State(state): State<Arc<AppState>>,
    session: Session,
    Json(req): Json<MyProfileUpdateRequest>,
) -> Json<MyProfileResponse> {
    Json(
        match data::apply_profile_update(&state, &session, &req).await {
            data::ProfileUpdateOutcome::AuditDisabled => MyProfileResponse::AuditDisabled,
            data::ProfileUpdateOutcome::NoAccountInSession => MyProfileResponse::NotLinked,
            data::ProfileUpdateOutcome::Updated(Some(account)) => MyProfileResponse::Ok { account },
            data::ProfileUpdateOutcome::Updated(None) => MyProfileResponse::NotLinked,
        },
    )
}

/// GET /api/my/activity
pub async fn api_activity(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Json<MyActivityResponse> {
    Json(data::load_activity(&state, &session).await)
}
