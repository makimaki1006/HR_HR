//! 背景の worker (ADR-033)。`CALL_REVIEW_ENABLED=1` のときだけ動く。
//!
//! 10 分おき (既定) に、対象のチームの担当者の Call (Zoom 純正連携・発信・70 秒以上) を
//! Search (関所の `Background` 優先度) で見つけて表に行を作り (順番待ち `queued`)、1 人 1 日あたり
//! `CALL_REVIEW_PER_PERSON_DAY` 本まで新しい電話から文字起こし待ちにして、Zoom から文字起こしを取って判定する。
//! 初回は過去 `CALL_REVIEW_BACKFILL_DAYS` 日まで遡る。同時に `CALL_REVIEW_CONCURRENCY` 本。判定済みの Call は二度判定しない。
//! 文字起こしがまだ無いものは間隔を空けて再試行し (上限あり)、取れないものは `unavailable` にして諦める
//! (取れなくなった電話の分は、同じ人・同じ日の次の電話が枠に入る)。
//! 録音 URL が Call に無いもの (Call Queue 経由の発信) は、Zoom の通話履歴から相手番号と時刻で録音を探す。
//! アポの日ごとの数え (`counts`) も、ここで前もって新しい日から進める。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::checks::loaded;
use super::config::TalkReviewConfig;
use super::counts::TeamOwners;
use super::judge::{
    sanitize_texts, source_text, texts_foreign, Judge, JudgeError, JudgeOutput, MiniMaxJudge,
};
use super::store::{self, NewRow, ReviewRow, ST_FAILED, ST_QUEUED, ST_UNAVAILABLE};
use super::transcript::{extract_call_log_id, has_both_sides};
use super::zoom::{TranscriptSource, ZoomClient, ZoomError, REC_PREFIX};
use crate::crm::caller_metrics::{flt, jst_day, CALL_SOURCE};
use crate::crm::caller_metrics_month::{jst_midnight_ms, OWNER_CHUNK};
use crate::crm::owners::CrmOwner;
use crate::hubspot::HubSpotClient;
use crate::AppState;

/// 再試行の上限 (これに達したら取れない / 失敗として諦める)
pub const MAX_ATTEMPTS: u32 = 8;
/// 1 回の見つけ直しの Search の上限 (暴走の安全装置)
const MAX_SEARCHES: u32 = 300;
/// 1 周の中で処理する行の上限
const MAX_PER_CYCLE: usize = 1_000;
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

// ---------------------------------------------------------------------------
// 1 周
// ---------------------------------------------------------------------------

struct Worker<Z, J> {
    audit: crate::audit::AuditDb,
    client: Arc<HubSpotClient>,
    zoom: Arc<Z>,
    judge: Arc<J>,
    cfg: TalkReviewConfig,
    known: HashSet<String>,
    /// 対象の担当者 (アポの数えに使う)
    team: Option<TeamOwners>,
    counts_task: Option<tokio::task::JoinHandle<()>>,
    last_poll: Option<DateTime<Utc>>,
    /// 初回の取り込みの進み具合 (次に読む日)。全部読み終えるまで `last_poll` は進めない
    backfill_next: Option<chrono::NaiveDate>,
    backfill_anchor: Option<DateTime<Utc>>,
    pending: Arc<PendingWrites>,
    /// 助言の文が日本語だけでない判定済みの行 (起動時に 1 回だけ探し、周回ごとに少しずつ判定し直す)
    rejudge: std::collections::VecDeque<ReviewRow>,
}

/// 保存済みの行の文章に日本語以外が混ざっているか (英単語の許可は行の引用だけで判断する)
pub fn row_has_foreign_text(row: &ReviewRow) -> bool {
    let src = source_text(&[], &row.items);
    texts_foreign(&row.items, row.summary.as_deref(), &src)
}

/// 1 周に判定し直す行の上限 (同時に判定する本数の 2 倍。既定 8 本)
fn rejudge_limit(concurrency: usize) -> usize {
    (concurrency * 2).max(1)
}

