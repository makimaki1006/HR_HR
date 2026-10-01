//! `/api/my/*` の contract テスト (W8、計画 §2.8 手順 5)。
//! 仕組みと helper は `handlers::admin::contract_tests` と同じ (偽 Turso + 本物の build_app)。

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::audit::fake_turso::{start_sqlite_audit, SharedConn};
use crate::audit::test_fixtures as fx;
use crate::handlers::admin::contract_tests::{
    body_string, get, get_json, login, seed_fixtures, test_state, USER_EMAIL,
};

/// React の `postJson` と同じヘッダで JSON を POST する。
async fn post_json(
    app: &Router,
    uri: &str,
    cookie: Option<&str>,
    origin: &str,
    body: &Value,
) -> (StatusCode, String) {
    let mut b = Request::builder()
        .method("POST")
        .uri(uri)
        .header(header::CONTENT_TYPE, "application/json")
        .header(header::ACCEPT, "application/json")
        .header("X-Requested-With", "fetch")
        .header(header::ORIGIN, origin);
    if let Some(c) = cookie {
        b = b.header(header::COOKIE, c);
    }
    let res = app
        .clone()
        .oneshot(b.body(Body::from(body.to_string())).unwrap())
        .await
        .unwrap();
    let status = res.status();
    (status, body_string(res).await)
}

