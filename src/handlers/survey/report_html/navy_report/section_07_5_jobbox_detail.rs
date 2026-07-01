//! Section 07.5 - 年間休日 × 給与 詳細
//!
//! ## 構成 (2026-07-01 セグメント別給与拡張版)
//!
//! - §07.5-1 サマリー: 概況 KPI 5 枚 (抽出件数/平均/中央値/Q3/120日以上比率)
//! - §07.5-2 分布: 年間休日カテゴリ分布 (横棒グラフ SVG + 給与中央値テーブル)
//! - §07.5-3 相関: 給与×年間休日 散布図 (雇用形態色分け + 相関係数 r + 回帰直線)
//! - §07.5-4 具体例: 個別求人テーブル (年間休日色分けバッジ + 給与 mini bar、最大 100 件)
//! - §07.5-5 セグメント別給与統計: 年間休日カテゴリ別 下限/上限の 平均/中央値/最頻値

// 一部 helper 関数は test 用、または将来拡張のために定義済み (使用されていないものは dead_code)
#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::{
    median_of, SurveyAggregation, SCATTER_X_MIN, SCATTER_Y_MAX, SCATTER_Y_MIN,
};
use super::common::{push_kpi_card_simple, push_page_head};

/// 年間休日 × 給与 詳細セクションを描画。
///
/// `agg.jobbox.jobbox_records` と `agg.jobbox.annual_holidays_values` の両方が空ならスキップ。
pub(crate) fn render_navy_section_jobbox_detail(html: &mut String, agg: &SurveyAggregation) {
    if agg.jobbox.annual_holidays_values.is_empty() && agg.jobbox.jobbox_records.is_empty() {
        return;
    }

    html.push_str("<section class=\"page-navy navy-jobbox-detail\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 07.5",
        "年間休日 × 給与 詳細",
        "テキストから年間休日数を抽出し、給与・企業別に集計",
    );

    render_summary_kpi(html, agg);
    render_overall_stats_table(html, agg);
    render_distribution_block(html, agg);
    render_correlation_block(html, agg);
    render_examples_block(html, agg);
    render_segment_salary_block(html, agg);

    // Finding #9 (2026-07-01): 印刷崩れ対策 — .navy-jobbox-detail スコープで改ページ制御
    html.push_str(
        "<style>\
         @media print {\
           .navy-jobbox-detail .kpi-row,\
           .navy-jobbox-detail table { break-inside: avoid; page-break-inside: avoid; }\
         }\
         </style>\n",
    );

    html.push_str("</section>\n");
}

// ============================================================================
// §07.5-1 サマリー: 概況 KPI 6 枚
// ============================================================================
fn render_summary_kpi(html: &mut String, agg: &SurveyAggregation) {
    let extracted = agg.jobbox.annual_holidays_values.len();
    if extracted == 0 {
        return;
    }
    let sum: i64 = agg.jobbox.annual_holidays_values.iter().sum();
    let mean = sum as f64 / extracted as f64;
    let median = median_of(&agg.jobbox.annual_holidays_values);
    let min_v = agg
        .jobbox
        .annual_holidays_values
        .iter()
        .min()
        .copied()
        .unwrap_or(0);
    let max_v = agg
        .jobbox
        .annual_holidays_values
        .iter()
        .max()
        .copied()
        .unwrap_or(0);

    html.push_str("<div class=\"block-title\">§07.5-1 &nbsp;サマリー</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    // 2026-06-26 「抽出件数 N件 全 M件中 (X%)」KPI は削除 (信頼性低下の印象を回避)
    push_kpi_card_simple(
        html,
        "平均年間休日",
        &format!("{:.0} 日", mean),
        &format!("中央値 {} 日 / 範囲 {} - {} 日", median, min_v, max_v),
    );
    push_kpi_card_simple(
        html,
        "第3四分位 (Q3)",
        &format!("{} 日", agg.jobbox.holiday_q3),
        "上位 25% はこれ以上",
    );
    push_kpi_card_simple(
        html,
        "標準偏差",
        &format!("{:.1} 日", agg.jobbox.holiday_stddev),
        "ばらつきの大きさ",
    );
    push_kpi_card_simple(
        html,
        "120日以上比率",
        &format!("{:.0}%", agg.jobbox.holiday_pct_ge_120 * 100.0),
        "週休2日+祝日 達成率",
    );
    push_kpi_card_simple(
        html,
        "125日以上比率",
        &format!("{:.0}%", agg.jobbox.holiday_pct_ge_125 * 100.0),
        "完全週休2日+α 達成率",
    );
    html.push_str("</div>\n");
}

// ============================================================================
// §07.5-1 サマリー補助: 全体統計テーブル (2026-07-01 追加)
//   年間休日 / 月給下限 / 月給上限 の 平均 / 中央値 / 最頻値
//   最頻値は 年間休日=5日刻みビン、給与=5万円刻みビン (compute_salary_stats と同ロジック)
// ============================================================================
fn render_overall_stats_table(html: &mut String, agg: &SurveyAggregation) {
    let hv = &agg.jobbox.annual_holidays_values;
    let n_hol = hv.len();
    // Monthly 月給制のレコードから (min, max) ペアを集める (compute_salary_stats に渡す形式)
    let salary_pairs: Vec<(Option<i64>, Option<i64>)> = agg
        .jobbox
        .jobbox_records
        .iter()
        .map(|r| (r.salary_min, r.salary_max))
        .collect();
    let salary_stats = super::super::super::aggregator::compute_salary_stats(&salary_pairs);
    // 3 行 (年間休日/月給下限/月給上限) すべて n=0 なら描画しない
    if n_hol == 0 && salary_stats.n == 0 {
        return;
    }
    html.push_str(
        "<div class=\"block-title\">§07.5-1 補助 &nbsp;全体統計 (平均 / 中央値 / 最頻値)</div>\n",
    );
    html.push_str(
        "<p class=\"note\" style=\"margin-top:4px;\">※ 給与は月給制のみを対象。最頻値は \
         年間休日=5日刻み、給与=5万円刻みビンの最頻。単位は「日」または「万円」。</p>\n",
    );
    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:34%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:18%;\">\
         <col style=\"width:18%;\">\
         <col style=\"width:18%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>項目</th>\
         <th style=\"text-align:right;\">n</th>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">最頻値</th>\
         </tr></thead>\n<tbody>\n",
    );
    // 年間休日 行
    if n_hol > 0 {
        let mean_days = hv.iter().sum::<i64>() as f64 / n_hol as f64;
        let median_days = super::super::super::aggregator::median_of(hv);
        let mode_days = mode_5day_bin(hv);
        html.push_str(&format!(
            "<tr><td>年間休日</td>\
             <td style=\"text-align:right;\">{}</td>\
             <td style=\"text-align:right;\">{:.0} 日</td>\
             <td style=\"text-align:right;\">{} 日</td>\
             <td style=\"text-align:right;\">{} 日</td></tr>\n",
            format_number(n_hol as i64),
            mean_days,
            median_days,
            mode_days
                .map(|v| format!("{}", v))
                .unwrap_or_else(|| "—".to_string()),
        ));
    } else {
        html.push_str(
            "<tr><td>年間休日</td>\
             <td style=\"text-align:right;\">0</td>\
             <td style=\"text-align:right;\">—</td>\
             <td style=\"text-align:right;\">—</td>\
             <td style=\"text-align:right;\">—</td></tr>\n",
        );
    }
    // 月給下限 行
    push_salary_stats_row(
        html,
        "月給下限",
        salary_stats.n,
        salary_stats.min_mean,
        salary_stats.min_median,
        salary_stats.min_mode,
    );
    // 月給上限 行
    push_salary_stats_row(
        html,
        "月給上限",
        salary_stats.n,
        salary_stats.max_mean,
        salary_stats.max_median,
        salary_stats.max_mode,
    );
    html.push_str("</tbody></table>\n");
}

