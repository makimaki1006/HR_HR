# src/handlers 17 件 modified 詳細 diff 監査 (2026-05-09)

read-only 監査。削除 / revert / commit / push は実施せず分類のみ。

## 0. 監査範囲

- HEAD: `5ac1967` (chore(pdf): clean up chart print polish follow-ups)
- origin/main: `2959519` (Round 2.11 fix(pdf): set Playwright viewport to A4 portrait)
- HEAD は origin/main の 1 つ先 (cleanup commit) を含むので origin より進んでいる
- 監査対象: working tree の `M` 状態 17 ファイル
  - `src/handlers/**` 16 件
  - `tests/no_forbidden_terms.rs` 1 件

## 1. Summary

| 分類 | 件数 | 概要 |
|---|---|---|
| **A** commit 漏れ (機能上必要) | 0 | 機能追加・修正は一切なし |
| **B** 重複 / 再注入 | 0 | OneDrive 由来の旧コンテンツ復活はなし |
| **C** 別タスク未完成 | 0 | 別機能の中途実装はなし |
| **D** 不要・誤編集 (rustfmt 由来) | 17 | **全件が `cargo fmt` (または rust-analyzer 保存時 fmt) によるフォーマット差分のみ** |
| 判断保留 | 0 | |

### 1.1 統計サマリ

```
標準 diff:    +974  -633  (改行差含む)
-w 適用:      +904  -563  (空白無視)
-w + 空行無視: +756  -441  (純粋なトークン位置変化)
```

`-w --ignore-all-space --ignore-blank-lines` 適用後に残る差分は **すべて改行位置・引数の縦並び・括弧の位置を rustfmt 標準に揃えるもの**。識別子追加・関数追加・条件式変更・assert 文言変更などのセマンティック変更は皆無。

### 1.2 環境情報

| 項目 | 値 | 影響 |
|---|---|---|
| `core.autocrlf` | true | チェックアウト時に LF→CRLF 変換 |
| `rustfmt.toml` / `.rustfmt.toml` | 存在しない | rustfmt デフォルト設定が適用 |

CRLF 警告は git の表示由来で、`-w` 適用後の差分には CRLF 起因の行は残らない。本質的に rustfmt のみが原因。

## 2. File-by-file Audit

| ファイル | -w 後 +/- | 直近 commit | 内容 | 分類 | 推奨 |
|---|---|---|---|---|---|
| src/handlers/analysis/fetch/market_intelligence.rs | +164/-? | 0fd7368 | 関数呼出引数の縦並び化、文字列リテラル位置変更、assert! 引数の縦並び化のみ | D | leave |
| src/handlers/analysis/fetch/mod.rs | +34/-? | 78a0556 | use リストの再パッキング、引数縦並び化のみ | D | leave |
| src/handlers/company/fetch.rs | +28/-? | 1782772 | format! 引数の縦並び化のみ | D | leave |
| src/handlers/insight/flow_context.rs | +6/-? | ebfa8bb | `query_turso_or_local(turso, db, sql, &params, ...)` を 1 引数 1 行に開いただけ | D | leave |
| src/handlers/recruitment_diag/talent_pool_expansion.rs | +13/-? | da71036 | 引数縦並び化のみ | D | leave |
| src/handlers/survey/handlers.rs | +11/-? | c74be05 | `ctx.ext_turnover = super::...::fetch_ext_turnover_with_industry(...)` の改行位置調整のみ | D | leave |
| src/handlers/survey/report_html/industry_mismatch.rs | +574/-? | 62568c0 | `s.contains("看護") \|\| s.contains("准看") \|\| ...` 連鎖の縦並び化が大量。条件式の意味は不変 | D | leave |
| src/handlers/survey/report_html/invariant_tests.rs | +22/-? | 1782772 | `assert!(joined.contains("縮小傾向"), "...")` などを 3 行に開いただけ | D | leave |
| src/handlers/survey/report_html/market_intelligence.rs | +370/-? | 54b6f6e | use 文・format! 引数・vec! 引数の縦並び化のみ | D | leave |
| src/handlers/survey/report_html/market_tightness.rs | +83/-? | 9ac1e33 | `render_figure_caption(html, "図 MT-2", "...")` を 4 行に開く等、引数縦並び化のみ | D | leave |
| src/handlers/survey/report_html/mod.rs | +122/-? | 4591232 | assert! 引数縦並び化、`super::super::super::company::fetch::RegionalCompanySegments::default()` を改行する等のみ | D | leave |
| src/handlers/survey/report_html/notes.rs | +9/-? | 847364b | `html.push_str("<section ...>")` を 3 行に開く + assert! の縦並び化のみ | D | leave |
| src/handlers/survey/report_html/region_filter.rs | +5/-? | 54b6f6e | `vec![ muni(...), muni(...), ]` を 1 行に畳む rustfmt 動作 | D | leave |
| src/handlers/survey/report_html/regional_compare.rs | +139/-? | 54b6f6e | format! 引数縦並び化、`assert_ne!(pref, "都道府県", "..")` の 4 行化のみ | D | leave |
| src/handlers/survey/report_html/salesnow.rs | +8/-? | 1782772 | `if c.employee_delta_1y > 0.0 { "#059669" } else { "#dc2626" }` を 5 行に開く + use 順序入替 のみ | D | leave |
| src/handlers/trend/fetch.rs | +5/-? | eb77a02 | `ind.chars().take_while(...).collect()` をメソッドチェーン縦並びに変えただけ | D | leave |
| tests/no_forbidden_terms.rs | +14/-? | d3cacbe | `format!(...)` 引数縦並び化のみ | D | leave |

