//! 顧客向け応募者画面の社内向けテスト版 (`/app/portal-test`) の API (2026-10-21)。
//!
//! 手元の橋渡しサーバー (`scripts/portal_dev/sheet_bridge.py`) と同じ読み書きを、本番の HR_HR の中で行う。
//!
//! - `GET  /api/portal-test/sheet?sid=&gid=` 見出し・応募者の行・プルダウンの選択肢・読んだ時刻・Drive の更新時刻
//! - `GET  /api/portal-test/version?sid=`    Drive の更新時刻 (20 秒使い回す)
//! - `POST /api/portal-test/cell`            {sid, gid, rowNumber, header, base, next}。読み直して base と違えば書かずに conflict
//! - `GET  /api/portal-test/sa`              読み書きに使うサービスアカウントの client_email だけ
//!
//! 認可は CRM と同じ (`crm::rbac::check_session_and_account`。会社の Google ログイン + 会社ドメイン +
//! 無効化されていないアカウント)。未ログインを JSON の 401 で返すため `auth_middleware` の外に置き、
//! 書き込みには同じ CSRF の検査と `X-Requested-With: fetch` を求める。
//! 読み書きできるシートはコードに固定した 1 枚だけ (それ以外は 403)。
//! ログには見出しと結果 (保存 / 競合 / 失敗) だけを出し、値・氏名・電話は出さない。応答は `Cache-Control: no-store`。

pub mod google;
pub mod sheet;

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::Session;

use crate::crm::rbac::{self, CrmAccess};
use crate::AppState;
use google::{GoogleError, GoogleSa, DRIVE_API, SHEETS_API};
use sheet::{Decision, HeaderMap as SheetHeaders, FIRST_APPLICANT_ROW, HEADER_ROW};

const VERSION_TTL: Duration = Duration::from_secs(20);
const MAX_HEADER_CHARS: usize = 64;
const MAX_VALUE_CHARS: usize = 20_000;
const MAX_ROW: usize = 100_000;

struct Ctx {
    access: CrmAccess,
    google: Option<Arc<GoogleSa>>,
    version: std::sync::Mutex<Option<(Instant, String)>>,
    title: std::sync::Mutex<Option<String>>,
    /// 読み直してから書くまでを 1 本ずつにする (単一インスタンス前提)
    write_lock: tokio::sync::Mutex<()>,
}

pub fn router() -> Router<Arc<AppState>> {
    router_with(CrmAccess::from_env(), GoogleSa::from_env().map(Arc::new))
}

