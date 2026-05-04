# Phase 3 Step 5: テーブル名再検討 (人数 → 指数/ランク) 設計決定書

**作成日**: 2026-05-04
**作成者**: Worker A2 (System Architect)
**ステータス**: 設計案 (実装承認待ち)
**前提**: Model F2 で grade A- 達成 → 本実装承認、ただし **指数/ランク限定 (人数推定禁止)**

---

## 0. 結論 (Executive Summary)

| 項目 | 決定 |
|------|------|
| **推奨テーブル名** | **`v2_municipality_target_thickness`** |
| **PRIMARY KEY** | `(municipality_code, basis, occupation_code, source_year)` |
| **列数** | 16 列 (旧 12 列) |
| **期待行数** | **約 39,600 行** (旧設計の 約 1/45) |
| **移行方針** | **(a) 旧テーブル完全廃止 → 新テーブル置換** |
| **影響ファイル** | docs 14 件 + Rust 2 ファイル + Python 1 ファイル + DDL 1 ファイル = **18 ファイル / 57 occurrences** |

---

## 1. テーブル名候補と評価

### 1.1 命名要件

| 要件 | 説明 |
|------|------|
| **R1. 人数を連想させない** | 「population」「count」「人口」等を含めない。誤解防止が本タスクの主目的 |
| **R2. 商品名・営業文脈で使いやすい** | 既存ドキュメントで確立済の用語「ターゲット厚み指数」と一致させる |
| **R3. V2 命名規則との整合** | V2 全体は `v2_*` プレフィックス採用 (`v2_external_*` 26 件、`v2_flow_*` 13 件、`v2_salesnow_*` 等)。新規テーブルは V2 規則に揃える |
| **R4. 既存 31 テーブルとの衝突回避** | `verify_turso_v2_sync` レポート (2026-05-04) の Turso V2 既存テーブル一覧と照合 |
| **R5. 内容の自己説明性** | 「市区町村 × 職業」の指数だと名前から読み取れる |

### 1.2 候補一覧と評価マトリクス

| # | 候補名 | R1 人数連想なし | R2 商品文脈 | R3 V2 規則 | R4 衝突回避 | R5 自己説明 | 総合 |
|---|--------|:-:|:-:|:-:|:-:|:-:|:--:|
| 1 | `municipality_target_thickness_scores` | ◎ | ◎ | △ (`v2_*` なし) | ◎ | ○ | △ |
| 2 | `municipality_occupation_thickness_index` | ◎ | ○ | △ (`v2_*` なし) | ◎ | ◎ | △ |
| 3 | `municipality_recruitment_thickness_index` | ◎ | ◎ | △ (`v2_*` なし) | ◎ | △ (採用一辺倒) | △ |
| 4 | `municipality_target_density_scores` | △ (density=密度=人数連想あり) | ○ | △ | ◎ | ○ | △ |
| 5 | `municipality_workforce_thickness_index` | △ (workforce=労働力人口連想) | △ | △ | ◎ | ○ | △ |
| 6 | **`v2_municipality_target_thickness`** | ◎ | ◎ | ◎ | ◎ | ◎ | **◎** |
| 7 | `v2_target_thickness_index` | ◎ | ◎ | ◎ | ◎ | △ (粒度が見えない) | ○ |
| 8 | `v2_municipality_occupation_thickness_index` | ◎ | ○ | ◎ | ◎ | ◎ (やや冗長) | ○ |

### 1.3 候補絞り込みプロセス

#### 候補 1〜3 (旧 prefix 方式) の問題点

- 既存 V2 命名規則 (`v2_external_*`, `v2_flow_*`, `v2_salesnow_*`, `v2_compensation_*`, `v2_monopsony_*` 等 31 件) と整合せず、孤立した命名空間になる
- 旧 Phase 0-2 schema が `v2_*` 抜きで設計された経緯はあるが、当時はまだ 14 件規模 → 現在は 31 件に拡大しており、**V2 全体の規則を `v2_*` で統一するのが妥当**

#### 候補 4 (density)、5 (workforce) の却下理由

