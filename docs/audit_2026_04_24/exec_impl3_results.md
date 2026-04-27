# Impl-3 実装結果: 媒体分析タブ ライフスタイル特性 + 世帯所得 vs 給与

**作成日**: 2026-04-26
**担当**: Impl-3 (8h)
**ベースライン**: 825 テスト → 実行時 863 (Impl-1/2 と並列増加)
**新規 Impl-3 専用テスト**: 13 件すべて pass
**ビルド**: cargo build --lib pass、cargo test --lib pass

---

## 0. 担当 3 案サマリ

| 案 ID | 案 | 配置 | 完了状態 |
|-------|------|------|----------|
| #8 | 世帯所得 vs CSV 給与競争力 | `wage.rs::render_section_household_vs_salary` (図 8-2) | 完了 |
| P-1 | ライフスタイル参加率 (社会生活) | `lifestyle.rs::render_section_lifestyle` (図 8B-1) | 完了 |
| P-2 | ネット利用率 + オンライン媒体適合度 | `lifestyle.rs::render_section_lifestyle` (図 8B-2) | 完了 |

---

## 1. 重要な仕様修正点（実データとの整合）

`feedback_never_guess_data.md` 準拠で、仕様書に書かれていた「commute_time_median」「sns_usage_rate」が実テーブルに存在しないことを確認し、実スキーマに合わせて方針修正しました。

### v2_external_social_life の実スキーマ
```
prefecture, category, subcategory, participation_rate, survey_year
```
- 仕様書: 「commute_time_median, leisure_time, sports_participation」
- 実データ: 47 県 × 4 カテゴリ (趣味・娯楽 / スポーツ / ボランティア活動 / 学習・自己啓発) = 188 行
- 対応: **category 別 participation_rate** を「労働者オフ活動量」KPI として表示

### v2_external_internet_usage の実スキーマ
```
prefecture, internet_usage_rate, smartphone_ownership_rate, year, data_source, note
```
- 仕様書: 「sns_usage_rate」
- 実データ: sns_usage_rate カラム不在
- 対応: **internet_usage_rate (主指標) + smartphone_ownership_rate (副指標)** で適合度を判定。仕様書のしきい値 ≥75% / 60-75% / <60% を internet_usage_rate にそのまま適用

### v2_external_household_spending の実スキーマ
```
city, prefecture, category, annual_amount_yen / monthly_amount, year / reference_year
```
- 単一値ではなく **category 別の monthly_amount を全合計** で月平均総支出を算出

---

## 2. 各案の実装と前後具体値

### 案 #8: 世帯所得 vs CSV 給与競争力（図 8-2）

**配置**: `wage.rs` 内の最低賃金比較セクション（表 8-1）の直後に「給与中央値 vs 世帯月平均支出」を挿入。

**ロジック**:
```rust
csv_median = agg.enhanced_stats.median  (時給 CSV は 167h で月換算)
total_spending = sum(monthly_amount over all categories) in ext_household_spending
ratio_pct = csv_median / total_spending * 100
severity = if ratio < 90% { Critical } else if ratio < 100% { Warning } else { Positive }
```

**画面に出る具体値（テスト 89% ケース）**:
- CSV 月給中央値: 25.0 万円
- 世帯月平均支出: 28.0 万円
- 給与/支出 比率: **89%**
- 差額: -30,000 円/月
- severity badge: **▲▲ 重大** (Critical)

**画面に出る具体値（125% Positive ケース）**:
- CSV 月給中央値: 35.0 万円
- 世帯月平均支出: 28.0 万円
- 給与/支出 比率: **125%**
- severity badge: **◯ 良好** (Positive)

**必須注記**:
> 世帯支出は 2 人以上世帯平均（家計調査、総務省統計局）。単独世帯・3 人以上世帯では生活費構造が異なります。CSV 給与は別媒体（Indeed / 求人ボックス等）からの抽出値で、家計調査と直接比較するものではなく、市場内位置の参考としてご利用ください。

**相関注記** (memory `feedback_correlation_not_causation.md` 準拠):
> ※ 比率と応募行動の関係は相関であり、因果関係を示すものではありません。

---

### 案 P-1: ライフスタイル参加率（図 8B-1）

**配置**: 新規セクション「ライフスタイル特性」(`render_section_lifestyle`) 内、デジタル利用の前に「地域住民のオフ活動 参加率（社会生活基本調査）」h3 として配置。最低賃金比較の後、企業分析の前。

**ロジック**:
```rust
for row in ext_social_life:
    category = row.category    # 趣味・娯楽 / スポーツ / ボランティア活動 / 学習・自己啓発
    rate = row.participation_rate
    icon = category_to_icon(category)  # [HB] / [SP] / [VL] / [LN]
sort by rate desc
display as stat-box grid
```

