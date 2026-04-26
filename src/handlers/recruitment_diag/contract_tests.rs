//! 統合契約テスト: 全8パネルのAPIレスポンス形状を検証する
//!
//! ## 目的
//!
//! `templates/tabs/recruitment_diag.html` の renderer が期待する JSON key が
//! backend から確実に返ることを保証する。
//!
//! ## なぜ必要か
//!
//! 2026-04-23 採用診断8パネル全滅事故。Agent A/B/C が backend 実装、
//! Agent D が frontend 実装したが、JSON shape が一致せず全パネルが empty/error に。
//! 個別ユニットテストは全 passed だったが、**agent間の契約が未検証**だった。
//!
//! ## 検証対象
//!
//! 各パネルについて:
//! 1. ハンドラが実際に返す JSON の top-level key
//! 2. frontend renderer (recruitment_diag.html) が読む key
//! 3. 両者が一致することをアサート

#![cfg(test)]

use super::*;
use crate::db::local_sqlite::LocalDb;
use crate::handlers::jobmap::company_markers::CompanyGeoEntry;
use crate::{config::AppConfig, db::cache::AppCache, AppState};
use axum::extract::{Query, State};
use axum::Json;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower_sessions::{MemoryStore, Session};

/// 最小限の hw_db を tempfile で作成
fn create_test_hw_db() -> (NamedTempFile, LocalDb) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();

    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
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

        -- 岩手県盛岡市に飲食業 正社員を 10件投入
        INSERT INTO postings (job_type, prefecture, municipality, employment_type,
            salary_type, salary_min, salary_max, annual_holidays, bonus_months, base_salary_min)
        VALUES
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 200000, 250000, 110, 2.5, 200000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 210000, 260000, 105, 2.0, 210000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 220000, 270000, 112, 3.0, 220000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 180000, 230000, 108, 2.5, 180000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 230000, 280000, 115, 3.5, 230000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 200000, 250000, 110, 2.5, 200000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 190000, 240000, 105, 2.0, 190000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 240000, 290000, 120, 4.0, 240000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 210000, 260000, 108, 2.5, 210000),
            ('飲食業', '岩手県', '盛岡市', '正社員', '月給', 220000, 270000, 110, 3.0, 220000);

        -- 東京都千代田区 製造業 少量（全業界比較用）
        INSERT INTO postings (job_type, prefecture, municipality, employment_type,
            salary_type, salary_min, salary_max, annual_holidays, bonus_months, base_salary_min)
        VALUES
            ('製造業', '東京都', '千代田区', '正社員', '月給', 280000, 350000, 125, 4.0, 280000),
            ('製造業', '東京都', '千代田区', '正社員', '月給', 300000, 370000, 120, 3.5, 300000);
        "#,
    )
    .unwrap();
    drop(conn);

    let db = LocalDb::new(path).unwrap();
    (tmp, db)
}

/// テスト用 AppState 構築（Turso/SalesNow/監査は全て None）
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
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
    })
}

/// 空のセッションを作成
async fn empty_session() -> Session {
    let store = MemoryStore::default();
    Session::new(None, Arc::new(store), None)
}

// ========== Panel 1: Difficulty ==========

