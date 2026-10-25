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
pub mod admin;
pub mod auth;
#[cfg(debug_assertions)]
mod dev_sheet;
pub mod mail;
pub mod sheet_store;
pub mod sheets;
pub mod store;

#[cfg(test)]
mod sheet_store_tests;
#[cfg(test)]
mod test_fake;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_http::set_header::SetResponseHeaderLayer;
use ts_rs::TS;

use crate::crm::rbac::CrmAccess;
use crate::handlers::spa_shell::{self, AppManifest};
use crate::AppState;
use auth::{CodeResult, InviteError, LoginResult, PasswordProblem, PortalAuth, ResendResult};
use mail::Mailer;
use sheets::{GoogleError, SheetsApi};
use store::{Account, LogEntry, LogSource};

pub const SESSION_COOKIE: &str = "__Host-portal_session";
pub const DEVICE_COOKIE: &str = "__Host-portal_device";
pub const CHALLENGE_COOKIE: &str = "__Host-portal_challenge";

/// 顧客の画面の CSP。社内の画面より狭くし、枠に入れさせない (計画 7 章)
const PORTAL_CSP: &str =
    "default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; \
img-src 'self' data:; font-src 'self' data:; connect-src 'self'; object-src 'none'; \
frame-ancestors 'none'; base-uri 'self'; form-action 'self'";

/// 顧客がログインするのに要るもの (保存先とメールの送信)
#[derive(Clone)]
pub struct Portal {
    pub auth: Arc<PortalAuth>,
    pub mailer: Arc<dyn Mailer>,
}

/// 組み立てに使う部品。揃っていないものは None (その API は 503)
#[derive(Default, Clone)]
pub struct Parts {
    /// 保存先 (社内用シート / メモリ) の上のログインの決まり。社内の管理 API に要る
    pub auth: Option<Arc<PortalAuth>>,
    /// 確認コードを送る。顧客のログインに要る
    pub mailer: Option<Arc<dyn Mailer>>,
    /// 顧客のシートを読む (試しに読む)
    pub reader: Option<Arc<dyn SheetsApi>>,
}

struct Ctx {
    auth: Option<Arc<PortalAuth>>,
    mailer: Option<Arc<dyn Mailer>>,
    reader: Option<Arc<dyn SheetsApi>>,
    access: CrmAccess,
    manifest: Option<AppManifest>,
}

impl Ctx {
    /// 顧客の API を開けるか。保存先が読み込み済みで、メールを送れるときだけ
    fn portal(&self) -> Option<Portal> {
        match (&self.auth, &self.mailer) {
            (Some(auth), Some(mailer)) if auth.store().ready() => Some(Portal {
                auth: auth.clone(),
                mailer: mailer.clone(),
            }),
            _ => None,
        }
    }
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
    router_parts(parts_from_env(), CrmAccess::from_env(), manifest)
}

