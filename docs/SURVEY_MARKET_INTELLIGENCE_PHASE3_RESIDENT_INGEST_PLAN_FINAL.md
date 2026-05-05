# estat_resident_merged.csv → v2_external_population / pyramid 投入手順書 (最終版)

作成日: 2026-05-05
ステータス: 設計確定、実投入はユーザー承認後

---

## 0. 結論 (推奨方針)

**designated_ward 175 件のみを追加投入**。既存 1,742 / 15,660 行は **触らない**。

理由 (3 つの実測発見が「全 1,917 を別 source で」案を不採用にする):

1. 既存テーブルに `source` / `source_name` 列が **存在しない**
2. PK は `(prefecture, municipality)` (population) と `(prefecture, municipality, age_group)` (pyramid) のみ
3. 既存 1,742 muni に普通市/特別区/aggregate_city はカバーされており、**designated_ward 175 件のみ完全欠損**

→ 全 1,917 を「別 source」で重複投入する経路は DDL に source 列を追加する必要がある。デメリット (既存 NULL 埋め + 関連スクリプト/handler 影響) が大きい。

→ designated_ward 175 件は既存に存在しない (PK 衝突なし) ため、**そのまま追加 INSERT** が最小侵襲で安全。

---

## 1. 入力 CSV と既存テーブル状況

### 1.1 estat_resident_merged.csv (sid=0003445236 fetch 結果)

| 項目 | 値 |
|------|---:|
| 行数 | 69,012 |
| 構造 | 1,917 muni × 2 gender × 18 age_class |
| age_class | `15-19`, `20-24`, ..., `95-99`, `100+` (5 歳階級 18) |
| gender | `male`, `female` |
| 対象 | **15 歳以上のみ** (0-14 欠損) |
| 全国合計 population | 140,227,491 |
| designated_ward カバレッジ | **175 / 175 ✅** |
| sum 内訳 | unit 1,896 + aggregate 21 = 1,917 |

### 1.2 v2_external_population (既存)

| 項目 | 値 |
|------|---:|
| PK | `(prefecture, municipality)` 複合 |
| 列数 | 12 |
| **source 列** | **なし** |
| 行数 | 1,742 |
| 内訳 | aggregate_city 20 + municipality 1,698 + special_ward 23 + orphan 1 |
| **designated_ward 含むか** | **❌ 全件欠損** (0/175) |

カラム:
```
prefecture, municipality, total_population, male_population, female_population,
age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate, reference_date
```

### 1.3 v2_external_population_pyramid (既存)

| 項目 | 値 |
|------|---:|
| PK | `(prefecture, municipality, age_group)` |
| 列数 | 5 |
| **source 列** | **なし** |
| 行数 | 15,660 = 1,740 × 9 |
| age_group 形式 | **10 歳階級** `0-9, 10-19, 20-29, ..., 70-79, 80+` (9 階級) |

CSV (5 歳階級) との粒度ズレに注意。10 歳階級への集約が必要。

---

## 2. 投入対象の絞り込み (designated_ward 175 件)

### 2.1 PK 衝突確認

```sql
-- 既存に designated_ward が含まれているか
SELECT COUNT(*) FROM v2_external_population p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 0 (確認済)

SELECT COUNT(*) FROM v2_external_population_pyramid p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 0 (確認済)
```

→ **PK 衝突なし**。INSERT (not OR REPLACE) で安全。

### 2.2 投入する muni の特定

CSV の 1,917 muni のうち master `area_type='designated_ward'` の 175 件のみ投入:

```python
import pandas as pd
import sqlite3

df_csv = pd.read_csv('data/generated/estat_resident_merged.csv', dtype={'municipality_code': str})
df_csv['municipality_code'] = df_csv['municipality_code'].str.zfill(5)

with sqlite3.connect('file:data/hellowork.db?mode=ro', uri=True) as conn:
    designated = pd.read_sql(
        "SELECT municipality_code, prefecture, municipality_name "
        "FROM municipality_code_master WHERE area_type='designated_ward'",
        conn
    )
df_designated = df_csv.merge(designated, on='municipality_code', how='inner')
assert df_designated['municipality_code'].nunique() == 175
# 175 muni × 2 gender × 18 age_class = 6,300 行
assert len(df_designated) == 175 * 2 * 18
```

