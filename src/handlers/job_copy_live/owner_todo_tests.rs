use super::*;
fn day(s: &str) -> NaiveDate {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").unwrap()
}
fn record(
    id: &str,
    stage: &str,
    cs: &str,
    owner: &str,
    action: &str,
    expiration: &str,
) -> WorkDeal {
    let record = Record {
        id: id.into(),
        properties: [
            ("pipeline", "21596025"),
            ("dealname", "合成案件"),
            ("dealstage", stage),
            ("consultant", cs),
            ("hubspot_owner_id", owner),
            ("actionyotei", action),
            ("actionnaiyou", "call"),
            ("contract_expiration_date", expiration),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), Some(v.into())))
        .collect(),
    };
    WorkDeal {
        assignment: Assignment::from_record(&record),
        record,
    }
}
fn snapshot() -> Snapshot {
    Snapshot {
        owners: vec![
            Owner {
                id: "11".into(),
                name: "合成担当A".into(),
                email: "reader@example.test".into(),
                archived: false,
            },
            Owner {
                id: "22".into(),
                name: "合成担当B".into(),
                email: "former@example.test".into(),
                archived: true,
            },
            Owner {
                id: "33".into(),
                name: "合成担当C".into(),
                email: "other@example.test".into(),
                archived: false,
            },
        ],
        deals: BTreeMap::from([
            (
                "1".into(),
                record("1", "open", "11", "22", "2026-10-15", "2026-10-16"),
            ),
            (
                "2".into(),
                record("2", "open", "11", "22", "2026-10-16", "2026-11-15"),
            ),
            (
                "3".into(),
                record("3", "open", "22", "11", "2026-10-23", "2026-11-16"),
            ),
            (
                "4".into(),
                record("4", "open", "22", "11", "2026-10-24", "2026-10-15"),
            ),
            (
                "5".into(),
                record("5", "open", "11", "22", "2026-10-17", ""),
            ),
        ]),
        labels: BTreeMap::from([("call".into(), "電話する".into())]),
        stages: BTreeMap::from([("open".into(), "対応中".into())]),
        ..Default::default()
    }
}
#[test]
fn resolves_self_switches_field_and_only_assigned_owners_including_former() {
    let s = snapshot();
    let q = OwnerQuery::default();
    assert_eq!(
        s.selection(&q, " Reader@Example.Test "),
        (Some("11".into()), None)
    );
    assert!(s.selection(&q, "missing@example.test").1.is_some());
    let today = day("2026-10-16");
    let report = s.report(&q, "reader@example.test", today, false);
    assert_eq!(report["owner"], "11");
    assert_eq!(report["owners"].as_array().unwrap().len(), 2);
    assert_eq!(
        report["owners"][1],
        json!({"id":"22","name":"合成担当B（退職）","archived":true})
    );
    assert_eq!(report["actions"]["overdue"][0]["id"], "1");
    assert_eq!(report["actions"]["today"][0]["content"], "電話する");
    assert_eq!(report["actions"]["today"][0]["stage"], "対応中");
    assert_eq!(report["actions"]["week"][0]["id"], "5");
    assert_eq!(report["expirations"].as_array().unwrap().len(), 2);
    let q = OwnerQuery {
        owner_field: Some("hubspot_owner_id".into()),
        ..q
    };
    let report = s.report(&q, "reader@example.test", today, false);
    assert_eq!(report["actions"]["week"][0]["id"], "3");
    assert_eq!(report["actions"]["later"][0]["id"], "4");
    assert_eq!(
        report["actions"]["week"][0]["consultant"],
        "合成担当B（退職）"
    );
    assert_eq!(report["expirations"][0]["id"], "3");
    let q = OwnerQuery {
        owner: Some("all".into()),
        ..Default::default()
    };
    let report = s.report(&q, "reader@example.test", today, false);
    assert_eq!(report["actions"]["week"].as_array().unwrap().len(), 2);
    assert!(s.matches("1", "consultant", Some("11")));
    assert!(!s.matches("1", "hubspot_owner_id", Some("11")));
}
#[test]
fn jst_midnight_and_day_seven_and_sixty_are_inclusive() {
    assert_eq!(
        today("2026-10-15T14:59:59Z".parse().unwrap()),
        day("2026-10-15")
    );
    assert_eq!(
        today("2026-10-15T15:00:00Z".parse().unwrap()),
        day("2026-10-16")
    );
    assert_eq!(date("2026-10-15T15:00:00Z"), Some(day("2026-10-16")));
    assert_eq!(date("0"), Some(day("1970-01-01")));
    assert_eq!(date("2026-02-30"), None);
    let d = day("2026-10-16");
    for (due, expected) in [
        ("2026-10-15", "overdue"),
        ("2026-10-16", "today"),
        ("2026-10-17", "week"),
        ("2026-10-23", "week"),
        ("2026-10-24", "later"),
    ] {
        assert_eq!(bucket(d, day(due)), expected);
    }
    let s = snapshot();
    let r = s.report(
        &OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        },
        "",
        d,
        false,
    );
    assert_eq!(
        r["expirations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v["due"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["2026-10-16", "2026-11-15", "2026-11-16"]
    );
}
#[test]
fn excludes_exact_six_ended_stage_ids_without_using_closed_probability() {
    let mut s = snapshot();
    for (n, stage) in ENDED_STAGES.iter().enumerate() {
        let id = (100 + n).to_string();
        s.deals.insert(
            id.clone(),
            record(&id, stage, "11", "11", "2026-10-16", "2026-10-16"),
        );
    }
    let r = s.report(
        &OwnerQuery::default(),
        "reader@example.test",
        day("2026-10-16"),
        false,
    );
    assert_eq!(r["actions"]["today"].as_array().unwrap().len(), 1);
    assert_eq!(r["expirations"].as_array().unwrap().len(), 2);
    // 他のIDは確率/closedにかかわらず残す。
    s.deals
        .get_mut("2")
        .unwrap()
        .record
        .properties
        .insert("hs_is_closed".into(), Some("true".into()));
    assert_eq!(
        s.report(
            &OwnerQuery::default(),
            "reader@example.test",
            day("2026-10-16"),
            false
        )["actions"]["today"][0]["id"],
        "2"
    );
}
#[test]
fn validates_selection_and_owner_pages_without_exposing_email() {
    assert!(OwnerQuery {
        owner_field: Some("email".into()),
        ..Default::default()
    }
    .validate()
    .is_err());
    assert!(OwnerQuery {
        owner: Some("oops".into()),
        ..Default::default()
    }
    .validate()
    .is_err());
    let owners=parse_owners(&json!({"results":[{"id":"22","lastName":"合成","firstName":"担当","email":" Reader@Example.Test "}]}),true).unwrap();
    assert_eq!(owners[0].email, "reader@example.test");
    assert_eq!(owners[0].name, "合成 担当");
    assert!(owners[0].archived);
    assert!(serde_json::to_value(&owners).unwrap()[0]
        .get("email")
        .is_none());
    assert_eq!(
        next(&json!({"paging":{"next":{"after":"200"}}})).unwrap(),
        Some("200".into())
    );
    assert_eq!(
        next(&json!({"paging":{"next":{"after":"synthetic+cursor=="}}})).unwrap(),
        Some("synthetic+cursor==".into())
    );
    assert!(next(&json!({"paging":{"next":{"after":"bad\ncursor"}}})).is_err());
    assert!(next(&json!({"paging":{}})).is_err());
}

