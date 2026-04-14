# V2 ハローワークダッシュボード マスターリファレンス

**最終更新**: 2026-03-14
**リポジトリ**: `makimaki1006/HR_HR`
**デプロイ**: Render (hellowork-dashboard)

---

## 🔴 絶対ルール

| ルール | 理由 |
|--------|------|
| `git add -A` / `git add .` 禁止 | 2026-03-10にdata/geojson_gz/ 47ファイル誤削除事故 |
| コミット前に `git diff --cached --stat` で削除確認 | バイナリ(.gz,.db)削除があれば即停止 |
| DB書き込みはユーザー実行のみ | Claudeによる$195超過請求事故(2026-01) |
| 推測を事実として報告しない | 未検証なら「未確認」「要検証」と明記 |
| V1(ジョブメドレー)とV2(ハローワーク)を混同しない | 別リポ・別DB・別雇用形態 |

---

## 1. プロジェクト概要

| 項目 | 値 |
|------|-----|
| データソース | ハローワーク求人情報 |
| 技術スタック | Rust (Axum 0.8) + HTMX 2.0 + ECharts 5.5 + Leaflet 1.9 |
| DB | SQLite 1個 (hellowork.db, ~1.6GB) |
| 求人数 | 469,027件 |
| 分析テーブル | 31テーブル (postings + municipality_geocode + layer_a/b/c 9個 + v2_* 24個) |
| 雇用形態 | **正社員**（V1の「正職員」とは異なる） |
| フィルタ | 都道府県 → 市区町村 → 産業（2階層ツリー） |
| タブ | 8タブ + 市場分析6サブタブ + 市場診断 |
| ポート | 9216 |

### V1/V2 分離

| | V1: ジョブメドレー | V2: ハローワーク |
|---|---|---|
| リポ | `makimaki1006/rust-dashboard` | `makimaki1006/HR_HR` |
| デプロイリポ | `rust-dashboard-deploy/` | `hellowork-deploy/` |
| DB | 2ローカル + Turso + GitHub Releases | 1個 (hellowork.db) |
| 雇用形態 | 正職員 | 正社員 |
| フィルタ | 職種→都道府県 | 都道府県→市区町村→産業 |

**混同禁止**: V2コードをV1リポにpush禁止 / V1のDB構造(3DB)をV2に適用禁止 / 雇用形態用語の混同禁止

---

## 2. アーキテクチャ

### ディレクトリ構造

```
hellowork-deploy/
├── Cargo.toml, Dockerfile, render.yaml
├── src/
│   ├── main.rs             # エントリーポイント（DB解凍、インデックス作成、サーバ起動）
│   ├── lib.rs              # ルーター定義 (build_app)、GeoJSON解凍
│   ├── config.rs           # AppConfig (環境変数)
│   ├── auth/               # 認証 (ドメイン+パスワード+レート制限)
│   ├── db/
│   │   ├── local_sqlite.rs # r2d2プール(max10) + PRAGMA(WAL/mmap256MB)
│   │   └── cache.rs        # DashMapキャッシュ (TTL30分, max3000)
│   └── handlers/
│       ├── helpers.rs       # 共通: escape_html, format_number, get_str/i64/f64等
│       ├── overview.rs      # 📊 地域概況 + SessionFilters構造体
│       ├── demographics.rs  # 📋 採用動向
│       ├── balance.rs       # 🏢 企業分析
│       ├── workstyle.rs     # 💰 求人条件
│       ├── diagnostic.rs    # 🩺 市場診断（条件入力→6軸レーダー診断）
│       ├── api.rs           # フィルタAPI, GeoJSON配信
│       ├── jobmap/          # 🗺️ 求人地図 (Leaflet)
│       ├── competitive/     # 🔍 詳細検索 (テーブル表示+個別求人)
│       └── analysis/        # 📈 市場分析 (6サブタブ、v2_*テーブル全表示)
│           ├── handlers.rs  # tab_analysis + analysis_subtab
│           ├── fetch.rs     # 22 fetch関数 + query_3level
│           ├── render.rs    # 6 render_subtab + 28 render関数 + ECharts
│           └── helpers.rs   # 色判定関数, ANALYSIS_SUBTABS定数
├── templates/dashboard_inline.html  # メインテンプレート
├── static/css/, static/js/         # ダークテーマCSS, ECharts/Leaflet等
├── data/hellowork.db               # メインDB (git非追跡)
├── data/geojson_gz/                # 47都道府県GeoJSON (gzip)
└── scripts/compute_v2_*.py         # Python事前計算 (7本)
```

### データフロー

