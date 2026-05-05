# Phase 3 ローカル DB 投入・検証手順書 (Plan B 並列保管)

**Worker C6 出力 / 2026-05-04**

本書は Plan B 体制下での `municipality_occupation_population` テーブルへのローカル投入と検証手順をまとめる。**実投入はユーザー手動**、本書は手順 + 検証 SQL のみを提供する。Turso upload は対象外 (Phase 8)。

---

## 0. 対象データと方針

| CSV 出力元 | ファイル | basis | data_label | 行数 (期待) |
|---|---|---|---|---|
| Worker A6 | `data/generated/estat_15_1_merged.csv` | `workplace` | `measured` | ~1.0-1.2M |
| Worker B6 | `data/generated/v2_municipality_target_thickness.csv` | `resident` | `estimated_beta` | ~19,162 (1,742 muni × 11 occ) |

- 派生指標 (rank_in_occupation, rank_percentile, distribution_priority, scenario_*_index) は **`v2_municipality_target_thickness` テーブル**に投入。`municipality_occupation_population` には入れない。
- 制約: `taskkill` 禁止、`.env` 直接 open 禁止、Claude による DB 書き込み禁止 → 本書は手順書のみ。

---

## 1. 前提条件チェック (DDL 反映確認)

```bash
sqlite3 data/hellowork.db "SELECT name FROM sqlite_master WHERE name='municipality_occupation_population';"
# 期待: 1 行 (テーブル存在)

sqlite3 data/hellowork.db "PRAGMA table_info(municipality_occupation_population);"
# 期待: 15 列
#   municipality_code, prefecture, municipality_name,
#   basis, occupation_code, occupation_name, age_class, gender,
#   population, estimate_index, data_label,
#   source_name, source_year, weight_source, estimated_at
```

`v2_municipality_target_thickness` も同様に DDL 反映済みであること:

```bash
sqlite3 data/hellowork.db "PRAGMA table_info(v2_municipality_target_thickness);"
# 期待: thickness_index, rank_in_occupation, rank_percentile, distribution_priority,
#       scenario_conservative_index, scenario_standard_index, scenario_aggressive_index,
#       is_industrial_anchor, estimate_grade, source_year, estimated_at, weight_source
#       など派生指標列を含む
```

DDL が未反映の場合: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md` を先に適用する。

---

## 2. CSV ファイル存在確認

```bash
ls -la data/generated/estat_15_1_merged.csv
ls -la data/generated/v2_municipality_target_thickness.csv
```

両方が存在し、行数が期待レンジ内であることを確認:

```bash
wc -l data/generated/estat_15_1_merged.csv          # 期待: 1,000,001 - 1,200,001 行 (header 含む)
wc -l data/generated/v2_municipality_target_thickness.csv  # 期待: 19,163 行 (header 含む)
```

---

## 3. 15-1 実測 CSV 投入手順

### 3.1 カラムマッピング

Worker A6 出力 CSV のカラム:

```
municipality_code, prefecture, municipality_name, gender, age_class,
occupation_code, occupation_name, population, source_name, source_year, fetched_at
```

`municipality_occupation_population` への投入時:

| 投入先 | 値 |
|---|---|
| 直接マップ | `municipality_code, prefecture, municipality_name, gender, age_class, occupation_code, occupation_name, population, source_name, source_year` |
| 固定値 | `basis = 'workplace'`, `data_label = 'measured'` |
| NULL | `estimate_index`, `weight_source` |
| 変換 | `estimated_at = fetched_at` |

### 3.2 投入コマンド A: Python ワンライナー (推奨)

```python
import pandas as pd
import sqlite3
from datetime import datetime

df = pd.read_csv('data/generated/estat_15_1_merged.csv')
df['basis'] = 'workplace'
df['data_label'] = 'measured'
df['estimate_index'] = None
df['weight_source'] = None
df['estimated_at'] = df.get('fetched_at', datetime.now().isoformat())

df = df[[
    'municipality_code','prefecture','municipality_name',
    'basis','occupation_code','occupation_name','age_class','gender',
    'population','estimate_index','data_label',
    'source_name','source_year','weight_source','estimated_at'
]]

