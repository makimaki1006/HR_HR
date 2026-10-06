//! `GET /api/crm/workspace/deals/{id}` の結合テスト (偽 HubSpot。本物の HubSpot は呼ばない)。
//!
//! 偽 HubSpot は受け取った呼び出しを順に記録する。呼び出し回数・順序・「呼ばないこと」は記録で検証する。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::call_queue::CallQueueState;
use super::rbac::{CrmAccess, CrmRole};
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

type Shared<T> = Arc<Mutex<T>>;
type Props = Vec<(String, String)>;
/// (from, to, from id) → [(to id, ラベル)]
type AssocKey = (String, String, String);
type AssocTargets = Vec<(String, Option<String>)>;

const HUBSPOT_TOKEN: &str = "test-token-XYZ";
const UPSTREAM_SECRET: &str = "UPSTREAM-SECRET-BODY";
const ADMIN: &str = "admin@f-a-c.co.jp";
const BPO: &str = "bpo@f-a-c.co.jp";
const OUTSIDER: &str = "outsider@f-a-c.co.jp";
const BPO_OWNER: &str = "111";
const OTHER_OWNER: &str = "222";
const PIPELINE: &str = "753186575";
const UNPROCESSED: &str = "1095387442";
const FUZAI: &str = "1095387445";
const KEY: [u8; 32] = [7u8; 32];
const DEAL: &str = "5001";

fn now_default() -> DateTime<Utc> {
    // JST 2026-10-05 12:00
    Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap()
}

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeHs {
    /// (object, id) → properties
    objects: HashMap<(String, String), Props>,
    archived: HashSet<(String, String)>,
    /// 案件本体 GET の `associations=` で返す関連 (deal id, 型) → ids
    v3_assoc: HashMap<(String, String), Vec<String>>,
    /// v4 batch の関連 (from, to, from id) → [(to id, ラベル)]
    v4_assoc: HashMap<AssocKey, AssocTargets>,
    /// このパスを含む呼び出しを失敗させる (パス断片 → status)
    fail: HashMap<String, u16>,
    /// 本体 GET (`associations=` に emails を含む) だけ 403 (読み取りスコープ不足)
    forbid_emails_get: bool,
    deal_delay: Duration,
    owners: HashMap<String, String>,
    /// (method + path, body)
    log: Vec<(String, String)>,
}

impl FakeHs {
    fn new() -> Self {
        Self {
            owners: HashMap::from([(BPO.to_string(), BPO_OWNER.to_string())]),
            ..Default::default()
        }
    }
    fn put(&mut self, o: &str, id: &str, props: &[(&str, &str)]) {
        self.objects.insert(
            (o.to_string(), id.to_string()),
            props
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
    }
    /// キューに出る (自分の担当・未済) 案件
    fn deal(&mut self, id: &str, owner: &str, stage: &str, extra: &[(&str, &str)]) {
        let mut p: Vec<(&str, &str)> = vec![
            ("dealname", "架空案件"),
            ("dealstage", stage),
            ("pipeline", PIPELINE),
            ("hubspot_owner_id", owner),
            ("amount", "120000"),
        ];
        p.extend_from_slice(extra);
        self.put("deals", id, &p);
    }
    fn v3(&mut self, deal: &str, ty: &str, ids: &[&str]) {
        self.v3_assoc.insert(
            (deal.to_string(), ty.to_string()),
            ids.iter().map(|s| s.to_string()).collect(),
        );
    }
    fn v4(&mut self, from: &str, to: &str, id: &str, targets: &[(&str, Option<&str>)]) {
        self.v4_assoc.insert(
            (from.to_string(), to.to_string(), id.to_string()),
            targets
                .iter()
                .map(|(i, l)| (i.to_string(), l.map(str::to_string)))
                .collect(),
        );
    }
    fn count(&self, needle: &str) -> usize {
        self.log.iter().filter(|(c, _)| c.contains(needle)).count()
    }
    fn failing(&self, path: &str) -> Option<u16> {
        self.fail
            .iter()
            .find(|(k, _)| path.contains(k.as_str()))
            .map(|(_, v)| *v)
    }
    fn record_json(&self, o: &str, id: &str, wanted: &[String]) -> Option<Value> {
        let props = self.objects.get(&(o.to_string(), id.to_string()))?;
        let mut m = serde_json::Map::new();
        for (k, v) in props {
            if wanted.is_empty() || wanted.contains(k) {
                m.insert(k.clone(), Value::String(v.clone()));
            }
        }
        Some(json!({"id": id, "properties": m,
            "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z",
            "archived": self.archived.contains(&(o.to_string(), id.to_string()))}))
    }
}

fn err_resp(code: u16) -> Response {
    if code == 429 {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "0")],
            UPSTREAM_SECRET,
        )
            .into_response();
    }
    (StatusCode::from_u16(code).unwrap(), UPSTREAM_SECRET).into_response()
}

