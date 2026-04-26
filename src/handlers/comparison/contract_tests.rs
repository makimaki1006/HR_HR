//! 47 県横断比較の契約テスト
//!
//! `feedback_reverse_proof_tests.md` に従い「要素存在」ではなく「データ妥当性」を検証する。
//! - 47 件必ず返却（postings に 1 件もない県も 0 件で含まれる）
//! - PREFECTURE_ORDER 順
//! - sort=desc が実際に降順になっている
//! - 集計値が SQL の素朴な結果と一致

#![cfg(test)]

use super::fetch::{fetch_all_prefecture_kpi, ComparisonMetric, PrefectureKpi};
use super::render::tab_comparison;
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
            prefecture TEXT,
            municipality TEXT,
            facility_name TEXT,
            employment_type TEXT,
            salary_type TEXT,
            salary_min INTEGER,
            salary_max INTEGER
        );
        -- 東京都: 5 件、内 4 件正社員、3 件給与開示、2 事業所
        INSERT INTO postings (job_type, prefecture, municipality, facility_name, employment_type, salary_type, salary_min)
        VALUES
            ('医療','東京都','千代田区','病院A','正社員','月給',300000),
            ('医療','東京都','千代田区','病院A','正社員','月給',310000),
            ('医療','東京都','港区','病院B','正社員','月給',320000),
            ('医療','東京都','港区','病院B','正社員',NULL,NULL),
            ('医療','東京都','港区','病院B','パート',NULL,NULL);
        -- 北海道: 2 件、内 1 件正社員、2 件給与開示、2 事業所
        INSERT INTO postings (job_type, prefecture, municipality, facility_name, employment_type, salary_type, salary_min)
        VALUES
            ('医療','北海道','札幌市','病院C','正社員','月給',250000),
            ('医療','北海道','旭川市','病院D','契約','月給',230000);
        -- 大阪府: 1 件、給与未開示
        INSERT INTO postings (job_type, prefecture, municipality, facility_name, employment_type, salary_type, salary_min)
        VALUES
            ('医療','大阪府','大阪市','病院E','正社員','月給',NULL);
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

// ========== 1: 47 件必ず返る ==========

#[test]
fn fetch_returns_exactly_47_prefectures() {
    let (_tmp, db) = create_test_db();
    let kpi = fetch_all_prefecture_kpi(&db, &[]);
    assert_eq!(kpi.len(), 47, "must return all 47 prefectures, got {}", kpi.len());
}

#[test]
fn fetch_returns_prefectures_in_jis_order() {
    let (_tmp, db) = create_test_db();
    let kpi = fetch_all_prefecture_kpi(&db, &[]);
    // PREFECTURE_ORDER 順
    use crate::models::job_seeker::PREFECTURE_ORDER;
    for (i, expected) in PREFECTURE_ORDER.iter().enumerate() {
        assert_eq!(
            kpi[i].prefecture, *expected,
            "position {} expected {} got {}",
            i, expected, kpi[i].prefecture
        );
    }
}

// ========== 2: 集計値の妥当性（具体値検証）==========

#[test]
fn tokyo_aggregates_match_inserted_data() {
    let (_tmp, db) = create_test_db();
    let kpi = fetch_all_prefecture_kpi(&db, &[]);
    let tokyo = kpi.iter().find(|k| k.prefecture == "東京都").unwrap();

    // 5 件投入
    assert_eq!(tokyo.posting_count, 5, "Tokyo posting_count");
    // 4 件正社員 → 4/5 = 0.8
    assert!(
        (tokyo.seishain_ratio - 0.8).abs() < 0.001,
        "Tokyo seishain_ratio = {}, expected 0.8",
        tokyo.seishain_ratio
    );
    // 給与開示は salary_min > 0 の 3 件 → 3/5 = 0.6
    assert!(
        (tokyo.salary_disclosure_rate - 0.6).abs() < 0.001,
        "Tokyo salary_disclosure_rate = {}, expected 0.6",
        tokyo.salary_disclosure_rate
    );
    // 月給平均 = (300000+310000+320000)/3 = 310000
    assert!(
        (tokyo.salary_min_avg - 310000.0).abs() < 1.0,
        "Tokyo salary_min_avg = {}, expected 310000",
        tokyo.salary_min_avg
    );
    // 事業所数 = 2 (病院A, 病院B)
    assert_eq!(tokyo.facility_count, 2, "Tokyo facility_count");
}

#[test]
fn empty_prefecture_returns_zero_values() {
    let (_tmp, db) = create_test_db();
    let kpi = fetch_all_prefecture_kpi(&db, &[]);
    // 沖縄県は postings に 1 件もないので 0 件
    let okinawa = kpi.iter().find(|k| k.prefecture == "沖縄県").unwrap();
    assert_eq!(okinawa.posting_count, 0, "Okinawa must be 0");
    assert_eq!(okinawa.facility_count, 0);
    assert!(okinawa.salary_min_avg == 0.0);
    assert!(okinawa.seishain_ratio == 0.0);
}

#[test]
fn industry_filter_excludes_non_matching_records() {
    let (_tmp, db) = create_test_db();
    // 「建設業」フィルタ → 投入データに建設業はないので全 0 件
    let kpi = fetch_all_prefecture_kpi(&db, &["建設業".to_string()]);
    assert_eq!(kpi.len(), 47);
    let total: i64 = kpi.iter().map(|k| k.posting_count).sum();
    assert_eq!(total, 0, "construction filter should match 0 records");
}

// ========== 3: ハンドラの contract（GET /tab/comparison）==========

