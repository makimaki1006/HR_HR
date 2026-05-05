# Phase 3 Step 5 Rust 統合 — Phase 1 既存資産監査

**Worker**: P1
**Date**: 2026-05-04
**Mode**: Read-only (Rust コード変更なし)
**Phase 0**: PASS (RANK/COUNT OVER + PARTITION BY が Turso libSQL 3.45.1 で動作確認済み)

---

## 0. 結論

Phase 2/3/4 着手準備は **OK**。

- 既存 DTO 4 つ + 上位 DTO 1 つは `src/handlers/analysis/fetch/market_intelligence.rs` に整備済み
- 既存 fetch 4 関数も同ファイルに整備済み (`Vec<Row>` → `to_*_dto` 経由で型変換)
- `render_section_market_intelligence` は L95 で完成しており、5 セクション render 関数は単に `&data: &SurveyMarketIntelligenceData` を受け取るだけの構造になっている
- variant ガードは `mod.rs` L927 に既に置かれており、現状 `SurveyMarketIntelligenceData::default()` (空 DTO) を渡している ← これを実 fetch 結果に置換するのが Step 5 の核心
- `query_turso_or_local` 利用パターンは確立済み (6 ファイルで 100+ 箇所利用)

懸念は §10 に記載。

---

## 1. 既存 DTO 行番号一覧

ファイル: `src/handlers/analysis/fetch/market_intelligence.rs` (1119 行)

| DTO | 行 | 説明 |
|-----|------|------|
| `MunicipalityRecruitingScore` | 343-381 | 配信優先度スコア (主要 DTO) |
| `MunicipalityRecruitingScore::from_row / is_scenario_consistent / is_priority_score_in_range` | 384-437 | impl |
| `LivingCostProxy` | 444-467 | 生活コスト proxy |
| `LivingCostProxy::from_row` | 470-486 | impl |
| `CommuteFlowSummary` | 497-526 | 通勤流入元 |
| `CommuteFlowSummary::from_row / is_scenario_consistent / is_flow_share_in_range` | 529-590 | impl |
| `OccupationPopulationCell` | 597-612 | 市区町村×職業×年齢×性別 (basis = "resident" / "workplace") |
| `OccupationPopulationCell::from_row` | 615-630 | impl |
| `SurveyMarketIntelligenceData` (上位 DTO) | 642-649 | 4 Vec を束ねる |
| `SurveyMarketIntelligenceData::is_empty / all_invariants_hold` | 651-672 | impl |
| `to_recruiting_scores / to_living_cost_proxies / to_commute_flows / to_occupation_populations` | 678-696 | Row → DTO 変換 |
| ヘルパー (`opt_i64`, `opt_f64`, `str_or_empty`) | 300-336 | 共通 |
| serde::Serialize 派生 | 全 DTO に `#[derive(Clone, Debug, Default, Serialize)]` 付与 | 統一パターン |

### Step 5 で追加する 4 DTO の挿入位置候補

`OccupationPopulationCell` (L597-630) の **直後** L631 (現在 `// -------- 上位 DTO` の直前) が最も自然。

| 追加 DTO | 推奨挿入行 | 理由 |
|---------|------------|------|
| `OccupationCellDto` (Plan B XOR) | L631 | 既存 `OccupationPopulationCell` の進化系として近接配置 |
| `WardThicknessDto` | L631 (続けて) | 親市内ランキング系 DTO 群を 1 ブロックに |
| `WardRankingRowDto` | L631 (続けて) | 同上 |
| `MunicipalityCodeMasterDto` | L631 (続けて) | 同上 |
| `DataSourceLabel` enum | L290 直後 (Step 2 ヘルパー域) | 列挙型は型ヘルパーゾーンが自然 |

### 既存 DTO の XOR 化判断

`OccupationPopulationCell` (L597) は `basis: String` で resident/workplace を文字列保持 (XOR 緩い実装)。

**判断**: 既存テスト (L2657-2683 ほか) が `basis = "resident"` リテラル比較しているため **破壊的変更を避ける**。
→ 新規 `OccupationCellDto` を **並列追加** (XOR enum 化) する案を採用。既存 `OccupationPopulationCell` は当面 `#[deprecated]` 扱いせず維持。

---

## 2. 既存 fetch 関数行番号

ファイル: `src/handlers/analysis/fetch/market_intelligence.rs`

