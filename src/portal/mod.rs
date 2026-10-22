//! 顧客向け応募者画面のログイン (計画 `docs/architecture/customer-applicant-portal-plan.html` 5 章・6 章、PR2・PR3)。
//!
//! - 画面 `GET /portal`, `/portal/{*rest}` (Vite の入口 `portal`)。社内の画面のコードは含めない。
//! - 顧客の API `/api/portal/*`。顧客の Cookie (`__Host-portal_session`) だけを見て、社内の Cookie は見ない。
//!   社内の `/app/*`・`/api/*`・`/tab/*` は社内の Cookie しか見ないので、顧客の Cookie では入れない。
//! - 社内の管理 API `/api/admin/portal/*`。CRM と同じ認可 (会社の Google ログイン) + 管理者だけ。
//!
//! 保存先 (社内用シート) とメールの送信用アカウントはまだ無い。どちらも差し替えられる作りにし、
//! 揃うまでは本番では `/api/portal/*` が 503 を返す。debug ビルドで `PORTAL_DEV_MEMORY=1` のときだけ、
//! メモリの保存先とログに出すメールで動かせる (手元の確認用)。
pub mod auth;
pub mod mail;
pub mod store;

#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::Session;
use ts_rs::TS;

use crate::crm::rbac::{self, CrmAccess};
use crate::handlers::spa_shell::{self, AppManifest};
use crate::AppState;
use auth::{
    AdminError, CodeResult, InviteError, LoginResult, PasswordProblem, PortalAuth, ResendResult,
};
use mail::Mailer;
use store::Account;

pub const SESSION_COOKIE: &str = "__Host-portal_session";
pub const DEVICE_COOKIE: &str = "__Host-portal_device";
pub const CHALLENGE_COOKIE: &str = "__Host-portal_challenge";

/// 顧客の画面の CSP。社内の画面より狭くし、枠に入れさせない (計画 7 章)
const PORTAL_CSP: &str =
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self' data:; connect-src 'self'; object-src 'none'; \
frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

pub struct Portal {
    pub auth: Arc<PortalAuth>,
    pub mailer: Arc<dyn Mailer>,
}

struct Ctx {
    portal: Option<Arc<Portal>>,
    access: CrmAccess,
    manifest: Option<AppManifest>,
}

// ---- 応答の型 (ts-rs) ----

#[derive(Serialize, TS)]
pub struct PortalMe {
    pub email: String,
    pub customer_id: String,
}

#[derive(Serialize, TS)]
pub struct PortalLoginResponse {
    /// `signed_in` (覚えた端末) / `code_required` (確認コードを送った)
    pub status: String,
    /// コードの送り先 (伏せ字)。`code_required` のときだけ
    pub to: Vec<String>,
}

