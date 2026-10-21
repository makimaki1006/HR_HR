//! 監査 Turso の表 `call_contact_marks` (接触・日程切りの印)。1 行 = つながった発信 1 本 (Zoom の通話ログ ID)。
//! 書き込みは「見つけたときのまとめ挿入」と「判定結果・再試行の更新」だけ。ポーリングは SELECT のみ。

use std::collections::{HashMap, HashSet};

use serde_json::Value;

use super::contact::{Candidate, ContactAgg, ContactResult, ST_JUDGED, ST_PENDING};
use crate::db::turso_http::{ToSqlTurso, TursoDb};

/// 1 回の INSERT にまとめる行数
const INSERT_CHUNK: usize = 50;

fn s(r: &HashMap<String, Value>, k: &str) -> Option<String> {
    r.get(k).and_then(Value::as_str).map(str::to_string)
}
fn n(r: &HashMap<String, Value>, k: &str) -> Option<i64> {
    r.get(k).and_then(|v| match v {
        Value::Number(x) => x.as_i64(),
        Value::String(t) => t.parse().ok(),
        _ => None,
    })
}

/// 見つけた発信をまとめて入れる (判定待ちとして、すぐ判定できる状態で)。同じ通話ログ ID は二度入らない
pub fn insert_batch(t: &TursoDb, now: &str, rows: &[Candidate]) -> Result<(), String> {
    for chunk in rows.chunks(INSERT_CHUNK) {
        let mut sql = String::from(
            "INSERT OR IGNORE INTO call_contact_marks (zoom_call_id, owner_id, call_at, connected, \
             status, attempts, next_retry_at, created_at, updated_at) VALUES ",
        );
        let mut params: Vec<&dyn ToSqlTurso> = Vec::with_capacity(chunk.len() * 3 + 1);
        for (i, r) in chunk.iter().enumerate() {
            if i > 0 {
                sql.push(',');
            }
            let b = i * 3;
            sql.push_str(&format!(
                "(?{}, ?{}, ?{}, 1, 'pending', 0, ?{}, ?{}, ?{})",
                b + 1,
                b + 2,
                b + 3,
                chunk.len() * 3 + 1,
                chunk.len() * 3 + 1,
                chunk.len() * 3 + 1,
            ));
            params.push(&r.id);
            params.push(&r.owner_id);
            params.push(&r.call_at);
        }
        params.push(&now);
        t.execute(&sql, &params)?;
    }
    Ok(())
}