fn push_salary_stats_row(
    html: &mut String,
    label: &str,
    n: usize,
    mean: Option<i64>,
    median: Option<i64>,
    mode: Option<i64>,
) {
    let fmt_manyen = |v: Option<i64>| -> String {
        v.map(|x| format!("{:.1} 万円", x as f64 / 10_000.0))
            .unwrap_or_else(|| "—".to_string())
    };
    html.push_str(&format!(
        "<tr><td>{}</td>\
         <td style=\"text-align:right;\">{}</td>\
         <td style=\"text-align:right;\">{}</td>\
         <td style=\"text-align:right;\">{}</td>\
         <td style=\"text-align:right;\">{}</td></tr>\n",
        escape_html(label),
        format_number(n as i64),
        fmt_manyen(mean),
        fmt_manyen(median),
        fmt_manyen(mode),
    ));
}

/// 年間休日 5 日刻みビンの最頻値 (存在すれば Some、空なら None)。
/// タイの場合は最小ビンを返す (compute_salary_stats と同挙動)。
fn mode_5day_bin(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut bins: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for v in values {
        *bins.entry(v / 5 * 5).or_insert(0) += 1;
    }
    let max_count = *bins.values().max()?;
    bins.iter()
        .filter(|(_, c)| **c == max_count)
        .map(|(b, _)| *b)
        .min()
}

// ============================================================================
// §07.5-2 分布: 年間休日カテゴリ分布 (横棒グラフ SVG)
// ============================================================================
fn render_distribution_block(html: &mut String, agg: &SurveyAggregation) {
    if agg.jobbox.annual_holidays_category_distribution.is_empty() {
        return;
    }
    let extracted = agg.jobbox.annual_holidays_values.len();
    if extracted == 0 {
        return;
    }
    html.push_str("<div class=\"block-title\">§07.5-2 &nbsp;年間休日カテゴリ分布</div>\n");

    // カテゴリラベルに副題を付ける
    let subtitles: std::collections::HashMap<&str, &str> = [
        ("～89日", "週休1日程度"),
        ("90～104日", "週休2日未満"),
        ("105～119日", "週休2日程度"),
        ("120～124日", "週休2日+祝日"),
        ("125～129日", "完全週休2日+α"),
        ("130日～", "優良企業水準"),
    ]
    .iter()
    .copied()
    .collect();

    let max_count = agg
        .jobbox
        .annual_holidays_category_distribution
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1)
        .max(1);

    // SVG 横棒グラフ
    let row_h: i64 = 32;
    let row_gap: i64 = 8;
    let rows = agg.jobbox.annual_holidays_category_distribution.len() as i64;
    let svg_h = rows * (row_h + row_gap) + 20;
    let svg_w: i64 = 720;
    let label_w: i64 = 200;
    let bar_max_w = svg_w - label_w - 80; // 80 = 件数表示エリア

    html.push_str(&format!(
        "<svg viewBox=\"0 0 {svg_w} {svg_h}\" preserveAspectRatio=\"xMidYMid meet\" \
         style=\"width:100%;max-width:760px;height:auto;background:#ffffff;\
         border:1px solid #cbd5e1;border-radius:4px;display:block;margin:8px 0;padding:6px;\" \
         role=\"img\" aria-label=\"年間休日カテゴリ分布\">\n"
    ));
    for (i, (label, count)) in agg
        .jobbox
        .annual_holidays_category_distribution
        .iter()
        .enumerate()
    {
        let y = (i as i64) * (row_h + row_gap) + row_gap;
        let bar_w = (*count as i64).max(0) * bar_max_w / max_count as i64;
        let pct = if extracted > 0 {
            *count as f64 / extracted as f64 * 100.0
        } else {
            0.0
        };
        let sub = subtitles.get(label.as_str()).copied().unwrap_or("");
        // カテゴリラベル (左)
        html.push_str(&format!(
            "<text x=\"4\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"#1e293b\">{}</text>\n",
            y + row_h / 2 - 2,
            escape_html(label),
        ));
        if !sub.is_empty() {
            html.push_str(&format!(
                "<text x=\"4\" y=\"{}\" font-size=\"9\" fill=\"#64748b\">{}</text>\n",
                y + row_h / 2 + 12,
                escape_html(sub),
            ));
        }
        // バー (中央)
        html.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"#1e3a8a\" rx=\"3\" ry=\"3\"/>\n",
            label_w,
            y + 4,
            bar_w,
            row_h - 8,
        ));
        // 構成比のみ (件数は信頼性懸念のため非表示、2026-06-26)
        html.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"12\" font-weight=\"600\" fill=\"#334155\" text-anchor=\"end\">{:.1}%</text>\n",
            svg_w - 6,
            y + row_h / 2 + 4,
            pct,
        ));
    }
    html.push_str("</svg>\n");

    // 2026-07-01: SVG 分布の下に、カテゴリ別 月給下限/上限 中央値テーブルを追加。
    // ユーザー要件: 「年間休日のセグメントごとの給与」を分布表の隣にも見せる。
    // データは jobbox_records から集計 (月給制のみ)。件数 0 のカテゴリは "—" 表示。
    render_distribution_salary_median_table(html, agg);
}

