# Phase 3 Step A: `commute_flow_summary` Turso V2 upload 手順書

作成日: 2026-05-03 (初版、擬似コード前提)
最終更新: 2026-05-04 (JIS 版実装完了反映)
対象: `commute_flow_summary` (Phase 3 Step 5 前提テーブル 4 件のうちの 1 件)

関連: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md` §2.2 / §3 (Step A)

## ⚠️ 2026-05-04 重要更新: JIS 版に完全置換済

本書は当初「擬似コード版 (`prefecture:municipality_name`)」前提で書かれていたが、
JIS 整備完了 (commit `e889683`) により、ローカル `commute_flow_summary` は
**JIS 5 桁 code 版** に完全置換された。本書の §2 (暫定キー警告) と §5 (改修案コード例) は
擬似版時代の記述として残置するが、**現状の運用は JIS 版** である。

最新 JIS 版の検証結果 + Turso upload 手順統合版は次の新規 docs を参照:
- `SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_UPLOAD_GUIDE_JIS.md` (Worker B 作成)

主要な変更点:
- 行数: 27,879 (擬似版と同数、形式のみ JIS に置換)
- `municipality_code` 形式: `01100` (5 桁 JIS) ← `北海道:札幌市` (擬似)
- `estimation_method`: `commute_od_top20_all_occupation_jis` ← `commute_od_top20_all_occupation`
- master 突合: 0 未登録 (`municipality_code_master` 1,917 行で完備)
- e-Stat 特殊コード `99998` (外国) / `99999` (不詳) は SELECT で除外

---

## 1. 前提と現状

### 1.1 入力 (ローカル)

| 項目 | 値 |
|------|----|
| DB | `data/hellowork.db` |
| テーブル | `commute_flow_summary` |
| 行数 | **27,879 行** |
| 派生スクリプト | `scripts/build_commute_flow_summary.py` (Step A で作成・実行済) |
| 派生元 | `v2_external_commute_od` (Turso V2 既投入済 / 83,402 行) |
| 検証 | 10 項目 pass (件数 / PK 重複 0 / rank ∈ [1,20] / flow_share ∈ [0,1] 等) |

### 1.2 出力先 (Turso V2)

| 項目 | 値 |
|------|----|
| ホスト | `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` |
| 環境変数 | `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN` (`.env` 参照) |
| 検証スクリプト | `scripts/verify_turso_v2_sync.py` |

### 1.3 Step 5 の 4 前提テーブルにおける位置付け

| # | テーブル | Turso 状態 | 本書対象 |
|---|---------|----------|:------:|
| A | `commute_flow_summary` | ❌ 不在 (本書で投入) | ✅ |
| B | `municipality_recruiting_scores` | ❌ 不在 | ❌ (本書範囲外) |
| C | `municipality_living_cost_proxy` | ❌ 不在 | ❌ (本書範囲外) |
| D | `municipality_occupation_population` | ❌ 不在 | ❌ (本書範囲外) |

→ 本書は **Step A のみ**。残り 3 テーブルは別書で扱う。

### 1.4 重要原則

**⚠️ Claude/AI は Turso 書き込みを行わない** (MEMORY: 2026-01-06 $195 課金事故防止 / `feedback_turso_upload_once.md`)。
本書は **ユーザー手動実行用の手順書**。`upload_to_turso.py` の改修も **コード例提示のみ** で、Claude はコミットしない。

---

## 2. 🔴 暫定キー警告 (必読)

### 2.1 `municipality_code` の実体

`commute_flow_summary` の `destination_municipality_code` および `origin_municipality_code` は、Step A の派生スクリプト (`scripts/build_commute_flow_summary.py`) で以下のように生成された **擬似キー**:

```
municipality_code = prefecture + ':' + municipality_name
例:
  destination_municipality_code = "北海道:札幌市"
  origin_municipality_code      = "北海道:札幌市北区"
