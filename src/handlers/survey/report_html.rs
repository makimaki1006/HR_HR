//! HTMLレポート生成（株式会社For A-career 求人市場 総合診断レポート）
//! 仕様書: docs/pdf_design_spec_2026_04_24.md (2026-04-24 追加要件反映済み)
//! A4縦向き印刷 / ダウンロード後編集 両対応のHTMLとして出力する
//! - 表紙 → Executive Summary → HWデータ連携 → 各セクション(So What付き) → 注記/免責
//! - EChartsインタラクティブチャート + ソート可能テーブル
//! - 印刷時はモノクロ耐性（severityアイコン併記）対応
//! - `contenteditable` により主要コメント欄はダウンロード後にユーザーが編集可能

use super::super::company::fetch::NearbyCompany;
use super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::insight::fetch::InsightContext;
use super::aggregator::{CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg};
use super::hw_enrichment::HwAreaEnrichment;
use super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

// ============================================================
// 共通: severity バッジ (モノクロ耐性 アイコン文字併記)
// ============================================================

/// Severity表現（色 + 文字アイコン）。helpers.rs::Severity に1対1対応。
#[derive(Clone, Copy)]
enum RptSev {
    Critical,
    Warning,
    Info,
    Positive,
}

impl RptSev {
    fn color(self) -> &'static str {
        match self {
            RptSev::Critical => "#ef4444",
            RptSev::Warning => "#f59e0b",
            RptSev::Info => "#3b82f6",
            RptSev::Positive => "#10b981",
        }
    }
    /// モノクロ耐性のためのアイコン文字併記ラベル
    fn label(self) -> &'static str {
        match self {
            RptSev::Critical => "\u{25B2}\u{25B2} 重大",
            RptSev::Warning => "\u{25B2} 注意",
            RptSev::Info => "\u{25CF} 情報",
            RptSev::Positive => "\u{25EF} 良好",
        }
    }
}

/// severity バッジ HTML（印刷/モノクロ両対応）
fn severity_badge(sev: RptSev) -> String {
    format!(
        "<span class=\"sev-badge\" style=\"background:{};color:#fff;font-weight:700;font-size:10pt;padding:2px 8px;border-radius:3px;letter-spacing:0.04em;\">{}</span>",
        sev.color(),
        escape_html(sev.label())
    )
}

// ============================================================
// メイン関数
// ============================================================

/// 求人市場 総合診断レポート 印刷/ダウンロード用 HTML を生成
///
/// # 引数
/// - `agg`: CSVから集計した求人データ
/// - `seeker`: 求職者心理分析結果
/// - `by_company`: 企業別集計（Step 2 で追加）
/// - `by_emp_type_salary`: 雇用形態別給与（Step 2 で追加）
/// - `salary_min_values`: 下限給与一覧（Step 2 で追加）
/// - `salary_max_values`: 上限給与一覧（Step 2 で追加）
/// - `hw_context`: HW ローカル/外部統計コンテキスト（Section 2/3/H 等で参照）
/// - `salesnow_companies`: 地域注目企業リスト（内部名は呼出側互換で維持）
pub(crate) fn render_survey_report_page(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
) -> String {
    // 後方互換: enrichment マップなしでの呼び出し
    render_survey_report_page_with_enrichment(
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        salary_min_values,
        salary_max_values,
        hw_context,
        salesnow_companies,
        &std::collections::HashMap::new(),
    )
}

