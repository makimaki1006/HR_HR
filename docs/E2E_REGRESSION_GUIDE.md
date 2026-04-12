# E2E リグレッションテスト運用ガイド

本ドキュメントは `hellowork-deploy` リポジトリにおける E2E リグレッションテストの運用手順・設計思想・CI/CD 統合案をまとめたものです。

- 対象環境: 本番 `https://hr-hw.onrender.com`
- 実行ホスト: Windows 11 + Git Bash (MSYS2) を前提 (Linux/macOS でも動作)
- 主要スタック: Rust (`cargo test`) + Python (Playwright, requests)

---

## 1. 各 E2E スクリプトの役割・実行時間目安

| スクリプト | 役割 | 実行時間目安 | 依存 |
|-----------|------|------------|------|
| `cargo test` | Rust ユニット/統合テスト (211 件想定) | 1-2 分 | ローカルビルド環境 |
| `e2e_security.py` | XSS / CSRF / SQLi / 文字コード / 大容量アップロード | 3-5 分 | 本番デプロイ |
| `e2e_report_survey.py` | アンケート CSV アップロード → レポート生成 | 2 分 | 本番デプロイ, `_survey_mock.csv` |
| `e2e_report_jobbox.py` | ジョブボックス CSV → レポート生成 | 2 分 | 本番デプロイ, `_jobbox_mock.csv` |
| `e2e_report_insight.py` | insight 22 パターンの生成・描画検証 | 2-3 分 | 本番デプロイ |
| `e2e_other_tabs.py` | 企業 / 地図 / トレンド / 分析 等その他タブ回遊 | 3-4 分 | 本番デプロイ |
| `e2e_api_excel.py` | Excel/API エンドポイント 36 項目 | 1-2 分 | 本番デプロイ |
| `e2e_print_verify.py` | 印刷レイアウト (表紙・ページ分割) 検証 | 1 分 | 本番デプロイ |

補助スクリプト (単発調査用・スイートには含めない):
- `e2e_post_deploy.py` — デプロイ直後スモーク
- `e2e_chart_json_verify.py` — ECharts JSON 構造検証
- `e2e_data_quality.py` — データ妥当性スナップショット
- `e2e_real_verify.py` / `e2e_visual_verify.py` — 画面キャプチャ比較
- `e2e_8fixes_verify.py` — 特定修正群のピンポイント検証

合計実行時間の目安: **約 15-20 分** (ブラウザ起動/ネットワーク待ち含む)。

---

## 2. 単独実行 vs 全実行

### 2-1. 開発中 (単独実行)

特定機能を修正した直後の確認用。ブラウザウィンドウも 1 本だけなので高速。

```bash
# 例: アンケートレポート部分だけ流す
python e2e_report_survey.py

# 例: セキュリティのみ
python e2e_security.py
```

### 2-2. デプロイ後 (全実行)

Render へのデプロイが完了した後、リグレッションを網羅的に検証。

```bash
bash scripts/run_all_e2e.sh
```

- ログは `$TMPDIR` または `/tmp` に `cargo_test.log` / `e2e_*.log` として保存される。
- 終了コードが 0 でも、`Summary` 行の `FAIL/VULN lines` 合計を必ず目視確認すること (Python 側が `sys.exit(0)` を返すケースがある)。

### 2-3. 実行前チェックリスト

1. Render のデプロイが完了していることを確認 (ダッシュボードで `Live` 表示)。
2. `hr-hw.onrender.com/healthz` 等に一度ブラウザでアクセスし、コールドスタート (初回 30-60 秒) を解消させる。
3. Playwright のブラウザが最新: `playwright install chromium`。
4. `.env` 等に本番認証情報 (テストユーザー) が設定済み。

---

## 3. CI/CD 統合案 (GitHub Actions) — 提案のみ

本セッションでは実装しない。後日導入する場合の指針として残す。

### 3-1. ワークフロー: `.github/workflows/deploy-verify.yml`

