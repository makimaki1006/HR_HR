# Phase 3: Model F2 (estimate_grade A-) 本実装計画書

**作成日**: 2026-05-04
**作成者**: Worker B2
**対象モデル**: Model F2 (industrial_anchor_city + HQ-with-factory boost)
**承認状態**: estimate_grade A- (5/5 達成、本実装承認済)
**プロト出典**: `scripts/proto_evaluate_occupation_population_models.py`
**プロト評価**: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_PROTO_EVALUATION.md` §0.6

---

## 0. 本書の位置付け

### 0.1 目的

Model F2 を本実装し、市区町村×職業のターゲット厚み指数 (0-200 正規化) を SQLite ローカル DB および Turso DB に投入するための **詳細実装計画書**。

### 0.2 商品仕様 (人数表示禁止)

商品メッセージは下記 3 種に限定:

| 出力 | 形式 | 範囲 |
|------|------|------|
| **ターゲット厚み指数** | 整数 (全国平均=100) | 0-200 (cap) |
| **配信優先度** | A / B / C / D | 上位 5% / 15% / 50% / それ以下 |
| **採用シナリオ濃淡** | 1× / 3× / 5× | 保守 / 標準 / 強気 |

**禁止**: 人数の絶対値表示、推定就業者数の数値出力、シナリオ別人数。

### 0.3 仮テーブル名

本書では `municipality_target_thickness_scores` を仮名として使用する (Worker A2 で確定後置換)。

### 0.4 制約

- DB 書き込みは Worker B2 (本書) では実行しない (計画書のみ)
- Turso 接続は本書で実行しない
- `.env` 直接 open 禁止
- 重大事故記録 (2026-01-06、$195 超過請求) を踏まえ、Turso 投入は CSV 完成後 1 回のみ

---

## 1. 入力テーブル一覧

### 1.1 全体マトリックス

| テーブル | データソース | 投入状態 | 備考 |
|---------|-------------|:-------:|------|
| `v2_external_population` | e-Stat | ✅ 既存 (ローカル) | F1 (人口比按分) |
| `v2_external_population_pyramid` | e-Stat | ✅ 既存 (ローカル) | F2 (年齢性別補正、生産年齢人口取得) |
| `v2_external_daytime_population` | e-Stat | ✅ 既存 (ローカル) | F4 (昼夜間人口比) + F5 (流入人口) |
| `commute_flow_summary` | e-Stat 通勤 OD | ✅ 既存 (ローカル) | F5 補強 (現状はプロトで daytime の inflow のみ使用) |
| `municipality_code_master` | 派生 | ✅ 既存 (ローカル) | JIS 結合キー |
| `v2_external_industry_structure` | 経済センサス R3 | ⚠️ **要投入** | F3 (産業構成補正)、F6 (本社過剰判定) |
| `v2_salesnow_companies` | SalesNow | ⚠️ **ローカル CSV のみ** (Turso V2 既存) | F6 (本社過剰判定) |
| `occupation_industry_weight` | hypothesis_v1 | ⚠️ **要新規投入** | 産業 → 職業重みマスタ (231 行) |
| (派生) 仮 ground truth | 全国構成比 × 都道府県人口 | スクリプト内構築 | 都道府県職業按分の起点 (Model B 相当) |

### 1.2 ⚠️ 要投入テーブルの投入手順

#### 1.2.1 `v2_external_industry_structure`

**ソース**: 経済センサス R3 (e-Stat、表番号 0003411932 等)
**ローカル CSV**: `scripts/data/industry_structure_by_municipality.csv` (36,099 行、既存)
**スクリプト**: `scripts/fetch_industry_structure.py` (取得済み、再取得不要)

**投入手順**:

```bash
# (1) 既に CSV 存在する場合は SQLite 投入のみ
cd C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy

# 投入用スクリプト (本実装で新規作成、ingest_industry_structure.py)
python scripts/ingest_industry_structure.py \
    --csv scripts/data/industry_structure_by_municipality.csv \
    --db data/hellowork.db \
    --table v2_external_industry_structure
