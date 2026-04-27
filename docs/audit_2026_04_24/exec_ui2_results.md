# UI-2 実行結果: 印刷レポート 主要 6 sections 強化

**日時**: 2026-04-26
**スコープ**: V2 HW Dashboard 媒体分析タブ 印刷レポート (`/report/integrated`, `/api/survey/report`) のうち以下 6 sections
- Section 1: Executive Summary (`executive_summary.rs`)
- Section 3: 給与統計 (`salary_stats.rs`)
- Section 5: 散布図 (`scatter.rs`)
- Section 6/7: 地域分析 (`region.rs`)
- Section 4: 雇用形態 (`employment.rs`)
- Section 8/9/10: 最低賃金/企業/タグ (`wage.rs`)

**目的**: 「数表だけのレポート」→「物語のあるレポート」への変換。各セクションに図表番号、読み方ヒント、追加可視化、優先度バッジ、つなぎ文を導入。

---

## 1. 実装サマリ

### 変更ファイル一覧（絶対パス）

| 種別 | ファイル |
|------|---------|
| 新規 helper | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\helpers.rs` |
| CSS 追加 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\style.rs` |
| Section 1 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\executive_summary.rs` |
| Section 3 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\salary_stats.rs` |
| Section 5 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\scatter.rs` |
| Section 6/7 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\region.rs` |
| Section 4 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\employment.rs` |
| Section 8/9/10 強化 | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\wage.rs` |
| 新規テスト | `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs` (`ui2_contract_tests` モジュール) |

---

## 2. 各 Section の Before/After

### Section 1: Executive Summary

**Before**: KPI×5 横並び + 推奨アクション（severity badge のみ）
**After**:
- 冒頭「このページの読み方」3 ステップガイド (`section-howto`)
- KPI×6 強化版カード (`kpi-card-v2`) — アイコン + 大数値 + 単位 + 比較値 + 状態バッジ (`✓ 良好` / `⚠ 注意` / `🚨 警戒`)
- 既存 KPI×5 (`kpi-card`) も互換のため併存
- 推奨アクションに **優先度バッジ** (`🔴 即対応` / `🟡 1週間以内` / `🟢 後回し可`)
- 末尾に次セクションへのつなぎ (`section-bridge`)

### Section 3: 給与統計

**Before**: 平均/中央値/給与範囲の 3 カード + 信頼区間 + ヒストグラム×4
**After**:
- 「読み方」3 ステップガイド
- **表 3-1**: 給与統計サマリ（件数・平均・中央値・標準偏差・Q1/Q3/IQR 含む詳細表 + 備考列）
- **図 3-1**: IQR シェードバー（Q1-Q3 を青シェード、中央値を緑線、min/Q1/中央値/Q3/max のレジェンド付）
- **表 3-2**: 外れ値除外の前後比較（除外前/除外後/除外件数 + 割合）
- **図 3-2/3-3**: 下限給与ヒストグラム 20k刻み・5k刻み（既存に図番号追加）
- **図 3-4/3-5**: 上限給与ヒストグラム
- 各図表後に読み方ヒント

### Section 5: 散布図

**Before**: 散布図 + 回帰線 + R² 文字列
**After**:
- 「読み方」3 ステップガイド (R² 閾値: > 0.5 強い / 0.3-0.5 中 / < 0.3 弱い)
- **図 5-1**: 散布図 + 回帰線オーバーレイ
- **表 5-1**: 回帰分析サマリ（R²・傾き・切片・有効サンプル + 各値の意味列、R² 値は強さに応じてカラー化（緑/橙/グレー））
- 相関≠因果の読み方ヒント（強調表示）
- 末尾に次セクション（地域分析）へのつなぎ

### Section 6: 都道府県分析

**Before**: 件数テーブル Top 10 のみ
**After**:
- **表 6-1**: 都道府県別件数 Top 10（zebra stripe 適用）
- **図 6-1**: 簡易ヒートマップ（最大 8 セル/行、件数比に応じて 4 段階の濃度 h-1〜h-4）+ 凡例
- 読み方ヒント（カバレッジ重心の見方）
- 次セクションへのつなぎ

### Section 7: 市区町村分析

**Before**: Top 15 テーブル
**After**:
- **表 7-1**: 市区町村別給与 Top 15
- **同名市区町村マーカー** (`⚠ 同名`) — 伊達市 (北海道/福島県)、府中市 (東京都/広島県) 等を自動検出
- 読み方ヒント
- 次セクションへのつなぎ

### Section 4: 雇用形態

