# Phase 3 Step 5 Turso Upload 直前差分監査

- 実行日時 (UTC): 2026-05-05T15:20:36.043262+00:00 〜 2026-05-05T15:20:43.555220+00:00
- 所要時間: 7.5 秒
- ローカル DB: `data/hellowork.db`
- リモート: `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` (Turso V2)
- READ 消費: **7 / 50**
- 監査者: Worker X1 (READ-ONLY)

## 0. サマリ (差分マトリクス)

| # | テーブル | ローカル | リモート | ケース | 推奨アクション |
|--:|---------|--------:|--------:|:------:|---------------|
| 1 | `v2_external_population` | 1917 | 1742 | (c) | 全置換 or 差分 INSERT (+175 行) |
| 2 | `v2_external_population_pyramid` | 17235 | 15660 | (c) | 全置換 or 差分 INSERT (+1575 行) |
| 3 | `municipality_occupation_population` | 729949 | MISSING | (a) | CREATE + INSERT (新規) |
| 4 | `v2_municipality_target_thickness` | 20845 | MISSING | (a) | CREATE + INSERT (新規) |
| 5 | `municipality_code_master` | 1917 | MISSING | (a) | CREATE + INSERT (新規) |
| 6 | `commute_flow_summary` | 27879 | MISSING | (a) | CREATE + INSERT (新規) |
| 7 | `v2_external_commute_od_with_codes` | 86762 | MISSING | (a) | CREATE + INSERT (新規) |

### ケース別集計

| ケース | 件数 | 意味 |
|:------:|----:|------|
| (a) | 5 | リモート不存在 → CREATE + INSERT |
| (b) | 0 | 差分なし → スキップ |
| (c) | 2 | リモート < ローカル → 全置換 or 差分 INSERT |
| (d) | 0 | ⚠️ リモート > ローカル → 警戒 |
| (e) | 0 | 構造差分 → DDL マイグレーション |

## 1. ローカル DB 7 テーブル状況

### `v2_external_population`

- 行数: **1,917**
- カラム数: 12
- カラム: prefecture, municipality, total_population, male_population, female_population, age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate, reference_date

### `v2_external_population_pyramid`

- 行数: **17,235**
- カラム数: 5
- カラム: prefecture, municipality, age_group, male_count, female_count

### `municipality_occupation_population`

- 行数: **729,949**
- カラム数: 15
- カラム: municipality_code, prefecture, municipality_name, basis, occupation_code, occupation_name, age_class, gender, population, estimate_index, data_label, source_name, source_year, weight_source, estimated_at

### `v2_municipality_target_thickness`

- 行数: **20,845**
- カラム数: 18
- カラム: municipality_code, prefecture, municipality_name, basis, occupation_code, occupation_name, thickness_index, rank_in_occupation, rank_percentile, distribution_priority, scenario_conservative_index, scenario_standard_index, scenario_aggressive_index, estimate_grade, weight_source, is_industrial_anchor, source_year, estimated_at

### `municipality_code_master`

- 行数: **1,917**
- カラム数: 12
- カラム: municipality_code, prefecture, municipality_name, pref_code, area_type, area_level, is_special_ward, is_designated_ward, parent_code, source, source_year, created_at

### `commute_flow_summary`

- 行数: **27,879**
- カラム数: 19
- カラム: destination_municipality_code, destination_prefecture, destination_municipality_name, origin_municipality_code, origin_prefecture, origin_municipality_name, occupation_group_code, occupation_group_name, flow_count, flow_share, target_origin_population, estimated_target_flow_conservative, estimated_target_flow_standard, estimated_target_flow_aggressive, estimation_method, estimated_at, rank_to_destination, source_year, created_at

### `v2_external_commute_od_with_codes`

- 行数: **86,762**
- カラム数: 12
- カラム: origin_municipality_code, dest_municipality_code, origin_prefecture, origin_municipality_name, dest_prefecture, dest_municipality_name, total_commuters, male_commuters, female_commuters, reference_year, source, created_at


## 2. Turso V2 リモート 7 テーブル状況

### `v2_external_population`

- 行数: **1,742**
- カラム数: 12
- カラム: prefecture, municipality, total_population, male_population, female_population, age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate, reference_date

