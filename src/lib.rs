pub mod auth;
pub mod config;
pub mod db;
pub mod geo;
pub mod handlers;
pub mod models;

use axum::{
    extract::{DefaultBodyLimit, Form, FromRequest, State},
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
    require_auth, validate_email_domain, verify_password_with_externals, SESSION_INDUSTRY_RAWS_KEY,
    SESSION_JOB_TYPES_KEY, SESSION_JOB_TYPE_KEY, SESSION_MUNICIPALITY_KEY, SESSION_PREFECTURE_KEY,
    SESSION_USER_KEY,
};
use config::AppConfig;
use db::cache::AppCache;
use models::job_seeker::PREFECTURE_ORDER;

/// アップロード上限（ボディサイズ）: 20MB
/// - CSVは通常 数MB 以下。20MBで常用範囲を大幅にカバーしつつ、
///   意図しない巨大アップロード(50MB/100MB 等)は 413 で即拒否。
/// - 将来、allowlisted なルートで 100MB 等へ拡張するため定数で定義。
pub const UPLOAD_BODY_LIMIT_BYTES: usize = 20 * 1024 * 1024;

/// アプリケーション共有状態
pub struct AppState {
    pub config: AppConfig,
    pub hw_db: Option<db::local_sqlite::LocalDb>,
    pub turso_db: Option<db::turso_http::TursoDb>,
    pub salesnow_db: Option<db::turso_http::TursoDb>,
    pub cache: AppCache,
    pub rate_limiter: auth::session::RateLimiter,
    /// SalesNow企業の座標キャッシュ（起動時にTursoからロード）
    pub company_geo_cache: Option<Vec<handlers::jobmap::company_markers::CompanyGeoEntry>>,
}