#[derive(Serialize, TS)]
pub struct PortalInviteInfo {
    pub email: String,
    /// `initial` (初回) / `reset` (再設定)
    pub purpose: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminAccount {
    pub id: String,
    pub email: String,
    pub customer_id: String,
    /// `invited` / `active` / `disabled`
    pub status: String,
    pub extra_code_email: Option<String>,
    pub last_login_at: Option<String>,
    pub created_at: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminAccounts {
    pub accounts: Vec<PortalAdminAccount>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminInvite {
    pub account: PortalAdminAccount,
    /// `/portal/invite#t=...`。この応答でだけ返す (保存先にはハッシュだけ)
    pub invite_path: String,
}

fn admin_account(a: &Account) -> PortalAdminAccount {
    PortalAdminAccount {
        id: a.id.clone(),
        email: a.email.clone(),
        customer_id: a.customer_id.clone(),
        status: a.status.as_str().to_string(),
        extra_code_email: a.extra_code_email.clone(),
        last_login_at: a.last_login_at.map(|t| t.to_rfc3339()),
        created_at: a.created_at.to_rfc3339(),
    }
}

fn invite_path(token: &str) -> String {
    // 値は # の後ろに置き、サーバーのログ・Referer に載せない
    format!("/portal/invite#t={token}")
}

// ---- 組み立て ----

pub fn router() -> Router<Arc<AppState>> {
    let manifest = spa_shell::load_manifest(std::path::Path::new(spa_shell::MANIFEST_PATH));
    router_with(portal_from_env(), CrmAccess::from_env(), manifest)
}

/// 保存先とメールの送信が揃うまで本番は None。debug ビルドの `PORTAL_DEV_MEMORY=1` だけメモリで動かす。
fn portal_from_env() -> Option<Arc<Portal>> {
    #[cfg(debug_assertions)]
    if std::env::var("PORTAL_DEV_MEMORY").is_ok_and(|v| v == "1") {
        let max = std::env::var("RATE_LIMIT_MAX_ATTEMPTS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(5);
        let lock = std::env::var("RATE_LIMIT_LOCKOUT_SECONDS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(300);
        let mut cfg = auth::AuthConfig::new(max, lock);
        cfg.bcrypt_cost = 6;
        let auth = Arc::new(PortalAuth::new(
            Arc::new(store::MemoryStore::default()),
            cfg,
        ));
        if let Ok((_, token)) = auth.create_account("dev@example.com", "dev", Utc::now()) {
            tracing::warn!(
                "[portal dev] メモリの保存先で動かす。試しのアカウント dev@example.com の招待: {}",
                invite_path(&token)
            );
        }
        return Some(Arc::new(Portal {
            auth,
            mailer: Arc::new(mail::DevLogMailer),
        }));
    }
    None
}

fn router_with(
    portal: Option<Arc<Portal>>,
    access: CrmAccess,
    manifest: Option<AppManifest>,
) -> Router<Arc<AppState>> {
    let ctx = Arc::new(Ctx {
        portal,
        access,
        manifest,
    });
    let pages = Router::new()
        .route("/portal", get(page))
        .route("/portal/{*rest}", get(page));
    let api = Router::new()
        .route("/api/portal/me", get(me))
        .route("/api/portal/login", post(login))
        .route("/api/portal/login/code", post(verify_code))
        .route("/api/portal/login/resend", post(resend))
        .route("/api/portal/logout", post(logout))
        .route("/api/portal/invite/check", post(invite_check))
        .route("/api/portal/invite/accept", post(invite_accept))
        .route(
            "/api/admin/portal/accounts",
            get(admin_list).post(admin_create),
        )
        .route("/api/admin/portal/accounts/{id}/invite", post(admin_invite))
        .route(
            "/api/admin/portal/accounts/{id}/disable",
            post(admin_disable),
        )
        .route("/api/admin/portal/accounts/{id}/enable", post(admin_enable))
        .route(
            "/api/admin/portal/accounts/{id}/extra-email",
            put(admin_extra_email),
        )
        .route_layer(middleware::from_fn(csrf_guard));
    pages
        .merge(api)
        .layer(Extension(ctx))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::HeaderName::from_static("x-robots-tag"),
            HeaderValue::from_static("noindex, nofollow"),
        ))
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(PORTAL_CSP),
        ))
}

/// 書き込みは社内と同じ Origin の確認に加え、`X-Requested-With: fetch` を必ず求める
async fn csrf_guard(request: axum::extract::Request, next: Next) -> Response {
    if request.method() != axum::http::Method::GET {
        let fetch = request
            .headers()
            .get("x-requested-with")
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| {
                v.trim()
                    .eq_ignore_ascii_case(crate::CSRF_REQUESTED_WITH_VALUE)
            });
        if !fetch || crate::check_csrf(&request).is_err() {
            return err(
                StatusCode::FORBIDDEN,
                "csrf",
                "書き込みの確認に失敗しました",
            );
        }
    }
    next.run(request).await
}

async fn page(Extension(ctx): Extension<Arc<Ctx>>) -> Response {
    let resp = spa_shell::entry_page(ctx.manifest.as_ref(), "portal");
    let (parts, body) = resp.into_parts();
    let bytes = axum::body::to_bytes(body, 1024 * 1024)
        .await
        .unwrap_or_default();
    let html = String::from_utf8_lossy(&bytes).replacen(
        "<title>HR_HR</title>",
        "<title>応募者の管理</title>\n<meta name=\"robots\" content=\"noindex, nofollow\">",
        1,
    );
    let mut resp = Response::from_parts(parts, axum::body::Body::from(html));
    resp.headers_mut().remove(header::CONTENT_LENGTH);
    resp
}

// ---- 共通 ----

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

fn unavailable() -> Response {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "portal_unavailable",
        "いまはログインできません。準備中です",
    )
}

fn unauthorized() -> Response {
    err(
        StatusCode::UNAUTHORIZED,
        "unauthorized",
        "ログインしてください",
    )
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|v| v.split(';'))
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
        .filter(|v| !v.is_empty() && v.len() <= 128)
}

