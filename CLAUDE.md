# V2 ハローワークダッシュボード マスターリファレンス

**最終更新**: 2026-04-26
**リポジトリ**: `makimaki1006/HR_HR`
**デプロイリポ**: `hellowork-deploy/` (Render: `hellowork-dashboard`)
**本ドキュメントの位置付け**: 新規参入者・自分自身が最初に読むべき索引。各深掘り資料は §13 から辿る。

---

## 🔴 絶対ルール (事故由来)

| ルール | 違反時の事故 | feedback 参照 |
|--------|-------------|---------------|
| `git add -A` / `git add .` 禁止。ファイル名を必ず指定 | 2026-03-10 `data/geojson_gz/` 47 ファイル誤削除 | `feedback_git_safety` |
| コミット前に `git diff --cached --stat` で削除確認。バイナリ削除があれば即停止 | 同上 | `feedback_git_safety` |
| DB 書き込み (Turso INSERT/UPDATE/DELETE/CTAS) はユーザー実行のみ | 2026-01 $195 超過請求 | `feedback_turso_priority` |
| Turso アップロードは 1 回で完了。何度も DROP+CREATE しない | 2026-04-03 無料枠浪費 | `feedback_turso_upload_once` |
| 推測を事実として報告しない。「正常」「問題ない」「大丈夫」禁止。SQL 結果を必ず提示 | 2026-01-05 / 2026-03-17 虚偽報告 | `feedback_never_guess_data` |
| 「データ」と聞いたら「人口データか求人データか」を必ず確認 | 2026-03-17 数時間無駄 | `feedback_population_vs_posting` |
| HW 掲載求人のみが対象であり、全求人市場ではないことを UI/レポートに必ず明記 | UI 誤認リスク | `feedback_hw_data_scope` |
| 雇用形態を必ず dedup キーに含める (V2 では「正社員」、V1 では「正職員」と区別) | 2026-02-24 大量データ消失 | `feedback_dedup_rules` |
| 部分コミット時は依存チェーン (`include_str!`/`pub mod`/可視性) を確認。ローカル成功 ≠ 本番成功 | 2026-04-22 Render deploy 失敗 | `feedback_partial_commit_verify` |
| 並列 agent 間の契約 (JSON shape 等) は agent 個別テスト pass でも別途 cross-check | 2026-04-23 採用診断 8 panel 全滅 | `feedback_agent_contract_verification` |
| テストはデータ妥当性 (具体値) で検証する。「要素存在」だけでは不可 | 2026-03-22 / 2026-04-12 バグ見逃し | `feedback_test_data_validation` / `feedback_reverse_proof_tests` |
| E2E では canvas 存在ではなく ECharts 初期化完了を確認 | 2026-04-08 19/24 ブランク見逃し | `feedback_e2e_chart_verification` |
| 相関 ≠ 因果。`phrase_validator` で「確実に」「必ず」「100%」を機械的に排除 | UI 誇大表現リスク | `feedback_correlation_not_causation` |
| 仮説なきデータ投入は無意味。So What を先に設計 | 営業ツール化失敗 | `feedback_hypothesis_driven` |
| 一発で完了する実装手順 (DB操作前にスキーマ確認、コミット前に grep 監査) | 2026-04 修正の繰り返し | `feedback_implement_once` |

memory 参照は MEMORY.md (auto memory) ベースで管理。各ルール → 実コード対応は [`docs/memory_feedback_mapping.md`](docs/memory_feedback_mapping.md) を参照。

---

## 🔴 V1 / V2 分離

| | V1: ジョブメドレー (求職者) | V2: ハローワーク (求人) |
|---|---|---|
| リポ | `makimaki1006/rust-dashboard` | `makimaki1006/HR_HR` |
| デプロイリポ | `rust-dashboard-deploy/` | **`hellowork-deploy/` (本リポ)** |
| データソース | ジョブメドレースクレイピング | ハローワーク掲載求人 (469,027 件) |
| DB | 3 個 (postings + segment + geocoded) | **1 個 ローカル + 3 個 Turso** |
| 雇用形態 | **正職員** | **正社員** |
| フィルタ階層 | 職種 → 都道府県 | **都道府県 → 市区町村 → 産業 (2 階層ツリー)** |
| タブ数 | 9 | **9** |
| 対応 CLAUDE.md | (V1 リポ側) | **本ファイル** |

**禁止**: V2 コードを V1 リポにpush / V1 DB 構造を V2 適用 / 雇用形態用語の混同。

---

## 1. プロジェクト概要

| 項目 | 値 |
|------|-----|
| 技術スタック | Rust 1.75+ / Axum 0.8 / HTMX 2.0 / ECharts 5.5 / Leaflet 1.9 / Tailwind precompiled |
| ローカル DB | SQLite 1 個 (`data/hellowork.db`、~1.6GB 解凍後、postings 469,027 行) |
| 外部 DB | **Turso 3 系統** (country-statistics / salesnow / audit) |
| GeoJSON 静的 | `static/geojson/` 47 都道府県 + 市区町村 (起動時 gz 解凍) |
| 認証 | bcrypt (Cargo.toml `bcrypt = "0.16"`) / 平文 / 外部期限付きパスワード + ドメイン許可 + IP レート制限 |
| 雇用形態 | 正社員 / パート / その他 (3 値、survey は 4 値、jobmap は 4 値) |
| ポート | 9216 (デフォルト、`PORT` env で上書き) |
| デプロイ | Render Free / Docker / `hr-hw.onrender.com` |

