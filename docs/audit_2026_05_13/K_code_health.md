---
audit_area: K
title: コード健全性 / 構造 監査
date: 2026-05-13
scope: src/, tests/
mode: read-only
totals:
  rust_files: ~110
  total_loc: 101806
  largest_file_loc: 5182
  pub_symbols_grepped: 351
  allow_dead_code_attrs: 88
  println_eprintln_call_sites: 6
  tracing_log_call_sites: 82
---

# K. コード健全性 / 構造 監査

## サマリー (200 語以内)

hellowork-deploy (rust_dashboard crate) は約 102K LoC、110 ファイル規模。V1/V2 分離は遵守済み (V1 cross-import は 0 件 — `jobmedley/ジョブメドレー/V1` token は CLAUDE.md 内 1 件のみ)。logging は `tracing` 82 箇所に概ね統一されており、`println!/eprintln!` 残骸はテスト周辺の 6 件のみで本番経路には侵入していない。一方で構造債務が顕著: (1) 5,000 行超ファイルが 4 本、3,000 行超が 5 本存在し、特に `survey/report_html/market_intelligence.rs` (5,182 行) / `report_html/mod.rs` (4,260 行) は責務肥大。(2) `render_survey_report_page` 系で 7 段の後方互換ラッパーが累積 (Full→v1→…→v3_themed)。(3) `#[allow(dead_code)]` が 88 箇所、特に `analysis/fetch/market_intelligence.rs` 単独で 37 件 — Phase 3 未完の DTO 群が放置リスク化。(4) error 型は anyhow/thiserror どちらも使われず Result の自前運用で、`.unwrap()` 65 + `.expect()` 52 件が本番経路にも散在。(5) module 境界では `super::super::super::` 4 段ネスト 65 件、`crate::handlers::` 直参照と混在し import 規約が未統一。P0 (V1/V2 contamination) は検出なし。P1 として巨大ファイル分割と progressive ラッパー縮約を推奨。

## 観点別所見

### 1. 巨大ファイル (P1)

| File | LoC | 備考 |
|------|-----|------|
| `src/handlers/survey/report_html/market_intelligence.rs` | 5,182 | Phase 3 Step 3 単独。5 セクション分の HTML 生成が同居 → 5 ファイルに分割候補 |
| `src/handlers/survey/report_html/mod.rs` | 4,260 | 7 つの `render_survey_report_page*` ラッパー + 6 つの test module をホスト。`#[cfg(test)] mod tests/ui2_contract_tests/readability_contract_tests/design_v2_contract_tests/variant_indicator_tests` (5 個) を別ファイルへ |
| `src/handlers/survey/report_html/market_tightness.rs` | 3,827 | |
| `src/handlers/analysis/fetch/market_intelligence.rs` | 2,860 | DTO + fetch 関数 35 個。DTO 層と fetch 層を分離可能 |
| `src/handlers/survey/report_html/style.rs` | 2,735 | CSS 文字列リテラルの集積。テーマ別 (`Default`/`v7a`/`v8`) で 3 ファイルに分割可 |

**証拠**: `find src -name "*.rs" -size +60000c` (上記サイズ降順)

### 2. Progressive ラッパー累積 (P1)

`src/handlers/survey/report_html/mod.rs:410-638` に 7 段のラッパー:

```
render_survey_report_page (8 引数)
 → render_survey_report_page_with_enrichment (9)
   → render_survey_report_page_with_municipalities (10)
     → render_survey_report_page_with_variant (11) [#[allow(clippy::too_many_arguments)]]
       → render_survey_report_page_with_variant_v2 (12)
         → render_survey_report_page_with_variant_v3 (13)
           → render_survey_report_page_with_variant_v3_themed (16)
```

- 各層が直下層へ委譲するだけの薄ラッパー。
- 引数 8 → 16 まで線形増加。`Builder` パターン or `RenderConfig` struct に集約可能。
- `#[allow(clippy::too_many_arguments)]` が 4 箇所 (line 500, 533, 574, 617) — clippy 警告を黙らせている。

### 3. `#[allow(dead_code)]` 濫用 (P1)

総 88 箇所。特に集中:
- `src/handlers/analysis/fetch/market_intelligence.rs`: 37 件 (line 31-1266)
- `src/handlers/survey/report_html/market_intelligence.rs`: 18 件
- 例 (line 580): `pub enum DataSourceLabel { ResidentActual, ... }` — Phase 3 後続で使うが現状未使用 5 variants
- 例 (line 1248): `pub struct WardRankingRowDto { ... 11 fields }` — `is_priority_score_in_range` 等の不変条件 method 群が dead

**risk**: Phase 3 Step 後続 (METRICS.md) が完了するまで dead 状態が長期化。implementation_once ルール (一発完了) に反する。対応: feature flag `phase3_mi` を切って `#[cfg(feature = ...)]` で囲うか、未使用なら一旦削除し PR 復元。

### 4. module 境界 / import 規約混在 (P2)

