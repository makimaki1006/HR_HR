//! 分割: report_html/seeker.rs (物理移動・内容変更なし)

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

pub(super) fn render_section_job_seeker(html: &mut String, seeker: &JobSeekerAnalysis) {
    if seeker.total_analyzed == 0 {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>第4章 求職者心理分析</h2>\n");
    // 章冒頭の「相関≠因果」凡例（UI-3）
    html.push_str(
        "<div class=\"report-banner-gray\" role=\"note\">\
         \u{1F50D} <strong>本章の解釈ガイド</strong>: \
         本セクションでは「給与レンジ」「未経験可」「新着求人」と給与水準の関連を示します。\
         観測されたパターンは<strong>相関の傾向</strong>であり、因果関係を主張するものではありません。\
         募集媒体のバイアス・再掲載・繁忙期等の要因も含まれます。</div>\n",
    );

    // 給与レンジ認知
    if let Some(perception) = &seeker.salary_range_perception {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>給与レンジ認知</h3>\n");
        // 図番号 (図 4-1)
        html.push_str(&render_figure_number(
            4,
            1,
            "求人側の給与レンジ幅と求職者期待値の比較",
        ));

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

        html.push_str(&render_reading_callout(
            "求人側の給与「下限」と「上限」の幅が広いほど、求職者は実態として下限〜下から1/3の水準を見積もる傾向があります（媒体記載の上限値は採用後の昇給上限を含む場合があるため）。",
        ));
        html.push_str("<p class=\"note\">※ 観測された関連性であり、因果関係を主張するものではありません。</p>\n");
        html.push_str("</div>\n");
    }

    // 未経験ペナルティ
    if let Some(inexp) = &seeker.inexperience_analysis {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>未経験ペナルティ</h3>\n");
        // 図番号 (図 4-2)
        html.push_str(&render_figure_number(
            4,
            2,
            "経験者求人 vs 未経験可求人 平均給与比較",
        ));

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
        // 図番号 (図 4-3)
        html.push_str(&render_figure_number(
            4,
            3,
            "新着求人 vs 既存求人 給与水準比較",
        ));
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
        // feedback_correlation_not_causation.md 準拠: 因果断定（「上昇傾向」）を避け、相関の観測表現に修正
        html.push_str("<p class=\"note\">※ 新着求人と既存求人の給与水準の間に正の関連が観測される場合があります。\
            ただし再掲載・採用失敗続き・繁忙期等の要因も含まれるため、給与の時系列的な上昇を断定するものではなく、因果関係を主張するものでもありません。</p>\n");
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");
}
