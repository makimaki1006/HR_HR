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

詳細は [`CLAUDE.md`](CLAUDE.md) §8 + [`docs/env_variables_reference.md`](docs/env_variables_reference.md) 参照 (19 個):

- `HELLOWORK_DB_PATH` (default: `data/hellowork.db`)
- `TURSO_EXTERNAL_URL` / `TURSO_EXTERNAL_TOKEN` (外部統計 + 人流)
- `SALESNOW_TURSO_URL` / `SALESNOW_TURSO_TOKEN` (企業検索)
- `AUDIT_TURSO_URL` / `AUDIT_TURSO_TOKEN` / `AUDIT_IP_SALT` (監査機能)
- `AUTH_PASSWORD` / `AUTH_PASSWORD_HASH` / `ALLOWED_DOMAINS` (認証)
- `PORT` (default: `9216`)

## テスト

```bash
# ユニットテスト
cargo test

# E2E (Playwright)
pip install playwright openpyxl pypdf
playwright install chromium
python e2e_final_verification.py  # 最終確認

# 全 E2E (約 30 分)
bash scripts/run_all_e2e.sh
```

## デプロイ

Render の auto-deploy (main ブランチ push で自動反映)。手動 deploy はダッシュボードから。

## ドキュメント

**最初に読む**: [`CLAUDE.md`](CLAUDE.md) — マスターリファレンス (9 タブ + データソース + envvar + 38 patterns)

**カテゴリ別**: [`docs/CLAUDE.md`](docs/CLAUDE.md) — docs/ 索引

主要ドキュメント:
- [`docs/USER_GUIDE.md`](docs/USER_GUIDE.md) - ユーザー向けガイド
- [`docs/E2E_TEST_PLAN.md`](docs/E2E_TEST_PLAN.md) / [`docs/E2E_TEST_PLAN_V2.md`](docs/E2E_TEST_PLAN_V2.md) - E2E テスト計画
- [`docs/E2E_COVERAGE_MATRIX.md`](docs/E2E_COVERAGE_MATRIX.md) - カバレッジマトリクス
- [`docs/E2E_REGRESSION_GUIDE.md`](docs/E2E_REGRESSION_GUIDE.md) - リグレッション運用
- [`docs/audit_2026_04_24/`](docs/audit_2026_04_24/) - 2026-04-24 全面監査

## ライセンス

Private / Internal use