### `v2_external_population_pyramid`

- 行数: **15,660**
- カラム数: 5
- カラム: prefecture, municipality, age_group, male_count, female_count

### `municipality_occupation_population`

- **リモート不在**

### `v2_municipality_target_thickness`

- **リモート不在**

### `municipality_code_master`

- **リモート不在**

### `commute_flow_summary`

- **リモート不在**

### `v2_external_commute_od_with_codes`

- **リモート不在**


## 3. 差分マトリクス (5 ケース別)

### ケース (a): リモート不存在 → CREATE + INSERT

- `municipality_occupation_population`: CREATE + INSERT (新規)
- `v2_municipality_target_thickness`: CREATE + INSERT (新規)
- `municipality_code_master`: CREATE + INSERT (新規)
- `commute_flow_summary`: CREATE + INSERT (新規)
- `v2_external_commute_od_with_codes`: CREATE + INSERT (新規)

### ケース (c): リモート < ローカル → 全置換 or 差分 INSERT

- `v2_external_population`: 全置換 or 差分 INSERT (+175 行)
- `v2_external_population_pyramid`: 全置換 or 差分 INSERT (+1575 行)


## 4. テーブルごとの upload 推奨方針

### `v2_external_population`

- ケース: (c)
- アクション: 全置換 or 差分 INSERT (+175 行)
- ローカル 1,917 行 / リモート 1742 行 / 差分 +175

### `v2_external_population_pyramid`

- ケース: (c)
- アクション: 全置換 or 差分 INSERT (+1575 行)
- ローカル 17,235 行 / リモート 15660 行 / 差分 +1,575

### `municipality_occupation_population`

- ケース: (a)
- アクション: CREATE + INSERT (新規)
- ローカル 729,949 行 / リモート MISSING 行 / 差分 +729,949

### `v2_municipality_target_thickness`

- ケース: (a)
- アクション: CREATE + INSERT (新規)
- ローカル 20,845 行 / リモート MISSING 行 / 差分 +20,845

### `municipality_code_master`

- ケース: (a)
- アクション: CREATE + INSERT (新規)
- ローカル 1,917 行 / リモート MISSING 行 / 差分 +1,917

### `commute_flow_summary`

- ケース: (a)
- アクション: CREATE + INSERT (新規)
- ローカル 27,879 行 / リモート MISSING 行 / 差分 +27,879

### `v2_external_commute_od_with_codes`

- ケース: (a)
- アクション: CREATE + INSERT (新規)
- ローカル 86,762 行 / リモート MISSING 行 / 差分 +86,762


## 5. Row writes 見積

| テーブル | アクション | writes 見積 |
|---------|----------|------------:|
| `v2_external_population` | (c) | 1,917 (全置換: DELETE 1742 + INSERT 1917 ≒ 1917 writes) |
| `v2_external_population_pyramid` | (c) | 17,235 (全置換: DELETE 15660 + INSERT 17235 ≒ 17235 writes) |
| `municipality_occupation_population` | (a) | 729,949 (全件 INSERT) |
| `v2_municipality_target_thickness` | (a) | 20,845 (全件 INSERT) |
| `municipality_code_master` | (a) | 1,917 (全件 INSERT) |
| `commute_flow_summary` | (a) | 27,879 (全件 INSERT) |
| `v2_external_commute_od_with_codes` | (a) | 86,762 (全件 INSERT) |
| **合計** | | **886,504** |

- Turso V2 月間 row writes 上限: 25M (無料枠)
- 本 upload 想定: **886,504** writes
- 上限消費率: **3.55%**

## 6. 既知のリスク

### 政令市区追加に伴う影響

- `v2_external_population`: 175 件追加 (designated_ward) → カラム構造変更なしの場合、単純 INSERT で OK。
- `v2_external_population_pyramid`: 1,575 行追加 → 同上。
- ただし municipality_code 重複チェック必須 (政令市の親と区が両方含まれる場合あり)。

### READ-ONLY 安全装置の動作確認

- WRITE 系 SQL 検出: 0 件
- READ 上限到達: なし
- 認証 token 露出: 本レポートに転記なし

---

生成: `scripts/audit_turso_upload_diff.py` (2026-05-05 15:20 UTC)