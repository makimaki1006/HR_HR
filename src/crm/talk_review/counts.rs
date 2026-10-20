//! 総架電数とアポ数を期間で数える (日ごとにキャッシュ)。
//!
//! - 総架電数: 今日の架電成績 (`caller_metrics`) と同じ定義。Zoom 純正連携の Call、`OUTBOUND`。
//!   過去の日は `caller_metrics_month` と同じく、担当者を 100 人ずつ `IN` で絞って 1 日分ずつ読む。
//! - アポ数: BPO は `caller_metrics` の定義 (「アポ日確定」= ステージ `1095457875` に入った取引を、
//!   アポ獲得者 `bpo_26`、空なら取引の担当者に付ける)。社内の新規営業は営業KPI (`caller_metrics_month`) と同じ
//!   定義 (`ST_APO` と `ST_APO_BPO` に入った取引を、取引の担当者 `hubspot_owner_id` に付ける)。
//!
//! HubSpot の呼び出しは、まだ持っていない日だけ (確定した日は読み直さない。今日と昨日は 5 分で読み直す)。
//! 見積り: Call は 1 日・100 人分の担当者あたり Search 約 5 回 (1 日の対象の架電が 1,000 件前後のとき)。
//! アポは持っていない日の範囲を 2 ステージ分まとめて 1 回ずつ (数百件で Search 2〜4 回)。
//! 1 回の読み込みの Search は `MAX_SEARCHES` まで。全部 `Background` 優先度。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde_json::{json, Value};

use crate::crm::caller_metrics::{
    aggregate, appointment_owner, flt, jst_day, APPOINTER_PROPERTY, PAGE,
};
use crate::crm::caller_metrics_month::{jst_midnight_ms, OWNER_CHUNK};
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};
use crate::hubspot::{HubSpotClient, HubSpotError};

pub const MAX_SEARCHES: u32 = 600;
/// 今日と昨日の読み直し間隔
pub const RECENT_TTL: Duration = Duration::seconds(300);

/// 対象の担当者。BPO は BPO の数え方 (アポ獲得者)、sales は営業KPI の数え方
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TeamOwners {
    pub bpo: Vec<String>,
    pub sales: Vec<String>,
}

impl TeamOwners {
    pub fn all(&self) -> Vec<String> {
        let mut v: Vec<String> = self.bpo.iter().chain(&self.sales).cloned().collect();
        v.sort();
        v.dedup();
        v
    }
}

/// 取引 1 件のアポを付ける担当者 (0〜2 人)。BPO の人にはアポ獲得者 (空なら担当者)、
/// 新規営業の人には取引の担当者。どちらにも当たらなければ付けない。
pub fn credit(
    appointer_or_owner: Option<&str>,
    deal_owner: Option<&str>,
    bpo: &HashSet<&str>,
    sales: &HashSet<&str>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(a) = appointer_or_owner.filter(|a| bpo.contains(a)) {
        out.push(a.to_string());
    }
    if let Some(o) = deal_owner.filter(|o| sales.contains(o)) {
        if !out.iter().any(|x| x == o) {
            out.push(o.to_string());
        }
    }
    out
}

type DayMap = HashMap<String, u32>;

/// 1 日分の総架電数。`covered` は読んだ担当者 (顔ぶれが増えたら増えた人の分だけ読む)
#[derive(Debug, Clone)]
struct DialsDay {
    at: DateTime<Utc>,
    covered: BTreeSet<String>,
    counts: DayMap,
}

/// 1 日分のアポ (取引ごとに、付いた担当者の一覧)。`covered` は読んだ時の対象の担当者
#[derive(Debug, Clone)]
struct ApoDay {
    at: DateTime<Utc>,
    covered: BTreeSet<String>,
    deals: Vec<Vec<String>>,
}

#[derive(Default)]
struct Inner {
    dials: BTreeMap<NaiveDate, DialsDay>,
    apo: BTreeMap<NaiveDate, ApoDay>,
}

#[derive(Default)]
pub struct CountsCache {
    inner: Mutex<Inner>,
    /// 同時に読むのは 1 本 (同じ日を二重に読まない)
    fill: tokio::sync::Mutex<()>,
}

