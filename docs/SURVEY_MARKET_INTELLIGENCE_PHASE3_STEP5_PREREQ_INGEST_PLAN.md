# Phase 3 Step 5 前提: 事前集計テーブル投入計画

作成日: 2026-05-04
対象: ハローワーク分析システムV2 / Turso V2 + ローカル `data/hellowork.db`

---

## 1. 経緯

Phase 3 Step 5 (実データ統合 = `handlers.rs` から `build_market_intelligence_data` を呼び placeholder を実データに置換) を着手する前に、必要な 4 つの事前集計テーブルが Turso V2 に存在するかを確認した (READ-only、4 READ 消費)。

### 確認結果 (2026-05-04 実施)

| テーブル | Turso V2 状態 | DTO 整合 |
|---------|:-----------:|:--------:|
| `municipality_recruiting_scores` | ❌ 不在 | 未確認 (テーブル不在) |
| `municipality_living_cost_proxy` | ❌ 不在 | 未確認 |
| `commute_flow_summary` | ❌ 不在 | 未確認 |
| `municipality_occupation_population` | ❌ 不在 | 未確認 |

**4/4 テーブルが不在** → ユーザー判断基準「1 つでも不在 → Step 5 実装は停止し、事前集計テーブル投入計画へ戻る」に該当。

### 現状の Step 3 動作

`?variant=market_intelligence` でアクセスした場合、`SurveyMarketIntelligenceData::default()` (空 Vec) を渡しているため、5+1 セクションすべてが「データ準備中」placeholder で表示される。HTML 枠組みは正常動作。

---

## 2. テーブル別投入計画

### 2.1 `municipality_occupation_population` (最優先・最大規模)

#### データソース
e-Stat 国勢調査 / 職業 (大分類) × 男女 × 年齢 × 市区町村別 統計 (statsDataId 要調査)

#### 既存資産
- `scripts/fetch_industry_structure.py` (経済センサス) のパターンが流用可能 (途中再開対応)
- `scripts/fetch_census_demographics.py` (既存) で人口は取得済 → 職業×年齢×性別はクロス集計版を別 fetch

#### 投入方針
**A 案 (推奨): 新規 fetch スクリプト作成**
- `scripts/fetch_occupation_population.py` (新規) で e-Stat API 経由で取得
- 出力: `scripts/data/occupation_population_by_municipality.csv`
- 投入: `import_ssdse_to_db.py` 拡張または専用 import

**B 案: 既存 v2_external_population_pyramid + 職業マッピング推定**
- 既存テーブル `v2_external_population_pyramid` (年齢×性別) と職業構成比 (都道府県別) を掛け合わせて推定
- 精度低下するが新規 fetch 不要

#### 推定行数
1,800 市区町村 × 21 職業大分類 × 8 年齢階級 × 3 性別 = **約 907,200 行** (常住地ベースのみ)
従業地ベースも入れると 約 1.8M 行 → Turso row writes 上限 (25M/月) の **7%**

#### スキーマ (Phase 0〜2 docs §schema)
```sql
CREATE TABLE municipality_occupation_population (
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis TEXT NOT NULL,           -- resident | workplace
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    age_group TEXT NOT NULL,
    gender TEXT NOT NULL,          -- male | female | total
    population INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL,
    source_name TEXT NOT NULL DEFAULT 'census',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (municipality_code, basis, occupation_code, age_group, gender, source_year)
);
```

DDL 投入: `docs/survey_market_intelligence_phase0_2_schema.sql` を `sqlite3 data/hellowork.db < ...sql` で実行 (**ユーザー手動**)。

---

### 2.2 `commute_flow_summary` (中規模・既存データから派生)

#### データソース
**既存** `v2_external_commute_od` (Turso V2 反映済 / 83,402 行 / 2026-05-04 アップロード) から TOP N 抽出 + 3 シナリオ推定計算。

#### 投入方針 (推奨)
新規 Python スクリプト `scripts/build_commute_flow_summary.py`:
1. `v2_external_commute_od` から目的地市区町村ごとに `total_commuters DESC LIMIT 20` を抽出
2. 各 OD ペアに `flow_share = total_commuters / SUM(total_commuters by destination)` を計算
3. 推定流入数 (保守 1% / 標準 3% / 強気 5%) を `total_commuters * scenario_rate` で算出 (METRICS.md §9 準拠)
4. CSV 出力 → Turso 投入

#### 推定行数
1,800 目的地市区町村 × 20 流入元 = **約 36,000 行**

