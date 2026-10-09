//! Headless CRM の書き込みの操作台帳 (監査 Turso の `crm_pending_operations`。ADR-018)。
//!
//! CRM のコピーではない。持つのは書き込み操作 1 件ごとの「誰が・どの案件に・何を書こうとして・今どうなっているか」だけ。
//! - 送る**前**に `in_progress` で入れる (冪等性: 同じ `operation_id` の再送は保存された結果を返す)
//! - 一時障害 (429 / 5xx / タイムアウト / 接続失敗 / 混雑) は `pending` にして指数バックオフで再送する
//! - 恒久エラー・競合・上限超えは `failed` (管理画面の一覧に出る)
//! - 成功は `saved` に更新して数日残す (再送への返答用)。古い `saved` / `failed` / `discarded` は worker が消す
//!
//! Turso の書き込みは (1) 受付の INSERT (2) 送信結果ごとの UPDATE (3) 掃除の DELETE だけ。
//! worker は**書き込みでポーリングしない** (期限が来た行の SELECT のみ。1 本も無いときは長く眠る)。
//!
//! 時刻は ISO-8601 UTC 秒 (`2026-10-09T01:02:03Z`) の文字列で、辞書順 = 時刻順。

use std::collections::BTreeMap;

use chrono::{DateTime, Duration as ChronoDuration, SecondsFormat, Utc};
use serde::{Deserialize, Serialize};

use crate::audit::AuditDb;
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_i64, get_str};

/// 再送の待ち時間 (試行 n 回目の失敗の後)。1 分・5 分・30 分・2 時間、以降は 6 時間
pub const BACKOFF_SECS: [i64; 5] = [60, 300, 1_800, 7_200, 21_600];
/// 再送の最大試行回数 (初回の同期送信を含む)。超えたら `failed`
pub const MAX_ATTEMPTS: i64 = 8;
/// `in_progress` のまま止まった行を再送の対象にするまでの時間 (プロセスが送信中に落ちた場合)
pub const IN_PROGRESS_STALE_SECS: i64 = 600;
/// 保存済み・失敗・破棄の行を残す日数
pub const RETAIN_DAYS: i64 = 7;
/// 保留の既定の上限 (`CRM_PENDING_MAX`)
pub const DEFAULT_PENDING_MAX: i64 = 50_000;

