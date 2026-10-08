use super::*;
use crate::hubspot::ClientOptions;
use crate::AppState;
use axum::{
    body::{to_bytes, Body},
    extract::{OriginalUri, State},
    http::{HeaderMap, Request, StatusCode},
    response::{IntoResponse, Response},
    Json, Router,
};
use std::sync::atomic::{AtomicU16, AtomicUsize, Ordering};
use std::sync::Arc;
use tokio::sync::Mutex;
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
fn client(mock: &MockUpstream) -> HubSpotClient {
    HubSpotClient::new(
        "upstream-secret-token".to_owned(),
        &mock.url,
        ClientOptions {
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
            search_min_interval: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(1),
        },
    )
    .unwrap()
}

#[tokio::test]
async fn live_labels_internal_values_sorting_cache_and_explicit_refresh() {
    let mock = mock_upstream().await;
    let (client, cache) = (client(&mock), MetadataCache::default());
    let first = cache.get(&client, false).await.unwrap();
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
    let cached = cache.get(&client, false).await.unwrap();
    assert!(cached.cache_hit);
    assert_eq!(cached.hubspot_ms, 0.0);
    assert_eq!(cached.fetched_at, first.fetched_at);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    assert!(!cache.get(&client, true).await.unwrap().cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 8);
}

#[tokio::test]
async fn concurrent_cache_misses_share_one_upstream_load() {
    let mock = mock_upstream().await;
    let (client, cache) = (client(&mock), MetadataCache::default());
    let (first, second, third) = tokio::join!(
        cache.get(&client, false),
        cache.get(&client, false),
        cache.get(&client, false)
    );
    let responses = [first.unwrap(), second.unwrap(), third.unwrap()];
    assert_eq!(responses.iter().filter(|r| !r.cache_hit).count(), 1);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    assert!(responses
        .iter()
        .all(|r| r.fetched_at == responses[0].fetched_at));
}

#[tokio::test]
async fn upstream_failures_map_to_error_kinds_and_do_not_create_a_success_cache() {
    for (upstream_status, expected_status, kind) in [
        (401, StatusCode::BAD_GATEWAY, "hubspot_auth"),
        (403, StatusCode::BAD_GATEWAY, "hubspot_auth"),
        (404, StatusCode::BAD_GATEWAY, "hubspot_upstream"),
        (429, StatusCode::SERVICE_UNAVAILABLE, "hubspot_rate_limited"),
        (500, StatusCode::BAD_GATEWAY, "hubspot_upstream"),
        (999, StatusCode::BAD_GATEWAY, "hubspot_decode"),
    ] {
        let mock = mock_upstream().await;
        mock.state.mode.store(upstream_status, Ordering::SeqCst);
        let (client, cache) = (client(&mock), MetadataCache::default());
        let error = cache.get(&client, false).await.unwrap_err();
        assert_eq!(error.error_kind(), kind, "upstream {upstream_status}");
        assert_eq!(error.http_status(), expected_status.as_u16());
        assert!(cache.slot.lock().await.is_none());
        // 上流の本文 (ここでは秘密の文字列) もトークンも、エラーの文言に出ない
        let text = format!("{error} {error:?}");
        assert!(!text.contains("upstream-secret-token"), "{text}");
        mock.state.mode.store(0, Ordering::SeqCst);
        let before = mock.state.requests.load(Ordering::SeqCst);
        let success = cache.get(&client, false).await.unwrap();
        assert!(!success.cache_hit);
        assert!(mock.state.requests.load(Ordering::SeqCst) >= before + 4);
    }
}

