use std::sync::{Arc, Mutex};

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::Router;
use chrono::{Duration, TimeZone, Utc};
use serde_json::Value;
use tower::ServiceExt;

use super::auth::{
    self, AuthConfig, CodeResult, InviteError, LoginResult, PasswordProblem, PortalAuth,
    ResendResult,
};
use super::mail::{MailError, MailFuture, Mailer};
use super::store::{AccountStatus, Customer, CustomerStage, MemoryStore, PortalStore};
use super::*;

const PW: &str = "correct horse battery";

fn cfg() -> AuthConfig {
    let mut c = AuthConfig::new(5, 300);
    c.bcrypt_cost = 4;
    c
}

/// 顧客 c1 (段階「両方」) を登録したメモリの保存先
fn store_c1() -> Arc<MemoryStore> {
    let store = Arc::new(MemoryStore::default());
    store
        .insert_customer(Customer {
            id: "c1".into(),
            name: "テスト顧客".into(),
            hubspot_company_id: None,
            stage: CustomerStage::Both,
            disabled: false,
            created_at: t0(),
            updated_at: t0(),
        })
        .unwrap();
    store
}

fn t0() -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 23, 1, 0, 0).unwrap()
}

/// アカウントを作り、招待でパスワードを決めたところまで
fn ready(auth: &PortalAuth, email: &str) -> String {
    let (a, token) = auth.create_account(email, "c1", t0()).unwrap();
    auth.accept_invite(&token, PW, t0()).unwrap();
    a.id
}

fn code_login(auth: &PortalAuth, email: &str, remember: bool) -> (String, Option<String>) {
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login(email, PW, None, "1.1.1.1", t0())
    else {
        panic!("新しい端末なのにコードを聞かない");
    };
    match auth.verify_code(&challenge, &code, remember, t0()) {
        CodeResult::SignedIn { session, device } => (session, device),
        other => panic!("{other:?}"),
    }
}

#[test]
fn 新しい端末ではコードを聞き_覚えた端末では聞かない() {
    let auth = PortalAuth::new(store_c1(), cfg());
    ready(&auth, "taro@example.com");
    let (session, device) = code_login(&auth, "Taro@Example.com ", true);
    assert!(auth.current(&session, t0()).is_some());
    let device = device.expect("覚える指定なのに札が無い");
    match auth.login(
        "taro@example.com",
        PW,
        Some(&device),
        "1.1.1.1",
        t0() + Duration::days(89),
    ) {
        LoginResult::SignedIn { .. } => {}
        other => panic!("覚えた端末でコードを聞いた: {other:?}"),
    }
    // 最後に使ってから 90 日で切れる
    let later = t0() + Duration::days(89) + Duration::days(90);
    assert!(matches!(
        auth.login("taro@example.com", PW, Some(&device), "1.1.1.1", later),
        LoginResult::CodeRequired { .. }
    ));
    // 覚えないを選べば札は渡さない
    assert!(code_login(&auth, "taro@example.com", false).1.is_none());
}

#[test]
fn 覚えた端末でもパスワードは要る_別の人の札は使えない() {
    let auth = PortalAuth::new(store_c1(), cfg());
    ready(&auth, "a@example.com");
    ready(&auth, "b@example.com");
    let (_, device) = code_login(&auth, "a@example.com", true);
    let device = device.unwrap();
    assert!(matches!(
        auth.login(
            "a@example.com",
            "wrong password!!",
            Some(&device),
            "1.1.1.1",
            t0()
        ),
        LoginResult::Invalid
    ));
    assert!(matches!(
        auth.login("b@example.com", PW, Some(&device), "1.1.1.1", t0()),
        LoginResult::CodeRequired { .. }
    ));
}

