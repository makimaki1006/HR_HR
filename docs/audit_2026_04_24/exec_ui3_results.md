# UI-3 実行結果レポート — 媒体分析印刷レポート 残 sections + 凡例/用語/style 強化

実行日: 2026-04-26
担当範囲: HW 連携 / 求職者 / SalesNow / フッタ + 共通 style + 用語 tooltip + 凡例システム
最終 cargo test 結果: **781 → 804（+23 件）/ 0 failed / 1 ignored**
うち UI-3 由来: 12 件（helpers 単体 9 件 + section 統合 3 件）

---

## 1. 追加した helper 関数 (helpers.rs)

すべて `pub(crate)` で公開し、UI-2 / UI-3 のいずれからも利用可能。

| 関数 | 用途 | 戻り値 |
|------|------|--------|
| `render_info_tooltip(label, description)` | 用語 tooltip（ⓘ + abbr 付き） | `String` |
| `render_legend_emoji(severity, text)` | 凡例 emoji + テキスト | `String` |
| `render_figure_number(chapter, num, title)` | 図番号「図 X-Y: ...」 | `String` |
| `render_table_number(chapter, num, title)` | 表番号「表 X-Y: ...」 | `String` |
| `render_reading_callout(text)` | 「読み方」吹き出し（💡 + role=note） | `String` |
| `render_severity_badge(severity)` | 🔴/🟡/🟢 + テキストバッジ | `String` |

加えて、`enum ReportSeverity { Critical, Warning, Info }` を `pub(crate)` で定義し、emoji / aria_label / class / action_text の 4 メソッドを提供。

---

## 2. 追加した CSS classes (style.rs)

すべて `report-*` 名前空間で UI-2 の `figure-caption` / `read-hint` 系と衝突回避。

### コンポーネント系
- `.report-figure-num` / `.report-table-num` — 図表番号
- `.report-tooltip` / `.report-tooltip-icon` — 用語 tooltip（abbr ベース）
- `.report-legend` / `.report-legend-emoji` / `.report-legend-text` — 凡例
- `.report-callout` / `.report-callout-icon` / `.report-callout-label` / `.report-callout-body` — 読み方吹き出し
- `.report-severity-badge` / `.report-severity-emoji` / `.report-severity-text` — 重要度バッジ

### 配色 / ステータス系
- `.report-sev-critical` / `.report-sev-warning` / `.report-sev-info`（light + dark）
- `.report-banner-amber` / `.report-banner-gray` — 注釈バナー（light + dark）
- `.report-gap-supply-shortage` / `.report-gap-demand-shortage` / `.report-gap-balanced` — 需給ギャップ色分け

### レイアウト / 印刷 / その他
- `.report-zebra` — テーブル zebra + hover highlight
- `.report-page-break-avoid` — 改ページ回避
- `.report-section-divider` — 章区切り
- `.report-notes-leadin` — notes 冒頭サマリ
- `.report-notes-category` + 5 種類のカテゴリ別配色（cat-data / cat-scope / cat-method / cat-corr / cat-update、light + dark）
- `.report-venn` / `.report-venn-circle` / `.report-venn-csv|hw|both` — CSV/HW スコープ概念図
- `.report-sparkline` — sparkline コンテナ（90×24px）
- `.report-mini-bar` / `.report-mini-bar-neg` — テーブル内 mini bar
- `@media print` ブロックで page-break-inside:avoid を集約

---

## 3. 各 section の Before / After

### 第3章 hw_enrichment.rs

| 項目 | Before | After |
|------|--------|-------|
| 見出し | `地域 × HW データ連携` | `第3章 地域 × HW データ連携` |
| 粒度制約 | section-sowhat 内に短文 | **amber バナー** (`report-banner-amber`) で「データ粒度の制約」を明示 |
| 概念図 | なし | **CSV / 重複領域 / HW の 3 円 Venn 概念図** + 読み方吹き出し |
| 図表番号 | なし | 図 3-1 / 表 3-1 |
| 用語 | 「欠員補充率」生のまま | `render_info_tooltip("欠員補充率", "...e-Stat 雇用動向調査由来...")` |
| 表 | hw-enrichment-table | + `report-zebra` で stripe + hover |

### 第4章 seeker.rs