```

実機サンプル (cp932 文字化けなしで再構成):

```
('北海道:札幌市', '北海道', '札幌市', '北海道:札幌市北区', '北海道', '札幌市北区', 'all', '全職業', 127892, 0.136649, NULL, NULL, NULL, NULL, 'commute_od_top20_all_occupation', '2026-05-04T03:53:41+00:00', 1, 2020, ...)
```

### 2.2 制約 (これに反する使い方をしないこと)

| 観点 | 結論 |
|------|------|
| **JIS 5 桁市区町村コードか?** | ❌ 違う (`"北海道:札幌市"` は文字列の擬似キー) |
| **`municipality_recruiting_scores.municipality_code` と JOIN 可能か?** | ❌ 不可 (recruiting_scores は JIS 5 桁を採用予定) |
| **`municipality_occupation_population.municipality_code` と JOIN 可能か?** | ❌ 不可 (同上) |
| **同一 prefecture × municipality_name で内部一致取得可能か?** | ✅ 可 (本テーブル内でのみ自己完結) |
| **prefecture + municipality_name で他テーブルと **名称一致** JOIN 可能か?** | ⚠️ 可 (ただし表記揺れに注意。例: 「青森」vs 「青森県」) |

### 2.3 将来の置換計画

JIS 5 桁マスタ (`municipality_geocode` 等) が Turso に投入された後、以下で置換予定:

```sql
-- 例: 将来の置換クエリ (現時点で実行しない)
UPDATE commute_flow_summary AS c
SET destination_municipality_code = (
  SELECT m.jis_code FROM municipality_geocode m
  WHERE m.prefecture = c.destination_prefecture
    AND m.municipality_name = c.destination_municipality_name
);
-- origin 側も同様
```

→ 置換完了後、`recruiting_scores` / `occupation_population` との JOIN が解禁される。

### 2.4 Step 5 (本実装) での扱い

本テーブルは **Step 5 の通勤流入元セクション専用データ** として扱う:

| Step 5 セクション | 本テーブル使用可否 |
|------------------|:------:|
| 通勤流入元 TOP N (流入元名称・件数・shareの表示) | ✅ |
| Sankey 流入元描画 (origin_prefecture / origin_municipality_name で描画) | ✅ |
| 配信地域ランキング (recruiting_scores との JOIN 必要) | ❌ Step C/B 投入後 |
| 母集団レンジ (occupation_population との JOIN 必要) | ❌ Step D 投入後 |
| 賃金/物価補正 (living_cost_proxy 必要) | ❌ Step C 投入後 |

---

## 3. 対象テーブル詳細 (実機 SELECT で確認)

### 3.1 スキーマ (`data/hellowork.db` 実機より)

```sql
CREATE TABLE commute_flow_summary (
    destination_municipality_code TEXT NOT NULL,
    destination_prefecture TEXT NOT NULL,
    destination_municipality_name TEXT NOT NULL,
    origin_municipality_code TEXT NOT NULL,
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    occupation_group_name TEXT NOT NULL DEFAULT '全職業',
    flow_count INTEGER NOT NULL DEFAULT 0,
    flow_share REAL,                          -- 0.0〜1.0 の比率
    target_origin_population INTEGER,         -- Step A では NULL (Step D 投入後に再計算)
    estimated_target_flow_conservative INTEGER,
    estimated_target_flow_standard INTEGER,
    estimated_target_flow_aggressive INTEGER,
    estimation_method TEXT,                   -- 例: 'commute_od_top20_all_occupation'
    estimated_at TEXT,                        -- 例: '2026-05-04T03:53:41+00:00'
    rank_to_destination INTEGER NOT NULL,     -- 1..20
    source_year INTEGER NOT NULL,             -- 例: 2020
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        destination_municipality_code,
        origin_municipality_code,
        occupation_group_code,
        source_year
    )
)
```

DDL は `docs/survey_market_intelligence_phase0_2_schema.sql:70-96` と同一。

### 3.2 統計値 (実機 SELECT 結果)

| 指標 | 値 |
|------|---|
| 行数 | 27,879 |
| `destination_municipality_code` 種別数 | 1,894 |
| `origin_municipality_code` 種別数 | 1,871 |
| `rank_to_destination` 範囲 | 1 〜 20 |
| `flow_share` 範囲 | 0.000216 〜 1.000000 |
| `flow_count` 範囲 | 10 〜 419,736 |
| `source_year` 種別 | 2020 のみ |
| `occupation_group_code` 種別 | `'all'` のみ |
| `estimation_method` 種別 | `'commute_od_top20_all_occupation'` のみ |
| `target_origin_population` NOT NULL 行数 | 0 (Step A では未計算、Step D 投入後に補完予定) |
| PK 重複 | 0 (`COUNT(*) - COUNT(DISTINCT ...)` = 0) |

### 3.3 サンプル (上位 3 行)

```
('北海道:札幌市', '北海道', '札幌市',
 '北海道:札幌市北区', '北海道', '札幌市北区',
 'all', '全職業',
 127892, 0.136649,
 NULL, NULL, NULL, NULL,
 'commute_od_top20_all_occupation', '2026-05-04T03:53:41+00:00',
 1, 2020, '2026-05-04 03:53:42')
