//! 確認コードのメールの送信元の Gmail 許可 (ADR-035)。社内の管理画面 `/app/portal-admin` から取る。
//!
//! - CRM のメール送信の許可 (`crm_gmail_grants`) とは別の表 `portal_mail_grants` に置き、範囲は gmail.send だけ。
//! - 暗号化は CRM と同じ方式・同じ鍵 (`CRM_EMAIL_TOKEN_KEY`) だが、AAD に用途の文字を足すので
//!   CRM の行とは取り違えて復号できない。
//! - 許可を取れるのは管理者で、本人が `PORTAL_MAIL_SENDER` と同じ人のときだけ。違う人の許可は保存しない。
//! - OAuth の state・PKCE・nonce・ID token の検証は CRM の作り (`crm::email::oauth`) を使い回す。
//!   callback は顧客画面用に別に持つ (`PORTAL_MAIL_REDIRECT_URL`)。
//! - Turso への書き込みは許可・取り消しのときだけ (1 回ずつ)。
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use tower_sessions::cookie::{Cookie, SameSite};
use tower_sessions::Session;
use ts_rs::TS;

use super::mail::GmailMailer;
use super::{err, Ctx};
use crate::crm::email::grants;
use crate::crm::pending;
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::get_str;
use crate::AppState;

/// 許可の範囲 (送るだけ。読み取りは要らない)
pub const SCOPE: &str = "https://www.googleapis.com/auth/gmail.send";
/// 顧客画面用の callback (Google の OAuth クライアントにこの URL を登録する)
pub const CALLBACK: &str = "/api/admin/portal/mail/callback";
const TX_COOKIE: &str = "hrhr_portal_mail_tx";
const TX_PATH: &str = "/api/admin/portal/mail";

/// 暗号化の AAD。CRM の行 (AAD = アドレスだけ) と取り違えて復号できないよう、用途の文字を足す
pub fn aad(sender: &str) -> String {
    format!("portal_mail_grants\n{sender}")
}

// ---- 保存 (監査 Turso) ----

pub struct GrantRow {
    pub encrypted: String,
    pub granted_at: String,
}

/// 取り消されていない許可。範囲が足りない行は無いものとして扱う
pub fn get(t: &TursoDb, sender: &str) -> Result<Option<GrantRow>, String> {
    let rows = t.query(
        "SELECT encrypted_refresh_token, scopes, granted_at FROM portal_mail_grants WHERE sender_email = ?1 AND revoked_at IS NULL",
        &[&sender],
    )?;
    Ok(rows
        .first()
        .filter(|r| get_str(r, "scopes").split_whitespace().any(|s| s == SCOPE))
        .map(|r| GrantRow {
            encrypted: get_str(r, "encrypted_refresh_token"),
            granted_at: get_str(r, "granted_at"),
        })
        .filter(|g| !g.encrypted.is_empty()))
}

pub fn save(t: &TursoDb, sender: &str, encrypted: &str, scopes: &str) -> Result<(), String> {
    let now = pending::now_iso();
    t.execute(
        "INSERT INTO portal_mail_grants (sender_email, encrypted_refresh_token, scopes, granted_at, revoked_at) VALUES (?1, ?2, ?3, ?4, NULL) ON CONFLICT(sender_email) DO UPDATE SET encrypted_refresh_token = excluded.encrypted_refresh_token, scopes = excluded.scopes, granted_at = excluded.granted_at, revoked_at = NULL",
        &[&sender, &encrypted, &scopes, &now],
    )
}

pub fn revoke(t: &TursoDb, sender: &str) -> Result<(), String> {
    let now = pending::now_iso();
    t.execute(
        "UPDATE portal_mail_grants SET revoked_at = ?1, encrypted_refresh_token = '' WHERE sender_email = ?2",
        &[&now, &sender],
    )
}

// ---- 社内の管理 API ----

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/portal/mail", routing::get(status))
        .route("/api/admin/portal/mail/authorize", routing::get(authorize))
        .route(CALLBACK, routing::get(callback))
        .route("/api/admin/portal/mail/revoke", routing::post(revoke_route))
}

