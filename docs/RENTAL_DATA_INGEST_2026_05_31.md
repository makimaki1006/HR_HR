# 賃貸データ (e-Stat 住宅・土地統計調査) 投入手順書

**作成日**: 2026-05-31
**最終更新**: 2026-05-31 (PR fix/rental-data-sqm-rate — m² 単価ベース仕様変更)
**配置先**: `docs/RENTAL_DATA_INGEST_2026_05_31.md`

---

## 🔴 重要前提 — m² 単価ベース仕様 (2026-05-31 PR fix/rental-data-sqm-rate)

### 仕様変更の背景

当初設計 (PR #8/#9) は「1ヶ月家賃 中央値」を取得して家賃負担率を出す想定だったが、
e-Stat 住宅・土地統計 2023 (statsCode=00200522) の市区町村粒度表に存在するのは
**延べ面積 1m² 当たり家賃 (円/m²)** のみ。

ユーザー判断:「m² 単価 (円/m²) が土地の金額とほぼ連動するため、地域コスト指標として活用する」
=> **生値の m² 単価を地域コスト指標として活用** する形に変更。

### 実体と列名のマッピング (DB column 名互換のため列名は変えていない)

| CSV/DB 列名             | 実体                                                | 単位/値域           |
|------------------------|----------------------------------------------------|--------------------|
| `prefecture`           | 都道府県名 (例: 東京都) / "全国"                        | -                  |
| `municipality`         | 市区町村名 (例: 新宿区) / 都道府県集計と全国は空        | -                  |
| `structure`            | **建て方** (cat01: 一戸建/長屋建/共同住宅/その他/総数)   | -                  |
| `area_class`           | **構造** (cat02: 木造/非木造/総数)                      | -                  |
| `rental_total_units`   | **NULL 固定** (本表には住戸数なし)                       | -                  |
| `median_rent_jpy`      | **m² 単価 (1m² 当たり月家賃)**                          | 円/m² (100〜30,000) |
| `as_of`                | データ基準年                                            | "2023"             |
| `fetched_at`           | 取得日時 ISO8601 UTC                                   | -                  |

### 想定値域 (m² 単価)

- **100〜30,000 円/m²** (月額)
- 地方の最安帯: 約 500 円/m²
- 東京都心の高額帯: 約 5,000〜10,000 円/m²
- 例: 東京都中央区 一戸建 木造 ≒ 1,500〜2,500 円/m²

### 表 ID 確定情報

| 項目          | 値                                                       |
|--------------|---------------------------------------------------------|
| 政府統計コード | `00200522` (住宅・土地統計調査)                            |
| **statsDataId** | **`0004021493`** (借家 専用住宅 延べ面積 1m² 当たり家賃) |
| 実施年        | 2023 年実施 / 2024 年公表 (5 年周期、前回 2018 年)         |
| 軸構成        | cat01(建て方 5) × cat02(構造 3) × cat03(2) × area(1283) × time(2023) |
| 取得時 cat03  | 「家賃０円を含まない」のみ (より代表的)                     |
| 想定行数      | 約 38,490 data points / 1,283 area = 約 38,490 行         |

### 運用上の注意

1. **Claude (AI) による DB 書き込みは禁止**。すべての `upload_new_external_to_turso.py` 実行は必ずユーザー (人間) が手動で実行する。
   - 根拠: MEMORY `feedback_turso_upload_once.md` + 2026-01-06 $195 超過請求事故
2. **Turso 無料枠の制約**: アップロードは「ローカル検証完了後に 1 回だけ」。何度も DROP + CREATE しない。
3. **列名は変えない**: コードベース既存参照との互換性のため、`median_rent_jpy`/`structure`/`area_class` の列名は維持。実体の違いはドキュメントとコメントで補う。

---

## 投入手順

### Step 1: statsDataId 候補一覧の取得 (任意・確認用)

`STATS_DATA_ID = "0004021493"` は確定済み。再選定が必要な場合のみ実行:

```powershell
$env:ESTAT_APP_ID = "85f70d978a4fd0da6234e2d07fc423920e077ee5"
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy
python scripts/fetch_rental_housing.py --metadata-only
```

**期待出力**:
- `scripts/data/rental_housing_stats_list.json` (raw レスポンス、~数 MB)
- 標準出力に「借家/家賃/構造/専有面積/民営/公営/1m/1㎡」を含む表一覧 (最大 50 件)

### Step 2: 軸構造の確認 (任意・推奨)

```powershell
python scripts/fetch_rental_housing.py --inspect-meta
```

**期待出力**:
- `scripts/data/rental_housing_meta_info.json`
- 標準出力に axis 一覧:
  - `cat01` 建て方: 一戸建 / 長屋建 / 共同住宅 / その他 / 総数 (5)
  - `cat02` 構造  : 木造 / 非木造 / 総数 (3)
  - `cat03`       : 家賃０円を含む / 家賃０円を含まない (2)
  - `area`        : 全国 / 47 県 / 市区町村 (約 1,283)
  - `time`        : 2023年 (1)

### Step 3: サンプル取得 (任意・推奨)

```powershell
python scripts/fetch_rental_housing.py --sample-only
```

**期待出力**: `scripts/data/rental_housing_sample.json` (1,000 行) + 最初の 5 行を標準出力に表示

データ構造を目視確認し、`@value` の値が想定値域 100〜30,000 (円/m²) に収まることを検証。

### Step 4: 本実行 (CSV 出力)

```powershell
python scripts/fetch_rental_housing.py --fetch
```

**所要時間**: 約 1,283 area × 1 リクエスト/秒 = **約 21 分**
- 進捗は 20 件ごとに標準出力
- 中断時は `scripts/data/rental_housing.progress` で resume 可能
- 失敗時は `--reset` で最初からやり直し可能
- cat03 = 「家賃０円を含まない」を API で絞り込むため取得量は半減

**期待出力**: `scripts/data/rental_housing_2026.csv` (約 38,490 行)

### Step 5: CSV validation

```powershell
python scripts/fetch_rental_housing.py --validate
```

**検証項目** (スクリプト内蔵):
1. 行数 > 0
2. 必須カラム存在 (8 列)
3. 都道府県カバレッジ (47 県揃っているか → WARN: missing prefectures)
4. 市区町村カバレッジ (50-2,000 の範囲 → WARN)
5. **median_rent_jpy (m² 単価)** が想定値域 (100〜30,000 円/m²) に収まるか → WARN
   - 100 未満 → 100 倍ずれ (× 0.01) の疑い
   - 30,000 超 → 100 倍ずれ (× 100) 又は月家賃混入の疑い
6. `rental_total_units` は NULL 固定の想定 → 非 NULL 行があれば WARN
7. PK 重複 (prefecture, municipality, structure, area_class)
8. structure (建て方) / area_class (構造) の distinct 数
9. as_of が `2023` を含むか

**OK 判定**: `[OK] CSV validation passed` が表示されたら次へ。

### Step 6: Turso 投入 (ユーザー手動実行)

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
- `v2_external_rental_housing: N 行アップロード完了` (N ≈ 38,490)

### Step 7: 投入後検証 SQL

Turso DB に接続して以下の SQL を実行:

```sql
-- 1. 行数確認 (期待: ~38,490)
SELECT COUNT(*) FROM v2_external_rental_housing;

-- 2. 47 都道府県 + "全国" のカバレッジ
SELECT COUNT(DISTINCT prefecture) AS pref_count
FROM v2_external_rental_housing;
-- 期待: 48 (47 県 + 全国)

-- 3. 市区町村数
SELECT COUNT(DISTINCT prefecture || '|' || municipality) AS muni_count
FROM v2_external_rental_housing
WHERE municipality != '';
-- 期待: 約 1,235

-- 4. m² 単価 (median_rent_jpy 列の実体) が想定値域に収まるか
SELECT
  prefecture,
  municipality,
  structure,    -- 建て方
  area_class,   -- 構造
  median_rent_jpy AS sqm_rate_jpy  -- m² 単価 (円/m², 月額)
FROM v2_external_rental_housing
WHERE median_rent_jpy IS NOT NULL
ORDER BY median_rent_jpy DESC
LIMIT 10;
-- 期待: 東京都 (中央区/港区/千代田区) が上位、値は 3,000-10,000 円/m²

-- 5. 全国基準値 (Rust 側の表 7-H で基準値として利用)
SELECT structure, area_class, median_rent_jpy
FROM v2_external_rental_housing
WHERE prefecture = '全国' AND municipality = '';
-- 期待: structure=総数, area_class=総数 で約 1,500 円/m²

-- 6. 範囲外チェック (異常値検出)
SELECT COUNT(*) FROM v2_external_rental_housing
WHERE median_rent_jpy IS NOT NULL
  AND (median_rent_jpy < 100 OR median_rent_jpy > 30000);
-- 期待: 0 (想定値域 100-30,000 円/m² 外は単位ずれ疑い)

-- 7. structure (建て方) distinct
SELECT structure, COUNT(*) AS cnt
FROM v2_external_rental_housing
GROUP BY structure ORDER BY cnt DESC;
-- 期待: 一戸建/長屋建/共同住宅/その他/総数

-- 8. area_class (構造) distinct
SELECT area_class, COUNT(*) AS cnt
FROM v2_external_rental_housing
GROUP BY area_class ORDER BY cnt DESC;
-- 期待: 木造/非木造/総数
```

**合格基準**:
- 47 都道府県 + 全国 すべて存在
- 全国レコード (基準値) に「総数 × 総数」行が存在
- m² 単価が 100-30,000 円/m² の範囲に収まる
- 範囲外行 = 0

### Step 8: ロールバック手順 (必要時のみ)

```sql
-- Turso 上で実行 (ユーザー手動)
DROP TABLE IF EXISTS v2_external_rental_housing;
```

再投入時:

```powershell
python scripts/fetch_rental_housing.py --fetch --reset
python scripts/upload_new_external_to_turso.py --table v2_external_rental_housing --refresh
```

🔴 `--refresh` も Claude は実行しない。ユーザー手動のみ。

---

## Rust 側 (Section 07 表 7-H) の動作仕様

PR fix/rental-data-sqm-rate により、`section_07_lifestyle.rs` の
`build_navy_rental_vs_salary_table()` は以下のロジックで動作する:

1. **基準値抽出**: `prefecture="全国" AND municipality=""` の m² 単価を全国平均として取得。
   - 優先順: 「総数 × 総数」 → 全国レコードの中央値 → 全データ中央値 (fallback)
2. **対象地域行抽出**: 全国以外の prefecture を持つレコードを建て方 × 構造でグルーピング。
3. **比率計算**: 対象 m² 単価 / 全国 m² 単価 × 100 を「全国平均比」として表示。
4. **想定 50m² 月家賃**: m² 単価 × 50 を概算月家賃 (1LDK 相当) として併記。
5. **月給カバー率**: 月給中央値 / 想定月家賃 × 100。
6. **判定** (中立表現):
   - 全国平均比 ≤ 70% → 家賃低水準 (給与訴求の余地あり)
   - 70-130% → 全国標準水準
   - > 130% → 家賃高水準 (家賃補助検討余地あり)
7. **単位防御** (Round 1-K): m² 単価が 100〜30,000 円/m² 外なら `debug_assert!` + `tracing::warn!`。

### 表示例

| 建て方   | 構造   | m² 単価 (円/m²) | 全国平均比 | 想定 50m² 月家賃 (円) | 月給カバー率 | 判定         |
|---------|--------|----------------|-----------|---------------------|------------|-------------|
| **総数** | **総数** | **2,500**     | **167%**  | **125,000**         | **200%**   | **家賃高水準** |
| 共同住宅 | 非木造  | 3,000          | 200%      | 150,000             | 167%       | 家賃高水準    |
| 一戸建   | 木造    | 1,800          | 120%      | 90,000              | 278%       | 全国標準水準  |

(全国平均 = 1,500 円/m², 月給中央値 = 250,000 円 想定)

---

## 失敗時のトラブルシューティング

### A. `--fetch` で全 area が skipped

- `cat03` 軸の「家賃０円を含まない」code が解決できなかった可能性
- `--inspect-meta` で cat03 axis の実体を確認、`_resolve_target_cat03_code()` のキーワードを調整
- 改修箇所: `scripts/fetch_rental_housing.py` の `TARGET_CAT03_KEYWORD`

### B. `--validate` で「m² 単価 < 100」の WARN が大量に出る

- 取得した値が比率や指数として返されている可能性
- `--inspect-meta` で表章単位 (`@unit`) を確認
- 代替案: 別の statsDataId (例: 平均家賃) を検討

### C. `--validate` で「m² 単価 > 30,000」の WARN が出る

- 月家賃ベースの表 (PR #8 当初想定) が混入している可能性
- 取得した statsDataId が `0004021493` (1m² 当たり家賃) であることを `--inspect-meta` で再確認

---

## 関連ドキュメント

| ドキュメント                                                | 内容                                  |
|-----------------------------------------------------------|--------------------------------------|
| `docs/audit_2026_04_24/survey_data_activation_plan.md`    | 案 R-A 原仕様 (1ヶ月家賃中央値 想定)    |
| `scripts/fetch_estat_15_1.py`                             | e-Stat API 既存実装パターン            |
| `scripts/fetch_industry_structure.py`                     | 市区町村ループ + 進捗管理パターン       |
| `scripts/upload_new_external_to_turso.py`                 | Turso 投入インフラ                    |
| `src/handlers/survey/report_html/navy_report/section_07_lifestyle.rs` | 表 7-H 描画ロジック (m² 単価対応) |
| memory `project_pending_rental_data_2026_05.md`           | Phase 2 待機事項                      |
| memory `feedback_turso_upload_once.md`                    | Turso 投入は 1 回限り                  |
| memory `feedback_never_guess_data.md`                     | 「正常」断言禁止、SQL 結果提示          |
| memory `feedback_unit_consistency_audit.md`               | 単位の一貫性監査 (Round 1-K)           |

---

## 改訂履歴

| 日付       | 内容                                                            |
|-----------|----------------------------------------------------------------|
| 2026-05-31 | 新規作成 (Phase 2 案 R-A 投入手順、Turso リセット後実装)            |
| 2026-05-31 | PR fix/rental-data-sqm-rate: m² 単価ベース仕様変更。statsDataId 0004021493 確定。Rust 側 表 7-H を「家賃 m² 単価 (地域コスト指標)」に変更。`rental_total_units` NULL 固定、`median_rent_jpy` 実体は m² 単価。 |
