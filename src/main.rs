use std::net::SocketAddr;
use std::sync::Arc;
use tracing_subscriber::EnvFilter;

use rust_dashboard::{
    build_app, decompress_db_if_needed, decompress_geojson_if_needed, AppState,
};
use rust_dashboard::auth::session::RateLimiter;
use rust_dashboard::config::AppConfig;
use rust_dashboard::db::cache::AppCache;
use rust_dashboard::db::local_sqlite::LocalDb;

#[tokio::main]
async fn main() {
    dotenvy::dotenv().ok();

    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()))
        .init();

    let config = AppConfig::from_env();
    let port = config.port;
    tracing::info!("Starting hellowork_dashboard on port {}", port);

    decompress_geojson_if_needed();
    decompress_db_if_needed(&config.hellowork_db_path);

    let hw_db = match LocalDb::new(&config.hellowork_db_path) {
        Ok(db) => {
            tracing::info!("HelloWork DB loaded: {}", config.hellowork_db_path);
            // インデックス自動作成
            let idx_sqls = [
                "CREATE INDEX IF NOT EXISTS idx_postings_job_pref ON postings (job_type, prefecture)",
                "CREATE INDEX IF NOT EXISTS idx_postings_job_lat_lng ON postings (job_type, latitude, longitude)",
                "CREATE INDEX IF NOT EXISTS idx_postings_lat_lng ON postings (latitude, longitude)",
            ];
            for sql in &idx_sqls {
                if let Err(e) = db.execute(sql, &[]) {
                    tracing::warn!("Index creation failed: {e}");
                }
            }
            Some(db)
        }
        Err(e) => {
            tracing::warn!("HelloWork DB not available: {e}");
            None
        }
    };

    let cache = AppCache::new(config.cache_ttl_secs, config.cache_max_entries);
    let rate_limiter = RateLimiter::new(config.rate_limit_max_attempts, config.rate_limit_lockout_secs);

    let state = Arc::new(AppState {
        config,
        hw_db,
        cache,
        rate_limiter,
    });

    let app = build_app(state);

    let addr = format!("0.0.0.0:{port}");
    tracing::info!("Listening on http://localhost:{port}");

    let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>()).await.unwrap();
}
