//! 統合 PDF レポートの契約テスト
//!
//! `feedback_reverse_proof_tests.md` に従い「要素存在」だけでなく
//! 必須セクション・スコープ表記・印刷スタイル・KPI 値の妥当性を検証する。

#![cfg(test)]

use super::render::{integrated_report, IntegratedReportQuery};
use crate::db::cache::AppCache;
use crate::db::local_sqlite::LocalDb;
use crate::handlers::jobmap::company_markers::CompanyGeoEntry;
use crate::{config::AppConfig, AppState};
use axum::extract::{Query, State};
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower_sessions::{MemoryStore, Session};

fn create_test_db() -> (NamedTempFile, LocalDb) {
    let tmp = NamedTempFile::new().unwrap();
    let path: String = tmp.path().to_str().unwrap().to_string();

    let conn = rusqlite::Connection::open(&path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE postings (
            id INTEGER PRIMARY KEY AUTOINCREMENT,
            job_type TEXT,
            industry_raw TEXT,
            prefecture TEXT,
            municipality TEXT,
            facility_name TEXT,
            employment_type TEXT,
            salary_type TEXT,
            salary_min INTEGER,
            salary_max INTEGER,
            annual_holidays INTEGER,
            bonus_months REAL,
            base_salary_min INTEGER,
            base_salary_max INTEGER,
            occupation_major TEXT,
            occupation_middle TEXT,
            recruitment_reason TEXT,
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
        INSERT INTO postings (job_type, industry_raw, prefecture, municipality, facility_name, employment_type, salary_type, salary_min, salary_max, annual_holidays, recruitment_reason)
        VALUES
            ('医療','病院','東京都','千代田区','病院A','正社員','月給',300000,400000,120,'欠員補充'),
            ('医療','病院','東京都','千代田区','病院A','正社員','月給',310000,410000,118,'増員'),
            ('医療','病院','東京都','千代田区','病院B','正社員','月給',320000,420000,125,'欠員補充'),
            ('医療','病院','東京都','千代田区','病院B','パート','時給',NULL,NULL,NULL,NULL),
            ('医療','病院','東京都','千代田区','病院C','契約','月給',280000,350000,110,NULL);
        "#,
    )
    .unwrap();
    drop(conn);
    let db = LocalDb::new(&path).unwrap();
    (tmp, db)
}

fn test_state(db: LocalDb) -> Arc<AppState> {
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
        hw_db: Some(db),
        turso_db: None,
        salesnow_db: None,
        cache: AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
    })
}

async fn empty_session() -> Session {
    let store = MemoryStore::default();
    Session::new(None, Arc::new(store), None)
}

// ========== 1: 必須セクションの存在 ==========

#[tokio::test]
async fn integrated_report_contains_all_required_sections() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // 必須セクション (見出し)
    let required = [
        "Executive Summary",
        "第 1 章 採用診断",
        "第 2 章 地域カルテ",
        "第 3 章 So What 示唆",
        "巻末",
    ];
    for s in required {
        assert!(
            html.contains(s),
            "integrated report must contain section '{}'. Output snippet: {}",
            s,
            crate::text_util::truncate_char_safe(&html, 500)
        );
    }

    // page-break / @media print
    assert!(html.contains("page-break"), "must define page-break class");
    assert!(html.contains("@media print"), "must contain @media print");
    assert!(
        html.contains("@page") && html.contains("A4"),
        "must define @page A4"
    );

    // window.print() ボタン
    assert!(html.contains("window.print()"), "must contain print button");
}

#[tokio::test]
async fn integrated_report_mentions_hw_scope_and_no_causation() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // HW 限定性
    assert!(html.contains("ハローワーク"), "must mention ハローワーク");
    assert!(
        html.contains("民間"),
        "must mention 民間 to clarify exclusion"
    );

    // 因果非主張
    assert!(html.contains("傾向"), "must mention 傾向");
    assert!(
        html.contains("因果関係を主張するものではありません"),
        "must explicitly disclaim causation"
    );
}

// ========== 2: KPI 値の妥当性（具体値検証） ==========

#[tokio::test]
async fn integrated_report_kpi_matches_inserted_data() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // 5 件投入 → posting_count: 5
    assert!(
        html.contains("5 件"),
        "must show 5 件 (posting count) somewhere. snippet: ..."
    );
    // 5 件中 3 件正社員 → 60.0%
    assert!(
        html.contains("60.0"),
        "must show 60.0 (seishain ratio percent)"
    );
}

// ========== 3: 単一 <html> ドキュメントを返す ==========

