//! 接触・日程切りの背景 worker。9 項目の判定 (`worker.rs`) とは別の周回で動き、互いに待たせない。
//!
//! 1 周: Zoom の通話ログから対象チームの「つながった発信」を新しい日から見つけて表に入れ (まとめ挿入)、
//! 判定待ちの行を新しい電話から、文字起こしを取って MiniMax で判定する。1 周で判定に使うのは `interval` まで
//! (超えたら、新しい電話を拾い直すために見つけ直しへ戻る)。
//! 見つけるのは、初回は過去 `CALL_CONTACT_BACKFILL_DAYS` 日、以後は今日と昨日だけ (通話ログの読み取りを増やさない)。
//! MiniMax が混雑 (429) を返したら、全体で少し待ってから続ける。
//!
//! 新規営業 (BPO 以外) の発信は、見つけても表に入れる前に `scope.rs` で「かけた時点の取引のステージ」を調べ、
//! アポ取りの段階なら通常どおり判定待ち、そうでなければ `out_of_scope` の行として入れる (文字起こしも MiniMax も呼ばない)。
//! 調べられるまでは表に入れずメモリで待ち (`scope_wait`)、上限 (`scope::SCOPE_WAIT`) を超えたら対象として入れる。
//! 再起動で待ちが消えても、通話ログの読み直し (今日と昨日) で拾い直すので、Turso の列も書き込みも増えない。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::config::TalkReviewConfig;
use super::contact::{self, ContactJudge, Outcome, Update};
use super::contact_store as cstore;
use super::live_dials::{owner_by_email, utc_range};
use super::scope::{self, HubspotScope, ScopeQuery, ScopeResolver};
use super::store;
use super::worker::team_members;
use super::zoom::{TranscriptSource, ZoomClient};
use crate::crm::caller_metrics::jst_day;
use crate::hubspot::HubSpotClient;

/// MiniMax の混雑 (429) のあと、新しい判定を始めるまで待つ時間
const RATE_LIMIT_PAUSE: Duration = Duration::from_secs(60);
/// 1 周で取り出す行数の上限の目安 (同時に判定する本数の倍数)
const BATCH_FACTOR: usize = 8;

struct ContactWorker<Z, J> {
    audit: crate::audit::AuditDb,
    client: Arc<HubSpotClient>,
    zoom: Arc<Z>,
    judge: Arc<J>,
    cfg: TalkReviewConfig,
    known: HashSet<String>,
    /// 初回の見つけ(遡り)が全部の日で済んだか
    backfilled: bool,
    pause_until: Arc<Mutex<Option<Instant>>>,
    /// アポ取りかの判定を待っている発信 (まだ表に入れていない)
    scope_wait: HashMap<String, contact::Candidate>,
    scope: Arc<ScopeResolver<HubspotScope>>,
    /// 発信をアポ取りだけに絞る担当者 (BPO 以外のチームの人)
    filtered: HashSet<String>,
}

impl ContactWorker<ZoomClient, super::judge::MiniMaxJudge> {
    /// 対象チームの人のつながった発信を見つけて表に入れる。担当者・Zoom ユーザーが読めない周回は何もしない
    async fn discover(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let bg = self.client.background();
        let list = crate::crm::owners::fetch_all(&bg)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        let members = team_members(&list.owners, &self.cfg.teams);
        self.filtered = if self.cfg.scope_filter {
            members
                .iter()
                .filter(|m| !TalkReviewConfig::is_bpo_team(&m.team))
                .map(|m| m.id.clone())
                .collect()
        } else {
            HashSet::new()
        };
        let allowed: HashSet<String> = members.into_iter().map(|m| m.id).collect();
        if allowed.is_empty() {
            return Ok(());
        }
        let email_owner = owner_by_email(&list.owners);
        let users = self
            .zoom
            .active_user_emails()
            .await
            .map_err(|e| format!("zoom users: {e:?}"))?;
        let today = jst_day(now).0;
        let span = if self.backfilled {
            1
        } else {
            self.cfg.contact_backfill_days
        };
        let mut all_ok = true;
        // 新しい日から
        for back in 0..=span {
            let day = today - chrono::Duration::days(back);
            let (from, to) = utc_range(day);
            let pages = match self.zoom.call_log_pages(&from, &to).await {
                Ok(p) => p,
                Err(e) => {
                    tracing::warn!(error = ?e, %day, "contact marks: call log read failed");
                    all_ok = false;
                    continue;
                }
            };
            let fresh: Vec<contact::Candidate> =
                contact::discover(&pages, day, &users, &email_owner, &allowed)
                    .into_iter()
                    .filter(|c| !self.known.contains(&c.id))
                    .collect();
            if fresh.is_empty() {
                continue;
            }
            // BPO の発信は絞らずそのまま入れる。新規営業の発信は、アポ取りかの判定を待つ
            let (direct, waiting): (Vec<_>, Vec<_>) = fresh
                .into_iter()
                .partition(|c| !self.filtered.contains(&c.owner_id));
            for c in waiting {
                self.scope_wait.entry(c.id.clone()).or_insert(c);
            }
            if direct.is_empty() {
                continue;
            }
            if self.insert(&direct, false, now).await {
                tracing::info!(%day, new_rows = direct.len(), "contact marks ingest done");
            } else {
                all_ok = false;
            }
        }
        if all_ok {
            self.backfilled = true;
        }
        Ok(())
    }
}