/// 市区町村別 HW enrichment map を受け取る拡張版
///
/// `hw_enrichment_map`: key = `"{prefecture}:{municipality}"` の HashMap
/// 各エントリに市区町村単位の HW 現在件数 / 推移 / 欠員率 を格納
pub(crate) fn render_survey_report_page_with_enrichment(
    agg: &SurveyAggregation,
    seeker: &JobSeekerAnalysis,
    by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    salary_min_values: &[i64],
    salary_max_values: &[i64],
    hw_context: Option<&InsightContext>,
    salesnow_companies: &[NearbyCompany],
    hw_enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
) -> String {
    let now = chrono::Local::now()
        .format("%Y年%m月%d日 %H:%M")
        .to_string();
    let mut html = String::with_capacity(64_000);

    // --- DOCTYPE + HEAD ---
    html.push_str("<!DOCTYPE html>\n<html lang=\"ja\">\n<head>\n");
    html.push_str("<meta charset=\"UTF-8\">\n");
    html.push_str("<meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">\n");
    html.push_str("<title>求人市場 総合診断レポート</title>\n");
    html.push_str("<style>\n");
    html.push_str(&render_css());
    html.push_str("</style>\n");
    // ECharts CDN
    html.push_str(
        "<script src=\"https://cdn.jsdelivr.net/npm/echarts@5/dist/echarts.min.js\"></script>\n",
    );
    html.push_str("</head>\n<body>\n");

    // --- テーマ切替 + 印刷ボタン ---
    html.push_str("<div class=\"no-print\" style=\"text-align:right;padding:8px 16px;\">\n");
    html.push_str("<button class=\"theme-toggle\" type=\"button\" onclick=\"toggleTheme()\" aria-label=\"ダークモード/ライトモードを切替\">\u{1F319} ダーク / \u{2600} ライト</button>\n");
    html.push_str("<button onclick=\"window.print()\" aria-label=\"印刷またはPDFで保存\" style=\"padding:8px 24px;font-size:14px;cursor:pointer;border:1px solid #666;border-radius:4px;background:#fff;\">印刷 / PDF保存</button>\n");
    html.push_str("</div>\n");

    // --- 表紙ページ (Section 0 / 仕様書 7.2) ---
    // 2026-04-24: 「競合調査分析」文言を全削除。タイトルは「求人市場 総合診断レポート」に統一。
    let today_short = chrono::Local::now().format("%Y年%m月").to_string();
    let target_region = compose_target_region(agg);
    html.push_str(
        "<section class=\"cover-page\" role=\"region\" aria-labelledby=\"cover-title\">\n",
    );
    html.push_str("<div class=\"cover-logo\" aria-hidden=\"true\">株式会社For A-career</div>\n");
    html.push_str(
        "<div class=\"cover-title\" id=\"cover-title\">求人市場<br>総合診断レポート</div>\n",
    );
    html.push_str("<div class=\"cover-sub\">");
    html.push_str(&escape_html(&today_short));
    html.push_str(" 版</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-target\">対象: {}</div>\n",
        escape_html(&target_region)
    ));
    // 表紙コメント（ダウンロード後にユーザーが追記できる欄）
    html.push_str(
        "<div class=\"cover-comment\" contenteditable=\"true\" spellcheck=\"false\" \
         aria-label=\"レポートコメント（クリックで編集可）\" \
         data-editable-placeholder=\"※ コメントを入力（例: 宛先部署・提案趣旨・補足事項）\">\
         ※ コメントを入力（例: 宛先部署・提案趣旨・補足事項）\
         </div>\n",
    );
    html.push_str("<div class=\"cover-confidential\">この資料は機密情報です。外部への持ち出しは社内規定に従ってください。</div>\n");
    html.push_str(&format!(
        "<div class=\"cover-footer\">株式会社For A-career &nbsp;|&nbsp; 生成日時: {}</div>\n",
        escape_html(&now)
    ));
    html.push_str("</section>\n");

    // --- Executive Summary (Section 1 / 仕様書 3章) ---
    render_section_executive_summary(
        &mut html,
        agg,
        seeker,
        by_company,
        by_emp_type_salary,
        hw_context,
    );

    // --- Section H: 地域 × HW データ連携（新規: 2026-04-24） ---
    // CSV の (都道府県, 市区町村) ごとに、HW ローカルDB/時系列/外部統計から取得された
    // HW 現在件数・3ヶ月/1年推移・欠員率を一覧表示する。
    // hw_context が無い場合はセクション自体を出力しない。
    if let Some(ctx) = hw_context {
        render_section_hw_enrichment(&mut html, agg, ctx, hw_enrichment_map);
    }

    // --- Section 1 補助: サマリー(旧) は Executive Summary に統合済み ---
    // 「サマリー」見出しはテスト互換のため Executive Summary 内で維持
    render_section_summary(&mut html, agg);

    // --- Section 2: HW 市場比較 ---
    // 2026-04-24 ユーザー指摘により削除:
    //   「任意でスクレイピングしている件数 VS ハローワークデータ」という
    //   非同質データ比較は無意味。雇用形態構成比・最低賃金比較の "媒体" 値も
    //   出どころ不明の誤誘導になるため、HW 市場比較セクション自体を非表示化。
    //   HW 側の補完数値は Section 3 (地域×HW データ連携) と Exec Summary で
    //   参考値として併記するに留める。
    let _ = hw_context;

    // --- Section 3: 給与分布 統計 ---
    render_section_salary_stats(&mut html, agg, salary_min_values, salary_max_values);

    // --- Section 4: 雇用形態分布 ---
    render_section_employment(&mut html, agg, by_emp_type_salary);

    // --- Section 4B: 雇用形態グループ別 ネイティブ単位集計 (2026-04-24 Phase 2) ---
    // 正社員 → 月給, パート → 時給 を並列表示
    render_section_emp_group_native(&mut html, agg);

    // --- Section 5: 給与の相関分析（散布図） ---
    render_section_scatter(&mut html, agg);

    // --- Section 6: 地域分析（都道府県） ---
    render_section_region(&mut html, agg);

    // --- Section 7: 地域分析（市区町村） ---
    render_section_municipality_salary(&mut html, agg);

    // --- Section 8: 最低賃金比較 ---
    render_section_min_wage(&mut html, agg);

    // --- Section 9: 企業分析 ---
    render_section_company(&mut html, by_company);

    // --- Section 10: タグ × 給与相関 ---
    render_section_tag_salary(&mut html, agg);

    // --- Section 11: 求職者心理分析 ---
    render_section_job_seeker(&mut html, seeker);

    // --- Section 12: SalesNow 地域注目企業（非空のときのみ） ---
    if !salesnow_companies.is_empty() {
        render_section_salesnow_companies(&mut html, salesnow_companies);
    }

    // --- Section 13: 注記・出典・免責 (必須) ---
    render_section_notes(&mut html, &now);

    // --- 画面下部フッター（印刷時は @page footer を使用） ---
    html.push_str("<div class=\"screen-footer no-print\">\n");
    html.push_str("<span>株式会社For A-career | ハローワーク求人データ分析レポート</span>\n");
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
  /* 仕様書 5.2 カラーパレット */
  --c-primary: #1e3a8a;        /* blue-900: セクション見出し・表紙 */
  --c-primary-light: #3b82f6;  /* blue-500 */
  --c-success: #10b981;        /* emerald-500: Positive */
  --c-danger: #ef4444;         /* red-500: Critical */
  --c-warning: #f59e0b;        /* amber-500: Warning */
  --c-info: #3b82f6;           /* blue-500: Info */
  --c-text: #0f172a;
  --c-text-muted: #64748b;
  --c-border: #e2e8f0;
  --c-bg-card: #f8fafc;
  --bg: #ffffff;
  --text: #0f172a;
  --shadow-card: 0 1px 3px rgba(0,0,0,0.08);
  --radius: 6px;
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

/* 仕様書 6.1: @page 宣言（A4縦、12mmマージン、フッター定型文） */
@page {
  size: A4 portrait;
  margin: 12mm;
  @bottom-left {
    content: "株式会社For A-career | ハローワーク求人データ分析レポート";
    font-size: 8pt;
    color: #999;
  }
  @bottom-right {
    content: "Page " counter(page) " / " counter(pages);
    font-size: 8pt;
    color: #999;
  }
}
@page :first {
  /* 表紙にはページ番号・フッター文言を出さない */
  @bottom-left { content: ""; }
  @bottom-right { content: ""; }
}

* { box-sizing: border-box; }

body {
  /* 仕様書 5.1 タイポグラフィ */
  font-family: "Hiragino Kaku Gothic ProN", "Meiryo", "Noto Sans JP", sans-serif;
  font-size: 11pt;
  line-height: 1.6;
  color: var(--text);
  margin: 0;
  padding: 8px 16px;
  background: var(--bg);
  font-feature-settings: "palt" 1;
  transition: background 0.2s, color 0.2s;
}

/* 表紙（仕様書 7.2） */
.cover-page {
  min-height: 260mm;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  text-align: center;
  padding: 40mm 10mm 30mm;
  page-break-after: always;
  break-after: page;
  border: 1px solid var(--c-border);
  border-radius: var(--radius);
  margin-bottom: 16px;
  position: relative;
  background: linear-gradient(180deg, var(--c-bg-card) 0%, var(--bg) 100%);
}
.cover-logo {
  min-width: 200px;
  width: auto;
  max-width: 360px;
  padding: 0 16px;
  height: 60px;
  display: inline-flex;
  align-items: center;
  justify-content: center;
  color: var(--c-primary);
  font-size: 13pt;
  font-weight: 700;
  border: 1px dashed var(--c-border);
  border-radius: 4px;
  margin-bottom: 36px;
  letter-spacing: 0;
  /* ASCII 名「For A-career」がハイフンで折り返されるのを防ぐ */
  white-space: nowrap;
  word-break: keep-all;
  overflow-wrap: normal;
}
.cover-title {
  font-size: 28pt;
  font-weight: 700;
  color: var(--c-primary);
  margin: 10px 0 6px;
  letter-spacing: 0.05em;
  line-height: 1.2;
}
.cover-sub { font-size: 14pt; color: var(--text); margin-bottom: 16mm; }
.cover-target { font-size: 12pt; color: var(--text); margin-bottom: 14mm; }

/* 表紙コメント欄（ダウンロード後に編集可能） */
.cover-comment {
  min-height: 40mm;
  width: 70%;
  padding: 10px 14px;
  border: 1px dashed var(--c-border);
  border-radius: var(--radius);
  background: rgba(255,255,255,0.6);
  font-size: 10pt;
  line-height: 1.6;
  color: var(--c-text-muted);
  margin-bottom: 10mm;
  text-align: left;
  white-space: pre-wrap;
}
.cover-comment:focus { outline: 2px solid var(--c-primary); outline-offset: 2px; }
[contenteditable="true"] {
  cursor: text;
  transition: background 0.15s;
}
[contenteditable="true"]:hover { background: rgba(30,58,138,0.04); }
[contenteditable="true"]:focus { outline: 2px solid var(--c-primary); outline-offset: 2px; background: #fff; }
@media print {
  [contenteditable="true"] { outline: none !important; background: transparent !important; }
  [contenteditable="true"]:empty::before { content: ""; }
}
.cover-confidential {
  margin-top: auto;
  font-size: 10pt;
  color: var(--c-text-muted);
  border-top: 1px solid var(--c-border);
  padding-top: 14px;
  width: 70%;
}
.cover-footer {
  position: absolute;
  bottom: 20mm;
  left: 0;
  right: 0;
  font-size: 10pt;
  color: var(--c-text-muted);
}

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

/* 見出し（仕様書 5.1、5.6 widow/orphan 対応） */
h1 { font-size: 22pt; font-weight: 700; letter-spacing: 0.02em; line-height: 1.3; }
h2 {
  font-size: 18pt;
  font-weight: 700;
  margin: 14px 0 8px;
  border-bottom: 2px solid var(--c-primary);
  padding-bottom: 4px;
  color: var(--c-primary);
  letter-spacing: 0.02em;
  line-height: 1.3;
  page-break-after: avoid;
  break-after: avoid;
}
h3 {
  font-size: 14pt;
  font-weight: 700;
  margin: 10px 0 4px;
  color: var(--text);
  line-height: 1.4;
  page-break-after: avoid;
  break-after: avoid;
}

p, li { orphans: 3; widows: 3; }

.section {
  margin-bottom: 16px;
  page-break-inside: avoid;
  break-inside: avoid;
}
.section-compact {
  margin-bottom: 8px;
  page-break-inside: avoid;
  break-inside: avoid;
}
.section-header-meta {
  font-size: 10pt;
  color: var(--c-text-muted);
  margin: 0 0 6px;
}
.section-sowhat {
  background: var(--c-bg-card);
  border-left: 4px solid var(--c-primary);
  padding: 6px 10px;
  margin: 0 0 8px;
  font-size: 10pt;
  line-height: 1.5;
}
.section-xref {
  font-size: 9pt;
  color: var(--c-text-muted);
  margin: 6px 0 0;
}

/* KPIカード（仕様書 3.2 Executive Summary / 5.2 カラー / 6.2 page-break-inside:avoid） */
.summary-grid {
  display: grid;
  grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
  gap: 8px;
  margin-bottom: 12px;
}
.exec-kpi-grid {
  display: grid;
  grid-template-columns: repeat(5, 1fr);
  gap: 8px;
  margin-bottom: 14px;
}
.summary-card, .kpi-card {
  background: var(--c-bg-card);
  border: 1px solid var(--c-border);
  border-radius: var(--radius);
  padding: 10px 14px;
  text-align: center;
  position: relative;
  overflow: hidden;
  page-break-inside: avoid;
  break-inside: avoid;
  transition: transform 0.2s, box-shadow 0.2s;
}
.summary-card::before, .kpi-card::before {
  content: '';
  position: absolute;
  top: 0; left: 0; right: 0;
  height: 3px;
  background: var(--c-primary);
}
.summary-card:hover, .kpi-card:hover {
  transform: translateY(-2px);
  box-shadow: 0 4px 12px rgba(0,0,0,0.12);
}
.summary-card .label, .kpi-card .label {
  font-size: 10pt;
  color: var(--c-text-muted);
  margin-bottom: 3px;
}
.summary-card .value, .kpi-card .value {
  font-size: 24pt;
  font-weight: 700;
  color: var(--c-primary);
  line-height: 1.1;
}
.summary-card .unit, .kpi-card .unit { font-size: 10pt; color: var(--c-text-muted); }

/* Executive Summary 推奨アクション（仕様書 3.4） */
.exec-action-list { margin: 8px 0; padding: 0; list-style: none; }
.exec-summary-action {
  border: 1px solid var(--c-border);
  border-radius: var(--radius);
  padding: 10px 12px;
  margin-bottom: 8px;
  background: var(--bg);
  page-break-inside: avoid;
  break-inside: avoid;
}
.exec-summary-action .action-head {
  display: flex;
  align-items: center;
  gap: 8px;
  margin-bottom: 4px;
  font-size: 12pt;
  font-weight: 700;
}
.exec-summary-action .action-body {
  font-size: 10pt;
  color: var(--text);
  line-height: 1.5;
}
.exec-summary-action .action-xref {
  font-size: 9pt;
  color: var(--c-text-muted);
  margin-top: 4px;
}
.exec-scope-note {
  font-size: 9pt;
  line-height: 1.5;
  color: var(--c-text-muted);
  border-top: 1px dashed var(--c-border);
  padding-top: 6px;
  margin-top: 8px;
}
.exec-summary { page-break-after: always; break-after: page; }

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

/* HW データ連携セクション用テーブル */
.hw-enrichment-table { width: 100%; border-collapse: collapse; font-size: 10pt; margin: 8px 0; }
.hw-enrichment-table th { background: var(--c-primary); color: #fff; padding: 6px 8px; text-align: left; font-weight: 700; border-bottom: 0; }
.hw-enrichment-table td { padding: 5px 8px; border-bottom: 1px solid var(--c-border); }
.hw-enrichment-table td.num { text-align: right; font-variant-numeric: tabular-nums; }
.hw-enrichment-table .trend-up { color: #059669; font-weight: 700; }
.hw-enrichment-table .trend-down { color: #dc2626; font-weight: 700; }
.hw-enrichment-table .trend-flat { color: var(--c-text-muted); }
.hw-change-label { display: inline-block; font-size: 9pt; color: var(--c-text-muted); margin-left: 4px; }

/* 印刷制御（仕様書 6.4, 6.5, 6.7 + 2026-04-24 追加要件 5: A4縦ページ分割 UX） */
.no-print { }
.echart-container { page-break-inside: avoid; break-inside: avoid; }

/* 印刷想定セクション境界（画面表示時でもページ分割位置の可視化はしない） */
.print-page-break { page-break-before: always; break-before: page; }

@media print {
  * {
    -webkit-print-color-adjust: exact;
    print-color-adjust: exact;
  }
  .no-print { display: none !important; }
  body {
    padding: 0;
    background: #fff !important;
    color: #0f172a !important;
    font-size: 10.5pt;
  }
  body.theme-dark { background: #fff !important; color: #0f172a !important; }
  body.theme-dark table th { background: var(--c-primary) !important; color: #fff !important; }
  body.theme-dark table td { background: transparent !important; color: #0f172a !important; }

  /* セクション境界：主要セクションは次ページから */
  .section { page-break-inside: avoid; break-inside: avoid; }
  .section.page-start,
  .section.print-page-break { page-break-before: always; break-before: page; }

  /* Executive Summary は必ず次ページから本編 */
  .exec-summary { page-break-after: always; break-after: page; }

  /* 主要サブ要素のページ内維持 */
  .summary-card, .kpi-card, .stat-box, .comparison-card, .exec-summary-action,
  .hw-area-row, .hw-enrichment-table tr {
    box-shadow: none !important;
    transform: none !important;
    page-break-inside: avoid;
    break-inside: avoid;
  }
  .echart, .echart-container { break-inside: avoid; page-break-inside: avoid; }
  .sortable-table th::after { display: none; }
  table { border-collapse: collapse; }
  thead { display: table-header-group; } /* 次ページに header 再表示 */
  tfoot { display: table-footer-group; }
  tr { page-break-inside: avoid; break-inside: avoid; }

  .cover-page {
    page-break-after: always;
    break-after: page;
    border: none;
    background: #fff !important;
    min-height: 90vh;
  }
  .cover-comment {
    border: 1px dashed #ccc !important;
    background: transparent !important;
    color: #0f172a !important;
  }
  .screen-footer { display: none !important; }

  /* 見出し孤立防止 */
  h2, h3 { page-break-after: avoid; break-after: avoid; }
  p, li { orphans: 3; widows: 3; }
}
"#.to_string()
}

// ============================================================
// ヘッダー/ターゲット領域表示（表紙・Executive Summary 用）
// ============================================================

/// 対象地域を人間可読形式で組み立てる（例: "東京都 千代田区" / "全国"）
fn compose_target_region(agg: &SurveyAggregation) -> String {
    match (&agg.dominant_prefecture, &agg.dominant_municipality) {
        (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => format!("{} {}", p, m),
        (Some(p), _) if !p.is_empty() => p.clone(),
        _ => "全国".to_string(),
    }
}

// ============================================================
// Executive Summary (Section 1 / 仕様書 3章)
// ============================================================

/// 仕様書 3章: 5 KPI + 推奨優先アクション 3 件 + スコープ注意 2 行
/// 1 ページ完結、表紙直後に配置。アクションは severity 高い順に上から最大 3 件。
fn render_section_executive_summary(
    html: &mut String,
    agg: &SurveyAggregation,
    _seeker: &JobSeekerAnalysis,
    _by_company: &[CompanyAgg],
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
) {
    html.push_str("<section class=\"section exec-summary\" role=\"region\" aria-labelledby=\"exec-sum-title\">\n");
    html.push_str("<h2 id=\"exec-sum-title\">Executive Summary</h2>\n");
    html.push_str(&format!(
        "<p class=\"section-header-meta\">対象: {} / 3分間で読み切れる全体要旨</p>\n",
        escape_html(&compose_target_region(agg))
    ));

    // ---- 5 KPI ----
    // 仕様書 3.3 の定義に厳密に従う
    // K1: サンプル件数
    let k1_value = format_number(agg.total_count as i64);
    // K2: 主要地域
    let k2_value = compose_target_region(agg);
    // K3: 主要雇用形態（件数最多）
    let k3_value: String = if let Some((name, count)) = agg.by_employment_type.first() {
        let pct = if agg.total_count > 0 {
            *count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        format!("{} ({:.0}%)", name, pct)
    } else {
        "-".to_string()
    };
    // K4: 給与中央値（雇用形態グループ別のネイティブ単位を優先）
    // 2026-04-24 Phase 2: 正社員 月給 / パート 時給 を並列表示して
    //   「月給/時給の単位が混ざって直感と合わない」問題を解消
    let k4_value = {
        let mut parts: Vec<String> = Vec::new();
        for g in &agg.by_emp_group_native {
            if g.count == 0 {
                continue;
            }
            let v_str = if g.native_unit == "時給" {
                format!("{}円", format_number(g.median))
            } else {
                format!("{:.1}万円", g.median as f64 / 10_000.0)
            };
            parts.push(format!(
                "{} ({}): {} (n={})",
                g.group_label, g.native_unit, v_str, g.count
            ));
        }
        if parts.is_empty() {
            match &agg.enhanced_stats {
                Some(s) if s.count > 0 => {
                    if agg.is_hourly {
                        format!("時給 {} 円", format_number(s.median))
                    } else {
                        format!("月給 {} 円", format_number(s.median))
                    }
                }
                _ => "算出不能 (サンプル不足)".to_string(),
            }
        } else {
            parts.join(" / ")
        }
    };
    // K5: 新着比率
    let k5_value = if agg.total_count > 0 && agg.new_count > 0 {
        format!(
            "{:.1}%",
            agg.new_count as f64 / agg.total_count as f64 * 100.0
        )
    } else if agg.total_count == 0 {
        "-".to_string()
    } else {
        "0.0%".to_string()
    };

    html.push_str("<div class=\"exec-kpi-grid\">\n");
    render_kpi_card(html, "サンプル件数", &k1_value, "件");
    render_kpi_card(html, "主要地域", &k2_value, "");
    render_kpi_card(html, "主要雇用形態", &k3_value, "");
    render_kpi_card(html, "給与中央値", &k4_value, "");
    render_kpi_card(html, "新着比率", &k5_value, "");
    html.push_str("</div>\n");

    // ---- 推奨優先アクション 3 件 ----
    html.push_str("<h3>推奨優先アクション候補（件数・差分条件を満たすもの）</h3>\n");
    let actions = build_exec_actions(agg, by_emp_type_salary, hw_context);
    if actions.is_empty() {
        html.push_str(
            "<div class=\"exec-summary-action\"><div class=\"action-body\">\
            現時点では該当条件を満たすアクション候補はありません。\
            各セクションの詳細を順にご確認ください。</div></div>\n",
        );
    } else {
        html.push_str("<div class=\"exec-action-list\">\n");
        for (idx, (sev, title, body, xref)) in actions.iter().enumerate() {
            html.push_str("<div class=\"exec-summary-action\">\n");
            html.push_str("<div class=\"action-head\">");
            html.push_str(&severity_badge(*sev));
            html.push_str(&format!(
                " <span>{}. {}</span>",
                idx + 1,
                escape_html(title)
            ));
            html.push_str("</div>\n");
            html.push_str(&format!(
                "<div class=\"action-body\" contenteditable=\"true\" spellcheck=\"false\">{}</div>\n",
                escape_html(body)
            ));
            html.push_str(&format!(
                "<div class=\"action-xref\">{}</div>\n",
                escape_html(xref)
            ));
            html.push_str("</div>\n");
        }
        html.push_str("</div>\n");
    }

    // ---- スコープ注意書き (必須 / 仕様書 3.5) ----
    // 2026-04-24 修正: CSV は Indeed/求人ボックス等の媒体由来なので「HW 掲載求人のみ」
    // 表現は誤り。CSV 側と HW 側それぞれのスコープを明示。
    let outlier_note = if agg.outliers_removed_total > 0 {
        format!(
            "<br>\u{203B} 給与統計は IQR 法（Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR）で外れ値 {} 件を除外した後の値です（除外前 {} 件、除外後 {} 件）。\
            雇用形態グループ別集計も各グループ内で同手法の外れ値除外を適用済。",
            agg.outliers_removed_total,
            agg.salary_values_raw_count,
            agg.salary_values_raw_count.saturating_sub(agg.outliers_removed_total),
        )
    } else {
        "<br>\u{203B} 給与統計は IQR 法（Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR）で外れ値除外を適用済（除外対象なし）。".to_string()
    };

    html.push_str(&format!(
        "<div class=\"exec-scope-note\">\
        \u{203B} 本レポートはアップロード CSV（媒体: Indeed / 求人ボックス等）の分析が主で、\
        HW データは比較参考値として併記しています。CSV はスクレイピング範囲に依存し、\
        HW は掲載求人に限定されるため、どちらも全求人市場の代表ではありません。<br>\
        \u{203B} 示唆は相関に基づく仮説であり、因果を証明するものではない。\
        実施判断は現場文脈に依存します。{}\
        </div>\n",
        outlier_note
    ));

    html.push_str("</section>\n");
}

/// Executive Summary の 3 件アクションを算出（severity 降順、最大3件）
/// 仕様書 3.4 の閾値と文言テンプレートに従う
fn build_exec_actions(
    agg: &SurveyAggregation,
    by_emp_type_salary: &[EmpTypeSalary],
    hw_context: Option<&InsightContext>,
) -> Vec<(RptSev, String, String, String)> {
    let mut out: Vec<(RptSev, String, String, String)> = Vec::new();

    // A: 給与ギャップ（当サンプル中央値 vs HW 市場中央値）
    // 月給データのときのみ有効（is_hourly 時はスキップ）
    if !agg.is_hourly {
        let csv_median = agg.enhanced_stats.as_ref().map(|s| s.median).unwrap_or(0);
        let hw_median: i64 = if let Some(ctx) = hw_context {
            // ts_salary の avg_salary_min 値を平均化して参考値に
            let vals: Vec<f64> = ctx
                .ts_salary
                .iter()
                .map(|r| get_f64(r, "avg_salary_min"))
                .filter(|&v| v > 0.0)
                .collect();
            if !vals.is_empty() {
                (vals.iter().sum::<f64>() / vals.len() as f64) as i64
            } else {
                0
            }
        } else {
            0
        };
        if csv_median > 0 && hw_median > 0 {
            let diff = hw_median - csv_median;
            let abs_diff = diff.abs();
            if abs_diff >= 20_000 {
                let direction = if diff > 0 {
                    "引き上げる"
                } else {
                    "再確認する"
                };
                out.push((
                    RptSev::Critical,
                    format!(
                        "給与下限を月 {:+.1} 万円 {} 候補",
                        diff as f64 / 10_000.0,
                        direction
                    ),
                    format!(
                        "当サンプル中央値 {:.1} 万円 / 該当市区町村 HW 中央値 {:.1} 万円で {:.1} 万円差。",
                        csv_median as f64 / 10_000.0,
                        hw_median as f64 / 10_000.0,
                        abs_diff as f64 / 10_000.0
                    ),
                    "(Section 6 / Section 8 参照)".to_string(),
                ));
            } else if abs_diff >= 10_000 {
                let direction = if diff > 0 {
                    "引き上げる"
                } else {
                    "再確認する"
                };
                out.push((
                    RptSev::Warning,
                    format!(
                        "給与下限を月 {:+.1} 万円 {} 候補",
                        diff as f64 / 10_000.0,
                        direction
                    ),
                    format!(
                        "当サンプル中央値 {:.1} 万円 / 該当市区町村 HW 中央値 {:.1} 万円で {:.1} 万円差。",
                        csv_median as f64 / 10_000.0,
                        hw_median as f64 / 10_000.0,
                        abs_diff as f64 / 10_000.0
                    ),
                    "(Section 6 / Section 8 参照)".to_string(),
                ));
            }
        }
    }

    // B: 雇用形態構成差（正社員構成比 vs HW）
    if let Some(ctx) = hw_context {
        // CSV 側: 正社員(正職員含む)構成比
        let total_emp: usize = by_emp_type_salary.iter().map(|e| e.count).sum();
        let fulltime_count: usize = by_emp_type_salary
            .iter()
            .filter(|e| e.emp_type.contains("正社員") || e.emp_type.contains("正職員"))
            .map(|e| e.count)
            .sum();
        let csv_rate = if total_emp > 0 {
            fulltime_count as f64 / total_emp as f64 * 100.0
        } else {
            -1.0
        };
        // HW 側
        let hw_total: f64 = ctx.vacancy.iter().map(|r| get_f64(r, "total_count")).sum();
        let hw_ft: f64 = ctx
            .vacancy
            .iter()
            .filter(|r| super::super::helpers::get_str_ref(r, "emp_group") == "正社員")
            .map(|r| get_f64(r, "total_count"))
            .sum();
        let hw_rate = if hw_total > 0.0 {
            hw_ft / hw_total * 100.0
        } else {
            -1.0
        };
        if csv_rate >= 0.0 && hw_rate >= 0.0 {
            let diff = (csv_rate - hw_rate).abs();
            if diff >= 15.0 {
                out.push((
                    RptSev::Warning,
                    "雇用形態「正社員」の構成比を見直す候補".to_string(),
                    format!(
                        "当サンプル {:.1}% / HW 市場 {:.1}% で {:.1}pt 差。",
                        csv_rate, hw_rate, diff
                    ),
                    "(Section 4 参照)".to_string(),
                ));
            }
        }
    }

    // C: タグプレミアム（diff_percent > 5%, count >= 10 の最大 1 件）
    let candidate_tag = agg
        .by_tag_salary
        .iter()
        .filter(|t| t.count >= 10 && t.diff_percent.abs() > 5.0)
        .max_by(|a, b| {
            a.diff_percent
                .abs()
                .partial_cmp(&b.diff_percent.abs())
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    if let Some(t) = candidate_tag {
        let direction = if t.diff_from_avg > 0 {
            "プレミアム要因の可能性"
        } else {
            "ディスカウント要因の可能性"
        };
        out.push((
            RptSev::Info,
            format!("訴求タグ「{}」の給与差分", t.tag),
            format!(
                "該当タグ平均が全体比 {:+.1} 万円 ({:+.1}%、n={})。{}（相関であり因果は別途検討）。",
                t.diff_from_avg as f64 / 10_000.0,
                t.diff_percent,
                t.count,
                direction
            ),
            "(Section 10 参照)".to_string(),
        ));
    }

    // severity 降順で並べて最大 3 件
    out.sort_by_key(|(sev, _, _, _)| match sev {
        RptSev::Critical => 0,
        RptSev::Warning => 1,
        RptSev::Info => 2,
        RptSev::Positive => 3,
    });
    out.truncate(3);
    out
}

/// Executive Summary 用 KPI カード
fn render_kpi_card(html: &mut String, label: &str, value: &str, unit: &str) {
    html.push_str("<div class=\"kpi-card\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<div class=\"unit\">{}</div>\n",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
}

// ============================================================
// セクション1: サマリー
// ============================================================

fn render_section_summary(html: &mut String, agg: &SurveyAggregation) {
    let salary_label = if agg.is_hourly {
        "平均時給"
    } else {
        "平均月給"
    };
    let salary_unit = if agg.is_hourly { "円" } else { "万円" };

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>サマリー</h2>\n");

    // KPIカード 2x2
    let avg_salary_display = agg
        .enhanced_stats
        .as_ref()
        .map(|s| {
            if agg.is_hourly {
                format_number(s.mean)
            } else {
                format!("{:.1}", s.mean as f64 / 10_000.0)
            }
        })
        .unwrap_or_else(|| "-".to_string());

    // 正社員率の計算
    let fulltime_count = agg
        .by_employment_type
        .iter()
        .filter(|(t, _)| t.contains("正社員") || t.contains("正職員"))
        .map(|(_, c)| c)
        .sum::<usize>();
    let fulltime_rate = if agg.total_count > 0 {
        format!(
            "{:.1}%",
            fulltime_count as f64 / agg.total_count as f64 * 100.0
        )
    } else {
        "-".to_string()
    };

    // 2026-04-24 ユーザー指摘反映:
    //   - 「掲載企業数」KPI 削除（CSV は任意スクレイピング件数なので母集団を示さず誤解）
    //   - 「正社員率」→「CSV内 正社員割合」: 「安定雇用が多い市場」表現は不正確なので削除
    //   - 新着率は CSV 側に新着列がある場合のみ表示（無ければ KPI 自体を省略）
    let has_new_rate = agg.new_count > 0;

    html.push_str("<div class=\"summary-grid\">\n");
    render_summary_card(
        html,
        "CSV上の求人件数",
        &format_number(agg.total_count as i64),
        "件",
    );
    render_summary_card(html, salary_label, &avg_salary_display, salary_unit);
    render_summary_card(html, "CSV内 正社員割合", &fulltime_rate, "");
    if has_new_rate {
        let nr = format!(
            "{:.1}%",
            agg.new_count as f64 / agg.total_count.max(1) as f64 * 100.0
        );
        render_summary_card(html, "新着率", &nr, "");
    }
    html.push_str("</div>\n");

    // 読み方ガイド
    let salary_guide = if agg.is_hourly {
        "CSV 行の時給平均値。月給・年俸は時給へ換算。"
    } else {
        "CSV 行の月給換算平均（時給・年俸は月給へ統一計算）。"
    };
    html.push_str("<div class=\"guide-grid\">\n");
    render_guide_item(
        html,
        "CSV上の求人件数",
        "アップロードされた CSV 行数。CSV スクレイピング範囲に依存するため市場全体の指標ではありません。",
    );
    render_guide_item(html, salary_label, salary_guide);
    render_guide_item(
        html,
        "CSV内 正社員割合",
        "CSV 内で雇用形態「正社員・正職員」の行が占める比率。ソース媒体の収集方針により値は変動します。",
    );
    if has_new_rate {
        render_guide_item(
            html,
            "新着率",
            "CSV 行のうち「新着」フラグが付与された比率。",
        );
    }
    html.push_str("</div>\n");

    html.push_str("</div>\n");
}

fn render_summary_card(html: &mut String, label: &str, value: &str, unit: &str) {
    html.push_str("<div class=\"summary-card\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<div class=\"unit\">{}</div>\n",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
}

fn render_guide_item(html: &mut String, title: &str, description: &str) {
    html.push_str("<div class=\"guide-item\">\n");
    html.push_str(&format!(
        "<div class=\"guide-title\">{}</div>\n",
        escape_html(title)
    ));
    html.push_str(&format!("{}\n", escape_html(description)));
    html.push_str("</div>\n");
}

// ============================================================
// Section H: 地域 × HW データ連携（2026-04-24 追加要件 4）
// ============================================================

/// CSV 住所（prefecture × municipality）× HW DB 連携セクション
///
/// 表示項目: 都道府県 / 市区町村 / HW現在件数 / 3ヶ月推移 / 1年推移 / 欠員率（外部統計）
///
/// # データ源
/// - HW 現在件数: `hw_enrichment::enrich_areas` 相当を `hw_context` から導出
///   （postings ローカル集計は handlers 層で行う前提だが、現行シグネチャでは
///   InsightContext のみが入力のため、`ctx.vacancy` の `total_count` から近似）
/// - 3ヶ月/1年推移: `ctx.ts_counts` の snapshot_id 時系列から集計
/// - 欠員率: `ctx.vacancy` の emp_group = 正社員 の `vacancy_rate`（外部統計連携値）
/// 市区町村粒度 HW enrichment map を受け取るバージョン
///
/// handlers.rs で `hw_enrichment::enrich_areas` を呼び、(pref,muni) ごとの
/// HW 現在件数 / 推移 / 欠員率 を渡す。都道府県粒度コピーのバグを解消。
fn render_section_hw_enrichment(
    html: &mut String,
    agg: &SurveyAggregation,
    ctx: &InsightContext,
    enrichment_map: &std::collections::HashMap<String, HwAreaEnrichment>,
) {
    let pairs: Vec<(String, String, usize)> = {
        let mut seen: std::collections::HashSet<(String, String)> =
            std::collections::HashSet::new();
        let mut v: Vec<(String, String, usize)> = Vec::new();
        for m in &agg.by_municipality_salary {
            let key = (m.prefecture.clone(), m.name.clone());
            if !m.prefecture.is_empty() && !m.name.is_empty() && seen.insert(key) {
                v.push((m.prefecture.clone(), m.name.clone(), m.count));
            }
        }
        v.sort_by(|a, b| b.2.cmp(&a.2));
        v
    };

    if pairs.is_empty() && agg.by_prefecture.is_empty() {
        return;
    }

    // フォールバック: map が空または map に無いエントリ用に ctx からの単一値を用意
    let (fallback_3m, fallback_1y) = compute_posting_change_from_ts(ctx);
    let fallback_vacancy: Option<f64> = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .filter(|v| *v > 0.0)
        .map(|v| v * 100.0);

    let rows: Vec<(HwAreaEnrichment, usize)> = pairs
        .iter()
        .take(15)
        .map(|(pref, muni, count)| {
            let key = format!("{}:{}", pref, muni);
            let enrich = enrichment_map
                .get(&key)
                .cloned()
                .unwrap_or_else(|| HwAreaEnrichment {
                    prefecture: pref.clone(),
                    municipality: muni.clone(),
                    hw_posting_count: 0,
                    posting_change_3m_pct: fallback_3m,
                    posting_change_1y_pct: fallback_1y,
                    vacancy_rate_pct: fallback_vacancy,
                });
            (enrich, *count)
        })
        .collect();

    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"hw-enrich-title\">\n",
    );
    html.push_str("<h2 id=\"hw-enrich-title\">地域 × HW データ連携</h2>\n");
    html.push_str(
        "<p class=\"section-header-meta\">\
         アップロード CSV の地域情報を HW postings の市区町村実件数と突合。</p>\n",
    );
    // 2026-04-24: build_hw_enrichment_sowhat は ts_turso_counts の初期ノイズで
    //   「+374.3%」など暴れやすく誤誘導になるため非表示化。欠員率（外部統計）
    //   のみ意味があるケースで別途言及する運用にする。
    let _ = (fallback_3m, fallback_1y);
    if let Some(vrate) = fallback_vacancy {
        html.push_str(
            "<div class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">",
        );
        html.push_str(&format!(
            "※ {} 正社員欠員補充率（求人理由が「欠員補充」の比率、外部統計 e-Stat 由来）は {:.1}%。\
             この値は都道府県粒度の単一値であり、市区町村別の差は反映していません。",
            escape_html(&rows.first().map(|(e, _)| e.prefecture.clone()).unwrap_or_default()),
            vrate
        ));
        html.push_str("</div>\n");
    }
    html.push_str("<table class=\"hw-enrichment-table\">\n");
    html.push_str(
        "<thead><tr>\
         <th>都道府県</th>\
         <th>市区町村</th>\
         <th class=\"num\">CSV件数</th>\
         <th class=\"num\">HW現在件数</th>\
         </tr></thead><tbody>\n",
    );
    // 2026-04-24 ユーザー指摘:
    //   旧実装は 3ヶ月推移/1年推移/欠員率 を各行に出したが、これらは都道府県
    //   粒度の単一値を全行に同じ値で表示していたため誤誘導だった。
    //   また ts_turso_counts 由来の変動率は初期スナップショットのノイズで
    //   「+374.3%」など現実離れした値が出やすく、実用性が低い。
    //   → テーブルからは市区町村粒度で確実に取れる CSV 件数 / HW 件数 のみに
    //      絞り、推移・欠員率は「注記」として都道府県代表値で別記する。
    for (e, csv_count) in &rows {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", escape_html(&e.prefecture)));
        html.push_str(&format!("<td>{}</td>", escape_html(&e.municipality)));
        html.push_str(&format!("<td class=\"num\">{}</td>", csv_count));
        html.push_str(&format!(
            "<td class=\"num\">{}</td>",
            if e.hw_posting_count > 0 {
                format!("{}", e.hw_posting_count)
            } else {
                "—".to_string()
            }
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p class=\"print-note\">\
         ※ 表示は「CSV 件数（アップロード行数）」と「HW 現在件数（HW postings の市区町村実件数）」の 2 軸。\
         CSV 件数は掲載媒体スクレイピング範囲に依存し、HW 件数はハローワーク側の掲載求人のみ。\
         単純比較ではなく、どのエリアに媒体側の露出が集中しているかの参考値として参照してください。</p>\n",
    );
    html.push_str("</section>\n");
}

/// ts_counts から posting_count 合計の 3m / 1y 変化率 (%) を算出
/// 戻り値: (change_3m_pct, change_1y_pct)
fn compute_posting_change_from_ts(ctx: &InsightContext) -> (Option<f64>, Option<f64>) {
    if ctx.ts_counts.is_empty() {
        return (None, None);
    }
    // snapshot_id → posting_count 合計 を集計
    use std::collections::BTreeMap;
    let mut by_snap: BTreeMap<String, f64> = BTreeMap::new();
    for r in &ctx.ts_counts {
        let snap = get_str_ref(r, "snapshot_id").to_string();
        if snap.is_empty() {
            continue;
        }
        let cnt = get_f64(r, "posting_count");
        *by_snap.entry(snap).or_insert(0.0) += cnt;
    }
    if by_snap.is_empty() {
        return (None, None);
    }
    // 昇順 → 末尾が最新
    let mut entries: Vec<(String, f64)> = by_snap.into_iter().collect();
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    let n = entries.len();
    let latest = entries[n - 1].1;
    if latest <= 0.0 {
        return (None, None);
    }
    // 3m 前 = -3, 1y 前 = -12（月次 snapshot 前提）
    let change_3m = if n >= 4 {
        let prev = entries[n - 4].1;
        if prev > 0.0 {
            Some((latest - prev) / prev * 100.0)
        } else {
            None
        }
    } else {
        None
    };
    let change_1y = if n >= 13 {
        let prev = entries[n - 13].1;
        if prev > 0.0 {
            Some((latest - prev) / prev * 100.0)
        } else {
            None
        }
    } else {
        None
    };
    (change_3m, change_1y)
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
        html.push_str(&format!(
            "<div class=\"diff {}\">{}</div>\n",
            cls,
            escape_html(d)
        ));
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
    html.push_str(
        "<p class=\"guide\" style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>CSV（媒体データ）とハローワーク全体データを\
        <strong>雇用形態ごと</strong>に並列比較。媒体に出現する雇用形態を動的に検出し、\
        対応するHWデータと同条件で比較します。\
    </p>\n",
    );

    // CSV側の雇用形態を正規化してHW側のemp_group（正社員/パート/その他）にマッピング
    // - "正社員"/"正職員" → HW "正社員"
    // - "パート"/"アルバイト" → HW "パート"
    // - "契約社員"/"派遣社員"/その他 → HW "その他"
    fn normalize_emp_type(csv_type: &str) -> Option<&'static str> {
        if csv_type.contains("正社員") || csv_type.contains("正職員") {
            Some("正社員")
        } else if csv_type.contains("パート") || csv_type.contains("アルバイト") {
            Some("パート")
        } else if csv_type.contains("契約")
            || csv_type.contains("派遣")
            || csv_type.contains("嘱託")
            || csv_type.contains("臨時")
            || csv_type.contains("その他")
        {
            Some("その他")
        } else {
            None
        }
    }

    // CSV側の雇用形態を集約（HWのemp_groupごとに合算）
    use std::collections::HashMap;
    let mut csv_by_hw_group: HashMap<&str, (usize, i64)> = HashMap::new(); // (件数, 給与合計)
    for e in by_emp_type_salary {
        if let Some(hw_key) = normalize_emp_type(&e.emp_type) {
            let entry = csv_by_hw_group.entry(hw_key).or_insert((0, 0));
            entry.0 += e.count;
            entry.1 += e.avg_salary * e.count as i64;
        }
    }

    // 対象とする雇用形態を出現順に並べる（正社員 > パート > その他）
    let emp_order = ["正社員", "パート", "その他"];
    let present_groups: Vec<&str> = emp_order
        .iter()
        .filter(|g| csv_by_hw_group.contains_key(*g))
        .copied()
        .collect();

    if present_groups.is_empty() {
        html.push_str(
            "<p style=\"color:#888;font-size:10pt;\">\
            CSVデータの雇用形態が判別できなかったため、HW比較をスキップしました。</p>\n",
        );
        html.push_str("</div>\n");
        return;
    }

    // --- 雇用形態別 平均月給比較（動的生成） ---
    if !agg.is_hourly {
        html.push_str("<h3 style=\"font-size:11pt;margin:8px 0;\">雇用形態別 平均月給比較</h3>\n");
        html.push_str("<div class=\"comparison-grid\">\n");
        for &group in &present_groups {
            let (count, salary_sum) = csv_by_hw_group[group];
            let csv_avg = if count > 0 {
                salary_sum / count as i64
            } else {
                0
            };
            let csv_display = if csv_avg > 0 {
                format!("{:.1}万円 (n={})", csv_avg as f64 / 10_000.0, count)
            } else {
                format!("- (n={})", count)
            };

            // HW側: cascade は industry_raw × emp_group の複合集計のため、
            // 同じ emp_group の全業種の avg_salary_min を平均化する
            let salaries: Vec<f64> = ctx
                .cascade
                .iter()
                .filter(|r| super::super::helpers::get_str_ref(r, "emp_group") == group)
                .map(|r| get_f64(r, "avg_salary_min"))
                .filter(|&v| v > 0.0)
                .collect();
            let hw_avg: i64 = if !salaries.is_empty() {
                (salaries.iter().sum::<f64>() / salaries.len() as f64) as i64
            } else {
                0
            };
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
                &csv_display,
                &hw_display,
                diff_text.as_deref(),
                positive,
            );
        }
        html.push_str("</div>\n");
    }

    // --- 雇用形態構成比（媒体 vs HW） ---
    html.push_str("<h3 style=\"font-size:11pt;margin:16px 0 8px;\">雇用形態構成比</h3>\n");
    html.push_str("<div class=\"comparison-grid\">\n");

    // 雇用形態構成比（CSV vs HW の割合）を雇用形態ごとに表示
    let csv_total: usize = csv_by_hw_group.values().map(|(c, _)| c).sum();
    let hw_total: i64 = ctx
        .vacancy
        .iter()
        .map(|r| get_f64(r, "total_count") as i64)
        .sum();

    for &group in &present_groups {
        let (csv_count, _) = csv_by_hw_group[group];
        let csv_rate = if csv_total > 0 {
            csv_count as f64 / csv_total as f64 * 100.0
        } else {
            -1.0
        };
        let csv_display = if csv_rate >= 0.0 {
            format!("{:.1}% ({}件)", csv_rate, csv_count)
        } else {
            "-".to_string()
        };

        let hw_count: i64 = ctx
            .vacancy
            .iter()
            .find(|r| super::super::helpers::get_str_ref(r, "emp_group") == group)
            .map(|r| get_f64(r, "total_count") as i64)
            .unwrap_or(0);
        let hw_rate = if hw_total > 0 {
            hw_count as f64 / hw_total as f64 * 100.0
        } else {
            -1.0
        };
        let hw_display = if hw_rate >= 0.0 && hw_total > 0 {
            format!("{:.1}% ({}件)", hw_rate, format_number(hw_count))
        } else {
            "データなし".to_string()
        };

        let (diff_text, positive) = if csv_rate >= 0.0 && hw_rate >= 0.0 {
            let d = csv_rate - hw_rate;
            (Some(format!("{:+.1}pt", d)), d >= 0.0)
        } else {
            (None, true)
        };
        render_comparison_card(
            html,
            &format!("{} 構成比", group),
            &csv_display,
            &hw_display,
            diff_text.as_deref(),
            positive,
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
        ctx.ext_population
            .first()
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
        html,
        "対象地域の人口",
        &pop_display,
        &pop_source,
        None,
        true,
    );

    // --- カード4: 最低賃金比較（CSV平均下限の160h換算 vs 都道府県最低賃金） ---
    if !agg.is_hourly {
        // CSV平均下限（月給→時給160h換算）
        let csv_avg_min: i64 = if !agg.by_prefecture_salary.is_empty() {
            let total: i64 = agg
                .by_prefecture_salary
                .iter()
                .filter(|p| p.avg_min_salary > 0)
                .map(|p| p.avg_min_salary)
                .sum();
            let n = agg
                .by_prefecture_salary
                .iter()
                .filter(|p| p.avg_min_salary > 0)
                .count();
            if n > 0 {
                total / n as i64
            } else {
                0
            }
        } else {
            0
        };
        let csv_hourly = csv_avg_min / super::aggregator::HOURLY_TO_MONTHLY_HOURS;
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
            html,
            "最低賃金比較（167h換算）",
            &csv_display,
            &mw_display,
            mw_diff_text.as_deref(),
            mw_positive,
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
    render_stat_box(
        html,
        "給与範囲",
        &format!(
            "{} 〜 {}",
            format_man_yen(stats.min),
            format_man_yen(stats.max)
        ),
    );
    html.push_str("</div>\n");

    // 信頼区間・四分位がある場合
    if let Some(ci) = &stats.bootstrap_ci {
        html.push_str(&format!(
            "<p class=\"note\">95%信頼区間: {} 〜 {} (Bootstrap法, n={})</p>\n",
            format_man_yen(ci.lower),
            format_man_yen(ci.upper),
            ci.sample_size
        ));
    }

    // 下限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_min_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<h3>下限給与の分布（20,000円刻み）</h3>\n");
        let (labels, values, _b) = build_salary_histogram(salary_min_values, 20_000);
        let mode_min_20k = compute_mode(salary_min_values, 20_000);
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_20k,
            20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>下限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_min_values, 5_000);
        let mode_min_5k = compute_mode(salary_min_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_5k,
            5_000,
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
            &labels,
            &values,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_20k,
            20_000,
        );
        html.push_str(&render_echart_div(&config, 220));

        // 詳細分布（5,000円刻み）
        html.push_str("<h3>上限給与の分布（5,000円刻み）- 詳細</h3>\n");
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_max_values, 5_000);
        let mode_max_5k = compute_mode(salary_max_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_5k,
            5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
    }

    html.push_str("</div>\n");
}

fn render_stat_box(html: &mut String, label: &str, value: &str) {
    html.push_str("<div class=\"stat-box\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    html.push_str("</div>\n");
}

// ============================================================
// セクション4B: 雇用形態グループ別 ネイティブ単位集計（2026-04-24 Phase 2）
// 正社員 → 月給ベース / パート → 時給ベース で別々に集計・表示
// ============================================================

fn render_section_emp_group_native(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_emp_group_native.is_empty() {
        return;
    }
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"emp-group-native-title\">\n",
    );
    html.push_str(
        "<h2 id=\"emp-group-native-title\">雇用形態グループ別 給与分析（ネイティブ単位）</h2>\n",
    );
    html.push_str(
        "<p class=\"section-header-meta\">\
         正社員は月給、パートは時給、と各グループのネイティブ単位で集計。\
         単位の異なる給与を混ぜず、直感と一致する単位で評価します。</p>\n",
    );

    html.push_str("<div class=\"emp-group-grid\" style=\"display:grid;grid-template-columns:repeat(auto-fit,minmax(260px,1fr));gap:16px;margin-top:12px;\">\n");

    for group in &agg.by_emp_group_native {
        let unit_suffix = if group.native_unit == "時給" {
            "円"
        } else {
            "円"
        };
        let is_hourly = group.native_unit == "時給";
        // 月給は「万円表示」、時給は「円表示」
        let format_salary = |v: i64| -> String {
            if is_hourly {
                format!("{}円", format_number(v))
            } else {
                format!("{:.1}万円", v as f64 / 10_000.0)
            }
        };

        html.push_str(&format!(
            "<div class=\"emp-group-card\" style=\"border:1px solid var(--c-border);border-radius:8px;padding:14px 16px;background:var(--c-bg-card);\">\n"
        ));
        html.push_str(&format!(
            "<div style=\"font-size:13pt;font-weight:700;color:var(--c-primary);\">{}</div>\n",
            escape_html(&group.group_label)
        ));
        // 「n=100件 (IQR外れ値除外: 3件)」のような表示
        let count_display = if group.outliers_removed > 0 {
            format!(
                "n={}件（IQR で {} 件除外、除外前 {}）",
                format_number(group.count as i64),
                format_number(group.outliers_removed as i64),
                format_number(group.raw_count as i64)
            )
        } else {
            format!("n={}件", format_number(group.count as i64))
        };
        html.push_str(&format!(
            "<div style=\"font-size:10pt;color:var(--c-muted);margin-bottom:8px;\">集計単位: {} / {}</div>\n",
            escape_html(&group.native_unit),
            count_display
        ));
        html.push_str("<table style=\"width:100%;font-size:10.5pt;border-collapse:collapse;\">\n");
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">中央値</td><td style=\"padding:3px 0;text-align:right;font-weight:600;\">{}</td></tr>\n",
            format_salary(group.median)
        ));
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">平均値</td><td style=\"padding:3px 0;text-align:right;\">{}</td></tr>\n",
            format_salary(group.mean)
        ));
        html.push_str(&format!(
            "<tr><td style=\"padding:3px 0;color:var(--c-muted);\">範囲</td><td style=\"padding:3px 0;text-align:right;font-size:9pt;\">{} 〜 {}</td></tr>\n",
            format_salary(group.min),
            format_salary(group.max)
        ));
        html.push_str("</table>\n");

        if !group.included_emp_types.is_empty() {
            html.push_str(&format!(
                "<div style=\"margin-top:6px;font-size:9pt;color:var(--c-muted);\">含まれる雇用形態: {}</div>\n",
                escape_html(&group.included_emp_types.join(" / "))
            ));
        }
        let _ = unit_suffix;
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");

    html.push_str(
        "<p class=\"print-note\">\
         ※ 「正社員」グループは月給ベース（時給は ×167 で月給換算）、\
         「パート」グループは時給ベース（月給は /167 で時給換算）。\
         「派遣・その他」はグループ内多数派の単位を採用。<br>\
         ※ 各グループ内で IQR 法（Q1 − 1.5×IQR ～ Q3 + 1.5×IQR の範囲外）\
         による外れ値除外を適用。除外件数は各カード内に表示。</p>\n",
    );
    html.push_str("</section>\n");
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
    // So What 行: 件数の多い形態と給与差を 1 行で示す
    if let Some((top_name, top_count)) = agg.by_employment_type.first() {
        let top_pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        let top_salary = by_emp_type_salary
            .iter()
            .find(|e| &e.emp_type == top_name)
            .map(|e| format_man_yen(e.avg_salary))
            .unwrap_or_else(|| "-".to_string());
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多い形態は「{}」で {:.1}% を占め、平均月給は {}。</p>\n",
            escape_html(top_name),
            top_pct,
            escape_html(&top_salary)
        ));
    }

    // EChartsドーナツチャート TOP6
    let colors = [
        "#1565C0", "#E69F00", "#009E73", "#D55E00", "#CC79A7", "#56B4E9",
    ];
    let pie_data: Vec<serde_json::Value> = agg
        .by_employment_type
        .iter()
        .take(6)
        .enumerate()
        .map(|(i, (name, count))| {
            json!({
                "value": count,
                "name": name,
                "itemStyle": {"color": colors[i % colors.len()]}
            })
        })
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
    html.push_str("<h2>地域分析（都道府県）</h2>\n");
    // So What 行: 件数の多い都道府県と割合を 1 行で提示
    if let Some((top_pref, top_count)) = agg.by_prefecture.first() {
        let pct = if agg.total_count > 0 {
            *top_count as f64 / agg.total_count as f64 * 100.0
        } else {
            0.0
        };
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数が最も多いのは「{}」で全体の {:.1}%（件数の多い順に整理）。</p>\n",
            escape_html(top_pref),
            pct
        ));
    }
    html.push_str(
        "<p class=\"section-xref\">関連: Section 7（市区町村）/ Section 8（最低賃金）</p>\n",
    );

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

    // So What 行: 件数の多い法人と給与水準の傾向を 1 行で
    if let Some(top) = by_company.iter().max_by_key(|c| c.count) {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 掲載件数が最も多い法人は「{}」（{} 件、平均月給 {}）。\
             件数・給与の分布は以下のテーブルを参照（ソート可能）。</p>\n",
            escape_html(&top.name),
            format_number(top.count as i64),
            escape_html(&format_man_yen(top.avg_salary))
        ));
    }

    // 企業数サマリー
    html.push_str(&format!(
        "<p>分析対象企業数: <strong>{}</strong>社（給与情報のある求人を持つ企業のみ）</p>\n",
        format_number(by_company.len() as i64)
    ));

    // 市場集中度（HHI: Herfindahl-Hirschman Index）の計算と表示
    // HHI = Σ(各企業の求人シェア%)² / 公正取引委員会基準:
    //   < 1500: 分散型市場 / 1500-2500: 中程度集中 / > 2500: 集中型市場
    // サンプル数不足（企業数<3）時は非表示
    if by_company.len() >= 3 {
        let total_count: i64 = by_company.iter().map(|c| c.count as i64).sum();
        if total_count > 0 {
            let hhi: f64 = by_company
                .iter()
                .map(|c| {
                    let share_pct = c.count as f64 / total_count as f64 * 100.0;
                    share_pct * share_pct
                })
                .sum();
            let (judgment, color) = if hhi < 1500.0 {
                ("分散型市場（競合多数・多様な選択肢）", "var(--c-success)")
            } else if hhi < 2500.0 {
                ("中程度集中（主要プレイヤー複数）", "var(--c-warning)")
            } else {
                ("集中型市場（少数企業が支配的）", "var(--c-danger)")
            };
            html.push_str(&format!(
                "<p style=\"margin:8px 0;font-size:10pt;\">\
                 <strong>市場集中度（HHI）: <span style=\"color:{}\">{:.0}</span></strong> \
                 / 判定: <span style=\"color:{}\">{}</span> \
                 <span style=\"font-size:9pt;color:#888;\">（公正取引委員会基準: &lt;1500=分散 / 1500-2500=中程度 / &gt;2500=集中）</span>\
                 </p>\n",
                color, hhi, color, judgment
            ));
        }
    }

    // 掲載件数の多い法人 15 件（件数の多い順に整理、ソート可能テーブル）
    let mut by_count = by_company.to_vec();
    by_count.sort_by(|a, b| b.count.cmp(&a.count));

    html.push_str("<h3>掲載件数の多い法人 15 件（給与情報あり）</h3>\n");
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

    // 平均給与の多い法人 15 件（サンプル数に応じて閾値動的調整）
    let multi_count = by_company.iter().filter(|c| c.count >= 2).count();
    let min_count_threshold = if multi_count >= 15 { 2 } else { 1 };
    let mut by_salary: Vec<&CompanyAgg> = by_company
        .iter()
        .filter(|c| c.count >= min_count_threshold && c.avg_salary > 0)
        .collect();
    by_salary.sort_by(|a, b| b.avg_salary.cmp(&a.avg_salary));

    if !by_salary.is_empty() {
        let title = if min_count_threshold >= 2 {
            "給与水準の高い法人 15 件（給与付き2件以上の企業）"
        } else {
            "給与水準の高い法人 15 件（給与付き、1件求人含む。※1件は参考値）"
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
        "北海道" => Some(1075),
        "青森県" => Some(1029),
        "岩手県" => Some(1031),
        "宮城県" => Some(1038),
        "秋田県" => Some(1031),
        "山形県" => Some(1032),
        "福島県" => Some(1038),
        "茨城県" => Some(1074),
        "栃木県" => Some(1058),
        "群馬県" => Some(1063),
        "埼玉県" => Some(1141),
        "千葉県" => Some(1140),
        "東京都" => Some(1226),
        "神奈川県" => Some(1225),
        "新潟県" => Some(1050),
        "富山県" => Some(1062),
        "石川県" => Some(1054),
        "福井県" => Some(1053),
        "山梨県" => Some(1052),
        "長野県" => Some(1061),
        "岐阜県" => Some(1065),
        "静岡県" => Some(1097),
        "愛知県" => Some(1140),
        "三重県" => Some(1087),
        "滋賀県" => Some(1080),
        "京都府" => Some(1122),
        "大阪府" => Some(1177),
        "兵庫県" => Some(1116),
        "奈良県" => Some(1051),
        "和歌山県" => Some(1045),
        "鳥取県" => Some(1030),
        "島根県" => Some(1033),
        "岡山県" => Some(1047),
        "広島県" => Some(1085),
        "山口県" => Some(1043),
        "徳島県" => Some(1046),
        "香川県" => Some(1038),
        "愛媛県" => Some(1033),
        "高知県" => Some(1023),
        "福岡県" => Some(1057),
        "佐賀県" => Some(1030),
        "長崎県" => Some(1031),
        "熊本県" => Some(1034),
        "大分県" => Some(1035),
        "宮崎県" => Some(1023),
        "鹿児島県" => Some(1026),
        "沖縄県" => Some(1023),
        _ => None,
    }
}

const _MIN_WAGE_NATIONAL_AVG: i64 = 1121;

// ============================================================
// セクション3-3: 相関分析（散布図） → ECharts scatter
// ============================================================

fn render_section_scatter(html: &mut String, agg: &SurveyAggregation) {
    if agg.scatter_min_max.len() < 6 {
        return;
    }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>相関分析（散布図）</h2>\n");
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各点が1件の求人。回帰線（赤破線）は全体傾向。\
        R²（決定係数）は0〜1で、1に近いほど相関が強い。\
    </p>\n",
    );

    // ECharts scatter データ生成（最大200点）
    html.push_str("<h3>月給下限 vs 上限</h3>\n");

    // 異常値除外: 5万〜200万円の妥当な範囲、かつ上限≧下限
    // （時給や年収の月給換算ミスによる外れ値を排除）
    let filtered_points: Vec<&ScatterPoint> = agg
        .scatter_min_max
        .iter()
        .filter(|p| {
            let x_man = p.x as f64 / 10_000.0;
            let y_man = p.y as f64 / 10_000.0;
            (5.0..=200.0).contains(&x_man) && (5.0..=200.0).contains(&y_man) && y_man >= x_man
        })
        .collect();

    if filtered_points.len() < 6 {
        html.push_str("<p style=\"font-size:9pt;color:#888;\">有効なデータ点が不足しているため散布図を省略しました。</p>\n");
        html.push_str("</div>\n");
        return;
    }

    let scatter_data: Vec<serde_json::Value> = filtered_points
        .iter()
        .take(200)
        .map(|p| json!([p.x as f64 / 10_000.0, p.y as f64 / 10_000.0]))
        .collect();

    // 軸範囲をパーセンタイル(P2.5〜P97.5)基準で決定、5%マージン
    let mut x_vals_man: Vec<f64> = filtered_points
        .iter()
        .map(|p| p.x as f64 / 10_000.0)
        .collect();
    let mut y_vals_man: Vec<f64> = filtered_points
        .iter()
        .map(|p| p.y as f64 / 10_000.0)
        .collect();
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
        let strength = if reg.r_squared > 0.7 {
            "強い相関"
        } else if reg.r_squared > 0.4 {
            "中程度の相関"
        } else {
            "弱い相関"
        };
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
    if sorted.is_empty() {
        return 0.0;
    }
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
    if agg.by_prefecture_salary.is_empty() {
        return;
    }

    // 都道府県ごとに最低賃金比較データを構築
    struct MinWageEntry {
        name: String,
        avg_min: i64,
        min_wage: i64,
        hourly_160: i64, // 月給÷160h
        diff_160: i64,
        ratio_160: f64,
    }
    let mut entries: Vec<MinWageEntry> = agg
        .by_prefecture_salary
        .iter()
        .filter_map(|p| {
            let mw = min_wage_for_prefecture(&p.name)?;
            if p.avg_min_salary <= 0 {
                return None;
            }
            let hourly_160 = p.avg_min_salary / super::aggregator::HOURLY_TO_MONTHLY_HOURS;
            let diff_160 = hourly_160 - mw;
            let ratio_160 = hourly_160 as f64 / mw as f64;
            Some(MinWageEntry {
                name: p.name.clone(),
                avg_min: p.avg_min_salary,
                min_wage: mw,
                hourly_160,
                diff_160,
                ratio_160,
            })
        })
        .collect();

    if entries.is_empty() {
        return;
    }
    entries.sort_by(|a, b| a.diff_160.cmp(&b.diff_160)); // 差が小さい順

    // 全体の平均比率
    let avg_ratio: f64 = entries.iter().map(|e| e.ratio_160).sum::<f64>() / entries.len() as f64;
    let avg_diff_pct = (avg_ratio - 1.0) * 100.0;

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>最低賃金比較</h2>\n");
    // So What + severity badge（diff < 0 は Critical、< 50 は Warning、それ以外 Positive）
    let below_count = entries.iter().filter(|e| e.diff_160 < 0).count();
    let near_count = entries
        .iter()
        .filter(|e| e.diff_160 >= 0 && e.diff_160 < 50)
        .count();
    let sev = if below_count > 0 {
        RptSev::Critical
    } else if near_count > 0 {
        RptSev::Warning
    } else {
        RptSev::Positive
    };
    html.push_str(&format!(
        "<p class=\"section-sowhat\">{} {} 県で平均下限給与の 167h 換算が最低賃金を下回る傾向。\
         差が 50 円未満（要確認）: {} 県。該当求人群は労基上要確認。</p>\n",
        severity_badge(sev),
        below_count,
        near_count
    ));
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>月給を167h（8h×20.875日、厚労省基準）で割り時給換算して最低賃金と比較。\
        全国加重平均: <strong>1,121円</strong>（2025年10月施行）\
    </p>\n",
    );

    // 概要カード
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均最低賃金比率", &format!("{:.2}倍", avg_ratio));
    render_stat_box(html, "全体差分", &format!("{:+.1}%", avg_diff_pct));
    render_stat_box(html, "分析対象", &format!("{}都道府県", entries.len()));
    html.push_str("</div>\n");

    // 最低賃金との差が小さい都道府県 10 件（差額の小さい順に整理、ソート可能テーブル）
    html.push_str("<h3>時給換算で最低賃金に近い都道府県 10 件（差額の小さい順）</h3>\n");
    html.push_str("<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>都道府県</th><th style=\"text-align:right\">平均月給下限</th>\
        <th style=\"text-align:right\">167h換算</th><th style=\"text-align:right\">最低賃金</th>\
        <th style=\"text-align:right\">差額</th><th style=\"text-align:right\">比率</th></tr></thead>\n<tbody>\n");
    for (i, e) in entries.iter().take(10).enumerate() {
        let diff_color = if e.diff_160 < 0 {
            "negative"
        } else if e.diff_160 < 50 {
            "color:#fb8c00;font-weight:bold"
        } else {
            ""
        };
        let diff_style = if diff_color.starts_with("color:") {
            format!(" style=\"text-align:right;{}\"", diff_color)
        } else {
            format!(" class=\"num {}\"", diff_color)
        };
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}円</td>\
             <td{}>{:+}円</td><td class=\"num\">{:.2}倍</td></tr>\n",
            i + 1,
            escape_html(&e.name),
            format_man_yen(e.avg_min),
            format_number(e.hourly_160),
            format_number(e.min_wage),
            diff_style,
            e.diff_160,
            e.ratio_160,
        ));
    }
    html.push_str("</tbody></table>\n");

    // 活用ポイント
    html.push_str(
        "<div class=\"note\">\
        <strong>活用ポイント:</strong> 167h=所定労働時間（8h×20.875日、厚労省「就業条件総合調査 2024」基準）で換算。\
        最低賃金水準の求人は応募者が集まりにくい傾向。+10%以上の求人を優先検討すると効率的です。\
    </div>\n",
    );

    html.push_str("</div>\n");
}

