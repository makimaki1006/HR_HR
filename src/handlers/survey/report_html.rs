//! PDF印刷用HTMLレポート生成（GAS createPdfReportHtml() 移植）
//! CSVアップロード分析結果をA4縦向き印刷用HTMLとして出力する

use super::aggregator::{SurveyAggregation, CompanyAgg, EmpTypeSalary, ScatterPoint, RegressionResult};
use super::job_seeker::JobSeekerAnalysis;
use super::super::helpers::{escape_html, format_number};

// ============================================================
// メイン関数
// ============================================================

/// 競合調査 PDF印刷用HTMLレポートを生成
///
/// # 引数
/// - `agg`: CSVから集計した求人データ
/// - `seeker`: 求職者心理分析結果
/// - `by_company`: 企業別集計（Step 2 で追加）
/// - `by_emp_type_salary`: 雇用形態別給与（Step 2 で追加）
/// - `salary_min_values`: 下限給与一覧（Step 2 で追加）
/// - `salary_max_values`: 上限給与一覧（Step 2 で追加）
pub(crate) fn render_survey_report_page(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
) -> String {
    let now = chrono::Local::now().format("%Y年%m月%d日 %H:%M").to_string();
    let mut html = String::with_capacity(64_000);

    // --- DOCTYPE + HEAD ---
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<title>競合調査レポート</title>\n");
    html.push_str("<style>\n");
    html.push_str(&render_css());
    html.push_str("</style>\n</head>\n<body>\n");

    // --- 印刷ボタン ---
    html.push_str("<div class=\"no-print\" style=\"text-align:right;padding:8px 16px;\">\n");
    html.push_str("<button onclick=\"window.print()\" style=\"padding:8px 24px;font-size:14px;cursor:pointer;border:1px solid #666;border-radius:4px;background:#fff;\">印刷 / PDF保存</button>\n");
    html.push_str("</div>\n");

    // --- ヘッダー ---
    html.push_str("<h1 style=\"text-align:center;margin:0 0 4px;\">競合調査レポート</h1>\n");
    html.push_str(&format!(
        "<p style=\"text-align:center;color:#666;margin:0 0 16px;font-size:12px;\">生成日時: {}</p>\n",
        escape_html(&now)
    ));

    // --- セクション1: サマリー ---
    render_section_summary(&mut html, agg);

    // --- セクション3: 給与分布 統計情報 ---
    render_section_salary_stats(&mut html, agg, salary_min_values, salary_max_values);

    // --- セクション4: 雇用形態分布 ---
    render_section_employment(&mut html, agg, by_emp_type_salary);

    // --- セクション3-3: 相関分析（散布図） ---
    render_section_scatter(&mut html, agg);

    // --- セクション5: 地域分析 ---
    render_section_region(&mut html, agg);

    // --- セクション5-2: 市区町村別給与 ---
    render_section_municipality_salary(&mut html, agg);

    // --- セクション6: 最低賃金比較 ---
    render_section_min_wage(&mut html, agg);

    // --- セクション8: 企業分析 ---
    render_section_company(&mut html, by_company);

    // --- セクション9: タグ×給与相関 ---
    render_section_tag_salary(&mut html, agg);

    // --- セクション10: 求職者心理分析 ---
    render_section_job_seeker(&mut html, seeker);

    // --- フッター ---
    html.push_str("<div class=\"section\" style=\"text-align:center;font-size:11px;color:#999;border-top:1px solid #ddd;padding-top:8px;margin-top:24px;\">\n");
    html.push_str(&format!("生成日時: {} | ", escape_html(&now)));
    html.push_str("データソース: CSVアップロード分析結果 | ");
    html.push_str("※本レポートはアップロードされたCSVデータに基づく分析です。ハローワーク掲載求人のみが対象であり、全求人市場を反映するものではありません。\n");
    html.push_str("</div>\n");

    html.push_str("</body>\n</html>");
    html
}

// ============================================================
// CSS
// ============================================================

