//! 分割: report_html/style.rs (物理移動・内容変更なし)



pub(super) fn render_css() -> String {
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
