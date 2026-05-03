# Turso V2 ↔ ローカル DB 詳細差分レポート (SAMPLE_MISMATCH 5 テーブル)

- 実行日時 (UTC): 2026-05-03T15:48:47.702750+00:00 〜 2026-05-03T15:48:49.113832+00:00
- ローカル DB: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db`
- リモート: `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` (Turso V2)
- READ 消費: 5 (上限 50)

## 目的

`turso_v2_sync_report_2026-05-03.md` で SAMPLE_MISMATCH 判定された 5 テーブルについて、
主キー単位で local 値と Turso 値を併記し、差分原因を切り分ける。

## v2_external_population

- 主キー: `prefecture, municipality`
- WHERE: `prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')`
- 取得行数: local=3, Turso=3

### 取得範囲では差分なし

### 詳細サンプル

#### 北海道 / 札幌市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 北海道 | 北海道 | ✅ |
| `municipality` | 札幌市 | 札幌市 | ✅ |
| `total_population` | 1973395 | 1973395 | ✅ |
| `male_population` | 918682 | 918682 | ✅ |
| `female_population` | 1054713 | 1054713 | ✅ |
| `aging_rate` | 27.43 | 27.43 | ✅ |
| `reference_date` | 2020-10-01 | 2020-10-01 | ✅ |

#### 大阪府 / 大阪市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 大阪府 | 大阪府 | ✅ |
| `municipality` | 大阪市 | 大阪市 | ✅ |
| `total_population` | 2752412 | 2752412 | ✅ |
| `male_population` | 1326875 | 1326875 | ✅ |
| `female_population` | 1425537 | 1425537 | ✅ |
| `aging_rate` | 24.59 | 24.59 | ✅ |
| `reference_date` | 2020-10-01 | 2020-10-01 | ✅ |

#### 東京都 / 新宿区

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `total_population` | 349385 | 349385 | ✅ |
| `male_population` | 174822 | 174822 | ✅ |
| `female_population` | 174563 | 174563 | ✅ |
| `aging_rate` | 18.08 | 18.08 | ✅ |
| `reference_date` | 2020-10-01 | 2020-10-01 | ✅ |

## v2_external_migration

- 主キー: `prefecture, municipality`
- WHERE: `prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')`
- 取得行数: local=3, Turso=3

### 取得範囲では差分なし

### 詳細サンプル

#### 北海道 / 札幌市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 北海道 | 北海道 | ✅ |
| `municipality` | 札幌市 | 札幌市 | ✅ |
| `inflow` | 111776 | 111776 | ✅ |
| `outflow` | 102951 | 102951 | ✅ |
| `net_migration` | 8825 | 8825 | ✅ |
| `net_migration_rate` | 4.472 | 4.472 | ✅ |
| `reference_year` | 2023 | 2023 | ✅ |

#### 大阪府 / 大阪市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 大阪府 | 大阪府 | ✅ |
| `municipality` | 大阪市 | 大阪市 | ✅ |
| `inflow` | 166692 | 166692 | ✅ |
| `outflow` | 151907 | 151907 | ✅ |
| `net_migration` | 14785 | 14785 | ✅ |
| `net_migration_rate` | 5.372 | 5.372 | ✅ |
| `reference_year` | 2023 | 2023 | ✅ |

#### 東京都 / 新宿区

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `inflow` | 27765 | 27765 | ✅ |
| `outflow` | 27428 | 27428 | ✅ |
| `net_migration` | 337 | 337 | ✅ |
| `net_migration_rate` | 0.9646 | 0.9646 | ✅ |
| `reference_year` | 2023 | 2023 | ✅ |

## v2_external_daytime_population

- 主キー: `prefecture, municipality`
- WHERE: `prefecture IN ('北海道', '東京都', '大阪府') AND municipality IN ('札幌市', '新宿区', '大阪市')`
- 取得行数: local=3, Turso=3

### 取得範囲では差分なし

### 詳細サンプル

#### 北海道 / 札幌市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 北海道 | 北海道 | ✅ |
| `municipality` | 札幌市 | 札幌市 | ✅ |
| `nighttime_pop` | 1973395 | 1973395 | ✅ |
| `daytime_pop` | 1968338 | 1968338 | ✅ |
| `day_night_ratio` | 99.74 | 99.74 | ✅ |
| `reference_year` | 2020 | 2020 | ✅ |

