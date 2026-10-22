//! 今日の架電数を Zoom の通話ログから直接数える (約 15 分おき)。数え方は営業KPI の日次同期
//! (`fetch_outbound_by_user`) と同じ: 発信 (outbound) のうち `result == "Auto Recorded"` の件数を、
//! Zoom ユーザー → メール → HubSpot の担当者で人に振り分ける。
//! 今日より前の日はシートの値を使う (`dials.rs`)。ここは今日の分だけ。

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, Utc};
use serde_json::Value;

use super::dial_breakdown::classify;
use super::types::DialBreakdown;
use super::zoom::ZoomClient;
use crate::crm::owners::CrmOwner;

/// 数え直す間隔
pub const LIVE_TTL: Duration = Duration::from_secs(15 * 60);
/// 数え直しに失敗したあと、次に試すまで
const RETRY_AFTER: Duration = Duration::from_secs(60);
/// 値が無いときに 1 回目の数え終わりを待つ上限
const FIRST_WAIT: Duration = Duration::from_secs(20);
/// Zoom ユーザー一覧を持っておく時間
const USERS_TTL: Duration = Duration::from_secs(3600);

/// 今日の人ごとの架電数と、数え終えた時刻 (JST `yyyy-MM-dd HH:mm`)
#[derive(Debug, Clone, PartialEq)]
pub struct LiveToday {
    /// 架電数 (つながった通話)
    pub by_owner: HashMap<String, u32>,
    /// 発信数 (切れた・つながらなかったものを含む発信すべて)
    pub calls_by_owner: HashMap<String, u32>,
    /// 発信の内訳 (Zoom の記録から直接数えたもの)
    pub breakdown_by_owner: HashMap<String, DialBreakdown>,
    pub fetched_at: String,
}

/// JST の `day` の 0:00:00 から 23:59:59 を UTC の `YYYY-MM-DDTHH:MM:SSZ` で
pub fn utc_range(day: NaiveDate) -> (String, String) {
    let start = day.and_hms_opt(0, 0, 0).expect("midnight").and_utc() - ChronoDuration::hours(9);
    let end = start + ChronoDuration::days(1) - ChronoDuration::seconds(1);
    let f = "%Y-%m-%dT%H:%M:%SZ";
    (start.format(f).to_string(), end.format(f).to_string())
}

/// メール (小文字) → HubSpot の担当者 ID。同じメールが有効と退職の両方にあれば有効を勝たせる
pub fn owner_by_email(owners: &[CrmOwner]) -> HashMap<String, String> {
    let mut m: HashMap<String, String> = HashMap::new();
    for archived in [false, true] {
        for o in owners.iter().filter(|o| o.archived == archived) {
            if let Some(e) = o
                .email
                .as_deref()
                .map(str::to_lowercase)
                .filter(|e| !e.is_empty())
            {
                m.entry(e).or_insert_with(|| o.id.clone());
            }
        }
    }
    m
}

/// 人ごとの発信数と架電数
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Counts {
    pub calls: HashMap<String, u32>,
    pub connected: HashMap<String, u32>,
    pub breakdown: HashMap<String, DialBreakdown>,
}

/// 通話ログのページから、JST の `day` の人ごとの発信数と架電数。
/// 発信だけ・`id`/`call_id` で重複除外・JST の日に入るものだけ・`user_id` 必須。発信数はそのすべて、
/// 架電数は `Auto Recorded` だけ。Zoom ユーザーが有効でない/担当者に結び付かない人は数えない。
pub fn count_calls(
    pages: &[Value],
    day: NaiveDate,
    user_email: &HashMap<String, String>,
    email_owner: &HashMap<String, String>,
) -> Counts {
    let start = day.and_hms_opt(0, 0, 0).expect("midnight").and_utc() - ChronoDuration::hours(9);
    let end = start + ChronoDuration::days(1);
    let mut seen = std::collections::HashSet::new();
    let mut per_user: HashMap<String, (u32, u32, DialBreakdown)> = HashMap::new();
    for page in pages {
        let Some(logs) = page.get("call_logs").and_then(Value::as_array) else {
            continue;
        };
        for log in logs {
            if log.get("direction").and_then(Value::as_str) != Some("outbound") {
                continue;
            }
            let text = |k: &str| log.get(k).and_then(Value::as_str).filter(|s| !s.is_empty());
            let Some(id) = text("id").or_else(|| text("call_id")) else {
                continue;
            };
            if !seen.insert(id.to_string()) {
                continue;
            }
            let Some(at) = text("date_time").or_else(|| text("start_time")) else {
                continue;
            };
            let Ok(when) = DateTime::parse_from_rfc3339(at) else {
                continue;
            };
            let when = when.with_timezone(&Utc);
            if when < start || when >= end {
                continue;
            }
            let Some(uid) = text("user_id") else {
                continue;
            };
            let e = per_user
                .entry(uid.to_string())
                .or_insert((0, 0, DialBreakdown::default()));
            e.0 += 1;
            if text("result") == Some("Auto Recorded") {
                e.1 += 1;
            }
            e.2.add(classify(log));
        }
    }
    let mut out = Counts::default();
    for (uid, (c, k, b)) in per_user {
        if let Some(owner) = user_email.get(&uid).and_then(|e| email_owner.get(e)) {
            *out.calls.entry(owner.clone()).or_insert(0) += c;
            *out.connected.entry(owner.clone()).or_insert(0) += k;
            let e = out.breakdown.entry(owner.clone()).or_default();
            *e = e.plus(&b);
        }
    }
    out
}

