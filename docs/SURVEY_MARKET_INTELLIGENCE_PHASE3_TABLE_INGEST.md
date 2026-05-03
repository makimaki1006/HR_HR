# Phase 3 前処理 (a): 不在 6 テーブル投入手順書

作成日: 2026-05-04
対象: ハローワーク分析システムV2 / `data/hellowork.db` + Turso V2

---

## 1. 目的

Phase 3 (媒体分析レポート拡張) で `municipality_recruiting_scores` を計算するために必要な 6 テーブルが、現状 `hellowork.db` に投入されていない。本手順書はユーザーが手動でデータ投入を完了させるためのオペレーションガイド。

**Claude は本手順書の作成と検証 SQL の提示のみ。スクリプト実行 (DB 書き込み) はユーザーが行う** (MEMORY: 2026-01 の Claude DB 書き込み事故再発防止)。

---

## 2. 投入対象テーブル

| # | テーブル | データソース | 既存スクリプト | 投入方式 | 期待行数 |
|--:|---------|------------|---------------|---------|---------:|
| 1 | `v2_external_household_spending` | e-Stat 家計調査 (`0002070003`) | `scripts/fetch_household_spending.py` | 直接 hellowork.db | 約 518 |
| 2 | `v2_external_labor_stats` | e-Stat 社会・人口統計体系 (`0000010206`) | `scripts/import_estat_labor_stats.py` | 直接 hellowork.db | 約 432 |
| 3 | `v2_external_land_price` | MLIT 国土数値情報 L01-25 | `scripts/fetch_geo_supplement.py --land` | CSV → 後段 import | 約 141 |
| 4 | `v2_external_industry_structure` | e-Stat 経済センサス (`0003449718`) | `scripts/fetch_industry_structure.py` | CSV → import | 約 36,100 |
| 5 | `v2_external_establishments` | e-Stat 経済センサス (`0004005687`) | `scripts/fetch_establishments.py` | 直接 hellowork.db | 約 940 |
| 6 | `v2_salesnow_companies` | HubSpot Companies API | `scripts/fetch_salesnow_companies.py` | CSV → 別 Turso DB | 約 198,201 |

データソースは各スクリプト先頭の docstring に明記されている (検証済)。

---

## 3. 共通設定

### 3.1 e-Stat APP_ID

全スクリプトで共通:

```
APP_ID = 85f70d978a4fd0da6234e2d07fc423920e077ee5
```

各スクリプトに既にハードコードされているため、追加引数不要 (`import_estat_labor_stats.py` のみ `--app-id` 引数で渡す方式)。

### 3.2 DB パス

```
C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\hellowork.db
```

直接投入スクリプトは `DB_PATH` 定数として既に固定済み。

### 3.3 HubSpot 認証

SalesNow 取得時のみ必要。`.env` ファイルパス (`scripts/fetch_salesnow_companies.py:23` で参照):

```
C:\Users\fuji1\OneDrive\デスクトップ\Hubspot\.env
```

実機確認済 (本手順書作成時点で存在)。トークン本体は本書に転記しない。

### 3.4 Turso V2 認証 (アップロード時)

```bash
export TURSO_EXTERNAL_URL=libsql://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io
export TURSO_EXTERNAL_TOKEN=<別途取得>
```

`memory/reference_turso_v2_credentials.md` 参照。

### 3.5 SalesNow 専用 Turso 認証 (テーブル #6 のみ)

SalesNow は country-statistics とは別 Turso DB を使用 (`scripts/upload_salesnow_to_turso.py:9-10`):

```bash
export SALESNOW_TURSO_URL=...
export SALESNOW_TURSO_TOKEN=...
```

`memory/reference_salesnow_credentials.md` 参照。

---

## 4. 投入順序と実行コマンド

