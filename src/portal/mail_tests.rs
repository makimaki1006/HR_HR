//! 社員本人の Gmail から送る `GmailMailer` のテスト。Google は偽のサーバー (token と send) だけを使う。
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::{HeaderMap, StatusCode as AxStatus};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::{Json, Router};
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use base64::Engine;
use serde_json::{json, Value};

use std::time::{Duration, Instant};

use axum::http::StatusCode;
use chrono::Utc;

use super::super::mail::{GmailMailer, Mailer, SendLimits};
use super::{call, cfg, harness, pair, store_c1, test_state, PW};
use crate::audit::fake_turso::start_sqlite_audit;
use crate::audit::AuditDb;
use crate::config::GoogleOidcConfig;
use crate::crm::email::gmail::encode_subject;
use crate::crm::email::{grants, EmailConfig, EmailService, SCOPES};
use crate::crm::pending;
use crate::crm::rbac::CrmAccess;
use crate::portal::auth::PortalAuth;
use crate::portal::auth::{CodeResult, LoginResult};
use crate::portal::mail_grant::{self, aad, CALLBACK, SCOPE};
use crate::portal::{router_parts, Parts};
use crate::portal::{CHALLENGE_COOKIE, DEVICE_COOKIE, SESSION_COOKIE};

const SENDER: &str = "sender@f-a-c.co.jp";
const ACCESS: &str = "ya29.access-SECRET-token";
const REFRESH: &str = "refresh-SECRET-value";

#[derive(Default)]
struct Google {
    token_calls: usize,
    /// 400 を返す (許可切れ・取り消し)
    token_invalid: bool,
    /// (bearer, raw) を受け取った順に
    sent: Vec<(String, String)>,
    /// send に返す状態 (0 なら 200)
    send_status: u16,
    /// この宛先への送信だけ 500 を返す
    fail_to: Option<String>,
    /// 許可の手続き: ID token に入れる nonce とアドレス (既定は送信元)
    nonce: String,
    consent_email: Option<String>,
    consent_scope: Option<String>,
    revoke_calls: usize,
}
type Shared = Arc<Mutex<Google>>;

async fn token(State(g): State<Shared>, body: String) -> impl IntoResponse {
    let mut g = g.lock().unwrap();
    if body.contains("grant_type=authorization_code") {
        // 許可の手続きの code → token (署名した ID token を返す)
        let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
        h.kid = Some("test-key-1".into());
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(
            include_str!("../../tests/fixtures/oidc/test_key_1.pem").as_bytes(),
        )
        .unwrap();
        let claims = json!({"iss":"https://accounts.google.com","aud":"test-client","sub":"sender-1",
            "email": g.consent_email.clone().unwrap_or(SENDER.into()),"hd":"f-a-c.co.jp",
            "email_verified":true,"nonce":g.nonce,"exp":Utc::now().timestamp()+3600});
        let id_token = jsonwebtoken::encode(&h, &claims, &key).unwrap();
        return (
            AxStatus::OK,
            Json(
                json!({"access_token": ACCESS, "refresh_token": "refresh-from-consent",
                "id_token": id_token, "expires_in": 3599,
                "scope": g.consent_scope.clone().unwrap_or(format!("openid {SCOPE}"))}),
            ),
        );
    }
    g.token_calls += 1;
    if g.token_invalid {
        return (
            AxStatus::BAD_REQUEST,
            Json(json!({"error": "invalid_grant"})),
        );
    }
    (
        AxStatus::OK,
        Json(json!({"access_token": ACCESS, "expires_in": 3599, "token_type": "Bearer"})),
    )
}

async fn send(
    State(g): State<Shared>,
    headers: HeaderMap,
    Json(v): Json<Value>,
) -> impl IntoResponse {
    let mut g = g.lock().unwrap();
    let bearer = headers
        .get("authorization")
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default()
        .to_string();
    g.sent
        .push((bearer, v["raw"].as_str().unwrap_or_default().to_string()));
    let raw = v["raw"].as_str().unwrap_or_default().to_string();
    let to = header(&decode(&raw).0, "To").to_string();
    let status = if g.fail_to.as_deref() == Some(to.as_str()) {
        500
    } else if g.send_status == 0 {
        200
    } else {
        g.send_status
    };
    (
        AxStatus::from_u16(status).unwrap(),
        Json(json!({"id": "m1", "threadId": "t1"})),
    )
}

