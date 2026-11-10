//! コンサルKPIが古いとき、既存の日次更新に再実行を頼む。
//! 単一インスタンス前提。状態はプロセス内のみで、再起動すると回数も失われる。

use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};
use reqwest::{header, Client, Request};
use serde::Serialize;
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
    count_date: Option<NaiveDate>,
    today_count: u8,
}

static STATE: Mutex<WatchState> = Mutex::new(WatchState {
    last_dispatch_at: None,
    last_result: None,
    count_date: None,
    today_count: 0,
});

fn jst_now() -> DateTime<FixedOffset> {
    Utc::now().with_timezone(&FixedOffset::east_opt(9 * 3600).unwrap())
}

/// 日付の出どころ・解釈は画面の data_age_days と同じ。日時不明なら推測しない。
/// now は JST。時間帯は [01:00, 22:00)。失敗も回数・間隔に含める。
fn should_dispatch(now: DateTime<FixedOffset>, as_of: Option<&str>, state: &WatchState) -> bool {
    let stale = as_of
        .and_then(super::date10)
        .is_some_and(|date| date < now.date_naive());
    stale
        && (1..22).contains(&now.hour())
        && (state.count_date != Some(now.date_naive()) || state.today_count < 4)
        && state
            .last_dispatch_at
            .is_none_or(|last| now.signed_duration_since(last) >= chrono::Duration::hours(2))
}

impl WatchState {
    fn attempted(&mut self, now: DateTime<FixedOffset>) {
        if self.count_date != Some(now.date_naive()) {
            self.count_date = Some(now.date_naive());
            self.today_count = 0;
        }
        self.today_count += 1;
        self.last_dispatch_at = Some(now);
        self.last_result = None;
    }

    fn add_meta(&self, meta: &mut serde_json::Map<String, Value>) {
        meta.insert("last_dispatch_at".into(), json!(self.last_dispatch_at));
        meta.insert("last_result".into(), json!(self.last_result));
    }
}

pub(super) fn add_meta(meta: &mut serde_json::Map<String, Value>) {
    STATE
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .add_meta(meta);
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
            let attempt = {
                let mut state = STATE.lock().unwrap_or_else(|e| e.into_inner());
                if should_dispatch(now, as_of.as_deref(), &state) {
                    state.attempted(now);
                    true
                } else {
                    false
                }
            };
            if !attempt {
                continue;
            }
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

    #[test]
    fn cooldown_failures_daily_limit_and_jst_rollover() {
        let old = Some("2026-11-08 23:59:59");
        let mut state = WatchState::default();
        state.attempted(at("2026-11-10T01:00:00+09:00"));
        state.last_result = Some(DispatchResult::Failure);
        assert!(!should_dispatch(
            at("2026-11-10T02:59:59+09:00"),
            old,
            &state
        ));
        assert!(should_dispatch(
            at("2026-11-10T03:00:00+09:00"),
            old,
            &state
        ));
        for hour in [3, 5, 7] {
            let now = at(&format!("2026-11-10T{hour:02}:00:00+09:00"));
            assert!(should_dispatch(now, old, &state));
            state.attempted(now);
        }
        assert_eq!(state.today_count, 4);
        assert!(!should_dispatch(
            at("2026-11-10T09:00:00+09:00"),
            old,
            &state
        ));
        // UTCではまだ前日だが、JSTの日付で上限をリセット。
        let tomorrow = at("2026-11-11T01:00:00+09:00");
        assert!(should_dispatch(tomorrow, old, &state));
        state.attempted(tomorrow);
        assert_eq!(state.today_count, 1);
        assert_eq!(state.count_date, Some(tomorrow.date_naive()));
        // 日が変わっても2時間の制限は残る。
        state.last_dispatch_at = Some(at("2026-11-10T23:30:00+09:00"));
        assert!(!should_dispatch(tomorrow, old, &state));
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
