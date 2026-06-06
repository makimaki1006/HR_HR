# データカタログ & 更新ランブック (V2 hellowork-deploy)

**作成**: 2026-06-05 / **方針**: データソースを体系管理し「○○のデータを更新」時に
取得→加工→投入→アプリ反映 がスムーズに回る状態を作る。静的解析ベース (DB 実接続なし)。

> 🔴 CLAUDE.md: Claude による DB 書き込み (INSERT/UPDATE/DELETE/インポート) は禁止。
> 本書の「投入」「Turso 反映」手順は **ユーザーが手動実行** する。Claude はコード修正・
> CSV 生成・カタログ保守のみ。

---

## 0. データ源の分類 (5系統)

| 系統 | 取得元 | 代表データ | 更新主体 |
|------|--------|-----------|---------|
| **HW 本体** | ハローワーク掲載求人 (スクレイピング/既存SQLite) | postings, facilities | 継続更新 (別パイプライン) |
| **HW 時系列** | HW 月次スナップショット | ts_turso_* (求人数/給与/欠員/充足) | 月次 |
| **SalesNow** | HubSpot API (外部企業DB) | v2_salesnow_companies | 不定期 |
| **e-Stat** | 政府統計 API (statsDataId 指定) | v2_external_* の大半 | 年次/5年次/不定期 |
| **手動DL / 他API** | e-Stat CSV手動DL・日銀API・国土数値情報・CSIS | migration, daytime, boj_tankan, land_price, geocode | 不定期 |

**データソース帰属ルール (MEMORY reference_data_source_attribution)**:
SalesNow=企業/採用シグナル/人員推移、HW=求人/給与、国勢=人口、e-Stat=求人倍率/最賃。
混用禁止 (「SalesNow で給与可視化」等は誤用)。

---

## 1. データカタログ (全データソース × 4軸 + 運用メタ)

凡例 — 活用状態: ✅表示4タブ / 📄navyレポート / ⚠️非表示タブのみ(要移植検討) / ❌未活用
粒度: 県=都道府県 / 市=市区町村 / 全=全国

### 1-A. HW 求人本体

| データ | 取得元 | 取得→加工→投入 | テーブル | 粒度 | 鮮度カラム | 更新頻度 | 活用状態 |
|--------|--------|---------------|---------|------|-----------|---------|---------|
| 求人 | HW掲載 | 別パイプライン (スクレイピング→分類→SQLite) | postings | 市 (郡名込み) | — | 継続 | ✅ 全4タブ + 📄 |
| 施設 | HW掲載 | 同上 | facilities | 施設 | — | 継続 | ✅ |

### 1-B. HW 時系列 (ts_turso_*)

| データ | 取得元 | 取得→加工→投入 | テーブル | 粒度 | 鮮度 | 更新頻度 | 活用状態 |
|--------|--------|---------------|---------|------|------|---------|---------|
| 求人数推移 | HWスナップショット | (月次集計)→upload_to_turso.py | ts_turso_counts | 県×産業×雇用形態 | snapshot_id | 月次 | ✅トレンド + 📄 |
| 給与推移 | 同上 | 同上 | ts_turso_salary | 県×業界大分類 | snapshot_id | 月次 | ✅トレンド |
| 欠員推移 | 同上 | 同上 | ts_turso_vacancy | 県 | snapshot_id | 月次 | ✅トレンド |
| 充足推移 | 同上 | 同上 | ts_turso_fulfillment | 県 | snapshot_id | 月次 | ⚠️ |

> ⚠️ snapshot_id のフォーマット定義が未統一 (YYYYMM 推奨)。muni 粒度なし (県のみ)。

### 1-C. SalesNow (企業データ)

| データ | 取得元 | 取得→加工→投入 | テーブル | 粒度 | 鮮度 | 更新頻度 | 活用状態 |
|--------|--------|---------------|---------|------|------|---------|---------|
| 企業マスタ44項目 | HubSpot API | fetch_salesnow_companies.py → upload_salesnow_to_turso.py | v2_salesnow_companies | 法人番号 | collated_at | 不定期 | ✅企業検索/地図 |
| 業界マッピング | 導出 | 同upload | v2_industry_mapping | 業界 | — | 不定期 | ✅ |
| 企業ジオコード | HubSpot+CSIS | build_company_geocode.py → upload_company_geocode_to_turso.py | v2_company_geocode | 法人番号 | — | 不定期 | ✅地図 |

### 1-D. e-Stat (政府統計 API)

