//! `/api/crm/*` の結合テスト。
//!
//! - 未ログイン / パスワードログインは本物の `build_app()` に通す (auth_middleware の外への配線ごと確かめる)
//! - Google OIDC のセッションは OIDC フローを通さず、テスト用ルートで注入する
//!   (`crm::router(許可メール)` + セッション層だけの小さなアプリ)
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
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::rbac::{CrmAccess, CrmRole};
use super::routes::{read_response, MAX_HUBSPOT_CALLS_PER_REQUEST};
use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::db::turso_http::TursoDb;
use crate::hubspot::{ClientOptions, HubSpotClient, RecordType};
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
    /// `accounts.disabled_at` が入っている email (無効化されたアカウント)
    disabled_emails: Vec<String>,
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
        if t.disabled_emails.contains(&arg0) {
            json!({"cols": [{"name": "disabled_at"}], "rows": [[text("2026-09-01T00:00:00Z")]]})
        } else {
            json!({"cols": [{"name": "disabled_at"}], "rows": []})
        }
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
    /// object → HTTP status: その object の batch read だけ失敗させる
    fail_batch_read: HashMap<String, u16>,
    /// Some(status) なら v4 batch associations を失敗させる
    fail_assoc_batch: Option<u16>,
    /// 本体 GET の応答を遅らせる
    get_delay: Duration,
    /// true なら `associations=` 付きの本体 GET だけ 403 (関連型のスコープ不足の再現)
    forbid_get_with_associations: bool,
    /// archived: true で返す (object, id)
    archived: std::collections::HashSet<(String, String)>,
    /// 本体 GET の同時実行数の観測 (現在 / 最大)
    inflight: usize,
    max_inflight: usize,
    /// Some(status) なら本体 GET を全部この status で返す (本文に上流の秘密文字列を入れる)
    get_status: Option<u16>,
    /// 定義 API (properties / pipelines) の応答: 0 正常 / 1 results 欠落 / 2 429 (秘密の本文)
    meta_mode: u8,
}

/// 上流のエラー本文に入れる文字列。ブラウザへの応答に出てはいけない
const UPSTREAM_SECRET: &str = "UPSTREAM-SECRET-BODY";

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
            "archived": self.archived.contains(&(o.to_string(), id.to_string()))
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

/// `GET /crm/v3/objects/{o}/{id}?properties=..&associations=a,b`。
/// 関連は応答の `associations.{type}.results[].id` に入れる (client 実装と同じ形。
/// [推測] の形であり、実データでの確認はユーザー承認後)。
async fn hs_get_object(
    State(st): State<Shared<FakeHubSpot>>,
    Path((o, id)): Path<(String, String)>,
    RawQuery(q): RawQuery,
    headers: HeaderMap,
) -> Response {
    let (resp, delay) = {
        let mut s = st.lock().unwrap();
        let q = q.unwrap_or_default();
        s.requests.push((
            "GET".into(),
            format!("/crm/v3/objects/{o}/{id}?{q}"),
            auth_header(&headers),
            String::new(),
        ));
        let delay = s.get_delay;
        s.inflight += 1;
        s.max_inflight = s.max_inflight.max(s.inflight);
        let resp = if s.auth_fail {
            unauthorized()
        } else if let Some(code) = s.get_status {
            (
                StatusCode::from_u16(code).unwrap(),
                [("retry-after", "0")],
                UPSTREAM_SECRET,
            )
                .into_response()
        } else if s.forbid_get_with_associations && q.contains("associations=") {
            (
                StatusCode::FORBIDDEN,
                Json(json!({"status": "error", "category": "MISSING_SCOPES"})),
            )
                .into_response()
        } else {
            let pairs: Vec<(String, String)> = reqwest::Url::parse(&format!("http://x/?{q}"))
                .unwrap()
                .query_pairs()
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect();
            let split = |key: &str| -> Vec<String> {
                pairs
                    .iter()
                    .filter(|(k, _)| k == key)
                    .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
                    .collect()
            };
            let wanted = split("properties");
            let want_assoc = split("associations");
            match s.record_json(&o, &id, &wanted) {
                Some(mut v) => {
                    let mut assoc_obj = serde_json::Map::new();
                    for t in &want_assoc {
                        let targets = s
                            .assocs
                            .get(&(o.clone(), id.clone(), t.clone()))
                            .cloned()
                            .unwrap_or_default();
                        if targets.is_empty() {
                            continue; // 関連が無い型はキーごと出さない
                        }
                        let results: Vec<Value> = targets
                            .iter()
                            .map(|(i, _)| json!({"id": i.to_string(), "type": "x_to_y"}))
                            .collect();
                        assoc_obj.insert(t.clone(), json!({ "results": results }));
                    }
                    if !assoc_obj.is_empty() {
                        v["associations"] = Value::Object(assoc_obj);
                    }
                    Json(v).into_response()
                }
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"status": "error", "category": "OBJECT_NOT_FOUND"})),
                )
                    .into_response(),
            }
        };
        (resp, delay)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    {
        let mut s = st.lock().unwrap();
        s.inflight -= 1;
    }
    resp
}