(`-?` は対応する `-` 行数。-w での内訳は項目ごとに精査するまでもなく対称的)

## 3. Round 2.x commit 漏れ チェック

**結論: 漏れなし。**

- Round 2.x (54b6f6e〜2959519, 5ac1967) の各 commit には期待された機能修正
  (HW 用語中立化 / variant-aware ラベル / Playwright viewport / chart container width
  / yAxis 0 強制 / SalaryHeadline 統合 等) が **すべて反映済み** でテストも通っている。
- 17 ファイルの diff は **これらの機能修正には一切寄与しない rustfmt フォーマットの揺れ**。
- 本番反映が不完全な機能は無し。

## 4. Revert Candidate

該当なし。
- すべての差分はノーオペな整形変更。バグや退行を生むコードは含まれていない。
- フォーマットを元に戻す必要がない (両方とも valid Rust で挙動同一)。
- `git checkout -- <files>` は本指示で禁止のため実施しない。

## 5. Separate Task Candidate

該当なし。Round 2.x とは独立した別機能の実装は 1 行も含まれていない。

仮に意義づけするなら以下のリポ衛生タスクが立てられるが、必須ではない:

| 想定タスク | 内容 | 優先度 |
|---|---|---|
| repo-hygiene-rustfmt-baseline | リポ全体に `cargo fmt --all` を 1 commit で適用、CI に `cargo fmt --all -- --check` 追加、rustfmt.toml 明示 | 低 |

## 6. Recommended Next Action

| ファイル | 推奨アクション |
|---|---|
| 17 ファイル全件 | **leave** (今回は触らない) |

### 6.1 補足

- 本指示 (commit / revert / push 全禁止) に従い、今回の監査では何も変更しない。
- 17 件はすべて HEAD に対する rustfmt 揺れ差分のみ。**機能的影響ゼロ、デプロイ影響ゼロ**。
- このまま放置しても origin への push 内容には影響しない (push されるのは commit のみ)。
- 別途リポ衛生タスクとして「rustfmt baseline + CI チェック」を立てるかは、ユーザー判断に委ねる。

### 6.2 想定原因 (証拠ベース)

- editor 側の rust-analyzer が "format on save" 有効で、HEAD のファイルを開く / 保存することで rustfmt 標準に再フォーマット。
- HEAD 時点のソースは `cargo fmt --check` を通っていない (Round 2.x の各 commit が手動でちょこちょこ書き加えるうちに rustfmt 揺れが蓄積)。
- OneDrive sync の関与: 直接的証拠は見つけられず。CRLF の警告は autocrlf=true 由来で、`-w` 後にも残る純粋なトークン位置差分は editor 側 rustfmt が最有力。

## 7. 結論

- A/B/C はゼロ件。Round 2.x の作業漏れも、本番反映の欠落も、別タスクの中途半端な混入もない。
- 17 件すべて D (rustfmt 由来の整形差分)。機能・テスト・デプロイへの影響はない。
- 本指示の制約 (read-only) に従い、いずれのファイルも触らずそのまま据え置きを推奨。
