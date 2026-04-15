use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use rust_dashboard::auth::session::RateLimiter;
use rust_dashboard::config::AppConfig;
use rust_dashboard::db::cache::AppCache;
use rust_dashboard::db::local_sqlite::LocalDb;
use rust_dashboard::{build_app, decompress_db_if_needed, decompress_geojson_if_needed, AppState};

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = AppConfig::from_env();
    let port = config.port;
    tracing::info!("Starting hellowork_dashboard on port {}", port);
    tracing::info!(
        "Auth: internal={}, external={} passwords, domains={:?}, domains_extra={:?}",
        if config.auth_password.is_empty() && config.auth_password_hash.is_empty() {
            "none"
        } else {
            "set"
        },
        config.external_passwords.len(),
        config.allowed_domains,
        config.allowed_domains_extra,
    );

    decompress_geojson_if_needed();
    rust_dashboard::precompress_geojson();
    decompress_db_if_needed(&config.hellowork_db_path);

    let hw_db = match LocalDb::new(&config.hellowork_db_path) {
        Ok(db) => {
            tracing::info!("HelloWork DB loaded: {}", config.hellowork_db_path);
            // インデックス自動作成
            let idx_sqls = [
                "CREATE INDEX IF NOT EXISTS idx_postings_job_pref ON postings (job_type, prefecture)",
                "CREATE INDEX IF NOT EXISTS idx_postings_job_lat_lng ON postings (job_type, latitude, longitude)",
                "CREATE INDEX IF NOT EXISTS idx_postings_lat_lng ON postings (latitude, longitude)",
                "CREATE INDEX IF NOT EXISTS idx_postings_prefecture ON postings (prefecture)",
                "CREATE INDEX IF NOT EXISTS idx_postings_employment ON postings (employment_type)",
                "CREATE INDEX IF NOT EXISTS idx_postings_occ_major ON postings (occupation_major)",
                "CREATE INDEX IF NOT EXISTS idx_postings_salary_type ON postings (salary_type)",
                "CREATE INDEX IF NOT EXISTS idx_postings_recruitment ON postings (recruitment_reason)",
                "CREATE INDEX IF NOT EXISTS idx_postings_pref_muni ON postings (prefecture, municipality)",
                "CREATE INDEX IF NOT EXISTS idx_postings_pref_job ON postings (prefecture, job_type)",
                "CREATE INDEX IF NOT EXISTS idx_postings_industry_raw ON postings (industry_raw)",
                "CREATE INDEX IF NOT EXISTS idx_postings_industry_raw_pref ON postings (industry_raw, prefecture)",
                "CREATE INDEX IF NOT EXISTS idx_postings_salary_min ON postings (salary_min)",
                "CREATE INDEX IF NOT EXISTS idx_postings_facility ON postings (facility_name)",
                "CREATE INDEX IF NOT EXISTS idx_postings_license1 ON postings (license_1)",
                "CREATE INDEX IF NOT EXISTS idx_postings_license2 ON postings (license_2)",
                "CREATE INDEX IF NOT EXISTS idx_postings_license3 ON postings (license_3)",
                "CREATE INDEX IF NOT EXISTS idx_postings_pref_salary ON postings (prefecture, salary_min DESC)",
                "CREATE INDEX IF NOT EXISTS idx_postings_pref_muni_job ON postings (prefecture, municipality, job_type)",
            ];
            for sql in &idx_sqls {
                if let Err(e) = db.execute(sql, &[]) {
                    tracing::warn!("Index creation failed: {e}");
                }
            }
            if let Err(e) = db.execute("ANALYZE", &[]) {
                tracing::warn!("ANALYZE failed: {e}");
            }
            Some(db)
        }
        Err(e) => {
            tracing::warn!("HelloWork DB not available: {e}");
            None
        }
    };

    // Turso外部統計DB接続（環境変数から）
    // reqwest::blocking::Client はasyncコンテキスト内で作成するとパニックするため
    // spawn_blocking で別スレッドで初期化する
    let turso_db = match (
        std::env::var("TURSO_EXTERNAL_URL").ok(),
        std::env::var("TURSO_EXTERNAL_TOKEN").ok(),
    ) {
        (Some(url), Some(token)) if !url.is_empty() && !token.is_empty() => {
            match tokio::task::spawn_blocking(move || {
                rust_dashboard::db::turso_http::TursoDb::new(&url, &token)
            })
            .await
            {
                Ok(Ok(db)) => Some(db),
                Ok(Err(e)) => {
                    tracing::warn!("Turso external DB not available: {e}");
                    None
                }
                Err(e) => {
                    tracing::warn!("Turso external DB init failed: {e}");
                    None
                }
            }
        }
        _ => {
            tracing::info!(
                "Turso external DB not configured (TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN)"
            );
            None
        }
    };

    // SalesNow Turso DB接続（企業分析タブ用）
    let salesnow_db = match (
        std::env::var("SALESNOW_TURSO_URL").ok(),
        std::env::var("SALESNOW_TURSO_TOKEN").ok(),
    ) {
        (Some(url), Some(token)) if !url.is_empty() && !token.is_empty() => {
            match tokio::task::spawn_blocking(move || {
                rust_dashboard::db::turso_http::TursoDb::new(&url, &token)
            })
            .await
            {
                Ok(Ok(db)) => {
                    tracing::info!(
                        "SalesNow DB connected: {}",
                        std::env::var("SALESNOW_TURSO_URL").unwrap_or_default()
                    );
                    Some(db)
                }
                Ok(Err(e)) => {
                    tracing::warn!("SalesNow DB not available: {e}");
                    None
                }
                Err(e) => {
                    tracing::warn!("SalesNow DB init failed: {e}");
                    None
                }
            }
        }
        _ => {
            tracing::info!(
                "SalesNow DB not configured (SALESNOW_TURSO_URL / SALESNOW_TURSO_TOKEN)"
            );
            None
        }
    };

    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(
        config.rate_limit_max_attempts,
        config.rate_limit_lockout_secs,
    );

    // 企業ジオコードキャッシュは無効化（Render無料プラン512MBでOOM発生のため）
    // 代わりにリクエスト時にTursoに直接クエリする方式に変更
    let company_geo_cache: Option<
        Vec<rust_dashboard::handlers::jobmap::company_markers::CompanyGeoEntry>,
    > = None;
    tracing::info!("企業ジオコードキャッシュ: 無効（オンデマンドクエリモード）");

    // 監査DB (AUDIT_TURSO_URL 未設定なら None = 監査機能OFF)
    let audit = if !config.audit_turso_url.is_empty() && !config.audit_turso_token.is_empty() {
        let url = config.audit_turso_url.clone();
        let token = config.audit_turso_token.clone();
        let salt = config.audit_ip_salt.clone();
        match tokio::task::spawn_blocking(move || {
            rust_dashboard::db::turso_http::TursoDb::new(&url, &token)
        })
        .await
        {
            Ok(Ok(turso)) => {
                let audit_db = rust_dashboard::audit::AuditDb::new(turso, salt);
                // テーブル初期化（冪等）
                let turso_clone = audit_db.turso().clone();
                let init_res = tokio::task::spawn_blocking(move || {
                    rust_dashboard::audit::schema::ensure_audit_tables(&turso_clone)
                })
                .await;
                match init_res {
                    Ok(Ok(())) => {
                        tracing::info!("Audit DB ready");
                        Some(audit_db)
                    }
                    Ok(Err(e)) => {
                        tracing::warn!("Audit schema init failed: {e}");
                        None
                    }
                    Err(e) => {
                        tracing::warn!("Audit schema init task failed: {e}");
                        None
                    }
                }
            }
            Ok(Err(e)) => {
                tracing::warn!("Audit DB connection failed: {e}");
                None
            }
            Err(e) => {
                tracing::warn!("Audit DB init task failed: {e}");
                None
            }
        }
    } else {
        tracing::info!(
            "Audit DB not configured (AUDIT_TURSO_URL / AUDIT_TURSO_TOKEN) - 監査機能OFF"
        );
        None
    };

    let state = Arc::new(AppState {
        config,
        hw_db,
        turso_db,
        salesnow_db,
        cache,
        rate_limiter,
        company_geo_cache,
        audit,
    });

    // Phase 3-C: 監査ログ自動削除バッチ (1年より古い entry を削除)
    // 24時間ごとに purge_old_activity を実行。失敗しても本番継続。
    if let Some(audit_for_purge) = state.audit.clone() {
        tokio::spawn(async move {
            // 起動時に 10分 待ってから初回実行 (cold start と競合避け)
            tokio::time::sleep(std::time::Duration::from_secs(600)).await;
            loop {
                let audit_clone = audit_for_purge.clone();
                let res = tokio::task::spawn_blocking(move || {
                    rust_dashboard::audit::purge_old_activity(&audit_clone)
                })
                .await;
                match res {
                    Ok(Ok(())) => tracing::info!("audit purge completed"),
                    Ok(Err(e)) => tracing::warn!("audit purge failed: {e}"),
                    Err(e) => tracing::warn!("audit purge task join failed: {e}"),
                }
                // 次回は 24時間後
                tokio::time::sleep(std::time::Duration::from_secs(86_400)).await;
            }
        });
        tracing::info!("Audit purge scheduler started (every 24h)");
    }

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("Listening on http://localhost:{port}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<SocketAddr>(),
    )
    .await
    .unwrap();
}