async fn hs_get_object(
    State(st): State<Shared<FakeHs>>,
    Path((o, id)): Path<(String, String)>,
    RawQuery(q): RawQuery,
) -> Response {
    let q = q.unwrap_or_default();
    let (resp, delay) = {
        let mut s = st.lock().unwrap();
        let path = format!("GET /crm/v3/objects/{o}/{id}");
        s.log.push((path.clone(), q.clone()));
        let delay = s.deal_delay;
        let resp = if let Some(c) = s.failing(&path) {
            err_resp(c)
        } else if s.forbid_emails_get && q.contains("emails") {
            (
                StatusCode::FORBIDDEN,
                Json(json!({"category": "MISSING_SCOPES"})),
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
            match s.record_json(&o, &id, &split("properties")) {
                Some(mut v) => {
                    let mut assoc = serde_json::Map::new();
                    for t in split("associations") {
                        if let Some(ids) = s.v3_assoc.get(&(id.clone(), t.clone())) {
                            if !ids.is_empty() {
                                let results: Vec<Value> =
                                    ids.iter().map(|i| json!({"id": i, "type": "x"})).collect();
                                assoc.insert(t, json!({"results": results}));
                            }
                        }
                    }
                    if !assoc.is_empty() {
                        v["associations"] = Value::Object(assoc);
                    }
                    Json(v).into_response()
                }
                None => (
                    StatusCode::NOT_FOUND,
                    Json(json!({"category": "OBJECT_NOT_FOUND"})),
                )
                    .into_response(),
            }
        };
        (resp, delay)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    resp
}

async fn hs_batch_read(
    State(st): State<Shared<FakeHs>>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    let path = format!("POST /crm/v3/objects/{o}/batch/read");
    s.log.push((path.clone(), body.to_string()));
    if let Some(c) = s.failing(&path) {
        return err_resp(c);
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
    State(st): State<Shared<FakeHs>>,
    Path((from, to)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    let path = format!("POST /crm/v4/associations/{from}/{to}/batch/read");
    s.log.push((path.clone(), body.to_string()));
    if let Some(c) = s.failing(&path) {
        return err_resp(c);
    }
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| {
            let fid = i["id"].as_str().unwrap_or("").to_string();
            let targets = s.v4_assoc.get(&(from.clone(), to.clone(), fid.clone()))?;
            if targets.is_empty() {
                return None;
            }
            let to_list: Vec<Value> = targets
                .iter()
                .map(|(t, label)| {
                    json!({"toObjectId": t.parse::<u64>().unwrap(), "associationTypes": [
                        {"category": "HUBSPOT_DEFINED", "typeId": 3, "label": label}]})
                })
                .collect();
            Some(json!({"from": {"id": fid}, "to": to_list}))
        })
        .collect();
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn hs_owners(State(st): State<Shared<FakeHs>>, RawQuery(q): RawQuery) -> Response {
    let q = q.unwrap_or_default();
    let mut s = st.lock().unwrap();
    s.log.push(("GET /crm/v3/owners".to_string(), q.clone()));
    if let Some(c) = s.failing("/crm/v3/owners") {
        return err_resp(c);
    }
    let url = reqwest::Url::parse(&format!("http://x/?{q}")).unwrap();
    let email = url
        .query_pairs()
        .find(|(k, _)| k == "email")
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default();
    let results: Vec<Value> = s
        .owners
        .get(&email)
        .map(|id| vec![json!({"id": id, "email": email, "archived": false})])
        .unwrap_or_default();
    Json(json!({"results": results})).into_response()
}

async fn hs_pipelines(State(st): State<Shared<FakeHs>>) -> Response {
    let mut s = st.lock().unwrap();
    s.log
        .push(("GET /crm/v3/pipelines/deals".to_string(), String::new()));
    if let Some(c) = s.failing("/crm/v3/pipelines/deals") {
        return err_resp(c);
    }
    Json(json!({"results": [{
        "id": PIPELINE, "label": "bpo_リクロジ", "displayOrder": 0,
        "stages": [
            {"id": UNPROCESSED, "label": "未済", "displayOrder": 0},
            {"id": FUZAI, "label": "不在", "displayOrder": 1}
        ]
    }]}))
    .into_response()
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

async fn start_hs(fake: FakeHs) -> (Arc<HubSpotClient>, Shared<FakeHs>) {
    let st = Arc::new(Mutex::new(fake));
    let base = spawn(
        Router::new()
            .route("/crm/v3/objects/{o}/batch/read", post(hs_batch_read))
            .route("/crm/v3/objects/{o}/{id}", get(hs_get_object))
            .route(
                "/crm/v4/associations/{from}/{to}/batch/read",
                post(hs_assoc),
            )
            .route("/crm/v3/owners", get(hs_owners))
            .route("/crm/v3/pipelines/deals", get(hs_pipelines))
            .with_state(st.clone()),
    )
    .await;
    let client = HubSpotClient::new(
        HUBSPOT_TOKEN.into(),
        &base,
        ClientOptions {
            timeout: Duration::from_millis(600),
            max_retries: 0,
            retry_base_delay: Duration::from_millis(1),
            search_min_interval: Duration::from_millis(1),
            rate_limited_min_wait: Duration::from_millis(1),
        },
    )
    .unwrap();
    (Arc::new(client), st)
}

// ---------------------------------------------------------------------------
// アプリ
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
        admin_emails: vec![ADMIN.to_string()],
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    }
}

fn test_state(hubspot: Option<Arc<HubSpotClient>>) -> Arc<AppState> {
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
        audit: None,
        google_oidc: None,
        hubspot,
    })
}

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
    StatusCode::NO_CONTENT
}

