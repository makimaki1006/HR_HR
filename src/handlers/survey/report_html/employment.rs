//! 分割: report_html/employment.rs (物理移動・内容変更なし)

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

pub(super) fn render_section_employment(
    html: &mut String,
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
) {
    if agg.by_employment_type.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>雇用形態分布</h2>\n");
    // So What 行: 件数の多い形態と給与差を 1 行で示す
    if let Some((top_name, top_count)) = agg.by_employment_type.first() {
        let top_pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        let top_salary = by_emp_type_salary
            .iter()
            .find(|e| &e.emp_type == top_name)
            .map(|e| format_man_yen(e.avg_salary))
            .unwrap_or_else(|| "-".to_string());
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多い形態は「{}」で {:.1}% を占め、平均月給は {}。</p>\n",
            escape_html(top_name),
            top_pct,
            escape_html(&top_salary)
        ));
    }

    // EChartsドーナツチャート TOP6
    let colors = [
        "#1565C0", "#E69F00", "#009E73", "#D55E00", "#CC79A7", "#56B4E9",
    ];
    let pie_data: Vec<serde_json::Value> = agg
        .by_employment_type
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, (name, count))| {
            json!({
                "value": count,
                "name": name,
                "itemStyle": {"color": colors[i % colors.len()]}
            })
        })
        .collect();

    let config = json!({
        "tooltip": {"trigger": "item", "formatter": "{b}: {c}件 ({d}%)"},
        "legend": {
            "orient": "vertical",
            "right": "5%",
            "top": "center",
            "textStyle": {"fontSize": 10}
        },
        "series": [{
            "type": "pie",
            "radius": ["35%", "65%"],
            "center": ["35%", "50%"],
            "data": pie_data,
            "label": {"formatter": "{b}\n{d}%", "fontSize": 10}
        }]
    });
    html.push_str(&render_echart_div(&config.to_string(), 250));

    // 雇用形態別給与テーブル（ソート可能）
    if !by_emp_type_salary.is_empty() {
        html.push_str("<h3>雇用形態別 給与水準</h3>\n");
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>雇用形態</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n");
        for e in by_emp_type_salary {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                escape_html(&e.emp_type),
                format_number(e.count as i64),
                format_man_yen(e.avg_salary),
                format_man_yen(e.median_salary),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}

pub(super) fn render_section_emp_group_native(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_emp_group_native.is_empty() {
        return;
    }
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"emp-group-native-title\">\n",
    );
    html.push_str(
        "<h2 id=\"emp-group-native-title\">雇用形態グループ別 給与分析（ネイティブ単位）</h2>\n",
    );
    html.push_str(
        "<p class=\"section-header-meta\">\
         正社員は月給、パートは時給、と各グループのネイティブ単位で集計。\
         単位の異なる給与を混ぜず、直感と一致する単位で評価します。</p>\n",
    );

    html.push_str("<div class=\"emp-group-grid\" style=\"display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px;margin-top:12px;\">\n");

    for group in &agg.by_emp_group_native {
        let unit_suffix = if group.native_unit == "時給" {
            "円"
        } else {
            "円"
        };
        let is_hourly = group.native_unit == "時給";
        // 月給は「万円表示」、時給は「円表示」
        let format_salary = |v: i64| -> String {
            if is_hourly {
                format!("{}円", format_number(v))
            } else {
                format!("{:.1}万円", v as f64 / 10_000.0)
            }
        };

        html.push_str(&format!(
            "<div class=\"emp-group-card\" style=\"border:1px solid var(--c-border);border-radius:8px;padding:14px 16px;background:var(--c-bg-card);\">\n"
        ));
        html.push_str(&format!(
            "<div style=\"font-size:13pt;font-weight:700;color:var(--c-primary);\">{}</div>\n",
            escape_html(&group.group_label)
        ));
        // 「n=100件 (IQR外れ値除外: 3件)」のような表示
        let count_display = if group.outliers_removed > 0 {
            format!(
                "n={}件（IQR で {} 件除外、除外前 {}）",
                format_number(group.count as i64),
                format_number(group.outliers_removed as i64),
                format_number(group.raw_count as i64)
            )
        } else {
            format!("n={}件", format_number(group.count as i64))
        };
        html.push_str(&format!(
            "<div style=\"font-size:10pt;color:var(--c-muted);margin-bottom:8px;\">集計単位: {} / {}</div>\n",
            escape_html(&group.native_unit),
            count_display
        ));
        html.push_str("<table style=\"width:100%;font-size:10.5pt;border-collapse:collapse;\">\n");
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">中央値</td><td style=\"padding:3px 0;text-align:right;font-weight:600;\">{}</td></tr>\n",
            format_salary(group.median)
        ));
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">平均値</td><td style=\"padding:3px 0;text-align:right;\">{}</td></tr>\n",
            format_salary(group.mean)
        ));
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">範囲</td><td style=\"padding:3px 0;text-align:right;font-size:9pt;\">{} 〜 {}</td></tr>\n",
            format_salary(group.min),
            format_salary(group.max)
        ));
        html.push_str("</table>\n");

        if !group.included_emp_types.is_empty() {
            html.push_str(&format!(
                "<div style=\"margin-top:6px;font-size:9pt;color:var(--c-muted);\">含まれる雇用形態: {}</div>\n",
                escape_html(&group.included_emp_types.join(" / "))
            ));
        }
        let _ = unit_suffix;
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");

    html.push_str(
        "<p class=\"print-note\">\
         ※ 「正社員」グループは月給ベース（時給は ×167 で月給換算）、\
         「パート」グループは時給ベース（月給は /167 で時給換算）。\
         「派遣・その他」はグループ内多数派の単位を採用。<br>\
         ※ 各グループ内で IQR 法（Q1 − 1.5×IQR ～ Q3 + 1.5×IQR の範囲外）\
         による外れ値除外を適用。除外件数は各カード内に表示。</p>\n",
    );
    html.push_str("</section>\n");
}