#### スキーマ
DTO 互換 (`docs/survey_market_intelligence_phase0_2_schema.sql:67-96`)。**`origin_municipality_code` カラムは v2_external_commute_od に存在しない** (origin_pref + origin_muni のみ) ため、`municipality_geocode` テーブルで name → code 変換が必要。

#### 依存
- `v2_external_commute_od` ✅ 既投入済
- `municipality_geocode` (name → code 変換マスター) ← 別途確認必要

---

### 2.3 `municipality_living_cost_proxy` (中規模・既存統計から派生)

#### データソース
- 住宅・土地統計 (家賃 proxy): 既存 `scripts/fetch_geo_supplement.py` 系の追加実装
- 小売物価統計: e-Stat
- `v2_external_household_spending` (既投入待ち、Phase 3 前処理 Task A 参照): 都市別年支出
- `v2_external_land_price` (既投入待ち): 都道府県×用途別地価

#### 投入方針
新規 `scripts/build_municipality_living_cost_proxy.py`:
1. e-Stat 住宅・土地統計 API で家賃中央値 (1R / 1LDK 相当) を取得
2. 都市レベル `household_spending` を市区町村レベルに按分 (人口比 / 既存 `v2_external_population` 利用)
3. 県レベル地価を市区町村に展開
4. 全国順位 `housing_cost_rank` を `RANK() OVER (ORDER BY single_household_rent_proxy)` で算出
5. CSV 出力 → Turso 投入

#### 推定行数
1,800 市区町村 = **約 1,800 行**

#### スキーマ
DTO 互換 (`schema.sql:44-64`)。

#### 依存
- e-Stat 住宅・土地統計 (要 statsDataId 調査)
- 投入順: Phase 3 前処理 Task A (household_spending / land_price) 完了後

---

### 2.4 `municipality_recruiting_scores` (最終派生・最後に作る)

#### データソース
**全 3 テーブルの計算結果 + 既存 HW + SalesNow** から派生。METRICS.md §2 の計算式に基づく:

```
positive_score =
  (target_population_index * 30
 + adjacent_population_index * 20
 + commute_reach_index * 25
 + effective_wage_index * 25) / 100
penalty_reduction_pct =
  (job_competition_density_index * 20
 + commute_burden_index * 10) / 100
distribution_priority_score =
  clamp(positive_score * (1 - penalty_reduction_pct / 100), 0, 100)
```

#### 投入方針
新規 `scripts/build_municipality_recruiting_scores.py`:
1. 入力: `municipality_occupation_population` + `municipality_living_cost_proxy` + `commute_flow_summary` + 既存 HW (`v2_postings`/集計) + 既存 SalesNow (`v2_salesnow_companies`)
2. 各市区町村 × 職業グループで指数 (0-100) を計算
3. 配信優先度スコアを clamp(0, 100) で算出
4. 母集団シナリオ (保守/標準/強気) を計算 (METRICS.md §9 の式)
5. CSV 出力 → Turso 投入

#### 推定行数
1,800 市区町村 × 21 職業大分類 = **約 37,800 行**

#### スキーマ
DTO 互換 (`schema.sql:100-138`)。

#### 依存 (重要)
**前 3 テーブルすべての投入後でないと計算不能。**

---

## 3. 推奨投入順序

依存関係グラフ:

```
[既存] v2_external_commute_od (Turso ✅)
   ↓ TOP N 抽出 + シナリオ推定
[Step A] commute_flow_summary
                                          ↘
[既存待ち] v2_external_household_spending  → [Step B] municipality_living_cost_proxy
[既存待ち] v2_external_land_price                                ↘
                                                                 → [Step D] municipality_recruiting_scores
[新規 fetch] e-Stat 国勢調査 職業×年齢×性別
   ↓
[Step C] municipality_occupation_population                        ↗
                                          [既存] v2_postings + SalesNow ↗
```

**実行順 (推奨)**:
1. **Step A**: `commute_flow_summary` (既存 v2_external_commute_od から派生、最速)
2. **Step C**: `municipality_occupation_population` (新規 fetch、最大規模、最も時間がかかる)
3. **Step B**: `municipality_living_cost_proxy` (Phase 3 前処理 Task A の household_spending / land_price 完了が前提)
4. **Step D**: `municipality_recruiting_scores` (上記すべて + 既存 HW + SalesNow を統合)

並行可能: A と C は独立。

---

## 4. 各 Step の所要時間見積

