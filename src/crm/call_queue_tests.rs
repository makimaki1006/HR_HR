//! `GET /api/crm/call-queue` の結合テスト (偽 HubSpot。本物の HubSpot は呼ばない)。
//!
//! 偽 HubSpot は Search の応答を「何回目の呼び出しか」で出し分け、受け取った本文と呼び出し順を記録する。
//! 絞り込み条件の正しさは **Search に送った本文** (filterGroups / sorts / after) で検証する。

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

use super::call_queue::{jst_today_ms, CallQueueState};
use super::rbac::{CrmAccess, CrmRole};
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

type Shared<T> = Arc<Mutex<T>>;

const HUBSPOT_TOKEN: &str = "test-token-XYZ";
const UPSTREAM_SECRET: &str = "UPSTREAM-SECRET-BODY";
const ADMIN: &str = "admin@f-a-c.co.jp";
const BPO: &str = "bpo@f-a-c.co.jp";
const OUTSIDER: &str = "outsider@f-a-c.co.jp";
const BPO_OWNER_ID: &str = "111";
const PIPELINE: &str = "753186575";
const UNPROCESSED: &str = "1095387442";
const FUZAI: &str = "1095387445"; // 不在 (次回架電日が来たら出す)
const KEY: [u8; 32] = [7u8; 32];

fn now_default() -> DateTime<Utc> {
    // JST 2026-10-05 12:00
    Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap()
}

fn midnight_utc_ms(y: i32, m: u32, d: u32) -> i64 {
    Utc.with_ymd_and_hms(y, m, d, 0, 0, 0)
        .unwrap()
        .timestamp_millis()
}

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

type SearchFn = Box<dyn Fn(&Value, usize) -> (u16, Value) + Send + Sync>;
type Props = Vec<(String, Option<String>)>;
/// 関連の 1 件 (toObjectId, label)
type Target = (u64, Option<String>);

struct FakeHs {
    search: SearchFn,
    search_delay: Duration,
    search_calls: usize,
    objects: HashMap<(String, String), Props>,
    /// (from, to, id) → [(toObjectId, label)]
    assocs: HashMap<(String, String, String), Vec<Target>>,
    assoc_fail: Option<u16>,
    batch_fail: HashMap<String, u16>,
    archived: HashSet<(String, String)>,
    owners: HashMap<String, String>,
    owners_fail: Option<u16>,
    /// 一覧 (email 指定なし) の (archived, after) → 応答本文。無ければ空の 1 ページ
    owner_list: HashMap<(bool, String), Value>,
    owner_list_delay: Duration,
    /// None なら pipelines API を 500 にする
    stages: Option<Vec<(String, String)>>,
    /// (method + path, body)
    log: Vec<(String, String)>,
}

impl FakeHs {
    fn new() -> Self {
        Self {
            search: Box::new(|_, _| (200, json!({"results": [], "total": 0}))),
            search_delay: Duration::ZERO,
            search_calls: 0,
            objects: HashMap::new(),
            assocs: HashMap::new(),
            assoc_fail: None,
            batch_fail: HashMap::new(),
            archived: HashSet::new(),
            owners: HashMap::from([(BPO.to_string(), BPO_OWNER_ID.to_string())]),
            owners_fail: None,
            owner_list: HashMap::new(),
            owner_list_delay: Duration::ZERO,
            stages: Some(vec![
                (UNPROCESSED.to_string(), "未済".to_string()),
                (FUZAI.to_string(), "不在".to_string()),
            ]),
            log: Vec::new(),
        }
    }
    /// n 回目の Search に n 番目のページを返す (足りなければ最後のページ)
    fn pages(mut self, pages: Vec<Page>) -> Self {
        self.search = Box::new(move |_, i| {
            let p = pages.get(i).or(pages.last()).expect("page");
            (200, p.json())
        });
        self
    }
    fn page(self, p: Page) -> Self {
        self.pages(vec![p])
    }
    fn contact(&mut self, id: &str, props: &[(&str, &str)]) {
        self.put("contacts", id, props);
    }
    fn company(&mut self, id: &str, props: &[(&str, &str)]) {
        self.put("companies", id, props);
    }
    fn put(&mut self, o: &str, id: &str, props: &[(&str, &str)]) {
        self.objects.insert(
            (o.to_string(), id.to_string()),
            props
                .iter()
                .map(|(k, v)| (k.to_string(), Some(v.to_string())))
                .collect(),
        );
    }
    fn link(&mut self, deal: &str, to: &str, targets: &[(u64, Option<&str>)]) {
        self.assocs.insert(
            ("deals".into(), to.into(), deal.into()),
            targets
                .iter()
                .map(|(i, l)| (*i, l.map(str::to_string)))
                .collect(),
        );
    }
    fn calls(&self) -> Vec<String> {
        self.log.iter().map(|(c, _)| c.clone()).collect()
    }
    fn count(&self, needle: &str) -> usize {
        self.log.iter().filter(|(c, _)| c.contains(needle)).count()
    }
    fn search_bodies(&self) -> Vec<Value> {
        self.log
            .iter()
            .filter(|(c, _)| c.ends_with("/deals/search"))
            .map(|(_, b)| serde_json::from_str(b).unwrap())
            .collect()
    }
}

/// Search の 1 ページ分
#[derive(Clone)]
struct Page {
    deals: Vec<Deal>,
    total: u64,
    next_after: Option<String>,
}

impl Page {
    fn new(deals: Vec<Deal>) -> Self {
        let total = deals.len() as u64;
        Self {
            deals,
            total,
            next_after: None,
        }
    }
    fn total(mut self, t: u64) -> Self {
        self.total = t;
        self
    }
    fn next(mut self, a: &str) -> Self {
        self.next_after = Some(a.to_string());
        self
    }
    fn json(&self) -> Value {
        let mut v = json!({
            "total": self.total,
            "results": self.deals.iter().map(Deal::json).collect::<Vec<_>>(),
        });
        if let Some(a) = &self.next_after {
            v["paging"] = json!({"next": {"after": a}});
        }
        v
    }
}

#[derive(Clone)]
struct Deal {
    id: String,
    props: Props,
    archived: bool,
}

impl Deal {
    fn new(id: &str, stage: &str) -> Self {
        let mut d = Self {
            id: id.to_string(),
            props: vec![],
            archived: false,
        };
        d.set("dealname", &format!("取引{id}"));
        d.set("dealstage", stage);
        d.set("pipeline", PIPELINE);
        d.set("hubspot_owner_id", BPO_OWNER_ID);
        d.set("bpo_29", &format!("03-0000-{id:0>4}"));
        d
    }
    fn set(&mut self, k: &str, v: &str) -> &mut Self {
        self.props.retain(|(x, _)| x != k);
        self.props.push((k.to_string(), Some(v.to_string())));
        self
    }
    fn unset(&mut self, k: &str) -> &mut Self {
        self.props.retain(|(x, _)| x != k);
        self
    }
    fn p(mut self, k: &str, v: &str) -> Self {
        self.set(k, v);
        self
    }
    fn json(&self) -> Value {
        let mut m = serde_json::Map::new();
        for (k, v) in &self.props {
            m.insert(k.clone(), v.clone().map_or(Value::Null, Value::String));
        }
        json!({"id": self.id, "properties": m, "archived": self.archived,
               "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-01T00:00:00Z"})
    }
}

async fn hs_search(
    State(st): State<Shared<FakeHs>>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let (status, v, delay) = {
        let mut s = st.lock().unwrap();
        s.log
            .push((format!("POST /crm/v3/objects/{o}/search"), body.to_string()));
        let i = s.search_calls;
        s.search_calls += 1;
        let (status, v) = (s.search)(&body, i);
        (status, v, s.search_delay)
    };
    if !delay.is_zero() {
        tokio::time::sleep(delay).await;
    }
    if status == 429 {
        return (
            StatusCode::TOO_MANY_REQUESTS,
            [("retry-after", "0")],
            UPSTREAM_SECRET,
        )
            .into_response();
    }
    (StatusCode::from_u16(status).unwrap(), Json(v)).into_response()
}

