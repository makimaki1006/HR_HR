//! 職種カルテ用 Turso データ取得。
//!
//! 7 テーブル:
//!   v2_external_jobtag_occupation         職業マスタ
//!   v2_external_jobtag_description        解説 4 種
//!   v2_external_jobtag_scores             スコア（興味/価値観/スキル）
//!   v2_external_jobtag_qualifications     関連資格
//!   v2_external_jobtag_wage_age           賃金センサス年齢別
//!   v2_external_jobtag_related_orgs       関連団体 (EX-D)
//!   v2_external_jobtag_wage_age_exp       経験年数別給与 (EX-A4)

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::fmt;
use std::sync::OnceLock;

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

/// 関連団体（EX-D）1行。
#[derive(Serialize, Clone, Default)]
pub struct RelatedOrgRow {
    pub name: String,
    pub url: String,
}

/// 経験年数別給与（EX-A4）1行。
#[derive(Serialize, Clone)]
pub struct WageAgeExpRow {
    pub age_range_order: i64,
    pub age_range: String,
    pub exp_range_order: i64,
    pub exp_range: String,
    pub monthly_scheduled_thousand_yen: Option<f64>,
    pub annual_bonus_thousand_yen: Option<f64>,
    pub workers_count_tenfold: Option<f64>,
    pub annual_salary_man_yen: Option<f64>,
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
    /// 関連団体（EX-D）。未投入時は空 Vec。
    pub related_orgs: Vec<RelatedOrgRow>,
    /// 経験年数別給与（EX-A4）。未投入時は空 Vec。
    pub wage_age_exp_rows: Vec<WageAgeExpRow>,
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
        ("driver", "ドライバー"),
        ("logistics", "物流・運輸"),
        ("manufacturing", "製造・加工"),
        ("construction", "建築・土木"),
        ("cleaning", "清掃・廃棄物"),
        ("labor", "倉庫・作業員"),
        ("office", "事務"),
        ("sales", "販売・営業"),
        ("service", "サービス"),
        ("professional", "専門・技術"),
        ("legal_culture", "法務・文化芸術"),
        ("education_childcare", "保育・教育"),
        ("security", "警備・保安"),
        ("agriculture", "農林漁業"),
        ("management", "管理職"),
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
        turso.query(
            &sql,
            &[&c.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
        )?
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
    let occ_row = occ_rows
        .into_iter()
        .next()
        .ok_or(DriverDataError::NotFound)?;
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

    let related_orgs = fetch_related_orgs(turso, occupation.jobtag_id).unwrap_or_else(|e| {
        // テーブル未投入時はエラーを無視して空配列を返す
        tracing::warn!("fetch_related_orgs({jobtag_id}) failed (treated as empty): {e}");
        Vec::new()
    });

    let wage_age_exp_rows =
        fetch_wage_age_exp(turso, &occupation.wage_census_code).unwrap_or_else(|e| {
            tracing::warn!(
                "fetch_wage_age_exp({}) failed (treated as empty): {e}",
                occupation.wage_census_code
            );
            Vec::new()
        });

    Ok(OccupationDetail {
        occupation,
        description,
        wage_rows,
        interest_scores,
        values_scores,
        skills_scores,
        qualifications,
        related_orgs,
        wage_age_exp_rows,
    })
}

/// 複数職業のディテールをまとめて取得（compare用）。失敗IDはNoneでスキップ。
pub fn fetch_multiple_occupations(turso: &TursoDb, ids: &[i64]) -> Vec<Option<OccupationDetail>> {
    ids.iter()
        .map(|id| fetch_occupation_detail(turso, *id).ok())
        .collect()
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

/// 関連団体（EX-D）を取得する。テーブル未投入時は Turso が Err を返すため呼び元で処理すること。
pub fn fetch_related_orgs(turso: &TursoDb, jobtag_id: i64) -> Result<Vec<RelatedOrgRow>, String> {
    let rows = turso.query(
        "SELECT name, COALESCE(url,'') AS url \
         FROM v2_external_jobtag_related_orgs \
         WHERE jobtag_id = ? ORDER BY item_order",
        &[&jobtag_id as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    Ok(rows
        .iter()
        .map(|r| RelatedOrgRow {
            name: s(r, "name"),
            url: s(r, "url"),
        })
        .collect())
}

/// 経験年数別給与（EX-A4）を取得する。wage_census_code が空なら空 Vec を返す。
pub fn fetch_wage_age_exp(turso: &TursoDb, wage_code: &str) -> Result<Vec<WageAgeExpRow>, String> {
    if wage_code.is_empty() {
        return Ok(Vec::new());
    }
    let rows = turso.query(
        "SELECT age_range_order, age_range, exp_range_order, exp_range, \
                monthly_scheduled_thousand_yen, annual_bonus_thousand_yen, \
                workers_count_tenfold, annual_salary_man_yen \
         FROM v2_external_jobtag_wage_age_exp \
         WHERE wage_census_code = ? \
         ORDER BY age_range_order, exp_range_order",
        &[&wage_code.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    Ok(rows
        .iter()
        .map(|r| WageAgeExpRow {
            age_range_order: i(r, "age_range_order"),
            age_range: s(r, "age_range"),
            exp_range_order: i(r, "exp_range_order"),
            exp_range: s(r, "exp_range"),
            monthly_scheduled_thousand_yen: f(r, "monthly_scheduled_thousand_yen"),
            annual_bonus_thousand_yen: f(r, "annual_bonus_thousand_yen"),
            workers_count_tenfold: f(r, "workers_count_tenfold"),
            annual_salary_man_yen: f(r, "annual_salary_man_yen"),
        })
        .collect())
}

// ───────────────────────── 都道府県別年齢分布（国勢調査 R2） ─────────────────────────

/// 国勢調査 R2 職業中分類 × 都道府県 の年齢階級別人口（1行）。
#[derive(Serialize, Clone)]
pub struct AgeDistributionRow {
    pub age_class: String,
    pub population: i64,
}

static OCC_MAP: OnceLock<HashMap<String, String>> = OnceLock::new();

fn get_occupation_middle_map() -> &'static HashMap<String, String> {
    OCC_MAP.get_or_init(|| {
        let raw = include_str!("../../../data/wage_census_to_occupation_middle_map.json");
        let v: serde_json::Value = serde_json::from_str(raw).unwrap_or(serde_json::json!({}));
        v.as_object()
            .map(|o| {
                o.iter()
                    .filter_map(|(k, val)| {
                        // "_comment", "_source", "_note" キーは除外
                        if k.starts_with('_') {
                            return None;
                        }
                        val.as_str().map(|s| (k.clone(), s.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default()
    })
}

/// 都道府県 × wage_census_code で国勢調査 R2 の年齢分布を取得する。
/// wage_census_code がマッピングになければ空 Vec を返す（テーブル未投入時も同様）。
pub fn fetch_age_distribution_by_pref(
    turso: &TursoDb,
    wage_census_code: &str,
    prefecture: &str,
) -> Result<Vec<AgeDistributionRow>, String> {
    if wage_census_code.is_empty() || prefecture.is_empty() {
        return Ok(Vec::new());
    }
    let map = get_occupation_middle_map();
    let occ_code = match map.get(wage_census_code) {
        Some(c) => c.clone(),
        None => return Ok(Vec::new()), // 未マッピング
    };

    let rows = turso.query(
        "SELECT age_class, COALESCE(population, 0) AS population \
         FROM v2_external_occupation_middle_pref \
         WHERE prefecture = ?1 AND occupation_code = ?2 AND gender = 'total' \
           AND age_class NOT LIKE '（再掲）%' \
           AND age_class <> '総数' \
           AND age_class <> '不詳' \
         ORDER BY age_class",
        &[
            &prefecture.to_string() as &dyn crate::db::turso_http::ToSqlTurso,
            &occ_code as &dyn crate::db::turso_http::ToSqlTurso,
        ],
    )?;

    if rows.is_empty() {
        return Ok(Vec::new());
    }

    // 年齢階級を辞書順ではなく年齢順で並べ替える
    let age_order = [
        "15～19歳",
        "20～24歳",
        "25～29歳",
        "30～34歳",
        "35～39歳",
        "40～44歳",
        "45～49歳",
        "50～54歳",
        "55～59歳",
        "60～64歳",
        "65～69歳",
        "70～74歳",
        "75～79歳",
        "80～84歳",
        "85歳以上",
    ];
    let order_map: HashMap<&str, usize> =
        age_order.iter().enumerate().map(|(i, &s)| (s, i)).collect();

    let mut out: Vec<AgeDistributionRow> = rows
        .iter()
        .map(|r| AgeDistributionRow {
            age_class: s(r, "age_class"),
            population: i(r, "population"),
        })
        .collect();

    out.sort_by_key(|r| {
        order_map
            .get(r.age_class.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });

    Ok(out)
}

// ───────────────────────── カテゴリ統計（KPIベンチマーク） ─────────────────────────

/// 全職種（505 職業）の中央値統計。起動時に 1 回だけ計算してキャッシュする。
#[derive(Serialize, Clone, Default)]
pub struct OverallStats {
    pub sample_size: i64,
    pub median_avg_age: Option<f64>,
    pub median_annual_salary_man_yen: Option<f64>,
    pub median_workers_count_tenfold: Option<f64>,
    pub median_scheduled_hours: Option<f64>,
}

static OVERALL_CACHE: OnceLock<OverallStats> = OnceLock::new();

/// 全職種の総計行から各指標の中央値を計算する。
/// wage_census_code が空の職業（賃金センサス対象外）は除外される。
fn fetch_overall_stats(turso: &TursoDb) -> Result<OverallStats, String> {
    let rows = turso.query(
        "SELECT w.annual_salary_man_yen, w.avg_age, w.workers_count_tenfold, w.scheduled_hours \
         FROM v2_external_jobtag_occupation o \
         JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE o.wage_census_code <> ''",
        &[],
    )?;

    fn median(mut values: Vec<f64>) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = values.len() / 2;
        Some(if values.len() % 2 == 0 {
            (values[mid - 1] + values[mid]) / 2.0
        } else {
            values[mid]
        })
    }

    let salaries: Vec<f64> = rows
        .iter()
        .filter_map(|r| f(r, "annual_salary_man_yen"))
        .collect();
    let ages: Vec<f64> = rows.iter().filter_map(|r| f(r, "avg_age")).collect();
    let workers: Vec<f64> = rows
        .iter()
        .filter_map(|r| f(r, "workers_count_tenfold"))
        .collect();
    let hours: Vec<f64> = rows
        .iter()
        .filter_map(|r| f(r, "scheduled_hours"))
        .collect();

    Ok(OverallStats {
        sample_size: rows.len() as i64,
        median_avg_age: median(ages),
        median_annual_salary_man_yen: median(salaries),
        median_workers_count_tenfold: median(workers),
        median_scheduled_hours: median(hours),
    })
}

/// 全職種中央値を取得する。初回呼び出し時のみ DB から計算し、以降はキャッシュを返す。
pub fn get_overall_stats(turso: &TursoDb) -> &'static OverallStats {
    OVERALL_CACHE.get_or_init(|| {
        fetch_overall_stats(turso).unwrap_or_else(|e| {
            tracing::warn!("fetch_overall_stats failed (using default): {e}");
            OverallStats::default()
        })
    })
}

/// 同カテゴリ内の中央値統計。個別カルテで KPI ベンチマーク表示に使う。
#[derive(Serialize, Clone, Default)]
pub struct CategoryStats {
    pub category: String,
    pub median_annual_salary_man_yen: Option<f64>,
    pub median_avg_age: Option<f64>,
    pub median_workers_count_tenfold: Option<f64>,
    pub sample_size: i64,
}

/// 同カテゴリの「総計」行から median を計算する。
/// 賃金センサス対象外職業（wage_census_code 空）は除外される。
pub fn fetch_category_stats(turso: &TursoDb, category: &str) -> Result<CategoryStats, String> {
    if category.is_empty() {
        return Ok(CategoryStats::default());
    }
    let rows = turso.query(
        "SELECT w.annual_salary_man_yen, w.avg_age, w.workers_count_tenfold \
         FROM v2_external_jobtag_occupation o \
         JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE o.category = ? AND o.wage_census_code <> ''",
        &[&category.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;

    fn median(mut values: Vec<f64>) -> Option<f64> {
        if values.is_empty() {
            return None;
        }
        values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        let mid = values.len() / 2;
        Some(if values.len() % 2 == 0 {
            (values[mid - 1] + values[mid]) / 2.0
        } else {
            values[mid]
        })
    }

    let salaries: Vec<f64> = rows
        .iter()
        .filter_map(|r| f(r, "annual_salary_man_yen"))
        .collect();
    let ages: Vec<f64> = rows.iter().filter_map(|r| f(r, "avg_age")).collect();
    let workers: Vec<f64> = rows
        .iter()
        .filter_map(|r| f(r, "workers_count_tenfold"))
        .collect();

    Ok(CategoryStats {
        category: category.to_string(),
        median_annual_salary_man_yen: median(salaries),
        median_avg_age: median(ages),
        median_workers_count_tenfold: median(workers),
        sample_size: rows.len() as i64,
    })
}