```
[Python事前計算]                       [Rustダッシュボード]
compute_v2_*.py (7本)             ┌─→ 8タブ + 6サブタブ
    ↓                             │   (spawn_blocking + DashMapキャッシュ)
hellowork.db (31テーブル)         │
    ↓ gzip (~297MB)              │
GitHub Release (db-v2.0)          │
    ↓ download_db.sh             │
Dockerビルド時にDL ───────────────┘
```

### 主要設計パターン

| パターン | 実装 |
|---------|------|
| 3レベルフィルタ | `query_3level()`: 市区町村→都道府県→全国で自動フォールバック |
| 雇用形態セグメント | 全v2_*テーブルに `emp_group` (正社員/パート/その他) |
| spawn_blocking | sync DB(r2d2)をtokioスレッドプールで非同期化 |
| HTMXパーシャル | 全ハンドラーが `Html<String>` 返却、JS最小化 |
| ECharts自動初期化 | `<div class="echart" data-chart-config='JSON'>` + htmx:afterSettle |
| XSS防止 | `escape_html()` で全DB文字列をエスケープ |
| キャッシュ | DashMap TTL30分、キー=`{tab}_{industry}_{pref}_{muni}` |

---

## 3. 原本データ投入パイプライン

### 全体フロー（求人データ原本を受け取ったとき）

```
Step 1: 原本データをpostingsテーブルに投入
        → hellowork_etl.py: ハローワークCSV(CP932,418列) → hellowork.db

Step 2: Layer A/B/C計算
        → hellowork_compute_layers.py → 9テーブル追加

Step 3: V2分析テーブル計算（7スクリプトを順番に実行）
        → 24個のv2_*テーブルを生成

Step 4: DB圧縮 + GitHub Releaseアップロード
        → gzip -c data/hellowork.db > data/hellowork.db.gz
        → gh release upload db-v2.0 data/hellowork.db.gz --clobber --repo makimaki1006/HR_HR

Step 5: Renderデプロイ
        → Renderダッシュボードから Manual Deploy
```

### Step 3 詳細: Python事前計算 (実行順序が重要)

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# Phase 1: 基本指標（依存なし）→ v2_vacancy_rate, v2_regional_resilience, v2_transparency_score
python scripts/compute_v2_analysis.py

# Phase 1b: 給与分析（依存なし）→ v2_salary_structure, v2_salary_competitiveness, v2_compensation_package
python scripts/compute_v2_salary.py

# Phase 2: テキスト分析（依存なし）→ v2_text_quality, v2_keyword_profile, v2_text_temperature
python scripts/compute_v2_text.py

# Phase 3: 市場構造（postingsのlat/lon必要）→ v2_employer_strategy*, v2_monopsony_index, v2_spatial_mismatch
python scripts/compute_v2_market.py

# Phase 4: 外部データ統合（★Phase1-2の結果テーブルに依存）
#   参照: v2_vacancy_rate, v2_salary_competitiveness, v2_regional_resilience,
#          v2_transparency_score, v2_text_temperature
python scripts/compute_v2_external.py

# Phase 5: 予測モデル（scikit-learn or lightgbm必要）
python scripts/compute_v2_prediction.py

# Phase 2拡張: テキスト温度計・異業種競合・異常値・カスケード
python scripts/compute_v2_phase2.py
```

### Step 4-5: DB更新デプロイ手順

```bash
# 圧縮（数分かかる、1.6GB→~297MB）
gzip -c data/hellowork.db > data/hellowork.db.gz

# GitHub Releaseにアップロード（--clobberで上書き）
gh release upload db-v2.0 data/hellowork.db.gz --clobber --repo makimaki1006/HR_HR

