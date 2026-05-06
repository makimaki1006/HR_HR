# SURVEY MARKET INTELLIGENCE - Phase 3+ : Living Cost Proxy & Recruiting Scores

**作成日**: 2026-05-04
**対象**: 本日完了モード追加フェーズ (Round 1 - 4 並列実行)
**前提**: Phase 3 Step 5 完了済 (HEAD `9a4f219`、47 push 済)
**ステータス**: 手順書 (DB 書き込みなし、Turso 接続なし)

---

## 0. 結論

本日追加分の概要:

- **2 新規テーブル** を新設
  - `municipality_living_cost_proxy` (basis=`reference` のみ)
  - `municipality_recruiting_scores` (basis=`resident`、本日版)
- **ローカル + Turso 投入** を 1 回ずつ完結
- **Rust DTO + fetch + HTML render 統合** を Round 2/3 で接続
- **E2E 10/10 維持** (Round 4 Worker F が確認)
- **Hard NG 維持** (人数表示禁止 / parent_rank 主表示 / 政令市区 175 件投入)
- **Full / Public 非影響** (既存 Phase 3 Step 5 範囲外への変更なし)

---

## 1. 追加テーブル

### 1.1 `municipality_living_cost_proxy`

| 項目 | 値 |
|------|------|
| basis | `'reference'` のみ |
| data_label | `'reference'` のみ |
| 用途 | 生活コスト指数 / 最低賃金 / 地価 proxy / 給与実質感 proxy |
| source | `household_spending` + `land_price` + `min_wage` 統合 |
| 期待行数 | 1,917 (master unit + aggregate 全件、データ不足は NULL) |
| PK | (municipality_code, basis, source_year) |

**カラム** (Worker A docs と一致):

```
municipality_code        TEXT NOT NULL
prefecture               TEXT NOT NULL
municipality_name        TEXT NOT NULL
basis                    TEXT NOT NULL DEFAULT 'reference'
cost_index               REAL                    -- 都道府県家計調査 (市区町村差なし)
min_wage                 INTEGER                 -- 都道府県値 (市区町村差なし)
land_price_proxy         REAL                    -- 市区町村粒度 → fallback 都道府県平均
salary_real_terms_proxy  REAL                    -- 上記 3 を統合
data_label               TEXT NOT NULL DEFAULT 'reference'
source_name              TEXT NOT NULL
source_year              INTEGER NOT NULL
weight_source            TEXT
estimated_at             TEXT NOT NULL
PRIMARY KEY (municipality_code, basis, source_year)
```

カラム数: **13**

### 1.2 `municipality_recruiting_scores`

| 項目 | 値 |
|------|------|
| basis | `'resident'` (本日版)、`'workplace'` は別フェーズ |
| data_label | `'estimated_beta'` |
| 用途 | 配信優先度 / 厚み指数 / 通勤アクセス / 競合度 / 給与実質感 統合スコア |
| 期待行数 | ~20,845 (1,895 muni × 11 occ、Plan B 範囲) |
| 入力 | target_thickness + commute_flow + occupation_population + living_cost_proxy |

**カラム** (Worker B docs と一致):

```
municipality_code             TEXT NOT NULL
prefecture                    TEXT NOT NULL
municipality_name             TEXT NOT NULL
basis                         TEXT NOT NULL DEFAULT 'resident'
occupation_code               TEXT NOT NULL
occupation_name               TEXT NOT NULL
distribution_priority_score   REAL NOT NULL          -- 主指標
target_thickness_index        REAL NOT NULL          -- resident estimated_beta から派生
commute_access_score          REAL
competition_score             REAL
salary_living_score           REAL                   -- living_cost_proxy 不足時 NULL
rank_in_occupation            INTEGER
rank_percentile               REAL
distribution_priority         TEXT                   -- S/A/B/C/D
scenario_conservative_score   REAL
scenario_standard_score       REAL
scenario_aggressive_score     REAL
data_label                    TEXT NOT NULL DEFAULT 'estimated_beta'
source_name                   TEXT NOT NULL
source_year                   INTEGER NOT NULL
weight_source                 TEXT
estimate_grade                TEXT
estimated_at                  TEXT NOT NULL
PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
```