**Before**: ドーナツチャート + 雇用形態別給与テーブル
**After**:
- **図 4-1**: 雇用形態構成ドーナツチャート（既存 + 図番号）
- **表 4-1**: 雇用形態別 給与水準テーブル（zebra stripe）
- **図 4-2**: 雇用形態別 平均月給ドット比較（Dumbbell 風: 横バー長 = 月給比、ドット = 中央値、正社員=青、パート=橙）
- 読み方ヒント（単位混在の問題提起）
- 次セクション（emp_group_native）へのつなぎ

### Section 8: 最低賃金

**Before**: 統計サマリ + 差額テーブル
**After**:
- **表 8-1**: 時給換算 vs 最低賃金 差額 Top 10
- **差額バー列**: 中央線（最低賃金）からの乖離を視覚化（赤=未満、橙=50円未満で近接、緑=余裕）
- 読み方ヒント

### Section 9: 企業

**Before**: 件数 Top 15 + 給与水準 Top 15 テーブル
**After**:
- **表 9-1**: 件数 Top 15 + **件数バー列**（青バー=件数比、橙の縦線=平均月給比の 2 軸）
- **表 9-2**: 給与水準 Top 15
- 読み方ヒント（規模 × 給与水準の傾向解釈）

### Section 10: タグ

**Before**: ツリーマップ + 差分テーブル
**After**:
- **図 10-1**: タグ件数ツリーマップ（既存 + 図番号）
- **表 10-1**: タグ別給与差分テーブル（zebra stripe）
- 読み方ヒント（相関≠因果を強調）

---

## 3. 追加した図表（図表番号付き）

### 図（10 件）

| 図番号 | タイトル | 出力先 |
|--------|---------|--------|
| 図 1-1 | 主要 KPI ダッシュボード（アイコン・状態・比較値付き） | executive_summary.rs |
| 図 3-1 | IQR シェード（Q1-Q3 中央 50% レンジ + 中央値マーカー） | salary_stats.rs |
| 図 3-2 | 下限月給ヒストグラム（20,000円刻み・縦線=平均/中央値/最頻値） | salary_stats.rs |
| 図 3-3 | 下限月給ヒストグラム（5,000円刻み・微細解像度） | salary_stats.rs |
| 図 3-4 | 上限月給ヒストグラム（20,000円刻み） | salary_stats.rs |
| 図 3-5 | 上限月給ヒストグラム（5,000円刻み・微細解像度） | salary_stats.rs |
| 図 4-1 | 雇用形態構成ドーナツチャート（Top 6） | employment.rs |
| 図 4-2 | 雇用形態別 平均月給ドット比較 | employment.rs |
| 図 5-1 | 月給 下限 × 上限 散布図（回帰線オーバーレイ） | scatter.rs |
| 図 6-1 | 都道府県別 求人件数ヒートマップ（Top 10、濃度=件数） | region.rs |
| 図 10-1 | 訴求タグ件数 ツリーマップ（面積=件数） | wage.rs |

### 表（10 件）

| 表番号 | タイトル | 出力先 |
|--------|---------|--------|
| 表 3-1 | 給与統計サマリ（外れ値除外後） | salary_stats.rs |
| 表 3-2 | 外れ値除外の前後比較（IQR 法） | salary_stats.rs |
| 表 4-1 | 雇用形態別 給与水準（件数・平均月給・中央値） | employment.rs |
| 表 5-1 | 回帰分析サマリ + R² 閾値ガイド | scatter.rs |
| 表 6-1 | 都道府県別 求人件数 Top 10 | region.rs |
| 表 7-1 | 市区町村別 給与水準 Top 15（同名市区町村マーク付き） | region.rs |
| 表 8-1 | 時給換算 vs 最低賃金 差額 Top 10（差小→大） | wage.rs |
| 表 9-1 | 掲載件数の多い法人 Top 15（件数 + 平均月給 2 軸） | wage.rs |
| 表 9-2 | 給与水準の高い法人 Top 15 | wage.rs |
| 表 10-1 | タグ別 給与差分（全体比、件数 10+、`|差分|` 2% 以上） | wage.rs |

**合計**: 図 11 + 表 10 = **21 図表**

---

## 4. 共通強化要件の達成状況

| 要件 | 達成 |
|------|------|
| 図表番号（章番号 + 連番） | ✓ 全 21 図表に付与 |
| 読み方吹き出し（結論先取り） | ✓ 各 section に 1-2 箇所、合計 10 箇所以上 |
| 凡例の絵文字化 + 色 + テキスト (a11y) | ✓ 優先度バッジ (🔴/🟡/🟢)、状態バッジ (✓/⚠/🚨)、ヒートマップ凡例 |
| 印刷時 `page-break-inside: avoid` | ✓ 新規クラス全てに適用 (`@media print` ブロック内) |
| zebra stripe | ✓ `.zebra` クラスを Section 3-10 の全主要テーブルに付与 |
| 因果断定の回避（memory 準拠） | ✓ 「傾向」「目安」「観測」表現を徹底 |
| 既存 781 テスト破壊禁止 | ✓ 全テスト pass |
| 公開 API 不変 | ✓ `render_section_*` 関数のシグネチャは全て維持 |

