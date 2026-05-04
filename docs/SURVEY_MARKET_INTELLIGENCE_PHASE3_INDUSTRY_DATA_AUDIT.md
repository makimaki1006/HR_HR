# Phase 3 Step 5 前提: 産業構造データ実態調査 (F3 補正項用)

- 作成日: 2026-05-04
- 担当: Worker A (調査担当)
- 対象: `v2_external_industry_structure` (経済センサス R3 想定)
- 用途: Phase 3 Step 5 `municipality_occupation_population` の F3 (産業構成補正) 入力
- 制約: ローカル DB / Turso V2 ともに **READ-only**。`.env` 直接 open 禁止。

---

## 0. 結論 (TL;DR)

| 項目 | 結果 |
|------|-----|
| ローカル `data/hellowork.db` 投入状況 | ❌ **未投入** (49 テーブル中、industry / establishment 系は `v2_cross_industry_competition` のみ) |
| Turso V2 上の `v2_external_industry_structure` | ✅ **存在** (`turso_v2_sync_report_2026-05-04.md` で LOCAL_MISSING として確認、行数は未確認: 同レポートはリモートのみ存在テーブルの COUNT を取得していない) |
| ローカルの CSV 元データ | ✅ **取得済み** `scripts/data/industry_structure_by_municipality.csv` (36,099 行 / 2.36 MB / 1,719 市区町村 × 21 産業) |
| 産業大分類 | **21 区分** (e-Stat 経済センサス独自体系: AS/AR/AB/CR + C〜S) |
| 市区町村コード体系 | **JIS 5 桁** (`prefecture_code` 2 桁 + 市区町村下 3 桁、`code[2:] != '000'` で都道府県集計除外済み) |
| F3 で使える列 | `establishments`, `employees_total`, `employees_male`, `employees_female` |
| **推奨投入ルート** | **A 案: 既存 CSV を `scripts/upload_new_external_to_turso.py` でローカル投入** (新規 fetch 不要、Turso V2 はリモート参照済とみなして読み取り専用、ローカル DDL は `upload_new_external_to_turso.py` 内に既存定義済み) |

---

## 1. ローカル `data/hellowork.db` 現状

### 1.1 関連テーブル探索結果 (2026-05-04 SELECT のみ)

```sql
-- 検索条件: name LIKE '%industry%' OR '%establishment%' OR '%business%'
SELECT name FROM sqlite_master WHERE type='table' AND ...
```

| テーブル | 行数 | 備考 |
|---------|-----:|-----|
| `v2_cross_industry_competition` | 1,689 | 競合分析用派生テーブル (F3 用ではない) |
| `v2_external_industry_structure` | **存在しない** | Phase 3 で投入予定 |
| `v2_external_establishments` | **存在しない** | 同上 (代替候補) |

### 1.2 全 v2_external_* テーブル投入状況 (10 件のみ)

| テーブル | 行数 |
|---------|-----:|
| `v2_external_commute_od` | 86,762 |
| `v2_external_commute_od_with_codes` | 86,762 |
| `v2_external_daytime_population` | 1,740 |
| `v2_external_foreign_residents` | 1,742 |
| `v2_external_job_opening_ratio` | 47 |
| `v2_external_migration` | 1,741 |
| `v2_external_minimum_wage` | 47 |
| `v2_external_population` | 1,742 |
| `v2_external_population_pyramid` | 15,660 |
| `v2_external_prefecture_stats` | 47 |

**結論**: ローカル DB は人口・通勤 OD・最低賃金等の市区町村基礎データのみ。産業構造データは未投入。

### 1.3 関連マスタ

- `municipality_code_master`: 1,917 行 (estat_commute_od 由来、source_year=2020)
  - 列: `municipality_code(JIS 5桁,PK)`, `prefecture`, `municipality_name`, `pref_code(2桁)`, `area_type`, `area_level`, `is_special_ward`, `is_designated_ward`, `parent_code`, `source`, `source_year`
  - 政令市の区 (designated_ward) も `unit` レベルで含む

---

## 2. Turso V2 上の状態 (READ-only 既存レポートからの転記)

`docs/turso_v2_sync_report_2026-05-04.md` (Worker B が `scripts/verify_turso_v2_sync.py --output ...` で生成、READ 17 消費) によると:

| テーブル | Turso V2 ローカル | Turso V2 リモート | 行数 |
|---------|:--:|:--:|---:|
| `v2_external_industry_structure` | ❌ LOCAL_MISSING | ✅ 存在 | **未取得** (LOCAL 不在のため COUNT 比較スキップ) |
| `v2_external_establishments` | ❌ LOCAL_MISSING | ✅ 存在 | **未取得** (同上) |

**未確認事項** (本作業ではライブクエリ実施せず):
- リモート行数
- 実際の列構成 (DDL は推測)
- year / 産業分類数の実態

> **理由**: `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN` は OS 環境変数として未エクスポート。Worker A 制約により `.env` を直接 open しないため、本セッションでライブ確認は不可能。次回 Worker C / ユーザー実行時に必要なら、`scripts/verify_turso_v2_sync.py` の `TARGET_TABLES` に絞り込んだフォーク版で COUNT + サンプル取得すること。

---

## 3. 推測される DDL (既存スクリプトから逆引き)

`scripts/upload_new_external_to_turso.py` L128-141 に既に **完全な DDL が定義済み** (Phase A SSDSE-A 実装で投入済の想定):

```sql
CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
    prefecture_code  TEXT NOT NULL,   -- 都道府県コード (2 桁、例: '01')
    city_code        TEXT NOT NULL,   -- 市区町村 JIS コード (5 桁、例: '01100')
    city_name        TEXT,            -- 市区町村名 (例: '札幌市')
    industry_code    TEXT NOT NULL,   -- 産業大分類コード (AS/AR/AB/CR/C..S)
    industry_name    TEXT,            -- 産業大分類名 (例: '製造業')
    establishments   INTEGER,         -- 事業所数
    employees_total  INTEGER,         -- 従業者数 (男女計)
    employees_male   INTEGER,         -- 従業者数 (男)
    employees_female INTEGER,         -- 従業者数 (女)
    PRIMARY KEY (city_code, industry_code)
);
```

参考 (代替): `v2_external_establishments` (Phase A SSDSE-A 由来、市区町村名ベース、より新しい設計)

```sql
CREATE TABLE IF NOT EXISTS v2_external_establishments (
    prefecture     TEXT NOT NULL,
    municipality   TEXT NOT NULL,
    industry_code  TEXT NOT NULL,
    industry_name  TEXT,
    establishments INTEGER,
    employees      INTEGER,
    reference_year INTEGER,
    PRIMARY KEY (prefecture, municipality, industry_code)
);
```

両者の使い分け (推奨: F3 では **`v2_external_industry_structure` を主、`v2_external_establishments` を従**):
- `industry_structure` は **JIS コード基準** で `municipality_code_master` と直接 JOIN 可、男女別従業者数あり (F3 が性別の重みづけに使う場合に有利)
- `establishments` は **名称ベース** で名寄せが必要、男女別なし

---

## 4. ローカル CSV 元データの実態 (2026-05-04 確認)

### 4.1 ファイル

- パス: `scripts/data/industry_structure_by_municipality.csv` (UTF-8-sig)
- サイズ: 2,365,407 bytes
- 行数: 36,099 行 (ヘッダ除く)
- 進捗: `scripts/data/industry_structure_by_municipality.progress` (途中再開記録あり)

### 4.2 ヘッダ

```
prefecture_code, city_code, city_name, industry_code, industry_name,
establishments, employees_total, employees_male, employees_female
```

### 4.3 集計値

| 指標 | 値 |
|-----|---:|
| Distinct `prefecture_code` | **47** (全都道府県) |
| Distinct `city_code` | **1,719** (5 桁 JIS、政令市は親市レベル `@level="2"` のみ。区 `@level="3"` は除外) |
| Distinct `industry_code` | **21** |

### 4.4 産業コード一覧 (e-Stat メタ情報)