#[derive(Serialize, TS)]
pub struct PortalAdminMail {
    /// 送信の設定 (`PORTAL_MAIL_SENDER`・鍵・Google の設定・callback・監査 DB) がそろっているか
    pub configured: bool,
    /// 送信元のアドレス (管理者にだけ見せる)。未設定なら null
    pub sender: Option<String>,
    /// 見ている管理者が送信元本人か (本人だけが許可を押せる)
    pub is_sender: bool,
    /// `granted` (許可済み) / `none` (未許可) / `expired` (切れ・取り消し) / `unknown` (読めない)
    pub state: String,
    pub granted_at: Option<String>,
    /// Gmail に断られて、しばらく送らない間か
    pub paused: bool,
}

fn gmail(ctx: &Ctx) -> Option<&GmailMailer> {
    ctx.mailer.as_deref().and_then(|m| m.gmail())
}

async fn admin(session: &Session, state: &Arc<AppState>, ctx: &Ctx) -> Result<String, Response> {
    super::admin::admin_email(session, state, ctx).await
}

async fn status(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let me = match admin(&session, &state, &ctx).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(m) = gmail(&ctx) else {
        return Json(PortalAdminMail {
            configured: false,
            sender: None,
            is_sender: false,
            state: "none".into(),
            granted_at: None,
            paused: false,
        })
        .into_response();
    };
    let (state_name, granted_at) = match m.grant_row().await {
        Ok(Some(row)) if m.expired() => ("expired", Some(row.granted_at)),
        Ok(Some(row)) => ("granted", Some(row.granted_at)),
        Ok(None) => ("none", None),
        Err(_) => ("unknown", None),
    };
    Json(PortalAdminMail {
        configured: true,
        sender: Some(m.sender().to_string()),
        is_sender: me.eq_ignore_ascii_case(m.sender()),
        state: state_name.into(),
        granted_at,
        paused: m.is_paused(),
    })
    .into_response()
}

fn tx_cookie(value: &str, age: i64) -> HeaderValue {
    HeaderValue::from_str(
        &Cookie::build((TX_COOKIE, value.to_string()))
            .path(TX_PATH)
            .http_only(true)
            .same_site(SameSite::Lax)
            .secure(crate::config::is_production_env())
            .max_age(time::Duration::seconds(age))
            .build()
            .to_string(),
    )
    .unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn not_sender() -> Response {
    err(
        StatusCode::FORBIDDEN,
        "not_sender",
        "送信元に決まっている人だけが許可できます。送信元の人がこの画面を開いて押してください",
    )
}

fn not_configured() -> Response {
    err(
        StatusCode::NOT_FOUND,
        "mail_not_configured",
        "メールの送信元が設定されていません",
    )
}

/// 押した管理者本人の Gmail の許可を取りに Google へ移る (送信元本人だけ)
async fn authorize(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let me = match admin(&session, &state, &ctx).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(m) = gmail(&ctx) else {
        return not_configured();
    };
    if !me.eq_ignore_ascii_case(m.sender()) {
        return not_sender();
    }
    if state.google_oidc.is_none() {
        return not_configured();
    }
    match m
        .service()
        .authorization_for(m.sender().to_string(), &[SCOPE])
    {
        Ok((url, tx)) => (
            StatusCode::SEE_OTHER,
            [
                (header::LOCATION, url),
                (
                    header::SET_COOKIE,
                    tx_cookie(&tx, 300).to_str().unwrap_or_default().into(),
                ),
            ],
        )
            .into_response(),
        Err(_) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "gmail_unavailable",
            "いまは許可を始められません。少し待ってからやり直してください",
        ),
    }
}

#[derive(Deserialize)]
struct CallbackQuery {
    state: Option<String>,
    code: Option<String>,
    error: Option<String>,
}

fn page(message: &str) -> Response {
    axum::response::Html(format!(
        "<!doctype html><html lang=\"ja\"><meta charset=\"utf-8\"><title>送信元の Gmail の許可</title><p>{message}</p><a href=\"/app/portal-admin?view=mail\">応募者画面の管理へ戻る</a></html>"
    ))
    .into_response()
}

fn refused(status: StatusCode, message: &str) -> Response {
    let mut r = page(message);
    *r.status_mut() = status;
    r
}

