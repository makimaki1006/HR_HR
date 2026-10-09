//! 書き込み (`write.rs`) の結合テスト。偽 HubSpot (記録つき) と、SQLite で裏打ちした偽の監査 Turso を使う。
//! 本物の HubSpot・本物の Turso には触れない。データはすべて架空。

use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::{
    body::Body,
    extract::{Path, RawQuery, State},
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::call_queue::CallQueueState;
use super::rbac::{CrmAccess, CrmRole};
use super::workspace_cache::WorkspaceCache;
use super::write::{self, WriteConfig};
use crate::audit::fake_turso::{start_sqlite_audit, SharedConn};
use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

type Shared<T> = Arc<Mutex<T>>;

const OPERATOR: &str = "operator@example.com";
const OTHER: &str = "other@example.com";
const ADMIN: &str = "admin@example.com";
const DEAL: &str = "7001";
const CONTACT: &str = "7101";
const STRANGER: &str = "7999";
const PIPELINE: &str = "753186575";
const UNPROCESSED: &str = "1095387442";
const FUZUU: &str = "1095387443";

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Fake {
    objects: HashMap<(String, String), BTreeMap<String, String>>,
    /// 次の PATCH から順に返す失敗 status (空なら成功)
    patch_fail: Vec<u16>,
    /// (method + path, body or query)
    log: Vec<(String, String)>,
    /// deal id → 担当者 id
    contacts: HashMap<String, Vec<String>>,
}

impl Fake {
    fn count(&self, needle: &str) -> usize {
        self.log.iter().filter(|(c, _)| c.contains(needle)).count()
    }
    fn patches(&self) -> Vec<Value> {
        self.log
            .iter()
            .filter(|(c, _)| c.starts_with("PATCH"))
            .map(|(_, b)| serde_json::from_str(b).unwrap())
            .collect()
    }
    fn value(&self, o: &str, id: &str, k: &str) -> Option<String> {
        self.objects
            .get(&(o.to_string(), id.to_string()))
            .and_then(|m| m.get(k).cloned())
    }
}

fn record(o: &str, id: &str, st: &Fake, wanted: &[String]) -> Option<Value> {
    let props = st.objects.get(&(o.to_string(), id.to_string()))?;
    let mut m = serde_json::Map::new();
    for (k, v) in props {
        if wanted.is_empty() || wanted.contains(k) {
            m.insert(k.clone(), Value::String(v.clone()));
        }
    }
    Some(
        json!({"id": id, "properties": m, "createdAt": "2026-01-01T00:00:00Z",
        "updatedAt": "2026-01-01T00:00:00Z", "archived": false}),
    )
}

async fn hs_get(
    State(st): State<Shared<Fake>>,
    Path((o, id)): Path<(String, String)>,
    RawQuery(q): RawQuery,
) -> Response {
    let q = q.unwrap_or_default();
    let mut s = st.lock().unwrap();
    s.log
        .push((format!("GET /crm/v3/objects/{o}/{id}"), q.clone()));
    let wanted: Vec<String> = reqwest::Url::parse(&format!("http://x/?{q}"))
        .unwrap()
        .query_pairs()
        .filter(|(k, _)| k == "properties")
        .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
        .collect();
    match record(&o, &id, &s, &wanted) {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn hs_patch(
    State(st): State<Shared<Fake>>,
    Path((o, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log
        .push((format!("PATCH /crm/v3/objects/{o}/{id}"), body.to_string()));
    if !s.patch_fail.is_empty() {
        let code = s.patch_fail.remove(0);
        return (StatusCode::from_u16(code).unwrap(), "UPSTREAM-SECRET").into_response();
    }
    let Some(map) = s.objects.get_mut(&(o.clone(), id.clone())) else {
        return StatusCode::NOT_FOUND.into_response();
    };
    for (k, v) in body["properties"].as_object().unwrap() {
        map.insert(k.clone(), v.as_str().unwrap().to_string());
    }
    let v = record(&o, &id, &s, &[]).unwrap();
    Json(v).into_response()
}

async fn hs_props(State(st): State<Shared<Fake>>, Path(o): Path<String>) -> Response {
    st.lock()
        .unwrap()
        .log
        .push((format!("GET /crm/v3/properties/{o}"), String::new()));
    let ro = json!({"readOnlyValue": true, "readOnlyDefinition": true, "archivable": false});
    let results = match o.as_str() {
        "deals" => json!([
            {"name": "bpo_10", "label": "不通時チェック", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "options": [{"label": "受付拒否", "value": "reception_refused"}, {"label": "その他", "value": "other"}]},
            {"name": "bpo_13", "label": "次回架電日", "type": "date", "fieldType": "date", "groupName": "g"},
            {"name": "bpo_50", "label": "架電メモ", "type": "string", "fieldType": "textarea", "groupName": "g"},
            {"name": "amount", "label": "金額", "type": "number", "fieldType": "number", "groupName": "g"},
            {"name": "bpo_hsurl", "label": "BPO_HSURL", "type": "string", "fieldType": "text", "groupName": "g"},
            {"name": "bpo_32", "label": "URL_求人検索 ※編集不可", "type": "string", "fieldType": "text", "groupName": "g"},
            {"name": "hs_lastmodifieddate", "label": "最終更新", "type": "datetime", "fieldType": "date", "groupName": "g"},
            {"name": "ro_prop", "label": "読み取り専用", "type": "string", "fieldType": "text", "groupName": "g", "modificationMetadata": ro},
            {"name": "calc_prop", "label": "計算", "type": "number", "fieldType": "number", "groupName": "g", "calculated": true},
            {"name": "dealstage", "label": "ステージ", "type": "enumeration", "fieldType": "select", "groupName": "g",
             "options": [{"label": "x", "value": "x"}]}
        ]),
        "contacts" => json!([
            {"name": "jobtitle", "label": "役職", "type": "string", "fieldType": "text", "groupName": "g"}
        ]),
        _ => json!([
            {"name": "website", "label": "Website", "type": "string", "fieldType": "text", "groupName": "g"}
        ]),
    };
    Json(json!({"results": results})).into_response()
}

async fn hs_groups(Path(_o): Path<String>) -> Response {
    Json(json!({"results": [{"name": "g", "label": "グループ", "displayOrder": 0}]}))
        .into_response()
}

async fn hs_pipelines() -> Response {
    Json(
        json!({"results": [{"id": PIPELINE, "label": "テスト用", "displayOrder": 0, "stages": [
            {"id": UNPROCESSED, "label": "未済", "displayOrder": 0},
            {"id": FUZUU, "label": "不通", "displayOrder": 1}
        ]}]}),
    )
    .into_response()
}

async fn hs_assoc(
    State(st): State<Shared<Fake>>,
    Path((_from, id, to)): Path<(String, String, String)>,
) -> Response {
    let s = st.lock().unwrap();
    let ids = if to == "contacts" {
        s.contacts.get(&id).cloned().unwrap_or_default()
    } else {
        vec![]
    };
    Json(json!({"results": ids.iter().map(|i| json!({"toObjectId": i, "associationTypes": []})).collect::<Vec<_>>()}))
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

async fn start_hs(fake: Fake) -> (Arc<HubSpotClient>, Shared<Fake>) {
    let st = Arc::new(Mutex::new(fake));
    let base = spawn(
        Router::new()
            .route("/crm/v3/objects/{o}/{id}", get(hs_get).patch(hs_patch))
            .route("/crm/v3/properties/{o}", get(hs_props))
            .route("/crm/v3/properties/{o}/groups", get(hs_groups))
            .route("/crm/v3/pipelines/deals", get(hs_pipelines))
            .route(
                "/crm/v4/objects/{from}/{id}/associations/{to}",
                get(hs_assoc),
            )
            .with_state(st.clone()),
    )
    .await;
    let client = HubSpotClient::new(
        "test-token".into(),
        &base,
        ClientOptions {
            timeout: Duration::from_secs(3),
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
        allowed_domains: vec!["example.com".to_string()],
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

fn test_state(hubspot: Arc<HubSpotClient>, audit: Option<AuditDb>) -> Arc<AppState> {
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
        hubspot: Some(hubspot),
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
    hs: Shared<Fake>,
    client: Arc<HubSpotClient>,
    audit: AuditDb,
    conn: SharedConn,
    op: String,
    other: String,
}

fn base_fake() -> Fake {
    let mut f = Fake::default();
    f.objects.insert(
        ("deals".into(), DEAL.into()),
        BTreeMap::from([
            ("dealname".to_string(), "架空案件".to_string()),
            ("dealstage".to_string(), UNPROCESSED.to_string()),
            ("pipeline".to_string(), PIPELINE.to_string()),
            ("bpo_50".to_string(), "旧メモ".to_string()),
            ("amount".to_string(), "1000".to_string()),
        ]),
    );
    f.objects.insert(
        ("contacts".into(), CONTACT.into()),
        BTreeMap::from([("jobtitle".to_string(), "旧役職".to_string())]),
    );
    f.objects.insert(
        ("contacts".into(), STRANGER.into()),
        BTreeMap::from([("jobtitle".to_string(), "無関係".to_string())]),
    );
    f.contacts.insert(DEAL.into(), vec![CONTACT.into()]);
    f
}

async fn env_with(fake: Fake, write: WriteConfig, with_audit: bool) -> Env {
    let (client, hs) = start_hs(fake).await;
    let (audit, conn) = start_sqlite_audit().await;
    let state = test_state(client.clone(), with_audit.then(|| audit.clone()));
    let access = CrmAccess::from_list("")
        .with_test_role(OPERATOR, CrmRole::Bpo)
        .with_test_role(OTHER, CrmRole::Bpo)
        .with_test_role(ADMIN, CrmRole::Admin);
    let app = Router::new()
        .merge(super::routes::router_with_write(
            access,
            CallQueueState::for_test([3u8; 32], chrono::Utc::now()),
            WorkspaceCache::default(),
            write,
        ))
        .route("/__test/session", post(inject_session))
        .route("/__admin/ops", get(write::api_admin_list))
        .route("/__admin/ops/{id}/retry", post(write::api_admin_retry))
        .route("/__admin/ops/{id}/discard", post(write::api_admin_discard))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()));
    let op = login(&app, OPERATOR, "google_oidc").await;
    let other = login(&app, OTHER, "google_oidc").await;
    Env {
        app,
        hs,
        client,
        audit,
        conn,
        op,
        other,
    }
}

fn open() -> WriteConfig {
    WriteConfig {
        enabled: true,
        ..WriteConfig::default()
    }
}

async fn env() -> Env {
    env_with(base_fake(), open(), true).await
}

async fn call(
    app: &Router,
    method: Method,
    uri: &str,
    cookie: Option<&str>,
    body: Option<Value>,
    csrf: bool,
) -> (StatusCode, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    if csrf {
        b = b.header("x-requested-with", "fetch");
    }
    let req = if let Some(v) = body {
        b.header(header::CONTENT_TYPE, "application/json")
            .body(Body::from(v.to_string()))
            .unwrap()
    } else {
        b.body(Body::empty()).unwrap()
    };
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    let text = String::from_utf8(bytes.to_vec()).unwrap();
    assert!(!text.contains("test-token"), "token leaked");
    assert!(!text.contains("UPSTREAM-SECRET"), "upstream body leaked");
    (status, serde_json::from_str(&text).unwrap_or(Value::Null))
}

impl Env {
    async fn patch(&self, body: Value) -> (StatusCode, Value) {
        call(
            &self.app,
            Method::PATCH,
            &format!("/api/crm/deals/{DEAL}"),
            Some(&self.op),
            Some(body),
            true,
        )
        .await
    }
    fn calls(&self, needle: &str) -> usize {
        self.hs.lock().unwrap().count(needle)
    }
    fn ledger(&self, op: &str) -> (String, i64, String) {
        self.conn
            .lock()
            .unwrap()
            .query_row(
                "SELECT status, attempts, COALESCE(last_error_code,'') FROM crm_pending_operations WHERE operation_id = ?1",
                [op],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .unwrap()
    }
    fn ledger_count(&self) -> i64 {
        self.conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_pending_operations", [], |r| {
                r.get(0)
            })
            .unwrap()
    }
    fn audit_metas(&self) -> Vec<Value> {
        let conn = self.conn.lock().unwrap();
        let mut st = conn
            .prepare("SELECT meta, target_id FROM activity_logs WHERE event_type = 'crm_write' ORDER BY at, rowid")
            .unwrap();
        st.query_map([], |r| {
            let m: String = r.get(0)?;
            let t: String = r.get(1)?;
            let mut v: Value = serde_json::from_str(&m).unwrap();
            v["target_id"] = json!(t);
            Ok(v)
        })
        .unwrap()
        .map(Result::unwrap)
        .collect()
    }
    fn make_due(&self, op: &str) {
        self.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE crm_pending_operations SET next_retry_at = '2000-01-01T00:00:00Z' WHERE operation_id = ?1",
                [op],
            )
            .unwrap();
    }
}

fn memo_patch(op: &str, base: &str, new: &str) -> Value {
    json!({"operation_id": op, "base": {"bpo_50": base}, "set": {"bpo_50": new}})
}

// ---------------------------------------------------------------------------
// テスト
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn 栓が閉じていれば_403_で_hubspot_も台帳も触らない() {
    let e = env_with(base_fake(), WriteConfig::default(), true).await;
    let (s, v) = e.patch(memo_patch("op-closed-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(v["error"], "writes_disabled");
    assert_eq!(
        e.hs.lock().unwrap().log.len(),
        0,
        "HubSpot を 1 回も呼ばない"
    );
    assert_eq!(e.ledger_count(), 0);
    // 許可リストに入れた案件だけ開く
    let mut cfg = WriteConfig::default();
    cfg.allowlist.insert(DEAL.to_string());
    let e2 = env_with(base_fake(), cfg, true).await;
    let (s, _) = e2.patch(memo_patch("op-allow-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread")]
async fn crm_の利用者でない人は拒否され_hubspot_を呼ばない() {
    let e = env().await;
    let uri = format!("/api/crm/deals/{DEAL}");
    let b = memo_patch("op-auth-00001", "旧メモ", "新");
    let (s, _) = call(&e.app, Method::PATCH, &uri, None, Some(b.clone()), true).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    let pw = login(&e.app, OPERATOR, "password_internal").await;
    let (s, _) = call(
        &e.app,
        Method::PATCH,
        &uri,
        Some(&pw),
        Some(b.clone()),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // X-Requested-With が無い書き込みは CSRF で 403
    let (s, _) = call(&e.app, Method::PATCH, &uri, Some(&e.op), Some(b), false).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(e.hs.lock().unwrap().log.len(), 0);
    assert_eq!(e.ledger_count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 許可リストに無い項目は_422_で_書き込まない() {
    let e = env().await;
    for bad in [
        "bpo_hsurl",
        "bpo_32",
        "hs_lastmodifieddate",
        "ro_prop",
        "calc_prop",
        "dealstage",
        "not_in_catalog",
    ] {
        let (s, v) = e
            .patch(json!({"operation_id": format!("op-bad-{bad}-0001"),
                "base": {bad: null}, "set": {bad: "x"}}))
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{bad}: {v}");
        assert_eq!(v["status"], "invalid");
        assert!(v["errors"][bad].is_string(), "{bad}: {v}");
    }
    assert_eq!(e.calls("PATCH"), 0);
    assert_eq!(e.ledger_count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 選択肢にない値_日付と数値の形は_422() {
    let e = env().await;
    for (prop, val) in [
        ("bpo_10", "no_such_option"),
        ("bpo_13", "2026/10/12"),
        ("amount", "abc"),
    ] {
        let (s, v) = e
            .patch(json!({"operation_id": format!("op-val-{prop}-0001"),
                "base": {prop: null}, "set": {prop: val}}))
            .await;
        assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{prop}: {v}");
        assert!(v["errors"][prop].is_string(), "{prop}: {v}");
    }
    // base が無い項目も 422 (上書きしないために必須)
    let (s, v) = e
        .patch(json!({"operation_id": "op-nobase-0001", "base": {}, "set": {"bpo_50": "x"}}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(v["errors"]["bpo_50"].is_string());
    assert_eq!(e.calls("PATCH"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 不通へ移すには_bpo_10_が必須で_あれば_dealstage_と一緒に_patch_される() {
    let e = env().await;
    let stage = json!({"pipeline_id": PIPELINE, "stage_id": FUZUU});
    let (s, v) = e
        .patch(json!({"operation_id": "op-stage-0001", "base": {}, "set": {}, "stage": stage}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["missing_required"], json!(["bpo_10"]));
    assert_eq!(e.calls("PATCH"), 0, "必須が足りなければ書かない");
    assert_eq!(e.ledger_count(), 0);

    let (s, v) = e
        .patch(
            json!({"operation_id": "op-stage-0002", "base": {"bpo_10": null},
            "set": {"bpo_10": "reception_refused"}, "stage": stage}),
        )
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "saved");
    assert_eq!(v["values"]["dealstage"], FUZUU);
    assert_eq!(v["values"]["bpo_10"], "reception_refused");
    let patches = e.hs.lock().unwrap().patches();
    assert_eq!(patches.len(), 1, "オブジェクトごとに 1 回");
    let props = &patches[0]["properties"];
    assert_eq!(props["dealstage"], FUZUU);
    assert_eq!(props["pipeline"], PIPELINE);
    assert_eq!(props["bpo_10"], "reception_refused");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "dealstage")
            .as_deref(),
        Some(FUZUU)
    );
    assert_eq!(e.ledger("op-stage-0002").0, "saved");
    // 許可されていないステージ (表に無い) は 422
    let (s, _) = e
        .patch(
            json!({"operation_id": "op-stage-0003", "base": {}, "set": {},
            "stage": {"pipeline_id": PIPELINE, "stage_id": "424242"}}),
        )
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
}

#[tokio::test(flavor = "multi_thread")]
async fn hubspot_の値が_base_と違えば_409_で上書きしない() {
    let e = env().await;
    // 利用者は「別のメモ」を見ていたが、HubSpot は「旧メモ」
    let (s, v) = e
        .patch(memo_patch("op-conf-00001", "別のメモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::CONFLICT, "{v}");
    assert_eq!(v["status"], "conflict");
    assert_eq!(v["current"]["bpo_50"], "旧メモ");
    assert_eq!(v["changed_by_hubspot"], json!(["bpo_50"]));
    assert_eq!(e.calls("PATCH"), 0);
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("旧メモ")
    );
    // 空と null は同じ扱い (base が空文字でも HubSpot が未設定なら競合しない)
    let (s, _) = e
        .patch(json!({"operation_id": "op-conf-00002", "base": {"bpo_13": ""}, "set": {"bpo_13": "2026-10-12"}}))
        .await;
    assert_eq!(s, StatusCode::OK);
    // 監査: 競合も記録される
    let metas = e.audit_metas();
    assert!(
        metas.iter().any(|m| m["outcome"] == "conflict"),
        "{metas:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 同じ_operation_id_は_1_回だけ書く() {
    let e = env().await;
    let body = memo_patch("op-idem-00001", "旧メモ", "新メモ");
    let (s1, v1) = e.patch(body.clone()).await;
    let (s2, v2) = e.patch(body).await;
    assert_eq!((s1, s2), (StatusCode::OK, StatusCode::OK));
    assert_eq!(v1["values"], v2["values"]);
    assert_eq!(v2["values"]["bpo_50"], "新メモ");
    assert_eq!(e.calls("PATCH"), 1, "HubSpot への PATCH は 1 回");
    // 別の案件に同じ operation_id を使うと 422
    let (s, v) = call(
        &e.app,
        Method::PATCH,
        "/api/crm/deals/7002",
        Some(&e.op),
        Some(memo_patch("op-idem-00001", "旧メモ", "x")),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    // 他人の operation_id の結果は見せない
    let (s, _) = call(
        &e.app,
        Method::PATCH,
        &format!("/api/crm/deals/{DEAL}"),
        Some(&e.other),
        Some(memo_patch("op-idem-00001", "旧メモ", "新メモ")),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn 保存の監査には_操作者_案件_項目_前後_結果が残る() {
    let e = env().await;
    let (s, _) = e
        .patch(memo_patch("op-audit-0001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::OK);
    let metas = e.audit_metas();
    assert_eq!(metas.len(), 1, "{metas:?}");
    let m = &metas[0];
    assert_eq!(m["target_id"], DEAL);
    assert_eq!(m["operator"], OPERATOR);
    assert_eq!(m["operation_id"], "op-audit-0001");
    assert_eq!(m["outcome"], "saved");
    assert!(m["at"].as_str().unwrap().contains('T'));
    assert_eq!(
        m["changes"],
        json!([{"object": "deal", "id": DEAL, "prop": "bpo_50", "before": "旧メモ", "after": "新メモ"}])
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 一時障害は_202_で台帳に積み_worker_が再送して保存する() {
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let e = env_with(f, open(), true).await;
    let (s, v) = e
        .patch(memo_patch("op-retry-0001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED, "{v}");
    assert_eq!(v["status"], "queued");
    assert_eq!(v["operation_id"], "op-retry-0001");
    assert_eq!(
        e.ledger("op-retry-0001"),
        ("pending".into(), 1, "hubspot_upstream".into())
    );
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("旧メモ")
    );
    // 状態の照会 (作った本人は見られる / 他人は 403)
    let uri = "/api/crm/operations/op-retry-0001";
    let (s, v) = call(&e.app, Method::GET, uri, Some(&e.op), None, false).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "pending");
    assert_eq!(v["attempts"], 1);
    assert!(v["next_retry_at"].is_string());
    let (s, _) = call(&e.app, Method::GET, uri, Some(&e.other), None, false).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    // 同じ operation_id の再送は 202 のまま (二重に積まない)
    let (s, _) = e
        .patch(memo_patch("op-retry-0001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    assert_eq!(e.ledger_count(), 1);

    // 期限前は何もしない
    write::run_due(&e.audit, &e.client).await;
    assert_eq!(e.ledger("op-retry-0001").0, "pending");
    assert_eq!(e.calls("PATCH"), 1);
    // 期限が来たら再送 → 保存
    e.make_due("op-retry-0001");
    write::run_due(&e.audit, &e.client).await;
    assert_eq!(e.ledger("op-retry-0001").0, "saved");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("新メモ")
    );
    let (_, v) = call(&e.app, Method::GET, uri, Some(&e.op), None, false).await;
    assert_eq!(v["status"], "saved");
    let outcomes: Vec<String> = e
        .audit_metas()
        .iter()
        .map(|m| m["outcome"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(outcomes, vec!["queued", "saved_by_retry"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn 再送で_hubspot_の値が変わっていたら_上書きせず_failed() {
    let mut f = base_fake();
    f.patch_fail = vec![500];
    let e = env_with(f, open(), true).await;
    let (s, _) = e
        .patch(memo_patch("op-rconf-0001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    // 待っている間に別の人が HubSpot で書き換えた
    e.hs.lock()
        .unwrap()
        .objects
        .get_mut(&("deals".to_string(), DEAL.to_string()))
        .unwrap()
        .insert("bpo_50".into(), "他の人のメモ".into());
    e.make_due("op-rconf-0001");
    write::run_due(&e.audit, &e.client).await;
    let (status, attempts, code) = e.ledger("op-rconf-0001");
    assert_eq!((status.as_str(), code.as_str()), ("failed", "conflict"));
    assert_eq!(attempts, 2);
    assert_eq!(e.calls("PATCH"), 1, "再送では書かない");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("他の人のメモ")
    );
    // 管理者の失敗一覧に出る
    let admin = login(&e.app, ADMIN, "google_oidc").await;
    let (s, v) = call(
        &e.app,
        Method::GET,
        "/__admin/ops?status=failed",
        Some(&admin),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let ops = v["operations"].as_array().unwrap();
    assert_eq!(ops.len(), 1);
    assert_eq!(ops[0]["operation_id"], "op-rconf-0001");
    assert_eq!(ops[0]["deal_id"], DEAL);
    assert_eq!(ops[0]["operator_email"], OPERATOR);
    assert_eq!(ops[0]["props"], json!(["bpo_50"]));
    assert_eq!(ops[0]["error"], "conflict");
    assert_eq!(ops[0]["status"], "failed");
}

#[tokio::test(flavor = "multi_thread")]
async fn 再送の途中で応答だけ失われた書き込みは_保存済みとして扱う() {
    // 1 回目: HubSpot には書けたが応答が 5xx だった想定 (値は既に新しい)
    let mut f = base_fake();
    f.patch_fail = vec![502];
    let e = env_with(f, open(), true).await;
    let (s, _) = e
        .patch(memo_patch("op-lost-00001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    e.hs.lock()
        .unwrap()
        .objects
        .get_mut(&("deals".to_string(), DEAL.to_string()))
        .unwrap()
        .insert("bpo_50".into(), "新メモ".into());
    e.make_due("op-lost-00001");
    write::run_due(&e.audit, &e.client).await;
    assert_eq!(e.ledger("op-lost-00001").0, "saved", "競合にしない");
    assert_eq!(e.calls("PATCH"), 1, "同じ値は書き直さない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 恒久エラーは再送せず_failed_で_管理者が再試行と破棄できる() {
    let mut f = base_fake();
    f.patch_fail = vec![400];
    let e = env_with(f, open(), true).await;
    let (s, v) = e
        .patch(memo_patch("op-perm-00001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["status"], "invalid");
    assert_eq!(e.ledger("op-perm-00001").0, "failed");
    // 同じ operation_id は保存された失敗を返す (書き直さない)
    let (s, _) = e
        .patch(memo_patch("op-perm-00001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(e.calls("PATCH"), 1);
    // worker は触らない
    e.make_due("op-perm-00001");
    write::run_due(&e.audit, &e.client).await;
    assert_eq!(e.calls("PATCH"), 1);

    let admin = login(&e.app, ADMIN, "google_oidc").await;
    // 一般ユーザーが管理 API を叩ける経路はこのテストのルーターには無い (lib.rs の require_admin_mw)。
    let (s, _) = call(
        &e.app,
        Method::POST,
        "/__admin/ops/op-perm-00001/retry",
        Some(&admin),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.ledger("op-perm-00001").0, "pending");
    write::run_due(&e.audit, &e.client).await;
    assert_eq!(
        e.ledger("op-perm-00001").0,
        "saved",
        "今度は HubSpot が受け付ける"
    );
    // 保存済みは破棄できない
    let (s, _) = call(
        &e.app,
        Method::POST,
        "/__admin/ops/op-perm-00001/discard",
        Some(&admin),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);
}

#[tokio::test(flavor = "multi_thread")]
async fn 失敗した操作は破棄できる() {
    let mut f = base_fake();
    f.patch_fail = vec![400];
    let e = env_with(f, open(), true).await;
    let _ = e
        .patch(memo_patch("op-disc-00001", "旧メモ", "新メモ"))
        .await;
    let admin = login(&e.app, ADMIN, "google_oidc").await;
    let (s, _) = call(
        &e.app,
        Method::POST,
        "/__admin/ops/op-disc-00001/discard",
        Some(&admin),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(e.ledger("op-disc-00001").0, "discarded");
    let (_, v) = call(
        &e.app,
        Method::GET,
        "/__admin/ops?status=failed",
        Some(&admin),
        None,
        false,
    )
    .await;
    assert_eq!(v["operations"].as_array().unwrap().len(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 再送待ちが上限に達したら_503_queue_full_で何も積まない() {
    let mut f = base_fake();
    f.patch_fail = vec![503, 503];
    let cfg = WriteConfig {
        pending_max: 1,
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let (s, _) = e.patch(memo_patch("op-cap-000001", "旧メモ", "A")).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let (s, v) = e.patch(memo_patch("op-cap-000002", "旧メモ", "B")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_eq!(v["error"], "queue_full");
    assert_eq!(e.ledger("op-cap-000002").0, "failed", "再送されない");
    assert_eq!(e.ledger("op-cap-000002").2, "queue_full");
}

#[tokio::test(flavor = "multi_thread")]
async fn 台帳に記録できないときは書かずに_503() {
    let e = env_with(base_fake(), open(), false).await;
    let (s, v) = e.patch(memo_patch("op-noaudit-001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_eq!(v["error"], "queue_unavailable");
    assert_eq!(e.calls("PATCH"), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 担当者の項目は_案件に紐づく担当者だけ書ける() {
    let e = env().await;
    let ok = json!({"operation_id": "op-contact-001", "base": {}, "set": {},
        "objects": {"contact": {"id": CONTACT, "base": {"jobtitle": "旧役職"}, "set": {"jobtitle": "部長"}}}});
    let (s, v) = e.patch(ok).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["objects_values"]["contact"]["jobtitle"], "部長");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("contacts", CONTACT, "jobtitle")
            .as_deref(),
        Some("部長")
    );
    let bad = json!({"operation_id": "op-contact-002", "base": {}, "set": {},
        "objects": {"contact": {"id": STRANGER, "base": {"jobtitle": "無関係"}, "set": {"jobtitle": "社長"}}}});
    let (s, _) = e.patch(bad).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("contacts", STRANGER, "jobtitle")
            .as_deref(),
        Some("無関係")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 編集スキーマは書ける項目だけを返し_ステージの必須を含む() {
    let mut cfg = WriteConfig::default();
    cfg.allowlist.insert(DEAL.to_string());
    let e = env_with(base_fake(), cfg, true).await;
    let uri = format!("/api/crm/edit-schema?deal_id={DEAL}");
    let (s, v) = call(&e.app, Method::GET, &uri, Some(&e.op), None, false).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["deal_id"], DEAL);
    assert_eq!(v["pipeline_id"], PIPELINE);
    assert_eq!(v["stage_id"], UNPROCESSED);
    assert_eq!(v["writes_enabled"], true);
    let names: Vec<(String, String)> = v["editable"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["object"].as_str().unwrap().to_string(),
                p["name"].as_str().unwrap().to_string(),
            )
        })
        .collect();
    for want in [
        ("deal", "bpo_10"),
        ("deal", "bpo_50"),
        ("deal", "amount"),
        ("contact", "jobtitle"),
        ("company", "website"),
    ] {
        assert!(
            names.contains(&(want.0.to_string(), want.1.to_string())),
            "{want:?} {names:?}"
        );
    }
    for no in [
        "bpo_hsurl",
        "bpo_32",
        "hs_lastmodifieddate",
        "ro_prop",
        "calc_prop",
        "dealstage",
    ] {
        assert!(!names.iter().any(|(_, n)| n == no), "{no} は書けない");
    }
    let bpo10 = v["editable"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["name"] == "bpo_10")
        .unwrap();
    assert_eq!(bpo10["type"], "enumeration");
    assert_eq!(bpo10["options"][0]["value"], "reception_refused");
    let fuzuu = v["stages"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["id"] == FUZUU)
        .unwrap();
    assert_eq!(fuzuu["required"], json!(["bpo_10"]));
    assert_eq!(fuzuu["shown"], json!(["bpo_10", "bpo_31"]));
    assert_eq!(v["pipelines"][0]["id"], PIPELINE);
    assert_eq!(v["pipelines"][0]["stages"][1]["label"], "不通");
    // 栓が閉じていれば writes_enabled=false (スキーマ自体は返す)
    let e2 = env_with(base_fake(), WriteConfig::default(), true).await;
    let (s, v) = call(&e2.app, Method::GET, &uri, Some(&e2.op), None, false).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["writes_enabled"], false);
    // 未ログインは 401
    let (s, _) = call(&e2.app, Method::GET, &uri, None, None, false).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
}
