//! Team δ 監査: 全タブ Frontend⇔Backend JSON 契約 L5 逆証明
//!
//! # 目的
//!
//! 2026-04-23 の採用診断 8 パネル全滅事故を受け、採用診断以外の全タブでも
//! 類似の契約ミスマッチが潜んでいないか網羅監査する。採用診断自身は既存
//! `src/handlers/recruitment_diag/contract_tests.rs` で担保済み。
//!
//! # 配置
//!
//! 本ファイルは `src/handlers/global_contract_audit_test.rs` に置き、
//! `handlers/mod.rs` に `#[cfg(test)] mod global_contract_audit_test;` として
//! 1 行追加することで test-only コンパイルに組み込む。
//! （既存 Team γ の `jobmap/flow_audit_test.rs` と同じパターン）
//!
//! # 対象 endpoint（jobmap タブ）
//!
//! 最もミスマッチリスクが高い jobmap タブの主要 JSON endpoint を tempfile
//! hw_db + minimal AppState で実際に呼出し、frontend renderer が参照する
//! top-level key が backend レスポンスに存在することを逆証明する。
//!
//! | Endpoint | Handler | 検証内容 |
//! |----------|---------|---------|
//! | `/api/jobmap/heatmap` | `jobmap::heatmap::jobmap_heatmap` | `points, data_count, max, meta, truncated, row_limit` + 数値一致 |
//! | `/api/jobmap/inflow`  | `jobmap::inflow::jobmap_inflow_sankey` | `sankey.{nodes,links}, summary, total_population, data_warning` or `error` |
//! | `/api/jobmap/correlation` | `jobmap::correlation::jobmap_correlation` | `correlation.{r,n,note}, outliers.{hiring_hard,underserved}, points` or `error` |
//! | `/api/jobmap/labor-flow` | `jobmap::company_markers::labor_flow` | error path: `error, industries, prefecture` |
//!
//! # 既知ミスマッチの記録テスト（`#[ignore]` 付き）
//!
//! docs/contract_audit_2026_04_23.md で検出した下記 2 件を FAILED テストとして
//! 残す。修正完了後に `#[ignore]` を外す運用。
//!
//! - `bug_marker_labor_flow_returns_municipality_key`: Mismatch #4
//! - `observation_center_format_inconsistency_across_jobmap_endpoints`: Mismatch #5
//!
//! # MEMORY 遵守
//! - `feedback_reverse_proof_tests.md`: 要素存在ではなく具体値（max=1200.0 等）で検証
//! - `feedback_agent_contract_verification.md`: agent 並列実装時の契約検証

#![cfg(test)]

use crate::config::AppConfig;
use crate::db::cache::AppCache;
use crate::db::local_sqlite::LocalDb;
use crate::handlers::jobmap::{
    company_markers::{self, CompanyGeoEntry, LaborFlowParams},
    correlation::{self, CorrelationParams},
    heatmap::{self, HeatmapParams},
    inflow::{self, InflowParams},
};
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use std::sync::Arc;
use tempfile::NamedTempFile;
use tower_sessions::{MemoryStore, Session};

// ========== テスト用インフラ ==========