| 項目 | Before | After |
|------|--------|-------|
| 見出し | `求職者心理分析` | `第4章 求職者心理分析` |
| 解釈ガイド | 各 note 内の小注記 | **章冒頭 gray バナー**で「相関≠因果」を強調 |
| 図表番号 | なし | 図 4-1 / 4-2 / 4-3（給与レンジ・未経験ペナルティ・新着プレミアム） |
| 読み方 | `<p class="note">` | `render_reading_callout()` で 💡 + 構造化 |

### 第5章 salesnow.rs

| 項目 | Before | After |
|------|--------|-------|
| 見出し | `地域注目企業` | `第5章 地域注目企業` |
| 組織改編注記 | section-sowhat 内 | **gray バナー** (`report-banner-gray`) で 🔄 アイコン + 視覚強調 |
| 凡例 | なし | 採用活動度 高/中/低 を 🔴/🟡/🟢 凡例で提示 |
| 採用活動度列 | なし | **新規列**: log1p(HW件数) + max(1年人員推移,0)×0.5 のスコア + テーブル内 normalized mini bar |
| 表番号 | なし | 表 5-1（「ランキング」「上位」は禁止ワードのため「一覧（従業員数の多い 30 社）」表記） |
| zebra | なし | `report-zebra` 適用 |

### 第6章 notes.rs

| 項目 | Before | After |
|------|--------|-------|
| 見出し | `注記・出典・免責` | `第6章 注記・出典・免責` |
| 構造 | フラットな `<ol>` 7 項目 | **5 カテゴリ別ボックス** + 末尾 `<ol>` 互換項目 + メタ情報フッタ |
| カテゴリ | なし | 📊 データソース / ⚠️ スコープ制約 / 🔬 統計手法 / 📐 相関≠因果 / 🔄 更新頻度 |
| 用語 tooltip | なし | IQR / Bootstrap 95% CI / Trimmed mean / 月給換算 167h / 欠員補充率 / 中央値 / 平均 — **7 用語**を tooltip 化 |
| 冒頭サマリ | なし | 「本レポートを正しく読むための前提」リード文（report-notes-leadin） |
| メタ情報 | 単純行 | gray バナーで「生成日時 / フォーマット v2 / レンダリング元」 |

---

## 4. 用語 tooltip システム設計

### 4.1 実装パターン

```html
<span class="report-tooltip" data-term-tooltip="1">
  <abbr title="...説明文..." tabindex="0" aria-label="IQR: ...説明文...">IQR</abbr>
  <span class="report-tooltip-icon" role="tooltip" aria-hidden="true">ⓘ</span>
</span>
```

### 4.2 a11y 対応

- `<abbr>` の `title` 属性により hover で説明文が表示される（標準ブラウザ機能）
- `tabindex="0"` でキーボードフォーカス可能
- `aria-label` に「ラベル: 説明」形式で全文を埋め込み、スクリーンリーダーで連続読み上げ
- `role="tooltip"` をアイコン span に付与し、補助技術での識別を可能にする
- 印刷時も点線下線を維持（`@media print` で `text-decoration: underline dotted #666` を強制）

### 4.3 用語登録一覧（notes.rs に集約）

| 用語 | 説明 |
|------|------|
| IQR | 四分位範囲 (Inter-Quartile Range)。Tukey 1977 由来の外れ値除外法 |
| Bootstrap 95% CI | 復元抽出による 95% 信頼区間。母集団分布を仮定しない |
| Trimmed mean | 刈り込み平均。両端の指定割合を除外して平均を取る |
| 月給換算 167h | 月平均所定労働時間 167h（厚生労働省ガイドライン）で時給→月給換算 |
| 欠員補充率 | e-Stat 雇用動向調査由来。新規拡大採用と区別される |
| 中央値 | 大小順並びの中央値。外れ値耐性あり |
| 平均 | 算術平均。総和情報を反映、外れ値の影響を受けやすい |

---

## 5. テスト結果

