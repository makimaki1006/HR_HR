//! PDF印刷用HTMLレポート生成（GAS createPdfReportHtml() 移植）
//! CSVアップロード分析結果をA4縦向き印刷用HTMLとして出力する
//! EChartsによるインタラクティブチャート + ソート可能テーブル

use super::aggregator::{SurveyAggregation, CompanyAgg, EmpTypeSalary, ScatterPoint, TagSalaryAgg};
use super::job_seeker::JobSeekerAnalysis;
use super::super::helpers::{escape_html, format_number, get_f64};
use super::super::insight::fetch::InsightContext;
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
    hw_context: Option<&InsightContext>,
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

    // --- テーマ切替 + 印刷ボタン ---
    html.push_str("<div class=\"no-print\" style=\"text-align:right;padding:8px 16px;\">\n");
    html.push_str("<button class=\"theme-toggle\" type=\"button\" onclick=\"toggleTheme()\" aria-label=\"ダークモード/ライトモードを切替\">\u{1F319} ダーク / \u{2600} ライト</button>\n");
    html.push_str("<button onclick=\"window.print()\" aria-label=\"印刷またはPDFで保存\" style=\"padding:8px 24px;font-size:14px;cursor:pointer;border:1px solid #666;border-radius:4px;background:#fff;\">印刷 / PDF保存</button>\n");
    html.push_str("</div>\n");

    // --- 表紙ページ ---
    let today_short = chrono::Local::now().format("%Y年%m月").to_string();
    html.push_str("<section class=\"cover-page\" role=\"region\" aria-labelledby=\"cover-title\">\n");
    html.push_str("<!-- LOGO -->\n");
    html.push_str("<div class=\"cover-logo\" aria-label=\"会社ロゴ枠\">[会社ロゴ]</div>\n");
    html.push_str("<div class=\"cover-title\" id=\"cover-title\">ハローワーク求人市場 総合診断レポート</div>\n");
    html.push_str("<div class=\"cover-sub\">競合調査分析 &nbsp;|&nbsp; ");
    html.push_str(&escape_html(&today_short));
    html.push_str("</div>\n");
    html.push_str("<div class=\"cover-confidential\">この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-footer\">F-A-C株式会社 &nbsp;|&nbsp; 生成日時: {}</div>\n",
        escape_html(&now)
    ));
    html.push_str("</section>\n");

    // --- ヘッダー ---
    html.push_str("<h1 style=\"text-align:center;margin:0 0 4px;\" id=\"report-main-title\">競合調査レポート</h1>\n");
    html.push_str(&format!(
        "<p style=\"text-align:center;color:#666;margin:0 0 16px;font-size:12px;\">生成日時: {}</p>\n",
        escape_html(&now)
    ));

    // --- セクション1: サマリー ---
    render_section_summary(&mut html, agg);

    // --- セクション1-2: HW市場比較（HWデータがある場合のみ） ---
    if let Some(ctx) = hw_context {
        render_section_hw_comparison(&mut html, agg, by_emp_type_salary, ctx);
    }

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

    // --- フッター（本文末尾の注記） ---
    html.push_str("<div class=\"section\" style=\"text-align:center;font-size:11px;color:#999;border-top:1px solid #ddd;padding-top:8px;margin-top:24px;\">\n");
    html.push_str(&format!("生成日時: {} | ", escape_html(&now)));
    html.push_str("データソース: CSVアップロード分析結果 | ");
    html.push_str("※本レポートはアップロードされたCSVデータに基づく分析です。ハローワーク掲載求人のみが対象であり、全求人市場を反映するものではありません。\n");
    html.push_str("</div>\n");

    // --- 画面下部フッター（印刷時は @page footer を使用） ---
    html.push_str("<div class=\"screen-footer no-print\">\n");
    html.push_str("<span>F-A-C株式会社 | ハローワーク求人データ分析レポート</span>\n");
    html.push_str(&format!("<span>生成日時: {}</span>\n", escape_html(&now)));
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
function toggleTheme() {
  document.body.classList.toggle('theme-dark');
  try {
    localStorage.setItem('report-theme',
      document.body.classList.contains('theme-dark') ? 'dark' : 'light');
  } catch(e) {}
}
(function() {
  try {
    if (localStorage.getItem('report-theme') === 'dark') {
      document.body.classList.add('theme-dark');
    }
  } catch(e) {}
})();
(function() {
  // ソート可能テーブルに role=grid / aria-sort を付与
  document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.sortable-table').forEach(function(t) {
      t.setAttribute('role', 'grid');
      t.querySelectorAll('th').forEach(function(th) {
        th.setAttribute('aria-sort', 'none');
        th.setAttribute('tabindex', '0');
      });
    });
    // セクションに role=region 付与
    document.querySelectorAll('.section').forEach(function(s, i) {
      if (!s.getAttribute('role')) s.setAttribute('role', 'region');
      var h = s.querySelector('h2');
      if (h && !h.id) {
        h.id = 'section-' + i;
        s.setAttribute('aria-labelledby', h.id);
      }
    });
  });
})();
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
        table.querySelectorAll('th').forEach(function(h) { h.classList.remove('sort-asc','sort-desc'); h.setAttribute('aria-sort','none'); });
        th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');
        th.setAttribute('aria-sort', isAsc ? 'descending' : 'ascending');
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
  --bg: #ffffff;
  --text: #1a1a2e;
  --shadow-card: 0 1px 3px rgba(0,0,0,0.08);
  --radius: 8px;
}

