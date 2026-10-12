//! 「今日の架電成績」: `GET /api/crm/metrics/today` (本人) と `GET /api/crm/metrics/today/all` (管理者だけ)。
//!
//! 定義 (headless-crm-design.md §19、ADR-027):
//! - 集計の単位は HubSpot の担当者 (owner)。期間は今日 (JST 0 時 〜 いま)。Zoom の純正連携が作った Call
//!   (`hs_call_source = INTEGRATIONS_PLATFORM`) だけを数える。
//! - 架電数 = `hs_call_direction = OUTBOUND` の件数。着信 (INBOUND) は `inbound` に別に数え、架電数に混ぜない。
//! - 通話 30 / 90 / 360 秒以上 = 通話時間 (`hs_call_duration`、ミリ秒) がちょうどその秒数以上の架電の件数。
//!   割合 = その件数 ÷ 架電数 (件数から毎回計算。架電数 0 は null)。「接続率」「接触率」とは呼ばない。
//! - 通話時間の合計 = 架電の通話時間の合計。平均 = 合計 ÷ 通話時間が 0 より長い架電の件数 (無ければ null)。
//! - アポ数 = 「アポ日確定」ステージに今日入った取引の数。取引の担当者に付ける (営業KPI と同じ定義)。
//! - 比べる集団 = 今日 1 件以上架電した人のうち、見ている人と同じ HubSpot チームの人 (チームが無い・
//!   一覧が取れないときは「今日架電した全員」)。平均は集団の人数で割った 1 人あたり (割合は件数の合計から)。
//! - 順位は 1 が最良 (多い順)。同じ値は同順位 (1, 1, 3)。集団に入っていない人 (今日の架電が 0) は順位なし。
//!
//! ## データの流れ (新しい DB は作らない。ADR-006 / ADR-027)
//! プロセス内に今日の Call を持つ。最初の要求で今日の分を全部読み (Search、200 件ずつ、1 つの窓の合計が
//! 9,000 件を超えるときは時間で半分に割る)、以降は `hs_lastmodifieddate` が前回の読み始めの 2 分前以降のものだけを読んで
//! id で上書きする。JST の 0 時を過ぎたら捨てる。読み直しは 2 分に 1 回まで (要求が来たときだけ。利用者をまたいで共有)。
//! 全部 `Background` 優先度。混んでいる・失敗したときは前回の結果を `stale: true` で返す。
//! アポの取引は 1 日 数十件なので増分にせず、毎回今日の分を読み直す (担当者の付け替えも拾える)。
//! **単一インスタンス前提** (record_lock と同じ)。複数台になると各台が別々に読む (結果は同じだが HubSpot への呼び出しが台数倍)。
//!
//! ## 将来 (作らない): 「今月」
//! 営業チーム向けの今月 = 既存の日次バッチ (昨日まで) + 今日のこの集計。`aggregate` が日付に依らない純粋な関数なので、
//! 昨日までの件数に足し合わせる形で追加できる。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex, RwLock};

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, NaiveDate, SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tower_sessions::Session;
use ts_rs::TS;

use super::rbac;
use super::routes::{error_json, hubspot_error_response, CrmCtx, CRM_REQUEST_DEADLINE};
use crate::hubspot::{HubSpotClient, HubSpotError};
use crate::AppState;

/// Zoom 純正連携が作る Call の `hs_call_source`
pub const CALL_SOURCE: &str = "INTEGRATIONS_PLATFORM";
/// 読み直しの最短間隔
pub const REFRESH_INTERVAL: Duration = Duration::seconds(120);
/// 失敗したあと次に試すまでの間隔
pub const RETRY_AFTER_FAILURE: Duration = Duration::seconds(30);
/// 増分の読み始めを前回の読み始めより戻す幅 (取りこぼし防止。id で上書きするので重複しても数が増えない)
pub const OVERLAP: Duration = Duration::seconds(120);
/// 1 つの窓の合計がこれを超えたら時間で半分に割る (Search は 1 回の検索で 10,000 件まで)
pub const SPLIT_AT: u64 = 9_000;
/// 1 回の読み直しで打つ Search の上限 (暴走の安全装置)
pub const MAX_SEARCHES: u32 = 200;
const PAGE: u32 = 200;

