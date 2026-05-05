# Phase 3: Designated Ward 投入手順書 + 検証 SQL

**Worker**: C8
**Date**: 2026-05-04
**Status**: 手順書のみ (本投入は Worker A8 fetch + B8 実装後)

## 1. ロールアウト前提条件

| # | 前提 | 確認方法 |
|--:|------|---------|
| 1 | Worker A8: `fetch_estat_resident_population.py --validate` PASS | A8 報告書 |
| 2 | Worker B8: `compute_designated_ward_rolldown` 本実装完了 | B8 報告書 |
| 3 | `municipality_code_master` の designated_ward 175 件が既存 | `SELECT COUNT(*) FROM municipality_code_master WHERE area_type='designated_ward'` = 175 (確認済) |
| 4 | 既存 mop workplace × measured = 709,104 行 (15-1 投入済) | `SELECT COUNT(*) FROM municipality_occupation_population WHERE basis='workplace' AND data_label='measured'` |
| 5 | 既存 v2_external_population = 1,742 行 | `SELECT COUNT(*) FROM v2_external_population` |

---

## 2. 投入順序 (4 ステップ)

```
[Step 1] e-Stat sid=0003445080 fetch
         → data/generated/estat_resident_merged.csv
[Step 2] estat_resident_merged.csv ingest
         → v2_external_population (designated_ward 175 行 append)
         → v2_external_population_pyramid (同 175 件 × 年齢階級)
[Step 3] F2 再計算
         → data/generated/v2_municipality_target_thickness.csv (20,845 行)
[Step 4] mop + v2_thickness 投入
         → mop resident × estimated_beta: 18,920 → 20,845 行
         → v2_municipality_target_thickness: 18,920 → 20,845 行
```

---

## 3. Step 2 投入手順 (Python ワンライナー)

```python
import pandas as pd
import sqlite3

# 既存 v2_external_population に designated_ward 行を append
df = pd.read_csv('data/generated/estat_resident_merged.csv', dtype={'municipality_code': str})

# master JOIN designated_ward のみ (municipality_code 経由で表記揺れ吸収)
with sqlite3.connect('file:data/hellowork.db?mode=ro', uri=True) as conn:
    designated = pd.read_sql(
        "SELECT municipality_code, prefecture, municipality_name "
        "FROM municipality_code_master WHERE area_type='designated_ward'",
        conn
    )
df = df.merge(designated, on='municipality_code', how='inner')
assert df['municipality_code'].nunique() == 175, f"only {df['municipality_code'].nunique()}/175"

# v2_external_population スキーマ集計
agg = df.groupby(['prefecture', 'municipality_name']).apply(
    lambda x: pd.Series({
        'total_population': x['population'].sum(),
        'male_population': x[x['gender']=='male']['population'].sum(),
        'female_population': x[x['gender']=='female']['population'].sum(),
        'age_0_14': x[x['age_class'].isin(['0-4','5-9','10-14'])]['population'].sum(),
        'age_15_64': x[x['age_class'].isin([
            '15-19','20-24','25-29','30-34','35-39',
            '40-44','45-49','50-54','55-59','60-64'
        ])]['population'].sum(),
        'age_65_over': x[x['age_class'].isin([
            '65-69','70-74','75-79','80-84','85-89','90-94','95+'
        ])]['population'].sum(),
    })
).reset_index().rename(columns={'municipality_name': 'municipality'})

# idempotent: 既存 designated_ward を削除してから append
with sqlite3.connect('data/hellowork.db') as conn:
    cur = conn.execute(
        "DELETE FROM v2_external_population WHERE municipality IN "
        "(SELECT municipality_name FROM municipality_code_master WHERE area_type='designated_ward')"
    )
    print(f"deleted: {cur.rowcount}")
    agg.to_sql('v2_external_population', conn, if_exists='append', index=False)
    print(f"appended: {len(agg)}")
```

---

## 4. Step 2 検証 SQL

