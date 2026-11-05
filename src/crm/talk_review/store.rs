//! 監査 Turso の表 `call_talk_reviews` (ADR-033)。書き込みは行の作成と結果の更新、掃除だけ。
//! ポーリングは SELECT のみ。呼び出しは同期 (TursoDb が blocking) なので `blocking` で包む。

use std::collections::{HashMap, HashSet};

use serde::Deserialize;
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

/// 集計と一覧が使う項目の判定だけ。引用・助言・メモ・根拠の番号は JSON から読み捨てる
/// (1 行で数 KB あり、期間の全行を持つとメモリを食うため。2026-11-04 に本番がメモリの上限を超えて落ちた)
#[derive(Deserialize)]
struct LightItem {
    id: String,
    status: String,
    #[serde(default)]
    answer: Option<String>,
    #[serde(default)]
    asked: Option<bool>,
    #[serde(default)]
    answered: Option<bool>,
}

impl From<LightItem> for StoredItem {
    fn from(i: LightItem) -> Self {
        StoredItem {
            id: i.id,
            status: i.status,
            evidence: Vec::new(),
            quote: None,
            advice: None,
            note: None,
            answer: i.answer,
            asked: i.asked,
            answered: i.answered,
        }
    }
}

/// 軽い読み方の列: 集計 (`summary.rs`) と一覧 (`routes.rs` の `call_row`) が使うものだけ
const LIGHT_COLS: &str = "hubspot_call_id, owner_id, call_at, duration_sec, status, outcome, items";
const LIGHT_COLS_WITH_SUMMARY: &str =
    "hubspot_call_id, owner_id, call_at, duration_sec, status, outcome, items, summary";

/// 軽い読み方の行。使わない欄 (録音 ID・試行回数・版・モデル・接触の印・メモ) は既定値、
/// `items` の引用・助言・メモ・根拠の番号は空、`summary` は `with_summary` のときだけ入れる
pub fn light_row_from(r: &HashMap<String, Value>, with_summary: bool) -> ReviewRow {
    ReviewRow {
        hubspot_call_id: s(r, "hubspot_call_id").unwrap_or_default(),
        owner_id: s(r, "owner_id").unwrap_or_default(),
        call_at: s(r, "call_at").unwrap_or_default(),
        duration_sec: n(r, "duration_sec").unwrap_or(0).max(0) as u32,
        zoom_call_log_id: None,
        status: s(r, "status").unwrap_or_default(),
        attempts: 0,
        checks_version: None,
        model: None,
        outcome: s(r, "outcome"),
        reached_person: None,
        reception: None,
        first_date_turn: None,
        items: s(r, "items")
            .and_then(|t| serde_json::from_str::<Vec<LightItem>>(&t).ok())
            .map(|v| v.into_iter().map(StoredItem::from).collect())
            .unwrap_or_default(),
        summary: if with_summary { s(r, "summary") } else { None },
        note: None,
    }
}

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
/// 枠に入れるのは接触した電話 (`note = 'contacted'`) の行だけ (接触を調べていない古い行は入れない)。
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
             WHERE call_at >= ?1 AND status IN ('queued', 'pending_transcript', 'judged') \
               AND (status <> 'queued' OR note = 'contacted')) \
           WHERE status = 'queued' AND rn <= ?2)",
        &[&since, &i64::from(per_day), &now],
    )
}

/// 接触の印をまだ付けていない順番待ちの行 (この変更より前に取り込んだ行)。起動後に 1 回だけ読む
pub fn queued_unmarked(t: &TursoDb, since: &str) -> Result<Vec<ReviewRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM call_talk_reviews WHERE status = 'queued' AND note IS NULL AND call_at >= ?1"
    );
    Ok(t.query(&sql, &[&since])?.iter().map(row_from).collect())
}

