//! 共通基盤 (platform-team、2026-09-30) の結合テスト。
//!
//! 本物の `build_app()` を組み、`POST /login` (社内パスワード) で取った cookie で
//! `/api/nav` / `/api/filters/current` / 旧シェル `/` / 隠した画面の URL 直アクセス /
//! `/api/*` の 401 JSON / CSRF を通す。DB・Turso・監査は全部 `None` (ルーティングと
//! session だけを見る)。

use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{header, Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::Value;
use tower::ServiceExt;

use crate::config::{AppConfig, ExternalPassword};
use crate::db::cache::AppCache;
use crate::handlers::nav::{nav_items, NavFeatures, NAV_DEFS};
use crate::{build_app, AppState};

const USER: &str = "hanako@f-a-c.co.jp";
const ADMIN: &str = "boss@f-a-c.co.jp";
const PASS: &str = "internal-pass";

/// プロセスの環境変数を書き換えるテストを直列にするロック。cargo test はテストを並列に走らせるので、
/// set_var / remove_var を同時に行わないようにする (営業KPI のテストで同種の競合による失敗が出た)。
static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

fn test_config() -> AppConfig {
    AppConfig {
        port: 0,
        auth_password: PASS.to_string(),
        auth_password_hash: String::new(),
        external_passwords: vec![ExternalPassword {
            password: "external-pass".to_string(),
            expires: "2099-12-31".to_string(),
        }],
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
        // 管理者判定 (handlers::nav::is_admin) は大文字小文字を区別しない
        admin_emails: vec!["Boss@f-a-c.co.jp".to_string()],
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    }
}

fn app() -> Router {
    build_app(Arc::new(AppState {
        config: test_config(),
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        hubspot: None,
    }))
}

async fn send(
    app: &Router,
    method: &str,
    uri: &str,
    cookie: Option<&str>,
    headers: &[(&str, &str)],
    body: Option<&str>,
) -> Response {
    let mut b = Request::builder().method(method).uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    for (k, v) in headers {
        b = b.header(*k, *v);
    }
    let body = match body {
        Some(s) => Body::from(s.to_string()),
        None => Body::empty(),
    };
    app.clone().oneshot(b.body(body).unwrap()).await.unwrap()
}

async fn body_string(resp: Response) -> String {
    let bytes = to_bytes(resp.into_body(), 8 * 1024 * 1024).await.unwrap();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// `POST /login` (Origin 無し: /login は auth_middleware の外なので従来どおり通る) → session cookie。
async fn login(app: &Router, email: &str) -> String {
    let body = format!(
        "email={}&password={}",
        urlencoding::encode(email),
        urlencoding::encode(PASS)
    );
    let res = send(
        app,
        "POST",
        "/login",
        None,
        &[("content-type", "application/x-www-form-urlencoded")],
        Some(&body),
    )
    .await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER, "ログインできない");
    assert_eq!(res.headers()[header::LOCATION], "/");
    let set_cookie = res
        .headers()
        .get(header::SET_COOKIE)
        .expect("ログイン応答に Set-Cookie が無い")
        .to_str()
        .unwrap();
    set_cookie.split(';').next().unwrap().to_string()
}

const JSON: &[(&str, &str)] = &[
    ("accept", "application/json"),
    ("x-requested-with", "fetch"),
];

async fn get_json(app: &Router, uri: &str, cookie: &str) -> Value {
    let res = send(app, "GET", uri, Some(cookie), JSON, None).await;
    assert_eq!(res.status(), StatusCode::OK, "{uri}");
    let ct = res.headers()[header::CONTENT_TYPE]
        .to_str()
        .unwrap()
        .to_string();
    assert!(ct.starts_with("application/json"), "{uri}: {ct}");
    serde_json::from_str(&body_string(res).await).unwrap()
}

fn item<'a>(v: &'a Value, id: &str) -> &'a Value {
    v["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["id"] == id)
        .unwrap_or_else(|| panic!("items に {id} が無い: {v}"))
}

// ================================================================ /api/nav