/// `GET /crm/v3/properties/{object}`
async fn hs_properties(
    State(st): State<Shared<FakeHubSpot>>,
    Path(o): Path<String>,
    headers: HeaderMap,
) -> Response {
    let mut s = st.lock().unwrap();
    s.requests.push((
        "GET".into(),
        format!("/crm/v3/properties/{o}"),
        auth_header(&headers),
        String::new(),
    ));
    match s.meta_mode {
        1 => Json(json!({"unexpected": "shape"})).into_response(),
        2 => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "0")],
            UPSTREAM_SECRET,
        )
            .into_response(),
        _ => {
            let name = match o.as_str() {
                "contacts" => "firstname",
                "companies" => "industry",
                _ => "bpo_42",
            };
            Json(json!({"results": [
                {"name": name, "label": "項目", "type": "enumeration", "fieldType": "select",
                 "options": [{"label": "A", "value": "a"}]},
                {"name": "not_reviewed", "label": "x", "type": "string", "fieldType": "text"}
            ]}))
            .into_response()
        }
    }
}

/// `GET /crm/v3/pipelines/deals`
async fn hs_pipelines(State(st): State<Shared<FakeHubSpot>>, headers: HeaderMap) -> Response {
    let mut s = st.lock().unwrap();
    s.requests.push((
        "GET".into(),
        "/crm/v3/pipelines/deals".into(),
        auth_header(&headers),
        String::new(),
    ));
    match s.meta_mode {
        1 => Json(json!({"unexpected": "shape"})).into_response(),
        2 => (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "0")],
            UPSTREAM_SECRET,
        )
            .into_response(),
        _ => Json(json!({"results": [
            {"id": "p1", "label": "営業", "displayOrder": 0,
             "stages": [{"id": "s2", "label": "商談", "displayOrder": 1},
                        {"id": "s1", "label": "新規", "displayOrder": 0}]}
        ]}))
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
    if let Some(status) = s.fail_batch_read.get(&o) {
        return (
            StatusCode::from_u16(*status).unwrap(),
            Json(json!({"status": "error", "category": "MISSING_SCOPES"})),
        )
            .into_response();
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

/// `POST /crm/v4/associations/{from}/{to}/batch/read`。関連の無い from は結果に含めない
async fn hs_assoc_batch(
    State(st): State<Shared<FakeHubSpot>>,
    Path((from, to)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.requests.push((
        "POST".into(),
        format!("/crm/v4/associations/{from}/{to}/batch/read"),
        auth_header(&headers),
        body.to_string(),
    ));
    if s.auth_fail {
        return unauthorized();
    }
    if let Some(status) = s.fail_assoc_batch {
        return (
            StatusCode::from_u16(status).unwrap(),
            Json(json!({"status": "error", "category": "OBJECT_NOT_FOUND"})),
        )
            .into_response();
    }
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| {
            let fid = i["id"].as_str().unwrap_or("").to_string();
            let targets = s.assocs.get(&(from.clone(), fid.clone(), to.clone()))?;
            if targets.is_empty() {
                return None;
            }
            let to_list: Vec<Value> = targets
                .iter()
                .map(|(t, label)| {
                    json!({"toObjectId": t, "associationTypes": [
                        {"category": "HUBSPOT_DEFINED", "typeId": 194, "label": label}]})
                })
                .collect();
            Some(json!({"from": {"id": fid}, "to": to_list}))
        })
        .collect();
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn start_fake_hubspot(fake: FakeHubSpot) -> (Arc<HubSpotClient>, Shared<FakeHubSpot>) {
    let st = Arc::new(Mutex::new(fake));
    let base = spawn(
        Router::new()
            .route("/crm/v3/properties/{o}", get(hs_properties))
            .route("/crm/v3/pipelines/deals", get(hs_pipelines))
            .route("/crm/v3/objects/{o}/{id}", get(hs_get_object))
            .route("/crm/v3/objects/{o}/batch/read", post(hs_batch_read))
            .route(
                "/crm/v4/associations/{from}/{to}/batch/read",
                post(hs_assoc_batch),
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
            rate_limited_min_wait: Duration::from_millis(1),
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

const TEST_EMAIL: &str = "taro@f-a-c.co.jp";

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

/// crm ルート + セッション注入ルートだけのアプリ
/// (役割は consultant 固定 = レコード全件を読める。BPO / user の判定は `roles_tests.rs`)
fn crm_app(state: Arc<AppState>) -> Router {
    crm_app_with(
        state,
        CrmAccess::from_list(TEST_EMAIL).with_test_role(TEST_EMAIL, CrmRole::Admin),
    )
}

/// 許可メールを指定する版
fn crm_app_with(state: Arc<AppState>, access: CrmAccess) -> Router {
    Router::new()
        .merge(super::router(access))
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
    login_as_email(app, TEST_EMAIL, login_method, account_id).await
}

async fn login_as_email(
    app: &Router,
    email: &str,
    login_method: &str,
    account_id: Option<&str>,
) -> String {
    let body = json!({"email": email, "login_method": login_method,
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

/// 本物の build_app で: 未ログインは HTML の /login への 303 ではなく JSON の 401
/// (fetch から呼ぶ API。auth_middleware の外に配線されている)
#[tokio::test(flavor = "multi_thread")]
async fn 未ログインは_json_401_で_login_へ飛ばさない() {
    let app = crate::build_app(test_state(None, None));
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let resp = get_req(&app, p, None).await;
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED, "{p}");
        assert!(resp.headers().get(header::LOCATION).is_none(), "{p}");
        assert_eq!(resp.headers()[header::CACHE_CONTROL], "no-store", "{p}");
        let v: Value = serde_json::from_str(&body_string(resp).await).unwrap();
        assert_eq!(v, json!({"error_kind": "login_required"}), "{p}");
    }
}

/// 本物の build_app + 本物のパスワードログインで: 403 `google_login_required`。
/// HubSpot 未設定でも 503 ではなく 403 (未認可の人に設定状況を見せない)
#[tokio::test(flavor = "multi_thread")]
async fn パスワードログインは_403_で_503_を見せない() {
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
    // ログイン時の upsert で admin アカウントとして扱われている (前提の確認: admin でも通らない)
    assert!(turso
        .lock()
        .unwrap()
        .sqls
        .iter()
        .any(|s| s.starts_with("SELECT id, role FROM accounts")));
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let (status, cc, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v, json!({"error_kind": "google_login_required"}), "{p}");
        assert_eq!(cc, "no-store", "{p}");
    }
}

/// 共有 / 外部パスワードのログイン方式はどれも、許可リストに載っているメールでも 403
#[tokio::test(flavor = "multi_thread")]
async fn パスワード系のログイン方式はすべて_403() {
    let app = crm_app(test_state(None, None));
    for method in ["password", "password_internal", "password_external", ""] {
        let cookie = login_as(&app, method, None).await;
        for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
            let (status, _, v) = get_json(&app, p, &cookie).await;
            assert_eq!(status, StatusCode::FORBIDDEN, "{method} {p}");
            assert_eq!(v["error_kind"], "google_login_required", "{method} {p}");
        }
    }
}

/// OIDC でも許可リストに無いメールは 403 (役割が admin でも関係ない)
#[tokio::test(flavor = "multi_thread")]
async fn oidc_でも許可リスト外は_403() {
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as_email(&app, "hanako@f-a-c.co.jp", "google_oidc", Some("acc-admin")).await;
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let (status, _, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v, json!({"error_kind": "forbidden"}), "{p}");
    }
}

/// 許可リストが空なら絞り込まない: 会社ドメインの Google ログインなら (役割・accounts の行が無くても) 認可を通り、
/// HubSpot 未設定の 503 まで進む。社外ドメインは空リストでも 403 (決定 2026-10-07)
#[tokio::test(flavor = "multi_thread")]
async fn 許可リストが空でも会社ドメインなら通り_社外は_403() {
    let app = crm_app_with(test_state(None, None), CrmAccess::from_list(""));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let (status, _, v) = get_json(&app, p, &cookie).await;
        assert_ne!(status, StatusCode::FORBIDDEN, "{p}: {v}");
        assert_ne!(status, StatusCode::UNAUTHORIZED, "{p}: {v}");
    }
    let outsider = login_as_email(&app, "taro@example.com", "google_oidc", None).await;
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let (status, _, v) = get_json(&app, p, &outsider).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v["error_kind"], "forbidden", "{p}");
    }
}

