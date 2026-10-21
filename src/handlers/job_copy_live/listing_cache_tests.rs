//! The detail screen's server-side reuse: one HubSpot read per listing per minute, the customer
//! name in its own request, authorization on every request, and nothing stale after a write.
use super::*;
use axum::{body::to_bytes, body::Body, http::Request, routing::any};
use std::collections::BTreeMap as Counts;
use std::sync::atomic::{AtomicBool, Ordering};

struct Fixture {
    state: Arc<AppState>,
    access: Access,
    session: Session,
    paths: Arc<std::sync::Mutex<Vec<String>>>,
    company_fails: Arc<AtomicBool>,
    _db: tempfile::NamedTempFile,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Fixture {
    async fn new() -> Self {
        let paths = Arc::new(std::sync::Mutex::new(Vec::<String>::new()));
        let company_fails = Arc::new(AtomicBool::new(false));
        let (seen, fails) = (paths.clone(), company_fails.clone());
        let upstream = Router::new().fallback(any(move |request: Request<Body>| {
            let (seen, fails) = (seen.clone(), fails.clone());
            async move {
                let path = request.uri().path().to_owned();
                seen.lock().unwrap().push(path.clone());
                match path.as_str() {
                    "/crm/v3/objects/0-420/30" => Json(json!({"id":"30","properties":{
                        "id_hrhakkaa":"synthetic-30","hs_name":"架空の求人",
                        "hrh_kyuujinhyou_honbun":"仕事内容：棚の品物を確認します。"
                    }}))
                    .into_response(),
                    "/crm/v4/objects/0-420/30/associations/deals" => {
                        if fails.load(Ordering::SeqCst) {
                            return (StatusCode::FORBIDDEN, Json(json!({}))).into_response();
                        }
                        Json(json!({"results":[{"toObjectId":"20"}]})).into_response()
                    }
                    "/crm/v4/objects/0-420/31/associations/deals" => {
                        Json(json!({"results":[{"toObjectId":"21"},{"toObjectId":"22"}]}))
                            .into_response()
                    }
                    "/crm/v4/objects/0-420/32/associations/deals" => {
                        Json(json!({"results":[]})).into_response()
                    }
                    "/crm/v4/objects/0-420/33/associations/deals" => {
                        Json(json!({"results":[{"toObjectId":"23"}]})).into_response()
                    }
                    "/crm/v4/associations/0-420/deals/batch/read" => {
                        let rows = batch_inputs(request).await;
                        let results: Vec<Value> = rows
                            .iter()
                            .map(|id| {
                                let to = match id.as_str() {
                                    "30" => vec!["20"],
                                    "31" => vec!["21", "22"],
                                    "33" => vec!["23"],
                                    _ => vec![],
                                };
                                json!({"from":{"id":id},"to":to.iter().map(|t| json!({"toObjectId":t})).collect::<Vec<_>>()})
                            })
                            .collect();
                        Json(json!({"results":results})).into_response()
                    }
                    "/crm/v4/associations/deals/companies/batch/read" => {
                        let rows = batch_inputs(request).await;
                        let results: Vec<Value> = rows
                            .iter()
                            .map(|id| {
                                let company = match id.as_str() {
                                    "20" | "21" => "10",
                                    _ => "11",
                                };
                                json!({"from":{"id":id},"to":[{"toObjectId":company}]})
                            })
                            .collect();
                        Json(json!({"results":results})).into_response()
                    }
                    "/crm/v3/objects/companies/batch/read" => {
                        let rows = batch_inputs(request).await;
                        let results: Vec<Value> = rows
                            .iter()
                            .map(|id| {
                                let name = if id == "10" { "合成運輸株式会社" } else { "別の合成株式会社" };
                                json!({"id":id,"properties":{"name":name}})
                            })
                            .collect();
                        Json(json!({"results":results})).into_response()
                    }
                    other => panic!("unexpected read: {other}"),
                }
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let service = Arc::new(JobReadService::for_test(format!(
            "http://{}",
            listener.local_addr().unwrap()
        )));
        let task = tokio::spawn(async move {
            axum::serve(listener, upstream).await.unwrap();
        });
        let access = Access {
            service: Some(service),
            moc_path: None,
            moc_drive: Ok(None),
            snapshot_reader: None,
            snapshot_cache: Arc::default(),
            resolved_jobs: Arc::default(),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
        };
        let db = tempfile::NamedTempFile::new().unwrap();
        rusqlite::Connection::open(db.path())
            .unwrap()
            .execute_batch("CREATE TABLE insight_title_pref (norm_title TEXT);")
            .unwrap();
        let mut state = super::super::integration_tests::moc_state();
        Arc::get_mut(&mut state).unwrap().indeed_db =
            Some(crate::db::local_sqlite::LocalDb::new(db.path().to_str().unwrap()).unwrap());
        Self {
            state,
            access,
            session: session_for(Some("reader@example.test")).await,
            paths,
            company_fails,
            _db: db,
            task,
        }
    }
    fn service(&self) -> &Arc<JobReadService> {
        self.access.service.as_ref().unwrap()
    }
    async fn versions(
        &self,
        session: &Session,
        id: &str,
        fresh: bool,
    ) -> Result<Value, StatusCode> {
        let result = read_versions(
            State(self.state.clone()),
            Extension(self.access.clone()),
            session.clone(),
            AxumPath(id.into()),
            Query(FreshQuery {
                fresh: fresh.then(|| "1".into()),
            }),
        )
        .await;
        body(result).await
    }
    async fn company(&self, session: &Session, id: &str, fresh: bool) -> Result<Value, StatusCode> {
        let result = read_company(
            State(self.state.clone()),
            Extension(self.access.clone()),
            session.clone(),
            AxumPath(id.into()),
            Query(FreshQuery {
                fresh: fresh.then(|| "1".into()),
            }),
        )
        .await;
        body(result).await
    }
    async fn companies(
        &self,
        session: &Session,
        ids: &str,
        fresh: bool,
    ) -> Result<Value, StatusCode> {
        let result = read_companies(
            State(self.state.clone()),
            Extension(self.access.clone()),
            session.clone(),
            Query(CompaniesQuery {
                ids: ids.into(),
                fresh: fresh.then(|| "1".into()),
            }),
        )
        .await;
        body(result).await
    }
    /// How many times each HubSpot path was read so far.
    fn counts(&self) -> Counts<String, usize> {
        let mut counts = Counts::new();
        for path in self.paths.lock().unwrap().iter() {
            *counts.entry(path.clone()).or_default() += 1;
        }
        counts
    }
    fn total(&self) -> usize {
        self.paths.lock().unwrap().len()
    }
}
/// The `id`s a HubSpot batch read asked for.
async fn batch_inputs(request: Request<Body>) -> Vec<String> {
    let bytes = to_bytes(request.into_body(), 1 << 20).await.unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap();
    body["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|input| input["id"].as_str().unwrap().to_owned())
        .collect()
}
async fn body(result: Result<impl IntoResponse, ReadError>) -> Result<Value, StatusCode> {
    match result {
        Ok(reply) => {
            let reply = reply.into_response();
            assert_eq!(reply.headers()[header::CACHE_CONTROL], "private, no-store");
            Ok(serde_json::from_slice(&to_bytes(reply.into_body(), 65536).await.unwrap()).unwrap())
        }
        Err(ReadError(status, _)) => Err(status),
    }
}
async fn session_for(email: Option<&str>) -> Session {
    let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    if let Some(email) = email {
        session.insert(SESSION_USER_KEY, email).await.unwrap();
        session
            .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
            .await
            .unwrap();
    }
    session
}

#[tokio::test]
async fn versions_read_hubspot_once_and_never_the_company() {
    let f = Fixture::new().await;
    let first = f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(
        f.counts(),
        Counts::from([("/crm/v3/objects/0-420/30".to_owned(), 1)]),
        "the first open is exactly one HubSpot read; no association or company read"
    );
    assert!(first.get("company_name").is_none());
    assert_eq!(first["listing"]["id"], "30");
    // Reopening within the minute is served from the cache with the same content.
    let second = f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 1);
    assert_eq!(second["versions"], first["versions"]);
    assert_eq!(second["current_fields"], first["current_fields"]);
    // `fresh=1` rereads (and replaces the cached copy).
    f.versions(&f.session, "30", true).await.unwrap();
    assert_eq!(f.total(), 2);
    f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 2);
}

#[tokio::test]
async fn cached_versions_still_require_an_authorized_user() {
    let f = Fixture::new().await;
    f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 1);
    let anonymous = session_for(None).await;
    assert_eq!(
        f.versions(&anonymous, "30", false).await.unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    let outsider = session_for(Some("outside@other.test")).await;
    assert_eq!(
        f.versions(&outsider, "30", false).await.unwrap_err(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.versions(&f.session, "not-a-job", false)
            .await
            .unwrap_err(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.total(), 1, "refused requests read nothing");
}

#[tokio::test]
async fn a_saved_draft_drops_the_cached_listing() {
    let f = Fixture::new().await;
    f.versions(&f.session, "30", false).await.unwrap();
    f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 1);
    // What the draft writer does after it writes (or may have written) this listing.
    f.service().detail_cache.forget_listing("30");
    f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 2, "the next open rereads HubSpot");
    f.versions(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 2);
}

#[tokio::test]
async fn company_is_its_own_request_cached_for_hours_and_authorized_each_time() {
    let f = Fixture::new().await;
    let first = f.company(&f.session, "30", false).await.unwrap();
    assert_eq!(
        first,
        json!({"listing_id":"30","company_name":"合成運輸株式会社"})
    );
    assert_eq!(
        f.counts(),
        Counts::from([
            ("/crm/v4/objects/0-420/30/associations/deals".to_owned(), 1),
            (
                "/crm/v4/associations/deals/companies/batch/read".to_owned(),
                1
            ),
            ("/crm/v3/objects/companies/batch/read".to_owned(), 1),
        ])
    );
    f.company(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 3, "second request is served from the cache");
    // A draft write does not throw the name away; the listing record is a different cache.
    f.service().detail_cache.forget_listing("30");
    f.company(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 3);
    // Authorization is checked before the cache is looked at.
    assert_eq!(
        f.company(&session_for(None).await, "30", false)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.company(&session_for(Some("outside@other.test")).await, "30", false)
            .await
            .unwrap_err(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        f.company(&f.session, "x/../1", false).await.unwrap_err(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.total(), 3);
    // `fresh=1` reads the relations again.
    f.company(&f.session, "30", true).await.unwrap();
    assert_eq!(f.total(), 6);
}

#[tokio::test]
async fn a_failed_company_read_is_an_error_and_is_not_remembered() {
    let f = Fixture::new().await;
    f.company_fails.store(true, Ordering::SeqCst);
    assert!(f.company(&f.session, "30", false).await.is_err());
    assert_eq!(f.total(), 1);
    f.company_fails.store(false, Ordering::SeqCst);
    let ok = f.company(&f.session, "30", false).await.unwrap();
    assert_eq!(ok["company_name"], "合成運輸株式会社");
    assert_eq!(f.total(), 4, "1 failed read + 3 reads of the retry");
}

#[tokio::test]
async fn batch_names_equal_the_single_reads_and_cost_three_reads() {
    let f = Fixture::new().await;
    let batch = f.companies(&f.session, "30,31,32,33", false).await.unwrap();
    assert_eq!(
        batch,
        json!({"companies":{
            "30":"合成運輸株式会社",
            "31":null,
            "32":null,
            "33":"別の合成株式会社"
        }}),
        "30 one company; 31 two companies is blank; 32 no deal is blank; 33 another company"
    );
    assert_eq!(
        f.counts(),
        Counts::from([
            ("/crm/v4/associations/0-420/deals/batch/read".to_owned(), 1),
            (
                "/crm/v4/associations/deals/companies/batch/read".to_owned(),
                1
            ),
            ("/crm/v3/objects/companies/batch/read".to_owned(), 1),
        ]),
        "four listings cost one read of each kind"
    );
    // The batch answer is cached: asking again, or one at a time, reads nothing more.
    assert_eq!(
        f.companies(&f.session, "30,31,32,33", false).await.unwrap(),
        batch
    );
    for (id, name) in batch["companies"].as_object().unwrap() {
        let single = f.company(&f.session, id, true).await.unwrap();
        assert_eq!(
            &single["company_name"], name,
            "listing {id}: single read agrees"
        );
    }
}

#[tokio::test]
async fn batch_names_use_the_cache_and_read_only_what_is_missing() {
    let f = Fixture::new().await;
    f.company(&f.session, "30", false).await.unwrap();
    assert_eq!(f.total(), 3);
    let only_known = f.companies(&f.session, "30", false).await.unwrap();
    assert_eq!(only_known, json!({"companies":{"30":"合成運輸株式会社"}}));
    assert_eq!(f.total(), 3, "everything was cached: HubSpot is not called");
    let mixed = f.companies(&f.session, "30,33", false).await.unwrap();
    assert_eq!(mixed["companies"]["33"], "別の合成株式会社");
    assert_eq!(f.total(), 6, "only listing 33 was resolved (3 reads)");
    // The batch filled the cache the single endpoint uses.
    f.company(&f.session, "33", false).await.unwrap();
    assert_eq!(f.total(), 6);
    // `fresh=1` rereads even known names.
    f.companies(&f.session, "30", true).await.unwrap();
    assert_eq!(f.total(), 9);
}

#[tokio::test]
async fn batch_names_refuse_bad_requests_and_unauthorized_users_before_any_read() {
    let f = Fixture::new().await;
    assert_eq!(
        f.companies(&session_for(None).await, "30", false)
            .await
            .unwrap_err(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        f.companies(&session_for(Some("outside@other.test")).await, "30", false)
            .await
            .unwrap_err(),
        StatusCode::FORBIDDEN
    );
    for bad in ["", "30,,31", "30,x/../1", "30,"] {
        assert_eq!(
            f.companies(&f.session, bad, false).await.unwrap_err(),
            StatusCode::BAD_REQUEST,
            "{bad:?}"
        );
    }
    let hundred_and_one = (1..=101)
        .map(|n| n.to_string())
        .collect::<Vec<_>>()
        .join(",");
    assert_eq!(
        f.companies(&f.session, &hundred_and_one, false)
            .await
            .unwrap_err(),
        StatusCode::BAD_REQUEST
    );
    assert_eq!(f.total(), 0, "refused requests read nothing");
}
