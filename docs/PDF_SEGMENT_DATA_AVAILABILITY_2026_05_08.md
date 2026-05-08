# 既存 DB セグメント分析データ availability (Round 1-F)

**作成日**: 2026-05-08
**対象 DB**: `data/hellowork.db` (ローカル, read-only 確認)
**目的**: PDF レポートのセグメント分析セクション拡充に向けて、既存 DB だけで作れる分析を棚卸しし、追加データ取得を待たずに着手できる範囲を明確化する。
**確認方法**: SELECT / PRAGMA のみ。書込・スキーマ変更なし。

---

## 1. テーブル別概要

| テーブル | 行数 | 主要列 | カバレッジ | 品質リスク |
|---|---:|---|---|---|
| `municipality_occupation_population` | 729,949 | municipality_code, prefecture, occupation_code, occupation_name, age_class (18種), gender (3種), population, estimate_index, basis, data_label, source_year=2020 | 47県, 1,896市区町村, occ 22 codes (大分類A-K=11 + 数値コード01-11=11) | population NULL=20,845 (数値コード行は population 無し)、estimate_index NULL=709,104 (アルファベット行は index 無し)。**2系統が同居**。`age_class='_total'` と各年齢階級が混在 ⇒ 集計時に二重計上注意 |
| `municipality_code_master` | 1,917 | municipality_code, prefecture, municipality_name, pref_code, area_type, area_level, is_special_ward, is_designated_ward, parent_code | 47県 / 1,917 muni (政令市分割含むマスタ) | NULL リスク低。joinキーとして堅牢 |
| `municipality_geocode` | 2,626 | id, prefecture, municipality, latitude, longitude | – | コード列なし（名称結合のみ）⇒ join 揺れリスク |
| `municipality_living_cost_proxy` | 1,917 | municipality_code, cost_index, min_wage, land_price_proxy, salary_real_terms_proxy | 1,917 muni | 推定値 (estimated_beta)。proxy は単独利用可 |
| `municipality_recruiting_scores` | 20,845 | muni × occupation_code (01_管理〜11_運輸郵便) のスコア群 | 1,895 muni × 11 occ | スコアは推定値。ranking のみで絶対値解釈は不可 |
| `v2_municipality_target_thickness` | 20,845 | muni × occupation_code, thickness_index, scenario_*, is_industrial_anchor | 1,895 muni × 11 occ | 同上 |
| `v2_external_population` | 1,917 | total/male/female_population, age_0_14 / 15_64 / 65_over, aging_rate ほか | 1,917 muni | 1スナップショット (reference_date 単一系) |
| `v2_external_population_pyramid` | 17,235 | age_group (0-9, 10-19, …, 80+ の9区分), male_count, female_count | 1,915 muni | 9 区分の粗さ。`municipality_occupation_population` の 18 区分とは粒度不一致 |
| `v2_external_daytime_population` | 1,740 | nighttime_pop, daytime_pop, day_night_ratio, inflow/outflow | 1,740 muni（90% 程度） | 一部市区町村欠損 |
| `v2_external_foreign_residents` | 1,742 | total_foreign, foreign_rate | 1,742 muni | 同上 |
| `v2_external_migration` | 1,741 | inflow, outflow, net_migration, net_migration_rate | 1,741 muni | 同上 |
| `v2_external_minimum_wage` | 47 | hourly_min_wage, fiscal_year | 47 県 | 県粒度のみ |
| `v2_external_prefecture_stats` | 47 | unemployment_rate, job_change_desire_rate, non_regular_rate, avg_monthly_wage, price_index, fulfillment_rate, real_wage_index | 47 県 | 県粒度のみ |
| `v2_external_job_opening_ratio` | 47 | job_opening_ratio (有効求人倍率) | 47 県 | 県粒度のみ・1スナップショット |
| `v2_external_commute_od` / `_with_codes` | 86,762 | origin × dest 通勤フロー (total / male / female) | 県・市区町村粒度 | 国勢調査ベース。職種なし |
| `commute_flow_summary` | 27,879 | OD × occupation_group_code | 全件 occ='all' のみ (1種) | **職種別 OD は実質ない** |
| `v2_commute_flow_summary` | 3,786 | muni × direction (inflow/outflow) サマリー (top10_json) | 1,917 muni 想定 | JSON で持つため SQL 加工は重い |
| `postings` | 469,027 | job_type (13), industry_raw (524), prefecture (47), municipality (1,696), employment_type (7), salary_min/max, occupation_major (14), job_category_name (16), age_min/max ほか | 全国 1,696 muni | **求職者ではなく求人**。industry_raw 524 種は粒度バラツキあり。`job_category_major` (430種) は 297,724 行が空文字。文字化け列名あり (cp932 列名) — 集計時は select 抽出に注意 |
| `layer_a_*`, `layer_b_*`, `layer_c_*`, `v2_anomaly_*`, `v2_*_score` 等 | – | 既存指標 | 各 muni / occupation 粒度 | 推定値・既存パイプ依存 |

