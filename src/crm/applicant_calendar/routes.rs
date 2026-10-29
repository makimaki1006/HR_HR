//! 管理画面の API (`/app/admin?view=calendar`)。管理者だけ。
//!
//! - `GET  /api/admin/applicant-calendar`: 許可の状態と最後の同期
//! - `GET  /api/admin/applicant-calendar/authorize`: Google へ (管理用カレンダーのアカウント本人だけ)
//! - `GET  /api/admin/applicant-calendar/callback`: Google から戻る (本人を確かめて保存)
//! - `POST /api/admin/applicant-calendar/revoke`: 取り消す (管理者なら誰でも)
//!
//! auth_middleware の外に置き、認可は各ハンドラで CRM と同じ (会社の Google ログイン) + 管理者だけ。
//! callback は HR_HR のログインの Cookie (Strict) が届かないので、1 回だけ使える state と ID token で本人を確かめる。
#![allow(clippy::result_large_err)]

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing;
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use serde_json::json;
use tower_sessions::cookie::{Cookie, SameSite};
use tower_sessions::Session;
use ts_rs::TS;

use super::google::{aad, save, ApplicantCalendar, Problem, CALLBACK, SCOPE};
use super::CalendarConfig;
use crate::crm::email::grants;
use crate::crm::pending;
use crate::crm::rbac::{self, CrmAccess};
use crate::AppState;

const TX_COOKIE: &str = "hrhr_calendar_tx";
const TX_PATH: &str = "/api/admin/applicant-calendar";

#[derive(Clone)]
struct Ctx {
    svc: Option<Arc<ApplicantCalendar>>,
    access: CrmAccess,
}

pub fn router(svc: Option<Arc<ApplicantCalendar>>, access: CrmAccess) -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/applicant-calendar", routing::get(status))
        .route(
            "/api/admin/applicant-calendar/authorize",
            routing::get(authorize),
        )
        .route(CALLBACK, routing::get(callback))
        .route(
            "/api/admin/applicant-calendar/revoke",
            routing::post(revoke_route),
        )
        .layer(Extension(Arc::new(Ctx { svc, access })))
}

fn err(status: StatusCode, code: &str, message: &str) -> Response {
    (status, Json(json!({ "error": code, "message": message }))).into_response()
}

#[derive(Serialize, TS)]
pub struct ApplicantCalendarStatus {
    /// 許可の設定 (アカウント・鍵・Google の設定・戻り先・監査 DB) がそろっているか
    pub configured: bool,
    /// 背景の周回が動く設定か (`APPLICANT_CALENDAR_ENABLED=1`)
    pub enabled: bool,
    /// 管理用カレンダーのアカウント (管理者にだけ見せる)
    pub account: Option<String>,
    /// 見ている管理者がそのアカウント本人か (本人だけが許可を押せる)
    pub is_account: bool,
    /// `granted` (許可済み) / `none` (未許可) / `expired` (切れている) / `unknown` (読めない)
    pub state: String,
    pub granted_at: Option<String>,
    /// 載せる項目の種類 (例: 面接・入社・内定)
    pub kinds: Vec<String>,
    /// 最後に同期が終わった時刻 (ISO 8601、UTC)
    pub last_sync_at: Option<String>,
    pub last_created: u32,
    pub last_updated: u32,
    pub last_deleted: u32,
    /// 直近の周回で書けなかった件数
    pub last_errors: u32,
    /// 起動してからの書けなかった件数の合計
    #[ts(type = "number")]
    pub errors_since_start: u64,
    /// 直近の周回の問題 (平易な文)。無ければ null
    pub problem: Option<String>,
}

async fn admin_email(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &Ctx,
) -> Result<String, Response> {
    let p = rbac::authorize(session, state, &ctx.access, None)
        .await
        .map_err(IntoResponse::into_response)?;
    if !p.role.is_some_and(|r| r.is_admin()) {
        return Err(err(
            StatusCode::FORBIDDEN,
            "forbidden",
            "管理者だけが使えます",
        ));
    }
    Ok(p.email.unwrap_or_default())
}

fn not_configured() -> Response {
    err(
        StatusCode::NOT_FOUND,
        "calendar_not_configured",
        "カレンダーの設定がされていません",
    )
}