---

## 2. アーキテクチャ概観

```
[Python ETL]                   [Rust Dashboard]
hellowork_etl.py               main.rs
   │ 418 列 CSV (CP932)            │ ① decompress_geojson_if_needed()
   ▼                               │ ② precompress_geojson()
hellowork_compute_layers.py    │ ③ decompress_db_if_needed()
   │ Layer A/B/C 9 テーブル        │ ④ LocalDb::new() + 19 INDEX
   ▼                               │ ⑤ TursoDb::new() × 3 (graceful)
scripts/compute_v2_*.py × 7    │ ⑥ AppCache (DashMap+TTL+max)
   │ 24 v2_* 分析テーブル          │ ⑦ build_app() → 9 タブ
   ▼                               ▼
hellowork.db (~1.6GB)          axum::serve (port 9216)
   │ gzip → ~297MB                 │
   ▼                               │ ┌─ /tab/* (HTML partial)
GitHub Release (db-v2.0)       │ ├─ /api/* (JSON)
   │ download_db.sh                │ ├─ /report/* (印刷 HTML)
   ▼                               │ └─ /api/v1/* (認証不要 MCP)
Docker build                   ▼
   │ + Render env vars         Browser (HTMX swap, ECharts auto-init)
   ▼
hr-hw.onrender.com
```

### 2.1 graceful degradation の原則

- **panic 0 件**: 全 Turso/SalesNow 接続は `Option<TursoDb>` で握る (`src/main.rs:82-207`)
- **未接続時**: `tracing::warn!` + 該当 API は空応答 / 該当 UI は注記表示
- **HW DB 未接続時**: 起動はするがダッシュボード上部に赤バナー表示 (`src/lib.rs:777-788`)

### 2.2 ディレクトリ構造

```
hellowork-deploy/
├── Cargo.toml / Dockerfile / render.yaml / .gitignore / README.md / CLAUDE.md
├── src/
│   ├── main.rs              # エントリ (起動シーケンス、INDEX 自動付与、Turso 接続)
│   ├── lib.rs               # build_app() ルーター定義 / AppState / CSRF / login / dashboard_page
│   ├── config.rs            # AppConfig (env var 15 個、§ 8.1 参照)
│   ├── audit/               # 監査 DB (アカウント、ログイン履歴、操作ログ、purge スケジューラ)
│   ├── auth/                # bcrypt + 外部期限付き + ドメイン許可 + RateLimiter
│   ├── db/
│   │   ├── local_sqlite.rs  # r2d2 max10 + WAL + mmap 256MB
│   │   ├── turso_http.rs    # libSQL HTTP client (timeout 30s)
│   │   └── cache.rs         # AppCache (DashMap + TTL + max_entries)
│   ├── geo/                 # city_code (master_city.csv、citycode 解決)
│   ├── models/              # job_seeker (PREFECTURE_ORDER 等)
│   └── handlers/            # 9 タブ + admin + my + api + api_v1 (`src/handlers/CLAUDE.md` 参照)
├── templates/
│   ├── dashboard_inline.html  # 9 タブ UI (現行)
│   ├── login_inline.html
│   ├── tabs/                  # competitive, jobmap, recruitment_diag, region_karte 等
│   └── dashboard.html         # ★ V1 遺物、未参照 (削除候補)
├── static/css/, static/js/    # ECharts/Leaflet/HTMX 連携 JS
├── data/
│   ├── hellowork.db           # 起動時に hellowork.db.gz から解凍 (git 非追跡)
│   └── geojson_gz/*.json.gz   # 起動時に static/geojson/ へ解凍
└── docs/                      # `docs/CLAUDE.md` 参照
```

---

## 3. ルーター総覧

`src/lib.rs:63-340` で定義。CSRF 保護付き、認証ミドルウェア配下。

### 3.1 公開 9 タブ (UI ナビから到達可能)

| ナビ表示 | URL | ハンドラ | 主用途 |
|----------|-----|---------|--------|
| 市場概況 | `/tab/market` | `handlers::market::tab_market` | 4 KPI + 比較バー + 産業別/職業/雇用形態/給与帯/求人理由 |
| 地図 | `/tab/jobmap` | `handlers::jobmap::tab_jobmap` | Leaflet + 6 コロプレス + Agoop ヒートマップ + SalesNow 企業マーカー + 半径検索 |
| 地域カルテ | `/tab/region_karte` | `handlers::region::tab_region_karte` | 1 市区町村の構造 + 人流 + 求人を 9 KPI + 7 セクション + 印刷可能 HTML |
| 詳細分析 | `/tab/analysis` | `handlers::analysis::tab_analysis` | 構造分析 / トレンド / 総合診断のグループ切替、サブタブ複層 (28 セクション) |
| **求人検索** | `/tab/competitive` | `handlers::competitive::tab_competitive` | 多次元フィルタ → 求人一覧 + 個別詳細 + 集計 (URL `/tab/competitive` は不変) |
| 条件診断 | `/tab/diagnostic` | `handlers::diagnostic::tab_diagnostic` | 月給/休日/賞与/雇用形態 → 6 軸レーダー + S/A/B/C/D グレード |
| 採用診断 | `/tab/recruitment_diag` | `handlers::recruitment_diag::tab_recruitment_diag` | 業種×エリア×雇用形態で 8 panel 並列ロード |
| 企業検索 | `/tab/company` | `handlers::company::tab_company` | SalesNow 198K 社 検索 → プロフィール × HW × 外部統計 |
| 媒体分析 | `/tab/survey` | `handlers::survey::tab_survey` | Indeed/求人ボックス CSV アップ → HW 統合 → 印刷 HTML |