impl<Z, J> ContactWorker<Z, J> {
    /// 行を表に入れる。入れたら `known` に足して true
    async fn insert(
        &mut self,
        rows: &[contact::Candidate],
        out_of_scope: bool,
        now: DateTime<Utc>,
    ) -> bool {
        let ts = store::ts_text(now);
        let owned = rows.to_vec();
        let res = store::blocking(&self.audit, move |t| {
            if out_of_scope {
                cstore::insert_out_of_scope(t, &ts, &owned)
            } else {
                cstore::insert_batch(t, &ts, &owned)
            }
        })
        .await;
        match res {
            Ok(()) => {
                self.known.extend(rows.iter().map(|c| c.id.clone()));
                for c in rows {
                    self.scope_wait.remove(&c.id);
                }
                true
            }
            Err(e) => {
                tracing::warn!(error = %e, "contact marks: insert failed");
                false
            }
        }
    }

    /// 待っている発信をアポ取りか調べ、対象は判定待ちとして、対象外は `out_of_scope` として表に入れる。
    /// 分からないものは待ち、上限を超えたら対象として入れる
    async fn settle_scope(&mut self, now: DateTime<Utc>) {
        if self.scope_wait.is_empty() {
            return;
        }
        let waiting: Vec<contact::Candidate> = self.scope_wait.values().cloned().collect();
        let mut qs = Vec::with_capacity(waiting.len());
        let mut ages = HashMap::new();
        for c in &waiting {
            let Ok(at) = DateTime::parse_from_rfc3339(&c.call_at) else {
                continue;
            };
            qs.push(ScopeQuery {
                id: c.id.clone(),
                callee9: c.callee9.clone(),
                at_ms: at.timestamp_millis(),
            });
            ages.insert(c.id.clone(), now - at.with_timezone(&Utc));
        }
        let verdicts = self.scope.resolve(&qs, now.timestamp_millis()).await;
        let p = scope::partition(&verdicts, &ages);
        let pick = |ids: &[String]| -> Vec<contact::Candidate> {
            ids.iter()
                .filter_map(|i| self.scope_wait.get(i).cloned())
                .collect()
        };
        let include = [pick(&p.include), pick(&p.timed_out)].concat();
        let exclude = pick(&p.exclude);
        // 日付の読めない発信は待ちから外れないので、対象として入れる
        let broken: Vec<contact::Candidate> = waiting
            .iter()
            .filter(|c| !ages.contains_key(&c.id))
            .cloned()
            .collect();
        let include = [include, broken].concat();
        if !include.is_empty() {
            self.insert(&include, false, now).await;
        }
        if !exclude.is_empty() {
            self.insert(&exclude, true, now).await;
        }
        tracing::info!(
            in_scope = p.include.len(),
            out_of_scope = p.exclude.len(),
            timed_out = p.timed_out.len(),
            waiting = p.waiting.len(),
            "contact marks scope settled"
        );
    }
}

async fn apply(audit: &crate::audit::AuditDb, id: String, u: Update) {
    let now = store::ts_text(Utc::now());
    let res = store::blocking(audit, move |t| match u {
        Update::Judged(r, attempts) => cstore::mark_judged(t, &id, attempts, &r, &now),
        Update::Retry { attempts, next } => {
            cstore::mark_retry(t, &id, attempts, &store::ts_text(next), &now)
        }
        Update::Final { status, attempts } => cstore::mark_final(t, &id, status, attempts, &now),
    })
    .await;
    if let Err(e) = res {
        // 書けなかった行は判定待ちのまま残り、次の周回でやり直す
        tracing::warn!(error = %e, "contact marks: result write failed");
    }
}