```

---

## 4. Upload 前確認 SQL

### 4.1 ローカル件数・統計確認

```sql
-- 行数
SELECT COUNT(*) FROM commute_flow_summary;
-- 期待: 27,879

-- distinct 数
SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary;
-- 期待: 1,894
SELECT COUNT(DISTINCT origin_municipality_code) FROM commute_flow_summary;
-- 期待: 1,871

-- rank と flow_share の範囲
SELECT MIN(rank_to_destination), MAX(rank_to_destination) FROM commute_flow_summary;
-- 期待: 1, 20
SELECT MIN(flow_share), MAX(flow_share) FROM commute_flow_summary;
-- 期待: 0.000216, 1.000000

-- ヘッダー混入チェック
SELECT COUNT(*) FROM commute_flow_summary
  WHERE destination_municipality_code IS NULL
     OR destination_prefecture IS NULL
     OR destination_municipality_name IS NULL
     OR origin_municipality_code IS NULL
     OR origin_prefecture IS NULL
     OR origin_municipality_name IS NULL;
-- 期待: 0

-- PK 重複なし
SELECT COUNT(*) - COUNT(DISTINCT
  destination_municipality_code || '|' || origin_municipality_code || '|' ||
  occupation_group_code || '|' || source_year
) FROM commute_flow_summary;
-- 期待: 0

-- サンプル先頭 5 件 (Turso 投入後の比較用)
SELECT * FROM commute_flow_summary
ORDER BY destination_municipality_code, origin_municipality_code,
         occupation_group_code, source_year
LIMIT 5;
```

### 4.2 Turso 側不在確認

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a

python scripts/verify_turso_v2_sync.py
# → commute_flow_summary が REMOTE_MISSING であることを確認
```

---

## 5. `scripts/upload_to_turso.py` 改修案 (コード例 — Claude はコミットしない)

### 5.1 現状 (確認済)

`scripts/upload_to_turso.py` の `TABLES` 配列 (line 25-46) と `TABLE_SCHEMAS` 辞書には `commute_flow_summary` は **未登録**。

### 5.2 改修方針 (2 案)

| 案 | 説明 | リスク | 推奨 |
|---|------|------|:---:|
| 案 1: TABLES を 1 件に絞る一時改修 | `TABLES = ["commute_flow_summary"]` に編集 → 実行 → 元に戻す | 既存 20 テーブルに触れない (低リスク) | ✅ |
| 案 2: 既存 TABLES に追加 | 末尾に `"commute_flow_summary"` を追加 | 既存 20 テーブルも DROP+CREATE+INSERT で再投入 (Turso WRITE 浪費) | △ |