⚠ **タブ呼称統一**: 「求人検索」を正式呼称とし、UI/H2/コメントで統一。詳細は [`docs/tab_naming_reference.md`](docs/tab_naming_reference.md) を参照。

### 3.2 dead route (UI 到達不可、ハンドラはコンパイル対象)

旧 `templates/dashboard.html` 用、現 UI には `hx-get` リンクなし:

| URL | ハンドラ | 状態 | 備考 |
|-----|---------|------|------|
| `/tab/overview` | `handlers::overview::tab_overview` | UI 非表示 | `templates/tabs/overview.html` は V1 求職者ダッシュボード遺物 (`{{AVG_AGE}}`, `{{MALE_COUNT}}` 誤用) |
| `/tab/balance` | `handlers::balance::tab_balance` | UI 非表示 | market タブ内で動的生成 |
| `/tab/workstyle` | `handlers::workstyle::tab_workstyle` | UI 非表示 | 同上 |
| `/tab/demographics` | `handlers::demographics::tab_demographics` | UI 非表示 | 同上 |
| `/tab/trend` | `handlers::trend::tab_trend` | UI 非表示 | analysis タブ内のサブグループから到達 |
| `/tab/insight` | `handlers::insight::tab_insight` | UI 非表示 | 同上。ただし `/api/insight/report*` `/report/insight` は外部出力経路として活きている可能性 |

**取扱方針**: 削除前に `/api/insight/report*` の外部利用 (Render ログ / nginx ログ) を確認すること。

### 3.3 主要 API ルート

| グループ | 主要パス | 主用途 |
|----------|---------|--------|
| ヘッダーフィルタ | `/api/prefectures`, `/api/municipalities_cascade`, `/api/industry_tree`, `/api/set_*` | セッション保存型カスケード |
| 市場概況 | `/api/market/{population,workstyle,balance,demographics}` | KPI 別の partial swap |
| 詳細分析 | `/api/analysis/subtab/{1-7}` | サブタブ HTML partial |
| 採用診断 | `/api/recruitment_diag/{difficulty,talent_pool,inflow,competitors,condition_gap,market_trend,opportunity_map,insights}` | 8 panel 並列ロード |
| 地図 | `/api/jobmap/{markers,detail/{id},detail-json/{id},stats,municipalities,seekers,seeker-detail,choropleth,heatmap,inflow,company-markers,labor-flow,industry-companies,correlation,region/*}` | 14+ JSON / HTML 端点 |
| Agoop 人流 | `/api/flow/karte/{profile,monthly,daynight_ratio,inflow_breakdown}`, `/api/flow/city_agg` | Round 2 人流 API |
| 地域カルテ | `/api/region/karte/{citycode}` | 1 市区町村統合 JSON |
| 媒体分析 | `/api/survey/{upload(POST),analyze,integrate,report}`, `/report/survey`, `/report/survey/download` | CSV アップ → HW 統合 |
| insight | `/api/insight/{subtab/{id},widget/{tab},report,report/xlsx}`, `/report/insight` | 38 patterns / レポート出力 |
| trend | `/api/trend/subtab/{id}` | トレンドサブタブ |
| 企業検索 | `/api/company/{search,profile/{cn},bulk-csv}`, `/report/company/{cn}` | SalesNow + HW 結合 |
| 求人検索 | `/api/competitive/{filter,municipalities,facility_types,service_types,analysis,analysis/filter}`, `/api/report` | フィルタ + 集計レポート |
| 条件診断 | `/api/diagnostic/{evaluate,reset}` | 6 軸診断 |
| 認証なし | `/api/v1/companies(/...)` | MCP/AI 連携用 (api_v1.rs) |
| 自己サービス | `/my/profile`, `/my/activity` | プロフィール / 活動履歴 |
| 管理者 | `/admin/{users,users/{aid},login-failures}` | role=admin のみ |
| 静的 | `/static/*`, `/api/geojson/{filename}` | precompressed_gzip 配信 |
| ヘルス | `/health`, `/api/status`, `/login`, `/logout` | 認証不要 |

---

## 4. データソース 4 系統

詳細は [`docs/data_sources.md`](docs/data_sources.md) を参照。要点のみ:

| 系統 | 型 | env var | 接続失敗時 |
|------|---|---------|----------|
| ローカル `hellowork.db` | `Option<LocalDb>` (r2d2) | `HELLOWORK_DB_PATH` | UI 上部に赤バナー (lib.rs:777) |
| Turso `country-statistics` | `Option<TursoDb>` | `TURSO_EXTERNAL_URL` / `_TOKEN` | 該当 API 空応答 |
| Turso `salesnow` (198K 社) | `Option<TursoDb>` | `SALESNOW_TURSO_URL` / `_TOKEN` | 企業検索/採用診断/labor_flow 空応答 |
| Turso `audit` | `Option<AuditDb>` | `AUDIT_TURSO_URL` / `_TOKEN` / `AUDIT_IP_SALT` | `/admin/*` が 403、ログイン履歴記録 OFF |
| GeoJSON 静的 | `static/geojson/*.json(.gz)` | (なし) | 地図描画失敗 (warn ログ) |
| CSV upload (媒体分析) | `tower-sessions` メモリ | (なし、`UPLOAD_BODY_LIMIT_BYTES=20MB`) | 20MB 超は 413 即拒否 |

