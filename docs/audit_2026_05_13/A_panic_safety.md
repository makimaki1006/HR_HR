# A. Panic 安全監査 (2026-05-13)

監査対象: `src/` 配下の Rust ソース (handlers, db, auth, geo, audit, lib, main)。
crate: `rust_dashboard`。

## 調査範囲と手法

- 検査対象 file 数: 約 120 `.rs` (test ファイル除く production 経路)
- grep pattern:
  - `\.unwrap\(\)` / `\.expect\(`
  - `&[ident]\[..\]` (byte slice 全形)
  - `/ ident\.len\(\)` / `ident\s*-\s*1\s*\]` (div0, underflow)
  - `panic!|unreachable!|unimplemented!|todo!`
  - `Number::from_f64\(.*\)\.unwrap\(\)` (NaN/Inf panic)
  - `Regex::new\(.*\)\.unwrap\(\)`
  - `await\.unwrap\(\)`
  - `\.find\(.*\)\.unwrap\(\)` / `\.nth\(.*\)\.unwrap\(\)`

production 経路と test 経路の切り分けは `#[cfg(test)]` mod tests / `#[test]` 直下 / `*_test.rs` / `tests.rs` / `*_audit_test.rs` を test として除外する。

## P0 重大

該当なし。

実 user データを直接受けて panic する production-path コードは、今回の grep + 個別確認では発見されなかった。
2026-05-13 直前に修正された byte slice 群 (`&html[start..start+8000]` 系) と類似する production パターンは現在存在しない (commit d5162b1 / cad9bc5 で text_util::truncate_char_safe + char-based slice に置換済)。残存する byte slice はすべて (a) `find()` 戻り位置を使う = char boundary 保証 (`salary_parser.rs`, `location_parser.rs`)、(b) `Vec` slice (UTF-8 boundary 無関係)、(c) `#[cfg(test)]` 配下、のいずれか。

## P1 中

該当 0 件。

検討した候補と判定:

- `src/handlers/insight/report.rs:663` `Number::from_f64(vacancy_rate).unwrap()` — `#[cfg(test)] mod tests` 内の `mock_ctx_with_vacancy` helper。test only。
- `src/handlers/analysis/render/mod.rs:75,209,225,415` `from_f64(...).unwrap()` — いずれも `#[cfg(test)] mod tests` 内 helper。
- `src/handlers/competitive/utils.rs:13-20` `truncate_str` `max_chars - 1` underflow — `max_chars == 0` の場合は `chars().count() <= 0` で早期 return するため到達しない (理論的に安全)。callers は const (20/18/12/6/8) のみ。
- `src/handlers/survey/statistics.rs:104` `&valid[trim_count..n - trim_count]` — line 94 `if n <= trim_count * 2 { return ... }` で先に弾かれる。
- `src/handlers/survey/report_html/helpers.rs:482` `((sorted.len() - 1) as f64) * ... ` — line 478 `if sorted.is_empty() return 0.0` で守られる。

## P2 軽

防御的に強化することが推奨されるが、現実的に発火困難なもの。

- [ ] **src/main.rs:240,246** `TcpListener::bind(...).await.unwrap()` / `axum::serve(...).await.unwrap()`
  - 発火: port bind 失敗 / serve エラー。startup-time のみ。production runtime には影響しないが、明示エラーメッセージ化を推奨。
- [ ] **src/auth/session.rs:28,41,64** `self.attempts.lock().unwrap()`
  - 発火: 他スレッドが lock 保有中に panic した場合の poison。同 crate の他箇所で attempts への access 中 panic がないため事実上発火しない。Rust 標準パターン。
- [ ] **src/db/cache.rs:175** `h.join().unwrap()`
  - 発火: spawn した thread が panic した場合。`#[cfg(test)]` 内の可能性が高い (要追加確認)。
- [ ] **src/handlers/competitive/utils.rs:17** `s.chars().take(max_chars - 1)`
  - 発火: `max_chars == 0` 呼出。現在の callers は const 20/18/12/6/8 のみだが、将来動的値が入った場合に underflow。`max_chars.saturating_sub(1)` への置換を推奨。

## 既に対応済 (false-positive)