fn set_cookie(resp: &mut Response, name: &str, value: &str, max_age_secs: i64, same_site: &str) {
    let v = format!(
        "{name}={value}; Path=/; Secure; HttpOnly; SameSite={same_site}; Max-Age={max_age_secs}"
    );
    if let Ok(v) = HeaderValue::from_str(&v) {
        resp.headers_mut().append(header::SET_COOKIE, v);
    }
}

fn clear_cookie(resp: &mut Response, name: &str) {
    set_cookie(resp, name, "", 0, "Lax");
}

fn client_ip(headers: &HeaderMap) -> String {
    headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "unknown".to_string())
}

fn now() -> DateTime<Utc> {
    Utc::now()
}

const SESSION_MAX_AGE: i64 = 12 * 60 * 60;
const DEVICE_MAX_AGE: i64 = 90 * 24 * 60 * 60;
const CHALLENGE_MAX_AGE: i64 = 30 * 60;

// ---- 顧客 ----

async fn me(Extension(ctx): Extension<Arc<Ctx>>, headers: HeaderMap) -> Response {
    let Some(p) = &ctx.portal else {
        return unavailable();
    };
    match cookie(&headers, SESSION_COOKIE).and_then(|s| p.auth.current(s, now())) {
        Some(a) => Json(PortalMe {
            email: a.email,
            customer_id: a.customer_id,
        })
        .into_response(),
        None => unauthorized(),
    }
}

#[derive(Deserialize)]
struct LoginBody {
    email: String,
    password: String,
}

async fn login(
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Response {
    let Some(p) = ctx.portal.clone() else {
        return unavailable();
    };
    if body.email.len() > 254 || body.password.len() > 1024 {
        return invalid_credentials();
    }
    let device = cookie(&headers, DEVICE_COOKIE).map(str::to_string);
    let ip = client_ip(&headers);
    let auth = p.auth.clone();
    let result = match tokio::task::spawn_blocking(move || {
        auth.login(&body.email, &body.password, device.as_deref(), &ip, now())
    })
    .await
    {
        Ok(r) => r,
        Err(_) => {
            return err(
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal",
                "ログインできませんでした",
            )
        }
    };
    match result {
        LoginResult::SignedIn { session } => {
            tracing::info!("[portal] ログイン (覚えた端末)");
            let mut resp = Json(PortalLoginResponse {
                status: "signed_in".into(),
                to: Vec::new(),
            })
            .into_response();
            set_cookie(&mut resp, SESSION_COOKIE, &session, SESSION_MAX_AGE, "Lax");
            clear_cookie(&mut resp, CHALLENGE_COOKIE);
            resp
        }
        LoginResult::CodeRequired {
            challenge,
            code,
            to,
        } => {
            if p.mailer.send_code(&to, &code).await.is_err() {
                p.auth.drop_challenge(&challenge);
                tracing::warn!("[portal] 確認コードを送れなかった");
                return err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "mail_failed",
                    "確認コードを送れませんでした。少し待ってからもう一度お試しください",
                );
            }
            tracing::info!("[portal] 確認コードを送った");
            let mut resp = Json(PortalLoginResponse {
                status: "code_required".into(),
                to: to.iter().map(|e| auth::mask_email(e)).collect(),
            })
            .into_response();
            set_cookie(
                &mut resp,
                CHALLENGE_COOKIE,
                &challenge,
                CHALLENGE_MAX_AGE,
                "Strict",
            );
            resp
        }
        LoginResult::Invalid => {
            tracing::info!("[portal] ログイン失敗");
            invalid_credentials()
        }
        LoginResult::Locked => err(
            StatusCode::TOO_MANY_REQUESTS,
            "locked",
            "続けて間違えたため、しばらくログインできません。時間をおいてお試しください",
        ),
    }
}

fn invalid_credentials() -> Response {
    err(
        StatusCode::UNAUTHORIZED,
        "invalid_credentials",
        "メールアドレスかパスワードが違います",
    )
}

#[derive(Deserialize)]
struct CodeBody {
    code: String,
    #[serde(default = "default_true")]
    remember: bool,
}

fn default_true() -> bool {
    true
}

