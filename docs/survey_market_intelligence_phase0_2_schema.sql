-- 媒体分析レポート拡張 Phase 0〜2 DDL案
-- 作成日: 2026-05-03
--
-- 目的:
--   レポート生成時に重い集計を行わず、市区町村単位の採用マーケット分析を
--   読み取り中心で実行するための事前集計テーブル案。
--
-- 方針:
--   - municipality_code を主キー系の結合キーにする。
--   - 表示用に prefecture / municipality_name も保持する。
--   - 実測、推定、参考指標を混在させるため、source_type / source_year / computed_at を持たせる。
--   - SQLite/Turso 互換を優先し、複雑な型や制約に依存しない。

-- ============================================================
-- DIFF NOTE (Plan B revision, 2026-05-04):
-- - municipality_occupation_population: complete rewrite
--   (population NULL-allowed, estimate_index added, data_label CHECK,
--    PRIMARY KEY + data_label, value XOR CHECK, indices doubled,
--    age_group renamed to age_class, weight_source / estimated_at added)
-- See: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md
-- ============================================================

-- ============================================================
-- municipality_occupation_population (Plan B: parallel storage)
-- ============================================================
-- Stores both measured (e-Stat 15-1, sid=0003454508) and estimated (Model F2)
-- occupation populations at municipality grain.
--
-- Axis combinations:
--   workplace × measured       : e-Stat 15-1 actual headcounts
--   workplace × estimated_beta : F2 fallback when 15-1 missing (rare)
--   resident  × estimated_beta : F2 estimation (15-1 has no resident grain)
--   resident  × measured       : reserved for future e-Stat resident actuals
--
-- Source documentation:
--   docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md (Worker B4)
--   docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_ESTAT_15_1_FEASIBILITY.md (Worker D3)
--   docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_F2_ROLE_AFTER_15_1.md (Worker D4)

CREATE TABLE IF NOT EXISTS municipality_occupation_population (
    -- 結合キー
    municipality_code TEXT NOT NULL,
    prefecture        TEXT NOT NULL,
    municipality_name TEXT NOT NULL,

    -- 軸
    basis TEXT NOT NULL CHECK (basis IN ('workplace','resident')),
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    age_class TEXT NOT NULL,                    -- '15-19'..'85+', '_total'
    gender TEXT NOT NULL CHECK (gender IN ('male','female','total')),

    -- 値 (XOR via CHECK below)
    population     INTEGER,                      -- measured: rows have value, estimated: NULL
    estimate_index REAL,                          -- estimated_beta: 0-200, measured: NULL

    -- メタデータ
    data_label    TEXT NOT NULL CHECK (data_label IN ('measured','estimated_beta')),
    source_name   TEXT NOT NULL,                  -- 'census_15_1' / 'model_f2_v1'
    source_year   INTEGER NOT NULL,
    weight_source TEXT,                            -- estimated only: 'hypothesis_v1' / 'estat_R2_xxx'

    -- 鮮度
    estimated_at TEXT NOT NULL DEFAULT (datetime('now')),

    PRIMARY KEY (municipality_code, basis, occupation_code, age_class, gender, source_year, data_label),

    -- value/label 整合性 (XOR)
    CHECK (
      (data_label = 'measured'        AND population IS NOT NULL AND estimate_index IS NULL) OR
      (data_label = 'estimated_beta'  AND population IS NULL     AND estimate_index IS NOT NULL)
    )
);

CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_pref
    ON municipality_occupation_population (prefecture, municipality_name);
CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_basis
    ON municipality_occupation_population (basis, occupation_code);
CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_label
    ON municipality_occupation_population (data_label);
CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_source
    ON municipality_occupation_population (source_name, source_year);
CREATE INDEX IF NOT EXISTS idx_muni_occ_pop_age
    ON municipality_occupation_population (age_class);


