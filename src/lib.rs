pub mod auth;
pub mod config;
pub mod db;
pub mod geo;
pub mod handlers;
pub mod models;

use axum::{
    extract::{Form, FromRequest, State},
    middleware,
    response::{Html, IntoResponse, Redirect},
    routing::{get, post},
    Router,
};
use std::sync::Arc;
use tower_http::compression::CompressionLayer;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::{Expiry, MemoryStore, Session, SessionManagerLayer};

use auth::{
    require_auth, validate_email_domain, verify_password,
    SESSION_JOB_TYPE_KEY, SESSION_MUNICIPALITY_KEY, SESSION_PREFECTURE_KEY, SESSION_USER_KEY,
};
use config::AppConfig;
use db::cache::AppCache;
use models::job_seeker::PREFECTURE_ORDER;

/// アプリケーション共有状態
pub struct AppState {
    pub config: AppConfig,
    pub hw_db: Option<db::local_sqlite::LocalDb>,
    pub cache: AppCache,
    pub rate_limiter: auth::session::RateLimiter,
}

/// アプリケーションRouter構築
pub fn build_app(state: Arc<AppState>) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    let protected_routes = Router::new()
        .route("/", get(dashboard_page))
        .route("/tab/overview", get(handlers::overview::tab_overview))
        .route(
            "/tab/demographics",
            get(handlers::demographics::tab_demographics),
        )
        .route("/tab/balance", get(handlers::balance::tab_balance))
        .route("/tab/workstyle", get(handlers::workstyle::tab_workstyle))
        .route("/tab/jobmap", get(handlers::jobmap::tab_jobmap))
        .route("/api/jobmap/markers", get(handlers::jobmap::jobmap_markers))
        .route("/api/jobmap/detail/{id}", get(handlers::jobmap::jobmap_detail))
        .route("/api/jobmap/detail-json/{id}", get(handlers::jobmap::jobmap_detail_json))
        .route("/api/jobmap/stats", post(handlers::jobmap::jobmap_stats))
        .route("/api/jobmap/municipalities", get(handlers::jobmap::jobmap_municipalities))
        .route("/api/jobmap/seekers", get(handlers::jobmap::jobmap_seekers))
        .route("/api/jobmap/seeker-detail", get(handlers::jobmap::jobmap_seeker_detail))
        .route("/api/jobmap/region/summary", get(handlers::jobmap::region_summary))
        .route("/api/jobmap/region/age_gender", get(handlers::jobmap::region_age_gender))
        .route("/api/jobmap/region/posting_stats", get(handlers::jobmap::region_posting_stats))
        .route("/api/jobmap/region/segments", get(handlers::jobmap::region_segments))
        .route(
            "/tab/competitive",
            get(handlers::competitive::tab_competitive),
        )
        .route(
            "/api/geojson/{filename}",
            get(handlers::api::get_geojson),
        )
        .route("/api/markers", get(handlers::api::get_markers))
        .route("/api/set_job_type", post(set_job_type))
        .route("/api/set_prefecture", post(set_prefecture))
        .route("/api/set_municipality", post(set_municipality))
        .route(
            "/api/prefectures",
            get(handlers::api::get_prefectures),
        )
        .route(
            "/api/municipalities_cascade",
            get(handlers::api::get_municipalities_cascade),
        )
        .route(
            "/api/industries",
            get(handlers::api::get_industries),
        )
        .route(
            "/api/competitive/filter",
            get(handlers::competitive::comp_filter),
        )
        .route(
            "/api/competitive/municipalities",
            get(handlers::competitive::comp_municipalities),
        )
        .route(
            "/api/competitive/facility_types",
            get(handlers::competitive::comp_facility_types),
        )
        .route(
            "/api/competitive/service_types",
            get(handlers::competitive::comp_service_types),
        )
        .route("/api/report", get(handlers::competitive::comp_report))
        .route(
            "/api/competitive/analysis",
            get(handlers::competitive::comp_analysis),
        )
        .route(
            "/api/competitive/analysis/filter",
            get(handlers::competitive::comp_analysis_filtered),
        )
        .route("/api/status", get(api_status))
        .route_layer(middleware::from_fn_with_state(
            state.clone(),
            auth_middleware,
        ));

    // 静的ファイル配信
    let static_router = Router::new()
        .nest_service("/static", ServeDir::new("static").precompressed_gzip())
        .layer(SetResponseHeaderLayer::if_not_present(
            http::header::CACHE_CONTROL,
            http::HeaderValue::from_static("public, max-age=86400"),
        ));

    Router::new()
        .route("/health", get(health_check))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
        .merge(protected_routes)
        .with_state(state)
        .merge(static_router)
        .layer(
            tower::ServiceBuilder::new()
                .layer(session_layer)
                .layer(CompressionLayer::new())
        )
}