/// jobmap 逆証明に最低限必要な Agoop テーブルを持つ tempfile SQLite を生成。
///
/// - `v2_flow_attribute_mesh1km`: 3 メッシュ（東京駅近辺想定、citycode=13101）
/// - `v2_flow_mesh1km_2021`: 3 メッシュ × 月07 × dayflag/timezone 生値4 象限
/// - `v2_flow_fromto_city`: 着地 13101 への 4 区分流入
/// - `v2_posting_mesh1km`: メッシュ別求人件数（相関用、最新 snapshot）
/// - `v2_flow_master_prefcity`: citycode → prefname/cityname マスタ
fn make_tmp_hw_db() -> (NamedTempFile, LocalDb) {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path().to_str().unwrap();
    let conn = rusqlite::Connection::open(path).unwrap();
    conn.execute_batch(
        r#"
        CREATE TABLE v2_flow_attribute_mesh1km (
            mesh1kmid     INTEGER PRIMARY KEY,
            center_lat    REAL NOT NULL,
            center_lng    REAL NOT NULL,
            bbox_min_lat  REAL NOT NULL,
            bbox_min_lng  REAL NOT NULL,
            bbox_max_lat  REAL NOT NULL,
            bbox_max_lng  REAL NOT NULL,
            prefcode      INTEGER NOT NULL,
            citycode      INTEGER NOT NULL
        );
        INSERT INTO v2_flow_attribute_mesh1km VALUES
            (10000, 35.68, 139.76, 35.67, 139.75, 35.69, 139.77, 13, 13101),
            (10001, 35.69, 139.77, 35.68, 139.76, 35.70, 139.78, 13, 13101),
            (10002, 35.70, 139.78, 35.69, 139.77, 35.71, 139.79, 13, 13101);

        CREATE TABLE v2_flow_mesh1km_2021 (
            mesh1kmid  INTEGER NOT NULL,
            month      INTEGER NOT NULL,
            dayflag    INTEGER NOT NULL,
            timezone   INTEGER NOT NULL,
            prefcode   INTEGER NOT NULL,
            citycode   INTEGER NOT NULL,
            population REAL NOT NULL,
            PRIMARY KEY (mesh1kmid, month, dayflag, timezone)
        );
        -- 生値 (dayflag∈{0,1}, timezone∈{0,1}) のみ投入
        -- ヒートマップは Raw モード限定のため集計値 2 は投入しない
        INSERT INTO v2_flow_mesh1km_2021 VALUES
            (10000, 7, 1, 0, 13, 13101,  500.0),
            (10000, 7, 0, 0, 13, 13101,  300.0),
            (10000, 7, 1, 1, 13, 13101,  200.0),
            (10000, 7, 0, 1, 13, 13101,  100.0),
            (10001, 7, 1, 0, 13, 13101,  800.0),
            (10001, 7, 0, 0, 13, 13101,  400.0),
            (10001, 7, 1, 1, 13, 13101,  250.0),
            (10001, 7, 0, 1, 13, 13101,  150.0),
            (10002, 7, 1, 0, 13, 13101, 1200.0),
            (10002, 7, 0, 0, 13, 13101,  600.0),
            (10002, 7, 1, 1, 13, 13101,  350.0),
            (10002, 7, 0, 1, 13, 13101,  200.0);

        CREATE TABLE v2_flow_fromto_city (
            citycode         INTEGER NOT NULL,
            from_area        INTEGER NOT NULL,
            year             INTEGER NOT NULL,
            month            INTEGER NOT NULL,
            dayflag          INTEGER NOT NULL,
            timezone         INTEGER NOT NULL,
            total_population REAL NOT NULL,
            PRIMARY KEY (citycode, from_area, year, month, dayflag, timezone)
        );
        -- 4 区分流入（from_area 0..=3）
        INSERT INTO v2_flow_fromto_city VALUES
            (13101, 0, 2021, 7, 1, 0, 5000.0),
            (13101, 1, 2021, 7, 1, 0, 3000.0),
            (13101, 2, 2021, 7, 1, 0, 1500.0),
            (13101, 3, 2021, 7, 1, 0,  500.0);

        CREATE TABLE v2_posting_mesh1km (
            mesh1kmid     INTEGER NOT NULL,
            snapshot_date TEXT    NOT NULL,
            job_count     INTEGER NOT NULL,
            prefcode      INTEGER NOT NULL,
            PRIMARY KEY (mesh1kmid, snapshot_date)
        );
        INSERT INTO v2_posting_mesh1km VALUES
            (10000, '2021-07-01', 30, 13),
            (10001, '2021-07-01', 50, 13),
            (10002, '2021-07-01', 80, 13);

        CREATE TABLE v2_flow_master_prefcity (
            citycode INTEGER PRIMARY KEY,
            prefname TEXT NOT NULL,
            cityname TEXT NOT NULL
        );
        INSERT INTO v2_flow_master_prefcity VALUES
            (13101, '東京都', '千代田区');
        "#,
    )
    .unwrap();
    drop(conn);

    let db = LocalDb::new(path).unwrap();
    (tmp, db)
}

