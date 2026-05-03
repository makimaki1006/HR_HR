# Phase 3 前処理 (d): REMOTE_MISSING テーブル upload 手順書

作成日: 2026-05-04
対象: `v2_external_commute_od` (最優先) + `v2_external_minimum_wage` (次点)

---

## 1. 現状

`scripts/verify_turso_v2_sync.py` の検証結果 (2026-05-03):

| テーブル | local | Turso V2 | Status |
|---------|:-----:|:-------:|--------|
| `v2_external_commute_od` | ✅ 存在 (83,402 行) | ❌ 不在 | 🟡 REMOTE_MISSING |
| `v2_external_minimum_wage` | ✅ 存在 (47 行) | ❌ 不在 | 🟡 REMOTE_MISSING |

**⚠️ Claude/AI は Turso 書き込みを行わない** (MEMORY: 2026-01-06 $195 課金事故防止)。本書はユーザー手動実行用の手順書。

---

## 2. 対象テーブル詳細 (実機 SELECT で確認)

### 2.1 `v2_external_commute_od` (最優先 — Phase 3 でクリティカル)

**スキーマ**:
```sql
CREATE TABLE IF NOT EXISTS v2_external_commute_od (
    origin_pref TEXT NOT NULL,
    origin_muni TEXT NOT NULL,
    dest_pref TEXT NOT NULL,
    dest_muni TEXT NOT NULL,
    total_commuters INTEGER NOT NULL,
    male_commuters INTEGER,
    female_commuters INTEGER,
    reference_year INTEGER,
    PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
);
```

**サンプル**:
```
('青森', '青森市', '北海道', '札幌市', 27, 21, 0, 2020)
('青森', '三沢市', '北海道', '札幌市', 13, 0, 0, 2020)
('青森', '弘前市', '北海道', '札幌市', 16, 12, 0, 2020)
```

**行数**: 83,402

**Phase 3 用途** (実機 grep で確認):
- `src/handlers/analysis/fetch/subtab7_other.rs` (3 関数: 通勤流入元分析)
- `src/handlers/recruitment_diag/talent_pool_expansion.rs` (採用診断・流入元 TOP N)
- `src/handlers/jobmap/flow_handlers.rs` / `fromto.rs` (細粒度 OD 参照)

**Phase 3 影響**: 通勤流入 Sankey、流入元 TOP N、採用診断の市区町村粒度 1:1 OD すべて。**未反映なら Phase 3 で当該機能は実装保留**。

### 2.2 `v2_external_minimum_wage` (次点)

**スキーマ**:
```sql
CREATE TABLE IF NOT EXISTS v2_external_minimum_wage (
    prefecture TEXT NOT NULL,
    hourly_min_wage INTEGER NOT NULL,
    effective_date TEXT NOT NULL,
    fiscal_year INTEGER NOT NULL,
    PRIMARY KEY (prefecture)
);
```

**サンプル**:
```
('北海道', 1075, '2025-10-01', 2025)
('青森', 1029, '2025-10-01', 2025)
('岩手県', 1031, '2025-10-01', 2025)
```

**行数**: 47 (47 都道府県)

**Phase 3 用途**:
- `src/handlers/analysis/fetch/subtab5_phase4.rs` (賃金水準クエリ 2 ブランチ)
- `src/handlers/jobmap/handlers.rs` (`fetch_min_wage_for_pref`)

**Phase 3 影響**: 給与競争力スコアの最低賃金補正。**未反映でも Phase 3 初期は進行可** (table_exists チェックで空結果返却フェイルセーフ済み)。

---

## 3. Upload 前確認 SQL

### 3.1 ローカル件数・サンプル確認

```sql
-- v2_external_commute_od
SELECT COUNT(*) FROM v2_external_commute_od;                      -- 期待: 83,402
SELECT COUNT(DISTINCT origin_pref) FROM v2_external_commute_od;   -- 期待: 47 都道府県
SELECT COUNT(DISTINCT dest_pref) FROM v2_external_commute_od;     -- 期待: 47
SELECT * FROM v2_external_commute_od ORDER BY rowid LIMIT 5;
SELECT reference_year, COUNT(*) FROM v2_external_commute_od GROUP BY reference_year;

-- v2_external_minimum_wage
SELECT COUNT(*) FROM v2_external_minimum_wage;                    -- 期待: 47
SELECT * FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC;
```

### 3.2 ヘッダー混入確認 (両テーブル念のため)

