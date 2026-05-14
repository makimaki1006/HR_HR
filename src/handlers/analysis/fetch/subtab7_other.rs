//! サブタブ7 通勤圏分析（CommuteMunicipality + CommuteFlow + commute関数群 + 県平均補助）

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{get_f64, get_i64, get_str, haversine, table_exists};
use super::query_turso_or_local;

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

/// 通勤圏内の市区町村
pub(crate) struct CommuteMunicipality {
    pub prefecture: String,
    pub municipality: String,
    pub distance_km: f64,
    pub _lat: f64,
    pub _lng: f64,
}

/// 通勤フローレコード
pub(crate) struct CommuteFlow {
    pub partner_pref: String,
    pub partner_muni: String,
    pub total_commuters: i64,
    pub male_commuters: i64,
    pub female_commuters: i64,
}

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

pub(crate) fn fetch_commute_zone_pyramid(
    db: &Db,
    turso: Option<&TursoDb>,
    munis: &[CommuteMunicipality],
) -> Vec<Row> {
    let mut agg: HashMap<String, (i64, i64)> = HashMap::new(); // age_group -> (male_sum, female_sum)

    for m in munis {
        let rows = super::subtab5_phase4::fetch_population_pyramid(
            db,
            turso,
            &m.prefecture,
            &m.municipality,
        );
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

// 2026-05-14: 旧コードは local DB のみ参照していたため、v2_external_commute_od が
//   Turso のみ投入の本番環境では永久に空 Vec を返していた。query_turso_or_local 経由で
//   Turso 優先 + local フォールバックに変更。signature に turso 引数追加。
pub(crate) fn fetch_commute_inflow(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<CommuteFlow> {
    if muni.is_empty() {
        return vec![];
    }
    let sql = "SELECT origin_pref, origin_muni, total_commuters, male_commuters, female_commuters \
         FROM v2_external_commute_od \
         WHERE dest_pref = ?1 AND dest_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT 20";
    let params = vec![pref.to_string(), muni.to_string()];
    let rows = super::query_turso_or_local(turso, db, sql, &params, "v2_external_commute_od");

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

pub(crate) fn fetch_commute_outflow(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<CommuteFlow> {
    if muni.is_empty() {
        return vec![];
    }
    let sql = "SELECT dest_pref, dest_muni, total_commuters, male_commuters, female_commuters \
         FROM v2_external_commute_od \
         WHERE origin_pref = ?1 AND origin_muni = ?2 \
           AND (origin_pref != dest_pref OR origin_muni != dest_muni) \
         ORDER BY total_commuters DESC LIMIT 20";
    let params = vec![pref.to_string(), muni.to_string()];
    let rows = super::query_turso_or_local(turso, db, sql, &params, "v2_external_commute_od");

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

pub(crate) fn fetch_self_commute_rate(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> f64 {
    if muni.is_empty() {
        return 0.0;
    }
    let params = vec![pref.to_string(), muni.to_string()];
    let self_rows = super::query_turso_or_local(
        turso,
        db,
        "SELECT total_commuters FROM v2_external_commute_od WHERE origin_pref=?1 AND origin_muni=?2 AND dest_pref=?1 AND dest_muni=?2",
        &params,
        "v2_external_commute_od",
    );
    let self_count = self_rows
        .first()
        .map(|r| get_i64(r, "total_commuters"))
        .unwrap_or(0);

    let out_rows = super::query_turso_or_local(
        turso,
        db,
        "SELECT SUM(total_commuters) as total FROM v2_external_commute_od WHERE origin_pref=?1 AND origin_muni=?2",
        &params,
        "v2_external_commute_od",
    );
    let total_outflow = out_rows
        .first()
        .map(|r| get_i64(r, "total"))
        .unwrap_or(0);

    if total_outflow > 0 {
        self_count as f64 / total_outflow as f64
    } else {
        0.0
    }
}

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
