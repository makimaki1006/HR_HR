# Fix-C 実行結果: vacancy_rate Newtype 化 + survey_deepdive E2E 修復

実行日: 2026-04-26
担当: Fix-C
対象 audit: deepdive_d2_survey_report.md (Q1.3) / deepdive_d3_survey_e2e.md

---

## 1. 概要

| 項目 | 結果 |
|------|------|
| vacancy_rate Newtype 導入 | 完了（段階的、HwAreaEnrichment.vacancy_rate_pct のみ） |
| 単体テスト | 9 件追加、全 pass |
| ライブラリ全テスト | 710 → **719 pass**（+9, 既存破壊 0、1 ignored は既存） |
| ビルド | pass（warning 2 件は既存 dead_code、Fix-C 影響なし） |
| survey_deepdive spec 修正 | 完了（13 test 構文 OK） |
| 本番実機 npx playwright test | サンドボックス制約により親セッション要実行 |

---

## 2. vacancy_rate Newtype 化

### 2.1 設計判断

監査レポート Q1.3 が指摘した「`vacancy_rate` のスケール混乱（0-1 vs 0-100% 混在）」に対する **段階的・最小侵襲アプローチ**を採用：

- **やったこと**: 公開境界に当たる 1 か所（`HwAreaEnrichment.vacancy_rate_pct`）を `Option<f64>` → `Option<VacancyRatePct>` に置換。代入経路（survey/integration.rs と survey/report_html/hw_enrichment.rs）に `VacancyRatePct::from_ratio()` を経由させた。
- **やらなかったこと**: insight engine 系（engine.rs, report.rs, render.rs）、recruitment_diag、analysis サブタブ等の **DB 由来 f64 を内部で直接使う関数**は触らない。これらは `vacancy_rate (0-1)` 一貫で、`* 100.0` も表示直前の format! のみで完結している。横断置換は影響範囲が広くリスクに見合わない。
- **将来的な拡張**: Phase 2 として、insight engine の表示境界も `VacancyRatePct` 化することは可能。本 PR では型基盤を整備するに留める。

### 2.2 追加・変更ファイル

| ファイル | 変更内容 |
|---------|---------|
| `src/handlers/types.rs` | **新規**。`VacancyRatePct(f64)` / `VacancyRateRatio(f64)` Newtype + 相互変換 + 9 単体テスト |
| `src/handlers/mod.rs` | `pub mod types;` 追加 |
| `src/handlers/survey/hw_enrichment.rs` | `HwAreaEnrichment.vacancy_rate_pct` を `Option<VacancyRatePct>` に変更 + コメント追記 |
| `src/handlers/survey/report_html/hw_enrichment.rs` | `fallback_vacancy` を `VacancyRatePct::from_ratio()` 経由で生成、format 部を `vrate.as_f64()` に変更 |
| `src/handlers/survey/integration.rs` | `vacancy_hint` の型を `Option<VacancyRatePct>` 化、`* 100.0` を `VacancyRatePct::from_ratio()` に置換、表示の match arm を `vp.as_f64()` 経由に変更 |

### 2.3 単体テスト（9 件、全 pass）

`src/handlers/types.rs` 末尾の `mod tests`：

| テスト名 | 検証内容 |
|---------|---------|
| `ratio_to_pct_normal` | 0.12 → 12.0 |
| `pct_to_ratio_normal` | 35.0 → 0.35 |
| `round_trip_ratio_pct_ratio` | ratio → pct → ratio で値不変 |
| `from_ratio_constructor` | `VacancyRatePct::from_ratio(0.085)` → 8.5% |
| `pct_range_check` | 0/50/100 OK、-1/101/NaN は範囲外 |
| `ratio_range_check` | 0/0.5/1.0 OK、-0.01/1.01 は範囲外 |
| `format_pct_known_values` | "12.0%" / "35.0%" / "8.5%" |
| `reverse_proof_ratio_misuse_visible` | 0.12 を pct として誤代入 → "0.1%" 表示で目視発見可能 |
| `serde_transparent` | f64 と同じ JSON 表現（12.5 ↔ "12.5"） |

