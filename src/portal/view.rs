//! 顧客の見る画面の API (計画 10 章 PR4)。読み取りだけ。
//!
//! - `GET /api/portal/sheets`               自分の顧客に登録されたシートの一覧
//! - `GET /api/portal/sheets/{id}/rows`     応募者の一覧 (見せる項目だけ)
//! - `GET /api/portal/sheets/{id}/version`  更新の確認 (Drive の最終更新時刻 + 項目の設定)。Sheets は読まない
//!
//! 権限: 顧客の Cookie のログイン中のアカウント (止めていない) で、顧客が使える状態 (止めておらず「シートだけ」でない)。
//! シートはその顧客に登録され、止めておらず、見せる項目が 1 つ以上あるものだけ。
//! それ以外 (別の顧客のシート・無い番号) はどれも同じ 404 にし、あるかどうかを明かさない。
//! ブラウザから受け取るのは登録の番号だけで、スプレッドシートの番号は受け取らない (計画 6 章)。
//! ログには件数と結果だけを出し、氏名・電話などの値は出さない。
#![allow(clippy::result_large_err)]
use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::{Extension, Json, Router};
use serde::Serialize;
use tokio::time::Instant;
use ts_rs::TS;

use super::live::{GridResult, LiveSheets, ReadError};
use super::sheets::{clean_header, GoogleError};
use super::store::{Account, ColumnSetting, SheetReg};
use super::{cookie, err, now, unauthorized, unavailable, Ctx, Portal, SESSION_COOKIE};
use crate::AppState;

/// 同じ人が同じシートを開いたことを記録に残す間隔 (30 秒おきの読み直しで記録を増やさない)
const VIEW_LOG_EVERY: Duration = Duration::from_secs(30 * 60);

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/portal/sheets", get(list_sheets))
        .route("/api/portal/sheets/{id}/rows", get(rows))
        .route("/api/portal/sheets/{id}/version", get(version))
}

// ---- 応答の型 (ts-rs) ----

#[derive(Serialize, TS)]
pub struct PortalSheet {
    /// 登録の番号 (スプレッドシートの番号ではない)
    pub id: String,
    /// 顧客に見せる名前
    pub label: String,
}

#[derive(Serialize, TS)]
pub struct PortalSheets {
    pub sheets: Vec<PortalSheet>,
}

#[derive(Serialize, TS)]
pub struct PortalColumn {
    /// 画面での名前 (未設定なら見出し)
    pub label: String,
    /// `text` / `choice` / `date` / `datetime` / `phone` / `email`
    pub kind: String,
    /// `summary` / `customer` / `staff` / `other`
    pub group: String,
    pub in_list: bool,
    pub multiline: bool,
}

#[derive(Serialize, TS)]
pub struct PortalApplicant {
    /// シートの行番号 (1 始まり)
    pub row: u32,
    /// `columns` と同じ並びの値
    pub cells: Vec<String>,
}

#[derive(Serialize, TS)]
pub struct PortalSheetRows {
    pub sheet: PortalSheet,
    /// 更新の確認で比べる値。`/version` が違う値を返したら読み直す
    pub version: String,
    /// シートから読んだ時刻
    pub read_at: String,
    /// シートを確かめられなかったので前に読んだ内容を返した
    pub stale: bool,
    pub columns: Vec<PortalColumn>,
    pub applicants: Vec<PortalApplicant>,
}

#[derive(Serialize, TS)]
pub struct PortalSheetVersion {
    pub version: String,
}

// ---- 閲覧の記録 ----

/// 閲覧の記録を (アカウント, シート) ごとに間引く
#[derive(Default)]
pub(super) struct ViewLog {
    last: Mutex<HashMap<(String, String), Instant>>,
}

impl ViewLog {
    fn should_log(&self, account: &str, sheet: &str) -> bool {
        let now = Instant::now();
        let mut g = self.last.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|_, t| now.duration_since(*t) < VIEW_LOG_EVERY);
        let key = (account.to_string(), sheet.to_string());
        if g.contains_key(&key) {
            return false;
        }
        g.insert(key, now);
        true
    }
}

// ---- 共通 ----

fn not_found() -> Response {
    err(StatusCode::NOT_FOUND, "not_found", "シートが見つかりません")
}

fn sheet_unavailable(e: ReadError) -> Response {
    let (code, message) = match e {
        ReadError::NotShared | ReadError::TabNotFound => (
            "sheet_unavailable",
            "シートを読めません。弊社の担当者にご連絡ください",
        ),
        ReadError::Google(_) => (
            "sheet_busy",
            "いまはシートを読めません。少し待ってから読み直してください",
        ),
    };
    log_read(e);
    err(StatusCode::SERVICE_UNAVAILABLE, code, message)
}