/// 許可リストに載っていても、監査 DB で無効化されたアカウントは 403。
/// 無効化されていなければ同じ構成で認可を通る (逆証明)
#[tokio::test(flavor = "multi_thread")]
async fn 無効化されたアカウントは_403() {
    let (audit, turso) = start_fake_audit().await;
    turso
        .lock()
        .unwrap()
        .disabled_emails
        .push(TEST_EMAIL.to_string());
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "google_oidc", None).await;
    for p in ALL_PATHS.iter().chain(&["/api/crm/metadata"]) {
        let (status, _, v) = get_json(&app, p, &cookie).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert_eq!(v["error_kind"], "account_disabled", "{p}");
    }
    let (audit, _) = start_fake_audit().await;
    let app = crm_app(test_state(Some(audit), None));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/1", &cookie).await;
    assert_eq!(
        (status, v),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"error_kind": "not_configured"})
        )
    );
}

/// 認可を通過した証拠: 許可リストの人が OIDC で入り、HubSpot 未設定なら 503 not_configured
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
        (
            StatusCode::FORBIDDEN,
            json!({"error_kind": "google_login_required"})
        )
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
    // v3 の「本体 + 関連」形では関連ラベルは取れないので空 (ラベル付きは v4 の関連 API のみ)
    assert_eq!(assoc["contacts"][0]["labels"], json!([]));
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

    // HubSpot への要求: Bearer トークン、contact → calls は 1 回の batch で 2 件とも辿った、
    // Call は hs_call_source を要求。回数は 本体 1 + contact→calls 1 + batch read (calls, notes) 2 = 4
    let reqs = hs.lock().unwrap().requests.clone();
    assert_eq!(reqs.len(), 4, "{reqs:?}");
    assert!(reqs
        .iter()
        .all(|r| r.2 == format!("Bearer {HUBSPOT_TOKEN}")));
    let assoc_batches: Vec<&(String, String, String, String)> = reqs
        .iter()
        .filter(|r| r.1 == "/crm/v4/associations/contacts/calls/batch/read")
        .collect();
    assert_eq!(assoc_batches.len(), 1);
    let body: Value = serde_json::from_str(&assoc_batches[0].3).unwrap();
    assert_eq!(body, json!({"inputs": [{"id": "55"}, {"id": "56"}]}));
    // 本体の GET は 1 回で関連型をまとめて要求し、email は要求しない
    let main_get = &reqs[0];
    assert!(
        main_get.1.starts_with("/crm/v3/objects/deals/900?"),
        "{main_get:?}"
    );
    assert!(
        main_get
            .1
            .contains("associations=contacts%2Ccompanies%2Ccalls%2Cnotes%2Ctasks%2Cmeetings")
            || main_get
                .1
                .contains("associations=contacts,companies,calls,notes,tasks,meetings"),
        "{main_get:?}"
    );
    assert!(!main_get.1.contains("emails"), "{main_get:?}");
    let call_batches: Vec<&String> = reqs
        .iter()
        .filter(|r| r.1 == "/crm/v3/objects/calls/batch/read")
        .map(|r| &r.3)
        .collect();
    assert_eq!(call_batches.len(), 1);
    for b in call_batches {
        assert!(b.contains("hs_call_source"), "{b}");
    }
    assert_eq!(v["meta"]["partial"], json!([]));

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
    let (client, hs) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/contacts/55", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    // 本体 1 + batch read (calls, notes) 2 = 3 回
    assert_eq!(hs.lock().unwrap().requests.len(), 3);
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

