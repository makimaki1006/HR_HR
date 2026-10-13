//! 「今日の架電成績」のテスト。偽 HubSpot (本物には通信しない)。人名・ID はすべて架空。
#![allow(clippy::field_reassign_with_default)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Duration, TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::call_queue::CallQueueState;
use super::caller_metrics::*;
use super::rbac::{CrmAccess, CrmRole};
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

const ADMIN: &str = "admin@f-a-c.co.jp";
const BPO: &str = "bpo@f-a-c.co.jp";
const LONE: &str = "lone@f-a-c.co.jp";
const NOBODY: &str = "nobody@f-a-c.co.jp";
const SECRET: &str = "UPSTREAM-SECRET-BODY";

/// JST 2026-10-05 12:00
fn now0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap()
}
/// JST 2026-10-05 00:00 (UTC ms)
fn day_start() -> i64 {
    Utc.with_ymd_and_hms(2026, 10, 4, 15, 0, 0)
        .unwrap()
        .timestamp_millis()
}

fn call(id: &str, owner: &str, dir: Direction, dur_ms: i64, at_ms: i64) -> CallRow {
    CallRow {
        id: id.into(),
        owner_id: owner.into(),
        ts_ms: at_ms,
        duration_ms: dur_ms,
        direction: dir,
    }
}
fn out(id: &str, owner: &str, dur_ms: i64) -> CallRow {
    call(
        id,
        owner,
        Direction::Outbound,
        dur_ms,
        day_start() + 3_600_000,
    )
}

// ---------------------------------------------------------------------------
// 純粋な計算
// ---------------------------------------------------------------------------

#[test]
fn しきい値はちょうど_30_90_360_秒を含み_ミリ秒で比べる() {
    let calls = vec![
        out("1", "a", 29_999),
        out("2", "a", 30_000),
        out("3", "a", 89_999),
        out("4", "a", 90_000),
        out("5", "a", 359_999),
        out("6", "a", 360_000),
        out("7", "a", 0),
    ];
    let m = &aggregate(&calls, &HashMap::new())["a"];
    assert_eq!(m.outbound, 7);
    assert_eq!((m.over_30s, m.over_90s, m.over_360s), (5, 3, 1));
    // 割合は件数から計算
    assert_eq!(m.rate_30s, Some(5.0 / 7.0));
    assert_eq!(m.rate_90s, Some(3.0 / 7.0));
    assert_eq!(m.rate_360s, Some(1.0 / 7.0));
    // 通話ありは 0 より長いもの 6 件。合計 = 29,999+30,000+89,999+90,000+359,999+360,000
    assert_eq!(m.talked, 6);
    assert_eq!(m.total_duration_ms, 959_997);
    assert_eq!(m.avg_duration_ms, Some(959_997 / 6));
}

#[test]
fn 着信は架電数にも通話時間にも入れず_別に数える() {
    let calls = vec![
        out("1", "a", 100_000),
        call("2", "a", Direction::Inbound, 500_000, day_start() + 10),
        call("3", "a", Direction::Inbound, 0, day_start() + 20),
    ];
    let m = &aggregate(&calls, &HashMap::new())["a"];
    assert_eq!((m.outbound, m.inbound), (1, 2));
    assert_eq!(m.over_360s, 0);
    assert_eq!(m.total_duration_ms, 100_000);
    // 最初・最後の架電は発信だけ (着信の時刻は入らない)
    assert_eq!(m.first_call_at, m.last_call_at);
}

#[test]
fn 平均は通話ありだけで割り_分母が_0_なら_null() {
    let calls = vec![
        out("1", "a", 0),
        out("2", "a", 0),
        out("3", "b", 60_000),
        out("4", "b", 0),
    ];
    let r = aggregate(&calls, &HashMap::new());
    assert_eq!(r["a"].avg_duration_ms, None);
    assert_eq!(r["a"].talked, 0);
    assert_eq!(r["b"].avg_duration_ms, Some(60_000));
    // 着信だけの人: 架電数 0 なので割合は null
    let only_in = vec![call("9", "c", Direction::Inbound, 1000, 1)];
    let c = &aggregate(&only_in, &HashMap::new())["c"];
    assert_eq!((c.rate_30s, c.rate_90s, c.rate_360s), (None, None, None));
    assert_eq!(c.first_call_at, None);
}

#[test]
fn 最初と最後の架電の時刻() {
    let t = day_start();
    let calls = vec![
        call("1", "a", Direction::Outbound, 0, t + 5 * 3_600_000),
        call("2", "a", Direction::Outbound, 0, t + 1_000),
        call("3", "a", Direction::Outbound, 0, t + 9 * 3_600_000),
    ];
    let m = &aggregate(&calls, &HashMap::new())["a"];
    // day_start は UTC 2026-10-04T15:00:00Z
    assert_eq!(m.first_call_at.as_deref(), Some("2026-10-04T15:00:01Z"));
    assert_eq!(m.last_call_at.as_deref(), Some("2026-10-05T00:00:00Z"));
}

