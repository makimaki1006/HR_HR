# memory feedback ルール → 実コード対応表

**最終更新**: 2026-04-26
**対象範囲**: MEMORY.md (auto memory) で参照される 14 件の `feedback_*` ルール × V2 実装内の遵守箇所
**根拠**: 各 feedback ルールの事故由来 + V2 コードベース内 grep 結果
**マスター**: ルート [`CLAUDE.md`](../CLAUDE.md) 「絶対ルール」テーブル

---

## 0. 参照方針

memory ルール `feedback_*.md` は **変更禁止**。本ファイルは参照リンクのみ追加し、実コード対応を一覧化。
グローバル設定パス (例: `C:/Users/fuji1/.claude/...`) は記載しない方針 (相対参照のみ)。

---

## 1. 14 ルール対応表

| # | ルール (memory) | 主用途 | 実装/遵守箇所 |
|---|----------------|--------|-------------|
| 1 | `feedback_dedup_rules` | 雇用形態を dedup キーに含める | Python ETL 側の責務。Rust 側では `survey/aggregator.rs:553-672` で emp_group 別集計、`feedback_test_data_validation` の test で逆証明 |
| 2 | `feedback_git_safety` | `git add -A` 禁止、ファイル名指定 | リポ運用ルール。CI/scripts では未強制 → `.gitignore` 強化が必要 (`team_delta_codehealth.md §8.3`) |
| 3 | `feedback_never_guess_data` | 推測禁止、SQL 結果提示 | コード上は phrase_validator (insight) と契約テスト (`global_contract_audit_test.rs`)。報告フェーズの規律 |
| 4 | `feedback_population_vs_posting` | 人口/求人データ混同禁止 | `analysis/fetch.rs` で `v2_external_population` と `postings` を明示分離。UI ラベル「人口」「求人数」を使い分け (`market.rs`, `karte.rs`) |
| 5 | `feedback_turso_priority` | Turso 優先、ローカル更新だけでは本番反映されない | `query_turso_or_local()` ヘルパー (`analysis/fetch.rs:648` 等) で Turso 優先、ローカル fallback |
| 6 | `feedback_hw_data_scope` | HW 掲載のみ、市場全体ではない | 11+ 箇所で明示 (`guide.rs:21,127`, `recruitment_diag/competitors.rs:273-277`, `region/karte.rs:807-808`, `insight/render.rs:99`, `jobmap/correlation.rs:155` 等)。⚠ 市場概況・求人検索・地図メインで欠落 (`team_alpha_userfacing.md §6.1`) |
| 7 | `feedback_implement_once` | 一発で完了、依存把握 | 設計指針。コードでは `pub mod` 管理 (`lib.rs:1-7`) と `mod.rs` で構造化。コミット前に依存チェーン (`include_str!`/`pub mod`/可視性) 確認必須 |
| 8 | `feedback_test_data_validation` | 要素存在ではなくデータ妥当性 | `recruitment_diag/contract_tests.rs`, `survey/parser_aggregator_audit_test.rs`, `pattern_audit_test.rs` で具体値検証 ✅ |
| 9 | `feedback_e2e_chart_verification` | canvas 存在ではなく ECharts 初期化確認 | `static/js/app.js` で htmx:afterSettle 後に setOption。E2E は `e2e_final_verification.py` 等 |
| 10 | `feedback_reverse_proof_tests` | 具体値で検証、要素存在禁止 | `pattern_audit_test.rs` で 22 patterns 各 body の具体値アサート (1,767 行) ✅ |
| 11 | `feedback_turso_upload_once` | 1 回で完了、何度も DROP+CREATE しない | Python ETL 側の責務。Claude による DB 書き込みは禁止 |
| 12 | `feedback_hypothesis_driven` | So What を先に設計 | insight 38 patterns が体現。phrase_validator で「示唆」を強制 |
| 13 | `feedback_correlation_not_causation` | 相関 ≠ 因果 | `phrase_validator.rs` で「確実に/必ず/100%/絶対」を機械的禁止 ✅。`assert_valid_phrase` 適用は LS/HH/MF/IN/GE/SW-F のみ (`engine.rs:1368-1388`) |
| 14 | `feedback_partial_commit_verify` | 依存チェーン確認、ローカル成功 ≠ 本番 | 設計指針。Render の Docker ビルドで `include_str!` 解決を確認すること |
| 15 | `feedback_agent_contract_verification` | agent 個別 pass でも cross-check | `global_contract_audit_test.rs` (19,670 B) で複数タブ JSON shape を tempfile DB で逆証明 ✅。bug marker test 2 件 `#[ignore]` で固定中 (P0 #1, #2) |

(MEMORY.md での 14 ルール → 実際は番号 1-15 だが #14 と #15 が重複しないよう付番。memory 側で正確な数を確認すること)

---

## 2. 適用状況サマリ

| 状態 | ルール数 | 対象 |
|------|---------|------|
| ✅ コード内で機械的検証済 | 5 | #5 (query_turso_or_local), #8 (contract_tests), #10 (pattern_audit), #13 (phrase_validator), #15 (global_contract_audit) |
| 🟡 部分適用 | 4 | #6 (HW 限定明示は 11+ 箇所、市場概況等で欠落), #9 (E2E スクリプト依存), #4 (UI ラベル分離は手作業), #1 (Rust 側集計のみ) |
| ❌ 設計指針のみ (コード強制なし) | 6 | #2 (git_safety), #3 (never_guess), #7 (implement_once), #11 (turso_upload_once), #12 (hypothesis), #14 (partial_commit) |

---

## 3. 強化提案

### 3.1 `.gitignore` で `git add -A` 事故予防 (`feedback_git_safety`)

`.gitignore` に大型バイナリのデフォルト除外を強化。pre-commit hook で `git diff --cached --stat` を表示して目視確認を強制。

### 3.2 phrase_validator 22 patterns 拡張 (`feedback_correlation_not_causation`)

現状: HS/FC/RC/AP/CZ/CF の 22 patterns には `assert_valid_phrase()` 未適用。
提案: `engine.rs` の各 pattern fn 末尾で body を `phrase_validator::assert_valid_phrase()` 通過必須に。P2 改善。

### 3.3 HW 限定スコープ明示の追加 (`feedback_hw_data_scope`)

現状欠落タブ:
- 市場概況 (`/tab/market`)
- 求人検索 (`/tab/competitive`)
- 地図メイン (`/tab/jobmap` のヘッダ)

これらに「HW（ハローワーク）掲載求人に基づく分析です」の注記を追加。

### 3.4 Turso 接続未設定時の警告強化 (`feedback_turso_priority`)

`AppConfig` に Turso 系 4 envvar を統合 (P0 #4)、`from_env()` 起動時に WARN ログを出力。
特に `AUDIT_IP_SALT` がデフォルト値 (`hellowork-default-salt`) のままなら本番起動時に強警告。

---

## 4. memory ルールへの追加リンク方法

V2 内の各実装箇所コメントから memory ルール参照する場合の規約:

```rust
// feedback_correlation_not_causation: 相関≠因果
phrase_validator::assert_valid_phrase(body)?;
```

```rust
// feedback_test_data_validation: 要素存在ではなく具体値で検証
assert_eq!(parsed.salary_median, 250_000);
```

memory ファイル名 (`feedback_xxx`) のみコメントに記載し、絶対パスは含めない (`feedback_implement_once` 遵守: 環境依存パスを残さない)。

---

**改訂履歴**:
- 2026-04-26: 新規作成 (P4 / audit_2026_04_24 #10 対応)。Plan P4 §12 から独立マッピング化
