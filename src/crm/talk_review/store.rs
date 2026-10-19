//! 監査 Turso の表 `call_talk_reviews` (ADR-033)。書き込みは行の作成と結果の更新、掃除だけ。
//! ポーリングは SELECT のみ。呼び出しは同期 (TursoDb が blocking) なので `blocking` で包む。

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::judge::{JudgeOutput, StoredItem};
use crate::audit::AuditDb;
use crate::db::turso_http::{ToSqlTurso, TursoArg, TursoDb};

pub const ST_PENDING: &str = "pending_transcript";
pub const ST_JUDGED: &str = "judged";
pub const ST_UNAVAILABLE: &str = "unavailable";
pub const ST_FAILED: &str = "failed";

pub fn ts_text(t: chrono::DateTime<chrono::Utc>) -> String {
    t.format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

struct NullText(Option<String>);
impl ToSqlTurso for NullText {
    fn to_turso_arg(&self) -> TursoArg {
        self.0.clone().map_or(TursoArg::Null, TursoArg::Text)
    }
}
struct NullInt(Option<i64>);
impl ToSqlTurso for NullInt {
    fn to_turso_arg(&self) -> TursoArg {
        self.0.map_or(TursoArg::Null, TursoArg::Integer)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRow {
    pub hubspot_call_id: String,
    pub owner_id: String,
    /// UTC、`ts_text` の形
    pub call_at: String,
    pub duration_sec: i64,
    pub zoom_call_log_id: Option<String>,
    /// pending_transcript か unavailable
    pub status: &'static str,
    pub note: Option<String>,
    pub next_retry_at: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReviewRow {
    pub hubspot_call_id: String,
    pub owner_id: String,
    pub call_at: String,
    pub duration_sec: u32,
    pub zoom_call_log_id: Option<String>,
    pub status: String,
    pub attempts: u32,
    pub checks_version: Option<String>,
    pub model: Option<String>,
    pub outcome: Option<String>,
    pub reached_person: Option<bool>,
    pub reception: Option<bool>,
    pub first_date_turn: Option<u32>,
    pub items: Vec<StoredItem>,
    pub summary: Option<String>,
    pub note: Option<String>,
}

fn s(r: &HashMap<String, Value>, k: &str) -> Option<String> {
    r.get(k).and_then(Value::as_str).map(str::to_string)
}
fn n(r: &HashMap<String, Value>, k: &str) -> Option<i64> {
    r.get(k).and_then(Value::as_i64)
}

pub fn row_from(r: &HashMap<String, Value>) -> ReviewRow {
    ReviewRow {
        hubspot_call_id: s(r, "hubspot_call_id").unwrap_or_default(),
        owner_id: s(r, "owner_id").unwrap_or_default(),
        call_at: s(r, "call_at").unwrap_or_default(),
        duration_sec: n(r, "duration_sec").unwrap_or(0).max(0) as u32,
        zoom_call_log_id: s(r, "zoom_call_log_id"),
        status: s(r, "status").unwrap_or_default(),
        attempts: n(r, "attempts").unwrap_or(0).max(0) as u32,
        checks_version: s(r, "checks_version"),
        model: s(r, "model"),
        outcome: s(r, "outcome"),
        reached_person: n(r, "reached_person").map(|v| v != 0),
        reception: n(r, "reception").map(|v| v != 0),
        first_date_turn: n(r, "first_date_turn").map(|v| v.max(0) as u32),
        items: s(r, "items")
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default(),
        summary: s(r, "summary"),
        note: s(r, "note"),
    }
}

const COLS: &str = "hubspot_call_id, owner_id, call_at, duration_sec, zoom_call_log_id, status, attempts, \
    checks_version, model, outcome, reached_person, reception, first_date_turn, items, summary, note";

/// 同期の Turso 呼び出しを async から使う
pub async fn blocking<T: Send + 'static>(
    audit: &AuditDb,
    f: impl FnOnce(&TursoDb) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    let t = audit.turso().clone();
    tokio::task::spawn_blocking(move || f(&t))
        .await
        .map_err(|e| format!("join: {e}"))?
}

pub fn insert_new(t: &TursoDb, now: &str, r: &NewRow) -> Result<(), String> {
    t.execute(
        "INSERT OR IGNORE INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, \
         zoom_call_log_id, status, attempts, next_retry_at, note, created_at, updated_at) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9, ?9)",
        &[
            &r.hubspot_call_id,
            &r.owner_id,
            &r.call_at,
            &r.duration_sec,
            &NullText(r.zoom_call_log_id.clone()),
            &r.status,
            &NullText(r.next_retry_at.clone()),
            &NullText(r.note.clone()),
            &now,
        ],
    )
}

/// 取り込み済みの Call ID (起動時に 1 回読み、以後は worker のメモリで持つ)
pub fn known_ids(t: &TursoDb, since: &str) -> Result<HashSet<String>, String> {
    Ok(t.query(
        "SELECT hubspot_call_id FROM call_talk_reviews WHERE call_at >= ?1",
        &[&since],
    )?
    .iter()
    .filter_map(|r| s(r, "hubspot_call_id"))
    .collect())
}

/// 録音 URL が無くて unavailable にした Call の ID (後で URL が付いたら取り込み直す)
pub fn no_url_ids(t: &TursoDb, since: &str) -> Result<HashSet<String>, String> {
    Ok(t.query(
        "SELECT hubspot_call_id FROM call_talk_reviews WHERE call_at >= ?1 \
         AND status = 'unavailable' AND zoom_call_log_id IS NULL",
        &[&since],
    )?
    .iter()
    .filter_map(|r| s(r, "hubspot_call_id"))
    .collect())
}

/// 録音 URL が無くて unavailable にした行を、URL が付いたので文字起こし待ちに戻す
pub fn revive(t: &TursoDb, id: &str, zoom_id: &str, next: &str, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET status = 'pending_transcript', zoom_call_log_id = ?2, attempts = 0, \
         next_retry_at = ?3, note = NULL, updated_at = ?4 \
         WHERE hubspot_call_id = ?1 AND status = 'unavailable' AND zoom_call_log_id IS NULL",
        &[&id, &zoom_id, &next, &now],
    )
}

/// 再試行の時刻が来た行
pub fn due(t: &TursoDb, now: &str, limit: i64) -> Result<Vec<ReviewRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM call_talk_reviews WHERE status = 'pending_transcript' \
         AND next_retry_at <= ?1 ORDER BY next_retry_at LIMIT ?2"
    );
    Ok(t.query(&sql, &[&now, &limit])?
        .iter()
        .map(row_from)
        .collect())
}