# Render再デプロイ → ダッシュボードから Manual Deploy
```

---

## 4. DBスキーマ

### 4.1 コアテーブル

**postings** (469,027行, 121カラム): 求人票の全データ

| カテゴリ | 主要カラム |
|---------|----------|
| 識別 | id(PK), job_number, job_type |
| 地域 | prefecture, municipality, latitude, longitude, access |
| 施設 | facility_name, employee_count, founding_year, capital |
| 給与 | salary_min/max, salary_type, bonus_months, base_salary_min/max |
| 雇用 | employment_type, age_min/max, education_required |
| 労働条件 | working_hours, annual_holidays, overtime_monthly, trial_period_months |
| 福利厚生フラグ(17個) | has_社会保険, has_退職金, has_賞与, has_昇給, has_育児休業 等 |
| テキスト分析 | text_entropy, kanji_ratio, benefits_score(0-32), content_richness_score |
| セグメント | tier1_salary/benefits/worklife/stability/growth, tier3_label_short |
| 募集 | hello_work_office, recruitment_reason_code(1=欠員,2=増員,3=新設), recruitment_count |

インデックス: 29個（prefecture, municipality, job_type等の単一+複合）

**municipality_geocode** (2,626行): 47都道府県×562市区町村の緯度経度

### 4.2 Layer A-C テーブル（9テーブル）

| テーブル | 行数 | 内容 |
|---------|------|------|
| layer_a_salary_stats | 687 | 職種×都道府県の給与統計(P25/P50/P75/P90, Gini) |
| layer_a_facility_concentration | 622 | HHI/Zipf指数で施設集中度 |
| layer_a_employment_diversity | 622 | Shannon entropy雇用形態多様性 |
| layer_b_keywords | 390 | TF-IDF抽出キーワード |
| layer_b_text_quality | 622 | 原稿品質グレード(A/B/C/D) |
| layer_b_cooccurrence | 554 | 条件フラグ共起(lift/phi) |
| layer_c_clusters | 469,027 | 全求人のk-means 40クラスタ割当 |
| layer_c_cluster_profiles | 40 | クラスタ統計プロファイル |
| layer_c_region_heatmap | 1,876 | 地域×クラスタ分布 |

### 4.3 V2分析テーブル（24テーブル）

共通設計: 全テーブルに `(prefecture, municipality, emp_group)` + 多くに `industry_raw`

#### Phase 1: 基本指標 (compute_v2_analysis.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_vacancy_rate | 34,299 | recruitment_reason_code=1(欠員補充)の比率 |
| v2_regional_resilience | 3,209 | Shannon H=-Σ(p_i×ln(p_i)), HHI=Σ(s_i²) |
| v2_transparency_score | 34,299 | 8任意開示項目(休日/賞与/従業員数/資本金/残業/女性比/パート比/設立年) |

#### Phase 1b: 給与分析 (compute_v2_salary.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_salary_structure | 23,499 | P10/P25/P50/P75/P90, 推定年収=月給×(12+賞与月) |
| v2_salary_competitiveness | 12,446 | (地域平均-全国平均)/全国平均×100 |
| v2_compensation_package | 12,446 | 給与45%+休日30%+賞与25%→S/A/B/C/Dランク |

#### Phase 2: テキスト分析 (compute_v2_text.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_text_quality | 21,490 | 文字数×ユニーク文字率×(1+数字率) |
| v2_keyword_profile | 128,940 | 6カテゴリ: 急募/未経験/待遇/WLB/成長/安定 |
| v2_text_temperature | 21,490 | (緊急密度-選択密度)‰, 高い=人手不足 |

**温度計ワード辞書**:
- 緊急: 急募, すぐ, 至急, 即日, 大量募集, 人手不足, 欠員
- 選択: 経験者優遇, 要経験, 有資格者, 経験N年, 選考あり

#### Phase 3: 市場構造 (compute_v2_market.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_employer_strategy | 469,027 | 給与percentile×福利amenity→4象限分類 |
| v2_employer_strategy_summary | 21,490 | 地域別4象限分布 |
| v2_monopsony_index | 21,490 | HHI, Gini, Top1/3/5シェア→分散/やや集中/高集中 |
| v2_spatial_mismatch | 3,721 | Haversine距離, 30km/60km圏内求人, 孤立度スコア |
| v2_cross_industry_competition | 2,192 | salary_band×education×emp_group→競合業種数 |

**4象限戦略**: プレミアム型(高給高福利) / 給与一本勝負型 / 福利厚生重視型 / コスト優先型
**Amenityスコア(0-100)**: 賞与(25) + 休日percentile(25) + 福利KW(25) + 低残業(25, NULL=0.5)

#### Phase 4: 外部データ (compute_v2_external.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_external_minimum_wage | 47 | 2024年都道府県別最低賃金(1,000～1,163円) |
| v2_wage_compliance | 2,174 | 時給求人のsalary_min<県最低賃金 の違反率 |
| v2_region_benchmark | 4,232 | 6軸平均: 活発度/給与競争力/定着度/多様性/透明性/温度 |

#### Phase 5: 予測 (compute_v2_prediction.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_fulfillment_score | 154,945 | LightGBM/LogReg 5-fold CV, A(<25)/B/C/D(≥75) |
| v2_fulfillment_summary | - | 地域集計版 |
| v2_mobility_estimate | 3,721 | 重力モデル: score=(salary×n)/distance^1.5 |
| v2_shadow_wage | 12,378 | P10/P25/P50/P75/P90, IQR |

#### Phase 2拡張 (compute_v2_phase2.py)

| テーブル | 行数 | アルゴリズム |
|---------|------|------------|
| v2_text_temperature | 21,490 | ‰単位版: (urgency-selectivity)/文字数×1000 |
| v2_cross_industry_competition | 2,192 | overlap_score=1/HHI |
| v2_anomaly_stats | 14,788 | 2σ閾値(mean±2×stddev)での異常値検出 |
| v2_cascade_summary | 19,239 | 都道府県→市区町村→産業ドリルダウン集計 |

---

## 5. Rustハンドラー・ルーティング

### ルート一覧

| パス | 説明 |
|------|------|
| /health | ヘルスチェック(JSON) |
| /login, /logout | 認証 |
| /tab/{overview,demographics,balance,workstyle,jobmap,analysis,competitive,diagnostic} | 8タブ |
| /api/analysis/subtab/{1-6} | 市場分析サブタブ |
| /api/diagnostic/evaluate | 市場診断評価 |
| /api/jobmap/* | 地図(markers, detail, stats, seekers等) |
| /api/competitive/* | 詳細検索(filter, report, analysis等) |
| /api/prefectures, /api/municipalities_cascade | フィルタカスケード |
| /api/industry_tree | 産業2階層ツリー |
| /api/geojson/{filename} | GeoJSON配信 |

### 市場分析 6サブタブ (analysis/)

| ID | サブタブ名 | 対応テーブル |
|----|----------|------------|
| 1 | 求人動向 | v2_vacancy_rate, v2_regional_resilience, v2_transparency_score |
| 2 | 給与分析 | v2_salary_structure, v2_salary_competitiveness, v2_compensation_package |
| 3 | テキスト分析 | v2_text_quality, v2_keyword_profile, v2_text_temperature |
| 4 | 市場構造 | v2_employer_strategy_summary, v2_monopsony_index, v2_spatial_mismatch, v2_cross_industry_competition, v2_cascade_summary |
| 5 | 異常値・外部 | v2_anomaly_stats, v2_external_minimum_wage, v2_wage_compliance, v2_region_benchmark |
| 6 | 予測・推定 | v2_fulfillment_summary, v2_mobility_estimate, v2_shadow_wage |

### 市場診断タブ (diagnostic.rs)

入力: 月給・年間休日・賞与・雇用形態 → 出力: 総合グレード(S/A/B/C/D) + 6軸レーダー + パーセンタイル + 改善提案

---

## 6. デプロイ設定

### Render

| 設定 | 値 |
|------|-----|
| サービス名 | hellowork-dashboard |
| ランタイム | Docker |
| リージョン | Oregon |
| プラン | Free |
| ポート | 9216 |
| AUTH_PASSWORD | Renderダッシュボード(sync:false) |
| GITHUB_TOKEN | Docker Build Argument(sync:false) |

### Dockerfile フロー

```
[ビルドステージ] rust:latest → cargo build --release
[ランタイム] debian:bookworm-slim
  → バイナリ + templates/ + static/ + data/geojson_gz/
  → download_db.sh: GitHub Release(db-v2.0)からhellowork.db.gzダウンロード
  → サイズ検証(≥10MB) + GITHUB_TOKEN認証 + レート制限フォールバック
