//! 帯の「自分の項目」の数字: `GET /api/crm/metrics/custom` (本人用、ADR-028・headless-crm-design §19)。
//!
//! 本人の設定 (`metric_prefs`) にある自分の項目ごとに、本人の値・比べる相手の平均・本人の順位を返す。
//!
//! - 比べる相手: 見ている人と同じ HubSpot チームの人 (アーカイブ済みを除く、本人を含む)。
//!   チームが無い・担当者一覧が取れないときは、今日架電した全員 (今日の成績の集団) と本人。
//! - 項目 1 つにつき Search を 1 回組む: 条件 (詳細条件と同じ検証で解決) + 「誰の」項目が比べる相手の
//!   owner の IN (100 人ずつ)。最大 2,000 件まで読み、owner ごとに件数・合計・平均を数える。
//!   2,000 件を超えたらその項目は `too_many` (黙って間違った数字を出さない)。
//! - 同じ定義 (解決後の条件・数え方・「誰の」・比べる相手の集合) の結果は、人をまたいで 2 分キャッシュする。
//! - 読み直しは要求から切り離して背景で走らせ (同じ定義は 1 本だけ)、終わるまでは前の値を返して
//!   `pending: true` (画面は少し後に取り直す)。HubSpot の呼び出しは Background の優先度。
//!   失敗 (関所の混雑 `hubspot_busy` 等) したら前の値を「少し前の値」(`stale`) として返し、30 秒は読み直さない。
//! - **単一インスタンス前提** (今日の成績と同じ。複数台では台ごとに読む)。

