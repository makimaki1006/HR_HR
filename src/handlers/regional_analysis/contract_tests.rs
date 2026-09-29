//! 地域分析 JSON API (`/api/app/regional/*`) の契約テスト (React 移行 W4, 2026-09-29)。
//!
//! - tempfile の SQLite (hw_db フォールバック経路) に外部統計の行を入れ、具体値で検証する。
//! - 同じ fixture で旧 partial (`/api/regional/*`) の HTML も取り、JSON の表示値が
//!   旧 HTML に出ていることを確かめる (旧新一致をサーバ側で先に固定する)。


use std::sync::Arc;

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use axum::routing::get;
use axum::Router;
use serde_json::Value;
use tempfile::NamedTempFile;
use tower::ServiceExt;

use crate::db::cache::AppCache;
use crate::db::local_sqlite::LocalDb;
use crate::handlers::jobmap::company_markers::CompanyGeoEntry;
use crate::{config::AppConfig, AppState};

fn create_db() -> (NamedTempFile, LocalDb) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap().to_string();
    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE municipality_code_master (prefecture TEXT, municipality_name TEXT);
        INSERT INTO municipality_code_master VALUES
            ('東京都','新宿区'),('東京都','港区'),('東京都','新宿区'),('北海道','札幌市');
        CREATE TABLE v2_external_job_openings_ratio (prefecture TEXT, fiscal_year TEXT, ratio_total REAL);
        INSERT INTO v2_external_job_openings_ratio VALUES
            ('東京都','2022',1.456),('東京都','2021',1.234),('北海道','2022',0.9);
        CREATE TABLE v2_external_labor_stats (
            prefecture TEXT, fiscal_year TEXT, unemployment_rate REAL, separation_rate REAL,
            monthly_salary_male REAL, monthly_salary_female REAL,
            working_hours_male REAL, working_hours_female REAL,
            part_time_wage_male REAL, part_time_wage_female REAL);
        INSERT INTO v2_external_labor_stats VALUES
            ('東京都','2021',9.9,9.9,1,1,1,1,1,1),
            ('東京都','2022',2.567,14.2,390.0,287.3,165.0,NULL,2084.4,1350.6);
        CREATE TABLE v2_external_industry_structure (
            prefecture_code TEXT, city_name TEXT, industry_code TEXT, industry_name TEXT, employees_total INTEGER);
        INSERT INTO v2_external_industry_structure VALUES
            ('13','新宿区','P','医療,福祉',6000),
            ('13','港区','P','医療,福祉',6345),
            ('13','新宿区','I','卸売業<小売>',6789),
            ('13','新宿区','AS','集計不能',99999);
        CREATE TABLE v2_external_population_pyramid (prefecture TEXT, municipality TEXT, age_group TEXT, male_count INTEGER, female_count INTEGER);
        INSERT INTO v2_external_population_pyramid VALUES
            ('東京都','新宿区','85歳以上',100,250),
            ('東京都','新宿区','0〜4歳',500,480),
            ('東京都','港区','0〜4歳',50,40);
        CREATE TABLE v2_external_minimum_wage (prefecture TEXT, hourly_min_wage REAL);
        INSERT INTO v2_external_minimum_wage VALUES ('東京都',1163.0);
        CREATE TABLE v2_external_foreign_residents (prefecture TEXT, visa_status TEXT, count INTEGER, survey_period TEXT);
        INSERT INTO v2_external_foreign_residents VALUES
            ('東京都','永住者',3000,'2023年末'),('東京都','技能実習',1000,'2023年末'),('東京都','総数',4000,'2023年末');
        CREATE TABLE v2_external_internet_usage (prefecture TEXT, internet_usage_rate REAL, smartphone_ownership_rate REAL, year INTEGER);
        INSERT INTO v2_external_internet_usage VALUES ('東京都',88.26,NULL,2023);
        CREATE TABLE municipality_occupation_population (
            prefecture TEXT, municipality_name TEXT, occupation_name TEXT, population INTEGER, data_label TEXT, basis TEXT);
        INSERT INTO municipality_occupation_population VALUES
            ('東京都','新宿区','事務従事者',3000,'measured','workplace'),
            ('東京都','港区','販売従事者',1000,'measured','workplace'),
            ('東京都','港区','販売従事者',777,'estimated','workplace');
        "#,
    )
    .unwrap();
    drop(conn);
    let db = LocalDb::new(&path).unwrap();
    (tmp, db)
}

