# Phase 3 F2 モデルの位置づけ整理 (案 B 並列保管 体制下)

**作成日**: 2026-05-04
**作成者**: Worker D4
**前提**: Worker D3 検証により e-Stat 15-1 (sid=0003454508) で `basis='workplace'` 実測が市区町村×職業×年齢×性別の粒度で取得可能と判明。これを受け **案 B (並列保管)** を採用。

---

## 0. 結論サマリ

F2 モデルは **廃止しない**。15-1 実測と並立させた上で、以下 3 用途で継続活用する。

1. **resident 側推定** (15-1 では未提供のため F2 が主役)
2. **target_thickness 補正** (本社過剰補正・industrial_anchor 判定)
3. **クロス検証** (15-1 実測との対比で F2 精度を事後評価)

---

## 1. F2 の 3 用途定義

### 1.1 用途①: resident 側推定 (常住地別職業構成)

**背景**: 15-1 は workplace (従業地・通学地) ベースのみ。常住地ベースの市区町村×職業の粒度データは現時点で公開統計に存在しない (Worker D3 で表 12 を検討したが年齢軸欠落で代替不可と確定)。

**役割**: 常住地ベース推定の**主役**として F2 を継続。

| 項目 | 内容 |
|------|------|
| 入力 | 既存 (人口/年齢/性別/昼夜間/通勤OD/産業構成 F3/重み hypothesis_v1/SalesNow F6) |
| 出力 | 推定指数 `estimate_index` (0-100、人数なし) |
| DB 格納 | `municipality_occupation_population` (basis='resident', data_label='estimated_beta') |
| weight_source | 'hypothesis_v1' → 将来 'estat_R2_xxx' で置換予定 |
| source_name | 'model_f2_v1' |

### 1.2 用途②: target_thickness 補正

`v2_municipality_target_thickness` (Worker A2 設計) の計算ロジックは F2 由来の補正群:

| ロジック | 内容 |
|---------|------|
| F3 | 産業構成補正 (経済センサス R3) |
| F4 | 昼夜間人口比補正 |
| F5 | 通勤流入補正 |
| F6 | 本社過剰補正 (SalesNow) |
| industrial_anchor | 4 条件 AND 判定 |

**役割**: 15-1 実測テーブルと**並立**して残す。15-1 を分母 (基礎データ) とし、F2 補正で「採用配信に効く」厚み指数化。

将来検討: 15-1 実測値ベースの補正版 (Model G) も視野。ただし当面は F2 補正を維持。

### 1.3 用途③: クロス検証 (15-1 ↔ F2 整合性)

15-1 (workplace 実測) と F2 (workplace 推定) を市区町村別に比較し、F2 精度を事後評価。

```python
# Phase 5+ で実装予定 (本書では設計のみ)
for muni in municipalities:
    for occ in occupations:
        actual    = census_15_1[muni, occ, basis='workplace']     # 実測人数
        estimated = model_f2[muni, occ, basis='workplace']         # F2 推定指数
        rmse           = sqrt(((actual_rank - estimated_rank) ** 2).mean())
        rank_corr      = spearman(actual_rank, estimated_rank)
```

**期待値**:
- Spearman 相関 > 0.85 (順位は概ね一致するはず)
- 都市別 RMSE: 大都市ほど絶対値で大きく、中規模都市は小さい傾向

**得られる打ち手**:
- F2 高精度領域 → resident 側でも信頼可能と判断
- F2 乖離領域 → 重みマスタの実測値置換 (estat_R2_xxx) を優先

---

## 2. F2 を残すべき理由 (廃止しない論拠)

| # | 理由 | 詳細 |
|---|------|------|
| 1 | 15-1 は workplace のみ | resident 別の市区町村×職業データは公開されていないため F2 で代替必須 |
| 2 | F3+重み+補正は独立価値 | 15-1 がない地域 (海外・将来年次・新設市町村) でも F2 でフォールバック可能 |
| 3 | target_thickness の主役 | 採用厚み指数化は 15-1 基礎 + F2 補正の派生集約。補正部は依然 F2 |
| 4 | クロス検証の品質保証 | 15-1 を ground truth として F2 精度を逆算でき、重み更新優先度に反映 |
| 5 | sensitivity 0.964 検証済 | Worker A3 でロバスト性確認済。投資した分析資産を活かす |
| 6 | 投資コスト > 維持コスト | E〜F2 で 7 commits + 8 docs 投入済。廃止コストの方が高い |

