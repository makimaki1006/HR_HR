use super::*;
use axum::{body::Body, http::Request, routing::any};
use std::sync::Mutex;

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn today() -> NaiveDate {
    day("2026-10-21")
}
fn app(id: &str, props: Value) -> Value {
    json!({"id":id,"properties":props})
}
fn search_page1() -> Value {
    json!({"total":7,"results":[
        app("1",json!({"yingmuri":"2026-10-20","todoufuken":"神奈川県","shikuchouson":"横浜市港北区",
            "oubodouki_1":"給与／手当;勤務地／通勤（車通勤可等）","oubodouki":"家から近い"})),
        app("2",json!({"yingmuri":"2026-04-21","todoufuken":"大阪府","shikuchouson":"大阪市北区",
            "kinmujoukennoyuusenjuni_1":"給与／手当","kinmujoukennoyuusenjuni_2":"","kinmujoukennoyuusenjuni_3":"休日／休暇"})),
        app("3",json!({"yingmuri":"2026-04-20","oubodouki_1":"仕事内容"})),
        app("4",json!({"yingmuri":"2026-10-01","oubodouki":"文章だけ","tenshokuriyuu":"給与／手当"})),
    ],"paging":{"next":{"after":"p2"}}})
}
fn search_page2() -> Value {
    json!({"total":7,"results":[
        app("5",json!({"yingmuri":"","hs_createdate":"2026-10-19T16:00:00Z","todoufuken":"東京都","shikuchouson":"千代田区",
            "oubodouki_1":"その他（詳細）","honnin_kibou_kinyuuran":"090-1234-5678に連絡してください"})),
        app("6",json!({"yingmuri":"2026-09-10","kinmujoukennoyuusenjuni_1":"休日／休暇","oubosaki_shokushu":"その他営業"})),
        app("7",json!({"yingmuri":"2026-10-15","oubodouki_1":"仕事内容","oubosaki_shokushu":"宇宙飛行士"})),
        app("8",json!({"yingmuri":"2026-10-10","oubodouki_1":"名前の無い値","oubodouki":"あ".repeat(2500)})),
    ]})
}
const NONE: &str = "crm.associations.NO_ASSOCIATIONS_FOUND";
async fn serve(log: Arc<Mutex<Vec<String>>>) -> (JobReadService, tokio::task::JoinHandle<()>) {
    let app = Router::new().fallback(any(move |req: Request<Body>| {
        let log = log.clone();
        async move {
            let path = req.uri().path().to_owned();
            let body: Value = serde_json::from_slice(
                &axum::body::to_bytes(req.into_body(), usize::MAX).await.unwrap(),
            )
            .unwrap_or(Value::Null);
            log.lock().unwrap().push(format!("{path} {body}"));
            let value = if path == "/crm/v3/objects/0-421/search" {
                if body["limit"] == 1 {
                    // 全応募の件数: yingmuri の起点ごとに total を返す。
                    let since = body.pointer("/filterGroups/0/filters/0/value").and_then(Value::as_str).unwrap();
                    let total = match since {
                        "1789948800000" => 2914, // 2026-09-21
                        "1784592000000" => 7000, // 2026-07-21
                        "1776729600000" => 13454, // 2026-04-21
                        other => panic!("unexpected since {other}"),
                    };
                    json!({"total":total,"results":[]})
                } else if body["after"] == "p2" { search_page2() } else { search_page1() }
            } else if path == "/crm/v4/associations/0-421/0-420/batch/read" {
                let ids: Vec<String> = body["inputs"].as_array().unwrap().iter().map(|i| i["id"].as_str().unwrap().to_owned()).collect();
                let to = |id: &str| -> Vec<&str> { match id { "1" => vec!["L1"], "2" => vec!["L2","L3"], "5" => vec!["L4"], _ => vec![] } };
                let (with, without): (Vec<_>, Vec<_>) = ids.iter().partition(|id| !to(id).is_empty());
                json!({"results":with.iter().map(|id| json!({"from":{"id":id},"to":to(id).iter().map(|l| json!({"toObjectId":l})).collect::<Vec<_>>()})).collect::<Vec<_>>(),
                    "errors":[{"status":"error","category":"OBJECT_NOT_FOUND","subCategory":NONE,"context":{"fromObjectId":without}}]})
            } else if path == "/crm/v3/properties/0-421/batch/read" {
                let options = json!([
                    {"value":"勤務地／通勤（車通勤可等）","label":"勤務地／通勤（車通勤可等）","displayOrder":3},
                    {"value":"給与／手当","label":"給与／手当","displayOrder":0},
                    {"value":"休日／休暇","label":"休日／休暇","displayOrder":1},
                    {"value":"仕事内容","label":"仕事内容","displayOrder":4},
                    {"value":"その他（詳細）","label":"その他（詳細）","displayOrder":11}]);
                let results: Vec<Value> = SELECT_PROPERTIES.iter().map(|n| json!({"name":n,"options":options})).collect();
                json!({"results":results})
            } else if path == "/crm/v3/objects/0-420/batch/read" {
                json!({"results":[
                    {"id":"L1","properties":{"todoufuken":"神奈川県","shikuchouson":"平塚市","zhizhong":"倉庫管理"}},
                    {"id":"L2","properties":{"todoufuken":"大阪府","shikuchouson":"大阪市北区","zhizhong":"一般事務"}},
                    {"id":"L3","properties":{"todoufuken":"大阪府","shikuchouson":"堺市堺区","zhizhong":"配送ドライバー"}},
                    {"id":"L4","properties":{"todoufuken":"東京都","shikuchouson":"千代田区","zhizhong":"一般事務"}}]})
            } else {
                panic!("unexpected {path}");
            };
            Json(value)
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    (service, server)
}
async fn ready(service: &JobReadService) -> Value {
    tokio::time::timeout(Duration::from_secs(10), async {
        loop {
            let v = answer(service, today()).await;
            if v["status"] != "preparing" {
                return v;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .unwrap()
}
fn row<'a>(data: &'a Value, date: &str, index: usize) -> &'a Value {
    data["report"]["rows"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|r| r["date"] == date)
        .nth(index)
        .unwrap_or_else(|| panic!("no row {date}"))
}

#[tokio::test]
async fn builds_crosstab_from_select_fields_and_caches_it() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let (service, server) = serve(log.clone()).await;
    assert_eq!(answer(&service, today()).await["status"], "preparing");
    let data = ready(&service).await;
    assert_eq!(data["status"], "ready");
    assert_eq!(data["stale"], false);
    let report = &data["report"];
    assert_eq!(report["today"], "2026-10-21");
    // 列は応募動機の選択肢を displayOrder の順に。
    assert_eq!(
        report["options"],
        json!([
            "給与／手当",
            "休日／休暇",
            "勤務地／通勤（車通勤可等）",
            "仕事内容",
            "その他（詳細）"
        ])
    );
    let got: Vec<_> = report["periods"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["months"].as_u64().unwrap(),
                p["since"].as_str().unwrap(),
                p["total"].as_u64().unwrap(),
                p["with_reason"].as_u64().unwrap(),
            )
        })
        .collect();
    // 全応募の数は Search の total、選んだ値のある応募はこちらで数える。
    assert_eq!(
        got,
        vec![
            (1, "2026-09-21", 2914, 4),
            (3, "2026-07-21", 7000, 5),
            (6, "2026-04-21", 13454, 6)
        ]
    );
    // 選択式の値がある 6 か月以内の応募だけ。文章だけ (4) と範囲外 (3) は入らない。
    let dates: Vec<_> = report["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| r["date"].as_str().unwrap())
        .collect();
    assert_eq!(
        dates,
        vec![
            "2026-10-20",
            "2026-10-20",
            "2026-10-15",
            "2026-10-10",
            "2026-09-10",
            "2026-04-21"
        ]
    );
    // 応募1: 応募動機の複数選択、文章は詳細として添える。関連の求人票の勤務地と職種。
    let r1 = row(&data, "2026-10-20", 0);
    assert_eq!(
        r1["selections"],
        json!([{"source_property":"oubodouki_1","label":"勤務地／通勤（車通勤可等）"},
               {"source_property":"oubodouki_1","label":"給与／手当"}])
    );
    assert_eq!(
        r1["texts"],
        json!([{"source_property":"oubodouki","text":"家から近い"}])
    );
    assert_eq!(
        r1["applicant_area"],
        json!({"prefecture":"神奈川県","municipality":"横浜市"})
    );
    assert_eq!(
        r1["job_area"],
        json!({"prefecture":"神奈川県","municipality":"平塚市"})
    );
    assert_eq!(r1["occupation"], "物流");
    // 応募5: 作成日の JST の日付、電話番号は伏せる。
    let r5 = row(&data, "2026-10-20", 1);
    assert_eq!(
        r5["selections"],
        json!([{"source_property":"oubodouki_1","label":"その他（詳細）"}])
    );
    let text = r5["texts"][0]["text"].as_str().unwrap();
    assert!(text.contains("＊＊") && !text.contains("090"));
    assert_eq!(r5["occupation"], "事務");
    // 応募2: 優先順位①と③（空の②は無い）。求人票が 2 件で食い違い。
    let r2 = row(&data, "2026-04-21", 0);
    assert_eq!(
        r2["selections"],
        json!([{"source_property":"kinmujoukennoyuusenjuni_1","label":"給与／手当"},
               {"source_property":"kinmujoukennoyuusenjuni_3","label":"休日／休暇"}])
    );
    assert_eq!(r2["job_area"], Value::Null);
    assert_eq!(r2["occupation_status"], "conflict");
    // 関連の無い応募: 求人票なし、応募の職種で区分。
    let r6 = row(&data, "2026-09-10", 0);
    assert_eq!(
        (r6["occupation"].as_str(), r6["job_area"].clone()),
        (Some("営業"), Value::Null)
    );
    assert_eq!(
        row(&data, "2026-10-15", 0)["occupation_status"],
        "unmatched"
    );
    // 選択肢に無い値はそのまま、文章は 2000 字まで。
    let r8 = row(&data, "2026-10-10", 0);
    assert_eq!(r8["selections"][0]["label"], "名前の無い値");
    assert_eq!(
        r8["texts"][0]["text"].as_str().unwrap().chars().count(),
        2000
    );
    // 呼び出し: 項目定義1 + 理由の Search 2 + 件数の Search 3 + 関連1 + 求人票1。
    {
        let calls = log.lock().unwrap();
        assert_eq!(calls.len(), 8, "{calls:?}");
        let first = calls.iter().find(|c| c.contains("HAS_PROPERTY")).unwrap();
        assert!(
            first.contains("oubodouki_1")
                && first.contains("hs_createdate")
                && first.contains("kinmujoukennoyuusenjuni_3")
                && first.contains("\"limit\":200")
        );
        assert!(!calls
            .iter()
            .any(|c| c.starts_with("/crm/v3/objects/0-421 ")));
        assert!(
            calls
                .iter()
                .filter(|c| c.contains("NOT_HAS_PROPERTY"))
                .count()
                == 3
        );
    }
    // 2回目は HubSpot を呼ばず同じ内容を返す。
    assert_eq!(answer(&service, today()).await, data);
    assert_eq!(log.lock().unwrap().len(), 8);
    server.abort();
}

#[test]
fn links_fail_on_errors_other_than_no_associations() {
    assert_eq!(
        no_association_ids(&json!({"subCategory":NONE,"context":{"fromObjectId":["1",2]}})),
        Some(vec!["1".to_owned(), "2".to_owned()])
    );
    assert_eq!(
        no_association_ids(&json!({"subCategory":"crm.other","context":{"fromObjectId":["1"]}})),
        None
    );
}

#[tokio::test]
async fn failure_with_no_previous_result_is_failed_then_waits_before_retry() {
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = hits.clone();
    let app = Router::new().fallback(any(move || {
        let counter = counter.clone();
        async move {
            counter.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
            StatusCode::FORBIDDEN
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert_eq!(ready(&service).await["status"], "failed");
    let before = hits.load(std::sync::atomic::Ordering::SeqCst);
    assert!(before > 0);
    assert_eq!(answer(&service, today()).await["status"], "failed");
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), before);
    server.abort();
}

#[test]
fn since_is_same_day_n_months_ago_clamped_to_month_end() {
    assert_eq!(since_for(day("2026-10-21"), 6), day("2026-04-21"));
    assert_eq!(since_for(day("2026-03-31"), 1), day("2026-02-28"));
    assert_eq!(since_for(day("2026-01-15"), 3), day("2025-10-15"));
}

#[test]
fn wards_of_designated_cities_roll_up_but_tokyo_wards_stay() {
    let a = area(Some("神奈川県"), Some("横浜市港北区"));
    assert_eq!(a.municipality.as_deref(), Some("横浜市"));
    let a = area(Some("東京都"), Some("千代田区"));
    assert_eq!(a.municipality.as_deref(), Some("千代田区"));
    let a = area(Some("大阪府"), Some("堺市堺区"));
    assert_eq!(a.municipality.as_deref(), Some("堺市"));
}

#[test]
fn occupation_table_maps_by_order_and_unmatched_is_none() {
    assert_eq!(classify("倉庫管理"), Some("物流"));
    assert_eq!(classify("その他営業"), Some("営業"));
    assert_eq!(classify("一般事務"), Some("事務"));
    assert_eq!(classify("軽作業スタッフ"), Some("製造"));
    assert_eq!(classify("生産管理"), Some("エンジニア"));
    assert_eq!(classify("運行管理"), Some("物流"));
    assert_eq!(classify("ホテルフロント"), Some("サービス"));
    assert_eq!(classify("重機オペレーター"), Some("技能職"));
    assert_eq!(classify("看護師"), Some("医療"));
    assert_eq!(classify("宇宙飛行士"), None);
    assert_eq!(classify("その他"), None);
}

/// HubSpot の Search の条件を、テスト用の応募 (応募日・作成日、ミリ秒) に当てる。
fn matches(body: &Value, yingmuri: Option<i64>, created: i64) -> bool {
    body["filterGroups"]
        .as_array()
        .unwrap()
        .iter()
        .any(|group| {
            group["filters"].as_array().unwrap().iter().all(|f| {
                let value = match f["propertyName"].as_str().unwrap() {
                    "yingmuri" => yingmuri,
                    "hs_createdate" => Some(created),
                    other => panic!("unexpected {other}"),
                };
                let limit = || f["value"].as_str().unwrap().parse::<i64>().unwrap();
                match f["operator"].as_str().unwrap() {
                    "NOT_HAS_PROPERTY" => value.is_none(),
                    "GTE" => value.is_some_and(|v| v >= limit()),
                    "LT" => value.is_some_and(|v| v < limit()),
                    other => panic!("unexpected {other}"),
                }
            })
        })
}
fn utc(s: &str) -> i64 {
    DateTime::parse_from_rfc3339(s).unwrap().timestamp_millis()
}

#[test]
fn count_uses_application_day_then_created_day_in_jst() {
    let body = count_body(day("2026-09-21"), today());
    let applied = |d: &str| Some(day_ms(day(d)));
    // 応募日があれば作成日によらず応募日で決める（後から作られた古い応募は数えない）。
    assert!(matches(
        &body,
        applied("2026-09-21"),
        utc("2026-11-01T00:00:00Z")
    ));
    assert!(matches(
        &body,
        applied("2026-10-21"),
        utc("2026-10-21T00:00:00Z")
    ));
    assert!(!matches(
        &body,
        applied("2026-09-20"),
        utc("2026-10-01T00:00:00Z")
    ));
    assert!(!matches(
        &body,
        applied("2026-10-22"),
        utc("2026-10-22T00:00:00Z")
    ));
    // 応募日が無ければ作成日の JST の日付: 09-21 0:00 JST = 09-20 15:00 UTC から 10-22 0:00 JST の前まで。
    assert!(!matches(&body, None, utc("2026-09-20T14:59:59Z")));
    assert!(matches(&body, None, utc("2026-09-20T15:00:00Z")));
    assert!(matches(&body, None, utc("2026-10-21T14:59:59Z")));
    assert!(!matches(&body, None, utc("2026-10-21T15:00:00Z")));
}

#[test]
fn reason_search_reaches_every_application_of_the_period_within_limits() {
    let since = day("2026-04-21");
    let created_since = jst_ms(since);
    // 応募日が起点ちょうどで、作成日が JST の起点 0 時ちょうどの応募も検索に入る。
    assert_eq!(created_since, utc("2026-04-20T15:00:00Z"));
    // filterGroups は HubSpot の上限 (5 まとまり・合計 18 条件) に収まる。
    assert!(SELECT_PROPERTIES.len() <= 5 && SELECT_PROPERTIES.len() * 2 <= 18);
}

#[tokio::test]
async fn selection_options_are_shared_and_cached_between_refreshes() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let seen = calls.clone();
    let app = Router::new().fallback(any(move || {
        let seen = seen.clone();
        async move {
            seen.fetch_add(1, Ordering::SeqCst);
            Json(
                json!({"results":SELECT_PROPERTIES.map(|p| json!({"name":p,"options":[
                    {"value":"給与","label":"給与","displayOrder":2},
                    {"value":"休日","label":"休日","displayOrder":1}
                ]}))}),
            )
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert_eq!(
        read_options(&service).await.unwrap().order,
        ["休日", "給与"]
    );
    assert_eq!(
        read_options(&service.clone()).await.unwrap().order,
        ["休日", "給与"]
    );
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    server.abort();
}
