//! コンサルKPIが古いとき、既存の日次更新に再実行を頼む。
//! 制限の根拠は GitHub の実行履歴。読めない場合は依頼しない。

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};
use reqwest::{header, Client, Request};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

const DEFAULT_REPO: &str = "makimaki1006/hubspot-sales-automation";
const API_BASE: &str = "https://api.github.com";
const CHECK_INTERVAL: Duration = Duration::from_secs(10 * 60);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum DispatchResult {
    Success,
    Failure,
}

/// 依頼の間隔と1日の上限。GitHub の実行履歴と、履歴に残らない失敗した依頼の両方で守る。
const DISPATCH_SPACING: chrono::Duration = chrono::Duration::hours(2);
const DAILY_LIMIT: usize = 4;

#[derive(Clone, Default)]
struct WatchState {
    last_dispatch_at: Option<DateTime<FixedOffset>>,
    last_result: Option<DispatchResult>,
    latest_run_state: Option<&'static str>,
    /// このプロセスが出した依頼（成功・失敗）。今日（JST）の分だけ残す。単一インスタンス前提。
    attempts: Vec<(DateTime<FixedOffset>, DispatchResult)>,
    /// 今日（JST）0時以降に始まり success で終わった実行を見た日。
    succeeded_on: Option<NaiveDate>,
    /// 直近の確認で実行履歴を読めなかったか。
    history_unreadable: bool,
}

static STATE: Mutex<WatchState> = Mutex::new(WatchState {
    last_dispatch_at: None,
    last_result: None,
    latest_run_state: None,
    attempts: Vec::new(),
    succeeded_on: None,
    history_unreadable: false,
});

/// 鍵があり見張りが動いているか。無いときは画面に自動の依頼のことを何も言わない。
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn jst_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap())
}

/// 環境変数の鍵。前後の空白・改行を落とす（残すとヘッダを作れず、依頼が一度も出ない）。
fn token_from(raw: Option<String>) -> Option<String> {
    raw.map(|v| v.trim().to_string()).filter(|v| !v.is_empty())
}

/// データが今日 0:00 JST より前か。日付の出どころ・解釈は画面の source_age_days と同じ。
/// 日時不明なら推測しない（古いとは扱わない）。
fn is_stale(now: DateTime<FixedOffset>, as_of: Option<&str>) -> bool {
    as_of
        .and_then(super::date10)
        .is_some_and(|date| date < now.date_naive())
}

/// 依頼してよい時間帯 [01:00, 22:00) JST。
fn in_window(now: DateTime<FixedOffset>) -> bool {
    (1..22).contains(&now.hour())
}

fn midnight(now: DateTime<FixedOffset>) -> Option<DateTime<FixedOffset>> {
    now.date_naive()
        .and_hms_opt(0, 0, 0)?
        .and_local_timezone(*now.offset())
        .single()
}

/// 今日（JST）0時以降に始まり success で終わった実行があるか。
/// ワークフローが「前日分が入っている」で何もせず終えた回も success なので、
/// HR_HR（CS_メタの取得時刻）とワークフロー（CS_日次状態の through）の基準のずれ、
/// HR_HR のシートのキャッシュ（1時間）の遅れの両方をここで吸収する。
fn succeeded_today(now: DateTime<FixedOffset>, history: &History) -> bool {
    let Some(midnight) = midnight(now) else {
        return false;
    };
    history.runs.iter().any(|run| {
        run.status == "completed"
            && run.conclusion.as_deref() == Some("success")
            && run.created_at.with_timezone(now.offset()) >= midnight
    })
}

/// 依頼するか。データが古い・時間帯の中・履歴を読めた・今日の成功が無い・
/// 履歴とこのプロセスの失敗を合わせて間隔2時間・1日4回の上限内、のすべてで頼む。
fn should_dispatch(
    now: DateTime<FixedOffset>,
    as_of: Option<&str>,
    history: Option<&History>,
    state: &WatchState,
) -> bool {
    let Some(history) = history else {
        return false;
    };
    is_stale(now, as_of)
        && in_window(now)
        && !succeeded_today(now, history)
        && state.local_allows(now)
        && history_allows_dispatch(now, history, state.attempts_today(now))
}

