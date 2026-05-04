# Phase 3 Step 5: `municipality_occupation_population` 推定モデル再設計 (v2)

作成日: 2026-05-04
最終更新: 2026-05-04 (修正版、レビュー指摘反映)
対象: Worker C 再設計 — 単純按分 (案 A) を不採用、市区町村差を出す多重補正モデル

**ステータス: 設計提示 (実装未着手、レビュー修正反映済)**

## 0. 修正版の主な変更点 (2026-05-04 レビュー反映)

| 修正項目 | 内容 |
|---------|------|
| 1. 精度表現を弱める | 絶対誤差 (±X%) を対外表示から除外、`estimate_grade` (A/A-/B/C/D/X) で運用 |
| 2. SalesNow 参照修正 | 「別 Turso DB」を撤回。Turso V2 内 `v2_salesnow_companies` を採用 (Phase 3 初期正本) |
| 3. F3/F6 重複リスク明記 | F6 単独使用禁止。F3 の残差補正のみ (alpha=0.25)。F3 不在なら F6 も無効化 |
| 4. baseline 式の明確化 | 年齢性別単位で都道府県人口比により按分。F1+F2 を baseline に統合、独立補正項から除外 |

---

## 0. 経緯

初版 docs `SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_FEASIBILITY.md` の **案 A (都道府県職業比 × 市区町村人口比 単純按分)** はユーザーレビューで **不採用**:

> 都道府県職業比を市区町村人口で単純按分する案 A は、市区町村差を潰しすぎるため、そのまま採用は不可。

理由 (具体例):
- 製造業が密集する太田市と観光地の伊香保温泉 (群馬県内) で職業構成が大きく異なるが、案 A では群馬県平均が両方に適用される
- 政令市の中心区 (商業/サービス業集中) と郊外区 (住宅地、ホワイトカラー) でも案 A は同じ職業比

→ **多重補正モデル (案 D)** で再設計する。

---

## 1. 必須要件 (ユーザー指示)

| # | 要件 | 対応方針 |
|--:|------|---------|
| 1 | 市区町村ごとの差が出ること | 補正項の積で差を生成 |
| 2 | 昼夜間人口比、通勤 OD、産業/事業所/SalesNow、年齢性別人口を補正項に使うこと | 補正項 5 種を §3 で定義 |
| 3 | 推定ラベル明示 | DTO `source_name = 'estimate_level_X'` で版判別 |
| 4 | 保守/標準/強気の母集団レンジに接続できること | METRICS.md §9 の式に推定 muni_occ_pop を入力 |
| 5 | 欠損時に段階的フォールバック | Level 1〜5 (§5) |
| 6 | 実装可能なテーブル設計と式 | §4 (式) + §6 (DDL 拡張案) |

---

## 2. 使う既存テーブル一覧

### 2.1 ローカル `data/hellowork.db` で利用可

| テーブル | 行数 | 用途 |
|---------|----:|------|
| `v2_external_population` | 1,742 | 市区町村別総人口、年齢区分 (0-14/15-64/65+) |
| `v2_external_population_pyramid` | 15,660 | 市区町村×年齢階級×性別人口 (10 歳刻み) |
| `v2_external_daytime_population` | 1,740 | **昼夜間人口比 + 流入/流出 (重要)** |
| `v2_external_migration` | 1,741 | 転入転出純移動 |
| `v2_external_commute_od_with_codes` | 86,762 | **JIS 5 桁 OD (重要)** |
| `v2_external_prefecture_stats` | 47 | 都道府県別総合統計 (失業率・賃金・物価等) |
| `municipality_code_master` | 1,917 | JIS マスタ + area_type |
| `commute_flow_summary` | 27,879 | 通勤要約 (TOP 20 流入元、JIS) |

### 2.2 Turso V2 にあり、ローカル不在 (要 download or Turso 直接 SELECT)

| テーブル | 用途 |
|---------|------|
| `v2_external_industry_structure` | **市区町村×産業大分類 (重要、補正項 F3)** |
| `v2_external_establishments` | 都道府県×産業×事業所数 |
| `v2_external_household_spending` | 都市×家計支出 |
| `v2_external_labor_stats` | 都道府県×労働統計 |
| `v2_salesnow_companies` | 企業データ (**Turso V2 `country-statistics` 内、198K 社**。前回セッションで Phase 3 初期の正本として採用済) |

