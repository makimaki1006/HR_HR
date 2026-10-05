//! 営業KPI: HubSpot 直読み(段階 1)のテスト (2026-10-05)
//!
//! 本物の HubSpot・Sheets・Zoom には一切つながない。127.0.0.1 に偽の HubSpot を立てる。
//!
//! 1. **旧新一致**: `tests/fixtures/sales_kpi/hubspot_direct/golden.json` は、Python 版
//!    `sync_daily.py` の実物の関数に同じ偽応答(`scenario.json`)を食わせて記録した「シートに書かれる行」と
//!    「HubSpot に投げた検索本文」(`scripts/sales_kpi_hubspot_direct_golden.py`)。Rust が組む行・本文と一致する。
//! 2. **経路の一致**: 既存の TSV fixture を HubSpot の応答に逆変換して直読み経路に通し、
//!    シート経由の payload と値で一致する。
//! 3. **逆証明**: 0 件 / 1 万件上限 / 429 / タイムアウト / 401 / 応答の欠落 / 日付境界 /
//!    プロパティ欠損 / 更新中の二重起動 / 初回取得前 / 失敗時に直前の値を出し続ける、など。
//! 4. 1 回の更新あたりの HubSpot 呼び出し回数を固定する。

use std::collections::BTreeMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body,
    extract::State,
    http::{Request, StatusCode},
    response::Response,
    Router,
};
use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime};
use serde_json::{json, Value};

use super::{fixture_day, fixture_sheets, load_tsv, payload_of, sheet_from_tsv};
use crate::handlers::call_quality::sheets::SheetData;
use crate::handlers::sales_kpi::hubspot_direct::{
    build_sheets, merge_kettei, overlay_meta, DirectState, FetchFn, RunOutcome, Snapshot,
};
use crate::handlers::sales_kpi::hubspot_source::{
    deal_row, exclusions_from_sheet, fetch_blocks, jst_midnight_ms, jst_text,
    roster_from_member_sheet, DirectBlocks, Exclusions, FetchError, RosterEntry, Windows,
    DEAL_HEADER,
};
use crate::handlers::sales_kpi::{Sheets, ST_C};
use crate::hubspot::{ClientOptions, HubSpotClient, HubSpotRecord};

// ---------------------------------------------------------------- 偽 HubSpot

#[derive(Debug, Clone)]
struct Rec {
    method: String,
    path: String,
    query: Vec<(String, String)>,
    body: Value,
}

struct Resp {
    status: u16,
    headers: Vec<(&'static str, String)>,
    body: Value,
    delay: Duration,
}

impl Resp {
    fn ok(body: Value) -> Self {
        Self {
            status: 200,
            headers: Vec::new(),
            body,
            delay: Duration::ZERO,
        }
    }
    fn status(status: u16) -> Self {
        Self {
            status,
            headers: Vec::new(),
            body: json!({"message": "x"}),
            delay: Duration::ZERO,
        }
    }
}

type Responder = Arc<dyn Fn(usize, &Rec) -> Resp + Send + Sync>;

struct Fake {
    calls: Mutex<Vec<Rec>>,
    responder: Responder,
}

impl Fake {
    fn calls(&self) -> Vec<Rec> {
        self.calls.lock().unwrap().clone()
    }
    fn count(&self) -> usize {
        self.calls.lock().unwrap().len()
    }
    /// 商談の検索(`limit=200`)の本文を順番どおり
    fn list_searches(&self) -> Vec<Value> {
        self.calls()
            .into_iter()
            .filter(|c| c.path.ends_with("/deals/search") && c.body["limit"] == json!(200))
            .map(|c| c.body)
            .collect()
    }
    /// 件数取り(`limit=1`)の filters を順番どおり
    fn count_filters(&self) -> Vec<Value> {
        self.calls()
            .into_iter()
            .filter(|c| c.path.ends_with("/deals/search") && c.body["limit"] == json!(1))
            .map(|c| c.body["filterGroups"][0]["filters"].clone())
            .collect()
    }
}

async fn handle(State(fake): State<Arc<Fake>>, req: Request<Body>) -> Response {
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 64 * 1024 * 1024).await.unwrap();
    let query = parts
        .uri
        .query()
        .map(|q| {
            reqwest::Url::parse(&format!("http://x/?{q}"))
                .unwrap()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect()
        })
        .unwrap_or_default();
    let rec = Rec {
        method: parts.method.to_string(),
        path: parts.uri.path().to_string(),
        query,
        body: serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    };
    let idx = {
        let mut calls = fake.calls.lock().unwrap();
        calls.push(rec.clone());
        calls.len() - 1
    };
    let resp = (fake.responder)(idx, &rec);
    if !resp.delay.is_zero() {
        tokio::time::sleep(resp.delay).await;
    }
    let mut b = Response::builder()
        .status(StatusCode::from_u16(resp.status).unwrap())
        .header("content-type", "application/json");
    for (k, v) in &resp.headers {
        b = b.header(*k, v);
    }
    b.body(Body::from(resp.body.to_string())).unwrap()
}

async fn spawn_fake(responder: Responder) -> (String, Arc<Fake>) {
    let fake = Arc::new(Fake {
        calls: Mutex::new(Vec::new()),
        responder,
    });
    let router = Router::new().fallback(handle).with_state(fake.clone());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), fake)
}

fn fast_opts() -> ClientOptions {
    ClientOptions {
        timeout: Duration::from_secs(5),
        max_retries: 2,
        retry_base_delay: Duration::from_millis(1),
        search_min_interval: Duration::ZERO,
        rate_limited_min_wait: Duration::from_millis(1),
    }
}

fn client(base: &str, opts: ClientOptions) -> HubSpotClient {
    HubSpotClient::new("test-token-XYZ".to_string(), base, opts).unwrap()
}

// ---------------------------------------------------------------- 偽の応答(scenario.json)

fn dir() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sales_kpi/hubspot_direct")
}

fn scenario() -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("scenario.json")).unwrap()).unwrap()
}

fn golden() -> Value {
    serde_json::from_str(&std::fs::read_to_string(dir().join("golden.json")).unwrap()).unwrap()
}

/// after をオフセットにした素朴なページング(`limit` 件ずつ)。
fn page(items: &[Value], body: &Value) -> Value {
    let start: usize = body["after"]
        .as_str()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let lim = body["limit"].as_u64().unwrap_or(200) as usize;
    let end = (start + lim).min(items.len());
    let mut out = json!({ "results": items[start.min(end)..end] });
    if end < items.len() {
        out["paging"] = json!({"next": {"after": end.to_string()}});
    }
    out
}

