# H 領域監査: テスト戦略 (2026-05-13)

監査範囲: `src/**/*test*.rs` (17 ファイル), `tests/` (Rust 3 + Python 2), `tests/e2e/` (Playwright 15 spec)。test 件数 1483 (cargo test --lib)。
監査方針: read-only、test 実行・build なし。「PASS = 妥当性 OK」を否定する観点で assertion の中身を読む。

---

## P0: 既知事故と同種パターン (assertion = 要素存在のみ)

### P0-1. K1 / K2 / K3 が「現状固定 (バグ放置)」のまま 1 ヶ月以上経過
- **file:line**: `src/handlers/survey/report_html/round12_integration_tests.rs:286-341`
- **症状**:
  - K1 `k1_dominant_pref_muni_inconsistency_silently_passes` (L289): 「東京都+川崎市」不整合を `assert!(!has_warning, "現状は警告なし。aggregator 層に整合性チェック追加が必要")` で **PASS 固定**。コメントで「必要」と認めながら 1 ヶ月放置。
  - K2 `k2_municipality_table_column_order_fixed` (L307): UX バグを「現状固定」として `assert!(p_muni < p_pref)`。
  - K3 `k3_min_wage_zero_count_phrasing_is_present` (L333): 文言矛盾を「現状固定」。
- **既知事故関係**: K9-K17 は Round 12-13 で修正済 (assert 反転)。K1-K3 だけ取り残されており、`bug_marker_workflow.md` の lifecycle (24h warning → 1 週間 red) に違反。`#[ignore]` 0 件は健全だが、anti-pattern は #[ignore] ではなく「現状 PASS で固定」の形で隠れている。
- **影響**: バグの存在を test が言語化しているのに修正されない silent failure。今朝の `graphic.is_empty()` 事故と同質 (test が現状の振る舞いを縛り、修正を遅らせる)。
- **修正**: K1/K2/K3 を `#[ignore = "BUG-K1: aggregator 整合性チェック未実装"]` に切り替え、bug tracker に紐付け。または修正コミットで assert 反転。

### P0-2. E2E smoke が `mi-empty` でも PASS する OR 条件
- **file:line**: `tests/e2e/market_intelligence_smoke.spec.ts:47-58`
- **assertion**:
  ```ts
  const hasMiMarker = html.includes('mi-parent-ward-ranking') ||
                      html.includes('mi-rank-table') ||
                      html.includes('mi-empty');
  expect(hasMiMarker).toBe(true);
  ```
- **問題**: 5 つの OR 条件のうち 1 つが `mi-empty` (= データなし表示)。本番で全パネルブランク (2026-04-23 8 パネル全滅事故と同型) でも PASS。コメントには `feedback_e2e_chart_verification.md: 「存在」だけでなく中身を検証する` と書いてあるが、実装は逆を行く。
- **既知事故関係**: feedback_e2e_chart_verification.md (canvas 存在のみで 19/24 ブランク見逃し)、feedback_agent_contract_verification.md (8 パネル全滅) の再発条件が残存。
- **修正**: `mi-empty` を OR から外す。または ECharts `getInstanceByDom` で series 数 >= 1 を検証する step を追加。

### P0-3. ECharts initialize 検証 E2E が 1 箇所のみ
- **file:line**: `tests/e2e/helpers/pdf_helper.ts:50` (`echarts?.getInstanceByDom?.(el)`)
- **観察**: 15 spec のうち `getInstanceByDom` を使うのは pdf_helper.ts のみ。`canvas|toBeVisible` 系の表面チェックは 66 件 (11 ファイル)。
- **影響**: ECharts が `setOption` を空 series で呼ばれるとブランクだが canvas は存在 → 全 spec で見逃せる。feedback_e2e_chart_verification.md 違反が広範に残存。
- **修正**: helpers に `expectChartInitialized(page, selector)` を追加し、`echarts.getInstanceByDom(el).getOption().series.length >= 1` を強制。

### P0-4. Rust unit test に「要素存在のみ」assertion が大量存在
- **file:line**: `src/handlers/survey/integration.rs:1721-2271` (例: L1721 `assert!(html.contains("東京都"))`, L2039 `assert!(html.contains("100"), "件数 100 表示")`)
- **count**: src 全体で `assert!(.*\.contains("` 系は 201 occurrences / 30 files。多くが要素存在のみで「データが正しい場所に正しい意味で出ているか」を検証していない。
- **具体的弱点**: `assert!(html.contains("100"))` は `"<tr>100</tr>"` でも `<div>1001 件</div>"` でも PASS。HTML 内で複数箇所に同じ数値が出る場合、対象テーブルの行と紐付けられない。
- **既知事故関係**: feedback_test_data_validation.md (2026-03-22 バグ見逃し)、feedback_code_first_test_second.md (今朝 stats_close 事故) の再発条件。
- **修正**: select_html / scraper crate で DOM パース → 該当 selector からテキスト抽出 → assert に置換。最低限の P0 対象は integration.rs (40+ 箇所)。