#[tokio::test]
async fn tab_comparison_returns_47_table_rows() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = super::render::ComparisonQuery {
        metric: Some("posting_count".to_string()),
        sort: Some("desc".to_string()),
    };
    let html = tab_comparison(State(state), session, Query(q)).await;
    let s = html.0;

    // 47 件のデータ行（思考: <tr ... カルテへ ... 47 個）
    let row_count = s.matches("カルテへ</button>").count();
    assert_eq!(
        row_count, 47,
        "comparison table must have exactly 47 rows, got {}",
        row_count
    );

    // HW 限定性の注記
    assert!(
        s.contains("ハローワーク") && s.contains("民間"),
        "must mention HW scope and exclude民間"
    );
    // 因果非主張の注記
    assert!(
        s.contains("傾向") && s.contains("因果"),
        "must mention 傾向 / 因果"
    );

    // 47 県すべての名前が出力されている
    use crate::models::job_seeker::PREFECTURE_ORDER;
    for p in PREFECTURE_ORDER.iter() {
        assert!(s.contains(p), "must contain prefecture {}", p);
    }

    // 東京都の posting_count = 5 が表示されている
    // 単純な部分一致で「5 件」が東京都の付近にあることだけ確認
    assert!(s.contains("5 件"), "must show 5 件 (Tokyo posting count)");

    // 北海道の正社員比率 1/2 = 50.0%
    assert!(s.contains("50.0"), "must show 50.0% somewhere (Hokkaido seishain_ratio)");
}

#[tokio::test]
async fn tab_comparison_sort_desc_actually_sorts_descending() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = super::render::ComparisonQuery {
        metric: Some("posting_count".to_string()),
        sort: Some("desc".to_string()),
    };
    let html = tab_comparison(State(state), session, Query(q)).await.0;

    // 最初の 100 文字程度に「東京都」が、最後の方に「沖縄県」あたりが来ているはず
    // (東京都 5件、北海道 2件、大阪府 1件、その他 0件)
    let tokyo_pos = html.find(r#">東京都<"#).unwrap_or(usize::MAX);
    let hokkaido_pos = html.find(r#">北海道<"#).unwrap_or(usize::MAX);
    let osaka_pos = html.find(r#">大阪府<"#).unwrap_or(usize::MAX);

    // テーブルの行は metric 値の降順で並ぶはず
    // ECharts 設定にも入るので、テーブル本体での出現位置を比較するため tbody 以降だけ調べる
    let tbody_start = html.find("<tbody>").unwrap_or(0);
    let table_part = &html[tbody_start..];

    let t_pos = table_part.find(r#">東京都<"#).unwrap_or(usize::MAX);
    let h_pos = table_part.find(r#">北海道<"#).unwrap_or(usize::MAX);
    let o_pos = table_part.find(r#">大阪府<"#).unwrap_or(usize::MAX);

    assert!(
        t_pos < h_pos,
        "東京都 (5件) must come before 北海道 (2件) in desc sort. \
         html positions tokyo={} hokkaido={} osaka={}",
        tokyo_pos, hokkaido_pos, osaka_pos
    );
    assert!(
        h_pos < o_pos,
        "北海道 (2件) must come before 大阪府 (1件) in desc sort"
    );
}

#[tokio::test]
async fn tab_comparison_sort_asc_inverts_order() {
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = super::render::ComparisonQuery {
        metric: Some("posting_count".to_string()),
        sort: Some("asc".to_string()),
    };
    let html = tab_comparison(State(state), session, Query(q)).await.0;

    let tbody_start = html.find("<tbody>").unwrap_or(0);
    let table_part = &html[tbody_start..];
    let t_pos = table_part.find(r#">東京都<"#).unwrap_or(usize::MAX);
    let o_pos = table_part.find(r#">大阪府<"#).unwrap_or(usize::MAX);

    // 昇順なら 0件県が上に、東京都(5件) が後ろに
    assert!(o_pos < t_pos, "ascending sort: 大阪府 must come before 東京都");
}

#[tokio::test]
async fn tab_comparison_unknown_metric_falls_back_to_posting_count() {
    // 不正な metric（XSS的な攻撃的文字列）でもクラッシュせず posting_count にフォールバック
    let (_tmp, db) = create_test_db();
    let state = test_state(db);
    let session = empty_session().await;

    let q = super::render::ComparisonQuery {
        metric: Some("<script>alert(1)</script>".to_string()),
        sort: None,
    };
    let html = tab_comparison(State(state), session, Query(q)).await.0;
    // <script> がエスケープされず実行可能な形では出ていないこと
    assert!(!html.contains("<script>alert(1)</script>"));
    // 47 行表示は維持
    assert_eq!(html.matches("カルテへ</button>").count(), 47);
}

// ========== 4: ComparisonMetric ロジック ==========

#[test]
fn metric_format_value_matches_unit() {
    let kpi = PrefectureKpi {
        prefecture: "テスト県".to_string(),
        posting_count: 12345,
        salary_min_avg: 250000.0,
        seishain_ratio: 0.78,
        facility_count: 100,
        salary_disclosure_rate: 0.5,
    };
    assert_eq!(ComparisonMetric::PostingCount.format_value(&kpi), "12,345");
    assert_eq!(ComparisonMetric::SalaryMinAvg.format_value(&kpi), "250,000");
    assert_eq!(ComparisonMetric::SeishainRatio.format_value(&kpi), "78.0");
    assert_eq!(ComparisonMetric::FacilityCount.format_value(&kpi), "100");
    assert_eq!(
        ComparisonMetric::SalaryDisclosureRate.format_value(&kpi),
        "50.0"
    );
}