/// アポとして数えるステージ。(パイプライン名, ステージ ID)。営業側 `52035886` (sales_kpi の `ST_APO`) は
/// 営業向けの画面を作るときにここへ足す。
pub const APPOINTMENT_STAGES: &[(&str, &str)] = &[("bpo_リクロジ", "1095457875")];

/// 通話時間のしきい値 (ミリ秒)
pub const T30: i64 = 30_000;
pub const T90: i64 = 90_000;
pub const T360: i64 = 360_000;

// ---------------------------------------------------------------------------
// 型 (React 側へ ts-rs で書き出す)
// ---------------------------------------------------------------------------

/// 1 人分の今日の数字
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct CallerMetrics {
    /// 架電数 (発信)
    pub outbound: u32,
    /// 着信数 (架電数には含めない)
    pub inbound: u32,
    pub over_30s: u32,
    pub over_90s: u32,
    pub over_360s: u32,
    /// 件数 ÷ 架電数 (架電数 0 は null)
    pub rate_30s: Option<f64>,
    pub rate_90s: Option<f64>,
    pub rate_360s: Option<f64>,
    /// 通話時間が 0 より長い架電の件数
    pub talked: u32,
    /// 架電の通話時間の合計 (ミリ秒)
    #[ts(type = "number")]
    pub total_duration_ms: i64,
    /// 合計 ÷ talked (talked 0 は null)
    #[ts(type = "number | null")]
    pub avg_duration_ms: Option<i64>,
    pub appointments: u32,
    /// 最初・最後の架電の時刻 (RFC 3339)
    pub first_call_at: Option<String>,
    pub last_call_at: Option<String>,
}

/// 集団の 1 人あたり平均
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TeamAverage {
    pub outbound: f64,
    pub over_30s: f64,
    pub over_90s: f64,
    pub over_360s: f64,
    /// 件数の合計から計算 (架電数の合計 0 は null)
    pub rate_30s: Option<f64>,
    pub rate_90s: Option<f64>,
    pub rate_360s: Option<f64>,
    pub total_duration_ms: f64,
    /// 通話時間の合計 ÷ 通話ありの件数の合計
    pub avg_duration_ms: Option<f64>,
    pub appointments: f64,
}