// ---------------------------------------------------------------------------
// 呼び出し回数の上限・締め切り・部分失敗・並び順
// ---------------------------------------------------------------------------

/// Deal 900: contact 30 件 (各 5 call)、直付き call 1 / note 150 / task 1 / meeting 1
fn big_deal_fixture() -> FakeHubSpot {
    let mut f = FakeHubSpot::default();
    f.obj("deals", "900", &[("dealname", Some("大きい Deal"))]);
    let contacts: Vec<(u64, Option<&str>)> = (1..=30u64).map(|c| (c, None)).collect();
    f.assoc("deals", "900", "contacts", &contacts);
    for c in 1..=30u64 {
        let calls: Vec<(u64, Option<&str>)> =
            (1..=5u64).map(|k| (100_000 + c * 10 + k, None)).collect();
        f.assoc("contacts", &c.to_string(), "calls", &calls);
        for (cid, _) in &calls {
            let ts = format!("2026-08-{:02}T00:00:00Z", (c % 28) + 1);
            f.obj("calls", &cid.to_string(), &[("hs_timestamp", Some(&ts))]);
        }
    }
    f.assoc("deals", "900", "calls", &[(1001, None)]);
    f.obj(
        "calls",
        "1001",
        &[("hs_timestamp", Some("2026-09-10T00:00:00Z"))],
    );
    let notes: Vec<(u64, Option<&str>)> = (3001..=3150u64).map(|i| (i, None)).collect();
    f.assoc("deals", "900", "notes", &notes);
    for (i, _) in &notes {
        f.obj(
            "notes",
            &i.to_string(),
            &[("hs_timestamp", Some("2026-07-01T00:00:00Z"))],
        );
    }
    f.assoc("deals", "900", "tasks", &[(4001, None)]);
    f.obj(
        "tasks",
        "4001",
        &[("hs_createdate", Some("2026-09-01T00:00:00Z"))],
    );
    f.assoc("deals", "900", "meetings", &[(5001, None)]);
    f.obj(
        "meetings",
        "5001",
        &[("hs_timestamp", Some("2026-09-02T00:00:00Z"))],
    );
    f
}

