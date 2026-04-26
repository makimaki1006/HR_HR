//! HTML描画関数（4サブタブ）

use serde_json::Value;
use std::collections::HashMap;

use super::super::helpers::{format_number, get_f64, get_i64};
use super::fetch::*;
use super::helpers::{
    align_yearly_to_monthly, dual_axis_chart_config, echart_div, emp_group_color,
    line_chart_config, parse_snapshot_id, snapshot_label, stacked_area_config, stacked_bar_config,
};

use std::fmt::Write as _;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

/// snapshot_idリスト -> ソートされたユニークなIDリスト
fn unique_snapshots(rows: &[Row]) -> Vec<i64> {
    let mut ids: Vec<i64> = rows
        .iter()
        .map(|r| parse_snapshot_id(r, "snapshot_id"))
        .collect();
    ids.sort();
    ids.dedup();
    ids
}

/// X軸ラベル（"YYYY/MM"）を生成
fn x_labels(snapshots: &[i64]) -> Vec<String> {
    snapshots.iter().map(|&id| snapshot_label(id)).collect()
}

/// 雇用形態グループ別に時系列データを抽出
fn extract_series(
    rows: &[Row],
    snapshots: &[i64],
    value_key: &str,
    groups: &[&str],
) -> Vec<(String, String, Vec<f64>)> {
    groups
        .iter()
        .map(|&group| {
            let data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    rows.iter()
                        .find(|r| {
                            parse_snapshot_id(r, "snapshot_id") == sid
                                && r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("")
                                    == group
                        })
                        .map(|r| get_f64(r, value_key))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            (group.to_string(), emp_group_color(group).to_string(), data)
        })
        .collect()
}

/// Turso未接続時のフォールバック表示
fn no_turso_html() -> String {
    r#"<div class="stat-card">
        <p class="text-slate-500 text-sm">
            <span class="text-amber-400">⚠️</span>
            Tursoデータベースに接続されていないため、時系列トレンドデータを表示できません。
        </p>
    </div>"#
        .to_string()
}

const EMP_GROUPS: [&str; 3] = ["正社員", "パート", "その他"];

// ======== サブタブ1: 量の変化 ========

