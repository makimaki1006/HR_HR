# Phase 3 Step 5: Rust 統合計画書

**作成日**: 2026-05-04
**Worker**: R1 (計画書のみ、実装は別ラウンド)
**前提**: Turso 4 テーブル (`municipality_occupation_population` 729,949 行 / `v2_municipality_target_thickness` 20,845 行 / `commute_flow_summary` 27,879 行 / `municipality_code_master` 1,917 行) 投入完了。

---

## 0. 結論

| 項目 | 値 |
|------|-----|
| **実装可否** | 可 (既存資産の延長で実現可能、破壊的変更なし) |
| **総工数見積** | 約 **7 人日** (Phase 1〜8、後述) |
| **新規ファイル** | 0 (既存 `market_intelligence.rs` 2 ファイルへの追記で完結) |
| **既存テスト破壊リスク** | 低 (variant ガードで Full/Public は影響ゼロ) |
| **不確実性** | XOR CHECK 制約に対する DTO 表現、parent_code ランクの SQL パフォーマンス |

Plan B (XOR フィールド設計、`workplace × measured` と `resident × estimated_beta` の二択) と designated_ward の親市内ランキングを核に据えた、最小侵襲的な拡張。Step 1〜4 (39e7566) で既に variant 分岐・DTO 基盤・placeholder render が揃っているため、Step 5 は **データ実接続 + DTO 拡張 + 5 セクション本実装 + parent_code ランキング追加** が中心になる。

---

## 1. 既存 Rust 資産の状況

### 1.1 fetch 層 (`src/handlers/analysis/fetch/market_intelligence.rs`)

| 行 | 要素 | 状態 |
|----|------|------|
| L52-113 | `fetch_recruiting_scores_by_municipalities` | 実装済 (`municipality_recruiting_scores` 用、Step 5 では未使用) |
| L121-152 | `fetch_living_cost_proxy` | 実装済 (Step 5 では未使用) |
| L170-215 | `fetch_commute_flow_summary` | 実装済。`commute_flow_summary` 優先 + `v2_external_commute_od` フォールバック動作 |
| L227-274 | `fetch_occupation_population` | 実装済。`basis` 引数で `resident`/`workplace` 切替可、ただし `data_label` / `source_name` / `weight_source` カラムは未取得 |
| L300-336 | `opt_i64` / `opt_f64` / `str_or_empty` | 実装済 (Turso 文字列値吸収ヘルパー) |
| L345-437 | `MunicipalityRecruitingScore` DTO | 実装済 |
| L446-486 | `LivingCostProxy` DTO | 実装済 |
| L497-590 | `CommuteFlowSummary` DTO | 実装済 (両カラム命名対応済、`origin_municipality_code` フィールドあり) |
| L599-630 | `OccupationPopulationCell` DTO | **拡張必要**: `data_label`, `source_name`, `weight_source`, `estimate_index` カラムなし |
| L644-672 | `SurveyMarketIntelligenceData` 上位 DTO | **拡張必要**: `target_thickness`, `ward_rankings`, `code_master` を追加 |
| L679-696 | `to_*` 変換ヘルパー | 実装済 |

### 1.2 fetch 共通基盤 (`src/handlers/analysis/fetch/mod.rs`)

| 行 | 要素 | 用途 |
|----|------|------|
| L111-114 | `EXTERNAL_CLEAN_FILTER` | `prefecture / municipality` ヘッダー混入除外 |
| L125-156 | `query_turso_or_local` | Turso 優先 → ローカル SQLite フォールバック (Step 5 でもこれを継承) |

### 1.3 HTML render 層 (`src/handlers/survey/report_html/market_intelligence.rs`)

| 行 | 要素 | 状態 |
|----|------|------|
| L41-45 | `MEASURED_LABEL` / `ESTIMATED_LABEL` / `REFERENCE_LABEL` 定数 | 既存利用継続。Step 5 で `WORKPLACE_LABEL` ("従業地") / `RESIDENT_LABEL` ("常住地") / `ESTIMATED_BETA_LABEL` ("推定 β") を追加 |
| L54-85 | `build_market_intelligence_data` | **改修必要**: 4 fetch 関数追加・signature 変更 (`occupation_code` 引数追加、parent_code 渡し追加) |
| L95-123 | `render_section_market_intelligence` 統合エントリ | **改修必要**: 5 → 6 セクション + parent ranking パネル追加 |
| L128-198 | `render_mi_summary_card` | 既存維持 (KPI 文言の Plan B 化のみ) |
| L202-276 | `render_mi_distribution_ranking` | **大幅改修**: priority A-D + parent_code 内ランクの並列表示 |
| L280-330 | `render_mi_talent_supply` | **改修必要**: `is_population_displayable` ガードで人数 ↔ 指数を出し分け |
| L334-402 | `render_mi_salary_living_cost` | 既存維持 (Step 5 では使わないが、変更しない) |
| L406-468 | `render_mi_scenario_population_range` | **大幅改修**: `v2_municipality_target_thickness` の `scenario_*` (cons/std/agg) を指数バー表示。「人数換算しない」ガード必須 |
| L472-514 | `render_mi_commute_inflow_supplement` | 既存維持 (parent_code JOIN による表示名整形のみ) |
| L518-559 | KPI / placeholder / format ヘルパー | 既存維持 |

### 1.4 mod.rs 統合点 (`src/handlers/survey/report_html/mod.rs`)