### Step 1: `v2_external_household_spending` (直接投入、最速)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts/fetch_household_spending.py
```

- 所要時間: 1〜2 分 (e-Stat API 1 回呼び出し)
- 出力: `data/hellowork.db` の `v2_external_household_spending` テーブル
- 検証 SQL:
  ```sql
  SELECT COUNT(*) FROM v2_external_household_spending;       -- 期待: 約 518
  SELECT DISTINCT prefecture FROM v2_external_household_spending LIMIT 5;
  ```

### Step 2: `v2_external_labor_stats` (直接投入)

```bash
python scripts/import_estat_labor_stats.py --app-id 85f70d978a4fd0da6234e2d07fc423920e077ee5
```

- 所要時間: 5〜10 分 (e-Stat API 多数回呼び出し)
- 取得指標: 完全失業率/就業者比率/離職率/転職率/月間労働時間/賃金 等 (`scripts/import_estat_labor_stats.py:50-` 参照)
- 検証 SQL:
  ```sql
  SELECT COUNT(*) FROM v2_external_labor_stats;              -- 期待: 約 432
  SELECT prefecture, fiscal_year, unemployment_rate FROM v2_external_labor_stats LIMIT 5;
  ```

### Step 3: `v2_external_establishments` (直接投入)

```bash
python scripts/fetch_establishments.py
```

- 所要時間: 5〜10 分
- 検証 SQL:
  ```sql
  SELECT COUNT(*) FROM v2_external_establishments;           -- 期待: 約 940 (47 県 × 20 産業)
  SELECT industry_code, COUNT(DISTINCT prefecture) FROM v2_external_establishments GROUP BY industry_code;
  ```

### Step 4: `v2_external_land_price` (CSV → import 経由)

#### 4.1 CSV 取得
```bash
python scripts/fetch_geo_supplement.py --land
```

- 出力: `scripts/data/land_price_by_prefecture.csv`
- 所要時間: 3〜5 分 (MLIT 全国 ZIP 約 20MB ダウンロード + 解析)

#### 4.2 hellowork.db 投入
`fetch_geo_supplement.py` は CSV 出力のみ。DB 投入は別途必要。

**選択肢 A** (推奨): `scripts/upload_new_external_to_turso.py` を Turso にアップロードしつつ、ローカル投入スクリプトを併用
**選択肢 B**: `scripts/import_external_csv.py` (汎用 CSV → DB) を使用

実装担当者がスクリプトを確認の上、選択。

- 検証 SQL:
  ```sql
  SELECT COUNT(*) FROM v2_external_land_price;               -- 期待: 約 141
  SELECT prefecture, land_use, avg_price_per_sqm FROM v2_external_land_price LIMIT 5;
  ```

### Step 5: `v2_external_industry_structure` (大規模、途中再開可)

#### 5.1 CSV 取得 (途中再開対応)
```bash
python scripts/fetch_industry_structure.py
# 中断時は再実行で .progress ファイルから続きから取得
# 完全リセットしたい場合は --reset オプション
```

- 出力: `scripts/data/industry_structure_by_municipality.csv` (約 36,100 行)
- 所要時間: **30〜60 分** (e-Stat API 1.0 秒間隔、約 1,800 市区町村)
- 進捗ファイル: `scripts/data/industry_structure_by_municipality.progress`

#### 5.2 hellowork.db 投入
```bash
python scripts/import_ssdse_to_db.py --table v2_external_industry_structure --csv scripts/data/industry_structure_by_municipality.csv
```
(具体的なオプションは `import_ssdse_to_db.py --help` で確認)

- 検証 SQL:
  ```sql
  SELECT COUNT(*) FROM v2_external_industry_structure;        -- 期待: 約 36,100
  SELECT industry_code, COUNT(DISTINCT city_code) FROM v2_external_industry_structure GROUP BY industry_code;
  ```

### Step 6: `v2_salesnow_companies` (HubSpot API、最大規模)

#### 6.1 HubSpot トークン確認
```bash
type "C:\Users\fuji1\OneDrive\デスクトップ\Hubspot\.env"
```
`HUBSPOT_TOKEN=...` または同等のキーが存在することを確認。

#### 6.2 CSV 取得 (チェックポイント対応)
```bash
python scripts/fetch_salesnow_companies.py
# 中断時は data/salesnow_checkpoint.json から再開
```

- 出力: `data/salesnow_companies.csv` (約 492 MB、198K 社、44 フィールド)
- 所要時間: **2〜4 時間** (HubSpot API レート制限あり)

#### 6.3 SalesNow 専用 Turso 投入
```bash
export SALESNOW_TURSO_URL=...
export SALESNOW_TURSO_TOKEN=...
python scripts/upload_salesnow_to_turso.py --resume
```

- バッチ 500 + 並列 3 で投入 (`upload_salesnow_to_turso.py:36-37`)
- 所要時間: 30〜60 分
- 検証 SQL (Turso 側):
  ```sql
  SELECT COUNT(*) FROM v2_salesnow_companies;                 -- 期待: 約 198,201
  SELECT prefecture, COUNT(*) FROM v2_salesnow_companies GROUP BY prefecture LIMIT 5;
  ```

#### 6.4 ローカル hellowork.db への投入 (任意)
SalesNow を Phase 3 で **Turso V2 経由のみ** で参照する場合、ローカル投入は不要。

ローカルにも保持したい場合は `scripts/upload_to_turso.py` をローカル SQLite モードに変更するか、別途 import スクリプトを書く必要あり。**Phase 3 初期は Turso 経由のみ** を推奨。

---

## 5. Turso V2 への昇格 (テーブル #1〜#5 共通)

ローカル hellowork.db に投入した後、本番反映のため Turso にアップロード:

```bash
export TURSO_EXTERNAL_URL=libsql://country-statistics-makimaki1006.aws-ap-northeast-1.turso.io
export TURSO_EXTERNAL_TOKEN=<token>

# dry-run でテーブル一覧と件数を確認
python scripts/upload_to_turso.py --dry-run

