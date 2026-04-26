# Plan P4: ドキュメント再構成プラン

**作成日**: 2026-04-26
**作成者**: P4 (Documentation Re-architect)
**対象**: V2 HW Dashboard (`C:\Users\fuji1\AppData\Local\Temp\hellowork-deploy\`)
**親監査**: `docs/audit_2026_04_24/00_overall_assessment.md` 課題 #10
**スコープ**: ルート CLAUDE.md / docs/CLAUDE.md / src/handlers/CLAUDE.md / 用語集 / リファレンステーブル / README

---

## 0. エグゼクティブサマリ

| 課題 | 現状 | 提案 |
|------|------|------|
| #10 ルート CLAUDE.md 再構成 | 2026-03-14 (40+ 日未更新)、9 タブ中 4 タブ未記載、Round 1-3 数値・SalesNow・envvar 全欠落 | §3 完全書き換えドラフト (約 700 行、コピペ可) |
| docs/CLAUDE.md 空テンプレ | `*No recent activity*` のみ | §4 マスター index ドラフト |
| src/handlers/CLAUDE.md 空テンプレ | 同上 | §5 ハンドラ別責務一覧ドラフト |
| 用語ブレ 4 重 | 求人検索 / 競合調査 / 企業調査 / 企業分析 が交錯 | §6 「求人検索」に統一 (根拠 + 影響箇所 5 件) |
| タブ呼称リファレンス | 散在 | §7 9 タブ呼称対応表 |
| 環境変数 19 個 | 15 個は config.rs、4 個は main.rs 直読 | §8 完全表 + 未設定時影響 |
| データソースマップ | 各ドキュメントに分散 | §9 4 系統 + 静的 GeoJSON 表 |
| タブ × データソース 依存マトリクス | 不在 | §10 9 タブ × 4 系統表 |
| insight 38 patterns カタログ | engine.rs / engine_flow.rs にコメント分散 | §11 38 行表 (id / カテゴリ / 閾値 / data source / phrase_validator 適用状況) |
| feedback ルール → 実コード対応 | 不明 | §12 14 ルール × ファイル位置表 |
| README.md | 9 タブ中 7 タブ・SalesNow・採用診断未記載、TURSO_URL 等の env 名が誤り | §13 修正提案 |

**最終提案**:
- 用語統一: **「求人検索」(`/tab/competitive`)** に統一 (根拠 §6.2)
- ルート CLAUDE.md は §3 ドラフトで全面置換
- README.md は env 名誤り (`TURSO_URL`) を修正、採用診断/SalesNow/9 タブを追記

---

## 1. 各課題の現状診断

### 1.1 ルート CLAUDE.md (#10) の乖離点

| 項目 | 現状 (2026-03-14版) | 現実装 |
|------|--------------------|--------|
| タブ数 | "8タブ + 市場分析6サブタブ + 市場診断" | **9 タブ** (templates/dashboard_inline.html:71-87) |
| ハンドラ列挙 | overview/demographics/balance/workstyle/diagnostic/api/jobmap/competitive/analysis | **insight, survey, recruitment_diag, region, trend, company, admin, my, api_v1 を欠落** |
| データソース | "SQLite 1個" | hellowork.db + Turso country-statistics + Turso salesnow + Turso audit (4 系統、main.rs:82-207) |
| Round 1-3 | 言及なし | Agoop 人流 (38M 行 mesh1km × 3年)、地域カルテ、SalesNow 統合、insight 38 patterns 完了 |
| 環境変数 | 列挙なし | 19 個 (config.rs 15 + main.rs 4) |
| memory feedback | 言及なし | 14 ルールが MEMORY.md に存在 |
| 採用診断 (4-23 事故対応) | 記載なし | recruitment_diag/ 8 panel 中核機能 |
| dead route | 言及なし | 6 件 (`/tab/{overview,balance,workstyle,demographics,trend,insight}` UI から到達不可) |
| CTAS fallback (5/1 期日) | 言及なし | 14 箇所、`docs/flow_ctas_restore.md` に手順 |

### 1.2 docs/CLAUDE.md / src/handlers/CLAUDE.md

両方とも `<claude-mem-context>*No recent activity*</claude-mem-context>` のみ (各 7 行)。実質ドキュメントなし。

### 1.3 用語 4 重ブレ (#7)

| 出現箇所 | 表記 |
|----------|------|
| `templates/dashboard_inline.html:79` (UI ボタン) | **求人検索** |
| URL / 関数名 (`src/lib.rs:232`, `src/handlers/competitive/render.rs:30`) | **competitive** |
| `templates/tabs/competitive.html:1` (HTMLコメント) | **タブ8: 競合調査** |
| `templates/tabs/competitive.html:3` (H2 表示) | **🔍 企業調査** |
| `src/handlers/company/render.rs:8` (別タブ H2) | **🔎 企業分析** |

ユーザー視点フロー: 「求人検索」タブをクリック → 「企業調査」と表示される → 別タブ「企業検索」では「企業分析」と表示。**4 単語が交錯**。

---

## 2. 再構成全体方針

```
hellowork-deploy/
├── README.md                       # OSS 風サマリ (新規参入者の最初の 5 分)
├── CLAUDE.md                       # ★ マスターリファレンス (本プランで全面書き換え)
├── docs/
│   ├── CLAUDE.md                   # ★ docs/ index (新規セクション)
│   ├── USER_GUIDE.md               # 既存維持
│   ├── USER_MANUAL.md              # 既存維持
│   ├── audit_2026_04_24/           # 本監査セット
│   ├── contract_audit_2026_04_23.md
│   ├── flow_ctas_restore.md
│   └── pdf_design_spec_2026_04_24.md
└── src/
    └── handlers/
        └── CLAUDE.md               # ★ ハンドラ別責務一覧 (新規セクション)
```

---

## 3. ルート CLAUDE.md 完全書き換え案 (コピペ可)

```markdown
# V2 ハローワークダッシュボード マスターリファレンス

**最終更新**: 2026-04-26
**リポジトリ**: `makimaki1006/HR_HR`
**デプロイリポ**: `hellowork-deploy/` (Render: `hellowork-dashboard`)
**本ドキュメントの位置付け**: 新規参入者・自分自身が最初に読むべき索引。各深掘り資料は §13 から辿る。

---

## 🔴 絶対ルール (事故由来)

| ルール | 違反時の事故 | feedback 参照 |
|--------|-------------|---------------|
| `git add -A` / `git add .` 禁止。ファイル名を必ず指定 | 2026-03-10 data/geojson_gz/ 47 ファイル誤削除 | `feedback_git_safety.md` |
| コミット前に `git diff --cached --stat` で削除確認。バイナリ削除があれば即停止 | 同上 | `feedback_git_safety.md` |
| DB 書き込み (Turso INSERT/UPDATE/DELETE/CTAS) はユーザー実行のみ | 2026-01 $195 超過請求 | `feedback_turso_priority.md` |
| Turso アップロードは 1 回で完了。何度も DROP+CREATE しない | 2026-04-03 無料枠浪費 | `feedback_turso_upload_once.md` |
| 推測を事実として報告しない。「正常」「問題ない」「大丈夫」禁止。SQL 結果を必ず提示 | 2026-01-05 / 2026-03-17 虚偽報告 | `feedback_never_guess_data.md` |
| 「データ」と聞いたら「人口データか求人データか」を必ず確認 | 2026-03-17 数時間無駄 | `feedback_population_vs_posting.md` |
| HW 掲載求人のみが対象であり、全求人市場ではないことを UI/レポートに必ず明記 | UI 誤認リスク | `feedback_hw_data_scope.md` |
| 雇用形態を必ず dedup キーに含める (V2 では「正社員」、V1 では「正職員」と区別) | 2026-02-24 大量データ消失 | `feedback_dedup_rules.md` |
| 部分コミット時は依存チェーン (`include_str!`/`pub mod`/可視性) を確認。ローカル成功 ≠ 本番成功 | 2026-04-22 Render deploy 失敗 | `feedback_partial_commit_verify.md` |
| 並列 agent 間の契約 (JSON shape 等) は agent 個別テスト pass でも別途 cross-check | 2026-04-23 採用診断 8 panel 全滅 | `feedback_agent_contract_verification.md` |
| テストはデータ妥当性 (具体値) で検証する。「要素存在」だけでは不可 | 2026-03-22 / 2026-04-12 バグ見逃し | `feedback_test_data_validation.md` `feedback_reverse_proof_tests.md` |
| E2E では canvas 存在ではなく ECharts 初期化完了を確認 | 2026-04-08 19/24 ブランク見逃し | `feedback_e2e_chart_verification.md` |
| 相関 ≠ 因果。`phrase_validator` で「確実に」「必ず」「100%」を機械的に排除 | UI 誇大表現リスク | `feedback_correlation_not_causation.md` |
| 仮説なきデータ投入は無意味。So What を先に設計 | 営業ツール化失敗 | `feedback_hypothesis_driven.md` |

---

## 🔴 V1 / V2 分離

| | V1: ジョブメドレー (求職者) | V2: ハローワーク (求人) |
|---|---|---|
| リポ | `makimaki1006/rust-dashboard` | `makimaki1006/HR_HR` |
| デプロイリポ | `rust-dashboard-deploy/` | **`hellowork-deploy/` (本リポ)** |
| データソース | ジョブメドレースクレイピング | ハローワーク掲載求人 (469,027 件) |
| DB | 3 個 (postings + segment + geocoded) | **1 個 ローカル + 3 個 Turso (本リポは V2 側)** |
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
| 認証 | bcrypt / 平文 / 外部期限付きパスワード + ドメイン許可 + IP レート制限 |
| 雇用形態 | 正社員 / パート / その他 (3 値、survey は 4 値、jobmap は 4 値) |
| ポート | 9216 (デフォルト、`PORT` env で上書き) |
| デプロイ | Render Free / Docker / `hr-hw.onrender.com` |

---

## 2. アーキテクチャ概観

```
[Python ETL]                   [Rust Dashboard]
hellowork_etl.py               main.rs
   │ 418 列 CSV (CP932)             │ ① decompress_geojson_if_needed()
   ▼                                │ ② precompress_geojson()
hellowork_compute_layers.py    │ ③ decompress_db_if_needed()
   │ Layer A/B/C 9 テーブル          │ ④ LocalDb::new() + 19 INDEX
   ▼                                │ ⑤ TursoDb::new() × 3 (graceful)
scripts/compute_v2_*.py × 7    │ ⑥ AppCache (DashMap+TTL+max)
   │ 24 v2_* 分析テーブル            │ ⑦ build_app() → 9 タブ
   ▼                                ▼
hellowork.db (~1.6GB)          axum::serve (port 9216)
   │ gzip → ~297MB                  │
   ▼                                │ ┌─ /tab/* (HTML partial)
GitHub Release (db-v2.0)       │ ├─ /api/* (JSON)
   │ download_db.sh                 │ ├─ /report/* (印刷 HTML)
   ▼                                │ └─ /api/v1/* (認証不要 MCP)
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
│   └── handlers/            # 9 タブ + admin + my + api + api_v1 (§ 7 参照)
├── templates/
│   ├── dashboard_inline.html  # 9 タブ UI (現行)
│   ├── login_inline.html
│   ├── tabs/                  # competitive, jobmap, recruitment_diag, region_karte 等
│   └── dashboard.html         # ★ V1 遺物、未参照 (削除候補)
├── static/css/, static/js/    # ECharts/Leaflet/HTMX 連携 JS
├── data/
│   ├── hellowork.db           # 起動時に hellowork.db.gz から解凍 (git 非追跡)
│   └── geojson_gz/*.json.gz   # 起動時に static/geojson/ へ解凍
└── docs/                      # § 13 参照
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
| 求人検索 | `/tab/competitive` | `handlers::competitive::tab_competitive` | 多次元フィルタ → 求人一覧 + 個別詳細 + 集計 |
| 条件診断 | `/tab/diagnostic` | `handlers::diagnostic::tab_diagnostic` | 月給/休日/賞与/雇用形態 → 6 軸レーダー + S/A/B/C/D グレード |
| 採用診断 | `/tab/recruitment_diag` | `handlers::recruitment_diag::tab_recruitment_diag` | 業種×エリア×雇用形態で 8 panel 並列ロード (採用難度・人材プール・流入元・競合・条件ギャップ・市場動向・穴場・AI 示唆) |
| 企業検索 | `/tab/company` | `handlers::company::tab_company` | SalesNow 198K 社 検索 → プロフィール × HW × 外部統計 |
| 媒体分析 | `/tab/survey` | `handlers::survey::tab_survey` | Indeed/求人ボックス CSV アップ → HW 統合 → 印刷 HTML |

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

`§9 データソースマップ` に詳細表。要点のみここに:

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
- **v2_** 分析テーブル (24 個): Phase 1 〜 Phase 5 + Phase 2 拡張、§5.3 参照
- **survey_records / survey_sessions / ts_agg_***: 媒体分析セッション保存 + 集計キャッシュ

### 5.2 Turso `country-statistics` (Round 1-3 の主データ)

| 系統 | テーブル | 用途 |
|------|---------|------|
| 外部統計 (e-Stat / SSDSE) | `v2_external_population`, `v2_external_population_pyramid`, `v2_external_migration`, `v2_external_daytime_population`, `v2_external_minimum_wage`, `v2_external_minimum_wage_history`, `v2_external_prefecture_stats`, `v2_external_job_openings_ratio`, `v2_external_labor_stats`, `v2_external_labor_force`, `v2_external_establishments`, `v2_external_turnover`, `v2_external_household_spending`, `v2_external_business_dynamics`, `v2_external_climate`, `v2_external_care_demand`, `v2_external_foreign_residents`, `v2_external_education`, `v2_external_education_facilities`, `v2_external_household`, `v2_external_households`, `v2_external_industry_structure`, `v2_external_internet_usage`, `v2_external_boj_tankan`, `v2_external_social_life`, `v2_external_land_price`, `v2_external_car_ownership`, `v2_external_medical_welfare`, `v2_external_geography`, `v2_external_vital_statistics`, `v2_external_commute_od` | SSDSE-A + e-Stat API 由来。30+ テーブル、~40,944 行 (memory:project_external_data_expansion_2026_04) |
| Agoop 人流 | `v2_flow_mesh1km_2019` / `_2020` / `_2021` (合計 38M 行)、`v2_flow_master_prefcity`, `v2_flow_fromto_city`, `v2_flow_attribute_mesh1km`, `v2_posting_mesh1km` | Round 2 人流分析の元データ |
| HW 時系列 | `ts_turso_counts`, `ts_turso_salary`, `ts_turso_vacancy`, `ts_turso_fulfillment` | 月次推移 (memory:project_hw_timeseries_analysis、~16 万行) |
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

### 5.5 24 個の v2_* 分析テーブル詳細

(2026-03-14版 CLAUDE.md の §4.3 と同等。簡易版のみここに、詳細は元 §4.3 を維持)

| Phase | テーブル数 | 主アルゴリズム |
|-------|----------|--------------|
| 1 (基本指標) | `v2_vacancy_rate`, `v2_regional_resilience`, `v2_transparency_score` | recruitment_reason 比率 / Shannon H / HHI / 8 任意開示項目 |
| 1b (給与) | `v2_salary_structure`, `v2_salary_competitiveness`, `v2_compensation_package` | P10/P25/P50/P75/P90, 推定年収, S/A/B/C/D ランク |
| 2 (テキスト) | `v2_text_quality`, `v2_keyword_profile`, `v2_text_temperature` | 文字数 × ユニーク率, 6 カテゴリ KW, (緊急-選択)/‰ |
| 3 (市場構造) | `v2_employer_strategy(_summary)`, `v2_monopsony_index`, `v2_spatial_mismatch`, `v2_cross_industry_competition` | 給与 × 福利 4 象限, HHI/Gini, Haversine 30/60km, 業種重複 |
| 4 (外部) | `v2_external_minimum_wage`, `v2_wage_compliance`, `v2_region_benchmark` | 最低賃金 違反率, 6 軸ベンチマーク |
| 5 (予測) | `v2_fulfillment_summary`, `v2_mobility_estimate`, `v2_shadow_wage` | LightGBM 5-fold CV, 重力モデル, P10〜P90 |
| 2 拡張 | `v2_anomaly_stats`, `v2_cascade_summary` | 2σ 異常値, 都道府県→市区町村→産業 |

⚠ **vacancy_rate の意味**: 「recruitment_reason_code=1 (欠員補充) を理由とする求人の割合」であり、労働経済学の欠員率 (未充足求人/常用労働者数) **ではない**。UI 表記統一が課題 (P0 #3、`docs/audit_2026_04_24/team_gamma_domain.md` M-1)。

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

- `docs/turso_import_ssdse_phase_a.md`: SSDSE-A
- `docs/turso_import_agoop.md`: Agoop 人流
- `docs/maintenance_posting_mesh1km.md`: posting_mesh1km

⚠ **Turso 書き込みはユーザー実行のみ** (`feedback_turso_priority.md`)。

---

## 7. 9 タブ機能サマリ

| # | タブ (UI 表示) | URL | ハンドラ | 主データ | 主出力 |
|---|--------------|-----|---------|----------|--------|
| 1 | 市場概況 | `/tab/market` | `market.rs` | postings + v2_external_* | KPI 4 + 比較バー 3 + チャート 5 |
| 2 | 地図 | `/tab/jobmap` | `jobmap/` (15 ファイル) | postings + v2_flow_* + v2_salesnow_* + v2_company_geocode | Leaflet 地図 + 6 レイヤー切替 + 半径検索 + 相関散布図 |
| 3 | 地域カルテ | `/tab/region_karte` | `region/karte.rs` | postings + v2_external_* + v2_flow_* | 9 KPI + 7 セクション + 印刷 HTML |
| 4 | 詳細分析 | `/tab/analysis` | `analysis/` (4 ファイル、render.rs 4,594 行) | 全 v2_* + ts_turso_* | 構造分析 / トレンド / 総合診断 グループ × サブタブ × 28 セクション |
| 5 | 求人検索 | `/tab/competitive` | `competitive/` | postings | 多次元フィルタ + 一覧 + 個別 + 集計レポート |
| 6 | 条件診断 | `/tab/diagnostic` | `diagnostic.rs` | postings + v2_vacancy_rate 等 | 月給/休日/賞与 → 6 軸レーダー + S/A/B/C/D |
| 7 | 採用診断 | `/tab/recruitment_diag` | `recruitment_diag/` (10 ファイル) | postings + v2_external_* + v2_flow_* + v2_salesnow_* | 8 panel 並列 (難度/プール/流入/競合/条件/動向/穴場/AI) |
| 8 | 企業検索 | `/tab/company` | `company/` | v2_salesnow_* + v2_external_prefecture_stats + postings | 検索 → プロフィール × HW × 外部統計 |
| 9 | 媒体分析 | `/tab/survey` | `survey/` (10+ ファイル、report_html.rs 3,912 行) | CSV upload + postings + ts_turso_counts | アップ → HW 統合 → 印刷 HTML / ダウンロード HTML |

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
| 3 | `/api/recruitment_diag/inflow` | 流入元分析 (注: v2_flow_fromto_city は 83% のみ投入済、`recruitment_diag/handlers.rs:23`) | v2_flow_fromto_city |
| 4 | `/api/recruitment_diag/competitors` | 競合企業ランキング | v2_salesnow_companies + postings |
| 5 | `/api/recruitment_diag/condition_gap` | 条件ギャップ (自社入力 vs 中央値、HW 給与は市場実勢より低めの注記あり) | postings (median by ORDER BY LIMIT OFFSET) |
| 6 | `/api/recruitment_diag/market_trend` | 市場動向 (job_type 指定時は ts_turso_salary 由来サンプル件数) | ts_turso_* |
| 7 | `/api/recruitment_diag/opportunity_map` | 穴場マップ (Panel 1 の市区町村展開) | postings + v2_flow_* |
| 8 | `/api/recruitment_diag/insights` | AI 示唆統合 (38 patterns 配信) | InsightContext |

⚠ **Panel 5 emp_type フィルタは `expand_employment_type` 未経由**。UI 値そのままで postings.employment_type を検索するため、ヒット 0 で「データ不足」誤表示の可能性 (P0 #8、`team_gamma_domain.md` §5-4)。

---

## 8. 環境変数 19 個

### 8.1 config.rs 管理 (15 個)

| 変数 | デフォルト | 用途 | 未設定時影響 |
|------|----------|------|-------------|
| `PORT` | `9216` | HTTP リッスンポート | デフォルト使用 |
| `AUTH_PASSWORD` | "" | 平文パスワード (社内・無期限) | 認証 OFF (`auth_password.is_empty() && auth_password_hash.is_empty()` で OFF) |
| `AUTH_PASSWORD_HASH` | "" | bcrypt ハッシュ (社内・無期限) | 同上 |
| `AUTH_PASSWORDS_EXTRA` | "" | 外部期限付きパスワード `pass1:2026-06-30,pass2:2026-12-31` | 外部認証なし |
| `ALLOWED_DOMAINS` | `f-a-c.co.jp,cyxen.co.jp` | 社内ドメイン (カンマ区切り) | デフォルト 2 ドメイン |
| `ALLOWED_DOMAINS_EXTRA` | "" | 外部追加ドメイン | 追加なし |
| `HELLOWORK_DB_PATH` | `data/hellowork.db` | SQLite ファイルパス | デフォルト |
| `CACHE_TTL_SECS` | `1800` (30 分) | DashMap TTL | デフォルト |
| `CACHE_MAX_ENTRIES` | `3000` | DashMap 最大エントリ | デフォルト |
| `RATE_LIMIT_MAX_ATTEMPTS` | `5` | ログイン失敗上限 | デフォルト |
| `RATE_LIMIT_LOCKOUT_SECONDS` | `300` (5 分) | ロックアウト秒数 | デフォルト |
| `AUDIT_TURSO_URL` | "" | 監査 DB URL | 監査機能 OFF (`/admin/*` 403) |
| `AUDIT_TURSO_TOKEN` | "" | 監査 DB トークン | 同上 |
| `AUDIT_IP_SALT` | `hellowork-default-salt` | IP ハッシュ用 salt | ⚠ デフォルトのままだとレインボーテーブル攻撃容易、本番では必須変更 |
| `ADMIN_EMAILS` | "" | 管理者メール (カンマ区切り) | role=admin 付与なし |

### 8.2 main.rs 直接読出 (4 個、🔴 config.rs 統合違反、P0 #4)

| 変数 | 用途 | 未設定時影響 |
|------|------|-------------|
| `TURSO_EXTERNAL_URL` | country-statistics URL | 外部統計タブ全機能 OFF (詳細分析 / 地域カルテ / 採用診断 / 媒体分析の HW 統合 / 一部 insight) |
| `TURSO_EXTERNAL_TOKEN` | country-statistics トークン | 同上 |
| `SALESNOW_TURSO_URL` | SalesNow URL | 企業検索タブ機能 OFF + 採用診断 Panel 4 (競合) + 地図 labor-flow / company-markers が空応答 |
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

詳細は `docs/audit_2026_04_24/00_overall_assessment.md` 参照。

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
   - 雇用形態を必ず dedup キーに含める (`feedback_dedup_rules.md`)
   - 最小サンプル数チェック (n≥30 が標準)
2. **Rust fetch** `src/handlers/analysis/fetch.rs`:
   - `query_3level()` で 市区町村→都道府県→全国 の自動フォールバック
   - または `table_exists(db, "v2_NEW")` + `query_turso_or_local()`
3. **Rust render** `src/handlers/analysis/render.rs`:
   - `escape_html()` 必須 (XSS 防止)
   - ECharts: `<div class="echart" data-chart-config='JSON'>` (htmx:afterSettle で自動 setOption)
   - サンプル件数表示 (n=...) で誠実性担保
4. **テスト** (`feedback_test_data_validation.md` / `feedback_reverse_proof_tests.md`):
   - 「要素存在」ではなく「具体値」で逆証明
   - `phrase_validator::assert_valid_phrase()` を必ず通す (相関 ≠ 因果)
5. **Turso 投入** (ユーザー手動実行のみ):
   - `turso_sync.py` 等の冪等インポート
   - 1 回で完了させる (`feedback_turso_upload_once.md`)
6. **デプロイ**: gzip → gh release upload → Render Manual Deploy

---

## 12. memory feedback ルール → 実コード対応

§ 12 (本ファイル別セクション) と `MEMORY.md` 参照。各ルールがどのファイル・どの仕組みで遵守されているかは `docs/audit_2026_04_24/plan_p4_documentation.md` § 12 表を参照。

---

## 13. ドキュメント索引

### 設計仕様

| ファイル | 内容 |
|---------|------|
| `docs/USER_GUIDE.md` | エンドユーザー向け使い方 (タブ別) |
| `docs/USER_MANUAL.md` | 詳細マニュアル |
| `docs/openapi.yaml` | `/api/v1/*` (MCP/AI 連携) の OpenAPI |
| `docs/pdf_design_spec_2026_04_24.md` | PDF レポート設計 (Agent P1/P2/P3 体制) |
| `docs/design_ssdse_a_*.md` | SSDSE-A 統計テーブル設計 (backend/frontend/expansion) |
| `docs/design_agoop_*.md` | Agoop 人流データ設計 (backend/frontend/jinryu) |
| `docs/requirements_*.md` | 要件定義 |

### 運用・移行

| ファイル | 内容 |
|---------|------|
| `docs/turso_import_ssdse_phase_a.md` | SSDSE-A 投入手順 |
| `docs/turso_import_agoop.md` | Agoop 人流投入手順 |
| `docs/maintenance_posting_mesh1km.md` | posting_mesh1km メンテ |
| `docs/flow_ctas_restore.md` | ★ 5/1 後の CTAS 戻し手順 (CTAS 投入 + Rust 戻し + 逆証明) |
| `docs/contract_audit_2026_04_23.md` | 全タブ契約監査 (Mismatch #1-#5) |
| `docs/qa_integration_round_1_3.md` | Round 1-3 QA 統合 |

### 監査

| ファイル | 内容 |
|---------|------|
| `docs/audit_2026_04_24/00_overall_assessment.md` | 5 チーム統合監査 (本リファレンスの根拠) |
| `docs/audit_2026_04_24/team_alpha_userfacing.md` | User-facing (260行) |
| `docs/audit_2026_04_24/team_beta_system.md` | System Integrity (234行) |
| `docs/audit_2026_04_24/team_gamma_domain.md` | Domain Logic 38 patterns (500行) |
| `docs/audit_2026_04_24/team_delta_codehealth.md` | Code Health (486行) |
| `docs/audit_2026_04_24/team_epsilon_walkthrough.md` | Persona Walkthrough (295行) |
| `docs/audit_2026_04_24/plan_p4_documentation.md` | ★ 本書再構成プラン (用語統一・38 patterns カタログ等) |

### テスト

| ファイル | 内容 |
|---------|------|
| `docs/E2E_TEST_PLAN.md` / `_V2.md` | E2E 計画 |
| `docs/E2E_COVERAGE_MATRIX.md` | カバレッジ |
| `docs/E2E_REGRESSION_GUIDE.md` | リグレッション運用 |
| `docs/SESSION_SUMMARY_*.md` | セッションサマリ |

---

**改訂履歴**:
- 2026-04-26: 全面再構成 (P4 / audit_2026_04_24 #10 対応)。9 タブ・Round 1-3・SalesNow・envvar 19 個・dead route 6 件・memory feedback 14 ルール 反映
- 2026-03-14: 旧版 (8 タブ + サブタブ)
```

---

## 4. docs/CLAUDE.md ドラフト

現在は空テンプレ。以下のように index 化を提案。

```markdown
# docs/ ディレクトリ index

**位置付け**: 設計仕様 / 運用手順 / 監査レポート / E2E 計画 のハブ。
**マスターリファレンス**: ルート `CLAUDE.md` を最初に読むこと。

---

## カテゴリ別

### 1. ユーザー向け
- [`USER_GUIDE.md`](USER_GUIDE.md) — タブ別の使い方
- [`USER_MANUAL.md`](USER_MANUAL.md) — 詳細マニュアル

### 2. 設計仕様
- [`openapi.yaml`](openapi.yaml) — `/api/v1/*` (MCP/AI 連携)
- [`pdf_design_spec_2026_04_24.md`](pdf_design_spec_2026_04_24.md) — PDF レポート設計
- [`design_ssdse_a_backend.md`](design_ssdse_a_backend.md) / [`_frontend.md`](design_ssdse_a_frontend.md) / [`_expansion.md`](design_ssdse_a_expansion.md) — SSDSE-A
- [`design_agoop_backend.md`](design_agoop_backend.md) / [`_frontend.md`](design_agoop_frontend.md) / [`_jinryu.md`](design_agoop_jinryu.md) — Agoop 人流
- [`requirements_agoop_jinryu.md`](requirements_agoop_jinryu.md) / [`requirements_ssdse_a_expansion.md`](requirements_ssdse_a_expansion.md) — 要件

### 3. 運用・移行手順
- [`flow_ctas_restore.md`](flow_ctas_restore.md) — ★ 5/1 期日: CTAS 戻し手順
- [`turso_import_ssdse_phase_a.md`](turso_import_ssdse_phase_a.md) — SSDSE-A 投入
- [`turso_import_agoop.md`](turso_import_agoop.md) — Agoop 投入
- [`maintenance_posting_mesh1km.md`](maintenance_posting_mesh1km.md) — メッシュメンテ

### 4. 監査・QA
- [`audit_2026_04_24/`](audit_2026_04_24/) — ★ 2026-04-24 全面監査 (5 チーム + 統合 + P4 ドキュ再構成)
- [`contract_audit_2026_04_23.md`](contract_audit_2026_04_23.md) — 全タブ契約監査 (Mismatch #1-#5)
- [`qa_integration_round_1_3.md`](qa_integration_round_1_3.md) — Round 1-3 統合 QA
- [`industry-filter-review-report.md`](industry-filter-review-report.md) — 産業フィルタレビュー
- [`5EXPERT_REVIEW_REPORT.md`](5EXPERT_REVIEW_REPORT.md) — 5 専門家レビュー

### 5. E2E テスト
- [`E2E_TEST_PLAN.md`](E2E_TEST_PLAN.md) / [`_V2.md`](E2E_TEST_PLAN_V2.md) — E2E 計画 (機能/UX)
- [`E2E_COVERAGE_MATRIX.md`](E2E_COVERAGE_MATRIX.md) — カバレッジマトリクス
- [`E2E_REGRESSION_GUIDE.md`](E2E_REGRESSION_GUIDE.md) — リグレッション運用
- [`E2E_RESULTS_LATEST.md`](E2E_RESULTS_LATEST.md) — 最新結果

### 6. 計画・進捗
- [`IMPLEMENTATION_PLAN_V2.md`](IMPLEMENTATION_PLAN_V2.md) — V2 実装計画
- [`IMPROVEMENT_ROADMAP_V2.md`](IMPROVEMENT_ROADMAP_V2.md) — 改善ロードマップ
- [`SESSION_SUMMARY_2026-04-12.md`](SESSION_SUMMARY_2026-04-12.md) — セッションサマリ

---

## 命名規則
- `design_*.md`: 機能設計仕様 (前置き不要、最初から仕様)
- `requirements_*.md`: 要件定義
- `turso_import_*.md` / `maintenance_*.md`: 運用手順
- `*_audit_*.md`: 監査レポート
- `E2E_*.md`: テスト関連

## 新規追加時のルール
1. ファイル名は kebab-case または `FEATURE_TYPE.md`
2. 先頭に **作成日** + **対象範囲** を明記
3. ルート CLAUDE.md `§13 ドキュメント索引` に追加リンクすること
```

---

## 5. src/handlers/CLAUDE.md ドラフト

現在は空テンプレ。ハンドラ別責務一覧として再構成:

```markdown
# src/handlers/ ハンドラ別責務リファレンス

**マスター**: ルート `CLAUDE.md §3 ルーター総覧` を先に読むこと。本ファイルはコード探索の入口。

---

## 1. ファイル構成

### 1.1 タブハンドラ (UI 公開)

| ハンドラ | UI 表示 | URL | ファイル数 | 主要ファイル |
|---------|---------|-----|----------|------------|
| `market.rs` | 市場概況 | `/tab/market` | 1 | `market.rs` (動的 HTML 生成) |
| `jobmap/` | 地図 | `/tab/jobmap` | 15 | `handlers.rs` (1,103行), `flow.rs`, `company_markers.rs`, `heatmap.rs`, `inflow.rs`, `correlation.rs` |
| `region/` | 地域カルテ | `/tab/region_karte` | 2 | `karte.rs` (1,511行) |
| `analysis/` | 詳細分析 | `/tab/analysis` | 4 | `handlers.rs`, `fetch.rs` (1,897行 / 22 fetch 関数), `render.rs` (4,594行 / 28 セクション), `helpers.rs` |
| `competitive/` | 求人検索 | `/tab/competitive` | 4 | `handlers.rs`, `fetch.rs` (1,033行), `render.rs`, `tests.rs` |
| `diagnostic.rs` | 条件診断 | `/tab/diagnostic` | 1 | `diagnostic.rs` (1,203行、`evaluate_diagnostic`) |
| `recruitment_diag/` | 採用診断 | `/tab/recruitment_diag` | 10 | `handlers.rs` (8 panel API), `competitors.rs`, `condition_gap.rs`, `market_trend.rs`, `opportunity_map.rs`, `insights.rs`, `contract_tests.rs` |
| `company/` | 企業検索 | `/tab/company` | 4 | `render.rs` (1,365行), `fetch.rs`, `handlers.rs` |
| `survey/` | 媒体分析 | `/tab/survey` | 10+ | `aggregator.rs` (1,259行), `report_html.rs` (3,912行), `location_parser.rs` (1,313行), `statistics.rs`, `hw_enrichment.rs`, `integration.rs`, `parser_aggregator_audit_test.rs`, `report_html_qa_test.rs` |

### 1.2 dead route ハンドラ (UI 到達不可)

| ハンドラ | URL | 旧用途 |
|---------|-----|-------|
| `overview.rs` | `/tab/overview` | V1 ダッシュボード遺物。`{{AVG_AGE}}=月給`, `{{MALE_COUNT}}=正社員数` の取り違え変数 (危険) |
| `balance.rs` | `/tab/balance` | 旧 6 タブ UI |
| `workstyle.rs` | `/tab/workstyle` | 同上 |
| `demographics.rs` | `/tab/demographics` | 同上 |
| `trend/` | `/tab/trend` | analysis 内サブグループから到達中 |
| `insight/` | `/tab/insight` | 同上、ただし `/api/insight/report*` `/report/insight` は活きている可能性 |

⚠ 削除前に外部 API 利用ログを確認 (`/api/insight/report*` は xlsx/JSON 出力経路として実用されている可能性)。

### 1.3 補助・管理

| ハンドラ | 用途 |
|---------|------|
| `helpers.rs` | 共通ユーティリティ (`escape_html`, `escape_url_attr`, `get_str/i64/f64`, `format_number`, `Row` 型) |
| `api.rs` | フィルタカスケード API (`/api/prefectures`, `/api/municipalities_cascade`, `/api/industry_tree`, `/api/geojson/*`) |
| `api_v1.rs` | 認証不要 MCP/AI 連携 (`/api/v1/companies/*`) |
| `guide.rs` | `/tab/guide` 使い方ガイド + 凡例 + 用語解説 |
| `admin/` | `/admin/*` 管理者画面 (role=admin 必須、audit DB 必須) |
| `my/` | `/my/profile`, `/my/activity` 自己サービス |
| `global_contract_audit_test.rs` | 横断契約テスト (Mismatch #1-#5 の bug marker テスト 2 件 `#[ignore]` で固定中) |
| `mod.rs` | サブモジュール宣言 |

### 1.4 insight サブモジュール (詳細)

| ファイル | 用途 |
|---------|------|
| `engine.rs` (1,740行) | 22 patterns (HS/FC/RC/AP/CZ/CF) + 構造分析 6 patterns (LS/HH/MF/IN/GE) |
| `engine_flow.rs` (359行) | SW-F01〜F10 (Agoop 人流) 10 patterns |
| `helpers.rs` | InsightCategory / Severity / 閾値定数 (VACANCY_*, SALARY_COMP_*, etc.) |
| `phrase_validator.rs` | 「確実に」「必ず」「100%」を機械的に排除 (相関≠因果) |
| `flow_context.rs` | FlowIndicators 計算 (CTAS fallback 3 箇所、5/1 期日) |
| `fetch.rs` | InsightContext 構築 |
| `handlers.rs` | `/api/insight/*` `/report/insight` |
| `render.rs` (1,605行) | サブタブ render + report HTML |
| `export.rs` | xlsx エクスポート |
| `report.rs` | レポート構造体 |
| `pattern_audit_test.rs` (1,767行) | 22 patterns 検証 |

---

## 2. 設計パターン

### 2.1 HTML partial + `data-chart-config`

タブハンドラは `Html<String>` を返却。ECharts 設定は `<div class="echart" data-chart-config='JSON-encoded option'>` で埋め込み、`static/js/app.js` が htmx:afterSettle で `setOption` する自動初期化方式。

**メリット**: backend は HTML 文字列のみ気にすれば良く、JSON key ミスマッチが構造的に発生しない (`docs/contract_audit_2026_04_23.md` §2 で全タブ確認済)。

**例外**: `jobmap/` は JSON 端点が多く、Mismatch #1-#5 が発生済 (`global_contract_audit_test.rs` 参照)。

### 2.2 query_3level

`analysis/fetch.rs::query_3level` は市区町村 → 都道府県 → 全国 で自動フォールバック (NULL/0 件時)。サンプル件数の妥当性確保。

### 2.3 spawn_blocking

`db::local_sqlite` は同期 (r2d2)。tokio 上では `tokio::task::spawn_blocking()` で別スレッド実行。Turso 接続も同様 (`reqwest::blocking` を async コンテキスト内で作るとパニック)。

### 2.4 emp_group 雇用形態セグメント

全 v2_* テーブルに `emp_group` カラム (`正社員` / `パート` / `その他`)。
⚠ 二重定義の問題: `survey/aggregator.rs:678-682` と `recruitment_diag/mod.rs:74-81` で契約社員・業務委託のグループが異なる (P1 #9、`team_gamma_domain.md` §3-4)。

### 2.5 phrase_validator

`insight/phrase_validator.rs` で「確実に」「必ず」「100%」「絶対」等の禁止表現を走時検証。
✅ 適用済み: SW-F01〜F10、LS/HH/MF/IN/GE
🟡 未適用: HS/FC/RC/AP/CZ/CF の 22 patterns (P2、`team_gamma_domain.md` §1-1 全般)

---

## 3. テスト指針

(`feedback_test_data_validation.md` / `feedback_reverse_proof_tests.md` 参照)

- **要素存在チェック禁止**: `assert!(html.contains("<canvas"))` ではなく具体値で逆証明
- **ECharts チャートは初期化完了確認** (`feedback_e2e_chart_verification.md`)
- **集計ロジックは具体値で検証** (例: 「東京都全産業の正社員割合は X%」)
- **契約は cross-check** (`feedback_agent_contract_verification.md`、`global_contract_audit_test.rs`)

---

**最終更新**: 2026-04-26 (P4 / audit_2026_04_24 #10 対応)
```

---

## 6. 用語統一の意思決定提案

### 6.1 4 案の比較

| 案 | UI/UX 影響 | コード変更 | ペルソナ整合 | デメリット |
|----|----------|----------|------------|----------|
| A. **求人検索** に統一 | UI ナビ既に「求人検索」のため変更ゼロ。ユーザーが既に慣れている | H2 / コメント / 関数名 / URL を `competitive` から `job_search` 等へ。中規模 | ペルソナ B/C (HR担当・営業) が「求人を検索する」と直感的に読める | URL 変更で外部ブックマーク破壊。`competitive` の歴史的経緯 (競合他社調査) との整合喪失 |
| B. **競合調査** に統一 | UI ナビを「競合調査」に変更 (慣れ直し) | コメントは既に「競合調査」、URL `competitive` も整合 | ペルソナ A (採用コンサル) は「競合調査」用語に親和的 | UI 既に「求人検索」で公開済み、再学習コスト。ペルソナ B/C は「競合」と聞いてピンとこない |
| C. **企業調査** に統一 | UI ナビ + H2 + URL 全変更 | 影響箇所最大 | "企業" は別タブ「企業検索」(`/tab/company`) と完全に重複 | 「企業検索」と「企業調査」の区別が UI 上不可能。混乱が悪化 |
| D. **企業分析** に統一 | 同上 | 同上 | "企業分析" も別タブ company の H2 と重複 | C と同じ問題 + 「分析」が `/tab/analysis` (詳細分析) とも被る |

### 6.2 最終提案: **A. 「求人検索」に統一**

**根拠**:
1. **UX 連続性**: ナビが既に「求人検索」で公開済 (`templates/dashboard_inline.html:79`)。ユーザーの再学習コストゼロ。
2. **ペルソナ整合**: ペルソナ B (HR担当) ε監査で達成度 3.7、C (リサーチャー) 3.7。両者の言語が「求人を絞り込む」 (`team_alpha_userfacing.md` §L1)。コンサル A も「競合調査の一部として求人検索する」フローのため、上位概念は「求人検索」で問題ない。
3. **別タブとの非衝突**: 「企業検索」(`/tab/company`) との区別が明確。「求人=jobs」と「企業=companies」の英語対応も自然。
4. **既存 URL 維持可**: `/tab/competitive` の URL は維持しつつ、UI 表示・H2・コメントのみを「求人検索」に統一する**部分的統一**で十分。`competitive.rs` の関数名は internal で UI に出ないため変更不要。

### 6.3 影響箇所一覧 (修正範囲)

| ファイル:行 | 現状 | 修正案 |
|-----------|------|--------|
| `templates/tabs/competitive.html:1` (HTMLコメント) | `<!-- タブ8: 競合調査 -->` | `<!-- タブ5: 求人検索 (URL: /tab/competitive) -->` |
| `templates/tabs/competitive.html:3` (H2 表示) | `🔍 企業調査` | `🔍 求人検索` |
| `src/handlers/competitive/render.rs:30` (関数 doc) | (もしあれば) `/// 競合調査タブのレンダリング` | `/// 求人検索タブのレンダリング (URL: /tab/competitive)` |
| `src/handlers/competitive/handlers.rs` 各 fn doc | コメント参照「競合」 | 「求人検索」 (検索 + grep で精査) |
| `templates/dashboard_inline.html:79` (UI ボタン) | `求人検索` | (変更なし、既に整合) |

**追加推奨**: `src/handlers/company/render.rs:8` H2 の **「🔎 企業分析」** も `/tab/company` の用途 (SalesNow 企業プロフィール表示) からは「**🔎 企業検索**」が適切。「分析」用語は `/tab/analysis` (詳細分析) に予約。

---

## 7. タブ呼称リファレンステーブル

| # | UI 表示 (推奨統一後) | URL | 関数名 | ファイル | コメント (推奨統一後) |
|---|--------------------|-----|--------|---------|--------------------|
| 1 | 市場概況 | `/tab/market` | `tab_market` | `market.rs` | `// タブ1: 市場概況` |
| 2 | 地図 | `/tab/jobmap` | `tab_jobmap` | `jobmap/handlers.rs` | `// タブ2: 地図 (jobmap)` |
| 3 | 地域カルテ | `/tab/region_karte` | `tab_region_karte` | `region/karte.rs` | `// タブ3: 地域カルテ` |
| 4 | 詳細分析 | `/tab/analysis` | `tab_analysis` | `analysis/handlers.rs` | `// タブ4: 詳細分析` |
| 5 | **求人検索** ★ | `/tab/competitive` | `tab_competitive` | `competitive/handlers.rs` | `// タブ5: 求人検索 (URL は competitive)` |
| 6 | 条件診断 | `/tab/diagnostic` | `tab_diagnostic` | `diagnostic.rs` | `// タブ6: 条件診断` |
| 7 | 採用診断 | `/tab/recruitment_diag` | `tab_recruitment_diag` | `recruitment_diag/handlers.rs` | `// タブ7: 採用診断 (8 panel)` |
| 8 | **企業検索** ★ | `/tab/company` | `tab_company` | `company/handlers.rs` | `// タブ8: 企業検索 (SalesNow + HW 結合)` |
| 9 | 媒体分析 | `/tab/survey` | `tab_survey` | `survey/handlers.rs` | `// タブ9: 媒体分析 (CSV upload)` |

★ = 用語統一による呼称変更箇所。

### 7.1 旧称対応 (移行期間用)

| 旧称 | 新称 |
|------|------|
| 競合調査 / 企業調査 | **求人検索** |
| 企業分析 (タブ 8 H2) | **企業検索** |
| 雇用形態別分析 | **詳細分析** (`analysis/handlers.rs:23,36` のフォールバック文言) |
| トレンド (独立タブ風) | **詳細分析 → トレンドサブグループ** (UI では analysis 内) |
| 総合診断 | **詳細分析 → 総合診断サブグループ** (insight、UI では analysis 内) |

---

## 8. 環境変数完全リファレンス

§3 ルート CLAUDE.md ドラフト §8 と同じ。簡略表のみここに再掲:

| # | 変数 | 種別 | デフォルト | 未設定時の挙動 |
|---|------|------|-----------|--------------|
| 1 | `PORT` | config.rs | `9216` | デフォルト使用 |
| 2 | `AUTH_PASSWORD` | config.rs | "" | 認証 OFF (起動可能だが推奨せず) |
| 3 | `AUTH_PASSWORD_HASH` | config.rs | "" | 同上 |
| 4 | `AUTH_PASSWORDS_EXTRA` | config.rs | "" | 外部期限付きパスワードなし |
| 5 | `ALLOWED_DOMAINS` | config.rs | `f-a-c.co.jp,cyxen.co.jp` | デフォルト |
| 6 | `ALLOWED_DOMAINS_EXTRA` | config.rs | "" | 追加なし |
| 7 | `HELLOWORK_DB_PATH` | config.rs | `data/hellowork.db` | デフォルト |
| 8 | `CACHE_TTL_SECS` | config.rs | `1800` | 30 分 TTL |
| 9 | `CACHE_MAX_ENTRIES` | config.rs | `3000` | 3000 |
| 10 | `RATE_LIMIT_MAX_ATTEMPTS` | config.rs | `5` | 5 回 |
| 11 | `RATE_LIMIT_LOCKOUT_SECONDS` | config.rs | `300` | 5 分 |
| 12 | `AUDIT_TURSO_URL` | config.rs | "" | 監査機能 OFF (`/admin/*` 403、活動記録 OFF) |
| 13 | `AUDIT_TURSO_TOKEN` | config.rs | "" | 同上 |
| 14 | `AUDIT_IP_SALT` | config.rs | `hellowork-default-salt` | ⚠ 本番危険 (レインボーテーブル攻撃容易) |
| 15 | `ADMIN_EMAILS` | config.rs | "" | role=admin 付与なし |
| 16 | `TURSO_EXTERNAL_URL` | 🔴 main.rs:83 | (なし) | 外部統計タブ全機能 OFF (詳細分析 / 地域カルテ / 採用診断 / 媒体分析 HW 統合) |
| 17 | `TURSO_EXTERNAL_TOKEN` | 🔴 main.rs:84 | (なし) | 同上 |
| 18 | `SALESNOW_TURSO_URL` | 🔴 main.rs:113,125 | (なし) | 企業検索 / 採用診断 Panel 4 / labor-flow / company-markers 空応答 |
| 19 | `SALESNOW_TURSO_TOKEN` | 🔴 main.rs:114 | (なし) | 同上 |

🔴 = config.rs 未統合 (P0 #4 修正対象、`team_delta_codehealth.md §4.2`)

---

## 9. データソースマップ (完成形)

| 系統 | 種別 | ローカル/Turso | env var | 接続単位 | 主テーブル | 用途タブ |
|------|------|---------------|---------|---------|----------|---------|
| **A. hellowork.db** | SQLite | ローカル (起動時 gz 解凍) | `HELLOWORK_DB_PATH` | r2d2 max10 | postings (469K行) / municipality_geocode / Layer A-C 9 / v2_* 24 / survey_* / ts_agg_* | 全タブ (中核) |
| **B. Turso country-statistics** | libSQL HTTP | Turso 1 系統 | `TURSO_EXTERNAL_URL` `_TOKEN` | spawn_blocking 初期化 | v2_external_* (30+ テーブル、~40K 行) / v2_flow_mesh1km_2019/2020/2021 (38M 行) / v2_flow_master_prefcity / v2_flow_fromto_city / v2_flow_attribute_mesh1km / v2_posting_mesh1km / ts_turso_counts / _salary / _vacancy / _fulfillment / **未投入: v2_flow_city_agg / v2_flow_mesh3km_agg (5/1)** | 詳細分析 / 地域カルテ / 採用診断 / 媒体分析 HW 統合 / 一部 jobmap / insight |
| **C. Turso salesnow** | libSQL HTTP | Turso 2 系統 | `SALESNOW_TURSO_URL` `_TOKEN` | spawn_blocking 初期化、起動キャッシュ無効 (Render OOM) | v2_salesnow_companies (198K 社 × 44列) / v2_industry_mapping / v2_company_geocode | 企業検索 (主) / 採用診断 Panel 4 / 地図 labor-flow / 地図 company-markers |
| **D. Turso audit** | libSQL HTTP | Turso 3 系統 | `AUDIT_TURSO_URL` `_TOKEN` `AUDIT_IP_SALT` | spawn_blocking 初期化、24h purge | accounts / login_sessions / activity / login_failures | `/admin/*` (主) / ログイン履歴 / `/my/activity` |
| **E. GeoJSON 静的** | ファイル | `static/geojson/*.json(.gz)` | (なし) | precompressed_gzip | 47 都道府県 + 市区町村 polygons | 地図 (jobmap) / 地域カルテ / コロプレス |
| **F. CSV upload** | tower-sessions メモリ | リクエストごと一時 | `UPLOAD_BODY_LIMIT_BYTES` (20MB hard) | セッション | (Indeed/求人ボックス CSV) | 媒体分析のみ |

### 9.1 graceful degradation マトリクス

| 系統 | 接続失敗時 | 部分機能停止 | UI 表示 |
|------|----------|------------|--------|
| A. hellowork.db | `tracing::warn!` + `None` | 全タブが空応答 | `<div id="db-warning">⚠️ DB接続エラー</div>` (lib.rs:777) |
| B. country-statistics | 同上 | 詳細分析サブタブ + 地域カルテ + 採用診断 + 媒体分析 HW 統合 が空応答 | 各タブで「データなし」または注記 |
| C. salesnow | 同上 | 企業検索 / 採用診断 Panel 4 / labor-flow / company-markers 空応答 | 同上 |
| D. audit | 同上 | `/admin/*` 403、活動記録 OFF | 該当画面のみ |
| E. GeoJSON | warn ログ | 地図 polygon 描画失敗 | 地図白塗 |
| F. CSV upload | 20MB 超 → 413 | (リクエスト失敗) | エラーバナー |

---

## 10. タブ × データソース 依存マトリクス

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

## 11. insight 38 patterns カタログ (完成形)

`team_gamma_domain.md` を一次ソースに精査。

### 11.1 HS (採用構造分析) 6 patterns

| ID | 名称 | カテゴリ | severity | 閾値 (constants in `helpers.rs`) | 発火条件 | data source | phrase_validator | ファイル:行 |
|----|------|---------|---------|----------------------------------|---------|------------|:----------------:|------------|
| HS-1 | 慢性的人材不足 | HiringStructure | Critical/Warning | `VACANCY_CRITICAL=0.30` `_WARNING=0.20` `_TREND=0.25` | vacancy_rate ≥ 0.20 | v2_vacancy_rate, ts_turso_vacancy | ❌ 未適用 (P2) | `engine.rs:73-144` |
| HS-2 | 給与競争力不足 | HiringStructure | Critical/Warning | `SALARY_COMP_CRITICAL=0.80` `_WARNING=0.90` | local_mean / national_mean ≤ 0.90 | v2_salary_competitiveness | ❌ 未適用 | `engine.rs:147-206` |
| HS-3 | 情報開示不足 | HiringStructure | Critical/Warning | `TRANSPARENCY_CRITICAL=0.40` `_WARNING=0.50` | transparency_score ≤ 0.50 | v2_transparency_score | ❌ 未適用 | `engine.rs:209-271` |
| HS-4 | 温度と採用難の乖離 | HiringStructure | Warning | `TEMP_LOW_THRESHOLD=0.0` (⚠ 根拠不明) | vacancy_rate ≥ Critical かつ temperature < 0 | v2_text_temperature + v2_vacancy_rate | ❌ 未適用 | `engine.rs:274-321` |
| HS-5 | 雇用者集中 | HiringStructure | Warning | `HHI_CRITICAL=0.25` `TOP1_SHARE_CRITICAL=0.30` | HHI > 0.25 OR top1 > 0.30 | v2_monopsony_index | ❌ 未適用 | `engine.rs:324-369` |
| HS-6 | 空間ミスマッチ | HiringStructure | Warning | `ISOLATION_WARNING=0.50` `DAYTIME_POP_RATIO_LOW=0.90` | isolation_score > 0.50 | v2_spatial_mismatch | ❌ 未適用 | `engine.rs:372-422` |

### 11.2 FC (将来予測) 4 patterns

| ID | 名称 | severity | 閾値 | 発火条件 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|---------|------------|:----------------:|------------|
| FC-1 | 求人量トレンド | Warning/Positive | `TREND_INCREASE=0.05` `_DECREASE=-0.05` | 線形外挿 forecast_6m = latest × (1 + slope×6) | ts_turso_counts | ❌ 未適用 | `engine.rs:444-486` |
| FC-2 | 給与上昇圧力 | Warning | (slope 比較) | salary_slope < wage_slope (賃金 < 最低賃金) | v2_external_minimum_wage_history + ts_turso_salary | ❌ 未適用 | `engine.rs:489-540` |
| FC-3 | 人口動態 | Critical/Warning | 0.30 + net_migration<0 で Critical, 0.25 で Warning | 55歳以上 / 生産年齢 ≥ 0.25 | v2_external_population_pyramid + v2_external_migration | ❌ 未適用 | `engine.rs:543-637` |
| FC-4 | 充足困難度悪化 | Warning | days_slope > 0.03 かつ churn_slope > 0.02 | 月次 3% / 2% 同時悪化 | ts_turso_fulfillment | ❌ 未適用 | `engine.rs:640-700` |

### 11.3 RC (地域比較) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| RC-1 | ベンチマーク順位 | Warning/Positive | composite < 30 / > 70 | v2_region_benchmark | ❌ 未適用 | `engine.rs:719-763` |
| RC-2 | 給与・休日地域差 | (各severity) | ±10000円 / -20000円 (固定、職種無視) | v2_salary_structure + holidays | ❌ 未適用 | `engine.rs:766-829` |
| RC-3 | 人口×求人密度 | Warning/Positive | density > 50/千人 (Warning) / < 5/千人 (Positive)、GE-1 と cross-ref | postings + v2_external_population | ❌ 未適用 ✅ caveat あり | `engine.rs:832-898` |

### 11.4 AP (アクション提案) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| AP-1 | 給与改善 | Info | (HS-2 trigger 後) | v2_salary_competitiveness + 全国中央値 | ❌ 未適用 (「到達できます」断定) | `engine.rs:928-971` |
| AP-2 | 求人原稿改善 | Info | 開示率 < 0.30 | v2_transparency_score | ❌ 未適用 | `engine.rs:974-1017` |
| AP-3 | 採用エリア拡大 | Info | daytime_ratio < 1.0 | v2_external_daytime_population | ❌ 未適用 (「可能性」あり) | `engine.rs:1020-1047` |

### 11.5 CZ (通勤圏 距離) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| CZ-1 | 通勤圏人口ポテンシャル | Positive | local_share < 0.05 | v2_external_population (30km 圏) | ❌ 未適用 | `engine.rs:1084-1128` |
| CZ-2 | 通勤圏給与格差 | Warning | ±5%/-10% | v2_salary_structure | ❌ 未適用 | `engine.rs:1131-1180` |
| CZ-3 | 通勤圏高齢化 | Info/Warning | 0.20/0.30 | v2_external_population_pyramid | ❌ 未適用 | `engine.rs:1183-1219` |

### 11.6 CF (通勤フロー) 3 patterns

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| CF-1 | 実通勤フロー | Warning | actual_ratio < 0.01 | v2_external_commute_od | ❌ 未適用 | `engine.rs:1224-1277` |
| CF-2 | 流入元ターゲティング | Info | (流入top抽出) | v2_external_commute_od | ❌ 未適用 | `engine.rs:1280-1306` |
| CF-3 | 地元就業率 | Positive/Warning | 0.7/0.3 | v2_external_commute_od | ❌ 未適用 | `engine.rs:1309-1355` |

### 11.7 構造分析 6 patterns (LS/HH/MF/IN/GE)

| ID | 名称 | severity | 閾値 | data source | phrase_validator | ファイル:行 |
|----|------|---------|------|------------|:----------------:|------------|
| LS-1 | 採用余力シグナル | Warning/Critical | unemployment > 県平均 × 1.2/1.5 | v2_external_labor_force + pref avg | ✅ 適用 | `engine.rs:1399-1445` (⚠「未マッチ層」用語問題) |
| LS-2 | 産業偏在 | Warning | 第3次 ≥ 85% OR 第1次 ≥ 20% | v2_external_industry_structure | ✅ 適用 | `engine.rs:1451-1501` |
| HH-1 | 単独世帯 | Info | 単独世帯率 ≥ 40% (全国 38%) | v2_external_household | ✅ 適用 | `engine.rs:1506-1543` |
| MF-1 | 医療福祉供給密度 | Warning/Critical | local/national < 0.8/0.6 | v2_external_medical_welfare + v2_external_population | ✅ 適用 | ⚠ `engine.rs:1565` 単位 10× バグ疑い (P0 #3) |
| IN-1 | 産業構造ミスマッチ | Warning | `!(0.05..=0.3).contains(&mw_share)` | v2_external_establishments (industry='850') | ✅ 適用 | ⚠ `engine.rs:1637` 発火条件反転疑い |
| GE-1 | 可住地密度 | Warning/Critical | 50-10000 / CRITICAL 20-20000 (人/km²) | v2_external_geography | ✅ 適用 ✅ RC-3 cross-ref | `engine.rs:1666-1740` |

### 11.8 Agoop 人流 SW-F01〜F10

| ID | 名称 | severity | 閾値 (`helpers.rs:185-220`) | 発火条件 | data source | phrase_validator | ファイル:行 |
|----|------|---------|----------------------------|---------|------------|:----------------:|------------|
| SW-F01 | 夜勤需要 | Warning/Critical | `MIDNIGHT_RATIO_WARNING=1.2` `_CRITICAL=1.5` | midnight/daytime ≥ 1.2 | v2_flow_mesh1km_* | ✅ 適用 | `engine_flow.rs:43-70` |
| SW-F02 | 休日商圏不足 | Warning | `HOLIDAY_CROWD_WARNING=1.3` | holiday/weekday ≥ 1.3 | 同上 | ✅ 適用 | `engine_flow.rs:73-95` (⚠ SW-F05 と同時発火) |
| SW-F03 | ベッドタウン | Info | `BEDTOWN_DIFF=0.2` (1-daynight) | daynight < 0.8 かつ outflow ≥ 0.2 | 同上 | ✅ 適用 | `engine_flow.rs:98-125` |
| SW-F04 | メッシュ人材ギャップ | (未実装) | `MESH_ZSCORE=1.5` | None 返却プレースホルダ (v2_posting_mesh1km 投入後拡張) | (将来) | (該当なし) | `engine_flow.rs:128-141` |
| SW-F05 | 観光ポテンシャル | Info | `TOURISM_RATIO=1.5` | holiday/weekday ≥ 1.5 | 同上 | ✅ 適用 | `engine_flow.rs:144-166` (⚠ SW-F02 矛盾) |
| SW-F06 | コロナ回復乖離 | Info | `COVID_FLOW_RECOVERY=0.9` `POSTING_LAG=0.8` | 仕様 AND だが実装は人流のみ | v2_flow_mesh1km_2019/2021 | ✅ 適用 | `engine_flow.rs:169-192` (⚠ 仕様乖離) |
| SW-F07 | 広域流入比率 | Info | `INFLOW_DIFF_REGION=0.15` | diff_region_inflow ≥ 15% | v2_flow_fromto_city | ✅ 適用 | `engine_flow.rs:195-217` |
| SW-F08 | 昼間労働力プール | Info | `DAYTIME_POOL=1.3` | daynight ≥ 1.3 | v2_flow_mesh1km_* | ✅ 適用 | `engine_flow.rs:220-243` (⚠ SW-F03 と中間沈黙) |
| SW-F09 | 季節雇用ミスマッチ | Info | `SEASONAL_AMPLITUDE=0.3` | 月次振幅 ≥ 0.3 | v2_flow_mesh1km_* (12 ヶ月) | ✅ 適用 | `engine_flow.rs:246-269` |
| SW-F10 | 企業立地マッチ | (未実装) | `COMPANY_TIME_DIFF=3h` | None 返却 (v2_posting_mesh1km 依存) | (将来) | (該当なし) | `engine_flow.rs:272-278` |

### 11.9 サマリ統計

- **発火可能**: 36 patterns (HS 6 + FC 4 + RC 3 + AP 3 + CZ 3 + CF 3 + LS 2 + HH 1 + MF 1 + IN 1 + GE 1 + SW-F 8)
- **未実装プレースホルダ**: 2 patterns (SW-F04, SW-F10)
- **phrase_validator 適用済**: 16 patterns (LS/HH/MF/IN/GE 6 + SW-F 10 のうち実装 8)
- **未適用**: 22 patterns (HS/FC/RC/AP/CZ/CF) → P2 改善対象
- **重大バグ疑い**: 3 件 (MF-1 単位、IN-1 反転、SW-F02 vs SW-F05 同時発火)
- **要追加検証**: 4 件 (`team_gamma_domain.md` §残課題 参照)

### 11.10 共通 caveat

- 全 patterns で「相関 ≠ 因果」原則。LS/HH/MF/IN/GE/SW-F は走時 phrase_validator で機械的検証
- 全 insight body に「傾向」「可能性」を含めることを推奨 (今後の HS/FC/RC/AP/CZ/CF にも展開)
- HW 限定性: insight タブヘッダ + 各レポートで明示 (`insight/render.rs:99` "HW（ハローワーク）掲載求人に基づく分析です")

---

## 12. memory feedback ルール → 実コード対応

| ルール (memory file) | 実装/遵守箇所 |
|---------------------|-------------|
| `feedback_dedup_rules.md` (employment_type を dedup キーに) | Python ETL 側の責務。Rust 側では `survey/aggregator.rs:553-672` で emp_group 別集計、`feedback_test_data_validation.md` の test で逆証明 |
| `feedback_git_safety.md` (`git add -A` 禁止) | リポ運用ルール。CI/scripts では未強制 → `.gitignore` 強化が必要 (`team_delta_codehealth.md §8.3`) |
| `feedback_never_guess_data.md` (推測禁止、SQL 結果提示) | コード上は phrase_validator (insight) と契約テスト (`global_contract_audit_test.rs`)。報告フェーズの規律 |
| `feedback_population_vs_posting.md` (人口/求人混同禁止) | `analysis/fetch.rs` で `v2_external_population` と `postings` を明示分離。UI ラベル「人口」「求人数」を使い分け (`market.rs`, `karte.rs`) |
| `feedback_turso_priority.md` (Turso 優先、ローカル更新だけでは本番反映されない) | `query_turso_or_local()` ヘルパー (`analysis/fetch.rs:648` 等) で Turso 優先、ローカル fallback |
| `feedback_hw_data_scope.md` (HW 掲載のみ、市場全体ではない) | 11+ 箇所で明示 (`guide.rs:21,127`, `recruitment_diag/competitors.rs:273-277`, `region/karte.rs:807-808`, `insight/render.rs:99`, `jobmap/correlation.rs:155` 等)。⚠ 市場概況・求人検索・地図メインで欠落 (`team_alpha_userfacing.md §6.1`) |
| `feedback_implement_once.md` (一発で完了、依存把握) | 設計指針。コードでは `pub mod` 管理 (`lib.rs:1-7`) と `mod.rs` で構造化 |
| `feedback_test_data_validation.md` (要素存在ではなくデータ妥当性) | `recruitment_diag/contract_tests.rs`, `survey/parser_aggregator_audit_test.rs`, `pattern_audit_test.rs` で具体値検証 ✅ |
| `feedback_e2e_chart_verification.md` (canvas 存在ではなく ECharts 初期化確認) | `static/js/app.js` で htmx:afterSettle 後に setOption。E2E は `e2e_final_verification.py` 等 |
| `feedback_reverse_proof_tests.md` (具体値で検証、要素存在禁止) | `pattern_audit_test.rs` で 22 patterns 各 body の具体値アサート (1,767 行) ✅ |
| `feedback_turso_upload_once.md` (1 回で完了、何度も DROP+CREATE しない) | Python ETL 側の責務 |
| `feedback_hypothesis_driven.md` (So What を先に設計) | insight 38 patterns が体現。phrase_validator で「示唆」を強制 |
| `feedback_correlation_not_causation.md` (相関 ≠ 因果) | `phrase_validator.rs` で「確実に/必ず/100%/絶対」を機械的禁止 ✅。assert_valid_phrase 適用は LS/HH/MF/IN/GE/SW-F のみ (`engine.rs:1368-1388`) |
| `feedback_partial_commit_verify.md` (依存チェーン確認、ローカル成功 ≠ 本番) | 設計指針。Render の Docker ビルドで `include_str!` 解決を確認すること |
| `feedback_agent_contract_verification.md` (agent 個別 pass でも cross-check) | `global_contract_audit_test.rs` (19,670 B) で複数タブ JSON shape を tempfile DB で逆証明 ✅。bug marker test 2 件 `#[ignore]` で固定中 (P0 #1, #2) |

---

## 13. README.md 修正提案

### 13.1 現状の誤り (2026-04-26 確認)

1. 環境変数名 `TURSO_URL` / `TURSO_TOKEN` / `SESSION_SECRET` は **存在しない**。実際は `TURSO_EXTERNAL_URL/_TOKEN` (3 系統)
2. ポート記述「http://localhost:3000」は誤り。デフォルトは **9216** (`config.rs:55`)
3. パスワード認証「Argon2 ハッシュ」と書かれているが実装は **bcrypt** (`Cargo.toml`)
4. ハローワーク求人「**469,027** 件」は維持
5. 「主な機能」に **採用診断 / 地域カルテ / SalesNow 198K 社** が未記載
6. タブ呼称「競合調査レポート生成」「企業検索」「条件診断」「企業プロフィール」が混在 (用語統一案 §6 で「求人検索」)

### 13.2 推奨 README.md ドラフト (差分のみ)

```markdown
# HR_HR - V2 ハローワーク求人市場分析ダッシュボード

Rust Axum + HTMX + ECharts + Leaflet で構築された求人市場分析ダッシュボード。
ハローワーク求人 469,027 件 + 外部統計 30+ テーブル + Agoop 人流 38M 行 + SalesNow 198K 社 を統合。

## 主な機能 (9 タブ)

- **市場概況**: KPI × 4 + 比較バー + 産業別/職業/雇用形態/給与帯/求人理由
- **地図**: Leaflet 求人マップ + 6 コロプレス + Agoop ヒートマップ + SalesNow 企業マーカー
- **地域カルテ**: 1 市区町村の構造 + 人流 + 求人を 9 KPI + 7 セクション + 印刷可能 HTML
- **詳細分析**: 構造分析 / トレンド / 総合診断 (insight 38 patterns) のサブタブ
- **求人検索**: 多次元フィルタ + 一覧 + 個別詳細 + 集計レポート
- **条件診断**: 月給/休日/賞与 → 6 軸レーダー + S/A/B/C/D グレード
- **採用診断**: 業種×エリア×雇用形態 で 8 panel 並列ロード
- **企業検索**: SalesNow 198K 社 + HW + 外部統計 統合
- **媒体分析**: Indeed/求人ボックス CSV 統合 + HW 比較 + 印刷 HTML

## レポート出力

- `/report/insight` - HW 市場総合診断 (10 ページ A4 横、ECharts SVG、38 patterns)
- `/report/survey` - 媒体分析レポート (CSV + HW 統合、A4 縦)
- `/report/company/{cn}` - 企業プロフィール
- `/api/insight/report/xlsx` - Excel ダウンロード

## セキュリティ

- CSRF: Origin/Referer 検証 (外部オリジン → 403)
- XSS: escape_html + escape_url_attr + sanitize_tag_text
- 20MB body size limit
- 認証: bcrypt + 平文 + 外部期限付きパスワード + ドメイン許可 + IP レート制限

## セットアップ

### 要件
- Rust 1.75+
- Python 3.11+ (E2E)
- SQLite3

### 起動
```bash
cargo build --release
./target/release/rust_dashboard
# http://localhost:9216 (PORT env で上書き可)
```

### 環境変数 (主要)
詳細は `CLAUDE.md §8` 参照 (19 個):

- `HELLOWORK_DB_PATH` (default: `data/hellowork.db`)
- `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN` (外部統計 + 人流)
- `SALESNOW_TURSO_URL` / `SALESNOW_TURSO_TOKEN` (企業検索)
- `AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN` / `AUDIT_IP_SALT` (監査機能)
- `AUTH_PASSWORD` / `AUTH_PASSWORD_HASH` / `ALLOWED_DOMAINS` (認証)
- `PORT` (default: `9216`)

## ドキュメント

**最初に読む**: [`CLAUDE.md`](CLAUDE.md) — マスターリファレンス (9 タブ + データソース + envvar + 38 patterns)

**カテゴリ別**: [`docs/CLAUDE.md`](docs/CLAUDE.md) — docs/ 索引

## ライセンス

Private / Internal use
```

---

## 14. 親セッションへの申し送り Top 5

1. **用語統一は「求人検索」を採用**: ペルソナ B/C 整合性 + UI 連続性 + 別タブ「企業検索」との非衝突。修正は `templates/tabs/competitive.html:1,3` H2 + コメント、`src/handlers/company/render.rs:8` を「企業検索」に変更で完了。URL `/tab/competitive` は維持 (外部ブックマーク保護)。

2. **ルート CLAUDE.md は §3 ドラフトで全面置換すべき**: 現行版は 9 タブ中 4 タブ未記載・SalesNow/採用診断/Round 1-3 全欠落。新規参入者の事故再発リスクが大きい。本書 §3 (約 700 行) はコピペ可能な完成形。

3. **環境変数 4 個 (TURSO_EXTERNAL_*, SALESNOW_TURSO_*) は config.rs に統合必須** (P0 #4): `main.rs:83-145` の直読を `AppConfig` に統合し、`from_env()` で一括検証。テスト容易性向上 + 未設定時警告ログを追加。`AUDIT_IP_SALT` のデフォルト値検出時にも警告ログ推奨 (本番危険)。

4. **README.md の env var 名が誤り**: `TURSO_URL` / `TURSO_TOKEN` / `SESSION_SECRET` は存在しない。実際は `TURSO_EXTERNAL_URL/_TOKEN` で 3 系統 (country / salesnow / audit)。さらにポート記述「localhost:3000」も誤り (実 9216)。`Argon2` も誤り (実 bcrypt)。本書 §13.2 ドラフト適用を推奨。

5. **38 patterns カタログ §11 を `docs/insight_patterns.md` として独立保存推奨**: 11 ファイル/閾値/data source/phrase_validator 適用状況を一覧化。今後の P0 #3 (MF-1)、IN-1 反転、SW-F02/F05 矛盾、SW-F04/F10 未実装 などの修正時に diff の根拠となる。`pattern_audit_test.rs` (1,767 行) の test 命名と本表 ID の対応関係を追記すれば監査の自走化が可能。

---

**作成完了**: 2026-04-26 (P4 / audit_2026_04_24 #10 対応)
**作業時間**: 約 90 分 (調査 60 分 + ドラフト 30 分)
**根拠ファイル**:
- `docs/audit_2026_04_24/00_overall_assessment.md` (統合監査)
- `docs/audit_2026_04_24/team_{alpha,beta,gamma,delta,epsilon}_*.md` (5 チーム個別)
- `CLAUDE.md` (現行 2026-03-14 版)
- `src/{config.rs, main.rs, lib.rs}` (実装一次ソース)
- `src/handlers/insight/{engine.rs, engine_flow.rs, helpers.rs, phrase_validator.rs}` (38 patterns)
- `templates/dashboard_inline.html` (9 タブ UI 一次ソース)
- `docs/contract_audit_2026_04_23.md` (Mismatch #1-#5)
- `docs/flow_ctas_restore.md` (5/1 期日)
- `MEMORY.md` (feedback ルール 14 件、システムリマインダー経由)
