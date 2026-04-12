//! PDF印刷用HTMLレポート生成（GAS createPdfReportHtml() 移植）
//! CSVアップロード分析結果をA4縦向き印刷用HTMLとして出力する
//! EChartsによるインタラクティブチャート + ソート可能テーブル

use super::aggregator::{SurveyAggregation, CompanyAgg, EmpTypeSalary};
use super::job_seeker::JobSeekerAnalysis;
use super::super::helpers::{escape_html, format_number};
use serde_json::json;

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
    html.push_str("</style>\n");
    // ECharts CDN
    html.push_str("<script src=\"https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js\"></script>\n");
    html.push_str("</head>\n<body>\n");

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

    // --- ECharts初期化スクリプト + ソート可能テーブル ---
    html.push_str(&render_scripts());

    html.push_str("</body>\n</html>");
    html
}

// ============================================================
// JavaScript（ECharts初期化 + ソート可能テーブル）
// ============================================================

fn render_scripts() -> String {
    r#"<script>
(function() {
  var charts = [];
  document.querySelectorAll('.echart[data-chart-config]').forEach(function(el) {
    if (el.offsetHeight === 0) return;
    try {
      var config = JSON.parse(el.getAttribute('data-chart-config'));
      config.animation = false;
      config.backgroundColor = '#fff';
      var chart = echarts.init(el, null, { renderer: 'svg' });
      chart.setOption(config);
      charts.push(chart);
    } catch(e) { console.warn('ECharts init error:', e); }
  });
  window.addEventListener('beforeprint', function() { charts.forEach(function(c) { c.resize(); }); });
  window.addEventListener('resize', function() { charts.forEach(function(c) { c.resize(); }); });
})();

function initSortableTables() {
  document.querySelectorAll('.sortable-table').forEach(function(table) {
    table.querySelectorAll('th').forEach(function(th, colIdx) {
      th.addEventListener('click', function() {
        var tbody = table.querySelector('tbody');
        if (!tbody) return;
        var rows = Array.from(tbody.querySelectorAll('tr'));
        var isAsc = th.classList.contains('sort-asc');
        table.querySelectorAll('th').forEach(function(h) { h.classList.remove('sort-asc','sort-desc'); });
        th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');
        rows.sort(function(a,b) {
          var at = a.children[colIdx] ? a.children[colIdx].textContent.trim() : '';
          var bt = b.children[colIdx] ? b.children[colIdx].textContent.trim() : '';
          var an = parseFloat(at.replace(/[,件%万円倍+]/g,''));
          var bn = parseFloat(bt.replace(/[,件%万円倍+]/g,''));
          if (!isNaN(an) && !isNaN(bn)) return isAsc ? bn-an : an-bn;
          return isAsc ? bt.localeCompare(at,'ja') : at.localeCompare(bt,'ja');
        });
        rows.forEach(function(r) { tbody.appendChild(r); });
      });
    });
  });
}
document.addEventListener('DOMContentLoaded', initSortableTables);
</script>
"#.to_string()
}

// ============================================================
// CSS
// ============================================================

