//! 新規営業向けの「今月の成績」: `GET /api/crm/metrics/month` (本人) と `GET /api/crm/metrics/month/all` (管理者だけ)。
//!
//! 定義 (headless-crm-design.md §19、ADR-027):
//! - 対象 = HubSpot のチーム「新規営業」の人だけ。BPO は今日の成績のまま (`caller_metrics.rs`)。
//! - 期間 = 今月 (JST の 1 日 0 時 〜 いま)。架電数・通話時間は今日の集計と同じ定義 (Zoom 純正連携の Call、
//!   `hs_call_direction = OUTBOUND`)。30 / 90 / 360 秒の基準は出さない。
//! - アポ = 営業KPI (日次バッチ) と同じ数え方。「アポ日確定」(`ST_APO` と `ST_APO_BPO` の 2 ステージ) に今月入った取引を、
//!   取引の担当者 (`hubspot_owner_id`) に付ける。アポ獲得者 (`bpo_26`) は使わない。営業KPI の名簿の「集計対象」による除外は
//!   HR_HR では名簿を読めないため、HubSpot のチーム (新規営業) で代える。
//!
//! ## 読み方
//! - 昨日までの分は日ごとに 1 回だけ読んで確定する (担当者 IN 100 人ずつ。1 日 1 回の検索が 1 万件の上限を超えない)。
//!   起動時 (今日の集計の温めのあと) と日が変わったときに、背景の優先度で読む。確定した日は読み直さない。
//! - 今日の分は今日の集計 (`CallerMetricsState`) の入れ物をそのまま使う (新規営業の人の分だけを足す)。
//! - アポは月初から毎回まとめて読み直す (数百件、Search 3〜4 回)。結果は 5 分キャッシュ。
//! - 1 回の読み込みの Search は `MONTH_MAX_SEARCHES` まで (今日の集計の `MAX_SEARCHES` とは別)。
//!   途中で失敗しても、確定した日は残して次の試行で続きから読む。
//! - 最初の読み込みが終わるまでは `loading: true` を返す。
//!
//! **単一インスタンス前提** (今日の集計と同じ)。

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Datelike, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tower_sessions::Session;
use ts_rs::TS;

use super::caller_metrics::{
    aggregate, as_of_text, flt, jst_day, rank_of, CallerMetrics, CallerMetricsState, Warm, PAGE,
    RETRY_AFTER_FAILURE,
};
use super::owners::CrmOwner;
use super::rbac;
use super::routes::{error_json, hubspot_error_response, CrmCtx, CRM_REQUEST_DEADLINE};
use crate::handlers::sales_kpi::{ST_APO, ST_APO_BPO};
use crate::hubspot::{HubSpotClient, HubSpotError};
use crate::AppState;

/// 「今月」を出す HubSpot のチーム
pub const SALES_TEAM: &str = "新規営業";
/// 1 回の読み込み (確定していない日 + アポ) の Search の上限。今日の集計の `MAX_SEARCHES` とは別
/// (月末で 1 日 約 5 回 × 30 日 + アポ 数回 ≒ 160〜320 回の見積もり)
pub const MONTH_MAX_SEARCHES: u32 = 500;
/// アポの読み直しの最短間隔
pub const APO_TTL: Duration = Duration::seconds(300);
/// Search の `hubspot_owner_id IN` に入れる担当者の最大数
pub const OWNER_CHUNK: usize = 100;

/// 見ている人が「今月」を出すチームか
pub fn is_sales(teams: &[String]) -> bool {
    teams.iter().any(|t| t == SALES_TEAM)
}

/// 担当者の一覧から新規営業の owner ID (昇順・重複なし)
pub fn sales_owner_ids(owners: &[CrmOwner]) -> Vec<String> {
    let set: BTreeSet<String> = owners
        .iter()
        .filter(|o| is_sales(&o.teams))
        .map(|o| o.id.clone())
        .collect();
    set.into_iter().collect()
}

// ---------------------------------------------------------------------------
// 型 (React 側へ ts-rs で書き出す)
// ---------------------------------------------------------------------------