→ **案 1 推奨**。Step A 単独投入なら案 1 で WRITE を最小化。

### 5.3 改修コード例 (案 1)

`scripts/upload_to_turso.py` の `TABLES` を一時的に以下に置換:

```python
# 一時変更 (Step A 投入のため、commute_flow_summary のみに絞る)
TABLES = [
    "commute_flow_summary",
]
# 投入完了後、元の 20 テーブル (v2_external_population 〜 ts_agg_tracking) に戻すこと
```

`TABLE_SCHEMAS` 辞書に追加 (`docs/survey_market_intelligence_phase0_2_schema.sql:70-96` と同一):

```python
TABLE_SCHEMAS = {
    # ... 既存 20 テーブル定義 (削除しない、コメントアウト不要) ...

    # ↓ 新規追加
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
            PRIMARY KEY (
                destination_municipality_code,
                origin_municipality_code,
                occupation_group_code,
                source_year
            )
        )
    """,
}
```

### 5.4 INDEX について

`schema.sql:98-106` には以下の INDEX 定義あり:

```sql
CREATE INDEX IF NOT EXISTS idx_commute_dest_rank
ON commute_flow_summary (
    destination_municipality_code,
    occupation_group_code,
    rank_to_destination
);

CREATE INDEX IF NOT EXISTS idx_commute_origin
ON commute_flow_summary (origin_municipality_code);
```

`upload_to_turso.py` の現行ロジックは TABLE のみ作成し INDEX は作成しない。**INDEX 追加が必要な場合は、upload 完了後に Turso CLI で別途実行** (本書範囲外)。Step 5 初期は PK だけで十分高速 (27,879 行)。

---

## 6. Upload 実行手順 (ユーザー手動)

### 6.1 一連の実行

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# 1. 環境変数読み込み
set -a && source .env && set +a

# 2. dry-run で件数とテーブル数を確認
python scripts/upload_to_turso.py --dry-run
# → 出力例:
#   commute_flow_summary: 27879行 (dry-run)

# 3. 本番実行 (TABLES = ["commute_flow_summary"] に絞った状態で)
python scripts/upload_to_turso.py
# → DROP TABLE IF EXISTS commute_flow_summary
#   CREATE TABLE commute_flow_summary (...)
#   INSERT 27,879 行 (BATCH_SIZE 200 / 約 140 batch)
```

### 6.2 所要時間見積

| ステップ | 所要時間 |
|---------|--------|
| 認証 + 接続 | 1 秒 |
| 既存 Turso データ DROP (1 テーブル、不在なら no-op) | 数秒 |
| CREATE TABLE (1 テーブル) | 数秒 |
| INSERT 27,879 行 (BATCH_SIZE 200, 約 140 batch) | 約 4〜7 分 (HTTP RTT 1〜2 秒/batch) |
| **合計** | **約 5 分** |

タイムアウト対策: `requests` の `timeout=120` (`turso_pipeline()` 内)。

### 6.3 Turso WRITE 消費見積

| 項目 | row writes |
|------|----------:|
| INSERT 27,879 行 | 27,879 |
| DROP + CREATE (1 テーブル) | 2 |
| **合計** | **約 27,881 row writes** |

Turso Hobby Plan (25M/月) における消費比率: **0.11%**。

---

## 7. Upload 後確認 SQL

### 7.1 verify_turso_v2_sync.py 再実行 (必須)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a
python scripts/verify_turso_v2_sync.py
# → commute_flow_summary が MATCH (または SAMPLE_MISMATCH 型表現差のみ) に変わっていることを確認
```

`SAMPLE_MISMATCH` が出ても、原因が「型表現差 (例: REAL vs FLOAT)」なら許容。データ不一致なら Step 6 (ロールバック) を検討。

### 7.2 Turso 側 個別 SQL 確認