fn log_read(e: ReadError) {
    match e {
        ReadError::NotShared => {
            tracing::warn!("[portal] 顧客のシートを読めない（共有されていない）")
        }
        ReadError::TabNotFound => tracing::warn!("[portal] 顧客のシートのタブが無い"),
        ReadError::Google(GoogleError::Status(s)) => {
            tracing::warn!("[portal] 顧客のシートの読み取りの失敗（{s}）")
        }
        ReadError::Google(GoogleError::Network) => {
            tracing::warn!("[portal] 顧客のシートの読み取りの失敗（通信）")
        }
        ReadError::Google(GoogleError::Token) => {
            tracing::warn!("[portal] 顧客のシートの読み取りの失敗（認証）")
        }
    }
}

/// ログイン中の顧客。保存先・メールが揃っていなければ 503、ログインしていなければ 401
fn viewer(ctx: &Ctx, headers: &HeaderMap) -> Result<(Portal, Account), Response> {
    let Some(p) = ctx.portal() else {
        return Err(unavailable());
    };
    match cookie(headers, SESSION_COOKIE).and_then(|s| p.auth.current(s, now())) {
        Some(a) => Ok((p, a)),
        None => Err(unauthorized()),
    }
}

/// 見せる項目 (並びの順)
fn visible_columns(p: &Portal, sheet_id: &str) -> Vec<ColumnSetting> {
    let mut cols: Vec<ColumnSetting> = p
        .auth
        .store()
        .columns(sheet_id)
        .into_iter()
        .filter(|c| c.visible)
        .collect();
    cols.sort_by_key(|c| c.order);
    cols
}

/// その顧客が見てよいシートの登録 (見せる項目が 1 つ以上あるもの)
fn usable_sheets(ctx: &Ctx, p: &Portal, a: &Account) -> Vec<(SheetReg, Vec<ColumnSetting>)> {
    let mut list: Vec<(SheetReg, Vec<ColumnSetting>)> = p
        .auth
        .store()
        .sheets(Some(&a.customer_id))
        .into_iter()
        .filter(|s| s.customer_id == a.customer_id && !s.disabled)
        .filter(|s| ctx.store_spreadsheet.as_deref() != Some(s.spreadsheet_id.as_str()))
        .map(|s| {
            let cols = visible_columns(p, &s.id);
            (s, cols)
        })
        .filter(|(_, cols)| !cols.is_empty())
        .collect();
    list.sort_by(|a, b| {
        a.0.created_at
            .cmp(&b.0.created_at)
            .then(a.0.id.cmp(&b.0.id))
    });
    list
}

fn sheet_of(
    ctx: &Ctx,
    p: &Portal,
    a: &Account,
    id: &str,
) -> Result<(SheetReg, Vec<ColumnSetting>), Response> {
    usable_sheets(ctx, p, a)
        .into_iter()
        .find(|(s, _)| s.id == id)
        .ok_or_else(not_found)
}

fn live(ctx: &Ctx) -> Result<&LiveSheets, Response> {
    ctx.live.as_deref().ok_or_else(|| {
        err(
            StatusCode::SERVICE_UNAVAILABLE,
            "sheet_unavailable",
            "いまは応募者の一覧を読めません。弊社の担当者にご連絡ください",
        )
    })
}

fn summary(s: &SheetReg) -> PortalSheet {
    PortalSheet {
        id: s.id.clone(),
        label: if s.label.trim().is_empty() {
            "応募者一覧".to_string()
        } else {
            s.label.clone()
        },
    }
}

/// 登録と項目の設定から作る印。設定が変われば画面に読み直させる
fn config_tag(s: &SheetReg, cols: &[ColumnSetting]) -> String {
    let mut h = DefaultHasher::new();
    (
        &s.spreadsheet_id,
        s.sheet_id,
        s.header_row,
        s.example_rows,
        &s.label,
    )
        .hash(&mut h);
    for c in cols {
        (
            &c.header,
            &c.label,
            c.in_list,
            c.multiline,
            c.kind.as_str(),
            c.group.as_str(),
            c.order,
        )
            .hash(&mut h);
    }
    format!("{:016x}", h.finish())
}

fn version_of(modified: &str, s: &SheetReg, cols: &[ColumnSetting]) -> String {
    format!("{modified}|{}", config_tag(s, cols))
}