/// GmailMailer のテストは 1 本ずつ流す (ログのテストが、並列の他のテストと tracing の
/// callsite の登録を取り合って取りこぼさないように)
static SERIAL: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
async fn serial() -> tokio::sync::MutexGuard<'static, ()> {
    SERIAL.lock().await
}

async fn jwks() -> Json<Value> {
    Json(serde_json::from_str(include_str!("../../tests/fixtures/oidc/jwks.json")).unwrap())
}

async fn revoke_google(State(g): State<Shared>) -> Json<Value> {
    g.lock().unwrap().revoke_calls += 1;
    Json(json!({}))
}

struct Env {
    google: Shared,
    email: Arc<EmailService>,
    audit: AuditDb,
    conn: crate::audit::fake_turso::SharedConn,
    base: String,
}

async fn env(granted: bool) -> Env {
    let google: Shared = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/token", post(token))
        .route("/gmail/users/me/messages/send", post(send))
        .route("/jwks", axum::routing::get(jwks))
        .route("/revoke", post(revoke_google))
        .with_state(google.clone());
    tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });
    let email = Arc::new(EmailService::new(EmailConfig {
        key: [9; 32],
        google: GoogleOidcConfig {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_url: "http://localhost/auth/google/callback".into(),
            hosted_domain: "f-a-c.co.jp".into(),
        },
        redirect_url: format!("http://localhost{CALLBACK}"),
        base_url: "http://localhost".into(),
        authorization_url: format!("{base}/authorize"),
        token_url: format!("{base}/token"),
        revoke_url: format!("{base}/revoke"),
        gmail_url: format!("{base}/gmail"),
    }));
    let (audit, conn) = start_sqlite_audit().await;
    if granted {
        portal_grant(&audit).await;
    }
    Env {
        google,
        email,
        audit,
        conn,
        base,
    }
}

/// 顧客画面の送信元の許可 (portal_mail_grants、AAD に用途の文字)
async fn portal_grant(audit: &AuditDb) {
    let enc = grants::encrypt(&[9; 32], &aad(SENDER), REFRESH).unwrap();
    pending::blocking(audit, move |t| mail_grant::save(t, SENDER, &enc, SCOPE))
        .await
        .unwrap()
        .unwrap();
}

/// 同じ人の CRM のメール送信の許可 (crm_gmail_grants)
async fn crm_grant(audit: &AuditDb) {
    let enc = grants::encrypt(&[9; 32], SENDER, "refresh-crm").unwrap();
    pending::blocking(audit, move |t| {
        grants::save(t, SENDER, &enc, &SCOPES.join(" "))
    })
    .await
    .unwrap()
    .unwrap();
}

fn mailer(e: &Env) -> Arc<GmailMailer> {
    GmailMailer::from_config(
        Some(SENDER.into()),
        Some(e.email.clone()),
        Some(e.audit.clone()),
    )
    .expect("CRM のメールが有効で送信元があれば作る")
}

/// raw (base64url) をヘッダーと本文 (base64 を戻したもの) に分ける
fn decode(raw: &str) -> (String, String) {
    let text = String::from_utf8(URL_SAFE_NO_PAD.decode(raw).unwrap()).unwrap();
    let (head, body) = text.split_once("\r\n\r\n").unwrap();
    let body: String = body.split_whitespace().collect();
    (
        // 折り返し (CRLF + 空白) を戻す
        head.replace("\r\n ", " "),
        String::from_utf8(STANDARD.decode(body).unwrap()).unwrap(),
    )
}

fn header<'a>(head: &'a str, name: &str) -> &'a str {
    head.split("\r\n")
        .find_map(|l| l.strip_prefix(&format!("{name}: ")))
        .unwrap_or_default()
}

