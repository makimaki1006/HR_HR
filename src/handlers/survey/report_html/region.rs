//! 分割: report_html/region.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;


pub(super) fn render_section_region(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（都道府県）</h2>\n");
    // So What 行: 件数の多い都道府県と割合を 1 行で提示
    if let Some((top_pref, top_count)) = agg.by_prefecture.first() {
        let pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多いのは「{}」で全体の {:.1}%（件数の多い順に整理）。</p>\n",
            escape_html(top_pref),
            pct
        ));
    }
    html.push_str(
        "<p class=\"section-xref\">関連: Section 7（市区町村）/ Section 8（最低賃金）</p>\n",
    );

    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr></thead>\n<tbody>\n");
    let total = agg.total_count.max(1);
    for (i, (pref, count)) in agg.by_prefecture.iter().take(10).enumerate() {
        let pct = *count as f64 / total as f64 * 100.0;
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            escape_html(pref),
            format_number(*count as i64),
            pct,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 残りの都道府県数を注記
    if agg.by_prefecture.len() > 10 {
        html.push_str(&format!(
            "<p class=\"note\">※ 他{}都道府県のデータは省略しています</p>\n",
            agg.by_prefecture.len() - 10
        ));
    }

    html.push_str("</div>\n");
}

pub(super) fn render_section_municipality_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_municipality_salary.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（市区町村）</h2>\n");
    // So What: 件数の多い市区町村の給与水準が最も高い先
    if let Some(top_hi_salary) = agg
        .by_municipality_salary
        .iter()
        .take(15)
        .max_by_key(|m| m.avg_salary)
    {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数の多い 15 市区町村のうち、平均月給が最も高いのは\
             「{} {}」で {}（同名異県を避けるため都道府県併記）。</p>\n",
            escape_html(&top_hi_salary.prefecture),
            escape_html(&top_hi_salary.name),
            escape_html(&format_man_yen(top_hi_salary.avg_salary))
        ));
    }
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>掲載件数の多い市区町村の給与水準を比較。\
        同じ都道府県内でも市区町村により給与差があります。\
    </p>\n",
    );

    html.push_str(
        "<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n",
    );
    for (i, m) in agg.by_municipality_salary.iter().take(15).enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td style=\"font-size:10px;color:#666\">{}</td>\
             <td class=\"num\">{}件</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&m.name),
            escape_html(&m.prefecture),
            m.count,
            format_man_yen(m.avg_salary),
            format_man_yen(m.median_salary),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str("</div>\n");
}
