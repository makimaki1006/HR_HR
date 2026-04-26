//! 求人×人流 メッシュ相関分析（SW-F04/F10 拡張）
//!
//! `v2_posting_mesh1km`（HW求人メッシュ集計）と `v2_flow_mesh1km_YYYY`（Agoop人流）を
//! 1kmメッシュで結合し、求人密度と滞在人口の相関・異常値メッシュを算出する。
//!
//! # 設計原則
//!
//! - **相関≠因果**: Pearson r を算出するが、レスポンス内に必ず注釈を含める。
//!   解釈文は「傾向がある」「可能性あり」に留める（MEMORY: feedback_correlation_not_causation）。
//! - **仮説駆動**: 単なる散布図ではなく、z-score>2 で**募集難 / 未開拓**の2クラスに分類し、
//!   各メッシュに営業上のアクションを想起させる `category` を付与（MEMORY: feedback_hypothesis_driven）。
//! - **double-count 防御**: dayflag=2 / timezone=2 は `AggregateMode::from_params` で弾く。
//! - **prefcode 絞り込み必須**: 全国クエリは重いので、prefcode フィルタを query 必須化。
//!
//! # データ範囲
//!
//! - 求人データは HW掲載求人に限定（全求人市場ではない）。MEMORY: feedback_hw_data_scope
//! - 人流は 2019-2021 Agoop オープンデータ（コロナ影響あり）。
//!
//! # スナップショット日付の扱い
//!
//! `v2_posting_mesh1km` は月次スナップショット（`snapshot_date`）で積み上がるため、
//! 指定年月に最も近い 1 スナップショットを採用（`MAX(snapshot_date) <= target`）。
//! 未投入時は空の `points` を返す（エラーにはしない）。

use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use super::flow::resolve_table_by_year;
use super::flow_types::{AggregateMode, FlowMeta};
use crate::db::local_sqlite::LocalDb;
use crate::db::turso_http::TursoDb;
use crate::AppState;

// ======== Query Params ========