/// encoded-word を戻してつなぐ (語の間の空白は無視する。RFC 2047)
fn subject(head: &str) -> String {
    let words = header(head, "Subject").split(' ').map(|w| {
        assert!(w.len() <= 75, "encoded-word は 75 文字まで: {}", w.len());
        let b64 = w
            .strip_prefix("=?UTF-8?B?")
            .and_then(|s| s.strip_suffix("?="))
            .unwrap();
        STANDARD.decode(b64).unwrap()
    });
    String::from_utf8(words.flatten().collect()).unwrap()
}

#[tokio::test]
async fn 許可があれば宛先ごとに1通ずつ_送信元はfrom_件名にコード_本文に個人情報やリンクは無い() {
    let _serial = serial().await;
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    assert!(m.ready());
    let to = vec!["a@example.com".to_string(), "b@example.org".to_string()];
    m.send_code(&to, "482913").await.unwrap();
    // もう 1 回送っても access token は使い回す
    m.send_code(&to[..1], "111222").await.unwrap();

    let g = e.google.lock().unwrap();
    assert_eq!(g.token_calls, 1, "期限内の access token は取り直さない");
    assert_eq!(g.sent.len(), 3, "宛先 1 つにつき 1 通");
    for (i, (bearer, raw)) in g.sent.iter().enumerate() {
        assert_eq!(bearer, &format!("Bearer {ACCESS}"));
        let (head, body) = decode(raw);
        assert_eq!(header(&head, "From"), SENDER);
        let expected_to = if i == 1 {
            "b@example.org"
        } else {
            "a@example.com"
        };
        assert_eq!(header(&head, "To"), expected_to);
        // 他の宛先は見えない (Cc・Bcc も無い)
        let other = if i == 1 {
            "a@example.com"
        } else {
            "b@example.org"
        };
        assert!(!head.contains(other) && !body.contains(other));
        assert!(!head.contains("Cc:") && !head.contains("Bcc:"));
        assert!(head.contains("Content-Type: text/plain; charset=UTF-8"));
        let code = if i == 2 { "111222" } else { "482913" };
        assert_eq!(subject(&head), format!("確認コード {code}（応募者の管理）"));
        assert!(body.contains(code));
        assert!(
            !body.contains("http") && !body.contains("www."),
            "本文にリンクは無い"
        );
        assert!(
            !body.contains('@'),
            "本文にメールアドレス (個人情報) は無い"
        );
        assert!(!body.contains("<img") && !body.contains("<a "));
    }
}

#[tokio::test]
async fn 汎用の送信は件名と本文をそのまま送る() {
    let _serial = serial().await;
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    m.send(
        &["a@example.com".to_string()],
        "新しい応募があります",
        "応募者の管理の画面でご確認ください。\n",
    )
    .await
    .unwrap();
    let g = e.google.lock().unwrap();
    let (head, body) = decode(&g.sent[0].1);
    assert_eq!(subject(&head), "新しい応募があります");
    assert_eq!(body, "応募者の管理の画面でご確認ください。\r\n");
}