fn router_with(access: CrmAccess, google: Option<Arc<GoogleSa>>) -> Router<Arc<AppState>> {
    let ctx = Arc::new(Ctx {
        access,
        google,
        version: std::sync::Mutex::new(None),
        title: std::sync::Mutex::new(None),
        write_lock: tokio::sync::Mutex::new(()),
    });
    Router::new()
        .route("/api/portal-test/sheet", get(get_sheet))
        .route("/api/portal-test/version", get(get_version))
        .route("/api/portal-test/sa", get(get_sa))
        .route(
            "/api/portal-test/cell",
            post(post_cell).route_layer(middleware::from_fn(csrf_guard)),
        )
        .layer(Extension(ctx))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

async fn csrf_guard(request: axum::extract::Request, next: Next) -> Response {
    if crate::check_csrf(&request).is_err() {
        return ApiErr::new(StatusCode::FORBIDDEN, "書き込みの確認に失敗しました").into_response();
    }
    next.run(request).await
}

// ---- 応答の失敗 ----

struct ApiErr {
    status: StatusCode,
    body: Value,
}

impl ApiErr {
    fn new(status: StatusCode, message: &str) -> Self {
        Self {
            status,
            body: json!({ "error": message }),
        }
    }
}

impl IntoResponse for ApiErr {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

/// Google の失敗を画面の文に直す。共有されていない (403 / 404) ときは追加するアドレスを添える。
fn google_err(e: GoogleError, client_email: &str, writing: bool) -> ApiErr {
    match e {
        GoogleError::Status(403 | 404) => ApiErr {
            status: StatusCode::FORBIDDEN,
            body: json!({
                "error": format!("このアドレスをシートに編集者として追加してください: {client_email}"),
                "code": "sheet_not_shared",
                "clientEmail": client_email,
            }),
        },
        GoogleError::Status(429) => ApiErr::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "Google が混み合っています。少し待ってからやり直してください",
        ),
        GoogleError::Status(s) if writing => ApiErr::new(
            StatusCode::BAD_GATEWAY,
            &format!("シートに保存できませんでした（{s}）"),
        ),
        GoogleError::Status(s) => ApiErr::new(
            StatusCode::BAD_GATEWAY,
            &format!("Google の読み取りに失敗しました（{s}）"),
        ),
        GoogleError::Network => ApiErr::new(StatusCode::BAD_GATEWAY, "Google につながりません"),
        GoogleError::Token => ApiErr::new(StatusCode::BAD_GATEWAY, "Google の認証に失敗しました"),
    }
}

fn log_google_failure(what: &str, e: GoogleError) {
    match e {
        GoogleError::Status(s) => tracing::warn!("portal-test: {what}の失敗（{s}）"),
        GoogleError::Network => tracing::warn!("portal-test: {what}の失敗（通信）"),
        GoogleError::Token => tracing::warn!("portal-test: {what}の失敗（認証）"),
    }
}

// ---- 共通の確認 ----

async fn authorize(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &Ctx,
) -> Result<(), rbac::Denied> {
    let principal = rbac::load_principal(session).await;
    rbac::check_session_and_account(&principal, None, state, &ctx.access).await
}

fn require_sheet(sid: &str, gid: Option<i64>) -> Result<(), ApiErr> {
    let gid_ok = gid.is_none_or(|g| g == sheet::ALLOWED_SHEET_ID);
    if sid == sheet::ALLOWED_SPREADSHEET_ID && gid_ok {
        Ok(())
    } else {
        Err(ApiErr::new(
            StatusCode::FORBIDDEN,
            "このシートは読み書きできません",
        ))
    }
}

fn require_google(ctx: &Ctx) -> Result<Arc<GoogleSa>, ApiErr> {
    ctx.google.clone().ok_or_else(|| {
        ApiErr::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "サービスアカウントの鍵（GOOGLE_SA_KEY_B64）が設定されていません",
        )
    })
}

// ---- Google の読み取り ----

async fn tab_title(g: &GoogleSa, ctx: &Ctx) -> Result<String, ApiErr> {
    if let Some(t) = ctx.title.lock().ok().and_then(|t| t.clone()) {
        return Ok(t);
    }
    let url = format!("{SHEETS_API}/{}", sheet::ALLOWED_SPREADSHEET_ID);
    let resp = g
        .get(&url, &[("fields", "sheets(properties(sheetId,title))")])
        .await
        .map_err(|e| {
            log_google_failure("タブ名の読み取り", e);
            google_err(e, g.client_email(), false)
        })?;
    let title = sheet::tab_title(&resp, sheet::ALLOWED_SHEET_ID)
        .ok_or_else(|| ApiErr::new(StatusCode::NOT_FOUND, "そのタブが見つかりません"))?;
    if let Ok(mut t) = ctx.title.lock() {
        *t = Some(title.clone());
    }
    Ok(title)
}

async fn read_headers(g: &GoogleSa, ctx: &Ctx) -> Result<(String, SheetHeaders), ApiErr> {
    let title = tab_title(g, ctx).await?;
    let url = format!("{SHEETS_API}/{}", sheet::ALLOWED_SPREADSHEET_ID);
    let range = sheet::a1(&title, &format!("A{HEADER_ROW}:ZZ{HEADER_ROW}"));
    let resp = g
        .get(
            &url,
            &[
                ("ranges", range.as_str()),
                ("includeGridData", "true"),
                (
                    "fields",
                    "sheets(properties(sheetId),merges,data(rowData(values(formattedValue))))",
                ),
            ],
        )
        .await
        .map_err(|e| {
            log_google_failure("見出しの読み取り", e);
            google_err(e, g.client_email(), false)
        })?;
    let headers = sheet::header_map(&resp, sheet::ALLOWED_SHEET_ID)
        .ok_or_else(|| ApiErr::new(StatusCode::NOT_FOUND, "そのタブが見つかりません"))?;
    Ok((title, headers))
}