fn filters_of(body: &Value) -> Vec<Value> {
    body["filterGroups"]
        .as_array()
        .map(|g| {
            g.iter()
                .flat_map(|g| g["filters"].as_array().cloned().unwrap_or_default())
                .collect()
        })
        .unwrap_or_default()
}

/// scenario.json から答える偽 HubSpot (Python の golden を作ったときの偽物と同じ振る舞い)。
fn scenario_responder(scn: Value) -> Responder {
    Arc::new(move |_idx, rec| {
        if rec.path.ends_with("/crm/v3/objects/deals/search") {
            let filters = filters_of(&rec.body);
            let names: Vec<&str> = filters
                .iter()
                .filter_map(|f| f["propertyName"].as_str())
                .collect();
            if rec.body["limit"] == json!(1) {
                // 件数取り
                let key = if let Some(f) = filters
                    .iter()
                    .find(|f| f["propertyName"] == "dealstage" && f["operator"] == "EQ")
                {
                    format!("stage:{}", f["value"].as_str().unwrap())
                } else if let Some(f) = filters.iter().find(|f| f["operator"] == "HAS_PROPERTY") {
                    format!("has:{}", f["propertyName"].as_str().unwrap())
                } else {
                    "all".to_string()
                };
                return Resp::ok(json!({"total": scn["counts"][&key], "results": []}));
            }
            let items = |k: &str| scn[k].as_array().cloned().unwrap_or_default();
            let set = if names.contains(&"scheduled_business_meeting_date") {
                items("shodan")
            } else if names.iter().any(|n| n.starts_with("hs_v2_date_entered_")) {
                items("apo")
            } else if filters
                .iter()
                .any(|f| f["propertyName"] == "dealstage" && f["value"] == ST_C)
            {
                items("cyomi")
            } else if names.contains(&"ketteishamei") {
                items("kettei")
            } else {
                panic!("想定外の検索 {}", rec.body);
            };
            return Resp::ok(page(&set, &rec.body));
        }
        if rec.path.ends_with("/crm/v3/owners") {
            let archived = rec
                .query
                .iter()
                .any(|(k, v)| k == "archived" && v == "true");
            let after = rec
                .query
                .iter()
                .find(|(k, _)| k == "after")
                .map(|(_, v)| v.clone());
            let pages = scn[if archived {
                "owners_archived"
            } else {
                "owners_active"
            }]
            .as_array()
            .unwrap();
            let pg = pages
                .iter()
                .find(|p| p["after"].as_str().map(String::from) == after)
                .unwrap_or_else(|| panic!("owners のページが無い {after:?}"));
            let mut body = json!({ "results": pg["results"] });
            if let Some(n) = pg["next_after"].as_str() {
                body["paging"] = json!({"next": {"after": n}});
            }
            return Resp::ok(body);
        }
        if rec.path.ends_with("/crm/v3/pipelines/deals") {
            return Resp::ok(json!({"results": [
                {"id": "62583420", "stages": [{"id": "z", "label": "別PL", "displayOrder": 0}]},
                {"id": "default", "stages": scn["stages"]},
            ]}));
        }
        panic!("想定外のパス {} {}", rec.method, rec.path)
    })
}

fn roster_of(scn: &Value) -> BTreeMap<String, RosterEntry> {
    scn["roster"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| {
            (
                k.clone(),
                RosterEntry {
                    name: v["name"].as_str().unwrap().to_string(),
                    team: v["team"].as_str().unwrap().to_string(),
                },
            )
        })
        .collect()
}

fn excl_of(scn: &Value) -> Exclusions {
    let set = |k: &str| {
        scn["exclusions"][k]
            .as_array()
            .map(|a| a.iter().map(|v| v.as_str().unwrap().to_string()).collect())
            .unwrap_or_default()
    };
    Exclusions {
        owner_ids: set("ownerId"),
        hubspot_teams: set("HubSpotチーム"),
    }
}

fn at(s: &str) -> DateTime<FixedOffset> {
    DateTime::parse_from_rfc3339(s).unwrap()
}

fn rows_of(s: &SheetData) -> Vec<Vec<String>> {
    s.rows
        .iter()
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect()
}