**重要**: SalesNow データは「別 Turso DB」ではなく **Turso V2 内 `v2_salesnow_companies` (`country-statistics-makimaki1006...`)** を使う。
SalesNow 専用 Turso (`salesnow-makimaki1006...`) は `employee_range` カラムに異常値が混入しているため Phase 3 では非採用。
詳細: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_SALESNOW_SOURCE.md` (commit `bf91272`)

### 2.3 必要な追加テーブル (新規取得)

| テーブル | 取得元 | 取得時間 |
|---------|--------|---------|
| `prefectural_occupation_population` | e-Stat 国勢調査 R2 (都道府県×職業×年齢×性別) | 0.5〜1 日 |
| `occupation_industry_weight` | 国勢調査の産業大分類×職業大分類クロス表 (都道府県全国) | 0.5 日 |

---

## 3. 補正項の定義 (案 D 多重補正、改訂版)

### 3.1 baseline と補正項の役割分離

**重要な再整理**: F1 (人口比) と F2 (年齢性別) は **baseline 計算に組み込み済み** で、
独立した補正項として再適用しない (二重カウント防止)。
補正項は F3/F4/F5/F6 のみで、それぞれ baseline からの **乗算的逸脱** を表現する。

| # | 補正項 | データソース | 効く対象 | basis | 役割 |
|--:|-------|------------|---------|:------:|------|
| baseline | 市区町村×年齢×性別人口比 (F1+F2 統合) | `v2_external_population_pyramid` | 全職業 | both | 都道府県職業人口を年齢性別単位で按分 |
| **F3** | 産業構成 | `v2_external_industry_structure` (Turso V2) | 職業大分類 | both | 製造業多→生産工程従事者多 等 |
| **F4** | 昼夜間人口比 | `v2_external_daytime_population` | 全職業 | workplace のみ | オフィス街→従業地人口大 |
| **F5** | 通勤 OD | `v2_external_commute_od_with_codes` | 流入元別職業 | workplace のみ | 流入元自治体の職業構成を取り込む |
| **F6** | SalesNow 業種企業数 | `v2_salesnow_companies` (Turso V2 内) | 職業 (残差補正のみ) | workplace のみ | F3 の残差をさらに精緻化 (単独使用不可) |

### 3.2 F3 と F6 の重複リスクと役割分担

**F3 と F6 は同種の情報** (産業 → 職業の写像) を持つため、二重補正のリスクがある:
- F3: 経済センサス (`v2_external_industry_structure`) の **従業者数ベース**
- F6: SalesNow (`v2_salesnow_companies`) の **企業数ベース**

これを踏まえた運用ルール:
- **F6 単独使用は禁止**。F3 が利用不可な市区町村では F6 も使わず、L3 (no_industry) にフォールバック
- F6 は F3 が利用可能な時のみ、「F3 の残差補正」として軽い重み (例: 0.2〜0.3) で乗算
- 残差補正の意味: 同じ産業内でも「企業の規模・新興度」で職業比が異なる (例: IT 大手 vs IT スタートアップ) を捉える

```
F6_effective[muni][occ] = 1 + alpha × (F6_raw[muni][occ] - F3_raw[muni][occ])
   alpha = 0.25 (F3 への補正強度、固定値)
```

この設計により F6 は F3 と排他ではなく、**F3 の前提下での微調整**になる。

### 3.3 basis (常住地/従業地) の使い分け

- `basis = 'resident'`: 居住地ベース。**baseline + F3** のみ (F4/F5/F6 は workplace 専用)
- `basis = 'workplace'`: 従業地ベース。**baseline + F3 + F4 + F5 (+F6 if F3 利用可)**

---

## 4. 推定式 (再正規化付き、改訂版)

### 4.1 ベースライン (明確化)

#### 入力データ

| 変数 | ソーステーブル + 集計 |
|------|--------------------|
| `pref_occ_pop[pref, occ, age, gender, basis]` | `prefectural_occupation_population` (新規、e-Stat 国勢調査就業状態等基本集計) |
| `muni_pyramid[muni, age, gender]` | `v2_external_population_pyramid` の `male_count` または `female_count` (gender = 'male'/'female') または合算 (gender = 'total') |
| `pref_pyramid[pref, age, gender]` | `Σ_muni muni_pyramid[muni, age, gender]` グループ化集計 (都道府県内の市区町村合計) |

#### baseline 式 (年齢性別単位で独立按分)

```
baseline[muni, occ, age, gender] =
    pref_occ_pop[pref(muni), occ, age, gender]              -- 都道府県職業×年齢×性別人口
  × (muni_pyramid[muni, age, gender] / pref_pyramid[pref(muni), age, gender])  -- 年齢性別単位の市区町村比
