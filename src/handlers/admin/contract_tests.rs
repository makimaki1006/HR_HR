//! `/api/admin/*` の contract テスト (W8、計画 §2.8 手順 5)。
//!
//! SQLite で裏打ちした偽 Turso (`audit::fake_turso`) に fixture 行を入れ、**本物の `build_app()`** に
//! `/login` からログインして JSON を取り、具体値で assert する。同じセッションで旧 HTML も取り、
//! JSON の値が HTML に出ていること (旧新の対応) も見る。
//! DAO の SQL は偽 Turso 経由で本物の SQLite に流れるので、`NULLS LAST` や GROUP BY も検証対象。
//!
//! helper (`test_state` / `login` / `get_json` / `seed_fixtures`) は `my::contract_tests` からも使う。

use std::sync::Arc;

use axum::body::Body;
use axum::http::{header, HeaderMap, Request, StatusCode};
use axum::Router;
use rusqlite::params;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::audit::fake_turso::{start_sqlite_audit, SharedConn};
use crate::audit::test_fixtures as fx;
use crate::audit::AuditDb;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::AppState;

pub(crate) const ADMIN_EMAIL: &str = "admin@f-a-c.co.jp";
/// fixture の花子 (role=user)。ログインすると既存行 acc-0001 に upsert される。
pub(crate) const USER_EMAIL: &str = "hanako@f-a-c.co.jp";
pub(crate) const PASSWORD: &str = "internal-pass";

const ADMIN_API_PATHS: [&str; 4] = [
    "/api/admin/users",
    "/api/admin/users/acc-0001",
    "/api/admin/login-failures",
    "/api/admin/usage?days=30",
];

pub(crate) fn test_state(audit: Option<AuditDb>) -> Arc<AppState> {
    let config = AppConfig {
        port: 0,
        auth_password: PASSWORD.to_string(),
        auth_password_hash: String::new(),
        external_passwords: vec![],
        allowed_domains: vec!["f-a-c.co.jp".to_string()],
        allowed_domains_extra: vec![],
        hellowork_db_path: String::new(),
        indeed_db_path: String::new(),
        cache_ttl_secs: 60,
        cache_max_entries: 10,
        rate_limit_max_attempts: 50,
        rate_limit_lockout_secs: 60,
        audit_turso_url: String::new(),
        audit_turso_token: String::new(),
        audit_ip_salt: String::new(),
        admin_emails: vec![ADMIN_EMAIL.to_string()],
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
    })
}

fn nz(s: &str) -> Option<&str> {
    if s.is_empty() {
        None
    } else {
        Some(s)
    }
}

/// fixture 行 (accounts 3 / login_sessions 5 / activity_logs 4) を SQLite に入れる。
/// 空文字は NULL で入れる (本番の INSERT と同じ形)。
pub(crate) fn seed_fixtures(conn: &SharedConn) {
    let c = conn.lock().unwrap();
    for a in fx::accounts() {
        c.execute(
            "INSERT INTO accounts (id, email, display_name, company, role, first_seen_at, \
             last_login_at, login_count, disabled_at) VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9)",
            params![
                a.id,
                a.email,
                nz(&a.display_name),
                nz(&a.company),
                a.role,
                a.first_seen_at,
                nz(&a.last_login_at),
                a.login_count,
                nz(&a.disabled_at)
            ],
        )
        .unwrap();
    }
    for s in fx::hanako_sessions()
        .into_iter()
        .chain(fx::login_failures())
    {
        c.execute(
            "INSERT INTO login_sessions (id, account_id, attempted_email, started_at, ended_at, \
             ip_hash, user_agent, login_method, success, failure_reason) \
             VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)",
            params![
                s.id,
                nz(&s.account_id),
                nz(&s.attempted_email),
                s.started_at,
                nz(&s.ended_at),
                s.ip_hash,
                s.user_agent,
                s.login_method,
                s.success,
                nz(&s.failure_reason)
            ],
        )
        .unwrap();
    }
    for a in fx::hanako_activities() {
        c.execute(
            "INSERT INTO activity_logs (id, account_id, session_id, at, event_type, target_type, \
             target_id, meta) VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
            params![
                a.id,
                a.account_id,
                nz(&a.session_id),
                a.at,
                a.event_type,
                nz(&a.target_type),
                nz(&a.target_id),
                nz(&a.meta)
            ],
        )
        .unwrap();
    }
}

