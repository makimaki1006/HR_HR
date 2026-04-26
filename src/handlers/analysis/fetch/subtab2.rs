//! サブタブ2（給与分析）系 fetch 関数
//! - Phase 1B: 給与構造、給与競争力指数、報酬パッケージ総合評価

use serde_json::Value;
use std::collections::HashMap;

use super::query_3level;

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_salary_structure(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_type, total_count, avg_salary_min, avg_salary_max, \
        median_salary_min, p25_salary_min, p75_salary_min, p90_salary_min, \
        salary_spread, avg_bonus_months, estimated_annual_min";
    let nat = "emp_group, salary_type, SUM(total_count) as total_count, \
        AVG(avg_salary_min) as avg_salary_min, AVG(avg_salary_max) as avg_salary_max, \
        AVG(median_salary_min) as median_salary_min, AVG(p25_salary_min) as p25_salary_min, \
        AVG(p75_salary_min) as p75_salary_min, AVG(p90_salary_min) as p90_salary_min, \
        AVG(salary_spread) as salary_spread, AVG(avg_bonus_months) as avg_bonus_months, \
        AVG(estimated_annual_min) as estimated_annual_min";
    query_3level(
        db,
        "v2_salary_structure",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group, salary_type",
        nat,
        "AND industry_raw = '' GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type",
    )
}

pub(crate) fn fetch_salary_competitiveness(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, local_avg_salary, national_avg_salary, competitiveness_index, \
        percentile_rank, sample_count";
    let nat = "emp_group, AVG(local_avg_salary) as local_avg_salary, \
        AVG(national_avg_salary) as national_avg_salary, \
        AVG(competitiveness_index) as competitiveness_index, \
        AVG(percentile_rank) as percentile_rank, SUM(sample_count) as sample_count";
    query_3level(
        db,
        "v2_salary_competitiveness",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}

pub(crate) fn fetch_compensation_package(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_count, avg_salary_min, avg_annual_holidays, avg_bonus_months, \
        salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_salary_min) as avg_salary_min, AVG(avg_annual_holidays) as avg_annual_holidays, \
        AVG(avg_bonus_months) as avg_bonus_months, \
        AVG(salary_pctile) as salary_pctile, AVG(holidays_pctile) as holidays_pctile, \
        AVG(bonus_pctile) as bonus_pctile, AVG(composite_score) as composite_score, \
        '' as rank_label";
    query_3level(
        db,
        "v2_compensation_package",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}