```

#### 重要な性質

- **年齢性別ごとに独立按分**: gender='male'/age='30-39' の比率は男性 30 代の人口比のみで決まる (女性比や全年齢平均で薄まらない)
- **F1 (人口比) と F2 (年齢性別) は baseline に統合**: 別途補正項として再適用しない
- **都道府県集計の自動整合性**: `Σ_muni baseline[muni, occ, age, gender] = pref_occ_pop[pref, occ, age, gender]` が定義上成立 (人口比の合計は 1.0)

#### 例

東京都の 30 代男性で「専門的・技術的職業」が 100,000 人とする (`pref_occ_pop`)。
新宿区の 30 代男性人口が東京都全体の 30 代男性人口の 5% (`muni_pyramid / pref_pyramid = 0.05`) なら:

```
baseline[新宿区, 専門技術, 30-39, male] = 100,000 × 0.05 = 5,000 人
```

これがベースライン (補正前)。F3〜F6 で新宿区の特性 (オフィス街・IT 集積) を加味して上方/下方修正する。

### 4.2 補正項 (raw factor)

#### F3: 産業構成 (resident/workplace 両方)

```
raw_industry[muni][occ] =
    Σ_industry (
        muni_industry_employees[industry]
      × occupation_industry_weight[occ][industry]
      / pref_industry_employees[industry]
    )
```

- `occupation_industry_weight[occ][industry]`: 職業大分類が産業大分類に占める割合 (国勢調査クロス表より)
- 例: 「専門技術者」職業は「医療福祉」「学術研究」産業に多く分布 → 医療福祉産業が多い市区町村は専門技術者が多い

#### F4: 昼夜間人口比 (workplace のみ)

```
raw_daytime[muni] = daytime_pop[muni] / nighttime_pop[muni]
```

オフィス街 (昼夜比 > 1.5) では従業地人口が大きく、住宅地 (昼夜比 < 0.8) では小さい。

#### F5: 通勤 OD (workplace のみ)

```
inflow_workplace[muni][occ] =
    Σ_origin (
        origin_resident_in_occ[origin][occ]
      × commute_share[origin → muni]
    )
```

- `commute_share[origin → muni] = total_commuters[origin → muni] / Σ_dest total_commuters[origin → *]`

#### F6: SalesNow 業種企業数 (workplace のみ、オプション)

```
raw_company[muni][occ] =
    Σ_industry (
        muni_company_count[industry]
      × occupation_industry_weight[occ][industry]
      / pref_company_count[industry]
    )
```

(F3 と類似だが、事業所/企業数ベース、F3 は従業者数ベースで二重独立補正)

### 4.3 補正項の合成 (再正規化、改訂版)

baseline (F1+F2 統合済) に **F3/F4/F5/F6 のみ** を乗算:

```
combined_factor[muni, occ, basis] =
    F3[muni, occ]                                            # 利用可なら、不可なら 1.0
  × (F4[muni] if basis='workplace' else 1.0)
  × (F5[muni, occ] if basis='workplace' else 1.0)
  × (F6_effective[muni, occ] if basis='workplace' AND F3 利用可 else 1.0)
```

注意: F6 は F3 が利用可能なときのみ適用 (§3.2 重複リスク回避)。

```
# scaling: 都道府県集計が公式値と一致するよう正規化
raw[muni, occ, age, gender, basis] = baseline × combined_factor[muni, occ, basis]

scaling[pref, occ, age, gender, basis] =
    pref_occ_pop[pref, occ, age, gender]
    / Σ_muni_in_pref raw[muni, occ, age, gender, basis]

