use serde::Serialize;
use serde_json::Value;

use crate::db::local_sqlite::LocalDb;
use crate::handlers::overview::SessionFilters;

/// マーカー表示用の軽量データ
#[derive(Serialize)]
pub(crate) struct MarkerRow {
    pub(crate) id: i64,
    pub(crate) lat: f64,
    pub(crate) lng: f64,
    #[serde(rename = "facility")]
    pub(crate) facility_name: String,
    #[serde(rename = "jobType")]
    pub(crate) job_type: String,
    #[serde(rename = "emp")]
    pub(crate) employment_type: String,
    #[serde(rename = "salaryType")]
    pub(crate) salary_type: String,
    #[serde(rename = "salaryMin")]
    pub(crate) salary_min: i64,
    #[serde(rename = "salaryMax")]
    pub(crate) salary_max: i64,
}

/// 詳細カード用の全カラムデータ
#[allow(dead_code)]
pub(crate) struct DetailRow {
    pub(crate) id: i64,
    pub(crate) job_type: String,
    pub(crate) prefecture: String,
    pub(crate) municipality: String,
    pub(crate) facility_name: String,
    pub(crate) employment_type: String,
    pub(crate) salary_type: String,
    pub(crate) salary_min: i64,
    pub(crate) salary_max: i64,
    pub(crate) headline: String,
    pub(crate) job_description: String,
    pub(crate) requirements: String,
    pub(crate) benefits: String,
    pub(crate) working_hours: String,
    pub(crate) holidays: String,
    pub(crate) access: String,
    pub(crate) tier3_label_short: String,
    pub(crate) lat: f64,
    pub(crate) lng: f64,
    // Hello Work固有フィールド
    pub(crate) job_number: String,
    pub(crate) hello_work_office: String,
    pub(crate) recruitment_reason: String,
}

fn value_to_i64(v: &Value) -> i64 {
    match v {
        Value::Number(n) => n.as_i64().unwrap_or(0),
        Value::String(s) => s.parse::<i64>().unwrap_or(0),
        _ => 0,
    }
}

fn value_to_f64(v: &Value) -> f64 {
    match v {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s.parse::<f64>().unwrap_or(0.0),
        _ => 0.0,
    }
}

fn value_to_str(v: Option<&Value>) -> String {
    v.and_then(|v| v.as_str()).unwrap_or("").to_string()
}

/// Haversine距離計算（km）
fn haversine_km(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371.0; // 地球の半径 (km)
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();
    let a = (d_lat / 2.0).sin().powi(2)
        + lat1.to_radians().cos() * lat2.to_radians().cos() * (d_lng / 2.0).sin().powi(2);
    let c = 2.0 * a.sqrt().asin();
    r * c
}

/// 産業フィルタをSqlValue型パラメータ付きSQLに追記するヘルパー
fn append_industry_sqlval(
    filters: &SessionFilters,
    sql: &mut String,
    params: &mut Vec<rusqlite::types::Value>,
) {
    use rusqlite::types::Value as SqlValue;
    let has_jt = !filters.job_types.is_empty();
    let has_ir = !filters.industry_raws.is_empty();
    if has_jt && has_ir {
        let jt_ph = vec!["?"; filters.job_types.len()].join(",");
        let ir_ph = vec!["?"; filters.industry_raws.len()].join(",");
        sql.push_str(&format!(
            " AND (job_type IN ({}) OR industry_raw IN ({}))",
            jt_ph, ir_ph
        ));
        params.extend(filters.job_types.iter().map(|s| SqlValue::Text(s.clone())));
        params.extend(
            filters
                .industry_raws
                .iter()
                .map(|s| SqlValue::Text(s.clone())),
        );
    } else if has_ir {
        let placeholders = vec!["?"; filters.industry_raws.len()].join(",");
        sql.push_str(&format!(" AND industry_raw IN ({})", placeholders));
        params.extend(
            filters
                .industry_raws
                .iter()
                .map(|s| SqlValue::Text(s.clone())),
        );
    } else if has_jt {
        let placeholders = vec!["?"; filters.job_types.len()].join(",");
        sql.push_str(&format!(" AND job_type IN ({})", placeholders));
        params.extend(filters.job_types.iter().map(|s| SqlValue::Text(s.clone())));
    }
}