```sql
-- minimum_wage
SELECT COUNT(*) FROM v2_external_minimum_wage WHERE prefecture = '都道府県';   -- 期待: 0
SELECT COUNT(*) FROM v2_external_minimum_wage WHERE prefecture IS NULL OR prefecture = '';  -- 期待: 0

-- commute_od (prefecture カラムなし、origin/dest 文字列を確認)
SELECT COUNT(*) FROM v2_external_commute_od
  WHERE origin_pref IN ('都道府県', '出発地') OR dest_pref IN ('都道府県', '到着地');  -- 期待: 0
SELECT COUNT(*) FROM v2_external_commute_od
  WHERE origin_pref IS NULL OR dest_pref IS NULL;                                       -- 期待: 0
```

### 3.3 Turso 側不在確認 (実行時点で再確認)

```bash
# scripts/verify_turso_v2_sync.py を再実行して REMOTE_MISSING のままであることを確認
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a
python scripts/verify_turso_v2_sync.py --dry-run
# → READ 試算で問題なければ本番実行
python scripts/verify_turso_v2_sync.py
# → 該当2テーブルが REMOTE_MISSING であることを確認
```

---

## 4. Upload 手順 (ユーザー手動実行)

### 4.1 使用スクリプトと改修必要性

`scripts/upload_to_turso.py` の現状 (`TABLES` 配列、`TABLE_SCHEMAS` 辞書) を確認:

| テーブル | `TABLES` 登録 | `TABLE_SCHEMAS` 登録 |
|---------|:-----------:|:-----------------:|
| `v2_external_commute_od` | ❌ 未登録 | ❌ 未登録 |
| `v2_external_minimum_wage` | ❌ 未登録 | ❌ 未登録 |

→ **`scripts/upload_to_turso.py` の改修が必要**。

### 4.2 改修案 (Claude が docs として提示、適用はユーザー)

`scripts/upload_to_turso.py` の `TABLES` 配列 (line 25-46) に追加:

```python
TABLES = [
    "v2_external_population",
    # ... 既存20テーブル ...
    "ts_agg_tracking",
    # ↓ 新規追加 (REMOTE_MISSING 解消)
    "v2_external_commute_od",
    "v2_external_minimum_wage",
]
```

`TABLE_SCHEMAS` 辞書に追加:

```python
TABLE_SCHEMAS = {
    # ... 既存20テーブル定義 ...
    # ↓ 新規追加
    "v2_external_commute_od": """
        CREATE TABLE IF NOT EXISTS v2_external_commute_od (
            origin_pref TEXT NOT NULL,
            origin_muni TEXT NOT NULL,
            dest_pref TEXT NOT NULL,
            dest_muni TEXT NOT NULL,
            total_commuters INTEGER NOT NULL,
            male_commuters INTEGER,
            female_commuters INTEGER,
            reference_year INTEGER,
            PRIMARY KEY (origin_pref, origin_muni, dest_pref, dest_muni)
        )
    """,
    "v2_external_minimum_wage": """
        CREATE TABLE IF NOT EXISTS v2_external_minimum_wage (
            prefecture TEXT NOT NULL,
            hourly_min_wage INTEGER NOT NULL,
            effective_date TEXT NOT NULL,
            fiscal_year INTEGER NOT NULL,
            PRIMARY KEY (prefecture)
        )
    """,
}
```

### 4.3 実行手順 (1 回で完了)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# 1. 環境変数読み込み
set -a && source .env && set +a

# 2. dry-run で件数とテーブル数を確認
python scripts/upload_to_turso.py --dry-run
# → 出力例:
#   v2_external_commute_od: 83402行 (dry-run)
#   v2_external_minimum_wage: 47行 (dry-run)

# 3. minimum_wage のみ先行投入 (リスク最小、47 行)
#   ※ 引数で指定する機能がない場合は、TABLES を一時的に絞ってテスト
python scripts/upload_to_turso.py
# → 全テーブル投入。本番では minimum_wage を含む全 22 テーブルを再投入