CREATE TABLE IF NOT EXISTS municipality_living_cost_proxy (
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    single_household_rent_proxy INTEGER, -- 円/月。公式統計から作る「単身向け相当」家賃proxy
    small_household_rent_proxy INTEGER, -- 円/月。公式統計から作る「小世帯向け相当」家賃proxy
    rent_per_square_meter REAL, -- 円/㎡
    retail_price_index_proxy REAL, -- 100を基準値とする指数
    household_spending_annual_yen INTEGER, -- 円/年
    land_price_residential_per_sqm REAL, -- 円/㎡
    housing_cost_rank INTEGER, -- 全国順位。1=住居コストが低い
    rent_source_year INTEGER, -- 住宅・土地統計など家賃proxyの参照年
    price_source_period TEXT, -- 小売物価など月次補正の参照期間。例: 2026-04
    land_price_source_year INTEGER, -- 地価系データの参照年
    source_year INTEGER NOT NULL, -- レコード代表年。互換用
    source_type TEXT NOT NULL DEFAULT 'official_proxy',
    note TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    updated_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (municipality_code, source_year)
);

CREATE INDEX IF NOT EXISTS idx_living_cost_pref_muni
ON municipality_living_cost_proxy (prefecture, municipality_name);


CREATE TABLE IF NOT EXISTS commute_flow_summary (
    destination_municipality_code TEXT NOT NULL,
    destination_prefecture TEXT NOT NULL,
    destination_municipality_name TEXT NOT NULL,
    origin_municipality_code TEXT NOT NULL,
    origin_prefecture TEXT NOT NULL,
    origin_municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    occupation_group_name TEXT NOT NULL DEFAULT '全職業',
    flow_count INTEGER NOT NULL DEFAULT 0,
    flow_share REAL, -- 0.0〜1.0 の比率
    target_origin_population INTEGER, -- 人
    estimated_target_flow_conservative INTEGER,
    estimated_target_flow_standard INTEGER,
    estimated_target_flow_aggressive INTEGER,
    estimation_method TEXT, -- 推定方法メモ。例: od_share_x_origin_target_population
    estimated_at TEXT, -- 推定実行日時
    rank_to_destination INTEGER NOT NULL,
    source_year INTEGER NOT NULL,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        destination_municipality_code,
        origin_municipality_code,
        occupation_group_code,
        source_year
    )
);

CREATE INDEX IF NOT EXISTS idx_commute_dest_rank
ON commute_flow_summary (
    destination_municipality_code,
    occupation_group_code,
    rank_to_destination
);

CREATE INDEX IF NOT EXISTS idx_commute_origin
ON commute_flow_summary (origin_municipality_code);


CREATE TABLE IF NOT EXISTS municipality_recruiting_scores (
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL,
    occupation_group_name TEXT NOT NULL,
    target_population INTEGER NOT NULL DEFAULT 0,
    adjacent_population INTEGER NOT NULL DEFAULT 0,
    media_job_count INTEGER NOT NULL DEFAULT 0, -- 件
    competitor_job_count INTEGER NOT NULL DEFAULT 0, -- 件
    median_salary_yen INTEGER, -- 円/月。媒体給与中央値
    effective_wage_index REAL, -- 0〜100 指数
    commute_reach_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数
    job_competition_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数。高いほど競合が強い
    establishment_competition_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数。高いほど競合事業所が多い
    wage_competitiveness_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数。高いほど給与競争力が高い
    living_cost_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数。高いほど生活コスト面で有利
    effective_wage_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数
    distribution_priority_score REAL NOT NULL DEFAULT 0, -- 0〜100 指数
    scenario_conservative_population INTEGER NOT NULL DEFAULT 0,
    scenario_standard_population INTEGER NOT NULL DEFAULT 0,
    scenario_aggressive_population INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL,
    computed_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        municipality_code,
        occupation_group_code,
        source_year
    )
);

CREATE INDEX IF NOT EXISTS idx_recruiting_scores_priority
ON municipality_recruiting_scores (
    occupation_group_code,
    distribution_priority_score DESC
);