```sql
-- 4.1 v2_external_population designated_ward 件数
SELECT COUNT(*) FROM v2_external_population p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 175

-- 4.2 全国 175 件すべてカバー
SELECT mcm.prefecture, mcm.municipality_name
FROM municipality_code_master mcm
LEFT JOIN v2_external_population p
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward' AND p.municipality IS NULL;
-- 期待: 0 行

-- 4.3 v2_external_population_pyramid designated_ward カバレッジ
SELECT COUNT(*) FROM v2_external_population_pyramid p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 175 × 年齢階級数 (~9) = ~1,575
```

---

## 5. Step 3: F2 再計算

```bash
python scripts/build_municipality_target_thickness.py --build --csv-only \
    --output-csv data/generated/v2_municipality_target_thickness.csv
```

期待出力: **20,845 行 (1,895 muni × 11 occ)**。

CSV 段階の検証:

```python
import pandas as pd
import sqlite3

df = pd.read_csv('data/generated/v2_municipality_target_thickness.csv', dtype={'municipality_code': str})
assert len(df) == 20_845, f"expected 20845, got {len(df)}"
assert df['municipality_code'].nunique() == 1_895

with sqlite3.connect('file:data/hellowork.db?mode=ro', uri=True) as conn:
    designated = {r[0] for r in conn.execute(
        "SELECT municipality_code FROM municipality_code_master WHERE area_type='designated_ward'"
    ).fetchall()}
csv_designated = set(df['municipality_code']) & designated
assert len(csv_designated) == 175, f"only {len(csv_designated)}/175"
print("CSV validation PASS")
```

---

## 6. Step 4: mop + v2_thickness 投入

既存スクリプト再実行 (idempotent、変更不要):

```bash
# mop へ F2 推定再投入
python scripts/ingest_f2_to_local.py --apply
# 期待: 20,845 行 (旧 18,920 + 1,925)

# v2_thickness へ再投入
python scripts/ingest_v2_thickness_to_local.py --apply
# 期待: 20,845 行
```

---

## 7. Step 4 投入後の検証 SQL (12 項目)

```sql
-- [1] mop workplace × measured 維持
SELECT COUNT(*) FROM municipality_occupation_population
WHERE basis='workplace' AND data_label='measured';
-- 期待: 709,104

-- [2] mop resident × estimated_beta 件数
SELECT COUNT(*) FROM municipality_occupation_population
WHERE basis='resident' AND data_label='estimated_beta';
-- 期待: 20,845

-- [3] designated_ward の resident 推定件数
SELECT COUNT(*) FROM municipality_occupation_population mop
JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
WHERE mop.basis='resident' AND mop.data_label='estimated_beta'
  AND mcm.area_type='designated_ward';
-- 期待: 1,925 (= 175 × 11)

-- [4] aggregate_city の resident 推定 (Plan B 設計通り 0)
SELECT COUNT(*) FROM municipality_occupation_population mop
JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
WHERE mop.basis='resident' AND mcm.area_type='aggregate_city';
-- 期待: 0

-- [5] aggregate_special_wards の resident 推定 (Plan B 設計通り 0)
SELECT COUNT(*) FROM municipality_occupation_population mop
JOIN municipality_code_master mcm ON mop.municipality_code = mcm.municipality_code
WHERE mop.basis='resident' AND mcm.area_type='aggregate_special_wards';
-- 期待: 0

-- [6] designated_ward 175 件全件 NOT NULL (★ロールアウト基準★)
SELECT mcm.municipality_code, mcm.municipality_name
FROM municipality_code_master mcm
LEFT JOIN (
  SELECT DISTINCT municipality_code FROM municipality_occupation_population
  WHERE basis='resident' AND data_label='estimated_beta'
) r ON r.municipality_code = mcm.municipality_code
WHERE mcm.area_type='designated_ward' AND r.municipality_code IS NULL;
-- 期待: 0 行

-- [7] v2_thickness designated 件数
SELECT COUNT(*) FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type='designated_ward';
-- 期待: 1,925

-- [8] サンプル: 横浜市 18 区 (parent_code='14100')
SELECT v.municipality_code, mcm.municipality_name, COUNT(*) AS rows
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.parent_code='14100'
GROUP BY v.municipality_code, mcm.municipality_name
ORDER BY v.municipality_code;
-- 期待: 18 行、各 11

-- [9] サンプル: 大阪市 24 区 (parent_code='27100')
SELECT v.municipality_code, mcm.municipality_name, COUNT(*) AS rows
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.parent_code='27100'
GROUP BY v.municipality_code, mcm.municipality_name
ORDER BY v.municipality_code;
-- 期待: 24 行、各 11

-- [10] サンプル: 川崎市 7 区 (parent_code='14130')
SELECT v.municipality_code, mcm.municipality_name, COUNT(*) AS rows
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.parent_code='14130'
GROUP BY v.municipality_code, mcm.municipality_name
ORDER BY v.municipality_code;
-- 期待: 7 行、各 11

-- [11] 親市平均整合性 (横浜市 18 区平均 ~100)
SELECT v.occupation_code, AVG(v.thickness_index) AS avg_idx
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.parent_code='14100'
GROUP BY v.occupation_code;
-- 期待: 各 occ で thickness_index ~100 中心 (誤差 < 1%)

-- [12] PK 重複なし
SELECT COUNT(*) FROM (
  SELECT 1 FROM municipality_occupation_population
  WHERE basis='resident' AND data_label='estimated_beta'
  GROUP BY municipality_code, basis, occupation_code, age_class, gender, source_year, data_label
  HAVING COUNT(*) > 1
);
-- 期待: 0
```

