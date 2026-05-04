# Phase 3 Step A: commute_flow_summary upload 前チェックリスト + Rollback 方針

作成日: 2026-05-04
対象: ローカル `data/hellowork.db::commute_flow_summary` (27,879 行) → Turso V2 反映

本書は `SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md` (実行手順書) の **補助** として、upload 前検証 + 擬似コード版 rollback 方針を集約したチェックリスト。

---

## 1. Upload 前チェックリスト

### 1.1 ローカルデータ前提

| # | チェック項目 | コマンド | 期待値 |
|--:|-------------|---------|--------|
| 1 | ローカル DB 存在 | `ls -la data/hellowork.db` | 存在 |
| 2 | `commute_flow_summary` テーブル存在 | `sqlite3 data/hellowork.db "SELECT name FROM sqlite_master WHERE name='commute_flow_summary'"` | 1 行返却 |
| 3 | 行数 | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM commute_flow_summary"` | **27,879** |
| 4 | DISTINCT destination 数 | `sqlite3 data/hellowork.db "SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary"` | **1,894** |
| 5 | rank 重複なし | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM (SELECT destination_municipality_code, occupation_group_code, source_year, rank_to_destination, COUNT(*) c FROM commute_flow_summary GROUP BY 1,2,3,4 HAVING c>1)"` | **0** |
| 6 | rank 範囲 (1〜20) | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM commute_flow_summary WHERE rank_to_destination NOT BETWEEN 1 AND 20"` | **0** |
| 7 | self-loop なし | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM commute_flow_summary WHERE destination_prefecture = origin_prefecture AND destination_municipality_name = origin_municipality_name"` | **0** |
| 8 | flow_share ∈ [0, 1] | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM commute_flow_summary WHERE flow_share < 0 OR flow_share > 1.0"` | **0** |
| 9 | 日本語表示正常 | `sqlite3 data/hellowork.db "SELECT DISTINCT destination_prefecture FROM commute_flow_summary LIMIT 5"` | 北海道, 青森県, 岩手県... |
| 10 | 擬似コード形式確認 | `sqlite3 data/hellowork.db "SELECT destination_municipality_code FROM commute_flow_summary LIMIT 1"` | `北海道:札幌市` 形式 |
| 11 | estimated_target_flow_* が NULL | `sqlite3 data/hellowork.db "SELECT COUNT(*) FROM commute_flow_summary WHERE estimated_target_flow_conservative IS NOT NULL"` | **0** (Step 5 後続で計算) |

→ チェック項目 1〜11 すべて期待値通りなら upload 適格。

### 1.2 環境前提

| # | チェック項目 | コマンド | 期待値 |
|--:|-------------|---------|--------|
| 12 | `.env` 存在 | `ls -la .env` | 存在 |
| 13 | `TURSO_EXTERNAL_URL` 設定 | `set -a && source .env && set +a && [ -n "$TURSO_EXTERNAL_URL" ] && echo OK` | OK |
| 14 | `TURSO_EXTERNAL_TOKEN` 設定 | 同上 | OK (token 自体は表示しない) |
| 15 | Python `requests` インストール | `python -c "import requests"` | エラーなし |
| 16 | `upload_to_turso.py` 存在 | `ls -la scripts/upload_to_turso.py` | 存在 |

### 1.3 Turso 側前提

| # | チェック項目 | コマンド | 期待値 |
|--:|-------------|---------|--------|
| 17 | Turso 接続成功 | `python scripts/verify_turso_v2_sync.py --dry-run` | "TURSO_EXTERNAL_URL: 設定済" |
| 18 | `commute_flow_summary` が Turso に **不在** であること | `python scripts/verify_turso_v2_sync.py 2>&1 \| grep commute_flow_summary` | "REMOTE_MISSING" or 不在 |
| 19 | Turso クォータ余裕 (25M row writes/月) | Turso ダッシュボード確認 | 直近の使用量 < 95% |
| 20 | `upload_to_turso.py` 改修済 (TABLES + TABLE_SCHEMAS に追加) | `grep "commute_flow_summary" scripts/upload_to_turso.py \| head -5` | 2 件以上ヒット |

### 1.4 安全性確認