# 注意: upload_to_turso.py は対象テーブルを DROP + CREATE してから INSERT する
#  → 既存 Turso データが消える。Phase 3 着手前なら問題ないが、
#    既に Turso に投入済みのデータも全て再投入される。
#    時間と帯域に余裕がある時に実行すること。
```

### 4.4 Chunk / Batch サイズ

`scripts/upload_to_turso.py` の現状設定 (確認済):

```python
BATCH_SIZE = 200   # upload_table 内の INSERT バッチサイズ
```

- `v2_external_commute_od`: 83,402 行 / 200 = **418 batch**
- `v2_external_minimum_wage`: 47 行 / 200 = **1 batch**

合計 419 batch。1 batch = 1 HTTP pipeline (200 INSERT 含む)。Turso 側は pipeline 単位で処理。

### 4.5 Turso WRITE 消費見積

| テーブル | 行数 | row writes |
|---------|-----:|----------:|
| `v2_external_commute_od` | 83,402 | 83,402 |
| `v2_external_minimum_wage` | 47 | 47 |
| 加えて DROP + CREATE | — | 4 (テーブル別 DROP + CREATE) |
| **合計** | — | **約 83,453 row writes** |

Turso Hobby Plan の row writes 上限 (2026 時点で 25M/月) を踏まえると、無料枠の **0.33%** で完了。

ただし、`upload_to_turso.py` は **全テーブル** を再投入するため、minimum_wage / commute_od 以外の既存 20 テーブルもすべて再書き込みが発生する。MEMORY「Tursoアップロードは1回で完了」のとおり、**1 回の実行で完了させる**。

#### より控えめな手順 (推奨)

`upload_to_turso.py` の `TABLES` を一時的に新規 2 テーブルだけに絞って実行する手順:

```bash
# 一時的に TABLES を [v2_external_commute_od, v2_external_minimum_wage] のみに編集
# → 実行 → 完了後に元の TABLES に戻す
```

これで row writes は **83,449** のみに抑制可能。

### 4.6 所要時間見積

| ステップ | 所要時間 |
|---------|--------|
| 認証 + 接続 | 1 秒 |
| 既存 Turso データ DROP (2 テーブル) | 数秒 |
| CREATE TABLE (2 テーブル) | 数秒 |
| INSERT 83,449 行 (BATCH_SIZE 200, 419 batch) | 約 7〜15 分 (HTTP RTT 1〜2 秒/batch) |
| **合計** | **10〜20 分** |

タイムアウト対策: `requests` の `timeout=120` (`turso_pipeline()` 内)。

---

## 5. Upload 後確認 SQL

### 5.1 件数一致

```bash
# verify_turso_v2_sync.py で再検証 (READ 6 程度)
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a
python scripts/verify_turso_v2_sync.py
# → 該当2テーブルが MATCH (または SAMPLE_MISMATCH 型表現差のみ) になっていることを確認
```

### 5.2 個別 SQL 確認 (Turso CLI または verify_turso_v2_sync.py の応用)

Turso 側で SQL 実行:

```sql
-- 件数
SELECT COUNT(*) FROM v2_external_commute_od;        -- 期待: 83,402
SELECT COUNT(*) FROM v2_external_minimum_wage;      -- 期待: 47

-- サンプル一致 (ローカル先頭5件と比較)
SELECT * FROM v2_external_commute_od ORDER BY origin_pref, origin_muni, dest_pref, dest_muni LIMIT 5;
SELECT * FROM v2_external_minimum_wage ORDER BY prefecture;

-- 主キー重複なし (commute_od)
SELECT COUNT(*) - COUNT(DISTINCT origin_pref || '|' || origin_muni || '|' || dest_pref || '|' || dest_muni)
  FROM v2_external_commute_od;                       -- 期待: 0

-- 主キー重複なし (minimum_wage)
SELECT COUNT(*) - COUNT(DISTINCT prefecture) FROM v2_external_minimum_wage;  -- 期待: 0

-- 日本語表示正常性
SELECT DISTINCT origin_pref FROM v2_external_commute_od LIMIT 10;
-- 期待: '北海道', '青森県', '岩手県', '宮城県', '秋田県', ... (47 都道府県)
SELECT prefecture, hourly_min_wage FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC LIMIT 5;
-- 期待: '東京都' / '神奈川県' / '大阪府' 等が上位
```

### 5.3 Phase 3 ハンドラの動作確認 (Rust ビルド後)

```bash
# Rust 側 table_exists() が true を返すこと
# → 既存ハンドラ (subtab7_other.rs / talent_pool_expansion.rs / subtab5_phase4.rs) が
#   commute_od / minimum_wage を SELECT して結果が返ること
cargo run --release
# → /api/recruitment_diag/talent_pool 等で commute_od の流入元データが返ること
```

---

## 6. ロールバック方針

### 6.1 原則: 即時 DELETE を前提にしない

MEMORY「Claude による DB 書き込み禁止」+ 「Turso 書込み制限」の二重ガード。

### 6.2 誤投入時の標準対応

#### Step 1: 影響評価
- ローカル DB に問題なし (Turso のみ書き込み発生)
- 既存 Phase 0〜2 機能は `table_exists` フェイルセーフ済 → クラッシュなし
- 誤データが含まれていても、Rust ハンドラがエラーを返すか空結果を返すかのどちらか

#### Step 2: 新テーブル名で再投入 + 参照切替 (推奨)

```sql
-- 例: v2_external_commute_od_v2 として再投入
CREATE TABLE v2_external_commute_od_v2 (...);
INSERT INTO v2_external_commute_od_v2 SELECT * FROM ローカル正本;
-- 並行運用: Rust 側を v2_external_commute_od_v2 に切り替え
-- 旧テーブル削除はユーザー手動 + 動作確認後
```

利点: ローリングバック可、Phase 3 サービス影響最小。

#### Step 3: どうしても削除する場合 (非推奨)

**Claude では実行しない**。ユーザーが Turso CLI または `upload_to_turso.py` で再実行 (DROP + CREATE で実質的に上書き):

```bash
# upload_to_turso.py は DROP TABLE IF EXISTS → CREATE TABLE → INSERT の流れ
# 正しいローカルデータで再実行すれば誤データは上書きされる
python scripts/upload_to_turso.py
```

または Turso CLI で:

```sql
DROP TABLE v2_external_commute_od;
-- → upload_to_turso.py を再実行
```

### 6.3 ロールバック判断フロー

```
誤投入発見
   ↓