/// 順位 (1 が最良。集団に入っていなければ null)
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
pub struct Ranks {
    pub outbound: Option<u32>,
    pub total_duration: Option<u32>,
    pub over_90s: Option<u32>,
    pub appointments: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct MyTodayMetrics {
    pub owner_id: String,
    pub metrics: CallerMetrics,
    pub rank: Ranks,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct TodayMetricsResponse {
    /// この数字を読んだ時刻 (RFC 3339)
    pub as_of: String,
    /// 集計の日 (JST、YYYY-MM-DD)
    pub day: String,
    /// 読み直しに失敗していて、前回の結果を返している
    pub stale: bool,
    pub me: Option<MyTodayMetrics>,
    pub team_avg: Option<TeamAverage>,
    pub population_size: u32,
    /// "team" = 同じチームの人、"all" = 今日架電した全員
    pub population: String,
    pub team_names: Vec<String>,
    /// me が null のときの理由 (そのまま画面に出せる文)
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CallerRowOut {
    pub owner_id: String,
    pub name: String,
    pub teams: Vec<String>,
    pub metrics: CallerMetrics,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct AllCallersResponse {
    pub as_of: String,
    pub day: String,
    pub stale: bool,
    /// 絞り込み用のチーム名の一覧
    pub teams: Vec<String>,
    pub callers: Vec<CallerRowOut>,
}

// ---------------------------------------------------------------------------
// 純粋な計算
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    Outbound,
    Inbound,
}

/// 集計に使う Call の最小の形
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallRow {
    pub id: String,
    pub owner_id: String,
    pub ts_ms: i64,
    pub duration_ms: i64,
    pub direction: Direction,
}

/// JST の暦日と、その日の [開始, 終了) の UTC エポック ms
pub fn jst_day(now: DateTime<Utc>) -> (NaiveDate, i64, i64) {
    let jst = now + Duration::hours(9);
    let date = jst.date_naive();
    let start = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight")
        .and_utc()
        .timestamp_millis()
        - 9 * 3_600_000;
    (date, start, start + 86_400_000)
}

fn ms_to_rfc3339(ms: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp_millis(ms).map(|t| t.to_rfc3339_opts(SecondsFormat::Secs, true))
}

fn rate(count: u32, outbound: u32) -> Option<f64> {
    (outbound > 0).then(|| f64::from(count) / f64::from(outbound))
}

/// Call の一覧を人ごとに集計する。アポは `apo` (owner → 件数) から入れる。
/// 架電も着信も無く、アポだけある人も 1 行になる。
pub fn aggregate<'a>(
    calls: impl IntoIterator<Item = &'a CallRow>,
    apo: &HashMap<String, u32>,
) -> HashMap<String, CallerMetrics> {
    #[derive(Default)]
    struct Acc {
        m: CallerMetrics,
        first: Option<i64>,
        last: Option<i64>,
    }
    let mut by: HashMap<String, Acc> = HashMap::new();
    for c in calls {
        let a = by.entry(c.owner_id.clone()).or_default();
        match c.direction {
            Direction::Inbound => a.m.inbound += 1,
            Direction::Outbound => {
                a.m.outbound += 1;
                if c.duration_ms >= T30 {
                    a.m.over_30s += 1;
                }
                if c.duration_ms >= T90 {
                    a.m.over_90s += 1;
                }
                if c.duration_ms >= T360 {
                    a.m.over_360s += 1;
                }
                if c.duration_ms > 0 {
                    a.m.talked += 1;
                    a.m.total_duration_ms += c.duration_ms;
                }
                a.first = Some(a.first.map_or(c.ts_ms, |f| f.min(c.ts_ms)));
                a.last = Some(a.last.map_or(c.ts_ms, |l| l.max(c.ts_ms)));
            }
        }
    }
    for owner in apo.keys() {
        by.entry(owner.clone()).or_default();
    }
    by.into_iter()
        .map(|(owner, mut a)| {
            let m = &mut a.m;
            m.rate_30s = rate(m.over_30s, m.outbound);
            m.rate_90s = rate(m.over_90s, m.outbound);
            m.rate_360s = rate(m.over_360s, m.outbound);
            m.avg_duration_ms = (m.talked > 0).then(|| m.total_duration_ms / i64::from(m.talked));
            m.appointments = apo.get(&owner).copied().unwrap_or(0);
            m.first_call_at = a.first.and_then(ms_to_rfc3339);
            m.last_call_at = a.last.and_then(ms_to_rfc3339);
            (owner, a.m)
        })
        .collect()
}

/// 集団に入れる人 = 今日 1 件以上架電した人
pub fn is_caller(m: &CallerMetrics) -> bool {
    m.outbound >= 1
}

/// 集団の 1 人あたり平均。集団が空なら None
pub fn average(pop: &[&CallerMetrics]) -> Option<TeamAverage> {
    if pop.is_empty() {
        return None;
    }
    let n = pop.len() as f64;
    let sum = |f: fn(&CallerMetrics) -> u32| pop.iter().map(|m| f64::from(f(m))).sum::<f64>();
    let out = sum(|m| m.outbound);
    let o30 = sum(|m| m.over_30s);
    let o90 = sum(|m| m.over_90s);
    let o360 = sum(|m| m.over_360s);
    let talked = sum(|m| m.talked);
    let dur: f64 = pop.iter().map(|m| m.total_duration_ms as f64).sum();
    let r = |c: f64| (out > 0.0).then(|| c / out);
    Some(TeamAverage {
        outbound: out / n,
        over_30s: o30 / n,
        over_90s: o90 / n,
        over_360s: o360 / n,
        rate_30s: r(o30),
        rate_90s: r(o90),
        rate_360s: r(o360),
        total_duration_ms: dur / n,
        avg_duration_ms: (talked > 0.0).then(|| dur / talked),
        appointments: sum(|m| m.appointments) / n,
    })
}

/// 順位 (1 が最良 = 値が大きい。同じ値は同順位)。`value` が集団に無ければ None
pub fn rank_of(values: &[i64], mine: i64) -> u32 {
    1 + values.iter().filter(|v| **v > mine).count() as u32
}

fn ranks(pop: &[&CallerMetrics], me: &CallerMetrics) -> Ranks {
    let r = |f: fn(&CallerMetrics) -> i64| {
        let vals: Vec<i64> = pop.iter().map(|m| f(m)).collect();
        rank_of(&vals, f(me))
    };
    Ranks {
        outbound: Some(r(|m| i64::from(m.outbound))),
        total_duration: Some(r(|m| m.total_duration_ms)),
        over_90s: Some(r(|m| i64::from(m.over_90s))),
        appointments: Some(r(|m| i64::from(m.appointments))),
    }
}

/// 見ている人の集団を決める。`teams_of` は owner → チーム名。
/// 戻り値: (集団の owner id, "team" か "all")。見ている人にチームが無ければ全員。
pub fn population<'a>(
    all: &'a HashMap<String, CallerMetrics>,
    viewer_teams: &[String],
    teams_of: &HashMap<String, Vec<String>>,
) -> (Vec<&'a str>, &'static str) {
    let callers = all.iter().filter(|(_, m)| is_caller(m));
    if viewer_teams.is_empty() {
        return (callers.map(|(o, _)| o.as_str()).collect(), "all");
    }
    let mine: HashSet<&str> = viewer_teams.iter().map(String::as_str).collect();
    (
        callers
            .filter(|(o, _)| {
                teams_of
                    .get(o.as_str())
                    .is_some_and(|ts| ts.iter().any(|t| mine.contains(t.as_str())))
            })
            .map(|(o, _)| o.as_str())
            .collect(),
        "team",
    )
}

/// 本人の応答を組み立てる (純粋)
pub fn build_today(
    all: &HashMap<String, CallerMetrics>,
    my_owner: Option<&str>,
    viewer_teams: &[String],
    teams_of: &HashMap<String, Vec<String>>,
) -> (
    Option<MyTodayMetrics>,
    Option<TeamAverage>,
    u32,
    &'static str,
) {
    let (mut pop_ids, mut label) = population(all, viewer_teams, teams_of);
    // 同じチームに自分しかいない・誰もいないときも、チームの集団のまま (人数を出す)
    if pop_ids.is_empty() && !viewer_teams.is_empty() {
        (pop_ids, label) = population(all, &[], teams_of);
    }
    let pop: Vec<&CallerMetrics> = pop_ids.iter().filter_map(|o| all.get(*o)).collect();
    let me = my_owner.map(|o| {
        let m = all.get(o).cloned().unwrap_or_default();
        let rank = if is_caller(&m) && pop_ids.contains(&o) {
            ranks(&pop, &m)
        } else {
            Ranks::default()
        };
        MyTodayMetrics {
            owner_id: o.to_string(),
            metrics: m,
            rank,
        }
    });
    (me, average(&pop), pop.len() as u32, label)
}

// ---------------------------------------------------------------------------
// Search の本文と応答の読み取り
// ---------------------------------------------------------------------------

fn flt(name: &str, op: &str, value: Value) -> Value {
    json!({"propertyName": name, "operator": op, "value": value})
}

fn call_body(prop: &str, from: i64, to: i64, day_start: Option<i64>, after: Option<&str>) -> Value {
    let mut filters = vec![
        flt("hs_call_source", "EQ", json!(CALL_SOURCE)),
        flt(prop, "GTE", json!(from.to_string())),
        flt(prop, "LT", json!(to.to_string())),
    ];
    if let Some(s) = day_start {
        filters.push(flt("hs_timestamp", "GTE", json!(s.to_string())));
    }
    let mut body = json!({
        "filterGroups": [{"filters": filters}],
        "properties": ["hubspot_owner_id", "hs_timestamp", "hs_call_duration", "hs_call_direction"],
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": PAGE,
    });
    if let Some(a) = after {
        body["after"] = json!(a);
    }
    body
}

/// 日時プロパティは REST では ISO 8601、Search でもそうだが、エポック ms の文字列のこともある
fn parse_time_ms(v: &Value) -> Option<i64> {
    match v {
        Value::String(s) => s.parse::<i64>().ok().or_else(|| {
            DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|t| t.timestamp_millis())
        }),
        Value::Number(n) => n.as_i64(),
        _ => None,
    }
}

