//! コンサルKPIが古いとき、既存の日次更新に再実行を頼む。
//! 制限の根拠は GitHub の実行履歴。読めない場合は依頼しない。

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, Timelike, Utc};
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

#[derive(Clone, Default)]
struct WatchState {
    last_dispatch_at: Option<DateTime<FixedOffset>>,
    last_result: Option<DispatchResult>,
    latest_run_state: Option<&'static str>,
}

static STATE: Mutex<WatchState> = Mutex::new(WatchState {
    last_dispatch_at: None,
    last_result: None,
    latest_run_state: None,
});

/// 鍵があり見張りが動いているか。無いときは画面に自動の依頼のことを何も言わない。
static ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

fn jst_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap())
}

/// 日付の出どころ・解釈は画面の data_age_days と同じ。日時不明なら推測しない。
/// now は JST。時間帯は [01:00, 22:00)。ローカルには10分の間隔だけ残す。
fn should_dispatch(now: DateTime<FixedOffset>, as_of: Option<&str>, state: &WatchState) -> bool {
    let stale = as_of
        .and_then(super::date10)
        .is_some_and(|date| date < now.date_naive());
    stale
        && (1..22).contains(&now.hour())
        && state
            .last_dispatch_at
            .is_none_or(|last| now.signed_duration_since(last) >= chrono::Duration::minutes(10))
}

impl WatchState {
    fn attempted(&mut self, now: DateTime<FixedOffset>) {
        self.last_dispatch_at = Some(now);
        self.last_result = None;
        self.latest_run_state = None;
    }

    fn add_meta(&self, meta: &mut serde_json::Map<String, Value>) {
        meta.insert(
            "auto_refresh".into(),
            json!(ENABLED.load(std::sync::atomic::Ordering::Relaxed)),
        );
        meta.insert("last_dispatch_at".into(), json!(self.last_dispatch_at));
        meta.insert("last_result".into(), json!(self.last_result));
        meta.insert("latest_run_state".into(), json!(self.latest_run_state));
    }
}

