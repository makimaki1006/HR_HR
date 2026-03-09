use serde_json::Value;

use crate::AppState;
use crate::handlers::overview::SessionFilters;
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
    pub(crate) working_hours: String,
    // 採用関連フィールド
    pub(crate) license_1: String,
    pub(crate) license_2: String,
    pub(crate) license_3: String,
    pub(crate) occupation_major: String,
    pub(crate) education_required: String,
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
/// 3つのクエリを1つに統合し、JSON_GROUP_ARRAYでpref_rankingを取得
pub(crate) fn fetch_competitive(state: &AppState, filters: &SessionFilters) -> CompStats {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return CompStats::default(),
    };

    // フィルタ句を一度だけ構築し、メインクエリとサブクエリの両方に使う
    let mut filter_fragment = String::new();
    let mut filter_params: Vec<String> = Vec::new();
    filters.append_industry_filter_str(&mut filter_fragment, &mut filter_params);

    // 統合クエリ: COUNT(*), COUNT(DISTINCT facility_name), pref_ranking を1回で取得
    let sql = format!(
        "SELECT \
           COUNT(*) as total_cnt, \
           COUNT(DISTINCT facility_name) as fac_cnt, \
           (SELECT JSON_GROUP_ARRAY(JSON_OBJECT('pref', sub.prefecture, 'cnt', sub.c)) \
            FROM ( \
              SELECT prefecture, COUNT(*) as c FROM postings \
              WHERE 1=1{filter} AND prefecture IS NOT NULL AND prefecture != '' \
              GROUP BY prefecture ORDER BY c DESC LIMIT 15 \
            ) sub \
           ) as pref_ranking_json \
         FROM postings WHERE 1=1{filter}",
        filter = filter_fragment
    );

    // パラメータはサブクエリ分 + メインクエリ分の2セット必要
    let mut params: Vec<String> = Vec::new();
    params.extend(filter_params.iter().cloned()); // サブクエリ用
    params.extend(filter_params);                 // メインクエリ用
    let bind: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();

    let rows = match db.query(&sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("fetch_competitive統合クエリ失敗: {e}");
            return CompStats::default();
        }
    };

    let mut stats = CompStats::default();
    if let Some(row) = rows.first() {
        stats.total_postings = row.get("total_cnt")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);
        stats.total_facilities = row.get("fac_cnt")
            .and_then(|v| v.as_i64())
            .unwrap_or(0);

        // JSON文字列からpref_rankingをパース
        if let Some(json_str) = row.get("pref_ranking_json").and_then(|v| v.as_str()) {
            if let Ok(arr) = serde_json::from_str::<Vec<serde_json::Value>>(json_str) {
                for item in &arr {
                    let pref = item.get("pref")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();
                    let cnt = item.get("cnt")
                        .and_then(|v| v.as_i64())
                        .unwrap_or(0);
                    if !pref.is_empty() {
                        stats.pref_ranking.push((pref, cnt));
                    }
                }
            }
        }
    }

    stats
}

/// 都道府県一覧
/// job_typeが空の場合は全体から取得
pub(crate) fn fetch_prefectures(state: &AppState, filters: &SessionFilters) -> Vec<String> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };

    let mut sql = "SELECT DISTINCT prefecture FROM postings WHERE 1=1".to_string();
    let mut param_values: Vec<String> = Vec::new();
    filters.append_industry_filter_str(&mut sql, &mut param_values);
    sql.push_str(" ORDER BY prefecture");

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

/// 求人一覧取得（ヘッダーフィルタ + 追加フィルタ + ページネーション）
/// job_typeが空の場合は全産業対象
/// page/page_sizeが指定された場合はLIMIT/OFFSETでSQLレベルのページネーションを行う
pub(crate) fn fetch_postings(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
    pref: &str,
    muni: Option<&str>,
    emp: &str,
    page: Option<i64>,
    page_size: Option<i64>,
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
         COALESCE(working_hours,'') as working_hours, \
         COALESCE(license_1,'') as license_1, \
         COALESCE(license_2,'') as license_2, \
         COALESCE(license_3,'') as license_3, \
         COALESCE(occupation_major,'') as occupation_major, \
         COALESCE(education_required,'') as education_required \
         FROM postings WHERE prefecture = ?"
    );
    let mut param_values: Vec<String> = vec![pref.to_string()];

    // 産業フィルタ
    filters.append_industry_filter_str(&mut sql, &mut param_values);
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

    // LIMIT/OFFSETによるSQLレベルのページネーション
    if let (Some(p), Some(ps)) = (page, page_size) {
        let offset = (p.max(1) - 1) * ps;
        sql.push_str(&format!(" LIMIT {} OFFSET {}", ps, offset));
    }

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