fn make_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(super::routes::router_with_queue(
            CrmAccess::from_list(&format!("{ADMIN},{BPO}"))
                .with_test_role(ADMIN, CrmRole::Admin)
                .with_test_role(BPO, CrmRole::Bpo),
            CallQueueState::for_test(KEY, now_default()),
        ))
        .route("/__test/session", post(inject_session))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

async fn login(app: &Router, email: &str, method: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/__test/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email": email, "login_method": method}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("id="))
        .map(|c| c.split(';').next().unwrap().to_string())
        .expect("cookie")
}

struct Env {
    app: Router,
    hs: Shared<FakeHs>,
    admin: String,
    bpo: String,
}

async fn env(fake: FakeHs) -> Env {
    let (client, hs) = start_hs(fake).await;
    let app = make_app(test_state(Some(client)));
    let admin = login(&app, ADMIN, "google_oidc").await;
    let bpo = login(&app, BPO, "google_oidc").await;
    Env {
        app,
        hs,
        admin,
        bpo,
    }
}

async fn get_raw(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, String, Value) {
    let mut b = Request::builder().uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let resp = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = resp.status();
    let cc = resp
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_string())
        .unwrap_or_default();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| panic!("not json: {text}"));
    assert!(!text.contains(HUBSPOT_TOKEN), "token leaked: {text}");
    assert!(
        !text.contains(UPSTREAM_SECRET),
        "upstream body leaked: {text}"
    );
    (status, cc, v)
}

fn url(id: &str) -> String {
    format!("/api/crm/workspace/deals/{id}")
}

impl Env {
    async fn admin_get(&self, id: &str) -> (StatusCode, Value) {
        let (s, cc, v) = get_raw(&self.app, &url(id), Some(&self.admin)).await;
        assert_eq!(cc, "no-store");
        (s, v)
    }
    async fn bpo_get(&self, id: &str) -> (StatusCode, Value) {
        let (s, _, v) = get_raw(&self.app, &url(id), Some(&self.bpo)).await;
        (s, v)
    }
    fn count(&self, n: &str) -> usize {
        self.hs.lock().unwrap().count(n)
    }
    fn total(&self) -> usize {
        self.hs.lock().unwrap().log.len()
    }
    fn calls(&self) -> Vec<String> {
        self.hs
            .lock()
            .unwrap()
            .log
            .iter()
            .map(|(c, _)| c.clone())
            .collect()
    }
}

fn kind(v: &Value) -> Option<&str> {
    v["error_kind"].as_str()
}

fn act_ids(v: &Value) -> Vec<String> {
    v["activities"]
        .as_array()
        .unwrap()
        .iter()
        .map(|a| {
            format!(
                "{}:{}",
                a["kind"].as_str().unwrap(),
                a["id"].as_str().unwrap()
            )
        })
        .collect()
}

