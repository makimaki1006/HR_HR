# Phase 3 JIS 整備: `municipality_code_master` DDL + 生成ロジック (Worker B)

作成日: 2026-05-04
対象: 改修後 OD データから `municipality_code_master` テーブルを派生し、`prefecture + municipality_name → municipality_code` 逆引きを可能にする

**ステータス: 設計提示のみ (DDL/SELECT/INSERT 未実行)**

---

## 1. 目的

Worker A 設計で投入予定の `v2_external_commute_od_with_codes` (JIS 5 桁コード保持) から、`prefecture + municipality_name → municipality_code` のマスタを派生する。

これにより:
- `commute_flow_summary` の擬似コード版を JIS 版に UPDATE 可能
- Step 5 の他 3 テーブル (`municipality_recruiting_scores` / `municipality_living_cost_proxy` / `municipality_occupation_population`) の `municipality_code` カラムに JIS を投入可能
- 既存 `v2_external_*` テーブル (名称ベース) と Step 5 テーブル (code ベース) の **JOIN ブリッジ** として機能

---

## 2. DDL 案 (v2: area_type / area_level 対応版、2026-05-04 改訂)

### 2.0 改訂理由

Worker A 改修で投入される `v2_external_commute_od_with_codes` には、e-Stat の cdArea 仕様により以下のような **集約地域コード** が含まれる可能性がある:
- `13100` = 「特別区部」 (東京 23 区を集約した擬似自治体)
- `01100` = 「札幌市」 (政令市本体、`endswith("000")` でないため除外されない)
- `27100` = 「大阪市」 (同上)

これらと通常の市区町村 (`01101` 札幌市中央区等) を **明示的に区別する分類カラム** が必要。

### 2.1 area_level / area_type 定義

| `area_type` | `area_level` | 説明 | コード例 |
|-----------|------------|------|---------|
| `municipality` | `unit` | 一般市/町/村 (政令市・特別区以外) | `13201` 八王子市、`24202` 四日市市 |
| `designated_ward` | `unit` | 政令指定都市の区 | `01101` 札幌市中央区、`27102` 大阪市都島区 |
| `special_ward` | `unit` | 東京都特別区 (23 区) | `13101` 千代田区〜`13123` 江戸川区 |
| `aggregate_city` | `aggregate` | 政令市本体 (区を束ねた集約) | `01100` 札幌市、`27100` 大阪市 |
| `aggregate_special_wards` | `aggregate` | 特別区部 (23 区を束ねた集約) | `13100` 特別区部 |

**area_level**:
- `unit`: 集計の最小単位 (1 行 = 1 自治体)
- `aggregate`: 複数 unit を束ねた集約地域 (区計 / 市本体)

**集計時の使い分け**:
- 純粋な市区町村単位の分析: `WHERE area_level = 'unit'`
- 都市単位 (政令市を 1 行で扱う): `WHERE area_type IN ('municipality', 'special_ward', 'aggregate_city')` (区を捨て、市本体を採用)
- 全データ: フィルタなし (集約と単位の両方を保持)

### 2.2 DDL

