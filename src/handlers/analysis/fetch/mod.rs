//! データ取得層（V2独自分析の各サブタブ向け fetch 関数）
//!
//! - サブタブ単位で sub-module に分割（リファクタ F2: ファイル肥大化解消）
//! - 公開 API は従来通り `super::fetch::*` で参照可能（pub use で互換維持）
//!
//! ## サブモジュール構成
//! - `subtab1`: 求人動向（vacancy / resilience / transparency）
//! - `subtab2`: 給与分析（salary_structure / competitiveness / compensation）
//! - `subtab3`: テキスト分析（text_quality / keyword_profile / temperature）
//! - `subtab4`: 市場構造（employer_strategy / monopsony / spatial_mismatch / competition / cascade）
//! - `subtab5_phase4`: Phase 4 外部データ統合（最賃・違反・地域ベンチマーク・人口/社会動態 等 17 関数）
//! - `subtab5_phase4_7`: Phase 4-7 外部データ（外国人・学歴・世帯・日銀短観 等 9 関数）
//! - `subtab6`: 予測・推定（fulfillment / mobility / shadow_wage）
//! - `subtab7_other`: 通勤圏（CommuteMunicipality + CommuteFlow + commute_* + 県平均）
//! - `subtab7_phase_a`: Phase A SSDSE-A 6 テーブル（households / vital_statistics / labor_force / medical_welfare / education_facilities / geography）

use serde_json::Value;
use std::collections::HashMap;

use super::super::helpers::table_exists;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

mod market_intelligence;
mod subtab1;
mod subtab2;
mod subtab3;
mod subtab4;
mod subtab5_phase4;
mod subtab5_phase4_7;
mod subtab6;
mod subtab7_other;
mod subtab7_phase_a;

// ======== 公開 API: 旧 `analysis::fetch` 名前空間互換 ========
//
// 既存呼び出し元（`super::fetch::*` / `analysis::fetch::*`）が
// import を変更せずに動くよう、サブモジュールの全 pub(crate) 関数を
// この mod.rs から再エクスポートする。

pub(crate) use subtab1::{
    fetch_resilience_data, fetch_transparency_data, fetch_vacancy_by_industry, fetch_vacancy_data,
};
pub(crate) use subtab2::{
    fetch_compensation_package, fetch_salary_competitiveness, fetch_salary_structure,
};
pub(crate) use subtab3::{fetch_keyword_profile, fetch_temperature_data, fetch_text_quality};
pub(crate) use subtab4::{
    fetch_cascade_data, fetch_competition_data, fetch_employer_strategy, fetch_monopsony_data,
    fetch_spatial_mismatch,
};
pub(crate) use subtab5_phase4::{
    fetch_anomaly_data, fetch_business_dynamics, fetch_care_demand, fetch_climate,
    fetch_daytime_population, fetch_establishments, fetch_household_spending,
    fetch_job_openings_ratio, fetch_labor_stats, fetch_migration_data, fetch_minimum_wage,
    fetch_population_data, fetch_population_pyramid, fetch_prefecture_stats,
    fetch_region_benchmark, fetch_region_benchmarks_for_prefs, fetch_turnover,
    fetch_wage_compliance,
};
pub(crate) use subtab5_phase4_7::{
    fetch_boj_tankan, fetch_car_ownership, fetch_education, fetch_foreign_residents,
    fetch_household_type, fetch_internet_usage, fetch_land_price, fetch_social_life,
};
// fetch_industry_structure: 媒体分析タブ D-3 (産業別就業者構成) で使用
pub(crate) use subtab5_phase4_7::fetch_industry_structure;
// fetch_hw_industry_counts: CR-9 産業ミスマッチ警戒 (postings 集計+12大分類マッピング)
pub(crate) use subtab5_phase4_7::fetch_hw_industry_counts;
pub(crate) use subtab6::{fetch_fulfillment_summary, fetch_mobility_estimate, fetch_shadow_wage};
pub(crate) use subtab7_other::{
    fetch_commute_inflow, fetch_commute_outflow, fetch_commute_zone, fetch_commute_zone_pyramid,
    fetch_prefecture_mean, fetch_self_commute_rate, CommuteFlow,
};
pub(crate) use subtab7_phase_a::{
    fetch_education_facilities, fetch_geography, fetch_households, fetch_labor_force,
    fetch_medical_welfare, fetch_vital_statistics,
};