| 候補 | 却下理由 |
|------|---------|
| `density_scores` | 「density」は人口密度を強く連想させ R1 違反。「採用ターゲットの密度」と読まれても、結局「人数あたり」に解釈が引き戻される |
| `workforce_thickness` | 「workforce」は「労働力人口」と直訳されるため R1 違反。Model F2 が **就業者推定値** を出している誤認を維持してしまう |

#### 候補 6 (推奨) vs 候補 7、8

| 候補 | 採用判定 | 理由 |
|------|:-------:|------|
| `v2_municipality_target_thickness` | **採用** ✅ | (a) 営業資料で確立済の「ターゲット厚み指数」と一致 (b) `_index` を省略しても列名 `thickness_index` で内容が判別可能 (c) 31 文字、SQLite/Turso 制限無問題 |
| `v2_target_thickness_index` | 不採用 | 粒度 (市区町村 vs 都道府県 vs 全国) がテーブル名から読めない。将来 prefectural 版を作る場合 conflict |
| `v2_municipality_occupation_thickness_index` | 不採用 | 41 文字でやや長い。`_occupation_` を省略しても職業コード列で表現可能なため冗長 |

### 1.4 既存 31 テーブルとの衝突確認

`turso_v2_sync_report_2026-05-04.md` のリモート全テーブル + ローカル全テーブル一覧から、`v2_municipality_target_thickness` が **既存テーブルと衝突しないこと** を確認済:

- 既存 `v2_*` テーブル: `v2_external_*`, `v2_flow_*`, `v2_posting_*`, `v2_salesnow_*`, `v2_compensation_*`, `v2_keyword_*`, `v2_text_*`, `v2_monopsony_*`, `v2_spatial_*`, `v2_salary_*`, `v2_shadow_*`, `v2_anomaly_*`, `v2_cascade_*`, `v2_employer_*`, `v2_fulfillment_*`, `v2_mobility_*`, `v2_region_*`, `v2_regional_*`, `v2_transparency_*`, `v2_vacancy_*`, `v2_industry_mapping`, `v2_commute_flow_summary`, `v2_cross_industry_competition`
- 既存 `municipality_*` テーブル (V2 prefix なし): `municipality_geocode`, `municipality_living_cost_proxy`, `municipality_recruiting_scores`, `commute_flow_summary` (旧 Phase 0-2 schema 由来)
- **`v2_municipality_*` プレフィックスは新規** → 衝突なし ✅

---

## 2. 新スキーマ DDL 設計

### 2.1 主テーブル

```sql
CREATE TABLE IF NOT EXISTS v2_municipality_target_thickness (
    -- 結合キー
    municipality_code   TEXT    NOT NULL,
    prefecture          TEXT    NOT NULL,
    municipality_name   TEXT    NOT NULL,
    basis               TEXT    NOT NULL,            -- 'resident' / 'workplace'
    occupation_code     TEXT    NOT NULL,            -- '01_管理' .. '11_運搬清掃'
    occupation_name     TEXT    NOT NULL,

    -- 指数とランク (人数の代替、商品メッセージ準拠)
    thickness_index             REAL    NOT NULL,    -- 0-200, 100 = 全国平均
    rank_in_occupation          INTEGER,             -- 全国順位 (1〜N)
    rank_percentile             REAL,                -- 上位 % (例 0.05 = 上位 5%)
    distribution_priority       TEXT,                -- 'S' / 'A' / 'B' / 'C' / 'D'

    -- 採用シナリオ濃淡 (turnover 1/3/5% を指数化、人数ではない)
    scenario_conservative_index INTEGER,             -- 0-200 正規化
    scenario_standard_index     INTEGER,             -- 0-200 正規化
    scenario_aggressive_index   INTEGER,             -- 0-200 正規化

    -- メタデータ
    estimate_grade              TEXT,                -- 'A-' (本ラウンド初期値)
    weight_source               TEXT    NOT NULL,    -- 'hypothesis_v1' / 'estat_R2_0003450543'
    is_industrial_anchor        INTEGER NOT NULL DEFAULT 0,  -- 0/1 (Model F2 anchor 判定)
    source_year                 INTEGER NOT NULL,
    estimated_at                TEXT    NOT NULL,    -- ISO8601 timestamp
    created_at                  TEXT    NOT NULL DEFAULT CURRENT_TIMESTAMP,

    PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
);

-- 商品 UI で最頻出: 職業ごとに厚み指数降順で TOP N を引く
CREATE INDEX IF NOT EXISTS idx_v2mtt_thickness
ON v2_municipality_target_thickness (occupation_code, thickness_index DESC);

-- 配信優先度マップ用: 職業ごとに全国順位で引く
CREATE INDEX IF NOT EXISTS idx_v2mtt_rank
ON v2_municipality_target_thickness (occupation_code, rank_in_occupation);

-- 都道府県絞り込み + 職業横断ビュー用
CREATE INDEX IF NOT EXISTS idx_v2mtt_pref_occ
ON v2_municipality_target_thickness (prefecture, occupation_code);

-- anchor city フィルタ (製造業 deep dive 用)
CREATE INDEX IF NOT EXISTS idx_v2mtt_anchor
ON v2_municipality_target_thickness (is_industrial_anchor, occupation_code, thickness_index DESC);
```

