//! ユーザー自己サービス用ハンドラ

use axum::extract::State;
use axum::response::{Html, IntoResponse, Redirect};
use axum::Form;
use serde::Deserialize;
use std::sync::Arc;
use tower_sessions::Session;

use super::render;
use crate::audit::dao;
use crate::AppState;

async fn current_account_id(session: &Session) -> Option<String> {
    session
        .get(crate::SESSION_ACCOUNT_ID_KEY)
        .await
        .unwrap_or(None)
}

/// GET /my/profile
pub async fn my_profile_get(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::audit_disabled_page());
    };
    let Some(aid) = current_account_id(&session).await else {
        return Html(render::not_linked_page());
    };
    let Some(acc) = dao::find_account_by_id(audit.turso(), &aid) else {
        return Html(render::not_linked_page());
    };
    Html(render::profile_page(&acc, None))
}

#[derive(Deserialize)]
pub struct ProfileForm {
    #[serde(default)]
    pub display_name: String,
    #[serde(default)]
    pub company: String,
}

/// POST /my/profile
pub async fn my_profile_post(
    State(state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<ProfileForm>,
) -> impl IntoResponse {
    let Some(audit) = &state.audit else {
        return Html(render::audit_disabled_page()).into_response();
    };
    let Some(aid) = current_account_id(&session).await else {
        return Redirect::to("/login").into_response();
    };
    // 長さ制限（表示崩れ防止）
    let name = form.display_name.chars().take(80).collect::<String>();
    let company = form.company.chars().take(120).collect::<String>();
    let _ = dao::update_profile(audit.turso(), &aid, &name, &company);

    // 監査: プロフィール更新を記録
    crate::audit::record_event(
        &state.audit,
        &session,
        "update_profile",
        "account",
        &aid,
        "",
    )
    .await;

    // 更新後のプロフィール取得して再表示
    if let Some(acc) = dao::find_account_by_id(audit.turso(), &aid) {
        Html(render::profile_page(
            &acc,
            Some("プロフィールを更新しました"),
        ))
        .into_response()
    } else {
        Html(render::not_linked_page()).into_response()
    }
}

/// GET /my/activity
pub async fn my_activity(State(state): State<Arc<AppState>>, session: Session) -> Html<String> {
    let Some(audit) = &state.audit else {
        return Html(render::audit_disabled_page());
    };
    let Some(aid) = current_account_id(&session).await else {
        return Html(render::not_linked_page());
    };
    let Some(acc) = dao::find_account_by_id(audit.turso(), &aid) else {
        return Html(render::not_linked_page());
    };
    let sessions = dao::list_sessions_for_account(audit.turso(), &aid, 50);
    let activities = dao::list_activity_for_account(audit.turso(), &aid, 100);
    Html(render::activity_page(&acc, &sessions, &activities))
}