| # | チェック項目 | コマンド | 期待値 |
|--:|-------------|---------|--------|
| 21 | バックアップ取得 (`upload_to_turso.py.bak`) | `ls -la scripts/upload_to_turso.py.bak` | 存在 (なければ `cp` で作成) |
| 22 | `upload_to_turso.py` の TABLES が **最小スコープ** に絞られている (commute_flow_summary だけ、または既存 + 新規の必要分のみ) | `grep -A 5 "^TABLES = " scripts/upload_to_turso.py` | TABLES 配列に余計な再投入対象が混入していない |
| 23 | dry-run で 27,879 行を確認 | `python scripts/upload_to_turso.py --dry-run 2>&1 \| grep commute_flow_summary` | "27879 行 (dry-run)" |
| 24 | git working tree クリーン (commit 漏れなし) | `git status --short` | 関連ファイルが全て tracked or 意図的 untracked |

→ 全 24 項目 OK で **本番実行可**。

---

## 2. 本番実行 (1 回限り、約 5 分)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
set -a && source .env && set +a
python scripts/upload_to_turso.py
```

期待ログ:
```
v2_commute_flow_summary: 27879 行
    commute_flow_summary: 1000/27879 行完了
    commute_flow_summary: 2000/27879 行完了
    ...
    commute_flow_summary: 27879/27879 行完了
  commute_flow_summary: 27879 行アップロード完了
