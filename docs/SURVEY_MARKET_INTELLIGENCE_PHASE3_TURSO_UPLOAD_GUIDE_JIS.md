# Phase 3 JIS 版: Turso V2 upload 統合手順書

作成日: 2026-05-04
対象: ローカル `data/hellowork.db` の Phase 3 JIS 整備済 3 テーブルを Turso V2 に upload

**Claude/AI は本書の作成のみ。実 upload はユーザー手動** (MEMORY: 2026-01-06 $195 課金事故防止 + Turso 書込制限)。

---

## 1. Upload 対象 3 テーブル

| # | テーブル | 行数 | 由来 | 既存 Turso 状態 |
|--:|---------|----:|------|----------------|
| 1 | `v2_external_commute_od_with_codes` | **86,762** | `fetch_commute_od.py --with-codes` (commit `b8bde10`/`fd91071`) | 不在 (新規 upload) |
| 2 | `municipality_code_master` | **1,917** | `build_municipality_code_master.py` (commit `9f667bc`) | 不在 (新規 upload) |
| 3 | `commute_flow_summary` (JIS 版) | **27,879** | `build_commute_flow_summary.py` (commit `e889683`) | 不在 (擬似版すら未投入) |
| **合計** | | **116,558 行** | | |

### 1.1 v2_external_commute_od (base) の扱い

| 観点 | 既存 Turso | ローカル | 判断 |
|------|:---------:|:--------:|------|
| 行数 | 83,402 | 86,762 (+3,360) | 差分あり |
| 内容 | 旧 fetch | `--with-codes` で再 fetch (INSERT OR REPLACE) | 微差 (e-Stat 最新値) |
| 再 upload 要否 | **保留** (本書範囲外) | - | ユーザー判断 |

**v2_external_commute_od の再 upload は本書では実施しない**。差分が大きい場合は別 commit + 別検証で対応。本書 3 テーブルは新規 upload (REMOTE_MISSING → MATCH) のみ。

---

## 2. Turso WRITE 消費見積

| 項目 | 値 |
|------|---:|
| `with_codes` row writes | 86,762 |
| `master` row writes | 1,917 |
| `cflow_summary` row writes | 27,879 |
| DROP + CREATE × 3 | 6 |
| **合計 row writes** | **約 116,564** |
| Turso Hobby Plan 上限 (25M/月) 比 | **約 0.47%** |

→ 1 回の実行で完了させる (MEMORY: 2026-04-03 浪費事故防止)。

---

## 3. Upload 前確認 SQL

### 3.1 ローカル件数

```sql
SELECT 'with_codes',  COUNT(*) FROM v2_external_commute_od_with_codes  -- 期待 86,762
UNION ALL
SELECT 'master',      COUNT(*) FROM municipality_code_master            -- 期待 1,917
UNION ALL
SELECT 'cflow_jis',   COUNT(*) FROM commute_flow_summary;               -- 期待 27,879
```

### 3.2 整合性確認

```sql
-- with_codes の JIS 5 桁完全性
SELECT COUNT(*) FROM v2_external_commute_od_with_codes
WHERE LENGTH(origin_municipality_code) != 5
   OR LENGTH(dest_municipality_code) != 5;
-- 期待: 0

-- master の area_type 分布 (期待: aggregate_city 20, special_ward 23, designated_ward 175, ...)
SELECT area_type, COUNT(*) FROM municipality_code_master GROUP BY area_type;

-- cflow_summary の master 突合
SELECT COUNT(*) FROM commute_flow_summary AS cfs
LEFT JOIN municipality_code_master AS dst ON dst.municipality_code = cfs.destination_municipality_code
LEFT JOIN municipality_code_master AS org ON org.municipality_code = cfs.origin_municipality_code
WHERE dst.municipality_code IS NULL OR org.municipality_code IS NULL;
-- 期待: 0
```

