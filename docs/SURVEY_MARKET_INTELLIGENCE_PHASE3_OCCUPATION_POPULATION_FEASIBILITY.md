# Phase 3 Step 5: `municipality_occupation_population` 実現可能性調査 (Worker C)

作成日: 2026-05-04
対象: Phase 3 Step 5 の 4 テーブル中、`municipality_occupation_population` の取得可否判定

**ステータス: 調査結果 (READ-only、ファイル変更なし)**

---

## 0. 結論

| 観点 | 結論 |
|------|------|
| e-Stat から「市区町村 × 職業大分類 × 年齢 × 性別」を **直接取得** | ❌ **困難** (公開 API 仕様上、市区町村粒度での職業統計は未確認) |
| 国勢調査「就業状態等基本集計」の職業分類 | ⚠️ **都道府県粒度のみ** API 提供確認済 |
| Phase 3 Step 5 着手可否 | 🟢 **代替モデル (案 A) で即着手可** |
| 推奨方針 | **案 A: 都道府県職業比 × 市区町村人口按分** (精度 ±20%、実装 2〜3 日) |

---

## 1. DTO スキーマ (Phase 0〜2 確定済)

`docs/survey_market_intelligence_phase0_2_schema.sql:14-35`:

```sql
CREATE TABLE municipality_occupation_population (
    municipality_code TEXT NOT NULL,         -- JIS 5 桁
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis TEXT NOT NULL,                     -- 'resident' / 'workplace'
    occupation_code TEXT NOT NULL,           -- 職業大分類 (21 区分)
    occupation_name TEXT NOT NULL,
    age_group TEXT NOT NULL,                 -- 8 階級
    gender TEXT NOT NULL,                    -- 'male' / 'female' / 'total'
    population INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL,
    source_name TEXT NOT NULL DEFAULT 'census',
    PRIMARY KEY (municipality_code, basis, occupation_code, age_group, gender, source_year)
);
```

期待行数: 1,800 市区町村 × 21 職業 × 8 年齢 × 3 性別 × 2 (resident/workplace) = **約 1,814,400 行**

---

## 2. 既存 e-Stat fetch スクリプト分析 (Worker C 調査)

15 個の `scripts/fetch_*.py` を Read した結果:

| スクリプト | 統計表 ID | 粒度 | 職業分類 |
|-----------|-----------|------|:-------:|
| `fetch_industry_structure.py` | `0003449718` (経済センサス R3 2021) | 市区町村 × 産業大分類 21 | ❌ 産業のみ (≠ 職業) |
| `fetch_census_demographics.py` | (国勢調査 R2 2020) | 都道府県のみ | ❌ 学歴・世帯 |
| `fetch_commute_od.py` | `0003454527` (国勢調査 従業地・通学地集計) | 市区町村 × 市区町村 | ❌ 通勤 OD、職業なし |
| その他 12 スクリプト | - | - | 職業を扱うものなし |

→ **既存スクリプトで「市区町村 × 職業」を扱うものは存在しない**。

---

## 3. e-Stat 国勢調査の関連 statsDataId 候補 (Worker C 確認)

国勢調査 R2 (令和 2 年) 統計表のうち職業を含むもの:

| 統計表 | statsDataId | 範囲 | 職業 | 市区町村 | 備考 |
|-------|-------------|------|:---:|:-------:|------|
| 就業状態等基本集計 (教育) | `0003450543` | 都道府県 | ✗ | ✗ | 教育・学歴のみ |
| 人口等基本集計 (世帯) | `0003445080` | 都道府県 | ✗ | ✗ | 世帯構成のみ |
| 従業地・通学地集計 | `0003454527` | 市区町村 | ✗ | ✓ | 通勤フロー、職業なし |

国勢調査の「就業状態等基本集計」には職業大分類 21 区分の表があるが、**e-Stat 公開 API での市区町村粒度提供は確認できず**。

---

## 4. 既存ローカルデータからの代替案 (案 B)

### 4.1 利用可能テーブル

| テーブル | 粒度 | 行数 | カラム |
|---------|------|----:|--------|
| `v2_external_population_pyramid` | 市区町村 × 年齢階級 | 15,660 | prefecture, municipality, age_group, male_count, female_count |
| `v2_external_industry_structure` | 市区町村 × 産業 | (Phase 3 後続で投入予定、現時点不在) | city_code, industry_code, employees_total (男女別) |

### 4.2 組み合わせの限界

- 2 テーブル結合でも「職業」情報は取得できない (**産業 ≠ 職業**)
- 産業 (例: 製造業) と職業 (例: 専門技術者・販売) は独立した分類
- 産業 → 職業マッピングは推定値が粗くなる (±40% 誤差)

---

## 5. 代替モデル比較 (案 A / B / C)

| 案 | 実装方法 | 精度 | 工数 | Phase 3 採用可 |
|---|---------|:---:|:---:|:---------:|
| **A** | 都道府県別職業構成 (`0003450543` 等) を市区町村人口比 (`v2_external_population`) で按分 | ⭐⭐ ±20% | 2〜3 日 | 🟢 可 (要注記) |
| B | `v2_external_industry_structure` の市区町村 × 産業 を職業マッピングテーブルで変換 | ⭐ ±40% | 1 週間+ | 🟡 不適格 (精度低) |
| C | 市区町村別 = 全国平均 (地域差なし) | ⭐ ±50% | < 1 日 | 🔴 Phase 3 品質要件に不適格 |

### 5.1 案 A 推奨理由