#[tokio::test]
async fn api_navはログイン済みなら定義どおりのjsonを返す() {
    let app = app();
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/nav", &cookie).await;

    assert_eq!(v["user_email"], USER);
    assert_eq!(v["is_admin"], false);
    let header_ids: Vec<&str> = v["header_links"]
        .as_array()
        .unwrap()
        .iter()
        .map(|l| l["id"].as_str().unwrap())
        .collect();
    assert_eq!(
        header_ids,
        ["guide", "settings", "logout"],
        "非 admin に「管理」は出ない"
    );
    assert_eq!(v["header_links"][1]["href"], "/my/profile");
    assert_eq!(v["header_links"][2]["href"], "/logout");
    assert_eq!(
        v["groups"],
        serde_json::json!([{"id": "explore", "label": "調べる"}])
    );

    // items は handlers::nav の定義そのもの (環境変数の出し分けは NavFeatures::from_env)
    let expected = serde_json::to_value(nav_items(NAV_DEFS, &NavFeatures::from_env())).unwrap();
    assert_eq!(v["items"], expected);

    // 具体値
    assert_eq!(item(&v, "survey")["href"], "/?tab=/tab/survey");
    assert_eq!(item(&v, "survey")["kind"], "legacy_tab");
    assert_eq!(item(&v, "survey")["hidden"], false);
    assert_eq!(item(&v, "jobmap")["group"], "explore");
    assert_eq!(item(&v, "consulting")["href"], "/consulting");
    assert_eq!(item(&v, "consulting")["kind"], "page");
    assert_eq!(item(&v, "market")["hidden"], true);
    assert_eq!(item(&v, "market")["hidden_since"], "2026-05-15");
    assert_eq!(item(&v, "call-quality")["href"], "/call-quality");
    assert_eq!(item(&v, "call-quality")["hidden_since"], "2026-09-07");
    assert_eq!(item(&v, "proposal-mock")["href"], "/proposal-mock");
    assert_eq!(item(&v, "job-copy")["href"], "/app/job-copy");
    assert_eq!(item(&v, "job-copy")["kind"], "app");
    assert_eq!(item(&v, "job-copy")["hidden"], false);
    let hidden = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["hidden"] == true)
        .count();
    assert_eq!(
        hidden, 15,
        "隠し対象 15 件 (8 タブ + 求人検索 + dead route 4 + proposal-mock + 架電)"
    );
    // 表示順: 既存の営業・コンサルKPIの順序を保持し、求人文面を末尾に追加。
    let visible: Vec<&str> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["hidden"] == false)
        .map(|i| i["id"].as_str().unwrap())
        .collect();
    assert_eq!(visible.first(), Some(&"survey"));
    assert_eq!(
        &visible[visible.len() - 3..],
        &["sales-kpi", "consulting", "job-copy"]
    );
}

#[tokio::test]
async fn api_navは管理者にだけ管理リンクを出す() {
    let app = app();
    let cookie = login(&app, ADMIN).await;
    let v = get_json(&app, "/api/nav", &cookie).await;
    assert_eq!(v["user_email"], ADMIN);
    assert_eq!(v["is_admin"], true);
    let admin = v["header_links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["id"] == "admin")
        .expect("admin に「管理」が無い");
    assert_eq!(admin["label"], "管理");
    assert_eq!(admin["href"], "/admin/usage");
    assert_eq!(admin["kind"], "page");
    assert_eq!(v["header_links"].as_array().unwrap().len(), 4);
    // CRM は admin かつ /app/crm が KNOWN_SCREENS に登録済みのときだけ items に入る
    let has_crm = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .any(|i| i["id"] == "crm");
    assert_eq!(has_crm, crate::handlers::nav::crm_screen_registered());
    if has_crm {
        let crm = item(&v, "crm");
        assert_eq!(crm["label"], "CRM");
        assert_eq!(crm["kind"], "app");
        assert_eq!(crm["href"], "/app/crm");
        assert_eq!(crm["hidden"], false);
    }

    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/nav", &cookie).await;
    assert_eq!(v["is_admin"], false);
    assert!(v["header_links"]
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["id"] != "admin"));
    assert!(
        v["items"]
            .as_array()
            .unwrap()
            .iter()
            .all(|i| i["id"] != "crm"),
        "非 admin に CRM は出ない"
    );
}

// ================================================================ 旧シェルと同じ定義

