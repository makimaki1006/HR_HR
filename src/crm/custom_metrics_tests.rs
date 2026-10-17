//! 帯の項目の個人設定 (`metric_prefs`) と自分の項目の数字 (`custom_metrics`) のテスト。
//! 偽 HubSpot・偽 Turso (本物には通信しない)。人名・ID はすべて架空。
#![allow(clippy::field_reassign_with_default)]

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration as StdDuration;

use axum::{
    body::Body,
    extract::{Path, State},
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
use super::custom_metrics::*;
use super::metric_prefs::*;
use super::property_catalog::{
    CatalogEntry, CrmCatalogObject, CrmCatalogProperty, CrmPropertyCatalogResponse,
    CrmPropertyGroup,
};
use super::queue_property_filter::PropertyCondition;
use super::rbac::{CrmAccess, CrmRole};
use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient, HubSpotError};
use crate::AppState;

const BPO: &str = "bpo@f-a-c.co.jp";
const LONE: &str = "lone@f-a-c.co.jp";

/// JST 2026-10-05 12:00
fn now0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap()
}

// ---------------------------------------------------------------------------
// 部品
// ---------------------------------------------------------------------------

fn prop(name: &str, kind: &str, owner_ref: bool) -> CrmCatalogProperty {
    CrmCatalogProperty {
        name: name.into(),
        label: format!("架空の{name}"),
        label_en: None,
        property_type: kind.into(),
        field_type: if kind == "enumeration" {
            "select"
        } else {
            "text"
        }
        .into(),
        options: vec![],
        owner_ref,
        writable: false,
    }
}

fn catalog() -> CatalogEntry {
    CatalogEntry::for_test(CrmPropertyCatalogResponse {
        objects: vec![CrmCatalogObject {
            object_type: "deals".into(),
            groups: vec![CrmPropertyGroup {
                name: "g".into(),
                label: "架空".into(),
                properties: vec![
                    prop("dealname", "string", false),
                    prop("amount", "number", false),
                    prop("closedate", "date", false),
                    prop("hubspot_owner_id", "enumeration", true),
                    prop("bpo_26", "enumeration", true),
                    prop("dealstage", "enumeration", false),
                    prop("pipeline", "enumeration", false),
                ],
            }],
        }],
        max_selected_per_object: 100,
        fetched_at: String::new(),
        cache_hit: false,
    })
}

fn cond(property: &str, operator: &str, values: &[&str]) -> PropertyCondition {
    PropertyCondition {
        property: property.into(),
        operator: operator.into(),
        values: values.iter().map(|v| v.to_string()).collect(),
    }
}

fn def(id: &str, kind: MetricKind, property: Option<&str>) -> CustomMetricDef {
    CustomMetricDef {
        id: id.into(),
        name: "架空の項目".into(),
        kind,
        property: property.map(str::to_string),
        condition_groups: vec![],
        owner_property: DEFAULT_OWNER_PROPERTY.into(),
    }
}

fn prefs_with(custom: Vec<CustomMetricDef>) -> MetricPrefs {
    let mut items: Vec<String> = DEFAULT_ITEMS.iter().map(|s| s.to_string()).collect();
    items.extend(custom.iter().map(|c| c.id.clone()));
    MetricPrefs { items, custom }
}

// ---------------------------------------------------------------------------
// 設定の形と上限
// ---------------------------------------------------------------------------

#[test]
fn 既定は今までの帯と同じ4項目で_自分の項目は無い() {
    let p = MetricPrefs::default();
    assert_eq!(
        p.items,
        ["outbound", "over_90s", "duration", "appointments"]
    );
    assert!(p.custom.is_empty());
    assert!(validate_shape(&p).is_ok());
}

#[test]
fn 自分の項目は6個まで_条件は5個まで() {
    let six: Vec<_> = (0..6)
        .map(|i| def(&format!("c-{i}"), MetricKind::Count, None))
        .collect();
    assert!(validate_shape(&prefs_with(six.clone())).is_ok());
    let mut seven = six;
    seven.push(def("c-7", MetricKind::Count, None));
    assert_eq!(
        validate_shape(&prefs_with(seven)).unwrap_err().0,
        "自分の項目は 6 個までです。"
    );
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("amount", "GTE", &["1"]); 5]];
    assert!(validate_shape(&prefs_with(vec![d.clone()])).is_ok());
    d.condition_groups[0].push(cond("amount", "GTE", &["1"]));
    assert_eq!(
        validate_shape(&prefs_with(vec![d])).unwrap_err().0,
        "条件は 1 つのまとまりに 5 個までです。"
    );
}

#[test]
fn 名前_識別子_数え方_並びの不正を拒む() {
    let bad = |f: &dyn Fn(&mut CustomMetricDef)| {
        let mut d = def("c-a", MetricKind::Count, None);
        f(&mut d);
        validate_shape(&prefs_with(vec![d])).is_err()
    };
    assert!(bad(&|d| d.name = "   ".into()));
    assert!(bad(&|d| d.name = "あ".repeat(13)));
    assert!(!bad(&|d| d.name = "あ".repeat(12)));
    assert!(bad(&|d| d.name = "改行\nあり".into()));
    assert!(bad(&|d| d.id = "outbound".into()));
    assert!(bad(&|d| d.id = "c-ABC".into()));
    assert!(bad(&|d| d.property = Some("amount".into())));
    assert!(bad(&|d| d.kind = MetricKind::Sum));
    assert!(bad(
        &|d| d.condition_groups = vec![vec![cond("amount", "GTE", &[&"9".repeat(201)])]]
    ));
    // 同じ識別子が 2 つ
    let p = prefs_with(vec![
        def("c-a", MetricKind::Count, None),
        def("c-a", MetricKind::Count, None),
    ]);
    assert!(validate_shape(&p).is_err());
    // 並びに知らない項目・重複
    let mut p = MetricPrefs::default();
    p.items.push("c-none".into());
    assert!(validate_shape(&p).is_err());
    let mut p = MetricPrefs::default();
    p.items.push("outbound".into());
    assert!(validate_shape(&p).is_err());
    // 並びから外した (出さない) 自分の項目は持っていてよい
    let mut p = prefs_with(vec![def("c-a", MetricKind::Count, None)]);
    p.items.retain(|i| i != "c-a");
    assert!(validate_shape(&p).is_ok());
}

