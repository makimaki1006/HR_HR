# 作業ツリー整理 事前監査 (read-only)

**日付**: 2026-05-09
**監査対象**: `C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy`
**HEAD**: `5ac1967 chore(pdf): clean up chart print polish follow-ups`
**origin/main**: `2959519` (Round 2.11)
**スコープ**: 分類のみ。削除/reset/stash/commit/push は全て未実施。

---

## 1. Summary

| 項目 | 値 |
|---|---|
| 未コミット modified | 17 ファイル (全て `src/handlers/...` および `tests/no_forbidden_terms.rs`) |
| 未追跡 ファイル/ディレクトリ合計 | 2,219 エントリ (`out/` 13 + `target-test/` 2,194 + その他 12) |
| origin/main..HEAD commit 数 | 1 (5ac1967) |
| push 待ち commit | 1 (5ac1967, ローカル先行) |
| 削除候補 (D 一時生成物) | 約 2,207 (target-test/ 2,194 + out/ 13) |
| commit 候補 (E 監査 docs + 再利用価値ある成果) | 4 (PDF*_2026_05_08.md 4 件) |
| 別タスク化候補 (C) | src/ 16 ファイルの実装変更 (PDF cleanup 範囲外) |

---

## 2. Modified Files (17 件)

| file | category | diff summary | likely owner | recommendation |
|---|---|---|---|---|
| src/handlers/analysis/fetch/market_intelligence.rs | C | +110 / -54 | Round 2.x analysis fetch 改修 | 別タスクで内容精査 → 単独 commit |
| src/handlers/analysis/fetch/mod.rs | C | +20 / -14 | 同上 | 同上 |
| src/handlers/company/fetch.rs | C | +18 / -10 | company fetch 修正 | 別タスク |
| src/handlers/insight/flow_context.rs | C | +4 / -2 | insight flow 微修正 | 別タスク |
| src/handlers/recruitment_diag/talent_pool_expansion.rs | C | +9 / -4 | 採用診断調整 | 別タスク |
| src/handlers/survey/handlers.rs | C | +7 / -4 | survey 入口 | 別タスク |
| src/handlers/survey/report_html/industry_mismatch.rs | C | +384 / -190 (最大) | レポート章 大幅改訂 | 別タスク (Round 2.x 系の未 commit 実装の可能性) |
| src/handlers/survey/report_html/invariant_tests.rs | C | +13 / -9 | 不変テスト調整 | 別タスク |
| src/handlers/survey/report_html/market_intelligence.rs | C | +211 / -159 | MI セクション改訂 | 別タスク |
| src/handlers/survey/report_html/market_tightness.rs | C | +47 / -36 | 4 軸レーダー周り | 別タスク |
| src/handlers/survey/report_html/mod.rs | C | +71 / -51 | variant ルーティング | 別タスク |
| src/handlers/survey/report_html/notes.rs | C | +5 / -4 (rustfmt 中心) | 注記章微修正 | 別タスク |
| src/handlers/survey/report_html/region_filter.rs | C | +3 / -2 | region filter 微修正 | 別タスク |
| src/handlers/survey/report_html/regional_compare.rs | C | +89 / -50 | 図 RC-1 周り | 別タスク |
| src/handlers/survey/report_html/salesnow.rs | C | +5 / -3 | SalesNow 微修正 | 別タスク |
| src/handlers/trend/fetch.rs | C | +3 / -2 | trend 微修正 | 別タスク |
| tests/no_forbidden_terms.rs | C (rustfmt のみ) | +7 / -7 (整形) | 自動整形 | 単独でも可、または上記 src と同じ別タスクに同梱 |

**判定根拠**: 直近 commit 5ac1967 は `e2e_print_verify.py` / `executive_summary.rs` / `helpers.rs` / `style.rs` / `pdf_helper.ts` のみ。modified 17 件はそれと別系統 (Round 2.x 系の MI / industry_mismatch / regional_compare 改修と、その付随 fetch 層変更) であり、PDF cleanup commit に同梱せず別タスク化するのが妥当。

---

## 3. Untracked Files (主要 25 件)