---

## 2. 分析別作成可能性

凡例: ✅ 作れる / ⚠ 加工要 / ❌ データ不足

### 2-1. 単軸セグメント

| 分析 | 判定 | 入力テーブル | 必要 SQL (概要) | 注意点 |
|---|---|---|---|---|
| **職種別人口** (大分類11) | ✅ | `municipality_occupation_population` | `SELECT occupation_code, occupation_name, SUM(population) FROM mop WHERE basis='measured' AND age_class='_total' AND gender='total' AND occupation_code GLOB '[A-K]' GROUP BY 1,2` | A-K の measured 行を使う。数値コード01-11 は estimate ベース |
| **年齢階級別人口** (18 区分 = 5歳階級) | ✅ | `municipality_occupation_population` | `SELECT age_class, SUM(population) WHERE age_class!='_total' AND occupation_code GLOB '[A-K]' AND gender='total' AND basis='measured' GROUP BY 1` | `_total` 行除外必須 |
| **年齢階級別人口** (9 区分 = 10歳階級) | ✅ | `v2_external_population_pyramid` | `SELECT age_group, SUM(male_count+female_count) GROUP BY 1` | 5歳階級が必要なら mop を使う |
| **性別人口** | ✅ | `v2_external_population` または mop | `SELECT SUM(male_population), SUM(female_population) FROM v2_external_population` | – |
| **市区町村別人口** | ✅ | `v2_external_population` (合計) または `municipality_code_master` ベース集計 | `SELECT prefecture, municipality, total_population FROM v2_external_population` | – |
| **業界別求人数** (industry_raw 524種) | ✅ | `postings` | `SELECT industry_raw, COUNT(*) FROM postings GROUP BY 1 ORDER BY 2 DESC` | 524 種は粒度バラバラ。Top N + その他 推奨 |
| **業界別求人数** (job_category_name 16種に集約) | ✅ | `postings` | `SELECT job_category_name, COUNT(*) GROUP BY 1` | 16 種なので PDF 1ページに収まる |
| **業界別企業数** | ⚠ | `postings` | facility_name で distinct。ただし企業=施設 ではない | facility_name は事業所単位。法人レベル集計は不可 |

### 2-2. クロス集計

