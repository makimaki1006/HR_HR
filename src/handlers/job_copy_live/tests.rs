//! Local upstream tests. No production API, credentials, or process environment changes.
use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Method, Request},
    response::Response,
    routing::any,
};
use std::sync::Mutex;

#[tokio::test]
async fn market_route_retains_oidc_allowlist_boundary_and_static_no_store_errors() {
    use tower::ServiceExt;
    let store = tower_sessions::MemoryStore::default();
    let access = Access {
        allowed: BTreeSet::from(["reader@example.test".into()]),
        service: None,
        moc_path: None,
        moc_drive: Ok(None),
        snapshot_reader: None,
        snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
        resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        images: None,
        drive_listings: BTreeSet::new(),
        drive_config_error: None,
    };
    let app = Router::new()
        .route(
            "/api/job-copy/market",
            get(super::super::job_copy_market::read),
        )
        .layer(Extension(access))
        .with_state(moc_state())
        .layer(tower_sessions::SessionManagerLayer::new(store.clone()).with_secure(false));
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/job-copy/market")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let json: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(json, json!({"code":"login_required"}));
    let session = Session::new(None, Arc::new(store), None);
    session
        .insert(SESSION_USER_KEY, "reader@example.test")
        .await
        .unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, "password")
        .await
        .unwrap();
    session.save().await.unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/job-copy/market")
                .header("cookie", format!("id={}", session.id().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let json: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), 4096).await.unwrap()).unwrap();
    assert_eq!(json, json!({"code":"job_copy_access_denied"}));
}

/// Explicit operator validation only; session is seeded in test, not through a new login endpoint.
#[tokio::test]
#[ignore = "Requires approved real Drive snapshot, HubSpot readonly credentials and private output"]
async fn configured_cloud_snapshot_through_authenticated_api_router() {
    use tower::ServiceExt;
    let output = PathBuf::from(
        std::env::var_os("JOB_COPY_APP_TEST_OUTPUT").expect("private output required"),
    );
    let email = std::env::var("JOB_COPY_ALLOWED_EMAILS")
        .expect("allowlist required")
        .split(',')
        .next()
        .unwrap()
        .trim()
        .to_owned();
    assert!(snapshot_pointer(
        std::env::var("JOB_COPY_MOC_DRIVE_FILE_ID").ok().as_deref(),
        std::env::var("JOB_COPY_MOC_DRIVE_SHA256").ok().as_deref()
    )
    .unwrap()
    .is_some());
    let store = tower_sessions::MemoryStore::default();
    let session = Session::new(None, Arc::new(store.clone()), None);
    session.insert(SESSION_USER_KEY, email).await.unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
        .await
        .unwrap();
    session.save().await.unwrap();
    let app = router()
        .with_state(moc_state())
        .layer(tower_sessions::SessionManagerLayer::new(store).with_secure(false));
    let start = Instant::now();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/job-copy/moc")
                .header("cookie", format!("id={}", session.id().unwrap()))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let raw = to_bytes(response.into_body(), MOC_MAX_BYTES).await.unwrap();
    let data: Value = serde_json::from_slice(&raw).unwrap();
    let jobs = data["capture_bundle"]["jobs"].as_array().unwrap();
    assert_eq!(jobs.len(), 36);
    let expected_applications = std::env::var("JOB_COPY_TEST_EXPECTED_APPLICATIONS")
        .ok()
        .map(|value| {
            value
                .parse::<u64>()
                .expect("numeric application expectation")
        })
        .unwrap_or(317);
    assert_eq!(
        data["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| r["summary"]["total"].as_u64().unwrap())
            .sum::<u64>(),
        expected_applications
    );
    if expected_applications == 320 {
        let results = data["results"].as_array().unwrap();
        assert_eq!(
            results
                .iter()
                .map(|r| r["summary"]["joint_demographics"]["total"]
                    .as_u64()
                    .unwrap())
                .sum::<u64>(),
            320
        );
        assert_eq!(
            results
                .iter()
                .map(|r| r["applicant_reasons"]["items"].as_array().unwrap().len())
                .sum::<usize>(),
            12
        );
        assert_eq!(
            results
                .iter()
                .map(|r| r["dated_comparison"]["unknown"].as_u64().unwrap())
                .sum::<u64>(),
            317
        );
        assert!(results
            .iter()
            .flat_map(|r| r["applicant_reasons"]["items"].as_array().unwrap())
            .all(|r| r["version_id"].is_null()));
    }
    let remote_images = jobs
        .iter()
        .flat_map(|j| j["images"].as_array().unwrap())
        .filter(|i| {
            i["url"]
                .as_str()
                .unwrap()
                .starts_with("/api/job-copy/snapshot-image?")
        })
        .count();
    assert_eq!(remote_images, 45);
    std::fs::create_dir_all(&output).unwrap();
    std::fs::write(output.join("api-moc.json"), &raw).unwrap();
    std::fs::write(output.join("api-verification.json"),serde_json::to_vec_pretty(&json!({"ok":true,"jobs":36,"applications":expected_applications,"remote_image_references":45,"elapsed_ms":start.elapsed().as_millis(),"authenticated_api_router":true,"scope":"Real Drive and HubSpot readonly API; test-seeded OIDC session, not production login"})).unwrap()).unwrap();
}

#[test]
fn cloud_snapshot_configuration_is_complete_and_bounded() {
    assert!(snapshot_pointer(None, None).unwrap().is_none());
    let hash = "a".repeat(64);
    assert!(snapshot_pointer(Some("snapshot_12345678"), Some(&hash))
        .unwrap()
        .is_some());
    for (file, hash) in [
        (Some("snapshot_12345678"), None),
        (None, Some(hash.as_str())),
        (Some("https://foreign.test/file"), Some(hash.as_str())),
        (Some("snapshot_12345678"), Some("bad")),
    ] {
        assert_eq!(
            snapshot_pointer(file, hash).err(),
            Some("moc_drive_configuration_invalid")
        );
    }
}

#[tokio::test]
async fn invalid_cloud_configuration_does_not_fall_back_to_local_file() {
    let state = moc_state();
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
        service: None,
        moc_path: Some(PathBuf::from("never_read_local_file.json")),
        moc_drive: Err("moc_drive_configuration_invalid"),
        snapshot_reader: None,
        snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
        resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        images: None,
        drive_listings: BTreeSet::new(),
        drive_config_error: None,
    };
    assert_eq!(
        moc(State(state), Extension(access), session)
            .await
            .err()
            .unwrap()
            .1,
        "moc_drive_configuration_invalid"
    );
}

