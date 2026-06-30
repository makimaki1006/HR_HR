//! Section 07.6 - 人気度シグナル / Indeed (SP) 表示優先度スコア 集計
//!
//! Indeed (SP) スマートフォン版 CSV の `css-u74ql7` 列から抽出した
//! 「人気」「超人気」タグの集計を表示する。Indeed (SP) 以外のソースでは
//! 全件タグなしになるため、`popular_count + super_popular_count == 0` で
//! セクションごとスキップする。
//!
//! ## 構成
//! - §07.6-1 サマリー: 件数 / 比率 KPI 5 枚
//! - §07.6-2 月給・年間休日 比較: 人気タグ あり vs なし の中央値比較
//!
//! ## 設計メモ
//! - 「人気タグ」は Indeed 内部の表示優先度スコアにすぎず、給与差・休日差は
//!   直接的な因果関係を意味しない (相関≠因果)。so-what は最小限に留める。

// 一部 helper 関数は test 用、または将来拡張のために定義済み (使用されていないものは dead_code)
#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::SurveyAggregation;
use super::common::{push_kpi_card_simple, push_page_head};

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

    // Finding #9 (2026-07-01): 印刷崩れ対策 — .navy-popularity スコープで改ページ制御
    html.push_str(
        "<style>\
         @media print {\
           .navy-popularity .kpi-row,\
           .navy-popularity table { break-inside: avoid; page-break-inside: avoid; }\
         }\
         </style>\n",
    );

    html.push_str("</section>\n");
}

// ============================================================================
// §07.6-1 サマリー KPI
// ============================================================================
fn render_summary_kpi(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    html.push_str("<div class=\"block-title\">§07.6-1 &nbsp;サマリー</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");

    push_kpi_card_simple(
        html,
        "人気タグ件数",
        &format!("{} 件", format_number(pop.popular_count as i64)),
        "Indeed (SP) 「人気」付与",
    );
    push_kpi_card_simple(
        html,
        "超人気タグ件数",
        &format!("{} 件", format_number(pop.super_popular_count as i64)),
        "Indeed (SP) 「超人気」付与",
    );
    // 2026-07-01 Finding #2: 分母を IndeedSp 由来件数に明示。
    push_kpi_card_simple(
        html,
        "人気タグ比率",
        &format!("{:.1}%", pop.popular_ratio * 100.0),
        &format!(
            "Indeed (SP) {} 件中 (人気+超人気)",
            format_number(pop.indeed_sp_total as i64)
        ),
    );

    // 月給差 (人気あり - なし) を補助 KPI として表示
    // Finding #5 (2026-07-01): 両群 n >= 5 を満たさない場合は "— (n不足)" に
    const N_MIN: usize = 5;
    // Finding #8 (2026-07-01): 月給差を万円表示に変更 (6-7 桁オーバーフロー解消)。
    let salary_diff_text = if pop.popular_n_salary >= N_MIN && pop.non_popular_n_salary >= N_MIN {
        match (pop.popular_salary_median, pop.non_popular_salary_median) {
            (Some(p), Some(n)) => {
                let diff = p - n;
                let diff_man = diff as f64 / 10_000.0;
                let sign = if diff >= 0 { "+" } else { "" };
                format!("{}{:.1} 万円", sign, diff_man)
            }
            _ => "—".to_string(),
        }
    } else {
        "— (n不足)".to_string()
    };
    let salary_diff_foot = format!(
        "人気タグ あり − なし (Monthly のみ) / 人気 n={} / なし n={}",
        pop.popular_n_salary, pop.non_popular_n_salary
    );
    push_kpi_card_simple(html, "月給中央値差", &salary_diff_text, &salary_diff_foot);

    let holiday_diff_text =
        if pop.popular_n_holidays >= N_MIN && pop.non_popular_n_holidays >= N_MIN {
            match (pop.popular_holidays_median, pop.non_popular_holidays_median) {
                (Some(p), Some(n)) => {
                    let diff = p - n;
                    let sign = if diff >= 0 { "+" } else { "" };
                    format!("{}{} 日", sign, diff)
                }
                _ => "—".to_string(),
            }
        } else {
            "— (n不足)".to_string()
        };
    let holiday_diff_foot = format!(
        "人気タグ あり − なし / 人気 n={} / なし n={}",
        pop.popular_n_holidays, pop.non_popular_n_holidays
    );
    push_kpi_card_simple(
        html,
        "年間休日中央値差",
        &holiday_diff_text,
        &holiday_diff_foot,
    );

    html.push_str("</div>\n");
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

    // Finding #5 (2026-07-01): n < 5 の場合は値非表示 (n 数は列ヘッダに併記)
    const N_MIN_TABLE: usize = 5;
    html.push_str("<div class=\"block-title\">§07.6-2 &nbsp;月給・年間休日 比較 (中央値)</div>\n");

    html.push_str(&format!(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:30%;\">\
         <col style=\"width:35%;\">\
         <col style=\"width:35%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right;\">人気タグ あり (n={})</th>\
         <th style=\"text-align:right;\">人気タグ なし (n={})</th>\
         </tr></thead>\n<tbody>\n",
        pop.popular_n_salary.max(pop.popular_n_holidays),
        pop.non_popular_n_salary.max(pop.non_popular_n_holidays),
    ));

    if has_salary {
        let pop_val = if pop.popular_n_salary >= N_MIN_TABLE {
            format_salary_yen(pop.popular_salary_median)
        } else {
            format!("— (n={})", pop.popular_n_salary)
        };
        let non_val = if pop.non_popular_n_salary >= N_MIN_TABLE {
            format_salary_yen(pop.non_popular_salary_median)
        } else {
            format!("— (n={})", pop.non_popular_n_salary)
        };
        html.push_str(&format!(
            "<tr>\
             <td>月給 中央値 (Monthly のみ)</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             </tr>\n",
            pop_val, non_val,
        ));
    }
    if has_holidays {
        let pop_val = if pop.popular_n_holidays >= N_MIN_TABLE {
            format_days(pop.popular_holidays_median)
        } else {
            format!("— (n={})", pop.popular_n_holidays)
        };
        let non_val = if pop.non_popular_n_holidays >= N_MIN_TABLE {
            format_days(pop.non_popular_holidays_median)
        } else {
            format!("— (n={})", pop.non_popular_n_holidays)
        };
        html.push_str(&format!(
            "<tr>\
             <td>年間休日 中央値 (日)</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             </tr>\n",
            pop_val, non_val,
        ));
    }
    html.push_str("</tbody></table>\n");

    // so-what は相関≠因果リスク回避のため最小限
    html.push_str(
        "<p class=\"note\">※ 「人気」「超人気」は Indeed 内部の表示優先度シグナル。\
         給与・休日との差分は相関の参考値であり、因果関係は示しません。</p>\n",
    );
}

