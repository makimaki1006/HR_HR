//! Incremental Gmail consent, separate from login. Lax transaction cookie survives Google redirect.
use super::{grants, EmailError, EmailService, SCOPES};
use crate::crm::{
    pending, rbac,
    routes::{error_json, CrmCtx},
};
use crate::{auth::google_oidc::LoginTx, AppState};
use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::Deserialize;
use std::{sync::Arc, time::Instant};
use tower_sessions::{
    cookie::{Cookie, SameSite},
    Session,
};

pub(crate) struct GrantTx {
    pub(crate) login: LoginTx,
    pub(crate) operator: String,
    created: Instant,
}
const COOKIE: &str = "hrhr_gmail_tx";
fn cookie(value: String, age: i64) -> HeaderValue {
    HeaderValue::from_str(
        &Cookie::build((COOKIE, value))
            .path("/api/crm/gmail")
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(crate::config::is_production_env())
            .max_age(time::Duration::seconds(age))
            .build()
            .to_string(),
    )
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}
impl EmailService {
    pub fn authorization(&self, operator: String) -> Result<(String, String), EmailError> {
        self.authorization_for(operator, &SCOPES)
    }
    /// Consent URL for the given Gmail scopes (the customer portal asks for gmail.send only).
    pub(crate) fn authorization_for(
        &self,
        operator: String,
        scopes: &[&str],
    ) -> Result<(String, String), EmailError> {
        let login = LoginTx::generate();
        let state = login.state.clone();
        let scope = format!("openid email {}", scopes.join(" "));
        let challenge = login.code_challenge();
        let url = reqwest::Url::parse_with_params(
            &self.cfg.authorization_url,
            &[
                ("response_type", "code"),
                ("client_id", self.cfg.google.client_id.as_str()),
                ("redirect_uri", self.cfg.redirect_url.as_str()),
                ("scope", scope.as_str()),
                ("access_type", "offline"),
                ("include_granted_scopes", "true"),
                ("prompt", "consent"),
                ("login_hint", operator.as_str()),
                ("hd", self.cfg.google.hosted_domain.as_str()),
                ("state", state.as_str()),
                ("nonce", login.nonce.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
            ],
        )
        .map_err(|_| EmailError::GmailUnavailable)?;
        let mut txs = self
            .transactions
            .lock()
            .map_err(|_| EmailError::GmailUnavailable)?;
        txs.retain(|_, t| t.created.elapsed().as_secs() < 300);
        if txs.len() >= 1000 {
            return Err(EmailError::GmailUnavailable);
        }
        txs.insert(
            state.clone(),
            GrantTx {
                login,
                operator,
                created: Instant::now(),
            },
        );
        Ok((url.to_string(), state))
    }
    fn consume(&self, state: &str, headers: &HeaderMap) -> Option<GrantTx> {
        self.consume_with(state, headers, COOKIE)
    }
    /// One-time consent transaction: the state must match the named Lax cookie.
    pub(crate) fn consume_with(
        &self,
        state: &str,
        headers: &HeaderMap,
        cookie_name: &str,
    ) -> Option<GrantTx> {
        let value = headers
            .get_all(header::COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|v| v.split(';'))
            .filter_map(|v| v.trim().split_once('='))
            .find(|(name, _)| *name == cookie_name)?
            .1;
        if state.is_empty() || state != value {
            return None;
        }
        self.transactions
            .lock()
            .ok()?
            .remove(state)
            .filter(|t| t.created.elapsed().as_secs() < 300)
    }
}
impl EmailService {
    /// Drop outstanding consent requests of this operator (after revocation).
    pub(crate) fn forget_transactions(&self, operator: &str) {
        if let Ok(mut txs) = self.transactions.lock() {
            txs.retain(|_, tx| tx.operator != operator);
        }
    }
}
pub(in crate::crm) async fn authorize(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let Some(service) = &ctx.write.email else {
        return error_json(StatusCode::NOT_FOUND, "email_disabled");
    };
    if state.audit.is_none() || state.google_oidc.is_none() {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    }
    match service.authorization(p.email.unwrap_or_default()) {
        Ok((url, tx)) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, url),
                (
                    header::SET_COOKIE,
                    cookie(tx, 300).to_str().unwrap_or_default().into(),
                ),
            ],
        )
            .into_response(),
        Err(_) => error_json(StatusCode::SERVICE_UNAVAILABLE, "gmail_unavailable"),
    }
}
#[derive(Deserialize)]
pub(in crate::crm) struct Callback {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}
pub(in crate::crm) async fn callback(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    headers: HeaderMap,
    Query(q): Query<Callback>,
) -> Response {
    let Some(service) = &ctx.write.email else {
        return error_json(StatusCode::NOT_FOUND, "email_disabled");
    };
    let tx = service.consume(q.state.as_deref().unwrap_or_default(), &headers);
    let mut response = callback_inner(&state, &ctx, service, q, tx).await;
    response
        .headers_mut()
        .append(header::SET_COOKIE, cookie(String::new(), 0));
    response
}
async fn callback_inner(
    state: &AppState,
    ctx: &CrmCtx,
    service: &EmailService,
    q: Callback,
    tx: Option<GrantTx>,
) -> Response {
    let Some(tx) = tx else {
        return error_json(StatusCode::BAD_REQUEST, "invalid_state");
    };
    if q.error.is_some() {
        return error_json(StatusCode::BAD_REQUEST, "gmail_consent_cancelled");
    }
    let Some(code) = q.code.filter(|s| !s.is_empty()) else {
        return error_json(StatusCode::BAD_REQUEST, "invalid_code");
    };
    let (Some(oidc), Some(audit)) = (&state.google_oidc, &state.audit) else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let tokens = match service
        .tokens(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &service.cfg.redirect_url),
            ("code_verifier", &tx.login.verifier),
        ])
        .await
    {
        Ok(t) => t,
        Err(e) => return error_json(StatusCode::BAD_GATEWAY, e.code()),
    };
    let identity = match oidc
        .verify_for_client(
            tokens.id_token.as_deref().unwrap_or_default(),
            &tx.login.nonce,
            &service.cfg.google.client_id,
        )
        .await
    {
        Ok(i) => i,
        Err(_) => return error_json(StatusCode::FORBIDDEN, "gmail_identity_mismatch"),
    };
    if identity.email != tx.operator
        || !ctx.access.is_allowed(&identity.email)
        || crate::account_is_disabled(state, &identity.email).await
    {
        return error_json(StatusCode::FORBIDDEN, "gmail_identity_mismatch");
    }
    let scopes = tokens.scope.unwrap_or_default();
    if !grants::has_scopes(&scopes) {
        return error_json(StatusCode::FORBIDDEN, "gmail_scopes_missing");
    }
    let Some(refresh) = tokens.refresh_token.filter(|t| !t.is_empty()) else {
        return error_json(StatusCode::BAD_REQUEST, "gmail_refresh_missing");
    };
    let encrypted = match grants::encrypt(&service.cfg.key, &identity.email, &refresh) {
        Ok(t) => t,
        Err(e) => return error_json(StatusCode::SERVICE_UNAVAILABLE, e.code()),
    };
    let result = pending::blocking(audit, move |t| {
        grants::save(t, &identity.email, &encrypted, &scopes)
    })
    .await;
    if !matches!(result, Ok(Ok(()))) {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    }
    // 200 link (no redirect chain): Strict login cookie is available for the next same-site navigation.
    axum::response::Html("<!doctype html><html lang=\"ja\"><meta charset=\"utf-8\"><title>Gmail の許可</title><p>Gmail での送信を許可しました。</p><a href=\"/app/crm\">架電画面へ戻る</a></html>").into_response()
}
pub(in crate::crm) async fn revoke(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let Some(service) = &ctx.write.email else {
        return error_json(StatusCode::NOT_FOUND, "email_disabled");
    };
    let Some(audit) = &state.audit else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let who = p.email.unwrap_or_default();
    let token = service.refresh_token(audit, &who).await;
    let op = who.clone();
    if !matches!(
        pending::blocking(audit, move |t| grants::revoke(t, &op)).await,
        Ok(Ok(()))
    ) {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    }
    // Invalidate outstanding consent requests as well.
    if let Ok(mut txs) = service.transactions.lock() {
        txs.retain(|_, tx| tx.operator != who);
    }
    if let Ok(token) = token {
        if service.revoke_google(&token).await.is_err() {
            return (
                StatusCode::ACCEPTED,
                Json(serde_json::json!({"granted":false,"google_revocation_pending":true})),
            )
                .into_response();
        }
    }
    Json(serde_json::json!({"granted":false,"google_revocation_pending":false})).into_response()
}