with sqlite3.connect('data/hellowork.db') as conn:
    df.to_sql('municipality_occupation_population', conn, if_exists='append', index=False)
    print(f'inserted {len(df):,} rows (workplace x measured)')
```

### 3.3 投入コマンド B: sqlite3 CLI (代替)

```bash
sqlite3 data/hellowork.db <<'EOF'
.mode csv
.import --skip 1 data/generated/estat_15_1_merged.csv tmp_15_1
INSERT INTO municipality_occupation_population (
  municipality_code, prefecture, municipality_name,
  basis, occupation_code, occupation_name, age_class, gender,
  population, estimate_index, data_label,
  source_name, source_year, weight_source, estimated_at
)
SELECT
  municipality_code, prefecture, municipality_name,
  'workplace', occupation_code, occupation_name, age_class, gender,
  CAST(population AS INTEGER), NULL, 'measured',
  source_name, CAST(source_year AS INTEGER), NULL, fetched_at
FROM tmp_15_1;
DROP TABLE tmp_15_1;
EOF
```

---

## 4. F2 推定 CSV 投入手順

### 4.1 Plan B 制約事前検証

Worker B6 出力 CSV は Plan B 制約に従っているはず:

- `basis = 'resident'` のみ
- `data_label = 'estimated_beta'` のみ
- `age_class = '_total'` のみ (年齢分解しない)
- `gender = 'total'` のみ (性別分解しない)
- 11 occupation × 1,742 municipality = 19,162 行

### 4.2 投入コマンド (Python)

```python
import pandas as pd
import sqlite3

df = pd.read_csv('data/generated/v2_municipality_target_thickness.csv')

# Plan B 制約の事前検証 (assert で投入前に止める)
assert (df['basis'] == 'resident').all(), 'basis must be resident'
assert (df['data_label'] == 'estimated_beta').all(), 'data_label must be estimated_beta'
assert (df['age_class'] == '_total').all(), 'age_class must be _total'
assert (df['gender'] == 'total').all(), 'gender must be total'
assert df['estimate_index'].notna().all(), 'estimate_index must not be NULL'

# --- (a) municipality_occupation_population への投入分 ---
mop_df = df[[
    'municipality_code','prefecture','municipality_name',
    'basis','occupation_code','occupation_name','age_class','gender',
    'estimate_index','data_label',
    'source_name','source_year','weight_source','estimated_at'
]].copy()
mop_df['population'] = None  # estimated_beta は population NULL 必須

mop_df = mop_df[[
    'municipality_code','prefecture','municipality_name',
    'basis','occupation_code','occupation_name','age_class','gender',
    'population','estimate_index','data_label',
    'source_name','source_year','weight_source','estimated_at'
]]

with sqlite3.connect('data/hellowork.db') as conn:
    mop_df.to_sql('municipality_occupation_population', conn, if_exists='append', index=False)
    print(f'inserted {len(mop_df):,} rows (resident x estimated_beta) into mop')

# --- (b) v2_municipality_target_thickness への派生指標投入分 ---
v2_df = df[[
    'municipality_code','prefecture','municipality_name',
    'basis','occupation_code','occupation_name',
    'estimate_index',
    'rank_in_occupation','rank_percentile','distribution_priority',
    'scenario_conservative_index','scenario_standard_index','scenario_aggressive_index',
    'is_industrial_anchor','source_year','estimated_at','weight_source',
]].copy()
v2_df = v2_df.rename(columns={'estimate_index': 'thickness_index'})
v2_df['estimate_grade'] = 'A-'  # Worker A3 sensitivity 確認済み

with sqlite3.connect('data/hellowork.db') as conn:
    v2_df.to_sql('v2_municipality_target_thickness', conn, if_exists='append', index=False)
    print(f'inserted {len(v2_df):,} rows into v2_municipality_target_thickness')
```

---

## 5. 検証 SQL (実投入後 — 9 検証項目)

### 5.1 行数検証

```sql
-- workplace x measured (15-1 実測)
SELECT COUNT(*) FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured';
-- 期待: 1,000,000 - 1,200,000

