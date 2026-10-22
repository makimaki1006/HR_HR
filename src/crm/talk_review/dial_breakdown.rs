//! 発信の内訳: つながった / 発信に失敗した / こちらで切った / モニタリング / 社内の内線。
//! Zoom の通話ログ (アカウント単位) の 1 行を `path` と `result` で分ける。既存の「発信数」「架電数」の
//! 定義は変えず、ここは表示用の別の数え方 (Zoom の記録から直接数えるので、シート由来の発信数と数本ずれることがある)。
//!
//! 今日の分は `live_dials` が同じページから数える。今日より前の日はシートに内訳が無いので、日ごとに
//! Zoom から取り、プロセス内に日単位でキャッシュする (過去の日は変わらないので有効期限なし)。
//! 取り込みは背景で 1 日ずつ順に行い、要求は待たない。揃っていない間は「集計中」を返す。
//! **単一インスタンス前提** (他の worker と同じ)。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::NaiveDate;
use serde_json::Value;

use super::live_dials::{active_users, count_calls, utc_range, LiveToday};
use super::types::{DialBreakdown, TalkPersonRow};
use super::zoom::ZoomClient;

/// キャッシュする日数の上限 (超えたら古い日から捨てる)
pub const MAX_DAYS: usize = 120;
/// 取り込みに失敗したあと、次に試すまで
const RETRY_AFTER: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DialKind {
    Connected,
    Failed,
    Cancelled,
    Monitor,
    Extension,
}

/// 通話ログの 1 行 (発信) の種類。`path` が monitor ならモニタリング、extension なら内線、
/// それ以外は `result` で、録音が残ったもの (Auto Recorded / Recorded) がつながった、Call Cancel が
/// こちらで切った、そのほか (主に "Call connected" と出るが実際は失敗した発信) は失敗。
pub fn classify(log: &Value) -> DialKind {
    match log.get("path").and_then(Value::as_str) {
        Some("monitor") => return DialKind::Monitor,
        Some("extension") => return DialKind::Extension,
        _ => {}
    }
    match log.get("result").and_then(Value::as_str) {
        Some("Auto Recorded" | "Recorded") => DialKind::Connected,
        Some("Call Cancel") => DialKind::Cancelled,
        _ => DialKind::Failed,
    }
}

impl DialBreakdown {
    pub fn add(&mut self, kind: DialKind) {
        match kind {
            DialKind::Connected => self.connected += 1,
            DialKind::Failed => self.failed += 1,
            DialKind::Cancelled => self.cancelled += 1,
            DialKind::Monitor => self.monitor += 1,
            DialKind::Extension => self.extension += 1,
        }
        self.total += 1;
    }

    pub fn plus(&self, o: &DialBreakdown) -> DialBreakdown {
        DialBreakdown {
            connected: self.connected + o.connected,
            failed: self.failed + o.failed,
            cancelled: self.cancelled + o.cancelled,
            monitor: self.monitor + o.monitor,
            extension: self.extension + o.extension,
            total: self.total + o.total,
        }
    }
}

type DayMap = Arc<HashMap<String, DialBreakdown>>;

#[derive(Default)]
struct History {
    days: BTreeMap<NaiveDate, DayMap>,
    in_flight: bool,
    retry_after: Option<Instant>,
}

fn history() -> &'static Mutex<History> {
    static S: OnceLock<Mutex<History>> = OnceLock::new();
    S.get_or_init(|| Mutex::new(History::default()))
}

fn insert_day(h: &mut History, day: NaiveDate, v: DayMap) {
    h.days.insert(day, v);
    while h.days.len() > MAX_DAYS {
        h.days.pop_first();
    }
}

/// 内訳の取り出し結果
#[derive(Debug, Clone, PartialEq)]
pub enum Outcome {
    Ready(HashMap<String, DialBreakdown>),
    /// 過去の日を取り込み中
    Pending,
    /// Zoom から取れない (未設定・失敗)
    Unavailable,
}

/// キャッシュにある日の内訳を足し合わせる。足りない日があれば `Err(足りない日)`
fn gather(
    h: &History,
    days: &[NaiveDate],
) -> Result<HashMap<String, DialBreakdown>, Vec<NaiveDate>> {
    let missing: Vec<NaiveDate> = days
        .iter()
        .filter(|d| !h.days.contains_key(d))
        .copied()
        .collect();
    if !missing.is_empty() {
        return Err(missing);
    }
    let mut out: HashMap<String, DialBreakdown> = HashMap::new();
    for d in days {
        for (o, b) in h.days[d].iter() {
            let e = out.entry(o.clone()).or_default();
            *e = e.plus(b);
        }
    }
    Ok(out)
}