fn render_css() -> String {
    r#"
@page {
  size: A4 portrait;
  margin: 8mm 10mm;
}

* { box-sizing: border-box; }

body {
  font-family: "Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif;
  font-size: 12px;
  line-height: 1.5;
  color: #333;
  margin: 0;
  padding: 8px 16px;
  background: #fff;
}

h1 { font-size: 20px; }
h2 { font-size: 15px; margin: 12px 0 6px; border-bottom: 2px solid #2196F3; padding-bottom: 4px; color: #1565C0; }
h3 { font-size: 13px; margin: 8px 0 4px; color: #333; }

.section {
  margin-bottom: 16px;
  page-break-inside: avoid;
}
.section-compact {
  margin-bottom: 8px;
  page-break-inside: avoid;
}

/* KPIカード 2x2 グリッド */
.summary-grid {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 8px;
  margin-bottom: 12px;
}
.summary-card {
  background: #f5f9ff;
  border: 1px solid #bbdefb;
  border-radius: 6px;
  padding: 10px 14px;
  text-align: center;
}
.summary-card .label { font-size: 11px; color: #666; margin-bottom: 2px; }
.summary-card .value { font-size: 22px; font-weight: bold; color: #1565C0; }
.summary-card .unit { font-size: 11px; color: #888; }

/* 統計ボックス 3列 */
.stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 8px;
  margin-bottom: 12px;
}
.stat-box {
  background: #fafafa;
  border: 1px solid #e0e0e0;
  border-radius: 4px;
  padding: 8px 12px;
  text-align: center;
}
.stat-box .label { font-size: 10px; color: #888; }
.stat-box .value { font-size: 18px; font-weight: bold; color: #333; }

/* 色分け */
.positive { color: #2e7d32; }
.negative { color: #c62828; }

/* グリッドレイアウト */
.two-column {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 12px;
}
.three-column {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 8px;
}

/* ボックス */
.highlight-box {
  background: #e8f5e9;
  border: 1px solid #a5d6a7;
  border-radius: 4px;
  padding: 8px 12px;
  margin-bottom: 8px;
}
.warning-box {
  background: #fff3e0;
  border: 1px solid #ffcc80;
  border-radius: 4px;
  padding: 8px 12px;
  margin-bottom: 8px;
}
.note {
  font-size: 10px;
  color: #999;
  margin-top: 4px;
}

/* テーブル */
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 11px;
  margin-bottom: 8px;
}
th {
  background: #e3f2fd;
  color: #1565C0;
  font-weight: 600;
  padding: 5px 8px;
  text-align: left;
  border-bottom: 2px solid #90caf9;
  position: sticky;
  top: 0;
}
td {
  padding: 4px 8px;
  border-bottom: 1px solid #eee;
}
tr:nth-child(even) td { background: #fafafa; }
td.num { text-align: right; font-variant-numeric: tabular-nums; }

/* 読み方ガイド */
.guide-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr 1fr;
  gap: 6px;
  margin-top: 8px;
}
.guide-item {
  background: #f9f9f9;
  border: 1px solid #eee;
  border-radius: 4px;
  padding: 6px 8px;
  font-size: 10px;
}
.guide-item .guide-title { font-weight: bold; color: #1565C0; font-size: 10px; margin-bottom: 2px; }

/* SVGチャート */
svg { max-width: 100%; height: auto; }

/* 印刷時非表示 */
.no-print { }
@media print {
  .no-print { display: none !important; }
  body { padding: 0; }
  .section { page-break-inside: avoid; }
  svg { max-width: 100% !important; }
}
"#.to_string()
}

// ============================================================
// セクション1: サマリー
// ============================================================

fn render_section_summary(html: &mut String, agg: &SurveyAggregation) {
    let salary_label = if agg.is_hourly { "平均時給" } else { "平均月給" };
    let salary_unit = if agg.is_hourly { "円" } else { "万円" };

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>サマリー</h2>\n");

    // KPIカード 2x2
    let avg_salary_display = agg.enhanced_stats.as_ref()
        .map(|s| {
            if agg.is_hourly {
                format_number(s.mean)
            } else {
                format!("{:.1}", s.mean as f64 / 10_000.0)
            }
        })
        .unwrap_or_else(|| "-".to_string());

    // 正社員率の計算
    let fulltime_count = agg.by_employment_type.iter()
        .filter(|(t, _)| t.contains("正社員") || t.contains("正職員"))
        .map(|(_, c)| c)
        .sum::<usize>();
    let fulltime_rate = if agg.total_count > 0 {
        format!("{:.1}%", fulltime_count as f64 / agg.total_count as f64 * 100.0)
    } else {
        "-".to_string()
    };

    let new_rate = if agg.total_count > 0 {
        format!("{:.1}%", agg.new_count as f64 / agg.total_count as f64 * 100.0)
    } else {
        "-".to_string()
    };

    html.push_str("<div class=\"summary-grid\">\n");
    render_summary_card(html, "総求人数", &format_number(agg.total_count as i64), "件");
    render_summary_card(html, salary_label, &avg_salary_display, salary_unit);
    render_summary_card(html, "正社員率", &fulltime_rate, "");
    render_summary_card(html, "新着率", &new_rate, "");
    html.push_str("</div>\n");

    // 読み方ガイド
    let salary_guide = if agg.is_hourly {
        "時給データとして解析されています。"
    } else {
        "全求人の月給換算平均値です。時給・年俸は月給に統一計算しています。"
    };
    html.push_str("<div class=\"guide-grid\">\n");
    render_guide_item(html, "総求人数", "CSVに含まれる求人の総数です。市場規模の目安になります。");
    render_guide_item(html, salary_label, salary_guide);
    render_guide_item(html, "正社員率", "正社員・正職員の求人割合です。高いほど安定雇用が多い市場です。");
    render_guide_item(html, "新着率", "新着求人の割合です。高いほど求人の入れ替わりが活発です。");
    html.push_str("</div>\n");

    html.push_str("</div>\n");
}

fn render_summary_card(html: &mut String, label: &str, value: &str, unit: &str) {
    html.push_str("<div class=\"summary-card\">\n");
    html.push_str(&format!("<div class=\"label\">{}</div>\n", escape_html(label)));
    html.push_str(&format!("<div class=\"value\">{}</div>\n", escape_html(value)));
    if !unit.is_empty() {
        html.push_str(&format!("<div class=\"unit\">{}</div>\n", escape_html(unit)));
    }
    html.push_str("</div>\n");
}

fn render_guide_item(html: &mut String, title: &str, description: &str) {
    html.push_str("<div class=\"guide-item\">\n");
    html.push_str(&format!("<div class=\"guide-title\">{}</div>\n", escape_html(title)));
    html.push_str(&format!("{}\n", escape_html(description)));
    html.push_str("</div>\n");
}

// ============================================================
// セクション3: 給与分布 統計情報
// ============================================================

fn render_section_salary_stats(
    html: &mut String,
    agg: &SurveyAggregation,
    salary_min_values: &[i64],
    salary_max_values: &[i64],
) {
    let stats = match &agg.enhanced_stats {
        Some(s) => s,
        None => return, // 給与データなし → セクションスキップ
    };

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>給与分布 - 統計情報</h2>\n");

    // 3カード: 平均、中央値、給与範囲
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均月給", &format_man_yen(stats.mean));
    render_stat_box(html, "中央値", &format_man_yen(stats.median));
    render_stat_box(html, "給与範囲", &format!("{} 〜 {}", format_man_yen(stats.min), format_man_yen(stats.max)));
    html.push_str("</div>\n");

    // 信頼区間・四分位がある場合
    if let Some(ci) = &stats.bootstrap_ci {
        html.push_str(&format!(
            "<p class=\"note\">95%信頼区間: {} 〜 {} (Bootstrap法, n={})</p>\n",
            format_man_yen(ci.lower), format_man_yen(ci.upper), ci.sample_size
        ));
    }

    // 下限給与ヒストグラム（統計ライン付き）
    if !salary_min_values.is_empty() {
        html.push_str("<h3>下限給与の分布</h3>\n");
        let (labels, values, boundaries) = build_salary_histogram(salary_min_values, 20_000);
        let svg = render_bar_chart_svg_with_stats(
            &labels, &values, "#42A5F5", 600, 200,
            &boundaries, Some(stats.mean), Some(stats.median),
        );
        html.push_str(&svg);
    }

    // 上限給与ヒストグラム（統計ライン付き）
    if !salary_max_values.is_empty() {
        html.push_str("<h3>上限給与の分布</h3>\n");
        let (labels, values, boundaries) = build_salary_histogram(salary_max_values, 20_000);
        let svg = render_bar_chart_svg_with_stats(
            &labels, &values, "#66BB6A", 600, 200,
            &boundaries, Some(stats.mean), Some(stats.median),
        );
        html.push_str(&svg);
    }

    html.push_str("</div>\n");
}

fn render_stat_box(html: &mut String, label: &str, value: &str) {
    html.push_str("<div class=\"stat-box\">\n");
    html.push_str(&format!("<div class=\"label\">{}</div>\n", escape_html(label)));
    html.push_str(&format!("<div class=\"value\">{}</div>\n", escape_html(value)));
    html.push_str("</div>\n");
}

// ============================================================
// セクション4: 雇用形態分布
// ============================================================

fn render_section_employment(
    html: &mut String,
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
) {
    if agg.by_employment_type.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>雇用形態分布</h2>\n");

    // 水平棒グラフ TOP6
    let colors = ["#1565C0", "#2196F3", "#42A5F5", "#64B5F6", "#90CAF9", "#BBDEFB"];
    let top6: Vec<(String, usize, String)> = agg.by_employment_type.iter()
        .take(6)
        .enumerate()
        .map(|(i, (label, count))| (label.clone(), *count, colors[i % colors.len()].to_string()))
        .collect();
    let svg = render_horizontal_bar_svg(&top6, 600, 180);
    html.push_str(&svg);

    // 雇用形態別給与テーブル
    if !by_emp_type_salary.is_empty() {
        html.push_str("<h3>雇用形態別 給与水準</h3>\n");
        html.push_str("<table>\n<tr><th>雇用形態</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">中央値</th></tr>\n");
        for e in by_emp_type_salary {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                escape_html(&e.emp_type),
                format_number(e.count as i64),
                format_man_yen(e.avg_salary),
                format_man_yen(e.median_salary),
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("</div>\n");
}

// ============================================================
// セクション5: 地域分析
// ============================================================

fn render_section_region(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析</h2>\n");

    html.push_str("<table>\n<tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr>\n");
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
    html.push_str("</table>\n");

    // 残りの都道府県数を注記
    if agg.by_prefecture.len() > 10 {
        html.push_str(&format!(
            "<p class=\"note\">※ 他{}都道府県のデータは省略しています</p>\n",
            agg.by_prefecture.len() - 10
        ));
    }

    html.push_str("</div>\n");
}

// ============================================================
// セクション8: 企業分析
// ============================================================

fn render_section_company(html: &mut String, by_company: &[CompanyAgg]) {
    if by_company.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>企業分析</h2>\n");

    // 企業数サマリー
    html.push_str(&format!(
        "<p>分析対象企業数: <strong>{}</strong>社</p>\n",
        format_number(by_company.len() as i64)
    ));

    // 求人数ランキング TOP15
    let mut by_count = by_company.to_vec();
    by_count.sort_by(|a, b| b.count.cmp(&a.count));

    html.push_str("<h3>求人数ランキング TOP15</h3>\n");
    html.push_str("<table>\n<tr><th>#</th><th>企業名</th><th style=\"text-align:right\">求人数</th><th style=\"text-align:right\">平均月給</th></tr>\n");
    for (i, c) in by_count.iter().take(15).enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&c.name),
            format_number(c.count as i64),
            format_man_yen(c.avg_salary),
        ));
    }
    html.push_str("</table>\n");

    // 給与ランキング TOP15（件数2件以上のみ）
    let mut by_salary: Vec<&CompanyAgg> = by_company.iter()
        .filter(|c| c.count >= 2 && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        html.push_str("<h3>給与ランキング TOP15（2件以上の企業）</h3>\n");
        html.push_str("<table>\n<tr><th>#</th><th>企業名</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">求人数</th></tr>\n");
        for (i, c) in by_salary.iter().take(15).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(&c.name),
                format_man_yen(c.avg_salary),
                format_number(c.count as i64),
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("</div>\n");
}

// ============================================================
// 最低賃金データ（2025年10月施行）
// ============================================================

/// 都道府県別最低賃金（円/時間）
fn min_wage_for_prefecture(pref: &str) -> Option<i64> {
    match pref {
        "北海道" => Some(1075), "青森県" => Some(1029), "岩手県" => Some(1031),
        "宮城県" => Some(1038), "秋田県" => Some(1031), "山形県" => Some(1032),
        "福島県" => Some(1038), "茨城県" => Some(1074), "栃木県" => Some(1058),
        "群馬県" => Some(1063), "埼玉県" => Some(1141), "千葉県" => Some(1140),
        "東京都" => Some(1226), "神奈川県" => Some(1225), "新潟県" => Some(1050),
        "富山県" => Some(1062), "石川県" => Some(1054), "福井県" => Some(1053),
        "山梨県" => Some(1052), "長野県" => Some(1061), "岐阜県" => Some(1065),
        "静岡県" => Some(1097), "愛知県" => Some(1140), "三重県" => Some(1087),
        "滋賀県" => Some(1080), "京都府" => Some(1122), "大阪府" => Some(1177),
        "兵庫県" => Some(1116), "奈良県" => Some(1051), "和歌山県" => Some(1045),
        "鳥取県" => Some(1030), "島根県" => Some(1033), "岡山県" => Some(1047),
        "広島県" => Some(1085), "山口県" => Some(1043), "徳島県" => Some(1046),
        "香川県" => Some(1038), "愛媛県" => Some(1033), "高知県" => Some(1023),
        "福岡県" => Some(1057), "佐賀県" => Some(1030), "長崎県" => Some(1031),
        "熊本県" => Some(1034), "大分県" => Some(1035), "宮崎県" => Some(1023),
        "鹿児島県" => Some(1026), "沖縄県" => Some(1023),
        _ => None,
    }
}

const _MIN_WAGE_NATIONAL_AVG: i64 = 1121;

// ============================================================
// セクション3-3: 相関分析（散布図）
// ============================================================

fn render_section_scatter(html: &mut String, agg: &SurveyAggregation) {
    if agg.scatter_min_max.len() < 6 { return; }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>相関分析（散布図）</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各点が1件の求人。回帰線（赤破線）は全体傾向。\
        R²（決定係数）は0〜1で、1に近いほど相関が強い。\
    </p>\n");

    // 下限 vs 上限 散布図
    html.push_str("<h3>月給下限 vs 上限</h3>\n");
    let svg = render_scatter_plot_svg(
        &agg.scatter_min_max, agg.regression_min_max.as_ref(),
        "月給下限（万円）", "月給上限（万円）", 600, 250,
    );
    html.push_str(&svg);

    if let Some(reg) = &agg.regression_min_max {
        let strength = if reg.r_squared > 0.7 { "強い相関" }
            else if reg.r_squared > 0.4 { "中程度の相関" }
            else { "弱い相関" };
        html.push_str(&format!(
            "<p style=\"font-size:9px;color:#666;\">データ点: {}件 / R² = {:.3}（{}）</p>\n",
            agg.scatter_min_max.len(), reg.r_squared, strength
        ));
    }

    html.push_str("</div>\n");
}

/// SVG散布図生成（回帰線+R²付き）
fn render_scatter_plot_svg(
    points: &[ScatterPoint], regression: Option<&RegressionResult>,
    x_label: &str, y_label: &str, width: u32, height: u32,
) -> String {
    if points.is_empty() { return String::new(); }

    let pad_l = 55u32;
    let pad_r = 20u32;
    let pad_t = 15u32;
    let pad_b = 35u32;
    let plot_w = width - pad_l - pad_r;
    let plot_h = height - pad_t - pad_b;

    // 万円換算
    let xs: Vec<f64> = points.iter().map(|p| p.x as f64 / 10_000.0).collect();
    let ys: Vec<f64> = points.iter().map(|p| p.y as f64 / 10_000.0).collect();
    let x_min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
    let x_max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_min = ys.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max = ys.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let x_range = if (x_max - x_min).abs() < 0.01 { 1.0 } else { x_max - x_min };
    let y_range = if (y_max - y_min).abs() < 0.01 { 1.0 } else { y_max - y_min };

    let to_sx = |v: f64| -> f64 { pad_l as f64 + ((v - x_min) / x_range) * plot_w as f64 };
    let to_sy = |v: f64| -> f64 { pad_t as f64 + plot_h as f64 - ((v - y_min) / y_range) * plot_h as f64 };

    let mut svg = format!(
        "<svg width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" \
         style=\"background:#fafafa;border-radius:8px;max-width:100%;\">"
    );

    // 軸線
    svg.push_str(&format!(
        "<line x1=\"{pl}\" y1=\"{b}\" x2=\"{r}\" y2=\"{b}\" stroke=\"#ccc\" stroke-width=\"1\"/>\
         <line x1=\"{pl}\" y1=\"{t}\" x2=\"{pl}\" y2=\"{b}\" stroke=\"#ccc\" stroke-width=\"1\"/>",
        pl = pad_l, b = height - pad_b, r = width - pad_r, t = pad_t
    ));

    // 目盛り（5刻み）
    for i in 0..=4 {
        let xv = x_min + x_range * i as f64 / 4.0;
        let yv = y_min + y_range * i as f64 / 4.0;
        svg.push_str(&format!(
            "<text x=\"{:.0}\" y=\"{}\" text-anchor=\"middle\" font-size=\"8\" fill=\"#999\">{:.1}</text>",
            to_sx(xv), height - pad_b + 12, xv
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{:.0}\" text-anchor=\"end\" font-size=\"8\" fill=\"#999\">{:.1}</text>",
            pad_l - 5, to_sy(yv) + 3.0, yv
        ));
    }

    // 軸ラベル
    svg.push_str(&format!(
        "<text x=\"{}\" y=\"{}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\">{}</text>",
        width / 2, height - 3, x_label
    ));
    svg.push_str(&format!(
        "<text x=\"12\" y=\"{}\" text-anchor=\"middle\" font-size=\"9\" fill=\"#666\" \
         transform=\"rotate(-90,12,{})\">{}</text>",
        height / 2, height / 2, y_label
    ));

    // データ点（最大100点サンプリング）
    let sample_rate = if points.len() > 100 { points.len() / 100 } else { 1 };
    for (i, p) in points.iter().enumerate() {
        if i % sample_rate != 0 { continue; }
        let sx = to_sx(p.x as f64 / 10_000.0);
        let sy = to_sy(p.y as f64 / 10_000.0);
        svg.push_str(&format!(
            "<circle cx=\"{sx:.1}\" cy=\"{sy:.1}\" r=\"3\" fill=\"rgba(59,130,246,0.5)\" stroke=\"#3b82f6\" stroke-width=\"0.5\"/>"
        ));
    }

    // 回帰線
    if let Some(reg) = regression {
        let y1 = reg.slope * (x_min * 10_000.0) + reg.intercept;
        let y2 = reg.slope * (x_max * 10_000.0) + reg.intercept;
        let y1d = y1 / 10_000.0;
        let y2d = y2 / 10_000.0;
        svg.push_str(&format!(
            "<line x1=\"{:.0}\" y1=\"{:.0}\" x2=\"{:.0}\" y2=\"{:.0}\" \
             stroke=\"#ef4444\" stroke-width=\"2\" stroke-dasharray=\"4,2\"/>",
            to_sx(x_min), to_sy(y1d), to_sx(x_max), to_sy(y2d)
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

// ============================================================
// セクション6: 最低賃金比較分析
// ============================================================

fn render_section_min_wage(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture_salary.is_empty() { return; }

    // 都道府県ごとに最低賃金比較データを構築
    struct MinWageEntry {
        name: String,
        avg_min: i64,
        min_wage: i64,
        hourly_160: i64,  // 月給÷160h
        diff_160: i64,
        ratio_160: f64,
    }
    let mut entries: Vec<MinWageEntry> = agg.by_prefecture_salary.iter()
        .filter_map(|p| {
            let mw = min_wage_for_prefecture(&p.name)?;
            if p.avg_min_salary <= 0 { return None; }
            let hourly_160 = p.avg_min_salary / 160;
            let diff_160 = hourly_160 - mw;
            let ratio_160 = hourly_160 as f64 / mw as f64;
            Some(MinWageEntry {
                name: p.name.clone(), avg_min: p.avg_min_salary,
                min_wage: mw, hourly_160, diff_160, ratio_160,
            })
        })
        .collect();

    if entries.is_empty() { return; }
    entries.sort_by(|a, b| a.diff_160.cmp(&b.diff_160)); // 差が小さい順

    // 全体の平均比率
    let avg_ratio: f64 = entries.iter().map(|e| e.ratio_160).sum::<f64>() / entries.len() as f64;
    let avg_diff_pct = (avg_ratio - 1.0) * 100.0;

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>最低賃金比較分析</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>月給を160h（8h×20日）で割り時給換算して最低賃金と比較。\
        全国加重平均: <strong>1,121円</strong>（2025年10月施行）\
    </p>\n");

    // 概要カード
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均最低賃金比率", &format!("{:.2}倍", avg_ratio));
    render_stat_box(html, "全体差分", &format!("{:+.1}%", avg_diff_pct));
    render_stat_box(html, "分析対象", &format!("{}都道府県", entries.len()));
    html.push_str("</div>\n");

    // 最低賃金との差が小さい都道府県TOP10
    html.push_str("<h3>時給換算で最低賃金に近い都道府県 TOP10</h3>\n");
    html.push_str("<table>\n<tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">平均月給下限</th>\
        <th style=\"text-align:right\">160h換算</th><th style=\"text-align:right\">最低賃金</th>\
        <th style=\"text-align:right\">差額</th><th style=\"text-align:right\">比率</th></tr>\n");
    for (i, e) in entries.iter().take(10).enumerate() {
        let diff_color = if e.diff_160 < 0 { "negative" } else if e.diff_160 < 50 { "color:#fb8c00;font-weight:bold" } else { "" };
        let diff_style = if diff_color.starts_with("color:") {
            format!(" style=\"text-align:right;{}\"", diff_color)
        } else {
            format!(" class=\"num {}\"", diff_color)
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}円</td>\
             <td{}>{:+}円</td><td class=\"num\">{:.2}倍</td></tr>\n",
            i + 1, escape_html(&e.name),
            format_man_yen(e.avg_min),
            format_number(e.hourly_160), format_number(e.min_wage),
            diff_style, e.diff_160, e.ratio_160,
        ));
    }
    html.push_str("</table>\n");

    // 活用ポイント
    html.push_str("<div class=\"note\">\
        <strong>活用ポイント:</strong> 160h=所定労働時間（8h×20日）で換算。\
        最低賃金水準の求人は応募者が集まりにくい傾向。+10%以上の求人を優先検討すると効率的です。\
    </div>\n");

    html.push_str("</div>\n");
}

// ============================================================
// セクション5-2: 市区町村別給与分析
// ============================================================

fn render_section_municipality_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_municipality_salary.is_empty() { return; }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>市区町村別 給与分析</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>求人数が多い市区町村の給与水準を比較。\
        同じ都道府県内でも市区町村により給与差があります。\
    </p>\n");

    html.push_str("<table>\n<tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr>\n");
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
    html.push_str("</table>\n");
    html.push_str("</div>\n");
}

// ============================================================
// セクション9: タグ×給与相関
// ============================================================

fn render_section_tag_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_tag_salary.is_empty() && agg.by_tags.is_empty() {
        return;
    }

    let overall_mean = agg.enhanced_stats.as_ref().map(|s| s.mean).unwrap_or(0);

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>タグ×給与相関分析</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各タグが付いた求人の平均給与と、全体平均との差を示します。\
        正の値（緑）=そのタグが付くと給与が高い傾向、負の値（赤）=低い傾向。\
    </p>\n");

    html.push_str(&format!(
        "<p>全体平均月給: <strong>{}</strong></p>\n",
        format_man_yen(overall_mean)
    ));

    if !agg.by_tag_salary.is_empty() {
        // タグ別給与差分テーブル（完全版）
        html.push_str("<table>\n<tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th>\
            <th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">全体比</th></tr>\n");
        for (i, ts) in agg.by_tag_salary.iter().enumerate() {
            let diff_class = if ts.diff_from_avg > 0 { "positive" } else if ts.diff_from_avg < 0 { "negative" } else { "" };
            let diff_sign = if ts.diff_from_avg > 0 { "+" } else { "" };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td>\
                 <td class=\"num {diff_class}\">{sign}{diff}万円 ({sign}{pct:.1}%)</td></tr>\n",
                i + 1,
                escape_html(&ts.tag),
                format_number(ts.count as i64),
                format_man_yen(ts.avg_salary),
                diff = format!("{:.1}", ts.diff_from_avg as f64 / 10_000.0),
                sign = diff_sign,
                pct = ts.diff_percent,
            ));
        }
        html.push_str("</table>\n");
    } else {
        // フォールバック: 件数のみテーブル
        html.push_str("<table>\n<tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th></tr>\n");
        for (i, (tag, count)) in agg.by_tags.iter().take(20).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1, escape_html(tag), format_number(*count as i64),
            ));
        }
        html.push_str("</table>\n");
    }

    html.push_str("</div>\n");
}

// ============================================================
// セクション10: 求職者心理分析
// ============================================================

fn render_section_job_seeker(html: &mut String, seeker: &JobSeekerAnalysis) {
    if seeker.total_analyzed == 0 {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>求職者心理分析</h2>\n");

    // 給与レンジ認知
    if let Some(perception) = &seeker.salary_range_perception {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>給与レンジ認知</h3>\n");

        html.push_str("<div class=\"stats-grid\">\n");
        render_stat_box(html, "平均レンジ幅", &format_man_yen(perception.avg_range_width));
        render_stat_box(html, "平均下限", &format_man_yen(perception.avg_lower));
        render_stat_box(html, "求職者期待値", &format_man_yen(perception.expected_point));
        html.push_str("</div>\n");

        // レンジ幅分類
        html.push_str("<div class=\"three-column\">\n");
        render_range_type_box(html, "狭い (<5万)", perception.narrow_count, "#e8f5e9");
        render_range_type_box(html, "中程度 (5〜10万)", perception.medium_count, "#fff8e1");
        render_range_type_box(html, "広い (>10万)", perception.wide_count, "#fce4ec");
        html.push_str("</div>\n");

        html.push_str("<p class=\"note\">※ 求職者は給与レンジの下限〜下から1/3の水準を期待する傾向があります。</p>\n");
        html.push_str("</div>\n");
    }

    // 未経験ペナルティ
    if let Some(inexp) = &seeker.inexperience_analysis {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>未経験ペナルティ</h3>\n");

        html.push_str("<div class=\"two-column\">\n");

        // 経験者
        html.push_str("<div class=\"stat-box\">\n");
        html.push_str(&format!("<div class=\"label\">経験者求人 ({}件)</div>\n", format_number(inexp.experience_count as i64)));
        if let Some(avg) = inexp.experience_avg_salary {
            html.push_str(&format!("<div class=\"value\">{}</div>\n", format_man_yen(avg)));
        } else {
            html.push_str("<div class=\"value\">-</div>\n");
        }
        html.push_str("</div>\n");

        // 未経験者
        html.push_str("<div class=\"stat-box\">\n");
        html.push_str(&format!("<div class=\"label\">未経験可求人 ({}件)</div>\n", format_number(inexp.inexperience_count as i64)));
        if let Some(avg) = inexp.inexperience_avg_salary {
            html.push_str(&format!("<div class=\"value\">{}</div>\n", format_man_yen(avg)));
        } else {
            html.push_str("<div class=\"value\">-</div>\n");
        }
        html.push_str("</div>\n");

        html.push_str("</div>\n");

        // 給与差
        if let Some(gap) = inexp.salary_gap {
            let class = if gap > 0 { "negative" } else { "positive" };
            html.push_str(&format!(
                "<div class=\"highlight-box\">経験者と未経験者の給与差: <span class=\"{}\">{}</span></div>\n",
                class,
                format_man_yen(gap),
            ));
        }

        html.push_str("</div>\n");
    }

    // 新着プレミアム
    if let Some(premium) = seeker.new_listings_premium {
        html.push_str("<div class=\"section-compact\">\n");
        html.push_str("<h3>新着プレミアム</h3>\n");
        let (class, sign) = if premium > 0 {
            ("positive", "+")
        } else if premium < 0 {
            ("negative", "")
        } else {
            ("", "")
        };
        html.push_str(&format!(
            "<div class=\"highlight-box\">新着求人 vs 既存求人の給与差: <span class=\"{}\">{}{}</span></div>\n",
            class,
            sign,
            format_man_yen(premium),
        ));
        html.push_str("<p class=\"note\">※ 新着求人は市場の最新トレンドを反映しています。プラスなら給与水準が上昇傾向です。</p>\n");
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");
}

fn render_range_type_box(html: &mut String, label: &str, count: usize, bg_color: &str) {
    html.push_str(&format!(
        "<div style=\"background:{};border:1px solid #e0e0e0;border-radius:4px;padding:6px 8px;text-align:center;\">\n",
        bg_color
    ));
    html.push_str(&format!("<div style=\"font-size:10px;color:#666;\">{}</div>\n", escape_html(label)));
    html.push_str(&format!("<div style=\"font-size:16px;font-weight:bold;\">{}件</div>\n", format_number(count as i64)));
    html.push_str("</div>\n");
}

// ============================================================
// SVGチャート生成
// ============================================================

/// SVG縦棒グラフ生成
fn render_bar_chart_svg(
    labels: &[String],
    values: &[usize],
    color: &str,
    width: u32,
    height: u32,
) -> String {
    if labels.is_empty() || values.is_empty() {
        return String::new();
    }

    let margin_left: u32 = 40;
    let margin_right: u32 = 10;
    let margin_top: u32 = 20;
    let margin_bottom: u32 = 60;
    let available_width = width.saturating_sub(margin_left + margin_right);
    let available_height = height.saturating_sub(margin_top + margin_bottom);
    let n = labels.len() as u32;
    let bar_width = ((available_width / n) as i32 - 2).max(8) as u32;
    let bar_gap = if n > 1 { (available_width - bar_width * n) / n } else { 0 };
    let max_val = *values.iter().max().unwrap_or(&1) as f64;

    let mut svg = format!(
        "<svg viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\" style=\"background:#fafafa;border-radius:4px;\">\n",
        width, height
    );

    // 背景グリッド線（5本）
    for i in 0..=4 {
        let y = margin_top + available_height - (available_height * i / 4);
        let val = (max_val * i as f64 / 4.0) as i64;
        svg.push_str(&format!(
            "<line x1=\"{}\" y1=\"{}\" x2=\"{}\" y2=\"{}\" stroke=\"#e0e0e0\" stroke-width=\"0.5\"/>\n",
            margin_left, y, width - margin_right, y
        ));
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"9\" fill=\"#999\" text-anchor=\"end\">{}</text>\n",
            margin_left - 4, y + 3, val
        ));
    }

    // バー描画
    let show_label_every = if bar_width < 15 {
        (15.0 / bar_width as f64).ceil() as usize
    } else {
        1
    };

    for (i, (label, &val)) in labels.iter().zip(values.iter()).enumerate() {
        let x = margin_left + (i as u32) * (bar_width + bar_gap) + bar_gap / 2;
        let bar_h = if max_val > 0.0 {
            (val as f64 / max_val * available_height as f64) as u32
        } else {
            0
        };
        let y = margin_top + available_height - bar_h;

        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"{}\" rx=\"2\"/>\n",
            x, y, bar_width, bar_h, color
        ));

        // 値ラベル（バー幅12px以上）
        if bar_width >= 12 && val > 0 {
            svg.push_str(&format!(
                "<text x=\"{}\" y=\"{}\" font-size=\"8\" fill=\"#333\" text-anchor=\"middle\">{}</text>\n",
                x + bar_width / 2, y.saturating_sub(3), val
            ));
        }

        // X軸ラベル（間引き対応）
        if i % show_label_every == 0 {
            if bar_width < 15 {
                // 回転ラベル
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"8\" fill=\"#666\" text-anchor=\"end\" transform=\"rotate(-45,{},{})\">{}</text>\n",
                    x + bar_width / 2, margin_top + available_height + 12,
                    x + bar_width / 2, margin_top + available_height + 12,
                    escape_html(label)
                ));
            } else {
                svg.push_str(&format!(
                    "<text x=\"{}\" y=\"{}\" font-size=\"9\" fill=\"#666\" text-anchor=\"middle\">{}</text>\n",
                    x + bar_width / 2, margin_top + available_height + 14,
                    escape_html(label)
                ));
            }
        }
    }

    svg.push_str("</svg>\n");
    svg
}

/// SVG水平棒グラフ生成
fn render_horizontal_bar_svg(
    items: &[(String, usize, String)],  // (ラベル, 値, 色)
    width: u32,
    height: u32,
) -> String {
    if items.is_empty() {
        return String::new();
    }

    let margin_left: u32 = 120;
    let margin_right: u32 = 60;
    let margin_top: u32 = 10;
    let margin_bottom: u32 = 10;
    let n = items.len() as u32;
    let available_width = width.saturating_sub(margin_left + margin_right);
    let available_height = height.saturating_sub(margin_top + margin_bottom);
    let bar_height = (available_height / n).min(28);
    let bar_gap = if n > 1 { (available_height - bar_height * n) / n } else { 4 };
    let max_val = items.iter().map(|(_, v, _)| *v).max().unwrap_or(1) as f64;

    let mut svg = format!(
        "<svg viewBox=\"0 0 {} {}\" xmlns=\"http://www.w3.org/2000/svg\" style=\"background:#fafafa;border-radius:4px;\">\n",
        width, height
    );

    // グラデーション定義
    svg.push_str("<defs>\n");
    for (i, (_, _, color)) in items.iter().enumerate() {
        svg.push_str(&format!(
            "<linearGradient id=\"hgrad{}\" x1=\"0%\" y1=\"0%\" x2=\"100%\" y2=\"0%\">\
             <stop offset=\"0%\" style=\"stop-color:{};stop-opacity:0.9\"/>\
             <stop offset=\"100%\" style=\"stop-color:{};stop-opacity:0.6\"/>\
             </linearGradient>\n",
            i, color, color
        ));
    }
    svg.push_str("</defs>\n");

    for (i, (label, val, _)) in items.iter().enumerate() {
        let y = margin_top + (i as u32) * (bar_height + bar_gap) + bar_gap / 2;
        let bar_w = if max_val > 0.0 {
            (*val as f64 / max_val * available_width as f64) as u32
        } else {
            0
        };

        // ラベル（左側）
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#333\" text-anchor=\"end\" dominant-baseline=\"middle\">{}</text>\n",
            margin_left - 6, y + bar_height / 2,
            escape_html(label)
        ));

        // バー
        svg.push_str(&format!(
            "<rect x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" fill=\"url(#hgrad{})\" rx=\"3\"/>\n",
            margin_left, y, bar_w, bar_height, i
        ));

        // 件数（バーの右）
        svg.push_str(&format!(
            "<text x=\"{}\" y=\"{}\" font-size=\"10\" fill=\"#333\" dominant-baseline=\"middle\">{}件</text>\n",
            margin_left + bar_w + 4, y + bar_height / 2,
            format_number(*val as i64)
        ));
    }

    svg.push_str("</svg>\n");
    svg
}

