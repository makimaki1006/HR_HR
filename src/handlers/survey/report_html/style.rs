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
  /* 2026-05-01 v8: Working Paper 多色パレット (歯抜け感解消) */
  --wp-brand: #1E3A8A;
  --wp-brand-tint: #E5EAF2;
  --wp-accent-red: #DC2626;
  --wp-accent-red-tint: #FEE2E2;
  --wp-accent-orange: #EA580C;
  --wp-accent-orange-tint: #FFEDD5;
  --wp-accent-amber: #D97706;
  --wp-accent-green: #16A34A;
  --wp-accent-green-tint: #DCFCE7;
  --wp-accent-teal: #0D9488;
  --wp-accent-purple: #7C3AED;
  --wp-accent-yellow: #CA8A04;
  --wp-zebra: #F8FAFC;
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
  /* 2026-04-30: 12mm → 8mm に縮小して幅確保 (A4 = 210mm、左右 8mm で本文 194mm 確保) */
  margin: 10mm 8mm 12mm 8mm;
  @bottom-left {
    content: "株式会社For A-career | 求人市場 総合診断レポート";
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

/* バリアントインジケータ + 切替リンク (2026-04-29)
 * 画面表示のみ（印刷時は @media print の .no-print で非表示）
 * 現在の PDF 出力モード（HW併載版 / 公開データ中心版）を視覚化し、
 * 反対バリアントへワンクリックで切替できる導線を提供 */
.variant-indicator {
  margin: 0 16px 12px;
  padding: 10px 14px;
  background: linear-gradient(135deg, #eef2ff 0%, #f8fafc 100%);
  border: 1px solid #c7d2fe;
  border-left: 5px solid #4f46e5;
  border-radius: 8px;
  box-shadow: 0 1px 2px rgba(15,23,42,0.04);
}
.variant-indicator-inner {
  display: flex;
  flex-wrap: wrap;
  align-items: center;
  gap: 12px;
  font-size: 11pt;
  line-height: 1.5;
  color: #1e293b;
}
.variant-indicator .variant-icon {
  font-size: 14pt;
  margin-right: 6px;
  vertical-align: -2px;
}
.variant-indicator .variant-current {
  font-weight: 500;
}
.variant-indicator .variant-current strong {
  color: #4f46e5;
  font-size: 12pt;
  font-weight: 700;
  letter-spacing: 0.02em;
}
.variant-indicator .variant-desc {
  font-size: 9.5pt;
  color: #64748b;
  flex: 1 1 200px;
  min-width: 180px;
}
.variant-indicator .variant-switch-link {
  display: inline-flex;
  align-items: center;
  gap: 6px;
  margin-left: auto;
  padding: 8px 16px;
  min-height: 40px;
  background: #4f46e5;
  color: #ffffff;
  border-radius: 6px;
  font-size: 10.5pt;
  font-weight: 600;
  text-decoration: none;
  letter-spacing: 0.02em;
  transition: background 0.15s, transform 0.15s, box-shadow 0.15s;
  white-space: nowrap;
}
.variant-indicator .variant-switch-link:hover {
  background: #4338ca;
  transform: translateY(-1px);
  box-shadow: 0 4px 8px rgba(79,70,229,0.25);
}
.variant-indicator .variant-switch-link:focus {
  outline: 2px solid #4f46e5;
  outline-offset: 2px;
}
body.theme-dark .variant-indicator {
  background: linear-gradient(135deg, #1e293b 0%, #232946 100%);
  border-color: #4338ca;
  border-left-color: #818cf8;
  color: #e2e8f0;
}
body.theme-dark .variant-indicator-inner { color: #e2e8f0; }
body.theme-dark .variant-indicator .variant-current strong { color: #a5b4fc; }
body.theme-dark .variant-indicator .variant-desc { color: #94a3b8; }
body.theme-dark .variant-indicator .variant-switch-link { background: #6366f1; }
body.theme-dark .variant-indicator .variant-switch-link:hover { background: #4f46e5; }

/* スマホでの折返し対応 */
@media (max-width: 600px) {
  .variant-indicator-inner { gap: 8px; }
  .variant-indicator .variant-switch-link {
    margin-left: 0;
    width: 100%;
    justify-content: center;
  }
}

/* 印刷時は完全非表示 (.no-print と二重に保証) */
/* T5 (2026-04-30): theme-toggle / 各種 UI ボタン / fixed 要素を印刷時に非表示化 */
@media print {
  .variant-indicator { display: none !important; }
  .theme-toggle { display: none !important; }
  /* 画面操作系の固定要素は印刷時に出してはいけない (position:fixed は印刷で予期せぬ位置に出る) */
  .no-print,
  button.print-toggle,
  button[onclick*="print"],
  a.print-link,
  .floating-actions,
  .scroll-to-top { display: none !important; }
  /* fixed positioning は印刷では static に変換 (印刷上の位置ズレ防止) */
  .theme-toggle, .variant-indicator, .floating-actions {
    position: static !important;
  }

  /* ========================================================================
     2026-04-30 Phase 3-2: A4 縦印刷品質強化
     ヒント: A4 縦 1 ページ ≈ 縦 1100px。長大セクションのページ跨ぎ問題を解消。
     ======================================================================== */

  /* 表のヘッダーをページ跨ぎで再表示 (どのページでも何の数字か分かるように) */
  table thead { display: table-header-group; }
  table tfoot { display: table-footer-group; }
  table tr { page-break-inside: avoid; break-inside: avoid; }

  /* 見出し直後で改ページしない (孤立防止) */
  h2, h3, h4 {
    page-break-after: avoid;
    break-after: avoid-page;
  }

  /* セクション内部での予期せぬ改ページを抑制 (短いセクションは 1 ページに収める) */
  .section-compact, .figure-caption, .report-banner-gray, .caveat,
  .section-howto, .read-hint, .section-bridge, .data-source-note,
  .recruit-difficulty, .business-dynamics-card, .structural-summary {
    page-break-inside: avoid;
    break-inside: avoid;
  }

  /* 画像/チャート要素のページ跨ぎ抑制 */
  .echart, figure, img { page-break-inside: avoid; break-inside: avoid; }

  /* 主要章で改ページ開始 (.page-start クラスを既存セクションに付与済み) */
  .section.page-start, h2.page-start {
    page-break-before: always;
    break-before: page;
  }

  /* 6 マトリクスを A4 縦に合わせて 2 列に変更 (3 列だと 1 セル 210px で窮屈なため) */
  .size-x-trend-matrix {
    grid-template-columns: repeat(2, 1fr) !important;
  }

  /* exec-kpi-grid-v2: 印刷時は 3 列維持 (A4 縦 180mm ≒ 680px で 3 列 OK) */
  .exec-kpi-grid-v2 { grid-template-columns: repeat(3, 1fr) !important; }

  /* 段落・リストの widow/orphan 強化 (2026-04-30: page-break-inside avoid 撤去
   * 長いリスト/段落が単元途中で大きな空白を生む原因だったため)。
   * widows: 3 / orphans: 3 のみで「見出しと孤立」「末尾1行残し」を防ぐ。 */
  p, li {
    orphans: 3;
    widows: 3;
  }

  /* 印刷時の余白調整: ベタ塗り背景はインクをセーブ (visualization は維持) */
  body { -webkit-print-color-adjust: exact; print-color-adjust: exact; color-adjust: exact; }

  /* リンクの URL 表示は不要 (印刷物が見苦しくなる) */
  a[href]:after { content: none !important; }
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
  /* 2026-04-30: section 全体の page-break-inside: avoid を撤去。
   * 長いセクション (給与統計・地域企業ベンチマーク等) を 1 ページに収めようとして
   * 大きな白紙領域が生まれていた。代わりに内部の短いブロック単位で avoid 適用 (@media print 内)。 */
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
/* 2026-04-30 タスク T6: legacy KPI grid を Web でも非表示（Design v2 への完全移行）
 * 既存テストは要素存在を前提とするため DOM には残し、視覚・アクセシビリティの両面から除外。
 * インライン style と二重保証、かつ @media print は別途 Agent A 担当ブロックでカバー。 */
.exec-kpi-grid-legacy {
  display: none !important;
  position: absolute !important;
  width: 1px !important;
  height: 1px !important;
  overflow: hidden !important;
  clip: rect(0, 0, 0, 0) !important;
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
/* 2026-04-30: px → pt 統一 (frontend review #6/#7)。
 * print 時に px と pt が混在すると Chrome/Edge で実寸が変わるため pt に統一。 */
.stat-box .label { font-size: 9pt; color: var(--c-text-muted); }
.stat-box .value { font-size: 14pt; font-weight: bold; color: #333; }

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
  font-size: 9pt;
  color: #999;
  margin-top: 4px;
}

/* テーブル (2026-04-30: px → pt 統一) */
table {
  width: 100%;
  border-collapse: collapse;
  font-size: 9.5pt;
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
.sortable-table th::after { content: '↕'; position: absolute; right: 4px; top: 50%; transform: translateY(-50%); font-size: 9pt; color: #999; opacity: 0.5; }
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
  font-size: 9pt;
}
.guide-item .guide-title { font-weight: bold; color: var(--c-primary); font-size: 9pt; margin-bottom: 2px; }

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
  font-size: 9.5pt;
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
.comparison-card .value-pair .label { font-size: 9pt; color: var(--c-text-muted); }
.comparison-card .value-pair .value { font-size: 11pt; font-weight: bold; color: var(--c-primary); }
.comparison-card .diff { font-size: 9.5pt; margin-top: 4px; font-weight: bold; }
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
    /* 2026-04-30: !important + margin 0 を追加。
     * L72 の body padding 8px 16px と @page margin が二重に効いて
     * 本文幅が 170mm まで縮んでいた (frontend-review #10)。
     * @page margin 8mm のみで本文幅 194mm を確保する。 */
    padding: 0 !important;
    margin: 0 !important;
    background: #fff !important;
    color: #0f172a !important;
    font-size: 10.5pt;
  }
  /* 2026-04-30: details 強制展開 (frontend-review #8)。
   * Chromium Headless での PDF 化時、open 属性なしの <details> は
   * 折りたたまれたまま出力され、閾値早見表/データ範囲注記等の重要情報が
   * PDF に出ない問題があった。印刷時は summary を非表示にし内容を全表示。 */
  details { display: block !important; }
  details > summary { display: none !important; }
  details > *:not(summary) { display: block !important; }
  details.collapsible-guide { border: 1px dashed var(--c-border) !important; padding: 6px 10px !important; }
  body.theme-dark { background: #fff !important; color: #0f172a !important; }
  body.theme-dark table th { background: var(--c-primary) !important; color: #fff !important; }
  body.theme-dark table td { background: transparent !important; color: #0f172a !important; }

  /* セクション境界：主要セクションは次ページから (章単位)
   * 2026-04-30: .section 全体の page-break-inside: avoid を再撤去。
   * 単元コンセプトを維持しつつ、長いセクションは複数ページに自然に流れる。
   * 内部の図表・表・KPI カードは個別に avoid 適用 (下記)。 */
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

/* 強化版 KPI カード（アイコン + 大きな数値 + 単位 + 比較値 + 状態）
 * 2026-04-26 Readability: 6 KPI を 2x3 grid (2 行 3 列) で 1 ページに収まる構造へ。
 *   従来 3 列だと 6 枚で 2 行折り返し → 読み方ガイドと推奨アクションが次ページへ流れていた。
 *   2x3 を維持しつつ、印刷時のみ列幅を均一化して 1 ページ完結を担保。 */
.exec-kpi-grid-v2 {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 8px;
  margin: 8px 0 10px;
}
/* 2026-04-30 タスク T6: Design v2 KPI grid のレスポンシブ対応
 * mobile (<=640px): 1 列 / tablet (<=1024px): 2 列 / desktop: 3 列（デフォルト）
 * @media print は Agent A 担当ブロック（既存）で 3 列維持。 */
@media screen and (max-width: 1024px) {
  .exec-kpi-grid-v2 {
    grid-template-columns: repeat(2, 1fr);
  }
}
@media screen and (max-width: 640px) {
  .exec-kpi-grid-v2 {
    grid-template-columns: 1fr;
  }
}
@media print {
  .exec-kpi-grid-v2 {
    grid-template-columns: repeat(3, 1fr);
    gap: 6px;
    margin: 6px 0 8px;
  }
  .exec-kpi-grid-v2 .kpi-card-v2 { padding: 6px 10px; }
  .exec-kpi-grid-v2 .kpi-value { font-size: 18pt !important; }
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

/* =====================================================================
   Readability 強化（2026-04-26）: 見やすさ徹底改善
   ・PDF 15 ページ → 10-12 ページへの圧縮を狙う
   ・「情報を減らさず、見やすさを上げる」: 折りたたみ + 集約 + 視覚階層
   ・既存クラスは変更せず、補強クラスのみ追加
   ===================================================================== */

/* 1. 印刷時のフォント・余白調整: 単元コンセプトを見やすく収める
 * @page 宣言は L42 の単一定義に集約 (重複定義を撤去、cascade による意図せぬ上書きを防ぐ) */
@media print {
  body {
    font-size: 10pt !important;
    line-height: 1.5 !important;
  }
  /* dark theme は light に強制（color-scheme 上書き） */
  html { color-scheme: light !important; }
  /* 見出しの上下余白を圧縮 */
  h2 { font-size: 16pt !important; margin: 10px 0 6px !important; padding-bottom: 3px !important; }
  h3 { font-size: 12pt !important; margin: 8px 0 3px !important; }
  /* 注記/読み方ヒントは印刷時に圧縮 */
  .read-hint, .section-howto, .figure-caption,
  .report-banner-gray, .report-banner-amber {
    font-size: 8.5pt !important;
    line-height: 1.45 !important;
    padding: 4px 8px !important;
    margin: 4px 0 6px !important;
  }
  /* 注記類はフッター注記参照を促す compact 版に */
  .read-hint-compact, .section-howto-compact { font-size: 8.5pt !important; }
  /* テーブルはより詰めて表示 (2026-04-30: A4 縦幅 194mm 確保のため微調整) */
  table { font-size: 9pt !important; }
  th, td { padding: 2px 4px !important; line-height: 1.3 !important; }
  /* 表内の改行不可テキストは縮小 */
  .data-table th, .data-table td { font-size: 8.5pt !important; padding: 2px 3px !important; }
  /* report-zebra でも同様 */
  .report-zebra td, .report-zebra th { font-size: 9pt !important; padding: 2px 4px !important; }
}

/* Executive Summary 後の強制改ページ */

/* 2. Executive Summary は表紙の次の独立ページに配置するが、
 *    page-break-after: always は撤去 (短い場合に空白ページが発生していた)。
 *    後続 section に .page-start クラスがあれば自然に改ページされる。*/
.exec-summary {
  page-break-before: always;
  break-before: page;
}

/* 3. 重複 KPI カード（旧 5 KPI grid）の印刷非表示
 * テスト互換のため HTML 出力は維持しつつ、印刷では強化版 v2 のみを表示 */
@media print {
  .exec-kpi-grid-legacy { display: none !important; }
}

/* 4. 折りたたみ details 要素（読み方ガイドのコンパクト化） */
details.collapsible-guide {
  margin: 4px 0 8px;
  border: 1px dashed var(--c-border);
  border-radius: var(--radius);
  background: var(--c-bg-card);
  padding: 0;
  page-break-inside: avoid;
  break-inside: avoid;
}
details.collapsible-guide > summary {
  cursor: pointer;
  padding: 6px 12px;
  font-size: 9.5pt;
  font-weight: 700;
  color: var(--c-primary);
  list-style: none;
  user-select: none;
}
details.collapsible-guide > summary::-webkit-details-marker { display: none; }
details.collapsible-guide > summary::before {
  content: '\25B8'; /* ▸ */
  display: inline-block;
  margin-right: 6px;
  transition: transform 0.15s;
  color: var(--c-primary-light);
}
details.collapsible-guide[open] > summary::before {
  transform: rotate(90deg);
}
details.collapsible-guide > .details-body {
  padding: 4px 12px 8px 24px;
  font-size: 9.5pt;
  line-height: 1.55;
  color: var(--c-text);
}
@media print {
  /* 印刷時は details を強制展開し、本文を読めるようにする */
  details.collapsible-guide { border: none; background: transparent; padding: 0; margin: 2px 0 4px; }
  details.collapsible-guide > summary { display: none; }
  details.collapsible-guide > .details-body {
    padding: 0 !important;
    font-size: 8.5pt !important;
    color: var(--c-text-muted) !important;
    line-height: 1.4 !important;
  }
  details.collapsible-guide > .details-body::before {
    content: '\203B '; /* ※ */
    color: #999;
  }
}

/* 5. コンパクト版「※ 詳細は注記参照」リンク */
.notes-pointer {
  font-size: 8.5pt;
  color: var(--c-text-muted);
  margin: 4px 0 6px;
  font-style: italic;
}
.notes-pointer::before { content: '\203B '; color: var(--c-warning); margin-right: 2px; }

/* 6. 視覚階層強化: 主要 KPI 数値はより大きく、注記はより小さく */
.kpi-card-v2 .kpi-value {
  font-size: 24pt; /* 22pt → 24pt */
  letter-spacing: -0.01em;
}
.kpi-card-v2 .kpi-compare {
  font-size: 8.5pt; /* 9pt → 8.5pt（注記を本文より明確に小さく） */
  color: var(--c-text-muted);
  line-height: 1.4;
}

/* 7. テーブル zebra stripe 強化（既存は薄すぎ → コントラスト強化） */
table tr:nth-child(even) td { background: #f3f6fb; } /* #fafafa → #f3f6fb (12% → ~15%) */
.zebra tbody tr:nth-child(even) td { background: #eef3fa; }
.report-zebra tbody tr:nth-child(even) td { background: #eef3fa; }
@media print {
  /* 印刷時はインクが薄いプリンタでも識別できる強度に */
  table tr:nth-child(even) td { background: #eef3fa !important; }
  .zebra tbody tr:nth-child(even) td { background: #eef3fa !important; }
  .report-zebra tbody tr:nth-child(even) td { background: #eef3fa !important; }
  /* テーブルヘッダはページ跨ぎで再表示 */
  thead { display: table-header-group; }
}

/* 8. テーブル行の改ページ回避（リスト系のみ。長大なテーブルは行内 break 許容） */
table tr { page-break-inside: avoid; break-inside: avoid; }

/* 9. 章番号の統一（h2 内の「第 N 章」プレフィックス強調） */
h2 .chapter-num {
  display: inline-block;
  font-size: 0.85em;
  color: var(--c-text-muted);
  font-weight: 600;
  margin-right: 8px;
  letter-spacing: 0.05em;
}

/* 10. 2 列レイアウト（タグ別給与表など縦長を 2 列に） */
.two-col-list {
  display: grid;
  grid-template-columns: 1fr 1fr;
  gap: 4px 12px;
  margin: 6px 0;
}
.two-col-list > * { font-size: 9.5pt; }
@media print {
  .two-col-list { gap: 2px 10px; }
  .two-col-list > * { font-size: 8.5pt; }
}

/* 11. 図表とキャプションを必ず一緒に保つ */
.figure-with-caption {
  page-break-inside: avoid;
  break-inside: avoid;
  margin-bottom: 8px;
}
.figure-caption { page-break-after: avoid; break-after: avoid; }

/* 12. 強調 KPI 数値 (P2 で「中央値」など主要値を最大強調) */
.kpi-emphasized .kpi-value {
  font-size: 28pt;
  color: var(--c-primary);
}
@media print {
  .kpi-emphasized .kpi-value { font-size: 22pt !important; }
}

/* 13. 印刷時 hover 効果は無効化（不要 transform / shadow を削除） */
@media print {
  .summary-card, .kpi-card, .kpi-card-v2,
  .summary-card:hover, .kpi-card:hover {
    transform: none !important;
    box-shadow: none !important;
    transition: none !important;
  }
}

/* 14. セクション最低限のページ余白 */
.section + .section { margin-top: 14px; }
@media print {
  .section + .section { margin-top: 8px; }
}

/* 15. read-hint と section-howto を印刷時に脚注的にコンパクト表示 */
@media print {
  /* 「📖 読み方」プレフィックスは印刷時には「※」に変換 */
  .read-hint .read-hint-label {
    font-size: 7.5pt !important;
    color: #999 !important;
  }
  .section-howto .howto-title {
    font-size: 8pt !important;
  }
}

/* =====================================================================
   Design v2 刷新（2026-04-26）: コンサル提案資料品質のプロフェッショナル版
   ・既存クラスは一切変更しない（dv2-* / --dv2-* 名前空間で完全分離）
   ・印刷時 (PDF/HTML download) に design-v2 を主役として有効化
   ・タブ UI（画面表示）でも一部の dv2-* を上書き的に重ねて適用可能
   ・色 + タイポ + 余白 + ビジュアル要素 の 4 軸で刷新
   ===================================================================== */

:root {
  /* dv2 カラーパレット: light theme 中心、severity ベースのアクセント */
  --dv2-bg: #ffffff;
  --dv2-bg-card: #f8fafc;
  --dv2-bg-subtle: #f1f5f9;
  --dv2-border: #e2e8f0;
  --dv2-border-strong: #cbd5e1;
  --dv2-text: #1e293b;
  --dv2-text-muted: #64748b;
  --dv2-text-faint: #94a3b8;
  --dv2-accent: #4f46e5;       /* indigo-600: primary accent */
  --dv2-accent-light: #6366f1; /* indigo-500 */
  --dv2-accent-soft: #eef2ff;  /* indigo-50: 背景強調 */
  --dv2-good: #10b981;         /* emerald-500 */
  --dv2-warn: #f59e0b;         /* amber-500 */
  --dv2-crit: #ef4444;         /* red-500 */
  --dv2-navy: #1e293b;         /* slate-800: 見出し色 */
  --dv2-shadow-sm: 0 1px 2px rgba(15,23,42,0.04);
  --dv2-shadow-md: 0 2px 8px rgba(15,23,42,0.06);
  --dv2-radius-sm: 4px;
  --dv2-radius-md: 8px;
  --dv2-radius-lg: 12px;
  /* タイポグラフィ 4 階層 */
  --dv2-fs-display: 32pt;
  --dv2-fs-display-lg: 40pt;
  --dv2-fs-heading: 18pt;
  --dv2-fs-heading-lg: 24pt;
  --dv2-fs-body: 11pt;
  --dv2-fs-body-sm: 10.5pt;
  --dv2-fs-caption: 9pt;
  --dv2-fs-caption-sm: 8.5pt;
  /* 等幅数字（KPI 整列用） */
  --dv2-num-feature: tabular-nums;
}

/* dv2 表紙刷新（3 段構成: タイトル / 対象 / ハイライト KPI） */
.dv2-cover {
  page-break-after: always;
  break-after: page;
  position: relative;
  min-height: 250mm;
  padding: 24mm 16mm 18mm;
  display: grid;
  grid-template-rows: auto 1fr auto;
  background:
    linear-gradient(180deg, var(--dv2-bg) 0%, var(--dv2-accent-soft) 100%);
  border-radius: var(--dv2-radius-lg);
  margin-bottom: 16px;
}
.dv2-cover-header {
  display: flex;
  align-items: center;
  justify-content: space-between;
  padding-bottom: 12px;
  border-bottom: 2px solid var(--dv2-accent);
}
.dv2-cover-brand {
  font-size: 12pt;
  font-weight: 700;
  color: var(--dv2-navy);
  letter-spacing: 0.04em;
  white-space: nowrap;
}
.dv2-cover-meta {
  font-size: 10pt;
  color: var(--dv2-text-muted);
  font-variant-numeric: tabular-nums;
}
.dv2-cover-main {
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: flex-start;
  gap: 10mm;
  padding: 12mm 0;
}
.dv2-cover-title {
  font-size: var(--dv2-fs-display-lg);
  font-weight: 700;
  color: var(--dv2-navy);
  line-height: 1.15;
  letter-spacing: 0.01em;
  margin: 0;
}
.dv2-cover-title-accent {
  display: inline-block;
  width: 64px;
  height: 4px;
  background: var(--dv2-accent);
  margin-bottom: 12px;
  border-radius: 2px;
}
.dv2-cover-subtitle {
  font-size: 14pt;
  color: var(--dv2-text-muted);
  font-weight: 400;
  margin: 0;
}
.dv2-cover-target {
  font-size: 16pt;
  color: var(--dv2-navy);
  font-weight: 600;
  padding: 8px 16px;
  background: rgba(255,255,255,0.7);
  border-left: 4px solid var(--dv2-accent);
  border-radius: 0 var(--dv2-radius-sm) var(--dv2-radius-sm) 0;
}
.dv2-cover-highlights {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  padding-top: 12mm;
}
.dv2-cover-hl {
  background: rgba(255,255,255,0.85);
  border: 1px solid var(--dv2-border);
  border-radius: var(--dv2-radius-md);
  padding: 12px 14px;
  text-align: left;
}
.dv2-cover-hl-label {
  font-size: var(--dv2-fs-caption);
  color: var(--dv2-text-muted);
  letter-spacing: 0.04em;
  margin-bottom: 4px;
  text-transform: uppercase;
}
.dv2-cover-hl-value {
  font-size: 18pt;
  font-weight: 700;
  color: var(--dv2-accent);
  line-height: 1.2;
  font-variant-numeric: tabular-nums;
}
.dv2-cover-hl-unit {
  font-size: 10pt;
  color: var(--dv2-text-muted);
  margin-left: 4px;
}
.dv2-cover-footer {
  display: flex;
  justify-content: space-between;
  align-items: flex-end;
  padding-top: 10px;
  border-top: 1px solid var(--dv2-border);
  font-size: 9pt;
  color: var(--dv2-text-muted);
}

/* dv2 Section 番号バッジ */
.dv2-section-badge {
  display: inline-flex;
  align-items: center;
  justify-content: center;
  width: 32px;
  height: 32px;
  background: var(--dv2-accent);
  color: #fff;
  font-size: 12pt;
  font-weight: 700;
  border-radius: var(--dv2-radius-sm);
  margin-right: 12px;
  vertical-align: middle;
  letter-spacing: 0;
  font-variant-numeric: tabular-nums;
  box-shadow: var(--dv2-shadow-sm);
}
.dv2-section-heading {
  display: flex;
  align-items: center;
  gap: 0;
  margin: 18px 0 10px;
  padding-left: 0;
  border-left: 4px solid var(--dv2-accent);
  padding: 6px 0 6px 12px;
  page-break-after: avoid;
  break-after: avoid;
}
.dv2-section-heading-title {
  font-size: var(--dv2-fs-heading);
  font-weight: 700;
  color: var(--dv2-navy);
  letter-spacing: 0.02em;
  line-height: 1.3;
}

/* dv2 KPI カード（modern: 左ラベル / 右値 / 下比較） */
.dv2-kpi-grid {
  display: grid;
  grid-template-columns: repeat(3, 1fr);
  gap: 10px;
  margin: 8px 0 12px;
}
.dv2-kpi-grid.dv2-kpi-2x3 {
  grid-template-columns: repeat(3, 1fr);
}
.dv2-kpi-card {
  background: var(--dv2-bg-card);
  border: 1px solid var(--dv2-border);
  border-radius: var(--dv2-radius-md);
  padding: 12px 14px;
  position: relative;
  page-break-inside: avoid;
  break-inside: avoid;
  display: flex;
  flex-direction: column;
  gap: 4px;
}
.dv2-kpi-card.dv2-kpi-large {
  grid-column: span 2;
  background: linear-gradient(135deg, var(--dv2-accent-soft) 0%, var(--dv2-bg-card) 100%);
  border-color: var(--dv2-accent-light);
}
.dv2-kpi-card-label {
  font-size: var(--dv2-fs-caption);
  color: var(--dv2-text-muted);
  letter-spacing: 0.04em;
  text-transform: uppercase;
}
.dv2-kpi-card-value {
  font-size: 26pt;
  font-weight: 700;
  color: var(--dv2-navy);
  line-height: 1.1;
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.01em;
}
.dv2-kpi-card.dv2-kpi-large .dv2-kpi-card-value {
  font-size: 32pt;
  color: var(--dv2-accent);
}
.dv2-kpi-card-unit {
  font-size: 11pt;
  color: var(--dv2-text-muted);
  margin-left: 4px;
  font-weight: 400;
}
.dv2-kpi-card-compare {
  font-size: var(--dv2-fs-caption-sm);
  color: var(--dv2-text-muted);
  line-height: 1.4;
}
.dv2-kpi-card[data-status="good"] { border-left: 4px solid var(--dv2-good); }
.dv2-kpi-card[data-status="warn"] { border-left: 4px solid var(--dv2-warn); }
.dv2-kpi-card[data-status="crit"] { border-left: 4px solid var(--dv2-crit); }

/* dv2 データバー（テーブル内の数値の隣に視覚的バー） */
.dv2-databar {
  position: relative;
  display: inline-block;
  width: 60px;
  height: 8px;
  background: var(--dv2-bg-subtle);
  border-radius: 2px;
  vertical-align: middle;
  margin-left: 6px;
  overflow: hidden;
}
.dv2-databar-fill {
  position: absolute;
  top: 0; bottom: 0; left: 0;
  background: var(--dv2-accent);
  border-radius: 2px;
}
.dv2-databar[data-tone="good"] .dv2-databar-fill { background: var(--dv2-good); }
.dv2-databar[data-tone="warn"] .dv2-databar-fill { background: var(--dv2-warn); }
.dv2-databar[data-tone="crit"] .dv2-databar-fill { background: var(--dv2-crit); }

/* dv2 進捗バー（充足度 / パーセンタイル） */
.dv2-progress {
  display: flex;
  align-items: center;
  gap: 8px;
  margin: 4px 0;
}
.dv2-progress-track {
  flex: 1;
  height: 8px;
  background: var(--dv2-bg-subtle);
  border-radius: 4px;
  overflow: hidden;
}
.dv2-progress-fill {
  height: 100%;
  background: linear-gradient(90deg, var(--dv2-accent-light) 0%, var(--dv2-accent) 100%);
  transition: width 0.3s;
}
.dv2-progress-label {
  font-size: var(--dv2-fs-caption);
  color: var(--dv2-text-muted);
  font-variant-numeric: tabular-nums;
  white-space: nowrap;
  min-width: 40px;
  text-align: right;
}

/* dv2 SVG inline icon (✓ checkmark, ⚠ warning) のサイズ統一 */
.dv2-icon {
  display: inline-block;
  width: 1em;
  height: 1em;
  vertical-align: -0.125em;
  fill: currentColor;
}
.dv2-icon-check { color: var(--dv2-good); }
.dv2-icon-warn { color: var(--dv2-warn); }
.dv2-icon-crit { color: var(--dv2-crit); }
.dv2-icon-info { color: var(--dv2-accent); }

/* dv2 トレンド矢印 (上↑ / 横→ / 下↓) */
.dv2-trend {
  display: inline-flex;
  align-items: center;
  gap: 2px;
  font-size: var(--dv2-fs-caption);
  font-variant-numeric: tabular-nums;
  font-weight: 600;
}
.dv2-trend-up { color: var(--dv2-good); }
.dv2-trend-down { color: var(--dv2-crit); }
.dv2-trend-flat { color: var(--dv2-text-muted); }

/* dv2 アクションカード（recommended actions） */
.dv2-action-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
  margin: 8px 0;
}
.dv2-action-card {
  display: grid;
  grid-template-columns: 80px 1fr;
  gap: 10px;
  align-items: start;
  padding: 10px 14px;
  background: var(--dv2-bg);
  border: 1px solid var(--dv2-border);
  border-radius: var(--dv2-radius-md);
  page-break-inside: avoid;
  break-inside: avoid;
}
.dv2-action-card[data-priority="now"] { border-left: 4px solid var(--dv2-crit); }
.dv2-action-card[data-priority="week"] { border-left: 4px solid var(--dv2-warn); }
.dv2-action-card[data-priority="later"] { border-left: 4px solid var(--dv2-good); }
.dv2-action-priority {
  font-size: var(--dv2-fs-caption);
  font-weight: 700;
  letter-spacing: 0.04em;
  color: var(--dv2-text-muted);
  text-align: center;
  padding: 4px 0;
}
.dv2-action-body {
  font-size: var(--dv2-fs-body-sm);
  line-height: 1.5;
  color: var(--dv2-text);
}

/* dv2 印刷時専用: design-v2 を主役に切替 */
@media print {
  body {
    /* dv2 タイポグラフィを印刷時の標準に */
    font-family: "Noto Sans JP", "Hiragino Kaku Gothic ProN", "Meiryo", sans-serif !important;
    color: var(--dv2-text) !important;
    background: var(--dv2-bg) !important;
  }
  /* 2026-04-30: dv2 専用 @page 重複定義を撤去
   * 余白は L42 の単一 @page 宣言 (margin: 10mm 8mm 12mm 8mm) を採用。
   * フッター文言も L46-55 で定義済みのため重複削除。
   * CSS cascade による意図せぬ上書きを防ぎ、本文幅 194mm を確保する。 */
  /* dv2 カードは hover 効果無効 */
  .dv2-kpi-card,
  .dv2-action-card,
  .dv2-cover-hl {
    box-shadow: none !important;
    transform: none !important;
    transition: none !important;
  }
  /* dv2 表紙: 印刷時は背景グラデーションを薄く */
  .dv2-cover {
    background: linear-gradient(180deg, #ffffff 0%, #f8fafc 100%) !important;
    border-radius: 0 !important;
  }
  /* Section 番号バッジは印刷時も色を維持 */
  .dv2-section-badge {
    background: var(--dv2-accent) !important;
    color: #fff !important;
    -webkit-print-color-adjust: exact;
    print-color-adjust: exact;
  }
  /* dv2 印刷時のフォントサイズ微調整 */
  .dv2-cover-title { font-size: 32pt !important; }
  .dv2-cover-target { font-size: 14pt !important; }
  .dv2-cover-hl-value { font-size: 16pt !important; }
  .dv2-section-heading-title { font-size: 16pt !important; }
  .dv2-kpi-card-value { font-size: 22pt !important; }
  .dv2-kpi-card.dv2-kpi-large .dv2-kpi-card-value { font-size: 28pt !important; }
}

/* dv2 アクセシビリティ: フォーカス表示 */
.dv2-kpi-card:focus-within,
.dv2-action-card:focus-within {
  outline: 2px solid var(--dv2-accent);
  outline-offset: 2px;
}

/* dv2 タブ UI 用（画面表示） KPI 強化 */
body.theme-dark .dv2-kpi-card {
  background: #1e293b;
  border-color: #334155;
  color: #e2e8f0;
}
body.theme-dark .dv2-kpi-card-value {
  color: #f1f5f9;
}
body.theme-dark .dv2-kpi-card-label,
body.theme-dark .dv2-kpi-card-compare {
  color: #94a3b8;
}
body.theme-dark .dv2-section-heading-title {
  color: #e2e8f0;
}
body.theme-dark .dv2-cover {
  background: linear-gradient(180deg, #0f172a 0%, #1e293b 100%);
}
body.theme-dark .dv2-cover-title,
body.theme-dark .dv2-cover-target {
  color: #f1f5f9;
}

/* =====================================================================
 * 2026-05-01 v8: Working Paper 視覚補強 (歯抜け感解消、多色化)
 * Phase 1 監査結果反映 (PDF 32 ページ視覚監査 + 実コード整合性監査)
 * 既存クラスを上書きせず、新規補強を追加。
 * ===================================================================== */

/* 多色 severity badge (緑/橙/赤の三段識別、色覚配慮で記号併用) */
.wp-sev-good { color: var(--wp-accent-green); font-weight: 700; }
.wp-sev-warn { color: var(--wp-accent-orange); font-weight: 700; }
.wp-sev-crit { color: var(--wp-accent-red); font-weight: 700; }
.wp-sev-good::before { content: "◯ "; }
.wp-sev-warn::before { content: "△ "; }
.wp-sev-crit::before { content: "× "; }

/* テーブル zebra strip + ブランド色ヘッダ (補強クラス、既存 .data-table と併用可能) */
table.data-table.wp-colorful { border-top: 2pt solid var(--wp-brand); }
table.data-table.wp-colorful thead tr {
  background: var(--wp-brand);
  color: #FFFFFF;
}
table.data-table.wp-colorful thead th {
  color: #FFFFFF !important;
  background: var(--wp-brand) !important;
  font-weight: 700;
  letter-spacing: 0.06em;
}
table.data-table.wp-colorful tbody tr:nth-child(even) {
  background: var(--wp-zebra);
}
table.data-table.wp-colorful tr.wp-self-row {
  background: linear-gradient(90deg, var(--wp-accent-yellow) 0%, var(--wp-accent-yellow) 1mm, var(--wp-brand-tint) 1mm) !important;
}
table.data-table.wp-colorful tr.wp-self-row td:first-child {
  font-weight: 700; color: var(--wp-brand);
}

/* 章バナー (青→青緑グラデ + 黄下罫) — オプトイン、既存 h2 とは独立クラス */
.wp-chapter-band {
  background: linear-gradient(90deg, var(--wp-brand) 0%, var(--wp-brand) 70%, var(--wp-accent-teal) 100%);
  color: #FFFFFF;
  padding: 4mm 6mm;
  display: grid;
  grid-template-columns: auto 1fr auto;
  gap: 6mm;
  align-items: baseline;
  margin: 5mm 0;
  border-bottom: 2pt solid var(--wp-accent-yellow);
  break-before: page;
  break-after: avoid;
}
.wp-chapter-band .ch-num {
  font-family: "JetBrains Mono", "IBM Plex Mono", monospace;
  font-size: 22pt; font-weight: 700; line-height: 1;
  color: var(--wp-accent-yellow);
}
.wp-chapter-band .ch-title { font-size: 18pt; font-weight: 700; line-height: 1.1; }
.wp-chapter-band .ch-meta {
  font-family: "JetBrains Mono", monospace; font-size: 9pt;
  color: rgba(255,255,255,0.85); letter-spacing: 0.06em; text-transform: uppercase;
}

/* リード段落 (章冒頭の要約段落、青グラデ背景 + 黄右罫) */
.wp-lead {
  font-size: 11pt; line-height: 1.6;
  margin: 0 0 4mm;
  border-left: 4pt solid var(--wp-brand);
  border-right: 1pt solid var(--wp-accent-yellow);
  padding: 2mm 4mm 2mm 5mm;
  background: linear-gradient(90deg, var(--wp-brand-tint) 0%, transparent 100%);
}
.wp-lead strong { color: var(--wp-brand); font-weight: 700; }

/* 6 マトリクス (拡大=緑系 / 縮小=赤系 / 各セル Top 5 企業名) */
.wp-matrix6-cell.wp-mat-growth {
  background: linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%) !important;
  border-left: 3pt solid var(--wp-accent-green) !important;
}
.wp-matrix6-cell.wp-mat-decline {
  background: linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%) !important;
  border-left: 3pt solid var(--wp-accent-red) !important;
}
.wp-matrix6-cell .wp-mat-count-pill {
  display: inline-block; padding: 0.5mm 2.5mm; border-radius: 8pt;
  color: #FFFFFF; font-family: "JetBrains Mono", monospace;
  font-size: 8.5pt; font-weight: 700;
}
.wp-matrix6-cell.wp-mat-growth .wp-mat-count-pill { background: var(--wp-accent-green); }
.wp-matrix6-cell.wp-mat-decline .wp-mat-count-pill { background: var(--wp-accent-red); }
.wp-matrix6-cell .wp-mat-top5 {
  margin: 2mm 0 0; padding-left: 5mm;
  font-size: 9.5pt; line-height: 1.55;
}
.wp-matrix6-cell .wp-mat-top5 li { margin-bottom: 1.5mm; }
.wp-matrix6-cell .wp-mat-top5 .wp-mat-name {
  font-weight: 700; color: var(--c-text); font-size: 9.5pt;
}
.wp-matrix6-cell .wp-mat-top5 .wp-mat-meta {
  display: block; font-family: "JetBrains Mono", monospace;
  font-size: 8.5pt; color: var(--c-text-muted); margin-top: 0.2mm;
}

/* Findings (推奨アクション) を赤罫で強調 */
.wp-findings {
  border-top: 3pt solid var(--wp-accent-red);
  padding: 3mm 0 0; margin: 5mm 0;
  background: linear-gradient(180deg, var(--wp-accent-red-tint) 0%, transparent 5mm);
}
.wp-findings-title {
  font-weight: 700; font-size: 10pt;
  letter-spacing: 0.14em; text-transform: uppercase;
  color: var(--wp-accent-red); margin: 0 0 3mm;
}

/* Observations (Key Takeaways) を黄罫で強調 */
.wp-observations {
  border-top: 2pt solid var(--wp-accent-yellow);
  border-bottom: 1pt solid var(--c-text);
  background: linear-gradient(180deg, #FFFBEB 0%, transparent 50%);
  padding: 3mm 4mm; margin: 5mm 0 0;
  break-inside: avoid;
}
.wp-observations h4 {
  font-weight: 700; font-size: 9.5pt;
  letter-spacing: 0.16em; text-transform: uppercase;
  color: var(--wp-accent-amber); margin: 0 0 2mm;
}

/* 印刷時 multi-color preserve */
@media print {
  .wp-sev-good, .wp-sev-warn, .wp-sev-crit,
  .wp-chapter-band, .wp-lead, .wp-matrix6-cell,
  .wp-findings, .wp-observations,
  table.data-table.wp-colorful thead tr,
  table.data-table.wp-colorful tr.wp-self-row {
    -webkit-print-color-adjust: exact !important;
    print-color-adjust: exact !important;
  }
}
"#.to_string()
}

/// V8 Working Paper テーマ (2026-05-01)
///
/// 既存マークアップ (`.section`, `.exec-kpi-grid-v2`, `table`, etc.) に対して
/// `[data-theme="v8"]` 属性セレクタで上書き形式で適用する。マークアップ変更なし。
///
/// 影響範囲: フォント (BIZ UDPGothic + JetBrains Mono) / 勝色基調 /
/// severity 多色アクセント / 章境界黒帯 / 印刷時 @page ヘッダフッタ。
pub(super) fn render_theme_v8_workingpaper() -> String {
    r#"
/* ============================================================
   V8 Working Paper Theme (2026-05-01)
   既存マークアップに data-theme="v8" 属性で上書き適用。
   ============================================================ */

[data-theme="v8"] {
  --wp-ink: #131313;
  --wp-paper: #FFFFFF;
  --wp-rule: #131313;
  --wp-rule-light: #C8C8C8;
  --wp-brand: #1E3A8A;
  --wp-brand-tint: #E5EAF2;
  --wp-muted: #555555;
  --wp-accent-red: #DC2626;
  --wp-accent-red-tint: #FEE2E2;
  --wp-accent-orange: #EA580C;
  --wp-accent-orange-tint: #FFEDD5;
  --wp-accent-amber: #D97706;
  --wp-accent-green: #16A34A;
  --wp-accent-green-tint: #DCFCE7;
  --wp-accent-yellow: #CA8A04;
  --wp-zebra: #F8FAFC;
  --wp-sans: 'BIZ UDPGothic', 'Hiragino Sans', 'Noto Sans CJK JP', sans-serif;
  --wp-mono: 'JetBrains Mono', 'Consolas', 'Roboto Mono', monospace;
}

[data-theme="v8"] body {
  background: var(--wp-paper);
  color: var(--wp-ink);
  font-family: var(--wp-sans);
  font-size: 11pt;
  line-height: 1.55;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}

/* 見出し: 勝色 + 黄色アクセント下罫 */
[data-theme="v8"] h1 {
  font-size: 24pt; font-weight: 700;
  border-bottom: 2pt solid var(--wp-ink);
  color: var(--wp-ink); letter-spacing: -0.01em;
  padding-bottom: 2mm; margin: 4mm 0 6mm;
}
[data-theme="v8"] h2 {
  font-size: 16pt; font-weight: 700;
  border-top: 1pt solid var(--wp-ink);
  border-bottom: 0.5pt solid var(--wp-accent-yellow);
  color: var(--wp-ink); letter-spacing: -0.01em;
  padding: 2mm 0 1mm; margin: 8mm 0 3mm;
  break-after: avoid;
}
[data-theme="v8"] h3 {
  font-size: 11pt; font-weight: 700;
  letter-spacing: 0.14em; text-transform: uppercase;
  color: var(--wp-brand);
  margin: 4mm 0 2mm;
}
[data-theme="v8"] h3::before {
  content: "▌"; color: var(--wp-accent-orange);
  margin-right: 2mm; font-size: 13pt; vertical-align: -1pt;
}

/* セクション境界の黒帯 (章バナー代替) */
[data-theme="v8"] .section,
[data-theme="v8"] section {
  border-top: 6mm solid var(--wp-brand);
  margin-top: 8mm; padding-top: 4mm;
}
[data-theme="v8"] .section:first-child,
[data-theme="v8"] section:first-of-type {
  border-top: none; padding-top: 0;
}

/* リード文・要旨ブロック */
[data-theme="v8"] .lead,
[data-theme="v8"] .section-header-meta,
[data-theme="v8"] .exec-story-guide,
[data-theme="v8"] .collapsible-guide > .details-body {
  border-left: 4pt solid var(--wp-brand);
  border-right: 1pt solid var(--wp-accent-yellow);
  background: linear-gradient(90deg, var(--wp-brand-tint) 0%, transparent 100%);
  padding: 2mm 4mm 2mm 5mm;
  font-size: 11pt; line-height: 1.6;
}

/* 強調文字: 勝色マーカー */
[data-theme="v8"] p strong {
  font-weight: 700;
  background: linear-gradient(transparent 60%, var(--wp-brand-tint) 60%);
  padding: 0 1px;
  color: var(--wp-ink);
}

/* KPI: severity 罫太さ + 背景グラデ */
[data-theme="v8"] .exec-kpi-grid-v2,
[data-theme="v8"] .kpi-emphasized-wrap {
  border-top: 1.5pt solid var(--wp-ink);
  border-bottom: 1.5pt solid var(--wp-ink);
}
[data-theme="v8"] .exec-kpi-grid-v2 > *,
[data-theme="v8"] .kpi-emphasized-wrap > * {
  border-right: 0.3pt solid var(--wp-rule-light);
}
[data-theme="v8"] .exec-kpi-grid-v2 .sev-good,
[data-theme="v8"] .kpi-emphasized-wrap .sev-good,
[data-theme="v8"] .exec-kpi-grid-v2 .severity-good {
  border-top: 3pt solid var(--wp-accent-green);
  background: linear-gradient(180deg, var(--wp-accent-green-tint) 0%, transparent 30%);
}
[data-theme="v8"] .exec-kpi-grid-v2 .sev-warn,
[data-theme="v8"] .kpi-emphasized-wrap .sev-warn,
[data-theme="v8"] .exec-kpi-grid-v2 .severity-warning {
  border-top: 3pt solid var(--wp-accent-orange);
  background: linear-gradient(180deg, var(--wp-accent-orange-tint) 0%, transparent 30%);
}
[data-theme="v8"] .exec-kpi-grid-v2 .sev-crit,
[data-theme="v8"] .kpi-emphasized-wrap .sev-crit,
[data-theme="v8"] .exec-kpi-grid-v2 .severity-critical {
  border-top: 3pt solid var(--wp-accent-red);
  background: linear-gradient(180deg, var(--wp-accent-red-tint) 0%, transparent 30%);
}

/* 数値: 等幅 + tabular-nums */
[data-theme="v8"] .num,
[data-theme="v8"] td.num,
[data-theme="v8"] .kpi-value,
[data-theme="v8"] .metric-value {
  font-family: var(--wp-mono);
  font-variant-numeric: tabular-nums;
  letter-spacing: -0.02em;
}

/* テーブル: 勝色ヘッダ + zebra */
[data-theme="v8"] table.data-table,
[data-theme="v8"] table.ledger,
[data-theme="v8"] table.salesnow-table,
[data-theme="v8"] table.region-compare-table {
  width: 100%;
  border-collapse: collapse;
  border-top: 2pt solid var(--wp-brand);
  border-bottom: 1pt solid var(--wp-ink);
  font-size: 10pt;
}
[data-theme="v8"] table.data-table thead tr,
[data-theme="v8"] table.ledger thead tr,
[data-theme="v8"] table.salesnow-table thead tr,
[data-theme="v8"] table.region-compare-table thead tr {
  background: var(--wp-brand);
  color: var(--wp-paper);
}
[data-theme="v8"] table.data-table thead th,
[data-theme="v8"] table.ledger thead th,
[data-theme="v8"] table.salesnow-table thead th,
[data-theme="v8"] table.region-compare-table thead th {
  font-size: 8.5pt; font-weight: 700;
  letter-spacing: 0.06em; text-transform: uppercase;
  color: var(--wp-paper);
  padding: 2mm 2.5mm; text-align: left;
}
[data-theme="v8"] table.data-table tbody tr:nth-child(even),
[data-theme="v8"] table.ledger tbody tr:nth-child(even),
[data-theme="v8"] table.salesnow-table tbody tr:nth-child(even),
[data-theme="v8"] table.region-compare-table tbody tr:nth-child(even) {
  background: var(--wp-zebra);
}
[data-theme="v8"] table.data-table td,
[data-theme="v8"] table.ledger td,
[data-theme="v8"] table.salesnow-table td,
[data-theme="v8"] table.region-compare-table td {
  padding: 1.6mm 2.5mm;
  border-bottom: 0.3pt solid var(--wp-rule-light);
}

/* 自社行ハイライト */
[data-theme="v8"] table tr.is-self,
[data-theme="v8"] table tr.self-row {
  background: linear-gradient(90deg, var(--wp-accent-yellow) 0%, var(--wp-accent-yellow) 1mm, var(--wp-brand-tint) 1mm) !important;
  font-weight: 700;
}

/* severity 記号: 色非依存 */
[data-theme="v8"] .sev-good::before,
[data-theme="v8"] .severity-good::before {
  content: "◯ "; font-weight: 700; color: var(--wp-accent-green);
}
[data-theme="v8"] .sev-warn::before,
[data-theme="v8"] .severity-warning::before {
  content: "△ "; font-weight: 700; color: var(--wp-accent-orange);
}
[data-theme="v8"] .sev-crit::before,
[data-theme="v8"] .severity-critical::before {
  content: "× "; font-weight: 700; color: var(--wp-accent-red);
}

/* 図表のキャプション + 印刷収まり */
[data-theme="v8"] figure,
[data-theme="v8"] .echart-wrap,
[data-theme="v8"] .chart-container {
  margin: 3mm 0;
  padding: 2mm 0;
  border-top: 2pt solid var(--wp-brand);
  border-bottom: 1pt solid var(--wp-ink);
  break-inside: avoid !important;
  page-break-inside: avoid !important;
}
[data-theme="v8"] figcaption {
  font-size: 10.5pt; font-weight: 700;
  margin-bottom: 2mm;
  display: flex; justify-content: space-between; align-items: baseline;
}
[data-theme="v8"] figcaption .fig-num,
[data-theme="v8"] figcaption .figure-num {
  font-family: var(--wp-mono);
  font-size: 9pt; font-weight: 700;
  color: var(--wp-brand);
  letter-spacing: 0.08em; text-transform: uppercase;
}

/* 注記・脚注 */
[data-theme="v8"] .notes-section,
[data-theme="v8"] .footnotes,
[data-theme="v8"] .glossary {
  border-top: 1pt solid var(--wp-ink);
  border-bottom: 1pt solid var(--wp-ink);
  padding: 3mm 0; margin: 4mm 0;
  font-size: 9.5pt;
  break-inside: avoid;
}

/* 印刷時の追加保証: ヘッダ/フッタ + 改ページ強化 */
@media print {
  [data-theme="v8"] {
    background: var(--wp-paper);
  }
  @page {
    size: A4 portrait;
    margin: 12mm 11mm;
  }
  [data-theme="v8"] figure,
  [data-theme="v8"] .echart-wrap,
  [data-theme="v8"] .chart-container,
  [data-theme="v8"] table.data-table,
  [data-theme="v8"] table.ledger,
  [data-theme="v8"] .exec-kpi-grid-v2,
  [data-theme="v8"] .kpi-emphasized-wrap,
  [data-theme="v8"] .glossary,
  [data-theme="v8"] .notes-section {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }
  [data-theme="v8"] h2,
  [data-theme="v8"] h3 {
    page-break-after: avoid !important;
    break-after: avoid !important;
  }
}
"#.to_string()
}

/// V7a Editorial テーマ (2026-05-01)
///
/// 編集記事スタイル: Serif 大見出し + 余白重視 + 細い罫線。
/// `[data-theme="v7a"]` 属性で既存マークアップに上書き適用。
pub(super) fn render_theme_v7a_editorial() -> String {
    r#"
/* ============================================================
   V7a Editorial Theme (2026-05-01)
   ============================================================ */

[data-theme="v7a"] {
  --ed-ink: #1A1A1A;
  --ed-paper: #FAFAF7;
  --ed-rule: #1A1A1A;
  --ed-rule-light: #D4D4D0;
  --ed-accent: #8B0000;
  --ed-accent-soft: #FAF0F0;
  --ed-muted: #6B6B6B;
  --ed-serif: 'Noto Serif JP', 'Hiragino Mincho ProN', 'Yu Mincho', serif;
  --ed-sans: 'Helvetica Neue', 'Hiragino Sans', sans-serif;
}

[data-theme="v7a"] body {
  background: var(--ed-paper);
  color: var(--ed-ink);
  font-family: var(--ed-serif);
  font-size: 11pt;
  line-height: 1.75;
  -webkit-print-color-adjust: exact;
  print-color-adjust: exact;
}

/* 大見出し: 余白重視・Serif */
[data-theme="v7a"] h1 {
  font-family: var(--ed-serif);
  font-size: 32pt; font-weight: 900;
  line-height: 1.15;
  border-top: 4pt solid var(--ed-ink);
  border-bottom: 1pt solid var(--ed-ink);
  padding: 4mm 0; margin: 8mm 0 6mm;
  letter-spacing: -0.02em;
}
[data-theme="v7a"] h2 {
  font-family: var(--ed-serif);
  font-size: 20pt; font-weight: 900;
  border-top: 1pt solid var(--ed-ink);
  padding: 4mm 0 1mm; margin: 10mm 0 4mm;
  letter-spacing: -0.01em;
  break-after: avoid;
}
[data-theme="v7a"] h2::before {
  content: ""; display: inline-block;
  width: 6mm; height: 0.5mm;
  background: var(--ed-accent);
  margin-right: 3mm; vertical-align: middle;
}
[data-theme="v7a"] h3 {
  font-family: var(--ed-sans);
  font-size: 9.5pt; font-weight: 700;
  letter-spacing: 0.18em; text-transform: uppercase;
  color: var(--ed-accent);
  margin: 5mm 0 2mm;
}

/* リード文: 大きく、Serif */
[data-theme="v7a"] .lead,
[data-theme="v7a"] .section-header-meta,
[data-theme="v7a"] .exec-story-guide {
  font-family: var(--ed-serif);
  font-size: 13pt; font-weight: 400;
  line-height: 1.7;
  border-left: 2pt solid var(--ed-accent);
  padding: 1mm 0 1mm 6mm;
  margin: 4mm 0 6mm;
  color: var(--ed-ink);
  background: transparent;
}

/* 段落: 余白多め */
[data-theme="v7a"] p {
  margin: 0 0 4mm;
  line-height: 1.85;
  text-indent: 0;
}
[data-theme="v7a"] p strong {
  font-weight: 900;
  color: var(--ed-accent);
  background: none;
}

/* セクション境界: 細い水平罫 */
[data-theme="v7a"] .section,
[data-theme="v7a"] section {
  border-top: 0.5pt solid var(--ed-rule-light);
  margin-top: 10mm; padding-top: 6mm;
}
[data-theme="v7a"] .section:first-child,
[data-theme="v7a"] section:first-of-type {
  border-top: none; padding-top: 0;
}

/* KPI: 数値大きく、罫細く、余白広く */
[data-theme="v7a"] .exec-kpi-grid-v2,
[data-theme="v7a"] .kpi-emphasized-wrap {
  border-top: 0.5pt solid var(--ed-ink);
  border-bottom: 0.5pt solid var(--ed-ink);
  padding: 4mm 0;
}
[data-theme="v7a"] .exec-kpi-grid-v2 > *,
[data-theme="v7a"] .kpi-emphasized-wrap > * {
  padding: 4mm 6mm;
  border-right: 0.3pt solid var(--ed-rule-light);
}
[data-theme="v7a"] .kpi-value,
[data-theme="v7a"] .metric-value {
  font-family: var(--ed-serif);
  font-size: 32pt; font-weight: 900;
  letter-spacing: -0.03em;
  line-height: 1.05;
}

/* 数値: Serif tabular-nums (Editorial らしさ) */
[data-theme="v7a"] .num,
[data-theme="v7a"] td.num {
  font-family: var(--ed-serif);
  font-variant-numeric: tabular-nums;
  font-weight: 700;
}

/* テーブル: 細い罫、ヘッダ uppercase Sans */
[data-theme="v7a"] table.data-table,
[data-theme="v7a"] table.ledger,
[data-theme="v7a"] table.salesnow-table,
[data-theme="v7a"] table.region-compare-table {
  width: 100%;
  border-collapse: collapse;
  border-top: 1pt solid var(--ed-ink);
  border-bottom: 0.5pt solid var(--ed-ink);
  font-family: var(--ed-serif);
  font-size: 10pt;
}
[data-theme="v7a"] table.data-table thead tr,
[data-theme="v7a"] table.ledger thead tr,
[data-theme="v7a"] table.salesnow-table thead tr,
[data-theme="v7a"] table.region-compare-table thead tr {
  background: transparent;
  border-bottom: 0.5pt solid var(--ed-ink);
}
[data-theme="v7a"] table.data-table thead th,
[data-theme="v7a"] table.ledger thead th,
[data-theme="v7a"] table.salesnow-table thead th,
[data-theme="v7a"] table.region-compare-table thead th {
  font-family: var(--ed-sans);
  font-size: 8pt; font-weight: 700;
  letter-spacing: 0.16em; text-transform: uppercase;
  color: var(--ed-muted);
  padding: 2mm 3mm;
  text-align: left;
}
[data-theme="v7a"] table.data-table td,
[data-theme="v7a"] table.ledger td {
  padding: 2mm 3mm;
  border-bottom: 0.3pt solid var(--ed-rule-light);
}
[data-theme="v7a"] table tr.is-self,
[data-theme="v7a"] table tr.self-row {
  background: var(--ed-accent-soft) !important;
  font-weight: 700;
}

/* 図表: Serif キャプション */
[data-theme="v7a"] figure,
[data-theme="v7a"] .echart-wrap,
[data-theme="v7a"] .chart-container {
  margin: 5mm 0;
  break-inside: avoid !important;
  page-break-inside: avoid !important;
}
[data-theme="v7a"] figcaption {
  font-family: var(--ed-sans);
  font-size: 9pt; font-weight: 700;
  letter-spacing: 0.12em; text-transform: uppercase;
  color: var(--ed-muted);
  margin-top: 2mm;
}

/* severity 記号 */
[data-theme="v7a"] .sev-good::before,
[data-theme="v7a"] .severity-good::before {
  content: "◯ "; font-weight: 700;
}
[data-theme="v7a"] .sev-warn::before,
[data-theme="v7a"] .severity-warning::before {
  content: "△ "; font-weight: 700;
}
[data-theme="v7a"] .sev-crit::before,
[data-theme="v7a"] .severity-critical::before {
  content: "× "; font-weight: 700; color: var(--ed-accent);
}

/* 注記・脚注 */
[data-theme="v7a"] .notes-section,
[data-theme="v7a"] .footnotes,
[data-theme="v7a"] .glossary {
  font-family: var(--ed-serif);
  font-size: 9pt;
  line-height: 1.7;
  border-top: 0.5pt solid var(--ed-rule-light);
  padding: 4mm 0; margin: 6mm 0;
  color: var(--ed-muted);
}

@media print {
  [data-theme="v7a"] {
    background: var(--ed-paper);
  }
  @page {
    size: A4 portrait;
    margin: 14mm 14mm;
  }
  [data-theme="v7a"] figure,
  [data-theme="v7a"] .echart-wrap,
  [data-theme="v7a"] .chart-container,
  [data-theme="v7a"] table.data-table,
  [data-theme="v7a"] table.ledger,
  [data-theme="v7a"] .exec-kpi-grid-v2,
  [data-theme="v7a"] .kpi-emphasized-wrap {
    break-inside: avoid !important;
    page-break-inside: avoid !important;
  }
  [data-theme="v7a"] h2,
  [data-theme="v7a"] h3 {
    page-break-after: avoid !important;
    break-after: avoid !important;
  }
}
"#.to_string()
}
