# Turso V2 同期検証レポート

- 実行日時 (UTC): 2026-05-03T15:13:10.018264+00:00 〜 2026-05-03T15:13:12.985102+00:00
- 所要時間: 3.0 秒
- ローカル DB: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db`
- リモート: `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` (Turso V2)
- READ 消費: 13 (上限 100)

## サマリ

| ステータス | 件数 | 意味 |
|-----------|-----:|------|
| ✅ MATCH | 0 | ローカル・リモート完全一致 |
| ❌ COUNT_MISMATCH | 1 | 行数が異なる |
| ⚠️ SAMPLE_MISMATCH | 5 | 行数は同じだが先頭 5 行のハッシュが異なる |
| 🔴 LOCAL_MISSING | 29 | ローカルに不在 (リモートのみ存在) |
| 🟡 REMOTE_MISSING | 2 | リモートに不在 (ローカルのみ存在、要 upload) |
| ⚪ BOTH_MISSING | 0 | 両方に不在 |
| ⏸️ READ_LIMIT | 0 | READ 上限到達で検証スキップ |

## テーブル別結果

| テーブル | 状態 | ローカル行数 | リモート行数 | ハッシュ一致 |
|---------|------|-----------:|------------:|:-----------:|
| `v2_external_population` | ⚠️ SAMPLE_MISMATCH | 1742 | 1742 | ❌ |
| `v2_external_migration` | ⚠️ SAMPLE_MISMATCH | 1741 | 1741 | ❌ |
| `v2_external_foreign_residents` | ❌ COUNT_MISMATCH | 1742 | 282 | — |
| `v2_external_daytime_population` | ⚠️ SAMPLE_MISMATCH | 1740 | 1740 | ❌ |
| `v2_external_population_pyramid` | ⚠️ SAMPLE_MISMATCH | 15660 | 15660 | ❌ |
| `v2_external_prefecture_stats` | ⚠️ SAMPLE_MISMATCH | 47 | 47 | ❌ |
| `v2_external_job_openings_ratio` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_labor_stats` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_establishments` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_turnover` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_household_spending` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_business_dynamics` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_climate` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_care_demand` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_turso_counts` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_turso_vacancy` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_turso_salary` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_turso_fulfillment` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_agg_workstyle` | 🔴 LOCAL_MISSING | - | - | — |
| `ts_agg_tracking` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_industry_structure` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_land_price` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_minimum_wage` | 🟡 REMOTE_MISSING | - | - | — |
| `v2_external_commute_od` | 🟡 REMOTE_MISSING | - | - | — |
| `v2_external_education` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_education_facilities` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_households` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_household` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_internet_usage` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_car_ownership` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_geography` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_social_life` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_vital_statistics` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_labor_force` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_medical_welfare` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_boj_tankan` | 🔴 LOCAL_MISSING | - | - | — |
| `v2_external_minimum_wage_history` | 🔴 LOCAL_MISSING | - | - | — |

## 追加発見: リモートのみに存在するテーブル

(検証対象 TARGET_TABLES に未登録)

- `v2_flow_attribute_mesh1km`
- `v2_flow_city_agg`
- `v2_flow_fromto_city`
- `v2_flow_master_city`
- `v2_flow_master_dayflag`
- `v2_flow_master_region`
- `v2_flow_master_regioncode`
- `v2_flow_master_timezone`
- `v2_flow_mesh1km_2019`
- `v2_flow_mesh1km_2020`
- `v2_flow_mesh1km_2021`
- `v2_flow_mesh3km_agg`
- `v2_industry_mapping`
- `v2_posting_mesh1km`
- `v2_salesnow_companies`

## 追加発見: ローカルのみに存在するテーブル

(検証対象 TARGET_TABLES に未登録)

- `layer_a_employment_diversity`
- `layer_a_facility_concentration`
- `layer_a_salary_stats`
- `layer_b_cooccurrence`
- `layer_b_keywords`
- `layer_b_text_quality`
- `layer_c_cluster_profiles`
- `layer_c_clusters`
- `layer_c_region_heatmap`
- `municipality_geocode`
- `postings`
- `v2_anomaly_stats`
- `v2_cascade_summary`
- `v2_commute_flow_summary`
- `v2_compensation_package`
- `v2_cross_industry_competition`
- `v2_employer_strategy`
- `v2_employer_strategy_summary`
- `v2_external_job_opening_ratio`
- `v2_fulfillment_score`
- `v2_fulfillment_summary`
- `v2_keyword_profile`
- `v2_mobility_estimate`
- `v2_monopsony_index`
- `v2_region_benchmark`
- `v2_regional_resilience`
- `v2_salary_competitiveness`
- `v2_salary_structure`
- `v2_shadow_wage`
- `v2_spatial_mismatch`
- `v2_text_quality`
- `v2_text_temperature`
- `v2_transparency_score`
- `v2_vacancy_rate`
- `v2_wage_compliance`

## 推奨対応

- **MATCH**: アクション不要。
- **COUNT_MISMATCH / SAMPLE_MISMATCH**: ローカル → リモートを `upload_to_turso.py` で再アップロード推奨。
- **REMOTE_MISSING**: ローカルに新規投入後、`upload_to_turso.py` で初回アップロード。
- **LOCAL_MISSING**: リモートにのみ存在。Phase 3 で必要なら `download_db.sh` 等で同期。
- **BOTH_MISSING**: Task A の投入手順書 (`SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_INGEST.md`) で投入。
- **READ_LIMIT**: 翌月 (READ クォータリセット後) または上限緩和後に再実行。

## 安全装置の動作確認

- WRITE 系 SQL 検出: 0 件 (本実行で WRITE 系クエリ未発行)
- READ 上限到達: なし
- 認証 token 露出: 本レポートに転記なし

---

生成: `scripts/verify_turso_v2_sync.py` (2026-05-03)