| 分析 | 判定 | 入力テーブル | 必要 SQL (概要) | 注意点 |
|---|---|---|---|---|
| **職種 × 地域 (求職者人口側)** | ✅ | `municipality_occupation_population` | `SELECT prefecture, occupation_code, SUM(population) WHERE basis='measured' AND age_class='_total' AND gender='total' AND occupation_code GLOB '[A-K]' GROUP BY 1,2` | 県粒度なら確実。muni 粒度は粗い職種のみ |
| **職種 × 地域 (求人側)** | ✅ | `postings` | `SELECT prefecture, occupation_major, COUNT(*) GROUP BY 1,2` | occ 14 種で県 47 = 658 セル |
| **職種 × 性別** | ✅ | `municipality_occupation_population` | `SELECT occupation_code, gender, SUM(population) WHERE age_class='_total' AND basis='measured' AND occupation_code GLOB '[A-K]' GROUP BY 1,2` | total/male/female の 3 区分 |
| **職種 × 年齢** | ✅ | `municipality_occupation_population` | `SELECT occupation_code, age_class, SUM(population) WHERE gender='total' AND basis='measured' AND age_class!='_total' AND occupation_code GLOB '[A-K]' GROUP BY 1,2` | 11 occ × 17 age = 187 セル |
| **業界 × 地域 (求人)** | ✅ | `postings` | `SELECT prefecture, job_category_name, COUNT(*) GROUP BY 1,2` | industry_raw 524種だと粒度過多 ⇒ job_category_name (16) を推奨 |
| **業界 × 職種** | ⚠ | `postings` | `SELECT industry_raw, occupation_major, COUNT(*) GROUP BY 1,2` | 求人側のみ。求職者側 (population) には industry が無いので「求職者業界別」は不可 |
| **性別 × 年齢** (mop, 5歳階級) | ✅ | `municipality_occupation_population` | `SELECT age_class, gender, SUM(population) WHERE basis='measured' AND age_class!='_total' AND occupation_code GLOB '[A-K]' GROUP BY 1,2` | 17 age × 2 gender |
| **性別 × 年齢** (pyramid, 10歳階級) | ✅ | `v2_external_population_pyramid` | `SELECT age_group, SUM(male_count), SUM(female_count) GROUP BY 1` | 9 階級 |
| **職種 × 年齢 × 性別** | ✅ | `municipality_occupation_population` | mop は本来この5次元 (muni×occ×age×gender×basis) | 11 × 17 × 2 = 374 セル |
| **職種別 × 通勤OD** | ❌ | `commute_flow_summary` | occupation_group_code は 'all' 1種のみ | データ不足 |
| **業界 × 通勤OD** | ❌ | – | 通勤 OD に業界軸がない | データ不足 |
| **業界別 × 求職者** | ❌ | – | mop に industry 軸がない | データ不足 (求職者の業界経験は調査外) |
| **求職者 × 求人マッチ (職種粒度)** | ⚠ | `mop` × `postings` | 職種マッチ用に occ コード対応表が必要 (mop は A-K / 01-11、postings は occupation_major 14種) | **コードマッピングが要設計**。既存 `v2_municipality_target_thickness` が 11 occ で使っているので、それを正解として参照可 |
| **求職者 × 求人 (地域)** | ✅ | `mop` + `postings` | muni 粒度で人口 vs 求人数比 | – |
| **求職者 × 求人 (年齢)** | ⚠ | `mop` + `postings.age_min/age_max` | postings 側の年齢は範囲表記 (NULL 比率未計測) | postings 側 NULL 確認要 |
| **業界 × 雇用形態** | ✅ | `postings` | `SELECT industry_raw, employment_type, COUNT(*)` | – |
| **業界 × 給与レンジ** | ✅ | `postings` | salary_min/max は NULL 0 ⇒ 全件使える | salary_type による正規化要 (時給/月給混在) |

### 2-3. 既存スコア活用

| 分析 | 判定 | 入力テーブル | 用途 |
|---|---|---|---|
| 市区町村 × 職種 採用優先度ランキング | ✅ | `municipality_recruiting_scores` (20,845行) | そのまま使える |
| 市区町村 × 職種 thickness | ✅ | `v2_municipality_target_thickness` | 同上 |
| 県別 求人倍率・失業率・賃金 | ✅ | `v2_external_prefecture_stats` + `v2_external_job_opening_ratio` + `v2_external_minimum_wage` | 47 県スナップショット |

---

## 3. 追加データなしで作れる P1 セクション候補 (Top 3)

### 推奨 1. 求職者人口プロファイル (職種 × 年齢 × 性別 の3次元クロス)

- **入力**: `municipality_occupation_population` 単独
- **理由**:
  - 約 70 万行に職種(11) × 年齢(17) × 性別(3) のフルクロスが既にある
  - `basis='measured'` の measured 行で2020年センサスベース、`estimated_beta` で推定値も区別可能
  - 県粒度では欠損ほぼゼロで 47/47 県カバー
- **PDF 想定アウトプット**:
  - 職種別人口バー (Top11)
  - 年齢ピラミッド (5歳階級)
  - 職種 × 年齢ヒートマップ (11 × 17)
  - 性別比 (職種別)
- **注意**: `age_class='_total'` 行と各階級行が同居しているので集計時に **必ず除外**。`occupation_code` は A-K と 01-11 が併存するため **片方に絞る** (推奨: A-K 大分類)。

### 推奨 2. 求人市場プロファイル (業界 × 地域 × 雇用形態)

- **入力**: `postings` 単独
- **理由**:
  - 469,027 行の 全国求人スナップショット
  - `industry_raw` (524) を `job_category_name` (16) に集約すれば PDF 表示に収まる
  - `employment_type` 7区分は全件埋まっている
  - salary_min/max が NULL ゼロで給与分析も可能
- **PDF 想定アウトプット**:
  - 業界別求人数 Top16 (job_category_name)
  - 都道府県 × 業界ヒートマップ (47 × 16)
  - 雇用形態構成比 (業界別)
  - 給与レンジ箱ひげ (業界別、salary_type 正規化後)
- **注意**: `job_category_major` の 297,724行 (約63%) が空文字。`industry_raw` を直接使うか集約コード `job_category_name` を使う方が安全。文字化けカラム (`has_*` 系) は select で除外。

