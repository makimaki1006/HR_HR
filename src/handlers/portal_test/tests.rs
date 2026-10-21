//! 認可・シートの限定・書き込みの確認が、Google を呼ぶ前に効くことを確かめる。
//! (Google の鍵は渡さない。鍵が要る所まで来たら 503 になるので、そこで止まったことも分かる)

use super::*;
use axum::body::{to_bytes, Body};
use axum::http::Request;
use axum::routing::post as post_route;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, SessionManagerLayer};

const ME: &str = "tester@f-a-c.co.jp";

fn test_state() -> Arc<AppState> {
    let mut config = crate::config::AppConfig::from_env();
    config.allowed_domains = vec!["f-a-c.co.jp".into()];
    Arc::new(AppState {
        config,
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

#[derive(Deserialize)]
struct Inject {
    email: String,
    method: String,
}

async fn inject(session: Session, Json(v): Json<Inject>) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, v.email)
        .await
        .unwrap();
    session
        .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, v.method)
        .await
        .unwrap();
    StatusCode::NO_CONTENT
}

fn app() -> Router {
    Router::new()
        .merge(router_with(CrmAccess::from_list(""), None))
        .route("/__test/session", post_route(inject))
        .with_state(test_state())
        .layer(SessionManagerLayer::new(MemoryStore::default()))
}

