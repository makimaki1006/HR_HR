//! 背景の worker (ADR-033)。`CALL_REVIEW_ENABLED=1` のときだけ動く。
//!
//! 10 分おき (既定) に、対象のチームの担当者の Call (Zoom 純正連携・発信・70 秒以上・録音 URL あり) を
//! Search (関所の `Background` 優先度) で見つけて表に行を作り、文字起こしを Zoom から取って判定する。
//! 初回は過去 `CALL_REVIEW_BACKFILL_DAYS` 日まで遡る。同時に 2 本。判定済みの Call は二度判定しない。
//! 文字起こしがまだ無いものは間隔を空けて再試行し (上限あり)、取れないものは `unavailable` にして諦める。

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use serde_json::{json, Value};

use super::checks::loaded;
use super::config::TalkReviewConfig;
use super::judge::{Judge, JudgeError, JudgeOutput, MiniMaxJudge};
use super::store::{self, NewRow, ReviewRow, ST_FAILED, ST_PENDING, ST_UNAVAILABLE};
use super::transcript::{extract_call_log_id, has_both_sides};
use super::zoom::{TranscriptSource, ZoomClient, ZoomError};
use crate::crm::caller_metrics::{flt, jst_day, CALL_SOURCE};
use crate::crm::caller_metrics_month::{jst_midnight_ms, OWNER_CHUNK};
use crate::crm::owners::CrmOwner;
use crate::hubspot::HubSpotClient;
use crate::AppState;

/// 再試行の上限 (これに達したら取れない / 失敗として諦める)
pub const MAX_ATTEMPTS: u32 = 8;
/// 同時に判定する本数
pub const CONCURRENCY: usize = 2;
/// 1 回の見つけ直しの Search の上限 (暴走の安全装置)
const MAX_SEARCHES: u32 = 300;
/// 1 周の中で処理する行の上限
const MAX_PER_CYCLE: usize = 1_000;
/// 録音 URL がまだ無い Call を「録音なし」にするまでの猶予
const URL_GRACE: chrono::Duration = chrono::Duration::hours(2);
/// 古くて録音が消えたとみなす Call の年齢
const GONE_AFTER: chrono::Duration = chrono::Duration::days(1);

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

pub fn search_body(
    range_prop: &str,
    from: i64,
    to: i64,
    ts_from: Option<i64>,
    owners: &[String],
    min_ms: i64,
    after: Option<&str>,
) -> Value {
    let mut filters = vec![
        flt("hs_call_source", "EQ", json!(CALL_SOURCE)),
        flt("hs_call_direction", "EQ", json!("OUTBOUND")),
        flt("hs_call_duration", "GTE", json!(min_ms.to_string())),
        flt(range_prop, "GTE", json!(from.to_string())),
        flt(range_prop, "LT", json!(to.to_string())),
        json!({"propertyName": "hubspot_owner_id", "operator": "IN", "values": owners}),
    ];
    if let Some(t) = ts_from {
        filters.push(flt("hs_timestamp", "GTE", json!(t.to_string())));
    }
    let mut body = json!({
        "filterGroups": [{"filters": filters}],
        "properties": ["hubspot_owner_id", "hs_timestamp", "hs_call_duration", "hs_call_body"],
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
    })
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
                    search_body(
                        range_prop,
                        from,
                        to,
                        ts_from,
                        chunk,
                        min_ms,
                        after.as_deref(),
                    ),
                )
                .await?;
            let results = v.get("results").and_then(Value::as_array).ok_or_else(|| {
                crate::hubspot::HubSpotError::Decode("calls without results".into())
            })?;
            out.extend(results.iter().filter_map(parse_candidate));
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
        Err(ZoomError::Gone) => {
            let old = DateTime::parse_from_rfc3339(&row.call_at)
                .is_ok_and(|t| now - t.with_timezone(&Utc) >= GONE_AFTER);
            return if old {
                Outcome::Unavailable("recording_gone")
            } else {
                Outcome::Retry("recording_not_found_yet")
            };
        }
        Err(ZoomError::Transient(_)) => return Outcome::Retry("zoom_error"),
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
        Err(JudgeError::Transient(_)) => Outcome::Retry("judge_error"),
        Err(JudgeError::NotConfigured) => Outcome::Retry("judge_not_configured"),
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
    /// 録音 URL が無くて unavailable にした Call (URL が付いたら取り込み直す)
    no_url: HashSet<String>,
    last_poll: Option<DateTime<Utc>>,
    /// 初回の取り込みの進み具合 (次に読む日)。全部読み終えるまで `last_poll` は進めない
    backfill_next: Option<chrono::NaiveDate>,
    backfill_anchor: Option<DateTime<Utc>>,
    pending: Arc<PendingWrites>,
}