/// 読み直しが要るか。その日が終わる (翌日 JST 0 時) 前に読んだ数字は途中なので確定扱いにしない。
/// 今日と昨日は 5 分で読み直す。
pub fn stale(at: DateTime<Utc>, day: NaiveDate, today: NaiveDate, now: DateTime<Utc>) -> bool {
    let day_end =
        DateTime::<Utc>::from_timestamp_millis(jst_midnight_ms(day) + 86_400_000).unwrap_or(now);
    let ttl_over = now - at >= RECENT_TTL;
    let incomplete = at < day_end;
    (incomplete && (day < today || ttl_over)) || (day >= today - Duration::days(1) && ttl_over)
}

fn days(from: NaiveDate, to: NaiveDate) -> Vec<NaiveDate> {
    from.iter_days().take_while(|d| *d <= to).collect()
}

/// 総架電数で読む日と、その日に読む担当者 (読み直しは全員、足りないだけなら増えた人だけ)
fn plan_dials(
    m: &BTreeMap<NaiveDate, DialsDay>,
    owners: &[String],
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
) -> Vec<(NaiveDate, Vec<String>)> {
    let (today, _, _) = jst_day(now);
    // 新しい日から読む (画面は、読み終えた日だけで先に答える)
    days(from, to)
        .into_iter()
        .rev()
        .filter_map(|d| match m.get(&d) {
            Some(e) if !stale(e.at, d, today, now) => {
                let add: Vec<String> = owners
                    .iter()
                    .filter(|o| !e.covered.contains(*o))
                    .cloned()
                    .collect();
                (!add.is_empty()).then_some((d, add))
            }
            _ => Some((d, owners.to_vec())),
        })
        .collect()
}

/// アポで読む日。取引は担当者で絞らずに読むので、顔ぶれが増えたら範囲を読み直す (Search 数回)
fn plan_apo(
    m: &BTreeMap<NaiveDate, ApoDay>,
    owners: &[String],
    from: NaiveDate,
    to: NaiveDate,
    now: DateTime<Utc>,
) -> Vec<NaiveDate> {
    let (today, _, _) = jst_day(now);
    days(from, to)
        .into_iter()
        .filter(|d| match m.get(d) {
            Some(e) => stale(e.at, *d, today, now) || owners.iter().any(|o| !e.covered.contains(o)),
            None => true,
        })
        .collect()
}

/// キャッシュにある分だけの集計 (HubSpot は呼ばない)。`counting` は、まだ数え終わっていない日
#[derive(Debug, Default)]
pub struct Snapshot {
    pub dials: DayMap,
    pub deals: Vec<Vec<String>>,
    pub counting: Vec<NaiveDate>,
}

impl CountsCache {
    /// 期間の人ごとの総架電数と、アポ (取引ごとに付いた担当者の一覧)。持っている日だけで答え、
    /// 総架電かアポのどちらかを持っていない日は `counting` に入れる (その日の数は含めない)。
    /// 今日と昨日の古い数字は、読み直しが済むまでそのまま使う。
    pub fn snapshot(&self, from: NaiveDate, to: NaiveDate) -> Snapshot {
        let g = self.inner.lock().unwrap();
        let mut out = Snapshot::default();
        for d in days(from, to) {
            let (Some(dd), Some(ad)) = (g.dials.get(&d), g.apo.get(&d)) else {
                out.counting.push(d);
                continue;
            };
            for (o, n) in &dd.counts {
                *out.dials.entry(o.clone()).or_default() += n;
            }
            out.deals.extend(ad.deals.iter().cloned());
        }
        out
    }