fn batch_inputs(reqs: &[(String, String, String, String)], path: &str) -> Vec<Vec<String>> {
    reqs.iter()
        .filter(|r| r.1 == path)
        .map(|r| {
            let b: Value = serde_json::from_str(&r.3).unwrap();
            b["inputs"]
                .as_array()
                .unwrap()
                .iter()
                .map(|i| i["id"].as_str().unwrap().to_string())
                .collect()
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn 大きい_deal_でも_hubspot_呼び出しは_6_回で打ち切りが出る() {
    let (client, hs) = start_fake_hubspot(big_deal_fixture()).await;
    let resp = read_response(
        &client,
        RecordType::Deal,
        "900",
        "1",
        MAX_HUBSPOT_CALLS_PER_REQUEST,
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: Value = serde_json::from_str(&body_string(resp).await).unwrap();

    let reqs = hs.lock().unwrap().requests.clone();
    // 期待値 = 本体 1 + contact→calls 1 + batch read 4 (calls / notes / tasks / meetings) = 6
    assert_eq!(
        reqs.len(),
        6,
        "{:?}",
        reqs.iter().map(|r| &r.1).collect::<Vec<_>>()
    );
    assert!(reqs.len() <= MAX_HUBSPOT_CALLS_PER_REQUEST);

    // contact は 30 件中 ID の新しい 20 件 (30〜11) だけ辿る (1 回の batch)
    let assoc_inputs = batch_inputs(&reqs, "/crm/v4/associations/contacts/calls/batch/read");
    assert_eq!(assoc_inputs.len(), 1);
    let newest20: Vec<String> = (11..=30u64).rev().map(|c| c.to_string()).collect();
    assert_eq!(assoc_inputs[0], newest20);

    // 型ごとに batch read は 1 回、calls / notes は 100 件で打ち切り
    let calls_in = batch_inputs(&reqs, "/crm/v3/objects/calls/batch/read");
    let notes_in = batch_inputs(&reqs, "/crm/v3/objects/notes/batch/read");
    assert_eq!(calls_in.len(), 1);
    assert_eq!(notes_in.len(), 1);
    assert_eq!(calls_in[0].len(), 100);
    assert_eq!(notes_in[0].len(), 100);
    // 打ち切りは直付き・経由を区別せず ID の新しい 100 件。
    // call: 経由 100 件 (contact 11〜30 × 5) が直付き 1001 より新しいので 1001 は落ちる。
    // note: 直付き 150 件 (3001〜3150) のうち 3150〜3051。
    let mut expect_calls: Vec<String> = (11..=30u64)
        .flat_map(|c| (1..=5u64).map(move |k| (100_000 + c * 10 + k).to_string()))
        .collect();
    expect_calls.sort_by_key(|b| std::cmp::Reverse(b.parse::<u64>().unwrap()));
    assert_eq!(calls_in[0], expect_calls);
    assert!(!calls_in[0].contains(&"1001".to_string()));
    assert_eq!(notes_in[0][0], "3150");
    assert_eq!(notes_in[0][99], "3051");
    assert_eq!(
        batch_inputs(&reqs, "/crm/v3/objects/tasks/batch/read").len(),
        1
    );
    assert_eq!(
        batch_inputs(&reqs, "/crm/v3/objects/meetings/batch/read").len(),
        1
    );

    assert_eq!(v["meta"]["activities_truncated"], true);
    assert_eq!(v["meta"]["partial"], json!([]));
    assert_eq!(v["recent_activities"].as_array().unwrap().len(), 10);
    // 関連 contact は 30 件ともそのまま表示 (打ち切るのはアクティビティ取得のための辿り先だけ)
    assert_eq!(v["associations"]["contacts"].as_array().unwrap().len(), 30);
}

/// 逆証明: 上限を 2 にすると 2 回で止まり、取得しなかった部分が partial に call_budget で出る
#[tokio::test(flavor = "multi_thread")]
async fn 呼び出し上限を小さくすると_call_budget_で止まる() {
    let (client, hs) = start_fake_hubspot(big_deal_fixture()).await;
    let resp = read_response(
        &client,
        RecordType::Deal,
        "900",
        "1",
        2,
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: Value = serde_json::from_str(&body_string(resp).await).unwrap();
    let reqs = hs.lock().unwrap().requests.clone();
    // 本体 1 + contact→calls 1 で上限 2。以降の batch read は呼ばない
    assert_eq!(
        reqs.len(),
        2,
        "{:?}",
        reqs.iter().map(|r| &r.1).collect::<Vec<_>>()
    );
    assert_eq!(
        v["meta"]["partial"],
        json!([
            {"part": "calls", "error_kind": "call_budget"},
            {"part": "notes", "error_kind": "call_budget"},
            {"part": "tasks", "error_kind": "call_budget"},
            {"part": "meetings", "error_kind": "call_budget"},
        ])
    );
    assert_eq!(v["recent_activities"], json!([]));
    // 本体と関連は返る (全体はエラーにしない)
    assert_eq!(v["properties"]["dealname"], "大きい Deal");
}

#[tokio::test(flavor = "multi_thread")]
async fn 締め切りを超えたら_504_crm_timeout() {
    let mut f = deal_fixture();
    f.get_delay = Duration::from_millis(2000);
    let (client, _) = start_fake_hubspot(f).await;
    let t = std::time::Instant::now();
    let resp = read_response(
        &client,
        RecordType::Deal,
        "900",
        "1",
        MAX_HUBSPOT_CALLS_PER_REQUEST,
        Duration::from_millis(100),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    // HubSpot の応答 (2 秒) を待たずに返る (高負荷でも揺れないよう余裕を大きく取る)
    assert!(
        t.elapsed() < Duration::from_millis(1500),
        "{:?}",
        t.elapsed()
    );
    let v: Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["error_kind"], "crm_timeout");
    assert!(v["message"].as_str().unwrap().contains("中断"));

    // 逆証明: 締め切りを長くすれば同じ遅さでも 200
    let resp = read_response(
        &client,
        RecordType::Deal,
        "900",
        "1",
        MAX_HUBSPOT_CALLS_PER_REQUEST,
        Duration::from_secs(5),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
}

/// notes の batch read だけ 403 でも全体は 200。calls は表示され、partial に notes / hubspot_auth
#[tokio::test(flavor = "multi_thread")]
async fn engagement_の_batch_read_失敗は_partial_で_200() {
    let (audit, _) = start_fake_audit().await;
    let mut f = deal_fixture();
    f.fail_batch_read.insert("notes".into(), 403);
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/900", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(
        v["meta"]["partial"],
        json!([{"part": "notes", "error_kind": "hubspot_auth"}])
    );
    let ids: Vec<&str> = v["recent_activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["1002", "1001", "1003"]);
    assert_eq!(v["properties"]["dealname"], "介護スタッフ採用支援");
}

/// contact 経由の calls (v4 batch associations) が 404 でも全体は 200。直付きは表示される
#[tokio::test(flavor = "multi_thread")]
async fn contact_経由の_calls_の失敗は_partial_で_200() {
    let (audit, _) = start_fake_audit().await;
    let mut f = deal_fixture();
    f.fail_assoc_batch = Some(404);
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/900", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(
        v["meta"]["partial"],
        json!([{"part": "calls_via_contacts", "error_kind": "not_found"}])
    );
    let ids: Vec<&str> = v["recent_activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    // 直付きの call 1001 と note 2001 だけ (contact 経由の 1002 / 1003 は取れていない)
    assert_eq!(ids, vec!["1001", "2001"]);
}

/// Task は期日 (hs_timestamp) ではなく作成日時 (hs_createdate) で並ぶ。
/// 期日が未来の Task が先頭に来ない (逆証明: 期日で並べると 4001 が先頭になる)
#[tokio::test(flavor = "multi_thread")]
async fn task_は作成日時で並び期日が未来でも先頭に来ない() {
    let (audit, _) = start_fake_audit().await;
    let mut f = FakeHubSpot::default();
    f.obj("contacts", "55", &[("firstname", Some("太郎"))]);
    f.assoc("contacts", "55", "calls", &[(1001, None)]);
    f.obj(
        "calls",
        "1001",
        &[("hs_timestamp", Some("2026-09-10T00:00:00Z"))],
    );
    f.assoc("contacts", "55", "tasks", &[(4001, None), (4002, None)]);
    // 4001: 期日が 2030 年 (未来)、作成は 09-01
    f.obj(
        "tasks",
        "4001",
        &[
            ("hs_timestamp", Some("2030-01-01T00:00:00Z")),
            ("hs_createdate", Some("2026-09-01T00:00:00Z")),
            ("hs_task_subject", Some("期日が未来のタスク")),
        ],
    );
    // 4002: 期日は 08-01 (過去)、作成は 09-15
    f.obj(
        "tasks",
        "4002",
        &[
            ("hs_timestamp", Some("2026-08-01T00:00:00Z")),
            ("hs_createdate", Some("2026-09-15T00:00:00Z")),
        ],
    );
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(Some(audit), Some(client)));
    let cookie = login_as(&app, "google_oidc", Some("acc-admin")).await;
    let (status, _, v) = get_json(&app, "/api/crm/contacts/55", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let acts = v["recent_activities"].as_array().unwrap();
    let ids: Vec<&str> = acts.iter().map(|a| a["id"].as_str().unwrap()).collect();
    assert_eq!(ids, vec!["4002", "1001", "4001"]);
    // timestamp は並べ替えに使った作成日時、期日は properties に残る
    assert_eq!(acts[0]["timestamp"], "2026-09-15T00:00:00Z");
    assert_eq!(
        acts[0]["properties"]["hs_timestamp"],
        "2026-08-01T00:00:00Z"
    );
    assert_eq!(acts[2]["timestamp"], "2026-09-01T00:00:00Z");
    assert_eq!(
        acts[2]["properties"]["hs_timestamp"],
        "2030-01-01T00:00:00Z"
    );
    assert_eq!(
        acts[2]["properties"]["hs_task_subject"],
        "期日が未来のタスク"
    );
    // call の timestamp は従来どおり hs_timestamp
    assert_eq!(acts[1]["timestamp"], "2026-09-10T00:00:00Z");
}

/// 関連付きの本体 GET が 403 (関連型のスコープ不足の想定) でも、本体だけ取り直して 200。
/// 関連とアクティビティは partial に `associations` / `hubspot_auth` で出る。呼び出しは 2 回。
#[tokio::test(flavor = "multi_thread")]
async fn 関連付き本体_get_が_403_なら本体だけ取り直して_partial() {
    let mut f = deal_fixture();
    f.forbid_get_with_associations = true;
    let (client, hs) = start_fake_hubspot(f).await;
    let resp = read_response(
        &client,
        RecordType::Deal,
        "900",
        "1",
        MAX_HUBSPOT_CALLS_PER_REQUEST,
        Duration::from_secs(10),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let v: Value = serde_json::from_str(&body_string(resp).await).unwrap();
    assert_eq!(v["properties"]["dealname"], "介護スタッフ採用支援");
    assert_eq!(
        v["meta"]["partial"],
        json!([{"part": "associations", "error_kind": "hubspot_auth"}])
    );
    assert_eq!(v["recent_activities"], json!([]));
    let paths: Vec<String> = hs
        .lock()
        .unwrap()
        .requests
        .iter()
        .map(|r| r.1.clone())
        .collect();
    assert_eq!(paths.len(), 2, "{paths:?}");
    assert!(paths[0].contains("associations="));
    assert!(!paths[1].contains("associations="));
}

// ---------------------------------------------------------------------------
// 段階 A (逆証明): HubSpot の応答の端
// ---------------------------------------------------------------------------

/// アーカイブ済み (archived: true) のレコードは、生きているものとして見せず 404
#[tokio::test(flavor = "multi_thread")]
async fn アーカイブ済みのレコードは_404() {
    let mut f = FakeHubSpot::default();
    f.obj("deals", "900", &[("dealname", Some("消えた Deal"))]);
    f.archived.insert(("deals".to_string(), "900".to_string()));
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/deals/900", &cookie).await;
    assert_eq!(status, StatusCode::NOT_FOUND, "{v}");
    assert_eq!(v["error_kind"], "not_found");
    assert!(!v.to_string().contains("消えた Deal"));
}

/// 直近アクティビティのうちアーカイブ済みのものは載せない
#[tokio::test(flavor = "multi_thread")]
async fn アーカイブ済みのアクティビティは載せない() {
    let mut f = FakeHubSpot::default();
    f.obj("contacts", "55", &[("firstname", Some("太郎"))]);
    f.assoc("contacts", "55", "notes", &[(3001, None), (3002, None)]);
    f.obj(
        "notes",
        "3001",
        &[("hs_timestamp", Some("2026-09-01T00:00:00Z"))],
    );
    f.obj(
        "notes",
        "3002",
        &[("hs_timestamp", Some("2026-09-02T00:00:00Z"))],
    );
    f.archived.insert(("notes".to_string(), "3002".to_string()));
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/contacts/55", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    let ids: Vec<&str> = v["recent_activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| a["id"].as_str().unwrap())
        .collect();
    assert_eq!(ids, vec!["3001"]);
}

/// プロパティが 1 つも無いレコードでも 200 (properties は空 / 要求したキーが欠けても壊れない)
#[tokio::test(flavor = "multi_thread")]
async fn プロパティが空のレコードでも_200() {
    let mut f = FakeHubSpot::default();
    f.obj("companies", "300", &[]);
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/companies/300", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["properties"], json!({}));
    assert_eq!(v["recent_activities"], json!([]));
    assert_eq!(v["meta"]["partial"], json!([]));
}

/// 上流の 429 / 500 / 403 はブラウザに「上流の本文」を出さず、error_kind と固定文言だけ返す
#[tokio::test(flavor = "multi_thread")]
async fn 上流のエラー本文はブラウザに返さない() {
    for (upstream, status, kind) in [
        (429, StatusCode::SERVICE_UNAVAILABLE, "hubspot_rate_limited"),
        (500, StatusCode::BAD_GATEWAY, "hubspot_upstream"),
        (400, StatusCode::BAD_GATEWAY, "hubspot_upstream"),
    ] {
        let mut f = FakeHubSpot::default();
        f.obj("deals", "900", &[("dealname", Some("x"))]);
        f.get_status = Some(upstream);
        let (client, _) = start_fake_hubspot(f).await;
        let app = crm_app(test_state(None, Some(client)));
        let cookie = login_as(&app, "google_oidc", None).await;
        let resp = get_req(&app, "/api/crm/deals/900", Some(&cookie)).await;
        assert_eq!(resp.status(), status, "upstream {upstream}");
        let body = body_string(resp).await;
        assert!(!body.contains(UPSTREAM_SECRET), "{body}");
        assert!(!body.contains(HUBSPOT_TOKEN), "{body}");
        let v: Value = serde_json::from_str(&body).unwrap();
        assert_eq!(v["error_kind"], kind, "upstream {upstream}");
    }
}

/// 同時に何本来ても、HubSpot への本体 GET の同時実行は枠 (4) を超えない。
/// 鍵は既存の営業自動化バッチと共有なので、連打・多タブで枠を食い尽くさないため
#[tokio::test(flavor = "multi_thread")]
async fn レコード読み取りの同時実行は枠を超えない() {
    let mut f = FakeHubSpot::default();
    f.obj("companies", "300", &[("name", Some("社"))]);
    f.get_delay = Duration::from_millis(150);
    let (client, hs) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let mut tasks = Vec::new();
    for _ in 0..10 {
        let (app, cookie) = (app.clone(), cookie.clone());
        tasks.push(tokio::spawn(async move {
            get_req(&app, "/api/crm/companies/300", Some(&cookie))
                .await
                .status()
        }));
    }
    for t in tasks {
        assert_eq!(t.await.unwrap(), StatusCode::OK);
    }
    let max = hs.lock().unwrap().max_inflight;
    assert_eq!(
        max,
        super::routes::MAX_CONCURRENT_RECORD_READS,
        "同時実行の最大値 (枠いっぱいまでは使う)"
    );
}

// --- /api/crm/metadata (HubSpot の定義) ---

fn meta_requests(hs: &Shared<FakeHubSpot>) -> usize {
    hs.lock()
        .unwrap()
        .requests
        .iter()
        .filter(|r| r.1.starts_with("/crm/v3/properties/") || r.1.starts_with("/crm/v3/pipelines/"))
        .count()
}

/// 許可された人は 200。キャッシュ → 連続 refresh は枠内ならキャッシュを返す。
/// 顧客レコードの GET は 1 回も飛ばない
#[tokio::test(flavor = "multi_thread")]
async fn metadata_は定義だけ返し_キャッシュと_refresh_の下限が効く() {
    let (client, hs) = start_fake_hubspot(FakeHubSpot::default()).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;

    let (status, cc, v) = get_json(&app, "/api/crm/metadata", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(cc, "no-store");
    assert_eq!(v["cache_hit"], false);
    assert_eq!(meta_requests(&hs), 4);
    // 確認済みの項目だけ (not_reviewed は出ない)
    let names: Vec<String> = v["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            format!(
                "{}:{}",
                p["object_type"].as_str().unwrap(),
                p["name"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        names,
        vec!["contacts:firstname", "companies:industry", "deals:bpo_42"]
    );
    // ステージは displayOrder 順
    let stages: Vec<&str> = v["pipelines"][0]["stages"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["id"].as_str().unwrap())
        .collect();
    assert_eq!(stages, vec!["s1", "s2"]);

    let (_, _, v2) = get_json(&app, "/api/crm/metadata", &cookie).await;
    assert_eq!(v2["cache_hit"], true);
    assert_eq!(meta_requests(&hs), 4, "キャッシュ中は上流を呼ばない");
    // 直後の refresh は下限 (5 秒) 内なのでキャッシュのまま
    let (_, _, v3) = get_json(&app, "/api/crm/metadata?refresh=true", &cookie).await;
    assert_eq!(v3["cache_hit"], true);
    assert_eq!(meta_requests(&hs), 4, "refresh の連打で上流を叩かない");
    // 顧客レコードは 1 回も読んでいない
    assert!(hs
        .lock()
        .unwrap()
        .requests
        .iter()
        .all(|r| !r.1.starts_with("/crm/v3/objects/")));
}

/// 想定外の形 (results 欠落) は 502 hubspot_decode。panic せず、キャッシュにも残さない
#[tokio::test(flavor = "multi_thread")]
async fn metadata_の応答の形が崩れたら_502_でキャッシュしない() {
    let f = FakeHubSpot {
        meta_mode: 1,
        ..Default::default()
    };
    let (client, hs) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/metadata", &cookie).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{v}");
    assert_eq!(v["error_kind"], "hubspot_decode");
    // 直したら次の呼び出しで取れる (失敗がキャッシュされていない)
    hs.lock().unwrap().meta_mode = 0;
    let (status, _, v) = get_json(&app, "/api/crm/metadata", &cookie).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["cache_hit"], false);
}

/// 上流の 429 (本文に秘密の文字列) は 503 hubspot_rate_limited。本文は出ない
#[tokio::test(flavor = "multi_thread")]
async fn metadata_の上流_429_は本文を出さず_503() {
    let f = FakeHubSpot {
        meta_mode: 2,
        ..Default::default()
    };
    let (client, _) = start_fake_hubspot(f).await;
    let app = crm_app(test_state(None, Some(client)));
    let cookie = login_as(&app, "google_oidc", None).await;
    let resp = get_req(&app, "/api/crm/metadata", Some(&cookie)).await;
    assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = body_string(resp).await;
    assert!(!body.contains(UPSTREAM_SECRET), "{body}");
    assert!(!body.contains(HUBSPOT_TOKEN), "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["error_kind"], "hubspot_rate_limited");
}

/// HubSpot 未設定は 503 not_configured (認可を通った人にだけ見せる)
#[tokio::test(flavor = "multi_thread")]
async fn metadata_は_hubspot_未設定なら_503() {
    let app = crm_app(test_state(None, None));
    let cookie = login_as(&app, "google_oidc", None).await;
    let (status, _, v) = get_json(&app, "/api/crm/metadata", &cookie).await;
    assert_eq!(
        (status, v),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            json!({"error_kind": "not_configured"})
        )
    );
}