impl<Z: TranscriptSource + 'static, J: Judge + 'static> Worker<Z, J> {
    async fn discover(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let bg = self.client.background();
        let list = crate::crm::owners::fetch_all(&bg)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        let ids: Vec<String> = {
            let mut v: Vec<String> = team_members(&list.owners, &self.cfg.teams)
                .into_iter()
                .map(|m| m.id)
                .collect();
            v.sort();
            v
        };
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
                    self.ingest(cands, now).await;
                    day += chrono::Duration::days(1);
                    self.backfill_next = Some(day);
                }
                self.last_poll = Some(anchor);
            }
            Some(prev) => {
                let to = (now + chrono::Duration::hours(1)).timestamp_millis();
                let from = (prev - chrono::Duration::minutes(30))
                    .timestamp_millis()
                    .max(backfill_from);
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
                self.ingest(cands, now).await;
                self.last_poll = Some(now);
            }
        }
        Ok(())
    }

    async fn ingest(&mut self, cands: Vec<Candidate>, now: DateTime<Utc>) {
        let (mut fresh, mut revived) = (0, 0);
        for c in cands {
            if self.known.contains(&c.id) {
                continue;
            }
            let ts = store::ts_text(now);
            // 録音 URL が無いまま unavailable にした Call に、後で URL が付いた
            if self.no_url.contains(&c.id) {
                let Some(z) = c.zoom_id.clone() else { continue };
                let (id, next) = (c.id.clone(), store::ts_text(now));
                match store::blocking(&self.audit, move |t| store::revive(t, &id, &z, &next, &ts))
                    .await
                {
                    Ok(()) => {
                        self.no_url.remove(&c.id);
                        self.known.insert(c.id);
                        revived += 1;
                    }
                    Err(e) => tracing::warn!(error = %e, "talk review: revive failed"),
                }
                continue;
            }
            let Some(at) = DateTime::<Utc>::from_timestamp_millis(c.ts_ms) else {
                continue;
            };
            let (status, note, next) = match (&c.zoom_id, now - at >= URL_GRACE) {
                (Some(_), _) => (ST_PENDING, None, Some(store::ts_text(now))),
                (None, true) => (ST_UNAVAILABLE, Some("no_recording_url".to_string()), None),
                // 録音 URL は通話のあとで本文に入る。まだなら次の周回で見直す
                (None, false) => continue,
            };
            let no_url_row = c.zoom_id.is_none();
            let row = NewRow {
                hubspot_call_id: c.id.clone(),
                owner_id: c.owner_id,
                call_at: store::ts_text(at),
                duration_sec: c.duration_ms / 1000,
                zoom_call_log_id: c.zoom_id,
                status,
                note,
                next_retry_at: next,
            };
            match store::blocking(&self.audit, move |t| store::insert_new(t, &ts, &row)).await {
                Ok(()) => {
                    if no_url_row {
                        self.no_url.insert(c.id.clone());
                        self.known.insert(c.id);
                    } else {
                        self.known.insert(c.id);
                    }
                    fresh += 1;
                }
                Err(e) => tracing::warn!(error = %e, "talk review: row insert failed"),
            }
        }
        tracing::info!(new_rows = fresh, revived, "talk review ingest done");
    }

    async fn process_due(&mut self) {
        let mut done: HashSet<String> = HashSet::new();
        for _ in 0..(MAX_PER_CYCLE / 40) {
            let now_s = store::ts_text(Utc::now());
            let due = match store::blocking(&self.audit, move |t| store::due(t, &now_s, 40)).await {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "talk review: due select failed");
                    return;
                }
            };
            let batch: Vec<ReviewRow> = due
                .into_iter()
                .filter(|r| {
                    !self.pending.contains(&r.hubspot_call_id)
                        && done.insert(r.hubspot_call_id.clone())
                })
                .collect();
            if batch.is_empty() {
                return;
            }
            let sem = Arc::new(tokio::sync::Semaphore::new(CONCURRENCY));
            let mut set = tokio::task::JoinSet::new();
            for row in batch {
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
                    let outcome = process_one(&*zoom, &*judge, &row, now).await;
                    let upd = decide(outcome, row.attempts, now);
                    apply(&audit, &pending, row.hubspot_call_id.clone(), upd, model).await;
                });
            }
            while set.join_next().await.is_some() {}
        }
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
        loop {
            let now = Utc::now();
            if let Err(e) = self.discover(now).await {
                tracing::warn!(error = %e, "talk review discover failed");
            }
            self.pending.flush(&self.audit).await;
            self.process_due().await;
            tokio::time::sleep(self.cfg.interval).await;
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
    let w = Worker {
        audit,
        client,
        zoom,
        judge: Arc::new(judge),
        cfg,
        known: HashSet::new(),
        no_url: HashSet::new(),
        last_poll: None,
        backfill_next: None,
        backfill_anchor: None,
        pending: Arc::new(PendingWrites::default()),
    };
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(60)).await;
        w.run().await;
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::crm::talk_review::types::TalkTurn;

    struct FakeZoom(Result<Vec<TalkTurn>, ZoomError>);
    impl TranscriptSource for FakeZoom {
        async fn fetch(&self, _: &str) -> Result<Vec<TalkTurn>, ZoomError> {
            self.0.clone()
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

        let b = search_body(
            "hs_timestamp",
            10,
            20,
            None,
            &["7".into()],
            70_000,
            Some("cur"),
        );
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
