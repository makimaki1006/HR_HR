use super::*;
use axum::{body::Body, http::Request, routing::any};
use std::sync::Mutex;

fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn today() -> NaiveDate {
    day("2026-10-21")
}
fn app(id: &str, props: Value, listings: &[&str]) -> Value {
    json!({"id":id,"properties":props,
        "associations":{"listings":{"results":listings.iter().map(|l|json!({"id":l,"type":"x"})).collect::<Vec<_>>()}}})
}
fn page1() -> Value {
    json!({"results":[
        app("1",json!({"yingmuri":"2026-10-20","todoufuken":"神奈川県","shikuchouson":"横浜市港北区","oubodouki":"家から近い"}),&["L1"]),
        app("2",json!({"yingmuri":"2026-04-21","todoufuken":"大阪府","shikuchouson":"大阪市北区",
            "ouboriyuukategori_hiaringu":"unset","ouboriyuukategori_baitaikisai":"kyuuyo"}),&["L2","L3","L2"]),
        app("3",json!({"yingmuri":"2026-04-20","oubodouki":"範囲外"}),&[]),
        app("4",json!({"yingmuri":"2026-10-01","genshokumaeshokukaranotenshokuriyuu":"転職理由","kyuujinkensakujouken":"探し方","ouboriyuukategori_hiaringu":"unset"}),&[]),
    ],"paging":{"next":{"after":"p2"}}})
}
fn page2() -> Value {
    json!({"results":[
        app("5",json!({"yingmuri":"","hs_createdate":"2026-10-19T16:00:00Z","todoufuken":"東京都","shikuchouson":"千代田区","honnin_kibou_kinyuuran":"090-1234-5678に連絡してください"}),&["L4"]),
        app("6",json!({"yingmuri":"2026-09-10","kinmujoukennoyuusenjuni":"休みが多い","oubosaki_shokushu":"その他営業"}),&[]),
        app("7",json!({"yingmuri":"2026-10-15","oubodouki":"理由","oubosaki_shokushu":"宇宙飛行士"}),&[]),
        app("8",json!({"yingmuri":"2026-10-10","oubodouki":"あ".repeat(2500)}),&[]),
    ]})
}
async fn serve(log: Arc<Mutex<Vec<String>>>) -> (JobReadService, tokio::task::JoinHandle<()>) {
    let app = Router::new().fallback(any(move |req: Request<Body>| {
        let log = log.clone();
        async move {
            let path = req.uri().path().to_owned();
            let query = req.uri().query().unwrap_or("").to_owned();
            log.lock().unwrap().push(format!("{} {path}?{query}", req.method()));
            let value = if path == "/crm/v3/objects/0-421" {
                if query.contains("after=p2") { page2() } else { page1() }
            } else if path == "/crm/v3/properties/0-421/batch/read" {
                json!({"results":[
                    {"name":"ouboriyuukategori_hiaringu","options":[{"value":"unset","label":"未設定"},{"value":"kyuuyo","label":"給与"}]},
                    {"name":"ouboriyuukategori_baitaikisai","options":[{"value":"unset","label":"未設定"},{"value":"kyuuyo","label":"給与"}]}]})
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
async fn builds_crosstab_from_two_pages_and_caches_it() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let (service, server) = serve(log.clone()).await;
    assert_eq!(answer(&service, today()).await["status"], "preparing");
    let data = ready(&service).await;
    assert_eq!(data["status"], "ready");
    assert_eq!(data["stale"], false);
    let report = &data["report"];
    assert_eq!(report["today"], "2026-10-21");
    let periods = report["periods"].as_array().unwrap();
    let got: Vec<_> = periods
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
    assert_eq!(
        got,
        vec![
            (1, "2026-09-21", 5, 4),
            (3, "2026-07-21", 6, 5),
            (6, "2026-04-21", 7, 6)
        ]
    );
    assert_eq!(report["occupation_groups"].as_array().unwrap().len(), 11);
    // 6か月以内の理由あり6件、日付の新しい順（2026-04-20 の応募3は範囲外）。
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
    // 応募1: 政令市の区は市にまとめる。求人側は平塚市。倉庫管理は物流。
    let r1 = row(&data, "2026-10-20", 0);
    assert_eq!(
        r1["applicant_area"],
        json!({"prefecture":"神奈川県","municipality":"横浜市"})
    );
    assert_eq!(
        r1["job_area"],
        json!({"prefecture":"神奈川県","municipality":"平塚市"})
    );
    assert_eq!(
        (r1["occupation"].as_str(), r1["occupation_status"].as_str()),
        (Some("物流"), Some("grouped"))
    );
    assert_eq!(
        r1["texts"],
        json!([{"source_property":"oubodouki","text":"家から近い"}])
    );
    assert_eq!(r1["selections"], json!([]));
    // 応募5: JST の日付、東京の区はそのまま、電話番号は伏せる、事務。
    let r5 = row(&data, "2026-10-20", 1);
    assert_eq!(
        r5["applicant_area"],
        json!({"prefecture":"東京都","municipality":"千代田区"})
    );
    assert_eq!(r5["job_area"]["municipality"], "千代田区");
    assert_eq!(r5["occupation"], "事務");
    let text = r5["texts"][0]["text"].as_str().unwrap();
    assert!(text.contains("＊＊") && !text.contains("090"));
    // 応募2: 求人票が2件で食い違い。区分は未設定を除いて「給与」だけ。
    let r2 = row(&data, "2026-04-21", 0);
    assert_eq!(r2["job_area"], Value::Null);
    assert_eq!(
        (r2["occupation"].as_str(), r2["occupation_status"].as_str()),
        (None, Some("conflict"))
    );
    assert_eq!(
        r2["selections"],
        json!([{"source_property":"ouboriyuukategori_baitaikisai","label":"給与"}])
    );
    assert_eq!(
        r2["applicant_area"],
        json!({"prefecture":"大阪府","municipality":"大阪市"})
    );
    // 求人票が無い応募: 「その他営業」→営業、当たらない→unmatched、職種なし→missing。
    let r6 = row(&data, "2026-09-10", 0);
    assert_eq!(
        (r6["occupation"].as_str(), r6["occupation_status"].as_str()),
        (Some("営業"), Some("grouped"))
    );
    assert_eq!(r6["job_area"], Value::Null);
    assert_eq!(
        row(&data, "2026-10-15", 0)["occupation_status"],
        "unmatched"
    );
    let r8 = row(&data, "2026-10-10", 0);
    assert_eq!(r8["occupation_status"], "missing");
    assert_eq!(
        r8["texts"][0]["text"].as_str().unwrap().chars().count(),
        2000
    );
    // 転職理由・探し方は理由に入らない（応募4は rows に無い）。
    assert!(!dates.contains(&"2026-10-01"));
    // 呼び出し: 項目定義1 + 応募2ページ + 求人票1回。
    {
        let calls = log.lock().unwrap();
        assert_eq!(calls.len(), 4, "{calls:?}");
        let first = calls
            .iter()
            .find(|c| c.starts_with("GET /crm/v3/objects/0-421?"))
            .unwrap();
        assert!(
            first.contains("limit=100")
                && first.contains("associations=0-420")
                && first.contains("oubodouki")
        );
        assert!(calls.iter().any(|c| c.contains("after=p2")));
    }
    // 2回目は HubSpot を呼ばず同じ内容を返す。
    assert_eq!(answer(&service, today()).await, data);
    assert_eq!(log.lock().unwrap().len(), 4);
    server.abort();
}

#[tokio::test]
async fn failure_with_no_previous_result_is_failed_then_waits_before_retry() {
    use axum::routing::get;
    let hits = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counter = hits.clone();
    let app = Router::new().fallback(get(move || {
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