use std::collections::{BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{json, Value};
use tower_sessions::Session;
use ts_rs::TS;

use super::metric_prefs::{self, MetricKind, ResolvedMetric};
use super::rbac;
use super::routes::{error_json, hubspot_error_response, CrmCtx, CRM_REQUEST_DEADLINE};
use crate::hubspot::{HubSpotClient, HubSpotError};
use crate::AppState;

/// 1 つの項目で読む取引の上限。これを超えたら数字を出さない
pub const MAX_ROWS: u64 = 2_000;
/// Search の 1 ページ
pub const PAGE: u32 = 200;
/// IN に入れる owner の数 (HubSpot の IN の値の上限に合わせる)
pub const OWNER_CHUNK: usize = 100;
/// 同じ定義の結果を使い回す時間
pub const TTL: Duration = Duration::seconds(120);
/// 失敗したあと読み直さない時間
pub const RETRY_AFTER_FAILURE: Duration = Duration::seconds(30);
/// キャッシュの上限 (定義 × 比べる相手の組)
pub const MAX_ENTRIES: usize = 500;

// ---------------------------------------------------------------------------
// 型 (React 側へ ts-rs で書き出す)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
pub struct CustomMetricValue {
    pub id: String,
    pub name: String,
    pub kind: MetricKind,
    /// `ready` (数字あり) / `loading` (最初の集計中) / `too_many` (2,000 件超え) /
    /// `invalid` (項目の定義が変わって使えない) / `unavailable` (読めなかった)
    pub status: String,
    /// 本人の値 (件数・合計・平均)。平均で値のある取引が無ければ null
    pub value: Option<f64>,
    /// 比べる相手の平均 (件数・合計は 1 人あたり、平均は全員の合計 ÷ 値のある取引の数)
    pub team_avg: Option<f64>,
    /// 1 が最良 (多い順、同じ値は同順位)。平均で本人の値が無ければ null
    pub rank: Option<u32>,
    /// 順位を付けた人数 (平均は値のある人だけ)
    pub ranked: u32,
    /// 読み直しに失敗していて、前の値を返している
    pub stale: bool,
    /// この数字を読んだ時刻 (RFC 3339)
    pub as_of: Option<String>,
    /// status が ready 以外のときの理由 (画面にそのまま出せる文)
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CustomMetricsResponse {
    /// true のとき、どれかの項目を背景で読み直している (画面は少し後に取り直す)
    pub pending: bool,
    pub as_of: String,
    /// "team" = 同じチームの人、"all" = 今日架電した全員
    pub population: String,
    pub population_size: u32,
    pub team_names: Vec<String>,
    pub items: Vec<CustomMetricValue>,
    /// 項目を出せないときの理由 (担当者と結び付けられない・設定を読めない)
    pub message: Option<String>,
}

pub const TOO_MANY_MESSAGE: &str = "件数が多すぎます（条件を絞ってください）";

// ---------------------------------------------------------------------------
// 純粋な計算
// ---------------------------------------------------------------------------

/// owner 1 人分の集計
#[derive(Debug, Clone, Default, PartialEq)]
pub struct OwnerAgg {
    /// 条件に合う取引の数
    pub count: u64,
    /// 選んだ数値の項目の合計 (値が数値の取引だけ)
    pub sum: f64,
    /// 値が数値だった取引の数 (平均の分母)
    pub with_value: u64,
}

fn num(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse::<f64>().ok(),
        _ => None,
    }
    .filter(|f| f.is_finite())
}

/// Search の結果 (取引) を owner ごとに数える。「誰の」が空の取引は数えない
pub fn aggregate(
    rows: &[Value],
    owner_property: &str,
    property: Option<&str>,
) -> HashMap<String, OwnerAgg> {
    let mut by: HashMap<String, OwnerAgg> = HashMap::new();
    for r in rows {
        let Some(p) = r.get("properties") else {
            continue;
        };
        let Some(owner) = p
            .get(owner_property)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
        else {
            continue;
        };
        let a = by.entry(owner.to_string()).or_default();
        a.count += 1;
        if let Some(v) = property.and_then(|k| p.get(k)).and_then(num) {
            a.sum += v;
            a.with_value += 1;
        }
    }
    by
}

/// 本人の値・比べる相手の平均・順位
#[derive(Debug, Clone, PartialEq)]
pub struct Summary {
    pub value: Option<f64>,
    pub team_avg: Option<f64>,
    pub rank: Option<u32>,
    pub ranked: u32,
}

fn rank_f(values: &[f64], mine: f64) -> u32 {
    1 + values.iter().filter(|v| **v > mine).count() as u32
}

/// 比べる相手 (`population`、本人を含む) で平均と順位を出す。相手に居ない owner の取引は数えない
pub fn summarize(
    kind: MetricKind,
    aggs: &HashMap<String, OwnerAgg>,
    population: &[String],
    me: &str,
) -> Summary {
    let zero = OwnerAgg::default();
    let of = |o: &str| aggs.get(o).unwrap_or(&zero);
    let n = population.len();
    match kind {
        MetricKind::Count | MetricKind::Sum => {
            let val = |a: &OwnerAgg| match kind {
                MetricKind::Count => a.count as f64,
                _ => a.sum,
            };
            let vals: Vec<f64> = population.iter().map(|o| val(of(o))).collect();
            let mine = val(of(me));
            Summary {
                value: Some(mine),
                team_avg: (n > 0).then(|| vals.iter().sum::<f64>() / n as f64),
                rank: population
                    .iter()
                    .any(|o| o == me)
                    .then(|| rank_f(&vals, mine)),
                ranked: n as u32,
            }
        }
        MetricKind::Avg => {
            let avg = |a: &OwnerAgg| (a.with_value > 0).then(|| a.sum / a.with_value as f64);
            let vals: Vec<f64> = population.iter().filter_map(|o| avg(of(o))).collect();
            let (s, c) = population.iter().fold((0.0, 0u64), |(s, c), o| {
                let a = of(o);
                (s + a.sum, c + a.with_value)
            });
            let mine = avg(of(me));
            Summary {
                value: mine,
                team_avg: (c > 0).then(|| s / c as f64),
                rank: mine.map(|m| rank_f(&vals, m)),
                ranked: vals.len() as u32,
            }
        }
    }
}

/// 比べる相手を決める。戻り値: (owner ID の昇順の一覧 (本人を含む), "team" か "all")
pub fn population(
    me: &str,
    viewer_teams: &[String],
    owners: Option<&[super::owners::CrmOwner]>,
    todays_callers: &[String],
) -> (Vec<String>, &'static str) {
    let mut set: BTreeSet<String> = BTreeSet::new();
    let label = match owners {
        Some(list) if !viewer_teams.is_empty() => {
            let mine: HashSet<&str> = viewer_teams.iter().map(String::as_str).collect();
            set.extend(
                list.iter()
                    .filter(|o| !o.archived && o.teams.iter().any(|t| mine.contains(t.as_str())))
                    .map(|o| o.id.clone()),
            );
            "team"
        }
        _ => {
            set.extend(todays_callers.iter().cloned());
            "all"
        }
    };
    set.insert(me.to_string());
    (set.into_iter().collect(), label)
}

/// キャッシュの鍵。解決後の条件 (相対日付は日付の数値になっている)・数え方・項目・「誰の」・比べる相手の集合
pub fn cache_key(r: &ResolvedMetric, population: &[String]) -> String {
    json!({
        "f": r.filters,
        "k": r.kind,
        "p": r.property,
        "o": r.owner_property,
        "who": population,
    })
    .to_string()
}

fn search_body(r: &ResolvedMetric, owners: &[String], after: Option<&str>) -> Value {
    let mut filters = r.filters.clone();
    filters.push(json!({"propertyName": r.owner_property, "operator": "IN", "values": owners}));
    let mut props = vec![r.owner_property.clone()];
    if let Some(p) = &r.property {
        props.push(p.clone());
    }
    let mut body = json!({
        "filterGroups": [{"filters": filters}],
        "properties": props,
        "sorts": [{"propertyName": "hs_object_id", "direction": "ASCENDING"}],
        "limit": PAGE,
    });
    if let Some(a) = after {
        body["after"] = json!(a);
    }
    body
}

/// 読み取りの結果
#[derive(Debug, Clone, PartialEq)]
pub enum Fetched {
    Rows(Arc<HashMap<String, OwnerAgg>>),
    /// 2,000 件を超えた
    TooMany,
}

/// 1 つの項目を HubSpot から読む (呼び出し側が Background の優先度のクライアントを渡す)
pub async fn fetch(
    client: &HubSpotClient,
    r: &ResolvedMetric,
    population: &[String],
) -> Result<Fetched, HubSpotError> {
    let mut rows: Vec<Value> = Vec::new();
    let mut total: u64 = 0;
    for chunk in population.chunks(OWNER_CHUNK) {
        let mut after: Option<String> = None;
        let mut first = true;
        loop {
            let v = client
                .search("deals", search_body(r, chunk, after.as_deref()))
                .await?;
            if first {
                first = false;
                total += v.get("total").and_then(Value::as_u64).unwrap_or(0);
                // 読む前に合計で判断する (超えるなら 1 回の Search で止める)
                if total > MAX_ROWS {
                    return Ok(Fetched::TooMany);
                }
            }
            let page = v
                .get("results")
                .and_then(Value::as_array)
                .ok_or_else(|| HubSpotError::Decode("deals without results".into()))?;
            rows.extend(page.iter().cloned());
            if rows.len() as u64 > MAX_ROWS {
                return Ok(Fetched::TooMany);
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
    Ok(Fetched::Rows(Arc::new(aggregate(
        &rows,
        &r.owner_property,
        r.property.as_deref(),
    ))))
}

// ---------------------------------------------------------------------------
// キャッシュ (人をまたいで共有)
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct Entry {
    /// 最後に読めた結果とその時刻
    ok: Option<(DateTime<Utc>, Fetched)>,
    /// 最後の失敗の時刻 (成功したら消す)
    failed_at: Option<DateTime<Utc>>,
    running: bool,
}

/// 項目 1 つの、いまの値
#[derive(Debug, Clone, PartialEq)]
pub enum Lookup {
    /// (結果, 読んだ時刻, 古い (読み直しに失敗している) か)
    Value(Fetched, DateTime<Utc>, bool),
    /// まだ一度も読めていない (読んでいる)
    Loading,
    /// まだ一度も読めておらず、直近の読み込みが失敗した
    Failed,
}

#[derive(Default)]
pub struct CustomMetricsCache {
    entries: Mutex<HashMap<String, Entry>>,
}

impl CustomMetricsCache {
    /// いまの値を返し、読み直しが要るかを決める。true が返ったら呼び出し側が [`Self::finish`] まで読む
    pub fn begin(&self, key: &str, now: DateTime<Utc>) -> (Lookup, bool) {
        let mut g = self.entries.lock().expect("cache lock");
        if !g.contains_key(key) && g.len() >= MAX_ENTRIES {
            // 古いものから捨てる (読んでいる最中のものは残す)
            let mut olds: Vec<(String, DateTime<Utc>)> = g
                .iter()
                .filter(|(_, e)| !e.running)
                .map(|(k, e)| {
                    (
                        k.clone(),
                        e.ok.as_ref().map_or(DateTime::<Utc>::MIN_UTC, |(t, _)| *t),
                    )
                })
                .collect();
            olds.sort_by_key(|(_, t)| *t);
            for (k, _) in olds.into_iter().take(g.len() + 1 - MAX_ENTRIES) {
                g.remove(&k);
            }
        }
        let e = g.entry(key.to_string()).or_insert(Entry {
            ok: None,
            failed_at: None,
            running: false,
        });
        let fresh = e.ok.as_ref().is_some_and(|(t, _)| now - *t < TTL);
        let backing_off = e.failed_at.is_some_and(|t| now - t < RETRY_AFTER_FAILURE);
        let start = !fresh && !backing_off && !e.running;
        if start {
            e.running = true;
        }
        let lookup = match (&e.ok, e.failed_at) {
            (Some((t, f)), failed) => Lookup::Value(f.clone(), *t, failed.is_some()),
            (None, Some(_)) if !e.running => Lookup::Failed,
            (None, _) => Lookup::Loading,
        };
        (lookup, start)
    }

    /// 読み直しの結果を入れる
    pub fn finish(&self, key: &str, now: DateTime<Utc>, result: Result<Fetched, HubSpotError>) {
        let mut g = self.entries.lock().expect("cache lock");
        let Some(e) = g.get_mut(key) else { return };
        e.running = false;
        match result {
            Ok(f) => {
                e.ok = Some((now, f));
                e.failed_at = None;
            }
            Err(err) => {
                tracing::warn!(
                    error_kind = err.error_kind(),
                    "crm custom metric refresh failed"
                );
                e.failed_at = Some(now);
            }
        }
    }

    pub fn len(&self) -> usize {
        self.entries.lock().map(|g| g.len()).unwrap_or(0)
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// 要求の中から呼ぶ。読み直しが要れば背景で走らせ、いまの値をすぐ返す
pub fn lookup_or_spawn(
    cache: &Arc<CustomMetricsCache>,
    client: &Arc<HubSpotClient>,
    r: &ResolvedMetric,
    population: &[String],
    now: DateTime<Utc>,
) -> Lookup {
    let key = cache_key(r, population);
    let (lookup, start) = cache.begin(&key, now);
    if start {
        let cache = cache.clone();
        let bg = client.background();
        let r = r.clone();
        let pop = population.to_vec();
        tokio::spawn(async move {
            let res = fetch(&bg, &r, &pop).await;
            cache.finish(&key, Utc::now().max(now), res);
        });
    }
    lookup
}

/// 1 つの項目の応答を組み立てる (純粋)
pub fn item_value(
    def: &metric_prefs::CustomMetricDef,
    lookup: &Lookup,
    population: &[String],
    me: &str,
) -> CustomMetricValue {
    let base = CustomMetricValue {
        id: def.id.clone(),
        name: def.name.clone(),
        kind: def.kind,
        status: "ready".into(),
        value: None,
        team_avg: None,
        rank: None,
        ranked: 0,
        stale: false,
        as_of: None,
        message: None,
    };
    match lookup {
        Lookup::Loading => CustomMetricValue {
            status: "loading".into(),
            message: Some("集計しています…".into()),
            ..base
        },
        Lookup::Failed => CustomMetricValue {
            status: "unavailable".into(),
            message: Some("いまは数字を読めません".into()),
            ..base
        },
        Lookup::Value(Fetched::TooMany, t, stale) => CustomMetricValue {
            status: "too_many".into(),
            stale: *stale,
            as_of: Some(super::caller_metrics::as_of_text(*t)),
            message: Some(TOO_MANY_MESSAGE.into()),
            ..base
        },
        Lookup::Value(Fetched::Rows(aggs), t, stale) => {
            let s = summarize(def.kind, aggs, population, me);
            CustomMetricValue {
                value: s.value,
                team_avg: s.team_avg,
                rank: s.rank,
                ranked: s.ranked,
                stale: *stale,
                as_of: Some(super::caller_metrics::as_of_text(*t)),
                ..base
            }
        }
    }
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn empty(now: DateTime<Utc>, message: Option<String>) -> CustomMetricsResponse {
    CustomMetricsResponse {
        pending: false,
        as_of: super::caller_metrics::as_of_text(now),
        population: "all".into(),
        population_size: 0,
        team_names: Vec::new(),
        items: Vec::new(),
        message,
    }
}

pub(super) async fn get_custom(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(d) => return d.into_response(),
    };
    let email = principal
        .email
        .clone()
        .unwrap_or_default()
        .trim()
        .to_lowercase();
    let now = ctx.queue.now();
    // 1) 本人の設定 (自分の項目が無ければ HubSpot を呼ばない)
    let prefs = match metric_prefs::load(&state, &ctx, &email).await {
        Ok(p) => p.unwrap_or_default(),
        Err(_) => {
            return Json(empty(now, Some(metric_prefs::UNAVAILABLE_MESSAGE.into()))).into_response()
        }
    };
    let defs: Vec<&metric_prefs::CustomMetricDef> = prefs
        .custom
        .iter()
        .filter(|c| prefs.items.contains(&c.id))
        .take(metric_prefs::MAX_CUSTOM)
        .collect();
    if defs.is_empty() {
        return Json(empty(now, None)).into_response();
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 2) 本人の担当者・項目の一覧・比べる相手 (どれもキャッシュあり)
    let work = async {
        let me = ctx.queue.owner_info_for(&client, &email).await?;
        let Some(me) = me else {
            return Ok::<_, HubSpotError>(None);
        };
        let (catalog, _) = ctx.catalog.get(&client).await?;
        let owners = if me.teams.is_empty() {
            None
        } else {
            ctx.queue.owner_list.get(&client).await.ok()
        };
        Ok(Some((me, catalog, owners)))
    };
    let (me, catalog, owners) = match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
        Err(_) => return super::routes::timeout_response(),
        Ok(Err(e)) => return hubspot_error_response(&e),
        Ok(Ok(None)) => {
            return Json(empty(
                now,
                Some("あなたのアカウントが HubSpot の担当者と結び付けられないため、自分の項目を出せません。".into()),
            ))
            .into_response()
        }
        Ok(Ok(Some(v))) => v,
    };
    let callers: Vec<String> = ctx
        .caller_metrics
        .current_snapshot()
        .map(|s| {
            s.all
                .iter()
                .filter(|(_, m)| super::caller_metrics::is_caller(m))
                .map(|(o, _)| o.clone())
                .collect()
        })
        .unwrap_or_default();
    let (pop, label) = population(
        &me.id,
        &me.teams,
        owners.as_ref().map(|l| l.owners.as_slice()),
        &callers,
    );
    let team_names = if label == "team" {
        me.teams.clone()
    } else {
        Vec::new()
    };
    // 3) 項目ごとにいまの値 (要れば背景で読み直す)
    let scope = metric_prefs::value_scope();
    let mut pending = false;
    let items: Vec<CustomMetricValue> = defs
        .iter()
        .map(
            |d| match metric_prefs::resolve_metric(d, &catalog, now, &scope) {
                Err(metric_prefs::Invalid(m)) => CustomMetricValue {
                    id: d.id.clone(),
                    name: d.name.clone(),
                    kind: d.kind,
                    status: "invalid".into(),
                    value: None,
                    team_avg: None,
                    rank: None,
                    ranked: 0,
                    stale: false,
                    as_of: None,
                    message: Some(format!("この項目の設定を直してください（{m}）")),
                },
                Ok(r) => {
                    let lookup = lookup_or_spawn(&ctx.custom_metrics, &client, &r, &pop, now);
                    // 最初の集計中か、期限が切れて背景で読み直している (失敗していない) なら、画面は少し後に取り直す
                    if matches!(&lookup, Lookup::Loading)
                        || matches!(&lookup, Lookup::Value(_, t, false) if now - *t >= TTL)
                    {
                        pending = true;
                    }
                    item_value(d, &lookup, &pop, &me.id)
                }
            },
        )
        .collect();
    Json(CustomMetricsResponse {
        pending,
        as_of: super::caller_metrics::as_of_text(now),
        population: label.into(),
        population_size: pop.len() as u32,
        team_names,
        items,
        message: None,
    })
    .into_response()
}