### 3.3 Turso 不在確認

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a
python scripts/verify_turso_v2_sync.py --dry-run
```

3 テーブルが Turso 側で REMOTE_MISSING であることを確認。

---

## 4. Upload 手順 (ユーザー手動)

### 4.1 `upload_to_turso.py` 改修案 (Claude が docs として提示、適用はユーザー)

#### `TABLES` 配列に追加 (一時改修案)

```python
TABLES = [
    "v2_external_commute_od_with_codes",  # 新規
    "municipality_code_master",           # 新規
    "commute_flow_summary",                # 新規 (JIS 版)
]
```

または永続化版 `TABLES_FULL` の末尾に追加。

#### `TABLE_SCHEMAS` 辞書に追加

```python
TABLE_SCHEMAS = {
    # ... 既存 ...

    # === 2026-05-04 Phase 3 JIS 整備 ===

    "v2_external_commute_od_with_codes": """
        CREATE TABLE IF NOT EXISTS v2_external_commute_od_with_codes (
            origin_municipality_code TEXT NOT NULL,
            dest_municipality_code TEXT NOT NULL,
            origin_prefecture TEXT NOT NULL,
            origin_municipality_name TEXT NOT NULL,
            dest_prefecture TEXT NOT NULL,
            dest_municipality_name TEXT NOT NULL,
            total_commuters INTEGER NOT NULL,
            male_commuters INTEGER DEFAULT 0,
            female_commuters INTEGER DEFAULT 0,
            reference_year INTEGER DEFAULT 2020,
            source TEXT NOT NULL DEFAULT 'estat_0003454527',
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (origin_municipality_code, dest_municipality_code, reference_year)
        )
    """,

    "municipality_code_master": """
        CREATE TABLE IF NOT EXISTS municipality_code_master (
            municipality_code TEXT PRIMARY KEY,
            prefecture TEXT NOT NULL,
            municipality_name TEXT NOT NULL,
            pref_code TEXT NOT NULL,
            area_type TEXT NOT NULL,
            area_level TEXT NOT NULL,
            is_special_ward INTEGER NOT NULL DEFAULT 0,
            is_designated_ward INTEGER NOT NULL DEFAULT 0,
            parent_code TEXT,
            source TEXT NOT NULL DEFAULT 'estat_commute_od',
            source_year INTEGER NOT NULL DEFAULT 2020,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP
        )
    """,

    "commute_flow_summary": """
        CREATE TABLE IF NOT EXISTS commute_flow_summary (
            destination_municipality_code TEXT NOT NULL,
            destination_prefecture TEXT NOT NULL,
            destination_municipality_name TEXT NOT NULL,
            origin_municipality_code TEXT NOT NULL,
            origin_prefecture TEXT NOT NULL,
            origin_municipality_name TEXT NOT NULL,
            occupation_group_code TEXT NOT NULL DEFAULT 'all',
            occupation_group_name TEXT NOT NULL DEFAULT '全職業',
            flow_count INTEGER NOT NULL DEFAULT 0,
            flow_share REAL,
            target_origin_population INTEGER,
            estimated_target_flow_conservative INTEGER,
            estimated_target_flow_standard INTEGER,
            estimated_target_flow_aggressive INTEGER,
            estimation_method TEXT,
            estimated_at TEXT,
            rank_to_destination INTEGER NOT NULL,
            source_year INTEGER NOT NULL,
            created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
            PRIMARY KEY (destination_municipality_code, origin_municipality_code,
                         occupation_group_code, source_year)
        )
    """,
}
```

注意: 一時改修中は CHECK 制約 (master の `area_type IN (...)`) は **省略推奨** (Turso pipeline API の互換性問題予防)。ローカル DDL では CHECK あり、Turso 側は CHECK なしでデータだけ転送。

### 4.2 実行手順

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# 事前バックアップ (任意)
Copy-Item scripts\upload_to_turso.py scripts\upload_to_turso.py.bak -Force

# 環境変数読み込み
set -a && source .env && set +a   # Bash
# または PowerShell:
# Get-Content .env | ForEach-Object { if ($_ -match '^([^=]+)=(.*)$') { [Environment]::SetEnvironmentVariable($matches[1], $matches[2], 'Process') } }

# dry-run で 3 テーブル × 期待行数を確認
python scripts/upload_to_turso.py --dry-run
# 期待:
#   v2_external_commute_od_with_codes: 86762 行 (dry-run)
#   municipality_code_master: 1917 行 (dry-run)
#   commute_flow_summary: 27879 行 (dry-run)

# 本番 upload
python scripts/upload_to_turso.py
```

### 4.3 所要時間見積

| テーブル | row writes | BATCH 200 | 推定時間 |
|---------|----------:|---------:|---------|
| `with_codes` | 86,762 | 434 batch | 約 8〜15 分 |
| `master` | 1,917 | 10 batch | 約 30 秒 |
| `cflow_summary` | 27,879 | 140 batch | 約 5 分 |
| **合計** | **116,558** | **584 batch** | **約 15〜25 分** |

---

## 5. Upload 後検証

### 5.1 verify_turso_v2_sync.py で MATCH 確認

```bash
python scripts/verify_turso_v2_sync.py
# → 3 テーブルすべて MATCH (or 型差のみ SAMPLE_MISMATCH) を確認
```

期待: REMOTE_MISSING → MATCH/SAMPLE_MISMATCH に変化。

### 5.2 Turso 個別 SQL 確認 (Turso CLI または verify スクリプト応用)

