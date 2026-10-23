//! ログインの結合テスト (Google OIDC コールバック / パスワードログイン / disabled_at)。
//!
//! 偽の Google (token endpoint + JWKS) と偽の Turso (監査 DB の HTTP Pipeline API) を
//! 127.0.0.1 のランダムポートに axum で立てる。新しい dev-dependency は使わない。
//!
//! 偽 Turso は SQL を実行しない (SQL の文字列で応答を切り替えるだけ)。
//! `is_email_disabled` の SQL そのものは末尾の rusqlite テストで本物の SQLite に流して確かめる。

use std::sync::{Arc, Mutex};

use axum::{
    body::Body,
    extract::State,
    http::{header, Request, StatusCode},
    routing::{get, post},
    Json, Router,
};
use serde_json::{json, Value};
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

use crate::audit::AuditDb;
use crate::auth::google_oidc::tests::{sign, CLIENT_ID, JWKS_JSON, KEY_1_PEM};
use crate::auth::google_oidc::{Endpoints, GoogleOidc, TX_COOKIE_NAME};
use crate::config::{AppConfig, ExternalPassword, GoogleOidcConfig};
use crate::db::cache::AppCache;
use crate::db::turso_http::TursoDb;
use crate::AppState;

const CLIENT_SECRET: &str = "test-client-secret";
const REDIRECT: &str = "http://localhost:9216/auth/google/callback";

async fn spawn(router: Router) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

// ---------------------------------------------------------------------------
// 偽 Google
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeGoogle {
    /// 次に発行する ID token に入れる値
    nonce: String,
    email: String,
    hd: String,
    /// token endpoint が受け取った form (key=value の組)
    token_requests: Vec<Vec<(String, String)>>,
}

type Shared<T> = Arc<Mutex<T>>;

async fn fake_token(State(st): State<Shared<FakeGoogle>>, body: String) -> Json<Value> {
    let form: Vec<(String, String)> = reqwest::Url::parse(&format!("http://x/?{body}"))
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    let mut g = st.lock().unwrap();
    g.token_requests.push(form);
    let now = chrono::Utc::now().timestamp();
    let claims = json!({
        "iss": "https://accounts.google.com",
        "aud": CLIENT_ID,
        "sub": "1234567890",
        "email": g.email,
        "email_verified": true,
        "hd": g.hd,
        "nonce": g.nonce,
        "iat": now,
        "exp": now + 3600,
    });
    let id_token = sign(&claims, KEY_1_PEM, "test-key-1");
    Json(json!({"id_token": id_token, "access_token": "x", "token_type": "Bearer"}))
}