pub(crate) fn render_subtab_1(turso: Option<&TursoDb>, pref: &str) -> String {
    let turso = match turso {
        Some(t) => t,
        None => return no_turso_html(),
    };

    let counts = fetch_ts_counts(turso, pref);
    let vacancy = fetch_ts_vacancy(turso, pref);

    if counts.is_empty() && vacancy.is_empty() {
        return r#"<p class="text-slate-500 text-sm p-4">時系列データがありません</p>"#.to_string();
    }

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-6">"#);

    // 求人数推移
    if !counts.is_empty() {
        let snapshots = unique_snapshots(&counts);
        let labels = x_labels(&snapshots);
        let posting_series = extract_series(&counts, &snapshots, "posting_count", &EMP_GROUPS);
        let facility_series = extract_series(&counts, &snapshots, "facility_count", &EMP_GROUPS);

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">求人数推移</h3>"#);
        let config = line_chart_config("求人数推移", &labels, &posting_series, "");
        html.push_str(&echart_div(&config, "320px"));
        // 最新月のサマリー
        if let Some(&latest) = snapshots.last() {
            html.push_str(r#"<div class="grid grid-cols-3 gap-3 mt-3">"#);
            for group in &EMP_GROUPS {
                if let Some(row) = counts.iter().find(|r| {
                    parse_snapshot_id(r, "snapshot_id") == latest
                        && r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("") == *group
                }) {
                    let cnt = get_i64(row, "posting_count");
                    let color = emp_group_color(group);
                    write!(html,
                        r#"<div class="text-center"><span class="text-xs text-slate-500">{}</span><div class="text-lg font-bold" style="color:{}">{}</div></div>"#,
                        group, color, format_number(cnt)
                    ).unwrap();
                }
            }
            html.push_str("</div>");
        }
        html.push_str("</div>");

        // 事業所数推移
        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(
            r#"<h3 class="text-base font-semibold text-slate-300 mb-3">事業所数推移</h3>"#,
        );
        let config = line_chart_config("事業所数推移", &labels, &facility_series, "");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");
    }

    // 欠員率・増員率推移
    if !vacancy.is_empty() {
        let snapshots = unique_snapshots(&vacancy);
        let labels = x_labels(&snapshots);
        let vac_series = extract_series(&vacancy, &snapshots, "vacancy_rate", &EMP_GROUPS);
        let growth_series = extract_series(&vacancy, &snapshots, "growth_rate", &EMP_GROUPS);

        // vacancy_rate を % 表示用に変換
        let vac_pct: Vec<(String, String, Vec<f64>)> = vac_series
            .into_iter()
            .map(|(n, c, d)| (n, c, d.iter().map(|v| v * 100.0).collect()))
            .collect();
        let growth_pct: Vec<(String, String, Vec<f64>)> = growth_series
            .into_iter()
            .map(|(n, c, d)| (n, c, d.iter().map(|v| v * 100.0).collect()))
            .collect();

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(
            r#"<h3 class="text-base font-semibold text-slate-300 mb-3">欠員補充率推移</h3>"#,
        );
        html.push_str(
            r#"<p class="text-xs text-slate-500 mb-2">募集理由が「欠員補充」である求人の割合</p>"#,
        );
        let config = line_chart_config("欠員補充率推移", &labels, &vac_pct, "percent");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">増員率推移</h3>"#);
        html.push_str(
            r#"<p class="text-xs text-slate-500 mb-2">募集理由が「増員」である求人の割合</p>"#,
        );
        let config = line_chart_config("増員率推移", &labels, &growth_pct, "percent");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");
    }

    html.push_str("</div>");
    html
}

// ======== サブタブ2: 質の変化 ========

pub(crate) fn render_subtab_2(turso: Option<&TursoDb>, pref: &str) -> String {
    let turso = match turso {
        Some(t) => t,
        None => return no_turso_html(),
    };

    let salary = fetch_ts_salary(turso, pref);
    let workstyle = fetch_ts_workstyle(turso, pref);

    if salary.is_empty() && workstyle.is_empty() {
        return r#"<p class="text-slate-500 text-sm p-4">時系列データがありません</p>"#.to_string();
    }

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-6">"#);

    // 給与推移
    if !salary.is_empty() {
        let snapshots = unique_snapshots(&salary);
        let labels = x_labels(&snapshots);

        // 正社員の給与下限/上限をdual line
        let seishain_rows: Vec<&Row> = salary
            .iter()
            .filter(|r| r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("") == "正社員")
            .collect();

        if !seishain_rows.is_empty() {
            let mean_min_data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    seishain_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "mean_min"))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            let mean_max_data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    seishain_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "mean_max"))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            let median_min_data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    seishain_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "median_min"))
                        .unwrap_or(f64::NAN)
                })
                .collect();

            let salary_series = vec![
                ("平均下限".to_string(), "#3b82f6".to_string(), mean_min_data),
                ("平均上限".to_string(), "#60a5fa".to_string(), mean_max_data),
                (
                    "中央値(下限)".to_string(),
                    "#f97316".to_string(),
                    median_min_data,
                ),
            ];

            html.push_str(r#"<div class="stat-card">"#);
            html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">正社員 給与推移（月額）</h3>"#);
            let config = line_chart_config("正社員 給与推移", &labels, &salary_series, "yen");
            html.push_str(&echart_div(&config, "320px"));
            html.push_str("</div>");
        }

        // パートの給与推移
        let part_rows: Vec<&Row> = salary
            .iter()
            .filter(|r| r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("") == "パート")
            .collect();

        if !part_rows.is_empty() {
            let mean_min_data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    part_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "mean_min"))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            let mean_max_data: Vec<f64> = snapshots
                .iter()
                .map(|&sid| {
                    part_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "mean_max"))
                        .unwrap_or(f64::NAN)
                })
                .collect();

            let part_series = vec![
                ("平均下限".to_string(), "#f97316".to_string(), mean_min_data),
                ("平均上限".to_string(), "#fb923c".to_string(), mean_max_data),
            ];

            html.push_str(r#"<div class="stat-card">"#);
            html.push_str(
                r#"<h3 class="text-base font-semibold text-slate-300 mb-3">パート 時給推移</h3>"#,
            );
            let config = line_chart_config("パート 時給推移", &labels, &part_series, "yen");
            html.push_str(&echart_div(&config, "280px"));
            html.push_str("</div>");
        }
    }

    // 年間休日推移（avg_annual_holidaysが全NULLの場合はスキップ）
    if !workstyle.is_empty() {
        let has_holiday_data = workstyle.iter().any(|r| {
            r.get("avg_annual_holidays")
                .and_then(|v| v.as_f64())
                .map(|v| v > 0.0)
                .unwrap_or(false)
        });
        if has_holiday_data {
            let snapshots = unique_snapshots(&workstyle);
            let labels = x_labels(&snapshots);
            let holiday_series =
                extract_series(&workstyle, &snapshots, "avg_annual_holidays", &EMP_GROUPS);

            html.push_str(r#"<div class="stat-card">"#);
            html.push_str(
                r#"<h3 class="text-base font-semibold text-slate-300 mb-3">年間休日数推移</h3>"#,
            );
            let config = line_chart_config("年間休日数推移", &labels, &holiday_series, "days");
            html.push_str(&echart_div(&config, "280px"));
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

// ======== サブタブ3: 構造の変化 ========

pub(crate) fn render_subtab_3(turso: Option<&TursoDb>, pref: &str) -> String {
    let turso = match turso {
        Some(t) => t,
        None => return no_turso_html(),
    };

    let counts = fetch_ts_counts(turso, pref);
    let fulfillment = fetch_ts_fulfillment(turso, pref);

    if counts.is_empty() && fulfillment.is_empty() {
        return r#"<p class="text-slate-500 text-sm p-4">時系列データがありません</p>"#.to_string();
    }

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-6">"#);

    // 雇用形態別構成推移（stacked area）
    if !counts.is_empty() {
        let snapshots = unique_snapshots(&counts);
        let labels = x_labels(&snapshots);
        let area_series = extract_series(&counts, &snapshots, "posting_count", &EMP_GROUPS);

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">雇用形態別求人構成推移</h3>"#);
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">正社員/パート/その他の求人数の積み上げ推移</p>"#);
        let config = stacked_area_config("雇用形態別求人構成", &labels, &area_series);
        html.push_str(&echart_div(&config, "320px"));
        html.push_str("</div>");
    }

    // 平均掲載日数推移
    if !fulfillment.is_empty() {
        let snapshots = unique_snapshots(&fulfillment);
        let labels = x_labels(&snapshots);
        let listing_series =
            extract_series(&fulfillment, &snapshots, "avg_listing_days", &EMP_GROUPS);

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(
            r#"<h3 class="text-base font-semibold text-slate-300 mb-3">平均掲載日数推移</h3>"#,
        );
        html.push_str(
            r#"<p class="text-xs text-slate-500 mb-2">求人の平均掲載期間（長いほど充足困難）</p>"#,
        );
        let config = line_chart_config("平均掲載日数推移", &labels, &listing_series, "days");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");

        // 長期掲載比率推移
        let long_term_series: Vec<(String, String, Vec<f64>)> = EMP_GROUPS
            .iter()
            .map(|&group| {
                let data: Vec<f64> = snapshots
                    .iter()
                    .map(|&sid| {
                        fulfillment
                            .iter()
                            .find(|r| {
                                parse_snapshot_id(r, "snapshot_id") == sid
                                    && r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("")
                                        == group
                            })
                            .map(|r| {
                                let long = get_i64(r, "long_term_count") as f64;
                                let total = get_i64(r, "count") as f64;
                                if total > 0.0 {
                                    (long / total) * 100.0
                                } else {
                                    f64::NAN
                                }
                            })
                            .unwrap_or(f64::NAN)
                    })
                    .collect();
                (group.to_string(), emp_group_color(group).to_string(), data)
            })
            .collect();

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(
            r#"<h3 class="text-base font-semibold text-slate-300 mb-3">長期掲載比率推移</h3>"#,
        );
        html.push_str(
            r#"<p class="text-xs text-slate-500 mb-2">60日以上掲載されている求人の割合</p>"#,
        );
        let config = line_chart_config("長期掲載比率推移", &labels, &long_term_series, "percent");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");
    }

    html.push_str("</div>");
    html
}

// ======== サブタブ4: シグナル ========

pub(crate) fn render_subtab_4(turso: Option<&TursoDb>, pref: &str) -> String {
    let turso = match turso {
        Some(t) => t,
        None => return no_turso_html(),
    };

    let tracking = fetch_ts_tracking(turso, pref);
    let fulfillment = fetch_ts_fulfillment(turso, pref);

    if tracking.is_empty() && fulfillment.is_empty() {
        return r#"<p class="text-slate-500 text-sm p-4">時系列データがありません</p>"#.to_string();
    }

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-6">"#);

    // 新規/継続/終了（stacked bar）— 全雇用形態合算
    if !tracking.is_empty() {
        let snapshots = unique_snapshots(&tracking);
        let labels = x_labels(&snapshots);

        // 雇用形態を合算
        let new_data: Vec<f64> = snapshots
            .iter()
            .map(|&sid| {
                tracking
                    .iter()
                    .filter(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_i64(r, "new_count") as f64)
                    .sum()
            })
            .collect();
        let continued_data: Vec<f64> = snapshots
            .iter()
            .map(|&sid| {
                tracking
                    .iter()
                    .filter(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_i64(r, "continue_count") as f64)
                    .sum()
            })
            .collect();
        let ended_data: Vec<f64> = snapshots
            .iter()
            .map(|&sid| {
                tracking
                    .iter()
                    .filter(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_i64(r, "end_count") as f64)
                    .sum()
            })
            .collect();

        let bar_series = vec![
            ("新規".to_string(), "#22c55e".to_string(), new_data),
            ("継続".to_string(), "#3b82f6".to_string(), continued_data),
            ("終了".to_string(), "#ef4444".to_string(), ended_data),
        ];

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">求人ライフサイクル推移</h3>"#);
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">新規掲載・継続掲載・掲載終了の月別推移（全雇用形態合計）</p>"#);
        let config = stacked_bar_config("求人ライフサイクル", &labels, &bar_series);
        html.push_str(&echart_div(&config, "320px"));
        html.push_str("</div>");

        // 離脱率推移（churn_rate）— ETLで既に%値として格納済みのためそのまま使用
        let churn_pct = extract_series(&tracking, &snapshots, "churn_rate", &EMP_GROUPS);

        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">離脱率推移</h3>"#);
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">前月からの求人終了率（高いほど入れ替わりが激しい）</p>"#);
        let config = line_chart_config("離脱率推移", &labels, &churn_pct, "percent");
        html.push_str(&echart_div(&config, "280px"));
        html.push_str("</div>");
    }

    // 充足困難度推移（very_long_countが全0の場合はスキップ）
    if !fulfillment.is_empty() {
        let has_data = fulfillment
            .iter()
            .any(|r| get_i64(r, "very_long_count") > 0);
        if has_data {
            let snapshots = unique_snapshots(&fulfillment);
            let labels = x_labels(&snapshots);

            let difficulty_series: Vec<(String, String, Vec<f64>)> = EMP_GROUPS
                .iter()
                .map(|&group| {
                    let data: Vec<f64> = snapshots
                        .iter()
                        .map(|&sid| {
                            fulfillment
                                .iter()
                                .find(|r| {
                                    parse_snapshot_id(r, "snapshot_id") == sid
                                        && r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("")
                                            == group
                                })
                                .map(|r| {
                                    let very_long = get_i64(r, "very_long_count") as f64;
                                    let total = get_i64(r, "count") as f64;
                                    if total > 0.0 {
                                        (very_long / total) * 100.0
                                    } else {
                                        f64::NAN
                                    }
                                })
                                .unwrap_or(f64::NAN)
                        })
                        .collect();
                    (group.to_string(), emp_group_color(group).to_string(), data)
                })
                .collect();

            html.push_str(r#"<div class="stat-card">"#);
            html.push_str(
                r#"<h3 class="text-base font-semibold text-slate-300 mb-3">充足困難度推移</h3>"#,
            );
            html.push_str(r#"<p class="text-xs text-slate-500 mb-2">90日以上掲載されている求人の割合（高いほど充足困難）</p>"#);
            let config =
                line_chart_config("充足困難度推移", &labels, &difficulty_series, "percent");
            html.push_str(&echart_div(&config, "280px"));
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

// ======== サブタブ5: 外部比較 ========

pub(crate) fn render_subtab_5(turso: Option<&TursoDb>, pref: &str) -> String {
    let turso = match turso {
        Some(t) => t,
        None => return no_turso_html(),
    };

    let counts = fetch_ts_counts(turso, pref);
    let salary = fetch_ts_salary(turso, pref);
    let tracking = fetch_ts_tracking(turso, pref);
    let ext_ratio = fetch_ext_job_openings_ratio(turso, pref);
    let ext_labor = fetch_ext_labor_stats(turso, pref);
    let ext_turnover = fetch_ext_turnover(turso, pref);
    let ext_min_wage = fetch_ext_minimum_wage_history(turso, pref);

    if counts.is_empty()
        && salary.is_empty()
        && tracking.is_empty()
        && ext_ratio.is_empty()
        && ext_labor.is_empty()
        && ext_turnover.is_empty()
        && ext_min_wage.is_empty()
    {
        return r#"<p class="text-slate-500 text-sm p-4">外部比較データがありません</p>"#
            .to_string();
    }

    let mut html = String::with_capacity(12_000);
    html.push_str(r#"<div class="space-y-6">"#);

    // 注意書き
    html.push_str(r#"<div class="stat-card" style="border-left: 3px solid #f59e0b;">"#);
    html.push_str(r#"<p class="text-xs text-amber-400">&#9888; 外部統計は年次データ（ステップ表示）、HWデータは月次です。時間粒度が異なります。</p>"#);
    html.push_str("</div>");

    // --- チャート1: 有効求人倍率 x HW求人数推移（dual axis） ---
    if !counts.is_empty() || !ext_ratio.is_empty() {
        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">有効求人倍率 x HW求人数推移</h3>"#);
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">左軸: HW月次求人数（全雇用形態合計）、右軸: 有効求人倍率（年次・e-Stat）</p>"#);

        // HW求人数: 雇用形態を合算して月次の合計求人数を算出
        let hw_snapshots = unique_snapshots(&counts);
        let labels = x_labels(&hw_snapshots);

        let hw_total: Vec<f64> = hw_snapshots
            .iter()
            .map(|&sid| {
                counts
                    .iter()
                    .filter(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_f64(r, "posting_count"))
                    .sum()
            })
            .collect();
        let left_series = vec![("HW求人数".to_string(), "#3b82f6".to_string(), hw_total)];

        // 外部求人倍率: 年度データを月次X軸に合わせる
        let right_series = if !ext_ratio.is_empty() {
            let fy: Vec<String> = ext_ratio
                .iter()
                .filter_map(|r| {
                    r.get("fiscal_year")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            let ratio_vals: Vec<f64> = ext_ratio
                .iter()
                .map(|r| get_f64(r, "ratio_total"))
                .collect();
            let aligned = align_yearly_to_monthly(&fy, &ratio_vals, &hw_snapshots);
            vec![("有効求人倍率".to_string(), "#f97316".to_string(), aligned)]
        } else {
            vec![]
        };

        let config = dual_axis_chart_config(
            "有効求人倍率 x HW求人数",
            &labels,
            &left_series,
            &right_series,
            "求人数",
            "倍率",
        );
        html.push_str(&echart_div(&config, "350px"));
        html.push_str(r#"<p class="text-xs text-slate-500 mt-2">有効求人倍率は厚生労働省「一般職業紹介状況」（e-Stat）に基づく年度値です。HW掲載求人数と比較することで、市場全体の需給動向を確認できます。</p>"#);
        html.push_str("</div>");
    }

    // --- チャート2: 賃金比較推移（dual axis） ---
    if !salary.is_empty() || !ext_labor.is_empty() {
        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(
            r#"<h3 class="text-base font-semibold text-slate-300 mb-3">賃金比較推移</h3>"#,
        );
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">左軸: HW正社員平均給与下限（月次）、右軸: 厚労省現金給与月額（年次・千円→円）</p>"#);

        // HW正社員の給与下限（月次）
        let hw_snapshots = unique_snapshots(&salary);
        let labels = x_labels(&hw_snapshots);

        let seishain_rows: Vec<&Row> = salary
            .iter()
            .filter(|r| r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("") == "正社員")
            .collect();

        let hw_mean_min: Vec<f64> = hw_snapshots
            .iter()
            .map(|&sid| {
                seishain_rows
                    .iter()
                    .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_f64(r, "mean_min"))
                    .unwrap_or(f64::NAN)
            })
            .collect();

        let left_series = vec![(
            "HW正社員 平均下限".to_string(),
            "#3b82f6".to_string(),
            hw_mean_min,
        )];

        // 外部賃金統計: 千円→円に変換、右軸に配置
        let right_series = if !ext_labor.is_empty() {
            let fy: Vec<String> = ext_labor
                .iter()
                .filter_map(|r| {
                    r.get("fiscal_year")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            let ext_salary_sen: Vec<f64> = ext_labor
                .iter()
                .map(|r| get_f64(r, "monthly_salary_male"))
                .collect();
            // 千円→円変換（例: 366.6千円 → 366,600円）
            let ext_salary_yen: Vec<f64> = ext_salary_sen.iter().map(|v| v * 1000.0).collect();
            let aligned = align_yearly_to_monthly(&fy, &ext_salary_yen, &hw_snapshots);
            vec![(
                "厚労省 現金給与(年次)".to_string(),
                "#f97316".to_string(),
                aligned,
            )]
        } else {
            vec![]
        };

        let config = dual_axis_chart_config(
            "賃金比較推移",
            &labels,
            &left_series,
            &right_series,
            "HW給与下限(円)",
            "厚労省給与(円)",
        );
        html.push_str(&echart_div(&config, "350px"));
        html.push_str(r#"<p class="text-xs text-slate-500 mt-2">HW「平均給与下限」は求人票記載の下限値、厚労省「現金給与月額」は手当含む実支給額のため水準が異なります。変化の方向性を比較してください。</p>"#);
        html.push_str("</div>");
    }

    // --- チャート3: 離職率比較（dual axis） ---
    if !tracking.is_empty() || !ext_turnover.is_empty() {
        html.push_str(r#"<div class="stat-card">"#);
        html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">離職率比較</h3>"#);
        html.push_str(r#"<p class="text-xs text-slate-500 mb-2">左軸: HW求人離脱率（月次）、右軸: 厚労省年間離職率（年次）</p>"#);

        // HW離脱率: 全雇用形態の平均churn_rate（ETLで既に%値として格納済み）
        let hw_snapshots = unique_snapshots(&tracking);
        let labels = x_labels(&hw_snapshots);

        let hw_churn_pct: Vec<f64> = hw_snapshots
            .iter()
            .map(|&sid| {
                let rates: Vec<f64> = tracking
                    .iter()
                    .filter(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                    .map(|r| get_f64(r, "churn_rate"))
                    .filter(|v| !v.is_nan())
                    .collect();
                if rates.is_empty() {
                    f64::NAN
                } else {
                    rates.iter().sum::<f64>() / rates.len() as f64
                }
            })
            .collect();

        let left_series = vec![(
            "HW月次離脱率(%)".to_string(),
            "#3b82f6".to_string(),
            hw_churn_pct,
        )];

        // 外部離職率: 右軸に配置
        let right_series = if !ext_turnover.is_empty() {
            let fy: Vec<String> = ext_turnover
                .iter()
                .filter_map(|r| {
                    r.get("fiscal_year")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string())
                })
                .collect();
            let sep_rate: Vec<f64> = ext_turnover
                .iter()
                .map(|r| get_f64(r, "separation_rate"))
                .collect();
            let aligned = align_yearly_to_monthly(&fy, &sep_rate, &hw_snapshots);
            vec![(
                "厚労省 年間離職率(%)".to_string(),
                "#f97316".to_string(),
                aligned,
            )]
        } else {
            vec![]
        };

        let config = dual_axis_chart_config(
            "離職率比較",
            &labels,
            &left_series,
            &right_series,
            "HW離脱率(%)",
            "厚労省離職率(%)",
        );
        html.push_str(&echart_div(&config, "350px"));
        html.push_str(r#"<p class="text-xs text-slate-500 mt-2">HW求人離脱率は「前月からの掲載終了率」、厚労省離職率は「雇用動向調査」に基づく年次値です。定義が異なるため左右の軸スケールが異なります。</p>"#);
        html.push_str("</div>");
    }

    // --- チャート4: 最低賃金推移 x HWパート給与推移（dual axis） ---
    if !ext_min_wage.is_empty() && !salary.is_empty() {
        // パートの平均時給下限（月次）
        let part_rows: Vec<&Row> = salary
            .iter()
            .filter(|r| r.get("emp_group").and_then(|v| v.as_str()).unwrap_or("") == "パート")
            .collect();

        if !part_rows.is_empty() {
            html.push_str(r#"<div class="stat-card">"#);
            html.push_str(r#"<h3 class="text-base font-semibold text-slate-300 mb-3">最低賃金推移 x HWパート給与推移</h3>"#);
            html.push_str(r#"<p class="text-xs text-slate-500 mb-2">左軸: HWパート求人の平均時給下限（月次）、右軸: 最低賃金・時給（年次・厚労省）</p>"#);

            let hw_snapshots = unique_snapshots(&salary);
            let labels = x_labels(&hw_snapshots);

            // 左軸: パート平均時給下限
            let part_mean_min: Vec<f64> = hw_snapshots
                .iter()
                .map(|&sid| {
                    part_rows
                        .iter()
                        .find(|r| parse_snapshot_id(r, "snapshot_id") == sid)
                        .map(|r| get_f64(r, "mean_min"))
                        .unwrap_or(f64::NAN)
                })
                .collect();
            let left_series = vec![(
                "HWパート 平均下限".to_string(),
                "#3b82f6".to_string(),
                part_mean_min,
            )];

            // 右軸: 最低賃金（年度データを月次に合わせる）
            let fy: Vec<String> = ext_min_wage
                .iter()
                .filter_map(|r| {
                    // fiscal_year は INTEGER なので数値として取得して文字列に変換
                    r.get("fiscal_year").and_then(|v| {
                        v.as_i64()
                            .map(|n| n.to_string())
                            .or_else(|| v.as_str().map(|s| s.to_string()))
                    })
                })
                .collect();
            let wage_vals: Vec<f64> = ext_min_wage
                .iter()
                .map(|r| get_f64(r, "hourly_min_wage"))
                .collect();
            let aligned = align_yearly_to_monthly(&fy, &wage_vals, &hw_snapshots);
            let right_series = vec![("最低賃金(時給)".to_string(), "#f97316".to_string(), aligned)];

            let config = dual_axis_chart_config(
                "最低賃金推移 x HWパート給与",
                &labels,
                &left_series,
                &right_series,
                "円",
                "円/時",
            );
            html.push_str(&echart_div(&config, "350px"));
            html.push_str(r#"<p class="text-xs text-slate-500 mt-2">最低賃金（時給）とHWパート求人の平均時給下限を比較。最低賃金引上げ後の時給追随を確認できます。</p>"#);
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}
