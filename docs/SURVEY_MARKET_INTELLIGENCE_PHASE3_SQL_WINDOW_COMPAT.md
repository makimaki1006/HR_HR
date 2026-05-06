# Phase 3 Step 5 SQL Window Function 互換性検証

- 実行日時 (UTC): 2026-05-05T17:44:11.436540+00:00 〜 2026-05-05T17:44:12.779248+00:00
- 実行モード: `both`
- 検証者: Worker P0 (READ-ONLY)

## 0. 結論

**PASS** (3/3 一致)。SQL Window Function は Turso libSQL で動作する。Phase 3 本実装で `RANK() OVER` / `COUNT(*) OVER` / `PARTITION BY` を採用可能。

## 1. SQLite バージョン

- ローカル: `3.49.1`
- Turso: `3.45.1`

Window Function は SQLite 3.25+ で利用可能 (RANK/PARTITION BY 含む)。

## 2. 検証 SQL (3 種)

### Q1_RANK_PARTITION

```sql
WITH tgt AS (
  SELECT v.municipality_code, mcm.municipality_name,
         mcm.parent_code, parent.municipality_name AS parent_name,
         v.thickness_index, v.occupation_code
  FROM v2_municipality_target_thickness v
  JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
  LEFT JOIN municipality_code_master parent ON mcm.parent_code = parent.municipality_code
  WHERE mcm.area_type = 'designated_ward'
    AND v.occupation_code = '08_生産工程'
    AND mcm.parent_code = '14100'
)
SELECT municipality_code, municipality_name,
       RANK() OVER (PARTITION BY parent_code ORDER BY thickness_index DESC) AS parent_rank
FROM tgt
ORDER BY parent_rank
```

### Q2_COUNT_OVER_PARTITION

```sql
SELECT v.municipality_code, mcm.municipality_name,
       mcm.parent_code,
       COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
WHERE mcm.area_type = 'designated_ward'
  AND v.occupation_code = '08_生産工程'
  AND mcm.parent_code = '14100'
LIMIT 18
```

### Q3_RANK_AND_COUNT

```sql
SELECT v.municipality_code, mcm.municipality_name,
       mcm.parent_code, parent.municipality_name AS parent_name,
       v.thickness_index, v.occupation_code,
       RANK() OVER (PARTITION BY mcm.parent_code ORDER BY v.thickness_index DESC) AS parent_rank,
       COUNT(*) OVER (PARTITION BY mcm.parent_code) AS parent_total
FROM v2_municipality_target_thickness v
JOIN municipality_code_master mcm ON v.municipality_code = mcm.municipality_code
LEFT JOIN municipality_code_master parent ON mcm.parent_code = parent.municipality_code
WHERE mcm.area_type = 'designated_ward'
  AND v.occupation_code = '08_生産工程'
ORDER BY mcm.parent_code, parent_rank
LIMIT 30
```

## 3. ローカル実行結果

### Q1_RANK_PARTITION

- 行数: 18

| municipality_code | municipality_name | parent_rank |
|---|---|---|
| 14109 | 横浜市港北区 | 1 |
| 14117 | 横浜市青葉区 | 1 |
| 14101 | 横浜市鶴見区 | 1 |
| 14110 | 横浜市戸塚区 | 1 |
| 14102 | 横浜市神奈川区 | 1 |
| 14112 | 横浜市旭区 | 1 |
| 14118 | 横浜市都筑区 | 1 |
| 14111 | 横浜市港南区 | 1 |
| 14106 | 横浜市保土ケ谷区 | 1 |
| 14105 | 横浜市南区 | 1 |

### Q2_COUNT_OVER_PARTITION

- 行数: 18

| municipality_code | municipality_name | parent_code | parent_total |
|---|---|---|---|
| 14101 | 横浜市鶴見区 | 14100 | 18 |
| 14102 | 横浜市神奈川区 | 14100 | 18 |
| 14103 | 横浜市西区 | 14100 | 18 |
| 14104 | 横浜市中区 | 14100 | 18 |
| 14105 | 横浜市南区 | 14100 | 18 |
| 14106 | 横浜市保土ケ谷区 | 14100 | 18 |
| 14107 | 横浜市磯子区 | 14100 | 18 |
| 14108 | 横浜市金沢区 | 14100 | 18 |
| 14109 | 横浜市港北区 | 14100 | 18 |
| 14110 | 横浜市戸塚区 | 14100 | 18 |

### Q3_RANK_AND_COUNT