#[tokio::test]
async fn 許可が無いとコードを送らないが_ログイン済みと覚えた端末は使える() {
    let _serial = serial().await;
    let e = env(false).await;
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert!(!m.ready());
    assert_eq!(
        e.google.lock().unwrap().token_calls,
        0,
        "許可が無ければ Google を呼ばない"
    );
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(e.google.lock().unwrap().sent.is_empty());

    // 覚えた端末とログイン中の画面を、メールを通さずに作っておく
    let auth = Arc::new(PortalAuth::new(store_c1(), cfg()));
    let (_, invite) = auth
        .create_account("hanako@customer.example", "c1", Utc::now())
        .unwrap();
    auth.accept_invite(&invite, PW, Utc::now()).unwrap();
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("hanako@customer.example", PW, None, "1.1.1.1", Utc::now())
    else {
        panic!("コードを聞かない");
    };
    let CodeResult::SignedIn {
        session,
        device: Some(device),
    } = auth.verify_code(&challenge, &code, true, Utc::now())
    else {
        panic!("ログインできない");
    };
    let session = format!("{SESSION_COOKIE}={session}");
    let device = format!("{DEVICE_COOKIE}={device}");

    let parts = Parts {
        auth: Some(auth.clone()),
        mailer: Some(m as Arc<dyn Mailer>),
        reader: None,
        store_spreadsheet: None,
        notify: None,
    };
    let app = router_parts(parts, CrmAccess::from_list(""), None).with_state(test_state());
    let login = json!({"email": "hanako@customer.example", "password": PW});

    // ログイン中の画面・見る画面は使える
    let (s, _, v) = call(
        &app,
        "GET",
        "/api/portal/me",
        std::slice::from_ref(&session),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["email"], "hanako@customer.example");
    let (s, _, v) = call(&app, "GET", "/api/portal/sheets", &[session], None, false).await;
    assert_eq!(s, StatusCode::OK);
    assert!(v["sheets"].is_array());
    // 覚えた端末ならコードを聞かないのでログインできる
    let (s, _, v) = call(
        &app,
        "POST",
        "/api/portal/login",
        &[device],
        Some(login.clone()),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "signed_in");
    // 新しい端末はコードを送れないので断る (やり取りは残さない)
    let (s, set, v) = call(&app, "POST", "/api/portal/login", &[], Some(login), true).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "mail_failed");
    assert!(pair(&set, CHALLENGE_COOKIE).is_none());
    // 送り直しも断り、今のコードは変えない
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("hanako@customer.example", PW, None, "1.1.1.1", Utc::now())
    else {
        panic!("コードを聞かない");
    };
    let ch = format!("{CHALLENGE_COOKIE}={challenge}");
    let (s, _, v) = call(&app, "POST", "/api/portal/login/resend", &[ch], None, true).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "mail_failed");
    assert!(matches!(
        auth.verify_code(&challenge, &code, false, Utc::now()),
        CodeResult::SignedIn { .. }
    ));
}

#[tokio::test]
async fn 送るたびに許可を読み直し_取り消しや無効化のあとは送らない() {
    let _serial = serial().await;
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    m.send_code(&["a@example.com".into()], "123456")
        .await
        .unwrap();
    // CRM 画面で取り消した (access token はまだ期限内)
    pending::blocking(&e.audit, |t| mail_grant::revoke(t, SENDER))
        .await
        .unwrap()
        .unwrap();
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(!m.ready());
    {
        let g = e.google.lock().unwrap();
        assert_eq!(g.sent.len(), 1, "取り消しのあとは送らない");
        assert_eq!(g.token_calls, 1);
    }

    // 送信元のアカウントが HR_HR で無効化された
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    pending::blocking(&e.audit, |t| {
        t.execute(
            "INSERT INTO accounts (id, email, first_seen_at, disabled_at) VALUES ('1', ?1, '2026-10-01T00:00:00Z', '2026-10-26T00:00:00Z')",
            &[&SENDER],
        )
    })
    .await
    .unwrap()
    .unwrap();
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(!m.ready());
    assert!(e.google.lock().unwrap().sent.is_empty());
}

#[tokio::test]
async fn 一部の宛先だけ届いたら届いた宛先だけを返す() {
    let _serial = serial().await;
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    e.google.lock().unwrap().fail_to = Some("b@example.org".into());
    let delivered = m
        .send_code(&["a@example.com".into(), "b@example.org".into()], "123456")
        .await
        .unwrap();
    assert_eq!(delivered, vec!["a@example.com".to_string()]);
    assert!(m.ready());
}

#[tokio::test]
async fn gmailに403か429で断られたらしばらく送らない() {
    let _serial = serial().await;
    for status in [403, 429] {
        let e = env(true).await;
        let m = mailer(&e);
        assert!(m.check_now().await);
        e.google.lock().unwrap().send_status = status;
        assert!(m
            .send_code(&["a@example.com".into()], "123456")
            .await
            .is_err());
        assert!(!m.ready(), "{status} のあとは送れない扱い");
        e.google.lock().unwrap().send_status = 0;
        assert!(m
            .send_code(&["a@example.com".into()], "123456")
            .await
            .is_err());
        assert_eq!(
            e.google.lock().unwrap().sent.len(),
            1,
            "待ちの間は Gmail を呼ばない"
        );
    }
}

