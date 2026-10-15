//! Synthetic index fixture and local latency probe. No production credentials or records.
use super::*;
use axum::{body::Body, http::Request, routing::any};
use std::sync::atomic::{AtomicUsize, Ordering};

struct Fixture {
    service: JobReadService,
    calls: Arc<AtomicUsize>,
    server: tokio::task::JoinHandle<()>,
    association_calls: Arc<AtomicUsize>,
    fail_on: Arc<AtomicUsize>,
    fail_status: Arc<AtomicUsize>,
    max_active: Arc<AtomicUsize>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new(total: usize, latency: Duration) -> Self {
        let calls = Arc::new(AtomicUsize::new(0));
        let association_calls = Arc::new(AtomicUsize::new(0));
        let fail_on = Arc::new(AtomicUsize::new(usize::MAX));
        let fail_status = Arc::new(AtomicUsize::new(503));
        let active = Arc::new(AtomicUsize::new(0));
        let max_active = Arc::new(AtomicUsize::new(0));
        let (seen, failed, status, maximum) = (
            association_calls.clone(),
            fail_on.clone(),
            fail_status.clone(),
            max_active.clone(),
        );
        let observed = calls.clone();
        let app = Router::new().fallback(any(move |request: Request<Body>| {
            let calls = observed.clone();
            let (seen, failed, status, active, maximum) = (seen.clone(), failed.clone(), status.clone(), active.clone(), maximum.clone());
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let path = request.uri().path().to_owned();
                let is_association = path.contains("/associations/");
                let nth = if is_association {
                    maximum.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    seen.fetch_add(1, Ordering::SeqCst) + 1
                } else { 0 };
                tokio::time::sleep(latency).await;
                if is_association {
                    active.fetch_sub(1, Ordering::SeqCst);
                    if nth == failed.load(Ordering::SeqCst) {
                        return (StatusCode::from_u16(status.load(Ordering::SeqCst) as u16).unwrap(), Json(json!({}))).into_response();
                    }
                }
                if path == "/crm/v3/objects/0-420" {
                    let start = request.uri().query().unwrap_or("").split('&').find_map(|p|p.strip_prefix("after=")).and_then(|s|s.parse::<usize>().ok()).unwrap_or(0);
                    let end = (start + 100).min(total);
                    let results: Vec<_> = (start+1..=end).map(|id| json!({"id":id.to_string(),"properties":{"id_hrhakkaa":format!("HR-{id}"),"hs_name":"合成求人","kyuujin_status":"公開中"}})).collect();
                    let mut body = json!({"results":results,"total":total});
                    if end < total { body["paging"] = json!({"next":{"after":end.to_string()}}); }
                    return Json(body).into_response();
                }
                let bytes = axum::body::to_bytes(request.into_body(),1_000_000).await.unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                let results: Vec<_> = body["inputs"].as_array().unwrap().iter().map(|v| {
                    let id = v["id"].as_str().unwrap();
                    if path.contains("/associations/") {
                        let targets = if path.contains("/deals/") {vec![json!({"toObjectId":(id.parse::<usize>().unwrap()%205+1).to_string()})]} else {vec![]};
                        json!({"from":{"id":id},"to":targets})
                    } else {
                        json!({"id":id,"properties":{"dealname":"合成案件","pipeline":"21596025"}})
                    }
                }).collect();
                Json(json!({"results":results})).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            service,
            calls,
            server,
            association_calls,
            fail_on,
            fail_status,
            max_active,
        }
    }
}

#[tokio::test]
#[ignore = "41,000-record local latency probe; run explicitly"]
async fn index_build_latency_probe() {
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let f = Fixture::new(41_000, Duration::from_millis(30)).await;
    let started = std::time::Instant::now();
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    let state = f.service.listing_index.state.read().await;
    let index = state.snapshot.as_ref().unwrap();
    assert_eq!(index.records.len(), 41_000);
    assert_eq!(index.deals.len(), 205);
    eprintln!(
        "index_build count={} calls={} elapsed_ms={}",
        index.records.len(),
        f.calls.load(Ordering::SeqCst),
        started.elapsed().as_millis()
    );
}

#[tokio::test]
async fn sixth_association_call_can_fail_once_and_the_index_still_completes() {
    let f = Fixture::new(6_100, Duration::from_millis(1)).await;
    f.fail_on.store(6, Ordering::SeqCst);
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    let state = f.service.listing_index.state.read().await;
    let index = state.snapshot.as_ref().unwrap();
    assert_eq!(index.records.len(), 6_100);
    assert!(index
        .records
        .iter()
        .all(|r| r.deal_ids.len() == 1 && r.indexed_application_count == Some(0)));
    assert_eq!(
        f.association_calls.load(Ordering::SeqCst),
        15,
        "7 deal chunks + 7 application chunks + 1 retry; no restart"
    );
    assert!(!state.refresh_failed);
    assert_eq!(state.consecutive_failures, 0);
}

#[tokio::test]
async fn bad_request_is_not_retried_and_rebuild_backoff_starts_at_one_minute() {
    let f = Fixture::new(200, Duration::ZERO).await;
    f.fail_on.store(1, Ordering::SeqCst);
    f.fail_status.store(400, Ordering::SeqCst);
    let started = std::time::Instant::now();
    assert!(
        refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
            .await
            .is_err()
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(f.association_calls.load(Ordering::SeqCst), 1);
    let mut state = f.service.listing_index.state.write().await;
    assert!(state.snapshot.is_none());
    assert!(state.refresh_failed);
    assert_eq!(state.consecutive_failures, 1);
    tokio::time::pause();
    let remaining = state.progress().retry_after_seconds.unwrap();
    assert!(
        (59..=60).contains(&remaining),
        "the first failed build must schedule its actual retry one minute later"
    );
    tokio::time::advance(Duration::from_secs(60)).await;
    assert!(state.retry_at.unwrap() <= tokio::time::Instant::now());
    for seconds in [120, 300, 600, 600] {
        state.failed();
        assert_eq!(state.progress().retry_after_seconds, Some(seconds));
    }
}

#[tokio::test]
async fn page_wait_is_removed_and_association_concurrency_is_bounded() {
    let f = Fixture::new(2_800, Duration::from_millis(5)).await;
    let started = std::time::Instant::now();
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    assert!(PAGE_PAUSE.is_zero());
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "28 pages must not take 27 extra seconds"
    );
    let max = f.max_active.load(Ordering::SeqCst);
    assert!(
        (2..=4).contains(&max),
        "associations should run concurrently, at most four at once"
    );
    assert_eq!(f.service.listing_index.state.read().await.read_count, 2_800);
}

#[tokio::test]
async fn preparing_progress_is_returned_by_home_and_listings() {
    let f = Fixture::new(100, Duration::ZERO).await;
    {
        let mut state = f.service.listing_index.state.write().await;
        state.read_count = 12_300;
        state.estimated_total = Some(41_000);
    }
    let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    session
        .insert(SESSION_USER_KEY, "reader@example.test")
        .await
        .unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
        .await
        .unwrap();
    let access = Access {
        allowed: BTreeSet::from(["reader@example.test".into()]),
        service: Some(Arc::new(f.service.clone())),
        moc_path: None,
        moc_drive: Ok(None),
        snapshot_reader: None,
        snapshot_cache: Arc::default(),
        resolved_jobs: Arc::default(),
        images: None,
        drive_listings: BTreeSet::new(),
        drive_config_error: None,
    };
    let app = crate::handlers::job_copy_live::integration_tests::moc_state();
    let home = home::read(
        State(app.clone()),
        Extension(access.clone()),
        session.clone(),
    )
    .await
    .unwrap()
    .into_response();
    let listings = read(
        State(app),
        Extension(access),
        session,
        Query(ListingsQuery::default()),
    )
    .await
    .unwrap()
    .into_response();
    for response in [home, listings] {
        let bytes = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let data: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(data["status"], "preparing");
        assert_eq!(
            data["progress"],
            json!({"read_count":12300,"estimated_total":41000,"retry_after_seconds":null})
        );
    }
}