| code | name | 性質 |
|:----:|------|------|
| `AS` | 全産業 | **集計用** (全産業合計) |
| `AR` | 全産業 (公務を除く) | **集計用** |
| `AB` | 農林漁業 | 産業大分類 (A 農業 + B 林業 + 漁業 統合) |
| `CR` | 非農林漁業 (公務を除く) | **集計用** |
| `C` | 鉱業, 採石業, 砂利採取業 | 産業大分類 |
| `D` | 建設業 | 産業大分類 |
| `E` | 製造業 | 産業大分類 |
| `F` | 電気・ガス・熱供給・水道業 | 産業大分類 |
| `G` | 情報通信業 | 産業大分類 |
| `H` | 運輸業, 郵便業 | 産業大分類 |
| `I` | 卸売業, 小売業 | 産業大分類 |
| `J` | 金融業, 保険業 | 産業大分類 |
| `K` | 不動産業, 物品賃貸業 | 産業大分類 |
| `L` | 学術研究, 専門・技術サービス業 | 産業大分類 |
| `M` | 宿泊業, 飲食サービス業 | 産業大分類 |
| `N` | 生活関連サービス業, 娯楽業 | 産業大分類 |
| `O` | 教育, 学習支援業 | 産業大分類 |
| `P` | 医療, 福祉 | 産業大分類 |
| `Q` | 複合サービス事業 | 産業大分類 |
| `R` | サービス業 (他に分類されないもの) | 産業大分類 |
| `S` | 公務 (他に分類されるものを除く) | 産業大分類 |

**実体は 17 産業大分類 + 4 集計コード**。F3 計算では **集計コード `AS`/`AR`/`CR` を使うか、個別大分類のみを使うかを Worker C で要決定**。

> 注: 標準的な日本産業分類 JSIC 大分類 A〜T は **20 区分**だが、本データは A 農業/B 林業を `AB` に統合・水産業を含むため **17 大分類** (A〜S のうち、A と B が統合)。21 区分という表現はメタ情報上のコード総数 (集計コード含む)。

### 4.5 サンプル行 (Mojibake 修正済の構造のみ)

- 札幌市 (`01100`) × `AS` 全産業: 事業所 71,870 / 従業者計 920,986 (男 489,881 / 女 422,574)
- 札幌市 × `AR` (公務除く): 事業所 71,580 / 従業者計 889,458
- 札幌市 × `AB` 農林漁業: 事業所 102 / 従業者計 1,152

### 4.6 統計表 ID / 取得仕様 (`scripts/fetch_industry_structure.py` より)

| 項目 | 値 |
|------|------|
| 出典 | e-Stat 経済センサス令和 3 年活動調査 |
| `statsDataId` | `0003449718` |
| `appId` (e-Stat API) | スクリプトに直書き (token 露出ではないが、運用では env 経由を推奨) |
| 表章項目 (tab) | 102-2021 (事業所数), 113-2021 (従業者計), 114-2021 (男), 115-2021 (女) |
| 経営組織 (cdCat02) | `0` (民営 + 公営の総数) |
| 取得粒度 | `@level="2"` の市区町村 (政令市親市のみ、区は除外) |
| 秘匿値処理 | `int()` 変換失敗時は `None` |
| 進捗管理 | `.progress` ファイルで取得済 city_code を記録 (途中再開可能) |

---

## 5. F3 計算で使う列の推奨

| 用途 | 列 | 補足 |
|------|----|------|
| 主要重み (推奨) | `employees_total` | 従業者数ベース。事業所規模の差を吸収 |
| 副次重み | `establishments` | 事業所数。従業者と二重補正に注意 (model_v2 §3.2) |
| 性別補正 | `employees_male`, `employees_female` | F3 を性別別に分解する場合のみ |
| 分母 (シェア計算用) | `industry_code='AR'` 行の `employees_total` | **公務除く全産業** (公務 S を産業区分に入れるか否かは要設計) |

**WHERE 句の推奨**: `industry_code IN ('C','D','E','F','G','H','I','J','K','L','M','N','O','P','Q','R')` (公務 S を除外、AB は農林漁業として個別 1 区分扱い、集計コード AS/AR/CR は分母として別途 SELECT)。AB を分けるか A と B にバラすかはデータ取得時点で AB 統合済のため不可分。

---

## 6. 投入推奨手順 (ユーザー手動実行)

### 6.1 推奨ルート: A 案 (既存 CSV → ローカル DB → Turso 投入)

#### Step 1. ローカル DB に投入

```powershell
# 作業ディレクトリ
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# 既存 CSV を確認
Get-Item scripts\data\industry_structure_by_municipality.csv | Select Length, LastWriteTime
# 期待: ~2.36 MB, 36,099 行

# upload_new_external_to_turso.py の --local-only モードがあれば使用 (要確認)
# なければ専用 import_external_csv.py 系を使うか、以下の SQL を直接実行:
```