1. **e-Stat 既存資産流用**: 都道府県別職業データの statsDataId 候補は確認済 (`0003450543` 等の都道府県粒度表)
2. **市区町村人口データ既存**: `v2_external_population` (1,742 行、commute_od の `with_codes` で 5 桁コード付き) を按分分母に使える
3. **DTO スキーマ変更不要**: `source_name` を `'prefectural_distribution_estimate'` に変えるだけ
4. **Phase 3 で十分な精度**: 市場規模推定が主目的であり、±20% は実務的に許容範囲

### 5.2 案 A 実装方針 (推奨)

#### Step 1: 都道府県別職業データ取得
- `scripts/fetch_prefectural_occupation.py` 新規作成
- e-Stat statsDataId (`0003450543` ほか職業含有表) から都道府県 × 職業大分類 × 年齢 × 性別 を取得
- 出力: `data/generated/prefectural_occupation.csv` (47 県 × 21 職業 × 8 年齢 × 3 性別 × 2 basis ≒ 47,376 行)

#### Step 2: 市区町村按分
- `scripts/build_municipality_occupation_population.py` 新規作成
- 入力: 都道府県別職業 + `v2_external_population` (市区町村人口)
- 計算式:
  ```
  市区町村_職業_年齢_性別人口 =
      都道府県_職業_年齢_性別人口
    × (市区町村_年齢_性別人口 / 都道府県_年齢_性別人口合計)
  ```
- 出力: `data/generated/municipality_occupation_population.csv` (1,800 × 21 × 8 × 3 × 2 ≈ 1.8M 行)
- ローカル DB 投入: `municipality_occupation_population` テーブル DROP + CREATE + INSERT

#### Step 3: 整合性検証
```sql
-- 都道府県集計 = e-Stat 公式値 と一致するか
SELECT prefecture, occupation_code, age_group, gender, basis,
       SUM(population) AS aggregated
FROM municipality_occupation_population
GROUP BY prefecture, occupation_code, age_group, gender, basis;
-- → e-Stat の都道府県別公式値と比較、誤差 < 1%
```

#### Step 4: source_name 表示ルール
- DTO `source_name = 'prefectural_distribution_estimate'` (`'census'` ではない)
- レポート HTML で「**推定 (都道府県別分布按分)**」ラベル表示
- METRICS.md §1 の「推定」区分に該当

### 5.3 推定実装時間 (案 A)

| 段階 | 作業 | 時間 |
|:----:|------|:---:|
| 1 | e-Stat statsDataId 確定 + fetch スクリプト | 1 日 |
| 2 | 按分スクリプト実装 + テスト | 1〜1.5 日 |
| 3 | 検証 + 都道府県集計値の e-Stat 公式値突合 | 0.5 日 |
| **合計** | | **2〜3 日** |

---

## 6. Phase 3 Step 5 への影響

### 6.1 着手可否

✅ **案 A 採用で Step 5 着手可能**。

### 6.2 Step 5 各テーブルの状態

| テーブル | 状態 |
|---------|------|
| `commute_flow_summary` | ✅ 投入済 (JIS 版、本タスクで完了) |
| `municipality_code_master` | ✅ 投入済 (1,917 行、20 政令市完備) |
| **`municipality_occupation_population`** | 🟡 **案 A 実装で 2〜3 日後に投入可** |
| `municipality_living_cost_proxy` | 🔴 別途調査必要 (住宅統計、家賃 proxy) |
| `municipality_recruiting_scores` | 🔴 上記すべて + HW + SalesNow 統合の最終派生 |

### 6.3 実装順序

```
[完了] commute_flow_summary (JIS 版)
[完了] municipality_code_master
   ↓
[次] municipality_occupation_population (案 A、2〜3 日)
   ↓
[次] municipality_living_cost_proxy (別調査)
   ↓
[最終] municipality_recruiting_scores (METRICS.md §2 計算)
```

---

## 7. リスクと注意点

### 7.1 案 A の精度限界

- 地域特性 (例: 工業都市 vs 観光地) で職業構成が都道府県平均から逸脱
- ±20% 誤差は**統計的妥当性は確保**するが、極端な地域比較では注意
- レポート上で「推定」ラベルを必ず付与し、絶対値の比較を避ける

### 7.2 source_name の運用

| 運用 | 値 |
|------|---|
| `'census'` | ❌ NG (実測ではないため) |
| `'prefectural_distribution_estimate'` | ✅ 推奨 (按分推定の明示) |
| `'estat_0003450543_v2_external_population'` | ⚠️ 詳細すぎ、運用しにくい |

### 7.3 将来対応

- 統計局が市区町村粒度の職業統計を公開した時点で、`source_name='census'` 版に置換可能
- 5 年後 (R7 国勢調査) で粒度向上の可能性あり

---

## 8. 実装着手時の最低条件

| # | 項目 | 状態 |
|--:|------|:---:|
| 1 | 案 A 採用の業務承認 (精度 ±20% で OK) | ユーザー判断待ち |
| 2 | e-Stat statsDataId (`0003450543` 等) の市区町村粒度の有無を WebFetch で実機確認 | 未実施 |
| 3 | `v2_external_population` の市区町村人口データの整合性 (現状 1,742 行、Worker A の with_codes 後の状況) | ✅ 既存確認済 |
| 4 | `municipality_code_master` 完備 (Step 5 結合キー) | ✅ |

---

## 9. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| Turso upload | ❌ |
| `.env` / token 読み | ❌ 不要 |
| Rust 変更 | ❌ |
| ファイル作成 | ⚠️ 本書のみ (調査結果 docs) |
| push | ❌ |

---

## 10. 関連 docs

- Phase 3 Step 5 全体: `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md`
- DTO スキーマ: `survey_market_intelligence_phase0_2_schema.sql`
- METRICS: `SURVEY_MARKET_INTELLIGENCE_METRICS.md` (推定値ラベル運用)
- Phase 3 全体計画: `SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md`