```

**スキーマ案 (industry_structure_by_municipality)**:
```sql
CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
    prefecture TEXT NOT NULL,
    municipality TEXT NOT NULL,
    jsic_code TEXT NOT NULL,         -- 'D','E','H','I' 等
    industry_name TEXT,
    employees INTEGER,                -- 従業者数
    establishments INTEGER,           -- 事業所数
    survey_year TEXT DEFAULT 'R3',
    source TEXT DEFAULT 'estat_economic_census_r3',
    PRIMARY KEY (prefecture, municipality, jsic_code)
);
CREATE INDEX idx_industry_structure_pref ON v2_external_industry_structure(prefecture);
CREATE INDEX idx_industry_structure_jsic ON v2_external_industry_structure(jsic_code);
```

#### 1.2.2 `v2_salesnow_companies` (ローカル投入)

**現状**: Turso V2 には既存、ローカルにはローカル CSV `data/salesnow_companies.csv` (492 MB) のみ
**集約 CSV (F6 用)**: `data/generated/salesnow_aggregate_for_f6.csv` (11,071 行、既存)

**方針**: Model F2 計算では **集約 CSV を直接ロード** (492 MB を毎回 SQLite に投入する必要はない)。プロトと同じ load_salesnow_aggregate(use_cache=True) パターンを踏襲。

```python
# build_municipality_target_thickness.py 内
sn_emp = load_salesnow_aggregate(use_cache=True)  # CSV キャッシュから読み込み
```

#### 1.2.3 `occupation_industry_weight` (新規)

**現状 CSV**: `data/generated/occupation_industry_weight.csv` (231 行 = 21 産業 × 11 職業)
**ステータス**: hypothesis_v1 (実測値ではない、Phase 4 で e-Stat 産業×職業実績で置換予定)

**重要**: `weight_source = 'hypothesis_v1'` を **毎行に明示** すること。ユーザーが商品上で信頼度を判断できるようにするため。

**投入手順**:
```bash
# 投入用スクリプト (新規作成、ingest_occupation_industry_weight.py)
python scripts/ingest_occupation_industry_weight.py \
    --csv data/generated/occupation_industry_weight.csv \
    --db data/hellowork.db \
    --table occupation_industry_weight
```

**スキーマ案**:
```sql
CREATE TABLE IF NOT EXISTS occupation_industry_weight (
    jsic_code TEXT NOT NULL,            -- 'D','E','H' 等
    occupation_code TEXT NOT NULL,      -- '01_管理'..'11_運搬清掃'
    weight REAL NOT NULL,               -- 0.0-1.0
    weight_source TEXT NOT NULL DEFAULT 'hypothesis_v1',
    notes TEXT,
    PRIMARY KEY (jsic_code, occupation_code)
);
```

**Phase 4 ロードマップ (別 docs)**: e-Stat 国勢調査 (就業状態等基本集計、表番号 0003411938 等) または労働力調査特別集計から **産業 × 職業の実測クロス表** を取得し、`weight_source = 'estat_v1'` で更新する手順を別途文書化。本書では hypothesis_v1 を初期値として採用する。

### 1.3 既存テーブルの利用

`v2_external_population` 等の既存テーブルは、プロトスクリプトの `load_data()` 関数のクエリをそのまま再利用する。本実装スクリプトでは関数化して切り出す。

---

## 2. 必要 CSV / マスタファイル

| ファイル | 場所 | 行数 | ステータス | 用途 |
|---------|------|:----:|:---------:|------|
| `occupation_industry_weight.csv` | `data/generated/` | 231 | ✅ 既存 | hypothesis_v1 の産業×職業重み |
| `industry_structure_by_municipality.csv` | `scripts/data/` | 36,099 | ✅ 既存 | 経済センサス R3 |
| `salesnow_companies.csv` | `data/` | 492 MB (約 19 万行) | ✅ 既存 | F6 用、集約後使用 |
| `salesnow_aggregate_for_f6.csv` | `data/generated/` | 11,071 | ✅ 既存 (キャッシュ) | F6 用集約版 |
| `municipality_target_thickness.csv` | `data/generated/` | (未作成) | ❌ **本実装で生成** | 最終アウトプット (~38,324 行) |

### 2.1 weight_source 列の必須運用

`occupation_industry_weight.csv` には全 231 行に `weight_source = hypothesis_v1` を保持する。本実装スクリプトはロード時に下記をチェック:

```python
def load_occupation_industry_weight(csv_path):
    df = pd.read_csv(csv_path)
    assert "weight_source" in df.columns, "weight_source 列が必須"
    sources = df["weight_source"].unique()
    assert all(s == "hypothesis_v1" for s in sources), \
        f"hypothesis_v1 以外の weight_source 検出: {sources}"
    print(f"[INFO] weight_source = hypothesis_v1, {len(df)} 行")
    return df
```

---

## 3. 生成スクリプト設計

### 3.1 新規スクリプト

| スクリプト | 役割 | 行数目安 |
|-----------|------|:-------:|
| `scripts/ingest_industry_structure.py` | CSV → SQLite (v2_external_industry_structure) | 100 |
| `scripts/ingest_occupation_industry_weight.py` | CSV → SQLite (occupation_industry_weight) | 80 |
| `scripts/build_municipality_target_thickness.py` | **メイン**: F2 計算 → CSV → SQLite | 700 |
| `scripts/proto_sensitivity_anchor_thresholds.py` | 4 条件閾値 sensitivity 分析 (本書 §8) | 300 |
| `scripts/upload_target_thickness_to_turso.py` | CSV → Turso | 200 |

### 3.2 アーキテクチャ

```
[ローカル DB + CSV]
     │
     ▼
