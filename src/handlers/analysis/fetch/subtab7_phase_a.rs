//! サブタブ7 Phase A: SSDSE-A 6テーブル（世帯・自然動態・労働力・医療福祉・教育施設・地理）

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::normalize_muni_for_external;
#[allow(unused_imports)]
use super::super::super::helpers::table_exists;
use super::{query_turso_or_local, EXTERNAL_CLEAN_FILTER};

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
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
    } else if !pref.is_empty() {
        // 2026-05-24 audit_B P0-2: EXTERNAL_CLEAN_FILTER 適用
        (
            format!(
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
             FROM v2_external_households WHERE prefecture = ?1 AND {}",
                EXTERNAL_CLEAN_FILTER
            ),
            vec![pref.to_string()],
        )
    } else {
        (
            format!(
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
             FROM v2_external_households WHERE {}",
                EXTERNAL_CLEAN_FILTER
            ),
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
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
    } else if !pref.is_empty() {
        // 2026-05-24 audit_B P0-2: EXTERNAL_CLEAN_FILTER 適用
        (
            format!(
                "SELECT ?1 as prefecture, '全体' as municipality, \
             SUM(births) as births, SUM(deaths) as deaths, \
             SUM(births) - SUM(deaths) as natural_change, \
             SUM(marriages) as marriages, SUM(divorces) as divorces, \
             NULL as birth_rate_permille, NULL as death_rate_permille, \
             NULL as marriage_rate_permille, NULL as divorce_rate_permille, \
             MAX(reference_year) as reference_year \
             FROM v2_external_vital_statistics WHERE prefecture = ?1 AND {}",
                EXTERNAL_CLEAN_FILTER
            ),
            vec![pref.to_string()],
        )
    } else {
        (
            format!(
                "SELECT '全国' as prefecture, '' as municipality, \
             SUM(births) as births, SUM(deaths) as deaths, \
             SUM(births) - SUM(deaths) as natural_change, \
             SUM(marriages) as marriages, SUM(divorces) as divorces, \
             NULL as birth_rate_permille, NULL as death_rate_permille, \
             NULL as marriage_rate_permille, NULL as divorce_rate_permille, \
             MAX(reference_year) as reference_year \
             FROM v2_external_vital_statistics WHERE {}",
                EXTERNAL_CLEAN_FILTER
            ),
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
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
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
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
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
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
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

/// v2_external_geography 単位バグ是正 (2026-07-03)
///
/// Turso DB 診断 (SELECT のみ、1,741 行、重複ゼロ) で以下を確認:
/// - `total_area_km2` / `habitable_area_km2` の実態は **ヘクタール** (km² の 100 倍)。
///   例: 大分市 total_area_km2=50,239 (実際 502.39km²) / habitable_area_km2=24,587 (実際 ≈245.9km²)。
///   横浜市 43,801→437.7km² / 浜松市 155,811→1,558km² / 別海町 131,717→1,317km² で全行一律 100 倍を確認。
/// - `population_density_per_km2` / `habitable_density_per_km2` の実態は **人/ha** (人/km² の 1/100)。
///   例: 大分市 density=9.467 (実際 ≈947 人/km²)。
/// - `habitable_ratio` は無次元比率のため影響なし。
///
/// DB 側は書き換えず、取得点 (fetch_geography) で一括換算する:
/// 面積 ÷100 (ha→km²)、密度 ×100 (人/ha→人/km²)。
/// 消費者側 (insight GE-1 / jobmap external_panels / region karte /
/// survey integration・region・regional_compare / navy §02 表2-B) の閾値は
/// すべて真値スケール (人/km²) で定義済みのため、本換算のみで整合する。
fn convert_geography_units_ha_to_km2(rows: &mut [Row]) {
    /// ha が km² 列に格納されている面積カラム (÷100 で km² に是正)
    const AREA_HA_COLS: [&str; 2] = ["total_area_km2", "habitable_area_km2"];
    /// 人/ha が 人/km² 列に格納されている密度カラム (×100 で 人/km² に是正)
    const DENSITY_PER_HA_COLS: [&str; 2] =
        ["population_density_per_km2", "habitable_density_per_km2"];

    // Turso HTTP は数値を文字列で返す場合があるため helpers::get_f64_opt と同じ解釈で読む
    fn to_f64(v: &Value) -> Option<f64> {
        if v.is_null() {
            return None;
        }
        v.as_f64()
            .or_else(|| v.as_i64().map(|i| i as f64))
            .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
    }
    // 浮動小数ノイズ抑制 (9.467×100 = 946.7000...01 → 946.7)
    fn round6(v: f64) -> f64 {
        (v * 1e6).round() / 1e6
    }

    for row in rows.iter_mut() {
        for col in AREA_HA_COLS {
            if let Some(v) = row.get(col).and_then(to_f64) {
                if let Some(n) = serde_json::Number::from_f64(round6(v / 100.0)) {
                    row.insert(col.to_string(), Value::Number(n));
                }
            }
        }
        for col in DENSITY_PER_HA_COLS {
            if let Some(v) = row.get(col).and_then(to_f64) {
                if let Some(n) = serde_json::Number::from_f64(round6(v * 100.0)) {
                    row.insert(col.to_string(), Value::Number(n));
                }
            }
        }
    }
}

pub(crate) fn fetch_geography(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    // 県/全国パスの SUM(): 現状 DB は 1 市 1 行 (PRIMARY KEY(prefecture, municipality)) だが、
    // 将来 reference_year 違いの多年スナップショットが混入した場合に面積が膨張しないよう
    // 最新年のみを対象とする防御ガードを入れる (2026-07-03)。
    const LATEST_YEAR_GUARD: &str =
        "reference_year = (SELECT MAX(reference_year) FROM v2_external_geography)";
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT prefecture, municipality, total_area_km2, habitable_area_km2, \
             population_density_per_km2, habitable_density_per_km2, reference_year \
             FROM v2_external_geography WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
    } else if !pref.is_empty() {
        (
            format!(
                "SELECT ?1 as prefecture, '全体' as municipality, \
                 SUM(total_area_km2) as total_area_km2, \
                 SUM(habitable_area_km2) as habitable_area_km2, \
                 NULL as population_density_per_km2, \
                 NULL as habitable_density_per_km2, \
                 MAX(reference_year) as reference_year \
                 FROM v2_external_geography WHERE prefecture = ?1 AND {LATEST_YEAR_GUARD}"
            ),
            vec![pref.to_string()],
        )
    } else {
        (
            format!(
                "SELECT '全国' as prefecture, '' as municipality, \
                 SUM(total_area_km2) as total_area_km2, \
                 SUM(habitable_area_km2) as habitable_area_km2, \
                 NULL as population_density_per_km2, \
                 NULL as habitable_density_per_km2, \
                 MAX(reference_year) as reference_year \
                 FROM v2_external_geography WHERE {LATEST_YEAR_GUARD}"
            ),
            vec![],
        )
    };
    let mut rows = query_turso_or_local(turso, db, &sql, &params, "v2_external_geography");
    // 単位バグ是正: ha 系実態 → km² 系へ一括換算 (詳細は convert_geography_units_ha_to_km2 の doc comment)
    convert_geography_units_ha_to_km2(&mut rows);
    rows
}

#[cfg(test)]
mod geography_unit_tests {
    use super::*;
    use serde_json::json;

    fn f(row: &Row, key: &str) -> f64 {
        crate::handlers::helpers::get_f64(row, key)
    }

    /// 大分市の実 DB 値 (ha 系混入: 50,239 / 24,587 / 9.467) が
    /// 実際の値 (502.39km² / 245.87km² / 946.7 人/km²) に換算されることを固定する。
    #[test]
    fn test_geography_unit_conversion_oita_case() {
        let mut row: Row = HashMap::new();
        row.insert("prefecture".into(), json!("大分県"));
        row.insert("municipality".into(), json!("大分市"));
        row.insert("total_area_km2".into(), json!(50_239.0));
        row.insert("habitable_area_km2".into(), json!(24_587.0));
        row.insert("population_density_per_km2".into(), json!(9.467));
        row.insert("habitable_density_per_km2".into(), json!(19.343));
        row.insert("reference_year".into(), json!("2021"));
        let mut rows = vec![row];

        convert_geography_units_ha_to_km2(&mut rows);

        let r = &rows[0];
        assert!(
            (f(r, "total_area_km2") - 502.39).abs() < 1e-6,
            "total_area_km2: 50,239ha → 502.39km² (実際: {})",
            f(r, "total_area_km2")
        );
        assert!(
            (f(r, "habitable_area_km2") - 245.87).abs() < 1e-6,
            "habitable_area_km2: 24,587ha → 245.87km² (実際: {})",
            f(r, "habitable_area_km2")
        );
        assert!(
            (f(r, "population_density_per_km2") - 946.7).abs() < 1e-6,
            "population_density: 9.467 人/ha → 946.7 人/km² (実際: {})",
            f(r, "population_density_per_km2")
        );
        assert!(
            (f(r, "habitable_density_per_km2") - 1_934.3).abs() < 1e-6,
            "habitable_density: 19.343 人/ha → 1,934.3 人/km² (実際: {})",
            f(r, "habitable_density_per_km2")
        );
        // 非対象カラムは無変更
        assert_eq!(
            r.get("reference_year").and_then(|v| v.as_str()),
            Some("2021")
        );
    }

    /// 県/全国 SUM パスの NULL 密度は NULL のまま素通しされる (0 等に化けない)。
    #[test]
    fn test_geography_unit_conversion_null_density_passthrough() {
        let mut row: Row = HashMap::new();
        row.insert("prefecture".into(), json!("大分県"));
        row.insert("municipality".into(), json!("全体"));
        row.insert("total_area_km2".into(), json!(634_071.0));
        row.insert("habitable_area_km2".into(), json!(180_000.0));
        row.insert("population_density_per_km2".into(), Value::Null);
        row.insert("habitable_density_per_km2".into(), Value::Null);
        let mut rows = vec![row];

        convert_geography_units_ha_to_km2(&mut rows);

        let r = &rows[0];
        assert!((f(r, "total_area_km2") - 6_340.71).abs() < 1e-6);
        assert!((f(r, "habitable_area_km2") - 1_800.0).abs() < 1e-6);
        assert!(r["population_density_per_km2"].is_null());
        assert!(r["habitable_density_per_km2"].is_null());
    }

    /// Turso HTTP が数値を文字列で返すケースも換算対象 (helpers::get_f64_opt と同じ解釈)。
    #[test]
    fn test_geography_unit_conversion_string_numbers() {
        let mut row: Row = HashMap::new();
        row.insert("total_area_km2".into(), json!("43801"));
        row.insert("population_density_per_km2".into(), json!("86.1"));
        let mut rows = vec![row];

        convert_geography_units_ha_to_km2(&mut rows);

        let r = &rows[0];
        // 横浜市: 43,801ha → 438.01km²
        assert!((f(r, "total_area_km2") - 438.01).abs() < 1e-6);
        assert!((f(r, "population_density_per_km2") - 8_610.0).abs() < 1e-6);
    }
}