impl<Z: TranscriptSource + 'static, J: Judge + 'static> Worker<Z, J> {
    /// 起動後に 1 回: 判定済みの行から、文章に日本語以外が混ざったものを集める (SELECT 1 回)
    async fn find_rejudge(&mut self) {
        let since = store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.backfill_days + 2));
        match store::blocking(&self.audit, move |t| store::judged_since(t, &since)).await {
            Ok(rows) => {
                self.rejudge = rows.into_iter().filter(row_has_foreign_text).collect();
                tracing::info!(rows = self.rejudge.len(), "talk review: rows to rejudge");
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
            let (zoom, judge, audit, model, sem, pending) = (
                self.zoom.clone(),
                self.judge.clone(),
                self.audit.clone(),
                self.cfg.model.clone(),
                sem.clone(),
                self.pending.clone(),
            );
            set.spawn(async move {
                let Ok(_p) = sem.acquire().await else { return };
                match process_one(&*zoom, &*judge, &row, Utc::now()).await {
                    Outcome::Judged(o) => {
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
        let min_ms = i64::from(loaded().checks.min_talk_sec) * 1000;
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

    /// 新しい Call を順番待ちの行にする。担当者は Zoom でかけた人 (`attribute`)。
    /// Zoom が読めなければ `Err` で止め、呼び出し側は同じ範囲を次の周回で読み直す
    async fn ingest(
        &mut self,
        cands: Vec<Candidate>,
        now: DateTime<Utc>,
        email_owner: &HashMap<String, String>,
    ) -> Result<(), String> {
        let (mut fresh, mut moved) = (0, 0);
        for c in cands {
            if self.known.contains(&c.id) {
                continue;
            }
            let Some(at) = DateTime::<Utc>::from_timestamp_millis(c.ts_ms) else {
                continue;
            };
            // 録音 URL は通話のあとで本文に入る。まだなら次の周回で見直す。
            // 猶予を過ぎても無いものは、判定のときに Zoom の通話履歴から探す
            if c.zoom_id.is_none() && now - at < URL_GRACE {
                continue;
            }
            let owner = caller_owner(
                &*self.zoom,
                &c.owner_id,
                c.to_number.as_deref(),
                c.ts_ms,
                email_owner,
            )
            .await?;
            if owner != c.owner_id {
                moved += 1;
            }
            let ts = store::ts_text(now);
            let row = NewRow {
                hubspot_call_id: c.id.clone(),
                owner_id: owner,
                call_at: store::ts_text(at),
                duration_sec: c.duration_ms / 1000,
                zoom_call_log_id: c.zoom_id,
                status: ST_QUEUED,
                note: None,
                next_retry_at: None,
            };
            match store::blocking(&self.audit, move |t| store::insert_new(t, &ts, &row)).await {
                Ok(()) => {
                    self.known.insert(c.id);
                    fresh += 1;
                }
                Err(e) => tracing::warn!(error = %e, "talk review: row insert failed"),
            }
        }
        tracing::info!(
            new_rows = fresh,
            caller_differs = moved,
            "talk review ingest done"
        );
        Ok(())
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
        let rows =
            store::blocking(&self.audit, move |t| store::rows_in(t, &since, &to, None)).await?;
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
                let (zoom, judge, audit, model, sem, pending) = (
                    self.zoom.clone(),
                    self.judge.clone(),
                    self.audit.clone(),
                    self.cfg.model.clone(),
                    sem.clone(),
                    self.pending.clone(),
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
            if !self.process_due().await {
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
        team: None,
        counts_task: None,
        last_poll: None,
        backfill_next: None,
        backfill_anchor: None,
        pending: Arc::new(PendingWrites::default()),
        rejudge: Default::default(),
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
    fn rows_with_foreign_advice_are_found_for_rejudge() {
        let item = |advice: &str, quote: &str| StoredItem {
            id: "early_date".into(),
            status: "not_done".into(),
            evidence: vec![],
            quote: Some(quote.into()),
            advice: Some(advice.into()),
            note: None,
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
}
