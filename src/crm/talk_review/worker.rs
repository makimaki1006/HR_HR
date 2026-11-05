//! 背景の worker (ADR-033)。`CALL_REVIEW_ENABLED=1` のときだけ動く。
//!
//! 10 分おき (既定) に、対象のチームの担当者の Call (Zoom 純正連携・発信・つながったもの) を
//! Search (関所の `Background` 優先度) で見つけ、接触の判定 (`contact_worker.rs`) で `contacted = 1` になった電話だけ
//! 表に行を作り (順番待ち `queued`、2026-10-26。長さの線は無い)、1 人 1 日あたり
//! `CALL_REVIEW_PER_PERSON_DAY` 本まで新しい電話から文字起こし待ちにして、Zoom から文字起こしを取って判定する。
//! 初回は過去 `CALL_REVIEW_BACKFILL_DAYS` 日まで遡る。同時に `CALL_REVIEW_CONCURRENCY` 本。判定済みの Call は二度判定しない。
//! 文字起こしがまだ無いものは間隔を空けて再試行し (上限あり)、取れないものは `unavailable` にして諦める
//! (取れなくなった電話の分は、同じ人・同じ日の次の電話が枠に入る)。
//! 録音 URL が Call に無いもの (Call Queue 経由の発信) は、Zoom の通話履歴から相手番号と時刻で録音を探す。
//! アポの日ごとの数え (`counts`) も、ここで前もって新しい日から進める。

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::apo_link;
use super::checks::{loaded, Checks};
use super::config::TalkReviewConfig;
use super::counts::TeamOwners;
use super::judge::{
    hearing_na, is_hearing, sanitize_texts, source_text, summary_of, texts_foreign, Judge,
    JudgeError, JudgeOutput, MiniMaxJudge, StoredItem, NO_APO_NOTE, SCHEDULED,
};
use super::store::{self, NewRow, ReviewRow, ST_FAILED, ST_QUEUED, ST_UNAVAILABLE};
use super::target::{self, MarkState, Verdict, CONTACTED_NOTE};
use super::transcript::{extract_call_log_id, has_both_sides};
use super::zoom::{TranscriptSource, ZoomClient, ZoomError, REC_PREFIX};
use crate::crm::caller_metrics::{flt, jst_day, CALL_SOURCE};
use crate::crm::caller_metrics_month::{jst_midnight_ms, OWNER_CHUNK};
use crate::crm::owners::CrmOwner;
use crate::db::turso_http::TursoDb;
use crate::hubspot::HubSpotClient;
use crate::AppState;

/// 再試行の上限 (これに達したら取れない / 失敗として諦める)
pub const MAX_ATTEMPTS: u32 = 8;
/// 1 回の見つけ直しの Search の上限 (暴走の安全装置)
const MAX_SEARCHES: u32 = 300;
/// 1 周の中で処理する行の上限
const MAX_PER_CYCLE: usize = 1_000;
/// Search の下限。10 秒未満は Zoom が文字起こしできず (code 12000)、接触の判定も付かない
const MIN_CONNECTED_SEC: i64 = 10;
/// 録音 URL がまだ無い Call を「録音なし」にするまでの猶予
const URL_GRACE: chrono::Duration = chrono::Duration::hours(2);
/// 古くて録音が消えたとみなす Call の年齢
const GONE_AFTER: chrono::Duration = chrono::Duration::days(1);

/// 2 回目以降に読む範囲の始まり (`hs_lastmodifieddate`)。前回の 30 分前か、
/// 録音 URL の猶予 + 30 分前の早い方。URL の無い Call (外部アポインターの通話など) は
/// 作られた後に更新されないので、猶予が明けるまで読み続けないと取り込まれない
fn poll_from(prev: DateTime<Utc>, now: DateTime<Utc>, backfill_from: i64) -> i64 {
    let margin = chrono::Duration::minutes(30);
    (prev - margin)
        .min(now - URL_GRACE - margin)
        .timestamp_millis()
        .max(backfill_from)
}

/// 次の再試行までの間隔 (試行回数ごと)
pub fn backoff(attempts: u32) -> chrono::Duration {
    let mins = [10, 30, 60, 120, 240, 480, 720];
    chrono::Duration::minutes(mins[(attempts.max(1) as usize - 1).min(mins.len() - 1)])
}

// ---------------------------------------------------------------------------
// チームの担当者
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub struct TeamMember {
    pub id: String,
    pub name: String,
    pub team: String,
}

/// 担当者の一覧から対象のチームの人 (チームは `teams` の並びで最初に当たったもの)。退職 (archived) も含める
/// (退職前の電話が期間に入るため。何も無い人は表に出ない)。
pub fn team_members(owners: &[CrmOwner], teams: &[String]) -> Vec<TeamMember> {
    owners
        .iter()
        .filter_map(|o| {
            teams
                .iter()
                .find(|t| o.teams.contains(t))
                .map(|t| TeamMember {
                    id: o.id.clone(),
                    name: o.name.clone(),
                    team: t.clone(),
                })
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Search
// ---------------------------------------------------------------------------

/// HubSpot の Search は 1 まとまり 6 条件まで (7 個で 400)。通話の日時の下限 (`ts_from`) は
/// 条件に入れず、`search_candidates` が受け取ってから絞る
pub fn search_body(
    range_prop: &str,
    from: i64,
    to: i64,
    owners: &[String],
    min_ms: i64,
    after: Option<&str>,
) -> Value {
    let filters = vec![
        flt("hs_call_source", "EQ", json!(CALL_SOURCE)),
        flt("hs_call_direction", "EQ", json!("OUTBOUND")),
        flt("hs_call_duration", "GTE", json!(min_ms.to_string())),
        flt(range_prop, "GTE", json!(from.to_string())),
        flt(range_prop, "LT", json!(to.to_string())),
        json!({"propertyName": "hubspot_owner_id", "operator": "IN", "values": owners}),
    ];
    let mut body = json!({
        "filterGroups": [{"filters": filters}],
        "properties": ["hubspot_owner_id", "hs_timestamp", "hs_call_duration", "hs_call_body", "hs_call_to_number"],
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": 100,
    });
    if let Some(a) = after {
        body["after"] = json!(a);
    }
    body
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub id: String,
    pub owner_id: String,
    pub ts_ms: i64,
    pub duration_ms: i64,
    pub zoom_id: Option<String>,
    /// 相手番号 (`hs_call_to_number`)。かけた人を Zoom で探すのに使う
    pub to_number: Option<String>,
}

fn time_ms(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => s.parse().ok().or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis())
        }),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

pub fn parse_candidate(v: &Value) -> Option<Candidate> {
    let id = match v.get("id")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let p = v.get("properties")?;
    let owner_id = p.get("hubspot_owner_id")?.as_str()?.trim().to_string();
    if owner_id.is_empty() {
        return None;
    }
    Some(Candidate {
        id,
        owner_id,
        ts_ms: time_ms(p.get("hs_timestamp")?)?,
        duration_ms: p.get("hs_call_duration").and_then(time_ms).unwrap_or(0),
        zoom_id: p
            .get("hs_call_body")
            .and_then(Value::as_str)
            .and_then(extract_call_log_id),
        to_number: p
            .get("hs_call_to_number")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string),
    })
}

/// 行に付ける担当者。Zoom でかけた人が HubSpot の担当者に結び付けば、その人。
/// 結び付かない (Zoom に見つからない・担当者に居ない) ときは HubSpot の担当者のまま
pub fn attribute(
    hubspot_owner: &str,
    caller_email: Option<&str>,
    email_owner: &HashMap<String, String>,
) -> String {
    caller_email
        .and_then(|e| email_owner.get(&e.to_lowercase()))
        .cloned()
        .unwrap_or_else(|| hubspot_owner.to_string())
}

/// 行の担当者を Zoom でかけた人に直す。Zoom の読み取りに失敗したら `Err` (呼び出し側はやり直す)
async fn caller_owner<Z: TranscriptSource>(
    zoom: &Z,
    hubspot_owner: &str,
    to_number: Option<&str>,
    at_ms: i64,
    email_owner: &HashMap<String, String>,
) -> Result<String, String> {
    let email = match to_number {
        Some(n) => match zoom.find_caller(n, at_ms).await {
            Ok(e) => e,
            Err(ZoomError::Transient(reason)) => return Err(format!("zoom call logs: {reason}")),
            Err(_) => None,
        },
        None => None,
    };
    Ok(attribute(hubspot_owner, email.as_deref(), email_owner))
}