async fn fill(zoom: Arc<ZoomClient>, owner_map: HashMap<String, String>, missing: Vec<NaiveDate>) {
    for day in missing {
        let r = async {
            let users = active_users(&zoom).await?;
            let (from, to) = utc_range(day);
            let pages = zoom.call_log_pages(&from, &to).await?;
            Ok::<_, super::zoom::ZoomError>(count_calls(&pages, day, &users, &owner_map).breakdown)
        }
        .await;
        let mut g = history().lock().unwrap();
        match r {
            Ok(b) => insert_day(&mut g, day, Arc::new(b)),
            Err(e) => {
                tracing::warn!(error = ?e, %day, "talk review: dial breakdown fetch failed");
                g.retry_after = Some(Instant::now() + RETRY_AFTER);
                g.in_flight = false;
                return;
            }
        }
    }
    history().lock().unwrap().in_flight = false;
}

/// 期間 `[from, to]` の人ごとの内訳。今日が入っていれば今日の分は `live` から、前の日はキャッシュから。
/// 足りない日があれば背景で取り込みを始め (同時に 1 本)、`Pending` を返す。
pub fn resolve(
    zoom: Option<Arc<ZoomClient>>,
    owner_map: HashMap<String, String>,
    from: NaiveDate,
    to: NaiveDate,
    today: NaiveDate,
    live: Option<&LiveToday>,
) -> Outcome {
    let Some(zoom) = zoom else {
        return Outcome::Unavailable;
    };
    let includes_today = to >= today;
    if includes_today && live.is_none() {
        return Outcome::Unavailable;
    }
    let past_last = if includes_today {
        today.pred_opt().unwrap_or(today)
    } else {
        to
    };
    let days: Vec<NaiveDate> = if from <= past_last {
        from.iter_days().take_while(|d| *d <= past_last).collect()
    } else {
        Vec::new()
    };
    let (gathered, spawn) = {
        let mut g = history().lock().unwrap();
        match gather(&g, &days) {
            Ok(m) => (Ok(m), None),
            Err(missing) => {
                let may_retry = g.retry_after.is_none_or(|t| Instant::now() >= t);
                if !g.in_flight && may_retry {
                    g.in_flight = true;
                    (Err(()), Some(missing))
                } else {
                    (Err(()), None)
                }
            }
        }
    };
    if let Some(missing) = spawn {
        tokio::spawn(fill(zoom, owner_map, missing));
    }
    let Ok(mut out) = gathered else {
        return Outcome::Pending;
    };
    if includes_today {
        if let Some(l) = live {
            for (o, b) in &l.breakdown_by_owner {
                let e = out.entry(o.clone()).or_default();
                *e = e.plus(b);
            }
        }
    }
    Outcome::Ready(out)
}