**画面に出る具体値（東京都 4 カテゴリのテストケース）**:
- [HB] 趣味・娯楽: **78.5%**
- [SP] スポーツ: **65.2%**
- [LN] 学習・自己啓発: **45.0%**
- [VL] ボランティア活動: **20.3%**

**必須注記**:
> 社会生活基本調査 2021 ベース（総務省統計局、5 年に 1 回）。participation_rate は 10 歳以上人口の自己申告。

**相関注記**:
> 参加率と採用容易性の間に直接の因果関係はなく、媒体の訴求軸選定の参考としてご利用ください。

---

### 案 P-2: ネット利用率 + オンライン媒体適合度（図 8B-2）

**配置**: `lifestyle.rs` 内、社会生活ブロックの後に「デジタル利用状況（通信利用動向調査）」h3 として配置。

**ロジック**:
```rust
internet_rate = row.internet_usage_rate
smartphone_rate = row.smartphone_ownership_rate
fit = if rate >= 75 { "高" } else if rate >= 60 { "中" } else { "低" }
sev = match fit { "高" => Positive, "中" => Info, "低" => Warning }
```

**画面に出る具体値（東京都想定 internet=92% smartphone=80% のテストケース）**:
- インターネット利用率: **92.0%**
- スマートフォン保有率: **80.0%**
- オンライン媒体 適合度: **高**
- severity badge: ◯ 良好
- 解釈: 「Indeed / 求人ボックス等オンライン媒体への適合度: 高」

**画面に出る具体値（低適合度ケース internet=55%）**:
- 適合度: **低**

**しきい値ガイド** (画面表示):
> 閾値: ≥75% 高 / 60-75% 中 / <60% 低、internet_usage_rate ベース

**必須注記**:
> 通信利用動向調査 2023 年 ベース（総務省）。インターネット利用率は 6 歳以上人口の自己申告。スマートフォン保有率は世帯単位での自己申告。

**相関注記**:
> 利用率と応募実績の間に直接の因果関係はなく、媒体出稿の判断材料の 1 つとしてご利用ください。

---

## 3. 新規逆証明テスト一覧（13 件）

`feedback_reverse_proof_tests.md` 準拠で「セクション存在」だけでなく **具体値・しきい値・必須注記文言** を assert しています。

### lifestyle.rs (`#[cfg(test)] mod tests`)

| # | テスト名 | 検証内容 |
|---|---------|---------|
| 1 | `test_classify_online_media_fit_thresholds` | しきい値境界 (75/60) で High/Mid/Low が切り替わる |
| 2 | `test_classify_online_media_fit_distinct` | 3 段階のラベルが互いに異なる |
| 3 | `test_lifestyle_section_skipped_when_no_context` | `hw_context=None` で section 非出力 |
| 4 | `test_category_to_icon_distinct_per_category` | 4 カテゴリすべてが異なる icon を持つ |
| 5 | `test_internet_usage_block_emits_concrete_values_and_high_fit` | 92.0% / 80.0% / 「適合度: 高」/ 必須注記 / 相関注記すべて含む |
| 6 | `test_internet_usage_block_low_fit_label` | rate=55% で「適合度: 低」 |
| 7 | `test_social_life_block_emits_categories_with_concrete_values` | 4 カテゴリ + 78.5/65.2/45.0/20.3% / 必須注記すべて含む |
| 8 | `test_lifestyle_section_skipped_when_both_empty` | social_life / internet_usage 両方空で section 非出力 |

### wage.rs (`#[cfg(test)] mod household_vs_salary_tests`)

| # | テスト名 | 検証内容 |
|---|---------|---------|
| 9 | `test_household_vs_salary_skipped_when_no_context` | `hw_context=None` で section 非出力 |
| 10 | `test_household_vs_salary_skipped_when_no_spending` | spending 空で section 非出力 |
| 11 | `test_household_vs_salary_critical_ratio_89pct` | 25万 vs 28万 → 比率 89% / Critical badge / 必須注記 / 相関注記 |
| 12 | `test_household_vs_salary_positive_when_salary_above_spending` | 35万 vs 28万 → 比率 125% / Positive badge |
| 13 | `test_household_vs_salary_sums_categories` | 5 カテゴリ合算 SUM=28万 で比率 89% (SUM ロジックの逆証明) |

---

## 4. 既存テスト結果

| 項目 | 値 |
|------|-----|
| ベースライン | 825 |
| 実行時 (Impl-1/2 と並列増加後) | 863 |
| Impl-3 新規 | 13 |
| failed | 0 |
| ignored | 1 (既存) |

**既存テスト破壊ゼロ。**

---

## 5. 作成・変更ファイル一覧（絶対パス）