#[test]
fn アポだけある人も1行になり_件数は担当者に付く() {
    let apo = HashMap::from([("a".to_string(), 2u32), ("z".to_string(), 1u32)]);
    let r = aggregate(&[out("1", "a", 1000)], &apo);
    assert_eq!(r["a"].appointments, 2);
    assert_eq!(r["z"].appointments, 1);
    assert_eq!(r["z"].outbound, 0);
    assert!(!is_caller(&r["z"]));
}

#[test]
fn 日付の境目は_jst_0_時() {
    let (d, s, e) = jst_day(Utc.with_ymd_and_hms(2026, 10, 5, 14, 59, 59).unwrap());
    assert_eq!(d.to_string(), "2026-10-05");
    assert_eq!(s, day_start());
    assert_eq!(e - s, 86_400_000);
    // UTC 15:00 = JST 翌日 0 時
    let (d2, s2, _) = jst_day(Utc.with_ymd_and_hms(2026, 10, 5, 15, 0, 0).unwrap());
    assert_eq!(d2.to_string(), "2026-10-06");
    assert_eq!(s2, e);
    // JST の朝 8 時 (UTC 前日 23 時) は JST の当日
    let (d3, s3, _) = jst_day(Utc.with_ymd_and_hms(2026, 10, 4, 23, 0, 0).unwrap());
    assert_eq!(
        (d3.to_string(), s3),
        ("2026-10-05".to_string(), day_start())
    );
}

#[test]
fn 順位は同じ値で同順位_次は飛ぶ() {
    let v = [10, 10, 7, 3];
    assert_eq!(rank_of(&v, 10), 1);
    assert_eq!(rank_of(&v, 7), 3);
    assert_eq!(rank_of(&v, 3), 4);
    // 0 件の人どうしも同順位
    assert_eq!(rank_of(&[0, 0, 5], 0), 2);
}

fn m(outbound: u32, over90: u32, dur: i64, apo: u32) -> CallerMetrics {
    CallerMetrics {
        outbound,
        over_90s: over90,
        total_duration_ms: dur,
        appointments: apo,
        ..CallerMetrics::default()
    }
}

#[test]
fn 平均は1人あたりで_割合は件数の合計から() {
    let a = CallerMetrics {
        outbound: 10,
        over_90s: 5,
        talked: 4,
        total_duration_ms: 400_000,
        appointments: 2,
        ..CallerMetrics::default()
    };
    let b = CallerMetrics {
        outbound: 30,
        over_90s: 5,
        talked: 4,
        total_duration_ms: 1_200_000,
        appointments: 0,
        ..CallerMetrics::default()
    };
    let avg = average(&[&a, &b]).unwrap();
    assert_eq!(avg.outbound, 20.0);
    assert_eq!(avg.over_90s, 5.0);
    // (5+5) / (10+30) = 0.25。人ごとの割合 (0.5 と 0.1667) の平均 (0.333) ではない
    assert_eq!(avg.rate_90s, Some(0.25));
    assert_eq!(avg.total_duration_ms, 800_000.0);
    assert_eq!(avg.avg_duration_ms, Some(200_000.0));
    assert_eq!(avg.appointments, 1.0);
    assert!(average(&[]).is_none());
}

fn world() -> (HashMap<String, CallerMetrics>, HashMap<String, Vec<String>>) {
    let all = HashMap::from([
        ("1".to_string(), m(50, 10, 3_000_000, 2)),
        ("2".to_string(), m(50, 12, 2_000_000, 0)),
        ("3".to_string(), m(20, 1, 500_000, 1)),
        ("4".to_string(), m(80, 30, 9_000_000, 5)), // 別チーム
        ("5".to_string(), m(0, 0, 0, 3)),           // 架電 0。集団に入らない
    ]);
    let teams = HashMap::from([
        ("1".to_string(), vec!["架空A".to_string()]),
        ("2".to_string(), vec!["架空A".to_string()]),
        (
            "3".to_string(),
            vec!["架空A".to_string(), "架空C".to_string()],
        ),
        ("4".to_string(), vec!["架空B".to_string()]),
        ("5".to_string(), vec!["架空A".to_string()]),
    ]);
    (all, teams)
}