#[test]
fn コードは別のやり取りでは通らず_5回間違えると無効_10分で切れる() {
    let auth = PortalAuth::new(store_c1(), cfg());
    ready(&auth, "a@example.com");
    let LoginResult::CodeRequired {
        challenge: c1,
        code: code1,
        ..
    } = auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    let LoginResult::CodeRequired { challenge: c2, .. } =
        auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    // 別のブラウザ (別のやり取り) で同じコードを入れても通らない
    assert!(!matches!(
        auth.verify_code(&c2, &code1, true, t0()),
        CodeResult::SignedIn { .. }
    ));
    for left in (1..5).rev() {
        match auth.verify_code(&c2, "000000x", true, t0()) {
            CodeResult::Wrong { left: l } => assert_eq!(l, left - 1, "残り"),
            CodeResult::Expired => break,
            other => panic!("{other:?}"),
        }
    }
    assert!(matches!(
        auth.verify_code(&c2, "123456", true, t0()),
        CodeResult::Expired
    ));
    // 10 分を過ぎたコードは使えない
    assert!(matches!(
        auth.verify_code(&c1, &code1, true, t0() + Duration::minutes(10)),
        CodeResult::Expired
    ));
}

#[test]
fn 送り直しは60秒あけて3回まで_古いコードは使えない() {
    let auth = PortalAuth::new(store_c1(), cfg());
    ready(&auth, "a@example.com");
    let LoginResult::CodeRequired {
        challenge,
        code: old,
        ..
    } = auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    assert!(matches!(
        auth.resend(&challenge, t0() + Duration::seconds(10)),
        ResendResult::Wait { secs: 50 }
    ));
    let mut t = t0();
    let mut last = String::new();
    for _ in 0..3 {
        t += Duration::seconds(61);
        let ResendResult::Sent { code, .. } = auth.resend(&challenge, t) else {
            panic!()
        };
        last = code;
    }
    t += Duration::seconds(61);
    assert!(matches!(auth.resend(&challenge, t), ResendResult::Limit));
    if old != last {
        assert!(matches!(
            auth.verify_code(&challenge, &old, true, t),
            CodeResult::Wrong { .. }
        ));
    }
    assert!(matches!(
        auth.verify_code(&challenge, &last, true, t),
        CodeResult::SignedIn { .. }
    ));
}

#[test]
fn 失敗が続くとロックし_登録の無いメールも同じ扱い() {
    let auth = PortalAuth::new(store_c1(), cfg());
    ready(&auth, "a@example.com");
    for _ in 0..5 {
        assert!(matches!(
            auth.login("a@example.com", "wrong password!!", None, "ip1", t0()),
            LoginResult::Invalid
        ));
    }
    // 正しいパスワードでもロック中は入れない (別の接続元からでも)
    assert!(matches!(
        auth.login("a@example.com", PW, None, "ip2", t0()),
        LoginResult::Locked
    ));
    // 時間が過ぎれば入れる
    assert!(matches!(
        auth.login(
            "a@example.com",
            PW,
            None,
            "ip2",
            t0() + Duration::seconds(300)
        ),
        LoginResult::CodeRequired { .. }
    ));
    for _ in 0..5 {
        assert!(matches!(
            auth.login("nobody@example.com", "whatever pass", None, "ip3", t0()),
            LoginResult::Invalid
        ));
    }
    assert!(matches!(
        auth.login("nobody@example.com", "whatever pass", None, "ip4", t0()),
        LoginResult::Locked
    ));
}

#[test]
fn 招待は1回だけ_1週間_新しいリンクで前のは使えない() {
    let auth = PortalAuth::new(store_c1(), cfg());
    let (a, t1) = auth.create_account("a@example.com", "c1", t0()).unwrap();
    // パスワードを決める前はログインできない
    assert!(matches!(
        auth.login("a@example.com", PW, None, "ip", t0()),
        LoginResult::Invalid
    ));
    let t2 = auth.issue_invite(&a.id, t0()).unwrap();
    assert_eq!(auth.accept_invite(&t1, PW, t0()), Err(InviteError::Invalid));
    assert_eq!(
        auth.accept_invite(&t2, "short", t0()),
        Err(InviteError::Password(PasswordProblem::TooShort))
    );
    let long_email = "longer.name@example.com";
    let (_, t3) = auth.create_account(long_email, "c1", t0()).unwrap();
    assert_eq!(
        auth.accept_invite(&t3, "Longer.Name@example.com", t0()),
        Err(InviteError::Password(PasswordProblem::SameAsEmail))
    );
    assert_eq!(
        auth.accept_invite(&t3, &"あ".repeat(25), t0()),
        Err(InviteError::Password(PasswordProblem::TooLong))
    );
    assert!(auth
        .accept_invite(&t2, PW, t0() + Duration::days(6))
        .is_ok());
    assert_eq!(
        auth.accept_invite(&t2, PW, t0() + Duration::days(6)),
        Err(InviteError::Invalid)
    );
    // 期限切れ
    let t4 = auth.issue_invite(&a.id, t0()).unwrap();
    assert_eq!(
        auth.accept_invite(&t4, PW, t0() + Duration::days(7)),
        Err(InviteError::Invalid)
    );
}