/// 標準の関連つき案件: 担当者 2 人 (7002 が主)、会社 1 社、通話・メモ・メール・ミーティング
fn full(f: &mut FakeHs, owner: &str) {
    f.deal(DEAL, owner, UNPROCESSED, &[("bpo_13", "2026-10-01")]);
    f.put(
        "contacts",
        "7001",
        &[
            ("firstname", "花子"),
            ("lastname", "架空"),
            ("phone", "03-1111-0001"),
            ("jobtitle", "経理"),
        ],
    );
    f.put(
        "contacts",
        "7002",
        &[
            ("firstname", "太郎"),
            ("lastname", "架空"),
            ("phone", "03-1111-0002"),
            ("mobilephone", "090-1111-0002"),
            ("jobtitle", "採用担当"),
            ("email", "taro@example.invalid"),
        ],
    );
    f.put(
        "companies",
        "8001",
        &[
            ("name", "架空商事"),
            ("phone", "03-9999-0000"),
            ("zip", "100-0001"),
            ("state", "東京都"),
            ("city", "千代田区"),
            ("address", "架空1-1"),
            ("industry", "介護"),
            ("domain", "example.invalid"),
        ],
    );
    f.v4(
        "deals",
        "contacts",
        DEAL,
        &[("7001", None), ("7002", Some("主"))],
    );
    f.v4("deals", "companies", DEAL, &[("8001", Some("主"))]);
    f.v3(DEAL, "calls", &["9001"]);
    f.v3(DEAL, "notes", &["9101"]);
    f.v3(DEAL, "emails", &["9201"]);
    f.v3(DEAL, "meetings", &["9301"]);
    f.v4("contacts", "calls", "7002", &[("9002", None)]);
    f.put(
        "calls",
        "9001",
        &[
            ("hs_timestamp", "2026-10-03T01:00:00Z"),
            ("hs_call_title", "架電1"),
            ("hs_call_direction", "OUTBOUND"),
            ("hs_call_status", "COMPLETED"),
            ("hs_call_duration", "65000"),
            ("hs_call_source", "INTEGRATIONS_PLATFORM"),
            ("hubspot_owner_id", BPO_OWNER),
        ],
    );
    f.put(
        "calls",
        "9002",
        &[
            ("hs_timestamp", "2026-10-04T01:00:00Z"),
            ("hs_call_title", "別案件の通話"),
            ("hs_call_direction", "OUTBOUND"),
            ("hs_call_status", "NO_ANSWER"),
        ],
    );
    f.put(
        "notes",
        "9101",
        &[
            ("hs_timestamp", "2026-10-02T01:00:00Z"),
            (
                "hs_note_body",
                "<p>受付で<b>不在</b></p><p>来週再架電 &amp; 確認</p>",
            ),
        ],
    );
    f.put(
        "emails",
        "9201",
        &[
            ("hs_timestamp", "2026-09-30T01:00:00Z"),
            ("hs_email_subject", "ご挨拶"),
            ("hs_email_text", "資料を送付します"),
            ("hs_email_direction", "EMAIL"),
        ],
    );
    f.put(
        "meetings",
        "9301",
        &[
            ("hs_timestamp", "2026-09-29T01:00:00Z"),
            ("hs_meeting_title", "初回商談"),
            ("hs_meeting_outcome", "COMPLETED"),
        ],
    );
}

// ---------------------------------------------------------------------------
// 認可: HubSpot を 1 回も呼ばない
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 権限なしは_hubspot_を呼ばない() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    let e = env(f).await;
    // 未ログイン
    let (s, _, v) = get_raw(&e.app, &url(DEAL), None).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::UNAUTHORIZED, Some("login_required"))
    );
    // パスワードログイン
    let pw = login(&e.app, ADMIN, "password_internal").await;
    let (s, _, v) = get_raw(&e.app, &url(DEAL), Some(&pw)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::FORBIDDEN, Some("google_login_required"))
    );
    // 許可リストに無い Google アカウント
    let out = login(&e.app, OUTSIDER, "google_oidc").await;
    let (s, _, v) = get_raw(&e.app, &url(DEAL), Some(&out)).await;
    assert_eq!((s, kind(&v)), (StatusCode::FORBIDDEN, Some("forbidden")));
    assert_eq!(e.total(), 0, "{:?}", e.calls());
}

#[tokio::test(flavor = "multi_thread")]
async fn 不正な_id_は_400_で_hubspot_を呼ばない_未設定は認可の後で_503() {
    let e = env(FakeHs::new()).await;
    for bad in ["abc", "12a", "1%2F2", &"9".repeat(21), "-1"] {
        let (s, v) = e.admin_get(bad).await;
        assert_eq!(
            (s, kind(&v)),
            (StatusCode::BAD_REQUEST, Some("invalid_id")),
            "{bad}"
        );
    }
    assert_eq!(e.total(), 0);
    // HubSpot 未設定: 認可された人は 503、未認可の人には設定状況を見せない
    let app = make_app(test_state(None));
    let admin = login(&app, ADMIN, "google_oidc").await;
    let (s, _, v) = get_raw(&app, &url(DEAL), Some(&admin)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::SERVICE_UNAVAILABLE, Some("not_configured"))
    );
    let out = login(&app, OUTSIDER, "google_oidc").await;
    let (s, _, v) = get_raw(&app, &url(DEAL), Some(&out)).await;
    assert_eq!((s, kind(&v)), (StatusCode::FORBIDDEN, Some("forbidden")));
}

