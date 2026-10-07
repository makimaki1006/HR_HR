//! 役割 (admin / consultant / bpo / user) の本実装の結合テスト。
//!
//! - 役割の読み取り: 本物の `accounts` テーブル (SQLite 裏の偽 Turso `audit::fake_turso`) を `rbac::authorize` が
//!   本物の SQL (`dao::find_roles_by_email`) で引く。Turso の障害は `DROP TABLE accounts` で作る (SQL エラー)。
//! - レコード単位の制限 (BPO): 偽 HubSpot (127.0.0.1)。送られた呼び出しを全部記録し、
//!   「権限なしの経路では HubSpot 呼び出し 0 回」「外れた BPO に本文を返さない」を実ログで確かめる。
//! - 本物の Turso・HubSpot には接続しない。
//!
//! 想定の基準日時: JST 2026-10-05 12:00 (`today_ms` = 2026-10-05 の UTC 0 時)。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::{
    body::Body,
    extract::{Path, Query, RawQuery, State},
    http::{header, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{TimeZone, Utc};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use super::call_queue::CallQueueState;
use super::rbac::{
    finalize_role, lookup_role, CrmAccess, CrmRole, RoleCache, RoleLookup, ROLE_CACHE_TTL,
};
use crate::audit::fake_turso::{start_sqlite_audit, SharedConn};
use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::hubspot::{ClientOptions, HubSpotClient};
use crate::AppState;

type Shared<T> = Arc<Mutex<T>>;

const ADMIN: &str = "admin@f-a-c.co.jp";
const CONSULT: &str = "consult@f-a-c.co.jp";
const BPO: &str = "bpo@f-a-c.co.jp";
const BPO_OWNER: &str = "111";
const OTHER_OWNER: &str = "222";
const PIPELINE: &str = "753186575";
const UNPROCESSED: &str = "1095387442";
const FUZAI: &str = "1095387445";
const APPOINTED: &str = "1095457875"; // アポ日確定 (キューに出さない)
const SECRET: &str = "SECRET-DEAL-NAME-9F3";
const KEY: [u8; 32] = [3u8; 32];

// ---------------------------------------------------------------------------
// 偽 HubSpot
// ---------------------------------------------------------------------------

#[derive(Default)]
struct Hs {
    /// (object, id) → properties
    objects: HashMap<(String, String), Vec<(String, String)>>,
    /// (from object, from id, to object) → 関連 id
    assocs: HashMap<(String, String, String), Vec<String>>,
    /// email → owner id
    owners: HashMap<String, String>,
    /// email → 所属チーム名 (無ければ teams を返さない)
    teams: HashMap<String, Vec<String>>,
    /// Some(status) なら全部この status で失敗させる
    fail_all: Option<u16>,
    /// 受け取った呼び出し ("GET /crm/v3/objects/deals/1?..." など)
    log: Vec<String>,
}

impl Hs {
    fn put(&mut self, o: &str, id: &str, props: &[(&str, &str)]) {
        self.objects.insert(
            (o.into(), id.into()),
            props
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        );
    }
    fn link(&mut self, from: &str, id: &str, to: &str, ids: &[&str]) {
        self.assocs.insert(
            (from.into(), id.into(), to.into()),
            ids.iter().map(|s| s.to_string()).collect(),
        );
    }
    fn record(&self, o: &str, id: &str) -> Option<Value> {
        let props = self.objects.get(&(o.to_string(), id.to_string()))?;
        let p: serde_json::Map<String, Value> =
            props.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        Some(json!({"id": id, "properties": p, "archived": false,
                    "createdAt": "2026-01-01T00:00:00Z", "updatedAt": "2026-01-02T00:00:00Z"}))
    }
    fn count(&self, needle: &str) -> usize {
        self.log.iter().filter(|l| l.contains(needle)).count()
    }
}

fn failure(status: u16) -> Response {
    (
        StatusCode::from_u16(status).unwrap(),
        Json(json!({"status": "error", "message": SECRET})),
    )
        .into_response()
}

async fn hs_owners(
    State(st): State<Shared<Hs>>,
    Query(q): Query<HashMap<String, String>>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log.push(format!("GET /crm/v3/owners?{q:?}"));
    if let Some(c) = s.fail_all {
        return failure(c);
    }
    let email = q.get("email").cloned().unwrap_or_default().to_lowercase();
    let results: Vec<Value> = s
        .owners
        .get(&email)
        .map(|id| {
            let mut o = json!({"id": id, "email": email, "archived": false});
            if let Some(ts) = s.teams.get(&email) {
                o["teams"] = json!(ts
                    .iter()
                    .enumerate()
                    .map(|(i, n)| json!({"id": format!("{i}"), "name": n, "primary": i == 0}))
                    .collect::<Vec<_>>());
            }
            vec![o]
        })
        .unwrap_or_default();
    Json(json!({"results": results})).into_response()
}

async fn hs_get_object(
    State(st): State<Shared<Hs>>,
    Path((o, id)): Path<(String, String)>,
    RawQuery(raw): RawQuery,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log.push(format!(
        "GET /crm/v3/objects/{o}/{id}?{}",
        raw.clone().unwrap_or_default()
    ));
    if let Some(c) = s.fail_all {
        return failure(c);
    }
    let Some(mut rec) = s.record(&o, &id) else {
        return failure(404);
    };
    // associations=a,b を要求されたら、その型の関連を返す
    let wanted: Vec<String> = raw
        .unwrap_or_default()
        .split('&')
        .find_map(|kv| kv.strip_prefix("associations="))
        .map(|v| {
            v.replace("%2C", ",")
                .split(',')
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default();
    let mut assoc = serde_json::Map::new();
    for t in wanted {
        let ids = s
            .assocs
            .get(&(o.clone(), id.clone(), t.clone()))
            .cloned()
            .unwrap_or_default();
        if !ids.is_empty() {
            assoc.insert(
                t,
                json!({"results": ids.iter().map(|i| json!({"id": i, "type": "x"})).collect::<Vec<_>>()}),
            );
        }
    }
    if !assoc.is_empty() {
        rec["associations"] = Value::Object(assoc);
    }
    Json(rec).into_response()
}

async fn hs_batch_read(
    State(st): State<Shared<Hs>>,
    Path(o): Path<String>,
    Json(body): Json<Value>,
) -> Response {
    let mut s = st.lock().unwrap();
    s.log.push(format!("POST /crm/v3/objects/{o}/batch/read"));
    if let Some(c) = s.fail_all {
        return failure(c);
    }
    let results: Vec<Value> = body["inputs"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .iter()
        .filter_map(|i| s.record(&o, i["id"].as_str().unwrap_or("")))
        .collect();
    Json(json!({"status": "COMPLETE", "results": results})).into_response()
}

async fn hs_search(State(st): State<Shared<Hs>>, Json(body): Json<Value>) -> Response {
    let mut s = st.lock().unwrap();
    s.log
        .push(format!("POST /crm/v3/objects/deals/search {body}"));
    Json(json!({"results": [], "total": 0})).into_response()
}

async fn hs_properties(State(st): State<Shared<Hs>>, Path(o): Path<String>) -> Response {
    st.lock()
        .unwrap()
        .log
        .push(format!("GET /crm/v3/properties/{o}"));
    Json(json!({"results": []})).into_response()
}

async fn hs_pipelines(State(st): State<Shared<Hs>>) -> Response {
    st.lock()
        .unwrap()
        .log
        .push("GET /crm/v3/pipelines/deals".into());
    Json(json!({"results": []})).into_response()
}

async fn hs_assoc_batch(
    State(st): State<Shared<Hs>>,
    Path((f, t)): Path<(String, String)>,
) -> Response {
    st.lock()
        .unwrap()
        .log
        .push(format!("POST /crm/v4/associations/{f}/{t}/batch/read"));
    Json(json!({"status": "COMPLETE", "results": []})).into_response()
}

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

async fn start_hs(hs: Hs) -> (Arc<HubSpotClient>, Shared<Hs>) {
    let st = Arc::new(Mutex::new(hs));
    let base = spawn(
        Router::new()
            .route("/crm/v3/owners", get(hs_owners))
            .route("/crm/v3/pipelines/deals", get(hs_pipelines))
            .route("/crm/v3/properties/{o}", get(hs_properties))
            .route("/crm/v3/objects/deals/search", post(hs_search))
            .route("/crm/v3/objects/{o}/{id}", get(hs_get_object))
            .route("/crm/v3/objects/{o}/batch/read", post(hs_batch_read))
            .route(
                "/crm/v4/associations/{f}/{t}/batch/read",
                post(hs_assoc_batch),
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
    .expect("client");
    (Arc::new(client), st)
}

// ---------------------------------------------------------------------------
// アプリ
// ---------------------------------------------------------------------------

fn state_with(
    audit: Option<AuditDb>,
    hubspot: Option<Arc<HubSpotClient>>,
    admin_emails: &[&str],
) -> Arc<AppState> {
    let config = AppConfig {
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
        admin_emails: admin_emails.iter().map(|s| s.to_string()).collect(),
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    };
    Arc::new(AppState {
        config,
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

fn app_with(state: Arc<AppState>, access: CrmAccess) -> Router {
    let now = Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap();
    Router::new()
        .merge(super::routes::router_with_queue(
            access,
            CallQueueState::for_test(KEY, now),
        ))
        .route("/__test/session", post(inject_session))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

async fn login(app: &Router, email: &str, method: &str) -> String {
    login_with_account(app, email, method, None).await
}

async fn login_with_account(
    app: &Router,
    email: &str,
    method: &str,
    account_id: Option<&str>,
) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/__test/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email": email, "login_method": method, "account_id": account_id})
                        .to_string(),
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

/// (status, 本文の文字列)
async fn get_text(app: &Router, uri: &str, cookie: &str) -> (StatusCode, String) {
    let resp = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = resp.status();
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    (status, String::from_utf8(bytes.to_vec()).unwrap())
}

fn kind(body: &str) -> String {
    serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["error_kind"].as_str().map(str::to_string))
        .unwrap_or_default()
}

fn add_account(conn: &SharedConn, id: &str, email: &str, role: &str) {
    conn.lock()
        .unwrap()
        .execute(
            "INSERT INTO accounts (id, email, role, first_seen_at, last_login_at, login_count) \
             VALUES (?1, ?2, ?3, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z', 1)",
            rusqlite::params![id, email, role],
        )
        .unwrap();
}

fn break_accounts(conn: &SharedConn) {
    conn.lock()
        .unwrap()
        .execute("DROP TABLE accounts", [])
        .unwrap();
}

// ---------------------------------------------------------------------------
// 1. 役割の読み取り (本物の accounts テーブル経由)
// ---------------------------------------------------------------------------

/// accounts.role の値に関わらず、会社 Google ログインなら全員 CRM を使える (認可を抜けて HubSpot 未設定の 503 まで進む)。
/// 行なし・未知の値・user でも拒否しない (範囲は別: 管理者以外は自分の分だけ)
#[tokio::test(flavor = "multi_thread")]
async fn accounts_の_role_の値に関わらず全員通る() {
    let (audit, conn) = start_sqlite_audit().await;
    let roles = [
        "admin",
        "consultant",
        "bpo",
        "user",
        "Admin",
        " BPO ",
        "",
        "   ",
        "superuser",
        "ａｄｍｉｎ",
        "admin,bpo",
        "manager",
    ];
    for (i, role) in roles.iter().enumerate() {
        add_account(&conn, &format!("a{i}"), &format!("u{i}@f-a-c.co.jp"), role);
    }
    let state = state_with(Some(audit), None, &[]);
    let app = app_with(state, CrmAccess::from_list(""));
    let mut emails: Vec<String> = (0..roles.len())
        .map(|i| format!("u{i}@f-a-c.co.jp"))
        .collect();
    emails.push("norow@f-a-c.co.jp".to_string()); // accounts に行なし
    for email in &emails {
        let cookie = login(&app, email, "google_oidc").await;
        for path in [
            "/api/crm/deals/1",
            "/api/crm/metadata",
            "/api/crm/call-queue",
        ] {
            let (status, body) = get_text(&app, path, &cookie).await;
            assert_eq!(
                (status, kind(&body)),
                (StatusCode::SERVICE_UNAVAILABLE, "not_configured".into()),
                "{email} {path}"
            );
        }
    }
}

/// 会社ドメイン外・パスワードログインは拒否し、HubSpot を 1 回も呼ばない
#[tokio::test(flavor = "multi_thread")]
async fn パスワードログインと社外ドメインは拒否され_hubspot_を呼ばない() {
    let (client, hs) = start_hs(std_hs()).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let paths = [
        "/api/crm/metadata",
        "/api/crm/call-queue",
        "/api/crm/owners",
        "/api/crm/deals/1",
        "/api/crm/contacts/1",
        "/api/crm/companies/1",
    ];
    // (メール, ログイン方式, 期待 status, error_kind)
    let cases: &[(&str, &str, u16, &str)] = &[
        (BPO, "password_internal", 403, "google_login_required"),
        (BPO, "password_external", 403, "google_login_required"),
        (ADMIN, "password_internal", 403, "google_login_required"),
        ("outsider@example.com", "google_oidc", 403, "forbidden"),
        (
            "outsider@f-a-c.co.jp.evil.com",
            "google_oidc",
            403,
            "forbidden",
        ),
        ("outsider@sub.f-a-c.co.jp", "google_oidc", 403, "forbidden"),
    ];
    for (email, method, status, kind_want) in cases {
        let cookie = login(&app, email, method).await;
        for path in paths {
            let (s, b) = get_text(&app, path, &cookie).await;
            assert_eq!(
                (s.as_u16(), kind(&b).as_str()),
                (*status, *kind_want),
                "{email} {method} {path}"
            );
            assert!(!b.contains(SECRET));
        }
    }
    assert_eq!(
        hs.lock().unwrap().log,
        Vec::<String>::new(),
        "HubSpot を 1 回も呼ばない"
    );
}

/// 同じメールの行が複数あるときは最小権限 (admin と user があれば user)
#[tokio::test(flavor = "multi_thread")]
async fn 重複行は最小権限() {
    let (audit, conn) = start_sqlite_audit().await;
    add_account(&conn, "e1", "dup@f-a-c.co.jp", "admin");
    add_account(&conn, "e2", "DUP@f-a-c.co.jp", "user");
    let state = state_with(Some(audit), None, &[]);
    let r = lookup_role(&state, &RoleCache::new(), "dup@f-a-c.co.jp", Instant::now()).await;
    assert_eq!(r, RoleLookup::Found(CrmRole::User));
}

/// Turso 障害: accounts を読めないときは管理者になれない (権限を広げない)。`ADMIN_EMAILS` の人だけ非常口で admin。
/// 管理者以外 (accounts.role = admin の人を含む) は自分の分だけのまま。DB を叩く前に落ちる経路 (未接続) も同じ
#[tokio::test(flavor = "multi_thread")]
async fn turso_障害時は管理者を広げない() {
    let (audit, conn) = start_sqlite_audit().await;
    add_account(&conn, "f3", "acctadmin@f-a-c.co.jp", "admin");
    break_accounts(&conn);
    for audit in [Some(audit), None] {
        let (client, _hs) = start_hs(std_hs()).await;
        let state = state_with(audit, Some(client), &["boss@f-a-c.co.jp"]);
        let app = app_with(state, CrmAccess::from_list(""));
        // 管理者だけの担当者一覧: 非常口の ADMIN_EMAILS は 200、accounts の admin は読めないので 403
        let cookie = login(&app, "boss@f-a-c.co.jp", "google_oidc").await;
        let (s, _) = get_text(&app, "/api/crm/owners", &cookie).await;
        assert_eq!(s, StatusCode::OK, "非常口の ADMIN_EMAILS");
        for email in ["acctadmin@f-a-c.co.jp", "random@f-a-c.co.jp"] {
            let cookie = login(&app, email, "google_oidc").await;
            let (s, b) = get_text(&app, "/api/crm/owners", &cookie).await;
            assert_eq!(
                (s, kind(&b)),
                (StatusCode::FORBIDDEN, "forbidden".into()),
                "{email}"
            );
            // CRM 自体は使える (自分の分だけ。owner が無いので owner_not_found)
            let (s, b) = get_text(&app, "/api/crm/call-queue", &cookie).await;
            assert_eq!(
                (s, kind(&b)),
                (StatusCode::FORBIDDEN, "owner_not_found".into()),
                "{email}"
            );
        }
    }
}

/// 管理者 = ADMIN_EMAILS または accounts.role = admin。ADMIN_EMAILS の人は accounts の値 (user など) に関わらず admin
#[tokio::test(flavor = "multi_thread")]
async fn 管理者は_admin_emails_か_accounts_admin() {
    let (audit, conn) = start_sqlite_audit().await;
    add_account(&conn, "d1", "boss@f-a-c.co.jp", "user");
    add_account(&conn, "d2", "acctadmin@f-a-c.co.jp", "admin");
    add_account(&conn, "d3", "consult@f-a-c.co.jp", "consultant");
    add_account(&conn, "d4", "bpo@f-a-c.co.jp", "bpo");
    let (client, _hs) = start_hs(std_hs()).await;
    let app = app_with(
        state_with(Some(audit), Some(client), &["boss@f-a-c.co.jp"]),
        CrmAccess::from_list(""),
    );
    for (email, owners_status) in [
        ("boss@f-a-c.co.jp", 200),
        ("acctadmin@f-a-c.co.jp", 200),
        ("consult@f-a-c.co.jp", 403), // consultant の値は判定に使わない
        ("bpo@f-a-c.co.jp", 403),
        ("nobody@f-a-c.co.jp", 403),
    ] {
        let cookie = login(&app, email, "google_oidc").await;
        let (s, b) = get_text(&app, "/api/crm/owners", &cookie).await;
        assert_eq!(s.as_u16(), owners_status, "{email}: {b}");
    }
}

/// キャッシュ: 5 分は DB を引かない (DB を壊しても値が残る)。5 分を過ぎたら引き直し、
/// **引き直しに失敗したら古い値を使わず Unavailable** (権限を広げない)。失敗は 30 秒だけ覚える
#[tokio::test(flavor = "multi_thread")]
async fn 役割の読み取りは_5_分キャッシュで_失敗時に古い値を使わない() {
    let (audit, conn) = start_sqlite_audit().await;
    add_account(&conn, "g1", "cache@f-a-c.co.jp", "admin");
    let state = state_with(Some(audit), None, &[]);
    let cache = RoleCache::new();
    let t0 = Instant::now();
    let at = |secs: u64| t0 + Duration::from_secs(secs);
    let email = "cache@f-a-c.co.jp";

    assert_eq!(
        lookup_role(&state, &cache, email, t0).await,
        RoleLookup::Found(CrmRole::Admin)
    );
    // DB 側を user に変えても、期限内は古い値のまま (キャッシュ)
    conn.lock()
        .unwrap()
        .execute("UPDATE accounts SET role = 'user' WHERE id = 'g1'", [])
        .unwrap();
    assert_eq!(
        lookup_role(&state, &cache, email, at(299)).await,
        RoleLookup::Found(CrmRole::Admin)
    );
    // 期限 (5 分) を過ぎたら引き直して user
    assert_eq!(ROLE_CACHE_TTL.as_secs(), 300);
    assert_eq!(
        lookup_role(&state, &cache, email, at(300)).await,
        RoleLookup::Found(CrmRole::User)
    );
    // 昇格し直して期限切れ → DB を壊す → 古い admin を使わず Unavailable
    conn.lock()
        .unwrap()
        .execute("UPDATE accounts SET role = 'admin' WHERE id = 'g1'", [])
        .unwrap();
    assert_eq!(
        lookup_role(&state, &cache, email, at(700)).await,
        RoleLookup::Found(CrmRole::Admin)
    );
    break_accounts(&conn);
    let r = lookup_role(&state, &cache, email, at(1100)).await;
    assert_eq!(
        r,
        RoleLookup::Unavailable,
        "期限切れ + 障害 = 古い admin を使わない"
    );
    assert_eq!(
        finalize_role(r, email, &[]),
        CrmRole::Bpo,
        "読めなければ管理者にしない"
    );
    // 失敗は 30 秒覚える (その間は DB を引かない = 復旧しても 29 秒後はまだ Unavailable)
    conn.lock()
        .unwrap()
        .execute_batch(
            "CREATE TABLE accounts (id TEXT PRIMARY KEY, email TEXT, role TEXT, \
             first_seen_at TEXT, last_login_at TEXT, login_count INTEGER, \
             display_name TEXT, company TEXT, disabled_at TEXT);
             INSERT INTO accounts (id, email, role) VALUES ('g9', 'cache@f-a-c.co.jp', 'bpo');",
        )
        .unwrap();
    assert_eq!(
        lookup_role(&state, &cache, email, at(1129)).await,
        RoleLookup::Unavailable
    );
    assert_eq!(
        lookup_role(&state, &cache, email, at(1130)).await,
        RoleLookup::Found(CrmRole::Bpo)
    );
}

/// 許可リスト (`CRM_METADATA_ALLOWED_EMAILS`) は設定されていれば役割に加えて必要 (AND)。
/// 空なら絞り込まない (役割だけ)
#[tokio::test(flavor = "multi_thread")]
async fn 許可リストは設定されていれば役割に加えて必要() {
    let (audit, conn) = start_sqlite_audit().await;
    add_account(&conn, "h1", "in@f-a-c.co.jp", "bpo");
    add_account(&conn, "h2", "out@f-a-c.co.jp", "admin");
    let state = state_with(Some(audit), None, &[]);
    let restricted = app_with(state.clone(), CrmAccess::from_list("in@f-a-c.co.jp"));
    let open = app_with(state, CrmAccess::from_list(""));
    let passed = (
        StatusCode::SERVICE_UNAVAILABLE,
        "not_configured".to_string(),
    );
    let denied = (StatusCode::FORBIDDEN, "forbidden".to_string());
    for (app, email, want) in [
        (&restricted, "in@f-a-c.co.jp", &passed),
        (&restricted, "out@f-a-c.co.jp", &denied), // admin でも一覧外は拒否
        (&open, "in@f-a-c.co.jp", &passed),
        (&open, "out@f-a-c.co.jp", &passed),
    ] {
        let cookie = login(app, email, "google_oidc").await;
        let (s, b) = get_text(app, "/api/crm/deals/1", &cookie).await;
        assert_eq!((s, kind(&b)), *want, "{email}");
    }
}

// ---------------------------------------------------------------------------
// 2. 役割 × エンドポイント。権限なしの経路では HubSpot 呼び出し 0 回
// ---------------------------------------------------------------------------

fn std_hs() -> Hs {
    let mut hs = Hs::default();
    hs.owners.insert(BPO.into(), BPO_OWNER.into());
    hs.put(
        "deals",
        "1",
        &[
            ("dealname", SECRET),
            ("pipeline", PIPELINE),
            ("dealstage", UNPROCESSED),
            ("hubspot_owner_id", BPO_OWNER),
        ],
    );
    hs
}

/// 役割はテストで固定せず、本番と同じ経路 (ADMIN_EMAILS = ADMIN だけ管理者、他は全員 自分の分だけ) で決める
fn role_access() -> CrmAccess {
    CrmAccess::from_list("")
}

/// 管理者は他人の Deal も読め、関門 (本人 owner の解決) を通らない
#[tokio::test(flavor = "multi_thread")]
async fn 管理者は全レコードを読め_関門を通らない() {
    let mut hs = std_hs();
    // 他人の担当・架電禁止・アポ確定 = 管理者以外なら読めないもの
    hs.put(
        "deals",
        "3",
        &[
            ("dealname", SECRET),
            ("pipeline", PIPELINE),
            ("dealstage", APPOINTED),
            ("hubspot_owner_id", OTHER_OWNER),
            ("bpo_3", "架電禁止"),
        ],
    );
    let (client, hs) = start_hs(hs).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, ADMIN, "google_oidc").await;
    let (s, b) = get_text(&app, "/api/crm/deals/3", &cookie).await;
    assert_eq!(s, StatusCode::OK, "{b}");
    assert!(b.contains(SECRET));
    assert_eq!(
        hs.lock().unwrap().count("/crm/v3/owners"),
        0,
        "関門 (owner 解決) を通らない"
    );
}

// ---------------------------------------------------------------------------
// 3. BPO のレコード単位の制限
// ---------------------------------------------------------------------------

fn gate_hs() -> Hs {
    let mut hs = Hs::default();
    hs.owners.insert(BPO.into(), BPO_OWNER.into());
    let deal = |hs: &mut Hs, id: &str, owner: Option<&str>, stage: &str, extra: &[(&str, &str)]| {
        let mut props: Vec<(&str, &str)> = vec![
            ("dealname", SECRET),
            ("pipeline", PIPELINE),
            ("dealstage", stage),
        ];
        if let Some(o) = owner {
            props.push(("hubspot_owner_id", o));
        }
        props.extend_from_slice(extra);
        hs.put("deals", id, &props);
    };
    // 自分の担当でキューに出るもの
    deal(&mut hs, "1", Some(BPO_OWNER), UNPROCESSED, &[]);
    deal(
        &mut hs,
        "2",
        Some(BPO_OWNER),
        FUZAI,
        &[("bpo_13", "2026-10-05")],
    ); // 今日
    deal(
        &mut hs,
        "21",
        Some(BPO_OWNER),
        FUZAI,
        &[("bpo_13", "2026-10-01")],
    ); // 過去
       // 自分の担当だがキューに出ないもの
    deal(
        &mut hs,
        "20",
        Some(BPO_OWNER),
        FUZAI,
        &[("bpo_13", "2026-10-06")],
    ); // 明日
    deal(&mut hs, "22", Some(BPO_OWNER), FUZAI, &[]); // 次回日なし
    deal(
        &mut hs,
        "23",
        Some(BPO_OWNER),
        FUZAI,
        &[("bpo_13", "not a date")],
    );
    deal(
        &mut hs,
        "4",
        Some(BPO_OWNER),
        UNPROCESSED,
        &[("bpo_3", "架電禁止理由")],
    );
    deal(
        &mut hs,
        "5",
        Some(BPO_OWNER),
        UNPROCESSED,
        &[("bpo_4", "ブロック理由")],
    );
    deal(
        &mut hs,
        "6",
        Some(BPO_OWNER),
        APPOINTED,
        &[("bpo_13", "2026-10-01")],
    );
    hs.put(
        "deals",
        "7",
        &[
            ("dealname", SECRET),
            ("pipeline", "999"),
            ("dealstage", UNPROCESSED),
            ("hubspot_owner_id", BPO_OWNER),
        ],
    );
    // 他人の担当・担当なし
    deal(&mut hs, "3", Some(OTHER_OWNER), UNPROCESSED, &[]);
    deal(&mut hs, "30", None, UNPROCESSED, &[]);
    // Contact / Company
    for (id, deals) in [
        ("101", &["1"][..]),            // 自分のキューの Deal に紐づく
        ("102", &["3"][..]),            // 他人の Deal だけ
        ("103", &["3", "1"][..]),       // 他人 + 自分の両方
        ("104", &[][..]),               // Deal なし
        ("105", &["20", "4", "6"][..]), // 自分の担当だがキュー外の Deal だけ
        ("106", &["30"][..]),           // 担当なしの Deal だけ
    ] {
        hs.put(
            "contacts",
            id,
            &[("firstname", SECRET), ("phone", "03-0000-0000")],
        );
        if !deals.is_empty() {
            hs.link("contacts", id, "deals", deals);
        }
    }
    for (id, deals) in [("201", &["2"][..]), ("202", &["3"][..]), ("203", &[][..])] {
        hs.put("companies", id, &[("name", SECRET)]);
        if !deals.is_empty() {
            hs.link("companies", id, "deals", deals);
        }
    }
    hs
}

/// (種類, id, BPO が読めるか)
const GATE_TABLE: &[(&str, &str, bool)] = &[
    // Deal: 自分 × キュー内
    ("deals", "1", true),
    ("deals", "2", true),
    ("deals", "21", true),
    // Deal: 自分だがキュー外 (明日 / 次回日なし / 日付不正 / 架電禁止 / ブロック / アポ確定 / 別パイプライン)
    ("deals", "20", false),
    ("deals", "22", false),
    ("deals", "23", false),
    ("deals", "4", false),
    ("deals", "5", false),
    ("deals", "6", false),
    ("deals", "7", false),
    // Deal: 他人 / 担当なし / 存在しない
    ("deals", "3", false),
    ("deals", "30", false),
    ("deals", "99999", false),
    // Contact: キュー内の Deal に紐づくだけ
    ("contacts", "101", true),
    ("contacts", "103", true),
    ("contacts", "102", false),
    ("contacts", "104", false),
    ("contacts", "105", false),
    ("contacts", "106", false),
    ("contacts", "199", false),
    // Company
    ("companies", "201", true),
    ("companies", "202", false),
    ("companies", "203", false),
    ("companies", "299", false),
];

#[tokio::test(flavor = "multi_thread")]
async fn bpo_は_自分の担当でキューに出る_deal_と_その_contact_company_だけ読める() {
    let (client, hs) = start_hs(gate_hs()).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, BPO, "google_oidc").await;
    for (kind_name, id, allowed) in GATE_TABLE {
        let before = hs.lock().unwrap().log.len();
        let (s, b) = get_text(&app, &format!("/api/crm/{kind_name}/{id}"), &cookie).await;
        if *allowed {
            assert_eq!(s, StatusCode::OK, "{kind_name}/{id}: {b}");
            assert!(b.contains(SECRET), "{kind_name}/{id}");
        } else {
            // 外れたら 403 forbidden_record だけ。存在しない id も同じ (404 で存在を教えない)
            assert_eq!(
                (s, b.as_str()),
                (
                    StatusCode::FORBIDDEN,
                    r#"{"error_kind":"forbidden_record"}"#
                ),
                "{kind_name}/{id}"
            );
            // 本文 (関連・アクティビティの読み取り) に進んでいない: 関門の 1〜2 回 (+ owner) 以外の呼び出しが無い
            let new_calls: Vec<String> = hs.lock().unwrap().log[before..].to_vec();
            assert!(
                !new_calls.iter().any(|l| l.contains("calls")
                    || l.contains("notes")
                    || l.contains("tasks")
                    || l.contains("meetings")
                    || l.contains("/crm/v4/associations")),
                "{kind_name}/{id}: {new_calls:?}"
            );
        }
    }
}

/// 許可表 (決定 2026-10-07): 管理者は全件。管理者以外は全員 (BPO のチームでも、そうでなくても) 自分の分だけ。
/// 列: admin / 非 BPO の社内ユーザー (owner 333、自分の Deal 7 あり) / BPO チームの人 (owner 111) / HubSpot owner なしの人
#[tokio::test(flavor = "multi_thread")]
async fn 役割ごとの許可表() {
    let mut hs = gate_hs();
    hs.owners.insert(CONSULT.into(), "333".into());
    hs.teams.insert(CONSULT.into(), vec!["新規営業".into()]);
    hs.owners.insert(BPO.into(), BPO_OWNER.into());
    hs.teams.insert(BPO.into(), vec!["BPO_リクロジ".into()]);
    hs.owners.insert(ADMIN.into(), "444".into());
    hs.put(
        "deals",
        "7",
        &[
            ("dealname", SECRET),
            ("pipeline", PIPELINE),
            ("dealstage", UNPROCESSED),
            ("hubspot_owner_id", "333"),
        ],
    );
    let (client, _hs) = start_hs(hs).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let table: &[(&str, [u16; 4])] = &[
        // キュー: 既定 (admin = 全員分、他は自分)
        ("/api/crm/call-queue", [200, 200, 200, 403]),
        // キュー: 全員分 / 担当なし / 他人 は管理者だけ。自分 (me) は誰でも (owner があれば)
        ("/api/crm/call-queue?owner=all", [200, 403, 403, 403]),
        ("/api/crm/call-queue?owner=unassigned", [200, 403, 403, 403]),
        ("/api/crm/call-queue?owner=222", [200, 403, 403, 403]),
        ("/api/crm/call-queue?owner=me", [200, 200, 200, 403]),
        // 担当者一覧: admin だけ
        ("/api/crm/owners", [200, 403, 403, 403]),
        // 個別 Deal: 自分のキュー内 / 他人
        ("/api/crm/deals/1", [200, 403, 200, 403]),
        ("/api/crm/deals/7", [200, 200, 403, 403]),
        ("/api/crm/deals/3", [200, 403, 403, 403]),
        // Contact / Company: 自分のキュー内の Deal に紐づく / 無関係
        ("/api/crm/contacts/101", [200, 403, 200, 403]),
        ("/api/crm/contacts/102", [200, 403, 403, 403]),
        ("/api/crm/companies/201", [200, 403, 200, 403]),
        ("/api/crm/companies/202", [200, 403, 403, 403]),
        // metadata: ログインした全員 (定義だけ。顧客の値は返らない)
        ("/api/crm/metadata", [200, 200, 200, 200]),
    ];
    for (i, email) in [ADMIN, CONSULT, BPO, "noowner@f-a-c.co.jp"]
        .iter()
        .enumerate()
    {
        let cookie = login(&app, email, "google_oidc").await;
        for (path, want) in table {
            let (s, b) = get_text(&app, path, &cookie).await;
            assert_eq!(s.as_u16(), want[i], "{email} {path}: {b}");
        }
    }
}

/// 所属チームの違い (BPO / 非 BPO / 複数の片方が BPO / primary でない BPO / チームなし / 前後空白・大文字小文字違い)
/// で見られる範囲は変わらない (全員 自分の分だけ)。チーム名は scope.teams に参考として出るだけ
#[tokio::test(flavor = "multi_thread")]
async fn 所属チームは範囲を変えず参考表示だけ() {
    let teams_cases: Vec<(&str, Option<Vec<&str>>)> = vec![
        ("t1@f-a-c.co.jp", Some(vec!["BPO_リクロジ"])),
        ("t2@f-a-c.co.jp", Some(vec!["新規営業"])),
        ("t3@f-a-c.co.jp", Some(vec!["新規営業", "BPO_リクロジ"])), // 2 つ目 (primary でない) が BPO
        ("t4@f-a-c.co.jp", Some(vec!["BPO_リクロジ", "管理部"])),
        ("t5@f-a-c.co.jp", Some(vec![])), // チームなし
        ("t6@f-a-c.co.jp", None),         // teams キー自体なし
        ("t7@f-a-c.co.jp", Some(vec![" bpo_リクロジ "])), // 空白付き (表示は trim)
    ];
    let mut hs = gate_hs();
    for (i, (email, teams)) in teams_cases.iter().enumerate() {
        hs.owners.insert(email.to_string(), format!("9{i}"));
        if let Some(t) = teams {
            hs.teams
                .insert(email.to_string(), t.iter().map(|s| s.to_string()).collect());
        }
    }
    let (client, hs) = start_hs(hs).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    for (i, (email, teams)) in teams_cases.iter().enumerate() {
        let cookie = login(&app, email, "google_oidc").await;
        hs.lock().unwrap().log.clear();
        let (s, b) = get_text(&app, "/api/crm/call-queue", &cookie).await;
        assert_eq!(s, StatusCode::OK, "{email}: {b}");
        let v: Value = serde_json::from_str(&b).unwrap();
        assert_eq!(v["scope"]["role"], "own", "{email}");
        assert_eq!(v["scope"]["owner"], "me", "{email}");
        let want: Vec<String> = teams
            .clone()
            .unwrap_or_default()
            .iter()
            .map(|s| s.trim().to_string())
            .collect();
        assert_eq!(v["scope"]["teams"], json!(want), "{email}");
        // 検索は自分の owner id で絞る (全員分に倒れない)
        let searches: Vec<String> = hs
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|l| l.starts_with("POST /crm/v3/objects/deals/search"))
            .cloned()
            .collect();
        assert!(!searches.is_empty(), "{email}");
        for q in &searches {
            assert!(q.contains(&format!(r#""value":"9{i}""#)), "{email}: {q}");
        }
        // 他人の指定・他人の Deal は 403
        for p in [
            "/api/crm/call-queue?owner=all",
            "/api/crm/call-queue?owner=111",
        ] {
            let (s, b) = get_text(&app, p, &cookie).await;
            assert_eq!(
                (s, kind(&b)),
                (StatusCode::FORBIDDEN, "forbidden_owner".into()),
                "{email} {p}"
            );
        }
        let (s, b) = get_text(&app, "/api/crm/deals/1", &cookie).await;
        assert_eq!(
            (s, kind(&b)),
            (StatusCode::FORBIDDEN, "forbidden_record".into()),
            "{email}"
        );
        assert!(!b.contains(SECRET));
    }
    // 管理者は HubSpot の owner を引かず teams は空
    let cookie = login(&app, ADMIN, "google_oidc").await;
    let (s, b) = get_text(&app, "/api/crm/call-queue", &cookie).await;
    assert_eq!(s, StatusCode::OK);
    let v: Value = serde_json::from_str(&b).unwrap();
    assert_eq!(
        (v["scope"]["role"].as_str(), v["scope"]["owner"].as_str()),
        (Some("admin"), Some("all"))
    );
    assert_eq!(v["scope"]["teams"], json!([]));
}

/// Owners の取得に失敗したら、全件に倒さずエラー (本文なし・検索なし)。失敗はキャッシュしない
#[tokio::test(flavor = "multi_thread")]
async fn owners_取得失敗は全件に倒さずエラー() {
    let mut hs = gate_hs();
    hs.owners.insert(CONSULT.into(), "333".into());
    let (client, hs) = start_hs(hs).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, CONSULT, "google_oidc").await;
    hs.lock().unwrap().fail_all = Some(500);
    for p in [
        "/api/crm/call-queue",
        "/api/crm/call-queue?owner=me",
        "/api/crm/deals/1",
        "/api/crm/contacts/101",
        "/api/crm/companies/201",
        "/api/crm/workspace/deals/1",
    ] {
        let (s, b) = get_text(&app, p, &cookie).await;
        assert!(s.is_server_error(), "{p}: {s} {b}");
        assert!(!b.contains(SECRET) && !b.contains("dealname"), "{p}: {b}");
    }
    assert_eq!(
        hs.lock().unwrap().count("/crm/v3/objects/"),
        0,
        "本体・検索は 1 回も呼ばない"
    );
    // 復旧すれば (失敗はキャッシュされていないので) すぐ読める
    hs.lock().unwrap().fail_all = None;
    let (s, b) = get_text(&app, "/api/crm/call-queue", &cookie).await;
    assert_eq!(s, StatusCode::OK, "{b}");
}

/// BPO の owner が HubSpot に無い → 何も読めない (全員分に倒さない)
#[tokio::test(flavor = "multi_thread")]
async fn owner_を引けない人は誰であれ何も読めない() {
    let mut hs = gate_hs();
    hs.owners.clear();
    let (client, hs) = start_hs(hs).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, BPO, "google_oidc").await;
    for p in [
        "/api/crm/deals/1",
        "/api/crm/contacts/101",
        "/api/crm/companies/201",
    ] {
        let (s, b) = get_text(&app, p, &cookie).await;
        assert_eq!(
            (s, b.as_str()),
            (StatusCode::FORBIDDEN, r#"{"error_kind":"owner_not_found"}"#),
            "{p}"
        );
    }
    assert_eq!(
        hs.lock().unwrap().count("/crm/v3/objects/"),
        0,
        "本体は 1 回も読まない"
    );
}

/// 関門の HubSpot 読み取りが失敗したら通さない (権限を確かめられないまま本文を返さない)。上流の本文も漏らさない
#[tokio::test(flavor = "multi_thread")]
async fn 関門の読み取りが失敗したら通さず_上流の本文も返さない() {
    let (client, hs) = start_hs(gate_hs()).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, BPO, "google_oidc").await;
    // 先に owner をキャッシュさせる (成功する 1 回)
    let (s, _) = get_text(&app, "/api/crm/deals/1", &cookie).await;
    assert_eq!(s, StatusCode::OK);
    hs.lock().unwrap().fail_all = Some(500);
    for p in [
        "/api/crm/deals/1",
        "/api/crm/contacts/101",
        "/api/crm/companies/201",
    ] {
        let (s, b) = get_text(&app, p, &cookie).await;
        assert!(s.is_server_error(), "{p}: {s} {b}");
        assert!(!b.contains(SECRET), "{p}: {b}");
        assert!(!b.contains("dealname"), "{p}: {b}");
    }
}

/// 不正な id は関門より前の 400 (HubSpot を呼ばない)
#[tokio::test(flavor = "multi_thread")]
async fn bpo_の不正_id_は_400_で_hubspot_を呼ばない() {
    let (client, hs) = start_hs(gate_hs()).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    let cookie = login(&app, BPO, "google_oidc").await;
    let (s, b) = get_text(&app, "/api/crm/deals/abc", &cookie).await;
    assert_eq!(
        (s, kind(&b)),
        (StatusCode::BAD_REQUEST, "invalid_id".into())
    );
    assert!(hs.lock().unwrap().log.is_empty());
}

/// 管理者のキューは全員分 (Search に担当者の絞り込みを入れない)、管理者以外は自分の owner で絞る
#[tokio::test(flavor = "multi_thread")]
async fn 管理者のキューは全員分_それ以外は自分の_owner_で絞る() {
    let (client, hs) = start_hs(gate_hs()).await;
    let app = app_with(state_with(None, Some(client), &[ADMIN]), role_access());
    for (email, owner_filter) in [(ADMIN, false), (BPO, true)] {
        hs.lock().unwrap().log.clear();
        let cookie = login(&app, email, "google_oidc").await;
        let (s, b) = get_text(&app, "/api/crm/call-queue", &cookie).await;
        assert_eq!(s, StatusCode::OK, "{email}: {b}");
        let v: Value = serde_json::from_str(&b).unwrap();
        assert_eq!(
            v["scope"]["role"],
            if owner_filter { "own" } else { "admin" }
        );
        assert_eq!(v["scope"]["owner"], if owner_filter { "me" } else { "all" });
        let searches: Vec<String> = hs
            .lock()
            .unwrap()
            .log
            .iter()
            .filter(|l| l.starts_with("POST /crm/v3/objects/deals/search"))
            .cloned()
            .collect();
        assert!(!searches.is_empty(), "{email}");
        for q in &searches {
            assert_eq!(
                q.contains(&format!(r#""value":"{BPO_OWNER}""#)),
                owner_filter,
                "{email}: {q}"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// 4. 純粋な判定 (deal_in_queue) の境界
// ---------------------------------------------------------------------------

#[test]
fn deal_in_queue_の境界() {
    use super::call_queue::{deal_in_queue, jst_today_ms};
    use crate::hubspot::HubSpotRecord;
    let today = jst_today_ms(Utc.with_ymd_and_hms(2026, 10, 5, 3, 0, 0).unwrap());
    let rec = |props: &[(&str, &str)]| HubSpotRecord {
        id: "1".into(),
        properties: props
            .iter()
            .map(|(k, v)| (k.to_string(), Some(v.to_string())))
            .collect(),
        created_at: None,
        updated_at: None,
        archived: false,
    };
    let base = |stage: &str, extra: &[(&str, &str)]| {
        let mut p = vec![
            ("pipeline", PIPELINE),
            ("dealstage", stage),
            ("hubspot_owner_id", "111"),
        ];
        p.extend_from_slice(extra);
        rec(&p)
    };
    assert!(deal_in_queue(&base(UNPROCESSED, &[]), "111", today));
    // 未済は次回架電日が未来でも出る
    assert!(deal_in_queue(
        &base(UNPROCESSED, &[("bpo_13", "2030-01-01")]),
        "111",
        today
    ));
    // 他のステージは次回日が今日以前だけ。日付の形 (ms / RFC 3339) も受ける
    assert!(deal_in_queue(
        &base(FUZAI, &[("bpo_13", "2026-10-05")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base(FUZAI, &[("bpo_13", "2026-10-06")]),
        "111",
        today
    ));
    assert!(deal_in_queue(
        &base(FUZAI, &[("bpo_13", &today.to_string())]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base(FUZAI, &[("bpo_13", &(today + 1).to_string())]),
        "111",
        today
    ));
    assert!(deal_in_queue(
        &base(FUZAI, &[("bpo_13", "2026-10-01T00:00:00Z")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(&base(FUZAI, &[]), "111", today));
    assert!(!deal_in_queue(
        &base(FUZAI, &[("bpo_13", "")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base(FUZAI, &[("bpo_13", "yesterday")]),
        "111",
        today
    ));
    // 担当者 (完全一致。空・前後空白付きの引数は扱いに注意)
    assert!(!deal_in_queue(&base(UNPROCESSED, &[]), "112", today));
    assert!(!deal_in_queue(&base(UNPROCESSED, &[]), "", today));
    assert!(!deal_in_queue(&base(UNPROCESSED, &[]), "  ", today));
    assert!(!deal_in_queue(
        &rec(&[("pipeline", PIPELINE), ("dealstage", UNPROCESSED)]),
        "111",
        today
    ));
    // 停止系・除外ステージ・別パイプライン・アーカイブ
    assert!(!deal_in_queue(
        &base(UNPROCESSED, &[("bpo_3", "x")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base(UNPROCESSED, &[("bpo_4", "x")]),
        "111",
        today
    ));
    assert!(deal_in_queue(
        &base(UNPROCESSED, &[("bpo_10", "x")]),
        "111",
        today
    )); // 不通時チェックは残す
        // 空白だけの停止理由は「入っていない」扱い (キューの検索の NOT_HAS_PROPERTY と同じ側。nz が trim する)
    assert!(deal_in_queue(
        &base(UNPROCESSED, &[("bpo_3", "  ")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base(APPOINTED, &[("bpo_13", "2026-10-01")]),
        "111",
        today
    ));
    assert!(!deal_in_queue(
        &base("999", &[("bpo_13", "2026-10-01")]),
        "111",
        today
    ));
    let mut archived = base(UNPROCESSED, &[]);
    archived.archived = true;
    assert!(!deal_in_queue(&archived, "111", today));
    let mut wrong_pipeline = base(UNPROCESSED, &[]);
    wrong_pipeline
        .properties
        .insert("pipeline".into(), Some("1".into()));
    assert!(!deal_in_queue(&wrong_pipeline, "111", today));
}

// ---------------------------------------------------------------------------
// 5. 役割の変更 API (管理者の操作。偽 Turso のみ)
// ---------------------------------------------------------------------------

mod change_role {
    use super::*;
    use crate::handlers::admin::contract_tests as ct;

    async fn post_role(
        app: &Router,
        cookie: &str,
        account_id: &str,
        body: &str,
    ) -> (StatusCode, Value) {
        let resp = app
            .clone()
            .oneshot(
                Request::post(format!("/api/admin/users/{account_id}/role"))
                    .header(header::COOKIE, cookie)
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-requested-with", "fetch")
                    .body(Body::from(body.to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        let status = resp.status();
        let text = ct::body_string(resp).await;
        (
            status,
            serde_json::from_str(&text).unwrap_or(Value::String(text)),
        )
    }

    fn role_in_db(conn: &SharedConn, id: &str) -> String {
        conn.lock()
            .unwrap()
            .query_row("SELECT role FROM accounts WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .unwrap()
    }

    fn id_of(conn: &SharedConn, email: &str) -> String {
        conn.lock()
            .unwrap()
            .query_row("SELECT id FROM accounts WHERE email = ?1", [email], |r| {
                r.get(0)
            })
            .unwrap()
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn 管理者は他人の役割を変えられ_記録され_キャッシュが捨てられる() {
        let (audit, conn) = start_sqlite_audit().await;
        ct::seed_fixtures(&conn);
        let state = ct::test_state(Some(audit));
        let app = crate::build_app(state);
        let cookie = ct::login(&app, ct::ADMIN_EMAIL).await;

        // 花子 (acc-0001, user) を bpo に。前後の空白・大文字小文字は吸収
        let (s, v) = post_role(&app, &cookie, "acc-0001", r#"{"role":" BPO "}"#).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(v["previous_role"], "user");
        assert_eq!(v["account"]["role"], "bpo");
        assert_eq!(v["account"]["email"], "hanako@f-a-c.co.jp");
        assert_eq!(role_in_db(&conn, "acc-0001"), "bpo");
        // 4 つ全部に変えられる
        for r in ["consultant", "admin", "user", "bpo"] {
            let (s, v) =
                post_role(&app, &cookie, "acc-0001", &format!(r#"{{"role":"{r}"}}"#)).await;
            assert_eq!(
                (s, v["account"]["role"].as_str()),
                (StatusCode::OK, Some(r))
            );
            assert_eq!(role_in_db(&conn, "acc-0001"), r);
        }
        // 操作記録 (非同期の書き込みなので少し待つ)。変更前後が meta に残る
        let mut found = String::new();
        for _ in 0..50 {
            found = conn
                .lock()
                .unwrap()
                .query_row(
                    "SELECT meta FROM activity_logs WHERE event_type = 'change_role' \
                     AND target_id = 'acc-0001' AND meta LIKE '%\"from\":\"user\"%' LIMIT 1",
                    [],
                    |r| r.get::<_, String>(0),
                )
                .unwrap_or_default();
            if !found.is_empty() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(40)).await;
        }
        let meta: Value = serde_json::from_str(&found).expect("meta が JSON");
        assert_eq!(meta["email"], "hanako@f-a-c.co.jp");
        assert_eq!(meta["from"], "user");
        assert_eq!(meta["to"], "bpo");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn 拒否の表_不正値_自分_環境変数の管理者_存在しない_非管理者() {
        let (audit, conn) = start_sqlite_audit().await;
        ct::seed_fixtures(&conn);
        // 別の管理者 (DB 上の admin。ADMIN_EMAILS ではない) を用意
        add_account(&conn, "acc-boss", "boss@f-a-c.co.jp", "admin");
        let app = crate::build_app(ct::test_state(Some(audit)));
        let admin_cookie = ct::login(&app, ct::ADMIN_EMAIL).await; // ADMIN_EMAILS の人 (ログインで行が作られる)
        let env_admin_id = id_of(&conn, ct::ADMIN_EMAIL);

        // 不正な役割値 → 400 (行は変わらない)
        for body in [
            r#"{"role":"superuser"}"#,
            r#"{"role":""}"#,
            r#"{"role":"admin,bpo"}"#,
            r#"{"role":"ａｄｍｉｎ"}"#,
        ] {
            let (s, v) = post_role(&app, &admin_cookie, "acc-0001", body).await;
            assert_eq!(
                (s, v["error_kind"].as_str()),
                (StatusCode::BAD_REQUEST, Some("invalid_role")),
                "{body}"
            );
        }
        assert_eq!(role_in_db(&conn, "acc-0001"), "user");
        // JSON でない / role が無い → 4xx (変更されない)
        let (s, _) = post_role(&app, &admin_cookie, "acc-0001", r#"{"x":1}"#).await;
        assert!(s.is_client_error());
        // 自分自身 → 403
        let (s, v) = post_role(&app, &admin_cookie, &env_admin_id, r#"{"role":"user"}"#).await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::FORBIDDEN, Some("cannot_change_self"))
        );
        assert_eq!(role_in_db(&conn, &env_admin_id), "admin");
        // 存在しない → 404
        let (s, v) = post_role(&app, &admin_cookie, "no-such", r#"{"role":"bpo"}"#).await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::NOT_FOUND, Some("account_not_found"))
        );
        // 別の管理者が ADMIN_EMAILS の人を降格しようとする → 409 (次のログインで admin に戻るため)
        let boss_cookie = ct::login(&app, "boss@f-a-c.co.jp").await;
        let (s, v) = post_role(&app, &boss_cookie, &env_admin_id, r#"{"role":"bpo"}"#).await;
        assert_eq!(
            (s, v["error_kind"].as_str()),
            (StatusCode::CONFLICT, Some("env_admin"))
        );
        assert_eq!(role_in_db(&conn, &env_admin_id), "admin");
        // admin のまま (昇格・据え置き) は通る
        let (s, _) = post_role(&app, &boss_cookie, &env_admin_id, r#"{"role":"admin"}"#).await;
        assert_eq!(s, StatusCode::OK);
        // 非管理者 (花子 = user) → 403 で何も変わらない
        let user_cookie = ct::login(&app, ct::USER_EMAIL).await;
        let (s, _) = post_role(&app, &user_cookie, "acc-0002", r#"{"role":"admin"}"#).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
        assert_eq!(role_in_db(&conn, "acc-0002"), "user");
        // 未ログイン → 303 /login (変更されない)
        let resp = app
            .clone()
            .oneshot(
                Request::post("/api/admin/users/acc-0002/role")
                    .header(header::CONTENT_TYPE, "application/json")
                    .header("x-requested-with", "fetch")
                    .body(Body::from(r#"{"role":"admin"}"#))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert!(
            resp.status() == StatusCode::SEE_OTHER || resp.status() == StatusCode::UNAUTHORIZED
        );
        assert_eq!(role_in_db(&conn, "acc-0002"), "user");
        // 監査 DB 未接続 → 403 (管理者を確かめられない)
        let app2 = crate::build_app(ct::test_state(None));
        let c2 = ct::login(&app2, ct::ADMIN_EMAIL).await;
        let (s, _) = post_role(&app2, &c2, "acc-0001", r#"{"role":"bpo"}"#).await;
        assert_eq!(s, StatusCode::FORBIDDEN);
    }

    /// 変更の直後に CRM の判定が変わる (同じプロセスのキャッシュを捨てる配線の実証)。
    /// CRM ルーターには本番と同じプロセス共通のキャッシュ (`global_role_cache`) を使わせる
    #[tokio::test(flavor = "multi_thread")]
    async fn 役割の変更は同じプロセスで即時に_crm_の判定へ反映される() {
        let (audit, conn) = start_sqlite_audit().await;
        add_account(&conn, "w1", "wire.test@f-a-c.co.jp", "bpo");
        add_account(&conn, "w2", "wire.boss@f-a-c.co.jp", "admin");
        let state = ct::test_state(Some(audit));
        let app = Router::new()
            .merge(super::super::router(
                CrmAccess::from_list("").with_global_role_cache(),
            ))
            .route(
                "/api/admin/users/{account_id}/role",
                post(crate::handlers::admin::api_change_role),
            )
            .route("/__test/session", post(inject_session))
            .with_state(state)
            .layer(SessionManagerLayer::new(MemoryStore::default()));
        let target = login(&app, "wire.test@f-a-c.co.jp", "google_oidc").await;
        let boss =
            login_with_account(&app, "wire.boss@f-a-c.co.jp", "google_oidc", Some("w2")).await;
        // 管理者だけの担当者一覧で確かめる (admin = 認可と役割を抜けて HubSpot 未設定の 503、それ以外 = 403)
        let probe = || async {
            let (s, b) = get_text(&app, "/api/crm/owners", &target).await;
            (s, kind(&b))
        };
        let passed = (
            StatusCode::SERVICE_UNAVAILABLE,
            "not_configured".to_string(),
        );
        let denied = (StatusCode::FORBIDDEN, "forbidden".to_string());
        assert_eq!(
            probe().await,
            denied,
            "bpo の値は管理者ではない (ここで役割がキャッシュされる)"
        );
        // DB を直接 admin に変えても、キャッシュ (5 分) が効いている間は変わらない = 逆証明
        conn.lock()
            .unwrap()
            .execute("UPDATE accounts SET role = 'admin' WHERE id = 'w1'", [])
            .unwrap();
        assert_eq!(probe().await, denied, "キャッシュ中は DB の変更が見えない");
        // 管理画面の API で変更すると、同じプロセスでは次のリクエストから効く
        let (s, v) = post_role(&app, &boss, "w1", r#"{"role":"admin"}"#).await;
        assert_eq!(s, StatusCode::OK, "{v}");
        assert_eq!(probe().await, passed, "変更直後に管理者として通る");
        // 降格も即時
        let (s, _) = post_role(&app, &boss, "w1", r#"{"role":"user"}"#).await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(probe().await, denied);
    }
}