fn render_css() -> String {
    r#"
:root {
  --c-primary: #1565C0;
  --c-primary-light: #42A5F5;
  --c-success: #009E73;
  --c-danger: #D55E00;
  --c-text: #1a1a2e;
  --c-text-muted: #888;
  --c-border: #e0e0e0;
  --c-bg-card: #f5f9ff;
  --shadow-card: 0 1px 3px rgba(0,0,0,0.08);
  --radius: 8px;
}

@page {
  size: A4 portrait;
  margin: 8mm 10mm;
}

* { box-sizing: border-box; }

body {
  font-family: "Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif;
  font-size: 12px;
  line-height: 1.5;
  color: var(--c-text);
  margin: 0;
  padding: 8px 16px;
  background: #fff;
}

h1 { font-size: 20px; }
h2 { font-size: 15px; margin: 12px 0 6px; border-bottom: 2px solid #2196F3; padding-bottom: 4px; color: var(--c-primary); }
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
.summary-card, .kpi-card {
  background: var(--c-bg-card);
  border: 1px solid #bbdefb;
  border-radius: var(--radius);
  padding: 10px 14px;
  text-align: center;
  position: relative;
  overflow: hidden;
  transition: transform 0.2s, box-shadow 0.2s;
}
.summary-card::before, .kpi-card::before {
  content: '';
  position: absolute;
  top: 0; left: 0; right: 0;
  height: 4px;
  background: linear-gradient(90deg, var(--c-primary), var(--c-primary-light));
}
.summary-card:hover, .kpi-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0,0,0,0.12);
}
.summary-card .label, .kpi-card .label { font-size: 11px; color: #666; margin-bottom: 2px; }
.summary-card .value, .kpi-card .value { font-size: 22px; font-weight: bold; color: var(--c-primary); }
.summary-card .unit, .kpi-card .unit { font-size: 11px; color: var(--c-text-muted); }

/* 統計ボックス 3列 */
.stats-grid {
  display: grid;
  grid-template-columns: 1fr 1fr 1fr;
  gap: 8px;
  margin-bottom: 12px;
}
.stat-box {
  background: #fafafa;
  border: 1px solid var(--c-border);
  border-radius: 4px;
  padding: 8px 12px;
  text-align: center;
}
.stat-box .label { font-size: 10px; color: var(--c-text-muted); }
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
  color: var(--c-primary);
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

/* ソート可能テーブル */
.sortable-table th { cursor: pointer; user-select: none; position: relative; padding-right: 18px; }
.sortable-table th::after { content: '\u2195'; position: absolute; right: 4px; top: 50%; transform: translateY(-50%); font-size: 10px; color: #999; opacity: 0.5; }
.sortable-table th.sort-asc::after { content: '\u25B2'; opacity: 1; color: var(--c-primary); }
.sortable-table th.sort-desc::after { content: '\u25BC'; opacity: 1; color: var(--c-primary); }

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
.guide-item .guide-title { font-weight: bold; color: var(--c-primary); font-size: 10px; margin-bottom: 2px; }

/* EChartsコンテナ */
.echart { max-width: 100%; }

/* 印刷時非表示 */
.no-print { }
@media print {
  .no-print { display: none !important; }
  body { padding: 0; }
  .section { page-break-inside: avoid; }
  .summary-card, .kpi-card { box-shadow: none !important; transform: none !important; }
  .echart { break-inside: avoid; }
  .sortable-table th::after { display: none; }
  thead { display: table-header-group; }
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
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
// ECharts config生成ヘルパー
// ============================================================

/// ヒストグラム用ECharts設定JSONを生成（平均・中央値のmarkLine付き）
fn build_histogram_echart_config(
    labels: &[String],
    values: &[usize],
    color: &str,
    mean: Option<i64>,
    median: Option<i64>,
) -> String {
    let mut mark_lines = vec![];
    if let Some(m) = mean {
        mark_lines.push(json!({
            "xAxis": format!("{}万", m / 10_000),
            "name": "平均",
            "lineStyle": {"color": "#e74c3c", "type": "dashed", "width": 2},
            "label": {"formatter": "平均", "fontSize": 10}
        }));
    }
    if let Some(m) = median {
        mark_lines.push(json!({
            "xAxis": format!("{}万", m / 10_000),
            "name": "中央値",
            "lineStyle": {"color": "#27ae60", "type": "dashed", "width": 2},
            "label": {"formatter": "中央値", "fontSize": 10}
        }));
    }

    let config = json!({
        "tooltip": {"trigger": "axis"},
        "xAxis": {
            "type": "category",
            "data": labels,
            "axisLabel": {"rotate": 30, "fontSize": 9}
        },
        "yAxis": {
            "type": "value",
            "axisLabel": {"fontSize": 9}
        },
        "grid": {"left": "10%", "right": "5%", "bottom": "20%", "top": "10%"},
        "series": [{
            "type": "bar",
            "data": values,
            "itemStyle": {"color": color},
            "markLine": {
                "data": mark_lines,
                "symbol": "none"
            }
        }]
    });
    config.to_string()
}

/// ECharts divタグを生成（data-chart-config属性付き）
fn render_echart_div(config_json: &str, height: u32) -> String {
    // シングルクォートをHTMLエンティティにエスケープ
    let escaped = config_json.replace('\'', "&#39;");
    format!(
        "<div class=\"echart\" style=\"height:{}px;width:100%;\" data-chart-config='{}'></div>\n",
        height, escaped
    )
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

    // 下限給与ヒストグラム（ECharts棒グラフ + markLine）
    if !salary_min_values.is_empty() {
        html.push_str("<h3>下限給与の分布</h3>\n");
        let (labels, values) = build_salary_histogram(salary_min_values, 20_000);
        let config = build_histogram_echart_config(&labels, &values, "#42A5F5", Some(stats.mean), Some(stats.median));
        html.push_str(&render_echart_div(&config, 220));
    }

    // 上限給与ヒストグラム（ECharts棒グラフ + markLine）
    if !salary_max_values.is_empty() {
        html.push_str("<h3>上限給与の分布</h3>\n");
        let (labels, values) = build_salary_histogram(salary_max_values, 20_000);
        let config = build_histogram_echart_config(&labels, &values, "#66BB6A", Some(stats.mean), Some(stats.median));
        html.push_str(&render_echart_div(&config, 220));
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

    // EChartsドーナツチャート TOP6
    let colors = ["#1565C0","#E69F00","#009E73","#D55E00","#CC79A7","#56B4E9"];
    let pie_data: Vec<serde_json::Value> = agg.by_employment_type.iter()
        .take(6)
        .enumerate()
        .map(|(i, (name, count))| json!({
            "value": count,
            "name": name,
            "itemStyle": {"color": colors[i % colors.len()]}
        }))
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

// ============================================================
// セクション5: 地域分析
// ============================================================

fn render_section_region(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_prefecture.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析</h2>\n");

    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr></thead>\n<tbody>\n");
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
    html.push_str("</tbody></table>\n");

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

    // 求人数ランキング TOP15（ソート可能テーブル）
    let mut by_count = by_company.to_vec();
    by_count.sort_by(|a, b| b.count.cmp(&a.count));

    html.push_str("<h3>求人数ランキング TOP15</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">求人数</th><th style=\"text-align:right\">平均月給</th></tr></thead>\n<tbody>\n");
    for (i, c) in by_count.iter().take(15).enumerate() {
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(&c.name),
            format_number(c.count as i64),
            format_man_yen(c.avg_salary),
        ));
    }
    html.push_str("</tbody></table>\n");

    // 給与ランキング TOP15（件数2件以上のみ）
    let mut by_salary: Vec<&CompanyAgg> = by_company.iter()
        .filter(|c| c.count >= 2 && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        html.push_str("<h3>給与ランキング TOP15（2件以上の企業）</h3>\n");
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">求人数</th></tr></thead>\n<tbody>\n");
        for (i, c) in by_salary.iter().take(15).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(&c.name),
                format_man_yen(c.avg_salary),
                format_number(c.count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
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
// セクション3-3: 相関分析（散布図） → ECharts scatter
// ============================================================

fn render_section_scatter(html: &mut String, agg: &SurveyAggregation) {
    if agg.scatter_min_max.len() < 6 { return; }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>相関分析（散布図）</h2>\n");
    html.push_str("<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各点が1件の求人。回帰線（赤破線）は全体傾向。\
        R²（決定係数）は0〜1で、1に近いほど相関が強い。\
    </p>\n");

    // ECharts scatter データ生成（最大200点）
    html.push_str("<h3>月給下限 vs 上限</h3>\n");

    let scatter_data: Vec<serde_json::Value> = agg.scatter_min_max.iter()
        .take(200)
        .map(|p| json!([p.x as f64 / 10_000.0, p.y as f64 / 10_000.0]))
        .collect();

    // 回帰線のmarkLine（2点指定）
    let mut series_list = vec![json!({
        "type": "scatter",
        "data": scatter_data,
        "symbolSize": 6,
        "itemStyle": {"color": "rgba(59,130,246,0.5)"}
    })];

    if let Some(reg) = &agg.regression_min_max {
        // 回帰線の2端点を計算
        let xs: Vec<f64> = agg.scatter_min_max.iter().map(|p| p.x as f64).collect();
        let x_min = xs.iter().cloned().fold(f64::INFINITY, f64::min);
        let x_max = xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let y1 = (reg.slope * x_min + reg.intercept) / 10_000.0;
        let y2 = (reg.slope * x_max + reg.intercept) / 10_000.0;
        let x1_man = x_min / 10_000.0;
        let x2_man = x_max / 10_000.0;

        series_list.push(json!({
            "type": "line",
            "data": [[x1_man, y1], [x2_man, y2]],
            "symbol": "none",
            "lineStyle": {"color": "#ef4444", "type": "dashed", "width": 2},
            "tooltip": {"show": false}
        }));
    }

    let config = json!({
        "tooltip": {
            "trigger": "item",
            "formatter": "下限: {c0}万円"
        },
        "xAxis": {
            "name": "下限（万円）",
            "nameLocation": "center",
            "nameGap": 25,
            "type": "value",
            "axisLabel": {"fontSize": 9}
        },
        "yAxis": {
            "name": "上限（万円）",
            "nameLocation": "center",
            "nameGap": 35,
            "type": "value",
            "axisLabel": {"fontSize": 9}
        },
        "grid": {"left": "12%", "right": "5%", "bottom": "15%", "top": "5%"},
        "series": series_list
    });
    html.push_str(&render_echart_div(&config.to_string(), 280));

    if let Some(reg) = &agg.regression_min_max {
        let strength = if reg.r_squared > 0.7 { "強い相関" }
            else if reg.r_squared > 0.4 { "中程度の相関" }
            else { "弱い相関" };
        html.push_str(&format!(
            "<p style=\"font-size:9px;color:#666;\">データ点: {}件 / R\u{00B2} = {:.3}（{}）</p>\n",
            agg.scatter_min_max.len(), reg.r_squared, strength
        ));
    }

    html.push_str("</div>\n");
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

    // 最低賃金との差が小さい都道府県TOP10（ソート可能テーブル）
    html.push_str("<h3>時給換算で最低賃金に近い都道府県 TOP10</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">平均月給下限</th>\
        <th style=\"text-align:right\">160h換算</th><th style=\"text-align:right\">最低賃金</th>\
        <th style=\"text-align:right\">差額</th><th style=\"text-align:right\">比率</th></tr></thead>\n<tbody>\n");
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
    html.push_str("</tbody></table>\n");

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

    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n");
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
    html.push_str("</tbody></table>\n");
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

    // タグ件数のツリーマップ（テーブルの上に配置）
    if !agg.by_tag_salary.is_empty() {
        let tree_data: Vec<serde_json::Value> = agg.by_tag_salary.iter()
            .map(|t| json!({"name": &t.tag, "value": t.count}))
            .collect();
        let config = json!({
            "tooltip": {"formatter": "{b}: {c}件"},
            "series": [{
                "type": "treemap",
                "data": tree_data,
                "roam": false,
                "label": {"show": true, "formatter": "{b}\n{c}件", "fontSize": 10},
                "breadcrumb": {"show": false},
                "levels": [{"colorSaturation": [0.3, 0.7]}]
            }]
        });
        html.push_str(&render_echart_div(&config.to_string(), 250));
    }

    if !agg.by_tag_salary.is_empty() {
        // タグ別給与差分テーブル（ソート可能・完全版）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th>\
            <th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">全体比</th></tr></thead>\n<tbody>\n");
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
        html.push_str("</tbody></table>\n");
    } else {
        // フォールバック: 件数のみテーブル（ソート可能）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th></tr></thead>\n<tbody>\n");
        for (i, (tag, count)) in agg.by_tags.iter().take(20).enumerate() {
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1, escape_html(tag), format_number(*count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
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
fn build_salary_histogram(values: &[i64], bin_size: i64) -> (Vec<String>, Vec<usize>) {
    if values.is_empty() || bin_size <= 0 {
        return (vec![], vec![]);
    }

    let valid: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    if valid.is_empty() {
        return (vec![], vec![]);
    }

    let min_val = *valid.iter().min().unwrap();
    let max_val = *valid.iter().max().unwrap();

    let start = (min_val / bin_size) * bin_size;
    let end = ((max_val / bin_size) + 1) * bin_size;

    let mut labels = Vec::new();
    let mut counts = Vec::new();

    let mut boundary = start;
    while boundary < end {
        let upper = boundary + bin_size;
        let count = valid.iter()
            .filter(|&&v| v >= boundary && v < upper)
            .count();
        labels.push(format!("{}万", boundary / 10_000));
        counts.push(count);
        boundary = upper;
    }

    (labels, counts)
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
        let (labels, counts) = build_salary_histogram(&values, 20_000);
        assert!(!labels.is_empty());
        assert_eq!(labels.len(), counts.len());
        let total: usize = counts.iter().sum();
        assert_eq!(total, 5);
    }

    #[test]
    fn test_build_salary_histogram_empty() {
        let (labels, counts) = build_salary_histogram(&[], 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
    }

    #[test]
    fn test_build_salary_histogram_zeros() {
        let values = vec![0, 0, 0];
        let (labels, counts) = build_salary_histogram(&values, 20_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
    }

    #[test]
    fn test_histogram_echart_config() {
        let labels = vec!["20万".to_string(), "22万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let config = build_histogram_echart_config(
            &labels, &values, "#42A5F5", Some(220_000), Some(215_000),
        );
        assert!(config.contains("bar"));
        assert!(config.contains("markLine"));
        assert!(config.contains("平均"));
        assert!(config.contains("中央値"));
        // JSON として妥当か
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&config);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_echart_div_output() {
        let config = r#"{"type":"bar"}"#;
        let div = render_echart_div(config, 200);
        assert!(div.contains("data-chart-config"));
        assert!(div.contains("echart"));
        assert!(div.contains("200px"));
    }

    #[test]
    fn test_render_empty_data() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[]);
        assert!(html.contains("<!DOCTYPE html>"));
        assert!(html.contains("</html>"));
        // ECharts CDN が含まれること
        assert!(html.contains("echarts"));
        // サマリーセクションは出力される
        assert!(html.contains("サマリー"));
        // ソート可能テーブルのスクリプトが含まれること
        assert!(html.contains("initSortableTables"));
    }

    #[test]
    fn test_render_scripts_contains_echart_init() {
        let scripts = render_scripts();
        assert!(scripts.contains("data-chart-config"));
        assert!(scripts.contains("echarts.init"));
        assert!(scripts.contains("initSortableTables"));
        assert!(scripts.contains("beforeprint"));
    }

    #[test]
    fn test_min_wage_all_47_prefectures() {
        // 47都道府県全てで Some を返すことを確認（地域比較の基準データ欠落検出）
        let prefectures = [
            "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県",
            "茨城県", "栃木県", "群馬県", "埼玉県", "千葉県", "東京都", "神奈川県",
            "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県", "岐阜県",
            "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府", "兵庫県",
            "奈良県", "和歌山県", "鳥取県", "島根県", "岡山県", "広島県", "山口県",
            "徳島県", "香川県", "愛媛県", "高知県", "福岡県", "佐賀県", "長崎県",
            "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
        ];
        assert_eq!(prefectures.len(), 47, "都道府県リストは47件");
        for pref in &prefectures {
            let mw = min_wage_for_prefecture(pref);
            assert!(mw.is_some(), "最低賃金データが欠落: {}", pref);
            let val = mw.unwrap();
            assert!(val >= 1000 && val <= 1300,
                "{} の最低賃金 {} が妥当範囲(1000-1300円)を逸脱", pref, val);
        }
    }

}
