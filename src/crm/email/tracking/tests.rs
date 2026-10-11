use super::*;
use axum::{body::Body, http::Request, routing::post};
use http_body_util::BodyExt;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, SessionManagerLayer};

async fn store() -> (
    Arc<AuditTrackingStore>,
    crate::audit::fake_turso::SharedConn,
) {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let db = audit.turso().clone();
    tokio::task::spawn_blocking(move || {
        ensure_tables(&db).unwrap();
        ensure_tables(&db).unwrap();
    })
    .await
    .unwrap();
    (Arc::new(AuditTrackingStore::new(audit)), conn)
}
fn meta() -> TrackingMeta {
    TrackingMeta {
        operator_email: "operator@f-a-c.co.jp".into(),
        hubspot_deal_id: "100".into(),
        hubspot_contact_id: "200".into(),
        subject: "ご案内（架空）".into(),
    }
}
fn state() -> Arc<AppState> {
    let mut config = crate::config::AppConfig::from_env();
    config.allowed_domains = vec!["f-a-c.co.jp".into()];
    config.admin_emails = vec![];
    config.auth_password = "test-only".into();
    config.auth_password_hash = String::new();
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
async fn body(r: Response) -> Vec<u8> {
    r.into_body().collect().await.unwrap().to_bytes().to_vec()
}
async fn get(app: &Router, path: &str) -> Response {
    app.clone()
        .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

#[tokio::test(flavor = "multi_thread")]
async fn html_rewrites_only_web_anchors_and_adds_one_transparent_pixel() {
    let (store, conn) = store().await;
    let html = r##"<!doctype html><html><body><a HREF='https://example.invalid/a?x=1&amp;y=2'>A</a><a href=http://example.invalid/b>B</a><a href="mailto:a@example.invalid">mail</a><a href="tel:123">phone</a><a href="#section">anchor</a><a href="/relative">relative</a><img src="https://example.invalid/logo.png"><img data-hr-email-open="1" src="https://old.invalid/pixel"><!-- <a href="https://ignored.invalid">comment</a> --><script>const x='<a href="https://script.invalid">';</script></body></html>"##;
    let t = instrument_html(store.as_ref(), "https://hr.example.invalid", html, meta())
        .await
        .unwrap();
    assert!(valid_id(&t.tracking_id));
    assert_eq!(t.html.matches("data-hr-email-open").count(), 1);
    assert_eq!(t.html.matches("/t/o/").count(), 1);
    assert_eq!(t.html.matches("/t/c/").count(), 2);
    assert!(t.html.contains("mailto:a@example.invalid"));
    assert!(t.html.contains("tel:123"));
    assert!(t.html.contains("#section"));
    assert!(t.html.contains("/relative"));
    assert!(t.html.contains("logo.png"));
    assert!(!t.html.contains("old.invalid"));
    assert_eq!(
        store.destination(&t.tracking_id, 1).await.unwrap().unwrap(),
        "https://example.invalid/a?x=1&y=2"
    );
    assert_eq!(
        store.destination(&t.tracking_id, 2).await.unwrap().unwrap(),
        "http://example.invalid/b"
    );
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_email_tracking_links", [], |r| r
                .get::<_, i64>(
                0
            ))
            .unwrap(),
        2
    );
    let again = instrument_html(
        store.as_ref(),
        "https://hr.example.invalid",
        "<p>hello</p>",
        meta(),
    )
    .await
    .unwrap();
    assert_ne!(again.tracking_id, t.tracking_id);
    assert_eq!(GIF[6..10], [1, 0, 1, 0]);
    assert_eq!(GIF[19..27], [33, 249, 4, 1, 0, 0, 0, 0]);
}

