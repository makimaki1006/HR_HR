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
/// 1 人 1 日の枠の順番待ち。まだ判定しない (画面の「待ち」には数えない)
pub const ST_QUEUED: &str = "queued";

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

/// 順番待ちの行を、1 人 1 日あたり `per_day` 本の枠まで文字起こし待ちにする。
/// 枠は判定済み + 文字起こし待ちの本数で数え、新しい電話から入れる (取れなくなった行は枠を空ける)。
pub fn promote(t: &TursoDb, since: &str, per_day: u32, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET status = 'pending_transcript', next_retry_at = ?3, updated_at = ?3 \
         WHERE status = 'queued' AND hubspot_call_id IN ( \
           SELECT hubspot_call_id FROM ( \
             SELECT hubspot_call_id, status, ROW_NUMBER() OVER ( \
               PARTITION BY owner_id, date(call_at, '+9 hours') \
               ORDER BY (status = 'queued'), call_at DESC) AS rn \
             FROM call_talk_reviews \
             WHERE call_at >= ?1 AND status IN ('queued', 'pending_transcript', 'judged')) \
           WHERE status = 'queued' AND rn <= ?2)",
        &[&since, &i64::from(per_day), &now],
    )
}

/// 枠を超えた文字起こし待ちの行を順番待ちに戻す (起動後に 1 回。枠ができる前の版が全部を待ちにしていた分)。
/// 判定済みを先に、次に新しい電話から数えて枠に残し、残りを戻す
pub fn demote_over_quota(t: &TursoDb, since: &str, per_day: u32, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET status = 'queued', next_retry_at = NULL, updated_at = ?3 \
         WHERE status = 'pending_transcript' AND hubspot_call_id IN ( \
           SELECT hubspot_call_id FROM ( \
             SELECT hubspot_call_id, status, ROW_NUMBER() OVER ( \
               PARTITION BY owner_id, date(call_at, '+9 hours') \
               ORDER BY (status = 'pending_transcript'), call_at DESC) AS rn \
             FROM call_talk_reviews \
             WHERE call_at >= ?1 AND status IN ('pending_transcript', 'judged')) \
           WHERE status = 'pending_transcript' AND rn > ?2)",
        &[&since, &i64::from(per_day), &now],
    )
}

/// 録音 URL が無いとして取れないことにした古い行 (理由 `no_recording_url`) を、順番待ちに戻す (起動後に 1 回)。
/// 今の worker は取れないとき `no_recording_found` を書くので、同じ行を繰り返し戻すことはない。
pub fn requeue_no_recording(t: &TursoDb, since: &str, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET status = 'queued', attempts = 0, next_retry_at = NULL, note = NULL, \
         updated_at = ?2 WHERE status = 'unavailable' AND note = 'no_recording_url' \
         AND zoom_call_log_id IS NULL AND call_at >= ?1",
        &[&since, &now],
    )
}

/// Zoom の通話履歴から見つけた録音 ID を行に覚える
pub fn set_zoom_id(t: &TursoDb, id: &str, zoom_id: &str, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET zoom_call_log_id = ?2, updated_at = ?3 \
         WHERE hubspot_call_id = ?1 AND zoom_call_log_id IS NULL",
        &[&id, &zoom_id, &now],
    )
}