#[test]
fn 同じチームの架電した人が集団で_順位は集団の中() {
    let (all, teams) = world();
    let (me, avg, n, label) = build_today(&all, Some("1"), &["架空A".to_string()], &teams);
    assert_eq!((n, label), (3, "team"));
    let me = me.unwrap();
    // 架電数 50 は 50, 50, 20 の中で同率 1 位。通話時間 3,000,000 は 1 位。90秒以上 10 は 12 に次いで 2 位。アポ 2 は 2 位
    assert_eq!(
        me.rank,
        Ranks {
            outbound: Some(1),
            total_duration: Some(1),
            over_90s: Some(2),
            appointments: Some(1),
        }
    );
    assert_eq!(avg.unwrap().outbound, 40.0);
    // 同率 1 位の相手側も 1 位 (2 位に落ちない)
    let (me2, ..) = build_today(&all, Some("2"), &["架空A".to_string()], &teams);
    assert_eq!(me2.unwrap().rank.outbound, Some(1));
    let (me3, ..) = build_today(&all, Some("3"), &["架空A".to_string()], &teams);
    assert_eq!(me3.unwrap().rank.outbound, Some(3));
}

#[test]
fn チームが無い人は今日架電した全員と比べる() {
    let (all, teams) = world();
    let (me, avg, n, label) = build_today(&all, Some("1"), &[], &teams);
    assert_eq!((n, label), (4, "all")); // 5 は架電 0 なので入らない
    assert_eq!(me.unwrap().rank.outbound, Some(2)); // 80 に次いで 50
    assert_eq!(avg.unwrap().outbound, 50.0);
}

#[test]
fn 自分が今日架電していなければ数字はあるが順位は無い() {
    let (all, teams) = world();
    let (me, _, n, _) = build_today(&all, Some("5"), &["架空A".to_string()], &teams);
    let me = me.unwrap();
    assert_eq!(n, 3);
    assert_eq!(me.metrics.appointments, 3);
    assert_eq!(me.rank, Ranks::default());
    // 今日の記録がまったく無い人は 0 の行
    let (me, ..) = build_today(&all, Some("999"), &[], &teams);
    assert_eq!(me.unwrap().metrics, CallerMetrics::default());
}

#[test]
fn 担当者が引けなければ_me_は_null() {
    let (all, teams) = world();
    let (me, avg, n, _) = build_today(&all, None, &[], &teams);
    assert!(me.is_none());
    assert_eq!(n, 4);
    assert!(avg.is_some());
}

#[test]
fn チームに今日架電した人がいなければ全員で比べる() {
    let (all, teams) = world();
    let (_, _, n, label) = build_today(&all, Some("5"), &["空のチーム".to_string()], &teams);
    assert_eq!((n, label), (4, "all"));
}

#[test]
fn call_の読み取り() {
    let v = json!({"id": "11", "properties": {
        "hubspot_owner_id": "7", "hs_timestamp": "2026-10-05T01:00:00Z",
        "hs_call_duration": "108000", "hs_call_direction": "OUTBOUND"}});
    let c = parse_call(&v).unwrap();
    assert_eq!(
        (c.owner_id.as_str(), c.duration_ms, c.direction),
        ("7", 108_000, Direction::Outbound)
    );
    assert_eq!(
        c.ts_ms,
        Utc.with_ymd_and_hms(2026, 10, 5, 1, 0, 0)
            .unwrap()
            .timestamp_millis()
    );
    // エポック ms の文字列
    let v = json!({"id": 12, "properties": {"hubspot_owner_id": "7", "hs_timestamp": "1790000000000",
        "hs_call_direction": "INBOUND"}});
    let c = parse_call(&v).unwrap();
    assert_eq!(
        (c.ts_ms, c.duration_ms, c.direction),
        (1_790_000_000_000, 0, Direction::Inbound)
    );
    // 担当者なし・向き不明は捨てる
    assert!(parse_call(
        &json!({"id":"1","properties":{"hs_timestamp":"1","hs_call_direction":"OUTBOUND"}})
    )
    .is_none());
    assert!(parse_call(&json!({"id":"1","properties":{"hubspot_owner_id":"7","hs_timestamp":"1","hs_call_direction":"X"}})).is_none());
}

#[test]
fn 増分の重なりは_id_で上書きされ_数が増えない() {
    let mut s = Store::default();
    let (d, ..) = jst_day(now0());
    s.reset_if_new_day(d);
    s.merge(vec![out("1", "a", 0), out("2", "a", 0)]);
    // 同じ id が通話時間つきで再び来た (更新された) + 新しい 3
    s.merge(vec![out("2", "a", 95_000), out("3", "a", 0)]);
    assert_eq!(s.len(), 3);
    // 日が変わると空になる
    s.reset_if_new_day(d.succ_opt().unwrap());
    assert!(s.is_empty());
}

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

#[derive(Clone)]
struct FakeCall {
    id: String,
    owner: String,
    ts: i64,
    dur: i64,
    dir: &'static str,
    modified: i64,
    source: &'static str,
}