```yaml
# ※ テンプレート / 未適用。実装時は secrets と Render webhook の設定が別途必要。
name: Deploy Verify (E2E Regression)

on:
  # Render のデプロイ完了 webhook を GitHub repository_dispatch で受ける想定
  repository_dispatch:
    types: [render-deploy-succeeded]
  workflow_dispatch:   # 手動実行も許可

jobs:
  e2e:
    runs-on: ubuntu-latest
    timeout-minutes: 45
    steps:
      - uses: actions/checkout@v4

      - name: Setup Rust
        uses: dtolnay/rust-toolchain@stable

      - name: Setup Python
        uses: actions/setup-python@v5
        with:
          python-version: "3.11"

      - name: Install Python deps
        run: |
          pip install playwright requests pandas openpyxl
          playwright install --with-deps chromium

      - name: Run E2E suite
        env:
          HR_HW_BASE_URL: https://hr-hw.onrender.com
          HR_HW_TEST_USER: ${{ secrets.HR_HW_TEST_USER }}
          HR_HW_TEST_PASS: ${{ secrets.HR_HW_TEST_PASS }}
        run: bash scripts/run_all_e2e.sh

      - name: Upload logs
        if: always()
        uses: actions/upload-artifact@v4
        with:
          name: e2e-logs
          path: /tmp/e2e_*.log

      - name: Notify Slack on failure
        if: failure()
        # TODO: slackapi/slack-github-action などで実装
        run: echo "Slack notification stub — implement when Slack webhook secret is ready"
```

### 3-2. 導入時に決めるべき事項

- Render デプロイ完了通知 → GitHub への連携方法 (Deploy Hook → `repository_dispatch`)。
- Slack 通知の粒度 (FAIL のみ / SUMMARY 全量 / 担当者メンション)。
- 週次スケジュール実行 (`schedule: cron`) を併用するか。
- `secrets` に入れる本番テストユーザーの権限範囲 (読み取り専用アカウント推奨)。

---

## 4. ブラウザ並列実行の制限 (重要)

**4 本以上の Playwright スクリプトを同時に動かすと Chromium のリソース競合で FAIL する**、という挙動を本セッションで実証した。

- 症状: `e2e_other_tabs.py` が単独実行では 34/35 PASS なのに、並列実行時には 24/30 に劣化。
- 原因: Chromium プロセス間の GPU/メモリ/ソケット競合、Render 側のレートリミットも加勢する。
- 対策: `run_all_e2e.sh` は **必ず順次 (sequential) 実行**。並列化しない。

どうしても時間短縮したい場合は、CI の `matrix` で **別ランナーに分散** させること (同一マシン上での並列はしない)。

---

## 5. トラブルシューティング

### 5-1. Render デプロイ遅延

- Render Free プランはコールドスタートが 30-60 秒。最初の HTTP リクエストで 502 が返ることがある。
- 対応: `run_all_e2e.sh` 実行前にブラウザで 1 回アクセスしてウォームアップする、もしくは各 E2E スクリプト冒頭で `/healthz` を数回リトライする。

### 5-2. セッションタイムアウト (30 分)

- アプリケーションのセッションは **30 分** で失効。
- E2E スイート全体で 15-20 分かかるため、1 スイート内で再ログインが挟まる可能性は低いが、`e2e_security.py` のような長尺スクリプトでは冒頭でログインし直す設計になっている。
- タイムアウト起因の FAIL が出た場合は該当スクリプトを **単独再実行** して再現性を確認すること。

### 5-3. `cache_entries` の扱い

- 一部の分析タブは `cache_entries` (サーバー側キャッシュ) を参照する。初回アクセス時はキャッシュ構築で数秒〜十数秒待機が発生。
- E2E スクリプトでは `wait_for_selector` + タイムアウト 30 秒で待機する設計。不足する場合は 60 秒に延長してよい。
- キャッシュが壊れている疑いがある場合は Render の再デプロイ (clean build) で解消する。

### 5-4. よくある誤検知

| 現象 | 原因 | 対応 |
|------|------|------|
| `favicon 404` が FAIL にカウント | favicon 未設置 | 既知、優先度低 |
| 大容量アップロードで Render timeout | Render 側 100 MB 制限 | INCONCLUSIVE 扱い。アプリで明示拒否する改修を別途検討 |
| `e2e_other_tabs.py` の断続 FAIL | 並列実行時のブラウザ競合 | `run_all_e2e.sh` は順次実行。単独再実行で確認 |
| 表紙テキスト未検出 | 印刷レイアウトの文言差分 | `e2e_print_verify.py` の期待文字列を最新に追従 |

---

## 6. 参考: 本ガイドに関連するファイル

- 実行スクリプト: `scripts/run_all_e2e.sh`
- 最新結果: `docs/E2E_RESULTS_LATEST.md`
- 旧テスト計画: `docs/E2E_TEST_PLAN.md`, `docs/E2E_TEST_PLAN_V2.md`
- E2E 本体: リポジトリ直下の `e2e_*.py`