---

## P1: 逆証明・契約・fixture の弱点

### P1-1. invariant_tests.rs は優秀だが survey ドメインに限定
- **file:line**: `src/handlers/survey/report_html/invariant_tests.rs:1-907`
- **good**: unemployment <= 100%、score 0-100、6 マトリクス sum <= total、% 合計 100±0.1 等 10 不変条件。逆証明テストとして模範的。
- **弱点**: `jobmap`, `recruitment_diag`, `competitive`, `trend`, `region` のドメイン不変条件 (給与下限<上限、年間休日 0-365、求人倍率 >= 0) が同等カバレッジで存在しない。
- **修正**: 各 handler に invariant_tests.rs を 1 ファイル追加 (各 5-10 不変条件)。

### P1-2. E2E fixture が production と乖離
- **file:line**: `tests/e2e/fixtures/indeed_test_50.csv`, `jobbox_test_50.csv` (50 行)
- **乖離**: 本番 86 万行 → fixture 50 行で「都道府県 47 件揃う」「Top10 制限」「同名市区町村」等の境界が発火しない。今朝の事故 (stats_close) も小規模 fixture では再現しない。
- **修正**: fixture を 47 都道府県×複数業界×給与帯横断にする最低限 1,000 行版を追加。

### P1-3. recruitment_diag 契約 test は cross-check 実装済だが他 handler 未適用
- **file:line**: `src/handlers/recruitment_diag/contract_tests.rs:1-100` (8 パネル契約検証)
- **good**: feedback_agent_contract_verification.md に基づき backend JSON shape と frontend key を cross-check。
- **弱点**: `integrated_report/contract_tests.rs`, `comparison/contract_tests.rs` は存在するが frontend renderer 側との shape 一致 cross-check が未実装 (handler の output 形状のみ確認)。
- **修正**: tab ごとに「renderer が読む key list を抽出 → handler output で全て存在」する test を追加。

### P1-4. screenshot 視覚レビューが round 単位、CI 連動なし
- **file:line**: `tests/e2e/_print_visual_review.spec.ts`, `_print_review_p1*.spec.ts` (アンダースコア prefix = 手動実行 only)
- **問題**: feedback_llm_visual_review.md (cargo test 単体は合意確認止まり) を受けて作成された spec だが、ファイル名 prefix `_` で playwright 自動実行から除外。round で人手起動しないと走らない。
- **修正**: CI に「nightly visual job」を切り出し、PNG → 目視 PR comment 投稿 (Anthropic API or 手動) のフローを定義。

---

## P2: 重複・命名・運用

### P2-1. `assert!(html.contains("東京都"))` 系の重複
- integration.rs:1721, 1860, 2036 など同じ assertion が 10+ 回。
- **修正**: `assert_html_contains_all(&html, &["東京都", "神奈川県", ...])` ヘルパで圧縮。

### P2-2. `#[ignore]` 0 件 = 健全だが bug_marker_workflow.md の運用 step (Phase 2 = 2026-05-15 以降 24h warning) は未実装
- workflow ドキュメントは整備されているが、`scripts/check_ignored_tests.sh` は audit_2026_04_24 ディレクトリの暫定配置のままで CI から呼ばれていない。
- **修正**: scripts に正式昇格 + CI step 追加。

### P2-3. helpers_round12_tests.rs が空 (4 行)
- `src/handlers/survey/report_html/helpers_round12_tests.rs` は実質空ファイル。
- **修正**: 削除または内容追加。

---

## サマリ表

| ID | 件名 | 優先度 | 工数 |
|---|---|---|---|
| P0-1 | K1/K2/K3 現状固定の anti-pattern | P0 | 30 分 |
| P0-2 | E2E smoke が mi-empty で PASS | P0 | 10 分 |
| P0-3 | ECharts initialize 検証の広範欠落 | P0 | 2 時間 |
| P0-4 | html.contains だけの assertion 201 件 | P0 | 1 日 |
| P1-1 | invariant test の他 handler 横展開 | P1 | 半日 |
| P1-2 | E2E fixture 50 行 → 1000 行 | P1 | 1 時間 |
| P1-3 | 他 handler への契約 cross-check | P1 | 半日 |
| P1-4 | visual review の CI 連動 | P1 | 半日 |
| P2-* | 重複、命名、workflow 昇格 | P2 | 1 時間 |
