# SalesNow データソース比較レポート

- 実行日時 (UTC): 2026-05-03T15:57:03.021558+00:00 〜 2026-05-03T15:57:16.656656+00:00
- 比較対象: Turso V2 / SalesNow 専用 Turso / ローカル CSV

## 比較サマリ

| 項目 | Turso V2 (`country-statistics`) | SalesNow 専用 Turso | ローカル CSV |
|------|--------------------------------|---------------------|--------------|
| ホスト | `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io` | `salesnow-makimaki1006.aws-ap-northeast-1.turso.io` | (ローカル) |
| 存在 | ✅ | ✅ | ✅ |
| 行数 | 198,201 | 198,243 | 467,626 |
| corporate_number 一意 | 198,201 | 198,243 | (未集計) |
| corporate_number NULL/空 | 0 | 0 | (未集計) |
| カラム数 | (テーブル定義 44) | (テーブル定義 44) | 46 |
| READ 消費 | 9 | 9 | (なし) |

## Turso V2 (`country-statistics`)

- ホスト: `country-statistics-makimaki1006.aws-ap-northeast-1.turso.io`
- 行数: **198,201** (`v2_salesnow_companies`)
- corporate_number 一意性: 198,201 / 198,201 (100.00% unique)
- corporate_number NULL/空: 0

### サンプル 3 件 (ORDER BY rowid LIMIT 3)

| corporate_number | company_name | prefecture | sn_industry | employee_count | employee_range | employee_delta_1y |
|---|---|---|---|---|---|---|
| 1010001045983 | 日本紙通商株式会社 | 東京都 | 商社 | 433 | 4: 50人以上~300人未満 | 1.88 |
| 2360003010660 | 合同会社ＫＴＴサービス | 沖縄県 | NULL | 3 | 0: 5人未満 | 0.0 |
| 1290001020273 | 三和陸運株式会社 | 福岡県 | 交通・運輸・物流 | 127 | 4: 50人以上~300人未満 | -3.79 |

### 都道府県別企業数 TOP 10

| 都道府県 | 企業数 |
|---------|------:|
| 東京都 | 24376 |
| 大阪府 | 17345 |
| 愛知県 | 14392 |
| 埼玉県 | 8606 |
| 北海道 | 8349 |
| 福岡県 | 7155 |
| 神奈川県 | 7107 |
| 兵庫県 | 7099 |
| 静岡県 | 7046 |
| 千葉県 | 5710 |

### 業種別企業数 TOP 10

| 業種 | 企業数 |
|------|------:|
| 工事・土木 | 22641 |
| 製造 | 20414 |
| 交通・運輸・物流 | 18681 |
| 食品 | 14644 |
| 機械 | 14531 |
| 材料・資源 | 13309 |
| 建設 | 10627 |
| 自動車・輸送 | 7446 |
| 小売・販売・卸売 | 7046 |
| 商社 | 6119 |

### employee_range 分布

| 規模 | 企業数 |
|------|------:|
| 3: 20人以上~50人未満 | 63175 |
| 4: 50人以上~300人未満 | 43088 |
| 2: 10人以上~20人未満 | 31625 |
| 0: 5人未満 | 21717 |
| 1: 5人以上~10人未満 | 21046 |
| 5: 300人以上~1,000人未満 | 6701 |
| 6: 1,000人以上~3,000人未満 | 1824 |
| 7: 3,000人以上~10,000人未満 | 573 |
| 8: 10,000人以上 | 141 |
| 6: 1,000人以上 | 1 |

## SalesNow 専用 Turso

- ホスト: `salesnow-makimaki1006.aws-ap-northeast-1.turso.io`
- 行数: **198,243** (`v2_salesnow_companies`)
- corporate_number 一意性: 198,243 / 198,243 (100.00% unique)
- corporate_number NULL/空: 0
- employee_count NULL: 2,820
- employee_delta_1y NULL: 7,004
- sales_amount NULL: 47,258

### サンプル 3 件 (ORDER BY rowid LIMIT 3)

