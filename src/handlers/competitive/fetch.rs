use serde_json::Value;

use crate::AppState;
use super::utils::{value_to_i64, haversine};

// --- 内部データ型 ---

pub(crate) struct CompStats {
    pub(crate) total_postings: i64,
    pub(crate) total_facilities: i64,
    pub(crate) pref_ranking: Vec<(String, i64)>,
}

impl Default for CompStats {
    fn default() -> Self {
        Self {
            total_postings: 0,
            total_facilities: 0,
            pref_ranking: Vec::new(),
        }
    }
}

pub(crate) struct PostingRow {
    pub(crate) facility_name: String,
    pub(crate) job_type: String,
    pub(crate) prefecture: String,
    pub(crate) municipality: String,
    pub(crate) employment_type: String,
    pub(crate) salary_type: String,
    pub(crate) salary_min: i64,
    pub(crate) salary_max: i64,
    pub(crate) requirements: String,
    pub(crate) annual_holidays: i64,
    pub(crate) distance_km: Option<f64>,
    pub(crate) tier3_label_short: String,
    // Hello Work固有フィールド
    pub(crate) job_number: String,
    pub(crate) hello_work_office: String,
    pub(crate) recruitment_reason: String,
    pub(crate) benefits: String,
    #[allow(dead_code)]
    pub(crate) working_hours: String,
}

pub(crate) struct SalaryStats {
    pub(crate) count: i64,
    pub(crate) salary_min_median: String,
    pub(crate) salary_min_avg: String,
    pub(crate) salary_min_mode: String,
    pub(crate) salary_max_median: String,
    pub(crate) salary_max_avg: String,
    pub(crate) salary_max_mode: String,
    #[allow(dead_code)]
    pub(crate) bonus_rate: String,
    pub(crate) avg_holidays: String,
    pub(crate) has_data: bool,
}

// --- データ取得関数 ---

/// 競合調査の基本統計
/// job_typeが空の場合は全体集計
pub(crate) fn fetch_competitive(state: &AppState, job_type: &str) -> CompStats {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return CompStats::default(),
    };

    let mut stats = CompStats::default();

    if job_type.is_empty() {
        stats.total_postings = db
            .query_scalar::<i64>(
                "SELECT COUNT(*) FROM postings",
                &[],
            )
            .unwrap_or(0);

        stats.total_facilities = db
            .query_scalar::<i64>(
                "SELECT COUNT(DISTINCT facility_name) FROM postings",
                &[],
            )
            .unwrap_or(0);

        if let Ok(rows) = db.query(
            "SELECT prefecture, COUNT(*) as cnt FROM postings GROUP BY prefecture ORDER BY cnt DESC LIMIT 15",
            &[],
        ) {
            for row in &rows {
                let pref = row.get("prefecture")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cnt = row.get("cnt")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if !pref.is_empty() {
                    stats.pref_ranking.push((pref, cnt));
                }
            }
        }
    } else {
        stats.total_postings = db
            .query_scalar::<i64>(
                "SELECT COUNT(*) FROM postings WHERE job_type = ?",
                &[&job_type as &dyn rusqlite::types::ToSql],
            )
            .unwrap_or(0);

        stats.total_facilities = db
            .query_scalar::<i64>(
                "SELECT COUNT(DISTINCT facility_name) FROM postings WHERE job_type = ?",
                &[&job_type as &dyn rusqlite::types::ToSql],
            )
            .unwrap_or(0);

        if let Ok(rows) = db.query(
            "SELECT prefecture, COUNT(*) as cnt FROM postings WHERE job_type = ? GROUP BY prefecture ORDER BY cnt DESC LIMIT 15",
            &[&job_type as &dyn rusqlite::types::ToSql],
        ) {
            for row in &rows {
                let pref = row.get("prefecture")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                let cnt = row.get("cnt")
                    .and_then(|v| v.as_i64())
                    .unwrap_or(0);
                if !pref.is_empty() {
                    stats.pref_ranking.push((pref, cnt));
                }
            }
        }
    }

    stats
}

/// 都道府県一覧
/// job_typeが空の場合は全体から取得
pub(crate) fn fetch_prefectures(state: &AppState, job_type: &str) -> Vec<String> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };

    let (sql, param_values) = if job_type.is_empty() {
        (
            "SELECT DISTINCT prefecture FROM postings ORDER BY prefecture".to_string(),
            vec![],
        )
    } else {
        (
            "SELECT DISTINCT prefecture FROM postings WHERE job_type = ? ORDER BY prefecture".to_string(),
            vec![job_type.to_string()],
        )
    };

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &params).unwrap_or_default();

    rows.iter()
        .filter_map(|r| r.get("prefecture").and_then(|v| v.as_str()).map(|s| s.to_string()))
        .collect()
}

/// 産業（job_type）一覧取得（競合調査フィルタ用）
pub(crate) fn fetch_job_types(state: &AppState, pref: &str) -> Vec<(String, i64)> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };

    let (sql, param_values) = if pref.is_empty() {
        (
            "SELECT job_type, COUNT(*) as cnt \
             FROM postings WHERE job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC".to_string(),
            vec![],
        )
    } else {
        (
            "SELECT job_type, COUNT(*) as cnt \
             FROM postings WHERE prefecture = ? AND job_type IS NOT NULL AND job_type != '' \
             GROUP BY job_type ORDER BY cnt DESC".to_string(),
            vec![pref.to_string()],
        )
    };

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &params_ref).unwrap_or_default();

    rows.iter()
        .filter_map(|r| {
            let jt = r.get("job_type").and_then(|v| v.as_str())?.to_string();
            let cnt = r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
            if jt.is_empty() { None } else { Some((jt, cnt)) }
        })
        .collect()
}

