# HR_HR - ハローワーク求人市場分析ダッシュボード

Rust Axum + HTMX + ECharts + Leaflet で構築された求人市場分析ダッシュボード。
ハローワーク求人データ（469,027件）を軸に、競合調査レポート生成・市場診断・
通勤圏分析を提供。

## 主な機能

- **市場概況タブ**: KPI、産業別求人、雇用条件分析
- **地図タブ**: Leaflet求人マップ + SalesNow企業マーカー
- **詳細分析タブ**: 構造分析/トレンド/総合診断（22パターン）
- **求人検索**: 条件フィルタ付き一覧
- **条件診断**: 月給/休日/賞与から採用難易度A-D判定
- **企業検索**: SalesNow 236,000社の企業プロフィール
- **媒体分析タブ**: Indeed/求人ボックスCSVアップロード → 競合調査レポート生成

## レポート出力

- `/report/insight` - HW市場総合診断レポート（10ページA4横、ECharts SVG）
- `/report/survey` - 競合調査レポート（CSV分析 + HW比較、A4縦）
- `/api/insight/report` - JSON API
- `/api/insight/report/xlsx` - Excel ダウンロード

## セキュリティ対策

- CSRF: Origin/Referer検証（外部オリジン→403）
- XSS: escape_html + escape_url_attr + sanitize_tag_text
- 20MB body size limit
- パスワード認証 + Argon2ハッシュ + レート制限

## セットアップ

### 必要なもの
- Rust 1.75+
- Python 3.11+ (E2Eテスト用)
- SQLite3

### ローカル起動
```bash
cargo build --release
./target/release/rust_dashboard
# http://localhost:3000
```

### 環境変数
- `DATABASE_URL` - SQLiteパス（デフォルト: `data/hellowork.db`）
- `TURSO_URL` / `TURSO_TOKEN` - 外部統計DB
- `SESSION_SECRET` - セッション暗号化キー（32文字以上）

## テスト

```bash
# ユニットテスト
cargo test

# E2E（Playwright）
pip install playwright openpyxl pypdf
playwright install chromium
python e2e_final_verification.py  # 最終確認

# 全E2E（約30分）
bash scripts/run_all_e2e.sh
```

## デプロイ

Renderの auto-deploy（mainブランチpushで自動反映）。手動deployはダッシュボードから。

## ドキュメント

- `docs/E2E_TEST_PLAN.md` - 機能/データ整合性テスト計画
- `docs/E2E_TEST_PLAN_V2.md` - Visual/UXテスト計画
- `docs/E2E_COVERAGE_MATRIX.md` - テストカバレッジマトリクス
- `docs/E2E_REGRESSION_GUIDE.md` - リグレッション運用ガイド
- `docs/SESSION_SUMMARY_2026-04-12.md` - 最新セッション総括

## ライセンス

Private / Internal use