| 関数 | 行 | 戻り値 | placeholder 状態 |
|------|------|--------|-----------------|
| `fetch_recruiting_scores_by_municipalities` | 52-113 | `Vec<Row>` | テーブル未投入時は空 Vec (フェイルセーフ) |
| `fetch_living_cost_proxy` | 115-152 | `Vec<Row>` | 同上 |
| `fetch_commute_flow_summary` | 170-215 | `Vec<Row>` | `commute_flow_summary` 不在時は `v2_external_commute_od` フォールバック (実データあり) |
| `fetch_occupation_population` | 227-274 | `Vec<Row>` | テーブル未投入時は空 Vec |

すべて `query_turso_or_local(turso, db, &sql, &params, &table_name)` を使用済み。

### Step 5 で追加する 4 fetch 関数の挿入位置

L274 (`fetch_occupation_population` の閉じ波括弧直後) の次。L276 の `// =========== Phase 3 Step 2: 型付き DTO 層` の **直前** が最適。

```
L274 } ← fetch_occupation_population end
L275 [挿入点]
  - fetch_occupation_cells       (Plan B XOR)
  - fetch_ward_thickness
  - fetch_ward_rankings_by_parent (核心 SQL: RANK/COUNT OVER + PARTITION BY)
  - fetch_code_master
L276 // ============================================================
L277 // Phase 3 Step 2: 型付き DTO 層
```

---

## 3. placeholder 渡し箇所

ファイル: `src/handlers/survey/report_html/mod.rs`

**L918-931** (Worker R1 が特定済の Step 5 統合点を確認):

```rust
// L927-931 (現状)
if variant.show_market_intelligence_sections() {
    let mi_data =
        super::super::analysis::fetch::SurveyMarketIntelligenceData::default();
    market_intelligence::render_section_market_intelligence(&mut html, &mi_data);
}
```

`SurveyMarketIntelligenceData::default()` がそのまま空 DTO で渡されている。

### 置換ポイント

L928-929 の 2 行を、`build_market_intelligence_data(db, turso, &target_municipalities, ...)` 呼び出しに置換する。

### async 化判断

`build_market_intelligence_data` は内部で `query_turso_or_local` (同期 reqwest blocking) を呼ぶ。**周辺 render 関数も同期で書かれている** ため、`spawn_blocking` は不要。同期のまま新規 fetch を呼ぶ方針が既存コードと一貫。

---

## 4. render 挿入点 + 既存セクション関数

ファイル: `src/handlers/survey/report_html/market_intelligence.rs` (740 行)

| 関数 | 行 | 責務 |
|------|------|------|
| `MEASURED_LABEL` 定数 | 41 | "実測" |
| `ESTIMATED_LABEL` 定数 | 43 | "推定" |
| `REFERENCE_LABEL` 定数 | 45 | "参考" |
| `build_market_intelligence_data` | 54-85 | Step 1 fetch + Step 2 DTO 変換統合 (現在は 4 fetch のみ呼ぶ) |
| `render_section_market_intelligence` (統合エントリ) | 95-123 | 6 sub-section を順番に呼ぶ |
| `render_mi_summary_card` | 128 | Section 1 |
| `render_mi_distribution_ranking` | 202 | Section 2 |
| `render_mi_talent_supply` | 280 | Section 3 |
| `render_mi_salary_living_cost` | 334 | Section 4 |
| `render_mi_scenario_population_range` | 406 | Section 5 |
| `render_mi_commute_inflow_supplement` | 472 | Section 6 (補助) |
| `render_mi_kpi` (KPI 共通) | 518 | ヘルパー |
| `render_mi_placeholder` | 531 | データ欠損時 |

### 新規ラベル定数の挿入位置

L45 直後 (L46) に追加:

```
WORKPLACE_LABEL: &str = "従業地"   ← L46 挿入
RESIDENT_LABEL:  &str = "常住地"   ← L47 挿入
ESTIMATED_BETA_LABEL: &str = "推定β" ← L48 挿入
```

### 親市内ランキング (新規 `render_mi_parent_ward_ranking`) の挿入位置

L120 (`render_mi_commute_inflow_supplement` 呼び出し) の **直前** または **直後**。

設計推奨: L120 の **直後** (L121) に追加し、L123 の `</section>` の前に挟み込む。

### 5+1 セクション render の placeholder → 実データ置換マップ