#[derive(Deserialize, Debug)]
pub struct CorrelationParams {
    /// 都道府県コード (1-47)、絞り込み必須
    pub prefcode: i32,
    /// 年次 (2019/2020/2021)
    #[serde(default = "default_year")]
    pub year: i32,
    /// 月 (1-12)
    #[serde(default = "default_month")]
    pub month: i32,
    /// dayflag: 0=休日, 1=平日（2=集計値は不可）
    #[serde(default = "default_dayflag")]
    pub dayflag: i32,
    /// timezone: 0=昼, 1=深夜（2=集計値は不可）
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

// ======== Handler ========

/// GET /api/jobmap/correlation?prefcode=13&year=2021&month=7&dayflag=1&timezone=0
///
/// メッシュ単位で（人口, 求人件数）の散布を返し、Pearson r と異常値を検出する。
pub async fn jobmap_correlation(
    State(state): State<Arc<AppState>>,
    _session: Session,
    Query(params): Query<CorrelationParams>,
) -> Json<Value> {
    let db = match &state.hw_db {
        Some(d) => d.clone(),
        None => return Json(error_response("DB未接続")),
    };

    // prefcode 必須（全国クエリ防止）
    if !(1..=47).contains(&params.prefcode) {
        return Json(error_response(&format!(
            "invalid prefcode: {} (must be 1-47)",
            params.prefcode
        )));
    }

    // AggregateMode 検証（dayflag=2/timezone=2 の二重カウント防御）
    let mode = match AggregateMode::from_params(params.dayflag, params.timezone) {
        Ok(m @ AggregateMode::Raw) => m,
        Ok(_) => {
            return Json(error_response(
                "correlation analysis requires raw mode (dayflag 0/1 and timezone 0/1)",
            ));
        }
        Err(e) => return Json(error_response(&e.to_string())),
    };

    let table = match resolve_table_by_year(params.year) {
        Ok(t) => t,
        Err(e) => return Json(error_response(&e.to_string())),
    };

    let turso = state.turso_db.clone();
    let prefcode = params.prefcode;
    let year = params.year;
    let month = params.month;
    let dayflag = params.dayflag;
    let timezone = params.timezone;

    let points = tokio::task::spawn_blocking(move || {
        collect_mesh_points(
            &db,
            turso.as_ref(),
            table,
            prefcode,
            year,
            month,
            dayflag,
            timezone,
        )
    })
    .await
    .unwrap_or_default();

    // 統計・異常値検出
    let stats = compute_correlation_stats(&points);

    let meta = FlowMeta::new("mesh1km", mode, year);
    Json(json!({
        "meta": meta,
        "prefcode": prefcode,
        "year": year,
        "month": month,
        "dayflag": dayflag,
        "timezone": timezone,
        "points": stats.points,
        "correlation": {
            "r": stats.r,
            "n": stats.n,
            "note": "Pearson相関係数。相関係数は関連の強さを示すが因果関係を示すものではありません。",
        },
        "outliers": {
            "hiring_hard": stats.hiring_hard,
            "underserved": stats.underserved,
        },
        "note": "HW掲載求人のみ対象（全求人市場ではない）。人流は2019-2021 Agoopデータ。",
    }))
}

// ======== データ収集 ========

#[derive(Debug, Clone)]
struct MeshPoint {
    mesh1kmid: i64,
    lat: f64,
    lng: f64,
    population: f64,
    job_count: f64,
}

/// prefcode 絞り込みで (mesh_id, population, job_count, lat, lng) を取得する。
///
/// # クエリ構造
///
/// ```sql
/// SELECT a.mesh1kmid, a.center_lat, a.center_lng,
///        SUM(f.population) AS population,
///        COALESCE(p.job_count, 0) AS job_count
///   FROM v2_flow_attribute_mesh1km a
///   JOIN v2_flow_mesh1km_YYYY f USING (mesh1kmid)
///   LEFT JOIN (最新スナップショットの v2_posting_mesh1km) p USING (mesh1kmid)
///  WHERE a.prefcode = ?
///    AND f.month = ? AND f.dayflag = ? AND f.timezone = ?
///  GROUP BY a.mesh1kmid
/// ```
fn collect_mesh_points(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    flow_table: &str,
    prefcode: i32,
    _year: i32,
    month: i32,
    dayflag: i32,
    timezone: i32,
) -> Vec<MeshPoint> {
    // 1) flow + attribute JOIN（prefcode絞り込み必須）
    let sql_flow = format!(
        "SELECT a.mesh1kmid AS mesh1kmid, a.center_lat AS lat, a.center_lng AS lng, \
                f.population AS population \
           FROM v2_flow_attribute_mesh1km a \
           JOIN {table} f ON a.mesh1kmid = f.mesh1kmid \
          WHERE a.prefcode = ?1 \
            AND f.month = ?2 \
            AND f.dayflag = ?3 \
            AND f.timezone = ?4",
        table = flow_table
    );
    let params_flow = vec![
        prefcode.to_string(),
        format!("{:02}", month),
        dayflag.to_string(),
        timezone.to_string(),
    ];
    let flow_rows = super::super::analysis::fetch::query_turso_or_local(
        turso,
        db,
        &sql_flow,
        &params_flow,
        flow_table,
    );

    // Turso/SQLite どちらでも動く。v2_flow_attribute_mesh1km が無ければ空になる。
    if flow_rows.is_empty() {
        return vec![];
    }

    // 2) 同一 prefcode の v2_posting_mesh1km を最新 snapshot で取得
    let posting_map = collect_latest_posting(db, turso, prefcode);

    // 3) mesh_id で結合
    let mut points: Vec<MeshPoint> = Vec::with_capacity(flow_rows.len());
    for row in &flow_rows {
        let mesh1kmid = super::super::helpers::get_i64(row, "mesh1kmid");
        let lat = super::super::helpers::get_f64(row, "lat");
        let lng = super::super::helpers::get_f64(row, "lng");
        let population = super::super::helpers::get_f64(row, "population");
        let job_count = posting_map.get(&mesh1kmid).copied().unwrap_or(0.0);
        points.push(MeshPoint {
            mesh1kmid,
            lat,
            lng,
            population,
            job_count,
        });
    }
    points
}

/// prefcode 絞り込み + 最新 snapshot_date の v2_posting_mesh1km を mesh1kmid -> job_count の HashMap で返す。
///
/// テーブル未投入時は空 HashMap（エラーではない）。
fn collect_latest_posting(
    db: &LocalDb,
    turso: Option<&TursoDb>,
    prefcode: i32,
) -> HashMap<i64, f64> {
    // 各 mesh1kmid ごとに最新の snapshot_date を採用
    let sql = "SELECT mesh1kmid, job_count \
                 FROM v2_posting_mesh1km \
                WHERE prefcode = ?1 \
                  AND snapshot_date = ( \
                      SELECT MAX(snapshot_date) FROM v2_posting_mesh1km \
                       WHERE prefcode = ?1 \
                  )";
    let params = vec![prefcode.to_string()];
    let rows = super::super::analysis::fetch::query_turso_or_local(
        turso,
        db,
        sql,
        &params,
        "v2_posting_mesh1km",
    );

    let mut out = HashMap::with_capacity(rows.len());
    for row in &rows {
        let mesh = super::super::helpers::get_i64(row, "mesh1kmid");
        let jc = super::super::helpers::get_f64(row, "job_count");
        out.insert(mesh, jc);
    }
    out
}

// ======== 統計計算 ========

#[derive(Debug)]
struct CorrelationStats {
    points: Vec<Value>,
    r: Option<f64>,
    n: usize,
    hiring_hard: Vec<Value>,
    underserved: Vec<Value>,
}

/// Pearson 相関係数 + z-score による異常値検出。
///
/// 両方の列に有効値がある（>0のメッシュを考慮するが、ここでは全メッシュ採用）。
/// 分散 0 の場合は r = None（直線性なし）。
fn compute_correlation_stats(mesh_points: &[MeshPoint]) -> CorrelationStats {
    let n = mesh_points.len();
    if n == 0 {
        return CorrelationStats {
            points: vec![],
            r: None,
            n: 0,
            hiring_hard: vec![],
            underserved: vec![],
        };
    }

    let pops: Vec<f64> = mesh_points.iter().map(|p| p.population).collect();
    let jobs: Vec<f64> = mesh_points.iter().map(|p| p.job_count).collect();

    let (mean_p, std_p) = mean_std(&pops);
    let (mean_j, std_j) = mean_std(&jobs);

    let r = pearson_r(&pops, &jobs);

    // z-score で分類
    let mut pts_json: Vec<Value> = Vec::with_capacity(n);
    let mut hiring_hard = Vec::new();
    let mut underserved = Vec::new();

    const Z_THRESHOLD: f64 = 2.0;

    for p in mesh_points {
        let z_pop = if std_p > f64::EPSILON {
            (p.population - mean_p) / std_p
        } else {
            0.0
        };
        let z_job = if std_j > f64::EPSILON {
            (p.job_count - mean_j) / std_j
        } else {
            0.0
        };

        // 募集難: 求人多いが人口少ない（z_job > 2 && z_pop < 0）
        // 未開拓: 人口多いが求人少ない（z_pop > 2 && z_job < 0）
        let category = if z_job > Z_THRESHOLD && z_pop < 0.0 {
            "hiring_hard"
        } else if z_pop > Z_THRESHOLD && z_job < 0.0 {
            "underserved"
        } else {
            "normal"
        };

        let point = json!({
            "mesh": p.mesh1kmid,
            "lat": p.lat,
            "lng": p.lng,
            "population": p.population,
            "job_count": p.job_count,
            "z_pop": round3(z_pop),
            "z_job": round3(z_job),
            "category": category,
        });

        match category {
            "hiring_hard" => hiring_hard.push(point.clone()),
            "underserved" => underserved.push(point.clone()),
            _ => {}
        }
        pts_json.push(point);
    }

    CorrelationStats {
        points: pts_json,
        r: r.map(round3),
        n,
        hiring_hard,
        underserved,
    }
}

fn mean_std(xs: &[f64]) -> (f64, f64) {
    if xs.is_empty() {
        return (0.0, 0.0);
    }
    let n = xs.len() as f64;
    let mean = xs.iter().sum::<f64>() / n;
    let var = xs.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / n;
    (mean, var.sqrt())
}

/// Pearson 相関係数。分散ゼロ時は None。
fn pearson_r(xs: &[f64], ys: &[f64]) -> Option<f64> {
    if xs.len() != ys.len() || xs.len() < 2 {
        return None;
    }
    let n = xs.len() as f64;
    let mean_x = xs.iter().sum::<f64>() / n;
    let mean_y = ys.iter().sum::<f64>() / n;
    let mut sxy = 0.0;
    let mut sxx = 0.0;
    let mut syy = 0.0;
    for (x, y) in xs.iter().zip(ys.iter()) {
        let dx = x - mean_x;
        let dy = y - mean_y;
        sxy += dx * dy;
        sxx += dx * dx;
        syy += dy * dy;
    }
    if sxx < f64::EPSILON || syy < f64::EPSILON {
        return None;
    }
    Some(sxy / (sxx.sqrt() * syy.sqrt()))
}

fn round3(v: f64) -> f64 {
    (v * 1000.0).round() / 1000.0
}

// ======== エラーレスポンス ========

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "meta": {
            "granularity": "error",
            "aggregate_mode": "n/a",
            "data_source": "国土交通省 全国の人流オープンデータ（Agoop社提供） × HW掲載求人メッシュ",
            "data_period": "2019-01〜2021-12 / HW snapshot",
        },
    })
}