/// 1 人分の今月の数字
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct MonthMetrics {
    /// 今月の架電数 (発信。今日の分を含む)
    pub outbound: u32,
    /// そのうち今日の架電数
    pub today_outbound: u32,
    /// 通話時間が 0 より長い架電の件数
    pub talked: u32,
    /// 架電の通話時間の合計 (ミリ秒)
    #[ts(type = "number")]
    pub total_duration_ms: i64,
    /// 合計 ÷ talked (talked 0 は null)
    #[ts(type = "number | null")]
    pub avg_duration_ms: Option<i64>,
    /// 今月「アポ日確定」に入った取引の数 (取引の担当者に付ける)
    pub appointments: u32,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MonthTeamAverage {
    pub outbound: f64,
    pub total_duration_ms: f64,
    /// 通話時間の合計 ÷ 通話ありの件数の合計
    pub avg_duration_ms: Option<f64>,
    pub appointments: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct MonthRanks {
    pub outbound: Option<u32>,
    pub total_duration: Option<u32>,
    pub appointments: Option<u32>,
    /// 1 件平均の通話時間 (長い順。帯の項目を自分で選んだときに出す)。通話のない人は比べない
    pub avg_duration: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MyMonthMetrics {
    pub owner_id: String,
    pub metrics: MonthMetrics,
    pub rank: MonthRanks,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct MonthMetricsResponse {
    /// true のとき、今月の最初の読み込みが背景で進んでいて数字はまだ無い
    pub loading: bool,
    pub as_of: String,
    /// 集計の月 (JST、YYYY-MM)
    pub month: String,
    pub stale: bool,
    pub me: Option<MyMonthMetrics>,
    pub team_avg: Option<MonthTeamAverage>,
    /// 比べる集団 (今月 1 件以上架電した新規営業の人) の人数
    pub population_size: u32,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct MonthCallerRow {
    pub owner_id: String,
    pub name: String,
    pub teams: Vec<String>,
    pub metrics: MonthMetrics,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct MonthAllResponse {
    pub loading: bool,
    pub as_of: String,
    pub month: String,
    pub stale: bool,
    pub callers: Vec<MonthCallerRow>,
}

// ---------------------------------------------------------------------------
// 純粋な計算
// ---------------------------------------------------------------------------

/// 1 人・1 日分の確定した数字
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DayTotals {
    pub outbound: u32,
    pub talked: u32,
    pub total_duration_ms: i64,
}

/// JST のその日の 0 時 (UTC エポック ms)
pub fn jst_midnight_ms(date: NaiveDate) -> i64 {
    date.and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis()
        - 9 * 3_600_000
}

/// 今月の [1 日, 翌月 1 日) (JST) の日付
pub fn month_bounds(today: NaiveDate) -> (NaiveDate, NaiveDate) {
    let first = today.with_day(1).expect("first of month");
    let next = if first.month() == 12 {
        NaiveDate::from_ymd_opt(first.year() + 1, 1, 1)
    } else {
        NaiveDate::from_ymd_opt(first.year(), first.month() + 1, 1)
    }
    .expect("next month");
    (first, next)
}

fn month_text(first: NaiveDate) -> String {
    format!("{:04}-{:02}", first.year(), first.month())
}

/// 確定した日々 + 今日の集計 + 今月のアポ → 人ごとの今月の数字。新規営業の人だけが入る
pub fn combine(
    days: &BTreeMap<NaiveDate, HashMap<String, DayTotals>>,
    today: &HashMap<String, CallerMetrics>,
    apo: &HashMap<String, u32>,
    owners: &[String],
) -> HashMap<String, MonthMetrics> {
    let members: HashSet<&str> = owners.iter().map(String::as_str).collect();
    let mut out: HashMap<String, MonthMetrics> = HashMap::new();
    for day in days.values() {
        for (o, t) in day {
            if !members.contains(o.as_str()) {
                continue;
            }
            let m = out.entry(o.clone()).or_default();
            m.outbound += t.outbound;
            m.talked += t.talked;
            m.total_duration_ms += t.total_duration_ms;
        }
    }
    for (o, t) in today {
        if !members.contains(o.as_str()) {
            continue;
        }
        let m = out.entry(o.clone()).or_default();
        m.outbound += t.outbound;
        m.today_outbound = t.outbound;
        m.talked += t.talked;
        m.total_duration_ms += t.total_duration_ms;
    }
    for (o, n) in apo {
        if members.contains(o.as_str()) {
            out.entry(o.clone()).or_default().appointments = *n;
        }
    }
    for m in out.values_mut() {
        m.avg_duration_ms = (m.talked > 0).then(|| m.total_duration_ms / i64::from(m.talked));
    }
    out
}

fn month_is_caller(m: &MonthMetrics) -> bool {
    m.outbound >= 1
}

/// 集団 (今月 1 件以上架電した新規営業の人) の 1 人あたり平均。空なら None
pub fn month_average(pop: &[&MonthMetrics]) -> Option<MonthTeamAverage> {
    if pop.is_empty() {
        return None;
    }
    let n = pop.len() as f64;
    let talked: f64 = pop.iter().map(|m| f64::from(m.talked)).sum();
    let dur: f64 = pop.iter().map(|m| m.total_duration_ms as f64).sum();
    Some(MonthTeamAverage {
        outbound: pop.iter().map(|m| f64::from(m.outbound)).sum::<f64>() / n,
        total_duration_ms: dur / n,
        avg_duration_ms: (talked > 0.0).then(|| dur / talked),
        appointments: pop.iter().map(|m| f64::from(m.appointments)).sum::<f64>() / n,
    })
}

/// 本人の応答を組み立てる (純粋)。集団に入っていない人 (今月の架電 0) は順位なし
pub fn build_month(
    all: &HashMap<String, MonthMetrics>,
    my_owner: &str,
) -> (MyMonthMetrics, Option<MonthTeamAverage>, u32) {
    let pop: Vec<&MonthMetrics> = all.values().filter(|m| month_is_caller(m)).collect();
    let me = all.get(my_owner).cloned().unwrap_or_default();
    let rank = if month_is_caller(&me) {
        let r = |f: fn(&MonthMetrics) -> i64| {
            let vals: Vec<i64> = pop.iter().map(|m| f(m)).collect();
            Some(rank_of(&vals, f(&me)))
        };
        MonthRanks {
            outbound: r(|m| i64::from(m.outbound)),
            total_duration: r(|m| m.total_duration_ms),
            appointments: r(|m| i64::from(m.appointments)),
            avg_duration: me.avg_duration_ms.map(|mine| {
                let vals: Vec<i64> = pop.iter().filter_map(|m| m.avg_duration_ms).collect();
                rank_of(&vals, mine)
            }),
        }
    } else {
        MonthRanks::default()
    };
    (
        MyMonthMetrics {
            owner_id: my_owner.to_string(),
            metrics: me,
            rank,
        },
        month_average(&pop),
        pop.len() as u32,
    )
}

// ---------------------------------------------------------------------------
// HubSpot からの読み取り
// ---------------------------------------------------------------------------

/// 1 日分 (JST) の新規営業の Call を読んで人ごとに数える。担当者は 100 人ずつ
async fn fetch_day(
    bg: &HubSpotClient,
    owners: &[String],
    day: NaiveDate,
    searches: &mut u32,
    max_searches: u32,
) -> Result<HashMap<String, DayTotals>, HubSpotError> {
    let a = jst_midnight_ms(day);
    let b = a + 86_400_000;
    let mut rows = Vec::new();
    for chunk in owners.chunks(OWNER_CHUNK) {
        rows.extend(
            super::caller_metrics::fetch_calls(
                bg,
                "hs_timestamp",
                a,
                b,
                None,
                Some(chunk),
                searches,
                max_searches,
            )
            .await?,
        );
    }
    Ok(aggregate(rows.iter(), &HashMap::new())
        .into_iter()
        .map(|(o, m)| {
            (
                o,
                DayTotals {
                    outbound: m.outbound,
                    talked: m.talked,
                    total_duration_ms: m.total_duration_ms,
                },
            )
        })
        .collect())
}

/// 今月「アポ日確定」(2 ステージ) に入った取引を、担当者 (新規営業の人だけ) ごとに数える。同じ取引は 1 回
async fn fetch_month_appointments(
    bg: &HubSpotClient,
    from: i64,
    to: i64,
    owners: &[String],
    searches: &mut u32,
    max_searches: u32,
) -> Result<HashMap<String, u32>, HubSpotError> {
    let members: HashSet<&str> = owners.iter().map(String::as_str).collect();
    let mut by: HashMap<String, u32> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for stage in [ST_APO, ST_APO_BPO] {
        let prop = format!("hs_v2_date_entered_{stage}");
        let mut after: Option<String> = None;
        loop {
            *searches += 1;
            if *searches > max_searches {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let mut body = json!({
                "filterGroups": [{"filters": [
                    flt(&prop, "GTE", json!(from.to_string())),
                    flt(&prop, "LT", json!(to.to_string())),
                ]}],
                "properties": ["hubspot_owner_id", prop],
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
                let owner = r
                    .pointer("/properties/hubspot_owner_id")
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|s| !s.is_empty());
                if let Some(o) = owner.filter(|o| members.contains(o)) {
                    *by.entry(o.to_string()).or_default() += 1;
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
    Ok(by)
}

// ---------------------------------------------------------------------------
// プロセス内の状態
// ---------------------------------------------------------------------------

#[derive(Default)]
struct MonthStore {
    /// 集計の月 (1 日)
    month: Option<NaiveDate>,
    /// 読んだ時の新規営業の owner ID (昇順)。変わったら読み直す
    owners: Vec<String>,
    /// 確定した日 (昨日まで)
    days: BTreeMap<NaiveDate, HashMap<String, DayTotals>>,
    /// (読んだ時刻, 担当者ごとのアポ)
    apo: Option<(DateTime<Utc>, HashMap<String, u32>)>,
    /// アポの読み直しに失敗したあと次に試す時刻
    apo_retry_at: Option<DateTime<Utc>>,
}

impl MonthStore {
    fn reset_if_changed(&mut self, first: NaiveDate, owners: &[String]) {
        if self.month != Some(first) || self.owners != owners {
            *self = MonthStore {
                month: Some(first),
                owners: owners.to_vec(),
                ..MonthStore::default()
            };
        }
    }

    /// 昨日までのうち、まだ確定していない日
    fn missing_days(&self, first: NaiveDate, today: NaiveDate) -> Vec<NaiveDate> {
        first
            .iter_days()
            .take_while(|d| *d < today)
            .filter(|d| !self.days.contains_key(d))
            .collect()
    }

    fn is_ready(&self, first: NaiveDate, today: NaiveDate, owners: &[String]) -> bool {
        self.month == Some(first)
            && self.owners == owners
            && self.apo.is_some()
            && self.missing_days(first, today).is_empty()
    }
}

#[derive(Default)]
pub struct MonthState {
    store: tokio::sync::Mutex<MonthStore>,
    next_try: Mutex<Option<DateTime<Utc>>>,
    /// 最初の読み込み (背景) が走っている間 true。二重に走らせない
    warming: AtomicBool,
    /// 1 回の読み込みの Search の上限 (0 は `MONTH_MAX_SEARCHES`。テストで小さくする)
    max_searches: AtomicU32,
}

/// 今月の読み込みの結果
pub enum MonthWarm {
    /// (人ごとの今月の数字, 今日の集計を読んだ時刻, 月の 1 日, 古いまま返しているか)
    Ready {
        all: HashMap<String, MonthMetrics>,
        as_of: DateTime<Utc>,
        month: NaiveDate,
        stale: bool,
    },
    Loading,
}

struct WarmGuard(Arc<MonthState>);
impl Drop for WarmGuard {
    fn drop(&mut self) {
        self.0.warming.store(false, Ordering::Release);
    }
}

impl MonthState {
    fn max(&self) -> u32 {
        match self.max_searches.load(Ordering::Relaxed) {
            0 => MONTH_MAX_SEARCHES,
            n => n,
        }
    }

    #[cfg(test)]
    pub(super) fn set_max_searches(&self, n: u32) {
        self.max_searches.store(n, Ordering::Relaxed);
    }

    fn backing_off(&self, now: DateTime<Utc>) -> bool {
        self.next_try
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|t| now < t)
    }

    fn back_off(&self, now: DateTime<Utc>) {
        if let Ok(mut g) = self.next_try.lock() {
            *g = Some(now + RETRY_AFTER_FAILURE);
        }
    }

    /// 確定していない日とアポを読む。成功した日は途中で失敗しても残す
    async fn fill(
        &self,
        client: &HubSpotClient,
        owners: &[String],
        now: DateTime<Utc>,
    ) -> Result<(), HubSpotError> {
        let (today, _, _) = jst_day(now);
        let (first, next) = month_bounds(today);
        let bg = client.background();
        let mut store = self.store.lock().await;
        store.reset_if_changed(first, owners);
        let mut searches = 0u32;
        if !owners.is_empty() {
            for day in store.missing_days(first, today) {
                let totals = fetch_day(&bg, owners, day, &mut searches, self.max()).await?;
                store.days.insert(day, totals);
            }
        } else {
            for day in store.missing_days(first, today) {
                store.days.insert(day, HashMap::new());
            }
        }
        if store.apo.is_none() {
            let apo = if owners.is_empty() {
                HashMap::new()
            } else {
                fetch_month_appointments(
                    &bg,
                    jst_midnight_ms(first),
                    jst_midnight_ms(next),
                    owners,
                    &mut searches,
                    self.max(),
                )
                .await?
            };
            store.apo = Some((now, apo));
        }
        tracing::info!(
            searches,
            days = store.days.len(),
            "crm caller metrics month filled"
        );
        Ok(())
    }

    /// アポの読み直し (5 分)。失敗したら前の値を残して `false` を返す
    async fn refresh_apo(
        &self,
        client: &HubSpotClient,
        owners: &[String],
        now: DateTime<Utc>,
    ) -> bool {
        let (today, _, _) = jst_day(now);
        let (first, next) = month_bounds(today);
        // 読んでいる間はロックを持たない (他の要求が「読み込み中」に見えないように)。
        // 先に次の試行の時刻を置いて、同時に読み直す要求を 1 本にする
        {
            let mut store = self.store.lock().await;
            let due = store
                .apo
                .as_ref()
                .is_some_and(|(at, _)| now - *at >= APO_TTL)
                && store.apo_retry_at.is_none_or(|t| now >= t);
            if !due || owners.is_empty() {
                return true;
            }
            store.apo_retry_at = Some(now + RETRY_AFTER_FAILURE);
        }
        let mut searches = 0u32;
        let res = fetch_month_appointments(
            &client.background(),
            jst_midnight_ms(first),
            jst_midnight_ms(next),
            owners,
            &mut searches,
            self.max(),
        )
        .await;
        let mut store = self.store.lock().await;
        match res {
            Ok(apo) => {
                store.apo = Some((now, apo));
                store.apo_retry_at = None;
                true
            }
            Err(e) => {
                tracing::warn!(
                    error_kind = e.error_kind(),
                    "crm caller metrics month appointments refresh failed"
                );
                false
            }
        }
    }
}

impl CallerMetricsState {
    /// 新規営業の「今月」を返す。最初の読み込みが終わっていなければ背景で 1 本だけ走らせてすぐ `Loading`。
    /// `owners` は新規営業の owner ID (昇順・重複なし)。
    pub async fn get_month_or_warm(
        self: &Arc<Self>,
        client: &Arc<HubSpotClient>,
        owners: &[String],
        now: DateTime<Utc>,
    ) -> Result<MonthWarm, HubSpotError> {
        let (today, _, _) = jst_day(now);
        let (first, _) = month_bounds(today);
        let ms = self.month.clone();
        let ready = ms
            .store
            .try_lock()
            .is_ok_and(|s| s.is_ready(first, today, owners));
        if !ready {
            if ms.backing_off(now) {
                return Err(HubSpotError::Busy);
            }
            if ms
                .warming
                .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                let ms2 = ms.clone();
                let client = client.clone();
                let owners = owners.to_vec();
                tokio::spawn(async move {
                    let _guard = WarmGuard(ms2.clone());
                    if let Err(e) = ms2.fill(&client, &owners, now).await {
                        tracing::warn!(
                            error_kind = e.error_kind(),
                            "crm caller metrics month fill failed"
                        );
                        ms2.back_off(now);
                    }
                });
            }
            return Ok(MonthWarm::Loading);
        }
        let (snap, stale_today) = match self.get_or_warm(client, now).await? {
            Warm::Ready(s, st) => (s, st),
            Warm::Loading => return Ok(MonthWarm::Loading),
        };
        let apo_ok = ms.refresh_apo(client, owners, now).await;
        let store = ms.store.lock().await;
        let empty = HashMap::new();
        let apo = store.apo.as_ref().map_or(&empty, |(_, m)| m);
        let all = combine(&store.days, &snap.all, apo, owners);
        Ok(MonthWarm::Ready {
            all,
            as_of: snap.as_of,
            month: first,
            stale: stale_today || !apo_ok,
        })
    }
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn loading_text(first: NaiveDate) -> String {
    month_text(first)
}

pub(super) async fn get_month(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(d) => return d.into_response(),
    };
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let email = principal.email.clone().unwrap_or_default();
    let now = ctx.queue.now();
    let (today, _, _) = jst_day(now);
    let (first, _) = month_bounds(today);
    let work = async {
        let me_ref = ctx.queue.owner_info_for(&client, &email).await?;
        let Some(me) = me_ref.filter(|o| is_sales(&o.teams)) else {
            return Ok::<_, HubSpotError>(None);
        };
        let list = ctx.queue.owner_list.get(&client).await?;
        let owners = sales_owner_ids(&list.owners);
        let warm = ctx
            .caller_metrics
            .get_month_or_warm(&client, &owners, now)
            .await?;
        Ok(Some((me.id.clone(), warm)))
    };
    let (my_id, warm) = match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
        Err(_) => return super::routes::timeout_response(),
        Ok(Err(e)) => return hubspot_error_response(&e),
        Ok(Ok(None)) => {
            return Json(MonthMetricsResponse {
                loading: false,
                as_of: String::new(),
                month: loading_text(first),
                stale: false,
                me: None,
                team_avg: None,
                population_size: 0,
                message: Some("今月の成績は、新規営業のチームの人だけに出しています。".to_string()),
            })
            .into_response();
        }
        Ok(Ok(Some(v))) => v,
    };
    match warm {
        MonthWarm::Loading => Json(MonthMetricsResponse {
            loading: true,
            as_of: String::new(),
            month: loading_text(first),
            stale: false,
            me: None,
            team_avg: None,
            population_size: 0,
            message: None,
        })
        .into_response(),
        MonthWarm::Ready {
            all,
            as_of,
            month,
            stale,
        } => {
            let (me, team_avg, population_size) = build_month(&all, &my_id);
            Json(MonthMetricsResponse {
                loading: false,
                as_of: as_of_text(as_of),
                month: month_text(month),
                stale,
                me: Some(me),
                team_avg,
                population_size,
                message: None,
            })
            .into_response()
        }
    }
}

pub(super) async fn get_month_all(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(d) => return d.into_response(),
    };
    if !rbac::resolve_role(&principal).is_admin() {
        return error_json(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let now = ctx.queue.now();
    let (today, _, _) = jst_day(now);
    let (first, _) = month_bounds(today);
    let work = async {
        let list = ctx.queue.owner_list.get(&client).await?;
        let owners = sales_owner_ids(&list.owners);
        let warm = ctx
            .caller_metrics
            .get_month_or_warm(&client, &owners, now)
            .await?;
        Ok::<_, HubSpotError>((list, warm))
    };
    let (list, warm) = match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
        Err(_) => return super::routes::timeout_response(),
        Ok(Err(e)) => return hubspot_error_response(&e),
        Ok(Ok(v)) => v,
    };
    let (all, as_of, month, stale) = match warm {
        MonthWarm::Loading => {
            return Json(MonthAllResponse {
                loading: true,
                as_of: String::new(),
                month: loading_text(first),
                stale: false,
                callers: Vec::new(),
            })
            .into_response()
        }
        MonthWarm::Ready {
            all,
            as_of,
            month,
            stale,
        } => (all, as_of, month, stale),
    };
    let info: HashMap<&str, &CrmOwner> = list.owners.iter().map(|o| (o.id.as_str(), o)).collect();
    let mut callers: Vec<MonthCallerRow> = all
        .into_iter()
        .filter(|(_, m)| m.outbound > 0 || m.appointments > 0)
        .map(|(id, m)| MonthCallerRow {
            name: info
                .get(id.as_str())
                .map_or_else(|| "(名前を取得できません)".to_string(), |o| o.name.clone()),
            teams: info
                .get(id.as_str())
                .map(|o| o.teams.clone())
                .unwrap_or_default(),
            owner_id: id,
            metrics: m,
        })
        .collect();
    callers.sort_by(|a, b| {
        b.metrics
            .outbound
            .cmp(&a.metrics.outbound)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    Json(MonthAllResponse {
        loading: false,
        as_of: as_of_text(as_of),
        month: month_text(month),
        stale,
        callers,
    })
    .into_response()
}