```sql
CREATE TABLE IF NOT EXISTS municipality_code_master (
    municipality_code TEXT PRIMARY KEY,           -- JIS 5 桁 [正本キー]
    prefecture TEXT NOT NULL,                     -- 表示用都道府県名
    municipality_name TEXT NOT NULL,              -- 表示用市区町村名
    pref_code TEXT NOT NULL,                      -- 上位 2 桁 (47 都道府県フィルタ用)
    -- 分類 (Worker B1 改訂: area_type / area_level 追加)
    area_type TEXT NOT NULL CHECK (area_type IN (
        'municipality',
        'designated_ward',
        'special_ward',
        'aggregate_city',
        'aggregate_special_wards'
    )),
    area_level TEXT NOT NULL CHECK (area_level IN ('unit', 'aggregate')),
    -- 旧フラグ (後方互換、area_type から派生)
    is_special_ward INTEGER NOT NULL DEFAULT 0,   -- area_type='special_ward' なら 1
    is_designated_ward INTEGER NOT NULL DEFAULT 0,-- area_type='designated_ward' なら 1
    -- 集約と unit の親子関係 (オプション、aggregate のみ NULL 以外)
    parent_code TEXT,                             -- unit 行が aggregate に属する場合の親 code
    -- メタ情報
    source TEXT NOT NULL DEFAULT 'estat_commute_od',
    source_year INTEGER NOT NULL DEFAULT 2020,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
);

-- 逆引き高速化: (prefecture, municipality_name) → code
-- 注意: 同一 prefecture 内で同名は禁止だが、aggregate と unit が同名のことはない
CREATE UNIQUE INDEX IF NOT EXISTS idx_mcm_pref_muni
ON municipality_code_master (prefecture, municipality_name);

-- 都道府県別フィルタ
CREATE INDEX IF NOT EXISTS idx_mcm_pref_code
ON municipality_code_master (pref_code);

-- area_type / area_level 別フィルタ (Step 5 各テーブルで頻用される想定)
CREATE INDEX IF NOT EXISTS idx_mcm_area_type ON municipality_code_master (area_type);
CREATE INDEX IF NOT EXISTS idx_mcm_area_level ON municipality_code_master (area_level);

-- 親 (aggregate) → 子 (unit) 検索
CREATE INDEX IF NOT EXISTS idx_mcm_parent ON municipality_code_master (parent_code);
```

### 2.3 設計メモ

| カラム | 型 | 用途 |
|--------|---|------|
| `municipality_code` | TEXT (PK) | **正本キー**。5 桁 JIS。先頭 2 桁が都道府県、後 3 桁が市区町村+区 |
| `prefecture` | TEXT NOT NULL | 表示用 (例: `"北海道"`) |
| `municipality_name` | TEXT NOT NULL | 表示用。集約地域も独立 (例: `"特別区部"`、`"札幌市"`、`"札幌市中央区"`) |
| `pref_code` | TEXT NOT NULL | `SUBSTR(municipality_code, 1, 2)`。47 都道府県別フィルタ用 |
| **`area_type`** | TEXT (CHECK) | 5 値: `municipality` / `designated_ward` / `special_ward` / `aggregate_city` / `aggregate_special_wards` |
| **`area_level`** | TEXT (CHECK) | `unit` または `aggregate` (粗い分類) |
| `is_special_ward` | INTEGER | 後方互換。`area_type='special_ward'` のとき 1 |
| `is_designated_ward` | INTEGER | 後方互換。`area_type='designated_ward'` のとき 1 |
| **`parent_code`** | TEXT | unit が属する aggregate の code (例: 札幌市中央区 `01101` の parent_code = 札幌市 `01100`) |
| `source` | TEXT | データソース |
| `source_year` | INTEGER | データ取得年 |

### 2.4 制約

- **PK = `municipality_code`**: **正本キー**。5 桁コードで完全一意 (UNIQUE 制約は冗長だが PK で担保)
- **`UNIQUE (prefecture, municipality_name)`**: 逆引き用 (同一都道府県内で同名禁止)
  - 注意: 全国では同名市区町村あり (例: 「府中市」東京都/広島県、「伊達市」北海道/福島県) → 都道府県内なら一意
- **`CHECK area_type IN (...)`**: 5 値以外の混入を DB レベルで防止
- **`CHECK area_level IN ('unit', 'aggregate')`**: 同上
- **`is_special_ward` / `is_designated_ward`**: `area_type` から派生 (CHECK で整合性チェック可、INSERT 時に自動計算推奨)

### 2.5 area_type 判定ロジック (INSERT 時の派生)

```python
def derive_area_type(code: str, prefecture: str, municipality_name: str) -> tuple[str, str, str | None]:
    """5 桁 code から area_type / area_level / parent_code を派生。"""
    pref_code = code[:2]
    suffix = code[2:5]  # 後 3 桁

    # 集約: 札幌市 (01100) や大阪市 (27100) は後 3 桁が "100"
    if suffix == "100" and pref_code != "13":
        return "aggregate_city", "aggregate", None

    # 集約: 特別区部 (13100)
    if code == "13100":
        return "aggregate_special_wards", "aggregate", None

    # 特別区: 13101〜13123
    if pref_code == "13" and "101" <= suffix <= "123":
        return "special_ward", "unit", "13100"  # 親 = 特別区部

    # 政令市の区: pref_code != 13 かつ name に "市" + "区" を含む (例: 札幌市中央区)
    if pref_code != "13" and "市" in municipality_name and municipality_name.endswith("区"):
        # 親 = 同じ pref_code の "100" コード (政令市本体)
        parent = pref_code + "100"
        return "designated_ward", "unit", parent

    # 一般市町村
    return "municipality", "unit", None
```