- `super::super::super::` 4 段ネスト: 65 件 (35 ファイル)
- `crate::handlers::` 直参照: 9 件 (5 ファイル)
- 例: `src/handlers/survey/report_html/market_intelligence.rs:25` は `super::super::super::analysis::fetch::{...}` (35 シンボル import)
  - `crate::handlers::analysis::fetch::*` のほうが可読性高い
- V1/V2 cross-import: **0 件** (`V1|jobmedley|ジョブメドレー` grep で `handlers/CLAUDE.md` のドキュメントのみ) → **P0 該当なし**

### 5. error handling 統一性 (P1)

- `anyhow::` import: 0 件
- `thiserror::` import: 0 件
- 各関数が `Result<T, E>` の `E` を毎回手書き / `String` でラップ。
- `.unwrap()` 65 件 (主要分布: `local_sqlite.rs:24`, `diagnostic.rs:13`, `analysis/fetch/market_intelligence.rs:10`, `auth/session.rs:3`)
- `.expect()` 52 件 (test 中心だが `analysis/fetch/mod.rs:11` `market_intelligence.rs:18` は本番経路)
- 推奨: `anyhow` 導入で handler 層 / `thiserror` で db 層を分離

### 6. logging 一貫性 (P2 — 良好)

- `tracing::(info|warn|error|debug)`: 82 件 (20 ファイル)
- `println!/eprintln!`: 6 件 (4 ファイル)
  - `src/handlers/survey/report_html/industry_mismatch.rs:2`
  - `src/handlers/survey/report_html/mod.rs:1`
  - `src/handlers/survey/report_html/region.rs:2`
  - `src/handlers/survey/report_html/round12_integration_tests.rs:1`
- いずれも `#[cfg(test)]` または test module 内のデバッグ出力と推定。本番経路への混入は確認限り無し。

### 7. TODO/FIXME (P2)

- `TODO/FIXME/XXX/HACK` を `src/` で grep → **2 件**のみ (`insight/phrase_validator.rs:79,91` で文字列リテラル中の "XXX" placeholder — テスト用)
- → **未解決 TODO の本番混入なし**。良好。

### 8. `#[ignore]` テスト (P2)

- `src/handlers/global_contract_audit_test.rs:439-449` に「既知ミスマッチの記録テスト (`#[ignore]`)」
- ルール `feedback_bug_marker_no_ignore.md` (silent failure 防止) に基づき修正 PR と同コミットで ignore を外す運用が docs に記載されており、運用ルールは整備済み。
- 実コードに `#[ignore]` attribute が貼られている箇所は `global_contract_audit_test.rs` のみ (要追跡)。

### 9. let _ = (warning 抑制) 濫用 (P2)

- `let _ = ` 49 件 (21 ファイル)
- 例: `src/lib.rs:21` (Router の `.layer` chain 抑制)
- 大半は legitimate (Future/Result の意図的破棄)。`audit/dao.rs:3` `handlers/survey/handlers.rs:3` は要検証 — error を黙殺している可能性。

### 10. public API 露出 (P2)

- `^pub fn|^pub async fn|^pub struct|^pub enum` で 351 件
- crate 内部のみで使う API が `pub` に昇格しているケース散見:
  - `src/handlers/survey/report_html/market_intelligence.rs:50,52,54,58` の `pub const MEASURED_LABEL/ESTIMATED_LABEL/REFERENCE_LABEL/INSUFFICIENT_LABEL` — モジュール内のみ使用なら `pub(crate)` で十分
- 既に `pub(crate) fn render_survey_report_page*` が 7 件 (`report_html/mod.rs:410-618`) — module 境界は概ね適切

## 推奨アクション (priority 順)

| P | item | file:line | 工数目安 |
|---|------|-----------|---------|
| P1 | `report_html/mod.rs` を分割 (4,260→<1,500): test module 5 個 + ラッパー群を別ファイル化 | mod.rs:410-638, 1108-3057+ | 4h |
| P1 | progressive ラッパー 7 段 → `RenderConfig` struct + Builder で 1 entry に統合 | mod.rs:410-638 | 3h |
| P1 | `market_intelligence.rs` (5,182 行) を 5 セクション別ファイルに分割 (`mi_summary.rs`/`mi_distribution.rs`/`mi_talent.rs`/`mi_salary.rs`/`mi_scenario.rs`) | report_html/market_intelligence.rs | 4h |
| P1 | `analysis/fetch/market_intelligence.rs` の 37 個の `#[allow(dead_code)]` 棚卸 — Phase 3 未完なら `feature = "phase3_mi"` で gate | fetch/market_intelligence.rs:31-1266 | 2h |
| P1 | error 型統一: `anyhow` 導入 + handler 層で `Result<_, anyhow::Error>` 標準化 | crate 全体 | 6h |
| P2 | `let _ = ` の audit (49 件中 error 黙殺の特定) | grep `let _ = .*\?` | 1h |
| P2 | `super::super::super::` → `crate::` 統一 (rustfmt rule化不可なため code review 規約) | 35 ファイル | 2h |

## 終了
- P0 (V1/V2 cross-contamination) は **検出なし**
- P1 主因は構造債務 (巨大ファイル + 累積ラッパー + dead_code 放置)
- 監査スコープ: read-only。ビルド・テスト未実施 (grep + Read のみ)