カラム数: **23**

---

## 2. data_label 運用

| data_label | basis | 表示形式 | 用途 |
|-----------|-------|---------|------|
| `measured` | workplace | 人数 OK | 15-1 census 実測 |
| `estimated_beta` | resident / workplace | 指数のみ、人数 NG | F2 推定 / recruiting_scores |
| `reference` | (n/a) | 参考統計表示 | living_cost_proxy |
| `derived` | resident | 指数のみ、人数 NG | 派生スコア |

**UI 側ラベル**:

- `measured`: 「実測 (国勢調査 R2)」緑バッジ
- `estimated_beta`: 「検証済み推定 β」黄バッジ
- `reference`: 「参考統計」グレーバッジ
- `derived`: 「派生指数」青バッジ

**3 ラベル混在画面ルール**: `measured` / `estimated_beta` / `reference` が同一画面に出る場合、必ず凡例を画面下部に添える。

---

## 3. 欠損時ルール

### 3.1 `living_cost_proxy`

| カラム | NULL 条件 | UI 挙動 |
|--------|----------|---------|
| `cost_index` | 都道府県家計調査未取得時 | 「-」表示 |
| `min_wage` | 取得失敗時のみ | 「-」表示 (基本は全 47 都道府県取得) |
| `land_price_proxy` | 市区町村粒度なし時 | 都道府県平均で fallback |
| `salary_real_terms_proxy` | 上記いずれかが NULL | 「-」表示、再計算なし |

### 3.2 `recruiting_scores`

| カラム | NULL 条件 | UI 挙動 |
|--------|----------|---------|
| `distribution_priority_score` | NOT NULL (必ず計算) | 主指標として常に表示 |
| `salary_living_score` | living_cost_proxy 不足時 | 加重平均で除外、他 3 項目で再配分 |
| `target_thickness_index` | NOT NULL | resident estimated_beta から派生、必ず存在 |
| `commute_access_score` | commute_flow 接続なし時 | 「-」表示 |
| `competition_score` | occupation_population 0 時 | 「-」表示 |

**ゼロ埋め禁止**: NULL は NULL のまま、UI 側で「-」または「データなし」表示。

---

## 4. update 手順

### 4.1 ローカル DB

```bash
# Round 1: データ生成 (Worker A / Worker B 並列)
python scripts/build_municipality_living_cost_proxy.py --apply
python scripts/build_municipality_living_cost_proxy.py --verify

python scripts/build_municipality_recruiting_scores.py --apply
python scripts/build_municipality_recruiting_scores.py --verify

# 検証
sqlite3 data/hellowork.db "SELECT COUNT(*) FROM municipality_living_cost_proxy;"
# 期待: 1917

sqlite3 data/hellowork.db "SELECT COUNT(*) FROM municipality_recruiting_scores;"
# 期待: ~20845
```

### 4.2 Turso upload

```powershell
# env 設定済前提 (TURSO_EXTERNAL_URL / TOKEN)
python scripts/upload_phase3_step5.py --check-remote

python scripts/upload_phase3_step5.py --upload --yes `
    --tables municipality_living_cost_proxy municipality_recruiting_scores `
    --strategy replace --max-writes 30000

python scripts/upload_phase3_step5.py --verify
```

**writes 想定**: ~22,762 行 (= 1,917 + 20,845)、`--max-writes 30000` で安全側。

---

## 5. rollback 手順

```sql
-- ローカル
DELETE FROM municipality_living_cost_proxy WHERE source_year = 2024;
DELETE FROM municipality_recruiting_scores WHERE source_year = 2024;

-- Turso (verify 後に問題判明時)
-- 同 SQL を Turso CLI / upload_phase3_step5.py --rollback で実行
```

`--rollback` は `--strategy replace` と対称。verify 後 24 時間以内に問題発覚時のみ使用。

---

## 6. UI 上の注意