注意: 政令市の区を含む `municipality_name` は「札幌市中央区」のような形式を想定。e-Stat 出力が「中央区」だけなら判定ロジック調整必要 (実データを Worker A 改修後の `v2_external_commute_od_with_codes` で確認)。

---

## 3. 生成ロジック (派生 SQL)

### 3.1 入力前提

Worker A 改修完了 + e-Stat 再 fetch 完了後の `v2_external_commute_od_with_codes` テーブル。

### 3.2 派生 SQL

```sql
-- Step 1: origin 側から DISTINCT 抽出
INSERT OR IGNORE INTO municipality_code_master (
    municipality_code, prefecture, municipality_name, pref_code,
    is_special_ward, is_designated_ward,
    source, source_year
)
SELECT DISTINCT
    origin_municipality_code,
    origin_prefecture,
    origin_municipality_name,
    SUBSTR(origin_municipality_code, 1, 2),
    -- 特別区: 東京都 (pref_code='13') かつ municipality_name が「○○区」で終わる
    CASE WHEN SUBSTR(origin_municipality_code, 1, 2) = '13'
              AND origin_municipality_name LIKE '%区'
              AND SUBSTR(origin_municipality_code, 3, 1) = '1'
         THEN 1 ELSE 0 END,
    -- 政令市の区: 都道府県以外で municipality_name が「○○市○○区」
    CASE WHEN origin_municipality_name LIKE '%市%区'
              AND SUBSTR(origin_municipality_code, 1, 2) != '13'
         THEN 1 ELSE 0 END,
    'estat_commute_od',
    2020
FROM v2_external_commute_od_with_codes
WHERE origin_municipality_code IS NOT NULL
  AND origin_municipality_code != '00000'
  AND NOT origin_municipality_code LIKE '__000';   -- 都道府県集計除外

-- Step 2: dest 側からも DISTINCT 抽出 (origin に含まれない自治体を補完)
INSERT OR IGNORE INTO municipality_code_master (
    municipality_code, prefecture, municipality_name, pref_code,
    is_special_ward, is_designated_ward,
    source, source_year
)
SELECT DISTINCT
    dest_municipality_code,
    dest_prefecture,
    dest_municipality_name,
    SUBSTR(dest_municipality_code, 1, 2),
    CASE WHEN SUBSTR(dest_municipality_code, 1, 2) = '13'
              AND dest_municipality_name LIKE '%区'
              AND SUBSTR(dest_municipality_code, 3, 1) = '1'
         THEN 1 ELSE 0 END,
    CASE WHEN dest_municipality_name LIKE '%市%区'
              AND SUBSTR(dest_municipality_code, 1, 2) != '13'
         THEN 1 ELSE 0 END,
    'estat_commute_od',
    2020
FROM v2_external_commute_od_with_codes
WHERE dest_municipality_code IS NOT NULL
  AND dest_municipality_code != '00000'
  AND NOT dest_municipality_code LIKE '__000';
```

`INSERT OR IGNORE` で同一 PK は無視 (origin/dest 両方で出現する自治体は 1 行)。

---

## 4. 生成ロジック (Python スクリプト案)

直接 SQL でも可能だが、検証ログ出力のため Python スクリプト化:

```python
# scripts/build_municipality_code_master.py (新規)
import sqlite3, sys, io
sys.stdout.reconfigure(encoding="utf-8")

DB = r"data/hellowork.db"
SOURCE_TABLE = "v2_external_commute_od_with_codes"
TARGET = "municipality_code_master"

conn = sqlite3.connect(DB)
cur = conn.cursor()

# 1. テーブル作成
cur.executescript(open("docs/.../master_ddl.sql").read())  # 上記 DDL

# 2. origin/dest から DISTINCT 抽出
sql = f"""
INSERT OR IGNORE INTO {TARGET} (...)
SELECT DISTINCT
    {col_code}, {col_pref}, {col_muni},
    SUBSTR({col_code}, 1, 2),
    CASE WHEN ... THEN 1 ELSE 0 END,  -- is_special_ward
    CASE WHEN ... THEN 1 ELSE 0 END,  -- is_designated_ward
    'estat_commute_od', 2020
FROM {SOURCE_TABLE}
WHERE {col_code} IS NOT NULL
  AND {col_code} != '00000'
  AND NOT {col_code} LIKE '__000'
"""
for prefix in ["origin", "dest"]:
    cur.execute(sql.replace("{col_code}", f"{prefix}_municipality_code")
                   .replace("{col_pref}", f"{prefix}_prefecture")
                   .replace("{col_muni}", f"{prefix}_municipality_name"))
conn.commit()

# 3. 検証
print(f"行数: {cur.execute(f'SELECT COUNT(*) FROM {TARGET}').fetchone()[0]:,}")
print(f"特別区: {cur.execute(f'SELECT COUNT(*) FROM {TARGET} WHERE is_special_ward = 1').fetchone()[0]}")
print(f"政令市の区: {cur.execute(f'SELECT COUNT(*) FROM {TARGET} WHERE is_designated_ward = 1').fetchone()[0]}")
print(f"DISTINCT pref_code: {cur.execute(f'SELECT COUNT(DISTINCT pref_code) FROM {TARGET}').fetchone()[0]}")
```

---

## 5. 検証 SQL

### 5.1 整合性検証

```sql
-- 1. 行数 (期待: 1,900〜2,000 程度。市区町村 1,747 + 特別区 23 + 政令市の区 175 ≈ 1,945)
SELECT COUNT(*) FROM municipality_code_master;

-- 2. PK 一意 (UNIQUE 制約により自動保証だが念のため)
SELECT COUNT(*) - COUNT(DISTINCT municipality_code) FROM municipality_code_master;
-- 期待: 0

-- 3. (prefecture, municipality_name) 一意
SELECT prefecture, municipality_name, COUNT(*) AS c
FROM municipality_code_master
GROUP BY prefecture, municipality_name
HAVING c > 1;
-- 期待: 0 件

-- 4. 47 都道府県カバレッジ
SELECT COUNT(DISTINCT pref_code) FROM municipality_code_master;
-- 期待: 47

-- 5. pref_code が PREF_NAMES 47 件と完全一致
SELECT pref_code, prefecture, COUNT(*) AS muni_count
FROM municipality_code_master
GROUP BY pref_code, prefecture
ORDER BY pref_code;
-- 期待: 47 行、各 pref_code が一意の prefecture と対応

-- 6. 特別区の数 (東京都 23 区、'13101'〜'13123')
SELECT COUNT(*) FROM municipality_code_master WHERE is_special_ward = 1;
-- 期待: 23

-- 7. 政令市の区 (約 175 区: 札幌市10/仙台市5/さいたま市10/千葉市6/横浜市18/川崎市7/相模原市3/新潟市8/静岡市3/浜松市7/名古屋市16/京都市11/大阪市24/堺市7/神戸市9/岡山市4/広島市8/北九州市7/福岡市7/熊本市5)
SELECT COUNT(*) FROM municipality_code_master WHERE is_designated_ward = 1;
-- 期待: 約 175

-- 8. (将来) JIS コードの大分類別カウント
SELECT
    SUBSTR(municipality_code, 3, 1) AS code_class,
    COUNT(*) AS c
FROM municipality_code_master
GROUP BY code_class
ORDER BY code_class;
-- 期待:
--   0: 都道府県集計 (除外済のため 0 件)
--   1: 政令指定都市/市/特別区 (大半)
--   2-3: 中核市・特例市
--   4-5: 一般市
--   6-7: 町
--   8-9: 村
```

### 5.2 ブリッジ動作検証