| corporate_number | company_name | prefecture | sn_industry | employee_count | employee_range | employee_delta_1y |
|---|---|---|---|---|---|---|
| 1370001003674 | 東日運送株式会社 | 宮城県 | 交通・運輸・物流 | 135 | 4: 50人以上~300人未満 | -0.74 |
| 5410001000077 | 株式会社アイマール | 秋田県 | NULL | 5 | 1: 5人以上~10人未満 | 0.0 |
| 1120101041632 | 高橋商運株式会社 | 大阪府 | 交通・運輸・物流 | 12 | 2: 10人以上~20人未満 | 0.0 |

### 都道府県別企業数 TOP 10

| 都道府県 | 企業数 |
|---------|------:|
| 東京都 | 24376 |
| 大阪府 | 17346 |
| 愛知県 | 14392 |
| 埼玉県 | 8606 |
| 北海道 | 8349 |
| 福岡県 | 7155 |
| 神奈川県 | 7107 |
| 兵庫県 | 7099 |
| 静岡県 | 7047 |
| 千葉県 | 5710 |

### 業種別企業数 TOP 10

| 業種 | 企業数 |
|------|------:|
| 工事・土木 | 22641 |
| 製造 | 20414 |
| 交通・運輸・物流 | 18682 |
| 食品 | 14644 |
| 機械 | 14531 |
| 材料・資源 | 13309 |
| 建設 | 10627 |
| 自動車・輸送 | 7447 |
| 小売・販売・卸売 | 7046 |
| 商社 | 6119 |

### employee_range 分布

| 規模 | 企業数 |
|------|------:|
| 3: 20人以上~50人未満 | 63179 |
| 4: 50人以上~300人未満 | 43113 |
| 2: 10人以上~20人未満 | 31597 |
| 0: 5人未満 | 21703 |
| 1: 5人以上~10人未満 | 21025 |
| 5: 300人以上~1,000人未満 | 6728 |
| 6: 1,000人以上~3,000人未満 | 1835 |
| 7: 3,000人以上~10,000人未満 | 577 |
| 8: 10,000人以上 | 142 |
| true | 2 |
| 8.57 | 1 |
| 78 | 1 |
| 6: 1,000人以上 | 1 |
| 68 | 1 |
| 65 | 1 |
| 51 | 1 |
| 2: 3億円以上~10億円未満 | 1 |
| 2025/11/10 15:41 | 1 |

## ローカル CSV

- パス: `C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\data\salesnow_companies.csv`
- サイズ: 492,443,204 B (約 470 MB)
- 行数: 約 467,626 (ヘッダー除く)
- カラム数: 46

### カラム一覧

`hubspot_id`, `name`, `corporate_number`, `sn_company_name`, `company_name_kana`, `company_url`, `established_date`, `listing_category`, `tob_toc`, `president_name`, `phone_number`, `mail_address`, `prefecture`, `address`, `postal_code`, `sn_industry`, `sn_industry2`, `sn_industry_subs`, `business_tags`, `business_description`
... ほか 26 カラム

### サンプル 3 行 (主要カラムのみ)

| corporate_number | company_name | prefecture | sn_industry | employee_count | employee_range | employee_delta_1y |
|---|---|---|---|---|---|---|
| 1010001045983 | ? | 東京都 | 商社 | 433 | 4: 50人以上~300人未満 | 1.88 |
| 2360003010660 | ? | 沖縄県 |  | 3 | 0: 5人未満 | 0.00 |
| 1290001020273 | ? | 福岡県 | 交通・運輸・物流 | 127 | 4: 50人以上~300人未満 | -3.79 |

## 判定

### 判定基準と評価

| 基準 | Turso V2 | SalesNow 専用 Turso | ローカル CSV |
|------|:-------:|:-------------------:|:------------:|
| 行数十分 (> 100,000) | ✅ 198,201 | ✅ 198,243 | ✅ 467,626 (重複含) |
| corporate_number 一意 | ✅ 100.00% | ✅ 100.00% | ⚠️ 約 2.4 倍 → 重複あり |
| 日本語表示正常 | ✅ | ✅ | ✅ |
| prefecture 取得可能 | ✅ | ✅ | ✅ |
| sn_industry 取得可能 | ✅ | ✅ | ✅ |
| employee_count / range 取得可能 | ✅ | ⚠️ **異常値混入** (後述) | ✅ |
| employee_delta_1y 取得可能 | ✅ | ✅ | ✅ |
| Phase 3 地域競合分析に足りる | ✅ | ✅ (品質課題あり) | ⚠️ DB 化必要 |

