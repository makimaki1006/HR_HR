//! Synthetic HubSpot only. Counters and timing output contain no record data.
use super::*;
use axum::{body::Body, http::Request, routing::any};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

struct Fixture {
    service: JobReadService,
    index: Arc<Index>,
    calls: Arc<AtomicUsize>,
    blocked: Arc<AtomicBool>,
    fails: Arc<AtomicBool>,
    gate: Arc<tokio::sync::Semaphore>,
    server: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let blocked = Arc::new(AtomicBool::new(false));
        let fails = Arc::new(AtomicBool::new(false));
        let gate = Arc::new(tokio::sync::Semaphore::new(0));
        let (observed, block, fail, wait) =
            (calls.clone(), blocked.clone(), fails.clone(), gate.clone());
        let app = Router::new().fallback(any(move |req: Request<Body>| {
            let (calls, blocked, fails, gate) =
                (observed.clone(), block.clone(), fail.clone(), wait.clone());
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                if blocked.load(Ordering::SeqCst) {
                    gate.acquire().await.unwrap().forget();
                }
                tokio::time::sleep(Duration::from_millis(30)).await;
                if fails.load(Ordering::SeqCst) {
                    return (StatusCode::FORBIDDEN, Json(json!({}))).into_response();
                }
                let path = req.uri().path().to_owned();
                if path.contains("/pipelines/") {
                    return Json(json!({"results":[{"id":"21596025","label":"合成・納品管理","stages":[
                        {"id":"stage-demo","label":"合成・契約中","metadata":{"isClosed":"false"}},
                        {"id":"66848546","label":"継続済","metadata":{"isClosed":"true"}}
                    ]}]})).into_response();
                }
                if path == "/crm/v3/objects/deals/1" {
                    assert_eq!(req.method(), axum::http::Method::GET);
                    return Json(json!({"id":"1","properties":{"pipeline":"21596025","dealstage":"66848546"}})).into_response();
                }
                let bytes = axum::body::to_bytes(req.into_body(), 1_000_000)
                    .await
                    .unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                let results: Vec<Value> = body["inputs"].as_array().unwrap().iter().map(|input| {
                    let id = input["id"].as_str().unwrap();
                    if path.contains("/associations/") {
                        json!({"from":{"id":id},"to":[{"toObjectId":id.parse::<u64>().unwrap()}]})
                    } else {
                        json!({"id":id,"properties":{"name":"合成事業所"}})
                    }
                }).collect();
                Json(json!({"results":results})).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let index = Arc::new(Index {
            records: (1..=10_000)
                .map(|n| {
                    tests::listing(
                        &n.to_string(),
                        &[&((n % 205) + 1).to_string()],
                        "掲載中",
                        "unset",
                        None,
                    )
                })
                .collect(),
            deals: (1..=205)
                .map(|n| Deal {
                    id: n.to_string(),
                    name: None,
                    pipeline: None,
                    stage: None,
                    created_at: None,
                    assignment: Default::default(),
                    count: 1,
                })
                .collect(),
            titles: vec![],
            built_at: "synthetic-build".into(),
        });
        service.listing_index.state.write().await.snapshot = Some(index.clone());
        owner_todo::seed_empty(&service).await;
        Self {
            service,
            index,
            calls,
            blocked,
            fails,
            gate,
            server,
        }
    }
    async fn expire(&self) {
        let mut state = self.service.listing_index.home.companies.lock().await;
        let at = Some(StdInstant::now() - COMPANY_TTL);
        Arc::make_mut(state.map.as_mut().unwrap()).at = at;
        // Simulate elapsed wall time, not a new successful company-map generation.
        if let Some(cached) = self
            .service
            .listing_index
            .home
            .response
            .lock()
            .await
            .as_mut()
        {
            cached.company_at = at;
        }
    }
    async fn settled(&self) {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let changed = self.service.listing_index.home.changed.notified();
                tokio::pin!(changed);
                changed.as_mut().enable();
                if !self
                    .service
                    .listing_index
                    .home
                    .companies
                    .lock()
                    .await
                    .refreshing
                {
                    break;
                }
                changed.await;
            }
        })
        .await
        .expect("background read completes");
    }
    async fn api(&self, signed_in: bool) -> Result<axum::response::Response, ReadError> {
        self.invoke(signed_in, false).await
    }
    async fn invoke(
        &self,
        signed_in: bool,
        refresh: bool,
    ) -> Result<axum::response::Response, ReadError> {
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        if signed_in {
            session
                .insert(SESSION_USER_KEY, "reader@example.test")
                .await
                .unwrap();
            session
                .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
                .await
                .unwrap();
        }
        let access = Access {
            allowed: BTreeSet::from(["reader@example.test".into()]),
            service: Some(Arc::new(self.service.clone())),
            moc_path: None,
            moc_drive: Ok(None),
            snapshot_reader: None,
            snapshot_cache: Arc::default(),
            resolved_jobs: Arc::default(),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
        };
        let state = State(crate::handlers::job_copy_live::integration_tests::moc_state());
        if refresh {
            refresh_deal(state, Extension(access), session, AxumPath("1".into()))
                .await
                .map(IntoResponse::into_response)
        } else {
            read(
                state,
                Extension(access),
                session,
                Query(owner_todo::OwnerQuery {
                    owner: Some("all".into()),
                    ..Default::default()
                }),
            )
            .await
            .map(IntoResponse::into_response)
        }
    }
}
fn assert_home_cards(data: &Value) {
    let cards = data["home"]["cards"].as_array().unwrap();
    assert_eq!(cards.len(), 7);
    assert!(cards.iter().all(|card| card["key"] != "occupation"));
}