---

## 5. DB スキーマ サマリ

### 5.1 ローカル `hellowork.db` (1 個、~1.6GB、起動時 gzip 解凍)

- **postings** (469,027 行 × 121 列): 求人票全データ。識別 / 地域 / 施設 / 給与 / 雇用 / 労働条件 / 福利厚生フラグ 17 個 / テキスト分析 / セグメント / 募集理由 等。起動時に 19 個の INDEX 自動付与 (`main.rs:42-67`) + ANALYZE。
- **municipality_geocode** (2,626 行): 47 都道府県 × 562 市区町村の緯度経度
- **Layer A-C** (9 テーブル): 給与統計 / 施設集中 / 雇用多様性 / TF-IDF キーワード / テキスト品質 / 共起 / k-means クラスタ
- **v2_** 分析テーブル (24 個): Phase 1 〜 Phase 5 + Phase 2 拡張、§5.5 参照
- **survey_records / survey_sessions / ts_agg_***: 媒体分析セッション保存 + 集計キャッシュ

### 5.2 Turso `country-statistics` (Round 1-3 の主データ)

| 系統 | テーブル | 用途 |
|------|---------|------|
| 外部統計 (e-Stat / SSDSE) | `v2_external_population`, `v2_external_population_pyramid`, `v2_external_migration`, `v2_external_daytime_population`, `v2_external_minimum_wage`, `v2_external_minimum_wage_history`, `v2_external_prefecture_stats`, `v2_external_job_openings_ratio`, `v2_external_labor_stats`, `v2_external_labor_force`, `v2_external_establishments`, `v2_external_turnover`, `v2_external_household_spending`, `v2_external_business_dynamics`, `v2_external_climate`, `v2_external_care_demand`, `v2_external_foreign_residents`, `v2_external_education`, `v2_external_education_facilities`, `v2_external_household`, `v2_external_households`, `v2_external_industry_structure`, `v2_external_internet_usage`, `v2_external_boj_tankan`, `v2_external_social_life`, `v2_external_land_price`, `v2_external_car_ownership`, `v2_external_medical_welfare`, `v2_external_geography`, `v2_external_vital_statistics`, `v2_external_commute_od` | SSDSE-A + e-Stat API 由来。30+ テーブル、~40,944 行 |
| Agoop 人流 | `v2_flow_mesh1km_2019` / `_2020` / `_2021` (合計 38M 行)、`v2_flow_master_prefcity`, `v2_flow_fromto_city`, `v2_flow_attribute_mesh1km`, `v2_posting_mesh1km` | Round 2 人流分析の元データ |
| HW 時系列 | `ts_turso_counts`, `ts_turso_salary`, `ts_turso_vacancy`, `ts_turso_fulfillment` | 月次推移 (~16 万行) |
| ⚠ 未投入 (5/1 期日) | `v2_flow_city_agg`, `v2_flow_mesh3km_agg` | 現在 14 箇所 GROUP BY 動的集計でフォールバック中 (`docs/flow_ctas_restore.md`) |

### 5.3 Turso `salesnow` (3 テーブル、198K 社)

| テーブル | 行数 | 用途 |
|---------|------|------|
| `v2_salesnow_companies` | 198K 社 × 44 フィールド | 信用スコア / 上場 / 事業 / 採用情報 |
| `v2_industry_mapping` | (中) | HW `industry_raw` ↔ SalesNow industry の対応 |
| `v2_company_geocode` | (中) | 企業所在地ジオコード (起動時キャッシュは Render 512MB OOM 対策で無効化、main.rs:155-158) |

### 5.4 Turso `audit`

- `accounts` / `login_sessions` / `activity` / `login_failures`
- 24h ごとに `purge_old_activity` (1 年経過分削除、`main.rs:222-242`)

### 5.5 24 個の v2_* 分析テーブル サマリ

| Phase | テーブル数 | 主アルゴリズム |
|-------|----------|--------------|
| 1 (基本指標) | `v2_vacancy_rate`, `v2_regional_resilience`, `v2_transparency_score` | recruitment_reason 比率 / Shannon H / HHI / 8 任意開示項目 |
| 1b (給与) | `v2_salary_structure`, `v2_salary_competitiveness`, `v2_compensation_package` | P10/P25/P50/P75/P90, 推定年収, S/A/B/C/D ランク |
| 2 (テキスト) | `v2_text_quality`, `v2_keyword_profile`, `v2_text_temperature` | 文字数 × ユニーク率, 6 カテゴリ KW, (緊急-選択)/‰ |
| 3 (市場構造) | `v2_employer_strategy(_summary)`, `v2_monopsony_index`, `v2_spatial_mismatch`, `v2_cross_industry_competition` | 給与 × 福利 4 象限, HHI/Gini, Haversine 30/60km, 業種重複 |
| 4 (外部) | `v2_external_minimum_wage`, `v2_wage_compliance`, `v2_region_benchmark` | 最低賃金 違反率, 6 軸ベンチマーク |
| 5 (予測) | `v2_fulfillment_summary`, `v2_mobility_estimate`, `v2_shadow_wage` | LightGBM 5-fold CV, 重力モデル, P10〜P90 |
| 2 拡張 | `v2_anomaly_stats`, `v2_cascade_summary` | 2σ 異常値, 都道府県→市区町村→産業 |