| 行 | 要素 | 状態 |
|----|------|------|
| L93-105 | `enum ReportVariant` (`Full`/`Public`/`MarketIntelligence`) | 既存維持 |
| L141-143 | `show_hw_sections` (`Full|MarketIntelligence` で true) | 既存維持 |
| L149-151 | `show_market_intelligence_sections` フックメソッド | 既存維持 (核となる variant ガード) |
| L918-931 | Step 5 セクション挿入箇所 (Section 13 直前) | **改修必要**: `SurveyMarketIntelligenceData::default()` を `build_market_intelligence_data(...)` 実呼び出しに置換 |

`?variant=market_intelligence` 解決 (L114) は既に動作。Step 5 ではここに **DB / Turso ハンドル + 対象 occupation_code + parent_code を流し込む配線** が新規作業の中心。

---

## 2. DTO 設計 (4 対象テーブル × Plan B 制約)

### 2.1 `OccupationCellDto` (拡張版、既存 `OccupationPopulationCell` と並列で追加)

```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct OccupationCellDto {
    // 共通キー
    pub municipality_code: String,
    pub prefecture: String,
    pub municipality_name: String,
    pub basis: String,                    // 'workplace' | 'resident'
    pub occupation_code: String,
    pub occupation_name: String,
    pub age_class: String,
    pub gender: String,

    // XOR フィールド (どちらか一方が必ず None)
    pub population: Option<i64>,          // measured 時のみ Some
    pub estimate_index: Option<f64>,      // estimated_beta 時のみ Some

    // 出所メタ
    pub data_label: String,                // 'measured' | 'estimated_beta'
    pub source_name: String,               // 'census_15_1' | 'model_f2_target_thickness' 等
    pub source_year: i64,
    pub weight_source: Option<String>,     // estimated_beta のときのみ Some
}

#[derive(Debug, Clone, Copy, Serialize)]
pub enum DataSourceLabel {
    ResidentActual,         // (将来) basis=resident + measured
    ResidentEstimatedBeta,  // basis=resident + estimated_beta (Phase 3 での主表示モード)
    WorkplaceMeasured,      // basis=workplace + measured (Phase 3 での主表示モード)
    WorkplaceEstimatedBeta, // basis=workplace + estimated_beta (15-1 fallback、現状未投入)
    AggregateParent,        // (UI 暫定) parent_code 集約表示
}

impl OccupationCellDto {
    pub fn label(&self) -> DataSourceLabel { /* basis × data_label で 4 分岐 */ }

    /// 人数を絶対値として表示してよいか (UI ガード)
    pub fn is_population_displayable(&self) -> bool {
        self.data_label == "measured" && self.population.is_some()
    }

    /// 指数のみ表示すべきか
    pub fn is_index_only(&self) -> bool {
        self.data_label == "estimated_beta" && self.estimate_index.is_some()
    }

    /// XOR CHECK 不変条件 (DB 側 CHECK と二重防御)
    pub fn is_xor_consistent(&self) -> bool {
        match self.data_label.as_str() {
            "measured" => self.population.is_some() && self.estimate_index.is_none(),
            "estimated_beta" => self.estimate_index.is_some() && self.population.is_none(),
            _ => false,
        }
    }
}
```

**ハード NG (Worker C2/C4 docs 準拠 + 2026-05-06 拡張、コードレビュー時の必須チェック項目)**:

#### 2.1.1 禁止フィールド名 / 関数名 (Rust + Python 全コード)

英語識別子:
- `population_count`
- `target_count`
- `market_size_yen`
- `applicant_count`
- **`estimated_population`** ← 新規
- **`estimated_worker_count`** ← 新規
- **`resident_population_estimate`** ← 新規
- `convert_index_to_population()`
- `index_to_count()` 等の変換関数全般

#### 2.1.2 禁止文字列 (HTML テンプレート / docstring / コメント / log message)

日本語:
- **「推定人数」** ← 新規
- **「想定人数」** ← 新規
- **「母集団人数」** ← 新規
- **「○人見込み」** (例: 「3000人見込み」、数字+「人見込み」のパターン) ← 新規
- 「採用ターゲット候補総数」 (Worker C2 既存)
- 「採用市場規模」 (Worker C2 既存)

#### 2.1.3 禁止表現の合理化試み (実装で見たら即 reject)

- HTML テンプレート内で `data_label != 'measured'` のとき `population` を 0 fallback して表示
- `estimate_index` を 100 倍して「人数」と称する文字列に埋め込み
- WHERE 句で `data_label='estimated_beta'` をフィルタしておきながらテンプレートで「人数」表記
- `format!("{}人見込み", estimate_index * factor)` 系の動的生成

#### 2.1.4 機械チェック (Phase 6 テスト + hooks 化)

Phase 6 で Hard NG 文字列テストを追加 (詳細 §6.5)。さらに R4 リスク (営業要望の誘惑) に対応するため CLAUDE.md hooks 化 (§10 参照)。

### 2.2 `WardThicknessDto` (`v2_municipality_target_thickness` 由来)

```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct WardThicknessDto {
    pub municipality_code: String,
    pub municipality_name: String,
    pub prefecture: String,
    pub occupation_code: String,
    pub occupation_name: String,

    pub thickness_index: f64,            // 0〜200 程度の指数
    pub rank_in_occupation: i64,         // 全国順位 (参考表示)
    pub distribution_priority: String,   // 'A' | 'B' | 'C' | 'D'
    pub is_industrial_anchor: bool,

    // シナリオ濃淡 (3 段階指数、人数換算しない)
    pub scenario_conservative_index: Option<f64>,
    pub scenario_standard_index: Option<f64>,
    pub scenario_aggressive_index: Option<f64>,

    pub source_year: i64,
}

impl WardThicknessDto {
    pub fn is_priority_valid(&self) -> bool {
        matches!(self.distribution_priority.as_str(), "A"|"B"|"C"|"D")
    }
    pub fn is_scenario_consistent(&self) -> bool {
        match (self.scenario_conservative_index, self.scenario_standard_index, self.scenario_aggressive_index) {
            (Some(c), Some(s), Some(a)) => c <= s && s <= a,
            _ => true,
        }
    }
}
```