    /// 持っていない日・古い日を HubSpot から読んで残す (新しい日から)。途中で失敗しても、読めた日は残る。
    /// 同時に読むのは 1 本。`wait` が false で他が読んでいる最中なら、何もしない。
    pub async fn fill(
        &self,
        client: &HubSpotClient,
        team: &TeamOwners,
        from: NaiveDate,
        to: NaiveDate,
        now: DateTime<Utc>,
        wait: bool,
    ) -> Result<(), HubSpotError> {
        let owners = team.all();
        let _fill = if wait {
            self.fill.lock().await
        } else {
            match self.fill.try_lock() {
                Ok(g) => g,
                Err(_) => return Ok(()),
            }
        };
        let (dial_plan, need_apo) = {
            let g = self.inner.lock().unwrap();
            (
                plan_dials(&g.dials, &owners, from, to, now),
                plan_apo(&g.apo, &owners, from, to, now),
            )
        };
        let bg = client.background();
        let mut searches = 0u32;
        if owners.is_empty() {
            return Ok(());
        }
        // アポは数回の Search で範囲ごと読めるので先に済ませる (総架電の日がそろえば、その日は答えに入る)
        if let (Some(first), Some(last)) = (need_apo.first(), need_apo.last()) {
            let by_day = fetch_appointments(&bg, team, *first, *last, &mut searches).await?;
            let covered: BTreeSet<String> = owners.iter().cloned().collect();
            let mut g = self.inner.lock().unwrap();
            for d in &need_apo {
                g.apo.insert(
                    *d,
                    ApoDay {
                        at: now,
                        covered: covered.clone(),
                        deals: by_day.get(d).cloned().unwrap_or_default(),
                    },
                );
            }
        }
        for (day, who) in dial_plan {
            let a = jst_midnight_ms(day);
            let mut rows = Vec::new();
            for chunk in who.chunks(OWNER_CHUNK) {
                rows.extend(
                    crate::crm::caller_metrics::fetch_calls(
                        &bg,
                        "hs_timestamp",
                        a,
                        a + 86_400_000,
                        None,
                        Some(chunk),
                        &mut searches,
                        MAX_SEARCHES,
                    )
                    .await?,
                );
            }
            let m: DayMap = aggregate(rows.iter(), &HashMap::new())
                .into_iter()
                .map(|(o, m)| (o, m.outbound))
                .filter(|(_, n)| *n > 0)
                .collect();
            let mut g = self.inner.lock().unwrap();
            match g.dials.get_mut(&day) {
                // 足りない人の分だけ足す (読み直しのときは全員分なので置き換え)
                Some(e) if who.len() < owners.len() && !stale(e.at, day, jst_day(now).0, now) => {
                    e.covered.extend(who);
                    e.counts.extend(m);
                }
                _ => {
                    g.dials.insert(
                        day,
                        DialsDay {
                            at: now,
                            covered: who.into_iter().collect(),
                            counts: m,
                        },
                    );
                }
            }
        }
        tracing::info!(searches, "talk review counts filled");
        Ok(())
    }
}

fn entered_day(v: &Value) -> Option<NaiveDate> {
    let ms = match v {
        Value::String(s) => s.parse::<i64>().ok().or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis())
        })?,
        Value::Number(n) => n.as_i64()?,
        _ => return None,
    };
    Some(jst_day(DateTime::<Utc>::from_timestamp_millis(ms)?).0)
}