### 2.2 期待行数試算

| 項目 | 旧設計 (人数) | 新設計 (指数) | 比率 |
|------|------------:|------------:|-----:|
| 市区町村数 | 1,800 | 1,800 | 1× |
| 職業大分類 | 21 (詳細分類) | 11 (大分類) | 0.52× |
| 年齢階級 | 8 | **削除** | 0× |
| 性別 | 3 | **削除** | 0× |
| basis | 2 | 2 | 1× |
| **合計行数** | **約 1,814,400** | **約 39,600** | **約 1/45** |

**根拠**:
- 1,800 × 11 × 2 = 39,600 行
- Turso 無料枠 (5GB / 1B reads/月) に対して **無視できる規模** (旧設計の 1.8M 行は read コスト懸念あり)

---

## 3. 既存 DDL との差分整理

| カラム / 項目 | 旧 (人数) | 新 (指数) | 影響 |
|---------------|-----------|-----------|------|
| **テーブル名** | `municipality_occupation_population` | `v2_municipality_target_thickness` | rename + V2 prefix 追加 |
| `municipality_code` | あり | あり | 維持 |
| `prefecture` | あり | あり | 維持 |
| `municipality_name` | あり | あり | 維持 |
| `basis` | あり | あり | 維持 (resident/workplace) |
| `occupation_code` | あり | あり | 維持 |
| `occupation_name` | あり | あり | 維持 |
| `age_group` | あり | **削除** | F2 が年齢別を出さないため |
| `gender` | あり | **削除** | 同上 |
| `population` | INTEGER | **削除** | 人数表示禁止方針 |
| `source_name` | TEXT | **削除 (weight_source に統合)** | 重複情報整理 |
| `thickness_index` | なし | **追加** REAL NOT NULL | 0-200, 100=全国平均 |
| `rank_in_occupation` | なし | **追加** INTEGER | 全国順位 |
| `rank_percentile` | なし | **追加** REAL | 上位 % |
| `distribution_priority` | なし | **追加** TEXT | S/A/B/C/D |
| `scenario_conservative_index` | なし | **追加** INTEGER | turnover 1% シナリオ指数 |
| `scenario_standard_index` | なし | **追加** INTEGER | turnover 3% シナリオ指数 |
| `scenario_aggressive_index` | なし | **追加** INTEGER | turnover 5% シナリオ指数 |
| `estimate_grade` | なし | **追加** TEXT | グローバル grade ('A-' 等) |
| `weight_source` | なし | **追加** TEXT NOT NULL | 'hypothesis_v1' / 'estat_*' |
| `is_industrial_anchor` | なし | **追加** INTEGER 0/1 | F2 anchor 判定 |
| `estimated_at` | なし | **追加** TEXT | ISO8601 timestamp |
| `created_at` | あり | あり (維持) | DEFAULT CURRENT_TIMESTAMP |
| **PRIMARY KEY** | (muni, basis, occ, age, gender, year) | **(muni, basis, occ, year)** | age/gender 削除に伴う簡素化 |
| **INDEX** | 2 個 (basis 結合, occ × age × gender) | 4 個 (thickness 降順, rank 昇順, pref × occ, anchor) | UI ユースケース最適化 |