// --- ミドルウェア ---

async fn auth_middleware(
    session: Session,
    State(_state): State<Arc<AppState>>,
    request: axum::extract::Request,
    next: middleware::Next,
) -> axum::response::Response {
    let path = request.uri().path().to_string();
    if path == "/login" || path == "/logout" || path == "/health" || path.starts_with("/static") {
        return next.run(request).await;
    }
    require_auth(session, request, next).await
}

// --- ログイン ---

#[derive(serde::Deserialize)]
struct LoginForm {
    email: String,
    password: String,
}

async fn login_page(State(state): State<Arc<AppState>>) -> Html<String> {
    render_login(&state, None)
}

async fn login_submit(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: axum::extract::Request,
) -> impl IntoResponse {
    let socket_ip = req.extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string());

    let client_ip = req.headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or(socket_ip)
        .unwrap_or_else(|| "unknown".to_string());

    let Form(form) = match axum::extract::Form::<LoginForm>::from_request(req, &()).await {
        Ok(f) => f,
        Err(_) => {
            return render_login(&state, Some("無効なリクエストです".to_string())).into_response();
        }
    };

    if !state.rate_limiter.is_allowed(&client_ip) {
        return render_login(
            &state,
            Some("ログイン試行回数超過。しばらく待ってください。".to_string()),
        )
        .into_response();
    }

    if !validate_email_domain(&form.email, &state.config.allowed_domains) {
        state.rate_limiter.record_failure(&client_ip);
        return render_login(
            &state,
            Some("許可されていないメールドメインです".to_string()),
        )
        .into_response();
    }

    if !verify_password(
        &form.password,
        &state.config.auth_password,
        &state.config.auth_password_hash,
    ) {
        state.rate_limiter.record_failure(&client_ip);
        return render_login(
            &state,
            Some("パスワードが正しくありません".to_string()),
        )
        .into_response();
    }

    state.rate_limiter.record_success(&client_ip);
    let _ = session.insert(SESSION_USER_KEY, &form.email).await;
    // デフォルト産業: 空（全産業）
    let _ = session.insert(SESSION_JOB_TYPE_KEY, "").await;
    let _ = session.insert(SESSION_PREFECTURE_KEY, "").await;
    let _ = session.insert(SESSION_MUNICIPALITY_KEY, "").await;

    Redirect::to("/").into_response()
}

async fn logout(session: Session) -> Redirect {
    session.flush().await.ok();
    Redirect::to("/login")
}

// --- ダッシュボード ---