// ============================================================
// ヘルパー関数
// ============================================================

/// 給与を万円表示にフォーマット
/// 例: 250000 → "25.0万円", 0 → "-"
fn format_man_yen(yen: i64) -> String {
    if yen == 0 {
        return "-".to_string();
    }
    format!("{:.1}万円", yen as f64 / 10_000.0)
}

/// ヒストグラム用バケット集計
/// 給与値配列をbin_size刻みでバケットに分類し、ラベルと件数を返す
/// ヒストグラムデータ構築（bin境界値付き）
fn build_salary_histogram(values: &[i64], bin_size: i64) -> (Vec<String>, Vec<usize>, Vec<i64>) {
    if values.is_empty() || bin_size <= 0 {
        return (vec![], vec![], vec![]);
    }

    let valid: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    if valid.is_empty() {
        return (vec![], vec![], vec![]);
    }

    let min_val = *valid.iter().min().unwrap();
    let max_val = *valid.iter().max().unwrap();

    let start = (min_val / bin_size) * bin_size;
    let end = ((max_val / bin_size) + 1) * bin_size;

    let mut labels = Vec::new();
    let mut counts = Vec::new();
    let mut boundaries = Vec::new();

    let mut boundary = start;
    while boundary < end {
        let upper = boundary + bin_size;
        let count = valid.iter()
            .filter(|&&v| v >= boundary && v < upper)
            .count();
        labels.push(format!("{}万", boundary / 10_000));
        counts.push(count);
        boundaries.push(boundary);
        boundary = upper;
    }

    (labels, counts, boundaries)
}