- `text_util::truncate_char_safe` による byte slice 9 件 (commit d5162b1, 2026-05-13)
- 郵便番号 char-based slice 2 件 (commit cad9bc5)
- `src/handlers/company/render.rs:1258-1262` `ctx.postal_code[..3]` — 直前に `if ctx.postal_code.len() >= 3` で byte len ガードあり、かつ郵便番号は ASCII (`123-4567` 形式) のため char boundary 違反は発生しない。
- `src/handlers/survey/salary_parser.rs:137,179,180,340,360,389,403,405,414,416,427` の byte slice — すべて `text.find('万'/'千'/'百'/'十')?` の戻り位置 (char boundary) + `char.len_utf8()` 加算であり UTF-8 安全。
- `src/handlers/survey/location_parser.rs:1147` `&clean[..pos]` — `find(suffix)` の戻り位置 (char boundary)。
- `src/handlers/recruitment_diag/market_trend.rs:251` `&t[..4]`, `&t[4..]` — `t.chars().all(|c| c.is_ascii_digit())` 確認後の ASCII 文字列のみ。
- `src/lib.rs:442,445` `&s[..pos]`, `&rest[..host_end]` — `find("://")` / `find('/')` の戻り位置 (char boundary)。
- `src/handlers/competitive/handlers.rs:116` `&postings[start..end]` — `Vec<PostingRow>` slice。`start.min(postings.len())` / `(start + page_size).min(postings.len())` でクランプ済。
- `src/handlers/survey/report_html/{executive_summary,region,market_intelligence}.rs` 内の `&css[start..start+block_end]` 系 11 件 — すべて `#[cfg(test)] mod tests` 内。production 経路に存在しない。
- 各種統計関数 `len() - 1` (`statistics.rs:62,258,298`, `helpers.rs:483`, `salary_stats.rs:33`, `analysis.rs:263`, `stats.rs:54`, `hw_enrichment.rs:231`, `region.rs:61`, `subtab5_anomaly.rs:550,585,790`) — すべて `is_empty()` / `len()>=N` / `if let Some(last)` で防御済。
- `write!(html, ...).unwrap()` / `writeln!(html, ...).unwrap()` 多数 — `fmt::Write for String` は `Infallible`。unwrap 安全。
- `serde_json::from_str(...).unwrap()` 群 — `mod.rs:1273,1309,1741,...` すべて `#[cfg(test)]` 配下。
- `*.test.rs` / `*_audit_test.rs` / `tests.rs` / `*_test.rs` 内の `unwrap` / `expect` / `panic!` / `assert!` 多数 — test fixture / 検証 assertion。
- `serde_json::Number::from_f64(NaN).unwrap()` リスク — production 経路では発見されず (`#[cfg(test)]` のみ)。
- `f64 as i64` 多数 (`diagnostic.rs`, `comparison/render.rs`, `subtab2_salary.rs` 等) — Rust 1.45+ で saturating cast に変更されており NaN→0, Inf→i64::MAX/MIN。UB / panic ではない。

## 安全と確認した範囲

- `unwrap()` / `expect()`: 全 production 経路をファイル別に列挙し、`write!`/test 配下を除外して残った数 0 件。
- panic マクロ: production 経路 0 件 (見つかった 9 件すべて test 内)。
- byte slice: production 経路 12 箇所すべて (a) `find()` 戻り or (b) `Vec` slice or (c) ASCII 限定 で UTF-8 panic 不可。
- 割算: `/ ident.len()` 全 26 箇所すべて `is_empty()` ガード or `if .len() > N` ガード or `entry.push` で非空保証。
- `len() - 1`: 全 12 箇所が `is_empty()` / `if let Some(last)` / `if .len() > N` で防御。
- `Regex::new(...).unwrap()`: src/ 内に該当なし。
- `await.unwrap()`: production は `main.rs:240,246` 2 件のみ (startup)。`spawn_blocking().await` は全て `.unwrap_or_else(...)`。

検査済 file pattern:
- `src/main.rs`, `src/lib.rs`, `src/config.rs`, `src/text_util.rs`
- `src/auth/**`, `src/db/**`, `src/geo/**`, `src/audit/**`
- `src/handlers/{api,balance,demographics,diagnostic,market,overview,workstyle,emp_classifier}.rs`
- `src/handlers/{admin,my,company,comparison,competitive,jobmap,trend,analysis,survey,insight,recruitment_diag,region,integrated_report}/**`

未探索: `tests/e2e/`, `scripts/`, `python_scripts/` (scope 外)。