async fn fake_jwks() -> ([(header::HeaderName, &'static str); 1], &'static str) {
    ([(header::CONTENT_TYPE, "application/json")], JWKS_JSON)
}

async fn start_fake_google(email: &str, hd: &str) -> (Arc<GoogleOidc>, Shared<FakeGoogle>) {
    let st: Shared<FakeGoogle> = Arc::new(Mutex::new(FakeGoogle {
        email: email.to_string(),
        hd: hd.to_string(),
        ..Default::default()
    }));
    let base = spawn(
        Router::new()
            .route("/token", post(fake_token))
            .route("/jwks", get(fake_jwks))
            .with_state(st.clone()),
    )
    .await;
    let cfg = GoogleOidcConfig::from_values(
        Some(CLIENT_ID.to_string()),
        Some(CLIENT_SECRET.to_string()),
        Some(REDIRECT.to_string()),
        Some("f-a-c.co.jp".to_string()),
    )
    .unwrap();
    let oidc = GoogleOidc::with_endpoints(
        cfg,
        Endpoints {
            issuer: "https://accounts.google.com".to_string(),
            authorization_endpoint: format!("{base}/authorize"),
            token_endpoint: format!("{base}/token"),
            jwks_uri: format!("{base}/jwks"),
        },
    );
    (Arc::new(oidc), st)
}

// ---------------------------------------------------------------------------
// 偽 Turso (監査 DB)
// ---------------------------------------------------------------------------

#[derive(Default)]
struct FakeTurso {
    /// disabled_at が入っている email (小文字)
    disabled: Vec<String>,
    /// 受け取った (sql, args の value 列)
    calls: Vec<(String, Vec<String>)>,
}

async fn fake_pipeline(
    State(st): State<Shared<FakeTurso>>,
    Json(body): Json<Value>,
) -> Json<Value> {
    let stmt = &body["requests"][0]["stmt"];
    let sql = stmt["sql"].as_str().unwrap_or("").to_string();
    let args: Vec<String> = stmt["args"]
        .as_array()
        .map(|a| {
            a.iter()
                .map(|v| v["value"].as_str().unwrap_or("").to_string())
                .collect()
        })
        .unwrap_or_default();
    let mut t = st.lock().unwrap();
    t.calls.push((sql.clone(), args.clone()));
    let result = if sql.starts_with("SELECT 1") {
        json!({"cols": [{"name": "1"}], "rows": [[{"type": "integer", "value": "1"}]]})
    } else if sql.starts_with("SELECT disabled_at FROM accounts") {
        let email = args.first().cloned().unwrap_or_default().to_lowercase();
        let rows = if t.disabled.contains(&email) {
            json!([[{"type": "text", "value": "2026-09-01T00:00:00Z"}]])
        } else {
            json!([])
        };
        json!({"cols": [{"name": "disabled_at"}], "rows": rows})
    } else if sql.starts_with("SELECT id, role FROM accounts") {
        // 未登録 → upsert_account は INSERT に進む
        json!({"cols": [{"name": "id"}, {"name": "role"}], "rows": []})
    } else {
        json!({"cols": [], "rows": [], "affected_row_count": 1})
    };
    Json(json!({"results": [
        {"type": "ok", "response": {"type": "execute", "result": result}},
        {"type": "ok", "response": {"type": "close"}}
    ]}))
}

async fn start_fake_audit(disabled: &[&str]) -> (AuditDb, Shared<FakeTurso>) {
    let st: Shared<FakeTurso> = Arc::new(Mutex::new(FakeTurso {
        disabled: disabled.iter().map(|s| s.to_string()).collect(),
        ..Default::default()
    }));
    let base = spawn(
        Router::new()
            .route("/v2/pipeline", post(fake_pipeline))
            .with_state(st.clone()),
    )
    .await;
    // TursoDb は reqwest::blocking なので spawn_blocking で作る
    let turso = tokio::task::spawn_blocking(move || TursoDb::new(&base, "test-token"))
        .await
        .unwrap()
        .expect("fake turso");
    (AuditDb::new(turso, "salt".to_string()), st)
}

/// login_sessions への INSERT の (login_method, success 相当の SQL 種別, failure_reason)
fn login_session_inserts(t: &Shared<FakeTurso>) -> Vec<(String, Vec<String>)> {
    t.lock()
        .unwrap()
        .calls
        .iter()
        .filter(|(sql, _)| sql.contains("INSERT INTO login_sessions"))
        .cloned()
        .collect()
}

// ---------------------------------------------------------------------------
// AppState / アプリ
// ---------------------------------------------------------------------------

fn test_config() -> AppConfig {
    AppConfig {
        port: 0,
        auth_password: "internal-pass".to_string(),
        auth_password_hash: String::new(),
        external_passwords: vec![ExternalPassword {
            password: "external-pass".to_string(),
            expires: "2099-12-31".to_string(),
        }],
        allowed_domains: vec!["f-a-c.co.jp".to_string()],
        allowed_domains_extra: vec!["client.example".to_string()],
        hellowork_db_path: String::new(),
        hellowork_db_enabled: false,
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

fn test_state(oidc: Option<Arc<GoogleOidc>>, audit: Option<AuditDb>) -> Arc<AppState> {
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
        google_oidc: oidc,
        crm_hubspot: None,
        hubspot: None,
    })
}

async fn whoami(session: Session) -> Json<Value> {
    let email: Option<String> = session.get(crate::auth::SESSION_USER_KEY).await.unwrap();
    let method: Option<String> = session
        .get(crate::auth::SESSION_LOGIN_METHOD_KEY)
        .await
        .unwrap();
    Json(json!({"user_email": email, "login_method": method}))
}

/// OIDC のルート + セッションの中身を見る /whoami だけのアプリ
fn oidc_app(state: Arc<AppState>) -> Router {
    Router::new()
        .merge(crate::auth::google_oidc::router())
        .route("/whoami", get(whoami))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

fn set_cookies(resp: &axum::response::Response) -> Vec<String> {
    resp.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect()
}

/// Set-Cookie から `name=value` 部分を取り出す
fn cookie_pair(resp: &axum::response::Response, name: &str) -> Option<String> {
    set_cookies(resp)
        .into_iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(|c| c.split(';').next().unwrap().to_string())
}

async fn body_string(resp: axum::response::Response) -> String {
    let bytes = http_body_util::BodyExt::collect(resp.into_body())
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

async fn get_req(app: &Router, uri: &str, cookie: Option<&str>) -> axum::response::Response {
    let mut b = Request::builder().uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    app.clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap()
}

/// /auth/google/login を叩き、(tx Cookie の name=value, 認可 URL の query) を返す
async fn start_login(app: &Router) -> (String, Vec<(String, String)>) {
    let resp = get_req(app, "/auth/google/login", None).await;
    assert_eq!(resp.status(), StatusCode::SEE_OTHER);
    let location = resp.headers()[header::LOCATION]
        .to_str()
        .unwrap()
        .to_string();
    let tx_cookie = cookie_pair(&resp, TX_COOKIE_NAME).expect("tx cookie");
    let query = reqwest::Url::parse(&location)
        .unwrap()
        .query_pairs()
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    (tx_cookie, query)
}

fn param<'a>(q: &'a [(String, String)], key: &str) -> &'a str {
    q.iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
        .unwrap_or_else(|| panic!("{key} が無い: {q:?}"))
}

/// callback 応答の Set-Cookie から session Cookie を取り出して /whoami を叩く
async fn whoami_after(app: &Router, resp: &axum::response::Response) -> Value {
    let session_cookie = cookie_pair(resp, "id");
    let who = get_req(app, "/whoami", session_cookie.as_deref()).await;
    serde_json::from_str(&body_string(who).await).unwrap()
}

// ---------------------------------------------------------------------------
// OIDC
// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn oidc_コールバックでセッションに_email_と_login_method_が入る() {
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let (audit, turso) = start_fake_audit(&[]).await;
    let app = oidc_app(test_state(Some(oidc), Some(audit)));

    // 1) 開始: 認可 URL と tx Cookie
    let start = get_req(&app, "/auth/google/login", None).await;
    let tx_set_cookie = set_cookies(&start)
        .into_iter()
        .find(|c| c.starts_with("hrhr_oidc_tx="))
        .unwrap();
    assert!(tx_set_cookie.contains("HttpOnly"), "{tx_set_cookie}");
    assert!(tx_set_cookie.contains("SameSite=Lax"), "{tx_set_cookie}");
    assert!(
        tx_set_cookie.contains("Path=/auth/google"),
        "{tx_set_cookie}"
    );
    assert!(tx_set_cookie.contains("Max-Age=300"), "{tx_set_cookie}");
    let (tx_cookie, q) = start_login(&app).await;
    assert_eq!(param(&q, "client_id"), CLIENT_ID);
    assert_eq!(param(&q, "redirect_uri"), REDIRECT);
    assert_eq!(param(&q, "hd"), "f-a-c.co.jp");
    assert_eq!(param(&q, "scope"), "openid email");
    assert_eq!(param(&q, "code_challenge_method"), "S256");
    let state_param = param(&q, "state").to_string();
    let challenge = param(&q, "code_challenge").to_string();
    google.lock().unwrap().nonce = param(&q, "nonce").to_string();

    // 2) コールバック
    let resp = get_req(
        &app,
        &format!("/auth/google/callback?code=test-code&state={state_param}"),
        Some(&tx_cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::OK);
    let cleared = set_cookies(&resp)
        .into_iter()
        .find(|c| c.starts_with("hrhr_oidc_tx="))
        .expect("tx cookie を消す Set-Cookie");
    assert!(cleared.contains("Max-Age=0"), "{cleared}");
    let who = whoami_after(&app, &resp).await;
    let body = body_string(resp).await;
    assert!(body.contains(r#"location.replace("/")"#), "{body}");
    assert!(body.contains(r#"content="0;url=/""#), "{body}");

    // 3) セッションの中身
    assert_eq!(who["user_email"], "taro@f-a-c.co.jp");
    assert_eq!(who["login_method"], "google_oidc");

    // 4) token endpoint に送った値 (client secret と、challenge に対応する verifier)
    let form = google.lock().unwrap().token_requests[0].clone();
    let get = |k: &str| form.iter().find(|(a, _)| a == k).unwrap().1.clone();
    assert_eq!(get("code"), "test-code");
    assert_eq!(get("client_secret"), CLIENT_SECRET);
    assert_eq!(get("grant_type"), "authorization_code");
    assert_eq!(get("redirect_uri"), REDIRECT);
    let verifier_tx = crate::auth::google_oidc::LoginTx {
        state: String::new(),
        nonce: String::new(),
        verifier: get("code_verifier"),
    };
    assert_eq!(verifier_tx.code_challenge(), challenge);

    // 5) 監査: login_sessions の login_method が実値
    let inserts = login_session_inserts(&turso);
    assert_eq!(inserts.len(), 1, "{inserts:?}");
    assert!(
        inserts[0].1.contains(&"google_oidc".to_string()),
        "{inserts:?}"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_state_不一致は_400_でセッションを作らない() {
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let app = oidc_app(test_state(Some(oidc), None));
    let (tx_cookie, q) = start_login(&app).await;
    google.lock().unwrap().nonce = param(&q, "nonce").to_string();

    let resp = get_req(
        &app,
        "/auth/google/callback?code=test-code&state=wrong-state",
        Some(&tx_cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let who = whoami_after(&app, &resp).await;
    assert_eq!(who["user_email"], Value::Null);
    // token endpoint は呼ばれていない
    assert!(google.lock().unwrap().token_requests.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_cookie_なしは_400() {
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let app = oidc_app(test_state(Some(oidc), None));
    let (_tx_cookie, q) = start_login(&app).await;
    let state_param = param(&q, "state").to_string();

    let resp = get_req(
        &app,
        &format!("/auth/google/callback?code=test-code&state={state_param}"),
        None,
    )
    .await;
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let who = whoami_after(&app, &resp).await;
    assert_eq!(who["user_email"], Value::Null);
    assert!(google.lock().unwrap().token_requests.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_hd_が_gmail_の_token_は_403_でセッションを作らない() {
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "gmail.com").await;
    let (audit, turso) = start_fake_audit(&[]).await;
    let app = oidc_app(test_state(Some(oidc), Some(audit)));
    let (tx_cookie, q) = start_login(&app).await;
    let state_param = param(&q, "state").to_string();
    google.lock().unwrap().nonce = param(&q, "nonce").to_string();

    let resp = get_req(
        &app,
        &format!("/auth/google/callback?code=test-code&state={state_param}"),
        Some(&tx_cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let who = whoami_after(&app, &resp).await;
    assert_eq!(who["user_email"], Value::Null);
    let inserts = login_session_inserts(&turso);
    assert_eq!(inserts.len(), 1, "{inserts:?}");
    assert!(inserts[0].1.contains(&"oidc_hosted_domain".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_disabled_at_のアカウントは_403() {
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let (audit, turso) = start_fake_audit(&["taro@f-a-c.co.jp"]).await;
    let app = oidc_app(test_state(Some(oidc), Some(audit)));
    let (tx_cookie, q) = start_login(&app).await;
    let state_param = param(&q, "state").to_string();
    google.lock().unwrap().nonce = param(&q, "nonce").to_string();

    let resp = get_req(
        &app,
        &format!("/auth/google/callback?code=test-code&state={state_param}"),
        Some(&tx_cookie),
    )
    .await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    let who = whoami_after(&app, &resp).await;
    assert_eq!(who["user_email"], Value::Null);
    let body_calls = turso.lock().unwrap().calls.clone();
    assert!(
        !body_calls
            .iter()
            .any(|(sql, _)| sql.contains("INSERT INTO accounts")),
        "無効アカウントで accounts を触らない"
    );
    let inserts = login_session_inserts(&turso);
    assert_eq!(inserts.len(), 1, "{inserts:?}");
    assert!(inserts[0].1.contains(&"account_disabled".to_string()));
    assert!(inserts[0].1.contains(&"google_oidc".to_string()));
}

// ---------------------------------------------------------------------------
// OIDC 未設定時 (回帰)
// ---------------------------------------------------------------------------

#[tokio::test]
async fn oidc_未設定ならボタンを出さず_auth_google_は_404() {
    let app = crate::build_app(test_state(None, None));
    let page = get_req(&app, "/login", None).await;
    assert_eq!(page.status(), StatusCode::OK);
    let html = body_string(page).await;
    assert!(
        html.contains(r#"action="/login""#),
        "パスワードフォームは残る"
    );
    assert!(!html.contains("/auth/google/login"), "ボタンが出ている");
    assert!(!html.contains("{{GOOGLE_LOGIN_HTML}}"), "置換漏れ");

    let start = get_req(&app, "/auth/google/login", None).await;
    assert_eq!(start.status(), StatusCode::NOT_FOUND);
    let cb = get_req(&app, "/auth/google/callback?code=x&state=y", None).await;
    assert_eq!(cb.status(), StatusCode::NOT_FOUND);
}

#[tokio::test(flavor = "multi_thread")]
async fn oidc_設定済みならログイン画面にボタンが出る() {
    let (oidc, _google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let app = crate::build_app(test_state(Some(oidc), None));
    let html = body_string(get_req(&app, "/login", None).await).await;
    assert!(
        html.contains(r#"href="/auth/google/login""#),
        "ボタンが無い"
    );
    assert!(html.contains("@f-a-c.co.jp"));
    // 未ログインでも auth_middleware に止められず、開始できる
    let start = get_req(&app, "/auth/google/login", None).await;
    assert_eq!(start.status(), StatusCode::SEE_OTHER);
}

// ---------------------------------------------------------------------------
// パスワードログイン (login_method の実値 / disabled_at)
// ---------------------------------------------------------------------------

async fn post_login(app: &Router, email: &str, password: &str) -> axum::response::Response {
    let body = format!(
        "email={}&password={}",
        urlencoding::encode(email),
        urlencoding::encode(password)
    );
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(body))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn パスワードログインの_login_method_は社内と外部で分かれる() {
    let (audit, turso) = start_fake_audit(&[]).await;
    let app = crate::build_app(test_state(None, Some(audit)));

    let r1 = post_login(&app, "hanako@f-a-c.co.jp", "internal-pass").await;
    assert_eq!(r1.status(), StatusCode::SEE_OTHER);
    assert_eq!(r1.headers()[header::LOCATION], "/");
    let r2 = post_login(&app, "guest@client.example", "external-pass").await;
    assert_eq!(r2.status(), StatusCode::SEE_OTHER);
    let r3 = post_login(&app, "hanako@f-a-c.co.jp", "wrong").await;
    assert_eq!(
        r3.status(),
        StatusCode::OK,
        "失敗時は従来どおりログイン画面"
    );

    let inserts = login_session_inserts(&turso);
    assert_eq!(inserts.len(), 3, "{inserts:?}");
    assert!(inserts[0].1.contains(&"password_internal".to_string()));
    assert!(inserts[1].1.contains(&"password_external".to_string()));
    assert!(inserts[2].1.contains(&"password".to_string()));
    assert!(inserts[2].1.contains(&"wrong_password".to_string()));
}

#[tokio::test(flavor = "multi_thread")]
async fn パスワードログインでも_disabled_at_のアカウントは拒否() {
    let (audit, turso) = start_fake_audit(&["jiro@f-a-c.co.jp"]).await;
    let app = crate::build_app(test_state(None, Some(audit)));

    // 入力の大文字小文字が違っても無効化は効く
    let resp = post_login(&app, "Jiro@F-A-C.co.jp", "internal-pass").await;
    assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    assert!(resp.headers().get(header::LOCATION).is_none());
    let html = body_string(resp).await;
    assert!(html.contains("無効化されています"), "{html}");

    let inserts = login_session_inserts(&turso);
    assert_eq!(inserts.len(), 1, "{inserts:?}");
    assert!(inserts[0].1.contains(&"account_disabled".to_string()));
    // 無効化されていない人は通る (対照)
    let ok = post_login(&app, "hanako@f-a-c.co.jp", "internal-pass").await;
    assert_eq!(ok.status(), StatusCode::SEE_OTHER);
}

// ---------------------------------------------------------------------------
// is_email_disabled の SQL を本物の SQLite で確かめる
// ---------------------------------------------------------------------------

#[test]
fn is_email_disabled_の_sql_は大文字小文字を無視して_disabled_at_を見る() {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE accounts (id TEXT PRIMARY KEY, email TEXT NOT NULL UNIQUE, disabled_at TEXT);
         INSERT INTO accounts VALUES ('1', 'Jiro@f-a-c.co.jp', '2026-09-01T00:00:00Z');
         INSERT INTO accounts VALUES ('2', 'hanako@f-a-c.co.jp', NULL);
         INSERT INTO accounts VALUES ('3', 'saburo@f-a-c.co.jp', '');",
    )
    .unwrap();
    let hit = |email: &str| -> bool {
        let mut st = conn
            .prepare(crate::audit::dao::IS_EMAIL_DISABLED_SQL)
            .unwrap();
        st.exists([email]).unwrap()
    };
    assert!(hit("jiro@f-a-c.co.jp"));
    assert!(hit("JIRO@F-A-C.CO.JP"));
    assert!(!hit("hanako@f-a-c.co.jp"));
    assert!(!hit("saburo@f-a-c.co.jp"));
    assert!(!hit("nobody@f-a-c.co.jp"));
}

// 認証 Cookie の復元は CRM のコードを変更せず、その読み取り・認可を通す。
async fn cookie_principal(session: Session) -> Json<Value> {
    let principal = crate::crm::rbac::load_principal(&session).await;
    let allowed =
        crate::crm::rbac::can_read(&principal, None, &crate::crm::rbac::CrmAccess::default())
            .is_ok();
    Json(
        json!({"email": principal.email, "method": principal.login_method, "crm_allowed": allowed,
        "temporary": session.get::<String>("uploaded_csv").await.unwrap()}),
    )
}

fn durable_app(state: Arc<AppState>, key: Option<tower_sessions::cookie::Key>) -> Router {
    let cookie_state = crate::auth::login_cookie::CookieState {
        app: state.clone(),
        key,
        secure: false,
        revoked: Arc::default(),
    };
    Router::new()
        .merge(crate::auth::google_oidc::router())
        .route("/login", post(crate::login_submit))
        .route("/logout", get(crate::logout))
        .route("/principal", get(cookie_principal))
        .route(
            "/api/nav",
            get(cookie_principal).route_layer(axum::middleware::from_fn(crate::auth::require_auth)),
        )
        .route(
            "/temporary",
            get(|session: Session| async move {
                session
                    .insert("uploaded_csv", "large temporary data")
                    .await
                    .unwrap();
                StatusCode::OK
            }),
        )
        .with_state(state)
        .layer(axum::middleware::from_fn_with_state(
            cookie_state,
            crate::auth::login_cookie::restore_login,
        ))
        .layer(
            SessionManagerLayer::new(MemoryStore::default())
                .with_secure(false)
                .with_expiry(tower_sessions::Expiry::OnInactivity(time::Duration::hours(
                    24,
                ))),
        )
}

async fn password_cookie(app: &Router, password: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(format!(
                    "email=taro%40f-a-c.co.jp&password={password}"
                )))
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn 同じ鍵の再起動でメールと方法を復元し一時データは消える() {
    use tower_sessions::cookie::Key;
    let key = Key::from(&[42; 64]);
    for password in ["internal-pass", "external-pass"] {
        let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
        let state = test_state(None, Some(audit));
        let app = durable_app(state.clone(), Some(key.clone()));
        let login = password_cookie(&app, password).await;
        assert_eq!(login.status(), StatusCode::SEE_OTHER);
        let cookie = cookie_pair(&login, crate::auth::login_cookie::COOKIE_NAME).unwrap();
        let memory = cookie_pair(&login, "id").unwrap();
        let combined = format!("{cookie}; {memory}");
        assert_eq!(
            get_req(&app, "/temporary", Some(&combined)).await.status(),
            StatusCode::OK
        );
        let reboot = durable_app(state, Some(key.clone()));
        let who: Value = serde_json::from_str(
            &body_string(get_req(&reboot, "/principal", Some(&combined)).await).await,
        )
        .unwrap();
        let method = if password == "internal-pass" {
            "password_internal"
        } else {
            "password_external"
        };
        assert_eq!(
            who,
            json!({"email":"taro@f-a-c.co.jp", "method":method, "crm_allowed":false, "temporary":null})
        );
        let set = set_cookies(&login)
            .into_iter()
            .find(|c| c.starts_with("hr_login="))
            .unwrap();
        assert!(
            set.contains("HttpOnly") && set.contains("SameSite=Lax") && set.contains("Expires=")
        );
        assert!(!set.contains("taro") && !set.contains(method));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn googleログインを再起動後にcrmが同じ方法で読める() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (oidc, google) = start_fake_google("taro@f-a-c.co.jp", "f-a-c.co.jp").await;
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(Some(oidc), Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let (tx, q) = start_login(&app).await;
    google.lock().unwrap().nonce = param(&q, "nonce").to_string();
    let login = get_req(
        &app,
        &format!(
            "/auth/google/callback?code=test-code&state={}",
            param(&q, "state")
        ),
        Some(&tx),
    )
    .await;
    assert_eq!(login.status(), StatusCode::OK);
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let reboot = durable_app(state, Some(key));
    let who: Value = serde_json::from_str(
        &body_string(get_req(&reboot, "/principal", Some(&cookie)).await).await,
    )
    .unwrap();
    assert_eq!(
        who,
        json!({"email":"taro@f-a-c.co.jp","method":"google_oidc","crm_allowed":true,"temporary":null})
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 別鍵と改ざんとログアウトと無効化と鍵なしは認証を復元しない() {
    use tower_sessions::cookie::Key;
    let key = Key::from(&[42; 64]);
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let memory = cookie_pair(&login, "id").unwrap();
    let combined = format!("{cookie}; {memory}");
    let wrong = durable_app(state.clone(), Some(Key::from(&[43; 64])));
    for (target, c) in [
        (&wrong, cookie.clone()),
        (&app, format!("{cookie}tampered; {memory}")),
        (&durable_app(state.clone(), None), combined.clone()),
    ] {
        let who: Value =
            serde_json::from_str(&body_string(get_req(target, "/principal", Some(&c)).await).await)
                .unwrap();
        assert_eq!(who["email"], Value::Null);
        assert!(!who["crm_allowed"].as_bool().unwrap());
    }
    // 元のメモリセッションは改ざんで破棄されている。ログアウトは復元した認証 Cookie も消す。
    let logout = get_req(&app, "/logout", Some(&cookie)).await;
    let cleared = set_cookies(&logout)
        .into_iter()
        .find(|c| c.starts_with("hr_login="))
        .unwrap();
    assert!(cleared.contains("Max-Age=0"));
    let who: Value =
        serde_json::from_str(&body_string(get_req(&app, "/principal", None).await).await).unwrap();
    assert_eq!(who["email"], Value::Null);
    // 保存した同じ Cookie を再送しても、ログアウト後は復元しない。
    for target in [&app, &durable_app(state.clone(), Some(key.clone()))] {
        let replay = get_json_req(target, "/api/nav", &combined).await;
        assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
    }
    let ended: i64 = conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT count(*) FROM login_sessions WHERE ended_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(ended, 1);
    let fresh = password_cookie(&app, "internal-pass").await;
    let cookie = cookie_pair(&fresh, "hr_login").unwrap();
    conn.lock()
        .unwrap()
        .execute(
            "UPDATE accounts SET disabled_at = '2026-10-11T00:00:00Z'",
            [],
        )
        .unwrap();
    let reboot = durable_app(state, Some(key));
    let rejected = get_req(&reboot, "/principal", Some(&cookie)).await;
    assert!(set_cookies(&rejected)
        .iter()
        .any(|c| c.starts_with("hr_login=") && c.contains("Max-Age=0")));
    let who: Value = serde_json::from_str(&body_string(rejected).await).unwrap();
    assert_eq!(who["email"], Value::Null);
    let without = durable_app(test_state(None, None), None);
    let login = password_cookie(&without, "internal-pass").await;
    assert!(cookie_pair(&login, "hr_login").is_none());
    let memory = cookie_pair(&login, "id").unwrap();
    let before: Value = serde_json::from_str(
        &body_string(get_req(&without, "/principal", Some(&memory)).await).await,
    )
    .unwrap();
    assert_eq!(before["email"], "taro@f-a-c.co.jp");
    let reboot = durable_app(test_state(None, None), None);
    let after: Value = serde_json::from_str(
        &body_string(get_req(&reboot, "/principal", Some(&memory)).await).await,
    )
    .unwrap();
    assert_eq!(after["email"], Value::Null);
}

#[tokio::test(flavor = "multi_thread")]
async fn 暗号が正しくても認証期限と社外パスワード期限が切れたcookieは拒否する() {
    use tower_sessions::cookie::{Cookie, CookieJar, Key};
    let key = Key::from(&[42; 64]);
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let memory = cookie_pair(&login, "id").unwrap();
    let now = time::OffsetDateTime::now_utc().unix_timestamp();
    for (password, expires, external) in [
        ("internal-pass", now - 1, None),
        ("external-pass", now + 86400, Some("2020-01-01")),
    ] {
        let original = password_cookie(&app, password).await;
        let mut jar = CookieJar::new();
        jar.add_original(
            Cookie::parse(cookie_pair(&original, "hr_login").unwrap())
                .unwrap()
                .into_owned(),
        );
        let mut login: Value =
            serde_json::from_str(jar.private(&key).get("hr_login").unwrap().value()).unwrap();
        login["expires"] = json!(expires);
        login["external_expires"] = json!(external);
        jar.private_mut(&key)
            .add(Cookie::new("hr_login", login.to_string()));
        let c = format!(
            "hr_login={}; {memory}",
            jar.get("hr_login").unwrap().value()
        );
        let reboot = durable_app(state.clone(), Some(key.clone()));
        let who: Value = serde_json::from_str(
            &body_string(get_req(&reboot, "/principal", Some(&c)).await).await,
        )
        .unwrap();
        assert_eq!(who["email"], Value::Null);
        assert_eq!(who["crm_allowed"], false);
    }
    // 期限切れ Cookie がブラウザから消えても、メモリ Cookie だけでは通さない。
    let who: Value =
        serde_json::from_str(&body_string(get_req(&app, "/principal", Some(&memory)).await).await)
            .unwrap();
    assert_eq!(who["email"], Value::Null);
    // 鍵なしのメモリ認証でも外部パスワードの期限を要求ごとに見る。
    let session = Session::new(None, Arc::new(MemoryStore::default()), None);
    session
        .insert(crate::auth::SESSION_USER_KEY, "taro@f-a-c.co.jp")
        .await
        .unwrap();
    session
        .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, "password_external")
        .await
        .unwrap();
    session
        .insert(crate::auth::login_cookie::EXTERNAL_EXPIRY_KEY, "2020-01-01")
        .await
        .unwrap();
    let state = crate::auth::login_cookie::CookieState {
        app: test_state(None, None),
        key: None,
        secure: false,
        revoked: Arc::default(),
    };
    let app = Router::new()
        .route("/principal", get(cookie_principal))
        .layer(axum::middleware::from_fn_with_state(
            state,
            crate::auth::login_cookie::restore_login,
        ))
        .layer(axum::Extension(session));
    let who: Value =
        serde_json::from_str(&body_string(get_req(&app, "/principal", None).await).await).unwrap();
    assert_eq!(who["email"], Value::Null);
}

async fn get_json_req(app: &Router, uri: &str, cookie: &str) -> axum::response::Response {
    app.clone()
        .oneshot(
            Request::builder()
                .uri(uri)
                .header(header::COOKIE, cookie)
                .header(header::ACCEPT, "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn 更新前後のcookieとメモリを再送してもログアウト失効は再起動をまたいで効く() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let first = cookie_pair(&login, "hr_login").unwrap();
    let memory = cookie_pair(&login, "id").unwrap();
    let updated = get_req(&app, "/temporary", Some(&format!("{first}; {memory}"))).await;
    let renewed = cookie_pair(&updated, "hr_login").unwrap();
    // 同じプロセス内でも Cookie だけの復元で別のメモリ状態が作られる。
    let restored_app = app.clone();
    let before = get_json_req(&restored_app, "/api/nav", &first).await;
    assert_eq!(before.status(), StatusCode::OK);
    let restored_memory = cookie_pair(&before, "id").unwrap();
    let logout = get_req(
        &restored_app,
        "/logout",
        Some(&format!("{renewed}; {restored_memory}")),
    )
    .await;
    assert_eq!(logout.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row(
                "SELECT count(*) FROM login_sessions WHERE ended_at IS NOT NULL",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        1
    );
    for target in [&app, &restored_app, &durable_app(state, Some(key))] {
        for cookies in [
            &first,
            &renewed,
            &format!("{first}; {memory}"),
            &format!("{renewed}; {restored_memory}"),
        ] {
            let replay = get_json_req(target, "/api/nav", cookies).await;
            assert_eq!(replay.status(), StatusCode::UNAUTHORIZED);
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 失効確認とログアウト書き込みの失敗を成功扱いせず回復後に再試行できる() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let memory = cookie_pair(&login, "id").unwrap();
    let combined = format!("{cookie}; {memory}");
    conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_logout BEFORE UPDATE OF ended_at ON login_sessions BEGIN SELECT RAISE(FAIL, 'mock write failure'); END;").unwrap();
    let failed = get_req(&app, "/logout", Some(&combined)).await;
    assert_eq!(failed.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert!(cookie_pair(&failed, "hr_login").is_none());
    assert_eq!(
        get_json_req(&app, "/api/nav", &combined).await.status(),
        StatusCode::OK
    );
    conn.lock().unwrap().execute_batch("DROP TRIGGER fail_logout; ALTER TABLE login_sessions RENAME TO unavailable_login_sessions;").unwrap();
    let reboot = durable_app(state, Some(key));
    assert_eq!(
        get_json_req(&app, "/api/nav", &combined).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_json_req(&reboot, "/api/nav", &combined).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    for target in [&app, &reboot] {
        assert_eq!(
            get_req(target, "/logout", Some(&combined)).await.status(),
            StatusCode::SERVICE_UNAVAILABLE
        );
    }
    conn.lock()
        .unwrap()
        .execute_batch("ALTER TABLE unavailable_login_sessions RENAME TO login_sessions;")
        .unwrap();
    assert_eq!(
        get_req(&reboot, "/logout", Some(&combined)).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get_json_req(&reboot, "/api/nav", &combined).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 再ログイン後は以前のcookieを失効し監査書き込み失敗時はcookieを発行しない() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let app = durable_app(test_state(None, Some(audit)), Some(key));
    let first = password_cookie(&app, "internal-pass").await;
    let old = cookie_pair(&first, "hr_login").unwrap();
    let relogin = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/login")
                .header(header::COOKIE, &old)
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from(
                    "email=taro%40f-a-c.co.jp&password=external-pass",
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(relogin.status(), StatusCode::SEE_OTHER);
    let new = cookie_pair(&relogin, "hr_login").unwrap();
    assert_eq!(
        get_json_req(&app, "/api/nav", &old).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        get_json_req(&app, "/api/nav", &new).await.status(),
        StatusCode::OK
    );
    conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_login BEFORE INSERT ON login_sessions BEGIN SELECT RAISE(FAIL,'mock write failure'); END;").unwrap();
    let failed = password_cookie(&app, "internal-pass").await;
    assert!(cookie_pair(&failed, "hr_login").is_none());
}

#[tokio::test(flavor = "multi_thread")]
async fn 監査がない場合は鍵があってもメモリだけに保持する() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let state = test_state(None, None);
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    assert_eq!(login.status(), StatusCode::SEE_OTHER);
    assert!(cookie_pair(&login, "hr_login").is_none());
    let memory = cookie_pair(&login, "id").unwrap();
    assert_eq!(
        get_json_req(&app, "/api/nav", &memory).await.status(),
        StatusCode::OK
    );
    let reboot = durable_app(state, Some(key));
    assert_eq!(
        get_json_req(&reboot, "/api/nav", &memory).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 監査の照会で本人と方式と成功状態を照合する() {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    conn.lock().unwrap().execute_batch("INSERT INTO accounts (id,email,role,first_seen_at) VALUES ('a','Taro@f-a-c.co.jp','user','2026-10-11');
    INSERT INTO login_sessions (id,account_id,login_method,success,started_at) VALUES ('s','a','google_oidc',1,'2026-10-11');").unwrap();
    let turso = audit.turso().clone();
    tokio::task::spawn_blocking(move || {
        assert_eq!(
            crate::audit::dao::active_login_account(&turso, "s", "taro@f-a-c.co.jp", "google_oidc")
                .unwrap(),
            Some("a".into())
        );
        assert!(crate::audit::dao::active_login_account(
            &turso,
            "s",
            "other@f-a-c.co.jp",
            "google_oidc"
        )
        .unwrap()
        .is_none());
        assert!(crate::audit::dao::active_login_account(
            &turso,
            "s",
            "taro@f-a-c.co.jp",
            "password_internal"
        )
        .unwrap()
        .is_none());
        assert!(crate::audit::dao::active_login_account(
            &turso,
            "missing",
            "taro@f-a-c.co.jp",
            "google_oidc"
        )
        .unwrap()
        .is_none());
    })
    .await
    .unwrap();
    conn.lock()
        .unwrap()
        .execute("UPDATE login_sessions SET success = 0", [])
        .unwrap();
    let turso = audit.turso().clone();
    assert!(
        tokio::task::spawn_blocking(move || crate::audit::dao::active_login_account(
            &turso,
            "s",
            "taro@f-a-c.co.jp",
            "google_oidc"
        ))
        .await
        .unwrap()
        .unwrap()
        .is_none()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 鍵なしのログアウトは監査書き込み失敗時も従来どおりメモリを削除する() {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let app = durable_app(test_state(None, Some(audit)), None);
    let login = password_cookie(&app, "internal-pass").await;
    let memory = cookie_pair(&login, "id").unwrap();
    conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_logout BEFORE UPDATE OF ended_at ON login_sessions BEGIN SELECT RAISE(FAIL, 'mock write failure'); END;").unwrap();
    assert_eq!(
        get_req(&app, "/logout", Some(&memory)).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get_json_req(&app, "/api/nav", &memory).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 発行済みcookieの鍵や監査接続を失った状態でログアウト完了と返さない() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let missing_key = durable_app(state, None);
    let missing_audit = durable_app(test_state(None, None), Some(key));
    for target in [&missing_key, &missing_audit] {
        let logout = get_req(target, "/logout", Some(&cookie)).await;
        assert_eq!(logout.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(cookie_pair(&logout, "hr_login").is_none());
    }
    assert_eq!(
        get_req(&app, "/logout", Some(&cookie)).await.status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        get_json_req(&app, "/api/nav", &cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn cookieが一部欠ける場合や無効化中のログアウトでも元の認証を失効する() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    for kind in ["memory_only", "invalid_cookie", "disabled"] {
        let login = password_cookie(&app, "internal-pass").await;
        let saved = cookie_pair(&login, "hr_login").unwrap();
        let memory = cookie_pair(&login, "id").unwrap();
        let cookies = match kind {
            "memory_only" => memory.clone(),
            "invalid_cookie" => format!("hr_login=tampered; {memory}"),
            _ => {
                conn.lock()
                    .unwrap()
                    .execute("UPDATE accounts SET disabled_at = '2026-10-11'", [])
                    .unwrap();
                format!("{saved}; {memory}")
            }
        };
        assert_eq!(
            get_req(&app, "/logout", Some(&cookies)).await.status(),
            StatusCode::SEE_OTHER
        );
        conn.lock()
            .unwrap()
            .execute("UPDATE accounts SET disabled_at = NULL", [])
            .unwrap();
        for target in [&app, &durable_app(state.clone(), Some(key.clone()))] {
            assert_eq!(
                get_json_req(target, "/api/nav", &format!("{saved}; {memory}"))
                    .await
                    .status(),
                StatusCode::UNAUTHORIZED
            );
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 履歴照会は復元時に一度だけでメモリ一致なら障害中も利用できる() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, conn, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
    let state = test_state(None, Some(audit));
    let app = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&app, "internal-pass").await;
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let memory = cookie_pair(&login, "id").unwrap();
    let combined = format!("{cookie}; {memory}");
    let history_reads = || {
        queries
            .lock()
            .unwrap()
            .iter()
            .filter(|sql| sql.starts_with("SELECT s.account_id FROM login_sessions"))
            .count()
    };
    assert_eq!(history_reads(), 0, "履歴はログイン時に保存済み");
    for path in ["/api/nav", "/temporary", "/api/nav"] {
        assert_eq!(
            get_json_req(&app, path, &combined).await.status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        history_reads(),
        0,
        "読み取り・セッション更新の応答も再照会しない"
    );
    let reboot = durable_app(state.clone(), Some(key.clone()));
    let restored = get_json_req(&reboot, "/api/nav", &combined).await;
    assert_eq!(restored.status(), StatusCode::OK);
    let new_memory = cookie_pair(&restored, "id").unwrap();
    let restored_cookie = format!("{cookie}; {new_memory}");
    assert_eq!(history_reads(), 1, "復元した最初の要求のみ");
    for _ in 0..3 {
        assert_eq!(
            get_json_req(&reboot, "/api/nav", &restored_cookie)
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(history_reads(), 1);
    // SQL を問わず監査 DB の読み取りが失敗する状況。既存無効化判定の失敗時方針は維持。
    conn.lock().unwrap().execute_batch("ALTER TABLE login_sessions RENAME TO lost_sessions; ALTER TABLE accounts RENAME TO lost_accounts;").unwrap();
    assert_eq!(
        get_json_req(&app, "/api/nav", &combined).await.status(),
        StatusCode::OK
    );
    assert_eq!(
        get_json_req(&reboot, "/api/nav", &restored_cookie)
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(history_reads(), 1);
    let cold = durable_app(state, Some(key));
    assert_eq!(
        get_json_req(&cold, "/api/nav", &cookie).await.status(),
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(history_reads(), 2);
    conn.lock().unwrap().execute_batch("ALTER TABLE lost_sessions RENAME TO login_sessions; ALTER TABLE lost_accounts RENAME TO accounts;").unwrap();
    let before = history_reads();
    assert_eq!(
        get_req(&reboot, "/logout", Some(&restored_cookie))
            .await
            .status(),
        StatusCode::SEE_OTHER
    );
    assert_eq!(
        history_reads(),
        before,
        "一致するメモリのログアウトは失効を書くだけ"
    );
    for target in [&reboot, &cold] {
        assert_eq!(
            get_json_req(target, "/api/nav", &cookie).await.status(),
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            get_json_req(target, "/api/nav", &restored_cookie)
                .await
                .status(),
            StatusCode::UNAUTHORIZED
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn 別インスタンスの失効は復元時に確認し確認済みのメモリは継続する() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
    let state = test_state(None, Some(audit));
    let first = durable_app(state.clone(), Some(key.clone()));
    let login = password_cookie(&first, "internal-pass").await;
    let cookie = cookie_pair(&login, "hr_login").unwrap();
    let memory = cookie_pair(&login, "id").unwrap();
    let other = durable_app(state.clone(), Some(key.clone()));
    assert_eq!(
        get_req(&other, "/logout", Some(&cookie)).await.status(),
        StatusCode::SEE_OTHER
    );
    // この変更で採用する境界。別インスタンスから既存メモリへの即時失効通知はない。
    assert_eq!(
        get_json_req(&first, "/api/nav", &format!("{cookie}; {memory}"))
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        get_json_req(&first, "/api/nav", &cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
    let reboot = durable_app(state, Some(key));
    assert_eq!(
        get_json_req(&reboot, "/api/nav", &cookie).await.status(),
        StatusCode::UNAUTHORIZED
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 本人か方式か履歴参照が違うメモリでは照会を省かない() {
    let key = tower_sessions::cookie::Key::from(&[42; 64]);
    for (email, password) in [
        ("jiro%40f-a-c.co.jp", "internal-pass"),
        ("taro%40f-a-c.co.jp", "external-pass"),
        ("taro%40f-a-c.co.jp", "internal-pass"),
    ] {
        let (audit, _, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
        let app = durable_app(test_state(None, Some(audit)), Some(key.clone()));
        let first = password_cookie(&app, "internal-pass").await;
        let cookie = cookie_pair(&first, "hr_login").unwrap();
        let other = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/login")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(format!("email={email}&password={password}")))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(other.status(), StatusCode::SEE_OTHER);
        let memory = cookie_pair(&other, "id").unwrap();
        queries.lock().unwrap().clear();
        let restored = get_json_req(&app, "/api/nav", &format!("{cookie}; {memory}")).await;
        assert_eq!(restored.status(), StatusCode::OK);
        let who: Value = serde_json::from_str(&body_string(restored).await).unwrap();
        assert_eq!(who["email"], "taro@f-a-c.co.jp");
        assert_eq!(who["method"], crate::auth::LOGIN_METHOD_PASSWORD_INTERNAL);
        assert_eq!(
            queries
                .lock()
                .unwrap()
                .iter()
                .filter(|sql| sql.starts_with("SELECT s.account_id FROM login_sessions"))
                .count(),
            1
        );
    }
}