| Section | 関数行 | データ参照 | 置換要否 |
|---------|--------|------------|----------|
| 1 (Summary) | L128 | `data: &SurveyMarketIntelligenceData` | DTO 拡張時に追加データ参照 |
| 2 (Distribution Ranking) | L202 | `&[MunicipalityRecruitingScore]` | recruiting_scores が空のため placeholder 表示 → fetch 実装後に実データ |
| 3 (Talent Supply) | L280 | `&[OccupationPopulationCell]` | 同上 |
| 4 (Salary/Living Cost) | L334 | `&[MunicipalityRecruitingScore]`, `&[LivingCostProxy]` | 同上 |
| 5 (Scenario Population) | L406 | `&[MunicipalityRecruitingScore]` | 同上 |
| 6 (Commute Inflow 補助) | L472 | `&[CommuteFlowSummary]` | `v2_external_commute_od` 実データあり、既に動く |
| 7 新規 (Parent Ward Ranking) | L121 (新規) | `&[WardRankingRowDto]` | 全新規実装 |

---

## 5. ReportVariant 分岐箇所

ファイル: `src/handlers/survey/report_html/mod.rs`

| 内容 | 行 |
|------|------|
| `enum ReportVariant` 定義 (`Full / Public / MarketIntelligence`) | 92-105 |
| `from_query` (`"market_intelligence"` → `MarketIntelligence`) | 111-117 |
| `as_query` | 120-126 |
| `display_name` | 129-135 |
| `show_hw_sections()` (Full + MarketIntelligence true) | 141-143 |
| `show_market_intelligence_sections()` (核心 hook) | 149-151 |
| `icon()` | 154-160 |
| `alternative()` (MarketIntelligence → Full フォールバック) | 166-172 |
| `description()` | 175-181 |
| 既存 match 呼び出し: `Full \| MarketIntelligence` (HW 表示) | 805-806 |
| Public 専用ブロック | 813 |
| MarketIntelligence variant ガード (Step 5 統合点) | 927-931 |
| variant 関連テスト | 2406-2735 (~30 件) |

variant ガードは **二重になっておらず、L927 の単一 hook** のみ。Step 5 はこの 1 箇所を実 fetch 呼び出しに置換するのみで完結する。

---

## 6. query_turso_or_local 利用パターン

ファイル: `src/handlers/analysis/fetch/mod.rs`

| 内容 | 行 |
|------|------|
| `EXTERNAL_CLEAN_FILTER` 定数 (`prefecture/municipality` 両方) | 111-114 |
| `EXTERNAL_CLEAN_FILTER_NO_MUNI` 定数 (prefecture のみ) | 119-121 |
| `query_turso_or_local` 関数本体 | 125-156 |
| `query_3level` (3 段階フィルタヘルパー) | 159-202 |

### 関数シグネチャ (Step 5 雛形のベース)

```rust
pub(crate) fn query_turso_or_local(
    turso: Option<&TursoDb>,
    local_db: &Db,
    sql: &str,
    params: &[String],
    local_table_check: &str,
) -> Vec<Row>
```

### 既存呼び出し例 (継承パターン)

利用ファイル 6 件:
- `analysis/fetch/mod.rs` (定義 + テスト)
- `analysis/fetch/market_intelligence.rs` (4 fetch 関数)
- `analysis/fetch/subtab5_phase4.rs`
- `analysis/fetch/subtab5_phase4_7.rs`
- `analysis/fetch/subtab7_phase_a.rs`
- `analysis/fetch/subtab7_other.rs`

### Step 5 雛形 (4 新規 fetch 関数の共通テンプレート)

```rust
pub(crate) fn fetch_ward_rankings_by_parent(
    db: &Db,
    turso: Option<&TursoDb>,
    parent_municipality_code: &str,
) -> Vec<Row> {
    if parent_municipality_code.is_empty() {
        return vec![];
    }
    if !table_exists(db, "ward_rankings") && turso.is_none() {
        return vec![];
    }
    // RANK() OVER (PARTITION BY parent_code ORDER BY ...) + COUNT(*) OVER (PARTITION BY parent_code)
    let sql = "SELECT ward_code, ward_name, parent_municipality_code, \
               metric_value, \
               RANK() OVER (PARTITION BY parent_municipality_code ORDER BY metric_value DESC) AS rank, \
               COUNT(*) OVER (PARTITION BY parent_municipality_code) AS total \
               FROM ward_rankings \
               WHERE parent_municipality_code = ?1 \
               ORDER BY rank";
    let params = vec![parent_municipality_code.to_string()];
    query_turso_or_local(turso, db, sql, &params, "ward_rankings")
}
```

---

