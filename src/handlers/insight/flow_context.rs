//! Agoop人流データの InsightContext 拡張用コンテキスト
//!
//! 既存 `InsightContext` に追加する `flow: Option<FlowIndicators>` を生成する。
//! v2_flow_* テーブル未作成環境では `None` を返し、engine_flow の各パターンは非発火。

use super::super::helpers::{get_f64, get_i64, Row};
use crate::db::local_sqlite::LocalDb as Db;
use crate::db::turso_http::TursoDb;

/// 人流指標の集約コンテキスト（SW-F01〜F10の入力）
#[derive(Debug, Clone, Default)]
pub struct FlowIndicators {
    /// 深夜滞在 / 昼間滞在 比率（citycode内、平日）
    pub midnight_ratio: Option<f64>,
    /// 休日昼 / 平日昼 比率
    pub holiday_day_ratio: Option<f64>,
    /// 昼夜比（平日昼 / 平日夜）
    pub daynight_ratio: Option<f64>,
    /// 平日昼-夜差の相対値（流出度合い、負数=ベッドタウン）
    pub day_night_diff_ratio: Option<f64>,
    /// コロナ期回復率（2021年9月 / 2019年9月、平日昼）
    pub covid_recovery_ratio: Option<f64>,
    /// 月次振幅係数（最大月 / 平均月 - 1、季節性指標）
    pub monthly_amplitude: Option<f64>,
    /// 異地方比率（from_area=3 の総人口 / 全from_area合計）
    pub diff_region_inflow_ratio: Option<f64>,
    /// 4区分流入（from_area別Vec）
    pub inflow_breakdown: Vec<Row>,
    /// 36ヶ月時系列（平日昼のみ）
    pub monthly_trend: Vec<Row>,
    /// メタ情報
    pub citycode: i64,
    pub year: i32,
}

/// FlowIndicators を構築する
///
/// v2_flow_* テーブルが存在しない場合は `None` を返す。
pub fn build_flow_context(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
    default_year: i32,
) -> Option<FlowIndicators> {
    if muni.is_empty() {
        return None;
    }

    let citycode = fetch_citycode(db, pref, muni)?;
    // 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接存在チェック。
    // Turso 専用環境では turso が渡された時点で存在と見なしクエリに委ねる。
    let flow_available =
        super::super::helpers::table_exists(db, "v2_flow_city_agg") || turso.is_some();
    if !flow_available {
        return None;
    }

    let monthly_trend = super::super::jobmap::flow::get_karte_monthly_trend(db, turso, citycode);

    let daynight_ratio =
        super::super::jobmap::flow::get_karte_daynight_ratio(db, turso, citycode, default_year);

    let inflow_breakdown =
        super::super::jobmap::fromto::get_inflow_breakdown(db, turso, citycode, default_year);

    let diff_region_inflow_ratio = calc_diff_region_ratio(&inflow_breakdown);

    // 深夜/昼間 比率（平日のみ、timezone=1/0）
    let midnight_ratio = calc_ratio_from_profile(db, turso, citycode, default_year, 1, 1, 1, 0);
    // 休日昼 / 平日昼 比率（dayflag=0/1, timezone=0）
    let holiday_day_ratio = calc_ratio_from_profile(db, turso, citycode, default_year, 0, 0, 1, 0);

    // 平日昼-夜差の相対値
    let day_night_diff_ratio = daynight_ratio.map(|r| (r - 1.0) / r.max(1.0));

    // コロナ期回復率（2021/2019、平日昼9月）
    let covid_recovery_ratio = calc_covid_recovery(db, turso, citycode);

    // 月次振幅係数
    let monthly_amplitude = calc_monthly_amplitude(&monthly_trend);

    Some(FlowIndicators {
        midnight_ratio,
        holiday_day_ratio,
        daynight_ratio,
        day_night_diff_ratio,
        covid_recovery_ratio,
        monthly_amplitude,
        diff_region_inflow_ratio,
        inflow_breakdown,
        monthly_trend,
        citycode,
        year: default_year,
    })
}

/// pref+muni → citycode 変換（attribute_mesh1km または prefcode_citycode_master から）
fn fetch_citycode(db: &Db, pref: &str, muni: &str) -> Option<i64> {
    // v2_flow_master_prefcity を優先、なければ attribute
    let sql = "SELECT citycode FROM v2_flow_master_prefcity \
               WHERE prefname = ?1 AND cityname = ?2 LIMIT 1";
    if super::super::helpers::table_exists(db, "v2_flow_master_prefcity") {
        let rows = super::super::analysis::fetch::query_turso_or_local(
            None,
            db,
            sql,
            &[pref.to_string(), muni.to_string()],
            "v2_flow_master_prefcity",
        );
        if let Some(r) = rows.first() {
            let code = get_i64(r, "citycode");
            if code > 0 {
                return Some(code);
            }
        }
    }
    None
}