CREATE INDEX IF NOT EXISTS idx_recruiting_scores_pref
ON municipality_recruiting_scores (prefecture, municipality_name);


CREATE TABLE IF NOT EXISTS media_area_performance_future (
    campaign_id TEXT NOT NULL,
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    occupation_group_code TEXT NOT NULL DEFAULT 'all',
    impressions INTEGER NOT NULL DEFAULT 0,
    clicks INTEGER NOT NULL DEFAULT 0,
    applications INTEGER NOT NULL DEFAULT 0,
    interviews INTEGER NOT NULL DEFAULT 0,
    hires INTEGER NOT NULL DEFAULT 0,
    spend_yen INTEGER NOT NULL DEFAULT 0,
    ctr REAL,
    cvr REAL,
    cpa_yen REAL,
    cost_per_hire_yen REAL,
    measured_from TEXT,
    measured_to TEXT,
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (campaign_id, municipality_code, occupation_group_code)
);

CREATE INDEX IF NOT EXISTS idx_media_area_performance_muni
ON media_area_performance_future (municipality_code);


-- ============================================================
-- DIFF NOTE (Plan B revision, 2026-05-04):
-- - v2_municipality_target_thickness: NEW (Worker A2 design)
--   Derived aggregation for "target thickness" UI dashboard.
-- See: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md
--      docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_TABLE_RENAME_DECISION.md (Worker A2)
-- ============================================================

-- ============================================================
-- v2_municipality_target_thickness (Worker A2 / Plan B derived)
-- ============================================================
-- Derived aggregation from municipality_occupation_population for the
-- "target thickness" UI dashboard. Populated by ETL after both 15-1 and
-- F2 ingest are complete.

CREATE TABLE IF NOT EXISTS v2_municipality_target_thickness (
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis TEXT NOT NULL,
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    -- 指数
    thickness_index REAL NOT NULL,
    rank_in_occupation INTEGER,
    rank_percentile REAL,
    distribution_priority TEXT,
    -- シナリオ
    scenario_conservative_index INTEGER,
    scenario_standard_index INTEGER,
    scenario_aggressive_index INTEGER,
    -- メタ
    estimate_grade TEXT,
    weight_source TEXT NOT NULL,
    is_industrial_anchor INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL,
    estimated_at TEXT NOT NULL,
    PRIMARY KEY (municipality_code, basis, occupation_code, source_year)
);

CREATE INDEX IF NOT EXISTS idx_v2_muni_target_thick_idx
    ON v2_municipality_target_thickness (occupation_code, thickness_index DESC);
CREATE INDEX IF NOT EXISTS idx_v2_muni_target_rank
    ON v2_municipality_target_thickness (occupation_code, rank_in_occupation);
CREATE INDEX IF NOT EXISTS idx_v2_muni_target_pref
    ON v2_municipality_target_thickness (prefecture, occupation_code);


-- ============================================================
-- DIFF NOTE (Plan B revision, 2026-05-04):
-- - occupation_industry_weight: NEW (composite PK with weight_source)
--   Industry-to-occupation weight master used by Model F2 (Worker C3).
-- See: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_DDL_PLAN_B_PARALLEL.md
-- ============================================================

-- ============================================================
-- occupation_industry_weight (Worker B 重みマスタ + Plan B 拡張)
-- ============================================================

CREATE TABLE IF NOT EXISTS occupation_industry_weight (
    industry_code TEXT NOT NULL,
    industry_name TEXT NOT NULL,
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    weight REAL NOT NULL CHECK (weight >= 0.0 AND weight <= 1.0),
    weight_source TEXT NOT NULL DEFAULT 'hypothesis_v1',
    note TEXT,
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    PRIMARY KEY (industry_code, occupation_code, weight_source)
);

CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_industry
    ON occupation_industry_weight (industry_code);
CREATE INDEX IF NOT EXISTS idx_occ_industry_weight_occupation
    ON occupation_industry_weight (occupation_code);