```
test result: ok. 804 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

### UI-3 の追加 contract test（12 件）

helpers.rs::ui3_helpers_tests（9 件）:
- `test_render_info_tooltip_contains_required_attrs`
- `test_render_info_tooltip_escapes_html`
- `test_render_legend_emoji_all_severities`
- `test_render_figure_number_format`
- `test_render_table_number_format`
- `test_render_reading_callout_a11y`
- `test_render_severity_badge_critical`
- `test_render_severity_badge_warning_info`
- `test_severity_distinct_outputs`

mod.rs::tests（UI-3 統合 3 件）:
- `ui3_notes_section_has_categorized_boxes`
- `ui3_seeker_section_has_chapter_4_and_guidance`
- `ui3_a11y_attributes_present`

逆証明テスト方針（`feedback_reverse_proof_tests.md` 準拠）:
- HTML 文字列の生成結果から **属性値・class 名・絵文字 codepoint・aria-label の具体値**を assert
- 「要素存在のみ」ではなく「該当属性で正しい意味付けがされていること」を検証
- `test_severity_distinct_outputs` は 3 値が確実に異なる値を返すことを sort+dedup で逆証明

---

## 6. memory ルール遵守確認

| ルール | 遵守内容 |
|--------|---------|
| `feedback_correlation_not_causation.md` | seeker 章冒頭 gray バナー / salesnow gray バナー / notes 第4カテゴリで「相関≠因果」を明示。「上昇傾向」「採用活発」等の因果断定文言を回避 |
| `feedback_hw_data_scope.md` | hw_enrichment amber バナーで「ts_turso_counts / 外部統計の都道府県粒度のみ」「市区町村単位の差は反映されない」を明示。notes スコープ制約カテゴリで「HW は掲載求人のみ。全求人市場ではない」を強調 |
| `feedback_test_data_validation.md` | 12 件の UI-3 テストすべてで具体値（codepoint U+1F534 / class 名 / data 属性値 / aria-label 文字列）を assert。「要素存在のみ」のテストは 0 件 |

---

## 7. UI-2 との連携ポイント

UI-2 が並行で `report_html/{executive_summary, salary_stats, scatter, region, employment, wage}.rs` を強化している。UI-3 が追加した resources は以下の通り、UI-2 から自由に利用可能：

### 7.1 利用可能 helper 関数（pub(crate)）

```rust
use super::helpers::{
    render_info_tooltip,        // ⓘ tooltip
    render_legend_emoji,        // 🔴🟡🟢 凡例
    render_figure_number,       // 図 X-Y
    render_table_number,        // 表 X-Y
    render_reading_callout,     // 💡 読み方
    render_severity_badge,      // 重要度バッジ
    ReportSeverity,             // enum
};
```

### 7.2 利用可能 CSS classes

UI-2 が新規追加した `.figure-caption` / `.read-hint` / `.section-howto` / `.kpi-card-v2` 等と機能重複しない `report-*` 名前空間。UI-2 は以下を自由に使える:

- 用語の説明文を加えたい時 → `render_info_tooltip()` を call、CSS は `.report-tooltip*` が自動適用
- 章番号を付けたい時 → `render_figure_number(chapter, num, title)`
- 「相関≠因果」「データ制約」を視覚強調したい時 → `<div class="report-banner-amber">` または `report-banner-gray` を直接埋め込み
- テーブル zebra → `<table class="data-table report-zebra">` のように複合 class

### 7.3 衝突回避ルール

- UI-2: `figure-caption` / `read-hint` / `section-howto` / `kpi-card-v2` / `priority-badge` / `iqr-bar` / `dumbbell-row` / `heatmap-grid` / `minwage-diff-bar` / `section-bridge`
- UI-3: `report-figure-num` / `report-tooltip*` / `report-legend*` / `report-callout*` / `report-severity-*` / `report-sev-*` / `report-banner-*` / `report-zebra` / `report-notes-*` / `report-venn*` / `report-sparkline` / `report-mini-bar*` / `report-section-divider` / `report-page-break-avoid`

両者の名前空間が完全に分離され、同一 HTML への両者出力は衝突しない。

---

## 8. 親セッションへの統合チェックリスト

- [x] 既存 781 テスト破壊なし → 781 → 804 (+23 件、すべて新規追加)
- [x] 公開 API シグネチャ不変 → `render_section_*` 系は引数も戻り値も変更なし
- [x] ビルド成功（warning は元から残存していた dead_code のみ、UI-3 起因 0 件）
- [x] memory ルール 3 種すべて遵守
- [x] 新規 contract test 10 件以上（要件 10 件 → 実装 12 件）
- [x] UI-2 と CSS / helper の名前空間衝突なし
- [x] 並列 agent の責務範囲外（UI-1 の render.rs / UI-2 の scatter.rs 等）には変更を加えていない
- [x] 禁止ワード（ランキング / 上位 / TOP / SalesNow ラベル）を全て除去（テスト pass で確認）

---

## 9. 変更ファイル一覧（絶対パス）

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\helpers.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\style.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\hw_enrichment.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\seeker.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\salesnow.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\notes.rs`
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`（UI-3 統合 contract test 3 件追加のみ）
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_ui3_results.md`（本ドキュメント / 新規）