fn auth_config() -> auth::AuthConfig {
    let max = std::env::var("RATE_LIMIT_MAX_ATTEMPTS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(5);
    let lock = std::env::var("RATE_LIMIT_LOCKOUT_SECONDS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    auth::AuthConfig::new(max, lock)
}

/// 環境変数から部品を作る。
/// - 社内用シート (`PORTAL_STORE_SPREADSHEET_ID` と `PORTAL_STORE_SA_KEY_B64`) が両方あれば保存先にする。
///   どちらかが無ければ保存先は無効 (社内の管理 API も顧客の API も 503)。
/// - 顧客のシートを読む鍵 (`PORTAL_GOOGLE_SA_KEY_B64`) があれば「試しに読む」に使う。
/// - メールの送信用アカウントはまだ無いので、本番の顧客の API は 503 のまま。
/// - debug ビルドで `PORTAL_DEV_MEMORY=1` のときだけ、メモリの保存先とログに出すメールで動かす (手元の確認用)。
fn parts_from_env() -> Parts {
    let reader = sheets::GoogleSheetsApi::from_env_var(sheets::ENV_CUSTOMER_KEY);
    #[cfg(debug_assertions)]
    if std::env::var("PORTAL_DEV_MEMORY").is_ok_and(|v| v == "1") {
        return dev_parts(reader);
    }
    let spreadsheet = std::env::var(sheets::ENV_STORE_SPREADSHEET)
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty());
    let store_api = sheets::GoogleSheetsApi::from_env_var(sheets::ENV_STORE_KEY);
    let auth = match (spreadsheet, store_api) {
        (Some(id), Some(api)) => {
            tracing::info!("[portal] 社内用シートを保存先にする（読み込み中）");
            let store =
                sheet_store::SheetStore::start(api, &id, sheet_store::SheetStoreConfig::default());
            Some(Arc::new(PortalAuth::new(store, auth_config())))
        }
        (Some(_), None) => {
            tracing::warn!("[portal] 社内用シートの鍵が無い・読めないので、保存先は無効");
            None
        }
        _ => None,
    };
    Parts {
        auth,
        mailer: None,
        reader,
    }
}

#[cfg(debug_assertions)]
fn dev_parts(reader: Option<Arc<dyn SheetsApi>>) -> Parts {
    let mut cfg = auth_config();
    cfg.bcrypt_cost = 6;
    let store = Arc::new(store::MemoryStore::default());
    let now = Utc::now();
    let _ = store::PortalStore::insert_customer(
        store.as_ref(),
        store::Customer {
            id: "dev".into(),
            name: "サンプル株式会社".into(),
            hubspot_company_id: None,
            stage: store::CustomerStage::Both,
            disabled: false,
            created_at: now,
            updated_at: now,
        },
    );
    let auth = Arc::new(PortalAuth::new(store, cfg));
    if let Ok((_, token)) = auth.create_account("dev@example.com", "dev", now) {
        tracing::warn!(
            "[portal dev] メモリの保存先で動かす。試しのアカウント dev@example.com の招待: {}",
            invite_path(&token)
        );
    }
    Parts {
        auth: Some(auth),
        mailer: Some(Arc::new(mail::DevLogMailer)),
        reader: reader.or_else(|| Some(Arc::new(dev_sheet::DevSheet) as Arc<dyn SheetsApi>)),
    }
}

/// テスト用: 顧客のログインに要るものが揃った (または無い) ときの組み立て
#[cfg(test)]
fn router_with(
    portal: Option<Arc<Portal>>,
    access: CrmAccess,
    manifest: Option<AppManifest>,
) -> Router<Arc<AppState>> {
    let parts = match portal {
        Some(p) => Parts {
            auth: Some(p.auth.clone()),
            mailer: Some(p.mailer.clone()),
            reader: None,
        },
        None => Parts::default(),
    };
    router_parts(parts, access, manifest)
}

fn router_parts(
    parts: Parts,
    access: CrmAccess,
    manifest: Option<AppManifest>,
) -> Router<Arc<AppState>> {
    let ctx = Arc::new(Ctx {
        auth: parts.auth,
        mailer: parts.mailer,
        reader: parts.reader,
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
        .merge(admin::routes())
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

/// 顧客の操作を記録に残す (アカウントの番号だけ。メールアドレス・パスワードは残さない)
fn portal_log(p: &Portal, kind: &str, account: Option<&Account>, detail: &str) {
    let mut e = LogEntry::new(now(), "", LogSource::Portal, kind);
    if let Some(a) = account {
        e.actor = a.id.clone();
        e.account_id = a.id.clone();
        e.customer_id = a.customer_id.clone();
    }
    e.detail = detail.to_string();
    p.auth.store().append_log(e);
}

/// Google の失敗をログに出す (HTTP の状態だけ)
fn log_google(what: &str, e: GoogleError) {
    match e {
        GoogleError::Status(s) => tracing::warn!("[portal] {what}の失敗（{s}）"),
        GoogleError::Network => tracing::warn!("[portal] {what}の失敗（通信）"),
        GoogleError::Token => tracing::warn!("[portal] {what}の失敗（認証）"),
    }
}

const SESSION_MAX_AGE: i64 = 12 * 60 * 60;
const DEVICE_MAX_AGE: i64 = 90 * 24 * 60 * 60;
const CHALLENGE_MAX_AGE: i64 = 30 * 60;

// ---- 顧客 ----

async fn me(Extension(ctx): Extension<Arc<Ctx>>, headers: HeaderMap) -> Response {
    let Some(p) = ctx.portal() else {
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
    let Some(p) = ctx.portal() else {
        return unavailable();
    };
    if body.email.len() > 254 || body.password.len() > 1024 {
        return invalid_credentials();
    }
    let device = cookie(&headers, DEVICE_COOKIE).map(str::to_string);
    let ip = client_ip(&headers);
    let auth = p.auth.clone();
    let known = p
        .auth
        .store()
        .account_by_email(&auth::normalize_email(&body.email));
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
            portal_log(&p, "login", known.as_ref(), "ログイン (覚えた端末)");
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
            portal_log(
                &p,
                "login_failed",
                known.as_ref(),
                if known.is_some() {
                    "ログインの失敗"
                } else {
                    "ログインの失敗 (登録の無いメールアドレス)"
                },
            );
            invalid_credentials()
        }
        LoginResult::Locked => {
            portal_log(
                &p,
                "login_locked",
                known.as_ref(),
                "続けて間違えたため一時ロック中",
            );
            err(
                StatusCode::TOO_MANY_REQUESTS,
                "locked",
                "続けて間違えたため、しばらくログインできません。時間をおいてお試しください",
            )
        }
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
    let Some(p) = ctx.portal() else {
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
            let who = p.auth.current(&session, now());
            portal_log(&p, "login", who.as_ref(), "ログイン (確認コード)");
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
    let Some(p) = ctx.portal() else {
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
    if let (Some(p), Some(s)) = (ctx.portal(), cookie(&headers, SESSION_COOKIE)) {
        let who = p.auth.current(s, now());
        p.auth.logout(s, now());
        if who.is_some() {
            portal_log(&p, "logout", who.as_ref(), "ログアウト");
        }
        // ログアウトは社内用シートに書けるまで少し待つ (再起動で戻らないように)
        let _ = p
            .auth
            .store()
            .persisted(std::time::Duration::from_secs(3))
            .await;
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
    let Some(p) = ctx.portal() else {
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
    let Some(p) = ctx.portal() else {
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