### 3.1 旧 `recruiting_scores` との関係整理

`municipality_recruiting_scores` (Phase 0-2 schema:109) は別テーブルで、**そちらにも `scenario_*_population` 列が存在** (line 128-130)。これは `media_area_performance_future` 等で使う「採用シナリオ × 母集団」の集計系で、本タスクでは **変更しない** (別ラウンドの設計に委ねる)。本タスクは `municipality_occupation_population` 1 テーブルのみ rename 対象。

---

## 4. 移行方針

### 4.1 選択肢比較

| 方針 | 内容 | メリット | デメリット | 推奨 |
|------|------|---------|-----------|:----:|
| **(a)** 完全廃止 → 置換 | 旧テーブル DDL を削除し、新テーブルのみ作成 | 命名空間がクリーン、人数誤認のリスクゼロ、列を削れるため Turso ストレージ最小 | 将来 e-Stat 実測値置換時に新規 DDL が必要 | ✅ |
| **(b)** 並列追加 | 旧 DDL 残置 + 新テーブル並列作成 | 将来 e-Stat 実測置換ですぐ復活可 | 命名空間が散らかり、未投入の旧テーブルが残置されると混乱の元、Rust 側で DTO が二重化 | ❌ |
| **(c)** ALTER + 拡張 | `ALTER TABLE RENAME` + 列追加/削除 | 既存 DDL 文を最小修正で済ませる | SQLite は `ALTER TABLE DROP COLUMN` を制限的サポート (3.35+ 必要)、Turso 互換性に懸念、PRIMARY KEY 変更は実質テーブル再作成と同等のため (a) と差がない | ❌ |

### 4.2 推奨: 方針 (a) 完全廃止 → 置換

**理由**:
1. **本ラウンド時点で旧テーブルへの本番データ投入実績ゼロ** → 廃止コストが極小 (Phase 0-2 schema で DDL 提案されただけで、実際のデータは未投入。`turso_v2_sync_report_2026-05-04.md` でも `municipality_occupation_population` は LOCAL/REMOTE 両方未存在)
2. 旧 DDL は **人数前提で設計されていた** ため、列の半分以上を削除する `ALTER` は実質テーブル再作成。それなら `CREATE TABLE` 1 本のほうが保守的
3. 将来 e-Stat 実測値置換時は **別テーブル** (例: `v2_municipality_occupation_population_actual`) として再開が可能。指数テーブルと混在させるよりも責務分離が明確
4. Rust 側 (`market_intelligence.rs`) は DTO 構造体ごと書き換える必要があるため、テーブル名と一緒に新規実装するほうが整合性高い

### 4.3 移行手順 (実装ラウンドで実施)

1. `docs/survey_market_intelligence_phase0_2_schema.sql` の `municipality_occupation_population` 関連 DDL (line 14-41) を削除し、`v2_municipality_target_thickness` 用 DDL に置換
2. `src/handlers/analysis/fetch/market_intelligence.rs` の DTO・SELECT 文を新スキーマに書き換え
3. `src/handlers/survey/report_html/market_intelligence.rs` の表示ロジックを「人数 → 指数」に書き換え
4. `scripts/build_municipality_occupation_population.py` (新規) ではなく `scripts/build_v2_municipality_target_thickness.py` (新規) として Model F2 結果を生成
5. 既存 docs (下記 5 章リスト) の旧テーブル名参照を一括 sed 置換 + 内容文脈に応じた書き換え

---

## 5. 影響を受ける既存ファイル一覧

### 5.1 サマリ

| 区分 | ファイル数 | occurrence 数 |
|------|-------:|------------:|
| Rust ソース | 2 | 7 |
| Python スクリプト | 1 | 1 |
| DDL ファイル | 1 | 3 |
| docs (Markdown) | 14 | 46 |
| **合計** | **18** | **57** |

### 5.2 詳細リスト (grep 結果より)

#### Rust ソース (2 files / 7 occ.)