### 推奨 3. 求職者-求人ギャップ (職種粒度・市区町村別)

- **入力**: `municipality_occupation_population` × `postings` × `municipality_code_master`
- **理由**:
  - 求職者人口 (mop の 11 occ) と求人数 (postings の 14 occ) を職種粒度でマッピング可能
  - `v2_municipality_target_thickness` (1,895 muni × 11 occ) が既存の正解として使えるので、そこに postings 側の求人数を join するだけで「需要 vs 供給」軸が揃う
  - 市区町村粒度のため地域戦略レポートに直結
- **PDF 想定アウトプット**:
  - 職種別「人口/求人」比のランキング (muni 単位)
  - 求人過剰 / 不足 muni Top20
  - 都道府県別ギャップ集計
- **注意**: occ コード対応表 (mop 11種 ↔ postings 14種) を **先に確定** する必要あり。既存 V2 パイプの分類ロジックを参照する。muni 名揺れ対策で必ず `municipality_code` で結合 (postings 側はコードを持たないので muni_name + prefecture でマッチング、mismatch 件数を計測すること)。

---

## 4. 不可なクロス (データ不足リスト)

| 分析 | 不足理由 |
|---|---|
| 職種別 × 通勤OD | `commute_flow_summary.occupation_group_code` は `'all'` 1種のみ |
| 業界 × 通勤OD | 通勤 OD (`v2_external_commute_od` / `commute_flow_summary`) に業界軸が無い |
| 業界別 求職者人口 | 求職者人口テーブル (mop, pyramid) に industry 軸が無い ⇒ 求職者の業界経験データは DB 内に存在しない |
| 業界 × 年齢 (求職者側) | 同上 |
| 業界 × 性別 (求職者側) | 同上 |
| 法人レベル集計 | postings.facility_name は事業所単位で法人グループ統合不可 (SalesNow 等の外部結合が必要だが本DB外) |
| 時系列推移 (mop) | source_year=2020 単一スナップショット |
| 時系列推移 (postings) | created_date は持つが、複数年分の継続採取データではないため snapshot 扱い |
| 5歳階級 × pyramid | pyramid は 10 歳階級。5 歳階級は mop のみ取得可能 |

---

## 5. データ品質上の主要リスク (横断)

1. **occupation_code 二重体系**: mop は A-K と 01-11 が同居。集計クエリでは `WHERE occupation_code GLOB '[A-K]'` または `LIKE '__\_%'` で **片方に絞る**こと。両方足すと population が二重計上される。
2. **age_class='_total' 行の混在**: mop は各5歳階級 + `_total` を保持。階級別集計時は `age_class != '_total'`、合計利用時は `age_class = '_total'` を明示。
3. **measured / estimated_beta の分岐**: `basis` 列で区別。混在集計は意味なし。レポートには出典区分を必ず明記。
4. **postings の文字化けカラム名**: cp932 で書かれた `has_*` 系列がある。SQL では `[has_xxx]` 形式の引用が必要 / または `SELECT * FROM postings` を避け必要列のみ指定。
5. **muni 名揺れ**: postings は `prefecture` + `municipality` の名称ベース。同名市区町村 (伊達市・府中市等) の判別に code_master との結合 (pref + name) が必要。
6. **occupation_code 対応表**: mop の 11 occ ↔ postings の 14 occ_major ↔ vmt の 11 occ_code (01_管理...) の **3系統対応表**を先に整備しないと「求職者×求人」分析がブレる。
7. **NULL 計測未実施列** (本回未確認): `postings.age_min/age_max`, `postings.salary_type`, `postings.bonus_*`, `postings.holiday_text` 等は NULL 比率を別途確認してから採用判断すること。

---

## 6. まとめ (作成可能性の数値サマリー)

- 確認テーブル数: **19** (主要候補)
- 分析別作成可能性 (単軸+クロス+既存スコア = 計 **22** 分析):
  - ✅ 作れる: **17**
  - ⚠ 加工要 / 設計要: **5**
  - ❌ データ不足: 4 (本表外、項4にリスト)
- 追加データなしで着手可能な P1 候補: **3 セクション** (職種×年齢×性別 / 業界×地域×雇用形態 / 求職者-求人ギャップ)
- 着手前に必須の整備: occ コード対応表 (mop 11 ↔ postings 14 ↔ vmt 11)、muni 名コード変換テーブルの拡張 (postings 側はコード無し)
