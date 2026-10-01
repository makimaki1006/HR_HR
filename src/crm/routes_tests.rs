//! `/api/crm/*` の結合テスト。
//!
//! - 未ログイン / パスワードログインは本物の `build_app()` に通す (protected_routes への配線ごと確かめる)
//! - Google OIDC のセッションは OIDC フローを通さず、テスト用ルートで注入する
//!   (`crm::router()` + `auth::require_auth` + セッション層だけの小さなアプリ)
//! - 監査 DB は偽 Turso (127.0.0.1)、HubSpot は偽 HubSpot (127.0.0.1)
//!
//! 偽 HubSpot を使う 200 系は本物の HubSpotClient と deep_link を通す。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{header, HeaderMap, Request, StatusCode},
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::db::turso_http::TursoDb;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

type Shared<T> = Arc<Mutex<T>>;

const HUBSPOT_TOKEN: &str = "test-token-XYZ";

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

// ---------------------------------------------------------------------------
// 偽 Turso (監査 DB)。SQL は実行せず、文字列で応答を切り替える
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeTurso {
    /// account_id → (role, disabled_at)
    accounts: HashMap<String, (String, String)>,
    /// この account_id の照会は HTTP 500 を返す
    fail_ids: Vec<String>,
    /// 受け取った SQL
    sqls: Vec<String>,
}

fn text(v: &str) -> Value {
    json!({"type": "text", "value": v})
}

async fn fake_pipeline(State(st): State<Shared<FakeTurso>>, Json(body): Json<Value>) -> Response {
    let stmt = &body["requests"][0]["stmt"];
    let sql = stmt["sql"].as_str().unwrap_or("").to_string();
    let arg0 = stmt["args"][0]["value"].as_str().unwrap_or("").to_string();
    let mut t = st.lock().unwrap();
    t.sqls.push(sql.clone());
    let result = if sql.starts_with("SELECT 1") {
        json!({"cols": [{"name": "1"}], "rows": [[{"type": "integer", "value": "1"}]]})
    } else if sql.starts_with("SELECT id, email, display_name") {
        if t.fail_ids.contains(&arg0) {
            return (StatusCode::INTERNAL_SERVER_ERROR, "boom").into_response();
        }
        let cols = json!([
            {"name": "id"}, {"name": "email"}, {"name": "display_name"}, {"name": "company"},
            {"name": "role"}, {"name": "first_seen_at"}, {"name": "last_login_at"},
            {"name": "login_count"}, {"name": "disabled_at"}
        ]);
        let rows = match t.accounts.get(&arg0) {
            Some((role, disabled)) => json!([[
                text(&arg0),
                text("taro@f-a-c.co.jp"),
                text(""),
                text(""),
                text(role),
                text(""),
                text(""),
                {"type": "integer", "value": "1"},
                if disabled.is_empty() { json!({"type": "null"}) } else { text(disabled) }
            ]]),
            None => json!([]),
        };
        json!({"cols": cols, "rows": rows})
    } else if sql.starts_with("SELECT id, role FROM accounts") {
        // パスワードログイン時の upsert_account: 既存の admin アカウントとして返す
        json!({"cols": [{"name": "id"}, {"name": "role"}],
               "rows": [[text("acc-admin"), text("admin")]]})
    } else if sql.starts_with("SELECT disabled_at FROM accounts") {
        json!({"cols": [{"name": "disabled_at"}], "rows": []})
    } else {
        json!({"cols": [], "rows": [], "affected_row_count": 1})
    };
    Json(json!({"results": [
        {"type": "ok", "response": {"type": "execute", "result": result}},
        {"type": "ok", "response": {"type": "close"}}
    ]}))
    .into_response()
}