fn golden_rows(g: &Value, name: &str) -> Vec<Vec<String>> {
    g["primary"]["written"][name]["rows"]
        .as_array()
        .unwrap_or_else(|| panic!("golden に {name} が無い"))
        .iter()
        .map(|r| {
            r.as_array()
                .unwrap()
                .iter()
                .map(|c| c.as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

async fn run_scenario(scn: &Value, now: &str) -> (Result<DirectBlocks, FetchError>, Arc<Fake>) {
    let (base, fake) = spawn_fake(scenario_responder(scn.clone())).await;
    let c = client(&base, fast_opts());
    let r = fetch_blocks(&c, at(now), &roster_of(scn), &excl_of(scn)).await;
    (r, fake)
}

// ---------------------------------------------------------------- 1. 旧新一致 (Python の golden と値で一致)

#[tokio::test]
async fn pythonが書く行と値で一致する() {
    let scn = scenario();
    let g = golden();
    let (r, _) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    let b = r.expect("取得できる");

    for (name, sheet) in [
        ("KPI営業_商談", &b.shodan),
        ("KPI営業_アポ", &b.apo),
        ("KPI営業_Cヨミ", &b.cyomi),
        ("KPI営業_メンバー", &b.member),
        ("KPI営業_架電リスト", &b.kaden_list),
    ] {
        let want_header: Vec<String> = g["primary"]["written"][name]["header"]
            .as_array()
            .unwrap()
            .iter()
            .map(|c| c.as_str().unwrap().to_string())
            .collect();
        assert_eq!(sheet.header, want_header, "{name}: 見出し");
        assert_eq!(rows_of(sheet), golden_rows(&g, name), "{name}: 行");
    }
    // 決定者(当日分)。Python は upsert するキー順に直すが、渡す行は「多い順」
    assert_eq!(b.kettei_rows, golden_rows(&g, "upsert:KPI営業_決定者"));
    assert_eq!(b.kettei_day, "2026-10-01");
    // 具体値(逆証明: 要素の存在ではなく値で)
    assert_eq!(b.shodan.rows.len(), 6);
    assert_eq!(&*b.shodan.rows[4][0], "5001");
    assert_eq!(&*b.shodan.rows[4][5], "2026-10-01 00:00"); // 2026-09-30T15:00Z = JST 0 時
    assert_eq!(&*b.shodan.rows[4][15], "決裁者商談");
    assert_eq!(&*b.shodan.rows[3][7], "2026-07-03 00:00"); // 2026-07-02T15:00Z
}

#[test]
fn 日時の読み方はpythonのjst_textと一致する() {
    let g = golden();
    let table = g["jst_text"].as_array().unwrap();
    assert!(table.len() >= 14);
    for pair in table {
        let input = pair[0].as_str();
        let want = pair[1].as_str().unwrap();
        assert_eq!(jst_text(input), want, "入力 {input:?}");
    }
}

#[tokio::test]
async fn 投げる検索本文はpythonと一致する() {
    let scn = scenario();
    let g = golden();
    let (r, fake) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    r.unwrap();
    let want: Vec<Value> = g["primary"]["searches"].as_array().unwrap().clone();
    assert_eq!(
        fake.list_searches(),
        want,
        "商談・アポ・Cヨミ・決定者の本文"
    );
    // 件数取りの filters(ステージ別 → 全体 → 充足 4)
    let want_counts: Vec<Value> = g["primary"]["count_jobs"][0]
        .as_array()
        .unwrap()
        .iter()
        .map(|job| job[1].clone())
        .collect();
    assert_eq!(want_counts.len(), 11);
    assert_eq!(fake.count_filters(), want_counts);
}

/// 日付の境目(月初・月末・週またぎ・年またぎ・閏でない 3/1)ごとの検索本文も Python と一致する。
#[tokio::test]
async fn 日付の境目でも検索本文はpythonと一致する() {
    let scn = scenario();
    let g = golden();
    for d in g["dates"].as_array().unwrap() {
        let day = d["today"].as_str().unwrap();
        let (r, fake) = run_scenario(&scn, &format!("{day}T09:30:00+09:00")).await;
        r.unwrap();
        let want: Vec<Value> = d["searches"].as_array().unwrap().clone();
        assert_eq!(fake.list_searches(), want, "{day} の検索本文");
    }
}

// ---------------------------------------------------------------- 日付の窓

fn d(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}

#[test]
fn 窓は月初と週またぎで広いほうを取る() {
    // 月初(10/1 木): 前月 1 日 9/1 より 60 日前 8/2 のほうが前。hi は翌月 1 日
    let w = Windows::of(d("2026-10-01"));
    assert_eq!((w.lo, w.hi), (d("2026-08-02"), d("2026-11-01")));
    assert_eq!(
        (w.month_start, w.month_end),
        (d("2026-10-01"), d("2026-11-01"))
    );
    assert_eq!(w.week_start, d("2026-09-28"));
    // 月曜(11/30): 週の終わり +14 日(12/14)が翌月 1 日(12/1)より後
    let w = Windows::of(d("2026-11-30"));
    assert_eq!(w.hi, d("2026-12-14"));
    assert_eq!(w.week_start, d("2026-11-30"));
    // 年またぎ(1/1 金)
    let w = Windows::of(d("2027-01-01"));
    assert_eq!(
        (w.month_start, w.month_end),
        (d("2027-01-01"), d("2027-02-01"))
    );
    // 12/31 は月末。翌月 1 日は 1/1
    assert_eq!(Windows::of(d("2026-12-31")).month_end, d("2027-01-01"));
}

#[test]
fn jstの0時はエポックミリ秒で前日15時utc() {
    // 2026-10-01 00:00 JST = 2026-09-30 15:00 UTC
    assert_eq!(jst_midnight_ms(d("2026-10-01")), "1790780400000");
    assert_eq!(jst_midnight_ms(d("2026-10-02")), "1790866800000");
}

#[tokio::test]
async fn 今日はjstの日付で決まりutcではない() {
    let scn = scenario();
    // 2026-09-30 15:00 UTC = 2026-10-01 00:00 JST → 当日は 10/1
    let (r, _) = run_scenario(&scn, "2026-09-30T15:00:00+00:00").await;
    assert_eq!(r.unwrap().kettei_day, "2026-10-01");
    // JST 0 時の 1 分前は前日のまま
    let (r, _) = run_scenario(&scn, "2026-09-30T23:59:00+09:00").await;
    let b = r.unwrap();
    assert_eq!(b.kettei_day, "2026-09-30");
    assert_eq!(b.windows.month_start, d("2026-09-01"));
    assert_eq!(b.fetched_at, "2026-09-30 23:59");
}

// ---------------------------------------------------------------- 4. 呼び出し回数

#[tokio::test]
async fn 一回の更新の呼び出し回数は式どおり() {
    let scn = scenario();
    let (r, fake) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    let b = r.unwrap();
    // 商談・アポ・Cヨミ・決定者 各 1 ページ = 4、Owners(在籍 2 ページ + 退職 1 ページ) = 3、
    // パイプライン 1、架電リスト = ステージ 6 + 全体 1 + 充足 4 = 11
    assert_eq!(b.requests, 4 + 3 + 1 + 11);
    assert_eq!(fake.count(), 19, "HTTP の実回数も同じ(retry なし)");
    let owners: Vec<(String, Option<String>)> = fake
        .calls()
        .iter()
        .filter(|c| c.path.ends_with("/owners"))
        .map(|c| {
            (
                c.query
                    .iter()
                    .find(|(k, _)| k == "archived")
                    .unwrap()
                    .1
                    .clone(),
                c.query
                    .iter()
                    .find(|(k, _)| k == "after")
                    .map(|(_, v)| v.clone()),
            )
        })
        .collect();
    assert_eq!(
        owners,
        vec![
            ("false".to_string(), None),
            ("false".to_string(), Some("p2".to_string())),
            ("true".to_string(), None),
        ]
    );
    // Search(POST)は 4 + 11 = 15、GET は Owners 3 + パイプライン 1
    let posts = fake.calls().iter().filter(|c| c.method == "POST").count();
    assert_eq!(posts, 15);
}

#[tokio::test]
async fn 件数はページ数で増え0件でも最低1回() {
    let mut scn = scenario();
    // 450 件 = 200 + 200 + 50 の 3 ページ
    let big: Vec<Value> = (0..450)
        .map(|i| json!({"id": (7000 + i).to_string(), "properties": {"hubspot_owner_id": "1001"}}))
        .collect();
    scn["shodan"] = json!(big);
    let (r, fake) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    let b = r.unwrap();
    assert_eq!(b.shodan.rows.len(), 450);
    assert_eq!(b.requests, 3 + 1 + 1 + 1 + 3 + 1 + 11);
    assert_eq!(fake.count(), 21);
    // ちょうど 200 件は 1 ページ(次ページの印が無い)
    let mut scn = scenario();
    scn["shodan"] = json!((0..200)
        .map(|i| json!({"id": (7000 + i).to_string(), "properties": {}}))
        .collect::<Vec<_>>());
    let (r, _) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    assert_eq!(r.unwrap().requests, 19);
}

// ---------------------------------------------------------------- 3. 逆証明

#[tokio::test]
async fn 全部0件でも組めて呼び出しは最低回数() {
    let mut scn = scenario();
    for k in ["shodan", "apo", "cyomi", "kettei"] {
        scn[k] = json!([]);
    }
    scn["owners_active"] = json!([{"after": null, "next_after": null, "results": []}]);
    scn["owners_archived"] = json!([{"after": null, "next_after": null, "results": []}]);
    scn["stages"] = json!([]);
    scn["roster"] = json!({});
    scn["exclusions"] = json!({});
    scn["counts"] = json!({"all": 0, "has:ketteishamei": 0, "has:ketteishanoyakushoku": 0,
        "has:kessaishamei": 0, "has:kessaishanoyakushoku": 0});
    let (r, _) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    let b = r.unwrap();
    // 4 検索 + Owners 2(在籍・退職 各 1 ページ)+ パイプライン 1 + (全体 1 + 充足 4)
    assert_eq!(b.requests, 4 + 2 + 1 + 5);
    assert!(b.shodan.rows.is_empty() && b.apo.rows.is_empty() && b.cyomi.rows.is_empty());
    assert!(b.kettei_rows.is_empty() && b.member.rows.is_empty());
    assert_eq!(b.shodan.header, DEAL_HEADER.map(String::from).to_vec());
    // 0 件は「合計 0」と「充足 0 × 4」の行になる(行ごと無いのではない)
    assert_eq!(
        rows_of(&b.kaden_list),
        vec![
            vec!["合計", "アポ前パイプライン全体", "", "0"],
            vec!["充足", "決定者名", "", "0"],
            vec!["充足", "決定者の役職", "", "0"],
            vec!["充足", "決裁者名", "", "0"],
            vec!["充足", "決裁者の役職", "", "0"],
        ]
    );
    assert!(b.truncated.is_empty());
}

#[tokio::test]
async fn 一万件の上限では9800件で打ち切り打ち切りを記録する() {
    let mut scn = scenario();
    scn["shodan"] = json!((0..10_050)
        .map(
            |i| json!({"id": (100000 + i).to_string(), "properties": {"hubspot_owner_id": "1001"}})
        )
        .collect::<Vec<_>>());
    let (r, fake) = run_scenario(&scn, scn["now_jst"].as_str().unwrap()).await;
    let b = r.unwrap();
    assert_eq!(
        b.shodan.rows.len(),
        9800,
        "Python と同じく 9,800 件で止める"
    );
    assert_eq!(b.truncated, vec!["商談"]);
    // 商談は 49 ページ(200 × 49 = 9,800)。残りは通常
    assert_eq!(b.requests, 49 + 1 + 1 + 1 + 3 + 1 + 11);
    let after_values: Vec<_> = fake
        .list_searches()
        .iter()
        .take(49)
        .map(|b| b["after"].clone())
        .collect();
    assert_eq!(after_values[0], Value::Null);
    assert_eq!(after_values[1], json!("200"));
    assert_eq!(after_values[48], json!("9600"));
    // 打ち切りは meta に出る(黙らない)
    let snap = Snapshot::Ready {
        blocks: Arc::new(b),
        error: None,
    };
    let meta = overlay_meta(&sheet_from_tsv("項目\t値\n"), &snap);
    assert_eq!(meta_get(&meta, "HubSpot打ち切り"), "商談");
}

fn meta_get(m: &SheetData, key: &str) -> String {
    m.rows
        .iter()
        .find(|r| m.get(r, "項目") == key)
        .map(|r| m.get(r, "値").to_string())
        .unwrap_or_else(|| panic!("meta に {key} が無い"))
}

fn meta_has(m: &SheetData, key: &str) -> bool {
    m.rows.iter().any(|r| m.get(r, "項目") == key)
}

#[tokio::test]
async fn 回数制限429はretryして成功しretry尽きたら種別を返す() {
    let scn = scenario();
    // 最初の 2 回の Search だけ 429(Retry-After: 0)。retry で成功する
    let inner = scenario_responder(scn.clone());
    let responder: Responder = Arc::new(move |idx, rec| {
        if idx < 2 {
            let mut r = Resp::status(429);
            r.headers.push(("retry-after", "0".to_string()));
            return r;
        }
        inner(idx, rec)
    });
    let (base, fake) = spawn_fake(responder).await;
    let b = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .expect("retry で成功");
    assert_eq!(b.requests, 19, "論理回数は retry を数えない");
    assert_eq!(fake.count(), 21, "HTTP は 429 の 2 回ぶん多い");

    // 毎回 429: 1 回目の検索で retry を尽くし(1 + 2 回)、そこで更新を止める。他の呼び出しはしない
    let (base, fake) = spawn_fake(Arc::new(|_, _| Resp::status(429))).await;
    let e = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .unwrap_err();
    assert_eq!(e.kind, "hubspot_rate_limited");
    assert_eq!(fake.count(), 3);
}

#[tokio::test]
async fn タイムアウトと認証エラーは種別つきで失敗する() {
    let scn = scenario();
    let mut opts = fast_opts();
    opts.timeout = Duration::from_millis(100);
    opts.max_retries = 0;
    let (base, _) = spawn_fake(Arc::new(|_, _| {
        let mut r = Resp::ok(json!({}));
        r.delay = Duration::from_millis(600);
        r
    }))
    .await;
    let e = fetch_blocks(
        &client(&base, opts),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .unwrap_err();
    assert_eq!(e.kind, "hubspot_timeout");

    for (status, kind) in [
        (401, "hubspot_auth"),
        (403, "hubspot_auth"),
        (500, "hubspot_upstream"),
    ] {
        let (base, fake) = spawn_fake(Arc::new(move |_, _| Resp::status(status))).await;
        let e = fetch_blocks(
            &client(&base, fast_opts()),
            at("2026-10-01T09:30:00+09:00"),
            &roster_of(&scn),
            &excl_of(&scn),
        )
        .await
        .unwrap_err();
        assert_eq!(e.kind, kind, "HTTP {status}");
        // 認証エラーは retry しない
        if status != 500 {
            assert_eq!(fake.count(), 1);
        }
        // 失敗の文面にトークン・応答本文を入れない
        let text = format!("{e} {e:?}");
        assert!(
            !text.contains("test-token-XYZ") && !text.contains("\"message\""),
            "{text}"
        );
    }
}

#[tokio::test]
async fn 応答の形が崩れたら0件に見せずに失敗する() {
    let scn = scenario();
    // results が無い検索
    let (base, _) = spawn_fake(Arc::new(|_, _| Resp::ok(json!({"total": 3})))).await;
    let e = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .unwrap_err();
    assert_eq!(e.kind, "hubspot_decode");

    // 件数取りの応答に total が無い
    let inner = scenario_responder(scn.clone());
    let responder: Responder = Arc::new(move |idx, rec| {
        if rec.body["limit"] == json!(1) {
            return Resp::ok(json!({"results": []}));
        }
        inner(idx, rec)
    });
    let (base, _) = spawn_fake(responder).await;
    let e = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .unwrap_err();
    assert_eq!(e.kind, "hubspot_decode");

    // 次ページがあると言いながら 0 件(無限ループにしない)
    let (base, fake) = spawn_fake(Arc::new(|_, _| {
        Resp::ok(json!({"results": [], "paging": {"next": {"after": "1"}}}))
    }))
    .await;
    let e = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-10-01T09:30:00+09:00"),
        &roster_of(&scn),
        &excl_of(&scn),
    )
    .await
    .unwrap_err();
    assert_eq!(e.kind, "hubspot_decode");
    assert_eq!(fake.count(), 1);
}

#[test]
fn プロパティや関連が欠けた取引でも行は16列で空文字になる() {
    let bare = HubSpotRecord {
        id: "42".into(),
        properties: BTreeMap::new(),
        created_at: None,
        updated_at: None,
        archived: false,
    };
    let row = deal_row(&bare);
    assert_eq!(row.len(), DEAL_HEADER.len());
    assert_eq!(row[0], "42");
    assert!(row[1..].iter().all(String::is_empty), "{row:?}");
    // null と空白だけの値は「有」にならない
    let mut props = BTreeMap::new();
    props.insert("jizenanketo_gyoushu".to_string(), None);
    props.insert(
        "jizenanketo_kyoten".to_string(),
        Some("  \u{3000} ".to_string()),
    );
    let r = HubSpotRecord {
        properties: props,
        ..bare.clone()
    };
    assert_eq!(deal_row(&r)[8], "");
    // 1 つでも入っていれば「有」(フォームが差し替わっても落ちない)
    let mut props = BTreeMap::new();
    props.insert(
        "jizenanketo_kaishamei".to_string(),
        Some("旧フォーム".to_string()),
    );
    let r = HubSpotRecord {
        properties: props,
        ..bare
    };
    assert_eq!(deal_row(&r)[8], "有");
}

#[test]
fn 名簿と除外はシートから復元する() {
    let m = sheet_from_tsv(
        "ownerId\t氏名\tチーム\t在籍\t出どころ\n1\t山田\t伊壺チーム\t在籍\t名簿\n2\t鈴木\tチーム未設定\t在籍\tHubSpot\n\t空\tx\t\t名簿\n",
    );
    let r = roster_from_member_sheet(&m);
    assert_eq!(r.len(), 1, "出どころが名簿で ownerId があるものだけ");
    assert_eq!(r["1"].name, "山田");
    assert_eq!(r["1"].team, "伊壺チーム");
    let e = exclusions_from_sheet(&sheet_from_tsv(
        "種別\t値\t理由\nHubSpotチーム\t BPO \tx\nownerId\t9\t\nownerId\t\t値なし\n不明\t1\t\n",
    ));
    assert_eq!(e.hubspot_teams.iter().collect::<Vec<_>>(), vec!["BPO"]);
    assert_eq!(e.owner_ids.iter().collect::<Vec<_>>(), vec!["9"]);
}

// ---------------------------------------------------------------- 決定者・メタの重ね合わせ

#[test]
fn 決定者は当日だけ差し替え過去日はシートのまま() {
    let sheet = sheet_from_tsv(
        "日付\townerId\t決定者名\t決定者の役職\t決裁者名\t決裁者の役職\t合計\n\
         2026-10-01\t1\t9\t9\t9\t9\t36\n\
         2026-10-01\t99\t1\t0\t0\t0\t1\n\
         2026-09-30\t1\t2\t2\t2\t2\t8\n",
    );
    let live = vec![vec![
        "2026-10-01".to_string(),
        "1".into(),
        "1".into(),
        "1".into(),
        "1".into(),
        "1".into(),
        "4".into(),
    ]];
    let m = merge_kettei(&sheet, "2026-10-01", &live);
    assert_eq!(
        rows_of(&m),
        vec![
            vec!["2026-09-30", "1", "2", "2", "2", "2", "8"],
            vec!["2026-10-01", "1", "1", "1", "1", "1", "4"],
        ],
        "当日のシート行(99 を含む)は捨て、過去日は残す"
    );
    // シートが空(見出しも無い)でも当日分は出る
    let m = merge_kettei(
        &SheetData {
            header: vec![],
            rows: vec![],
            fetched_at: std::time::Instant::now(),
        },
        "2026-10-01",
        &live,
    );
    assert_eq!(
        m.header,
        super::super::hubspot_source::KETTEI_HEADER
            .map(String::from)
            .to_vec()
    );
    assert_eq!(m.rows.len(), 1);
}

fn dummy_blocks(fetched_at: &str) -> DirectBlocks {
    let e = || super::super::empty_sheet();
    DirectBlocks {
        shodan: e(),
        apo: e(),
        cyomi: e(),
        member: e(),
        kaden_list: e(),
        kettei_day: "2026-10-01".to_string(),
        kettei_rows: vec![],
        windows: Windows::of(d("2026-10-01")),
        fetched_at: fetched_at.to_string(),
        requests: 19,
        took_secs: 7,
        truncated: vec![],
    }
}

fn zoom_meta() -> SheetData {
    sheet_from_tsv(
        "項目\t値\n取得時刻\t2026-10-05 06:30\n取得元\tsync_daily.py\n架電の最終日\t2026-10-05\n\
         架電の取得時刻\t2026-10-05 09:00\n架電の最終日は途中\tはい\n当月\t2026-09\n",
    )
}

#[test]
fn メタはzoomの項目を残しhubspotの項目で上書きする() {
    let snap = Snapshot::Ready {
        blocks: Arc::new(dummy_blocks("2026-10-01 09:30")),
        error: None,
    };
    let m = overlay_meta(&zoom_meta(), &snap);
    // Zoom 由来はそのまま
    assert_eq!(meta_get(&m, "架電の最終日"), "2026-10-05");
    assert_eq!(meta_get(&m, "架電の取得時刻"), "2026-10-05 09:00");
    assert_eq!(meta_get(&m, "架電の最終日は途中"), "はい");
    // HubSpot 由来は上書き
    assert_eq!(meta_get(&m, "取得時刻"), "2026-10-01 09:30");
    assert_eq!(meta_get(&m, "当月"), "2026-10");
    assert_eq!(meta_get(&m, "商談の範囲"), "2026-08-02 〜 2026-11-01");
    assert_eq!(meta_get(&m, "今週のはじまり"), "2026-09-28");
    assert_eq!(meta_get(&m, "HubSpot取得状態"), "ok");
    assert_eq!(meta_get(&m, "HubSpot取得時刻"), "2026-10-01 09:30");
    assert_eq!(meta_get(&m, "HubSpot更新のリクエスト数"), "19");
    assert!(!meta_has(&m, "HubSpot最終失敗種別"));
    assert!(!meta_has(&m, "HubSpot打ち切り"));
}

// ---------------------------------------------------------------- 常駐キャッシュ・単一実行・失敗時の保持

fn counting_fetch(started: Arc<AtomicUsize>, release: Option<Arc<tokio::sync::Notify>>) -> FetchFn {
    Arc::new(move || {
        let started = started.clone();
        let release = release.clone();
        Box::pin(async move {
            started.fetch_add(1, Ordering::SeqCst);
            if let Some(r) = release {
                r.notified().await;
            }
            Ok(dummy_blocks("2026-10-01 09:30"))
        })
    })
}

#[tokio::test]
async fn 初回取得前はloadingでリクエストはhubspotを叩かない() {
    let started = Arc::new(AtomicUsize::new(0));
    let state = DirectState::new(counting_fetch(started.clone(), None));
    // リクエストの経路 = snapshot() と build_sheets(): どちらも更新を起こさない
    let snap = state.snapshot();
    assert!(matches!(snap, Snapshot::Loading));
    let f = fixture_sheets();
    let sheets: Sheets = build_sheets(
        &snap,
        f.kaden.clone(),
        f.kaden_by_owner.clone(),
        f.weekly.clone(),
        &f.kettei,
        f.list_stock.clone(),
        &zoom_meta(),
        true,
    );
    assert_eq!(started.load(Ordering::SeqCst), 0, "更新は起きない");
    // 既存の画面が壊れない: 組める。HubSpot 由来の 6 ブロックは空で、取得中だと分かる
    let p = payload_of(&sheets, fixture_day());
    assert_eq!(p["meta"]["HubSpot取得状態"], "loading");
    assert_eq!(p["meta"]["取得元"], "HubSpot直読み(取得中)");
    assert_eq!(
        p["meta"]["架電の最終日"], "2026-10-05",
        "Zoom 由来の項目は残る"
    );
    assert_eq!(p["generated_at"], "", "取得時刻を作らない");
    // シート由来のブロック(週次)は出る
    assert!(!p["snapshots"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn 更新中は二重に始めない() {
    let started = Arc::new(AtomicUsize::new(0));
    let release = Arc::new(tokio::sync::Notify::new());
    let state = DirectState::new(counting_fetch(started.clone(), Some(release.clone())));
    let a = {
        let s = state.clone();
        tokio::spawn(async move { s.run_once().await })
    };
    while started.load(Ordering::SeqCst) == 0 {
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    // 走っている間に来た更新は始まらない。リクエスト側の snapshot も待たされない(loading のまま)
    assert_eq!(state.run_once().await, RunOutcome::AlreadyRunning);
    assert_eq!(state.run_once().await, RunOutcome::AlreadyRunning);
    assert!(matches!(state.snapshot(), Snapshot::Loading));
    assert_eq!(started.load(Ordering::SeqCst), 1);
    release.notify_one();
    assert_eq!(a.await.unwrap(), RunOutcome::Updated);
    assert!(matches!(
        state.snapshot(),
        Snapshot::Ready { error: None, .. }
    ));
    // 終われば次を始められる
    release.notify_one();
    assert_eq!(state.run_once().await, RunOutcome::Updated);
    assert_eq!(started.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn 背景ループは周期で更新し同時には走らない() {
    let started = Arc::new(AtomicUsize::new(0));
    let cur = Arc::new(AtomicUsize::new(0));
    let max = Arc::new(AtomicUsize::new(0));
    let (s2, c2, m2) = (started.clone(), cur.clone(), max.clone());
    let fetch: FetchFn = Arc::new(move || {
        let (s, c, m) = (s2.clone(), c2.clone(), m2.clone());
        Box::pin(async move {
            s.fetch_add(1, Ordering::SeqCst);
            let n = c.fetch_add(1, Ordering::SeqCst) + 1;
            m.fetch_max(n, Ordering::SeqCst);
            tokio::time::sleep(Duration::from_millis(25)).await;
            c.fetch_sub(1, Ordering::SeqCst);
            Ok(dummy_blocks("2026-10-01 09:30"))
        })
    });
    let state =
        DirectState::with_timing(fetch, Duration::from_millis(40), Duration::from_secs(3600));
    // リクエストが何度来てもループは 1 本
    for _ in 0..5 {
        state.touch();
    }
    tokio::time::sleep(Duration::from_millis(400)).await;
    let n = started.load(Ordering::SeqCst);
    assert!(n >= 3, "周期で複数回更新する: {n}");
    assert_eq!(max.load(Ordering::SeqCst), 1, "同時に 2 本走らない");
}

#[tokio::test]
async fn 強制更新は最短間隔を守る() {
    let started = Arc::new(AtomicUsize::new(0));
    let state = DirectState::new(counting_fetch(started.clone(), None));
    assert!(state.request_refresh(), "初回は起こす");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(started.load(Ordering::SeqCst), 1);
    assert!(!state.request_refresh(), "直後の連打は起こさない");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(started.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn 失敗しても直前の値を出し続け失敗と時刻を示す() {
    let n = Arc::new(AtomicUsize::new(0));
    let n2 = n.clone();
    let fetch: FetchFn = Arc::new(move || {
        let i = n2.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            match i {
                0 => Err(FetchError {
                    kind: "hubspot_timeout",
                    detail: "x".into(),
                }),
                1 => Ok(dummy_blocks("2026-10-01 09:30")),
                _ => Err(FetchError {
                    kind: "hubspot_rate_limited",
                    detail: "x".into(),
                }),
            }
        })
    });
    let state = DirectState::new(fetch);
    // 1 度も取れないまま失敗 → 値なし(503 にする側が使う)
    assert_eq!(
        state.run_once().await,
        RunOutcome::Failed("hubspot_timeout")
    );
    assert!(matches!(state.snapshot(), Snapshot::NoValue(ref e) if e.kind == "hubspot_timeout"));
    // 成功
    assert_eq!(state.run_once().await, RunOutcome::Updated);
    // その後の失敗: 直前の値(09:30)を保ち、失敗の種別と時刻を出す。シートには戻らない
    assert_eq!(
        state.run_once().await,
        RunOutcome::Failed("hubspot_rate_limited")
    );
    let snap = state.snapshot();
    match &snap {
        Snapshot::Ready { blocks, error } => {
            assert_eq!(blocks.fetched_at, "2026-10-01 09:30");
            let e = error.as_ref().expect("失敗が残る");
            assert_eq!(e.kind, "hubspot_rate_limited");
            assert_eq!(e.at.len(), "2026-10-01 09:30".len());
        }
        other => panic!("値を保つはず: {other:?}"),
    }
    let m = overlay_meta(&zoom_meta(), &snap);
    assert_eq!(meta_get(&m, "HubSpot取得状態"), "stale");
    assert_eq!(meta_get(&m, "HubSpot取得時刻"), "2026-10-01 09:30");
    assert_eq!(meta_get(&m, "取得時刻"), "2026-10-01 09:30");
    assert_eq!(meta_get(&m, "HubSpot最終失敗種別"), "hubspot_rate_limited");
    assert!(!meta_get(&m, "HubSpot最終失敗時刻").is_empty());
    // 次に成功すれば失敗の印は消える
    // (3 回目以降は常に失敗する偽物なので、ここでは状態の組み立てだけを確かめる)
    let ok = Snapshot::Ready {
        blocks: Arc::new(dummy_blocks("2026-10-01 09:35")),
        error: None,
    };
    assert_eq!(
        meta_get(&overlay_meta(&zoom_meta(), &ok), "HubSpot取得状態"),
        "ok"
    );
}

#[test]
fn 切り替えは既定でオフ() {
    // 環境変数を読むのはこのテストだけ(並列で走る他のテストは触らない)
    std::env::remove_var("SALES_KPI_HUBSPOT_DIRECT");
    assert!(!crate::handlers::sales_kpi::hubspot_direct::enabled());
    std::env::set_var("SALES_KPI_HUBSPOT_DIRECT", "1");
    assert!(crate::handlers::sales_kpi::hubspot_direct::enabled());
    std::env::set_var("SALES_KPI_HUBSPOT_DIRECT", "0");
    assert!(!crate::handlers::sales_kpi::hubspot_direct::enabled());
    std::env::set_var("SALES_KPI_HUBSPOT_DIRECT", "");
    assert!(!crate::handlers::sales_kpi::hubspot_direct::enabled());
    std::env::remove_var("SALES_KPI_HUBSPOT_DIRECT");
}

// ---------------------------------------------------------------- 2. 経路の一致 (既存 TSV fixture → HubSpot 応答 → 直読み)

fn jst_to_iso(s: &str) -> Value {
    if s.is_empty() {
        return Value::Null;
    }
    let n = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M").unwrap();
    json!((n - chrono::Duration::hours(9))
        .format("%Y-%m-%dT%H:%M:00Z")
        .to_string())
}

/// fixture の商談 1 行を、HubSpot の検索結果 1 件に戻す。
fn deal_json(sheet: &SheetData, row: &[Arc<str>]) -> Value {
    let g = |n: &str| sheet.get(row, n);
    json!({"id": g("dealId"), "properties": {
        "hubspot_owner_id": g("ownerId"),
        "pipeline": g("pipeline"),
        "dealstage": g("dealstage"),
        "scheduled_business_meeting_date": jst_to_iso(g("商談予定日時")),
        "jikan": g("時間"),
        "bpo_appo_date": jst_to_iso(g("BPOアポ取得日")),
        "jizenanketo_tantou_namae": if g("事前アンケート") == "有" { "回答" } else { "" },
        "aposyutokusya": g("アポ取得者"),
        "hs_v2_date_exited_52035886": jst_to_iso(g("アポ日確定を出た日")),
        "hs_v2_date_exited_1095457875": jst_to_iso(g("BPOアポ日確定を出た日")),
        "hs_v2_date_entered_52035886": jst_to_iso(g("アポ日確定に入った日")),
        "hs_v2_date_entered_52035889": jst_to_iso(g("Cヨミに入った日")),
    }})
}

/// 列を足して、シート経由で Python が書く形(商談種別・商談属性が空で付く)にする。
fn with_attr_cols(s: &SheetData) -> Arc<SheetData> {
    let mut header = s.header.clone();
    header.push("商談種別".into());
    header.push("商談属性".into());
    let rows = s
        .rows
        .iter()
        .map(|r| {
            let mut r = r.clone();
            r.push(Arc::from(""));
            r.push(Arc::from(""));
            r
        })
        .collect();
    Arc::new(SheetData {
        header,
        rows,
        fetched_at: s.fetched_at,
    })
}

#[tokio::test]
async fn fixtureをhubspot応答に戻して直読みしてもpayloadはシート経由と一致する() {
    let f = fixture_sheets();
    let day = "2026-09-04";
    assert_eq!(fixture_day().to_string(), day);

    // 商談・アポ・Cヨミ
    let deals = |s: &SheetData| -> Vec<Value> { s.rows.iter().map(|r| deal_json(s, r)).collect() };
    // 決定者(当日)
    let mut kettei_deals: Vec<Value> = Vec::new();
    let mut kettei_expect: Vec<Vec<String>> = Vec::new();
    for r in f
        .kettei
        .rows
        .iter()
        .filter(|r| f.kettei.get(r, "日付") == day)
    {
        let c: Vec<usize> = ["決定者名", "決定者の役職", "決裁者名", "決裁者の役職"]
            .iter()
            .map(|n| f.kettei.get(r, n).parse().unwrap())
            .collect();
        let total: usize = f.kettei.get(r, "合計").parse().unwrap();
        assert_eq!(c.iter().sum::<usize>(), total, "fixture の合計は 4 列の和");
        kettei_expect.push(r.iter().map(|x| x.to_string()).collect());
        let oid = f.kettei.get(r, "ownerId");
        for i in 0..*c.iter().max().unwrap() {
            let has = |k: usize| if i < c[k] { "入力" } else { "" };
            kettei_deals.push(json!({"id": format!("k{oid}-{i}"), "properties": {
                "hubspot_owner_id": oid,
                "ketteishamei": has(0), "ketteishanoyakushoku": has(1),
                "kessaishamei": has(2), "kessaishanoyakushoku": has(3),
            }}));
        }
    }
    assert!(!kettei_expect.is_empty(), "fixture に当日の決定者が無い");

    // メンバー: 氏名・チームは名簿、HubSpot チームは Owners、集計対象は除外
    let owners: Vec<Value> = f
        .member
        .rows
        .iter()
        .map(|r| {
            let hs = f.member.get(r, "HubSpotチーム");
            json!({"id": f.member.get(r, "ownerId"), "lastName": f.member.get(r, "氏名"),
                   "firstName": "", "email": "",
                   "teams": if hs.is_empty() { json!([]) } else { json!([{"id": "1", "name": hs, "primary": true}]) }})
        })
        .collect();
    let roster: BTreeMap<String, RosterEntry> = f
        .member
        .rows
        .iter()
        .map(|r| {
            (
                f.member.get(r, "ownerId").to_string(),
                RosterEntry {
                    name: f.member.get(r, "氏名").to_string(),
                    team: f.member.get(r, "チーム").to_string(),
                },
            )
        })
        .collect();
    let excl = Exclusions {
        owner_ids: f
            .member
            .rows
            .iter()
            .filter(|r| f.member.get(r, "集計対象") == "対象外")
            .map(|r| f.member.get(r, "ownerId").to_string())
            .collect(),
        hubspot_teams: Default::default(),
    };

    // 架電リスト: ステージ ID はクラスごとの既知の ID から割り当てる
    let mut pools: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    pools.insert("未架電", vec!["appointmentscheduled"]);
    pools.insert(
        "未接触",
        vec![
            "presentationscheduled",
            "decisionmakerboughtin",
            "qualifiedtobuy",
        ],
    );
    pools.insert(
        "接触済み",
        vec![
            "closedwon",
            "122445644",
            "122445645",
            "1366400580",
            "1332175104",
            "51997752",
        ],
    );
    pools.insert(
        "対象外",
        vec!["s1", "s2", "s3", "s4", "s5", "s6", "s7", "s8"],
    );
    let mut counts = serde_json::Map::new();
    let mut stages: Vec<Value> = Vec::new();
    for r in f.kaden_list.rows.iter() {
        let (kind, name, class, n) = (
            f.kaden_list.get(r, "区分"),
            f.kaden_list.get(r, "名前"),
            f.kaden_list.get(r, "分類"),
            f.kaden_list.get(r, "件数").parse::<u64>().unwrap(),
        );
        match kind {
            "ステージ" => {
                let id = pools.get_mut(class).unwrap().remove(0);
                stages.push(json!({"id": id, "label": name, "displayOrder": stages.len()}));
                counts.insert(format!("stage:{id}"), json!(n));
            }
            "合計" => {
                counts.insert("all".into(), json!(n));
            }
            "充足" => {
                let p = match name {
                    "決定者名" => "ketteishamei",
                    "決定者の役職" => "ketteishanoyakushoku",
                    "決裁者名" => "kessaishamei",
                    "決裁者の役職" => "kessaishanoyakushoku",
                    other => panic!("{other}"),
                };
                counts.insert(format!("has:{p}"), json!(n));
            }
            other => panic!("{other}"),
        }
    }
    let scn = json!({
        "shodan": deals(&f.shodan), "apo": deals(&f.apo), "cyomi": deals(&f.cyomi),
        "kettei": kettei_deals,
        "owners_active": [{"after": null, "next_after": null, "results": owners}],
        "owners_archived": [{"after": null, "next_after": null, "results": []}],
        "stages": stages, "counts": Value::Object(counts),
    });
    let (base, fake) = spawn_fake(scenario_responder(scn)).await;
    let b = fetch_blocks(
        &client(&base, fast_opts()),
        at("2026-09-04T10:00:00+09:00"),
        &roster,
        &excl,
    )
    .await
    .expect("取得できる");

    // 行そのものの一致(列は fixture にある分)
    assert_eq!(rows_of(&b.kaden_list), rows_of(&f.kaden_list), "架電リスト");
    let mut got = b.kettei_rows.clone();
    let mut want = kettei_expect;
    got.sort();
    want.sort();
    assert_eq!(got, want, "決定者の当日分");
    for (name, direct, sheet) in [
        ("商談", &b.shodan, &f.shodan),
        ("アポ", &b.apo, &f.apo),
        ("Cヨミ", &b.cyomi, &f.cyomi),
    ] {
        assert_eq!(direct.rows.len(), sheet.rows.len(), "{name}の件数");
        for (dr, sr) in direct.rows.iter().zip(&sheet.rows) {
            for col in &sheet.header {
                assert_eq!(
                    direct.get(dr, col),
                    sheet.get(sr, col),
                    "{name} の列「{col}」(dealId {})",
                    sheet.get(sr, "dealId")
                );
            }
        }
    }
    let mut got_m: Vec<Vec<String>> = b
        .member
        .rows
        .iter()
        .map(|r| {
            ["ownerId", "氏名", "チーム", "HubSpotチーム", "集計対象"]
                .iter()
                .map(|c| b.member.get(r, c).to_string())
                .collect()
        })
        .collect();
    let mut want_m: Vec<Vec<String>> = f
        .member
        .rows
        .iter()
        .map(|r| {
            ["ownerId", "氏名", "チーム", "HubSpotチーム", "集計対象"]
                .iter()
                .map(|c| f.member.get(r, c).to_string())
                .collect()
        })
        .collect();
    got_m.sort();
    want_m.sort();
    assert_eq!(got_m, want_m, "メンバー");

    // payload の一致: 直読み(HubSpot 由来 6 ブロック)+ シート(残り)と、全部シートの経路
    let snap = Snapshot::Ready {
        blocks: Arc::new(b),
        error: None,
    };
    let direct = build_sheets(
        &snap,
        f.kaden.clone(),
        f.kaden_by_owner.clone(),
        f.weekly.clone(),
        &f.kettei,
        f.list_stock.clone(),
        &f.meta,
        true,
    );
    let sheet_path = Sheets {
        shodan: with_attr_cols(&f.shodan),
        apo: with_attr_cols(&f.apo),
        cyomi: with_attr_cols(&f.cyomi),
        ..fixture_sheets()
    };
    let mut a = payload_of(&direct, fixture_day());
    let mut s = payload_of(&sheet_path, fixture_day());
    // 取得時刻まわりの meta と、それを写す generated_at だけは違って当然
    for p in [&mut a, &mut s] {
        let o = p.as_object_mut().unwrap();
        o.remove("meta");
        o.remove("generated_at");
    }
    assert_eq!(a, s, "シート経由と直読み経由の payload");
    // 一致が空振りでないこと(具体値)
    assert!(!a["people"].as_array().unwrap().is_empty());
    assert!(!a["week_deals"].as_array().unwrap().is_empty());
    assert!(!a["kettei"].is_null() && !a["kaden"].is_null());
    assert!(fake.count() > 15);
}

#[test]
fn fixtureの列定義は直読みの列定義を含む() {
    // 直読みが作る列名が、画面(Deal::from_row)の読む名前とずれていない
    let f = fixture_sheets();
    for col in &f.shodan.header {
        assert!(
            DEAL_HEADER.contains(&col.as_str()),
            "fixture の列「{col}」が直読みに無い"
        );
    }
    let _ = load_tsv("KPI営業_商談");
}
