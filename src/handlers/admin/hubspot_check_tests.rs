//! `/api/admin/hubspot-check` のテスト。本物の HubSpot には通信しない
//! (127.0.0.1 に偽 HubSpot を立て、呼び出し回数・受信内容を記録する)。

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use super::{run_check, REQUIRED_SCOPES};
use crate::audit::fake_turso::start_sqlite_audit;
use crate::handlers::admin::contract_tests::{
    get, get_json, login, seed_fixtures, test_state, ADMIN_EMAIL, USER_EMAIL,
};
use crate::hubspot::{ClientOptions, HubSpotClient};

/// 偽の鍵。どの応答・ログにも現れてはならない
const TOKEN: &str = "pat-na1-CANARY-0123456789-abcdef";
const PATH: &str = "/oauth/v2/private-apps/get/access-token-info";
const API: &str = "/api/admin/hubspot-check";

struct Fake {
    calls: AtomicUsize,
    /// (path, authorization, body)
    bodies: Mutex<Vec<(String, String, Value)>>,
    status: u16,
    body: String,
    delay: Duration,
}

async fn handle(
    axum::extract::State(f): axum::extract::State<Arc<Fake>>,
    req: Request<Body>,
) -> Response {
    f.calls.fetch_add(1, Ordering::SeqCst);
    let (parts, body) = req.into_parts();
    let bytes = axum::body::to_bytes(body, 1 << 20).await.unwrap();
    f.bodies.lock().unwrap().push((
        parts.uri.path().to_string(),
        parts
            .headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
            .to_string(),
        serde_json::from_slice(&bytes).unwrap_or(Value::Null),
    ));
    if !f.delay.is_zero() {
        tokio::time::sleep(f.delay).await;
    }
    Response::builder()
        .status(StatusCode::from_u16(f.status).unwrap())
        .header("content-type", "application/json")
        .body(Body::from(f.body.clone()))
        .unwrap()
}

async fn spawn_fake(status: u16, body: Value, delay: Duration) -> (String, Arc<Fake>) {
    let fake = Arc::new(Fake {
        calls: AtomicUsize::new(0),
        bodies: Mutex::new(vec![]),
        status,
        body: body.to_string(),
        delay,
    });
    let router = Router::new().fallback(handle).with_state(fake.clone());
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    (format!("http://{addr}"), fake)
}

fn client(base: &str) -> HubSpotClient {
    HubSpotClient::new(TOKEN.into(), base, ClientOptions::default()).unwrap()
}

fn ok_body(scopes: &[&str]) -> Value {
    json!({"hubId": 23708633, "userId": 1, "appId": 2, "scopes": scopes, "tokenType": "pat"})
}

/// admin / 一般ユーザーでログインできる app を作る (偽 Turso + build_app)
async fn app_with(hubspot: Option<HubSpotClient>) -> Router {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let mut state = test_state(Some(audit));
    Arc::get_mut(&mut state).unwrap().hubspot = hubspot.map(Arc::new);
    crate::build_app(state)
}

#[tokio::test(flavor = "multi_thread")]
async fn 非管理者は403で_未ログインは401でhubspotを呼ばない() {
    let (base, fake) = spawn_fake(200, ok_body(&REQUIRED_SCOPES), Duration::ZERO).await;
    let app = app_with(Some(client(&base))).await;

    // 未ログイン: 既存の規約どおり、Accept: application/json なら 401 JSON、ブラウザの直打ちは /login へ 303
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .uri(API)
                .header("accept", "application/json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let (status, _h, body) = get(&app, API, None).await;
    assert_eq!(status, StatusCode::SEE_OTHER, "{body}");

    let cookie = login(&app, USER_EMAIL).await;
    let (status, _h, body) = get(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");

    assert_eq!(
        fake.calls.load(Ordering::SeqCst),
        0,
        "HubSpot を呼んではいけない"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者は必要scopeの有無を見られ_2回目はキャッシュで呼び出しは1回() {
    let (base, fake) = spawn_fake(200, ok_body(&REQUIRED_SCOPES), Duration::ZERO).await;
    let app = app_with(Some(client(&base))).await;
    let cookie = login(&app, ADMIN_EMAIL).await;

    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["configured"], true);
    assert_eq!(v["portal_id"], "23708633");
    assert_eq!(v["all_required_present"], true);
    assert_eq!(v["cached"], false);
    assert_eq!(v["scopes"].as_array().unwrap().len(), 3);
    let req = v["required"].as_array().unwrap();
    assert_eq!(req.len(), 3);
    assert!(req.iter().all(|r| r["present"] == true), "{v}");
    assert!(v["checked_at"].as_str().unwrap().contains('T'));

    let (_s, v2) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(v2["cached"], true);
    assert_eq!(v2["checked_at"], v["checked_at"]);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1, "5 分キャッシュ");

    let rec = fake.bodies.lock().unwrap().clone();
    assert_eq!(rec[0].0, PATH);
    assert_eq!(rec[0].2["tokenKey"], TOKEN);
}

#[tokio::test(flavor = "multi_thread")]
async fn scope不足はどれが無いか分かる() {
    let (base, _f) = spawn_fake(
        200,
        ok_body(&["crm.objects.deals.read", "crm.objects.contacts.read"]),
        Duration::ZERO,
    )
    .await;
    let app = app_with(Some(client(&base))).await;
    let cookie = login(&app, ADMIN_EMAIL).await;
    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["all_required_present"], false);
    let present = |name: &str| {
        v["required"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["scope"] == name)
            .unwrap()["present"]
            .clone()
    };
    assert_eq!(present("crm.objects.deals.read"), true);
    assert_eq!(present("crm.objects.owners.read"), false);
    assert_eq!(present("crm.schemas.deals.read"), false);
}

#[tokio::test(flavor = "multi_thread")]
async fn 鍵未設定はconfigured_falseで503() {
    let app = app_with(None).await;
    let cookie = login(&app, ADMIN_EMAIL).await;
    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(v["configured"], false);
    assert_eq!(v["error_kind"], "not_configured");
    assert!(v["scopes"].is_null());
}

#[tokio::test(flavor = "multi_thread")]
async fn hubspotが401なら鍵が無効としてhubspot_auth() {
    let (base, fake) = spawn_fake(
        401,
        json!({"status":"error","message": format!("Invalid token {TOKEN}")}),
        Duration::ZERO,
    )
    .await;
    let app = app_with(Some(client(&base))).await;
    let cookie = login(&app, ADMIN_EMAIL).await;
    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY, "{v}");
    assert_eq!(v["configured"], true);
    assert_eq!(v["error_kind"], "hubspot_auth");
    assert!(v["scopes"].is_null());
    assert!(v["all_required_present"].is_null());
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1, "retry しない");
    assert!(!v.to_string().contains(TOKEN));
}