/// Bounding Box + Haversine距離フィルタでマーカーデータを取得
/// job_typeが空の場合はjob_typeフィルタを省略する
pub(crate) fn fetch_markers(
    db: &LocalDb,
    filters: &SessionFilters,
    _prefecture: &str,
    _municipality: &str,
    employment_type: &str,
    salary_type: &str,
    lat: f64,
    lng: f64,
    radius_km: f64,
) -> (Vec<MarkerRow>, usize) {
    let lat_delta = radius_km / 111.0;
    let lng_delta = radius_km / (111.0 * lat.to_radians().cos().abs().max(0.01));
    let lat_min = lat - lat_delta;
    let lat_max = lat + lat_delta;
    let lng_min = lng - lng_delta;
    let lng_max = lng + lng_delta;

    let mut sql = String::from(
        "SELECT id, latitude, longitude, facility_name, job_type, employment_type, \
         salary_type, salary_min, salary_max \
         FROM postings WHERE \
         latitude BETWEEN ? AND ? AND longitude BETWEEN ? AND ?",
    );
    // rusqlite::types::Value を使い、REAL列にはREAL型でバインド
    // （String→TEXT型だとSQLiteの型比較ルールでBETWEENが常にFALSEになる）
    use rusqlite::types::Value as SqlValue;
    let mut param_values: Vec<SqlValue> = vec![
        SqlValue::Real(lat_min),
        SqlValue::Real(lat_max),
        SqlValue::Real(lng_min),
        SqlValue::Real(lng_max),
    ];

    // 産業フィルタ
    append_industry_sqlval(filters, &mut sql, &mut param_values);

    // GAS方式: 半径検索時は prefecture/municipality でフィルタしない
    // 中心座標 + Bounding Box + Haversine で地理的に絞る
    // （隣接県・隣接市区町村の求人も含めるため）
    if !employment_type.is_empty() && employment_type != "全て選択" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(SqlValue::Text(employment_type.to_string()));
    }
    if !salary_type.is_empty() && salary_type != "どちらも" {
        sql.push_str(" AND salary_type = ?");
        param_values.push(SqlValue::Text(salary_type.to_string()));
    }
    // 中心から近い順にソート → 5000件サンプルの代表性を向上
    sql.push_str(" ORDER BY ABS(latitude - ?) + ABS(longitude - ?)");
    param_values.push(SqlValue::Real(lat));
    param_values.push(SqlValue::Real(lng));
    // LIMIT 20000はHaversineフィルタ前の粗い取得（最終truncate 5000）
    sql.push_str(" LIMIT 20000");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("fetch_markers failed: {e}");
            return (Vec::new(), 0);
        }
    };

    // Bounding Box結果からHaversine距離で正確な円内フィルタ
    let mut result: Vec<MarkerRow> = rows
        .iter()
        .filter_map(|r| {
            let m_lat = r.get("latitude").map(value_to_f64).unwrap_or(0.0);
            let m_lng = r.get("longitude").map(value_to_f64).unwrap_or(0.0);
            let dist = haversine_km(lat, lng, m_lat, m_lng);
            if dist <= radius_km {
                Some(MarkerRow {
                    id: r.get("id").map(value_to_i64).unwrap_or(0),
                    lat: m_lat,
                    lng: m_lng,
                    facility_name: value_to_str(r.get("facility_name")),
                    job_type: value_to_str(r.get("job_type")),
                    employment_type: value_to_str(r.get("employment_type")),
                    salary_type: value_to_str(r.get("salary_type")),
                    salary_min: r.get("salary_min").map(value_to_i64).unwrap_or(0),
                    salary_max: r.get("salary_max").map(value_to_i64).unwrap_or(0),
                })
            } else {
                None
            }
        })
        .collect();

    // パフォーマンス最適化: 表示上限5000件（Canvas + LayerGroup）
    let total_available = result.len();
    result.truncate(5000);
    (result, total_available)
}