/// §07.5-2 分布 SVG の下に配置される「カテゴリ別 月給下限/上限 中央値」表。
/// 表 7.5-A への給与列追加要件 (2026-07-01) に対応。
fn render_distribution_salary_median_table(html: &mut String, agg: &SurveyAggregation) {
    let stats = compute_salary_stats_by_holiday_category(agg);
    // 全カテゴリで n=0 なら描画しない (分布表そのものだけを見せる)
    if stats.iter().all(|s| s.n == 0) {
        return;
    }
    html.push_str(
        "<p class=\"note\" style=\"margin-top:4px;\">※ 下表は月給制求人のみを対象に、\
         各カテゴリの下限・上限給与の中央値を集計 (万円表示)。</p>\n",
    );
    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:34%;\">\
         <col style=\"width:14%;\">\
         <col style=\"width:26%;\">\
         <col style=\"width:26%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>カテゴリ</th>\
         <th style=\"text-align:right;\">件数</th>\
         <th style=\"text-align:right;\">月給下限 中央値</th>\
         <th style=\"text-align:right;\">月給上限 中央値</th>\
         </tr></thead>\n<tbody>\n",
    );
    for s in &stats {
        let (min_med, max_med) = if s.n == 0 {
            ("—".to_string(), "—".to_string())
        } else {
            (
                format!("{:.1} 万円", s.min_median as f64 / 10_000.0),
                format!("{:.1} 万円", s.max_median as f64 / 10_000.0),
            )
        };
        html.push_str(&format!(
            "<tr>\
             <td>{cat}</td>\
             <td style=\"text-align:right;\">{n}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{min_med}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{max_med}</td>\
             </tr>\n",
            cat = escape_html(&s.category),
            n = s.n,
            min_med = min_med,
            max_med = max_med,
        ));
    }
    html.push_str("</tbody></table>\n");
}

// ============================================================================
// §07.5-3 相関: 給与×年間休日 散布図 (雇用形態色分け + 相関係数 r + 回帰直線)
// ============================================================================
fn render_correlation_block(html: &mut String, agg: &SurveyAggregation) {
    if agg.jobbox.salary_vs_holidays_scatter.is_empty() {
        return;
    }
    html.push_str(
        "<div class=\"block-title\">§07.5-3 &nbsp;給与 × 年間休日 散布図 (月給/年俸のみ)</div>\n",
    );

    // 相関係数の表示
    // Finding #6 (2026-07-01): n に応じて表示を変える
    //   n < 10:  "相関係数 r = X.XXX (n=N, n 不足のため傾向判定なし)"
    //   10 ≤ n < 30: "相関係数 r = X.XXX (弱い正相関、参考値 n=N)"
    //   n ≥ 30:  "相関係数 r = X.XXX (弱い正相関)"
    let scatter_n = agg.jobbox.salary_vs_holidays_scatter_emp.len();
    if let Some(r) = agg.jobbox.salary_holidays_correlation {
        let corr_text = if scatter_n < 10 {
            format!(
                "相関係数 r = <strong>{:.3}</strong> (n={}, n 不足のため傾向判定なし)",
                r, scatter_n
            )
        } else if scatter_n < 30 {
            let strength = describe_correlation(r);
            format!(
                "相関係数 r = <strong>{:.3}</strong> ({}, 参考値 n={})",
                r, strength, scatter_n
            )
        } else {
            let strength = describe_correlation(r);
            format!("相関係数 r = <strong>{:.3}</strong> ({})", r, strength)
        };
        html.push_str(&format!("<p class=\"so-what\">{corr_text}</p>\n"));
    }

    // Finding #6: 回帰直線は n < 10 では非描画
    let regression_for_plot = if scatter_n < 10 {
        None
    } else {
        agg.jobbox.salary_holidays_regression
    };
    render_scatter_svg_emp(
        html,
        &agg.jobbox.salary_vs_holidays_scatter_emp,
        regression_for_plot,
    );
}

fn describe_correlation(r: f64) -> String {
    let abs = r.abs();
    if abs < 0.2 {
        return "ほぼ無相関".to_string();
    }
    let direction = if r > 0.0 { "正" } else { "負" };
    let level = if abs < 0.4 {
        "弱い"
    } else if abs < 0.6 {
        "中程度の"
    } else if abs < 0.8 {
        "強い"
    } else {
        "非常に強い"
    };
    format!("{}{}相関", level, direction)
}