#[tokio::test(flavor = "multi_thread")]
async fn invalid_base_or_excessive_html_never_creates_a_record() {
    let (store, conn) = store().await;
    for base in [
        "javascript:alert(1)",
        "https://hr.invalid/?redirect=evil",
        "https://user:pass@hr.invalid",
        "https://hr.invalid/path",
    ] {
        assert!(matches!(
            instrument_html(store.as_ref(), base, "<p>x</p>", meta()).await,
            Err(TrackingError::InvalidInput)
        ));
    }
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_email_tracking", [], |r| r
                .get::<_, i64>(0))
            .unwrap(),
        0
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn binding_is_idempotent_and_can_complete_pending_hubspot_record() {
    let (store, _) = store().await;
    let t = instrument_html(store.as_ref(), "https://hr.invalid", "", meta())
        .await
        .unwrap();
    bind_sent(store.as_ref(), &t.tracking_id, None, "gmail-1")
        .await
        .unwrap();
    bind_sent(store.as_ref(), &t.tracking_id, Some("999"), "gmail-1")
        .await
        .unwrap();
    bind_sent(store.as_ref(), &t.tracking_id, Some("999"), "gmail-1")
        .await
        .unwrap();
    assert!(store.summary("999").await.unwrap().is_some());
    assert_eq!(
        bind_sent(store.as_ref(), &t.tracking_id, Some("998"), "gmail-1").await,
        Err(TrackingError::BindConflict)
    );
    assert_eq!(
        bind_sent(store.as_ref(), &t.tracking_id, Some("999"), "gmail-2").await,
        Err(TrackingError::BindConflict)
    );
}

#[test]
fn automatic_checks_are_separate_from_normal_requests() {
    let mut h = HeaderMap::new();
    h.insert(header::USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
    assert!(automatic_hint(&h, 109_999, 100_000));
    assert!(!automatic_hint(&h, 110_000, 100_000));
    for ua in [
        "Proofpoint URL Defense",
        "Mimecast",
        "Barracuda Link Check",
        "AppleMail",
        "MailPrivacyProtection",
        "Microsoft Office Existence Discovery",
        "SafeLinks",
    ] {
        h.insert(header::USER_AGENT, HeaderValue::from_str(ua).unwrap());
        assert!(automatic_hint(&h, 200_000, 100_000));
    }
    h.insert(header::USER_AGENT, HeaderValue::from_static("Mozilla/5.0"));
    h.insert(
        "sec-purpose",
        HeaderValue::from_static("prefetch;prerender"),
    );
    assert!(automatic_hint(&h, 200_000, 100_000));
}

#[tokio::test(flavor = "multi_thread")]
async fn unknown_ids_do_not_write_and_redirect_uses_only_saved_destination() {
    let (store, conn) = store().await;
    let t = instrument_html(
        store.as_ref(),
        "https://hr.invalid",
        r#"<a href="https://saved.invalid/path">x</a>"#,
        meta(),
    )
    .await
    .unwrap();
    bind_sent(store.as_ref(), &t.tracking_id, Some("999"), "gmail")
        .await
        .unwrap();
    let ctx = Arc::new(TrackingContext::new(Some(store), CrmAccess::default()));
    let app = router(ctx)
        .with_state(state())
        .layer(SessionManagerLayer::new(MemoryStore::default()));
    let unknown = "00000000000000000000000000000000";
    let r = get(&app, &format!("/t/o/{unknown}.gif")).await;
    assert_eq!(r.status(), StatusCode::OK);
    assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    assert_eq!(r.headers()[header::CONTENT_TYPE], "image/gif");
    assert_eq!(body(r).await, GIF);
    assert_eq!(
        get(&app, &format!("/t/c/{unknown}/1")).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        get(&app, &format!("/t/c/{}/99", t.tracking_id))
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_email_tracking_events", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
        0
    );
    for _ in 0..4 {
        let r = get(
            &app,
            &format!("/t/c/{}/1?url=https://evil.invalid", t.tracking_id),
        )
        .await;
        assert_eq!(r.status(), StatusCode::FOUND);
        assert_eq!(r.headers()[header::LOCATION], "https://saved.invalid/path");
        assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
    }
    for _ in 0..4 {
        assert_eq!(
            get(&app, &format!("/t/o/{}.gif", t.tracking_id))
                .await
                .status(),
            StatusCode::OK
        );
    }
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_email_tracking_events", [], |r| {
                r.get::<_, i64>(0)
            })
            .unwrap(),
        2
    );
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row(
                "SELECT SUM(automatic) FROM crm_email_tracking_events",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        2
    );
}

async fn inject(session: Session, Path(method): Path<String>) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, "operator@f-a-c.co.jp")
        .await
        .unwrap();
    session
        .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, method)
        .await
        .unwrap();
    StatusCode::OK
}