/// Google から戻ってくる。HR_HR のログインの Cookie は Strict で届かないので、
/// 1 回だけ使える state (Lax の Cookie と一致) と ID token で本人を確かめる
async fn callback(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let mut resp = match gmail(&ctx) {
        Some(m) => callback_inner(&state, m, &headers, q).await,
        None => not_configured(),
    };
    resp.headers_mut()
        .append(header::SET_COOKIE, tx_cookie("", 0));
    resp
}

async fn callback_inner(
    state: &AppState,
    m: &GmailMailer,
    headers: &HeaderMap,
    q: CallbackQuery,
) -> Response {
    let service = m.service();
    let Some(tx) = service.consume_with(q.state.as_deref().unwrap_or_default(), headers, TX_COOKIE)
    else {
        return refused(
            StatusCode::BAD_REQUEST,
            "この許可の手続きは使えません。管理画面からやり直してください。",
        );
    };
    if q.error.is_some() {
        return refused(
            StatusCode::BAD_REQUEST,
            "許可は取り消されました。何も保存していません。",
        );
    }
    let Some(code) = q.code.filter(|s| !s.is_empty()) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "許可を確かめられませんでした。何も保存していません。",
        );
    };
    let (Some(oidc), Some(audit)) = (&state.google_oidc, &state.audit) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "いまは保存できません。");
    };
    let Ok(tokens) = service
        .tokens(&[
            ("grant_type", "authorization_code"),
            ("code", &code),
            ("redirect_uri", &service.cfg.redirect_url),
            ("code_verifier", &tx.login.verifier),
        ])
        .await
    else {
        return refused(
            StatusCode::BAD_GATEWAY,
            "Google から許可を受け取れませんでした。",
        );
    };
    let Ok(identity) = oidc
        .verify_for_client(
            tokens.id_token.as_deref().unwrap_or_default(),
            &tx.login.nonce,
            &service.cfg.google.client_id,
        )
        .await
    else {
        return refused(
            StatusCode::FORBIDDEN,
            "本人を確かめられませんでした。何も保存していません。",
        );
    };
    // 許可した Google のアカウントが送信元本人 (手続きを始めた人) でなければ保存しない
    if !identity.email.eq_ignore_ascii_case(m.sender())
        || !tx.operator.eq_ignore_ascii_case(m.sender())
        || crate::account_is_disabled(state, &identity.email).await
    {
        tracing::warn!("[portal] 送信元と違う人の Gmail の許可だったので保存しなかった");
        return refused(
            StatusCode::FORBIDDEN,
            "送信元に決まっている人の Google アカウントで許可してください。何も保存していません。",
        );
    }
    let scopes = tokens.scope.unwrap_or_default();
    if !scopes.split_whitespace().any(|s| s == SCOPE) {
        return refused(
            StatusCode::FORBIDDEN,
            "メールを送る許可が選ばれていませんでした。何も保存していません。",
        );
    }
    let Some(refresh) = tokens.refresh_token.filter(|t| !t.is_empty()) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "許可を受け取れませんでした。管理画面からやり直してください。",
        );
    };
    let sender = m.sender().to_string();
    let Ok(encrypted) = grants::encrypt(&service.cfg.key, &aad(&sender), &refresh) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "いまは保存できません。");
    };
    let saved = pending::blocking(audit, move |t| save(t, &sender, &encrypted, &scopes)).await;
    if !matches!(saved, Ok(Ok(()))) {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "いまは保存できません。少し待ってからやり直してください。",
        );
    }
    tracing::info!("[portal] 確認コードのメールの送信元の Gmail の許可を保存した");
    m.check_now().await;
    page("送信元の Gmail の許可を保存しました。確認コードのメールを送れるようになりました。")
}

/// 許可を取り消す (管理者なら誰でも。止めたいときにすぐ止められるように)
async fn revoke_route(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    if let Err(r) = admin(&session, &state, &ctx).await {
        return r;
    }
    let Some(m) = gmail(&ctx) else {
        return not_configured();
    };
    match m.revoke().await {
        Ok(google_pending) => Json(
            serde_json::json!({ "state": "none", "google_revocation_pending": google_pending }),
        )
        .into_response(),
        Err(_) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "mail_revoke_failed",
            "いまは取り消せません。少し待ってからやり直してください",
        ),
    }
}