async fn login(app: &Router, email: &str, method: &str) -> String {
    let resp = app
        .clone()
        .oneshot(
            Request::post("/__test/session")
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from(
                    json!({"email": email, "method": method}).to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::NO_CONTENT);
    resp.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string()
}

async fn call(app: &Router, req: Request<Body>) -> (StatusCode, Option<String>, Value) {
    let resp = app.clone().oneshot(req).await.unwrap();
    let status = resp.status();
    let cache = resp
        .headers()
        .get(header::CACHE_CONTROL)
        .map(|v| v.to_str().unwrap().to_string());
    let body = to_bytes(resp.into_body(), 1 << 20).await.unwrap();
    (
        status,
        cache,
        serde_json::from_slice(&body).unwrap_or(Value::Null),
    )
}

fn get_req(uri: &str, cookie: Option<&str>) -> Request<Body> {
    let mut b = Request::get(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    b.body(Body::empty()).unwrap()
}

fn cell_req(cookie: &str, body: Value, requested_with: bool) -> Request<Body> {
    let mut b = Request::post("/api/portal-test/cell")
        .header(header::COOKIE, cookie)
        .header(header::CONTENT_TYPE, "application/json");
    if requested_with {
        b = b.header("X-Requested-With", "fetch");
    }
    b.body(Body::from(body.to_string())).unwrap()
}

fn cell_body(header: &str, row: usize) -> Value {
    json!({
        "sid": sheet::ALLOWED_SPREADSHEET_ID,
        "gid": sheet::ALLOWED_SHEET_ID,
        "rowNumber": row,
        "header": header,
        "base": "",
        "next": "x",
    })
}

const SHEET_URI: &str =
    "/api/portal-test/sheet?sid=1m5DRb117MXqCQw4GhJC21spHE6N3lqE0dtqP7zxwLeg&gid=510300204";

#[tokio::test]
async fn 未ログインはjsonの401でキャッシュさせない() {
    let app = app();
    let (s, cache, body) = call(&app, get_req(SHEET_URI, None)).await;
    assert_eq!(s, StatusCode::UNAUTHORIZED);
    assert_eq!(cache.as_deref(), Some("no-store"));
    assert_eq!(body["error_kind"], "login_required");
}

#[tokio::test]
async fn パスワードのログインや社外のドメインは403() {
    let app = app();
    let c = login(&app, ME, crate::auth::LOGIN_METHOD_PASSWORD).await;
    let (s, _, body) = call(&app, get_req(SHEET_URI, Some(&c))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error_kind"], "google_login_required");

    let c = login(&app, "x@example.com", crate::auth::LOGIN_METHOD_GOOGLE_OIDC).await;
    let (s, _, body) = call(&app, get_req("/api/portal-test/sa", Some(&c))).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error_kind"], "forbidden");
}

#[tokio::test]
async fn 決めたシート以外は403() {
    let app = app();
    let c = login(&app, ME, crate::auth::LOGIN_METHOD_GOOGLE_OIDC).await;
    for uri in [
        "/api/portal-test/sheet?sid=other&gid=510300204",
        "/api/portal-test/sheet?sid=1m5DRb117MXqCQw4GhJC21spHE6N3lqE0dtqP7zxwLeg&gid=0",
        "/api/portal-test/version?sid=other",
    ] {
        let (s, cache, body) = call(&app, get_req(uri, Some(&c))).await;
        assert_eq!(s, StatusCode::FORBIDDEN, "{uri}");
        assert_eq!(cache.as_deref(), Some("no-store"));
        assert_eq!(body["error"], "このシートは読み書きできません");
    }
    // 決めたシートなら鍵の確認まで進む (鍵を渡していないので 503)
    let (s, _, body) = call(&app, get_req(SHEET_URI, Some(&c))).await;
    assert_eq!(s, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(
        body["error"],
        "サービスアカウントの鍵（GOOGLE_SA_KEY_B64）が設定されていません"
    );
}

#[tokio::test]
async fn 書き込みはx_requested_withと書ける見出しが要る() {
    let app = app();
    let c = login(&app, ME, crate::auth::LOGIN_METHOD_GOOGLE_OIDC).await;

    let (s, _, _) = call(&app, cell_req(&c, cell_body("状況", 3), false)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "X-Requested-With なし");

    let (s, _, body) = call(&app, cell_req(&c, cell_body("氏名", 3), true)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "この項目は画面から書けません");

    let (s, _, body) = call(&app, cell_req(&c, cell_body("状況", 2), true)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "この行には書けません");

    let mut other = cell_body("状況", 3);
    other["sid"] = json!("other");
    let (s, _, body) = call(&app, cell_req(&c, other, true)).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(body["error"], "このシートは読み書きできません");

    let (s, _, body) = call(&app, cell_req(&c, json!({"sid": "x"}), true)).await;
    assert_eq!(s, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"], "入力の形が違います");

    let (s, _, _) = call(&app, cell_req(&c, cell_body("状況", 3), true)).await;
    assert_eq!(
        s,
        StatusCode::SERVICE_UNAVAILABLE,
        "ここまで来て初めて鍵が要る"
    );
}

#[tokio::test]
async fn 別のオリジンからの書き込みは403() {
    let app = app();
    let c = login(&app, ME, crate::auth::LOGIN_METHOD_GOOGLE_OIDC).await;
    let req = Request::post("/api/portal-test/cell")
        .header(header::COOKIE, &c)
        .header(header::CONTENT_TYPE, "application/json")
        .header("X-Requested-With", "fetch")
        .header(header::ORIGIN, "https://evil.example.com")
        .body(Body::from(cell_body("状況", 3).to_string()))
        .unwrap();
    let (s, cache, body) = call(&app, req).await;
    assert_eq!(s, StatusCode::FORBIDDEN);
    assert_eq!(cache.as_deref(), Some("no-store"));
    assert_eq!(body["error"], "書き込みの確認に失敗しました");
}

#[test]
fn 共有されていないときは追加するアドレスを返す() {
    let e = google_err(
        GoogleError::Status(403),
        "bot@example.iam.gserviceaccount.com",
        false,
    );
    assert_eq!(e.status, StatusCode::FORBIDDEN);
    assert_eq!(
        e.body["error"],
        "このアドレスをシートに編集者として追加してください: bot@example.iam.gserviceaccount.com"
    );
    assert_eq!(e.body["code"], "sheet_not_shared");
    let e = google_err(GoogleError::Status(404), "a@b", true);
    assert_eq!(e.body["code"], "sheet_not_shared");
    let e = google_err(GoogleError::Status(500), "a@b", true);
    assert_eq!(e.status, StatusCode::BAD_GATEWAY);
    assert_eq!(e.body["error"], "シートに保存できませんでした（500）");
    let e = google_err(GoogleError::Status(500), "a@b", false);
    assert_eq!(e.body["error"], "Google の読み取りに失敗しました（500）");
}
