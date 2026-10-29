//! 顧客の行保存。元の行を秘密列ごと照合し、許可されたセルだけをRAWで書く。
use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::body::Bytes;
use axum::extract::Path;
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::patch;
use axum::{Extension, Json, Router};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use tokio::sync::Mutex as AsyncMutex;
use ts_rs::TS;

use super::sheets::{a1, col_letter, SheetCell};
use super::store::{Account, ColumnSetting, LogEntry, LogSource, SheetReg};
use super::view::{self, PortalApplicant};
use super::{err, now, Ctx, Portal};
use crate::AppState;

pub(super) struct EditState {
    secret: [u8; 32],
    locks: Mutex<HashMap<String, Arc<AsyncMutex<()>>>>,
}

impl Default for EditState {
    fn default() -> Self {
        let mut secret = [0; 32];
        rand::rngs::OsRng.fill_bytes(&mut secret);
        Self {
            secret,
            locks: Mutex::new(HashMap::new()),
        }
    }
}

// RFC 2104。既存のCRMカーソルと同じSHA-256のHMAC。
fn hmac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    let mut block = [0u8; 64];
    block[..32].copy_from_slice(key);
    let mut inner = Sha256::new();
    inner.update(block.map(|b| b ^ 0x36));
    inner.update(message);
    let mut outer = Sha256::new();
    outer.update(block.map(|b| b ^ 0x5c));
    outer.update(inner.finalize());
    outer.finalize().into()
}

fn constant_equal(a: &str, b: &str) -> bool {
    a.len() == b.len() && a.bytes().zip(b.bytes()).fold(0, |d, (a, b)| d | (a ^ b)) == 0
}

/// 空セルの省略と明示的な空文字は同じとして比較する。
fn normalized(cells: &[SheetCell]) -> Vec<SheetCell> {
    let mut cells = cells.to_vec();
    for cell in &mut cells {
        if cell
            .entered
            .get("stringValue")
            .and_then(serde_json::Value::as_str)
            == Some("")
        {
            cell.entered = serde_json::Value::Null;
        }
    }
    while cells
        .last()
        .is_some_and(|c| c.text.is_empty() && c.entered.is_null() && c.validation.is_none())
    {
        cells.pop();
    }
    cells
}

