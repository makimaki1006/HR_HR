//! サブタブ7（通勤圏分析）+ Phase A SSDSE-A 6関数 + 県平均補助関数
//! - 通勤圏（コミュートゾーン、bounding box + haversine）
//! - 通勤フロー（v2_external_commute_od、国勢調査OD実データ）
//! - Phase A: 世帯・自然動態・労働力・医療福祉・教育施設・地理 6テーブル
//! - 県平均（SUM方式）汎用補助関数

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{get_f64, get_i64, get_str, haversine, table_exists};
use super::query_turso_or_local;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

// ======== 通勤圏（コミュートゾーン） ========

/// 通勤圏内の市区町村
pub(crate) struct CommuteMunicipality {
    pub prefecture: String,
    pub municipality: String,
    pub distance_km: f64,
    pub _lat: f64,
    pub _lng: f64,
}

/// 通勤圏市区町村を抽出（bounding box + haversine 2段階フィルタ）
pub(crate) fn fetch_commute_zone(
    db: &Db,
    center_pref: &str,
    center_muni: &str,
    radius_km: f64,
) -> Vec<CommuteMunicipality> {
    if center_muni.is_empty() {
        return vec![];
    }

    // 中心座標取得
    let center = db.query(
        "SELECT latitude, longitude FROM municipality_geocode WHERE prefecture = ?1 AND municipality = ?2",
        &[&center_pref as &dyn rusqlite::types::ToSql, &center_muni],
    ).unwrap_or_default();
    if center.is_empty() {
        return vec![];
    }

    let center_lat = get_f64(&center[0], "latitude");
    let center_lng = get_f64(&center[0], "longitude");
    if center_lat.abs() < 1.0 {
        return vec![];
    }

    // Bounding box計算
    let lat_delta = radius_km / 111.0;
    let lng_delta = radius_km / (111.0 * center_lat.to_radians().cos().abs().max(0.01));
    let lat_min = center_lat - lat_delta;
    let lat_max = center_lat + lat_delta;
    let lng_min = center_lng - lng_delta;
    let lng_max = center_lng + lng_delta;

    // Bounding boxクエリ
    let candidates = db
        .query(
            "SELECT prefecture, municipality, latitude, longitude FROM municipality_geocode \
         WHERE latitude BETWEEN ?1 AND ?2 AND longitude BETWEEN ?3 AND ?4",
            &[
                &lat_min as &dyn rusqlite::types::ToSql,
                &lat_max,
                &lng_min,
                &lng_max,
            ],
        )
        .unwrap_or_default();

    // Haversineフィルタ + ソート
    let mut result: Vec<CommuteMunicipality> = candidates
        .iter()
        .filter_map(|row| {
            let pref = get_str(row, "prefecture");
            let muni = get_str(row, "municipality");
            let lat = get_f64(row, "latitude");
            let lng = get_f64(row, "longitude");
            let dist = haversine(center_lat, center_lng, lat, lng);
            if dist <= radius_km && !muni.is_empty() {
                Some(CommuteMunicipality {
                    prefecture: pref,
                    municipality: muni,
                    distance_km: dist,
                    _lat: lat,
                    _lng: lng,
                })
            } else {
                None
            }
        })
        .collect();

    result.sort_by(|a, b| {
        a.distance_km
            .partial_cmp(&b.distance_km)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    result
}

/// 通勤圏内の人口ピラミッドを集約（性別×年齢）
pub(crate) fn fetch_commute_zone_pyramid(
    db: &Db,
    turso: Option<&TursoDb>,
    munis: &[CommuteMunicipality],
) -> Vec<Row> {
    let mut agg: HashMap<String, (i64, i64)> = HashMap::new(); // age_group -> (male_sum, female_sum)

    for m in munis {
        let rows =
            super::subtab5::fetch_population_pyramid(db, turso, &m.prefecture, &m.municipality);
        for row in &rows {
            let age = get_str(row, "age_group");
            let male = get_i64(row, "male_count");
            let female = get_i64(row, "female_count");
            let entry = agg.entry(age).or_insert((0, 0));
            entry.0 += male;
            entry.1 += female;
        }
    }

    // 年齢順ソート用の変換
    let age_order = |a: &str| -> i32 {
        match a {
            "0-4" => 0,
            "5-9" => 1,
            "10-14" => 2,
            "15-19" => 3,
            "20-24" => 4,
            "25-29" => 5,
            "30-34" => 6,
            "35-39" => 7,
            "40-44" => 8,
            "45-49" => 9,
            "50-54" => 10,
            "55-59" => 11,
            "60-64" => 12,
            "65-69" => 13,
            "70-74" => 14,
            "75-79" => 15,
            "80-84" => 16,
            "85+" => 17,
            // 9区分フォールバック
            "0-9" => 0,
            "10-19" => 2,
            "20-29" => 4,
            "30-39" => 6,
            "40-49" => 8,
            "50-59" => 10,
            "60-69" => 12,
            "70-79" => 14,
            "80+" => 16,
            _ => 99,
        }
    };

    let mut rows: Vec<Row> = agg
        .into_iter()
        .map(|(age, (male, female))| {
            let mut row = HashMap::new();
            row.insert("age_group".to_string(), serde_json::Value::String(age));
            row.insert("male_count".to_string(), serde_json::json!(male));
            row.insert("female_count".to_string(), serde_json::json!(female));
            row
        })
        .collect();

    rows.sort_by_key(|r| {
        let age = r.get("age_group").and_then(|v| v.as_str()).unwrap_or("");
        age_order(age)
    });

    rows
}

// ======== 通勤フロー（実データ: 国勢調査OD行列） ========

/// 通勤フローレコード
pub(crate) struct CommuteFlow {
    pub partner_pref: String,
    pub partner_muni: String,
    pub total_commuters: i64,
    pub male_commuters: i64,
    pub female_commuters: i64,
}

/// この市区町村への通勤流入（実フロー: v2_external_commute_od）
pub(crate) fn fetch_commute_inflow(db: &Db, pref: &str, muni: &str) -> Vec<CommuteFlow> {
    if muni.is_empty() {
        return vec![];
    }
    if !table_exists(db, "v2_external_commute_od") {
        return vec![];
    }

    let rows = db
        .query(
            "SELECT origin_pref, origin_muni, total_commuters, male_commuters, female_commuters \
         FROM v2_external_commute_od \
         WHERE dest_pref = ?1 AND dest_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT 20",
            &[&pref as &dyn rusqlite::types::ToSql, &muni],
        )
        .unwrap_or_default();

    rows.iter()
        .map(|r| CommuteFlow {
            partner_pref: get_str(r, "origin_pref"),
            partner_muni: get_str(r, "origin_muni"),
            total_commuters: get_i64(r, "total_commuters"),
            male_commuters: get_i64(r, "male_commuters"),
            female_commuters: get_i64(r, "female_commuters"),
        })
        .collect()
}

/// この市区町村からの通勤流出（実フロー）
pub(crate) fn fetch_commute_outflow(db: &Db, pref: &str, muni: &str) -> Vec<CommuteFlow> {
    if muni.is_empty() {
        return vec![];
    }
    if !table_exists(db, "v2_external_commute_od") {
        return vec![];
    }

    let rows = db
        .query(
            "SELECT dest_pref, dest_muni, total_commuters, male_commuters, female_commuters \
         FROM v2_external_commute_od \
         WHERE origin_pref = ?1 AND origin_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT 20",
            &[&pref as &dyn rusqlite::types::ToSql, &muni],
        )
        .unwrap_or_default();

    rows.iter()
        .map(|r| CommuteFlow {
            partner_pref: get_str(r, "dest_pref"),
            partner_muni: get_str(r, "dest_muni"),
            total_commuters: get_i64(r, "total_commuters"),
            male_commuters: get_i64(r, "male_commuters"),
            female_commuters: get_i64(r, "female_commuters"),
        })
        .collect()
}

/// 地元就業率（自市区町村内で働く人の割合）
pub(crate) fn fetch_self_commute_rate(db: &Db, pref: &str, muni: &str) -> f64 {
    if muni.is_empty() || !table_exists(db, "v2_external_commute_od") {
        return 0.0;
    }

    let self_count = db.query_scalar::<i64>(
        "SELECT total_commuters FROM v2_external_commute_od WHERE origin_pref=?1 AND origin_muni=?2 AND dest_pref=?1 AND dest_muni=?2",
        &[&pref as &dyn rusqlite::types::ToSql, &muni],
    ).unwrap_or(0);

    let total_outflow = db.query_scalar::<i64>(
        "SELECT SUM(total_commuters) FROM v2_external_commute_od WHERE origin_pref=?1 AND origin_muni=?2",
        &[&pref as &dyn rusqlite::types::ToSql, &muni],
    ).unwrap_or(0);

    if total_outflow > 0 {
        self_count as f64 / total_outflow as f64
    } else {
        0.0
    }
}

// ======== Phase A: SSDSE-A 新規6関数 + 県平均補助関数 ========

/// Phase A: 世帯構造データ（v2_external_households）
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

/// Phase A: 自然動態データ（v2_external_vital_statistics）
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

/// Phase A: 労働力データ（v2_external_labor_force、SUM方式で比率再計算）
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
             NULL as labor_force_participation_rate, \
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
             NULL as labor_force_participation_rate, \
             MAX(reference_date) as reference_date \
             FROM v2_external_labor_force".to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_labor_force")
}