#[test]
fn 長い件名は75文字までの語に分ける() {
    let long = "確認コード 123456（応募者の管理）".repeat(5);
    let enc = encode_subject(&long);
    assert!(enc.contains("\r\n "), "折り返している");
    let head = format!("Subject: {}", enc.replace("\r\n ", " "));
    assert_eq!(subject(&head), long);
    assert_eq!(
        subject(&format!("Subject: {}", encode_subject("短い"))),
        "短い"
    );
}

#[test]
fn 送信の回数はアカウントごとと全体で数える() {
    let limits = SendLimits::new(2, 3, Duration::from_secs(3600));
    let t = Instant::now();
    assert!(limits.take("a", 1, t));
    assert!(limits.take("a", 1, t));
    assert!(!limits.allows("a", 1, t), "アカウントごとの上限");
    assert!(!limits.take("a", 1, t));
    assert!(!limits.take("b", 2, t), "全体の通数の上限");
    assert!(limits.take("b", 1, t));
    assert!(!limits.take("c", 1, t));
    let later = t + Duration::from_secs(3600);
    assert!(limits.take("a", 1, later), "1 時間たてば戻る");
}

#[tokio::test]
async fn 確認コードは1時間に5回まで_超えたら送らずに断る() {
    let h = harness();
    let (_, invite) = h
        .auth
        .create_account("hanako@customer.example", "c1", Utc::now())
        .unwrap();
    h.auth.accept_invite(&invite, PW, Utc::now()).unwrap();
    let login = json!({"email": "hanako@customer.example", "password": PW});
    let mut last = None;
    for _ in 0..5 {
        let (s, set, v) = call(
            &h.app,
            "POST",
            "/api/portal/login",
            &[],
            Some(login.clone()),
            true,
        )
        .await;
        assert_eq!(s, StatusCode::OK);
        assert_eq!(v["status"], "code_required");
        last = pair(&set, CHALLENGE_COOKIE);
    }
    let (s, set, v) = call(&h.app, "POST", "/api/portal/login", &[], Some(login), true).await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["error"], "mail_limit");
    assert!(v["message"].as_str().unwrap().contains("1 時間ほどおいて"));
    assert!(pair(&set, CHALLENGE_COOKIE).is_none());
    // 送り直しも同じ上限で、送らない
    let (s, _, v) = call(
        &h.app,
        "POST",
        "/api/portal/login/resend",
        &[last.unwrap()],
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(v["error"], "mail_limit");
    assert_eq!(h.mailer.sent.lock().unwrap().len(), 5);
}

#[tokio::test]
async fn 許可が取り消されていれば送れない() {
    let _serial = serial().await;
    let e = env(true).await;
    e.google.lock().unwrap().token_invalid = true;
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert!(!m.ready());
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(e.google.lock().unwrap().sent.is_empty());
    // CRM 画面で取り消したあと (監査 DB の許可が消えている)
    let e = env(true).await;
    let db = e.audit.clone();
    pending::blocking(&db, |t| mail_grant::revoke(t, SENDER))
        .await
        .unwrap()
        .unwrap();
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert!(!m.ready());
}

#[tokio::test]
async fn crmのメールが無効_送信元が無い_監査dbが無いときは作らない() {
    let _serial = serial().await;
    let e = env(true).await;
    assert!(GmailMailer::from_config(Some(SENDER.into()), None, Some(e.audit.clone())).is_none());
    assert!(GmailMailer::from_config(None, Some(e.email.clone()), Some(e.audit.clone())).is_none());
    assert!(GmailMailer::from_config(
        Some("  ".into()),
        Some(e.email.clone()),
        Some(e.audit.clone())
    )
    .is_none());
    assert!(GmailMailer::from_config(Some(SENDER.into()), Some(e.email.clone()), None).is_none());
    assert!(GmailMailer::from_config(
        Some("bad\r\nBcc: x@example.com".into()),
        Some(e.email.clone()),
        Some(e.audit)
    )
    .is_none());
}

