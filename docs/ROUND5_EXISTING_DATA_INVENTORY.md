# ROUND5 既存データ拡張棚卸し (Worker E)

**作成日**: 2026-05-06
**対象**: `data/hellowork.db` ローカル SQLite + `data/` 配下 CSV + `docs/` 既存設計
**禁則**: 新規 Web 探索 / API call / Turso 書き込み / target_count・推定人数・想定人数・母集団人数 等の用語

---

## 1. 即利用可 (UI 表示まで素直に通せる)

| データ名 | 場所 | 規模 | MarketIntelligence UI での使い道 |
|---|---|---|---|
| `municipality_code_master` | `data/hellowork.db` / `data/generated/municipality_code_master.csv` | 1,917 行 (1,918 行 CSV) | 市区町村フィルタ・名寄せ・JIS 紐付けのマスタ。全タブ共通の参照元 |
| `v2_external_population` | `data/hellowork.db` | 1,917 行 | 市区町村カードの基礎KPI (総人口/年齢三区分/高齢化率)。既に `v2_external_*` で UI 接続実績あり |
| `v2_external_population_pyramid` | `data/hellowork.db` | 17,235 行 | 年代別ピラミッドチャート。市区町村ドリルダウンで使える |
| `v2_external_minimum_wage` | `data/hellowork.db` | 47 行 | 都道府県カードの参考値表示。既存タブで使用済 |
| `v2_external_prefecture_stats` | `data/hellowork.db` | 47 行 | 都道府県マクロ指標 (有効求人倍率・実質賃金index 等)。`v2_external_job_opening_ratio` (47) と併用可 |
| `v2_external_daytime_population` | `data/hellowork.db` | 1,740 行 | 昼夜間人口比 → 「働く人が流入する街か」のラベルに直結 |
| `v2_external_foreign_residents` | `data/hellowork.db` | 1,742 行 | 外国人比率を市区町村スライスで表示可能 |
| `v2_external_migration` | `data/hellowork.db` | 1,741 行 | 社会増減フロー。地域選好シグナルとして即利用可 |
| `municipality_geocode` | `data/hellowork.db` | 2,626 行 | 市区町村ピン地図用。MarketIntelligence の地図レイヤに直結 |

## 2. 少し加工すれば使える

| データ名 | 場所 | 規模 | 加工内容 | UI 用途 |
|---|---|---|---|---|
| `commute_flow_summary` | DB / `data/generated/commute_flow_summary.csv` | 27,879 / 27,880 行 | `municipality_code_master` で名寄せ済。表示用に top-N 抽出 + 自治体ペアラベル付与 | 「他地域からの流入元 TOP10」パネル |
| `v2_commute_flow_summary` | DB | 3,786 行 | direction 別 (in/out) 集計済。top10_json をパースして表示 | 通勤フロー方向別の要約カード |
| `v2_external_commute_od` / `_with_codes` | DB | 86,762 行 ×2 | code 付きを優先採用。重複 OD は除外済 | 通勤 OD ヒートマップの裏データ |
| `v2_municipality_target_thickness` | DB / `data/generated/v2_municipality_target_thickness.csv` | 20,845 / 21,066 行 | カラム `thickness_index`, `rank_percentile`, `distribution_priority` をそのままスコア表示 (※ NG 用語に該当する文言は出力時にラベル変換が必要) | 市区町村×職種の優先度ランキング |
| `municipality_recruiting_scores` | DB | 20,845 行 | 5 つの構成スコアを stacked bar / radar に整形 | 採用環境スコアカード |
| `municipality_living_cost_proxy` | DB | 1,917 行 | `cost_index`, `min_wage`, `land_price_proxy` を min-max 正規化 | 生活コスト指標カード |
| `municipality_occupation_population` | DB | 729,949 行 | basis × occupation × age × gender でピボット。表示単位を occupation_group へ集約 | 職種別の地域分布ビュー |
| `data/generated/occupation_industry_weight.csv` | CSV | 232 行 | 職種↔産業マッピング表。DB 化はせず JSON 化で十分 | フィルタ間の橋渡し参照表 |
| `data/agoop/posting_mesh1km_20260401.csv` | CSV | 49,370 行 | 月次スナップショット。mesh1km × 求人件数のみ。`v2_posting_mesh1km` 系の月次合流が既設 | 1km メッシュの求人密度ヒートマップ |
| `data/agoop/turso_csv/attribute_mesh1km.csv` | CSV | 387,501 行 | メッシュ属性 (居住・就業) を join 用に整形 | 求人密度 / メッシュ属性のクロス |
| `data/agoop/turso_csv/fromto_city.csv` | CSV | 2,340,981 行 | 市区町村粒度の人流 OD。`v2_external_commute_od` と粒度・指標が異なるため別レイヤとして扱う | 人流 OD タブの別系統 |

