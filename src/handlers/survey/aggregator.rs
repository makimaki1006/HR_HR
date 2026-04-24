//! 集計モジュール
//! パース済みレコードを地域別・給与帯別・雇用形態別・タグ別に集計

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::statistics::enhanced_salary_statistics;
use super::upload::SurveyRecord;

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
    pub diff_from_avg: i64, // 全体平均との差分（円）
    pub diff_percent: f64,  // 差分率（%）
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
    pub avg_min_salary: i64, // 下限給与の平均
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

/// スライスの中央値を計算（コピー＆ソートする）
/// - 空: 0
/// - 奇数件: 中央要素
/// - 偶数件: 中央2要素の平均（整数割り算）
/// `enhanced_salary_statistics` の定義と整合。
fn median_of(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted: Vec<i64> = values.to_vec();
    sorted.sort();
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    } else {
        sorted[n / 2]
    }
}

/// パース済みレコードを集計
/// 後方互換: 自動判定モードで集計
pub fn aggregate_records(records: &[SurveyRecord]) -> SurveyAggregation {
    aggregate_records_with_mode(records, super::upload::WageMode::Auto)
}

/// ユーザー明示の給与単位モードで集計
///
/// - Monthly: 全レコードを月給換算で扱う（時給×160）
/// - Hourly:  全レコードを時給換算で扱う（月給/160）
/// - Auto:    多数派で自動判定（従来動作）
pub fn aggregate_records_with_mode(
    records: &[SurveyRecord],
    wage_mode: super::upload::WageMode,
) -> SurveyAggregation {
    use super::upload::WageMode;
    let forced_hourly = matches!(wage_mode, WageMode::Hourly);
    let forced_monthly = matches!(wage_mode, WageMode::Monthly);
    // forced_* は後段で is_hourly を上書きする際に使う
    let _ = (forced_hourly, forced_monthly);
    aggregate_records_core(records, wage_mode)
}