/// 人の行とチーム全体の行に内訳を入れる。人は map に無ければ 0 本、チーム全体は表に出ている人の合計
pub fn apply(
    people: &mut [TalkPersonRow],
    total: &mut TalkPersonRow,
    map: &HashMap<String, DialBreakdown>,
) {
    let mut sum = DialBreakdown::default();
    for p in people.iter_mut() {
        let b = p
            .owner_id
            .as_ref()
            .and_then(|o| map.get(o))
            .copied()
            .unwrap_or_default();
        sum = sum.plus(&b);
        p.dial_breakdown = Some(b);
    }
    total.dial_breakdown = Some(sum);
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 20).unwrap()
    }
    fn log(id: &str, dir: &str, at: &str, path: &str, result: &str) -> Value {
        json!({"id": id, "direction": dir, "date_time": at, "user_id": "u1", "path": path, "result": result})
    }
    fn maps() -> (HashMap<String, String>, HashMap<String, String>) {
        (
            HashMap::from([("u1".to_string(), "a@f-a-c.co.jp".to_string())]),
            HashMap::from([("a@f-a-c.co.jp".to_string(), "11".to_string())]),
        )
    }

    #[test]
    fn classify_follows_path_then_result() {
        let k = |p: &str, r: &str| classify(&json!({"path": p, "result": r}));
        assert_eq!(k("pstn", "Auto Recorded"), DialKind::Connected);
        assert_eq!(k("pstn", "Recorded"), DialKind::Connected);
        assert_eq!(k("pstn", "Call Cancel"), DialKind::Cancelled);
        assert_eq!(k("pstn", "Call connected"), DialKind::Failed);
        assert_eq!(k("monitor", "Call connected"), DialKind::Monitor);
        assert_eq!(k("extension", "Call connected"), DialKind::Extension);
        // path が内線なら result に関わらず内線
        assert_eq!(k("extension", "Auto Recorded"), DialKind::Extension);
        // 値が無いものは失敗
        assert_eq!(classify(&json!({})), DialKind::Failed);
    }

    #[test]
    fn breakdown_counts_real_combinations_and_leaves_calls_unchanged() {
        let (users, owners) = maps();
        let pages = vec![json!({"call_logs": [
            log("1", "outbound", "2026-10-20T00:00:00Z", "pstn", "Auto Recorded"),
            log("1", "outbound", "2026-10-20T00:00:00Z", "pstn", "Auto Recorded"), // 重複
            log("2", "outbound", "2026-10-20T00:01:00Z", "pstn", "Auto Recorded"),
            log("3", "outbound", "2026-10-20T00:02:00Z", "pstn", "Recorded"),
            log("4", "outbound", "2026-10-20T00:03:00Z", "pstn", "Call Cancel"),
            log("5", "outbound", "2026-10-20T00:04:00Z", "pstn", "Call connected"),
            log("6", "outbound", "2026-10-20T00:05:00Z", "monitor", "Call connected"),
            log("7", "outbound", "2026-10-20T00:06:00Z", "extension", "Call connected"),
            log("8", "inbound", "2026-10-20T00:07:00Z", "pstn", "Auto Recorded"), // 着信は除く
            log("9", "outbound", "2026-10-19T14:59:59Z", "pstn", "Call Cancel"), // JST 前日
            log("10", "outbound", "2026-10-20T15:00:00Z", "pstn", "Call Cancel"), // JST 翌日
            log("11", "outbound", "2026-10-20T14:59:59Z", "pstn", "Call Cancel"), // JST 当日の最後
        ]})];
        let r = count_calls(&pages, day(), &users, &owners);
        let b = r.breakdown.get("11").unwrap();
        assert_eq!(
            *b,
            DialBreakdown {
                connected: 3,
                failed: 1,
                cancelled: 2,
                monitor: 1,
                extension: 1,
                total: 8
            }
        );
        // 既存の発信数・架電数は変わらない (架電数は Auto Recorded だけ)
        assert_eq!(r.calls.get("11"), Some(&8));
        assert_eq!(r.connected.get("11"), Some(&2));
    }

    #[test]
    fn history_sums_days_and_reports_missing_ones() {
        let d19 = NaiveDate::from_ymd_opt(2026, 10, 19).unwrap();
        let mut h = History::default();
        let b = |c, f| DialBreakdown {
            connected: c,
            failed: f,
            total: c + f,
            ..Default::default()
        };
        insert_day(
            &mut h,
            d19,
            Arc::new(HashMap::from([("11".to_string(), b(8, 1))])),
        );
        insert_day(
            &mut h,
            day(),
            Arc::new(HashMap::from([("11".to_string(), b(2, 0))])),
        );
        let m = gather(&h, &[d19, day()]).unwrap();
        assert_eq!(m["11"], b(10, 1));
        let d18 = NaiveDate::from_ymd_opt(2026, 10, 18).unwrap();
        assert_eq!(gather(&h, &[d18, d19]).unwrap_err(), vec![d18]);
    }

    #[test]
    fn history_drops_oldest_days_over_the_cap() {
        let mut h = History::default();
        let first = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        for i in 0..(MAX_DAYS as i64 + 5) {
            insert_day(
                &mut h,
                first + chrono::Duration::days(i),
                Arc::new(HashMap::new()),
            );
        }
        assert_eq!(h.days.len(), MAX_DAYS);
        assert!(!h.days.contains_key(&first));
        assert!(h
            .days
            .contains_key(&(first + chrono::Duration::days(MAX_DAYS as i64 + 4))));
    }

    #[test]
    fn apply_gives_zero_to_people_without_calls_and_sums_the_total() {
        let mk = |id: Option<&str>| TalkPersonRow {
            owner_id: id.map(str::to_string),
            name: "x".into(),
            team: None,
            outbound: None,
            dial_breakdown: None,
            dials: None,
            appointments: 0,
            appointment_rate_pct: None,
            appointment_rate_flagged: false,
            contacts: None,
            contact_rate_pct: None,
            date_proposal_calls: None,
            date_proposal_rate_pct: None,
            proposals_per_contact: None,
            contact_pending: 0,
            eligible: 0,
            reviewed: 0,
            waiting: 0,
            unavailable: 0,
            items: vec![],
        };
        let mut people = vec![mk(Some("11")), mk(Some("22"))];
        let mut total = mk(None);
        let map = HashMap::from([(
            "11".to_string(),
            DialBreakdown {
                connected: 5,
                cancelled: 2,
                total: 7,
                ..Default::default()
            },
        )]);
        apply(&mut people, &mut total, &map);
        assert_eq!(people[0].dial_breakdown.unwrap().total, 7);
        assert_eq!(people[1].dial_breakdown, Some(DialBreakdown::default()));
        assert_eq!(total.dial_breakdown.unwrap().connected, 5);
    }
}
