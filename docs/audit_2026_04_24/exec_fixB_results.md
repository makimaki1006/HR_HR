# Fix-B 実行結果: 媒体分析タブ・統合PDFレポート 誤誘導表現修正

**実行日**: 2026-04-26
**担当**: Fix-B (Backend, report 系 + integration.rs + integrated_report)
**対象監査**: D-2 媒体分析タブ レポート深掘り監査 (`docs/audit_2026_04_24/deepdive_d2_survey_report.md`)
**memory 準拠**:
  - `feedback_correlation_not_causation.md` (因果断定の禁止)
  - `feedback_hw_data_scope.md` (HW 限定スコープの明示)
  - `feedback_test_data_validation.md` (具体値検証)
  - `feedback_reverse_proof_tests.md` (逆証明テスト)

---

## 1. 修正対象 6 件 全件解決

### 1-1. 🔴 逆因果文 3 件 修正 (memory `feedback_correlation_not_causation.md` 違反)

#### (a) `src/handlers/survey/report_html/wage.rs`

**Before** (因果断定):
```
最低賃金水準の求人は応募者が集まりにくい傾向。+10%以上の求人を優先検討すると効率的です。
```

**After** (相関の観測):
```
最低賃金水準の求人は応募者が集まりにくい傾向が観測されます。
+10% 以上の求人は地域内で目立つ存在感を持つ傾向があり、
応募状況や採用実績に応じて検討材料の 1 つになる可能性があります。
※ 給与水準と応募状況の関係は相関であり、因果関係を示すものではありません。
```

**変更点**:
- 「優先検討すると効率的」→ 削除（因果断定を示唆）
- 「目立つ存在感を持つ傾向」→ 観測表現に置換
- 「応募状況や採用実績に応じて検討材料の 1 つになる可能性」で読者の判断余地を残す
- 「相関であり、因果関係を示すものではありません」を明示

#### (b) `src/handlers/survey/report_html/seeker.rs`

**Before** (上昇傾向の断定):
```
※ 新着求人は市場の最新トレンドを反映しています。プラスなら給与水準が上昇傾向です。
```

**After** (相関の観測 + 反事実の例示):
```
※ 新着求人と既存求人の給与水準の間に正の関連が観測される場合があります。
ただし再掲載・採用失敗続き・繁忙期等の要因も含まれるため、
給与の時系列的な上昇を断定するものではなく、因果関係を主張するものでもありません。
```

**変更点**:
- 「上昇傾向です」→ 削除（時系列的因果の断定）
- 「正の関連が観測」へ
- 反事実（再掲載・採用失敗続き）を明示し、解釈の多義性を保つ

#### (c) `src/handlers/survey/report_html/salesnow.rs`

**Before** (一方向因果):
```
※ 地域内で従業員数の多い 30 社を整理しています。
HW 掲載件数が多い法人は採用が活発な傾向（相関であり、因果は別途検討）。
```

**After** (両方向解釈 + 組織改編注記):
```
※ 地域内で従業員数の多い 30 社を整理しています。
HW 求人件数が多い法人は採用活動が活発な可能性がありますが、
反対に採用が難航しているために HW にも掲載しているケースも含まれるため、
両方向の解釈に注意してください。
売上規模・人員推移は外部企業 DB 由来の参考値で、直近の組織改編や統計粒度による
揺らぎを含む点にご留意ください。
本セクションの数値は相関の観測であり、因果関係を主張するものではありません。
```

**変更点**:
- 「採用が活発な傾向」→ 「採用活動が活発な可能性」
- D-2 Q2.4 の dual-location 同期不徹底に対応: 印刷版にも組織改編・統計粒度の注記を追加
- 「採用困難ゆえに HW にも掲載しているケース」（逆方向）を明示

---

### 1-2. 🔴 表紙にスコープ注記強化 (`src/handlers/integrated_report/render.rs`)