| file/dir | category | purpose inferred | recommendation |
|---|---|---|---|
| docs/PDF_CHART_BUILDER_PATH_AUDIT_2026_05_08.md | E | Round 2.8-B builder 経路監査 docs | 監査 docs として commit |
| docs/PDF_CHART_OPTION_RUNTIME_AUDIT_2026_05_08.md | E | Round 2.8 option runtime 監査 | commit |
| docs/PDF_CONTAINER_RENDER_AUDIT_2026_05_08.md | E | container render 監査 | commit |
| docs/PDF_DEPLOY_BUILD_AUDIT_2026_05_08.md | E | deploy build 監査 | commit |
| tests/e2e/_print_review_round2.spec.ts | F | Round 2 PDF 監査用一時 spec | 検証用 (再実行価値あり) → 残すなら commit (`_` プレフィクスは playwright 既定 glob 対象外。ファイル名 prefix `_` で自動実行から除外設計と推測) |
| tests/e2e/_print_review_round2_9.spec.ts | F | Round 2.9 検証 spec | 同上 |
| tests/e2e/_print_review_round2_11.spec.ts | F | Round 2.11-C 本番 PDF 検証 | 同上 |
| tests/e2e/_round2_10_dom_audit.spec.ts | F | DOM 監査 (read-only) | 同上 |
| tests/e2e/_round2_10_option_diff.spec.ts | F | print/screen option diff | 同上 |
| tests/e2e/_round2_10_selector_audit.spec.ts | F | selector 監査 | 同上 |
| _wait_render.ps1 | D | Render cold start 待機用 ad-hoc PS | プロジェクト直下に置きっぱなしは不適切。`scripts/` 移動 or 削除候補 |
| server.err | D | サイズ 0 の空ログ | 削除候補 |
| out/round2_pdf_review/mi_via_action_bar.pdf | D | Round 2 検証 PDF | 削除候補 (再生成可) |
| out/round2_pdf_review/analyze.py | D ? | ad-hoc 解析スクリプト | 内容次第。再利用するなら `scripts/` 移動 |
| out/round2_7_pdf_review/mi_via_action_bar.pdf | D | 検証 PDF | 削除候補 |
| out/round2_9_pdf_review/mi_via_action_bar.pdf | D | 検証 PDF | 削除候補 |
| out/round2_11_pdf_review/mi_via_action_bar.pdf | D | 検証 PDF | 削除候補 |
| out/round2_10_dom_audit/{dom_tree,meta}.json | D | DOM 監査出力 | 削除候補 |
| out/round2_10_option_diff/{prod_html_print,prod_html_screen}.html | D | option diff 出力 (HTML 抜粋) | 削除候補 |
| out/round2_10_option_diff/{runtime_print,runtime_screen}.json | D | option JSON | 削除候補 |
| out/round2_10_option_diff/ssr_data_chart_config.json | D | SSR 抽出 config | 削除候補 |
| out/round2_10_selector_audit/match.json | D | selector 監査出力 | 削除候補 |
| target-test/ (2,194 エントリ) | D | cargo target キャッシュ (検査用) | 削除候補。`.gitignore` 追加必須 |

---

## 4. Commit Candidate (推奨 commit セット)

### 4.1 監査 docs commit (低リスク, すぐ実行可能)
- docs/PDF_CHART_BUILDER_PATH_AUDIT_2026_05_08.md
- docs/PDF_CHART_OPTION_RUNTIME_AUDIT_2026_05_08.md
- docs/PDF_CONTAINER_RENDER_AUDIT_2026_05_08.md
- docs/PDF_DEPLOY_BUILD_AUDIT_2026_05_08.md

(既に同日付 docs 多数が commit 済みなので追従が自然)

### 4.2 別タスク化が望ましい (内容精査必要)
- src/ 16 ファイル + tests/no_forbidden_terms.rs … industry_mismatch.rs / market_intelligence.rs / regional_compare.rs の大幅改訂を含むため、Round 2.x の続きの実装と推測。コンパイル/テストが通る状態か別途検証してから複数の論理 commit に分けて push。

### 4.3 監査用 e2e spec (任意)
- `tests/e2e/_round2_10_*.spec.ts` 3 件 / `_print_review_round2*.spec.ts` 3 件
- 再現価値があるなら docs commit と一緒に残す。`_` prefix のため playwright 通常実行からは除外される設計の可能性大。

---

## 5. Ignore Candidate (`.gitignore` 追加候補)

```
out/
target-test/
server.err
_wait_render.ps1   # もしくは scripts/ 移動
```

`out/` は監査スクリプトが出力する review artefacts 置き場で、再生成可能。`target-test/` はテスト用 cargo キャッシュ。`.gitignore` の `target/` は `target-test/` をカバーしないため明示追加が必要。

---

## 6. Delete Candidate (本タスクでは削除しない)

| 対象 | 件数 | 理由 |
|---|---|---|
| target-test/ 全体 | 2,194 | cargo キャッシュ。再生成可能 |
| out/ 配下の PDF / JSON / HTML | 13 | E2E 検証出力。再実行で再生成 |
| server.err | 1 | 空ファイル |
| _wait_render.ps1 | 1 | 場所不適切 (移動 or 削除) |

合計 約 2,209 件。

---

## 7. Risk Notes (誤削除危険物)

| 件 | 内容 |
|---|---|
| 1 | docs/PDF_*_2026_05_08.md 4 件 — 削除すると監査ログ消失。必ず commit 推奨 |
| 2 | tests/e2e/_print_review_round2*.spec.ts / _round2_10_*.spec.ts 6 件 — 監査再現用。削除前に再利用要否を要確認 |
| 3 | src/ modified 17 件 — Round 2.x 続編の未 commit 実装の可能性大。`git checkout -- .` で消すと改修が失われる |
| 4 | out/round2_pdf_review/analyze.py — ad-hoc 解析スクリプトだが `scripts/` 配下に移すべきか確認要 |
| 5 | _wait_render.ps1 — Render cold start 待機ロジック。再利用するなら `scripts/wait_render.ps1` に移動推奨 |

合計 5 件。

---

## 8. Recommended Next Action

1. **docs 4 件を commit** (`docs/PDF_CHART_BUILDER_PATH_AUDIT_2026_05_08.md` 等) → 低リスクで監査ログを保全。
2. **src/ 17 ファイルの内容精査** — `cargo check` / 関連テストを走らせ、Round 2.x 系統の論理単位 (industry_mismatch / market_intelligence / regional_compare / analysis fetch 層) に分けて commit。
3. **`.gitignore` に `out/` `target-test/` `server.err` 追加** → そのうえで `out/` `target-test/` を削除。
4. **e2e 監査 spec 6 件** — 再利用価値を判断。残すなら `tests/e2e/_round2_*` のまま commit、不要なら削除。
5. **`_wait_render.ps1`** — `scripts/wait_render.ps1` に移動して commit、または削除。
6. **削除/reset/push は段階を分ける** — 1 → 3 → 4 → 5 → 2 の順で進めると安全。

---

(read-only 監査につき、削除・reset・stash・commit・push は本タスクで一切実施せず)