fn state_with(db: Option<LocalDb>) -> Arc<AppState> {
    let cfg = AppConfig {
        port: 0,
        auth_password: String::new(),
        auth_password_hash: String::new(),
        external_passwords: Vec::new(),
        allowed_domains: Vec::new(),
        allowed_domains_extra: Vec::new(),
        hellowork_db_path: String::new(),
        indeed_db_path: String::new(),
        cache_ttl_secs: 60,
        cache_max_entries: 10,
        rate_limit_max_attempts: 5,
        rate_limit_lockout_secs: 60,
        audit_turso_url: String::new(),
        audit_turso_token: String::new(),
        audit_ip_salt: String::new(),
        admin_emails: Vec::new(),
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    };
    Arc::new(AppState {
        config: cfg,
        hw_db: db,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
        google_oidc: None,
    })
}

/// JSON ルート + 旧 partial ルートを持つテスト用ルータ (session 層つき)。
fn app(state: Arc<AppState>) -> Router {
    use super::handlers as h;
    let store = tower_sessions::MemoryStore::default();
    Router::new()
        .merge(super::api::router())
        .route(
            "/api/regional/job_openings_ratio",
            get(h::regional_job_openings_ratio),
        )
        .route("/api/regional/labor_stats", get(h::regional_labor_stats))
        .route(
            "/api/regional/industry_structure",
            get(h::regional_industry_structure),
        )
        .route(
            "/api/regional/population_pyramid",
            get(h::regional_population_pyramid),
        )
        .route(
            "/api/regional/wage_comparison",
            get(h::regional_wage_comparison),
        )
        .route(
            "/api/regional/foreign_residents",
            get(h::regional_foreign_residents),
        )
        .route(
            "/api/regional/internet_usage",
            get(h::regional_internet_usage),
        )
        .route("/api/regional/occupation", get(h::regional_occupation))
        .with_state(state)
        .layer(tower_sessions::SessionManagerLayer::new(store))
}

async fn get_text(app: &Router, uri: &str) -> (StatusCode, String) {
    let res = app
        .clone()
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap();
    let status = res.status();
    let body = to_bytes(res.into_body(), 4 * 1024 * 1024).await.unwrap();
    (status, String::from_utf8(body.to_vec()).unwrap())
}

async fn get_json(app: &Router, uri: &str) -> Value {
    let (status, text) = get_text(app, uri).await;
    assert_eq!(status, StatusCode::OK, "{uri}: {text}");
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{uri}: JSON でない ({e}): {text}"))
}

const TOKYO: &str = "prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD";
const SHINJUKU: &str =
    "prefecture=%E6%9D%B1%E4%BA%AC%E9%83%BD&municipality=%E6%96%B0%E5%AE%BF%E5%8C%BA";

#[tokio::test]
async fn init_and_municipalities_return_distinct_sorted_names() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    let v = get_json(&app, "/api/app/regional/init").await;
    assert_eq!(v["prefectures"], serde_json::json!(["北海道", "東京都"]));
    let v = get_json(&app, &format!("/api/app/regional/municipalities?{TOKYO}")).await;
    assert_eq!(v["prefecture"], "東京都");
    assert_eq!(v["municipalities"], serde_json::json!(["新宿区", "港区"]));
    let v = get_json(&app, "/api/app/regional/municipalities").await;
    assert_eq!(v["municipalities"], serde_json::json!([]));
}