-- resident x estimated_beta (F2 推定)
SELECT COUNT(*) FROM municipality_occupation_population
WHERE basis='resident' AND data_label='estimated_beta';
-- 期待: 19,162

-- 全体合計
SELECT COUNT(*) FROM municipality_occupation_population;
-- 期待: 1,019,162 - 1,219,162

-- v2 派生指標テーブル
SELECT COUNT(*) FROM v2_municipality_target_thickness;
-- 期待: 19,162
```

### 5.2 PK 重複検査

```sql
SELECT municipality_code, basis, occupation_code, age_class, gender, source_year, data_label,
       COUNT(*) AS dup_count
FROM municipality_occupation_population
GROUP BY 1,2,3,4,5,6,7
HAVING dup_count > 1
LIMIT 5;
-- 期待: 0 行
```

### 5.3 ラベル整合性検証 (XOR CHECK 制約)

```sql
-- measured 行: population NOT NULL かつ estimate_index NULL でなければならない
SELECT COUNT(*) FROM municipality_occupation_population
WHERE data_label='measured'
  AND (population IS NULL OR estimate_index IS NOT NULL);
-- 期待: 0 (CHECK 制約で挿入時に拒否される)

-- estimated_beta 行: 逆 (population NULL かつ estimate_index NOT NULL)
SELECT COUNT(*) FROM municipality_occupation_population
WHERE data_label='estimated_beta'
  AND (population IS NOT NULL OR estimate_index IS NULL);
-- 期待: 0
```

### 5.4 軸完全性検証 (basis × data_label)

```sql
SELECT basis, data_label, COUNT(*) AS rows
FROM municipality_occupation_population
GROUP BY basis, data_label
ORDER BY basis, data_label;
-- 期待: 2 行のみ
--   resident  | estimated_beta | ~19,162
--   workplace | measured       | ~1.0-1.2M
```

### 5.5 年齢/性別/職業コード検証

```sql
-- 15-1 実測の軸
SELECT DISTINCT age_class FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured'
ORDER BY age_class;
-- 期待: 14-22 区分 (15-19, 20-24, ..., 85+ 等)

SELECT DISTINCT gender FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured';
-- 期待: 'male', 'female' (total は除外済み)

SELECT DISTINCT occupation_code FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured'
ORDER BY occupation_code;
-- 期待: 11 区分 (大分類 A-K または 01-11)

-- F2 推定の軸 (Plan B 制約で限定)
SELECT DISTINCT age_class, gender FROM municipality_occupation_population
WHERE basis='resident' AND data_label='estimated_beta';
-- 期待: 1 行のみ (_total, total)
```

### 5.6 source_name 検証

```sql
SELECT source_name, COUNT(*)
FROM municipality_occupation_population
GROUP BY source_name
ORDER BY source_name;
-- 期待:
--   census_15_1   ~1.0-1.2M
--   model_f2_v1   ~19,162
```

### 5.7 weight_source 検証

```sql
SELECT weight_source, data_label, COUNT(*)
FROM municipality_occupation_population
GROUP BY weight_source, data_label;
-- 期待:
--   NULL          | measured       | ~1.0-1.2M
--   hypothesis_v1 | estimated_beta | ~19,162
```

### 5.8 数値妥当性 (ドメイン不変条件)

```sql
-- 全国就業者総数 (実測) — ドメイン不変条件
SELECT SUM(population) AS total_workers
FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured';
-- 期待: 50,000,000 - 70,000,000 (令和2年国勢調査の全国就業者 ~6,758万)
-- 範囲外なら異常 (集計漏れ or 二重計上)

-- 推定指数のレンジ
SELECT
  MIN(estimate_index) AS min_idx,
  MAX(estimate_index) AS max_idx,
  AVG(estimate_index) AS avg_idx
