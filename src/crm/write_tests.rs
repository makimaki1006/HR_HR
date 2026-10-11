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
use super::write::{self, norm, WriteConfig};
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
    /// オブジェクト種別 (`contacts` など) ごとに、その PATCH を常に失敗させる status
    fail_object: HashMap<String, u16>,
    /// (method + path, body or query)
    log: Vec<(String, String)>,
    /// deal id → 担当者 id
    contacts: HashMap<String, Vec<String>>,
    /// 偽 HubSpot の応答遅延 (ミリ秒。読み取りは「読んだ時点の値」を返したあとで遅れる)
    delay_ms: u64,
    /// PATCH を**書いたあとで**応答だけ遅らせる (ミリ秒。「書けたのに応答が間に合わない」を再現する)
    patch_after_delay_ms: u64,
    /// true の間、プロパティ定義の取得を 503 にする
    props_fail: bool,
    /// 次の PATCH から順に、**書いたあとで**この失敗 status を返す (「書けたのに失敗に見える」を再現する)
    patch_apply_fail: Vec<u16>,
    /// 応答を遅らせている最中のリクエスト数と、その最大値 (「同時に進んだか」を時間でなく数で確かめる)
    inflight: usize,
    max_inflight: usize,
    /// PATCH を受け取った数 (遅延より前に数える。「送信が始まった」ことを待つ目印)
    patch_started: usize,
}

/// 偽 HubSpot の応答を `ms` ミリ秒遅らせる (その間は inflight に数える)
async fn hold(st: &Shared<Fake>, ms: u64) {
    if ms == 0 {
        return;
    }
    {
        let mut s = st.lock().unwrap();
        s.inflight += 1;
        s.max_inflight = s.max_inflight.max(s.inflight);
    }
    tokio::time::sleep(Duration::from_millis(ms)).await;
    st.lock().unwrap().inflight -= 1;
}