#[tokio::test]
async fn integrated_report_returns_single_html_document() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery::default();
    let html = integrated_report(State(state), session, Query(q)).await.0;

    let doctype_count = html.matches("<!DOCTYPE html>").count();
    assert_eq!(
        doctype_count, 1,
        "must contain exactly 1 <!DOCTYPE html>, got {}",
        doctype_count
    );

    // <html> 開始タグも 1 個のみ
    let html_open_count = html.matches("<html").count();
    assert_eq!(
        html_open_count, 1,
        "must contain exactly 1 <html> open tag, got {}",
        html_open_count
    );
}

// ========== 4: ロゴ URL 差し替え ==========

#[tokio::test]
async fn integrated_report_accepts_safe_logo_url() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: Some("https://example.com/logo.png".to_string()),
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;
    assert!(
        html.contains(r#"src="https://example.com/logo.png""#),
        "must include safe logo URL via src attr"
    );
}

#[tokio::test]
async fn integrated_report_rejects_dangerous_logo_url() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: Some("javascript:alert(1)".to_string()),
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;
    // javascript: スキームは escape_url_attr で # に書き換えられ、空判定で削除される
    assert!(
        !html.contains("javascript:alert"),
        "javascript: scheme must be sanitized"
    );
    // 既定のロゴプレースホルダにフォールバック
    assert!(
        html.contains("F-A-C 株式会社") || html.contains("cover-logo"),
        "must fall back to default logo placeholder"
    );
}

// ========== 5: DB 未接続時の graceful fallback ==========

#[tokio::test]
async fn integrated_report_no_db_returns_minimal_error_page() {
    // hw_db = None
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
        rate_limiter: crate::auth::session::RateLimiter::new(5, 60),
        company_geo_cache: None::<Vec<CompanyGeoEntry>>,
        audit: None,
    });
    let session = empty_session().await;

    let q = IntegratedReportQuery::default();
    let html = integrated_report(State(state), session, Query(q)).await.0;
    assert!(html.contains("データベース未接続") || html.contains("hellowork.db"));
    // それでも 1 つの <!DOCTYPE> ドキュメントとして返ってくる
    assert_eq!(html.matches("<!DOCTYPE html>").count(), 1);
}

// ========================================================
// Fix-B 追加 (D-2 監査 Q4.4): 表紙スコープ注記の充実検証
// feedback_hw_data_scope.md / feedback_correlation_not_causation.md 準拠
// ========================================================

#[tokio::test]
async fn fixb_cover_has_explicit_hw_only_scope_warning() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // 表紙セクションの抽出（最初の </section> までが cover-page）
    let cover_section = match html.split("class=\"cover-page\"").nth(1) {
        Some(s) => match s.split("</section>").next() {
            Some(c) => c,
            None => panic!("cover-page section の終端が見つからない"),
        },
        None => panic!("cover-page section が見つからない"),
    };

    // 表紙にハローワーク限定スコープが明記されている
    assert!(
        cover_section.contains("ハローワーク"),
        "表紙に「ハローワーク」が明記されているべき (D-2 Q4.4)"
    );
    // 民間求人サイトの除外を明記
    assert!(
        cover_section.contains("民間求人サイト") || cover_section.contains("Indeed"),
        "表紙に民間求人サイト除外の明記が必須"
    );
    // 「全求人市場の代表ではない」旨
    assert!(
        cover_section.contains("全求人市場") || cover_section.contains("HW 限定"),
        "表紙に全求人市場の代表ではない旨が必須"
    );
}

#[tokio::test]
async fn fixb_cover_has_filter_conditions_and_data_date() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // 表紙にデータ取得日 / フィルタ条件 / 対象期間
    assert!(
        html.contains("データ取得日"),
        "表紙にデータ取得日が必須 (D-2 Q4.4)"
    );
    assert!(html.contains("フィルタ条件"), "表紙にフィルタ条件が必須");
    assert!(html.contains("対象期間"), "表紙に対象期間が必須");
    // フィルタ値が反映されていること
    assert!(
        html.contains("東京都") && html.contains("千代田区"),
        "表紙にフィルタ値（pref/muni）が反映されているべき"
    );
}

#[tokio::test]
async fn fixb_each_chapter_has_hw_only_scope_banner() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = IntegratedReportQuery {
        prefecture: Some("東京都".to_string()),
        municipality: Some("千代田区".to_string()),
        logo_url: None,
    };
    let html = integrated_report(State(state), session, Query(q)).await.0;

    // 各章ヘッダー直下に HW 限定スコープバナーが存在
    let banner_count = html.matches("chapter-scope-banner").count();
    assert!(
        banner_count >= 3,
        "各章 (第1-4章) にスコープバナーが必須。検出数: {}",
        banner_count
    );
    // バナー内に必須キーワード
    assert!(
        html.contains("HW 限定スコープ"),
        "章スコープバナーに「HW 限定スコープ」表記が必須"
    );
}
