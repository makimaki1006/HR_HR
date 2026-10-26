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

use super::super::mail::{GmailMailer, Mailer};
use super::{call, cfg, store_c1, test_state};
use crate::audit::fake_turso::start_sqlite_audit;
use crate::audit::AuditDb;
use crate::config::GoogleOidcConfig;
use crate::crm::email::{grants, EmailConfig, EmailService, SCOPES};
use crate::crm::pending;
use crate::crm::rbac::CrmAccess;
use crate::portal::auth::PortalAuth;
use crate::portal::{router_parts, Parts};

const SENDER: &str = "sender@example.com";
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
}
type Shared = Arc<Mutex<Google>>;

async fn token(State(g): State<Shared>) -> impl IntoResponse {
    let mut g = g.lock().unwrap();
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
    let status = if g.send_status == 0 {
        200
    } else {
        g.send_status
    };
    (
        AxStatus::from_u16(status).unwrap(),
        Json(json!({"id": "m1", "threadId": "t1"})),
    )
}

struct Env {
    google: Shared,
    email: Arc<EmailService>,
    audit: AuditDb,
}

async fn env(granted: bool) -> Env {
    let google: Shared = Arc::default();
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let app = Router::new()
        .route("/token", post(token))
        .route("/gmail/users/me/messages/send", post(send))
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
            hosted_domain: "example.com".into(),
        },
        redirect_url: "http://localhost/api/crm/gmail/callback".into(),
        base_url: "http://localhost".into(),
        authorization_url: format!("{base}/authorize"),
        token_url: format!("{base}/token"),
        revoke_url: format!("{base}/revoke"),
        gmail_url: format!("{base}/gmail"),
    }));
    let (audit, _conn) = start_sqlite_audit().await;
    if granted {
        let enc = grants::encrypt(&[9; 32], SENDER, REFRESH).unwrap();
        pending::blocking(&audit, move |t| {
            grants::save(t, SENDER, &enc, &SCOPES.join(" "))
        })
        .await
        .unwrap()
        .unwrap();
    }
    Env {
        google,
        email,
        audit,
    }
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
        head.to_string(),
        String::from_utf8(STANDARD.decode(body).unwrap()).unwrap(),
    )
}

fn header<'a>(head: &'a str, name: &str) -> &'a str {
    head.split("\r\n")
        .find_map(|l| l.strip_prefix(&format!("{name}: ")))
        .unwrap_or_default()
}

fn subject(head: &str) -> String {
    let enc = header(head, "Subject");
    let b64 = enc
        .strip_prefix("=?UTF-8?B?")
        .and_then(|s| s.strip_suffix("?="))
        .unwrap();
    String::from_utf8(STANDARD.decode(b64).unwrap()).unwrap()
}

#[tokio::test]
async fn 許可があれば宛先ごとに1通ずつ_送信元はfrom_件名にコード_本文に個人情報やリンクは無い() {
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
async fn 許可が無いと送れず_顧客のapiは503() {
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

    let parts = Parts {
        auth: Some(Arc::new(PortalAuth::new(store_c1(), cfg()))),
        mailer: Some(m as Arc<dyn Mailer>),
        reader: None,
        store_spreadsheet: None,
    };
    let app = router_parts(parts, CrmAccess::from_list(""), None).with_state(test_state());
    let (status, _, v) = call(
        &app,
        "POST",
        "/api/portal/login",
        &[],
        Some(json!({"email": "x@example.com", "password": "pw"})),
        true,
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "portal_unavailable");
}

#[tokio::test]
async fn 許可が取り消されていれば送れない() {
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
    pending::blocking(&db, |t| grants::revoke(t, SENDER))
        .await
        .unwrap()
        .unwrap();
    let m = mailer(&e);
    assert!(!m.check_now().await);
    assert!(!m.ready());
}

#[tokio::test]
async fn crmのメールが無効_送信元が無い_監査dbが無いときは作らない() {
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
    assert!(logged.contains("送信元の許可を確かめた"));
    assert!(logged.contains("許可切れを返した"));
    assert!(logged.contains("送信元の許可がない"));
    for secret in [ACCESS, REFRESH, SENDER, "a@example.com", "482913"] {
        assert!(!logged.contains(secret), "ログに {secret} が出ている");
    }
}
