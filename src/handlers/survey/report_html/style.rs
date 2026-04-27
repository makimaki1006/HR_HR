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

/* ============================================================
   UI-2 強化（2026-04-26）: 物語のあるレポートのための共通スタイル
   ・既存クラスは変更しない（新規クラスのみ追加）
   ・UI-3 の style.rs 編集と競合しない
   ============================================================ */

/* 図表キャプション（図 1-1, 表 1-1 ... 形式） */
.figure-caption {
  font-size: 9.5pt;
  color: var(--c-text-muted);
  margin: 4px 0 8px;
  text-align: left;
  border-left: 3px solid var(--c-primary-light);
  padding: 2px 0 2px 8px;
  page-break-after: avoid;
  break-after: avoid;
}
.figure-caption .fig-no {
  font-weight: 700;
  color: var(--c-primary);
  margin-right: 6px;
  letter-spacing: 0.02em;
}

/* 読み方吹き出し（結論先取り） */
.read-hint {
  background: #f0f7ff;
  border: 1px solid #c7dcf8;
  border-left: 4px solid var(--c-primary-light);
  border-radius: 0 var(--radius) var(--radius) 0;
  padding: 6px 12px;
  margin: 6px 0 10px;
  font-size: 9.5pt;
  line-height: 1.55;
  color: var(--c-text);
  page-break-inside: avoid;
  break-inside: avoid;
}
.read-hint .read-hint-label {
  display: inline-block;
  font-weight: 700;
  color: var(--c-primary);
  margin-right: 6px;
  font-size: 9pt;
}
body.theme-dark .read-hint {
  background: #1f2a44;
  border-color: #37415a;
  color: #e6e6f0;
}

/* 「このページの読み方」ガイド */
.section-howto {
  background: var(--c-bg-card);
  border: 1px dashed var(--c-border);
  border-radius: var(--radius);
  padding: 8px 12px;
  margin: 8px 0 12px;
  font-size: 9.5pt;
  line-height: 1.55;
  page-break-inside: avoid;
  break-inside: avoid;
}
.section-howto .howto-title {
  font-weight: 700;
  color: var(--c-primary);
  font-size: 10pt;
  margin-bottom: 4px;
}
.section-howto ol {
  margin: 0;
  padding-left: 20px;
}
.section-howto li {
  margin: 2px 0;
}

/* 強化版 KPI カード（アイコン + 大きな数値 + 単位 + 比較値 + 状態） */
.exec-kpi-grid-v2 {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  margin: 10px 0 14px;
}
.kpi-card-v2 {
  background: var(--c-bg-card);
  border: 1px solid var(--c-border);
  border-radius: var(--radius);
  padding: 10px 14px;
  position: relative;
  page-break-inside: avoid;
  break-inside: avoid;
}
.kpi-card-v2::before {
  content: '';
  position: absolute;
  top: 0; left: 0;
  width: 4px;
  height: 100%;
  background: var(--c-primary);
  border-radius: var(--radius) 0 0 var(--radius);
}
.kpi-card-v2.kpi-good::before { background: var(--c-success); }
.kpi-card-v2.kpi-warn::before { background: var(--c-warning); }
.kpi-card-v2.kpi-crit::before { background: var(--c-danger); }
.kpi-card-v2 .kpi-head {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 10pt;
  color: var(--c-text-muted);
  margin-bottom: 4px;
}
.kpi-card-v2 .kpi-icon {
  font-size: 14pt;
  line-height: 1;
}
.kpi-card-v2 .kpi-status {
  margin-left: auto;
  font-size: 9pt;
  font-weight: 700;
}
.kpi-card-v2 .kpi-status.good { color: var(--c-success); }
.kpi-card-v2 .kpi-status.warn { color: var(--c-warning); }
.kpi-card-v2 .kpi-status.crit { color: var(--c-danger); }
.kpi-card-v2 .kpi-value-line {
  display: flex;
  align-items: baseline;
  gap: 6px;
  flex-wrap: wrap;
}
.kpi-card-v2 .kpi-value {
  font-size: 22pt;
  font-weight: 700;
  color: var(--c-primary);
  line-height: 1.1;
}
.kpi-card-v2 .kpi-unit {
  font-size: 11pt;
  color: var(--c-text-muted);
}
.kpi-card-v2 .kpi-compare {
  margin-top: 4px;
  font-size: 9pt;
  color: var(--c-text-muted);
}