| データ | statsDataId | 取得script | 加工/投入script | テーブル | 粒度 | 鮮度 | 頻度 | 活用 |
|--------|-------------|-----------|----------------|---------|------|------|------|------|
| 人口(基本) | 0003445236 | fetch_estat_resident_population.py | ingest_resident_to_external_population.py | v2_external_population | 市 | reference_date | 5年 | ✅地図/企業/📄 |
| 人口ピラミッド | 0003445236 | 同上 | 同上 | v2_external_population_pyramid | 市×5歳階級 | ⚠️なし | 5年 | ✅地図/📄 |
| 職業別人口 | 0003454508 | fetch_estat_15_1.py | ingest_estat_15_1_to_local.py | municipality_occupation_population | 市×職業×年齢×性 | survey_year | 5年 | ⚠️詳細分析(MI) |
| 学歴 | 0003450543 | fetch_census_demographics.py | import_external_csv→upload_new_external | v2_external_education | 県×学歴 | ⚠️なし | 5年 | ✅求人検索 |
| 世帯 | 0003445080 | fetch_census_demographics.py | 同上 | v2_external_household | 県×世帯型 | ⚠️なし | 5年 | ✅求人検索 |
| 労働統計19指標 | 0000010206 | import_estat_labor_stats.py | (同scriptで直投) | v2_external_labor_stats | 県×年度 | fiscal_year | 年次 | ✅求人検索/📄 |
| 有効求人倍率(推移) | 0000010206 | import_estat_timeseries.py | (直投) | v2_external_job_openings_ratio | 県×年度 | fiscal_year | 年次 | ✅求人検索 |
| 事業所数 | 0004005687 | fetch_establishments.py | (直投) | v2_external_establishments | 県×産業 | reference_year | 5年 | ✅企業検索 |
| 産業構造(市区町村) | 0003449718 | fetch_industry_structure.py | ingest_industry_structure_to_local.py | v2_external_industry_structure | 市×産業 | ⚠️なし | 5年 | ✅求人/企業検索 |
| 家計支出 | 0002070003 | fetch_household_spending.py | upload_new_external | v2_external_household_spending | 県×カテゴリ | year | 年次 | ✅求人検索 |
| 入離職率 | 0003376330他 | fetch_turnover_data.py | (直投) | v2_external_turnover | 県×年度×産業 | fiscal_year | 年次 | ✅求人検索 |
| 在留外国人 | 0003147704 | fetch_foreign_residents.py | import_external_csv→upload | v2_external_foreign_residents | 市 | survey_period | 不定期 | ⚠️詳細分析 |
| 通勤OD | 0003454527 | fetch_commute_od.py | (直投+build_commute_flow_summary.py) | v2_external_commute_od(_with_codes) | 市OD | reference_year | 不定期 | ✅地図 + ⚠️詳細分析 |
| 賃貸住宅(m²単価) | 0004021493 | fetch_rental_housing.py | upload_new_external | v2_external_rental_housing | 市 | as_of | 5年 | ✅地図 |
| 開廃業率 | (経センサス) | fetch_business_dynamics.py | (直投) | v2_external_business_dynamics | 県×年度 | fiscal_year | 年次 | ✅企業検索(移植済) |
| 気候 | 0000010102 | fetch_climate_data.py | (直投) | v2_external_climate | 県×年度 | fiscal_year | 年次 | ✅企業検索(移植済) |
| 介護需要 | 0000010110他 | fetch_care_demand.py | (直投) | v2_external_care_demand | 県×年度 | fiscal_year | 年次 | ❌(対象外) |
| 社会生活 | (社会生活基本) | (CSV) | upload_new_external | v2_external_social_life | 県×カテゴリ | survey_year | 不定期 | ✅求人検索 |

### 1-E. 手動DL / 他API

| データ | 取得元 | 取得→加工→投入 | テーブル | 粒度 | 鮮度 | 頻度 | 活用 |
|--------|--------|---------------|---------|------|------|------|------|
| 社会移動 | 手動DL CSV | import_external_csv.py | v2_external_migration | 市 | reference_year | 不定期 | ✅地図 |
| 昼夜間人口 | 手動DL CSV | import_external_csv.py | v2_external_daytime_population | 市 | reference_year | 5年 | ✅求人/企業検索 |
| 日銀短観DI | 日銀STAT-SEARCH API | fetch_boj_tankan.py | upload_new_external | v2_external_boj_tankan | 全×業種×規模 | survey_date | 四半期 | ✅企業検索(移植済) |
| 地価 | 国土数値情報ZIP | fetch_geo_supplement.py --land | upload_new_external | v2_external_land_price | 県×地目 | year | 不定期 | ✅企業検索(移植済) |
| 自動車保有 | e-Stat | fetch_geo_supplement.py --car | upload_new_external | v2_external_car_ownership | 県 | year | 不定期 | ✅企業検索(移植済) |
| ネット利用率 | e-Stat | fetch_geo_supplement.py --net | upload_new_external | v2_external_internet_usage | 県 | year | 不定期 | ❌未活用 |