// ======== Unit Tests ========
#[cfg(test)]
mod tests {
    use super::*;

    fn mk(mesh: i64, pop: f64, jobs: f64) -> MeshPoint {
        MeshPoint {
            mesh1kmid: mesh,
            lat: 35.0,
            lng: 139.0,
            population: pop,
            job_count: jobs,
        }
    }

    #[test]
    fn pearson_r_perfect_positive() {
        // y = 2x + 1 → r = 1.0
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![3.0, 5.0, 7.0, 9.0, 11.0];
        let r = pearson_r(&xs, &ys).unwrap();
        assert!((r - 1.0).abs() < 1e-9, "expected r=1.0, got {r}");
    }

    #[test]
    fn pearson_r_perfect_negative() {
        // y = -x → r = -1.0
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![5.0, 4.0, 3.0, 2.0, 1.0];
        let r = pearson_r(&xs, &ys).unwrap();
        assert!((r - (-1.0)).abs() < 1e-9, "expected r=-1.0, got {r}");
    }

    #[test]
    fn pearson_r_zero_variance_returns_none() {
        let xs = vec![3.0, 3.0, 3.0];
        let ys = vec![1.0, 2.0, 3.0];
        assert!(pearson_r(&xs, &ys).is_none());
    }

    #[test]
    fn pearson_r_known_moderate_correlation() {
        // 手計算: xs=[1,2,3,4,5] ys=[2,4,5,4,5]
        // mean_x=3, mean_y=4
        // sxy = (-2)(-2)+(-1)(0)+(0)(1)+(1)(0)+(2)(1) = 4+0+0+0+2 = 6
        // sxx = 4+1+0+1+4 = 10
        // syy = 4+0+1+0+1 = 6
        // r = 6 / (sqrt(10)*sqrt(6)) = 6 / sqrt(60) = 0.7746
        let xs = vec![1.0, 2.0, 3.0, 4.0, 5.0];
        let ys = vec![2.0, 4.0, 5.0, 4.0, 5.0];
        let r = pearson_r(&xs, &ys).unwrap();
        assert!((r - 0.7746).abs() < 1e-3, "expected r≈0.7746, got {r}");
    }