#[tokio::test]
async fn gmailの失敗で送信失敗_許可切れなら送れない状態に戻る() {
    let _serial = serial().await;
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    e.google.lock().unwrap().send_status = 500;
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(m.ready(), "一時的な失敗では止めない");

    e.google.lock().unwrap().send_status = 401;
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert!(!m.ready(), "許可切れを返したら顧客の API は 503 に戻る");

    // 宛先の形が正しくなければ送らない (ヘッダーの差し込みを防ぐ)
    let e = env(true).await;
    let m = mailer(&e);
    assert!(m.check_now().await);
    assert!(m
        .send_code(&["a@example.com\r\nBcc: z@example.com".into()], "123456")
        .await
        .is_err());
    assert!(e.google.lock().unwrap().sent.is_empty());
}

#[tokio::test]
async fn トークン_送信元_宛先_コードはログに出ない() {
    let _serial = serial().await;
    use std::io::Write;
    #[derive(Clone)]
    struct Writer(Arc<Mutex<Vec<u8>>>);
    impl Write for Writer {
        fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(buf);
            Ok(buf.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }
    let buffer = Arc::new(Mutex::new(Vec::new()));
    let writer = Writer(buffer.clone());
    let subscriber = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::TRACE)
        .with_ansi(false)
        .with_writer(move || writer.clone())
        .finish();
    let _guard = tracing::subscriber::set_default(subscriber);
    // 並列に走る他テストが購読者なしで先に callsite を登録すると interest が never のまま
    // 残ることがある (tracing の既知の競合。hubspot の client_tests と同じ)。操作ごとに作り直す
    let rebuild = tracing::callsite::rebuild_interest_cache;

    let e = env(true).await;
    let m = mailer(&e);
    rebuild();
    assert!(m.check_now().await);
    rebuild();
    m.send_code(&["a@example.com".into()], "482913")
        .await
        .unwrap();
    e.google.lock().unwrap().send_status = 401;
    rebuild();
    let _ = m.send_code(&["a@example.com".into()], "482913").await;
    let none = env(false).await;
    let none = mailer(&none);
    rebuild();
    assert!(!none.check_now().await);

    let logged = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
    assert!(
        logged.contains("送信元の許可を確かめた"),
        "{}",
        logged
            .lines()
            .filter(|l| l.contains("portal"))
            .collect::<Vec<_>>()
            .join("\n")
    );
    assert!(logged.contains("許可切れを返した"));
    assert!(logged.contains("送信元の許可がない"));
    for secret in [ACCESS, REFRESH, SENDER, "a@example.com", "482913"] {
        assert!(!logged.contains(secret), "ログに {secret} が出ている");
    }
}

// ---- 顧客画面の許可は CRM の許可と別 (ADR-036) ----

#[tokio::test]
async fn crmの許可と顧客画面の許可は別で_片方の取り消しはもう片方に効かない() {
    let _serial = serial().await;
    let e = env(false).await;
    // CRM の許可だけでは送らない
    crm_grant(&e.audit).await;
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert_eq!(e.google.lock().unwrap().token_calls, 0);
    // 顧客画面の許可を取ると送れる
    portal_grant(&e.audit).await;
    assert!(m.check_now().await);
    // CRM 側で取り消しても、顧客画面の送信は続く
    pending::blocking(&e.audit, |t| grants::revoke(t, SENDER))
        .await
        .unwrap()
        .unwrap();
    m.send_code(&["a@example.com".into()], "123456")
        .await
        .unwrap();
    // CRM 側でもう一度許可しても、顧客画面の許可は書き換わらない
    crm_grant(&e.audit).await;
    m.send_code(&["a@example.com".into()], "123456")
        .await
        .unwrap();
    // 顧客画面で取り消しても、CRM の許可は残る
    assert_eq!(m.revoke().await, Ok(false));
    let (crm, portal) = pending::blocking(&e.audit, |t| {
        (
            grants::get(t, SENDER).unwrap(),
            mail_grant::get(t, SENDER).unwrap(),
        )
    })
    .await
    .unwrap();
    assert!(crm.is_some(), "CRM の許可は消さない");
    assert!(portal.is_none());
    assert!(!m.ready());
    assert!(m
        .send_code(&["a@example.com".into()], "123456")
        .await
        .is_err());
    assert_eq!(e.google.lock().unwrap().sent.len(), 2);
    assert_eq!(
        e.google.lock().unwrap().revoke_calls,
        1,
        "Google にも取り消しを頼む"
    );

    // CRM の暗号文 (AAD がアドレスだけ) を顧客画面の表に入れても復号できない
    let e = env(false).await;
    let enc = grants::encrypt(&[9; 32], SENDER, REFRESH).unwrap();
    pending::blocking(&e.audit, move |t| mail_grant::save(t, SENDER, &enc, SCOPE))
        .await
        .unwrap()
        .unwrap();
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert_eq!(e.google.lock().unwrap().token_calls, 0);
}