## 7. 既存テスト + snapshot ライブラリ

### `market_intelligence.rs` (analysis/fetch) のテスト

- L702 `#[cfg(test)] mod tests` 開始
- `create_test_db()` (L708) — `tempfile::NamedTempFile` 使用
- フィルタ系 / DTO 構築 / 不変条件テスト ~30 件

### `market_intelligence.rs` (report_html) のテスト

- L580 付近からテスト群開始 (推定、未読部分)
- `render_section_market_intelligence` 呼び出しテスト (L603, L637, L659, L678, L733)
- ラベル必須チェック (L640-642)

### `mod.rs` (report_html) のテスト

- L949 `#[cfg(test)] mod tests`
- L2406-2735 の variant 関連テスト (30+ 件)
- `test_market_intelligence_section_only_in_market_intelligence_variant` (L2652) ほか

### Cargo.toml dev-dependencies

L72-73:
```toml
[dev-dependencies]
tempfile = "3"
```

**snapshot ライブラリ (`insta`) は未導入**。既存テストは `assert!(html.contains(...))` パターンで検証。
→ Step 5 の追加テストも同パターン推奨。`insta` 追加は提案するが必須ではない。

### E2E test 所在

`tests/e2e/`:
- `a11y_helpers_2026_04_26.spec.ts`
- `mobile_layout_2026_04_26.spec.ts`
- `regression_2026_04_26.spec.ts`
- `survey_deepdive_2026_04_26.spec.ts`
- `fixtures/`

その他 `tests/` 直下:
- `auth_extra_tests.rs` (Rust integration)
- `test_industry_mapping_ssdse.py`
- `test_ssdse_phase_a.py`

### `tests/no_forbidden_terms.rs` (新規)

未存在。Step 5 で新規追加。

---

## 8. 変更対象ファイル一覧

| カテゴリ | ファイル | 主な変更内容 |
|---------|---------|------------|
| 編集 (DTO 追加) | `src/handlers/analysis/fetch/market_intelligence.rs` | L631 直前に 4 DTO 追加 + L290 直後に enum 追加 |
| 編集 (fetch 追加) | (同上) | L274-275 に 4 fetch 関数追加 |
| 編集 (render 改修) | `src/handlers/survey/report_html/market_intelligence.rs` | L46 にラベル 3 追加 / L121 に新規セクション render 追加 / L70-77 の `build_market_intelligence_data` に新規 fetch 統合 |
| 編集 (統合点) | `src/handlers/survey/report_html/mod.rs` | L928-929 を実 fetch 呼び出しに置換 |
| (確認のみ) | `src/handlers/analysis/fetch/mod.rs` | `query_turso_or_local` 継承 (変更なし) |
| 新規 | `tests/no_forbidden_terms.rs` | Hard NG 文字列テスト |
| 新規 | `tests/e2e/market_intelligence_*.spec.ts` | E2E 4 spec |
| 新規 | `.claude/hooks/no_population_terms.sh` | hooks ガード |
| 触らない | 上記以外すべて | Full / Public / 既存 26 テーブルロジック |

**編集対象**: 4 ファイル / **新規作成**: 6+ ファイル / **触らない**: 上記以外すべて

---

## 9. Phase 2/3/4 の行番号付き作業分解

### Phase 2 (DTO 追加、~1 日)

ファイル: `src/handlers/analysis/fetch/market_intelligence.rs`

| ステップ | 挿入行 | 内容 |
|---------|--------|------|
| (a) `OccupationCellDto` 追加 | L631 | Plan B XOR (`enum DataBasis { Resident, Workplace }`) |
| (b) `WardThicknessDto` 追加 | L631 続き | 親市内 ward 厚み指標 |
| (c) `WardRankingRowDto` 追加 | L631 続き | RANK + COUNT OVER 結果格納 |
| (d) `MunicipalityCodeMasterDto` 追加 | L631 続き | コード→名称 master |
| (e) `DataSourceLabel` enum 追加 | L290 直後 (型ヘルパー域) | `Measured / Estimated / Reference` |
| (f) 既存 `OccupationPopulationCell` の判断 | L599 | **並列追加** (破壊的変更回避)。既存テスト維持 |

### Phase 3 (fetch 追加、~1.5 日)

ファイル同上、挿入点 L274-275 (`fetch_occupation_population` 閉じた直後 / Step 2 ヘッダーの直前)。

