//! 架空 HubSpot だけを使う、読み取り契約のテスト。
use super::*;
use axum::body::to_bytes;
#[derive(Default)]
struct Fake {
    requests: Vec<(String, Value)>,
    forbidden: bool,
    sheet: Option<String>,
    deals: Vec<Value>,
    deal_forbidden: Option<String>,
    listing_ids: Vec<String>,
    listing_deals: Option<Value>,
    large_queue: bool,
    due_count: usize,
    delay_ms: u64,
    inflight: usize,
    max_inflight: usize,
}
fn props() -> Value {
    json!({"results":[
        {"name":"ouboshashimei","label":"応募者氏名","type":"string"},
        {"name":"ouboshashimei_kana","label":"カナ","type":"string"},
        {"name":"denwabangou","label":"電話番号","type":"string"},
        {"name":"yingmuri","label":"応募日","type":"date"},
        {"name":"email","label":"Email","type":"string","hubspotDefined":true},
        {"name":"hs_pipeline_stage","label":"Pipeline Stage","type":"enumeration","hubspotDefined":true},
        {"name":"oubosaki_kyuujin_title","label":"応募先求人タイトル","type":"string"},
        {"name":"oubobaitaimei","label":"応募媒体","type":"enumeration","options":[{"value":"web","label":"架空求人サイト"}]},
        {"name":"customer_sheet_url_synced","label":"報告シート","type":"string"},
        {"name":"anmokuchi_shodoutaiou","label":"初動対応","type":"string"},
        {"name":"anmokuchi_ashigirijouken","label":"足切り条件","type":"string"},
        {"name":"hoyuushikaku_hiaringu","label":"保有資格","type":"enumeration","options":[{"value":"none","label":"資格なし"}]}
    ]})
}
fn applicant(id: &str, date: &str) -> Value {
    json!({"id":id,"properties":{"ouboshashimei":"架空 はるか","ouboshashimei_kana":"カクウ ハルカ","denwabangou":"00000000000","email":"applicant@example.invalid","yingmuri":date,"hs_createdate":"2026-10-16T01:00:00Z","hs_pipeline_stage":"1343099551","oubobaitaimei":"web","oubosaki_kyuujin_title":"架空の検品求人","hoyuushikaku_hiaringu":"none","anmokuchi_shodoutaiou":"希望時間を確認","anmokuchi_ashigirijouken":"  "}})
}
async fn fake(State(st): State<Shared<Fake>>, req: Request<Body>) -> Response {
    let delay = {
        let mut f = st.lock().unwrap();
        f.inflight += 1;
        f.max_inflight = f.max_inflight.max(f.inflight);
        f.delay_ms
    };
    tokio::time::sleep(Duration::from_millis(delay)).await;
    st.lock().unwrap().inflight -= 1;
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
    if f.deal_forbidden.as_deref() == Some(&path) {
        return StatusCode::FORBIDDEN.into_response();
    }
    let result = match path.as_str() {
        "/crm/v3/properties/0-421" => props(),
        "/crm/v3/pipelines/0-421" => {
            json!({"results":[{"id":"t_3c8f8179ebedb62c890f1cf2c73ff9c8","label":"一次対応_BPO","stages":[{"id":"1343099551","label":"応募受付"},{"id":"1431574804","label":"朝連絡"}]}]})
        }
        "/crm/v3/objects/0-421/search" => {
            let sort = &body["sorts"][0]["propertyName"];
            if f.large_queue {
                let kind = body["sorts"][0]["propertyName"].as_str().unwrap();
                let offset = body
                    .get("after")
                    .map_or(0, |a| a.as_str().unwrap().parse::<usize>().unwrap());
                let total = if body["limit"] == 1 {
                    62 + f.due_count
                } else if kind == "yakusokukadennichiji" {
                    f.due_count
                } else {
                    31
                };
                let mut rows = Vec::new();
                if body["limit"] != 1 && total > 0 {
                    for i in offset..(offset + 30).min(total) {
                        if kind == "yakusokukadennichiji" {
                            let mut a = applicant(&(3000 + i).to_string(), "2026-10-01");
                            a["properties"]["yakusokukadennichiji"] =
                                json!(format!("2026-10-01T00:{i:02}:00Z"));
                            rows.push(a);
                            continue;
                        }
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
        "/crm/v4/objects/0-421/100/associations/0-420" => {
            let ids = if f.listing_ids.is_empty() {
                vec!["200".to_string()]
            } else {
                f.listing_ids.clone()
            };
            json!({"results":ids.iter().map(|id| json!({"toObjectId":id})).collect::<Vec<_>>()})
        }
        "/crm/v4/associations/0-420/deals/batch/read" => f.listing_deals.clone().unwrap_or_else(
            || json!({"results":[{"from":{"id":"200"},"to":[{"toObjectId":400}]}]}),
        ),
        "/crm/v3/objects/deals/batch/read" => json!({"results":f.deals}),
        "/crm/v4/objects/0-421/100/associations/notes" => {
            json!({"results":(300..307).map(|i|json!({"toObjectId":i})).collect::<Vec<_>>()})
        }
        "/crm/v3/objects/0-420/batch/read" => {
            json!({"results":[{"id":"200","properties":{"hs_name":"架空求人票","url_hrhakkaa":"https://www.example.com/fictional-posting","url_airwork":"https://www.example.com/fictional-airwork","jobcopy_app_url":"https://www.example.com/app/job-copy/fictional","customer_sheet_url":"https://docs.google.com/spreadsheets/d/fake-job/edit"}}]})
        }
        "/crm/v3/objects/notes/batch/read" => {
            json!({"results":(1..=7).map(|i|json!({"id":(299+i).to_string(),"properties":{"hs_timestamp":format!("2026-10-{:02}T00:00:00Z", i+9),"hs_note_body":format!("架空のメモ {i}")}})).collect::<Vec<_>>()})
        }
        _ => panic!("想定外: {path}"),
    };
    Json(result).into_response()
}
async fn app() -> (Router, String, Shared<Fake>) {
    let (app, cookie, fake, _) = app_with_client().await;
    (app, cookie, fake)
}
async fn app_with_client() -> (Router, String, Shared<Fake>, Arc<HubSpotClient>) {
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
    let client = Arc::new(client);
    let app = crm_app(test_state(None, Some(client.clone())));
    let cookie = login_as(&app, "google_oidc", None).await;
    (app, cookie, f, client)
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
        assert_eq!(searches.len(), 4);
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
    assert_eq!(f.lock().unwrap().requests.len(), n + 4);
    assert_eq!(a["items"].as_array().unwrap().len(), 3);
    assert_eq!(a["items"][1]["id"], "101");
    assert_eq!(a["items"][2]["id"], "102");
    assert_eq!(a["next_cursor"], Value::Null);
    assert_eq!(a["items"][2]["date"], "2026-10-16T01:00:00Z");
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
}
#[tokio::test]
async fn applicants_due_pages_fill_and_resume_without_duplicates() {
    for due_count in [3, 30, 31] {
        let (app, c, f) = app().await;
        {
            let mut fake = f.lock().unwrap();
            fake.large_queue = true;
            fake.due_count = due_count;
        }
        let url = "/api/crm/applicants/queue?media=web";
        let (status, _, first) = get_json(&app, url, &c).await;
        assert_eq!(status, StatusCode::OK, "{first}");
        assert_eq!(first["total"], 62 + due_count);
        assert_eq!(first["items"].as_array().unwrap().len(), 30);
        let n = f.lock().unwrap().requests.len();
        assert_eq!(n, if due_count < 30 { 6 } else { 4 });
        assert_eq!(get_json(&app, url, &c).await.2, first);
        assert_eq!(f.lock().unwrap().requests.len(), n);
        let cursor = first["next_cursor"].as_str().unwrap().to_owned();
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
        let mut page = first;
        let mut ids = Vec::new();
        for page_index in 0..4 {
            assert_eq!(page["total"], 62 + due_count);
            if page_index == 1 {
                let expected = match due_count {
                    3 => "2013",
                    30 => "1000",
                    _ => "3030",
                };
                assert_eq!(page["items"][0]["id"], expected);
            }
            ids.extend(
                page["items"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|r| r["id"].as_str().unwrap().to_owned()),
            );
            let Some(cursor) = page["next_cursor"].as_str() else {
                break;
            };
            let (status, _, next) = get_json(&app, &format!("{url}&cursor={cursor}"), &c).await;
            assert_eq!(status, StatusCode::OK, "{next}");
            page = next;
        }
        assert_eq!(page["next_cursor"], Value::Null);
        let expected: Vec<_> = (0..due_count)
            .map(|i| (3000 + i).to_string())
            .chain((0..31).flat_map(|i| [(1000 + i).to_string(), (2000 + i).to_string()]))
            .collect();
        assert_eq!(ids, expected, "due_count={due_count}");
    }
}

#[tokio::test]
async fn applicants_detail_labels_empty_fields_sheet_notes_cache_and_fresh() {
    let (app, c, f) = app().await;
    let (status, _, a) = get_json(&app, "/api/crm/applicants/100", &c).await;
    assert_eq!(status, StatusCode::OK, "{a}");
    assert_eq!(a["phone"], "00000000000");
    assert_eq!(a["row"]["phone"], "00000000000");
    assert_eq!(
        a["posting_url"],
        "https://www.example.com/fictional-posting"
    );
    assert_eq!(
        a["jobcopy_url"],
        "https://www.example.com/app/job-copy/fictional"
    );
    {
        let requests = &f.lock().unwrap().requests;
        let applicant = requests
            .iter()
            .find(|(p, _)| p == "/crm/v3/objects/0-421/batch/read")
            .unwrap();
        assert!(applicant.1["properties"]
            .as_array()
            .unwrap()
            .contains(&json!("oubosaki_kyuujin_url")));
        let job = requests
            .iter()
            .find(|(p, _)| p == "/crm/v3/objects/0-420/batch/read")
            .unwrap();
        for name in ["url_hrhakkaa", "url_airwork", "jobcopy_app_url"] {
            assert!(job.1["properties"]
                .as_array()
                .unwrap()
                .contains(&json!(name)));
        }
    }
    assert_eq!(
        a["sections"][0]["fields"],
        json!([{"label":"カナ", "value":"カクウ ハルカ"}, {"label":"メールアドレス", "value":"applicant@example.invalid"}])
    );
    let all_fields = a["sections"]
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|section| section["fields"].as_array().unwrap());
    assert!(!all_fields.clone().any(|f| [
        "Pipeline Stage",
        "ステージ",
        "電話番号",
        "応募媒体",
        "応募日"
    ]
    .contains(&f["label"].as_str().unwrap())));

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
    assert_eq!(a["notes"][0]["date"], "2026-10-16T00:00:00Z");
    assert_eq!(a["notes"].as_array().unwrap().len(), 5);
    assert_eq!(a["notes"][4]["date"], "2026-10-12T00:00:00Z");
    let n = f.lock().unwrap().requests.len();
    assert_eq!(n, 9); // 定義2、本体1、応募関連2、求人票1、メモ1、取引関連1、取引1
    assert_eq!(get_json(&app, "/api/crm/applicants/100", &c).await.2, a);
    assert_eq!(f.lock().unwrap().requests.len(), n);
    f.lock().unwrap().sheet = Some("https://example.invalid/sheet".into());
    let (_, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &c).await;
    assert_eq!(b["sheet_url"], "https://example.invalid/sheet");
    assert_eq!(b["sheet_embed"], false);
    assert_eq!(f.lock().unwrap().requests.len(), n + 7);
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

// r1 の直列スケジュールと同じ API を同じ偽サーバへ送る（計測の比較用だけ）。
async fn sequential_detail(client: &HubSpotClient) {
    let defs = client
        .read_raw(false, "/crm/v3/properties/0-421", &[], None)
        .await
        .unwrap()
        .body
        .unwrap();
    client
        .read_raw(false, "/crm/v3/pipelines/0-421", &[], None)
        .await
        .unwrap();
    let names: Vec<&str> = defs["results"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    client
        .batch_read("0-421", &["100".into()], &names)
        .await
        .unwrap();
    client
        .read_raw(
            false,
            "/crm/v4/objects/0-421/100/associations/0-420",
            &[("limit", "500".into())],
            None,
        )
        .await
        .unwrap();
    client
        .read_raw(
            false,
            "/crm/v4/objects/0-421/100/associations/notes",
            &[("limit", "500".into())],
            None,
        )
        .await
        .unwrap();
    client
        .batch_read("0-420", &["200".into()], &["hs_name", "customer_sheet_url"])
        .await
        .unwrap();
    client
        .batch_associations("0-420", "deals", &["200".into()])
        .await
        .unwrap();
    client
        .batch_read(
            "deals",
            &["400".into()],
            &[
                "customer_sheet_url",
                "dealstage",
                "dealname",
                "contract_start_date",
                "createdate",
            ],
        )
        .await
        .unwrap();
    client
        .batch_read(
            "notes",
            &(300..307).map(|id| id.to_string()).collect::<Vec<_>>(),
            &["hs_timestamp", "hs_note_body"],
        )
        .await
        .unwrap();
}
#[tokio::test]
async fn applicant_detail_parallel_latency_and_same_call_budget() {
    let (app, cookie, fake, client) = app_with_client().await;
    fake.lock().unwrap().delay_ms = 100;
    let before = std::time::Instant::now();
    sequential_detail(&client).await;
    let before_ms = before.elapsed().as_millis();
    let old_paths = {
        let mut f = fake.lock().unwrap();
        assert_eq!(f.max_inflight, 1);
        let paths = f
            .requests
            .iter()
            .map(|(p, _)| p.clone())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(f.requests.len(), 9);
        f.requests.clear();
        f.max_inflight = 0;
        paths
    };
    let after = std::time::Instant::now();
    let (status, _, body) = get_json(&app, "/api/crm/applicants/100", &cookie).await;
    let after_ms = after.elapsed().as_millis();
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["phone"], "00000000000");
    assert_eq!(body["jobs"][0]["label"], "架空求人票");
    assert_eq!(body["notes"][0]["date"], "2026-10-16T00:00:00Z");
    {
        let f = fake.lock().unwrap();
        assert_eq!(f.requests.len(), 9);
        assert_eq!(
            f.requests
                .iter()
                .map(|(p, _)| p.clone())
                .collect::<std::collections::BTreeSet<_>>(),
            old_paths
        );
        assert_eq!(f.max_inflight, 3, "本体と二つの関連を同時に取得する");
    }
    fake.lock().unwrap().requests.clear();
    let warm = std::time::Instant::now();
    assert_eq!(
        get_json(&app, "/api/crm/applicants/100?fresh=1", &cookie)
            .await
            .0,
        StatusCode::OK
    );
    let warm_ms = warm.elapsed().as_millis();
    assert_eq!(fake.lock().unwrap().requests.len(), 7);
    println!("APPLICANT_DETAIL_LATENCY delay_ms=100 before_cold_ms={before_ms} after_cold_ms={after_ms} after_warm_ms={warm_ms} calls=9/9/7 sequential_stages=9 parallel_stages=4 warm_parallel_stages=3");
}

fn sheet_deal(id: &str, stage: &str, name: &str, start: &str, created: &str, url: &str) -> Value {
    json!({"id":id,"properties":{"dealstage":stage,"dealname":name,"contract_start_date":start,"createdate":created,"customer_sheet_url":url}})
}
#[tokio::test]
async fn applicant_report_sheet_owner_priority_dates_and_fallback() {
    let (app, cookie, fake) = app().await;
    // batch応答は関連順の逆。2求人の取引をまとめ、重複は1度だけ読む。
    {
        let mut f = fake.lock().unwrap();
        f.sheet = Some("https://example.invalid/synced".into());
        f.listing_ids = vec!["200".into(), "201".into()];
        f.listing_deals = Some(json!({"results":[
            {"from":{"id":"200"},"to":[{"toObjectId":400},{"toObjectId":401}]},
            {"from":{"id":"201"},"to":[{"toObjectId":402},{"toObjectId":403},{"toObjectId":400}]}
        ]}));
    }
    let base = vec![
        sheet_deal(
            "403",
            "52016158",
            "架空の終了契約",
            "2029-01-01",
            "2029-01-01",
            "https://example.invalid/ended",
        ),
        sheet_deal(
            "402",
            "1049738304",
            "架空オプション",
            "2028-01-01",
            "2028-01-01",
            "https://example.invalid/option",
        ),
        sheet_deal(
            "401",
            "52016153",
            "架空主契約",
            "2026-02-01T12:00:00Z",
            "2026-02-02",
            "https://example.invalid/new-main",
        ),
        sheet_deal(
            "400",
            "52016153",
            "架空主契約",
            "2026-01-01",
            "2027-01-01",
            "https://example.invalid/old-main",
        ),
    ];
    let mut scenarios = vec![(base.clone(), "https://example.invalid/new-main")];
    let mut same_start = base.clone();
    same_start[3]["properties"]["contract_start_date"] = json!("2026-02-01T23:00:00Z");
    scenarios.push((same_start, "https://example.invalid/old-main"));
    let mut tie = base.clone();
    tie[3]["properties"]["contract_start_date"] = json!("2026-02-01");
    tie[3]["properties"]["createdate"] = json!("2026-02-02");
    scenarios.push((tie, "https://example.invalid/old-main"));
    scenarios.push((base[..2].to_vec(), "https://example.invalid/option"));
    scenarios.push((base[..1].to_vec(), "https://example.invalid/ended"));
    // 未知・継続済・満了済・マーケ関連は生きた契約にしない。
    for stage in ["unknown-stage", "66848546", "1281526627", "1278456227"] {
        let mut rows = base.clone();
        rows[2]["properties"]["dealstage"] = json!(stage);
        rows[3]["properties"]["dealstage"] = json!(stage);
        scenarios.push((rows, "https://example.invalid/option"));
    }
    // 名前の先頭で判定するオプション。NFKC・先頭空白も外部バッチと揃える。
    for prefix in [
        "求人追加",
        " ＡｉｒＷｏｒｋ広告運用",
        "一次対応",
        "エントリーフォーム",
    ] {
        let mut rows = base.clone();
        rows[2]["properties"]["dealname"] = json!(format!("{prefix} 架空契約"));
        scenarios.push((rows, "https://example.invalid/old-main"));
    }
    let mut no_url = base.clone();
    no_url[2]["properties"]["customer_sheet_url"] = Value::Null;
    scenarios.push((no_url, "https://example.invalid/old-main"));
    let mut missing_start = base.clone();
    missing_start[2]["properties"]["contract_start_date"] = Value::Null;
    scenarios.push((missing_start, "https://example.invalid/old-main"));
    let mut missing_dates = base.clone();
    for r in &mut missing_dates[2..] {
        r["properties"]["contract_start_date"] = Value::Null;
        r["properties"]["createdate"] = Value::Null;
    }
    scenarios.push((missing_dates, "https://example.invalid/old-main"));
    scenarios.push((vec![], "https://example.invalid/synced"));
    for (deals, expected) in scenarios {
        fake.lock().unwrap().deals = deals;
        let (status, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &cookie).await;
        assert_eq!(status, StatusCode::OK, "{b}");
        assert_eq!(b["sheet_url"], expected);
        assert_eq!(b["sheet_embed"], false);
    }
    {
        let f = fake.lock().unwrap();
        let reads: Vec<_> = f
            .requests
            .iter()
            .filter(|(p, _)| p == "/crm/v3/objects/deals/batch/read")
            .collect();
        for (_, body) in reads {
            assert_eq!(
                body["inputs"],
                json!([{"id":"400"},{"id":"401"},{"id":"402"},{"id":"403"}])
            );
            assert_eq!(
                body["properties"],
                json!([
                    "customer_sheet_url",
                    "dealstage",
                    "dealname",
                    "contract_start_date",
                    "createdate"
                ])
            );
        }
    }
    fake.lock().unwrap().sheet = None;
    let (status, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &cookie).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        b["sheet_url"],
        "https://docs.google.com/spreadsheets/d/fake-job/edit"
    );
    assert_eq!(b["sheet_embed"], true);
}
#[tokio::test]
async fn applicant_report_sheet_deal_403_keeps_detail_and_fallback() {
    for path in [
        "/crm/v4/associations/0-420/deals/batch/read",
        "/crm/v3/objects/deals/batch/read",
    ] {
        let (app, cookie, fake) = app().await;
        {
            let mut f = fake.lock().unwrap();
            f.deal_forbidden = Some(path.into());
            f.sheet = Some("https://example.invalid/synced".into());
        }
        let (status, _, b) = get_json(&app, "/api/crm/applicants/100", &cookie).await;
        assert_eq!(status, StatusCode::OK, "{b}");
        assert_eq!(b["phone"], "00000000000");
        assert_eq!(b["notes"].as_array().unwrap().len(), 5);
        assert_eq!(b["sheet_url"], "https://example.invalid/synced");
        assert_eq!(
            fake.lock().unwrap().requests.len(),
            if path.contains("associations") { 8 } else { 9 }
        );
        fake.lock().unwrap().sheet = None;
        let (status, _, b) = get_json(&app, "/api/crm/applicants/100?fresh=1", &cookie).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            b["sheet_url"],
            "https://docs.google.com/spreadsheets/d/fake-job/edit"
        );
        assert_eq!(b["sheet_embed"], true);
    }
}
