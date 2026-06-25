//! Section 07.5 - 求人ボックス 年間休日 × 給与 詳細 (2026-06-24 追加)
//!
//! 求人ボックス CSV の `p-result_lines` (description) から年間休日を抽出し、
//! ① カテゴリ分布 ② 給与帯別 平均年間休日 ③ 企業名×年間休日×給与 個別求人一覧
//! の 3 ブロックを Section 07 (Lifestyle) と Section 08 (Notes) の間に挿入する。
//!
//! 移植元: Google Apps Script `Aggregator.js:createAnnualHolidaysAggregation` +
//!         `ApiHandler.js:annualHolidaysData` セクション。
//! GAS には無い「個別求人一覧」(表 7.5-C) は V2 独自拡張 (ユーザー要望 2026-06-24)。
//!
//! 適用条件:
//!   - Indeed CSV (年間休日カラムなし) では `agg.annual_holidays_values` が空 → 自動スキップ
//!   - 求人ボックス CSV で 1 件でも年間休日が抽出されていれば表示

#![allow(dead_code)]

// パス解析 (現在位置: survey::report_html::navy_report::section_07_5_jobbox_detail):
//   super              = navy_report
//   super::super       = report_html
//   super::super::super = survey
//   super::super::super::super = handlers
use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::SurveyAggregation;
use super::common::push_page_head;