async fn post_form(app: &Router, cookie: &str, form: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/my/profile")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .header(header::ORIGIN, "http://localhost:8080")
                .header(header::COOKIE, cookie)
                .body(Body::from(form.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = res.status();
    (status, body_string(res).await)
}

fn count_update_profile_events(conn: &SharedConn) -> i64 {
    conn.lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*) FROM activity_logs WHERE account_id = ?1 AND event_type = 'update_profile'",
            [fx::HANAKO_ID],
            |r| r.get(0),
        )
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn my_profile_and_activity_json_for_the_logged_in_user() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    // 花子は fixture に居る → upsert で login_count 12 → 13、セッションは acc-0001 に紐付く
    let cookie = login(&app, USER_EMAIL).await;

    let (status, v) = get_json(&app, "/api/my/profile", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["account"]["id"], fx::HANAKO_ID);
    assert_eq!(v["account"]["email"], USER_EMAIL);
    assert_eq!(v["account"]["display_name"], "山田 花子");
    assert_eq!(v["account"]["company"], "F&A <Consulting>");
    assert_eq!(v["account"]["role"], "user");
    assert_eq!(v["account"]["login_count"], 13);
    assert_eq!(v["account"]["first_seen_at"], "2026-01-05T09:00:00Z");

    let (status, v) = get_json(&app, "/api/my/activity", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v["status"], "ok");
    assert_eq!(v["account"]["id"], fx::HANAKO_ID);
    let sessions = v["sessions"].as_array().unwrap();
    // 2099 の 2 件 → いまのログイン → 2000 の 1 件
    assert_eq!(sessions.len(), 4, "{v}");
    assert_eq!(sessions[0]["id"], "ses-0001");
    assert_eq!(sessions[1]["id"], "ses-0002");
    assert_eq!(sessions[2]["success"], 1);
    assert_eq!(sessions[2]["login_method"], "password_internal");
    assert_eq!(sessions[2]["account_id"], fx::HANAKO_ID);
    assert_eq!(sessions[3]["id"], "ses-0003");
    let activities = v["activities"].as_array().unwrap();
    assert_eq!(activities.len(), 5);
    assert_eq!(activities[0]["id"], "act-0001");
    assert_eq!(activities[2]["id"], "act-0003");
    assert_eq!(activities[3]["event_type"], "login", "いまのログイン");
    assert_eq!(activities[4]["id"], "act-0004");

    // 旧 HTML: 同じ値 (UA は 50 文字に切り詰め)
    let (status, _, html) = get(&app, "/my/profile", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains(r#"value="山田 花子""#), "{html}");
    assert!(html.contains("F&amp;A &lt;Consulting&gt;"));
    assert!(html.contains(">13<"));
    let (_, _, html) = get(&app, "/my/activity", Some(&cookie)).await;
    let ua50: String = fx::LONG_UA.chars().take(50).collect();
    assert!(html.contains(&ua50), "{html}");
    assert!(!html.contains(fx::LONG_UA));
    assert!(html.contains("1234567890123"));
}

#[tokio::test(flavor = "multi_thread")]
async fn my_profile_post_json_uses_the_same_write_path_as_the_form() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let app = crate::build_app(test_state(Some(audit)));
    let cookie = login(&app, USER_EMAIL).await;

    // React からの JSON POST。氏名は 80 文字に切り詰め (旧フォームと同じ)
    let long_name = "あ".repeat(100);
    let (status, body) = post_json(
        &app,
        "/api/my/profile",
        Some(&cookie),
        "http://localhost:8080",
        &json!({"display_name": long_name, "company": "新会社 <X>"}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{body}");
    let v: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(v["status"], "ok", "{v}");
    assert_eq!(v["account"]["display_name"], "あ".repeat(80));
    assert_eq!(v["account"]["company"], "新会社 <X>");
    assert_eq!(v["account"]["id"], fx::HANAKO_ID);

    // SQLite に書かれ、監査 (update_profile) が記録されている
    let name: String = conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT display_name FROM accounts WHERE id = ?1",
            [fx::HANAKO_ID],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(name, "あ".repeat(80));
    // record_event は spawn_blocking で detach されるので少し待つ
    let mut n = 0;
    for _ in 0..50 {
        n = count_update_profile_events(&conn);
        if n >= 1 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(n, 1, "update_profile の監査記録");

    // 旧 HTML にも反映 (同じ DB)
    let (_, _, html) = get(&app, "/my/profile", Some(&cookie)).await;
    assert!(
        html.contains(&format!("value=\"{}\"", "あ".repeat(80))),
        "{html}"
    );
    assert!(html.contains("新会社 &lt;X&gt;"));

    // 旧フォーム POST も従来どおり動く (回帰)
    let (status, html) = post_form(&app, &cookie, "display_name=%E8%8A%B1%E5%AD%90&company=").await;
    assert_eq!(status, StatusCode::OK);
    assert!(html.contains("プロフィールを更新しました"), "{html}");
    assert!(html.contains(r#"value="花子""#));
    let (_, v) = get_json(&app, "/api/my/profile", Some(&cookie)).await;
    assert_eq!(v["account"]["display_name"], "花子");
    assert_eq!(v["account"]["company"], "");
    for _ in 0..50 {
        if count_update_profile_events(&conn) >= 2 {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(20)).await;
    }
    assert_eq!(count_update_profile_events(&conn), 2);

    // CSRF: 別オリジンからの POST は 403 (auth_middleware の Origin 検査)
    let (status, _) = post_json(
        &app,
        "/api/my/profile",
        Some(&cookie),
        "https://evil.example",
        &json!({"display_name": "x", "company": "y"}),
    )
    .await;
    assert_eq!(status, StatusCode::FORBIDDEN);
    let (_, v) = get_json(&app, "/api/my/profile", Some(&cookie)).await;
    assert_eq!(v["account"]["display_name"], "花子", "書き換わっていない");
}

#[tokio::test(flavor = "multi_thread")]
async fn my_api_without_audit_db_reports_audit_disabled() {
    let app = crate::build_app(test_state(None));
    let cookie = login(&app, USER_EMAIL).await;
    let (status, v) = get_json(&app, "/api/my/profile", Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(v, json!({"status": "audit_disabled"}));
    let (_, v) = get_json(&app, "/api/my/activity", Some(&cookie)).await;
    assert_eq!(v, json!({"status": "audit_disabled"}));
    let (status, body) = post_json(
        &app,
        "/api/my/profile",
        Some(&cookie),
        "http://localhost:8080",
        &json!({"display_name": "x", "company": ""}),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap(),
        json!({"status": "audit_disabled"})
    );
    // 旧 HTML も同じ状態
    let (_, _, html) = get(&app, "/my/profile", Some(&cookie)).await;
    assert!(
        html.contains("この機能は現在ご利用いただけません"),
        "{html}"
    );
}

#[tokio::test]
async fn my_api_requires_login() {
    let app = crate::build_app(test_state(None));
    for p in ["/api/my/profile", "/api/my/activity"] {
        let (status, headers, _) = get(&app, p, None).await;
        assert_eq!(status, StatusCode::SEE_OTHER, "{p}");
        assert_eq!(headers[header::LOCATION], "/login");
    }
    let (status, body) = post_json(
        &app,
        "/api/my/profile",
        None,
        "http://localhost:8080",
        &json!({"display_name": "x", "company": ""}),
    )
    .await;
    // React の fetch (Accept: application/json、HX-Request 無し) は 303 ではなく 401 JSON
    // (src/auth/mod.rs の unauthenticated_response)。
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(
        serde_json::from_str::<Value>(&body).unwrap()["error"],
        "auth_required"
    );
}
