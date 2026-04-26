//! サブタブ6（予測・推定）系 fetch 関数
//! - Phase 5-1: 充足困難度予測
//! - Phase 5-2: 地域間流動性推定
//! - Phase 5-3: 給与分位（shadow_wage）

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;
use super::query_3level;

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

/// Phase 5-1: 充足困難度予測
pub(crate) fn fetch_fulfillment_summary(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols =
        "emp_group, total_count, avg_score, grade_a_pct, grade_b_pct, grade_c_pct, grade_d_pct";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_score) as avg_score, AVG(grade_a_pct) as grade_a_pct, \
        AVG(grade_b_pct) as grade_b_pct, AVG(grade_c_pct) as grade_c_pct, \
        AVG(grade_d_pct) as grade_d_pct";
    query_3level(
        db,
        "v2_fulfillment_summary",
        pref,
        muni,
        cols,
        "ORDER BY emp_group",
        nat,
        "GROUP BY emp_group ORDER BY emp_group",
    )
}

/// Phase 5-2: 地域間流動性推定（市区町村選択時のみ）
pub(crate) fn fetch_mobility_estimate(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if muni.is_empty() {
        return vec![];
    }
    if !table_exists(db, "v2_mobility_estimate") {
        return vec![];
    }

    let sql = "SELECT emp_group, local_postings, local_avg_salary, gravity_attractiveness, \
               gravity_outflow, net_gravity, top3_destinations \
               FROM v2_mobility_estimate WHERE prefecture = ?1 AND municipality = ?2 \
               ORDER BY emp_group";
    let params = [pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(sql, &p).unwrap_or_default()
}

/// Phase 5-3: 給与分位テーブル
pub(crate) fn fetch_shadow_wage(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_type, total_count, p10, p25, p50, p75, p90, mean, stddev, iqr";
    let nat = "emp_group, salary_type, SUM(total_count) as total_count, \
        AVG(p10) as p10, AVG(p25) as p25, AVG(p50) as p50, AVG(p75) as p75, AVG(p90) as p90, \
        AVG(mean) as mean, AVG(stddev) as stddev, AVG(iqr) as iqr";
    query_3level(
        db,
        "v2_shadow_wage",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group, salary_type",
        nat,
        "AND industry_raw = '' GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type",
    )
}