### 1-F. 分析テーブル (compute_v2_*.py が postings + 外部統計から生成)

| テーブル | 生成script | 活用状態 |
|---------|-----------|---------|
| v2_salary_structure / _competitiveness / _compensation_package | compute_v2_salary.py | ✅求人検索(移植済 market_forecast) + 📄 |
| v2_vacancy_rate / v2_regional_resilience / v2_transparency_score | compute_v2_analysis.py | ⚠️詳細分析タブのみ |
| v2_text_quality / keyword_profile / text_temperature / cross_industry_competition / anomaly_stats / cascade_summary | compute_v2_text/phase2.py | ⚠️詳細分析タブのみ |
| v2_employer_strategy(_summary) / monopsony_index / spatial_mismatch | compute_v2_market.py | ⚠️詳細分析タブのみ |
| v2_wage_compliance / v2_region_benchmark / v2_prefecture_stats | compute_v2_external.py | 📄 + ⚠️ |
| v2_fulfillment_score(_summary) / mobility_estimate / shadow_wage | compute_v2_prediction.py | ⚠️採用診断/詳細分析のみ |

---

## 2. ハードコード手動更新箇所 (毎年度 手で直す)

`scripts/compute_v2_external.py` 内に集約された政策データ。年度更新時に値を差し替える:

| 定数 | 内容 | 更新タイミング |
|------|------|--------------|
| MINIMUM_WAGE_2025 | 都道府県別最低賃金(時給) | 10月施行後 |
| JOB_OPENING_RATIO_202601 | 有効求人倍率(季節調整) | 月次/確定後 |
| UNEMPLOYMENT_RATE_2024 | 失業率(モデル推計) | 年確定後 |
| JOB_CHANGE_DESIRE_RATE_2022 | 転職者比率(就業構造基本調査) | 5年(調査年) |
| NON_REGULAR_RATE_2022 | 非正規雇用比率 | 5年 |
| AVG_MONTHLY_WAGE_2024 | 平均月給(賃金構造基本統計) | 年確定後 |

`scripts/upload_minimum_wage_history.py`: PREF_2023/2024/2025 (最賃推移) も同期更新。
`scripts/fetch_household_spending.py`: TIME_CODE / REFERENCE_YEAR を毎年更新。

> 🟡 改善余地: これらを `scripts/masters/<name>_<year>.yaml` に外出しし、CI で
> 「47都道府県揃っているか・数値範囲妥当か」を検証する案 (将来)。現状は .py 内 dict。

---

## 3. 既知の課題 (データ品質・鮮度)

| 課題 | 該当 | 影響 | 対応 |
|------|------|------|------|
| 鮮度カラム欠損 | population_pyramid / 全分析テーブル / prefecture_stats | 「いつのデータか不明」 | created_at 自動カラム追加 (将来) |
| 粒度の穴 (県のみ) | labor_stats / job_openings_ratio / establishments / turnover / business_dynamics / climate / care_demand / boj_tankan(全国) | 市区町村施策に使えない | e-Stat 市区町村版取得 (将来) |
| 郡名 mismatch | postings(郡込み) ↔ v2_external_*(郡なし) | JOIN 失敗 | strip_county_prefix 適用済 (要全SQL点検) |
| 死蔵分析テーブル | §1-F の ⚠️ 群 | 非表示タブのみ=ユーザー不可視 | 表示4タブへの移植検討 (Wave1-A/D は移植済) |
| ネット利用率 未活用 | v2_external_internet_usage | 投入済だが参照ゼロ | 活用 or 投入見送り判断 |
| snapshot_id 未定義 | ts_turso_* | 時点フォーマット不統一 | YYYYMM 標準化 |

---

## 4. 更新ランブック (「○○のデータを更新」時の標準手順)

### 共通フロー (全データ共通の4ステップ)

```
[1] 取得   fetch_*.py 実行 → data/*.csv 生成 (または直接ローカルDB)
[2] 加工   compute_*.py / ingest_*.py で集計・正規化・単位変換・郡名処理
[3] 投入   (ユーザー手動) upload_*.py で Turso へ。投入前に validate_all_csvs.py
[4] 反映   アプリは Turso/ローカルDB を都度参照するためコード変更不要。
           ただし新規テーブル追加時のみ handler/route 追加が必要
```

### 頻度別ランブック