## 3. 品質確認が必要

| データ名 | 場所 | 規模 | リスク |
|---|---|---|---|
| `data/agoop/turso_csv/mesh1km_2019.csv` / 2020 / 2021 | CSV | 12〜13M 行 ×3 (合計約 38M 行 / 1.2GB) | 行数が大きく Turso 投入コストが重い。年次差分の整合性 (2019→2021 の欠損メッシュ) を `attribute_mesh1km` と突合する必要 |
| `data/csis_geocoded.csv` / `csis_checkpoint.csv` / `unique_addresses_for_csis.csv` | CSV | 237,105 / 237,482 / 237,482 行 | 行数は近いが完全一致ではなく、checkpoint と geocoded の差分が未消化の可能性。`iConf`/`iLvl` が低い行を除外する基準が docs に未明記 |
| `data/company_geocode.csv` | CSV | 237,518 行 | 求人事業所単位でのジオコード。`csis_geocoded.csv` との id 体系の同一性を確認しないと postings との結合精度がぶれる |
| `data/generated/estat_15_1_merged.csv` | CSV | 716,959 行 (87MB) | 産業×従業者規模の e-Stat 表。投入計画は `..._FETCH_ESTAT_15_1_PLAN.md` で議論済だが DB 未投入。スキーマ凍結が要 |
| `data/generated/estat_resident_merged.csv` | CSV | 69,013 行 | 居住人口の途中集計。`v2_external_population` と粒度・年次が一致するかの確認が必要 |
| `municipality_occupation_population` の `estimate_index` | DB | 729,949 行 | 計算元 (basis, weight_source) の混在状態を再点検しないと、表示時に「どの基底か」がユーザーに伝わらない |
| `salesnow_companies.csv` | CSV | 467,627 行 (470MB) | サイズ大。フィールド数 44 のうち UI 必須カラムを絞らないと取り回せない。`salesnow_aggregate_for_f6.csv` (11,072 行) が既に集約版として存在 |

## 4. 今回は使わない

| データ名 | 理由 |
|---|---|
| `layer_a_*` / `layer_b_*` / `layer_c_*` (10 テーブル) | V1 由来の派生指標。MarketIntelligence の本体スコープ外 |
| `v2_anomaly_stats` / `v2_cascade_summary` / `v2_employer_strategy*` / `v2_fulfillment_*` / `v2_text_*` / `v2_keyword_profile` ほか v2_* 派生テーブル群 | 既に他タブで利用中。Round5 の「拡張棚卸し」対象ではない |
| `data/generated/debug_mi_*` HTML | デバッグ成果物 |
| `data/generated/server_8080_std*.log` / `*_progress.json` / `*_audit_log.json` | 運用ログ。データ層では使わない |
| `data/hellowork.db.bak.before_jis_fetch` / WAL/SHM | バックアップ・実行時ファイル |
| `data/csis_batches/*.csv` (68 ファイル) | CSIS 投入用の都道府県別バッチ。`csis_geocoded.csv` に集約済のため UI 直結では不要 |

---

## 5. 既存設計 docs インデックス (Round5 で参照すべきもの)

