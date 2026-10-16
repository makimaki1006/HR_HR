use super::*;
fn date(day: &str) -> NaiveDate {
    NaiveDate::parse_from_str(day, "%Y-%m-%d").unwrap()
}
fn history(key: &str, entries: &[(&str, &str)]) -> Value {
    json!({"properties":{key:entries.last().unwrap().1}, "propertiesWithHistory":{key:entries.iter().map(|(at,value)| json!({"timestamp":at,"value":value})).collect::<Vec<_>>()}})
}
#[test]
fn first_record_and_same_value_rewrites_do_not_count() {
    let (start, end) = bounds(date("2026-10-16"));
    let first = history("hs_name", &[("2026-10-15T01:00:00Z", "配送")]);
    assert_eq!(detect(&first, start, end), BTreeMap::new());
    let same = history(
        "hs_name",
        &[
            ("2026-10-01T00:00:00Z", "配送"),
            ("2026-10-15T01:00:00Z", "配送"),
        ],
    );
    assert_eq!(detect(&same, start, end), BTreeMap::new());
}
#[test]
fn jst_window_includes_six_days_ago_midnight_and_excludes_next_midnight() {
    let (start, end) = bounds(date("2026-10-16"));
    assert_eq!(start.to_rfc3339(), "2026-10-09T15:00:00+00:00");
    assert_eq!(end.to_rfc3339(), "2026-10-16T15:00:00+00:00");
    for (at, expected) in [
        ("2026-10-09T14:59:59.999Z", false),
        ("2026-10-09T15:00:00Z", true),
        ("2026-10-16T14:59:59.999Z", true),
        ("2026-10-16T15:00:00Z", false),
    ] {
        let data = history("hs_name", &[("2026-10-01T00:00:00Z", "配送"), (at, "倉庫")]);
        assert_eq!(
            detect(&data, start, end).contains_key("求人名"),
            expected,
            "{at}"
        );
    }
}
#[test]
fn earlier_change_counts_even_when_current_value_was_reverted() {
    let (start, end) = bounds(date("2026-10-16"));
    let data = history(
        "hs_name",
        &[
            ("2026-10-01T00:00:00Z", "配送"),
            ("2026-10-11T00:00:00Z", "倉庫"),
            ("2026-10-12T00:00:00Z", "配送"),
            ("2026-10-13T00:00:00Z", "配送"),
        ],
    );
    assert_eq!(
        detect(&data, start, end)["求人名"].to_rfc3339(),
        "2026-10-12T00:00:00+00:00"
    );
}
#[test]
fn ambiguous_or_invalid_history_cannot_establish_change() {
    let (start, end) = bounds(date("2026-10-16"));
    let mut data = history(
        "hs_name",
        &[
            ("2026-10-01T00:00:00Z", "配送"),
            ("2026-10-11T00:00:00Z", "倉庫"),
            ("2026-10-11T00:00:00Z", "配送"),
        ],
    );
    assert!(detect(&data, start, end).is_empty());
    data = history(
        "hs_name",
        &[
            ("2026-10-01T00:00:00Z", "配送"),
            ("2026-10-11T00:00:00Z", "倉庫"),
        ],
    );
    data["properties"]["hs_name"] = json!("別の値");
    assert!(detect(&data, start, end).is_empty());
    data["propertiesWithHistory"]["hs_name"][0]["timestamp"] = json!("invalid");
    assert!(detect(&data, start, end).is_empty());
}
#[test]
fn salary_uses_summary_fields_for_both_media() {
    let (start, end) = bounds(date("2026-10-16"));
    for key in ["hrh_kyuujinhyou_honbun", "shigotonaiyou"] {
        let data = history(
            key,
            &[
                (
                    "2026-10-01T00:00:00Z",
                    "基本給与 最小：1000\n基本給与 最大：1500\n",
                ),
                (
                    "2026-10-11T00:00:00Z",
                    "基本給与 最小：1200\n基本給与 最大：1800\n",
                ),
            ],
        );
        assert_eq!(
            detect(&data, start, end)
                .keys()
                .copied()
                .collect::<Vec<_>>(),
            vec!["本文", "給与"]
        );
        assert_eq!(
            salary(data["properties"][key].as_str().unwrap(), false),
            vec![Some("1200".into()), Some("1800".into())]
        );
    }
    for key in ["hrh_kyuujinhyou_honbun", "kyuuyokeitai"] {
        let (a, b) = if key == "kyuuyokeitai" {
            ("時給", "月給")
        } else {
            ("給与形態：1\n", "給与形態：2\n")
        };
        let data = history(
            key,
            &[("2026-10-01T00:00:00Z", a), ("2026-10-11T00:00:00Z", b)],
        );
        assert!(detect(&data, start, end).contains_key("給与"));
    }
    let data = history(
        "hrh_kyuujinhyou_honbun",
        &[
            (
                "2026-10-01T00:00:00Z",
                "仕事内容：配送\n基本給与 最小：1000\n",
            ),
            (
                "2026-10-11T00:00:00Z",
                "仕事内容：倉庫\n基本給与 最小：1000\n",
            ),
        ],
    );
    assert_eq!(
        detect(&data, start, end)
            .keys()
            .copied()
            .collect::<Vec<_>>(),
        vec!["本文"]
    );
}
#[test]
fn ended_listings_count_and_deals_are_deduplicated_and_filtered() {
    let (start, end) = bounds(date("2026-10-16"));
    let mut ended = history(
        "hs_name",
        &[
            ("2026-10-01T00:00:00Z", "配送"),
            ("2026-10-11T00:00:00Z", "倉庫"),
        ],
    );
    ended["properties"]["kyuujin_status"] = json!("掲載終了");
    let changed = BTreeMap::from([
        ("1".into(), detect(&ended, start, end)),
        ("2".into(), BTreeMap::from([("給与", start)])),
    ]);
    let associations = BTreeMap::from([
        (
            "1".into(),
            vec!["10".into(), "10".into(), "20".into(), "30".into()],
        ),
        ("2".into(), vec!["10".into()]),
    ]);
    let deals: Vec<Record> = serde_json::from_value(json!([
        {"id":"10","properties":{"dealname":"合成案件A","pipeline":"21596025"}},
        {"id":"20","properties":{"dealname":"合成案件B","pipeline":"743961896"}},
        {"id":"30","properties":{"dealname":"合成対象外","pipeline":"other"}}
    ]))
    .unwrap();
    let report = aggregate(&changed, &associations, &deals);
    assert_eq!(report.count, 2);
    assert_eq!(report.deals[0].id, "10");
    assert_eq!(report.deals[0].changed_listings, 2);
    assert_eq!(report.deals[0].fields, BTreeSet::from(["給与", "求人名"]));
    assert_eq!(report.deals[1].changed_listings, 1);
}