/// 求人件数のみ取得（ページネーション用カウントクエリ）
/// fetch_postingsと同じWHERE条件だがSELECT COUNT(*)のみで効率的
pub(crate) fn count_postings(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
    pref: &str,
    muni: Option<&str>,
    emp: &str,
) -> i64 {
    let mut sql = String::from(
        "SELECT COUNT(*) as cnt FROM postings WHERE prefecture = ?"
    );
    let mut param_values: Vec<String> = vec![pref.to_string()];

    filters.append_industry_filter_str(&mut sql, &mut param_values);
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

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    match db.query(&sql, &params) {
        Ok(rows) => rows.first()
            .and_then(|r| r.get("cnt"))
            .map(value_to_i64)
            .unwrap_or(0),
        Err(e) => {
            tracing::error!("Count query failed: {e}");
            0
        }
    }
}

/// 給与統計をSQLで直接計算（全件メモリロードを回避）
/// calc_salary_statsと同じSalaryStatsを返すが、SQL集計で効率的に計算
pub(crate) fn fetch_salary_stats_sql(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
    pref: &str,
    muni: Option<&str>,
    emp: &str,
) -> SalaryStats {
    // WHERE句を構築（fetch_postingsと同じ条件）
    let mut where_clause = String::from(" WHERE prefecture = ?");
    let mut param_values: Vec<String> = vec![pref.to_string()];

    filters.append_industry_filter_str(&mut where_clause, &mut param_values);
    if let Some(m) = muni {
        if !m.is_empty() {
            where_clause.push_str(" AND municipality = ?");
            param_values.push(m.to_string());
        }
    }
    if !emp.is_empty() && emp != "全て" {
        where_clause.push_str(" AND employment_type = ?");
        param_values.push(emp.to_string());
    }

    let empty_stats = SalaryStats {
        count: 0,
        salary_min_median: "-".to_string(),
        salary_min_avg: "-".to_string(),
        salary_min_mode: "-".to_string(),
        salary_max_median: "-".to_string(),
        salary_max_avg: "-".to_string(),
        salary_max_mode: "-".to_string(),
        bonus_rate: "-".to_string(),
        avg_holidays: "-".to_string(),
        has_data: false,
    };

    // 全体件数を先に取得
    let total_count = {
        let sql = format!("SELECT COUNT(*) as cnt FROM postings{}", where_clause);
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("cnt"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => return empty_stats,
        }
    };

    if total_count == 0 {
        return empty_stats;
    }

    // salary_min の集計統計（AVG）
    let sal_min_filter = format!("{} AND salary_min >= 50000", where_clause);
    let sal_max_filter = format!("{} AND salary_max >= 50000", where_clause);

    // クエリ1: salary_min の基本統計（件数, 平均）
    let (min_count, min_avg) = {
        let sql = format!(
            "SELECT COUNT(*) as cnt, ROUND(AVG(salary_min)) as avg_sal \
             FROM postings{}",
            sal_min_filter
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => {
                let row = rows.first();
                let cnt = row.and_then(|r| r.get("cnt")).map(value_to_i64).unwrap_or(0);
                let avg = row.and_then(|r| r.get("avg_sal")).map(value_to_i64).unwrap_or(0);
                (cnt, avg)
            }
            Err(_) => (0, 0),
        }
    };

    // クエリ2: salary_max の基本統計（件数, 平均）
    let (max_count, max_avg) = {
        let sql = format!(
            "SELECT COUNT(*) as cnt, ROUND(AVG(salary_max)) as avg_sal \
             FROM postings{}",
            sal_max_filter
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => {
                let row = rows.first();
                let cnt = row.and_then(|r| r.get("cnt")).map(value_to_i64).unwrap_or(0);
                let avg = row.and_then(|r| r.get("avg_sal")).map(value_to_i64).unwrap_or(0);
                (cnt, avg)
            }
            Err(_) => (0, 0),
        }
    };

    if min_count == 0 && max_count == 0 {
        return SalaryStats {
            count: total_count,
            has_data: false,
            ..empty_stats
        };
    }

    // クエリ3: salary_min の中央値（LIMIT 1 OFFSET count/2）
    let min_median = if min_count > 0 {
        let offset = min_count / 2;
        let sql = format!(
            "SELECT salary_min FROM postings{} ORDER BY salary_min LIMIT 1 OFFSET {}",
            sal_min_filter, offset
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("salary_min"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => 0,
        }
    } else { 0 };

    // クエリ4: salary_max の中央値
    let max_median = if max_count > 0 {
        let offset = max_count / 2;
        let sql = format!(
            "SELECT salary_max FROM postings{} ORDER BY salary_max LIMIT 1 OFFSET {}",
            sal_max_filter, offset
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("salary_max"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => 0,
        }
    } else { 0 };

    // クエリ5: salary_min の最頻値（1万円帯）
    let min_mode = if min_count > 0 {
        let sql = format!(
            "SELECT CAST(ROUND(salary_min / 10000.0) * 10000 AS INTEGER) as band, COUNT(*) as cnt \
             FROM postings{} GROUP BY band ORDER BY cnt DESC LIMIT 1",
            sal_min_filter
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("band"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => 0,
        }
    } else { 0 };

    // クエリ6: salary_max の最頻値（1万円帯）
    let max_mode = if max_count > 0 {
        let sql = format!(
            "SELECT CAST(ROUND(salary_max / 10000.0) * 10000 AS INTEGER) as band, COUNT(*) as cnt \
             FROM postings{} GROUP BY band ORDER BY cnt DESC LIMIT 1",
            sal_max_filter
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("band"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => 0,
        }
    } else { 0 };

    // クエリ7: 賞与率（benefitsに「賞与」を含む割合）
    let bonus_rate = {
        let sql = format!(
            "SELECT COUNT(*) as cnt FROM postings{} AND benefits LIKE '%賞与%'",
            where_clause
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        let bonus_count = match db.query(&sql, &params) {
            Ok(rows) => rows.first()
                .and_then(|r| r.get("cnt"))
                .map(value_to_i64)
                .unwrap_or(0),
            Err(_) => 0,
        };
        if total_count > 0 {
            format!("{:.0}%", bonus_count as f64 / total_count as f64 * 100.0)
        } else {
            "-".to_string()
        }
    };

    // クエリ8: 平均休日数
    let avg_holidays = {
        let sql = format!(
            "SELECT ROUND(AVG(annual_holidays)) as avg_hol \
             FROM postings{} AND annual_holidays >= 80 AND annual_holidays <= 200",
            where_clause
        );
        let params: Vec<&dyn rusqlite::types::ToSql> = param_values
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        match db.query(&sql, &params) {
            Ok(rows) => {
                let val = rows.first()
                    .and_then(|r| r.get("avg_hol"))
                    .map(value_to_i64)
                    .unwrap_or(0);
                if val > 0 { format!("{}日", val) } else { "-".to_string() }
            }
            Err(_) => "-".to_string(),
        }
    };

    // フォーマットして返す
    use crate::handlers::overview::format_number;
    let fmt = |v: i64| -> String {
        if v > 0 { format!("{}円", format_number(v)) } else { "-".to_string() }
    };

    SalaryStats {
        count: total_count,
        salary_min_median: fmt(min_median),
        salary_min_avg: fmt(min_avg),
        salary_min_mode: fmt(min_mode),
        salary_max_median: fmt(max_median),
        salary_max_avg: fmt(max_avg),
        salary_max_mode: fmt(max_mode),
        bonus_rate,
        avg_holidays,
        has_data: min_count > 0,
    }
}

/// 近隣求人取得（半径検索）
pub(crate) fn fetch_nearby_postings(
    db: &crate::db::local_sqlite::LocalDb,
    filters: &SessionFilters,
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
         COALESCE(license_1,'') as license_1, \
         COALESCE(license_2,'') as license_2, \
         COALESCE(license_3,'') as license_3, \
         COALESCE(occupation_major,'') as occupation_major, \
         COALESCE(education_required,'') as education_required, \
         latitude, longitude \
         FROM postings WHERE \
         latitude BETWEEN ? AND ? AND longitude BETWEEN ? AND ?"
    );
    // REAL列にはREAL型でバインド（String→TEXT型だとBETWEENが常にFALSEになる）
    use rusqlite::types::Value as SqlValue;
    let mut param_values: Vec<SqlValue> = vec![
        SqlValue::Real(lat_min),
        SqlValue::Real(lat_max),
        SqlValue::Real(lng_min),
        SqlValue::Real(lng_max),
    ];

    // 産業フィルタ（大分類+中分類混合時はOR結合）
    {
        let has_jt = !filters.job_types.is_empty();
        let has_ir = !filters.industry_raws.is_empty();
        if has_jt && has_ir {
            let jt_ph = vec!["?"; filters.job_types.len()].join(",");
            let ir_ph = vec!["?"; filters.industry_raws.len()].join(",");
            sql.push_str(&format!(" AND (job_type IN ({}) OR industry_raw IN ({}))", jt_ph, ir_ph));
            param_values.extend(filters.job_types.iter().map(|s| SqlValue::Text(s.clone())));
            param_values.extend(filters.industry_raws.iter().map(|s| SqlValue::Text(s.clone())));
        } else if has_ir {
            let placeholders = vec!["?"; filters.industry_raws.len()].join(",");
            sql.push_str(&format!(" AND industry_raw IN ({})", placeholders));
            param_values.extend(filters.industry_raws.iter().map(|s| SqlValue::Text(s.clone())));
        } else if has_jt {
            let placeholders = vec!["?"; filters.job_types.len()].join(",");
            sql.push_str(&format!(" AND job_type IN ({})", placeholders));
            param_values.extend(filters.job_types.iter().map(|s| SqlValue::Text(s.clone())));
        }
    }
    if !emp.is_empty() && emp != "全て" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(SqlValue::Text(emp.to_string()));
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
        license_1: r.get("license_1").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        license_2: r.get("license_2").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        license_3: r.get("license_3").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        occupation_major: r.get("occupation_major").and_then(|v| v.as_str()).unwrap_or("").to_string(),
        education_required: r.get("education_required").and_then(|v| v.as_str()).unwrap_or("").to_string(),
    }
}