(計 6 件)

---

## 3. 旧 docs への追記文案 (推奨)

本書 Worker D4 では旧 docs を**直接編集しない**。以下は次ラウンドでユーザー判断のための推奨文案。

### 3.1 `OCCUPATION_POPULATION_MODEL_V2.md` § 0 結論部 への追記

```markdown
**Phase 3+ アップデート (2026-05-04):**
e-Stat 15-1 (sid=0003454508) で workplace 実測が取得可能と判明したため、
案 B (並列保管) を採用。F2 は廃止せず、以下の役割で継続:

1. resident 側推定 (常住地別職業構成、15-1 では未提供)
2. target_thickness 補正 (本社過剰補正・anchor 判定)
3. クロス検証 (15-1 実測との対比で F2 精度を事後評価)

詳細: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_ROLE_AFTER_15_1.md
```

### 3.2 `OCCUPATION_PROTO_EVALUATION.md` § 0.7 (新設) への追記

```markdown
## 0.7 評価結果の取り扱い (2026-05-04 アップデート)

本評価で採択した F2 モデルは、15-1 実測投入後も廃止せず、
resident 側推定・target_thickness 補正・クロス検証の 3 用途で継続使用する。
詳細: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_ROLE_AFTER_15_1.md
```

### 3.3 `OCCUPATION_POPULATION_FEASIBILITY.md` (旧 v1 案 A) 冒頭への追記

```markdown
> **注意 (2026-05-04)**: 本書は v1 案 A の歴史的記録。
> 現行方針は案 B (並列保管) で、F2 の役割は
> docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_ROLE_AFTER_15_1.md を参照。
```

(計 3 件の追記文案)

---

## 4. クロス検証フロー (Phase 5+ 実装予定)

```
[Worker A4 plan] 15-1 実測投入
     ↓
     INSERT into municipality_occupation_population
       (basis='workplace', data_label='measured', source_name='census_15_1')
     ↓
[F2 workplace 推定計算]
     ↓
     INSERT into municipality_occupation_population
       (basis='workplace', data_label='estimated_beta', source_name='model_f2_v1')
     ↓
[整合性スクリプト: scripts/cross_validate_f2_vs_15_1.py]
     ↓
     - 同一 (muni, occupation) で actual vs estimated_index を結合
     - Spearman 順位相関、RMSE (順位ベース)、都市規模別バイアス
     ↓
[結果出力: data/generated/f2_vs_15_1_cross_validation.json]
     ↓
[lessons learned]
     - 重み実測置換の優先度マップ (低相関 occupation を優先)
     - F3/F4/F5/F6 のチューニング指針 (バイアス方向に応じて)
```

**スクリプト設計骨子** (本書では設計のみ、実装は Phase 5+):

| 項目 | 内容 |
|------|------|
| 入力 1 | `municipality_occupation_population` (basis='workplace', data_label='measured') |
| 入力 2 | `municipality_occupation_population` (basis='workplace', data_label='estimated_beta') |
| 結合キー | (municipality_code, occupation_code) |
| 指標 | Spearman 相関, ランク RMSE, 都市規模ビン別バイアス |
| 出力 | JSON (per-occupation: corr, rmse, n) + Markdown サマリ |

---

## 5. 移行スコープ整理 (F2 関連ファイル)