async fn verify_code(
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Json(body): Json<CodeBody>,
) -> Response {
    let Some(p) = &ctx.portal else {
        return unavailable();
    };
    let Some(challenge) = cookie(&headers, CHALLENGE_COOKIE) else {
        return code_expired();
    };
    match p
        .auth
        .verify_code(challenge, &body.code, body.remember, now())
    {
        CodeResult::SignedIn { session, device } => {
            tracing::info!("[portal] ログイン (確認コード)");
            let mut resp = Json(json!({ "status": "signed_in" })).into_response();
            set_cookie(&mut resp, SESSION_COOKIE, &session, SESSION_MAX_AGE, "Lax");
            if let Some(d) = device {
                set_cookie(&mut resp, DEVICE_COOKIE, &d, DEVICE_MAX_AGE, "Lax");
            }
            clear_cookie(&mut resp, CHALLENGE_COOKIE);
            resp
        }
        CodeResult::Wrong { left } => (
            StatusCode::UNAUTHORIZED,
            Json(json!({
                "error": "wrong_code",
                "message": "コードが違います。もう一度確かめてください",
                "left": left,
            })),
        )
            .into_response(),
        CodeResult::Expired => {
            let mut resp = code_expired();
            clear_cookie(&mut resp, CHALLENGE_COOKIE);
            resp
        }
    }
}

fn code_expired() -> Response {
    err(
        StatusCode::GONE,
        "code_expired",
        "このコードは使えなくなりました。ログインからやり直してください",
    )
}

async fn resend(Extension(ctx): Extension<Arc<Ctx>>, headers: HeaderMap) -> Response {
    let Some(p) = &ctx.portal else {
        return unavailable();
    };
    let Some(challenge) = cookie(&headers, CHALLENGE_COOKIE) else {
        return code_expired();
    };
    match p.auth.resend(challenge, now()) {
        ResendResult::Sent { code, to } => {
            if p.mailer.send_code(&to, &code).await.is_err() {
                return err(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "mail_failed",
                    "確認コードを送れませんでした。少し待ってからもう一度お試しください",
                );
            }
            Json(PortalLoginResponse {
                status: "code_required".into(),
                to: to.iter().map(|e| auth::mask_email(e)).collect(),
            })
            .into_response()
        }
        ResendResult::Wait { secs } => (
            StatusCode::TOO_MANY_REQUESTS,
            Json(json!({
                "error": "wait",
                "message": format!("あと {secs} 秒待ってから送り直してください"),
                "wait": secs,
            })),
        )
            .into_response(),
        ResendResult::Limit => err(
            StatusCode::TOO_MANY_REQUESTS,
            "resend_limit",
            "送り直せる回数を超えました。ログインからやり直してください",
        ),
        ResendResult::Expired => code_expired(),
    }
}

async fn logout(Extension(ctx): Extension<Arc<Ctx>>, headers: HeaderMap) -> Response {
    if let (Some(p), Some(s)) = (&ctx.portal, cookie(&headers, SESSION_COOKIE)) {
        p.auth.logout(s, now());
    }
    let mut resp = Json(json!({ "status": "signed_out" })).into_response();
    clear_cookie(&mut resp, SESSION_COOKIE);
    resp
}

#[derive(Deserialize)]
struct InviteCheckBody {
    token: String,
}

fn invite_invalid() -> Response {
    err(
        StatusCode::GONE,
        "invite_invalid",
        "このリンクは使えません。期限が切れたか、すでに使われています。弊社の担当者にご連絡ください",
    )
}

async fn invite_check(
    Extension(ctx): Extension<Arc<Ctx>>,
    Json(body): Json<InviteCheckBody>,
) -> Response {
    let Some(p) = &ctx.portal else {
        return unavailable();
    };
    match p.auth.invite_info(body.token.trim(), now()) {
        Ok((a, purpose)) => Json(PortalInviteInfo {
            email: a.email,
            purpose: purpose.as_str().to_string(),
        })
        .into_response(),
        Err(_) => invite_invalid(),
    }
}

#[derive(Deserialize)]
struct InviteAcceptBody {
    token: String,
    password: String,
}

