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
pub(crate) use subtab6::{fetch_fulfillment_summary, fetch_mobility_estimate, fetch_shadow_wage};
pub(crate) use subtab7_other::{
    fetch_commute_inflow, fetch_commute_outflow, fetch_commute_zone, fetch_commute_zone_pyramid,
    fetch_prefecture_mean, fetch_self_commute_rate, CommuteFlow,
};
pub(crate) use subtab7_phase_a::{
    fetch_education_facilities, fetch_geography, fetch_households, fetch_labor_force,
    fetch_medical_welfare, fetch_vital_statistics,
};
// CommuteMunicipality は subtab7_other 内部のみで使用 (fetch_commute_zone の戻り値型として fetch_commute_zone_pyramid に渡される)
// 外部からは直接参照されないため再エクスポートしない

// ======== 共通ヘルパー ========

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