/// 統計ライン付きSVG棒グラフ（平均=赤破線、中央値=緑破線）
fn render_bar_chart_svg_with_stats(
    labels: &[String], values: &[usize], color: &str,
    width: u32, height: u32,
    boundaries: &[i64], mean: Option<i64>, median: Option<i64>,
) -> String {
    if labels.is_empty() { return String::new(); }

    // まず基本棒グラフを生成（</svg>を除く）
    let base = render_bar_chart_svg(labels, values, color, width, height);
    let trimmed = base.trim_end();
    let close_tag = "</svg>";
    let base_without_close = if trimmed.ends_with(close_tag) {
        &trimmed[..trimmed.len() - close_tag.len()]
    } else {
        return base;
    };

    let mut svg = base_without_close.to_string();

    // 統計ラインの描画ヘルパー
    let left_margin = 40u32;
    let right_margin = 10u32;
    let available_width = width - left_margin - right_margin;
    let bar_w = if labels.is_empty() { 20 } else {
        std::cmp::max(8, available_width as usize / labels.len() - 2)
    };
    let top = 20u32;
    let bottom = height - 60;

    let stat_line = |value: i64, line_color: &str, label_text: &str| -> String {
        if boundaries.is_empty() { return String::new(); }
        // valueに最も近いbinを見つけて補間
        let mut idx = boundaries.len() - 1;
        for (i, &b) in boundaries.iter().enumerate() {
            if b >= value { idx = i; break; }
        }
        let x = if idx == 0 || boundaries[idx] == value {
            left_margin as f64 + idx as f64 * (bar_w + 2) as f64 + bar_w as f64 / 2.0
        } else {
            let ratio = (value - boundaries[idx - 1]) as f64
                / (boundaries[idx] - boundaries[idx - 1]) as f64;
            let x1 = left_margin as f64 + (idx - 1) as f64 * (bar_w + 2) as f64 + bar_w as f64 / 2.0;
            let x2 = left_margin as f64 + idx as f64 * (bar_w + 2) as f64 + bar_w as f64 / 2.0;
            x1 + ratio * (x2 - x1)
        };
        format!(
            "<line x1=\"{x:.0}\" y1=\"{top}\" x2=\"{x:.0}\" y2=\"{bottom}\" \
             stroke=\"{line_color}\" stroke-width=\"2\" stroke-dasharray=\"5,3\"/>\
             <text x=\"{x:.0}\" y=\"{lt}\" text-anchor=\"middle\" font-size=\"9\" \
             fill=\"{line_color}\" font-weight=\"bold\">{label_text}</text>",
            lt = top - 4,
        )
    };

    if let Some(m) = mean {
        svg.push_str(&stat_line(m, "#e74c3c", "平均"));
    }
    if let Some(m) = median {
        svg.push_str(&stat_line(m, "#27ae60", "中央"));
    }

    // 凡例
    let legend_x = width - 160;
    svg.push_str(&format!(
        "<g transform=\"translate({legend_x},5)\">\
         <rect x=\"0\" y=\"0\" width=\"150\" height=\"18\" fill=\"white\" fill-opacity=\"0.85\" rx=\"3\"/>\
         <line x1=\"5\" y1=\"9\" x2=\"18\" y2=\"9\" stroke=\"#e74c3c\" stroke-width=\"2\" stroke-dasharray=\"5,3\"/>\
         <text x=\"22\" y=\"12\" font-size=\"8\">平均</text>\
         <line x1=\"55\" y1=\"9\" x2=\"68\" y2=\"9\" stroke=\"#27ae60\" stroke-width=\"2\" stroke-dasharray=\"5,3\"/>\
         <text x=\"72\" y=\"12\" font-size=\"8\">中央値</text></g>"
    ));

    svg.push_str("</svg>");
    svg
}

