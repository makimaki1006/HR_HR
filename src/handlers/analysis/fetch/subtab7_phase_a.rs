//! サブタブ7 Phase A: SSDSE-A 6テーブル（世帯・自然動態・労働力・医療福祉・教育施設・地理）

use serde_json::Value;
use std::collections::HashMap;

#[allow(unused_imports)]
use super::super::super::helpers::table_exists;
use super::query_turso_or_local;

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
#[allow(dead_code)]
type Row = HashMap<String, Value>;

pub(crate) fn fetch_households(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, total_households, general_households, \
             general_household_members, nuclear_family_households, single_households, \
             elderly_nuclear_households, elderly_couple_households, elderly_single_households, \
             avg_household_size, single_rate, elderly_single_rate, reference_date \
             FROM v2_external_households WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(total_households) as total_households, \
             SUM(general_households) as general_households, \
             SUM(general_household_members) as general_household_members, \
             SUM(nuclear_family_households) as nuclear_family_households, \
             SUM(single_households) as single_households, \
             SUM(elderly_nuclear_households) as elderly_nuclear_households, \
             SUM(elderly_couple_households) as elderly_couple_households, \
             SUM(elderly_single_households) as elderly_single_households, \
             CAST(SUM(general_household_members) AS REAL) / NULLIF(SUM(general_households), 0) as avg_household_size, \
             CAST(SUM(single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 as single_rate, \
             CAST(SUM(elderly_single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 as elderly_single_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_households WHERE prefecture = ?1".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(total_households) as total_households, \
             SUM(general_households) as general_households, \
             SUM(general_household_members) as general_household_members, \
             SUM(nuclear_family_households) as nuclear_family_households, \
             SUM(single_households) as single_households, \
             SUM(elderly_nuclear_households) as elderly_nuclear_households, \
             SUM(elderly_couple_households) as elderly_couple_households, \
             SUM(elderly_single_households) as elderly_single_households, \
             CAST(SUM(general_household_members) AS REAL) / NULLIF(SUM(general_households), 0) as avg_household_size, \
             CAST(SUM(single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 as single_rate, \
             CAST(SUM(elderly_single_households) AS REAL) / NULLIF(SUM(total_households), 0) * 100 as elderly_single_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_households".to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_households")
}

pub(crate) fn fetch_vital_statistics(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, births, deaths, natural_change, marriages, divorces, \
             birth_rate_permille, death_rate_permille, marriage_rate_permille, divorce_rate_permille, \
             reference_year \
             FROM v2_external_vital_statistics WHERE prefecture = ?1 AND municipality = ?2".to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(births) as births, SUM(deaths) as deaths, \
             SUM(births) - SUM(deaths) as natural_change, \
             SUM(marriages) as marriages, SUM(divorces) as divorces, \
             NULL as birth_rate_permille, NULL as death_rate_permille, \
             NULL as marriage_rate_permille, NULL as divorce_rate_permille, \
             MAX(reference_year) as reference_year \
             FROM v2_external_vital_statistics WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(births) as births, SUM(deaths) as deaths, \
             SUM(births) - SUM(deaths) as natural_change, \
             SUM(marriages) as marriages, SUM(divorces) as divorces, \
             NULL as birth_rate_permille, NULL as death_rate_permille, \
             NULL as marriage_rate_permille, NULL as divorce_rate_permille, \
             MAX(reference_year) as reference_year \
             FROM v2_external_vital_statistics"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_vital_statistics")
}

pub(crate) fn fetch_labor_force(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, employed, employed_male, employed_female, \
             unemployed, unemployed_male, unemployed_female, not_in_labor_force, \
             not_in_labor_force_male, not_in_labor_force_female, \
             primary_industry_employed, secondary_industry_employed, tertiary_industry_employed, \
             unemployment_rate, labor_force_participation_rate, reference_date \
             FROM v2_external_labor_force WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(employed) as employed, SUM(employed_male) as employed_male, \
             SUM(employed_female) as employed_female, SUM(unemployed) as unemployed, \
             SUM(unemployed_male) as unemployed_male, SUM(unemployed_female) as unemployed_female, \
             SUM(not_in_labor_force) as not_in_labor_force, \
             SUM(not_in_labor_force_male) as not_in_labor_force_male, \
             SUM(not_in_labor_force_female) as not_in_labor_force_female, \
             SUM(primary_industry_employed) as primary_industry_employed, \
             SUM(secondary_industry_employed) as secondary_industry_employed, \
             SUM(tertiary_industry_employed) as tertiary_industry_employed, \
             CAST(SUM(unemployed) AS REAL) / NULLIF(SUM(employed) + SUM(unemployed), 0) * 100 as unemployment_rate, \
             CAST(SUM(employed) + SUM(unemployed) AS REAL) / NULLIF(SUM(employed) + SUM(unemployed) + SUM(not_in_labor_force), 0) * 100 as labor_force_participation_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_labor_force WHERE prefecture = ?1".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(employed) as employed, SUM(employed_male) as employed_male, \
             SUM(employed_female) as employed_female, SUM(unemployed) as unemployed, \
             SUM(unemployed_male) as unemployed_male, SUM(unemployed_female) as unemployed_female, \
             SUM(not_in_labor_force) as not_in_labor_force, \
             SUM(not_in_labor_force_male) as not_in_labor_force_male, \
             SUM(not_in_labor_force_female) as not_in_labor_force_female, \
             SUM(primary_industry_employed) as primary_industry_employed, \
             SUM(secondary_industry_employed) as secondary_industry_employed, \
             SUM(tertiary_industry_employed) as tertiary_industry_employed, \
             CAST(SUM(unemployed) AS REAL) / NULLIF(SUM(employed) + SUM(unemployed), 0) * 100 as unemployment_rate, \
             CAST(SUM(employed) + SUM(unemployed) AS REAL) / NULLIF(SUM(employed) + SUM(unemployed) + SUM(not_in_labor_force), 0) * 100 as labor_force_participation_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_labor_force".to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_labor_force")
}

pub(crate) fn fetch_medical_welfare(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, general_hospitals, general_clinics, dental_clinics, \
             physicians, dentists, pharmacists, daycare_facilities, \
             physicians_per_10k_pop, daycare_per_1k_children_0_14, reference_year \
             FROM v2_external_medical_welfare WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(general_hospitals) as general_hospitals, \
             SUM(general_clinics) as general_clinics, \
             SUM(dental_clinics) as dental_clinics, \
             SUM(physicians) as physicians, SUM(dentists) as dentists, \
             SUM(pharmacists) as pharmacists, SUM(daycare_facilities) as daycare_facilities, \
             NULL as physicians_per_10k_pop, NULL as daycare_per_1k_children_0_14, \
             MAX(reference_year) as reference_year \
             FROM v2_external_medical_welfare WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(general_hospitals) as general_hospitals, \
             SUM(general_clinics) as general_clinics, \
             SUM(dental_clinics) as dental_clinics, \
             SUM(physicians) as physicians, SUM(dentists) as dentists, \
             SUM(pharmacists) as pharmacists, SUM(daycare_facilities) as daycare_facilities, \
             NULL as physicians_per_10k_pop, NULL as daycare_per_1k_children_0_14, \
             MAX(reference_year) as reference_year \
             FROM v2_external_medical_welfare"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_medical_welfare")
}

pub(crate) fn fetch_education_facilities(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, kindergartens, elementary_schools, \
             junior_high_schools, high_schools, reference_year \
             FROM v2_external_education_facilities WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(kindergartens) as kindergartens, \
             SUM(elementary_schools) as elementary_schools, \
             SUM(junior_high_schools) as junior_high_schools, \
             SUM(high_schools) as high_schools, \
             MAX(reference_year) as reference_year \
             FROM v2_external_education_facilities WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(kindergartens) as kindergartens, \
             SUM(elementary_schools) as elementary_schools, \
             SUM(junior_high_schools) as junior_high_schools, \
             SUM(high_schools) as high_schools, \
             MAX(reference_year) as reference_year \
             FROM v2_external_education_facilities"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_education_facilities")
}

pub(crate) fn fetch_geography(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, total_area_km2, habitable_area_km2, \
             population_density_per_km2, habitable_density_per_km2, reference_year \
             FROM v2_external_geography WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(total_area_km2) as total_area_km2, \
             SUM(habitable_area_km2) as habitable_area_km2, \
             NULL as population_density_per_km2, \
             NULL as habitable_density_per_km2, \
             MAX(reference_year) as reference_year \
             FROM v2_external_geography WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, '' as municipality, \
             SUM(total_area_km2) as total_area_km2, \
             SUM(habitable_area_km2) as habitable_area_km2, \
             NULL as population_density_per_km2, \
             NULL as habitable_density_per_km2, \
             MAX(reference_year) as reference_year \
             FROM v2_external_geography"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_geography")
}