### 2.3 `WardRankingRowDto` (parent_code ランキング、商品の核心)

```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct WardRankingRowDto {
    pub municipality_code: String,
    pub municipality_name: String,
    pub parent_code: String,          // master_by_code から JOIN
    pub parent_name: String,           // 政令市本体名 (例: "横浜市")
    pub parent_rank: i64,              // 親市内ランク (1〜N、N=区数)
    pub parent_total: i64,             // N (区数)
    pub national_rank: i64,            // 全国順位 (参考表示)
    pub national_total: i64,           // 約 1,917
    pub thickness_index: f64,
    pub priority: String,              // A/B/C/D
}
```

#### 2.3.1 表示優先順位 (UI 必須ルール、user 指示)

商品 UI / HTML テンプレートでは **必ず以下の順序で配置** する。全国順位を主指標にしないこと。

| 表示順位 | 項目 | フィールド | UI 表現 |
|:-------:|------|-----------|--------|
| **1 (主)** | **市内順位** | `parent_rank` | 大きく強調、例: 「**1 位 / 18 区**」 |
| **2 (主)** | **市内総数** | `parent_total` | 同行、分母 |
| 3 (参考) | 全国順位 | `national_rank` | 小さく、灰色、例: 「(全国 12 位 / 1,917 参考)」 |

#### 2.3.2 NG レイアウト (実装で禁止)

- 全国順位を主指標として大きく出す
- 市内順位を省略
- `national_rank` だけを表示し `parent_rank` を出さない
- 並び順を逆転 (national → parent)

テンプレート grep でこれらをチェックする (Phase 6 のテストで検出)。

### 2.4 `MunicipalityCodeMasterDto` (`municipality_code_master`、補助 lookup)

```rust
#[derive(Debug, Clone, Default, Serialize)]
pub struct MunicipalityCodeMasterDto {
    pub municipality_code: String,     // JIS 5 桁
    pub municipality_name: String,
    pub prefecture: String,
    pub area_type: String,             // 'designated_ward' | 'designated_city' | 'standard' | ...
    pub parent_code: Option<String>,
}
```

### 2.5 上位 DTO 拡張

```rust
#[derive(Clone, Debug, Default, Serialize)]
pub struct SurveyMarketIntelligenceData {
    // 既存
    pub recruiting_scores: Vec<MunicipalityRecruitingScore>,
    pub living_cost_proxies: Vec<LivingCostProxy>,
    pub commute_flows: Vec<CommuteFlowSummary>,
    pub occupation_populations: Vec<OccupationPopulationCell>,

    // Step 5 で追加
    pub occupation_cells: Vec<OccupationCellDto>,         // Plan B 対応版
    pub ward_thickness: Vec<WardThicknessDto>,
    pub ward_rankings: Vec<WardRankingRowDto>,
    pub code_master: Vec<MunicipalityCodeMasterDto>,
}
```

---

## 3. fetch 関数設計 (4 関数、SQL 雛形)

すべて既存パターンに揃え、`query_turso_or_local` 経由で Turso 優先・ローカル SQLite フォールバック。`fetch/market_intelligence.rs` の末尾 (現行 L274 直後、`Phase 3 Step 2` コメントブロック内) に追記。

### 3.1 `fetch_occupation_cells` (Plan B 対応版、既存 `fetch_occupation_population` の上位互換)

```rust
pub(crate) fn fetch_occupation_cells(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_codes: &[&str],
    basis: &str, // 空なら両方、'workplace' or 'resident'
) -> Vec<Row> {
    // EXTERNAL_CLEAN_FILTER は使わない (本テーブルにはヘッダー混入なし、JIS 統制済)
    let placeholders_m = (1..=municipality_codes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
    let mut where_clauses = vec![format!("municipality_code IN ({placeholders_m})")];
    let mut params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();

    if !basis.is_empty() {
        params.push(basis.to_string());
        where_clauses.push(format!("basis = ?{}", params.len()));
    }
    if !occupation_codes.is_empty() {
        let start = params.len() + 1;
        let p: String = (start..start + occupation_codes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
        where_clauses.push(format!("occupation_code IN ({p})"));
        params.extend(occupation_codes.iter().map(|s| s.to_string()));
    }

    let sql = format!(
        "SELECT municipality_code, prefecture, municipality_name, basis,
                occupation_code, occupation_name, age_class, gender,
                population, estimate_index,
                data_label, source_name, source_year, weight_source
         FROM municipality_occupation_population
         WHERE {} ORDER BY occupation_code, age_class, gender",
        where_clauses.join(" AND ")
    );
    query_turso_or_local(turso, db, &sql, &params, "municipality_occupation_population")
}
```

### 3.2 `fetch_ward_thickness` (priority + thickness_index)

```rust
pub(crate) fn fetch_ward_thickness(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
    occupation_code: &str, // 空なら全職業
) -> Vec<Row> {
    if municipality_codes.is_empty() { return vec![]; }
    let placeholders = (1..=municipality_codes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
    let mut params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();
    let occ_clause = if occupation_code.is_empty() {
        String::new()
    } else {
        params.push(occupation_code.to_string());
        format!(" AND occupation_code = ?{}", params.len())
    };
    let sql = format!(
        "SELECT municipality_code, municipality_name, prefecture,
                occupation_code, occupation_name,
                thickness_index, rank_in_occupation, distribution_priority,
                is_industrial_anchor,
                scenario_conservative_index, scenario_standard_index, scenario_aggressive_index,
                source_year
         FROM v2_municipality_target_thickness
         WHERE municipality_code IN ({placeholders}){occ_clause}
         ORDER BY thickness_index DESC"
    );
    query_turso_or_local(turso, db, &sql, &params, "v2_municipality_target_thickness")
}
```