// ============================================================
// セクション5-2: 市区町村別給与分析
// ============================================================

fn render_section_municipality_salary(html: &mut String, agg: &SurveyAggregation) {
    if agg.by_municipality_salary.is_empty() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>地域分析（市区町村）</h2>\n");
    // So What: 件数の多い市区町村の給与水準が最も高い先
    if let Some(top_hi_salary) = agg
        .by_municipality_salary
        .iter()
        .take(15)
        .max_by_key(|m| m.avg_salary)
    {
        html.push_str(&format!(
            "<p class=\"section-sowhat\">\u{203B} 件数の多い 15 市区町村のうち、平均月給が最も高いのは\
             「{} {}」で {}（同名異県を避けるため都道府県併記）。</p>\n",
            escape_html(&top_hi_salary.prefecture),
            escape_html(&top_hi_salary.name),
            escape_html(&format_man_yen(top_hi_salary.avg_salary))
        ));
    }
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>掲載件数の多い市区町村の給与水準を比較。\
        同じ都道府県内でも市区町村により給与差があります。\
    </p>\n",
    );

    html.push_str(
        "<table class=\"sortable-table\">\n<thead><tr><th>#</th><th>市区町村</th><th>都道府県</th>\
        <th style=\"text-align:right\">件数</th><th style=\"text-align:right\">平均月給</th>\
        <th style=\"text-align:right\">中央値</th></tr></thead>\n<tbody>\n",
    );
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
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各タグが付いた求人の平均給与と、全体平均との差を示します。\
        正の値（緑）=そのタグが付くと給与が高い傾向、負の値（赤）=低い傾向。\
    </p>\n",
    );

    html.push_str(&format!(
        "<p>全体平均月給: <strong>{}</strong></p>\n",
        format_man_yen(overall_mean)
    ));

    // タグ件数のツリーマップ（テーブルの上に配置）
    if !agg.by_tag_salary.is_empty() {
        let tree_data: Vec<serde_json::Value> = agg
            .by_tag_salary
            .iter()
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
        let significant: Vec<&TagSalaryAgg> = agg
            .by_tag_salary
            .iter()
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
            let diff_class = if ts.diff_from_avg > 0 {
                "positive"
            } else if ts.diff_from_avg < 0 {
                "negative"
            } else {
                ""
            };
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
                i + 1,
                escape_html(tag),
                format_number(*count as i64),
            ));
        }
        html.push_str("</tbody></table>\n");
    }

    html.push_str("</div>\n");
}