// ============================================================
// テスト
// ============================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_man_yen() {
        assert_eq!(format_man_yen(250_000), "25.0万円");
        assert_eq!(format_man_yen(183_500), "18.4万円");
        assert_eq!(format_man_yen(0), "-");
        assert_eq!(format_man_yen(-50_000), "-5.0万円");
    }

    #[test]
    fn test_build_salary_histogram() {
        let values = vec![200_000, 210_000, 250_000, 270_000, 300_000];
        let (labels, counts, boundaries) = build_salary_histogram(&values, 20_000);
        assert!(!labels.is_empty());
        assert_eq!(labels.len(), counts.len());
        assert_eq!(labels.len(), boundaries.len());
        let total: usize = counts.iter().sum();
        assert_eq!(total, 5);
        // bin境界が昇順であること
        for w in boundaries.windows(2) {
            assert!(w[0] < w[1]);
        }
    }

    #[test]
    fn test_build_salary_histogram_empty() {
        let (labels, counts, boundaries) = build_salary_histogram(&[], 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_build_salary_histogram_zeros() {
        let values = vec![0, 0, 0];
        let (labels, counts, boundaries) = build_salary_histogram(&values, 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
        assert!(boundaries.is_empty());
    }

    #[test]
    fn test_render_bar_chart_with_stats() {
        let labels = vec!["20万".to_string(), "22万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let boundaries = vec![200_000, 220_000, 240_000];
        let svg = render_bar_chart_svg_with_stats(
            &labels, &values, "#42A5F5", 600, 200,
            &boundaries, Some(220_000), Some(215_000),
        );
        assert!(svg.contains("stroke-dasharray"));
        assert!(svg.contains("平均"));
        assert!(svg.contains("中央"));
    }

    #[test]
    fn test_render_bar_chart_svg_not_empty() {
        let labels = vec!["10万".to_string(), "20万".to_string(), "30万".to_string()];
        let values = vec![5, 12, 8];
        let svg = render_bar_chart_svg(&labels, &values, "#42A5F5", 400, 200);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("</svg>"));
        assert!(svg.contains("rect"));
    }

    #[test]
    fn test_render_horizontal_bar_svg_not_empty() {
        let items = vec![
            ("正社員".to_string(), 100, "#1565C0".to_string()),
            ("パート".to_string(), 60, "#2196F3".to_string()),
        ];
        let svg = render_horizontal_bar_svg(&items, 500, 100);
        assert!(svg.contains("<svg"));
        assert!(svg.contains("linearGradient"));
    }

    #[test]
    fn test_render_empty_data() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[]);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        // 空データでもサマリーセクションは出力される
        assert!(html.contains("サマリー"));
    }
}