# 最終推定値
muni_occ_pop[muni, occ, age, gender, basis] = raw × scaling
```

→ **整合性保証**: `Σ_muni_in_pref muni_occ_pop = pref_occ_pop` (公式都道府県値) が必ず成立。

→ 補正項は市区町村「相対」差を生み、scaling で都道府県絶対値を保つ。

---

## 5. フォールバック階層 (estimate_grade)

### 5.1 階層定義 (改訂版: grade ベース)

絶対誤差 (±X%) は **検証前の仮レンジ表示にとどめ**、レポート上では `estimate_grade` (A/A-/B/C/D/X) で扱う。
精度の絶対的な保証ではなく、**「補正項の充実度」を表す相対的な信頼度ラベル** として運用。

| grade | 利用可能補正項 (baseline + F3〜F6) | `precision_level` | 信頼度 | 仮レンジ (検証前、参考値) |
|:-----:|----------------------------------|-------------------|:----:|:-------------------:|
| **A** | F3 + F4 + F5 + F6 すべて利用可 | `estimate_level_1_full` | 高 | 仮レンジ: 概ね数%程度の差を表現可能 (要実測検証) |
| **A-** | F3 + F4 + F5 (F6 不可、または F3 残差補正なし) | `estimate_level_2_no_salesnow_residual` | 中-高 | 仮レンジ: A と同程度〜やや低下 |
| **B** | F4 + F5 (F3 産業構成 不可) | `estimate_level_3_no_industry` | 中 | 仮レンジ: 都道府県差は明確、地域内差は限定的 |
| **C** | F4 のみ (F5 OD 不可) | `estimate_level_4_no_od` | 低-中 | 仮レンジ: 昼夜間効果のみ、職業差は薄い |
| **D** | baseline のみ (F3〜F6 すべて不可) | `estimate_level_5_baseline_only` | 低 (参考扱い) | 仮レンジ: 都道府県差のみ、市区町村差は微小 |
| **X** | データ不在 | `estimate_unavailable` | 表示不可 | - |

### 5.2 仮レンジについての注意 (重要)

- 上記の「仮レンジ」は **実測検証前の理論値**。実際の精度は地域・職業によって変動する。
- 数値レンジ (例: ±X%) を **対外レポートに記載してはならない**。代わりに `estimate_grade` を表示。
- 実装後の **検証フェーズ** で、都道府県集計値との突合 + 主要市区町村のサンプルチェックを行い、信頼度ラベルの根拠を更新する。

### 5.3 フォールバック判定ロジック (改訂版)

```python
def determine_grade(muni_code: str, available_tables: set) -> tuple[str, str]:
    """戻り値: (estimate_grade, precision_level)"""
    has_industry = 'v2_external_industry_structure' in available_tables and \
                   exists_in_table('v2_external_industry_structure', muni_code)
    has_daytime = 'v2_external_daytime_population' in available_tables and \
                  exists_in_table('v2_external_daytime_population', muni_code)
    has_od = exists_in_commute_od(muni_code)
    has_salesnow = 'v2_salesnow_companies' in available_tables and \
                   has_companies_in_muni(muni_code)
    has_pyramid = exists_in_table('v2_external_population_pyramid', muni_code)
    has_pop = exists_in_table('v2_external_population', muni_code)

    # baseline 必須 (人口 + pyramid)
    if not has_pop or not has_pyramid:
        return ('X', 'estimate_unavailable')

    # F3 (industry) は重要だが必須ではない (代わりに F4/F5 で C にフォールバック可能)
    # F6 単独使用は禁止 (§3.2)、F3 が無いと F6 も無効化
    if has_industry and has_daytime and has_od and has_salesnow:
        return ('A',  'estimate_level_1_full')
    if has_industry and has_daytime and has_od:
        return ('A-', 'estimate_level_2_no_salesnow_residual')
    if has_daytime and has_od:
        return ('B',  'estimate_level_3_no_industry')
    if has_daytime:
        return ('C',  'estimate_level_4_no_od')
    return ('D', 'estimate_level_5_baseline_only')
```

### 5.4 想定 grade 分布 (検証前の仮目安)

レビュー時点では分布見込みは「全テーブル投入後の推定」のみ。実投入後に再確認:

| grade | 想定割合 (仮、検証前) | 想定原因 |
|:-----:|--------------------:|---------|
| A | 半数程度 | SalesNow + industry + daytime + OD すべて利用可 |
| A- | 一定数 | SalesNow データに該当 muni の企業がない場合 |
| B | 少数 | industry_structure が当該 muni で不在 |
| C | 稀 | 通勤 OD が疎な地域 |
| D / X | ごく少数 | データ抜けの抜け漏れ |

→ 想定割合は実投入後の検証で更新する (現時点では参考値)。

---

## 6. テーブル設計拡張案

### 6.1 既存 DTO へのカラム追加

```sql
ALTER TABLE municipality_occupation_population
  ADD COLUMN estimate_grade TEXT NOT NULL DEFAULT 'D' CHECK (estimate_grade IN ('A', 'A-', 'B', 'C', 'D', 'X'));
