//! Turso時系列集計テーブルからのデータ取得関数

use std::collections::HashMap;
use serde_json::Value;

type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

/// Tursoクエリ実行ヘルパー（Turso専用、ローカルフォールバックなし）
fn query_turso(
    turso: &TursoDb,
    sql: &str,
    params: &[String],
) -> Vec<Row> {
    let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
        params.iter().map(|s| s as &dyn crate::db::turso_http::ToSqlTurso).collect();
    match turso.query(sql, &p) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("Turso trend query failed: {e}");
            vec![]
        }
    }
}

// ======== Sub1: 量の変化 ========

/// 求人数・事業所数の時系列（ts_turso_counts）
pub(crate) fn fetch_ts_counts(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(posting_count) as posting_count, \
             SUM(facility_count) as facility_count \
             FROM ts_turso_counts WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(posting_count) as posting_count, \
             SUM(facility_count) as facility_count \
             FROM ts_turso_counts \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

/// 欠員率・増員率の時系列（ts_turso_vacancy）
pub(crate) fn fetch_ts_vacancy(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(total_count) as total_count, \
             SUM(vacancy_count) as vacancy_count, \
             SUM(growth_count) as growth_count, \
             CAST(SUM(vacancy_count) AS REAL) / NULLIF(SUM(total_count), 0) as vacancy_rate, \
             CAST(SUM(growth_count) AS REAL) / NULLIF(SUM(total_count), 0) as growth_rate \
             FROM ts_turso_vacancy WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(total_count) as total_count, \
             SUM(vacancy_count) as vacancy_count, \
             SUM(growth_count) as growth_count, \
             CAST(SUM(vacancy_count) AS REAL) / NULLIF(SUM(total_count), 0) as vacancy_rate, \
             CAST(SUM(growth_count) AS REAL) / NULLIF(SUM(total_count), 0) as growth_rate \
             FROM ts_turso_vacancy \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

// ======== Sub2: 質の変化 ========

/// 給与統計の時系列（ts_turso_salary）
/// カラム: snapshot_id, prefecture, industry_major_code, emp_group, count, mean_min, mean_max, median_min, min_val, max_val
pub(crate) fn fetch_ts_salary(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(mean_min) as mean_min, AVG(mean_max) as mean_max, \
             AVG(median_min) as median_min, \
             SUM(count) as count \
             FROM ts_turso_salary WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(mean_min) as mean_min, AVG(mean_max) as mean_max, \
             AVG(median_min) as median_min, \
             SUM(count) as count \
             FROM ts_turso_salary \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

/// 働き方統計の時系列（ts_agg_workstyle）
/// カラム: prefecture, emp_group, count, avg_annual_holidays, avg_overtime, snapshot_id
pub(crate) fn fetch_ts_workstyle(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(avg_annual_holidays) as avg_annual_holidays, \
             AVG(avg_overtime) as avg_overtime, \
             SUM(count) as count \
             FROM ts_agg_workstyle WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(avg_annual_holidays) as avg_annual_holidays, \
             AVG(avg_overtime) as avg_overtime, \
             SUM(count) as count \
             FROM ts_agg_workstyle \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

// ======== Sub3: 構造の変化 ========

/// 充足度統計の時系列（ts_turso_fulfillment）
/// カラム: snapshot_id, prefecture, industry_major_code, emp_group, count, avg_listing_days, median_listing_days, long_term_count, very_long_count
pub(crate) fn fetch_ts_fulfillment(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(avg_listing_days) as avg_listing_days, \
             SUM(long_term_count) as long_term_count, \
             SUM(very_long_count) as very_long_count, \
             SUM(count) as count \
             FROM ts_turso_fulfillment WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             AVG(avg_listing_days) as avg_listing_days, \
             SUM(long_term_count) as long_term_count, \
             SUM(very_long_count) as very_long_count, \
             SUM(count) as count \
             FROM ts_turso_fulfillment \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

// ======== Sub4: シグナル ========

/// 求人追跡統計の時系列（ts_agg_tracking）
/// カラム: snapshot_id, prefecture, industry_major_code, emp_group, new_count, continue_count, end_count, churn_rate
pub(crate) fn fetch_ts_tracking(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let (sql, params) = if !pref.is_empty() {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(new_count) as new_count, \
             SUM(continue_count) as continue_count, \
             SUM(end_count) as end_count, \
             AVG(churn_rate) as churn_rate \
             FROM ts_agg_tracking WHERE prefecture = ?1 \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT snapshot_id, emp_group, \
             SUM(new_count) as new_count, \
             SUM(continue_count) as continue_count, \
             SUM(end_count) as end_count, \
             AVG(churn_rate) as churn_rate \
             FROM ts_agg_tracking \
             GROUP BY snapshot_id, emp_group \
             ORDER BY snapshot_id, emp_group".to_string(),
            vec![],
        )
    };
    query_turso(turso, &sql, &params)
}

// ======== Sub5: 外部比較 ========

/// 有効求人倍率の年度別推移（v2_external_job_openings_ratio）
pub(crate) fn fetch_ext_job_openings_ratio(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let effective_pref = if pref.is_empty() { "全国" } else { pref };
    let sql = "SELECT fiscal_year, ratio_total, ratio_excl_part \
               FROM v2_external_job_openings_ratio \
               WHERE prefecture = ?1 \
               ORDER BY fiscal_year";
    query_turso(turso, sql, &[effective_pref.to_string()])
}

/// 労働統計の年度別推移（v2_external_labor_stats）
/// 月給（男女別）、パート時給（男女別）、離職率、転職率
pub(crate) fn fetch_ext_labor_stats(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let effective_pref = if pref.is_empty() { "全国" } else { pref };
    let sql = "SELECT fiscal_year, monthly_salary_male, monthly_salary_female, \
               part_time_wage_male, part_time_wage_female, \
               separation_rate, turnover_rate \
               FROM v2_external_labor_stats \
               WHERE prefecture = ?1 \
               ORDER BY fiscal_year";
    query_turso(turso, sql, &[effective_pref.to_string()])
}

/// 入職・離職率の年度別推移（v2_external_turnover）
/// 産業計のみ取得
pub(crate) fn fetch_ext_turnover(turso: &TursoDb, pref: &str) -> Vec<Row> {
    let effective_pref = if pref.is_empty() { "全国" } else { pref };
    let sql = "SELECT fiscal_year, entry_rate, separation_rate, net_rate \
               FROM v2_external_turnover \
               WHERE prefecture = ?1 AND industry = '産業計' \
               ORDER BY fiscal_year";
    query_turso(turso, sql, &[effective_pref.to_string()])
}
