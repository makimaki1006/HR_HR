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
        // 図番号 (図 11-1): 求職者心理章 (Round 12 リナンバ、雇用形態章の 4-1/4-2 との重複解消)
        html.push_str(&render_figure_number(
            11,
            1,
            "求人側の給与レンジ幅と求職者期待値の比較",
        ));

        // Round 12 (2026-05-12) K10 修正:
        // 旧実装は stats-grid + three-column の数字表示のみで chart 不在 (「図」を名乗っているが視覚化なし)。
        // 横棒グラフ (平均下限 / 求職者期待値 / 平均上限 = 下限 + レンジ幅) を追加し、
        // レンジ幅分類のドーナツも併設する。
        let to_man = |v: i64| (v as f64) / 10_000.0;
        let avg_upper = perception
            .avg_lower
            .saturating_add(perception.avg_range_width);
        let range_chart = json!({
            "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
            "grid": {"left": "20%", "right": "12%", "top": 16, "bottom": 32, "containLabel": true},
            "xAxis": {
                "type": "value",
                "name": "月給 (万円)",
                "nameLocation": "middle",
                "nameGap": 24,
                "axisLabel": {"fontSize": 10}
            },
            "yAxis": {
                "type": "category",
                "data": ["平均下限", "求職者期待値", "平均上限 (下限+幅)"],
                "axisLabel": {"fontSize": 10}
            },
            "series": [{
                "type": "bar",
                "data": [
                    {"value": to_man(perception.avg_lower), "itemStyle": {"color": "#3b82f6"}},
                    {"value": to_man(perception.expected_point), "itemStyle": {"color": "#22c55e"}},
                    {"value": to_man(avg_upper), "itemStyle": {"color": "#f59e0b"}}
                ],
                "label": {"show": true, "position": "right", "formatter": "{c} 万円", "fontSize": 10}
            }]
        });
        html.push_str(&render_echart_div(&range_chart.to_string(), 200));

        // 数値 KPI も併記 (chart からの読み取り補助)
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

        // レンジ幅分類: ドーナツチャート化
        let range_donut = json!({
            "tooltip": {"trigger": "item", "formatter": "{b}: {c} 件 ({d}%)"},
            "legend": {"bottom": 0, "textStyle": {"fontSize": 10}},
            "series": [{
                "type": "pie",
                "radius": ["35%", "60%"],
                "center": ["50%", "45%"],
                "label": {"show": true, "formatter": "{b}\n{c} 件 ({d}%)", "fontSize": 9},
                "data": [
                    {"value": perception.narrow_count, "name": "狭い (<5万)", "itemStyle": {"color": "#a7f3d0"}},
                    {"value": perception.medium_count, "name": "中程度 (5〜10万)", "itemStyle": {"color": "#fde68a"}},
                    {"value": perception.wide_count, "name": "広い (>10万)", "itemStyle": {"color": "#fbcfe8"}}
                ]
            }]
        });
        html.push_str(&render_echart_div(&range_donut.to_string(), 220));

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
        // 図番号 (図 11-2): 求職者心理章 (Round 12 リナンバ)
        html.push_str(&render_figure_number(
            11,
            2,
            "経験者求人 vs 未経験可求人 平均給与比較",
        ));

        // Round 17 (2026-05-13): ECharts → SSR SVG bar (print emulate 対応)
        // 旧実装は None → 0.0 で未経験バーが消失 (B-P1 / agent 報告 #5)。
        // SSR SVG では None の系列は items から完全に除外 (chart 範囲を歪めない)。
        let mut items: Vec<(String, f64)> = vec![];
        if let Some(exp) = inexp.experience_avg_salary {
            items.push((
                format!("経験者求人 ({}件)", inexp.experience_count),
                exp as f64 / 10_000.0,
            ));
        }
        if let Some(inx) = inexp.inexperience_avg_salary {
            items.push((
                format!("未経験可求人 ({}件)", inexp.inexperience_count),
                inx as f64 / 10_000.0,
            ));
        }
        if items.is_empty() {
            html.push_str(
                "<p class=\"data-empty\">経験者・未経験可求人いずれも平均給与データなし</p>\n",
            );
        } else {
            html.push_str(&build_vbar_svg(&items, "#1e40af", "万"));
        }

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
        // 図番号 (図 11-3): 求職者心理章 (Round 12 リナンバ、章内連続性のため 11-1/11-2 と統一)
        html.push_str(&render_figure_number(
            11,
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
