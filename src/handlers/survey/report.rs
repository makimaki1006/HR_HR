//! 統合レポートJSON生成

use super::aggregator::SurveyAggregation;
use super::job_seeker::JobSeekerAnalysis;
use serde_json::{json, Value};

/// 統合レポートJSON構築
pub fn build_survey_report(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    pref: &str,
    muni: &str,
    hw_insights: &[super::super::insight::helpers::Insight],
) -> Value {
    let location = if !muni.is_empty() {
        format!("{} {}", pref, muni)
    } else if !pref.is_empty() {
        pref.to_string()
    } else {
        "全国".to_string()
    };

    json!({
        "title": "媒体分析 統合レポート",
        "subtitle": format!("{} | {}", location, chrono::Local::now().format("%Y年%m月")),
        "generated_at": chrono::Local::now().to_rfc3339(),
        "survey": {
            "total_records": agg.total_count,
            "new_count": agg.new_count,
            "parse_rate": {
                "salary": agg.salary_parse_rate,
                "location": agg.location_parse_rate,
            },
            "dominant_region": {
                "prefecture": agg.dominant_prefecture,
                "municipality": agg.dominant_municipality,
            },
            "salary_stats": agg.enhanced_stats.as_ref().map(|s| json!({
                "mean": s.mean,
                "median": s.median,
                "min": s.min,
                "max": s.max,
                "std_dev": s.std_dev,
                "count": s.count,
                "reliability": s.reliability,
                "bootstrap_ci_95": s.bootstrap_ci.as_ref().map(|ci| json!({
                    "lower": ci.lower,
                    "upper": ci.upper,
                })),
                "trimmed_mean_10": s.trimmed_mean.as_ref().map(|tm| tm.trimmed_mean),
            })),
            "distributions": {
                "by_prefecture": agg.by_prefecture.iter().take(10)
                    .map(|(k, v)| json!({"name": k, "count": v}))
                    .collect::<Vec<_>>(),
                "by_salary_range": agg.by_salary_range.iter()
                    .map(|(k, v)| json!({"range": k, "count": v}))
                    .collect::<Vec<_>>(),
                "by_employment_type": agg.by_employment_type.iter()
                    .map(|(k, v)| json!({"type": k, "count": v}))
                    .collect::<Vec<_>>(),
                "top_tags": agg.by_tags.iter().take(15)
                    .map(|(k, v)| json!({"tag": k, "count": v}))
                    .collect::<Vec<_>>(),
            },
            "job_seeker": {
                "expected_salary": seeker.expected_salary,
                "salary_range_perception": seeker.salary_range_perception.as_ref().map(|p| json!({
                    "avg_lower": p.avg_lower,
                    "avg_upper": p.avg_upper,
                    "expected_point": p.expected_point,
                    "avg_range_width": p.avg_range_width,
                })),
                "inexperience_gap": seeker.inexperience_analysis.as_ref()
                    .and_then(|a| a.salary_gap),
                "new_listings_premium": seeker.new_listings_premium,
            },
        },
        "insights_count": hw_insights.len(),
        "insights": hw_insights.iter().take(10).map(|i| json!({
            "id": i.id,
            "severity": i.severity.label(),
            "title": i.title,
            "body": i.body,
        })).collect::<Vec<_>>(),
    })
}