#[test]
fn 保存する形は未知の項目を受け付けない() {
    assert!(serde_json::from_value::<MetricPrefs>(json!({"items": [], "extra": 1})).is_err());
    let p: MetricPrefs = serde_json::from_value(json!({
        "items": ["c-a"],
        "custom": [{"id": "c-a", "name": "不在", "kind": "count", "conditions": []}]
    }))
    .unwrap();
    // 「誰の」の既定は取引の担当者
    assert_eq!(p.custom[0].owner_property, "hubspot_owner_id");
}

// ---------------------------------------------------------------------------
// 項目の型の検証 (400 になる定義)
// ---------------------------------------------------------------------------

#[test]
fn 誰のは担当者型だけ_合計と平均は数値の項目だけ() {
    let c = catalog();
    let ok = |d: CustomMetricDef| validate_against_catalog(&prefs_with(vec![d]), &c, now0());
    assert!(ok(def("c-a", MetricKind::Count, None)).is_ok());
    assert!(ok(def("c-a", MetricKind::Sum, Some("amount"))).is_ok());
    assert!(ok(def("c-a", MetricKind::Avg, Some("amount"))).is_ok());
    let mut d = def("c-a", MetricKind::Count, None);
    d.owner_property = "bpo_26".into();
    assert!(ok(d).is_ok());
    let mut d = def("c-a", MetricKind::Count, None);
    d.owner_property = "dealname".into();
    assert_eq!(
        ok(d).unwrap_err().0,
        "「架空の項目」: 「誰の」には担当者を選ぶ型の項目を選んでください。"
    );
    assert!(ok(def("c-a", MetricKind::Sum, Some("dealname"))).is_err());
    assert!(ok(def("c-a", MetricKind::Avg, Some("no_such"))).is_err());
    // 条件は詳細条件と同じ検証 (選択肢・型)
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("amount", "CONTAINS_TOKEN", &["x"])]];
    assert!(ok(d).is_err());
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("no_such", "EQ", &["x"])]];
    assert!(ok(d).is_err());
}

#[test]
fn 相対日付は同じ仕組みで解決し_今月も使える() {
    let c = catalog();
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("closedate", "THIS_MONTH", &[])]];
    let r = resolve_metric(&d, &c, now0(), &value_scope()).unwrap();
    // 2026-10-01 〜 2026-10-31 (日付型は UTC 0 時の暦日)
    let lo = Utc
        .with_ymd_and_hms(2026, 10, 1, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    let hi = Utc
        .with_ymd_and_hms(2026, 10, 31, 0, 0, 0)
        .unwrap()
        .timestamp_millis();
    assert_eq!(
        r.filter_groups[0],
        vec![
            json!({"propertyName":"closedate","operator":"BETWEEN","value":lo.to_string(),"highValue":hi.to_string()})
        ]
    );
    d.condition_groups = vec![vec![cond("closedate", "TODAY", &[])]];
    let today = resolve_metric(&d, &c, now0(), &value_scope()).unwrap();
    let tomorrow = resolve_metric(&d, &c, now0() + Duration::days(1), &value_scope()).unwrap();
    assert_ne!(today.filter_groups, tomorrow.filter_groups);
    // ステージは架電キューのパイプラインのステージだけ
    d.condition_groups = vec![vec![cond("dealstage", "IN", &["1095457875"])]];
    assert!(resolve_metric(&d, &c, now0(), &value_scope()).is_ok());
    d.condition_groups = vec![vec![cond("dealstage", "IN", &["unknown-stage"])]];
    assert!(resolve_metric(&d, &c, now0(), &value_scope()).is_err());
}

// ---------------------------------------------------------------------------
// 集計
// ---------------------------------------------------------------------------

fn deal(owner: Option<&str>, amount: Option<&str>) -> Value {
    let mut p = json!({});
    if let Some(o) = owner {
        p["hubspot_owner_id"] = json!(o);
    }
    if let Some(a) = amount {
        p["amount"] = json!(a);
    }
    json!({"id": "x", "properties": p})
}

#[test]
fn ownerごとに件数_合計_値のある件数を数える() {
    let rows = vec![
        deal(Some("1"), Some("100")),
        deal(Some("1"), Some("50.5")),
        deal(Some("1"), None),
        deal(Some("2"), Some("abc")),
        deal(Some("2"), Some("10")),
        deal(None, Some("999")),
        deal(Some(" "), Some("999")),
    ];
    let a = aggregate(&rows, "hubspot_owner_id", Some("amount"));
    assert_eq!(a.len(), 2);
    assert_eq!(
        a["1"],
        OwnerAgg {
            count: 3,
            sum: 150.5,
            with_value: 2
        }
    );
    assert_eq!(
        a["2"],
        OwnerAgg {
            count: 2,
            sum: 10.0,
            with_value: 1
        }
    );
    let c = aggregate(&rows, "hubspot_owner_id", None);
    assert_eq!((c["1"].count, c["1"].with_value), (3, 0));
}

fn aggs(v: &[(&str, u64, f64, u64)]) -> HashMap<String, OwnerAgg> {
    v.iter()
        .map(|(o, c, s, w)| {
            (
                o.to_string(),
                OwnerAgg {
                    count: *c,
                    sum: *s,
                    with_value: *w,
                },
            )
        })
        .collect()
}