/// 取り込み済みの通話ログ ID (起動時に 1 回読み、以後は worker のメモリで持つ)
pub fn known_ids(t: &TursoDb, since: &str) -> Result<HashSet<String>, String> {
    Ok(t.query(
        "SELECT zoom_call_id FROM call_contact_marks WHERE call_at >= ?1",
        &[&since],
    )?
    .iter()
    .filter_map(|r| s(r, "zoom_call_id"))
    .collect())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DueMark {
    pub id: String,
    pub call_at: String,
    pub attempts: u32,
}

/// 判定の時刻が来た行 (新しい電話から)
pub fn due(t: &TursoDb, now: &str, limit: i64) -> Result<Vec<DueMark>, String> {
    Ok(t.query(
        "SELECT zoom_call_id, call_at, attempts FROM call_contact_marks \
         WHERE status = 'pending' AND next_retry_at <= ?1 ORDER BY call_at DESC LIMIT ?2",
        &[&now, &limit],
    )?
    .iter()
    .map(|r| DueMark {
        id: s(r, "zoom_call_id").unwrap_or_default(),
        call_at: s(r, "call_at").unwrap_or_default(),
        attempts: n(r, "attempts").unwrap_or(0).max(0) as u32,
    })
    .collect())
}

pub fn mark_judged(
    t: &TursoDb,
    id: &str,
    attempts: u32,
    r: &ContactResult,
    now: &str,
) -> Result<(), String> {
    t.execute(
        "UPDATE call_contact_marks SET status = 'judged', attempts = ?2, next_retry_at = NULL, \
         contacted = ?3, date_proposals = ?4, updated_at = ?5 WHERE zoom_call_id = ?1",
        &[
            &id,
            &i64::from(attempts),
            &i64::from(r.contacted),
            &i64::from(r.date_proposals),
            &now,
        ],
    )
}

pub fn mark_retry(
    t: &TursoDb,
    id: &str,
    attempts: u32,
    next: &str,
    now: &str,
) -> Result<(), String> {
    t.execute(
        "UPDATE call_contact_marks SET attempts = ?2, next_retry_at = ?3, updated_at = ?4 \
         WHERE zoom_call_id = ?1 AND status = 'pending'",
        &[&id, &i64::from(attempts), &next, &now],
    )
}

pub fn mark_final(
    t: &TursoDb,
    id: &str,
    status: &str,
    attempts: u32,
    now: &str,
) -> Result<(), String> {
    t.execute(
        "UPDATE call_contact_marks SET status = ?2, attempts = ?3, next_retry_at = NULL, updated_at = ?4 \
         WHERE zoom_call_id = ?1 AND status = 'pending'",
        &[&id, &status, &i64::from(attempts), &now],
    )
}

/// `[from, to)` (UTC の文字列) の人ごとの合計
pub fn aggregate(t: &TursoDb, from: &str, to: &str) -> Result<HashMap<String, ContactAgg>, String> {
    let rows = t.query(
        "SELECT owner_id, status, COUNT(*) AS n, \
         SUM(CASE WHEN contacted = 1 THEN 1 ELSE 0 END) AS contacts, \
         SUM(CASE WHEN contacted = 1 AND date_proposals > 0 THEN 1 ELSE 0 END) AS proposal_calls, \
         SUM(CASE WHEN contacted = 1 THEN COALESCE(date_proposals, 0) ELSE 0 END) AS proposals \
         FROM call_contact_marks WHERE call_at >= ?1 AND call_at < ?2 GROUP BY owner_id, status",
        &[&from, &to],
    )?;
    let mut out: HashMap<String, ContactAgg> = HashMap::new();
    for r in &rows {
        let Some(owner) = s(r, "owner_id") else {
            continue;
        };
        let e = out.entry(owner).or_default();
        let u = |k: &str| n(r, k).unwrap_or(0).max(0) as u32;
        match s(r, "status").as_deref() {
            Some(ST_JUDGED) => {
                e.contacts += u("contacts");
                e.proposal_calls += u("proposal_calls");
                e.proposals += u("proposals");
            }
            Some(ST_PENDING) => e.pending += u("n"),
            _ => {}
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audit::fake_turso::start_sqlite_audit;
    use crate::crm::talk_review::store::blocking;

    fn cand(id: &str, owner: &str, at: &str) -> Candidate {
        Candidate {
            id: id.into(),
            owner_id: owner.into(),
            call_at: at.into(),
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn insert_judge_and_aggregate_on_fake_turso() {
        let (audit, _conn) = start_sqlite_audit().await;
        blocking(&audit, move |t| {
            // 120 行は 50 + 50 + 20 の 3 回の挿入。同じ ID の二重挿入は無視される
            let mut rows: Vec<Candidate> = (0..120)
                .map(|i| cand(&format!("x{i}"), "o9", "2026-10-10T00:00:00Z"))
                .collect();
            rows.extend([
                cand("a1", "o1", "2026-10-15T01:00:00Z"),
                cand("a2", "o1", "2026-10-15T02:00:00Z"),
                cand("a3", "o1", "2026-10-15T03:00:00Z"),
                cand("a4", "o1", "2026-10-15T04:00:00Z"),
                cand("b1", "o2", "2026-10-15T05:00:00Z"),
                // 期間の外
                cand("a5", "o1", "2026-10-17T00:00:00Z"),
            ]);
            insert_batch(t, "2026-10-15T06:00:00Z", &rows)?;
            insert_batch(t, "2026-10-15T06:01:00Z", &rows[..3])?;
            insert_batch(t, "2026-10-15T06:01:00Z", &[])?;
            assert_eq!(known_ids(t, "2026-10-01T00:00:00Z")?.len(), 126);
            assert_eq!(known_ids(t, "2026-10-15T00:00:00Z")?.len(), 6);
            // すぐ判定できる
            let d = due(t, "2026-10-15T06:00:00Z", 3)?;
            assert_eq!(d.len(), 3);
            assert_eq!(d[0].id, "a5", "新しい電話から");
            assert_eq!(d[0].attempts, 0);

            let r = |c, p| ContactResult {
                contacted: c,
                date_proposals: p,
            };
            mark_judged(t, "a1", 1, &r(true, 2), "2026-10-15T07:00:00Z")?;
            mark_judged(t, "a2", 1, &r(true, 0), "2026-10-15T07:00:00Z")?;
            mark_judged(t, "a3", 1, &r(false, 0), "2026-10-15T07:00:00Z")?;
            mark_judged(t, "b1", 2, &r(true, 3), "2026-10-15T07:00:00Z")?;
            // a4: 再試行 → 時刻前は due に出ない
            mark_retry(t, "a4", 1, "2026-10-15T09:00:00Z", "2026-10-15T07:00:00Z")?;
            assert!(!due(t, "2026-10-15T08:00:00Z", 500)?
                .iter()
                .any(|d| d.id == "a4"));
            let d4 = due(t, "2026-10-15T09:00:00Z", 500)?
                .into_iter()
                .find(|d| d.id == "a4")
                .unwrap();
            assert_eq!(d4.attempts, 1);
            // 判定済みの行は再試行・確定の更新を受けない
            mark_retry(t, "a1", 9, "x", "x")?;
            mark_final(t, "a1", "unavailable", 9, "x")?;

            let agg = aggregate(t, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z")?;
            let a = agg.get("o1").unwrap();
            assert_eq!(
                (a.contacts, a.proposal_calls, a.proposals, a.pending),
                (2, 1, 2, 1),
                "a1 (接触・2 回) と a2 (接触・0 回) が接触。判定待ちは a4"
            );
            let b = agg.get("o2").unwrap();
            assert_eq!(
                (b.contacts, b.proposal_calls, b.proposals, b.pending),
                (1, 1, 3, 0)
            );
            assert!(!agg.contains_key("o9"), "期間の外の人は入らない");

            // 取れなかった行は判定待ちにも接触にも数えない
            mark_final(t, "a4", "unavailable", 8, "2026-10-15T10:00:00Z")?;
            let agg = aggregate(t, "2026-10-15T00:00:00Z", "2026-10-16T00:00:00Z")?;
            let a = agg.get("o1").unwrap();
            assert_eq!((a.contacts, a.pending), (2, 0));
            Ok(())
        })
        .await
        .unwrap();
    }
}
