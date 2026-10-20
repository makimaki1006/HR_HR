//! アポ数を日ごとに数えてキャッシュする (架電数は営業KPI のシートから読む。`dials.rs`)。
//!
//! アポの定義 (BPO と新規営業で付け方が違う。ステージは営業KPI と同じ `ST_APO` と `ST_APO_BPO`):
//! - 新規営業: 取引の担当者 `hubspot_owner_id` に付ける (営業KPI と同じ。BPO 経由かどうかによらない)。
//! - BPO: BPO 経由の取引だけ、アポ獲得者 `bpo_26` (空なら取引の担当者) に付ける。BPO 経由かは営業KPI の
//!   `is_bpo` と同じ比較で、`bpo_appo_date` が「アポ日確定に入った日の月」に入っているもの。`bpo_26` と
//!   `bpo_appo_date` は一度 BPO がアポを取ると取引に残り続けるので、新規営業が取り直したアポ
//!   (`ST_APO` に入る) を BPO のアポに数えないために月で絞る。
//!   BPO のアポ取引の多くは担当者が新規営業の人なので、担当者に付けると BPO の人のアポが 0 になる。
//!
//! HubSpot の呼び出しは、まだ持っていない日だけ (確定した日は読み直さない。今日と昨日は 5 分で読み直す)。
//! 持っていない日の範囲を 2 ステージ分まとめて 1 回ずつ (数百件で Search 2〜4 回)。
//! 1 回の読み込みの Search は `MAX_SEARCHES` まで。全部 `Background` 優先度。

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::{json, Value};

use crate::crm::caller_metrics::{appointment_owner, flt, jst_day, APPOINTER_PROPERTY, PAGE};
use crate::crm::caller_metrics_month::jst_midnight_ms;
use crate::handlers::sales_kpi::hubspot_source::jst_text;
use crate::handlers::sales_kpi::{bpo_in_window, ST_APO, ST_APO_BPO};
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

/// 取引 1 件のアポを付ける担当者 (0〜2 人)。新規営業の人には、BPO 経由かどうかによらず取引の担当者
/// (営業KPI と同じ)。BPO の人には、BPO 経由 (`via_bpo`) のときだけアポ獲得者 (空なら担当者)。
pub fn credit(
    appointer_or_owner: Option<&str>,
    deal_owner: Option<&str>,
    via_bpo: bool,
    bpo: &HashSet<&str>,
    sales: &HashSet<&str>,
) -> Vec<String> {
    let mut out = Vec::new();
    if let Some(a) = appointer_or_owner.filter(|a| via_bpo && bpo.contains(a)) {
        out.push(a.to_string());
    }
    if let Some(o) = deal_owner.filter(|o| sales.contains(o)) {
        if !out.iter().any(|x| x == o) {
            out.push(o.to_string());
        }
    }
    out
}

/// アポ日確定に入った日の月 `[月初, 翌月初)` (`yyyy-MM-dd`)。営業KPI の当月窓と同じ
fn month_window(day: NaiveDate) -> (String, String) {
    let first = day.with_day(1).unwrap_or(day);
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    }
    .unwrap_or(first);
    (
        first.format("%Y-%m-%d").to_string(),
        next.format("%Y-%m-%d").to_string(),
    )
}