impl<Z: TranscriptSource + 'static, J: ContactJudge + 'static> ContactWorker<Z, J> {
    /// 判定待ちを新しい電話から進める。時間切れで残りがあれば true
    async fn process_due(&self) -> bool {
        let started = Instant::now();
        let mut done: HashSet<String> = HashSet::new();
        let batch_size = (self.cfg.contact_concurrency * BATCH_FACTOR).max(40);
        loop {
            if started.elapsed() >= self.cfg.interval {
                return true;
            }
            let now_s = store::ts_text(Utc::now());
            let due = match store::blocking(&self.audit, move |t| {
                cstore::due(t, &now_s, batch_size as i64)
            })
            .await
            {
                Ok(v) => v,
                Err(e) => {
                    tracing::warn!(error = %e, "contact marks: due select failed");
                    return false;
                }
            };
            let batch: Vec<cstore::DueMark> = due
                .into_iter()
                .filter(|r| done.insert(r.id.clone()))
                .collect();
            if batch.is_empty() {
                return false;
            }
            let sem = Arc::new(tokio::sync::Semaphore::new(self.cfg.contact_concurrency));
            let mut set = tokio::task::JoinSet::new();
            for row in batch {
                let (zoom, judge, audit, sem, pause) = (
                    self.zoom.clone(),
                    self.judge.clone(),
                    self.audit.clone(),
                    sem.clone(),
                    self.pause_until.clone(),
                );
                set.spawn(async move {
                    let Ok(_p) = sem.acquire().await else { return };
                    // 混雑のあとは、みんなで待つ
                    let wait = pause
                        .lock()
                        .unwrap()
                        .and_then(|t| t.checked_duration_since(Instant::now()));
                    if let Some(w) = wait {
                        tokio::time::sleep(w).await;
                    }
                    let now = Utc::now();
                    let outcome =
                        contact::process_one(&*zoom, &*judge, &row.id, &row.call_at, now).await;
                    if outcome == Outcome::Retry("judge_rate_limited") {
                        *pause.lock().unwrap() = Some(Instant::now() + RATE_LIMIT_PAUSE);
                    }
                    apply(&audit, row.id, contact::decide(outcome, row.attempts, now)).await;
                });
            }
            while set.join_next().await.is_some() {}
        }
    }
}

impl ContactWorker<ZoomClient, super::judge::MiniMaxJudge> {
    async fn run(mut self) {
        let since =
            store::ts_text(Utc::now() - chrono::Duration::days(self.cfg.contact_backfill_days + 2));
        match store::blocking(&self.audit, move |t| cstore::known_ids(t, &since)).await {
            Ok(k) => self.known = k,
            Err(e) => {
                tracing::warn!(error = %e, "contact marks: cannot read known ids; worker stops");
                return;
            }
        }
        loop {
            if let Err(e) = self.discover(Utc::now()).await {
                tracing::warn!(error = %e, "contact marks discover failed");
            }
            self.settle_scope(Utc::now()).await;
            if !self.process_due().await {
                tokio::time::sleep(self.cfg.interval).await;
            }
        }
    }
}

/// 起動 (呼び出し元が設定の有無を確かめたあと)
pub fn spawn(
    audit: crate::audit::AuditDb,
    client: Arc<HubSpotClient>,
    zoom: Arc<ZoomClient>,
    judge: Arc<super::judge::MiniMaxJudge>,
    cfg: TalkReviewConfig,
) {
    tracing::info!(
        days = cfg.contact_backfill_days,
        concurrency = cfg.contact_concurrency,
        "contact marks worker started"
    );
    let scope = Arc::new(scope::production_resolver(&client, &cfg.scope_stages));
    let w = ContactWorker {
        audit,
        client,
        zoom,
        judge,
        cfg,
        known: HashSet::new(),
        backfilled: false,
        pause_until: Arc::new(Mutex::new(None)),
        scope_wait: HashMap::new(),
        scope,
        filtered: HashSet::new(),
    };
    tokio::spawn(async move {
        // 9 項目の worker と同時に重い読み取りを始めない
        tokio::time::sleep(Duration::from_secs(90)).await;
        w.run().await;
    });
}