/// 明示実行だけ。本番への書き込みは行わず、秘密・レコードは出力しない。
#[tokio::test]
#[ignore]
async fn production_read_probe() {
    let token = std::env::var("HUBSPOT_ACCESS_TOKEN").expect("probe token is required");
    let service = JobReadService::new(token).unwrap();
    let started = Instant::now();
    let snapshot = fetch(&service)
        .await
        .expect("production read failed (no payload logged)");
    let cold_ms = started.elapsed().as_millis();
    let gateway = service.hs.gateway().snapshot();
    let day = today(Utc::now());
    let report = snapshot.report(
        &OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        },
        "",
        day,
        false,
    );
    let counts: BTreeMap<_, _> = report["actions"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_array().unwrap().len()))
        .collect();
    eprintln!(
        "PROBE {}",
        json!({"today_jst":day.to_string(),"scope":"all_delivery_owners","delivery_deals":snapshot.deals.len(),"actions":counts,"expirations_60_days":report["expirations"].as_array().unwrap().len(),"cold_ms":cold_ms,"cold_calls":gateway.total_calls,"search_calls":gateway.search_calls,"calls_by_group":gateway.calls_by_group})
    );
    service.owner_todo.state.lock().await.snapshot = Some((Instant::now(), Arc::new(snapshot)));
    let started = Instant::now();
    let result = view(&service).await;
    assert!(result.snapshot.is_some());
    eprintln!(
        "PROBE {}",
        json!({"warm_ms":started.elapsed().as_secs_f64()*1000.0,"warm_calls":service.hs.gateway().snapshot().total_calls-gateway.total_calls})
    );
}