#[test]
fn 件数と合計の平均は1人あたり_順位は多い順で同値は同順位() {
    let pop: Vec<String> = ["1", "2", "3", "4"].iter().map(|s| s.to_string()).collect();
    // 4 は取引なし (0 として数える)。5 は比べる相手に居ないので数えない
    let a = aggs(&[
        ("1", 5, 500.0, 5),
        ("2", 8, 100.0, 2),
        ("3", 5, 300.0, 3),
        ("5", 99, 9_999.0, 99),
    ]);
    let s = summarize(MetricKind::Count, &a, &pop, "1");
    assert_eq!(s.value, Some(5.0));
    assert_eq!(s.team_avg, Some(18.0 / 4.0));
    assert_eq!((s.rank, s.ranked), (Some(2), 4));
    assert_eq!(summarize(MetricKind::Count, &a, &pop, "3").rank, Some(2));
    assert_eq!(summarize(MetricKind::Count, &a, &pop, "4").rank, Some(4));
    let s = summarize(MetricKind::Sum, &a, &pop, "1");
    assert_eq!(s.value, Some(500.0));
    assert_eq!(s.team_avg, Some(900.0 / 4.0));
    assert_eq!(s.rank, Some(1));
}

#[test]
fn 平均は値のある取引だけで割り_相手の平均は全員の合計から() {
    let pop: Vec<String> = ["1", "2", "3"].iter().map(|s| s.to_string()).collect();
    let a = aggs(&[("1", 5, 500.0, 5), ("2", 8, 100.0, 2), ("3", 4, 0.0, 0)]);
    let s = summarize(MetricKind::Avg, &a, &pop, "2");
    // 2 の平均 = 100 / 2 = 50。相手の平均 = (500 + 100) / (5 + 2)。値が無い 3 は順位に入れない
    assert_eq!(s.value, Some(50.0));
    assert_eq!(s.team_avg, Some(600.0 / 7.0));
    assert_eq!((s.rank, s.ranked), (Some(2), 2));
    let none = summarize(MetricKind::Avg, &a, &pop, "3");
    assert_eq!((none.value, none.rank), (None, None));
}

#[test]
fn 比べる相手は同じチームの人_チームが無ければ今日架電した全員() {
    let o = |id: &str, teams: &[&str], archived: bool| super::owners::CrmOwner {
        id: id.into(),
        name: "架空".into(),
        email: None,
        archived,
        teams: teams.iter().map(|t| t.to_string()).collect(),
    };
    let list = vec![
        o("7", &["架空A"], false),
        o("9", &["架空A", "架空C"], false),
        o("10", &["架空B"], false),
        o("11", &["架空A"], true),
    ];
    let callers = vec!["10".to_string(), "12".to_string()];
    let (p, l) = population("7", &["架空A".into()], Some(&list), &callers);
    assert_eq!((p, l), (vec!["7".to_string(), "9".to_string()], "team"));
    let (p, l) = population("8", &[], Some(&list), &callers);
    assert_eq!(
        (p, l),
        (
            vec!["10".to_string(), "12".to_string(), "8".to_string()],
            "all"
        )
    );
    // 担当者一覧が取れないときも全員
    let (_, l) = population("7", &["架空A".into()], None, &callers);
    assert_eq!(l, "all");
}

// ---------------------------------------------------------------------------
// キャッシュ
// ---------------------------------------------------------------------------

fn resolved(kind: MetricKind, filters: Vec<Value>) -> ResolvedMetric {
    ResolvedMetric {
        filter_groups: vec![filters],
        kind,
        property: (kind != MetricKind::Count).then(|| "amount".to_string()),
        owner_property: "hubspot_owner_id".into(),
    }
}

#[test]
fn キャッシュの鍵は定義が違えば混ざらない() {
    let f1 = vec![json!({"propertyName":"amount","operator":"GTE","value":"1"})];
    let f2 = vec![json!({"propertyName":"amount","operator":"GTE","value":"2"})];
    let pop = vec!["1".to_string(), "2".to_string()];
    let base = cache_key(&resolved(MetricKind::Count, f1.clone()), &pop);
    assert_eq!(
        base,
        cache_key(&resolved(MetricKind::Count, f1.clone()), &pop)
    );
    let mut keys = vec![
        base,
        cache_key(&resolved(MetricKind::Count, f2), &pop),
        cache_key(&resolved(MetricKind::Sum, f1.clone()), &pop),
        cache_key(&resolved(MetricKind::Avg, f1.clone()), &pop),
        cache_key(&resolved(MetricKind::Count, f1.clone()), &["1".to_string()]),
    ];
    let mut other_owner = resolved(MetricKind::Count, f1.clone());
    other_owner.owner_property = "bpo_26".into();
    keys.push(cache_key(&other_owner, &pop));
    let mut other_prop = resolved(MetricKind::Sum, f1);
    other_prop.property = Some("other_amount".into());
    keys.push(cache_key(&other_prop, &pop));
    let n = keys.len();
    keys.sort();
    keys.dedup();
    assert_eq!(keys.len(), n);
    // 相対日付は解決した値が鍵に入る (日が変われば別の鍵)
    let c = catalog();
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("closedate", "TODAY", &[])]];
    let a = resolve_metric(&d, &c, now0(), &value_scope()).unwrap();
    let b = resolve_metric(&d, &c, now0() + Duration::days(1), &value_scope()).unwrap();
    assert_ne!(cache_key(&a, &pop), cache_key(&b, &pop));
}