### 3.3 `fetch_ward_rankings_by_parent` (商品の核心 SQL、parent_code 内 RANK)

```rust
pub(crate) fn fetch_ward_rankings_by_parent(
    db: &Db,
    turso: Option<&TursoDb>,
    parent_code: &str,
    occupation_code: &str,
) -> Vec<Row> {
    if parent_code.is_empty() || occupation_code.is_empty() { return vec![]; }
    let sql = "
        SELECT
            v.municipality_code,
            v.municipality_name,
            mcm.parent_code,
            COALESCE(parent.municipality_name, '') AS parent_name,
            RANK() OVER (PARTITION BY mcm.parent_code ORDER BY v.thickness_index DESC) AS parent_rank,
            COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total,
            v.rank_in_occupation AS national_rank,
            (SELECT COUNT(*) FROM v2_municipality_target_thickness WHERE occupation_code = ?2) AS national_total,
            v.thickness_index,
            v.distribution_priority
        FROM v2_municipality_target_thickness v
        JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
        LEFT JOIN municipality_code_master parent ON mcm.parent_code = parent.municipality_code
        WHERE mcm.area_type = 'designated_ward'
          AND v.occupation_code = ?2
          AND mcm.parent_code = ?1
        ORDER BY parent_rank
    ";
    let params = vec![parent_code.to_string(), occupation_code.to_string()];
    query_turso_or_local(turso, db, sql, &params, "v2_municipality_target_thickness")
}
```

### 3.4 `fetch_code_master` (parent_code lookup)

```rust
pub(crate) fn fetch_code_master(
    db: &Db,
    turso: Option<&TursoDb>,
    municipality_codes: &[&str],
) -> Vec<Row> {
    if municipality_codes.is_empty() { return vec![]; }
    let placeholders = (1..=municipality_codes.len()).map(|i| format!("?{i}")).collect::<Vec<_>>().join(",");
    let sql = format!(
        "SELECT municipality_code, municipality_name, prefecture, area_type, parent_code
         FROM municipality_code_master
         WHERE municipality_code IN ({placeholders})"
    );
    let params: Vec<String> = municipality_codes.iter().map(|s| s.to_string()).collect();
    query_turso_or_local(turso, db, &sql, &params, "municipality_code_master")
}
```

---

## 4. HTML render 設計 (5 セクション + 親市内ランキング)

### 4.1 統合エントリ改修

`render_section_market_intelligence` (L95) のシグネチャを `data: &SurveyMarketIntelligenceData` のまま維持し、内部で **既存 5 セクション + Step 5 新規 1 セクション** をレンダ。`build_market_intelligence_data` は引数追加 (occupation_code、parent_code) で改修。

### 4.2 ラベル定数追加

```rust
pub const WORKPLACE_LABEL: &str = "従業地";
pub const RESIDENT_LABEL: &str = "常住地";
pub const ESTIMATED_BETA_LABEL: &str = "推定 β";
```

### 4.3 セクション一覧 (Step 5 後)

| # | セクション | 関数 | データソース | 表示ガード |
|---|----------|------|------------|-----------|
| 1 | 結論サマリー | `render_mi_summary_card` | 既存 + ward_thickness 件数 KPI 追加 | なし |
| 2 | **配信地域ランキング (priority A-D + parent rank)** | `render_mi_distribution_ranking` 改修 | `ward_thickness` + `ward_rankings` | priority is_priority_valid() |
| 3 | 人材供給 (人数 ↔ 指数 出し分け) | `render_mi_talent_supply` 改修 | `occupation_cells` | `is_population_displayable` で表示モード分岐 |
| 4 | 給与・生活コスト | 既存維持 | 既存 | 既存 |
| 5 | **シナリオ濃淡バー (cons/std/agg 指数)** | `render_mi_scenario_population_range` 改修 | `ward_thickness.scenario_*_index` | `is_scenario_consistent()` |
| 6 | 通勤流入元 (補助、parent_name で表示整形) | `render_mi_commute_inflow_supplement` 微改修 | `commute_flows` + `code_master` | 既存 |
| 7 (新規) | **親市内ランキングパネル** | `render_mi_parent_ward_ranking` (新規追加) | `ward_rankings` | designated_ward のみ |

### 4.4 親市内ランキング HTML (核心、新規)

```html
<section class="mi-parent-rank">
  <h3>市内ランキング (designated_ward のみ)</h3>
  <table>
    <thead><tr>
      <th>区</th><th>市内順位</th><th>厚み指数 (β)</th>
      <th>優先度</th><th>全国順位 (参考)</th>
    </tr></thead>
    <tbody>
      <!-- 例: 横浜市鶴見区 1 / 18 区、指数 142、A、全国 12/1917 -->
    </tbody>
  </table>
  <p class="muted">
    [推定 β] 「市内順位」が主指標、「全国順位」は参考表示です。
    指数値は推定 β モデルによるもので、人数ではありません。
  </p>
</section>
```

UI 表示優先順 (user 指示):

```
🏠 常住地ベース (推定 β):
  横浜市鶴見区: 厚み指数 142 (β)
    🏢 市内ランク: 1 位 / 18 区 (横浜市内)  ← 主表示 (font-size 大)
    🌐 全国ランク: 12 位 / 1,917 (参考)     ← 補助 (font-size 小)
```

