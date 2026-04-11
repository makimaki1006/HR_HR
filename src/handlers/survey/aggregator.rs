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

/// タグ別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TagSalaryAgg {
    pub tag: String,
    pub count: usize,
    pub avg_salary: i64,
    pub diff_from_avg: i64,   // 全体平均との差分（円）
    pub diff_percent: f64,    // 差分率（%）
}

/// 市区町村別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct MunicipalitySalaryAgg {
    pub name: String,
    pub prefecture: String,
    pub count: usize,
    pub avg_salary: i64,
    pub median_salary: i64,
}

/// 都道府県別給与集計
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PrefectureSalaryAgg {
    pub name: String,
    pub count: usize,
    pub avg_salary: i64,
    pub avg_min_salary: i64,  // 下限給与の平均
}

/// 散布図データ点
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ScatterPoint {
    pub x: i64,
    pub y: i64,
}

/// 回帰分析結果
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RegressionResult {
    pub slope: f64,
    pub intercept: f64,
    pub r_squared: f64,
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
    pub by_tag_salary: Vec<TagSalaryAgg>,
    pub by_municipality_salary: Vec<MunicipalitySalaryAgg>,
    pub scatter_min_max: Vec<ScatterPoint>,
    pub regression_min_max: Option<RegressionResult>,
    pub by_prefecture_salary: Vec<PrefectureSalaryAgg>,
    pub is_hourly: bool,
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

    // タグ別給与集計
    let mut tag_salary_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        if let Some(sal) = r.salary_parsed.unified_monthly {
            if sal > 0 && !r.tags_raw.is_empty() {
                for tag in r.tags_raw.split(|c: char| c == ',' || c == '、' || c == '/' || c == '\t') {
                    let tag = tag.trim();
                    if !tag.is_empty() && tag.chars().count() <= 20 {
                        tag_salary_map.entry(tag.to_string()).or_default().push(sal);
                    }
                }
            }
        }
    }

    // 給与統計
    let salary_values: Vec<i64> = records.iter()
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .collect();
    let enhanced_stats = enhanced_salary_statistics(&salary_values);

    // タグ別給与差分の計算
    let overall_mean = enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);
    let mut by_tag_salary: Vec<TagSalaryAgg> = tag_salary_map.into_iter()
        .filter(|(_, salaries)| salaries.len() >= 3) // 3件以上のタグのみ
        .map(|(tag, salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let diff_from_avg = avg_salary - overall_mean;
            let diff_percent = if overall_mean > 0 {
                diff_from_avg as f64 / overall_mean as f64 * 100.0
            } else { 0.0 };
            TagSalaryAgg { tag, count, avg_salary, diff_from_avg, diff_percent }
        })
        .collect();
    by_tag_salary.sort_by(|a, b| b.diff_from_avg.cmp(&a.diff_from_avg));
    by_tag_salary.truncate(20);

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

    // 都道府県別給与集計（最低賃金比較用）
    let mut pref_salary_map: HashMap<String, (Vec<i64>, Vec<i64>)> = HashMap::new(); // (unified, min_values)
    for r in records {
        if let Some(pref) = &r.location_parsed.prefecture {
            let entry = pref_salary_map.entry(pref.clone()).or_default();
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 { entry.0.push(sal); }
            }
            if let Some(min_sal) = r.salary_parsed.min_value {
                if min_sal > 0 { entry.1.push(min_sal); }
            }
        }
    }
    let mut by_prefecture_salary: Vec<PrefectureSalaryAgg> = pref_salary_map.into_iter()
        .map(|(name, (salaries, min_salaries))| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() { 0 } else { salaries.iter().sum::<i64>() / count as i64 };
            let avg_min_salary = if min_salaries.is_empty() { 0 } else {
                min_salaries.iter().sum::<i64>() / min_salaries.len() as i64
            };
            PrefectureSalaryAgg { name, count, avg_salary, avg_min_salary }
        })
        .collect();
    by_prefecture_salary.sort_by(|a, b| b.count.cmp(&a.count));

    // 時給モード判定（時給レコードが過半数なら時給モード）
    let hourly_count = records.iter()
        .filter(|r| r.salary_parsed.salary_type == super::salary_parser::SalaryType::Hourly)
        .count();
    let total_with_salary = records.iter()
        .filter(|r| r.salary_parsed.min_value.is_some())
        .count();
    let is_hourly = total_with_salary > 0 && hourly_count > total_with_salary / 2;

    // 散布図データ（下限 vs 上限）
    let scatter_min_max: Vec<ScatterPoint> = records.iter()
        .filter_map(|r| {
            let min = r.salary_parsed.min_value?;
            let max = r.salary_parsed.max_value?;
            if min > 0 && max > 0 && max >= min { Some(ScatterPoint { x: min, y: max }) } else { None }
        })
        .collect();
    let regression_min_max = linear_regression_points(&scatter_min_max);

    // 市区町村別給与集計
    let mut muni_salary_map: HashMap<(String, String), Vec<i64>> = HashMap::new();
    for r in records {
        if let (Some(pref), Some(muni)) = (&r.location_parsed.prefecture, &r.location_parsed.municipality) {
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    muni_salary_map.entry((pref.clone(), muni.clone())).or_default().push(sal);
                }
            }
        }
    }
    let mut by_municipality_salary: Vec<MunicipalitySalaryAgg> = muni_salary_map.into_iter()
        .map(|((pref, name), salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let median_salary = {
                let mut sorted = salaries;
                sorted.sort();
                sorted[sorted.len() / 2]
            };
            MunicipalitySalaryAgg { name, prefecture: pref, count, avg_salary, median_salary }
        })
        .collect();
    by_municipality_salary.sort_by(|a, b| b.count.cmp(&a.count));
    by_municipality_salary.truncate(15);

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
        by_tag_salary,
        by_municipality_salary,
        scatter_min_max,
        regression_min_max,
        by_prefecture_salary,
        is_hourly,
    }
}

/// 線形回帰（最小二乗法）
fn linear_regression_points(points: &[ScatterPoint]) -> Option<RegressionResult> {
    let n = points.len();
    if n < 3 { return None; }
    let n_f = n as f64;
    let sum_x: f64 = points.iter().map(|p| p.x as f64).sum();
    let sum_y: f64 = points.iter().map(|p| p.y as f64).sum();
    let sum_xy: f64 = points.iter().map(|p| p.x as f64 * p.y as f64).sum();
    let sum_x2: f64 = points.iter().map(|p| (p.x as f64).powi(2)).sum();

    let denom = n_f * sum_x2 - sum_x.powi(2);
    if denom.abs() < 1e-10 { return None; }

    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_f;

    // R²計算
    let mean_y = sum_y / n_f;
    let ss_tot: f64 = points.iter().map(|p| (p.y as f64 - mean_y).powi(2)).sum();
    let ss_res: f64 = points.iter().map(|p| {
        let pred = slope * p.x as f64 + intercept;
        (p.y as f64 - pred).powi(2)
    }).sum();
    let r_squared = if ss_tot > 0.0 { 1.0 - ss_res / ss_tot } else { 0.0 };

    Some(RegressionResult { slope, intercept, r_squared })
}