fn moc_state() -> Arc<AppState> {
    use crate::{config::AppConfig, db::cache::AppCache};
    Arc::new(AppState {
        config: AppConfig {
            port: 0,
            auth_password: String::new(),
            auth_password_hash: String::new(),
            external_passwords: vec![],
            allowed_domains: vec![],
            allowed_domains_extra: vec![],
            hellowork_db_path: String::new(),
            indeed_db_path: String::new(),
            cache_ttl_secs: 60,
            cache_max_entries: 10,
            rate_limit_max_attempts: 5,
            rate_limit_lockout_secs: 60,
            audit_turso_url: String::new(),
            audit_turso_token: String::new(),
            audit_ip_salt: String::new(),
            admin_emails: vec![],
            turso_external_url: String::new(),
            turso_external_token: String::new(),
            salesnow_turso_url: String::new(),
            salesnow_turso_token: String::new(),
            scout_turso_url: String::new(),
            scout_turso_token: String::new(),
        },
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        hubspot: None,
    })
}

#[tokio::test]
async fn moc_requires_same_oidc_allowlist_before_disclosing_file_configuration() {
    let state = moc_state();
    let access = Access {
        allowed: BTreeSet::from(["reader@example.test".into()]),
        service: None,
        moc_path: None,
        moc_drive: Ok(None),
        snapshot_reader: None,
        snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
        resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        images: None,
        drive_listings: BTreeSet::new(),
        drive_config_error: None,
    };
    let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    assert_eq!(
        moc(
            State(state.clone()),
            Extension(access.clone()),
            session.clone()
        )
        .await
        .err()
        .unwrap()
        .1,
        "login_required"
    );
    session
        .insert(SESSION_USER_KEY, "reader@example.test")
        .await
        .unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, "password")
        .await
        .unwrap();
    assert_eq!(
        moc(
            State(state.clone()),
            Extension(access.clone()),
            session.clone()
        )
        .await
        .err()
        .unwrap()
        .1,
        "job_copy_access_denied"
    );
    session
        .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
        .await
        .unwrap();
    assert_eq!(
        moc(
            State(state.clone()),
            Extension(access.clone()),
            session.clone()
        )
        .await
        .err()
        .unwrap()
        .1,
        "moc_not_configured"
    );
    let invalid = Access {
        drive_config_error: Some("drive_listing_configuration_invalid"),
        ..access.clone()
    };
    assert_eq!(
        moc(
            State(state.clone()),
            Extension(invalid.clone()),
            session.clone()
        )
        .await
        .err()
        .unwrap()
        .1,
        "drive_listing_configuration_invalid"
    );
    let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    assert_eq!(
        moc(State(state.clone()), Extension(invalid), anonymous)
            .await
            .err()
            .unwrap()
            .1,
        "login_required"
    );
    let empty = Access {
        allowed: BTreeSet::new(),
        ..access
    };
    assert_eq!(
        moc(State(state), Extension(empty), session)
            .await
            .err()
            .unwrap()
            .1,
        "job_copy_access_denied"
    );
}

#[tokio::test]
async fn image_requires_oidc_before_reading_drive_configuration() {
    let state = moc_state();
    let access = Access {
        allowed: BTreeSet::from(["reader@example.test".into()]),
        service: None,
        moc_path: None,
        moc_drive: Ok(None),
        snapshot_reader: None,
        snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
        resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        images: None,
        drive_listings: BTreeSet::from(["30".into()]),
        drive_config_error: None,
    };
    let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    let query = || {
        Query(ImageQuery {
            company_id: "10".into(),
            listing_id: "30".into(),
            manifest_id: "manifest_12345678".into(),
            slot: 1,
        })
    };
    assert_eq!(
        image(
            State(state.clone()),
            Extension(access.clone()),
            session.clone(),
            query()
        )
        .await
        .err()
        .unwrap()
        .1,
        "login_required"
    );
    session
        .insert(SESSION_USER_KEY, "reader@example.test")
        .await
        .unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, "password")
        .await
        .unwrap();
    assert_eq!(
        image(
            State(state.clone()),
            Extension(access.clone()),
            session.clone(),
            query()
        )
        .await
        .err()
        .unwrap()
        .1,
        "job_copy_access_denied"
    );
    session
        .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
        .await
        .unwrap();
    assert_eq!(
        image(State(state), Extension(access), session, query())
            .await
            .err()
            .unwrap()
            .1,
        "drive_not_configured"
    );
}

#[tokio::test]
#[ignore = "Explicit local private-artifact validation; never required in CI"]
async fn moc_private_artifact_schema_contract() {
    let path = std::env::var_os("JOB_COPY_TEST_MOC_PATH")
        .expect("explicit private validation path required");
    let expected_jobs = std::env::var("JOB_COPY_TEST_EXPECTED_JOBS")
        .map(|value| {
            value
                .parse::<usize>()
                .expect("expected job count must be numeric")
        })
        .unwrap_or(20);
    let expected_applications = std::env::var("JOB_COPY_TEST_EXPECTED_APPLICATIONS")
        .map(|value| {
            value
                .parse::<u64>()
                .expect("expected application count must be numeric")
        })
        .unwrap_or(11);
    let data = load_moc(Some(Path::new(&path))).await.unwrap();
    assert_eq!(
        data["capture_bundle"]["jobs"].as_array().unwrap().len(),
        expected_jobs
    );
    assert_eq!(
        data["results"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["summary"]["total"].as_u64().unwrap())
            .sum::<u64>(),
        expected_applications
    );
}

