# Granularity 実装結果 (2026-04-26)

## ユーザー指摘
> 都道府県単位の集計データはあまり参考にならない

媒体分析タブの主役は CSV に登場する **市区町村** であり、47 都道府県全部ではない。
**CSV に登場する地域に絞った市区町村粒度可視化** が core value。

## 実装サマリ

| 項目 | Before | After |
|---|---|---|
| 統合レポートのレーダー | CSV 件数 上位 3 **都道府県** | CSV 件数 上位 3 **市区町村** (主役 / pink/green/blue 配色) |
| ヒートマップ | 47 県 grid | CSV 件数 Top 30 **市区町村** rose-500 グラデーション |
| 印刷レポート デモグラフィック | 都道府県粒度 (`muni=""` 強制) | 市区町村別カード追加 (Top 5、KPI 4 種) |
| lifestyle (P-1, P-2) 注記 | 通常 grey 注記 | **オレンジ強調**「都道府県粒度のみ」警告 |
| wage 世帯所得 (#8) 注記 | 通常 grey 注記 | **オレンジ強調**「都道府県粒度+政令市」警告 |
| wage 最低賃金 注記 | なし | 「47 県粒度のみ・市区町村差なし」明示 |

## データ粒度マトリクス

実装時点の schema 確認結果:

| テーブル | 市区町村粒度 | 利用方針 |
|---|---|---|
| v2_external_population_pyramid | OK | 主要市区町村別ピラミッド |
| v2_external_labor_force | OK | 失業者・労働力 |
| v2_external_education_facilities | OK | 教育施設密度 |
| v2_external_population | OK | 高齢化率 / 生産年齢比率 |
| v2_external_geography | OK | 可住地密度 |
| v2_region_benchmark | OK (muni 列) | 6 軸ベンチマーク主役 |
| v2_external_education | NG | 都道府県値 fallback (`is_education_pref_fallback=true`) |
| v2_external_industry_structure | NG | 都道府県値のみ (注記既存) |
| v2_external_household_spending | NG | 注記強化 (オレンジ) |
| v2_external_social_life | NG | 注記強化 (オレンジ) |
| v2_external_internet_usage | NG | 注記強化 (オレンジ) |
| v2_external_minimum_wage | NG | 47 県のみ (注記強化) |
| ts_turso_counts | NG | Turso 容量待ち (5/1) |

## 変更ファイル一覧

### 新規作成
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\granularity.rs`
  - `top_municipalities(agg, n)` — CSV 件数 Top N 市区町村抽出
  - `MunicipalityDemographics` 構造体 + helper メソッド (aging_rate / working_age_rate / estimated_unemployed / total_facilities)
  - `fetch_municipality_demographics(db, turso, top_munis)` — 既存 fetch ヘルパーを使い市区町村別データ取得
  - `fetch_region_benchmarks_for_municipalities(db, top_munis)` — fallback ロジック付きベンチマーク取得

### 既存ファイル修正
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\mod.rs`
  - `pub mod granularity;` 追加
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\handlers.rs`
  - `integrate_report` 内で `top_munis_3 / top_munis_30` 計算追加
  - `survey_report_html` 内で `municipality_demographics` 取得追加 → `render_survey_report_page_with_municipalities` 呼び出し
  - `build_survey_extension_data` シグネチャ拡張 (`+top_munis_3, +top_munis_30`)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\integration.rs`
  - `SurveyExtensionData` に `top3_municipality_benchmark` (Vec<(String, Row, bool)>) と `top_municipalities_heatmap` 追加
  - `render_municipality_benchmark_radar_section` 新規 (主役レーダー)
  - `render_municipality_heatmap_section` 新規 (CSV 件数 Top 30 ヒートマップ)
  - `render_integration_with_ext` で市区町村ヒートマップ → 市区町村レーダー → 都道府県レーダー (抑制) の順
  - 12 件の新規 contract test 追加
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
  - `render_survey_report_page_with_municipalities` 新規エントリ
  - 旧 `_with_enrichment` は `_with_municipalities(&[])` を呼ぶ wrapper に
  - Section 3D-M 追加 (主要市区町村別 デモグラフィック)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\demographics.rs`
  - `render_section_demographics_by_municipality` 新規 (5 件の contract test 追加)
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\lifestyle.rs`
  - social_life / internet_usage 注記強化 (オレンジ枠 + 「都道府県粒度の参考値」明示) + 2 件 contract test
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\wage.rs`
  - 世帯支出 (#8) と 最低賃金 注記強化 + 1 件 contract test

## fetch 関数の前後 schema 比較

既存 fetch 関数のシグネチャ変更は **ゼロ** (後方互換維持)。
新規 helper を `granularity.rs` に追加し既存 fetch を再利用:

| 関数 | 既存シグネチャ | 利用方法 |
|---|---|---|
| `fetch_population_pyramid` | `(db, turso, pref, muni)` | muni に Top N 市区町村名を順次渡す |
| `fetch_labor_force` | `(db, turso, pref, muni)` | 同上 |
| `fetch_education_facilities` | `(db, turso, pref, muni)` | 同上 |
| `fetch_population_data` | `(db, turso, pref, muni)` | 同上 |
| `fetch_geography` | `(db, turso, pref, muni)` | 同上 |
| `fetch_region_benchmark` | `(db, pref, muni)` | 市区町村粒度試行 → fallback で `(pref, "")` |
| `fetch_education` | `(db, turso, pref)` | **市区町村未対応**: 都道府県値固定で `is_education_pref_fallback=true` |

## UI Before/After

### 統合レポート (`render_integration_with_ext`)

**Before (Impl-1 時点)**:
```
HW統合分析 [対象地域]
↓
ハローワーク求人市場
地域×HW データ連携
外部統計
[Impl-1 #6] 主要地域 6 軸ベンチマーク (都道府県 上位 3)
[Impl-1 #18] 地理 / 高齢化率 KPI
[Impl-1 D-3] 産業構成 Top10 (都道府県)
企業データ
insight 示唆
```

**After (2026-04-26 Granularity)**:
```
HW統合分析 [対象地域]
↓
ハローワーク求人市場
地域×HW データ連携
外部統計
[NEW] 主要市区町村ヒートマップ (CSV 件数 Top 30) ← 媒体分析の主役
[NEW] 主要都市 6 軸ベンチマーク (市区町村 上位 3, fallback 注記付) ← 主役
[Impl-1 #18] 地理 / 高齢化率 KPI
[Impl-1 D-3] 産業構成 Top10 (都道府県、注記既存)
企業データ
insight 示唆

※ top3_municipality_benchmark がある場合、都道府県レーダーは冗長回避のため抑制
```

### 印刷レポート (`render_survey_report_page_with_municipalities`)

**Before**:
```
... Section 3D: 人材デモグラフィック (都道府県粒度、muni="" ハードコード)
    - 年齢ピラミッド (都道府県)
    - 学歴分布 (都道府県)
    - 失業者 KPI (都道府県)
    - 教育施設 (都道府県)
```

**After**:
```
... Section 3D: 人材デモグラフィック (都道府県粒度、既存維持)
... Section 3D-M: 主要市区町村別 人材デモグラフィック (NEW)
    - CSV 件数 Top 5 市区町村のカードグリッド
    - 各カード: 市区町村名 / CSV 件数 / 高齢化率 / 生産年齢比率 / 失業者 / 教育施設
    - 学歴は schema 都道府県粒度のみのため別 section 参照と注記
```

## 既存テスト結果

```
$ cargo test --lib
test result: ok. 908 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

| 項目 | Before | After |
|---|---|---|
| 既存テスト | 866 | 866 (全 pass) |
| 新規 contract test | 0 | 42 |
| 合計 | 866 | 908 |

**既存テスト破壊ゼロ。** 全 866 既存テスト + 42 新規テスト すべて pass。

## 新規 contract test 内訳

### `granularity.rs` (10 件)
- `top_municipalities_returns_n_items`
- `top_municipalities_skips_empty_fields`
- `top_municipalities_handles_n_larger_than_data`
- `top_municipalities_empty`
- `aging_rate_calculates_from_pyramid` (具体値: 60.0%)
- `working_age_rate_calculates_from_pyramid` (具体値: 40.0%)
- `estimated_unemployed_uses_direct_value` (具体値: 25,000)
- `estimated_unemployed_calculates_from_rate` (具体値: 16,000)
- `total_facilities_sums_4_categories` (具体値: 110)
- `education_is_always_pref_fallback`

### `integration.rs` (12 件)
- `granularity_heatmap_hidden_when_empty`
- `granularity_heatmap_shows_top_municipalities_with_counts`
- `granularity_heatmap_color_intensity_by_count` (alpha 値検証)
- `granularity_municipality_radar_hidden_when_empty`
- `granularity_municipality_radar_emits_6axis_data` (6 軸 + 図番号)
- `granularity_municipality_radar_shows_fallback_note`
- `granularity_municipality_radar_no_fallback_note_when_all_municipal`
- `granularity_extension_data_municipality_benchmark_renders`
- `granularity_municipality_benchmark_suppresses_prefecture_radar` (重複回避)
- `granularity_extension_data_heatmap_renders`
- `granularity_heatmap_pref_short_form` (短縮表記)
- `granularity_extension_data_defaults_empty`

### `report_html/demographics.rs` (5 件)
- `granularity_demographics_municipality_empty_renders_nothing`
- `granularity_demographics_municipality_all_empty_data_renders_nothing`
- `granularity_demographics_municipality_renders_kpi_values` (具体値: 高齢化率 20%, 生産年齢 80%, 失業者 5,000, 施設 45)
- `granularity_demographics_municipality_card_no_data_shows_placeholder`
- `granularity_section_bridge_present`

### `report_html/lifestyle.rs` (2 件)
- `granularity_lifestyle_social_life_pref_only_warning_strengthened`
- `granularity_lifestyle_internet_usage_pref_only_warning_strengthened`

### `report_html/wage.rs` (1 件)
- `granularity_household_spending_pref_only_warning_strengthened`

(`min_wage` 強化分は既存テスト数値検証で自動カバー)

## memory ルール遵守確認

| ルール | 遵守 |
|---|---|
| `feedback_correlation_not_causation.md` | ✅ 全注記で「相関」「傾向」「参考値」維持。新規セクションも同方針 |
| `feedback_hw_data_scope.md` | ✅ 都道府県粒度データに「市区町村別差なし」明示 |
| `feedback_test_data_validation.md` | ✅ 全 contract test に具体値検証 (60.0%, 25,000人, 110校 等) |
| `feedback_never_guess_data.md` | ✅ schema 確認後 (subtab5_phase4_7.rs / subtab7_phase_a.rs を読了) に実装、不明テーブルは fallback |
| `feedback_reverse_proof_tests.md` | ✅ 「要素存在」ではなくロジック検証 (alpha 値計算、fallback フラグ、KPI 具体値) |

## 都道府県粒度のままの箇所 (注記強化済み)

| 箇所 | データ | 強化方法 |
|---|---|---|
| `lifestyle.rs::render_social_life_block` | v2_external_social_life | オレンジ枠 + 「⚠ 都道府県粒度の参考値」+ 「市区町村別の差は反映されていません」 |
| `lifestyle.rs::render_internet_usage_block` | v2_external_internet_usage | 同上 (内容: 通信利用動向調査の注記) |
| `wage.rs::render_section_household_vs_salary` | v2_external_household_spending | 同上 (内容: 家計調査) |
| `wage.rs::render_section_min_wage` | v2_external_minimum_wage | 「47 県粒度のみ・同一都道府県内では最低賃金は同一」明示 |
| `integration.rs::render_industry_structure_section` | v2_external_industry_structure | 既存注記 (「国勢調査 2020 ベース」) 維持 |

## 親セッションへの統合チェックリスト

- [x] `cargo build --lib` pass
- [x] `cargo test --lib` 全 pass (908/908)
- [x] 既存 866 テスト破壊なし
- [x] 新規 contract test 10 件以上 (実装: 42 件)
- [x] 公開 API シグネチャ後方互換 (`render_survey_report_page` / `_with_enrichment` 維持)
- [x] memory ルール遵守 (相関≠因果 / HW スコープ / 具体値検証 / fallback 注記)
- [x] 都道府県粒度のままのデータ (P-1/P-2/#8/min_wage) の注記強化
- [ ] **未実施**: ブラウザでの実機確認 (本タスクはコード実装のみ)
- [ ] **未実施**: 実 DB データでの市区町村粒度 fetch 動作確認 (HW データ + Turso 接続環境必要)
- [ ] **未実施**: PDF 印刷時のレイアウト崩れ確認 (Readability チームと並行)

## 注意事項

1. **市区町村レーダーが空の時のみ都道府県レーダー描画**: 実 CSV に有効な市区町村が含まれている場合は都道府県レーダーは抑制される。これは UI 重複回避のためで、後方互換テストは `top3_region_benchmark` のみ設定するパスでも動作する。

2. **`fetch_education` は市区町村粒度未対応**: schema (v2_external_education) に citycode 列がないため、`MunicipalityDemographics.education` は常に都道府県値で `is_education_pref_fallback=true`。表示側ではこのデータは別 section の都道府県粒度ピラミッドに任せ、市区町村カードでは表示しない (混乱回避)。

3. **`render_survey_report_page_with_municipalities` の追加**: 既存の `render_survey_report_page_with_enrichment` を維持しつつ、新エントリを追加。`handlers.rs::survey_report_html` のみ新エントリを使用する。

4. **`survey_report_html` の `muni2 = String::new()` ハードコードは維持**: `InsightContext` 全体は依然として都道府県粒度 (cascade 等の HW 比較のため)。市区町村粒度は新追加の `municipality_demographics` パラメータでのみ提供。