async fn hs_batch_read(
    State(st): State<Shared<FakeHs>>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log.push((
        format!("POST /crm/v3/objects/{o}/batch/read"),
        body.to_string(),
    ));
    if let Some(code) = s.batch_fail.get(&o) {
        return (StatusCode::from_u16(*code).unwrap(), UPSTREAM_SECRET).into_response();
    }
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| {
            let id = i["id"].as_str().unwrap().to_string();
            let props = s.objects.get(&(o.clone(), id.clone()))?;
            let mut m = serde_json::Map::new();
            for (k, v) in props {
                m.insert(k.clone(), v.clone().map_or(Value::Null, Value::String));
            }
            Some(json!({"id": id, "properties": m,
                        "archived": s.archived.contains(&(o.clone(), id))}))
        })
        .collect();
    if results.is_empty() {
        // 全 ID が見つからないときの 207 (results 無し・errors のみ)
        return (
            StatusCode::MULTI_STATUS,
            Json(json!({"status": "COMPLETE", "errors": [{"category": "OBJECT_NOT_FOUND"}]})),
        )
            .into_response();
    }
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn hs_assoc(
    State(st): State<Shared<FakeHs>>,
    Path((from, to)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log.push((
        format!("POST /crm/v4/associations/{from}/{to}/batch/read"),
        body.to_string(),
    ));
    if let Some(code) = s.assoc_fail {
        return (StatusCode::from_u16(code).unwrap(), UPSTREAM_SECRET).into_response();
    }
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|i| {
            let id = i["id"].as_str().unwrap().to_string();
            let targets = s.assocs.get(&(from.clone(), to.clone(), id.clone()))?;
            if targets.is_empty() {
                return None;
            }
            let to_list: Vec<Value> = targets
                .iter()
                .map(|(t, l)| {
                    json!({"toObjectId": t,
                           "associationTypes": [{"category": "USER_DEFINED", "typeId": 1, "label": l}]})
                })
                .collect();
            Some(json!({"from": {"id": id}, "to": to_list}))
        })
        .collect();
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn hs_owners(State(st): State<Shared<FakeHs>>, RawQuery(q): RawQuery) -> Response {
    let url = reqwest::Url::parse(&format!("http://x/?{}", q.clone().unwrap_or_default())).unwrap();
    let qp = |name: &str| {
        url.query_pairs()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.into_owned())
    };
    // ロックは await をまたがない (ブロックで閉じる)
    let (fail, list, delay) = {
        let mut s = st.lock().unwrap();
        s.log.push((
            "GET /crm/v3/owners".to_string(),
            q.clone().unwrap_or_default(),
        ));
        let list = if qp("email").is_none() {
            let key = (
                qp("archived").as_deref() == Some("true"),
                qp("after").unwrap_or_default(),
            );
            Some(
                s.owner_list
                    .get(&key)
                    .cloned()
                    .unwrap_or(json!({"results": []})),
            )
        } else {
            None
        };
        (s.owners_fail, list, s.owner_list_delay)
    };
    if let Some(code) = fail {
        return (StatusCode::from_u16(code).unwrap(), UPSTREAM_SECRET).into_response();
    }
    if let Some(body) = list {
        if !delay.is_zero() {
            tokio::time::sleep(delay).await;
        }
        return Json(body).into_response();
    }
    let email = qp("email").unwrap_or_default();
    let results: Vec<Value> = st
        .lock()
        .unwrap()
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
    match &s.stages {
        None => (StatusCode::INTERNAL_SERVER_ERROR, UPSTREAM_SECRET).into_response(),
        Some(stages) => Json(json!({"results": [{
            "id": PIPELINE, "label": "bpo_リクロジ", "displayOrder": 0,
            "stages": stages.iter().enumerate().map(|(i, (id, label))|
                json!({"id": id, "label": label, "displayOrder": i})).collect::<Vec<_>>()
        }]}))
        .into_response(),
    }
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
            .route("/crm/v3/objects/{o}/search", post(hs_search))
            .route("/crm/v3/objects/{o}/batch/read", post(hs_batch_read))
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

fn make_app(state: Arc<AppState>, now: DateTime<Utc>) -> Router {
    Router::new()
        .merge(super::routes::router_with_queue(
            CrmAccess::from_list(&format!("{ADMIN},{BPO}"))
                .with_test_role(ADMIN, CrmRole::Admin)
                .with_test_role(BPO, CrmRole::Bpo),
            CallQueueState::for_test(KEY, now),
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
    env_at(fake, now_default()).await
}

async fn env_at(fake: FakeHs, now: DateTime<Utc>) -> Env {
    let (client, hs) = start_hs(fake).await;
    let app = make_app(test_state(Some(client)), now);
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
    // 秘密が応答に出ないこと (全テスト共通)
    assert!(!text.contains(HUBSPOT_TOKEN), "token leaked: {text}");
    assert!(
        !text.contains(UPSTREAM_SECRET),
        "upstream body leaked: {text}"
    );
    (status, cc, v)
}

impl Env {
    async fn admin_get(&self, q: &str) -> (StatusCode, Value) {
        let (s, cc, v) = get_raw(
            &self.app,
            &format!("/api/crm/call-queue{q}"),
            Some(&self.admin),
        )
        .await;
        assert_eq!(cc, "no-store");
        (s, v)
    }
    async fn bpo_get(&self, q: &str) -> (StatusCode, Value) {
        let (s, _, v) = get_raw(
            &self.app,
            &format!("/api/crm/call-queue{q}"),
            Some(&self.bpo),
        )
        .await;
        (s, v)
    }
    fn calls(&self) -> Vec<String> {
        self.hs.lock().unwrap().calls()
    }
    fn count(&self, n: &str) -> usize {
        self.hs.lock().unwrap().count(n)
    }
    fn searches(&self) -> Vec<Value> {
        self.hs.lock().unwrap().search_bodies()
    }
}

fn groups(body: &Value) -> Vec<Value> {
    body["filterGroups"].as_array().unwrap().clone()
}

fn flt(g: &Value, prop: &str) -> Option<Value> {
    g["filters"]
        .as_array()
        .unwrap()
        .iter()
        .find(|f| f["propertyName"] == prop)
        .cloned()
}

fn ids(v: &Value) -> Vec<String> {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| i["deal_id"].as_str().unwrap().to_string())
        .collect()
}

fn kind(v: &Value) -> Option<&str> {
    v["error_kind"].as_str()
}

/// 関連つきの標準データ: Deal d に Contact (100+d) と Company (200+d) を付ける
fn with_relations(f: &mut FakeHs, deal_ids: &[&str]) {
    for d in deal_ids {
        let n: u64 = d.parse().unwrap();
        f.contact(
            &(100 + n).to_string(),
            &[
                ("firstname", "太郎"),
                ("lastname", "山田"),
                ("phone", "03-1111-0000"),
                ("jobtitle", "店長"),
            ],
        );
        f.company(
            &(200 + n).to_string(),
            &[("name", "株式会社テスト"), ("phone", "03-9999-0000")],
        );
        f.link(d, "contacts", &[(100 + n, None)]);
        f.link(d, "companies", &[(200 + n, None)]);
    }
}

// ---------------------------------------------------------------------------
// 認可: HubSpot を 1 回も呼ばない
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 権限なしは_hubspot_を呼ばない() {
    let e = env(FakeHs::new().page(Page::new(vec![Deal::new("1", UNPROCESSED)]))).await;
    // 未ログイン
    let (s, _, v) = get_raw(&e.app, "/api/crm/call-queue", None).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::UNAUTHORIZED, Some("login_required"))
    );
    // パスワードログイン
    let c = login(&e.app, ADMIN, "password_internal").await;
    let (s, _, v) = get_raw(&e.app, "/api/crm/call-queue", Some(&c)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::FORBIDDEN, Some("google_login_required"))
    );
    // 許可リスト外
    let c = login(&e.app, OUTSIDER, "google_oidc").await;
    let (s, _, v) = get_raw(&e.app, "/api/crm/call-queue", Some(&c)).await;
    assert_eq!((s, kind(&v)), (StatusCode::FORBIDDEN, Some("forbidden")));
    assert!(e.calls().is_empty(), "HubSpot を呼んだ: {:?}", e.calls());
}

#[tokio::test(flavor = "multi_thread")]
async fn hubspot_未設定は_503_で認可の後() {
    let app = make_app(test_state(None), now_default());
    let c = login(&app, ADMIN, "google_oidc").await;
    let (s, _, v) = get_raw(&app, "/api/crm/call-queue", Some(&c)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::SERVICE_UNAVAILABLE, Some("not_configured"))
    );
    let (s, _, v) = get_raw(&app, "/api/crm/call-queue", None).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::UNAUTHORIZED, Some("login_required"))
    );
}

// ---------------------------------------------------------------------------
// クエリの検証 (不正は 400。HubSpot を呼ばない)
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 不正なクエリは_400_で_hubspot_を呼ばない() {
    let e = env(FakeHs::new()).await;
    let long_q = "あ".repeat(101);
    let cases = [
        "?limit=0".to_string(),
        "?limit=51".to_string(),
        "?limit=abc".to_string(),
        "?sort=bogus".to_string(),
        "?sort=".to_string(),
        "?due=tomorrow".to_string(),
        "?stage=999".to_string(),
        "?stage=1095457875".to_string(), // 除外ステージ (アポ日確定)
        "?stage=1095457878".to_string(), // 架電禁止
        "?stage=1325086466".to_string(), // 商談実施処理
        "?owner=abc".to_string(),
        "?owner=123456789012345678901".to_string(),
        "?foo=1".to_string(),
        format!("?q={long_q}"),
    ];
    for q in cases {
        let (s, v) = e.admin_get(&q).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{q}");
        assert_eq!(v["error_kind"], "invalid_param", "{q}");
    }
    assert!(e.calls().is_empty(), "{:?}", e.calls());
    // q はちょうど 100 文字なら通る
    let (s, _) = e.admin_get(&format!("?q={}", "あ".repeat(100))).await;
    assert_eq!(s, StatusCode::OK);
}