---

## 3. v2_external_population 投入手順 (175 行 追加)

### 3.1 5 歳階級 → 12 列ワイド形式へ集約

```python
# CSV は 15+ のみのため、age_0_14 は NULL とする (実測なしを明示)
def to_wide(group):
    male_total = group[group['gender']=='male']['population'].sum()
    female_total = group[group['gender']=='female']['population'].sum()
    total = male_total + female_total
    age_15_64 = group[group['age_class'].isin([
        '15-19','20-24','25-29','30-34','35-39',
        '40-44','45-49','50-54','55-59','60-64'
    ])]['population'].sum()
    age_65_over = total - age_15_64  # 15+ のみなので 65+ は total - 15-64
    return pd.Series({
        'total_population': int(total),
        'male_population': int(male_total),
        'female_population': int(female_total),
        'age_0_14': None,           # CSV 不在、明示 NULL
        'age_15_64': int(age_15_64),
        'age_65_over': int(age_65_over),
        'aging_rate': None,         # 0-14 なしで分母不完全 → NULL
        'working_age_rate': None,
        'youth_rate': None,
        'reference_date': '2020-10-01',
    })

agg = df_designated.groupby(
    ['prefecture', 'municipality_name']
).apply(to_wide).reset_index().rename(columns={'municipality_name': 'municipality'})
assert len(agg) == 175

with sqlite3.connect('data/hellowork.db') as conn:
    # rollback target を先に削除 (idempotent、初回は 0 削除)
    cur = conn.execute("""
        DELETE FROM v2_external_population
        WHERE municipality IN (
          SELECT municipality_name FROM municipality_code_master
          WHERE area_type='designated_ward'
        )
    """)
    print(f'deleted: {cur.rowcount}')  # 初回 0、再実行時 175
    agg.to_sql('v2_external_population', conn, if_exists='append', index=False)
    print(f'inserted: {len(agg)}')

# 投入後 1,742 + 175 = 1,917 行になる
```

### 3.2 NULL カラムの意味

| カラム | 値 | 理由 |
|-------|----|------|
| `age_0_14` | NULL | CSV (15+ のみ) で値が取れない |
| `aging_rate` | NULL | 全年齢人口が不明なので比率計算不可 |
| `working_age_rate` | NULL | 同上 |
| `youth_rate` | NULL | 同上 |

`age_15_64`, `age_65_over` は CSV 範囲内で計算可能。

---

## 4. v2_external_population_pyramid 投入手順 (1,575 行 追加)

### 4.1 5 歳階級 → 10 歳階級 集約マッピング

| 既存 age_group | CSV 5 歳階級 | 集約方法 | 完全性 |
|----------------|-------------|----------|:------:|
| `0-9` | (CSV になし) | 0 を入れる | ⚠️ 欠損 |
| `10-19` | `15-19` のみ | 15-19 をそのまま使う (10-14 欠損) | ⚠️ 部分 |
| `20-29` | `20-24` + `25-29` | SUM | ✅ |
| `30-39` | `30-34` + `35-39` | SUM | ✅ |
| `40-49` | `40-44` + `45-49` | SUM | ✅ |
| `50-59` | `50-54` + `55-59` | SUM | ✅ |
| `60-69` | `60-64` + `65-69` | SUM | ✅ |
| `70-79` | `70-74` + `75-79` | SUM | ✅ |
| `80+` | `80-84`+`85-89`+`90-94`+`95-99`+`100+` | SUM | ✅ |

### 4.2 投入コード

