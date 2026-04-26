# データソースマップ + タブ依存マトリクス

**最終更新**: 2026-04-26
**対象範囲**: V2 ハローワークダッシュボードで参照する全データソース (4 系統 + 静的 GeoJSON + CSV upload)
**根拠**: `src/main.rs:82-207`、`src/lib.rs:39, 777-788`、`src/db/{local_sqlite.rs, turso_http.rs}`
**マスター**: ルート [`CLAUDE.md`](../CLAUDE.md) §4, §5

---

## 1. データソース 6 系統 (完成形)

| 系統 | 種別 | ローカル/Turso | env var | 接続単位 | 主テーブル | 用途タブ |
|------|------|---------------|---------|---------|----------|---------|
| **A. hellowork.db** | SQLite | ローカル (起動時 gz 解凍) | `HELLOWORK_DB_PATH` | r2d2 max10、WAL、mmap 256MB | postings (469K行) / municipality_geocode / Layer A-C 9 / v2_* 24 / survey_* / ts_agg_* | 全タブ (中核) |
| **B. Turso country-statistics** | libSQL HTTP | Turso 1 系統 | `TURSO_EXTERNAL_URL` `_TOKEN` | spawn_blocking 初期化、timeout 30s | v2_external_* (30+ テーブル、~40K 行) / v2_flow_mesh1km_2019/2020/2021 (38M 行) / v2_flow_master_prefcity / v2_flow_fromto_city / v2_flow_attribute_mesh1km / v2_posting_mesh1km / ts_turso_counts / _salary / _vacancy / _fulfillment / **未投入: v2_flow_city_agg / v2_flow_mesh3km_agg (5/1)** | 詳細分析 / 地域カルテ / 採用診断 / 媒体分析 HW 統合 / 一部 jobmap / insight |
| **C. Turso salesnow** | libSQL HTTP | Turso 2 系統 | `SALESNOW_TURSO_URL` `_TOKEN` | spawn_blocking 初期化、起動キャッシュ無効 (Render OOM) | v2_salesnow_companies (198K 社 × 44列) / v2_industry_mapping / v2_company_geocode | 企業検索 (主) / 採用診断 Panel 4 / 地図 labor-flow / 地図 company-markers |
| **D. Turso audit** | libSQL HTTP | Turso 3 系統 | `AUDIT_TURSO_URL` `_TOKEN` `AUDIT_IP_SALT` | spawn_blocking 初期化、24h purge | accounts / login_sessions / activity / login_failures | `/admin/*` (主) / ログイン履歴 / `/my/activity` |
| **E. GeoJSON 静的** | ファイル | `static/geojson/*.json(.gz)` | (なし) | precompressed_gzip 配信 | 47 都道府県 + 市区町村 polygons | 地図 (jobmap) / 地域カルテ / コロプレス |
| **F. CSV upload** | tower-sessions メモリ | リクエストごと一時 | `UPLOAD_BODY_LIMIT_BYTES` (20MB hard、`lib.rs:39`) | セッション | (Indeed/求人ボックス CSV) | 媒体分析のみ |

---

## 2. graceful degradation マトリクス

| 系統 | 接続失敗時 | 部分機能停止 | UI 表示 |
|------|----------|------------|--------|
| A. hellowork.db | `tracing::warn!` + `None` | 全タブが空応答 | `<div id="db-warning">⚠️ DB接続エラー</div>` (lib.rs:777) |
| B. country-statistics | 同上 | 詳細分析サブタブ + 地域カルテ + 採用診断 + 媒体分析 HW 統合 が空応答 | 各タブで「データなし」または注記 |
| C. salesnow | 同上 | 企業検索 / 採用診断 Panel 4 / labor-flow / company-markers 空応答 | 同上 |
| D. audit | 同上 | `/admin/*` 403、活動記録 OFF | 該当画面のみ |
| E. GeoJSON | warn ログ | 地図 polygon 描画失敗 | 地図白塗 |
| F. CSV upload | 20MB 超 → 413 | (リクエスト失敗) | エラーバナー |

**panic 0 件原則**: 全 Turso 接続は `Option<TursoDb>` で握る (`src/main.rs:82-207`)。HW DB 未接続時も起動可能 (上部赤バナーで通知)。