/// 読んだ表から、見せる項目だけを取り出す。シートに見出しが無い項目は出さない。
/// 見せる項目がどれも空の行は出さない
fn applicants(
    s: &SheetReg,
    cols: &[ColumnSetting],
    rows: &[Vec<String>],
) -> (Vec<PortalColumn>, Vec<PortalApplicant>) {
    let header_row = s.header_row.max(1);
    // 見出し → 列の位置。同じ見出しが 2 つ以上ある列はどちらの値か決められないので None にする
    let mut positions: HashMap<String, Option<usize>> = HashMap::new();
    for (i, h) in rows
        .first()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .enumerate()
    {
        let h = clean_header(h);
        if !h.is_empty() {
            positions
                .entry(h)
                .and_modify(|p| *p = None)
                .or_insert(Some(i));
        }
    }
    let duplicated = cols
        .iter()
        .filter(|c| matches!(positions.get(&c.header), Some(None)))
        .count();
    let found: Vec<(&ColumnSetting, usize)> = cols
        .iter()
        .filter_map(|c| positions.get(&c.header).copied().flatten().map(|i| (c, i)))
        .collect();
    if duplicated > 0 {
        tracing::warn!(
            "[portal] シートに同じ見出しが 2 つ以上あり、出さない項目がある（{duplicated} 件）"
        );
    }
    if found.len() + duplicated < cols.len() {
        tracing::warn!(
            "[portal] シートに見出しが見つからない項目がある（{} 件）",
            cols.len() - found.len() - duplicated
        );
    }
    let columns = found
        .iter()
        .map(|(c, _)| PortalColumn {
            label: if c.label.trim().is_empty() {
                c.header.clone()
            } else {
                c.label.clone()
            },
            kind: c.kind.as_str().into(),
            group: c.group.as_str().into(),
            in_list: c.in_list,
            multiline: c.multiline,
        })
        .collect();
    let skip = 1 + s.example_rows as usize;
    let list = rows
        .iter()
        .enumerate()
        .skip(skip)
        .filter_map(|(i, r)| {
            let cells: Vec<String> = found
                .iter()
                .map(|(_, at)| r.get(*at).cloned().unwrap_or_default())
                .collect();
            if cells.iter().all(|c| c.trim().is_empty()) {
                return None;
            }
            Some(PortalApplicant {
                row: header_row + i as u32,
                cells,
            })
        })
        .collect();
    (columns, list)
}

// ---- ハンドラ ----

async fn list_sheets(Extension(ctx): Extension<Arc<Ctx>>, headers: HeaderMap) -> Response {
    let (p, a) = match viewer(&ctx, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    Json(PortalSheets {
        sheets: usable_sheets(&ctx, &p, &a)
            .iter()
            .map(|(s, _)| summary(s))
            .collect(),
    })
    .into_response()
}

async fn version(
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let (p, a) = match viewer(&ctx, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (s, cols) = match sheet_of(&ctx, &p, &a, &id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let live = match live(&ctx) {
        Ok(l) => l,
        Err(r) => return r,
    };
    match live.modified(&s.spreadsheet_id).await {
        Ok(m) => Json(PortalSheetVersion {
            version: version_of(&m, &s, &cols),
        })
        .into_response(),
        Err(e) => {
            let e: ReadError = e.into();
            if e == ReadError::NotShared {
                live.forget(&s.id).await;
            }
            sheet_unavailable(e)
        }
    }
}

async fn rows(
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Path(id): Path<String>,
) -> Response {
    let (p, a) = match viewer(&ctx, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (s, cols) = match sheet_of(&ctx, &p, &a, &id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let live = match live(&ctx) {
        Ok(l) => l,
        Err(r) => return r,
    };
    // Drive が 403 / 404 なら共有が外れた: 前に読んだ表も出さない。
    // 前の表を返してよいのは、混雑・通信の失敗などの一時的なときだけ
    let modified = match live.modified(&s.spreadsheet_id).await {
        Ok(m) => Some(m),
        Err(e) => match ReadError::from(e) {
            ReadError::NotShared => {
                live.forget(&s.id).await;
                return sheet_unavailable(ReadError::NotShared);
            }
            other => {
                log_read(other);
                None
            }
        },
    };
    let (grid, stale) = match live.grid(&s, modified.as_deref()).await {
        Ok(GridResult::Fresh(g)) => (g, false),
        Ok(GridResult::Stale(g)) => (g, true),
        Err(e) => return sheet_unavailable(e),
    };
    let (columns, list) = applicants(&s, &cols, &grid.rows);
    if ctx.views.should_log(&a.id, &s.id) {
        let mut detail = format!("応募者の一覧を開いた（{} 人）", list.len());
        if stale {
            detail.push_str("。シートを確かめられず、前に読んだ内容");
        }
        portal_log_sheet(&p, &a, &s, "view", &detail);
    }
    tracing::info!(
        "[portal] 応募者の一覧を返した（{} 人・{} 項目{}）",
        list.len(),
        columns.len(),
        if stale {
            "・前に読んだ内容"
        } else {
            ""
        }
    );
    Json(PortalSheetRows {
        sheet: summary(&s),
        version: version_of(&grid.modified, &s, &cols),
        read_at: grid.read_at.to_rfc3339(),
        stale,
        columns,
        applicants: list,
    })
    .into_response()
}

fn portal_log_sheet(p: &Portal, a: &Account, s: &SheetReg, kind: &str, detail: &str) {
    let mut e = super::store::LogEntry::new(now(), &a.id, super::store::LogSource::Portal, kind);
    e.account_id = a.id.clone();
    e.customer_id = a.customer_id.clone();
    e.sheet_reg_id = s.id.clone();
    e.detail = detail.to_string();
    p.auth.store().append_log(e);
}