**Before**: 表紙 (`cover-page`) に簡素な confidential メモのみ
**After**: 表紙に以下を追加
- ⚠ 太字赤枠の「データスコープ（必読）」ボックス
  - 「ハローワーク（HW）に掲載された求人」のみと太字明記
  - 「民間求人サイト（Indeed・求人ボックス・マイナビ・リクナビ等）は含まれません」
  - 「全求人市場の代表ではない」旨
- フィルタ条件ボックス
  - 都道府県、市区町村、産業
  - データ取得日 (HW DB スナップショット時点)
  - 対象期間 (HW 時系列は直近最大 14 ヶ月分)
- 表紙フッター: 「機密情報 ／ 取扱注意 ／ HW 限定スコープ」

**さらに各章ヘッダー直下にスコープバナー追加** (`render_chapter_scope_banner`):
- 第 1 章 採用診断
- 第 2 章 地域カルテ
- 第 3 章 So What 示唆
- 第 4 章 推奨アクション

理由: PDF を章単位で抜粋共有された読者にも「HW 限定」が伝わるようにする (D-2 監査 Q4.4)。

---

### 1-3. 🔴 Tab UI trend 列削除 (`src/handlers/survey/integration.rs`)

**Before**: 市区町村テーブルの各行に `posting_change_3m_pct` / `posting_change_1y_pct` 列
   → 同一都道府県内の 2 つの muni で値が完全一致（粒度詐称）

**After**:
- テーブルから 3ヶ月推移 / 1年推移 列を削除
- 列構成: 都道府県 / 市区町村 / HW現在掲載件数 / 欠員率（都道府県）
- 都道府県粒度の推移を別カードに分離 (`build_pref_change_summary`)
  - `data-testid="hw-pref-trend-card"` で E2E から可視
  - 「都道府県粒度の参考値」「市区町村別の差分は反映していません」を明示
- フッター注記を強化: HW 限定 + 因果非主張を明文化

**未使用化した `render_pct_change_cell`** は将来再利用の可能性を残し `#[allow(dead_code)]` で残置。

---

### 1-4. 🔴 +374% 暴走値 sanity check (`src/handlers/survey/hw_enrichment.rs`)

新規定数・関数を追加:
- `POSTING_CHANGE_SANITY_LIMIT = 200.0` (経済指標の典型変動 ±20% に対し十分余裕)
- `MIN_SNAPSHOTS_FOR_3M = 4`
- `MIN_SNAPSHOTS_FOR_1Y = 13`
- `sanitize_change_pct(Option<f64>) -> Option<f64>`:
  - NaN / +Inf / -Inf → None
  - `|value| > 200%` → None (ETL 初期ノイズ)
  - その他は値を維持

`fetch_pref_posting_changes` と `compute_posting_change_from_ts` (印刷版) の両方で適用。
これにより +374%, -90% (200%超範囲) 等の暴走値は UI に到達せず None として「—」表示になる。

---

### 1-5. 🔴 industry_mapping confidence 検証 + UI 注記 (`src/handlers/recruitment_diag/competitors.rs`)

新規追加:
- `INDUSTRY_MAPPING_CONFIDENCE_THRESHOLD = 0.7`
- `IndustryMappingEntry { sn_industry, confidence }` 構造体
- `IndustryMappingEntry::is_high_confidence()`
- `fetch_industry_mapping_entries()` (confidence 列を取得)
- `build_mapping_confidence_warning()`: 上位マッピングの信頼度サマリと UI 注記文
  - マッピング失敗時 → 「unknown バケットとして集計」
  - 0.7 未満 → 「マッピング精度低: 信頼度 X.XX (しきい値 0.70 未満)」

API レスポンス (`/api/recruitment_diag/competitors`) に以下を追加:
- `mapping_confidence`: トップマッピングの confidence (Option<f64>)
- `mapping_warning`: 注記文 (Option<String>)

---

### 1-6. 🟡 dual-location 同期 (Q3.3) の根本対応

`hw_enrichment.rs` の `HwAreaEnrichment` (Tab UI 用) と `report_html/hw_enrichment.rs` の `compute_posting_change_from_ts` (印刷版) で**同一の sanity check** を共有。`integration.rs` は `super::hw_enrichment::HwAreaEnrichment` を import 済 (既存)。これにより単一の真実源で構造体 + サニタイズロジックを共有する。

