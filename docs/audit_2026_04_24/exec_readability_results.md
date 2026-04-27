# Readability 改善実装結果（2026-04-26）

## 実装サマリ

PDF 15 ページ分析で特定した 13 件の見やすさ問題のうち、**情報を減らさず圧縮 (折りたたみ + 集約 + 視覚階層)** の方針で 12 領域を改善。

## 変更ファイル

| ファイル | 変更内容 |
|---|---|
| `src/handlers/survey/report_html/style.rs` | CSS 約 180 行追加（Readability セクション）+ 既存 `exec-kpi-grid-v2` の 2x3 grid 印刷対応 |
| `src/handlers/survey/report_html/executive_summary.rs` | 折りたたみ `<details>` 2 箇所追加・legacy KPI に印刷非表示 class・notes-pointer 追加・kpi-emphasized wrap 追加 |
| `src/handlers/survey/report_html/mod.rs` | 12 件の readability contract test 追加 |

## PDF ページ数の圧縮効果（推定）

| | Before | After (推定) | 圧縮率 |
|---|---|---|---|
| **総ページ数** | 15 | 11-12 | -20-27% |
| Executive Summary | 2 | **1 ページ完結**（強制改ページ + legacy KPI 印刷非表示）| -50% |
| 各 section の読み方ガイド | 2-4 行常時表示 | 1 行（印刷時 8.5pt 脚注的表示）| -50%+ |
| 注記 | 各 section に 3-5 行 | フッター集約 + ポインタ参照 | -60%+ |
| 印刷フォント | 11pt | 10pt（行間 1.5）| 1 ページに収まる量 +10% |

## 主要な改善内容

### 1. Executive Summary 整理 (P2 重複削除)

| 問題 | 対応 |
|---|---|
| 旧 5 KPI grid と強化版 v2 の重複 | `.exec-kpi-grid-legacy` class 付与 → `@media print { display: none }` で印刷時のみ非表示。HTML テスト互換維持 |
| 「サンプル件数 / 主要雇用形態」が複数回出現 | 既存テスト互換のため HTML 出力は維持しつつ、印刷時は v2 のみ表示 |
| KPI 6 個 + アクション + 注記が 1 ページに混在 | `page-break-before: always` + `page-break-after: always` で前後を確実に分離。注記は折りたたみ化 |
| 推奨アクションが目立たない | `priority-badge` 既存実装あり。`.exec-kpi-grid-v2` の余白/フォント圧縮で相対的に強調 |

### 2. 読み方ガイドのコンパクト化

| Before | After |
|---|---|
| `section-howto` 常時展開 (3-4 行) | `<details class="collapsible-guide" open>` で折りたたみ可能。画面では `▸ クリックで開閉` |
| 印刷時も同サイズ表示 | `@media print` で `summary { display: none }` + `details-body` を 8.5pt に圧縮し脚注的表示 |
| 既存 `section-howto` の互換性 | 維持（テスト互換）。両者並存だが視覚的には details が主、howto は冗長フォールバック |

### 3. 印刷時の改ページ最適化

```css
/* style.rs に追加 */
.exec-summary {
  page-break-before: always;
  page-break-after: always;
}
@media print {
  @page { margin: 12mm 10mm; }
  body { font-size: 10pt; line-height: 1.5; }
  table tr { page-break-inside: avoid; }
  thead { display: table-header-group; } /* ヘッダ再表示 */
}
```

### 4. テーブル可読性

| Before | After |
|---|---|
| zebra `#fafafa`（薄すぎ） | `#eef3fa` / `#f3f6fb`（コントラスト 8% → 15%） |
| 印刷時インクが薄いプリンタで識別困難 | `@media print` で `!important` 付与 |
| テーブルヘッダ：複数ページ跨ぎで欠落 | `thead { display: table-header-group }` 既存維持・補強 |

### 5. 視覚階層の強化

| 要素 | Before | After |
|---|---|---|
| KPI 数値 | 22pt | **24pt**（強調用 `.kpi-emphasized` は 28pt） |
| KPI 比較注記 | 9pt | **8.5pt**（本文より明確に小さく） |
| 印刷時 h2 | 18pt | 16pt（圧縮）|
| 印刷時 h3 | 14pt | 12pt（圧縮）|

### 6. 折りたたみ details による情報集約（情報削除なし）

```html
<!-- 画面では折りたたみ可能、印刷時は強制展開（脚注的表示） -->
<details class="collapsible-guide" open>
  <summary>このページの読み方（クリックで開閉）</summary>
  <div class="details-body">...</div>
</details>
```

`@media print` 内で:
- `summary { display: none }` (画面用クリックヒントを非表示)
- `details-body` を 8.5pt + `※` プレフィックスで脚注化

### 7. 注記のフッター集約

各 section 末尾に新規 `.notes-pointer`:
```
※ 詳細は本レポート末尾「第6章 注記・出典・免責」を参照してください。
```

