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
    page_fail_on: Arc<AtomicUsize>,
    page_fail_status: Arc<AtomicUsize>,
    page_calls: Arc<AtomicUsize>,
    call_times: Arc<std::sync::Mutex<Vec<(bool, std::time::Instant)>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.server.abort();
    }
}
impl Fixture {
    async fn new(total: usize, latency: Duration) -> Self {
        Self::with_ids((1..=total as u128).collect(), latency, false, false).await
    }
    async fn with_ids(ids: Vec<u128>, latency: Duration, limited: bool, large_body: bool) -> Self {
        let total = ids.len();
        let ids = Arc::new(ids);
        let page_fail_on = Arc::new(AtomicUsize::new(usize::MAX));
        let page_fail_status = Arc::new(AtomicUsize::new(503));
        let page_calls = Arc::new(AtomicUsize::new(0));
        let call_times = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (page_failed, page_status, pages, times) = (
            page_fail_on.clone(),
            page_fail_status.clone(),
            page_calls.clone(),
            call_times.clone(),
        );
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
            let (ids, page_failed, page_status, pages, times) = (ids.clone(), page_failed.clone(), page_status.clone(), pages.clone(), times.clone());
            let (seen, failed, status, active, maximum) = (seen.clone(), failed.clone(), status.clone(), active.clone(), maximum.clone());
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let path = request.uri().path().to_owned();
                times.lock().unwrap().push((path.ends_with("/search"), std::time::Instant::now()));
                let light = request.uri().query().is_some_and(|q| q.contains("properties=hs_object_id"));
                let is_page = path == "/crm/v3/objects/0-420" && !light;
                let page_nth = if is_page { pages.fetch_add(1, Ordering::SeqCst) + 1 } else { 0 };
                let is_association = path.contains("/associations/");
                let nth = if is_association {
                    maximum.fetch_max(active.fetch_add(1, Ordering::SeqCst) + 1, Ordering::SeqCst);
                    seen.fetch_add(1, Ordering::SeqCst) + 1
                } else { 0 };
                tokio::time::sleep(if is_page {latency} else {latency.min(Duration::from_millis(if large_body { 30 } else { 200 }))}).await;
                if is_page && page_nth == page_failed.load(Ordering::SeqCst) {
                    return (StatusCode::from_u16(page_status.load(Ordering::SeqCst) as u16).unwrap(), Json(json!({}))).into_response();
                }
                if path.ends_with("/search") {
                    return Json(json!({"results":ids.last().map(|id| vec![json!({"id":id.to_string(),"properties":{}})]).unwrap_or_default(),"total":total})).into_response();
                }
                if is_association {
                    active.fetch_sub(1, Ordering::SeqCst);
                    if nth == failed.load(Ordering::SeqCst) {
                        return (StatusCode::from_u16(status.load(Ordering::SeqCst) as u16).unwrap(), Json(json!({}))).into_response();
                    }
                }
                if path == "/crm/v3/objects/0-420" {
                    let after = request.uri().query().unwrap_or("").split('&').find_map(|p|p.strip_prefix("after=")).and_then(|s|s.parse::<u128>().ok());
                    // Real 0-420 behavior verified 2026-10-15: after includes its ID.
                    let start = after.map(|a| ids.partition_point(|&id| id < a)).unwrap_or(0);
                    let end = (start + 100).min(total);
                    let results: Vec<_> = ids[start..end].iter().map(|id| {
                        if light { json!({"id":id.to_string(),"properties":{"hs_object_id":id.to_string()}}) }
                        else { json!({"id":id.to_string(),"properties":{"id_hrhakkaa":format!("HR-{id}"),"hs_name":"合成求人","kyuujin_status":"公開中","hrh_kyuujinhyou_honbun":if large_body {"合".repeat(5300)} else {String::new()}}}) }
                    }).collect();
                    let mut body = json!({"results":results,"total":total});
                    // next.after is the last ID + 1, even when the next ID is far away.
                    if end < total { body["paging"] = json!({"next":{"after":(ids[end-1] + 1).to_string()}}); }
                    return Json(body).into_response();
                }
                let bytes = axum::body::to_bytes(request.into_body(),1_000_000).await.unwrap();
                let body: Value = serde_json::from_slice(&bytes).unwrap();
                let results: Vec<_> = body["inputs"].as_array().unwrap().iter().map(|v| {
                    let id = v["id"].as_str().unwrap();
                    if path.contains("/associations/") {
                        let targets = if path.contains("/deals/") {vec![json!({"toObjectId":(id.parse::<u128>().unwrap()%205+1).to_string()})]} else {vec![]};
                        json!({"from":{"id":id},"to":targets})
                    } else {
                        json!({"id":id,"properties":{"dealname":"合成案件","pipeline":"21596025"}})
                    }
                }).collect();
                Json(json!({"results":results})).into_response()
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let service = if limited {
            JobReadService::from_client(
                crate::hubspot::HubSpotClient::with_gateway(
                    "synthetic".into(),
                    &base,
                    JobReadService::options(),
                    Arc::new(crate::hubspot::gateway::Gateway::new(
                        crate::hubspot::gateway::GatewayConfig::default(),
                    )),
                )
                .unwrap(),
            )
        } else {
            JobReadService::for_test(base)
        };
        let server = tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Self {
            service,
            calls,
            server,
            association_calls,
            fail_on,
            fail_status,
            max_active,
            page_fail_on,
            page_fail_status,
            page_calls,
            call_times,
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
    let ids = production_like_ids();
    let f = Fixture::with_ids(ids.clone(), Duration::from_millis(1200), true, true).await;
    let started = std::time::Instant::now();
    // Read the same pages serially through the same production-configured gateway.
    let serial = build_index(
        &f.service.listing_index,
        &f.service,
        vec![],
        Duration::ZERO,
        1,
    )
    .await
    .unwrap();
    assert_eq!(serial.records.len(), 41_000);
    drop(serial);
    let serial_ms = started.elapsed().as_millis();
    parallel_probe(&f, &ids, serial_ms).await;
}
#[tokio::test]
#[ignore = "41,000-record parallel-only latency probe; run explicitly"]
async fn index_build_parallel_latency_probe() {
    let subscriber = tracing_subscriber::fmt()
        .with_ansi(false)
        .without_time()
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    let ids = production_like_ids();
    let f = Fixture::with_ids(ids.clone(), Duration::from_millis(1200), true, true).await;
    parallel_probe(&f, &ids, 0).await;
}
async fn parallel_probe(f: &Fixture, ids: &[u128], serial_ms: u128) {
    f.call_times.lock().unwrap().clear();
    for mode in ["cold", "warm"] {
        let started = std::time::Instant::now();
        refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
            .await
            .unwrap();
        let elapsed = started.elapsed();
        let state = f.service.listing_index.state.read().await;
        let index = state.snapshot.as_ref().unwrap();
        assert_eq!(index.records.len(), 41_000);
        assert_eq!(index.deals.len(), 205);
        assert_eq!(state.read_count, 41_000);
        assert_eq!(
            index
                .records
                .iter()
                .map(|r| numeric_id(&r.id).unwrap())
                .collect::<BTreeSet<_>>(),
            ids.iter().copied().collect()
        );
        assert!(elapsed < Duration::from_secs(90), "{mode}: {elapsed:?}");
        let (max_second, max_ten) = assert_windows(f);
        eprintln!("index_parallel mode={mode} records=41000 serial_ms={serial_ms} elapsed_ms={} max_calls_1s={max_second} max_calls_10s={max_ten}", elapsed.as_millis());
    }
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
        17,
        "8 range chunks per target + 1 retry; no restart"
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
    assert!(started.elapsed() < Duration::from_secs(3));
    assert!(f.association_calls.load(Ordering::SeqCst) <= 4);
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
    let f = Fixture::new(2_800, Duration::from_millis(200)).await;
    let started = std::time::Instant::now();
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    assert!(PAGE_PAUSE.is_zero());
    assert!(
        started.elapsed() < Duration::from_secs(15),
        "per-range batches and paced starts must not add the old 27-second page wait"
    );
    // A small range may need only one batch. Exercise a full four-way group too.
    let ids: Vec<_> = (1..=2800).map(|id| id.to_string()).collect();
    index_associations(&IndexReader::new(&f.service.hs.background()), &ids, "deals")
        .await
        .unwrap();
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

fn production_like_ids() -> Vec<u128> {
    // Increasing, deliberately uneven gaps over 557e9..597e9; no production records.
    (0..41_000u128)
        .map(|i| {
            557_000_000_000 + 40_000_000_000 * (4 * i * 40_999 + i * i) / (5 * 40_999 * 40_999)
        })
        .collect()
}
fn assert_windows(f: &Fixture) -> (usize, usize) {
    let times = f.call_times.lock().unwrap();
    let times: Vec<_> = times
        .iter()
        .filter(|(search, _)| !search)
        .map(|(_, time)| *time)
        .collect();
    let max = |window| {
        times
            .iter()
            .enumerate()
            .map(|(i, t)| {
                times[i..]
                    .iter()
                    .take_while(|next| next.duration_since(*t) < window)
                    .count()
            })
            .max()
            .unwrap_or(0)
    };
    let (second, ten) = (max(Duration::from_secs(1)), max(Duration::from_secs(10)));
    assert!(second <= 8, "maximum calls in 1s: {second}");
    assert!(ten <= 80, "maximum calls in 10s: {ten}");
    (second, ten)
}
#[tokio::test]
async fn half_open_boundaries_empty_and_single_record_ranges() {
    let f = Fixture::with_ids(vec![10, 20, 21, 40, 99], Duration::ZERO, false, false).await;
    let ranges = ranges_from_cuts(vec![10, 20, 21, 22, 30, 40, 100]);
    let mut all = BTreeSet::new();
    let mut counts = vec![];
    for (i, range) in ranges.into_iter().enumerate() {
        let (ids, _) = read_index_range(
            &f.service.listing_index,
            &IndexReader::new(&f.service.hs.background()),
            range,
            i,
            Duration::ZERO,
        )
        .await
        .unwrap();
        counts.push(ids.len());
        for id in ids {
            assert!(all.insert(id));
        }
    }
    assert_eq!(counts, vec![0, 1, 1, 1, 0, 0, 2, 0]);
    assert_eq!(
        all,
        [10, 20, 21, 40, 99]
            .map(|n| n.to_string())
            .into_iter()
            .collect()
    );
    assert_eq!(f.service.listing_index.state.read().await.read_count, 5);
}
#[tokio::test]
async fn failed_range_preserves_snapshot_and_one_transient_page_retries_locally() {
    let f = Fixture::new(2100, Duration::from_millis(1)).await;
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    let old = f
        .service
        .listing_index
        .state
        .read()
        .await
        .snapshot
        .clone()
        .unwrap();
    let calls = f.page_calls.load(Ordering::SeqCst);
    f.page_fail_on.store(calls + 2, Ordering::SeqCst);
    f.page_fail_status.store(400, Ordering::SeqCst);
    assert!(
        refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
            .await
            .is_err()
    );
    assert!(Arc::ptr_eq(
        &old,
        f.service
            .listing_index
            .state
            .read()
            .await
            .snapshot
            .as_ref()
            .unwrap()
    ));
    // Drain cancellation before choosing the next call to fail.
    tokio::task::yield_now().await;
    f.page_fail_on
        .store(f.page_calls.load(Ordering::SeqCst) + 2, Ordering::SeqCst);
    f.page_fail_status.store(503, Ordering::SeqCst);
    let before = f.page_calls.load(Ordering::SeqCst);
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    let retried = f.page_calls.load(Ordering::SeqCst) - before;
    f.page_fail_on.store(usize::MAX, Ordering::SeqCst);
    let before = f.page_calls.load(Ordering::SeqCst);
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    assert_eq!(retried, f.page_calls.load(Ordering::SeqCst) - before + 1);
}
#[tokio::test]
async fn shared_gateway_windows_hold_for_concurrent_pages_and_batches() {
    let f = Fixture::with_ids((1..=1200).collect(), Duration::from_millis(5), true, false).await;
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    assert_windows(&f);
    assert!(f.service.hs.background().last_rate_limit().is_none());
}

#[tokio::test]
async fn previous_index_quantiles_keep_new_low_and_high_ids_and_sparse_gaps() {
    let f = Fixture::with_ids(vec![10, 20, 21, 40, 99], Duration::ZERO, false, false).await;
    let rows = [20, 40].map(|id| {
        listing(
            &serde_json::from_value(json!({
                "id":id.to_string(), "properties":{"id_hrhakkaa":"synthetic"}
            }))
            .unwrap(),
        )
        .unwrap()
    });
    f.service.listing_index.state.write().await.snapshot = Some(Arc::new(Index {
        records: rows.to_vec(),
        titles: vec![],
        built_at: String::new(),
        deals: vec![],
    }));
    let reader = IndexReader::new(&f.service.hs.background());
    let ranges = index_ranges(&f.service.listing_index, &reader, 8)
        .await
        .unwrap();
    assert_eq!(
        ranges
            .iter()
            .map(|r| (r.lower, r.upper))
            .collect::<Vec<_>>(),
        vec![(0, Some(20)), (20, Some(40)), (40, None)]
    );
    assert_eq!(
        f.calls.load(Ordering::SeqCst),
        0,
        "previous distribution needs no probe"
    );
    refresh(&f.service.listing_index, &f.service, vec![], PAGE_PAUSE)
        .await
        .unwrap();
    let state = f.service.listing_index.state.read().await;
    assert_eq!(state.read_count, 5);
    assert_eq!(
        state
            .snapshot
            .as_ref()
            .unwrap()
            .records
            .iter()
            .map(|r| r.id.clone())
            .collect::<BTreeSet<_>>(),
        [10, 20, 21, 40, 99]
            .map(|n| n.to_string())
            .into_iter()
            .collect()
    );
}

#[tokio::test]
async fn inclusive_fixture_and_serial_paging_preserve_sparse_ids() {
    let ids: Vec<u128> = (0..203).map(|i| 557_000_000_000 + i * 17).collect();
    let f = Fixture::with_ids(ids.clone(), Duration::ZERO, false, false).await;
    let client = f.service.hs.background();
    let first = client
        .read_raw(
            false,
            "/crm/v3/objects/0-420",
            &[
                ("after", ids[0].to_string()),
                ("limit", "100".into()),
                ("properties", "hs_object_id".into()),
            ],
            None,
        )
        .await
        .unwrap()
        .body
        .unwrap();
    assert_eq!(first["results"][0]["id"], ids[0].to_string());
    assert_eq!(first["paging"]["next"]["after"], (ids[99] + 1).to_string());
    assert_ne!(ids[99] + 1, ids[100]);
    let index = build_index(
        &f.service.listing_index,
        &f.service,
        vec![],
        Duration::ZERO,
        1,
    )
    .await
    .unwrap();
    assert_eq!(
        index
            .records
            .iter()
            .map(|r| numeric_id(&r.id).unwrap())
            .collect::<BTreeSet<_>>(),
        ids.into_iter().collect()
    );
    assert_eq!(f.page_calls.load(Ordering::SeqCst), 3);
    assert_eq!(f.service.listing_index.state.read().await.read_count, 203);
}

#[tokio::test]
async fn next_cursor_must_exceed_last_id_but_need_not_equal_last_plus_one() {
    // Synthetic gap cursors also guard against accidentally requiring exact +1.
    for (next, second_next, valid) in [
        (10, 21, false),
        (9, 21, false),
        (0, 21, false),
        (15, 21, true),
        (15, 15, false),
        (15, 19, false),
    ] {
        let observed = Arc::new(std::sync::Mutex::new(Vec::new()));
        let seen = observed.clone();
        let app = Router::new().fallback(any(move |request: Request<Body>| {
            let seen = seen.clone();
            async move {
                let after = request.uri().query().unwrap().split('&')
                    .find_map(|p| p.strip_prefix("after=")).unwrap().parse::<u128>().unwrap();
                seen.lock().unwrap().push(after);
                let record = |id: u128| json!({"id": id.to_string(), "properties":{"id_hrhakkaa":"synthetic"}});
                Json(if after == 10 {
                    json!({"results":[record(10)], "paging":{"next":{"after":next.to_string()}}})
                } else if after == 15 {
                    json!({"results":[record(20)], "paging":{"next":{"after":second_next.to_string()}}})
                } else {
                    assert_eq!(after, 21);
                    json!({"results":[record(30)]})
                })
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service =
            JobReadService::for_test(format!("http://{}", listener.local_addr().unwrap()));
        let server = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let result = read_index_range(
            &service.listing_index,
            &IndexReader::new(&service.hs.background()),
            IdRange {
                lower: 10,
                upper: None,
            },
            0,
            Duration::ZERO,
        )
        .await;
        if valid {
            assert_eq!(
                result.unwrap().0,
                [10, 20, 30].map(|id| id.to_string()).into_iter().collect()
            );
            assert_eq!(*observed.lock().unwrap(), vec![10, 15, 21]);
        } else {
            assert_eq!(result.err().unwrap().1, "listing_paging_invalid");
            assert_eq!(
                *observed.lock().unwrap(),
                if next == 15 { vec![10, 15] } else { vec![10] }
            );
        }
        server.abort();
    }
}