### 4.5 人数表示 OK/NG マトリクス (テンプレートガード)

| 表示要素 | 出力条件 | 例 |
|---------|---------|-----|
| 人数 (絶対値) | `data_label='measured'` のみ | 「川崎市鶴見区 (従業地) 生産工程 12,345 人」 |
| 推定指数 (0-200) | `data_label='estimated_beta'` のみ | 「鶴見区 (常住地) 厚み指数 142 (推定 β)」 |
| 配信優先度 (A-D) | basis 不問 | 「A ランク」 |
| 全国ランク | **参考注記必須** | 「全国 7 位 / 1,917 (参考)、市内 1 位 / 18 区 (推奨)」 |
| シナリオ濃淡 (cons/std/agg) | 指数のみ、人数換算禁止 | バー 3 段階 |
| 産業集積 | `is_industrial_anchor=true` | 🏭 工業集積地 |

---

## 5. variant ガード戦略 (Full/Public 非影響)

### 5.1 既存ガード継承

`mod.rs` L927 `if variant.show_market_intelligence_sections() { ... }` を維持。Step 5 の build / render 全パス を **このブロック内** にしか書かない。

```rust
if variant.show_market_intelligence_sections() {
    let occupation_code = /* リクエストパラメータから抽出 */;
    let parent_code = /* code_master の parent_code lookup */;
    let target_municipalities = /* 主要市区町村 TOP N */;
    let mi_data = market_intelligence::build_market_intelligence_data(
        &db, turso.as_ref(),
        &target_municipalities, occupation_code, parent_code, dest_pref, dest_muni, top_n,
    );
    market_intelligence::render_section_market_intelligence(&mut html, &mi_data);
}
```

### 5.2 個別レンダラ内の二重ガード (防御的)

```rust
fn render_mi_parent_ward_ranking(
    html: &mut String,
    rankings: &[WardRankingRowDto],
    variant: ReportVariant, // ← 引数で受ける
) {
    if !variant.show_market_intelligence_sections() { return; }
    // ...
}
```

`build_market_intelligence_data` 内部でも変則的に呼ばれた場合に備え、空 Vec で早期 return。

### 5.3 影響範囲 grep (実装着手前 必須)

```
rg "ReportVariant" src/
rg "MarketIntelligence" src/
rg "render_section" src/handlers/survey/report_html/
rg "show_market_intelligence_sections" src/
```

期待値:
- `ReportVariant::Full | ReportVariant::Public` のみで分岐している箇所を特定
- `MarketIntelligence` を含む match arm が `Full` と同じ挙動になっている箇所 (L142、L170 等) を破壊しない

---

## 6. テスト計画

### 6.1 unit (`#[cfg(test)] mod tests` 内、既存ファイルに追記)

`fetch/market_intelligence.rs` 末尾:

| # | テスト | 検証内容 |
|---|-------|---------|
| 1 | `test_fetch_occupation_cells_returns_xor_consistent_rows` | measured と estimated_beta の XOR 不変条件 |
| 2 | `test_fetch_ward_thickness_orders_by_index_desc` | thickness_index 降順 |
| 3 | `test_fetch_ward_rankings_by_parent_uses_window_fn` | RANK() OVER の動作確認 (in-memory SQLite) |
| 4 | `test_fetch_code_master_resolves_parent_chain` | parent_code lookup |
| 5 | `test_occupation_cell_dto_label_classifier` | DataSourceLabel 4 分岐 |
| 6 | `test_occupation_cell_dto_xor_consistent` | XOR CHECK の DTO レベル検証 |
| 7 | `test_ward_thickness_priority_validity` | A/B/C/D 以外を弾く |
| 8 | `test_ward_thickness_scenario_index_consistency` | cons ≤ std ≤ agg |
| 9 | `test_fetch_returns_empty_when_turso_and_local_missing` | フェイルセーフ |

### 6.2 HTML スナップショット (`report_html/market_intelligence.rs` 末尾)

| # | テスト | 検証内容 |
|---|-------|---------|
| 1 | `test_render_includes_parent_rank_for_designated_ward` | "市内順位" 文字列 + `parent_rank` 値 |
| 2 | `test_render_uses_index_label_for_estimated_beta` | "推定 β" ラベル & 「人」単位を含まない |
| 3 | `test_render_uses_population_label_for_measured` | 「人」単位を含む & 数字がカンマ区切り |
| 4 | `test_render_does_not_emit_section_for_full_variant` | Full では `mi-root` クラス不在 |
| 5 | `test_render_does_not_emit_section_for_public_variant` | Public でも同様 |
| 6 | `test_render_scenario_bar_does_not_show_population` | バー要素のみ、数値「人」表示なし |
| 7 | `test_render_priority_badge_a_b_c_d` | A/B/C/D 4 種すべて出力可能 |
| 8 | `test_render_industrial_anchor_emoji_only_when_true` | 🏭 表示条件 |

### 6.3 ドメイン不変条件テスト (逆証明)

```rust
#[test]
fn invariant_estimated_beta_never_has_population() {
    // mop の resident × estimated_beta は population IS NULL
    let cell = OccupationCellDto { data_label:"estimated_beta".into(), population: Some(100), estimate_index: Some(140.0), ..default() };
    assert!(!cell.is_xor_consistent(), "estimated_beta + population が同居 → 不変条件違反");
}

#[test]
fn invariant_measured_never_has_estimate_index() { /* 対称 */ }

#[test]
fn invariant_designated_ward_count_per_occupation() {
    // 政令指定区 175 件 × 17 職業 = 2,975 件期待 (実 DB との健全性チェック、CI では skip)
}

#[test]
fn invariant_thickness_index_within_plausible_range() {
    // 0 < thickness_index < 500 (異常値検出、380% 失業率事故の教訓)
}
```