/// 本番読み取り専用の計測。出力は件数と呼び出し回数のみ。
#[tokio::test]
#[ignore]
async fn production_read_probe() {
    let token = std::env::var("HUBSPOT_ACCESS_TOKEN").expect("token supplied through environment");
    let mut service = JobReadService::new(token).unwrap();
    service.hs = service.hs.background();
    let today = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let (start, _) = bounds(today);
    let mut stats = Stats::default();
    let ids = candidate_ids(
        &service,
        start.timestamp_millis(),
        Utc::now().timestamp_millis(),
        &mut stats,
    )
    .await
    .unwrap();
    eprintln!(
        "recent_changes_search day={today} candidates={} search_calls={}",
        stats.candidates, stats.search_calls
    );
    let changed = histories(&service, &ids, start, Utc::now(), &mut stats).await;
    let usage = service.hs.gateway().snapshot();
    eprintln!("recent_changes_history candidates={} search_calls={} history_calls={} network_calls={} rate_limited={}",stats.candidates,stats.search_calls,stats.history_calls,usage.total_calls,usage.rate_limited);
    let changed = changed.unwrap();
    let report = resolve(&service, &changed).await.unwrap();
    let usage = service.hs.gateway().snapshot();
    eprintln!("recent_changes_probe day={today} candidates={} search_calls={} history_calls={} changed_listings={} delivery_deals={} network_calls={} network_search_calls={} rate_limited={}",stats.candidates,stats.search_calls,stats.history_calls,changed.len(),report.count,usage.total_calls,usage.search_calls,usage.rate_limited);
}