#[derive(Default)]
struct FakeHs {
    calls: Vec<FakeCall>,
    /// (id, owner, 入った時刻 ms)
    deals: Vec<(String, String, i64)>,
    /// メール → (id, teams)
    owners_by_email: HashMap<String, (String, Vec<String>)>,
    owner_list: Vec<Value>,
    owner_list_fail: bool,
    fail_search: bool,
    /// 窓の幅 (ms) がこれより大きいとき、合計 10,000 だけ返して中身を返さない (分割の確認)
    big_above_ms: Option<i64>,
    /// (object, 本文)
    searches: Vec<(String, Value)>,
    owner_calls: usize,
}

type Shared = Arc<Mutex<FakeHs>>;

fn filter_val(f: &Value) -> i64 {
    f["value"].as_str().unwrap().parse().unwrap()
}

async fn hs_search(
    State(st): State<Shared>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.searches.push((o.clone(), body.clone()));
    if s.fail_search {
        return (StatusCode::INTERNAL_SERVER_ERROR, SECRET).into_response();
    }
    let filters = body["filterGroups"][0]["filters"]
        .as_array()
        .unwrap()
        .clone();
    let bound = |prop: &str, op: &str| -> Option<i64> {
        filters
            .iter()
            .find(|f| f["propertyName"] == prop && f["operator"] == op)
            .map(filter_val)
    };
    let after: usize = body["after"].as_str().map_or(0, |a| a.parse().unwrap());
    let limit = body["limit"].as_u64().unwrap() as usize;
    let rows: Vec<Value> = if o == "calls" {
        let src_ok = filters
            .iter()
            .any(|f| f["propertyName"] == "hs_call_source" && f["value"] == CALL_SOURCE);
        let range = |p: &str| (bound(p, "GTE"), bound(p, "LT"));
        let (lm_a, lm_b) = range("hs_lastmodifieddate");
        let (ts_a, ts_b) = range("hs_timestamp");
        if let (Some(th), Some(a), Some(b)) = (s.big_above_ms, ts_a.or(lm_a), ts_b.or(lm_b)) {
            if filters
                .iter()
                .all(|f| f["propertyName"] != "hs_lastmodifieddate")
                && b - a > th
            {
                return Json(json!({"results": [], "total": 10_000})).into_response();
            }
        }
        s.calls
            .iter()
            .filter(|c| src_ok && c.source == CALL_SOURCE)
            .filter(|c| ts_a.is_none_or(|a| c.ts >= a) && ts_b.is_none_or(|b| c.ts < b))
            .filter(|c| lm_a.is_none_or(|a| c.modified >= a) && lm_b.is_none_or(|b| c.modified < b))
            .map(|c| {
                json!({"id": c.id, "properties": {
                    "hubspot_owner_id": c.owner,
                    "hs_timestamp": Utc.timestamp_millis_opt(c.ts).unwrap().to_rfc3339(),
                    "hs_call_duration": c.dur.to_string(),
                    "hs_call_direction": c.dir}})
            })
            .collect()
    } else {
        let prop = filters[0]["propertyName"].as_str().unwrap().to_string();
        assert!(prop.starts_with("hs_v2_date_entered_"), "{prop}");
        let (a, b) = (bound(&prop, "GTE"), bound(&prop, "LT"));
        s.deals
            .iter()
            .filter(|(_, _, t)| a.is_none_or(|a| *t >= a) && b.is_none_or(|b| *t < b))
            .map(|(id, owner, _)| json!({"id": id, "properties": {"hubspot_owner_id": owner}}))
            .collect()
    };
    let total = rows.len();
    let page: Vec<Value> = rows.into_iter().skip(after).take(limit).collect();
    let mut v = json!({"results": page, "total": total});
    if after + limit < total {
        v["paging"] = json!({"next": {"after": (after + limit).to_string()}});
    }
    Json(v).into_response()
}

async fn hs_owners(State(st): State<Shared>, RawQuery(q): RawQuery) -> Response {
    let mut s = st.lock().unwrap();
    s.owner_calls += 1;
    let url = reqwest::Url::parse(&format!("http://x/?{}", q.unwrap_or_default())).unwrap();
    let get = |n: &str| {
        url.query_pairs()
            .find(|(k, _)| k == n)
            .map(|(_, v)| v.into_owned())
    };
    if let Some(email) = get("email") {
        let results: Vec<Value> = s
            .owners_by_email
            .get(&email)
            .map(|(id, teams)| {
                vec![json!({"id": id, "email": email, "archived": false,
                    "teams": teams.iter().map(|t| json!({"id": "1", "name": t})).collect::<Vec<_>>()})]
            })
            .unwrap_or_default();
        return Json(json!({"results": results})).into_response();
    }
    if s.owner_list_fail {
        return (StatusCode::INTERNAL_SERVER_ERROR, SECRET).into_response();
    }
    let archived = get("archived").as_deref() == Some("true");
    Json(json!({"results": if archived { vec![] } else { s.owner_list.clone() }})).into_response()
}