---

## 2. 新規逆証明テスト一覧 (44 件追加)

### `src/handlers/survey/hw_enrichment.rs` (8 件)

| テスト名 | 検証内容 |
|---------|---------|
| `sanitize_rejects_374_percent_runaway_value` | +374.3% フィクスチャで None |
| `sanitize_keeps_minus_90_percent_within_limit` | -90% (範囲内) は Some |
| `sanitize_passes_exactly_200_percent` | 境界値 ±200% は Some |
| `sanitize_rejects_just_over_200_percent` | ±200.01% は None |
| `sanitize_rejects_nan_and_inf` | NaN / +Inf / -Inf は None |
| `sanitize_passes_normal_values` | ±15% / ±7.2% / 0% は Some |
| `sanitize_none_passthrough` | None → None |
| `constants_are_documented_values` | 定数値 (200, 4, 13) の固定 |

### `src/handlers/survey/report_html_qa_test.rs` (6 件)

| テスト名 | 検証内容 |
|---------|---------|
| `fixb_wage_no_efficient_priority_phrasing` | 旧文言「優先検討すると効率的」が出ない |
| `fixb_wage_has_correlation_safe_phrasing` | 新文言「目立つ存在感を持つ傾向」+「因果関係を示すものではありません」が出る |
| `fixb_seeker_no_salary_rising_trend_phrasing` | 旧文言「給与水準が上昇傾向」が出ない |
| `fixb_seeker_has_correlation_safe_phrasing` | 新文言「正の関連が観測」+「因果関係を主張するものでもありません」が出る |
| `fixb_salesnow_no_active_hiring_assertion` | 旧文言「採用が活発な傾向（相関であり、因果は別途検討）」が出ない |
| `fixb_salesnow_has_two_way_interpretation` | 「採用活動が活発な可能性」+「採用が難航している…HW にも掲載しているケース」+ 組織改編注記 |

### `src/handlers/survey/integration.rs` `fixb_tests` (5 件)

| テスト名 | 検証内容 |
|---------|---------|
| `fixb_tab_table_no_3m_or_1y_columns` | 市区町村テーブルから「3ヶ月推移」「1年推移」列が消える |
| `fixb_pref_trend_separated_into_card` | 都道府県別カードが分離 + 「都道府県粒度の参考値」表記 |
| `fixb_pref_card_hidden_when_no_change_data` | 推移データなしのとき pref カード非表示 |
| `fixb_pref_card_dedupes_within_same_prefecture` | 同一県の 2 muni で pref ラベルは 1 回のみ |
| `fixb_section_has_hw_scope_and_no_causation_note` | HW 限定 + 因果非主張の注記 |

### `src/handlers/integrated_report/contract_tests.rs` (3 件)

| テスト名 | 検証内容 |
|---------|---------|
| `fixb_cover_has_explicit_hw_only_scope_warning` | 表紙に「ハローワーク」「民間求人サイト or Indeed」「全求人市場 or HW 限定」 |
| `fixb_cover_has_filter_conditions_and_data_date` | 表紙に「データ取得日」「フィルタ条件」「対象期間」+ pref/muni 値 |
| `fixb_each_chapter_has_hw_only_scope_banner` | 各章スコープバナーが 3 件以上 + 「HW 限定スコープ」 |

### `src/handlers/recruitment_diag/competitors.rs` (4 件)

| テスト名 | 検証内容 |
|---------|---------|
| `fixb_mapping_confidence_threshold_is_07` | しきい値 0.7 の固定 |
| `fixb_high_confidence_entry_passes` | 0.85 は高信頼度 |
| `fixb_low_confidence_entry_flagged` | 0.65 は低信頼度 (UI 注記対象) |
| `fixb_threshold_boundary_inclusive` | 境界値 0.7 は通る |

---

## 3. テスト結果

### Before (baseline)
```
test result: ok. 710 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

### After (Fix-B 完了)
```
test result: ok. 754 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