**月次 (HW時系列)**
1. HW月次スナップショット集計 → ts_turso_* 生成
2. (手動) `upload_to_turso.py` で ts_turso_* を Turso へ
3. アプリ反映: 自動 (トレンドタブが都度参照)

**四半期 (日銀短観)**
1. `fetch_boj_tankan.py` (発表日後)
2. (手動) `upload_new_external_to_turso.py --table v2_external_boj_tankan`
3. 反映: 自動 (企業検索タブが参照)

**年次 (最賃・失業率・求人倍率・家計等)**
1. `compute_v2_external.py` のハードコード dict を新年度値に更新 (§2)
2. `fetch_household_spending.py` の TIME_CODE 更新 → 実行
3. `import_estat_labor_stats.py` / `import_estat_timeseries.py` / `fetch_turnover_data.py` 実行
4. `compute_v2_external.py` 実行 (prefecture_stats/region_benchmark 再計算)
5. (手動) `upload_to_turso.py` + `upload_minimum_wage_history.py`
6. 反映: 自動

**5年次 (国勢調査・経済センサス)**
1. 各 statsDataId を新年度版に更新 (§1-D の id 列)
2. `fetch_estat_*.py` → `ingest_*.py` の順で実行 (DAG: §棚卸しA 参照)
3. (手動) `upload_to_turso.py` / `upload_new_external_to_turso.py`
4. 反映: 自動 (粒度・カラムが変わらない限り)

**不定期 (SalesNow・手動DL)**
1. SalesNow: `fetch_salesnow_companies.py` → (手動)`upload_salesnow_to_turso.py`
2. 手動DL系: 政府ポータルから CSV DL → `import_external_csv.py` → (手動)upload

### 新規データソース追加時 (テーブル新設を伴う場合)

```
1. fetch_<name>.py (取得) + CREATE TABLE 定義
2. compute/ingest (加工) — 既存パターン踏襲。郡名処理・単位明記・鮮度カラム付与
3. upload_<name> or upload_new_external_to_turso.py に追加 (手動投入)
4. アプリ側:
   - src/handlers/<tab>/external*.rs に fetch + render パネル追加
   - handlers.rs に endpoint、mod.rs に export、lib.rs に route
   - テンプレート(tabs/*.html)に hx-get パネル追加 → 表示4タブに必ず接続
   - ⚠️ 非表示タブ(analysis/karte等)に追加しない (Wave1-A/D の失敗例: UI不可視)
5. テスト: silent fallback禁止 / 粒度明記 / 出典 / 中立表現
```

---

## 5. 共通化の設計提案 (都度ロジックを作らない仕組み)

現状は fetch_*/compute_*/upload_* がデータソースごとに独立実装。共通化するなら:

### 案1: データソースレジストリ (推奨・低コスト)
`scripts/data_sources.yaml` に全データソースを1レコードで定義:
```yaml
- name: minimum_wage
  source: e-stat
  stats_data_id: "0000010206"
  fetch: fetch_geo_supplement.py
  transform: compute_v2_external.py
  table: v2_external_minimum_wage
  granularity: prefecture
  freshness_col: effective_date
  frequency: yearly
  manual_consts: [MINIMUM_WAGE_2025]   # 手で直す箇所
  tabs: [competitive]                  # 活用先タブ
```
- 本 DATA_CATALOG.md の表をそのまま機械可読化したもの
- `scripts/pipeline.py --update minimum_wage` で fetch→transform→(投入指示) を自動実行
- CI で「レジストリの table が実DBに存在するか」「tabs が表示4タブか」を検証

### 案2: orchestration script (中コスト)
`scripts/pipeline_orchestration.py` で依存DAG (§棚卸しA) を定義し、
`--frequency yearly` で該当データを一括更新。失敗タスクのみ再実行。

### 案3: 鮮度メタ統一 (品質向上)
全テーブルに `created_at` / `source_as_of` を必須化し、アプリの各パネル caption に
「データ時点: YYYY-MM」を自動表示 (鮮度の透明性)。

> いずれも CLAUDE.md「Claude DB書込禁止」を踏襲し、投入(upload)はユーザー手動のまま。
> Claude が担うのは レジストリ保守・fetch/transform コード・検証スクリプト。

---

## 6. 参照

- 棚卸しA (取得・加工): scripts/ の fetch/compute/upload 全フロー + ハードコード一覧
- 棚卸しB (DB): 全テーブルスキーマ・粒度・鮮度カラム
- 棚卸しC (活用): テーブル→handler→タブ の参照経路
- 関連: scripts/upload_to_turso.py, upload_new_external_to_turso.py, compute_v2_external.py