load_inputs()  ─────────────► dict (population, pyramid, daytime, industry_*, sn_emp, weights)
     │
     ▼
compute_baseline()  ────────► Model B (生産年齢人口比按分)
     │
     ▼
compute_f3() (べき乗 1.5)
     │
     ▼
compute_f4_occupation_weighted() (職業別重み)
     │
     ▼
compute_f5() (流入補正)
     │
     ▼
compute_industrial_anchor() (4条件 AND)
     │
     ▼
compute_f6_v2() (anchor 分岐 + HQ-with-factory boost)
     │
     ▼
integrate_model_f2() (F1×F2×F3×F4×F5×F6 + 都道府県スケーリング)
     │
     ▼
derive_thickness_index() (0-200 正規化)
     │
     ▼
derive_rank_and_priority() (全国順位 + percentile + A/B/C/D)
     │
     ▼
derive_scenario_indices() (1×/3×/5×)
     │
     ▼
derive_estimate_grade() (A- 固定 or 検証 fail で B/C)
     │
     ▼
export_to_csv() ─────────► data/generated/municipality_target_thickness.csv
     │
     ▼
import_to_sqlite() ──────► data/hellowork.db (DROP + CREATE + INSERT)
     │
     ▼
[Turso upload は別スクリプト]
```

### 3.3 関数構成 (build_municipality_target_thickness.py)

```python
# === I/O ===
def load_inputs() -> dict:
    """ローカル DB + CSV から入力をロード.
    Returns:
        data: {
            "population": dict[(pref, muni)] -> int,
            "pyramid": dict[(pref, muni)] -> {age: int},
            "daytime": dict[(pref, muni)] -> {day, night, ratio, inflow},
            "industry_emp": dict[(pref, muni)] -> {jsic_code: int},
            "industry_est": dict[(pref, muni)] -> {jsic_code: int},
            "national_emp": dict[jsic_code] -> int,
            "industry_share": dict[(pref, muni)] -> {jsic_code: float},
            "national_share": dict[jsic_code] -> float,
            "sn_emp": dict[(pref, muni, jsic_code)] -> int (SalesNow),
            "weights": dict[(jsic_code, occ_code)] -> float (hypothesis_v1),
            "muni_master": dict[(pref, muni)] -> jis_code,
        }
    """

def load_occupation_industry_weight(csv_path: Path) -> dict:
    """weight_source = 'hypothesis_v1' を assert"""

# === 計算 ===
def compute_baseline(data: dict) -> dict:
    """F1 (総人口) × F2 (生産年齢人口比) → Model B 相当.
    都道府県就業者数 × (muni 生産年齢 / pref 生産年齢) で按分.
    """

def compute_f3(data: dict, baseline: dict) -> dict:
    """F3 (産業構成補正、べき乗 1.5).
    F3_per_occ = (Σ ind_share × weight) / (Σ nat_share × weight) ** 1.5
    """

def compute_f4_occupation_weighted(data: dict) -> dict:
    """F4 (職業別 OCCUPATION_F4_WEIGHT 適用).
    f4 = 1 + (day/night - 1) × occ_weight,  clamp [0.1, 5.0]
    """

def compute_f5(data: dict) -> dict:
    """F5 (通勤流入補正).
    f5 = 1 + (inflow / night) × 0.3,  clamp [0.5, 2.0]
    """

def compute_industrial_anchor(data: dict) -> tuple[set, dict]:
    """4 条件 AND 判定.
    COND_A: mfg_share > 0.12
    COND_B: emp_per_est > 20.0
    COND_C: dn_ratio < 150.0
    COND_D: hq_excess_E < 5.0
    Returns: (anchor_set, audit)
    """

def compute_f6_v2(data: dict, anchor_set: set) -> dict:
    """F6 anchor 分岐.
    anchor IN: HQ 減衰スキップ + boost = 1 + γ×(mfg_share - SHARE_MIN)×sqrt(emp_per_est/EST_MIN)
    anchor OUT: F (HQ 減衰 + 工場ブースト)
    """

def integrate_model_f2(baseline, f3, f4, f5, f6, data) -> dict:
    """全要素統合 + 都道府県スケーリング.
    raw[(pref,muni)][occ] = baseline × f3 × f4 × f5 × f6
    scaling[pref][occ] = pref_target / raw_sum
    out[(pref,muni)][occ] = raw × scaling
    """

# === 派生指標 ===
def derive_thickness_index(model_f2: dict) -> dict:
    """0-200 正規化 (100 = 全国平均).
    thickness_index[muni, occ] = 100 × (model_f2[muni, occ] × N_munis) /
                                       (Σ_muni model_f2[muni, occ])
    cap [0, 200]
    """

def derive_rank_and_priority(thickness: dict) -> dict:
    """全国順位 + percentile + A/B/C/D.
    rank_in_occupation = 1 + count(other > self)
    rank_percentile = rank / N_munis
    priority:
        <= 0.05 → A
        <= 0.15 → B
        <= 0.50 → C
        else    → D
    """

