//! `/api/admin/hubspot-usage` のテスト。本物の HubSpot には通信しない。

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::Router;
use serde_json::{json, Value};
use tower::ServiceExt;

use crate::audit::fake_turso::start_sqlite_audit;
use crate::handlers::admin::contract_tests::{
    get, get_json, login, seed_fixtures, test_state, ADMIN_EMAIL, USER_EMAIL,
};
use crate::hubspot::{ClientOptions, HubSpotClient};

const TOKEN: &str = "pat-na1-CANARY-USAGE-0123456789";
const API: &str = "/api/admin/hubspot-usage";

/// 応答ヘッダに X-HubSpot-RateLimit-* を付けて Contact を返す偽 HubSpot
async fn spawn_fake() -> String {
    async fn handle(_req: Request<Body>) -> Response {
        Response::builder()
            .status(200)
            .header("content-type", "application/json")
            .header("x-hubspot-ratelimit-max", "190")
            .header("x-hubspot-ratelimit-remaining", "170")
            .header("x-hubspot-ratelimit-secondly", "19")
            .header("x-hubspot-ratelimit-secondly-remaining", "15")
            .header("x-hubspot-ratelimit-daily", "625000")
            .header("x-hubspot-ratelimit-daily-remaining", "612345")
            .body(Body::from(
                json!({"id": "1", "properties": {"firstname": "x"}}).to_string(),
            ))
            .unwrap()
    }
    let router = Router::new().fallback(handle);
    let l = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = l.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(l, router).await.unwrap() });
    format!("http://{addr}")
}

async fn app_with(hubspot: Option<Arc<HubSpotClient>>) -> Router {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    let mut state = test_state(Some(audit));
    Arc::get_mut(&mut state).unwrap().hubspot = hubspot;
    crate::build_app(state)
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者だけが見られ_非管理者は403_未ログインは401() {
    let app = app_with(None).await;
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
    let cookie = login(&app, USER_EMAIL).await;
    let (status, _h, body) = get(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.contains("rate_limit"), "{body}");
}

#[tokio::test(flavor = "multi_thread")]
async fn 管理者には観測値の項目がそろって返り_鍵は出ない() {
    let base = spawn_fake().await;
    let client = Arc::new(
        HubSpotClient::new(
            TOKEN.into(),
            &base,
            ClientOptions {
                timeout: Duration::from_secs(5),
                ..ClientOptions::default()
            },
        )
        .unwrap(),
    );
    // 2 本を同時に (同じ読み取りは 1 回にまとまる) + 別の読み取り 1 本
    let (a, b) = tokio::join!(
        client.get_object("contacts", "1", &["firstname"]),
        client.get_object("contacts", "1", &["firstname"]),
    );
    a.unwrap();
    b.unwrap();
    client.get_object("contacts", "2", &[]).await.unwrap();
    let app = app_with(Some(client.clone())).await;
    let cookie = login(&app, ADMIN_EMAIL).await;
    let (status, v) = get_json(&app, API, Some(&cookie)).await;
    assert_eq!(status, StatusCode::OK, "{v}");
    assert_eq!(v["configured"], true);
    // 最後に見たヘッダの値
    assert_eq!(v["rate_limit"]["per_10s_max"], 190);
    assert_eq!(v["rate_limit"]["per_10s_remaining"], 170);
    assert_eq!(v["rate_limit"]["per_second_max"], 19);
    assert_eq!(v["rate_limit"]["per_second_remaining"], 15);
    assert_eq!(v["rate_limit"]["daily_max"], 625_000);
    assert_eq!(v["rate_limit"]["daily_remaining"], 612_345);
    assert!(v["rate_limit"]["observed_at"].is_string());
    // 起動してからの数 (このテストのクライアント専用の関所)
    let total = v["counters"]["calls"].as_u64().unwrap();
    let coalesced = v["counters"]["coalesced"].as_u64().unwrap();
    assert_eq!(total + coalesced, 3, "{v}");
    assert!(total >= 2, "{v}");
    assert_eq!(v["counters"]["search_calls"], 0);
    assert_eq!(v["counters"]["rate_limited"], 0);
    assert_eq!(v["counters"]["busy_rejected"], 0);
    let groups = v["calls_by_group"].as_array().unwrap();
    assert_eq!(groups[0]["key"], "object_read");
    assert_eq!(groups[0]["label"], "レコード 1 件の読み取り");
    assert_eq!(groups[0]["count"], total);
    // 設定・待ち行列・キャッシュの項目
    for k in [
        "per_second",
        "per_10s",
        "search_interval_ms",
        "interactive_max_wait_ms",
        "background_max_wait_ms",
    ] {
        assert!(v["limits"][k].is_number(), "limits.{k}: {v}");
    }
    assert_eq!(v["queue"]["waiting"], 0);
    assert_eq!(v["queue"]["waiting_search"], 0);
    assert_eq!(v["queue"]["wait_window_secs"], 300);
    assert!(v["queue"]["wait_samples"].as_u64().unwrap() >= 2);
    assert!(v["queue"]["wait_p50_ms"].is_number());
    assert!(v["queue"]["wait_p95_ms"].is_number());
    assert!(v["queue"]["paused_ms"].is_null());
    assert!(v["caches"].is_array());
    // 鍵は出ない
    let text = v.to_string();
    assert!(!text.contains(TOKEN), "{text}");
    assert!(!text.contains("CANARY"), "{text}");
}

#[test]
fn キャッシュと種類の表示名は日本語で_知らない名前はその他() {
    let snap = crate::hubspot::Gateway::new(crate::hubspot::GatewayConfig::default()).snapshot();
    crate::hubspot::gateway::cache_hit("call_queue_page");
    let r = super::build_response(false, &snap);
    let page = r
        .caches
        .iter()
        .find(|c| c.key == "call_queue_page")
        .expect("架電キューのキャッシュ");
    assert_eq!(page.label, "架電キューの一覧 (30 秒)");
    assert!(page.hits >= 1);
    assert_eq!(super::group_label("unknown_group"), "その他");
    assert_eq!(r.limits.per_second, 8);
    assert_eq!(r.limits.per_10s, 80);
    assert_eq!(r.limits.search_interval_ms, 333);
    assert_eq!(r.limits.interactive_max_wait_ms, 5000);
    let v: Value = serde_json::to_value(&r).unwrap();
    assert_eq!(v["configured"], false);
}
