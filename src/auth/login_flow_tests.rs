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