---

## 3. タブ × データソース 依存マトリクス (9 タブ)

| タブ | A. hellowork.db | B. country-statistics | C. salesnow | D. audit | E. GeoJSON | F. CSV |
|------|:--:|:--:|:--:|:--:|:--:|:--:|
| 1. 市場概況 | ✅ 主 | ✅ v2_external_* | - | - | - | - |
| 2. 地図 | ✅ 主 (postings) | ✅ v2_flow_*, v2_external_* | ✅ v2_salesnow_*, v2_company_geocode | - | ✅ 必須 (47県 polygons) | - |
| 3. 地域カルテ | ✅ 主 | ✅ v2_external_*, v2_flow_* | - | - | ✅ (市区町村 polygon) | - |
| 4. 詳細分析 | ✅ 主 (v2_*, ts_*) | ✅ v2_external_* 30+, ts_turso_* | - | - | - | - |
| 5. 求人検索 | ✅ 主 | - | - | - | - | - |
| 6. 条件診断 | ✅ 主 | ✅ v2_vacancy_rate 等 | - | - | - | - |
| 7. 採用診断 | ✅ 主 (postings) | ✅ v2_external_*, v2_flow_* | ✅ Panel 4 競合 | - | - | - |
| 8. 企業検索 | ✅ (求人結合) | ✅ v2_external_prefecture_stats | ✅ 主 (v2_salesnow_*) | - | - | - |
| 9. 媒体分析 | ✅ (HW enrichment) | ✅ ts_turso_counts | ✅ (企業情報補強) | - | - | ✅ 主 |
| `/admin/*` | - | - | - | ✅ 必須 | - | - |
| `/my/*` | - | - | - | ✅ (推奨) | - | - |
| `/api/v1/*` | ✅ | - | ✅ | - | - | - |
| `/health` | ✅ (確認のみ) | - | - | - | - | - |

✅ = 主データソース。- = 不使用。

---

## 4. データフロー (Python ETL → Rust Dashboard)

```
[原本受領]
   ↓
[A. hellowork.db]
hellowork_etl.py: HW CSV (CP932, 418 列) → postings
hellowork_compute_layers.py → Layer A/B/C 9 テーブル
scripts/compute_v2_*.py × 7 → v2_* 24 テーブル
   ↓ gzip → ~297MB
GitHub Release (db-v2.0)
   ↓ download_db.sh
Render Docker Build → /app/data/hellowork.db
   ↓ 起動時に gunzip + 19 INDEX 自動付与 (main.rs:42-67)

[B/C/D. Turso 3 系統]
ユーザー手動 turso_sync.py 等 (Claude 書き込み禁止)
   ↓ libSQL HTTP
Rust Dashboard (起動時に spawn_blocking で初期化)

[E. GeoJSON]
data/geojson_gz/*.json.gz
   ↓ 起動時に decompress_geojson_if_needed()
static/geojson/*.json
   ↓ precompress_geojson() で gzip 版を準備
配信 (precompressed_gzip)
```

⚠ **Turso 書き込みはユーザー実行のみ** (`feedback_turso_priority`、2026-01 $195 超過請求事故)。

---

## 5. 主要テーブル早見表

### 5.1 ローカル v2_* (24 個、Phase 別)