---

## 5. 新規追加した CSS クラス（`style.rs` に追加）

```
.figure-caption / .fig-no            -- 図表キャプション
.read-hint / .read-hint-label        -- 読み方ヒント吹き出し
.section-howto / .howto-title        -- このページの読み方ガイド
.exec-kpi-grid-v2 / .kpi-card-v2     -- 強化版 KPI グリッド + カード
.kpi-card-v2.kpi-good/warn/crit      -- KPI 状態色分け
.kpi-icon / .kpi-status / .kpi-value
.kpi-unit / .kpi-compare
.priority-badge.priority-now/week/later  -- 優先度バッジ
.zebra                               -- テーブル zebra stripe
.iqr-bar / .iqr-shade / .iqr-median  -- IQR シェードバー
.iqr-bar-legend
.dumbbell-list / .dumbbell-row       -- 雇用形態 dumbbell chart
.db-label / .db-track / .db-line
.db-dot.dot-ft/dot-pt / .db-diff
.heatmap-grid / .heatmap-cell        -- 都道府県別ヒートマップ
.heatmap-cell.h-1/h-2/h-3/h-4/h-empty
.heatmap-legend / .swatch
.minwage-diff-bar / .mwd-fill        -- 最低賃金差分バー
.mwd-fill.below/near / .mwd-baseline
.section-bridge                      -- 次セクションへのつなぎ
```

---

## 6. 新規追加した helpers（`helpers.rs` に追加）

```
render_figure_caption(html, fig_no, title)
render_read_hint(html, body)         -- escape_html 適用
render_read_hint_html(html, body)    -- HTML 直挿し（<strong> 埋め込み用）
render_section_howto(html, lines)
render_section_bridge(html, text)
render_kpi_card_v2(html, icon, label, value, unit, compare, status, status_label)
priority_badge_html(sev) -> String
```

---

## 7. テスト結果

### 既存テスト互換性