/// 旧シェル `/` の HTML と `/api/nav` の JSON が同じ定義から出ていること
/// (可視の項目は HTML にあり、hidden の項目は HTML に無い)。
#[tokio::test]
async fn 旧シェルのナビとapi_navは同じ定義から描く() {
    let app = app();
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/nav", &cookie).await;
    let res = send(&app, "GET", "/", Some(&cookie), &[], None).await;
    assert_eq!(res.status(), StatusCode::OK);
    let html = body_string(res).await;
    assert!(
        !html.contains("{{NAV_TOP_ITEMS}}") && !html.contains("{{NAV_EXPLORE_ITEMS}}"),
        "差し込み口が残っている"
    );
    for i in v["items"].as_array().unwrap() {
        let id = i["id"].as_str().unwrap();
        let href = i["href"].as_str().unwrap();
        let needle = match i["kind"].as_str().unwrap() {
            "legacy_tab" => format!(r#"hx-get="{}""#, href.strip_prefix("/?tab=").unwrap()),
            _ => format!(r#"href="{href}""#),
        };
        if i["hidden"] == true {
            assert!(
                !html.contains(&needle),
                "{id}: hidden なのに旧シェルに {needle} がある"
            );
        } else {
            assert!(html.contains(&needle), "{id}: 旧シェルに {needle} が無い");
            let label = i["label"].as_str().unwrap();
            assert!(
                html.contains(&format!(">{label}</")),
                "{id}: ラベル {label} が無い"
            );
        }
    }
    // キーワード需要 / 求人票作成の出し分けも両方で同じ (環境変数がどうであれ一致する)
    for (id, target) in [
        ("keyword-tools", "/tab/keyword_tools"),
        ("jobgen-tools", "/tab/jobgen_tools"),
    ] {
        let in_json = v["items"].as_array().unwrap().iter().any(|i| i["id"] == id);
        let in_html = html.contains(&format!(r#"hx-get="{target}""#));
        assert_eq!(in_json, in_html, "{id}: JSON={in_json} HTML={in_html}");
    }
    // 旧 JS が頼る骨組み
    assert!(html.contains(r#"id="explore-group-btn""#));
    assert!(html.contains(r#"id="explore-subnav""#));
    assert!(html.contains(
        r#"class="tab-btn active" role="tab" aria-selected="true" hx-get="/tab/survey""#
    ));
    assert!(html.contains("function setActiveTab(el)"));
    assert!(html.contains("function toggleExploreGroup()"));
    // 管理リンクは admin だけ (dashboard_page と /api/nav が同じ is_admin を使う)
    assert!(!html.contains(r#"href="/admin/usage""#));
    let admin_cookie = login(&app, ADMIN).await;
    let res = send(&app, "GET", "/", Some(&admin_cookie), &[], None).await;
    let admin_html = body_string(res).await;
    assert!(admin_html.contains(
        r#"<a href="/admin/usage" class="text-slate-400 hover:text-white text-sm transition" title="利用状況・ユーザー管理">管理</a>"#
    ));
}

/// 隠した画面の URL 直アクセスは、ログイン済みなら従来どおり 200 (ルートは消していない)。
#[tokio::test]
async fn 隠した画面のurl直アクセスはログイン済みで200のまま() {
    let app = app();
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/nav", &cookie).await;
    let hidden: Vec<(String, String)> = v["items"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|i| i["hidden"] == true)
        .map(|i| {
            let href = i["href"].as_str().unwrap();
            let direct = href.strip_prefix("/?tab=").unwrap_or(href).to_string();
            (i["id"].as_str().unwrap().to_string(), direct)
        })
        .collect();
    assert_eq!(hidden.len(), 15);
    assert!(hidden.iter().any(|(_, u)| u == "/tab/overview"));
    assert!(hidden.iter().any(|(_, u)| u == "/call-quality"));
    assert!(hidden.iter().any(|(_, u)| u == "/proposal-mock"));
    for (id, uri) in &hidden {
        let res = send(&app, "GET", uri, Some(&cookie), &[], None).await;
        assert_eq!(
            res.status(),
            StatusCode::OK,
            "{id}: {uri} がログイン済みで 200 でない"
        );
    }
    // 未ログインなら従来どおり 303 /login
    for (_, uri) in &hidden {
        let res = send(&app, "GET", uri, None, &[], None).await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "{uri}");
        assert_eq!(res.headers()[header::LOCATION], "/login");
    }
}

// ================================================================ /api/filters/current

#[tokio::test]
async fn api_filters_currentはsessionの値を返しset_apiで変わる() {
    let app = app();
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/filters/current", &cookie).await;
    assert_eq!(
        v,
        serde_json::json!({"prefecture": "", "municipality": "", "job_types": [], "industry_raws": []})
    );
    // 既存の POST /api/set_prefecture (Origin 付き = ブラウザからの書き込み) → session が変わる
    let res = send(
        &app,
        "POST",
        "/api/set_prefecture",
        Some(&cookie),
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("origin", "https://hr-hw.onrender.com"),
        ],
        Some("prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let v = get_json(&app, "/api/filters/current", &cookie).await;
    assert_eq!(v["prefecture"], "東京都");
    assert_eq!(v["municipality"], "");
    // React の filterApi.ts と同じ形 (Origin 無し + X-Requested-With: fetch)
    let res = send(
        &app,
        "POST",
        "/api/set_municipality",
        Some(&cookie),
        &[
            ("content-type", "application/x-www-form-urlencoded"),
            ("x-requested-with", "fetch"),
        ],
        Some("municipality=%E5%8D%83%E4%BB%A3%E7%94%B0%E5%8C%BA"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK, "{}", body_string(res).await);
    let v = get_json(&app, "/api/filters/current", &cookie).await;
    assert_eq!(v["prefecture"], "東京都");
    assert_eq!(v["municipality"], "千代田区");
    // 未ログインは 401 JSON (Accept json、HX-Request 無し)
    let res = send(&app, "GET", "/api/filters/current", None, JSON, None).await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
}

// ================================================================ /api/* の 401 JSON

#[tokio::test]
async fn 未ログインのapiはaccept_jsonかつhx_request無しのときだけ401json() {
    let app = app();
    // Accept json + HX-Request 無し → 401 + JSON body
    for uri in [
        "/api/nav",
        "/api/filters/current",
        "/api/app/ping",
        "/api/set_prefecture",
    ] {
        let method = if uri == "/api/set_prefecture" {
            "POST"
        } else {
            "GET"
        };
        let mut headers = vec![("accept", "application/json")];
        if method == "POST" {
            // CSRF は認証より先に見るので、書き込みは Origin を付けて 401 まで到達させる
            headers.push(("origin", "https://hr-hw.onrender.com"));
        }
        let res = send(&app, method, uri, None, &headers, Some("")).await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri}");
        assert_eq!(
            res.headers()[header::CONTENT_TYPE],
            "application/json",
            "{uri}"
        );
        assert!(
            res.headers().get(header::LOCATION).is_none(),
            "{uri}: 401 に Location は要らない"
        );
        let v: Value = serde_json::from_str(&body_string(res).await).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"error": "auth_required", "login_url": "/login"}),
            "{uri}"
        );
    }
    // HX-Request: true → 従来どおり 303 /login (HTMX の既存呼び出し)
    let res = send(
        &app,
        "GET",
        "/api/nav",
        None,
        &[("accept", "application/json"), ("hx-request", "true")],
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()[header::LOCATION], "/login");
    // Accept 無し → 303
    let res = send(&app, "GET", "/api/nav", None, &[], None).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    assert_eq!(res.headers()[header::LOCATION], "/login");
    // Accept: */* → 303
    let res = send(&app, "GET", "/api/nav", None, &[("accept", "*/*")], None).await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
    // /api/ 以外 (/tab/market、/、/app/dummy) は Accept json でも 303
    for uri in ["/tab/market", "/", "/app/dummy"] {
        let res = send(
            &app,
            "GET",
            uri,
            None,
            &[("accept", "application/json")],
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::SEE_OTHER, "{uri}");
        assert_eq!(res.headers()[header::LOCATION], "/login", "{uri}");
    }
    // ログイン済み → 200
    let cookie = login(&app, USER).await;
    let res = send(
        &app,
        "GET",
        "/api/nav",
        Some(&cookie),
        &[("accept", "application/json")],
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    // /api/v1 (認証不要) は変わらない: 401 にも 303 にもならない
    let res = send(
        &app,
        "GET",
        "/api/v1/companies?q=x",
        None,
        &[("accept", "application/json")],
        None,
    )
    .await;
    assert_ne!(res.status(), StatusCode::UNAUTHORIZED);
    assert_ne!(res.status(), StatusCode::SEE_OTHER);
}

/// jobgen は `jobgen_auth_middleware` → `require_auth` の別経路。同じ条件で 401 JSON になる。
#[tokio::test]
async fn jobgen経路でも未ログインのapiは同じ条件で401json() {
    let app = app();
    let res = send(
        &app,
        "POST",
        "/api/jobgen/ab",
        None,
        &[
            ("accept", "application/json"),
            ("content-type", "application/json"),
            ("x-requested-with", "fetch"),
        ],
        Some("{}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::UNAUTHORIZED);
    let v: Value = serde_json::from_str(&body_string(res).await).unwrap();
    assert_eq!(v["error"], "auth_required");
    // HX-Request があれば 303
    let res = send(
        &app,
        "POST",
        "/api/jobgen/ab",
        None,
        &[
            ("accept", "application/json"),
            ("content-type", "application/json"),
            ("hx-request", "true"),
        ],
        Some("{}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::SEE_OTHER);
}

// ================================================================ CSRF

/// トークン認証 (Cookie を使わない) の要求は CSRF の攻撃対象にならないので、Origin も目印ヘッダーも
/// 無くても通る。Cookie セッションだけの要求は Origin 無し・目印ヘッダー無しなら 403。
/// 本番でトークン認証される書き込み経路は `/api/jobgen/*` (`jobgen_auth_middleware`、`API_AUTH_TOKEN`)
/// と `/scout/*` (auth_middleware の外) だけ。
#[tokio::test]
async fn csrfはトークン認証の書き込みを対象外にしcookieだけの書き込みは厳格化する() {
    const TOKEN: &str = "platform-api-test-token-7f3a";
    // 環境変数を書き換えるテストは ENV_LOCK で直列にする。トークン未提示の要求は値に関係なく
    // セッション認証に落ちるので、並行する他のテストの結果は変わらない。
    let _env = ENV_LOCK.lock().await;
    std::env::set_var("API_AUTH_TOKEN", TOKEN);
    let app = app();
    let json = [("content-type", "application/json")];

    // トークンのみ・Origin 無し・目印ヘッダー無し → CSRF も認証も通ってハンドラに届く
    let mut h = json.to_vec();
    h.push(("x-api-token", TOKEN));
    let res = send(&app, "POST", "/api/jobgen/ab", None, &h, Some("{}")).await;
    let status = res.status();
    assert_ne!(
        status,
        StatusCode::FORBIDDEN,
        "token の要求が CSRF で止まった"
    );
    assert_ne!(status, StatusCode::UNAUTHORIZED);
    assert_ne!(status, StatusCode::SEE_OTHER);

    // Authorization: Bearer でも同じ
    let mut h = json.to_vec();
    let bearer = format!("Bearer {TOKEN}");
    h.push(("authorization", bearer.as_str()));
    let res = send(&app, "POST", "/api/jobgen/ab", None, &h, Some("{}")).await;
    assert_ne!(res.status(), StatusCode::FORBIDDEN);

    // 違うトークン + Origin 無し + 目印ヘッダー無し → セッション経路の CSRF で 403
    let mut h = json.to_vec();
    h.push(("x-api-token", "wrong"));
    let res = send(&app, "POST", "/api/jobgen/ab", None, &h, Some("{}")).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);

    // ログイン済み Cookie のみ・Origin 無し・目印ヘッダー無し → 403
    let cookie = login(&app, USER).await;
    let res = send(
        &app,
        "POST",
        "/api/jobgen/ab",
        Some(&cookie),
        &json,
        Some("{}"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    let res = send(
        &app,
        "POST",
        "/api/set_prefecture",
        Some(&cookie),
        &[("content-type", "application/x-www-form-urlencoded")],
        Some("prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD"),
    )
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    std::env::remove_var("API_AUTH_TOKEN");
}

#[tokio::test]
async fn csrfはoriginが無い書き込みをfetchかhx_requestのときだけ通す() {
    let app = app();
    let cookie = login(&app, USER).await;
    let form = ("content-type", "application/x-www-form-urlencoded");
    let body = Some("prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD");
    let post = |headers: Vec<(&'static str, &'static str)>| {
        let app = app.clone();
        let cookie = cookie.clone();
        async move {
            let mut h = vec![form];
            h.extend(headers);
            send(&app, "POST", "/api/set_prefecture", Some(&cookie), &h, body).await
        }
    };
    // Origin 許可 → 通る
    let res = post(vec![("origin", "https://hr-hw.onrender.com")]).await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = post(vec![("origin", "http://localhost:8080")]).await;
    assert_eq!(res.status(), StatusCode::OK);
    // Origin 不許可 → 403 (従来どおり)
    let res = post(vec![("origin", "https://evil.example")]).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_string(res).await, "Forbidden: CSRF: invalid origin");
    // 不許可 Origin は X-Requested-With があっても 403 (Origin がある場合の判定は変えない)
    let res = post(vec![
        ("origin", "https://evil.example"),
        ("x-requested-with", "fetch"),
    ])
    .await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // Referer だけ (Origin 無し) → origin 部分で判定 (従来どおり)
    let res = post(vec![(
        "referer",
        "https://hr-hw.onrender.com/?tab=/tab/survey",
    )])
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    let res = post(vec![("referer", "https://evil.example/")]).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // 両方無し + X-Requested-With: fetch → 通る (React)
    let res = post(vec![("x-requested-with", "fetch")]).await;
    assert_eq!(res.status(), StatusCode::OK);
    // 両方無し + HX-Request → 通る (HTMX)
    let res = post(vec![("hx-request", "true")]).await;
    assert_eq!(res.status(), StatusCode::OK);
    // 何も無し → 403 (2026-09-30 まではここが通っていた)
    let res = post(vec![]).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    assert_eq!(body_string(res).await, "Forbidden: CSRF: missing origin");
    // X-Requested-With の別の値 (XMLHttpRequest 等) は通さない
    let res = post(vec![("x-requested-with", "XMLHttpRequest")]).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
    // GET は対象外
    let res = send(
        &app,
        "GET",
        "/api/filters/current",
        Some(&cookie),
        &[],
        None,
    )
    .await;
    assert_eq!(res.status(), StatusCode::OK);
    // 未ログインでも CSRF が先: 何も無い書き込みは 403 (303 でも 401 でもない)
    let res = send(&app, "POST", "/api/set_prefecture", None, &[form], body).await;
    assert_eq!(res.status(), StatusCode::FORBIDDEN);
}

/// allowlist の規則は `check_csrf_with` で直接押さえる (本番の allowlist は空)。
#[test]
fn csrf_allowlistの機械向け経路はorigin無しでも通る() {
    use crate::{check_csrf, check_csrf_with, CSRF_HEADERLESS_ALLOWLIST};
    let req = |method: &str, uri: &str, headers: &[(&str, &str)]| {
        let mut b = Request::builder().method(method).uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(Body::empty()).unwrap()
    };
    assert!(
        CSRF_HEADERLESS_ALLOWLIST.is_empty(),
        "2026-09-30 の棚卸しでは機械向け経路は無い"
    );
    // 本番の規則
    assert_eq!(
        check_csrf(&req("POST", "/api/hook", &[])),
        Err("CSRF: missing origin")
    );
    assert_eq!(check_csrf(&req("GET", "/api/hook", &[])), Ok(()));
    assert_eq!(
        check_csrf(&req("POST", "/api/hook", &[("x-requested-with", "fetch")])),
        Ok(())
    );
    assert_eq!(
        check_csrf(&req("POST", "/api/hook", &[("x-requested-with", "Fetch")])),
        Ok(())
    );
    assert_eq!(
        check_csrf(&req("POST", "/api/hook", &[("hx-request", "true")])),
        Ok(())
    );
    assert_eq!(
        check_csrf(&req(
            "POST",
            "/api/hook",
            &[("origin", "https://evil.example")]
        )),
        Err("CSRF: invalid origin")
    );
    // allowlist に載せた前方一致の経路だけ、Origin 無し・ヘッダー無しで通る
    let allow: &[&str] = &["/api/hook/"];
    assert_eq!(
        check_csrf_with(&req("POST", "/api/hook/x", &[]), allow),
        Ok(())
    );
    assert_eq!(
        check_csrf_with(&req("DELETE", "/api/hook/x?y=1", &[]), allow),
        Ok(())
    );
    assert_eq!(
        check_csrf_with(&req("POST", "/api/hook", &[]), allow),
        Err("CSRF: missing origin")
    );
    assert_eq!(
        check_csrf_with(&req("POST", "/api/other", &[]), allow),
        Err("CSRF: missing origin")
    );
    // allowlist でも不許可 Origin は 403 (Origin がある場合の判定は変えない)
    assert_eq!(
        check_csrf_with(
            &req("POST", "/api/hook/x", &[("origin", "https://evil.example")]),
            allow
        ),
        Err("CSRF: invalid origin")
    );
}

/// `CSRF_EXTRA_ORIGINS_DEBUG` は debug ビルドだけが読む追加許可 Origin (PR 時 E2E のポート 9217 用)。
/// 他のテストが 9217 / 9218 を使わないので、env を立てても競合しない。
#[cfg(debug_assertions)]
#[test]
fn csrf_debug_envで追加したoriginだけ通る() {
    // 環境変数を書き換えるので ENV_LOCK で他の書き換えテストと直列にする (同期テストなので blocking_lock)。
    let _env = ENV_LOCK.blocking_lock();
    use crate::check_csrf;
    let req = |origin: &str| {
        Request::builder()
            .method("POST")
            .uri("/api/set_prefecture")
            .header("origin", origin)
            .body(Body::empty())
            .unwrap()
    };
    // 設定前は 403
    assert_eq!(
        check_csrf(&req("http://localhost:9217")),
        Err("CSRF: invalid origin")
    );
    std::env::set_var(
        "CSRF_EXTRA_ORIGINS_DEBUG",
        "http://localhost:9217, http://127.0.0.1:9218",
    );
    assert_eq!(check_csrf(&req("http://localhost:9217")), Ok(()));
    assert_eq!(
        check_csrf(&req("http://127.0.0.1:9218")),
        Ok(()),
        "空白は trim される"
    );
    // 載っていない Origin は引き続き 403 (部分一致・前方一致で通らない)
    assert_eq!(
        check_csrf(&req("http://localhost:92170")),
        Err("CSRF: invalid origin")
    );
    assert_eq!(
        check_csrf(&req("https://evil.example")),
        Err("CSRF: invalid origin")
    );
    // 既存の許可は変わらない
    assert_eq!(check_csrf(&req("https://hr-hw.onrender.com")), Ok(()));
    std::env::remove_var("CSRF_EXTRA_ORIGINS_DEBUG");
    assert_eq!(
        check_csrf(&req("http://localhost:9217")),
        Err("CSRF: invalid origin")
    );
}

// ---- 2026-10-01: 都道府県・市区町村の JSON API (/api/app/geo/*) ----

fn geo_app(db: Option<crate::db::local_sqlite::LocalDb>) -> Router {
    build_app(Arc::new(AppState {
        config: test_config(),
        hw_db: db,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
        hubspot: None,
    }))
}

/// postings(prefecture, municipality) だけを持つ tempfile の SQLite。
fn geo_db(
    rows: &[(Option<&str>, Option<&str>)],
) -> (tempfile::NamedTempFile, crate::db::local_sqlite::LocalDb) {
    let tmp = tempfile::NamedTempFile::new().unwrap();
    let conn = rusqlite::Connection::open(tmp.path()).unwrap();
    conn.execute_batch("CREATE TABLE postings (prefecture TEXT, municipality TEXT);")
        .unwrap();
    for (p, m) in rows {
        conn.execute(
            "INSERT INTO postings VALUES (?1, ?2)",
            rusqlite::params![p, m],
        )
        .unwrap();
    }
    drop(conn);
    let db = crate::db::local_sqlite::LocalDb::new(tmp.path().to_str().unwrap()).unwrap();
    (tmp, db)
}

fn all_prefs_reversed_rows() -> Vec<(Option<&'static str>, Option<&'static str>)> {
    // 挿入順は逆順。重複行・空文字・NULL も混ぜる
    let mut v: Vec<_> = crate::models::job_seeker::PREFECTURE_ORDER
        .iter()
        .rev()
        .map(|p| (Some(*p), Some("X市")))
        .collect();
    v.push((Some("東京都"), Some("千代田区")));
    v.push((Some(""), Some("空県市")));
    v.push((None, Some("NULL県市")));
    v
}

/// `<option value="V"[ data-citycode="N"]>` を (value, citycode) の列にする。
fn parse_options(html: &str) -> Vec<(String, Option<u32>)> {
    html.split("<option value=\"")
        .skip(1)
        .map(|chunk| {
            let (value, rest) = chunk.split_once('"').unwrap();
            let tag = rest.split_once('>').unwrap().0;
            let code = tag
                .split_once("data-citycode=\"")
                .map(|(_, r)| r.split_once('"').unwrap().0.parse().unwrap());
            (value.to_string(), code)
        })
        .collect()
}

#[tokio::test]
async fn geo_都道府県は47件でjis順_prefcodeは1から47() {
    let (_tmp, db) = geo_db(&all_prefs_reversed_rows());
    let app = geo_app(Some(db));
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/app/geo/prefectures", &cookie).await;
    let arr = v.as_array().unwrap();
    assert_eq!(arr.len(), 47);
    assert_eq!(arr[0], serde_json::json!({"name":"北海道","prefcode":1}));
    assert_eq!(arr[12], serde_json::json!({"name":"東京都","prefcode":13}));
    assert_eq!(arr[46], serde_json::json!({"name":"沖縄県","prefcode":47}));
    for (i, (o, p)) in arr
        .iter()
        .zip(crate::models::job_seeker::PREFECTURE_ORDER.iter())
        .enumerate()
    {
        assert_eq!(o["name"], *p, "index {i}");
        assert_eq!(o["prefcode"], (i + 1) as u64, "index {i}");
    }
}

#[tokio::test]
async fn geo_未知の都道府県名は末尾でprefcodeはnull() {
    let (_tmp, db) = geo_db(&[
        (Some("架空県"), Some("a")),
        (Some("沖縄県"), Some("b")),
        (Some("北海道"), Some("c")),
    ]);
    let app = geo_app(Some(db));
    let cookie = login(&app, USER).await;
    let v = get_json(&app, "/api/app/geo/prefectures", &cookie).await;
    assert_eq!(
        v,
        serde_json::json!([
            {"name":"北海道","prefcode":1},
            {"name":"沖縄県","prefcode":47},
            {"name":"架空県","prefcode":null}
        ])
    );
}

fn tokyo_rows() -> Vec<(Option<&'static str>, Option<&'static str>)> {
    vec![
        (Some("東京都"), Some("港区")),
        (Some("東京都"), Some("新宿区")),
        (Some("東京都"), Some("千代田区")),
        (Some("東京都"), Some("千代田区")), // 重複
        (Some("東京都"), Some("八王子市")),
        (Some("東京都"), Some("架空市")), // マスタに無い
        (Some("東京都"), Some("")),
        (Some("東京都"), None),
        (Some("大阪府"), Some("大阪市北区")), // 別県
    ]
}

#[tokio::test]
async fn geo_市区町村は東京都で5件_citycodeはマスタ値_架空はnull() {
    let (_tmp, db) = geo_db(&tokyo_rows());
    let app = geo_app(Some(db));
    let cookie = login(&app, USER).await;
    let v = get_json(
        &app,
        &format!(
            "/api/app/geo/municipalities?prefecture={}",
            urlencoding::encode("東京都")
        ),
        &cookie,
    )
    .await;
    // 既存 SQL の ORDER BY municipality (UTF-8 バイト順)
    assert_eq!(
        v,
        serde_json::json!([
            {"name":"八王子市","citycode":13201},
            {"name":"千代田区","citycode":13101},
            {"name":"新宿区","citycode":13104},
            {"name":"架空市","citycode":null},
            {"name":"港区","citycode":13103}
        ])
    );
    // 未指定・空は []
    for uri in [
        "/api/app/geo/municipalities",
        "/api/app/geo/municipalities?prefecture=",
    ] {
        assert_eq!(
            get_json(&app, uri, &cookie).await,
            serde_json::json!([]),
            "{uri}"
        );
    }
}

#[tokio::test]
async fn geo_既存html_apiとjsonが一致する() {
    let mut rows = all_prefs_reversed_rows();
    rows.extend(tokyo_rows());
    rows.push((Some("架空県"), Some("z")));
    let (_tmp, db) = geo_db(&rows);
    let app = geo_app(Some(db));
    let cookie = login(&app, USER).await;
    let hx = [("hx-request", "true")];

    let html =
        body_string(send(&app, "GET", "/api/prefectures", Some(&cookie), &hx, None).await).await;
    let html_names: Vec<String> = parse_options(&html).into_iter().map(|(v, _)| v).collect();
    let json = get_json(&app, "/api/app/geo/prefectures", &cookie).await;
    let json_names: Vec<String> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|o| o["name"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(html_names.len(), 48);
    assert_eq!(html_names, json_names);

    let uri = format!("prefecture={}", urlencoding::encode("東京都"));
    let html = body_string(
        send(
            &app,
            "GET",
            &format!("/api/municipalities_cascade?{uri}"),
            Some(&cookie),
            &hx,
            None,
        )
        .await,
    )
    .await;
    let html_pairs = parse_options(&html);
    let json = get_json(&app, &format!("/api/app/geo/municipalities?{uri}"), &cookie).await;
    let json_pairs: Vec<(String, Option<u32>)> = json
        .as_array()
        .unwrap()
        .iter()
        .map(|o| {
            (
                o["name"].as_str().unwrap().to_string(),
                o["citycode"].as_u64().map(|n| n as u32),
            )
        })
        .collect();
    assert_eq!(html_pairs.len(), 6); // tokyo_rows の 5 件 + all_prefs_reversed_rows の「X市」
    assert_eq!(html_pairs, json_pairs);
    // 両方とも同じ関数を通るので、上の一致だけでは共通部分の回帰を検出できない。
    // 旧 HTML の出力そのもの (区切りは "\n") を固定値で確かめる。
    assert!(
        html.contains("<option value=\"千代田区\" data-citycode=\"13101\">千代田区</option>\n"),
        "{html}"
    );
    assert!(!html.contains('\r'), "{html:?}");
}

#[tokio::test]
async fn geo_未ログインは401_dbなしは空配列() {
    let (_tmp, db) = geo_db(&tokyo_rows());
    let app = geo_app(Some(db));
    for uri in [
        "/api/app/geo/prefectures",
        "/api/app/geo/municipalities?prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD",
    ] {
        let res = send(
            &app,
            "GET",
            uri,
            None,
            &[("accept", "application/json")],
            None,
        )
        .await;
        assert_eq!(res.status(), StatusCode::UNAUTHORIZED, "{uri}");
    }
    let app = geo_app(None);
    let cookie = login(&app, USER).await;
    for uri in [
        "/api/app/geo/prefectures",
        "/api/app/geo/municipalities?prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD",
    ] {
        assert_eq!(
            get_json(&app, uri, &cookie).await,
            serde_json::json!([]),
            "{uri}"
        );
    }
}
