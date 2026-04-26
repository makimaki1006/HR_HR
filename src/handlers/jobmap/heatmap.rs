//! メッシュ人口ヒートマップ API ハンドラ
//!
//! `v2_flow_mesh1km_YYYY` × `v2_flow_attribute_mesh1km` を JOIN して
//! 1kmメッシュ中心点の緯度経度に人口を紐付けて返す。
//!
//! # 設計
//!
//! - AggregateMode で dayflag=2 / timezone=2 の double count を防御
//! - 行数爆発を防ぐため prefcode / citycode による絞り込み必須
//! - LIMIT 10000 でペイロード肥大化を防止
//! - `super::flow::resolve_table_by_year` で年別テーブル名を解決

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use super::flow::resolve_table_by_year;
use super::flow_types::{AggregateMode, FlowMeta};
use crate::AppState;

const HEATMAP_ROW_LIMIT: i64 = 10_000;

/// ヒートマップ API Query Params
#[derive(Deserialize, Debug)]
pub struct HeatmapParams {
    /// 年次 (2019 / 2020 / 2021)
    pub year: i32,
    /// 月 (1〜12)
    pub month: i32,
    /// dayflag: 0=休日, 1=平日（集計値2は禁止）
    pub dayflag: i32,
    /// timezone: 0=昼, 1=深夜（集計値2は禁止）
    pub timezone: i32,
    /// 都道府県コード (省略可、citycode が指定されていれば省略可)
    #[serde(default)]
    pub prefcode: Option<i32>,
    /// 市区町村コード (省略可、prefcode と併用可)
    #[serde(default)]
    pub citycode: Option<i64>,
}

/// GET /api/jobmap/heatmap
///
/// 1kmメッシュ人口ヒートマップを返す。
///
/// # レスポンス構造
///
/// ```json
/// {
///   "points": [{"lat": 35.6, "lng": 139.7, "population": 1234.5}, ...],
///   "max": 12345.6,
///   "meta": { "granularity": "mesh1km", "aggregate_mode": "raw", ... },
///   "year": 2021, "month": 7, "dayflag": 1, "timezone": 0,
///   "prefcode": 13, "citycode": 13101,
///   "data_count": 1234,
///   "truncated": false
/// }
/// ```
pub async fn jobmap_heatmap(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<HeatmapParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();

    // dayflag/timezone 検証（Raw モード = 生値のみ許可。集計値 2 は禁止）
    let mode = match AggregateMode::from_params(params.dayflag, params.timezone) {
        Ok(m) => m,
        Err(e) => return Json(error_response(&e.to_string())),
    };
    if mode != AggregateMode::Raw {
        return Json(error_response(
            "ヒートマップは生値のみ対応 (dayflag∈{0,1} かつ timezone∈{0,1})。集計値は double count 発生のため禁止。",
        ));
    }

    // 年次検証
    let table = match resolve_table_by_year(params.year) {
        Ok(t) => t,
        Err(_) => {
            return Json(error_response(&format!(
                "非対応年次 year={} (2019/2020/2021のみ)",
                params.year
            )))
        }
    };

    // 月検証
    if !(1..=12).contains(&params.month) {
        return Json(error_response(&format!(
            "不正な月 month={} (1〜12)",
            params.month
        )));
    }

    // prefcode / citycode のどちらか必須（行数制御のため）
    if params.prefcode.is_none() && params.citycode.is_none() {
        return Json(error_response(
            "prefcode または citycode のいずれか必須（行数制御のため）",
        ));
    }

    let year = params.year;
    let month = params.month;
    let dayflag = params.dayflag;
    let timezone = params.timezone;
    let prefcode = params.prefcode;
    let citycode = params.citycode;

    let rows = tokio::task::spawn_blocking(move || {
        query_heatmap(
            &db,
            turso.as_ref(),
            table,
            year,
            month,
            dayflag,
            timezone,
            prefcode,
            citycode,
        )
    })
    .await
    .unwrap_or_default();

    // points 構築 + max 計算
    let mut points: Vec<Value> = Vec::with_capacity(rows.len());
    let mut max_pop: f64 = 0.0;
    for r in &rows {
        let lat = super::super::helpers::get_f64(r, "center_lat");
        let lng = super::super::helpers::get_f64(r, "center_lng");
        let pop = super::super::helpers::get_f64(r, "population");
        if pop > max_pop {
            max_pop = pop;
        }
        points.push(json!({
            "lat": lat,
            "lng": lng,
            "population": pop,
        }));
    }

    let meta = FlowMeta::new("mesh1km", mode, year);
    let truncated = points.len() as i64 >= HEATMAP_ROW_LIMIT;

    Json(json!({
        "meta": meta,
        "year": year,
        "month": month,
        "dayflag": dayflag,
        "timezone": timezone,
        "prefcode": prefcode,
        "citycode": citycode,
        "data_count": points.len(),
        "truncated": truncated,
        "row_limit": HEATMAP_ROW_LIMIT,
        "max": max_pop,
        "points": points,
    }))
}