| Step | 内容 | 推定実装時間 | 推定実行時間 |
|:----:|------|------------|------------|
| A | `commute_flow_summary` 派生スクリプト | 半日 (Python) | 数分 |
| C | `municipality_occupation_population` fetch + DDL + 投入 | 1〜2 日 (e-Stat API 調査含む) | **1〜3 時間** (fetch) |
| B | `municipality_living_cost_proxy` 派生 | 1 日 (住宅統計 API 調査含む) | 数十分 |
| D | `municipality_recruiting_scores` 計算 | 1 日 (METRICS.md §2 式実装) | 数分 |
| **合計** | | **約 3〜5 日** | **約 2〜4 時間** |

---

## 5. ブロッカーと判断ポイント

| ブロッカー | 状態 | 対応 |
|-----------|------|------|
| Phase 3 前処理 Task A の不在 6 テーブル投入完了 | 🟡 部分完了 (2026-05-04 時点で commute_od + minimum_wage のみ反映) | household_spending / land_price / labor_stats / industry_structure / establishments / salesnow 投入が並行で必要 |
| `municipality_geocode` テーブル (name → code 変換) | 🟡 要確認 (既存にあるか SELECT で確認) | 不在ならマスター作成 |
| e-Stat 国勢調査 職業×年齢×性別 statsDataId | 🔴 未調査 | API ドキュメント調査が Step C 前提 |
| METRICS.md §2 式の重み係数の業務承認 | 🟡 仮値 (30/20/25/25/-20/-10) で運用 | Step D 実装後にユーザーレビュー |

---

## 6. Step 5 着手の最低条件

**4 テーブルすべてが Turso V2 に投入済 + DTO スキーマと整合**

部分着手案 (フォールバック):
- `commute_flow_summary` のみ投入 → MarketIntelligence variant の通勤流入元セクションだけ実データで動作 (他 4 セクションは placeholder のまま)
- ただし配信優先度・母集団レンジは未表示。**ユーザー判断「実データ版レポートには 4 テーブル必要」と整合しないため非推奨**

---

## 7. 推奨次アクション (Phase 3 Step 5 着手前)

| # | アクション | 担当 | 優先度 |
|--:|-----------|------|--------|
| 1 | DDL 投入 (`schema.sql` を `sqlite3 data/hellowork.db < ...` 実行) → 4 テーブルを **空のまま** 作成 | ユーザー手動 | 高 |
| 2 | `scripts/build_commute_flow_summary.py` 新規実装 | 実装担当 | 高 |
| 3 | e-Stat 国勢調査 職業×年齢×性別 statsDataId 調査 | 実装担当 | 高 |
| 4 | `scripts/fetch_occupation_population.py` 新規実装 + 投入 | 実装担当 | 中 (時間かかる) |
| 5 | Phase 3 前処理 Task A の household_spending / land_price 投入 (`upload_to_turso.py`) | ユーザー手動 | 中 |
| 6 | `scripts/build_municipality_living_cost_proxy.py` 新規実装 | 実装担当 | 中 |
| 7 | `scripts/build_municipality_recruiting_scores.py` 新規実装 (METRICS.md §2 式) | 実装担当 | 低 (最後) |
| 8 | 4 テーブル Turso 投入 → `verify_turso_v2_sync.py` で MATCH 確認 → Step 5 着手 | ユーザー手動 + Claude | 最終 |

---

## 8. 暫定対応案 (Step 5 を急ぐ場合)

### 案 X: モックデータで Step 5 着手
- 4 テーブルにダミーデータ 10〜100 行を投入
- DTO 経路 + UI レンダリングを実データで通す動作確認のみ
- 業務的には無価値だが、E2E テストに使える

### 案 Y: Step 3 placeholder 維持で Step 6 (Sankey 実装) や Step 7 (印刷 CSS 調整) に進む
- データ投入を後回しにし、UI 改善を先行
- 実データなしでも進捗が見える

ユーザー判断:
- ✅ **推奨**: §7 のアクションを進めて 4 テーブル投入後に Step 5 (本格実装)
- ⚠️ 急ぐ場合: 案 X (モックデータ) または 案 Y (UI 先行)

---

## 9. 完了条件 (本書の)

- [x] 4 テーブルの Turso V2 存在確認 (READ-only 4 READ 消費)
- [x] 不在 4 件を確定
- [x] 投入計画 (Step A-D + 依存関係 + 所要時間) 記載
- [x] 推奨次アクション 8 件 + 暫定対応 2 案
- [x] ユーザー判断材料を docs として残す

Step 5 実装には進まず、本書をもって Phase 3 前処理 Round 2 (事前集計テーブル投入) を起動する。