```python
AGE_BUCKET_MAP = {
    "0-9":   [],
    "10-19": ["15-19"],          # 10-14 欠損
    "20-29": ["20-24", "25-29"],
    "30-39": ["30-34", "35-39"],
    "40-49": ["40-44", "45-49"],
    "50-59": ["50-54", "55-59"],
    "60-69": ["60-64", "65-69"],
    "70-79": ["70-74", "75-79"],
    "80+":   ["80-84", "85-89", "90-94", "95-99", "100+"],
}

rows = []
for (pref, muni_name), grp in df_designated.groupby(['prefecture', 'municipality_name']):
    for bucket, sources in AGE_BUCKET_MAP.items():
        sub = grp[grp['age_class'].isin(sources)] if sources else grp.iloc[0:0]
        rows.append({
            'prefecture': pref,
            'municipality': muni_name,
            'age_group': bucket,
            'male_count': int(sub[sub['gender']=='male']['population'].sum()) if sources else 0,
            'female_count': int(sub[sub['gender']=='female']['population'].sum()) if sources else 0,
        })
pyr = pd.DataFrame(rows)
assert len(pyr) == 175 * 9 == 1575

with sqlite3.connect('data/hellowork.db') as conn:
    cur = conn.execute("""
        DELETE FROM v2_external_population_pyramid
        WHERE municipality IN (
          SELECT municipality_name FROM municipality_code_master
          WHERE area_type='designated_ward'
        )
    """)
    print(f'deleted: {cur.rowcount}')
    pyr.to_sql('v2_external_population_pyramid', conn, if_exists='append', index=False)
    print(f'inserted: {len(pyr)}')

# 投入後 15,660 + 1,575 = 17,235 行
```

### 4.3 不完全バケットの注記

- `0-9`: 全 175 区で 0 (実測値ではない)
- `10-19`: 15-19 のみ (10-14 が欠損)、実値の 1/2 程度になる

→ F2 推定の入力としては age_15_64 全体で按分するため、10-19 の不完全さは生産年齢全体への影響は小さい。
→ ただし 10 代狙いの採用分析では 10-19 の数値を **直接使わない** ことに注記が必要。

---

## 5. rollback 条件

```sql
-- v2_external_population の designated_ward 175 件削除
DELETE FROM v2_external_population
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);
-- 期待: 175 行削除

-- v2_external_population_pyramid の designated_ward 1,575 行削除
DELETE FROM v2_external_population_pyramid
WHERE municipality IN (
  SELECT municipality_name FROM municipality_code_master
  WHERE area_type='designated_ward'
);
-- 期待: 1,575 行削除
```

両 DELETE は idempotent (再実行で行数 0 削除)。投入手順内で **DELETE → INSERT** にしてあるので、複数回 apply しても結果同じ。

---

## 6. 投入後の検証 SQL (8 項目)

```sql
-- [1] v2_external_population designated_ward カバレッジ = 175
SELECT COUNT(*) FROM v2_external_population p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 175

-- [2] 既存 1,742 行が破壊されていない
SELECT COUNT(*) FROM v2_external_population p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type IN ('aggregate_city','municipality','special_ward');
-- 期待: 1,741 (orphan 1 を除く)

-- [3] 全 175 designated_ward 完全カバレッジ確認
SELECT mcm.municipality_code, mcm.prefecture, mcm.municipality_name
FROM municipality_code_master mcm
LEFT JOIN v2_external_population p
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward' AND p.municipality IS NULL;
-- 期待: 0 行

-- [4] v2_external_population_pyramid designated_ward カバレッジ
SELECT COUNT(*) FROM v2_external_population_pyramid p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward';
-- 期待: 1,575 (= 175 × 9)

-- [5] designated_ward の age_group が 9 階級揃っている
SELECT mcm.prefecture, mcm.municipality_name, COUNT(DISTINCT p.age_group) AS n_age
FROM v2_external_population_pyramid p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.area_type='designated_ward'
GROUP BY mcm.prefecture, mcm.municipality_name
HAVING n_age != 9;
-- 期待: 0 行

-- [6] サンプル: 横浜市鶴見区
SELECT * FROM v2_external_population_pyramid
WHERE prefecture='神奈川県' AND municipality='横浜市鶴見区'
ORDER BY age_group;
-- 期待: 9 行 (0-9 は 0、10-19 は部分値、20+ は完全)

-- [7] population 数値妥当性 (15+ サンプル)
SELECT prefecture, municipality, age_15_64, age_65_over, total_population
FROM v2_external_population p
JOIN municipality_code_master mcm
  ON p.prefecture = mcm.prefecture AND p.municipality = mcm.municipality_name
WHERE mcm.municipality_code IN ('14101','14131','27141','40131','01101')
ORDER BY mcm.municipality_code;
-- 期待: 5 行、各 age_15_64 + age_65_over = total

-- [8] PK 重複なし (両テーブル)
SELECT prefecture, municipality, COUNT(*) FROM v2_external_population
GROUP BY 1,2 HAVING COUNT(*) > 1;
-- 期待: 0 行

SELECT prefecture, municipality, age_group, COUNT(*) FROM v2_external_population_pyramid
GROUP BY 1,2,3 HAVING COUNT(*) > 1;
-- 期待: 0 行
```