```text
test handlers::types::tests::format_pct_known_values ... ok
test handlers::types::tests::from_ratio_constructor ... ok
test handlers::types::tests::pct_range_check ... ok
test handlers::types::tests::pct_to_ratio_normal ... ok
test handlers::types::tests::ratio_range_check ... ok
test handlers::types::tests::ratio_to_pct_normal ... ok
test handlers::types::tests::reverse_proof_ratio_misuse_visible ... ok
test handlers::types::tests::round_trip_ratio_pct_ratio ... ok
test handlers::types::tests::serde_transparent ... ok
```

### 2.4 cargo test 全体

```text
test result: ok. 719 passed; 0 failed; 1 ignored; 0 measured; 0 filtered out; finished in 1.73s
```

(既存 710 → 719、+9 は本 PR の types テスト。1 ignored は本 PR 以前から存在)

---

## 3. survey_deepdive_2026_04_26.spec.ts 修正

### 3.1 修正前の問題

監査 D-3 で報告された「13/13 fail (60s timeout)」の原因：

1. **`page.waitForLoadState('networkidle')` 依存**: Render 本番では analytics や HTMX 通信が継続的に発生し networkidle に到達しない
2. **fixture 存在確認なし**: ENOENT エラー時の根本原因発見が遅れる
3. **同タブ再クリック対応なし**: clickNavTab で active タブを再クリックすると HTMX no-op で waitForFunction が timeout
4. **Render cold start 未考慮**: 初回レスポンス 60+ 秒に対して `timeout: 90_000`（playwright.config）が短い

### 3.2 修正方針

regression_2026_04_26.spec.ts と同等のパターンを採用しつつ、Render cold start + アップロード処理時間を吸収。

| 修正点 | 内容 |
|-------|------|
| **`test.setTimeout(240_000)`** | describe 直下で全 test に 4 分の上限 |
| **`login()` リファクタ** | `waitForURL((url) => !/login/.test(url))` に変更し networkidle 依存撤廃。タイムアウト 60s → 90s |
| **`clickNavTab` 強化** | active 状態の再クリックを skip（feedback_htmx_same_tab_reclick.md 準拠）、内部 timeout を 30s/60s に拡張 |
| **`uploadCsv` 強化** | `waitForResponse('/api/survey/upload')` を併設、HTTP status 検証、待機 60s |
| **`beforeAll` で fixture 検証** | `fs.existsSync` で indeed/jobbox 両 CSV を assert |
| **タイムアウト全般** | `setInputFiles` 後の innerHTML 待機 60s、印刷レポート開封 60s、ECharts 描画後 1.5s 確保 |

### 3.3 修正前後 pass/fail 比較

| Test ID | 修正前 (audit D-3) | 修正後 (構文) | 修正後 (本番実機) |
|---------|-------------------|--------------|--------------------|
| S-1 ～ S-13 | 13/13 fail (60s timeout) | 13/13 list 成功 | **親セッション要実行** |

`npx playwright test ... --list` の結果（一部抜粋）:

```text
Listing tests:
  [chromium] › survey_deepdive_2026_04_26.spec.ts:225:7 › ... › S-1: ...
  ...
  [chromium] › survey_deepdive_2026_04_26.spec.ts:525:7 › ... › S-13: ...
Total: 13 tests in 1 file
```

### 3.4 サンドボックス制約

Fix-C 環境では `npx playwright test` の本番実機実行に必要な外部 HTTPS アクセスがサンドボックスで制限されているため、**親セッション側で以下のコマンドを実行して 13/13 pass を確認する必要がある**：

```bash
cd C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy
BASE_URL=https://hr-hw.onrender.com \
E2E_EMAIL=s_fujimaki@f-a-c.co.jp \
E2E_PASS=cyxen_2025 \
npx playwright test tests/e2e/survey_deepdive_2026_04_26.spec.ts --reporter=list
```

