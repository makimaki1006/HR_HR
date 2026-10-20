//! アポ数を日ごとに数えてキャッシュする (架電数は営業KPI のシートから読む。`dials.rs`)。
//!
//! アポの定義 (どの取引も 1 人だけに付ける。ステージは営業KPI と同じ `ST_APO` と `ST_APO_BPO`):
//! - BPO のアポ日確定 (`ST_APO_BPO`): `bpo_appo_date` が「ステージに入った日の月」に入っていて `bpo_26` が
//!   空でなければ、`bpo_26` に付ける (営業KPI の `is_bpo` と同じ比較)。無ければ取引の担当者に付ける。
//! - 新規営業のアポ日確定 (`ST_APO`): `aposyutokubi` が「ステージに入った日の月」に入っていて `aposyutokusya` が
//!   空でなければ、`aposyutokusya` に付ける。無ければ取引の担当者に付ける。
//! - どちらも、`bpo_26` と `aposyutokusya` は一度入ると取引に残り続けるので月で絞る。アポを取った人は
//!   対象者の一覧に載っていなくてもその人に付ける (一覧に無ければ画面に出ないだけで、担当者には戻さない)。
//!   担当者に付けるのは、取った人がいない (空・日付が別の月) ときだけ。
//!
//! HubSpot の呼び出しは、まだ持っていない日だけ (確定した日は読み直さない。今日と昨日は 5 分で読み直す)。
//! 持っていない日の範囲を 2 ステージ分まとめて 1 回ずつ (数百件で Search 2〜4 回)。
//! 1 回の読み込みの Search は `MAX_SEARCHES` まで。全部 `Background` 優先度。

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde_json::{json, Value};

use crate::crm::caller_metrics::{flt, jst_day, APPOINTER_PROPERTY, PAGE};
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

/// 新規営業ステージ (`ST_APO`) に入った取引のアポ取得者。アポ取得日が入った日の月に入り、取得者が空でないときだけ
fn acquirer(p: &Value, day: NaiveDate) -> Option<String> {
    let who = p
        .get("aposyutokusya")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    let (lo, hi) = month_window(day);
    bpo_in_window(
        &jst_text(p.get("aposyutokubi").and_then(Value::as_str)),
        &lo,
        &hi,
    )
    .then(|| who.to_string())
}

/// BPO のアポ日確定 (`ST_APO_BPO`) の取引のアポ獲得者。`bpo_appo_date` が入った日の月に入り、`bpo_26` が空でないときだけ
fn bpo_taker(p: &Value, day: NaiveDate) -> Option<String> {
    let who = p
        .get("bpo_26")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())?;
    via_bpo(p, day).then(|| who.to_string())
}

/// 取引 1 件のアポを付ける人 (必ず 1 人以下)。アポを取った人 (BPO は `bpo_26`、新規営業は `aposyutokusya`。
/// どちらも入った日の月に日付があるときだけ) に付ける。対象者の一覧に載っていなくてもその人に付ける
/// (一覧に無ければ画面に出ないだけで、担当者には戻さない)。取った人がいなければ取引の担当者に付ける (同じく一覧に無ければ画面に出ないだけ)。
pub fn credit_for_stage(
    stage: &str,
    p: &Value,
    day: NaiveDate,
    owner: Option<&str>,
) -> Vec<String> {
    let taker = if stage == ST_APO {
        acquirer(p, day)
    } else {
        bpo_taker(p, day)
    };
    if let Some(t) = taker {
        return vec![t];
    }
    owner.map(|o| vec![o.to_string()]).unwrap_or_default()
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
                "properties": ["hubspot_owner_id", APPOINTER_PROPERTY, "bpo_appo_date", "aposyutokusya", "aposyutokubi", prop],
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
                let who = credit_for_stage(stage, p, day, owner);
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

    fn day() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 15).unwrap()
    }

    /// 新規営業のステージ (ST_APO)。担当者 S1。営業の人は S1・S2
    fn apo_credit(who: Option<&str>, date: Option<&str>) -> Vec<String> {
        let p = json!({"hubspot_owner_id": "S1", "aposyutokusya": who, "aposyutokubi": date});
        credit_for_stage(ST_APO, &p, day(), Some("S1"))
    }

    /// BPO のステージ (ST_APO_BPO)。担当者 S1 (新規営業)
    fn bpo_credit(who: Option<&str>, date: Option<&str>) -> Vec<String> {
        let p = json!({"hubspot_owner_id": "S1", "bpo_26": who, "bpo_appo_date": date});
        credit_for_stage(ST_APO_BPO, &p, day(), Some("S1"))
    }

    #[test]
    fn new_sales_appointment_goes_to_the_acquirer_in_the_same_month() {
        assert_eq!(apo_credit(Some("S2"), Some("2026-10-03")), vec!["S2"]);
        assert_eq!(apo_credit(Some("S2"), Some("2026-10-01")), vec!["S2"]);
        assert_eq!(apo_credit(Some("S2"), Some("2026-10-31")), vec!["S2"]);
        // 取得日が前の月 → 担当者
        assert_eq!(apo_credit(Some("S2"), Some("2026-09-30")), vec!["S1"]);
        // 取得者が空・取得日なし → 担当者
        assert_eq!(apo_credit(Some(""), Some("2026-10-03")), vec!["S1"]);
        assert_eq!(apo_credit(None, None), vec!["S1"]);
        assert_eq!(apo_credit(Some("S2"), None), vec!["S1"]);
        // 取得者 = 担当者でも 1 回
        assert_eq!(apo_credit(Some("S1"), Some("2026-10-03")), vec!["S1"]);
        // 対象者の一覧に無い人でも取得者に付く (担当者には付かない)
        assert_eq!(apo_credit(Some("Z9"), Some("2026-10-03")), vec!["Z9"]);
    }

    #[test]
    fn bpo_appointment_goes_only_to_the_bpo_taker() {
        // 同じ月の bpo_26 → B1 だけ (担当者 S1 には付けない)
        assert_eq!(bpo_credit(Some("B1"), Some("2026-10-15")), vec!["B1"]);
        assert_eq!(bpo_credit(Some("B1"), Some("2026-10-01")), vec!["B1"]);
        assert_eq!(bpo_credit(Some("B1"), Some("2026-10-31")), vec!["B1"]);
        // 取得日が前の月 (古い値が残っている) → 担当者
        assert_eq!(bpo_credit(Some("B1"), Some("2026-09-30")), vec!["S1"]);
        // 2025-10-09 (JST) の ms → 担当者
        assert_eq!(bpo_credit(Some("B1"), Some("1760000000000")), vec!["S1"]);
        // bpo_26 が空・日付なし → 担当者
        assert_eq!(bpo_credit(Some(""), Some("2026-10-15")), vec!["S1"]);
        assert_eq!(bpo_credit(Some("B1"), None), vec!["S1"]);
        // 一覧に無い人でも bpo_26 に付く
        assert_eq!(bpo_credit(Some("Z9"), Some("2026-10-15")), vec!["Z9"]);
        // 取った人がいなければ、担当者がどのチームでも担当者に付く
        let p = json!({"hubspot_owner_id": "X", "bpo_26": null, "bpo_appo_date": null});
        assert_eq!(
            credit_for_stage(ST_APO_BPO, &p, day(), Some("X")),
            vec!["X".to_string()]
        );
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