/// 求人ボックス 年間休日 × 給与 詳細セクションを描画。
///
/// `agg.jobbox_records` と `agg.annual_holidays_values` の両方が空ならスキップ。
pub(crate) fn render_navy_section_jobbox_detail(html: &mut String, agg: &SurveyAggregation) {
    // 診断用 HTML コメント: 関数呼出の確証 + 早期 return 判定の可視化
    // (本番で Section 07.5 が表示されない原因切り分け用、2026-06-25)
    html.push_str(&format!(
        "<!-- Section 07.5 DIAG: total_count={}, annual_holidays_values.len={}, jobbox_records.len={}, salary_vs_holidays_scatter.len={}, category_distribution.len={} -->\n",
        agg.total_count,
        agg.annual_holidays_values.len(),
        agg.jobbox_records.len(),
        agg.salary_vs_holidays_scatter.len(),
        agg.annual_holidays_category_distribution.len(),
    ));
    // 診断 2: SurveyAggregation の他フィールド (集計の代理指標)
    //   by_company が 0 = source 判定や CSV 解釈が失敗、salary_min_values が 0 = 給与パース失敗、等を切り分け
    html.push_str(&format!(
        "<!-- Section 07.5 DIAG-2: by_company.len={}, by_emp_type_salary.len={}, salary_min_values.len={}, by_prefecture.len={}, salary_values.len={}, dominant_pref={:?}, dominant_muni={:?} -->\n",
        agg.by_company.len(),
        agg.by_emp_type_salary.len(),
        agg.salary_min_values.len(),
        agg.by_prefecture.len(),
        agg.salary_values.len(),
        agg.dominant_prefecture,
        agg.dominant_municipality,
    ));

    if agg.annual_holidays_values.is_empty() && agg.jobbox_records.is_empty() {
        html.push_str("<!-- Section 07.5 SKIPPED: both annual_holidays_values and jobbox_records are empty -->\n");
        return;
    }

    html.push_str("<section class=\"page-navy navy-jobbox-detail\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 07.5",
        "求人ボックス 年間休日 × 給与 詳細",
        "求人ボックスCSV の description テキストから年間休日数を抽出し、企業別 / 給与帯別に分析",
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
        html.push_str(
            "<p class=\"so-what\">120 日以上 (週休2日+祝日相当) の比率が高いほど、\
             公休面で魅力的な求人が多い市場と言えます。</p>\n",
        );
    }

    // -- 表 7.5-B 給与帯別 平均年間休日 (月給/年俸 のみ、5万円刻みビン)
    if !agg.salary_vs_holidays_scatter.is_empty() {
        let mut bins: std::collections::BTreeMap<i64, Vec<i64>> = std::collections::BTreeMap::new();
        for p in &agg.salary_vs_holidays_scatter {
            let bin = (p.x / 50_000) * 50_000;
            bins.entry(bin).or_default().push(p.y);
        }
        if !bins.is_empty() {
            html.push_str(
                "<div class=\"block-title\">表 7.5-B &nbsp;給与帯別 平均年間休日 \
                 (月給/年俸のみ、5万円刻み)</div>\n",
            );
            html.push_str(
                "<table class=\"table-navy\">\n<thead><tr>\
                           <th>月給帯</th>\
                           <th style=\"text-align:right;\">件数</th>\
                           <th style=\"text-align:right;\">平均年間休日</th>\
                           <th style=\"text-align:right;\">中央値</th>\
                           </tr></thead>\n<tbody>\n",
            );
            for (bin, holidays) in &bins {
                let avg = (holidays.iter().sum::<i64>() as f64) / (holidays.len() as f64);
                let med = compute_median_i64(holidays);
                html.push_str(&format!(
                    "<tr><td>月給 {} 万〜</td>\
                     <td style=\"text-align:right;\">{}</td>\
                     <td style=\"text-align:right;\">{:.1} 日</td>\
                     <td style=\"text-align:right;\">{} 日</td></tr>\n",
                    bin / 10_000,
                    holidays.len(),
                    avg,
                    med,
                ));
            }
            html.push_str("</tbody></table>\n");
            html.push_str(
                "<p class=\"so-what\">給与水準と年間休日の関係を確認できます。\
                 一般に給与帯が上がるほど年間休日も増える傾向がありますが、\
                 業種・職種により例外があります (相関 ≠ 因果)。</p>\n",
            );
        }
    }

    // -- 表 7.5-C 個別求人一覧 (年間休日抽出成功分)
    if !agg.jobbox_records.is_empty() {
        let total_jobbox = agg.jobbox_records.len();
        const TABLE_LIMIT: usize = 300;
        let limit = total_jobbox.min(TABLE_LIMIT);
        html.push_str(
            "<div class=\"block-title\">表 7.5-C &nbsp;個別求人一覧 \
             (年間休日抽出成功分、年間休日降順)</div>\n",
        );
        html.push_str(
            "<p class=\"so-what\">企業名・求人タイトル・勤務地・雇用形態・年間休日・\
             給与下限/上限・URL を並列表示。GAS には無い V2 独自機能。</p>\n",
        );
        html.push_str(
            "<table class=\"table-navy\">\n<thead><tr>\
                       <th>企業名</th>\
                       <th>求人タイトル</th>\
                       <th>勤務地</th>\
                       <th>雇用</th>\
                       <th style=\"text-align:right;\">年間休日</th>\
                       <th style=\"text-align:right;\">給与下限</th>\
                       <th style=\"text-align:right;\">給与上限</th>\
                       <th>単位</th>\
                       <th>URL</th>\
                       </tr></thead>\n<tbody>\n",
        );
        for rec in &agg.jobbox_records[..limit] {
            let url_cell = match rec.url.as_deref() {
                Some(u) if u.starts_with("http://") || u.starts_with("https://") => format!(
                    "<a href=\"{}\" target=\"_blank\" rel=\"noopener\">開く &#x2197;</a>",
                    escape_html(u)
                ),
                _ => "-".to_string(),
            };
            let s_min = rec
                .salary_min
                .map(|v| format!("{} 円", format_number(v)))
                .unwrap_or_else(|| "-".to_string());
            let s_max = rec
                .salary_max
                .map(|v| format!("{} 円", format_number(v)))
                .unwrap_or_else(|| "-".to_string());
            html.push_str(&format!(
                "<tr>\
                 <td>{company}</td>\
                 <td>{title}</td>\
                 <td>{loc}</td>\
                 <td>{emp}</td>\
                 <td style=\"text-align:right;\">{hol} 日</td>\
                 <td style=\"text-align:right;\">{smin}</td>\
                 <td style=\"text-align:right;\">{smax}</td>\
                 <td>{unit}</td>\
                 <td>{url}</td>\
                 </tr>\n",
                company = escape_html(&rec.company_name),
                title = escape_html(&rec.job_title),
                loc = escape_html(&rec.location),
                emp = escape_html(&rec.employment_type),
                hol = rec.annual_holidays,
                smin = s_min,
                smax = s_max,
                unit = escape_html(&rec.salary_unit),
                url = url_cell,
            ));
        }
        html.push_str("</tbody></table>\n");
        if total_jobbox > limit {
            html.push_str(&format!(
                "<p class=\"note\">全 {} 件のうち上位 {} 件を表示中 (年間休日降順 → 企業名昇順)。\
                 印刷物のサイズ制約のため上限あり。</p>\n",
                format_number(total_jobbox as i64),
                format_number(limit as i64),
            ));
        }
    }

    html.push_str("</section>\n");
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
        assert!(html.contains("テスト株式会社"), "company name rendered");
        assert!(html.contains("120 日"), "annual holidays value rendered");
        assert!(html.contains("250,000 円"), "salary_min rendered");
        assert!(html.contains("https://example.com/job/1"), "url rendered");
        assert!(html.contains("分布バー"), "category distribution rendered");
    }

    #[test]
    fn skips_when_no_jobbox_data() {
        let mut html = String::new();
        render_navy_section_jobbox_detail(&mut html, &SurveyAggregation::default());
        assert!(html.is_empty(), "empty agg → section not rendered");
    }

    #[test]
    fn median_helper_basic() {
        assert_eq!(compute_median_i64(&[1, 2, 3, 4, 5]), 3);
        assert_eq!(compute_median_i64(&[1, 2, 3, 4]), 2);
        assert_eq!(compute_median_i64(&[]), 0);
        assert_eq!(compute_median_i64(&[100]), 100);
    }
}
