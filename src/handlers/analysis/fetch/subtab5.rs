//! サブタブ5（異常値・外部）系 fetch 関数
//! - Phase 2: A-1 異常値統計
//! - Phase 4: 外部データ統合（最賃マスタ・違反、地域ベンチマーク、都道府県統計、人口・社会動態・昼夜間人口、求人倍率、労働、事業所、入離職、家計消費、業況、気象、介護需要）
//! - Phase 4-7: 外国人在留・学歴・世帯・日銀短観・社会生活・地価・自動車・ネット・産業構造

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;
use super::{query_3level, query_turso_or_local};

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_anomaly_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, metric_name, total_count, anomaly_count, anomaly_rate, \
        avg_value, stddev_value, anomaly_high_count, anomaly_low_count";
    let nat = "emp_group, metric_name, SUM(total_count) as total_count, \
        SUM(anomaly_count) as anomaly_count, \
        CAST(SUM(anomaly_count) AS REAL) / SUM(total_count) as anomaly_rate, \
        AVG(avg_value) as avg_value, AVG(stddev_value) as stddev_value, \
        SUM(anomaly_high_count) as anomaly_high_count, SUM(anomaly_low_count) as anomaly_low_count";
    query_3level(
        db,
        "v2_anomaly_stats",
        pref,
        muni,
        cols,
        "ORDER BY emp_group, metric_name",
        nat,
        "GROUP BY emp_group, metric_name ORDER BY emp_group, metric_name",
    )
}

/// Phase 4-1: 最低賃金マスタ
pub(crate) fn fetch_minimum_wage(db: &Db, pref: &str) -> Vec<Row> {
    if !table_exists(db, "v2_external_minimum_wage") {
        return vec![];
    }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC"
                .to_string(),
            vec![],
        )
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// Phase 4-2: 最低賃金違反チェック
pub(crate) fn fetch_wage_compliance(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_hourly_postings, min_wage, below_min_count, below_min_rate, \
        avg_hourly_wage, median_hourly_wage";
    let nat = "emp_group, SUM(total_hourly_postings) as total_hourly_postings, \
        AVG(min_wage) as min_wage, SUM(below_min_count) as below_min_count, \
        CAST(SUM(below_min_count) AS REAL) / SUM(total_hourly_postings) as below_min_rate, \
        AVG(avg_hourly_wage) as avg_hourly_wage, AVG(median_hourly_wage) as median_hourly_wage";
    query_3level(
        db,
        "v2_wage_compliance",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}

/// Phase 4-3: 地域ベンチマーク（12軸レーダー用）
pub(crate) fn fetch_region_benchmark(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_competitiveness, job_market_tightness, wage_compliance, \
        industry_diversity, info_transparency, text_urgency, posting_freshness, \
        real_wage_power, labor_fluidity, working_age_ratio, population_growth, foreign_workforce, \
        composite_benchmark";
    let nat = "emp_group, \
        AVG(salary_competitiveness) as salary_competitiveness, \
        AVG(job_market_tightness) as job_market_tightness, \
        AVG(wage_compliance) as wage_compliance, \
        AVG(industry_diversity) as industry_diversity, \
        AVG(info_transparency) as info_transparency, \
        AVG(text_urgency) as text_urgency, \
        AVG(posting_freshness) as posting_freshness, \
        AVG(real_wage_power) as real_wage_power, \
        AVG(labor_fluidity) as labor_fluidity, \
        AVG(working_age_ratio) as working_age_ratio, \
        AVG(population_growth) as population_growth, \
        AVG(foreign_workforce) as foreign_workforce, \
        AVG(composite_benchmark) as composite_benchmark";
    query_3level(
        db,
        "v2_region_benchmark",
        pref,
        muni,
        cols,
        "ORDER BY emp_group",
        nat,
        "GROUP BY emp_group ORDER BY emp_group",
    )
}

/// Phase 4-4: 都道府県別外部指標マスタ（Turso優先）
pub(crate) fn fetch_prefecture_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats ORDER BY prefecture"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_prefecture_stats")
}