impl EditState {
    pub(super) fn revision(
        &self,
        a: &Account,
        s: &SheetReg,
        cols: &[ColumnSetting],
        headers: &[SheetCell],
        row: u32,
        cells: &[SheetCell],
    ) -> String {
        let mut columns: Vec<_> = cols
            .iter()
            .map(|c| {
                (
                    &c.header,
                    &c.label,
                    c.visible,
                    c.writable,
                    c.in_list,
                    c.multiline,
                    c.kind.as_str(),
                    c.group.as_str(),
                    c.order,
                )
            })
            .collect();
        columns.sort();
        let payload = json!([
            "portal-row-v1",
            a.id,
            a.customer_id,
            s.id,
            s.spreadsheet_id,
            s.sheet_id,
            s.header_row,
            s.example_rows,
            s.label,
            columns,
            normalized(headers),
            row,
            normalized(cells)
        ]);
        let schema = json!([
            "portal-schema-v1",
            a.id,
            a.customer_id,
            s.id,
            s.spreadsheet_id,
            s.sheet_id,
            s.header_row,
            s.example_rows,
            s.label,
            columns,
            normalized(headers)
        ]);
        hmac(&self.secret, schema.to_string().as_bytes())
            .into_iter()
            .chain(hmac(&self.secret, payload.to_string().as_bytes()))
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    fn lock(&self, s: &SheetReg) -> Arc<AsyncMutex<()>> {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .entry(format!("{}:{}", s.spreadsheet_id, s.sheet_id))
            .or_default()
            .clone()
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Change {
    column: String,
    value: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SaveRequest {
    revision: String,
    changes: Vec<Change>,
}

#[derive(Serialize, TS)]
pub struct PortalSaved {
    pub applicant: PortalApplicant,
}

pub(super) fn routes() -> Router<Arc<AppState>> {
    Router::new().route("/api/portal/sheets/{id}/rows/{row}", patch(save))
}

fn positions(headers: &[SheetCell]) -> HashMap<String, Option<usize>> {
    let mut positions = HashMap::new();
    for (i, cell) in headers.iter().enumerate() {
        let name = super::sheets::clean_header(&cell.text);
        if !name.is_empty() {
            positions
                .entry(name)
                .and_modify(|p| *p = None)
                .or_insert(Some(i));
        }
    }
    positions
}

fn applicant(
    ctx: &Ctx,
    p: &Portal,
    a: &Account,
    s: &SheetReg,
    headers: &[SheetCell],
    row: u32,
    cells: &[SheetCell],
) -> PortalApplicant {
    let positions = positions(headers);
    let visible = view::visible_columns(p, &s.id);
    PortalApplicant {
        row,
        cells: visible
            .iter()
            .filter_map(|c| positions.get(&c.header).copied().flatten())
            .map(|i| cells.get(i).map(|c| c.text.clone()).unwrap_or_default())
            .collect(),
        revision: ctx
            .edits
            .revision(a, s, &p.auth.store().columns(&s.id), headers, row, cells),
    }
}

fn log(
    p: &Portal,
    a: &Account,
    s: &SheetReg,
    row: u32,
    kind: &str,
    column: &str,
    before: &str,
    after: &str,
) {
    let mut entry = LogEntry::new(now(), &a.id, LogSource::Portal, kind);
    entry.account_id = a.id.clone();
    entry.customer_id = a.customer_id.clone();
    entry.sheet_reg_id = s.id.clone();
    entry.row = row.to_string();
    entry.column = column.to_string();
    entry.before = before.to_string();
    entry.after = after.to_string();
    p.auth.store().append_log(entry);
}

async fn save(
    Extension(ctx): Extension<Arc<Ctx>>,
    headers: HeaderMap,
    Path((id, row)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let (p, a) = match view::viewer(&ctx, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (s, _) = match view::sheet_of(&ctx, &p, &a, &id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let row: u32 = match row
        .parse::<u32>()
        .ok()
        .filter(|r| *r > s.header_row.saturating_add(s.example_rows) && *r <= 1_000_000)
    {
        Some(r) => r,
        None => {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "readonly",
                "この行には保存できません",
            )
        }
    };
    let request: SaveRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => {
            return err(
                StatusCode::BAD_REQUEST,
                "invalid",
                "保存する内容を読み取れません",
            )
        }
    };
    if request.revision.len() != 128
        || !request.revision.bytes().all(|c| c.is_ascii_hexdigit())
        || request.changes.is_empty()
        || request.changes.len() > 200
        || request
            .changes
            .iter()
            .any(|c| c.column.len() > 400 || c.value.chars().count() > 20_000)
    {
        return err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "invalid",
            "保存する内容を確認してください",
        );
    }
    let lock = ctx.edits.lock(&s);
    let _guard = lock.lock().await;
    let live = match view::live(&ctx) {
        Ok(l) => l,
        Err(r) => return r,
    };
    let (title, head, mut cells) = match live.fresh_row(&s, row).await {
        Ok(v) => v,
        Err(e) => {
            log(&p, &a, &s, row, "save_failed", "", "", "");
            return view::sheet_unavailable(e);
        }
    };
    // 読み取りを待つ間の停止・列設定の変更も、書き込みの直前に確かめる。
    let (p, a) = match view::viewer(&ctx, &headers) {
        Ok(v) => v,
        Err(r) => return r,
    };
    let (current_reg, _) = match view::sheet_of(&ctx, &p, &a, &id) {
        Ok(v) => v,
        Err(r) => return r,
    };
    // 設定の変更を確かめる前に古い値を新しい列設定で投影してはいけない。
    if current_reg.spreadsheet_id != s.spreadsheet_id
        || current_reg.sheet_id != s.sheet_id
        || current_reg.header_row != s.header_row
        || current_reg.example_rows != s.example_rows
    {
        return err(
            StatusCode::CONFLICT,
            "conflict",
            "シートの設定が変わりました。読み直してください",
        );
    }
    let actual_revision = ctx.edits.revision(
        &a,
        &current_reg,
        &p.auth.store().columns(&s.id),
        &head,
        row,
        &cells,
    );
    if !constant_equal(&request.revision[..64], &actual_revision[..64]) {
        log(&p, &a, &current_reg, row, "conflict", "", "", "");
        return err(
            StatusCode::CONFLICT,
            "conflict",
            "項目の設定が変わりました。読み直してください",
        );
    }
    let current = applicant(&ctx, &p, &a, &current_reg, &head, row, &cells);
    if !constant_equal(&request.revision, &current.revision) {
        log(&p, &a, &current_reg, row, "conflict", "", "", "");
        return (StatusCode::CONFLICT, Json(json!({"error":"conflict", "message":"シートで変更されています。最新の内容を確認してください", "latest":current}))).into_response();
    }
    if current.cells.iter().all(|v| v.trim().is_empty()) {
        return view::not_found();
    }
    let settings = p.auth.store().columns(&s.id);
    let positions = positions(&head);
    let mut seen = HashSet::new();
    let mut updates = Vec::new();
    let mut changed = Vec::new();
    for change in request.changes {
        if !seen.insert(change.column.clone()) {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "invalid",
                "同じ項目が重複しています",
            );
        }
        let Some(setting) = settings
            .iter()
            .find(|c| c.header == change.column && c.visible && c.writable)
        else {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "readonly",
                "この項目は画面から書けません",
            );
        };
        let Some(index) = positions.get(&setting.header).copied().flatten() else {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "readonly",
                "シートの見出しを確認できません。読み直してください",
            );
        };
        let cell = cells.get(index).cloned().unwrap_or_default();
        if cell.formula() {
            return err(
                StatusCode::UNPROCESSABLE_ENTITY,
                "readonly",
                "数式のある項目には保存できません",
            );
        }
        match cell.options() {
            Err(_) => {
                return err(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "readonly",
                    "この入力規則の項目には保存できません",
                )
            }
            Ok(Some(options)) if !change.value.is_empty() && !options.contains(&change.value) => {
                return err(
                    StatusCode::UNPROCESSABLE_ENTITY,
                    "validation",
                    "シートの選択肢から選んでください",
                )
            }
            _ => {}
        }
        if cell.text == change.value {
            continue;
        }
        updates.push((
            a1(
                &title,
                &format!("{}{row}:{}{row}", col_letter(index), col_letter(index)),
            ),
            vec![vec![change.value.clone()]],
        ));
        changed.push((setting.header.clone(), cell.text, change.value.clone()));
        if cells.len() <= index {
            cells.resize(index + 1, SheetCell::default());
        }
        let validation = cells[index].validation.clone();
        cells[index] = SheetCell::literal(change.value);
        cells[index].validation = validation;
    }
    if !updates.is_empty() {
        let Some(api) = &ctx.reader else {
            return super::unavailable();
        };
        if let Err(e) = api.batch_update(&s.spreadsheet_id, &updates).await {
            // Google側の受理後に通信が切れた可能性もある。古いキャッシュは出さない。
            live.invalidate(&s).await;
            log(&p, &a, &s, row, "save_failed", "", "", "");
            tracing::warn!("[portal] 応募者の保存に失敗（{e:?}）");
            return err(
                StatusCode::SERVICE_UNAVAILABLE,
                "save_failed",
                "保存できたか確認できません。読み直して内容を確認してください",
            );
        }
        for (column, before, after) in changed {
            log(&p, &a, &s, row, "save", &column, &before, &after);
        }
        live.invalidate(&s).await;
    }
    Json(PortalSaved {
        applicant: applicant(&ctx, &p, &a, &s, &head, row, &cells),
    })
    .into_response()
}

#[cfg(test)]
mod tests {
    #[test]
    fn hmacはrfc4231の既知の値と一致する() {
        let mut key = [0; 32];
        key[..20].fill(0x0b);
        let actual: String = super::hmac(&key, b"Hi There")
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            actual,
            "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7"
        );
    }
}
