# 媒体分析レポート データ更新・再現 Runbook

作成日: 2026-05-11  
対象: 採用コンサル PDF / Market Intelligence variant  
目的: ここまでの Round 8〜10 の実装成果を整理し、各データを取得し直した時に同じ手順でローカル DB・Turso・本番 PDF へ反映できる状態を作る。

---

## 1. ここまでに到達した状態

Round 8〜10 で、媒体分析 PDF は「CSV 単体の求人集計」から「CSV とオープンデータを役割分離して採用示唆を出すレポート」へ寄せた。

| 領域 | PDF での主な表示 | 主データ |
|---|---|---|
| 地域 × 職業 × 性別 × 年齢 | 対象自治体ごとの職業 Top 5、女性比、年齢構成、採用示唆 | `municipality_occupation_population` |
| 地域 × 産業 × 性別 | 対象自治体ごとの産業 Top、男女別従業者構成 | `v2_external_industry_structure` |
| CSV 求人数 × 地域母集団 | 4 象限図、推奨アクション、集約地域注記 | CSV 集計 + 公的統計 |
| 配信地域ランキング | 配信優先度スコア、代表職種、スコア区分 | `v2_municipality_target_thickness` + `municipality_recruiting_scores` |
| 最低賃金 | 最低賃金比、都道府県別最低賃金、給与妥当性 | `v2_external_minimum_wage` |
| PDF 品質 | A4 収まり、グラフ見切れ防止、実 PDF 検証 | Playwright + PyMuPDF |

設計上の重要ルール:

- CSV に業界列・職種列がない限り、CSV 給与を「業界別給与」「職種別給与」として断定表示しない。
- MI variant では、ハローワーク由来の文言・データを安易に混ぜない。使う場合は別 variant / 別章で明示する。
- E2E PASS や DOM grep だけでは完了扱いにしない。PDF 実物生成後のテキスト抽出・必要に応じた PNG / bbox 確認を完了条件にする。
- `municipality_recruiting_scores` は `0〜200` スケール。重点配信の閾値は `score >= 160`。
- Round 10 Phase 1B は全面 percentile 化ではない。`thickness=200` 上限到達グループ内だけを de-tie し、上限未到達自治体は変えない。

---

## 2. データソース・主要派生テーブル一覧と更新計画