const BOSS: &str = "boss@f-a-c.co.jp";

/// 管理画面の API を通すための組み立て (送信元と別の管理者の 2 人がログイン済み)
async fn admin_app(
    e: &Env,
    m: Arc<GmailMailer>,
) -> (Router, std::collections::HashMap<&'static str, String>) {
    use crate::auth::google_oidc::{Endpoints, GoogleOidc};
    use crate::crm::rbac::CrmRole;
    let base = Arc::try_unwrap(test_state()).ok().unwrap();
    let oidc = GoogleOidc::with_endpoints(
        GoogleOidcConfig {
            client_id: "test-client".into(),
            client_secret: "test-secret".into(),
            redirect_url: "http://localhost/auth/google/callback".into(),
            hosted_domain: "f-a-c.co.jp".into(),
        },
        Endpoints {
            issuer: "https://accounts.google.com".into(),
            authorization_endpoint: format!("{}/authorize", e.base),
            token_endpoint: format!("{}/token", e.base),
            jwks_uri: format!("{}/jwks", e.base),
        },
    );
    let state = Arc::new(crate::AppState {
        audit: Some(e.audit.clone()),
        google_oidc: Some(Arc::new(oidc)),
        ..base
    });
    let access = CrmAccess::from_list(&format!("{SENDER},{BOSS}"))
        .with_test_role(SENDER, CrmRole::Admin)
        .with_test_role(BOSS, CrmRole::Admin);
    let parts = Parts {
        auth: Some(Arc::new(PortalAuth::new(store_c1(), cfg()))),
        mailer: Some(m as Arc<dyn Mailer>),
        reader: None,
        store_spreadsheet: None,
        notify: None,
    };
    let app = Router::new()
        .merge(router_parts(parts, access, None))
        .route("/__test/session", post(super::inject_staff))
        .with_state(state)
        .layer(tower_sessions::SessionManagerLayer::new(
            tower_sessions::MemoryStore::default(),
        ));
    let mut cookies = std::collections::HashMap::new();
    for who in [SENDER, BOSS] {
        let (_, set, _) = call(
            &app,
            "POST",
            "/__test/session",
            &[],
            Some(json!({ "email": who })),
            false,
        )
        .await;
        cookies.insert(who, pair(&set, "id").expect("社内のセッション"));
    }
    (app, cookies)
}