def derive_scenario_indices(thickness: dict) -> dict:
    """conservative=1×, standard=3×, aggressive=5× (定数倍ラベル付与).
    本実装では数値ではなく 1× / 3× / 5× ラベル文字列のみを保持.
    """

def derive_estimate_grade(model_f2: dict, conditions: dict) -> str:
    """グローバル grade 算出 (5 条件再評価).
    検証 pass で 'A-' 固定、4/5 で 'B' (要再検証)、3/5 で 'C'.
    """

# === 出力 ===
def export_to_csv(records: list[dict], path: Path) -> None:
    """CSV 出力 (utf-8、quoting=csv.QUOTE_MINIMAL)"""

def import_to_sqlite(csv_path: Path, db_path: Path, table_name: str) -> None:
    """ローカル DB 投入 (DROP TABLE IF EXISTS + CREATE + INSERT batch)"""

# === エントリポイント ===
def main():
    args = parse_args()
    data = load_inputs(args.db_path)
    baseline = compute_baseline(data)
    f3 = compute_f3(data, baseline)
    f4 = compute_f4_occupation_weighted(data)
    f5 = compute_f5(data)
    anchor_set, audit = compute_industrial_anchor(data)
    f6 = compute_f6_v2(data, anchor_set)
    model_f2 = integrate_model_f2(baseline, f3, f4, f5, f6, data)
    thickness = derive_thickness_index(model_f2)
    ranked = derive_rank_and_priority(thickness)
    scenarios = derive_scenario_indices(thickness)
    grade = derive_estimate_grade(model_f2, conditions={...})
    records = assemble_records(ranked, scenarios, grade, anchor_set)
    export_to_csv(records, args.csv_path)
    if not args.dry_run:
        import_to_sqlite(args.csv_path, args.db_path, "municipality_target_thickness_scores")
```

### 3.4 計算式 (重要部分の詳細)

#### 3.4.1 thickness_index の正規化式

```
thickness_index[muni, occ] = 100 × (model_f2[muni, occ] × N_munis) /
                                   (Σ_muni model_f2[muni, occ])
```

ここで:
- N_munis = 1,742 (全国市区町村数、basis ごと)
- 全国平均は厳密に 100 に揃う
- 上下に対称的に分布、上限 200 で cap (`min(value, 200)`)

#### 3.4.2 rank と priority

```
rank_in_occupation[muni, occ] = 1 + |{m : model_f2[m, occ] > model_f2[muni, occ]}|
rank_percentile[muni, occ] = rank_in_occupation / N_munis

distribution_priority:
  rank_percentile <= 0.05 → 'A'
  rank_percentile <= 0.15 → 'B'
  rank_percentile <= 0.50 → 'C'
  else                    → 'D'
```

期待分布: A 約 5% / B 約 10% / C 約 35% / D 約 50%

#### 3.4.3 シナリオラベル

`scenario_label` は文字列で保持 (人数禁止):

```
conservative_label = '1×'
standard_label = '3×'
aggressive_label = '5×'
```

別途 `scenario_recommendation` 列で「strong (priority A)」「standard (B)」「weak (C/D)」を出力。

### 3.5 出力テーブルスキーマ案 (仮)

```sql
CREATE TABLE IF NOT EXISTS municipality_target_thickness_scores (
    -- 識別子
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    jis_code TEXT,
    occupation_code TEXT NOT NULL,        -- '01_管理' ... '11_運搬清掃'
    basis TEXT NOT NULL,                  -- 'workplace' | 'residence'

    -- メイン指標 (人数禁止)
    thickness_index INTEGER NOT NULL,     -- 0-200 (100=全国平均)
    rank_in_occupation INTEGER NOT NULL,  -- 1..N_munis
    rank_percentile REAL NOT NULL,        -- 0.0-1.0
    distribution_priority TEXT NOT NULL,  -- 'A' | 'B' | 'C' | 'D'

    -- シナリオ濃淡 (数値ではなくラベル)
    scenario_conservative TEXT NOT NULL DEFAULT '1×',
    scenario_standard TEXT NOT NULL DEFAULT '3×',
    scenario_aggressive TEXT NOT NULL DEFAULT '5×',
    scenario_recommendation TEXT,          -- 'strong'|'standard'|'weak'

    -- メタデータ (信頼度・診断)
    estimate_grade TEXT NOT NULL,          -- 'A-' (本初期版固定)
    is_industrial_anchor INTEGER NOT NULL, -- 0|1
    weight_source TEXT NOT NULL DEFAULT 'hypothesis_v1',
    model_version TEXT NOT NULL DEFAULT 'F2_v1',
    computed_at TEXT NOT NULL,             -- ISO8601

    PRIMARY KEY (prefecture, municipality_name, occupation_code, basis)
);

