# 賃貸データ (e-Stat 住宅・土地統計調査) 投入手順書

**作成日**: 2026-05-31
**対象**: Phase 2 案 R-A (`docs/audit_2026_04_24/survey_data_activation_plan.md:831-863`)
**配置先**: `docs/RENTAL_DATA_INGEST_2026_05_31.md` (この draft は `src/handlers/survey/_drafts_rental_2026_05_31/` にあるため parent でコピー)

---

## 🔴 重要前提

1. **Claude (AI) による DB 書き込みは禁止**。すべての `upload_new_external_to_turso.py` 実行は必ずユーザー (人間) が手動で実行する。
   - 根拠: MEMORY `feedback_turso_upload_once.md` + 2026-01-06 $195 超過請求事故
2. **Turso 無料枠の制約**: アップロードは「ローカル検証完了後に 1 回だけ」。何度も DROP + CREATE しない。
3. **対象データソース**:
   - 統計表: 住宅・土地統計調査 (総務省統計局)
   - 政府統計コード: `00200522`
   - 実施年: 2023 年実施 / 2024 年公表
   - 5 年周期 (前回 2018 年実施)

---

## 投入手順

### Step 1: statsDataId 候補一覧の取得

住宅・土地統計は表が多数存在 (借家数、家賃中央値、構造別、面積別、用途別など多軸)。
本実装では「市区町村×構造×面積階級」のクロス表を選定するため、まず候補一覧を取得する。

```powershell
$env:ESTAT_APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts/fetch_rental_housing.py --metadata-only
```

**期待出力**:
- `scripts/data/rental_housing_stats_list.json` (raw レスポンス、~数 MB)
- 標準出力に「借家/家賃/構造/専有面積/民営/公営」を含む表一覧 (最大 50 件)
- 各表に `statsDataId` (例: `0003228366`) と SURVEY_DATE が併記

**選定基準** (上から優先):
1. SURVEY_DATE が 2023 年 (=`202300` or `202301-202312`) を含む
2. タイトルに「借家」「家賃」「構造別」「専有面積階級別」「市区町村」がすべて含まれる
3. 単独表 (集計表ではなく統計表) であること

### Step 2: STATS_DATA_ID 定数の手動更新

Step 1 で確定した statsDataId を `scripts/fetch_rental_housing.py` に反映:

```python
# scripts/fetch_rental_housing.py の冒頭
STATS_DATA_ID = "0003XXXXXX"  # ← Step 1 で確定した値に置き換え
```

🔴 **暫定値 `0003228366` のままで実行しないこと**。これは 2018 年実施分の暫定値。

### Step 3: 軸構造の確認 (任意・推奨)

```powershell
python scripts/fetch_rental_housing.py --inspect-meta
```

**期待出力**:
- `scripts/data/rental_housing_meta_info.json` (raw メタ、~数百 KB)
- 標準出力に axis 一覧 (area, cat01=構造, cat02=面積階級, tab=表章項目 など)
- 各 axis の code/name 上位 20 件

**確認ポイント**:
- `tab` に「家賃」「住戸数」相当の項目が存在するか
- `area` に市区町村レベル (5 桁、末尾 `000` でない) コードが含まれるか
- `cat01` (構造) の distinct 数が 4-6 程度 (木造/防火木造/RC/鉄骨等)
- `cat02` (面積階級) の distinct 数が 4-6 程度

確認結果が想定と乖離する場合は Step 1 に戻って別 statsDataId を選び直す。

### Step 4: サンプル取得 (任意・推奨)

```powershell
python scripts/fetch_rental_housing.py --sample-only
```

**期待出力**: `scripts/data/rental_housing_sample.json` (1000 行) + 最初の 5 行を標準出力に表示

データ構造を目視確認し、tab/cat01/cat02 のコードが期待通りか検証。

### Step 5: 本実行 (CSV 出力)

```powershell
python scripts/fetch_rental_housing.py --fetch
```

**所要時間**: 約 100-150 市区町村 × 1 リクエスト/秒 = **3-5 分**
- 進捗は 20 件ごとに標準出力
- 中断時は `scripts/data/rental_housing.progress` で resume 可能
- 失敗時は `--reset` で最初からやり直し可能

**期待出力**: `scripts/data/rental_housing_2026.csv`

### Step 6: CSV validation

```powershell
python scripts/fetch_rental_housing.py --validate
```

**検証項目** (スクリプト内蔵):
1. 行数 > 0
2. 必須カラム存在 (prefecture, municipality, structure, area_class, rental_total_units, median_rent_jpy, as_of, fetched_at)
3. 都道府県カバレッジ (47 県揃っているか → WARN: missing prefectures)
4. 市区町村カバレッジ (50-1900 の範囲 → WARN)
5. median_rent_jpy が正値か (10,000 - 500,000 円/月 の範囲 → WARN)
6. rental_total_units が非負か (負値 → NG)
7. PK 重複 (prefecture, municipality, structure, area_class)
8. structure / area_class の distinct 数 (ログ出力)
9. as_of が `2023` を含むか

**OK 判定**: `[OK] CSV validation passed` が表示されたら次へ。

### Step 7: Turso 投入 (ユーザー手動実行)

🔴 **ここから Claude は実行しない**。ユーザーが手動で 1 回だけ実行。

