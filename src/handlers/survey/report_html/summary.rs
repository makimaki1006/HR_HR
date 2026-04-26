//! 分割: report_html/summary.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;


pub(super) fn render_section_summary(html: &mut String, agg: &SurveyAggregation) {
    let salary_label = if agg.is_hourly {
        "平均時給"
    } else {
        "平均月給"
    };
    let salary_unit = if agg.is_hourly { "円" } else { "万円" };

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>サマリー</h2>\n");

    // KPIカード 2x2
    let avg_salary_display = agg
        .enhanced_stats
        .as_ref()
        .map(|s| {
            if agg.is_hourly {
                format_number(s.mean)
            } else {
                format!("{:.1}", s.mean as f64 / 10_000.0)
            }
        })
        .unwrap_or_else(|| "-".to_string());

    // 正社員率の計算
    let fulltime_count = agg
        .by_employment_type
        .iter()
        .filter(|(t, _)| t.contains("正社員") || t.contains("正職員"))
        .map(|(_, c)| c)
        .sum::<usize>();
    let fulltime_rate = if agg.total_count > 0 {
        format!(
            "{:.1}%",
            fulltime_count as f64 / agg.total_count as f64 * 100.0
        )
    } else {
        "-".to_string()
    };

    // 2026-04-24 ユーザー指摘反映:
    //   - 「掲載企業数」KPI 削除（CSV は任意スクレイピング件数なので母集団を示さず誤解）
    //   - 「正社員率」→「CSV内 正社員割合」: 「安定雇用が多い市場」表現は不正確なので削除
    //   - 新着率は CSV 側に新着列がある場合のみ表示（無ければ KPI 自体を省略）
    let has_new_rate = agg.new_count > 0;

    html.push_str("<div class=\"summary-grid\">\n");
    render_summary_card(
        html,
        "CSV上の求人件数",
        &format_number(agg.total_count as i64),
        "件",
    );
    render_summary_card(html, salary_label, &avg_salary_display, salary_unit);
    render_summary_card(html, "CSV内 正社員割合", &fulltime_rate, "");
    if has_new_rate {
        let nr = format!(
            "{:.1}%",
            agg.new_count as f64 / agg.total_count.max(1) as f64 * 100.0
        );
        render_summary_card(html, "新着率", &nr, "");
    }
    html.push_str("</div>\n");

    // 読み方ガイド
    let salary_guide = if agg.is_hourly {
        "CSV 行の時給平均値。月給・年俸は時給へ換算。"
    } else {
        "CSV 行の月給換算平均（時給・年俸は月給へ統一計算）。"
    };
    html.push_str("<div class=\"guide-grid\">\n");
    render_guide_item(
        html,
        "CSV上の求人件数",
        "アップロードされた CSV 行数。CSV スクレイピング範囲に依存するため市場全体の指標ではありません。",
    );
    render_guide_item(html, salary_label, salary_guide);
    render_guide_item(
        html,
        "CSV内 正社員割合",
        "CSV 内で雇用形態「正社員・正職員」の行が占める比率。ソース媒体の収集方針により値は変動します。",
    );
    if has_new_rate {
        render_guide_item(
            html,
            "新着率",
            "CSV 行のうち「新着」フラグが付与された比率。",
        );
    }
    html.push_str("</div>\n");

    html.push_str("</div>\n");
}