// ============================================================
// セクション10: 求職者心理分析
// ============================================================

/// 地域注目企業テーブル
/// Why: 求人市場分析レポートから実際にアプローチ可能な企業リストへ繋げる
/// How: employee_count 降順で従業員数の多い 30 社を印刷レポートに追加
///
/// 2026-04-24 追加要件 3: 表示項目刷新
/// - 削除: 信用スコア (credit_score) — struct には残すが UI 非表示
/// - 追加: 売上 (sales_amount / sales_range) / 1年人員推移 / 3ヶ月人員推移
///
/// 関数名は呼出側の互換のため残す（UI 表示文言のみ「地域注目企業」に統一）
fn render_section_salesnow_companies(html: &mut String, companies: &[NearbyCompany]) {
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"region-featured-title\">\n",
    );
    html.push_str("<h2 id=\"region-featured-title\">地域注目企業</h2>\n");
    html.push_str(
        "<p class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">\
        \u{203B} 地域内で従業員数の多い 30 社を整理しています。\
        HW 掲載件数が多い法人は採用が活発な傾向（相関であり、因果は別途検討）。\
        売上規模・人員推移も参考値として併記します。</p>\n",
    );
    html.push_str("<table class=\"data-table\">\n");
    html.push_str("<thead><tr>");
    for h in [
        "番号",
        "企業名",
        "都道府県",
        "業種",
        "従業員数",
        "売上",
        "1年人員推移",
        "3ヶ月人員推移",
        "HW求人数",
    ] {
        html.push_str(&format!("<th>{}</th>", escape_html(h)));
    }
    html.push_str("</tr></thead><tbody>\n");
    for (i, c) in companies.iter().take(30).enumerate() {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", i + 1));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.company_name)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.prefecture)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.sn_industry)));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.employee_count)
        ));
        // 売上: 金額と区分ラベルを併記
        let sales_cell = format_sales_cell(c.sales_amount, &c.sales_range);
        html.push_str(&format!("<td class=\"right\">{}</td>", sales_cell));
        // 1年推移 / 3ヶ月推移: 増減符号付きの %
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_1y)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_3m)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.hw_posting_count)
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table>\n");
    html.push_str("</section>\n");
}