```powershell
# 事前に .env で TURSO_EXTERNAL_URL / TURSO_EXTERNAL_TOKEN を設定
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# dry-run で内容確認 (DB 書き込みなし)
python scripts/upload_new_external_to_turso.py --dry-run --table v2_external_rental_housing

# 本投入 (1 回だけ実行)
python scripts/upload_new_external_to_turso.py --table v2_external_rental_housing
```

**期待出力**:
- `v2_external_rental_housing: N 行アップロード完了` (N は CSV 行数)
- 検証ログに `v2_external_rental_housing: N 行`

### Step 8: 投入後検証 SQL

Turso DB に接続して以下の SQL を実行:

```sql
-- 1. 行数確認
SELECT COUNT(*) FROM v2_external_rental_housing;

-- 2. 47 都道府県カバレッジ
SELECT COUNT(DISTINCT prefecture) AS pref_count
FROM v2_external_rental_housing
WHERE prefecture != '全国';
-- 期待: 47

-- 3. 市区町村数
SELECT COUNT(DISTINCT prefecture || '|' || municipality) AS muni_count
FROM v2_external_rental_housing
WHERE municipality != '';
-- 期待: 約 100 (案 R-A 想定)

-- 4. 家賃中央値が 0 超
SELECT
  prefecture,
  municipality,
  structure,
  area_class,
  median_rent_jpy
FROM v2_external_rental_housing
WHERE median_rent_jpy IS NOT NULL
ORDER BY median_rent_jpy DESC
LIMIT 10;
-- 期待: 東京/神奈川/大阪が上位、値は 50,000-200,000 円

-- 5. 家賃 NULL 率 (借家集計のみで家賃なしの行は多い想定)
SELECT
  COUNT(*) AS total,
  SUM(CASE WHEN median_rent_jpy IS NULL THEN 1 ELSE 0 END) AS null_rent,
  ROUND(100.0 * SUM(CASE WHEN median_rent_jpy IS NULL THEN 1 ELSE 0 END) / COUNT(*), 1) AS null_rate_pct
FROM v2_external_rental_housing;

-- 6. 構造 distinct
SELECT structure, COUNT(*) AS cnt
FROM v2_external_rental_housing
GROUP BY structure ORDER BY cnt DESC;

-- 7. 面積階級 distinct
SELECT area_class, COUNT(*) AS cnt
FROM v2_external_rental_housing
GROUP BY area_class ORDER BY cnt DESC;
```

**合格基準**:
- 47 都道府県すべて存在 (pref_count = 47)
- 市区町村は 50-200 の範囲
- 家賃中央値の上位は東京/神奈川/大阪
- 家賃中央値が 10,000 - 500,000 円/月 の範囲に収まる

### Step 9: ロールバック手順 (必要時のみ)

万一データが不正だった場合のロールバック:

```sql
-- Turso 上で実行 (ユーザー手動)
DROP TABLE IF EXISTS v2_external_rental_housing;
```

再投入時:

```powershell
# CSV 再生成
python scripts/fetch_rental_housing.py --fetch --reset

# 再投入 (refresh フラグで DROP + CREATE)
python scripts/upload_new_external_to_turso.py --table v2_external_rental_housing --refresh
```

🔴 `--refresh` も Claude は実行しない。ユーザー手動のみ。

---

## 失敗時のトラブルシューティング

### A. `--metadata-only` で表が見つからない

- 政府統計コード `00200522` が正しいか e-Stat ポータルで確認
  - https://www.e-stat.go.jp/stat-search/files?page=1&toukei=00200522
- ESTAT_APP_ID の権限・有効性を確認
- API 制限 (1 日あたり) に達していないか確認

### B. `--fetch` で全 area が skipped

- `tab` 軸の家賃/住戸数識別キーワードが期待と異なる可能性
- スクリプト内 `rent_tab_codes` / `unit_tab_codes` の判定ロジックを `--inspect-meta` で確認したコードに合わせて調整
- 改修箇所: `run_fetch()` 内の `keywords` リスト

### C. `--validate` で家賃が 0 ばかり

- 選定した statsDataId が「住戸数のみの表」だった可能性
- Step 1 で別の表 (家賃を含む) を選び直し
- 住戸数と家賃が別表の場合、2 つの statsDataId を順次 fetch して CSV をマージする実装が必要 (現状未対応)

---

## 関連ドキュメント

| ドキュメント | 参照箇所 | 内容 |
|------|------|------|
| `docs/audit_2026_04_24/survey_data_activation_plan.md` | 831-863 | 案 R-A 仕様 |
| `docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_STEP5_PREREQ_INGEST_PLAN.md` | 104-124 | living_cost_proxy 派生計画 |
| `scripts/fetch_estat_15_1.py` | 全体 | e-Stat API 既存実装パターン |
| `scripts/fetch_industry_structure.py` | 全体 | 市区町村ループ + 進捗管理パターン |
| `scripts/upload_new_external_to_turso.py` | TABLE_SCHEMAS 等 | Turso 投入インフラ |
| memory `project_pending_rental_data_2026_05.md` | - | Phase 2 待機事項 |
| memory `feedback_turso_upload_once.md` | - | Turso 投入は 1 回限り |
| memory `feedback_never_guess_data.md` | - | 「正常」断言禁止、SQL 結果提示 |

---

## 改訂履歴

| 日付 | 内容 |
|------|------|
| 2026-05-31 | 新規作成 (Phase 2 案 R-A 投入手順、Turso リセット後実装) |