⚠ **vacancy_rate の意味**: 「recruitment_reason_code=1 (欠員補充) を理由とする求人の割合」であり、労働経済学の欠員率 (未充足求人/常用労働者数) **ではない**。UI 表記統一が課題 (P0、`docs/audit_2026_04_24/team_gamma_domain.md` M-1)。

---

## 6. 原本データ投入パイプライン

### 6.1 全体フロー

```
[原本受領]
   ↓
hellowork_etl.py: HW CSV (CP932, 418列) → postings テーブル
   ↓
hellowork_compute_layers.py: Layer A/B/C 9 テーブル
   ↓
scripts/compute_v2_*.py × 7: v2_* 24 テーブル (実行順序重要、§6.2)
   ↓
gzip -c data/hellowork.db > data/hellowork.db.gz (1.6GB → ~297MB)
   ↓
gh release upload db-v2.0 data/hellowork.db.gz --clobber --repo makimaki1006/HR_HR
   ↓
Render Manual Deploy (Dockerfile 内 download_db.sh が GitHub Release から取得)
```

### 6.2 Python 事前計算 実行順序

```bash
cd C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy

# Phase 1 (依存なし)
python scripts/compute_v2_analysis.py
python scripts/compute_v2_salary.py
python scripts/compute_v2_text.py
# Phase 3 (postings の lat/lng 必須)
python scripts/compute_v2_market.py
# Phase 4 (★ Phase 1-2 結果テーブルに依存)
python scripts/compute_v2_external.py
# Phase 5 (scikit-learn or lightgbm)
python scripts/compute_v2_prediction.py
# Phase 2 拡張
python scripts/compute_v2_phase2.py
```

### 6.3 Turso 側 ETL

外部統計 / Agoop 人流 / SalesNow / HW 時系列の Turso 投入手順は別ドキュメント:

- [`docs/turso_import_ssdse_phase_a.md`](docs/turso_import_ssdse_phase_a.md): SSDSE-A
- [`docs/turso_import_agoop.md`](docs/turso_import_agoop.md): Agoop 人流
- [`docs/maintenance_posting_mesh1km.md`](docs/maintenance_posting_mesh1km.md): posting_mesh1km

⚠ **Turso 書き込みはユーザー実行のみ** (`feedback_turso_priority`)。

---

## 7. 9 タブ機能サマリ

| # | タブ (UI 表示) | URL | ハンドラ | 主データ | 主出力 |
|---|--------------|-----|---------|----------|--------|
| 1 | 市場概況 | `/tab/market` | `market.rs` | postings + v2_external_* | KPI 4 + 比較バー 3 + チャート 5 |
| 2 | 地図 | `/tab/jobmap` | `jobmap/` (15 ファイル) | postings + v2_flow_* + v2_salesnow_* + v2_company_geocode | Leaflet 地図 + 6 レイヤー切替 + 半径検索 + 相関散布図 |
| 3 | 地域カルテ | `/tab/region_karte` | `region/karte.rs` | postings + v2_external_* + v2_flow_* | 9 KPI + 7 セクション + 印刷 HTML |
| 4 | 詳細分析 | `/tab/analysis` | `analysis/` (4 ファイル、render.rs 4,594 行) | 全 v2_* + ts_turso_* | 構造分析 / トレンド / 総合診断 グループ × サブタブ × 28 セクション |
| 5 | **求人検索** | `/tab/competitive` | `competitive/` | postings | 多次元フィルタ + 一覧 + 個別 + 集計レポート |
| 6 | 条件診断 | `/tab/diagnostic` | `diagnostic.rs` | postings + v2_vacancy_rate 等 | 月給/休日/賞与 → 6 軸レーダー + S/A/B/C/D |
| 7 | 採用診断 | `/tab/recruitment_diag` | `recruitment_diag/` (10 ファイル) | postings + v2_external_* + v2_flow_* + v2_salesnow_* | 8 panel 並列 (難度/プール/流入/競合/条件/動向/穴場/AI) |
| 8 | 企業検索 | `/tab/company` | `company/` | v2_salesnow_* + v2_external_prefecture_stats + postings | 検索 → プロフィール × HW × 外部統計 |
| 9 | 媒体分析 | `/tab/survey` | `survey/` (10+ ファイル、report_html.rs 3,912 行) | CSV upload + postings + ts_turso_counts | アップ → HW 統合 → 印刷 HTML / ダウンロード HTML |

詳細は [`src/handlers/CLAUDE.md`](src/handlers/CLAUDE.md) を参照。

### 7.1 詳細分析 (`/tab/analysis`) サブタブ構成

`src/handlers/analysis/handlers.rs` のグループ切替経由。`ANALYSIS_SUBTABS` 定数で定義 (`analysis/helpers.rs`):

| グループ | サブタブ ID | 名称 | 主テーブル |
|---------|-------------|------|----------|
| 構造分析 | 1 | 求人動向 | v2_vacancy_rate, v2_regional_resilience, v2_transparency_score |
| 構造分析 | 2 | 給与分析 | v2_salary_structure, v2_salary_competitiveness, v2_compensation_package |
| 構造分析 | 3 | テキスト分析 | v2_text_quality, v2_keyword_profile, v2_text_temperature |
| 構造分析 | 4 | 市場構造 | v2_employer_strategy_summary, v2_monopsony_index, v2_spatial_mismatch, v2_cross_industry_competition, v2_cascade_summary |
| 構造分析 | 5 | 異常値・外部 | v2_anomaly_stats, v2_external_minimum_wage, v2_wage_compliance, v2_region_benchmark |
| 構造分析 | 6 | 予測・推定 | v2_fulfillment_summary, v2_mobility_estimate, v2_shadow_wage |
| 構造分析 | 7 | (BoJ Tankan / 外部統計拡張) | v2_external_boj_tankan 等 |
| トレンド | (trend.rs) | トレンドサブタブ複数 | ts_turso_*, v2_external_* |
| 総合診断 | (insight.rs) | 採用構造 / 将来予測 / 地域比較 / 構造分析 (38 patterns) | InsightContext (postings + v2_* + flow) |