#[tokio::test]
async fn background_refresh_is_single_flight_keeps_stale_and_owners_are_cached_ten_minutes() {
    use axum::{body::Body, http::Request, routing::any};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let calls = Arc::new(AtomicUsize::new(0));
    let owner_calls = Arc::new(AtomicUsize::new(0));
    let block = Arc::new(AtomicBool::new(false));
    let fails = Arc::new(AtomicBool::new(false));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let (c, o, b, f, g) = (
        calls.clone(),
        owner_calls.clone(),
        block.clone(),
        fails.clone(),
        gate.clone(),
    );
    let app=Router::new().fallback(any(move |req: Request<Body>| {
        let (c,o,b,f,g)=(c.clone(),o.clone(),b.clone(),f.clone(),g.clone());
        async move {
            c.fetch_add(1,Ordering::SeqCst);
            let path=req.uri().path();
            if path.ends_with("/search") {
                if b.load(Ordering::SeqCst) {g.acquire().await.unwrap().forget();}
                if f.load(Ordering::SeqCst) {return (StatusCode::FORBIDDEN,Json(json!({}))).into_response();}
                return Json(json!({"total":1,"results":[{"id":"1","properties":{"pipeline":"21596025","dealstage":"open","consultant":"11","actionyotei":"2026-10-16","actionnaiyou":"call"}}]})).into_response();
            }
            let data=if path=="/crm/v3/owners" {o.fetch_add(1,Ordering::SeqCst);json!({"results":if req.uri().query().unwrap_or("").contains("archived=true") {vec![]} else {vec![json!({"id":"11","firstName":"合成担当","email":"reader@example.test"})]}})}
            else if path.contains("/properties/") {json!({"options":[{"value":"call","label":"電話する"}]})}
            else if path.contains("/pipelines/") {json!({"results":[{"id":"21596025","label":"納品管理","stages":[{"id":"open","label":"対応中"}]}]})}
            else if path.contains("/associations/") {json!({"results":[{"from":{"id":"1"},"to":[{"toObjectId":901}]}]})}
            else {json!({"results":[{"id":"901","properties":{"name":"合成事業所"}}]})};
            Json(data).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    assert!(view(&service).await.snapshot.is_none());
    async fn settled(service: &JobReadService) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if !service.owner_todo.state.lock().await.refreshing {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    settled(&service).await;
    let before = calls.load(Ordering::SeqCst);
    assert_eq!(owner_calls.load(Ordering::SeqCst), 2);
    for _ in 0..10 {
        let v = view(&service).await;
        let s = v.snapshot.unwrap();
        assert_eq!(s.deals.len(), 1);
        assert_eq!(
            s.report(
                &OwnerQuery::default(),
                "reader@example.test",
                day("2026-10-16"),
                v.failed
            )["actions"]["today"][0]["company"],
            "合成事業所"
        );
    }
    assert_eq!(calls.load(Ordering::SeqCst), before);
    service
        .owner_todo
        .state
        .lock()
        .await
        .snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - TTL;
    block.store(true, Ordering::SeqCst);
    for _ in 0..10 {
        assert!(
            tokio::time::timeout(Duration::from_millis(100), view(&service))
                .await
                .unwrap()
                .snapshot
                .is_some()
        );
    }
    block.store(false, Ordering::SeqCst);
    gate.add_permits(1);
    settled(&service).await;
    assert_eq!(
        owner_calls.load(Ordering::SeqCst),
        2,
        "owners reused for five-minute deal refresh"
    );
    assert_eq!(
        calls.load(Ordering::SeqCst) - before,
        3,
        "one search + association + company; no definitions or owner calls"
    );
    service.owner_todo.owners.lock().await.as_mut().unwrap().0 = Instant::now() - OWNER_TTL;
    assert_eq!(owners(&service).await.unwrap().len(), 1);
    assert_eq!(owner_calls.load(Ordering::SeqCst), 4);
    service
        .owner_todo
        .state
        .lock()
        .await
        .snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - TTL;
    fails.store(true, Ordering::SeqCst);
    view(&service).await;
    settled(&service).await;
    let before = calls.load(Ordering::SeqCst);
    let v = view(&service).await;
    assert!(v.failed);
    assert_eq!(v.snapshot.unwrap().deals.len(), 1);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        before,
        "failure retries are bounded"
    );
    server.abort();
}

#[tokio::test]
async fn saved_expiration_and_stage_replace_cached_values_and_reject_inflight_reads() {
    use axum::routing::any;
    let latest = Arc::new(std::sync::Mutex::new(record(
        "1",
        "open",
        "11",
        "22",
        "2026-10-16",
        "2026-10-20",
    )));
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let started = Arc::new(tokio::sync::Notify::new());
    let (remote, wait, entered) = (latest.clone(), gate.clone(), started.clone());
    let app = Router::new().fallback(any(move |req: axum::extract::Request| {
        let (remote, wait, entered) = (remote.clone(), wait.clone(), entered.clone());
        async move {
            if req.method() == axum::http::Method::PATCH {
                return Json(json!(remote.lock().unwrap().record));
            }
            let path = req.uri().path();
            let data = if path.ends_with("/search") {
                let record = remote.lock().unwrap().record.clone();
                entered.notify_one();
                wait.acquire().await.unwrap().forget();
                json!({"results":[record]})
            } else if path.contains("/owners") {
                json!({"results":[]})
            } else if path.contains("/properties/") {
                json!({"options":[]})
            } else if path.contains("/pipelines/") {
                json!({"results":[]})
            } else if path.contains("/associations/") {
                json!({"results":[{"from":{"id":"1"},"to":[]}]})
            } else {
                json!({"results":[]})
            };
            Json(data)
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    async fn finish(service: &JobReadService) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while service.owner_todo.state.lock().await.refreshing {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    let query = OwnerQuery {
        owner: Some("all".into()),
        ..Default::default()
    };
    let report = |s: &Snapshot| s.report(&query, "", day("2026-10-16"), false);
    view(&service).await;
    started.notified().await;
    // 保存前に始まった Search が後から完了しても採用しない。
    *latest.lock().unwrap() = record("1", "open", "11", "22", "2026-10-16", "2026-11-15");
    service
        .hs
        .background()
        .patch_object("deals", "1", &BTreeMap::new())
        .await
        .unwrap();
    gate.add_permits(1);
    finish(&service).await;
    assert!(service.owner_todo.state.lock().await.snapshot.is_none());
    view(&service).await;
    started.notified().await;
    gate.add_permits(1);
    finish(&service).await;
    let current = view(&service).await.snapshot.unwrap();
    assert_eq!(report(&current)["expirations"][0]["due"], "2026-11-15");
    assert_eq!(
        report(&current)["actions"]["today"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    *latest.lock().unwrap() = record("1", "90598807", "11", "22", "2026-10-16", "2026-11-15");
    service
        .hs
        .background()
        .patch_object("deals", "1", &BTreeMap::new())
        .await
        .unwrap();
    assert!(view(&service).await.snapshot.is_none());
    started.notified().await;
    gate.add_permits(1);
    finish(&service).await;
    let current = view(&service).await.snapshot.unwrap();
    assert_eq!(report(&current)["expirations"], json!([]));
    assert_eq!(report(&current)["actions"]["today"], json!([]));
    server.abort();
}

#[test]
fn merges_reasons_sorts_count_then_expiration_and_filters_owner() {
    let mut s = snapshot();
    for (id, contact) in [("1", "2026-09-16"), ("2", "2026-09-01")] {
        s.deals
            .get_mut(id)
            .unwrap()
            .record
            .properties
            .insert("notes_last_contacted".into(), Some(contact.into()));
    }
    let r = s.report(
        &OwnerQuery {
            owner: Some("all".into()),
            ..Default::default()
        },
        "",
        day("2026-10-16"),
        false,
    );
    let rows = r["signals"].as_array().unwrap();
    assert_eq!(
        rows.iter()
            .map(|v| v["id"].as_str().unwrap())
            .collect::<Vec<_>>(),
        ["1", "2", "4", "3"]
    );
    assert_eq!(rows[0]["reasons"].as_array().unwrap().len(), 2);
    assert_eq!(rows[1]["reasons"].as_array().unwrap().len(), 2);
    // 4 は満了日（10/15）が今日より前で終了していないので「満了後も稼働中」だけが付く
    assert_eq!(
        rows[2]["reasons"],
        json!([{"kind":"expired","text":"満了日から 1 日が過ぎている"}])
    );
    assert_eq!(rows[3]["reasons"].as_array().unwrap().len(), 1);
    let r = s.report(
        &OwnerQuery {
            owner: Some("22".into()),
            ..Default::default()
        },
        "",
        day("2026-10-16"),
        false,
    );
    assert_eq!(r["signals"].as_array().unwrap().len(), 2);
    assert_eq!(r["signals"][0]["id"], "4");
    assert_eq!(r["signals"][1]["id"], "3");
}

#[tokio::test]
async fn saving_next_plan_updates_the_existing_snapshot_without_rereading_applications() {
    let service = JobReadService::for_test("http://127.0.0.1:1".into());
    let mut snapshot = snapshot();
    snapshot
        .companies
        .insert("1".into(), ("901".into(), Some("合成事業所".into())));
    service.owner_todo.state.lock().await.snapshot = Some((Instant::now(), Arc::new(snapshot)));
    let mut fresh = record("1", "open", "11", "22", "2026-10-23", "2026-11-15").record;
    fresh
        .properties
        .insert("actionnaiyou".into(), Some("call".into()));
    apply_fresh_deal(&service, fresh).await;
    let state = service.owner_todo.state.lock().await;
    let report = state.snapshot.as_ref().unwrap().1.report(
        &OwnerQuery::default(),
        "reader@example.test",
        day("2026-10-16"),
        false,
    );
    assert_eq!(report["actions"]["overdue"], json!([]));
    assert_eq!(report["actions"]["week"][1]["id"], "1");
    assert_eq!(report["actions"]["week"][1]["actionyotei"], "2026-10-23");
    assert_eq!(report["actions"]["week"][1]["content"], "電話する");
    assert_eq!(report["signals"][0]["actionyotei"], "2026-10-23");
}

#[tokio::test]
async fn deals_five_minutes_and_applications_sixty_minutes_refresh_only_on_demand() {
    use axum::{body::Body, http::Request, routing::any};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    let searches = Arc::new(AtomicUsize::new(0));
    let applications = Arc::new(AtomicUsize::new(0));
    let fail_apps = Arc::new(AtomicBool::new(false));
    let app_gate = Arc::new(tokio::sync::Semaphore::new(0));
    let latest = Arc::new(std::sync::Mutex::new(
        record("1", "open", "11", "22", "2026-10-16", "2026-10-20").record,
    ));
    latest
        .lock()
        .unwrap()
        .properties
        .insert("contract_start_date".into(), Some("2026-08-17".into()));
    let (s, a, f, g, r) = (
        searches.clone(),
        applications.clone(),
        fail_apps.clone(),
        app_gate.clone(),
        latest.clone(),
    );
    let app = Router::new().fallback(any(move |req: Request<Body>| {
        let (s, a, f, g, r) = (s.clone(), a.clone(), f.clone(), g.clone(), r.clone());
        async move {
            let path = req.uri().path();
            let data = match path {
                "/crm/v3/objects/deals/search" => {
                    s.fetch_add(1, Ordering::SeqCst);
                    json!({"results":[r.lock().unwrap().clone()]})
                }
                "/crm/v4/associations/deals/0-420/batch/read" => {
                    a.fetch_add(1, Ordering::SeqCst);
                    g.acquire().await.unwrap().forget();
                    if f.load(Ordering::SeqCst) {
                        return (StatusCode::FORBIDDEN, Json(json!({}))).into_response();
                    }
                    json!({"results":[{"from":{"id":"1"},"to":[{"toObjectId":10}]}]})
                }
                "/crm/v3/objects/0-420/batch/read" => {
                    json!({"results":[{"id":"10","properties":{"kyuujin_status":"公開中"}}]})
                }
                "/crm/v4/associations/0-420/0-421/batch/read" => {
                    json!({"results":[{"from":{"id":"10"},"to":[{"toObjectId":100}]}]})
                }
                "/crm/v3/objects/0-421/batch/read" => {
                    json!({"results":[{"id":"100","properties":{"yingmuri":"2026-10-15"}}]})
                }
                "/crm/v4/associations/deals/companies/batch/read" => {
                    json!({"results":[{"from":{"id":"1"},"to":[{"toObjectId":901}]}]})
                }
                "/crm/v3/objects/companies/batch/read" => {
                    json!({"results":[{"id":"901","properties":{"name":"合成事業所"}}]})
                }
                "/crm/v3/properties/deals/actionnaiyou" => json!({"options":[]}),
                _ => json!({"results":[]}),
            };
            Json(data).into_response()
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let service = JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    async fn settle_deals(service: &JobReadService) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while service.owner_todo.state.lock().await.refreshing {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    async fn settle_apps(service: &JobReadService) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while service.owner_todo.state.lock().await.facts_refreshing {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }
    assert!(work_view(&service).await.snapshot.is_none());
    settle_deals(&service).await;
    assert_eq!(searches.load(Ordering::SeqCst), 1);
    assert_eq!(
        applications.load(Ordering::SeqCst),
        0,
        "取引取得の完了だけでは応募を読み始めない"
    );
    view(&service).await;
    assert!(
        !service.owner_todo.state.lock().await.facts_refreshing,
        "担当者の絞り込みだけでは応募を取得しない"
    );
    assert!(work_view(&service).await.partial);
    app_gate.add_permits(1);
    settle_apps(&service).await;
    assert!(!work_view(&service).await.partial);
    assert_eq!(applications.load(Ordering::SeqCst), 1);
    assert_eq!(
        work_view(&service).await.snapshot.unwrap().facts["1"].days,
        [day("2026-10-15")]
    );
    let app_at = service
        .owner_todo
        .state
        .lock()
        .await
        .facts_snapshot
        .as_ref()
        .unwrap()
        .0;

    latest
        .lock()
        .unwrap()
        .properties
        .insert("actionyotei".into(), Some("2026-10-23".into()));
    service
        .owner_todo
        .state
        .lock()
        .await
        .snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - TTL;
    work_view(&service).await;
    settle_deals(&service).await;
    let v = work_view(&service).await;
    assert_eq!(
        v.snapshot.unwrap().deals["1"]
            .assignment
            .actionyotei
            .as_deref(),
        Some("2026-10-23")
    );
    assert_eq!(searches.load(Ordering::SeqCst), 2);
    assert_eq!(
        applications.load(Ordering::SeqCst),
        1,
        "5分の更新で応募を読まない"
    );
    assert_eq!(
        service
            .owner_todo
            .state
            .lock()
            .await
            .facts_snapshot
            .as_ref()
            .unwrap()
            .0,
        app_at
    );
    let fresh = latest.lock().unwrap().clone();
    apply_fresh_deal(&service, fresh).await;
    assert_eq!(
        applications.load(Ordering::SeqCst),
        1,
        "予定保存で応募を読まない"
    );

    service
        .owner_todo
        .state
        .lock()
        .await
        .facts_snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - Duration::from_secs(3599);
    work_view(&service).await;
    assert!(
        !service.owner_todo.state.lock().await.facts_refreshing,
        "59分59秒では応募を読み直さない"
    );
    assert_eq!(applications.load(Ordering::SeqCst), 1);
    service
        .owner_todo
        .state
        .lock()
        .await
        .facts_snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - Duration::from_secs(3600);
    view(&service).await;
    assert!(
        !service.owner_todo.state.lock().await.facts_refreshing,
        "期限切れでも他画面の担当者参照は応募を再取得しない"
    );
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(
        applications.load(Ordering::SeqCst),
        1,
        "期限切れでも要求なしなら通信しない"
    );
    for _ in 0..10 {
        assert!(work_view(&service).await.snapshot.is_some());
    }
    // 応募の更新が止まっていても取引の更新は先に完了できる。
    service
        .owner_todo
        .state
        .lock()
        .await
        .snapshot
        .as_mut()
        .unwrap()
        .0 = Instant::now() - TTL;
    work_view(&service).await;
    settle_deals(&service).await;
    assert_eq!(searches.load(Ordering::SeqCst), 3);
    fail_apps.store(true, Ordering::SeqCst);
    app_gate.add_permits(1);
    settle_apps(&service).await;
    let v = work_view(&service).await;
    assert!(v.failed);
    assert!(!v.partial, "取得失敗でも前回の応募集計を保持する");
    assert_eq!(v.snapshot.unwrap().facts["1"].days.len(), 1);
    assert_eq!(
        applications.load(Ordering::SeqCst),
        2,
        "同時要求と失敗時の連続再試行を抑える"
    );
    assert_eq!(
        service
            .owner_todo
            .state
            .lock()
            .await
            .snapshot
            .as_ref()
            .unwrap()
            .1
            .deals["1"]
            .assignment
            .actionyotei
            .as_deref(),
        Some("2026-10-23")
    );
    server.abort();
}

#[tokio::test(flavor = "multi_thread")]
async fn restored_facts_keep_their_age_and_remain_visible_during_refresh() {
    use axum::routing::any;
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let built = Utc::now() - chrono::Duration::hours(2);
    let facts = BTreeMap::from([(
        "1".into(),
        super::super::todo_signals::Facts {
            live: 2,
            unknown_dates: 0,
            days: vec![today(built)],
            listings: None,
        },
    )]);
    report_snapshot::persist(
        Some(audit.clone()),
        report_snapshot::Kind::Facts,
        Arc::new(facts.clone()),
        built,
    )
    .await;
    let gate = Arc::new(tokio::sync::Semaphore::new(0));
    let blocked = gate.clone();
    let app = Router::new().fallback(any(move || {
        let blocked = blocked.clone();
        async move {
            blocked.acquire().await.unwrap().forget();
            Json(json!({"results":[{"from":{"id":"1"},"to":[]}]}))
        }
    }));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let mut service =
        JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
    service.audit = Some(audit.clone());
    let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
    let snapshot = Snapshot {
        complete: true,
        deals: BTreeMap::from([(
            "1".into(),
            WorkDeal {
                record: Record {
                    id: "1".into(),
                    properties: BTreeMap::from([(
                        "contract_start_date".into(),
                        Some("2026-01-01".into()),
                    )]),
                },
                assignment: Assignment::default(),
            },
        )]),
        ..Snapshot::default()
    };
    service.owner_todo.state.lock().await.snapshot = Some((Instant::now(), Arc::new(snapshot)));
    let view = work_view(&service).await;
    assert!(!view.partial);
    assert!(view.refreshing);
    assert!(view.facts_stale);
    assert_eq!(view.facts_built_at, Some(built));
    assert_eq!(view.snapshot.unwrap().facts, facts);
    gate.add_permits(20);
    tokio::time::timeout(Duration::from_secs(5), async {
        while service.owner_todo.state.lock().await.facts_refreshing {
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    })
    .await
    .unwrap();
    let fresh = work_view(&service).await;
    assert!(!fresh.facts_stale);
    let expected = fresh.snapshot.unwrap().facts.clone();
    assert_eq!(expected["1"].live, 0);
    assert!(expected["1"].days.is_empty());
    let mut restarted = JobReadService::for_test("http://127.0.0.1:1".into());
    restarted.audit = Some(audit);
    report_snapshot::ensure_restored(&restarted).await;
    assert_eq!(
        restarted
            .owner_todo
            .state
            .lock()
            .await
            .facts_snapshot
            .as_ref()
            .unwrap()
            .1,
        expected
    );
    server.abort();
}

#[test]
fn measure_synthetic_facts_500_deals_30000_applications() {
    let day = today(Utc::now());
    let facts: BTreeMap<_, _> = (0..500)
        .map(|i| {
            (
                (i + 800000).to_string(),
                super::super::todo_signals::Facts {
                    live: 82,
                    unknown_dates: 0,
                    days: (0..60).map(|j| day - chrono::Duration::days(j)).collect(),
                    listings: Some(1),
                },
            )
        })
        .collect();
    let (json, gzip, parts, base64) = report_snapshot::sizes(&facts);
    eprintln!("synthetic_todo deals=500 applications=30000 json={json} gzip={gzip} parts={parts} base64={base64}");
    assert_eq!(parts, 1);
}