async fn search_candidates(
    client: &HubSpotClient,
    range_prop: &str,
    from: i64,
    to: i64,
    ts_from: Option<i64>,
    owners: &[String],
    min_ms: i64,
) -> Result<Vec<Candidate>, crate::hubspot::HubSpotError> {
    let mut out = Vec::new();
    let mut searches = 0u32;
    for chunk in owners.chunks(OWNER_CHUNK) {
        let mut after: Option<String> = None;
        loop {
            searches += 1;
            if searches > MAX_SEARCHES {
                return Err(crate::hubspot::HubSpotError::Decode(
                    "too many searches".into(),
                ));
            }
            let v = client
                .search(
                    "calls",
                    search_body(range_prop, from, to, chunk, min_ms, after.as_deref()),
                )
                .await?;
            let results = v.get("results").and_then(Value::as_array).ok_or_else(|| {
                crate::hubspot::HubSpotError::Decode("calls without results".into())
            })?;
            out.extend(
                results
                    .iter()
                    .filter_map(parse_candidate)
                    .filter(|c| ts_from.is_none_or(|t| c.ts_ms >= t)),
            );
            match v
                .pointer("/paging/next/after")
                .and_then(Value::as_str)
                .filter(|s| !s.is_empty())
            {
                Some(n) => after = Some(n.to_string()),
                None => break,
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// 1 本の処理 (HubSpot・Zoom・判定は差し替え可能)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Judged(JudgeOutput),
    /// 後で再試行 (理由の短い記号)
    Retry(&'static str),
    Unavailable(&'static str),
}

pub async fn process_one<Z: TranscriptSource, J: Judge>(
    zoom: &Z,
    judge: &J,
    row: &ReviewRow,
    now: DateTime<Utc>,
) -> Outcome {
    let Some(zid) = row.zoom_call_log_id.as_deref() else {
        return Outcome::Unavailable("no_recording_url");
    };
    let turns = match zoom.fetch(zid).await {
        Ok(t) => t,
        Err(ZoomError::NotReady) => return Outcome::Retry("transcript_not_ready"),
        Err(ZoomError::Untranscribable) => return Outcome::Unavailable("untranscribable"),
        Err(ZoomError::Gone) => {
            let old = DateTime::parse_from_rfc3339(&row.call_at)
                .is_ok_and(|t| now - t.with_timezone(&Utc) >= GONE_AFTER);
            return if old {
                Outcome::Unavailable("recording_gone")
            } else {
                Outcome::Retry("recording_not_found_yet")
            };
        }
        Err(ZoomError::Transient(reason)) => {
            // 理由は短い記号だけ (http 429・token: 接続の種類 など)。URL・鍵は含まない
            tracing::info!(%reason, "talk review: zoom transcript failed");
            return Outcome::Retry("zoom_error");
        }
        Err(ZoomError::NotConfigured) => return Outcome::Retry("zoom_not_configured"),
    };
    // 200 でも発言が 1 件も無い文字起こしは、まだ空 (後で取り直す)
    if turns.is_empty() {
        return Outcome::Retry("transcript_empty");
    }
    // 発言があるのに営業か先方のどちらかしか居ないものは、判定できない
    if !has_both_sides(&turns) {
        return Outcome::Unavailable("speakers_not_separated");
    }
    match judge.judge(&loaded().checks, &turns).await {
        Ok(o) => Outcome::Judged(o),
        Err(JudgeError::Transient(reason)) => {
            // 理由は短い記号だけ (http 429・json・send: 接続の種類)。本文・鍵は含まない
            tracing::info!(%reason, "talk review: judge failed");
            Outcome::Retry("judge_error")
        }
        Err(JudgeError::NotConfigured) => Outcome::Retry("judge_not_configured"),
    }
}

/// 録音 ID の見つけ方の結果
#[derive(Debug, Clone, PartialEq)]
pub enum Resolve {
    /// 行がすでに録音 ID を持っている
    Have,
    /// 通話履歴から見つかった (`zoom_call_log_id` に入れる値)
    Found(String),
    /// 見つからない・探せない (そのまま結果になる)
    Done(Outcome),
}

/// 録音 URL が Call に無い行について、Zoom の通話履歴から録音を探す。`number` は HubSpot の `hs_call_to_number`
pub async fn resolve_recording<Z: TranscriptSource>(
    zoom: &Z,
    row: &ReviewRow,
    number: Option<&str>,
) -> Resolve {
    if row.zoom_call_log_id.is_some() {
        return Resolve::Have;
    }
    let not_found = Resolve::Done(Outcome::Unavailable("no_recording_found"));
    let (Some(number), Ok(at)) = (
        number.filter(|n| !n.trim().is_empty()),
        DateTime::parse_from_rfc3339(&row.call_at),
    ) else {
        return not_found;
    };
    match zoom.find_recording(number, at.timestamp_millis()).await {
        Ok(Some(rec)) => Resolve::Found(format!("{REC_PREFIX}{rec}")),
        Ok(None) | Err(ZoomError::NotReady | ZoomError::Gone | ZoomError::Untranscribable) => {
            not_found
        }
        Err(ZoomError::Transient(reason)) => {
            tracing::info!(%reason, "talk review: zoom call history failed");
            Resolve::Done(Outcome::Retry("zoom_error"))
        }
        Err(ZoomError::NotConfigured) => Resolve::Done(Outcome::Retry("zoom_not_configured")),
    }
}

/// 結果を表の更新に直す。(新しい状態, 試行回数, 次回時刻, メモ) の決め方
#[derive(Debug, Clone, PartialEq)]
pub enum Update {
    Judged(JudgeOutput, u32),
    Retry {
        attempts: u32,
        next: DateTime<Utc>,
        note: &'static str,
    },
    Final {
        status: &'static str,
        attempts: u32,
        note: &'static str,
    },
}

pub fn decide(outcome: Outcome, prev_attempts: u32, now: DateTime<Utc>) -> Update {
    let attempts = prev_attempts + 1;
    match outcome {
        Outcome::Judged(o) => Update::Judged(o, attempts),
        Outcome::Unavailable(note) => Update::Final {
            status: ST_UNAVAILABLE,
            attempts,
            note,
        },
        Outcome::Retry(note) if attempts >= MAX_ATTEMPTS => Update::Final {
            // 判定そのものが続けて失敗したものは failed、録音・文字起こしが取れないものは unavailable
            status: if note.starts_with("judge") {
                ST_FAILED
            } else {
                ST_UNAVAILABLE
            },
            attempts,
            note,
        },
        Outcome::Retry(note) => Update::Retry {
            attempts,
            next: now + backoff(attempts),
            note,
        },
    }
}

/// 判定は済んだが表への書き込みに失敗したもの。同じ Call を判定し直さず、次の周回で書き込みだけやり直す
#[derive(Default)]
pub struct PendingWrites {
    inner: std::sync::Mutex<std::collections::HashMap<String, (JudgeOutput, u32, String)>>,
}

impl PendingWrites {
    pub fn contains(&self, id: &str) -> bool {
        self.inner.lock().unwrap().contains_key(id)
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn keep(&self, id: String, o: JudgeOutput, attempts: u32, model: String) {
        self.inner.lock().unwrap().insert(id, (o, attempts, model));
    }

    /// 溜まっている書き込みをやり直す。書けたものは忘れる
    pub async fn flush(&self, audit: &crate::audit::AuditDb) {
        let items: Vec<(String, (JudgeOutput, u32, String))> = self
            .inner
            .lock()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        for (id, (o, attempts, model)) in items {
            if write_judged(audit, &id, &o, attempts, &model).await.is_ok() {
                self.inner.lock().unwrap().remove(&id);
            }
        }
    }
}

async fn write_judged(
    audit: &crate::audit::AuditDb,
    id: &str,
    o: &JudgeOutput,
    attempts: u32,
    model: &str,
) -> Result<(), String> {
    let (id, o, model) = (id.to_string(), o.clone(), model.to_string());
    let now = store::ts_text(Utc::now());
    let version = loaded().version.clone();
    store::blocking(audit, move |t| {
        store::mark_judged(t, &id, attempts, &version, &model, &o, &now)
    })
    .await
}

async fn apply(
    audit: &crate::audit::AuditDb,
    pending: &PendingWrites,
    id: String,
    u: Update,
    model: String,
) {
    let now = store::ts_text(Utc::now());
    let res = match u {
        Update::Judged(o, attempts) => {
            let r = write_judged(audit, &id, &o, attempts, &model).await;
            if r.is_err() {
                // 判定結果はメモリに残し、次の周回で書き込みだけやり直す
                pending.keep(id.clone(), o, attempts, model);
            }
            r
        }
        Update::Retry {
            attempts,
            next,
            note,
        } => {
            let id = id.clone();
            store::blocking(audit, move |t| {
                store::mark_retry(t, &id, attempts, &store::ts_text(next), note, &now)
            })
            .await
        }
        Update::Final {
            status,
            attempts,
            note,
        } => {
            let id = id.clone();
            store::blocking(audit, move |t| {
                store::mark_final(t, &id, status, attempts, note, &now)
            })
            .await
        }
    };
    if let Err(e) = res {
        tracing::warn!(error = %e, "talk review: result write failed");
    }
}

/// ヒアリングの周回の書き込み: 項目とまとめだけ (outcome などは変えない)。UPDATE 1 回
async fn write_items(audit: &crate::audit::AuditDb, id: String, items: Vec<StoredItem>) {
    let summary = summary_of(&items);
    let now = store::ts_text(Utc::now());
    if let Err(e) = store::blocking(audit, move |t| {
        store::mark_text_cleaned(t, &id, &items, summary.as_deref(), &now)
    })
    .await
    {
        tracing::warn!(error = %e, "talk review: hearing write failed");
    }
}

/// ヒアリングの書き込みの結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum HearingWrite {
    /// 書いた (書き込みの失敗は記録して終わる。従来どおり)
    Written,
    /// 行が無い・判定済みでない・もう済んでいる。書かずに、確かめる一覧から外す
    Skipped,
    /// 行を読み直せなかった。書かずに、後で確かめ直す
    ReadFailed,
}

/// 今の行にヒアリングを足す (`attach`) か外す (`!attach`) 必要があるか。
/// 行が判定済みで、足すならまだ付いていない・外すなら付いているときだけ
fn hearing_write_needed(row: &ReviewRow, checks: &Checks, attach: bool) -> bool {
    row.status == store::ST_JUDGED && has_hearing(row, checks) != attach
}

/// 書く直前に行を読み直し、**今の** 項目にヒアリングの項目を足して書く (UPDATE 1 回)。
/// 確かめる一覧は軽い型で項目を持たないうえ、足す内容を作る間 (文字起こし・判定) に行が変わることがあるため。
/// `attach` は、ヒアリングを付ける (まだ付いていない行だけ) か、対象外にする (付いている行だけ) か
async fn rewrite_hearing(
    audit: &crate::audit::AuditDb,
    id: &str,
    hearing: Vec<StoredItem>,
    attach: bool,
) -> HearingWrite {
    let checks = &loaded().checks;
    let key = id.to_string();
    let row = match store::blocking(audit, move |t| store::get(t, &key)).await {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!(error = %e, "talk review: hearing row read failed");
            return HearingWrite::ReadFailed;
        }
    };
    let Some(row) = row.filter(|r| hearing_write_needed(r, checks, attach)) else {
        return HearingWrite::Skipped;
    };
    write_items(
        audit,
        id.to_string(),
        merge_hearing(checks, &row.items, hearing),
    )
    .await;
    HearingWrite::Written
}

// ---------------------------------------------------------------------------
// 1 周
// ---------------------------------------------------------------------------

/// 接触の判定を待っている Call (担当者は Zoom でかけた人に直したもの)
#[derive(Debug, Clone)]
pub struct Waiting {
    pub cand: Candidate,
    pub owner: String,
}

#[derive(Debug, Default)]
pub struct Settled {
    pub targets: Vec<Waiting>,
    pub skipped: Vec<String>,
    pub waiting: Vec<Waiting>,
}

/// 接触の印を読む窓 (担当者, 開始, 終了)。録音 URL のある Call も含める (純粋)。
/// 2026-10-26 に結び付けを担当者と時刻に変えたとき、URL のある Call (新規営業のほとんど) は ID で読んだままで、
/// 印が 1 本も読まれず、6 時間待って見送られていた (10/27〜11/04 の新規営業の判定が 0 本)
pub fn mark_windows(pool: &[Waiting]) -> Vec<(String, String, String)> {
    let window = chrono::Duration::milliseconds(super::zoom::CALLER_WINDOW_MS);
    pool.iter()
        .filter_map(|w| {
            let at = DateTime::<Utc>::from_timestamp_millis(w.cand.ts_ms)?;
            Some((
                w.owner.clone(),
                store::ts_text(at - window),
                store::ts_text(at + window),
            ))
        })
        .collect()
}

/// 待っている Call を、接触の印に照らして振り分ける (純粋)。録音 URL のある Call を先に結び、新しい電話から
pub fn settle(
    mut pool: Vec<Waiting>,
    marks: &[MarkState],
    used: &mut HashSet<String>,
    now: DateTime<Utc>,
) -> Settled {
    pool.sort_by_key(|w| (w.cand.zoom_id.is_none(), std::cmp::Reverse(w.cand.ts_ms)));
    let mut out = Settled::default();
    for w in pool {
        match target::verdict(
            w.cand.zoom_id.as_deref(),
            &w.owner,
            w.cand.ts_ms,
            marks,
            used,
        ) {
            Verdict::Target(_) => out.targets.push(w),
            Verdict::Skip => out.skipped.push(w.cand.id),
            Verdict::Wait(kind) => {
                let age = DateTime::<Utc>::from_timestamp_millis(w.cand.ts_ms)
                    .map_or(chrono::Duration::zero(), |at| now - at);
                if age > kind.limit() {
                    out.skipped.push(w.cand.id);
                } else {
                    out.waiting.push(w);
                }
            }
        }
    }
    out
}

struct Worker<Z, J> {
    audit: crate::audit::AuditDb,
    client: Arc<HubSpotClient>,
    zoom: Arc<Z>,
    judge: Arc<J>,
    cfg: TalkReviewConfig,
    known: HashSet<String>,
    /// 接触の判定を待っている Call (メモリだけ。再起動したら初回の読み込みで見つけ直す)
    waiting: HashMap<String, Waiting>,
    /// 接触していない・判定できない Call (読み直しで拾い直さない)
    skipped: HashSet<String>,
    /// 結び付けに使った接触の印 (時刻で結ぶ Call が同じ印を二度使わない)
    used_marks: HashSet<String>,
    /// 対象の担当者 (アポの数えに使う)
    team: Option<TeamOwners>,
    counts_task: Option<tokio::task::JoinHandle<()>>,
    last_poll: Option<DateTime<Utc>>,
    /// 初回の取り込みの進み具合 (次に読む日)。全部読み終えるまで `last_poll` は進めない
    backfill_next: Option<chrono::NaiveDate>,
    backfill_anchor: Option<DateTime<Utc>>,
    pending: Arc<PendingWrites>,
    /// 助言の文が日本語だけでない判定済みの行 (起動時に 1 回だけ探し、周回ごとに少しずつ判定し直す)。全部の欄を持つ
    rejudge: VecDeque<ReviewRow>,
    /// ヒアリングを付けるか決めていない判定済みの行 (判断に要る欄だけ。項目の引用・助言・まとめは持たない)。
    /// 起動時に直近の行を全部入れ、新しく判定した行は `judged_rows` から足す。
    /// HubSpot のアポの記録で決まったら取り除く (`apo_link`)
    hearing_check: HashMap<String, HearingCandidate>,
    /// この起動で判定した行 (判定のタスクが入れ、ヒアリングの周回が `hearing_check` に移す)
    judged_rows: Arc<std::sync::Mutex<Vec<HearingCandidate>>>,
}

/// 保存済みの行の文章に日本語以外が混ざっているか (英単語の許可は行の引用だけで判断する)
pub fn row_has_foreign_text(row: &ReviewRow) -> bool {
    let src = source_text(&[], &row.items);
    texts_foreign(&row.items, row.summary.as_deref(), &src)
}

/// 項目にヒアリングの判定が付いているか (ヒアリングの項目に「できた / できていない」が 1 つでもある)
fn items_have_hearing(items: &[StoredItem], checks: &Checks) -> bool {
    checks.checks.iter().filter(|c| is_hearing(c)).any(|c| {
        items
            .iter()
            .any(|i| i.id == c.id && matches!(i.status.as_str(), "done" | "not_done"))
    })
}

/// ヒアリングの判定が付いている行か (ヒアリングの項目に「できた / できていない」が 1 つでもある)
pub fn has_hearing(row: &ReviewRow, checks: &Checks) -> bool {
    items_have_hearing(&row.items, checks)
}

/// ヒアリングの周回で 1 行にすること
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HearingAction {
    /// アポの記録があり、ヒアリングがまだ無い: ヒアリングを付ける
    Run,
    /// アポの記録が無いまま待つ期間が過ぎ、ヒアリングが付いている: ヒアリングを対象外にする
    Strip,
    /// 決まった (何もしない。確かめる一覧から外す)
    Done,
    /// まだ待つ (後で確かめ直す)
    Later,
}

/// アポの記録の有無・待つ期間・ヒアリングの有無から、することを決める (純粋)
pub fn hearing_action(has: bool, matched: bool, waited: bool) -> HearingAction {
    match (matched, has, waited) {
        (true, false, _) => HearingAction::Run,
        (true, true, _) => HearingAction::Done,
        (false, true, true) => HearingAction::Strip,
        (false, false, true) => HearingAction::Done,
        (false, _, false) => HearingAction::Later,
    }
}

/// ヒアリングの周回が確かめる判定済みの行。判断に要る欄だけを持ち、項目の引用・助言・メモ・まとめは持たない。
/// 起動時に直近 (`backfill_days + 2` 日) の判定済みの行を全部入れ、決まるまで持ち続けるので、全部の欄のままでは
/// 数千本で数百 MB になる (2026-11-04 に本番がメモリの上限を超えて落ちた)。
/// 付ける・外すときは、`store::get` で今の行を読み直し、今の項目に足す (ここには項目が無い)
#[derive(Debug, Clone, PartialEq)]
pub struct HearingCandidate {
    pub id: String,
    /// 電話の時刻 (ms)
    pub call_ms: i64,
    /// 今の判定が「日程が決まった」(`hearing_priority` の材料)
    pub scheduled: bool,
    /// ヒアリングの判定が付いている (`has_hearing`)
    pub has_hearing: bool,
    /// 録音 ID (ヒアリングを付けるとき、文字起こしを取るのに使う)
    pub zoom_call_log_id: Option<String>,
    /// 次に確かめる時刻
    pub next: DateTime<Utc>,
}

impl HearingCandidate {
    /// 判定済みの行から作る。電話の時刻が読めない行は確かめようがないので `None`
    pub fn from_row(row: &ReviewRow, checks: &Checks, next: DateTime<Utc>) -> Option<Self> {
        Self::new(row, row.outcome.as_deref(), &row.items, checks, next)
    }

    /// 判定したばかりの行から作る。`row` は判定待ちだった行、`o` はその判定の結果 (outcome と項目は `o` のもの)
    pub fn from_judged(
        row: &ReviewRow,
        o: &JudgeOutput,
        checks: &Checks,
        next: DateTime<Utc>,
    ) -> Option<Self> {
        Self::new(row, o.outcome.as_deref(), &o.items, checks, next)
    }

    fn new(
        row: &ReviewRow,
        outcome: Option<&str>,
        items: &[StoredItem],
        checks: &Checks,
        next: DateTime<Utc>,
    ) -> Option<Self> {
        Some(Self {
            id: row.hubspot_call_id.clone(),
            call_ms: apo_link::call_ms(&row.call_at)?,
            scheduled: outcome == Some(SCHEDULED),
            has_hearing: items_have_hearing(items, checks),
            zoom_call_log_id: row.zoom_call_log_id.clone(),
            next,
        })
    }

    /// 先に確かめる順 (小さいほど先)。`hearing_priority` と同じ答え
    pub fn priority(&self) -> u8 {
        priority_of(self.scheduled, self.has_hearing)
    }
}

fn priority_of(scheduled: bool, has_hearing: bool) -> u8 {
    if scheduled || has_hearing {
        0
    } else {
        1
    }
}

/// ヒアリングの周回で先に確かめる順 (小さいほど先)。今の判定が「日程が決まった」行と、
/// ヒアリングが付いている行は、付ける・外す見込みが高い。それ以外は、ほとんどがアポにもならない行
pub fn hearing_priority(row: &ReviewRow, checks: &Checks) -> u8 {
    priority_of(
        row.outcome.as_deref() == Some(SCHEDULED),
        has_hearing(row, checks),
    )
}

/// 確かめる時刻が来た行から、付ける・外す見込みの高い行を先に、その中では新しい電話から `limit` 本選ぶ
/// (同じなら ID の順)。並べるのは参照だけで、写すのは選んだ分だけ (起動直後は数千本が期限に来ている)
fn select_due(
    check: &HashMap<String, HearingCandidate>,
    now: DateTime<Utc>,
    limit: usize,
) -> Vec<HearingCandidate> {
    let key = |c: &HearingCandidate| (c.priority(), std::cmp::Reverse(c.call_ms));
    let mut due: Vec<&HearingCandidate> = check.values().filter(|c| c.next <= now).collect();
    due.sort_by(|a, b| key(a).cmp(&key(b)).then_with(|| a.id.cmp(&b.id)));
    due.into_iter().take(limit).cloned().collect()
}

/// 起動時の判定済みの行の読み取りの結果
#[derive(Debug, Default)]
pub struct JudgedScan {
    /// 日本語以外が混ざる行 (判定し直す)。全部の欄を持つ。新しい電話から
    pub rejudge: VecDeque<ReviewRow>,
    /// ヒアリングを確かめる行 (最初からすぐ確かめる)
    pub hearing: HashMap<String, HearingCandidate>,
    /// 読んだ判定済みの行の数 (電話の時刻が読めず確かめられない行も数える)
    pub read: usize,
}

impl JudgedScan {
    /// 1 ページの行を振り分ける。日本語以外が混ざる行 (少数) だけ全部の欄のまま持ち、残りは軽い型にして行は捨てる
    fn add_page(&mut self, rows: Vec<ReviewRow>, checks: &Checks, now: DateTime<Utc>) {
        self.read += rows.len();
        for r in rows {
            if row_has_foreign_text(&r) {
                self.rejudge.push_back(r);
            } else if let Some(c) = HearingCandidate::from_row(&r, checks, now) {
                self.hearing.insert(c.id.clone(), c);
            }
        }
    }
}

/// 起動時に 1 回: 判定済みの行 (`call_at >= since`) を `page` 行ずつ読んで振り分ける。
/// 全部の行を 1 回で全部の欄のまま読むと、一時的にも持ち続けても数百 MB になる (`store::ROWS_PAGE`)
pub fn scan_judged(
    t: &TursoDb,
    since: &str,
    checks: &Checks,
    now: DateTime<Utc>,
    page: usize,
) -> Result<JudgedScan, String> {
    let mut scan = JudgedScan::default();
    store::for_each_judged_page(t, since, page, |rows| scan.add_page(rows, checks, now))?;
    Ok(scan)
}

/// 1 周に HubSpot と突き合わせる行の上限
const HEARING_CHECK_BATCH: usize = 300;

/// アポの記録をまだ待つ行を確かめ直す間隔
fn hearing_recheck() -> chrono::Duration {
    chrono::Duration::hours(1)
}

/// 保存済みの項目にヒアリングの項目を足し、checks の順に並べる (checks に無い古い項目は後ろに残す)
pub fn merge_hearing(
    checks: &Checks,
    items: &[StoredItem],
    hearing: Vec<StoredItem>,
) -> Vec<StoredItem> {
    let mut out: Vec<StoredItem> = checks
        .checks
        .iter()
        .filter_map(|c| {
            hearing
                .iter()
                .chain(items.iter())
                .find(|i| i.id == c.id)
                .cloned()
        })
        .collect();
    out.extend(
        items
            .iter()
            .filter(|i| checks.checks.iter().all(|c| c.id != i.id))
            .cloned(),
    );
    out
}

/// 1 周に判定し直す行の上限 (同時に判定する本数の 2 倍。既定 8 本)
fn rejudge_limit(concurrency: usize) -> usize {
    (concurrency * 2).max(1)
}

impl<Z: TranscriptSource + 'static, J: Judge + 'static> Worker<Z, J> {
    /// 起動後に 1 回: 判定済みの行から、文章に日本語以外が混ざったものを集め、残りはヒアリングを確かめる一覧に入れる
    /// (`store::ROWS_PAGE` 行ずつの SELECT。日本語以外が混ざる行だけ全部の欄を持ち、残りは軽い型)
    async fn find_rejudge(&mut self) {
        let since = store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
        let now = Utc::now();
        match store::blocking(&self.audit, move |t| {
            scan_judged(t, &since, &loaded().checks, now, store::ROWS_PAGE)
        })
        .await
        {
            Ok(scan) => {
                self.rejudge = scan.rejudge;
                // 判定し直す行は、判定し直した後に `judged_rows` から入る
                self.hearing_check = scan.hearing;
                tracing::info!(
                    read = scan.read,
                    rows = self.rejudge.len(),
                    hearing = self.hearing_check.len(),
                    "talk review: rows to rejudge"
                );
            }
            Err(e) => tracing::warn!(error = %e, "talk review: rejudge scan failed"),
        }
    }

    /// 文章が不合格の行を、1 周に上限本数だけ判定し直す。録音が消えた行は文章だけ取り除く。
    /// 通信などの一時的な失敗は、その行を見送る (次の起動でまた探す)
    async fn process_rejudge(&mut self) {
        let n = rejudge_limit(self.cfg.concurrency).min(self.rejudge.len());
        if n == 0 {
            return;
        }
        let sem = Arc::new(tokio::sync::Semaphore::new(self.cfg.concurrency));
        let mut set = tokio::task::JoinSet::new();
        for row in self.rejudge.drain(..n) {
            let (zoom, judge, audit, model, sem, pending, judged) = (
                self.zoom.clone(),
                self.judge.clone(),
                self.audit.clone(),
                self.cfg.model.clone(),
                sem.clone(),
                self.pending.clone(),
                self.judged_rows.clone(),
            );
            set.spawn(async move {
                let Ok(_p) = sem.acquire().await else { return };
                match process_one(&*zoom, &*judge, &row, Utc::now()).await {
                    Outcome::Judged(o) => {
                        if let Some(c) =
                            HearingCandidate::from_judged(&row, &o, &loaded().checks, Utc::now())
                        {
                            judged.lock().unwrap().push(c);
                        }
                        apply(
                            &audit,
                            &pending,
                            row.hubspot_call_id.clone(),
                            Update::Judged(o, row.attempts),
                            model,
                        )
                        .await;
                    }
                    Outcome::Unavailable(_) => {
                        let (mut items, mut summary) = (row.items.clone(), row.summary.clone());
                        sanitize_texts(&mut items, &mut summary, &source_text(&[], &row.items));
                        let (id, now) = (row.hubspot_call_id.clone(), store::ts_text(Utc::now()));
                        if let Err(e) = store::blocking(&audit, move |t| {
                            store::mark_text_cleaned(t, &id, &items, summary.as_deref(), &now)
                        })
                        .await
                        {
                            tracing::warn!(error = %e, "talk review: text cleanup failed");
                        }
                    }
                    Outcome::Retry(note) => {
                        tracing::info!(note, "talk review: rejudge skipped");
                    }
                }
            });
        }
        while set.join_next().await.is_some() {}
    }

    /// ヒアリングの周回: 決めていない行を HubSpot のアポの記録と突き合わせ、記録のある行にヒアリングだけを付ける
    /// (1 周に MiniMax は上限本数まで)。記録が無いまま待つ期間が過ぎた行は、付いていたヒアリングを対象外にする。
    /// 今の判定 (9 項目・outcome) は変えない。録音が取れない行はあきらめ、一時的な失敗は後で確かめ直す。
    /// 付ける・外すときは、書く直前に行を読み直し、今の項目に足す (`rewrite_hearing`)
    async fn process_hearing(&mut self) {
        let now = Utc::now();
        self.take_judged(now);
        // 付ける・外す見込みの高い行を先に、新しい電話から
        let due = select_due(&self.hearing_check, now, HEARING_CHECK_BATCH);
        if due.is_empty() {
            return;
        }
        let calls: Vec<(String, i64)> = due.iter().map(|c| (c.id.clone(), c.call_ms)).collect();
        let matched = match apo_link::matched_calls(&self.client.background(), &calls).await {
            Ok(m) => m,
            Err(e) => {
                tracing::warn!(error = %e.error_kind(), "talk review: apo link read failed");
                for c in &due {
                    self.recheck_later(&c.id, now);
                }
                return;
            }
        };
        self.act_on_hearing(due, &matched, now).await;
    }

    /// この起動で判定した行を、確かめる一覧に移す (すぐ確かめる)
    fn take_judged(&mut self, now: DateTime<Utc>) {
        let fresh = std::mem::take(&mut *self.judged_rows.lock().unwrap());
        for mut c in fresh {
            c.next = now;
            self.hearing_check.insert(c.id.clone(), c);
        }
    }

    /// 確かめ直す時刻を先に延ばす (アポの記録を待つ間・一時的な失敗のとき)
    fn recheck_later(&mut self, id: &str, now: DateTime<Utc>) {
        if let Some(c) = self.hearing_check.get_mut(id) {
            c.next = now + hearing_recheck();
        }
    }

    /// 突き合わせの結果 (`matched` = アポの記録のある電話の ID) で、確かめた行にすることを行う
    async fn act_on_hearing(
        &mut self,
        due: Vec<HearingCandidate>,
        matched: &HashSet<String>,
        now: DateTime<Utc>,
    ) {
        let checks = &loaded().checks;
        let limit = rejudge_limit(self.cfg.concurrency);
        let mut run = Vec::new();
        for c in due {
            let action = hearing_action(
                c.has_hearing,
                matched.contains(&c.id),
                apo_link::waited_enough(c.call_ms, now.timestamp_millis()),
            );
            match action {
                HearingAction::Done => {
                    self.hearing_check.remove(&c.id);
                }
                HearingAction::Later => self.recheck_later(&c.id, now),
                HearingAction::Strip => {
                    let na = hearing_na(checks, NO_APO_NOTE);
                    match rewrite_hearing(&self.audit, &c.id, na, false).await {
                        HearingWrite::ReadFailed => self.recheck_later(&c.id, now),
                        HearingWrite::Written | HearingWrite::Skipped => {
                            self.hearing_check.remove(&c.id);
                        }
                    }
                }
                // 1 周の上限を超えた分は次の周回 (時刻はそのまま)
                HearingAction::Run if run.len() < limit => run.push(c),
                HearingAction::Run => {}
            }
        }
        let sem = Arc::new(tokio::sync::Semaphore::new(self.cfg.concurrency));
        let mut set = tokio::task::JoinSet::new();
        for c in run {
            let (zoom, judge, audit, sem) = (
                self.zoom.clone(),
                self.judge.clone(),
                self.audit.clone(),
                sem.clone(),
            );
            set.spawn(async move {
                let id = c.id.clone();
                let Ok(_p) = sem.acquire().await else {
                    return (id, false);
                };
                let Some(zid) = c.zoom_call_log_id.as_deref() else {
                    return (id, true);
                };
                let turns = match zoom.fetch(zid).await {
                    Ok(t) if has_both_sides(&t) => t,
                    Ok(_) | Err(ZoomError::Gone) | Err(ZoomError::Untranscribable) => {
                        tracing::info!("talk review: hearing skipped (transcript)");
                        return (id, true);
                    }
                    Err(_) => return (id, false),
                };
                let checks = &loaded().checks;
                let hearing = match judge.hearing(checks, &turns).await {
                    Ok(h) => h,
                    Err(_) => {
                        tracing::info!("talk review: hearing skipped (judge)");
                        return (id, false);
                    }
                };
                // 文字起こしと判定に時間がかかる間に行が変わっていることがあるので、書く直前に読み直す
                let wrote = rewrite_hearing(&audit, &id, hearing, true).await;
                (id, wrote != HearingWrite::ReadFailed)
            });
        }
        while let Some(r) = set.join_next().await {
            let Ok((id, finished)) = r else { continue };
            if finished {
                self.hearing_check.remove(&id);
            } else {
                self.recheck_later(&id, now);
            }
        }
    }

    async fn discover(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let bg = self.client.background();
        let list = crate::crm::owners::fetch_all(&bg)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        let members = team_members(&list.owners, &self.cfg.teams);
        let email_owner = super::live_dials::owner_by_email(&list.owners);
        let ids: Vec<String> = {
            let mut v: Vec<String> = members.iter().map(|m| m.id.clone()).collect();
            v.sort();
            v
        };
        let of = |bpo: bool| -> Vec<String> {
            members
                .iter()
                .filter(|m| TalkReviewConfig::is_bpo_team(&m.team) == bpo)
                .map(|m| m.id.clone())
                .collect()
        };
        self.team = Some(TeamOwners {
            bpo: of(true),
            sales: of(false),
        });
        if ids.is_empty() {
            return Ok(());
        }
        let today = jst_day(now).0;
        let backfill_start = today - chrono::Duration::days(self.cfg.backfill_days);
        let backfill_from = jst_midnight_ms(backfill_start);
        let min_ms = MIN_CONNECTED_SEC * 1000;
        match self.last_poll {
            None => {
                // 初回: 日ごとに読み、1 日済むたびに進み具合を覚える (途中で止まっても続きから)
                let anchor = *self.backfill_anchor.get_or_insert(now);
                let mut day = self.backfill_next.unwrap_or(backfill_start);
                while day <= today {
                    let (a, b) = (jst_midnight_ms(day), jst_midnight_ms(day) + 86_400_000);
                    let cands = search_candidates(&bg, "hs_timestamp", a, b, None, &ids, min_ms)
                        .await
                        .map_err(|e| e.error_kind().to_string())?;
                    self.ingest(cands, now, &email_owner).await?;
                    day += chrono::Duration::days(1);
                    self.backfill_next = Some(day);
                }
                self.last_poll = Some(anchor);
            }
            Some(prev) => {
                let to = (now + chrono::Duration::hours(1)).timestamp_millis();
                let from = poll_from(prev, now, backfill_from);
                let cands = search_candidates(
                    &bg,
                    "hs_lastmodifieddate",
                    from,
                    to,
                    Some(backfill_from),
                    &ids,
                    min_ms,
                )
                .await
                .map_err(|e| e.error_kind().to_string())?;
                self.ingest(cands, now, &email_owner).await?;
                self.last_poll = Some(now);
            }
        }
        Ok(())
    }

    /// 新しい Call のうち、接触した電話だけを順番待ちの行にする。担当者は Zoom でかけた人 (`attribute`)。
    /// 接触の判定がまだの Call はメモリで待つ (行は作らない)。接触していない Call は何も書かない。
    /// Zoom が読めなければ `Err` で止め、呼び出し側は同じ範囲を次の周回で読み直す
    async fn ingest(
        &mut self,
        cands: Vec<Candidate>,
        now: DateTime<Utc>,
        email_owner: &HashMap<String, String>,
    ) -> Result<(), String> {
        let mut pool: Vec<Waiting> = std::mem::take(&mut self.waiting).into_values().collect();
        let mut in_pool: HashSet<String> = pool.iter().map(|w| w.cand.id.clone()).collect();
        let mut moved = 0;
        for c in cands {
            if self.known.contains(&c.id) || self.skipped.contains(&c.id) || in_pool.contains(&c.id)
            {
                continue;
            }
            let Some(at) = DateTime::<Utc>::from_timestamp_millis(c.ts_ms) else {
                continue;
            };
            // 録音 URL は通話のあとで本文に入る。まだなら次の周回で見直す。
            // 猶予を過ぎても無いものは、時刻で接触の印と結び、判定のときに Zoom の通話履歴から録音を探す
            if c.zoom_id.is_none() && now - at < URL_GRACE {
                continue;
            }
            let owner = match caller_owner(
                &*self.zoom,
                &c.owner_id,
                c.to_number.as_deref(),
                c.ts_ms,
                email_owner,
            )
            .await
            {
                Ok(o) => o,
                Err(e) => {
                    self.waiting = pool.into_iter().map(|w| (w.cand.id.clone(), w)).collect();
                    return Err(e);
                }
            };
            if owner != c.owner_id {
                moved += 1;
            }
            in_pool.insert(c.id.clone());
            pool.push(Waiting { cand: c, owner });
        }
        if pool.is_empty() {
            return Ok(());
        }
        // 接触の印を読む (SELECT のみ)。結び付けは担当者と時刻なので (`target::verdict`)、録音 URL の有無にかかわらず窓で読む
        let windows = mark_windows(&pool);
        let marks = match store::blocking(&self.audit, move |t| {
            super::contact_store::marks_for(t, &[], &windows)
        })
        .await
        {
            Ok(m) => m,
            Err(e) => {
                self.waiting = pool.into_iter().map(|w| (w.cand.id.clone(), w)).collect();
                return Err(format!("contact marks read: {e}"));
            }
        };
        let settled = settle(pool, &marks, &mut self.used_marks, now);
        self.skipped.extend(settled.skipped);
        let mut fresh = 0;
        let mut retry: Vec<Waiting> = settled.waiting;
        for w in settled.targets {
            let (c, owner) = (&w.cand, w.owner.clone());
            let Some(at) = DateTime::<Utc>::from_timestamp_millis(c.ts_ms) else {
                continue;
            };
            let ts = store::ts_text(now);
            let row = NewRow {
                hubspot_call_id: c.id.clone(),
                owner_id: owner,
                call_at: store::ts_text(at),
                duration_sec: c.duration_ms / 1000,
                zoom_call_log_id: c.zoom_id.clone(),
                status: ST_QUEUED,
                note: Some(CONTACTED_NOTE.to_string()),
                next_retry_at: None,
            };
            match store::blocking(&self.audit, move |t| store::insert_new(t, &ts, &row)).await {
                Ok(()) => {
                    self.known.insert(c.id.clone());
                    fresh += 1;
                }
                Err(e) => {
                    tracing::warn!(error = %e, "talk review: row insert failed");
                    retry.push(w);
                }
            }
        }
        let waiting_n = retry.len();
        self.waiting = retry.into_iter().map(|w| (w.cand.id.clone(), w)).collect();
        tracing::info!(
            new_rows = fresh,
            waiting = waiting_n,
            caller_differs = moved,
            "talk review ingest done"
        );
        Ok(())
    }

    /// 起動後に 1 回: この変更より前に取り込んだ順番待ちの行のうち、接触した電話に印を付ける。
    /// 印の無い行は枠に入らない。書き込みは接触した行の分だけ (200 行ずつ 1 文)
    async fn adopt_backlog(&self) {
        let since = store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
        let to = store::ts_text(Utc::now() + chrono::Duration::days(1));
        let res = store::blocking(&self.audit, move |t| {
            let rows = store::queued_unmarked(t, &since)?;
            if rows.is_empty() {
                return Ok(0);
            }
            let marks = super::contact_store::contacted_marks(t, &since, &to)?;
            let ids: Vec<String> = target::contacted_rows(rows, &marks)
                .into_iter()
                .map(|r| r.hubspot_call_id)
                .collect();
            store::mark_contacted(t, &ids, &store::ts_text(Utc::now()))?;
            Ok(ids.len())
        })
        .await;
        match res {
            Ok(n) => tracing::info!(rows = n, "talk review: backlog rows adopted as contacted"),
            Err(e) => tracing::warn!(error = %e, "talk review: backlog adopt failed"),
        }
    }

    /// 起動後に 1 回: 取り込み済みの行の担当者を、Zoom でかけた人に直す (この変更より前に入った行)
    async fn reattribute(&self) -> Result<(), String> {
        let bg = self.client.background();
        let list = crate::crm::owners::fetch_all(&bg)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        let email_owner = super::live_dials::owner_by_email(&list.owners);
        let since = store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
        let to = store::ts_text(Utc::now() + chrono::Duration::days(1));
        // 要るのは ID・担当者・時刻だけ。引用などを持たない軽い読み方で、ページに分けて読む
        let rows = store::blocking(&self.audit, move |t| {
            store::rows_light_in(t, &since, &to, None, false)
        })
        .await?;
        let ids: Vec<String> = rows.iter().map(|r| r.hubspot_call_id.clone()).collect();
        let numbers: HashMap<String, String> = bg
            .batch_read("calls", &ids, &["hs_call_to_number"])
            .await
            .map_err(|e| e.error_kind().to_string())?
            .into_iter()
            .filter_map(|r| match r.properties.get("hs_call_to_number") {
                Some(Some(n)) => Some((r.id, n.clone())),
                _ => None,
            })
            .collect();
        let mut moved = 0;
        for r in rows {
            let Ok(at) = DateTime::parse_from_rfc3339(&r.call_at) else {
                continue;
            };
            let owner = caller_owner(
                &*self.zoom,
                &r.owner_id,
                numbers.get(&r.hubspot_call_id).map(String::as_str),
                at.timestamp_millis(),
                &email_owner,
            )
            .await?;
            if owner == r.owner_id {
                continue;
            }
            let (id, now) = (r.hubspot_call_id.clone(), store::ts_text(Utc::now()));
            store::blocking(&self.audit, move |t| store::set_owner(t, &id, &owner, &now)).await?;
            moved += 1;
        }
        tracing::info!(moved, "talk review: rows reattributed to the Zoom caller");
        Ok(())
    }

    /// 順番待ちの行を、1 人 1 日の枠まで文字起こし待ちにする
    async fn promote(&self) {
        let since = store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
        let (now, n) = (store::ts_text(Utc::now()), self.cfg.per_person_day);
        if let Err(e) =
            store::blocking(&self.audit, move |t| store::promote(t, &since, n, &now)).await
        {
            tracing::warn!(error = %e, "talk review: promote failed");
        }
    }

    /// アポの日ごとの数えを、新しい日から前もって進める (画面はこれを読む)。前の周回が続いていれば足さない
    fn prefill_counts(&mut self, now: DateTime<Utc>) {
        if self.counts_task.as_ref().is_some_and(|t| !t.is_finished()) {
            return;
        }
        let Some(team) = self.team.clone() else {
            return;
        };
        let today = jst_day(now).0;
        let from = today - chrono::Duration::days(self.cfg.backfill_days);
        let (client, st) = (self.client.clone(), super::routes::shared());
        self.counts_task = Some(tokio::spawn(async move {
            if let Err(e) = st.counts.fill(&client, &team, from, today, now, true).await {
                tracing::warn!(error = %e.error_kind(), "talk review: counts prefill failed");
            }
        }));
    }

    /// 判定待ちを新しい電話から進める。新しく入った電話を拾い直せるよう、1 回に使うのは `interval` まで。
    /// 時間切れで残りがあれば true (呼び出し側は待たずに次の周回へ)
    async fn process_due(&mut self) -> bool {
        let started = std::time::Instant::now();
        let mut done: HashSet<String> = HashSet::new();
        // 1 回に取る本数は同時に判定する本数の 8 倍 (遅い 1 本を待つ間も残りの枠で進める)
        let batch_size = (self.cfg.concurrency * 8).max(40);
        for _ in 0..(MAX_PER_CYCLE / batch_size).max(1) {
            if started.elapsed() >= self.cfg.interval {
                return true;
            }
            let now_s = store::ts_text(Utc::now());
            let due = match store::blocking(&self.audit, move |t| {
                store::due(t, &now_s, batch_size as i64)
            })
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "talk review: due select failed");
                    return false;
                }
            };
            let mut batch: Vec<ReviewRow> = due
                .into_iter()
                .filter(|r| {
                    !self.pending.contains(&r.hubspot_call_id)
                        && done.insert(r.hubspot_call_id.clone())
                })
                .collect();
            if batch.is_empty() {
                return false;
            }
            // 録音 URL が Call に無い行は、相手番号を HubSpot から 1 回でまとめて読む (読めなければ今回は見送る)
            let need: Vec<String> = batch
                .iter()
                .filter(|r| r.zoom_call_log_id.is_none())
                .map(|r| r.hubspot_call_id.clone())
                .collect();
            let mut numbers: HashMap<String, String> = HashMap::new();
            if !need.is_empty() {
                match self
                    .client
                    .background()
                    .batch_read("calls", &need, &["hs_call_to_number"])
                    .await
                {
                    Ok(recs) => {
                        for r in recs {
                            if let Some(Some(n)) = r.properties.get("hs_call_to_number") {
                                numbers.insert(r.id, n.clone());
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e.error_kind(), "talk review: number read failed");
                        batch.retain(|r| r.zoom_call_log_id.is_some());
                    }
                }
            }
            let sem = Arc::new(tokio::sync::Semaphore::new(self.cfg.concurrency));
            let mut set = tokio::task::JoinSet::new();
            for mut row in batch {
                let number = numbers.remove(&row.hubspot_call_id);
                let (zoom, judge, audit, model, sem, pending, judged) = (
                    self.zoom.clone(),
                    self.judge.clone(),
                    self.audit.clone(),
                    self.cfg.model.clone(),
                    sem.clone(),
                    self.pending.clone(),
                    self.judged_rows.clone(),
                );
                set.spawn(async move {
                    let Ok(_p) = sem.acquire().await else { return };
                    let now = Utc::now();
                    let outcome = match resolve_recording(&*zoom, &row, number.as_deref()).await {
                        Resolve::Done(o) => o,
                        found => {
                            if let Resolve::Found(zid) = found {
                                let (id, z, ts) = (
                                    row.hubspot_call_id.clone(),
                                    zid.clone(),
                                    store::ts_text(now),
                                );
                                if let Err(e) = store::blocking(&audit, move |t| {
                                    store::set_zoom_id(t, &id, &z, &ts)
                                })
                                .await
                                {
                                    tracing::warn!(error = %e, "talk review: zoom id write failed");
                                }
                                row.zoom_call_log_id = Some(zid);
                            }
                            process_one(&*zoom, &*judge, &row, now).await
                        }
                    };
                    let upd = decide(outcome, row.attempts, now);
                    if let Update::Judged(o, _) = &upd {
                        if let Some(c) =
                            HearingCandidate::from_judged(&row, o, &loaded().checks, Utc::now())
                        {
                            judged.lock().unwrap().push(c);
                        }
                    }
                    apply(&audit, &pending, row.hubspot_call_id.clone(), upd, model).await;
                });
            }
            while set.join_next().await.is_some() {}
        }
        true
    }

    async fn run(mut self) {
        match store::blocking(&self.audit, {
            let since =
                store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
            move |t| store::known_ids(t, &since)
        })
        .await
        {
            Ok(k) => self.known = k,
            Err(e) => {
                tracing::warn!(error = %e, "talk review: cannot read known ids; worker stops");
                return;
            }
        }
        // 起動後に 1 回: 録音 URL が無いとして取れなかった古い行を、通話履歴から探して試し直す
        {
            let since =
                store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
            let now = store::ts_text(Utc::now());
            let n = self.cfg.per_person_day;
            if let Err(e) = store::blocking(&self.audit, move |t| {
                store::requeue_no_recording(t, &since, &now)?;
                store::demote_over_quota(t, &since, n, &now)
            })
            .await
            {
                tracing::warn!(error = %e, "talk review: requeue failed");
            }
        }
        if let Err(e) = self.reattribute().await {
            tracing::warn!(error = %e, "talk review: reattribute failed");
        }
        self.adopt_backlog().await;
        self.find_rejudge().await;
        loop {
            let now = Utc::now();
            if let Err(e) = self.discover(now).await {
                tracing::warn!(error = %e, "talk review discover failed");
            }
            self.prefill_counts(now);
            self.promote().await;
            self.pending.flush(&self.audit).await;
            // 時間切れで判定待ちが残っていれば待たずに次の周回 (新しい電話を拾い直してから続ける)
            let busy = self.process_due().await;
            // HubSpot のアポの記録と突き合わせて、ヒアリングを付ける・外す (毎周。日中は判定待ちが続き、
            // 手の空いた周回が少ないため)
            self.process_hearing().await;
            if !busy {
                // 新しい判定が無い周回に、文章が不合格の行を少しずつ判定し直す
                self.process_rejudge().await;
                tokio::time::sleep(self.cfg.interval).await;
            }
        }
    }
}

/// 起動。`CALL_REVIEW_ENABLED=1`・HubSpot・監査 DB・Zoom・MiniMax の設定がそろっていなければ何もしない
pub fn spawn_worker(state: Arc<AppState>) {
    let cfg = TalkReviewConfig::from_env();
    if !cfg.enabled {
        return;
    }
    let (Some(audit), Some(client)) = (state.audit.clone(), state.crm_hubspot()) else {
        tracing::warn!("talk review: HubSpot または監査 DB が未接続のため起動しません");
        return;
    };
    let Some(zoom) = ZoomClient::from_env() else {
        tracing::warn!("talk review: ZOOM_ACCOUNT_ID / ZOOM_CLIENT_ID / ZOOM_CLIENT_SECRET が未設定のため起動しません");
        return;
    };
    let Some(judge) = MiniMaxJudge::new(&cfg.minimax_base_url, &cfg.model) else {
        tracing::warn!("talk review: MINIMAX_API_KEY が未設定のため起動しません");
        return;
    };
    tracing::info!(model = %cfg.model, teams = ?cfg.teams, "talk review worker started");
    let judge = Arc::new(judge);
    super::contact_worker::spawn(
        audit.clone(),
        client.clone(),
        zoom.clone(),
        judge.clone(),
        cfg.clone(),
    );
    let w = Worker {
        audit,
        client,
        zoom,
        judge,
        cfg,
        known: HashSet::new(),
        waiting: HashMap::new(),
        skipped: HashSet::new(),
        used_marks: HashSet::new(),
        team: None,
        counts_task: None,
        last_poll: None,
        backfill_next: None,
        backfill_anchor: None,
        pending: Arc::new(PendingWrites::default()),
        rejudge: Default::default(),
        hearing_check: Default::default(),
        judged_rows: Default::default(),
    };
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        w.run().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::judge::StoredItem;
    use crate::crm::talk_review::store::ST_PENDING;
    use crate::crm::talk_review::types::TalkTurn;

    struct FakeZoom(Result<Vec<TalkTurn>, ZoomError>);
    impl TranscriptSource for FakeZoom {
        async fn fetch(&self, _: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.0.clone()
        }
        async fn find_recording(&self, number: &str, _: i64) -> Result<Option<String>, ZoomError> {
            match number {
                "03-1234-5678" => Ok(Some("REC1".into())),
                "err" => Err(ZoomError::Transient("x".into())),
                _ => Ok(None),
            }
        }
        async fn find_caller(&self, number: &str, _: i64) -> Result<Option<String>, ZoomError> {
            match number {
                "03-1111-2222" => Ok(Some("Kubo@X".into())),
                "03-3333-4444" => Ok(Some("outsider@x".into())),
                "err" => Err(ZoomError::Transient("x".into())),
                "gone" => Err(ZoomError::Gone),
                _ => Ok(None),
            }
        }
    }
    struct FakeJudge(Result<JudgeOutput, JudgeError>);
    impl Judge for FakeJudge {
        async fn judge(
            &self,
            _: &super::super::checks::Checks,
            _: &[TalkTurn],
        ) -> Result<JudgeOutput, JudgeError> {
            self.0.clone()
        }
        async fn hearing(
            &self,
            _: &super::super::checks::Checks,
            _: &[TalkTurn],
        ) -> Result<Vec<StoredItem>, JudgeError> {
            Err(JudgeError::NotConfigured)
        }
    }

    fn out() -> JudgeOutput {
        JudgeOutput {
            reached_person: Some(true),
            reception: None,
            first_date_turn: None,
            outcome: None,
            items: vec![],
            summary: None,
        }
    }
    fn turns() -> Vec<TalkTurn> {
        vec![
            TalkTurn {
                n: 1,
                speaker: "rep".into(),
                text: "a".into(),
            },
            TalkTurn {
                n: 2,
                speaker: "customer".into(),
                text: "b".into(),
            },
        ]
    }
    fn row(zid: Option<&str>, call_at: &str) -> ReviewRow {
        ReviewRow {
            hubspot_call_id: "1".into(),
            owner_id: "o".into(),
            call_at: call_at.into(),
            duration_sec: 90,
            zoom_call_log_id: zid.map(String::from),
            status: ST_PENDING.into(),
            attempts: 0,
            checks_version: None,
            model: None,
            outcome: None,
            reached_person: None,
            reception: None,
            first_date_turn: None,
            items: vec![],
            summary: None,
            note: None,
        }
    }
    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-10-15T12:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    #[test]
    fn hearing_is_decided_by_the_apo_record() {
        use HearingAction::*;
        // (ヒアリングが付いているか, アポの記録があるか, 待つ期間が過ぎたか)
        assert_eq!(
            hearing_action(false, true, false),
            Run,
            "記録があれば待たずに付ける"
        );
        assert_eq!(hearing_action(false, true, true), Run);
        assert_eq!(
            hearing_action(true, true, false),
            Done,
            "付いていればそのまま"
        );
        assert_eq!(
            hearing_action(true, false, false),
            Later,
            "記録を待つ間は付いていても外さない"
        );
        assert_eq!(hearing_action(false, false, false), Later);
        assert_eq!(
            hearing_action(true, false, true),
            Strip,
            "待っても記録が無ければ外す"
        );
        assert_eq!(hearing_action(false, false, true), Done);
    }

    #[test]
    fn scheduled_or_heard_rows_are_checked_first() {
        let c = &loaded().checks;
        let mut r = row(Some("z"), "2026-10-27T00:00:00Z");
        r.outcome = Some("決まらなかった".into());
        assert_eq!(hearing_priority(&r, c), 1);
        r.outcome = Some("日程が決まった".into());
        assert_eq!(hearing_priority(&r, c), 0);
        r.outcome = Some("決まらなかった".into());
        r.items = hearing_na(c, NO_APO_NOTE);
        r.items[0].status = "not_done".into();
        assert_eq!(hearing_priority(&r, c), 0, "ヒアリングが付いている行も先");
    }

    #[test]
    fn hearing_items_are_added_or_set_aside_without_touching_the_main_items() {
        let c = &loaded().checks;
        let main = |id: &str, st: &str| StoredItem {
            id: id.into(),
            status: st.into(),
            evidence: vec![],
            quote: None,
            advice: None,
            note: None,
            answer: None,
            asked: None,
            answered: None,
        };
        let mut r = row(Some("z"), "2026-10-27T00:00:00Z");
        r.items = vec![
            main("early_date", "done"),
            main("old_removed_check", "done"),
        ];
        assert!(!has_hearing(&r, c), "ヒアリングの項目が無い行");
        let hearing: Vec<StoredItem> = c
            .checks
            .iter()
            .filter(|c| is_hearing(c))
            .map(|c| main(&c.id, "not_done"))
            .collect();
        let merged = merge_hearing(c, &r.items, hearing);
        let ids: Vec<&str> = merged.iter().map(|i| i.id.as_str()).collect();
        let order: Vec<&str> = c
            .checks
            .iter()
            .map(|c| c.id.as_str())
            .filter(|id| ids.contains(id))
            .collect();
        assert_eq!(&ids[..order.len()], &order[..], "checks の順");
        assert_eq!(
            ids.last(),
            Some(&"old_removed_check"),
            "checks に無い古い項目は後ろに残す"
        );
        assert_eq!(
            merged.iter().find(|i| i.id == "early_date").unwrap().status,
            "done",
            "今の判定は変えない"
        );
        r.items = merged;
        assert!(has_hearing(&r, c), "付けた後");
        // 記録が無ければ、付いていたヒアリングを該当なし (メモ付き) にする
        r.items = merge_hearing(c, &r.items, hearing_na(c, NO_APO_NOTE));
        assert!(!has_hearing(&r, c));
        assert!(r
            .items
            .iter()
            .filter(|i| i.id.starts_with("aq_"))
            .all(|i| i.status == "na" && i.note.as_deref() == Some(NO_APO_NOTE)));
        assert_eq!(
            r.items
                .iter()
                .find(|i| i.id == "early_date")
                .unwrap()
                .status,
            "done"
        );
    }

    #[test]
    fn marks_are_read_for_calls_with_a_recording_url_too() {
        let pool = vec![
            waiting("c1", "o1", "2026-10-28T01:00:00Z", Some("20261028-aaaa")),
            waiting("c2", "o2", "2026-10-28T02:00:00Z", None),
        ];
        let w = mark_windows(&pool);
        assert_eq!(
            w.len(),
            2,
            "録音 URL のある Call (新規営業のほとんど) も窓で読む"
        );
        assert_eq!(
            w[0],
            (
                "o1".to_string(),
                "2026-10-28T00:57:00Z".to_string(),
                "2026-10-28T01:03:00Z".to_string()
            )
        );
        // 読んだ印は担当者と時刻で結び付く (URL の ID は使わない)
        let marks = vec![mk(
            "zoomlog1",
            "o1",
            "2026-10-28T01:00:40Z",
            "judged",
            Some(true),
        )];
        let s = settle(pool, &marks, &mut HashSet::new(), Utc::now());
        assert_eq!(
            s.targets
                .iter()
                .map(|w| w.cand.id.as_str())
                .collect::<Vec<_>>(),
            vec!["c1"]
        );
    }

    #[test]
    fn rows_with_foreign_advice_are_found_for_rejudge() {
        let item = |advice: &str, quote: &str| StoredItem {
            id: "early_date".into(),
            status: "not_done".into(),
            evidence: vec![],
            quote: Some(quote.into()),
            advice: Some(advice.into()),
            note: None,
            answer: None,
            asked: None,
            answered: None,
        };
        let mut r = row(Some("z"), "2026-10-15T11:00:00Z");
        r.items = vec![item("先に用件を伝える", "来週は")];
        assert!(!row_has_foreign_text(&r));
        r.items = vec![item("这个问题", "来週は")];
        assert!(row_has_foreign_text(&r));
        r.items = vec![item("operatorsconciled", "来週は")];
        assert!(row_has_foreign_text(&r));
        // 引用にある語は許す
        r.items = vec![item("kintone を出す", "kintone です")];
        assert!(!row_has_foreign_text(&r));
        r.items = vec![item("先に用件を伝える", "来週は")];
        r.summary = Some("먼저".into());
        assert!(row_has_foreign_text(&r));
        assert_eq!(rejudge_limit(4), 8);
    }

    #[tokio::test]
    async fn process_outcomes() {
        let r = row(Some("z"), "2026-10-15T11:00:00Z");
        let j_ok = FakeJudge(Ok(out()));
        assert_eq!(
            process_one(&FakeZoom(Ok(turns())), &j_ok, &r, now()).await,
            Outcome::Judged(out())
        );
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::NotReady)), &j_ok, &r, now()).await,
            Outcome::Retry("transcript_not_ready")
        );
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::Untranscribable)), &j_ok, &r, now()).await,
            Outcome::Unavailable("untranscribable")
        );
        // 新しい通話の 404 は録音がまだ出来ていないだけかもしれない
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::Gone)), &j_ok, &r, now()).await,
            Outcome::Retry("recording_not_found_yet")
        );
        let old = row(Some("z"), "2026-10-01T00:00:00Z");
        assert_eq!(
            process_one(&FakeZoom(Err(ZoomError::Gone)), &j_ok, &old, now()).await,
            Outcome::Unavailable("recording_gone")
        );
        // 営業しか話していない文字起こしは判定しない
        assert_eq!(
            process_one(&FakeZoom(Ok(turns()[..1].to_vec())), &j_ok, &r, now()).await,
            Outcome::Unavailable("speakers_not_separated")
        );
        assert_eq!(
            process_one(
                &FakeZoom(Ok(turns())),
                &FakeJudge(Err(JudgeError::Transient("x".into()))),
                &r,
                now()
            )
            .await,
            Outcome::Retry("judge_error")
        );
        assert_eq!(
            process_one(
                &FakeZoom(Ok(turns())),
                &j_ok,
                &row(None, "2026-10-15T11:00:00Z"),
                now()
            )
            .await,
            Outcome::Unavailable("no_recording_url")
        );
    }

    #[tokio::test]
    async fn recording_is_looked_up_only_when_the_call_has_no_url() {
        let z = FakeZoom(Ok(turns()));
        let none = row(None, "2026-10-15T01:00:00Z");
        assert_eq!(
            resolve_recording(&z, &none, Some("03-1234-5678")).await,
            Resolve::Found("rec:REC1".into())
        );
        let unavailable = Resolve::Done(Outcome::Unavailable("no_recording_found"));
        assert_eq!(
            resolve_recording(&z, &none, Some("090-0000-0000")).await,
            unavailable
        );
        assert_eq!(resolve_recording(&z, &none, None).await, unavailable);
        assert_eq!(resolve_recording(&z, &none, Some("  ")).await, unavailable);
        assert_eq!(
            resolve_recording(&z, &none, Some("err")).await,
            Resolve::Done(Outcome::Retry("zoom_error"))
        );
        // URL がある行は探さない
        let has = row(Some("z"), "2026-10-15T01:00:00Z");
        assert_eq!(
            resolve_recording(&z, &has, Some("03-1234-5678")).await,
            Resolve::Have
        );
    }

    #[test]
    fn decide_backoff_and_limits() {
        let n = now();
        match decide(Outcome::Retry("transcript_not_ready"), 0, n) {
            Update::Retry { attempts, next, .. } => {
                assert_eq!(attempts, 1);
                assert_eq!(next, n + chrono::Duration::minutes(10));
            }
            u => panic!("{u:?}"),
        }
        match decide(Outcome::Retry("zoom_error"), 2, n) {
            Update::Retry { attempts, next, .. } => {
                assert_eq!(attempts, 3);
                assert_eq!(next, n + chrono::Duration::minutes(60));
            }
            u => panic!("{u:?}"),
        }
        assert_eq!(
            decide(Outcome::Retry("transcript_not_ready"), 7, n),
            Update::Final {
                status: "unavailable",
                attempts: 8,
                note: "transcript_not_ready"
            }
        );
        assert_eq!(
            decide(Outcome::Retry("judge_error"), 7, n),
            Update::Final {
                status: "failed",
                attempts: 8,
                note: "judge_error"
            }
        );
        assert_eq!(backoff(99), chrono::Duration::minutes(720));
    }

    #[test]
    fn candidate_parsing_and_search_body() {
        let id = "20261015-0a1b2c3d-4e5f-4a6b-8c7d-9e0f1a2b3c4d";
        let v = json!({"id": "55", "properties": {
            "hubspot_owner_id": "7", "hs_timestamp": "2026-10-15T01:02:03Z", "hs_call_duration": "95000",
            "hs_call_body": format!("Recording Url: https://x/audio/play/0/{id}")}});
        let c = parse_candidate(&v).unwrap();
        assert_eq!(c.to_number, None);
        assert_eq!(
            (c.id.as_str(), c.owner_id.as_str(), c.duration_ms),
            ("55", "7", 95_000)
        );
        assert_eq!(c.zoom_id.as_deref(), Some(id));
        assert_eq!(
            c.ts_ms,
            DateTime::parse_from_rfc3339("2026-10-15T01:02:03Z")
                .unwrap()
                .timestamp_millis()
        );
        assert!(
            parse_candidate(&json!({"id": "1", "properties": {"hs_timestamp": "1"}})).is_none()
        );

        let b = search_body("hs_timestamp", 10, 20, &["7".into()], 70_000, Some("cur"));
        let f = b["filterGroups"][0]["filters"].as_array().unwrap();
        assert!(f
            .iter()
            .any(|x| x["propertyName"] == "hs_call_direction" && x["value"] == "OUTBOUND"));
        assert!(f.iter().any(|x| x["propertyName"] == "hs_call_duration"
            && x["operator"] == "GTE"
            && x["value"] == "70000"));
        assert!(f
            .iter()
            .any(|x| x["propertyName"] == "hubspot_owner_id" && x["operator"] == "IN"));
        assert_eq!(b["after"], "cur");
        // HubSpot の上限 (1 まとまり 6 条件) を超えない
        assert!(f.len() <= 6, "filters = {}", f.len());
    }

    #[test]
    fn poll_reaches_back_past_the_url_grace() {
        let t = |s: &str| DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc);
        let ms = |s: &str| t(s).timestamp_millis();
        let now = t("2026-10-26T03:00:00Z");
        // 前回が直前でも、猶予 (2 時間) + 30 分前から読む: 09:00 に作られて URL が付かない Call を 11:00 過ぎに拾う
        assert_eq!(
            poll_from(t("2026-10-26T02:59:00Z"), now, 0),
            ms("2026-10-26T00:30:00Z")
        );
        // 前回がもっと前ならそこから
        assert_eq!(
            poll_from(t("2026-10-25T20:00:00Z"), now, 0),
            ms("2026-10-25T19:30:00Z")
        );
        // 遡りの下限より前は読まない
        let floor = ms("2026-10-26T01:00:00Z");
        assert_eq!(poll_from(t("2026-10-26T02:59:00Z"), now, floor), floor);
    }

    #[tokio::test]
    async fn rows_go_to_the_zoom_caller_not_the_hubspot_owner() {
        let z = FakeZoom(Ok(vec![]));
        let m: HashMap<String, String> = [
            ("kubo@x".to_string(), "11".to_string()),
            ("aikawa@x".to_string(), "22".to_string()),
        ]
        .into();
        let who = |n: Option<&'static str>| {
            let (z, m) = (&z, &m);
            async move { caller_owner(z, "22", n, 0, m).await }
        };
        // HubSpot では相川さん (22) の名前でも、Zoom でかけたのは久保さん (11)。メールの大小文字は区別しない
        assert_eq!(who(Some("03-1111-2222")).await, Ok("11".into()));
        // かけた人が担当者に居ない・Zoom に見つからない・番号が無い・通話が消えた → HubSpot の担当者のまま
        assert_eq!(who(Some("03-3333-4444")).await, Ok("22".into()));
        assert_eq!(who(Some("03-9999-9999")).await, Ok("22".into()));
        assert_eq!(who(None).await, Ok("22".into()));
        assert_eq!(who(Some("gone")).await, Ok("22".into()));
        // Zoom が読めないときは決めずにやり直す (HubSpot の担当者で書かない)
        assert!(who(Some("err")).await.is_err());

        let v = json!({"id": "9", "properties": {"hubspot_owner_id": "22",
            "hs_timestamp": "2026-10-15T01:02:03Z", "hs_call_to_number": " +81311112222 "}});
        assert_eq!(
            parse_candidate(&v).unwrap().to_number.as_deref(),
            Some("+81311112222")
        );
        let b = search_body("hs_timestamp", 10, 20, &["7".into()], 70_000, None);
        assert!(b["properties"]
            .as_array()
            .unwrap()
            .iter()
            .any(|p| p == "hs_call_to_number"));
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn judged_result_is_kept_and_written_on_the_next_flush() {
        use crate::audit::fake_turso::start_sqlite_audit;
        let (audit, conn) = start_sqlite_audit().await;
        let ins = |id: &str| {
            conn.lock().unwrap().execute(
                "INSERT INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, status, attempts, created_at, updated_at) \
                 VALUES (?1, 'o', '2026-10-15T00:00:00Z', 90, 'pending_transcript', 0, 'x', 'x')",
                [id],
            ).unwrap();
        };
        let p = PendingWrites::default();
        // 書き込みに失敗した想定 (表に行が無い間は UPDATE が 0 件で成功してしまうので、直接 keep する)
        p.keep("c1".into(), out(), 2, "MiniMax-M3".into());
        assert!(p.contains("c1"));
        ins("c1");
        p.flush(&audit).await;
        assert!(p.is_empty());
        let (status, attempts, model): (String, i64, String) = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT status, attempts, model FROM call_talk_reviews WHERE hubspot_call_id='c1'",
                [],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        assert_eq!(
            (status.as_str(), attempts, model.as_str()),
            ("judged", 2, "MiniMax-M3")
        );
    }

    fn waiting(id: &str, owner: &str, at: &str, zid: Option<&str>) -> Waiting {
        let ts = DateTime::parse_from_rfc3339(at).unwrap().timestamp_millis();
        Waiting {
            cand: Candidate {
                id: id.into(),
                owner_id: owner.into(),
                ts_ms: ts,
                duration_ms: 40_000,
                zoom_id: zid.map(String::from),
                to_number: None,
            },
            owner: owner.into(),
        }
    }
    fn mk(id: &str, owner: &str, at: &str, status: &str, c: Option<bool>) -> MarkState {
        MarkState {
            id: id.into(),
            owner_id: owner.into(),
            at_ms: DateTime::parse_from_rfc3339(at).unwrap().timestamp_millis(),
            status: status.into(),
            contacted: c,
        }
    }

    #[test]
    fn only_contacted_calls_become_rows_and_uncontacted_ones_are_dropped() {
        let at = "2026-10-26T01:00:00Z";
        // 40 秒の電話でも接触していれば対象。90 秒でも接触していなければ対象外。判定中は待つ
        let pool = vec![
            waiting("short_contact", "o1", at, Some("Z1")),
            waiting("long_no_contact", "o1", at, Some("Z2")),
            waiting("judging", "o1", at, Some("Z3")),
            waiting("no_mark_yet", "o1", at, Some("Z4")),
            waiting("queue_call", "o2", "2026-10-26T01:00:20Z", None),
        ];
        let marks = vec![
            mk("Z1", "o1", at, "judged", Some(true)),
            mk("Z2", "o1", at, "judged", Some(false)),
            mk("Z3", "o1", at, "pending", None),
            mk("Q", "o2", "2026-10-26T01:00:00Z", "judged", Some(true)),
        ];
        let now = DateTime::parse_from_rfc3339("2026-10-26T02:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let mut used = HashSet::new();
        let s = settle(pool, &marks, &mut used, now);
        let ids = |v: &[Waiting]| {
            let mut x: Vec<String> = v.iter().map(|w| w.cand.id.clone()).collect();
            x.sort();
            x
        };
        assert_eq!(ids(&s.targets), vec!["queue_call", "short_contact"]);
        assert_eq!(s.skipped, vec!["long_no_contact".to_string()]);
        assert_eq!(ids(&s.waiting), vec!["judging", "no_mark_yet"]);
    }

    #[test]
    fn waiting_gives_up_when_no_mark_appears_in_six_hours() {
        let at = "2026-10-26T01:00:00Z";
        let pool = vec![waiting("a", "o1", at, Some("Z9"))];
        let early = DateTime::parse_from_rfc3339("2026-10-26T06:59:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let late = DateTime::parse_from_rfc3339("2026-10-26T07:01:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let s = settle(pool.clone(), &[], &mut HashSet::new(), early);
        assert_eq!((s.waiting.len(), s.skipped.len()), (1, 0));
        let s = settle(pool, &[], &mut HashSet::new(), late);
        assert_eq!((s.waiting.len(), s.skipped), (0, vec!["a".to_string()]));
    }

    #[test]
    fn members_by_team() {
        let o = |id: &str, teams: &[&str]| CrmOwner {
            id: id.into(),
            name: id.into(),
            email: None,
            archived: false,
            teams: teams.iter().map(|s| s.to_string()).collect(),
        };
        let owners = vec![
            o("1", &["BPO_リクロジ"]),
            o("2", &["新規営業", "x"]),
            o("3", &["他"]),
            o("4", &[]),
        ];
        let m = team_members(&owners, &["BPO_リクロジ".into(), "新規営業".into()]);
        assert_eq!(
            m.iter()
                .map(|x| (x.id.as_str(), x.team.as_str()))
                .collect::<Vec<_>>(),
            vec![("1", "BPO_リクロジ"), ("2", "新規営業")]
        );
    }

    // ---- ヒアリングの周回: 軽い型・ページ読み・書く直前の読み直し ------------------------------

    use crate::audit::fake_turso::{start_sqlite_audit_with_queries, SharedConn};
    use crate::hubspot::ClientOptions;

    fn ts(s: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(s).unwrap().with_timezone(&Utc)
    }

    /// ヒアリング以外の 9 項目 (checks の順)。引用・メモは `tag` の入った文、助言は「できていない」のときだけ
    fn main_items(tag: &str, status: &str) -> Vec<StoredItem> {
        loaded()
            .checks
            .checks
            .iter()
            .filter(|c| !is_hearing(c))
            .enumerate()
            .map(|(i, c)| StoredItem {
                id: c.id.clone(),
                status: status.into(),
                evidence: vec![3, 4],
                quote: Some(format!("{tag}の引用{i}")),
                advice: (status == "not_done").then(|| format!("{tag}の助言{i}")),
                note: Some(format!("{tag}のメモ{i}")),
                answer: None,
                asked: None,
                answered: None,
            })
            .collect()
    }

    /// ヒアリングの 6 項目 (checks の順)。`status` は done / not_done / na
    fn hearing_items(status: &str) -> Vec<StoredItem> {
        loaded()
            .checks
            .checks
            .iter()
            .filter(|c| is_hearing(c))
            .map(|c| StoredItem {
                id: c.id.clone(),
                status: status.into(),
                evidence: vec![],
                quote: None,
                advice: None,
                note: None,
                answer: None,
                asked: None,
                answered: None,
            })
            .collect()
    }

    /// ヒアリングの判定の結果 (先頭が「できた」で先方の答えつき、4 番目が「できていない」、残りは該当なし)
    fn judged_hearing() -> Vec<StoredItem> {
        let mut h = hearing_items("na");
        h[0].status = "done".into();
        h[0].answer = Some("3か月以内".into());
        h[0].asked = Some(true);
        h[0].answered = Some(true);
        h[0].quote = Some("来月からです".into());
        h[3].status = "not_done".into();
        h[3].advice = Some("金額の目安を聞く".into());
        h
    }

    fn items_text(items: &[StoredItem]) -> String {
        serde_json::to_string(items).unwrap()
    }

    /// 偽 Turso の表に判定済みなどの行を直接入れる (`updated_at` は `u0`: 書き込まれたかの目印)
    fn seed_row(
        conn: &SharedConn,
        (id, call_at, status): (&str, &str, &str),
        (outcome, zoom): (Option<&str>, Option<&str>),
        items: Option<&[StoredItem]>,
        summary: Option<&str>,
    ) {
        conn.lock()
            .unwrap()
            .execute(
                "INSERT INTO call_talk_reviews (hubspot_call_id, owner_id, call_at, duration_sec, \
                 zoom_call_log_id, status, attempts, checks_version, model, outcome, reached_person, \
                 reception, first_date_turn, items, summary, note, created_at, updated_at) \
                 VALUES (?1, 'o1', ?2, 95, ?3, ?4, 1, 'v1', 'MiniMax-M3', ?5, 1, 0, 3, ?6, ?7, 'contacted', 'c', 'u0')",
                rusqlite::params![id, call_at, zoom, status, outcome, items.map(items_text), summary],
            )
            .unwrap();
    }

    /// 起動後に別の書き手が項目を書き換えた状態にする (`updated_at` は変えない)
    fn rewrite_items(conn: &SharedConn, id: &str, items: &[StoredItem], summary: Option<&str>) {
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE call_talk_reviews SET items = ?2, summary = ?3 WHERE hubspot_call_id = ?1",
                rusqlite::params![id, items_text(items), summary],
            )
            .unwrap();
    }

    fn updated_at(conn: &SharedConn, id: &str) -> String {
        conn.lock()
            .unwrap()
            .query_row(
                "SELECT updated_at FROM call_talk_reviews WHERE hubspot_call_id = ?1",
                [id],
                |r| r.get(0),
            )
            .unwrap()
    }

    async fn get_row(audit: &crate::audit::AuditDb, id: &str) -> Option<ReviewRow> {
        let id = id.to_string();
        store::blocking(audit, move |t| store::get(t, &id))
            .await
            .unwrap()
    }

    /// 偽 Turso への UPDATE の回数 (`call_talk_reviews` への書き込み)
    fn updates(queries: &std::sync::Mutex<Vec<String>>) -> usize {
        queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.starts_with("UPDATE call_talk_reviews"))
            .count()
    }

    /// ヒアリングを付けるための偽の判定 (今の判定は使わない)
    struct HearingJudge(Result<Vec<StoredItem>, JudgeError>);
    impl Judge for HearingJudge {
        async fn judge(
            &self,
            _: &super::super::checks::Checks,
            _: &[TalkTurn],
        ) -> Result<JudgeOutput, JudgeError> {
            Err(JudgeError::NotConfigured)
        }
        async fn hearing(
            &self,
            _: &super::super::checks::Checks,
            _: &[TalkTurn],
        ) -> Result<Vec<StoredItem>, JudgeError> {
            self.0.clone()
        }
    }

    fn test_worker(
        audit: crate::audit::AuditDb,
        hubspot_url: &str,
        zoom: FakeZoom,
        judge: HearingJudge,
    ) -> Worker<FakeZoom, HearingJudge> {
        let opts = ClientOptions {
            timeout: Duration::from_millis(800),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
            search_min_interval: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(1),
        };
        Worker {
            audit,
            client: Arc::new(HubSpotClient::new("test-token".into(), hubspot_url, opts).unwrap()),
            zoom: Arc::new(zoom),
            judge: Arc::new(judge),
            cfg: TalkReviewConfig::from_lookup(|_| None),
            known: HashSet::new(),
            waiting: HashMap::new(),
            skipped: HashSet::new(),
            used_marks: HashSet::new(),
            team: None,
            counts_task: None,
            last_poll: None,
            backfill_next: None,
            backfill_anchor: None,
            pending: Arc::new(PendingWrites::default()),
            rejudge: VecDeque::new(),
            hearing_check: HashMap::new(),
            judged_rows: Default::default(),
        }
    }

    fn cand(
        id: &str,
        call_ms: i64,
        scheduled: bool,
        has_hearing: bool,
        next: &str,
    ) -> HearingCandidate {
        HearingCandidate {
            id: id.into(),
            call_ms,
            scheduled,
            has_hearing,
            zoom_call_log_id: Some(format!("z-{id}")),
            next: ts(next),
        }
    }

    #[test]
    fn light_candidates_decide_like_the_full_rows() {
        let c = &loaded().checks;
        let next = ts("2026-10-27T00:00:00Z");
        let outcomes = [
            None,
            Some("決まらなかった"),
            Some("担当者不在"),
            Some("日程が決まった"),
        ];
        for outcome in outcomes {
            for hearing in ["none", "na", "done", "not_done"] {
                let mut r = row(Some("z"), "2026-10-27T01:02:03Z");
                r.status = store::ST_JUDGED.into();
                r.outcome = outcome.map(String::from);
                r.items = main_items("t", "done");
                if hearing != "none" {
                    r.items.extend(hearing_items(hearing));
                }
                let l = HearingCandidate::from_row(&r, c, next).unwrap();
                let scheduled = outcome == Some("日程が決まった");
                let heard = hearing == "done" || hearing == "not_done";
                assert_eq!(l.scheduled, scheduled, "{outcome:?}・{hearing}");
                assert_eq!(l.has_hearing, heard, "{outcome:?}・{hearing}");
                assert_eq!(l.has_hearing, has_hearing(&r, c));
                // 順番は、日程が決まった行とヒアリングが付いている行が先 (0)、ほかは後 (1)
                let want = if scheduled || heard { 0 } else { 1 };
                assert_eq!(l.priority(), want, "{outcome:?}・{hearing}");
                assert_eq!(l.priority(), hearing_priority(&r, c));
                assert_eq!(
                    (
                        l.id.as_str(),
                        l.call_ms,
                        l.zoom_call_log_id.as_deref(),
                        l.next
                    ),
                    (
                        "1",
                        ts("2026-10-27T01:02:03Z").timestamp_millis(),
                        Some("z"),
                        next
                    )
                );
                // 判定したばかりの行 (判定待ちの行 + 判定の結果) からも、判定済みの行と同じものができる
                let pending = ReviewRow {
                    status: ST_PENDING.into(),
                    outcome: None,
                    items: vec![],
                    ..r.clone()
                };
                let o = JudgeOutput {
                    reached_person: None,
                    reception: None,
                    first_date_turn: None,
                    outcome: r.outcome.clone(),
                    items: r.items.clone(),
                    summary: None,
                };
                assert_eq!(
                    HearingCandidate::from_judged(&pending, &o, c, next),
                    Some(l)
                );
            }
        }
        // 電話の時刻が読めない行は確かめようがない
        let mut r = row(Some("z"), "いつか");
        r.status = store::ST_JUDGED.into();
        assert_eq!(HearingCandidate::from_row(&r, c, next), None);
    }

    #[test]
    fn due_rows_are_chosen_by_priority_then_newest_first() {
        let now = ts("2026-10-27T00:00:00Z");
        let due = "2026-10-26T23:00:00Z";
        let check: HashMap<String, HearingCandidate> = [
            cand("old_plain", 100, false, false, due),
            cand("new_plain", 300, false, false, due),
            cand("tie_a", 300, false, false, due),
            cand("tie_b", 300, false, false, due),
            cand("old_sched", 50, true, false, due),
            cand("mid_heard", 200, false, true, due),
            // 確かめる時刻がまだ来ていない行は選ばない (ちょうどの時刻は来ている)
            cand("later", 999, true, true, "2026-10-27T00:00:01Z"),
            cand("exact", 10, false, false, "2026-10-27T00:00:00Z"),
        ]
        .into_iter()
        .map(|c| (c.id.clone(), c))
        .collect();
        let ids =
            |v: Vec<HearingCandidate>| -> Vec<String> { v.into_iter().map(|c| c.id).collect() };
        assert_eq!(
            ids(select_due(&check, now, 100)),
            [
                "mid_heard",
                "old_sched", // 先: ヒアリングが付いている行・日程が決まった行、新しい電話から
                "new_plain",
                "tie_a",
                "tie_b",
                "old_plain",
                "exact", // 後: 新しい電話から、同じなら ID の順
            ]
        );
        // 上限で切る。選んだ行の中身は一覧のものと同じ
        let top = select_due(&check, now, 3);
        assert_eq!(ids(top.clone()), ["mid_heard", "old_sched", "new_plain"]);
        assert_eq!(top[2], check["new_plain"]);
        assert!(select_due(&HashMap::new(), now, 300).is_empty());
        assert!(select_due(&check, ts("2026-10-01T00:00:00Z"), 300).is_empty());
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn startup_scan_gives_the_same_rows_as_the_single_query_at_any_page_size() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        let judged = |id: &'static str, at: &'static str| (id, at, "judged");
        let sched = Some("日程が決まった");
        let missed = Some("決まらなかった");
        // 日本語以外が混ざる行 (判定し直す): 助言にハングル・メモに英単語・まとめに簡体字
        let mut foreign_advice = main_items("f1", "not_done");
        foreign_advice[0].advice = Some("먼저 용건을 전한다".into());
        let mut foreign_note = main_items("f2", "done");
        foreign_note[1].note = Some("operatorsconciled を使う".into());
        let heard = [main_items("a2", "done"), hearing_items("done")].concat();
        let scheduled_heard = [main_items("a5", "not_done"), hearing_items("not_done")].concat();
        let all_na = [main_items("a1", "done"), hearing_items("na")].concat();
        seed_row(
            &conn,
            judged("a1", "2026-10-20T09:00:00Z"),
            (sched, Some("z-a1")),
            Some(&all_na),
            Some("a1 のまとめ"),
        );
        seed_row(
            &conn,
            judged("a2", "2026-10-20T10:00:00Z"),
            (missed, Some("z-a2")),
            Some(&heard),
            Some("a2 のまとめ"),
        );
        seed_row(
            &conn,
            judged("a3", "2026-10-20T11:00:00Z"),
            (Some("担当者不在"), None),
            Some(&main_items("a3", "done")),
            None,
        );
        seed_row(
            &conn,
            judged("a4", "2026-10-20T12:00:00Z"),
            (None, Some("z-a4")),
            Some(&[]),
            None,
        );
        seed_row(
            &conn,
            judged("f1", "2026-10-20T13:00:00Z"),
            (missed, Some("z-f1")),
            Some(&foreign_advice),
            None,
        );
        seed_row(
            &conn,
            judged("f2", "2026-10-20T14:00:00Z"),
            (missed, None),
            Some(&foreign_note),
            Some("f2 のまとめ"),
        );
        seed_row(
            &conn,
            judged("f3", "2026-10-20T15:00:00Z"),
            (sched, Some("z-f3")),
            Some(&main_items("f3", "done")),
            Some("这个问题需要先确认"),
        );
        seed_row(
            &conn,
            judged("a5", "2026-10-20T16:00:00Z"),
            (sched, Some("z-a5")),
            Some(&scheduled_heard),
            Some("a5 のまとめ"),
        );
        // 電話の時刻が読めない行 (判定済み・日本語): 読むが、確かめる一覧には入れない
        seed_row(
            &conn,
            judged("bad", "2026-10-20T25:61:61Z"),
            (missed, Some("z-bad")),
            Some(&main_items("bad", "done")),
            None,
        );
        // 判定済みでない行・遡りの下限より前の行は読まない
        seed_row(
            &conn,
            ("p1", "2026-10-20T17:00:00Z", "pending_transcript"),
            (None, Some("z-p1")),
            None,
            None,
        );
        seed_row(
            &conn,
            ("u1", "2026-10-20T18:00:00Z", "unavailable"),
            (None, None),
            None,
            None,
        );
        seed_row(
            &conn,
            ("q1", "2026-10-20T19:00:00Z", "queued"),
            (None, None),
            None,
            None,
        );
        seed_row(
            &conn,
            ("x1", "2026-09-30T23:59:59Z", "judged"),
            (sched, Some("z-x1")),
            Some(&all_na),
            None,
        );
        let since = "2026-10-01T00:00:00Z";
        let now = ts("2026-10-27T00:00:00Z");
        let q = queries.clone();
        store::blocking(&audit, move |t| {
            let c = &loaded().checks;
            // 基準: 変更前の読み方 (1 回の問い合わせで全部の列、日本語以外が混ざる行とそれ以外に分ける)
            let (base_rejudge, base_rest): (Vec<_>, Vec<_>) =
                store::judged_since_single_query(t, since)?
                    .into_iter()
                    .partition(row_has_foreign_text);
            assert_eq!(
                base_rejudge
                    .iter()
                    .map(|r| r.hubspot_call_id.as_str())
                    .collect::<Vec<_>>(),
                ["f3", "f2", "f1"]
            );
            assert_eq!(base_rest.len(), 6);
            // 変更前は、決まった行も全部の欄のまま一覧に入れていた。同じ行から軽い型の答えを求める
            let expected = |r: &ReviewRow| {
                Some(HearingCandidate {
                    id: r.hubspot_call_id.clone(),
                    call_ms: DateTime::parse_from_rfc3339(&r.call_at)
                        .ok()?
                        .timestamp_millis(),
                    scheduled: r.outcome.as_deref() == Some("日程が決まった"),
                    has_hearing: has_hearing(r, c),
                    zoom_call_log_id: r.zoom_call_log_id.clone(),
                    next: now,
                })
            };
            let want: HashMap<String, HearingCandidate> = base_rest
                .iter()
                .filter_map(expected)
                .map(|c| (c.id.clone(), c))
                .collect();
            assert_eq!(want.len(), 5, "時刻が読めない行は入らない");
            for page in [1, 2, 3, 4, 5, 9, 10, 500] {
                q.lock().unwrap().clear();
                let scan = scan_judged(t, since, c, now, page)?;
                // 判定し直す行は全部の欄のまま、新しい電話から
                assert_eq!(
                    scan.rejudge,
                    VecDeque::from(base_rejudge.clone()),
                    "ページ {page}"
                );
                assert_eq!(scan.hearing, want, "ページ {page}");
                assert_eq!(scan.read, 9, "ページ {page}: 読んだ判定済みの行");
                // 9 行を page 行ずつ読む。最後のページが page 行ちょうどなら、空のページをもう 1 回読む
                let reads = q
                    .lock()
                    .unwrap()
                    .iter()
                    .filter(|q| q.contains("FROM call_talk_reviews") && q.contains("rowid AS rid"))
                    .count();
                assert_eq!(reads, 9 / page + 1, "ページ {page} の問い合わせ回数");
            }
            // 具体的な値: 「日程が決まった」・ヒアリング付きは先に確かめる
            let scan = scan_judged(t, since, c, now, 2)?;
            let h = &scan.hearing;
            assert_eq!((h["a1"].scheduled, h["a1"].has_hearing), (true, false));
            assert_eq!((h["a2"].scheduled, h["a2"].has_hearing), (false, true));
            assert_eq!((h["a5"].scheduled, h["a5"].has_hearing), (true, true));
            assert_eq!((h["a3"].scheduled, h["a3"].has_hearing), (false, false));
            assert_eq!(h["a3"].zoom_call_log_id, None);
            assert_eq!(h["a1"].zoom_call_log_id.as_deref(), Some("z-a1"));
            assert_eq!(
                h["a1"].call_ms,
                ts("2026-10-20T09:00:00Z").timestamp_millis()
            );
            assert!(
                h.values().all(|c| c.next == now),
                "起動時の行はすぐ確かめる"
            );
            assert!(!h.contains_key("bad") && !h.contains_key("f1") && !h.contains_key("p1"));
            Ok(())
        })
        .await
        .unwrap();
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn find_rejudge_fills_the_rejudge_queue_and_the_check_list() {
        let (audit, conn, _) = start_sqlite_audit_with_queries().await;
        let recent = |hours: i64| store::ts_text(Utc::now() - chrono::Duration::hours(hours));
        let mut foreign = main_items("f", "not_done");
        foreign[0].advice = Some("这个问题".into());
        seed_row(
            &conn,
            ("ok1", &recent(30), "judged"),
            (Some("日程が決まった"), Some("z1")),
            Some(&main_items("a", "done")),
            None,
        );
        seed_row(
            &conn,
            ("ng1", &recent(20), "judged"),
            (None, Some("z2")),
            Some(&foreign),
            None,
        );
        // 遡りの日数 (既定 14 日 + 2 日) より前の行は読まない
        seed_row(
            &conn,
            ("old", &recent(24 * 20), "judged"),
            (None, Some("z3")),
            Some(&main_items("o", "done")),
            None,
        );
        let mut w = test_worker(
            audit,
            "http://127.0.0.1:9",
            FakeZoom(Ok(turns())),
            HearingJudge(Ok(vec![])),
        );
        w.find_rejudge().await;
        assert_eq!(
            w.rejudge
                .iter()
                .map(|r| r.hubspot_call_id.as_str())
                .collect::<Vec<_>>(),
            ["ng1"]
        );
        assert_eq!(w.rejudge[0].items, foreign, "判定し直す行は全部の欄のまま");
        assert_eq!(w.hearing_check.keys().collect::<Vec<_>>(), ["ok1"]);
        let c = &w.hearing_check["ok1"];
        assert_eq!(
            (c.scheduled, c.has_hearing, c.zoom_call_log_id.as_deref()),
            (true, false, Some("z1"))
        );
        assert!(c.next <= Utc::now(), "すぐ確かめる");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn rows_judged_in_this_run_move_into_the_check_list_due_now() {
        let (audit, _conn, _) = start_sqlite_audit_with_queries().await;
        let mut w = test_worker(
            audit,
            "http://127.0.0.1:9",
            FakeZoom(Ok(turns())),
            HearingJudge(Ok(vec![])),
        );
        let now = ts("2026-10-27T00:00:00Z");
        w.judged_rows
            .lock()
            .unwrap()
            .push(cand("j1", 5, true, false, "2026-10-30T00:00:00Z"));
        w.judged_rows
            .lock()
            .unwrap()
            .push(cand("j2", 6, false, false, "2026-10-30T00:00:00Z"));
        w.take_judged(now);
        assert!(w.judged_rows.lock().unwrap().is_empty());
        assert_eq!(w.hearing_check.len(), 2);
        assert!(
            w.hearing_check.values().all(|c| c.next == now),
            "新しく判定した行はすぐ確かめる"
        );
        assert_eq!(select_due(&w.hearing_check, now, 10).len(), 2);
    }

    /// 書く直前の読み直しで、そのとき DB にある項目に足す・外す (起動時に読んだ項目では書かない)
    #[tokio::test(flavor = "multi_thread")]
    async fn strip_writes_over_the_items_in_the_database_now_not_the_ones_read_at_startup() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        let c = &loaded().checks;
        // 起動時に読んだ行: ヒアリングが付いている
        let at_startup = [main_items("起動時", "done"), hearing_items("done")].concat();
        seed_row(
            &conn,
            ("c1", "2026-10-20T00:00:00Z", "judged"),
            (missed(), Some("z1")),
            Some(&at_startup),
            Some("起動時のまとめ"),
        );
        let now = ts("2026-10-24T00:00:00Z");
        let startup_row = get_row(&audit, "c1").await.unwrap();
        let candidate = HearingCandidate::from_row(&startup_row, c, now).unwrap();
        assert!(candidate.has_hearing);
        // そのあとに項目が書き換わる (判定し直しなど): 9 項目が「できていない」になり、助言とまとめが付く。ヒアリングは付いたまま
        let in_db_now = [main_items("今", "not_done"), hearing_items("done")].concat();
        rewrite_items(&conn, "c1", &in_db_now, Some("今のまとめ"));
        queries.lock().unwrap().clear();

        let mut w = test_worker(
            audit.clone(),
            "http://127.0.0.1:9",
            FakeZoom(Ok(turns())),
            HearingJudge(Ok(vec![])),
        );
        w.hearing_check.insert("c1".into(), candidate.clone());
        // アポの記録は無く、電話 (10/20 0 時) から 72 時間を過ぎている → ヒアリングを外す
        w.act_on_hearing(vec![candidate], &HashSet::new(), now)
            .await;

        // 書き込みは UPDATE 1 回。読み直しの SELECT が 1 回増える (検証のための読み出しより先に数える)
        assert_eq!((updates(&queries), selects_of_row(&queries)), (1, 1));
        let row = get_row(&audit, "c1").await.unwrap();
        assert_eq!(
            row.items,
            merge_hearing(c, &in_db_now, hearing_na(c, NO_APO_NOTE))
        );
        // 9 項目は DB にあった今のもの
        assert_eq!(row.items[0].quote.as_deref(), Some("今の引用0"));
        assert_eq!(row.items[0].advice.as_deref(), Some("今の助言0"));
        assert_eq!(row.items[8].note.as_deref(), Some("今のメモ8"));
        // ヒアリングの 6 項目は該当なし (メモ付き)
        let hearing: Vec<&StoredItem> = row.items[9..].iter().collect();
        assert_eq!(hearing.len(), 6);
        assert!(hearing
            .iter()
            .all(|i| i.status == "na" && i.note.as_deref() == Some(NO_APO_NOTE)));
        assert!(!has_hearing(&row, c));
        // まとめは書き込み後の項目から (最初の「できていない」項目の助言)
        assert_eq!(row.summary.as_deref(), Some("今の助言0"));
        // 9 項目の判定・outcome・状態は変えない
        assert_eq!(
            (row.outcome.as_deref(), row.status.as_str()),
            (Some("決まらなかった"), "judged")
        );
        assert!(w.hearing_check.is_empty(), "済んだ行は確かめる一覧から外す");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn run_adds_hearing_to_the_items_in_the_database_now_not_the_ones_read_at_startup() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        let c = &loaded().checks;
        // 起動時に読んだ行: 日程が決まった電話で、ヒアリングはまだ無い
        let at_startup = [main_items("起動時", "done"), hearing_items("na")].concat();
        seed_row(
            &conn,
            ("c2", "2026-10-20T00:00:00Z", "judged"),
            (sched(), Some("z2")),
            Some(&at_startup),
            Some("起動時のまとめ"),
        );
        let now = ts("2026-10-21T00:00:00Z");
        let candidate =
            HearingCandidate::from_row(&get_row(&audit, "c2").await.unwrap(), c, now).unwrap();
        assert!((candidate.scheduled, candidate.has_hearing) == (true, false));
        // 文字起こしと判定に時間がかかる間に、項目が書き換わる
        let in_db_now = [main_items("今", "not_done"), hearing_items("na")].concat();
        rewrite_items(&conn, "c2", &in_db_now, Some("今のまとめ"));
        queries.lock().unwrap().clear();

        let mut w = test_worker(
            audit.clone(),
            "http://127.0.0.1:9",
            FakeZoom(Ok(turns())),
            HearingJudge(Ok(judged_hearing())),
        );
        w.hearing_check.insert("c2".into(), candidate.clone());
        // アポの記録がある → ヒアリングを付ける
        w.act_on_hearing(vec![candidate], &HashSet::from(["c2".to_string()]), now)
            .await;

        assert_eq!((updates(&queries), selects_of_row(&queries)), (1, 1));
        let row = get_row(&audit, "c2").await.unwrap();
        assert_eq!(row.items, merge_hearing(c, &in_db_now, judged_hearing()));
        assert_eq!(
            row.items[0].quote.as_deref(),
            Some("今の引用0"),
            "9 項目は DB にあった今のもの"
        );
        assert_eq!(row.items[0].advice.as_deref(), Some("今の助言0"));
        let (first, fourth) = (&row.items[9], &row.items[12]);
        assert_eq!(
            (first.status.as_str(), first.answer.as_deref()),
            ("done", Some("3か月以内"))
        );
        assert_eq!(
            (fourth.status.as_str(), fourth.advice.as_deref()),
            ("not_done", Some("金額の目安を聞く"))
        );
        assert!(has_hearing(&row, c));
        assert_eq!(row.summary.as_deref(), Some("今の助言0"));
        assert_eq!(
            (row.outcome.as_deref(), row.status.as_str()),
            (Some("日程が決まった"), "judged")
        );
        assert!(w.hearing_check.is_empty());
    }

    fn sched() -> Option<&'static str> {
        Some("日程が決まった")
    }
    fn missed() -> Option<&'static str> {
        Some("決まらなかった")
    }

    /// 偽 Turso への、1 行を ID で読む SELECT の回数
    fn selects_of_row(queries: &std::sync::Mutex<Vec<String>>) -> usize {
        queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.starts_with("SELECT") && q.contains("WHERE hubspot_call_id = ?1"))
            .count()
    }

    /// 読み直した行がもう付ける・外す必要のない状態なら、何も書かずに確かめる一覧から外す
    #[tokio::test(flavor = "multi_thread")]
    async fn nothing_is_written_when_the_row_no_longer_needs_the_change() {
        let c = &loaded().checks;
        let na = [main_items("m", "done"), hearing_items("na")].concat();
        let heard = [main_items("m", "done"), hearing_items("done")].concat();
        // 読み直した時点の行: (状態, 項目)。行が無いときは None
        type InDb<'a> = Option<(&'a str, &'a [StoredItem])>;
        // (名前, 付けるか, 読み直した時点の行)
        let cases: [(&str, bool, InDb); 6] = [
            (
                "付けるのに、もう別の周回が付けた",
                true,
                Some(("judged", &heard)),
            ),
            (
                "付けるのに、判定し直しの途中で判定済みでなくなった",
                true,
                Some(("pending_transcript", &na)),
            ),
            ("付けるのに、行が無い", true, None),
            ("外すのに、もう付いていない", false, Some(("judged", &na))),
            (
                "外すのに、判定済みでなくなった",
                false,
                Some(("pending_transcript", &heard)),
            ),
            ("外すのに、行が無い", false, None),
        ];
        for (name, attach, in_db) in cases {
            let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
            let startup_items = if attach { &na } else { &heard };
            // 起動時の行を一度入れて軽い型を作り、そのあと DB の行を読み直した時点の状態にする
            seed_row(
                &conn,
                ("c3", "2026-10-20T00:00:00Z", "judged"),
                (sched(), Some("z3")),
                Some(startup_items),
                Some("まとめ"),
            );
            let now = ts("2026-10-24T00:00:00Z");
            let candidate =
                HearingCandidate::from_row(&get_row(&audit, "c3").await.unwrap(), c, now).unwrap();
            assert_eq!(candidate.has_hearing, !attach, "{name}");
            match in_db {
                Some((status, items)) => {
                    rewrite_items(&conn, "c3", items, Some("まとめ"));
                    conn.lock()
                        .unwrap()
                        .execute(
                            "UPDATE call_talk_reviews SET status = ?1 WHERE hubspot_call_id = 'c3'",
                            [status],
                        )
                        .unwrap();
                }
                None => {
                    conn.lock()
                        .unwrap()
                        .execute("DELETE FROM call_talk_reviews", [])
                        .unwrap();
                }
            }
            queries.lock().unwrap().clear();
            let before = in_db.map(|_| get_row_sync(&conn, "c3"));

            let mut w = test_worker(
                audit.clone(),
                "http://127.0.0.1:9",
                FakeZoom(Ok(turns())),
                HearingJudge(Ok(judged_hearing())),
            );
            w.hearing_check.insert("c3".into(), candidate.clone());
            let matched = if attach {
                HashSet::from(["c3".to_string()])
            } else {
                HashSet::new()
            };
            w.act_on_hearing(vec![candidate], &matched, now).await;

            assert_eq!(updates(&queries), 0, "{name}: UPDATE しない");
            assert!(w.hearing_check.is_empty(), "{name}: 確かめる一覧から外す");
            assert_eq!(
                before.is_some(),
                get_row(&audit, "c3").await.is_some(),
                "{name}"
            );
            if let Some(before) = before {
                assert_eq!(
                    get_row(&audit, "c3").await.unwrap(),
                    before,
                    "{name}: 行は変わらない"
                );
                assert_eq!(updated_at(&conn, "c3"), "u0", "{name}");
            }
        }
    }

    fn get_row_sync(conn: &SharedConn, id: &str) -> ReviewRow {
        let (items, status, outcome): (Option<String>, String, Option<String>) = conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT items, status, outcome FROM call_talk_reviews WHERE hubspot_call_id = ?1",
                [id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap();
        ReviewRow {
            hubspot_call_id: id.into(),
            owner_id: "o1".into(),
            call_at: "2026-10-20T00:00:00Z".into(),
            duration_sec: 95,
            zoom_call_log_id: Some("z3".into()),
            status,
            attempts: 1,
            checks_version: Some("v1".into()),
            model: Some("MiniMax-M3".into()),
            outcome,
            reached_person: Some(true),
            reception: Some(false),
            first_date_turn: Some(3),
            items: serde_json::from_str(&items.unwrap_or_default()).unwrap_or_default(),
            summary: Some("まとめ".into()),
            note: Some("contacted".into()),
        }
    }

    /// 行を読み直せなかったら、書かずに 1 時間後に確かめ直す (一覧に残す)
    #[tokio::test(flavor = "multi_thread")]
    async fn a_failed_reread_keeps_the_row_in_the_list_for_an_hour_later() {
        let c = &loaded().checks;
        for attach in [false, true] {
            let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
            let items = [
                main_items("m", "done"),
                hearing_items(if attach { "na" } else { "done" }),
            ]
            .concat();
            seed_row(
                &conn,
                ("c4", "2026-10-20T00:00:00Z", "judged"),
                (sched(), Some("z4")),
                Some(&items),
                None,
            );
            let now = ts("2026-10-24T00:00:00Z");
            let candidate =
                HearingCandidate::from_row(&get_row(&audit, "c4").await.unwrap(), c, now).unwrap();
            // 表を読めない状態 (SELECT が失敗する)
            conn.lock()
                .unwrap()
                .execute("DROP TABLE call_talk_reviews", [])
                .unwrap();
            queries.lock().unwrap().clear();
            let mut w = test_worker(
                audit.clone(),
                "http://127.0.0.1:9",
                FakeZoom(Ok(turns())),
                HearingJudge(Ok(judged_hearing())),
            );
            w.hearing_check.insert("c4".into(), candidate.clone());
            let matched = if attach {
                HashSet::from(["c4".to_string()])
            } else {
                HashSet::new()
            };
            w.act_on_hearing(vec![candidate], &matched, now).await;
            assert_eq!(updates(&queries), 0, "attach={attach}");
            assert_eq!(w.hearing_check.len(), 1, "attach={attach}: 一覧に残す");
            assert_eq!(
                w.hearing_check["c4"].next,
                now + chrono::Duration::hours(1),
                "attach={attach}"
            );
        }
    }

    /// 録音が取れない行 (録音が消えた・文字起こしできない・話者が分かれない・録音 ID が無い) はあきらめ、
    /// 一時的な失敗 (Zoom・判定の通信など) は 1 時間後にやり直す。どちらも書かない
    #[tokio::test(flavor = "multi_thread")]
    async fn recording_problems_give_up_but_temporary_failures_try_again_in_an_hour() {
        let c = &loaded().checks;
        let now = ts("2026-10-21T00:00:00Z");
        let one_side = turns()[..1].to_vec();
        let transient = || ZoomError::Transient("http 429".into());
        struct Case {
            name: &'static str,
            has_zoom_id: bool,
            zoom: Result<Vec<TalkTurn>, ZoomError>,
            judge_err: JudgeError,
            /// あきらめて一覧から外すか (false なら 1 時間後にやり直す)
            give_up: bool,
        }
        let case = |name, has_zoom_id, zoom, judge_err, give_up| Case {
            name,
            has_zoom_id,
            zoom,
            judge_err,
            give_up,
        };
        let no_key = || JudgeError::NotConfigured;
        let cases = vec![
            case("録音 ID が無い", false, Ok(turns()), no_key(), true),
            case("録音が消えた", true, Err(ZoomError::Gone), no_key(), true),
            case(
                "文字起こしできない",
                true,
                Err(ZoomError::Untranscribable),
                no_key(),
                true,
            ),
            case("営業しか話していない", true, Ok(one_side), no_key(), true),
            case(
                "文字起こしがまだ",
                true,
                Err(ZoomError::NotReady),
                no_key(),
                false,
            ),
            case("Zoom の通信の失敗", true, Err(transient()), no_key(), false),
            case(
                "判定の通信の失敗",
                true,
                Ok(turns()),
                JudgeError::Transient("http 429".into()),
                false,
            ),
            case("判定の鍵が無い", true, Ok(turns()), no_key(), false),
        ];
        for Case {
            name,
            has_zoom_id,
            zoom,
            judge_err,
            give_up,
        } in cases
        {
            let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
            let items = [main_items("m", "done"), hearing_items("na")].concat();
            let zoom_id = has_zoom_id.then_some("z5");
            seed_row(
                &conn,
                ("c5", "2026-10-20T00:00:00Z", "judged"),
                (sched(), zoom_id),
                Some(&items),
                None,
            );
            let candidate =
                HearingCandidate::from_row(&get_row(&audit, "c5").await.unwrap(), c, now).unwrap();
            queries.lock().unwrap().clear();
            let mut w = test_worker(
                audit.clone(),
                "http://127.0.0.1:9",
                FakeZoom(zoom),
                HearingJudge(Err(judge_err)),
            );
            w.hearing_check.insert("c5".into(), candidate.clone());
            w.act_on_hearing(vec![candidate], &HashSet::from(["c5".to_string()]), now)
                .await;
            assert_eq!(updates(&queries), 0, "{name}: 書かない");
            assert_eq!(updated_at(&conn, "c5"), "u0", "{name}");
            if give_up {
                assert!(w.hearing_check.is_empty(), "{name}: あきらめて一覧から外す");
            } else {
                assert_eq!(w.hearing_check.len(), 1, "{name}: 一覧に残す");
                assert_eq!(
                    w.hearing_check["c5"].next,
                    now + chrono::Duration::hours(1),
                    "{name}"
                );
            }
        }
    }

    // ---- 周回の全体 (偽の HubSpot・Zoom・判定・Turso) -------------------------------------------

    use axum::extract::{Path, State};
    use axum::routing::post;
    use axum::{Json, Router};

    /// 偽の HubSpot: 電話 → 取引の関連と、取引がアポ日確定に入った時刻だけを答える
    struct FakeHs {
        call_deals: HashMap<String, Vec<String>>,
        entered: HashMap<String, String>,
    }

    async fn fake_associations(
        State(st): State<Arc<FakeHs>>,
        Path((from, to)): Path<(String, String)>,
        Json(body): Json<Value>,
    ) -> Json<Value> {
        if (from.as_str(), to.as_str()) != ("calls", "deals") {
            return Json(json!({"results": []}));
        }
        let results: Vec<Value> = body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| {
                let id = i["id"].as_str()?;
                let deals = st.call_deals.get(id)?;
                Some(json!({
                    "from": {"id": id},
                    "to": deals.iter().map(|d| json!({"toObjectId": d, "associationTypes": []})).collect::<Vec<_>>(),
                }))
            })
            .collect();
        Json(json!({"results": results}))
    }

    async fn fake_deals(State(st): State<Arc<FakeHs>>, Json(body): Json<Value>) -> Json<Value> {
        let results: Vec<Value> = body["inputs"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|i| {
                let id = i["id"].as_str()?;
                let at = st.entered.get(id)?;
                Some(json!({"id": id, "properties": {"hs_v2_date_entered_52035886": at}}))
            })
            .collect();
        Json(json!({"results": results}))
    }

    async fn start_fake_hubspot(call_deals: &[(&str, &str)], entered: &[(&str, String)]) -> String {
        let mut cd: HashMap<String, Vec<String>> = HashMap::new();
        for (call, deal) in call_deals {
            cd.entry(call.to_string())
                .or_default()
                .push(deal.to_string());
        }
        let st = Arc::new(FakeHs {
            call_deals: cd,
            entered: entered
                .iter()
                .map(|(d, at)| (d.to_string(), at.clone()))
                .collect(),
        });
        let app = Router::new()
            .route(
                "/crm/v4/associations/{from}/{to}/batch/read",
                post(fake_associations),
            )
            .route("/crm/v3/objects/deals/batch/read", post(fake_deals))
            .with_state(st);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        url
    }

    /// 1 周のヒアリングの周回 (`process_hearing`): アポの記録の有無・待つ期間・ヒアリングの有無・録音で、
    /// 付ける・外す・待つ・済み・あきらめるが決まる。書くのは項目とまとめの UPDATE 1 回ずつ
    #[tokio::test(flavor = "multi_thread")]
    async fn one_hearing_cycle_attaches_strips_waits_and_gives_up_by_the_rules() {
        let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
        let c = &loaded().checks;
        let hours_ago = |h: i64| Utc::now() - chrono::Duration::hours(h);
        let at = |h: i64| store::ts_text(hours_ago(h));
        let na = [main_items("m", "done"), hearing_items("na")].concat();
        let heard = [main_items("m", "done"), hearing_items("done")].concat();
        // 5001: 日程が決まった・ヒアリング無し・電話の 1 時間後にアポ日確定の記録がある → 付ける
        seed_row(
            &conn,
            ("5001", &at(10), "judged"),
            (sched(), Some("zoom5001")),
            Some(&na),
            Some("s"),
        );
        // 5002: ヒアリングが付いている・記録が無く、電話から 5 日 (72 時間を過ぎている) → 外す
        seed_row(
            &conn,
            ("5002", &at(24 * 5), "judged"),
            (missed(), Some("zoom5002")),
            Some(&heard),
            Some("s"),
        );
        // 5003: ヒアリングが付いている・記録が無く、電話から 10 時間 (まだ待つ) → 1 時間後に確かめ直す
        seed_row(
            &conn,
            ("5003", &at(10), "judged"),
            (missed(), Some("zoom5003")),
            Some(&heard),
            Some("s"),
        );
        // 5004: ヒアリング無し・記録が無く、電話から 5 日 → 対象外と決まる (何も書かない)
        seed_row(
            &conn,
            ("5004", &at(24 * 5), "judged"),
            (missed(), Some("zoom5004")),
            Some(&na),
            Some("s"),
        );
        // 5005: 記録があるが録音の ID が無い → あきらめる (何も書かない)
        seed_row(
            &conn,
            ("5005", &at(10), "judged"),
            (sched(), None),
            Some(&na),
            Some("s"),
        );
        // 5006: 電話の 1 時間前より前にアポ日確定に入っていた取引 (確認の電話) は、記録とみなさない
        //       → 記録が無いのと同じで、待つ期間 (72 時間) の間は 1 時間後にまた確かめる
        seed_row(
            &conn,
            ("5006", &at(10), "judged"),
            (sched(), Some("zoom5006")),
            Some(&na),
            Some("s"),
        );
        let hs = start_fake_hubspot(
            &[("5001", "9001"), ("5005", "9005"), ("5006", "9006")],
            &[
                ("9001", store::ts_text(hours_ago(9))),
                ("9005", store::ts_text(hours_ago(9))),
                ("9006", store::ts_text(hours_ago(10 + 3))),
            ],
        )
        .await;
        let mut w = test_worker(
            audit.clone(),
            &hs,
            FakeZoom(Ok(turns())),
            HearingJudge(Ok(judged_hearing())),
        );
        // 起動時の読み取り (期間は設定の遡り日数 + 2 日)
        w.find_rejudge().await;
        assert_eq!(w.hearing_check.len(), 6);
        assert!(w.rejudge.is_empty());
        queries.lock().unwrap().clear();

        w.process_hearing().await;

        // 5001: ヒアリングが付く (今の項目はそのまま)
        let r = get_row(&audit, "5001").await.unwrap();
        assert_eq!(r.items, merge_hearing(c, &na, judged_hearing()));
        assert_eq!(r.outcome.as_deref(), Some("日程が決まった"));
        // 5002: ヒアリングは該当なし (メモ付き) になる
        let r = get_row(&audit, "5002").await.unwrap();
        assert_eq!(
            r.items,
            merge_hearing(c, &heard, hearing_na(c, NO_APO_NOTE))
        );
        // 5003〜5006: 書かない
        for id in ["5003", "5004", "5005", "5006"] {
            assert_eq!(updated_at(&conn, id), "u0", "{id}");
        }
        assert_ne!(updated_at(&conn, "5001"), "u0");
        assert_ne!(updated_at(&conn, "5002"), "u0");
        assert_eq!(
            updates(&queries),
            2,
            "書き込みは付けた 1 行と外した 1 行の UPDATE だけ"
        );
        // 待つ 5003・5006 だけが一覧に残り、1 時間ほど先に確かめ直す。5001・5002 は書いた、5004・5005 は決まったので外れる
        let mut left: Vec<&String> = w.hearing_check.keys().collect();
        left.sort();
        assert_eq!(left, ["5003", "5006"]);
        for id in ["5003", "5006"] {
            let wait = w.hearing_check[id].next - Utc::now();
            assert!(
                wait > chrono::Duration::minutes(59) && wait <= chrono::Duration::hours(1),
                "{id}: {wait}"
            );
        }
        // 次の周回では、確かめる時刻がまだ来ていない 5003・5006 は確かめない
        queries.lock().unwrap().clear();
        w.process_hearing().await;
        assert_eq!(updates(&queries), 0);
    }
}