// Finding #8 (2026-07-01): 月給中央値を万円表示に変更 (§07.6-2 比較表も統一)。
fn format_salary_yen(v: Option<i64>) -> String {
    match v {
        Some(x) => format!("{:.1} 万円", x as f64 / 10_000.0),
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
                // Finding #5 (2026-07-01): n >= 5 で正常表示されることを検証
                popular_n_salary: 6,
                non_popular_n_salary: 14,
                popular_n_holidays: 6,
                non_popular_n_holidays: 14,
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
        // Finding #8: 月給差は万円表示 (+2.0 万円)
        assert!(html.contains("+2.0 万円"), "salary diff in manyen");
        // 年間休日差 +10 日 (日は変更なし)
        assert!(html.contains("+10 日"), "holidays diff");
        // Finding #8: 比較表も万円表示
        assert!(html.contains("28.0 万円"));
        assert!(html.contains("26.0 万円"));
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
                // Finding #5: n >= 5 で月給表示が出ることを検証
                popular_n_salary: 5,
                non_popular_n_salary: 8,
                popular_n_holidays: 0,
                non_popular_n_holidays: 0,
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.contains("SECTION 07.6"));
        // Finding #8: 月給は万円表示
        assert!(html.contains("25.0 万円"), "250,000 → 25.0 万円");
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
                // Finding #5: n >= 5 で月給差が表示されることを検証
                popular_n_salary: 5,
                non_popular_n_salary: 10,
                popular_n_holidays: 0,
                non_popular_n_holidays: 0,
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // Finding #8: 200,000 - 260,000 = -60,000 → "-6.0 万円" (万円表示)
        assert!(
            html.contains("-6.0 万円"),
            "negative diff displayed in manyen"
        );
    }

    #[test]
    fn shows_insufficient_n_when_n_below_threshold() {
        // Finding #5: 両群 n < 5 の場合は "— (n不足)" を表示する
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
                popular_holidays_median: Some(115),
                non_popular_holidays_median: Some(108),
                popular_n_salary: 3,       // < 5
                non_popular_n_salary: 4,   // < 5
                popular_n_holidays: 2,     // < 5
                non_popular_n_holidays: 3, // < 5
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // KPI で n不足表示
        assert!(html.contains("n不足"), "n < 5 → insufficient-n indicator");
        // 差分 KPI に実値が出ない
        assert!(
            !html.contains("-6.0 万円"),
            "no diff value when n insufficient"
        );
        // 比較表にも n 表示 (n=3 or n=4)
        assert!(
            html.contains("n=3") || html.contains("n=2"),
            "table shows n"
        );
    }
}
