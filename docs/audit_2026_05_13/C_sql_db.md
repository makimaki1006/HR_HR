# C 領域監査: SQL / DB 経路

**対象**: `src/handlers/`, `src/db/`, `scripts/`
**監査日**: 2026-05-13
**監査方針**: read-only (build/test/DB 書込なし)

---

## サマリー

全 SQL 経路 (約 200 件超の SELECT / format!() ベース動的 WHERE) を走査した結果、ユーザー入力を直接 SQL リテラルへ埋め込む経路は確認されなかった。すべての user-controlled 値は `?N` プレースホルダ + `params_own` バインディング、または `emp_classifier::expand_to_db_values` 等のドメイン allowlist を経由している。`ORDER BY` の動的列名は `condition_gap.rs:297` で `["salary_min","bonus_months","annual_holidays"]` の明示 allowlist 検査済み。

主要な懸念は (1) **`v2_external_*` テーブル群に対するヘッダー混入ガード `EXTERNAL_CLEAN_FILTER` が `population` / `foreign_residents` 以外 ~18 テーブルに未適用** で過去事故の再発リスクが残存していること、(2) **`src/audit/dao.rs` が本番 Turso に対し login/activity 毎に 2〜3 回の execute() を発行しトランザクション境界なし**、(3) **LIKE 検索の `%`/`_` エスケープ未実装** (`company/fetch.rs:122`)、の 3 点。SQL injection の実害可能性は確認できなかったが、Turso 書込量管理 (2026-01-06 $195 事故) 観点で audit DAO の運用検証が必要。

---

## P0 (重大)

### P0-1: 本番 Turso への runtime 書込経路

**file:line**: `src/audit/dao.rs:85-104, 156, 190, 219, 292, 341-343`

**evidence**:
```rust
// dao.rs:85
let _ = turso.execute(
    "UPDATE accounts SET role = ?1, last_login_at = ?2, login_count = login_count + 1 WHERE id = ?3",
    &[&role, &now, &id],
);
```

**問題**:
- login 1 回ごとに最低 1 UPDATE (accounts) + 1 INSERT (login_sessions) + 場合により INSERT activity_logs を本番 Turso 経由で発行。
- `purge_old_logs()` (line 341) は `DELETE FROM activity_logs` / `DELETE FROM login_sessions` を実行。
- 2026-01-06 の $195 超過事故は「Claude による複数回 Turso 書込」が原因。同じ書込経路がアプリ runtime にも存在することを明示すべき。

**影響**: 高トラフィック時の Turso 書込量爆発リスク。SQL 自体は parameterized で安全。

**推奨**: (a) `purge_old_logs` の cron 化と頻度制限、(b) activity_logs を local SQLite に逃がすかバッチ集約、(c) Turso 書込メトリクス監視。

---

## P1 (重要)

### P1-1: `v2_external_*` ヘッダー混入ガード未適用テーブル多数

**file:line**:
- `src/handlers/analysis/fetch/subtab7_phase_a.rs:29, 48, 66, 84, 96, 109, 130, 149, 167, 185, 199, 213, 231, 243, 255, 273, 285, 297` (households, vital_statistics, labor_force, medical_welfare, education_facilities, geography)
- `src/handlers/analysis/fetch/subtab5_phase4_7.rs:51, 62, 76, 86, 100, 116, 126, 140, 152, 166, 175, 187, 198, 284, 300` (education, household, boj_tankan, social_life, land_price, car_ownership, internet_usage, industry_structure)
- `src/handlers/analysis/fetch/subtab5_phase4.rs:41, 148, 156, 215, 223, 230, 248, 256` (minimum_wage, prefecture_stats, population_pyramid, migration)
- `src/handlers/analysis/fetch/subtab7_other.rs:202, 232, 257, 262` (commute_od)
- `src/handlers/overview.rs:1058, 1060` (daytime_population)
- `src/handlers/company/fetch.rs:240` (prefecture_stats)
- `src/handlers/analysis/fetch/market_intelligence.rs:207, 1379` (commute_od, industry_structure)

**evidence**: `src/handlers/analysis/fetch/mod.rs:118` で定数定義済みだが、適用は `v2_external_population` / `v2_external_foreign_residents` のみ。

```rust
// subtab7_phase_a.rs:29 (ガード未適用例)
"SELECT ... FROM v2_external_households WHERE prefecture = ?1 AND municipality = ?2"
```

**影響**: もし e-Stat CSV インポート時にヘッダー行 (`'都道府県', '市区町村', ...`) が data 行として混入していれば、これら 18+ テーブルのクエリ結果に `prefecture='都道府県'` の偽行が混入し SUM/AVG が汚染される。過去事故 (V1 で `('都道府県', '市区町村', ...)` 混入) の再発可能性あり。

**推奨**: 全 `v2_external_*` SELECT に `EXTERNAL_CLEAN_FILTER` / `_NO_MUNI` を強制適用する linter ルール、または DB CHECK 制約 (`CHECK (prefecture <> '都道府県')`) を追加。

### P1-2: audit DAO のトランザクション境界欠落

**file:line**: `src/audit/dao.rs:85-104` (login flow), `:190-219` (session flow)

**evidence**: `turso.execute()` を連続発行する箇所が複数あるが、Turso HTTP API は statement 単位の autocommit (BEGIN/COMMIT なし)。

