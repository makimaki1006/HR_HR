//! Section 01 - Cover ページ (1 枚)
//!
//! navy_report.rs の分割 (A1 Commit 2 / 2026-05-29) で抽出。
//!
//! 元 `navy_report/mod.rs` L29-L136 の `render_navy_cover` 本体および
//! 内部 helper (`push_cover_stat` / `push_cover_stat_small` / `push_cover_footer`)
//! を物理コピー。API 表面 (`pub(super) fn render_navy_cover`) は不変。
//!
//! 内部 helper 3 つは本ファイル内のみで使用されるため、元コードと同じく
//! `fn` (module-private) を維持。`navy_report` モジュール外への露出はない。

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_01_cover):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::SurveyAggregation;
use super::super::salary_summary;
use super::super::ReportVariant;

// ============================================================
// 公開 API
// ============================================================

/// Cover ページ全体 (1 枚)
pub(crate) fn render_navy_cover(
    html: &mut String,
    agg: &SurveyAggregation,
    variant: ReportVariant,
    now: &str,
    today_short: &str,
    target_region: &str,
) {
    // 作成日時 (now / today_short) は cover 表示しない方針 (2026-06-02 ユーザー要望)。
    // 引数は呼出側 (report_html/mod.rs) との互換維持のため残置。
    let _ = (now, today_short);
    let cover_lede = match variant {
        ReportVariant::Full => "ハローワーク掲載求人 + アップロード CSV クロス分析により、対象地域における求人市場の構造と機会を可視化します。",
        // 2026-07-09: Extended (詳細版) は MI と同じリード文を用いる (追加 4 図は Section 10 で明示)。
        // 2026-07-11: SP版 (仮) も同じリード文 (Extended 全部入りベースのため)。
        ReportVariant::MarketIntelligence | ReportVariant::Extended | ReportVariant::Sp => "アップロード CSV + 公開統計クロス分析により、採用市場・ターゲット分析と競合動向を立体的に把握します。",
        ReportVariant::Public => "アップロード CSV + 公開統計クロス分析により、対象地域の構造的特徴を把握します。",
    };

    let hl_count = format_number(agg.total_count as i64);
    let salary_headline = salary_summary::SalaryHeadline::from_aggregation(agg);
    let cover_hl = salary_headline.cover_highlight_text();

    html.push_str("<section class=\"page-navy cover-navy\" role=\"region\" aria-labelledby=\"navy-cover-title\">\n");

    // topbar
    html.push_str("<div class=\"cover-topbar\">\n");
    html.push_str("<div class=\"brand\">\n");
    html.push_str("<span class=\"brand-mark\" aria-hidden=\"true\"></span>\n");
    html.push_str("<span class=\"brand-name\">FOR A-CAREER</span>\n");
    html.push_str("</div>\n");
    html.push_str("</div>\n");

    // body
    html.push_str("<div class=\"cover-body\">\n");
    html.push_str("<div class=\"cover-eyebrow\">RECRUITMENT MARKET REPORT</div>\n");
    html.push_str("<div class=\"cover-rule\" aria-hidden=\"true\"></div>\n");
    html.push_str(
        "<h1 id=\"navy-cover-title\" class=\"cover-title\">求人市場<br>総合診断レポート</h1>\n",
    );
    html.push_str(&format!(
        "<p class=\"cover-lede\">{}</p>\n",
        escape_html(cover_lede)
    ));

    // stats
    html.push_str("<div class=\"cover-stats\">\n");
    push_cover_stat(html, &hl_count, "件", "サンプル件数");
    push_cover_stat_small(html, target_region, "主要地域 (対象)");
    push_cover_stat(html, &cover_hl.value_text, &cover_hl.unit, &cover_hl.label);
    push_cover_stat_small(html, variant.display_name(), "レポート版");
    html.push_str("</div>\n");

    html.push_str("</div>\n"); // /cover-body

    // footer
    html.push_str("<div class=\"cover-footer\">\n");
    push_cover_footer(html, "発行", "株式会社 For A-career");
    push_cover_footer(html, "対象地域", target_region);
    push_cover_footer(html, "取扱区分", "機密 / 社外秘");
    html.push_str("</div>\n");

    html.push_str("</section>\n");
}

fn push_cover_stat(html: &mut String, value: &str, unit: &str, label: &str) {
    html.push_str(&format!(
        "<div class=\"cover-stat\">\
         <div class=\"cs-num\">{}<span class=\"cs-unit\">{}</span></div>\
         <div class=\"cs-label\">{}</div>\
         </div>\n",
        escape_html(value),
        escape_html(unit),
        escape_html(label)
    ));
}

fn push_cover_stat_small(html: &mut String, value: &str, label: &str) {
    html.push_str(&format!(
        "<div class=\"cover-stat\">\
         <div class=\"cs-num\" style=\"font-size:18pt;\">{}</div>\
         <div class=\"cs-label\">{}</div>\
         </div>\n",
        escape_html(value),
        escape_html(label)
    ));
}

fn push_cover_footer(html: &mut String, label: &str, value: &str) {
    html.push_str(&format!(
        "<div><div class=\"cf-label\">{}</div><div class=\"cf-val\">{}</div></div>\n",
        escape_html(label),
        escape_html(value)
    ));
}
