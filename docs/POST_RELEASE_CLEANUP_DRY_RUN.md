# Post-Release Cleanup — Dry Run Report

実施日: 2026-05-06
作業ディレクトリ: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`
実施者: Task A エージェント (Claude Code)

## 概要

`git status --short` 上の未追跡 (untracked) ファイルを 4 カテゴリに分類し、
A1 (明らかな一時ファイル) のみを削除する。A2/A3 は次フェーズ判断のため保持、
A4 は本タスク対象外 (既存変更ファイル)。

## 分類表

### A1: 削除対象 (明らかな一時ファイル / E2E 成果物)

| パス | 種別 | 削除理由 |
|------|------|---------|
| `test-results/` | ディレクトリ | Playwright 実行成果物 (db-wal, db-shm 含む) |
| `playwright-report/` | ディレクトリ | Playwright HTML レポート |
| `build.log` | ログ | ビルド出力ログ |
| `server.log` | ログ | サーバ実行ログ (※ファイルロック中で削除スキップ。サーバ停止後に再実行) |
| `server.err` | (該当なし) | 直下に存在せず |
| `server.pid` | PID | 古いサーバプロセス記録 |
| `_e2e_run.log` | ログ | E2E ランナーログ |
| `e2e_run.log` | ログ | E2E ランナーログ |
| `e2e_run2.log` | ログ | E2E ランナーログ |
| `target-e2e-server-3000.err.log` | ログ | E2E サーバ stderr |
| `target-e2e-server-3000.out.log` | ログ | E2E サーバ stdout |
| `target-e2e-server-9316.err.log` | ログ | E2E サーバ stderr |
| `target-e2e-server-9316.out.log` | ログ | E2E サーバ stdout |
| `report_insight_print.pdf` | PDF | 印刷確認用 (履歴あり) |
| `report_survey_print.pdf` | PDF | 印刷確認用 |
| `_e2e_check_ports.ps1` | スクリプト | 一時 E2E スクリプト |
| `_e2e_compare_creds.ps1` | スクリプト | 一時 E2E スクリプト |
| `_e2e_kill_playwright.ps1` | スクリプト | 一時 E2E スクリプト |
| `_e2e_run_tests.ps1` | スクリプト | 一時 E2E スクリプト |
| `_e2e_start_server.ps1` | スクリプト | 一時 E2E スクリプト |
| `_e2e_server.pid` | PID | 一時 PID |
| `_print_review_t5.py` | スクリプト | 一時 print review スクリプト |

注: `*.db-wal` / `*.db-shm` は `test-results/data/` 配下に存在しディレクトリごと削除される。
`data/hellowork.db-wal` / `data/hellowork.db-shm` は **存在しない** ため対象外。

### A2: 次フェーズ候補 (残す)

- `data/agoop/` (CSV, parquet, manifest, normalized 一式)
- `docs/design_agoop_*.md`
- `docs/design_ssdse_*.md`
- `docs/requirements_agoop_jinryu.md`
- `docs/requirements_ssdse_a_expansion.md`
- `docs/maintenance_posting_mesh1km.md`
- `docs/turso_import_agoop.md`
- `docs/turso_import_ssdse_phase_a.md`
- `scripts/*agoop*` / `scripts/*ssdse*` / `scripts/build_posting_mesh1km.py` / `scripts/build_municipality_geocode.py`
- `scripts/extract_addresses_for_csis.py` / `scripts/geocode_with_csis_api.py` / `scripts/merge_csis_results.py` / `scripts/split_for_csis.py` / `scripts/update_db_from_csis.py`

理由: Agoop 人流 / SSDSE-A / posting_mesh1km / CSIS ジオコード次フェーズで Turso 投入予定のため温存。

### A3: 保持判断要 (残す)

| パス | 保持理由 |
|------|---------|
| `data/salesnow_companies.csv` | SalesNow 企業マスタ (再取得コスト高) |
| `data/salesnow_companies_14fields_backup.csv` | SalesNow バックアップ |
| `data/company_geocode.csv` | 企業ジオコード結果 |
| `data/csis_batches/` | CSIS 分割アップロード入力 |
| `data/csis_checkpoint.csv` | CSIS 進捗チェックポイント |
| `data/csis_geocoded.csv` | CSIS ジオコード結果 |
| `data/unique_addresses_for_csis.csv` | CSIS 入力マスタ |
| `data/hellowork.db.bak.before_jis_fetch` | 復元用バックアップ DB |
| `data/snapshot_metadata.json` | スナップショット情報 |
| `data/ts_parquet_cache/` | 時系列 parquet キャッシュ (再生成コスト高) |
| `s008.xlsx` | 外部統計入力 |
| `s009.xlsx` | 外部統計入力 |

### A4: 既存変更 (本タスク対象外)

`M docs/CLAUDE.md` / `M scripts/import_ssdse_to_db.py` / `M scripts/industry_mapping.py` /
`M scripts/upload_new_external_to_turso.py` — Orchestrator が後で別 commit。

### 対象外 (ビルド成果物 / cache)

- `target/` (Cargo build cache, 仕様により残す)
- `target-e2e/` (E2E 用 Cargo cache, 仕様により残す)
- `node_modules/` (npm cache)
- `.claude/plans/` (Claude セッション計画)

## 削除コマンド

A1 のみ実行。push / commit は本タスクでは行わない。

## 検証ポリシー

1. 削除前に `git ls-files --error-unmatch <path>` で **未追跡** であることを確認
2. 削除前後で `git status --short` の untracked 件数を比較
3. `data/hellowork.db` / `data/agoop/` / `data/salesnow_*` には触れない