```sql
-- 1. 行数一致
SELECT 'with_codes',  COUNT(*) FROM v2_external_commute_od_with_codes
UNION ALL SELECT 'master',     COUNT(*) FROM municipality_code_master
UNION ALL SELECT 'cflow_jis',  COUNT(*) FROM commute_flow_summary;

-- 2. master 突合 (Turso 側でも整合性)
SELECT COUNT(*) FROM commute_flow_summary AS cfs
LEFT JOIN municipality_code_master AS dst ON dst.municipality_code = cfs.destination_municipality_code
WHERE dst.municipality_code IS NULL;
-- 期待: 0

-- 3. 重要コードサンプル
SELECT * FROM municipality_code_master
WHERE municipality_code IN ('13100', '13101', '01100', '01101', '14130', '40130');

-- 4. cflow の主要 destination
SELECT origin_municipality_name, flow_count
FROM commute_flow_summary
WHERE destination_municipality_code = '13101'  -- 千代田区への流入元 TOP 5
ORDER BY rank_to_destination LIMIT 5;
```

---

## 6. Rollback 方針

### 6.1 原則: 即時 DELETE しない

MEMORY「Claude による DB 書き込み禁止」+ Turso 書込制限。

### 6.2 シナリオ別対応

| シナリオ | 対応 |
|---------|------|
| 完全失敗 (HTTP エラー中断、部分データ) | `upload_to_turso.py` 再実行 (DROP + CREATE で先頭から再投入) |
| 品質問題発覚 (e.g. master 不整合) | ローカルで再生成 (`build_municipality_code_master.py`) → 再 upload |
| 型違反 (CHECK 制約等) | Turso 側 DDL から CHECK 除外 → 再 upload |
| 重大なロールバック | DROP TABLE ... → Turso CLI でユーザー手動 (Claude/AI 不可) |

### 6.3 v2_external_commute_od の base 側との不整合発覚時

| 状況 | 対応 |
|------|------|
| Turso `v2_external_commute_od` (83,402 行) と `_with_codes` (86,762 行) で件数差 | 名称マッピング照合を Turso 側で実施 (READ-only) |
| base が古い (e-Stat 旧値) で問題が出る場合 | 別 commit で `v2_external_commute_od` 再 upload を計画 (本書範囲外) |

---

## 7. Phase 3 Step 5 への影響

| 機能 | 反映状態 |
|------|:------:|
| Rust ハンドラ `fetch_commute_flow_summary` | Turso 反映後に自動的に JIS 版を読む (`query_turso_or_local` で Turso 優先) |
| `municipality_code_master` を使った JOIN | Step 5 の他 3 テーブル投入時に活用可能 |
| 配信地域ランキング HTML セクション | placeholder のまま (`municipality_recruiting_scores` 未投入のため) |
| 通勤流入元 HTML セクション | **JIS 版で実データ表示可能** (本 upload 完了後) |

---

## 8. 禁止事項

| 項目 | 状態 |
|------|:---:|
| AI による Turso 書き込み | ❌ 禁止 |
| AI による `upload_to_turso.py` 改修コミット | ❌ 禁止 (本書はコード例提示のみ) |
| `v2_external_commute_od` (base) の再 upload 判断 | ❌ 本書範囲外 |
| token 表示 | ❌ 禁止 |
| push | ❌ 禁止 |
| Rust 実装 | ❌ |

---

## 9. 完了条件

- [ ] ステップ 4.2 dry-run で 3 テーブル × 期待行数を確認
- [ ] 本番 `upload_to_turso.py` 完走 (約 15〜25 分)
- [ ] verify_turso_v2_sync.py で 3 テーブル MATCH 確認
- [ ] Turso 個別 SQL で master 突合 0、重要コード存在確認
- [ ] `commute_flow_summary` JIS 版 destination 1,894 確認
- [ ] Rust ハンドラから Turso 経由で JIS 版が読めること (Phase 3 Step 3 通勤セクション)

完了後、Phase 3 Step 5 の前提 1/4 が **JIS 版で完全解除**。残り 3 テーブル
(`municipality_recruiting_scores`、`municipality_living_cost_proxy`、`municipality_occupation_population`)
の投入で Phase 3 Step 5 着手可能。

---

## 10. 関連 docs

- 旧 (擬似版手順、廃止予定): `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md`
- JIS 整備全体計画: `SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md`
- Worker A 改修: `SURVEY_MARKET_INTELLIGENCE_PHASE3_FETCH_COMMUTE_OD_REFACTOR.md`
- Worker B master DDL: `SURVEY_MARKET_INTELLIGENCE_PHASE3_MUNICIPALITY_CODE_MASTER.md`
- Worker C JIS 化: `SURVEY_MARKET_INTELLIGENCE_PHASE3_BUILD_COMMUTE_FLOW_JIS_MIGRATION.md`
- Step 5 全体前提: `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md`