# 本番投入
python scripts/upload_to_turso.py
```

`upload_to_turso.py` の `TABLES` 配列 (line 25-46) に既に 6 テーブルのうち 4 つ (`labor_stats / establishments / household_spending` など) が含まれている。`industry_structure` と `land_price` は配列に追加が必要 (要コード修正)。

→ **要追加修正候補ファイル**: `scripts/upload_to_turso.py:25-46` の `TABLES` 配列

---

## 6. 投入後の総合検証

すべて投入完了後:

```sql
-- 6 テーブルすべての行数確認
SELECT 'household_spending' AS tbl, COUNT(*) FROM v2_external_household_spending
UNION ALL SELECT 'labor_stats', COUNT(*) FROM v2_external_labor_stats
UNION ALL SELECT 'establishments', COUNT(*) FROM v2_external_establishments
UNION ALL SELECT 'land_price', COUNT(*) FROM v2_external_land_price
UNION ALL SELECT 'industry_structure', COUNT(*) FROM v2_external_industry_structure;

-- ヘッダー混入チェック (Phase 3 (b) 参照)
SELECT COUNT(*) FROM v2_external_household_spending WHERE prefecture = '都道府県';
SELECT COUNT(*) FROM v2_external_labor_stats WHERE prefecture = '都道府県';
-- 以下省略
```

期待値: すべて行数が期待範囲内、ヘッダー混入は 0 件。

ヘッダー混入が新規発生していた場合は、`SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md` の WHERE フィルタ対象に追加すること。

---

## 7. 依存関係

```
Step 1 (household_spending)  ─┐
Step 2 (labor_stats)         ─┤  独立、並列実行可
Step 3 (establishments)      ─┤
Step 4 (land_price)          ─┘

Step 5 (industry_structure) → industry_mapping.py の業種コードマッピングと整合性確認推奨

Step 6 (salesnow_companies) → 完全独立 (別 Turso DB)
```

並列実行可能 (Step 1〜4)。Step 5 は時間が長いため最後に開始。Step 6 は HubSpot API レート上限の関係で並列効果が薄い。

---

## 8. エラー対応

### 8.1 e-Stat API レート制限
1 日あたり 100,000 リクエスト上限 (APP_ID あたり)。`fetch_industry_structure.py` は `REQUEST_INTERVAL = 1.0` 秒で安全側設定済み。

### 8.2 HubSpot API レート制限
10 req/sec 上限。`fetch_salesnow_companies.py` 内で sleep 制御済み。

### 8.3 Turso 書込制限
無料枠 100 WRITE/月。テーブル投入は 1 回 = 1 WRITE × N 行。198K 行投入は枠を消費する。**1 回で完了させる** (MEMORY: 2026-04-03 浪費事故)。

### 8.4 中断時の再開

| スクリプト | 再開方法 |
|-----------|---------|
| `fetch_industry_structure.py` | 再実行で `.progress` から続行 |
| `fetch_salesnow_companies.py` | 再実行で `salesnow_checkpoint.json` から続行 |
| `upload_salesnow_to_turso.py` | `--resume` フラグ |
| その他 | 全件再取得 (DB は CREATE TABLE IF NOT EXISTS + INSERT OR REPLACE 想定、要確認) |

### 8.5 ロールバック方針

| シナリオ | 対応 |
|---------|------|
| 1 テーブルのみ失敗 | 該当テーブルを `DROP TABLE IF EXISTS <table>` 後、Step 再実行 |
| Turso アップロード失敗 | ローカル hellowork.db は保全。Turso 側のみ DROP + 再アップロード |
| 全体やり直し | `data/hellowork.db` をバックアップから戻す (前回スナップショットの確認推奨) |

DROP は Claude による実行不可 (MEMORY ルール)。**ユーザー手動で実行**。

---

## 9. 推奨実行スケジュール

| 時間枠 | 実行内容 |
|--------|---------|
| Day 1 朝 | Step 1〜3 (household_spending / labor_stats / establishments) 並列実行 (合計 15 分) |
| Day 1 朝 | Step 4 (land_price CSV 取得) (5 分) |
| Day 1 朝 | Step 5 (industry_structure CSV 取得) を起動 → 60 分待機 |
| Day 1 昼 | Step 1〜5 の検証 SQL 実行 |
| Day 1 昼 | `upload_to_turso.py` で Turso 反映 |
| Day 1 午後 | Step 6 (salesnow) 起動 → 数時間待機 |
| Day 2 | Phase 3 着手前検証 + Turso 同期確認 |

---

## 10. 完了条件

- [ ] 6 テーブルすべての行数が期待範囲内 (検証 SQL pass)
- [ ] ヘッダー混入が 0 件 (新規発生していない)
- [ ] Turso V2 にも反映済 (5 テーブル) + SalesNow 別 Turso 反映済 (1 テーブル)
- [ ] `SURVEY_MARKET_INTELLIGENCE_PHASE3_TURSO_VERIFY.md` の検証スクリプトで PASS

完了後、Phase 3 (Rust ハンドラ実装) 着手可能。
