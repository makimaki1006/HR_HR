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
                    return Json(json!({"results":[]})).into_response();
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
                    count: 1,
                })
                .collect(),
            titles: vec![],
            built_at: "synthetic-build".into(),
        });
        service.listing_index.state.write().await.snapshot = Some(index.clone());
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
        read(
            State(crate::handlers::job_copy_live::integration_tests::moc_state()),
            Extension(access),
            session,
        )
        .await
        .map(IntoResponse::into_response)
    }
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
        assert_eq!(data["home"]["cards"][5]["count"], 10_000);
        assert_eq!(
            data["home"]["cards"][5]["listings"]
                .as_array()
                .unwrap()
                .len(),
            LIST_CAP
        );
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
    assert_eq!(data["home"]["cards"][5]["count"], 9999);
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
    assert_eq!(data["home"]["cards"][5]["count"], 10_000);
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