| # | データソース / 派生テーブル | 用途 | ローカル / Turso / ファイル | 取得・生成スクリプト | 推奨更新頻度 | 更新トリガー |
|---|---|---|---|---|---|---|
| 1 | 顧客 CSV / Indeed CSV | 求人数、給与中央値、対象自治体、4 象限 X 軸 | セッション内 CSV 集計 | アプリ upload | 案件ごと | 顧客 CSV 受領時 |
| 2 | 国勢調査 R2 表 15-1 職業大分類 | 地域 × 職業 × 性別 × 年齢 | `municipality_occupation_population` / `data/generated/estat_15_1_merged.csv` | `fetch_estat_15_1.py` → `ingest_estat_15_1_to_local.py` | 5年ごと。年1回改訂確認 | 国勢調査更新、表 ID / 市区町村コード変更 |
| 3 | 経済センサス R3 産業構造 | 地域 × 産業 × 性別、4 象限 Y 軸、F2 厚み | `v2_external_industry_structure` / `scripts/data/industry_structure_by_municipality.csv` | `fetch_industry_structure.py` → `ingest_industry_structure_to_local.py` | 5年ごと。年1回改訂確認 | 経済センサス更新、自治体コード再編 |
| 4 | 市区町村人口 / 年齢構成 | 地域基礎情報、将来拡張 | `v2_external_population`, `v2_external_population_pyramid` | 既存 ingest 系 | 年1回確認 | e-Stat / 自治体統計更新 |
| 5 | 通勤 OD / 通勤流入 | 通勤流入、将来の Model F2 補正 | `v2_external_commute_od_with_codes`, `commute_flow_summary` | `fetch_commute_od.py` → `build_commute_flow_summary.py` | 5年ごと。国勢調査更新時 | 国勢調査 OD 更新 |
| 6 | 最低賃金 | 最低賃金比、給与妥当性 | `v2_external_minimum_wage`, `v2_external_minimum_wage_history` | `upload_minimum_wage_history.py` 等 | 年1回。原則 10月改定後 | 厚労省 / 都道府県別最低賃金改定 |
| 7 | 生活コスト proxy | 給与実質 proxy、4 象限補助 | `municipality_living_cost_proxy` | `build_municipality_living_cost_proxy.py` → `upload_living_cost_proxy.py` | 年1回 | 最低賃金、地価、生活費 proxy 更新 |
| 8 | 産業 × 職業重み仮説 | F2 thickness 推定 | `data/generated/occupation_industry_weight.csv` | 手動管理 / 仮説 CSV | 半年〜年1回レビュー | 商品方針、業界分類、職業分類見直し |
| 9 | SalesNow 集約 | F6 / 企業側補正 | `data/generated/salesnow_aggregate_for_f6.csv` ほか | SalesNow fetch / aggregate 系 | 月次〜四半期 | SalesNow データ更新、企業マスタ更新 |
| 10 | Model F2 thickness | 母集団厚み指数、職業別推定母集団 | `v2_municipality_target_thickness` / `data/generated/v2_municipality_target_thickness.csv` | `build_municipality_target_thickness.py` → `ingest_v2_thickness_to_local.py` | 上流更新時に必ず再生成 | #2/#3/#5/#8/#9 更新時。SalesNow は score 直接ではなく thickness 経由 |
| 11 | Recruiting scores | 配信地域ランキング、KPI、スコア区分 | `municipality_recruiting_scores` | `build_municipality_recruiting_scores.py` | #10 更新後に必ず再生成 | thickness / commute / competition / living cost 更新時 |
| 12 | GeoJSON / 地図境界 | 地図表示、将来の地域可視化 | `static/geojson/*` | 既存地図生成 / 配布 | 年1回または行政区変更時 | 市区町村合併、境界変更 |

### 2.1 PDF / レポートで使用中の追加外部統計テーブル

媒体分析 PDF (survey report HTML / lifestyle セクション) で実参照されているテーブル。`hellowork.db` に未投入のものも含む点に注意。

| # | データソース / 派生テーブル | 用途 | ローカル / Turso / ファイル | 取得・生成スクリプト | 推奨更新頻度 | 更新トリガー |
|---|---|---|---|---|---|---|
| 13 | 社会生活基本調査 (社会生活参加率) | P-1 社会生活参加率 (PDF p17、`src/handlers/survey/report_html/lifestyle.rs:5`) | `v2_external_social_life` (現状 ローカル `hellowork.db` 未投入、Turso のみと推定 / 要確認) | (未確認、e-Stat 系 fetch script と推定) | 5年ごと (社会生活基本調査は 5 年周期、最新 2021) | 社会生活基本調査更新 |
| 14 | 通信利用動向調査 (ネット利用率) | P-2 ネット利用率 (PDF p17、`lifestyle.rs:12`) | `v2_external_internet_usage` (現状 ローカル未投入、Turso のみと推定 / 要確認) | (未確認、e-Stat 系 fetch script と推定) | 年1回 (総務省 通信利用動向調査、最新参照 2016 → 要更新候補) | 通信利用動向調査更新 |
| 15 | 人口ピラミッド | PDF p11 年齢構成可視化、survey granularity 検証で参照 (`survey/granularity.rs:17`) | `v2_external_population_pyramid` (ローカル 17,235 行) | 既存 ingest 系 | 5年ごと (国勢調査ベース) | 国勢調査更新。`reference_year` カラムなしのため source 改訂時に全置換 |

### 2.2 現状未活用の V2 外部・派生テーブル (調査・拡張候補)

