//! Section 07.5 - 年間休日 × 給与 詳細
//!
//! ## 構成 (2026-06-26 UI/UX 改善版)
//!
//! - §07.5-1 サマリー: 概況 KPI 6 枚 (抽出件数/平均/中央値/Q3/標準偏差/120日以上比率)
//! - §07.5-2 分布: 年間休日カテゴリ分布 (横棒グラフ SVG)
//! - §07.5-3 相関: 給与×年間休日 散布図 (雇用形態色分け + 相関係数 r + 回帰直線)
//! - §07.5-4 具体例: 個別求人テーブル (年間休日色分けバッジ + 給与 mini bar、最大 100 件)

#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::SurveyAggregation;
use super::common::push_page_head;

/// 年間休日 × 給与 詳細セクションを描画。
///
/// `agg.jobbox_records` と `agg.annual_holidays_values` の両方が空ならスキップ。
pub(crate) fn render_navy_section_jobbox_detail(html: &mut String, agg: &SurveyAggregation) {
    if agg.annual_holidays_values.is_empty() && agg.jobbox_records.is_empty() {
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
    render_distribution_block(html, agg);
    render_correlation_block(html, agg);
    render_examples_block(html, agg);

    html.push_str("</section>\n");
}

// ============================================================================
// §07.5-1 サマリー: 概況 KPI 6 枚
// ============================================================================
fn render_summary_kpi(html: &mut String, agg: &SurveyAggregation) {
    let extracted = agg.annual_holidays_values.len();
    if extracted == 0 {
        return;
    }
    let sum: i64 = agg.annual_holidays_values.iter().sum();
    let mean = sum as f64 / extracted as f64;
    let median = compute_median_i64(&agg.annual_holidays_values);
    let min_v = agg
        .annual_holidays_values
        .iter()
        .min()
        .copied()
        .unwrap_or(0);
    let max_v = agg
        .annual_holidays_values
        .iter()
        .max()
        .copied()
        .unwrap_or(0);

    html.push_str("<div class=\"block-title\">§07.5-1 &nbsp;サマリー</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");
    // 2026-06-26 「抽出件数 N件 全 M件中 (X%)」KPI は削除 (信頼性低下の印象を回避)
    push_kpi_card(
        html,
        "平均年間休日",
        &format!("{:.0} 日", mean),
        &format!("中央値 {} 日 / 範囲 {} - {} 日", median, min_v, max_v),
    );
    push_kpi_card(
        html,
        "第3四分位 (Q3)",
        &format!("{} 日", agg.holiday_q3),
        "上位 25% はこれ以上",
    );
    push_kpi_card(
        html,
        "標準偏差",
        &format!("{:.1} 日", agg.holiday_stddev),
        "ばらつきの大きさ",
    );
    push_kpi_card(
        html,
        "120日以上比率",
        &format!("{:.0}%", agg.holiday_pct_ge_120 * 100.0),
        "週休2日+祝日 達成率",
    );
    push_kpi_card(
        html,
        "125日以上比率",
        &format!("{:.0}%", agg.holiday_pct_ge_125 * 100.0),
        "完全週休2日+α 達成率",
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
// §07.5-2 分布: 年間休日カテゴリ分布 (横棒グラフ SVG)
// ============================================================================
fn render_distribution_block(html: &mut String, agg: &SurveyAggregation) {
    if agg.annual_holidays_category_distribution.is_empty() {
        return;
    }
    let extracted = agg.annual_holidays_values.len();
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
        .annual_holidays_category_distribution
        .iter()
        .map(|(_, c)| *c)
        .max()
        .unwrap_or(1)
        .max(1);

    // SVG 横棒グラフ
    let row_h: i64 = 32;
    let row_gap: i64 = 8;
    let rows = agg.annual_holidays_category_distribution.len() as i64;
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
    for (i, (label, count)) in agg.annual_holidays_category_distribution.iter().enumerate() {
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
}

// ============================================================================
// §07.5-3 相関: 給与×年間休日 散布図 (雇用形態色分け + 相関係数 r + 回帰直線)
// ============================================================================
fn render_correlation_block(html: &mut String, agg: &SurveyAggregation) {
    if agg.salary_vs_holidays_scatter.is_empty() {
        return;
    }
    html.push_str(
        "<div class=\"block-title\">§07.5-3 &nbsp;給与 × 年間休日 散布図 (月給/年俸のみ)</div>\n",
    );

    // 相関係数の表示
    if let Some(r) = agg.salary_holidays_correlation {
        let strength = describe_correlation(r);
        html.push_str(&format!(
            "<p class=\"so-what\">相関係数 r = <strong>{:.3}</strong> ({})</p>\n",
            r, strength
        ));
    }

    render_scatter_svg_emp(
        html,
        &agg.salary_vs_holidays_scatter_emp,
        agg.salary_holidays_regression,
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

    let x_min: i64 = 150_000;
    let data_x_max = points.iter().map(|p| p.0).max().unwrap_or(500_000);
    let x_max: i64 = data_x_max.max(500_000);
    let y_min: i64 = 70;
    let y_max: i64 = 180;
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
    if agg.jobbox_records.is_empty() {
        return;
    }
    let listed = agg.jobbox_records.len();
    const TABLE_LIMIT: usize = 100;
    let limit = listed.min(TABLE_LIMIT);
    let extracted = agg.annual_holidays_values.len();

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

    for rec in &agg.jobbox_records[..limit] {
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

/// 給与レンジを mini bar (SVG) で描画
/// i64 配列の中央値
fn compute_median_i64(values: &[i64]) -> i64 {
    if values.is_empty() {
        return 0;
    }
    let mut sorted: Vec<i64> = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2
    } else {
        sorted[mid]
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::aggregator::JobBoxRecord;
    use super::*;

    fn agg_with_jobbox() -> SurveyAggregation {
        SurveyAggregation {
            total_count: 5,
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
                salary_unit: "月給".to_string(),
                salary_raw: "月給25万円〜35万円".to_string(),
                url: Some("https://example.com/job/1".to_string()),
            }],
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
        assert!(html.contains("正相関"), "correlation description");
        // 雇用形態凡例
        assert!(html.contains("正社員"));
        assert!(html.contains("パート・アルバイト"));
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
        assert_eq!(compute_median_i64(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(compute_median_i64(&[1, 2, 3, 4]), 2);
        assert_eq!(compute_median_i64(&[]), 0);
    }
}
