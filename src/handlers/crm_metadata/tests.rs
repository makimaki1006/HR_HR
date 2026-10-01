use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::OriginalUri,
    http::{HeaderMap, Request},
};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use tower::ServiceExt;

#[derive(Default)]
struct UpstreamState {
    requests: AtomicUsize,
    mode: AtomicU16,
    auth: Mutex<Vec<String>>,
}
struct MockUpstream {
    state: Arc<UpstreamState>,
    url: String,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for MockUpstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
async fn upstream(
    State(state): State<Arc<UpstreamState>>,
    OriginalUri(uri): OriginalUri,
    headers: HeaderMap,
) -> Response {
    state.requests.fetch_add(1, Ordering::SeqCst);
    state.auth.lock().await.push(
        headers
            .get("authorization")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned(),
    );
    let mode = state.mode.load(Ordering::SeqCst);
    if mode == 999 {
        return (StatusCode::OK, "invalid upstream-secret-token").into_response();
    }
    if mode != 0 {
        return (
            StatusCode::from_u16(mode).unwrap(),
            [("retry-after", "0")],
            "upstream-secret-token",
        )
            .into_response();
    }
    let body = if uri.path() == "/crm/v3/pipelines/deals" {
        serde_json::json!({"results":[
            {"id":"later","label":"営業パイプライン","displayOrder":10,"stages":[
                {"id":"meeting","label":"商談設定","displayOrder":2},
                {"id":"new","label":"新規受付","displayOrder":0}]},
            {"id":"first","label":"採用支援","displayOrder":0,"stages":[]}]})
    } else {
        let name = if uri.path().ends_with("contacts") {
            "firstname"
        } else if uri.path().ends_with("companies") {
            "industry"
        } else {
            "bpo_42"
        };
        serde_json::json!({"results":[
            {"name":name,"label":"実アカウントの項目名","type":"enumeration","fieldType":"select",
             "options":[{"label":"表示ラベル","value":"internal_value","hidden":true}]},
            {"name":"unreviewed_private_field","label":"Do not expose","type":"string","fieldType":"text"}]})
    };
    Json(body).into_response()
}
async fn mock_upstream() -> MockUpstream {
    let state = Arc::new(UpstreamState::default());
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new().fallback(upstream).with_state(state.clone());
    let task = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    MockUpstream { state, url, task }
}
fn service(mock: &MockUpstream) -> MetadataService {
    MetadataService::with_base_url("upstream-secret-token".to_owned(), mock.url.clone()).unwrap()
}

#[test]
fn only_allowlisted_google_workspace_users_are_authorized() {
    let allowed = HashSet::from(["operator@example.com".to_owned()]);
    for email in [None, Some("")] {
        let err = authorize(email, Some(LOGIN_METHOD_GOOGLE_OIDC), &allowed).unwrap_err();
        assert_eq!(err.status, StatusCode::UNAUTHORIZED);
        assert_eq!(err.code, "login_required");
    }
    for method in [None, Some("password")] {
        let err = authorize(Some("operator@example.com"), method, &allowed).unwrap_err();
        assert_eq!(err.status, StatusCode::FORBIDDEN);
        assert_eq!(err.code, "google_login_required");
    }
    let err = authorize(
        Some("other@example.com"),
        Some(LOGIN_METHOD_GOOGLE_OIDC),
        &allowed,
    )
    .unwrap_err();
    assert_eq!(err.status, StatusCode::FORBIDDEN);
    assert_eq!(err.code, "crm_metadata_access_denied");
    assert!(authorize(
        Some("Operator@Example.com"),
        Some(LOGIN_METHOD_GOOGLE_OIDC),
        &allowed
    )
    .is_ok());
    assert!(authorize(
        Some("operator@example.com"),
        Some(LOGIN_METHOD_GOOGLE_OIDC),
        &HashSet::new()
    )
    .is_err());
}

#[tokio::test]
async fn live_labels_internal_values_sorting_cache_and_explicit_refresh() {
    let mock = mock_upstream().await;
    let service = service(&mock);
    let first = service.metadata(false).await.unwrap();
    assert!(!first.cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    assert_eq!(first.properties.len(), 3);
    assert_eq!(
        first
            .pipelines
            .iter()
            .map(|p| p.id.as_str())
            .collect::<Vec<_>>(),
        ["first", "later"]
    );
    assert_eq!(first.pipelines[1].label, "営業パイプライン");
    assert_eq!(
        first.pipelines[1]
            .stages
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        ["new", "meeting"]
    );
    assert_eq!(first.pipelines[1].stages[0].label, "新規受付");
    for property in &first.properties {
        assert_eq!(property.label, "実アカウントの項目名");
        assert_eq!(property.options[0].label, "表示ラベル");
        assert_eq!(property.options[0].value, "internal_value");
        assert!(property.options[0].hidden);
    }
    let serialized = serde_json::to_string(&first).unwrap();
    assert!(!serialized.contains("upstream-secret-token"));
    assert!(!serialized.contains("unreviewed_private_field"));
    assert!(chrono::DateTime::parse_from_rfc3339(&first.fetched_at).is_ok());
    assert!(first.total_ms >= first.hubspot_ms);
    assert!(mock
        .state
        .auth
        .lock()
        .await
        .iter()
        .all(|h| h == "Bearer upstream-secret-token"));
    let cached = service.metadata(false).await.unwrap();
    assert!(cached.cache_hit);
    assert_eq!(cached.hubspot_ms, 0.0);
    assert_eq!(cached.fetched_at, first.fetched_at);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    assert!(!service.metadata(true).await.unwrap().cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn concurrent_cache_misses_share_one_upstream_load() {
    let mock = mock_upstream().await;
    let service = Arc::new(service(&mock));
    let (first, second, third) = tokio::join!(
        service.metadata(false),
        service.metadata(false),
        service.metadata(false)
    );
    let responses = [first.unwrap(), second.unwrap(), third.unwrap()];
    assert_eq!(responses.iter().filter(|r| !r.cache_hit).count(), 1);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    assert!(responses
        .iter()
        .all(|r| r.fetched_at == responses[0].fetched_at));
}

#[tokio::test]
async fn upstream_failures_are_sanitized_and_do_not_create_a_success_cache() {
    for (upstream_status, expected_status, code) in [
        (401, StatusCode::BAD_GATEWAY, "hubspot_auth_failed"),
        (403, StatusCode::BAD_GATEWAY, "hubspot_scope_denied"),
        (404, StatusCode::BAD_GATEWAY, "hubspot_endpoint_missing"),
        (429, StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
        (500, StatusCode::BAD_GATEWAY, "hubspot_unavailable"),
        (999, StatusCode::BAD_GATEWAY, "hubspot_invalid_response"),
    ] {
        let mock = mock_upstream().await;
        mock.state.mode.store(upstream_status, Ordering::SeqCst);
        let service = service(&mock);
        let error = service.metadata(false).await.unwrap_err();
        assert_eq!(error.status, expected_status);
        assert_eq!(error.code, code);
        assert!(service.cache.lock().await.is_none());
        let response = error.into_response();
        assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"code":code})
        );
        assert!(!String::from_utf8(body.to_vec())
            .unwrap()
            .contains("upstream-secret-token"));
        mock.state.mode.store(0, Ordering::SeqCst);
        let before = mock.state.requests.load(Ordering::SeqCst);
        let success = service.metadata(false).await.unwrap();
        assert!(!success.cache_hit);
        assert!(mock.state.requests.load(Ordering::SeqCst) >= before + 4);
    }
}

#[test]
fn missing_token_is_unavailable_not_an_upstream_request() {
    let error = MetadataService::with_base_url("  ".to_owned(), "http://127.0.0.1:1".to_owned())
        .err()
        .unwrap();
    assert_eq!(error.status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(error.code, "hubspot_not_configured");
}

#[tokio::test]
async fn real_application_api_returns_json_401_instead_of_login_redirect() {
    let state = Arc::new(AppState {
        config: crate::config::AppConfig::from_env(),
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: crate::db::cache::AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
    });
    let response = crate::build_app(state)
        .oneshot(
            Request::builder()
                .uri("/api/crm/metadata")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert!(response.headers().get("location").is_none());
    assert_eq!(response.headers().get("cache-control").unwrap(), "no-store");
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
        serde_json::json!({"code":"login_required"})
    );
}

#[test]
fn metadata_types_expose_labels_internal_ids_and_timing() {
    let cfg = ts_rs::Config::default();
    let response = CrmMetadataResponse::decl(&cfg);
    for field in [
        "properties: Array<CrmPropertyDefinition>",
        "pipelines: Array<CrmPipeline>",
        "hubspot_ms: number",
        "total_ms: number",
        "cache_hit: boolean",
    ] {
        assert!(response.contains(field), "{response}");
    }
    let option = CrmPropertyOption::decl(&cfg);
    assert!(option.contains("label: string"));
    assert!(option.contains("value: string"));
    let stage = CrmStage::decl(&cfg);
    assert!(stage.contains("id: string"));
    assert!(stage.contains("label: string"));
}