/// 接触した電話と分かった順番待ちの行に印を付ける (枠に入れるようにする)
pub fn mark_contacted(t: &TursoDb, ids: &[String], now: &str) -> Result<(), String> {
    for chunk in ids.chunks(200) {
        let ph: Vec<String> = (2..=chunk.len() + 1).map(|i| format!("?{i}")).collect();
        let sql = format!(
            "UPDATE call_talk_reviews SET note = 'contacted', updated_at = ?1 \
             WHERE status = 'queued' AND note IS NULL AND hubspot_call_id IN ({})",
            ph.join(",")
        );
        let mut params: Vec<&dyn ToSqlTurso> = vec![&now];
        params.extend(chunk.iter().map(|x| x as &dyn ToSqlTurso));
        t.execute(&sql, &params)?;
    }
    Ok(())
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

/// 行の担当者を書き換える (Zoom でかけた人に直すとき)
pub fn set_owner(t: &TursoDb, id: &str, owner: &str, now: &str) -> Result<(), String> {
    t.execute(
        "UPDATE call_talk_reviews SET owner_id = ?2, updated_at = ?3 WHERE hubspot_call_id = ?1",
        &[&id, &owner, &now],
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

/// 判定済みの行 (新しい電話から)。助言の文が不合格の行を見つける起動時の 1 回の SELECT 用
pub fn judged_since(t: &TursoDb, since: &str) -> Result<Vec<ReviewRow>, String> {
    let sql = format!(
        "SELECT {COLS} FROM call_talk_reviews WHERE status = 'judged' AND call_at >= ?1 ORDER BY call_at DESC"
    );
    Ok(t.query(&sql, &[&since])?.iter().map(row_from).collect())
}

/// 判定済みの行の項目とまとめだけを書き換える (録音が消えて判定し直せない行の文章の掃除と、
/// ヒアリングを入れる前の行にヒアリングの項目を足すとき。outcome などは変えない)
pub fn mark_text_cleaned(
    t: &TursoDb,
    id: &str,
    items: &[StoredItem],
    summary: Option<&str>,
    now: &str,
) -> Result<(), String> {
    let items = serde_json::to_string(items).map_err(|e| e.to_string())?;
    t.execute(
        "UPDATE call_talk_reviews SET items = ?2, summary = ?3, updated_at = ?4 \
         WHERE hubspot_call_id = ?1 AND status = 'judged'",
        &[&id, &items, &NullText(summary.map(str::to_string)), &now],
    )
}

/// 期間の行を読む 1 ページの行数。Turso の応答は本文・`Value`・行の 3 つが同時にメモリに載るので、
/// 期間の全行を 1 回で読むと数百 MB になる (2026-11-04 にメモリの上限 2GB を超えて落ちた)。
/// ページに分けると、一時的に使う分は 1 ページ (数 MB) で済む
pub const ROWS_PAGE: usize = 500;

/// 次のページの始まり = 前のページの最後の行の (call_at, rowid)
type RowsCursor = (String, i64);

fn page_cursor(r: &HashMap<String, Value>) -> Result<RowsCursor, String> {
    match (s(r, "call_at"), n(r, "rid")) {
        (Some(at), Some(id)) => Ok((at, id)),
        _ => Err("talk review: page cursor is missing".to_string()),
    }
}

/// `[from, to)` の行の 1 ページ。新しい順 (`call_at` の降順、同じなら rowid の降順 = 索引
/// `idx_call_talk_reviews_call_at` を逆に走査した順)。次のページは OFFSET でなく、前のページの最後の行から
/// (`after`) 続ける。2 ページ目以降は上の端を最後の行の `call_at` まで狭めるので、読み終えた行を読み直さない
fn rows_page(
    t: &TursoDb,
    cols: &str,
    (from, to): (&str, &str),
    owner: Option<&str>,
    after: Option<&RowsCursor>,
    page: usize,
) -> Result<Vec<HashMap<String, Value>>, String> {
    let mut sql = format!("SELECT rowid AS rid, {cols} FROM call_talk_reviews WHERE call_at >= ?1");
    let mut params: Vec<&dyn ToSqlTurso> = vec![&from];
    match after {
        None => {
            params.push(&to);
            sql.push_str(" AND call_at < ?2");
        }
        Some((last_at, last_id)) => {
            params.push(last_at);
            params.push(last_id);
            sql.push_str(" AND call_at <= ?2 AND (call_at < ?2 OR rowid < ?3)");
        }
    }
    if let Some(o) = &owner {
        params.push(o);
        sql.push_str(&format!(" AND owner_id = ?{}", params.len()));
    }
    sql.push_str(&format!(" ORDER BY call_at DESC, rowid DESC LIMIT {page}"));
    t.query(&sql, &params)
}

/// 期間の行を `page` 行ずつ読み、行ごとに `convert` で作って全部つなぐ
fn read_rows<T>(
    t: &TursoDb,
    cols: &str,
    range: (&str, &str),
    owner: Option<&str>,
    page: usize,
    mut convert: impl FnMut(&HashMap<String, Value>) -> T,
) -> Result<Vec<T>, String> {
    let page = page.max(1);
    let mut out: Vec<T> = Vec::new();
    let mut after: Option<RowsCursor> = None;
    loop {
        let rows = rows_page(t, cols, range, owner, after.as_ref(), page)?;
        out.extend(rows.iter().map(&mut convert));
        let Some(last) = rows.last().filter(|_| rows.len() >= page) else {
            return Ok(out);
        };
        let next = page_cursor(last)?;
        // 続きが前に進まないなら、同じページを読み続けて終わらなくなる
        if after.as_ref().is_some_and(|prev| next >= *prev) {
            return Err("talk review: page cursor did not advance".to_string());
        }
        after = Some(next);
    }
}

/// `[from, to)` (UTC の文字列) の行を全部の列で読む。`owner` を指定するとその人だけ
pub fn rows_in(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
) -> Result<Vec<ReviewRow>, String> {
    rows_in_paged(t, from, to, owner, ROWS_PAGE)
}

fn rows_in_paged(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
    page: usize,
) -> Result<Vec<ReviewRow>, String> {
    read_rows(t, COLS, (from, to), owner, page, row_from)
}

/// `[from, to)` の行を、集計と一覧が使う列だけで読む (引用・助言・メモ・根拠は空。`with_summary` でなければ
/// `summary` も空)。並びと中身は `rows_in` と同じ。集計 (`summary.rs`)・一覧 (`call_row`)・担当者の付け直しは
/// これで足りる。詳細の画面は `get` で 1 件だけ全部の列を読む
pub fn rows_light_in(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
    with_summary: bool,
) -> Result<Vec<ReviewRow>, String> {
    rows_light_in_paged(t, from, to, owner, with_summary, ROWS_PAGE)
}

fn rows_light_in_paged(
    t: &TursoDb,
    from: &str,
    to: &str,
    owner: Option<&str>,
    with_summary: bool,
    page: usize,
) -> Result<Vec<ReviewRow>, String> {
    let cols = if with_summary {
        LIGHT_COLS_WITH_SUMMARY
    } else {
        LIGHT_COLS
    };
    read_rows(t, cols, (from, to), owner, page, |r| {
        light_row_from(r, with_summary)
    })
}

/// 変更前の読み方 (1 回の問い合わせで期間の全行を全部の列で読む)。ページ読み・軽い読み方の答えの基準と、
/// メモリ計測の「前」に使う (テスト専用)
#[cfg(test)]
pub(crate) fn rows_in_single_query(
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
                answer: None,
                asked: None,
                answered: None,
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
            // 担当者を Zoom でかけた人に直すと、その人の行として読める
            set_owner(t, "c1", "o7", "2026-10-15T00:50:00Z")?;
            assert_eq!(get(t, "c1")?.unwrap().owner_id, "o7");
            assert_eq!(
                rows_in(
                    t,
                    "2026-10-15T00:00:00Z",
                    "2026-10-16T00:00:00Z",
                    Some("o7")
                )?
                .len(),
                1
            );
            set_owner(t, "c1", "o1", "2026-10-15T00:51:00Z")?;
            // 判定済みの行の一覧と、文章だけの書き換え
            assert_eq!(judged_since(t, "2026-10-01T00:00:00Z")?.len(), 1);
            let mut cleaned = out.items.clone();
            cleaned[0].advice = None;
            mark_text_cleaned(
                t,
                "c1",
                &cleaned,
                Some("判定の文章を作れませんでした"),
                "2026-10-15T00:50:00Z",
            )?;
            let r = get(t, "c1")?.unwrap();
            assert_eq!(r.items[0].advice, None);
            assert_eq!(r.summary.as_deref(), Some("判定の文章を作れませんでした"));
            assert_eq!(r.outcome.as_deref(), Some("決まらなかった"));
            assert_eq!(r.items[0].status, "not_done");
            mark_judged(
                t,
                "c1",
                1,
                "abcd1234",
                "MiniMax-M3",
                &out,
                "2026-10-15T00:51:00Z",
            )?;
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
                note: Some("contacted".into()),
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
            // 接触を調べていない古い行 (note が空) は枠に入らず、新しい電話の枠も奪わない
            let mut u = q("u1", "o5", "2026-10-15T04:00:00Z");
            u.note = None;
            insert_new(t, "2026-10-15T05:05:00Z", &u)?;
            promote(t, "2026-10-01T00:00:00Z", 2, "2026-10-15T05:11:00Z")?;
            assert_eq!(status("u1")?, ST_QUEUED);
            assert_eq!(status("a1")?, ST_QUEUED);
            assert_eq!(queued_unmarked(t, "2026-10-01T00:00:00Z")?.len(), 1);
            mark_contacted(t, &["u1".to_string()], "2026-10-15T05:12:00Z")?;
            assert_eq!(queued_unmarked(t, "2026-10-01T00:00:00Z")?.len(), 0);
            assert_eq!(get(t, "u1")?.unwrap().note.as_deref(), Some("contacted"));
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
            assert_eq!(d[0].hubspot_call_id, "u1");
            assert_eq!(d[1].hubspot_call_id, "a2");
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

    // ---- ページ読み・軽い読み方 ----------------------------------------------------------------

    use crate::audit::fake_turso::{explain_plan, start_sqlite_audit_with_queries, SharedConn};

    /// 本番に近い、引用・助言・メモ・根拠つきの項目 (ヒアリングの項目は先方の答えつき)
    fn full_items(i: usize) -> Vec<StoredItem> {
        let status = ["done", "not_done", "na"][i % 3];
        vec![
            StoredItem {
                id: "early_date".into(),
                status: status.into(),
                evidence: vec![i as u32, 7],
                quote: Some("来週の火曜日でどうですか".into()),
                advice: Some("先に相手の状況を聞いてから日程を出す".into()),
                note: Some(format!("{i} 本目のメモ")),
                answer: None,
                asked: None,
                answered: None,
            },
            StoredItem {
                id: "aq_budget".into(),
                status: "done".into(),
                evidence: vec![3],
                quote: Some("月に二十万円くらいです".into()),
                advice: None,
                note: Some("予算は具体額".into()),
                answer: Some("具体額".into()),
                asked: Some(true),
                answered: Some(true),
            },
        ]
    }

    /// 偽 Turso の SQLite に、行を直接入れる (rowid は入れた順)。`items` は JSON の文字列そのまま
    fn seed(
        conn: &SharedConn,
        id: &str,
        owner: &str,
        at: &str,
        status: &str,
        items: Option<&str>,
        summary: Option<&str>,
    ) {
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, \
                 zoom_call_log_id, status, attempts, checks_version, model, outcome, reached_person, \
                 reception, first_date_turn, items, summary, note, created_at, updated_at) \
                 VALUES (?1, ?2, ?3, 95, 'zoom-' || ?1, ?4, 2, 'v1', 'MiniMax-M3', '決まらなかった', 1, 0, 3, \
                 ?5, ?6, 'contacted', 'c', 'u')",
                rusqlite::params![id, owner, at, status, items, summary],
            )
            .unwrap();
    }

    /// 期間 [2026-10-15T00:00:00Z, 2026-10-17T00:00:00Z) に 11 行、外に 2 行。
    /// 同じ call_at の行 (2026-10-16T05:00 の 4 本、2026-10-15T12:00 の 2 本) は rowid の順がページの境目をまたぐ
    fn seed_period(conn: &SharedConn) {
        let judged = |i: usize| serde_json::to_string(&full_items(i)).unwrap();
        let rows: [(&str, &str, &str, &str, Option<String>); 13] = [
            (
                "before1",
                "o1",
                "2026-10-14T23:59:59Z",
                "judged",
                Some(judged(0)),
            ),
            (
                "t1",
                "o1",
                "2026-10-16T05:00:00Z",
                "judged",
                Some(judged(1)),
            ),
            (
                "t2",
                "o2",
                "2026-10-16T05:00:00Z",
                "judged",
                Some(judged(2)),
            ),
            (
                "t3",
                "o1",
                "2026-10-16T05:00:00Z",
                "pending_transcript",
                None,
            ),
            (
                "u1",
                "o3",
                "2026-10-15T00:00:00Z",
                "judged",
                Some(judged(3)),
            ),
            (
                "t4",
                "o2",
                "2026-10-16T05:00:00Z",
                "judged",
                Some(judged(4)),
            ),
            ("v1", "o1", "2026-10-15T12:00:00Z", "unavailable", None),
            (
                "v2",
                "o1",
                "2026-10-15T12:00:00Z",
                "judged",
                Some(judged(5)),
            ),
            (
                "w1",
                "o2",
                "2026-10-16T23:59:59Z",
                "judged",
                Some(judged(6)),
            ),
            (
                "after1",
                "o1",
                "2026-10-17T00:00:00Z",
                "judged",
                Some(judged(7)),
            ),
            // items が壊れている行は、全部の列の読み方でも空になる。軽い読み方も同じ
            (
                "bad1",
                "o3",
                "2026-10-15T06:00:00Z",
                "judged",
                Some("not json".into()),
            ),
            ("f1", "o1", "2026-10-15T18:00:00Z", "failed", None),
            ("q1", "o2", "2026-10-15T19:00:00Z", "queued", None),
        ];
        for (id, owner, at, status, items) in rows {
            let summary = (status == "judged").then(|| format!("{id} のまとめ: 日程が早い"));
            seed(
                conn,
                id,
                owner,
                at,
                status,
                items.as_deref(),
                summary.as_deref(),
            );
        }
    }

    const FROM: &str = "2026-10-15T00:00:00Z";
    const TO: &str = "2026-10-17T00:00:00Z";
    /// 期間の中の行の新しい順 (call_at の降順、同じなら rowid の降順 = 入れた順の逆)
    const ALL_IDS: [&str; 11] = [
        "w1", "t4", "t3", "t2", "t1", "q1", "f1", "v2", "v1", "bad1", "u1",
    ];

    fn ids(rows: &[ReviewRow]) -> Vec<&str> {
        rows.iter().map(|r| r.hubspot_call_id.as_str()).collect()
    }

    /// 全部の列の行から、軽い読み方が返すはずの行を作る
    fn light_of(full: &ReviewRow, with_summary: bool) -> ReviewRow {
        ReviewRow {
            zoom_call_log_id: None,
            attempts: 0,
            checks_version: None,
            model: None,
            reached_person: None,
            reception: None,
            first_date_turn: None,
            note: None,
            items: full
                .items
                .iter()
                .map(|i| StoredItem {
                    evidence: vec![],
                    quote: None,
                    advice: None,
                    note: None,
                    ..i.clone()
                })
                .collect(),
            summary: full.summary.clone().filter(|_| with_summary),
            ..full.clone()
        }
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn paged_reads_return_the_same_rows_in_the_same_order_as_one_query() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        seed_period(&conn);
        blocking(&audit, move |t| {
            // 基準: 変更前の読み方 (1 回の問い合わせ・全部の列)
            let single = rows_in_single_query(t, FROM, TO, None)?;
            assert_eq!(single.len(), 11);
            // 同じ call_at の行は rowid の大きい順 (入れた順の逆) で返る。これが今の並び
            assert_eq!(ids(&single), ALL_IDS);
            assert_eq!(single[0].call_at, "2026-10-16T23:59:59Z");
            assert_eq!(single[10].call_at, "2026-10-15T00:00:00Z");
            // 範囲は [from, to): call_at が from と同じ行は入り、to と同じ行は入らない
            assert!(single.iter().all(|r| r.hubspot_call_id != "after1"));
            assert!(single.iter().any(|r| r.hubspot_call_id == "u1"));

            let count_reads = |queries: &std::sync::Mutex<Vec<String>>| {
                let mut g = queries.lock().unwrap();
                let n = g
                    .iter()
                    .filter(|q| q.contains("FROM call_talk_reviews") && q.contains("rowid AS rid"))
                    .count();
                g.clear();
                n
            };
            for page in [1, 2, 3, 4, 5, 10, 11, 12, 500] {
                count_reads(&queries);
                let full = rows_in_paged(t, FROM, TO, None, page)?;
                assert_eq!(full, single, "全部の列・ページ {page}");
                // 11 行を page 行ずつ読む。最後のページが page 行ちょうどなら、空のページをもう 1 回読む
                assert_eq!(
                    count_reads(&queries),
                    11 / page + 1,
                    "ページ {page} の問い合わせ回数"
                );
                for with_summary in [true, false] {
                    let light = rows_light_in_paged(t, FROM, TO, None, with_summary, page)?;
                    let expected: Vec<ReviewRow> =
                        single.iter().map(|r| light_of(r, with_summary)).collect();
                    assert_eq!(
                        light, expected,
                        "軽い・summary={with_summary}・ページ {page}"
                    );
                    assert_eq!(count_reads(&queries), 11 / page + 1);
                }
            }
            // 担当者で絞った読み方も、基準の絞り込みと同じ行・同じ順
            for (owner, want) in [
                ("o1", vec!["t3", "t1", "f1", "v2", "v1"]),
                ("o2", vec!["w1", "t4", "t2", "q1"]),
                ("o3", vec!["bad1", "u1"]),
                ("nobody", vec![]),
            ] {
                let single = rows_in_single_query(t, FROM, TO, Some(owner))?;
                assert_eq!(ids(&single), want, "{owner}");
                for page in [1, 2, 3, 500] {
                    assert_eq!(rows_in_paged(t, FROM, TO, Some(owner), page)?, single);
                    assert_eq!(
                        rows_light_in_paged(t, FROM, TO, Some(owner), true, page)?,
                        single.iter().map(|r| light_of(r, true)).collect::<Vec<_>>(),
                        "{owner}・ページ {page}"
                    );
                }
            }
            // 期間の行が 0 件なら、問い合わせは 1 回で終わる
            count_reads(&queries);
            assert!(rows_light_in_paged(
                t,
                "2026-11-01T00:00:00Z",
                "2026-11-02T00:00:00Z",
                None,
                false,
                3
            )?
            .is_empty());
            assert_eq!(count_reads(&queries), 1);
            // 既定のページの大きさでも同じ
            assert_eq!(rows_in(t, FROM, TO, None)?, single_all(t)?);
            assert_eq!(
                rows_light_in(t, FROM, TO, None, true)?,
                single_all(t)?
                    .iter()
                    .map(|r| light_of(r, true))
                    .collect::<Vec<_>>()
            );
            Ok(())
        })
        .await
        .unwrap();
    }

    fn single_all(t: &TursoDb) -> Result<Vec<ReviewRow>, String> {
        rows_in_single_query(t, FROM, TO, None)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn light_rows_drop_the_text_but_keep_what_the_summary_and_the_list_use() {
        let (audit, conn, _) = start_sqlite_audit_with_queries().await;
        seed_period(&conn);
        blocking(&audit, move |t| {
            let rows = rows_light_in(t, FROM, TO, Some("o2"), true)?;
            // o2 の判定済み 3 本 (w1・t4・t2) と、順番待ちの 1 本 (q1)
            assert_eq!(ids(&rows), vec!["w1", "t4", "t2", "q1"]);
            let w1 = &rows[0];
            assert_eq!(w1.hubspot_call_id, "w1");
            assert_eq!(w1.owner_id, "o2");
            assert_eq!(w1.call_at, "2026-10-16T23:59:59Z");
            assert_eq!(w1.duration_sec, 95);
            assert_eq!(w1.status, ST_JUDGED);
            assert_eq!(w1.outcome.as_deref(), Some("決まらなかった"));
            assert_eq!(w1.summary.as_deref(), Some("w1 のまとめ: 日程が早い"));
            // 項目は id と判定 (i = 6 なので 6 % 3 = 0 → done)、ヒアリングの答えだけ残る
            assert_eq!(w1.items.len(), 2);
            assert_eq!(
                (w1.items[0].id.as_str(), w1.items[0].status.as_str()),
                ("early_date", "done")
            );
            assert_eq!(w1.items[0].answer, None);
            assert_eq!(
                (
                    w1.items[1].id.as_str(),
                    w1.items[1].status.as_str(),
                    w1.items[1].answer.as_deref(),
                    w1.items[1].asked,
                    w1.items[1].answered
                ),
                ("aq_budget", "done", Some("具体額"), Some(true), Some(true))
            );
            // 引用・助言・メモ・根拠の番号は空
            for it in &w1.items {
                assert_eq!(it.evidence, Vec::<u32>::new());
                assert_eq!((&it.quote, &it.advice, &it.note), (&None, &None, &None));
            }
            // 使わない欄は既定値
            assert_eq!(w1.zoom_call_log_id, None);
            assert_eq!(w1.attempts, 0);
            assert_eq!((&w1.checks_version, &w1.model), (&None, &None));
            assert_eq!(
                (w1.reached_person, w1.reception, w1.first_date_turn),
                (None, None, None)
            );
            assert_eq!(w1.note, None);
            // 順番待ち・失敗の行は項目が空
            assert!(rows[3].items.is_empty());
            assert_eq!(rows[3].status, ST_QUEUED);

            // summary を読まない読み方では summary が空
            let no_summary = rows_light_in(t, FROM, TO, Some("o2"), false)?;
            assert!(no_summary.iter().all(|r| r.summary.is_none()));
            assert_eq!(no_summary[0].items, w1.items);

            // 全部の列の読み方 (詳細の画面の 1 件) は引用・助言・メモ・根拠をそのまま返す
            let detail = get(t, "w1")?.unwrap();
            assert_eq!(detail.items, full_items(6));
            assert_eq!(detail.zoom_call_log_id.as_deref(), Some("zoom-w1"));
            assert_eq!(detail.checks_version.as_deref(), Some("v1"));
            // 壊れた items は、全部の列でも軽い読み方でも空
            let bad = get(t, "bad1")?.unwrap();
            assert!(bad.items.is_empty());
            let light_bad = rows_light_in(t, FROM, TO, Some("o3"), false)?;
            let bad = light_bad
                .iter()
                .find(|r| r.hubspot_call_id == "bad1")
                .unwrap();
            assert_eq!(bad.status, ST_JUDGED);
            assert!(bad.items.is_empty());
            Ok(())
        })
        .await
        .unwrap();
    }

    /// 期間の索引を逆に走査し、並べ替えが要らない問い合わせになっている
    /// (1 ページ目も 2 ページ目以降も、担当者で絞っても)。ページを重ねても読む行が増えない
    #[tokio::test(flavor = "multi_thread")]
    async fn row_pages_use_the_call_at_index_without_sorting() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        seed_period(&conn);
        let q = queries.clone();
        blocking(&audit, move |t| {
            q.lock().unwrap().clear();
            rows_light_in_paged(t, FROM, TO, None, false, 4)?;
            rows_light_in_paged(t, FROM, TO, Some("o1"), true, 2)?;
            Ok(())
        })
        .await
        .unwrap();
        let reads: Vec<String> = queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.contains("FROM call_talk_reviews") && q.contains("rowid AS rid"))
            .cloned()
            .collect();
        // 全員 11 行を 4 行ずつ (4 + 4 + 3 の 3 回)、o1 の 5 行を 2 行ずつ (2 + 2 + 1 の 3 回)
        assert_eq!(reads.len(), 6);
        assert!(
            !reads[0].contains("rowid < ?3"),
            "1 ページ目に続きの条件はない"
        );
        assert!(reads[1].contains("call_at <= ?2 AND (call_at < ?2 OR rowid < ?3)"));
        assert!(reads[1].contains("ORDER BY call_at DESC, rowid DESC LIMIT 4"));
        assert!(reads[4].contains("AND owner_id = ?4"));
        for sql in &reads {
            let plan = explain_plan(&conn, sql).join(" / ");
            assert!(plan.contains("idx_call_talk_reviews_call_at"), "{plan}");
            assert!(!plan.contains("TEMP B-TREE"), "{plan}");
        }
    }
}