#[derive(Default)]
struct Inner {
    entry: Option<(NaiveDate, Instant, Arc<LiveToday>)>,
    in_flight: bool,
    retry_after: Option<Instant>,
    users: Option<(Instant, Arc<HashMap<String, String>>)>,
}

/// 有効な Zoom ユーザー (`user_id -> メール`)。1 時間は持ち回す
pub(super) async fn active_users(
    zoom: &ZoomClient,
) -> Result<Arc<HashMap<String, String>>, super::zoom::ZoomError> {
    let cached = {
        let g = state().lock().unwrap();
        g.users
            .as_ref()
            .filter(|(at, _)| at.elapsed() < USERS_TTL)
            .map(|(_, u)| u.clone())
    };
    match cached {
        Some(u) => Ok(u),
        None => {
            let u = Arc::new(zoom.active_user_emails().await?);
            state().lock().unwrap().users = Some((Instant::now(), u.clone()));
            Ok(u)
        }
    }
}

fn state() -> &'static Mutex<Inner> {
    static S: OnceLock<Mutex<Inner>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(Inner::default()))
}

fn jst_text(now: DateTime<Utc>) -> String {
    (now + ChronoDuration::hours(9))
        .format("%Y-%m-%d %H:%M")
        .to_string()
}

async fn refresh(zoom: Arc<ZoomClient>, owner_map: HashMap<String, String>, day: NaiveDate) {
    let result = async {
        let users = active_users(&zoom).await?;
        let (from, to) = utc_range(day);
        let pages = zoom.call_log_pages(&from, &to).await?;
        Ok::<_, super::zoom::ZoomError>(count_calls(&pages, day, &users, &owner_map))
    }
    .await;
    let mut g = state().lock().unwrap();
    g.in_flight = false;
    match result {
        Ok(counts) => {
            g.retry_after = None;
            g.entry = Some((
                day,
                Instant::now(),
                Arc::new(LiveToday {
                    by_owner: counts.connected,
                    calls_by_owner: counts.calls,
                    breakdown_by_owner: counts.breakdown,
                    fetched_at: jst_text(Utc::now()),
                }),
            ));
        }
        Err(e) => {
            tracing::warn!(error = ?e, "talk review: live dials refresh failed");
            g.retry_after = Some(Instant::now() + RETRY_AFTER);
        }
    }
}

