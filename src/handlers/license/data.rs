//! 資格カルテ用 Turso データ取得。
//!
//! テーブル:
//!   v2_external_jobtag_qualifications  (jobtag_id, item_order, name)
//!   v2_external_jobtag_occupation      (jobtag_id, name, category, wage_census_code, ...)
//!   v2_external_jobtag_wage_age        (wage_census_code, age_range_order, annual_salary_man_yen, avg_age, workers_count_tenfold, ...)

use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;

use crate::db::turso_http::TursoDb;

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

// ───────────────────────── 公開データ型 ─────────────────────────

/// 一覧ページ用サマリ。
#[derive(Serialize, Clone)]
pub struct LicenseSummary {
    pub name: String,
    pub occupation_count: i64,
    pub avg_salary_man_yen: Option<f64>,
    pub wage_target_n: i64,
}

/// 詳細ページ用関連職業1行。
#[derive(Serialize, Clone)]
pub struct RelatedOccupation {
    pub jobtag_id: i64,
    pub name: String,
    pub category: String,
    pub wage_census_code: String,
    pub annual_salary_man_yen: Option<f64>,
    pub avg_age: Option<f64>,
    pub workers_count_tenfold: Option<f64>,
}

/// 詳細ページ用統計サマリ。
#[derive(Serialize, Clone, Default)]
pub struct LicenseStats {
    pub occupation_count: i64,
    pub wage_target_n: i64,
    pub median_salary_man_yen: Option<f64>,
    pub avg_age: Option<f64>,
    pub total_workers_count_tenfold: Option<f64>,
}

/// 詳細ページ用まとめ。
#[derive(Serialize)]
pub struct LicenseDetail {
    pub name: String,
    pub occupations: Vec<RelatedOccupation>,
    pub category_distribution: Vec<(String, i64)>,
    pub co_occurring_licenses: Vec<(String, i64)>,
    pub stats: LicenseStats,
}

// ───────────────────────── 一覧取得 ─────────────────────────

/// 全資格の一覧（SQLite UTF-8 コードポイント順）を取得し、avg_salary を付加する。
pub fn fetch_license_summaries(turso: &TursoDb) -> Result<Vec<LicenseSummary>, String> {
    // 1. 資格ごとの関連職業数
    let count_rows = turso.query(
        "SELECT q.name, COUNT(DISTINCT q.jobtag_id) AS oc_n \
         FROM v2_external_jobtag_qualifications q \
         GROUP BY q.name \
         ORDER BY q.name",
        &[],
    )?;

    // 2. 資格ごとの賃金センサス集計 (avg_salary / 対象者数)
    let wage_rows = turso.query(
        "SELECT q.name, \
                AVG(w.annual_salary_man_yen) AS avg_sal, \
                SUM(CASE WHEN w.annual_salary_man_yen IS NOT NULL THEN 1 ELSE 0 END) AS wage_n \
         FROM v2_external_jobtag_qualifications q \
         JOIN v2_external_jobtag_occupation o ON o.jobtag_id = q.jobtag_id \
         LEFT JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE o.wage_census_code <> '' \
         GROUP BY q.name",
        &[],
    )?;

    // wage_rows をマップ化
    let wage_map: HashMap<String, (Option<f64>, i64)> = wage_rows
        .iter()
        .map(|r| {
            let name = s(r, "name");
            let avg = f(r, "avg_sal");
            let n = i(r, "wage_n");
            (name, (avg, n))
        })
        .collect();

    let out = count_rows
        .iter()
        .map(|r| {
            let name = s(r, "name");
            let (avg_salary_man_yen, wage_target_n) =
                wage_map.get(&name).cloned().unwrap_or((None, 0));
            LicenseSummary {
                name,
                occupation_count: i(r, "oc_n"),
                avg_salary_man_yen,
                wage_target_n,
            }
        })
        .collect();

    Ok(out)
}

// ───────────────────────── 詳細取得 ─────────────────────────