/// 売上セル整形: 売上金額と区分ラベルを 1 セル 2 行で表示
fn format_sales_cell(amount: f64, range: &str) -> String {
    if amount <= 0.0 && range.is_empty() {
        return "-".to_string();
    }
    // 金額は百万円単位以上に丸めて表示
    let amount_display = if amount >= 1.0e9 {
        format!("{:.1} 億円", amount / 1.0e8)
    } else if amount >= 1.0e6 {
        format!("{:.0} 百万円", amount / 1.0e6)
    } else if amount > 0.0 {
        format!("{:.0} 円", amount)
    } else {
        "-".to_string()
    };
    let range_display = if range.is_empty() {
        String::new()
    } else {
        format!(
            "<br><span style=\"font-size:9pt;color:var(--c-text-muted);\">{}</span>",
            escape_html(range)
        )
    };
    format!("{}{}", escape_html(&amount_display), range_display)
}

/// 人員推移セル整形: 増減符号付き %、0 は横ばい
fn format_delta_cell(pct: f64) -> String {
    // NaN / 極端値ガード
    if !pct.is_finite() {
        return "-".to_string();
    }
    let cls = if pct > 0.5 {
        "trend-up"
    } else if pct < -0.5 {
        "trend-down"
    } else {
        "trend-flat"
    };
    format!("<span class=\"{}\">{:+.1}%</span>", cls, pct)
}