FROM municipality_occupation_population
WHERE basis='resident' AND data_label='estimated_beta';
-- 期待: MIN >= 0, MAX <= 200, AVG ≈ 100
-- AVG が 100 から大きくずれたら正規化バグの可能性
```

### 5.9 master 突合 (orphan 検出)

```sql
SELECT COUNT(DISTINCT mop.municipality_code) AS orphan_count
FROM municipality_occupation_population mop
LEFT JOIN municipality_code_master mcm
  ON mop.municipality_code = mcm.municipality_code
WHERE mcm.municipality_code IS NULL;
-- 期待: 0 or 極小 (政令市の区など一時的に発生し得る)
```

---

## 6. 投入失敗時の rollback 手順

CHECK 制約違反やデータ不整合を検出した場合、source 単位で全件削除:

```sql
-- 15-1 実測のみ rollback
DELETE FROM municipality_occupation_population
WHERE source_name='census_15_1' AND source_year=2020;

-- F2 推定のみ rollback
DELETE FROM municipality_occupation_population
WHERE source_name='model_f2_v1' AND source_year=2026;

-- v2 派生指標 rollback
DELETE FROM v2_municipality_target_thickness
WHERE source_year=2026;
```

`VACUUM` は SQLite 用途では任意 (DB 肥大が気になれば実行)。

---

## 7. 投入順序の推奨

```
[Step 1] DDL 反映確認 (本書 §1, §2)
[Step 2] 15-1 CSV 投入 (本書 §3) → §5.1 / §5.6 で 15-1 のみ事前確認
[Step 3] F2 CSV 投入 (本書 §4)   → §5.1 〜 §5.9 で全項目検証
[Step 4] エラー検出時 → §6 rollback、なければ完了
```

**重要**: Step 2 で異常を検出した場合は Step 3 に進まず、§6 の 15-1 rollback のみを実行して停止する。

---

## 8. (オプション) テストデータ投入で CHECK 制約を事前確認

実投入前にダミーデータで挙動確認したい場合:

```sql
-- (1) 正常 measured (挿入成功するはず)
INSERT INTO municipality_occupation_population VALUES
('13104','東京都','新宿区','workplace','08','生産工程','15-19','male',
 1234, NULL, 'measured', 'census_15_1', 2020, NULL, '2026-05-04T00:00:00');

-- (2) 正常 estimated_beta (挿入成功するはず)
INSERT INTO municipality_occupation_population VALUES
('13104','東京都','新宿区','resident','08','生産工程','_total','total',
 NULL, 142.5, 'estimated_beta', 'model_f2_v1', 2026, 'hypothesis_v1', '2026-05-04T00:00:00');

-- (3) CHECK 違反 (population と estimate_index 同時セット) → 拒否されるはず
INSERT INTO municipality_occupation_population VALUES
('13104','東京都','新宿区','workplace','08','生産工程','15-19','male',
 1234, 99.0, 'measured', 'census_15_1', 2020, NULL, '2026-05-04T00:00:00');
-- 期待: CHECK constraint failed エラー

-- (4) ダミー rollback
DELETE FROM municipality_occupation_population
WHERE prefecture='東京都' AND municipality_name='新宿区' AND occupation_code='08';
```

(3) が通ってしまう場合は DDL の CHECK 制約が反映されていないので、`PRAGMA table_info` および DDL ドキュメントを再確認すること。

---

## 9. 投入後の commit/監査 (将来)

- 本書では扱わないが、将来的には:
  - データ投入後の SHA256 ハッシュチェック (再投入の冪等性確認)
  - Turso upload は別タスク (Phase 8) — `LIMIT/REPLACE` ではなく **1 回で完了** すること (`feedback_turso_upload_once.md` 準拠)

---

## 10. 制約・前提

| 項目 | 値 |
|---|---|
| Claude DB 書き込み | 禁止 (本書は手順書のみ) |
| Turso 接続 | 不要 (Phase 8 で実施) |
| `.env` 直接 open | 禁止 |
| 新規ファイル | 本書 1 つのみ |
| 投入実行者 | ユーザー手動 |
| 想定 DB | `data/hellowork.db` (ローカル SQLite) |

---

**ファイルパス**: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_LOCAL_INGEST_VALIDATION.md`