#### 大阪府 / 大阪市

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 大阪府 | 大阪府 | ✅ |
| `municipality` | 大阪市 | 大阪市 | ✅ |
| `nighttime_pop` | 2752412 | 2752412 | ✅ |
| `daytime_pop` | 3645921 | 3645921 | ✅ |
| `day_night_ratio` | 132.5 | 132.5 | ✅ |
| `reference_year` | 2020 | 2020 | ✅ |

#### 東京都 / 新宿区

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `nighttime_pop` | 349385 | 349385 | ✅ |
| `daytime_pop` | 903456 | 903456 | ✅ |
| `day_night_ratio` | 258.6 | 258.6 | ✅ |
| `reference_year` | 2020 | 2020 | ✅ |

## v2_external_population_pyramid

- 主キー: `prefecture, municipality, age_group`
- WHERE: `prefecture = '東京都' AND municipality = '新宿区'`
- 取得行数: local=9, Turso=9

### 取得範囲では差分なし

### 詳細サンプル

#### 東京都 / 新宿区 / 0-9

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `age_group` | 0-9 | 0-9 | ✅ |
| `male_count` | 9849 | 9849 | ✅ |
| `female_count` | 9618 | 9618 | ✅ |

#### 東京都 / 新宿区 / 10-19

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `age_group` | 10-19 | 10-19 | ✅ |
| `male_count` | 11730 | 11730 | ✅ |
| `female_count` | 11199 | 11199 | ✅ |

#### 東京都 / 新宿区 / 20-29

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `municipality` | 新宿区 | 新宿区 | ✅ |
| `age_group` | 20-29 | 20-29 | ✅ |
| `male_count` | 21701 | 21701 | ✅ |
| `female_count` | 21280 | 21280 | ✅ |

## v2_external_prefecture_stats

- 主キー: `prefecture`
- WHERE: `prefecture IN ('北海道', '東京都', '大阪府')`
- 取得行数: local=3, Turso=3

### 取得範囲では差分なし

### 詳細サンプル

#### 北海道

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 北海道 | 北海道 | ✅ |
| `unemployment_rate` | 3.2 | 3.2 | ✅ |
| `job_change_desire_rate` | 4.2 | 4.2 | ✅ |
| `non_regular_rate` | 39.9 | 39.9 | ✅ |
| `avg_monthly_wage` | 288.5 | 288.5 | ✅ |
| `price_index` | 101.9 | 101.9 | ✅ |
| `fulfillment_rate` | 15.2 | 15.2 | ✅ |
| `real_wage_index` | 283.1 | 283.1 | ✅ |

#### 大阪府

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 大阪府 | 大阪府 | ✅ |
| `unemployment_rate` | 3 | 3 | ✅ |
| `job_change_desire_rate` | 4.9 | 4.9 | ✅ |
| `non_regular_rate` | 39.8 | 39.8 | ✅ |
| `avg_monthly_wage` | 348 | 348 | ✅ |
| `price_index` | 99.3 | 99.3 | ✅ |
| `fulfillment_rate` | 12.2 | 12.2 | ✅ |
| `real_wage_index` | 350.5 | 350.5 | ✅ |

#### 東京都

| カラム | local | Turso | 一致 |
|--------|-------|-------|:----:|
| `prefecture` | 東京都 | 東京都 | ✅ |
| `unemployment_rate` | 2.6 | 2.6 | ✅ |
| `job_change_desire_rate` | 5.4 | 5.4 | ✅ |
| `non_regular_rate` | 32.6 | 32.6 | ✅ |
| `avg_monthly_wage` | 403.7 | 403.7 | ✅ |
| `price_index` | 104 | 104 | ✅ |
| `fulfillment_rate` | 11.2 | 11.2 | ✅ |
| `real_wage_index` | 388.2 | 388.2 | ✅ |

## 推定原因の判断基準

| 観察 | 推定原因 |
|------|---------|
| 数値カラムが微妙にずれている (端数差、丸め差) | 集計年度が異なる / 集計ロジック更新 |
| 数値が大きく異なる (オーダー違い) | データソース変更 / 単位変更 (% vs 比率) |
| 1 列だけ異なる、他は完全一致 | カラム追加・型変更による再投入 |
| local が古く Turso が新しい | Turso が正本、ローカルは古いキャッシュ |
| 主キーが違う行が混入 | ヘッダー混入 / フィルタ条件差 |