```sql
-- DDL (scripts/upload_new_external_to_turso.py L128-141 と一致させる)
CREATE TABLE IF NOT EXISTS v2_external_industry_structure (
    prefecture_code  TEXT NOT NULL,
    city_code        TEXT NOT NULL,
    city_name        TEXT,
    industry_code    TEXT NOT NULL,
    industry_name    TEXT,
    establishments   INTEGER,
    employees_total  INTEGER,
    employees_male   INTEGER,
    employees_female INTEGER,
    PRIMARY KEY (city_code, industry_code)
);

CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_pref
    ON v2_external_industry_structure(prefecture_code);
CREATE INDEX IF NOT EXISTS idx_v2_external_industry_structure_industry
    ON v2_external_industry_structure(industry_code);
```

```powershell
# CSV を SQLite にインポート (sqlite3 CLI、UTF-8-sig BOM 注意)
# .mode csv → BOM 付き CSV では先頭フィールド名が "﻿prefecture_code" になる場合あり、要 BOM 除去
sqlite3 data\hellowork.db ".mode csv" ".import --skip 1 scripts\data\industry_structure_by_municipality.csv v2_external_industry_structure"
```

または既存パターンに合わせるなら、`scripts/import_external_csv.py` に `import_industry_structure()` 関数を追加 (現時点では未実装)。

#### Step 2. Turso V2 にすでにある場合

`turso_v2_sync_report_2026-05-04.md` で確認済み。**追加 upload は不要**。Turso V2 を権威ソースと見なし、`scripts/upload_new_external_to_turso.py --refresh` を実行するとローカル CSV (36,099 行) で上書きする可能性があるため、**実行前に行数差分を要確認**。

#### Step 3. 検証 SQL

```sql
-- ローカル DB 投入後
SELECT COUNT(*) FROM v2_external_industry_structure;
-- 期待: 36,099

SELECT COUNT(DISTINCT city_code) FROM v2_external_industry_structure;
-- 期待: 1,719

SELECT COUNT(DISTINCT industry_code) FROM v2_external_industry_structure;
-- 期待: 21

SELECT COUNT(DISTINCT prefecture_code) FROM v2_external_industry_structure;
-- 期待: 47

-- F3 計算で実際に使う行数 (公務 S 除外、集計コード除外)
SELECT COUNT(*) FROM v2_external_industry_structure
WHERE industry_code IN ('C','D','E','F','G','H','I','J','K','L','M','N','O','P','Q','R','AB');
-- 期待: 1,719 × 17 ≈ 29,223 (秘匿値ありで NULL も含む)

-- 札幌市サンプル
SELECT industry_code, industry_name, establishments, employees_total
FROM v2_external_industry_structure
WHERE city_code = '01100'
ORDER BY industry_code;

-- JIS マスタとの突合
SELECT i.city_code, m.municipality_code, COUNT(*) AS cnt
FROM v2_external_industry_structure i
LEFT JOIN municipality_code_master m ON i.city_code = m.municipality_code
WHERE m.municipality_code IS NULL
GROUP BY i.city_code;
-- 期待: 0 件 (industry_structure は @level="2" のみ、municipality_code_master は区 @level="3" を unit で含むため、industry 側が常に部分集合のはず)
```

### 6.2 不採用案: B 案 (Turso V2 から ETL でローカル取り込み)

- 課題: Turso V2 → ローカル sqlite の標準同期スクリプトが現状ない (`verify_turso_v2_sync.py` は READ 比較専用)
- ローカル CSV が既に手元にあるため、Turso 経由は冗長
- 採用しない

### 6.3 不採用案: C 案 (e-Stat 新規 fetch)

- ローカルに既に最新の CSV があり、Turso V2 にも投入済 (turso_v2_sync_report_2026-05-04.md で確認)
- 不要

---

## 7. Worker C への引き継ぎ情報 (F3 計算実装に必要な定数)

### 7.1 使うべきテーブル (優先順)

1. **第一候補**: `v2_external_industry_structure` (JIS コード基準、性別別従業者あり、Phase 3 docs 公式参照先)
2. 第二候補: `v2_external_establishments` (名称ベース、Phase A SSDSE-A 由来、より新しい reference_year を持つ可能性、市区町村名突合が必要)

### 7.2 主要列名

| 役割 | 列名 |
|------|------|
| 市区町村 PK | `city_code` (TEXT, JIS 5 桁、`municipality_code_master.municipality_code` と完全互換) |
| 都道府県 | `prefecture_code` (TEXT, 2 桁、'01'〜'47') |
| 産業 PK | `industry_code` (TEXT, 'AS'/'AR'/'AB'/'CR'/'C'..'S') |
| 産業名 | `industry_name` (TEXT) |
| F3 主要数値 | `employees_total` (INTEGER, 従業者数 男女計) |
| F3 副次 | `establishments` (INTEGER), `employees_male`/`employees_female` (INTEGER) |