/// `[first, last]` (JST の日) に「アポ日確定」へ入った取引を、日ごと・担当者ごとに数える
async fn fetch_appointments(
    bg: &HubSpotClient,
    team: &TeamOwners,
    first: NaiveDate,
    last: NaiveDate,
    searches: &mut u32,
) -> Result<BTreeMap<NaiveDate, Vec<Vec<String>>>, HubSpotError> {
    let bpo: HashSet<&str> = team.bpo.iter().map(String::as_str).collect();
    let sales: HashSet<&str> = team.sales.iter().map(String::as_str).collect();
    let from = jst_midnight_ms(first);
    let to = jst_midnight_ms(last) + 86_400_000;
    let mut out: BTreeMap<NaiveDate, Vec<Vec<String>>> = BTreeMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for stage in [ST_APO, ST_APO_BPO] {
        let prop = format!("hs_v2_date_entered_{stage}");
        let mut after: Option<String> = None;
        loop {
            *searches += 1;
            if *searches > MAX_SEARCHES {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let mut body = json!({
                "filterGroups": [{"filters": [
                    flt(&prop, "GTE", json!(from.to_string())),
                    flt(&prop, "LT", json!(to.to_string())),
                ]}],
                "properties": ["hubspot_owner_id", APPOINTER_PROPERTY, prop],
                "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
                "limit": PAGE,
            });
            if let Some(a) = &after {
                body["after"] = json!(a);
            }
            let v = bg.search("deals", body).await?;
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("deals without results".into()))?;
            for r in results {
                let id = r.get("id").map(Value::to_string).unwrap_or_default();
                if !seen.insert(id) {
                    continue;
                }
                let Some(p) = r.get("properties") else {
                    continue;
                };
                let Some(day) = p.get(&prop).and_then(entered_day) else {
                    continue;
                };
                let owner = p
                    .get("hubspot_owner_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                let who = credit(appointment_owner(p), owner, &bpo, &sales);
                if !who.is_empty() {
                    out.entry(day).or_default().push(who);
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn set<'a>(v: &[&'a str]) -> HashSet<&'a str> {
        v.iter().copied().collect()
    }

    #[test]
    fn credit_follows_each_teams_definition() {
        let bpo = set(&["B1"]);
        let sales = set(&["S1"]);
        // BPO が取ったアポ: アポ獲得者 B1 に付く。取引の担当者 S1 (新規営業) にも付く
        assert_eq!(
            credit(Some("B1"), Some("S1"), &bpo, &sales),
            vec!["B1", "S1"]
        );
        // 担当者が BPO 以外の別の人 → 対象外
        assert!(credit(Some("X"), Some("Y"), &bpo, &sales).is_empty());
        // 担当者が新規営業でアポ獲得者が空 (担当者が代わりに入る) → 1 人に 1 回
        assert_eq!(credit(Some("S1"), Some("S1"), &bpo, &sales), vec!["S1"]);
        // BPO 本人が取引の担当者
        assert_eq!(credit(Some("B1"), Some("B1"), &bpo, &sales), vec!["B1"]);
    }

    #[test]
    fn entered_day_uses_jst() {
        // 2026-10-14 15:30 UTC = 2026-10-15 00:30 JST
        let ms = DateTime::parse_from_rfc3339("2026-10-14T15:30:00Z")
            .unwrap()
            .timestamp_millis();
        assert_eq!(
            entered_day(&json!(ms.to_string())),
            NaiveDate::from_ymd_opt(2026, 10, 15)
        );
        assert_eq!(
            entered_day(&json!("2026-10-14T14:59:59Z")),
            NaiveDate::from_ymd_opt(2026, 10, 14)
        );
        assert_eq!(entered_day(&json!(null)), None);
    }

    #[test]
    fn stale_only_for_today_and_yesterday() {
        let now = DateTime::parse_from_rfc3339("2026-10-15T03:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let today = jst_day(now).0;
        let old = now - Duration::minutes(10);
        assert!(stale(old, today, today, now));
        assert!(stale(old, today - Duration::days(1), today, now));
        assert!(!stale(old, today - Duration::days(2), today, now));
        assert!(!stale(now - Duration::minutes(1), today, today, now));
    }

    #[test]
    fn team_owners_all_is_sorted_unique() {
        let t = TeamOwners {
            bpo: vec!["2".into(), "1".into()],
            sales: vec!["1".into(), "3".into()],
        };
        assert_eq!(t.all(), vec!["1", "2", "3"]);
    }

    #[test]
    fn snapshot_answers_with_counted_days_and_lists_the_rest() {
        let d = |n: u32| NaiveDate::from_ymd_opt(2026, 10, n).unwrap();
        let at = DateTime::parse_from_rfc3339("2026-10-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let c = CountsCache::default();
        {
            let mut g = c.inner.lock().unwrap();
            let dial = |o: &str, n: u32| DialsDay {
                at,
                covered: BTreeSet::new(),
                counts: [(o.to_string(), n)].into(),
            };
            let apo = |deals: Vec<Vec<String>>| ApoDay {
                at,
                covered: BTreeSet::new(),
                deals,
            };
            // 10/07 と 10/08 は数え終わり、10/06 は総架電だけ、10/09 は何もない
            g.dials.insert(d(7), dial("1", 100));
            g.dials.insert(d(8), dial("1", 50));
            g.dials.insert(d(6), dial("1", 999));
            g.apo.insert(d(7), apo(vec![vec!["1".into()]]));
            g.apo.insert(d(8), apo(vec![]));
        }
        let s = c.snapshot(d(6), d(9));
        assert_eq!(s.dials.get("1"), Some(&150), "数え中の日の 999 は含めない");
        assert_eq!(s.deals, vec![vec!["1".to_string()]]);
        assert_eq!(s.counting, vec![d(6), d(9)]);
        // 1 日だけの要求は、他の日の数え中に影響されない
        let one = c.snapshot(d(7), d(7));
        assert!(one.counting.is_empty());
        assert_eq!(one.dials.get("1"), Some(&100));
    }

    #[test]
    fn dial_plan_reads_newest_day_first() {
        let d = |n: u32| NaiveDate::from_ymd_opt(2026, 10, n).unwrap();
        let now = DateTime::parse_from_rfc3339("2026-10-09T10:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        let plan = plan_dials(&BTreeMap::new(), &["1".to_string()], d(6), d(9), now);
        assert_eq!(
            plan.iter().map(|(day, _)| *day).collect::<Vec<_>>(),
            vec![d(9), d(8), d(7), d(6)]
        );
    }
}