async fn start_fake_audit() -> (AuditDb, Shared<FakeTurso>) {
    let mut accounts = HashMap::new();
    accounts.insert(
        "acc-admin".to_string(),
        ("admin".to_string(), String::new()),
    );
    accounts.insert("acc-user".to_string(), ("user".to_string(), String::new()));
    accounts.insert(
        "acc-disabled".to_string(),
        ("admin".to_string(), "2026-09-01T00:00:00Z".to_string()),
    );
    let st: Shared<FakeTurso> = Arc::new(Mutex::new(FakeTurso {
        accounts,
        fail_ids: vec!["acc-error".to_string()],
        ..Default::default()
    }));
    let base = spawn(
        Router::new()
            .route("/v2/pipeline", post(fake_pipeline))
            .with_state(st.clone()),
    )
    .await;
    let turso = tokio::task::spawn_blocking(move || TursoDb::new(&base, "audit-token"))
        .await
        .unwrap()
        .expect("fake turso");
    (AuditDb::new(turso, "salt".to_string()), st)
}

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

/// (object, id) → properties
type ObjectMap = HashMap<(String, String), Vec<(String, Option<String>)>>;
/// (from, id, to) → [(toObjectId, label)]
type AssocMap = HashMap<(String, String, String), Vec<(u64, Option<String>)>>;

#[derive(Default)]
struct FakeHubSpot {
    /// (object, id) → properties
    objects: ObjectMap,
    /// (from, id, to) → [(toObjectId, label)]
    assocs: AssocMap,
    /// true なら全リクエストに 401
    auth_fail: bool,
    /// (method, path?query, Authorization, body)
    requests: Vec<(String, String, String, String)>,
}

impl FakeHubSpot {
    fn obj(&mut self, o: &str, id: &str, props: &[(&str, Option<&str>)]) {
        self.objects.insert(
            (o.to_string(), id.to_string()),
            props
                .iter()
                .map(|(k, v)| (k.to_string(), v.map(str::to_string)))
                .collect(),
        );
    }
    fn assoc(&mut self, from: &str, id: &str, to: &str, targets: &[(u64, Option<&str>)]) {
        self.assocs.insert(
            (from.to_string(), id.to_string(), to.to_string()),
            targets
                .iter()
                .map(|(i, l)| (*i, l.map(str::to_string)))
                .collect(),
        );
    }
    fn record_json(&self, o: &str, id: &str, wanted: &[String]) -> Option<Value> {
        let props = self.objects.get(&(o.to_string(), id.to_string()))?;
        let mut m = serde_json::Map::new();
        for (k, v) in props {
            if wanted.is_empty() || wanted.contains(k) {
                m.insert(k.clone(), v.clone().map_or(Value::Null, Value::String));
            }
        }
        Some(json!({
            "id": id,
            "properties": m,
            "createdAt": "2026-01-02T03:04:05.000Z",
            "updatedAt": "2026-09-01T00:00:00.000Z",
            "archived": false
        }))
    }
}