pub fn mark_retry(
    t: &TursoDb,
    id: &str,
    attempts: u32,
    next: &str,
    note: &str,
    now: &str,
) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET attempts = ?2, next_retry_at = ?3, note = ?4, updated_at = ?5 \
         WHERE hubspot_call_id = ?1 AND status = 'pending_transcript'",
        &[&id, &i64::from(attempts), &next, &note, &now],
    )
}

pub fn mark_final(
    t: &TursoDb,
    id: &str,
    status: &str,
    attempts: u32,
    note: &str,
    now: &str,
) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET status = ?2, attempts = ?3, next_retry_at = NULL, note = ?4, updated_at = ?5 \
         WHERE hubspot_call_id = ?1 AND status = 'pending_transcript'",
        &[&id, &status, &i64::from(attempts), &note, &now],
    )
}

pub fn mark_judged(
    t: &TursoDb,
    id: &str,
    attempts: u32,
    version: &str,
    model: &str,
    o: &JudgeOutput,
    now: &str,
) -> Result<(), String> {
    let items = serde_json::to_string(&o.items).map_err(|e| e.to_string())?;
    t.execute(
        "UPDATE call_talk_reviews SET status = 'judged', attempts = ?2, next_retry_at = NULL, \
         checks_version = ?3, model = ?4, outcome = ?5, reached_person = ?6, reception = ?7, \
         first_date_turn = ?8, items = ?9, summary = ?10, note = NULL, updated_at = ?11 \
         WHERE hubspot_call_id = ?1",
        &[
            &id,
            &i64::from(attempts),
            &version,
            &model,
            &NullText(o.outcome.clone()),
            &NullInt(o.reached_person.map(i64::from)),
            &NullInt(o.reception.map(i64::from)),
            &NullInt(o.first_date_turn.map(i64::from)),
            &items,
            &NullText(o.summary.clone()),
            &now,
        ],
    )
}

/// `[from, to)` (UTC の文字列) の行。`owner` を指定するとその人だけ
pub fn rows_in(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
) -> Result<Vec<ReviewRow>, String> {
    let rows = match owner {
        Some(o) => t.query(
            &format!("SELECT {COLS} FROM call_talk_reviews WHERE call_at >= ?1 AND call_at < ?2 AND owner_id = ?3 ORDER BY call_at DESC"),
            &[&from, &to, &o],
        )?,
        None => t.query(
            &format!("SELECT {COLS} FROM call_talk_reviews WHERE call_at >= ?1 AND call_at < ?2 ORDER BY call_at DESC"),
            &[&from, &to],
        )?,
    };
    Ok(rows.iter().map(row_from).collect())
}