CREATE INDEX idx_target_thickness_pref ON municipality_target_thickness_scores(prefecture);
CREATE INDEX idx_target_thickness_priority ON municipality_target_thickness_scores(distribution_priority);
CREATE INDEX idx_target_thickness_occ ON municipality_target_thickness_scores(occupation_code);
```

### 3.6 期待行数

```
1,742 市区町村 × 11 職業 × 2 basis (workplace, residence) = 38,324 行
```

許容範囲: ± 5% (基礎データ欠損吸収)、つまり 36,408 〜 40,240 行。

---

## 4. 検証 SQL

### 4.1 行数チェック

```sql
SELECT COUNT(*) FROM municipality_target_thickness_scores;
-- 期待: 1,742 × 11 × 2 = 38,324 ± 5%
```

### 4.2 thickness_index レンジ

```sql
SELECT MIN(thickness_index) AS min_idx,
       MAX(thickness_index) AS max_idx,
       AVG(thickness_index) AS avg_idx,
       -- SQLite に STDEV ない場合は (avg(x*x) - avg(x)*avg(x)) を sqrt
       (AVG(thickness_index * thickness_index) - AVG(thickness_index) * AVG(thickness_index)) AS variance
FROM municipality_target_thickness_scores;
-- 期待: MIN >= 0, MAX <= 200, AVG ≈ 100
```

### 4.3 distribution_priority 分布

```sql
SELECT distribution_priority, COUNT(*) AS cnt,
       ROUND(100.0 * COUNT(*) / (SELECT COUNT(*) FROM municipality_target_thickness_scores), 2) AS pct
FROM municipality_target_thickness_scores
GROUP BY distribution_priority
ORDER BY distribution_priority;
-- 期待: A ≈ 5%, B ≈ 10%, C ≈ 35%, D ≈ 50%
```

### 4.4 港区生産工程順位

```sql
SELECT rank_in_occupation, thickness_index, distribution_priority
FROM municipality_target_thickness_scores
WHERE prefecture = '東京都'
  AND municipality_name = '港区'
  AND occupation_code = '08_生産工程'
  AND basis = 'workplace';
-- 期待: rank_in_occupation >= 30 (プロト F2 = 50)
```

### 4.5 製造系工業都市 TOP 10 検証 (4 都市以上が含まれること)

```sql
WITH mfg AS (
    SELECT municipality_name, prefecture,
           AVG(thickness_index) AS avg_idx
    FROM municipality_target_thickness_scores
    WHERE basis = 'workplace'
      AND occupation_code IN ('08_生産工程', '09_輸送機械')
    GROUP BY prefecture, municipality_name
    ORDER BY avg_idx DESC
    LIMIT 10
)
SELECT prefecture, municipality_name FROM mfg
WHERE municipality_name IN
    ('豊田市', '太田市', '浜松市', '堺市', '川崎市',
     '相模原市', '厚木市', '四日市市', '北九州市');
-- 期待: 4 行以上 (5/5 条件達成のため)
```

### 4.6 weight_source 確認

```sql
SELECT DISTINCT weight_source
FROM municipality_target_thickness_scores;
-- 期待: 'hypothesis_v1' (本ラウンド初期版のみ)
```

### 4.7 anchor city フラグの整合性

```sql
SELECT prefecture, municipality_name, is_industrial_anchor
FROM municipality_target_thickness_scores
WHERE (prefecture='愛知県' AND municipality_name='豊田市')
   OR (prefecture='神奈川県' AND municipality_name='川崎市')
   OR (prefecture='東京都' AND municipality_name='港区')
GROUP BY prefecture, municipality_name, is_industrial_anchor;
-- 期待: 豊田市=1, 川崎市=1, 港区=0
```

### 4.8 estimate_grade 一意性

```sql
SELECT DISTINCT estimate_grade FROM municipality_target_thickness_scores;
-- 期待: 'A-' のみ (本初期版グローバル固定)
```

検証 SQL 件数: **8 件**

---

## 5. Turso upload 手順

### 5.1 制約 (重大事故記録 2026-01-06)

> 過去事故: Claude による DB 書き込み 4 倍超過 → $195 超過請求、1 ヶ月実装遅延。

**遵守事項**:

| ルール | 具体的行動 |
|--------|-----------|
| upload は 1 回のみ | CSV 検証完了後、人間がコマンド実行 |
| Claude は upload しない | 計画書のみ作成、本番 DB 操作禁止 |
| 再投入時は事前許可 | DROP + CREATE は重ね実行しない |
| Turso V1 (`hellowork.db` 用) と V2 (外部統計) を区別 | 本テーブルは V1 |

### 5.2 upload スクリプト設計

新規: `scripts/upload_target_thickness_to_turso.py`

```python
# 既存 scripts/upload_to_turso.py の turso_pipeline() を流用.
# テーブル定義 + INSERT OR REPLACE バッチ (1,000 行/batch).