Turso CLI または `verify_turso_v2_sync.py` の応用で以下を実行:

```sql
-- 件数 (期待: 27,879)
SELECT COUNT(*) FROM commute_flow_summary;

-- distinct 数
SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary;
-- 期待: 1,894
SELECT COUNT(DISTINCT origin_municipality_code) FROM commute_flow_summary;
-- 期待: 1,871

-- サンプル先頭 5 件 (ローカル §4.1 と一致確認)
SELECT * FROM commute_flow_summary
ORDER BY destination_municipality_code, origin_municipality_code,
         occupation_group_code, source_year
LIMIT 5;

-- PK 重複なし (期待: 0)
SELECT COUNT(*) - COUNT(DISTINCT
  destination_municipality_code || '|' || origin_municipality_code || '|' ||
  occupation_group_code || '|' || source_year
) FROM commute_flow_summary;

-- 日本語表示正常性
SELECT DISTINCT destination_prefecture FROM commute_flow_summary LIMIT 10;
-- 期待: '北海道', '青森県', ... (47 都道府県のうち上位 10)

-- rank 範囲チェック (期待: 1, 20)
SELECT MIN(rank_to_destination), MAX(rank_to_destination) FROM commute_flow_summary;

-- flow_share 範囲チェック (期待: > 0, ≤ 1.0)
SELECT MIN(flow_share), MAX(flow_share) FROM commute_flow_summary;
-- 期待: 0.000216, 1.000000

-- 暫定キー形式確認 (期待: ':' を含む全行)
SELECT COUNT(*) FROM commute_flow_summary
  WHERE destination_municipality_code NOT LIKE '%:%'
     OR origin_municipality_code NOT LIKE '%:%';
-- 期待: 0 (全行が prefecture:municipality_name 形式)
```

---

## 8. ロールバック方針

### 8.1 原則: 即時 DELETE しない

既存 `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_REMOTE_MISSING_UPLOAD.md §6` と同じ:

- MEMORY「Claude による DB 書き込み禁止」+ 「Turso 書込み制限」の二重ガード
- ローカル DB に問題なし (Turso のみ書き込み発生)
- 既存 Phase 0〜2 機能は `table_exists` フェイルセーフ済 → クラッシュなし
- 誤データが含まれていても、Step 5 未着手なら影響範囲ゼロ

### 8.2 標準対応: 新テーブル名で並行運用 (推奨)

```sql
-- 例: commute_flow_summary_v2 として再投入
CREATE TABLE commute_flow_summary_v2 (...);
-- ローカル正本から再投入
-- Step 5 着手時に Rust 側参照を v2 に切り替え
-- 旧テーブル削除はユーザー手動 + 動作確認後
```

利点: ローリングバック可、Step 5 サービス影響最小。

### 8.3 上書き再投入 (DROP + CREATE)

`upload_to_turso.py` は `DROP TABLE IF EXISTS` → `CREATE TABLE` → `INSERT` の流れのため、ローカル DB を修正してから再実行すれば誤データは上書きされる。

```bash
# 修正後のローカル DB で再実行
python scripts/upload_to_turso.py
```

または Turso CLI で:

```sql
DROP TABLE commute_flow_summary;
-- → upload_to_turso.py を再実行
```

**Claude では DROP/再投入を実行しない**。ユーザー手動。

---

## 9. Phase 3 Step 5 への影響

### 9.1 本書完了で解除される前提条件

| Step 5 前提テーブル | 本書完了前 | 本書完了後 |
|------|:---:|:---:|
| A. `commute_flow_summary` | ❌ 不在 | ✅ 投入 |
| B. `municipality_recruiting_scores` | ❌ 不在 | ❌ 不在 |
| C. `municipality_living_cost_proxy` | ❌ 不在 | ❌ 不在 |
| D. `municipality_occupation_population` | ❌ 不在 | ❌ 不在 |