// ---------------------------------------------------------------------------
// 正常系: 項目・呼び出し回数
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 管理者は案件_担当者_会社_活動を読み_呼び出しは固定の回数() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK, "{v}");

    // 案件
    let d = &v["deal"];
    assert_eq!(d["id"], DEAL);
    assert_eq!(d["name"], "架空案件");
    assert_eq!(d["stage_id"], UNPROCESSED);
    assert_eq!(
        d["stage_label"], "未済",
        "ID を表示名にせず、パイプライン定義から引く"
    );
    assert_eq!(d["owner_id"], BPO_OWNER);
    assert_eq!(d["amount"], "120000");
    assert_eq!(d["next_call_date"], "2026-10-01");
    assert_eq!(d["last_call_date"], Value::Null);
    assert_eq!(
        d["deep_link"],
        "https://app.hubspot.com/contacts/23708633/record/0-3/5001/"
    );
    assert_eq!(v["hubspot_portal_id"], "23708633");

    // 担当者: 主 (7002) が先頭
    let cs = v["contacts"].as_array().unwrap();
    assert_eq!(cs.len(), 2);
    assert_eq!(v["contacts_total"], 2);
    assert_eq!(cs[0]["id"], "7002");
    assert_eq!(cs[0]["is_primary"], true);
    assert_eq!(cs[0]["name"], "架空 太郎");
    assert_eq!(cs[0]["job_title"], "採用担当");
    assert_eq!(cs[0]["phone"], "03-1111-0002");
    assert_eq!(cs[0]["mobile"], "090-1111-0002");
    assert_eq!(cs[0]["email"], "taro@example.invalid");
    assert_eq!(cs[0]["labels"], json!(["主"]));
    assert_eq!(
        cs[0]["deep_link"],
        "https://app.hubspot.com/contacts/23708633/record/0-1/7002/"
    );
    assert_eq!(cs[1]["id"], "7001");
    assert_eq!(cs[1]["is_primary"], false);

    // 会社
    let co = &v["companies"][0];
    assert_eq!(co["name"], "架空商事");
    assert_eq!(co["address"], "100-0001 東京都 千代田区 架空1-1");
    assert_eq!(co["industry"], "介護");
    assert_eq!(co["domain"], "example.invalid");
    assert_eq!(
        co["deep_link"],
        "https://app.hubspot.com/contacts/23708633/record/0-2/8001/"
    );

    // 架ける番号: bpo_29 が無いので主担当者の phone
    assert_eq!(
        v["dial"],
        json!({"number": "03-1111-0002", "source": "contact"})
    );

    // 活動: 新しい順、本文は平文、担当者経由の通話は via=contact
    assert_eq!(
        act_ids(&v),
        [
            "call:9002",
            "call:9001",
            "note:9101",
            "email:9201",
            "meeting:9301"
        ]
    );
    let acts = v["activities"].as_array().unwrap();
    assert_eq!(acts[0]["via"], "contact");
    assert_eq!(acts[0]["via_id"], "7002");
    assert_eq!(acts[1]["via"], "deal");
    assert_eq!(acts[1]["duration_ms"], 65000);
    assert_eq!(acts[1]["direction"], "OUTBOUND");
    assert_eq!(acts[1]["source"], "INTEGRATIONS_PLATFORM");
    assert_eq!(acts[2]["body"], "受付で不在\n来週再架電 & 確認");
    assert_eq!(acts[3]["title"], "ご挨拶");
    assert_eq!(acts[4]["title"], "初回商談");
    assert_eq!(v["partial"], json!([]));
    assert_eq!(v["activities_truncated"], false);
    assert!(v["activity_scope"].as_str().unwrap().contains("通話"));

    // 呼び出し回数 (1 画面 = 11 回。キャッシュが冷えている最初の 1 回)
    assert_eq!(e.count("GET /crm/v3/objects/deals/5001"), 1);
    assert_eq!(e.count("/associations/deals/contacts/"), 1);
    assert_eq!(e.count("/associations/deals/companies/"), 1);
    assert_eq!(e.count("/associations/contacts/calls/"), 1);
    assert_eq!(e.count("POST /crm/v3/objects/contacts/batch/read"), 1);
    assert_eq!(e.count("POST /crm/v3/objects/companies/batch/read"), 1);
    assert_eq!(e.count("GET /crm/v3/pipelines/deals"), 1);
    for t in ["calls", "notes", "emails", "meetings"] {
        assert_eq!(
            e.count(&format!("POST /crm/v3/objects/{t}/batch/read")),
            1,
            "{t}"
        );
    }
    assert_eq!(e.total(), 11, "{:?}", e.calls());
    assert_eq!(e.count("/owners"), 0, "管理者は owner を引かない");

    // 2 回目: ステージ名はキャッシュされる (10 回)
    let before = e.total();
    let (s, _) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.total() - before, 10);
}

