use super::*;
use crate::audit::fake_turso::start_sqlite_audit_with_queries;
fn id(n: usize) -> String {
    format!("00000000-0000-4000-8000-{n:012}")
}
fn create(n: usize, scope: &str, query: &str) -> Mutation {
    Mutation::Create(Create {
        id: id(n),
        name: format!("架空ビュー {n}"),
        scope: scope.into(),
        query: query.into(),
        position: None,
    })
}
#[tokio::test(flavor = "multi_thread")]
async fn crud_preserves_query_privacy_permissions_and_cache_invalidation() {
    let (audit, conn, queries) = start_sqlite_audit_with_queries().await;
    let store = ViewsStore::default();
    let alpha = "alpha@example.invalid";
    let beta = "beta@example.invalid";
    let query = "owner=all&q=%E6%9E%B6%E7%A9%BA&sort=next_call_desc";
    let own = store
        .mutate(
            audit.clone(),
            alpha.into(),
            false,
            create(1, "private", query),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(own.query, query);
    assert!(store
        .list(audit.clone(), beta.into())
        .await
        .unwrap()
        .is_empty());
    store.list(audit.clone(), beta.into()).await.unwrap();
    assert_eq!(
        queries
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.contains("ORDER BY position"))
            .count(),
        1
    );
    let shared = store
        .mutate(
            audit.clone(),
            alpha.into(),
            false,
            create(2, "shared", "owner=unassigned"),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        store
            .list(audit.clone(), beta.into())
            .await
            .unwrap()
            .iter()
            .map(|v| v.id.clone())
            .collect::<Vec<_>>(),
        vec![id(2)]
    );
    assert!(matches!(
        store
            .mutate(
                audit.clone(),
                beta.into(),
                false,
                Mutation::Update(
                    shared.id.clone(),
                    Patch {
                        name: Some("変更不可".into()),
                        ..Default::default()
                    }
                )
            )
            .await,
        Err(Error::Forbidden)
    ));
    assert!(matches!(
        store
            .mutate(audit.clone(), beta.into(), true, Mutation::Delete(own.id))
            .await,
        Err(Error::Missing)
    ));
    let changed = store
        .mutate(
            audit.clone(),
            beta.into(),
            true,
            Mutation::Update(
                shared.id.clone(),
                Patch {
                    name: Some("共有の架空一覧".into()),
                    query: Some("due=today".into()),
                    position: Some(-1.0),
                    ..Default::default()
                },
            ),
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        (
            changed.name.as_str(),
            changed.query.as_str(),
            changed.position
        ),
        ("共有の架空一覧", "due=today", -1.0)
    );
    assert_eq!(
        store.list(audit.clone(), alpha.into()).await.unwrap().len(),
        2
    );
    store
        .mutate(
            audit.clone(),
            alpha.into(),
            false,
            Mutation::Delete(shared.id),
        )
        .await
        .unwrap();
    assert!(store.list(audit, beta.into()).await.unwrap().is_empty());
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_saved_views", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        1
    );
}
#[tokio::test(flavor = "multi_thread")]
async fn quotas_are_atomic_and_apply_to_scope_changes_and_idempotent_retries() {
    let (audit, conn, _) = start_sqlite_audit_with_queries().await;
    let store = ViewsStore::default();
    let owner = "alpha@example.invalid";
    for n in 1..=50 {
        store
            .mutate(audit.clone(), owner.into(), false, create(n, "private", ""))
            .await
            .unwrap();
    }
    assert!(matches!(
        store
            .mutate(
                audit.clone(),
                owner.into(),
                false,
                create(51, "private", "")
            )
            .await,
        Err(Error::Limit)
    ));
    store
        .mutate(audit.clone(), owner.into(), false, create(1, "private", ""))
        .await
        .unwrap();
    for n in 101..=200 {
        store
            .mutate(audit.clone(), owner.into(), false, create(n, "shared", ""))
            .await
            .unwrap();
    }
    assert!(matches!(
        store
            .mutate(
                audit.clone(),
                "beta@example.invalid".into(),
                false,
                create(201, "shared", "")
            )
            .await,
        Err(Error::Limit)
    ));
    assert!(matches!(
        store
            .mutate(
                audit.clone(),
                owner.into(),
                false,
                Mutation::Update(
                    id(1),
                    Patch {
                        scope: Some("shared".into()),
                        ..Default::default()
                    }
                )
            )
            .await,
        Err(Error::Limit)
    ));
    assert!(matches!(
        store
            .mutate(
                audit,
                owner.into(),
                false,
                Mutation::Update(
                    id(101),
                    Patch {
                        scope: Some("private".into()),
                        ..Default::default()
                    }
                )
            )
            .await,
        Err(Error::Limit)
    ));
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_saved_views", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        150
    );
}
#[test]
fn rejects_invalid_or_long_queries_without_reencoding_valid_queries() {
    for q in [
        "unknown=1",
        "owner=not-an-owner",
        "sort=not-a-sort",
        "next_from=2026-02-30",
        "conditions=broken",
        "q=x#fragment",
    ] {
        assert!(
            matches!(valid("架空", "private", q, None), Err(Error::Invalid)),
            "{q}"
        );
    }
    assert!(valid("架空", "private", "owner=all&q=two%20words", None).is_ok());
    assert!(valid("架空", "private", &"q=".to_owned().repeat(1001), None).is_err());
    assert!(valid("架空", "unsupported", "", None).is_err());
}
#[tokio::test(flavor = "multi_thread")]
async fn api_enforces_auth_csrf_bad_query_and_missing_audit() {
    use axum::{
        body::{to_bytes, Body},
        http::{header, Request},
        routing::post,
        Router,
    };
    use tower::ServiceExt;
    use tower_sessions::{MemoryStore, SessionManagerLayer};
    async fn login(session: Session) -> StatusCode {
        session
            .insert(crate::auth::SESSION_USER_KEY, "alpha@example.invalid")
            .await
            .unwrap();
        session
            .insert(
                crate::auth::SESSION_LOGIN_METHOD_KEY,
                crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
            )
            .await
            .unwrap();
        StatusCode::OK
    }
    for with_audit in [false, true] {
        let audit = if with_audit {
            Some(start_sqlite_audit_with_queries().await.0)
        } else {
            None
        };
        let mut config = crate::config::AppConfig::from_env();
        config.allowed_domains = vec!["example.invalid".into()];
        config.admin_emails.clear();
        let state = Arc::new(AppState {
            config,
            hw_db: None,
            indeed_db: None,
            turso_db: None,
            salesnow_db: None,
            scout_db: None,
            cache: crate::db::cache::AppCache::new(60, 10),
            rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
            company_geo_cache: None,
            audit,
            google_oidc: None,
            hubspot: None,
            crm_hubspot: None,
        });
        let app = Router::new()
            .merge(super::super::routes::router(rbac::CrmAccess::default()))
            .route("/__login", post(login))
            .with_state(state)
            .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/crm/views")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/__login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = response.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap()
            .to_owned();
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/api/crm/views")
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            response.status().as_u16(),
            if with_audit { 200 } else { 503 }
        );
        for (q, expected) in [
            ("owner=all".to_owned(), 200),
            ("sort=bad".to_owned(), 400),
            (format!("q={}", "%20".repeat(667)), 400),
        ] {
            let response=app.clone().oneshot(Request::builder().method("POST").uri("/api/crm/views").header(header::COOKIE,&cookie).header(header::CONTENT_TYPE,"application/json").header("X-Requested-With","fetch").body(Body::from(serde_json::json!({"id":id(500),"name":"架空一覧","scope":"private","query":q}).to_string())).unwrap()).await.unwrap();
            assert_eq!(
                response.status().as_u16(),
                if with_audit { expected } else { 503 }
            );
            let _ = to_bytes(response.into_body(), 32768).await.unwrap();
        }
        let response = app
            .oneshot(
                Request::builder()
                    .method("DELETE")
                    .uri(format!("/api/crm/views/{}", id(500)))
                    .header(header::COOKIE, &cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::FORBIDDEN);
    }
}
