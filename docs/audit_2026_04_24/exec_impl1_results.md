# Impl-1 実装結果: 媒体分析データ活用 #6 / #18 / D-3 / D-4

**作成日**: 2026-04-26
**担当範囲**: 媒体分析タブ (`/tab/survey`) HW 統合分析セクション + 印刷レポート region 補助
**根拠**: `docs/audit_2026_04_24/survey_data_activation_plan.md` §2 案 #6 / #18 / D-3 / D-4
**並列協調**: Impl-2 (デモグラ系) / Impl-3 (経済+サイコ系) と非競合

---

## 1. 実装サマリ

| 案 | 配置 | データソース | 完了 |
|----|------|------------|------|
| #6 地域ベンチマーク 6 軸レーダー | Tab UI 統合分析 (新規) | `v2_region_benchmark` (top3 prefs 取得) | OK |
| #18 可住地密度 + 都市分類 | Tab UI 統合分析 + 印刷版 region | `InsightContext.ext_geography` 既取得 | OK |
| D-3 産業別就業者構成 | Tab UI 統合分析 (新規) | `v2_external_industry_structure` 新規 fetch | OK |
| D-4 高齢化率 (現在値) | Tab UI 統合分析 + 印刷版 region | `ext_population.aging_rate` / `ext_pyramid` 計算 | OK |

---

## 2. ファイル変更一覧 (絶対パス)