async fn json_body(response: axum::response::Response) -> Value {
    assert_eq!(
        response.headers()[header::CACHE_CONTROL],
        "private, no-store"
    );
    assert_eq!(response.headers()[header::CONTENT_TYPE], "application/json");
    let body = axum::body::to_bytes(response.into_body(), 1_000_000)
        .await
        .unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn home_speed_probe() {
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let f = Fixture::new().await;
    let today = tests::date("2026-10-15");
    for phase in ["cold", "warm", "expired"] {
        if phase == "expired" {
            f.expire().await;
        }
        let before = f.calls.load(Ordering::SeqCst);
        let start = StdInstant::now();
        let body = answer(&f.service, f.index.clone(), today).await;
        eprintln!(
            "home_speed phase={phase} total_ms={:.3} calls={} bytes={}",
            start.elapsed().as_secs_f64() * 1000.0,
            f.calls.load(Ordering::SeqCst) - before,
            body.len()
        );
        let data: Value = serde_json::from_slice(&body).unwrap();
        assert_home_cards(&data);
        let total: u64 = data["home"]["customers"]
            .as_array()
            .unwrap()
            .iter()
            .map(|customer| customer["live_listings"].as_u64().unwrap())
            .sum();
        assert_eq!(total, 10_000);
        assert_eq!(data["home"]["customers"].as_array().unwrap().len(), 205);
        if phase == "warm" {
            assert_eq!(f.calls.load(Ordering::SeqCst), before);
        }
    }
    f.settled().await;
}

/// API boundary: authorized warm reads make no HubSpot calls; cached content cannot bypass login.
#[tokio::test]
async fn warm_api_uses_no_hubspot_and_still_authorizes() {
    let f = Fixture::new().await;
    let first = json_body(f.api(true).await.unwrap()).await;
    assert_eq!(first["companies_ok"], true);
    let calls = f.calls.load(Ordering::SeqCst);
    assert_eq!(calls, 7);
    assert_eq!(json_body(f.api(true).await.unwrap()).await, first);
    assert_eq!(f.calls.load(Ordering::SeqCst), calls);
    assert!(f.api(false).await.is_err());
    assert_eq!(f.calls.load(Ordering::SeqCst), calls);
    let state = &f.service.listing_index.state;
    state.write().await.refreshing = true;
    assert_eq!(json_body(f.api(true).await.unwrap()).await, first);
    state.write().await.refresh_failed = true;
    assert_eq!(json_body(f.api(true).await.unwrap()).await, first);
    assert_eq!(f.calls.load(Ordering::SeqCst), calls);
    state.write().await.snapshot = None;
    assert_eq!(
        json_body(f.api(true).await.unwrap()).await["status"],
        "preparing"
    );
}

#[tokio::test]
async fn expired_map_returns_immediately_and_refreshes_once_for_concurrent_requests() {
    let f = Fixture::new().await;
    let today = tests::date("2026-10-15");
    answer(&f.service, f.index.clone(), today).await;
    f.expire().await;
    f.blocked.store(true, Ordering::SeqCst);
    let before = f.calls.load(Ordering::SeqCst);
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..20 {
        let (service, index) = (f.service.clone(), f.index.clone());
        requests.spawn(async move { answer(&service, index, today).await });
    }
    let answers = tokio::time::timeout(Duration::from_millis(500), requests.join_all())
        .await
        .expect("requests must not wait for blocked HubSpot");
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(
        serde_json::from_slice::<Value>(&answers[0]).unwrap()["home"]["customers"]
            .as_array()
            .unwrap()
            .len(),
        205
    );
    assert!(
        f.service
            .listing_index
            .home
            .companies
            .lock()
            .await
            .refreshing
    );
    f.blocked.store(false, Ordering::SeqCst);
    f.gate.add_permits(10);
    f.settled().await;
    assert_eq!(
        f.calls.load(Ordering::SeqCst) - before,
        6,
        "one complete refresh, not one per request"
    );
    assert_eq!(
        f.service.hs.background().priority(),
        crate::hubspot::gateway::Priority::Background
    );
}

#[tokio::test]
async fn response_invalidates_for_jst_date_local_edits_index_and_company_rebuilds() {
    let f = Fixture::new().await;
    let today = tests::date("2026-10-15");
    let first = answer(&f.service, f.index.clone(), today).await;
    let again = answer(&f.service, f.index.clone(), today).await;
    assert_eq!(first.as_ptr(), again.as_ptr(), "encoded bytes are reused");
    let next = answer(
        &f.service,
        f.index.clone(),
        today + chrono::Duration::days(1),
    )
    .await;
    assert_ne!(first, next);
    update_indeed_title(&f.service, "1", "合成職種").await;
    let edited = f
        .service
        .listing_index
        .state
        .read()
        .await
        .snapshot
        .clone()
        .unwrap();
    let changed = answer(&f.service, edited, today).await;
    let data: Value = serde_json::from_slice(&changed).unwrap();
    assert_home_cards(&data);
    assert_eq!(
        data["home"],
        serde_json::from_slice::<Value>(&first).unwrap()["home"]
    );
    assert_ne!(
        first.as_ptr(),
        changed.as_ptr(),
        "local edits rebuild the response"
    );
    let calls = f.calls.load(Ordering::SeqCst);
    assert_eq!(calls, 7, "local edit reuses company map");
    let mut rebuilt = (*f.index).clone();
    rebuilt.built_at = "synthetic-rebuild".into();
    let rebuilt = Arc::new(rebuilt);
    prefetch(&f.service, &rebuilt).await;
    f.settled().await;
    let refreshed = answer(&f.service, rebuilt, today).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&refreshed).unwrap()["index_built_at"],
        "synthetic-rebuild"
    );
    assert_eq!(f.calls.load(Ordering::SeqCst) - calls, 6);
}

