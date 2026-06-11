//! 職種カルテ用 Turso データ取得。
//!
//! 5 テーブル:
//!   v2_external_jobtag_occupation       職業マスタ
//!   v2_external_jobtag_description      解説 4 種
//!   v2_external_jobtag_scores           スコア（興味/価値観/スキル）
//!   v2_external_jobtag_qualifications   関連資格
//!   v2_external_jobtag_wage_age         賃金センサス年齢別

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;

use crate::db::turso_http::TursoDb;

use super::OccupationCard;

#[derive(Debug)]
pub enum DriverDataError {
    Turso(String),
    NotFound,
}

impl fmt::Display for DriverDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DriverDataError::Turso(s) => write!(f, "turso error: {s}"),
            DriverDataError::NotFound => write!(f, "not found"),
        }
    }
}

impl From<String> for DriverDataError {
    fn from(s: String) -> Self {
        DriverDataError::Turso(s)
    }
}

// ───────────────────────── データ型 ─────────────────────────

#[derive(Serialize, Clone)]
pub struct OccupationRow {
    pub jobtag_id: i64,
    pub name: String,
    pub category: String,
    pub aliases: String,
    pub mhlw_classification: String,
    pub wage_census_code: String,
    pub wage_census_name: String,
}

#[derive(Serialize, Clone, Default)]
pub struct DescriptionRow {
    pub summary: String,
    pub what_is_the_job: String,
    pub how_to_become: String,
    pub working_conditions: String,
}

#[derive(Serialize, Clone)]
pub struct WageAgeRow {
    pub age_range_order: i64,
    pub age_range: String,
    pub avg_age: Option<f64>,
    pub tenure_years: Option<f64>,
    pub scheduled_hours: Option<f64>,
    pub overtime_hours: Option<f64>,
    pub monthly_total_thousand_yen: Option<f64>,
    pub monthly_scheduled_thousand_yen: Option<f64>,
    pub annual_bonus_thousand_yen: Option<f64>,
    pub workers_count_tenfold: Option<f64>,
    pub annual_salary_man_yen: Option<f64>,
}

#[derive(Serialize, Clone)]
pub struct ScoreItem {
    pub item_order: i64,
    pub item: String,
    pub score: f64,
}

#[derive(Serialize)]
pub struct OccupationDetail {
    pub occupation: OccupationRow,
    pub description: DescriptionRow,
    pub wage_rows: Vec<WageAgeRow>,
    pub interest_scores: Vec<ScoreItem>,
    pub values_scores: Vec<ScoreItem>,
    pub skills_scores: Vec<ScoreItem>,
    pub qualifications: Vec<String>,
}

// ───────────────────────── ヘルパ ─────────────────────────

fn s(row: &HashMap<String, Value>, key: &str) -> String {
    row.get(key)
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string()
}

fn i(row: &HashMap<String, Value>, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn f(row: &HashMap<String, Value>, key: &str) -> Option<f64> {
    row.get(key).and_then(|v| match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse::<f64>().ok(),
        _ => None,
    })
}

// ───────────────────────── カテゴリ情報 ─────────────────────────

#[derive(Serialize, Clone)]
pub struct CategoryInfo {
    pub key: String,
    pub label: String,
    pub count: i64,
}

/// カテゴリ別件数を取得する。DBに存在するキーのみ返す。
pub fn fetch_category_counts(turso: &TursoDb) -> Result<Vec<CategoryInfo>, String> {
    let rows = turso.query(
        "SELECT category, COUNT(*) AS cnt \
         FROM v2_external_jobtag_occupation \
         WHERE category IS NOT NULL \
         GROUP BY category \
         ORDER BY category",
        &[],
    )?;

    // key → 日本語ラベルの固定マップ
    let label_map: HashMap<&str, &str> = [
        ("driver",              "ドライバー"),
        ("logistics",           "物流・運輸"),
        ("manufacturing",       "製造・加工"),
        ("construction",        "建築・土木"),
        ("cleaning",            "清掃・廃棄物"),
        ("labor",               "倉庫・作業員"),
        ("office",              "事務"),
        ("sales",               "販売・営業"),
        ("service",             "サービス"),
        ("professional",        "専門・技術"),
        ("legal_culture",       "法務・文化芸術"),
        ("education_childcare", "保育・教育"),
        ("security",            "警備・保安"),
        ("agriculture",         "農林漁業"),
        ("management",          "管理職"),
    ]
    .into_iter()
    .collect();

    let out = rows
        .iter()
        .map(|r| {
            let key = s(r, "category");
            let label = label_map
                .get(key.as_str())
                .copied()
                .unwrap_or(key.as_str())
                .to_string();
            CategoryInfo {
                key,
                label,
                count: i(r, "cnt"),
            }
        })
        .collect();

    Ok(out)
}

// ───────────────────────── 公開関数 ─────────────────────────

