//! サブタブ1（求人動向）系 fetch 関数
//! - C-4 欠員補充率（全体 + 業種別）
//! - S-2 地域レジリエンス
//! - C-1 透明性スコア

use serde_json::Value;
use std::collections::HashMap;

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_vacancy_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, SUM(vacancy_count) as vacancy_count, \
          SUM(growth_count) as growth_count, SUM(new_facility_count) as new_facility_count, \
          CAST(SUM(vacancy_count) AS REAL) / SUM(total_count) as vacancy_rate, \
          CAST(SUM(growth_count) AS REAL) / SUM(total_count) as growth_rate \
          FROM v2_vacancy_rate WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// C-4 業種別: 正社員とパートの欠員補充率上位10業種
pub(crate) fn fetch_vacancy_by_industry(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (filter, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0".to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0".to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国: 業種集計
        (
            "municipality = '' AND length(industry_raw) > 0".to_string(),
            vec![],
        )
    };

    let sql = format!(
        "SELECT industry_raw, emp_group, total_count, vacancy_rate, growth_rate \
         FROM v2_vacancy_rate WHERE {filter} AND total_count >= 30 \
         ORDER BY vacancy_rate DESC LIMIT 30"
    );
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_resilience_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        ("SELECT prefecture as emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE municipality = '' AND emp_group = '正社員' \
          ORDER BY shannon_index DESC LIMIT 10".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_transparency_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_transparency) as avg_transparency, AVG(median_transparency) as median_transparency, \
          AVG(disclosure_annual_holidays) as disclosure_annual_holidays, \
          AVG(disclosure_bonus_months) as disclosure_bonus_months, \
          AVG(disclosure_employee_count) as disclosure_employee_count, \
          AVG(disclosure_capital) as disclosure_capital, \
          AVG(disclosure_overtime) as disclosure_overtime, \
          AVG(disclosure_female_ratio) as disclosure_female_ratio, \
          AVG(disclosure_parttime_ratio) as disclosure_parttime_ratio, \
          AVG(disclosure_founding_year) as disclosure_founding_year \
          FROM v2_transparency_score WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}