impl WatchState {
    fn attempts_today(&self, now: DateTime<FixedOffset>) -> (usize, usize) {
        let today = now.date_naive();
        let mut ok = 0;
        let mut failed = 0;
        for (at, result) in &self.attempts {
            if at.date_naive() == today {
                match result {
                    DispatchResult::Success => ok += 1,
                    DispatchResult::Failure => failed += 1,
                }
            }
        }
        (ok, failed)
    }

    /// 成功・失敗によらず、このプロセスの前回の依頼から2時間あける。
    fn local_allows(&self, now: DateTime<FixedOffset>) -> bool {
        self.attempts
            .iter()
            .all(|(at, _)| now.signed_duration_since(*at) >= DISPATCH_SPACING)
    }

    fn record(&mut self, now: DateTime<FixedOffset>, result: DispatchResult) {
        let today = now.date_naive();
        self.attempts.retain(|(at, _)| at.date_naive() == today);
        self.attempts.push((now, result));
        self.last_dispatch_at = Some(now);
        self.last_result = Some(result);
        self.latest_run_state = None;
    }

    fn add_meta(&self, meta: &mut serde_json::Map<String, Value>, enabled: bool) {
        meta.insert("auto_refresh".into(), json!(enabled));
        meta.insert("last_dispatch_at".into(), json!(self.last_dispatch_at));
        meta.insert("last_result".into(), json!(self.last_result));
        meta.insert("latest_run_state".into(), json!(self.latest_run_state));
        meta.insert(
            "auto_state".into(),
            json!(enabled.then(|| self.auto_state(jst_now()))),
        );
    }

    /// 画面の案内の種類。文言は画面（Ver1 の setFresh・Ver2 の Fresh）が同じ表で出す。
    /// 画面はデータが今日 0:00 JST より前（age が1以上）のときだけ出す。
    fn auto_state(&self, now: DateTime<FixedOffset>) -> &'static str {
        let today = now.date_naive();
        if self.history_unreadable {
            return "unknown"; // 自動の更新の状態を確かめられません
        }
        if self.latest_run_state == Some("running") {
            return "running"; // 自動でデータを更新しています
        }
        if self.succeeded_on == Some(today) {
            return "done"; // 更新は済んでいます。まもなく表示に反映されます
        }
        if self.latest_run_state == Some("failed") {
            return "failed"; // 自動の更新に失敗しました。次の更新を待っています
        }
        if now.hour() >= 22 {
            return "tomorrow"; // 自動の更新は明日1時以降に頼みます
        }
        if now.hour() < 1 {
            return "after_one"; // 自動の更新は1時以降に頼みます
        }
        match (
            self.last_dispatch_at.map(|at| at.date_naive()) == Some(today),
            self.last_result,
        ) {
            (true, Some(DispatchResult::Success)) => "requested", // 自動で更新を頼みました（HH:MM）
            (true, Some(DispatchResult::Failure)) => "request_failed", // 自動の更新を頼めていません
            _ => "pending",                                       // 自動の更新はこのあと頼みます
        }
    }
}

pub(super) fn add_meta(meta: &mut serde_json::Map<String, Value>) {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .add_meta(meta, ENABLED.load(std::sync::atomic::Ordering::Relaxed));
}

#[derive(Clone, Debug, Deserialize)]
struct WorkflowRun {
    event: String,
    created_at: DateTime<Utc>,
    status: String,
    conclusion: Option<String>,
}

#[derive(Deserialize)]
struct RunsPage {
    total_count: usize,
    workflow_runs: Vec<WorkflowRun>,
}

struct History {
    runs: Vec<WorkflowRun>,
    active: bool,
}

/// 履歴から見て頼めるか。`local` は（このプロセスで成功した依頼, 失敗した依頼）の今日の件数。
/// 成功した依頼は履歴に残るので、反映前の分を数え落とさないよう多い方を使い、
/// 履歴に残らない失敗は足す。
fn history_allows_dispatch(
    now: DateTime<FixedOffset>,
    history: &History,
    local: (usize, usize),
) -> bool {
    let dispatched_today = history
        .runs
        .iter()
        .filter(|run| {
            run.event == "workflow_dispatch"
                && run.created_at.with_timezone(now.offset()).date_naive() == now.date_naive()
        })
        .count();
    !history.active
        && !history.runs.iter().any(|run| {
            matches!(run.status.as_str(), "queued" | "in_progress")
                || (run.event == "workflow_dispatch"
                    && now.signed_duration_since(run.created_at) < DISPATCH_SPACING)
        })
        && dispatched_today.max(local.0) + local.1 < DAILY_LIMIT
}