/// 今日の人ごとの架電数。15 分以内の値があればそれを返し、古ければ背景で数え直して前の値を返す。
/// 値がまだ無いときだけ、最初の数え終わりを最大 20 秒待つ。数えられなければ `None`。
pub async fn live_today(
    zoom: Arc<ZoomClient>,
    owner_map: HashMap<String, String>,
    day: NaiveDate,
) -> Option<Arc<LiveToday>> {
    let (cached, spawn) = {
        let mut g = state().lock().unwrap();
        let cached = g
            .entry
            .as_ref()
            .filter(|(d, _, _)| *d == day)
            .map(|(_, at, v)| (*at, v.clone()));
        let fresh = cached
            .as_ref()
            .is_some_and(|(at, _)| at.elapsed() < LIVE_TTL);
        let may_retry = g.retry_after.is_none_or(|t| Instant::now() >= t);
        let spawn = !fresh && !g.in_flight && may_retry;
        if spawn {
            g.in_flight = true;
        }
        (cached.map(|(_, v)| v), spawn)
    };
    let handle = spawn.then(|| tokio::spawn(refresh(zoom, owner_map, day)));
    if cached.is_some() {
        return cached;
    }
    // 値がまだ無い: 数え終わりを待つ (上限つき。超えたらシートに戻す)
    let deadline = Instant::now() + FIRST_WAIT;
    if let Some(h) = handle {
        let _ = tokio::time::timeout(FIRST_WAIT, h).await;
    } else {
        loop {
            let (done, busy) = {
                let g = state().lock().unwrap();
                (
                    g.entry.as_ref().is_some_and(|(d, _, _)| *d == day),
                    g.in_flight,
                )
            };
            if done || !busy || Instant::now() >= deadline {
                break;
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
    }
    state()
        .lock()
        .unwrap()
        .entry
        .as_ref()
        .filter(|(d, _, _)| *d == day)
        .map(|(_, _, v)| v.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }
    fn maps() -> (HashMap<String, String>, HashMap<String, String>) {
        let users = HashMap::from([
            ("u1".to_string(), "a@f-a-c.co.jp".to_string()),
            ("u2".to_string(), "b@f-a-c.co.jp".to_string()),
            ("u3".to_string(), "nobody@f-a-c.co.jp".to_string()),
        ]);
        let owners = HashMap::from([
            ("a@f-a-c.co.jp".to_string(), "11".to_string()),
            ("b@f-a-c.co.jp".to_string(), "22".to_string()),
        ]);
        (users, owners)
    }
    fn log(id: &str, dir: &str, at: &str, user: &str, result: &str) -> Value {
        json!({"id": id, "direction": dir, "date_time": at, "user_id": user, "result": result})
    }

    #[test]
    fn utc_range_is_jst_midnight_to_last_second() {
        assert_eq!(
            utc_range(day()),
            (
                "2026-10-19T15:00:00Z".to_string(),
                "2026-10-20T14:59:59Z".to_string()
            )
        );
    }

    #[test]
    fn counts_only_outbound_auto_recorded_inside_the_jst_day() {
        let (users, owners) = maps();
        let pages = vec![
            json!({"call_logs": [
                log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
                // 同じ id は 1 件
                log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
                // 着信は数えない
                log("2", "inbound", "2026-10-20T01:00:00Z", "u1", "Auto Recorded"),
                // つながっていない
                log("3", "outbound", "2026-10-20T01:00:00Z", "u1", "Call connected"),
                // UTC 19 日 23:59 = JST 20 日 08:59 は今日
                log("4", "outbound", "2026-10-19T23:59:00Z", "u2", "Auto Recorded"),
                // JST 20 日 0:00 の 1 秒前 (UTC 19 日 14:59:59) は前日
                log("5", "outbound", "2026-10-19T14:59:59Z", "u2", "Auto Recorded"),
                // JST 21 日 0:00 (UTC 20 日 15:00:00) は翌日
                log("6", "outbound", "2026-10-20T15:00:00Z", "u2", "Auto Recorded"),
                // JST 20 日 23:59:59 は今日
                log("7", "outbound", "2026-10-20T14:59:59Z", "u2", "Auto Recorded"),
            ]}),
            json!({"call_logs": [
                // call_id と start_time の代替項目
                {"call_id": "8", "direction": "outbound", "start_time": "2026-10-20T02:00:00Z", "user_id": "u1", "result": "Auto Recorded"},
                // user_id なし
                {"id": "9", "direction": "outbound", "date_time": "2026-10-20T02:00:00Z", "result": "Auto Recorded"},
                // 担当者に結び付かない人
                log("10", "outbound", "2026-10-20T02:00:00Z", "u3", "Auto Recorded"),
                log("11", "outbound", "2026-10-20T02:00:00Z", "u9", "Auto Recorded"),
            ]}),
        ];
        let r = count_calls(&pages, day(), &users, &owners).connected;
        assert_eq!(r.get("11"), Some(&2)); // id 1 と 8
        assert_eq!(r.get("22"), Some(&2)); // id 4 と 7
        assert_eq!(r.len(), 2);
    }

    #[test]
    fn calls_include_cancelled_and_failed_but_connected_does_not() {
        let (users, owners) = maps();
        let pages = vec![json!({"call_logs": [
            log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
            log("1", "outbound", "2026-10-20T00:30:00Z", "u1", "Auto Recorded"),
            log("2", "outbound", "2026-10-20T00:31:00Z", "u1", "Call Cancel"),
            log("3", "outbound", "2026-10-20T00:32:00Z", "u1", "Call failed"),
            log("4", "inbound", "2026-10-20T00:33:00Z", "u1", "Auto Recorded"),
            log("5", "outbound", "2026-10-19T14:59:59Z", "u1", "Call Cancel"),
            log("6", "outbound", "2026-10-20T01:00:00Z", "u2", "Call Cancel"),
            log("7", "outbound", "2026-10-20T01:00:00Z", "u3", "Call Cancel"),
        ]})];
        let r = count_calls(&pages, day(), &users, &owners);
        assert_eq!(r.calls.get("11"), Some(&3)); // 1 + 2 + 3 (重複・着信・前日は除く)
        assert_eq!(r.connected.get("11"), Some(&1));
        assert_eq!(r.calls.get("22"), Some(&1));
        assert_eq!(r.connected.get("22"), Some(&0));
        assert_eq!(r.calls.len(), 2); // 担当者に結び付かない u3 は数えない
    }

    #[test]
    fn active_owner_wins_over_archived_for_the_same_email() {
        let o = |id: &str, email: &str, archived: bool| CrmOwner {
            id: id.into(),
            name: id.into(),
            email: Some(email.into()),
            archived,
            teams: vec![],
        };
        let m = owner_by_email(&[
            o("old", "A@f-a-c.co.jp", true),
            o("new", "a@f-a-c.co.jp", false),
            o("x", "x@f-a-c.co.jp", true),
        ]);
        assert_eq!(m.get("a@f-a-c.co.jp").map(String::as_str), Some("new"));
        assert_eq!(m.get("x@f-a-c.co.jp").map(String::as_str), Some("x"));
    }
}