`hellowork.db` には存在するが、媒体分析 PDF (survey report) では現状参照されていないテーブル群。Round 1-F (#168) 等の探索対象。Turso との同期方針・更新頻度は個別調査要。

| # | テーブル | 行数 (ローカル) | 想定用途 (schema 推定) | 更新頻度 (暫定) |
|---|---|---|---|---|
| 16 | `v2_external_prefecture_stats` | 47 | 県別マクロ指標 (失業率/賃金/物価指数等) | 年1回 |
| 17 | `v2_external_daytime_population` | 1,740 | 昼夜間人口、流入/流出。recruitment_diag opportunity_map で参照済 | 5年ごと (国勢調査) |
| 18 | `v2_external_foreign_residents` | 1,742 | 在留外国人数・比率 | 年1回 |
| 19 | `v2_external_migration` | 1,741 | 転入・転出、純移動 | 年1回 |
| 20 | `v2_external_job_opening_ratio` | 47 | 都道府県別有効求人倍率 | 月次 |
| 21 | `v2_anomaly_stats` | 10,800 | metric 別異常値検出 | 上流 HW posting 更新時 |
| 22 | `v2_cascade_summary` | 8,382 | prefecture × municipality × industry × emp_group 集約 (件数 / 給与 / 休日 / 空き率) | HW posting 更新時 |
| 23 | `v2_commute_flow_summary` | 3,786 | V2 版 通勤フロー集約 (`commute_flow_summary` と区別) | 5年ごと |
| 24 | `v2_compensation_package` | 11,757 | 給与+休日+賞与複合スコア、rank_label | HW posting 更新時 |
| 25 | `v2_cross_industry_competition` | 1,689 | 県 × 給与帯 × 学歴 × 雇用形態の業界横断競合 | HW posting 更新時 |
| 26 | `v2_employer_strategy` | 469,027 | 施設単位の戦略タイプ (premium / salary_focus 等) | HW posting 更新時 |
| 27 | `v2_employer_strategy_summary` | 20,605 | 上記の市区町村 × 業界 × 雇用形態集約 | HW posting 更新時 |
| 28 | `v2_fulfillment_score` | 149,696 | 施設別充足スコア / グレード | HW posting 更新時 |
| 29 | `v2_fulfillment_summary` | 2,762 | 市区町村 × 雇用形態の充足サマリ | HW posting 更新時 |
| 30 | `v2_keyword_profile` | 123,630 | キーワードカテゴリ別ヒット率 / 密度 | HW posting 更新時 |
| 31 | `v2_mobility_estimate` | 3,082 | 重力モデルによる吸引力 / 流出推定、top3_destinations | 通勤 OD + HW posting 更新時 |
| 32 | `v2_monopsony_index` | 20,605 | HHI / Gini / 上位 N シェアによる集中度 | HW posting 更新時 |
| 33 | `v2_region_benchmark` | 8,002 | 地域 × 雇用形態の総合ベンチマーク (14 指標 + 合成) | 上流複数テーブル更新時 |
| 34 | `v2_regional_resilience` | 2,809 | 産業多様性 (Shannon / HHI) | HW posting 更新時 |
| 35 | `v2_salary_competitiveness` | 11,757 | 地域給与 vs 全国給与、percentile_rank | HW posting 更新時 |
| 36 | `v2_salary_structure` | 22,759 | 給与 p10 / p25 / p50 / p75 / p90、賞与開示率 | HW posting 更新時 |
| 37 | `v2_shadow_wage` | 12,136 | 給与分布統計 (mean / stddev / IQR) | HW posting 更新時 |
| 38 | `v2_spatial_mismatch` | 3,082 | 重心緯度経度、30km / 60km 圏内アクセス可能求人、孤立度 | HW posting 更新時 |
| 39 | `v2_text_quality` | 20,605 | 文字数 / 漢字率 / 情報スコア | HW posting 更新時 |
| 40 | `v2_text_temperature` | 8,382 | 緊急度 / 選好性密度 (テキストヒート) | HW posting 更新時 |
| 41 | `v2_transparency_score` | 32,545 | 開示率 (年間休日 / 賞与 / 従業員数 / 設立年等) | HW posting 更新時 |
| 42 | `v2_vacancy_rate` | 32,545 | 空席率 / 成長率 / 新規施設数 | HW posting 更新時 |
| 43 | `v2_wage_compliance` | 2,263 | 最低賃金未達件数・比率 | 最低賃金 (#6) 改定時 + HW posting 更新時 |

注意:

- #13・#14 は `hellowork.db` 上に存在せず、Turso のみで保持されている可能性が高い。媒体 PDF 生成パイプラインが Turso direct query なのか、それとも別 DB / API 経由なのかは未確認。
- #16〜#43 は schema からの推定であり、生成元スクリプトと冪等再生成手順は別途棚卸し要 (Round 1-F #168 探索対象)。
- #26 `v2_employer_strategy` (469K 行) は単独サイズが大きいため、Turso 反映時の write budget を必ず dry-run で確認すること。
- #23 `v2_commute_flow_summary` と #5 系の `commute_flow_summary` (派生) は別物。両者を混同しない。
- 「更新頻度 (暫定)」は schema からの推定で、実際のソース年・ETL ジョブの依存は未確認。

更新頻度の考え方:

- 案件ごと: 顧客 CSV。
- 月次〜四半期: SalesNow、求人 DB 由来の競合・企業情報。
- 年次: 最低賃金、生活コスト proxy、人口系の年次確認。
- 5年周期: 国勢調査、経済センサス、通勤 OD。
- 上流データが変わったら必ず派生再生成: `v2_municipality_target_thickness` と `municipality_recruiting_scores`。

---

## 3. 標準更新フロー

どのデータでも、原則は以下の順序を守る。

1. 元データ取得
2. ローカル CSV / ローカル DB 反映
3. 派生テーブル再生成
4. ローカル検証
5. Turso dry-run
6. Turso upload
7. Turso verify
8. 本番 PDF 再生成
9. PDF grep / PNG / bbox 検証
10. docs 更新

Turso 反映は remote DB の書き換えなので、必ず dry-run → upload → verify の順で実行する。

---

## 4. データ別の再現手順

### 4.1 地域 × 職業 × 性別 × 年齢

対象:

- `municipality_occupation_population`
- `data/generated/estat_15_1_merged.csv`

取得:

```powershell
# .env または安全な保管場所から e-Stat APP_ID を設定する。実値は docs / GitHub に書かない。
$env:ESTAT_APP_ID = "<your-app-id>"
python scripts\fetch_estat_15_1.py --dry-run
python scripts\fetch_estat_15_1.py --metadata-only
python scripts\fetch_estat_15_1.py --sample-only
python scripts\fetch_estat_15_1.py --fetch
python scripts\fetch_estat_15_1.py --merge
python scripts\fetch_estat_15_1.py --validate
```

ローカル投入:

```powershell
python scripts\ingest_estat_15_1_to_local.py --dry-run
python scripts\ingest_estat_15_1_to_local.py --apply
python scripts\ingest_estat_15_1_to_local.py --verify-only
```

Turso:

```powershell
python scripts\upload_phase3_step5.py --dry-run --tables municipality_occupation_population
python scripts\upload_phase3_step5.py --upload --tables municipality_occupation_population --strategy replace --yes
python scripts\upload_phase3_step5.py --verify --tables municipality_occupation_population
```

PDF 確認:

- `対象自治体 × 職業 × 性別 × 年齢`
- 対象自治体ごとの職業 Top 5 が出る。
- `北海道 伊達市` などの誤地大量表示が復活していない。

### 4.2 地域 × 産業 × 性別

対象:

- `scripts/data/industry_structure_by_municipality.csv`
- `v2_external_industry_structure`

取得:

```powershell
python scripts\fetch_industry_structure.py --reset
python scripts\fetch_industry_structure.py
```

ローカル投入:

```powershell
python scripts\ingest_industry_structure_to_local.py --dry-run
python scripts\ingest_industry_structure_to_local.py --apply
python scripts\ingest_industry_structure_to_local.py --verify-only
```

Turso:

`upload_phase3_step5.py` の対象 7 テーブルには `v2_external_industry_structure` が含まれていない。Turso に反映する場合は、既存の外部テーブル upload script の対象定義を確認してから実行する。未確認のまま `upload_phase3_step5.py` に渡すと `unknown table` で止まる。

PDF 確認:

- `対象自治体 × 産業 × 性別`
- `東京都 特別区部` の集約注記。
- `医療，福祉` など Top 産業。
- 男性比 / 女性比 / 採用示唆。

### 4.3 Model F2 thickness

対象:

- `data/generated/v2_municipality_target_thickness.csv`
- `v2_municipality_target_thickness`

生成:

```powershell
python scripts\build_municipality_target_thickness.py --build --csv-only
python scripts\ingest_v2_thickness_to_local.py --apply
```

Round 10 Phase 1B 仕様:

```text
if old_thickness >= 200:
    thickness = 199.001 + within_capped_percentile * 0.999
else:
    thickness = old_thickness
```

検証期待値:

- `top100 overlap = 100%`
- `dropped = 0`
- `entered = 0`
- `score >= 160` の自治体数が極端に崩れない。Round 10 Phase 1B 実績は `7`。
- `thickness=200` の全職業同値 saturation が復活しない。
- `score range OOR = 0`

### 4.4 Recruiting scores

対象:

- `municipality_recruiting_scores`

`v2_municipality_target_thickness` を更新した後は必ず連鎖再生成する。

```powershell
python scripts\build_municipality_recruiting_scores.py --apply
```

Turso:

```powershell
python scripts\upload_phase3_step5.py --dry-run --tables v2_municipality_target_thickness
python scripts\upload_phase3_step5.py --dry-run --tables municipality_recruiting_scores
python scripts\upload_phase3_step5.py --upload --tables v2_municipality_target_thickness municipality_recruiting_scores --strategy replace --yes
python scripts\upload_phase3_step5.py --verify --tables v2_municipality_target_thickness municipality_recruiting_scores
```

PDF 確認:

- `配信地域ランキング`
- `配信優先度スコアは 0〜200`
- `160 以上`
- `スコア160+`
- `スコア80+` が 0 件。

### 4.5 最低賃金

対象:

- `v2_external_minimum_wage`
- `v2_external_minimum_wage_history`

用途:

- 最低賃金セクション。
- 最賃比。
- 給与妥当性。

更新タイミング:

- 原則年1回。10月の都道府県別最低賃金改定後。

反映:

```powershell
python scripts\upload_minimum_wage_history.py --dry-run
python scripts\upload_minimum_wage_history.py
```

注意:

- この script は既存実装上、対象テーブルを作り直す可能性がある。実行前に dry-run と対象テーブルを必ず確認する。
- P2-C の実績では、福島県の DB 値 `1,033` が PDF に出て、旧 hardcode `1,038` が消えることで DB 接続を確認した。詳細は `docs/ROUND8_P2_C_COMPLETION_2026_05_10.md` を参照。

PDF 確認:

- 該当県の新最低賃金が出る。
- 旧値が残らない。
- `最低賃金` セクションと最賃比が消えていない。

### 4.6 生活コスト proxy

対象:

- `municipality_living_cost_proxy`

生成:

```powershell
python scripts\build_municipality_living_cost_proxy.py --dry-run
python scripts\build_municipality_living_cost_proxy.py --apply
python scripts\build_municipality_living_cost_proxy.py --verify
```

Turso:

```powershell
python scripts\upload_living_cost_proxy.py --dry-run
python scripts\upload_living_cost_proxy.py --check-remote
python scripts\upload_living_cost_proxy.py --upload --yes
python scripts\upload_living_cost_proxy.py --verify
```

更新タイミング:

- 年1回。
- 最低賃金、地価、生活費 proxy の上流を変えた時。

### 4.7 通勤流入 / commute

対象:

- `v2_external_commute_od_with_codes`
- `commute_flow_summary`
- `data/generated/commute_flow_summary.csv`

取得・生成:

```powershell
python scripts\fetch_commute_od.py --schema-only
python scripts\build_commute_flow_summary.py --dry-run
python scripts\build_commute_flow_summary.py --csv-only
```

注意:

- 通勤 OD は国勢調査系なので 5年周期が基本。
- Model F2 側の commute 成分は現時点で職業別ではない。`build_municipality_recruiting_scores.py` は `commute_flow_summary` を `occupation_group_code = 'all'` で集計している。職業別化する場合は別設計が必要。

---

## 5. 派生テーブルの依存関係

| 上流更新 | 必ず再生成する派生 | PDF 検証箇所 |
|---|---|---|
| `municipality_occupation_population` | なし | `対象自治体 × 職業 × 性別 × 年齢` |
| `v2_external_industry_structure` | `v2_municipality_target_thickness`, `municipality_recruiting_scores` | `対象自治体 × 産業 × 性別`, `CSV 求人数 × 地域母集団`, `配信地域ランキング` |
| `occupation_industry_weight.csv` | `v2_municipality_target_thickness`, `municipality_recruiting_scores` | `配信地域ランキング`, `4 象限図` |
| SalesNow 集約 | `v2_municipality_target_thickness`。その後 `municipality_recruiting_scores` を再生成 | `配信地域ランキング` |
| `v2_external_minimum_wage` | 必要に応じて `municipality_living_cost_proxy`, `municipality_recruiting_scores` | `最低賃金`, `推奨アクション` |
| `municipality_living_cost_proxy` | `municipality_recruiting_scores` | `4 象限図`, `配信地域ランキング` |
| `commute_flow_summary` | `municipality_recruiting_scores` | `配信地域ランキング` |

---

## 6. PDF 検証プロトコル

DB・コード・データの反映後は、必ず PDF 実物で確認する。

### 6.1 固定手順

1. 古い PDF を削除する。
2. Playwright spec で PDF を新規生成する。
3. file mtime / size / pages を確認する。
4. PyMuPDF でテキスト抽出する。
5. 固有文言を grep する。
6. 図説変更がある場合は PNG 出力または bbox 確認を行う。
7. 本番は Render 反映遅延を考慮し、初回失敗時は旧 PDF / 旧 deploy の可能性を先に疑う。

### 6.2 最低限の grep 項目

| 項目 | 期待 |
|---|---|
| `対象自治体 × 職業` | 1 件以上 |
| `対象自治体 × 産業 × 性別` | 1 件以上 |
| `CSV 求人数 × 地域母集団` | 1 件以上 |
| `推奨アクション` | 1 件以上 |
| `最低賃金` | 1 件以上 |
| `配信地域ランキング` | 1 件以上 |
| `0〜200` | 1 件以上 |
| `160 以上` | 1 件以上 |
| `スコア160+` | 1 件以上 |
| `スコア80+` | 0 件 |

### 6.3 図説変更時の追加確認

4 象限図や chart を変えた場合:

- ページ数が想定範囲内。
- 点ラベルが図内にある。
- 凡例が読める。
- `対数スケール` などの説明が出る。
- PNG で見切れがない。
- chart width が A4 本文域からはみ出していない。

---

## 7. データ境界ルール

このレポートで最も重要なのは「何を言えるか / 言えないか」を分けること。

| データ | 言えること | 言ってはいけないこと |
|---|---|---|
| 顧客 CSV | 求人数、給与中央値、対象自治体、求人分布 | CSV にない業界別給与・職種別給与を断定する |
| 国勢調査 職業 | 地域ごとの職業人口、性別、年齢 | CSV 求人の職種別給与に直接変換する |
| 経済センサス 産業 | 地域ごとの産業従業者、男女比 | 産業 × 年齢の完全クロスを既存資産だけで断定する |
| 最低賃金 | 都道府県別の下限賃金、給与妥当性の基準 | 業界横断の賃金水準比較として断定する |
| Recruiting scores | 配信地域の相対優先度 | 絶対的な採用成功確率として断定する |
| SalesNow | 企業側の補正材料 | CSV 給与の根拠として直接使う |

禁止に近い表現:

- `業界別給与`
- `職種別給与`
- `会社名から職種を推定した給与`
- `法人種別から業界給与を推定`

使うなら必ず `推定`, `参考`, `CSV に分類列がないため断定不可` を明記し、原則として採用示唆の中心に置かない。

---

## 8. 月次・年次運用計画

### 毎回 / 案件ごと

- 顧客 CSV をアップロード。
- PDF を生成。
- 対象自治体、求人数、給与中央値、4 象限図、推奨アクションを確認。

### 月次

- SalesNow / 企業マスタが更新される運用なら集約を更新。
- 求人 DB 由来の competition 指標を使う場合は再計算。
- 必要なら `municipality_recruiting_scores` を再生成。

### 四半期

- SalesNow 集約、企業 geocode、産業 mapping の品質確認。
- 4 象限図や配信ランキングが極端に変わっていないか sample PDF で確認。

### 年次

- 最低賃金改定後に `v2_external_minimum_wage` / history を更新。
- 生活コスト proxy を再生成。
- PDF の最低賃金値、最賃比、推奨アクション文を確認。
- 人口・自治体コード・GeoJSON の更新有無を確認。

### 5年周期

- 国勢調査 表 15-1 を更新。
- 国勢調査 通勤 OD を更新。
- 経済センサス 産業構造を更新。
- 更新後は `v2_municipality_target_thickness` と `municipality_recruiting_scores` を再生成し、PDF 回帰確認を行う。

---

## 9. 更新後の完了条件

更新作業は、以下を満たして完了とする。

1. ローカル DB の投入 verify が PASS。
2. 必要な派生テーブルを再生成済み。
3. Turso `--dry-run` が write budget 内。
4. Turso `--upload` 完了。
5. Turso `--verify` で local_total / remote_total が一致。
6. 本番 PDF を新規生成。
7. PDF テキスト grep が PASS。
8. 図説変更がある場合は PNG / bbox 確認が PASS。
9. 更新内容と検証結果を docs に残す。

---

## 10. 既知の注意点

- `upload_phase3_step5.py` は対象テーブルが固定されている。未知のテーブルを渡すと abort する。
- `v2_external_industry_structure` は `upload_phase3_step5.py` の対象外。投入経路を確認してから Turso 反映する。
- `build_municipality_recruiting_scores.py` は `v2_municipality_target_thickness` 更新後に必ず再実行する。
- `build_municipality_recruiting_scores.py` の commute 成分は `commute_flow_summary WHERE occupation_group_code = 'all'` を読むため、現時点では職業別 commute ではない。
- `build_municipality_recruiting_scores.py` は SalesNow を直接読まない。SalesNow 更新の影響は `v2_municipality_target_thickness` を再生成した場合にだけ score 側へ波及する。
- `page.pdf()` の viewport はデフォルト 1280px の罠がある。PDF 生成 helper は A4 portrait viewport を維持する。
- release build 時に稼働中 `rust_dashboard.exe` があると exe 上書きに失敗する。プロセス停止対象を確認してから build する。
- PowerShell の here-string は `@'` の後に同じ行で文字を書かない。短い確認は `python -c` で 1 行にする方が安全。