fn history_request(
    client: &Client,
    repo: &str,
    token: &str,
    query: &[(&str, String)],
) -> Option<Request> {
    let mut auth = header::HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    auth.set_sensitive(true);
    client
        .get(format!(
            "{API_BASE}/repos/{repo}/actions/workflows/consulting_nightly.yml/runs"
        ))
        .header(header::AUTHORIZATION, auth)
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::USER_AGENT, "HR_HR-consulting-freshness-watchdog")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .query(query)
        .build()
        .ok()
}

/// 今日と直近2時間の両方を含める。実行中は開始日を限定せず別途調べる。
/// ページの欠落、上限超過、通信・JSONエラーはすべて依頼を止める。
async fn history_with<S, F>(
    client: &Client,
    repo: &str,
    token: &str,
    now: DateTime<FixedOffset>,
    mut send: S,
) -> Option<History>
where
    S: FnMut(Request) -> F,
    F: std::future::Future<Output = Option<Value>>,
{
    let since = midnight(now)?
        .min(now - chrono::Duration::hours(2))
        .with_timezone(&Utc);
    let mut runs = Vec::new();
    for page in 1..=10 {
        let query = [
            ("per_page", "100".into()),
            ("page", page.to_string()),
            ("created", format!(">={}", since.to_rfc3339())),
        ];
        let response: RunsPage =
            serde_json::from_value(send(history_request(client, repo, token, &query)?).await?)
                .ok()?;
        if response.workflow_runs.is_empty() && runs.len() < response.total_count {
            return None;
        }
        runs.extend(response.workflow_runs);
        if runs.len() >= response.total_count {
            break;
        }
        if page == 10 {
            return None;
        }
    }
    let mut active = false;
    for status in ["queued", "in_progress"] {
        let query = [("per_page", "1".into()), ("status", status.into())];
        let response: RunsPage =
            serde_json::from_value(send(history_request(client, repo, token, &query)?).await?)
                .ok()?;
        active |= response.total_count > 0;
        runs.extend(response.workflow_runs);
    }
    Some(History { runs, active })
}

async fn history(
    client: &Client,
    repo: &str,
    token: &str,
    now: DateTime<FixedOffset>,
) -> Option<History> {
    history_with(client, repo, token, now, |request| async {
        // 認証情報を含むRequest・エラー・本文はログに出さない。
        let response = client.execute(request).await.ok()?;
        if !response.status().is_success() {
            return None;
        }
        response.json().await.ok()
    })
    .await
}

impl WatchState {
    /// 実行履歴を読んだ結果を表示用の状態に写す。読めなければ「確かめられない」にする。
    fn observe(&mut self, history: Option<&History>, now: DateTime<FixedOffset>) {
        self.latest_run_state = None;
        let Some(history) = history else {
            self.history_unreadable = true;
            return;
        };
        self.history_unreadable = false;
        if succeeded_today(now, history) {
            self.succeeded_on = Some(now.date_naive());
        }
        self.latest_run_state = if history.active {
            Some("running")
        } else {
            history
                .runs
                .iter()
                .max_by_key(|run| run.created_at)
                .and_then(|run| {
                    if matches!(run.status.as_str(), "queued" | "in_progress") {
                        Some("running")
                    } else if run.conclusion.as_deref() == Some("failure") {
                        Some("failed")
                    } else {
                        None
                    }
                })
        };
        if let Some(run) = history
            .runs
            .iter()
            .filter(|run| run.event == "workflow_dispatch")
            .max_by_key(|run| run.created_at)
        {
            let at = run.created_at.with_timezone(now.offset());
            if self.last_dispatch_at.is_none_or(|last| at > last) {
                self.last_dispatch_at = Some(at);
                self.last_result = Some(DispatchResult::Success);
            }
        }
    }
}