### 6.5 Hard NG 文字列テスト (新規、user 指示)

§2.1.1〜§2.1.2 で禁止した識別子・文字列が **コードベース全体に混入していないこと** を `cargo test` 内で grep ベースで検出する。CI で必ず実行。

```rust
// tests/no_forbidden_terms.rs (新規ファイル、integration test)
use std::fs;
use std::path::Path;

const FORBIDDEN_IDENTIFIERS: &[&str] = &[
    "population_count",
    "target_count",
    "market_size_yen",
    "applicant_count",
    "estimated_population",      // user 指示 (新規)
    "estimated_worker_count",    // user 指示 (新規)
    "resident_population_estimate", // user 指示 (新規)
    "convert_index_to_population",
    "index_to_count",
];

const FORBIDDEN_JA_PHRASES: &[&str] = &[
    "推定人数",        // user 指示 (新規)
    "想定人数",        // user 指示 (新規)
    "母集団人数",      // user 指示 (新規)
    "採用ターゲット候補総数",
    "採用市場規模",
    // 「○人見込み」 (3000人見込み 等) は regex 検出: r"\d+\s*人\s*見込み"
];

#[test]
fn no_forbidden_identifiers_in_src() {
    let mut found = Vec::new();
    visit_dir(Path::new("src"), &mut |path, content| {
        for term in FORBIDDEN_IDENTIFIERS {
            if content.contains(term) {
                found.push(format!("{}: {}", path.display(), term));
            }
        }
    });
    assert!(found.is_empty(), "Hard NG identifiers found:\n{}", found.join("\n"));
}

#[test]
fn no_forbidden_ja_phrases_in_templates_and_strings() {
    let mut found = Vec::new();
    for dir in &["src", "templates", "tests"] {
        visit_dir(Path::new(dir), &mut |path, content| {
            for phrase in FORBIDDEN_JA_PHRASES {
                if content.contains(phrase) {
                    found.push(format!("{}: {}", path.display(), phrase));
                }
            }
            // 「○人見込み」 regex
            let re = regex::Regex::new(r"\d+\s*人\s*見込み").unwrap();
            if let Some(m) = re.find(content) {
                found.push(format!("{}: {}", path.display(), m.as_str()));
            }
        });
    }
    assert!(found.is_empty(), "Hard NG ja phrases found:\n{}", found.join("\n"));
}

#[test]
fn parent_rank_displayed_before_national_rank() {
    // §2.3.1 表示優先順位ルール
    // HTML テンプレートで <parent_rank>...</> が <national_rank>...</> より前に出現
    let templates = read_templates();
    for (path, content) in templates {
        if !content.contains("national_rank") { continue; }
        let p = content.find("parent_rank");
        let n = content.find("national_rank");
        if let (Some(pi), Some(ni)) = (p, n) {
            assert!(pi < ni, "{}: national_rank appears before parent_rank", path);
        }
    }
}
```

このテストファイル (`tests/no_forbidden_terms.rs`) は **cargo test --all で自動実行**、CI ゲートに組み込む。

### 6.4 E2E テスト (Playwright、`tests/e2e/` 配下)

| spec | 検証 |
|------|------|
| `market_intelligence_thickness.spec.ts` | 厚み指数表示、(推定) ラベル必須、人数表示禁止 |
| `market_intelligence_population.spec.ts` | 実測人数表示、(従業地) ラベル必須 |
| `market_intelligence_ranking.spec.ts` | 親市内ランク主表示、全国ランク参考表示 |
| `market_intelligence_variant_isolation.spec.ts` | `?variant=full` / `?variant=public` で Step 5 セクション非表示 |

navigationTimeout 60s (Render cold start 対応、MEMORY ルール)。

---

## 7. fallback / エラーハンドリング

### 7.1 Turso 未接続時

`query_turso_or_local` が空 Vec を返す → 各 fetch 関数も空 Vec → `build_market_intelligence_data` は空 DTO を返す → `render_*` 関数群は既存の `render_mi_placeholder` で「データ準備中」を出す。

### 7.2 部分欠損時

`unwrap_or` 多用ではなく `Option<T>` のまま render し、表示時に `format_opt_*` で `-` に置換。

### 7.3 fallback HTML

```html
<div class="market-intelligence-unavailable">
    Phase 3 Step 5 データが現在取得できません。
    Turso 接続を確認してください: <code>$env:TURSO_EXTERNAL_URL</code>
</div>
```

`render_mi_placeholder` の既存パターン (黄色背景 + ⓘ アイコン) を継承。

### 7.4 invariant 違反検出時の報告

`render_mi_summary_card` 内の `invariant_violation` カウント表示を踏襲。XOR 不整合・priority 不正・scenario 順序逆転を全て集計し、KPI 行末尾に「⚠ 不変条件違反 N 件を表示から除外」と注記。

---

## 8. 実装ロードマップ