```
running 825 tests
test result: ok. 825 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

**ベースライン (UI-2 開始前)**: 781 passed
**最終 (UI-2 完了後)**: 825 passed
**差分**: +44 (UI-2 が 21、その他 UI-3 等の並列 agent が 23 追加)

### UI-2 追加 contract tests（21 件、全 pass）

`src/handlers/survey/report_html/mod.rs` に `ui2_contract_tests` モジュールを追加。

| # | テスト名 | 検証内容 |
|---|---------|---------|
| 1 | `ui2_exec_summary_has_howto_guide` | Executive Summary に「このページの読み方」 |
| 2 | `ui2_exec_summary_has_kpi_v2_with_icon_and_compare` | kpi-card-v2 / kpi-icon / kpi-compare / 図 1-1 |
| 3 | `ui2_exec_summary_has_priority_badges` | priority-badge クラス |
| 4 | `ui2_salary_stats_has_summary_table_with_figure_no` | 表 3-1 + 給与統計サマリ |
| 5 | `ui2_salary_stats_has_iqr_shade_bar` | iqr-bar / iqr-shade + 図 3-1 |
| 6 | `ui2_salary_stats_has_outlier_removal_table` | 表 3-2 + 外れ値除外 + read-hint |
| 7 | `ui2_salary_stats_has_histogram_figure_numbers` | 図 3-2 / 図 3-3 |
| 8 | `ui2_scatter_has_regression_table_and_threshold_guide` | 図 5-1 / 表 5-1 + 0.5/0.3 閾値 |
| 9 | `ui2_scatter_has_correlation_not_causation_warning` | 相関 + 因果関係 |
| 10 | `ui2_region_has_heatmap` | heatmap-grid / heatmap-cell + 図 6-1 |
| 11 | `ui2_region_has_pref_table_figure_no` | 表 6-1 |
| 12 | `ui2_municipality_has_dup_marker` | 同名市区町村マーカー + 表 7-1 |
| 13 | `ui2_employment_has_dumbbell_chart` | dumbbell-list / dumbbell-row + 図 4-x |
| 14 | `ui2_min_wage_has_diff_bar` | minwage-diff-bar + 表 8-1 |
| 15 | `ui2_company_has_two_axis_visualization` | 表 9-1 |
| 16 | `ui2_tag_has_treemap_with_caption` | 図 10-1 / 表 10-1 |
| 17 | `ui2_multiple_read_hints_present` | read-hint-label が 4 箇所以上 |
| 18 | `ui2_figure_caption_total_count` | figure-caption が 10 箇所以上 |
| 19 | `ui2_kpi_values_consistent_with_legacy` | 旧 KPI と v2 KPI 両方 + 5 ラベル |
| 20 | `ui2_section_bridges_present` | section-bridge が 3 箇所以上 |
| 21 | `ui2_preserves_hw_data_scope_warning` | HW スコープ注意維持 |

**合計**: 21 件すべて pass。最低要件 (12 件) を超過達成。

---

## 8. memory ルール遵守状況

| ルール | 遵守 |
|--------|------|
| `feedback_correlation_not_causation.md` | ✓ Section 5 散布図に「相関≠因果」明記、「傾向がある」表現徹底 |
| `feedback_hw_data_scope.md` | ✓ Executive Summary の scope-note を維持。CSV/HW スコープ併記 |
| `feedback_test_data_validation.md` | ✓ contract test は具体値（伊達市 2 件、表 3-1 文言、図 1-1 等）で検証 |
| `feedback_e2e_chart_verification.md` | ✓ data-chart-config 属性を含むチャート要素の存在を検証 (既存テストで担保) |
| 禁止ワード | ✓ 「ランキング」「上位」「SalesNow」「最適」「すべき」等を一切追加していない |

---

## 9. 親セッションへの統合チェックリスト

- [x] 既存 781 テスト破壊なし（825 → 全 pass）
- [x] 公開 API シグネチャ変更なし（`render_section_*` 関数群）
- [x] 新規 contract test 12 件以上（実績: 21 件）
- [x] 図表番号 6 セクション全てに付与（21 図表）
- [x] 読み方吹き出し 6 セクション全てに付与（10+ 箇所）
- [x] memory ルール遵守（相関≠因果 / HW スコープ / 禁止ワード）
- [x] 印刷時の `page-break-inside: avoid` 適用
- [x] 並列 agent 競合回避（UI-3 が触る範囲: hw_enrichment / seeker / salesnow / notes / helpers の test 追加部 / style.rs の `.report-zebra` 等は競合せず、共通 CSS は新規クラス名で名前空間を分離）

### UI-3 との style.rs 共存

UI-3 が `style.rs` を編集する想定だが、UI-2 は **`r#"..."#.to_string()` ブロック末尾の `}` の直前** に新規クラスのみ追記。既存クラス名 (`.kpi-card`, `.stat-box`, `.zebra` 等) は変更せず、UI-2 専用は `.kpi-card-v2`, `.iqr-bar`, `.dumbbell-row`, `.heatmap-grid`, `.minwage-diff-bar`, `.section-bridge` 等で UI-3 と名前空間を分離。

### 既存テスト担保した重要文字列（不変）

- `Executive Summary`, `推奨優先アクション`, `サンプル件数`, `主要地域`, `主要雇用形態`, `給与中央値`, `新着比率`
- `平均月給`, `中央値`, `給与範囲`, `下限給与の分布`, `上限給与の分布`
- `相関分析（散布図）`, `R²`, `地域分析（都道府県）`, `地域分析（市区町村）`
- `雇用形態分布`, `雇用形態別 給与水準`, `最低賃金比較`, `企業分析`, `タグ×給与相関`
- 禁止ワードの非含有
- exec-kpi-grid / kpi-card / stat-box / sortable-table / section-sowhat 等 既存 CSS クラス

---

## 10. 既知の制約・今後の改善余地

- **95% 信頼区間バンド (Bootstrap)**: scatter.rs に解析的 CI を追加検討したが、既存 `regression_min_max` は `slope/intercept/r_squared` のみで CI 値を保持していない。aggregator.rs の改修が必要なため今回はスコープ外（要件 4-2 の brief「Bootstrap 由来 or 解析的」のうち、追加データ無しでは正確な CI が出せないため見送り。代わりに R² の色分けと閾値ガイドで「相関の確からしさ」を表現）。
- **賞与込み年収の比較表 (要件 4-3)**: aggregator.rs に賞与情報のフィールドが現状無いため、雇用形態別の比較は `EmpTypeSalary.avg_salary` (月給) と `EmpGroupNativeAgg` のネイティブ単位値で代替表現。賞与情報の取り込みは Fix-A の延長線で別タスクが妥当。
- **タグ×給与の wordcloud**: 既存実装の treemap で機能要件は満たすため、wordcloud は ECharts 標準機能ではなく外部依存となるため見送り。
