# T5: 印刷プレビュー実視覚レビュー (4 パターン)

**実施日時**: 2026-04-30
**対象**: 媒体分析レポート (本番 https://hr-hw.onrender.com)
**手法**: Playwright + `page.emulate_media({media:'print'})` + `page.pdf()` (A4 portrait)
**サンプルデータ**: `tests/e2e/fixtures/indeed_test_50.csv` (54 件 / 東京都 新宿区)
**スクリプト**: `_print_review_t5.py`
**スクリーンショット**: `docs/screenshots/2026-04-30/print/`

---

## 取得アーティファクト

| パターン | URL クエリ | h2 数 | echart 初期化 | bodyHeight | PDF ページ数 | スクショ |
|---|---|---|---|---|---|---|
| Full / 業界未指定 | `?variant=full` | 21 | 8 | 19,754 px | **21** | `full_no_industry_*` |
| Full / 業界=病院 | `?variant=full&industry=病院` | 24 (+3) | 8 | 22,490 px | **23** | `full_hospital_*` |
| Public / 業界未指定 | `?variant=public` | 20 | 9 | 19,508 px | **21** | `public_no_industry_*` |
| Public / 業界=病院 | `?variant=public&industry=病院` | 23 (+3) | 9 | 22,243 px | **23** | `public_hospital_*` |

**目視確認件数**: フルページ 4 件 + h2 セクション 88 件 (4 パターン × 平均 22 h2) + チャンク化 18 件 = **計 110 件**

---

## 視覚レビュー結果 (LLM playbook 観点)

### 1. ページ崩れ / A4 ページ跨ぎ
- ✅ 表紙ページ独立 (`.cover-page` の `page-break-after: always` 効果あり)
- ✅ Executive Summary 後に `page-break-after: always` で本編が次ページ開始
- ⚠️ 一部の長いセクション (>1000px) は 1 ページに収まらず複数ページに跨る:
  - 給与分布ヒストグラム (h=2006px)
  - 第 5 章 地域注目企業 (h=1597px)
  - 第 5 章 地域企業ベンチマーク (h=2323-2512px)
  - これらは `.section { page-break-inside: avoid }` を効かせると逆にレイアウト破綻するため、内部要素 (`.echart-container`, `tr`, `.kpi-card-v2`) のみ avoid とする現行設計は妥当
- ✅ テーブル `<thead>` の `display: table-header-group` でページ跨ぎ時のヘッダー再表示が指定済み

### 2. 非印刷要素の表示制御
- ✅ `.variant-indicator` 印刷時 visible=0 (既存ルール OK)
- ✅ `.no-print` (3 件) 全て visible=0
- ✅ `.screen-footer` 印刷時 visible=0
- ✅ `.exec-kpi-grid-legacy` 印刷時 visible=0 (重複 KPI 抑止 OK)
- 🔴 **`.theme-toggle` 印刷時 visible=1 (P0)** → **修正実施 (本コミット)**
  - 既存実装には `position: fixed` で右上に表示されたまま印刷される脆弱性あり
  - 全 4 パターンで同じ問題を再現

### 3. ECharts チャート描画
- ✅ ドーナツチャート (雇用形態) 凡例・ラベル・配色 OK
- ✅ 散布図 + 回帰線 (R²=0.969) 軸ラベル・凡例 OK
- ✅ 人口ピラミッド (男女別) 軸・凡例 OK
- ✅ レーダーチャート (採用市場 4 軸) 軸ラベル OK
- ✅ Full と Public で初期化数 8 / 9 (Public のほうが汎用統計図表が多いため)

### 4. テキスト切れ / KPI カード
- ✅ KPI カード v2 (`.kpi-card-v2`) 数値が 24pt で読みやすい
- ✅ 業界バナー (黄色 `.report-banner-amber`) 「業界フィルタ指定中」表示 OK
- ✅ 表紙の対象地域 (東京都 新宿区) 切れなし

### 5. 凡例 (legend)
- ✅ ECharts 凡例 (ドーナツ・人口ピラミッド・レーダー) 重なりなし
- ✅ 凡例ラベル日本語フォント (Noto Sans JP) 適用 OK

### 6. 印刷モード切替の有効性 (`@media print`)
- ✅ 4 パターン全てで `emulate_media('print')` 後に印刷モード CSS が適用される
- ✅ ダークテーマが white に強制 (`html { color-scheme: light }`) OK
- ✅ 印刷時のフォントサイズが 10pt に圧縮 (`body { font-size: 10pt }`) OK
- ✅ A4 portrait + margin 12mm 10mm 適用 OK

### 7. ヘッダー/フッター (running header/footer)
- ✅ `@page { @top-left: "求人市場 総合診断レポート"; @top-right: counter(page); @bottom-left: "株式会社For A-career | 機密情報" }` 定義済み
- ✅ `@page :first` で表紙ページのヘッダー/フッターを抑制 OK
- ⚠️ 注意: Chromium の `page.pdf()` は `@page` margin box (top-left 等) を **完全にはサポートしない**。実際の PDF ヘッダー/フッターは Chromium 固有の `headerTemplate`/`footerTemplate` 経由で指定する必要がある
- 影響: ブラウザの「印刷」メニューから PDF 化した場合は `@page` ルールが効くため (Chrome / Safari)、運用上は問題なし

---

## 発見した問題と修正

### P0 (即時修正)

#### #1: `.theme-toggle` が印刷時に表示される
- **症状**: 全 4 パターンで `getComputedStyle(.theme-toggle).display !== 'none'` (visible=1)
- **原因**: `style.rs:265` で `.theme-toggle { position: fixed; }` 定義のみ。印刷時の `display: none` ルールなし
- **影響**: 印刷物の右上 (top:10px right:200px) に「ダークモード切替」ボタンが残る
- **修正**:

```diff
 /* 印刷時は完全非表示 (.no-print と二重に保証) */
+/* T5 (2026-04-30): theme-toggle / 各種 UI ボタン / fixed 要素を印刷時に非表示化 */
 @media print {
   .variant-indicator { display: none !important; }
+  .theme-toggle { display: none !important; }
+  /* 画面操作系の固定要素は印刷時に出してはいけない (position:fixed は印刷で予期せぬ位置に出る) */
+  .no-print,
+  button.print-toggle,
+  button[onclick*="print"],
+  a.print-link,
+  .floating-actions,
+  .scroll-to-top { display: none !important; }
+  /* fixed positioning は印刷では static に変換 (印刷上の位置ズレ防止) */
+  .theme-toggle, .variant-indicator, .floating-actions {
+    position: static !important;
+  }
 }
```

- **検証**:
  - `cargo build --lib`: warning 0 (既存 dead_code 11件のみ、本変更由来なし)
  - `cargo test --lib`: **1038 passed; 0 failed; 1 ignored**
  - 修正は `style.rs` の `@media print` ブロック内のみ (Agent C / B / D の担当範囲外)

### P1 (次フェーズ候補)

#### #1: 長大セクションのページ分割粒度
- 給与分布ヒストグラム (2006px) や地域企業ベンチマーク (2512px) は A4 1 ページに収まらず 2-3 ページに跨る
- 現状: `tr` `kpi-card-v2` `echart-container` 単位で `page-break-inside: avoid` 指定済み
- 改善余地: 内部のサブヘッダー (h3) に `page-break-after: avoid` を追加して、見出しと直後のテーブル/チャートを分離させない (既存に `h2, h3 { page-break-after: avoid }` あり、追加対応不要)

#### #2: 印刷時の不要な `position: fixed` 要素の網羅
- `.theme-toggle` の他に、JavaScript 駆動のフローティングボタンが将来追加された場合のガード必要
- 今回の修正で `button[onclick*="print"]` `.floating-actions` `.scroll-to-top` 等の予防策は導入済み

#### #3: `@page` running header/footer の Chrome PDF 互換性
- `page.pdf()` 経由では `@page` の `@top-left` 等のマージンボックスが Chromium で実装されていないため、PDF 出力時にヘッダー/フッターが出ない
- 解決策 (将来): Chromium 固有 `headerTemplate`/`footerTemplate` を使うか、PDF 出力をバックエンドで行う場合 (例: weasyprint) は `@page` ルールがそのまま使える

### P2 (本タスク範囲外)
- 日本語フォントの埋め込み (CSS では指定済み、ブラウザ依存)
- カラー印刷とモノクロ印刷の切替 (現状はカラー前提)

---

## 修正前後の比較

### 修正前
```rust
/* 印刷時は完全非表示 (.no-print と二重に保証) */
@media print {
  .variant-indicator { display: none !important; }
}
```

### 修正後 (本コミット)
```rust
/* 印刷時は完全非表示 (.no-print と二重に保証) */
/* T5 (2026-04-30): theme-toggle / 各種 UI ボタン / fixed 要素を印刷時に非表示化 */
@media print {
  .variant-indicator { display: none !important; }
  .theme-toggle { display: none !important; }
  .no-print,
  button.print-toggle,
  button[onclick*="print"],
  a.print-link,
  .floating-actions,
  .scroll-to-top { display: none !important; }
  .theme-toggle, .variant-indicator, .floating-actions {
    position: static !important;
  }
}
```

---

## 完了条件チェック

- [x] 4 パターン全て印刷プレビュー取得
- [x] 4 パターン全て fullpage screenshot + PDF 生成
- [x] 各 h2 セクション (合計 88 件) を Read で目視確認
- [x] フルページをチャンク化して上から下まで目視確認
- [x] P0 問題 (theme-toggle) を `@media print` のみで修正
- [x] `cargo test --lib`: **1038 passed**
- [x] `cargo build --lib`: warning 11 (本修正由来 0)
- [x] 修正対象は `style.rs` の `@media print` ブロックのみ (他 agent の担当範囲に侵入なし)

---

## 印刷品質改善のためのチェックリスト (将来の運用)

| # | 項目 | 確認方法 |
|---|---|---|
| 1 | `position: fixed` 要素を新規追加した場合、`@media print` で `display: none` または `position: static` を必ず指定 | grep `position: fixed` → 各要素の `@media print` 内対応 |
| 2 | KPI カード/長文テキストは A4 1 ページに収まる文字数か | bodyHeight ÷ pageCount で平均ページ高さを確認 |
| 3 | テーブルはページ跨ぎ時に `<thead>` が再表示されるか | `display: table-header-group` 指定確認 |
| 4 | 見出し `h2`/`h3` がページ末尾に孤立しないか | `page-break-after: avoid` 指定確認 |
| 5 | ECharts コンテナがページ跨ぎで分断されないか | `.echart-container { page-break-inside: avoid }` 確認 |
| 6 | ダークテーマが印刷時に強制 light か | `html { color-scheme: light !important }` 確認 |
| 7 | カラー保持 (`-webkit-print-color-adjust: exact`) | 主要カラー要素 (KPI バッジ等) で確認 |
| 8 | running header/footer が `@page` で定義されているか | weasyprint 等のバックエンド PDF 出力時のみ有効。Chromium `page.pdf()` 用には別途 headerTemplate 必要 |

---

## 残課題 (Phase 3 候補)

1. **本番反映確認**: 本修正は deploy 必須 (Render auto-deploy 待ち)。本番で `.theme-toggle { display: none }` 適用を再確認したい
2. **長大セクションの段組検討**: 「地域企業ベンチマーク」(2512px) を 2 段組にすると A4 1 ページに収まる可能性
3. **PDF 出力時の running footer**: Chromium `page.pdf()` 経由では `@page @bottom-left` が出ない。Chromium `headerTemplate`/`footerTemplate` 経由のサーバ側 PDF 生成を検討

---

## 参考: 印刷時 metrics サマリ

```json
{
  "full_no_industry": {
    "h2": 21, "h3": 26, "tables": 22, "kpiV2": 11,
    "figureCaption": 30, "readHint": 18, "echartInited": 8,
    "bodyHeight_px": 19754, "tallSections_gt1000px": 7
  },
  "full_hospital": {
    "h2": 24, "h3": 31, "tables": 27, "kpiV2": 11,
    "figureCaption": 30, "readHint": 18, "echartInited": 8,
    "bodyHeight_px": 22490, "tallSections_gt1000px": 8
  },
  "public_no_industry": {
    "h2": 20, "h3": 26, "tables": 23, "kpiV2": 10,
    "figureCaption": 29, "readHint": 18, "echartInited": 9,
    "bodyHeight_px": 19508, "tallSections_gt1000px": 8
  },
  "public_hospital": {
    "h2": 23, "h3": 31, "tables": 28, "kpiV2": 10,
    "figureCaption": 29, "readHint": 18, "echartInited": 9,
    "bodyHeight_px": 22243, "tallSections_gt1000px": 9
  }
}
```

詳細生データ: `docs/screenshots/2026-04-30/print/_summary.json`