fn prop_str<'a>(p: &'a Value, k: &str) -> Option<&'a str> {
    p.get(k)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|s| !s.is_empty())
}

/// Call 1 件を読む。担当者・時刻・向きが読めないものは捨てる (None)
pub fn parse_call(v: &Value) -> Option<CallRow> {
    let id = match v.get("id")? {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    let p = v.get("properties")?;
    let owner_id = prop_str(p, "hubspot_owner_id")?.to_string();
    let ts_ms = parse_time_ms(p.get("hs_timestamp")?)?;
    let direction = match prop_str(p, "hs_call_direction")? {
        "OUTBOUND" => Direction::Outbound,
        "INBOUND" => Direction::Inbound,
        _ => return None,
    };
    let duration_ms = p
        .get("hs_call_duration")
        .and_then(parse_time_ms)
        .unwrap_or(0)
        .max(0);
    Some(CallRow {
        id,
        owner_id,
        ts_ms,
        duration_ms,
        direction,
    })
}

/// `[from, to)` の Call を `prop` (hs_timestamp か hs_lastmodifieddate) で読む。合計が多ければ時間で割る
async fn fetch_calls(
    client: &HubSpotClient,
    prop: &str,
    from: i64,
    to: i64,
    day_start: Option<i64>,
    searches: &mut u32,
) -> Result<Vec<CallRow>, HubSpotError> {
    let mut out = Vec::new();
    let mut stack = vec![(from, to)];
    while let Some((a, b)) = stack.pop() {
        let mut after: Option<String> = None;
        let mut first = true;
        loop {
            *searches += 1;
            if *searches > MAX_SEARCHES {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let v = client
                .search("calls", call_body(prop, a, b, day_start, after.as_deref()))
                .await?;
            if first {
                first = false;
                let total = v.get("total").and_then(Value::as_u64).unwrap_or(0);
                if total > SPLIT_AT && b - a > 1_000 {
                    let mid = a + (b - a) / 2;
                    stack.push((mid, b));
                    stack.push((a, mid));
                    break;
                }
            }
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("calls without results".into()))?;
            out.extend(results.iter().filter_map(parse_call));
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

/// 今日アポ日確定に入った取引を担当者ごとに数える
async fn fetch_appointments(
    client: &HubSpotClient,
    day_start: i64,
    day_end: i64,
    searches: &mut u32,
) -> Result<HashMap<String, u32>, HubSpotError> {
    let mut by: HashMap<String, u32> = HashMap::new();
    let mut seen: HashSet<String> = HashSet::new();
    for (_, stage) in APPOINTMENT_STAGES {
        let prop = format!("hs_v2_date_entered_{stage}");
        let mut after: Option<String> = None;
        loop {
            *searches += 1;
            if *searches > MAX_SEARCHES {
                return Err(HubSpotError::Decode("too many searches".into()));
            }
            let mut body = json!({
                "filterGroups": [{"filters": [
                    flt(&prop, "GTE", json!(day_start.to_string())),
                    flt(&prop, "LT", json!(day_end.to_string())),
                ]}],
                "properties": ["hubspot_owner_id", prop],
                "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
                "limit": PAGE,
            });
            if let Some(a) = &after {
                body["after"] = json!(a);
            }
            let v = client.search("deals", body).await?;
            let results = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("deals without results".into()))?;
            for r in results {
                let id = r.get("id").map(Value::to_string).unwrap_or_default();
                // 複数のステージを数えるとき、同じ取引を 2 度数えない
                if !seen.insert(id) {
                    continue;
                }
                if let Some(o) = r
                    .get("properties")
                    .and_then(|p| prop_str(p, "hubspot_owner_id"))
                {
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

/// 今日の Call の入れ物。id で上書きする (増分の重なりで数が増えない)
#[derive(Default)]
pub struct Store {
    day: Option<NaiveDate>,
    calls: HashMap<String, CallRow>,
    /// 前回の読み始めの時刻
    last_fetch_start: Option<DateTime<Utc>>,
}

impl Store {
    /// 日が変わっていたら空にする
    pub fn reset_if_new_day(&mut self, day: NaiveDate) {
        if self.day != Some(day) {
            *self = Store {
                day: Some(day),
                ..Store::default()
            };
        }
    }

    /// 読んだ Call を入れる (同じ id は新しい方で上書き)
    pub fn merge(&mut self, rows: Vec<CallRow>) {
        for r in rows {
            self.calls.insert(r.id.clone(), r);
        }
    }

    pub fn len(&self) -> usize {
        self.calls.len()
    }

    pub fn is_empty(&self) -> bool {
        self.calls.is_empty()
    }
}

/// 集計済みの結果 (要求はこれを読む)
pub struct Snapshot {
    pub day: NaiveDate,
    pub as_of: DateTime<Utc>,
    pub all: HashMap<String, CallerMetrics>,
}

#[derive(Default)]
pub struct CallerMetricsState {
    store: tokio::sync::Mutex<Store>,
    snapshot: RwLock<Option<Arc<Snapshot>>>,
    next_try: Mutex<Option<DateTime<Utc>>>,
}

impl CallerMetricsState {
    pub fn new() -> Self {
        Self::default()
    }

    fn current(&self) -> Option<Arc<Snapshot>> {
        self.snapshot.read().ok().and_then(|g| g.clone())
    }

    fn fresh(&self, now: DateTime<Utc>, day: NaiveDate) -> Option<Arc<Snapshot>> {
        self.current()
            .filter(|s| s.day == day && now - s.as_of < REFRESH_INTERVAL)
    }

    fn backing_off(&self, now: DateTime<Utc>) -> bool {
        self.next_try
            .lock()
            .ok()
            .and_then(|g| *g)
            .is_some_and(|t| now < t)
    }

    /// 今日の結果を返す。(結果, 古いまま返しているか)
    pub async fn get(
        &self,
        client: &HubSpotClient,
        now: DateTime<Utc>,
    ) -> Result<(Arc<Snapshot>, bool), HubSpotError> {
        let (day, _, _) = jst_day(now);
        if let Some(s) = self.fresh(now, day) {
            return Ok((s, false));
        }
        // 昨日の結果は今日の数字として返さない
        let usable_stale = || self.current().filter(|s| s.day == day);
        if self.backing_off(now) {
            return usable_stale().map(|s| (s, true)).ok_or(HubSpotError::Busy);
        }
        let mut store = match self.store.try_lock() {
            Ok(g) => g,
            // 誰かが読み直している最中。前回の結果があればそれを返す
            Err(_) => {
                if let Some(s) = usable_stale() {
                    return Ok((s, true));
                }
                self.store.lock().await
            }
        };
        // 待っている間に別の要求が読み直したかもしれない
        if let Some(s) = self.fresh(now, day) {
            return Ok((s, false));
        }
        match refresh(client, &mut store, now).await {
            Ok(snap) => {
                let snap = Arc::new(snap);
                if let Ok(mut g) = self.snapshot.write() {
                    *g = Some(snap.clone());
                }
                Ok((snap, false))
            }
            Err(e) => {
                tracing::warn!(
                    error_kind = e.error_kind(),
                    "crm caller metrics refresh failed"
                );
                if let Ok(mut g) = self.next_try.lock() {
                    *g = Some(now + RETRY_AFTER_FAILURE);
                }
                usable_stale().map(|s| (s, true)).ok_or(e)
            }
        }
    }
}

/// 1 回の読み直し。失敗したら Store を中途半端に進めない
async fn refresh(
    client: &HubSpotClient,
    store: &mut Store,
    now: DateTime<Utc>,
) -> Result<Snapshot, HubSpotError> {
    let (day, start, end) = jst_day(now);
    store.reset_if_new_day(day);
    let bg = client.background();
    let mut searches = 0u32;
    let rows = match store.last_fetch_start {
        None => fetch_calls(&bg, "hs_timestamp", start, end, None, &mut searches).await?,
        Some(prev) => {
            let from = ((prev - OVERLAP).timestamp_millis()).max(start);
            let to = (now + Duration::hours(1)).timestamp_millis();
            fetch_calls(
                &bg,
                "hs_lastmodifieddate",
                from,
                to,
                Some(start),
                &mut searches,
            )
            .await?
        }
    };
    let apo = fetch_appointments(&bg, start, end, &mut searches).await?;
    store.merge(rows);
    store.last_fetch_start = Some(now);
    tracing::info!(
        searches,
        calls = store.len(),
        "crm caller metrics refreshed"
    );
    Ok(Snapshot {
        day,
        as_of: now,
        all: aggregate(store.calls.values(), &apo),
    })
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn as_of_text(t: DateTime<Utc>) -> String {
    t.to_rfc3339_opts(SecondsFormat::Secs, true)
}

fn teams_map(owners: &[super::owners::CrmOwner]) -> HashMap<String, Vec<String>> {
    owners
        .iter()
        .map(|o| (o.id.clone(), o.teams.clone()))
        .collect()
}

pub(super) async fn get_today(
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
    let work = async {
        let (snap, stale) = ctx.caller_metrics.get(&client, now).await?;
        let me_ref = ctx.queue.owner_info_for(&client, &email).await?;
        // チームの照合に必要なときだけ担当者一覧 (10 分キャッシュ) を読む。取れなければ全員で比べる
        let viewer_teams = me_ref.as_ref().map(|o| o.teams.clone()).unwrap_or_default();
        let teams_of = if viewer_teams.is_empty() {
            HashMap::new()
        } else {
            match ctx.queue.owner_list.get(&client).await {
                Ok(l) => teams_map(&l.owners),
                Err(_) => HashMap::new(),
            }
        };
        let eff_teams = if teams_of.is_empty() {
            Vec::new()
        } else {
            viewer_teams
        };
        Ok::<_, HubSpotError>((snap, stale, me_ref, eff_teams, teams_of))
    };
    let (snap, stale, me_ref, viewer_teams, teams_of) =
        match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
            Err(_) => return super::routes::timeout_response(),
            Ok(Err(e)) => return hubspot_error_response(&e),
            Ok(Ok(v)) => v,
        };
    let my_owner = me_ref.as_ref().map(|o| o.id.as_str());
    let (me, team_avg, population_size, label) =
        build_today(&snap.all, my_owner, &viewer_teams, &teams_of);
    let message = if me.is_none() {
        Some(
            "あなたのアカウントが HubSpot の担当者と結び付けられないため、個人の成績を出せません。"
                .to_string(),
        )
    } else {
        None
    };
    Json(TodayMetricsResponse {
        as_of: as_of_text(snap.as_of),
        day: snap.day.to_string(),
        stale,
        me,
        team_avg,
        population_size,
        population: label.to_string(),
        team_names: viewer_teams,
        message,
    })
    .into_response()
}

pub(super) async fn get_today_all(
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
    let work = async {
        let (snap, stale) = ctx.caller_metrics.get(&client, now).await?;
        // 名前・チームは付け足し。取れなくても数字は返す
        let owners = ctx.queue.owner_list.get(&client).await.ok();
        Ok::<_, HubSpotError>((snap, stale, owners))
    };
    let (snap, stale, owners) = match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
        Err(_) => return super::routes::timeout_response(),
        Ok(Err(e)) => return hubspot_error_response(&e),
        Ok(Ok(v)) => v,
    };
    let info: HashMap<&str, &super::owners::CrmOwner> = owners
        .as_ref()
        .map(|l| l.owners.iter().map(|o| (o.id.as_str(), o)).collect())
        .unwrap_or_default();
    let mut callers: Vec<CallerRowOut> = snap
        .all
        .iter()
        .filter(|(_, m)| m.outbound > 0 || m.inbound > 0 || m.appointments > 0)
        .map(|(id, m)| CallerRowOut {
            owner_id: id.clone(),
            name: info
                .get(id.as_str())
                .map_or_else(|| "(名前を取得できません)".to_string(), |o| o.name.clone()),
            teams: info
                .get(id.as_str())
                .map(|o| o.teams.clone())
                .unwrap_or_default(),
            metrics: m.clone(),
        })
        .collect();
    callers.sort_by(|a, b| {
        b.metrics
            .outbound
            .cmp(&a.metrics.outbound)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    let teams: BTreeSet<String> = callers
        .iter()
        .flat_map(|c| c.teams.iter().cloned())
        .collect();
    Json(AllCallersResponse {
        as_of: as_of_text(snap.as_of),
        day: snap.day.to_string(),
        stale,
        teams: teams.into_iter().collect(),
        callers,
    })
    .into_response()
}