### コード本体
- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\integration.rs`
  - `SurveyExtensionData` 構造体新設 (L9-26)
  - `render_integration_with_ext` 拡張版エントリ追加 (L62-93)
  - `render_region_benchmark_radar_section` 新設 (案 #6)
  - `render_geography_aging_section` 新設 (案 #18 + D-4)
  - `render_industry_structure_section` 新設 (案 D-3)
  - `radar_score_to_pct` / `classify_habitable_density` / `calc_aging_rate_from_pyramid` ヘルパー
  - 旧 `render_integration` は `SurveyExtensionData::default()` 経由で 100% 後方互換

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\handlers.rs`
  - `integrate_report` ハンドラに `top3_prefs` 抽出ロジック (L267-280)
  - `build_survey_extension_data` ヘルパー追加 (L325-374)
  - `render_integration_with_ext` 呼び出しに変更

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\region.rs`
  - `Row` 型インポート追加
  - `render_section_region_extras` 新設 (印刷版 #18 + D-4)
  - `render_section_industry_structure` 新設 (印刷版 D-3、API 拡張時に活用、現在は dead_code 許容)
  - `classify_habitable_density` / `calc_aging_rate_from_pyramid` ヘルパー (region scope)
  - 印刷版テスト 3 件追加

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\survey\report_html\mod.rs`
  - `render_section_region_extras` を Section 6 (region) と Section 7 (municipality) の間に挿入

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\analysis\fetch\subtab5_phase4.rs`
  - `fetch_region_benchmarks_for_prefs` 新規追加 (複数 pref 一括取得)

- `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\src\handlers\analysis\fetch\mod.rs`
  - `fetch_region_benchmarks_for_prefs` / `fetch_industry_structure` を pub(crate) として正規エクスポート (#[allow(unused_imports)] 削除)

---

## 3. 各案の前後具体値 (逆証明)

### 案 #6 6 軸レーダーチャート

**Before**: `InsightContext.region_benchmark` は dominant pref のみで、tab UI / 印刷で未表示。
**After**:
- 新規 fetch `fetch_region_benchmarks_for_prefs(db, &top3_prefs)` で CSV 上位 3 県分取得
- ECharts `radar` series に 3 prefs × 6 軸を JSON で渡す
- 6 軸: 給与競争力 / 求人量 / 充足度 / 人口動態 / 賃金遵守 / 産業多様性
- 0-1 → 0-100 変換 (`radar_score_to_pct`)
- フォールバック凡例 (テキスト) も並記

**逆証明テスト** (`impl1_radar_section_emits_6axis_data_with_3_prefs`):
- 入力: 東京都 (sal=0.85, working_age=0.92), 神奈川県, 千葉県
- 出力検証: `給与競争力=85`, `人口動態=92` 文字列を含む
- ECharts config に `"type":"radar"` と `"max":100` を含む
- 必須注記「地域間の戦略的優劣ではなく特性の違い」「因果関係や採用成功の保証ではありません」

### 案 #18 可住地密度 + 都市分類

**Before**: `ext_geography` 取得済みだが媒体分析タブで未表示。
**After**:
- KPI カード: `5,200 人/km²` + 都市分類ラベル `都市型`
- 分類しきい値: ≥5000 都市型 / ≥1000 中間型 / 未満 郊外型
- 印刷版にも図 6-3 として追加 (Section 6 直後)

**逆証明テスト** (`impl1_habitable_density_classification_thresholds`):
- 5000.0 → 都市型 / 4999.0 → 中間型 / 1000.0 → 中間型 / 999.9 → 郊外型 / 0.0 → 郊外型 (境界の逆証明)

**逆証明テスト** (`impl1_habitable_density_kpi_with_city_class`):
- 入力 density=5200 → "5,200" + "都市型" + 必須注記「可住地密度は地理特性」「傾向参照」を含む

### 案 D-3 産業別就業者構成

**Before**: `fetch_industry_structure` 関数は存在したが dead_code (どこからも呼ばれず)。
**After**:
- handlers 側で `geo::pref_name_to_code()` で pref code 解決 → `fetch_industry_structure(db, turso, code)`
- Top10 横バー表示 (各バー幅 = max 比、構成比 % 併記)
- データ件数 0 / total 0 時は fail-soft (セクション非表示)

**逆証明テスト** (`impl1_industry_section_top10_with_pct`):
- 入力: 医療福祉 200000, 製造 150000, ... (合計 610000)
- 出力検証: `200,000` + 産業名表示 + 必須注記「国勢調査 2020 ベース」「HW industry_raw と粒度が異なる可能性」「職種・条件マッチングが本質的要因」

**逆証明テスト** (`impl1_industry_section_hidden_when_empty_or_zero_total`):
- 空入力 OR 全行 employees_total=0 → 空文字列 (fail-soft)

### 案 D-4 高齢化率 (現在値)

**Before**: `ext_population.aging_rate` 列はある (karte で使用) が媒体分析タブで未表示。
**After**:
- aging_rate を `ext_population` から優先取得、無ければ `ext_pyramid` から再計算 (`calc_aging_rate_from_pyramid`)
- KPI カード: `30.0%` + 全国 29% との差分 `+1.0pt` 表示
- 警告色: ≥35% warn / ≤22% good / 中央 neutral

**逆証明テスト** (`impl1_aging_rate_calc_specific_values_reverse_proof`):
- pyramid: 0-14:100, 15-64:600, 65-74:200, 75+:100 → elderly=300/total=1000 = 30.0%
- 計算結果が `(rate - 30.0).abs() < 0.01` (具体値検証)

**逆証明テスト** (`impl1_aging_rate_kpi_with_national_compare`):
- 出力に `30.0%`, `全国 29%`, `+1.0pt` (差分計算検証) を含む
- 必須注記「65 歳以上人口比率 = 高齢化率定義」「労働人口希少性の参考指標」を含む

---

## 4. 新規 contract テスト一覧

### Tab UI (`integration.rs::impl1_contract_tests`)
| # | テスト | 検証対象 |
|---|--------|----------|
| 1 | `impl1_radar_section_emits_6axis_data_with_3_prefs` | #6 ECharts data + 6 軸 + 具体値 (85, 92) |
| 2 | `impl1_radar_section_hidden_when_top3_empty` | #6 fail-soft |
| 3 | `impl1_radar_score_clamps_invalid_values` | #6 数値変換境界 (NaN / 負 / >1 / 1.0) |
| 4 | `impl1_habitable_density_kpi_with_city_class` | #18 値 5,200 + 都市型 + 注記 |
| 5 | `impl1_habitable_density_classification_thresholds` | #18 境界の逆証明 (5 ケース) |
| 6 | `impl1_industry_section_top10_with_pct` | D-3 値 200,000 + 注記 + pref 表示 |
| 7 | `impl1_industry_section_hidden_when_empty_or_zero_total` | D-3 fail-soft 2 系統 |
| 8 | `impl1_aging_rate_kpi_with_national_compare` | D-4 値 30.0% + 全国 29% + +1.0pt |
| 9 | `impl1_aging_rate_calc_specific_values_reverse_proof` | D-4 計算ロジック (pyramid → 30.0%) |
| 10 | `impl1_geo_aging_section_hidden_when_no_data` | #18 + D-4 fail-soft |
| 11 | `impl1_survey_extension_data_default_is_empty` | 後方互換 (Default 空) |
| 12 | `impl1_render_integration_legacy_signature_still_works` | 旧 API 互換性 |

### 印刷版 (`region.rs::impl1_print_tests`)
| # | テスト | 検証対象 |
|---|--------|----------|
| 13 | `impl1_print_region_extras_renders_density_and_aging` | 印刷 #18 + D-4 (具体値 8,500 / 33.5% / +4.5pt) |
| 14 | `impl1_print_industry_section_renders_top10_with_pct` | 印刷 D-3 (具体値 41.7% / 100,000) |
| 15 | `impl1_print_region_extras_hidden_when_no_data` | 印刷 fail-soft |

**合計 15 件**（spec 要求の最低 8 件を上回る）。

---

## 5. 既存テスト結果

```
test result: ok. 866 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out
```

- ベースライン: 825 (Impl-1 着手前)
- Impl-1 実装後: **866** (+41 件、ただし他並列 agent 追加分を含む。Impl-1 単独では +15 件)
- **既存 825 テストの破壊ゼロ**

ビルド警告: 2 件 (既存 dead_code 警告のみ、新規導入なし)。

---

## 6. memory ルール準拠状況

| ルール | 適用 | 確認方法 |
|--------|------|----------|
| `feedback_correlation_not_causation.md` | 全案 | 「因果関係や採用成功の保証ではありません」「因果ではなく傾向参照」「職種・条件マッチングが本質的要因」など断定語回避 |
| `feedback_hw_data_scope.md` | 該当箇所のみ | 既存 hw_section の HW 限定スコープ注記を維持。新セクションは外部統計ベースのため HW スコープ注記は付与せず、各データの出典を明示 (国勢調査 2020 等) |
| `feedback_test_data_validation.md` | 全 contract テスト | 「要素存在」だけでなく `5,200` `30.0%` `+1.0pt` `41.7%` 等の **具体計算値** で検証 |
| `feedback_reverse_proof_tests.md` | fail-soft / 境界値 | 各案で「データ無 → 非表示」「境界値で分類が変わる」「Default 状態の互換性」を逆証明 |
| 絵文字禁止 | 全コード | 装飾絵文字なし。<sup>2</sup> など機能的な HTML エンティティのみ |

---

## 7. 親セッションへの統合チェックリスト

- [x] `cargo test --lib` 866/866 passed (1 ignored)
- [x] `cargo build --lib` 2 warnings (既存のみ、新規導入なし)
- [x] 公開 API シグネチャ変更なし
  - `render_integration` 旧シグネチャ温存、`render_integration_with_ext` を pub(crate) で追加
  - `render_survey_report_page` シグネチャ不変、印刷版 region_extras は `hw_context` から条件分岐
- [x] InsightContext 構造体は他 agent (Impl-2/3) の既存追加 (`ext_education` / `ext_social_life` / `ext_internet_usage`) と整合
- [x] 並列ファイル競合なし
  - Impl-1: `integration.rs` + `report_html/region.rs` (本担当)
  - Impl-2: `report_html/{seeker,executive_summary,demographics}.rs` (干渉なし)
  - Impl-3: `report_html/{wage,lifestyle}.rs` (干渉なし)
- [x] `helpers.rs` の UI-3 関数を呼出のみ (新規追加なし)
- [x] `style.rs` の既存 CSS class (`stat-card`, `kpi-grid`, `kpi-card-v2` 等) を再利用 (新規追加なし)
- [x] 全案で `data-testid` 付与済 (E2E test / 後続検証で参照可能)
- [x] 各案に必須注記、図番号 (図 6-2 / 図 6-3 / 表 6-2)、具体値検証

### 統合手順
1. 親セッションで `cargo test --lib` を再実行し、全 agent 統合後の合計が 866 以上を維持していることを確認
2. `top3_prefs` が CSV キャッシュ (`survey_agg_{session_id}`) から確実に取得できることをユーザー実機で確認
3. 印刷版 `render_section_region_extras` は ctx ありの場合のみ表示 → 本番運用で hw_context が常に Some であるか確認
4. v2_external_industry_structure テーブルの prefecture_code 文字列形式 ("01"〜"47") が `geo::pref_name_to_code()` の出力と一致することをローカル DB で目視確認

---

## 8. 既知の制約・将来の拡張

| 項目 | 制約 | 提案 |
|------|------|-----|
| 印刷版 #6 レーダー | render_survey_report_page 公開 API シグネチャ不変のため未実装 | 将来 InsightContext に `top3_region_benchmark` フィールドを追加する形で API 不変のまま拡張可能 |
| 印刷版 D-3 産業構造 | 同上 | `render_section_industry_structure` 関数は実装済 (dead_code 許容) → API 拡張時に呼び出し可能 |
| 全国高齢化率 | 定数 29.0% (2023 年人口推計) | 将来 v2_external_population の '全国' 行を取得して動的化可能 |
| 産業構造 prefecture_code | `geo::pref_name_to_code()` (47 県固定) で解決 | 47 県以外 (海外等) は空フォールバック (fail-soft) |

---

## 9. 改訂履歴

| 日付 | 内容 |
|------|------|
| 2026-04-26 | 新規作成 (Impl-1 並列実装、案 #6 / #18 / D-3 / D-4) |