pub fn get(t: &TursoDb, id: &str) -> Result<Option<ReviewRow>, String> {
    Ok(t.query(
        &format!("SELECT {COLS} FROM call_talk_reviews WHERE hubspot_call_id = ?1"),
        &[&id],
    )?
    .first()
    .map(row_from))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::fake_turso::start_sqlite_audit;

    fn new(id: &str, owner: &str, at: &str) -> NewRow {
        NewRow {
            hubspot_call_id: id.into(),
            owner_id: owner.into(),
            call_at: at.into(),
            duration_sec: 95,
            zoom_call_log_id: Some("20261015-aaaaaaaa-aaaa-aaaa-aaaa-aaaaaaaaaaaa".into()),
            status: ST_PENDING,
            note: None,
            next_retry_at: Some("2026-10-15T01:00:00Z".into()),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn lifecycle_roundtrip_on_fake_turso() {
        let (audit, _conn) = start_sqlite_audit().await;
        let out = JudgeOutput {
            reached_person: Some(true),
            reception: Some(false),
            first_date_turn: Some(4),
            outcome: Some("決まらなかった".into()),
            items: vec![StoredItem {
                id: "early_date".into(),
                status: "not_done".into(),
                evidence: vec![4],
                quote: Some("来週は".into()),
                advice: Some("先に内容".into()),
                note: None,
            }],
            summary: Some("日付が早い".into()),
        };
        blocking(&audit, move |t| {
            insert_new(
                t,
                "2026-10-15T00:30:00Z",
                &new("c1", "o1", "2026-10-15T00:00:00Z"),
            )?;
            // 同じ Call は二度入らない
            insert_new(
                t,
                "2026-10-15T00:31:00Z",
                &new("c1", "o9", "2026-10-15T00:00:00Z"),
            )?;
            insert_new(
                t,
                "2026-10-15T00:30:00Z",
                &new("c2", "o2", "2026-10-16T00:00:00Z"),
            )?;
            assert_eq!(known_ids(t, "2026-10-01T00:00:00Z")?.len(), 2);
            // 時刻前は due に出ない
            assert!(due(t, "2026-10-15T00:59:59Z", 10)?.is_empty());
            let d = due(t, "2026-10-15T01:00:00Z", 10)?;
            assert_eq!(d.len(), 2);
            assert_eq!(d[0].owner_id, "o1");
            mark_retry(
                t,
                "c2",
                1,
                "2026-10-17T00:00:00Z",
                "not_ready",
                "2026-10-16T00:10:00Z",
            )?;
            assert_eq!(due(t, "2026-10-15T01:00:00Z", 10)?.len(), 1);
            mark_judged(
                t,
                "c1",
                1,
                "abcd1234",
                "MiniMax-M3",
                &out,
                "2026-10-15T00:40:00Z",
            )?;
            let r = get(t, "c1")?.unwrap();
            assert_eq!(r.status, ST_JUDGED);
            assert_eq!(r.owner_id, "o1");
            assert_eq!(r.outcome.as_deref(), Some("決まらなかった"));
            assert_eq!(r.reception, Some(false));
            assert_eq!(r.first_date_turn, Some(4));
            assert_eq!(r.items, out.items);
            assert_eq!(r.checks_version.as_deref(), Some("abcd1234"));
            // 判定済みの行は再試行の更新を受けない
            mark_retry(t, "c1", 9, "x", "x", "x")?;
            assert_eq!(get(t, "c1")?.unwrap().attempts, 1);
            // 判定済みの行は mark_final で書き換わらない
            mark_final(t, "c1", ST_FAILED, 9, "x", "2026-10-17T00:00:00Z")?;
            assert_eq!(get(t, "c1")?.unwrap().status, ST_JUDGED);
            mark_final(t, "c2", ST_UNAVAILABLE, 8, "gone", "2026-10-17T00:00:00Z")?;
            let w = rows_in(t, "2026-10-16T00:00:00Z", "2026-10-17T00:00:00Z", None)?;
            assert_eq!(w.len(), 1);
            assert_eq!(w[0].status, ST_UNAVAILABLE);
            assert_eq!(
                rows_in(
                    t,
                    "2026-10-01T00:00:00Z",
                    "2026-11-01T00:00:00Z",
                    Some("o1")
                )?
                .len(),
                1
            );
            Ok(())
        })
        .await
        .unwrap();
    }
}