// === Phase 3: 採用マーケットインテリジェンス データアクセス層 ===
// 詳細: docs/SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md §5 Step 1〜2
// Step 1: fetch 関数群
// Step 2: 型付き DTO + 変換ヘルパー
#[allow(unused_imports)]
pub(crate) use market_intelligence::{
    aggregate_to_industry_structure_code, fetch_code_master, fetch_code_master_by_names,
    fetch_commute_flow_summary, fetch_industry_structure_for_municipalities,
    fetch_living_cost_proxy, fetch_occupation_cells, fetch_occupation_population,
    fetch_recruiting_scores_by_municipalities, fetch_ward_rankings_by_parent, fetch_ward_thickness,
    to_code_master, to_commute_flows, to_industry_gender_rows, to_living_cost_proxies,
    to_occupation_cells, to_occupation_populations, to_recruiting_scores, to_ward_rankings,
    to_ward_thickness_dtos, CommuteFlowSummary, DataSourceLabel, IndustryGenderRow,
    LivingCostProxy, MunicipalityCodeMasterDto, MunicipalityRecruitingScore, OccupationCellDto,
    OccupationPopulationCell, SurveyMarketIntelligenceData, WardRankingRowDto, WardThicknessDto,
};
// CommuteMunicipality は subtab7_other 内部のみで使用 (fetch_commute_zone の戻り値型として fetch_commute_zone_pyramid に渡される)
// 外部からは直接参照されないため再エクスポートしない

// ======== 共通ヘルパー ========

/// `v2_external_*` 系テーブルの「ヘッダー風文字列レコード」混入を除外する WHERE 句断片。
///
/// CSV 投入時に 1 行目ヘッダーがレコード化されたデータ品質問題への防御。
/// `v2_external_population` と `v2_external_foreign_residents` で各 1 件混入が確認済 (2026-05-03)。
///
/// `prefecture` と `municipality` の両方を持つテーブル用。
///
/// 使用例:
/// ```ignore
/// let sql = format!(
///     "SELECT ... FROM v2_external_population WHERE prefecture = ?1 AND {}",
///     EXTERNAL_CLEAN_FILTER
/// );
/// ```
///
/// 詳細: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md`
pub(crate) const EXTERNAL_CLEAN_FILTER: &str = "prefecture IS NOT NULL \
     AND prefecture <> '' \
     AND prefecture <> '都道府県' \
     AND municipality <> '市区町村'";

/// `municipality` カラムがないテーブル用 (例: `v2_external_foreign_residents`)。
///
/// `prefecture` のみで防御する。
pub(crate) const EXTERNAL_CLEAN_FILTER_NO_MUNI: &str = "prefecture IS NOT NULL \
     AND prefecture <> '' \
     AND prefecture <> '都道府県'";

/// Turso外部DBクエリ実行ヘルパー
/// Turso接続がある場合はTursoを使い、なければローカルDBにフォールバック
pub(crate) fn query_turso_or_local(
    turso: Option<&TursoDb>,
    local_db: &Db,
    sql: &str,
    params: &[String],
    local_table_check: &str,
) -> Vec<Row> {
    // Turso優先
    if let Some(tdb) = turso {
        let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params
            .iter()
            .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
            .collect();
        match tdb.query(sql, &p) {
            Ok(rows) if !rows.is_empty() => return rows,
            Ok(_) => {} // 空結果 → ローカルにフォールバック
            Err(e) => {
                tracing::warn!("Turso query failed, falling back to local: {e}");
            }
        }
    }

    // ローカルDBフォールバック
    if !local_table_check.is_empty() && !table_exists(local_db, local_table_check) {
        return vec![];
    }
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    local_db.query(sql, &p).unwrap_or_default()
}