期待結果: **13 passed**

注意:
- 初回実行時は Render cold start で最初の test が 60 秒以上かかる可能性。`test.setTimeout(240_000)` でカバー済み
- workers=1 (playwright.config) のため逐次実行、所要時間は **15-25 分** を想定
- 失敗した場合のスクリーンショット・トレースは `playwright-report/` と `test-results/` 配下に保存される

---

## 4. 親セッションへの統合チェックリスト

### 4.1 Fix-A / Fix-B との競合確認

- [ ] **Fix-B との競合**: `src/handlers/survey/hw_enrichment.rs` の `HwAreaEnrichment` struct 定義は **Fix-C が変更**（`vacancy_rate_pct: Option<VacancyRatePct>`）。Fix-B は同ファイルの sanity_check（`POSTING_CHANGE_SANITY_LIMIT` 等）を追記済（linter 経由で観測）。両者は別関数・別フィールドのため衝突なし。
- [ ] **Fix-A との競合**: aggregator.rs / salary_parser.rs / upload.rs に Fix-C は触れていない。

### 4.2 マージ前検証

- [ ] `cargo build --lib` が成功すること（現状: ✅ pass）
- [ ] `cargo test --lib` で 719 件全 pass を確認（現状: ✅ pass）
- [ ] `npx playwright test tests/e2e/regression_2026_04_26.spec.ts` が引き続き 13/13 pass であること（Fix-C は regression spec を変更していないが、念のため）
- [ ] `npx playwright test tests/e2e/survey_deepdive_2026_04_26.spec.ts` を本番実機で実行し 13/13 pass を確認

### 4.3 Newtype 移行後の追加検討項目（任意・将来）

- [ ] insight engine（engine.rs L133, L302）の `vacancy_rate * 100.0` を `VacancyRatePct::from_ratio(vacancy_rate).format_pct()` に置換（Phase 2）
- [ ] insight report（report.rs L121, L278）の `vr_pct = vacancy_rate * 100.0` を Newtype 化（Phase 2）
- [ ] DB ETL 段階で `v2_vacancy_rate.vacancy_rate` カラムに CHECK(0 <= vacancy_rate AND vacancy_rate <= 1) 制約を追加（Q1.3 真因対策）

---

## 5. 変更ファイル一覧（絶対パス）

```
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/types.rs                          (新規)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/mod.rs                            (1 行追加)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/hw_enrichment.rs           (struct + コメント)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/integration.rs             (vacancy_hint + match arm)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/src/handlers/survey/report_html/hw_enrichment.rs (fallback_vacancy + format)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/tests/e2e/survey_deepdive_2026_04_26.spec.ts   (rewrite)
C:/Users/fuji1/AppData/Local/Temp/hellowork-deploy/docs/audit_2026_04_24/exec_fixC_results.md     (新規=本書)
```

---

## 6. memory feedback ルール準拠状況

| Rule | 準拠 |
|------|------|
| feedback_test_data_validation.md | ✅ types テストは具体値（0.12 → 12.0 等）で逆証明含む |
| feedback_reverse_proof_tests.md | ✅ `reverse_proof_ratio_misuse_visible` で誤代入時の挙動も明示テスト |
| feedback_render_cold_start_timeout.md | ✅ `test.setTimeout(240_000)` + 個別 timeout 拡張 |
| feedback_htmx_same_tab_reclick.md | ✅ `clickNavTab` で active 再クリック検知して skip |
| feedback_e2e_chart_verification.md | ✅ S-7 で canvas/svg 描画要素数 ≥ 1 を検証（既存維持） |
| feedback_correlation_not_causation.md | ✅ S-13 で因果断定文の否定検証（既存維持） |

---

以上、Fix-C 担当範囲の作業を完了。Fix-A / Fix-B との統合および本番実機 E2E 確認は親セッションへ申し送りとなる。