#[tokio::test(flavor = "multi_thread")]
async fn write_budget_drops_measurements_without_breaking_saved_redirects() {
    let (store, conn) = store().await;
    let html = (0..100)
        .map(|n| format!("<a href=\"https://saved.invalid/{n}\">link</a>"))
        .collect::<String>();
    let tracked = instrument_html(store.as_ref(), "https://hr.invalid", &html, meta())
        .await
        .unwrap();
    bind_sent(
        store.as_ref(),
        &tracked.tracking_id,
        Some("987"),
        "gmail-budget",
    )
    .await
    .unwrap();
    conn.lock()
        .unwrap()
        .execute("UPDATE crm_email_tracking SET sent_at=0", [])
        .unwrap();
    let app = router(Arc::new(TrackingContext::new(
        Some(store),
        CrmAccess::default(),
    )))
    .with_state(state());
    for (automatic, max) in [(false, 100), (true, 21)] {
        for no in 1..=max {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(format!("/t/c/{}/{no}", tracked.tracking_id))
                        .header(
                            header::USER_AGENT,
                            if automatic {
                                "Proofpoint"
                            } else {
                                "Mozilla/5.0"
                            },
                        )
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FOUND);
            assert_eq!(
                response.headers()[header::LOCATION],
                format!("https://saved.invalid/{}", no - 1)
            );
        }
    }
    let (total, automatic): (i64, i64) = conn
        .lock()
        .unwrap()
        .query_row(
            "SELECT COUNT(*), SUM(automatic) FROM crm_email_tracking_events",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!((total, automatic), (120, 20));
}
#[tokio::test(flavor = "multi_thread")]
async fn aggregate_api_has_exact_counts_dates_and_crm_rbac() {
    let (store, _) = store().await;
    let t = instrument_html(
        store.as_ref(),
        "https://hr.invalid",
        r#"<a href="https://saved.invalid/a">a</a><a href="https://saved.invalid/b">b</a>"#,
        meta(),
    )
    .await
    .unwrap();
    bind_sent(store.as_ref(), &t.tracking_id, Some("999"), "gmail")
        .await
        .unwrap();
    let ts = DateTime::parse_from_rfc3339("2026-10-12T05:00:00Z")
        .unwrap()
        .timestamp_millis();
    for (kind, no, offset, automatic) in [
        ("open", 0, 0, false),
        ("open", 0, 60_000, false),
        ("open", 0, 60_000, false),
        ("open", 0, 120_000, true),
        ("click", 1, 180_000, false),
        ("click", 1, 240_000, false),
        ("click", 2, 300_000, true),
    ] {
        store
            .record(TrackingEvent {
                tracking_id: t.tracking_id.clone(),
                kind,
                link_no: no,
                at: ts + offset,
                automatic,
            })
            .await
            .unwrap();
    }
    store
        .record(TrackingEvent {
            tracking_id: "00000000000000000000000000000000".into(),
            kind: "open",
            link_no: 0,
            at: ts,
            automatic: false,
        })
        .await
        .unwrap();
    let ctx = Arc::new(TrackingContext::new(
        Some(store.clone()),
        CrmAccess::default(),
    ));
    let app = router(ctx)
        .route("/inject/{method}", post(inject))
        .with_state(state())
        .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));
    assert_eq!(
        get(&app, "/api/crm/emails/999/tracking").await.status(),
        StatusCode::UNAUTHORIZED
    );
    for (method, status) in [
        ("password", StatusCode::FORBIDDEN),
        (crate::auth::LOGIN_METHOD_GOOGLE_OIDC, StatusCode::OK),
    ] {
        let res = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/inject/{method}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let cookie = res.headers()[header::SET_COOKIE]
            .to_str()
            .unwrap()
            .split(';')
            .next()
            .unwrap();
        let request = |path: &str| {
            Request::builder()
                .uri(path)
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap()
        };
        let res = app
            .clone()
            .oneshot(request("/api/crm/emails/999/tracking"))
            .await
            .unwrap();
        assert_eq!(res.status(), status);
        if status == StatusCode::OK {
            assert_eq!(res.headers()[header::CACHE_CONTROL], "no-store");
            let v: EmailTrackingSummary = serde_json::from_slice(&body(res).await).unwrap();
            assert_eq!(
                v.opens,
                TrackingCounts {
                    count: 2,
                    first_at: Some("2026-10-12T05:00:00.000Z".into()),
                    last_at: Some("2026-10-12T05:01:00.000Z".into())
                }
            );
            assert_eq!(v.automatic_opens.count, 1);
            assert_eq!(v.clicks.count, 2);
            assert_eq!(v.automatic_clicks.count, 1);
            assert_eq!(v.links.len(), 2);
            assert_eq!(v.links[0].clicks.count, 2);
            assert_eq!(v.links[1].automatic_clicks.count, 1);
            assert_eq!(
                app.clone()
                    .oneshot(request("/api/crm/emails/888/tracking"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::NO_CONTENT
            );
            assert_eq!(
                app.clone()
                    .oneshot(request("/api/crm/emails/invalid/tracking"))
                    .await
                    .unwrap()
                    .status(),
                StatusCode::BAD_REQUEST
            );
        }
    }
}

#[tokio::test]
async fn actual_app_routes_keep_other_authentication_and_csrf_guards() {
    let app = crate::build_app(state());
    assert_eq!(
        get(&app, "/t/o/00000000000000000000000000000000.gif")
            .await
            .status(),
        StatusCode::OK
    );
    assert_eq!(
        get(&app, "/t/c/00000000000000000000000000000000/1")
            .await
            .status(),
        StatusCode::NOT_FOUND
    );
    for path in [
        "/api/crm/emails/999/tracking",
        "/api/crm/metadata",
        "/api/crm/deals/100",
    ] {
        assert_eq!(get(&app, path).await.status(), StatusCode::UNAUTHORIZED);
    }
    assert_eq!(get(&app, "/app/crm").await.status(), StatusCode::SEE_OTHER);
    assert_eq!(
        app.oneshot(
            Request::builder()
                .method("PATCH")
                .uri("/api/crm/deals/100")
                .header("origin", "https://evil.invalid")
                .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                .body(Body::from("email=a%40f-a-c.co.jp&password=test-only"))
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::FORBIDDEN
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn disabled_feature_returns_no_measurements_after_authentication() {
    let app = router(Arc::new(TrackingContext::new(None, CrmAccess::default())))
        .route("/inject/{method}", post(inject))
        .with_state(state())
        .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));
    let res = app
        .clone()
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(format!("/inject/{}", crate::auth::LOGIN_METHOD_GOOGLE_OIDC))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let cookie = res.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap();
    assert_eq!(
        app.oneshot(
            Request::builder()
                .uri("/api/crm/emails/999/tracking")
                .header(header::COOKIE, cookie)
                .body(Body::empty())
                .unwrap()
        )
        .await
        .unwrap()
        .status(),
        StatusCode::NO_CONTENT
    );
}

#[test]
fn memory_limits_are_bounded_and_recover_after_one_minute() {
    let now = Instant::now();
    let mut window = Window { at: now, used: 0 };
    for _ in 0..120 {
        assert!(window.take(now, 120));
    }
    assert!(!window.take(now, 120));
    assert!(window.take(now + Duration::from_secs(60), 120));
}

#[test]
fn export_tracking_types() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated");
    EmailTrackingSummary::export_all(&ts_rs::Config::new().with_out_dir(dir)).unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn yearly_purge_uses_sent_time_and_removes_all_three_tables_together() {
    let (store, conn) = store().await;
    let cutoff = 1_800_000_000_000i64;
    for (n, created_at, sent_at) in [
        (1, cutoff - 100, Some(cutoff - 1)),
        (2, cutoff - 100, Some(cutoff)),
        (3, cutoff - 100, Some(cutoff + 1)),
        (4, cutoff - 1, None),
        (5, cutoff + 1, None),
    ] {
        let id = format!("{n:032x}");
        store
            .save(
                SentEmail {
                    tracking_id: id.clone(),
                    meta: meta(),
                    created_at,
                    sent_at: None,
                },
                vec![TrackingLink {
                    link_no: 1,
                    url: "https://saved.invalid".into(),
                }],
            )
            .await
            .unwrap();
        conn.lock()
            .unwrap()
            .execute(
                "UPDATE crm_email_tracking SET sent_at=? WHERE tracking_id=?",
                rusqlite::params![sent_at, id],
            )
            .unwrap();
        for (kind, link_no) in [("open", 0), ("click", 1)] {
            store
                .record(TrackingEvent {
                    tracking_id: id.clone(),
                    kind,
                    link_no,
                    at: cutoff + 100,
                    automatic: false,
                })
                .await
                .unwrap();
        }
    }
    // A database created by the first version has the tables but no purge trigger yet.
    conn.lock()
        .unwrap()
        .execute("DROP TRIGGER crm_email_tracking_delete_children", [])
        .unwrap();
    let db = store.audit.turso().clone();
    tokio::task::spawn_blocking(move || {
        purge_old_tracking(&db, cutoff).unwrap();
        purge_old_tracking(&db, cutoff).unwrap();
    })
    .await
    .unwrap();
    {
        let db = conn.lock().unwrap();
        for (table, expected) in [
            ("crm_email_tracking", 3),
            ("crm_email_tracking_links", 3),
            ("crm_email_tracking_events", 6),
        ] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                expected
            );
        }
        let remaining = db
            .prepare("SELECT tracking_id FROM crm_email_tracking ORDER BY tracking_id")
            .unwrap()
            .query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(remaining, [2, 3, 5].map(|n| format!("{n:032x}")));
    }
    let app = router(Arc::new(TrackingContext::new(
        Some(store),
        CrmAccess::default(),
    )))
    .with_state(state());
    let expired = format!("{:032x}", 1);
    assert_eq!(
        get(&app, &format!("/t/c/{expired}/1")).await.status(),
        StatusCode::NOT_FOUND
    );
    assert_eq!(
        body(get(&app, &format!("/t/o/{expired}.gif")).await).await,
        GIF
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn failed_child_purge_rolls_back_parent_and_all_measurements() {
    let (store, conn) = store().await;
    let tracked = instrument_html(
        store.as_ref(),
        "https://hr.invalid",
        "<a href='https://saved.invalid'>x</a>",
        meta(),
    )
    .await
    .unwrap();
    store
        .record(TrackingEvent {
            tracking_id: tracked.tracking_id,
            kind: "open",
            link_no: 0,
            at: 1,
            automatic: false,
        })
        .await
        .unwrap();
    conn.lock().unwrap().execute_batch(
        "CREATE TRIGGER fail_tracking_purge BEFORE DELETE ON crm_email_tracking_events BEGIN SELECT RAISE(ABORT,'test failure'); END;"
    ).unwrap();
    let db = store.audit.turso().clone();
    let cutoff = Utc::now().timestamp_millis() + 60_000;
    tokio::task::spawn_blocking(move || assert!(purge_old_tracking(&db, cutoff).is_err()))
        .await
        .unwrap();
    {
        let db = conn.lock().unwrap();
        for table in [
            "crm_email_tracking",
            "crm_email_tracking_links",
            "crm_email_tracking_events",
        ] {
            assert_eq!(
                db.query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                    .unwrap(),
                1
            );
        }
        db.execute("DROP TRIGGER fail_tracking_purge", []).unwrap();
    }
    let db = store.audit.turso().clone();
    tokio::task::spawn_blocking(move || purge_old_tracking(&db, cutoff).unwrap())
        .await
        .unwrap();
    for table in [
        "crm_email_tracking",
        "crm_email_tracking_links",
        "crm_email_tracking_events",
    ] {
        assert_eq!(
            conn.lock()
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn existing_audit_purge_handles_uninstalled_and_disabled_tracking() {
    let (audit, conn) = crate::audit::fake_turso::start_sqlite_audit().await;
    let first = audit.clone();
    tokio::task::spawn_blocking(move || crate::audit::purge_old_activity(&first).unwrap())
        .await
        .unwrap();
    assert_eq!(
        conn.lock()
            .unwrap()
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE name LIKE 'crm_email_tracking%'",
                [],
                |r| r.get::<_, i64>(0)
            )
            .unwrap(),
        0
    );
    let db = audit.turso().clone();
    tokio::task::spawn_blocking(move || ensure_tables(&db).unwrap())
        .await
        .unwrap();
    let old = (Utc::now() - chrono::Duration::days(366)).timestamp_millis();
    let store = AuditTrackingStore::new(audit.clone());
    store
        .save(
            SentEmail {
                tracking_id: format!("{:032x}", 1),
                meta: meta(),
                created_at: old,
                sent_at: None,
            },
            vec![TrackingLink {
                link_no: 1,
                url: "https://saved.invalid".into(),
            }],
        )
        .await
        .unwrap();
    // No feature/key check in the scheduler path: cleanup keeps working after disabling sending.
    tokio::task::spawn_blocking(move || crate::audit::purge_old_activity(&audit).unwrap())
        .await
        .unwrap();
    for table in ["crm_email_tracking", "crm_email_tracking_links"] {
        assert_eq!(
            conn.lock()
                .unwrap()
                .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r
                    .get::<_, i64>(0))
                .unwrap(),
            0
        );
    }
}
