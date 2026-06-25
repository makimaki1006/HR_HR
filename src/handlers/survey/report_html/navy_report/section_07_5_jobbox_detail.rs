//! Section 07.5 - 年間休日 × 給与 詳細 (2026-06-24 追加、2026-06-25 UI 改善)
//!
//! 求人ボックス CSV の `p-result_lines` (description) から年間休日を抽出し、
//! ① カテゴリ分布 ② 給与×年間休日 散布図 ③ 企業名×年間休日×給与 個別求人一覧
//! の 3 ブロックを Section 07 (Lifestyle) と Section 08 (Notes) の間に挿入する。

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
        "テキストから年間休日数を抽出し、給与・企業別に集計 (求人ボックスCSV 対応)",
    );

    let total_records = agg.total_count;
    let extracted = agg.annual_holidays_values.len();
    let extract_rate = if total_records > 0 {
        (extracted as f64) / (total_records as f64) * 100.0
    } else {
        0.0
    };

    let (mean_days, median_days, min_v, max_v) = if extracted > 0 {
        let sum: i64 = agg.annual_holidays_values.iter().sum();
        let mean = (sum as f64) / (extracted as f64);
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
        (mean, median, min_v, max_v)
    } else {
        (0.0, 0, 0, 0)
    };

    // -- 概況 KPI 4 枚
    html.push_str(&format!(
        "<div class=\"kpi-row\">\
         <div class=\"kpi-card\"><div class=\"kpi-label\">抽出件数</div>\
         <div class=\"kpi-value\">{} 件</div>\
         <div class=\"kpi-foot\">全 {} 件中 ({:.1}%)</div></div>\
         <div class=\"kpi-card\"><div class=\"kpi-label\">平均年間休日</div>\
         <div class=\"kpi-value\">{:.0} 日</div></div>\
         <div class=\"kpi-card\"><div class=\"kpi-label\">中央値</div>\
         <div class=\"kpi-value\">{} 日</div></div>\
         <div class=\"kpi-card\"><div class=\"kpi-label\">範囲</div>\
         <div class=\"kpi-value\">{} - {} 日</div></div>\
         </div>\n",
        format_number(extracted as i64),
        format_number(total_records as i64),
        extract_rate,
        mean_days,
        median_days,
        min_v,
        max_v,
    ));

    // -- 表 7.5-A 年間休日カテゴリ分布
    if !agg.annual_holidays_category_distribution.is_empty() && extracted > 0 {
        let max_count = agg
            .annual_holidays_category_distribution
            .iter()
            .map(|(_, c)| *c)
            .max()
            .unwrap_or(1)
            .max(1);
        html.push_str("<div class=\"block-title\">表 7.5-A &nbsp;年間休日カテゴリ分布</div>\n");
        html.push_str(
            "<table class=\"table-navy\">\n<thead><tr>\
                       <th>カテゴリ</th>\
                       <th style=\"text-align:right;\">件数</th>\
                       <th style=\"text-align:right;\">構成比</th>\
                       <th style=\"width:40%;\">分布バー (最大値比)</th>\
                       </tr></thead>\n<tbody>\n",
        );
        for (label, count) in &agg.annual_holidays_category_distribution {
            let pct_max = (*count as f64 / max_count as f64) * 100.0;
            let pct_total = (*count as f64 / extracted as f64) * 100.0;
            html.push_str(&format!(
                "<tr><td>{}</td>\
                 <td style=\"text-align:right;\">{}</td>\
                 <td style=\"text-align:right;\">{:.1}%</td>\
                 <td><div style=\"background:#1e3a8a;height:14px;width:{:.1}%;border-radius:2px;\" \
                 title=\"カテゴリ内最大値比 {:.1}%\"></div></td></tr>\n",
                escape_html(label),
                format_number(*count as i64),
                pct_total,
                pct_max,
                pct_max,
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    // -- 図 7.5-B 月給 × 年間休日 散布図 (SVG)
    if !agg.salary_vs_holidays_scatter.is_empty() {
        html.push_str(
            "<div class=\"block-title\">図 7.5-B &nbsp;月給 × 年間休日 散布図 \
             (月給/年俸のみ)</div>\n",
        );
        let points: Vec<(i64, i64)> = agg
            .salary_vs_holidays_scatter
            .iter()
            .map(|p| (p.x, p.y))
            .collect();
        render_scatter_svg(html, &points);
    }

    // -- 表 7.5-C 個別求人一覧 (年間休日抽出成功分)
    if !agg.jobbox_records.is_empty() {
        let total_jobbox = agg.jobbox_records.len();
        const TABLE_LIMIT: usize = 300;
        let limit = total_jobbox.min(TABLE_LIMIT);
        html.push_str(
            "<div class=\"block-title\">表 7.5-C &nbsp;個別求人一覧 \
             (年間休日降順)</div>\n",
        );
        // 各列の幅を固定して縦長を抑制 (table-layout: fixed)
        html.push_str(
            "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
             <colgroup>\
             <col style=\"width:20%;\">\
             <col style=\"width:28%;\">\
             <col style=\"width:18%;\">\
             <col style=\"width:8%;\">\
             <col style=\"width:8%;\">\
             <col style=\"width:9%;\">\
             <col style=\"width:9%;\">\
             </colgroup>\n\
             <thead><tr>\
             <th>企業名</th>\
             <th>求人タイトル</th>\
             <th>勤務地</th>\
             <th>雇用</th>\
             <th style=\"text-align:right;\">年間休日</th>\
             <th style=\"text-align:right;\">給与下限</th>\
             <th style=\"text-align:right;\">給与上限</th>\
             </tr></thead>\n<tbody>\n",
        );
        for rec in &agg.jobbox_records[..limit] {
            let s_min = rec
                .salary_min
                .map(|v| format!("{} 円", format_number(v)))
                .unwrap_or_else(|| "-".to_string());
            let s_max = rec
                .salary_max
                .map(|v| format!("{} 円", format_number(v)))
                .unwrap_or_else(|| "-".to_string());
            // word-break: keep-all で英数字は途中改行しない (URL 除去済みなので主に日本語)
            // overflow-wrap: anywhere で長すぎる文字列は折り返す
            html.push_str(&format!(
                "<tr>\
                 <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{company}</td>\
                 <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{title}</td>\
                 <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{loc}</td>\
                 <td style=\"overflow-wrap:anywhere;word-break:keep-all;\">{emp}</td>\
                 <td style=\"text-align:right;\">{hol} 日</td>\
                 <td style=\"text-align:right;\">{smin}</td>\
                 <td style=\"text-align:right;\">{smax}</td>\
                 </tr>\n",
                company = escape_html(&rec.company_name),
                title = escape_html(&rec.job_title),
                loc = escape_html(&rec.location),
                emp = escape_html(&rec.employment_type),
                hol = rec.annual_holidays,
                smin = s_min,
                smax = s_max,
            ));
        }
        html.push_str("</tbody></table>\n");
        if total_jobbox > limit {
            html.push_str(&format!(
                "<p class=\"note\">全 {} 件のうち上位 {} 件を表示中 (年間休日降順 → 企業名昇順)。</p>\n",
                format_number(total_jobbox as i64),
                format_number(limit as i64),
            ));
        }
    }

    html.push_str("</section>\n");
}

/// 給与×年間休日 散布図を SVG で描画する。
///
/// 入力: `points` は (月給円換算, 年間休日日数) のペア。
/// 横軸: 月給 (万円表示)、縦軸: 年間休日 (日)。
/// 縦長になりすぎない 640×320 のビューポートで描画 (max-width で印刷時もはみ出さない)。
fn render_scatter_svg(html: &mut String, points: &[(i64, i64)]) {
    if points.is_empty() {
        return;
    }

    let w: i64 = 640;
    let h: i64 = 320;
    let margin_l: i64 = 50;
    let margin_r: i64 = 20;
    let margin_t: i64 = 20;
    let margin_b: i64 = 50;
    let plot_w = w - margin_l - margin_r;
    let plot_h = h - margin_t - margin_b;

    // 軸範囲: X 軸は 15 万円〜データ最大、Y 軸は固定 70-180 日 (妥当範囲)
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
         style=\"width:100%;max-width:720px;height:auto;background:#0a1430;\
         border:1px solid #1e3a8a;border-radius:4px;display:block;margin:8px 0;\" \
         role=\"img\" aria-label=\"月給×年間休日 散布図\">\n"
    ));

    // Y 軸グリッド + ラベル (80, 100, 120, 140, 160)
    for y_val in [80i64, 100, 120, 140, 160] {
        let py = y_to_px(y_val);
        html.push_str(&format!(
            "<line x1=\"{}\" y1=\"{py}\" x2=\"{}\" y2=\"{py}\" \
             stroke=\"#374151\" stroke-dasharray=\"2,3\"/>\n",
            margin_l,
            margin_l + plot_w,
        ));
        html.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#d1d5db\" text-anchor=\"end\">{y_val}日</text>\n",
            margin_l - 6,
            py + 4,
        ));
    }
    // X 軸グリッド + ラベル (動的に 5 段階)
    let x_ticks: Vec<i64> = {
        let step = ((x_max - x_min) / 5).max(50_000);
        let mut v = Vec::new();
        let mut t = x_min;
        while t <= x_max {
            v.push(t);
            t += step;
        }
        v
    };
    for x_val in &x_ticks {
        let px = x_to_px(*x_val);
        html.push_str(&format!(
            "<line x1=\"{px}\" y1=\"{}\" x2=\"{px}\" y2=\"{}\" \
             stroke=\"#374151\" stroke-dasharray=\"2,3\"/>\n",
            margin_t,
            margin_t + plot_h,
        ));
        html.push_str(&format!(
            "<text x=\"{px}\" y=\"{}\" font-size=\"10\" fill=\"#d1d5db\" text-anchor=\"middle\">{}万</text>\n",
            margin_t + plot_h + 14,
            x_val / 10_000,
        ));
    }
    // 軸線
    html.push_str(&format!(
        "<line x1=\"{margin_l}\" y1=\"{margin_t}\" x2=\"{margin_l}\" y2=\"{}\" stroke=\"#6b7280\" stroke-width=\"1\"/>\n",
        margin_t + plot_h,
    ));
    html.push_str(&format!(
        "<line x1=\"{margin_l}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#6b7280\" stroke-width=\"1\"/>\n",
        margin_t + plot_h,
        margin_l + plot_w,
        margin_t + plot_h,
    ));
    // 軸ラベル
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"11\" fill=\"#9ca3af\" text-anchor=\"middle\">月給</text>\n",
        margin_l + plot_w / 2,
        h - 8,
    ));
    let y_label_x = 15;
    let y_label_y = margin_t + plot_h / 2;
    html.push_str(&format!(
        "<text x=\"{y_label_x}\" y=\"{y_label_y}\" font-size=\"11\" fill=\"#9ca3af\" \
         text-anchor=\"middle\" transform=\"rotate(-90 {y_label_x} {y_label_y})\">年間休日 (日)</text>\n",
    ));

    // データプロット (半透明青)
    for (x, y) in points {
        if *x < x_min || *x > x_max || *y < y_min || *y > y_max {
            continue;
        }
        let px = x_to_px(*x);
        let py = y_to_px(*y);
        html.push_str(&format!(
            "<circle cx=\"{px}\" cy=\"{py}\" r=\"3\" fill=\"#3b82f6\" opacity=\"0.6\"/>\n"
        ));
    }

    // 件数表示 (右上)
    html.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#9ca3af\" text-anchor=\"end\">n = {}</text>\n",
        margin_l + plot_w - 4,
        margin_t + 12,
        points.len(),
    ));

    html.push_str("</svg>\n");
}

