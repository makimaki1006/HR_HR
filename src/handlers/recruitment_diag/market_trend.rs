//! Panel 6: 市場動向グラフ
//!
//! HW 時系列テーブル (ts_turso_counts) から過去 N ヶ月の
//! 指定業種 × 都道府県の求人数推移と増加率を算出する。
//!
//! データソース: ts_turso_counts (月次スナップショット)
//! - カラム: snapshot_id, prefecture, emp_group, posting_count, facility_count
//! - industry_major_code カラムは現時点の集計テーブルに無いため、
//!   job_type 指定時は ts_turso_salary (industry_major_code 有) の count 列を
//!   代替集計として使用する。
//!
//! 増加率計算:
//!   growth_rate = (今月 - N ヶ月前) / N ヶ月前 × 100
//!
//! データ範囲制約 (feedback_hw_data_scope):
//! - HW 掲載求人のみ。全求人市場ではない。
//! - 月次スナップショット由来のため季節要因を含む。単月比較は注意。
//! - 相関は示せても因果は証明できない (feedback_correlation_not_causation)。

use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_i64, Row};
use crate::handlers::recruitment_diag::competitors::{hw_data_scope_warning, prefcode_to_name};
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;

#[derive(Deserialize)]
pub struct MarketTrendQuery {
    #[serde(default)]
    pub job_type: String,
    #[serde(default)]
    pub emp_type: String, // 例: "正社員"
    pub prefcode: Option<i32>,
    /// 過去 N ヶ月 (default 6)
    pub months: Option<usize>,
}

/// GET /api/recruitment_diag/market_trend
pub async fn market_trend(
    State(state): State<Arc<AppState>>,
    Query(q): Query<MarketTrendQuery>,
) -> Json<Value> {
    let prefecture = prefcode_to_name(q.prefcode).unwrap_or_default();
    let months = q.months.unwrap_or(6).clamp(2, 24);

    let turso = match &state.turso_db {
        Some(t) => t.clone(),
        None => return Json(error_response("Turso DB 未接続")),
    };

    let pref_snap = prefecture.clone();
    let emp_snap = q.emp_type.clone();
    let job_type_snap = q.job_type.clone();

    let rows = tokio::task::spawn_blocking(move || {
        fetch_monthly_counts(&turso, &pref_snap, &emp_snap, &job_type_snap, months)
    })
    .await
    .unwrap_or_default();

    let (labels, counts) = extract_series(&rows);
    let growth_rate = compute_growth_rate(&counts);
    let interpretation = build_interpretation(growth_rate, &counts, &prefecture, &q.job_type);

    Json(json!({
        "prefecture": prefecture,
        "job_type": q.job_type,
        "emp_type": q.emp_type,
        "months_requested": months,
        "months": labels,
        "counts": counts,
        "growth_rate_pct": growth_rate,
        "interpretation": interpretation,
        "warning": hw_data_scope_warning(),
    }))
}

/// 月次求人数取得
/// job_type 空: ts_turso_counts.posting_count を集計
/// job_type 有: ts_turso_salary.count を集計 (業界指標あり)
pub(crate) fn fetch_monthly_counts(
    turso: &TursoDb,
    prefecture: &str,
    emp_type: &str,
    job_type: &str,
    months: usize,
) -> Vec<Row> {
    let mut wc: Vec<String> = Vec::new();
    let mut params_own: Vec<String> = Vec::new();
    let mut idx = 1;

    if !prefecture.is_empty() {
        wc.push(format!("prefecture = ?{}", idx));
        params_own.push(prefecture.to_string());
        idx += 1;
    }
    if !emp_type.is_empty() {
        let emp_group = employment_type_to_group(emp_type);
        wc.push(format!("emp_group = ?{}", idx));
        params_own.push(emp_group);
        idx += 1;
    }

    // 業種フィルタ有無でテーブル選択
    let (table, count_col) = if !job_type.is_empty() {
        // ts_turso_salary: industry_major_code あり、count = 対象求人数
        let major = job_type_to_industry_major(job_type);
        if !major.is_empty() {
            wc.push(format!("industry_major_code = ?{}", idx));
            params_own.push(major);
            #[allow(unused_assignments)]
            {
                idx += 1;
            }
        }
        ("ts_turso_salary", "count")
    } else {
        ("ts_turso_counts", "posting_count")
    };

    let where_sql = if wc.is_empty() {
        "1=1".to_string()
    } else {
        wc.join(" AND ")
    };

    // SUM 集計してスナップショット単位で 1 行にする
    let sql = format!(
        "SELECT snapshot_id, SUM({count_col}) as posting_count \
         FROM {table} WHERE {where_sql} \
         GROUP BY snapshot_id \
         ORDER BY snapshot_id DESC \
         LIMIT {months}"
    );

    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params_own
        .iter()
        .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
        .collect();

    let mut rows = turso.query(&sql, &params).unwrap_or_default();
    // 新しい順で取得したので時系列にするために reverse
    rows.reverse();
    rows
}

/// emp_type (正社員 / パート / その他) → emp_group 列の値 (同一)
fn employment_type_to_group(emp_type: &str) -> String {
    // V2 HW: emp_group は "正社員"/"パート"/"その他" の 3 値
    match emp_type {
        "正社員" | "パート" | "その他" => emp_type.to_string(),
        "アルバイト" => "パート".to_string(),
        _ => emp_type.to_string(),
    }
}

