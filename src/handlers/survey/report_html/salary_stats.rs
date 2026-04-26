//! 分割: report_html/salary_stats.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

pub(super) fn render_section_salary_stats(
    html: &mut String,
    agg: &SurveyAggregation,
    salary_min_values: &[i64],
    salary_max_values: &[i64],
) {
    let stats = match &agg.enhanced_stats {
        Some(s) => s,
        None => return, // 給与データなし → セクションスキップ
    };

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>給与分布 - 統計情報</h2>\n");

    // 3カード: 平均、中央値、給与範囲
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均月給", &format_man_yen(stats.mean));
    render_stat_box(html, "中央値", &format_man_yen(stats.median));
    render_stat_box(
        html,
        "給与範囲",
        &format!(
            "{} 〜 {}",
            format_man_yen(stats.min),
            format_man_yen(stats.max)
        ),
    );
    html.push_str("</div>\n");

    // 信頼区間・四分位がある場合
    if let Some(ci) = &stats.bootstrap_ci {
        html.push_str(&format!(
            "<p class=\"note\">95%信頼区間: {} 〜 {} (Bootstrap法, n={})</p>\n",
            format_man_yen(ci.lower),
            format_man_yen(ci.upper),
            ci.sample_size
        ));
    }

    // 下限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_min_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<h3>下限給与の分布（20,000円刻み）</h3>\n");
        let (labels, values, _b) = build_salary_histogram(salary_min_values, 20_000);
        let mode_min_20k = compute_mode(salary_min_values, 20_000);
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_20k,
            20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>下限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_min_values, 5_000);
        let mode_min_5k = compute_mode(salary_min_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_5k,
            5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
    }

    // 上限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_max_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<h3>上限給与の分布（20,000円刻み）</h3>\n");
        let (labels, values, _b) = build_salary_histogram(salary_max_values, 20_000);
        let mode_max_20k = compute_mode(salary_max_values, 20_000);
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_20k,
            20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>上限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_max_values, 5_000);
        let mode_max_5k = compute_mode(salary_max_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_5k,
            5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
    }

    html.push_str("</div>\n");
}