/// 職業一覧（カードビュー用）を取得する。category=None で全件。
pub fn fetch_occupation_list(
    turso: &TursoDb,
    category: Option<&str>,
) -> Result<Vec<OccupationCard>, String> {
    // wage_age とのLEFT JOINで「総計行」(age_range_order=0)から代表値を引く
    let mut sql = String::from(
        "SELECT o.jobtag_id, o.name, o.category, o.wage_census_code, o.wage_census_name, o.aliases, \
                w.avg_age, w.scheduled_hours, w.annual_salary_man_yen, w.workers_count_tenfold \
         FROM v2_external_jobtag_occupation o \
         LEFT JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0",
    );
    if category.is_some() {
        sql.push_str(" WHERE o.category = ?");
    }
    sql.push_str(" ORDER BY o.category, o.name");

    let rows = if let Some(c) = category {
        turso.query(&sql, &[&c.to_string() as &dyn crate::db::turso_http::ToSqlTurso])?
    } else {
        turso.query(&sql, &[])?
    };

    let mut out = Vec::with_capacity(rows.len());
    for r in rows.iter() {
        out.push(OccupationCard {
            jobtag_id: i(r, "jobtag_id"),
            name: s(r, "name"),
            category: s(r, "category"),
            wage_census_code: s(r, "wage_census_code"),
            wage_census_name: s(r, "wage_census_name"),
            avg_age: f(r, "avg_age"),
            annual_salary_man_yen: f(r, "annual_salary_man_yen"),
            workers_count: f(r, "workers_count_tenfold").map(|x| x * 10.0),
            aliases: s(r, "aliases"),
        });
    }
    Ok(out)
}

/// 個別職業の全データを取得する。
pub fn fetch_occupation_detail(
    turso: &TursoDb,
    jobtag_id: i64,
) -> Result<OccupationDetail, DriverDataError> {
    let occ_rows = turso.query(
        "SELECT jobtag_id, name, COALESCE(category,'') AS category, COALESCE(aliases,'') AS aliases, \
                COALESCE(mhlw_classification,'') AS mhlw_classification, \
                COALESCE(wage_census_code,'') AS wage_census_code, \
                COALESCE(wage_census_name,'') AS wage_census_name \
         FROM v2_external_jobtag_occupation WHERE jobtag_id = ?",
        &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let occ_row = occ_rows.into_iter().next().ok_or(DriverDataError::NotFound)?;
    let occupation = OccupationRow {
        jobtag_id: i(&occ_row, "jobtag_id"),
        name: s(&occ_row, "name"),
        category: s(&occ_row, "category"),
        aliases: s(&occ_row, "aliases"),
        mhlw_classification: s(&occ_row, "mhlw_classification"),
        wage_census_code: s(&occ_row, "wage_census_code"),
        wage_census_name: s(&occ_row, "wage_census_name"),
    };

    // description（未投入時は空構造体）
    let desc_rows = turso.query(
        "SELECT COALESCE(summary,'') AS summary, COALESCE(what_is_the_job,'') AS what_is_the_job, \
                COALESCE(how_to_become,'') AS how_to_become, COALESCE(working_conditions,'') AS working_conditions \
         FROM v2_external_jobtag_description WHERE jobtag_id = ?",
        &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let description = desc_rows
        .into_iter()
        .next()
        .map(|r| DescriptionRow {
            summary: s(&r, "summary"),
            what_is_the_job: s(&r, "what_is_the_job"),
            how_to_become: s(&r, "how_to_become"),
            working_conditions: s(&r, "working_conditions"),
        })
        .unwrap_or_default();

    let wage_rows = fetch_wage_age(turso, &occupation.wage_census_code)?;

    let scores_rows = turso.query(
        "SELECT category, item_order, item, score FROM v2_external_jobtag_scores \
         WHERE jobtag_id = ? ORDER BY category, item_order",
        &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let mut interest_scores = Vec::new();
    let mut values_scores = Vec::new();
    let mut skills_scores = Vec::new();
    for r in scores_rows.iter() {
        let cat = s(r, "category");
        let item = ScoreItem {
            item_order: i(r, "item_order"),
            item: s(r, "item"),
            score: f(r, "score").unwrap_or(0.0),
        };
        match cat.as_str() {
            "interest" => interest_scores.push(item),
            "values" => values_scores.push(item),
            "skills" => skills_scores.push(item),
            _ => {}
        }
    }

    let qual_rows = turso.query(
        "SELECT name FROM v2_external_jobtag_qualifications \
         WHERE jobtag_id = ? ORDER BY item_order",
        &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let qualifications: Vec<String> = qual_rows.iter().map(|r| s(r, "name")).collect();

    Ok(OccupationDetail {
        occupation,
        description,
        wage_rows,
        interest_scores,
        values_scores,
        skills_scores,
        qualifications,
    })
}

/// 賃金センサスの年齢階級別データを取得する（総計含む13行）。
pub fn fetch_wage_age(turso: &TursoDb, wage_code: &str) -> Result<Vec<WageAgeRow>, String> {
    if wage_code.is_empty() {
        return Ok(Vec::new());
    }
    let rows = turso.query(
        "SELECT age_range_order, age_range, avg_age, tenure_years, scheduled_hours, overtime_hours, \
                monthly_total_thousand_yen, monthly_scheduled_thousand_yen, annual_bonus_thousand_yen, \
                workers_count_tenfold, annual_salary_man_yen \
         FROM v2_external_jobtag_wage_age \
         WHERE wage_census_code = ? ORDER BY age_range_order",
        &[&wage_code.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    Ok(rows
        .iter()
        .map(|r| WageAgeRow {
            age_range_order: i(r, "age_range_order"),
            age_range: s(r, "age_range"),
            avg_age: f(r, "avg_age"),
            tenure_years: f(r, "tenure_years"),
            scheduled_hours: f(r, "scheduled_hours"),
            overtime_hours: f(r, "overtime_hours"),
            monthly_total_thousand_yen: f(r, "monthly_total_thousand_yen"),
            monthly_scheduled_thousand_yen: f(r, "monthly_scheduled_thousand_yen"),
            annual_bonus_thousand_yen: f(r, "annual_bonus_thousand_yen"),
            workers_count_tenfold: f(r, "workers_count_tenfold"),
            annual_salary_man_yen: f(r, "annual_salary_man_yen"),
        })
        .collect())
}