| Phase | 作業 | 所要 | 依存 |
|------:|------|:----:|------|
| **0** | **SQL 互換性検証 (RANK/COUNT OVER, PARTITION BY) — 必須前提** | **0.5 日** | **即時** |
| 1 | 既存 Rust 資産の grep + 行番号確定 | 0.5 日 | Phase 0 |
| 2 | DTO 4 種追加 (`OccupationCellDto` / `WardThicknessDto` / `WardRankingRowDto` / `MunicipalityCodeMasterDto`) + 上位 DTO 拡張 | 1 日 | Phase 1 |
| 3 | fetch 関数 4 つ実装 (`fetch_occupation_cells` / `fetch_ward_thickness` / `fetch_ward_rankings_by_parent` / `fetch_code_master`) | 1.5 日 | Phase 2 |
| 4 | HTML render 5 セクション改修 + parent ranking 新規 | 1.5 日 | Phase 3 |
| 5 | variant ガード + 既存テスト維持確認 (`cargo test --all`) | 0.5 日 | Phase 4 |
| 6 | unit + HTML snapshot test (9 + 8 + 4 件) + Hard NG 文字列テスト | 1 日 | Phase 5 |
| 7 | E2E テスト 4 spec | 1 日 | Phase 6 |
| 8 | ローカル動作確認 + Turso 接続テスト + 視覚レビュー (LLM 視覚レビュールール) | 0.5 日 | Phase 7 |
| **合計** | | **~7.5 日** | |

並列化機会: Phase 6 の unit と HTML snapshot は同時並行可。Phase 7 の 4 spec も並列実行可。

### Phase 0: SQL 互換性検証 (新規必須前提)

Turso libSQL の Window Function 互換性は **R1 リスク (既知)** で、本実装の根幹 (`fetch_ward_rankings_by_parent`) を支えるため、**実装着手前に確定検証** を実施する。

#### 0.1 検証対象 SQL 構文

```sql
-- (a) RANK() OVER + PARTITION BY
SELECT municipality_code,
       RANK() OVER (PARTITION BY parent_code ORDER BY thickness_index DESC) AS rk
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
LIMIT 50;

-- (b) COUNT() OVER + PARTITION BY
SELECT municipality_code,
       COUNT(*) OVER (PARTITION BY parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
LIMIT 50;

-- (c) RANK() + COUNT() 同時 (本実装の核心)
SELECT v.municipality_code, mcm.parent_code,
       RANK() OVER (PARTITION BY mcm.parent_code ORDER BY v.thickness_index DESC) AS parent_rank,
       COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
ORDER BY mcm.parent_code, parent_rank
LIMIT 50;
```

#### 0.2 検証手順 (両環境)

```bash
# (i) ローカル SQLite (sqlite3 CLI、3.39+ で window 対応)
sqlite3 data/hellowork.db < /tmp/window_test.sql > /tmp/sqlite_result.txt

# (ii) Turso (READ-only allowlist で curl or audit_turso_upload_diff.py 拡張)
python scripts/verify_turso_window_function.py  # 新規スクリプト (Phase 0 で作成)

# (iii) 結果突合: 両環境で同じ rank/total が出ることを確認
diff /tmp/sqlite_result.txt /tmp/turso_result.txt
```

#### 0.3 検証 PASS 基準

| 確認項目 | 期待 |
|---------|------|
| (a) RANK() OVER (PARTITION BY...) | 動作、各 parent 内 1〜N の rank |
| (b) COUNT(*) OVER (PARTITION BY...) | 動作、各 parent の総数 |
| (c) 同時利用 | 動作、両 column が正しい値 |
| ローカル vs Turso 結果 | 完全一致 |

#### 0.4 PASS の場合 → `fetch_ward_rankings_by_parent` を SQL ベースで実装 (§3.3 の設計通り)

#### 0.5 FAIL (どれか不可) の場合 → Rust 側 fallback 計算

```rust
// fallback: SQL は ORDER BY だけにして、Rust 側で rank 計算
// 1. SELECT * ... ORDER BY parent_code, thickness_index DESC
// 2. Rust で group by parent_code, enumerate で rank, count で total
fn compute_parent_rank_in_rust(rows: Vec<RawThicknessRow>) -> Vec<WardRankingRowDto> {
    let mut grouped: HashMap<String, Vec<RawThicknessRow>> = HashMap::new();
    for r in rows { grouped.entry(r.parent_code.clone()).or_default().push(r); }
    let mut out = Vec::new();
    for (parent_code, mut group) in grouped {
        group.sort_by(|a, b| b.thickness_index.partial_cmp(&a.thickness_index).unwrap());
        let total = group.len() as i64;
        for (i, r) in group.into_iter().enumerate() {
            out.push(WardRankingRowDto {
                parent_rank: (i + 1) as i64,
                parent_total: total,
                ...
            });
        }
    }
    out
}
```

fallback 採用時:
- パフォーマンス: 1,895 unit muni を Rust 側で sort/group → 数 ms 程度の追加コスト、許容範囲
- テスト追加: window 関数を使わないことを保証する unit test

#### 0.6 出力

`docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_SQL_WINDOW_COMPAT.md` (Phase 0 完了時に作成)
- ローカル / Turso での実行結果
- 採用方針 (SQL ベース or Rust fallback)
- 採用後の §3.3 fetch 関数の最終実装方針

---

## 9. レビューチェックリスト

実装 PR レビュー時の必須チェック項目:

- [ ] `OccupationCellDto` に `population_count`/`market_size_yen` 等の禁止フィールドがないか
- [ ] `convert_index_to_population` 等の禁止関数がないか
- [ ] `data_label != 'measured'` で `population` を表示している箇所がないか (template grep)
- [ ] `ReportVariant::Full` / `ReportVariant::Public` のスナップショットテストが diff なく pass するか
- [ ] `?variant=market_intelligence` 以外で Step 5 セクションが出力されないか (E2E variant_isolation.spec で担保)
- [ ] 親市内ランクが主表示、全国ランクが参考表示 (font-size 差異) になっているか
- [ ] 「推定 β」ラベルが estimated_beta セルに必ず付与されているか
- [ ] cargo test 全体 pass + cargo clippy 警告ゼロ
- [ ] LLM 視覚レビュー (実画面スクリーンショット 3 枚以上)
- [ ] Turso 未接続時に panic せず placeholder を返すか
- [ ] 単位の一貫性 (% vs 比率混在なし、2026-04-30 100倍ずれ事故の教訓)