#[tokio::test(flavor = "multi_thread")]
async fn 電話番号の優先順位は案件_主担当者_携帯_会社() {
    for (deal_phone, contact_phone, mobile, company_phone, want) in [
        (
            Some("03-0000-0000"),
            Some("03-1"),
            Some("090-1"),
            Some("03-9"),
            Some(("deal", "03-0000-0000")),
        ),
        (
            None,
            Some("03-1"),
            Some("090-1"),
            Some("03-9"),
            Some(("contact", "03-1")),
        ),
        (
            None,
            None,
            Some("090-1"),
            Some("03-9"),
            Some(("mobile", "090-1")),
        ),
        (None, None, None, Some("03-9"), Some(("company", "03-9"))),
        (None, None, None, None, None),
    ] {
        let mut f = FakeHs::new();
        let extra: Vec<(&str, &str)> = deal_phone.map(|p| vec![("bpo_29", p)]).unwrap_or_default();
        f.deal(DEAL, BPO_OWNER, UNPROCESSED, &extra);
        let mut c: Vec<(&str, &str)> = vec![("lastname", "架空")];
        if let Some(p) = contact_phone {
            c.push(("phone", p));
        }
        if let Some(p) = mobile {
            c.push(("mobilephone", p));
        }
        f.put("contacts", "7002", &c);
        let mut co: Vec<(&str, &str)> = vec![("name", "架空商事")];
        if let Some(p) = company_phone {
            co.push(("phone", p));
        }
        f.put("companies", "8001", &co);
        f.v4("deals", "contacts", DEAL, &[("7002", Some("主"))]);
        f.v4("deals", "companies", DEAL, &[("8001", None)]);
        let e = env(f).await;
        let (s, v) = e.admin_get(DEAL).await;
        assert_eq!(s, StatusCode::OK);
        match want {
            Some((src, num)) => assert_eq!(v["dial"], json!({"number": num, "source": src})),
            None => assert_eq!(v["dial"], Value::Null),
        }
    }
}