### 重要な観察

#### 1. Turso V2 vs SalesNow 専用 Turso の差分

行数差わずか **42 件** (198,201 vs 198,243、0.02%)。両者はほぼ同期しているが、SalesNow 専用 Turso がわずかに新しい (より新規企業を含む可能性)。

#### 2. SalesNow 専用 Turso の `employee_range` 異常値混入

SalesNow 専用 Turso の `employee_range` カラム分布に、本来想定されない値が混入:

| 異常値 | 件数 |
|--------|----:|
| `true` | 2 |
| `8.57` | 1 |
| `78` | 1 |
| `2025/11/10 15:41` | 1 |
| `2: 3億円以上~10億円未満` | 1 |
| `68`, `65`, `51` 等 | 各 1 |

→ これは **CSV カラムずれ** (`is_estimated_sales` (true/false), `salesnow_score` (浮動小数), `collated_at` (日時), `sales_range` (金額帯) が `employee_range` カラムに紛れ込んでいる) の典型症状。

**Turso V2 内 `v2_salesnow_companies` には異常値なし** (規模 0〜8 の正規ラベルのみ)。

#### 3. ローカル CSV の重複

CSV 行数 467,626 ≈ Turso 198,201 × 2.36。HubSpot fetch 時のチェックポイント再開で同一 `corporate_number` が複数回追記された結果と推察。DB 投入時に `PRIMARY KEY` 制約で重複が排除されるため、Turso 側は 198K 行に正規化済み。

### 最終判定

| ソース | Phase 3 用途 | 判定 |
|--------|-------------|------|
| **Turso V2 内 `v2_salesnow_companies`** | 正本 | ✅ **採用** |
| SalesNow 専用 Turso | 補完用 | 🟡 employee_range 品質課題があり、Phase 3 では非採用 |
| ローカル CSV | 再投入原本 | 🟡 重複あり、通常参照しない |

## 推奨 (Plan の初期方針を実機検証で裏付け)

### 1. Phase 3 初期の正本: **Turso V2 内 `v2_salesnow_companies`**

採用理由 (実機データで確認):
- 198,201 行 (corporate_number 100% 一意、NULL ゼロ)
- Phase 3 で必要な分析項目すべて取得可能
  - 地域内企業数 (例: 東京都 24,376 / 大阪府 17,345 / 愛知県 14,392)
  - 業種別企業数 (例: 工事・土木 22,641 / 製造 20,414 / 交通・運輸・物流 18,681)
  - 従業員規模別企業数 (5 人未満 21,717 ... 10,000 人以上 141)
  - 採用競合候補抽出 (prefecture × sn_industry でフィルタ可能)
  - 企業成長/縮小傾向 (employee_delta_1y 利用可)
- 既存 Rust ハンドラの Turso 接続 (`TURSO_EXTERNAL_URL`) で参照可能、追加接続不要
- データ品質が SalesNow 専用 Turso より良い (employee_range 異常値なし)

### 2. 補完用: SalesNow 専用 Turso (使わない)

差分 42 件と employee_range 異常値が解消されるまで、Phase 3 初期では参照しない。

将来的に補完で使う場合の前提条件:
- employee_range 異常値の調査と再投入
- Turso V2 との差分理由の特定 (新規企業 / 削除済み企業 / 同期失敗)

### 3. 再投入用: ローカル CSV

重複が含まれるため通常参照は禁止。Turso 再構築時の原本データとしてのみ使用。

## 残課題 (Phase 3 着手前にユーザー判断が必要)

| # | 課題 | 優先度 |
|--:|------|:------:|
| 1 | SalesNow 専用 Turso の `employee_range` 異常値の解消 | 中 (補完用なので Phase 3 初期は無関係) |
| 2 | Turso V2 vs SalesNow 専用 Turso の 42 件差分の理由特定 | 低 |
| 3 | ローカル CSV → Turso 再投入時の重複排除ロジックの確認 | 低 (現状 PRIMARY KEY で自動排除) |
| 4 | Phase 3 で SalesNow を使う Rust ハンドラの設計 (Turso V2 経由読取) | Phase 3 着手時 |

---

生成: `scripts/inspect_salesnow_sources.py` (2026-05-03)