ALTER TABLE municipality_occupation_population
  ADD COLUMN precision_level TEXT NOT NULL DEFAULT 'estimate_level_5_baseline_only';
ALTER TABLE municipality_occupation_population
  ADD COLUMN correction_factors_json TEXT;
-- 例: '{"F3": 1.18, "F4": 0.92, "F5": 1.04, "F6_effective": 1.02, "scaling": 1.001, "baseline": 5000}'

CREATE INDEX IF NOT EXISTS idx_mop_estimate_grade
  ON municipality_occupation_population (estimate_grade);
CREATE INDEX IF NOT EXISTS idx_mop_precision_level
  ON municipality_occupation_population (precision_level);
```

`source_name` は既存 `'census'` から `'estimate'` 系に変更:
- `estimate_level_1_full` (grade A)
- `estimate_level_2_no_salesnow_residual` (A-)
- `estimate_level_3_no_industry` (B)
- `estimate_level_4_no_od` (C)
- `estimate_level_5_baseline_only` (D)
- `estimate_unavailable` (X)

### 6.2 補助マスタ: `occupation_industry_weight`

新規テーブル (国勢調査クロス表より派生):

```sql
CREATE TABLE IF NOT EXISTS occupation_industry_weight (
    occupation_code TEXT NOT NULL,
    industry_code TEXT NOT NULL,
    weight REAL NOT NULL,    -- 0.0〜1.0、industry 別の職業比率
    source TEXT NOT NULL DEFAULT 'estat_census_r2',
    source_year INTEGER NOT NULL DEFAULT 2020,
    PRIMARY KEY (occupation_code, industry_code)
);
```

- 行数想定: 21 職業 × 21 産業 = 441 行
- 国勢調査の「就業者の産業×職業」クロス表 (都道府県集計) から派生

### 6.3 中間テーブル: `prefectural_occupation_population`

新規テーブル:

```sql
CREATE TABLE IF NOT EXISTS prefectural_occupation_population (
    prefecture TEXT NOT NULL,
    pref_code TEXT NOT NULL,
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    age_group TEXT NOT NULL,
    gender TEXT NOT NULL,
    population INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL DEFAULT 2020,
    PRIMARY KEY (pref_code, occupation_code, age_group, gender, source_year)
);
```

- 行数想定: 47 県 × 21 職業 × 8 年齢 × 3 性別 ≈ 23,688 行
- e-Stat 国勢調査就業状態等基本集計から fetch

---

## 7. 実装ステップ (再設計版)

| # | ステップ | 工数 | 依存 |
|--:|---------|:---:|------|
| 1 | e-Stat statsDataId 確定 (都道府県×職業×年齢×性別) | 0.5 日 | - |
| 2 | `fetch_prefectural_occupation.py` 新規実装 + 投入 | 1 日 | 1 |
| 3 | e-Stat 産業×職業クロス表 fetch + `occupation_industry_weight` 構築 | 0.5 日 | - |
| 4 | `v2_external_industry_structure` を Turso からローカルに download (or Turso 経由 SELECT 設計) | 0.5 日 | - |
| 5 | `v2_salesnow_companies` 同上 (任意、Level 1 用) | 0.5 日 | - |
| 6 | `build_municipality_occupation_population.py` 新規実装 (案 D 多重補正) | 1.5 日 | 1〜5 |
| 7 | フォールバック階層判定ロジック + precision_level 出力 | 0.5 日 | 6 |
| 8 | 検証スクリプト (都道府県集計 = 公式値、precision_level 分布) | 1 日 | 6/7 |
| 9 | 既存 5 都市 (川崎/相模原/浜松/堺/福岡) の妥当性確認 | 0.5 日 | 8 |
| 10 | docs 更新 (METRICS.md / 推定値ラベル運用 / レポート HTML 表示ルール) | 0.5 日 | 8/9 |
| **合計** | | **約 6〜7 日** | |

初版 (案 A) 推定 2〜3 日 → 再設計 (案 D) **6〜7 日**。+3〜4 日のコストで補正項を増やし、`estimate_grade` ラベル A 相当の市区町村を増やせる設計。実精度は実投入後の検証で確定する。

---

## 8. 精度限界 (補正後、改訂版)

### 8.1 補正項別の役割 (相対的な重要度)

| 補正項 | 役割 | 重要度 (相対) | grade への寄与 |
|-------|------|:-----------:|--------------|
| baseline (人口比+年齢性別) | 都道府県職業人口を市区町村に按分 | 必須 | grade 計算の前提 |
| F3 (産業構成) | 製造業集積/サービス業集積を反映 | ⭐⭐⭐ | A/A-/B の境界 |
| F4 (昼夜間) | オフィス街と住宅地の差 (workplace) | ⭐⭐ | C 以上に必要 |
| F5 (通勤 OD) | 流入元の職業構成を取り込む (workplace) | ⭐⭐⭐ | B 以上に必要 |
| F6 (SalesNow 残差) | F3 のみではカバーできない企業特性 | ⭐ | A の付加価値 |

→ **F3 と F5 が最重要**。利用可能なら相対的に高い grade (A/A-/B) になる。
→ ただし数値レンジ (±X%) の保証は実測検証後にのみ行う。**設計時点では grade ラベルのみ運用**。

### 8.2 検証可能な不変条件

```sql
-- 1. 都道府県集計 = e-Stat 公式値 (誤差 <1%)
SELECT prefecture, occupation_code, age_group, gender, basis,
       SUM(population) AS estimated_total