---

## 8. ロールアウト承認基準 (12/12 PASS)

| # | 項目 | 期待値 |
|--:|------|------|
| 1 | mop workplace 維持 | 709,104 行 |
| 2 | mop resident 行数 | 20,845 行 |
| 3 | designated_ward resident 行数 | 1,925 行 |
| 4 | aggregate_city resident | 0 行 |
| 5 | aggregate_special_wards resident | 0 行 |
| 6 | **designated_ward NOT NULL カバレッジ** | **175/175 ← ロールアウト基準** |
| 7 | v2_thickness designated 件数 | 1,925 行 |
| 8 | 横浜市 18 区サンプル | 各 11 行 |
| 9 | 大阪市 24 区サンプル | 各 11 行 |
| 10 | 川崎市 7 区サンプル | 各 11 行 |
| 11 | 親市平均整合性 | 各 occ ~100 (誤差 < 1%) |
| 12 | PK 重複 | 0 |

**12/12 PASS = Turso upload + Rust 統合の解禁条件**。1 件でも FAIL なら rollback。

---

## 9. Rollback 手順 (4 SQL)

```sql
-- [R1] mop rollback
DELETE FROM municipality_occupation_population
WHERE basis='resident' AND data_label='estimated_beta'
  AND source_name='model_f2_target_thickness';

-- [R2] v2_thickness rollback
DELETE FROM v2_municipality_target_thickness
WHERE basis='resident' AND weight_source='hypothesis_v1' AND source_year=2020;

-- [R3] v2_external_population rollback (designated_ward のみ)
DELETE FROM v2_external_population
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);

-- [R4] v2_external_population_pyramid rollback (designated_ward のみ)
DELETE FROM v2_external_population_pyramid
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);
```

---

## 10. 既知のリスク

| # | リスク | 影響 | 対策 |
|--:|--------|------|------|
| 1 | estat_resident_merged.csv の `municipality_name` が master と表記揺れ (例: 'さいたま市浦和区' vs 'さいたま市 浦和区') | JOIN miss → 175 件未満 | master JOIN を `municipality_code` 経由で強制 (Step 3 の Python 参照) |
| 2 | 親市平均整合性 [11] で誤差 > 1% | thickness_index の信頼性低下 | B7 設計の `normalize_to_parent_mean` で吸収。誤差 > 1% なら B8 実装見直し |
| 3 | 横浜市の 18 区がすべて anchor=False (親市横浜市が anchor=False のため) | 採用診断で anchor 値が出ない | Plan B 設計通り受容。Phase 4+ で anchor 拡張検討 |

---

## 制約遵守

- DB 書き込みなし (本書は手順書のみ)
- スクリプト新規作成なし (既存 `ingest_f2_to_local.py` / `ingest_v2_thickness_to_local.py` 再実行)
- Turso 接続なし
- `.env` 直接 open なし
- ファイル作成: 本ファイル 1 つのみ
