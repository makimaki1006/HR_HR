//! Section 07.6 - 人気度シグナル (Indeed SP のみ)
//!
//! Indeed (SP) スマートフォン版 CSV の `css-u74ql7` 列から抽出した
//! 「人気」「超人気」タグの集計を表示する。Indeed (SP) 以外のソースでは
//! 全件タグなしになるため、`popular_count + super_popular_count == 0` で
//! セクションごとスキップする。
//!
//! ## 構成
//! - §07.6-1 サマリー: 件数 / 比率 KPI 4 枚
//! - §07.6-2 月給・年間休日 比較: 人気タグ あり vs なし の中央値比較
//!
//! ## 設計メモ
//! - 「人気タグ」は Indeed 内部の表示優先度シグナルにすぎず、給与差・休日差は
//!   直接的な因果関係を意味しない (相関≠因果)。so-what は最小限に留める。

#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::SurveyAggregation;
use super::common::push_page_head;

/// 人気度シグナル セクションを描画。
///
/// `agg.popularity.popular_count == 0 && agg.popularity.super_popular_count == 0`
/// ならスキップ (Indeed SP 以外 / 集計対象なし)。
pub(crate) fn render_navy_section_popularity(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    if pop.popular_count == 0 && pop.super_popular_count == 0 {
        return;
    }

    html.push_str("<section class=\"page-navy navy-popularity\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 07.6",
        "人気度シグナル",
        "Indeed (SP) の「人気」「超人気」タグ集計 — 表示優先度の参考指標",
    );

    render_summary_kpi(html, agg);
    render_comparison_block(html, agg);

    html.push_str("</section>\n");
}

// ============================================================================
// §07.6-1 サマリー KPI
// ============================================================================
fn render_summary_kpi(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    html.push_str("<div class=\"block-title\">§07.6-1 &nbsp;サマリー</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");

    push_kpi_card(
        html,
        "人気タグ件数",
        &format!("{} 件", format_number(pop.popular_count as i64)),
        "Indeed (SP) 「人気」付与",
    );
    push_kpi_card(
        html,
        "超人気タグ件数",
        &format!("{} 件", format_number(pop.super_popular_count as i64)),
        "Indeed (SP) 「超人気」付与",
    );
    // 2026-07-01 Finding #2: 分母を IndeedSp 由来件数に明示。
    push_kpi_card(
        html,
        "人気タグ比率",
        &format!("{:.1}%", pop.popular_ratio * 100.0),
        &format!(
            "Indeed (SP) {} 件中 (人気+超人気)",
            format_number(pop.indeed_sp_total as i64)
        ),
    );

    // 月給差 (人気あり - なし) を補助 KPI として表示
    let salary_diff_text = match (pop.popular_salary_median, pop.non_popular_salary_median) {
        (Some(p), Some(n)) => {
            let diff = p - n;
            let sign = if diff >= 0 { "+" } else { "" };
            format!("{}{} 円", sign, format_number(diff))
        }
        _ => "—".to_string(),
    };
    push_kpi_card(
        html,
        "月給中央値差",
        &salary_diff_text,
        "人気タグ あり − なし (Monthly のみ)",
    );

    let holiday_diff_text = match (pop.popular_holidays_median, pop.non_popular_holidays_median) {
        (Some(p), Some(n)) => {
            let diff = p - n;
            let sign = if diff >= 0 { "+" } else { "" };
            format!("{}{} 日", sign, diff)
        }
        _ => "—".to_string(),
    };
    push_kpi_card(
        html,
        "年間休日中央値差",
        &holiday_diff_text,
        "人気タグ あり − なし",
    );

    html.push_str("</div>\n");
}

fn push_kpi_card(html: &mut String, label: &str, value: &str, foot: &str) {
    html.push_str(&format!(
        "<div class=\"kpi-card\">\
         <div class=\"kpi-label\">{}</div>\
         <div class=\"kpi-value\">{}</div>\
         <div class=\"kpi-foot\">{}</div>\
         </div>\n",
        escape_html(label),
        escape_html(value),
        escape_html(foot),
    ));
}

