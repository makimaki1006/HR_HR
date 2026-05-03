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

CREATE TABLE IF NOT EXISTS municipality_occupation_population (
    municipality_code TEXT NOT NULL,
    prefecture TEXT NOT NULL,
    municipality_name TEXT NOT NULL,
    basis TEXT NOT NULL, -- resident | workplace
    occupation_code TEXT NOT NULL,
    occupation_name TEXT NOT NULL,
    age_group TEXT NOT NULL,
    gender TEXT NOT NULL, -- male | female | total
    population INTEGER NOT NULL DEFAULT 0,
    source_year INTEGER NOT NULL,
    source_name TEXT NOT NULL DEFAULT 'census',
    created_at TEXT NOT NULL DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (
        municipality_code,
        basis,
        occupation_code,
        age_group,
        gender,
        source_year
    )
);

CREATE INDEX IF NOT EXISTS idx_mop_muni_basis
ON municipality_occupation_population (municipality_code, basis);

CREATE INDEX IF NOT EXISTS idx_mop_occupation_age_gender
ON municipality_occupation_population (occupation_code, age_group, gender);


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