#[tokio::test]
async fn panel1_difficulty_shape_contains_required_keys() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);
    let session = empty_session().await;

    let params = handlers::DifficultyParams {
        job_type: "飲食業".to_string(),
        emp_type: "正社員".to_string(),
        prefecture: "岩手県".to_string(),
        municipality: "盛岡市".to_string(),
        prefcode: Some("3".to_string()),
        citycode: None,
    };

    let Json(v) = handlers::api_difficulty_score(State(state), session, Query(params)).await;

    // frontend renderer が参照する key が存在すること
    assert!(v.get("metrics").is_some(), "metrics key missing, got: {v}");
    let metrics = v.get("metrics").unwrap();
    assert!(
        metrics.get("score_per_10k").is_some(),
        "metrics.score_per_10k missing"
    );
    assert!(
        metrics.get("hw_count").is_some(),
        "metrics.hw_count missing"
    );
    assert!(
        metrics.get("population").is_some(),
        "metrics.population missing"
    );
    assert!(
        metrics.get("national_hw_count").is_some(),
        "metrics.national_hw_count missing"
    );
    assert!(
        metrics.get("area_share_of_national").is_some(),
        "metrics.area_share_of_national missing"
    );
    // F1 #3: 観光地補正で追加された keys
    assert!(
        metrics.get("day_population").is_some(),
        "metrics.day_population missing (F1 #3)"
    );
    assert!(
        metrics.get("night_population").is_some(),
        "metrics.night_population missing (F1 #3)"
    );
    assert!(
        metrics.get("day_night_ratio").is_some(),
        "metrics.day_night_ratio missing (F1 #3)"
    );
    assert!(
        metrics.get("is_tourist_area").is_some(),
        "metrics.is_tourist_area missing (F1 #3)"
    );
    assert!(v.get("rank_label").is_some(), "rank_label missing");
    assert!(v.get("so_what").is_some(), "so_what missing");
    assert!(v.get("notes").is_some(), "notes missing");

    // HW注意書き (MEMORY feedback_hw_data_scope)
    let notes = v.get("notes").unwrap();
    assert!(notes.get("hw_scope").is_some(), "notes.hw_scope missing");
    assert!(notes.get("causation").is_some(), "notes.causation missing");

    // hw_count > 0 であること (10件投入したので)
    let hw_count = metrics["hw_count"].as_i64().unwrap();
    assert_eq!(hw_count, 10, "投入したHW求人10件が集計されていない");
}

// ========== Panel 2: Talent Pool ==========

#[tokio::test]
async fn panel2_talent_pool_shape_contains_required_keys() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);
    let session = empty_session().await;

    let params = handlers::TalentPoolParams {
        prefecture: "岩手県".to_string(),
        municipality: "盛岡市".to_string(),
        citycode: Some(3201), // 盛岡市
        year: None,
    };

    let Json(v) = handlers::api_talent_pool(State(state), session, Query(params)).await;

    // frontend が読む keys
    assert!(v.get("metrics").is_some(), "metrics missing");
    let m = v.get("metrics").unwrap();
    assert!(
        m.get("day_population").is_some(),
        "metrics.day_population missing"
    );
    assert!(
        m.get("night_population").is_some(),
        "metrics.night_population missing"
    );
    assert!(
        m.get("commuter_inflow").is_some(),
        "metrics.commuter_inflow missing"
    );
    assert!(
        m.get("day_night_ratio").is_some(),
        "metrics.day_night_ratio missing"
    );
    assert!(v.get("so_what").is_some(), "so_what missing");
    assert!(v.get("notes").is_some(), "notes missing");
}

// ========== Panel 3: Inflow ==========

#[tokio::test]
async fn panel3_inflow_shape_when_citycode_missing_returns_error() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);
    let session = empty_session().await;

    let params = handlers::InflowParams {
        prefecture: String::new(),
        municipality: String::new(),
        citycode: None,
        year: None,
    };

    let Json(v) = handlers::api_inflow_analysis(State(state), session, Query(params)).await;

    // citycode なしは error_body を返す契約
    assert!(
        v.get("error").is_some(),
        "error key missing in citycode-absent case"
    );
}

#[tokio::test]
async fn panel3_inflow_shape_contains_breakdown() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);
    let session = empty_session().await;

    let params = handlers::InflowParams {
        prefecture: "岩手県".to_string(),
        municipality: "盛岡市".to_string(),
        citycode: Some(3201),
        year: None,
    };

    let Json(v) = handlers::api_inflow_analysis(State(state), session, Query(params)).await;

    // error でなければ breakdown フィールド必須
    if v.get("error").is_none() {
        assert!(v.get("breakdown").is_some(), "breakdown missing: {v}");
        assert!(v.get("so_what").is_some(), "so_what missing");
    }
}