async fn drive_modified(g: &GoogleSa, ctx: &Ctx, fresh: bool) -> Result<String, ApiErr> {
    if !fresh {
        if let Some((at, m)) = ctx.version.lock().ok().and_then(|v| v.clone()) {
            if at.elapsed() < VERSION_TTL {
                return Ok(m);
            }
        }
    }
    let url = format!("{DRIVE_API}/{}", sheet::ALLOWED_SPREADSHEET_ID);
    let resp = g
        .get(
            &url,
            &[("fields", "modifiedTime"), ("supportsAllDrives", "true")],
        )
        .await
        .map_err(|e| {
            log_google_failure("更新時刻の読み取り", e);
            google_err(e, g.client_email(), false)
        })?;
    let m = resp
        .get("modifiedTime")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    if let Ok(mut v) = ctx.version.lock() {
        *v = Some((Instant::now(), m.clone()));
    }
    Ok(m)
}

async fn read_cell(g: &GoogleSa, title: &str, col: usize, row: usize) -> Result<String, ApiErr> {
    let range = sheet::a1(title, &format!("{}{row}", sheet::col_letter(col)));
    let url = format!(
        "{SHEETS_API}/{}/values/{}",
        sheet::ALLOWED_SPREADSHEET_ID,
        urlencoding::encode(&range)
    );
    let resp = g.get(&url, &[]).await.map_err(|e| {
        log_google_failure("マスの読み直し", e);
        google_err(e, g.client_email(), false)
    })?;
    Ok(sheet::single_value(&resp))
}

// ---- ハンドラ ----

#[derive(Deserialize)]
struct SheetQuery {
    sid: Option<String>,
    gid: Option<i64>,
}