/// 取引のアポが BPO 経由か。`bpo_appo_date` は一度 BPO が取ると残り続けるので、入った日の月に
/// 入っているものだけ BPO 経由とする (営業KPI の `is_bpo` と同じ比較)。
fn via_bpo(p: &Value, day: NaiveDate) -> bool {
    let (lo, hi) = month_window(day);
    bpo_in_window(
        &jst_text(p.get("bpo_appo_date").and_then(Value::as_str)),
        &lo,
        &hi,
    )
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

/// キャッシュにある分だけのアポ (HubSpot は呼ばない)。`counting` は、まだ数え終わっていない日
#[derive(Debug, Default)]
pub struct Snapshot {
    pub deals: Vec<Vec<String>>,
    pub counting: Vec<NaiveDate>,
}

impl CountsCache {
    /// 期間のアポ (取引ごとに付いた担当者の一覧)。持っている日だけで答え、持っていない日は `counting` に入れる。
    /// 今日と昨日の古い数字は、読み直しが済むまでそのまま使う。
    pub fn snapshot(&self, from: NaiveDate, to: NaiveDate) -> Snapshot {
        let g = self.inner.lock().unwrap();
        let mut out = Snapshot::default();
        for d in days(from, to) {
            match g.apo.get(&d) {
                Some(e) => out.deals.extend(e.deals.iter().cloned()),
                None => out.counting.push(d),
            }
        }
        out
    }

    /// 持っていない日・古い日のアポを HubSpot から読んで残す。同時に読むのは 1 本。
    /// `wait` が false で他が読んでいる最中なら、何もしない。
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
        let need_apo = {
            let g = self.inner.lock().unwrap();
            plan_apo(&g.apo, &owners, from, to, now)
        };
        if owners.is_empty() {
            return Ok(());
        }
        let mut searches = 0u32;
        if let (Some(first), Some(last)) = (need_apo.first(), need_apo.last()) {
            let by_day =
                fetch_appointments(&client.background(), team, *first, *last, &mut searches)
                    .await?;
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
                "properties": ["hubspot_owner_id", APPOINTER_PROPERTY, "bpo_appo_date", prop],
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
                let who = credit(appointment_owner(p), owner, via_bpo(p, day), &bpo, &sales);
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
            credit(Some("B1"), Some("S1"), true, &bpo, &sales),
            vec!["B1", "S1"]
        );
        // BPO 経由でなければ BPO の人には付けず、担当者にだけ付ける
        assert_eq!(
            credit(Some("B1"), Some("S1"), false, &bpo, &sales),
            vec!["S1"]
        );
        // 担当者が BPO 以外の別の人 → 対象外
        assert!(credit(Some("X"), Some("Y"), true, &bpo, &sales).is_empty());
        // 担当者が新規営業でアポ獲得者が空 (担当者が代わりに入る) → 1 人に 1 回
        assert_eq!(
            credit(Some("S1"), Some("S1"), true, &bpo, &sales),
            vec!["S1"]
        );
        // BPO 本人が取引の担当者
        assert_eq!(
            credit(Some("B1"), Some("B1"), true, &bpo, &sales),
            vec!["B1"]
        );
    }

    /// 取引 1 件が「入った日 `day`」「bpo_appo_date `bpo_date`」のとき、BPO の人 B1・営業の人 S1 に付く数
    fn credited(day: &str, bpo_date: Option<&str>) -> (usize, usize) {
        let day = NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap();
        let p = json!({"hubspot_owner_id": "S1", "bpo_26": "B1", "bpo_appo_date": bpo_date});
        let who = credit(
            appointment_owner(&p),
            Some("S1"),
            via_bpo(&p, day),
            &set(&["B1"]),
            &set(&["S1"]),
        );
        (
            who.iter().filter(|x| *x == "B1").count(),
            who.iter().filter(|x| *x == "S1").count(),
        )
    }

    #[test]
    fn bpo_credit_cases() {
        // 2025-10-09 (JST) の ms
        let old = Some("1760000000000");
        assert_eq!(credited("2026-10-15", old), (0, 1));
        // (2) 入った日と同じ日 (date 型の YYYY-MM-DD) → BPO 1、担当者 1
        assert_eq!(credited("2026-10-15", Some("2026-10-15")), (1, 1));
        // (3) 前月末で、入った日が当月 1 日 → BPO 0
        assert_eq!(credited("2026-10-01", Some("2026-09-30")), (0, 1));
        // 同じ月の月初と月末は入る。bpo_appo_date が無ければ BPO 0
        assert_eq!(credited("2026-12-31", Some("2026-12-01")), (1, 1));
        assert_eq!(credited("2026-10-15", None), (0, 1));
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
            let apo = |deals: Vec<Vec<String>>| ApoDay {
                at,
                covered: BTreeSet::new(),
                deals,
            };
            g.apo.insert(d(7), apo(vec![vec!["1".into()]]));
            g.apo.insert(d(8), apo(vec![]));
        }
        let s = c.snapshot(d(6), d(9));
        assert_eq!(s.deals, vec![vec!["1".to_string()]]);
        assert_eq!(s.counting, vec![d(6), d(9)]);
        // 1 日だけの要求は、他の日の数え中に影響されない
        let one = c.snapshot(d(7), d(7));
        assert!(one.counting.is_empty());
        assert_eq!(one.deals.len(), 1);
    }
}