#[test]
fn キャッシュは2分使い回し_失敗したら前の値を古いとして返し_30秒は読み直さない() {
    let cache = CustomMetricsCache::default();
    let t = now0();
    let rows = Fetched::Rows(Arc::new(aggs(&[("1", 3, 0.0, 0)])));
    // 最初は読む
    let (l, start) = cache.begin("k", t);
    assert_eq!((l, start), (Lookup::Loading, true));
    // 読んでいる間は 2 本目を走らせない
    assert_eq!(cache.begin("k", t), (Lookup::Loading, false));
    cache.finish("k", t, Ok(rows.clone()));
    // 2 分以内は読まない
    assert_eq!(
        cache.begin("k", t + Duration::seconds(119)),
        (Lookup::Value(rows.clone(), t, false), false)
    );
    // 2 分を過ぎたら前の値を返しながら読み直す
    let t2 = t + Duration::seconds(120);
    assert_eq!(
        cache.begin("k", t2),
        (Lookup::Value(rows.clone(), t, false), true)
    );
    cache.finish("k", t2, Err(HubSpotError::Busy));
    // 失敗: 前の値を「古い」として返し、30 秒は読み直さない
    assert_eq!(
        cache.begin("k", t2 + Duration::seconds(29)),
        (Lookup::Value(rows, t, true), false)
    );
    assert!(cache.begin("k", t2 + Duration::seconds(30)).1);
    // 一度も読めていないまま失敗したら「読めない」
    let (_, s) = cache.begin("never", t);
    assert!(s);
    cache.finish("never", t, Err(HubSpotError::Busy));
    assert_eq!(cache.begin("never", t), (Lookup::Failed, false));
}

#[test]
fn 件数が多すぎる結果も同じように使い回し_画面に理由を出す() {
    let d = def("c-a", MetricKind::Count, None);
    let v = item_value(
        &d,
        &Lookup::Value(Fetched::TooMany, now0(), false),
        &["1".to_string()],
        "1",
    );
    assert_eq!(v.status, "too_many");
    assert_eq!(v.value, None);
    assert_eq!(
        v.message.as_deref(),
        Some("件数が多すぎます（条件を絞ってください）")
    );
}

// ---------------------------------------------------------------------------
// 偽 HubSpot (Search・定義・担当者)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeHs {
    /// (担当者, アポ獲得者, 金額)
    deals: Vec<(String, Option<String>, Option<f64>)>,
    /// total だけ大きく返す
    total_override: Option<u64>,
    duplicate_rows: bool,
    fail: bool,
    searches: Vec<Value>,
    owners_by_email: HashMap<String, (String, Vec<String>)>,
    owner_list: Vec<Value>,
}
type Shared = Arc<Mutex<FakeHs>>;

fn passes(f: &Value, props: &Value) -> bool {
    let name = f["propertyName"].as_str().unwrap();
    let v = props.get(name).and_then(Value::as_str);
    match f["operator"].as_str().unwrap() {
        "IN" => v.is_some_and(|v| f["values"].as_array().unwrap().iter().any(|x| x == v)),
        "GTE" => v
            .and_then(|v| v.parse::<f64>().ok())
            .is_some_and(|n| n >= f["value"].as_str().unwrap().parse::<f64>().unwrap()),
        "HAS_PROPERTY" => v.is_some(),
        other => panic!("偽 HubSpot が知らない比べ方: {other}"),
    }
}