#[tokio::test]
async fn failed_initial_read_keeps_cards_empty_table_and_retries_are_bounded() {
    let f = Fixture::new().await;
    f.fails.store(true, Ordering::SeqCst);
    let data = json_body(f.api(true).await.unwrap()).await;
    assert_eq!(data["status"], "ready");
    assert_eq!(data["companies_ok"], false);
    assert_home_cards(&data);
    assert_eq!(data["home"]["live_without_company"], 10_000);
    assert_eq!(data["home"]["customers"], json!([]));
    let before = f.calls.load(Ordering::SeqCst);
    assert_eq!(json_body(f.api(true).await.unwrap()).await, data);
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn failed_background_read_preserves_last_successful_table() {
    let f = Fixture::new().await;
    let first = json_body(f.api(true).await.unwrap()).await;
    f.expire().await;
    f.fails.store(true, Ordering::SeqCst);
    assert_eq!(json_body(f.api(true).await.unwrap()).await, first);
    f.settled().await;
    let before = f.calls.load(Ordering::SeqCst);
    assert_eq!(json_body(f.api(true).await.unwrap()).await, first);
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn cold_requests_share_prefetch_even_if_a_waiter_is_cancelled() {
    let f = Fixture::new().await;
    f.blocked.store(true, Ordering::SeqCst);
    prefetch(&f.service, &f.index).await;
    let today = tests::date("2026-10-15");
    let (service, index) = (f.service.clone(), f.index.clone());
    let cancelled = tokio::spawn(async move { answer(&service, index, today).await });
    tokio::task::yield_now().await;
    cancelled.abort();
    let mut requests = tokio::task::JoinSet::new();
    for _ in 0..10 {
        let (service, index) = (f.service.clone(), f.index.clone());
        requests.spawn(async move { answer(&service, index, today).await });
    }
    f.blocked.store(false, Ordering::SeqCst);
    f.gate.add_permits(10);
    let answers = tokio::time::timeout(Duration::from_secs(5), requests.join_all())
        .await
        .unwrap();
    assert!(answers.windows(2).all(|pair| pair[0] == pair[1]));
    assert_eq!(f.calls.load(Ordering::SeqCst), 7);
    let before = f.calls.load(Ordering::SeqCst);
    answer(&f.service, f.index.clone(), today).await;
    assert_eq!(f.calls.load(Ordering::SeqCst), before);
}

#[tokio::test]
async fn cached_and_background_answers_keep_all_five_trend_buckets() {
    let mut f = Fixture::new().await;
    let examples = [
        (0, 0, "zero_stay"),
        (1, 0, "to_zero"),
        (5, 0, "to_zero"),
        (2, 1, "less"),
        (3, 2, "less"),
        (5, 4, "less"),
        (10, 8, "less"),
        (10, 9, "flat"),
        (0, 1, "more"),
        (4, 5, "more"),
        (10, 11, "flat"),
        (10, 12, "more"),
        (5, 5, "flat"),
    ];
    let today = tests::date("2026-10-15");
    let index = Arc::make_mut(&mut f.index);
    for (row, &(prev, recent, _)) in index.records.iter_mut().zip(&examples) {
        row.perf = Some(PerfDigest {
            comparison_recent: None,
            latest_end: today,
            prev_apps: prev,
            recent_apps: recent,
            recent_cost: 1000,
        });
    }
    let verify = |body: &[u8]| {
        let data: Value = serde_json::from_slice(body).unwrap();
        assert_eq!(data["rules"]["trend_min_diff"], 2);
        assert_eq!(data["rules"]["trend_min_percent"], 20);
        let cards = data["home"]["cards"].as_array().unwrap();
        assert_eq!(
            cards
                .iter()
                .map(|c| c["key"].as_str().unwrap())
                .collect::<Vec<_>>(),
            [
                "zero_stay",
                "to_zero",
                "less",
                "more",
                "flat",
                "costly",
                "stale"
            ]
        );
        for (row, &(prev, recent, expected)) in f.index.records.iter().zip(&examples) {
            let picked: Vec<_> = cards
                .iter()
                .take(5)
                .filter(|c| {
                    c["listings"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|r| r["id"] == row.id)
                })
                .collect();
            assert_eq!(picked.len(), 1, "{prev} → {recent}");
            assert_eq!(picked[0]["key"], expected);
        }
    };
    let first = answer(&f.service, f.index.clone(), today).await;
    verify(&first);
    let calls = f.calls.load(Ordering::SeqCst);
    let warm = answer(&f.service, f.index.clone(), today).await;
    assert_eq!(warm.as_ptr(), first.as_ptr(), "reuse the encoded response");
    assert_eq!(f.calls.load(Ordering::SeqCst), calls);
    verify(&warm);
    f.expire().await;
    f.blocked.store(true, Ordering::SeqCst);
    let during_refresh = tokio::time::timeout(
        Duration::from_millis(500),
        answer(&f.service, f.index.clone(), today),
    )
    .await
    .expect("return cached cards while HubSpot is blocked");
    verify(&during_refresh);
    f.blocked.store(false, Ordering::SeqCst);
    f.gate.add_permits(10);
    f.settled().await;
    verify(&answer(&f.service, f.index.clone(), today).await);
}

#[tokio::test]
async fn stage_refresh_reads_authorized_source_and_updates_index_home_and_cards() {
    let mut fixture = Fixture::new().await;
    let index = Arc::make_mut(&mut fixture.index);
    index.records.truncate(1);
    index.records[0].deal_ids = vec!["1".into()];
    index.deals.truncate(1);
    index.deals[0].pipeline = Some("21596025".into());
    index.deals[0].stage = Some("stage-demo".into());
    fixture.service.listing_index.state.write().await.snapshot = Some(fixture.index.clone());
    let before = json_body(fixture.api(true).await.unwrap()).await;
    assert_eq!(
        before["home"]["customers"][0]["latest_deal"]["stage_label"],
        "合成・契約中"
    );
    assert_eq!(before["home"]["customers"][0]["active_deals"], 1);
    let calls = fixture.calls.load(Ordering::SeqCst);
    assert!(fixture.invoke(false, true).await.is_err());
    assert_eq!(fixture.calls.load(Ordering::SeqCst), calls);
    assert_eq!(
        json_body(fixture.invoke(true, true).await.unwrap()).await["status"],
        "refreshed"
    );
    let snapshot = fixture
        .service
        .listing_index
        .state
        .read()
        .await
        .snapshot
        .clone()
        .unwrap();
    assert_eq!(snapshot.deals[0].stage.as_deref(), Some("66848546"));
    let after = json_body(fixture.api(true).await.unwrap()).await;
    assert_eq!(
        after["home"]["customers"][0]["latest_deal"]["stage_label"],
        "継続済"
    );
    assert_eq!(after["home"]["customers"][0]["active_deals"], 0);
    // 「まず見るもの」の件数も同じ更新済みのホーム応答から取得する。
    assert_home_cards(&after);
    assert_eq!(after["home"]["cards"], before["home"]["cards"]);
}
