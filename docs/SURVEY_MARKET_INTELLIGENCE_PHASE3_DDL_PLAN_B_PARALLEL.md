# Phase 3 DDL — Plan B 並列保管方針 (`municipality_occupation_population` 改訂)

**作成日**: 2026-05-04
**Worker**: B4
**Status**: 設計確定 (DB 投入なし)
**対象 Phase**: Phase 3 → Phase 5 投入時に使用

---

## 1. 方針サマリ

e-Stat 15-1 実測データ (workplace) と F2 推定データ (resident / workplace fallback) を **同一テーブル内に並列保管** する。区別は `basis` × `data_label` の二軸で行い、人数 (`population`) と指数 (`estimate_index`) を排他列として保持する。

採用厚み指数 (Worker A2 設計の `v2_municipality_target_thickness`) は集約版として独立保持し、本テーブルから ETL で派生させる。

### 1.1 テーブル分担方針 (二テーブル並立)

| テーブル | 内容 | 用途 |
|---------|------|------|
| `municipality_occupation_population` (本書、改訂) | basis × age × gender × occ の人数 (workplace=実測 / resident=推定) | 採用配信時のターゲット粒度詳細 |
| `v2_municipality_target_thickness` (Worker A2 既設計) | 厚み指数 / ランク / 濃淡 (basis × occ) | UI ダッシュボードのサマリ・配信優先度 |

両テーブルは `basis × municipality_code × occupation_code` で結合可能。`v2_municipality_target_thickness` は **派生テーブル** として `municipality_occupation_population` から導出する (ETL ステップ追加)。

---

## 2. `municipality_occupation_population` 改訂 DDL

```sql
CREATE TABLE municipality_occupation_population (
    -- 結合キー
    municipality_code TEXT NOT NULL,            -- JIS 5 桁
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,

    -- 軸
    basis             TEXT NOT NULL CHECK (basis IN ('workplace','resident')),
    occupation_code   TEXT NOT NULL,            -- 大分類 'A'..'L' (13 区分、総数/分類不能除外で 11)
    occupation_name   TEXT NOT NULL,
    age_class         TEXT NOT NULL,            -- '15-19','20-24',...,'85+', or '_total'
    gender            TEXT NOT NULL CHECK (gender IN ('male','female','total')),

    -- 値 (排他)
    population        INTEGER,                  -- 実測 = 人数 / 推定 = NULL
    estimate_index    REAL,                     -- 推定 = 0-200 / 実測 = NULL

    -- メタデータ
    data_label        TEXT NOT NULL CHECK (data_label IN ('measured','estimated_beta')),
    source_name       TEXT NOT NULL,            -- 'census_15_1' / 'model_f2_v1' / 'census_resident_xxx'
    source_year       INTEGER NOT NULL,         -- 2020 (census) / 2026 (model)
    weight_source     TEXT,                     -- estimated_beta 時のみ 'hypothesis_v1' or 'estat_R2_xxx'

    -- 鮮度
    estimated_at      TEXT NOT NULL DEFAULT (datetime('now')),

    -- 排他制約: measured ↔ population、estimated_beta ↔ estimate_index
    CHECK (
      (data_label = 'measured'        AND population IS NOT NULL AND estimate_index IS NULL) OR
      (data_label = 'estimated_beta'  AND population IS NULL     AND estimate_index IS NOT NULL)
    ),

    PRIMARY KEY (municipality_code, basis, occupation_code, age_class, gender, source_year, data_label)
);

CREATE INDEX idx_muni_occ_pop_pref   ON municipality_occupation_population (prefecture, municipality_name);
CREATE INDEX idx_muni_occ_pop_basis  ON municipality_occupation_population (basis, occupation_code);
CREATE INDEX idx_muni_occ_pop_label  ON municipality_occupation_population (data_label);
CREATE INDEX idx_muni_occ_pop_source ON municipality_occupation_population (source_name, source_year);
CREATE INDEX idx_muni_occ_pop_age    ON municipality_occupation_population (age_class);
```

### 2.1 列サマリ

- 結合キー: 3 列
- 軸: 5 列
- 値: 2 列 (排他)
- メタデータ: 4 列
- 鮮度: 1 列
- **計 15 列**

### 2.2 制約サマリ

| 種別 | 数 | 内容 |
|------|---:|------|
| CHECK | 3 | basis ∈ {workplace,resident} / gender ∈ {male,female,total} / data_label 排他 (population vs estimate_index) |
| PRIMARY KEY | 1 | 7 列複合 |
| INDEX | 5 | pref/basis/label/source/age |

---

## 3. data_label 設計の整合性ルール

| basis | data_label | population | estimate_index | source_name 例 |
|-------|-----------|:---------:|:--------------:|----------------|
| `workplace` | `measured` | INTEGER (実数) | NULL | `census_15_1` |
| `workplace` | `estimated_beta` | NULL | REAL (0-200) | `model_f2_v1` (15-1 fallback) |
| `resident` | `estimated_beta` | NULL | REAL (0-200) | `model_f2_v1` |
| `resident` | `measured` | (将来) INTEGER | NULL | `census_resident_xxx` (Phase 5+) |

### 3.1 表示ポリシー (Plan B コア)

- `data_label = 'measured'` → UI で人数表示 OK (採用配信ターゲット数として実用)
- `data_label = 'estimated_beta'` → UI で **指数 / 濃淡のみ表示** (人数禁止、`measured 比較ベンチマーク (β)` バッジ必須)

---

## 4. 行数見積