/// 指定資格名の詳細データを取得する。name が存在しない場合は None。
pub fn fetch_license_detail(
    turso: &TursoDb,
    name: &str,
) -> Result<Option<LicenseDetail>, String> {
    // 1. 関連職業（賃金センサス総計行 LEFT JOIN）
    let occ_rows = turso.query(
        "SELECT o.jobtag_id, o.name, COALESCE(o.category,'') AS category, \
                COALESCE(o.wage_census_code,'') AS wage_census_code, \
                w.annual_salary_man_yen, w.avg_age, w.workers_count_tenfold \
         FROM v2_external_jobtag_qualifications q \
         JOIN v2_external_jobtag_occupation o ON o.jobtag_id = q.jobtag_id \
         LEFT JOIN v2_external_jobtag_wage_age w \
           ON w.wage_census_code = o.wage_census_code AND w.age_range_order = 0 \
         WHERE q.name = ? \
         ORDER BY o.category, o.name",
        &[&name.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;

    if occ_rows.is_empty() {
        return Ok(None);
    }

    let occupations: Vec<RelatedOccupation> = occ_rows
        .iter()
        .map(|r| RelatedOccupation {
            jobtag_id: i(r, "jobtag_id"),
            name: s(r, "name"),
            category: s(r, "category"),
            wage_census_code: s(r, "wage_census_code"),
            annual_salary_man_yen: f(r, "annual_salary_man_yen"),
            avg_age: f(r, "avg_age"),
            workers_count_tenfold: f(r, "workers_count_tenfold"),
        })
        .collect();

    // 2. カテゴリ分布
    let cat_rows = turso.query(
        "SELECT o.category, COUNT(*) AS cnt \
         FROM v2_external_jobtag_qualifications q \
         JOIN v2_external_jobtag_occupation o ON o.jobtag_id = q.jobtag_id \
         WHERE q.name = ? \
         GROUP BY o.category \
         ORDER BY cnt DESC",
        &[&name.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let category_distribution: Vec<(String, i64)> = cat_rows
        .iter()
        .map(|r| (s(r, "category"), i(r, "cnt")))
        .collect();

    // 3. 共起資格 TOP 10
    let co_rows = turso.query(
        "SELECT q2.name, COUNT(DISTINCT q2.jobtag_id) AS co_n \
         FROM v2_external_jobtag_qualifications q1 \
         JOIN v2_external_jobtag_qualifications q2 USING (jobtag_id) \
         WHERE q1.name = ? AND q2.name != q1.name \
         GROUP BY q2.name \
         ORDER BY co_n DESC \
         LIMIT 10",
        &[&name.to_string() as &dyn crate::db::turso_http::ToSqlTurso],
    )?;
    let co_occurring_licenses: Vec<(String, i64)> = co_rows
        .iter()
        .map(|r| (s(r, "name"), i(r, "co_n")))
        .collect();

    // 4. 統計（Rust 側で計算）
    let stats = compute_stats(&occupations);

    Ok(Some(LicenseDetail {
        name: name.to_string(),
        occupations,
        category_distribution,
        co_occurring_licenses,
        stats,
    }))
}

/// 関連職業リストから統計を計算する（中央値は Rust 側で算出）。
fn compute_stats(occupations: &[RelatedOccupation]) -> LicenseStats {
    let occupation_count = occupations.len() as i64;

    let salaries: Vec<f64> = occupations
        .iter()
        .filter_map(|o| o.annual_salary_man_yen)
        .collect();
    let wage_target_n = salaries.len() as i64;

    let ages: Vec<f64> = occupations.iter().filter_map(|o| o.avg_age).collect();
    let avg_age = if ages.is_empty() {
        None
    } else {
        Some(ages.iter().sum::<f64>() / ages.len() as f64)
    };

    let total_workers = occupations
        .iter()
        .filter_map(|o| o.workers_count_tenfold)
        .sum::<f64>();
    let total_workers_count_tenfold = if wage_target_n > 0 {
        Some(total_workers)
    } else {
        None
    };

    LicenseStats {
        occupation_count,
        wage_target_n,
        median_salary_man_yen: median(salaries),
        avg_age,
        total_workers_count_tenfold,
    }
}

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