async fn start_hs(fake: FakeHs) -> (Arc<HubSpotClient>, Shared) {
    let st = Arc::new(Mutex::new(fake));
    let app = Router::new()
        .route("/crm/v3/objects/{o}/search", post(hs_search))
        .route("/crm/v3/owners", get(hs_owners))
        .with_state(st.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let client = HubSpotClient::new(
        "test-token".into(),
        &format!("http://{addr}"),
        ClientOptions {
            timeout: StdDuration::from_millis(800),
            max_retries: 0,
            retry_base_delay: StdDuration::from_millis(1),
            search_min_interval: StdDuration::from_millis(1),
            rate_limited_min_wait: StdDuration::from_millis(1),
        },
    )
    .unwrap();
    (Arc::new(client), st)
}

fn fc(id: &str, owner: &str, dir: &'static str, dur: i64, ts: i64, modified: i64) -> FakeCall {
    FakeCall {
        id: id.into(),
        owner: owner.into(),
        ts,
        dur,
        dir,
        modified,
        source: CALL_SOURCE,
    }
}

fn searches_of(st: &Shared, object: &str) -> Vec<Value> {
    st.lock()
        .unwrap()
        .searches
        .iter()
        .filter(|(o, _)| o == object)
        .map(|(_, b)| b.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// 状態 (最初の全件読み・増分・日替わり・失敗)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 初回は今日の分を全部読み_2分以内は読み直さない() {
    let t = day_start();
    let mut f = FakeHs::default();
    f.calls = vec![
        fc("1", "7", "OUTBOUND", 100_000, t + 1000, t + 1000),
        fc("2", "7", "OUTBOUND", 0, t + 2000, t + 2000),
        fc("3", "7", "INBOUND", 50_000, t + 3000, t + 3000),
        // 昨日の分は範囲外
        fc("0", "7", "OUTBOUND", 99_000, t - 1000, t - 1000),
    ];
    f.calls.push(FakeCall {
        source: "OTHER",
        ..fc("9", "7", "OUTBOUND", 99_000, t + 4000, t + 4000)
    });
    f.deals = vec![
        ("d1".into(), "7".into(), t + 10),
        ("d2".into(), "8".into(), t + 20),
    ];
    let (client, st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    let (snap, stale) = state.get(&client, now0()).await.unwrap();
    assert!(!stale);
    let a = &snap.all["7"];
    assert_eq!(
        (a.outbound, a.inbound, a.over_90s, a.appointments),
        (2, 1, 1, 1)
    );
    assert_eq!(snap.all["8"].appointments, 1);
    assert_eq!(snap.as_of, now0());
    // 呼び出し: calls 1 + deals 1
    assert_eq!(
        (
            searches_of(&st, "calls").len(),
            searches_of(&st, "deals").len()
        ),
        (1, 1)
    );
    // 1 分後は同じ結果を返し、HubSpot を呼ばない
    state
        .get(&client, now0() + Duration::seconds(119))
        .await
        .unwrap();
    assert_eq!(searches_of(&st, "calls").len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn 増分は_lastmodified_だけを読み_更新を反映し_重複しない() {
    let t = day_start();
    let mut f = FakeHs::default();
    f.calls = vec![
        fc("1", "7", "OUTBOUND", 0, t + 1000, t + 1000),
        fc("2", "7", "OUTBOUND", 0, t + 2000, t + 2000),
    ];
    let (client, st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    state.get(&client, now0()).await.unwrap();
    let n = now0().timestamp_millis();
    {
        let mut s = st.lock().unwrap();
        // 1 は通話が終わって更新、3 は新規。2 は変更なし
        s.calls[0].dur = 120_000;
        s.calls[0].modified = n + 30_000;
        s.calls
            .push(fc("3", "7", "OUTBOUND", 40_000, n + 40_000, n + 40_000));
    }
    let later = now0() + Duration::seconds(130);
    let (snap, _) = state.get(&client, later).await.unwrap();
    let a = &snap.all["7"];
    assert_eq!(a.outbound, 3, "重なりで二重に数えない");
    assert_eq!((a.over_30s, a.over_90s), (2, 1));
    assert_eq!(a.total_duration_ms, 160_000);
    // 2 回目の Search は lastmodified の範囲で、前回の読み始めの 2 分前から
    let calls = searches_of(&st, "calls");
    assert_eq!(calls.len(), 2);
    let filters = calls[1]["filterGroups"][0]["filters"].to_string();
    assert!(filters.contains("hs_lastmodifieddate"), "{filters}");
    assert!(filters.contains(&(n - 120_000).to_string()), "{filters}");
}

#[tokio::test(flavor = "multi_thread")]
async fn 日が変わったら捨てて読み直す() {
    let t = day_start();
    let mut f = FakeHs::default();
    f.calls = vec![fc("1", "7", "OUTBOUND", 0, t + 1000, t + 1000)];
    let (client, _st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    state.get(&client, now0()).await.unwrap();
    // 翌日 JST 0 時 1 分: 昨日の Call は今日の数字に入らない
    let next = Utc.with_ymd_and_hms(2026, 10, 5, 15, 1, 0).unwrap();
    let (snap, _) = state.get(&client, next).await.unwrap();
    assert_eq!(snap.day.to_string(), "2026-10-06");
    assert!(snap.all.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn 合計が多い窓は時間で割って読む() {
    let t = day_start();
    let mut f = FakeHs::default();
    f.big_above_ms = Some(4 * 3_600_000);
    f.calls = (0..5)
        .map(|i| fc(&i.to_string(), "7", "OUTBOUND", 0, t + i * 5 * 3_600_000, t))
        .collect();
    let (client, st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    let (snap, _) = state.get(&client, now0()).await.unwrap();
    // 24 時間 → 12 時間 ×2 → 6 時間 ×4 → 3 時間 ×8。窓は重ならないので 5 件がちょうど 1 回ずつ
    // (now0 は JST 正午だが範囲は JST 当日の終わりまで)
    assert_eq!(snap.all["7"].outbound, 5);
    assert!(searches_of(&st, "calls").len() > 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn 失敗したら前回の結果を古いと示して返し_30秒は再試行しない() {
    let t = day_start();
    let mut f = FakeHs::default();
    f.calls = vec![fc("1", "7", "OUTBOUND", 0, t + 1000, t + 1000)];
    let (client, st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    state.get(&client, now0()).await.unwrap();
    st.lock().unwrap().fail_search = true;
    let t1 = now0() + Duration::seconds(200);
    let (snap, stale) = state.get(&client, t1).await.unwrap();
    assert!(stale);
    assert_eq!(snap.as_of, now0());
    let before = st.lock().unwrap().searches.len();
    let (_, stale) = state
        .get(&client, t1 + Duration::seconds(10))
        .await
        .unwrap();
    assert!(stale);
    assert_eq!(
        st.lock().unwrap().searches.len(),
        before,
        "30 秒は再試行しない"
    );
    // 直ったら読み直す
    st.lock().unwrap().fail_search = false;
    let (snap, stale) = state
        .get(&client, t1 + Duration::seconds(31))
        .await
        .unwrap();
    assert!(!stale);
    assert_eq!(snap.as_of, t1 + Duration::seconds(31));
}

#[tokio::test(flavor = "multi_thread")]
async fn 最初の読み込みが失敗したらエラー() {
    let mut f = FakeHs::default();
    f.fail_search = true;
    let (client, _st) = start_hs(f).await;
    let state = CallerMetricsState::new();
    assert!(state.get(&client, now0()).await.is_err());
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn test_state(hubspot: Option<Arc<HubSpotClient>>) -> Arc<AppState> {
    Arc::new(AppState {
        config: AppConfig {
            port: 0,
            auth_password: "internal-pass".to_string(),
            auth_password_hash: String::new(),
            external_passwords: Vec::new(),
            allowed_domains: vec!["f-a-c.co.jp".to_string()],
            allowed_domains_extra: Vec::new(),
            hellowork_db_path: String::new(),
            indeed_db_path: String::new(),
            cache_ttl_secs: 60,
            cache_max_entries: 10,
            rate_limit_max_attempts: 50,
            rate_limit_lockout_secs: 60,
            audit_turso_url: String::new(),
            audit_turso_token: String::new(),
            audit_ip_salt: String::new(),
            admin_emails: vec![ADMIN.to_string()],
            turso_external_url: String::new(),
            turso_external_token: String::new(),
            salesnow_turso_url: String::new(),
            salesnow_turso_token: String::new(),
            scout_turso_url: String::new(),
            scout_turso_token: String::new(),
        },
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        crm_hubspot: None,
        hubspot,
    })
}

async fn inject_session(session: Session, Json(v): Json<Value>) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, v["email"].as_str().unwrap())
        .await
        .unwrap();
    session
        .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, "google_oidc")
        .await
        .unwrap();
    StatusCode::NO_CONTENT
}

async fn login(app: &Router, email: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/__test/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"email": email}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("id="))
        .map(|c| c.split(';').next().unwrap().to_string())
        .expect("cookie")
}

async fn get_json(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
    let mut b = Request::builder().uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(
        !text.contains(SECRET) && !text.contains("test-token"),
        "leak: {text}"
    );
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

struct Env {
    app: Router,
    st: Shared,
    admin: String,
    bpo: String,
    lone: String,
    nobody: String,
}

fn owner(id: &str, first: &str, teams: &[&str]) -> Value {
    json!({"id": id, "firstName": first, "lastName": "架空", "email": format!("{first}@example.invalid"),
        "teams": teams.iter().map(|t| json!({"id":"1","name":t})).collect::<Vec<_>>()})
}

async fn env() -> Env {
    let t = day_start();
    let mut f = FakeHs::default();
    f.owners_by_email = HashMap::from([
        (
            BPO.to_string(),
            ("7".to_string(), vec!["架空A".to_string()]),
        ),
        (LONE.to_string(), ("8".to_string(), vec![])),
    ]);
    f.owner_list = vec![
        owner("7", "太郎", &["架空A"]),
        owner("8", "花子", &[]),
        owner("9", "次郎", &["架空A"]),
        owner("10", "三郎", &["架空B"]),
    ];
    let mut id = 0;
    let mut add = |f: &mut FakeHs, owner: &str, n: usize, dur: i64| {
        for _ in 0..n {
            id += 1;
            f.calls.push(fc(
                &id.to_string(),
                owner,
                "OUTBOUND",
                dur,
                t + 60_000 * id,
                t,
            ));
        }
    };
    add(&mut f, "7", 4, 100_000); // 4 件・90 秒以上 4
    add(&mut f, "9", 6, 0); // 6 件・通話なし
    add(&mut f, "10", 10, 400_000); // 別チーム
    add(&mut f, "8", 2, 31_000);
    f.deals = vec![
        ("d1".into(), "7".into(), t + 5),
        ("d2".into(), "7".into(), t + 6),
    ];
    let (client, st) = start_hs(f).await;
    let app = Router::new()
        .merge(super::routes::router_with_queue(
            CrmAccess::from_list(&format!("{ADMIN},{BPO},{LONE},{NOBODY}"))
                .with_test_role(ADMIN, CrmRole::Admin)
                .with_test_role(BPO, CrmRole::Bpo)
                .with_test_role(LONE, CrmRole::Bpo)
                .with_test_role(NOBODY, CrmRole::Bpo),
            CallQueueState::for_test([7u8; 32], now0()),
        ))
        .route("/__test/session", post(inject_session))
        .with_state(test_state(Some(client)))
        .layer(SessionManagerLayer::new(MemoryStore::default()));
    Env {
        admin: login(&app, ADMIN).await,
        bpo: login(&app, BPO).await,
        lone: login(&app, LONE).await,
        nobody: login(&app, NOBODY).await,
        app,
        st,
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 本人の成績_同じチームの平均と順位() {
    let e = env().await;
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today", Some(&e.bpo)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["day"], "2026-10-05");
    assert_eq!(v["as_of"], "2026-10-05T03:00:00Z");
    assert_eq!(v["stale"], false);
    assert_eq!(v["me"]["owner_id"], "7");
    let m = &v["me"]["metrics"];
    assert_eq!(
        (
            m["outbound"].as_u64(),
            m["over_90s"].as_u64(),
            m["appointments"].as_u64()
        ),
        (Some(4), Some(4), Some(2))
    );
    assert_eq!(m["total_duration_ms"], 400_000);
    assert_eq!(m["avg_duration_ms"], 100_000);
    assert_eq!(m["rate_90s"], 1.0);
    // 集団 = 架空A で今日架電した 7 と 9 (別チームの 10 と、チームの無い 8 は入らない)
    assert_eq!(
        (v["population"].as_str(), v["population_size"].as_u64()),
        (Some("team"), Some(2))
    );
    assert_eq!(v["team_avg"]["outbound"], 5.0);
    // 架電数: 4 は 6 に次いで 2 位。通話時間・90秒以上・アポは 1 位
    assert_eq!(v["me"]["rank"]["outbound"], 2);
    assert_eq!(v["me"]["rank"]["total_duration"], 1);
    assert_eq!(v["me"]["rank"]["over_90s"], 1);
    assert_eq!(v["me"]["rank"]["appointments"], 1);
    assert_eq!(v["team_names"], json!(["架空A"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn チームが無い人は今日架電した全員と比べる_http() {
    let e = env().await;
    let (_, v) = get_json(&e.app, "/api/crm/metrics/today", Some(&e.lone)).await;
    assert_eq!(
        (v["population"].as_str(), v["population_size"].as_u64()),
        (Some("all"), Some(4))
    );
    assert_eq!(v["me"]["metrics"]["outbound"], 2);
    // 2 件は 10, 6, 4 に次いで 4 位
    assert_eq!(v["me"]["rank"]["outbound"], 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn 担当者を引けない人は_me_が_null_で_理由を返す() {
    let e = env().await;
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today", Some(&e.nobody)).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["me"].is_null());
    assert!(v["message"].as_str().unwrap().contains("結び付けられない"));
    assert_eq!(v["population_size"], 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn 全員の表は管理者だけ_それ以外は_403() {
    let e = env().await;
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today/all", Some(&e.bpo)).await;
    assert_eq!(
        (s, v["error_kind"].as_str()),
        (StatusCode::FORBIDDEN, Some("forbidden"))
    );
    assert!(v.get("callers").is_none());
    let (s, _) = get_json(&e.app, "/api/crm/metrics/today/all", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let (s, _) = get_json(&e.app, "/api/crm/metrics/today", None).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    // 拒否のときは HubSpot を呼ばない
    assert!(e.st.lock().unwrap().searches.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者は全員の表を見られる() {
    let e = env().await;
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today/all", Some(&e.admin)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let rows = v["callers"].as_array().unwrap();
    let ids: Vec<&str> = rows
        .iter()
        .map(|r| r["owner_id"].as_str().unwrap())
        .collect();
    // 架電数の多い順
    assert_eq!(ids, ["10", "9", "7", "8"]);
    assert_eq!(rows[0]["name"], "三郎 架空");
    assert_eq!(rows[0]["teams"], json!(["架空B"]));
    assert_eq!(rows[2]["metrics"]["appointments"], 2);
    assert_eq!(v["teams"], json!(["架空A", "架空B"]));
}

#[tokio::test(flavor = "multi_thread")]
async fn 担当者一覧が取れなくても数字は返す() {
    let e = env().await;
    e.st.lock().unwrap().owner_list_fail = true;
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today/all", Some(&e.admin)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["callers"][0]["name"], "(名前を取得できません)");
    // 本人の応答は全員との比較に倒れる
    let (s, v) = get_json(&e.app, "/api/crm/metrics/today", Some(&e.bpo)).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        (v["population"].as_str(), v["population_size"].as_u64()),
        (Some("all"), Some(4))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 利用者が増えても_hubspot_の_search_は共有される() {
    let e = env().await;
    for c in [&e.bpo, &e.lone, &e.admin, &e.bpo] {
        let _ = get_json(&e.app, "/api/crm/metrics/today", Some(c)).await;
    }
    assert_eq!(searches_of(&e.st, "calls").len(), 1);
    assert_eq!(searches_of(&e.st, "deals").len(), 1);
}

#[test]
fn アポはアポ獲得者に付き_空なら担当者に付く() {
    // アポ獲得者あり: 担当者 (新規営業) ではなく獲得者
    let with = json!({"hubspot_owner_id": "sales1", "bpo_26": "caller1"});
    assert_eq!(appointment_owner(&with), Some("caller1"));
    // 空文字・空白だけ・項目なしは担当者
    let empty = json!({"hubspot_owner_id": "sales1", "bpo_26": ""});
    assert_eq!(appointment_owner(&empty), Some("sales1"));
    let blank = json!({"hubspot_owner_id": "sales2", "bpo_26": "  "});
    assert_eq!(appointment_owner(&blank), Some("sales2"));
    let absent = json!({"hubspot_owner_id": "sales3"});
    assert_eq!(appointment_owner(&absent), Some("sales3"));
    // どちらも無ければ数えない
    assert_eq!(appointment_owner(&json!({"bpo_26": null})), None);
}

#[test]
fn 架電0件のアポ獲得者は一覧に出て_集団には入らない() {
    let calls = [out("1", "c1", 1000), out("2", "c2", 2000)];
    // c3 は今日 1 件も架電していないがアポを取った
    let apo = HashMap::from([("c1".to_string(), 1u32), ("c3".to_string(), 2u32)]);
    let all = aggregate(&calls, &apo);
    let z = &all["c3"];
    assert_eq!(
        (z.appointments, z.outbound, z.inbound, z.talked),
        (2, 0, 0, 0)
    );
    assert!(z.first_call_at.is_none() && z.last_call_at.is_none());
    // 管理者の一覧の条件 (get_today_all) に当たる
    assert!(z.outbound > 0 || z.inbound > 0 || z.appointments > 0);
    assert!(!is_caller(z));
    // 集団 (平均・順位の母数) は架電した c1, c2 の 2 人だけ
    let (me, avg, n, label) = build_today(&all, Some("c3"), &[], &HashMap::new());
    assert_eq!((n, label), (2, "all"));
    assert_eq!(me.as_ref().unwrap().rank.appointments, None);
    // 平均のアポは (1 + 0) / 2。c3 の 2 件は入らない
    assert_eq!(avg.unwrap().appointments, 0.5);
}

#[test]
fn アポ欄はアポを数えるチームの人かチーム無しの人にだけ出す() {
    let t = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    assert!(appointments_tracked(&t(&["BPO_リクロジ"])));
    assert!(!appointments_tracked(&t(&["新規営業"])));
    assert!(appointments_tracked(&[]));
    assert!(appointments_tracked(&t(&["BPO_リクロジ", "新規営業"])));
    assert!(!appointments_tracked(&t(&["新規営業", "コンサル営業"])));
}
