//! 地図上のSalesNow企業マーカーAPI
//! 起動時にTursoから全企業座標をメモリにロードし、
//! viewport bounding boxでフィルタして返す。

use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::sync::Arc;

use crate::AppState;

/// 企業ジオコードエントリ（メモリキャッシュ用）
#[derive(Clone, Debug, Serialize)]
pub struct CompanyGeoEntry {
    pub corporate_number: String,
    pub lat: f64,
    pub lng: f64,
    pub company_name: String,
    pub sn_industry: String,
    pub employee_count: i64,
    pub credit_score: f64,
}

#[derive(Deserialize)]
pub struct CompanyMarkerParams {
    #[serde(default)]
    pub south: Option<f64>,
    #[serde(default)]
    pub north: Option<f64>,
    #[serde(default)]
    pub west: Option<f64>,
    #[serde(default)]
    pub east: Option<f64>,
    #[serde(default)]
    pub zoom: Option<u8>,
    #[serde(default)]
    pub min_employees: Option<i64>,
}

/// 企業マーカーAPI: viewport内の企業をJSON返却
pub async fn company_markers(
    State(state): State<Arc<AppState>>,
    Query(params): Query<CompanyMarkerParams>,
) -> Json<Value> {
    let south = params.south.unwrap_or(24.0);
    let north = params.north.unwrap_or(46.0);
    let west = params.west.unwrap_or(122.0);
    let east = params.east.unwrap_or(154.0);
    let zoom = params.zoom.unwrap_or(5);
    let _min_emp = params.min_employees.unwrap_or(0);

    // zoom < 10 では企業マーカーを表示しない
    if zoom < 10 {
        return Json(json!({
            "markers": [],
            "total": 0,
            "zoom_required": 10,
            "message": "zoom >= 10 で企業マーカーが表示されます"
        }));
    }

    // メモリキャッシュがある場合はそちらを使用
    if let Some(ref cache) = state.company_geo_cache {
        let mut filtered: Vec<&CompanyGeoEntry> = cache
            .iter()
            .filter(|e| e.lat >= south && e.lat <= north && e.lng >= west && e.lng <= east)
            .collect();
        filtered.sort_by(|a, b| b.employee_count.cmp(&a.employee_count));
        let total = filtered.len();
        filtered.truncate(500);
        let markers: Vec<Value> = filtered.iter().map(|e| json!({
            "corporate_number": e.corporate_number, "lat": e.lat, "lng": e.lng,
            "company_name": e.company_name, "sn_industry": e.sn_industry,
            "employee_count": e.employee_count, "credit_score": e.credit_score,
        })).collect();
        return Json(json!({"markers": markers, "total": total, "shown": markers.len()}));
    }

    // キャッシュなし: Turso直接クエリ（オンデマンドモード）
    let sn_db = match &state.salesnow_db {
        Some(db) => db.clone(),
        None => return Json(json!({"markers": [], "total": 0, "error": "SalesNow DB未接続"})),
    };

    let s = south; let n = north; let w = west; let e = east;
    let result = tokio::task::spawn_blocking(move || {
        use crate::handlers::helpers::{get_f64, get_i64, get_str};
        let sql = r#"
            SELECT c.corporate_number, g.lat, g.lng,
                   c.company_name, c.sn_industry, c.employee_count, c.credit_score
            FROM v2_company_geocode g
            JOIN v2_salesnow_companies c ON g.corporate_number = c.corporate_number
            WHERE g.lat BETWEEN ?1 AND ?2 AND g.lng BETWEEN ?3 AND ?4
            ORDER BY c.employee_count DESC
            LIMIT 500
        "#;
        let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&s, &n, &w, &e];
        let rows = sn_db.query(sql, &params).unwrap_or_default();
        let markers: Vec<Value> = rows.iter().map(|r| json!({
            "corporate_number": get_str(r, "corporate_number"),
            "lat": get_f64(r, "lat"), "lng": get_f64(r, "lng"),
            "company_name": get_str(r, "company_name"),
            "sn_industry": get_str(r, "sn_industry"),
            "employee_count": get_i64(r, "employee_count"),
            "credit_score": get_f64(r, "credit_score"),
        })).collect();
        let total = markers.len();
        json!({"markers": markers, "total": total, "shown": total})
    }).await.unwrap_or_else(|_| json!({"markers": [], "total": 0, "error": "query failed"}));

    Json(result)
}

/// 起動時にTursoから企業ジオコードデータをロード
pub fn load_company_geo_cache(
    sn_db: &crate::db::turso_http::TursoDb,
) -> Vec<CompanyGeoEntry> {
    use crate::handlers::helpers::{get_f64, get_i64, get_str};

    let sql = r#"
        SELECT c.corporate_number, g.lat, g.lng,
               c.company_name, c.sn_industry, c.employee_count, c.credit_score
        FROM v2_company_geocode g
        JOIN v2_salesnow_companies c ON g.corporate_number = c.corporate_number
        ORDER BY c.employee_count DESC
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![];

    let rows = match sn_db.query(sql, &params) {
        Ok(rows) => rows,
        Err(e) => {
            tracing::warn!("企業ジオコードデータの読込に失敗: {e}");
            return vec![];
        }
    };

    let entries: Vec<CompanyGeoEntry> = rows
        .iter()
        .map(|r| CompanyGeoEntry {
            corporate_number: get_str(r, "corporate_number"),
            lat: get_f64(r, "lat"),
            lng: get_f64(r, "lng"),
            company_name: get_str(r, "company_name"),
            sn_industry: get_str(r, "sn_industry"),
            employee_count: get_i64(r, "employee_count"),
            credit_score: get_f64(r, "credit_score"),
        })
        .filter(|e| e.lat > 0.0 && e.lng > 0.0)
        .collect();

    tracing::info!("企業ジオコードキャッシュ: {}件ロード完了", entries.len());
    entries
}