#[test]
fn パスワードの再設定と停止で端末とログイン中の画面が無効になる() {
    let auth = PortalAuth::new(store_c1(), cfg());
    let id = ready(&auth, "a@example.com");
    let (session, device) = code_login(&auth, "a@example.com", true);
    let device = device.unwrap();
    let reset = auth.issue_invite(&id, t0()).unwrap();
    let later = t0() + Duration::minutes(1);
    auth.accept_invite(&reset, "another long password", later)
        .unwrap();
    assert!(
        auth.current(&session, later).is_none(),
        "再設定後も前のログインが生きている"
    );
    assert!(matches!(
        auth.login(
            "a@example.com",
            "another long password",
            Some(&device),
            "ip",
            later
        ),
        LoginResult::CodeRequired { .. }
    ));

    let (session, _) = {
        let LoginResult::CodeRequired {
            challenge, code, ..
        } = auth.login("a@example.com", "another long password", None, "ip", later)
        else {
            panic!()
        };
        match auth.verify_code(&challenge, &code, true, later) {
            CodeResult::SignedIn { session, device } => (session, device),
            other => panic!("{other:?}"),
        }
    };
    auth.disable(&id, later).unwrap();
    assert!(auth.current(&session, later).is_none());
    assert!(matches!(
        auth.login("a@example.com", "another long password", None, "ip", later),
        LoginResult::Invalid
    ));
    // 戻せばログインできる (端末は覚えていないのでコードを聞く)
    auth.enable(&id).unwrap();
    assert!(matches!(
        auth.login("a@example.com", "another long password", None, "ip", later),
        LoginResult::CodeRequired { .. }
    ));
}

#[test]
fn 追加の送り先にもコードを送り_ログイン中の画面は12時間で切れる() {
    let auth = PortalAuth::new(store_c1(), cfg());
    let id = ready(&auth, "a@example.com");
    auth.set_extra_email(&id, "Phone@Example.jp").unwrap();
    assert!(auth.set_extra_email(&id, "a@example.com").is_err());
    let LoginResult::CodeRequired {
        to,
        challenge,
        code,
    } = auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    assert_eq!(
        to,
        vec!["a@example.com".to_string(), "phone@example.jp".to_string()]
    );
    let CodeResult::SignedIn { session, .. } = auth.verify_code(&challenge, &code, false, t0())
    else {
        panic!()
    };
    assert!(auth.current(&session, t0() + Duration::hours(11)).is_some());
    assert!(auth.current(&session, t0() + Duration::hours(12)).is_none());
    assert_eq!(auth::mask_email("taro@example.com"), "t***@example.com");
}

#[test]
fn 保存先にはハッシュだけを置く() {
    let store = store_c1();
    let auth = PortalAuth::new(store.clone(), cfg());
    let (a, token) = auth.create_account("a@example.com", "c1", t0()).unwrap();
    assert!(store.invite(&token).is_none());
    assert!(store.invite(&auth::hash_token(&token)).is_some());
    auth.accept_invite(&token, PW, t0()).unwrap();
    let saved = store.account(&a.id).unwrap();
    assert!(saved
        .password_hash
        .as_deref()
        .is_some_and(|h| h.starts_with("$2") && !h.contains(PW)));
    assert_eq!(saved.status, AccountStatus::Active);
    let (session, device) = code_login(&auth, "a@example.com", true);
    assert!(store.session(&session).is_none());
    assert!(store.device(&device.unwrap()).is_none());
}

// ---- HTTP ----

#[derive(Default)]
struct FakeMailer {
    sent: Mutex<Vec<(Vec<String>, String)>>,
    fail: bool,
}

impl Mailer for FakeMailer {
    fn send<'a>(&'a self, _: &'a [String], _: &'a str, _: &'a str) -> MailFuture<'a> {
        Box::pin(async { Err(MailError) })
    }
    fn send_code<'a>(&'a self, to: &'a [String], code: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            if self.fail {
                return Err(MailError);
            }
            self.sent
                .lock()
                .unwrap()
                .push((to.to_vec(), code.to_string()));
            Ok(())
        })
    }
}