### 7.2 採用診断 8 panel

`templates/tabs/recruitment_diag.html:13-228` で並列ロード:

| Panel | API | 内容 | 主データ |
|-------|-----|------|---------|
| 1 | `/api/recruitment_diag/difficulty` | 採用難度スコア (HW 求人数 / Agoop 平日昼滞在人口 × 10000) | postings + v2_flow_* |
| 2 | `/api/recruitment_diag/talent_pool` | 人材プール (day_pop - night_pop = 流入者) | v2_flow_* + v2_external_population |
| 3 | `/api/recruitment_diag/inflow` | 流入元分析 (注: v2_flow_fromto_city は 83% のみ投入済) | v2_flow_fromto_city |
| 4 | `/api/recruitment_diag/competitors` | 競合企業ランキング | v2_salesnow_companies + postings |
| 5 | `/api/recruitment_diag/condition_gap` | 条件ギャップ (自社入力 vs 中央値) | postings (median by ORDER BY LIMIT OFFSET) |
| 6 | `/api/recruitment_diag/market_trend` | 市場動向 (job_type 指定時は ts_turso_salary 由来サンプル件数) | ts_turso_* |
| 7 | `/api/recruitment_diag/opportunity_map` | 穴場マップ (Panel 1 の市区町村展開) | postings + v2_flow_* |
| 8 | `/api/recruitment_diag/insights` | AI 示唆統合 (38 patterns 配信) | InsightContext |

⚠ **Panel 5 emp_type フィルタは `expand_employment_type` 未経由**。UI 値そのままで postings.employment_type を検索するため、ヒット 0 で「データ不足」誤表示の可能性。

---

## 8. 環境変数 19 個

完全リファレンスは [`docs/env_variables_reference.md`](docs/env_variables_reference.md) を参照。要点:

### 8.1 config.rs 管理 (15 個)

| 変数 | デフォルト | 用途 | 未設定時影響 |
|------|----------|------|-------------|
| `PORT` | `9216` | HTTP リッスンポート | デフォルト使用 |
| `AUTH_PASSWORD` | "" | 平文パスワード (社内・無期限) | 認証 OFF |
| `AUTH_PASSWORD_HASH` | "" | bcrypt ハッシュ (社内・無期限) | 同上 |
| `AUTH_PASSWORDS_EXTRA` | "" | 外部期限付きパスワード `pass1:2026-06-30,...` | 外部認証なし |
| `ALLOWED_DOMAINS` | `f-a-c.co.jp,cyxen.co.jp` | 社内ドメイン | デフォルト 2 ドメイン |
| `ALLOWED_DOMAINS_EXTRA` | "" | 外部追加ドメイン | 追加なし |
| `HELLOWORK_DB_PATH` | `data/hellowork.db` | SQLite ファイルパス | デフォルト |
| `CACHE_TTL_SECS` | `1800` (30 分) | DashMap TTL | デフォルト |
| `CACHE_MAX_ENTRIES` | `3000` | DashMap 最大エントリ | デフォルト |
| `RATE_LIMIT_MAX_ATTEMPTS` | `5` | ログイン失敗上限 | デフォルト |
| `RATE_LIMIT_LOCKOUT_SECONDS` | `300` (5 分) | ロックアウト秒数 | デフォルト |
| `AUDIT_TURSO_URL` | "" | 監査 DB URL | 監査機能 OFF (`/admin/*` 403) |
| `AUDIT_TURSO_TOKEN` | "" | 監査 DB トークン | 同上 |
| `AUDIT_IP_SALT` | `hellowork-default-salt` | IP ハッシュ用 salt | ⚠ デフォルトのままだとレインボーテーブル攻撃容易、本番では必須変更 |
| `ADMIN_EMAILS` | "" | 管理者メール | role=admin 付与なし |

### 8.2 main.rs 直接読出 (4 個、🔴 config.rs 統合違反、P0)

| 変数 | 用途 | 未設定時影響 |
|------|------|-------------|
| `TURSO_EXTERNAL_URL` | country-statistics URL | 外部統計タブ全機能 OFF |
| `TURSO_EXTERNAL_TOKEN` | country-statistics トークン | 同上 |
| `SALESNOW_TURSO_URL` | SalesNow URL | 企業検索 / 採用診断 Panel 4 / 地図 labor-flow / company-markers 空応答 |
| `SALESNOW_TURSO_TOKEN` | SalesNow トークン | 同上 |

### 8.3 アップロード上限

`UPLOAD_BODY_LIMIT_BYTES = 20 * 1024 * 1024` (20MB、`src/lib.rs:39`)。`/api/survey/upload` のみ適用。20MB 超は 413 即拒否。

---

## 9. 検証チェックリスト

### 9.1 デプロイ後