```

### 2.1 中断時の挙動

- ネットワーク切断 → 再実行で続行可 (DROP IF EXISTS + CREATE で先頭から再投入)
- HTTP 500 / タイムアウト → 30 秒待機して再実行 (Turso のレート制限の場合)

### 2.2 完了確認

```bash
python scripts/verify_turso_v2_sync.py
# → commute_flow_summary が REMOTE_MISSING → MATCH (or 型差のみ SAMPLE_MISMATCH)
```

---

## 3. Upload 後検証 (15 項目)

### 3.1 件数・整合性

| # | 検証 | 実行場所 | 期待値 |
|--:|------|---------|--------|
| 1 | Turso 側行数 | Turso CLI: `SELECT COUNT(*) FROM commute_flow_summary` | **27,879** |
| 2 | local vs Turso 件数一致 | verify スクリプト判定 | MATCH or 型差のみ |
| 3 | PK 重複なし | `SELECT COUNT(*) - COUNT(DISTINCT destination_municipality_code \|\| origin_municipality_code \|\| occupation_group_code \|\| source_year) FROM commute_flow_summary` | **0** |
| 4 | DISTINCT destination 数 | `SELECT COUNT(DISTINCT destination_municipality_code) FROM commute_flow_summary` | **1,894** |
| 5 | rank 範囲 | `SELECT MIN(rank_to_destination), MAX(rank_to_destination) FROM commute_flow_summary` | **(1, 20)** |
| 6 | self-loop 0 件 | `WHERE destination_pref = origin_pref AND destination_muni = origin_muni` | **0** |
| 7 | flow_share 範囲 | `SELECT MIN(flow_share), MAX(flow_share) FROM commute_flow_summary` | **(>=0, <=1)** |
| 8 | dest 別 share 合計 | `SELECT MAX(s) FROM (SELECT SUM(flow_share) s FROM commute_flow_summary GROUP BY destination_municipality_code)` | **<= 1.0001** |

### 3.2 日本語表示

| # | 検証 | 期待値 |
|--:|------|--------|
| 9 | DISTINCT destination_prefecture サンプル | 47 都道府県名が日本語で正常 |
| 10 | DISTINCT origin_municipality_name サンプル | 主要市区町村名が日本語で正常 |
| 11 | 政令市の区表示 (例: 札幌市東区) | 正常 |

### 3.3 ビジネスロジック整合

| # | 検証 | 期待値 |
|--:|------|--------|
| 12 | 札幌市の流入 TOP 1 | 札幌市内の他区 (北区など) |
| 13 | 新宿区の流入 TOP 1 | 特別区部 or 隣接区 |
| 14 | 全 destination で TOP 20 揃い | `SELECT COUNT(*) FROM (SELECT destination_municipality_code FROM commute_flow_summary GROUP BY 1 HAVING COUNT(*) < 20 AND COUNT(*) >= 1)` で部分的 (TOP 19 以下) は許容、ただし極端に少ない (< 5) は要調査 |
| 15 | occupation_group_code の統一 | DISTINCT で `'all'` のみ |

---

## 4. Rollback 方針 (擬似コード版用)

### 4.1 原則: 即時 DELETE しない

MEMORY「Claude による DB 書き込み禁止」+ Turso 課金事故防止に基づき、誤投入時も **即時 DELETE は避ける**。

### 4.2 シナリオ別対応

#### シナリオ A: 投入完全失敗 (HTTP エラーで中断)

| 状況 | 対応 |
|------|------|
| Turso に部分データ (例: 1,000 行のみ) | `upload_to_turso.py` 再実行 (DROP IF EXISTS で先頭から再投入) |
| Turso 側に何も入っていない | `upload_to_turso.py` 再実行 |
| エラー原因が不明 | ログ確認 → 原因特定後再実行 |

→ **Claude では実行しない、ユーザー手動**。

#### シナリオ B: データは入ったが品質に問題発覚

例: ヘッダー混入レコードを除外し忘れた、self-loop が混入していた

| 段階 | 対応 |
|------|------|
| 1. ローカルで再生成 | `python scripts/build_commute_flow_summary.py` でローカル再構築 (擬似コード版なら冪等) |
| 2. ローカル検証 | 本書 §1.1 のチェックリスト 1〜11 を再実行 |
| 3. Turso 再投入 | `upload_to_turso.py` を再実行 (DROP + CREATE で上書き) |
| 4. verify | `verify_turso_v2_sync.py` で MATCH 確認 |

→ ローカルでの修正 + ユーザー手動再 upload。

#### シナリオ C: 擬似コードを JIS コードに置換したい (将来の整合化)

`SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md` 完了後の手順:

| 段階 | 対応 |
|------|------|
| 1. JIS マスタ整備完了 | `municipality_code_master` テーブル投入 |
| 2. ローカル UPDATE | `UPDATE commute_flow_summary SET destination_municipality_code = (SELECT mcm.municipality_code FROM municipality_code_master mcm WHERE mcm.prefecture = ... AND mcm.municipality_name = ...)` |
| 3. ローカル検証 | `WHERE destination_municipality_code LIKE '%:%' で残擬似コード検出` → 0 件 |
| 4. Turso 再投入 | `upload_to_turso.py` で再投入 (DROP + CREATE) |

→ **本シナリオは Phase 3 Step A の範囲外**。JIS マスタ整備が前提。

#### シナリオ D: テーブル自体を削除したい (極端な場合のみ、非推奨)

```sql
-- Turso CLI で直接実行 (Claude/AI からは実行不可)
DROP TABLE IF EXISTS commute_flow_summary;
```

注意:
- これを行うと、Phase 3 Step 3 で実装した HTML セクション (variant=market_intelligence) の通勤流入元データが空になる
- 既存 `v2_external_commute_od` (元データ) からのフォールバック計算 (`fetch_commute_flow_summary` 関数の Step 1 設計) で代替可能だが、性能低下
- **基本的にはシナリオ B (上書き再投入) で対処**

### 4.3 緊急停止判断

| 症状 | 判断 |
|------|------|
| Turso WRITE クォータ警告 (>95%) | upload 即時中断、翌月待機 |
| HTTP 401 / 403 (auth エラー) | token 無効 → ユーザーが Turso ダッシュボードで再発行 |
| HTTP 429 (レート制限) | 30 秒待機 → 再実行 |
| データ整合性違反 (PK 違反等) | DROP TABLE → 再投入 (シナリオ B) |

---

## 5. 完了条件 (本チェックリストの)

- [x] Upload 前チェック 24 項目
- [x] 本番実行手順
- [x] Upload 後検証 15 項目
- [x] Rollback 方針 4 シナリオ
- [x] 緊急停止判断基準

ユーザーが本書に従い:
1. §1 で 24 項目 OK 確認
2. §2 で本番実行
3. §3 で 15 項目検証
4. 問題発生時は §4 のシナリオで対応

これで `commute_flow_summary` の Turso 反映が完結する (擬似コード版)。

---

## 6. 関連 docs

- 実行手順書: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP_A_COMMUTE_FLOW_UPLOAD.md`
- DDL: `docs/survey_market_intelligence_phase0_2_schema.sql`
- 生成スクリプト: `scripts/build_commute_flow_summary.py`
- JIS 整備設計: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_JIS_CODE_PLAN.md`
- Step 5 全体前提: `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md`
