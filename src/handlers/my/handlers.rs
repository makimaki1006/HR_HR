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
    // AUDIT E P0-1: spawn_blocking で worker thread 解放
    let audit_clone = audit.clone();
    let aid_clone = aid.clone();
    let acc_opt = match tokio::task::spawn_blocking(move || {
        dao::find_account_by_id(audit_clone.turso(), &aid_clone)
    })
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("my_profile_get spawn_blocking join failed: {e}");
            None
        }
    };
    let Some(acc) = acc_opt else {
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
    // AUDIT E P0-1: spawn_blocking で worker thread 解放
    {
        let audit_clone = audit.clone();
        let aid_owned = aid.clone();
        let name_owned = name.clone();
        let company_owned = company.clone();
        match tokio::task::spawn_blocking(move || {
            dao::update_profile(audit_clone.turso(), &aid_owned, &name_owned, &company_owned)
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(e)) => tracing::warn!("update_profile failed: {e}"),
            Err(e) => tracing::warn!("update_profile spawn_blocking join failed: {e}"),
        }
    }

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
    let audit_clone = audit.clone();
    let aid_for_get = aid.clone();
    let acc_opt = match tokio::task::spawn_blocking(move || {
        dao::find_account_by_id(audit_clone.turso(), &aid_for_get)
    })
    .await
    {
        Ok(v) => v,
        Err(e) => {
            tracing::warn!("my_profile_post find_account spawn_blocking join failed: {e}");
            None
        }
    };
    if let Some(acc) = acc_opt {
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
    // AUDIT E P0-1: 3 つの blocking DAO 呼出を 1 度の spawn_blocking にまとめる
    let audit_clone = audit.clone();
    let aid_clone = aid.clone();
    let triple = tokio::task::spawn_blocking(move || {
        let acc = dao::find_account_by_id(audit_clone.turso(), &aid_clone);
        let sessions = dao::list_sessions_for_account(audit_clone.turso(), &aid_clone, 50);
        let activities = dao::list_activity_for_account(audit_clone.turso(), &aid_clone, 100);
        (acc, sessions, activities)
    })
    .await;
    let (acc_opt, sessions, activities) = match triple {
        Ok(t) => t,
        Err(e) => {
            tracing::warn!("my_activity spawn_blocking join failed: {e}");
            (None, Vec::new(), Vec::new())
        }
    };
    let Some(acc) = acc_opt else {
        return Html(render::not_linked_page());
    };
    Html(render::activity_page(&acc, &sessions, &activities))
}
