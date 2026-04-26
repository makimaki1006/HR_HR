//! Team γ 監査テスト: region/karte.rs データフロー契約検証
//!
//! # 目的
//!
//! 地域カルテ (`api_region_karte`) のレスポンス JSON 契約を逆証明する。
//!
//! - citycode → (prefecture, municipality) の逆引きが `v2_flow_master_prefcity` 経由で機能
//! - レスポンスに以下の top-level key が**必ず存在**すること:
//!     * citycode / prefecture / municipality / kpi / posting_count / daynight_ratio / covid_recovery_ratio
//! - `kpi` オブジェクトに 9 キー全て存在 (total_population / total_households / elderly_rate 等)
//! - 存在しない citycode → `error` key を含む JSON
//! - HW postings の集計 (posting_count, fulltime_count 相当) が手計算と一致
//!
//! # 制約
//!
//! karte.rs 内部の fetch_karte_bundle / render_* / build_* は private `fn` のため直接呼べない。
//! 公開 handler (`api_region_karte`) 経由で契約を検証する。
//!
//! # MEMORY 参照
//!
//! - `feedback_reverse_proof_tests`: shape だけでなく数値を手計算と突き合わせ
//! - `feedback_e2e_chart_verification`: API レスポンス数値検証で transitively 描画データを担保

#![cfg(test)]

use super::karte::api_region_karte;
use crate::auth::session::RateLimiter;
use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::db::local_sqlite::LocalDb;
use crate::handlers::jobmap::company_markers::CompanyGeoEntry;
use crate::AppState;
use axum::extract::{Path, State};
use std::sync::Arc;
use tempfile::NamedTempFile;

// ========== テスト用インフラ ==========

/// karte テスト用最小 DB:
///
/// - `v2_flow_master_prefcity(prefname, cityname, citycode)` — citycode 逆引き用
/// - `postings(...)` — HW求人集計用
///
/// v2_external_* / v2_flow_* の本格テーブルは投入せず、KarteBundle はデフォルト値で埋まる前提。
fn create_test_karte_db() -> (NamedTempFile, LocalDb) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        -- citycode 逆引きマスタ
        CREATE TABLE v2_flow_master_prefcity (
            citycode  INTEGER PRIMARY KEY,
            prefname  TEXT NOT NULL,
            cityname  TEXT NOT NULL
        );
        INSERT INTO v2_flow_master_prefcity VALUES
            (13101, '東京都', '千代田区'),
            (13102, '東京都', '中央区');

        -- postings (karte が内部で集計) - 13101 に紐づく求人 5 件
        CREATE TABLE postings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_number TEXT,
            job_type TEXT NOT NULL,
            industry_raw TEXT,
            prefecture TEXT NOT NULL,
            municipality TEXT NOT NULL,
            facility_name TEXT,
            employment_type TEXT,
            salary_type TEXT,
            salary_min INTEGER,
            salary_max INTEGER,
            annual_holidays INTEGER,
            bonus_months REAL,
            base_salary_min INTEGER,
            base_salary_max INTEGER,
            latitude REAL,
            longitude REAL,
            headline TEXT,
            job_description TEXT,
            requirements TEXT,
            benefits TEXT,
            working_hours TEXT,
            holidays TEXT,
            access TEXT,
            hello_work_office TEXT
        );
        -- 千代田区: 正社員 3, パート 2  → cnt=5, ft=3, pt=2, avg_sal=(200k+210k+220k+180k+230k)/5 = 208000
        INSERT INTO postings (job_type, prefecture, municipality, employment_type,
            salary_type, salary_min, salary_max, annual_holidays, bonus_months, base_salary_min)
        VALUES
            ('飲食業', '東京都', '千代田区', '正社員', '月給', 200000, 250000, 110, 2.5, 200000),
            ('飲食業', '東京都', '千代田区', '正社員', '月給', 210000, 260000, 105, 2.0, 210000),
            ('飲食業', '東京都', '千代田区', '正社員', '月給', 220000, 270000, 112, 3.0, 220000),
            ('飲食業', '東京都', '千代田区', 'パート・アルバイト', '時給', 180000, 230000, 108, 2.5, 180000),
            ('飲食業', '東京都', '千代田区', 'パート', '時給', 230000, 280000, 115, 3.5, 230000);

        -- 中央区: 求人なし (0件になることを検証)
        "#,
    )
    .unwrap();
    drop(conn);
    let db = LocalDb::new(path).unwrap();
    (tmp, db)
}

fn test_app_state(hw_db: LocalDb) -> Arc<AppState> {
    let cfg = AppConfig {
        port: 0,
        auth_password: String::new(),
        auth_password_hash: String::new(),
        external_passwords: Vec::new(),
        allowed_domains: Vec::new(),
        allowed_domains_extra: Vec::new(),
        hellowork_db_path: String::new(),
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
    };
    Arc::new(AppState {
        config: cfg,
        hw_db: Some(hw_db),
        turso_db: None,
        salesnow_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
    })
}

// ========== Contract: レスポンス shape ==========