- **人数表示禁止維持**: resident / derived は指数のみ。人数 (例: 「1,234 人」) を併記しない
- **parent_rank 主表示維持**: `rank_in_parent` を主表示、`national_rank` は参考補足
- **3 ラベル混在画面**: 凡例を必ず画面下部に添える (measured / estimated_beta / reference)
- **欠損 (NULL) 表示**: 「-」または「データなし」、ゼロ埋めしない
- **全国順位の濫用禁止**: 政令市区が上位独占しがち、parent 内ランク (= 都道府県内 + 政令市内) を優先

---

## 7. 商品上の制限事項 (顧客向け文言用)

| 項目 | 制限事項 |
|------|---------|
| `living_cost_proxy.cost_index` | 都道府県家計調査の市区町村継承 (市区町村差なし) |
| `living_cost_proxy.min_wage` | 都道府県値 (市区町村差なし) |
| `recruiting_scores` 全般 | **採用市場の相対濃淡** であり、絶対人数ではない |
| `salary_living_score` 不在時 | `distribution_priority_score` の重みが他 3 項目に再配分される |
| 政令市区の resident 推定 | 175 件投入済 (横浜 18 / 大阪 24 / 川崎 7 / 福岡 7 等) |
| basis | 本日版は `resident` のみ。`workplace` 版は別フェーズ |

**営業ツール用文言例**:

> 「本指標は採用市場の相対濃淡を可視化したものであり、絶対的な人数や最終的な採用成果を保証するものではありません。住所地ベース (resident) で集計しており、勤務地ベース (workplace) とは異なる場合があります。」

---

## 8. 検証 SQL (確認用、Worker F が使用)

```sql
-- 1. 両テーブル投入完了
SELECT 'living_cost' AS tbl, COUNT(*) FROM municipality_living_cost_proxy
UNION ALL
SELECT 'scores', COUNT(*) FROM municipality_recruiting_scores;
-- 期待: living_cost 1917, scores ~20845

-- 2. designated_ward カバレッジ (recruiting_scores)
SELECT COUNT(*) FROM municipality_recruiting_scores mrs
JOIN municipality_code_master mcm ON mrs.municipality_code = mcm.municipality_code
WHERE mcm.area_type='designated_ward' AND mrs.basis='resident';
-- 期待: 175 × 11 = 1,925

-- 3. distribution_priority 分布
SELECT distribution_priority, COUNT(*) FROM municipality_recruiting_scores
GROUP BY distribution_priority ORDER BY distribution_priority;
-- 期待: S 約 2%, A 約 5%, B 約 15%, C 約 50%, D 残り

-- 4. salary_living_score NULL 率
SELECT
    COUNT(*) AS total,
    SUM(CASE WHEN salary_living_score IS NULL THEN 1 ELSE 0 END) AS null_count
FROM municipality_recruiting_scores;
-- 期待 NULL 率: < 30%
```

検証 SQL 件数: **4 件** (両テーブル件数 / designated_ward カバレッジ / 配信優先度分布 / NULL 率)。

---

## 9. 関連 docs

| ドキュメント | 役割 |
|-------------|------|
| `SURVEY_MARKET_INTELLIGENCE_METRICS.md` | 減衰式 / 指標定義 |
| `SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md` | DDL 全体 (Plan B) |
| `SURVEY_MARKET_INTELLIGENCE_PHASE3_DISPLAY_SPEC_PLAN_B.md` | 表示仕様 (Plan B) |
| `SURVEY_MARKET_INTELLIGENCE_PHASE3_RUST_INTEGRATION_PLAN.md` | Rust DTO / fetch / render |

---

## 付録: Round 構成

| Round | Worker | 役割 |
|-------|--------|------|
| 1 | A | `municipality_living_cost_proxy` 生成 (並列) |
| 1 | B | `municipality_recruiting_scores` 生成 (並列) |
| 1 | G | 本 docs 雛形 + 運用手順 (並列、本ファイル) |
| 2 | C | Rust DTO + fetch 接続 |
| 2 | E | HTML render + UI 統合 |
| 3 | D | Turso upload + verify |
| 4 | F | E2E 10/10 + 最終 push |