/// アプリケーションRouter構築
pub fn build_app(state: Arc<AppState>) -> Router {
    let session_store = MemoryStore::default();
    let session_layer = SessionManagerLayer::new(session_store)
        .with_secure(false)
        .with_expiry(Expiry::OnInactivity(time::Duration::hours(24)));

    let protected_routes = Router::new()
        .route("/", get(dashboard_page))
        .route("/tab/market", get(handlers::market::tab_market))
        .route(
            "/api/market/population",
            get(handlers::market::market_population),
        )
        .route(
            "/api/market/workstyle",
            get(handlers::market::market_workstyle),
        )
        .route("/api/market/balance", get(handlers::market::market_balance))
        .route(
            "/api/market/demographics",
            get(handlers::market::market_demographics),
        )
        .route("/tab/overview", get(handlers::overview::tab_overview))
        .route(
            "/tab/demographics",
            get(handlers::demographics::tab_demographics),
        )
        .route("/tab/balance", get(handlers::balance::tab_balance))
        .route("/tab/workstyle", get(handlers::workstyle::tab_workstyle))
        .route("/tab/analysis", get(handlers::analysis::tab_analysis))
        .route(
            "/api/analysis/subtab/{id}",
            get(handlers::analysis::analysis_subtab),
        )
        .route("/tab/diagnostic", get(handlers::diagnostic::tab_diagnostic))
        .route(
            "/api/diagnostic/evaluate",
            get(handlers::diagnostic::evaluate_diagnostic),
        )
        .route(
            "/api/diagnostic/reset",
            get(handlers::diagnostic::reset_diagnostic),
        )
        .route("/tab/jobmap", get(handlers::jobmap::tab_jobmap))
        .route("/api/jobmap/markers", get(handlers::jobmap::jobmap_markers))
        .route(
            "/api/jobmap/detail/{id}",
            get(handlers::jobmap::jobmap_detail),
        )
        .route(
            "/api/jobmap/detail-json/{id}",
            get(handlers::jobmap::jobmap_detail_json),
        )
        .route("/api/jobmap/stats", post(handlers::jobmap::jobmap_stats))
        .route(
            "/api/jobmap/municipalities",
            get(handlers::jobmap::jobmap_municipalities),
        )
        .route("/api/jobmap/seekers", get(handlers::jobmap::jobmap_seekers))
        .route(
            "/api/jobmap/seeker-detail",
            get(handlers::jobmap::jobmap_seeker_detail),
        )
        .route(
            "/api/jobmap/choropleth",
            get(handlers::jobmap::jobmap_choropleth),
        )
        .route(
            "/api/jobmap/region/summary",
            get(handlers::jobmap::region_summary),
        )
        .route(
            "/api/jobmap/region/age_gender",
            get(handlers::jobmap::region_age_gender),
        )
        .route(
            "/api/jobmap/region/posting_stats",
            get(handlers::jobmap::region_posting_stats),
        )
        .route(
            "/api/jobmap/region/segments",
            get(handlers::jobmap::region_segments),
        )
        .route(
            "/api/jobmap/company-markers",
            get(handlers::jobmap::jobmap_company_markers),
        )
        .route(
            "/api/jobmap/labor-flow",
            get(handlers::jobmap::jobmap_labor_flow),
        )
        .route(
            "/api/jobmap/industry-companies",
            get(handlers::jobmap::jobmap_industry_companies),
        )
        .route(
            "/tab/competitive",
            get(handlers::competitive::tab_competitive),
        )
        .route("/tab/trend", get(handlers::trend::tab_trend))
        .route("/api/trend/subtab/{id}", get(handlers::trend::trend_subtab))
        .route("/tab/insight", get(handlers::insight::tab_insight))
        .route(
            "/api/insight/subtab/{id}",
            get(handlers::insight::insight_subtab),
        )
        .route(
            "/api/insight/widget/{tab}",
            get(handlers::insight::insight_widget),
        )
        .route(
            "/api/insight/report",
            get(handlers::insight::insight_report_json),
        )
        .route(
            "/api/insight/report/xlsx",
            get(handlers::insight::insight_report_xlsx),
        )
        .route(
            "/report/insight",
            get(handlers::insight::insight_report_html),
        )
        .route("/tab/survey", get(handlers::survey::tab_survey))
        .route(
            "/api/survey/upload",
            post(handlers::survey::upload_csv)
                // 20MB超のアップロードは 413 Payload Too Large で即拒否。
                // Render無料プランのタイムアウト(502)より前にアプリ層で明示拒否する。
                .layer(DefaultBodyLimit::max(UPLOAD_BODY_LIMIT_BYTES)),
        )
        .route("/api/survey/analyze", get(handlers::survey::analyze_survey))
        .route(
            "/api/survey/integrate",
            get(handlers::survey::integrate_report),
        )
        .route("/api/survey/report", get(handlers::survey::report_json))
        .route("/report/survey", get(handlers::survey::survey_report_html))
        .route("/tab/company", get(handlers::company::tab_company))
        .route(
            "/api/company/search",
            get(handlers::company::company_search),
        )
        .route(
            "/api/company/profile/{corporate_number}",
            get(handlers::company::company_profile),
        )
        .route("/api/company/bulk-csv", get(handlers::company::bulk_csv))
        .route(
            "/report/company/{corporate_number}",
            get(handlers::company::company_report),
        )
        .route("/tab/guide", get(handlers::guide::tab_guide))
        .route("/api/geojson/{filename}", get(handlers::api::get_geojson))
        .route("/api/markers", get(handlers::api::get_markers))
        .route("/api/set_job_type", post(set_job_type))
        .route("/api/set_prefecture", post(set_prefecture))
        .route("/api/set_municipality", post(set_municipality))
        .route("/api/prefectures", get(handlers::api::get_prefectures))
        .route(
            "/api/municipalities_cascade",
            get(handlers::api::get_municipalities_cascade),
        )
        .route("/api/industries", get(handlers::api::get_industries))
        .route("/api/industry_tree", get(handlers::api::get_industry_tree))
        .route("/api/set_industry_filter", post(set_industry_filter))
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
            http::HeaderValue::from_static("public, max-age=604800, immutable"),
        ));

    // JSON REST API v1（認証不要 - MCP/AI連携用）
    let api_v1 = Router::new()
        .route("/api/v1/companies", get(handlers::api_v1::search_companies))
        .route(
            "/api/v1/companies/{corporate_number}",
            get(handlers::api_v1::company_profile),
        )
        .route(
            "/api/v1/companies/{corporate_number}/nearby",
            get(handlers::api_v1::nearby_companies),
        )
        .route(
            "/api/v1/companies/{corporate_number}/postings",
            get(handlers::api_v1::company_postings),
        );

    Router::new()
        .route("/health", get(health_check))
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
        .merge(api_v1)
        .merge(protected_routes)
        .with_state(state)
        .merge(static_router)
        .layer(
            tower::ServiceBuilder::new()
                .layer(session_layer)
                .layer(CompressionLayer::new()),
        )
}

