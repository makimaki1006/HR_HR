//! `/api/admin/crm-operations/failed-count` のテスト (管理者の印用の件数)。

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
use tower::ServiceExt;

use super::{shared_failed_count, FailedCountCache};
use crate::audit::fake_turso::start_sqlite_audit;
use crate::handlers::admin::contract_tests::{
    get, get_json, login, seed_fixtures, test_state, ADMIN_EMAIL, USER_EMAIL,
};

const API: &str = "/api/admin/crm-operations/failed-count";

fn insert(conn: &crate::audit::fake_turso::SharedConn, id: &str, status: &str) {
    conn.lock()
        .unwrap()
        .execute(
            "INSERT INTO crm_pending_operations (operation_id, operator_email, deal_id, payload, \
             status, attempts, created_at, updated_at) \
             VALUES (?1, 'bpo@f-a-c.co.jp', '1', '{\"steps\":[]}', ?2, 1, '2026-10-25T00:00:00Z', '2026-10-25T00:00:00Z')",
            rusqlite::params![id, status],
        )
        .unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn 失敗の件数は一覧と同じ条件で数え_60秒キャッシュし_破棄でキャッシュを捨てる() {
    let (audit, conn) = start_sqlite_audit().await;
    seed_fixtures(&conn);
    for i in 0..3 {
        insert(&conn, &format!("op-failed-{i:04}"), "failed");
    }
    // 数えないもの: 破棄済み・再送待ち・保存済み
    insert(&conn, "op-disc-0001", "discarded");
    insert(&conn, "op-pend-0001", "pending");
    insert(&conn, "op-save-0001", "saved");
    shared_failed_count().invalidate();
    let app = crate::build_app(test_state(Some(audit)));

    // 管理者以外は 403 (件数は返らない)、未ログインは 401
    let user = login(&app, USER_EMAIL).await;
    let (s, _h, body) = get(&app, API, Some(&user)).await;
    assert_eq!(s, StatusCode::FORBIDDEN, "{body}");
    assert!(!body.contains("failed"), "{body}");
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

    let admin = login(&app, ADMIN_EMAIL).await;
    let (s, v) = get_json(&app, API, Some(&admin)).await;
    assert_eq!(s, StatusCode::OK, "{v}");
    assert_eq!(v["failed"], 3, "一覧 (status=failed) と同じ 3 件");
    let (_, list) = get_json(
        &app,
        "/api/admin/crm-operations?status=failed",
        Some(&admin),
    )
    .await;
    assert_eq!(list["operations"].as_array().unwrap().len(), 3);

    // 失敗が増えても 60 秒以内はキャッシュのまま (Turso を読まない)
    insert(&conn, "op-failed-0099", "failed");
    let (_, v) = get_json(&app, API, Some(&admin)).await;
    assert_eq!(v["failed"], 3, "キャッシュ");

    // 管理者の破棄でキャッシュを捨てる → 4 件 - 破棄 1 件 = 3 件 (キャッシュのままなら 3 のままなので、もう 1 件足して区別する)
    insert(&conn, "op-failed-0100", "failed");
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/api/admin/crm-operations/op-failed-0000/discard")
                .header(header::COOKIE, &admin)
                .header("x-requested-with", "fetch")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    let (_, v) = get_json(&app, API, Some(&admin)).await;
    assert_eq!(v["failed"], 4, "破棄後に数え直した (5 - 1)");
}

#[tokio::test(flavor = "multi_thread")]
async fn 監査dbが無ければ_503_で件数を返さない() {
    // 管理者の判定は監査 DB を使うので、ルーター経由では 403 になる。ハンドラ単体で確かめる
    let res = super::api_admin_failed_count(axum::extract::State(test_state(None))).await;
    assert_eq!(res.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[test]
fn キャッシュは捨てると空になる() {
    let c = FailedCountCache::default();
    assert_eq!(c.fresh(), None);
    c.put(5);
    assert_eq!(c.fresh(), Some(5));
    c.invalidate();
    assert_eq!(c.fresh(), None);
}