/// 3レベルフィルタクエリ実行（市区町村→都道府県→全国）
pub(super) fn query_3level(
    db: &Db,
    table: &str,
    pref: &str,
    muni: &str,
    select_cols: &str,
    filter_suffix: &str,
    national_select: &str,
    national_suffix: &str,
) -> Vec<Row> {
    if !table_exists(db, table) {
        return vec![];
    }
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            format!(
                "SELECT {} FROM {} WHERE prefecture = ?1 AND municipality = ?2 {}",
                select_cols, table, filter_suffix
            ),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            format!(
                "SELECT {} FROM {} WHERE prefecture = ?1 AND municipality = '' {}",
                select_cols, table, filter_suffix
            ),
            vec![pref.to_string()],
        )
    } else {
        (
            format!(
                "SELECT {} FROM {} WHERE municipality = '' {}",
                national_select, table, national_suffix
            ),
            vec![],
        )
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db::local_sqlite::LocalDb;

    /// 空の一時 SQLite DB を作成 (既存パターン: `db/local_sqlite.rs:131` の `create_test_db` 流用)
    fn create_test_db() -> (tempfile::NamedTempFile, LocalDb) {
        let tmp = tempfile::NamedTempFile::new().unwrap();
        let path = tmp.path().to_str().unwrap();
        // 一旦空ファイルを sqlite として認識可能にする (LocalDb::new は exists チェックあり)
        let _ = rusqlite::Connection::open(path).unwrap();
        let db = LocalDb::new(path).unwrap();
        (tmp, db)
    }

    /// `EXTERNAL_CLEAN_FILTER` が `v2_external_population` のヘッダー混入レコードを除外することを検証する。
    ///
    /// シナリオ: in-memory DB に正規 2 行 + ヘッダー風 1 行を投入し、
    /// 全国集計クエリ (WHERE filter のみ) で 2 行のみ取得されることを確認。
    #[test]
    fn test_external_clean_filter_excludes_header_records_in_population() {
        let (_tmp, db) = create_test_db();

        db.execute(
            "CREATE TABLE v2_external_population (
                prefecture TEXT,
                municipality TEXT,
                total_population INTEGER,
                male_population INTEGER,
                female_population INTEGER,
                age_0_14 INTEGER,
                age_15_64 INTEGER,
                age_65_over INTEGER,
                aging_rate REAL,
                working_age_rate REAL,
                youth_rate REAL,
                reference_date TEXT,
                PRIMARY KEY (prefecture, municipality)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        // ヘッダー混入レコード (1 行)
        db.execute(
            "INSERT INTO v2_external_population VALUES \
             ('都道府県', '市区町村', 0, 0, 0, 0, 0, 0, 0.0, 0.0, 0.0, '2020-10-01')",
            &[],
        )
        .expect("ヘッダー INSERT 失敗");

        // 正規レコード (2 行)
        db.execute(
            "INSERT INTO v2_external_population VALUES \
             ('北海道', '札幌市', 1973395, 918682, 1054713, 217554, 1196458, 559383, 28.34, 60.62, 11.03, '2020-10-01')",
            &[],
        )
        .expect("INSERT 失敗");
        db.execute(
            "INSERT INTO v2_external_population VALUES \
             ('東京都', '新宿区', 349385, 174251, 175134, 30000, 250000, 69385, 19.86, 71.55, 8.59, '2020-10-01')",
            &[],
        )
        .expect("INSERT 失敗");

        // EXTERNAL_CLEAN_FILTER を直接使ったクエリを検証
        let sql = format!(
            "SELECT prefecture, municipality FROM v2_external_population WHERE {EXTERNAL_CLEAN_FILTER}"
        );
        let rows = db.query(&sql, &[]).expect("SELECT 失敗");

        assert_eq!(
            rows.len(),
            2,
            "ヘッダー除外後は正規 2 件であること (実際: {})",
            rows.len()
        );

        // ヘッダー文字列がレコードに含まれないこと
        for row in &rows {
            let pref = row.get("prefecture").and_then(|v| v.as_str()).unwrap_or("");
            let muni = row
                .get("municipality")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            assert_ne!(
                pref, "都道府県",
                "prefecture='都道府県' は除外されているべき"
            );
            assert_ne!(
                muni, "市区町村",
                "municipality='市区町村' は除外されているべき"
            );
        }
    }

    /// `EXTERNAL_CLEAN_FILTER_NO_MUNI` が `municipality` 列を持たないテーブル
    /// (例: `v2_external_foreign_residents`) でヘッダーを除外することを検証する。
    #[test]
    fn test_external_clean_filter_no_muni_excludes_header_in_foreign_residents() {
        let (_tmp, db) = create_test_db();

        db.execute(
            "CREATE TABLE v2_external_foreign_residents (
                prefecture TEXT,
                visa_status TEXT,
                count INTEGER,
                survey_period TEXT,
                PRIMARY KEY (prefecture, visa_status)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        // ヘッダー風 + 正規 2 件
        db.execute(
            "INSERT INTO v2_external_foreign_residents VALUES \
             ('都道府県', 'visa', 0, '2024Q4'), \
             ('東京都', '永住者', 100000, '2024Q4'), \
             ('北海道', '技能実習', 5000, '2024Q4')",
            &[],
        )
        .expect("INSERT 失敗");

        let sql = format!(
            "SELECT prefecture FROM v2_external_foreign_residents WHERE {EXTERNAL_CLEAN_FILTER_NO_MUNI}"
        );
        let rows = db.query(&sql, &[]).expect("SELECT 失敗");
        assert_eq!(rows.len(), 2, "ヘッダー除外後は 2 件");
        for row in &rows {
            let pref = row.get("prefecture").and_then(|v| v.as_str()).unwrap_or("");
            assert_ne!(pref, "都道府県");
        }
    }

    /// 空文字 / NULL の prefecture も除外されること (二重防御)。
    #[test]
    fn test_external_clean_filter_excludes_empty_and_null_prefecture() {
        let (_tmp, db) = create_test_db();

        db.execute(
            "CREATE TABLE v2_external_population (
                prefecture TEXT,
                municipality TEXT,
                total_population INTEGER,
                PRIMARY KEY (prefecture, municipality)
            )",
            &[],
        )
        .expect("CREATE TABLE 失敗");

        db.execute(
            "INSERT INTO v2_external_population VALUES \
             ('', '札幌市', 1000), \
             (NULL, '函館市', 500), \
             ('北海道', '小樽市', 200)",
            &[],
        )
        .expect("INSERT 失敗");

        let sql = format!(
            "SELECT prefecture, municipality FROM v2_external_population WHERE {EXTERNAL_CLEAN_FILTER}"
        );
        let rows = db.query(&sql, &[]).expect("SELECT 失敗");
        assert_eq!(rows.len(), 1, "正規 1 件のみ通過 (北海道/小樽市)");
        assert_eq!(
            rows[0].get("prefecture").and_then(|v| v.as_str()),
            Some("北海道")
        );
    }
}
