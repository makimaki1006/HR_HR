-- =============================================================
-- Agoop 集計テーブル CTAS (2026-05-01 戻し版、INSERT 分割方式)
--
-- ## 設計仕様 (`docs/flow_ctas_restore.md` 準拠)
--
-- - 年別 mesh1km テーブル `v2_flow_mesh1km_2019/2020/2021` を 各々 INSERT で集約
--   (UNION ALL 一括ではなく分割。1 statement あたり 300s timeout に余裕を持たせる)
-- - mesh3km は `mesh1kmid / 1000` で 3km 近似 (カラム名: `mesh3kmid_approx`)
-- - month は TEXT 型 ('01'..'12') で保存し Rust 側 `format!("{:02}", month)` の WHERE と一致
-- - 全 dayflag/timezone (0/1/2) を含む。double count 防御は呼び出し側 `mode.where_clause()` で個別実施
--
-- ## 重複防止
--
-- 各 CREATE TABLE の前に `DROP TABLE IF EXISTS` を必ず実行。
-- 投入失敗時の再実行でも重複しない冪等設計。
-- INSERT は GROUP BY で集約後に投入されるため、source 単位の重複も発生しない。
--
-- ## Rust 側コード対応
--
-- - `src/handlers/jobmap/flow.rs::get_city_agg`/`get_mesh3km_heatmap`/`get_karte_*`
-- - `src/handlers/insight/flow_context.rs::calc_ratio_from_profile`/`calc_covid_recovery`
--
-- ## 実行手順
--
-- ```bash
-- python scripts/upload_agoop_to_turso.py --ctas-only
-- ```
--
-- ## 想定処理時間 (各 statement)
--
-- - DROP / CREATE TABLE / CREATE INDEX: 1 秒以内
-- - INSERT 1 年分 (約 13M source 行 → 約 200K agg 行): 30-90 秒
--   - city_agg: idx_mesh1km_YYYY_city(citycode, month, dayflag, timezone) で index-driven GROUP BY
--   - mesh3km_agg: 計算列 (mesh1kmid/1000) のため full scan、約 60-150 秒
-- - 合計想定: 5-12 分 (300s timeout に各 statement で余裕)
--
-- ## ADR-006: dayflag/timezone double count 防御方針
--
-- mesh1km source 側で dayflag=2 (全日合算) や timezone=2 (終日合算) は既に集計値として存在。
-- CTAS でこれらをそのまま継承し、SUM(pop) 取得時に呼び出し側 (Rust) で
-- WHERE dayflag IN (0,1) AND timezone IN (0,1) (Raw mode) または
-- WHERE dayflag = 2 AND timezone IN (0,1) (DayAgg mode) 等で個別に絞ること。
-- =============================================================

-- ─────────────────────────────────────────────────────────────
-- v2_flow_city_agg (市区町村×年月×平休日×時間帯)
-- ─────────────────────────────────────────────────────────────

DROP TABLE IF EXISTS v2_flow_city_agg;

CREATE TABLE v2_flow_city_agg (
    citycode   INTEGER NOT NULL,
    year       INTEGER NOT NULL,
    month      TEXT    NOT NULL,
    dayflag    INTEGER NOT NULL,
    timezone   INTEGER NOT NULL,
    pop_sum    REAL    NOT NULL,
    mesh_count INTEGER NOT NULL,
    PRIMARY KEY (citycode, year, month, dayflag, timezone)
) WITHOUT ROWID;

INSERT INTO v2_flow_city_agg (citycode, year, month, dayflag, timezone, pop_sum, mesh_count)
  SELECT citycode, 2019 AS year, printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum,
         COUNT(*)        AS mesh_count
    FROM v2_flow_mesh1km_2019
   GROUP BY citycode, month, dayflag, timezone;

INSERT INTO v2_flow_city_agg (citycode, year, month, dayflag, timezone, pop_sum, mesh_count)
  SELECT citycode, 2020 AS year, printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum,
         COUNT(*)        AS mesh_count
    FROM v2_flow_mesh1km_2020
   GROUP BY citycode, month, dayflag, timezone;

INSERT INTO v2_flow_city_agg (citycode, year, month, dayflag, timezone, pop_sum, mesh_count)
  SELECT citycode, 2021 AS year, printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum,
         COUNT(*)        AS mesh_count
    FROM v2_flow_mesh1km_2021
   GROUP BY citycode, month, dayflag, timezone;

-- PRIMARY KEY が暗黙インデックスを作るため idx_city_agg_lookup は不要 (重複防止)。
-- (citycode, year, month, dayflag, timezone) ルックアップは PK で十分高速。


-- ─────────────────────────────────────────────────────────────
-- v2_flow_mesh3km_agg (3km メッシュ集約、z=10-12 用)
-- mesh3kmid_approx = mesh1kmid / 1000 (1次メッシュ + 2次メッシュ上 2 桁)
-- ─────────────────────────────────────────────────────────────

DROP TABLE IF EXISTS v2_flow_mesh3km_agg;

CREATE TABLE v2_flow_mesh3km_agg (
    mesh3kmid_approx INTEGER NOT NULL,
    year             INTEGER NOT NULL,
    month            TEXT    NOT NULL,
    dayflag          INTEGER NOT NULL,
    timezone         INTEGER NOT NULL,
    pop_sum          REAL    NOT NULL,
    PRIMARY KEY (mesh3kmid_approx, year, month, dayflag, timezone)
) WITHOUT ROWID;

INSERT INTO v2_flow_mesh3km_agg (mesh3kmid_approx, year, month, dayflag, timezone, pop_sum)
  SELECT (mesh1kmid / 1000) AS mesh3kmid_approx,
         2019 AS year,
         printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum
    FROM v2_flow_mesh1km_2019
   GROUP BY mesh3kmid_approx, month, dayflag, timezone;

INSERT INTO v2_flow_mesh3km_agg (mesh3kmid_approx, year, month, dayflag, timezone, pop_sum)
  SELECT (mesh1kmid / 1000) AS mesh3kmid_approx,
         2020 AS year,
         printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum
    FROM v2_flow_mesh1km_2020
   GROUP BY mesh3kmid_approx, month, dayflag, timezone;

INSERT INTO v2_flow_mesh3km_agg (mesh3kmid_approx, year, month, dayflag, timezone, pop_sum)
  SELECT (mesh1kmid / 1000) AS mesh3kmid_approx,
         2021 AS year,
         printf('%02d', month) AS month,
         dayflag, timezone,
         SUM(population) AS pop_sum
    FROM v2_flow_mesh1km_2021
   GROUP BY mesh3kmid_approx, month, dayflag, timezone;

-- PRIMARY KEY が暗黙インデックスを作るため、追加 INDEX 不要。