// ---------------------------------------------------------------------------
// 0 件・通常・呼び出し回数
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn ゼロ件はエラーにせず空で返す() {
    let e = env(FakeHs::new()).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["items"], json!([]));
    assert_eq!(v["next_cursor"], Value::Null);
    assert_eq!(v["total"], 0);
    assert_eq!(v["truncated"], false);
    assert_eq!(v["partial"]["missing_contacts"], 0);
    // 既定の並びは 3 つの段階 (次回日が来た / 未架電 / 最終架電日順)。全部空なので 3 回 Search して終わる。
    // 関連・ステージ名の呼び出しは行わない
    let c = e.calls();
    assert_eq!(c.len(), 3, "{c:?}");
    assert!(c.iter().all(|x| x.ends_with("/deals/search")), "{c:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn 通常の_1_ページは_hubspot_5_回で件数に依存しない() {
    for n in [1usize, 50] {
        let mut f = FakeHs::new();
        let id_list: Vec<String> = (1..=n).map(|i| i.to_string()).collect();
        let refs: Vec<&str> = id_list.iter().map(String::as_str).collect();
        with_relations(&mut f, &refs);
        let deals: Vec<Deal> = id_list
            .iter()
            .map(|i| Deal::new(i, UNPROCESSED).p("bpo_13", "2026-10-01"))
            .collect();
        let f = f.page(Page::new(deals));
        let e = env(f).await;
        let (s, v) = e.admin_get("?limit=50").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["items"].as_array().unwrap().len(), n);
        // 初回 (ステージ名が冷えている) は pipelines が +1 = 6 回
        let mut c = e.calls();
        c.sort();
        let mut want = vec![
            "GET /crm/v3/pipelines/deals",
            "POST /crm/v3/objects/companies/batch/read",
            "POST /crm/v3/objects/contacts/batch/read",
            "POST /crm/v3/objects/deals/search",
            "POST /crm/v4/associations/deals/companies/batch/read",
            "POST /crm/v4/associations/deals/contacts/batch/read",
        ];
        want.sort();
        assert_eq!(c, want, "n={n}");
        // 2 回目 (ステージ名がキャッシュ済み) はちょうど 5 回
        let before = e.calls().len();
        let (s, _) = e.admin_get("?limit=50").await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(e.calls().len() - before, 5, "n={n}");
        assert_eq!(e.count("/crm/v3/objects/contacts/batch/read"), 2, "n={n}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 項目の形と既定の_search_本文() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1"]);
    f.link(
        "1",
        "contacts",
        &[(101, Some("主")), (777, None), (778, None)],
    );
    let deal = Deal::new("1", FUZAI)
        .p("bpo_13", "2026-10-03")
        .p("bpo_14", "9:30")
        .p("bpo_20", "2026-09-01")
        .p("bpo_10", "FAX音(コール音なし)");
    let e = env(f.page(Page::new(vec![deal]))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK);
    let it = &v["items"][0];
    assert_eq!(it["deal_id"], "1");
    assert_eq!(it["deal_name"], "取引1");
    assert_eq!(it["stage_id"], FUZAI);
    assert_eq!(it["stage_label"], "不在");
    assert_eq!(it["owner_id"], BPO_OWNER_ID);
    assert_eq!(it["next_call_date"], "2026-10-03");
    assert_eq!(it["next_call_time"], "9:30");
    assert_eq!(it["last_call_date"], "2026-09-01");
    assert_eq!(it["stop"]["unreachable_check"], "FAX音(コール音なし)");
    assert_eq!(it["stop"]["prohibited_reason"], Value::Null);
    assert_eq!(it["contact"]["id"], "101");
    assert_eq!(it["contact"]["name"], "山田 太郎");
    assert_eq!(it["contact"]["job_title"], "店長");
    assert_eq!(it["contact"]["extra_count"], 2);
    assert_eq!(it["company"]["id"], "201");
    assert_eq!(it["company"]["name"], "株式会社テスト");
    assert_eq!(it["phone"], "03-0000-0001"); // bpo_29 が最優先
    assert_eq!(it["phone_source"], "deal");
    assert!(it["deep_links"]["deal"]
        .as_str()
        .unwrap()
        .ends_with("/record/0-3/1/"));
    assert!(it["deep_links"]["contact"]
        .as_str()
        .unwrap()
        .ends_with("/record/0-1/101/"));
    assert!(it["deep_links"]["company"]
        .as_str()
        .unwrap()
        .ends_with("/record/0-2/201/"));
    assert_eq!(v["scope"]["owner"], "all");
    assert_eq!(v["scope"]["role"], "admin");
    assert_eq!(v["scope"]["due"], "all");
    assert_eq!(v["scope"]["sort"], "default");
    assert!(v["generated_at"].as_str().unwrap().ends_with('Z'));

    // Search の本文: 既定の並びの 1 段階目 (次回架電日が今日以前) = OR グループ 1 つ
    let b = &e.searches()[0];
    assert_eq!(b["limit"], 25);
    assert!(b.get("after").is_none());
    assert!(b.get("query").is_none());
    assert_eq!(
        b["sorts"],
        json!([{"propertyName": "bpo_13", "direction": "ASCENDING"}])
    );
    let gs = groups(b);
    assert_eq!(gs.len(), 1);
    let stage = flt(&gs[0], "dealstage").unwrap();
    assert_eq!(stage["operator"], "IN");
    let stages: Vec<&str> = stage["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert_eq!(stages.len(), 16, "未済 + 15 ステージ");
    assert!(stages.contains(&UNPROCESSED));
    for excluded in ["1095457875", "1095457878", "1325086466"] {
        assert!(!stages.contains(&excluded), "{excluded} は出さない");
    }
    let today = midnight_utc_ms(2026, 10, 5).to_string();
    assert_eq!(flt(&gs[0], "bpo_13").unwrap()["operator"], "LTE");
    assert_eq!(flt(&gs[0], "bpo_13").unwrap()["value"], today);
    assert_eq!(
        flt(&gs[0], "bpo_3").unwrap()["operator"],
        "NOT_HAS_PROPERTY"
    );
    assert_eq!(
        flt(&gs[0], "bpo_4").unwrap()["operator"],
        "NOT_HAS_PROPERTY"
    );
    assert!(
        flt(&gs[0], "hubspot_owner_id").is_none(),
        "管理者の既定は全員分"
    );
    let props: Vec<&str> = b["properties"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    for p in [
        "dealname",
        "dealstage",
        "pipeline",
        "hubspot_owner_id",
        "bpo_13",
        "bpo_14",
        "bpo_20",
        "bpo_3",
        "bpo_4",
        "bpo_10",
        "bpo_29",
    ] {
        assert!(props.contains(&p), "{p}");
    }
}

// ---------------------------------------------------------------------------
// 段階 (既定の並び) と cursor
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 既定の並びは三段階で_cursor_が次の段階へ進む() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2", "3"]);
    let f = f.pages(vec![
        Page::new(vec![Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")]),
        Page::new(vec![Deal::new("2", UNPROCESSED)]),
        Page::new(vec![Deal::new("3", UNPROCESSED).p("bpo_20", "2026-09-01")]),
    ]);
    let e = env(f).await;
    let (_, p1) = e.admin_get("").await;
    assert_eq!(ids(&p1), ["1"]);
    let c1 = p1["next_cursor"]
        .as_str()
        .expect("次の段階がある")
        .to_string();
    // 段階 1 の total は全体の件数ではない (他の段階を数えていない) ので null
    assert_eq!(p1["total"], Value::Null);

    let (_, p2) = e.admin_get(&format!("?cursor={c1}")).await;
    assert_eq!(ids(&p2), ["2"]);
    let c2 = p2["next_cursor"].as_str().unwrap().to_string();
    let (_, p3) = e.admin_get(&format!("?cursor={c2}")).await;
    assert_eq!(ids(&p3), ["3"]);
    assert_eq!(p3["next_cursor"], Value::Null);

    let b = e.searches();
    assert_eq!(b.len(), 3);
    // 段階 2: 未架電 (最終架電日なし)。未済のみ。次回日が無い or 明日以降
    let g2 = groups(&b[1]);
    assert_eq!(g2.len(), 2);
    let today = midnight_utc_ms(2026, 10, 5).to_string();
    let mut ops: Vec<String> = Vec::new();
    for g in &g2 {
        assert_eq!(flt(g, "dealstage").unwrap()["value"], UNPROCESSED);
        assert_eq!(flt(g, "bpo_20").unwrap()["operator"], "NOT_HAS_PROPERTY");
        let d = flt(g, "bpo_13").unwrap();
        ops.push(d["operator"].as_str().unwrap().to_string());
        if d["operator"] == "GT" {
            assert_eq!(d["value"], today);
        }
    }
    ops.sort();
    assert_eq!(ops, ["GT", "NOT_HAS_PROPERTY"]);
    assert_eq!(
        b[1]["sorts"],
        json!([{"propertyName": "hs_object_id", "direction": "ASCENDING"}])
    );
    assert!(b[1].get("after").is_none());
    // 段階 3: 最終架電日あり。最終架電日の古い順
    for g in groups(&b[2]) {
        assert_eq!(flt(&g, "bpo_20").unwrap()["operator"], "HAS_PROPERTY");
    }
    assert_eq!(
        b[2]["sorts"],
        json!([{"propertyName": "bpo_20", "direction": "ASCENDING"}])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 段階が空なら同じ要求の中で次の段階へ進む() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["2"]);
    let f = f.pages(vec![
        Page::new(vec![]),
        Page::new(vec![Deal::new("2", UNPROCESSED)]),
    ]);
    let e = env(f).await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(ids(&v), ["2"]);
    assert_eq!(e.count("/deals/search"), 2);
    assert!(v["next_cursor"].is_string(), "段階 3 が残っている");
}

#[tokio::test(flavor = "multi_thread")]
async fn 同じ段階の続きは_after_を引き継ぎ_cursor_は条件が違えば使えない() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2"]);
    let f = f.pages(vec![
        Page::new(vec![Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")])
            .total(80)
            .next("25"),
        Page::new(vec![Deal::new("2", FUZAI).p("bpo_13", "2026-10-02")]).total(80),
    ]);
    let e = env(f).await;
    let (_, p1) = e.admin_get("?due=today").await;
    assert_eq!(
        p1["total"], 80,
        "due=today は段階が 1 つなので total を出せる"
    );
    let c = p1["next_cursor"].as_str().unwrap().to_string();
    let (_, p2) = e.admin_get(&format!("?due=today&cursor={c}")).await;
    assert_eq!(ids(&p2), ["2"]);
    assert_eq!(p2["next_cursor"], Value::Null);
    assert_eq!(e.searches()[1]["after"], "25");

    // 条件を変えた cursor (limit / q / stage / owner / due / sort) は 400 で HubSpot を呼ばない
    let before = e.calls().len();
    for q in [
        format!("?due=today&limit=10&cursor={c}"),
        format!("?due=today&q=abc&cursor={c}"),
        format!("?due=today&stage={FUZAI}&cursor={c}"),
        format!("?due=today&owner=unassigned&cursor={c}"),
        format!("?due=today&sort=last_call_asc&cursor={c}"),
        format!("?cursor={c}"), // due を外した
    ] {
        let (s, v) = e.admin_get(&q).await;
        assert_eq!(
            (s, kind(&v)),
            (StatusCode::BAD_REQUEST, Some("cursor_mismatch")),
            "{q}"
        );
    }
    assert_eq!(e.calls().len(), before);
}

#[tokio::test(flavor = "multi_thread")]
async fn 壊れた_cursor_と別人の_cursor_と期限切れは_400() {
    let mk = || {
        FakeHs::new().page(
            Page::new(vec![Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")])
                .total(80)
                .next("25"),
        )
    };
    let e = env(mk()).await;
    let (_, p1) = e.admin_get("?due=today").await;
    let c = p1["next_cursor"].as_str().unwrap().to_string();
    let n = e.calls().len();

    // 改ざん (本文の 1 文字を変える / 署名を差し替える / 切り詰める / ゴミ)
    let (body, sig) = c.split_once('.').expect("body.sig");
    let flip = |s: &str| {
        let mut b: Vec<u8> = s.bytes().collect();
        b[0] = if b[0] == b'A' { b'B' } else { b'A' };
        String::from_utf8(b).unwrap()
    };
    for bad in [
        format!("{}.{}", flip(body), sig),
        format!("{}.{}", body, flip(sig)),
        body.to_string(),
        "x".to_string(),
        "....".to_string(),
        "%00".to_string(),
    ] {
        let (s, v) = e.admin_get(&format!("?due=today&cursor={bad}")).await;
        assert_eq!(
            (s, kind(&v)),
            (StatusCode::BAD_REQUEST, Some("cursor_mismatch")),
            "{bad}"
        );
    }
    // 別の人 (bpo) が admin の cursor を使う → 400 (本人でないと使えない)
    let (s, v) = e.bpo_get(&format!("?cursor={c}")).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_REQUEST, Some("cursor_mismatch"))
    );
    assert_eq!(e.calls().len(), n, "HubSpot を呼んだ");

    // 期限切れ: 31 分後の時計の同じ鍵のアプリ
    let (client, _hs) = start_hs(mk()).await;
    let later = make_app(
        test_state(Some(client)),
        now_default() + chrono::Duration::minutes(31),
    );
    let ck = login(&later, ADMIN, "google_oidc").await;
    let (s, _, v) = get_raw(
        &later,
        &format!("/api/crm/call-queue?due=today&cursor={c}"),
        Some(&ck),
    )
    .await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_REQUEST, Some("cursor_mismatch"))
    );
    // 同じ鍵・29 分後なら通る (期限切れの逆証明)
    let (client, _hs) = start_hs(mk()).await;
    let ok = make_app(
        test_state(Some(client)),
        now_default() + chrono::Duration::minutes(29),
    );
    let ck = login(&ok, ADMIN, "google_oidc").await;
    let (s, _, _) = get_raw(
        &ok,
        &format!("/api/crm/call-queue?due=today&cursor={c}"),
        Some(&ck),
    )
    .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn 日付が変わると前日の_cursor_は使えない() {
    let e = env_at(
        FakeHs::new().page(
            Page::new(vec![Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")])
                .total(80)
                .next("25"),
        ),
        Utc.with_ymd_and_hms(2026, 10, 5, 14, 59, 0).unwrap(), // JST 23:59
    )
    .await;
    let (_, p1) = e.admin_get("?due=today").await;
    let c = p1["next_cursor"].as_str().unwrap().to_string();
    // JST 0 時を過ぎたアプリ (同じ鍵)
    let (client, _) = start_hs(FakeHs::new()).await;
    let next_day = make_app(
        test_state(Some(client)),
        Utc.with_ymd_and_hms(2026, 10, 5, 15, 0, 0).unwrap(),
    );
    let ck = login(&next_day, ADMIN, "google_oidc").await;
    let (s, _, v) = get_raw(
        &next_day,
        &format!("/api/crm/call-queue?due=today&cursor={c}"),
        Some(&ck),
    )
    .await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_REQUEST, Some("cursor_mismatch"))
    );
}

// ---------------------------------------------------------------------------
// 1 万件の上限
// ---------------------------------------------------------------------------

async fn truncation_case(next_after: &str) -> Value {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1"]);
    let f = f.pages(vec![Page::new(vec![
        Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")
    ])
    .total(12000)
    .next(next_after)]);
    let e = env(f).await;
    let (s, v) = e.admin_get("?due=today&limit=50").await;
    assert_eq!(s, StatusCode::OK);
    v
}

#[tokio::test(flavor = "multi_thread")]
async fn 一万件に届いたら_truncated_で止まり_cursor_は進まない() {
    // 次の after が 10000 (= もう取れない)
    let v = truncation_case("10000").await;
    assert_eq!(v["truncated"], true);
    assert_eq!(
        v["next_cursor"],
        Value::Null,
        "上限を超える cursor は出さない"
    );
    assert_eq!(v["total"], 12000, "件数を偽らない (実際の total を出す)");
    // 次ページが after=9960 + 50 件 = 10010 件目を要求する → 取れない
    let v = truncation_case("9960").await;
    assert_eq!(v["truncated"], true);
    assert_eq!(v["next_cursor"], Value::Null);
    // 上限に届く前は truncated = false で cursor が出る
    let v = truncation_case("50").await;
    assert_eq!(v["truncated"], false);
    assert!(v["next_cursor"].is_string());
    // after=9950 + 50 件 = 10000 件目までは取れる (境界)
    let v = truncation_case("9950").await;
    assert_eq!(v["truncated"], false);
    assert!(v["next_cursor"].is_string());
}

// ---------------------------------------------------------------------------
// 関連の欠落・部分成功・失敗
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 関連が欠けても行は落とさず_partial_に実数を出す() {
    let mut f = FakeHs::new();
    // 1: contact も company もある / 2: contact なし / 3: company なし / 4: どちらもなし
    with_relations(&mut f, &["1", "2", "3", "4"]);
    f.link("2", "contacts", &[]);
    f.link("3", "companies", &[]);
    f.link("4", "contacts", &[]);
    f.link("4", "companies", &[]);
    let deals: Vec<Deal> = ["1", "2", "3", "4"]
        .iter()
        .map(|i| Deal::new(i, UNPROCESSED).p("bpo_13", "2026-10-01"))
        .collect();
    let e = env(f.page(Page::new(deals))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(ids(&v), ["1", "2", "3", "4"]);
    let items = v["items"].as_array().unwrap();
    assert!(items[0]["contact"].is_object() && items[0]["company"].is_object());
    assert!(items[1]["contact"].is_null() && items[1]["company"].is_object());
    assert!(items[2]["contact"].is_object() && items[2]["company"].is_null());
    assert!(items[3]["contact"].is_null() && items[3]["company"].is_null());
    assert_eq!(v["partial"]["missing_contacts"], 2);
    assert_eq!(v["partial"]["missing_companies"], 2);
    assert_eq!(v["partial"]["failed"], json!([]));
}

#[tokio::test(flavor = "multi_thread")]
async fn 一部の_id_が返らない_207_は欠落扱いで全体は成功() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2"]);
    // Contact 102 は batch read で見つからない (Contact 自体が無い)
    f.objects
        .remove(&("contacts".to_string(), "102".to_string()));
    let deals = vec![Deal::new("1", UNPROCESSED), Deal::new("2", UNPROCESSED)];
    let e = env(f.page(Page::new(deals))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["items"][0]["contact"].is_object());
    assert!(v["items"][1]["contact"].is_null());
    assert_eq!(v["partial"]["missing_contacts"], 1);

    // 全部見つからない (results 無しの 207) でも成功
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1"]);
    f.objects.clear();
    let e = env(f.page(Page::new(vec![Deal::new("1", UNPROCESSED)]))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["partial"]["missing_contacts"], 1);
    assert_eq!(v["partial"]["missing_companies"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn search_成功_関連失敗は_200_と_partial() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2"]);
    f.assoc_fail = Some(500);
    f.batch_fail.insert("companies".into(), 403);
    f.stages = None; // ステージ名の取得も失敗
                     // 2 は bpo_29 が無い。関連を取れていないので電話番号の有無を判断できず、外さない
    let mut d2 = Deal::new("2", UNPROCESSED);
    d2.unset("bpo_29");
    let e = env(f.page(Page::new(vec![Deal::new("1", UNPROCESSED), d2]))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(ids(&v), ["1", "2"], "行は落とさない");
    let failed: Vec<&str> = v["partial"]["failed"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect();
    assert!(failed.contains(&"associations"), "{failed:?}");
    assert!(failed.contains(&"stage_labels"), "{failed:?}");
    assert!(
        v["items"][0]["stage_label"].is_null(),
        "ID を表示名にしない"
    );
    assert!(v["items"][1]["phone"].is_null());
    assert_eq!(v["partial"]["excluded"]["no_phone"], 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn contact_の読み取りだけ失敗しても行は返る() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1"]);
    f.batch_fail.insert("contacts".into(), 500);
    let e = env(f.page(Page::new(vec![Deal::new("1", UNPROCESSED)]))).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["items"][0]["contact"].is_null());
    assert!(v["items"][0]["company"].is_object());
    assert!(v["partial"]["failed"]
        .as_array()
        .unwrap()
        .iter()
        .any(|x| x == "contacts"));
}

#[tokio::test(flavor = "multi_thread")]
async fn アーカイブ済みは_deal_は出さず関連先は欠落扱い() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2"]);
    f.archived.insert(("contacts".into(), "102".into()));
    let mut gone = Deal::new("3", UNPROCESSED);
    gone.archived = true;
    let e = env(f.page(Page::new(vec![
        Deal::new("1", UNPROCESSED),
        Deal::new("2", UNPROCESSED),
        gone,
    ])))
    .await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(ids(&v), ["1", "2"]);
    assert!(v["items"][1]["contact"].is_null());
    assert_eq!(v["partial"]["missing_contacts"], 1);
    assert_eq!(v["partial"]["excluded"]["out_of_scope"], 1);
}

// ---------------------------------------------------------------------------
// エラー: 429 / タイムアウト / 認証失敗
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn search_が_429_なら_503_で部分結果を成功として返さない() {
    let mut f = FakeHs::new();
    f.search = Box::new(|_, _| (429, json!({})));
    let e = env(f).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error_kind"], "hubspot_rate_limited");
    assert!(v.get("items").is_none());
    assert_eq!(
        e.count("/deals/search"),
        1,
        "最初の 429 で止まる (他の段階へ進まない)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn search_がタイムアウトなら_502() {
    let mut f = FakeHs::new();
    f.search_delay = Duration::from_millis(1500);
    let e = env(f).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_GATEWAY, Some("hubspot_timeout"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn search_が_401_なら_502_hubspot_auth() {
    let mut f = FakeHs::new();
    f.search = Box::new(|_, _| (401, json!({"message": "UPSTREAM-SECRET-BODY"})));
    let e = env(f).await;
    let (s, v) = e.admin_get("").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_GATEWAY, Some("hubspot_auth"))
    );
}

// ---------------------------------------------------------------------------
// 役割と担当者
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 管理者以外も他人_全員分_担当なしを指定して見られる() {
    let e = env(FakeHs::new()).await;
    // (クエリ, scope.owner, 検索に入る owner の絞り込み: None = 絞り込みなし / Some(None) = 担当なし / Some(Some(id)))
    #[allow(clippy::type_complexity)]
    let cases: [(&str, &str, Option<Option<&str>>); 4] = [
        ("?owner=all", "all", None),
        ("?owner=unassigned", "unassigned", Some(None)),
        ("?owner=999", "999", Some(Some("999"))),
        ("?owner=555", "555", Some(Some("555"))),
    ];
    for (q, scope_owner, want) in cases {
        let before = e.searches().len();
        let (s, v) = e.bpo_get(q).await;
        assert_eq!(s, StatusCode::OK, "{q}: {v}");
        assert_eq!(v["scope"]["owner"], scope_owner, "{q}");
        // 管理者の画面の「管理者」と区別するための role は own のまま (既定が自分かどうかの目印)
        assert_eq!(v["scope"]["role"], "own", "{q}");
        let searches = e.searches();
        assert!(searches.len() > before, "{q}: 検索していない");
        for b in &searches[before..] {
            for g in groups(b) {
                match (want, flt(&g, "hubspot_owner_id")) {
                    (None, f) => assert!(f.is_none(), "{q}: {f:?}"),
                    (Some(None), Some(f)) => assert_eq!(f["operator"], "NOT_HAS_PROPERTY", "{q}"),
                    (Some(Some(id)), Some(f)) => {
                        assert_eq!(
                            (f["operator"].as_str(), f["value"].as_str()),
                            (Some("EQ"), Some(id)),
                            "{q}"
                        )
                    }
                    (w, f) => panic!("{q}: want {w:?} got {f:?}"),
                }
            }
        }
    }
    // 他人を指定するときは自分の owner を引かない (HubSpot の Owners を呼ばない)
    assert_eq!(e.count("/crm/v3/owners"), 0, "{:?}", e.calls());
}

#[tokio::test(flavor = "multi_thread")]
async fn bpo_は自分の_owner_だけ_search_に入り_owner_はキャッシュされる() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1"]);
    let e = env(f.page(Page::new(vec![Deal::new("1", UNPROCESSED)]))).await;
    for q in ["", "?owner=me"] {
        let (s, v) = e.bpo_get(q).await;
        assert_eq!(s, StatusCode::OK, "{q}: {v}");
        assert_eq!(v["scope"]["owner"], "me");
        assert_eq!(v["scope"]["role"], "own");
    }
    // 全ての OR グループに、自分の owner の絞り込みが入っている
    for b in e.searches() {
        for g in groups(&b) {
            let o = flt(&g, "hubspot_owner_id").expect("owner の絞り込み");
            assert_eq!(
                (o["operator"].as_str(), o["value"].as_str()),
                (Some("EQ"), Some(BPO_OWNER_ID))
            );
        }
    }
    assert_eq!(
        e.count("/crm/v3/owners"),
        1,
        "owner の対応は 1 回だけ引く (キャッシュ)"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 自分の_owner_が見つからない人の既定は_409_で選択を促し_全員分に倒さない() {
    let mut f = FakeHs::new();
    f.owners.clear();
    let e = env(f).await;
    let (s, v) = e.bpo_get("").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::CONFLICT, Some("owner_not_resolved"))
    );
    assert_eq!(e.count("/deals/search"), 0, "全員分を返さない");
    // 引けなかった結果も短時間はキャッシュして連打で Owners API を叩かない
    let _ = e.bpo_get("").await;
    assert_eq!(e.count("/crm/v3/owners"), 1);
    // 明示の me も同じ。all / 他人の指定なら見られる (選択すれば進める)
    let (s, v) = e.bpo_get("?owner=me").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::CONFLICT, Some("owner_not_resolved"))
    );
    let (s, _) = e.bpo_get("?owner=all").await;
    assert_eq!(s, StatusCode::OK);
    let (s, _) = e.bpo_get("?owner=777").await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_api_の失敗は_bpo_を全員分に倒さない() {
    let mut f = FakeHs::new();
    f.owners_fail = Some(403);
    let e = env(f).await;
    let (s, v) = e.bpo_get("").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::BAD_GATEWAY, Some("hubspot_auth"))
    );
    assert_eq!(e.count("/deals/search"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者は_owner_を選べる() {
    let e = env(FakeHs::new()).await;
    // unassigned = 担当なし
    let (_, v) = e.admin_get("?owner=unassigned&due=today").await;
    assert_eq!(v["scope"]["owner"], "unassigned");
    let g = groups(&e.searches()[0]);
    assert_eq!(
        flt(&g[0], "hubspot_owner_id").unwrap()["operator"],
        "NOT_HAS_PROPERTY"
    );
    // owner id
    let (_, v) = e.admin_get("?owner=555&due=today").await;
    assert_eq!(v["scope"]["owner"], "555");
    let g = groups(&e.searches()[1]);
    assert_eq!(flt(&g[0], "hubspot_owner_id").unwrap()["value"], "555");
    // all を明示 = 絞り込みなし。me は管理者のメールが HubSpot に無いので 409 owner_not_resolved
    let (_, v) = e.admin_get("?owner=all&due=today").await;
    assert_eq!(v["scope"]["owner"], "all");
    assert!(flt(&groups(&e.searches()[2])[0], "hubspot_owner_id").is_none());
    let (s, v) = e.admin_get("?owner=me").await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::CONFLICT, Some("owner_not_resolved"))
    );
}

// ---------------------------------------------------------------------------
// 日付境界 (JST)
// ---------------------------------------------------------------------------

#[test]
fn 今日は_jst_で数え_utc_0_時のエポック_ms_に直す() {
    let t = |y, m, d, h, mi| Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap();
    // JST 10/05 23:59 (= UTC 10/05 14:59) はまだ 10/05
    assert_eq!(
        jst_today_ms(t(2026, 10, 5, 14, 59)),
        midnight_utc_ms(2026, 10, 5)
    );
    // JST 10/06 00:00 (= UTC 10/05 15:00) は 10/06
    assert_eq!(
        jst_today_ms(t(2026, 10, 5, 15, 0)),
        midnight_utc_ms(2026, 10, 6)
    );
    // JST 10/05 00:00 (= UTC 10/04 15:00) は 10/05。1 分前は 10/04
    assert_eq!(
        jst_today_ms(t(2026, 10, 4, 15, 0)),
        midnight_utc_ms(2026, 10, 5)
    );
    assert_eq!(
        jst_today_ms(t(2026, 10, 4, 14, 59)),
        midnight_utc_ms(2026, 10, 4)
    );
    // 月末・年末またぎ
    assert_eq!(
        jst_today_ms(t(2026, 12, 31, 15, 0)),
        midnight_utc_ms(2027, 1, 1)
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn search_に渡す今日は_jst_の日付() {
    for (now, want) in [
        (
            Utc.with_ymd_and_hms(2026, 10, 5, 14, 59, 0).unwrap(),
            midnight_utc_ms(2026, 10, 5),
        ),
        (
            Utc.with_ymd_and_hms(2026, 10, 5, 15, 0, 0).unwrap(),
            midnight_utc_ms(2026, 10, 6),
        ),
    ] {
        let e = env_at(FakeHs::new(), now).await;
        let _ = e.admin_get("?due=today").await;
        let g = groups(&e.searches()[0]);
        assert_eq!(
            flt(&g[0], "bpo_13").unwrap()["value"],
            want.to_string(),
            "{now}"
        );
    }
}

// ---------------------------------------------------------------------------
// 停止系・電話番号・担当者の選び方・重複
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 停止系は_search_でも後段でも外し_不通時チェックは残して印() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2", "3", "4"]);
    // 1: 禁止理由あり / 2: ブロック理由あり / 3: 不通時チェックのみ (禁止理由は空白だけ = 入力なし) / 4: 何もなし。
    // (Search が NOT_HAS_PROPERTY を取りこぼして返してきても後段で外す)
    let d1 = Deal::new("1", UNPROCESSED).p("bpo_3", "クレーム");
    let d2 = Deal::new("2", UNPROCESSED).p("bpo_4", "廃業");
    let d3 = Deal::new("3", UNPROCESSED)
        .p("bpo_10", "常時通話中")
        .p("bpo_3", "  ");
    let d4 = Deal::new("4", UNPROCESSED);
    let e = env(f.page(Page::new(vec![d1, d2, d3, d4]))).await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(ids(&v), ["3", "4"]);
    assert_eq!(v["items"][0]["stop"]["unreachable_check"], "常時通話中");
    assert!(v["items"][1]["stop"]["unreachable_check"].is_null());
    assert_eq!(v["partial"]["excluded"]["stop_reason"], 2);
    // 全段階・全 OR グループに bpo_3 / bpo_4 の NOT_HAS_PROPERTY がある
    let e2 = env(FakeHs::new()).await;
    let _ = e2.admin_get("").await;
    assert_eq!(e2.searches().len(), 3);
    for b in e2.searches() {
        for g in groups(&b) {
            assert_eq!(flt(&g, "bpo_3").unwrap()["operator"], "NOT_HAS_PROPERTY");
            assert_eq!(flt(&g, "bpo_4").unwrap()["operator"], "NOT_HAS_PROPERTY");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 電話番号の優先順位と番号なしの除外() {
    let mut f = FakeHs::new();
    // 1: bpo_29 / 2: contact phone / 3: contact mobile のみ / 4: company のみ / 5: どこにもない
    type Case<'a> = (&'a str, Vec<(&'a str, &'a str)>, Vec<(&'a str, &'a str)>);
    let cases: Vec<Case> = vec![
        ("1", vec![("phone", "03-1")], vec![("phone", "03-9")]),
        (
            "2",
            vec![("phone", "03-2"), ("mobilephone", "090-2")],
            vec![("phone", "03-9")],
        ),
        ("3", vec![("mobilephone", "090-3")], vec![("phone", "03-9")]),
        ("4", vec![], vec![("phone", "03-4")]),
        ("5", vec![], vec![]),
    ];
    for (d, c_props, co_props) in cases {
        let n: u64 = d.parse().unwrap();
        f.contact(&(100 + n).to_string(), &c_props);
        f.company(&(200 + n).to_string(), &co_props);
        f.link(d, "contacts", &[(100 + n, None)]);
        f.link(d, "companies", &[(200 + n, None)]);
    }
    let mut deals: Vec<Deal> = ["1", "2", "3", "4", "5"]
        .iter()
        .map(|i| Deal::new(i, UNPROCESSED))
        .collect();
    for d in deals.iter_mut().skip(1) {
        d.unset("bpo_29");
    }
    let e = env(f.page(Page::new(deals))).await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(
        ids(&v),
        ["1", "2", "3", "4"],
        "5 は番号がどこにも無いので外す"
    );
    let got: Vec<(String, String)> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|i| {
            (
                i["phone"].as_str().unwrap().to_string(),
                i["phone_source"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    assert_eq!(
        got,
        [
            ("03-0000-0001".to_string(), "deal".to_string()),
            ("03-2".to_string(), "contact".to_string()),
            ("090-3".to_string(), "mobile".to_string()),
            ("03-4".to_string(), "company".to_string()),
        ]
    );
    assert_eq!(v["partial"]["excluded"]["no_phone"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn 担当者は主ラベルがあればそれ_なければ最初の_1_人と人数() {
    let mut f = FakeHs::new();
    f.contact(
        "11",
        &[("firstname", "一"), ("lastname", "甲"), ("phone", "03-11")],
    );
    f.contact(
        "12",
        &[("firstname", "二"), ("lastname", "乙"), ("phone", "03-12")],
    );
    f.contact(
        "13",
        &[("firstname", "三"), ("lastname", "丙"), ("phone", "03-13")],
    );
    f.link("1", "contacts", &[(11, None), (12, Some("主")), (13, None)]);
    f.link("2", "contacts", &[(11, None), (13, None)]);
    f.link("3", "contacts", &[(13, None)]);
    let mut deals = vec![
        Deal::new("1", UNPROCESSED),
        Deal::new("2", UNPROCESSED),
        Deal::new("3", UNPROCESSED),
    ];
    for d in deals.iter_mut() {
        d.unset("bpo_29");
    }
    let e = env(f.page(Page::new(deals))).await;
    let (_, v) = e.admin_get("").await;
    let c = |i: usize| {
        (
            v["items"][i]["contact"]["id"].clone(),
            v["items"][i]["contact"]["extra_count"].clone(),
        )
    };
    assert_eq!(c(0), (json!("12"), json!(2)));
    assert_eq!(c(1), (json!("11"), json!(1)));
    assert_eq!(c(2), (json!("13"), json!(0)));
}

#[tokio::test(flavor = "multi_thread")]
async fn 同じ_contact_を共有する取引は_id_を重複させず_1_回で読む() {
    let mut f = FakeHs::new();
    f.contact("50", &[("phone", "03-50")]);
    f.company("60", &[("name", "共有会社")]);
    for d in ["1", "2", "3"] {
        f.link(d, "contacts", &[(50, None)]);
        f.link(d, "companies", &[(60, None)]);
    }
    let deals = vec![
        Deal::new("1", UNPROCESSED),
        Deal::new("2", UNPROCESSED),
        Deal::new("3", UNPROCESSED),
    ];
    let e = env(f.page(Page::new(deals))).await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(ids(&v), ["1", "2", "3"]);
    assert!(v["items"]
        .as_array()
        .unwrap()
        .iter()
        .all(|i| i["contact"]["id"] == "50"));
    let hs = e.hs.lock().unwrap();
    let mut checked = 0;
    for (c, body) in &hs.log {
        if c == "POST /crm/v3/objects/contacts/batch/read"
            || c == "POST /crm/v3/objects/companies/batch/read"
        {
            let b: Value = serde_json::from_str(body).unwrap();
            assert_eq!(b["inputs"].as_array().unwrap().len(), 1, "{body}");
            checked += 1;
        }
    }
    assert_eq!(checked, 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn 別パイプラインと許可外ステージの取引は後段でも外す() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2", "3"]);
    let other_pl = Deal::new("2", UNPROCESSED).p("pipeline", "default");
    let excluded_stage = Deal::new("3", "1095457878"); // 架電禁止
    let e = env(f.page(Page::new(vec![
        Deal::new("1", UNPROCESSED),
        other_pl,
        excluded_stage,
    ])))
    .await;
    let (_, v) = e.admin_get("").await;
    assert_eq!(ids(&v), ["1"]);
    assert_eq!(v["partial"]["excluded"]["out_of_scope"], 2);
}

// ---------------------------------------------------------------------------
// 絞り込み: stage / q / sort / due と、HubSpot の上限への収まり
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn stage_は部分集合だけ_q_は_query_に入る() {
    let e = env(FakeHs::new()).await;
    // 未済 + 不在 (次回日が来たもの)
    let (s, v) = e
        .admin_get(&format!(
            "?stage={UNPROCESSED}&stage={FUZAI}&q=%E3%83%86%E3%82%B9%E3%83%88&due=today&limit=10"
        ))
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        v["scope"]["stages"],
        json!([UNPROCESSED, FUZAI]),
        "並べ替えて重複なしで返す"
    );
    assert_eq!(v["scope"]["q"], "テスト");
    let b = &e.searches()[0];
    assert_eq!(b["query"], "テスト");
    assert_eq!(b["limit"], 10);
    let g = groups(b);
    let mut values: Vec<String> = flt(&g[0], "dealstage").unwrap()["values"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap().to_string())
        .collect();
    values.sort();
    assert_eq!(values, [UNPROCESSED, FUZAI]);

    // 未済を含まない stage では「未済」だけの段階 (既定 sort の 2・3 段階) は検索しない
    let e = env(FakeHs::new()).await;
    let (_, _) = e.admin_get(&format!("?stage={FUZAI}")).await;
    assert_eq!(e.searches().len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn sort_の許可値は並びを切り替える() {
    type Sorts<'a> = Vec<(&'a str, &'a str)>;
    let cases: [(&str, Sorts); 4] = [
        (
            "default",
            vec![
                ("bpo_13", "ASCENDING"),
                ("hs_object_id", "ASCENDING"),
                ("bpo_20", "ASCENDING"),
            ],
        ),
        (
            "next_call_desc",
            vec![
                ("bpo_13", "DESCENDING"),
                ("hs_object_id", "ASCENDING"),
                ("bpo_20", "ASCENDING"),
            ],
        ),
        (
            "last_call_asc",
            vec![("hs_object_id", "ASCENDING"), ("bpo_20", "ASCENDING")],
        ),
        (
            "last_call_desc",
            vec![("bpo_20", "DESCENDING"), ("hs_object_id", "ASCENDING")],
        ),
    ];
    for (sort, want) in cases {
        let mut f = FakeHs::new();
        with_relations(&mut f, &["1"]);
        // どの段階でも 1 件返す。cursor を最後まで辿る
        let f = f.pages(vec![Page::new(vec![Deal::new("1", UNPROCESSED)])]);
        let e = env(f).await;
        let mut q = format!("?sort={sort}");
        for _ in 0..5 {
            let (s, v) = e.admin_get(&q).await;
            assert_eq!(s, StatusCode::OK);
            assert_eq!(v["scope"]["sort"], sort);
            match v["next_cursor"].as_str() {
                Some(c) => q = format!("?sort={sort}&cursor={c}"),
                None => break,
            }
        }
        let got: Vec<(String, String)> = e
            .searches()
            .iter()
            .map(|b| {
                let s0 = &b["sorts"][0];
                (
                    s0["propertyName"].as_str().unwrap().to_string(),
                    s0["direction"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        let want: Vec<(String, String)> = want
            .iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect();
        assert_eq!(got, want, "{sort}");
    }
}

/// HubSpot Search の上限 (OR グループ 5、グループあたりフィルタ 6、全体 18) に、
/// あらゆる組み合わせ (並び × due × owner × stage) で収まる。
#[tokio::test(flavor = "multi_thread")]
async fn 全ての組み合わせで_search_の上限に収まる() {
    let mut combos = 0;
    let mut searched = 0;
    for sort in [
        "default",
        "next_call_desc",
        "last_call_asc",
        "last_call_desc",
    ] {
        for due in ["all", "today"] {
            for owner in ["all", "unassigned", "555"] {
                for stage in [
                    "",
                    "&stage=1095387442",
                    "&stage=1095387445",
                    "&stage=1095387442&stage=1095387445",
                ] {
                    let e = env(FakeHs::new()).await;
                    // 全部空なので 1 要求で全段階を検索する
                    let (s, v) = e
                        .admin_get(&format!("?sort={sort}&due={due}&owner={owner}{stage}"))
                        .await;
                    assert_eq!(s, StatusCode::OK, "{sort} {due} {owner} {stage}: {v}");
                    assert_eq!(v["next_cursor"], Value::Null);
                    for b in e.searches() {
                        searched += 1;
                        let gs = groups(&b);
                        assert!(!gs.is_empty() && gs.len() <= 5, "グループ {}", gs.len());
                        let total: usize = gs
                            .iter()
                            .map(|g| g["filters"].as_array().unwrap().len())
                            .sum();
                        assert!(total <= 18, "全体 {total}: {sort} {due} {owner} {stage}");
                        for g in &gs {
                            let n = g["filters"].as_array().unwrap().len();
                            assert!(n <= 6, "グループあたり {n}: {sort} {due} {owner} {stage}");
                            // 必ず bpo_3 / bpo_4 を外している
                            assert!(flt(g, "bpo_3").is_some() && flt(g, "bpo_4").is_some());
                        }
                    }
                    combos += 1;
                }
            }
        }
    }
    assert_eq!(combos, 4 * 2 * 3 * 4);
    assert!(searched >= combos, "検索が行われている ({searched})");
}

#[tokio::test(flavor = "multi_thread")]
async fn due_today_は次回日が来たものだけ() {
    let e = env(FakeHs::new()).await;
    let (_, v) = e.admin_get("?due=today").await;
    assert_eq!(v["scope"]["due"], "today");
    let bodies = e.searches();
    assert_eq!(bodies.len(), 1, "段階は 1 つ");
    for g in groups(&bodies[0]) {
        assert_eq!(flt(&g, "bpo_13").unwrap()["operator"], "LTE");
    }
}

// ---------------------------------------------------------------------------
// PR-2: 日付の範囲 (next_from / next_to / last_from / last_to) と sort=next_call_asc
// ---------------------------------------------------------------------------

fn bpo13_filters(g: &Value, prop: &str) -> Vec<(String, String)> {
    g["filters"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|f| f["propertyName"] == prop)
        .map(|f| {
            (
                f["operator"].as_str().unwrap().to_string(),
                f["value"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn 日付の範囲と未知の_sort_の不正値は_400_で_hubspot_を呼ばない() {
    let e = env(FakeHs::new()).await;
    let cases = [
        "?next_from=2026-13-01",
        "?next_from=2026-02-30",
        "?next_from=20261005",
        "?next_from=2026-1-5",
        "?next_from=",
        "?next_from=2026-10-05T00:00:00Z",
        "?next_from=1999-12-31",
        "?next_from=2101-01-01",
        "?next_to=abc",
        "?last_from=2026-10-32",
        "?last_to=2026/10/05",
        "?next_from=2026-10-10&next_to=2026-10-09",
        "?last_from=2026-10-10&last_to=2026-10-09",
        "?next_from=2026-10-01&next_from=2026-10-02",
        "?last_to=2026-10-01&last_to=2026-10-02",
        "?sort=next_call",
        "?sort=NEXT_CALL_ASC",
        "?sort=next_call_asc&sort=default",
    ];
    for q in cases {
        let (s, v) = e.admin_get(q).await;
        assert_eq!(s, StatusCode::BAD_REQUEST, "{q}");
        assert_eq!(v["error_kind"], "invalid_param", "{q}");
    }
    assert!(e.calls().is_empty(), "{:?}", e.calls());
    // from == to は通る (1 日だけの範囲)
    let (s, _) = e
        .admin_get("?next_from=2026-10-01&next_to=2026-10-01")
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn sort_next_call_asc_は_default_と同じ並び() {
    async fn run(q: &str) -> (Value, Vec<(String, String)>) {
        let e = env(FakeHs::new()).await;
        let (s, v) = e.admin_get(q).await;
        assert_eq!(s, StatusCode::OK);
        let got = e
            .searches()
            .iter()
            .map(|b| {
                (
                    b["sorts"][0]["propertyName"].as_str().unwrap().to_string(),
                    b["sorts"][0]["direction"].as_str().unwrap().to_string(),
                )
            })
            .collect();
        (v["scope"]["sort"].clone(), got)
    }
    let (a_scope, a) = run("?sort=next_call_asc").await;
    let (_, b) = run("?sort=default").await;
    assert_eq!(a_scope, "next_call_asc");
    assert_eq!(a, b);
    assert_eq!(a[0], ("bpo_13".to_string(), "ASCENDING".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn 次回架電日の範囲は_jst_の日付を_utc_0_時_ms_にして_search_に入る() {
    // 今日 = JST 2026-10-05
    let d = |day: u32| midnight_utc_ms(2026, 10, day).to_string();
    let e = env(FakeHs::new()).await;
    let (s, v) = e
        .admin_get("?next_from=2026-10-01&next_to=2026-10-31")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["scope"]["next_from"], "2026-10-01");
    assert_eq!(v["scope"]["next_to"], "2026-10-31");
    assert!(v["scope"]["last_from"].is_null());
    let bodies = e.searches();
    // 段階 0: 今日以前 → 下端 10-01、上端は今日 (10-05) と 10-31 の小さい方
    let g0 = groups(&bodies[0]);
    assert_eq!(g0.len(), 1);
    assert_eq!(
        bpo13_filters(&g0[0], "bpo_13"),
        [("GTE".to_string(), d(1)), ("LTE".to_string(), d(5))]
    );
    // 段階 1 以降 (未済で次回日が明日以降): 下端は max(10-01, 明日 10-06)、上端は 10-31。日付なしの OR グループは無い
    assert!(bodies.len() >= 2);
    for b in &bodies[1..] {
        let gs = groups(b);
        assert_eq!(gs.len(), 1, "{b}");
        assert_eq!(
            bpo13_filters(&gs[0], "bpo_13"),
            [("GTE".to_string(), d(6)), ("LTE".to_string(), d(31))]
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 次回架電日の範囲が片側の時間帯だけなら当てはまらない段階は検索しない() {
    // 範囲 10-01〜10-03 は今日 (10-05) より前: 「明日以降の未済」の段階には当てはまる日が無い
    let e = env(FakeHs::new()).await;
    let (s, _) = e
        .admin_get("?next_from=2026-10-01&next_to=2026-10-03")
        .await;
    assert_eq!(s, StatusCode::OK);
    let bodies = e.searches();
    assert_eq!(bodies.len(), 1, "{bodies:?}");
    // 範囲が未来だけ (10-10〜10-20): 今日以前の段階には当てはまる日が無い。残るのは未済だけ
    let e = env(FakeHs::new()).await;
    let (s, _) = e
        .admin_get("?next_from=2026-10-10&next_to=2026-10-20")
        .await;
    assert_eq!(s, StatusCode::OK);
    let bodies = e.searches();
    assert!(!bodies.is_empty());
    for b in &bodies {
        for g in groups(b) {
            assert_eq!(flt(&g, "dealstage").unwrap()["value"], UNPROCESSED);
            let f = bpo13_filters(&g, "bpo_13");
            assert_eq!(f[0].0, "GTE");
            assert_eq!(f[0].1, midnight_utc_ms(2026, 10, 10).to_string());
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 最終架電日の範囲は_bpo_20_の_gte_lte_で_架電なしの段階は検索しない() {
    let d = |day: u32| midnight_utc_ms(2026, 9, day).to_string();
    let e = env(FakeHs::new()).await;
    let (s, v) = e
        .admin_get("?last_from=2026-09-01&last_to=2026-09-30")
        .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["scope"]["last_from"], "2026-09-01");
    assert_eq!(v["scope"]["last_to"], "2026-09-30");
    let bodies = e.searches();
    // 既定は 3 段階だが、最終架電日なし (NOT_HAS) の段階は範囲と矛盾するので出さない => 2 段階
    assert_eq!(bodies.len(), 2);
    for b in &bodies {
        for g in groups(b) {
            assert_eq!(
                bpo13_filters(&g, "bpo_20"),
                [("GTE".to_string(), d(1)), ("LTE".to_string(), d(30))],
                "{g}"
            );
        }
    }
    // 片側だけ
    let e = env(FakeHs::new()).await;
    let (s, _) = e.admin_get("?last_from=2026-09-01").await;
    assert_eq!(s, StatusCode::OK);
    for b in e.searches() {
        for g in groups(&b) {
            assert_eq!(flt(&g, "bpo_20").unwrap()["operator"], "GTE");
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 範囲を含む全ての組み合わせで_search_の上限に収まる() {
    let nexts = [
        "",
        "&next_from=2026-10-01",
        "&next_to=2026-10-31",
        "&next_from=2026-10-01&next_to=2026-10-31",
        "&next_from=2026-10-10&next_to=2026-10-20",
        "&next_from=2026-09-01&next_to=2026-10-03",
    ];
    let lasts = [
        "",
        "&last_from=2026-09-01",
        "&last_to=2026-09-30",
        "&last_from=2026-09-01&last_to=2026-09-30",
    ];
    let mut combos = 0;
    let mut searched = 0;
    for sort in [
        "default",
        "next_call_asc",
        "next_call_desc",
        "last_call_asc",
        "last_call_desc",
    ] {
        for due in ["all", "today"] {
            for owner in ["all", "unassigned", "555"] {
                for stage in ["", "&stage=1095387442", "&stage=1095387445"] {
                    for n in nexts {
                        for l in lasts {
                            let e = env(FakeHs::new()).await;
                            let q = format!("?sort={sort}&due={due}&owner={owner}{stage}{n}{l}");
                            let (s, v) = e.admin_get(&q).await;
                            assert_eq!(s, StatusCode::OK, "{q}: {v}");
                            for b in e.searches() {
                                searched += 1;
                                let gs = groups(&b);
                                assert!(!gs.is_empty() && gs.len() <= 5, "{q}");
                                let total: usize = gs
                                    .iter()
                                    .map(|g| g["filters"].as_array().unwrap().len())
                                    .sum();
                                assert!(total <= 18, "全体 {total}: {q}");
                                for g in &gs {
                                    let n = g["filters"].as_array().unwrap().len();
                                    assert!(n <= 6, "グループあたり {n}: {q} {g}");
                                }
                            }
                            combos += 1;
                        }
                    }
                }
            }
        }
    }
    assert_eq!(combos, 5 * 2 * 3 * 3 * 6 * 4);
    assert!(searched > combos / 2, "検索が行われている ({searched})");
}

#[tokio::test(flavor = "multi_thread")]
async fn 範囲で_search_から停止系を外せないときも後段で外し_件数を出す() {
    let mut f = FakeHs::new();
    with_relations(&mut f, &["1", "2"]);
    let d1 = Deal::new("1", UNPROCESSED)
        .p("bpo_3", "クレーム")
        .p("bpo_13", "2026-10-10");
    let d2 = Deal::new("2", UNPROCESSED).p("bpo_13", "2026-10-10");
    let e = env(f.page(Page::new(vec![d1, d2]))).await;
    let (s, v) = e
        .admin_get("?owner=555&next_from=2026-10-10&next_to=2026-10-20&last_from=2026-09-01&last_to=2026-09-30")
        .await;
    assert_eq!(s, StatusCode::OK);
    // 範囲 4 件 + owner + dealstage で 6。Search 側の停止条件は入れられない
    for b in e.searches() {
        for g in groups(&b) {
            assert!(flt(&g, "bpo_3").is_none());
        }
    }
    assert_eq!(ids(&v), ["2"]);
    assert_eq!(v["partial"]["excluded"]["stop_reason"], 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn 範囲が違う_cursor_は使えない() {
    let mk = || {
        FakeHs::new().page(
            Page::new(vec![Deal::new("1", FUZAI).p("bpo_13", "2026-10-01")])
                .total(80)
                .next("25"),
        )
    };
    let e = env(mk()).await;
    let (_, p1) = e.admin_get("?next_from=2026-10-01").await;
    let c = p1["next_cursor"].as_str().unwrap().to_string();
    let n = e.calls().len();
    for q in [
        "?next_from=2026-10-02",
        "?next_from=2026-10-01&next_to=2026-10-30",
        "?last_from=2026-09-01",
        "?sort=next_call_asc&next_from=2026-10-01",
        "?",
    ] {
        let (s, v) = e.admin_get(&format!("{q}&cursor={c}")).await;
        assert_eq!(
            (s, kind(&v)),
            (StatusCode::BAD_REQUEST, Some("cursor_mismatch")),
            "{q}"
        );
    }
    assert_eq!(e.calls().len(), n, "HubSpot を呼んだ");
    // 同じ条件なら進める
    let (s, _) = e
        .admin_get(&format!("?next_from=2026-10-01&cursor={c}"))
        .await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者以外は範囲を指定しても既定は自分の_owner_() {
    let e = env(FakeHs::new()).await;
    let (s, _) = e
        .bpo_get("?next_from=2026-10-01&next_to=2026-10-31&last_from=2026-09-01&last_to=2026-09-30")
        .await;
    assert_eq!(s, StatusCode::OK);
    for b in e.searches() {
        for g in groups(&b) {
            assert_eq!(flt(&g, "hubspot_owner_id").unwrap()["value"], BPO_OWNER_ID);
            assert!(g["filters"].as_array().unwrap().len() <= 6);
        }
    }
    // 他人を指定すれば範囲があっても見られる (その人の owner で絞る)
    let before = e.searches().len();
    let (s, _) = e.bpo_get("?owner=555&next_from=2026-10-01").await;
    assert_eq!(s, StatusCode::OK);
    for b in &e.searches()[before..] {
        for g in groups(b) {
            assert_eq!(flt(&g, "hubspot_owner_id").unwrap()["value"], "555");
        }
    }
}

// ---------------------------------------------------------------------------
// GET /api/crm/owners (管理者のみの担当者一覧)
// ---------------------------------------------------------------------------

fn owner(
    id: &str,
    first: Option<&str>,
    last: Option<&str>,
    email: Option<&str>,
    archived: bool,
) -> Value {
    let mut o = json!({"id": id, "archived": archived});
    if let Some(f) = first {
        o["firstName"] = json!(f);
    }
    if let Some(l) = last {
        o["lastName"] = json!(l);
    }
    if let Some(e) = email {
        o["email"] = json!(e);
    }
    o
}

fn owners_page(results: Vec<Value>, next: Option<&str>) -> Value {
    let mut v = json!({"results": results});
    if let Some(a) = next {
        v["paging"] = json!({"next": {"after": a}});
    }
    v
}

fn put_owner_pages(f: &mut FakeHs, archived: bool, after: &str, body: Value) {
    f.owner_list.insert((archived, after.to_string()), body);
}

async fn env_owner_ttl(fake: FakeHs, ttl: Duration) -> Env {
    let (client, hs) = start_hs(fake).await;
    let state = test_state(Some(client));
    let app = Router::new()
        .merge(super::routes::router_with_queue(
            CrmAccess::from_list(&format!("{ADMIN},{BPO}"))
                .with_test_role(ADMIN, CrmRole::Admin)
                .with_test_role(BPO, CrmRole::Bpo),
            CallQueueState::for_test(KEY, now_default()).with_owner_list_ttl(ttl),
        ))
        .route("/__test/session", post(inject_session))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()));
    let admin = login(&app, ADMIN, "google_oidc").await;
    let bpo = login(&app, BPO, "google_oidc").await;
    Env {
        app,
        hs,
        admin,
        bpo,
    }
}

impl Env {
    async fn owners(&self) -> (StatusCode, Value) {
        let (s, cc, v) = get_raw(&self.app, "/api/crm/owners", Some(&self.admin)).await;
        assert_eq!(cc, "no-store");
        (s, v)
    }
    fn owner_calls(&self) -> usize {
        self.count("GET /crm/v3/owners")
    }
}

fn names(v: &Value) -> Vec<(String, String, bool)> {
    v["owners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["id"].as_str().unwrap().to_string(),
                o["name"].as_str().unwrap().to_string(),
                o["archived"].as_bool().unwrap(),
            )
        })
        .collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_は未ログイン_パスワードログイン_社外で_hubspot_を呼ばず_管理者以外の_google_ログインには返す(
) {
    let e = env(FakeHs::new()).await;
    let (s, _, v) = get_raw(&e.app, "/api/crm/owners", None).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::UNAUTHORIZED, Some("login_required"))
    );
    let c = login(&e.app, ADMIN, "password_internal").await;
    let (s, _, v) = get_raw(&e.app, "/api/crm/owners", Some(&c)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::FORBIDDEN, Some("google_login_required"))
    );
    let c = login(&e.app, OUTSIDER, "google_oidc").await;
    let (s, _, v) = get_raw(&e.app, "/api/crm/owners", Some(&c)).await;
    assert_eq!((s, kind(&v)), (StatusCode::FORBIDDEN, Some("forbidden")));
    assert_eq!(e.owner_calls(), 0, "HubSpot を呼んだ: {:?}", e.calls());
    assert!(e.calls().is_empty());
    // 管理者ではない Google ログインの人 (bpo) にも返す
    let (s, _, v) = get_raw(&e.app, "/api/crm/owners", Some(&e.bpo)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert!(v["owners"].is_array());
    assert_eq!(e.owner_calls(), 2, "有効 + 退職者");
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_は_hubspot_未設定なら管理者も管理者以外も_503() {
    let app = make_app(test_state(None), now_default());
    let admin = login(&app, ADMIN, "google_oidc").await;
    let bpo = login(&app, BPO, "google_oidc").await;
    let (s, _, v) = get_raw(&app, "/api/crm/owners", Some(&admin)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::SERVICE_UNAVAILABLE, Some("not_configured"))
    );
    let (s, _, v) = get_raw(&app, "/api/crm/owners", Some(&bpo)).await;
    assert_eq!(
        (s, kind(&v)),
        (StatusCode::SERVICE_UNAVAILABLE, Some("not_configured"))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_が_0_人でもエラーにしない() {
    let e = env(FakeHs::new()).await;
    let (s, v) = e.owners().await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["owners"], json!([]));
    assert_eq!(v["truncated"], json!(false));
    // 有効と退職者の 2 回だけ
    assert_eq!(e.owner_calls(), 2);
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_はページを跨いで退職者も集め_有効な人が先_名前順() {
    let mut f = FakeHs::new();
    put_owner_pages(
        &mut f,
        false,
        "",
        owners_page(
            vec![
                owner(
                    "30",
                    Some("ハナ"),
                    Some("テスト"),
                    Some("hana@example.test"),
                    false,
                ),
                owner(
                    "10",
                    Some("アキ"),
                    Some("サンプル"),
                    Some("aki@example.test"),
                    false,
                ),
            ],
            Some("cursor-2"),
        ),
    );
    put_owner_pages(
        &mut f,
        false,
        "cursor-2",
        owners_page(
            vec![owner("20", Some("イチロ"), Some("ダミー"), None, false)],
            None,
        ),
    );
    put_owner_pages(
        &mut f,
        true,
        "",
        owners_page(
            vec![owner(
                "99",
                Some("カツ"),
                Some("退職"),
                Some("old@example.test"),
                true,
            )],
            None,
        ),
    );
    let e = env(f).await;
    let (s, v) = e.owners().await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        names(&v),
        vec![
            ("10".into(), "アキ サンプル".into(), false),
            ("20".into(), "イチロ ダミー".into(), false),
            ("30".into(), "ハナ テスト".into(), false),
            ("99".into(), "カツ 退職".into(), true),
        ]
    );
    assert_eq!(v["owners"][0]["email"], json!("aki@example.test"));
    assert_eq!(v["owners"][1]["email"], Value::Null);
    // ページを追った: 有効 2 ページ + 退職者 1 ページ。2 ページ目は after 付き
    let log: Vec<String> =
        e.hs.lock()
            .unwrap()
            .log
            .iter()
            .filter(|(m, _)| m == "GET /crm/v3/owners")
            .map(|(_, q)| q.clone())
            .collect();
    assert_eq!(log.len(), 3, "{log:?}");
    assert!(log[0].contains("archived=false") && !log[0].contains("after="));
    assert!(log[1].contains("archived=false") && log[1].contains("after=cursor-2"));
    assert!(log[2].contains("archived=true"));
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_の名前は姓名の欠損と同名を扱う() {
    let mut f = FakeHs::new();
    put_owner_pages(
        &mut f,
        false,
        "",
        owners_page(
            vec![
                owner("1", Some("名のみ"), None, Some("a@example.test"), false),
                owner("2", None, Some("姓のみ"), Some("b@example.test"), false),
                owner("3", None, None, Some("mail.local@example.test"), false),
                owner("4", Some("  "), Some(""), None, false),
                // 同名の 2 人は両方残り、email で見分けられる
                owner(
                    "5",
                    Some("同名"),
                    Some("太郎"),
                    Some("same1@example.test"),
                    false,
                ),
                owner(
                    "6",
                    Some("同名"),
                    Some("太郎"),
                    Some("same2@example.test"),
                    false,
                ),
                // ID の無い行は捨てる
                json!({"firstName": "ID無し"}),
            ],
            None,
        ),
    );
    let e = env(f).await;
    let (_, v) = e.owners().await;
    let got = names(&v);
    let name_of = |id: &str| got.iter().find(|(i, _, _)| i == id).unwrap().1.clone();
    assert_eq!(name_of("1"), "名のみ");
    assert_eq!(name_of("2"), "姓のみ");
    assert_eq!(name_of("3"), "mail.local");
    assert_eq!(name_of("4"), "(名前なし)");
    assert_eq!(name_of("5"), "同名 太郎");
    assert_eq!(name_of("6"), "同名 太郎");
    assert_eq!(got.len(), 6);
    assert_eq!(
        v["owners"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|o| o["name"] == "同名 太郎")
            .map(|o| o["email"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["same1@example.test", "same2@example.test"]
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_の_hubspot_失敗は_error_kind_で返しキャッシュしない() {
    for (code, want) in [
        (429u16, "hubspot_rate_limited"),
        (401, "hubspot_auth"),
        (500, "hubspot_upstream"),
    ] {
        let mut f = FakeHs::new();
        f.owners_fail = Some(code);
        let e = env(f).await;
        let (s, v) = e.owners().await;
        assert_ne!(s, StatusCode::OK, "code={code} {v}");
        assert_eq!(kind(&v), Some(want), "code={code}");
        // 失敗は覚えない: 直ったらすぐ取れる
        e.hs.lock().unwrap().owners_fail = None;
        let (s, _) = e.owners().await;
        assert_eq!(s, StatusCode::OK, "code={code}");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_の_hubspot_タイムアウト() {
    let mut f = FakeHs::new();
    f.owner_list_delay = Duration::from_millis(1500);
    let e = env(f).await;
    let (s, v) = e.owners().await;
    assert_eq!(kind(&v), Some("hubspot_timeout"), "{s} {v}");
    assert_ne!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_は有効期間内は再取得せず期限が切れれば取り直す() {
    let mut f = FakeHs::new();
    put_owner_pages(
        &mut f,
        false,
        "",
        owners_page(vec![owner("1", Some("甲"), Some("乙"), None, false)], None),
    );
    let e = env(f).await;
    for _ in 0..3 {
        let (s, v) = e.owners().await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(names(&v).len(), 1);
    }
    assert_eq!(
        e.owner_calls(),
        2,
        "3 回呼んでも HubSpot は 2 回 (有効 + 退職者)"
    );

    // 有効期間 0 なら毎回取り直す (逆証明)
    let e0 = env_owner_ttl(FakeHs::new(), Duration::ZERO).await;
    e0.owners().await;
    e0.owners().await;
    assert_eq!(e0.owner_calls(), 4);
}

#[tokio::test(flavor = "multi_thread")]
async fn owners_は同時に冷えた要求を_1_回の取得にまとめる() {
    let mut f = FakeHs::new();
    f.owner_list_delay = Duration::from_millis(150);
    let e = env(f).await;
    let (a, b, c) = tokio::join!(e.owners(), e.owners(), e.owners());
    assert_eq!(a.0, StatusCode::OK);
    assert_eq!(b.0, StatusCode::OK);
    assert_eq!(c.0, StatusCode::OK);
    assert_eq!(e.owner_calls(), 2);
}