| basis | data_label | 行数見積 | 内訳 |
|------|-----------|---------:|------|
| workplace | measured (15-1) | ~1.0–1.2M | 1,965 muni × 22 age × 2 gender × 11 occ (NULL 除外で減少見込) |
| workplace | estimated_beta (F2 fallback) | ~38,000 | 1,742 muni × 11 occ × (age=`_total`, gender=`total`) |
| resident | estimated_beta (F2) | ~38,000 | 同上 |
| **合計** |  | **~1.1–1.3M 行** |  |

**補足**: F2 は age × gender 粒度を持たないため `age_class='_total'` / `gender='total'` で 1 行/(muni×occ×basis) とする。15-1 実測のみ完全粒度 (22 age × 2 gender)。

---

## 5. 既存 DDL からの差分 (Worker A2 旧 `municipality_occupation_population`)

| 項目 | 旧 (A2) | 新 (本書) | 理由 |
|------|---------|-----------|------|
| `population` | `INTEGER NOT NULL DEFAULT 0` | `INTEGER` (NULL 許容) | 推定行は人数 NULL |
| `estimate_index` | (列なし) | `REAL` (新規) | 推定値の指数表現 |
| `data_label` | (列なし) | `TEXT NOT NULL CHECK` | measured / estimated_beta 区別 |
| `weight_source` | (列なし) | `TEXT` | F2 重み出所トレース |
| `estimated_at` | (列なし) | `TEXT DEFAULT datetime('now')` | 鮮度管理 |
| `source_name` | `DEFAULT 'census'` | CHECK 自由 (census_15_1 / model_f2_v1 等) | 並列ソース対応 |
| PRIMARY KEY | 6 列 (label なし) | 7 列 (`data_label` 追加) | 同 muni/age/gender 内で実測+推定 2 行可 |
| 排他 CHECK | なし | 1 制約追加 | population/estimate_index 整合性 |
| INDEX | 3 (pref/basis/source) | 5 (+ label, age) | data_label / age_class クエリ用 |

### 5.1 既存 `v2_municipality_target_thickness` への影響

Worker A2 推奨どおり残す。位置づけを変更:

- 旧: `municipality_occupation_population` の代替集約
- 新: `municipality_occupation_population` から ETL で導出される **派生テーブル**

ETL 順序: `15-1 実測 + F2 推定` → `municipality_occupation_population` → 集計 → `v2_municipality_target_thickness`

---

## 6. SQL クエリ例

### 6.1 採用配信ターゲット (workplace 実測の人数)

```sql
SELECT prefecture, municipality_name, age_class, gender, population
FROM municipality_occupation_population
WHERE basis = 'workplace'
  AND data_label = 'measured'
  AND occupation_code = 'H'                     -- 例: 生産工程
  AND age_class IN ('25-29','30-34','35-39')
  AND prefecture = '愛知県'
ORDER BY population DESC
LIMIT 20;
```

### 6.2 常住地ターゲット (resident 推定の指数)

```sql
SELECT prefecture, municipality_name, estimate_index
FROM municipality_occupation_population
WHERE basis = 'resident'
  AND data_label = 'estimated_beta'
  AND occupation_code = 'H'
  AND age_class = '_total'
  AND gender = 'total'
ORDER BY estimate_index DESC
LIMIT 20;
```

### 6.3 同一 muni × occ で実測と推定を並べる

```sql
SELECT
  m.municipality_code,
  m.municipality_name,
  SUM(CASE WHEN data_label='measured'        THEN population     END)      AS measured_pop,
  MAX(CASE WHEN data_label='estimated_beta'  THEN estimate_index END)      AS f2_index
FROM municipality_occupation_population m
WHERE basis = 'workplace' AND occupation_code = 'H'
GROUP BY m.municipality_code, m.municipality_name;
```

---

## 7. 移行戦略

| 段階 | 作業 | Status |
|------|------|--------|
| (a) | Worker A2 旧 `municipality_occupation_population` DDL は **未投入なので破棄** (本テーブルが置換) | 計画 |
| (b) | 新 DDL を `docs/survey_market_intelligence_phase3_ddl_v2.sql` として整備 | Phase 5 着手時 |
| (c) | Worker A2 `TABLE_RENAME_DECISION.md` を **`v2_municipality_target_thickness` 専用** に修正範囲を絞る | Phase 5 着手時 |
| (d) | 本書を `municipality_occupation_population` の **正式 DDL** として確定 | 本書で完了 |

---

## 8. 投入手順 (Phase 5 で実装)

1. **CSV 1 (15-1 実測)**: `data/generated/estat_15_1.csv` (Worker A4 計画)
   → `INSERT ... WHERE data_label='measured'`
2. **CSV 2 (F2 推定)**: `data/generated/v2_municipality_target_thickness.csv` (Worker B3 スケルトン) を粒度展開
   → `INSERT ... WHERE data_label='estimated_beta'`
3. **検証 SQL**:
   - `SELECT data_label, basis, COUNT(*) FROM municipality_occupation_population GROUP BY 1,2;`
   - NULL 整合性: `SELECT COUNT(*) FROM ... WHERE data_label='measured' AND population IS NULL;` → 0 期待
   - 排他 CHECK 検証: `SELECT COUNT(*) FROM ... WHERE population IS NOT NULL AND estimate_index IS NOT NULL;` → 0 期待

---

## 9. 制約事項 (本書作成スコープ)

- DB 書き込みなし (本書は設計のみ)
- 既存 DDL ファイル変更禁止 (本書 1 ファイルのみ作成)
- Turso 接続不要
- `.env` 直接 open 禁止

---

## 10. 関連ドキュメント

- `SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_FEASIBILITY.md`
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_MODEL_V2.md`
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md`
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_SENSITIVITY_ANALYSIS.md`
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_ESTAT_15_1_FEASIBILITY.md`
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC.md` (人数表示ポリシー)
