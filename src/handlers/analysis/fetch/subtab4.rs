//! サブタブ4（市場構造）系 fetch 関数
//! - Phase 2: L-3 異業種競合、S-1 カスケード
//! - Phase 3: 企業採用戦略4象限、雇用者集中度（独占力）、空間ミスマッチ

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_competition_data(db: &Db, pref: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cross_industry_competition'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT salary_band, education_group, emp_group, total_postings, industry_count, top_industries \
          FROM v2_cross_industry_competition WHERE prefecture = ?1 AND total_postings >= 10 \
          ORDER BY industry_count DESC LIMIT 30".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT salary_band, education_group, emp_group, \
          SUM(total_postings) as total_postings, AVG(industry_count) as industry_count, '' as top_industries \
          FROM v2_cross_industry_competition WHERE total_postings >= 10 \
          GROUP BY salary_band, education_group, emp_group \
          ORDER BY AVG(industry_count) DESC LIMIT 30".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_cascade_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if db
        .query_scalar::<i64>(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cascade_summary'",
            &[],
        )
        .unwrap_or(0)
        == 0
    {
        return vec![];
    }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0 \
          AND posting_count >= 20 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0 \
          AND posting_count >= 30 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 業種別サマリー（各雇用形態の上位）
        ("SELECT emp_group, industry_raw, SUM(posting_count) as posting_count, \
          SUM(facility_count) as facility_count, \
          AVG(avg_salary_min) as avg_salary_min, AVG(median_salary_min) as median_salary_min, \
          AVG(avg_employee_count) as avg_employee_count, AVG(avg_annual_holidays) as avg_annual_holidays, \
          CAST(SUM(vacancy_rate * posting_count) AS REAL) / SUM(posting_count) as vacancy_rate \
          FROM v2_cascade_summary WHERE municipality = '' AND length(industry_raw) > 0 \
          GROUP BY emp_group, industry_raw HAVING SUM(posting_count) >= 100 \
          ORDER BY SUM(posting_count) DESC LIMIT 20".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_employer_strategy(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_employer_strategy_summary") {
        return vec![];
    }

    // Python側テーブルはピボット形式（premium_count/premium_pct/salary_focus_count/...）
    // Rust側はrow形式（strategy_type/count/pct）で表示するため、UNION ALLで変換
    let base_filter = if !muni.is_empty() {
        "WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = ''".to_string()
    } else if !pref.is_empty() {
        "WHERE prefecture = ?1 AND municipality = '' AND industry_raw = ''".to_string()
    } else {
        "WHERE municipality = '' AND industry_raw = ''".to_string()
    };

    let agg = muni.is_empty() && pref.is_empty();

    let (sql, params): (String, Vec<String>) = if agg {
        // 全国集計: SUMでピボットカラムを集計後、UNION ALL
        (
            format!(
                "SELECT emp_group, 'プレミアム型' as strategy_type, SUM(premium_count) as count, \
               CAST(SUM(premium_count) AS REAL) / SUM(total_count) * 100.0 as pct \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, '給与一本勝負型', SUM(salary_focus_count), \
               CAST(SUM(salary_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, '福利厚生重視型', SUM(benefits_focus_count), \
               CAST(SUM(benefits_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, 'コスト優先型', SUM(cost_focus_count), \
               CAST(SUM(cost_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             ORDER BY emp_group, strategy_type",
                f = base_filter
            ),
            vec![],
        )
    } else {
        let params = if !muni.is_empty() {
            vec![pref.to_string(), muni.to_string()]
        } else {
            vec![pref.to_string()]
        };
        (format!(
            "SELECT emp_group, 'プレミアム型' as strategy_type, premium_count as count, premium_pct as pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, '給与一本勝負型', salary_focus_count, salary_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, '福利厚生重視型', benefits_focus_count, benefits_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, 'コスト優先型', cost_focus_count, cost_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             ORDER BY emp_group, strategy_type", f = base_filter), params)
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_monopsony_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_monopsony_index") {
        return vec![];
    }

    // Python側テーブルにtop1_nameカラムは存在しない
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        (
            "SELECT emp_group, SUM(total_postings) as total_postings, \
          SUM(unique_facilities) as unique_facilities, \
          AVG(hhi) as hhi, '' as concentration_level, \
          AVG(top1_share) as top1_share, \
          AVG(top3_share) as top3_share, AVG(top5_share) as top5_share, AVG(gini) as gini \
          FROM v2_monopsony_index WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group"
                .to_string(),
            vec![],
        )
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_spatial_mismatch(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_spatial_mismatch") {
        return vec![];
    }

    // 空間ミスマッチは市区町村レベルのみ（industry_rawフィルタなし）
    let sql = "SELECT emp_group, posting_count, avg_salary_min, \
          accessible_postings_30km, accessible_avg_salary_30km, \
          accessible_postings_60km, salary_gap_vs_accessible, isolation_score \
          FROM v2_spatial_mismatch WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group"
        .to_string();
    let params = [pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}