| ファイル | 行 | コンテキスト |
|---------|---:|-------------|
| `src/handlers/survey/report_html/market_intelligence.rs` | 293 | エラーメッセージ "職業×年齢×性別人口データが未投入です (municipality_occupation_population テーブル)。" → 新メッセージへ |
| `src/handlers/analysis/fetch/market_intelligence.rs` | 12 | doc コメント (テーブル一覧) |
| 同上 | 219 | `fetch_occupation_population` の doc コメント |
| 同上 | 237 | `table_exists(db, "municipality_occupation_population")` |
| 同上 | 268 | `FROM municipality_occupation_population` |
| 同上 | 273 | `query_turso_or_local` の table 名引数 |
| 同上 | 592 | DTO セクション見出しコメント |

**追加修正項目** (line 番号は概算):
- DTO 構造体名 (例 `OccupationPopulationRow` → `MunicipalityTargetThicknessRow`)
- `scenario_conservative_population` 等 6 箇所の列名 (line 75-77, 376-378, 406-408, 419-420 等) → `*_index` へ
- SELECT 列リスト (population/age_group/gender 削除、thickness_index/rank_*/distribution_priority/scenario_*_index 追加)

#### Python スクリプト (1 file / 1 occ.)

| ファイル | 行 | コンテキスト |
|---------|---:|-------------|
| `scripts/proto_evaluate_occupation_population_models.py` | 3 | docstring 内のテーブル名言及 (プロトタイプ評価スクリプトのため、本実装時は別スクリプト `build_v2_municipality_target_thickness.py` を新規作成し、こちらは旧名のまま履歴保存推奨) |

#### DDL ファイル (1 file / 3 occ.)

| ファイル | 行 | コンテキスト |
|---------|---:|-------------|
| `docs/survey_market_intelligence_phase0_2_schema.sql` | 14, 38, 41 | CREATE TABLE 本体 + INDEX 2 本 |

#### Docs Markdown (14 files / 46 occ.)