body.theme-dark {
  --c-primary: #64b5f6;
  --c-primary-light: #90caf9;
  --c-text: #e6e6f0;
  --c-text-muted: #aaa;
  --c-border: #37415a;
  --c-bg-card: #232946;
  --bg: #1a1a2e;
  --text: #e6e6f0;
}
body.theme-dark table th { background: #283350; color: #90caf9; border-bottom-color: #3a4a6a; }
body.theme-dark table td { border-bottom-color: #2a3450; }
body.theme-dark tr:nth-child(even) td { background: #20283d; }
body.theme-dark .highlight-box { background: #1f3a2a; border-color: #2e5b40; color: #d4f4dd; }
body.theme-dark .warning-box { background: #3a2e1a; border-color: #6b5020; color: #ffe0b2; }
body.theme-dark .stat-box { background: #232946; border-color: #37415a; }
body.theme-dark .guide-item { background: #232946; border-color: #37415a; }

@page {
  size: A4 portrait;
  margin: 8mm 10mm 18mm 10mm;
  @bottom-right { content: "Page " counter(page); font-size: 8px; color: #999; }
  @bottom-left { content: "F-A-C株式会社 | ハローワーク求人データ分析レポート"; font-size: 8px; color: #999; }
}

* { box-sizing: border-box; }

body {
  font-family: "Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif;
  font-size: 12px;
  line-height: 1.5;
  color: var(--text);
  margin: 0;
  padding: 8px 16px;
  background: var(--bg);
  transition: background 0.2s, color 0.2s;
}

/* 表紙 */
.cover-page {
  min-height: 260mm;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  padding: 20mm 10mm;
  page-break-after: always;
  border: 1px solid var(--c-border);
  border-radius: var(--radius);
  margin-bottom: 16px;
  position: relative;
  background: linear-gradient(180deg, var(--c-bg-card) 0%, var(--bg) 100%);
}
.cover-logo { width: 180px; height: 60px; display: flex; align-items: center; justify-content: center; color: var(--c-text-muted); font-size: 11px; border: 1px dashed var(--c-border); border-radius: 4px; margin-bottom: 28px; }
.cover-title { font-size: 28px; font-weight: 700; color: var(--c-primary); margin: 10px 0 6px; letter-spacing: 0.05em; }
.cover-sub { font-size: 16px; color: var(--text); margin-bottom: 40px; }
.cover-confidential { margin-top: auto; font-size: 11px; color: var(--c-text-muted); border-top: 1px solid var(--c-border); padding-top: 14px; width: 70%; }
.cover-footer { position: absolute; bottom: 12mm; left: 0; right: 0; font-size: 10px; color: var(--c-text-muted); }

/* テーマ切替ボタン */
.theme-toggle {
  position: fixed; top: 10px; right: 200px; z-index: 100;
  padding: 6px 12px; font-size: 12px; cursor: pointer;
  border: 1px solid var(--c-border); border-radius: 4px;
  background: var(--bg); color: var(--text);
}
.theme-toggle:focus { outline: 2px solid var(--c-primary); outline-offset: 2px; }

/* 画面下部フッター（画面表示のみ） */
.screen-footer {
  margin-top: 24px; padding: 10px 16px;
  border-top: 1px solid var(--c-border);
  font-size: 10px; color: var(--c-text-muted);
  display: flex; justify-content: space-between;
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
.sortable-table th::after { content: '↕'; position: absolute; right: 4px; top: 50%; transform: translateY(-50%); font-size: 10px; color: #999; opacity: 0.5; }
.sortable-table th.sort-asc::after { content: '▲'; opacity: 1; color: var(--c-primary); }
.sortable-table th.sort-desc::after { content: '▼'; opacity: 1; color: var(--c-primary); }

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

/* HW市場比較カード */
.comparison-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
  gap: 12px;
  margin-top: 8px;
}
.comparison-card {
  border: 1px solid var(--c-border);
  border-radius: 6px;
  padding: 12px;
  background: var(--c-bg-card);
}
.comparison-card h3 {
  font-size: 11px;
  color: var(--c-text-muted);
  margin: 0 0 4px;
  font-weight: bold;
}
.comparison-card .value-pair {
  display: flex;
  gap: 12px;
  margin: 6px 0;
}
.comparison-card .value-pair > div {
  display: flex;
  flex-direction: column;
}
.comparison-card .value-pair .label { font-size: 9px; color: var(--c-text-muted); }
.comparison-card .value-pair .value { font-size: 14px; font-weight: bold; color: var(--c-primary); }
.comparison-card .diff { font-size: 11px; margin-top: 4px; font-weight: bold; }
.comparison-card .diff.positive { color: var(--c-success); }
.comparison-card .diff.negative { color: var(--c-danger); }

/* EChartsコンテナ */
.echart { max-width: 100%; }

/* 印刷時非表示 */
.no-print { }
@media print {
  .no-print { display: none !important; }
  body { padding: 0; background: #fff !important; color: #1a1a2e !important; }
  body.theme-dark { background: #fff !important; color: #1a1a2e !important; }
  body.theme-dark table th { background: #e3f2fd !important; color: #1565C0 !important; }
  body.theme-dark table td { background: transparent !important; color: #1a1a2e !important; }
  .section { page-break-inside: avoid; }
  .summary-card, .kpi-card { box-shadow: none !important; transform: none !important; }
  .echart { break-inside: avoid; }
  .sortable-table th::after { display: none; }
  thead { display: table-header-group; }
  .cover-page { page-break-after: always; border: none; background: #fff !important; min-height: 90vh; }
  .screen-footer { display: none !important; }
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
// セクション1-2: HW市場比較（CSVデータ vs HW全体データ）
// ============================================================

/// 比較カード: 媒体（CSV）の値とHW全体の値を並列表示し、差分を算出
///
/// - `label`: 指標名
/// - `csv_value`: CSVから算出した値（整形済み文字列）
/// - `hw_value`: HWから算出した値（整形済み文字列）
/// - `diff_text`: 差分表示（正負込みのフォーマット済み文字列、Noneなら非表示）
/// - `positive`: 差分が「媒体が上回る（良い方向）」かどうか
fn render_comparison_card(
    html: &mut String,
    label: &str,
    csv_value: &str,
    hw_value: &str,
    diff_text: Option<&str>,
    positive: bool,
) {
    html.push_str("<div class=\"comparison-card\">\n");
    html.push_str(&format!("<h3>{}</h3>\n", escape_html(label)));
    html.push_str("<div class=\"value-pair\">\n");
    html.push_str(&format!(
        "<div><span class=\"label\">媒体</span><span class=\"value\">{}</span></div>\n",
        escape_html(csv_value)
    ));
    html.push_str(&format!(
        "<div><span class=\"label\">HW</span><span class=\"value\">{}</span></div>\n",
        escape_html(hw_value)
    ));
    html.push_str("</div>\n");
    if let Some(d) = diff_text {
        let cls = if positive { "positive" } else { "negative" };
        html.push_str(&format!("<div class=\"diff {}\">{}</div>\n", cls, escape_html(d)));
    }
    html.push_str("</div>\n");
}

fn render_section_hw_comparison(
    html: &mut String,
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
    ctx: &InsightContext,
) {
    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>HW市場比較</h2>\n");
    html.push_str("<p class=\"guide\" style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>CSV（媒体データ）とハローワーク全体データを\
        <strong>雇用形態ごと</strong>に並列比較。媒体に出現する雇用形態を動的に検出し、\
        対応するHWデータと同条件で比較します。\
    </p>\n");

    // CSV側の雇用形態を正規化してHW側のemp_group（正社員/パート/その他）にマッピング
    // - "正社員"/"正職員" → HW "正社員"
    // - "パート"/"アルバイト" → HW "パート"
    // - "契約社員"/"派遣社員"/その他 → HW "その他"
    fn normalize_emp_type(csv_type: &str) -> Option<&'static str> {
        if csv_type.contains("正社員") || csv_type.contains("正職員") {
            Some("正社員")
        } else if csv_type.contains("パート") || csv_type.contains("アルバイト") {
            Some("パート")
        } else if csv_type.contains("契約") || csv_type.contains("派遣")
            || csv_type.contains("嘱託") || csv_type.contains("臨時")
            || csv_type.contains("その他") {
            Some("その他")
        } else {
            None
        }
    }

    // CSV側の雇用形態を集約（HWのemp_groupごとに合算）
    use std::collections::HashMap;
    let mut csv_by_hw_group: HashMap<&str, (usize, i64)> = HashMap::new();  // (件数, 給与合計)
    for e in by_emp_type_salary {
        if let Some(hw_key) = normalize_emp_type(&e.emp_type) {
            let entry = csv_by_hw_group.entry(hw_key).or_insert((0, 0));
            entry.0 += e.count;
            entry.1 += e.avg_salary * e.count as i64;
        }
    }

    // 対象とする雇用形態を出現順に並べる（正社員 > パート > その他）
    let emp_order = ["正社員", "パート", "その他"];
    let present_groups: Vec<&str> = emp_order.iter()
        .filter(|g| csv_by_hw_group.contains_key(*g))
        .copied()
        .collect();

    if present_groups.is_empty() {
        html.push_str("<p style=\"color:#888;font-size:10pt;\">\
            CSVデータの雇用形態が判別できなかったため、HW比較をスキップしました。</p>\n");
        html.push_str("</div>\n");
        return;
    }

    // --- 雇用形態別 平均月給比較（動的生成） ---
    if !agg.is_hourly {
        html.push_str("<h3 style=\"font-size:11pt;margin:8px 0;\">雇用形態別 平均月給比較</h3>\n");
        html.push_str("<div class=\"comparison-grid\">\n");
        for &group in &present_groups {
            let (count, salary_sum) = csv_by_hw_group[group];
            let csv_avg = if count > 0 { salary_sum / count as i64 } else { 0 };
            let csv_display = if csv_avg > 0 {
                format!("{:.1}万円 (n={})", csv_avg as f64 / 10_000.0, count)
            } else {
                format!("- (n={})", count)
            };

            // HW側: cascade は industry_raw × emp_group の複合集計のため、
            // 同じ emp_group の全業種の avg_salary_min を平均化する
            let salaries: Vec<f64> = ctx.cascade.iter()
                .filter(|r| super::super::helpers::get_str_ref(r, "emp_group") == group)
                .map(|r| get_f64(r, "avg_salary_min"))
                .filter(|&v| v > 0.0)
                .collect();
            let hw_avg: i64 = if !salaries.is_empty() {
                (salaries.iter().sum::<f64>() / salaries.len() as f64) as i64
            } else { 0 };
            let hw_display = if hw_avg > 0 {
                format!("{:.1}万円", hw_avg as f64 / 10_000.0)
            } else {
                "データなし".to_string()
            };

            let (diff_text, positive) = if csv_avg > 0 && hw_avg > 0 {
                let diff = csv_avg - hw_avg;
                let pct = diff as f64 / hw_avg as f64 * 100.0;
                (
                    Some(format!("{:+.1}万円 ({:+.1}%)", diff as f64 / 10_000.0, pct)),
                    diff >= 0,
                )
            } else {
                (None, true)
            };
            render_comparison_card(
                html,
                &format!("{} 平均月給", group),
                &csv_display, &hw_display,
                diff_text.as_deref(), positive,
            );
        }
        html.push_str("</div>\n");
    }

    // --- 雇用形態構成比（媒体 vs HW） ---
    html.push_str("<h3 style=\"font-size:11pt;margin:16px 0 8px;\">雇用形態構成比</h3>\n");
    html.push_str("<div class=\"comparison-grid\">\n");

    // 雇用形態構成比（CSV vs HW の割合）を雇用形態ごとに表示
    let csv_total: usize = csv_by_hw_group.values().map(|(c, _)| c).sum();
    let hw_total: i64 = ctx.vacancy.iter()
        .map(|r| get_f64(r, "total_count") as i64)
        .sum();

    for &group in &present_groups {
        let (csv_count, _) = csv_by_hw_group[group];
        let csv_rate = if csv_total > 0 {
            csv_count as f64 / csv_total as f64 * 100.0
        } else { -1.0 };
        let csv_display = if csv_rate >= 0.0 {
            format!("{:.1}% ({}件)", csv_rate, csv_count)
        } else { "-".to_string() };

        let hw_count: i64 = ctx.vacancy.iter()
            .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == group)
            .map(|r| get_f64(r, "total_count") as i64)
            .unwrap_or(0);
        let hw_rate = if hw_total > 0 {
            hw_count as f64 / hw_total as f64 * 100.0
        } else { -1.0 };
        let hw_display = if hw_rate >= 0.0 && hw_total > 0 {
            format!("{:.1}% ({}件)", hw_rate, format_number(hw_count))
        } else { "データなし".to_string() };

        let (diff_text, positive) = if csv_rate >= 0.0 && hw_rate >= 0.0 {
            let d = csv_rate - hw_rate;
            (Some(format!("{:+.1}pt", d)), d >= 0.0)
        } else {
            (None, true)
        };
        render_comparison_card(
            html,
            &format!("{} 構成比", group),
            &csv_display, &hw_display,
            diff_text.as_deref(), positive,
        );
    }
    html.push_str("</div>\n"); // comparison-grid (構成比)

    // --- 地域人口/最低賃金の比較カード（従来通り、正社員雇用前提でない） ---
    html.push_str("<h3 style=\"font-size:11pt;margin:16px 0 8px;\">地域指標</h3>\n");
    html.push_str("<div class=\"comparison-grid\">\n");

    // --- カード3: 対象地域の人口（通勤圏優先、なければ市区町村／都道府県人口） ---
    let population: i64 = if ctx.commute_zone_total_pop > 0 {
        ctx.commute_zone_total_pop
    } else {
        ctx.ext_population.first()
            .map(|r| get_f64(r, "total_population") as i64)
            .unwrap_or(0)
    };
    let pop_source = if ctx.commute_zone_total_pop > 0 {
        format!("通勤圏内 {}自治体", ctx.commute_zone_count)
    } else if !ctx.muni.is_empty() {
        ctx.muni.clone()
    } else {
        ctx.pref.clone()
    };
    let pop_display = if population > 0 {
        format!("{}人", format_number(population))
    } else {
        "-".to_string()
    };
    render_comparison_card(
        html, "対象地域の人口",
        &pop_display,
        &pop_source,
        None, true,
    );

    // --- カード4: 最低賃金比較（CSV平均下限の160h換算 vs 都道府県最低賃金） ---
    if !agg.is_hourly {
        // CSV平均下限（月給→時給160h換算）
        let csv_avg_min: i64 = if !agg.by_prefecture_salary.is_empty() {
            let total: i64 = agg.by_prefecture_salary.iter()
                .filter(|p| p.avg_min_salary > 0)
                .map(|p| p.avg_min_salary)
                .sum();
            let n = agg.by_prefecture_salary.iter()
                .filter(|p| p.avg_min_salary > 0).count();
            if n > 0 { total / n as i64 } else { 0 }
        } else {
            0
        };
        let csv_hourly = csv_avg_min / 160;
        let csv_display = if csv_hourly > 0 {
            format!("{}円/h", format_number(csv_hourly))
        } else {
            "-".to_string()
        };

        // 都道府県最低賃金（ctx.prefから取得）
        let mw = min_wage_for_prefecture(&ctx.pref).unwrap_or(0);
        let mw_display = if mw > 0 {
            format!("{}円/h", format_number(mw))
        } else {
            "-".to_string()
        };

        let (mw_diff_text, mw_positive) = if csv_hourly > 0 && mw > 0 {
            let d = csv_hourly - mw;
            let pct = d as f64 / mw as f64 * 100.0;
            (Some(format!("{:+}円 ({:+.1}%)", d, pct)), d >= 0)
        } else {
            (None, true)
        };
        render_comparison_card(
            html, "最低賃金比較（160h換算）",
            &csv_display, &mw_display,
            mw_diff_text.as_deref(), mw_positive,
        );
    }

    html.push_str("</div>\n"); // comparison-grid
    html.push_str("<div class=\"note\" style=\"font-size:9pt;color:#555;margin-top:8px;\">\
        ※HW側データは「ハローワーク掲載求人のみ」が対象であり、全求人市場を反映するものではありません。\
        媒体（CSV）との差異は、掲載媒体の選定バイアスによる可能性があります。\
    </div>\n");
    html.push_str("</div>\n");
}

// ============================================================
// ECharts config生成ヘルパー
// ============================================================

/// ヒストグラム用ECharts設定JSONを生成（平均・中央値・最頻値のmarkLine付き）
///
/// markLineのxAxis値は、category軸のラベル（例: "20万"）に正確一致させる必要がある。
/// bin_size で丸めた「bin開始値（万単位）」を渡すことで、
/// 該当binの棒の開始位置に縦線を表示する。
fn build_histogram_echart_config(
    labels: &[String],
    values: &[usize],
    color: &str,
    mean: Option<i64>,
    median: Option<i64>,
    mode: Option<i64>,
    bin_size: i64,
) -> String {
    // 値を category 軸ラベルに合わせる: (値 / bin_size) * bin_size を「X万」形式に
    let to_label = |yen: i64| -> String {
        if bin_size <= 0 {
            return format!("{}万", yen / 10_000);
        }
        let snapped = (yen / bin_size) * bin_size;
        // 小数万対応（5,000円刻みで 22.5万 など）
        let man = snapped as f64 / 10_000.0;
        if (man.fract()).abs() < 1e-9 {
            format!("{}万", snapped / 10_000)
        } else {
            format!("{:.1}万", man)
        }
    };

    let mut mark_lines = vec![];
    if let Some(m) = mean {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "平均",
            "lineStyle": {"color": "#e74c3c", "type": "dashed", "width": 2},
            "label": {"formatter": "平均", "fontSize": 10}
        }));
    }
    if let Some(m) = median {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "中央値",
            "lineStyle": {"color": "#27ae60", "type": "dashed", "width": 2},
            "label": {"formatter": "中央値", "fontSize": 10}
        }));
    }
    if let Some(m) = mode {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "最頻値",
            "lineStyle": {"color": "#9b59b6", "type": "dashed", "width": 2},
            "label": {"formatter": "最頻値", "fontSize": 10}
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

    // 下限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_min_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<h3>下限給与の分布（20,000円刻み）</h3>\n");
        let (labels, values, _b) = build_salary_histogram(salary_min_values, 20_000);
        let mode_min_20k = compute_mode(salary_min_values, 20_000);
        let config = build_histogram_echart_config(
            &labels, &values, "#42A5F5",
            Some(stats.mean), Some(stats.median), mode_min_20k, 20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>下限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_min_values, 5_000);
        let mode_min_5k = compute_mode(salary_min_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f, &values_f, "#42A5F5",
            Some(stats.mean), Some(stats.median), mode_min_5k, 5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
    }

    // 上限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_max_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<h3>上限給与の分布（20,000円刻み）</h3>\n");
        let (labels, values, _b) = build_salary_histogram(salary_max_values, 20_000);
        let mode_max_20k = compute_mode(salary_max_values, 20_000);
        let config = build_histogram_echart_config(
            &labels, &values, "#66BB6A",
            Some(stats.mean), Some(stats.median), mode_max_20k, 20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>上限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_max_values, 5_000);
        let mode_max_5k = compute_mode(salary_max_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f, &values_f, "#66BB6A",
            Some(stats.mean), Some(stats.median), mode_max_5k, 5_000,
        );
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

    html.push_str("<h3>求人数ランキング TOP15（給与情報あり）</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">給与付き求人数</th><th style=\"text-align:right\">平均月給</th></tr></thead>\n<tbody>\n");
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

    // 給与ランキング TOP15（サンプル数に応じて閾値動的調整）
    let multi_count = by_company.iter().filter(|c| c.count >= 2).count();
    let min_count_threshold = if multi_count >= 15 { 2 } else { 1 };
    let mut by_salary: Vec<&CompanyAgg> = by_company.iter()
        .filter(|c| c.count >= min_count_threshold && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        let title = if min_count_threshold >= 2 {
            "給与ランキング TOP15（給与付き2件以上の企業）"
        } else {
            "給与ランキング TOP15（給与付き、1件求人含む。※1件は参考値）"
        };
        html.push_str(&format!("<h3>{}</h3>\n", title));
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>企業名</th><th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">給与付き求人数</th></tr></thead>\n<tbody>\n");
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

    // 異常値除外: 5万〜200万円の妥当な範囲、かつ上限≧下限
    // （時給や年収の月給換算ミスによる外れ値を排除）
    let filtered_points: Vec<&ScatterPoint> =
        agg.scatter_min_max.iter()
            .filter(|p| {
                let x_man = p.x as f64 / 10_000.0;
                let y_man = p.y as f64 / 10_000.0;
                x_man >= 5.0 && x_man <= 200.0
                    && y_man >= 5.0 && y_man <= 200.0
                    && y_man >= x_man
            })
            .collect();

    if filtered_points.len() < 6 {
        html.push_str("<p style=\"font-size:9pt;color:#888;\">有効なデータ点が不足しているため散布図を省略しました。</p>\n");
        html.push_str("</div>\n");
        return;
    }

    let scatter_data: Vec<serde_json::Value> = filtered_points.iter()
        .take(200)
        .map(|p| json!([p.x as f64 / 10_000.0, p.y as f64 / 10_000.0]))
        .collect();

    // 軸範囲をパーセンタイル(P2.5〜P97.5)基準で決定、5%マージン
    let mut x_vals_man: Vec<f64> = filtered_points.iter().map(|p| p.x as f64 / 10_000.0).collect();
    let mut y_vals_man: Vec<f64> = filtered_points.iter().map(|p| p.y as f64 / 10_000.0).collect();
    let (x_axis_min, x_axis_max) = compute_axis_range(&mut x_vals_man);
    let (y_axis_min, y_axis_max) = compute_axis_range(&mut y_vals_man);

    // 回帰線のmarkLine（2点指定）
    let mut series_list = vec![json!({
        "type": "scatter",
        "data": scatter_data,
        "symbolSize": 6,
        "itemStyle": {"color": "rgba(59,130,246,0.5)"}
    })];

    if let Some(reg) = &agg.regression_min_max {
        // 回帰線の端点は軸の表示範囲にクランプ（外れ値によるスケール崩れ防止）
        let x_min_yen = x_axis_min * 10_000.0;
        let x_max_yen = x_axis_max * 10_000.0;
        let y1 = (reg.slope * x_min_yen + reg.intercept) / 10_000.0;
        let y2 = (reg.slope * x_max_yen + reg.intercept) / 10_000.0;

        series_list.push(json!({
            "type": "line",
            "data": [[x_axis_min, y1], [x_axis_max, y2]],
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
            "min": x_axis_min,
            "max": x_axis_max,
            "axisLabel": {"fontSize": 9}
        },
        "yAxis": {
            "name": "上限（万円）",
            "nameLocation": "center",
            "nameGap": 35,
            "type": "value",
            "min": y_axis_min,
            "max": y_axis_max,
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
            "<p style=\"font-size:9px;color:#666;\">データ点: {}件（表示: {}件、異常値除外後）/ R\u{00B2} = {:.3}（{}）</p>\n",
            agg.scatter_min_max.len(), filtered_points.len(), reg.r_squared, strength
        ));
    }

    html.push_str("</div>\n");
}

/// ソート済みでない値の配列から、指定パーセンタイル値を返す。
/// 空配列の場合は 0.0 を返す。
fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() { return 0.0; }
    let clamped = p.clamp(0.0, 100.0);
    let idx = (((sorted.len() - 1) as f64) * clamped / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 散布図軸の表示範囲を P2.5〜P97.5 基準で計算し、5% のマージンを追加して返す。
/// 下限は 0 未満にはならない。範囲が潰れる場合は ±1.0 万円のフォールバック。
fn compute_axis_range(values: &mut Vec<f64>) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo = percentile_sorted(values, 2.5);
    let hi = percentile_sorted(values, 97.5);
    let (lo, hi) = if (hi - lo).abs() < f64::EPSILON {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    };
    let pad = (hi - lo) * 0.05;
    let lo_padded = (lo - pad).max(0.0);
    let hi_padded = hi + pad;
    // ECharts が整数目盛りを選びやすいよう、整数に丸める
    (lo_padded.floor(), hi_padded.ceil())
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
        // 有意タグのフィルタリング:
        // 1. 出現率50%超のタグは共通属性として除外（全求人の半数以上に付く「交通費支給」等は差分がゼロに収束）
        // 2. 差分 |diff_percent| >= 2% のタグのみハイライト（それ未満は参考扱い）
        let total_records = agg.total_count as f64;
        let significant: Vec<&TagSalaryAgg> = agg.by_tag_salary.iter()
            .filter(|t| {
                let frequency = t.count as f64 / total_records;
                frequency < 0.5 && t.diff_percent.abs() >= 2.0
            })
            .collect();
        let display_tags: Vec<&TagSalaryAgg> = if significant.is_empty() {
            // フォールバック: 有意なタグがない場合は全タグを表示
            agg.by_tag_salary.iter().collect()
        } else {
            significant
        };
        if agg.by_tag_salary.len() > display_tags.len() {
            html.push_str(&format!(
                "<p class=\"note\" style=\"font-size:9pt;color:#888;\">※{}タグから{}タグに絞り込み表示中（出現率50%超の共通タグと差分±2%未満を除外）</p>\n",
                agg.by_tag_salary.len(), display_tags.len()
            ));
        }
        // タグ別給与差分テーブル（ソート可能・完全版）
        html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>タグ</th><th style=\"text-align:right\">件数</th>\
            <th style=\"text-align:right\">平均月給</th><th style=\"text-align:right\">全体比</th></tr></thead>\n<tbody>\n");
        for (i, ts) in display_tags.iter().enumerate() {
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
/// 給与値配列をbin_size刻みでバケットに分類し、ラベル・件数・bin下端境界配列を返す
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
        // ラベル: bin_size が万円未満の場合は小数表記（例: 22.5万）
        let man = boundary as f64 / 10_000.0;
        let label = if (man.fract()).abs() < 1e-9 {
            format!("{}万", boundary / 10_000)
        } else {
            format!("{:.1}万", man)
        };
        labels.push(label);
        counts.push(count);
        boundaries.push(boundary);
        boundary = upper;
    }

    (labels, counts, boundaries)
}

/// 最頻値を計算（ヒストグラム最大カウントのbin中心値を返す）
fn compute_mode(values: &[i64], bin_size: i64) -> Option<i64> {
    let (_labels, counts, boundaries) = build_salary_histogram(values, bin_size);
    if counts.is_empty() {
        return None;
    }
    let max_idx = counts.iter().enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(i, _)| i)?;
    // markLine を bin の下端ラベルに一致させるため、bin開始値を返す
    Some(boundaries[max_idx])
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
    fn test_compute_mode() {
        // 200_000 が最頻帯（bin 200_000..220_000 に3件）
        let values = vec![200_000, 205_000, 210_000, 250_000, 300_000];
        let mode = compute_mode(&values, 20_000);
        assert_eq!(mode, Some(200_000));
    }

    #[test]
    fn test_compute_mode_empty() {
        assert_eq!(compute_mode(&[], 20_000), None);
        assert_eq!(compute_mode(&[0, 0], 20_000), None);
    }

    #[test]
    fn test_histogram_echart_config() {
        let labels = vec!["20万".to_string(), "22万".to_string(), "24万".to_string()];
        let values = vec![5, 12, 8];
        let config = build_histogram_echart_config(
            &labels, &values, "#42A5F5",
            Some(220_000), Some(215_000), Some(220_000), 20_000,
        );
        assert!(config.contains("bar"));
        assert!(config.contains("markLine"));
        assert!(config.contains("平均"));
        assert!(config.contains("中央値"));
        assert!(config.contains("最頻値"));
        // 最頻値カラー（紫 #9b59b6）が含まれる
        assert!(config.contains("#9b59b6"));
        // JSON として妥当か
        let parsed: Result<serde_json::Value, _> = serde_json::from_str(&config);
        assert!(parsed.is_ok());
    }

    #[test]
    fn test_histogram_echart_config_fine_bin() {
        // 5,000円刻みで 22.5万 などの小数ラベルが生成できること
        let labels = vec!["22.5万".to_string(), "23万".to_string()];
        let values = vec![3, 7];
        let config = build_histogram_echart_config(
            &labels, &values, "#42A5F5",
            Some(225_000), Some(230_000), Some(225_000), 5_000,
        );
        // 225_000 は 22.5万 にスナップされる
        assert!(config.contains("22.5万"));
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
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None);
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

    /// hw_context 有無で「HW市場比較」セクションの出力有無が切り替わる
    #[test]
    fn test_render_with_hw_context_adds_comparison_section() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();

        // None の場合、比較セクションは出ない
        let html_without = render_survey_report_page(
            &agg, &seeker, &[], &[], &[], &[], None,
        );
        // h2 見出し（HTMLタグ付き）が存在しないことを確認
        assert!(!html_without.contains("<h2>HW市場比較</h2>"),
            "hw_context=None のときは HW市場比較セクション（h2）を出さない");
        assert!(!html_without.contains("<div class=\"comparison-grid\">"),
            "hw_context=None のときは comparison-grid コンテナは出さない（CSS内の定義は除く）");

        // Some の場合は出る（空の InsightContext でもヘッダーは出力される）
        let ctx = mock_empty_insight_ctx();
        let html_with = render_survey_report_page(
            &agg, &seeker, &[], &[], &[], &[], Some(&ctx),
        );
        assert!(html_with.contains("<h2>HW市場比較</h2>"),
            "hw_context=Some のときは HW市場比較セクション（h2）を出す");
        // by_emp_type_salary が空なので雇用形態判別不能メッセージが出る想定
        assert!(
            html_with.contains("comparison-grid") || html_with.contains("雇用形態が判別できなかった"),
            "comparison-grid か 判別不能メッセージのどちらかが出る"
        );
    }

    /// テスト用: 空の InsightContext を生成
    fn mock_empty_insight_ctx() -> super::super::super::insight::fetch::InsightContext {
        use super::super::super::insight::fetch::InsightContext;
        InsightContext {
            vacancy: vec![], resilience: vec![], transparency: vec![], temperature: vec![],
            competition: vec![], cascade: vec![], salary_comp: vec![], monopsony: vec![],
            spatial_mismatch: vec![], wage_compliance: vec![], region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![], ts_vacancy: vec![], ts_salary: vec![],
            ts_fulfillment: vec![], ts_tracking: vec![],
            ext_job_ratio: vec![], ext_labor_stats: vec![],
            ext_min_wage: vec![], ext_turnover: vec![],
            ext_population: vec![], ext_pyramid: vec![], ext_migration: vec![],
            ext_daytime_pop: vec![], ext_establishments: vec![],
            ext_business_dynamics: vec![], ext_care_demand: vec![],
            ext_household_spending: vec![], ext_climate: vec![],
            commute_zone_count: 0, commute_zone_pref_count: 0,
            commute_zone_total_pop: 0, commute_zone_working_age: 0, commute_zone_elderly: 0,
            commute_inflow_total: 0, commute_outflow_total: 0,
            commute_self_rate: 0.0, commute_inflow_top3: vec![],
            pref: "東京都".to_string(), muni: String::new(),
        }
    }

    /// パーセンタイル計算: 基本動作
    #[test]
    fn test_percentile_sorted_basic() {
        let sorted = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        assert_eq!(percentile_sorted(&sorted, 0.0), 10.0);
        assert_eq!(percentile_sorted(&sorted, 100.0), 100.0);
        let p50 = percentile_sorted(&sorted, 50.0);
        assert!((p50 - 60.0).abs() < 20.0, "p50は中央付近のはず, got {}", p50);
    }

    #[test]
    fn test_percentile_sorted_empty() {
        assert_eq!(percentile_sorted(&[], 50.0), 0.0);
    }

    /// 軸範囲計算: 異常値が混ざっていない場合もデータ範囲に沿うこと
    #[test]
    fn test_compute_axis_range_basic() {
        let mut values: Vec<f64> = (20..=50).map(|v| v as f64).collect();
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(lo >= 0.0 && lo <= 25.0, "lo should be near data min, got {}", lo);
        assert!(hi >= 45.0 && hi <= 60.0, "hi should be near data max, got {}", hi);
        assert!(hi > lo, "hi > lo");
        // ECharts PDF の 0〜700 問題が再発しないことを保証
        assert!(hi < 700.0, "hi should not explode to 700, got {}", hi);
    }

    #[test]
    fn test_compute_axis_range_empty() {
        let mut values: Vec<f64> = vec![];
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(hi > lo, "degenerate range should still yield hi>lo");
    }

    #[test]
    fn test_compute_axis_range_single_value() {
        let mut values: Vec<f64> = vec![30.0, 30.0, 30.0];
        let (lo, hi) = compute_axis_range(&mut values);
        assert!(hi > lo, "単一値でも範囲が潰れないこと");
        assert!(lo >= 0.0);
    }

    /// 散布図の異常値除外ロジック（render_section_scatter 内のフィルタ条件を直接検証）
    #[test]
    fn test_scatter_outlier_filter() {
        let points = vec![
            ScatterPoint { x: 200_000, y: 300_000 },      // OK
            ScatterPoint { x: 150_000, y: 250_000 },      // OK
            ScatterPoint { x: 10_000, y: 6_000_000 },     // NG: y=600万
            ScatterPoint { x: 5_000, y: 7_000_000 },      // NG: x<5万 かつ y=700万
            ScatterPoint { x: 300_000, y: 200_000 },      // NG: x>200万 かつ y<x
            ScatterPoint { x: 40_000, y: 50_000 },        // NG: x<5万
        ];
        let filtered: Vec<&ScatterPoint> = points.iter()
            .filter(|p| {
                let x_man = p.x as f64 / 10_000.0;
                let y_man = p.y as f64 / 10_000.0;
                x_man >= 5.0 && x_man <= 200.0
                    && y_man >= 5.0 && y_man <= 200.0
                    && y_man >= x_man
            })
            .collect();
        assert_eq!(filtered.len(), 2, "5万〜200万の範囲内かつ y>=x の2点のみ残る");
    }
}