#[test]
fn missing_token_is_unavailable_not_an_upstream_request() {
    let error = HubSpotClient::new(
        "  ".to_owned(),
        "http://127.0.0.1:1",
        ClientOptions::default(),
    )
    .err()
    .unwrap();
    assert_eq!(error, HubSpotError::NotConfigured);
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
        hubspot: None,
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
        serde_json::json!({"error_kind":"login_required"})
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

// ---------------------------------------------------------------------------
// 段階 A (逆証明): HubSpot の定義の応答の端
// ---------------------------------------------------------------------------

/// null / 欠落があっても、確認済みの項目は読める。確認済みでない項目は中身を見ない
/// (形が想定外でも全体を壊さない)。
#[test]
fn definitions_tolerate_null_missing_and_ignore_unreviewed_garbage() {
    let v = serde_json::json!({"results": [
        // 確認済み: options が null, label / fieldType が欠落, option の hidden が null
        {"name": "bpo_42", "type": "string", "options": null},
        {"name": "bpo_10", "label": null, "type": "enumeration", "fieldType": "select",
         "options": [{"label": "A", "value": "a", "hidden": null}, {"value": "b"}]},
        // 確認済みでない: 形が壊れていても無視される
        {"name": "unreviewed", "options": "not-an-array", "label": 5},
        {"label": "name の無い項目"},
        "文字列だけの要素"
    ]});
    let out = parse_properties(RecordType::Deal, &v).expect("壊れた未確認項目で失敗しない");
    assert_eq!(
        out.iter().map(|p| p.name.as_str()).collect::<Vec<_>>(),
        vec!["bpo_42", "bpo_10"]
    );
    assert_eq!(out[0].label, "");
    assert_eq!(out[0].field_type, "");
    assert!(out[0].options.is_empty());
    assert_eq!(out[1].options.len(), 2);
    assert!(!out[1].options[0].hidden);
    assert_eq!(out[1].options[1].label, "");
    assert_eq!(out[1].options[1].value, "b");
}

/// 確認済みの項目の形が壊れている (options が配列でない) / results が無い → 例外ではなく Decode エラー
#[test]
fn broken_reviewed_definition_or_missing_results_is_a_decode_error() {
    let broken = serde_json::json!({"results": [{"name": "bpo_42", "options": "x"}]});
    assert_eq!(
        parse_properties(RecordType::Deal, &broken)
            .unwrap_err()
            .error_kind(),
        "hubspot_decode"
    );
    for v in [
        serde_json::json!({}),
        serde_json::json!({"results": null}),
        serde_json::json!([]),
    ] {
        assert_eq!(
            parse_properties(RecordType::Deal, &v)
                .unwrap_err()
                .error_kind(),
            "hubspot_decode"
        );
        assert_eq!(
            parse_pipelines(&v).unwrap_err().error_kind(),
            "hubspot_decode"
        );
    }
}

/// パイプライン: displayOrder が null / 欠落、stages が null / 欠落でも並べられる。
/// 順序は displayOrder (同順位は元の順)
#[test]
fn pipelines_tolerate_missing_display_order_and_stages() {
    let v = serde_json::json!({"results": [
        {"id": "b", "label": "B", "displayOrder": 2, "stages": null},
        {"id": "a", "label": "A", "stages": [
            {"id": "s2", "label": "二", "displayOrder": 1},
            {"id": "s0", "label": null, "displayOrder": null}]},
        {"id": "c", "displayOrder": 2}
    ]});
    let out = parse_pipelines(&v).unwrap();
    assert_eq!(
        out.iter().map(|p| p.id.as_str()).collect::<Vec<_>>(),
        vec!["a", "b", "c"]
    );
    assert_eq!(
        out[0]
            .stages
            .iter()
            .map(|s| s.id.as_str())
            .collect::<Vec<_>>(),
        vec!["s0", "s2"]
    );
    assert_eq!(out[0].stages[0].label, "");
    assert!(out[1].stages.is_empty());
}

/// refresh の下限: 下限内のキャッシュは refresh=true でも返し、上流を叩かない。
/// 下限 0 (既定) なら従来どおり取り直す (逆証明)
#[tokio::test]
async fn refresh_floor_serves_cache_and_zero_floor_refetches() {
    let mock = mock_upstream().await;
    let client = client(&mock);
    let cache = MetadataCache::with_refresh_floor(Duration::from_secs(30));
    assert!(!cache.get(&client, false).await.unwrap().cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);
    let r = cache.get(&client, true).await.unwrap();
    assert!(r.cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 4);

    let open = MetadataCache::default();
    open.get(&client, false).await.unwrap();
    assert!(!open.get(&client, true).await.unwrap().cache_hit);
    assert_eq!(mock.state.requests.load(Ordering::SeqCst), 12);
}

/// 架電結果の入力欄 (frontend/src/screens/crm/callResultModel.ts の FIELD_PROPERTY) が使う Deal プロパティは、
/// すべて /api/crm/metadata の許可リストに入っていること。外すと画面は「入力欄を表示できません」になる
#[test]
fn deal_allowlist_covers_every_call_result_form_property() {
    for name in [
        "bpo_40", "bpo_42", "bpo_45", "bpo_14", "bpo_10", "bpo_4", "bpo__", "bpo_33", "bpo_13",
        "bpo_16", "bpo_3", "bpo_23", "bpo_57",
    ] {
        assert!(
            allowed_property("deals", name),
            "{name} must stay in the deals allowlist"
        );
    }
    assert!(!allowed_property("deals", "bpo_20"));
    assert!(!allowed_property("contacts", "bpo_57"));
}