/// Phase A: 医療福祉データ（v2_external_medical_welfare）
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

/// Phase A: 教育施設データ（v2_external_education_facilities）
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

/// Phase A: 地理データ（v2_external_geography、密度は動的再計算）
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

/// Phase A: 県平均（SUM方式）汎用補助関数
///
/// 指定テーブル・指定カラムについて、都道府県内の市区町村合計から比率を再計算する。
/// 例: unemployment_rate の県平均 = SUM(unemployed) / SUM(employed + unemployed) × 100
///
/// # Arguments
/// * `numerator_sum_sql` - 分子のSUM式 (例: "SUM(unemployed)")
/// * `denominator_sum_sql` - 分母のSUM式 (例: "SUM(employed) + SUM(unemployed)")
/// * `table` - テーブル名 (例: "v2_external_labor_force")
pub(crate) fn fetch_prefecture_mean(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    numerator_sum_sql: &str,
    denominator_sum_sql: &str,
    table: &str,
) -> Option<f64> {
    let sql = format!(
        "SELECT CAST({numerator_sum_sql} AS REAL) / NULLIF({denominator_sum_sql}, 0) * 100 as rate \
         FROM {table} WHERE prefecture = ?1"
    );
    let rows = query_turso_or_local(turso, db, &sql, &[pref.to_string()], table);
    rows.first().and_then(|r| {
        let v = get_f64(r, "rate");
        if v.is_nan() || v.is_infinite() {
            None
        } else {
            Some(v)
        }
    })
}
