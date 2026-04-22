# Flow CTAS 戻し手順（2026-05-01 Turso 無料枠リセット後）

## 背景

2026-04 時点で Turso 無料枠書き込み上限超過により、以下の CTAS テーブルが未作成：

- `v2_flow_city_agg`（市区町村×年×月×dayflag×timezone 集計）
- `v2_flow_mesh3km_agg`（3km メッシュ集計、population SUM）

暫定措置として `src/handlers/jobmap/flow.rs` および
`src/handlers/insight/flow_context.rs` の該当クエリを `v2_flow_mesh1km_YYYY`
生テーブルからの `GROUP BY` 動的集計に差し替えている。
各 FALLBACK 箇所には `// FALLBACK: GROUP BY, replace with CTAS after May 1`
コメントを付与済み。

## CTAS 投入手順（サイト側）

Python 側 ETL で以下を投入する（書き込み枠回復後）：

1. `v2_flow_city_agg`
   - カラム: `citycode INTEGER, year INTEGER, month TEXT, dayflag INTEGER, timezone INTEGER, pop_sum REAL, mesh_count INTEGER`
   - ソース: `v2_flow_mesh1km_2019/2020/2021` の `GROUP BY citycode, year, month, dayflag, timezone`
   - インデックス: `(citycode, year, month, dayflag, timezone)`
2. `v2_flow_mesh3km_agg`
   - カラム: `mesh3kmid_approx INTEGER, year INTEGER, month TEXT, dayflag INTEGER, timezone INTEGER, pop_sum REAL`
   - ソース: `SELECT (mesh1kmid/1000) AS mesh3kmid_approx, ..., SUM(population) AS pop_sum GROUP BY ...`
   - インデックス: `(mesh3kmid_approx, year, month)`

## Rust コード側の戻し手順

### 対象ファイル

- `src/handlers/jobmap/flow.rs`
- `src/handlers/insight/flow_context.rs`

### 戻し方針

各 FALLBACK コメントの直下 SQL を、以下パターンで置換する：

#### `get_city_agg`

```sql
-- 戻し後（CTAS 利用、簡潔）
SELECT citycode, year, month, dayflag, timezone, pop_sum, mesh_count
FROM v2_flow_city_agg
WHERE year = ?1 AND month = ?2 AND dayflag = ?3 AND timezone = ?4
ORDER BY citycode
```

参照: Git 履歴 `git log -p src/handlers/jobmap/flow.rs` で 2026-04-22 以前の
`get_city_agg` 実装を復元可能。`has_flow_table(db, "v2_flow_city_agg")`
早期 return も併せて復活させる。

#### `get_mesh3km_heatmap`

```sql
SELECT mesh3kmid_approx, year, month, dayflag, timezone, pop_sum
FROM v2_flow_mesh3km_agg
WHERE mesh3kmid_approx BETWEEN ?1 AND ?2 AND year = ?3 AND month = ?4
ORDER BY mesh3kmid_approx
```

#### `get_karte_profile` / `get_karte_monthly_trend` / `get_karte_daynight_ratio`

旧 CTAS ベース SQL（Git 2026-04-22 以前）に戻す。`has_flow_table(db, "v2_flow_city_agg")`
の早期 return を復活。

#### `flow_context::calc_ratio_from_profile` / `calc_covid_recovery`

同上。`v2_flow_city_agg` 直接クエリに戻す。
`build_flow_context` 先頭の存在チェックも `table_exists(db, "v2_flow_city_agg")`
のシンプルな形式に戻す。

### 手順

1. CTAS 投入完了を `SELECT COUNT(*) FROM v2_flow_city_agg` / `v2_flow_mesh3km_agg`
   で Turso 上確認（想定規模：city_agg 数十万行、mesh3km_agg 数百万行）
2. `grep -rn "FALLBACK: GROUP BY, replace with CTAS" src/` で全 FALLBACK 箇所列挙
3. 各箇所を CTAS ベース SQL に置換
4. `cargo test --lib --release handlers::jobmap` / `handlers::insight::flow_context`
   で既存テスト全通過確認
5. 手動 API 検証（`/api/flow/city_agg?year=2019&month=9&dayflag=1&timezone=0` 等）で
   FALLBACK と CTAS の結果総和が概ね一致することを確認
   （`SUM(pop_sum)` レベルで一致、mesh_count は CTAS の方が正確）
6. `flow.rs` 冒頭の「CTAS 未作成期間の FALLBACK 実装（2026-04-22）」コメント削除

## 逆証明用検証 SQL（CTAS 投入直後）

CTAS と FALLBACK が等価であることを確認するため、以下を Turso で実行：

```sql
-- 総和一致確認
SELECT
  (SELECT SUM(pop_sum) FROM v2_flow_city_agg
   WHERE year = 2019 AND month = '09' AND dayflag = 1 AND timezone = 0) AS ctas_sum,
  (SELECT SUM(population) FROM v2_flow_mesh1km_2019
   WHERE month = '09' AND dayflag = 1 AND timezone = 0) AS raw_sum;
-- 期待: ctas_sum = raw_sum
```

```sql
-- 特定 citycode の pop_sum 一致確認
SELECT
  (SELECT pop_sum FROM v2_flow_city_agg
   WHERE citycode = 13101 AND year = 2019 AND month = '09'
     AND dayflag = 1 AND timezone = 0) AS ctas_val,
  (SELECT SUM(population) FROM v2_flow_mesh1km_2019
   WHERE citycode = 13101 AND month = '09' AND dayflag = 1 AND timezone = 0) AS raw_val;
-- 期待: ctas_val = raw_val
```

差分がある場合は CTAS 投入 SQL のフィルタ条件（特に dayflag/timezone 集計値の扱い）
を疑い、投入 SQL を `GROUP BY` で生値のみ扱う形に修正する。

## double count 防御（常時）

`AggregateMode::Raw` 以外を使う場合、dayflag=2/timezone=2 を含むデータと
IN(0,1) のデータを**同時に SUM してはいけない**。CTAS 戻し時も
`AggregateMode::where_clause()` を必ず経由させる方針は維持すること。