- 行数: 30

| municipality_code | municipality_name | parent_code | parent_name | thickness_index | occupation_code | parent_rank | parent_total |
|---|---|---|---|---|---|---|---|
| 01102 | 札幌市北区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01103 | 札幌市東区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01101 | 札幌市中央区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01105 | 札幌市豊平区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01104 | 札幌市白石区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01107 | 札幌市西区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01109 | 札幌市手稲区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01106 | 札幌市南区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01108 | 札幌市厚別区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01110 | 札幌市清田区 | 01100 | 札幌市 | 187.1 | 08_生産工程 | 10 | 10 |

## 4. Turso 実行結果

- ホスト: `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` (token はマスク済)
- READ 消費: 4 / 10

### Q1_RANK_PARTITION

- 行数: 18

| municipality_code | municipality_name | parent_rank |
|---|---|---|
| 14109 | 横浜市港北区 | 1 |
| 14117 | 横浜市青葉区 | 1 |
| 14101 | 横浜市鶴見区 | 1 |
| 14110 | 横浜市戸塚区 | 1 |
| 14102 | 横浜市神奈川区 | 1 |
| 14112 | 横浜市旭区 | 1 |
| 14118 | 横浜市都筑区 | 1 |
| 14111 | 横浜市港南区 | 1 |
| 14106 | 横浜市保土ケ谷区 | 1 |
| 14105 | 横浜市南区 | 1 |

### Q2_COUNT_OVER_PARTITION

- 行数: 18

| municipality_code | municipality_name | parent_code | parent_total |
|---|---|---|---|
| 14101 | 横浜市鶴見区 | 14100 | 18 |
| 14102 | 横浜市神奈川区 | 14100 | 18 |
| 14103 | 横浜市西区 | 14100 | 18 |
| 14104 | 横浜市中区 | 14100 | 18 |
| 14105 | 横浜市南区 | 14100 | 18 |
| 14106 | 横浜市保土ケ谷区 | 14100 | 18 |
| 14107 | 横浜市磯子区 | 14100 | 18 |
| 14108 | 横浜市金沢区 | 14100 | 18 |
| 14109 | 横浜市港北区 | 14100 | 18 |
| 14110 | 横浜市戸塚区 | 14100 | 18 |

### Q3_RANK_AND_COUNT

- 行数: 30

| municipality_code | municipality_name | parent_code | parent_name | thickness_index | occupation_code | parent_rank | parent_total |
|---|---|---|---|---|---|---|---|
| 01102 | 札幌市北区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01103 | 札幌市東区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01101 | 札幌市中央区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01105 | 札幌市豊平区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01104 | 札幌市白石区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01107 | 札幌市西区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01109 | 札幌市手稲区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01106 | 札幌市南区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01108 | 札幌市厚別区 | 01100 | 札幌市 | 200.0 | 08_生産工程 | 1 | 10 |
| 01110 | 札幌市清田区 | 01100 | 札幌市 | 187.1 | 08_生産工程 | 10 | 10 |

## 5. 差分比較 (ローカル vs Turso)

| Query | 状態 | ローカル行数 | Turso行数 | 一致 |
|-------|------|----:|----:|:---:|
| Q1_RANK_PARTITION | MATCH | 18 | 18 | ✅ |
| Q2_COUNT_OVER_PARTITION | MATCH | 18 | 18 | ✅ |
| Q3_RANK_AND_COUNT | MATCH | 30 | 30 | ✅ |

## 6. PASS 判定

✅ **PASS** (3/3)

Turso libSQL は SQL Window Function (`RANK() OVER`, `COUNT(*) OVER`, `PARTITION BY`) を完全サポート。

## 7. FAIL 時の Rust fallback 案

Window Function が Turso 側で動かない場合の代替策 (Rust Integration Plan §0.5):

1. **Rust 側でランク計算**: `SELECT municipality_code, parent_code, thickness_index FROM ...` のみ Turso から取得し、Rust の `Vec` を `parent_code` でグループ化 → `sort_by` → enumerate で rank 付与。
2. **2 ステップクエリ**: 親グループの総数を別 `COUNT(*) GROUP BY parent_code` で取得し、Rust 側で Map<parent_code, total> を保持。
3. **事前計算**: `build_municipality_target_thickness.py` の段階で rank/total カラムを事前計算しテーブルに格納。Phase 3 のクエリ複雑度を下げる。

---

生成: `scripts/verify_turso_window_function.py` (2026-05-05 17:44 UTC)