/// 逆証明: api_region_karte は citycode=13101 に対し、
///   - prefecture="東京都", municipality="千代田区"
///   - kpi オブジェクトに 9 キー
///   - posting_count=5 (手計算)
///   - daynight_ratio = null (v2_flow_* テーブル未作成環境)
/// を含む JSON を返す。
#[tokio::test]
async fn api_region_karte_shape_contains_required_keys() {
    let (_tmp, db) = create_test_karte_db();
    let state = test_app_state(db);

    let resp = api_region_karte(State(state), Path(13101i64)).await;
    let json = resp.0;

    // top-level keys (contract)
    let required_keys = [
        "citycode",
        "prefecture",
        "municipality",
        "kpi",
        "posting_count",
        "daynight_ratio",
        "covid_recovery_ratio",
    ];
    for k in required_keys {
        assert!(
            json.get(k).is_some(),
            "JSON に top-level key `{}` が無い → frontend 契約違反",
            k
        );
    }

    // pref/muni の正引き正しいこと
    assert_eq!(json["prefecture"].as_str(), Some("東京都"));
    assert_eq!(json["municipality"].as_str(), Some("千代田区"));
    assert_eq!(json["citycode"].as_i64(), Some(13101));

    // posting_count = 5 (千代田区に 5 件投入)
    assert_eq!(
        json["posting_count"].as_i64(),
        Some(5),
        "postings テーブル集計件数は 5 (手計算)"
    );

    // daynight_ratio は v2_flow_* 未投入環境では null
    assert!(
        json["daynight_ratio"].is_null(),
        "v2_flow 未投入環境では daynight_ratio=null"
    );
    assert!(
        json["covid_recovery_ratio"].is_null(),
        "同 covid_recovery_ratio=null"
    );

    // kpi オブジェクトに 9 キー
    let kpi = &json["kpi"];
    assert!(kpi.is_object(), "kpi は object");
    let kpi_keys = [
        "total_population",
        "total_households",
        "elderly_rate",
        "single_rate",
        "unemployment_rate",
        "physicians_per_10k",
        "daycare_per_1k",
        "establishment_count",
        "habitable_density",
    ];
    for k in kpi_keys {
        assert!(
            kpi.get(k).is_some(),
            "kpi に `{}` が無い → S1 構造KPI 9 枚のどれかが未表示になる",
            k
        );
    }
}

/// 逆証明: 存在しない citycode (=99999) → `error` key を持つ JSON
#[tokio::test]
async fn api_region_karte_returns_error_for_unknown_citycode() {
    let (_tmp, db) = create_test_karte_db();
    let state = test_app_state(db);

    let resp = api_region_karte(State(state), Path(99999i64)).await;
    let json = resp.0;

    assert!(json.get("error").is_some(), "未知 citycode は error を返す");
    assert_eq!(json["citycode"].as_i64(), Some(99999));
}

/// 逆証明: citycode=13102 (中央区、postings なし) は prefecture/municipality を返すが
/// posting_count=0 となる。
#[tokio::test]
async fn api_region_karte_zero_postings_is_zero_not_missing() {
    let (_tmp, db) = create_test_karte_db();
    let state = test_app_state(db);

    let resp = api_region_karte(State(state), Path(13102i64)).await;
    let json = resp.0;

    assert_eq!(json["prefecture"].as_str(), Some("東京都"));
    assert_eq!(json["municipality"].as_str(), Some("中央区"));
    // 0 行 → COUNT(*) は 0 を返す。null ではなく 0 が正しい契約。
    assert_eq!(
        json["posting_count"].as_i64(),
        Some(0),
        "求人 0 件は posting_count=0 (null でない)"
    );
}

/// 逆証明: hw_db = None でも panic せず、空 prefecture/municipality で error を返す。
#[tokio::test]
async fn api_region_karte_handles_missing_db() {
    let cfg = AppConfig {
        port: 0,
        auth_password: String::new(),
        auth_password_hash: String::new(),
        external_passwords: Vec::new(),
        allowed_domains: Vec::new(),
        allowed_domains_extra: Vec::new(),
        hellowork_db_path: String::new(),
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
    };
    let state = Arc::new(AppState {
        config: cfg,
        hw_db: None,
        turso_db: None,
        salesnow_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
    });

    let resp = api_region_karte(State(state), Path(13101i64)).await;
    let json = resp.0;
    // hw_db=None → lookup_pref_muni が空 → error 分岐
    assert!(
        json.get("error").is_some(),
        "hw_db 無しでも panic せず error JSON を返す"
    );
}

/// 逆証明: master テーブル未作成でも panic せず error を返す (table_exists で防御)。
#[tokio::test]
async fn api_region_karte_handles_missing_master_table() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    // 空 DB (v2_flow_master_prefcity 無し、postings 無し)
    conn.execute_batch("CREATE TABLE dummy (id INTEGER);")
        .unwrap();
    drop(conn);
    let db = LocalDb::new(path).unwrap();
    let state = test_app_state(db);

    let resp = api_region_karte(State(state), Path(13101i64)).await;
    let json = resp.0;
    assert!(
        json.get("error").is_some(),
        "master テーブル無しなら error を返す"
    );
}