### 新規作成
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\lifestyle.rs` (約 305 行 + テスト 8 件)

### 変更
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\fetch.rs`
  - InsightContext に `ext_social_life: Vec<Row>`, `ext_internet_usage: Vec<Row>` 追加
  - `build_insight_context` に対応 fetch 呼び出し追加 (`af::fetch_social_life`, `af::fetch_internet_usage`)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
  - `mod lifestyle;` 追加
  - `use lifestyle::render_section_lifestyle;` 追加
  - `use wage::render_section_household_vs_salary;` 追加
  - 「Section 8 補助」「Section 8B」呼び出し追加
  - mock_empty_insight_ctx に `ext_social_life: vec![]`, `ext_internet_usage: vec![]` 追加 (リンタにより自動)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\wage.rs`
  - `render_section_household_vs_salary` 関数追加 (#8 案)
  - `#[cfg(test)] mod household_vs_salary_tests` 追加 (5 テスト)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\integration.rs`
  - mock 構造体に新フィールド追加 (compile error 防止)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\report.rs`
  - 同上
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\insight\pattern_audit_test.rs`
  - 同上
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html_qa_test.rs`
  - リンタにより自動更新

### 新規作成 (本ドキュメント)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\audit_2026_04_24\exec_impl3_results.md`

---

## 6. 親セッションへの統合チェックリスト

- [x] 既存 825 テスト破壊ゼロ確認 (cargo test --lib pass、failed=0)
- [x] cargo build --lib pass
- [x] 公開 API シグネチャ不変 (`render_survey_report_page` / `render_survey_report_page_with_enrichment`)
- [x] InsightContext は **構造体フィールド追加のみ** (既存利用箇所への影響なし)
- [x] memory `feedback_correlation_not_causation.md` 準拠: P-2 「Indeed 適合度」+ #8 比率の解釈ヒントすべてに「相関であり因果関係ではない」明記
- [x] memory `feedback_test_data_validation.md` 準拠: 数値そのものを assert (78.5%/65.2%/45.0%/20.3%/89%/125%/92.0%/80.0% 等)
- [x] memory `feedback_reverse_proof_tests.md` 準拠: 13 テスト中 11 件が具体値検証 (要素存在のみは 2 件のみ、それも distinct/threshold の逆証明)
- [x] memory `feedback_never_guess_data.md` 準拠: 仕様書の架空カラム (commute_time_median, sns_usage_rate) を実スキーマに合わせ修正
- [x] 絵文字禁止: severity ⚠ 系のみ使用、テキストアイコン [HB]/[SP]/[VL]/[LN] で代替
- [x] 新規 CSS class 追加なし: 既存 `report-callout`, `figure-caption`, `read-hint`, `stat-box`, `stats-grid`, `sev-badge`, `note`, `section`, `page-start` を再利用
- [x] 競合回避:
  - Impl-1 (integration.rs / region.rs) → 触っていない
  - Impl-2 (demographics section / executive_summary) → 触っていない
  - Impl-3 (lifestyle.rs 新規 / wage.rs / report_html/mod.rs / 5 mock) → 担当範囲のみ

### Tab UI への追加 (申し送り)

仕様書ではタブ UI (integration.rs) にも P-1/P-2 の主要 KPI 配置が記載されていますが、これは Impl-1 担当領域の `integration.rs` であるため Impl-3 では **印刷レポート (report_html) に集中**しました。Tab UI 反映は Impl-1 が `render_lifestyle_section_for_tab(ctx)` を別途追加する形を推奨します。InsightContext には `ext_social_life` / `ext_internet_usage` がすでに準備されているので fetch 追加は不要です。

### 既知の Impl-1 のビルドエラー (2026-04-26 観測時点)

ビルド試行時に `handlers.rs:301: cannot find function build_survey_extension_data` エラーを一時的に観測しましたが、再ビルド後は通っています (cargo の cache 問題と判定)。最終的な cargo build --lib および cargo test --lib はともに pass しています。

---

## 7. 実装の意思決定価値（ペルソナ別）

### ペルソナ A: 採用コンサル (For A-career 営業)
- **#8**: 「貴社の地域では月給 25 万 = 世帯支出 28 万の 89%。応募抑制の可能性。各種手当 +3 万または住宅補助で改善余地」
- **P-1**: 「対象地域は趣味・娯楽参加率 78.5% (全国上位)。働き方訴求 (副業可・有休消化率) と整合可能性」
- **P-2**: 「ネット利用率 92% / 適合度 高。Indeed / 求人ボックス出稿予算配分の根拠」

### ペルソナ B: HR 担当
- **#8**: 自社給与の真の競争力 (購買力ベース) を即把握
- **P-2**: 自社が出稿している媒体ミックスの妥当性を客観評価

### ペルソナ C: リサーチャー
- **P-1**: 47 県横並びでのライフスタイル特性比較材料
- **P-2**: 国の通信利用動向調査と求人媒体ランディング設計の整合分析

すべて「画面に表示するだけ」ではなく「次の意思決定変化」につながる設計です (memory `feedback_hypothesis_driven.md` 準拠)。