    #[test]
    fn compute_stats_detects_hiring_hard() {
        // 10メッシュ: 1件だけ job_count 極端に大（z > 2）& population 低い
        let mut pts = Vec::new();
        for i in 0..9 {
            pts.push(mk(i, 1000.0, 10.0));
        }
        // 異常値: job=1000 (平均よりはるかに大), pop=10 (平均より低)
        pts.push(mk(99, 10.0, 1000.0));

        let stats = compute_correlation_stats(&pts);
        assert_eq!(stats.n, 10);
        assert_eq!(stats.hiring_hard.len(), 1, "1件の募集難を検出する想定");
        let hh = &stats.hiring_hard[0];
        assert_eq!(hh.get("mesh").and_then(|v| v.as_i64()), Some(99));
    }

    #[test]
    fn compute_stats_detects_underserved() {
        // 人口だけ突出・求人ゼロのケース
        let mut pts = Vec::new();
        for i in 0..9 {
            pts.push(mk(i, 100.0, 50.0));
        }
        pts.push(mk(77, 100000.0, 0.0));

        let stats = compute_correlation_stats(&pts);
        assert_eq!(stats.n, 10);
        assert_eq!(stats.underserved.len(), 1);
        let us = &stats.underserved[0];
        assert_eq!(us.get("mesh").and_then(|v| v.as_i64()), Some(77));
    }

    #[test]
    fn compute_stats_empty_points() {
        let stats = compute_correlation_stats(&[]);
        assert_eq!(stats.n, 0);
        assert!(stats.r.is_none());
        assert!(stats.points.is_empty());
        assert!(stats.hiring_hard.is_empty());
        assert!(stats.underserved.is_empty());
    }

    #[test]
    fn round3_rounds_to_three_decimals() {
        assert!((round3(1.23456) - 1.235).abs() < 1e-9);
        assert!((round3(-0.7745966) - (-0.775)).abs() < 1e-9);
    }
}