/* 推奨アクションの優先度バッジ */
.priority-badge {
  display: inline-block;
  padding: 2px 8px;
  border-radius: 10px;
  font-size: 9pt;
  font-weight: 700;
  letter-spacing: 0.04em;
  margin-right: 6px;
}
.priority-badge.priority-now { background: #fee2e2; color: #b91c1c; border: 1px solid #fca5a5; }
.priority-badge.priority-week { background: #fef3c7; color: #92400e; border: 1px solid #fcd34d; }
.priority-badge.priority-later { background: #dcfce7; color: #166534; border: 1px solid #86efac; }
body.theme-dark .priority-badge.priority-now { background: #4a1414; color: #fca5a5; }
body.theme-dark .priority-badge.priority-week { background: #4a3614; color: #fcd34d; }
body.theme-dark .priority-badge.priority-later { background: #14401e; color: #86efac; }

/* テーブル zebra stripe（既存 tr:nth-child(even) を補強。新規クラス） */
.zebra tbody tr:nth-child(even) td { background: #f6f9fc; }
.zebra tbody tr:hover td { background: #fff7e6; }
body.theme-dark .zebra tbody tr:nth-child(even) td { background: #1d2440; }

/* IQR シェード補助バー（給与統計セクションで使用） */
.iqr-bar {
  position: relative;
  height: 18px;
  background: linear-gradient(90deg, #fee2e2 0%, #fef3c7 50%, #dcfce7 100%);
  border-radius: 9px;
  margin: 6px 0;
  overflow: hidden;
}
.iqr-bar .iqr-shade {
  position: absolute;
  top: 0; bottom: 0;
  background: rgba(59,130,246,0.30);
  border-left: 2px solid var(--c-primary);
  border-right: 2px solid var(--c-primary);
}
.iqr-bar .iqr-median {
  position: absolute;
  top: 0; bottom: 0; width: 2px;
  background: #16a34a;
}
.iqr-bar-legend {
  font-size: 8.5pt;
  color: var(--c-text-muted);
  display: flex;
  justify-content: space-between;
  margin-top: 2px;
}

/* Dumbbell chart（雇用形態別 同一地域 給与比較） */
.dumbbell-list {
  margin: 6px 0;
  padding: 0;
  list-style: none;
}
.dumbbell-row {
  display: grid;
  grid-template-columns: 88px 1fr 70px;
  gap: 8px;
  align-items: center;
  padding: 4px 6px;
  border-bottom: 1px dashed var(--c-border);
  font-size: 9.5pt;
}
.dumbbell-row:last-child { border-bottom: 0; }
.dumbbell-row .db-label { font-weight: 600; color: var(--c-text); }
.dumbbell-row .db-track {
  position: relative;
  height: 14px;
  background: #f1f5f9;
  border-radius: 7px;
}
.dumbbell-row .db-line {
  position: absolute;
  top: 6px; height: 2px;
  background: var(--c-primary-light);
}
.dumbbell-row .db-dot {
  position: absolute;
  top: 1px;
  width: 12px; height: 12px;
  border-radius: 50%;
  border: 2px solid #fff;
}
.dumbbell-row .db-dot.dot-ft { background: var(--c-primary); }
.dumbbell-row .db-dot.dot-pt { background: var(--c-warning); }
.dumbbell-row .db-diff { text-align: right; font-variant-numeric: tabular-nums; font-size: 9pt; color: var(--c-text-muted); }

/* 簡易ヒートマップ（都道府県別 件数） */
.heatmap-grid {
  display: grid;
  grid-template-columns: repeat(8, minmax(0, 1fr));
  gap: 2px;
  margin: 6px 0 4px;
}
.heatmap-cell {
  font-size: 8.5pt;
  text-align: center;
  padding: 4px 2px;
  border-radius: 3px;
  background: #eef2f7;
  color: #334155;
  border: 1px solid transparent;
}
.heatmap-cell.h-empty { background: #f8fafc; color: #cbd5e1; }
.heatmap-cell.h-1 { background: #dbeafe; color: #1e3a8a; }
.heatmap-cell.h-2 { background: #93c5fd; color: #0c1d52; }
.heatmap-cell.h-3 { background: #3b82f6; color: #fff; }
.heatmap-cell.h-4 { background: #1e40af; color: #fff; }
.heatmap-legend {
  display: flex;
  align-items: center;
  gap: 6px;
  font-size: 8.5pt;
  color: var(--c-text-muted);
  margin: 4px 0 8px;
}
.heatmap-legend .swatch {
  display: inline-block;
  width: 14px; height: 10px; border-radius: 2px;
}

/* 最低賃金差分バー */
.minwage-diff-bar {
  position: relative;
  height: 8px;
  background: #e2e8f0;
  border-radius: 4px;
  overflow: hidden;
}
.minwage-diff-bar .mwd-fill {
  position: absolute; top: 0; bottom: 0;
  background: var(--c-success);
}
.minwage-diff-bar .mwd-fill.below { background: var(--c-danger); }
.minwage-diff-bar .mwd-fill.near { background: var(--c-warning); }
.minwage-diff-bar .mwd-baseline {
  position: absolute; top: -2px; bottom: -2px; width: 1px;
  background: var(--c-text-muted);
}

/* セクションのつなぎ（次セクションへの橋渡し） */
.section-bridge {
  margin: 10px 0 0;
  padding: 6px 10px;
  font-size: 9.5pt;
  color: var(--c-text-muted);
  border-top: 1px dashed var(--c-border);
  font-style: italic;
}
.section-bridge::before {
  content: '\2192 ';
  color: var(--c-primary);
  font-weight: 700;
  font-style: normal;
  margin-right: 4px;
}

/* 全テーブルにzebra強制（既存tr:nth-child(even)を維持しつつ改善） */
@media print {
  .figure-caption, .read-hint, .section-howto,
  .kpi-card-v2, .priority-badge, .iqr-bar,
  .dumbbell-row, .heatmap-grid, .minwage-diff-bar,
  .section-bridge {
    page-break-inside: avoid;
    break-inside: avoid;
  }
  .read-hint { background: #f8fafc !important; }
  .section-howto { background: #f8fafc !important; }
}

/* =====================================================================
   UI-3 強化（2026-04-26）: 用語ツールチップ / 図表番号 / 凡例 /
   読み方吹き出し / 重要度バッジ / zebra / page-break / セクション区切り /
   注記カテゴリ別ボックス / Venn 概念図 / sparkline / 需給ギャップ色分け
   - 既存クラスは変更しない（新規クラスのみ追加）
   - UI-2 の追加分（figure-caption / read-hint 等）と機能重複しない
     名前空間 (`report-*`) を採用
   ===================================================================== */

/* 図表番号 (図 X-Y / 表 X-Y) */
.report-figure-num {
  font-weight: 700;
  color: var(--c-primary);
  font-size: 10pt;
  margin: 8px 0 4px;
  letter-spacing: 0.02em;
}
.report-figure-num.report-table-num { color: var(--c-primary-light); }

/* 用語ツールチップ */
.report-tooltip {
  display: inline-flex;
  align-items: baseline;
  gap: 2px;
  cursor: help;
  position: relative;
}
.report-tooltip abbr[title] {
  text-decoration: underline dotted var(--c-primary-light);
  text-underline-offset: 2px;
  cursor: help;
  border: none;
}
.report-tooltip abbr[title]:focus {
  outline: 2px solid var(--c-primary);
  outline-offset: 2px;
}
.report-tooltip-icon {
  font-size: 9pt;
  color: var(--c-primary-light);
  vertical-align: super;
  line-height: 1;
}

/* 凡例 (emoji + テキスト) */
.report-legend {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  font-size: 10pt;
  margin-right: 12px;
  white-space: nowrap;
}
.report-legend-emoji { font-size: 11pt; line-height: 1; }
.report-legend-text { color: var(--text); }

/* 読み方吹き出し */
.report-callout {
  background: #fff8e1;
  border-left: 4px solid var(--c-warning);
  border-radius: 4px;
  padding: 8px 12px;
  margin: 6px 0 10px;
  font-size: 10pt;
  line-height: 1.6;
  display: flex;
  align-items: flex-start;
  gap: 8px;
  page-break-inside: avoid;
  break-inside: avoid;
}
body.theme-dark .report-callout {
  background: #3a2f12;
  border-left-color: var(--c-warning);
  color: #ffe9b8;
}
.report-callout-icon { flex-shrink: 0; font-size: 13pt; line-height: 1.2; }
.report-callout-label {
  font-weight: 700;
  color: var(--c-warning);
  margin-right: 4px;
  flex-shrink: 0;
}
.report-callout-body { flex: 1; }

/* 重要度バッジ */
.report-severity-badge {
  display: inline-flex;
  align-items: center;
  gap: 4px;
  padding: 2px 8px;
  border-radius: 12px;
  font-size: 9pt;
  font-weight: 700;
  letter-spacing: 0.02em;
  vertical-align: middle;
}
.report-severity-emoji { font-size: 10pt; line-height: 1; }
.report-severity-text { white-space: nowrap; }
.report-sev-critical { background: #fee2e2; color: #991b1b; }
.report-sev-warning  { background: #fef3c7; color: #92400e; }
.report-sev-info     { background: #d1fae5; color: #065f46; }
body.theme-dark .report-sev-critical { background: #4b1d1d; color: #fecaca; }
body.theme-dark .report-sev-warning  { background: #4a3a1a; color: #fde68a; }
body.theme-dark .report-sev-info     { background: #14361f; color: #a7f3d0; }

/* zebra stripe + hover highlight */
.report-zebra tbody tr:nth-child(even) td { background: #f6f8fc; }
.report-zebra tbody tr:hover td { background: #e8f1ff; }
body.theme-dark .report-zebra tbody tr:nth-child(even) td { background: #20283d; }
body.theme-dark .report-zebra tbody tr:hover td { background: #2a3450; }

/* 改ページ回避 / セクション区切り */
.report-page-break-avoid { page-break-inside: avoid; break-inside: avoid; }
.report-section-divider {
  margin-top: 16px;
  padding: 6px 12px;
  border-left: 6px solid var(--c-primary);
  background: var(--c-bg-card);
  font-weight: 700;
  font-size: 12pt;
  color: var(--c-primary);
  letter-spacing: 0.04em;
}

/* amber バナー / gray バナー */
.report-banner-amber {
  background: #fff7ed;
  border: 1px solid #fed7aa;
  border-left: 5px solid var(--c-warning);
  border-radius: 4px;
  padding: 6px 12px;
  margin: 6px 0 10px;
  font-size: 10pt;
  line-height: 1.5;
  color: #7c2d12;
}
body.theme-dark .report-banner-amber {
  background: #382518;
  border-color: #6b3a1a;
  color: #ffd9b3;
}
.report-banner-gray {
  background: #f1f5f9;
  border: 1px solid #cbd5e1;
  border-left: 5px solid #64748b;
  border-radius: 4px;
  padding: 6px 12px;
  margin: 6px 0 10px;
  font-size: 10pt;
  line-height: 1.5;
  color: #334155;
}
body.theme-dark .report-banner-gray {
  background: #232946;
  border-color: #37415a;
  color: #cbd5e1;
}

/* sparkline コンテナ */
.report-sparkline {
  display: inline-block;
  width: 90px;
  height: 24px;
  vertical-align: middle;
}

/* CSV/HW 概念 Venn */
.report-venn {
  display: flex;
  justify-content: center;
  gap: 0;
  margin: 8px 0 12px;
  align-items: center;
  flex-wrap: wrap;
}
.report-venn-circle {
  width: 130px;
  height: 130px;
  border-radius: 50%;
  display: flex;
  flex-direction: column;
  align-items: center;
  justify-content: center;
  font-size: 10pt;
  text-align: center;
  opacity: 0.85;
  margin: 0 -18px;
  padding: 6px;
  page-break-inside: avoid;
  break-inside: avoid;
}
.report-venn-csv { background: rgba(59,130,246,0.45); color: #0a2954; }
.report-venn-hw  { background: rgba(245,158,11,0.45); color: #4a2a00; }
.report-venn-both { background: rgba(16,185,129,0.55); color: #053b29; font-weight: 700; }
.report-venn-label { font-weight: 700; font-size: 10pt; }
.report-venn-count { font-size: 14pt; font-weight: 700; }

/* 需給ギャップ色分け */
.report-gap-supply-shortage { background: #fee2e2; color: #991b1b; }
.report-gap-demand-shortage { background: #d1fae5; color: #065f46; }
.report-gap-balanced { background: #fef3c7; color: #713f12; }

/* 注記カテゴリ別ボックス */
.report-notes-category {
  margin: 10px 0;
  padding: 10px 12px;
  border-radius: 6px;
  page-break-inside: avoid;
  break-inside: avoid;
}
.report-notes-category h3 {
  margin: 0 0 6px;
  font-size: 11pt;
  display: flex;
  align-items: center;
  gap: 6px;
}
.report-notes-cat-data    { background: #eff6ff; border: 1px solid #bfdbfe; }
.report-notes-cat-scope   { background: #fff7ed; border: 1px solid #fed7aa; }
.report-notes-cat-method  { background: #f0fdf4; border: 1px solid #bbf7d0; }
.report-notes-cat-corr    { background: #fef3c7; border: 1px solid #fde68a; }
.report-notes-cat-update  { background: #f1f5f9; border: 1px solid #cbd5e1; }
body.theme-dark .report-notes-cat-data    { background: #14253d; border-color: #1e3a8a; }
body.theme-dark .report-notes-cat-scope   { background: #382518; border-color: #6b3a1a; }
body.theme-dark .report-notes-cat-method  { background: #14361f; border-color: #14532d; }
body.theme-dark .report-notes-cat-corr    { background: #3a2f12; border-color: #6b5020; }
body.theme-dark .report-notes-cat-update  { background: #232946; border-color: #37415a; }
.report-notes-category ul { margin: 4px 0 0 1.2em; padding: 0; font-size: 10pt; line-height: 1.55; }
.report-notes-category li { margin-bottom: 2px; }

/* notes 冒頭サマリ */
.report-notes-leadin {
  background: var(--c-bg-card);
  border-left: 4px solid var(--c-primary);
  padding: 8px 12px;
  margin: 6px 0 12px;
  font-size: 10pt;
  font-weight: 600;
}

/* 注目企業: ミニ bar */
.report-mini-bar {
  display: inline-block;
  height: 8px;
  background: var(--c-primary-light);
  border-radius: 2px;
  vertical-align: middle;
  margin-left: 4px;
}
.report-mini-bar-neg { background: var(--c-danger); }

@media print {
  .report-callout,
  .report-banner-amber,
  .report-banner-gray,
  .report-notes-category,
  .report-section-divider,
  .report-page-break-avoid {
    page-break-inside: avoid;
    break-inside: avoid;
  }
  /* 印刷時も用語の点線下線を維持 */
  .report-tooltip abbr[title] { text-decoration: underline dotted #666; }
  .report-tooltip-icon { color: #666; }
}
"#.to_string()
}
