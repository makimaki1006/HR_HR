# E2E 最新実行結果

**実行日**: 2026-04-12
**対象**: https://hr-hw.onrender.com
**コミット**: 94c51f2 (3-team parallel: V2 data expansion + UX + residual FAIL fixes)
**実行者**: (記入してください)
**実行コマンド**: `bash scripts/run_all_e2e.sh`

---

## サマリー

| スイート | 合計 | PASS | FAIL | 備考 |
|---------|------|------|------|------|
| cargo test | 211 | 211 | 0 | - |
| e2e_security | 29 | 27 SECURE | 0 VULN + 2 INCONC | Render 50MB/100MB timeout |
| e2e_report_insight | 12 | 12 | 0 | - |
| e2e_report_survey | 21 | 20 | 1 | favicon 404 のみ |
| e2e_report_jobbox | (未記入) | - | - | 実行時に追記 |
| e2e_api_excel | 36 | 36 | 0 | - |
| e2e_other_tabs | 30 | 24 | 6 | ブラウザ並列時の競合 (単独実行では 34/35) |
| e2e_print_verify | 6 | 4 | 2 | 表紙テキスト検出仕様差 |

**合計**: 345 項目中 334 PASS (96.8%)

> INCONCLUSIVE (INCONC) は FAIL ではないが判定保留。リリース判定時に別途レビューすること。

---

## P0 項目の合格状況

- 認証: 100% ✅
- CSRF: 100% ✅
- レポート出力: 100% ✅
- XSS 防御: 100% ✅

---

## 既知の残件 / 残タスク

- [ ] 条件診断グレード表示の本番動作確認
- [ ] 50/100 MB 大容量アップロード時の明示拒否 (現状は Render timeout で INCONCLUSIVE)
- [ ] `e2e_print_verify.py` の表紙テキスト検出基準を実画面仕様に追従
- [ ] `e2e_other_tabs.py` の単独再実行による 6 FAIL の真因切り分け (並列競合 vs ロジック起因)
- [ ] favicon 設置 (低優先)

---

## 差分運用メモ

次回このファイルを更新する際は以下の手順で。

1. `bash scripts/run_all_e2e.sh` を実行。
2. `$TMPDIR/e2e_*.log` と `cargo_test.log` を確認して上表に数値を反映。
3. コミット: `2026-04-12 コミット <hash> の実行結果を反映` のような形で記録。
4. 回帰が発生した場合は必ず「残タスク」節に追記し、原因切り分けの手掛かりを残す。

---

## 参考ログ

- Rust: `/tmp/cargo_test.log`
- E2E: `/tmp/e2e_security.log`, `/tmp/e2e_report_survey.log`, ... (スクリプト名と対応)

詳細な運用手順は [`E2E_REGRESSION_GUIDE.md`](./E2E_REGRESSION_GUIDE.md) を参照。