| ステップ | 関数名 | 内容 |
|---------|--------|------|
| (a) | `fetch_occupation_cells` | Plan B XOR 版 fetch (basis enum 化) |
| (b) | `fetch_ward_thickness` | ward_thickness テーブル読取 |
| (c) | `fetch_ward_rankings_by_parent` | **核心 SQL**: RANK/COUNT OVER + PARTITION BY (Phase 0 で動作確認済) |
| (d) | `fetch_code_master` | municipality コード→名称マッピング |
| (e) | 共通: `query_turso_or_local(turso, db, &sql, &params, "<table_name>")` 雛形継承 |

### Phase 4 (render 改修、~1.5 日)

ファイル: `src/handlers/survey/report_html/market_intelligence.rs`

| ステップ | 行 | 内容 |
|---------|------|------|
| (a) ラベル定数 3 つ追加 | L46-48 | `WORKPLACE_LABEL / RESIDENT_LABEL / ESTIMATED_BETA_LABEL` |
| (b1) `render_mi_summary_card` 拡張 | L128- | 新規 KPI を追加データから読む |
| (b2) `render_mi_distribution_ranking` 実データ化 | L202- | placeholder 解除 (現状空 Vec で placeholder 表示) |
| (b3) `render_mi_talent_supply` 実データ化 | L280- | XOR DTO 対応 |
| (b4) `render_mi_salary_living_cost` 実データ化 | L334- | LivingCostProxy 接続強化 |
| (b5) `render_mi_scenario_population_range` 実データ化 | L406- | scenario 値接続 |
| (c) `render_mi_parent_ward_ranking` **新規追加** | L121 (新規挿入) | RANK/COUNT 表示 |
| (d) `build_market_intelligence_data` の 4 新規 fetch 統合 | L66-77 | 既存 4 fetch 直後に追加 |

ファイル: `src/handlers/survey/report_html/mod.rs`

| ステップ | 行 | 内容 |
|---------|------|------|
| (e) L928-929 を実 fetch 呼び出しに置換 | L927-931 | `SurveyMarketIntelligenceData::default()` → `build_market_intelligence_data(db, turso, &target_municipalities, occupation_group_code, dest_pref, dest_muni, top_n)` |
| (e2) `target_municipalities` を agg から抽出するロジック追加 | L927 直前 | agg.top_municipalities() 等から TOP N コード抽出 |

---

## 10. 既知の懸念

| # | 懸念 | 対応方針 |
|---|------|---------|
| 1 | 既存 `OccupationPopulationCell` の XOR 化が破壊的になる | **並列追加** で回避 (既存 DTO は当面維持、新規 `OccupationCellDto` を XOR enum で導入) |
| 2 | `target_municipalities` 抽出ロジックが未定義 | Phase 4 (e2) で agg から導出するロジックを追加実装 (Step 5 スコープ内) |
| 3 | 4 新規テーブル (`ward_rankings` 等) のデータが未投入 | フェイルセーフ (`!table_exists && turso.is_none()` → 空 Vec) で placeholder 表示。データ投入は Phase 3 後続で対応 |
| 4 | `render_mi_summary_card` 等が `data: &SurveyMarketIntelligenceData` 全体を受け取る設計のため、新規 DTO 4 つは `SurveyMarketIntelligenceData` を拡張する必要 | L644 の `SurveyMarketIntelligenceData` 構造体に Vec 4 つ追加 (破壊的だが Default 派生で空ベクタ自動補完。`is_empty / all_invariants_hold` impl も拡張) |
| 5 | snapshot ライブラリ未導入 | `assert!(html.contains(...))` パターンで継続。`insta` 導入は範囲外 |
| 6 | E2E spec ファイルの命名規約 | 既存 4 spec が `*_2026_04_26.spec.ts` 形式 → 新規も `market_intelligence_2026_05_04.spec.ts` 形式推奨 |
| 7 | hooks `.claude/hooks/no_population_terms.sh` の Windows 対応 | bash で動作するため Git Bash / WSL 前提。PowerShell 版を別途用意するか `pwsh` 版に切替が必要かも |

---

## 11. ファイルパス (絶対パス)

- 既存 DTO + fetch: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/analysis/fetch/market_intelligence.rs`
- query_turso_or_local: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/analysis/fetch/mod.rs`
- variant ガード + 統合点: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/report_html/mod.rs`
- render 5+1 セクション: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/report_html/market_intelligence.rs`
- E2E test ディレクトリ: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/`
- Cargo.toml: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/Cargo.toml`
- 本書: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_PHASE1_AUDIT.md`