/// 求人一覧取得（ヘッダーフィルタ + 追加フィルタ）
/// job_typeが空の場合は全産業対象
pub(crate) fn fetch_postings(
    db: &crate::db::local_sqlite::LocalDb,
    job_type: &str,
    pref: &str,
    muni: Option<&str>,
    emp: &str,
) -> Vec<PostingRow> {
    let mut sql = String::from(
        "SELECT facility_name, job_type, prefecture, municipality, employment_type, \
         salary_type, salary_min, salary_max, requirements, \
         annual_holidays, \
         COALESCE(tier3_label_short,'') as tier3_label_short, \
         COALESCE(job_number,'') as job_number, \
         COALESCE(hello_work_office,'') as hello_work_office, \
         COALESCE(recruitment_reason,'') as recruitment_reason, \
         COALESCE(benefits,'') as benefits, \
         COALESCE(working_hours,'') as working_hours \
         FROM postings WHERE prefecture = ?"
    );
    let mut param_values: Vec<String> = vec![pref.to_string()];

    if !job_type.is_empty() {
        sql.push_str(" AND job_type = ?");
        param_values.push(job_type.to_string());
    }
    if let Some(m) = muni {
        if !m.is_empty() {
            sql.push_str(" AND municipality = ?");
            param_values.push(m.to_string());
        }
    }
    if !emp.is_empty() && emp != "全て" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(emp.to_string());
    }
    sql.push_str(" ORDER BY salary_min DESC");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Posting query failed: {e}");
            return Vec::new();
        }
    };

    rows.iter().map(|r| row_to_posting(r, None)).collect()
}

/// 近隣求人取得（半径検索）
pub(crate) fn fetch_nearby_postings(
    db: &crate::db::local_sqlite::LocalDb,
    job_type: &str,
    pref: &str,
    muni: &str,
    radius_km: f64,
    emp: &str,
) -> Vec<PostingRow> {
    let center = match get_geocode(db, pref, muni) {
        Some(c) => c,
        None => return Vec::new(),
    };

    let lat_delta = radius_km / 111.0;
    let lng_delta = radius_km / (111.0 * center.0.to_radians().cos());
    let lat_min = center.0 - lat_delta;
    let lat_max = center.0 + lat_delta;
    let lng_min = center.1 - lng_delta;
    let lng_max = center.1 + lng_delta;

    let mut sql = String::from(
        "SELECT facility_name, job_type, prefecture, municipality, employment_type, \
         salary_type, salary_min, salary_max, requirements, \
         annual_holidays, \
         COALESCE(tier3_label_short,'') as tier3_label_short, \
         COALESCE(job_number,'') as job_number, \
         COALESCE(hello_work_office,'') as hello_work_office, \
         COALESCE(recruitment_reason,'') as recruitment_reason, \
         COALESCE(benefits,'') as benefits, \
         COALESCE(working_hours,'') as working_hours, \
         latitude, longitude \
         FROM postings WHERE \
         latitude BETWEEN ? AND ? AND longitude BETWEEN ? AND ?"
    );
    let mut param_values: Vec<String> = vec![
        lat_min.to_string(), lat_max.to_string(),
        lng_min.to_string(), lng_max.to_string(),
    ];

    if !job_type.is_empty() {
        sql.push_str(" AND job_type = ?");
        param_values.push(job_type.to_string());
    }
    if !emp.is_empty() && emp != "全て" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(emp.to_string());
    }
    sql.push_str(" ORDER BY salary_min DESC");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("Nearby query failed: {e}");
            return Vec::new();
        }
    };

    rows.iter()
        .filter_map(|r| {
            let lat = r.get("latitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
            let lng = r.get("longitude").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if lat == 0.0 || lng == 0.0 {
                return None;
            }
            let dist = haversine(center.0, center.1, lat, lng);
            if dist <= radius_km {
                Some(row_to_posting(r, Some(dist)))
            } else {
                None
            }
        })
        .collect()
}

pub(crate) fn get_geocode(db: &crate::db::local_sqlite::LocalDb, pref: &str, muni: &str) -> Option<(f64, f64)> {
    let rows = db.query(
        "SELECT latitude, longitude FROM municipality_geocode WHERE prefecture = ? AND municipality = ?",
        &[&pref as &dyn rusqlite::types::ToSql, &muni as &dyn rusqlite::types::ToSql],
    ).ok()?;

    let row = rows.first()?;
    let lat = row.get("latitude").and_then(|v| v.as_f64())?;
    let lng = row.get("longitude").and_then(|v| v.as_f64())?;
    Some((lat, lng))
}

fn row_to_posting(r: &std::collections::HashMap<String, Value>, distance: Option<f64>) -> PostingRow {
    PostingRow {
        facility_name: r.get("facility_name").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        job_type: r.get("job_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        prefecture: r.get("prefecture").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        municipality: r.get("municipality").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        employment_type: r.get("employment_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        salary_type: r.get("salary_type").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        salary_min: r.get("salary_min").map(value_to_i64).unwrap_or(0),
        salary_max: r.get("salary_max").map(value_to_i64).unwrap_or(0),
        requirements: r.get("requirements").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        annual_holidays: r.get("annual_holidays").map(value_to_i64).unwrap_or(0),
        distance_km: distance,
        tier3_label_short: r.get("tier3_label_short").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        job_number: r.get("job_number").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        hello_work_office: r.get("hello_work_office").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        recruitment_reason: r.get("recruitment_reason").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        benefits: r.get("benefits").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        working_hours: r.get("working_hours").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }
}