```sql
-- 既存 v2_external_population (名称ベース) を JIS 経由で読めるか
SELECT
    pop.prefecture,
    pop.municipality,
    mcm.municipality_code,
    pop.total_population
FROM v2_external_population AS pop
LEFT JOIN municipality_code_master AS mcm
  ON mcm.prefecture = pop.prefecture
  AND mcm.municipality_name = pop.municipality
WHERE pop.municipality IS NOT NULL
  AND pop.municipality != ''
  AND mcm.municipality_code IS NULL
LIMIT 20;
-- 期待: 0 件 (全レコードがブリッジ可能)
-- もし出るなら、名称揺れ (例: '一宮市' (愛知/千葉) のような同名問題、
-- または合併済み旧自治体が pop に残っている等) → 個別調査
```

---

## 6. 想定行数 (e-Stat 国勢調査 R2 ベース)

| カテゴリ | 推定行数 |
|---------|---------:|
| 一般市・町・村 | 約 1,718 |
| 東京都特別区 | 23 |
| 政令指定都市の区 | 約 175 |
| 北方領土等の村 (国勢調査外) | 0 (e-Stat 未対応) |
| **合計** | **約 1,916** |

実機で取得後、§5.1 のクエリで実数確認。

---

## 7. JOIN 利用例 (Step 5 後続テーブルでの活用)

### 7.1 配信地域ランキング SQL

```sql
-- municipality_recruiting_scores (Step 5 で投入予定) と表示名を結合
SELECT
    mrs.distribution_priority_score,
    mcm.prefecture,
    mcm.municipality_name,
    mrs.target_population
FROM municipality_recruiting_scores AS mrs
JOIN municipality_code_master AS mcm
  ON mcm.municipality_code = mrs.municipality_code
WHERE mrs.occupation_group_code = 'driver'
ORDER BY mrs.distribution_priority_score DESC
LIMIT 20;
```

### 7.2 通勤流入元検索 SQL

```sql
-- 札幌市中央区 (01101) への流入 TOP 5
SELECT
    org.prefecture AS origin_pref,
    org.municipality_name AS origin_muni,
    cod.total_commuters
FROM v2_external_commute_od_with_codes AS cod
JOIN municipality_code_master AS dst
  ON dst.municipality_code = cod.dest_municipality_code
JOIN municipality_code_master AS org
  ON org.municipality_code = cod.origin_municipality_code
WHERE dst.municipality_code = '01101'  -- 札幌市中央区
  AND cod.origin_municipality_code != cod.dest_municipality_code
ORDER BY cod.total_commuters DESC
LIMIT 5;
```

→ 名称揺れリスクなく、code で確実に JOIN。

---

## 8. 推定実装時間

| 作業 | 時間 |
|------|:----:|
| DDL 確定 (本書) | 完了 |
| Python スクリプト実装 (`build_municipality_code_master.py`) | 30 分 |
| 検証 SQL 実行 | 30 分 |
| 結果確認 + 名称揺れ補正 (必要なら) | 30 分 |
| **合計** | **約 1.5 時間** |

Worker A の `fetch_commute_od.py` 改修 + e-Stat 再 fetch 完了が前提。

---

## 9. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| DDL 実行 | ❌ 設計のみ (実行はユーザー手動) |
| INSERT 実行 | ❌ 同上 |
| Turso upload | ❌ |
| `.env` / token 読み | ❌ 不要 |
| Rust 実装 | ❌ |
| push | ❌ |

---

## 10. 完了条件 (本書の)

- [x] DDL 案 (PK + 2 INDEX、型定義、デフォルト値含む)
- [x] 生成ロジック (Step 1+2 の `INSERT OR IGNORE` + Python 案)
- [x] 検証 SQL (整合性 8 件 + ブリッジ動作 1 件)
- [x] 想定行数 (約 1,916)
- [x] 7 つの JOIN 利用例 (Step 5 着手時に流用可)
- [x] 推定実装時間 (約 1.5h)

---

## 11. 関連 docs

- 改修案 (前提): `SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_COMMUTE_OD_REFACTOR.md` (Worker A)
- 移行設計 (後続): `SURVEY_MARKET_INTELLIGENCE_PHASE3_BUILD_COMMUTE_FLOW_JIS_MIGRATION.md` (Worker C)
- 全体計画: `SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md` の §2.1 Step 4 と整合