// ---------------------------------------------------------------------------
// 役割の関門
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn bpo_は自分の担当でキューに出る案件を読める_owner_は_1_回だけ引く() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    let e = env(f).await;
    for _ in 0..2 {
        let (s, v) = e.bpo_get(DEAL).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["deal"]["id"], DEAL);
    }
    assert_eq!(e.count("GET /crm/v3/owners"), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn bpo_は他人の案件_キュー外_存在しない_id_を読めず_本文も関連も返さず_以降の読み取りをしない(
) {
    let mut f = FakeHs::new();
    // 他人の担当
    full(&mut f, OTHER_OWNER);
    // 自分の担当だが次回日が未来の不在ステージ (キューに出ない)
    f.deal("5002", BPO_OWNER, FUZAI, &[("bpo_13", "2026-12-01")]);
    // 自分の担当だが架電禁止理由あり
    f.deal("5003", BPO_OWNER, UNPROCESSED, &[("bpo_3", "禁止")]);
    // 自分の担当だが別パイプライン
    f.put(
        "deals",
        "5004",
        &[
            ("dealname", "別"),
            ("dealstage", UNPROCESSED),
            ("pipeline", "999"),
            ("hubspot_owner_id", BPO_OWNER),
        ],
    );
    // アーカイブ済み
    f.deal("5005", BPO_OWNER, UNPROCESSED, &[]);
    f.archived.insert(("deals".into(), "5005".into()));
    let e = env(f).await;
    let mut bodies = Vec::new();
    for id in [DEAL, "5002", "5003", "5004", "5005", "6999"] {
        let before = e.total();
        let (s, v) = e.bpo_get(id).await;
        assert_eq!(
            (s, kind(&v)),
            (StatusCode::FORBIDDEN, Some("forbidden_record")),
            "{id}: {v}"
        );
        assert!(
            v.get("deal").is_none() && v.get("contacts").is_none() && v.get("activities").is_none(),
            "{id}"
        );
        assert!(!v.to_string().contains("架空"), "本文が漏れた: {v}");
        // この要求の呼び出しは案件の読み取り 1 回だけ (owner の対応は最初の 1 回)
        let calls = e.calls()[before..].to_vec();
        assert!(
            calls
                .iter()
                .all(|c| c.starts_with("GET /crm/v3/objects/deals/") || c == "GET /crm/v3/owners"),
            "{id}: {calls:?}"
        );
        bodies.push(v);
    }
    // 存在しない id も担当外も同じ応答 (id の存在を探れない)
    assert!(bodies.windows(2).all(|w| w[0] == w[1]), "{bodies:?}");
    for n in ["/associations/", "/batch/read", "/pipelines/"] {
        assert_eq!(e.count(n), 0, "関門で止まったので {n} は呼ばない");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn owner_を引けない_bpo_は_403_で案件を読まない_owners_の失敗は全員分に倒さない() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.owners.clear();
    let e = env(f).await;
    let (s, v) = e.bpo_get(DEAL).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::FORBIDDEN, Some("owner_not_found"))
    );
    assert_eq!(e.count("/objects/"), 0);
    // Owners API の失敗
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.fail.insert("/crm/v3/owners".into(), 500);
    let e = env(f).await;
    let (s, v) = e.bpo_get(DEAL).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_GATEWAY, Some("hubspot_upstream"))
    );
    assert_eq!(e.count("/objects/"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者は担当者に関係なく読める() {
    let mut f = FakeHs::new();
    full(&mut f, OTHER_OWNER);
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["deal"]["owner_id"], OTHER_OWNER);
}

// ---------------------------------------------------------------------------
// 欠落・重複・上限
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 関連も活動も無い案件は空で返り_余計な読み取りをしない() {
    let mut f = FakeHs::new();
    f.deal(DEAL, BPO_OWNER, UNPROCESSED, &[("bpo_29", "03-0000-0001")]);
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["contacts"], json!([]));
    assert_eq!(v["companies"], json!([]));
    assert_eq!(v["activities"], json!([]));
    assert_eq!(v["partial"], json!([]));
    assert_eq!(v["contacts_total"], 0);
    assert_eq!(
        v["dial"],
        json!({"number": "03-0000-0001", "source": "deal"})
    );
    // 案件 1 + 関連 2 + ステージ名 1 だけ。担当者・会社・活動の batch_read は ID が無いので呼ばない
    assert_eq!(e.count("/batch/read"), 2, "{:?}", e.calls());
    assert_eq!(e.count("/associations/contacts/calls/"), 0);
    assert_eq!(e.total(), 4, "{:?}", e.calls());
}