/// 許可を始める。(状態, Google の URL の query, 手続きの Cookie)
async fn start_consent(
    app: &Router,
    cookie: &str,
) -> (
    StatusCode,
    std::collections::HashMap<String, String>,
    Option<String>,
) {
    use tower::ServiceExt;
    let r = app
        .clone()
        .oneshot(
            axum::http::Request::get("/api/admin/portal/mail/authorize")
                .header(axum::http::header::COOKIE, cookie)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status();
    let query = r
        .headers()
        .get(axum::http::header::LOCATION)
        .map(|l| {
            reqwest::Url::parse(l.to_str().unwrap())
                .unwrap()
                .query_pairs()
                .into_owned()
                .collect()
        })
        .unwrap_or_default();
    let tx = r
        .headers()
        .get_all(axum::http::header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .find(|c| c.starts_with("hrhr_portal_mail_tx="))
        .map(|c| c.split(';').next().unwrap().to_string());
    (status, query, tx)
}

async fn finish_consent(app: &Router, state: &str, tx: &str) -> StatusCode {
    use tower::ServiceExt;
    app.clone()
        .oneshot(
            axum::http::Request::get(format!("{CALLBACK}?code=fake-code&state={state}"))
                .header(axum::http::header::COOKIE, tx)
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
        .status()
}

fn portal_rows(e: &Env) -> Vec<String> {
    let conn = e.conn.lock().unwrap();
    let mut st = conn
        .prepare("SELECT encrypted_refresh_token FROM portal_mail_grants WHERE revoked_at IS NULL")
        .unwrap();
    st.query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .map(Result::unwrap)
        .collect()
}

#[tokio::test]
async fn 管理画面の許可は送信元本人だけ_gmail_sendだけを求め_別の人の許可は保存しない() {
    let _serial = serial().await;
    let e = env(false).await;
    let m = mailer(&e);
    let (app, cookies) = admin_app(&e, m.clone()).await;

    // 別の管理者: 状態は見られるが、許可は始められない
    let (s, _, v) = call(
        &app,
        "GET",
        "/api/admin/portal/mail",
        &[cookies[BOSS].clone()],
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["configured"], true);
    assert_eq!(v["is_sender"], false);
    assert_eq!(v["state"], "none");
    let (s, _, _) = start_consent(&app, &cookies[BOSS]).await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    // 送信元本人が始める。範囲は gmail.send だけ、callback は顧客画面用
    let (s, q, tx) = start_consent(&app, &cookies[SENDER]).await;
    assert_eq!(s, StatusCode::SEE_OTHER);
    assert_eq!(q["scope"], format!("openid email {SCOPE}"));
    assert!(!q["scope"].contains("readonly"));
    assert_eq!(q["redirect_uri"], format!("http://localhost{CALLBACK}"));
    assert_eq!(q["code_challenge_method"], "S256");
    // Google で別の人のアカウントを選んだ → 保存しない
    {
        let mut g = e.google.lock().unwrap();
        g.nonce = q["nonce"].clone();
        g.consent_email = Some(BOSS.into());
    }
    assert_eq!(
        finish_consent(&app, &q["state"], tx.as_deref().unwrap()).await,
        StatusCode::FORBIDDEN
    );
    assert!(portal_rows(&e).is_empty(), "別の人の許可は保存しない");

    // 本人のアカウントで許可 → 保存し、送れるようになる
    let (_, q, tx) = start_consent(&app, &cookies[SENDER]).await;
    {
        let mut g = e.google.lock().unwrap();
        g.nonce = q["nonce"].clone();
        g.consent_email = None;
    }
    let tx = tx.unwrap();
    assert_eq!(finish_consent(&app, &q["state"], &tx).await, StatusCode::OK);
    let rows = portal_rows(&e);
    assert_eq!(rows.len(), 1);
    assert!(
        !rows[0].contains("refresh-from-consent"),
        "暗号化して保存する"
    );
    assert_eq!(
        grants::decrypt(&[9; 32], &aad(SENDER), &rows[0]).unwrap(),
        "refresh-from-consent"
    );
    assert!(
        grants::decrypt(&[9; 32], SENDER, &rows[0]).is_err(),
        "CRM の AAD では復号できない"
    );
    let crm_rows: i64 = e
        .conn
        .lock()
        .unwrap()
        .query_row("SELECT COUNT(*) FROM crm_gmail_grants", [], |r| r.get(0))
        .unwrap();
    assert_eq!(crm_rows, 0, "CRM の表には書かない");
    assert!(m.ready());
    // 同じ手続きはもう使えない
    assert_eq!(
        finish_consent(&app, &q["state"], &tx).await,
        StatusCode::BAD_REQUEST
    );
    let (_, _, v) = call(
        &app,
        "GET",
        "/api/admin/portal/mail",
        &[cookies[SENDER].clone()],
        None,
        false,
    )
    .await;
    assert_eq!(v["state"], "granted");
    assert_eq!(v["is_sender"], true);

    // 取り消しは管理者なら押せる
    let (s, _, _) = call(
        &app,
        "POST",
        "/api/admin/portal/mail/revoke",
        &[cookies[BOSS].clone()],
        Some(json!({})),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let (_, _, v) = call(
        &app,
        "GET",
        "/api/admin/portal/mail",
        &[cookies[BOSS].clone()],
        None,
        false,
    )
    .await;
    assert_eq!(v["state"], "none");
    assert!(!m.ready());
    assert_eq!(e.google.lock().unwrap().revoke_calls, 1);
}