async fn invite_accept(
    Extension(ctx): Extension<Arc<Ctx>>,
    Json(body): Json<InviteAcceptBody>,
) -> Response {
    let Some(p) = ctx.portal.clone() else {
        return unavailable();
    };
    if body.password.len() > 1024 {
        return password_problem(PasswordProblem::TooLong);
    }
    let auth = p.auth.clone();
    let result = tokio::task::spawn_blocking(move || {
        auth.accept_invite(body.token.trim(), &body.password, now())
    })
    .await;
    match result {
        Ok(Ok(email)) => {
            tracing::info!("[portal] パスワードを設定した");
            let mut resp =
                Json(json!({ "status": "password_set", "email": email })).into_response();
            clear_cookie(&mut resp, SESSION_COOKIE);
            resp
        }
        Ok(Err(InviteError::Password(problem))) => password_problem(problem),
        Ok(Err(InviteError::Invalid)) => invite_invalid(),
        Err(_) => err(
            StatusCode::INTERNAL_SERVER_ERROR,
            "internal",
            "設定できませんでした",
        ),
    }
}

fn password_problem(p: PasswordProblem) -> Response {
    let message = match p {
        PasswordProblem::TooShort => "パスワードは 12 文字以上にしてください",
        PasswordProblem::TooLong => "パスワードが長すぎます",
        PasswordProblem::SameAsEmail => "メールアドレスと同じパスワードは使えません",
    };
    err(StatusCode::UNPROCESSABLE_ENTITY, "weak_password", message)
}

// ---- 社内の管理 ----

#[allow(clippy::result_large_err)]
async fn admin_gate(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &Ctx,
) -> Result<(Arc<Portal>, String), Response> {
    let principal = rbac::authorize(session, state, &ctx.access, None)
        .await
        .map_err(IntoResponse::into_response)?;
    if !principal.role.is_some_and(|r| r.is_admin()) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "管理者だけが使えます",
        ));
    }
    let p = ctx.portal.clone().ok_or_else(unavailable)?;
    Ok((p, principal.email.unwrap_or_default()))
}

fn admin_error(e: AdminError) -> Response {
    match e {
        AdminError::BadEmail => err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "bad_email",
            "メールアドレスの形が正しくありません",
        ),
        AdminError::BadCustomer => err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "bad_customer",
            "顧客を選んでください",
        ),
        AdminError::Duplicate => err(
            StatusCode::CONFLICT,
            "duplicate",
            "このメールアドレスのアカウントはすでにあります",
        ),
        AdminError::NotFound => err(
            StatusCode::NOT_FOUND,
            "not_found",
            "アカウントが見つかりません",
        ),
    }
}

async fn admin_list(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let (p, _) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let accounts = p
        .auth
        .store()
        .accounts()
        .iter()
        .map(admin_account)
        .collect();
    Json(PortalAdminAccounts { accounts }).into_response()
}

#[derive(Deserialize)]
struct CreateBody {
    email: String,
    customer_id: String,
}

async fn admin_create(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Json(body): Json<CreateBody>,
) -> Response {
    let (p, by) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match p.auth.create_account(&body.email, &body.customer_id, now()) {
        Ok((a, token)) => {
            tracing::info!("[portal] アカウントを作った (by {by})");
            Json(PortalAdminInvite {
                account: admin_account(&a),
                invite_path: invite_path(&token),
            })
            .into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn admin_invite(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (p, by) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    let result = p.auth.issue_invite(&id, now()).and_then(|t| {
        p.auth
            .store()
            .account(&id)
            .map(|a| (a, t))
            .ok_or(AdminError::NotFound)
    });
    match result {
        Ok((a, token)) => {
            tracing::info!("[portal] 招待のリンクを出した (by {by})");
            Json(PortalAdminInvite {
                account: admin_account(&a),
                invite_path: invite_path(&token),
            })
            .into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn admin_disable(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (p, by) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match p.auth.disable(&id, now()) {
        Ok(a) => {
            tracing::info!("[portal] アカウントを止めた (by {by})");
            Json(admin_account(&a)).into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn admin_enable(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (p, by) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match p.auth.enable(&id) {
        Ok(a) => {
            tracing::info!("[portal] アカウントの停止を戻した (by {by})");
            Json(admin_account(&a)).into_response()
        }
        Err(e) => admin_error(e),
    }
}

#[derive(Deserialize)]
struct ExtraEmailBody {
    email: String,
}

async fn admin_extra_email(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
    Json(body): Json<ExtraEmailBody>,
) -> Response {
    let (p, _) = match admin_gate(&session, &state, &ctx).await {
        Ok(v) => v,
        Err(r) => return r,
    };
    match p.auth.set_extra_email(&id, &body.email) {
        Ok(a) => Json(admin_account(&a)).into_response(),
        Err(e) => admin_error(e),
    }
}