#[tokio::test(flavor = "multi_thread")]
async fn 同じ通話が案件と複数の担当者に付いていても_1_件で_案件直付きを優先() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    // 9001 は案件直付き。担当者 7001・7002 の両方にも付いている。9002 は 2 人の担当者の両方に付く
    f.v4(
        "contacts",
        "calls",
        "7001",
        &[("9001", None), ("9002", None)],
    );
    f.v4(
        "contacts",
        "calls",
        "7002",
        &[("9001", None), ("9002", None)],
    );
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK);
    let calls: Vec<String> = act_ids(&v)
        .into_iter()
        .filter(|a| a.starts_with("call:"))
        .collect();
    assert_eq!(calls, ["call:9002", "call:9001"], "重複なし");
    let acts = v["activities"].as_array().unwrap();
    let by = |id: &str| {
        acts.iter()
            .find(|a| a["id"] == id && a["kind"] == "call")
            .unwrap()
            .clone()
    };
    assert_eq!(by("9001")["via"], "deal");
    assert_eq!(by("9002")["via"], "contact");
    assert_eq!(by("9002")["via_id"], "7002", "先に辿った担当者 (主が先頭)");
    // batch_read に渡した通話 ID も重複していない
    let body: Value = {
        let s = e.hs.lock().unwrap();
        let (_, b) = s
            .log
            .iter()
            .find(|(c, _)| c.contains("calls/batch/read"))
            .unwrap();
        serde_json::from_str(b).unwrap()
    };
    assert_eq!(body["inputs"].as_array().unwrap().len(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn 活動が多いときは新しい_id_を残して上限で切り_truncated_を立てる() {
    let mut f = FakeHs::new();
    f.deal(DEAL, BPO_OWNER, UNPROCESSED, &[]);
    let ids: Vec<String> = (1..=130).map(|i| (10_000 + i).to_string()).collect();
    for (n, id) in ids.iter().enumerate() {
        let ts = format!("2026-09-{:02}T{:02}:00:00Z", 1 + (n / 24) % 28, n % 24);
        f.put(
            "notes",
            id,
            &[("hs_timestamp", &ts), ("hs_note_body", "メモ")],
        );
    }
    f.v3_assoc
        .insert((DEAL.into(), "notes".into()), ids.clone());
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        v["activities"].as_array().unwrap().len(),
        super::workspace::MAX_ACTIVITIES
    );
    assert_eq!(v["activities_truncated"], true);
    let body: Value = {
        let s = e.hs.lock().unwrap();
        let (_, b) = s
            .log
            .iter()
            .find(|(c, _)| c.contains("notes/batch/read"))
            .unwrap();
        serde_json::from_str(b).unwrap()
    };
    let sent: Vec<String> = body["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["id"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(sent.len(), super::workspace::MAX_ENGAGEMENTS_PER_TYPE);
    assert!(sent.contains(&"10130".to_string()) && !sent.contains(&"10001".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn アーカイブ済みの関連先と活動は出さない() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.archived.insert(("contacts".into(), "7002".into()));
    f.archived.insert(("notes".into(), "9101".into()));
    let e = env(f).await;
    let (_, v) = e.admin_get(DEAL).await;
    assert_eq!(v["contacts"].as_array().unwrap().len(), 1);
    assert_eq!(v["contacts"][0]["id"], "7001");
    // 主が読めないとき、2 人目を主として電話番号に使わない
    assert_eq!(v["dial"]["source"], "company");
    assert!(!act_ids(&v).contains(&"note:9101".to_string()));
}

// ---------------------------------------------------------------------------
// HubSpot の障害
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 案件の取得が_429_なら_503_で部分結果を返さない() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.fail.insert("GET /crm/v3/objects/deals/".into(), 429);
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(
        (s, kind(&v)),
        (
            StatusCode::SERVICE_UNAVAILABLE,
            Some("hubspot_rate_limited")
        )
    );
    assert!(v.get("deal").is_none());
    assert_eq!(e.total(), 1, "最初の失敗で止まる");
}

#[tokio::test(flavor = "multi_thread")]
async fn 案件の取得がタイムアウトなら_502() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.deal_delay = Duration::from_millis(1500);
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_GATEWAY, Some("hubspot_timeout"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 存在しない案件は管理者には_404() {
    let e = env(FakeHs::new()).await;
    let (s, v) = e.admin_get("424242").await;
    assert_eq!((s, kind(&v)), (StatusCode::NOT_FOUND, Some("not_found")));
}

#[tokio::test(flavor = "multi_thread")]
async fn 一部の読み取りが失敗しても案件は返り_失敗した部分を_partial_に出す() {
    for (fail_key, part) in [
        ("POST /crm/v3/objects/contacts/batch/read", "contacts"),
        ("POST /crm/v3/objects/companies/batch/read", "companies"),
        ("POST /crm/v3/objects/emails/batch/read", "emails"),
        ("POST /crm/v3/objects/calls/batch/read", "calls"),
        ("/associations/contacts/calls/", "calls_via_contacts"),
        ("/associations/deals/contacts/", "associations"),
        ("/crm/v3/pipelines/deals", "stage_labels"),
    ] {
        let mut f = FakeHs::new();
        full(&mut f, BPO_OWNER);
        f.fail.insert(fail_key.into(), 500);
        let e = env(f).await;
        let (s, v) = e.admin_get(DEAL).await;
        assert_eq!(s, StatusCode::OK, "{part}: {v}");
        assert_eq!(v["deal"]["id"], DEAL);
        let parts: Vec<&str> = v["partial"]
            .as_array()
            .unwrap()
            .iter()
            .map(|p| p["part"].as_str().unwrap())
            .collect();
        assert!(parts.contains(&part), "{part}: {parts:?}");
        assert_eq!(v["partial"][0]["error_kind"], "hubspot_upstream");
        if part == "stage_labels" {
            assert_eq!(v["deal"]["stage_label"], Value::Null, "ID を表示名にしない");
        }
        if part == "emails" {
            assert!(!act_ids(&v).iter().any(|a| a.starts_with("email:")));
            assert!(
                act_ids(&v).iter().any(|a| a.starts_with("note:")),
                "他の型は返る"
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn メールの読み取りスコープが無ければメール抜きで読み直し_partial_に出す() {
    let mut f = FakeHs::new();
    full(&mut f, BPO_OWNER);
    f.forbid_emails_get = true;
    let e = env(f).await;
    let (s, v) = e.admin_get(DEAL).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(
        v["partial"],
        json!([{"part": "emails", "error_kind": "hubspot_auth"}])
    );
    assert!(act_ids(&v).iter().any(|a| a.starts_with("call:")));
    assert!(!act_ids(&v).iter().any(|a| a.starts_with("email:")));
    assert_eq!(
        e.count("GET /crm/v3/objects/deals/5001"),
        2,
        "読み直しは 1 回だけ"
    );
    assert_eq!(e.count("emails/batch/read"), 0);
}