/// i64 配列の中央値を計算 (Section 07.5 用 helper)
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
    fn renders_when_jobbox_data_present() {
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &agg_with_jobbox());
        assert!(html.contains("SECTION 07.5"), "section header required");
        assert!(
            html.contains("年間休日 × 給与 詳細"),
            "title without jobbox keyword"
        );
        assert!(html.contains("テスト株式会社"), "company name rendered");
        assert!(html.contains("120 日"), "annual holidays value rendered");
        assert!(html.contains("250,000 円"), "salary_min rendered");
        // URL 列削除済み - href 含まない
        assert!(!html.contains("https://example.com/job/1"), "url removed");
        assert!(!html.contains("V2 独自機能"), "so-what removed (表 7.5-C)");
        assert!(
            !html.contains("週休2日+祝日相当"),
            "so-what removed (表 7.5-A)"
        );
        assert!(!html.contains("相関 ≠ 因果"), "so-what removed (表 7.5-B)");
    }

    #[test]
    fn skips_when_no_jobbox_data() {
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &SurveyAggregation::default());
        assert!(html.is_empty(), "empty agg → section not rendered");
    }

    #[test]
    fn scatter_svg_renders_with_points() {
        let mut html = String::new();
        render_scatter_svg(&mut html, &[(250_000, 120), (300_000, 125), (200_000, 110)]);
        assert!(html.contains("<svg"));
        assert!(html.contains("<circle"));
        assert!(html.contains("n = 3"));
    }

    #[test]
    fn scatter_svg_empty_renders_nothing() {
        let mut html = String::new();
        render_scatter_svg(&mut html, &[]);
        assert!(html.is_empty());
    }

    #[test]
    fn median_helper_basic() {
        assert_eq!(compute_median_i64(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(compute_median_i64(&[1, 2, 3, 4]), 2);
        assert_eq!(compute_median_i64(&[]), 0);
        assert_eq!(compute_median_i64(&[100]), 100);
    }
}