---

## 10. 既知のリスク

### R1: parent_code RANK() OVER の Turso 互換性

Turso (libSQL) の RANK() OVER 対応は新しめ。クエリが想定通り動かない場合のフォールバック:
- アプリ側で全 designated_ward を取得し Rust 側で sort + rank 付与
- 影響: パフォーマンスは僅かに悪化するが行数 175 件レベルなので実用上問題なし
- 検証: Phase 3 のローカル SQLite で window function 動作確認 → Turso staging で再確認

### R2: occupation_code / parent_code の取り回し

`render_survey_report_page_with_variant_v3_themed` (mod.rs L488〜) の引数に `occupation_code` / `parent_code` がまだない。Step 5 では:
- A. ハンドラ層 (handlers.rs) でクエリ抽出してこの関数に渡す (既存シグネチャ変更が必要)
- B. `target_municipalities` の 1 件目から code_master 経由で逆引き (シグネチャ無変更)

推奨: B (Step 5 のスコープ最小化)、Phase 4+ で A に置換。

### R3: 既存 Full/Public スナップショットの偶発破壊

`render_section_market_intelligence` を呼ばない場合でも、`mod.rs` の挿入位置 (Section 13 直前) を間違えると Section 13 の改行・空白が変わって既存テストが落ちる。
- 緩和: `if variant.show_market_intelligence_sections()` ブロックの **前後に余分な改行を入れない**
- 検証: Phase 5 で `cargo test test_render_full_variant` / `test_render_public_variant` 系を必ず通す

### R4: 「推定指数の人数化」誘惑 (hooks 化必須、user 指示)

UI レビュー時に営業サイドから「指数だと顧客に伝わらない、人数換算してほしい」と要望が来る確率が高い。

#### R4.1 多層対策

| 層 | 対策 | 実装場所 |
|---|------|---------|
| 1 | Phase 6 unit test (Hard NG 文字列テスト) | `tests/no_forbidden_terms.rs` (§6.5) |
| 2 | コードレビュー時の必須チェック (PR template) | `.github/pull_request_template.md` |
| 3 | **CLAUDE.md hooks 化 (機械チェック)** ← user 必須 | `.claude/hooks/no_population_terms.sh` |
| 4 | METRICS.md / 本書 §4.5 NG マトリクスでの文書化 | 既存 |

#### R4.2 hooks 化実装案

ユーザー指示の機械的検査:

```bash
#!/usr/bin/env bash
# .claude/hooks/no_population_terms.sh
# Triggered on PreToolUse (Edit/Write) hooks
# Blocks any edit that introduces forbidden terms.

set -e

FORBIDDEN_PATTERNS=(
    "target_count"
    "estimated_population"
    "estimated_worker_count"
    "resident_population_estimate"
    "convert_index_to_population"
    "推定人数"
    "想定人数"
    "母集団人数"
)

# Read tool_input from stdin (Claude Code hook format)
INPUT=$(cat)
NEW_CONTENT=$(echo "$INPUT" | jq -r '.tool_input.new_string // .tool_input.content // ""')

for pat in "${FORBIDDEN_PATTERNS[@]}"; do
    if echo "$NEW_CONTENT" | grep -qF "$pat"; then
        echo "{\"decision\":\"block\",\"reason\":\"Hard NG term '$pat' detected (Phase 3 Step 5 wording guard).\"}" >&2
        exit 2
    fi
done

# Regex pattern: 数字 + 人見込み
if echo "$NEW_CONTENT" | grep -qE '[0-9]+\s*人\s*見込み'; then
    echo "{\"decision\":\"block\",\"reason\":\"'数字+人見込み' pattern matched (Phase 3 Step 5 wording guard).\"}" >&2
    exit 2
fi

exit 0
```

`.claude/settings.json` の hooks に登録:

```json
{
  "hooks": {
    "PreToolUse": [
      {
        "matcher": "Edit|Write|MultiEdit",
        "hooks": [{"type": "command", "command": ".claude/hooks/no_population_terms.sh"}]
      }
    ]
  }
}
```

これにより、Claude が `推定人数` 等を含む編集を試みた瞬間にブロック (CLAUDE.md memory ルール `feedback_hooks_runtime_guard.md` 準拠)。

#### R4.3 hooks 失敗時のエスカレーション

block された場合、Claude はユーザーに以下を報告:

> Hard NG 用語 (例: 推定人数) を含む編集が hooks でブロックされました。
> 商品方針として人数表現は使えません。指数表記 ("ターゲット厚み指数 142") に変更してください。

ユーザーが意図的に解除する場合のみ、`.claude/settings.json` で hooks を一時無効化 (商品方針変更の意思決定として扱う)。

### R5: Turso 接続なし環境でのテスト精度

CI が Turso を持たない場合、unit テストは `query_turso_or_local` のローカルフォールバック側だけで動く。
- 対策: in-memory SQLite で 4 テーブルの最小スキーマを CREATE → INSERT してから fetch を呼ぶテストを Phase 6 で 1 件以上必ず作る (既存 `test_commute_flow_summary_falls_back_to_external_commute_od` のパターン継承)

---

## 参考ドキュメント

- `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` (指標定義)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md` (Phase 0〜2 準備)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md` (XOR CHECK 制約 DDL)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DESIGNATED_WARD_F2_DESIGN.md` (F2 推定モデル仕様)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC_PLAN_B.md` (UI 表示仕様)
- `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md` (EXTERNAL_CLEAN_FILTER 経緯)
- 既存 commit 39e7566 (Step 1〜4 の DTO + variant 基盤)