```
1. /health → {"status":"healthy","db_connected":true,"db_rows":469027}
2. /login → ログイン成功 → /
3. ナビバーに 9 タブ全表示
4. 市場概況 → 4 KPI + 比較バー + チャート 5 表示
5. 地図 → Leaflet 描画 + 6 レイヤー切替動作 + 求人ピン表示
6. 地域カルテ → 市区町村未選択時は誘導文、citycode 指定で 9 KPI
7. 詳細分析 → 構造分析グループ → サブタブ 1-7 切替
8. 求人検索 → フィルタ + 検索結果テーブル
9. 条件診断 → 月給/休日/賞与 → S/A/B/C/D グレード
10. 採用診断 → 業種×エリア → 8 panel 並列ロード (各 200 OK)
11. 企業検索 → 法人検索 → プロフィール
12. 媒体分析 → CSV アップ → 統合分析 → 印刷 HTML
13. /api/v1/companies?q=○○ → JSON
14. /admin/users → role=admin で 200、それ以外 403
```

### 9.2 Python 事前計算後

```python
import sqlite3
conn = sqlite3.connect('data/hellowork.db')
for t in ['v2_vacancy_rate','v2_salary_structure','v2_text_quality',
          'v2_employer_strategy_summary','v2_anomaly_stats','v2_fulfillment_summary',
          'v2_compensation_package', 'v2_monopsony_index', 'v2_spatial_mismatch']:
    c = conn.execute(f'SELECT COUNT(*) FROM {t}').fetchone()[0]
    print(f'{t}: {c}行')
```

### 9.3 Turso 側

```bash
# country-statistics
turso db shell country-statistics "SELECT COUNT(*) FROM v2_external_population"
turso db shell country-statistics "SELECT COUNT(*) FROM v2_flow_mesh1km_2021"

# salesnow
turso db shell salesnow "SELECT COUNT(*) FROM v2_salesnow_companies"
```

---

## 10. 既知の重大課題 (P0 / P1)

詳細は [`docs/audit_2026_04_24/00_overall_assessment.md`](docs/audit_2026_04_24/00_overall_assessment.md) 参照。

| ID | 内容 | ファイル:行 | 期日 |
|----|------|-----------|------|
| P0 #1 | jobmap Mismatch #1 (`name` キー欠落) | `jobmap/handlers.rs:399` | 即修正 (5 分) |
| P0 #2 | jobmap Mismatch #4 (`municipality` キー欠落) | `jobmap/company_markers.rs:128` | 即修正 (5 分) |
| P0 #3 | MF-1 医師密度 単位 10× ズレ疑 | `insight/engine.rs:1565` | 検証 30 分 |
| P0 #4 | vacancy_rate の概念混乱 (HS/FC/RC/IN/balance に波及) | `insight/engine.rs:127` 等 | UI ラベル統一 半日 |
| P0 #5 | posting_change_3m/1y muni 粒度詐称 | `survey/hw_enrichment.rs:108-128` | UI 注記 半日 |
| P0 #6 | CTAS fallback 14 箇所の戻し | `flow.rs` / `flow_context.rs` | 5/1 期日 (`docs/flow_ctas_restore.md`) |
| P1 #7 | insight / trend がナビ非表示 | `dashboard_inline.html:70-89` | UI 修正 1h |
| P1 #8 | タブ呼称 4 重ブレ | competitive 関連 5 箇所 | 「求人検索」に統一 |
| P1 #9 | 雇用形態分類二重定義 (survey vs recruitment_diag) | `survey/aggregator.rs:678-682` / `recruitment_diag/mod.rs:74-81` | `emp_classifier.rs` 単一モジュール化 |
| P1 #10 | 統合 PDF レポート不在 | `/report/integrated` 新規 | コンサルA決定打 |

---

## 11. 新規分析指標の追加ガイド

1. **Python ETL** `scripts/compute_v2_NEW.py`:
   - `CREATE TABLE` に `prefecture, municipality, industry_raw, emp_group` を必ず含める
   - 雇用形態を必ず dedup キーに含める (`feedback_dedup_rules`)
   - 最小サンプル数チェック (n≥30 が標準)
2. **Rust fetch** `src/handlers/analysis/fetch.rs`:
   - `query_3level()` で 市区町村→都道府県→全国 の自動フォールバック
   - または `table_exists(db, "v2_NEW")` + `query_turso_or_local()`
3. **Rust render** `src/handlers/analysis/render.rs`:
   - `escape_html()` 必須 (XSS 防止)
   - ECharts: `<div class="echart" data-chart-config='JSON'>` (htmx:afterSettle で自動 setOption)
   - サンプル件数表示 (n=...) で誠実性担保
4. **テスト** (`feedback_test_data_validation` / `feedback_reverse_proof_tests`):
   - 「要素存在」ではなく「具体値」で逆証明
   - `phrase_validator::assert_valid_phrase()` を必ず通す (相関 ≠ 因果)
5. **Turso 投入** (ユーザー手動実行のみ):
   - `turso_sync.py` 等の冪等インポート
   - 1 回で完了させる (`feedback_turso_upload_once`)
6. **デプロイ**: gzip → gh release upload → Render Manual Deploy

---

## 12. memory feedback ルール → 実コード対応

詳細は [`docs/memory_feedback_mapping.md`](docs/memory_feedback_mapping.md) を参照 (14 ルール × ファイル位置)。

主要ポイント:
- `phrase_validator.rs` (insight) で「確実に」「必ず」「100%」を機械的禁止 (`feedback_correlation_not_causation`)
- `global_contract_audit_test.rs` で複数タブ JSON shape を tempfile DB で逆証明 (`feedback_agent_contract_verification`)
- `pattern_audit_test.rs` (1,767 行) で 22 patterns 各 body を具体値検証 (`feedback_reverse_proof_tests`)
- `query_turso_or_local()` で Turso 優先、ローカル fallback (`feedback_turso_priority`)
- 11+ 箇所の HW 限定スコープ明示 (`feedback_hw_data_scope`)

