//! Phase 6: 条件入力→市場診断ハンドラー（拡張版）
//! 入力: 月給、年間休日、賞与月数、雇用形態
//! 出力: EChartsレーダー + 総合診断 + 業界比較 + 具体的改善提案

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::overview::{get_session_filters, make_location_label, render_no_db_data};
use super::helpers::{get_f64, format_number, table_exists};

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

fn get_str<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

/// クエリパラメータ: 条件入力
#[derive(Debug, Deserialize)]
pub struct DiagnosticQuery {
    pub salary: Option<i64>,        // 月給（円）
    pub holidays: Option<i64>,      // 年間休日
    pub bonus: Option<f64>,         // 賞与月数
    pub emp_type: Option<String>,   // 正社員/パート
}

/// HTMXパーシャル: 市場診断フォーム（初期表示）
pub async fn tab_diagnostic(
    State(_state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;
    let location = make_location_label(&filters.prefecture, &filters.municipality);

    let mut html = String::with_capacity(8_000);
    html.push_str("<div class=\"space-y-6\">\
        <h2 class=\"text-xl font-bold text-white\">市場診断ツール \
            <span class=\"text-blue-400 text-base font-normal\">");
    html.push_str(&location);
    html.push_str("</span></h2>\
        <p class=\"text-xs text-slate-500\">求人条件を入力すると、市場内でのポジションを診断します</p>\
        <div class=\"stat-card\">\
            <h3 class=\"text-sm text-slate-400 mb-4\">条件入力</h3>\
            <form hx-get=\"/api/diagnostic/evaluate\" hx-target=\"#diagnostic-result\" \
                  hx-swap=\"innerHTML\" hx-indicator=\"#diag-loading\" \
                  class=\"grid grid-cols-2 gap-4\">\
                <div><label class=\"text-xs text-slate-400 block mb-1\">月給（円）</label>\
                    <input type=\"number\" name=\"salary\" placeholder=\"例: 250000\" \
                           class=\"w-full bg-slate-700 border border-slate-600 rounded px-3 py-2 text-white text-sm\"></div>\
                <div><label class=\"text-xs text-slate-400 block mb-1\">年間休日（日）</label>\
                    <input type=\"number\" name=\"holidays\" placeholder=\"例: 120\" \
                           class=\"w-full bg-slate-700 border border-slate-600 rounded px-3 py-2 text-white text-sm\"></div>\
                <div><label class=\"text-xs text-slate-400 block mb-1\">賞与（月数）</label>\
                    <input type=\"number\" name=\"bonus\" step=\"0.1\" placeholder=\"例: 3.5\" \
                           class=\"w-full bg-slate-700 border border-slate-600 rounded px-3 py-2 text-white text-sm\"></div>\
                <div><label class=\"text-xs text-slate-400 block mb-1\">雇用形態</label>\
                    <select name=\"emp_type\" class=\"w-full bg-slate-700 border border-slate-600 rounded px-3 py-2 text-white text-sm\">\
                        <option value=\"正社員\">正社員</option><option value=\"パート\">パート</option></select></div>\
                <div class=\"col-span-2\"><button type=\"submit\" \
                    class=\"w-full bg-blue-600 hover:bg-blue-500 text-white rounded py-2 text-sm font-medium\">診断する</button></div>\
            </form>\
            <div id=\"diag-loading\" class=\"htmx-indicator text-center text-slate-400 py-4\">\
                <span class=\"text-sm\">診断中...</span></div>\
        </div>\
        <div id=\"diagnostic-result\"></div>\
    </div>");

    Html(html)
}

/// API: 診断結果を返す（拡張版）
pub async fn evaluate_diagnostic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<DiagnosticQuery>,
) -> Html<String> {
    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Html(render_no_db_data("市場診断")),
    };

    let filters = get_session_filters(&session).await;
    let pref = filters.prefecture.clone();
    let muni = filters.municipality.clone();
    let emp_type = q.emp_type.as_deref().unwrap_or("正社員").to_string();

    let salary = match q.salary {
        Some(s) if s > 0 => s,
        _ => return Html(r#"<div class="stat-card border-l-4 border-amber-500"><p class="text-amber-400">月給を入力してください</p></div>"#.to_string()),
    };

    let holidays = q.holidays.unwrap_or(0);
    let bonus = q.bonus.unwrap_or(0.0);

    // 全DBクエリをspawn_blockingで一括実行
    let pref2 = pref.clone();
    let muni2 = muni.clone();
    let emp2 = emp_type.clone();
    let db_data = tokio::task::spawn_blocking(move || {
        let salary_pct = compute_salary_percentile(&db, &pref2, &muni2, &emp2, salary);
        let holidays_pct = if holidays > 0 {
            compute_holidays_percentile(&db, &pref2, &muni2, &emp2, holidays)
        } else { None };
        let bonus_pct = if bonus > 0.0 {
            compute_bonus_percentile(&db, &pref2, &muni2, &emp2, bonus)
        } else { None };

        let benchmark = fetch_benchmark_for_diagnostic(&db, &pref2, &muni2, &emp2);
        let comp_pkg = fetch_compensation_for_diagnostic(&db, &pref2, &muni2, &emp2);
        let shadow = fetch_shadow_wage_for_diagnostic(&db, &pref2, &muni2, &emp2);
        let fulfillment = fetch_fulfillment_grade(&db, &pref2, &muni2, &emp2);

        (salary_pct, holidays_pct, bonus_pct, benchmark, comp_pkg, shadow, fulfillment)
    }).await;

    let (salary_pct, holidays_pct, bonus_pct, benchmark, comp_pkg, shadow, fulfillment) = match db_data {
        Ok(data) => data,
        Err(_) => return Html(render_no_db_data("市場診断")),
    };

    let mut html = String::with_capacity(32_000);

    // 3. 総合評価（加重平均: 給与50%, 休日30%, 賞与20%）
    let (overall_pct, overall_grade, grade_color) = compute_composite_grade(
        salary_pct, holidays_pct, bonus_pct
    );

    // ========================================
    // ヘッダー + 総合グレード
    // ========================================
    html.push_str(r#"<div class="space-y-4 mt-4">"#);
    html.push_str(r#"<h3 class="text-lg font-bold text-white">📊 総合診断結果</h3>"#);

    // 総合グレードカード
    html.push_str(&format!(
        r#"<div class="stat-card border-l-4" style="border-color:{grade_color}">
            <div class="flex items-center justify-between">
                <div>
                    <span class="text-sm text-slate-400">市場ポジション</span>
                    <p class="text-xs text-slate-500 mt-1">
                        この地域の{emp_type}求人の中で、総合的に上位{top_pct:.0}%に位置します
                    </p>
                </div>
                <div class="text-right">
                    <span class="text-3xl font-bold" style="color:{grade_color}">{overall_grade}</span>
                    <p class="text-xs text-slate-500">{overall_pct:.1}点 / 100</p>
                </div>
            </div>
        </div>"#,
        top_pct = 100.0 - overall_pct,
    ));

    // ========================================
    // ECharts レーダーチャート（6軸）
    // ========================================
    html.push_str(&render_radar_chart(
        salary_pct, holidays_pct, bonus_pct, &benchmark, &comp_pkg,
    ));

    // ========================================
    // パーセンタイルバー（個別指標）
    // ========================================
    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-3">"#);

    html.push_str(&render_percentile_bar(
        "月給", &format!("{}円", format_number(salary)),
        salary_pct, "#3B82F6",
    ));
    if let Some(hp) = holidays_pct {
        html.push_str(&render_percentile_bar(
            "年間休日", &format!("{}日", holidays),
            Some(hp), "#10B981",
        ));
    }
    if let Some(bp) = bonus_pct {
        html.push_str(&render_percentile_bar(
            "賞与", &format!("{:.1}ヶ月", bonus),
            Some(bp), "#8B5CF6",
        ));
    }

    html.push_str("</div>");

    // ========================================
    // 業界比較バー（給与分位での位置）
    // ========================================
    if !shadow.is_empty() {
        html.push_str(&render_industry_comparison(salary, &shadow));
    }

    // ========================================
    // 充足困難度
    // ========================================
    if let Some((avg_score, grade_label)) = fulfillment {
        let fc = if avg_score >= 75.0 { "#EF4444" }
            else if avg_score >= 50.0 { "#F59E0B" }
            else { "#10B981" };
        html.push_str(&format!(
            r#"<div class="stat-card">
                <div class="flex items-center justify-between">
                    <span class="text-sm text-slate-400">この地域の充足困難度</span>
                    <span class="font-bold" style="color:{fc}">{avg_score:.0}点 ({grade_label})</span>
                </div>
                <div class="w-full bg-slate-700 rounded h-2 mt-2">
                    <div class="h-2 rounded" style="width:{avg_score:.0}%;background:{fc}"></div>
                </div>
            </div>"#
        ));
    }

    // ========================================
    // 具体的改善提案（数値付き）
    // ========================================
    html.push_str(&render_actionable_suggestions(
        salary, holidays, bonus, salary_pct, holidays_pct, bonus_pct,
        &shadow, overall_grade, &pref,
    ));

    html.push_str("</div>");

    Html(html)
}

// ======== 総合グレード計算 ========

/// 加重平均で総合スコアを計算し、S/A/B/C/D グレードを返す
fn compute_composite_grade(
    salary_pct: Option<f64>,
    holidays_pct: Option<f64>,
    bonus_pct: Option<f64>,
) -> (f64, &'static str, &'static str) {
    let mut total_weight = 0.0_f64;
    let mut weighted_sum = 0.0_f64;

    // 給与: 50%
    if let Some(sp) = salary_pct {
        weighted_sum += sp * 0.50;
        total_weight += 0.50;
    }
    // 休日: 30%
    if let Some(hp) = holidays_pct {
        weighted_sum += hp * 0.30;
        total_weight += 0.30;
    }
    // 賞与: 20%
    if let Some(bp) = bonus_pct {
        weighted_sum += bp * 0.20;
        total_weight += 0.20;
    }

    let overall = if total_weight > 0.0 { weighted_sum / total_weight } else { 50.0 };

    let (grade, color) = if overall >= 85.0 { ("S", "#FFD700") }
        else if overall >= 70.0 { ("A", "#10B981") }
        else if overall >= 50.0 { ("B", "#3B82F6") }
        else if overall >= 30.0 { ("C", "#F59E0B") }
        else { ("D", "#EF4444") };

    (overall, grade, color)
}

// ======== ECharts レーダーチャート ========

fn render_radar_chart(
    salary_pct: Option<f64>,
    holidays_pct: Option<f64>,
    bonus_pct: Option<f64>,
    benchmark: &Option<Row>,
    comp_pkg: &Option<Row>,
) -> String {
    // 6軸: 給与, 休日, 賞与, 透明性, 原稿品質, 人材定着
    let v_salary = salary_pct.unwrap_or(50.0);
    let v_holidays = holidays_pct.unwrap_or(50.0);
    let v_bonus = bonus_pct.unwrap_or(50.0);

    // ベンチマークデータがあれば活用、なければ50.0
    let v_transparency = benchmark.as_ref()
        .map(|r| get_f64(r, "info_transparency")).unwrap_or(50.0);
    let v_text_temp = benchmark.as_ref()
        .map(|r| get_f64(r, "text_urgency")).unwrap_or(50.0);
    let v_retention = benchmark.as_ref()
        .map(|r| get_f64(r, "posting_freshness")).unwrap_or(50.0);

    // 地域平均のコンパレータ
    let avg_salary = comp_pkg.as_ref()
        .map(|r| get_f64(r, "salary_pctile")).unwrap_or(50.0);
    let avg_holidays = comp_pkg.as_ref()
        .map(|r| get_f64(r, "holidays_pctile")).unwrap_or(50.0);
    let avg_bonus = comp_pkg.as_ref()
        .map(|r| get_f64(r, "bonus_pctile")).unwrap_or(50.0);

    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">"#);
    html.push_str(r#"<h4 class="text-sm text-slate-400 mb-2">🎯 6軸レーダー診断</h4>"#);
    html.push_str(r#"<p class="text-xs text-slate-500 mb-3">青=あなたの条件 / 灰=地域平均</p>"#);

    // ECharts レーダーチャート
    html.push_str(&format!(
        r##"<div class="echart" style="height:350px;" data-chart-config='{{
            "tooltip": {{"trigger": "item"}},
            "legend": {{
                "data": ["あなたの条件", "地域平均"],
                "bottom": 0,
                "textStyle": {{"color": "#94a3b8", "fontSize": 11}}
            }},
            "radar": {{
                "indicator": [
                    {{"name": "給与水準", "max": 100}},
                    {{"name": "年間休日", "max": 100}},
                    {{"name": "賞与", "max": 100}},
                    {{"name": "情報透明性", "max": 100}},
                    {{"name": "原稿品質", "max": 100}},
                    {{"name": "人材定着度", "max": 100}}
                ],
                "shape": "circle",
                "radius": "60%",
                "axisName": {{"color": "#94a3b8", "fontSize": 11}},
                "splitArea": {{"areaStyle": {{"color": ["rgba(30,41,59,0.3)", "rgba(30,41,59,0.5)"]}}}},
                "axisLine": {{"lineStyle": {{"color": "rgba(100,116,139,0.3)"}}}},
                "splitLine": {{"lineStyle": {{"color": "rgba(100,116,139,0.2)"}}}}
            }},
            "series": [{{
                "type": "radar",
                "data": [
                    {{
                        "value": [{v_salary:.1}, {v_holidays:.1}, {v_bonus:.1}, {v_transparency:.1}, {v_text_temp:.1}, {v_retention:.1}],
                        "name": "あなたの条件",
                        "areaStyle": {{"color": "rgba(59,130,246,0.25)"}},
                        "lineStyle": {{"color": "#3B82F6", "width": 2}},
                        "itemStyle": {{"color": "#3B82F6"}}
                    }},
                    {{
                        "value": [{avg_salary:.1}, {avg_holidays:.1}, {avg_bonus:.1}, {v_transparency:.1}, {v_text_temp:.1}, {v_retention:.1}],
                        "name": "地域平均",
                        "areaStyle": {{"color": "rgba(100,116,139,0.15)"}},
                        "lineStyle": {{"color": "#64748B", "width": 1, "type": "dashed"}},
                        "itemStyle": {{"color": "#64748B"}}
                    }}
                ]
            }}]
        }}'></div>"##
    ));

    html.push_str("</div>");
    html
}

// ======== 業界比較（給与分位での位置） ========

fn render_industry_comparison(salary: i64, shadow: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">"#);
    html.push_str(r#"<h4 class="text-sm text-slate-400 mb-2">📊 給与分布における位置</h4>"#);
    html.push_str(r#"<p class="text-xs text-slate-500 mb-3">地域の給与分布（月給）と入力値の比較</p>"#);

    // shadow_wage から月給データを取得
    let monthly = shadow.iter().find(|r| {
        let st = get_str(r, "salary_type");
        st == "月給" || st.is_empty()
    });

    if let Some(row) = monthly {
        let p10 = get_f64(row, "p10") as i64;
        let p25 = get_f64(row, "p25") as i64;
        let p50 = get_f64(row, "p50") as i64;
        let p75 = get_f64(row, "p75") as i64;
        let p90 = get_f64(row, "p90") as i64;

        // ユーザー値の位置を計算
        let range = (p90 - p10).max(1) as f64;
        let user_pos = ((salary - p10) as f64 / range * 100.0).clamp(2.0, 98.0);

        // 横バーで分位点を可視化
        html.push_str(&format!(
            r#"<div class="relative">
                <div class="flex items-center gap-1 mb-2">
                    <span class="text-xs text-slate-500 w-16">P10</span>
                    <span class="text-xs text-slate-500 flex-1 text-center">P25</span>
                    <span class="text-xs text-slate-500 flex-1 text-center font-medium text-white">P50（中央値）</span>
                    <span class="text-xs text-slate-500 flex-1 text-center">P75</span>
                    <span class="text-xs text-slate-500 w-16 text-right">P90</span>
                </div>
                <div class="relative w-full h-8 bg-slate-700 rounded overflow-hidden">
                    <div class="absolute h-full bg-red-900/40" style="left:0;width:12.5%"></div>
                    <div class="absolute h-full bg-amber-900/30" style="left:12.5%;width:25%"></div>
                    <div class="absolute h-full bg-blue-900/30" style="left:37.5%;width:25%"></div>
                    <div class="absolute h-full bg-emerald-900/30" style="left:62.5%;width:25%"></div>
                    <div class="absolute h-full bg-emerald-900/40" style="left:87.5%;width:12.5%"></div>
                    <div class="absolute h-full w-0.5 bg-slate-500" style="left:12.5%"></div>
                    <div class="absolute h-full w-0.5 bg-slate-500" style="left:37.5%"></div>
                    <div class="absolute h-full w-0.5 bg-white/50" style="left:50%"></div>
                    <div class="absolute h-full w-0.5 bg-slate-500" style="left:62.5%"></div>
                    <div class="absolute h-full w-0.5 bg-slate-500" style="left:87.5%"></div>
                    <div class="absolute -top-1 -bottom-1 w-1 bg-yellow-400 shadow-lg shadow-yellow-400/50" style="left:{user_pos:.1}%"></div>
                </div>
                <div class="flex items-center gap-1 mt-1">
                    <span class="text-xs text-slate-500 w-16">{p10_s}</span>
                    <span class="text-xs text-slate-500 flex-1 text-center">{p25_s}</span>
                    <span class="text-xs text-white flex-1 text-center font-medium">{p50_s}</span>
                    <span class="text-xs text-slate-500 flex-1 text-center">{p75_s}</span>
                    <span class="text-xs text-slate-500 w-16 text-right">{p90_s}</span>
                </div>
                <div class="text-center mt-2">
                    <span class="text-xs text-yellow-400">▲ あなた: {user_s}円</span>
                </div>
            </div>"#,
            user_pos = user_pos,
            p10_s = format_number(p10),
            p25_s = format_number(p25),
            p50_s = format_number(p50),
            p75_s = format_number(p75),
            p90_s = format_number(p90),
            user_s = format_number(salary),
        ));
    } else {
        html.push_str(r#"<p class="text-xs text-slate-500">給与分布データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

// ======== パーセンタイル計算 ========

fn compute_salary_percentile(db: &Db, pref: &str, muni: &str, emp_type: &str, salary: i64) -> Option<f64> {
    // 雇用形態でフィルタ: 正社員の月給とパートの月給を混同しない
    let emp_filter = if emp_type == "パート" { " AND employment_type='パート'" } else { " AND employment_type='正社員'" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT COUNT(*) as below FROM postings WHERE prefecture=?1 AND municipality=?2 \
          AND salary_min > 0 AND salary_type='月給' AND salary_min <= ?3{}", emp_filter),
         vec![pref.to_string(), muni.to_string(), salary.to_string()])
    } else if !pref.is_empty() {
        (format!("SELECT COUNT(*) as below FROM postings WHERE prefecture=?1 \
          AND salary_min > 0 AND salary_type='月給' AND salary_min <= ?2{}", emp_filter),
         vec![pref.to_string(), salary.to_string()])
    } else {
        (format!("SELECT COUNT(*) as below FROM postings WHERE salary_min > 0 AND salary_type='月給' AND salary_min <= ?1{}", emp_filter),
         vec![salary.to_string()])
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !muni.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND salary_min > 0 AND salary_type='月給'{}", emp_filter)
    } else if !pref.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND salary_min > 0 AND salary_type='月給'{}", emp_filter)
    } else {
        format!("SELECT COUNT(*) FROM postings WHERE salary_min > 0 AND salary_type='月給'{}", emp_filter)
    };

    let total_params: Vec<String> = if !muni.is_empty() {
        vec![pref.to_string(), muni.to_string()]
    } else if !pref.is_empty() {
        vec![pref.to_string()]
    } else {
        vec![]
    };
    let tp: Vec<&dyn rusqlite::types::ToSql> = total_params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let total = db.query_scalar::<i64>(&total_sql, &tp).unwrap_or(0);

    if total > 0 {
        Some((below as f64 / total as f64) * 100.0)
    } else {
        None
    }
}

fn compute_holidays_percentile(db: &Db, pref: &str, muni: &str, emp_type: &str, holidays: i64) -> Option<f64> {
    let emp_filter = if emp_type == "パート" { " AND employment_type='パート'" } else { " AND employment_type='正社員'" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND annual_holidays > 0 AND annual_holidays <= ?3{}", emp_filter),
         vec![pref.to_string(), muni.to_string(), holidays.to_string()])
    } else if !pref.is_empty() {
        (format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND annual_holidays > 0 AND annual_holidays <= ?2{}", emp_filter),
         vec![pref.to_string(), holidays.to_string()])
    } else {
        (format!("SELECT COUNT(*) FROM postings WHERE annual_holidays > 0 AND annual_holidays <= ?1{}", emp_filter),
         vec![holidays.to_string()])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !muni.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND annual_holidays > 0{}", emp_filter)
    } else if !pref.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND annual_holidays > 0{}", emp_filter)
    } else {
        format!("SELECT COUNT(*) FROM postings WHERE annual_holidays > 0{}", emp_filter)
    };
    let tp: Vec<String> = if !muni.is_empty() {
        vec![pref.to_string(), muni.to_string()]
    } else if !pref.is_empty() {
        vec![pref.to_string()]
    } else { vec![] };
    let tpp: Vec<&dyn rusqlite::types::ToSql> = tp.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let total = db.query_scalar::<i64>(&total_sql, &tpp).unwrap_or(0);

    if total > 0 { Some((below as f64 / total as f64) * 100.0) } else { None }
}

fn compute_bonus_percentile(db: &Db, pref: &str, muni: &str, emp_type: &str, bonus: f64) -> Option<f64> {
    let emp_filter = if emp_type == "パート" { " AND employment_type='パート'" } else { " AND employment_type='正社員'" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND bonus_months > 0 AND bonus_months <= ?3{}", emp_filter),
         vec![pref.to_string(), muni.to_string(), format!("{:.1}", bonus)])
    } else if !pref.is_empty() {
        (format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND bonus_months > 0 AND bonus_months <= ?2{}", emp_filter),
         vec![pref.to_string(), format!("{:.1}", bonus)])
    } else {
        (format!("SELECT COUNT(*) FROM postings WHERE bonus_months > 0 AND bonus_months <= ?1{}", emp_filter),
         vec![format!("{:.1}", bonus)])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !muni.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND bonus_months > 0{}", emp_filter)
    } else if !pref.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND bonus_months > 0{}", emp_filter)
    } else {
        format!("SELECT COUNT(*) FROM postings WHERE bonus_months > 0{}", emp_filter)
    };
    let tp: Vec<String> = if !muni.is_empty() {
        vec![pref.to_string(), muni.to_string()]
    } else if !pref.is_empty() {
        vec![pref.to_string()]
    } else { vec![] };
    let tpp: Vec<&dyn rusqlite::types::ToSql> = tp.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let total = db.query_scalar::<i64>(&total_sql, &tpp).unwrap_or(0);

    if total > 0 { Some((below as f64 / total as f64) * 100.0) } else { None }
}

// ======== 事前計算テーブル参照 ========

fn fetch_fulfillment_grade(db: &Db, pref: &str, muni: &str, _emp_type: &str) -> Option<(f64, String)> {
    if !table_exists(db, "v2_fulfillment_summary") { return None; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT avg_score, \
          CASE WHEN avg_score < 25 THEN 'A(容易)' WHEN avg_score < 50 THEN 'B(普通)' \
          WHEN avg_score < 75 THEN 'C(やや困難)' ELSE 'D(困難)' END as grade \
          FROM v2_fulfillment_summary WHERE prefecture=?1 AND municipality=?2 LIMIT 1".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT avg_score, \
          CASE WHEN avg_score < 25 THEN 'A(容易)' WHEN avg_score < 50 THEN 'B(普通)' \
          WHEN avg_score < 75 THEN 'C(やや困難)' ELSE 'D(困難)' END as grade \
          FROM v2_fulfillment_summary WHERE prefecture=?1 AND municipality='' LIMIT 1".to_string(),
         vec![pref.to_string()])
    } else {
        return None;
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = db.query(&sql, &p).unwrap_or_default();
    rows.first().map(|r| (get_f64(r, "avg_score"), get_str(r, "grade").to_string()))
}

fn fetch_benchmark_for_diagnostic(db: &Db, pref: &str, muni: &str, emp_type: &str) -> Option<Row> {
    if !table_exists(db, "v2_region_benchmark") { return None; }

    let emp_group = if emp_type == "パート" { "パート" } else { "正社員" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT salary_competitiveness, job_market_tightness, wage_compliance, \
          industry_diversity, info_transparency, text_urgency, posting_freshness, \
          real_wage_power, labor_fluidity, working_age_ratio, population_growth, foreign_workforce, \
          composite_benchmark \
          FROM v2_region_benchmark WHERE prefecture=?1 AND municipality=?2 AND emp_group=?3 LIMIT 1".to_string(),
         vec![pref.to_string(), muni.to_string(), emp_group.to_string()])
    } else if !pref.is_empty() {
        ("SELECT salary_competitiveness, job_market_tightness, wage_compliance, \
          industry_diversity, info_transparency, text_urgency, posting_freshness, \
          real_wage_power, labor_fluidity, working_age_ratio, population_growth, foreign_workforce, \
          composite_benchmark \
          FROM v2_region_benchmark WHERE prefecture=?1 AND municipality='' AND emp_group=?2 LIMIT 1".to_string(),
         vec![pref.to_string(), emp_group.to_string()])
    } else {
        return None;
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = db.query(&sql, &p).unwrap_or_default();
    rows.into_iter().next()
}

fn fetch_compensation_for_diagnostic(db: &Db, pref: &str, muni: &str, emp_type: &str) -> Option<Row> {
    if !table_exists(db, "v2_compensation_package") { return None; }

    let emp_group = if emp_type == "パート" { "パート" } else { "正社員" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT avg_salary_min, avg_annual_holidays, avg_bonus_months, \
          salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label \
          FROM v2_compensation_package WHERE prefecture=?1 AND municipality=?2 AND industry_raw='' AND emp_group=?3 LIMIT 1".to_string(),
         vec![pref.to_string(), muni.to_string(), emp_group.to_string()])
    } else if !pref.is_empty() {
        ("SELECT avg_salary_min, avg_annual_holidays, avg_bonus_months, \
          salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label \
          FROM v2_compensation_package WHERE prefecture=?1 AND municipality='' AND industry_raw='' AND emp_group=?2 LIMIT 1".to_string(),
         vec![pref.to_string(), emp_group.to_string()])
    } else {
        return None;
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = db.query(&sql, &p).unwrap_or_default();
    rows.into_iter().next()
}

fn fetch_shadow_wage_for_diagnostic(db: &Db, pref: &str, muni: &str, emp_type: &str) -> Vec<Row> {
    if !table_exists(db, "v2_shadow_wage") { return vec![]; }

    let emp_group = if emp_type == "パート" { "パート" } else { "正社員" };

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT salary_type, total_count, p10, p25, p50, p75, p90, mean \
          FROM v2_shadow_wage WHERE prefecture=?1 AND municipality=?2 AND industry_raw='' AND emp_group=?3 \
          ORDER BY salary_type".to_string(),
         vec![pref.to_string(), muni.to_string(), emp_group.to_string()])
    } else if !pref.is_empty() {
        ("SELECT salary_type, total_count, p10, p25, p50, p75, p90, mean \
          FROM v2_shadow_wage WHERE prefecture=?1 AND municipality='' AND industry_raw='' AND emp_group=?2 \
          ORDER BY salary_type".to_string(),
         vec![pref.to_string(), emp_group.to_string()])
    } else {
        ("SELECT salary_type, SUM(total_count) as total_count, \
          AVG(p10) as p10, AVG(p25) as p25, AVG(p50) as p50, AVG(p75) as p75, AVG(p90) as p90, AVG(mean) as mean \
          FROM v2_shadow_wage WHERE municipality='' AND industry_raw='' AND emp_group=?1 \
          GROUP BY salary_type ORDER BY salary_type".to_string(),
         vec![emp_group.to_string()])
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== レンダリング ========

fn render_percentile_bar(label: &str, value_str: &str, pct: Option<f64>, color: &str) -> String {
    let pct_val = pct.unwrap_or(50.0);
    let pct_label = if pct.is_some() {
        format!("上位{:.0}%", 100.0 - pct_val)
    } else {
        "データ不足".to_string()
    };

    format!(
        r#"<div class="stat-card">
            <div class="flex justify-between items-center mb-2">
                <span class="text-sm text-slate-400">{label}</span>
                <div class="text-right">
                    <span class="text-white font-medium">{value_str}</span>
                    <span class="text-xs ml-2" style="color:{color}">{pct_label}</span>
                </div>
            </div>
            <div class="relative w-full bg-slate-700 rounded h-4">
                <div class="absolute h-4 rounded" style="width:{pct_val:.1}%;background:{color};opacity:0.7"></div>
                <div class="absolute h-6 w-0.5 bg-white -top-1" style="left:{pct_val:.1}%"></div>
            </div>
            <div class="flex justify-between text-xs text-slate-600 mt-1">
                <span>0%</span><span>25%</span><span>50%</span><span>75%</span><span>100%</span>
            </div>
        </div>"#
    )
}

/// 具体的数値を含む改善提案
fn render_actionable_suggestions(
    salary: i64, holidays: i64, bonus: f64,
    salary_pct: Option<f64>, holidays_pct: Option<f64>, bonus_pct: Option<f64>,
    shadow: &[Row], overall_grade: &str, _pref: &str,
) -> String {
    let mut suggestions: Vec<(String, &str)> = Vec::new(); // (提案テキスト, 重要度色)

    let sp = salary_pct.unwrap_or(50.0);

    // 給与分位から目標額を計算
    let monthly = shadow.iter().find(|r| {
        let st = get_str(r, "salary_type");
        st == "月給" || st.is_empty()
    });

    if sp < 25.0 {
        if let Some(row) = monthly {
            let p50 = get_f64(row, "p50") as i64;
            let diff = p50 - salary;
            if diff > 0 {
                suggestions.push((
                    format!("月給を{diff_s}円増額して{p50_s}円（中央値）にすると、ランクCからBに改善が見込めます",
                        diff_s = format_number(diff),
                        p50_s = format_number(p50)),
                    "#EF4444"
                ));
            }
        } else {
            suggestions.push((
                "給与水準が下位25%に位置しています。中央値以上への引き上げを検討してください".to_string(),
                "#EF4444"
            ));
        }
    } else if sp < 50.0 {
        if let Some(row) = monthly {
            let p75 = get_f64(row, "p75") as i64;
            let diff = p75 - salary;
            if diff > 0 {
                suggestions.push((
                    format!("月給を{diff_s}円増額して{p75_s}円にすると上位25%に入り、ランクAが狙えます",
                        diff_s = format_number(diff),
                        p75_s = format_number(p75)),
                    "#F59E0B"
                ));
            }
        }
    }

    // 休日の改善提案
    if let Some(hp) = holidays_pct {
        if hp < 30.0 && holidays > 0 && holidays < 105 {
            suggestions.push((
                format!("年間休日{}日は下位圏です。105日以上にすると法令遵守面でも安心です", holidays),
                "#EF4444"
            ));
        } else if hp < 50.0 && holidays < 120 {
            let target = 120;
            suggestions.push((
                format!("年間休日を{}日から{}日に増やすと、中央値を超えてランクアップが見込めます",
                    holidays, target),
                "#F59E0B"
            ));
        }
    } else if holidays == 0 {
        suggestions.push((
            "年間休日を明示すると求職者の安心感が向上します。120日以上が競争力の目安です".to_string(),
            "#3B82F6"
        ));
    }

    // 賞与の改善提案
    if let Some(bp) = bonus_pct {
        if bp < 25.0 && bonus > 0.0 {
            suggestions.push((
                format!("賞与{:.1}ヶ月は下位25%です。3.0ヶ月以上にすると競争力が大幅に向上します", bonus),
                "#F59E0B"
            ));
        }
    } else if bonus <= 0.0 {
        suggestions.push((
            "賞与の明示は応募率向上に効果的です。業界平均は2.0〜3.5ヶ月です".to_string(),
            "#3B82F6"
        ));
    }

    // 総合グレードがS/Aの場合のポジティブ提案
    if overall_grade == "S" || overall_grade == "A" {
        suggestions.push((
            "優良な求人条件です。求人原稿の充実（具体的な業務内容、キャリアパス、職場の雰囲気等の記載）で更なる差別化を図りましょう".to_string(),
            "#10B981"
        ));
    }

    if suggestions.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card border-l-4 border-blue-500">
        <h4 class="text-sm font-medium text-blue-400 mb-3">💡 具体的改善提案</h4>
        <ul class="space-y-2">"#);
    for (text, color) in &suggestions {
        html.push_str(&format!(
            r#"<li class="flex items-start gap-2 p-2 rounded bg-slate-800/50">
                <span class="mt-0.5 text-lg" style="color:{color}">●</span>
                <span class="text-sm text-slate-300">{text}</span>
            </li>"#
        ));
    }
    html.push_str("</ul></div>");
    html
}