---

## SAMPLE_MISMATCH 真因確定 (2026-05-04 追補)

主要市区町村 (北海道札幌市, 東京都新宿区, 大阪府大阪市) で **0 差分** だったため、
追加で 5 テーブルすべての `ORDER BY rowid LIMIT 5` を local/Turso 両方で取得し、
verify_turso_v2_sync.py の SAMPLE_MISMATCH 判定の真因を特定した。

### 比較結果

すべて **データ内容は完全一致**。値そのものは local/Turso で同じ。

### 真因: Turso HTTP API の **型表現差**

verify_turso_v2_sync.py の SHA256 比較は値の型を区別する。Turso の v2/pipeline API は
**整数値を JSON 文字列として返す** ため、ローカル sqlite3 (整数のまま返却) と
SHA256 が一致しない。

#### 観察例 (`v2_external_population` rowid=2)

| ソース | 結果 |
|--------|------|
| ローカル sqlite3 | `('北海道', '札幌市', 1973395, '2020-10-01')` ← integer |
| Turso v2/pipeline | `('北海道', '札幌市', '1973395', '2020-10-01')` ← **string** |

データとしては同値だが、Python の SHA256 計算で型が異なるため違うハッシュに。

#### 観察例 (`v2_external_prefecture_stats` rowid=3)

| ソース | 結果 |
|--------|------|
| ローカル | `('岩手県', 2.1, 267)` |
| Turso | `('岩手県', 2.1, 267.0)` |

整数 vs 浮動小数点表現の違い (267 ↔ 267.0)。

### ヘッダー混入の確認 (副次)

| テーブル | ローカル | Turso |
|---------|:-------:|:-----:|
| `v2_external_population` | 1 件 | **1 件** (両方混入) |
| `v2_external_migration` | 0 件 | 0 件 |
| `v2_external_daytime_population` | 0 件 | 0 件 |
| `v2_external_population_pyramid` | 0 件 | 0 件 |
| `v2_external_prefecture_stats` | 0 件 | 0 件 |

`v2_external_population` のヘッダー混入は **両方に存在**。本資料 §SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md の WHERE フィルタで対応する方針に変更なし。

### 結論

**SAMPLE_MISMATCH 5 件はすべて Turso HTTP API の型表現差のみ。実データは完全一致。**

→ Phase 3 では問題ない (Rust ハンドラは `query_turso_or_local` 経由で Turso 値を JSON Value として受け取り、必要に応じて型変換する設計のため)。

### Phase 3 着手可否 (追補後)

| ステータス | 該当 | 影響 |
|-----------|------|------|
| ❌ COUNT_MISMATCH | `v2_external_foreign_residents` のみ (1742 vs 282) | **要詳細調査** (本追補スコープ外) |
| ⚠️ SAMPLE_MISMATCH (5 件) | population / migration / daytime / pyramid / prefecture_stats | **解消済み** (型表現差のみ、データ一致) |
| 🔴 LOCAL_MISSING (29 件) | 多数 | Phase 3 で Turso 優先参照する場合は問題なし |
| 🟡 REMOTE_MISSING (2 件) | minimum_wage, commute_od | **要 upload** (commute_od は Phase 3 でクリティカル) |

### 推奨次アクション

1. ✅ SAMPLE_MISMATCH 解消確認 (本追補で完了)
2. **次優先**: `v2_external_foreign_residents` の COUNT_MISMATCH (1742 vs 282) 詳細調査
3. **次優先**: SalesNow 正本決定 (Turso V2 内 `v2_salesnow_companies` vs SalesNow 専用 Turso)
4. REMOTE_MISSING 2 件のアップロード手順書作成

### 副次知見: verify_turso_v2_sync.py の改善余地

将来、SAMPLE_MISMATCH の判定ロジックは以下に変更すると有意な差分のみ検出できる:
- `int(value)` / `float(value)` 正規化を経由してから SHA256 比較
- または、各カラム値を文字列で `str()` 化してから正規化比較

→ 本セッションでは改修せず (Phase 3 着手時に判断)。

---

生成: `scripts/inspect_turso_local_diff.py` + `.playwright-mcp/inspect_rowid_first_rows.py` (2026-05-04)