/// 雇用形態色分け + 回帰直線付き散布図 SVG
fn render_scatter_svg_emp(
    html: &mut String,
    points: &[(i64, i64, String)],
    regression: Option<(f64, f64)>,
) {
    if points.is_empty() {
        return;
    }
    let w: i64 = 720;
    let h: i64 = 360;
    let margin_l: i64 = 60;
    let margin_r: i64 = 30;
    let margin_t: i64 = 40;
    let margin_b: i64 = 50;
    let plot_w = w - margin_l - margin_r;
    let plot_h = h - margin_t - margin_b;

    let x_min: i64 = SCATTER_X_MIN;
    let data_x_max = points.iter().map(|p| p.0).max().unwrap_or(500_000);
    let x_max: i64 = data_x_max.max(500_000);
    let y_min: i64 = SCATTER_Y_MIN;
    let y_max: i64 = SCATTER_Y_MAX;
    let x_range = (x_max - x_min).max(1);
    let y_range = (y_max - y_min).max(1);

    let x_to_px = |x: i64| -> i64 { margin_l + ((x - x_min).max(0) * plot_w) / x_range };
    let y_to_px = |y: i64| -> i64 { margin_t + plot_h - ((y - y_min).max(0) * plot_h) / y_range };

    html.push_str(&format!(
        "<svg viewBox=\"0 0 {w} {h}\" preserveAspectRatio=\"xMidYMid meet\" \
         style=\"width:100%;max-width:760px;height:auto;background:#ffffff;\
         border:1px solid #cbd5e1;border-radius:4px;display:block;margin:8px 0;\" \
         role=\"img\" aria-label=\"月給×年間休日 散布図\">\n"
    ));

    // 凡例 (右上)
    let legend_y = 14;
    html.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"#1e3a8a\" opacity=\"0.75\"/>\n",
        margin_l + 10,
        legend_y,
    ));
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#334155\">正社員</text>\n",
        margin_l + 20,
        legend_y + 4,
    ));
    html.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"#f97316\" opacity=\"0.75\"/>\n",
        margin_l + 80,
        legend_y,
    ));
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#334155\">パート・アルバイト</text>\n",
        margin_l + 90,
        legend_y + 4,
    ));
    html.push_str(&format!(
        "<circle cx=\"{}\" cy=\"{}\" r=\"4\" fill=\"#94a3b8\" opacity=\"0.75\"/>\n",
        margin_l + 220,
        legend_y,
    ));
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#334155\">その他</text>\n",
        margin_l + 230,
        legend_y + 4,
    ));

    // Y軸グリッド + ラベル
    for y_val in [80i64, 100, 120, 140, 160] {
        let py = y_to_px(y_val);
        html.push_str(&format!(
            "<line x1=\"{}\" y1=\"{py}\" x2=\"{}\" y2=\"{py}\" stroke=\"#e5e7eb\" stroke-dasharray=\"2,3\"/>\n",
            margin_l,
            margin_l + plot_w,
        ));
        html.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#334155\" text-anchor=\"end\">{y_val}日</text>\n",
            margin_l - 6,
            py + 4,
        ));
    }

    // X軸グリッド + ラベル
    let step = ((x_max - x_min) / 5).max(50_000);
    let mut t = x_min;
    while t <= x_max {
        let px = x_to_px(t);
        html.push_str(&format!(
            "<line x1=\"{px}\" y1=\"{}\" x2=\"{px}\" y2=\"{}\" stroke=\"#e5e7eb\" stroke-dasharray=\"2,3\"/>\n",
            margin_t,
            margin_t + plot_h,
        ));
        html.push_str(&format!(
            "<text x=\"{px}\" y=\"{}\" font-size=\"10\" fill=\"#334155\" text-anchor=\"middle\">{}万</text>\n",
            margin_t + plot_h + 14,
            t / 10_000,
        ));
        t += step;
    }

    // 軸線
    html.push_str(&format!(
        "<line x1=\"{margin_l}\" y1=\"{margin_t}\" x2=\"{margin_l}\" y2=\"{}\" stroke=\"#475569\" stroke-width=\"1\"/>\n",
        margin_t + plot_h,
    ));
    html.push_str(&format!(
        "<line x1=\"{margin_l}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#475569\" stroke-width=\"1\"/>\n",
        margin_t + plot_h,
        margin_l + plot_w,
        margin_t + plot_h,
    ));

    // 軸ラベル
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#475569\" text-anchor=\"middle\">月給</text>\n",
        margin_l + plot_w / 2,
        h - 8,
    ));
    let y_lx = 18;
    let y_ly = margin_t + plot_h / 2;
    html.push_str(&format!(
        "<text x=\"{y_lx}\" y=\"{y_ly}\" font-size=\"11\" fill=\"#475569\" text-anchor=\"middle\" \
         transform=\"rotate(-90 {y_lx} {y_ly})\">年間休日 (日)</text>\n",
    ));

    // 回帰直線 (描画範囲内のみ)
    if let Some((slope, intercept)) = regression {
        // y = slope * x + intercept (x: 円, y: 日)
        let x1 = x_min;
        let y1 = (slope * x1 as f64 + intercept).clamp(y_min as f64, y_max as f64);
        let x2 = x_max;
        let y2 = (slope * x2 as f64 + intercept).clamp(y_min as f64, y_max as f64);
        html.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#dc2626\" stroke-width=\"1.5\" \
             stroke-dasharray=\"5,3\" opacity=\"0.7\"/>\n",
            x_to_px(x1),
            y_to_px(y1 as i64),
            x_to_px(x2),
            y_to_px(y2 as i64),
        ));
    }

    // データプロット (雇用形態色分け)
    for (x, y, emp) in points {
        if *x < x_min || *x > x_max || *y < y_min || *y > y_max {
            continue;
        }
        let color = match emp.as_str() {
            "正社員" => "#1e3a8a",
            "パート・アルバイト" => "#f97316",
            _ => "#94a3b8",
        };
        let px = x_to_px(*x);
        let py = y_to_px(*y);
        html.push_str(&format!(
            "<circle cx=\"{px}\" cy=\"{py}\" r=\"3.5\" fill=\"{color}\" opacity=\"0.65\"/>\n"
        ));
    }

    // 件数表示 (右下)
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#64748b\" text-anchor=\"end\">n = {}</text>\n",
        margin_l + plot_w - 4,
        margin_t + plot_h - 6,
        points.len(),
    ));

    html.push_str("</svg>\n");
}

// ============================================================================
// §07.5-4 具体例: 個別求人テーブル
// ============================================================================
fn render_examples_block(html: &mut String, agg: &SurveyAggregation) {
    if agg.jobbox.jobbox_records.is_empty() {
        return;
    }
    let listed = agg.jobbox.jobbox_records.len();
    const TABLE_LIMIT: usize = 100;
    let limit = listed.min(TABLE_LIMIT);
    let extracted = agg.jobbox.annual_holidays_values.len();

    html.push_str(
        "<div class=\"block-title\">§07.5-4 &nbsp;個別求人 具体例 (年間休日降順)</div>\n",
    );
    // 注記を 1 行に統合 (2 連続注記の冗長さを解消、2026-06-26)
    html.push_str(&format!(
        "<p class=\"note\">※ 表示対象: 月給制で給与記載のある企業名記載分のみ ({} 件)。\
         年俸制／時給制／給与未記載／企業名空欄は除外 (KPI／分布／散布図には含まれる、集計対象 全 {} 件)。</p>\n",
        format_number(listed as i64),
        format_number(extracted as i64),
    ));

    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:17%;\">\
         <col style=\"width:25%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:8%;\">\
         <col style=\"width:8%;\">\
         <col style=\"width:29%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>企業名</th>\
         <th>求人タイトル</th>\
         <th>勤務地</th>\
         <th>雇用</th>\
         <th style=\"text-align:right;\">年間休日</th>\
         <th>月給レンジ</th>\
         </tr></thead>\n<tbody>\n",
    );

    for rec in &agg.jobbox.jobbox_records[..limit] {
        // 雇用形態バッジ
        let emp_badge = render_emp_badge(&rec.employment_type);
        // 年間休日色分け
        let (hol_bg, hol_fg) = holiday_color(rec.annual_holidays);
        // 月給レンジ (テキストのみ、mini bar 廃止 2026-06-26)
        let salary_text = match (rec.salary_min, rec.salary_max) {
            (Some(lo), Some(hi)) if hi == lo => format!("{} 円", format_number(lo)),
            (Some(lo), Some(hi)) if hi > lo => {
                format!("{} 〜 {} 円", format_number(lo), format_number(hi))
            }
            (Some(lo), _) => format!("{} 円 〜", format_number(lo)),
            (None, Some(hi)) => format!("〜 {} 円", format_number(hi)),
            _ => "—".to_string(),
        };
        html.push_str(&format!(
            "<tr>\
             <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{company}</td>\
             <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{title}</td>\
             <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{loc}</td>\
             <td style=\"white-space:nowrap;\">{emp}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">\
             <span style=\"display:inline-block;padding:2px 8px;border-radius:10px;\
             background:{hol_bg};color:{hol_fg};font-weight:600;white-space:nowrap;\">{hol} 日</span>\
             </td>\
             <td style=\"white-space:nowrap;\">{salary}</td>\
             </tr>\n",
            company = escape_html(&rec.company_name),
            title = escape_html(&rec.job_title),
            loc = escape_html(&rec.location),
            emp = emp_badge,
            hol_bg = hol_bg,
            hol_fg = hol_fg,
            hol = rec.annual_holidays,
            salary = salary_text,
        ));
    }
    html.push_str("</tbody></table>\n");
    if listed > limit {
        html.push_str(&format!(
            "<p class=\"note\">具体例 {} 件のうち上位 {} 件を表示中 (年間休日降順 → 企業名昇順)。</p>\n",
            format_number(listed as i64),
            format_number(limit as i64),
        ));
    }
}