**4 件中 1 件解除** → 残り B / C / D は別書 (各テーブル単位の手順書) で扱う。

### 9.2 Step 5 単独着手可能なセクション

| Step 5 セクション | commute_flow_summary 単独で動作可? |
|------------------|:---:|
| 通勤流入元 TOP N (流入元名称・件数・share) | ✅ |
| Sankey 流入元描画 (origin_prefecture / municipality_name) | ✅ |
| 配信地域ランキング (`recruiting_scores` JOIN 必要) | ❌ Step C/B 投入後 |
| 母集団レンジ (`occupation_population` JOIN 必要) | ❌ Step D 投入後 |
| 賃金/物価補正 (`living_cost_proxy` JOIN 必要) | ❌ Step C 投入後 |

→ 本書完了後、Step 5 で「**通勤流入元セクションのみ**」を実データ化する部分着手は可能。
ただし `MEMORY: feedback_hypothesis_driven.md` のとおり、部分機能だけ実装してもユーザー価値は限定的なため、**B/C/D も並行投入してから Step 5 を一括着手することを推奨**。

### 9.3 暫定キー由来の制約 (再掲)

Step 5 で本テーブルを参照する Rust ハンドラを実装する際:

- `municipality_code` で他テーブル (`recruiting_scores` / `occupation_population`) と JOIN しない
- 名称表示用途 (`destination_municipality_name`, `origin_municipality_name`) のみ使用
- 数値計算 (TOP N、share) は本テーブル内で完結させる

JIS 5 桁マスタ投入後の置換は §2.3 参照。

---

## 10. 禁止事項

| 項目 | 状態 |
|------|------|
| Claude/AI による Turso 書き込み | ❌ 完全禁止 |
| Claude/AI による `upload_to_turso.py` 実行 | ❌ 禁止 (本書はユーザー手動実行用) |
| Claude/AI による `scripts/upload_to_turso.py` の TABLES / TABLE_SCHEMAS 改修コミット | ❌ 禁止 (改修案を docs として提示するのみ) |
| 他 3 テーブル (`recruiting_scores` / `living_cost_proxy` / `occupation_population`) への着手 | ❌ 本書範囲外 |
| `municipality_code` を使った他テーブルとの JOIN クエリ実装 | ❌ JIS マスタ投入まで禁止 |
| token 表示 (本書、ログ、レポート) | ❌ 禁止 |
| push (本コミット含む) | ❌ 禁止 (ユーザーが集約 commit) |

---

## 11. 完了条件チェックリスト

本書 (Step A 手順書) は **手順書作成のみ** で完了。

- [x] 入力 (ローカル) と出力 (Turso V2) が明記されている
- [x] 暫定キー警告 (`prefecture:municipality_name` 形式、JIS 5 桁ではない、JOIN 不可) が §2 に記載
- [x] 対象テーブルのスキーマ・統計値・サンプルが実機 SELECT で確認済 (§3)
- [x] upload 前確認 SQL が記載されている (§4)
- [x] `upload_to_turso.py` 改修案 (TABLES 1 件に絞る方式 + TABLE_SCHEMAS DDL) がコード例として記載 (§5)
- [x] upload 実行手順 (dry-run → 本番、所要時間 5 分、WRITE 27,881) が記載 (§6)
- [x] upload 後確認 SQL (`verify_turso_v2_sync.py` + 個別 SQL) が記載 (§7)
- [x] ロールバック方針 (DROP しない原則 + 新テーブル並行運用) が記載 (§8)
- [x] Step 5 への影響 (1/4 解除、通勤流入元セクションのみ単独着手可、暫定キー制約) が記載 (§9)
- [x] 禁止事項一覧 (§10)

ユーザーが本書に従って upload を実行した後、Step 5 の通勤流入元セクション部分着手が可能。
ただし B / C / D テーブルも投入してから Step 5 一括着手することを推奨。