async fn status(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let me = match admin_email(&session, &state, &ctx).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(svc) = &ctx.svc else {
        let cfg = CalendarConfig::from_env();
        return Json(ApplicantCalendarStatus {
            configured: false,
            enabled: cfg.enabled,
            account: None,
            is_account: false,
            state: "none".into(),
            granted_at: None,
            kinds: cfg.props.iter().map(|(_, l)| l.clone()).collect(),
            last_sync_at: None,
            last_created: 0,
            last_updated: 0,
            last_deleted: 0,
            last_errors: 0,
            errors_since_start: 0,
            problem: None,
        })
        .into_response();
    };
    let (state_name, granted_at) = match svc.grant_row().await {
        Ok(Some(row)) if svc.expired() => ("expired", Some(row.granted_at)),
        Ok(Some(row)) => ("granted", Some(row.granted_at)),
        Ok(None) => ("none", None),
        Err(_) => ("unknown", None),
    };
    let s = svc.status();
    Json(ApplicantCalendarStatus {
        configured: true,
        enabled: svc.cfg.enabled,
        account: Some(svc.account().to_string()),
        is_account: me.eq_ignore_ascii_case(svc.account()),
        state: state_name.into(),
        granted_at,
        kinds: svc.cfg.props.iter().map(|(_, l)| l.clone()).collect(),
        last_sync_at: s
            .last_sync_at
            .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Secs, true)),
        last_created: s.last.created,
        last_updated: s.last.updated,
        last_deleted: s.last.deleted,
        last_errors: s.last.errors,
        errors_since_start: s.errors_since_start,
        problem: s
            .problem
            // 許可が無い・切れたは state で出すので重ねない
            .filter(|p| !matches!(p, Problem::NoGrant | Problem::Expired))
            .map(|p| p.message().to_string()),
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

/// 押した管理者本人の Google カレンダーの許可を取りに Google へ移る (管理用カレンダーのアカウント本人だけ)
async fn authorize(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let me = match admin_email(&session, &state, &ctx).await {
        Ok(e) => e,
        Err(r) => return r,
    };
    let Some(svc) = &ctx.svc else {
        return not_configured();
    };
    if !me.eq_ignore_ascii_case(svc.account()) {
        return err(
            StatusCode::FORBIDDEN,
            "not_account",
            "管理用カレンダーのアカウントの人だけが許可できます。その人がこの画面を開いて押してください",
        );
    }
    if state.google_oidc.is_none() {
        return not_configured();
    }
    match svc
        .service()
        .authorization_for(svc.account().to_string(), &[SCOPE])
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
            "calendar_unavailable",
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
        "<!doctype html><html lang=\"ja\"><meta charset=\"utf-8\"><title>カレンダーの許可</title><p>{message}</p><a href=\"/app/admin?view=calendar\">管理画面へ戻る</a></html>"
    ))
    .into_response()
}

fn refused(status: StatusCode, message: &str) -> Response {
    let mut r = page(message);
    *r.status_mut() = status;
    r
}

/// 許可を保存してよいか。Google で選んだアカウント・手続きを始めた人が、どちらも
/// 管理用カレンダーのアカウント本人で、HR_HR で無効になっておらず、範囲が選ばれているときだけ
pub(crate) fn consent_allowed(
    account: &str,
    operator: &str,
    google_email: &str,
    disabled: bool,
    scopes: &str,
) -> Result<(), &'static str> {
    if !google_email.eq_ignore_ascii_case(account)
        || !operator.eq_ignore_ascii_case(account)
        || disabled
    {
        return Err("管理用カレンダーのアカウントで許可してください。何も保存していません。");
    }
    if !scopes.split_whitespace().any(|s| s == SCOPE) {
        return Err("カレンダーに予定を書く許可が選ばれていませんでした。何も保存していません。");
    }
    Ok(())
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Query(q): Query<CallbackQuery>,
) -> Response {
    let mut resp = match &ctx.svc {
        Some(svc) => callback_inner(&state, svc, &headers, q).await,
        None => not_configured(),
    };
    resp.headers_mut()
        .append(header::SET_COOKIE, tx_cookie("", 0));
    resp
}

async fn callback_inner(
    state: &AppState,
    svc: &ApplicantCalendar,
    headers: &HeaderMap,
    q: CallbackQuery,
) -> Response {
    let service = svc.service();
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
    let Some(oidc) = &state.google_oidc else {
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
    let scopes = tokens.scope.unwrap_or_default();
    let disabled = crate::account_is_disabled(state, &identity.email).await;
    if let Err(message) = consent_allowed(
        svc.account(),
        &tx.operator,
        &identity.email,
        disabled,
        &scopes,
    ) {
        tracing::warn!("[calendar] 管理用カレンダーの許可を保存しなかった (本人・範囲が違う)");
        return refused(StatusCode::FORBIDDEN, message);
    }
    let Some(refresh) = tokens.refresh_token.filter(|t| !t.is_empty()) else {
        return refused(
            StatusCode::BAD_REQUEST,
            "許可を受け取れませんでした。管理画面からやり直してください。",
        );
    };
    let account = svc.account().to_string();
    let Ok(encrypted) = grants::encrypt(&service.cfg.key, &aad(&account), &refresh) else {
        return refused(StatusCode::SERVICE_UNAVAILABLE, "いまは保存できません。");
    };
    let saved =
        pending::blocking(svc.audit(), move |t| save(t, &account, &encrypted, &scopes)).await;
    if !matches!(saved, Ok(Ok(()))) {
        return refused(
            StatusCode::SERVICE_UNAVAILABLE,
            "いまは保存できません。少し待ってからやり直してください。",
        );
    }
    svc.granted().await;
    tracing::info!("[calendar] 管理用カレンダーの許可を保存した");
    page("管理用カレンダーの許可を保存しました。次の周回から応募の日付をカレンダーに載せます。")
}

fn fetch_request(headers: &HeaderMap) -> bool {
    headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim()
                .eq_ignore_ascii_case(crate::CSRF_REQUESTED_WITH_VALUE)
        })
}

/// 許可を取り消す (管理者なら誰でも。止めたいときにすぐ止められるように)
async fn revoke_route(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    request: axum::extract::Request,
) -> Response {
    if !fetch_request(request.headers()) || crate::check_csrf(&request).is_err() {
        return err(
            StatusCode::FORBIDDEN,
            "csrf",
            "書き込みの確認に失敗しました",
        );
    }
    if let Err(r) = admin_email(&session, &state, &ctx).await {
        return r;
    }
    let Some(svc) = &ctx.svc else {
        return not_configured();
    };
    match svc.revoke().await {
        Ok(google_pending) => {
            Json(json!({ "state": "none", "google_revocation_pending": google_pending }))
                .into_response()
        }
        Err(_) => err(
            StatusCode::SERVICE_UNAVAILABLE,
            "calendar_revoke_failed",
            "いまは取り消せません。少し待ってからやり直してください",
        ),
    }
}