async fn get_sheet(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    Query(q): Query<SheetQuery>,
) -> Response {
    if let Err(d) = authorize(&session, &state, &ctx).await {
        return d.into_response();
    }
    let (Some(sid), Some(gid)) = (q.sid, q.gid) else {
        return ApiErr::new(StatusCode::BAD_REQUEST, "sid と gid が必要です").into_response();
    };
    match read_sheet(&ctx, &sid, gid).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn read_sheet(ctx: &Ctx, sid: &str, gid: i64) -> Result<Value, ApiErr> {
    require_sheet(sid, Some(gid))?;
    let g = require_google(ctx)?;
    let (title, headers) = read_headers(&g, ctx).await?;
    let url = format!("{SHEETS_API}/{}", sheet::ALLOWED_SPREADSHEET_ID);
    let range = sheet::a1(&title, &format!("A{FIRST_APPLICANT_ROW}:ZZ"));
    let resp = g
        .get(
            &url,
            &[
                ("ranges", range.as_str()),
                ("includeGridData", "true"),
                (
                    "fields",
                    "sheets(properties(sheetId),data(startRow,rowData(values(formattedValue,dataValidation))))",
                ),
            ],
        )
        .await
        .map_err(|e| {
            log_google_failure("シートの読み取り", e);
            google_err(e, g.client_email(), false)
        })?;
    let rows = sheet::extract_rows(&resp, sheet::ALLOWED_SHEET_ID, &headers);
    let modified = drive_modified(&g, ctx, true).await?;
    tracing::info!("portal-test: シートの読み取り（{} 行）", rows.rows.len());
    Ok(json!({
        "headers": headers.positions,
        "rows": rows.rows,
        "options": rows.options,
        "readAt": chrono::Utc::now().to_rfc3339(),
        "modifiedTime": modified,
    }))
}

async fn get_version(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    Query(q): Query<SheetQuery>,
) -> Response {
    if let Err(d) = authorize(&session, &state, &ctx).await {
        return d.into_response();
    }
    let result = async {
        require_sheet(q.sid.as_deref().unwrap_or(""), q.gid)?;
        let g = require_google(&ctx)?;
        drive_modified(&g, &ctx, false).await
    }
    .await;
    match result {
        Ok(m) => Json(json!({ "modifiedTime": m })).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn get_sa(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
) -> Response {
    if let Err(d) = authorize(&session, &state, &ctx).await {
        return d.into_response();
    }
    match require_google(&ctx) {
        Ok(g) => Json(json!({ "clientEmail": g.client_email() })).into_response(),
        Err(e) => e.into_response(),
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CellReq {
    sid: String,
    gid: i64,
    row_number: usize,
    header: String,
    base: String,
    next: String,
}

fn parse_cell(body: &[u8]) -> Result<CellReq, ApiErr> {
    let req: CellReq = serde_json::from_slice(body)
        .map_err(|_| ApiErr::new(StatusCode::BAD_REQUEST, "入力の形が違います"))?;
    if req.header.chars().count() > MAX_HEADER_CHARS
        || req.base.chars().count() > MAX_VALUE_CHARS
        || req.next.chars().count() > MAX_VALUE_CHARS
        || req.row_number > MAX_ROW
    {
        return Err(ApiErr::new(StatusCode::BAD_REQUEST, "入力が長すぎます"));
    }
    Ok(req)
}

async fn post_cell(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    body: Bytes,
) -> Response {
    if let Err(d) = authorize(&session, &state, &ctx).await {
        return d.into_response();
    }
    let requested_with = headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        .is_some_and(|v| {
            v.trim()
                .eq_ignore_ascii_case(crate::CSRF_REQUESTED_WITH_VALUE)
        });
    if !requested_with {
        return ApiErr::new(StatusCode::FORBIDDEN, "書き込みの確認に失敗しました").into_response();
    }
    match write_cell(&ctx, &body).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => e.into_response(),
    }
}

async fn write_cell(ctx: &Ctx, body: &[u8]) -> Result<Value, ApiErr> {
    let req = parse_cell(body)?;
    require_sheet(&req.sid, Some(req.gid))?;
    sheet::check_writable(&req.header, req.row_number)
        .map_err(|m| ApiErr::new(StatusCode::FORBIDDEN, m))?;
    let g = require_google(ctx)?;
    let (title, headers) = read_headers(&g, ctx).await?;
    let Some(&col) = headers.positions.get(&req.header) else {
        return Err(ApiErr::new(
            StatusCode::NOT_FOUND,
            "その見出しがシートにありません",
        ));
    };
    let header = req.header.as_str();
    let _guard = ctx.write_lock.lock().await;
    let current = read_cell(&g, &title, col, req.row_number).await?;
    if let Decision::Conflict(latest) = sheet::decide(&current, &req.base) {
        tracing::info!("portal-test: 「{header}」 競合（書かなかった）");
        return Ok(json!({ "kind": "conflict", "latest": latest }));
    }
    let range = sheet::a1(
        &title,
        &format!("{}{}", sheet::col_letter(col), req.row_number),
    );
    let url = format!(
        "{SHEETS_API}/{}/values/{}",
        sheet::ALLOWED_SPREADSHEET_ID,
        urlencoding::encode(&range)
    );
    let put_body = json!({ "range": range, "values": [[req.next]] });
    if let Err(e) = g.put(&url, &[("valueInputOption", "RAW")], &put_body).await {
        log_google_failure(&format!("「{header}」 保存"), e);
        return Err(google_err(e, g.client_email(), true));
    }
    if let Ok(mut v) = ctx.version.lock() {
        *v = None;
    }
    tracing::info!("portal-test: 「{header}」 保存");
    Ok(json!({ "kind": "saved", "value": req.next }))
}

#[cfg(test)]
mod tests;
