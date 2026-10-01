use crate::auth::session::RateLimiter;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::{build_app, AppState};
use std::sync::Arc;

fn bare_state() -> Arc<AppState> {
    let config = AppConfig::from_env();
    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(
        config.rate_limit_max_attempts,
        config.rate_limit_lockout_secs,
    );
    Arc::new(AppState {
        config,
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache,
        rate_limiter,
        company_geo_cache: None,
        audit: None,
        google_oidc: None,
    })
}

#[test]
fn population_context_keeps_official_minimum_wage_without_database() {
    let value = super::competitor::population_context(&bare_state(), "大阪府");
    let rate =
        crate::minimum_wage::official_at("大阪府", crate::minimum_wage::japan_today()).unwrap();
    assert_eq!(value["status"], "ok");
    assert_eq!(
        value["minimum_wage"].as_f64(),
        Some(rate.hourly_min_wage as f64)
    );
    assert_eq!(value["minimum_wage_fiscal_year"], rate.fiscal_year);
    assert_eq!(
        value["minimum_wage_effective_date"],
        rate.effective_date.to_string()
    );
    assert_eq!(value["minimum_wage_source"], "official_csv");
    assert_eq!(value["minimum_wage_source_url"], rate.source_url);
    assert_eq!(
        value["minimum_wage_as_of"],
        crate::minimum_wage::japan_today().to_string()
    );
    assert!(value["bands"].as_array().unwrap().is_empty());
    assert!(value["labor"].is_null());
}

#[tokio::test]
async fn competitor_routes_require_login_and_reject_foreign_origin() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    let app = build_app(bare_state());
    for (method, path) in [("GET", "/competitor"), ("POST", "/report/competitor")] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("origin", "https://hr-hw.onrender.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::SEE_OTHER);
        assert_eq!(response.headers()["location"], "/login");
    }
    let response = app
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/report/competitor")
                .header("origin", "https://foreign.example")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[tokio::test]
async fn competitor_csv_upload_produces_separate_report_and_rejects_empty_file() {
    use axum::body::Body;
    use axum::http::{Request, StatusCode};
    use axum::routing::{get, post};
    use http_body_util::BodyExt;
    use tower::ServiceExt;

    let app = axum::Router::new()
        .route("/competitor", get(crate::handlers::competitor::page))
        .route(
            "/report/competitor",
            post(crate::handlers::competitor::report),
        )
        .with_state(bare_state());
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/competitor")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let page = String::from_utf8(
        response
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes()
            .to_vec(),
    )
    .unwrap();
    assert!(page.contains("name=\"market_title\""));
    assert!(page.contains("name=\"search_keyword\""));
    assert!(!page.contains("{{TITLE_OPTIONS}}"));
    if let Some(root) = std::env::var_os("COMPETITOR_PREVIEW_DIR") {
        let root = std::path::PathBuf::from(root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("index.html"), &page).unwrap();
    }

    for (csv, expected_status) in [
        ("タイトル,会社名,勤務地,給与,雇用形態\n施設長,A社,大阪府大阪市,月給 25万円 ~ 30万円,正社員\n施設長,B社,大阪府大阪市,月給 35万円 ~ 40万円,正社員\n", StatusCode::OK),
        ("", StatusCode::BAD_REQUEST),
    ] {
        let mut body = String::new();
        for (name, value) in [("source_type", "indeed"), ("wage_mode", "monthly"), ("top_n", "1"), ("prefecture", "大阪府")] {
            body.push_str(&format!("--competitor-boundary\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"));
        }
        body.push_str(&format!("--competitor-boundary\r\nContent-Disposition: form-data; name=\"csv_file\"; filename=\"jobs.csv\"\r\nContent-Type: text/csv\r\n\r\n{csv}\r\n--competitor-boundary--\r\n"));
        let response = app.clone().oneshot(Request::builder().method("POST").uri("/report/competitor").header("content-type", "multipart/form-data; boundary=competitor-boundary").body(Body::from(body)).unwrap()).await.unwrap();
        assert_eq!(response.status(), expected_status);
        let html = String::from_utf8(response.into_body().collect().await.unwrap().to_bytes().to_vec()).unwrap();
        if expected_status == StatusCode::OK {
            if let Some(root) = std::env::var_os("COMPETITOR_PREVIEW_DIR") {
                std::fs::write(std::path::PathBuf::from(root).join("report.html"), &html).unwrap();
            }
            assert!(html.contains("CSV重複排除後 2 件"));
            assert!(html.contains("競合調査ダッシュボード"));
            assert!(html.contains("30")); // 下限 (25 + 35) / 2 = 30万円
            assert!(html.contains("上位 1 件"));
            assert!(html.contains("Google広告API"));
            assert!(!html.contains("地域企業構造"));
        } else {
            assert!(html.contains("求人一覧CSVを選択してください"));
        }
    }
}