// ============================================================================
// §07.5-5 セグメント別 給与統計 (年間休日カテゴリ別 下限/上限 平均/中央値/最頻値)
// 2026-07-01 追加。
// - データソース: agg.jobbox.jobbox_records (月給制のみ)
// - 単位: 万円表示
// - n=0 のカテゴリは "—" で表示 (テーブル自体は残す)
// - 全カテゴリで n=0 の場合はセクションごとスキップ
// - 最頻値: 5 万円ビン (bin 開始値を "XX.X 万円" として表示)
// ============================================================================
fn render_segment_salary_block(html: &mut String, agg: &SurveyAggregation) {
    let stats = compute_salary_stats_by_holiday_category(agg);
    if stats.iter().all(|s| s.n == 0) {
        return;
    }
    html.push_str(
        "<div class=\"block-title\">§07.5-5 &nbsp;セグメント別 給与統計 (年間休日カテゴリ別)</div>\n",
    );
    html.push_str(
        "<p class=\"note\">※ 月給制求人のみを対象に、年間休日カテゴリ別で\
         下限・上限給与の平均/中央値/最頻値を算出。最頻値は 5 万円刻みビンの開始値。単位: 万円。</p>\n",
    );
    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:20%;\">\
         <col style=\"width:8%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:12%;\">\
         <col style=\"width:12%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th rowspan=\"2\">カテゴリ</th>\
         <th rowspan=\"2\" style=\"text-align:right;\">n</th>\
         <th colspan=\"3\" style=\"text-align:center;\">月給下限</th>\
         <th colspan=\"3\" style=\"text-align:center;\">月給上限</th>\
         </tr><tr>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">最頻値</th>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">最頻値</th>\
         </tr></thead>\n<tbody>\n",
    );
    for s in &stats {
        let cells = if s.n == 0 {
            [
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
                "—".to_string(),
            ]
        } else {
            [
                format!("{:.1}", s.min_mean as f64 / 10_000.0),
                format!("{:.1}", s.min_median as f64 / 10_000.0),
                format!("{:.1}", s.min_mode as f64 / 10_000.0),
                format!("{:.1}", s.max_mean as f64 / 10_000.0),
                format!("{:.1}", s.max_median as f64 / 10_000.0),
                format!("{:.1}", s.max_mode as f64 / 10_000.0),
            ]
        };
        html.push_str(&format!(
            "<tr>\
             <td>{cat}</td>\
             <td style=\"text-align:right;\">{n}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c0}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c1}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c2}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c3}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c4}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{c5}</td>\
             </tr>\n",
            cat = escape_html(&s.category),
            n = s.n,
            c0 = cells[0],
            c1 = cells[1],
            c2 = cells[2],
            c3 = cells[3],
            c4 = cells[4],
            c5 = cells[5],
        ));
    }
    html.push_str("</tbody></table>\n");
}

/// カテゴリ別 給与統計 1 行分。
///
/// - `min_*` は月給下限給与、`max_*` は月給上限給与の統計 (単位: 円)
/// - `min_mode` / `max_mode` は 5 万円ビンの開始値 (例: 25 万〜30 万 → 250000)
/// - `n == 0` のカテゴリは全統計値が 0 のまま
struct HolidayCategorySalaryRow {
    category: String,
    n: usize,
    min_mean: i64,
    min_median: i64,
    min_mode: i64,
    max_mean: i64,
    max_median: i64,
    max_mode: i64,
}

/// jobbox_records から年間休日カテゴリ別の給与統計を算出。
///
/// ANNUAL_HOLIDAYS_CATEGORIES の 6 カテゴリ順に必ず 6 行返す (n=0 も含む)。
/// upload.rs の `annual_holidays_category` と一致するカテゴリ分類を再現する。
fn compute_salary_stats_by_holiday_category(
    agg: &SurveyAggregation,
) -> Vec<HolidayCategorySalaryRow> {
    // upload.rs::ANNUAL_HOLIDAYS_CATEGORIES と同順を維持
    const CATEGORIES: [&str; 6] = [
        "～89日",
        "90～104日",
        "105～119日",
        "120～124日",
        "125～129日",
        "130日～",
    ];
    let mut buckets: std::collections::HashMap<&'static str, (Vec<i64>, Vec<i64>)> = CATEGORIES
        .iter()
        .map(|&c| (c, (Vec::new(), Vec::new())))
        .collect();
    for rec in &agg.jobbox.jobbox_records {
        let cat = category_for_holidays(rec.annual_holidays);
        let entry = buckets.get_mut(cat).expect("cat は CATEGORIES のいずれか");
        if let Some(v) = rec.salary_min {
            if v > 0 {
                entry.0.push(v);
            }
        }
        if let Some(v) = rec.salary_max {
            if v > 0 {
                entry.1.push(v);
            }
        }
    }
    CATEGORIES
        .iter()
        .map(|&cat| {
            let (mins, maxs) = buckets.remove(cat).unwrap_or_default();
            // 行の n は「下限 or 上限のどちらかが有効な件数」ではなく、
            // 「下限が有効な件数」を採用 (jobbox_records は最低 min or max 片方あり)。
            // 実運用上 min はほぼ全件で存在するため乖離は微小。
            let n = mins.len().max(maxs.len());
            HolidayCategorySalaryRow {
                category: cat.to_string(),
                n,
                min_mean: mean_of_i64(&mins),
                min_median: median_i64(&mins),
                min_mode: mode_5man_bin(&mins),
                max_mean: mean_of_i64(&maxs),
                max_median: median_i64(&maxs),
                max_mode: mode_5man_bin(&maxs),
            }
        })
        .collect()
}