/// ビューポート矩形でマーカーを取得（パン/ズーム時の動的ロード用）
pub(crate) fn fetch_markers_by_bounds(
    db: &LocalDb,
    filters: &SessionFilters,
    employment_type: &str,
    salary_type: &str,
    south: f64,
    north: f64,
    west: f64,
    east: f64,
) -> (Vec<MarkerRow>, usize) {
    let mut sql = String::from(
        "SELECT id, latitude, longitude, facility_name, job_type, employment_type, \
         salary_type, salary_min, salary_max \
         FROM postings WHERE \
         latitude BETWEEN ? AND ? AND longitude BETWEEN ? AND ?",
    );
    use rusqlite::types::Value as SqlValue;
    let mut param_values: Vec<SqlValue> = vec![
        SqlValue::Real(south),
        SqlValue::Real(north),
        SqlValue::Real(west),
        SqlValue::Real(east),
    ];

    // 産業フィルタ
    append_industry_sqlval(filters, &mut sql, &mut param_values);
    if !employment_type.is_empty() && employment_type != "全て選択" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(SqlValue::Text(employment_type.to_string()));
    }
    if !salary_type.is_empty() && salary_type != "どちらも" {
        sql.push_str(" AND salary_type = ?");
        param_values.push(SqlValue::Text(salary_type.to_string()));
    }
    sql.push_str(" LIMIT 5000");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("fetch_markers_by_bounds failed: {e}");
            return (Vec::new(), 0);
        }
    };

    let mut result: Vec<MarkerRow> = rows
        .iter()
        .map(|r| MarkerRow {
            id: r.get("id").map(value_to_i64).unwrap_or(0),
            lat: r.get("latitude").map(value_to_f64).unwrap_or(0.0),
            lng: r.get("longitude").map(value_to_f64).unwrap_or(0.0),
            facility_name: value_to_str(r.get("facility_name")),
            job_type: value_to_str(r.get("job_type")),
            employment_type: value_to_str(r.get("employment_type")),
            salary_type: value_to_str(r.get("salary_type")),
            salary_min: r.get("salary_min").map(value_to_i64).unwrap_or(0),
            salary_max: r.get("salary_max").map(value_to_i64).unwrap_or(0),
        })
        .collect();

    let total_available = result.len();
    result.truncate(5000);
    (result, total_available)
}

/// 都道府県指定でマーカーを取得（半径なし・Bounding Boxなし）
/// job_typeが空の場合はjob_typeフィルタを省略する
pub(crate) fn fetch_markers_by_pref(
    db: &LocalDb,
    filters: &SessionFilters,
    prefecture: &str,
    municipality: &str,
    employment_type: &str,
    salary_type: &str,
) -> (Vec<MarkerRow>, usize) {
    let mut sql = String::from(
        "SELECT id, latitude, longitude, facility_name, job_type, employment_type, \
         salary_type, salary_min, salary_max \
         FROM postings WHERE prefecture = ? AND latitude IS NOT NULL",
    );
    let mut param_values: Vec<String> = vec![prefecture.to_string()];

    // 産業フィルタ
    filters.append_industry_filter_str(&mut sql, &mut param_values);

    if !municipality.is_empty() {
        sql.push_str(" AND municipality = ?");
        param_values.push(municipality.to_string());
    }
    if !employment_type.is_empty() && employment_type != "全て選択" {
        sql.push_str(" AND employment_type = ?");
        param_values.push(employment_type.to_string());
    }
    if !salary_type.is_empty() && salary_type != "どちらも" {
        sql.push_str(" AND salary_type = ?");
        param_values.push(salary_type.to_string());
    }
    sql.push_str(" LIMIT 50000");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("fetch_markers_by_pref failed: {e}");
            return (Vec::new(), 0);
        }
    };

    let mut result: Vec<MarkerRow> = rows
        .iter()
        .map(|r| MarkerRow {
            id: r.get("id").map(value_to_i64).unwrap_or(0),
            lat: r.get("latitude").map(value_to_f64).unwrap_or(0.0),
            lng: r.get("longitude").map(value_to_f64).unwrap_or(0.0),
            facility_name: value_to_str(r.get("facility_name")),
            job_type: value_to_str(r.get("job_type")),
            employment_type: value_to_str(r.get("employment_type")),
            salary_type: value_to_str(r.get("salary_type")),
            salary_min: r.get("salary_min").map(value_to_i64).unwrap_or(0),
            salary_max: r.get("salary_max").map(value_to_i64).unwrap_or(0),
        })
        .collect();

    // パフォーマンス最適化: 表示上限5000件
    let total_available = result.len();
    result.truncate(5000);
    (result, total_available)
}