#[derive(Clone, Copy)]
enum Scenario {
    Happy,
    MissingBatchRecord,
    BatchErrors,
    AssociationPartial,
    AssociationDuplicate,
    AssociationErrors,
    AssociationEmpty207,
    ForeignJob,
    PagingLoop,
    MalformedCursor,
    RetryOnce,
    AlwaysLimited,
    ScopeDenied,
    Redirect,
    ListingLinksDown,
}
#[derive(Clone)]
struct Seen {
    method: Method,
    path: String,
    query: String,
    body: Value,
}
struct Fixture {
    scenario: Scenario,
    requests: Mutex<Vec<Seen>>,
}
struct Upstream {
    base: String,
    fixture: Arc<Fixture>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Upstream {
    async fn start(scenario: Scenario) -> Self {
        let fixture = Arc::new(Fixture {
            scenario,
            requests: Mutex::new(vec![]),
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .fallback(any(reply))
            .with_state(fixture.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            base,
            fixture,
            task,
        }
    }
    fn service(&self) -> JobReadService {
        JobReadService::with_base("local-test-token".into(), self.base.clone()).unwrap()
    }
    fn calls(&self) -> Vec<Seen> {
        self.fixture.requests.lock().unwrap().clone()
    }
}
fn associations(ids: &[&str], after: Option<&str>) -> Value {
    let mut v = json!({"results":ids.iter().map(|id|json!({"toObjectId":id,"associationTypes":[]})).collect::<Vec<_>>()});
    if let Some(after) = after {
        v["paging"] = json!({"next":{"after":after}});
    }
    v
}
fn record(id: &str, properties: Value) -> Value {
    json!({"id":id,"properties":properties})
}
async fn reply(State(fixture): State<Arc<Fixture>>, request: Request<Body>) -> Response {
    let (parts, body) = request.into_parts();
    let body = to_bytes(body, 16_384).await.unwrap();
    let body: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let path = parts.uri.path().to_owned();
    let query = parts.uri.query().unwrap_or("").to_owned();
    let count = {
        let mut requests = fixture.requests.lock().unwrap();
        requests.push(Seen {
            method: parts.method.clone(),
            path: path.clone(),
            query: query.clone(),
            body: body.clone(),
        });
        requests.iter().filter(|r| r.path == path).count()
    };
    if path == "/crm/v3/objects/companies" {
        match fixture.scenario {
            Scenario::ScopeDenied=>return (StatusCode::FORBIDDEN,Json(json!({"message":"private upstream details"}))).into_response(),
            Scenario::Redirect=>return (StatusCode::FOUND,[("location","/sink")]).into_response(),
            Scenario::AlwaysLimited=>return (StatusCode::TOO_MANY_REQUESTS,[("retry-after","0")]).into_response(),
            Scenario::RetryOnce if count==1=>return (StatusCode::TOO_MANY_REQUESTS,[("retry-after","0")]).into_response(),
            _=>return Json(json!({"results":[record("10",json!({"name":"Test customer"}))],"paging":{"next":{"after":"70"}}})).into_response(),
        }
    }
    let response = match path.as_str() {
        "/crm/v4/objects/companies/10/associations/deals" => {
            if matches!(fixture.scenario, Scenario::MalformedCursor) {
                json!({"results":[{"toObjectId":"20"}],"paging":{"next":{"after":{}}}})
            } else if matches!(fixture.scenario, Scenario::PagingLoop) {
                associations(&["20"], Some("70"))
            } else if query.contains("after=70") {
                associations(&["21"], None)
            } else {
                associations(&["20"], Some("70"))
            }
        }
        "/crm/v4/objects/deals/20/associations/0-420" => {
            if query.contains("after=71") {
                associations(&["31"], None)
            } else {
                associations(&["30", "30"], Some("71"))
            }
        }
        "/crm/v4/objects/deals/21/associations/0-420" => associations(
            if matches!(fixture.scenario, Scenario::AssociationEmpty207) {
                &[]
            } else {
                &["30"]
            },
            None,
        ),
        "/crm/v4/associations/deals/0-420/batch/read" => {
            let first = json!({"from":{"id":"20"},"to":[{"toObjectId":"30"}],"paging":{}});
            let second = json!({"from":{"id":"21"},"to":[{"toObjectId":"30"}]});
            match fixture.scenario {
                Scenario::AssociationEmpty207=>return (StatusCode::MULTI_STATUS,Json(json!({"status":"COMPLETE","results":[first],"errors":[{"status":"error","category":"OBJECT_NOT_FOUND","context":{"fromObjectId":["21"],"fromObjectType":["0-3"],"toObjectType":["0-420"]}}]}))).into_response(),
                Scenario::AssociationPartial => json!({"results":[first]}),
                Scenario::AssociationDuplicate => json!({"results":[first.clone(),first,second]}),
                Scenario::AssociationErrors => {
                    json!({"results":[],"errors":[{"message":"read failed"}]})
                }
                _ => json!({"results":[first,second]}),
            }
        }
        "/crm/v4/objects/0-420/30/associations/deals" => associations(
            if matches!(fixture.scenario, Scenario::ForeignJob) {
                &["22"]
            } else {
                &["20", "21"]
            },
            None,
        ),
        "/crm/v4/objects/0-420/30/associations/0-421" => associations(&["50", "51", "50"], None),
        // Application 50 is also linked to job 31; 51 only to job 30.
        "/crm/v4/associations/0-421/0-420/batch/read"
            if matches!(fixture.scenario, Scenario::ListingLinksDown) =>
        {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({"message":"association read failed"})),
            )
                .into_response()
        }
        "/crm/v4/associations/0-421/0-420/batch/read" => json!({"status":"COMPLETE","results":[
            {"from":{"id":"50"},"to":[{"toObjectId":"30"},{"toObjectId":31}]},
            {"from":{"id":"51"},"to":[{"toObjectId":"30"}]}
        ]}),
        // A definition read is refused in ListingLinksDown, to check the label fallback.
        "/crm/v3/properties/0-421/batch/read"
            if matches!(fixture.scenario, Scenario::ListingLinksDown) =>
        {
            return (
                StatusCode::FORBIDDEN,
                Json(json!({"message":"missing scope"})),
            )
                .into_response()
        }
        "/crm/v3/properties/0-421/batch/read" => json!({"status":"COMPLETE","results":[
            {"name":"ouboriyuukategori_hiaringu","type":"enumeration","options":[
                {"value":"kyuuyo","label":"給与"},{"value":"kinmuchi","label":"勤務地"},{"value":"mise","label":"未設定"}]},
            {"name":"ouboriyuukategori_baitaikisai","type":"enumeration","options":[{"value":"kyuuyo","label":"給与"}]},
            {"name":"unrelated","options":[{"value":"x","label":"y"}]}
        ]}),
        "/crm/v3/objects/deals/batch/read"
        | "/crm/v3/objects/0-420/batch/read"
        | "/crm/v3/objects/0-421/batch/read" => {
            if matches!(fixture.scenario, Scenario::BatchErrors) {
                json!({"results":[],"errors":[{"message":"missing object"}]})
            } else {
                let rows = body["inputs"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter_map(|input| {
                        let id = input["id"].as_str().unwrap();
                        if matches!(fixture.scenario, Scenario::MissingBatchRecord) && id == "21" {
                            return None;
                        }
                        let props = match (path.as_str(), id) {
                            ("/crm/v3/objects/0-421/batch/read", "50") => {
                                json!({"yingmuri":"2026-10-03","nenrei":"35","seibetsu":null,"oubodouki":"Flexible hours","ouboriyuu_baitaikisai":"  ","ouboriyuu_hiaringu":null,"genshokumaeshokukaranotenshokuriyuu":"山田さんの店が遠かった","ouboriyuukategori_hiaringu":"kyuuyo"})
                            }
                            ("/crm/v3/objects/0-421/batch/read", "51") => {
                                json!({"yingmuri":null,"nenrei":null,"todoufuken":null})
                            }
                            // No HR ID deliberately avoids capture configuration dependence.
                            ("/crm/v3/objects/0-420/batch/read", _) => {
                                json!({"hs_name":"Test job","shigotonaiyou":"Test body"})
                            }
                            _ => json!({"dealname":"Test contract"}),
                        };
                        Some(record(id, props))
                    })
                    .collect::<Vec<_>>();
                json!({"status":"COMPLETE","results":rows})
            }
        }
        _ => {
            return (
                StatusCode::NOT_FOUND,
                Json(json!({"message":"unexpected read"})),
            )
                .into_response()
        }
    };
    Json(response).into_response()
}

#[tokio::test]
async fn traverses_all_pages_preserves_contracts_and_aggregates_unknowns() {
    let upstream = Upstream::start(Scenario::Happy).await;
    let service = upstream.service();
    let jobs = service.jobs("10", 0).await.unwrap();
    assert_eq!(jobs["total"], 2);
    assert_eq!(jobs["jobs"].as_array().unwrap().len(), 2);
    assert_eq!(jobs["jobs"][0]["deal_ids"], json!(["20", "21"]));
    assert_eq!(jobs["jobs"][1]["record"]["id"], "31");
    let applicants = service.applicants("10", "30").await.unwrap();
    assert_eq!(applicants["summary"]["total"], 2);
    assert_eq!(applicants["summary"]["by_date"]["2026-10-03"], 1);
    assert_eq!(applicants["summary"]["missing_date"], 1);
    // The dated application 50 is also linked to job 31: counted apart, by its date.
    assert_eq!(
        applicants["summary"]["multi_listing_by_date"],
        json!({"2026-10-03": 1})
    );
    assert_eq!(applicants["summary"]["multi_listing_missing_date"], 0);
    assert_eq!(applicants["summary"]["dimensions"]["gender"]["不明"], 2);
    assert!(applicants.get("rows").is_none());
    assert_eq!(applicants["applicant_reasons"]["total_applicants"], 2);
    assert_eq!(applicants["applicant_reasons"]["missing"], 8);
    assert_eq!(applicants["applicant_reasons"]["total_source_values"], 12);
    let counts = &applicants["applicant_reasons"]["source_counts"];
    assert_eq!(
        counts["genshokumaeshokukaranotenshokuriyuu"],
        json!({"missing":1,"blank":0,"nonblank":1})
    );
    assert_eq!(
        counts["ouboriyuukategori_hiaringu"],
        json!({"missing":1,"blank":0,"nonblank":1})
    );
    assert_eq!(
        counts["ouboriyuukategori_baitaikisai"],
        json!({"missing":2,"blank":0,"nonblank":0})
    );
    // The internal value is kept and the label comes from the property definition.
    let selections = &applicants["applicant_reasons"]["selections"];
    assert_eq!(selections.as_array().unwrap().len(), 1);
    assert_eq!(selections[0]["value"], "kyuuyo");
    assert_eq!(selections[0]["label"], "給与");
    assert_eq!(selections[0]["application_date"], "2026-10-03");
    // The transfer reason is masked like the other texts.
    let texts: Vec<_> = applicants["applicant_reasons"]["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["text"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(texts, ["Flexible hours", "＊＊さんの店が遠かった"]);
    assert_eq!(applicants["applicant_reasons"]["blank"], 1);
    assert_eq!(
        applicants["applicant_reasons"]["items"][0]["text"],
        "Flexible hours"
    );
    assert!(applicants["applicant_reasons"]["items"][0]["version_id"].is_null());
    let joint = &applicants["summary"]["joint_demographics"];
    assert_eq!(joint["total"], 2);
    assert_eq!(
        joint["cells"]
            .as_array()
            .unwrap()
            .iter()
            .map(|cell| cell["count"].as_u64().unwrap())
            .sum::<u64>(),
        2
    );
    let calls = upstream.calls();
    assert!(calls.iter().any(|c| c.query.contains("after=71")));
    assert_eq!(
        calls
            .iter()
            .filter(|c| c.path == "/crm/v4/associations/deals/0-420/batch/read")
            .count(),
        1
    );
    assert!(!calls
        .iter()
        .any(|c| c.path == "/crm/v4/objects/deals/21/associations/0-420"));
    assert!(calls
        .iter()
        .all(|c| c.method == Method::GET
            || c.method == Method::POST && c.path.ends_with("/batch/read")));
    let job_batch = calls
        .iter()
        .find(|c| c.path == "/crm/v3/objects/0-420/batch/read")
        .unwrap();
    assert_eq!(job_batch.body["inputs"], json!([{"id":"30"},{"id":"31"}]));
    let applicant_batch = calls
        .iter()
        .find(|c| c.path == "/crm/v3/objects/0-421/batch/read")
        .unwrap();
    assert_eq!(
        applicant_batch.body["properties"],
        json!([
            "yingmuri",
            "seibetsu",
            "nenrei",
            "todoufuken",
            "shikuchouson",
            "oubodouki",
            "ouboriyuu_baitaikisai",
            "ouboriyuu_hiaringu",
            "genshokumaeshokukaranotenshokuriyuu",
            "ouboriyuukategori_hiaringu",
            "ouboriyuukategori_baitaikisai"
        ])
    );
    // One applicant batch read for all the reason sources, and one definition read.
    let count = |path: &str| calls.iter().filter(|c| c.path == path).count();
    assert_eq!(count("/crm/v3/objects/0-421/batch/read"), 1);
    assert_eq!(count("/crm/v3/properties/0-421/batch/read"), 1);
    let definition = calls
        .iter()
        .find(|c| c.path == "/crm/v3/properties/0-421/batch/read")
        .unwrap();
    assert_eq!(
        definition.body["inputs"],
        json!([{"name":"ouboriyuukategori_hiaringu"},{"name":"ouboriyuukategori_baitaikisai"}])
    );
    // A second read of the same job reuses the labels: no second definition read.
    let again = service.applicants("10", "30").await.unwrap();
    assert_eq!(again["applicant_reasons"]["selections"][0]["label"], "給与");
    let calls = upstream.calls();
    let count = |path: &str| calls.iter().filter(|c| c.path == path).count();
    assert_eq!(count("/crm/v3/objects/0-421/batch/read"), 2);
    assert_eq!(count("/crm/v3/properties/0-421/batch/read"), 1);
}

#[tokio::test]
async fn a_failed_listing_link_read_still_returns_totals_and_dates() {
    let upstream = Upstream::start(Scenario::ListingLinksDown).await;
    let applicants = upstream.service().applicants("10", "30").await.unwrap();
    assert_eq!(applicants["summary"]["total"], 2);
    assert_eq!(applicants["summary"]["by_date"]["2026-10-03"], 1);
    assert_eq!(applicants["summary"]["missing_date"], 1);
    // Unknown, not 0: the multi-job counts are left out.
    assert!(applicants["summary"].get("multi_listing_by_date").is_none());
    assert!(applicants["summary"]
        .get("multi_listing_missing_date")
        .is_none());
    assert!(applicants["listing_links_status"].is_string());
    assert!(applicants["dated_comparison"].is_null());
    // The definition read was refused: the value is shown as it is, and the read still works.
    let selection = &applicants["applicant_reasons"]["selections"][0];
    assert_eq!(selection["value"], "kyuuyo");
    assert!(selection["label"].is_null());
}

#[tokio::test]
async fn missing_ids_and_embedded_errors_never_become_complete_results() {
    for scenario in [Scenario::MissingBatchRecord, Scenario::BatchErrors] {
        let upstream = Upstream::start(scenario).await;
        let error = upstream.service().jobs("10", 0).await.unwrap_err();
        assert_eq!(error.1, "hubspot_partial_read");
    }
}

#[tokio::test]
async fn association_batch_requires_exact_sources_and_no_embedded_errors() {
    for scenario in [
        Scenario::AssociationPartial,
        Scenario::AssociationDuplicate,
        Scenario::AssociationErrors,
    ] {
        let upstream = Upstream::start(scenario).await;
        let error = upstream.service().jobs("10", 0).await.unwrap_err();
        assert_eq!(error.1, "hubspot_partial_read");
    }
}

#[tokio::test]
async fn empty_association_207_requires_successful_individual_read_before_zero() {
    let upstream = Upstream::start(Scenario::AssociationEmpty207).await;
    let jobs = upstream.service().jobs("10", 0).await.unwrap();
    assert_eq!(jobs["total"], 2);
    assert_eq!(jobs["jobs"][0]["deal_ids"], json!(["20"]));
    assert!(upstream.calls().iter().any(
        |c| c.method == Method::GET && c.path == "/crm/v4/objects/deals/21/associations/0-420"
    ));
}

#[tokio::test]
async fn foreign_job_is_rejected_before_applicant_read() {
    let upstream = Upstream::start(Scenario::ForeignJob).await;
    let error = upstream.service().applicants("10", "30").await.unwrap_err();
    assert_eq!(error.0, StatusCode::FORBIDDEN);
    assert_eq!(error.1, "job_not_related_to_customer");
    assert!(!upstream.calls().iter().any(|c| c.path.contains("0-421")));
}

#[tokio::test]
async fn association_cursor_loop_is_an_error_not_an_empty_customer() {
    let upstream = Upstream::start(Scenario::PagingLoop).await;
    let error = upstream.service().jobs("10", 0).await.unwrap_err();
    assert_eq!(error.1, "association_paging_invalid");
    assert_eq!(upstream.calls().len(), 2);
}

#[tokio::test]
async fn malformed_next_cursor_cannot_silently_truncate_the_customer() {
    let upstream = Upstream::start(Scenario::MalformedCursor).await;
    let error = upstream.service().jobs("10", 0).await.unwrap_err();
    assert_eq!(error.1, "hubspot_invalid_response");
    assert_eq!(upstream.calls().len(), 1);
}

#[tokio::test]
async fn retry_is_bounded_and_scope_denial_is_explicit() {
    let retry = Upstream::start(Scenario::RetryOnce).await;
    assert_eq!(
        retry.service().customers(None).await.unwrap()["customers"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(retry.calls().len(), 2);
    let limited = Upstream::start(Scenario::AlwaysLimited).await;
    assert_eq!(
        limited.service().customers(None).await.unwrap_err().1,
        "hubspot_rate_limited"
    );
    assert_eq!(limited.calls().len(), 2);
    let denied = Upstream::start(Scenario::ScopeDenied).await;
    assert_eq!(
        denied.service().customers(None).await.unwrap_err().1,
        "hubspot_scope_denied"
    );
    assert_eq!(denied.calls().len(), 1);
}

#[tokio::test]
async fn redirects_and_injected_ids_never_reach_other_paths() {
    let upstream = Upstream::start(Scenario::Redirect).await;
    assert_eq!(
        upstream.service().customers(None).await.unwrap_err().1,
        "hubspot_read_failed"
    );
    assert_eq!(upstream.calls().len(), 1);
    let service = upstream.service();
    assert_eq!(
        service.jobs("10/../../sink", 0).await.unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        service.customers(Some("70?secret=x")).await.unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(
        service.applicants("10", "30/sink").await.unwrap_err().0,
        StatusCode::BAD_REQUEST
    );
    assert_eq!(upstream.calls().len(), 1);
}

// Progressive load fixtures: local HTTP only, preserving all original hashes and metadata.
fn progressive_fixture() -> Value {
    json!({"schemaVersion":1,"capturedAt":"2026-10-04T01:00:00Z",
        "capture_bundle":{"schemaVersion":1,"capturedAt":"2026-10-03T01:00:00Z","jobs":[{
            "id":"capture-job-30","hubspotListingId":"30","companyIds":["10"],"title":"Synthetic","body":"Synthetic body",
            "images":[{"id":"image-1","url":"data:image/png;base64,YQ==","contentHash":"a".repeat(64),"sourceSlot":2}]}]},
        "results":[{"listing_id":"30","summary":{"total":0,"duplicate_ids":0,"missing_date":0,"by_date":{},"dimensions":{}},"dated_comparison":null}]})
}
fn progressive_access() -> Access {
    Access {
        allowed: BTreeSet::from(["reader@example.test".into()]),
        service: None,
        moc_path: None,
        moc_drive: snapshot_pointer(Some("synthetic_snapshot_123"), Some(&"b".repeat(64))),
        snapshot_reader: None,
        snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
        resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
        images: None,
        drive_listings: BTreeSet::from(["30".into()]),
        drive_config_error: None,
    }
}
async fn progressive_session() -> Session {
    let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    session
        .insert(SESSION_USER_KEY, "reader@example.test")
        .await
        .unwrap();
    session
        .insert(SESSION_LOGIN_METHOD_KEY, LOGIN_METHOD_GOOGLE_OIDC)
        .await
        .unwrap();
    session
}
async fn seed_snapshot(access: &Access, data: Value) {
    let pointer = access.moc_drive.as_ref().unwrap().as_ref().unwrap();
    *access.snapshot_cache.lock().await = Some(CachedSnapshot {
        file_id: pointer.file_id.clone(),
        sha256: pointer.sha256.clone(),
        data: Arc::new(data),
    });
}
fn progressive_query() -> Result<Query<SnapshotImageQuery>, QueryRejection> {
    Ok(Query(SnapshotImageQuery {
        listing_id: "30".into(),
        version: 0,
        slot: 1,
        image_hash: "a".repeat(64),
    }))
}
#[test]
fn deferred_images_preserve_metadata_and_do_not_fill_unobserved_history() {
    let mut data = progressive_fixture();
    let current = data["capture_bundle"]["jobs"][0]["images"].clone();
    data["capture_bundle"]["jobs"][0]["history"] = json!([
        {"historicalImageBytesAvailable":true,"images":current},
        {"historicalImageBytesAvailable":false,"images":current}]);
    let original = data.clone();
    defer_snapshot_images(&mut data, &BTreeSet::from(["30".into()])).unwrap();
    let job = &data["capture_bundle"]["jobs"][0];
    assert!(job["images"][0]["url"]
        .as_str()
        .unwrap()
        .contains("version=0&slot=1&image_hash="));
    assert_eq!(job["images"][0]["sourceSlot"], 2);
    assert!(job["history"][0]["images"][0]["url"]
        .as_str()
        .unwrap()
        .contains("version=1&slot=1"));
    assert_eq!(job["history"][1]["images"], current);
    assert_eq!(data["results"], original["results"]);
    assert_eq!(job["body"], original["capture_bundle"]["jobs"][0]["body"]);
    assert_eq!(job["images"][0]["contentHash"], "a".repeat(64));
    assert!(snapshot_image_entry(job, 2, 1).is_err());
    assert!(snapshot_image_entry(job, 11, 1).is_err());
    assert!(snapshot_image_entry(job, 0, 0).is_err());
    assert!(snapshot_image_entry(job, 0, 2).is_err());
}
#[tokio::test]
async fn initial_moc_never_requires_image_bridge_and_keeps_raw_cache_immutable() {
    let access = progressive_access();
    let raw = progressive_fixture();
    seed_snapshot(&access, raw.clone()).await;
    let response = moc(
        State(moc_state()),
        Extension(access.clone()),
        progressive_session().await,
    )
    .await
    .unwrap()
    .into_response();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
    let data: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), MOC_MAX_BYTES).await.unwrap())
            .unwrap();
    assert!(data["capture_bundle"]["jobs"][0]["images"][0]["url"]
        .as_str()
        .unwrap()
        .starts_with("/api/job-copy/snapshot-image?"));
    assert_eq!(*raw_moc(&access).await.unwrap(), raw);
    assert!(access.resolved_jobs.lock().await.is_empty());
}
#[tokio::test]
async fn deferred_query_auth_hash_and_selection_are_checked_before_upstream() {
    let access = progressive_access();
    seed_snapshot(&access, progressive_fixture()).await;
    let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    assert_eq!(
        snapshot_image(
            State(moc_state()),
            Extension(access.clone()),
            anonymous,
            progressive_query()
        )
        .await
        .err()
        .unwrap()
        .1,
        "login_required"
    );
    let session = progressive_session().await;
    for (version, slot, hash, code) in [
        (0, 1, "c".repeat(64), "snapshot_image_changed"),
        (0, 0, "a".repeat(64), "invalid_snapshot_image_request"),
        (1, 1, "a".repeat(64), "snapshot_image_not_found"),
        (0, 1, "bad".into(), "invalid_snapshot_image_request"),
    ] {
        let query = Ok(Query(SnapshotImageQuery {
            listing_id: "30".into(),
            version,
            slot,
            image_hash: hash,
        }));
        let error = snapshot_image(
            State(moc_state()),
            Extension(access.clone()),
            session.clone(),
            query,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(error.1, code);
        assert_eq!(
            error.into_response().headers()[header::CACHE_CONTROL],
            "no-store"
        );
    }
    let query = Ok(Query(SnapshotImageQuery {
        listing_id: "31".into(),
        version: 0,
        slot: 1,
        image_hash: "a".repeat(64),
    }));
    assert_eq!(
        snapshot_image(
            State(moc_state()),
            Extension(access.clone()),
            session,
            query
        )
        .await
        .err()
        .unwrap()
        .1,
        "image_listing_not_enabled"
    );
    let mut enabled_unknown = access.clone();
    enabled_unknown.drive_listings.insert("31".into());
    let query = Ok(Query(SnapshotImageQuery {
        listing_id: "31".into(),
        version: 0,
        slot: 1,
        image_hash: "a".repeat(64),
    }));
    assert_eq!(
        snapshot_image(
            State(moc_state()),
            Extension(enabled_unknown),
            progressive_session().await,
            query
        )
        .await
        .err()
        .unwrap()
        .1,
        "snapshot_listing_not_found"
    );
    for raw in [
        "listing_id=30&version=0&slot=1",
        "listing_id=30&version=0&slot=1&image_hash=a&extra=1",
        "listing_id=30&listing_id=31&version=0&slot=1&image_hash=a",
        "listing_id=30&version=-1&slot=1&image_hash=a",
    ] {
        let uri = format!("/api/job-copy/snapshot-image?{raw}")
            .parse()
            .unwrap();
        assert!(Query::<SnapshotImageQuery>::try_from_uri(&uri).is_err());
    }
}
struct ProgressiveUpstream {
    access: Access,
    requests: Arc<Mutex<Vec<String>>>,
    fail_snapshot: Arc<std::sync::atomic::AtomicBool>,
    fail_pointer: Arc<std::sync::atomic::AtomicBool>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for ProgressiveUpstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl ProgressiveUpstream {
    async fn start() -> Self {
        use sha2::Digest;
        use std::sync::atomic::{AtomicBool, Ordering};
        let requests = Arc::new(Mutex::new(Vec::new()));
        let fail_snapshot = Arc::new(AtomicBool::new(false));
        let fail_pointer = Arc::new(AtomicBool::new(false));
        let raw = serde_json::to_vec(&progressive_fixture()).unwrap();
        let sha = format!("{:x}", sha2::Sha256::digest(&raw));
        let manifest=serde_json::to_vec(&json!({"schemaVersion":1,"listingId":"30","companyIds":["10"],"observedAt":"2026-10-03T00:00:00Z","operationId":"f".repeat(64),"images":[{"slot":2,"fileId":"synthetic_image_123","sha256":"a".repeat(64),"mimeType":"image/png","size":1}]})).unwrap();
        let pointer=json!({"fileId":"synthetic_manifest_123","sha256":format!("{:x}",sha2::Sha256::digest(&manifest)),"observedAt":"2026-10-03T00:00:00Z"}).to_string();
        let seen = requests.clone();
        let fail = fail_snapshot.clone();
        let fail_hs = fail_pointer.clone();
        let app=Router::new().fallback(any(move |request:Request<Body>| {
            let seen=seen.clone();let fail=fail.clone();let fail_hs=fail_hs.clone();let raw=raw.clone();let manifest=manifest.clone();let pointer=pointer.clone();
            async move {
                let path=request.uri().path().to_owned();seen.lock().unwrap().push(path.clone());
                if path=="/token" {return Json(json!({"access_token":"synthetic-token","token_type":"Bearer","expires_in":3600})).into_response();}
                if path=="/drive/v3/files/synthetic_snapshot_123" {
                    assert_eq!(request.headers()[header::AUTHORIZATION],"Bearer synthetic-token");
                    if fail.swap(false,Ordering::SeqCst){return StatusCode::FORBIDDEN.into_response();}
                    return ([(header::CONTENT_TYPE,"application/json")],raw).into_response();
                }
                if path=="/drive/v3/files/synthetic_manifest_123" {return ([(header::CONTENT_TYPE,"application/json")],manifest).into_response();}
                if path=="/crm/v3/objects/0-420/30" {
                    if fail_hs.load(Ordering::SeqCst){return StatusCode::FORBIDDEN.into_response();}
                    return Json(json!({"properties":{"job_copy_drive_manifest_v1":pointer}})).into_response();
                }
                if path=="/crm/v4/objects/companies/10/associations/deals" || path=="/crm/v4/objects/0-420/30/associations/deals" {return Json(json!({"results":[{"toObjectId":"20"}]})).into_response();}
                panic!("unexpected local fixture route");
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let reader = Arc::new(super::super::job_copy_drive::DriveReader::for_test(&base));
        let mut access = progressive_access();
        access.moc_drive = snapshot_pointer(Some("synthetic_snapshot_123"), Some(&sha));
        access.snapshot_reader = Some(reader.clone());
        access.images = Some(Arc::new(
            super::super::job_copy_image_bridge::ImageBridge::for_test(&base, reader),
        ));
        Self {
            access,
            requests,
            fail_snapshot,
            fail_pointer,
            task,
        }
    }
    fn count(&self, path: &str) -> usize {
        self.requests
            .lock()
            .unwrap()
            .iter()
            .filter(|p| p.as_str() == path)
            .count()
    }
}
#[tokio::test]
async fn raw_snapshot_singleflight_retries_failure_and_does_not_resolve_images() {
    use std::sync::atomic::Ordering;
    let upstream = ProgressiveUpstream::start().await;
    upstream.fail_snapshot.store(true, Ordering::SeqCst);
    assert!(raw_moc(&upstream.access).await.is_err());
    assert!(upstream.access.snapshot_cache.lock().await.is_none());
    let (a, b, c) = tokio::join!(
        raw_moc(&upstream.access),
        raw_moc(&upstream.access),
        raw_moc(&upstream.access)
    );
    let a = a.unwrap();
    assert!(Arc::ptr_eq(&a, &b.unwrap()));
    assert!(Arc::ptr_eq(&a, &c.unwrap()));
    assert_eq!(upstream.count("/drive/v3/files/synthetic_snapshot_123"), 2);
    assert_eq!(upstream.count("/crm/v3/objects/0-420/30"), 0);
    let mut changed = upstream.access.clone();
    changed.moc_drive = snapshot_pointer(Some("synthetic_snapshot_123"), Some(&"d".repeat(64)));
    assert!(raw_moc(&changed).await.is_err());
    assert_eq!(upstream.count("/drive/v3/files/synthetic_snapshot_123"), 3);
}
#[tokio::test]
async fn selected_job_redirect_retries_errors_shares_resolution_and_expires() {
    use std::sync::atomic::Ordering;
    let upstream = ProgressiveUpstream::start().await;
    let session = progressive_session().await;
    upstream.fail_pointer.store(true, Ordering::SeqCst);
    assert!(snapshot_image(
        State(moc_state()),
        Extension(upstream.access.clone()),
        session.clone(),
        progressive_query()
    )
    .await
    .is_err());
    assert!(upstream.access.resolved_jobs.lock().await.is_empty());
    upstream.fail_pointer.store(false, Ordering::SeqCst);
    let (a, b) = tokio::join!(
        snapshot_image(
            State(moc_state()),
            Extension(upstream.access.clone()),
            session.clone(),
            progressive_query()
        ),
        snapshot_image(
            State(moc_state()),
            Extension(upstream.access.clone()),
            session.clone(),
            progressive_query()
        )
    );
    for response in [a.unwrap(), b.unwrap()] {
        assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
        assert_eq!(
            response.headers()[header::CACHE_CONTROL],
            "private, no-store"
        );
        assert_eq!(response.headers()[header::LOCATION],"/api/job-copy/image?company_id=10&listing_id=30&manifest_id=synthetic_manifest_123&slot=2");
    }
    assert_eq!(upstream.count("/crm/v3/objects/0-420/30"), 2);
    assert_eq!(upstream.count("/drive/v3/files/synthetic_manifest_123"), 1);
    let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
    assert_eq!(
        snapshot_image(
            State(moc_state()),
            Extension(upstream.access.clone()),
            anonymous,
            progressive_query()
        )
        .await
        .err()
        .unwrap()
        .1,
        "login_required"
    );
    for (expires, _) in upstream.access.resolved_jobs.lock().await.values_mut() {
        *expires = Instant::now() - Duration::from_secs(1);
    }
    snapshot_image(
        State(moc_state()),
        Extension(upstream.access.clone()),
        session,
        progressive_query(),
    )
    .await
    .unwrap();
    assert_eq!(upstream.count("/crm/v3/objects/0-420/30"), 3);
}

#[tokio::test]
async fn local_snapshot_is_reloaded_instead_of_using_cloud_cache() {
    let mut access = progressive_access();
    access.moc_drive = Ok(None);
    let path = std::env::temp_dir().join(format!(
        "job-copy-progressive-{}-{}.json",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let mut data = progressive_fixture();
    tokio::fs::write(&path, serde_json::to_vec(&data).unwrap())
        .await
        .unwrap();
    access.moc_path = Some(path.clone());
    assert_eq!(
        raw_moc(&access).await.unwrap()["capture_bundle"]["jobs"][0]["body"],
        "Synthetic body"
    );
    data["capture_bundle"]["jobs"][0]["body"] = json!("Changed local body");
    tokio::fs::write(&path, serde_json::to_vec(&data).unwrap())
        .await
        .unwrap();
    assert_eq!(
        raw_moc(&access).await.unwrap()["capture_bundle"]["jobs"][0]["body"],
        "Changed local body"
    );
    assert!(access.snapshot_cache.lock().await.is_none());
    tokio::fs::remove_file(path).await.unwrap();
}