/// 条件が成り立つまで待つ (固定の sleep で順序を当てにしない)。10 秒で諦めて失敗にする
async fn wait_until(what: &str, mut cond: impl FnMut() -> bool) {
    for _ in 0..1000 {
        if cond() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("待っていた条件が成り立たない: {what}");
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
    let wanted: Vec<String> = reqwest::Url::parse(&format!("http://x/?{q}"))
        .unwrap()
        .query_pairs()
        .filter(|(k, _)| k == "properties")
        .flat_map(|(_, v)| v.split(',').map(str::to_string).collect::<Vec<_>>())
        .collect();
    let (found, delay) = {
        let mut s = st.lock().unwrap();
        s.log
            .push((format!("GET /crm/v3/objects/{o}/{id}"), q.clone()));
        (record(&o, &id, &s, &wanted), s.delay_ms)
    };
    // 読んだ時点の値を持ったまま遅れる (書き込みが割り込むと古い値を返す = 実際の HubSpot と同じ)
    hold(&st, delay).await;
    match found {
        Some(v) => Json(v).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn hs_patch(
    State(st): State<Shared<Fake>>,
    Path((o, id)): Path<(String, String)>,
    Json(body): Json<Value>,
) -> Response {
    let delay = {
        let mut s = st.lock().unwrap();
        s.patch_started += 1;
        s.delay_ms
    };
    hold(&st, delay).await;
    // ロックは await をまたがない (このブロックの中だけ)
    let applied: Result<(Value, u64, Option<u16>), Response> = {
        let mut s = st.lock().unwrap();
        s.log
            .push((format!("PATCH /crm/v3/objects/{o}/{id}"), body.to_string()));
        if let Some(code) = s.fail_object.get(&o).copied() {
            Err((StatusCode::from_u16(code).unwrap(), "UPSTREAM-SECRET").into_response())
        } else if !s.patch_fail.is_empty() {
            let code = s.patch_fail.remove(0);
            Err((StatusCode::from_u16(code).unwrap(), "UPSTREAM-SECRET").into_response())
        } else if let Some(map) = s.objects.get_mut(&(o.clone(), id.clone())) {
            for (k, v) in body["properties"].as_object().unwrap() {
                map.insert(k.clone(), v.as_str().unwrap().to_string());
            }
            let v = record(&o, &id, &s, &[]).unwrap();
            let after = s.patch_after_delay_ms;
            let apply_fail = (!s.patch_apply_fail.is_empty()).then(|| s.patch_apply_fail.remove(0));
            Ok((v, after, apply_fail))
        } else {
            Err(StatusCode::NOT_FOUND.into_response())
        }
    };
    let (v, after, apply_fail) = match applied {
        Ok(x) => x,
        Err(r) => return r,
    };
    hold(&st, after).await;
    if let Some(code) = apply_fail {
        return (StatusCode::from_u16(code).unwrap(), "UPSTREAM-SECRET").into_response();
    }
    Json(v).into_response()
}

async fn hs_props(State(st): State<Shared<Fake>>, Path(o): Path<String>) -> Response {
    let fail = {
        let mut s = st.lock().unwrap();
        s.log
            .push((format!("GET /crm/v3/properties/{o}"), String::new()));
        s.props_fail
    };
    if fail {
        return (StatusCode::SERVICE_UNAVAILABLE, "UPSTREAM-SECRET").into_response();
    }
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
    start_hs_gw(fake, None).await
}

/// 関所を差し替えられる版 (`None` は流量の制限なし)
async fn start_hs_gw(
    fake: Fake,
    gateway: Option<Arc<crate::hubspot::gateway::Gateway>>,
) -> (Arc<HubSpotClient>, Shared<Fake>) {
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
    let opts = ClientOptions {
        timeout: Duration::from_secs(3),
        max_retries: 0,
        retry_base_delay: Duration::from_millis(1),
        search_min_interval: Duration::from_millis(1),
        rate_limited_min_wait: Duration::from_millis(1),
    };
    let client = match gateway {
        Some(gw) => HubSpotClient::with_gateway("test-token".into(), &base, opts, gw).unwrap(),
        None => HubSpotClient::new("test-token".into(), &base, opts).unwrap(),
    };
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
    /// アプリ側と同じ設定 (鍵・状態の表を共有する。再送 worker にも渡す)
    write: WriteConfig,
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
    env_with_gw(fake, write, with_audit, None).await
}

async fn env_with_gw(
    fake: Fake,
    write: WriteConfig,
    with_audit: bool,
    gateway: Option<Arc<crate::hubspot::gateway::Gateway>>,
) -> Env {
    let (client, hs) = start_hs_gw(fake, gateway).await;
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
            write.clone(),
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
        write,
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
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.ledger("op-retry-0001").0, "pending");
    assert_eq!(e.calls("PATCH"), 1);
    // 期限が来たら再送 → 保存
    e.make_due("op-retry-0001");
    write::run_due(&e.audit, &e.client, &e.write).await;
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
    write::run_due(&e.audit, &e.client, &e.write).await;
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
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.ledger("op-lost-00001").0, "saved", "競合にしない");
    assert_eq!(e.calls("PATCH"), 1, "同じ値は書き直さない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 恒久エラーは再送せず_failed_で_管理者が再試行と破棄できる() {
    let mut f = base_fake();
    f.patch_fail = vec![400, 400];
    let e = env_with(f, open(), true).await;
    let (s, v) = e
        .patch(memo_patch("op-perm-00001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["status"], "invalid");
    assert_eq!(e.ledger("op-perm-00001").0, "failed");
    // 同じ operation_id の再送は失敗を返し続けず、受付し直して HubSpot に送る (まだ受け付けられなければ同じ 422)
    let (s, _) = e
        .patch(memo_patch("op-perm-00001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(e.calls("PATCH"), 2);
    assert_eq!(e.ledger("op-perm-00001").0, "failed");
    // worker は触らない
    e.make_due("op-perm-00001");
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.calls("PATCH"), 2);

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
    write::run_due(&e.audit, &e.client, &e.write).await;
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
    // 上限は受付の前に見る: 台帳にも HubSpot にも触らない (Turso への書き込みを増やさない)
    assert_eq!(e.ledger_count(), 1, "2 件目は台帳に入れない");
    assert_eq!(e.calls("PATCH"), 1, "2 件目は HubSpot に送らない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 受付を断った後_詰まりが解ければ同じ操作を新しい_operation_id_で保存できる() {
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let cfg = WriteConfig {
        pending_max: 1,
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let (s, _) = e.patch(memo_patch("op-drain-0001", "旧メモ", "A")).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    let (s, _) = e.patch(memo_patch("op-drain-0002", "旧メモ", "B")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    // worker が詰まりを解く
    e.make_due("op-drain-0001");
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.ledger("op-drain-0001").0, "saved");
    // 断られた側は台帳に残らないので、同じ operation_id でも新しい受付になる
    let (s, v) = e.patch(memo_patch("op-drain-0002", "A", "B")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(e.ledger("op-drain-0002").0, "saved");
}

#[tokio::test(flavor = "multi_thread")]
async fn 失敗した操作の再送は_失敗を返し続けず_受け付け直す() {
    // 1 回目 400 (恒久)、再送は HubSpot が受け付ける
    let mut f = base_fake();
    f.patch_fail = vec![400];
    let e = env_with(f, open(), true).await;
    let (s, _) = e.patch(memo_patch("op-redo-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(e.ledger("op-redo-0001").0, "failed");
    let (s, v) = e.patch(memo_patch("op-redo-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["values"]["bpo_50"], "新");
    assert_eq!(e.ledger("op-redo-0001").0, "saved");
    // 保存済みは二重に書かない
    let (s, _) = e.patch(memo_patch("op-redo-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(
        e.calls("PATCH"),
        2,
        "2 回目の送信 (成功) まで。保存後の再送は書かない"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 破棄された操作の再送は_410_discarded() {
    let mut f = base_fake();
    f.patch_fail = vec![400];
    let e = env_with(f, open(), true).await;
    let _ = e.patch(memo_patch("op-gone-0001", "旧メモ", "新")).await;
    let admin = login(&e.app, ADMIN, "google_oidc").await;
    let (s, _) = call(
        &e.app,
        Method::POST,
        "/__admin/ops/op-gone-0001/discard",
        Some(&admin),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (s, v) = e.patch(memo_patch("op-gone-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::GONE);
    assert_eq!(v["error"], "discarded");
    assert_eq!(e.calls("PATCH"), 1, "破棄された操作は送らない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 他人の_operation_id_は案件が違っても同じ_403_で存在を探れない() {
    let e = env().await;
    let (s, _) = e.patch(memo_patch("op-oracle-001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::OK);
    for deal in [DEAL, "999"] {
        let (s, v) = call(
            &e.app,
            Method::PATCH,
            &format!("/api/crm/deals/{deal}"),
            Some(&e.other),
            Some(memo_patch("op-oracle-001", "旧メモ", "新")),
            true,
        )
        .await;
        assert_eq!(s, StatusCode::FORBIDDEN, "deal={deal} {v}");
        assert_eq!(v["error"], "forbidden");
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 栓を閉じたら_積んである操作も送らず_failed_にする() {
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let e = env_with(f, open(), true).await;
    let (s, _) = e.patch(memo_patch("op-gate-0001", "旧メモ", "新")).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    e.make_due("op-gate-0001");
    // 受付のあとで栓を閉じた (閉じた設定で worker を回す)
    write::run_due(&e.audit, &e.client, &WriteConfig::default()).await;
    assert_eq!(
        e.ledger("op-gate-0001"),
        ("failed".into(), 1, "writes_disabled".into())
    );
    assert_eq!(e.calls("PATCH"), 1, "栓が閉じた後は HubSpot に送らない");
    // 開け直して管理者が再試行すれば送られる
    let admin = login(&e.app, ADMIN, "google_oidc").await;
    let (s, _) = call(
        &e.app,
        Method::POST,
        "/__admin/ops/op-gate-0001/retry",
        Some(&admin),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.ledger("op-gate-0001").0, "saved");
}

#[tokio::test(flavor = "multi_thread")]
async fn 案件は書けて担当者が恒久エラーなら_書けた分を_partial_で返す() {
    let mut f = base_fake();
    f.fail_object.insert("contacts".into(), 400);
    let e = env_with(f, open(), true).await;
    let body = json!({"operation_id": "op-part-0001",
        "base": {"bpo_50": "旧メモ"}, "set": {"bpo_50": "新メモ"},
        "objects": {"contact": {"id": CONTACT, "base": {"jobtitle": "旧役職"}, "set": {"jobtitle": "部長"}}}});
    let (s, v) = e.patch(body).await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY, "{v}");
    assert_eq!(v["status"], "invalid");
    assert_eq!(v["partial"]["values"]["bpo_50"], "新メモ", "{v}");
    assert!(v["partial"]["objects_values"]
        .as_object()
        .unwrap()
        .is_empty());
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("新メモ"),
        "案件は実際に書けている"
    );
    // 何も書けなかった失敗には partial を付けない
    let mut f2 = base_fake();
    f2.patch_fail = vec![400];
    let e2 = env_with(f2, open(), true).await;
    let (_, v2) = e2.patch(memo_patch("op-part-0002", "旧メモ", "新")).await;
    assert!(v2.get("partial").is_none(), "{v2}");
}

#[tokio::test(flavor = "multi_thread")]
async fn 監査には担当者と会社の値を写さない() {
    let e = env().await;
    let ok = json!({"operation_id": "op-redact-001", "base": {"bpo_50": "旧メモ"}, "set": {"bpo_50": "新メモ"},
        "objects": {"contact": {"id": CONTACT, "base": {"jobtitle": "旧役職"}, "set": {"jobtitle": "部長"}}}});
    let (s, v) = e.patch(ok).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    let metas = e.audit_metas();
    let text = serde_json::to_string(&metas).unwrap();
    assert!(!text.contains("部長") && !text.contains("旧役職"), "{text}");
    assert!(text.contains("新メモ"), "案件の値は残す: {text}");
    assert!(
        text.contains("jobtitle"),
        "どの項目が変わったかは残す: {text}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 必須の真偽値は_false_でも入力済み_として扱う() {
    // 画面 (draftToValue) も true / false のどちらかを必ず送る。サーバーも明示した false は空とみなさない (仕様の固定)
    assert_eq!(norm(Some("false"), "bool"), "false");
    assert!(!norm(Some("false"), "bool").is_empty());
    assert!(norm(None, "bool").is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn 書き込みの受付は操作者ごとに速さを制限する() {
    let l = write::WriteRateLimiter::default();
    assert_eq!(write::DEFAULT_WRITE_RATE_PER_MIN, 200);
    for i in 0..200 {
        assert!(l.allow("a@example.com"), "{} 件目は通る", i + 1);
    }
    assert!(!l.allow("a@example.com"), "201 件目は断る");
    assert!(l.allow("b@example.com"), "他の人には影響しない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 書き込みの上限は設定で変えられる() {
    assert_eq!(write::parse_rate_per_min(Some("5")), 5);
    assert_eq!(write::parse_rate_per_min(Some(" 1 ")), 1);
    for bad in [None, Some("0"), Some("-3"), Some("abc"), Some("")] {
        assert_eq!(write::parse_rate_per_min(bad), 200, "{bad:?}");
    }
    let l = write::WriteRateLimiter::new(write::parse_rate_per_min(Some("5")));
    for _ in 0..5 {
        assert!(l.allow("a@example.com"));
    }
    assert!(!l.allow("a@example.com"), "6 件目は断る");
}

#[tokio::test(flavor = "multi_thread")]
async fn 上限を超えた_patch_は_429_rate_limited_で何も書かない() {
    let cfg = WriteConfig {
        rate_per_min: 1,
        ..open()
    };
    let e = env_with(base_fake(), cfg, true).await;
    let (s1, v1) = e.patch(memo_patch("op-rate-001", "旧メモ", "新1")).await;
    assert_eq!(s1, StatusCode::OK, "{v1}");
    let patches = e.calls("PATCH");
    let (s2, v2) = e.patch(memo_patch("op-rate-002", "新1", "新2")).await;
    assert_eq!(s2, StatusCode::TOO_MANY_REQUESTS, "{v2}");
    assert_eq!(v2["error"], "rate_limited");
    assert_eq!(e.calls("PATCH"), patches, "断った分は HubSpot に書かない");
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

// ---------------------------------------------------------------------------
// 同時書き込み (レコード単位の直列化)
// ---------------------------------------------------------------------------

fn concurrent_fake(deals: &[&str]) -> Fake {
    let mut f = base_fake();
    f.delay_ms = 40;
    for d in deals {
        f.objects.insert(
            ("deals".into(), d.to_string()),
            BTreeMap::from([
                ("dealname".to_string(), "架空案件".to_string()),
                ("dealstage".to_string(), UNPROCESSED.to_string()),
                ("pipeline".to_string(), PIPELINE.to_string()),
                ("bpo_50".to_string(), "初期値".to_string()),
            ]),
        );
    }
    f
}

async fn patch_on(e: &Env, deal: &str, body: Value) -> (StatusCode, Value) {
    call(
        &e.app,
        Method::PATCH,
        &format!("/api/crm/deals/{deal}"),
        Some(&e.op),
        Some(body),
        true,
    )
    .await
}

async fn fire(e: &Env, deals: &[&str], n: usize) -> Vec<(String, StatusCode, Value)> {
    let mut hs = Vec::new();
    for d in deals {
        for i in 0..n {
            let (app, cookie, d) = (e.app.clone(), e.op.clone(), d.to_string());
            hs.push(tokio::spawn(async move {
                let body = memo_patch(&format!("op-{d}-{i:04}"), "初期値", &format!("値{d}-{i}"));
                let (s, v) = call(
                    &app,
                    Method::PATCH,
                    &format!("/api/crm/deals/{d}"),
                    Some(&cookie),
                    Some(body),
                    true,
                )
                .await;
                (d, s, v)
            }));
        }
    }
    let mut out = Vec::new();
    for h in hs {
        out.push(h.await.unwrap());
    }
    out
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 同じ案件へ_15_件同時に同じ_base_で書くと_1_件だけ保存され_14_件は_409() {
    let cfg = open();
    let e = env_with(concurrent_fake(&["7201"]), cfg.clone(), true).await;
    let before = e.client.gateway().snapshot().coalesced;
    let res = fire(&e, &["7201"], 15).await;
    let saved = res.iter().filter(|r| r.1 == StatusCode::OK).count();
    let conflict = res.iter().filter(|r| r.1 == StatusCode::CONFLICT).count();
    assert_eq!((saved, conflict), (1, 14), "{res:?}");
    let patches =
        e.hs.lock()
            .unwrap()
            .count("PATCH /crm/v3/objects/deals/7201");
    assert_eq!(patches, 1, "HubSpot へ届いた PATCH は 1 回だけ");
    // 保存された値は、保存に成功した要求の値と一致する (取りこぼしで別の値に上書きされていない)
    let winner = res.iter().find(|r| r.1 == StatusCode::OK).unwrap();
    let won = winner.2["values"]["bpo_50"].as_str().unwrap().to_string();
    assert_eq!(
        e.hs.lock().unwrap().value("deals", "7201", "bpo_50"),
        Some(won)
    );
    // 競合確認の読み取りは相乗りしない
    assert_eq!(e.client.gateway().snapshot().coalesced, before);
    // 鍵の表は空に戻る
    assert!(cfg.locks.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 三つの案件へ_15_件ずつ同時に書くと_案件ごとに_1_件保存_14_件は_409() {
    let deals = ["7211", "7212", "7213"];
    let e = env_with(concurrent_fake(&deals), open(), true).await;
    let res = fire(&e, &deals, 15).await;
    assert_eq!(res.iter().filter(|r| r.1 == StatusCode::OK).count(), 3);
    assert_eq!(
        res.iter().filter(|r| r.1 == StatusCode::CONFLICT).count(),
        42
    );
    for d in deals {
        let ok = res
            .iter()
            .filter(|r| r.0 == d && r.1 == StatusCode::OK)
            .count();
        assert_eq!(ok, 1, "{d}");
        assert_eq!(
            e.hs.lock()
                .unwrap()
                .count(&format!("PATCH /crm/v3/objects/deals/{d}")),
            1
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 順番に前の結果を_base_にして書けば全部_200() {
    let e = env_with(concurrent_fake(&["7221"]), open(), true).await;
    let mut base = "初期値".to_string();
    for i in 0..6 {
        let new = format!("値{i}");
        let (s, v) = patch_on(
            &e,
            "7221",
            memo_patch(&format!("op-chain-{i:04}"), &base, &new),
        )
        .await;
        assert_eq!(s, StatusCode::OK, "{v}");
        base = new;
    }
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .count("PATCH /crm/v3/objects/deals/7221"),
        6
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 案件と担当者を同時に書く要求が重なっても詰まらず_1_件だけ保存される() {
    let cfg = open();
    let mut f = concurrent_fake(&["7231"]);
    f.contacts.insert("7231".into(), vec![CONTACT.into()]);
    let e = env_with(f, cfg.clone(), true).await;
    let mut hs = Vec::new();
    for i in 0..6 {
        let (app, cookie) = (e.app.clone(), e.op.clone());
        hs.push(tokio::spawn(async move {
            let body = json!({
                "operation_id": format!("op-multi-{i:04}"),
                "base": {"bpo_50": "初期値"}, "set": {"bpo_50": format!("値{i}")},
                "objects": {"contact": {"id": CONTACT, "base": {"jobtitle": "旧役職"}, "set": {"jobtitle": format!("役職{i}")}}}
            });
            call(&app, Method::PATCH, "/api/crm/deals/7231", Some(&cookie), Some(body), true).await
        }));
    }
    let mut codes = Vec::new();
    for h in hs {
        let r = tokio::time::timeout(Duration::from_secs(15), h)
            .await
            .expect("詰まった")
            .unwrap();
        codes.push(r.0);
    }
    assert_eq!(
        codes.iter().filter(|c| **c == StatusCode::OK).count(),
        1,
        "{codes:?}"
    );
    assert_eq!(
        codes.iter().filter(|c| **c == StatusCode::CONFLICT).count(),
        5,
        "{codes:?}"
    );
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .count(&format!("PATCH /crm/v3/objects/contacts/{CONTACT}")),
        1
    );
    assert!(cfg.locks.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 先行する書き込みが終わらなければ_503_record_busy_で何も書かない() {
    let cfg = WriteConfig {
        lock_wait: Duration::from_millis(80),
        ..open()
    };
    let e = env_with(concurrent_fake(&["7241"]), cfg.clone(), true).await;
    let held = cfg
        .locks
        .acquire(
            [super::record_lock::key("deals", "7241")],
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    let (s, v) = patch_on(&e, "7241", memo_patch("op-busy-0001", "初期値", "新")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_eq!(v["error"], "record_busy");
    assert_eq!(e.calls("PATCH"), 0);
    assert_eq!(e.ledger_count(), 0);
    drop(held);
    let (s, _) = patch_on(&e, "7241", memo_patch("op-busy-0002", "初期値", "新")).await;
    assert_eq!(s, StatusCode::OK);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 再送_worker_も同じ鍵を待ち_取れなければ見送って_鍵が空けば保存する() {
    let cfg = WriteConfig {
        lock_wait: Duration::from_millis(80),
        ..open()
    };
    let mut f = concurrent_fake(&["7251"]);
    f.delay_ms = 0;
    f.patch_fail = vec![503];
    let e = env_with(f, cfg.clone(), true).await;
    let (s, _) = patch_on(&e, "7251", memo_patch("op-wk-00001", "初期値", "再送値")).await;
    assert_eq!(s, StatusCode::ACCEPTED);
    e.make_due("op-wk-00001");
    let held = cfg
        .locks
        .acquire(
            [super::record_lock::key("deals", "7251")],
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    write::run_due(&e.audit, &e.client, &cfg).await;
    assert_eq!(
        e.ledger("op-wk-00001").0,
        "pending",
        "鍵を取れない間は見送る"
    );
    assert_eq!(e.calls("PATCH"), 1, "見送った間は HubSpot へ送らない");
    drop(held);
    write::run_due(&e.audit, &e.client, &cfg).await;
    assert_eq!(e.ledger("op-wk-00001").0, "saved");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", "7251", "bpo_50")
            .as_deref(),
        Some("再送値")
    );
    assert!(cfg.locks.is_empty());
}

// ---------------------------------------------------------------------------
// 負荷試験 (2026-10-09) の指摘への対応
// ---------------------------------------------------------------------------

async fn get_status(e: &Env, cookie: &str, op: &str) -> (StatusCode, Value) {
    call(
        &e.app,
        Method::GET,
        &format!("/api/crm/operations/{op}"),
        Some(cookie),
        None,
        false,
    )
    .await
}

fn counter(kind: &str) -> u64 {
    write::rejections_snapshot().get(kind).copied().unwrap_or(0)
}

#[tokio::test(flavor = "multi_thread")]
async fn 状態の照会はメモリの表から答え_表に無いときだけ台帳を読んで入れ直す() {
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let e = env_with(f, open(), true).await;
    let (s, _) = e
        .patch(memo_patch("op-st-000001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    // 台帳の行を消しても、照会は表から答える (= Turso を読んでいない証拠)
    e.conn
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM crm_pending_operations WHERE operation_id = 'op-st-000001'",
            [],
        )
        .unwrap();
    let (s, v) = get_status(&e, &e.op, "op-st-000001").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "pending");
    assert_eq!(v["attempts"], 1);
    assert_eq!(v["last_error_code"], "hubspot_upstream");
    // 表から答えるときも持ち主の確認は同じ
    let (s, _) = get_status(&e, &e.other, "op-st-000001").await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    let admin = login(&e.app, ADMIN, "google_oidc").await;
    let (s, _) = get_status(&e, &admin, "op-st-000001").await;
    assert_eq!(s, StatusCode::OK, "管理者は他人の分も見られる");

    // 表に無い操作 (再起動した後など) は台帳を 1 回読み、表に入れ直す
    let payload = super::pending::Payload { steps: vec![] };
    pending_insert(&e, "op-st-000002", payload).await;
    let (s, v) = get_status(&e, &e.op, "op-st-000002").await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "pending", "in_progress は pending と答える");
    e.conn
        .lock()
        .unwrap()
        .execute(
            "DELETE FROM crm_pending_operations WHERE operation_id = 'op-st-000002'",
            [],
        )
        .unwrap();
    let (s, _) = get_status(&e, &e.op, "op-st-000002").await;
    assert_eq!(s, StatusCode::OK, "入れ直した表から答える");
    let (s, _) = get_status(&e, &e.op, "op-st-nothing1").await;
    assert_eq!(s, StatusCode::NOT_FOUND, "どこにも無ければ 404");
}

async fn pending_insert(e: &Env, op: &str, payload: super::pending::Payload) {
    let op = op.to_string();
    super::pending::blocking(&e.audit, move |t| {
        super::pending::insert_op(t, &op, OPERATOR, DEAL, &payload, "{}")
    })
    .await
    .unwrap()
    .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn 再送で保存したら状態の表も_saved_になり_照会が_saved_と答える() {
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let e = env_with(f, open(), true).await;
    let _ = e
        .patch(memo_patch("op-st-000003", "旧メモ", "新メモ"))
        .await;
    let (_, v) = get_status(&e, &e.op, "op-st-000003").await;
    assert_eq!(v["status"], "pending");
    e.make_due("op-st-000003");
    write::run_due(&e.audit, &e.client, &e.write).await;
    // 台帳を読み直さなくても worker の結果が見える
    e.conn
        .lock()
        .unwrap()
        .execute("DELETE FROM crm_pending_operations", [])
        .unwrap();
    let (_, v) = get_status(&e, &e.op, "op-st-000003").await;
    assert_eq!(v["status"], "saved");
}

#[tokio::test(flavor = "multi_thread")]
async fn 台帳に記録したあとで締め切りが来ても_打ち切らず最後まで送って確定し_202_を返す() {
    // 書けたのに応答が締め切りに間に合わない (負荷試験の「失敗と答えたのに入っていた」)
    let mut f = base_fake();
    f.patch_after_delay_ms = 1200;
    let cfg = WriteConfig {
        deadline: Duration::from_millis(500),
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let (s, v) = e
        .patch(memo_patch("op-to-000001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED, "504 ではなく 202: {v}");
    assert_eq!(v["status"], "queued");
    assert_eq!(v["operation_id"], "op-to-000001");
    // 送信は続いていて、台帳は in_progress のまま取り残されず確定する
    let mut status = String::new();
    for _ in 0..60 {
        tokio::time::sleep(Duration::from_millis(100)).await;
        status = e.ledger("op-to-000001").0;
        if status == "saved" {
            break;
        }
    }
    assert_eq!(status, "saved");
    assert_eq!(e.calls("PATCH"), 1, "二重に送らない");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("新メモ")
    );
    let (_, v) = get_status(&e, &e.op, "op-to-000001").await;
    assert_eq!(v["status"], "saved");
    wait_until("鍵が放される", || e.write.locks.is_empty()).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn 台帳に記録する前に締め切りが来たら_504_で何も送らず_後から送ることもない() {
    let mut f = base_fake();
    f.delay_ms = 900; // 現在値の読み取りが締め切りを超える
    let cfg = WriteConfig {
        deadline: Duration::from_millis(400),
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let before = counter("crm_timeout");
    let (s, v) = e
        .patch(memo_patch("op-to-000002", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::GATEWAY_TIMEOUT, "{v}");
    assert_eq!(v["error_kind"], "crm_timeout");
    assert!(counter("crm_timeout") > before, "crm_timeout を数える");
    // 取り下げたタスクが後から送らない
    tokio::time::sleep(Duration::from_millis(1800)).await;
    assert_eq!(e.calls("PATCH"), 0);
    assert_eq!(e.ledger_count(), 0);
    assert!(e.write.locks.is_empty(), "鍵も放している");
}

#[tokio::test(flavor = "multi_thread")]
async fn 断った要求は監査に書かず種類別に数え_競合と保存は監査に残る() {
    let e = env().await;
    // 必須項目の不足 (不通へ移すには bpo_10 が要る)
    let before = counter("missing_required");
    let (s, _) = e
        .patch(json!({
            "operation_id": "op-rj-000001",
            "base": {}, "set": {},
            "stage": {"pipeline_id": PIPELINE, "stage_id": FUZUU}
        }))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        counter("missing_required") > before,
        "missing_required を数える"
    );
    // 許可されない項目 (入力の誤り)
    let before_v = counter("validation");
    let (s, _) = e
        .patch(json!({"operation_id": "op-rj-000002", "base": {"ro_prop": "x"}, "set": {"ro_prop": "y"}}))
        .await;
    assert_eq!(s, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(counter("validation") > before_v, "validation を数える");
    // 読み取りに失敗 (存在しない案件)
    let before_nf = counter("read_failed:not_found");
    let (s, _) = call(
        &e.app,
        Method::PATCH,
        "/api/crm/deals/7777777",
        Some(&e.op),
        Some(memo_patch("op-rj-000003", "x", "y")),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::NOT_FOUND);
    assert!(
        counter("read_failed:not_found") > before_nf,
        "read_failed:not_found を数える"
    );
    assert!(
        e.audit_metas().is_empty(),
        "断った要求は監査 (activity_logs) に書かない: {:?}",
        e.audit_metas()
    );
    assert_eq!(e.ledger_count(), 0);

    // 競合は監査に残る
    let (s, _) = e.patch(memo_patch("op-rj-000004", "違う値", "新")).await;
    assert_eq!(s, StatusCode::CONFLICT);
    // 保存も残る
    let (s, _) = e
        .patch(memo_patch("op-rj-000005", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::OK);
    let outcomes: Vec<String> = e
        .audit_metas()
        .iter()
        .map(|m| m["outcome"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(outcomes, vec!["conflict", "saved"]);
}

#[tokio::test(flavor = "multi_thread")]
async fn 速さの上限と鍵の混雑も数えるだけで_監査にも台帳にも書かない() {
    let cfg = WriteConfig {
        rate_per_min: 1,
        lock_wait: Duration::from_millis(60),
        ..open()
    };
    let e = env_with(concurrent_fake(&["7261"]), cfg.clone(), true).await;
    let held = cfg
        .locks
        .acquire(
            [super::record_lock::key("deals", "7261")],
            Duration::from_secs(1),
        )
        .await
        .unwrap();
    let before_busy = counter("record_busy");
    let before_rate = counter("rate_limited");
    let (s, _) = patch_on(&e, "7261", memo_patch("op-rb-000001", "初期値", "A")).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    let (s, _) = patch_on(&e, "7261", memo_patch("op-rb-000002", "初期値", "B")).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    drop(held);
    assert!(counter("record_busy") > before_busy, "record_busy を数える");
    assert!(
        counter("rate_limited") > before_rate,
        "rate_limited を数える"
    );
    assert!(e.audit_metas().is_empty());
    assert_eq!(e.ledger_count(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn 送ったあとの曖昧な失敗が満杯のときは_読み直して書けていれば保存済み() {
    // PATCH は HubSpot に届いたが応答が 502。その間に再送待ちが上限に達した
    let mut f = base_fake();
    f.patch_after_delay_ms = 400;
    f.patch_apply_fail = vec![502];
    let cfg = WriteConfig {
        pending_max: 1,
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let app = e.app.clone();
    let cookie = e.op.clone();
    let req = tokio::spawn(async move {
        call(
            &app,
            Method::PATCH,
            &format!("/api/crm/deals/{DEAL}"),
            Some(&cookie),
            Some(memo_patch("op-amb-000001", "旧メモ", "新メモ")),
            true,
        )
        .await
    });
    // PATCH が HubSpot に届いた (応答は 400ms 止まっている) ことを待ってから割り込む
    wait_until("PATCH が届く", || e.calls("PATCH") == 1).await;
    // 送信中に別の操作が再送待ちとして入った (上限 1)
    pending_insert(
        &e,
        "op-amb-other1",
        super::pending::Payload { steps: vec![] },
    )
    .await;
    e.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE crm_pending_operations SET status = 'pending' WHERE operation_id = 'op-amb-other1'",
            [],
        )
        .unwrap();
    let (s, v) = req.await.unwrap();
    assert_eq!(s, StatusCode::OK, "書けていたので失敗と答えない: {v}");
    assert_eq!(v["status"], "saved");
    assert_eq!(e.ledger("op-amb-000001").0, "saved");
    assert_eq!(e.calls("PATCH"), 1, "書き直さない");
}

#[tokio::test(flavor = "multi_thread")]
async fn 送ったあとの曖昧な失敗が満杯で_書けていないと確かめられたときだけ_queue_full_で失敗() {
    let mut f = base_fake();
    f.delay_ms = 400; // 読み取りも PATCH も 400ms かかる (PATCH は書かずに 502)
    f.patch_fail = vec![502];
    let cfg = WriteConfig {
        pending_max: 1,
        ..open()
    };
    let e = env_with(f, cfg, true).await;
    let (app, cookie) = (e.app.clone(), e.op.clone());
    let req = tokio::spawn(async move {
        call(
            &app,
            Method::PATCH,
            &format!("/api/crm/deals/{DEAL}"),
            Some(&cookie),
            Some(memo_patch("op-amb-000002", "旧メモ", "新メモ")),
            true,
        )
        .await
    });
    // 読み取りが終わり PATCH が始まった (これも 400ms 止まる) ことを待ってから割り込む
    wait_until("PATCH が始まる", || {
        e.hs.lock().unwrap().patch_started == 1
    })
    .await;
    pending_insert(
        &e,
        "op-amb-other2",
        super::pending::Payload { steps: vec![] },
    )
    .await;
    e.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE crm_pending_operations SET status = 'pending' WHERE operation_id = 'op-amb-other2'",
            [],
        )
        .unwrap();
    let (s, v) = req.await.unwrap();
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_eq!(v["error"], "queue_full");
    assert_eq!(e.ledger("op-amb-000002").0, "failed");
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("旧メモ")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn プロパティ一覧の取り直しに失敗しても_前の一覧で続け_短い間隔で取り直す() {
    let (client, hs) = start_hs(base_fake()).await;
    // 時計は手で進める (実時間の sleep で期限を待つと、負荷の高い環境で順序が崩れる)
    let (clock, time) = super::clock::Clock::manual();
    let cache = super::property_catalog::PropertyCatalogCache::with_clock(
        Duration::from_millis(60),
        Duration::from_millis(120),
        clock,
    );
    let props_calls = |hs: &Shared<Fake>| hs.lock().unwrap().count("GET /crm/v3/properties/deals");
    let (entry, hit) = cache.get(&client).await.unwrap();
    assert!(!hit);
    assert!(entry.property("deals", "bpo_50").is_some());
    assert_eq!(props_calls(&hs), 1);
    // 期限の直前はキャッシュ
    time.advance(Duration::from_millis(59));
    let (_, hit) = cache.get(&client).await.unwrap();
    assert!(hit);
    assert_eq!(props_calls(&hs), 1);

    // 期限が切れ、取り直しが失敗する: 前の一覧を返す (保存の許可リストが止まらない)
    hs.lock().unwrap().props_fail = true;
    time.advance(Duration::from_millis(2));
    let (stale, hit) = cache.get(&client).await.unwrap();
    assert!(hit, "前の一覧");
    assert!(stale.property("deals", "bpo_50").is_some());
    assert_eq!(props_calls(&hs), 2, "期限が切れたので取り直しに行った");
    // 失敗の直後は取り直さない
    let _ = cache.get(&client).await.unwrap();
    assert_eq!(props_calls(&hs), 2, "失敗した直後は呼ばない");

    // 障害が終わっても、失敗の待ち (短い) の間は呼ばない。明けた瞬間に取り直す
    hs.lock().unwrap().props_fail = false;
    time.advance(Duration::from_millis(119));
    let (_, hit) = cache.get(&client).await.unwrap();
    assert!(hit);
    assert_eq!(props_calls(&hs), 2, "失敗の待ちの間は呼ばない");
    time.advance(Duration::from_millis(1));
    let (fresh, hit) = cache.get(&client).await.unwrap();
    assert!(!hit);
    assert!(fresh.property("deals", "bpo_50").is_some());
    assert_eq!(props_calls(&hs), 3);

    // 一度も取れていないときだけ失敗を返す
    let cold = super::property_catalog::PropertyCatalogCache::with_ttl(
        Duration::from_millis(60),
        Duration::from_millis(120),
    );
    hs.lock().unwrap().props_fail = true;
    assert!(cold.get(&client).await.is_err());
    assert_eq!(
        super::property_catalog::FAILURE_TTL,
        Duration::from_secs(5),
        "失敗を覚える時間は短い (60 秒から変更)"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 再送_worker_は別々の案件を同時に進める() {
    let deals: Vec<String> = (0..8).map(|i| format!("73{i:02}")).collect();
    let refs: Vec<&str> = deals.iter().map(String::as_str).collect();
    let mut f = concurrent_fake(&refs);
    f.delay_ms = 0;
    f.patch_fail = vec![503; 8];
    let e = env_with(f, open(), true).await;
    for (i, d) in deals.iter().enumerate() {
        let (s, _) = patch_on(
            &e,
            d,
            memo_patch(&format!("op-wc-{i:05}"), "初期値", "再送値"),
        )
        .await;
        assert_eq!(s, StatusCode::ACCEPTED);
    }
    for i in 0..8 {
        e.make_due(&format!("op-wc-{i:05}"));
    }
    // 偽 HubSpot の 1 回の呼び出しを 150ms 止める (同時に止まっている本数を max_inflight に記録する)
    e.hs.lock().unwrap().delay_ms = 150;
    let round = write::run_due_round(&e.audit, &e.client, &e.write).await;
    for i in 0..8 {
        assert_eq!(e.ledger(&format!("op-wc-{i:05}")).0, "saved");
    }
    // 時間でなく数で確かめる: 直列なら偽 HubSpot で同時に待たされるリクエストは常に 1 本
    let peak = e.hs.lock().unwrap().max_inflight;
    assert!(peak >= 2, "別々の案件は同時に進める: 同時 {peak} 本");
    assert!(!round.more, "取り切ったのですぐ次の周回はしない");
    assert!(e.write.locks.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn 再送_worker_は同じ案件の操作を古い順に直列で処理する() {
    // 同じ案件に 2 つ積む: 先の操作の base は旧メモ、後の操作の base は先の操作の書く値
    let mut f = base_fake();
    f.patch_fail = vec![503];
    let e = env_with(f, open(), true).await;
    let (s, _) = e
        .patch(memo_patch("op-og-000001", "旧メモ", "一回目"))
        .await;
    assert_eq!(s, StatusCode::ACCEPTED);
    // 2 つ目は台帳に直接積む (受付時は先の値がまだ HubSpot に無く、409 になるため)
    let steps = vec![super::pending::Step {
        object: "deals".into(),
        id: DEAL.into(),
        base: BTreeMap::from([("bpo_50".to_string(), Some("一回目".to_string()))]),
        set: BTreeMap::from([("bpo_50".to_string(), Some("二回目".to_string()))]),
        types: BTreeMap::from([("bpo_50".to_string(), "string".to_string())]),
        stage: None,
        done: false,
    }];
    pending_insert(&e, "op-og-000002", super::pending::Payload { steps }).await;
    e.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE crm_pending_operations SET status = 'pending', attempts = 1 WHERE operation_id = 'op-og-000002'",
            [],
        )
        .unwrap();
    e.make_due("op-og-000001");
    e.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE crm_pending_operations SET next_retry_at = '2000-01-01T00:00:05Z' WHERE operation_id = 'op-og-000002'",
            [],
        )
        .unwrap();
    write::run_due(&e.audit, &e.client, &e.write).await;
    assert_eq!(e.ledger("op-og-000001").0, "saved");
    assert_eq!(
        e.ledger("op-og-000002").0,
        "saved",
        "後の操作は先の結果を base に通る"
    );
    assert_eq!(
        e.hs.lock()
            .unwrap()
            .value("deals", DEAL, "bpo_50")
            .as_deref(),
        Some("二回目")
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 関所が混んでいると新しい保存は台帳も_hubspot_も触らず断り_受け付け済みの再送は結果を返す()
{
    use crate::hubspot::gateway::{Gateway, GatewayConfig};
    // 流量の制限は無いが、画面の操作の待ちの上限は 1 秒の関所
    let mut gc = GatewayConfig::unlimited(Duration::ZERO, Duration::from_millis(1));
    gc.interactive_max_wait = Duration::from_millis(1_000);
    let gw = Arc::new(Gateway::new(gc));
    let e = env_with_gw(base_fake(), open(), true, Some(gw.clone())).await;
    // 先に 1 件、保存を通しておく (台帳に結果が残る。関所は空いている)
    let (s, _) = e
        .patch(memo_patch("op-bz-000001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::OK);
    let calls_before = e.calls("");
    // 429 を受けた直後を再現する: 全員が数秒止まる (待ちの上限 1 秒を超える)
    gw.on_rate_limited(Some(Duration::from_secs(5)));
    let before = counter("hubspot_busy");
    let ledger_before = e.ledger_count();
    let (s, v) = e
        .patch(memo_patch("op-bz-000002", "新メモ", "別の値"))
        .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE, "{v}");
    assert_eq!(v["error_kind"], "hubspot_busy");
    assert!(counter("hubspot_busy") > before);
    assert_eq!(e.ledger_count(), ledger_before, "台帳に入れない");
    assert_eq!(e.calls(""), calls_before, "HubSpot を呼ばない (読み取りも)");
    assert!(
        e.audit_metas()
            .iter()
            .all(|m| m["operation_id"] != "op-bz-000002"),
        "断りは監査に書かない"
    );
    // 受け付け済みの操作の再送は、混んでいても保存された結果を返す
    let (s, v) = e
        .patch(memo_patch("op-bz-000001", "旧メモ", "新メモ"))
        .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["status"], "saved");
}