FROM municipality_occupation_population
GROUP BY prefecture, occupation_code, age_group, gender, basis;
-- prefectural_occupation_population との誤差 <1%

-- 2. 全市区町村合計 = 全国合計 (誤差 <0.5%)
SELECT occupation_code, age_group, gender, basis, SUM(population)
FROM municipality_occupation_population
GROUP BY occupation_code, age_group, gender, basis;

-- 3. precision_level 分布
SELECT precision_level, COUNT(*) FROM municipality_occupation_population
GROUP BY precision_level;
```

### 8.3 残差リスク (定性的)

| リスク | 確率 | 対応 |
|-------|:---:|------|
| 産業 → 職業マッピングの精度限界 (例: IT エンジニアは複数産業に分布) | 中 | `occupation_industry_weight` の精緻化 |
| 通勤 OD のサンプリング誤差 (希少 OD は不安定) | 中 | F5 の重みを `flow_count >= 100` で限定 |
| 政令市内の区差 (中心区 vs 郊外区) | 低 | F4/F5 で吸収 (区単位の昼夜比/OD あり) |
| 山間部・離島 | 高 | grade C/D にフォールバック、警告ラベル |
| F3 と F6 の二重補正による過剰補正 | 低 | §3.2 の運用ルール (alpha=0.25 残差補正) で抑制 |
| baseline の都道府県平均化 (大都市集中の歪み) | 中 | scaling で都道府県絶対値は保つが、市区町村相対差は補正項依存 |

---

## 9. 保守/標準/強気 母集団レンジへの接続 (METRICS.md §9)

### 9.1 統合式

```
推定母集団[muni][occ][scenario] =
    Σ_age_gender muni_occ_pop[muni][occ][age][gender][basis='resident']
  × age_gender_match_rate[occ][age][gender]
  × commute_reachable_rate[muni][occ]
  × occupation_transfer_rate[occ]
  × turnover_rate[scenario]    # 1% / 3% / 5%
  × competition_correction[muni][occ]