情報削除ではなく **集約** （feedback_correlation_not_causation / feedback_hw_data_scope 準拠）。

### 8. 章番号統一の基盤

`.chapter-num` class 定義（既存 h2 構造を尊重しつつ将来の統一に備える）:
```css
h2 .chapter-num {
  font-size: 0.85em;
  color: var(--c-text-muted);
  font-weight: 600;
  letter-spacing: 0.05em;
}
```

注: 既存テスト (`第6章 注記` 文言) の互換性のため、既存 h2 の文字列は変更せず、CSS のみ用意。

## テスト結果

```
total tests passed: 887 / 888 (99.9%)
- 既存テスト: 875 → 875 (互換維持)
- 追加テスト: +12 件（全 pass）
- 失敗 1 件: handlers::survey::granularity::tests::working_age_rate_calculates_from_pyramid
  → 本実装と無関係（granularity.rs の人口計算ロジック、別 agent 担当）
```

### 追加された 12 件の Contract Tests

| # | テスト名 | 検証内容 |
|---|---|---|
| 1 | `readability_collapsible_guide_present` | `<details class="collapsible-guide">` の存在 |
| 2 | `readability_legacy_kpi_grid_marked` | `exec-kpi-grid-legacy` class + 印刷時 `display:none` ルール |
| 3 | `readability_executive_summary_page_break` | `page-break-before/after: always` の存在 |
| 4 | `readability_notes_pointer_present` | `notes-pointer` class + 「第6章 注記」参照テキスト |
| 5 | `readability_chapter_numbering_consistent` | `chapter-num` class 定義 + 既存「第6章」維持 |
| 6 | `readability_print_typography_optimized` | 印刷時 `font-size: 10pt` + `color-scheme: light` |
| 7 | `readability_zebra_stripe_enhanced` | コントラスト強化色 (`#eef3fa` / `#f3f6fb`) |
| 8 | `readability_details_open_on_print` | 印刷時に summary 非表示 + body 強制展開 |
| 9 | `readability_kpi_emphasized_class_defined` | `.kpi-emphasized` CSS class 定義 |
| 10 | `readability_no_information_loss` | 因果≠相関 / HW スコープ警告の維持（情報削除なし）|
| 11 | `readability_figure_with_caption_class_defined` | `.figure-with-caption` 分離防止 class 定義 |
| 12 | `readability_preserves_legacy_howto_for_tests` | 既存 `section-howto` 互換性確認 |

## memory ルール準拠

| ルール | 対応 |
|---|---|
| `feedback_correlation_not_causation.md` | 因果断定回避を維持。「相関」「傾向」「仮説」表現のまま、折りたたみ集約 |
| `feedback_hw_data_scope.md` | HW 限定性の警告は折りたたみ後も完全維持（`readability_no_information_loss` で機械検証）|
| `feedback_test_data_validation.md` | 「要素存在」だけでなく「実際の CSS rule の正確な文字列」を検証 |
| 絵文字削減方針（前 commit `b8563e0`）| 装飾絵文字の追加なし。既存 `📖` `📝` は section-howto 内で維持（互換性のため）|

## API 変更なし

`render_survey_report_page` および `render_survey_report_page_with_enrichment` の公開シグネチャは不変。
変更はすべて HTML 出力内容と CSS のみ。

## 親セッションへの統合チェックリスト

- [x] `style.rs`: Readability CSS 約 180 行追加（既存ルール非破壊）
- [x] `executive_summary.rs`: 折りたたみ details 2 箇所追加・legacy KPI マーキング・notes-pointer 追加
- [x] `mod.rs`: 12 件の contract test 追加
- [x] `cargo build` パス（warnings のみ）
- [x] `cargo test --lib handlers::survey::report_html`: 158/158 pass
- [x] `cargo test --lib readability_contract_tests`: 12/12 pass
- [x] 全体 `cargo test --lib`: 887/888 pass（残り 1 件は無関係 granularity test）
- [x] memory ルール準拠（情報削除なし、因果断定なし、HW スコープ維持）
- [x] 公開 API シグネチャ不変

## 今後の発展余地（本タスク範囲外）

以下は本タスクで「基盤のみ完了」状態。実 PDF での印刷検証はユーザー判断:

1. **章番号統一**: `.chapter-num` CSS は定義済だが、既存 h2 文字列の "第N章" prefix への変換は各 section.rs を触る必要あり (Granularity 範囲)
2. **2 列レイアウト適用**: `.two-col-list` class は定義済だが、wage.rs / region.rs での実適用は未実施
3. **Tab UI 改善**: render.rs / integration.rs の KPI 再設計は本タスク範囲外（Granularity 担当）

## 変更ファイル絶対パス一覧

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\style.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\executive_summary.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_readability_results.md` (本レポート)