| ファイル | 取り扱い | 備考 |
|---------|---------|------|
| `scripts/proto_evaluate_occupation_population_models.py` | 維持 | クロス検証の元ロジックを流用予定 |
| `scripts/sensitivity_anchor_thresholds.py` | 維持 | sensitivity 0.964 の検証資産 |
| `scripts/build_municipality_target_thickness.py` (skeleton) | 維持・継続実装 | Phase 4 で本実装 |
| `data/generated/occupation_industry_weight.csv` (hypothesis_v1) | 維持 | 将来 estat_R2_xxx で置換 |
| `data/generated/salesnow_aggregate_for_f6.csv` | 維持 | F6 補正で継続使用 |
| `docs/.../OCCUPATION_PROTO_EVALUATION.md` | 維持 + § 0.7 追記推奨 | 本書を参照 |
| `docs/.../OCCUPATION_POPULATION_MODEL_V2.md` | 維持 + § 0 末尾追記推奨 | 本書を参照 |
| `docs/.../OCCUPATION_POPULATION_FEASIBILITY.md` (v1 案 A) | 歴史保存タグ | 冒頭注記推奨 |

---

## 6. 用語整理 (混乱防止)

| 用語 | 定義 | 例 |
|------|------|------|
| 実測 (measured) | e-Stat 15-1 国勢調査 R2 の市区町村別人数 | population カラムに人数 |
| 推定 β (estimated_beta) | F2 モデルによる推定指数 (人数なし、0-100) | estimate_index カラム |
| 厚み指数 | target_thickness の採用配信用集約指数 | thickness_index 0-200 |
| basis=workplace | 従業地ベース (その地域で働いている人) | 15-1 実測 (推奨) / F2 推定 (fallback) |
| basis=resident | 常住地ベース (その地域に住んでいる人) | F2 推定のみ (現状) |
| weight_source | 産業 → 職業マッピング元 | 'hypothesis_v1' / 'estat_R2_xxx' |
| source_name | データ元の識別子 | 'census_15_1' / 'model_f2_v1' |
| data_label | レコード種別フラグ | 'measured' / 'estimated_beta' |

(計 8 用語)

---

## 7. 全体構成図 (案 B 並列保管 アーキテクチャ)

```
=== Phase 3 Step 5 アーキテクチャ (案B 並列保管) ===

入力:
├── e-Stat API
│   ├── 15-1 (sid=0003454508)         → census_15_1.csv (実測 workplace)
│   ├── 経済センサス R3                → industry_structure.csv (F3)
│   ├── 国勢調査 11-1, 12              → 補完候補 (将来)
│   └── 通勤 OD                        → commute_flow_summary (F5)
├── SalesNow                           → salesnow_aggregate_for_f6.csv (F6)
├── 重みマスタ (hypothesis_v1)         → occupation_industry_weight.csv
└── master                             → municipality_code_master

計算:
├── 実測ロジック (Worker A4 plan)
│   └── census_15_1.csv → INSERT (basis=workplace, data_label=measured)
└── 推定ロジック (Worker B3 skeleton + F2)
    ├── basis=resident   → estimate_index (data_label=estimated_beta) ← 主役
    └── basis=workplace  → estimate_index (data_label=estimated_beta) ← fallback

DB:
├── municipality_occupation_population (Worker B4 DDL)
│   ├── workplace × measured          (15-1 実測、人数あり)
│   ├── workplace × estimated_beta    (F2 fallback、指数のみ)
│   ├── resident  × estimated_beta    (F2 主役、指数のみ)
│   └── resident  × measured          (将来予約スロット)
└── v2_municipality_target_thickness (Worker A2 設計)
    └── 採用厚み指数 (15-1 基礎 + F2 補正の派生集約)

UI:
├── workplace measured:        人数表示 OK + 年齢/性別内訳
└── resident estimated_beta:   指数 + ランク + 濃淡 (β タグ明示)

クロス検証 (Phase 5+):
└── 15-1 vs F2 (workplace) → 精度評価 → 重み置換優先度マップ
```

---

## 8. 参照リンク

- 案 B 採用判断の根拠: Worker D3 検証結果 (15-1 で workplace×age×sex 取得可能、表 12 は年齢軸欠落)
- 実測投入計画: Worker A4 plan
- DB 並列保管 DDL: Worker B4
- 推定ロジック skeleton: Worker B3
- target_thickness 設計: Worker A2
- sensitivity 検証 (0.964): Worker A3