/// Phase B: 人口ピラミッドデータ（市区町村レベル、Turso優先）
pub(crate) fn fetch_population_data(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT prefecture, municipality, total_population, male_population, female_population, \
          age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate \
          FROM v2_external_population WHERE prefecture = ?1 AND municipality = ?2".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT ?1 as prefecture, '全体' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT '全国' as prefecture, '' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population")
}

/// Phase B: 人口ピラミッド詳細（5歳階級×男女、Turso優先）
pub(crate) fn fetch_population_pyramid(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let order_clause = "ORDER BY CASE age_group \
          WHEN '0-9' THEN 0 WHEN '10-19' THEN 10 WHEN '20-29' THEN 20 \
          WHEN '30-39' THEN 30 WHEN '40-49' THEN 40 WHEN '50-59' THEN 50 \
          WHEN '60-69' THEN 60 WHEN '70-79' THEN 70 WHEN '80+' THEN 80 \
          WHEN '0-14' THEN 0 WHEN '15-64' THEN 15 WHEN '65-74' THEN 65 WHEN '75+' THEN 75 \
          ELSE 999 END";
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            format!(
                "SELECT age_group, male_count, female_count \
          FROM v2_external_population_pyramid WHERE prefecture = ?1 AND municipality = ?2 \
          {order_clause}"
            ),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid WHERE prefecture = ?1 \
          GROUP BY age_group \
          {order_clause}"),
         vec![pref.to_string()])
    } else {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid \
          GROUP BY age_group \
          {order_clause}"),
         vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population_pyramid")
}

/// Phase B: 社会動態（転入転出、Turso優先）
pub(crate) fn fetch_migration_data(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT inflow, outflow, net_migration, net_migration_rate \
          FROM v2_external_migration WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        ("SELECT SUM(inflow) as inflow, SUM(outflow) as outflow, \
          SUM(net_migration) as net_migration, \
          CAST(SUM(net_migration) AS REAL) / NULLIF(SUM(inflow + outflow), 0) * 1000 as net_migration_rate \
          FROM v2_external_migration WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT SUM(inflow) as inflow, SUM(outflow) as outflow, \
          SUM(net_migration) as net_migration, \
          CAST(SUM(net_migration) AS REAL) / NULLIF(SUM(inflow + outflow), 0) * 1000 as net_migration_rate \
          FROM v2_external_migration".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_migration")
}

/// Phase B: 昼夜間人口（Turso優先）
pub(crate) fn fetch_daytime_population(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT nighttime_pop, daytime_pop, day_night_ratio, inflow_pop, outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_daytime_population")
}

/// Phase 4-5: 有効求人倍率の年度次推移（Turso優先）
pub(crate) fn fetch_job_openings_ratio(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_job_openings_ratio")
}

/// 労働市場指標の年度次推移（Turso優先）
pub(crate) fn fetch_labor_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_labor_stats")
}

/// 事業所数データ（都道府県別×産業分類、Turso優先）
pub(crate) fn fetch_establishments(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, industry_code as industry, industry_name, \
          SUM(establishments) as establishment_count, SUM(employees) as employees, \
          MAX(reference_year) as reference_year \
          FROM v2_external_establishments \
          WHERE prefecture = ?1 AND industry_code <> 'ALL' \
          GROUP BY prefecture, industry_code, industry_name \
          ORDER BY establishment_count DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, industry_code as industry, industry_name, \
          SUM(establishments) as establishment_count, SUM(employees) as employees, \
          MAX(reference_year) as reference_year \
          FROM v2_external_establishments \
          WHERE industry_code <> 'ALL' \
          GROUP BY industry_code, industry_name \
          ORDER BY establishment_count DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_establishments")
}

/// 入職率・離職率データ（都道府県別×産業、Turso優先）
pub(crate) fn fetch_turnover(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture IN ('全国', ?1) AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture = '全国' AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_turnover")
}

