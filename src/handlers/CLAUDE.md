# src/handlers/ ハンドラ別責務リファレンス

**最終更新**: 2026-04-26
**マスター**: ルート [`CLAUDE.md`](../../CLAUDE.md) §3 ルーター総覧 を先に読むこと。本ファイルはコード探索の入口。

---

## 1. ファイル構成

### 1.1 タブハンドラ (UI 公開、9 タブ)

| ハンドラ | UI 表示 | URL | ファイル数 | 主要ファイル |
|---------|---------|-----|----------|------------|
| `market.rs` | 市場概況 | `/tab/market` | 1 | `market.rs` (動的 HTML 生成) |
| `jobmap/` | 地図 | `/tab/jobmap` | 15 | `handlers.rs` (1,103行), `flow.rs`, `company_markers.rs`, `heatmap.rs`, `inflow.rs`, `correlation.rs` |
| `region/` | 地域カルテ | `/tab/region_karte` | 2 | `karte.rs` (1,511行) |
| `analysis/` | 詳細分析 | `/tab/analysis` | 4 | `handlers.rs`, `fetch.rs` (1,897行 / 22 fetch 関数), `render.rs` (4,594行 / 28 セクション), `helpers.rs` |
| `competitive/` | **求人検索** | `/tab/competitive` | 4 | `handlers.rs`, `fetch.rs` (1,033行), `render.rs`, `tests.rs` |
| `diagnostic.rs` | 条件診断 | `/tab/diagnostic` | 1 | `diagnostic.rs` (1,203行、`evaluate_diagnostic`) |
| `recruitment_diag/` | 採用診断 | `/tab/recruitment_diag` | 10 | `handlers.rs` (8 panel API), `competitors.rs`, `condition_gap.rs`, `market_trend.rs`, `opportunity_map.rs`, `insights.rs`, `contract_tests.rs` |
| `company/` | 企業検索 | `/tab/company` | 4 | `render.rs` (1,365行), `fetch.rs`, `handlers.rs` |
| `survey/` | 媒体分析 | `/tab/survey` | 10+ | `aggregator.rs` (1,259行), `report_html.rs` (3,912行), `location_parser.rs` (1,313行), `statistics.rs`, `hw_enrichment.rs`, `integration.rs`, `parser_aggregator_audit_test.rs`, `report_html_qa_test.rs` |

⚠ **タブ呼称統一**: 「求人検索」を正式呼称とし、UI/H2/コメントで統一。URL `/tab/competitive` は外部ブックマーク互換のため不変。詳細は [`docs/tab_naming_reference.md`](../../docs/tab_naming_reference.md)。

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

### 1.4 insight サブモジュール (38 patterns)

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

詳細な 38 patterns カタログは [`docs/insight_patterns.md`](../../docs/insight_patterns.md) を参照。

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
⚠ 二重定義の問題: `survey/aggregator.rs:678-682` と `recruitment_diag/mod.rs:74-81` で契約社員・業務委託のグループが異なる (P1、`team_gamma_domain.md` §3-4)。

### 2.5 phrase_validator

`insight/phrase_validator.rs` で「確実に」「必ず」「100%」「絶対」等の禁止表現を走時検証。
✅ 適用済み: SW-F01〜F10、LS/HH/MF/IN/GE
🟡 未適用: HS/FC/RC/AP/CZ/CF の 22 patterns (P2)

---

## 3. テスト指針

(`feedback_test_data_validation` / `feedback_reverse_proof_tests` 参照)

- **要素存在チェック禁止**: `assert!(html.contains("<canvas"))` ではなく具体値で逆証明
- **ECharts チャートは初期化完了確認** (`feedback_e2e_chart_verification`)
- **集計ロジックは具体値で検証** (例: 「東京都全産業の正社員割合は X%」)
- **契約は cross-check** (`feedback_agent_contract_verification`、`global_contract_audit_test.rs`)

---

## 4. 採用診断 8 panel ハンドラ対応 (`recruitment_diag/`)

| Panel | API | ハンドラファイル | 主データ |
|-------|-----|----------------|---------|
| 1. 採用難度 | `/api/recruitment_diag/difficulty` | `handlers.rs` | postings + v2_flow_* |
| 2. 人材プール | `/api/recruitment_diag/talent_pool` | `handlers.rs` | v2_flow_* + v2_external_population |
| 3. 流入元分析 | `/api/recruitment_diag/inflow` | `handlers.rs` | v2_flow_fromto_city |
| 4. 競合企業 | `/api/recruitment_diag/competitors` | `competitors.rs` | v2_salesnow_companies + postings |
| 5. 条件ギャップ | `/api/recruitment_diag/condition_gap` | `condition_gap.rs` | postings (median by ORDER BY LIMIT OFFSET) |
| 6. 市場動向 | `/api/recruitment_diag/market_trend` | `market_trend.rs` | ts_turso_* |
| 7. 穴場マップ | `/api/recruitment_diag/opportunity_map` | `opportunity_map.rs` | postings + v2_flow_* |
| 8. AI 示唆 | `/api/recruitment_diag/insights` | `insights.rs` | InsightContext (38 patterns) |

⚠ **2026-04-23 事故対応中核機能**: 8 panel 並列ロード時に JSON shape 契約違反で全滅。`contract_tests.rs` で逆証明テスト追加済。

---

**改訂履歴**:
- 2026-04-26: 全面投入 (P4 / audit_2026_04_24 #10 対応)。9 タブ + dead route 6 + insight サブモジュール + 採用診断 8 panel 反映
- 旧版: 空テンプレ (`*No recent activity*` のみ)
