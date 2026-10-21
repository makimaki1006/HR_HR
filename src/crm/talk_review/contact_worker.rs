//! 接触・日程切りの背景 worker。9 項目の判定 (`worker.rs`) とは別の周回で動き、互いに待たせない。
//!
//! 1 周: Zoom の通話ログから対象チームの「つながった発信」を新しい日から見つけて表に入れ (まとめ挿入)、
//! 判定待ちの行を新しい電話から、文字起こしを取って MiniMax で判定する。1 周で判定に使うのは `interval` まで
//! (超えたら、新しい電話を拾い直すために見つけ直しへ戻る)。
//! 見つけるのは、初回は過去 `CALL_CONTACT_BACKFILL_DAYS` 日、以後は今日と昨日だけ (通話ログの読み取りを増やさない)。
//! MiniMax が混雑 (429) を返したら、全体で少し待ってから続ける。

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use chrono::{DateTime, Utc};

use super::config::TalkReviewConfig;
use super::contact::{self, ContactJudge, Outcome, Update};
use super::contact_store as cstore;
use super::live_dials::{owner_by_email, utc_range};
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
}

impl ContactWorker<ZoomClient, super::judge::MiniMaxJudge> {
    /// 対象チームの人のつながった発信を見つけて表に入れる。担当者・Zoom ユーザーが読めない周回は何もしない
    async fn discover(&mut self, now: DateTime<Utc>) -> Result<(), String> {
        let bg = self.client.background();
        let list = crate::crm::owners::fetch_all(&bg)
            .await
            .map_err(|e| e.error_kind().to_string())?;
        let allowed: HashSet<String> = team_members(&list.owners, &self.cfg.teams)
            .into_iter()
            .map(|m| m.id)
            .collect();
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
            let ts = store::ts_text(now);
            let rows = fresh.clone();
            match store::blocking(&self.audit, move |t| cstore::insert_batch(t, &ts, &rows)).await {
                Ok(()) => {
                    tracing::info!(%day, new_rows = fresh.len(), "contact marks ingest done");
                    self.known.extend(fresh.into_iter().map(|c| c.id));
                }
                Err(e) => {
                    tracing::warn!(error = %e, "contact marks: insert failed");
                    all_ok = false;
                }
            }
        }
        if all_ok {
            self.backfilled = true;
        }
        Ok(())
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
    let w = ContactWorker {
        audit,
        client,
        zoom,
        judge,
        cfg,
        known: HashSet::new(),
        backfilled: false,
        pause_until: Arc::new(Mutex::new(None)),
    };
    tokio::spawn(async move {
        // 9 項目の worker と同時に重い読み取りを始めない
        tokio::time::sleep(Duration::from_secs(90)).await;
        w.run().await;
    });
}