/// 家計消費支出データ（都道府県別×カテゴリ、Turso優先）
pub(crate) fn fetch_household_spending(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, category, monthly_amount, reference_year \
          FROM v2_external_household_spending \
          WHERE prefecture = ?1 \
          ORDER BY monthly_amount DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国選択時: 全47県の平均を計算
        (
            "SELECT '全国' as prefecture, category, \
          AVG(monthly_amount) as monthly_amount, MAX(reference_year) as reference_year \
          FROM v2_external_household_spending \
          GROUP BY category \
          ORDER BY monthly_amount DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_household_spending")
}

// ======== Phase 4-6（事業所動態・気象・介護需要） ========

/// 事業所動態データ（開業率・廃業率、Turso優先）
pub(crate) fn fetch_business_dynamics(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, opening_rate, closure_rate, \
          new_establishments, closed_establishments, net_change \
          FROM v2_external_business_dynamics \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国: 全都道府県の合計から算出
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          AVG(opening_rate) as opening_rate, AVG(closure_rate) as closure_rate, \
          SUM(new_establishments) as new_establishments, \
          SUM(closed_establishments) as closed_establishments, \
          SUM(net_change) as net_change \
          FROM v2_external_business_dynamics \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_business_dynamics")
}

/// 気象データ（都道府県別、Turso優先）
pub(crate) fn fetch_climate(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, avg_temperature, max_temperature, \
          min_temperature, snow_days, sunshine_hours, precipitation \
          FROM v2_external_climate \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          AVG(avg_temperature) as avg_temperature, \
          MAX(max_temperature) as max_temperature, \
          MIN(min_temperature) as min_temperature, \
          AVG(snow_days) as snow_days, \
          AVG(sunshine_hours) as sunshine_hours, \
          AVG(precipitation) as precipitation \
          FROM v2_external_climate \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_climate")
}

/// 介護需要データ（給付件数・施設数等、Turso優先）
pub(crate) fn fetch_care_demand(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, insurance_benefit_cases, \
          nursing_home_count, health_facility_count, \
          home_care_offices, day_service_offices, \
          pop_65_over, pop_75_over, pop_65_over_rate \
          FROM v2_external_care_demand \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          SUM(insurance_benefit_cases) as insurance_benefit_cases, \
          SUM(nursing_home_count) as nursing_home_count, \
          SUM(health_facility_count) as health_facility_count, \
          SUM(home_care_offices) as home_care_offices, \
          SUM(day_service_offices) as day_service_offices, \
          SUM(pop_65_over) as pop_65_over, SUM(pop_75_over) as pop_75_over, \
          AVG(pop_65_over_rate) as pop_65_over_rate \
          FROM v2_external_care_demand \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_care_demand")
}

// ======== Phase 4-7（外部統計） ========

/// 外国人在留資格別データ（Turso優先）
pub(crate) fn fetch_foreign_residents(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, visa_status, count, survey_period \
          FROM v2_external_foreign_residents \
          WHERE prefecture = ?1 \
          ORDER BY count DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, visa_status, \
          SUM(count) as count, MAX(survey_period) as survey_period \
          FROM v2_external_foreign_residents \
          GROUP BY visa_status \
          ORDER BY count DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_foreign_residents")
}

/// 学歴分布データ（Turso優先）
pub(crate) fn fetch_education(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, education_level, male_count, female_count, total_count \
          FROM v2_external_education \
          WHERE prefecture = ?1 \
          ORDER BY total_count DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, education_level, \
          SUM(male_count) as male_count, SUM(female_count) as female_count, \
          SUM(total_count) as total_count \
          FROM v2_external_education \
          GROUP BY education_level \
          ORDER BY total_count DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_education")
}

/// 世帯構成データ（Turso優先）
pub(crate) fn fetch_household_type(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, household_type, count, ratio \
          FROM v2_external_household \
          WHERE prefecture = ?1 \
          ORDER BY count DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, household_type, \
          SUM(count) as count, AVG(ratio) as ratio \
          FROM v2_external_household \
          GROUP BY household_type \
          ORDER BY count DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_household")
}