fn aggregate_records_core(
    records: &[SurveyRecord],
    wage_mode: super::upload::WageMode,
) -> SurveyAggregation {
    let total = records.len();
    if total == 0 {
        return SurveyAggregation::default();
    }

    let new_count = records.iter().filter(|r| r.is_new).count();

    // パース成功率
    let salary_ok = records
        .iter()
        .filter(|r| r.salary_parsed.min_value.is_some())
        .count();
    let location_ok = records
        .iter()
        .filter(|r| r.location_parsed.prefecture.is_some())
        .count();

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
    let dominant_municipality = muni_map.into_iter().max_by_key(|(_, c)| *c).map(|(m, _)| m);

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
        let emp = if r.employment_type.is_empty() {
            "不明".to_string()
        } else {
            r.employment_type.clone()
        };
        *emp_map.entry(emp).or_default() += 1;
    }
    let mut by_employment_type: Vec<(String, usize)> = emp_map.into_iter().collect();
    by_employment_type.sort_by(|a, b| b.1.cmp(&a.1));

    // タグ別（カンマ/スペース区切りで分解、危険URLプレフィックスをサニタイズ）
    use super::super::helpers::sanitize_tag_text;
    let mut tag_map: HashMap<String, usize> = HashMap::new();
    for r in records {
        if !r.tags_raw.is_empty() {
            for tag in r.tags_raw.split([',', '、', '/', '\t']) {
                let sanitized = sanitize_tag_text(tag);
                if !sanitized.is_empty() && sanitized.chars().count() <= 20 {
                    *tag_map.entry(sanitized).or_default() += 1;
                }
            }
        }
    }
    let mut by_tags: Vec<(String, usize)> = tag_map.into_iter().collect();
    by_tags.sort_by(|a, b| b.1.cmp(&a.1));
    by_tags.truncate(30); // 上位30タグ

    // タグ別給与集計（サニタイズ済みタグで集計）
    let mut tag_salary_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        if let Some(sal) = r.salary_parsed.unified_monthly {
            if sal > 0 && !r.tags_raw.is_empty() {
                for tag in r.tags_raw.split([',', '、', '/', '\t']) {
                    let sanitized = sanitize_tag_text(tag);
                    if !sanitized.is_empty() && sanitized.chars().count() <= 20 {
                        tag_salary_map.entry(sanitized).or_default().push(sal);
                    }
                }
            }
        }
    }

    // 給与統計
    let salary_values: Vec<i64> = records
        .iter()
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .collect();
    let enhanced_stats = enhanced_salary_statistics(&salary_values);

    // タグ別給与差分の計算
    let overall_mean = enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);
    let mut by_tag_salary: Vec<TagSalaryAgg> = tag_salary_map
        .into_iter()
        .filter(|(_, salaries)| salaries.len() >= 3) // 3件以上のタグのみ
        .map(|(tag, salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let diff_from_avg = avg_salary - overall_mean;
            let diff_percent = if overall_mean > 0 {
                diff_from_avg as f64 / overall_mean as f64 * 100.0
            } else {
                0.0
            };
            TagSalaryAgg {
                tag,
                count,
                avg_salary,
                diff_from_avg,
                diff_percent,
            }
        })
        .collect();
    by_tag_salary.sort_by(|a, b| b.diff_from_avg.cmp(&a.diff_from_avg));
    by_tag_salary.truncate(20);

    // 下限/上限給与（レポート用、月給換算）
    // 時給データは160h倍して月給相当に変換、月給はそのまま
    use super::salary_parser::SalaryType;
    let salary_min_values: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            let v = r.salary_parsed.min_value?;
            match r.salary_parsed.salary_type {
                SalaryType::Hourly => Some(v * 160),
                SalaryType::Daily => Some(v * 20), // 月20日想定
                SalaryType::Annual => Some(v / 12),
                _ => Some(v),
            }
        })
        .filter(|&v| v >= 50_000) // 5万円未満は異常値として除外
        .collect();
    let salary_max_values: Vec<i64> = records
        .iter()
        .filter_map(|r| {
            let v = r.salary_parsed.max_value?;
            match r.salary_parsed.salary_type {
                SalaryType::Hourly => Some(v * 160),
                SalaryType::Daily => Some(v * 20),
                SalaryType::Annual => Some(v / 12),
                _ => Some(v),
            }
        })
        .filter(|&v| v >= 50_000)
        .collect();

    // 企業別集計
    // count/avg/median の意味論一致のため、給与情報（unified_monthly > 0）があるレコードのみ集計。
    // これにより count == 集計対象件数 となり、avg/median の計算母集団と一致する。
    // 表示上は「給与情報のある求人数」として扱う。
    let mut company_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        if !r.company_name.is_empty() {
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    company_map
                        .entry(r.company_name.clone())
                        .or_default()
                        .push(sal);
                }
            }
        }
    }
    let mut by_company: Vec<CompanyAgg> = company_map
        .into_iter()
        .map(|(name, salaries)| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let median_salary = median_of(&salaries);
            CompanyAgg {
                name,
                count,
                avg_salary,
                median_salary,
            }
        })
        .collect();
    by_company.sort_by(|a, b| b.count.cmp(&a.count));

    // 雇用形態別給与
    let mut emp_salary_map: HashMap<String, Vec<i64>> = HashMap::new();
    for r in records {
        let emp = if r.employment_type.is_empty() {
            "不明".to_string()
        } else {
            r.employment_type.clone()
        };
        if let Some(sal) = r.salary_parsed.unified_monthly {
            emp_salary_map.entry(emp).or_default().push(sal);
        }
    }
    let mut by_emp_type_salary: Vec<EmpTypeSalary> = emp_salary_map
        .into_iter()
        .map(|(emp_type, salaries)| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let median_salary = median_of(&salaries);
            EmpTypeSalary {
                emp_type,
                count,
                avg_salary,
                median_salary,
            }
        })
        .collect();
    by_emp_type_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    // 都道府県別給与集計（最低賃金比較用）
    let mut pref_salary_map: HashMap<String, (Vec<i64>, Vec<i64>)> = HashMap::new(); // (unified, min_values)
    for r in records {
        if let Some(pref) = &r.location_parsed.prefecture {
            let entry = pref_salary_map.entry(pref.clone()).or_default();
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    entry.0.push(sal);
                }
            }
            if let Some(min_sal) = r.salary_parsed.min_value {
                if min_sal > 0 {
                    entry.1.push(min_sal);
                }
            }
        }
    }
    let mut by_prefecture_salary: Vec<PrefectureSalaryAgg> = pref_salary_map
        .into_iter()
        .map(|(name, (salaries, min_salaries))| {
            let count = salaries.len();
            let avg_salary = if salaries.is_empty() {
                0
            } else {
                salaries.iter().sum::<i64>() / count as i64
            };
            let avg_min_salary = if min_salaries.is_empty() {
                0
            } else {
                min_salaries.iter().sum::<i64>() / min_salaries.len() as i64
            };
            PrefectureSalaryAgg {
                name,
                count,
                avg_salary,
                avg_min_salary,
            }
        })
        .collect();
    by_prefecture_salary.sort_by(|a, b| b.count.cmp(&a.count));

    // 時給モード判定
    // 時給レコードが過半数（半数超）の場合 true。
    // 境界値（同数、例: 5-5）は整数割り算のため strict 比較で false となり、
    // Monthly として扱う（より保守的な挙動）。
    let hourly_count = records
        .iter()
        .filter(|r| r.salary_parsed.salary_type == super::salary_parser::SalaryType::Hourly)
        .count();
    let total_with_salary = records
        .iter()
        .filter(|r| r.salary_parsed.min_value.is_some())
        .count();
    use super::upload::WageMode;
    let is_hourly = match wage_mode {
        WageMode::Hourly => true,
        WageMode::Monthly => false,
        WageMode::Auto => total_with_salary > 0 && hourly_count > total_with_salary / 2,
    };

    // 散布図データ（下限 vs 上限）
    let scatter_min_max: Vec<ScatterPoint> = records
        .iter()
        .filter_map(|r| {
            let min = r.salary_parsed.min_value?;
            let max = r.salary_parsed.max_value?;
            if min > 0 && max > 0 && max >= min {
                Some(ScatterPoint { x: min, y: max })
            } else {
                None
            }
        })
        .collect();
    let regression_min_max = linear_regression_points(&scatter_min_max);

    // 市区町村別給与集計
    let mut muni_salary_map: HashMap<(String, String), Vec<i64>> = HashMap::new();
    for r in records {
        if let (Some(pref), Some(muni)) = (
            &r.location_parsed.prefecture,
            &r.location_parsed.municipality,
        ) {
            if let Some(sal) = r.salary_parsed.unified_monthly {
                if sal > 0 {
                    muni_salary_map
                        .entry((pref.clone(), muni.clone()))
                        .or_default()
                        .push(sal);
                }
            }
        }
    }
    let mut by_municipality_salary: Vec<MunicipalitySalaryAgg> = muni_salary_map
        .into_iter()
        .map(|((pref, name), salaries)| {
            let count = salaries.len();
            let avg_salary = salaries.iter().sum::<i64>() / count as i64;
            let median_salary = median_of(&salaries);
            MunicipalitySalaryAgg {
                name,
                prefecture: pref,
                count,
                avg_salary,
                median_salary,
            }
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
    if n < 3 {
        return None;
    }
    let n_f = n as f64;
    let sum_x: f64 = points.iter().map(|p| p.x as f64).sum();
    let sum_y: f64 = points.iter().map(|p| p.y as f64).sum();
    let sum_xy: f64 = points.iter().map(|p| p.x as f64 * p.y as f64).sum();
    let sum_x2: f64 = points.iter().map(|p| (p.x as f64).powi(2)).sum();

    let denom = n_f * sum_x2 - sum_x.powi(2);
    if denom.abs() < 1e-10 {
        return None;
    }

    let slope = (n_f * sum_xy - sum_x * sum_y) / denom;
    let intercept = (sum_y - slope * sum_x) / n_f;

    // R²計算
    let mean_y = sum_y / n_f;
    let ss_tot: f64 = points.iter().map(|p| (p.y as f64 - mean_y).powi(2)).sum();
    let ss_res: f64 = points
        .iter()
        .map(|p| {
            let pred = slope * p.x as f64 + intercept;
            (p.y as f64 - pred).powi(2)
        })
        .sum();
    // ss_tot=0（全yが同値、ゼロ分散）の場合、統計的には R² は定義されない。
    // 本実装では 0.0 を返す（ゼロ分散データは「相関なし」として扱う保守的挙動）。
    let r_squared = if ss_tot > 0.0 {
        1.0 - ss_res / ss_tot
    } else {
        0.0
    };

    Some(RegressionResult {
        slope,
        intercept,
        r_squared,
    })
}

#[cfg(test)]
mod tests {
    use super::super::location_parser::ParsedLocation;
    use super::super::salary_parser::{ParsedSalary, SalaryType};
    use super::super::upload::{CsvSource, SurveyRecord};
    use super::*;

    // ======== テストヘルパー ========

    fn empty_salary() -> ParsedSalary {
        ParsedSalary {
            original_text: String::new(),
            salary_type: SalaryType::Monthly,
            min_value: None,
            max_value: None,
            has_range: false,
            unified_monthly: None,
            unified_annual: None,
            range_category: None,
            confidence: 0.0,
        }
    }

    fn empty_location() -> ParsedLocation {
        ParsedLocation {
            original_text: String::new(),
            prefecture: None,
            municipality: None,
            region_block: None,
            city_type: None,
            confidence: 0.0,
            method: "empty".to_string(),
        }
    }

    /// テスト用SurveyRecord作成ヘルパー
    fn mock_record(
        company: &str,
        prefecture: Option<&str>,
        municipality: Option<&str>,
        salary_monthly: Option<i64>,
        salary_min: Option<i64>,
        salary_max: Option<i64>,
        salary_type: SalaryType,
        emp_type: &str,
        tags: &str,
    ) -> SurveyRecord {
        let mut sal = empty_salary();
        sal.salary_type = salary_type;
        sal.unified_monthly = salary_monthly;
        sal.min_value = salary_min;
        sal.max_value = salary_max;

        let mut loc = empty_location();
        loc.prefecture = prefecture.map(|s| s.to_string());
        loc.municipality = municipality.map(|s| s.to_string());

        SurveyRecord {
            row_index: 0,
            source: CsvSource::Unknown,
            job_title: String::new(),
            company_name: company.to_string(),
            location_raw: String::new(),
            salary_raw: String::new(),
            employment_type: emp_type.to_string(),
            tags_raw: tags.to_string(),
            url: None,
            is_new: false,
            description: String::new(),
            salary_parsed: sal,
            location_parsed: loc,
            annual_holidays: None,
        }
    }

    // ======== A. 線形回帰テスト ========

    #[test]
    fn test_linear_regression_known_points() {
        // y = 2x + 1 の5点
        let points = vec![
            ScatterPoint { x: 1, y: 3 },
            ScatterPoint { x: 2, y: 5 },
            ScatterPoint { x: 3, y: 7 },
            ScatterPoint { x: 4, y: 9 },
            ScatterPoint { x: 5, y: 11 },
        ];
        let result = linear_regression_points(&points).expect("5点あるのでSomeを返すはず");
        assert!((result.slope - 2.0).abs() < 0.01, "slope={}", result.slope);
        assert!(
            (result.intercept - 1.0).abs() < 0.01,
            "intercept={}",
            result.intercept
        );
        assert!(
            (result.r_squared - 1.0).abs() < 0.01,
            "r_squared={}",
            result.r_squared
        );
    }

    #[test]
    fn test_linear_regression_n_less_than_3() {
        let points = vec![ScatterPoint { x: 1, y: 2 }, ScatterPoint { x: 2, y: 4 }];
        assert!(
            linear_regression_points(&points).is_none(),
            "n<3ではNoneを返すべき"
        );
    }

    #[test]
    fn test_linear_regression_all_same_x() {
        // 垂直分布: denom = n*sum(x^2) - sum(x)^2 = 0
        let points = vec![
            ScatterPoint { x: 5, y: 10 },
            ScatterPoint { x: 5, y: 20 },
            ScatterPoint { x: 5, y: 30 },
        ];
        assert!(
            linear_regression_points(&points).is_none(),
            "denom≈0ではNoneを返すべき"
        );
    }

    #[test]
    fn test_linear_regression_r_squared_zero_ss_tot() {
        // 水平分布: 全点のyが同じ → ss_tot=0 → r_squared=0.0（現状動作）
        let points = vec![
            ScatterPoint { x: 1, y: 100 },
            ScatterPoint { x: 2, y: 100 },
            ScatterPoint { x: 3, y: 100 },
        ];
        let result = linear_regression_points(&points).expect("xは分散しているのでSome");
        // ss_tot=0（ゼロ分散）の場合、統計的には R² 未定義だが、
        // 本実装では 0.0 を返す仕様（「相関なし」として扱う保守的挙動、ドキュメント化済）。
        assert!(
            result.slope.abs() < 1e-9,
            "slope should be ~0, got {}",
            result.slope
        );
        assert!((result.intercept - 100.0).abs() < 1e-6);
        assert_eq!(result.r_squared, 0.0, "ss_tot=0時はr_squared=0.0を返す仕様");
    }

    #[test]
    fn test_linear_regression_points_struct_sanity() {
        // 大きな値でも正しくf64変換されて処理される
        let points = vec![
            ScatterPoint {
                x: 100_000,
                y: 200_000,
            },
            ScatterPoint {
                x: 150_000,
                y: 250_000,
            },
            ScatterPoint {
                x: 200_000,
                y: 300_000,
            },
            ScatterPoint {
                x: 250_000,
                y: 350_000,
            },
        ];
        let result = linear_regression_points(&points).expect("4点あればSome");
        // y = x + 100_000 → slope=1.0, intercept=100_000
        assert!((result.slope - 1.0).abs() < 0.01);
        assert!((result.intercept - 100_000.0).abs() < 1.0);
        assert!((result.r_squared - 1.0).abs() < 0.01);
    }

    // ======== B. 集計ロジックテスト ========

    #[test]
    fn test_aggregate_by_company_count_vs_valid() {
        // 企業A: 給与あり + 給与なし / 企業B: 給与あり
        let records = vec![
            mock_record(
                "企業A",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(280_000),
                Some(320_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "企業A",
                Some("東京都"),
                Some("千代田区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "企業B",
                Some("東京都"),
                Some("新宿区"),
                Some(400_000),
                Some(380_000),
                Some(420_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);

        // count/avg/median の意味論を一致させるため、給与情報のあるレコードのみ集計対象。
        // 企業A: unified_monthly=None のレコードはスキップ → salaries=[300_000]
        //   → count=1, avg=300_000, median=300_000
        let a = agg
            .by_company
            .iter()
            .find(|c| c.name == "企業A")
            .expect("企業A");
        assert_eq!(
            a.count, 1,
            "企業Aは給与情報のある1件のみ（Noneレコードは除外）"
        );
        assert_eq!(a.avg_salary, 300_000);
        assert_eq!(a.median_salary, 300_000);

        let b = agg
            .by_company
            .iter()
            .find(|c| c.name == "企業B")
            .expect("企業B");
        assert_eq!(b.count, 1);
        assert_eq!(b.avg_salary, 400_000);
    }

    #[test]
    fn test_aggregate_by_tag_salary_overall_mean_zero() {
        // 全レコードでunified_monthly=None → tag_salary_mapが populate されない
        let records = vec![
            mock_record(
                "X社",
                Some("東京都"),
                Some("千代田区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
            mock_record(
                "Y社",
                Some("東京都"),
                Some("新宿区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
            mock_record(
                "Z社",
                Some("東京都"),
                Some("渋谷区"),
                None,
                None,
                None,
                SalaryType::Monthly,
                "正社員",
                "タグA,タグB",
            ),
        ];
        let agg = aggregate_records(&records);
        // tag_salary は全給与Noneなので空（3件フィルタ以前に populate されない）
        assert!(
            agg.by_tag_salary.is_empty(),
            "全給与None時は by_tag_salary が空であること（巨大正値の diff_from_avg が出ないこと）"
        );
    }

    #[test]
    fn test_aggregate_is_hourly_detection_majority() {
        // 6 Hourly + 4 Monthly = 10件。hourly_count=6 > 10/2=5 → true
        let mut records = Vec::new();
        for _ in 0..6 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..4 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(agg.is_hourly, "時給6 vs 月給4 → is_hourly=true");
    }

    #[test]
    fn test_aggregate_is_hourly_detection_minority() {
        // 3 Hourly + 7 Monthly = 10件。hourly_count=3, 3>5=false
        let mut records = Vec::new();
        for _ in 0..3 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..7 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(!agg.is_hourly, "時給3 vs 月給7 → is_hourly=false");
    }

    #[test]
    fn test_aggregate_is_hourly_detection_boundary() {
        // 5 Hourly + 5 Monthly = 10件。hourly_count=5, 5>10/2=5 は strict比較で false
        let mut records = Vec::new();
        for _ in 0..5 {
            records.push(mock_record(
                "H",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(1200),
                Some(1500),
                SalaryType::Hourly,
                "パート",
                "",
            ));
        }
        for _ in 0..5 {
            records.push(mock_record(
                "M",
                Some("東京都"),
                Some("千代田区"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ));
        }
        let agg = aggregate_records(&records);
        assert!(
            !agg.is_hourly,
            "境界（5-5）: hourly_count > total/2 の strict 比較により false。\
             同数時は Monthly として扱う保守的仕様（ドキュメント化済）"
        );
    }

    #[test]
    fn test_aggregate_by_municipality_salary_median_even_count() {
        // 同一市区町村に4件: [100_000, 200_000, 300_000, 400_000]
        // sorted[4/2] = sorted[2] = 300_000 （現状: 偶数件でも上側要素を取る）
        let records = vec![
            mock_record(
                "A",
                Some("東京都"),
                Some("千代田区"),
                Some(100_000),
                Some(100_000),
                Some(100_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "B",
                Some("東京都"),
                Some("千代田区"),
                Some(200_000),
                Some(200_000),
                Some(200_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "C",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(300_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "D",
                Some("東京都"),
                Some("千代田区"),
                Some(400_000),
                Some(400_000),
                Some(400_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);
        let muni = agg
            .by_municipality_salary
            .iter()
            .find(|m| m.name == "千代田区" && m.prefecture == "東京都")
            .expect("千代田区");
        assert_eq!(muni.count, 4);
        assert_eq!(muni.avg_salary, 250_000);
        // 偶数件の中央値は中央2要素の平均: (sorted[1]+sorted[2])/2 = (200_000+300_000)/2 = 250_000
        // enhanced_salary_statistics と一貫した定義。
        assert_eq!(
            muni.median_salary, 250_000,
            "偶数件の中央値は中央2要素の平均"
        );
    }

    #[test]
    fn test_aggregate_by_prefecture_salary() {
        // 東京都: 2件、大阪府: 2件
        let records = vec![
            mock_record(
                "A",
                Some("東京都"),
                Some("千代田区"),
                Some(300_000),
                Some(280_000),
                Some(320_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "B",
                Some("東京都"),
                Some("新宿区"),
                Some(400_000),
                Some(380_000),
                Some(420_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "C",
                Some("大阪府"),
                Some("大阪市"),
                Some(250_000),
                Some(200_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
            mock_record(
                "D",
                Some("大阪府"),
                Some("堺市"),
                Some(270_000),
                Some(240_000),
                Some(300_000),
                SalaryType::Monthly,
                "正社員",
                "",
            ),
        ];
        let agg = aggregate_records(&records);

        let tokyo = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "東京都")
            .expect("東京都");
        assert_eq!(tokyo.count, 2);
        assert_eq!(tokyo.avg_salary, 350_000); // (300_000+400_000)/2
        assert_eq!(tokyo.avg_min_salary, 330_000); // (280_000+380_000)/2

        let osaka = agg
            .by_prefecture_salary
            .iter()
            .find(|p| p.name == "大阪府")
            .expect("大阪府");
        assert_eq!(osaka.count, 2);
        assert_eq!(osaka.avg_salary, 260_000); // (250_000+270_000)/2
        assert_eq!(osaka.avg_min_salary, 220_000); // (200_000+240_000)/2
    }
}