| Phase | テーブル | 行数 | アルゴリズム概要 |
|-------|---------|------|----------------|
| 1 | v2_vacancy_rate | 34,299 | recruitment_reason_code=1 比率 |
| 1 | v2_regional_resilience | 3,209 | Shannon H, HHI |
| 1 | v2_transparency_score | 34,299 | 8 任意開示項目 |
| 1b | v2_salary_structure | 23,499 | P10/P25/P50/P75/P90 + 推定年収 |
| 1b | v2_salary_competitiveness | 12,446 | 全国比 % |
| 1b | v2_compensation_package | 12,446 | 給与 45%+休日 30%+賞与 25% → S/A/B/C/D |
| 2 | v2_text_quality | 21,490 | 文字数 × ユニーク率 × (1+数字率) |
| 2 | v2_keyword_profile | 128,940 | 6 カテゴリ KW 出現数 |
| 2 | v2_text_temperature | 21,490 | (緊急-選択)/‰ |
| 3 | v2_employer_strategy | 469,027 | 給与 × 福利 4 象限 |
| 3 | v2_employer_strategy_summary | 21,490 | 集計版 |
| 3 | v2_monopsony_index | 21,490 | HHI/Gini/Top1/3/5 |
| 3 | v2_spatial_mismatch | 3,721 | Haversine 30/60km, 孤立度 |
| 3 | v2_cross_industry_competition | 2,192 | 業種重複 |
| 4 | v2_external_minimum_wage | 47 | 2024 年最低賃金 |
| 4 | v2_wage_compliance | 2,174 | 違反率 |
| 4 | v2_region_benchmark | 4,232 | 6 軸ベンチマーク |
| 5 | v2_fulfillment_score | 154,945 | LightGBM 5-fold CV |
| 5 | v2_fulfillment_summary | - | 集計版 |
| 5 | v2_mobility_estimate | 3,721 | 重力モデル |
| 5 | v2_shadow_wage | 12,378 | P10〜P90 |
| 2拡張 | v2_anomaly_stats | 14,788 | 2σ 異常値 |
| 2拡張 | v2_cascade_summary | 19,239 | 都道府県→市区町村→産業 |

### 5.2 Turso country-statistics 主テーブル

`v2_external_*` 30+ テーブル (~40,944 行) の代表:
- `v2_external_population` (人口)
- `v2_external_population_pyramid` (年齢階級)
- `v2_external_migration` (転入転出)
- `v2_external_minimum_wage` / `_history` (最低賃金)
- `v2_external_labor_force` (労働力人口)
- `v2_external_industry_structure` (産業構造)
- `v2_external_medical_welfare` (医療福祉)
- `v2_external_geography` (地理)
- `v2_external_commute_od` (通勤 OD)
- (他 21+ テーブル)

`v2_flow_*` (Agoop 人流):
- `v2_flow_mesh1km_2019/2020/2021` (合計 38M 行)
- `v2_flow_master_prefcity` (市区町村マスター)
- `v2_flow_fromto_city` (流入流出 OD、83% 投入済)
- `v2_flow_attribute_mesh1km` (属性別)
- `v2_posting_mesh1km` (求人 × メッシュ)

`ts_turso_*` (HW 時系列、~16 万行):
- `ts_turso_counts` / `_salary` / `_vacancy` / `_fulfillment`

### 5.3 Turso salesnow

| テーブル | 行数 | 用途 |
|---------|------|------|
| `v2_salesnow_companies` | 198K 社 × 44 フィールド | 信用スコア / 上場 / 事業 / 採用情報 |
| `v2_industry_mapping` | (中) | HW `industry_raw` ↔ SalesNow industry の対応 |
| `v2_company_geocode` | (中) | 企業所在地ジオコード (起動時キャッシュ無効、main.rs:155-158) |

### 5.4 Turso audit

- `accounts`: ユーザーアカウント
- `login_sessions`: アクティブセッション
- `activity`: 操作履歴 (24h purge、`main.rs:222-242`)
- `login_failures`: ログイン失敗履歴

---

## 6. 検証クエリ

### 6.1 ローカル DB

```python
import sqlite3
conn = sqlite3.connect('data/hellowork.db')
print(conn.execute("SELECT COUNT(*) FROM postings").fetchone())  # 469027
print(conn.execute("SELECT COUNT(*) FROM v2_vacancy_rate").fetchone())  # 34299
```

### 6.2 Turso 各系統

```bash
turso db shell country-statistics "SELECT COUNT(*) FROM v2_external_population"
turso db shell country-statistics "SELECT COUNT(*) FROM v2_flow_mesh1km_2021"
turso db shell salesnow "SELECT COUNT(*) FROM v2_salesnow_companies"  # ~198K
turso db shell audit "SELECT COUNT(*) FROM accounts"
```

---

**改訂履歴**:
- 2026-04-26: 新規作成 (P4 / audit_2026_04_24 #10 対応)。Plan P4 §9, §10 から独立マップ化