/// 年間休日 → カテゴリラベル (upload.rs::annual_holidays_category と同定義)
fn category_for_holidays(days: i64) -> &'static str {
    match days {
        i64::MIN..=89 => "～89日",
        90..=104 => "90～104日",
        105..=119 => "105～119日",
        120..=124 => "120～124日",
        125..=129 => "125～129日",
        _ => "130日～",
    }
}

fn mean_of_i64(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let sum: i64 = values.iter().sum();
    sum / values.len() as i64
}

fn median_i64(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted = values.to_vec();
    sorted.sort();
    let n = sorted.len();
    if n.is_multiple_of(2) {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2
    } else {
        sorted[n / 2]
    }
}

/// 5 万円ビンでの最頻値 (ビン開始値、円単位で返す)。
///
/// 例: 250,000〜299,999 円 → 250,000 として計上。最頻ビン開始値を返す。
/// 空配列は 0。
fn mode_5man_bin(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    const BIN: i64 = 50_000;
    let mut counts: std::collections::HashMap<i64, usize> = std::collections::HashMap::new();
    for &v in values {
        let bin_start = (v / BIN) * BIN;
        *counts.entry(bin_start).or_insert(0) += 1;
    }
    // 最多カウントのビン (同数タイは値の小さい方を採用 = 保守的)
    let max_count = counts.values().max().copied().unwrap_or(0);
    counts
        .into_iter()
        .filter(|(_, c)| *c == max_count)
        .map(|(bin, _)| bin)
        .min()
        .unwrap_or(0)
}

/// 雇用形態を色分けバッジ HTML に
fn render_emp_badge(emp: &str) -> String {
    if emp.is_empty() {
        return "<span style=\"color:#94a3b8;\">—</span>".to_string();
    }
    let (bg, fg) = if emp.contains("正社員") || emp.contains("正職員") {
        ("#dbeafe", "#1e3a8a")
    } else if emp.contains("契約") {
        ("#ede9fe", "#5b21b6")
    } else if emp.contains("パート") || emp.contains("アルバイト") || emp.contains("バイト")
    {
        ("#ffedd5", "#9a3412")
    } else if emp.contains("派遣") {
        ("#fef3c7", "#854d0e")
    } else {
        ("#f1f5f9", "#475569")
    };
    format!(
        "<span style=\"display:inline-block;padding:2px 8px;border-radius:10px;\
         background:{bg};color:{fg};font-size:10pt;white-space:nowrap;\">{}</span>",
        escape_html(emp)
    )
}

/// 年間休日カテゴリ別カラー (背景, 前景)
fn holiday_color(days: i64) -> (&'static str, &'static str) {
    match days {
        i64::MIN..=89 => ("#fee2e2", "#991b1b"),
        90..=104 => ("#fed7aa", "#9a3412"),
        105..=119 => ("#fef3c7", "#854d0e"),
        120..=124 => ("#d9f99d", "#365314"),
        125..=129 => ("#bbf7d0", "#14532d"),
        _ => ("#a7f3d0", "#064e3b"),
    }
}

// Finding #18 (2026-06-30): `compute_median_i64` を削除し、
// `super::super::super::aggregator::median_of` に統合 (上で use 済み)。
// 動作仕様は完全互換 (空→0、奇数→中央、偶数→中央2要素平均の整数割り算)。
// 旧実装は sort_unstable、新実装は sort。値は同一だが順序のみの差は無関係。

#[cfg(test)]
mod tests {
    use super::super::super::super::aggregator::{JobBoxRecord, JobboxAnalysis};
    use super::*;