pub(super) fn add_meta(meta: &mut serde_json::Map<String, Value>) {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .add_meta(meta);
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

fn history_allows_dispatch(now: DateTime<FixedOffset>, history: Option<&History>) -> bool {
    let Some(history) = history else {
        return false;
    };
    !history.active
        && !history.runs.iter().any(|run| {
            matches!(run.status.as_str(), "queued" | "in_progress")
                || (run.event == "workflow_dispatch"
                    && now.signed_duration_since(run.created_at) < chrono::Duration::hours(2))
        })
        && history
            .runs
            .iter()
            .filter(|run| {
                run.event == "workflow_dispatch"
                    && run.created_at.with_timezone(now.offset()).date_naive() == now.date_naive()
            })
            .count()
            < 4
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
    let midnight = now
        .date_naive()
        .and_hms_opt(0, 0, 0)?
        .and_local_timezone(*now.offset())
        .single()?;
    let since = midnight
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
    fn observe(&mut self, history: &History, now: DateTime<FixedOffset>) {
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
    let token = std::env::var("CONSULTING_NIGHTLY_DISPATCH_TOKEN").unwrap_or_default();
    if token.trim().is_empty() {
        tracing::info!("コンサル鮮度見張り: 無効（CONSULTING_NIGHTLY_DISPATCH_TOKEN 未設定）");
        return;
    }
    ENABLED.store(true, std::sync::atomic::Ordering::Relaxed);
    let repo = std::env::var("CONSULTING_NIGHTLY_REPO")
        .ok()
        .filter(|v| !v.trim().is_empty())
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
            if !should_dispatch(
                now,
                as_of.as_deref(),
                &STATE.lock().unwrap_or_else(|e| e.into_inner()),
            ) {
                continue;
            }
            let history = history(&client, &repo, &token, now).await;
            {
                let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
                state.latest_run_state = None;
                if let Some(history) = &history {
                    state.observe(history, now);
                }
            }
            if !history_allows_dispatch(now, history.as_ref()) {
                if history.is_none() {
                    tracing::warn!(
                        "コンサル鮮度見張り: 実行履歴を確認できないため依頼を見送ります"
                    );
                }
                continue;
            }
            STATE
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .attempted(now);
            let result = dispatch(&client, &repo, &token).await;
            STATE.lock().unwrap_or_else(|e| e.into_inner()).last_result = Some(result);
            match result {
                DispatchResult::Success => {
                    tracing::info!("コンサル鮮度見張り: 更新依頼を受け付けました")
                }
                DispatchResult::Failure => {
                    tracing::warn!("コンサル鮮度見張り: 更新を依頼できませんでした")
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

    #[test]
    fn freshness_and_window_boundaries() {
        let state = WatchState::default();
        let old = Some("2026-11-09 23:59:59");
        for (time, expected) in [
            ("2026-11-10T00:59:59+09:00", false),
            ("2026-11-10T01:00:00+09:00", true),
            ("2026-11-10T21:59:59+09:00", true),
            ("2026-11-10T22:00:00+09:00", false),
        ] {
            assert_eq!(should_dispatch(at(time), old, &state), expected, "{time}");
        }
        let now = at("2026-11-10T01:00:00+09:00");
        for value in [
            None,
            Some(""),
            Some("unknown"),
            Some("2026-11-10 00:00:00"),
            Some("2026-11-11 00:00:00"),
        ] {
            assert!(!should_dispatch(now, value, &state), "{value:?}");
        }
    }

    fn run(event: &str, time: &str, status: &str) -> WorkflowRun {
        WorkflowRun {
            event: event.into(),
            created_at: at(time).with_timezone(&Utc),
            status: status.into(),
            conclusion: Some("success".into()),
        }
    }

    #[test]
    fn history_limits_and_jst_boundaries() {
        let now = at("2026-11-10T10:00:00+09:00");
        assert!(!history_allows_dispatch(now, None));
        let mut h = History {
            runs: vec![],
            active: false,
        };
        assert!(history_allows_dispatch(now, Some(&h)));
        h.runs.push(run(
            "workflow_dispatch",
            "2026-11-10T08:00:01+09:00",
            "completed",
        ));
        assert!(!history_allows_dispatch(now, Some(&h)));
        h.runs[0].created_at = at("2026-11-10T08:00:00+09:00").with_timezone(&Utc);
        assert!(history_allows_dispatch(now, Some(&h)));
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
        assert!(!history_allows_dispatch(now, Some(&h)));
        // JST前日の実行を今日の回数には含めない（UTCの日付は同じ）。
        h.runs[0].created_at = at("2026-11-09T23:59:59+09:00").with_timezone(&Utc);
        assert!(history_allows_dispatch(now, Some(&h)));
        for status in ["queued", "in_progress"] {
            h.runs = vec![run("schedule", "2026-11-09T01:00:00+09:00", status)];
            assert!(!history_allows_dispatch(now, Some(&h)));
        }
        h.runs = vec![run("schedule", "2026-11-10T09:59:00+09:00", "completed")];
        assert!(history_allows_dispatch(now, Some(&h)));
        h.active = true;
        assert!(!history_allows_dispatch(now, Some(&h)));
    }

    #[test]
    fn local_attempt_spacing_is_ten_minutes() {
        let mut state = WatchState::default();
        state.attempted(at("2026-11-10T01:00:00+09:00"));
        assert!(!should_dispatch(
            at("2026-11-10T01:09:59+09:00"),
            Some("2026-11-08"),
            &state
        ));
        assert!(should_dispatch(
            at("2026-11-10T01:10:00+09:00"),
            Some("2026-11-08"),
            &state
        ));
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
        assert!(!history_allows_dispatch(now, Some(&h)));
        let mut state = WatchState::default();
        state.observe(&h, now);
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
            assert!(!history_allows_dispatch(now, h.as_ref()));
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
            assert!(!history_allows_dispatch(now, h.as_ref()));
            assert_eq!(calls, fail_at);
        }
        // 多すぎて確認しきれない履歴も、途中までの集計で許可しない。
        let mut calls = 0;
        let h = history_with(&client, DEFAULT_REPO, "test-secret", now, |_| {
            calls += 1;
            async { Some(json!({"total_count":1001,"workflow_runs":[{"event":"schedule","created_at":"2026-11-09T15:00:00Z","status":"completed","conclusion":"success"}]})) }
        }).await;
        assert_eq!(calls, 10);
        assert!(!history_allows_dispatch(now, h.as_ref()));
        let h = history_with(&client, DEFAULT_REPO, "bad\nsecret", now, |_| async {
            panic!("不正なヘッダで送らない")
        })
        .await;
        assert!(!history_allows_dispatch(now, h.as_ref()));
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
            state.attempted(at("2026-11-10T01:20:00+09:00"));
            state.last_result = Some(result);
            let mut meta = serde_json::Map::new();
            state.add_meta(&mut meta);
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