/// 日銀短観DI（全国データ、都道府県フィルタなし、Turso優先）
pub(crate) fn fetch_boj_tankan(db: &Db, turso: Option<&TursoDb>) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = (
        "SELECT survey_date, industry_code, industry_j, enterprise_size, \
          di_type, result_type, di_value \
          FROM v2_external_boj_tankan \
          WHERE result_type = 'actual' \
          AND (industry_j LIKE '%製造業%' OR industry_j LIKE '%非製造業%') \
          AND di_type IN ('business', 'employment') \
          ORDER BY survey_date DESC, industry_j \
          LIMIT 400"
            .to_string(),
        vec![],
    );
    query_turso_or_local(turso, db, &sql, &params, "v2_external_boj_tankan")
}

/// 社会生活基本調査（サイコグラフィック、Turso優先）
pub(crate) fn fetch_social_life(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, category, subcategory, participation_rate, survey_year \
          FROM v2_external_social_life \
          WHERE prefecture = ?1 \
          ORDER BY category"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, category, subcategory, \
          AVG(participation_rate) as participation_rate, MAX(survey_year) as survey_year \
          FROM v2_external_social_life \
          GROUP BY category, subcategory \
          ORDER BY category"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_social_life")
}

/// 地価公示データ（Turso優先）
pub(crate) fn fetch_land_price(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, land_use, avg_price_per_sqm, yoy_change_pct, year, point_count \
          FROM v2_external_land_price \
          WHERE prefecture = ?1 \
          ORDER BY land_use"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, land_use, \
          AVG(avg_price_per_sqm) as avg_price_per_sqm, \
          AVG(yoy_change_pct) as yoy_change_pct, \
          MAX(year) as year, SUM(point_count) as point_count \
          FROM v2_external_land_price \
          GROUP BY land_use \
          ORDER BY land_use"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_land_price")
}

/// 自動車保有率データ（Turso優先）
pub(crate) fn fetch_car_ownership(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, cars_per_100people, year \
          FROM v2_external_car_ownership \
          WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, \
          AVG(cars_per_100people) as cars_per_100people, MAX(year) as year \
          FROM v2_external_car_ownership"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_car_ownership")
}

/// ネット利用率データ（Turso優先）
pub(crate) fn fetch_internet_usage(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, internet_usage_rate, smartphone_ownership_rate, year \
          FROM v2_external_internet_usage \
          WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, \
          AVG(internet_usage_rate) as internet_usage_rate, \
          AVG(smartphone_ownership_rate) as smartphone_ownership_rate, \
          MAX(year) as year \
          FROM v2_external_internet_usage"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_internet_usage")
}

/// 産業構造データ（経済センサス、都道府県コードで集約、Turso優先）
pub(crate) fn fetch_industry_structure(
    db: &Db,
    turso: Option<&TursoDb>,
    prefecture_code: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !prefecture_code.is_empty() {
        (
            "SELECT industry_code, industry_name, \
          SUM(establishments) as establishments, \
          SUM(employees_total) as employees_total, \
          SUM(employees_male) as employees_male, \
          SUM(employees_female) as employees_female \
          FROM v2_external_industry_structure \
          WHERE prefecture_code = ?1 \
          AND industry_code NOT IN ('AS', 'AR', 'CR') \
          GROUP BY industry_code, industry_name \
          ORDER BY employees_total DESC"
                .to_string(),
            vec![prefecture_code.to_string()],
        )
    } else {
        // 全国: 全都道府県の合計を産業別に集約
        (
            "SELECT industry_code, industry_name, \
          SUM(establishments) as establishments, \
          SUM(employees_total) as employees_total, \
          SUM(employees_male) as employees_male, \
          SUM(employees_female) as employees_female \
          FROM v2_external_industry_structure \
          WHERE industry_code NOT IN ('AS', 'AR', 'CR') \
          GROUP BY industry_code, industry_name \
          ORDER BY employees_total DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_industry_structure")
}
