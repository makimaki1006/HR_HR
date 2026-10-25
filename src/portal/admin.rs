//! 社内の管理 API `/api/admin/portal/*` (計画 10 章 PR1・PR2)。
//!
//! 認可は CRM と同じ (会社の Google ログイン + 会社ドメイン + 無効化されていないアカウント) に加えて管理者だけ。
//! 顧客の Cookie は見ない。書き込みは `X-Requested-With: fetch` と Origin の確認 (mod.rs の csrf_guard)。
//! 応答は `Cache-Control: no-store`。シートを試しに読む応答には応募者の個人情報が入るので、ログには値を出さない
//! (件数と結果だけ)。
#![allow(clippy::result_large_err)]

use std::sync::Arc;
use std::time::Duration;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Extension, Json, Router};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use ts_rs::TS;

use super::auth::{AdminError, PortalAuth};
use super::sheets::{self, ProbeError, SheetsApi};
use super::store::{
    ColumnGroup, ColumnKind, ColumnSetting, Customer, CustomerStage, LogEntry, LogSource, SheetReg,
    StoreError,
};
use super::{admin_account, err, invite_path, Ctx, PortalAdminAccount};
use crate::crm::rbac;
use crate::AppState;

/// 書き込みのあと、社内用シートに書けるまで待つ最長の時間
const PERSIST_WAIT: Duration = Duration::from_secs(8);
const MAX_NAME_CHARS: usize = 100;
const MAX_LABEL_CHARS: usize = 50;
const MAX_HEADER_CHARS: usize = 100;
const MAX_COLUMNS: usize = 200;

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/admin/portal/status", get(status))
        .route(
            "/api/admin/portal/accounts",
            get(list_accounts).post(create_account),
        )
        .route("/api/admin/portal/accounts/{id}/invite", post(invite))
        .route("/api/admin/portal/accounts/{id}/disable", post(disable))
        .route("/api/admin/portal/accounts/{id}/enable", post(enable))
        .route(
            "/api/admin/portal/accounts/{id}/extra-email",
            put(extra_email),
        )
        .route(
            "/api/admin/portal/customers",
            get(list_customers).post(create_customer),
        )
        .route(
            "/api/admin/portal/customers/{id}",
            get(get_customer).put(update_customer),
        )
        .route("/api/admin/portal/customers/{id}/sheets", post(add_sheet))
        .route("/api/admin/portal/sheet-probe", post(probe))
        .route(
            "/api/admin/portal/sheet-preview",
            post(preview_unregistered),
        )
        .route("/api/admin/portal/sheets/{id}", put(update_sheet))
        .route("/api/admin/portal/sheets/{id}/preview", post(preview_sheet))
        .route(
            "/api/admin/portal/sheets/{id}/columns",
            get(get_columns).put(put_columns),
        )
        .route("/api/admin/portal/activity", get(activity))
}

// ---- 応答の型 (ts-rs) ----

