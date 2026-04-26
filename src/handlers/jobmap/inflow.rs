//! 流入元サンキー図ハンドラ
//!
//! GET /api/jobmap/inflow?citycode=13101&year=2021&month=7&dayflag=1&timezone=0
//!
//! v2_flow_fromto_city から from_area 4区分の流入量を取得し、
//! ECharts sankey 形式で返す。
//!
//! 🔴 重要:
//! - dayflag/timezone の集計値 (2) と非集計値 (0,1) を同時に SUM しない
//! - v2_flow_fromto_city は Turso書き込み制限により約83%のみ投入済み
//!   → レスポンスに data_warning を必ず含める

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

use super::flow_types::{AggregateMode, FlowMeta};
use super::fromto;
use crate::AppState;

const DATA_WARNING: &str = "fromto_city 投入済みデータは全体の約83%です（Turso書き込み制限のため）。5月1日以降に完全投入予定。";

#[derive(Deserialize, Debug)]
pub struct InflowParams {
    /// 着地市区町村コード (5桁)
    pub citycode: i64,
    /// 年次 (2019/2020/2021)
    #[serde(default = "default_year")]
    pub year: i32,
    /// 月 (1-12)
    #[serde(default = "default_month")]
    pub month: i32,
    /// dayflag: 0=休日, 1=平日（2=全日は集計値のため原則非推奨）
    #[serde(default = "default_dayflag")]
    pub dayflag: i32,
    /// timezone: 0=昼, 1=深夜（2=終日は集計値のため原則非推奨）
    #[serde(default = "default_timezone")]
    pub timezone: i32,
}

fn default_year() -> i32 {
    2021
}
fn default_month() -> i32 {
    7
}
fn default_dayflag() -> i32 {
    1
}
fn default_timezone() -> i32 {
    0
}

/// GET /api/jobmap/inflow
///
/// レスポンス:
/// ```json
/// {
///   "meta": {...},
///   "citycode": 13101,
///   "sankey": {
///     "nodes": [{"name": "同市区町村"}, ...],
///     "links": [{"source": "同市区町村", "target": "着地(citycode)", "value": 12345}, ...]
///   },
///   "summary": [{"from_area": 0, "area_name": "同市区町村", "population": 12345, "share": 0.45}, ...],
///   "data_warning": "..."
/// }
/// ```
pub async fn jobmap_inflow_sankey(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<InflowParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };
    let turso = state.turso_db.clone();

    // 非集計値のみ許可（double-count防止）
    let mode = match AggregateMode::from_params(params.dayflag, params.timezone) {
        Ok(m) => m,
        Err(e) => return Json(error_response(&e.to_string())),
    };
    if params.dayflag == 2 || params.timezone == 2 {
        return Json(error_response(
            "流入サンキーは非集計値 (dayflag∈{0,1}, timezone∈{0,1}) のみ対応",
        ));
    }

    let citycode = params.citycode;
    let year = params.year;
    let month = params.month;
    let dayflag = params.dayflag;
    let timezone = params.timezone;

    let rows = tokio::task::spawn_blocking(move || {
        fromto::get_inflow_sankey(
            &db,
            turso.as_ref(),
            citycode,
            year,
            month,
            dayflag,
            timezone,
        )
    })
    .await
    .unwrap_or_default();

    // サマリー計算
    let total: f64 = rows
        .iter()
        .map(|r| super::super::helpers::get_f64(r, "total_population"))
        .sum();

    let target_node = format!("着地({})", citycode);

    let mut nodes: Vec<Value> = Vec::with_capacity(5);
    let mut links: Vec<Value> = Vec::with_capacity(4);
    let mut summary: Vec<Value> = Vec::with_capacity(4);

    for r in &rows {
        let from_area = super::super::helpers::get_i64(r, "from_area");
        let pop = super::super::helpers::get_f64(r, "total_population");
        let label = fromto::from_area_short_label(from_area);
        let full_label = fromto::from_area_label(from_area);
        let share = if total > 0.0 { pop / total } else { 0.0 };

        nodes.push(json!({ "name": label }));
        links.push(json!({
            "source": label,
            "target": target_node,
            "value": pop,
        }));
        summary.push(json!({
            "from_area": from_area,
            "area_name": full_label,
            "short_name": label,
            "population": pop,
            "share": share,
        }));
    }
    // 着地ノード
    nodes.push(json!({ "name": target_node }));

    let meta = FlowMeta::new("city", mode, year);
    Json(json!({
        "meta": meta,
        "citycode": citycode,
        "year": year,
        "month": month,
        "dayflag": dayflag,
        "timezone": timezone,
        "sankey": {
            "nodes": nodes,
            "links": links,
        },
        "summary": summary,
        "total_population": total,
        "data_warning": DATA_WARNING,
        "note": "from_area 4区分（Agoop地方ブロック粒度）。非集計値のみ使用。",
    }))
}

// ======== ヘルパー ========

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "data_warning": DATA_WARNING,
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
    fn data_warning_not_empty() {
        assert!(!DATA_WARNING.is_empty());
        assert!(DATA_WARNING.contains("83%"));
    }

    #[test]
    fn default_params() {
        assert_eq!(default_year(), 2021);
        assert_eq!(default_month(), 7);
        assert_eq!(default_dayflag(), 1);
        assert_eq!(default_timezone(), 0);
    }
}