async fn dashboard_page(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> impl IntoResponse {
    let user_email: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    let current_job_type: String = session
        .get(SESSION_JOB_TYPE_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let current_prefecture: String = session
        .get(SESSION_PREFECTURE_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let current_municipality: String = session
        .get(SESSION_MUNICIPALITY_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    // 都道府県オプション（PREFECTURE_ORDER定数から、DBクエリ不要）
    let pref_options: String = PREFECTURE_ORDER
        .iter()
        .map(|&p| {
            let selected = if p == current_prefecture {
                " selected"
            } else {
                ""
            };
            format!(r#"<option value="{p}"{selected}>{p}</option>"#)
        })
        .collect::<Vec<_>>()
        .join("\n");

    // 市区町村オプション（都道府県選択時のみ）
    let muni_options = if !current_prefecture.is_empty() {
        let muni_list =
            fetch_municipality_list(&state, &current_prefecture).await;
        muni_list
            .iter()
            .map(|m| {
                let selected = if *m == current_municipality {
                    " selected"
                } else {
                    ""
                };
                format!(r#"<option value="{m}"{selected}>{m}</option>"#)
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };

    // 産業オプション（地域で絞り込み、件数付き）
    let industry_list = fetch_industry_list(&state, &current_prefecture, &current_municipality).await;
    let industry_options: String = industry_list
        .iter()
        .map(|(jt, cnt)| {
            let selected = if *jt == current_job_type {
                " selected"
            } else {
                ""
            };
            format!(
                r#"<option value="{jt}"{selected}>{jt} ({cnt})</option>"#,
                jt = jt,
                selected = selected,
                cnt = handlers::overview::format_number(*cnt),
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    let db_warning = if state.hw_db.is_none() {
        r#"<div id="db-warning" class="bg-red-900/80 border border-red-600 text-red-200 px-4 py-3 text-sm flex items-center gap-2">
            <span class="text-lg">⚠️</span>
            <div>
                <strong>データベース接続エラー:</strong> hellowork.db に接続できません。
                <a href="/api/status" target="_blank" class="underline text-red-300 hover:text-white ml-2">詳細ステータス →</a>
            </div>
        </div>"#
            .to_string()
    } else {
        String::new()
    };

    let html = include_str!("../templates/dashboard_inline.html")
        .replace("{{PREF_OPTIONS}}", &pref_options)
        .replace("{{MUNI_OPTIONS}}", &muni_options)
        .replace("{{INDUSTRY_OPTIONS}}", &industry_options)
        .replace("{{USER_EMAIL}}", &user_email)
        .replace("{{TURSO_WARNING}}", &db_warning);

    Html(html)
}

// --- セッション更新API ---

#[derive(serde::Deserialize)]
struct SetJobTypeForm {
    job_type: String,
}

async fn set_job_type(
    State(state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetJobTypeForm>,
) -> impl IntoResponse {
    let _ = session.insert(SESSION_JOB_TYPE_KEY, &form.job_type).await;
    state.cache.clear();
    Html("OK".to_string())
}

#[derive(serde::Deserialize)]
struct SetPrefectureForm {
    prefecture: String,
}

async fn set_prefecture(
    State(state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetPrefectureForm>,
) -> impl IntoResponse {
    let _ = session
        .insert(SESSION_PREFECTURE_KEY, &form.prefecture)
        .await;
    // 都道府県変更時、市区町村をリセット（job_typeはリセットしない）
    let _ = session.insert(SESSION_MUNICIPALITY_KEY, "").await;
    state.cache.clear();
    Html("OK".to_string())
}

#[derive(serde::Deserialize)]
struct SetMunicipalityForm {
    municipality: String,
}

async fn set_municipality(
    State(state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetMunicipalityForm>,
) -> impl IntoResponse {
    let _ = session
        .insert(SESSION_MUNICIPALITY_KEY, &form.municipality)
        .await;
    state.cache.clear();
    Html("OK".to_string())
}

// --- ヘルパー ---

/// 市区町村一覧取得（job_typeフィルタなし）
async fn fetch_municipality_list(
    state: &AppState,
    prefecture: &str,
) -> Vec<String> {
    if let Some(db) = &state.hw_db {
        if let Ok(rows) = db.query(
            "SELECT DISTINCT municipality FROM postings WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != '' ORDER BY municipality",
            &[&prefecture as &dyn rusqlite::types::ToSql],
        ) {
            return rows.iter()
                .filter_map(|r| r.get("municipality").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect();
        }
    }
    Vec::new()
}

/// 産業一覧取得（地域フィルタ付き、件数付き）
async fn fetch_industry_list(
    state: &AppState,
    prefecture: &str,
    municipality: &str,
) -> Vec<(String, i64)> {
    if let Some(db) = &state.hw_db {
        let (loc_filter, loc_params) =
            handlers::overview::build_hw_location_filter(prefecture, municipality, 0);
        let sql = format!(
            "SELECT job_type, COUNT(*) as cnt FROM postings \
             WHERE 1=1{loc_filter} AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC"
        );
        let bind_refs: Vec<&dyn rusqlite::types::ToSql> =
            loc_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
        if let Ok(rows) = db.query(&sql, &bind_refs) {
            return rows
                .iter()
                .filter_map(|r| {
                    let jt = r.get("job_type").and_then(|v| v.as_str()).map(|s| s.to_string())?;
                    let cnt = r
                        .get("cnt")
                        .and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64)))
                        .unwrap_or(0);
                    Some((jt, cnt))
                })
                .collect();
        }
    }
    Vec::new()
}

/// ヘルスチェック
async fn health_check() -> &'static str {
    "OK"
}

/// ステータスAPI
async fn api_status(
    State(state): State<Arc<AppState>>,
) -> axum::response::Json<serde_json::Value> {
    let db_ok = state.hw_db.is_some();
    let db_count = if let Some(db) = &state.hw_db {
        db.query_scalar::<i64>("SELECT COUNT(*) FROM postings", &[])
            .unwrap_or(0)
    } else {
        0
    };

    axum::response::Json(serde_json::json!({
        "hellowork_db_loaded": db_ok,
        "hellowork_db_rows": db_count,
        "status": if db_ok { "healthy" } else { "degraded" }
    }))
}

fn render_login(state: &AppState, error_message: Option<String>) -> Html<String> {
    let domains = state
        .config
        .allowed_domains
        .iter()
        .map(|d| format!("@{d}"))
        .collect::<Vec<_>>()
        .join(", ");

    let error_html = error_message
        .map(|msg| {
            format!(
                r#"<div class="bg-red-900/50 border border-red-700 text-red-300 px-4 py-3 rounded-lg mb-4 text-sm">{msg}</div>"#
            )
        })
        .unwrap_or_default();

    let html = include_str!("../templates/login_inline.html")
        .replace("{{ERROR_HTML}}", &error_html)
        .replace("{{DOMAINS}}", &domains);

    Html(html)
}

// --- ファイル解凍 ---

/// data/geojson_gz/*.json.gz → static/geojson/*.json に解凍
pub fn decompress_geojson_if_needed() {
    use std::path::Path;

    let gz_dir = Path::new("data/geojson_gz");
    let out_dir = Path::new("static/geojson");

    if !gz_dir.exists() {
        tracing::info!("No geojson_gz directory found, skipping GeoJSON decompression");
        return;
    }

    let _ = std::fs::create_dir_all(out_dir);

    let entries = match std::fs::read_dir(gz_dir) {
        Ok(e) => e,
        Err(e) => {
            tracing::warn!("Cannot read geojson_gz dir: {e}");
            return;
        }
    };

    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        let fname = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) if n.ends_with(".json.gz") => n.to_string(),
            _ => continue,
        };
        let json_name = fname.trim_end_matches(".gz");
        let out_path = out_dir.join(json_name);

        if out_path.exists() {
            continue;
        }

        decompress_gz_file(
            path.to_str().unwrap_or_default(),
            out_path.to_str().unwrap_or_default(),
        );
        count += 1;
    }
    if count > 0 {
        tracing::info!("Decompressed {count} GeoJSON files");
    }
}

/// gzip圧縮DBファイルを解凍
pub fn decompress_db_if_needed(db_path: &str) {
    use flate2::read::GzDecoder;
    use std::fs::File;
    use std::io::{self, Read, Write};
    use std::path::Path;

    let db_file = Path::new(db_path);
    if db_file.exists() {
        return;
    }

    let gz_path = format!("{}.gz", db_path);
    let gz_file = Path::new(&gz_path);
    if !gz_file.exists() {
        tracing::info!("No gzip DB found at {gz_path}, skipping decompression");
        return;
    }

    tracing::info!("Decompressing {gz_path} -> {db_path}...");

    match (|| -> io::Result<u64> {
        let f = File::open(&gz_path)?;
        let mut decoder = GzDecoder::new(f);
        let mut out = File::create(db_path)?;
        let mut buf = vec![0u8; 1024 * 1024];
        let mut total: u64 = 0;
        loop {
            let n = decoder.read(&mut buf)?;
            if n == 0 {
                break;
            }
            out.write_all(&buf[..n])?;
            total += n as u64;
        }
        out.flush()?;
        Ok(total)
    })() {
        Ok(bytes) => {
            tracing::info!("Decompressed {} bytes -> {db_path}", bytes);
        }
        Err(e) => {
            tracing::error!("Failed to decompress {gz_path}: {e}");
            let _ = std::fs::remove_file(db_path);
        }
    }
}

fn decompress_gz_file(gz_path: &str, out_path: &str) {
    use flate2::read::GzDecoder;
    use std::fs::File;
    use std::io::{Read, Write};

    let f = match File::open(gz_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("Cannot open {gz_path}: {e}");
            return;
        }
    };
    let mut decoder = GzDecoder::new(f);
    let mut out = match File::create(out_path) {
        Ok(f) => f,
        Err(e) => {
            tracing::warn!("Cannot create {out_path}: {e}");
            return;
        }
    };
    let mut buf = vec![0u8; 1024 * 1024];
    loop {
        match decoder.read(&mut buf) {
            Ok(0) => break,
            Ok(n) => {
                if out.write_all(&buf[..n]).is_err() {
                    let _ = std::fs::remove_file(out_path);
                    return;
                }
            }
            Err(_) => {
                let _ = std::fs::remove_file(out_path);
                return;
            }
        }
    }
    let _ = out.flush();
}