/// 再試行の時刻が来た行
pub fn due(t: &TursoDb, now: &str, limit: i64) -> Result<Vec<ReviewRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM call_talk_reviews WHERE status = 'pending_transcript' \
         AND next_retry_at <= ?1 ORDER BY call_at DESC LIMIT ?2"
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
            // 新しい電話から
            assert_eq!(d[0].owner_id, "o2");
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

    #[tokio::test(flavor = "multi_thread")]
    async fn promote_fills_the_per_person_day_quota_newest_first() {
        let (audit, _conn) = start_sqlite_audit().await;
        blocking(&audit, move |t| {
            let q = |id: &str, owner: &str, at: &str| NewRow {
                status: ST_QUEUED,
                next_retry_at: None,
                ..new(id, owner, at)
            };
            // o1 の JST 10/15 (UTC 10/14 15:00 から 10/15 14:59): 4 本。UTC 10/14 14:00 は JST 10/14 の別の日
            for (id, owner, at) in [
                ("a1", "o1", "2026-10-15T01:00:00Z"),
                ("a2", "o1", "2026-10-15T02:00:00Z"),
                ("a3", "o1", "2026-10-15T03:00:00Z"),
                ("a4", "o1", "2026-10-14T16:00:00Z"),
                ("b1", "o1", "2026-10-14T14:00:00Z"),
                ("c1", "o2", "2026-10-15T01:00:00Z"),
            ] {
                insert_new(t, "2026-10-15T05:00:00Z", &q(id, owner, at))?;
            }
            let status = |id: &str| get(t, id).map(|r| r.unwrap().status);
            promote(t, "2026-10-01T00:00:00Z", 2, "2026-10-15T05:10:00Z")?;
            assert_eq!(status("a3")?, ST_PENDING);
            assert_eq!(status("a2")?, ST_PENDING);
            assert_eq!(status("a1")?, ST_QUEUED, "枠は 1 人 1 日 2 本");
            assert_eq!(status("a4")?, ST_QUEUED);
            assert_eq!(status("b1")?, ST_PENDING, "別の日は別の枠");
            assert_eq!(status("c1")?, ST_PENDING, "別の人は別の枠");
            // 何度呼んでも枠を超えない
            promote(t, "2026-10-01T00:00:00Z", 2, "2026-10-15T05:20:00Z")?;
            assert_eq!(status("a1")?, ST_QUEUED);
            // 枠内の 1 本が取れなくなったら、次に新しい 1 本が入る
            mark_final(
                t,
                "a3",
                ST_UNAVAILABLE,
                1,
                "recording_gone",
                "2026-10-15T05:30:00Z",
            )?;
            promote(t, "2026-10-01T00:00:00Z", 2, "2026-10-15T05:31:00Z")?;
            assert_eq!(status("a1")?, ST_PENDING);
            assert_eq!(status("a4")?, ST_QUEUED);
            // 文字起こし待ちになった行は新しい順に処理される
            let d = due(t, "2026-10-15T06:00:00Z", 10)?;
            assert_eq!(d[0].hubspot_call_id, "a2");
            // 古い no_recording_url の行は順番待ちに戻る (理由が違う行・URL がある行は戻らない)
            let mut n = new("n1", "o3", "2026-10-15T01:00:00Z");
            n.zoom_call_log_id = None;
            n.status = ST_UNAVAILABLE;
            n.note = Some("no_recording_url".into());
            n.next_retry_at = None;
            insert_new(t, "2026-10-15T05:00:00Z", &n)?;
            n.hubspot_call_id = "n2".into();
            n.note = Some("no_recording_found".into());
            insert_new(t, "2026-10-15T05:00:00Z", &n)?;
            requeue_no_recording(t, "2026-10-01T00:00:00Z", "2026-10-15T06:00:00Z")?;
            for (id, at) in [
                ("p1", "2026-10-13T01:00:00Z"),
                ("p2", "2026-10-13T02:00:00Z"),
                ("p3", "2026-10-13T03:00:00Z"),
            ] {
                insert_new(t, "2026-10-13T05:00:00Z", &new(id, "o9", at))?;
            }
            demote_over_quota(t, "2026-10-01T00:00:00Z", 2, "2026-10-15T06:00:00Z")?;
            assert_eq!(status("p3")?, ST_PENDING);
            assert_eq!(status("p2")?, ST_PENDING);
            assert_eq!(
                status("p1")?,
                ST_QUEUED,
                "枠を超えた古い待ちは順番待ちに戻る"
            );
            assert_eq!(status("n1")?, ST_QUEUED);
            assert_eq!(status("n2")?, ST_UNAVAILABLE);
            set_zoom_id(t, "n1", "rec:R1", "2026-10-15T06:01:00Z")?;
            assert_eq!(
                get(t, "n1")?.unwrap().zoom_call_log_id.as_deref(),
                Some("rec:R1")
            );
            Ok(())
        })
        .await
        .unwrap();
    }
}