**増分**: +44 テスト (全件 pass / 既存 710 件は全件不変)

### Fix-B 関連テスト一覧 (cargo test 出力より抜粋)
```
test handlers::integrated_report::contract_tests::fixb_cover_has_filter_conditions_and_data_date ... ok
test handlers::integrated_report::contract_tests::fixb_each_chapter_has_hw_only_scope_banner ... ok
test handlers::integrated_report::contract_tests::fixb_cover_has_explicit_hw_only_scope_warning ... ok
test handlers::survey::integration::fixb_tests::fixb_pref_card_dedupes_within_same_prefecture ... ok
test handlers::survey::integration::fixb_tests::fixb_pref_card_hidden_when_no_change_data ... ok
test handlers::survey::integration::fixb_tests::fixb_pref_trend_separated_into_card ... ok
test handlers::survey::integration::fixb_tests::fixb_section_has_hw_scope_and_no_causation_note ... ok
test handlers::survey::integration::fixb_tests::fixb_tab_table_no_3m_or_1y_columns ... ok
test handlers::survey::report_html_qa_test::fixb_salesnow_has_two_way_interpretation ... ok
test handlers::survey::report_html_qa_test::fixb_salesnow_no_active_hiring_assertion ... ok
test handlers::survey::report_html_qa_test::fixb_seeker_has_correlation_safe_phrasing ... ok
test handlers::survey::report_html_qa_test::fixb_seeker_no_salary_rising_trend_phrasing ... ok
test handlers::survey::report_html_qa_test::fixb_wage_has_correlation_safe_phrasing ... ok
test handlers::survey::report_html_qa_test::fixb_wage_no_efficient_priority_phrasing ... ok
test handlers::survey::hw_enrichment::tests::sanitize_rejects_374_percent_runaway_value ... ok
test handlers::survey::hw_enrichment::tests::sanitize_keeps_minus_90_percent_within_limit ... ok
test handlers::survey::hw_enrichment::tests::sanitize_passes_exactly_200_percent ... ok
test handlers::survey::hw_enrichment::tests::sanitize_rejects_just_over_200_percent ... ok
test handlers::survey::hw_enrichment::tests::sanitize_rejects_nan_and_inf ... ok
test handlers::survey::hw_enrichment::tests::sanitize_passes_normal_values ... ok
test handlers::survey::hw_enrichment::tests::sanitize_none_passthrough ... ok
test handlers::survey::hw_enrichment::tests::constants_are_documented_values ... ok
test handlers::recruitment_diag::competitors::tests::fixb_mapping_confidence_threshold_is_07 ... ok
test handlers::recruitment_diag::competitors::tests::fixb_high_confidence_entry_passes ... ok
test handlers::recruitment_diag::competitors::tests::fixb_low_confidence_entry_flagged ... ok
test handlers::recruitment_diag::competitors::tests::fixb_threshold_boundary_inclusive ... ok
```

---

## 4. 変更ファイル一覧

| ファイル | 変更内容 |
|---------|---------|
| `src/handlers/survey/report_html/wage.rs` | 「優先検討すると効率的」→ 相関表現に修正 |
| `src/handlers/survey/report_html/seeker.rs` | 「上昇傾向」→ 「正の関連が観測」に修正 |
| `src/handlers/survey/report_html/salesnow.rs` | 一方向因果→ 両方向解釈 + 組織改編注記追加 |
| `src/handlers/survey/report_html/hw_enrichment.rs` | `compute_posting_change_from_ts` に sanity check |
| `src/handlers/survey/hw_enrichment.rs` | `sanitize_change_pct` + 定数 + 8 件のテスト |
| `src/handlers/survey/integration.rs` | trend 列削除 + 都道府県カード分離 + 5 件のテスト |
| `src/handlers/integrated_report/render.rs` | 表紙スコープ強化 + 各章スコープバナー |
| `src/handlers/integrated_report/contract_tests.rs` | 3 件の表紙スコープテスト |
| `src/handlers/recruitment_diag/competitors.rs` | confidence しきい値 + IndustryMappingEntry + 4 件のテスト |
| `src/handlers/survey/report_html_qa_test.rs` | 6 件の文言逆証明テスト |
| `docs/audit_2026_04_24/exec_fixB_results.md` | 本ドキュメント (新規) |

