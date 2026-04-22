//! 人流データ API Handler（Round 2 Phase 6）
//!
//! 9エンドポイントのうち地域カルテ系5個を実装（残4個はPhase C）。
//! 全レスポンスに `meta.granularity` / `aggregate_mode` / `data_source` / `data_period` 必須。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use super::flow_types::{AggregateMode, FlowMeta};
use super::{flow, fromto};
use crate::AppState;

// ======== 共通 Query Params ========

#[derive(Deserialize, Debug)]
pub struct KarteParams {
    /// 着地市区町村コード (5桁)
    pub citycode: i64,
    /// 年次 (2019/2020/2021)、デフォルト2019
    #[serde(default = "default_year")]
    pub year: i32,
}

fn default_year() -> i32 {
    2019
}

#[derive(Deserialize, Debug)]
pub struct CityAggParams {
    pub year: i32,
    pub month: i32,
    /// dayflag: 0=休日, 1=平日, 2=全日
    pub dayflag: i32,
    /// timezone: 0=昼, 1=深夜, 2=終日
    pub timezone: i32,
}

// ======== エンドポイント実装 ========

/// GET /api/flow/karte/profile?citycode=XX&year=YYYY
/// 時間帯×平休日の滞在人口プロファイル（市区町村内、年次）
pub async fn flow_karte_profile(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<KarteParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();
    let citycode = params.citycode;
    let year = params.year;

    let rows = tokio::task::spawn_blocking(move || {
        flow::get_karte_profile(&db, turso.as_ref(), citycode, year)
    })
    .await
    .unwrap_or_default();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "month": get_i64(r, "month"),
                "dayflag": get_i64(r, "dayflag"),
                "timezone": get_i64(r, "timezone"),
                "pop_sum": get_f64(r, "pop_sum"),
            })
        })
        .collect();

    let meta = FlowMeta::new("city", AggregateMode::Raw, year);
    Json(json!({
        "meta": meta,
        "citycode": citycode,
        "year": year,
        "data": data,
    }))
}

/// GET /api/flow/karte/monthly?citycode=XX
/// 36ヶ月時系列（平日昼のみ）
pub async fn flow_karte_monthly(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<KarteParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();
    let citycode = params.citycode;

    let rows = tokio::task::spawn_blocking(move || {
        flow::get_karte_monthly_trend(&db, turso.as_ref(), citycode)
    })
    .await
    .unwrap_or_default();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "year": get_i64(r, "year"),
                "month": get_i64(r, "month"),
                "pop_sum": get_f64(r, "pop_sum"),
            })
        })
        .collect();

    let meta = FlowMeta::new("city", AggregateMode::Raw, params.year);
    Json(json!({
        "meta": meta,
        "citycode": citycode,
        "note": "平日昼（dayflag=1, timezone=0）のみ抽出、2019-2021全36ヶ月",
        "data": data,
    }))
}

/// GET /api/flow/karte/daynight_ratio?citycode=XX&year=YYYY
/// 昼夜比（平日昼 / 平日夜）
pub async fn flow_karte_daynight_ratio(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<KarteParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();
    let citycode = params.citycode;
    let year = params.year;

    let ratio = tokio::task::spawn_blocking(move || {
        flow::get_karte_daynight_ratio(&db, turso.as_ref(), citycode, year)
    })
    .await
    .unwrap_or(None);

    let meta = FlowMeta::new("city", AggregateMode::Raw, year);
    Json(json!({
        "meta": meta,
        "citycode": citycode,
        "year": year,
        "ratio": ratio,
        "interpretation": ratio.map(|r| {
            if r > 1.3 { "昼間流入超過（商業・オフィス街型）" }
            else if r < 0.8 { "昼間流出超過（ベッドタウン型）" }
            else { "均衡型" }
        }),
    }))
}

/// GET /api/flow/karte/inflow_breakdown?citycode=XX&year=YYYY
/// from_area 4区分の流入構造
pub async fn flow_karte_inflow_breakdown(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<KarteParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();
    let citycode = params.citycode;
    let year = params.year;

    let rows = tokio::task::spawn_blocking(move || {
        fromto::get_inflow_breakdown(&db, turso.as_ref(), citycode, year)
    })
    .await
    .unwrap_or_default();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            let from_area = get_i64(r, "from_area");
            json!({
                "from_area": from_area,
                "label": fromto::from_area_label(from_area),
                "total_population": get_f64(r, "total_population"),
            })
        })
        .collect();

    let meta = FlowMeta::new("city", AggregateMode::Raw, year);
    Json(json!({
        "meta": meta,
        "citycode": citycode,
        "year": year,
        "note": "地方ブロック4区分（個別citycodeODはv2_external_commute_od参照）",
        "granularity_note": "Agoopは地方ブロック4区分のみ。市区町村間1:1 ODは国勢調査OD（別エンドポイント）。",
        "data": data,
    }))
}

/// GET /api/flow/city_agg?year=YYYY&month=MM&dayflag=X&timezone=X
/// 市区町村レベル集計（ズームレベル低、z≤9）
pub async fn flow_city_agg(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<CityAggParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();

    // AggregateMode 検証（不正なパラメータで 400 相当）
    let mode = match AggregateMode::from_params(params.dayflag, params.timezone) {
        Ok(m) => m,
        Err(e) => return Json(error_response(&e.to_string())),
    };

    let year = params.year;
    let month = params.month;
    let dayflag = params.dayflag;
    let timezone = params.timezone;

    let rows = tokio::task::spawn_blocking(move || {
        flow::get_city_agg(&db, turso.as_ref(), year, month, dayflag, timezone)
    })
    .await
    .unwrap_or_default();

    let data: Vec<Value> = rows
        .iter()
        .map(|r| {
            json!({
                "citycode": get_i64(r, "citycode"),
                "pop_sum": get_f64(r, "pop_sum"),
                "mesh_count": get_i64(r, "mesh_count"),
            })
        })
        .collect();

    let meta = FlowMeta::new("city", mode, year);
    Json(json!({
        "meta": meta,
        "year": year,
        "month": month,
        "dayflag": dayflag,
        "timezone": timezone,
        "data_count": data.len(),
        "data": data,
    }))
}

// ======== ヘルパー ========

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

fn get_i64(row: &super::super::helpers::Row, key: &str) -> i64 {
    super::super::helpers::get_i64(row, key)
}

fn get_f64(row: &super::super::helpers::Row, key: &str) -> f64 {
    super::super::helpers::get_f64(row, key)
}