// --- ミドルウェア ---

/// 許可されたOriginのリスト（本番ドメインとローカル開発）
const ALLOWED_ORIGINS: &[&str] = &[
    "https://hr-hw.onrender.com",
    "http://localhost:3000",
    "http://localhost:8080",
    "http://127.0.0.1:3000",
    "http://127.0.0.1:8080",
];

/// CSRF保護: POSTリクエストに対してOrigin/Refererヘッダーを検証
fn check_csrf(request: &axum::extract::Request) -> Result<(), &'static str> {
    // GET/HEAD/OPTIONSは安全メソッドなのでスキップ
    let method = request.method();
    if method == axum::http::Method::GET
        || method == axum::http::Method::HEAD
        || method == axum::http::Method::OPTIONS
    {
        return Ok(());
    }

    let headers = request.headers();

    // Originヘッダー優先、なければRefererフォールバック
    let origin = headers
        .get("origin")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let referer_origin = headers
        .get("referer")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| {
            // refererから origin部分（scheme://host[:port]）を手動抽出
            // 例: "https://hr-hw.onrender.com/tab/market" → "https://hr-hw.onrender.com"
            let after_scheme = if let Some(pos) = s.find("://") {
                let scheme = &s[..pos];
                let rest = &s[pos + 3..];
                let host_end = rest.find('/').unwrap_or(rest.len());
                Some(format!("{}://{}", scheme, &rest[..host_end]))
            } else {
                None
            };
            after_scheme
        });

    let check_origin = origin.as_deref().or(referer_origin.as_deref());

    match check_origin {
        Some(o) if ALLOWED_ORIGINS.contains(&o) => Ok(()),
        Some(o) => {
            // 明示的に別オリジンを指定された場合のみ拒否（ブラウザからのCSRF攻撃対策）
            tracing::warn!("CSRF: rejected origin/referer: {}", o);
            Err("CSRF: invalid origin")
        }
        None => {
            // Origin/Referer無し = curl/API client/モバイルアプリ等からのリクエスト
            // ブラウザからのsame-originは Origin ヘッダーが付くため、これが無い場合は
            // スクリプト経由アクセスとみなして通す。認証はAuth middlewareで別途チェック済み。
            Ok(())
        }
    }
}

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

    // CSRF保護: 書き込み系リクエストに対してOrigin/Referer検証
    if let Err(msg) = check_csrf(&request) {
        return (
            axum::http::StatusCode::FORBIDDEN,
            format!("Forbidden: {}", msg),
        )
            .into_response();
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
    let socket_ip = req
        .extensions()
        .get::<axum::extract::ConnectInfo<std::net::SocketAddr>>()
        .map(|ci| ci.0.ip().to_string());

    let client_ip = req
        .headers()
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.split(',').next())
        .map(|s| s.trim().to_string())
        .or(socket_ip)
        .unwrap_or_else(|| "unknown".to_string());

    let req_ua = req
        .headers()
        .get("user-agent")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown")
        .to_string();

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

    // ドメインチェック: 社内ドメイン + 外部追加ドメインの両方を許可
    let all_domains: Vec<String> = state
        .config
        .allowed_domains
        .iter()
        .chain(state.config.allowed_domains_extra.iter())
        .cloned()
        .collect();
    if !validate_email_domain(&form.email, &all_domains) {
        state.rate_limiter.record_failure(&client_ip);
        return render_login(
            &state,
            Some("許可されていないメールドメインです".to_string()),
        )
        .into_response();
    }

    // パスワードチェック: 社内（無期限） + 外部（有効期限付き）
    tracing::info!(
        "Login attempt: domain={}, external_count={}",
        form.email.split('@').nth(1).unwrap_or("?"),
        state.config.external_passwords.len(),
    );
    let (pw_ok, expired_msg) = verify_password_with_externals(
        &form.password,
        &state.config.auth_password,
        &state.config.auth_password_hash,
        &state.config.external_passwords,
    );
    if !pw_ok {
        tracing::warn!(
            "LOGIN_FAILED: email={}, ip={}, reason={}",
            form.email,
            client_ip,
            expired_msg.as_deref().unwrap_or("wrong_password"),
        );
        state.rate_limiter.record_failure(&client_ip);
        let msg = expired_msg.unwrap_or_else(|| "パスワードが正しくありません".to_string());
        return render_login(&state, Some(msg)).into_response();
    }

    state.rate_limiter.record_success(&client_ip);
    tracing::info!(
        "LOGIN_SUCCESS: email={}, ip={}, user_agent={}",
        form.email,
        client_ip,
        req_ua,
    );
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