async fn hs_search(
    State(st): State<Shared>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    assert_eq!(o, "deals");
    s.searches.push(body.clone());
    if s.fail {
        return (StatusCode::TOO_MANY_REQUESTS, "busy").into_response();
    }
    let groups = body["filterGroups"].as_array().unwrap();
    assert!(groups
        .iter()
        .all(|g| g["filters"].as_array().unwrap().len() <= 6));
    let rows: Vec<Value> = s
        .deals
        .iter()
        .enumerate()
        .map(|(i, (o, a, amt))| {
            let mut p = json!({"hubspot_owner_id": o});
            if let Some(a) = a {
                p["bpo_26"] = json!(a);
            }
            if let Some(n) = amt {
                p["amount"] = json!(n.to_string());
            }
            json!({"id": i.to_string(), "properties": p})
        })
        .filter(|r| {
            groups.iter().any(|g| {
                g["filters"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .all(|f| passes(f, &r["properties"]))
            })
        })
        .collect();
    let after: usize = body["after"].as_str().map_or(0, |a| a.parse().unwrap());
    let limit = body["limit"].as_u64().unwrap() as usize;
    let total = s.total_override.unwrap_or(rows.len() as u64);
    let mut page: Vec<Value> = rows.iter().skip(after).take(limit).cloned().collect();
    if s.duplicate_rows {
        page.extend(page.clone());
    }
    let mut v = json!({"results": page, "total": total});
    if after + limit < rows.len() {
        v["paging"] = json!({"next": {"after": (after + limit).to_string()}});
    }
    Json(v).into_response()
}

async fn hs_props(Path(o): Path<String>) -> Response {
    let results = if o == "deals" {
        json!([
            {"name":"dealname","label":"案件名","type":"string","fieldType":"text","groupName":"g"},
            {"name":"amount","label":"金額","type":"number","fieldType":"number","groupName":"g"},
            {"name":"closedate","label":"完了予定日","type":"date","fieldType":"date","groupName":"g"},
            {"name":"hubspot_owner_id","label":"担当者","type":"enumeration","fieldType":"select","groupName":"g","referencedObjectType":"OWNER"},
            {"name":"bpo_26","label":"アポ獲得者","type":"enumeration","fieldType":"select","groupName":"g","referencedObjectType":"OWNER"}
        ])
    } else {
        json!([])
    };
    Json(json!({"results": results})).into_response()
}

async fn hs_groups() -> Response {
    Json(json!({"results": [{"name": "g", "label": "架空", "displayOrder": 0}]})).into_response()
}

async fn hs_owners(
    State(st): State<Shared>,
    axum::extract::RawQuery(q): axum::extract::RawQuery,
) -> Response {
    let s = st.lock().unwrap();
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
    let archived = get("archived").as_deref() == Some("true");
    Json(json!({"results": if archived { vec![] } else { s.owner_list.clone() }})).into_response()
}

async fn start_hs(fake: FakeHs) -> (Arc<HubSpotClient>, Shared) {
    let st = Arc::new(Mutex::new(fake));
    let app = Router::new()
        .route("/crm/v3/objects/{o}/search", post(hs_search))
        .route("/crm/v3/properties/{o}", get(hs_props))
        .route("/crm/v3/properties/{o}/groups", get(hs_groups))
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

fn many(owner: &str, n: usize, amount: f64) -> Vec<(String, Option<String>, Option<f64>)> {
    (0..n)
        .map(|_| (owner.to_string(), None, Some(amount)))
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn 比べる相手の担当者_in_で読み_ページを追って数える() {
    let mut f = FakeHs::default();
    f.deals = [many("1", 250, 10.0), many("2", 30, 5.0), many("3", 9, 1.0)].concat();
    let (client, st) = start_hs(f).await;
    let r = resolved(
        MetricKind::Sum,
        vec![json!({"propertyName":"amount","operator":"GTE","value":"2"})],
    );
    let pop = vec!["1".to_string(), "2".to_string()];
    let Fetched::Rows(a) = fetch(&client, &r, &pop).await.unwrap() else {
        panic!("too many")
    };
    assert_eq!(a["1"].count, 250);
    assert_eq!(a["1"].sum, 2500.0);
    assert_eq!(a["2"].sum, 150.0);
    assert!(!a.contains_key("3"));
    let s = st.lock().unwrap();
    // 280 件 = 200 + 80 の 2 ページ。条件 + 担当者 IN
    assert_eq!(s.searches.len(), 2);
    let f0 = &s.searches[0]["filterGroups"][0]["filters"];
    assert_eq!(
        f0[1],
        json!({"propertyName":"hubspot_owner_id","operator":"IN","values":["1","2"]})
    );
    assert_eq!(
        s.searches[0]["properties"],
        json!(["hubspot_owner_id", "amount"])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 比べる相手が100人を超えたら100人ずつ読む() {
    let mut f = FakeHs::default();
    f.deals = [many("5", 3, 1.0), many("150", 2, 1.0)].concat();
    let (client, st) = start_hs(f).await;
    let r = resolved(MetricKind::Count, vec![]);
    let pop: Vec<String> = (0..150).map(|i| i.to_string()).collect();
    let Fetched::Rows(a) = fetch(&client, &r, &pop).await.unwrap() else {
        panic!()
    };
    assert_eq!(a["5"].count, 3);
    assert!(!a.contains_key("150"));
    let s = st.lock().unwrap();
    assert_eq!(s.searches.len(), 2);
    let len = |i: usize| {
        s.searches[i]["filterGroups"][0]["filters"][0]["values"]
            .as_array()
            .unwrap()
            .len()
    };
    assert_eq!((len(0), len(1)), (100, 50));
}

#[tokio::test(flavor = "multi_thread")]
async fn 二千件を超えたら1回目で止めて多すぎると返す() {
    let mut f = FakeHs::default();
    f.deals = many("1", 10, 1.0);
    f.total_override = Some(2_001);
    let (client, st) = start_hs(f).await;
    let r = resolved(MetricKind::Count, vec![]);
    assert_eq!(
        fetch(&client, &r, &["1".to_string()]).await.unwrap(),
        Fetched::TooMany
    );
    assert_eq!(st.lock().unwrap().searches.len(), 1);
    // ちょうど 2,000 件は読む
    let mut f = FakeHs::default();
    f.deals = many("1", 2_000, 1.0);
    let (client, st) = start_hs(f).await;
    let Fetched::Rows(a) = fetch(&client, &r, &["1".to_string()]).await.unwrap() else {
        panic!()
    };
    assert_eq!(a["1"].count, 2_000);
    assert_eq!(st.lock().unwrap().searches.len(), 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn 誰のをアポ獲得者にするとその項目で数える() {
    let mut f = FakeHs::default();
    f.deals = vec![
        ("1".into(), Some("2".into()), None),
        ("1".into(), Some("2".into()), None),
        ("1".into(), None, None),
    ];
    let (client, _) = start_hs(f).await;
    let mut r = resolved(MetricKind::Count, vec![]);
    r.owner_property = "bpo_26".into();
    let Fetched::Rows(a) = fetch(&client, &r, &["1".to_string(), "2".to_string()])
        .await
        .unwrap()
    else {
        panic!()
    };
    assert_eq!(a.get("2").map(|x| x.count), Some(2));
    assert!(!a.contains_key("1"));
}

// ---------------------------------------------------------------------------
// HTTP (設定の保存・読み取り・監査 DB 未接続・自分の項目の数字)
// ---------------------------------------------------------------------------

fn state(hubspot: Option<Arc<HubSpotClient>>, audit: Option<AuditDb>) -> Arc<AppState> {
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
            admin_emails: vec![],
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
        audit,
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

fn app(st: Arc<AppState>) -> Router {
    Router::new()
        .merge(super::routes::router_with_queue(
            CrmAccess::from_list(&format!("{BPO},{LONE}"))
                .with_test_role(BPO, CrmRole::Bpo)
                .with_test_role(LONE, CrmRole::Bpo),
            CallQueueState::for_test([7u8; 32], now0()),
        ))
        .route("/__test/session", post(inject_session))
        .with_state(st)
        .layer(SessionManagerLayer::new(MemoryStore::default()))
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

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let mut b = Request::builder()
        .method(method)
        .uri(uri)
        .header(header::COOKIE, cookie)
        .header("X-Requested-With", "fetch");
    let body = match body {
        Some(v) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let resp = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!text.contains("test-token"), "leak: {text}");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

#[tokio::test(flavor = "multi_thread")]
async fn 監査dbが無いとき_帯は既定で_設定は案内_保存は503() {
    let app = app(state(None, None));
    let c = login(&app, BPO).await;
    let (s, v) = send(&app, "GET", "/api/crm/metrics/prefs", &c, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["available"], false);
    assert_eq!(v["saved"], false);
    assert_eq!(
        v["prefs"]["items"],
        json!(["outbound", "over_90s", "duration", "appointments"])
    );
    assert_eq!(v["message"], UNAVAILABLE_MESSAGE);
    let (s, v) = send(
        &app,
        "PUT",
        "/api/crm/metrics/prefs",
        &c,
        Some(json!({"items": ["outbound"]})),
    )
    .await;
    assert_eq!(
        (s, v["error_kind"].as_str()),
        (StatusCode::SERVICE_UNAVAILABLE, Some("prefs_unavailable"))
    );
    let (s, v) = send(&app, "GET", "/api/crm/metrics/custom", &c, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["items"], json!([]));
    assert_eq!(v["message"], UNAVAILABLE_MESSAGE);
}

#[tokio::test(flavor = "multi_thread")]
async fn 不正な設定は400で_csrfの無い保存は403() {
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let (client, _) = start_hs(FakeHs::default()).await;
    let app = app(state(Some(client), Some(audit)));
    let c = login(&app, BPO).await;
    for body in [
        json!({"items": ["nope"]}),
        json!({"items": [], "unknown": 1}),
        json!({"items": ["c-a"], "custom": [{"id":"c-a","name":"金額","kind":"sum","conditions":[]}]}),
        json!({"items":["c-a"],"custom":[{"id":"c-a","name":"架空","kind":"count","condition_groups":[vec![cond("amount","GTE", &["1"]); 6]]}]}),
        json!({"items":["c-a"],"custom":[{"id":"c-a","name":"架空","kind":"count","conditions":[],"condition_groups":[]}]}),
        // 誰の が担当者型でない
        json!({"items": ["c-a"], "custom": [{"id":"c-a","name":"件数","kind":"count","owner_property":"dealname"}]}),
        // 合計の項目が数値でない
        json!({"items": ["c-a"], "custom": [{"id":"c-a","name":"合計","kind":"sum","property":"closedate"}]}),
    ] {
        let (s, v) = send(
            &app,
            "PUT",
            "/api/crm/metrics/prefs",
            &c,
            Some(body.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{body}");
        assert_eq!(v["error_kind"], "invalid_metric_prefs");
        assert!(v["message"].as_str().is_some_and(|m| !m.is_empty()));
    }
    // 大きすぎる本文
    let big = json!({"items": [], "custom": [], "pad": "x".repeat(MAX_PREFS_BYTES)});
    let (s, _) = send(&app, "PUT", "/api/crm/metrics/prefs", &c, Some(big)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    // X-Requested-With の無い PUT は CSRF で止める
    let resp = app
        .clone()
        .oneshot(
            Request::put("/api/crm/metrics/prefs")
                .header(header::COOKIE, &c)
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(json!({"items": []}).to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn 設定は人ごとに1行で保存し_読み取りは60秒キャッシュ_保存で捨てる() {
    let (audit, conn, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
    let store = PrefsStore::default();
    let reads = || {
        queries
            .lock()
            .unwrap()
            .iter()
            .filter(|q| q.contains("SELECT prefs FROM crm_metric_prefs"))
            .count()
    };
    assert_eq!(store.get(&audit, BPO).await.unwrap(), None);
    assert_eq!(store.get(&audit, BPO).await.unwrap(), None);
    assert_eq!(reads(), 1);
    let p = prefs_with(vec![def("c-a", MetricKind::Count, None)]);
    store.put(&audit, BPO, &p).await.unwrap();
    assert_eq!(store.get(&audit, BPO).await.unwrap(), Some(p.clone()));
    assert_eq!(reads(), 2);
    // 上書き (行は増えない)
    let mut p2 = p.clone();
    p2.items.retain(|i| i != "duration");
    store.put(&audit, BPO, &p2).await.unwrap();
    assert_eq!(store.get(&audit, BPO).await.unwrap(), Some(p2));
    // 別の人は別の行
    store
        .put(&audit, LONE, &MetricPrefs::default())
        .await
        .unwrap();
    let n: i64 = conn
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM crm_metric_prefs", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
    // 壊れた行は既定に戻す (None)
    conn.lock()
        .unwrap()
        .execute(
            "UPDATE crm_metric_prefs SET prefs='{broken' WHERE email=?",
            [LONE],
        )
        .unwrap();
    let fresh = PrefsStore::default();
    assert_eq!(fresh.get(&audit, LONE).await.unwrap(), None);
}

/// 「集計中」(`pending: true`) の間は取り直す (画面の取り直しと同じ)
async fn custom_until_ready(app: &Router, cookie: &str) -> Value {
    for _ in 0..200 {
        let (s, v) = send(app, "GET", "/api/crm/metrics/custom", cookie, None).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        if v["pending"] != true {
            return v;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    panic!("集計中のまま終わらない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 自分の項目は本人の値とチームの平均と順位を返し_同じ定義は人をまたいで共有する() {
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let mut f = FakeHs::default();
    f.owners_by_email = HashMap::from([
        (
            BPO.to_string(),
            ("7".to_string(), vec!["架空A".to_string()]),
        ),
        (
            LONE.to_string(),
            ("9".to_string(), vec!["架空A".to_string()]),
        ),
    ]);
    let owner = |id: &str, team: &str| {
        json!({"id": id, "firstName": "架空", "lastName": id, "email": format!("{id}@example.invalid"),
            "teams": [{"id": "1", "name": team}]})
    };
    f.owner_list = vec![
        owner("7", "架空A"),
        owner("9", "架空A"),
        owner("10", "架空B"),
    ];
    f.deals = [many("7", 3, 100.0), many("9", 5, 10.0), many("10", 50, 1.0)].concat();
    let (client, st) = start_hs(f).await;
    let app = app(state(Some(client), Some(audit)));
    let bpo = login(&app, BPO).await;
    let lone = login(&app, LONE).await;
    let prefs = json!({
        "items": ["outbound", "c-n", "c-s"],
        "custom": [
            {"id": "c-n", "name": "件数", "kind": "count",
             "conditions": [{"property": "amount", "operator": "GTE", "values": ["1"]}]},
            {"id": "c-s", "name": "今月の金額", "kind": "sum", "property": "amount",
             "conditions": [{"property": "amount", "operator": "GTE", "values": ["1"]}]},
            {"id": "c-h", "name": "出さない", "kind": "count"}
        ]
    });
    for c in [&bpo, &lone] {
        let (s, v) = send(
            &app,
            "PUT",
            "/api/crm/metrics/prefs",
            c,
            Some(prefs.clone()),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{v}");
    }
    let (_, v) = send(&app, "GET", "/api/crm/metrics/prefs", &bpo, None).await;
    assert_eq!(
        (v["saved"].as_bool(), v["available"].as_bool()),
        (Some(true), Some(true))
    );
    assert_eq!(v["prefs"]["items"], json!(["outbound", "c-n", "c-s"]));
    let v = custom_until_ready(&app, &bpo).await;
    assert_eq!(
        (v["population"].as_str(), v["population_size"].as_u64()),
        (Some("team"), Some(2))
    );
    // 並びに無い項目 (c-h) は数えない
    let items = v["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    assert_eq!(items[0]["id"], "c-n");
    assert_eq!(items[0]["status"], "ready");
    // 本人 7: 3 件。チーム (7, 9) の平均 (3 + 5) / 2 = 4。順位 2 位/2 人
    assert_eq!(
        (items[0]["value"].as_f64(), items[0]["team_avg"].as_f64()),
        (Some(3.0), Some(4.0))
    );
    assert_eq!(
        (items[0]["rank"].as_u64(), items[0]["ranked"].as_u64()),
        (Some(2), Some(2))
    );
    // 合計: 7 = 300、9 = 50 → 平均 175、順位 1 位
    assert_eq!(
        (items[1]["value"].as_f64(), items[1]["team_avg"].as_f64()),
        (Some(300.0), Some(175.0))
    );
    assert_eq!(items[1]["rank"].as_u64(), Some(1));
    let searches_after_first = st.lock().unwrap().searches.len();
    assert_eq!(searches_after_first, 2, "項目 1 つにつき 1 回");
    // 同じチームの別の人 (同じ定義・同じ比べる相手) はキャッシュを共有し、HubSpot を呼ばない
    let v = custom_until_ready(&app, &lone).await;
    assert_eq!(v["items"][0]["value"].as_f64(), Some(5.0));
    assert_eq!(v["items"][0]["rank"].as_u64(), Some(1));
    assert_eq!(st.lock().unwrap().searches.len(), searches_after_first);
}

async fn one_item_env(f: FakeHs) -> (Router, String, Shared) {
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let (client, st) = start_hs(f).await;
    let app = app(state(Some(client), Some(audit)));
    let bpo = login(&app, BPO).await;
    let prefs =
        json!({"items": ["c-n"], "custom": [{"id": "c-n", "name": "件数", "kind": "count"}]});
    let (s, _) = send(&app, "PUT", "/api/crm/metrics/prefs", &bpo, Some(prefs)).await;
    assert_eq!(s, StatusCode::OK);
    (app, bpo, st)
}

#[tokio::test(flavor = "multi_thread")]
async fn 一度も読めないまま失敗したら読めないと出す_チームの無い人は今日架電した全員と比べる() {
    let mut f = FakeHs::default();
    f.owners_by_email = HashMap::from([(BPO.to_string(), ("7".to_string(), vec![]))]);
    f.fail = true;
    let (app, bpo, _) = one_item_env(f).await;
    let v = custom_until_ready(&app, &bpo).await;
    // 今日の集計がまだ無いので本人だけ
    assert_eq!(
        (v["population"].as_str(), v["population_size"].as_u64()),
        (Some("all"), Some(1))
    );
    assert_eq!(v["items"][0]["status"], "unavailable");
    assert_eq!(v["items"][0]["value"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn 件数が多すぎる項目は数字を出さず理由を返す() {
    let mut f = FakeHs::default();
    f.owners_by_email = HashMap::from([(BPO.to_string(), ("7".to_string(), vec![]))]);
    f.deals = many("7", 3, 1.0);
    f.total_override = Some(5_000);
    let (app, bpo, st) = one_item_env(f).await;
    let v = custom_until_ready(&app, &bpo).await;
    assert_eq!(v["items"][0]["status"], "too_many");
    assert_eq!(v["items"][0]["value"], Value::Null);
    assert_eq!(v["items"][0]["message"], TOO_MANY_MESSAGE);
    assert_eq!(st.lock().unwrap().searches.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn 担当者と結び付けられない人には理由を返す() {
    let (app, bpo, st) = one_item_env(FakeHs::default()).await;
    let (s, v) = send(&app, "GET", "/api/crm/metrics/custom", &bpo, None).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["items"], json!([]));
    assert!(v["message"]
        .as_str()
        .unwrap()
        .contains("担当者と結び付けられない"));
    assert!(st.lock().unwrap().searches.is_empty());
}

#[test]
fn 旧条件を読み新形式だけを書き両形式の同時指定を拒む() {
    let old = json!({"id":"c-a", "name":"架空", "kind":"count", "conditions":[{"property":"amount","operator":"GTE","values":["10"]}]});
    let d: CustomMetricDef = serde_json::from_value(old.clone()).unwrap();
    assert_eq!(
        d.condition_groups,
        vec![vec![cond("amount", "GTE", &["10"])]]
    );
    let written = serde_json::to_value(&d).unwrap();
    assert!(written.get("conditions").is_none());
    assert_eq!(
        written["condition_groups"],
        json!([[{"property":"amount","operator":"GTE","values":["10"]}]])
    );
    assert_eq!(
        serde_json::from_value::<CustomMetricDef>(written).unwrap(),
        d
    );
    let mut both = old;
    both["condition_groups"] = json!([]);
    assert!(serde_json::from_value::<CustomMetricDef>(both).is_err());
    assert!(serde_json::from_value::<CustomMetricDef>(
        json!({"id":"c-a","name":"架空","kind":"count","conditions":[],"condition_groups":[]})
    )
    .is_err());
}

#[test]
fn 三まとまり五条件は担当者込み十八で可_六条件や四まとまりは不可() {
    let mut d = def("c-a", MetricKind::Count, None);
    d.condition_groups = vec![vec![cond("amount", "GTE", &["1"]); 5]; 3];
    assert!(validate_shape(&prefs_with(vec![d.clone()])).is_ok());
    let r = resolve_metric(&d, &catalog(), now0(), &value_scope()).unwrap();
    assert_eq!(
        r.filter_groups.iter().map(|g| g.len() + 1).sum::<usize>(),
        18
    );
    d.condition_groups[2].push(cond("amount", "GTE", &["2"]));
    assert_eq!(
        validate_shape(&prefs_with(vec![d.clone()])).unwrap_err().0,
        "条件は 1 つのまとまりに 5 個までです。"
    );
    d.condition_groups = vec![vec![cond("amount", "GTE", &["1"])]; 4];
    assert_eq!(
        validate_shape(&prefs_with(vec![d.clone()])).unwrap_err().0,
        "条件のまとまりは 3 つまでです。"
    );
    d.condition_groups = vec![vec![]];
    assert!(validate_shape(&prefs_with(vec![d])).is_err());
}

#[test]
fn ステージは同じまとまりのパイプライン内だけ_納品管理も選べる() {
    let mut d = def("c-a", MetricKind::Count, None);
    let resolve = |d: &CustomMetricDef| resolve_metric(d, &catalog(), now0(), &value_scope());
    d.condition_groups = vec![vec![
        cond("pipeline", "IN", &["753186575"]),
        cond("dealstage", "IN", &["1095387445"]),
    ]];
    assert!(resolve(&d).is_ok());
    d.condition_groups[0][0].values = vec!["default".into()];
    assert!(resolve(&d).is_err());
    d.condition_groups[0][0] = cond("pipeline", "NOT_IN", &["753186575"]);
    assert!(resolve(&d).is_err());
    // OR の別のまとまりにあるパイプラインは影響しない
    d.condition_groups = vec![
        vec![cond("pipeline", "IN", &["default"])],
        vec![cond("dealstage", "IN", &["1095387445"])],
    ];
    assert!(resolve(&d).is_ok());
    for p in super::queue_pipelines::write_pipelines() {
        d.condition_groups = vec![vec![
            cond("pipeline", "IN", &[p.id]),
            cond("dealstage", "IN", &[p.stages[0].0]),
        ]];
        assert!(resolve(&d).is_ok(), "{}", p.id);
    }
    assert_eq!(value_scope().pipelines.len(), 8);
}

#[tokio::test(flavor = "multi_thread")]
async fn またはの各まとまりに担当者を入れ一回の検索で和集合を数える() {
    let mut f = FakeHs::default();
    f.deals = [
        ("1".into(), Some("8".into()), Some(10.0)),
        ("1".into(), Some("8".into()), Some(30.0)),
        ("2".into(), None, Some(40.0)),
        ("9".into(), Some("8".into()), Some(50.0)),
    ]
    .into();
    f.duplicate_rows = true;
    let (client, state) = start_hs(f).await;
    let a = json!({"propertyName":"amount","operator":"GTE","value":"20"});
    let b = json!({"propertyName":"bpo_26","operator":"IN","values":["8"]});
    let mut r = resolved(MetricKind::Sum, vec![a.clone()]);
    let single_key = cache_key(&r, &["1".into(), "2".into()]);
    r.filter_groups.push(vec![b.clone()]);
    assert_ne!(single_key, cache_key(&r, &["1".into(), "2".into()]));
    let Fetched::Rows(aggs) = fetch(&client, &r, &["1".into(), "2".into()]).await.unwrap() else {
        panic!("too many")
    };
    // 30 は両方に一致し、偽応答も重複するが一度しか数えない。
    assert_eq!(
        aggs["1"],
        OwnerAgg {
            count: 2,
            sum: 40.0,
            with_value: 2
        }
    );
    assert_eq!(aggs["2"].sum, 40.0);
    assert!(!aggs.contains_key("9"));
    let state = state.lock().unwrap();
    assert_eq!(state.searches.len(), 1);
    let owner = json!({"propertyName":"hubspot_owner_id","operator":"IN","values":["1","2"]});
    assert_eq!(
        state.searches[0]["filterGroups"],
        json!([{"filters":[a, owner]},{"filters":[b, owner]}])
    );
}