#[tokio::test(flavor = "multi_thread")]
async fn 種類の合わない鍵の400はhubspot_upstream() {
    let (base, _f) = spawn_fake(400, json!({"message":"bad"}), Duration::ZERO).await;
    let app = app_with(Some(client(&base))).await;
    let cookie = login(&app, ADMIN_EMAIL).await;
    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(v["error_kind"], "hubspot_upstream");
}

#[tokio::test(flavor = "multi_thread")]
async fn タイムアウトはhubspot_timeoutで再試行しない() {
    let (base, fake) =
        spawn_fake(200, ok_body(&REQUIRED_SCOPES), Duration::from_millis(1500)).await;
    let c = client(&base);
    let (status, r) = run_check(Some(&c), Duration::from_millis(200)).await;
    assert_eq!(status, StatusCode::BAD_GATEWAY);
    assert_eq!(r.error_kind.as_deref(), Some("hubspot_timeout"));
    assert_eq!(fake.calls.load(Ordering::SeqCst), 1);
}

/// HubSpot が鍵を本文に反映して返す失敗系も含め、すべての応答に鍵が出ない
#[tokio::test(flavor = "current_thread")]
async fn 応答に鍵の値が出ない() {
    let mut all = String::new();
    for (status, body) in [
        (200, ok_body(&REQUIRED_SCOPES)),
        (401, json!({"message": format!("token {TOKEN} invalid")})),
        (500, json!({"message": TOKEN})),
        (200, json!({"scopes": "not-an-array", "echo": TOKEN})),
    ] {
        let (base, _f) = spawn_fake(status, body, Duration::ZERO).await;
        let c = client(&base);
        let (st, r) = run_check(Some(&c), Duration::from_secs(5)).await;
        all.push_str(&format!(
            "{st} {r:?}
{}
",
            serde_json::to_string(&r).unwrap()
        ));
        // 2 回目 (キャッシュ経路)
        let (_st, r) = run_check(Some(&c), Duration::from_secs(5)).await;
        all.push_str(&serde_json::to_string(&r).unwrap());
    }
    assert!(!all.contains(TOKEN), "応答に鍵: {all}");
    assert!(!all.contains("CANARY"), "応答に鍵の一部: {all}");
    // Debug 出力 (HubSpotClient) にも出ない
    assert!(!format!("{:?}", client("http://127.0.0.1:1")).contains(TOKEN));
}

/// 失敗時のログに出すのは `error_kind()` だけ。これは固定の識別子なので鍵を含みえない。
/// (以前は tracing の出力を横取りして調べていたが、並行する他テストと callsite の
///  interest キャッシュを共有するため CI で不安定だった。2026-10-06)
#[test]
fn ログに出す値は固定の識別子だけ() {
    use crate::hubspot::HubSpotError;
    let kinds = [
        HubSpotError::NotConfigured.error_kind(),
        HubSpotError::NotFound.error_kind(),
        HubSpotError::RateLimited.error_kind(),
        HubSpotError::Timeout.error_kind(),
    ];
    for k in kinds {
        assert!(k.chars().all(|c| c.is_ascii_lowercase() || c == '_'), "{k}");
    }
    let src = include_str!("hubspot_check.rs");
    let logs: Vec<&str> = src.lines().filter(|l| l.contains("tracing::")).collect();
    assert_eq!(logs.len(), 1, "{logs:?}");
    assert!(
        logs[0].contains("error_kind = e.error_kind()") && !logs[0].contains("{"),
        "{}",
        logs[0]
    );
}