| ファイル | occ. | 主な参照箇所 (行番号) | 推奨対応 |
|---------|---:|-----------------------|---------|
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE0_2_PREP.md` | 3 | 137 (生データ説明), 212, 220 | 新名へ rename + 「人数 → 指数」説明文書き換え |
| `docs/SURVEY_MARKET_INTELLIGENCE_METRICS.md` | 1 | 109 (`対象職業人口 = municipality_occupation_population`) | 「対象職業厚み指数 = v2_municipality_target_thickness」に書き換え (人数定義式自体を見直し) |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md` | 3 | 56, 92, 493 (テーブル不在表) | 表内のテーブル名のみ rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md` | 7 | 19, 31, 56 (DDL ブロック), 149, 180, 186, 199 | DDL ブロック全体差し替え + フロー図 rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_PROTO_EVALUATION.md` | 5 | 1 (タイトル), 171, 1040, 1145 | タイトル + Turso 投入手順を新名へ |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_MODEL_V2.md` | 11 | 1 (タイトル), 319-330 (ALTER 文), 391, 423, 429, 433, 533 | **タイトル含むファイル全体の前提が「人数推定」 → 大幅書き換え or 新ファイル作成 (`*_THICKNESS_MODEL_V2.md`) を推奨** |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_FEASIBILITY.md` | 9 | 1, 4, 26 (DDL), 115, 123, 124, 131, 164, 174 | feasibility 結論は維持、DDL/出力先の名前のみ rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_INDUSTRY_WEIGHT_HYPOTHESIS.md` | 3 | 11, 121, 195 | F3 ロジック説明内のテーブル名のみ rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_INDUSTRY_DATA_AUDIT.md` | 1 | 6 | 用途説明の rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md` | 1 | 16 (Step 5 関連テーブル列挙) | 列挙内のテーブル名 rename |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md` | 1 | 14 | 同上 |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_GUIDE_JIS.md` | 1 | 310 | アップロード対象テーブル列挙の rename |

### 5.3 旧名前で書かれている docs の更新リスト (推奨アクション別)

| 更新タイプ | 対象 docs | 理由 |
|-----------|---------|------|
| **A. テーブル名のみ rename (sed 一括可)** | `STEP_A_COMMUTE_FLOW_UPLOAD.md`, `MUNICIPALITY_CODE_MASTER.md`, `JIS_CODE_PLAN.md`, `TURSO_UPLOAD_GUIDE_JIS.md`, `INDUSTRY_DATA_AUDIT.md`, `OCCUPATION_INDUSTRY_WEIGHT_HYPOTHESIS.md` | 列挙・参照のみで内容文脈は人数前提に依存しない |
| **B. 部分書き換え (内容文脈見直し)** | `PHASE0_2_PREP.md`, `METRICS.md`, `STEP5_PREREQ_INGEST_PLAN.md`, `OCCUPATION_PROTO_EVALUATION.md`, `OCCUPATION_POPULATION_FEASIBILITY.md` | 「人数」「就業者」前提の説明文を「指数」「厚み」に書き換え必要 |
| **C. ファイル新設 + 旧ファイル archive** | `OCCUPATION_POPULATION_MODEL_V2.md` | タイトルから「Model V2 = 人数推定」を前提とした構成。新規 `PHASE3_TARGET_THICKNESS_MODEL.md` を作成し、旧ファイルは `archive/` に移動推奨 |
| **D. 修正不要 (履歴保存)** | `proto_evaluate_occupation_population_models.py` (スクリプト) | プロトタイプ評価の履歴記録としてそのまま保持。本実装は別スクリプト |

---

## 6. リスク・トレードオフ

| リスク | 影響度 | 緩和策 |
|--------|:------:|--------|
| **Rust DTO 構造体名衝突** | 🟡 中 | 新名 `MunicipalityTargetThicknessRow` に統一、旧 `OccupationPopulationRow` は完全削除 |
| **既存 docs 参照先が古い名前のまま** | 🟡 中 | 本書を起点に、実装ラウンドで一括 rename PR を出す。grep 件数 (57) を CI で監視 |
| **e-Stat 実測値投入の将来パスが断たれる** | 🟢 低 | 将来は別テーブル `v2_municipality_occupation_population_actual` 等で並列追加可能。本タスクの指数テーブルは廃止せず、両者を JOIN して使う設計に拡張可 |
| **「ターゲット厚み指数」という商品用語が変更される可能性** | 🟢 低 | 列名 `thickness_index` は内部実装、表示は別途 i18n で管理可。テーブル名は商品名と直結する形にしておく方が将来の翻訳・marketing 変更時に追従しやすい |
| **Turso `v2_*` プレフィックスへの将来統一作業** | 🟢 低 | 本タスクで `v2_*` を採用することで、将来旧 `municipality_*` 系 4 テーブルを順次 V2 化する流れと整合 |

---

## 7. 次アクション (本書承認後)

1. ユーザー承認 (本書のレビュー)
2. **実装ラウンド A**: `docs/survey_market_intelligence_phase0_2_schema.sql` の DDL 差し替え (Worker B 担当想定)
3. **実装ラウンド B**: Rust 側 fetch/DTO/HTML 書き換え (Worker C 担当想定)
4. **実装ラウンド C**: `scripts/build_v2_municipality_target_thickness.py` 新規作成 (Model F2 結果を CSV 出力)
5. **実装ラウンド D**: docs 一括 rename + Type B/C ファイル書き換え
6. **実装ラウンド E**: ローカル DB 投入 → 検証 → Turso V2 投入 (1 回のみ、無料枠リセット後)

**注意**: 本書時点で **DB 書き込みは一切行わない**。設計提案のみ。

---

## 8. 関連文書

| 文書 | 内容 |
|------|------|
| `docs/survey_market_intelligence_phase0_2_schema.sql` | 旧 DDL (本タスクでは変更しない) |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_PROTO_EVALUATION.md` | Model F2 grade A- 達成の根拠 |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md` | Step 5 全体の投入計画 |
| `docs/turso_v2_sync_report_2026-05-04.md` | 既存 31 テーブルの一覧 (衝突確認に使用) |
| `scripts/proto_evaluate_occupation_population_models.py` | Model F2 のプロトタイプ実装 (履歴保存) |