fn auth_header(h: &HeaderMap) -> String {
    h.get(header::AUTHORIZATION)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

fn unauthorized() -> Response {
    (
        StatusCode::UNAUTHORIZED,
        Json(
            json!({"status": "error", "category": "INVALID_AUTHENTICATION",
                    "message": "Authentication credentials not found"}),
        ),
    )
        .into_response()
}

async fn hs_get_object(
    State(st): State<Shared<FakeHubSpot>>,
    Path((o, id)): Path<(String, String)>,
    RawQuery(q): RawQuery,
    headers: HeaderMap,
) -> Response {
    let mut s = st.lock().unwrap();
    let q = q.unwrap_or_default();
    s.requests.push((
        "GET".into(),
        format!("/crm/v3/objects/{o}/{id}?{q}"),
        auth_header(&headers),
        String::new(),
    ));
    if s.auth_fail {
        return unauthorized();
    }
    let wanted: Vec<String> = reqwest::Url::parse(&format!("http://x/?{q}"))
        .unwrap()
        .query_pairs()
        .filter(|(k, _)| k == "properties")
        .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
        .collect();
    match s.record_json(&o, &id, &wanted) {
        Some(v) => Json(v).into_response(),
        None => (
            StatusCode::NOT_FOUND,
            Json(json!({"status": "error", "category": "OBJECT_NOT_FOUND"})),
        )
            .into_response(),
    }
}

async fn hs_batch_read(
    State(st): State<Shared<FakeHubSpot>>,
    Path(o): Path<String>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.requests.push((
        "POST".into(),
        format!("/crm/v3/objects/{o}/batch/read"),
        auth_header(&headers),
        body.to_string(),
    ));
    if s.auth_fail {
        return unauthorized();
    }
    let wanted: Vec<String> = body["properties"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default();
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| s.record_json(&o, i["id"].as_str().unwrap_or(""), &wanted))
        .collect();
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn hs_assoc(
    State(st): State<Shared<FakeHubSpot>>,
    Path((from, id, to)): Path<(String, String, String)>,
    RawQuery(q): RawQuery,
    headers: HeaderMap,
) -> Response {
    let mut s = st.lock().unwrap();
    s.requests.push((
        "GET".into(),
        format!(
            "/crm/v4/objects/{from}/{id}/associations/{to}?{}",
            q.unwrap_or_default()
        ),
        auth_header(&headers),
        String::new(),
    ));
    if s.auth_fail {
        return unauthorized();
    }
    let results: Vec<Value> = s
        .assocs
        .get(&(from, id, to))
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .map(|(i, label)| {
            json!({"toObjectId": i, "associationTypes": [
                {"category": if label.is_some() { "USER_DEFINED" } else { "HUBSPOT_DEFINED" },
                 "typeId": 4, "label": label}
            ]})
        })
        .collect();
    Json(json!({"results": results})).into_response()
}

async fn start_fake_hubspot(fake: FakeHubSpot) -> (Arc<HubSpotClient>, Shared<FakeHubSpot>) {
    let st = Arc::new(Mutex::new(fake));
    let base = spawn(
        Router::new()
            .route("/crm/v3/objects/{o}/{id}", get(hs_get_object))
            .route("/crm/v3/objects/{o}/batch/read", post(hs_batch_read))
            .route(
                "/crm/v4/objects/{from}/{id}/associations/{to}",
                get(hs_assoc),
            )
            .with_state(st.clone()),
    )
    .await;
    let client = HubSpotClient::new(
        HUBSPOT_TOKEN.into(),
        &base,
        ClientOptions {
            timeout: Duration::from_secs(3),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
            search_min_interval: Duration::from_millis(1),
        },
    )
    .expect("client");
    (Arc::new(client), st)
}

// ---------------------------------------------------------------------------
// AppState / アプリ
// ---------------------------------------------------------------------------

fn test_config() -> AppConfig {
    AppConfig {
        port: 0,
        auth_password: "internal-pass".to_string(),
        auth_password_hash: String::new(),
        external_passwords: Vec::new(),
        allowed_domains: vec!["f-a-c.co.jp".to_string()],
        allowed_domains_extra: Vec::new(),
        hellowork_db_path: String::new(),
        indeed_db_path: String::new(),
        cache_ttl_secs: 60,
        cache_max_entries: 10,
        rate_limit_max_attempts: 50,
        rate_limit_lockout_secs: 60,
        audit_turso_url: String::new(),
        audit_turso_token: String::new(),
        audit_ip_salt: String::new(),
        admin_emails: Vec::new(),
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    }
}

fn test_state(audit: Option<AuditDb>, hubspot: Option<Arc<HubSpotClient>>) -> Arc<AppState> {
    Arc::new(AppState {
        config: test_config(),
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit,
        google_oidc: None,
        hubspot,
    })
}

/// テスト用: セッションに email / login_method / account_id を入れる
async fn inject_session(session: Session, Json(v): Json<Value>) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, v["email"].as_str().unwrap())
        .await
        .unwrap();
    session
        .insert(
            crate::auth::SESSION_LOGIN_METHOD_KEY,
            v["login_method"].as_str().unwrap(),
        )
        .await
        .unwrap();
    if let Some(aid) = v["account_id"].as_str() {
        session
            .insert(crate::SESSION_ACCOUNT_ID_KEY, aid)
            .await
            .unwrap();
    }
    StatusCode::NO_CONTENT
}

/// crm ルート + require_auth + セッション注入ルートだけのアプリ
fn crm_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(super::router())
        .route_layer(middleware::from_fn(crate::auth::require_auth))
        .route("/__test/session", post(inject_session))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

fn session_cookie(resp: &Response) -> Option<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("id="))
        .map(|c| c.split(';').next().unwrap().to_string())
}