pub fn iso(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

pub fn now_iso() -> String {
    iso(Utc::now())
}

/// 失敗 `attempts` 回目 (1 始まり) の後に待つ時間
pub fn backoff_after(attempts: i64) -> ChronoDuration {
    if let Some(secs) = debug_override_secs("CRM_PENDING_BACKOFF_SECS_DEBUG") {
        return ChronoDuration::seconds(secs);
    }
    let i = (attempts.max(1) - 1) as usize;
    ChronoDuration::seconds(BACKOFF_SECS[i.min(BACKOFF_SECS.len() - 1)])
}

/// E2E 用: debug ビルドだけが読む秒数の上書き (`*_DEBUG` 環境変数。release では常に None)。
/// 再送の待ち時間 (60 秒〜) を E2E で待てる長さに縮めるために使う
pub fn debug_override_secs(var: &str) -> Option<i64> {
    if !cfg!(debug_assertions) {
        return None;
    }
    std::env::var(var)
        .ok()
        .and_then(|v| v.trim().parse::<i64>().ok())
        .filter(|n| *n >= 0)
}

/// 書き込みの 1 段 (オブジェクト 1 つ分の PATCH)
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Step {
    /// `deals` / `contacts` / `companies`
    pub object: String,
    pub id: String,
    /// 利用者が見ていた値 (競合の検査用)
    pub base: BTreeMap<String, Option<String>>,
    /// 書く値 (`None` = 消去)
    pub set: BTreeMap<String, Option<String>>,
    /// 項目の型 (値の比較の正規化用。再送時にカタログを引かなくて済むように持つ)
    pub types: BTreeMap<String, String>,
    /// ステージ・パイプラインの移動 (案件の段だけ)
    pub stage: Option<StageMove>,
    /// 送信済み (再送では飛ばす)
    #[serde(default)]
    pub done: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StageMove {
    pub pipeline_id: String,
    pub stage_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Payload {
    pub steps: Vec<Step>,
}

/// 台帳の 1 行
#[derive(Debug, Clone)]
pub struct OpRow {
    pub operation_id: String,
    pub operator_email: String,
    pub deal_id: String,
    pub payload: Payload,
    pub status: String,
    pub attempts: i64,
    pub last_error_code: String,
    pub http_status: i64,
    pub result_json: String,
    pub next_retry_at: String,
    pub created_at: String,
    pub updated_at: String,
}

impl OpRow {
    /// 変えようとしている項目の名前 (オブジェクト順)
    pub fn changed_props(&self) -> Vec<String> {
        let mut v = Vec::new();
        for s in &self.payload.steps {
            for k in s.set.keys() {
                v.push(k.clone());
            }
            if s.stage.is_some() {
                v.push("dealstage".to_string());
            }
        }
        v
    }
}

const COLS: &str = "operation_id, operator_email, deal_id, payload, status, attempts, \
     last_error_code, http_status, result_json, next_retry_at, created_at, updated_at";

fn row_to_op(r: &std::collections::HashMap<String, serde_json::Value>) -> Option<OpRow> {
    let payload: Payload = serde_json::from_str(&get_str(r, "payload")).ok()?;
    Some(OpRow {
        operation_id: get_str(r, "operation_id"),
        operator_email: get_str(r, "operator_email"),
        deal_id: get_str(r, "deal_id"),
        payload,
        status: get_str(r, "status"),
        attempts: get_i64(r, "attempts"),
        last_error_code: get_str(r, "last_error_code"),
        http_status: get_i64(r, "http_status"),
        result_json: get_str(r, "result_json"),
        next_retry_at: get_str(r, "next_retry_at"),
        created_at: get_str(r, "created_at"),
        updated_at: get_str(r, "updated_at"),
    })
}

/// 同期の Turso 呼び出しを spawn_blocking に載せる
pub async fn blocking<T: Send + 'static>(
    audit: &AuditDb,
    f: impl FnOnce(&TursoDb) -> T + Send + 'static,
) -> Result<T, String> {
    let turso = audit.turso().clone();
    tokio::task::spawn_blocking(move || f(&turso))
        .await
        .map_err(|e| format!("join: {e}"))
}

pub fn get_op(turso: &TursoDb, operation_id: &str) -> Result<Option<OpRow>, String> {
    let rows = turso.query(
        &format!("SELECT {COLS} FROM crm_pending_operations WHERE operation_id = ?1 LIMIT 1"),
        &[&operation_id],
    )?;
    Ok(rows.first().and_then(row_to_op))
}

/// 受付 (送る前)。同じ `operation_id` が既にあると PRIMARY KEY 違反で Err
pub fn insert_op(
    turso: &TursoDb,
    operation_id: &str,
    operator_email: &str,
    deal_id: &str,
    payload: &Payload,
    object_refs: &str,
) -> Result<(), String> {
    let now = now_iso();
    let payload_json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let stale_at = iso(Utc::now() + ChronoDuration::seconds(IN_PROGRESS_STALE_SECS));
    turso.execute(
        "INSERT INTO crm_pending_operations \
         (operation_id, operator_email, deal_id, object_refs, payload, status, attempts, \
          next_retry_at, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, 'in_progress', 0, ?6, ?7, ?7)",
        &[
            &operation_id,
            &operator_email,
            &deal_id,
            &object_refs,
            &payload_json,
            &stale_at,
            &now,
        ],
    )
}

/// 状態の更新 (送信結果ごとに 1 回)
#[allow(clippy::too_many_arguments)]
pub fn update_op(
    turso: &TursoDb,
    operation_id: &str,
    status: &str,
    attempts: i64,
    last_error_code: &str,
    http_status: i64,
    result_json: &str,
    next_retry_at: &str,
    payload: &Payload,
) -> Result<(), String> {
    let payload_json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let now = now_iso();
    turso.execute(
        "UPDATE crm_pending_operations SET status = ?1, attempts = ?2, last_error_code = ?3, \
         http_status = ?4, result_json = ?5, next_retry_at = ?6, payload = ?7, updated_at = ?8 \
         WHERE operation_id = ?9",
        &[
            &status,
            &attempts,
            &last_error_code,
            &http_status,
            &result_json,
            &next_retry_at,
            &payload_json,
            &now,
            &operation_id,
        ],
    )
}

/// 再送待ち (`pending`) の件数。上限の判定に使う
pub fn count_pending(turso: &TursoDb) -> Result<i64, String> {
    let rows = turso.query(
        "SELECT COUNT(*) AS n FROM crm_pending_operations WHERE status = 'pending'",
        &[],
    )?;
    Ok(rows.first().map(|r| get_i64(r, "n")).unwrap_or(0))
}

/// 未完了 (`pending` + `in_progress`) の件数。受付の上限 (Turso への書き込みの総量) の判定に使う
pub fn count_active(turso: &TursoDb) -> Result<i64, String> {
    let rows = turso.query(
        "SELECT COUNT(*) AS n FROM crm_pending_operations WHERE status IN ('pending', 'in_progress')",
        &[],
    )?;
    Ok(rows.first().map(|r| get_i64(r, "n")).unwrap_or(0))
}

/// `failed` の行を、同じ `operation_id` の再送で新しい受付 (`in_progress`) に戻す。
/// 失敗の結果を永久に返し続けない (再送が成功する道を残す)。`failed` 以外には効かない
pub fn reset_failed_op(
    turso: &TursoDb,
    operation_id: &str,
    payload: &Payload,
    object_refs: &str,
) -> Result<(), String> {
    let now = now_iso();
    let payload_json = serde_json::to_string(payload).map_err(|e| e.to_string())?;
    let stale_at = iso(Utc::now() + ChronoDuration::seconds(IN_PROGRESS_STALE_SECS));
    turso.execute(
        "UPDATE crm_pending_operations SET status = 'in_progress', attempts = 0, \
         last_error_code = '', http_status = 0, result_json = '', object_refs = ?1, payload = ?2, \
         next_retry_at = ?3, updated_at = ?4 WHERE operation_id = ?5 AND status = 'failed'",
        &[&object_refs, &payload_json, &stale_at, &now, &operation_id],
    )
}

/// 期限が来た再送待ち (と、止まった送信中) を古い順に
pub fn due_ops(turso: &TursoDb, now: &str, limit: i64) -> Result<Vec<OpRow>, String> {
    let rows = turso.query(
        &format!(
            "SELECT {COLS} FROM crm_pending_operations \
             WHERE status IN ('pending', 'in_progress') AND next_retry_at <= ?1 \
             ORDER BY next_retry_at ASC LIMIT ?2"
        ),
        &[&now, &limit],
    )?;
    Ok(rows.iter().filter_map(row_to_op).collect())
}

/// 次に期限が来る時刻 (再送待ちが無ければ None)
pub fn next_due(turso: &TursoDb) -> Result<Option<String>, String> {
    let rows = turso.query(
        "SELECT MIN(next_retry_at) AS t FROM crm_pending_operations \
         WHERE status IN ('pending', 'in_progress')",
        &[],
    )?;
    Ok(rows
        .first()
        .map(|r| get_str(r, "t"))
        .filter(|t| !t.is_empty()))
}

/// 管理画面の一覧 (`status` は `failed` / `pending`。新しい順)。
/// `failed` は破棄 (`discarded`) を含まない。`pending` は送信中 (`in_progress`) も含む
pub fn list_ops(turso: &TursoDb, status: &str, limit: i64) -> Result<Vec<OpRow>, String> {
    let where_clause = match status {
        "failed" => "status = 'failed'",
        _ => "status IN ('pending', 'in_progress')",
    };
    let rows = turso.query(
        &format!(
            "SELECT {COLS} FROM crm_pending_operations WHERE {where_clause} \
             ORDER BY created_at DESC LIMIT ?1"
        ),
        &[&limit],
    )?;
    Ok(rows.iter().filter_map(row_to_op).collect())
}

/// 古い保存済み・失敗・破棄の行を消す (worker が 1 日 1 回)
pub fn purge_old(turso: &TursoDb) -> Result<(), String> {
    let cutoff = iso(Utc::now() - ChronoDuration::days(RETAIN_DAYS));
    turso.execute(
        "DELETE FROM crm_pending_operations \
         WHERE status IN ('saved', 'failed', 'discarded') AND updated_at < ?1",
        &[&cutoff],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 再送の待ち時間は指数で_6時間が上限() {
        let secs: Vec<i64> = (1..=8).map(|n| backoff_after(n).num_seconds()).collect();
        assert_eq!(
            secs,
            vec![60, 300, 1_800, 7_200, 21_600, 21_600, 21_600, 21_600]
        );
        assert_eq!(backoff_after(0).num_seconds(), 60);
    }
}