/// 求人詳細を1件取得
pub(crate) fn fetch_detail(db: &LocalDb, posting_id: i64) -> Option<DetailRow> {
    let rows = db
        .query(
            "SELECT id, job_type, prefecture, municipality, facility_name, \
             employment_type, salary_type, salary_min, salary_max, \
             headline, job_description, requirements, benefits, working_hours, \
             holidays, access, \
             COALESCE(tier3_label_short,'') as tier3_label_short, \
             latitude, longitude, \
             COALESCE(job_number,'') as job_number, \
             COALESCE(hello_work_office,'') as hello_work_office, \
             COALESCE(recruitment_reason,'') as recruitment_reason \
             FROM postings WHERE id = ?",
            &[&posting_id as &dyn rusqlite::types::ToSql],
        )
        .ok()?;

    let r = rows.first()?;
    Some(DetailRow {
        id: r.get("id").map(value_to_i64).unwrap_or(0),
        job_type: value_to_str(r.get("job_type")),
        prefecture: value_to_str(r.get("prefecture")),
        municipality: value_to_str(r.get("municipality")),
        facility_name: value_to_str(r.get("facility_name")),
        employment_type: value_to_str(r.get("employment_type")),
        salary_type: value_to_str(r.get("salary_type")),
        salary_min: r.get("salary_min").map(value_to_i64).unwrap_or(0),
        salary_max: r.get("salary_max").map(value_to_i64).unwrap_or(0),
        headline: value_to_str(r.get("headline")),
        job_description: value_to_str(r.get("job_description")),
        requirements: value_to_str(r.get("requirements")),
        benefits: value_to_str(r.get("benefits")),
        working_hours: value_to_str(r.get("working_hours")),
        holidays: value_to_str(r.get("holidays")),
        access: value_to_str(r.get("access")),
        tier3_label_short: value_to_str(r.get("tier3_label_short")),
        lat: r.get("latitude").map(value_to_f64).unwrap_or(0.0),
        lng: r.get("longitude").map(value_to_f64).unwrap_or(0.0),
        job_number: value_to_str(r.get("job_number")),
        hello_work_office: value_to_str(r.get("hello_work_office")),
        recruitment_reason: value_to_str(r.get("recruitment_reason")),
    })
}

/// 都道府県→市区町村カスケード用データ取得
/// job_typeが空の場合はjob_typeフィルタを省略する
pub(crate) fn fetch_municipalities(
    db: &LocalDb,
    filters: &SessionFilters,
    prefecture: &str,
) -> Vec<String> {
    let mut sql = "SELECT DISTINCT municipality FROM postings \
         WHERE prefecture = ? AND municipality != ''"
        .to_string();
    let mut param_values: Vec<String> = vec![prefecture.to_string()];

    filters.append_industry_filter_str(&mut sql, &mut param_values);
    sql.push_str(" ORDER BY municipality");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &params).unwrap_or_default();

    rows.iter()
        .filter_map(|r| {
            r.get("municipality")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}

/// 市区町村の中心座標を取得
pub(crate) fn get_muni_center(
    local_db: &LocalDb,
    prefecture: &str,
    municipality: &str,
) -> Option<(f64, f64)> {
    let rows = local_db
        .query(
            "SELECT latitude, longitude FROM municipality_geocode \
             WHERE prefecture = ? AND municipality = ?",
            &[
                &prefecture as &dyn rusqlite::types::ToSql,
                &municipality as &dyn rusqlite::types::ToSql,
            ],
        )
        .ok()?;
    let r = rows.first()?;
    let lat = r.get("latitude").and_then(|v| v.as_f64())?;
    let lng = r.get("longitude").and_then(|v| v.as_f64())?;
    Some((lat, lng))
}

/// 指定職種がgeocode_dbに存在するかチェック
/// job_typeが空の場合は全体でデータ存在チェック
pub(crate) fn has_job_type_data(db: &LocalDb, filters: &SessionFilters) -> bool {
    let mut sql = "SELECT 1 FROM postings WHERE 1=1".to_string();
    let mut param_values: Vec<String> = Vec::new();
    filters.append_industry_filter_str(&mut sql, &mut param_values);
    sql.push_str(" LIMIT 1");

    let params: Vec<&dyn rusqlite::types::ToSql> = param_values
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = db.query(&sql, &params);
    matches!(rows, Ok(ref r) if !r.is_empty())
}

/// 都道府県一覧取得
/// job_typeが空の場合はjob_typeフィルタを省略する
pub(crate) fn fetch_prefectures(db: &LocalDb, filters: &SessionFilters) -> Vec<String> {
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
        .filter_map(|r| {
            r.get("prefecture")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .collect()
}
