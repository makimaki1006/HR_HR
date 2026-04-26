//! 分割: report_html/seeker.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;


pub(super) fn render_section_job_seeker(html: &mut String, seeker: &JobSeekerAnalysis) {
    if seeker.total_analyzed == 0 {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>求職者心理分析</h2>\n");

    // 給与レンジ認知
    if let Some(perception) = &seeker.salary_range_perception {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>給与レンジ認知</h3>\n");

        html.push_str("<div class=\"stats-grid\">\n");
        render_stat_box(
            html,
            "平均レンジ幅",
            &format_man_yen(perception.avg_range_width),
        );
        render_stat_box(html, "平均下限", &format_man_yen(perception.avg_lower));
        render_stat_box(
            html,
            "求職者期待値",
            &format_man_yen(perception.expected_point),
        );
        html.push_str("</div>\n");

        // レンジ幅分類
        html.push_str("<div class=\"three-column\">\n");
        render_range_type_box(html, "狭い (<5万)", perception.narrow_count, "#e8f5e9");
        render_range_type_box(html, "中程度 (5〜10万)", perception.medium_count, "#fff8e1");
        render_range_type_box(html, "広い (>10万)", perception.wide_count, "#fce4ec");
        html.push_str("</div>\n");

        html.push_str("<p class=\"note\">※ 求職者は給与レンジの下限〜下から1/3の水準を期待する傾向があります。</p>\n");
        html.push_str("</div>\n");
    }

    // 未経験ペナルティ
    if let Some(inexp) = &seeker.inexperience_analysis {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>未経験ペナルティ</h3>\n");

        html.push_str("<div class=\"two-column\">\n");

        // 経験者
        html.push_str("<div class=\"stat-box\">\n");
        html.push_str(&format!(
            "<div class=\"label\">経験者求人 ({}件)</div>\n",
            format_number(inexp.experience_count as i64)
        ));
        if let Some(avg) = inexp.experience_avg_salary {
            html.push_str(&format!(
                "<div class=\"value\">{}</div>\n",
                format_man_yen(avg)
            ));
        } else {
            html.push_str("<div class=\"value\">-</div>\n");
        }
        html.push_str("</div>\n");

        // 未経験者
        html.push_str("<div class=\"stat-box\">\n");
        html.push_str(&format!(
            "<div class=\"label\">未経験可求人 ({}件)</div>\n",
            format_number(inexp.inexperience_count as i64)
        ));
        if let Some(avg) = inexp.inexperience_avg_salary {
            html.push_str(&format!(
                "<div class=\"value\">{}</div>\n",
                format_man_yen(avg)
            ));
        } else {
            html.push_str("<div class=\"value\">-</div>\n");
        }
        html.push_str("</div>\n");

        html.push_str("</div>\n");

        // 給与差
        if let Some(gap) = inexp.salary_gap {
            let class = if gap > 0 { "negative" } else { "positive" };
            html.push_str(&format!(
                "<div class=\"highlight-box\">経験者と未経験者の給与差: <span class=\"{}\">{}</span></div>\n",
                class,
                format_man_yen(gap),
            ));
        }

        html.push_str("</div>\n");
    }

    // 新着プレミアム
    if let Some(premium) = seeker.new_listings_premium {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>新着プレミアム</h3>\n");
        let (class, sign) = if premium > 0 {
            ("positive", "+")
        } else if premium < 0 {
            ("negative", "")
        } else {
            ("", "")
        };
        html.push_str(&format!(
            "<div class=\"highlight-box\">新着求人 vs 既存求人の給与差: <span class=\"{}\">{}{}</span></div>\n",
            class,
            sign,
            format_man_yen(premium),
        ));
        html.push_str("<p class=\"note\">※ 新着求人は市場の最新トレンドを反映しています。プラスなら給与水準が上昇傾向です。</p>\n");
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");
}