TABLE_SCHEMA = """
CREATE TABLE IF NOT EXISTS municipality_target_thickness_scores (
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    jis_code TEXT,
    occupation_code TEXT NOT NULL,
    basis TEXT NOT NULL,
    thickness_index INTEGER NOT NULL,
    rank_in_occupation INTEGER NOT NULL,
    rank_percentile REAL NOT NULL,
    distribution_priority TEXT NOT NULL,
    scenario_conservative TEXT NOT NULL,
    scenario_standard TEXT NOT NULL,
    scenario_aggressive TEXT NOT NULL,
    scenario_recommendation TEXT,
    estimate_grade TEXT NOT NULL,
    is_industrial_anchor INTEGER NOT NULL,
    weight_source TEXT NOT NULL,
    model_version TEXT NOT NULL,
    computed_at TEXT NOT NULL,
    PRIMARY KEY (prefecture, municipality_name, occupation_code, basis)
)
"""

def main():
    args = parse_args()
    rows = read_csv_rows(args.csv)  # ~38,324 行

    # (1) DDL
    turso_pipeline(args.url, args.token, [
        "DROP TABLE IF EXISTS municipality_target_thickness_scores",  # 初回のみ
        TABLE_SCHEMA,
    ])

    # (2) DML (バッチ 1,000 行)
    BATCH = 1000
    for chunk in batch(rows, BATCH):
        stmts = [build_insert(row) for row in chunk]
        turso_pipeline(args.url, args.token, stmts)

    # (3) 検証
    cnt = turso_query(args.url, args.token, "SELECT COUNT(*) FROM municipality_target_thickness_scores")
    assert cnt[0] >= 36000, f"行数異常: {cnt[0]}"
```

### 5.3 Turso write 上限確認

- 行数: 38,324 行
- バッチサイズ: 1,000 行 → 38 バッチ
- 1 バッチ = 1 write request → 約 38 write
- **Turso 月間 write 上限の約 5%** (V1 hellowork.db 想定、実測値要確認)

### 5.4 推奨実行手順 (人間)

```bash
# (1) ローカル CSV 検証完了後
sqlite3 data/hellowork.db < scripts/sql/verify_target_thickness.sql
# 期待出力: 全 8 検証 SQL が pass

# (2) Turso upload (人間が実行)
python scripts/upload_target_thickness_to_turso.py \
    --csv data/generated/municipality_target_thickness.csv \
    --url $TURSO_DATABASE_URL \
    --token $TURSO_AUTH_TOKEN

# (3) Turso 側でも検証 SQL 実行
turso db shell hellowork < scripts/sql/verify_target_thickness.sql
```

---

## 6. estimate_grade 算出ロジック

### 6.1 グローバル grade

本初期版は **全行同じ grade** を持つ (テーブル全体の信頼度メトリック)。

```python
def compute_global_grade(model_f2: dict, conditions: dict) -> str:
    """本ラウンドの 5 条件を再評価:
    - C1: 港区生産工程順位 >= 30
    - C2: 製造系 TOP 10 工業都市数 >= 4/9
    - C3: 物流系 TOP 10 湾岸都市数 >= 5/7
    - C4: TOP 10 Jaccard 平均 < 0.65
    - C5: 全職種同 TOP 10 でない
    """
    pass_cnt = sum(1 for c in conditions.values() if c)
    if pass_cnt == 5:
        return "A-"
    elif pass_cnt == 4:
        return "B"
    elif pass_cnt == 3:
        return "C"  # 要再検証
    else:
        return "D"  # 投入見送り推奨
