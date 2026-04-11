//! 集計モジュール
//! パース済みレコードを地域別・給与帯別・雇用形態別・タグ別に集計

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

use super::upload::SurveyRecord;
use super::statistics::enhanced_salary_statistics;

/// 企業別集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CompanyAgg {
    pub name: String,
    pub count: usize,
    pub avg_salary: i64,
    pub median_salary: i64,
}

/// 雇用形態別給与
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmpTypeSalary {
    pub emp_type: String,
    pub count: usize,
    pub avg_salary: i64,
    pub median_salary: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SurveyAggregation {
    pub total_count: usize,
    pub new_count: usize,
    pub salary_parse_rate: f64,
    pub location_parse_rate: f64,
    pub dominant_prefecture: Option<String>,
    pub dominant_municipality: Option<String>,
    pub by_prefecture: Vec<(String, usize)>,
    pub by_salary_range: Vec<(String, usize)>,
    pub by_employment_type: Vec<(String, usize)>,
    pub by_tags: Vec<(String, usize)>,
    pub salary_values: Vec<i64>,
    pub enhanced_stats: Option<super::statistics::EnhancedStats>,
    // レポート用追加フィールド
    pub by_company: Vec<CompanyAgg>,
    pub by_emp_type_salary: Vec<EmpTypeSalary>,
    pub salary_min_values: Vec<i64>,
    pub salary_max_values: Vec<i64>,
}

/// パース済みレコードを集計
pub fn aggregate_records(records: &[SurveyRecord]) -> SurveyAggregation {
    let total = records.len();
    if total == 0 { return SurveyAggregation::default(); }

    let new_count = records.iter().filter(|r| r.is_new).count();

    // パース成功率
    let salary_ok = records.iter().filter(|r| r.salary_parsed.min_value.is_some()).count();
    let location_ok = records.iter().filter(|r| r.location_parsed.prefecture.is_some()).count();

    // 都道府県別
    let mut pref_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        if let Some(pref) = &r.location_parsed.prefecture {
            *pref_map.entry(pref.clone()).or_default() += 1;
        }
    }
    let mut by_prefecture: Vec<(String, usize)> = pref_map.into_iter().collect();
    by_prefecture.sort_by(|a, b| b.1.cmp(&a.1));

    let dominant_prefecture = by_prefecture.first().map(|(p, _)| p.clone());

    // 市区町村別（最多を特定）
    let mut muni_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        if let Some(muni) = &r.location_parsed.municipality {
            *muni_map.entry(muni.clone()).or_default() += 1;
        }
    }
    let dominant_municipality = muni_map.into_iter()
        .max_by_key(|(_, c)| *c)
        .map(|(m, _)| m);

    // 給与レンジ別
    let mut salary_range_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        if let Some(cat) = &r.salary_parsed.range_category {
            *salary_range_map.entry(cat.clone()).or_default() += 1;
        }
    }
    let mut by_salary_range: Vec<(String, usize)> = salary_range_map.into_iter().collect();
    by_salary_range.sort_by(|a, b| a.0.cmp(&b.0));

    // 雇用形態別
    let mut emp_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        let emp = if r.employment_type.is_empty() { "不明".to_string() }
            else { r.employment_type.clone() };
        *emp_map.entry(emp).or_default() += 1;
    }
    let mut by_employment_type: Vec<(String, usize)> = emp_map.into_iter().collect();
    by_employment_type.sort_by(|a, b| b.1.cmp(&a.1));

    // タグ別（カンマ/スペース区切りで分解）
    let mut tag_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        if !r.tags_raw.is_empty() {
            for tag in r.tags_raw.split(|c: char| c == ',' || c == '、' || c == '/' || c == '\t') {
                let tag = tag.trim();
                if !tag.is_empty() && tag.chars().count() <= 20 {
                    *tag_map.entry(tag.to_string()).or_default() += 1;
                }
            }
        }
    }
    let mut by_tags: Vec<(String, usize)> = tag_map.into_iter().collect();
    by_tags.sort_by(|a, b| b.1.cmp(&a.1));
    by_tags.truncate(30); // 上位30タグ

    // 給与統計
    let salary_values: Vec<i64> = records.iter()
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .collect();
    let enhanced_stats = enhanced_salary_statistics(&salary_values);

    // 下限/上限給与（レポート用）
    let salary_min_values: Vec<i64> = records.iter()
        .filter_map(|r| r.salary_parsed.min_value)
        .collect();
    let salary_max_values: Vec<i64> = records.iter()
        .filter_map(|r| r.salary_parsed.max_value)
        .collect();

    // 企業別集計
    let mut company_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        if !r.company_name.is_empty() {
            let salary = r.salary_parsed.unified_monthly.unwrap_or(0);
            company_map.entry(r.company_name.clone()).or_default().push(salary);
        }
    }
    let mut by_company: Vec<CompanyAgg> = company_map.into_iter()
        .map(|(name, salaries)| {
            let count = salaries.len();
            let valid: Vec<i64> = salaries.iter().filter(|&&s| s > 0).copied().collect();
            let avg_salary = if valid.is_empty() { 0 } else { valid.iter().sum::<i64>() / valid.len() as i64 };
            let median_salary = if valid.is_empty() { 0 } else {
                let mut sorted = valid.clone();
                sorted.sort();
                sorted[sorted.len() / 2]
            };
            CompanyAgg { name, count, avg_salary, median_salary }
        })
        .collect();
    by_company.sort_by(|a, b| b.count.cmp(&a.count));

    // 雇用形態別給与
    let mut emp_salary_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        let emp = if r.employment_type.is_empty() { "不明".to_string() }
            else { r.employment_type.clone() };
        if let Some(sal) = r.salary_parsed.unified_monthly {
            emp_salary_map.entry(emp).or_default().push(sal);
        }
    }
    let mut by_emp_type_salary: Vec<EmpTypeSalary> = emp_salary_map.into_iter()
        .map(|(emp_type, salaries)| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() { 0 } else { salaries.iter().sum::<i64>() / count as i64 };
            let median_salary = if salaries.is_empty() { 0 } else {
                let mut sorted = salaries;
                sorted.sort();
                sorted[sorted.len() / 2]
            };
            EmpTypeSalary { emp_type, count, avg_salary, median_salary }
        })
        .collect();
    by_emp_type_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    SurveyAggregation {
        total_count: total,
        new_count,
        salary_parse_rate: salary_ok as f64 / total as f64,
        location_parse_rate: location_ok as f64 / total as f64,
        dominant_prefecture,
        dominant_municipality,
        by_prefecture,
        by_salary_range,
        by_employment_type,
        by_tags,
        salary_values,
        enhanced_stats,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
    }
}
