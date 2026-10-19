//! 架空 HubSpot だけを使う、読み取り契約のテスト。
use super::*;
use axum::body::to_bytes;
#[derive(Default)]
struct Fake {
    requests: Vec<(String, Value)>,
    forbidden: bool,
    sheet: Option<String>,
    large_queue: bool,
}
fn props() -> Value {
    json!({"results":[
        {"name":"ouboshashimei","label":"応募者氏名","type":"string"},
        {"name":"ouboshashimei_kana","label":"カナ","type":"string"},
        {"name":"denwabangou","label":"電話番号","type":"string"},
        {"name":"yingmuri","label":"応募日","type":"date"},
        {"name":"hs_pipeline_stage","label":"ステージ","type":"enumeration"},
        {"name":"oubosaki_kyuujin_title","label":"応募先求人タイトル","type":"string"},
        {"name":"oubobaitaimei","label":"応募媒体","type":"enumeration","options":[{"value":"web","label":"架空求人サイト"}]},
        {"name":"customer_sheet_url_synced","label":"報告シート","type":"string"},
        {"name":"anmokuchi_shodoutaiou","label":"初動対応","type":"string"},
        {"name":"anmokuchi_ashigirijouken","label":"足切り条件","type":"string"},
        {"name":"hoyuushikaku_hiaringu","label":"保有資格","type":"enumeration","options":[{"value":"none","label":"資格なし"}]}
    ]})
}
fn applicant(id: &str, date: &str) -> Value {
    json!({"id":id,"properties":{"ouboshashimei":"架空 はるか","ouboshashimei_kana":"カクウ ハルカ","denwabangou":"00000000000","yingmuri":date,"hs_createdate":"2026-10-16T01:00:00Z","hs_pipeline_stage":"1343099551","oubobaitaimei":"web","oubosaki_kyuujin_title":"架空の検品求人","hoyuushikaku_hiaringu":"none","anmokuchi_shodoutaiou":"希望時間を確認","anmokuchi_ashigirijouken":"  "}})
}
async fn fake(State(st): State<Shared<Fake>>, req: Request<Body>) -> Response {
    let path = req.uri().path().to_string();
    let method = req.method().clone();
    let body = to_bytes(req.into_body(), 1024 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let mut f = st.lock().unwrap();
    assert!(
        method == "GET"
            || (method == "POST" && (path.ends_with("/batch/read") || path.ends_with("/search"))),
        "書き込みは認めない: {method} {path}"
    );
    f.requests.push((path.clone(), body.clone()));
    if f.forbidden {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = match path.as_str() {
        "/crm/v3/properties/0-421" => props(),
        "/crm/v3/pipelines/0-421" => {
            json!({"results":[{"id":"pipeline-from-hubspot","label":"一次対応_BPO","stages":[{"id":"1343099551","label":"応募受付"},{"id":"1431574804","label":"朝連絡"}]}]})
        }
        "/crm/v3/objects/0-421/search" => {
            let sort = &body["sorts"][0]["propertyName"];
            if f.large_queue {
                let kind = body["sorts"][0]["propertyName"].as_str().unwrap();
                let offset = body
                    .get("after")
                    .map_or(0, |a| a.as_str().unwrap().parse::<usize>().unwrap());
                let total = if body["limit"] == 1 {
                    62
                } else if kind == "yakusokukadennichiji" {
                    0
                } else {
                    31
                };
                let mut rows = Vec::new();
                if body["limit"] != 1 && total > 0 {
                    for i in offset..(offset + 30).min(31) {
                        let dated = kind == "yingmuri";
                        let day = chrono::NaiveDate::from_ymd_opt(2026, 10, 31).unwrap()
                            - chrono::Duration::days((i * 2 + usize::from(!dated)) as i64);
                        let mut a = applicant(
                            &(if dated { 1000 + i } else { 2000 + i }).to_string(),
                            if dated { "temporary" } else { "" },
                        );
                        if dated {
                            a["properties"]["yingmuri"] = json!(day.to_string());
                        } else {
                            a["properties"]["hs_createdate"] = json!(day.to_string());
                        }
                        rows.push(a);
                    }
                }
                let mut result = json!({"results":rows,"total":total});
                if offset + 30 < total && body["limit"] != 1 {
                    result["paging"] = json!({"next":{"after":(offset+30).to_string()}});
                }
                return Json(result).into_response();
            }
            if body["limit"] == 1 {
                json!({"total":3,"results":[]})
            } else if sort == "yakusokukadennichiji" {
                json!({"total":1,"results":[applicant("100","2026-10-15")]})
            } else if sort == "yingmuri" {
                json!({"total":1,"results":[applicant("101","2026-10-17")]})
            } else {
                json!({"total":1,"results":[applicant("102","")]})
            }
        }
        "/crm/v3/objects/0-421/batch/read" => {
            let mut a = applicant("100", "2026-10-15");
            a["properties"]["customer_sheet_url_synced"] = json!(f.sheet);
            json!({"results":[a]})
        }
        "/crm/v4/objects/0-421/100/associations/0-420" => json!({"results":[{"toObjectId":200}]}),
        "/crm/v4/objects/0-421/100/associations/notes" => {
            json!({"results":(300..307).map(|i|json!({"toObjectId":i})).collect::<Vec<_>>()})
        }
        "/crm/v3/objects/0-420/batch/read" => {
            json!({"results":[{"id":"200","properties":{"hs_name":"架空求人票","customer_sheet_url":"https://docs.google.com/spreadsheets/d/fake-job/edit","url_hrhakkaa":"https://example.invalid/hrh/job/1","url_airwork":null}}]})
        }
        "/crm/v3/objects/notes/batch/read" => {
            json!({"results":(1..=7).map(|i|json!({"id":(299+i).to_string(),"properties":{"hs_timestamp":format!("2026-10-{:02}T00:00:00Z", i+9),"hs_note_body":format!("架空のメモ {i}")}})).collect::<Vec<_>>()})
        }
        _ => panic!("想定外: {path}"),
    };
    Json(result).into_response()
}
async fn app() -> (Router, String, Shared<Fake>) {
    let f = Arc::new(Mutex::new(Fake::default()));
    let base = spawn(Router::new().fallback(fake).with_state(f.clone())).await;
    let client = HubSpotClient::new(
        "fake-key".into(),
        &base,
        ClientOptions {
            timeout: Duration::from_secs(2),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
            search_min_interval: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(1),
        },
    )
    .unwrap();
    let app = crm_app(test_state(None, Some(Arc::new(client))));
    let cookie = login_as(&app, "google_oidc", None).await;
    (app, cookie, f)
}
#[tokio::test]
async fn applicants_queue_search_phases_cursor_total_cache_and_fresh() {
    let (app, c, f) = app().await;
    let url = "/api/crm/applicants/queue?media=web";
    let (status, cc, a) = get_json(&app, url, &c).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(cc, "no-store");
    assert_eq!(a["total"], 3);
    assert_eq!(a["items"][0]["name"], "架空 はるか");
    assert_eq!(a["items"][0]["job"], "架空の検品求人");
    assert_eq!(a["items"][0]["media"], "架空求人サイト");
    assert_eq!(a["pipelines"][0]["label"], "一次対応_BPO");
    {
        let g = f.lock().unwrap();
        let searches: Vec<_> = g
            .requests
            .iter()
            .filter(|(p, _)| p.ends_with("search"))
            .collect();
        assert_eq!(searches.len(), 2);
        assert!(searches.iter().all(|(_, b)| b.get("after").is_none()));
        let body = &searches[1].1;
        assert_eq!(
            body["sorts"],
            json!([{"propertyName":"yakusokukadennichiji","direction":"ASCENDING"}])
        );
        assert_eq!(body["filterGroups"][0]["filters"][0]["operator"], "IN");
        assert_eq!(
            body["filterGroups"][0]["filters"][0]["values"]
                .as_array()
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            body["filterGroups"][0]["filters"][1],
            json!({"propertyName":"ichijitaiounoumu","operator":"EQ","value":"必要"})
        );
        assert_eq!(body["filterGroups"][0]["filters"][2]["value"], "web");
        assert_eq!(body["filterGroups"][0]["filters"][3]["operator"], "LTE");
    }
    let n = f.lock().unwrap().requests.len();
    assert_eq!(get_json(&app, url, &c).await.2, a);
    assert_eq!(f.lock().unwrap().requests.len(), n);
    assert_eq!(
        get_json(&app, &format!("{url}&fresh=1"), &c).await.0,
        StatusCode::OK
    );
    assert_eq!(f.lock().unwrap().requests.len(), n + 2);
    let cursor = a["next_cursor"].as_str().unwrap();
    let (status, _, b) = get_json(&app, &format!("{url}&cursor={cursor}"), &c).await;
    assert_eq!(status, StatusCode::OK, "{b}");
    assert_eq!(b["items"][0]["id"], "101");
    assert_eq!(b["items"][1]["id"], "102");
    assert_eq!(b["next_cursor"], Value::Null);
    assert_eq!(b["items"][1]["date"], "2026-10-16T01:00:00Z");
    {
        let g = f.lock().unwrap();
        let dated = &g.requests[g.requests.len() - 2].1;
        let missing = &g.requests[g.requests.len() - 1].1;
        assert_eq!(dated["sorts"][0]["propertyName"], "yingmuri");
        assert_eq!(missing["sorts"][0]["propertyName"], "hs_createdate");
        assert_eq!(
            missing["filterGroups"][1]["filters"][3]["operator"],
            "NOT_HAS_PROPERTY"
        );
    }
    assert_eq!(
        get_json(
            &app,
            &format!("/api/crm/applicants/queue?cursor={cursor}"),
            &c
        )
        .await
        .0,
        StatusCode::BAD_REQUEST
    );
}
#[tokio::test]
async fn applicants_detail_labels_empty_fields_sheet_notes_cache_and_fresh() {
    let (app, c, f) = app().await;
    let (status, _, a) = get_json(&app, "/api/crm/applicants/100", &c).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["phone"], "00000000000");
    assert_eq!(
        a["sections"]
            .as_array()
            .unwrap()
            .iter()
            .map(|s| s["title"].as_str().unwrap())
            .collect::<Vec<_>>(),
        [
            "応募者の情報",
            "この求人の対応ルール",
            "応募先",
            "応募の内容",
            "ヒアリングと結果"
        ]
    );
    assert_eq!(
        a["sections"][2]["fields"],
        json!([{ "label": "応募先求人タイトル", "value": "架空の検品求人" }])
    );
    assert_eq!(
        a["sheet_url"],
        "https://docs.google.com/spreadsheets/d/fake-job/edit"
    );
    assert_eq!(a["sheet_embed"], true);
    assert_eq!(
        a["sections"][1]["fields"],
        json!([{"label":"初動対応","value":"希望時間を確認"}])
    );
    assert_eq!(
        a["sections"][4]["fields"],
        json!([{"label":"保有資格","value":"資格なし"}])
    );
    assert_eq!(a["jobs"][0]["label"], "架空求人票");
    assert!(a["jobs"][0]["url"]
        .as_str()
        .unwrap()
        .ends_with("/record/0-420/200/"));
    assert_eq!(
        a["job_pages"],
        json!([{"label":"HRハッカー","url":"https://example.invalid/hrh/job/1"}])
    );
    assert_eq!(a["notes"][0]["date"], "2026-10-16T00:00:00Z");
    assert_eq!(a["notes"].as_array().unwrap().len(), 5);
    assert_eq!(a["notes"][4]["date"], "2026-10-12T00:00:00Z");
    let n = f.lock().unwrap().requests.len();
    assert_eq!(n, 7); // 定義2、本体1、関連2、求人票1、メモ1
    assert_eq!(get_json(&app, "/api/crm/applicants/100", &c).await.2, a);
    assert_eq!(f.lock().unwrap().requests.len(), n);
    f.lock().unwrap().sheet = Some("https://example.invalid/sheet".into());
    let (_, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &c).await;
    assert_eq!(b["sheet_url"], "https://example.invalid/sheet");
    assert_eq!(b["sheet_embed"], false);
    assert_eq!(f.lock().unwrap().requests.len(), n + 5);
    f.lock().unwrap().sheet = Some("https://docs.google.com/spreadsheets/d/fake-app/edit".into());
    let (_, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &c).await;
    assert_eq!(
        b["sheet_url"],
        "https://docs.google.com/spreadsheets/d/fake-app/edit"
    );
    assert_eq!(b["sheet_embed"], true);
}
#[tokio::test]
async fn applicants_authorization_invalid_id_and_scope_error() {
    let (app, c, f) = app().await;
    assert_eq!(
        get_req(&app, "/api/crm/applicants/queue", None)
            .await
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_json(&app, "/api/crm/applicants/nope", &c).await.0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        get_json(&app, "/api/crm/applicants/queue?stages=1431589175", &c)
            .await
            .0,
        StatusCode::BAD_REQUEST
    );
    assert!(f.lock().unwrap().requests.is_empty());
    f.lock().unwrap().forbidden = true;
    for path in ["/api/crm/applicants/queue", "/api/crm/applicants/100"] {
        let (status, _, b) = get_json(&app, path, &c).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(b["message"], "HubSpot の鍵に応募を読む権限がありません");
    }
}

#[tokio::test]
async fn applicants_merge_keeps_order_across_pages_without_duplicates() {
    let (app, c, f) = app().await;
    f.lock().unwrap().large_queue = true;
    let (_, _, mut page) = get_json(&app, "/api/crm/applicants/queue", &c).await;
    assert_eq!(page["total"], 62);
    assert_eq!(
        page["items"][0]["id"], "1000",
        "約束のない応募も先頭要求から表示する"
    );
    let mut ids = Vec::new();
    for _ in 0..5 {
        ids.extend(
            page["items"]
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["id"].as_str().unwrap().to_string()),
        );
        let Some(cursor) = page["next_cursor"].as_str() else {
            break;
        };
        let (status, _, next) = get_json(
            &app,
            &format!("/api/crm/applicants/queue?cursor={cursor}"),
            &c,
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{next}");
        page = next;
    }
    let expected: Vec<_> = (0..31)
        .flat_map(|i| [(1000 + i).to_string(), (2000 + i).to_string()])
        .collect();
    assert_eq!(ids, expected);
    let g = f.lock().unwrap();
    let searches: Vec<_> = g
        .requests
        .iter()
        .filter(|(p, _)| p.ends_with("/search"))
        .map(|(_, b)| b)
        .collect();
    assert_eq!(
        searches
            .iter()
            .map(|b| (
                b["sorts"][0]["propertyName"].as_str().unwrap(),
                b.get("after").cloned()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("hs_createdate", None),
            ("yakusokukadennichiji", None),
            ("yingmuri", None),
            ("hs_createdate", None),
            ("hs_createdate", Some(json!("15"))),
            ("yingmuri", Some(json!("15"))),
            ("hs_createdate", Some(json!("15"))),
            ("hs_createdate", Some(json!("30"))),
            ("yingmuri", Some(json!("30"))),
            ("hs_createdate", Some(json!("30"))),
        ]
    );
    assert_eq!(page["next_cursor"], Value::Null);
}