#[derive(Serialize, TS)]
pub struct PortalAdminStatus {
    /// `sheet` (社内用シート) / `memory` (手元の確認用) / `none` (未設定)
    pub store_kind: String,
    pub ready: bool,
    /// 社内用シートにまだ書けていない変更の数
    pub pending: u32,
    pub last_error: Option<String>,
    pub loaded_at: Option<String>,
    pub last_saved_at: Option<String>,
    /// 顧客がログインできる準備 (保存先とメールの送信) が揃っているか
    pub customer_login: bool,
    /// 顧客のシートを読むサービスアカウントのアドレス (シートに編集者として追加してもらう)。未設定なら null
    pub reader_email: Option<String>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminCustomer {
    pub id: String,
    pub name: String,
    pub hubspot_company_id: Option<String>,
    /// `sheet_only` / `both` / `portal_only`
    pub stage: String,
    pub disabled: bool,
    pub created_at: String,
    pub updated_at: String,
    pub sheet_count: u32,
    pub account_count: u32,
    pub active_account_count: u32,
    pub last_login_at: Option<String>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminCustomers {
    pub customers: Vec<PortalAdminCustomer>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminSheet {
    pub id: String,
    pub customer_id: String,
    pub spreadsheet_id: String,
    #[ts(type = "number")]
    pub sheet_id: i64,
    /// シートを開く URL (社内の人が確かめる用)
    pub url: String,
    pub tab_title: String,
    pub label: String,
    pub header_row: u32,
    pub example_rows: u32,
    pub customer_can_edit_columns: bool,
    pub disabled: bool,
    /// 顧客に見せる項目の数
    pub visible_columns: u32,
    pub created_at: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminCustomerDetail {
    pub customer: PortalAdminCustomer,
    pub sheets: Vec<PortalAdminSheet>,
    pub accounts: Vec<PortalAdminAccount>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminTab {
    #[ts(type = "number")]
    pub sheet_id: i64,
    pub title: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminProbe {
    pub spreadsheet_id: String,
    /// URL に入っていたタブの番号
    #[ts(type = "number | null")]
    pub gid: Option<i64>,
    pub tabs: Vec<PortalAdminTab>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminPreview {
    pub tab_title: String,
    pub header_row: u32,
    pub example_rows: u32,
    pub headers: Vec<String>,
    pub above: Vec<Vec<String>>,
    pub examples: Vec<Vec<String>>,
    pub rows: Vec<Vec<String>>,
    pub read_at: String,
}

#[derive(Serialize, Deserialize, TS, Clone)]
#[serde(deny_unknown_fields)]
pub struct PortalAdminColumn {
    pub header: String,
    pub label: String,
    pub visible: bool,
    pub writable: bool,
    pub in_list: bool,
    pub multiline: bool,
    /// `text` / `date` / `datetime` / `phone` / `email` / `choice`
    pub kind: String,
    /// `summary` / `customer` / `staff` / `other`
    pub group: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminColumns {
    pub sheet: PortalAdminSheet,
    pub columns: Vec<PortalAdminColumn>,
}

#[derive(Serialize, TS)]
pub struct PortalAdminLog {
    pub at: String,
    pub actor: String,
    /// `admin` / `portal` / `system`
    pub source: String,
    pub kind: String,
    pub customer_id: String,
    pub sheet_reg_id: String,
    pub account_id: String,
    pub column: String,
    pub before: String,
    pub after: String,
    pub detail: String,
}

#[derive(Serialize, TS)]
pub struct PortalAdminLogs {
    pub entries: Vec<PortalAdminLog>,
}

// ---- 認可 ----

/// 管理者か。保存先の有無は見ない
#[allow(clippy::result_large_err)]
async fn admin_email(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &Ctx,
) -> Result<String, Response> {
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
    Ok(principal.email.unwrap_or_default())
}

/// 管理者で、保存先が読み込み済み
#[allow(clippy::result_large_err)]
async fn gate(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &Ctx,
) -> Result<(Arc<PortalAuth>, String), Response> {
    let by = admin_email(session, state, ctx).await?;
    let auth = ctx
        .auth
        .clone()
        .filter(|a| a.store().ready())
        .ok_or_else(store_unavailable)?;
    Ok((auth, by))
}

fn store_unavailable() -> Response {
    err(
        StatusCode::SERVICE_UNAVAILABLE,
        "store_unavailable",
        "社内用シートをまだ読めていません。少し待ってから開き直してください",
    )
}

macro_rules! gate {
    ($session:expr, $state:expr, $ctx:expr) => {
        match gate(&$session, &$state, &$ctx).await {
            Ok(v) => v,
            Err(r) => return r,
        }
    };
}

fn log(auth: &PortalAuth, by: &str, kind: &str, f: impl FnOnce(&mut LogEntry)) {
    let mut e = LogEntry::new(Utc::now(), by, LogSource::Admin, kind);
    f(&mut e);
    auth.store().append_log(e);
}

async fn persist(auth: &PortalAuth) {
    if !auth.store().persisted(PERSIST_WAIT).await {
        tracing::warn!("[portal] 社内用シートへの保存が待ちきれなかった (裏で再試行する)");
    }
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

/// 本文は認可のあとで読む (管理者でない人に、入力の形の誤りを先に返さない)
#[allow(clippy::result_large_err)]
fn parse_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, Response> {
    serde_json::from_slice(body)
        .map_err(|_| err(StatusCode::BAD_REQUEST, "bad_request", "入力の形が違います"))
}

fn not_found(what: &str) -> Response {
    err(
        StatusCode::NOT_FOUND,
        "not_found",
        &format!("{what}が見つかりません"),
    )
}

fn invalid(message: &str) -> Response {
    err(StatusCode::UNPROCESSABLE_ENTITY, "invalid", message)
}

fn clean_text(raw: &str, max: usize) -> Option<String> {
    let t = raw.trim();
    (!t.is_empty() && t.chars().count() <= max && !t.chars().any(char::is_control))
        .then(|| t.to_string())
}

// ---- 状態 ----

async fn status(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    if let Err(r) = admin_email(&session, &state, &ctx).await {
        return r;
    }
    let (kind, st) = match &ctx.auth {
        Some(a) => {
            let st = a.store().status();
            (st.kind.to_string(), Some(st))
        }
        None => ("none".to_string(), None),
    };
    Json(PortalAdminStatus {
        store_kind: kind,
        ready: st.as_ref().is_some_and(|s| s.ready),
        pending: st.as_ref().map_or(0, |s| s.pending as u32),
        last_error: st.as_ref().and_then(|s| s.last_error.clone()),
        loaded_at: st
            .as_ref()
            .and_then(|s| s.loaded_at)
            .map(|t| t.to_rfc3339()),
        last_saved_at: st
            .as_ref()
            .and_then(|s| s.last_saved_at)
            .map(|t| t.to_rfc3339()),
        customer_login: ctx.portal().is_some(),
        reader_email: ctx.reader.as_ref().map(|r| r.client_email().to_string()),
    })
    .into_response()
}

// ---- アカウント ----

async fn list_accounts(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let (auth, _) = gate!(session, state, ctx);
    let accounts = auth.store().accounts().iter().map(admin_account).collect();
    Json(super::PortalAdminAccounts { accounts }).into_response()
}

#[derive(Deserialize)]
struct CreateAccountBody {
    email: String,
    customer_id: String,
}

async fn create_account(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: CreateAccountBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if auth.store().customer(body.customer_id.trim()).is_none() {
        return admin_error(AdminError::BadCustomer);
    }
    match auth.create_account(&body.email, &body.customer_id, Utc::now()) {
        Ok((a, token)) => {
            log(&auth, &by, "account_invite", |e| {
                e.customer_id = a.customer_id.clone();
                e.account_id = a.id.clone();
                e.detail = "アカウントを作り、招待のリンクを出した".into();
            });
            persist(&auth).await;
            tracing::info!("[portal] アカウントを作った (by {by})");
            Json(super::PortalAdminInvite {
                account: admin_account(&a),
                invite_path: invite_path(&token),
            })
            .into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn invite(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let result = auth.issue_invite(&id, Utc::now()).and_then(|t| {
        auth.store()
            .account(&id)
            .map(|a| (a, t))
            .ok_or(AdminError::NotFound)
    });
    match result {
        Ok((a, token)) => {
            log(&auth, &by, "account_invite", |e| {
                e.customer_id = a.customer_id.clone();
                e.account_id = a.id.clone();
                e.detail = "招待のリンクを出し直した (前のリンクは使えない)".into();
            });
            persist(&auth).await;
            tracing::info!("[portal] 招待のリンクを出した (by {by})");
            Json(super::PortalAdminInvite {
                account: admin_account(&a),
                invite_path: invite_path(&token),
            })
            .into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn disable(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    match auth.disable(&id, Utc::now()) {
        Ok(a) => {
            log(&auth, &by, "account_disable", |e| {
                e.customer_id = a.customer_id.clone();
                e.account_id = a.id.clone();
                e.detail = "アカウントを止めた (覚えた端末・ログイン中の画面も無効)".into();
            });
            persist(&auth).await;
            tracing::info!("[portal] アカウントを止めた (by {by})");
            Json(admin_account(&a)).into_response()
        }
        Err(e) => admin_error(e),
    }
}

async fn enable(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    match auth.enable(&id) {
        Ok(a) => {
            log(&auth, &by, "account_enable", |e| {
                e.customer_id = a.customer_id.clone();
                e.account_id = a.id.clone();
                e.detail = "アカウントの停止を戻した".into();
            });
            persist(&auth).await;
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

async fn extra_email(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: ExtraEmailBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    match auth.set_extra_email(&id, &body.email) {
        Ok(a) => {
            log(&auth, &by, "account_extra_email", |e| {
                e.customer_id = a.customer_id.clone();
                e.account_id = a.id.clone();
                e.detail = "確認コードの追加の送り先を変えた".into();
            });
            persist(&auth).await;
            Json(admin_account(&a)).into_response()
        }
        Err(e) => admin_error(e),
    }
}

// ---- 顧客 ----

fn customer_view(auth: &PortalAuth, c: &Customer) -> PortalAdminCustomer {
    let store = auth.store();
    let accounts: Vec<_> = store
        .accounts()
        .into_iter()
        .filter(|a| a.customer_id == c.id)
        .collect();
    PortalAdminCustomer {
        id: c.id.clone(),
        name: c.name.clone(),
        hubspot_company_id: c.hubspot_company_id.clone(),
        stage: c.stage.as_str().into(),
        disabled: c.disabled,
        created_at: c.created_at.to_rfc3339(),
        updated_at: c.updated_at.to_rfc3339(),
        sheet_count: store.sheets(Some(&c.id)).len() as u32,
        account_count: accounts.len() as u32,
        active_account_count: accounts
            .iter()
            .filter(|a| a.status == super::store::AccountStatus::Active)
            .count() as u32,
        last_login_at: accounts
            .iter()
            .filter_map(|a| a.last_login_at)
            .max()
            .map(|t| t.to_rfc3339()),
    }
}

fn sheet_view(auth: &PortalAuth, s: &SheetReg) -> PortalAdminSheet {
    PortalAdminSheet {
        id: s.id.clone(),
        customer_id: s.customer_id.clone(),
        spreadsheet_id: s.spreadsheet_id.clone(),
        sheet_id: s.sheet_id,
        url: format!(
            "https://docs.google.com/spreadsheets/d/{}/edit#gid={}",
            s.spreadsheet_id, s.sheet_id
        ),
        tab_title: s.tab_title.clone(),
        label: s.label.clone(),
        header_row: s.header_row,
        example_rows: s.example_rows,
        customer_can_edit_columns: s.customer_can_edit_columns,
        disabled: s.disabled,
        visible_columns: auth
            .store()
            .columns(&s.id)
            .iter()
            .filter(|c| c.visible)
            .count() as u32,
        created_at: s.created_at.to_rfc3339(),
    }
}

async fn list_customers(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
) -> Response {
    let (auth, _) = gate!(session, state, ctx);
    let customers = auth
        .store()
        .customers()
        .iter()
        .map(|c| customer_view(&auth, c))
        .collect();
    Json(PortalAdminCustomers { customers }).into_response()
}

fn new_id(prefix: &str) -> String {
    let u = uuid::Uuid::new_v4().simple().to_string();
    format!("{prefix}-{}", &u[..12])
}

fn clean_hubspot_id(raw: &str) -> Result<Option<String>, Response> {
    let t = raw.trim();
    if t.is_empty() {
        return Ok(None);
    }
    if t.len() <= 20 && t.bytes().all(|b| b.is_ascii_digit()) {
        Ok(Some(t.to_string()))
    } else {
        Err(invalid("HubSpot の会社 ID は数字で入れてください"))
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateCustomerBody {
    name: String,
    #[serde(default)]
    hubspot_company_id: String,
}

async fn create_customer(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: CreateCustomerBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(name) = clean_text(&body.name, MAX_NAME_CHARS) else {
        return invalid("顧客名を 100 文字以内で入れてください");
    };
    let hubspot = match clean_hubspot_id(&body.hubspot_company_id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let now = Utc::now();
    let c = Customer {
        id: new_id("c"),
        name,
        hubspot_company_id: hubspot,
        stage: CustomerStage::SheetOnly,
        disabled: false,
        created_at: now,
        updated_at: now,
    };
    if auth.store().insert_customer(c.clone()).is_err() {
        return err(StatusCode::CONFLICT, "duplicate", "もう一度お試しください");
    }
    log(&auth, &by, "customer_add", |e| {
        e.customer_id = c.id.clone();
        e.detail = "顧客を追加した".into();
    });
    persist(&auth).await;
    Json(customer_view(&auth, &c)).into_response()
}

async fn get_customer(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, _) = gate!(session, state, ctx);
    let Some(c) = auth.store().customer(&id) else {
        return not_found("顧客");
    };
    let sheets = auth
        .store()
        .sheets(Some(&id))
        .iter()
        .map(|s| sheet_view(&auth, s))
        .collect();
    let accounts = auth
        .store()
        .accounts()
        .iter()
        .filter(|a| a.customer_id == id)
        .map(admin_account)
        .collect();
    Json(PortalAdminCustomerDetail {
        customer: customer_view(&auth, &c),
        sheets,
        accounts,
    })
    .into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateCustomerBody {
    name: Option<String>,
    hubspot_company_id: Option<String>,
    stage: Option<String>,
    disabled: Option<bool>,
}

fn stage_label(s: CustomerStage) -> &'static str {
    match s {
        CustomerStage::SheetOnly => "シートだけ",
        CustomerStage::Both => "両方",
        CustomerStage::PortalOnly => "画面だけ",
    }
}

async fn update_customer(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: UpdateCustomerBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(old) = auth.store().customer(&id) else {
        return not_found("顧客");
    };
    let mut c = old.clone();
    if let Some(n) = &body.name {
        let Some(n) = clean_text(n, MAX_NAME_CHARS) else {
            return invalid("顧客名を 100 文字以内で入れてください");
        };
        c.name = n;
    }
    if let Some(h) = &body.hubspot_company_id {
        c.hubspot_company_id = match clean_hubspot_id(h) {
            Ok(v) => v,
            Err(r) => return r,
        };
    }
    if let Some(s) = &body.stage {
        let Some(stage) = CustomerStage::parse_known(s) else {
            return invalid("段階を選んでください");
        };
        if stage != CustomerStage::SheetOnly {
            let ready = auth
                .store()
                .sheets(Some(&id))
                .iter()
                .any(|s| !s.disabled && auth.store().columns(&s.id).iter().any(|c| c.visible));
            if !ready {
                return invalid("先にシートを登録して、顧客に見せる項目を設定してください");
            }
        }
        c.stage = stage;
    }
    if let Some(d) = body.disabled {
        c.disabled = d;
    }
    if c == old {
        return Json(customer_view(&auth, &c)).into_response();
    }
    c.updated_at = Utc::now();
    if auth.store().update_customer(c.clone()).is_err() {
        return not_found("顧客");
    }
    if c.disabled && !old.disabled {
        // 再開しても、止める前の端末・ログイン中の画面・招待は生き返らない
        let n = auth.revoke_customer_access(&c.id, Utc::now());
        log(&auth, &by, "customer_revoke", |e| {
            e.customer_id = c.id.clone();
            e.detail = format!(
                "顧客を止めたので、アカウント {n} 件の覚えた端末・ログイン中の画面・招待を無効にした"
            );
        });
    }
    let changes: [(&str, String, String); 4] = [
        ("顧客名", old.name.clone(), c.name.clone()),
        (
            "HubSpotの会社ID",
            old.hubspot_company_id.clone().unwrap_or_default(),
            c.hubspot_company_id.clone().unwrap_or_default(),
        ),
        (
            "段階",
            stage_label(old.stage).into(),
            stage_label(c.stage).into(),
        ),
        (
            "状態",
            if old.disabled { "停止" } else { "利用中" }.into(),
            if c.disabled { "停止" } else { "利用中" }.into(),
        ),
    ];
    for (column, before, after) in changes {
        if before != after {
            log(&auth, &by, "customer_change", |e| {
                e.customer_id = c.id.clone();
                e.column = column.into();
                e.before = before;
                e.after = after;
            });
        }
    }
    persist(&auth).await;
    tracing::info!("[portal] 顧客を変えた (by {by})");
    Json(customer_view(&auth, &c)).into_response()
}

// ---- シート ----

fn require_reader(ctx: &Ctx) -> Result<Arc<dyn SheetsApi>, Response> {
    ctx.reader.clone().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "reader_unavailable",
            "顧客のシートを読むための鍵がまだ設定されていません",
        )
    })
}

fn probe_error(e: ProbeError, reader: &dyn SheetsApi) -> Response {
    match e {
        ProbeError::NotShared => (
            StatusCode::FORBIDDEN,
            Json(serde_json::json!({
                "error": "sheet_not_shared",
                "message": format!(
                    "シートを開けません。このアドレスをシートに編集者として追加してください: {}",
                    reader.client_email()
                ),
                "client_email": reader.client_email(),
            })),
        )
            .into_response(),
        ProbeError::TabNotFound => err(
            StatusCode::NOT_FOUND,
            "tab_not_found",
            "そのタブがシートに見つかりません",
        ),
        ProbeError::Google(e) => {
            super::log_google("顧客のシートの読み取り", e);
            err(
                StatusCode::BAD_GATEWAY,
                "google_failed",
                &super::sheet_store::describe(e).replace("社内用シート", "シート"),
            )
        }
    }
}

/// 顧客のシートの URL を読む。社内用シート (アプリの情報) は、試しに読む・登録することを拒む
fn parse_url(ctx: &Ctx, url: &str) -> Result<(String, Option<i64>), Response> {
    let (id, gid) = sheets::parse_sheet_url(url).ok_or_else(|| {
        invalid("Google スプレッドシートの URL を貼ってください（https://docs.google.com/spreadsheets/d/… の形）")
    })?;
    if ctx.store_spreadsheet.as_deref() == Some(id.as_str()) {
        return Err(invalid(
            "このシートはアプリの情報を置く社内用のシートです。顧客のシートとしては読めません",
        ));
    }
    Ok((id, gid))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProbeBody {
    url: String,
}

async fn probe(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    body: Bytes,
) -> Response {
    let (_auth, _) = gate!(session, state, ctx);
    let body: ProbeBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (spreadsheet_id, gid) = match parse_url(&ctx, &body.url) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let reader = match require_reader(&ctx) {
        Ok(r) => r,
        Err(r) => return r,
    };
    match reader.tabs(&spreadsheet_id).await {
        Ok(tabs) => Json(PortalAdminProbe {
            spreadsheet_id,
            gid,
            tabs: tabs
                .into_iter()
                .map(|t| PortalAdminTab {
                    sheet_id: t.sheet_id,
                    title: t.title,
                })
                .collect(),
        })
        .into_response(),
        Err(e) => probe_error(e.into(), reader.as_ref()),
    }
}

fn check_rows(header_row: u32, example_rows: u32) -> Result<(), Response> {
    if !(1..=sheets::MAX_HEADER_ROW).contains(&header_row) {
        return Err(invalid("見出しの行は 1〜20 で入れてください"));
    }
    if example_rows > sheets::MAX_EXAMPLE_ROWS {
        return Err(invalid("記入例の行数は 0〜10 で入れてください"));
    }
    Ok(())
}

fn preview_view(p: sheets::Preview, header_row: u32, example_rows: u32) -> PortalAdminPreview {
    PortalAdminPreview {
        tab_title: p.tab_title,
        header_row,
        example_rows,
        headers: p.headers,
        above: p.above,
        examples: p.examples,
        rows: p.rows,
        read_at: Utc::now().to_rfc3339(),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewBody {
    url: String,
    sheet_id: i64,
    header_row: u32,
    #[serde(default)]
    example_rows: u32,
}

/// 登録の前に、選んだタブを試しに読む
async fn preview_unregistered(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: PreviewBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (spreadsheet_id, _) = match parse_url(&ctx, &body.url) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if let Err(r) = check_rows(body.header_row, body.example_rows) {
        return r;
    }
    let reader = match require_reader(&ctx) {
        Ok(r) => r,
        Err(r) => return r,
    };
    match sheets::read_preview(
        reader.as_ref(),
        &spreadsheet_id,
        body.sheet_id,
        body.header_row,
        body.example_rows,
    )
    .await
    {
        Ok(p) => {
            tracing::info!(
                "[portal] 登録前のシートを試しに読んだ（{} 行）",
                p.rows.len()
            );
            log(&auth, &by, "sheet_preview", |e| {
                e.detail = format!(
                    "登録前のシートを試しに読んだ（スプレッドシート {spreadsheet_id}・タブ {}「{}」）",
                    body.sheet_id, p.tab_title
                );
            });
            Json(preview_view(p, body.header_row, body.example_rows)).into_response()
        }
        Err(e) => probe_error(e, reader.as_ref()),
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddSheetBody {
    url: String,
    sheet_id: i64,
    label: String,
    header_row: u32,
    #[serde(default)]
    example_rows: u32,
}

async fn add_sheet(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(customer_id): Path<String>,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: AddSheetBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    if auth.store().customer(&customer_id).is_none() {
        return not_found("顧客");
    }
    let (spreadsheet_id, _) = match parse_url(&ctx, &body.url) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(label) = clean_text(&body.label, MAX_LABEL_CHARS) else {
        return invalid("顧客に見せる名前を 50 文字以内で入れてください");
    };
    if let Err(r) = check_rows(body.header_row, body.example_rows) {
        return r;
    }
    let reader = match require_reader(&ctx) {
        Ok(r) => r,
        Err(r) => return r,
    };
    // 登録の前に試しに読み、開けること・タブがあることを確かめる (計画 4.1)
    let preview = match sheets::read_preview(
        reader.as_ref(),
        &spreadsheet_id,
        body.sheet_id,
        body.header_row,
        body.example_rows,
    )
    .await
    {
        Ok(p) => p,
        Err(e) => return probe_error(e, reader.as_ref()),
    };
    let now = Utc::now();
    let sheet = SheetReg {
        id: new_id("s"),
        customer_id: customer_id.clone(),
        spreadsheet_id,
        sheet_id: body.sheet_id,
        tab_title: preview.tab_title.clone(),
        label,
        header_row: body.header_row,
        example_rows: body.example_rows,
        customer_can_edit_columns: false,
        disabled: false,
        created_at: now,
        updated_at: now,
    };
    match auth.store().insert_sheet(sheet.clone()) {
        Ok(()) => {}
        Err(StoreError::Duplicate) => {
            return err(
                StatusCode::CONFLICT,
                "duplicate",
                "このシートのこのタブは、すでに登録されています",
            )
        }
        Err(StoreError::NotFound) => return not_found("顧客"),
    }
    // 項目は見出しから作り、最初は全部「見せない」(設定するまで顧客には何も出ない)
    let columns = columns_from_headers(&sheet.id, &preview.headers, &[]);
    let n = columns.len();
    auth.store().replace_columns(&sheet.id, columns);
    log(&auth, &by, "sheet_add", |e| {
        e.customer_id = customer_id.clone();
        e.sheet_reg_id = sheet.id.clone();
        e.detail = format!("シートを登録した（項目 {n} 個）");
    });
    persist(&auth).await;
    tracing::info!("[portal] シートを登録した（項目 {n} 個, by {by}）");
    Json(sheet_view(&auth, &sheet)).into_response()
}

/// 見出しの行から項目を作る。前の設定 (`old`) に同じ見出しがあればその設定を引き継ぎ、
/// 無い見出しは「見せない」で足す。シートに無くなった見出しの項目は消える
fn columns_from_headers(
    sheet_id: &str,
    headers: &[String],
    old: &[ColumnSetting],
) -> Vec<ColumnSetting> {
    let mut seen = std::collections::HashSet::new();
    headers
        .iter()
        .map(|h| sheets::clean_header(h))
        .filter(|h| {
            !h.is_empty() && h.chars().count() <= MAX_HEADER_CHARS && seen.insert(h.clone())
        })
        .take(MAX_COLUMNS)
        .enumerate()
        .map(
            |(i, header)| match old.iter().find(|c| c.header == header) {
                Some(c) => ColumnSetting {
                    order: i as u32,
                    ..c.clone()
                },
                None => ColumnSetting {
                    sheet_reg_id: sheet_id.to_string(),
                    header,
                    label: String::new(),
                    visible: false,
                    writable: false,
                    in_list: false,
                    multiline: false,
                    kind: ColumnKind::Text,
                    group: ColumnGroup::Other,
                    order: i as u32,
                },
            },
        )
        .collect()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct UpdateSheetBody {
    label: Option<String>,
    header_row: Option<u32>,
    example_rows: Option<u32>,
    customer_can_edit_columns: Option<bool>,
    disabled: Option<bool>,
}

async fn update_sheet(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: UpdateSheetBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(old) = auth.store().sheet(&id) else {
        return not_found("シート");
    };
    let mut s = old.clone();
    if let Some(l) = &body.label {
        let Some(l) = clean_text(l, MAX_LABEL_CHARS) else {
            return invalid("顧客に見せる名前を 50 文字以内で入れてください");
        };
        s.label = l;
    }
    s.header_row = body.header_row.unwrap_or(s.header_row);
    s.example_rows = body.example_rows.unwrap_or(s.example_rows);
    if let Err(r) = check_rows(s.header_row, s.example_rows) {
        return r;
    }
    s.customer_can_edit_columns = body
        .customer_can_edit_columns
        .unwrap_or(s.customer_can_edit_columns);
    s.disabled = body.disabled.unwrap_or(s.disabled);
    if s == old {
        return Json(sheet_view(&auth, &s)).into_response();
    }
    // 見出しの行を変えたら、その行を読んで項目を作り直す (同じ見出しの設定は引き継ぐ)。
    // 読めなければ何も変えない (古い見出しの項目のまま新しい行を読ませない)
    let mut rebuilt = None;
    if s.header_row != old.header_row {
        let reader = match require_reader(&ctx) {
            Ok(r) => r,
            Err(r) => return r,
        };
        match sheets::read_preview(
            reader.as_ref(),
            &s.spreadsheet_id,
            s.sheet_id,
            s.header_row,
            s.example_rows,
        )
        .await
        {
            Ok(p) => {
                s.tab_title = p.tab_title.clone();
                rebuilt = Some(columns_from_headers(
                    &s.id,
                    &p.headers,
                    &auth.store().columns(&s.id),
                ));
            }
            Err(e) => return probe_error(e, reader.as_ref()),
        }
    }
    s.updated_at = Utc::now();
    if auth.store().update_sheet(s.clone()).is_err() {
        return not_found("シート");
    }
    if let Some(cols) = rebuilt {
        let n = cols.len();
        auth.store().replace_columns(&s.id, cols);
        log(&auth, &by, "column_change", |e| {
            e.customer_id = s.customer_id.clone();
            e.sheet_reg_id = s.id.clone();
            e.detail = format!("見出しの行を変えたので、項目を作り直した（項目 {n} 個）");
        });
    }
    let yes_no = |b: bool| if b { "はい" } else { "いいえ" }.to_string();
    let changes: [(&str, String, String); 5] = [
        ("顧客に見せる名前", old.label.clone(), s.label.clone()),
        (
            "見出しの行",
            old.header_row.to_string(),
            s.header_row.to_string(),
        ),
        (
            "記入例の行数",
            old.example_rows.to_string(),
            s.example_rows.to_string(),
        ),
        (
            "顧客に項目の変更を許す",
            yes_no(old.customer_can_edit_columns),
            yes_no(s.customer_can_edit_columns),
        ),
        ("停止", yes_no(old.disabled), yes_no(s.disabled)),
    ];
    for (column, before, after) in changes {
        if before != after {
            log(&auth, &by, "sheet_change", |e| {
                e.customer_id = s.customer_id.clone();
                e.sheet_reg_id = s.id.clone();
                e.column = column.into();
                e.before = before;
                e.after = after;
            });
        }
    }
    persist(&auth).await;
    Json(sheet_view(&auth, &s)).into_response()
}

/// 登録したシートを試しに読む (見出しと先頭の数行)
async fn preview_sheet(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let Some(s) = auth.store().sheet(&id) else {
        return not_found("シート");
    };
    let reader = match require_reader(&ctx) {
        Ok(r) => r,
        Err(r) => return r,
    };
    match sheets::read_preview(
        reader.as_ref(),
        &s.spreadsheet_id,
        s.sheet_id,
        s.header_row,
        s.example_rows,
    )
    .await
    {
        Ok(p) => {
            // 読み取りの操作なので POST (CSRF を通す)。タブの名前が変わっていれば覚え直す
            if p.tab_title != s.tab_title {
                let mut s2 = s.clone();
                s2.tab_title = p.tab_title.clone();
                let _ = auth.store().update_sheet(s2);
            }
            tracing::info!("[portal] シートを試しに読んだ（{} 行）", p.rows.len());
            log(&auth, &by, "sheet_preview", |e| {
                e.customer_id = s.customer_id.clone();
                e.sheet_reg_id = s.id.clone();
                e.detail = format!(
                    "シートを試しに読んだ（スプレッドシート {}・タブ {}「{}」）",
                    s.spreadsheet_id, s.sheet_id, p.tab_title
                );
            });
            Json(preview_view(p, s.header_row, s.example_rows)).into_response()
        }
        Err(e) => probe_error(e, reader.as_ref()),
    }
}

fn column_view(c: &ColumnSetting) -> PortalAdminColumn {
    PortalAdminColumn {
        header: c.header.clone(),
        label: c.label.clone(),
        visible: c.visible,
        writable: c.writable,
        in_list: c.in_list,
        multiline: c.multiline,
        kind: c.kind.as_str().into(),
        group: c.group.as_str().into(),
    }
}

async fn get_columns(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    let (auth, _) = gate!(session, state, ctx);
    let Some(s) = auth.store().sheet(&id) else {
        return not_found("シート");
    };
    Json(PortalAdminColumns {
        sheet: sheet_view(&auth, &s),
        columns: auth.store().columns(&id).iter().map(column_view).collect(),
    })
    .into_response()
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutColumnsBody {
    columns: Vec<PortalAdminColumn>,
}

/// 項目の設定を確かめて変換する。書ける列は見せる列に限る
fn to_settings(sheet_id: &str, cols: &[PortalAdminColumn]) -> Result<Vec<ColumnSetting>, Response> {
    if cols.len() > MAX_COLUMNS {
        return Err(invalid("項目が多すぎます"));
    }
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::with_capacity(cols.len());
    for (i, c) in cols.iter().enumerate() {
        // 登録・項目の作成と同じに見出しをそろえる (セル内の改行は空白にする)
        let Some(header) = clean_text(&sheets::clean_header(&c.header), MAX_HEADER_CHARS) else {
            return Err(invalid("見出しが空、または長すぎる項目があります"));
        };
        if !seen.insert(header.clone()) {
            return Err(invalid("同じ見出しの項目が 2 つあります"));
        }
        let label = c.label.trim();
        if label.chars().count() > MAX_LABEL_CHARS || label.chars().any(char::is_control) {
            return Err(invalid("画面での名前は 50 文字以内で入れてください"));
        }
        let (Some(kind), Some(group)) = (ColumnKind::parse(&c.kind), ColumnGroup::parse(&c.group))
        else {
            return Err(invalid("項目の種類・まとまりを選んでください"));
        };
        if c.writable && !c.visible {
            return Err(invalid("見せない項目は書けるようにできません"));
        }
        out.push(ColumnSetting {
            sheet_reg_id: sheet_id.to_string(),
            header,
            label: label.to_string(),
            visible: c.visible,
            writable: c.writable,
            in_list: c.in_list && c.visible,
            multiline: c.multiline,
            kind,
            group,
            order: i as u32,
        });
    }
    Ok(out)
}

async fn put_columns(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Path(id): Path<String>,
    body: Bytes,
) -> Response {
    let (auth, by) = gate!(session, state, ctx);
    let body: PutColumnsBody = match parse_body(&body) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let Some(s) = auth.store().sheet(&id) else {
        return not_found("シート");
    };
    let settings = match to_settings(&id, &body.columns) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let before = auth.store().columns(&id);
    let summary = |cols: &[ColumnSetting]| {
        format!(
            "見せる {}・書ける {}",
            cols.iter().filter(|c| c.visible).count(),
            cols.iter().filter(|c| c.writable).count()
        )
    };
    let (b, a) = (summary(&before), summary(&settings));
    auth.store().replace_columns(&id, settings);
    log(&auth, &by, "column_change", |e| {
        e.customer_id = s.customer_id.clone();
        e.sheet_reg_id = s.id.clone();
        e.column = "項目の設定".into();
        e.before = b;
        e.after = a;
    });
    persist(&auth).await;
    Json(PortalAdminColumns {
        sheet: sheet_view(&auth, &s),
        columns: auth.store().columns(&id).iter().map(column_view).collect(),
    })
    .into_response()
}

// ---- 操作の記録 ----

#[derive(Deserialize)]
struct ActivityQuery {
    limit: Option<usize>,
}

async fn activity(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<Ctx>>,
    session: Session,
    Query(q): Query<ActivityQuery>,
) -> Response {
    let (auth, _) = gate!(session, state, ctx);
    let entries = auth
        .store()
        .recent_logs(q.limit.unwrap_or(200).min(500))
        .into_iter()
        .map(|e| PortalAdminLog {
            at: e.at.to_rfc3339(),
            actor: e.actor,
            source: e.source.as_str().into(),
            kind: e.kind,
            customer_id: e.customer_id,
            sheet_reg_id: e.sheet_reg_id,
            account_id: e.account_id,
            column: e.column,
            before: e.before,
            after: e.after,
            detail: e.detail,
        })
        .collect();
    Json(PortalAdminLogs { entries }).into_response()
}