```

### 6.2 grade による分岐

| grade | 行動 |
|-------|------|
| `A-` (5/5) | そのまま全行に書き込み、Turso 投入可 |
| `B` (4/5) | 投入可だが docs に「品質低下」記録 |
| `C` (3/5) | **投入見送り、プロトに戻る** |
| `D` (2/5 以下) | **投入禁止、原因調査** |

### 6.3 将来拡張 (本実装外)

市区町村別の信頼度:
- anchor city (4 条件 AND True) → A
- 大都市 (人口 50 万+) → A
- 中規模都市 (10 万 - 50 万) → B
- 住宅地・小規模 → C

→ 本実装ではグローバル `'A-'` 固定で十分。Phase 5 以降で row-level grade に拡張余地あり。

---

## 7. extreme 値検証

### 7.1 検証ターゲット

```python
EXTREME_TARGETS = [
    # オフィス街 (生産工程は低位であるべき)
    ('東京都', '港区', '08_生産工程', 'workplace', 'rank >= 30'),
    ('東京都', '中央区', '08_生産工程', 'workplace', 'rank >= 30'),
    ('東京都', '千代田区', '08_生産工程', 'workplace', 'rank >= 30'),
    ('東京都', '新宿区', '08_生産工程', 'workplace', 'rank >= 20'),

    # 工業都市 (生産工程は上位であるべき)
    ('愛知県', '豊田市', '08_生産工程', 'workplace', 'rank <= 15'),
    ('神奈川県', '川崎市', '08_生産工程', 'workplace', 'rank <= 10'),
    ('大阪府', '堺市',   '08_生産工程', 'workplace', 'rank <= 10'),
    ('福岡県', '北九州市', '08_生産工程', 'workplace', 'rank <= 15'),

    # 住宅地 (専門技術が極端に高くないこと)
    ('東京都', '青梅市', '02_専門技術', 'workplace', 'thickness_index <= 130'),

    # 異常検出
    ('*', '*', '*', '*', 'thickness_index <= 200'),  # cap 超過なし
    ('*', '*', '*', '*', 'thickness_index >= 0'),    # 負値なし
    ('*', '*', '*', '*', 'rank_in_occupation > 0'),  # rank 1 以上
]
```

### 7.2 検証スクリプト

`scripts/verify_target_thickness.py` (新規):

```python
def verify_extreme_targets(db_path):
    failures = []
    for pref, muni, occ, basis, expected in EXTREME_TARGETS:
        if pref == '*':
            # 全行チェック
            row = db.execute(f"SELECT MIN(...), MAX(...) FROM ...").fetchone()
            if not eval_expression(row, expected):
                failures.append(f"GLOBAL {expected} FAIL")
        else:
            row = db.execute(
                "SELECT rank_in_occupation, thickness_index FROM municipality_target_thickness_scores "
                "WHERE prefecture=? AND municipality_name=? AND occupation_code=? AND basis=?",
                (pref, muni, occ, basis)
            ).fetchone()
            if not eval_expression(row, expected):
                failures.append(f"{pref} {muni} {occ} ({basis}): {expected} FAIL (got {row})")
    return failures
```

### 7.3 検証 fail 時の対応

> 検証 fail なら **本実装中止 → プロト戻り**。原因調査して F2 ロジック修正、再評価。

---

## 8. F2 閾値 sensitivity 分析 (本実装前必須)

### 8.1 目的

F2 の 4 条件の閾値:
- ANCHOR_MFG_SHARE_MIN = 0.12
- ANCHOR_EMP_PER_EST_MIN = 20.0
- ANCHOR_DN_RATIO_MAX = 150.0
- ANCHOR_HQ_EXCESS_E_MAX = 5.0

これらを ±20% 振った時、estimate_grade が安定して A- を維持するかを確認。

### 8.2 sensitivity スクリプト

`scripts/proto_sensitivity_anchor_thresholds.py` (新規、スケルトンのみ本書範囲外):

```python
"""F2 anchor city 閾値 sensitivity 分析.

各閾値を ±20% (低/中/高) の 3 段階で振り、3^4 = 81 通りで estimate_grade を再算出.
grade A- 維持率 >= 80% なら本実装着手可、未満なら閾値再調整.
"""
import itertools

THRESHOLD_VARIATIONS = {
    "ANCHOR_MFG_SHARE_MIN":   [0.096, 0.12, 0.144],   # ±20%
    "ANCHOR_EMP_PER_EST_MIN": [16.0, 20.0, 24.0],     # ±20%
    "ANCHOR_DN_RATIO_MAX":    [120.0, 150.0, 180.0],  # ±20%
    "ANCHOR_HQ_EXCESS_E_MAX": [4.0, 5.0, 6.0],        # ±20%
}

def main():
    results = []
    for combo in itertools.product(*THRESHOLD_VARIATIONS.values()):
        cfg = dict(zip(THRESHOLD_VARIATIONS.keys(), combo))
        anchor_set, audit = compute_industrial_anchor(data, **cfg)
        f6 = compute_f6_v2(data, anchor_set)
        model_f2 = integrate_model_f2(...)
        grade, conditions = evaluate_grade(model_f2)
        results.append({"cfg": cfg, "grade": grade, "anchor_count": len(anchor_set), **conditions})

    # 集計
    a_minus_count = sum(1 for r in results if r["grade"] == "A-")
    print(f"A- 維持率: {a_minus_count}/{len(results)} = {100*a_minus_count/len(results):.1f}%")

    # 81 通り中 65 以上 (>= 80%) なら本実装着手可
    if a_minus_count / len(results) >= 0.80:
        print("[OK] sensitivity PASS, 本実装着手可")
    else:
        print("[FAIL] 閾値再調整が必要")