#[tokio::test]
async fn job_openings_ratio_points_are_rounded_and_in_year_order() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    let v = get_json(
        &app,
        &format!("/api/app/regional/job_openings_ratio?{TOKYO}"),
    )
    .await;
    assert_eq!(v["status"], "ok");
    assert_eq!(v["title"], "有効求人倍率 推移");
    assert_eq!(v["scope_label"], "東京都");
    assert_eq!(v["message"], Value::Null);
    let pts = v["data"]["points"].as_array().unwrap();
    assert_eq!(pts.len(), 2);
    assert_eq!(pts[0]["fiscal_year"], 2021);
    assert_eq!(pts[0]["label"], "2021年度");
    assert_eq!(pts[0]["ratio"], 1.23);
    assert_eq!(pts[1]["ratio"], 1.46);
    assert_eq!(v["data"]["reference_line"], 1.0);
    assert!(v["note"].as_str().unwrap().contains("00450091"));

    // 旧 partial のチャートにも同じ値が載っている
    let (_, html) = get_text(&app, &format!("/api/regional/job_openings_ratio?{TOKYO}")).await;
    assert!(html.contains(r#""data": [1.23,1.46]"#), "{html}");
    assert!(html.contains(r#"["2021年度","2022年度"]"#), "{html}");
}

#[tokio::test]
async fn labor_stats_cards_match_old_partial_text() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    let v = get_json(&app, &format!("/api/app/regional/labor_stats?{TOKYO}")).await;
    assert_eq!(v["status"], "ok");
    assert_eq!(v["data"]["fiscal_year"], 2022);
    let cards = v["data"]["cards"].as_array().unwrap();
    let displays: Vec<&str> = cards
        .iter()
        .map(|c| c["display"].as_str().unwrap())
        .collect();
    assert_eq!(
        displays,
        vec![
            "2.57%",
            "14.20%",
            "39.0万円",
            "28.7万円",
            "165.00h",
            "-",
            "2,084円/時",
            "1,351円/時"
        ]
    );
    assert_eq!(cards[2]["value"], 390.0);
    assert_eq!(cards[5]["value"], Value::Null);

    let (_, html) = get_text(&app, &format!("/api/regional/labor_stats?{TOKYO}")).await;
    for c in cards {
        let label = c["label"].as_str().unwrap();
        let disp = c["display"].as_str().unwrap();
        let needle = format!(r#"<div class="text-xs text-slate-400">{label}</div><div class="#);
        let at = html
            .find(&needle)
            .unwrap_or_else(|| panic!("{label} が旧 HTML に無い"));
        let rest = &html[at..];
        let close = rest.find("</div></div>").unwrap();
        assert!(
            rest[..close].ends_with(&format!(">{disp}")),
            "{label}: {disp}"
        );
    }
}

#[tokio::test]
async fn industry_structure_shares_exclude_uncountable_codes() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    // 都道府県集計: 医療,福祉 = 6000+6345, 卸売業 = 6789、AS は除外
    let v = get_json(
        &app,
        &format!("/api/app/regional/industry_structure?{TOKYO}"),
    )
    .await;
    assert_eq!(v["status"], "ok");
    assert_eq!(v["data"]["granularity"], "都道府県");
    let t = &v["data"]["table"];
    assert_eq!(t["total"], 19134);
    assert_eq!(t["total_display"], "19,134");
    assert_eq!(t["rows"][0]["label"], "医療,福祉");
    assert_eq!(t["rows"][0]["value"], 12345);
    assert_eq!(t["rows"][0]["share_display"], "64.5%");
    assert_eq!(t["rows"][1]["label"], "卸売業<小売>");
    assert_eq!(t["rows"][1]["share_display"], "35.5%");

    let (_, html) = get_text(&app, &format!("/api/regional/industry_structure?{TOKYO}")).await;
    assert!(html.contains(
        r#"<tr><td>医療,福祉</td><td class="text-right">12,345</td><td class="text-right">64.5%</td></tr>"#
    ));
    // JSON は生の文字列、旧 HTML はエスケープ済み
    assert!(html.contains("卸売業&lt;小売&gt;"));

    // 市区町村指定: 新宿区のみ
    let v = get_json(
        &app,
        &format!("/api/app/regional/industry_structure?{SHINJUKU}"),
    )
    .await;
    assert_eq!(v["scope_label"], "東京都 新宿区");
    assert_eq!(v["data"]["granularity"], "市区町村");
    assert_eq!(v["data"]["table"]["total"], 12789);
}

#[tokio::test]
async fn pyramid_bands_are_sorted_young_first() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    let v = get_json(
        &app,
        &format!("/api/app/regional/population_pyramid?{SHINJUKU}"),
    )
    .await;
    assert_eq!(v["status"], "ok");
    let bands = v["data"]["bands"].as_array().unwrap();
    assert_eq!(bands[0]["age_group"], "0〜4歳");
    assert_eq!(bands[0]["male"], 500);
    assert_eq!(bands[1]["age_group"], "85歳以上");
    assert_eq!(bands[1]["female"], 250);
    assert!(v["note"]
        .as_str()
        .unwrap()
        .contains("求人・求職の人数ではありません"));
}

#[tokio::test]
async fn wage_foreign_internet_occupation_values() {
    let (_t, db) = create_db();
    let app = app(state_with(Some(db)));
    let v = get_json(&app, &format!("/api/app/regional/wage_comparison?{TOKYO}")).await;
    assert_eq!(v["data"]["card"]["display"], "1,163円/時");
    let (_, html) = get_text(&app, &format!("/api/regional/wage_comparison?{TOKYO}")).await;
    assert!(html.contains(">1,163円/時</div>"));

    let v = get_json(
        &app,
        &format!("/api/app/regional/foreign_residents?{TOKYO}"),
    )
    .await;
    let t = &v["data"]["table"];
    assert_eq!(t["total"], 4000, "総数の行は除外される");
    assert_eq!(t["rows"][0]["label"], "永住者");
    assert_eq!(t["rows"][0]["share_display"], "75.0%");
    assert_eq!(t["chart_limit"], 12);
    assert_eq!(v["data"]["survey_period"], "2023年末");

    let v = get_json(&app, &format!("/api/app/regional/internet_usage?{TOKYO}")).await;
    assert_eq!(
        v["data"]["cards"][0]["label"],
        "インターネット利用率 2023年"
    );
    assert_eq!(v["data"]["cards"][0]["display"], "88.3%");
    assert_eq!(v["data"]["cards"][1]["display"], "-");

    let v = get_json(&app, &format!("/api/app/regional/occupation?{TOKYO}")).await;
    assert_eq!(
        v["data"]["table"]["total"], 4000,
        "estimated の行は除外される"
    );
    let v = get_json(&app, &format!("/api/app/regional/occupation?{SHINJUKU}")).await;
    assert_eq!(v["data"]["table"]["rows"][0]["label"], "事務従事者");
    assert_eq!(v["data"]["granularity"], "市区町村");
}

#[tokio::test]
async fn status_branches_match_old_partials() {
    let (_t, db) = create_db();
    let app1 = app(state_with(Some(db)));
    // 都道府県未選択
    let v = get_json(&app1, "/api/app/regional/labor_stats").await;
    assert_eq!(v["status"], "pref_required");
    assert_eq!(v["message"], "都道府県を選択してください。");
    assert_eq!(v["data"], Value::Null);
    assert_eq!(v["scope_label"], "未選択");
    // データなし (北海道に労働統計は無い)
    let v = get_json(
        &app1,
        "/api/app/regional/labor_stats?prefecture=%E5%8C%97%E6%B5%B7%E9%81%93",
    )
    .await;
    assert_eq!(v["status"], "no_data");
    assert_eq!(
        v["message"],
        "労働統計 に該当するデータがありません。条件を変更してください。"
    );
    assert!(v["note"].as_str().unwrap().starts_with("出典: e-Stat"));
    // 外部企業データ未接続 (salesnow_db = None)
    let v = get_json(&app1, &format!("/api/app/regional/company_matrix?{TOKYO}")).await;
    assert_eq!(v["status"], "company_data_unavailable");
    assert_eq!(v["message"], "外部企業データに接続できません。");

    // DB 未接続
    let app2 = app(state_with(None));
    let v = get_json(&app2, &format!("/api/app/regional/occupation?{TOKYO}")).await;
    assert_eq!(v["status"], "db_unavailable");
    assert_eq!(v["message"], "外部統計データベースに接続できません。");
    let (_, html) = get_text(&app2, &format!("/api/regional/occupation?{TOKYO}")).await;
    assert!(html.contains("外部統計データベースに接続できません。"));
}

#[test]
fn company_matrix_json_keeps_all_points_and_table_limit() {
    let pts = super::golden_tests::companies();
    let p = super::api::build_company_matrix(&super::golden_tests::pref(), &pts);
    let d = p.data.unwrap();
    assert_eq!(d.points.len(), 22);
    assert_eq!(d.table_limit, 20);
    assert_eq!(d.points[0].company_name, "会社0&Co");
    assert_eq!(d.points[0].growth_rate_display, "-3.3");
    assert_eq!(d.points[0].growth_rate_chart, -3.3);
    assert_eq!(
        d.count_note,
        "対象企業数: 22社 (従業員数の多い順、上限あり)。表は上位20社。"
    );
    assert!(p.note.contains("因果関係を示すものではありません"));
}