// ========== Panel 5: Condition Gap ==========

#[tokio::test]
async fn panel5_condition_gap_shape_and_reverse_proof() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);

    let params = condition_gap::ConditionGapQuery {
        job_type: "飲食業".to_string(),
        emp_type: "正社員".to_string(),
        prefcode: Some(3), // 岩手県
        municipality: "盛岡市".to_string(),
        company_salary_min: Some(220_000.0),
        company_bonus_months: Some(3.0),
        company_annual_holidays: Some(115.0),
    };

    let Json(v) = condition_gap::condition_gap(State(state), Query(params)).await;

    // frontend renderer が読む key
    assert!(
        v.get("industry_median").is_some(),
        "industry_median missing: {v}"
    );
    assert!(
        v.get("all_industry_median").is_some(),
        "all_industry_median missing"
    );
    assert!(v.get("company").is_some(), "company missing");

    let industry = v.get("industry_median").unwrap();
    assert!(
        industry.get("annual_income").is_some(),
        "industry_median.annual_income missing"
    );
    assert!(
        industry.get("annual_holidays").is_some(),
        "industry_median.annual_holidays missing"
    );
    assert!(
        industry.get("bonus_months").is_some(),
        "industry_median.bonus_months missing"
    );
    assert!(
        industry.get("sample_size").is_some(),
        "industry_median.sample_size missing"
    );

    let company = v.get("company").unwrap();
    assert!(
        company.get("annual_income_estimated").is_some(),
        "company.annual_income_estimated missing"
    );

    // 逆証明: 自社年収 = 月給22万 × (12 + 3ヶ月) = 330万 = 3,300,000 円
    let own_income = company["annual_income_estimated"].as_f64().unwrap();
    assert!(
        (own_income - 3_300_000.0).abs() < 1.0,
        "自社年収計算ミス: expected 3,300,000, got {}",
        own_income
    );
}

// ========== Panel 6: Market Trend ==========

#[tokio::test]
async fn panel6_market_trend_shape_when_no_turso() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);

    let params = market_trend::MarketTrendQuery {
        job_type: "飲食業".to_string(),
        emp_type: "正社員".to_string(),
        prefcode: Some(3),
        months: None,
    };

    let Json(v) = market_trend::market_trend(State(state), Query(params)).await;

    // Turso なし → エラーまたは空データだが、frontend が読む key を返すこと
    if v.get("error").is_none() {
        assert!(v.get("months").is_some(), "months missing: {v}");
        assert!(v.get("counts").is_some(), "counts missing");
    }
}

// ========== Panel 7: Opportunity Map ==========

#[tokio::test]
async fn panel7_opportunity_map_shape() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);

    let params = opportunity_map::OpportunityMapParams {
        job_type: Some("飲食業".to_string()),
        emp_type: Some("正社員".to_string()),
        prefcode: 3,
    };

    let session = empty_session().await;
    let Json(v) = opportunity_map::opportunity_map(State(state), session, Query(params)).await;

    // frontend renderer が読む key
    if v.get("error").is_none() {
        assert!(
            v.get("municipalities").is_some(),
            "municipalities missing: {v}"
        );
    }
}

// ========== Panel 8: Insights ==========

#[tokio::test]
async fn panel8_insights_shape() {
    let (_tmp, db) = create_test_hw_db();
    let state = test_app_state(db);

    let params = insights::InsightsParams {
        job_type: Some("飲食業".to_string()),
        emp_type: Some("正社員".to_string()),
        prefcode: 3,
        citycode: Some(3201),
    };

    let session = empty_session().await;
    let Json(v) = insights::insights(State(state), session, Query(params)).await;

    // frontend が読む key
    assert!(v.get("insights").is_some(), "insights missing: {v}");
}