fn dispatch_request(client: &Client, repo: &str, token: &str) -> Option<Request> {
    let mut auth = header::HeaderValue::from_str(&format!("Bearer {token}")).ok()?;
    auth.set_sensitive(true);
    client
        .post(format!(
            "{API_BASE}/repos/{repo}/actions/workflows/consulting_nightly.yml/dispatches"
        ))
        .header(header::AUTHORIZATION, auth)
        .header(header::ACCEPT, "application/vnd.github+json")
        .header(header::USER_AGENT, "HR_HR-consulting-freshness-watchdog")
        .header("X-GitHub-Api-Version", "2022-11-28")
        .json(&json!({"ref": "master", "inputs": {"mode": "nightly", "apply": "true"}}))
        .build()
        .ok()
}

async fn dispatch_with<S, F>(request: Option<Request>, send: S) -> DispatchResult
where
    S: FnOnce(Request) -> F,
    F: std::future::Future<Output = Option<reqwest::StatusCode>>,
{
    let Some(request) = request else {
        return DispatchResult::Failure;
    };
    match send(request).await {
        Some(reqwest::StatusCode::NO_CONTENT) => DispatchResult::Success,
        _ => DispatchResult::Failure,
    }
}

async fn dispatch(client: &Client, repo: &str, token: &str) -> DispatchResult {
    // reqwest のエラー・応答本文・Request はログに渡さない（認証情報を保護）。
    dispatch_with(dispatch_request(client, repo, token), |request| async {
        client
            .execute(request)
            .await
            .ok()
            .map(|response| response.status())
    })
    .await
}