// ======== 内部ヘルパー ========

/// DB クエリ本体（spawn_blocking 内で実行）
#[allow(clippy::too_many_arguments)]
fn query_heatmap(
    db: &crate::db::local_sqlite::LocalDb,
    turso: Option<&crate::db::turso_http::TursoDb>,
    table: &str,
    year: i32,
    month: i32,
    dayflag: i32,
    timezone: i32,
    prefcode: Option<i32>,
    citycode: Option<i64>,
) -> Vec<super::super::helpers::Row> {
    let _ = year; // テーブル名に既に年が含まれる
                  // attribute_mesh1km と JOIN して lat/lng 取得
                  // 生値のみ（dayflag IN (0,1) AND timezone IN (0,1)）を強制
    let mut sql = String::from(
        "SELECT f.mesh1kmid AS mesh1kmid, \
                a.center_lat AS center_lat, \
                a.center_lng AS center_lng, \
                f.population AS population \
         FROM ",
    );
    sql.push_str(table);
    sql.push_str(
        " AS f \
         INNER JOIN v2_flow_attribute_mesh1km AS a \
           ON f.mesh1kmid = a.mesh1kmid \
         WHERE f.month = ?1 \
           AND f.dayflag = ?2 \
           AND f.timezone = ?3 ",
    );

    // 位置フィルタ（prefcode / citycode の組み合わせ）
    let mut params: Vec<String> = vec![
        format!("{:02}", month),
        dayflag.to_string(),
        timezone.to_string(),
    ];
    let mut next_idx = 4;
    if let Some(cc) = citycode {
        sql.push_str(&format!("AND f.citycode = ?{} ", next_idx));
        params.push(cc.to_string());
        next_idx += 1;
    }
    if let Some(pc) = prefcode {
        sql.push_str(&format!("AND f.prefcode = ?{} ", next_idx));
        params.push(pc.to_string());
    }

    sql.push_str(&format!(
        "ORDER BY f.population DESC LIMIT {}",
        HEATMAP_ROW_LIMIT
    ));

    super::super::analysis::fetch::query_turso_or_local(turso, db, &sql, &params, table)
}

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "meta": {
            "granularity": "error",
            "aggregate_mode": "n/a",
            "data_source": "国土交通省 全国の人流オープンデータ（Agoop社提供）",
            "data_period": "2019-01〜2021-12",
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn raw_only_check_via_aggregate_mode() {
        // 生値の組み合わせのみ許可（double count 防止）
        assert_eq!(
            AggregateMode::from_params(0, 0).unwrap(),
            AggregateMode::Raw
        );
        assert_eq!(
            AggregateMode::from_params(1, 1).unwrap(),
            AggregateMode::Raw
        );
        assert_eq!(
            AggregateMode::from_params(1, 0).unwrap(),
            AggregateMode::Raw
        );
        assert_eq!(
            AggregateMode::from_params(0, 1).unwrap(),
            AggregateMode::Raw
        );
        // 集計値は Raw ではない
        assert_ne!(
            AggregateMode::from_params(2, 0).unwrap(),
            AggregateMode::Raw
        );
        assert_ne!(
            AggregateMode::from_params(1, 2).unwrap(),
            AggregateMode::Raw
        );
        assert_ne!(
            AggregateMode::from_params(2, 2).unwrap(),
            AggregateMode::Raw
        );
    }

    #[test]
    fn row_limit_is_ten_thousand() {
        assert_eq!(HEATMAP_ROW_LIMIT, 10_000);
    }
}
