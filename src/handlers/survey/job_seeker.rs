//! 求職者心理分析（GAS JobSeekerAnalysis.js移植）
//! 求職者の期待給与モデル、未経験タグ分析、市場暗黙レート

use serde::Serialize;
use super::upload::SurveyRecord;

#[derive(Debug, Clone, Serialize, Default)]
pub struct JobSeekerAnalysis {
    pub expected_salary: Option<i64>,
    pub salary_range_perception: Option<SalaryRangePerception>,
    pub inexperience_analysis: Option<InexperienceAnalysis>,
    pub new_listings_premium: Option<i64>,
    pub total_analyzed: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct SalaryRangePerception {
    pub avg_range_width: i64,
    pub avg_lower: i64,
    pub avg_upper: i64,
    pub expected_point: i64,  // lower + (upper - lower) * 0.33
    pub narrow_count: usize,  // レンジ幅 < 5万
    pub medium_count: usize,  // 5万〜10万
    pub wide_count: usize,    // > 10万
}

#[derive(Debug, Clone, Serialize)]
pub struct InexperienceAnalysis {
    pub inexperience_count: usize,
    pub experience_count: usize,
    pub inexperience_avg_salary: Option<i64>,
    pub experience_avg_salary: Option<i64>,
    pub salary_gap: Option<i64>,  // 経験者 - 未経験者
}

/// 求職者心理分析を実行
pub fn analyze_job_seeker(records: &[SurveyRecord]) -> JobSeekerAnalysis {
    if records.is_empty() {
        return JobSeekerAnalysis::default();
    }

    let perception = analyze_salary_range_perception(records);
    let inexperience = analyze_inexperience_tag(records);
    let new_premium = analyze_new_listings_premium(records);

    let expected_salary = perception.as_ref().map(|p| p.expected_point);

    JobSeekerAnalysis {
        expected_salary,
        salary_range_perception: perception,
        inexperience_analysis: inexperience,
        new_listings_premium: new_premium,
        total_analyzed: records.len(),
    }
}

/// 給与レンジ知覚分析（統一月給ベース）
/// 求職者は給与レンジの下限〜1/3地点を期待値とする
fn analyze_salary_range_perception(records: &[SurveyRecord]) -> Option<SalaryRangePerception> {
    // unified_monthlyベースでレンジを取得（時給・年俸を月給換算済み）
    let ranges: Vec<(i64, i64)> = records.iter()
        .filter_map(|r| {
            // min/maxが両方ある場合のみレンジ分析対象
            let min_raw = r.salary_parsed.min_value?;
            let max_raw = r.salary_parsed.max_value?;
            if min_raw <= 0 || max_raw <= min_raw { return None; }
            // unified_monthlyで統一（年俸÷12、時給×173.8等の変換済み値を使用）
            let monthly = r.salary_parsed.unified_monthly?;
            if monthly <= 0 { return None; }
            // レンジ幅も月給換算で計算
            let ratio = if min_raw > 0 { monthly as f64 / ((min_raw + max_raw) as f64 / 2.0) } else { 1.0 };
            let min_monthly = (min_raw as f64 * ratio) as i64;
            let max_monthly = (max_raw as f64 * ratio) as i64;
            Some((min_monthly, max_monthly))
        })
        .collect();

    if ranges.is_empty() { return None; }

    let n = ranges.len() as i64;
    let avg_lower = ranges.iter().map(|(l, _)| l).sum::<i64>() / n;
    let avg_upper = ranges.iter().map(|(_, u)| u).sum::<i64>() / n;
    let avg_width = ranges.iter().map(|(l, u)| u - l).sum::<i64>() / n;
    // 求職者期待値 = 下限 + レンジ × 0.33
    let expected = avg_lower + ((avg_upper - avg_lower) as f64 * 0.33) as i64;

    // レンジ幅の分類（月給ベース）
    let narrow = ranges.iter().filter(|(l, u)| u - l < 50_000).count();
    let wide = ranges.iter().filter(|(l, u)| u - l > 100_000).count();
    let medium = ranges.len() - narrow - wide;

    Some(SalaryRangePerception {
        avg_range_width: avg_width,
        avg_lower,
        avg_upper,
        expected_point: expected,
        narrow_count: narrow,
        medium_count: medium,
        wide_count: wide,
    })
}

/// 未経験タグ分析
fn analyze_inexperience_tag(records: &[SurveyRecord]) -> Option<InexperienceAnalysis> {
    let inexperience_keywords = ["未経験", "未経験可", "未経験OK", "経験不問", "初心者"];

    let mut inexp_salaries = Vec::new();
    let mut exp_salaries = Vec::new();

    for r in records {
        let monthly = match r.salary_parsed.unified_monthly {
            Some(v) if v > 0 => v,
            _ => continue,
        };
        let is_inexperience = inexperience_keywords.iter()
            .any(|kw| r.tags_raw.contains(kw) || r.job_title.contains(kw));
        if is_inexperience {
            inexp_salaries.push(monthly);
        } else {
            exp_salaries.push(monthly);
        }
    }

    if inexp_salaries.is_empty() && exp_salaries.is_empty() { return None; }

    let inexp_avg = if inexp_salaries.is_empty() { None }
        else { Some(inexp_salaries.iter().sum::<i64>() / inexp_salaries.len() as i64) };
    let exp_avg = if exp_salaries.is_empty() { None }
        else { Some(exp_salaries.iter().sum::<i64>() / exp_salaries.len() as i64) };

    let gap = match (exp_avg, inexp_avg) {
        (Some(e), Some(i)) => Some(e - i),
        _ => None,
    };

    Some(InexperienceAnalysis {
        inexperience_count: inexp_salaries.len(),
        experience_count: exp_salaries.len(),
        inexperience_avg_salary: inexp_avg,
        experience_avg_salary: exp_avg,
        salary_gap: gap,
    })
}

/// 新着求人のプレミアム分析
fn analyze_new_listings_premium(records: &[SurveyRecord]) -> Option<i64> {
    let new_salaries: Vec<i64> = records.iter()
        .filter(|r| r.is_new)
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .filter(|&v| v > 0)
        .collect();
    let old_salaries: Vec<i64> = records.iter()
        .filter(|r| !r.is_new)
        .filter_map(|r| r.salary_parsed.unified_monthly)
        .filter(|&v| v > 0)
        .collect();

    if new_salaries.is_empty() || old_salaries.is_empty() { return None; }

    let new_avg = new_salaries.iter().sum::<i64>() / new_salaries.len() as i64;
    let old_avg = old_salaries.iter().sum::<i64>() / old_salaries.len() as i64;
    Some(new_avg - old_avg)
}