fn test_state() -> Arc<AppState> {
    Arc::new(AppState {
        config: crate::config::AppConfig::from_env(),
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: crate::db::cache::AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        crm_hubspot: None,
        hubspot: None,
    })
}

struct Harness {
    app: Router,
    auth: Arc<PortalAuth>,
    mailer: Arc<FakeMailer>,
}

fn harness() -> Harness {
    let auth = Arc::new(PortalAuth::new(store_c1(), cfg()));
    let mailer = Arc::new(FakeMailer::default());
    let portal = Arc::new(Portal {
        auth: auth.clone(),
        mailer: mailer.clone(),
    });
    let app = router_with(Some(portal), CrmAccess::from_list(""), None).with_state(test_state());
    Harness { app, auth, mailer }
}

async fn call(
    app: &Router,
    method: &str,
    uri: &str,
    cookies: &[String],
    body: Option<Value>,
    fetch: bool,
) -> (StatusCode, Vec<String>, Value) {
    let mut b = Request::builder().method(method).uri(uri);
    if fetch {
        b = b.header("x-requested-with", "fetch");
    }
    if !cookies.is_empty() {
        b = b.header(header::COOKIE, cookies.join("; "));
    }
    let body = match body {
        Some(v) => {
            b = b.header(header::CONTENT_TYPE, "application/json");
            Body::from(v.to_string())
        }
        None => Body::empty(),
    };
    let res = app.clone().oneshot(b.body(body).unwrap()).await.unwrap();
    let status = res.status();
    let set: Vec<String> = res
        .headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .map(|v| v.to_str().unwrap().to_string())
        .collect();
    let bytes = to_bytes(res.into_body(), 1 << 20).await.unwrap();
    let v = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    (status, set, v)
}

fn pair(set: &[String], name: &str) -> Option<String> {
    set.iter()
        .find(|c| c.starts_with(&format!("{name}=")))
        .map(|c| c.split(';').next().unwrap().to_string())
        .filter(|c| !c.ends_with('='))
}