// ============================================================
// Section 13: 注記・出典・免責（必須・仕様書 4.12）
// ============================================================

/// スコープ制約、相関≠因果、データ限界を明示
/// 記載項目の文言は仕様書 4.12 に沿うこと（変更不可）
fn render_section_notes(html: &mut String, now: &str) {
    html.push_str("<section class=\"section\" role=\"region\" aria-labelledby=\"notes-title\">\n");
    html.push_str("<h2 id=\"notes-title\">注記・出典・免責</h2>\n");
    html.push_str(
        "<ol style=\"padding-left:1.4em;font-size:10pt;line-height:1.6;color:var(--text);\">\n",
    );
    html.push_str(
        "<li><strong>データスコープ</strong>: 本レポートはアップロード CSV（Indeed / 求人ボックス等）\
        の行に基づく分析が主で、HW 掲載データは比較参考値として併記している。\
        CSV はスクレイピング範囲に依存し、HW は掲載求人のみに限定されるため、\
        いずれも全求人市場を代表するものではない。\
        職業紹介事業者の求人・非公開求人は本レポートに含まれない。</li>\n",
    );
    html.push_str(
        "<li><strong>給与バイアス</strong>: HW 掲載求人は中小企業・地方案件の比率が高く民間媒体より\
        給与水準が低く出る傾向がある。CSV 側も掲載元媒体のバイアスを内包するため、\
        両者の単純比較には注意が必要。</li>\n",
    );
    html.push_str(
        "<li><strong>相関と因果</strong>: 本レポートに記載する「傾向」「相関」は因果関係を\
        証明するものではない。示唆は仮説であり、実施判断は現場文脈に依存する。</li>\n",
    );
    html.push_str(
        "<li><strong>外れ値処理</strong>: 給与統計（中央値・平均・グループ別集計）は IQR 法\
        （Q1 − 1.5×IQR 〜 Q3 + 1.5×IQR の範囲外を除外）を適用済。\
        雇用形態グループ別集計も各グループ内で同手法の除外を実行。\
        除外件数は Executive Summary および各カード内に明示表示。</li>\n",
    );
    html.push_str(
        "<li><strong>サンプル件数と求人件数</strong>: 本レポートの「サンプル件数」は分析対象求人数で\
        あり、地域全体の求人件数ではない。</li>\n",
    );
    html.push_str(
        "<li><strong>出典</strong>: データ源 - アップロード CSV / ハローワーク公開データ / \
        地域注目企業データベース / e-Stat。</li>\n",
    );
    html.push_str(&format!(
        "<li><strong>生成元</strong>: 株式会社For A-career / 生成日時: {}</li>\n",
        escape_html(now)
    ));
    html.push_str("</ol>\n");
    html.push_str("</section>\n");
}

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
        render_stat_box(
            html,
            "平均レンジ幅",
            &format_man_yen(perception.avg_range_width),
        );
        render_stat_box(html, "平均下限", &format_man_yen(perception.avg_lower));
        render_stat_box(
            html,
            "求職者期待値",
            &format_man_yen(perception.expected_point),
        );
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
        html.push_str(&format!(
            "<div class=\"label\">経験者求人 ({}件)</div>\n",
            format_number(inexp.experience_count as i64)
        ));
        if let Some(avg) = inexp.experience_avg_salary {
            html.push_str(&format!(
                "<div class=\"value\">{}</div>\n",
                format_man_yen(avg)
            ));
        } else {
            html.push_str("<div class=\"value\">-</div>\n");
        }
        html.push_str("</div>\n");

        // 未経験者
        html.push_str("<div class=\"stat-box\">\n");
        html.push_str(&format!(
            "<div class=\"label\">未経験可求人 ({}件)</div>\n",
            format_number(inexp.inexperience_count as i64)
        ));
        if let Some(avg) = inexp.inexperience_avg_salary {
            html.push_str(&format!(
                "<div class=\"value\">{}</div>\n",
                format_man_yen(avg)
            ));
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
    html.push_str(&format!(
        "<div style=\"font-size:10px;color:#666;\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div style=\"font-size:16px;font-weight:bold;\">{}件</div>\n",
        format_number(count as i64)
    ));
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
        let count = valid
            .iter()
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
    let max_idx = counts
        .iter()
        .enumerate()
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
            &labels,
            &values,
            "#42A5F5",
            Some(220_000),
            Some(215_000),
            Some(220_000),
            20_000,
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
            &labels,
            &values,
            "#42A5F5",
            Some(225_000),
            Some(230_000),
            Some(225_000),
            5_000,
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
        let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
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
            "北海道",
            "青森県",
            "岩手県",
            "宮城県",
            "秋田県",
            "山形県",
            "福島県",
            "茨城県",
            "栃木県",
            "群馬県",
            "埼玉県",
            "千葉県",
            "東京都",
            "神奈川県",
            "新潟県",
            "富山県",
            "石川県",
            "福井県",
            "山梨県",
            "長野県",
            "岐阜県",
            "静岡県",
            "愛知県",
            "三重県",
            "滋賀県",
            "京都府",
            "大阪府",
            "兵庫県",
            "奈良県",
            "和歌山県",
            "鳥取県",
            "島根県",
            "岡山県",
            "広島県",
            "山口県",
            "徳島県",
            "香川県",
            "愛媛県",
            "高知県",
            "福岡県",
            "佐賀県",
            "長崎県",
            "熊本県",
            "大分県",
            "宮崎県",
            "鹿児島県",
            "沖縄県",
        ];
        assert_eq!(prefectures.len(), 47, "都道府県リストは47件");
        for pref in &prefectures {
            let mw = min_wage_for_prefecture(pref);
            assert!(mw.is_some(), "最低賃金データが欠落: {}", pref);
            let val = mw.unwrap();
            assert!(
                (1000..=1300).contains(&val),
                "{} の最低賃金 {} が妥当範囲(1000-1300円)を逸脱",
                pref,
                val
            );
        }
    }

    /// 2026-04-24 ユーザー指摘により HW市場比較セクションは削除済み
    /// (任意スクレイピング件数 vs HW 全体の非同質比較は無意味)
    /// → hw_context の有無に関わらず <h2>HW市場比較</h2> が **出ないこと** を検証
    #[test]
    fn test_render_hw_market_comparison_section_removed() {
        let agg = SurveyAggregation::default();
        let seeker = JobSeekerAnalysis::default();

        let html_without = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
        assert!(
            !html_without.contains("<h2>HW市場比較</h2>"),
            "hw_context=None: HW市場比較は削除済"
        );

        let ctx = mock_empty_insight_ctx();
        let html_with =
            render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], Some(&ctx), &[]);
        assert!(
            !html_with.contains("<h2>HW市場比較</h2>"),
            "hw_context=Some でも HW市場比較は削除済（2026-04-24 ユーザー指摘）"
        );
    }

    /// テスト用: 空の InsightContext を生成
    fn mock_empty_insight_ctx() -> super::super::super::insight::fetch::InsightContext {
        use super::super::super::insight::fetch::InsightContext;
        InsightContext {
            vacancy: vec![],
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy: vec![],
            ts_salary: vec![],
            ts_fulfillment: vec![],
            ts_tracking: vec![],
            ext_job_ratio: vec![],
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover: vec![],
            ext_population: vec![],
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics: vec![],
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            // Phase A: SSDSE-A 新規6テーブル
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force: vec![],
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            // Phase A: 県平均
            pref_avg_unemployment_rate: None,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            // Phase B: Agoop 人流
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: "東京都".to_string(),
            muni: String::new(),
        }
    }

    /// パーセンタイル計算: 基本動作
    #[test]
    fn test_percentile_sorted_basic() {
        let sorted = vec![10.0, 20.0, 30.0, 40.0, 50.0, 60.0, 70.0, 80.0, 90.0, 100.0];
        assert_eq!(percentile_sorted(&sorted, 0.0), 10.0);
        assert_eq!(percentile_sorted(&sorted, 100.0), 100.0);
        let p50 = percentile_sorted(&sorted, 50.0);
        assert!(
            (p50 - 60.0).abs() < 20.0,
            "p50は中央付近のはず, got {}",
            p50
        );
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
        assert!(
            (0.0..=25.0).contains(&lo),
            "lo should be near data min, got {}",
            lo
        );
        assert!(
            (45.0..=60.0).contains(&hi),
            "hi should be near data max, got {}",
            hi
        );
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
            ScatterPoint {
                x: 200_000,
                y: 300_000,
            }, // OK
            ScatterPoint {
                x: 150_000,
                y: 250_000,
            }, // OK
            ScatterPoint {
                x: 10_000,
                y: 6_000_000,
            }, // NG: y=600万
            ScatterPoint {
                x: 5_000,
                y: 7_000_000,
            }, // NG: x<5万 かつ y=700万
            ScatterPoint {
                x: 300_000,
                y: 200_000,
            }, // NG: x>200万 かつ y<x
            ScatterPoint {
                x: 40_000,
                y: 50_000,
            }, // NG: x<5万
        ];
        let filtered: Vec<&ScatterPoint> = points
            .iter()
            .filter(|p| {
                let x_man = p.x as f64 / 10_000.0;
                let y_man = p.y as f64 / 10_000.0;
                (5.0..=200.0).contains(&x_man) && (5.0..=200.0).contains(&y_man) && y_man >= x_man
            })
            .collect();
        assert_eq!(
            filtered.len(),
            2,
            "5万〜200万の範囲内かつ y>=x の2点のみ残る"
        );
    }
}
