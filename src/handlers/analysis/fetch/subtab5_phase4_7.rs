//! サブタブ5 Phase 4-7: 外国人在留・学歴・世帯・日銀短観・社会生活・地価・自動車・ネット・産業構造

#[allow(unused_imports)]
use serde_json::Value;
#[allow(unused_imports)]
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

/// HW 求人 (postings) の `industry_raw` を集計し、12 大分類 (国勢調査) に
/// マッピングして件数降順で返す。
///
/// 用途: CR-9 産業ミスマッチ警戒 section (`industry_mismatch.rs`) の比較対象。
///
/// 粒度:
/// - `pref` + `muni` 指定: 当該市区町村の HW 掲載求人を集計
/// - `pref` のみ: 都道府県集計
/// - 両方空: 全国集計
///
/// メモリルール準拠:
/// - `feedback_hw_data_scope`: HW 掲載求人のみで全求人市場ではない
/// - `feedback_never_guess_data`: postings.industry_raw は実カラム grep 確認済み
pub(crate) fn fetch_hw_industry_counts(db: &Db, pref: &str, muni: &str) -> Vec<(String, i64)> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT industry_raw, COUNT(*) as cnt FROM postings \
             WHERE prefecture = ?1 AND municipality = ?2 \
               AND industry_raw IS NOT NULL AND industry_raw != '' \
             GROUP BY industry_raw"
                .to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT industry_raw, COUNT(*) as cnt FROM postings \
             WHERE prefecture = ?1 \
               AND industry_raw IS NOT NULL AND industry_raw != '' \
             GROUP BY industry_raw"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT industry_raw, COUNT(*) as cnt FROM postings \
             WHERE industry_raw IS NOT NULL AND industry_raw != '' \
             GROUP BY industry_raw"
                .to_string(),
            vec![],
        )
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows: Vec<Row> = db.query(&sql, &p).unwrap_or_default();
    use super::super::super::helpers::{get_i64, get_str_ref};
    use super::super::super::survey::report_html::industry_mismatch::map_hw_to_major_industry;
    let mut major_counts: HashMap<String, i64> = HashMap::new();
    for r in &rows {
        let raw = get_str_ref(r, "industry_raw");
        if raw.is_empty() {
            continue;
        }
        let cnt = get_i64(r, "cnt");
        if cnt <= 0 {
            continue;
        }
        let major = map_hw_to_major_industry(raw).to_string();
        *major_counts.entry(major).or_insert(0) += cnt;
    }
    let mut out: Vec<(String, i64)> = major_counts.into_iter().collect();
    out.sort_by(|a, b| b.1.cmp(&a.1));
    out
}

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