/// job_type (HW 業界名) → industry_major_code
/// 13 業界の暫定マッピング。CLAUDE.md JOB_TYPES と揃える。
pub(crate) fn job_type_to_industry_major(job_type: &str) -> String {
    // 未確定な場合は空文字 (業界フィルタしない) を返す
    match job_type {
        "医療" => "83".to_string(),
        "老人福祉・介護" => "85".to_string(),
        "建設業" => "06".to_string(),
        "製造業" => "09".to_string(),
        "小売業" => "56".to_string(),
        "飲食業" | "宿泊業、飲食サービス業" => "75".to_string(),
        "サービス業" => "88".to_string(),
        "IT・通信" | "情報通信業" => "37".to_string(),
        "運輸業" | "運輸業、郵便業" => "42".to_string(),
        "教育・保育" | "教育、学習支援業" => "81".to_string(),
        "派遣・人材" => "91".to_string(),
        _ => String::new(),
    }
}

/// Row 配列から (labels, counts) を抽出
fn extract_series(rows: &[Row]) -> (Vec<String>, Vec<i64>) {
    let mut labels = Vec::with_capacity(rows.len());
    let mut counts = Vec::with_capacity(rows.len());
    for r in rows {
        let snap = get_i64(r, "snapshot_id");
        labels.push(snapshot_label(snap));
        counts.push(get_i64(r, "posting_count"));
    }
    (labels, counts)
}

/// snapshot_id (YYYYMM) → "YYYY/MM"
fn snapshot_label(id: i64) -> String {
    let year = id / 100;
    let month = id % 100;
    format!("{:04}/{:02}", year, month)
}

/// 増加率計算: (最新 - 最古) / 最古 × 100
pub(crate) fn compute_growth_rate(counts: &[i64]) -> f64 {
    if counts.len() < 2 {
        return 0.0;
    }
    let first = *counts.first().unwrap() as f64;
    let last = *counts.last().unwrap() as f64;
    if first <= 0.0 {
        return 0.0;
    }
    (last - first) / first * 100.0
}

/// 解釈テキスト生成 (feedback_hypothesis_driven / feedback_correlation_not_causation)
fn build_interpretation(
    growth: f64,
    counts: &[i64],
    prefecture: &str,
    job_type: &str,
) -> String {
    if counts.len() < 2 {
        return "時系列データが不足しており、増加率を算出できませんでした。".to_string();
    }

    let region = if prefecture.is_empty() {
        "全国".to_string()
    } else {
        prefecture.to_string()
    };
    let industry = if job_type.is_empty() {
        "全業界".to_string()
    } else {
        job_type.to_string()
    };

    let label = classify_trend(growth);

    format!(
        "{region}・{industry}の求人数は期間中 {growth:+.1}% 推移。市場動向: {label}。\
         \n※HW 掲載求人の推移であり市場全体の動向とは異なる可能性。季節要因・月次の振れを含む。\
         \n※あくまで傾向を示すもので因果関係を示すものではない。",
        growth = growth,
        label = label,
    )
}

/// 増加率 → 定性ラベル
pub(crate) fn classify_trend(growth: f64) -> &'static str {
    if growth >= 10.0 {
        "採用競争激化の傾向"
    } else if growth >= 2.0 {
        "ゆるやかな増加傾向"
    } else if growth > -2.0 {
        "横ばい/安定"
    } else if growth > -10.0 {
        "ゆるやかな縮小傾向"
    } else {
        "市場縮小の傾向"
    }
}

fn error_response(msg: &str) -> Value {
    json!({
        "error": msg,
        "months": [],
        "counts": [],
        "growth_rate_pct": 0.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_label_format() {
        assert_eq!(snapshot_label(202401), "2024/01");
        assert_eq!(snapshot_label(202512), "2025/12");
    }

    #[test]
    fn growth_rate_positive() {
        // 100 → 150 → 200 : (200-100)/100 = 100%
        assert!((compute_growth_rate(&[100, 150, 200]) - 100.0).abs() < 0.001);
    }

    #[test]
    fn growth_rate_negative() {
        // 200 → 100: (100-200)/200 = -50%
        assert!((compute_growth_rate(&[200, 100]) - (-50.0)).abs() < 0.001);
    }

    #[test]
    fn growth_rate_flat() {
        assert_eq!(compute_growth_rate(&[100, 100, 100]), 0.0);
    }

    #[test]
    fn growth_rate_single_point() {
        assert_eq!(compute_growth_rate(&[100]), 0.0);
    }

    #[test]
    fn growth_rate_empty() {
        assert_eq!(compute_growth_rate(&[]), 0.0);
    }

    #[test]
    fn growth_rate_zero_start() {
        // 0 始まりは NaN/Inf を避けるため 0.0 を返す
        assert_eq!(compute_growth_rate(&[0, 100]), 0.0);
    }

    #[test]
    fn classify_acceleration() {
        assert_eq!(classify_trend(15.0), "採用競争激化の傾向");
        assert_eq!(classify_trend(5.0), "ゆるやかな増加傾向");
        assert_eq!(classify_trend(0.0), "横ばい/安定");
        assert_eq!(classify_trend(-5.0), "ゆるやかな縮小傾向");
        assert_eq!(classify_trend(-15.0), "市場縮小の傾向");
    }

    #[test]
    fn industry_major_mapping() {
        assert_eq!(job_type_to_industry_major("医療"), "83");
        assert_eq!(job_type_to_industry_major("老人福祉・介護"), "85");
        assert_eq!(job_type_to_industry_major("未定義"), "");
    }

    #[test]
    fn interpretation_contains_region_and_industry() {
        let msg = build_interpretation(5.0, &[100, 105], "東京都", "医療");
        assert!(msg.contains("東京都"));
        assert!(msg.contains("医療"));
        assert!(msg.contains("+5.0%"));
        assert!(msg.contains("因果関係を示すものではない"));
    }

    #[test]
    fn interpretation_insufficient_data() {
        let msg = build_interpretation(0.0, &[100], "", "");
        assert!(msg.contains("不足"));
    }
}