/// 招待 → パスワード → ログイン → コード までを HTTP で通し、顧客の Cookie を返す
async fn sign_in(h: &Harness) -> String {
    let (_, token) = h
        .auth
        .create_account("hanako@customer.example", "c1", Utc::now())
        .unwrap();
    let (s, _, v) = call(
        &h.app,
        "POST",
        "/api/portal/invite/check",
        &[],
        Some(serde_json::json!({ "token": token })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["email"], "hanako@customer.example");
    assert_eq!(v["purpose"], "initial");
    let (s, _, _) = call(
        &h.app,
        "POST",
        "/api/portal/invite/accept",
        &[],
        Some(serde_json::json!({ "token": token, "password": PW })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);

    let login = serde_json::json!({ "email": "hanako@customer.example", "password": PW });
    let (s, set, v) = call(&h.app, "POST", "/api/portal/login", &[], Some(login), true).await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["status"], "code_required");
    assert_eq!(v["to"][0], "h***@customer.example");
    let ch = pair(&set, CHALLENGE_COOKIE).expect("やり取りの Cookie が無い");
    let challenge_cookie = set
        .iter()
        .find(|c| c.starts_with(CHALLENGE_COOKIE))
        .unwrap();
    assert!(
        challenge_cookie.contains("HttpOnly")
            && challenge_cookie.contains("Secure")
            && challenge_cookie.contains("SameSite=Strict")
    );
    let code = h.mailer.sent.lock().unwrap().last().unwrap().1.clone();
    let (s, set, _) = call(
        &h.app,
        "POST",
        "/api/portal/login/code",
        &[ch],
        Some(serde_json::json!({ "code": code })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let session_cookie = set.iter().find(|c| c.starts_with(SESSION_COOKIE)).unwrap();
    assert!(
        session_cookie.contains("HttpOnly")
            && session_cookie.contains("Secure")
            && session_cookie.contains("Path=/")
    );
    assert!(pair(&set, DEVICE_COOKIE).is_some(), "覚えた端末の札が無い");
    pair(&set, SESSION_COOKIE).unwrap()
}

#[tokio::test]
async fn httpで招待からログイン_ログアウトまで() {
    let h = harness();
    let session = sign_in(&h).await;
    let (s, _, v) = call(
        &h.app,
        "GET",
        "/api/portal/me",
        std::slice::from_ref(&session),
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["customer_id"], "c1");
    let (s, set, _) = call(
        &h.app,
        "POST",
        "/api/portal/logout",
        std::slice::from_ref(&session),
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert!(set
        .iter()
        .any(|c| c.starts_with(&format!("{SESSION_COOKIE}=;")) && c.contains("Max-Age=0")));
    let (s, _, _) = call(&h.app, "GET", "/api/portal/me", &[session], None, false).await;
    assert_eq!(
        s,
        StatusCode::UNAUTHORIZED,
        "ログアウトした Cookie を送り直すと入れた"
    );
}

#[tokio::test]
async fn httpの失敗は登録の有無を明かさず_csrfが無い書き込みは断る() {
    let h = harness();
    sign_in(&h).await;
    let wrong =
        serde_json::json!({ "email": "hanako@customer.example", "password": "wrong password!!" });
    let nobody =
        serde_json::json!({ "email": "nobody@customer.example", "password": "wrong password!!" });
    let (s1, _, v1) = call(
        &h.app,
        "POST",
        "/api/portal/login",
        &[],
        Some(wrong.clone()),
        true,
    )
    .await;
    let (s2, _, v2) = call(&h.app, "POST", "/api/portal/login", &[], Some(nobody), true).await;
    assert_eq!(
        (s1, s2),
        (StatusCode::UNAUTHORIZED, StatusCode::UNAUTHORIZED)
    );
    assert_eq!(v1, v2);
    assert_eq!(v1["message"], "メールアドレスかパスワードが違います");
    let (s, _, _) = call(&h.app, "POST", "/api/portal/login", &[], Some(wrong), false).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn コードを送れなければ503でやり取りを残さない() {
    let auth = Arc::new(PortalAuth::new(store_c1(), cfg()));
    let (_, token) = auth
        .create_account("a@example.com", "c1", Utc::now())
        .unwrap();
    auth.accept_invite(&token, PW, Utc::now()).unwrap();
    let portal = Arc::new(Portal {
        auth,
        mailer: Arc::new(FakeMailer {
            fail: true,
            ..Default::default()
        }),
    });
    let app = router_with(Some(portal), CrmAccess::from_list(""), None).with_state(test_state());
    let (s, set, v) = call(
        &app,
        "POST",
        "/api/portal/login",
        &[],
        Some(serde_json::json!({ "email": "a@example.com", "password": PW })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["error"], "mail_failed");
    assert!(pair(&set, CHALLENGE_COOKIE).is_none());
}

#[tokio::test]
async fn 準備が無ければ503_応答は保存させず枠に入れさせない() {
    let app = router_with(None, CrmAccess::from_list(""), None).with_state(test_state());
    let res = app
        .clone()
        .oneshot(Request::get("/api/portal/me").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
    assert!(res.headers()[header::CONTENT_SECURITY_POLICY]
        .to_str()
        .unwrap()
        .contains("frame-ancestors 'none'"));
    let page = app
        .oneshot(Request::get("/portal/login").body(Body::empty()).unwrap())
        .await
        .unwrap();
    assert_eq!(page.status(), StatusCode::OK);
    assert_eq!(page.headers()["x-robots-tag"], "noindex, nofollow");
}

#[tokio::test]
async fn 社内の管理apiは社内のログインが無ければ入れない() {
    let h = harness();
    let session = sign_in(&h).await;
    // 本物の組み立て (社内のセッションの層がある) で叩く
    let app = crate::build_app(test_state());
    for (m, uri) in [
        ("GET", "/api/admin/portal/accounts"),
        ("POST", "/api/admin/portal/accounts"),
        ("POST", "/api/admin/portal/accounts/x/invite"),
        ("POST", "/api/admin/portal/accounts/x/disable"),
    ] {
        let (s, _, _) = call(
            &app,
            m,
            uri,
            std::slice::from_ref(&session),
            Some(serde_json::json!({ "email": "x@y.z", "customer_id": "c" })),
            true,
        )
        .await;
        assert_eq!(s, StatusCode::UNAUTHORIZED, "{m} {uri}");
    }
}

/// 顧客の Cookie で社内の全ルートを叩き、どれも中身を返さないこと (計画 7 章)。
/// ルートは src/ の `.route("...")` を読んで作る (足されたルートも自動で対象になる)。
#[tokio::test]
async fn 顧客のcookieでは社内のルートに入れない() {
    let h = harness();
    let session = sign_in(&h).await;
    // 顧客側で本当にログインできている Cookie であること
    assert_eq!(
        call(
            &h.app,
            "GET",
            "/api/portal/me",
            std::slice::from_ref(&session),
            None,
            false
        )
        .await
        .0,
        StatusCode::OK
    );
    let device = String::from("__Host-portal_device=x");

    let mut paths = std::collections::BTreeSet::new();
    let mut stack = vec![std::path::PathBuf::from("src")];
    while let Some(dir) = stack.pop() {
        for e in std::fs::read_dir(&dir).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|x| x == "rs") {
                let text = std::fs::read_to_string(&p).unwrap();
                for part in text.split(".route(\"").skip(1) {
                    if let Some(path) = part.split('"').next() {
                        paths.insert(path.to_string());
                    }
                }
            }
        }
    }
    let internal: Vec<String> = paths
        .into_iter()
        .filter(|p| {
            ["/app", "/api/", "/tab/", "/admin", "/my", "/report"]
                .iter()
                .any(|x| p.starts_with(x))
        })
        // 認証の要らない公開の入口 (MCP 連携・顧客の入口・送ったメールの計測)
        .filter(|p| {
            !p.starts_with("/api/v1")
                && !p.starts_with("/api/portal/")
                && !p.starts_with("/api/crm/email/t/")
        })
        .map(|p| {
            p.split('/')
                .map(|seg| if seg.starts_with('{') { "1" } else { seg })
                .collect::<Vec<_>>()
                .join("/")
        })
        .collect();
    assert!(
        internal.len() > 100,
        "ルートを読めていない: {}",
        internal.len()
    );

    let app = crate::build_app(test_state());
    let mut leaked = Vec::new();
    for path in &internal {
        for method in ["GET", "POST"] {
            let req = Request::builder()
                .method(method)
                .uri(path)
                .header(header::COOKIE, format!("{session}; {device}"))
                .header("accept", "application/json")
                .header("x-requested-with", "fetch")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .unwrap();
            let res = app.clone().oneshot(req).await.unwrap();
            let s = res.status();
            let to_login = s.is_redirection()
                && res
                    .headers()
                    .get(header::LOCATION)
                    .and_then(|v| v.to_str().ok())
                    .is_some_and(|l| l.starts_with("/login"));
            if s.is_success() || (s.is_redirection() && !to_login) {
                leaked.push(format!("{method} {path} → {s}"));
            }
        }
    }
    assert!(
        leaked.is_empty(),
        "顧客の Cookie で中身が返った: {leaked:#?}"
    );
}

/// テスト用: 社内のセッションに Google ログインした人を入れる
async fn inject_staff(
    session: tower_sessions::Session,
    axum::Json(v): axum::Json<Value>,
) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, v["email"].as_str().unwrap())
        .await
        .unwrap();
    session
        .insert(
            crate::auth::SESSION_LOGIN_METHOD_KEY,
            crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
        )
        .await
        .unwrap();
    StatusCode::NO_CONTENT
}

#[tokio::test]
async fn 社内の管理者は作る_招待を出し直す_止める_送り先を足すができる() {
    use crate::crm::rbac::CrmRole;
    const BOSS: &str = "boss@f-a-c.co.jp";
    const STAFF: &str = "staff@f-a-c.co.jp";
    let auth = Arc::new(PortalAuth::new(store_c1(), cfg()));
    let portal = Arc::new(Portal {
        auth: auth.clone(),
        mailer: Arc::new(FakeMailer::default()),
    });
    let access = CrmAccess::from_list(&format!("{BOSS},{STAFF}"))
        .with_test_role(BOSS, CrmRole::Admin)
        .with_test_role(STAFF, CrmRole::Bpo);
    let app = Router::new()
        .merge(router_with(Some(portal), access, None))
        .route("/__test/session", axum::routing::post(inject_staff))
        .with_state(test_state())
        .layer(tower_sessions::SessionManagerLayer::new(
            tower_sessions::MemoryStore::default(),
        ));
    let mut cookies = std::collections::HashMap::new();
    for who in [BOSS, STAFF] {
        let (_, set, _) = call(
            &app,
            "POST",
            "/__test/session",
            &[],
            Some(serde_json::json!({ "email": who })),
            false,
        )
        .await;
        cookies.insert(who, pair(&set, "id").expect("社内のセッション"));
    }
    let boss = vec![cookies[BOSS].clone()];

    // 管理者でない社内の人は使えない
    let (s, _, _) = call(
        &app,
        "GET",
        "/api/admin/portal/accounts",
        &[cookies[STAFF].clone()],
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::FORBIDDEN);

    let (s, _, v) = call(
        &app,
        "POST",
        "/api/admin/portal/accounts",
        &boss,
        Some(serde_json::json!({ "email": " Hanako@Customer.example ", "customer_id": "c1" })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["account"]["email"], "hanako@customer.example");
    assert_eq!(v["account"]["status"], "invited");
    let id = v["account"]["id"].as_str().unwrap().to_string();
    let first = v["invite_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/portal/invite#t=")
        .unwrap()
        .to_string();
    let (s, _, _) = call(
        &app,
        "POST",
        "/api/admin/portal/accounts",
        &boss,
        Some(serde_json::json!({ "email": "hanako@customer.example", "customer_id": "c1" })),
        true,
    )
    .await;
    assert_eq!(s, StatusCode::CONFLICT);

    // 出し直すと前のリンクは使えない
    let (s, _, v) = call(
        &app,
        "POST",
        &format!("/api/admin/portal/accounts/{id}/invite"),
        &boss,
        None,
        true,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    let second = v["invite_path"]
        .as_str()
        .unwrap()
        .strip_prefix("/portal/invite#t=")
        .unwrap()
        .to_string();
    assert!(auth.invite_info(&first, Utc::now()).is_err());
    assert!(auth.invite_info(&second, Utc::now()).is_ok());

    let (s, _, v) = call(
        &app,
        "PUT",
        &format!("/api/admin/portal/accounts/{id}/extra-email"),
        &boss,
        Some(serde_json::json!({ "email": "phone@example.jp" })),
        true,
    )
    .await;
    assert_eq!(
        (s, v["extra_code_email"].as_str()),
        (StatusCode::OK, Some("phone@example.jp"))
    );

    let (s, _, v) = call(
        &app,
        "POST",
        &format!("/api/admin/portal/accounts/{id}/disable"),
        &boss,
        None,
        true,
    )
    .await;
    assert_eq!(
        (s, v["status"].as_str()),
        (StatusCode::OK, Some("disabled"))
    );
    // 止めたら招待のリンクも使えない
    assert!(auth.invite_info(&second, Utc::now()).is_err());

    let (s, _, v) = call(
        &app,
        "GET",
        "/api/admin/portal/accounts",
        &boss,
        None,
        false,
    )
    .await;
    assert_eq!(s, StatusCode::OK);
    assert_eq!(v["accounts"].as_array().unwrap().len(), 1);
    // 一覧にパスワード・招待の値は出さない
    let text = v.to_string();
    assert!(!text.contains("password") && !text.contains(&second));
}

/// ログインのとき、古く読んだアカウントを丸ごと書き戻さない (最後のログインの日時だけを変える)。
/// ログインの途中 (セッションを作る瞬間) に管理者がアカウントを止めた場合に、止めたのが戻らないこと
#[test]
fn ログインは最後のログインの日時だけを変えて管理者の停止を戻さない() {
    struct Racy {
        inner: Arc<MemoryStore>,
    }
    impl PortalStore for Racy {
        fn accounts(&self) -> Vec<store::Account> {
            self.inner.accounts()
        }
        fn account(&self, id: &str) -> Option<store::Account> {
            self.inner.account(id)
        }
        fn account_by_email(&self, e: &str) -> Option<store::Account> {
            self.inner.account_by_email(e)
        }
        fn insert_account(&self, a: store::Account) -> Result<(), store::StoreError> {
            self.inner.insert_account(a)
        }
        fn update_account(&self, a: store::Account) -> Result<(), store::StoreError> {
            self.inner.update_account(a)
        }
        fn touch_login(&self, id: &str, now: DateTime<Utc>) {
            self.inner.touch_login(id, now)
        }
        fn insert_invite(&self, i: store::Invite) {
            self.inner.insert_invite(i)
        }
        fn invite(&self, h: &str) -> Option<store::Invite> {
            self.inner.invite(h)
        }
        fn use_invite(&self, h: &str, now: DateTime<Utc>) -> bool {
            self.inner.use_invite(h, now)
        }
        fn revoke_invites(&self, id: &str) {
            self.inner.revoke_invites(id)
        }
        fn insert_device(&self, d: store::Device) {
            self.inner.insert_device(d)
        }
        fn device(&self, h: &str) -> Option<store::Device> {
            self.inner.device(h)
        }
        fn touch_device(&self, h: &str, now: DateTime<Utc>) {
            self.inner.touch_device(h, now)
        }
        fn revoke_devices(&self, id: &str) {
            self.inner.revoke_devices(id)
        }
        fn insert_session(&self, s: store::Session) {
            // セッションを作る瞬間に、管理者がアカウントを止めた
            let mut a = self.inner.account(&s.account_id).unwrap();
            a.status = AccountStatus::Disabled;
            self.inner.update_account(a).unwrap();
            self.inner.insert_session(s)
        }
        fn session(&self, h: &str) -> Option<store::Session> {
            self.inner.session(h)
        }
        fn end_session(&self, h: &str, now: DateTime<Utc>) {
            self.inner.end_session(h, now)
        }
        fn end_sessions(&self, id: &str, now: DateTime<Utc>) {
            self.inner.end_sessions(id, now)
        }
        fn customers(&self) -> Vec<store::Customer> {
            self.inner.customers()
        }
        fn customer(&self, id: &str) -> Option<store::Customer> {
            self.inner.customer(id)
        }
        fn insert_customer(&self, c: store::Customer) -> Result<(), store::StoreError> {
            self.inner.insert_customer(c)
        }
        fn update_customer(&self, c: store::Customer) -> Result<(), store::StoreError> {
            self.inner.update_customer(c)
        }
        fn sheets(&self, c: Option<&str>) -> Vec<store::SheetReg> {
            self.inner.sheets(c)
        }
        fn sheet(&self, id: &str) -> Option<store::SheetReg> {
            self.inner.sheet(id)
        }
        fn insert_sheet(&self, s: store::SheetReg) -> Result<(), store::StoreError> {
            self.inner.insert_sheet(s)
        }
        fn update_sheet(&self, s: store::SheetReg) -> Result<(), store::StoreError> {
            self.inner.update_sheet(s)
        }
        fn columns(&self, id: &str) -> Vec<store::ColumnSetting> {
            self.inner.columns(id)
        }
        fn replace_columns(&self, id: &str, c: Vec<store::ColumnSetting>) {
            self.inner.replace_columns(id, c)
        }
        fn append_log(&self, e: store::LogEntry) {
            self.inner.append_log(e)
        }
        fn recent_logs(&self, n: usize) -> Vec<store::LogEntry> {
            self.inner.recent_logs(n)
        }
        fn status(&self) -> store::StoreStatus {
            self.inner.status()
        }
    }
    let inner = store_c1();
    let plain = PortalAuth::new(inner.clone(), cfg());
    let (a, token) = plain.create_account("a@example.com", "c1", t0()).unwrap();
    plain.accept_invite(&token, PW, t0()).unwrap();
    let auth = PortalAuth::new(
        Arc::new(Racy {
            inner: inner.clone(),
        }),
        cfg(),
    );
    let LoginResult::CodeRequired {
        challenge, code, ..
    } = auth.login("a@example.com", PW, None, "ip", t0())
    else {
        panic!()
    };
    let CodeResult::SignedIn { session, .. } = auth.verify_code(&challenge, &code, false, t0())
    else {
        panic!()
    };
    let saved = inner.account(&a.id).unwrap();
    assert_eq!(
        saved.status,
        AccountStatus::Disabled,
        "止めたのが古い値で戻っていない"
    );
    assert_eq!(saved.last_login_at, Some(t0()), "最後のログインは入る");
    assert!(auth.current(&session, t0()).is_none());
}

#[path = "admin_tests.rs"]
mod admin_tests;

#[path = "view_tests.rs"]
mod view_tests;

#[path = "mail_tests.rs"]
mod mail_tests;