pub fn spawn() {
    let Some(token) = token_from(std::env::var("CONSULTING_NIGHTLY_DISPATCH_TOKEN").ok()) else {
        tracing::info!("コンサル鮮度見張り: 無効（CONSULTING_NIGHTLY_DISPATCH_TOKEN 未設定）");
        return;
    };
    let repo = std::env::var("CONSULTING_NIGHTLY_REPO")
        .ok()
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| DEFAULT_REPO.into());
    let client = match Client::builder()
        .timeout(Duration::from_secs(30))
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(client) => client,
        Err(_) => {
            tracing::warn!("コンサル鮮度見張り: HTTPクライアントを初期化できません");
            return;
        }
    };
    ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(CHECK_INTERVAL);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let Ok(sheets) = crate::handlers::call_quality::routes::cq_state() else {
                continue;
            };
            // 同じ SheetStore を通し、TTL内は読み取りゼロ。強制更新も全シートのloadもしない。
            let Ok((meta, _)) = sheets.store.get(&sheets.client, super::SHEET_META).await else {
                tracing::warn!("コンサル鮮度見張り: データの日時を読めません");
                continue;
            };
            let as_of = super::data_as_of(&meta);
            let now = jst_now();
            // データが古い間は時間帯の外でも実行の状態を読み直す（表示のため。頼むのは時間帯の中だけ）。
            if !is_stale(now, as_of.as_deref()) {
                continue;
            }
            let history = history(&client, &repo, &token, now).await;
            if history.is_none() {
                tracing::warn!("コンサル鮮度見張り: 実行履歴を確認できないため依頼を見送ります");
            }
            let go = {
                let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
                state.observe(history.as_ref(), now);
                should_dispatch(now, as_of.as_deref(), history.as_ref(), &state)
            };
            if !go {
                continue;
            }
            let result = dispatch(&client, &repo, &token).await;
            STATE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .record(now, result);
            match result {
                DispatchResult::Success => {
                    tracing::info!("コンサル鮮度見張り: 更新依頼を受け付けました")
                }
                DispatchResult::Failure => {
                    tracing::warn!(
                        "コンサル鮮度見張り: 更新を依頼できませんでした（2時間あけて再試行、1日4回まで）"
                    )
                }
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(s: &str) -> DateTime<FixedOffset> {
        DateTime::parse_from_rfc3339(s).unwrap()
    }

    fn run(event: &str, time: &str, status: &str) -> WorkflowRun {
        WorkflowRun {
            event: event.into(),
            created_at: at(time).with_timezone(&Utc),
            status: status.into(),
            conclusion: (status == "completed").then(|| "success".into()),
        }
    }

    fn empty() -> History {
        History {
            runs: vec![],
            active: false,
        }
    }

    const OLD: Option<&str> = Some("2026-11-09 23:59:59");

    #[test]
    fn freshness_and_window_boundaries() {
        let state = WatchState::default();
        let h = empty();
        for (time, expected) in [
            ("2026-11-10T00:59:59+09:00", false),
            ("2026-11-10T01:00:00+09:00", true),
            ("2026-11-10T21:59:59+09:00", true),
            ("2026-11-10T22:00:00+09:00", false),
        ] {
            assert_eq!(
                should_dispatch(at(time), OLD, Some(&h), &state),
                expected,
                "{time}"
            );
        }
        let now = at("2026-11-10T01:00:00+09:00");
        for value in [
            None,
            Some(""),
            Some("unknown"),
            Some("2026-11-10 00:00:00"),
            Some("2026-11-11 00:00:00"),
        ] {
            assert!(!should_dispatch(now, value, Some(&h), &state), "{value:?}");
        }
        // 履歴を読めなければ頼まない。
        assert!(!should_dispatch(now, OLD, None, &state));
    }

    #[test]
    fn success_since_jst_midnight_stops_dispatch() {
        let now = at("2026-11-10T07:00:00+09:00");
        let state = WatchState::default();
        // 前日分が入っていて何もせず終えた回も success。今日 0:00 以降なら頼まない。
        let mut h = empty();
        h.runs = vec![run("schedule", "2026-11-10T00:00:00+09:00", "completed")];
        assert!(succeeded_today(now, &h));
        assert!(!should_dispatch(now, OLD, Some(&h), &state));
        // 前日 23:59:59 JST の成功は今日の成功ではない（UTC では同じ日付）。
        h.runs[0].created_at = at("2026-11-09T23:59:59+09:00").with_timezone(&Utc);
        assert!(!succeeded_today(now, &h));
        assert!(should_dispatch(now, OLD, Some(&h), &state));
        // 今日でも失敗・実行中は成功に数えない。
        h.runs = vec![run("schedule", "2026-11-10T05:00:00+09:00", "completed")];
        h.runs[0].conclusion = Some("failure".into());
        assert!(!succeeded_today(now, &h));
        assert!(should_dispatch(now, OLD, Some(&h), &state));
        h.runs = vec![run("schedule", "2026-11-10T05:00:00+09:00", "in_progress")];
        assert!(!succeeded_today(now, &h));
        // 成功を見たら表示は「済んでいます」。
        let mut state = WatchState::default();
        h.runs = vec![run(
            "workflow_dispatch",
            "2026-11-10T01:00:00+09:00",
            "completed",
        )];
        state.observe(Some(&h), now);
        assert_eq!(state.auto_state(now), "done");
        // 翌日になれば前日の成功は使わない。
        assert_eq!(state.auto_state(at("2026-11-11T07:00:00+09:00")), "pending");
    }

    #[test]
    fn history_limits_and_jst_boundaries() {
        let now = at("2026-11-10T10:00:00+09:00");
        let mut h = empty();
        assert!(history_allows_dispatch(now, &h, (0, 0)));
        h.runs.push(run(
            "workflow_dispatch",
            "2026-11-10T08:00:01+09:00",
            "completed",
        ));
        assert!(!history_allows_dispatch(now, &h, (0, 0)));
        h.runs[0].created_at = at("2026-11-10T08:00:00+09:00").with_timezone(&Utc);
        assert!(history_allows_dispatch(now, &h, (0, 0)));
        h.runs = ["00", "02", "04", "06"]
            .iter()
            .map(|hour| {
                run(
                    "workflow_dispatch",
                    &format!("2026-11-10T{hour}:00:00+09:00"),
                    "completed",
                )
            })
            .collect();
        assert!(!history_allows_dispatch(now, &h, (0, 0)));
        // JST前日の実行を今日の回数には含めない（UTCの日付は同じ）。
        h.runs[0].created_at = at("2026-11-09T23:59:59+09:00").with_timezone(&Utc);
        assert!(history_allows_dispatch(now, &h, (0, 0)));
        // 履歴3回 + 失敗した依頼1回で4回。
        assert!(!history_allows_dispatch(now, &h, (0, 1)));
        // このプロセスで成功した依頼は履歴と重なるので多い方を使う。
        assert!(history_allows_dispatch(now, &h, (3, 0)));
        for status in ["queued", "in_progress"] {
            h.runs = vec![run("schedule", "2026-11-09T01:00:00+09:00", status)];
            assert!(!history_allows_dispatch(now, &h, (0, 0)));
        }
        h.runs = vec![run("schedule", "2026-11-10T09:59:00+09:00", "completed")];
        assert!(history_allows_dispatch(now, &h, (0, 0)));
        h.active = true;
        assert!(!history_allows_dispatch(now, &h, (0, 0)));
    }

    #[test]
    fn failed_dispatch_waits_two_hours_and_counts_toward_four() {
        let h = empty();
        let mut state = WatchState::default();
        state.record(at("2026-11-10T01:00:00+09:00"), DispatchResult::Failure);
        // 失敗しても10分後には頼まない。2時間あける。
        for (time, expected) in [
            ("2026-11-10T01:10:00+09:00", false),
            ("2026-11-10T02:59:59+09:00", false),
            ("2026-11-10T03:00:00+09:00", true),
        ] {
            assert_eq!(
                should_dispatch(at(time), OLD, Some(&h), &state),
                expected,
                "{time}"
            );
        }
        for time in [
            "2026-11-10T03:00:00+09:00",
            "2026-11-10T05:00:00+09:00",
            "2026-11-10T07:00:00+09:00",
        ] {
            state.record(at(time), DispatchResult::Failure);
        }
        // 失敗を含めて4回で、その日はもう頼まない。
        assert_eq!(
            state.attempts_today(at("2026-11-10T09:00:00+09:00")),
            (0, 4)
        );
        assert!(!should_dispatch(
            at("2026-11-10T09:00:00+09:00"),
            OLD,
            Some(&h),
            &state
        ));
        assert!(!should_dispatch(
            at("2026-11-10T21:00:00+09:00"),
            OLD,
            Some(&h),
            &state
        ));
        // 翌日は数え直す（前日 23 時の依頼から2時間たった 1 時から）。
        let mut state = WatchState::default();
        state.record(at("2026-11-10T21:00:00+09:00"), DispatchResult::Failure);
        assert!(should_dispatch(
            at("2026-11-11T01:00:00+09:00"),
            Some("2026-11-10 21:00:00"),
            Some(&h),
            &state
        ));
        state.record(at("2026-11-11T01:00:00+09:00"), DispatchResult::Failure);
        assert_eq!(state.attempts.len(), 1, "前日の依頼は捨てる");
        assert_eq!(
            state.auto_state(at("2026-11-11T01:00:00+09:00")),
            "request_failed"
        );
    }

    #[test]
    fn token_is_trimmed_and_builds_headers() {
        assert_eq!(token_from(None), None);
        assert_eq!(token_from(Some(" \n\t".into())), None);
        let token = token_from(Some("  test-secret\r\n".into())).unwrap();
        assert_eq!(token, "test-secret");
        let client = Client::new();
        let request = dispatch_request(&client, DEFAULT_REPO, &token).unwrap();
        assert_eq!(request.headers()["authorization"], "Bearer test-secret");
        assert!(history_request(&client, DEFAULT_REPO, &token, &[]).is_some());
        // trim しない値ではヘッダを作れない（以前は依頼が一度も出なかった）。
        assert!(dispatch_request(&client, DEFAULT_REPO, "test-secret\n").is_none());
    }

    #[test]
    fn status_outside_window_follows_history() {
        let mut state = WatchState::default();
        let night = at("2026-11-10T22:10:00+09:00");
        // 22時に実行中だったものが、時間帯の外の読み直しで失敗に変わる。
        let mut h = empty();
        h.runs = vec![run(
            "workflow_dispatch",
            "2026-11-10T21:00:00+09:00",
            "in_progress",
        )];
        state.observe(Some(&h), at("2026-11-10T21:50:00+09:00"));
        assert_eq!(state.auto_state(night), "running");
        h.runs[0].status = "completed".into();
        h.runs[0].conclusion = Some("failure".into());
        state.observe(Some(&h), night);
        assert_eq!(state.auto_state(night), "failed");
        // 時間帯の外では頼まない。
        assert!(!should_dispatch(
            night,
            OLD,
            Some(&empty()),
            &WatchState::default()
        ));
        // 失敗も実行中も無ければ、いつ頼むかを言う。
        let mut state = WatchState::default();
        state.observe(Some(&empty()), night);
        assert_eq!(state.auto_state(night), "tomorrow");
        assert_eq!(
            state.auto_state(at("2026-11-10T00:30:00+09:00")),
            "after_one"
        );
        assert_eq!(state.auto_state(at("2026-11-10T01:00:00+09:00")), "pending");
        // 読めなくなったら「確かめられない」、読めたら戻す。
        state.observe(None, night);
        assert_eq!(state.auto_state(night), "unknown");
        assert_eq!(state.latest_run_state, None);
        state.observe(Some(&empty()), night);
        assert_eq!(state.auto_state(night), "tomorrow");
    }

    #[test]
    fn auto_state_meta_and_texts() {
        let now = at("2026-11-10T10:00:00+09:00");
        let mut state = WatchState::default();
        state.record(at("2026-11-10T03:27:18+09:00"), DispatchResult::Success);
        assert_eq!(state.auto_state(now), "requested");
        // 前日の依頼は今日の依頼ではない。
        assert_eq!(state.auto_state(at("2026-11-11T10:00:00+09:00")), "pending");
        state.record(at("2026-11-10T05:30:00+09:00"), DispatchResult::Failure);
        assert_eq!(state.auto_state(now), "request_failed");
        let mut meta = serde_json::Map::new();
        state.add_meta(&mut meta, false);
        assert_eq!(meta["auto_refresh"], json!(false));
        assert_eq!(
            meta["auto_state"],
            Value::Null,
            "見張りが動いていなければ出さない"
        );
        state.add_meta(&mut meta, true);
        assert!(meta["auto_state"].is_string());
    }

    #[tokio::test]
    async fn history_http_contract_pagination_and_fail_closed() {
        let client = Client::new();
        let now = at("2026-11-10T01:00:00+09:00");
        let mut calls = 0;
        let h = history_with(&client, DEFAULT_REPO, "test-secret", now, |request| {
            calls += 1;
            assert_eq!(request.method(), "GET");
            assert!(request.url().path().ends_with("/consulting_nightly.yml/runs"));
            assert_eq!(request.headers()["authorization"], "Bearer test-secret");
            assert!(!format!("{request:?}").contains("test-secret"));
            let query: std::collections::HashMap<_, _> = request.url().query_pairs().into_owned().collect();
            let body = if calls <= 2 {
                assert_eq!(query["created"], ">=2026-11-09T14:00:00+00:00");
                assert_eq!(query["page"], calls.to_string());
                json!({"total_count": 2, "workflow_runs": [{"event":"workflow_dispatch", "created_at":"2026-11-09T15:30:00Z", "status":"completed", "conclusion":"failure"}]})
            } else {
                assert_eq!(query["status"], if calls == 3 { "queued" } else { "in_progress" });
                json!({"total_count":0,"workflow_runs":[]})
            };
            async move { Some(body) }
        }).await.unwrap();
        assert_eq!(calls, 4);
        assert!(!history_allows_dispatch(now, &h, (0, 0)));
        let mut state = WatchState::default();
        state.observe(Some(&h), now);
        assert_eq!(state.latest_run_state, Some("failed"));
        assert_eq!(
            state.last_dispatch_at,
            Some(at("2026-11-10T00:30:00+09:00"))
        );
        for body in [
            None,
            Some(json!({})),
            Some(json!({"total_count":1,"workflow_runs":[]})),
            Some(json!({"total_count":1,"workflow_runs":[{"created_at":"bad"}]})),
        ] {
            let h = history_with(&client, DEFAULT_REPO, "test-secret", now, |_| {
                let body = body.clone();
                async move { body }
            })
            .await;
            assert!(h.is_none());
        }
    }

    #[tokio::test]
    async fn incomplete_history_never_allows_dispatch() {
        let client = Client::new();
        let now = at("2026-11-10T10:00:00+09:00");
        // 通常の履歴が空でも、実行中の照会を読めなければ呼ばない。
        for fail_at in [2, 3] {
            let mut calls = 0;
            let h = history_with(&client, DEFAULT_REPO, "test-secret", now, |_| {
                calls += 1;
                let value = (calls != fail_at).then(|| json!({"total_count":0,"workflow_runs":[]}));
                async move { value }
            })
            .await;
            assert!(h.is_none());
            assert_eq!(calls, fail_at);
        }
        // 多すぎて確認しきれない履歴も、途中までの集計で許可しない。
        let mut calls = 0;
        let h = history_with(&client, DEFAULT_REPO, "test-secret", now, |_| {
            calls += 1;
            async { Some(json!({"total_count":1001,"workflow_runs":[{"event":"schedule","created_at":"2026-11-09T15:00:00Z","status":"completed","conclusion":"success"}]})) }
        }).await;
        assert_eq!(calls, 10);
        assert!(h.is_none());
        let h = history_with(&client, DEFAULT_REPO, "bad\nsecret", now, |_| async {
            panic!("不正なヘッダで送らない")
        })
        .await;
        assert!(h.is_none());
    }

    #[tokio::test]
    async fn http_contract_and_safe_results() {
        let client = Client::builder().no_proxy().build().unwrap();
        let token = "test-secret";
        for (status, expected) in [
            (
                Some(reqwest::StatusCode::NO_CONTENT),
                DispatchResult::Success,
            ),
            (
                Some(reqwest::StatusCode::UNAUTHORIZED),
                DispatchResult::Failure,
            ),
            (
                Some(reqwest::StatusCode::FORBIDDEN),
                DispatchResult::Failure,
            ),
            (
                Some(reqwest::StatusCode::TOO_MANY_REQUESTS),
                DispatchResult::Failure,
            ),
            (
                Some(reqwest::StatusCode::INTERNAL_SERVER_ERROR),
                DispatchResult::Failure,
            ),
            (Some(reqwest::StatusCode::FOUND), DispatchResult::Failure),
            (None, DispatchResult::Failure), // 接続失敗・タイムアウト
        ] {
            // 実際に送る Request をモックの通信層で検査する。TCP待受の権限は不要。
            let result = dispatch_with(
                dispatch_request(&client, DEFAULT_REPO, token),
                |request| async move {
                    assert_eq!(request.method(), "POST");
                    assert_eq!(request.url().as_str(), "https://api.github.com/repos/makimaki1006/hubspot-sales-automation/actions/workflows/consulting_nightly.yml/dispatches");
                    let headers = request.headers();
                    assert_eq!(headers["authorization"], format!("Bearer {token}"));
                    assert_eq!(headers["accept"], "application/vnd.github+json");
                    assert_eq!(headers["user-agent"], "HR_HR-consulting-freshness-watchdog");
                    assert_eq!(headers["x-github-api-version"], "2022-11-28");
                    assert_eq!(headers["content-type"], "application/json");
                    assert_eq!(serde_json::from_slice::<Value>(request.body().unwrap().as_bytes().unwrap()).unwrap(), json!({"ref":"master","inputs":{"mode":"nightly","apply":"true"}}));
                    assert!(!format!("{request:?}").contains(token));
                    status
                },
            ).await;
            assert_eq!(result, expected);
            let mut state = WatchState::default();
            state.record(at("2026-11-10T01:20:00+09:00"), result);
            let mut meta = serde_json::Map::new();
            state.add_meta(&mut meta, true);
            assert_eq!(meta["last_dispatch_at"], "2026-11-10T01:20:00+09:00");
            assert_eq!(meta["last_result"], json!(expected));
            assert!(!json!(meta).to_string().contains(token));
        }
        let custom = dispatch_request(&client, "another/repo", token).unwrap();
        assert_eq!(
            custom.url().path(),
            "/repos/another/repo/actions/workflows/consulting_nightly.yml/dispatches"
        );
        assert_eq!(
            dispatch_with(
                dispatch_request(&client, DEFAULT_REPO, "bad\nsecret"),
                |_| async { panic!("不正な認証ヘッダでは送信しない") }
            )
            .await,
            DispatchResult::Failure
        );
    }
}