/// 最小 AppState（Turso/SalesNow/監査は None）
fn make_test_state(hw_db: LocalDb) -> Arc<AppState> {
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

async fn empty_session() -> Session {
    let store = MemoryStore::default();
    Session::new(None, Arc::new(store), None)
}

// ========== L5 逆証明 #1: /api/jobmap/heatmap ==========

/// 逆証明: ヒートマップ API は frontend (templates/tabs/jobmap.html) が参照する
/// `points, data_count, max, meta.{granularity,aggregate_mode,data_source,data_period},
/// truncated, row_limit` を全て返すこと。
///
/// 値レベル逆証明:
/// - 投入 3 メッシュ × 4 象限 = 12 行中 Raw モード (dayflag=1, timezone=0) でクエリ
///   → 3 メッシュ分の points が出ること
/// - max は mesh 10002 の 1200.0 と一致すること
#[tokio::test]
async fn jobmap_heatmap_contract_contains_required_keys_and_correct_values() {
    let (_tmp, db) = make_tmp_hw_db();
    let state = make_test_state(db);
    let session = empty_session().await;

    let params = HeatmapParams {
        year: 2021,
        month: 7,
        dayflag: 1,  // 平日
        timezone: 0, // 昼
        prefcode: Some(13),
        citycode: None,
    };

    let Json(v) = heatmap::jobmap_heatmap(State(state), session, Query(params)).await;

    // frontend 参照キー全てを検証
    assert!(v.get("points").is_some(), "points key missing: {v}");
    assert!(v.get("data_count").is_some(), "data_count missing");
    assert!(v.get("max").is_some(), "max missing");
    assert!(v.get("meta").is_some(), "meta missing");
    assert!(v.get("truncated").is_some(), "truncated missing");
    assert!(v.get("row_limit").is_some(), "row_limit missing");

    let meta = v.get("meta").unwrap();
    assert!(
        meta.get("granularity").is_some(),
        "meta.granularity missing"
    );
    assert!(
        meta.get("aggregate_mode").is_some(),
        "meta.aggregate_mode missing"
    );
    assert!(
        meta.get("data_source").is_some(),
        "meta.data_source missing (MEMORY: feedback_hw_data_scope)"
    );
    assert!(
        meta.get("data_period").is_some(),
        "meta.data_period missing"
    );

    // 逆証明: 投入 3 メッシュが heatmap points に反映されていること
    let points = v.get("points").unwrap().as_array().unwrap();
    assert_eq!(
        points.len(),
        3,
        "投入 3 メッシュが points[] に 3 件返されるべき。実際: {}",
        points.len()
    );

    // 各 point の frontend 参照キー（p.lat, p.lng, p.population）
    let first = &points[0];
    assert!(first.get("lat").is_some(), "point.lat missing");
    assert!(first.get("lng").is_some(), "point.lng missing");
    assert!(
        first.get("population").is_some(),
        "point.population missing"
    );

    // 逆証明: max は投入値 1200.0 と一致（mesh 10002 平日昼）
    let max = v.get("max").unwrap().as_f64().unwrap();
    assert!(
        (max - 1200.0).abs() < 0.01,
        "max={} (expected 1200.0、投入 mesh 10002 平日昼の population)",
        max
    );
}

// ========== L5 逆証明 #2: /api/jobmap/inflow ==========

/// 逆証明: 流入サンキー API は frontend が参照する
/// `sankey.{nodes,links}, summary, year, month, total_population, data_warning`
/// (正常時) または `error, meta, data_warning` (エラー時) を返すこと。
///
/// Turso 未接続環境では fromto fetch が local SQLite にフォールバックする実装と
/// なっているため、hw_db にデータがあれば正常系レスポンスを返すことを期待。
#[tokio::test]
async fn jobmap_inflow_contract_contains_required_keys() {
    let (_tmp, db) = make_tmp_hw_db();
    let state = make_test_state(db);
    let session = empty_session().await;

    let params = InflowParams {
        citycode: 13101,
        year: 2021,
        month: 7,
        dayflag: 1,
        timezone: 0,
    };

    let Json(v) = inflow::jobmap_inflow_sankey(State(state), session, Query(params)).await;

    // data_warning と meta はエラー系でも正常系でも常に返す契約
    assert!(
        v.get("data_warning").is_some(),
        "data_warning must always be present (MEMORY: feedback_hw_data_scope): {v}"
    );

    if v.get("error").is_none() {
        // 正常系: frontend (templates/tabs/jobmap.html renderSankey/renderSummary) 参照キー
        assert!(v.get("sankey").is_some(), "sankey missing: {v}");
        let sankey = v.get("sankey").unwrap();
        assert!(sankey.get("nodes").is_some(), "sankey.nodes missing");
        assert!(sankey.get("links").is_some(), "sankey.links missing");

        assert!(v.get("summary").is_some(), "summary missing");
        assert!(v.get("year").is_some(), "year missing");
        assert!(v.get("month").is_some(), "month missing");
        assert!(
            v.get("total_population").is_some(),
            "total_population missing"
        );

        // summary[] の要素が持つ key (frontend: s.area_name, s.population, s.share)
        let summary = v.get("summary").unwrap().as_array().unwrap();
        if !summary.is_empty() {
            let s = &summary[0];
            assert!(s.get("area_name").is_some(), "summary[].area_name missing");
            assert!(
                s.get("population").is_some(),
                "summary[].population missing"
            );
            assert!(s.get("share").is_some(), "summary[].share missing");
        }
    } else {
        // エラー系: meta も返す契約
        assert!(
            v.get("meta").is_some(),
            "error response should still contain meta"
        );
    }
}

// ========== L5 逆証明 #3: /api/jobmap/correlation ==========

/// 逆証明: 相関 API は frontend が参照する
/// `correlation.{r,n,note}, outliers.{hiring_hard,underserved}, points` を返すこと。
///
/// サンプル数 (3 メッシュ) が少ないため Pearson r は N/A or 非 null 両方あり得るが、
/// key 構造は必ず存在することを検証。
#[tokio::test]
async fn jobmap_correlation_contract_contains_required_keys() {
    let (_tmp, db) = make_tmp_hw_db();
    let state = make_test_state(db);
    let session = empty_session().await;

    let params = CorrelationParams {
        prefcode: 13,
        year: 2021,
        month: 7,
        dayflag: 1,
        timezone: 0,
    };

    let Json(v) = correlation::jobmap_correlation(State(state), session, Query(params)).await;

    if v.get("error").is_none() {
        // frontend (templates/tabs/jobmap.html 相関散布図) 参照キー
        assert!(v.get("correlation").is_some(), "correlation missing: {v}");
        let corr = v.get("correlation").unwrap();
        assert!(
            corr.get("r").is_some(),
            "correlation.r key missing (値が null は許容)"
        );
        assert!(corr.get("n").is_some(), "correlation.n missing");
        assert!(
            corr.get("note").is_some(),
            "correlation.note missing (MEMORY: feedback_correlation_not_causation)"
        );

        assert!(v.get("outliers").is_some(), "outliers missing");
        let out = v.get("outliers").unwrap();
        assert!(
            out.get("hiring_hard").is_some(),
            "outliers.hiring_hard missing"
        );
        assert!(
            out.get("underserved").is_some(),
            "outliers.underserved missing"
        );

        // points[] 各要素 (frontend: p.population, p.job_count, p.mesh, p.category)
        assert!(v.get("points").is_some(), "points missing");
        let points = v.get("points").unwrap().as_array().unwrap();
        if !points.is_empty() {
            let p = &points[0];
            assert!(p.get("population").is_some(), "point.population missing");
            assert!(p.get("job_count").is_some(), "point.job_count missing");
            assert!(p.get("mesh").is_some(), "point.mesh missing");
            assert!(p.get("category").is_some(), "point.category missing");
        }
    }
}

// ========== L5 逆証明 #4: /api/jobmap/labor-flow ==========

/// 逆証明: labor_flow API は SalesNow DB 未接続時に frontend が参照する
/// `error, industries, prefecture` キーを全て含むエラー応答を返すこと。
/// （Frontend: `if (data.error) { ... }` / `data.industries || []` / `data.prefecture`）
#[tokio::test]
async fn jobmap_labor_flow_contract_error_path_when_no_salesnow() {
    let (_tmp, db) = make_tmp_hw_db();
    let state = make_test_state(db);

    let params = LaborFlowParams {
        prefecture: "東京都".to_string(),
        municipality: "千代田区".to_string(),
    };

    let Json(v) = company_markers::labor_flow(State(state), Query(params)).await;

    // SalesNow 未接続 → error 分岐
    assert!(
        v.get("error").is_some(),
        "error key missing in no-salesnow path: {v}"
    );
    assert!(
        v.get("industries").is_some(),
        "industries (fallback []) missing; laborflow.js reads `data.industries || []`"
    );
    assert!(
        v.get("prefecture").is_some(),
        "prefecture echo missing; laborflow.js reads `data.prefecture`"
    );
}

// ========== 既知ミスマッチの記録テスト（#[ignore]） ==========

/// 🔴 BUG MARKER: Mismatch #4 (docs/contract_audit_2026_04_23.md)
///
/// labor_flow backend が `"location"` を返しているが frontend (laborflow.js) は
/// `data.municipality` を参照している。
///
/// 修正方法: `src/handlers/jobmap/company_markers.rs:128` 付近の json! に
/// `"municipality": muni` を追加する。
///
/// 修正完了後は本アサートが通過するようになるので `#[ignore]` を外して
/// CI に永続組み込むこと。
#[tokio::test]
async fn bug_marker_labor_flow_returns_municipality_key() {
    let (_tmp, db) = make_tmp_hw_db();
    let state = make_test_state(db);

    let params = LaborFlowParams {
        prefecture: "東京都".to_string(),
        municipality: "千代田区".to_string(),
    };

    let Json(v) = company_markers::labor_flow(State(state), Query(params)).await;

    assert!(
        v.get("municipality").is_some(),
        "Mismatch #4: frontend (laborflow.js) reads data.municipality but backend \
         returns only 'location'. Got: {v}"
    );
}

/// 🔵 Mismatch #5 観察: `data.center` 形式が endpoint により object と array で不統一
///
/// - `jobmap_seekers` / `markers_to_json`: object `{lat, lng}`
/// - `jobmap_choropleth`: array `[lat, lng]`
///
/// 現状 frontend 側が consumer 毎に使い分け（choropleth_overlay.js は array、
/// postingmap.js は object）しており破綻していない。
/// 統一する場合は OpenAPI 文書化と合わせて実施し、本テストを有効化する。
#[tokio::test]
#[ignore = "Observation only (Mismatch #5): center format inconsistency across endpoints"]
async fn observation_center_format_inconsistency() {
    // 形式統一を行う場合の検証コード（擬似）:
    //   let Json(seekers_resp) = handlers::jobmap_seekers(...).await;
    //   let Json(choropleth_resp) = handlers::jobmap_choropleth(...).await;
    //   assert!(seekers_resp["center"].is_array());
    //   assert!(choropleth_resp["center"].is_array());
    //
    // 現状は handlers モジュールが private のため、構造的にこのテストは
    // 本ファイルからは書けない。統一作業と同時に handlers モジュールの公開化
    // または別の契約検証手段を設計する必要がある。
}