pub(crate) async fn body_string(res: axum::response::Response) -> String {
    let bytes = http_body_util::BodyExt::collect(res.into_body())
        .await
        .unwrap()
        .to_bytes();
    String::from_utf8(bytes.to_vec()).unwrap()
}

/// パスワードログインしてセッション Cookie (`id=...`) を返す。
pub(crate) async fn login(app: &Router, email: &str) -> String {
    let res = post_login(app, email, PASSWORD).await;
    assert_eq!(
        res.status(),
        StatusCode::SEE_OTHER,
        "{email} でログインできない"
    );
    session_cookie(&res).expect("ログイン応答に Set-Cookie が無い")
}

pub(crate) async fn post_login(
    app: &Router,
    email: &str,
    password: &str,
) -> axum::response::Response {
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

fn session_cookie(res: &axum::response::Response) -> Option<String> {
    res.headers()
        .get_all(header::SET_COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .map(|c| c.split(';').next().unwrap_or("").to_string())
        .find(|c| c.starts_with("id="))
}

pub(crate) async fn get(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
) -> (StatusCode, HeaderMap, String) {
    let mut b = Request::builder().uri(uri);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    (status, headers, body_string(res).await)
}

pub(crate) async fn get_json(app: &Router, uri: &str, cookie: Option<&str>) -> (StatusCode, Value) {
    let (status, headers, body) = get(app, uri, cookie).await;
    let ct = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(
        ct.starts_with("application/json"),
        "{uri}: content-type が JSON でない: {ct} / {status} / {body}"
    );
    (
        status,
        serde_json::from_str(&body).unwrap_or_else(|e| panic!("{uri}: {e}\n{body}")),
    )
}

fn keys(v: &Value) -> Vec<&str> {
    v.as_object().unwrap().keys().map(String::as_str).collect()
}

// ---------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread")]
async fn admin_users_json_has_fixture_values_and_matches_html() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    let cookie = login(&app, ADMIN_EMAIL).await;

    let (status, v) = get_json(&app, "/api/admin/users", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys(&v), ["accounts"]);
    let accounts = v["accounts"].as_array().unwrap();
    // fixture 3 件 + いまログインした管理者 (upsert で自動登録) = 4 件、last_login_at 降順
    assert_eq!(accounts.len(), 4, "{v}");
    assert_eq!(accounts[0]["email"], ADMIN_EMAIL);
    assert_eq!(accounts[0]["role"], "admin");
    assert_eq!(accounts[0]["login_count"], 1);
    assert_eq!(accounts[0]["display_name"], "", "NULL は空文字");
    assert_eq!(accounts[1]["id"], fx::HANAKO_ID);
    assert_eq!(accounts[1]["display_name"], "山田 花子");
    assert_eq!(accounts[1]["company"], "F&A <Consulting>", "JSON は生の値");
    assert_eq!(accounts[1]["login_count"], 12);
    assert_eq!(accounts[1]["last_login_at"], "2026-09-28T01:23:45Z");
    assert_eq!(accounts[1]["disabled_at"], "");
    assert_eq!(accounts[2]["id"], fx::JIRO_ID);
    assert_eq!(accounts[2]["company"], "");
    assert_eq!(accounts[3]["id"], fx::SABURO_ID);
    assert_eq!(accounts[3]["role"], "admin");
    assert_eq!(accounts[3]["disabled_at"], "2026-09-01T00:00:00Z");
    assert_eq!(
        keys(&accounts[1]),
        [
            "id",
            "email",
            "display_name",
            "company",
            "role",
            "first_seen_at",
            "last_login_at",
            "login_count",
            "disabled_at"
        ]
    );

    // 旧 HTML に同じ値が出る (同じセッション、同じ DB)
    let (status, _, html) = get(&app, "/admin/users", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("ユーザー一覧 (4 件)"), "{html}");
    assert!(
        html.contains("F&amp;A &lt;Consulting&gt;"),
        "HTML はエスケープ"
    );
    for a in accounts {
        let id = a["id"].as_str().unwrap();
        assert!(
            html.contains(&format!("href=\"/admin/users/{id}\"")),
            "{id}"
        );
        assert!(html.contains(&format!(">{}<", a["login_count"])));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_user_detail_json_kpis_and_404() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    let cookie = login(&app, ADMIN_EMAIL).await;

    let (status, v) = get_json(&app, "/api/admin/users/acc-0001", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys(&v), ["account", "sessions", "activities", "kpi_30d"]);
    assert_eq!(v["account"]["email"], USER_EMAIL);
    assert_eq!(
        v["kpi_30d"],
        json!({"login_ok": 1, "login_fail": 1, "activity": 3, "company_views": 2})
    );
    let sessions = v["sessions"].as_array().unwrap();
    assert_eq!(sessions.len(), 3);
    assert_eq!(sessions[0]["id"], "ses-0001", "started_at 降順");
    assert_eq!(sessions[0]["user_agent"], fx::LONG_UA, "JSON は UA 全文");
    assert_eq!(sessions[0]["success"], 1);
    assert_eq!(sessions[0]["login_method"], "password_internal");
    assert_eq!(sessions[1]["failure_reason"], "wrong_password");
    assert_eq!(sessions[1]["success"], 0);
    assert_eq!(sessions[2]["id"], "ses-0003");
    assert_eq!(sessions[2]["ended_at"], "", "NULL は空文字");
    let activities = v["activities"].as_array().unwrap();
    assert_eq!(activities.len(), 4);
    assert_eq!(activities[0]["target_id"], "1234567890123", "at 降順");
    assert_eq!(activities[1]["event_type"], "view_tab");
    assert_eq!(activities[3]["event_type"], "login");

    // 旧 HTML: 同じ KPI・件数・UA 40 文字
    let (status, _, html) = get(&app, "/admin/users/acc-0001", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        html.contains(r#"<div class="text-3xl font-bold">1</div>"#),
        "{html}"
    );
    assert!(html.contains(r#"<div class="text-3xl font-bold text-red-400">1</div>"#));
    assert!(html.contains(r#"<div class="text-3xl font-bold">3</div>"#));
    assert!(html.contains(r#"<div class="text-3xl font-bold">2</div>"#));
    assert!(html.contains("ログイン履歴 (3 件)"));
    assert!(html.contains("操作履歴 (4 件)"));
    let ua40: String = fx::LONG_UA.chars().take(40).collect();
    assert!(html.contains(&ua40));
    assert!(!html.contains(fx::LONG_UA));

    // 無いアカウントは 404 + JSON
    let (status, v) = get_json(&app, "/api/admin/users/acc-none", Some(&cookie)).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(v["account_id"], "acc-none");
    assert!(v["error"].is_string(), "{v}");
    let (status, _, html) = get(&app, "/admin/users/acc-none", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "旧 HTML は 200 で未検出ページ");
    assert!(html.contains("アカウントが見つかりません"));
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_login_failures_json_grows_with_a_real_failure() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    let cookie = login(&app, ADMIN_EMAIL).await;

    let (status, v) = get_json(&app, "/api/admin/login-failures", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys(&v), ["failures"]);
    let f = v["failures"].as_array().unwrap();
    // ses-0002 (2099) → fail-0001 → fail-0002。管理者のログイン成功は含まれない
    assert_eq!(f.len(), 3, "{v}");
    assert_eq!(f[0]["id"], "ses-0002");
    assert_eq!(f[1]["attempted_email"], "attacker@evil.example");
    assert_eq!(f[1]["failure_reason"], "invalid_domain");
    assert_eq!(f[1]["ip_hash"], "ff00ff00ff00");
    assert_eq!(f[2]["id"], "fail-0002");
    assert!(f.iter().all(|x| x["success"] == 0));

    // 実際に失敗させると 1 件増える (started_at=now は 2099 の後、2026-09-28 の前)
    let res = post_login(&app, "x@f-a-c.co.jp", "wrong").await;
    assert_eq!(res.status(), StatusCode::OK, "失敗時はログイン画面");
    let (_, v) = get_json(&app, "/api/admin/login-failures", Some(&cookie)).await;
    let f = v["failures"].as_array().unwrap();
    assert_eq!(f.len(), 4);
    assert_eq!(f[1]["attempted_email"], "x@f-a-c.co.jp");
    assert_eq!(f[1]["failure_reason"], "wrong_password");
    assert_eq!(f[1]["account_id"], "");

    let (_, _, html) = get(&app, "/admin/login-failures", Some(&cookie)).await;
    assert!(html.contains("ログイン失敗ログ (4 件)"), "{html}");
    assert!(html.contains("attacker@evil.example"));
    assert!(html.contains("x@f-a-c.co.jp"));
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_usage_json_labels_and_days_clamp() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    // 管理者のログインで activity_logs に 'login' が 1 件入る
    let cookie = login(&app, ADMIN_EMAIL).await;

    let (status, v) = get_json(&app, "/api/admin/usage?days=30", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(keys(&v), ["days", "by_event", "by_account", "cross"]);
    assert_eq!(v["days"], 30);
    // fixture の 2099 年の操作は常に期間内、2000 年の login は期間外
    let by_event = v["by_event"].as_array().unwrap();
    assert_eq!(by_event.len(), 3, "{v}");
    assert_eq!(by_event[0]["event_type"], "view_company_profile");
    assert_eq!(by_event[0]["label"], "企業カルテを見る");
    assert_eq!(by_event[0]["count"], 2);
    assert_eq!(by_event[0]["last_at"], "2099-01-02T09:30:00Z");
    let tab = by_event
        .iter()
        .find(|e| e["event_type"] == "view_tab")
        .expect("view_tab");
    assert_eq!(tab["target_id"], "/tab/survey");
    assert_eq!(tab["label"], "タブを開く: 媒体分析");
    assert_eq!(tab["count"], 1);
    let login_ev = by_event
        .iter()
        .find(|e| e["event_type"] == "login")
        .expect("login");
    assert_eq!(login_ev["label"], "ログイン");
    assert_eq!(login_ev["count"], 1);
    assert_eq!(login_ev["email"], "", "機能別は email 無し");

    let by_account = v["by_account"].as_array().unwrap();
    assert_eq!(by_account.len(), 2);
    assert_eq!(by_account[0]["account_id"], fx::HANAKO_ID);
    assert_eq!(by_account[0]["email"], USER_EMAIL);
    assert_eq!(by_account[0]["count"], 3);
    assert_eq!(by_account[1]["email"], ADMIN_EMAIL);
    assert_eq!(by_account[1]["count"], 1);

    let cross = v["cross"].as_array().unwrap();
    assert_eq!(cross.len(), 3);
    assert_eq!(cross[0]["email"], USER_EMAIL);
    assert_eq!(cross[0]["event_type"], "view_company_profile");
    assert_eq!(cross[0]["label"], "企業カルテを見る");
    assert_eq!(cross[0]["count"], 2);
    assert_eq!(
        keys(&cross[0]),
        [
            "account_id",
            "email",
            "event_type",
            "target_id",
            "label",
            "count",
            "last_at"
        ]
    );

    // days は 1〜365 に丸める (旧 HTML と同じ)
    for (q, expect) in [
        ("", 30),
        ("?days=7", 7),
        ("?days=0", 1),
        ("?days=9999", 365),
    ] {
        let (_, v) = get_json(&app, &format!("/api/admin/usage{q}"), Some(&cookie)).await;
        assert_eq!(v["days"], expect, "{q}");
    }

    // 旧 HTML: 同じラベルと件数
    let (_, _, html) = get(&app, "/admin/usage?days=30", Some(&cookie)).await;
    assert!(html.contains("企業カルテを見る"), "{html}");
    assert!(html.contains("タブを開く: 媒体分析"));
    assert!(html.contains(">2<"));
    assert!(html.contains(">3<"));
    assert!(
        !html.contains("view_company_profile"),
        "内部コードは出さない"
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn admin_api_rejects_anonymous_non_admin_and_no_audit() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));

    // 未ログイン → 303 /login
    for p in ADMIN_API_PATHS {
        let (status, headers, _) = get(&app, p, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{p}");
        assert_eq!(headers[header::LOCATION], "/login", "{p}");
    }
    // 一般ユーザー (fixture の花子、role=user) → 403
    let cookie = login(&app, USER_EMAIL).await;
    for p in ADMIN_API_PATHS {
        let (status, _, body) = get(&app, p, Some(&cookie)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}: {body}");
        assert!(body.contains("管理者権限が必要です"), "{p}: {body}");
    }
    // 管理者 → 200 (対照)
    let admin_cookie = login(&app, ADMIN_EMAIL).await;
    for p in ADMIN_API_PATHS {
        let (status, _) = get_json(&app, p, Some(&admin_cookie)).await;
        assert_eq!(status, StatusCode::OK, "{p}");
    }
    // 監査 DB 未接続 → 管理者メールでも 403 (role を確かめられない)
    let app2 = crate::build_app(test_state(None));
    let cookie2 = login(&app2, ADMIN_EMAIL).await;
    for p in ADMIN_API_PATHS {
        let (status, _, body) = get(&app2, p, Some(&cookie2)).await;
        assert_eq!(status, StatusCode::FORBIDDEN, "{p}");
        assert!(body.contains("監査DB未接続"), "{p}: {body}");
    }
}