### 7.3 年次

- 経済センサス **令和 3 年 (2021)** 活動調査
- DDL に `reference_year` 列なし (industry_structure 側)。`v2_external_establishments` 側には `reference_year INTEGER` あり、Phase A SSDSE-A の年に依存 (要 Worker C 確認)。

### 7.4 推奨 WHERE 句 (F3 で実際に使う産業)

```sql
-- F3 産業構成シェア計算 (公務 S を除外、集計コード AS/AR/CR を除外、AB は維持)
WHERE industry_code IN (
    'AB',  -- 農林漁業
    'C',   -- 鉱業
    'D',   -- 建設業
    'E',   -- 製造業
    'F',   -- 電気・ガス
    'G',   -- 情報通信業
    'H',   -- 運輸業
    'I',   -- 卸売・小売
    'J',   -- 金融・保険
    'K',   -- 不動産
    'L',   -- 学術・専門
    'M',   -- 宿泊・飲食
    'N',   -- 生活関連サービス
    'O',   -- 教育・学習
    'P',   -- 医療・福祉
    'Q',   -- 複合サービス
    'R'    -- サービス (他に分類されない)
)
```

公務 (`S`) は職業大分類との対応が公務員職に偏るため、F3 では除外を推奨 (Worker C 判断)。

### 7.5 産業 → 職業大分類マッピング

- **未確認**: 本調査範囲外。F3 計算では「産業大分類 17 区分 → 職業大分類 (国勢調査基準で 11 区分など)」のマッピングテーブルが別途必要。
- 関連既存資産: Turso V2 上に `v2_industry_mapping` テーブルが存在 (`turso_v2_sync_report_2026-05-04.md` 「リモートのみ存在」セクション)。**Worker C はこのテーブルの DDL とサンプル行を要確認**。
- ローカルにも `scripts/industry_mapping.py` あり (本調査では中身未確認)。

### 7.6 JIS マスタ突合可能性

- `v2_external_industry_structure.city_code` (5 桁) と `municipality_code_master.municipality_code` (5 桁、PK) は **完全互換**。
- `municipality_code_master` には政令市の区 (`area_level='unit', is_designated_ward=1`) が含まれるが、`industry_structure` は親市レベルのみ。**LEFT JOIN で industry 側が常に部分集合**。
- 1,719 (industry) vs 1,917 (master) の差 198 件は政令市の区 (推定、Worker C で SELECT 検証推奨)。

---

## 8. 未確認事項 / 制約事項

| 項目 | 状態 | 必要な追加作業 |
|------|------|---------------|
| Turso V2 上の `v2_external_industry_structure` 実行数 | 未確認 (LOCAL_MISSING のため COUNT 比較スキップ) | `verify_turso_v2_sync.py` のフォークで COUNT + サンプルハッシュを 2 READ 取得 |
| Turso V2 上の `v2_external_establishments` 実行数 | 同上 | 同上 |
| Turso V2 上の `v2_industry_mapping` の DDL / サンプル | 未確認 | Worker C で別途調査 |
| `scripts/industry_mapping.py` の中身 | 未確認 | Worker C で読み込み |
| `reference_year` 列の有無と値 | 未確認 (DDL 上は industry_structure になし、establishments にあり) | ライブクエリで確認 |
| `.env` 経由での Turso ライブクエリ | 制約により本調査では実施せず | ユーザーが OS 環境変数を export 後に再調査可能 |

---

## 9. 安全性チェック

- 本調査は SELECT のみ実施。INSERT/UPDATE/DELETE/DROP/CREATE は一切なし。
- ローカル DB は read-only (`PRAGMA` + `SELECT` のみ)、Turso V2 はアクセスなし (既存レポートのみ参照)。
- token / URL / 認証情報の docs 転記なし。
- 作成ファイル数: 本書 1 ファイルのみ。

---

生成: Worker A (2026-05-04)
参照: `docs/turso_v2_sync_report_2026-05-04.md`, `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_OCCUPATION_POPULATION_MODEL_V2.md`, `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md`, `scripts/upload_new_external_to_turso.py`, `scripts/fetch_industry_structure.py`, `scripts/data/industry_structure_by_municipality.csv`