| 分類 | docs |
|---|---|
| MarketIntelligence メトリクス本体 | `SURVEY_MARKET_INTELLIGENCE_METRICS.md`, `..._PHASE0_2_PREP.md` |
| 通勤フロー / OD | `..._PHASE3_BUILD_COMMUTE_FLOW_JIS_MIGRATION.md`, `..._COMMUTE_FLOW_SUMMARY_TABLE_INVESTIGATION.md`, `..._FETCH_COMMUTE_OD_REFACTOR.md`, `..._STEP_A_COMMUTE_FLOW_UPLOAD.md` |
| 職種×人口モデル | `..._OCCUPATION_POPULATION_FEASIBILITY.md`, `..._OCCUPATION_POPULATION_MODEL_V2.md`, `..._OCCUPATION_PROTO_EVALUATION.md`, `..._OCCUPATION_INDUSTRY_WEIGHT_HYPOTHESIS.md` |
| 居住人口 ingest | `..._RESIDENT_INGEST_PLAN_FINAL.md`, `..._RESIDENT_POP_SID_RESEARCH.md`, `..._RESIDENT_POP_SID_RESEARCH_PHASE2.md` |
| e-Stat 15_1 (産業×規模) | `..._ESTAT_15_1_FEASIBILITY.md`, `..._FETCH_ESTAT_15_1_PLAN.md`, `..._F2_*` 群 |
| 政令指定都市の取り扱い | `..._DESIGNATED_WARD_DATA_AUDIT.md`, `..._DESIGNATED_WARD_F2_DESIGN.md`, `..._DESIGNATED_WARD_INGEST_VALIDATION.md`, `..._UI_INTERIM_DESIGNATED_WARD.md` |
| 生活コスト・スコア | `..._PLUS_LIVING_COST_AND_SCORES.md` |
| Turso アップロード手順 | `..._TURSO_UPLOAD_PLAN.md`, `..._TURSO_UPLOAD_GUIDE_JIS.md`, `..._TURSO_UPLOAD_DIFF_AUDIT.md`, `..._TURSO_VERIFY.md`, `turso_import_agoop.md`, `turso_import_ssdse_phase_a.md` |
| Agoop (人流・メッシュ) | `design_agoop_backend.md`, `design_agoop_frontend.md`, `design_agoop_jinryu.md`, `requirements_agoop_jinryu.md` |
| SSDSE 拡張 | `design_ssdse_a_backend.md`, `design_ssdse_a_expansion.md`, `design_ssdse_a_frontend.md`, `requirements_ssdse_a_expansion.md` |
| SalesNow | `salesnow_source_comparison_2026-05-03.md` |

---

## 6. Round5 で次に実装するなら (棚卸し由来の優先順)

1. **市区町村カードの基礎拡張**: `v2_external_population` + `_pyramid` + `_daytime_population` + `_migration` + `_foreign_residents` + `municipality_geocode` を 1 ペイロードに束ねる API を 1 本追加。すべて DB 既投入のためコード差分のみで完結。
2. **通勤フロー要約パネル**: `v2_commute_flow_summary` (3,786 行) と `commute_flow_summary` (27,879 行) を direction × top-N で表示。既存 `top10_json` が活きる。
3. **市区町村×職種の採用環境ビュー**: `municipality_recruiting_scores` (20,845 行) と `v2_municipality_target_thickness` (20,845 行) を 1:1 結合。**カラム名の `thickness_index` / `distribution_priority` を UI ラベルに変換する際、Hard NG 用語 (target_count / 推定人数 / 想定人数 / 母集団人数) は使用しない**。
4. **生活コスト × 最低賃金 オーバーレイ**: `municipality_living_cost_proxy` + `v2_external_minimum_wage` + `v2_external_prefecture_stats` を都道府県粒度の補助レイヤとして追加。
5. **Agoop 月次メッシュの軽量採用**: `data/agoop/posting_mesh1km_20260401.csv` (49,370 行) のみを先行採用し、12M 行級の `mesh1km_2019/2020/2021` は今期スコープ外。

---

## 7. 編集ファイル

- 絶対パス: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\docs\ROUND5_EXISTING_DATA_INVENTORY.md`