#[tokio::test]
async fn search_splits_over_limit_before_paging_and_deduplicates_ids() {
    use axum::{body::Body, http::Request, routing::post};
    use std::sync::atomic::{AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let observed = calls.clone();
    let app = Router::new().route("/crm/v3/objects/0-420/search", post(move |req: Request<Body>| {
        let calls=observed.clone();
        async move {
            calls.fetch_add(1,Ordering::SeqCst);
            let body: Value=serde_json::from_slice(&axum::body::to_bytes(req.into_body(),10000).await.unwrap()).unwrap();
            let filters=body["filterGroups"][0]["filters"].as_array().unwrap();
            let low=filters[0]["value"].as_str().unwrap().parse::<i64>().unwrap();
            let high=filters[1]["value"].as_str().unwrap().parse::<i64>().unwrap();
            assert_eq!(body["properties"],json!(["hs_object_id"]));
            assert!(body.get("propertiesWithHistory").is_none());
            if high-low>10 { assert!(body.get("after").is_none()); Json(json!({"total":10000,"results":[]})) }
            else if body.get("after").is_none() { Json(json!({"total":2,"results":[{"id":low.to_string()}],"paging":{"next":{"after":"1"}}})) }
            else { Json(json!({"total":2,"results":[{"id":(low+1).to_string()}]})) }
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let mut stats = Stats::default();
    let ids = candidate_ids(&service, 100, 120, &mut stats).await.unwrap();
    assert_eq!(ids, vec!["100", "101", "110", "111"]);
    assert_eq!(stats.candidates, 4);
    assert_eq!(stats.search_calls, 5);
    assert_eq!(calls.load(Ordering::SeqCst), 5);
    server.abort();
}

#[tokio::test]
async fn cold_and_stale_reads_never_wait_and_share_one_background_refresh() {
    use axum::{body::Body, http::Request, routing::any};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let fails = Arc::new(AtomicBool::new(false));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (observed, failing, wait) = (calls.clone(), fails.clone(), gate.clone());
    let today = (Utc::now() + chrono::Duration::hours(9)).date_naive();
    let (start, _) = bounds(today);
    let at = (start + chrono::Duration::hours(1)).to_rfc3339();
    let app=Router::new().fallback(any(move |req: Request<Body>| {
        let (calls,fails,gate,at)=(observed.clone(),failing.clone(),wait.clone(),at.clone());
        async move {
            calls.fetch_add(1,Ordering::SeqCst);
            let path=req.uri().path().to_owned();
            if path.ends_with("/search") { gate.acquire().await.unwrap().forget(); }
            if fails.load(Ordering::SeqCst) { return (StatusCode::FORBIDDEN,Json(json!({}))).into_response(); }
            let value=if path.ends_with("/search") { json!({"total":1,"results":[{"id":"1"}]}) }
            else if path=="/crm/v3/objects/0-420/batch/read" {
                let body:Value=serde_json::from_slice(&axum::body::to_bytes(req.into_body(),10000).await.unwrap()).unwrap();
                assert_eq!(body["propertiesWithHistory"],json!(PROPERTIES));
                let mut row=history("hs_name",&[("2020-01-01T00:00:00Z","旧求人"),(&at,"新求人")]);
                row["id"]=json!("1"); row["properties"]["kyuujin_status"]=json!("掲載終了");
                json!({"results":[row]})
            } else if path=="/crm/v4/associations/0-420/deals/batch/read" { json!({"results":[{"from":{"id":"1"},"to":[{"toObjectId":10}]}]}) }
            else if path=="/crm/v3/objects/deals/batch/read" { json!({"results":[{"id":"10","properties":{"dealname":"合成案件","pipeline":"21596025"}}]}) }
            else if path=="/crm/v4/associations/deals/companies/batch/read" { json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":100}]}]}) }
            else if path=="/crm/v3/objects/companies/batch/read" { json!({"results":[{"id":"100","properties":{"name":"合成会社"}}]}) }
            else { panic!("unexpected read"); };
            Json(value).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    for _ in 0..20 {
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), answer(&service, today))
                .await
                .unwrap()["status"],
            "preparing"
        );
    }
    gate.add_permits(1);
    settle(&service).await;
    let ready = answer(&service, today).await;
    assert_eq!(ready["status"], "ready");
    assert_eq!(ready["report"]["count"], 1);
    assert_eq!(ready["report"]["deals"][0]["company"], "合成会社");
    assert_eq!(ready["report"]["deals"][0]["changed_listings"], 1);
    assert_eq!(ready["report"]["deals"][0]["fields"], json!(["求人名"]));
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    assert_eq!(answer(&service, today).await, ready);
    assert_eq!(calls.load(Ordering::SeqCst), 6);
    service
        .recent_changes
        .state
        .lock()
        .await
        .snapshot
        .as_mut()
        .unwrap()
        .1 = Instant::now() - TTL;
    fails.store(true, Ordering::SeqCst);
    for _ in 0..20 {
        assert_eq!(
            tokio::time::timeout(Duration::from_millis(100), answer(&service, today))
                .await
                .unwrap(),
            ready
        );
    }
    gate.add_permits(1);
    settle(&service).await;
    let failed = answer(&service, today).await;
    assert_eq!(failed["report"], ready["report"]);
    assert_eq!(failed["refresh_failed"], true);
    assert_eq!(calls.load(Ordering::SeqCst), 7);
    assert_eq!(
        answer(&service, today + chrono::Duration::days(1)).await["status"],
        "failed"
    );
    assert_eq!(calls.load(Ordering::SeqCst), 7);
    assert_eq!(
        service.hs.background().priority(),
        crate::hubspot::gateway::Priority::Background
    );
    server.abort();
}
async fn settle(service: &JobReadService) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while service.recent_changes.state.lock().await.refreshing {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
}

#[tokio::test]
async fn history_batches_are_at_most_fifty_and_partial_results_fail() {
    use axum::routing::post;
    use std::sync::atomic::{AtomicBool, Ordering};
    let sizes = Arc::new(std::sync::Mutex::new(Vec::new()));
    let partial = Arc::new(AtomicBool::new(false));
    let (observed, missing) = (sizes.clone(), partial.clone());
    let app = Router::new().route(
        "/crm/v3/objects/0-420/batch/read",
        post(move |Json(body): Json<Value>| {
            let (sizes, partial) = (observed.clone(), missing.clone());
            async move {
                let inputs = body["inputs"].as_array().unwrap();
                sizes.lock().unwrap().push(inputs.len());
                assert!(inputs.len() <= 50);
                let mut rows: Vec<_> = inputs
                    .iter()
                    .map(|v| json!({"id":v["id"],"properties":{},"propertiesWithHistory":{}}))
                    .collect();
                if partial.load(Ordering::SeqCst) {
                    rows.pop();
                }
                Json(json!({"results":rows}))
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let ids: Vec<_> = (1..=121).map(|id| id.to_string()).collect();
    let (start, end) = bounds(date("2026-10-16"));
    let mut stats = Stats::default();
    assert!(histories(&service, &ids, start, end, &mut stats)
        .await
        .unwrap()
        .is_empty());
    assert_eq!(*sizes.lock().unwrap(), vec![50, 50, 21]);
    assert_eq!(stats.history_calls, 3);
    partial.store(true, Ordering::SeqCst);
    assert!(histories(&service, &ids, start, end, &mut stats)
        .await
        .is_err());
    server.abort();
}