```

---

## 7. 雇用形態セグメンテーション

全computeスクリプト共通:
```python
def emp_group(et):
    if et is None: return "その他"
    if "パート" in et: return "パート"
    if et == "正社員": return "正社員"
    return "その他"
```

---

## 8. 検証手順

### デプロイ後

1. `/health` → `{"status":"ok","db_connected":true}`
2. ログイン → 8タブ全表示
3. 市場分析 → 6サブタブ切替 → データ表示
4. フィルタ変更(東京都→千代田区) → データ更新
5. 市場診断 → 月給250000/休日120/賞与2.0 → グレード表示

### Python事前計算後

```python
import sqlite3
conn = sqlite3.connect('data/hellowork.db')
for t in ['v2_vacancy_rate','v2_salary_structure','v2_text_quality',
          'v2_employer_strategy_summary','v2_anomaly_stats','v2_fulfillment_summary']:
    c = conn.execute(f'SELECT COUNT(*) FROM {t}').fetchone()[0]
    print(f'{t}: {c}行')
```

---

## 9. 新規分析指標の追加ガイド

1. **Pythonスクリプト**: `scripts/compute_v2_NEW.py`
   - CREATE TABLE (prefecture, municipality, industry_raw, emp_group 必須)
   - 3レベル集計 + emp_groupセグメント + 最小サンプル数チェック

2. **Rust fetch**: `src/handlers/analysis/fetch.rs`
   - `query_3level()` or `table_exists()` + カスタムSQL

3. **Rust render**: `src/handlers/analysis/render.rs`
   - `render_subtab_N()` に追加、`escape_html()` 必須
   - ECharts: `<div class="echart" data-chart-config='JSON'>`

4. **デプロイ**: gzip → gh release upload → Render Manual Deploy