    fn agg_with_jobbox() -> SurveyAggregation {
        SurveyAggregation {
            total_count: 5,
            jobbox: JobboxAnalysis {
                annual_holidays_values: vec![100, 110, 120, 125, 130],
                annual_holidays_category_distribution: vec![
                    ("～89日".to_string(), 0),
                    ("90～104日".to_string(), 1),
                    ("105～119日".to_string(), 1),
                    ("120～124日".to_string(), 1),
                    ("125～129日".to_string(), 1),
                    ("130日～".to_string(), 1),
                ],
                holiday_pct_ge_120: 0.6,
                holiday_pct_ge_125: 0.4,
                holiday_stddev: 11.4,
                holiday_q3: 125,
                salary_vs_holidays_scatter: vec![
                    super::super::super::super::aggregator::ScatterPoint { x: 250_000, y: 120 },
                    super::super::super::super::aggregator::ScatterPoint { x: 300_000, y: 125 },
                    super::super::super::super::aggregator::ScatterPoint { x: 200_000, y: 110 },
                ],
                salary_vs_holidays_scatter_emp: vec![
                    (250_000, 120, "正社員".to_string()),
                    (300_000, 125, "正社員".to_string()),
                    (200_000, 110, "パート・アルバイト".to_string()),
                ],
                salary_holidays_correlation: Some(0.45),
                salary_holidays_regression: Some((0.0001, 100.0)),
                jobbox_records: vec![JobBoxRecord {
                    company_name: "テスト株式会社".to_string(),
                    job_title: "ドライバー".to_string(),
                    location: "群馬県 高崎市".to_string(),
                    employment_type: "正社員".to_string(),
                    annual_holidays: 120,
                    salary_min: Some(250_000),
                    salary_max: Some(350_000),
                }],
                // 2026-07-01 追加フィールドは default で埋める (テストヘルパー簡略化のため)
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn renders_summary_kpi() {
        let mut html = String::new();
        render_summary_kpi(&mut html, &agg_with_jobbox());
        assert!(html.contains("§07.5-1"), "summary subheader");
        assert!(html.contains("第3四分位"), "Q3 KPI");
        assert!(html.contains("120日以上比率"), "120 day rate");
        assert!(html.contains("125日以上比率"), "125 day rate");
        assert!(html.contains("60%"), "120日以上 60%");
    }

    #[test]
    fn renders_distribution_horizontal_bars() {
        let mut html = String::new();
        render_distribution_block(&mut html, &agg_with_jobbox());
        assert!(html.contains("§07.5-2"));
        assert!(html.contains("<svg"), "SVG horizontal bar chart");
        assert!(html.contains("週休2日+祝日"), "subtitle for 120-124");
        assert!(html.contains("<rect"), "bar rect");
    }

    #[test]
    fn renders_correlation_with_r_and_regression() {
        let mut html = String::new();
        render_correlation_block(&mut html, &agg_with_jobbox());
        assert!(html.contains("§07.5-3"));
        assert!(html.contains("r ="), "correlation coefficient label");
        assert!(html.contains("0.450"), "r value formatted");
        // Finding #6 (2026-07-01): scatter_emp が 3 件 (n < 10) のため傾向判定なし表示
        assert!(
            html.contains("n 不足のため傾向判定なし"),
            "n < 10: no correlation description"
        );
        // 雇用形態凡例
        assert!(html.contains("正社員"));
        assert!(html.contains("パート・アルバイト"));
    }

    #[test]
    fn renders_correlation_with_sufficient_n() {
        // n >= 30 では describe_correlation が表示される
        let emp_points: Vec<(i64, i64, String)> = (0..30)
            .map(|i| (200_000 + i * 5_000, 110 + i % 20, "正社員".to_string()))
            .collect();
        let scatter_points: Vec<super::super::super::super::aggregator::ScatterPoint> = emp_points
            .iter()
            .map(|(x, y, _)| super::super::super::super::aggregator::ScatterPoint { x: *x, y: *y })
            .collect();
        let agg = SurveyAggregation {
            total_count: 30,
            jobbox: super::super::super::super::aggregator::JobboxAnalysis {
                annual_holidays_values: (0..30).map(|i| 110 + i % 20).collect(),
                salary_vs_holidays_scatter: scatter_points,
                salary_vs_holidays_scatter_emp: emp_points,
                salary_holidays_correlation: Some(0.45),
                salary_holidays_regression: Some((0.0001, 100.0)),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut html = String::new();
        render_correlation_block(&mut html, &agg);
        assert!(
            html.contains("正相関"),
            "n >= 30: correlation description shown"
        );
        assert!(
            !html.contains("n 不足"),
            "n >= 30: no insufficient-n message"
        );
    }

    #[test]
    fn renders_correlation_reference_n_range() {
        // 10 <= n < 30 では "(参考値 n=N)" が表示される
        let emp_points: Vec<(i64, i64, String)> = (0..15)
            .map(|i| (200_000 + i * 5_000, 110 + i % 10, "正社員".to_string()))
            .collect();
        let scatter_points: Vec<super::super::super::super::aggregator::ScatterPoint> = emp_points
            .iter()
            .map(|(x, y, _)| super::super::super::super::aggregator::ScatterPoint { x: *x, y: *y })
            .collect();
        let agg = SurveyAggregation {
            total_count: 15,
            jobbox: super::super::super::super::aggregator::JobboxAnalysis {
                annual_holidays_values: (0..15).map(|i| 110 + i % 10).collect(),
                salary_vs_holidays_scatter: scatter_points,
                salary_vs_holidays_scatter_emp: emp_points,
                salary_holidays_correlation: Some(0.35),
                salary_holidays_regression: Some((0.00005, 108.0)),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut html = String::new();
        render_correlation_block(&mut html, &agg);
        assert!(
            html.contains("参考値 n=15"),
            "10<=n<30: reference annotation"
        );
        assert!(
            html.contains("正相関"),
            "10<=n<30: correlation description shown"
        );
    }

    #[test]
    fn renders_examples_with_badges_and_text_salary() {
        let mut html = String::new();
        render_examples_block(&mut html, &agg_with_jobbox());
        assert!(html.contains("§07.5-4"));
        assert!(html.contains("テスト株式会社"));
        assert!(
            html.contains("border-radius:10px"),
            "badge style (employment type or holiday color)"
        );
        // 2026-06-26 mini bar 廃止 → 給与はテキスト表示
        assert!(
            html.contains("250,000 〜 350,000 円"),
            "salary range as plain text"
        );
        assert!(!html.contains("salary mini bar"), "mini bar removed");
    }

    #[test]
    fn full_section_renders_4_subheaders() {
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &agg_with_jobbox());
        assert!(html.contains("SECTION 07.5"));
        assert!(html.contains("§07.5-1"));
        assert!(html.contains("§07.5-2"));
        assert!(html.contains("§07.5-3"));
        assert!(html.contains("§07.5-4"));
        // 2026-07-01: §07.5-5 セグメント別給与統計セクション
        // agg_with_jobbox() には jobbox_records が 1 件あるので描画される
        assert!(html.contains("§07.5-5"));
    }

    // =========================================================================
    // 2026-07-01 新規: §07.5-2 給与中央値テーブル + §07.5-5 セグメント別給与
    // =========================================================================

    /// jobbox_records に月給データがあるとき、
    /// §07.5-2 分布 SVG の下に「月給下限/上限 中央値」テーブルが描画される。
    #[test]
    fn renders_salary_column_in_distribution_table() {
        let mut html = String::new();
        render_distribution_block(&mut html, &agg_with_jobbox());
        // 分布 SVG は当然出る
        assert!(html.contains("<svg"), "距離分布 SVG が存在する");
        // 追加された中央値テーブル (見出しセル)
        assert!(
            html.contains("月給下限 中央値"),
            "月給下限 中央値 列が存在する"
        );
        assert!(
            html.contains("月給上限 中央値"),
            "月給上限 中央値 列が存在する"
        );
        // agg_with_jobbox の 1 件は 120 日 / 25万〜35万 → 120～124日 行に "25.0 万円" が入る
        assert!(
            html.contains("25.0 万円"),
            "下限中央値 25.0 万円 が表示される"
        );
        assert!(
            html.contains("35.0 万円"),
            "上限中央値 35.0 万円 が表示される"
        );
        // 件数 0 のカテゴリ (～89日) は — で表示
        assert!(html.contains("—"), "件数 0 のカテゴリは — 表示");
    }

    /// §07.5-5 セグメント別給与統計セクションが描画される。
    /// agg_with_jobbox の jobbox_records に 1 件 (120 日, 25 万-35 万) あるので表示。
    #[test]
    fn renders_segment_salary_section() {
        let mut html = String::new();
        render_segment_salary_block(&mut html, &agg_with_jobbox());
        assert!(html.contains("§07.5-5"), "セグメント別給与セクション見出し");
        assert!(
            html.contains("セグメント別 給与統計"),
            "セクションタイトル本体"
        );
        // 6 カテゴリ全て行として存在
        assert!(html.contains("～89日"));
        assert!(html.contains("90～104日"));
        assert!(html.contains("105～119日"));
        assert!(html.contains("120～124日"));
        assert!(html.contains("125～129日"));
        assert!(html.contains("130日～"));
        // 単位ヘッダ
        assert!(html.contains("月給下限"));
        assert!(html.contains("月給上限"));
        // 25 万 (250,000 円) = 25.0 が下限に、35.0 が上限に表示
        assert!(html.contains("25.0"), "下限 25.0 万円");
        assert!(html.contains("35.0"), "上限 35.0 万円");
    }

    /// jobbox_records が空 (全カテゴリで n=0) の場合、§07.5-5 は描画しない。
    #[test]
    fn skips_segment_salary_when_all_zero() {
        let agg = SurveyAggregation {
            jobbox: JobboxAnalysis {
                // 年間休日抽出は存在するが、jobbox_records (月給付き求人) はゼロ
                annual_holidays_values: vec![100, 110, 120],
                annual_holidays_category_distribution: vec![
                    ("90～104日".to_string(), 1),
                    ("105～119日".to_string(), 1),
                    ("120～124日".to_string(), 1),
                ],
                jobbox_records: Vec::new(),
                ..Default::default()
            },
            ..Default::default()
        };
        let mut html = String::new();
        render_segment_salary_block(&mut html, &agg);
        assert!(
            html.is_empty(),
            "jobbox_records 空 → §07.5-5 は描画されない"
        );
    }

    /// mode_5man_bin: 5 万円ビン最頻値の基本動作。
    #[test]
    fn mode_5man_bin_basic() {
        // 250_000, 260_000 (両方 [250k,300k) ビン), 320_000 ([300k,350k) ビン)
        // → 最頻ビンは 250_000
        assert_eq!(mode_5man_bin(&[250_000, 260_000, 320_000]), 250_000);
        // タイは値の小さいビンを採用 (保守的)
        assert_eq!(mode_5man_bin(&[250_000, 320_000]), 250_000);
        // 空
        assert_eq!(mode_5man_bin(&[]), 0);
    }

    #[test]
    fn skips_when_no_jobbox_data() {
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &SurveyAggregation::default());
        assert!(html.is_empty());
    }

    #[test]
    fn correlation_description_basic() {
        assert_eq!(describe_correlation(0.05), "ほぼ無相関");
        assert!(describe_correlation(0.45).contains("正相関"));
        assert!(describe_correlation(-0.45).contains("負相関"));
    }

    #[test]
    fn median_helper_basic() {
        // Finding #18 (2026-06-30): aggregator::median_of に統合済み
        assert_eq!(median_of(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(median_of(&[1, 2, 3, 4]), 2);
        assert_eq!(median_of(&[]), 0);
    }

    // =========================================================================
    // Finding #22: salary_text 全分岐 + 空データ非対称セクション描画
    // =========================================================================

    /// salary_text 生成ロジックのテストヘルパー
    /// render_examples_block 内の match 式と同一ロジックを再現
    fn salary_text(min: Option<i64>, max: Option<i64>) -> String {
        use super::super::super::super::super::helpers::format_number;
        match (min, max) {
            (Some(lo), Some(hi)) if hi == lo => format!("{} 円", format_number(lo)),
            (Some(lo), Some(hi)) if hi > lo => {
                format!("{} 〜 {} 円", format_number(lo), format_number(hi))
            }
            (Some(lo), _) => format!("{} 円 〜", format_number(lo)),
            (None, Some(hi)) => format!("〜 {} 円", format_number(hi)),
            _ => "—".to_string(),
        }
    }

    #[test]
    fn salary_text_min_eq_max() {
        // salary_min=Some(250000) salary_max=Some(250000) → "250,000 円" (Commit 1 の修正検証)
        assert_eq!(
            salary_text(Some(250_000), Some(250_000)),
            "250,000 円",
            "min == max → 単一額表示 (〜 なし)"
        );
    }

    #[test]
    fn salary_text_min_only_shows_open() {
        // salary_min=Some(250000), salary_max=None → "250,000 円 〜"
        assert_eq!(
            salary_text(Some(250_000), None),
            "250,000 円 〜",
            "下限のみ → オープンレンジ表示"
        );
    }

    #[test]
    fn salary_text_max_only() {
        // salary_min=None, salary_max=Some(300000) → "〜 300,000 円"
        assert_eq!(
            salary_text(None, Some(300_000)),
            "〜 300,000 円",
            "上限のみ → 上限のみ表示"
        );
    }

    #[test]
    fn salary_text_both_none() {
        // 両方 None → "—"
        assert_eq!(salary_text(None, None), "—", "両 None → ダッシュ");
    }

    #[test]
    fn renders_when_only_holidays_present() {
        // annual_holidays_values が 5 件 / jobbox_records 空 → セクション描画される
        // (個別求人テーブルは省略されるが KPI / 分布は描画される)
        let agg = SurveyAggregation {
            jobbox: JobboxAnalysis {
                annual_holidays_values: vec![100, 110, 120, 125, 130],
                annual_holidays_category_distribution: vec![
                    ("90～104日".to_string(), 1),
                    ("105～119日".to_string(), 1),
                    ("120～124日".to_string(), 1),
                    ("125～129日".to_string(), 1),
                    ("130日～".to_string(), 1),
                ],
                holiday_pct_ge_120: 0.6,
                holiday_pct_ge_125: 0.4,
                holiday_stddev: 11.4,
                holiday_q3: 125,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &agg);
        // セクション全体が描画される
        assert!(
            html.contains("SECTION 07.5"),
            "annual_holidays のみでもセクション描画される"
        );
        // KPI サマリーが描画される
        assert!(html.contains("§07.5-1"), "KPI サマリーが描画される");
        // 個別求人テーブルは描画されない (jobbox_records が空)
        assert!(
            !html.contains("§07.5-4"),
            "jobbox_records 空 → 具体例テーブルは非表示"
        );
    }
}