**影響**: login 中に UPDATE accounts 成功 → INSERT login_sessions 失敗、で integrity 不整合。`src/db/turso_http.rs:100` の `execute()` は単発 statement 用で transaction 概念なし。

**推奨**: Turso pipeline 機能で複数 statement を 1 リクエストに束ねるか、`BEGIN; ...; COMMIT;` を `execute_pipeline` で明示送出。

### P1-3: LIKE pattern 特殊文字エスケープ未実装

**file:line**: `src/handlers/company/fetch.rs:122`

**evidence**:
```rust
let like_pattern = format!("%{}%", query.trim());
// → company_name LIKE ?1
```

**問題**: ユーザー入力 `query` に `%` `_` `\` を含むと wildcard 解釈される。SQL injection ではないが (parameterized)、(a) 検索結果の意図せぬ拡大、(b) `_` で過剰一致による情報漏洩 / DoS (full-scan 誘発)。

**影響**: 中。POST `/company/search` 経由で誰でも実行可能。

**推奨**: 入力前にエスケープ:
```rust
let q = query.trim().replace('\\', "\\\\").replace('%', "\\%").replace('_', "\\_");
let like_pattern = format!("%{}%", q);
// SQL: ... LIKE ?1 ESCAPE '\\'
```

同パターンの可能性: `src/handlers/recruitment_diag/competitors.rs:263` (`address LIKE ?N`), `src/handlers/recruitment_diag/opportunity_map.rs:217` (`job_type LIKE ?N`) — bind 元 (`address`, `job_type`) が controlled value (allowlist) か再確認推奨。

### P1-4: SQLite に median 関数なし → ORDER BY OFFSET で代替 (パフォーマンス)

**file:line**: `src/handlers/recruitment_diag/condition_gap.rs:289-322`

**evidence**:
```rust
"SELECT {column} as v FROM postings WHERE {where_sql} ORDER BY {column} LIMIT 1 OFFSET {offset}"
```

`{column}` は allowlist で安全。だが大規模 `postings` (数百万行想定) に対し index なしの ORDER BY OFFSET N/2 は実用速度劣化リスクあり。median 計算 1 回ごとに full sort。

**影響**: condition_gap タブの中央値表示が遅延。

**推奨**: `salary_min`, `bonus_months`, `annual_holidays` に index を追加、または median を ETL 段階で事前計算。

---

## P2 (改善)

### P2-1: prepared statement 再利用なし

`LocalDb::query()` (`src/db/local_sqlite.rs:60`) は呼出ごとに `conn.prepare()`。同一 SQL を多数回発行する `competitive/analysis.rs` (5-6 連発), `workstyle.rs`, `demographics.rs` でも毎回 prepare。SQLite は statement cache を持つので影響は小だが、明示 LRU cache で再利用余地あり。

### P2-2: `query_scalar::<i64>()` の NULL 列ハンドリング

`compute_salary_percentile()` 等で `db.query_scalar::<i64>(&sql, &p).unwrap_or(0)` を多用 (`diagnostic.rs:738, 762, 798, 826` 等)。SQL エラーと「データなし (0 件)」が区別不能。`Option<i64>` での区別が望ましい。

### P2-3: 動的 WHERE 構築の DRY 違反

`build_filter_clause` (overview.rs:253), `build_region_filter` (region.rs:14), `build_hw_location_filter` (overview.rs:277), `competitive/fetch.rs:462`, `competitive/analysis.rs:44`, `condition_gap.rs:180-220` で類似の `prefecture/municipality/job_type/employment_type` フィルタ構築コードが 6 箇所重複。共通 builder へ集約推奨。

### P2-4: テスト用 INSERT は安全

`market_intelligence.rs:1668, 2076, 2142, 2554, 2584, 2608, 2638, 2647, 2698, 2735, 2785`, `contract_tests.rs:41,49,54`, `karte_audit_test.rs:57,90`, `global_contract_audit_test.rs:85,102,127,140,150` — すべて `#[cfg(test)]` ブロック内の in-memory `rusqlite::Connection` 向け。本番 Turso には到達しない。確認済み。

---

## OK 確認済み項目

| 観点 | 結果 |
|------|------|
| SQL injection 経路 | 全 24 件の `format!() + SELECT/WHERE` を確認、すべて `?N` バインド経由。実害なし |
| ORDER BY 動的列名 | `condition_gap.rs:297` で allowlist 明示。他箇所は静的列名 |
| 雇用形態の expand | `emp_classifier::from_ui_value` + `expand_to_db_values` でドメイン allowlist 経由 (`condition_gap.rs:183`) |
| ヘッダー混入ガード | `v2_external_population`, `v2_external_foreign_residents` には適用済 (`subtab5_phase4.rs`, `subtab5_phase4_7.rs:25,36`, `mod.rs:118-126`) |
| Turso 書込経路 (handlers) | `src/audit/dao.rs` のみ。他 handler 配下は SELECT のみ |
| Python ETL scripts | `scripts/upload_*.py` 等は user-run ETL。web 経路からは到達不能 |
| placeholder index 管理 | `build_industry_clause` (overview.rs:104-144) で `idx` を mutable に管理、prefecture/municipality と整合 |