// ============================================================================
// §07.6-2 月給・年間休日 比較
// ============================================================================
fn render_comparison_block(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    // 比較可能な指標が 1 つもなければスキップ
    let has_salary = pop.popular_salary_median.is_some() || pop.non_popular_salary_median.is_some();
    let has_holidays =
        pop.popular_holidays_median.is_some() || pop.non_popular_holidays_median.is_some();
    if !has_salary && !has_holidays {
        return;
    }

    html.push_str("<div class=\"block-title\">§07.6-2 &nbsp;月給・年間休日 比較 (中央値)</div>\n");

    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:30%;\">\
         <col style=\"width:35%;\">\
         <col style=\"width:35%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right;\">人気タグ あり</th>\
         <th style=\"text-align:right;\">人気タグ なし</th>\
         </tr></thead>\n<tbody>\n",
    );

    if has_salary {
        html.push_str(&format!(
            "<tr>\
             <td>月給 中央値 (Monthly のみ)</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             </tr>\n",
            format_salary_yen(pop.popular_salary_median),
            format_salary_yen(pop.non_popular_salary_median),
        ));
    }
    if has_holidays {
        html.push_str(&format!(
            "<tr>\
             <td>年間休日 中央値 (日)</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             </tr>\n",
            format_days(pop.popular_holidays_median),
            format_days(pop.non_popular_holidays_median),
        ));
    }
    html.push_str("</tbody></table>\n");

    // so-what は相関≠因果リスク回避のため最小限
    html.push_str(
        "<p class=\"note\">※ 「人気」「超人気」は Indeed 内部の表示優先度シグナル。\
         給与・休日との差分は相関の参考値であり、因果関係は示しません。</p>\n",
    );
}

fn format_salary_yen(v: Option<i64>) -> String {
    match v {
        Some(x) => format!("{} 円", format_number(x)),
        None => "—".to_string(),
    }
}

fn format_days(v: Option<i64>) -> String {
    match v {
        Some(x) => format!("{} 日", x),
        None => "—".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::aggregator::PopularityAnalysis;
    use super::*;

    fn agg_with_popularity() -> SurveyAggregation {
        SurveyAggregation {
            total_count: 20,
            popularity: PopularityAnalysis {
                popular_count: 4,
                super_popular_count: 2,
                none_count: 14,
                popular_ratio: 6.0 / 20.0,
                indeed_sp_total: 20,
                popular_salary_median: Some(280_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: Some(120),
                non_popular_holidays_median: Some(110),
            },
            ..Default::default()
        }
    }

    #[test]
    fn renders_full_section_with_popularity() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &agg_with_popularity());
        assert!(html.contains("SECTION 07.6"));
        assert!(html.contains("§07.6-1"));
        assert!(html.contains("§07.6-2"));
        assert!(html.contains("人気タグ件数"));
        assert!(html.contains("超人気タグ件数"));
        // 30% (6/20) を含む
        assert!(html.contains("30.0%"), "popular_ratio formatted");
        // 月給差 +20,000 円
        assert!(html.contains("+20,000 円"), "salary diff");
        // 年間休日差 +10 日
        assert!(html.contains("+10 日"), "holidays diff");
        // 比較表
        assert!(html.contains("280,000 円"));
        assert!(html.contains("260,000 円"));
        assert!(html.contains("120 日"));
        assert!(html.contains("110 日"));
    }

    #[test]
    fn skips_when_no_popular_tags() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &SurveyAggregation::default());
        assert!(html.is_empty(), "no popular tag → skip section entirely");
    }

    #[test]
    fn skips_when_only_none_count() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 10,
            popularity: PopularityAnalysis {
                popular_count: 0,
                super_popular_count: 0,
                none_count: 10,
                popular_ratio: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.is_empty());
    }

    #[test]
    fn renders_with_only_popular_salary_missing_holidays() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 5,
            popularity: PopularityAnalysis {
                popular_count: 2,
                super_popular_count: 0,
                none_count: 3,
                popular_ratio: 0.4,
                indeed_sp_total: 5,
                popular_salary_median: Some(250_000),
                non_popular_salary_median: Some(240_000),
                popular_holidays_median: None,
                non_popular_holidays_median: None,
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.contains("SECTION 07.6"));
        assert!(html.contains("250,000 円"));
        // holidays 行は出ない (両方 None)
        assert!(
            !html.contains("年間休日 中央値"),
            "holiday row should be absent when both None"
        );
    }

    #[test]
    fn salary_diff_negative_sign() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 5,
            popularity: PopularityAnalysis {
                popular_count: 1,
                super_popular_count: 0,
                none_count: 4,
                popular_ratio: 0.2,
                indeed_sp_total: 5,
                popular_salary_median: Some(200_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: None,
                non_popular_holidays_median: None,
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // 200,000 - 260,000 = -60,000 → "-60,000 円" (sign は format_number 内で付く)
        assert!(html.contains("-60,000 円"), "negative diff displayed");
    }
}