```

### 9.2 各項のソース

| 項 | ソース |
|---|--------|
| `muni_occ_pop` | 本テーブル (推定) |
| `age_gender_match_rate` | METRICS.md §3 (転換可能性) |
| `commute_reachable_rate` | `commute_flow_summary` (本タスクで JIS 化済) |
| `occupation_transfer_rate` | METRICS.md §4 (転換重み) |
| `turnover_rate` | 1%/3%/5% シナリオ別固定値 |
| `competition_correction` | `municipality_recruiting_scores.competitor_job_count` (Step 5 後段) |

### 9.3 不変条件

- `保守 ≦ 標準 ≦ 強気` (turnover_rate の単調性で保証)
- 各シナリオで合計が労働力人口を超えない

---

## 10. 推定ラベルの表示ルール (METRICS.md §1、改訂版)

レポート上では絶対誤差 (±X%) を表示せず、**`estimate_grade` のラベル** で扱う:

| `estimate_grade` | METRICS 表示 | レポート文言 (例) | UI 強調 |
|:---------------:|:-----------:|----------------|:------:|
| **A** | **推定 (高)** | 「baseline + 産業 + 昼夜間 + 通勤OD + 企業残差で算出」 | 通常 |
| **A-** | **推定 (高)** | 「baseline + 産業 + 昼夜間 + 通勤OD で算出」 | 通常 |
| **B** | **推定 (中)** | 「baseline + 昼夜間 + 通勤OD で算出 (産業構成情報なし)」 | 注記 (黄色) |
| **C** | **参考 (低-中)** | 「baseline + 昼夜間のみ。職業差の表現力が限定的」 | 警告 (オレンジ) |
| **D** | **参考 (低)** | 「baseline のみ (都道府県差で按分、市区町村差は微小)」 | 警告 (オレンジ) |
| **X** | **データ不足** | 「該当地域は推定不能、表示せず」 | 非表示 |

### 注意 (運用ルール)

- **絶対誤差 (±X%) を対外レポートに記載しない**。検証前の仮レンジは内部設計用のみ。
- レポート読者には `estimate_grade` (A〜D/X) で信頼度を伝え、grade C/D には注意文を付与する。
- 実投入後の検証フェーズで都道府県集計値との突合 (期待誤差 <1%) と主要市区町村サンプルチェックを行い、grade の信頼度ラベルの妥当性を検証する。

---

## 11. 制約と禁止事項遵守

| 項目 | 状態 |
|------|:---:|
| Turso upload | ❌ 設計のみ |
| `.env` / token 読み | ❌ 不要 |
| Rust 変更 | ❌ |
| ファイル作成 | 本書のみ (調査+設計) |
| push | ❌ |

---

## 12. 着手判断ポイント

### 12.1 本書承認後のアクション

| # | 必要前提 | 期間 |
|--:|---------|:----:|
| 1 | 案 D 多重補正の業務承認 | ユーザー判断 |
| 2 | e-Stat 都道府県×職業×年齢×性別 statsDataId 確定 | 0.5 日 |
| 3 | `v2_external_industry_structure` のローカル取得 (Turso からダウンロード or e-Stat 再 fetch) | 0.5 日 |
| 4 | `v2_salesnow_companies` のローカル取得 (任意、L1 用) | 0.5 日 |
| 5 | 実装着手 | 6〜7 日 |

### 12.2 Phase 3 Step 5 全体スケジュール (再見積)

| テーブル | 投入完了見込み |
|---------|:------------:|
| commute_flow_summary | ✅ 完了 |
| municipality_code_master | ✅ 完了 |
| municipality_occupation_population (案 D) | **+ 6〜7 日** |
| municipality_living_cost_proxy | + 別調査 (3〜5 日) |
| municipality_recruiting_scores | + 上記すべて + 計算 (2 日) |
| **Phase 3 Step 5 着手まで** | **約 14〜17 日** |

---

## 13. 関連 docs

- 旧版 (案 A、不採用): `SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_FEASIBILITY.md`
- DTO スキーマ: `survey_market_intelligence_phase0_2_schema.sql`
- METRICS: `SURVEY_MARKET_INTELLIGENCE_METRICS.md` (推定値ラベル §1、母集団 §9)
- Phase 3 全体: `SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md`
- 通勤 OD: `SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_COMMUTE_OD_REFACTOR.md`
- master: `SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md`

---

## 14. 完了条件 (本書の)

- [x] 案 A 不採用の理由明記
- [x] 必須要件 6 件すべて対応
- [x] 使う既存テーブル一覧 (ローカル/Turso/不在の区別)
- [x] 必要追加テーブル (`prefectural_occupation_population`, `occupation_industry_weight`)
- [x] 推定式 (再正規化付き、都道府県集計整合性保証)
- [x] フォールバック階層 (L1〜L6) + 判定ロジック
- [x] 精度限界 (補正項別寄与 + 残差リスク)
- [x] 保守/標準/強気 接続 (METRICS.md §9 統合式)
- [x] 推定ラベル表示ルール
- [x] 実装ステップ (10 段階、6〜7 日)
- [x] Phase 3 Step 5 全体スケジュール (再見積)

本書をもって、Worker C 再設計が完了。実装着手はユーザー承認後。