影響評価 (Phase 3 機能が動作するか)
   ↓
正常に動作している場合 → 次回ローリング更新で対応
正常に動作しない場合 → 新テーブル名で並行運用 (推奨)
                     → 緊急時は DROP + 再投入 (ユーザー手動)
```

---

## 7. Phase 3 での扱い

### 7.1 commute_od が未反映の場合

| 機能 | 影響 | 対応 |
|------|------|------|
| 通勤流入 Sankey (Phase 3 §10 後続追加) | ❌ 実装保留 | Turso 反映後に着手 |
| 流入元 TOP N (`commute_flow_summary`) | ❌ 計算不可 | 同上 |
| 採用診断 (`recruitment_diag/talent_pool`) | ⚠️ 既存機能、空結果返却 | 既存挙動継続 |

→ **Phase 3 で commute_od 関連機能を実装するなら、本タスクの upload を先行する必要あり**。

### 7.2 minimum_wage が未反映の場合

| 機能 | 影響 | 対応 |
|------|------|------|
| 給与競争力スコアの最低賃金補正 | ⚠️ 補助情報が欠落 | Phase 3 初期は進行可 |
| `subtab5_phase4.rs` 賃金水準クエリ | ⚠️ 空結果、UI に「データなし」表示 | 既存挙動継続 |

→ **Phase 3 初期は minimum_wage 未反映でも進行可能**。後続で upload。

---

## 8. 推奨実行スケジュール

| # | タイミング | 実行内容 | 担当 |
|--:|-----------|---------|------|
| 1 | upload 前 | `scripts/verify_turso_v2_sync.py` で REMOTE_MISSING 状態確認 | Claude or ユーザー |
| 2 | upload 前 | `scripts/upload_to_turso.py` の TABLES / TABLE_SCHEMAS 改修 (本書 §4.2) | ユーザー |
| 3 | upload 実行 | `python scripts/upload_to_turso.py` (TABLES を 2 テーブルに絞った状態で) | **ユーザー** |
| 4 | upload 後 | `scripts/verify_turso_v2_sync.py` 再実行 → MATCH/SAMPLE_MISMATCH 確認 | Claude or ユーザー |
| 5 | upload 後 | 個別 SQL で件数・サンプル・PK重複・日本語確認 (本書 §5.2) | ユーザー |
| 6 | Phase 3 着手 | Rust ハンドラの該当機能を有効化 | ユーザー (Phase 3 実装時) |

---

## 9. 禁止事項

| 項目 | 状態 |
|------|------|
| Claude/AI による Turso 書き込み | ❌ 完全禁止 |
| Claude/AI による `upload_to_turso.py` 実行 | ❌ 禁止 (本書はユーザー手動実行用) |
| Claude/AI による `scripts/upload_to_turso.py` の TABLES 改修コミット | ❌ 禁止 (改修案を docs として提示するのみ) |
| token 表示 (本書、ログ、レポート) | ❌ 禁止 |
| push (本コミット含む) | ❌ 禁止 |
| Rust 実装 (commute_od / minimum_wage 関連 Phase 3 機能) | ❌ 本書範囲外 |

---

## 10. 完了条件

本書 (Task d) は **手順書作成のみ** で完了。

- [x] 対象テーブル詳細 (スキーマ・行数・サンプル) が実機 SELECT で確認済
- [x] upload 前後検証 SQL が記載されている
- [x] upload 手順 (改修案コード含む) が記載されている
- [x] WRITE 消費見積が算出されている (83,449 row writes)
- [x] ロールバック方針が「DROP しない」を原則として記載
- [x] Phase 3 への影響 (commute_od クリティカル / minimum_wage 補助) が記載
- [x] 禁止事項一覧

ユーザーが本書に従って upload を実行した後、Phase 3 着手可能。