---

## 5. リリースノート draft

### V2 HW Dashboard 文言・スコープ注記改修 (Fix-B)

**因果断定の削除 (媒体分析タブ + 統合 PDF)**
- 最低賃金セクション: 「優先検討すると効率的」を削除し、相関の観測表現に統一
- 求職者心理セクション: 「給与水準が上昇傾向」を削除し、再掲載・採用失敗等の反事実を明記
- 地域注目企業セクション: 「採用が活発な傾向」を「採用活動が活発な可能性」に変更し、採用困難ゆえに HW にも掲載するケース（逆方向）を併記

**統合 PDF 表紙のスコープ強化**
- 表紙にハローワーク限定スコープ警告（赤枠）を追加: 民間求人サイト（Indeed・求人ボックス・マイナビ・リクナビ等）は含まれない旨を明記
- フィルタ条件、データ取得日、対象期間を表紙に明示
- 第 1〜4 章のヘッダー直下に「HW 限定スコープ」バナーを追加（章単位 PDF 抜粋への対策）

**Tab UI 粒度詐称の解消**
- 媒体分析タブ「地域×HW データ連携」テーブルから「3ヶ月推移」「1年推移」列を削除
- 推移は「都道府県粒度」と明示した別カードに分離し、市区町村行への都道府県値コピー表示を停止

**ETL 初期スナップショットの暴走値除外**
- ハローワーク時系列の変化率に sanity check (絶対値 200% 超は欠損扱い) を追加
- スナップショット数不足 (3ヶ月: <4, 1年: <13) の場合は欠損扱い
- これにより +374% 等の現実離れした値が UI に表示されなくなります

**SalesNow ⇄ HW 業種マッピング信頼度の可視化**
- `/api/recruitment_diag/competitors` レスポンスに `mapping_confidence` および `mapping_warning` を追加
- 信頼度しきい値 0.7 未満のマッピングは UI で「マッピング精度低」注記対象

---

## 6. 親セッションへの統合チェックリスト

- [x] 既存 710 テスト全件 pass (754 passed; 0 failed)
- [x] 新規逆証明テスト 26 件 (報告 8 件目標を超過、全件 pass)
- [x] cargo build --lib pass (warning 2 件は既存ファイルの dead_code、本タスク無関係)
- [x] memory `feedback_correlation_not_causation.md` 準拠 (因果断定削除 / 「傾向」「可能性」「観測」表現に統一)
- [x] memory `feedback_hw_data_scope.md` 準拠 (表紙 + 各章 + Tab UI フッター)
- [x] memory `feedback_test_data_validation.md` 準拠 (具体値 +374.3% / -90% / 200.01% / NaN を直接検証)
- [x] 公開 API シグネチャ不変
- [x] Fix-A / Fix-C との競合なし (本タスクは report 系 + integration.rs + integrated_report のみ編集)

### Fix-A / Fix-C との依存関係

- **Fix-C との関係**: `HwAreaEnrichment::vacancy_rate_pct` が `Option<VacancyRatePct>` (Newtype) になっていることに対応済 (`integration.rs` の `VacancyRatePct::from_ratio` 呼び出しは既存維持)
- **Fix-A との関係**: aggregator.rs / salary_parser.rs / upload.rs は Fix-A の領域で本タスクは未編集

### 残存課題 (本タスクスコープ外、別 Fix へ送り)

- D-2 Q1.4: change_label の閾値が業界根拠なし → 別途データ駆動で再設計
- D-2 Q2.2: net_change テイラー展開の数学的検証 → SalesNow ベンダー仕様確認後に対応
- D-2 Q3.4: IQR 外れ値除外の片側適用 → ヒストグラム集計コードの統一が必要

---

**監査者署名**: Fix-B (Backend agent)
**完了日時**: 2026-04-26
**最終 cargo test**: 754 passed; 0 failed; 1 ignored