async fn dashboard_page(State(state): State<Arc<AppState>>, session: Session) -> impl IntoResponse {
    let user_email: String = session
        .get(SESSION_USER_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "unknown".to_string());

    // current_job_type は産業ツリードロップダウン移行により不要（JS側で動的ロード）

    // 複数選択フィルタ（JSON配列文字列）
    let selected_job_types_json: String = session
        .get::<String>(SESSION_JOB_TYPES_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".to_string());
    let selected_industry_raws_json: String = session
        .get::<String>(SESSION_INDUSTRY_RAWS_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| "[]".to_string());

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
        let muni_list = fetch_municipality_list(&state, &current_prefecture).await;
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
        .replace("{{SELECTED_JOB_TYPES_JSON}}", &selected_job_types_json)
        .replace(
            "{{SELECTED_INDUSTRY_RAWS_JSON}}",
            &selected_industry_raws_json,
        )
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
    State(_state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetJobTypeForm>,
) -> impl IntoResponse {
    let _ = session.insert(SESSION_JOB_TYPE_KEY, &form.job_type).await;
    // キャッシュキーにフィルタ条件が含まれるため、フィルタ変更時は
    // 自動的にキャッシュミスとなる。古いエントリはTTLで自然失効。
    // cache.clear() は他ユーザーのキャッシュまで破棄してしまうため削除。
    Html("OK".to_string())
}

#[derive(serde::Deserialize)]
struct SetIndustryFilterForm {
    #[serde(default)]
    job_types: String,
    #[serde(default)]
    industry_raws: String,
}

async fn set_industry_filter(
    State(_state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetIndustryFilterForm>,
) -> impl IntoResponse {
    // カンマ区切り → JSON配列に変換してセッション保存
    let jt_list: Vec<String> = form
        .job_types
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    let ir_list: Vec<String> = form
        .industry_raws
        .split(',')
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();

    let jt_json = serde_json::to_string(&jt_list).unwrap_or_else(|_| "[]".to_string());
    let ir_json = serde_json::to_string(&ir_list).unwrap_or_else(|_| "[]".to_string());

    let _ = session.insert(SESSION_JOB_TYPES_KEY, &jt_json).await;
    let _ = session.insert(SESSION_INDUSTRY_RAWS_KEY, &ir_json).await;
    // 旧キーをクリア（後方互換のフォールバック用）
    let _ = session.insert(SESSION_JOB_TYPE_KEY, "").await;
    Html("OK".to_string())
}

#[derive(serde::Deserialize)]
struct SetPrefectureForm {
    prefecture: String,
}

async fn set_prefecture(
    State(_state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetPrefectureForm>,
) -> impl IntoResponse {
    let _ = session
        .insert(SESSION_PREFECTURE_KEY, &form.prefecture)
        .await;
    // 都道府県変更時、市区町村をリセット（job_typeはリセットしない）
    let _ = session.insert(SESSION_MUNICIPALITY_KEY, "").await;
    Html("OK".to_string())
}

#[derive(serde::Deserialize)]
struct SetMunicipalityForm {
    municipality: String,
}

async fn set_municipality(
    State(_state): State<Arc<AppState>>,
    session: Session,
    Form(form): Form<SetMunicipalityForm>,
) -> impl IntoResponse {
    let _ = session
        .insert(SESSION_MUNICIPALITY_KEY, &form.municipality)
        .await;
    Html("OK".to_string())
}

// --- ヘルパー ---

/// 市区町村一覧取得（job_typeフィルタなし）
async fn fetch_municipality_list(state: &AppState, prefecture: &str) -> Vec<String> {
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Vec::new(),
    };
    let pref = prefecture.to_string();
    tokio::task::spawn_blocking(move || {
        if let Ok(rows) = db.query(
            "SELECT DISTINCT municipality FROM postings WHERE prefecture = ?1 AND municipality IS NOT NULL AND municipality != '' ORDER BY municipality",
            &[&pref as &dyn rusqlite::types::ToSql],
        ) {
            rows.iter()
                .filter_map(|r| r.get("municipality").and_then(|v| v.as_str()).map(|s| s.to_string()))
                .collect()
        } else {
            Vec::new()
        }
    }).await.unwrap_or_default()
}

/// ヘルスチェック（DB接続+キャッシュ状態をJSON返却）
async fn health_check(
    State(state): State<Arc<AppState>>,
) -> axum::response::Json<serde_json::Value> {
    let db_ok = state.hw_db.is_some();
    let db_rows = if let Some(db) = &state.hw_db {
        let db = db.clone();
        tokio::task::spawn_blocking(move || {
            db.query_scalar::<i64>("SELECT COUNT(*) FROM postings", &[])
                .unwrap_or(-1)
        })
        .await
        .unwrap_or(-1)
    } else {
        -1
    };
    axum::response::Json(serde_json::json!({
        "status": if db_ok { "healthy" } else { "degraded" },
        "db_connected": db_ok,
        "db_rows": db_rows,
        "cache_entries": state.cache.len(),
    }))
}

/// ステータスAPI
async fn api_status(State(state): State<Arc<AppState>>) -> axum::response::Json<serde_json::Value> {
    let db_ok = state.hw_db.is_some();
    let db_count = if let Some(db) = &state.hw_db {
        let db = db.clone();
        tokio::task::spawn_blocking(move || {
            db.query_scalar::<i64>("SELECT COUNT(*) FROM postings", &[])
                .unwrap_or(0)
        })
        .await
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

    let guide_html = handlers::guide::build_guide_html();

    let html = include_str!("../templates/login_inline.html")
        .replace("{{ERROR_HTML}}", &error_html)
        .replace("{{DOMAINS}}", &domains)
        .replace("{{GUIDE_HTML}}", &guide_html);

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
    let gz_path = format!("{}.gz", db_path);
    let gz_file = Path::new(&gz_path);

    // gzが存在する場合は常にgzから再解凍（DB更新を確実に反映）
    if db_file.exists() && gz_file.exists() {
        tracing::info!("Both {db_path} and {gz_path} exist, removing old DB to re-decompress");
        let _ = std::fs::remove_file(db_path);
    }

    if db_file.exists() {
        return;
    }

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

/// GeoJSON事前圧縮: static/geojson/*.json → *.json.gz
/// precompressed_gzip() (ServeDir) が .gz を自動配信する
pub fn precompress_geojson() {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use std::path::Path;

    let geojson_dir = Path::new("static/geojson");
    if !geojson_dir.exists() {
        return;
    }

    let entries = match std::fs::read_dir(geojson_dir) {
        Ok(e) => e,
        Err(_) => return,
    };

    let mut count = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let gz_path_str = format!("{}.gz", path.display());
        let gz_path = Path::new(&gz_path_str);
        if gz_path.exists() {
            continue;
        }
        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };
        let gz_file = match std::fs::File::create(&gz_path_str) {
            Ok(f) => f,
            Err(_) => continue,
        };
        let mut encoder = GzEncoder::new(gz_file, Compression::best());
        if encoder.write_all(&data).is_ok() && encoder.finish().is_ok() {
            count += 1;
        }
    }
    if count > 0 {
        tracing::info!("Pre-compressed {count} GeoJSON files to .gz");
    }
}