async fn login_as(app: &Router, login_method: &str, account_id: Option<&str>) -> String {
    let body = json!({"email": "taro@f-a-c.co.jp", "login_method": login_method,
                      "account_id": account_id});
    let resp = app
        .clone()
        .oneshot(
            Request::post("/__test/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    session_cookie(&resp).expect("session cookie")
}

async fn get_req(app: &Router, uri: &str, cookie: Option<&str>) -> Response {
    let mut b = Request::builder().uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    app.clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn body_string(resp: Response) -> String {
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// (status, Cache-Control, JSON 本文)
async fn get_json(app: &Router, uri: &str, cookie: &str) -> (StatusCode, String, Value) {
    let resp = get_req(app, uri, Some(cookie)).await;
    let status = resp.status();
    let cc = resp
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let body = body_string(resp).await;
    let v: Value = serde_json::from_str(&body).unwrap_or_else(|_| panic!("not json: {body}"));
    (status, cc, v)
}

const ALL_PATHS: [&str; 3] = [
    "/api/crm/contacts/123",
    "/api/crm/companies/123",
    "/api/crm/deals/123",
];

// ---------------------------------------------------------------------------
// 認証・認可 (HubSpot を呼ばない経路)
// ---------------------------------------------------------------------------

/// 本物の build_app で: 未ログインは 303 /login (protected_routes 配下に配線されている)
#[tokio::test(flavor = "multi_thread")]
async fn 未ログインは_303_で_login_へ() {
    let app = crate::build_app(test_state(None, None));
    for p in ALL_PATHS {
        let resp = get_req(&app, p, None).await;
        assert_eq!(resp.status(), StatusCode::SEE_OTHER, "{p}");
        assert_eq!(resp.headers()[header::LOCATION], "/login", "{p}");
    }
}

/// 本物の build_app + 本物のパスワードログインで: role=admin のアカウントでも 403。
/// HubSpot 未設定でも 503 ではなく 403 (未認可の人に設定状況を見せない)
#[tokio::test(flavor = "multi_thread")]
async fn パスワードログインは_admin_でも_403_で_503_を見せない() {
    let (audit, turso) = start_fake_audit().await;
    let app = crate::build_app(test_state(Some(audit), None));
    let resp = app
        .clone()
        .oneshot(
            Request::post("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=taro%40f-a-c.co.jp&password=internal-pass",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        StatusCode::SEE_OTHER,
        "login should redirect"
    );
    let cookie = session_cookie(&resp).expect("session cookie");
    // ログイン時の upsert で admin アカウントとして扱われている (前提の確認)
    assert!(turso
        .lock()
        .unwrap()
        .sqls
        .iter()
        .any(|s| s.starts_with("SELECT id, role FROM accounts")));
    for p in ALL_PATHS {
        let (status, cc, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v, json!({"error_kind": "forbidden"}), "{p}");
        assert_eq!(cc, "no-store", "{p}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_でも_role_user_は_403() {
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "google_oidc", Some("acc-user")).await;
    for p in ALL_PATHS {
        let (status, _, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v["error_kind"], "forbidden");
    }
}

/// 監査未接続 / account_id 無し / 照会失敗 / 未登録 / 無効化済み → すべて 403 (fail closed)
#[tokio::test(flavor = "multi_thread")]
async fn 役割が取れなければ_403() {
    // 監査 DB 未接続
    let app = crm_app(test_state(None, None));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/1", &cookie).await;
    assert_eq!(
        (status, v),
        (StatusCode::FORBIDDEN, json!({"error_kind": "forbidden"}))
    );

    let (audit, turso) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    for aid in [
        None,
        Some("acc-error"),
        Some("acc-unknown"),
        Some("acc-disabled"),
    ] {
        let cookie = login_as(&app, "google_oidc", aid).await;
        let (status, _, v) = get_json(&app, "/api/crm/deals/1", &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{aid:?}");
        assert_eq!(v["error_kind"], "forbidden", "{aid:?}");
    }
    // acc-error / acc-unknown / acc-disabled の 3 回は実際に照会している (account_id 無しは照会しない)
    let lookups = turso
        .lock()
        .unwrap()
        .sqls
        .iter()
        .filter(|s| s.starts_with("SELECT id, email, display_name"))
        .count();
    assert_eq!(lookups, 3);
}

/// 認可を通過した証拠: admin + OIDC で HubSpot 未設定なら 503 not_configured
#[tokio::test(flavor = "multi_thread")]
async fn oidc_admin_で_hubspot_未設定なら_503() {
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    for p in ALL_PATHS {
        let (status, cc, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{p}");
        assert_eq!(v, json!({"error_kind": "not_configured"}), "{p}");
        assert_eq!(cc, "no-store", "{p}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 不正な_id_は_400() {
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    for kind in ["contacts", "companies", "deals"] {
        for id in ["abc", "12a", "123456789012345678901", "%EF%BC%91"] {
            let (status, _, v) = get_json(&app, &format!("/api/crm/{kind}/{id}"), &cookie).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{kind}/{id}");
            assert_eq!(v, json!({"error_kind": "invalid_id"}), "{kind}/{id}");
        }
        // 20 桁ちょうどは id として通る (HubSpot 未設定なので 503 まで進む)
        let (status, _, _) = get_json(
            &app,
            &format!("/api/crm/{kind}/12345678901234567890"),
            &cookie,
        )
        .await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE, "{kind}");
    }
}

/// 未認可の人は不正 id でも 400 ではなく 403 (認可が先)
#[tokio::test(flavor = "multi_thread")]
async fn 未認可なら不正_id_でも_403() {
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "password_internal", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/abc", &cookie).await;
    assert_eq!(
        (status, v),
        (StatusCode::FORBIDDEN, json!({"error_kind": "forbidden"}))
    );
}

// ---------------------------------------------------------------------------
// 200 系 (偽 HubSpot)。HubSpotClient (A) / deep_link (B) の実装待ち
// ---------------------------------------------------------------------------

fn portal() -> String {
    crate::hubspot::deep_link::hubspot_portal_id()
}

/// Deal 900 を中心にした偽データ
///
/// - Deal 900 直付き: call 1001 (09-10), note 2001 (09-05)
/// - Deal → contacts 55 (label "Decision maker"), 56 / companies 300
/// - Contact 55 → calls 1001 (Deal 直付きと重複), 1002 (09-20, Zoom 純正連携)
/// - Contact 56 → calls 1002 (Contact 55 と重複), 1003 (08-01)
fn deal_fixture() -> FakeHubSpot {
    let mut f = FakeHubSpot::default();
    f.obj(
        "deals",
        "900",
        &[
            ("dealname", Some("介護スタッフ採用支援")),
            ("dealstage", Some("123456789")),
            ("pipeline", Some("default")),
            ("amount", Some("500000")),
            ("closedate", Some("2026-10-31T00:00:00Z")),
            ("hubspot_owner_id", Some("777")),
        ],
    );
    f.assoc(
        "deals",
        "900",
        "contacts",
        &[(55, Some("Decision maker")), (56, None)],
    );
    f.assoc("deals", "900", "companies", &[(300, None)]);
    f.assoc("deals", "900", "calls", &[(1001, None)]);
    f.assoc("deals", "900", "notes", &[(2001, None)]);
    f.assoc("contacts", "55", "calls", &[(1001, None), (1002, None)]);
    f.assoc("contacts", "56", "calls", &[(1002, None), (1003, None)]);
    f.obj(
        "calls",
        "1001",
        &[
            ("hs_timestamp", Some("2026-09-10T01:00:00Z")),
            ("hs_call_title", Some("条件すり合わせ")),
            ("hs_call_source", Some("CALLING_TOOL")),
            ("hs_call_duration", Some("300000")),
        ],
    );
    f.obj(
        "calls",
        "1002",
        &[
            ("hs_timestamp", Some("2026-09-20T05:30:00.000Z")),
            ("hs_call_title", Some("初回ヒアリング")),
            ("hs_call_source", Some("INTEGRATIONS_PLATFORM")),
            ("hs_call_direction", Some("OUTBOUND")),
        ],
    );
    f.obj(
        "calls",
        "1003",
        &[
            ("hs_timestamp", Some("2026-08-01T00:00:00Z")),
            ("hs_call_title", Some("折り返し")),
            ("hs_call_source", None),
        ],
    );
    f.obj(
        "notes",
        "2001",
        &[
            ("hs_timestamp", Some("2026-09-05T00:00:00Z")),
            ("hs_note_body", Some("見積提出済み")),
        ],
    );
    f
}

#[tokio::test(flavor = "multi_thread")]
async fn deal_200_は_contact_経由の_call_も含め重複を除いて時刻降順() {
    let (audit, _) = start_fake_audit().await;
    let (client, hs) = start_fake_hubspot(deal_fixture()).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, cc, v) = get_json(&app, "/api/crm/deals/900", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(cc, "no-store");
    let portal = portal();

    assert_eq!(v["object_type"], "deal");
    assert_eq!(v["id"], "900");
    assert_eq!(v["properties"]["dealname"], "介護スタッフ採用支援");
    // dealstage は ID のまま (ラベルを推測しない)
    assert_eq!(v["properties"]["dealstage"], "123456789");
    assert_eq!(v["properties"]["amount"], "500000");
    assert_eq!(v["created_at"], "2026-01-02T03:04:05.000Z");
    assert_eq!(
        v["deep_link"],
        format!("https://app.hubspot.com/contacts/{portal}/record/0-3/900/")
    );

    // 関連: 自分の型 (deals) は含めない
    let assoc = &v["associations"];
    assert!(assoc.get("deals").is_none(), "{assoc}");
    assert_eq!(assoc["contacts"][0]["id"], "55");
    assert_eq!(assoc["contacts"][0]["labels"], json!(["Decision maker"]));
    assert_eq!(
        assoc["contacts"][0]["deep_link"],
        format!("https://app.hubspot.com/contacts/{portal}/record/0-1/55/")
    );
    assert_eq!(assoc["contacts"][1]["id"], "56");
    assert_eq!(assoc["contacts"][1]["labels"], json!([]));
    assert_eq!(assoc["companies"][0]["id"], "300");
    assert_eq!(
        assoc["companies"][0]["deep_link"],
        format!("https://app.hubspot.com/contacts/{portal}/record/0-2/300/")
    );
    assert_eq!(
        assoc["truncated"],
        json!({"contacts": false, "companies": false})
    );

    // 直近アクティビティ: 1002(09-20, contact 55 経由) > 1001(09-10, deal 直付きを優先)
    //   > 2001(09-05 note) > 1003(08-01, contact 56 経由)。重複 (1001, 1002) は 1 件ずつ
    let acts = v["recent_activities"].as_array().unwrap();
    let got: Vec<(String, String, String, String)> = acts
        .iter()
        .map(|a| {
            (
                a["type"].as_str().unwrap().to_string(),
                a["id"].as_str().unwrap().to_string(),
                a["via"]["object_type"].as_str().unwrap().to_string(),
                a["via"]["id"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    let want: Vec<(String, String, String, String)> = [
        ("call", "1002", "contact", "55"),
        ("call", "1001", "deal", "900"),
        ("note", "2001", "deal", "900"),
        ("call", "1003", "contact", "56"),
    ]
    .iter()
    .map(|(a, b, c, d)| (a.to_string(), b.to_string(), c.to_string(), d.to_string()))
    .collect();
    assert_eq!(got, want);
    assert_eq!(acts[0]["timestamp"], "2026-09-20T05:30:00.000Z");
    assert_eq!(
        acts[0]["properties"]["hs_call_source"],
        "INTEGRATIONS_PLATFORM"
    );
    assert_eq!(acts[0]["properties"]["hs_call_title"], "初回ヒアリング");
    assert_eq!(acts[2]["properties"]["hs_note_body"], "見積提出済み");
    assert_eq!(acts[3]["properties"]["hs_call_source"], Value::Null);

    assert_eq!(v["meta"]["hubspot_portal_id"], portal);
    assert_eq!(
        v["meta"]["data_scope"],
        "HubSpot の読み取り結果。書き込みはしない"
    );
    assert_eq!(v["meta"]["activities_truncated"], false);

    // HubSpot への要求: Bearer トークン、contact → calls を 2 件とも辿った、Call は hs_call_source を要求
    let reqs = hs.lock().unwrap().requests.clone();
    assert!(reqs
        .iter()
        .all(|r| r.2 == format!("Bearer {HUBSPOT_TOKEN}")));
    for cid in ["55", "56"] {
        assert!(
            reqs.iter().any(|r| r.1.starts_with(&format!(
                "/crm/v4/objects/contacts/{cid}/associations/calls"
            ))),
            "{reqs:?}"
        );
    }
    let call_batches: Vec<&String> = reqs
        .iter()
        .filter(|r| r.1 == "/crm/v3/objects/calls/batch/read")
        .map(|r| &r.3)
        .collect();
    assert!(!call_batches.is_empty());
    for b in call_batches {
        assert!(b.contains("hs_call_source"), "{b}");
    }

    // 本文にトークンが無い
    let text = v.to_string();
    assert!(!text.contains(HUBSPOT_TOKEN), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn contact_200_は具体値と直近_10_件() {
    let (audit, _) = start_fake_audit().await;
    let mut f = FakeHubSpot::default();
    f.obj(
        "contacts",
        "55",
        &[
            ("firstname", Some("太郎")),
            ("lastname", Some("山田")),
            ("email", Some("yamada@example.com")),
            ("phone", None),
            ("lifecyclestage", Some("opportunity")),
        ],
    );
    f.assoc("contacts", "55", "deals", &[(900, None)]);
    f.assoc("contacts", "55", "calls", &[(1001, None), (1002, None)]);
    f.obj(
        "calls",
        "1001",
        &[("hs_timestamp", Some("2026-09-10T01:00:00Z"))],
    );
    f.obj(
        "calls",
        "1002",
        &[("hs_timestamp", Some("2026-09-20T05:30:00Z"))],
    );
    let note_ids: Vec<(u64, Option<&str>)> = (3001..=3012).map(|i| (i, None)).collect();
    f.assoc("contacts", "55", "notes", &note_ids);
    for i in 1..=12u64 {
        let ts = format!("2026-07-{i:02}T00:00:00Z");
        f.obj(
            "notes",
            &(3000 + i).to_string(),
            &[("hs_timestamp", Some(&ts))],
        );
    }
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/contacts/55", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["object_type"], "contact");
    assert_eq!(v["properties"]["firstname"], "太郎");
    assert_eq!(v["properties"]["email"], "yamada@example.com");
    assert_eq!(v["properties"]["phone"], Value::Null);
    assert_eq!(
        v["deep_link"],
        format!(
            "https://app.hubspot.com/contacts/{}/record/0-1/55/",
            portal()
        )
    );
    assert!(v["associations"].get("contacts").is_none());
    assert_eq!(v["associations"]["deals"][0]["id"], "900");
    assert_eq!(v["associations"]["companies"], json!([]));

    let ids: Vec<&str> = v["recent_activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    // 14 件中、時刻降順の上位 10 件
    assert_eq!(
        ids,
        vec!["1002", "1001", "3012", "3011", "3010", "3009", "3008", "3007", "3006", "3005"]
    );
    assert!(v["recent_activities"]
        .as_array()
        .unwrap()
        .iter()
        .all(|a| a["via"] == json!({"object_type": "contact", "id": "55"})));
}

#[tokio::test(flavor = "multi_thread")]
async fn hubspot_401_は_502_hubspot_auth_でトークンを出さない() {
    let (audit, _) = start_fake_audit().await;
    let mut f = deal_fixture();
    f.auth_fail = true;
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let resp = get_req(&app, "/api/crm/deals/900", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store");
    let body = body_string(resp).await;
    assert!(!body.contains(HUBSPOT_TOKEN), "{body}");
    assert!(!body.contains("INVALID_AUTHENTICATION"), "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error_kind"], "hubspot_auth");
    assert_eq!(v["message"], "HubSpot の認証に失敗しました (401)");
}

#[tokio::test(flavor = "multi_thread")]
async fn 存在しない_id_は_404_not_found() {
    let (audit, _) = start_fake_audit().await;
    let (client, _) = start_fake_hubspot(deal_fixture()).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/companies/424242", &cookie).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["error_kind"], "not_found");
}