---

## 13. ドキュメント索引

### マスター/index
- [`CLAUDE.md`](CLAUDE.md) — ★ 本ファイル (マスターリファレンス)
- [`docs/CLAUDE.md`](docs/CLAUDE.md) — docs/ 索引
- [`src/handlers/CLAUDE.md`](src/handlers/CLAUDE.md) — ハンドラ別責務一覧

### 横断リファレンス (本監査で新設)
- [`docs/insight_patterns.md`](docs/insight_patterns.md) — insight 38 patterns カタログ
- [`docs/tab_naming_reference.md`](docs/tab_naming_reference.md) — タブ呼称統一意思決定 + リファレンステーブル
- [`docs/env_variables_reference.md`](docs/env_variables_reference.md) — 19 環境変数完全表
- [`docs/data_sources.md`](docs/data_sources.md) — データソースマップ + 依存マトリクス
- [`docs/memory_feedback_mapping.md`](docs/memory_feedback_mapping.md) — feedback 14 ルール → 実コード対応

### 設計仕様
| ファイル | 内容 |
|---------|------|
| [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) | エンドユーザー向け使い方 (タブ別) |
| [`docs/USER_MANUAL.md`](docs/USER_MANUAL.md) | 詳細マニュアル |
| [`docs/openapi.yaml`](docs/openapi.yaml) | `/api/v1/*` (MCP/AI 連携) の OpenAPI |
| [`docs/pdf_design_spec_2026_04_24.md`](docs/pdf_design_spec_2026_04_24.md) | PDF レポート設計 |
| `docs/design_ssdse_a_*.md` | SSDSE-A 統計テーブル設計 (backend/frontend/expansion) |
| `docs/design_agoop_*.md` | Agoop 人流データ設計 (backend/frontend/jinryu) |
| `docs/requirements_*.md` | 要件定義 |

### 運用・移行
| ファイル | 内容 |
|---------|------|
| [`docs/turso_import_ssdse_phase_a.md`](docs/turso_import_ssdse_phase_a.md) | SSDSE-A 投入手順 |
| [`docs/turso_import_agoop.md`](docs/turso_import_agoop.md) | Agoop 人流投入手順 |
| [`docs/maintenance_posting_mesh1km.md`](docs/maintenance_posting_mesh1km.md) | posting_mesh1km メンテ |
| [`docs/flow_ctas_restore.md`](docs/flow_ctas_restore.md) | ★ 5/1 後の CTAS 戻し手順 |
| [`docs/contract_audit_2026_04_23.md`](docs/contract_audit_2026_04_23.md) | 全タブ契約監査 (Mismatch #1-#5) |
| [`docs/qa_integration_round_1_3.md`](docs/qa_integration_round_1_3.md) | Round 1-3 QA 統合 |

### 監査
| ファイル | 内容 |
|---------|------|
| [`docs/audit_2026_04_24/00_overall_assessment.md`](docs/audit_2026_04_24/00_overall_assessment.md) | 5 チーム統合監査 (本リファレンスの根拠) |
| [`docs/audit_2026_04_24/team_alpha_userfacing.md`](docs/audit_2026_04_24/team_alpha_userfacing.md) | User-facing |
| [`docs/audit_2026_04_24/team_beta_system.md`](docs/audit_2026_04_24/team_beta_system.md) | System Integrity |
| [`docs/audit_2026_04_24/team_gamma_domain.md`](docs/audit_2026_04_24/team_gamma_domain.md) | Domain Logic 38 patterns |
| [`docs/audit_2026_04_24/team_delta_codehealth.md`](docs/audit_2026_04_24/team_delta_codehealth.md) | Code Health |
| [`docs/audit_2026_04_24/team_epsilon_walkthrough.md`](docs/audit_2026_04_24/team_epsilon_walkthrough.md) | Persona Walkthrough |
| [`docs/audit_2026_04_24/plan_p4_documentation.md`](docs/audit_2026_04_24/plan_p4_documentation.md) | ★ 本リファレンス再構成プラン |

### テスト
| ファイル | 内容 |
|---------|------|
| [`docs/E2E_TEST_PLAN.md`](docs/E2E_TEST_PLAN.md) / [`docs/E2E_TEST_PLAN_V2.md`](docs/E2E_TEST_PLAN_V2.md) | E2E 計画 |
| [`docs/E2E_COVERAGE_MATRIX.md`](docs/E2E_COVERAGE_MATRIX.md) | カバレッジ |
| [`docs/E2E_REGRESSION_GUIDE.md`](docs/E2E_REGRESSION_GUIDE.md) | リグレッション運用 |
| [`docs/E2E_RESULTS_LATEST.md`](docs/E2E_RESULTS_LATEST.md) | 最新結果 |
| [`docs/SESSION_SUMMARY_2026-04-12.md`](docs/SESSION_SUMMARY_2026-04-12.md) | セッションサマリ |

---

**改訂履歴**:
- 2026-04-26: 全面再構成 (P4 / audit_2026_04_24 #10 対応)。9 タブ・Round 1-3・SalesNow・envvar 19 個・dead route 6 件・memory feedback 14 ルール 反映
- 2026-03-14: 旧版 (8 タブ + サブタブ)