---

## 7. 既存データ保護方針 (3 つの安全装置)

1. **DELETE スコープ限定**: master `area_type='designated_ward'` でフィルタした 175 muni のみ削除。aggregate_city/municipality/special_ward は触らない
2. **PK 衝突なし確認済**: 既存に designated_ward 0 件、追加投入なので衝突発生不可
3. **冪等性**: DELETE → INSERT 構造のため複数回 apply しても結果同一

---

## 8. 投入後の影響範囲

### 8.1 build_municipality_target_thickness.py の挙動変化

`load_inputs()` が `v2_external_population` を読むため、investagainの自動的に designated_ward 175 件が含まれる。F2 推定で:

- 投入前: 1,740 muni (=15,660 / 9) で計算 → 19,140 行 CSV
- 投入後: 1,915 muni で計算 → 21,065 行 CSV

ただし、投入後の F2 再実行 + mop/v2_thickness 再投入は **別タスク**。本書はあくまで `v2_external_population` / `_pyramid` への 175 muni 追加のみ。

### 8.2 Worker B8 の rolldown スケルトン

`compute_designated_ward_rolldown` は **不要**になる。理由: F2 推定が直接 designated_ward を入力として処理できるようになる (rolldown による親市按分が不要)。

→ Worker B8 のスケルトンは Phase 4+ の rolldown 補完用 (例: 0-14 補完 or 親市集約) として温存可能。

---

## 9. 既知の限界 (商品 UI 側の注記候補)

| 限界 | 対応 |
|------|------|
| age_0_14 が NULL | UI で「0-14 歳推定は別ソース必要」と注記 |
| 10-19 の 10-14 欠損 | UI で年齢階級表示時に「10-19 (15-19 ベース)」と但書 |
| aging_rate 等 NULL | UI で全年齢人口比率を表示しない (designated_ward に限り) |
| 0-9 の male/female_count = 0 | F2 推定では age_15_64 のみ参照のため影響なし |

---

## 10. 実装順序 (ユーザー承認後)

1. 本書 §3 のスクリプトを 1 つの `scripts/ingest_resident_to_external_population.py` にまとめる
2. `--dry-run` で見積 (175 行 / 1,575 行 / 既存 1,742 維持確認)
3. `--apply` で実投入 + 検証 SQL 8 項目自動実行
4. 12/12 PASS → 次フェーズ (F2 再実行 → mop/v2_thickness 再投入)

---

## 11. 制約遵守

- DB 書き込み: 本書は手順書のみ、実投入はユーザー承認後の別タスク
- Turso upload: 禁止
- Rust 統合: 禁止
- push: 禁止
- 既存テーブル DDL 変更: なし (source 列追加せず、別テーブル作成せず)

---

## 12. 関連ドキュメント

- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DESIGNATED_WARD_DATA_AUDIT.md` (Worker A7、初期調査)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_RESIDENT_POP_SID_RESEARCH_PHASE2.md` (Worker A10、sid=0003445236 採用根拠)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DESIGNATED_WARD_F2_DESIGN.md` (Worker B7、rolldown 設計、本書投入後は不要)
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_DESIGNATED_WARD_INGEST_VALIDATION.md` (Worker C8、想定 schema 版、本書で実 schema に置き換え)
