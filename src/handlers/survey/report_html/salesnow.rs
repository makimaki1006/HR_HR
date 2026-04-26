//! 分割: report_html/salesnow.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;


/// 地域注目企業テーブル
/// Why: 求人市場分析レポートから実際にアプローチ可能な企業リストへ繋げる
/// How: employee_count 降順で従業員数の多い 30 社を印刷レポートに追加
///
/// 2026-04-24 追加要件 3: 表示項目刷新
/// - 削除: 信用スコア (credit_score) — struct には残すが UI 非表示
/// - 追加: 売上 (sales_amount / sales_range) / 1年人員推移 / 3ヶ月人員推移
///
/// 関数名は呼出側の互換のため残す（UI 表示文言のみ「地域注目企業」に統一）
pub(super) fn render_section_salesnow_companies(html: &mut String, companies: &[NearbyCompany]) {
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"region-featured-title\">\n",
    );
    html.push_str("<h2 id=\"region-featured-title\">地域注目企業</h2>\n");
    html.push_str(
        "<p class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">\
        \u{203B} 地域内で従業員数の多い 30 社を整理しています。\
        HW 掲載件数が多い法人は採用が活発な傾向（相関であり、因果は別途検討）。\
        売上規模・人員推移も参考値として併記します。</p>\n",
    );
    html.push_str("<table class=\"data-table\">\n");
    html.push_str("<thead><tr>");
    for h in [
        "番号",
        "企業名",
        "都道府県",
        "業種",
        "従業員数",
        "売上",
        "1年人員推移",
        "3ヶ月人員推移",
        "HW求人数",
    ] {
        html.push_str(&format!("<th>{}</th>", escape_html(h)));
    }
    html.push_str("</tr></thead><tbody>\n");
    for (i, c) in companies.iter().take(30).enumerate() {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", i + 1));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.company_name)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.prefecture)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.sn_industry)));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.employee_count)
        ));
        // 売上: 金額と区分ラベルを併記
        let sales_cell = format_sales_cell(c.sales_amount, &c.sales_range);
        html.push_str(&format!("<td class=\"right\">{}</td>", sales_cell));
        // 1年推移 / 3ヶ月推移: 増減符号付きの %
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_1y)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_3m)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.hw_posting_count)
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table>\n");
    html.push_str("</section>\n");
}

/// 売上セル整形: 売上金額と区分ラベルを 1 セル 2 行で表示
pub(super) fn format_sales_cell(amount: f64, range: &str) -> String {
    if amount <= 0.0 && range.is_empty() {
        return "-".to_string();
    }
    // 金額は百万円単位以上に丸めて表示
    let amount_display = if amount >= 1.0e9 {
        format!("{:.1} 億円", amount / 1.0e8)
    } else if amount >= 1.0e6 {
        format!("{:.0} 百万円", amount / 1.0e6)
    } else if amount > 0.0 {
        format!("{:.0} 円", amount)
    } else {
        "-".to_string()
    };
    let range_display = if range.is_empty() {
        String::new()
    } else {
        format!(
            "<br><span style=\"font-size:9pt;color:var(--c-text-muted);\">{}</span>",
            escape_html(range)
        )
    };
    format!("{}{}", escape_html(&amount_display), range_display)
}

/// 人員推移セル整形: 増減符号付き %、0 は横ばい
pub(super) fn format_delta_cell(pct: f64) -> String {
    // NaN / 極端値ガード
    if !pct.is_finite() {
        return "-".to_string();
    }
    let cls = if pct > 0.5 {
        "trend-up"
    } else if pct < -0.5 {
        "trend-down"
    } else {
        "trend-flat"
    };
    format!("<span class=\"{}\">{:+.1}%</span>", cls, pct)
}