```

### 8.3 着手判定基準

- A- 維持率 ≥ 80% → 本実装着手 OK
- 60% - 80% → 閾値微調整 (中央値検討) 後再実行
- < 60% → モデル設計再考 (F6 ロジック見直し含む)

---

## 9. 実装ロードマップ

### 9.1 Phase 別タスク

| Phase | 作業 | 所要 | 依存 | 担当 |
|------:|------|:----:|:----:|:----:|
| 1 | テーブル名確定 | 即時 | - | Worker A2 |
| 2 | F2 閾値 sensitivity 分析 (§8) | 1 日 | プロト + 本書 | 別 worker |
| 3 | `v2_external_industry_structure` ローカル投入 | 0.5 日 | 既存 CSV (`industry_structure_by_municipality.csv`) | 別 worker |
| 4 | `occupation_industry_weight` テーブル投入 (hypothesis_v1) | 0.5 日 | 既存 CSV (`occupation_industry_weight.csv`) | 別 worker |
| 5 | `build_municipality_target_thickness.py` 実装 | 2 日 | プロト関数化 + Phase 1-4 | 別 worker |
| 6 | ローカル DB 投入 + 検証 SQL 実行 (§4) | 0.5 日 | Phase 5 完了 | 別 worker |
| 7 | extreme 値検証 (§7) | 0.5 日 | Phase 6 完了 | 別 worker |
| 8 | Turso upload (§5) | 0.5 日 | Phase 7 pass | 人間 (Claude 禁止) |
| 9 | Rust ハンドラ統合 | 別ラウンド | Phase 8 完了 | 別 worker |

### 9.2 合計所要日数

**Phase 1-8 合計: 約 5.5 日** (Phase 9 別ラウンド)

### 9.3 ボトルネック

| Phase | 所要 | ボトルネック理由 |
|------:|:----:|------------------|
| **Phase 5** | **2 日** | プロトの 9 関数 (model_f2, compute_f6_factor_v2, compute_industrial_anchor_cities, load_*, etc.) を本実装用にリファクタ。CSV 出力 + SQLite 投入の追加。**最大ボトルネック**。 |
| Phase 2 | 1 日 | sensitivity 81 通りはプロトの再実行で計算コスト中程度 (1 通り ~30 秒 × 81 = 約 40 分)。グレード判定ロジックは新規実装。 |
| Phase 6+7 | 1 日 | 検証 SQL 8 件 + extreme 値 12 件。fail 時はプロト戻り。 |

### 9.4 並列化機会

- Phase 3 と Phase 4 は独立 → 並列実行可能 (両方とも 0.5 日)
- Phase 2 (sensitivity) と Phase 3-4 (DB 投入準備) も独立 → 並列実行可能

→ 並列化により **合計 4.5 日** に短縮可能。

### 9.5 リスクと対策

| リスク | 対策 |
|--------|------|
| sensitivity で A- 維持率 < 80% | Phase 1-4 並行で進めず、まず sensitivity 完了確認 |
| DB 投入時に行数不一致 | Phase 6 検証 SQL で即検出、fail 時 Phase 5 戻り |
| Turso write 上限超過 | バッチサイズ 1,000 行で 38 write、上限の 5% 程度。事前に Turso ダッシュボードで残 quota 確認 |
| weight_source 仮値の品質懸念 | Phase 4 で `weight_source = 'hypothesis_v1'` を CSV と DB で必須 assert、商品上に注記表示 |

---

## 10. ファイル成果物

### 10.1 本書範囲 (Worker B2 作成)

| ファイル | 種別 | 状態 |
|---------|------|:----:|
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_IMPLEMENTATION_PLAN.md` | 計画書 (本書) | ✅ 作成 |

### 10.2 本実装範囲 (別 worker)

| ファイル | 種別 | Phase |
|---------|------|:-----:|
| `scripts/proto_sensitivity_anchor_thresholds.py` | sensitivity スクリプト | 2 |
| `scripts/ingest_industry_structure.py` | DB 投入 | 3 |
| `scripts/ingest_occupation_industry_weight.py` | DB 投入 | 4 |
| `scripts/build_municipality_target_thickness.py` | メインスクリプト | 5 |
| `scripts/verify_target_thickness.py` | 検証スクリプト | 6+7 |
| `scripts/sql/verify_target_thickness.sql` | 検証 SQL 集 | 6 |
| `scripts/upload_target_thickness_to_turso.py` | Turso upload | 8 |
| `data/generated/municipality_target_thickness.csv` | 出力 CSV | 5 |

---

## 11. 参照ドキュメント

| ドキュメント | 内容 |
|-------------|------|
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_PROTO_EVALUATION.md` | プロト評価結果 (Model F2 grade A- 達成根拠) |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_INDUSTRY_WEIGHT_HYPOTHESIS.md` | hypothesis_v1 重みマスタの根拠 |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_MODEL_V2.md` | モデル V2 設計仕様 |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_INDUSTRY_DATA_AUDIT.md` | 経済センサス監査結果 |
| `scripts/proto_evaluate_occupation_population_models.py` | プロト本体 (Model F2 実装含む) |

---

## 12. 改訂履歴

| 日付 | 改訂内容 | 作成者 |
|------|---------|-------|
| 2026-05-04 | 初版作成 (estimate_grade A- 本実装計画) | Worker B2 |