/// 指定 (dayflag_a, timezone_a) と (dayflag_b, timezone_b) の滞在人口比率を取得
fn calc_ratio_from_profile(
    db: &Db,
    turso: Option<&TursoDb>,
    citycode: i64,
    year: i32,
    dayflag_a: i32,
    timezone_a: i32,
    dayflag_b: i32,
    timezone_b: i32,
) -> Option<f64> {
    // 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照。
    if super::super::jobmap::flow::resolve_table_by_year(year).is_err() {
        return None;
    }
    let sql = "SELECT dayflag, timezone, SUM(pop_sum) as total \
               FROM v2_flow_city_agg \
               WHERE citycode = ?1 AND year = ?2 \
                 AND ((dayflag = ?3 AND timezone = ?4) OR (dayflag = ?5 AND timezone = ?6)) \
               GROUP BY dayflag, timezone";
    let params = vec![
        citycode.to_string(),
        year.to_string(),
        dayflag_a.to_string(),
        timezone_a.to_string(),
        dayflag_b.to_string(),
        timezone_b.to_string(),
    ];
    let rows = super::super::analysis::fetch::query_turso_or_local(
        turso, db, sql, &params, "v2_flow_city_agg",
    );
    let mut num = 0.0;
    let mut den = 0.0;
    for r in &rows {
        let d = get_i64(r, "dayflag") as i32;
        let t = get_i64(r, "timezone") as i32;
        let total = get_f64(r, "total");
        if d == dayflag_a && t == timezone_a {
            num = total;
        } else if d == dayflag_b && t == timezone_b {
            den = total;
        }
    }
    if den > 0.0 {
        Some(num / den)
    } else {
        None
    }
}

/// 異地方比率（from_area=3 / 全合計）
fn calc_diff_region_ratio(inflow_breakdown: &[Row]) -> Option<f64> {
    if inflow_breakdown.is_empty() {
        return None;
    }
    let mut total = 0.0;
    let mut diff_region = 0.0;
    for r in inflow_breakdown {
        let from_area = get_i64(r, "from_area");
        let pop = get_f64(r, "total_population");
        total += pop;
        if from_area == 3 {
            diff_region = pop;
        }
    }
    if total > 0.0 {
        Some(diff_region / total)
    } else {
        None
    }
}

/// コロナ期回復率（2021年9月 / 2019年9月、平日昼）
fn calc_covid_recovery(db: &Db, turso: Option<&TursoDb>, citycode: i64) -> Option<f64> {
    // 2026-05-01 CTAS 戻し: `v2_flow_city_agg` 直接参照。
    // 平日昼 (dayflag=1, timezone=0) × 9 月 × 2019/2021 のピンポイント。
    let sql = "SELECT year, pop_sum AS total \
               FROM v2_flow_city_agg \
               WHERE citycode = ?1 \
                 AND year IN (2019, 2021) \
                 AND month = '09' \
                 AND dayflag = 1 AND timezone = 0";
    let params = vec![citycode.to_string()];
    let rows = super::super::analysis::fetch::query_turso_or_local(
        turso,
        db,
        sql,
        &params,
        "v2_flow_city_agg",
    );
    let mut y2019 = 0.0;
    let mut y2021 = 0.0;
    for r in &rows {
        let y = get_i64(r, "year") as i32;
        let total = get_f64(r, "total");
        match y {
            2019 => y2019 = total,
            2021 => y2021 = total,
            _ => {}
        }
    }
    if y2019 > 0.0 {
        Some(y2021 / y2019)
    } else {
        None
    }
}

/// 月次振幅係数（最大月 / 平均月 - 1）
fn calc_monthly_amplitude(monthly_trend: &[Row]) -> Option<f64> {
    if monthly_trend.is_empty() {
        return None;
    }
    let values: Vec<f64> = monthly_trend
        .iter()
        .map(|r| get_f64(r, "pop_sum"))
        .filter(|v| *v > 0.0)
        .collect();
    if values.len() < 3 {
        return None;
    }
    let sum: f64 = values.iter().sum();
    let avg = sum / values.len() as f64;
    let max = values.iter().cloned().fold(0.0_f64, f64::max);
    if avg > 0.0 {
        Some(max / avg - 1.0)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::helpers::Row;
    use super::*;

    /// diff_region_ratio: from_area=3 の比率計算
    #[test]
    fn calc_diff_region_ratio_standard() {
        let mut row_types_total: Vec<(i64, f64)> = vec![
            (0, 1000.0), // 同市区町村
            (1, 500.0),  // 同県別市
            (2, 200.0),  // 同地方別県
            (3, 300.0),  // 異地方
        ];
        // Row は HashMap<String, Value> 相当。既存テストパターンに揃える
        let rows: Vec<Row> = row_types_total
            .drain(..)
            .map(|(from_area, pop)| {
                let mut r = Row::new();
                r.insert("from_area".to_string(), serde_json::Value::from(from_area));
                r.insert("total_population".to_string(), serde_json::Value::from(pop));
                r
            })
            .collect();

        let ratio = calc_diff_region_ratio(&rows).unwrap();
        // 300 / (1000+500+200+300) = 300/2000 = 0.15
        assert!((ratio - 0.15).abs() < 0.001);
    }

    #[test]
    fn calc_diff_region_ratio_empty() {
        assert!(calc_diff_region_ratio(&[]).is_none());
    }

    /// 振幅係数: 最大 / 平均 - 1
    #[test]
    fn calc_monthly_amplitude_standard() {
        let rows: Vec<Row> = [100.0, 120.0, 150.0, 80.0, 90.0]
            .iter()
            .map(|&v| {
                let mut r = Row::new();
                r.insert("pop_sum".to_string(), serde_json::Value::from(v));
                r
            })
            .collect();
        let amp = calc_monthly_amplitude(&rows).unwrap();
        // avg = 108, max = 150 → 150/108 - 1 ≈ 0.389
        assert!(amp > 0.35 && amp < 0.45);
    }

    #[test]
    fn calc_monthly_amplitude_too_few() {
        let rows: Vec<Row> = [100.0, 120.0]
            .iter()
            .map(|&v| {
                let mut r = Row::new();
                r.insert("pop_sum".to_string(), serde_json::Value::from(v));
                r
            })
            .collect();
        // 3未満は None
        assert!(calc_monthly_amplitude(&rows).is_none());
    }
}
