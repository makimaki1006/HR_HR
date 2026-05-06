# Round 4 Worktree Cleanup Audit

作業日: 2026-05-06
対象worktree: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`
Worker A 担当範囲: 分類のみ (read-only、削除しない)

## サマリー

- 未追跡ファイル/ディレクトリ: 約80件
- 巨大ファイル/ディレクトリ (>100MB): 5件、合計 約 6.4 GB
  - `data/agoop/` 2.7GB
  - `data/hellowork.db.bak.before_jis_fetch` 1.6GB
  - `data/salesnow_companies.csv` 470MB
  - `data/ts_parquet_cache/` 316MB
  - `data/salesnow_companies_14fields_backup.csv` 58MB
- 変更ファイル (M): 4件 (`docs/CLAUDE.md`, `scripts/import_ssdse_to_db.py`, `scripts/industry_mapping.py`, `scripts/upload_new_external_to_turso.py`)

## 分類表

### A1: Round 4 関連残骸 (一時実験/レビュー用スクリプト)

| パス | サイズ | 判定根拠 |
|---|---|---|
| `_print_review_t5.py` | 12K | 印刷レビュー時の使い捨てスクリプト (アンダースコア接頭辞は一時規約) |
| `e2e_8fixes_verify.py` | 12K | 8件修正の事後検証スクリプト |
| `e2e_chart_json_verify.py` | 8K | チャート JSON 検証用 (Round 4 議論で発生) |
| `e2e_final_verification_result.json` | 8K | 検証結果 JSON (再生成可能) |
| `e2e_security_result.json` | 8K | セキュリティ E2E 結果 (再生成可能) |
| `sys_verify.py` | 12K | セッション検証ワンショット |
| `verify_dashboard.py` | 20K | ダッシュボード検証ワンショット |
| `test_map_company_layer.py` | 16K | ルート直下に置かれた単発テスト (`scripts/` または `tests/` 配下が適切) |
| `report_insight_print.pdf` | 868K | 印刷出力サンプル |
| `report_survey_mixed.pdf` | 444K | 印刷出力サンプル |
| `report_survey_print.pdf` | 12K | 印刷出力サンプル |
| `mockup_labor_flow.html` | 16K | UIモックアップ (ドラフト) |

### A2: 次フェーズ候補 (agoop / SSDSE-A 拡張)

| パス | サイズ | 判定根拠 |
|---|---|---|
| `scripts/build_municipality_geocode.py` | - | agoop連携用住所ジオコード |
| `scripts/build_posting_mesh1km.py` | - | posting_mesh1km 月次更新 (CLAUDE.md 記載のフロー) |
| `scripts/export_agoop_to_turso_csv.py` | - | agoop Turso 投入 |
| `scripts/extract_addresses_for_csis.py` | - | CSIS 住所抽出 |
| `scripts/fetch_agoop_flow.py` | - | agoop 取得 |
| `scripts/geocode_with_csis_api.py` | - | CSIS API ジオコード |
| `scripts/import_agoop_to_sqlite.py` | - | agoop SQLite 投入 |
| `scripts/merge_csis_results.py` | - | CSIS 結果マージ |
| `scripts/normalize_agoop.py` | - | agoop 正規化 |
| `scripts/run_agoop_turso_upload.ps1` | - | agoop Turso アップロード |
| `scripts/split_for_csis.py` | - | CSIS 分割 |
| `scripts/update_db_direct.py` | - | DB 直接更新 |
| `scripts/update_db_from_csis.py` | - | CSIS 結果反映 |
| `scripts/update_external_data.py` | - | 外部データ更新 |
| `scripts/upload_agoop_to_turso.py` | - | agoop Turso 投入 |
| `scripts/verify_phase3_jis_fetch_result.py` | - | Phase3 JIS 検証 |
| `scripts/test_agoop_phase_2_5.py` | - | agoop Phase 2.5 テスト |
| `scripts/test_trend_e2e_extended_prod.py` | - | トレンド E2E (本番拡張) |
| `scripts/test_trend_e2e_prod.py` | - | トレンド E2E (本番) |
| `scripts/data/` (18ファイル) | - | 外部統計CSV/XLSX (boj_tankan, census 等) |
| `templates/tabs/region_karte.html` | - | 地域カルテUI |
| `static/js/region_karte.js` | - | 地域カルテJS |
| `data/agoop/turso_csv/` | 1.3GB | agoop Turso 投入用 (再生成可) |
| `data/agoop/normalized/` | 1.3GB | agoop 正規化済 (再生成可) |
| `data/agoop/raw/` | 215MB | agoop 生データ (再取得可、重い) |
| `data/agoop/posting_mesh1km_20260401.csv` | 2.4MB | 月次スナップショット |

### A3: 一時ファイル・ログ・スクショ・DB sidecar

| パス | サイズ | 判定根拠 |
|---|---|---|
| `data/hellowork.db-shm` | 32K | SQLite WAL sidecar |
| `data/hellowork.db-wal` | 0 | SQLite WAL sidecar |
| `data/agoop/build.log` | - | パイプラインログ |
| `data/agoop/export.log` | - | 同上 |
| `data/agoop/import.log` | - | 同上 |
| `data/agoop/import2.log` | - | 同上 |
| `data/agoop/import3.log` | - | 同上 |
| `data/agoop/normalize.log` | - | 同上 |
| `data/agoop/normalize2.log` | - | 同上 |
| `data/agoop/logs/` | 164K | ログディレクトリ |
| `data/agoop/import_manifest.json` | - | インポート履歴 |
| `data/csis_batches/` | 14MB | CSIS バッチ作業中間ファイル |
| `data/csis_checkpoint.csv` | 18MB | CSIS チェックポイント |
| `data/csis_geocoded.csv` | 18MB | CSIS結果中間 |
| `data/unique_addresses_for_csis.csv` | 14MB | CSIS入力中間 |
| `data/ts_parquet_cache/` | 316MB | Parquet キャッシュ (再生成可) |
| `docs/screenshots/2026-04-30/print/` | 不明(多数PNG/PDF) | 印刷検証スクショ |
| `s008.xlsx` | 28K | ルート直下のスプレッドシート (出所不明) |
| `s009.xlsx` | 28K | 同上 |

### A4: repo に残すべき設定/ドキュメント候補

| パス | 判定根拠 |
|---|---|
| `docs/E2E_TEST_PLAN.md` | E2E 計画 (永続) |
| `docs/E2E_TEST_PLAN_V2.md` | E2E 計画 V2 (永続) |
| `docs/design_agoop_backend.md` | 設計書 |
| `docs/design_agoop_frontend.md` | 設計書 |
| `docs/design_agoop_jinryu.md` | 設計書 |
| `docs/design_ssdse_a_backend.md` | 設計書 |
| `docs/design_ssdse_a_expansion.md` | 設計書 |
| `docs/design_ssdse_a_frontend.md` | 設計書 |
| `docs/maintenance_posting_mesh1km.md` | 運用手順 |
| `docs/qa_integration_round_1_3.md` | QA結果 |
| `docs/requirements_agoop_jinryu.md` | 要件 |
| `docs/requirements_ssdse_a_expansion.md` | 要件 |
| `docs/turso_import_agoop.md` | Turso 投入手順 |
| `docs/turso_import_ssdse_phase_a.md` | Turso 投入手順 |
| `docs/turso_v2_sync_report_2026-05-04.md` | 同期レポート |
| `scripts/CLAUDE.md` | scripts 用 CLAUDE 指示 |
| `scripts/DATA_UPDATE_GUIDE.md` | データ更新ガイド |
| `data/CLAUDE.md` | data 用 CLAUDE 指示 |
| `static/CLAUDE.md` | static 用 CLAUDE 指示 |
| `.claude/plans/CLAUDE.md` | プラン用 CLAUDE 指示 |

### A5: 危険/要確認 (機微の可能性)

| パス | サイズ | 判定根拠 |
|---|---|---|
| `data/salesnow_companies.csv` | 470MB | SalesNow 198K社データ。**Turso 限定。git に絶対入れない** |
| `data/salesnow_companies_14fields_backup.csv` | 58MB | SalesNow バックアップ。**git に絶対入れない** |
| `data/snapshot_metadata.json` | 8K | スナップショット metadata。中身要確認 (token/path 含む可能性) |
| `data/hellowork.db.bak.before_jis_fetch` | 1.6GB | DB バックアップ。**git に絶対入れない**。ローカル保管のみ |
| `data/company_geocode.csv` | 14MB | 企業ジオコード。機微判定要 |

## 削除候補 (確実なもの — Worker B が rm 検討)

| パス | 理由 |
|---|---|
| `_print_review_t5.py` | 一時実験スクリプト |
| `e2e_8fixes_verify.py` | 一時検証スクリプト |
| `e2e_chart_json_verify.py` | 一時検証スクリプト |
| `e2e_final_verification_result.json` | 再生成可能 |
| `e2e_security_result.json` | 再生成可能 |
| `sys_verify.py` | 一時検証 |
| `verify_dashboard.py` | 一時検証 |
| `data/agoop/*.log` (build/export/import/import2/import3/normalize/normalize2) | ログ |
| `data/hellowork.db-shm`, `data/hellowork.db-wal` | SQLite sidecar (DB 終了で消える) |
| `report_*.pdf` 3件 | 検証出力 |
| `s008.xlsx`, `s009.xlsx` | 出所不明、ルート直下不適切 |

## .gitignore 追加候補 (Worker B 用)

```gitignore
# A3/A5: 巨大データ・機微データ (絶対に commit しない)
data/agoop/raw/
data/agoop/normalized/
data/agoop/turso_csv/
data/agoop/logs/
data/agoop/*.log
data/agoop/import_manifest.json
data/csis_batches/
data/csis_checkpoint.csv
data/csis_geocoded.csv
data/unique_addresses_for_csis.csv
data/ts_parquet_cache/
data/salesnow_companies*.csv
data/company_geocode.csv
data/hellowork.db.bak*
data/hellowork.db-shm
data/hellowork.db-wal
data/snapshot_metadata.json

# A1: 一時検証スクリプト・出力
/_*.py
/sys_verify.py
/verify_dashboard.py
/e2e_*_verify.py
/e2e_*_result.json
/test_map_company_layer.py
/report_*.pdf
/mockup_*.html
/s00*.xlsx

# 印刷検証スクショ (大量PNG)
docs/screenshots/
```

## commit すべき候補 (Worker B / ユーザー判断)

### 設計・運用ドキュメント (A4)
- `docs/E2E_TEST_PLAN.md`, `docs/E2E_TEST_PLAN_V2.md`
- `docs/design_agoop_*.md` (3件)
- `docs/design_ssdse_a_*.md` (3件)
- `docs/maintenance_posting_mesh1km.md`
- `docs/qa_integration_round_1_3.md`
- `docs/requirements_agoop_jinryu.md`, `docs/requirements_ssdse_a_expansion.md`
- `docs/turso_import_agoop.md`, `docs/turso_import_ssdse_phase_a.md`
- `docs/turso_v2_sync_report_2026-05-04.md`
- `scripts/CLAUDE.md`, `scripts/DATA_UPDATE_GUIDE.md`
- `data/CLAUDE.md`, `static/CLAUDE.md`, `.claude/plans/CLAUDE.md`

### スクリプト (A2、運用パイプライン)
- `scripts/build_municipality_geocode.py`
- `scripts/build_posting_mesh1km.py`
- `scripts/export_agoop_to_turso_csv.py`
- `scripts/extract_addresses_for_csis.py`
- `scripts/fetch_agoop_flow.py`
- `scripts/geocode_with_csis_api.py`
- `scripts/import_agoop_to_sqlite.py`
- `scripts/merge_csis_results.py`
- `scripts/normalize_agoop.py`
- `scripts/run_agoop_turso_upload.ps1`
- `scripts/split_for_csis.py`
- `scripts/update_db_direct.py`
- `scripts/update_db_from_csis.py`
- `scripts/update_external_data.py`
- `scripts/upload_agoop_to_turso.py`
- `scripts/verify_phase3_jis_fetch_result.py`
- `scripts/test_agoop_phase_2_5.py`, `scripts/test_trend_e2e_*.py` (`tests/` 配下移動が望ましい)

### UI (A2)
- `templates/tabs/region_karte.html`
- `static/js/region_karte.js`

### 既存変更 (M)
- `docs/CLAUDE.md`
- `scripts/import_ssdse_to_db.py`
- `scripts/industry_mapping.py`
- `scripts/upload_new_external_to_turso.py`

## ユーザー確認が必要な候補

| パス | 確認事項 |
|---|---|
| `data/hellowork.db.bak.before_jis_fetch` (1.6GB) | 削除可? 別ストレージへ退避済み? |
| `data/salesnow_companies.csv` (470MB) | Turso 投入済みなら削除可? |
| `data/salesnow_companies_14fields_backup.csv` (58MB) | バックアップ保存先は別 (Turso/GitHub Release) でよい? |
| `data/snapshot_metadata.json` | 中身に token/credential 含まれていないか? (Worker A は中身を開いていない) |
| `data/company_geocode.csv` (14MB) | 個人情報なし? Turso 投入済みなら削除可? |
| `scripts/data/` (18ファイル、census/boj_tankan 等) | git 管理対象でよいか? それとも `data/external/` へ移動? |
| `data/agoop/raw/` (215MB) | 再取得可能? 一旦削除して .gitignore? |
| `data/ts_parquet_cache/` (316MB) | キャッシュ削除して再生成スクリプト整備? |
| `_print_review_t5.py` および `e2e_*_verify.py` | 削除前に内容確認 (検証ロジック流用予定の有無) |
| `s008.xlsx`, `s009.xlsx` | 出所と用途確認 |
| `mockup_labor_flow.html` | 設計参考に保管 vs 削除 |
| `report_*.pdf` 3件 | 印刷検証アーティファクト保管要否 |

## 完了報告

- 編集ファイル: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/ROUND4_WORKTREE_CLEANUP_AUDIT.md`
- 操作: read-only (`git status --short`、`du -sh`、`ls`)。削除/編集は本監査ファイルのみ。
- 機微ファイル中身は開いていない (.env/token/password は表示せず)。
