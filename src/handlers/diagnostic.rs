//! Phase 6: 条件入力→市場診断ハンドラー
//! 入力: 月給、年間休日、賞与月数、雇用形態
//! 出力: パーセンタイルバー + 充足困難度 + 改善提案

use axum::extract::{Query, State};
use axum::response::Html;
use serde::Deserialize;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::overview::{get_session_filters, make_location_label, render_no_db_data, format_number};

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

fn get_f64(row: &Row, key: &str) -> f64 {
    row.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}
fn get_i64(row: &Row, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}
fn get_str<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn table_exists(db: &Db, name: &str) -> bool {
    db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        &[&name],
    ).unwrap_or(0) > 0
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
    State(state): State<Arc<AppState>>,
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

/// API: 診断結果を返す
pub async fn evaluate_diagnostic(
    State(state): State<Arc<AppState>>,
    session: Session,
    Query(q): Query<DiagnosticQuery>,
) -> Html<String> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("市場診断")),
    };

    let filters = get_session_filters(&session).await;
    let pref = &filters.prefecture;
    let muni = &filters.municipality;
    let emp_type = q.emp_type.as_deref().unwrap_or("正社員");

    let salary = match q.salary {
        Some(s) if s > 0 => s,
        _ => return Html(r#"<div class="stat-card border-l-4 border-amber-500"><p class="text-amber-400">月給を入力してください</p></div>"#.to_string()),
    };

    let holidays = q.holidays.unwrap_or(0);
    let bonus = q.bonus.unwrap_or(0.0);

    let mut html = String::with_capacity(16_000);

    // 1. 給与パーセンタイル計算
    let salary_pct = compute_salary_percentile(db, pref, muni, emp_type, salary);
    let holidays_pct = if holidays > 0 {
        compute_holidays_percentile(db, pref, muni, emp_type, holidays)
    } else { None };
    let bonus_pct = if bonus > 0.0 {
        compute_bonus_percentile(db, pref, muni, emp_type, bonus)
    } else { None };

    // 2. 充足困難度
    let fulfillment = fetch_fulfillment_grade(db, pref, muni, emp_type);

    // 3. 報酬パッケージランク
    let comp_rank = fetch_comp_rank(db, pref, muni, emp_type);

    // ヘッダー
    html.push_str(r#"<div class="space-y-4 mt-4">"#);
    html.push_str(r#"<h3 class="text-lg font-bold text-white">診断結果</h3>"#);

    // パーセンタイルバー: 給与
    html.push_str(&render_percentile_bar(
        "月給", &format!("{}円", format_number(salary)),
        salary_pct, "#3B82F6",
    ));

    // パーセンタイルバー: 休日
    if let Some(hp) = holidays_pct {
        html.push_str(&render_percentile_bar(
            "年間休日", &format!("{}日", holidays),
            Some(hp), "#10B981",
        ));
    }

    // パーセンタイルバー: 賞与
    if let Some(bp) = bonus_pct {
        html.push_str(&render_percentile_bar(
            "賞与", &format!("{:.1}ヶ月", bonus),
            Some(bp), "#8B5CF6",
        ));
    }

    // 総合評価
    let overall_pct = salary_pct.unwrap_or(50.0);
    let grade = if overall_pct >= 75.0 { "A（上位）" }
        else if overall_pct >= 50.0 { "B（中位以上）" }
        else if overall_pct >= 25.0 { "C（中位以下）" }
        else { "D（下位）" };

    let grade_color = if overall_pct >= 75.0 { "#10B981" }
        else if overall_pct >= 50.0 { "#3B82F6" }
        else if overall_pct >= 25.0 { "#F59E0B" }
        else { "#EF4444" };

    html.push_str(&format!(
        r#"<div class="stat-card border-l-4" style="border-color:{grade_color}">
            <div class="flex items-center justify-between">
                <span class="text-sm text-slate-400">市場ポジション</span>
                <span class="text-lg font-bold" style="color:{grade_color}">{grade}</span>
            </div>
            <p class="text-xs text-slate-500 mt-1">
                この地域の{emp_type}求人の中で、給与は上位{top_pct:.0}%に位置します
            </p>
        </div>"#,
        top_pct = 100.0 - overall_pct,
    ));

    // 充足困難度（テーブルが存在する場合のみ）
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

    // 改善提案
    html.push_str(&render_improvement_suggestions(salary, holidays, bonus, overall_pct, pref));

    html.push_str("</div>");

    Html(html)
}

// ======== パーセンタイル計算 ========

fn compute_salary_percentile(db: &Db, pref: &str, muni: &str, _emp_type: &str, salary: i64) -> Option<f64> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT COUNT(*) as below FROM postings WHERE prefecture=?1 AND municipality=?2 \
          AND salary_min > 0 AND salary_type='月給' AND salary_min <= ?3".to_string(),
         vec![pref.to_string(), muni.to_string(), salary.to_string()])
    } else if !pref.is_empty() {
        ("SELECT COUNT(*) as below FROM postings WHERE prefecture=?1 \
          AND salary_min > 0 AND salary_type='月給' AND salary_min <= ?2".to_string(),
         vec![pref.to_string(), salary.to_string()])
    } else {
        ("SELECT COUNT(*) as below FROM postings WHERE salary_min > 0 AND salary_type='月給' AND salary_min <= ?1".to_string(),
         vec![salary.to_string()])
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !muni.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND municipality=?2 AND salary_min > 0 AND salary_type='月給'")
    } else if !pref.is_empty() {
        format!("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND salary_min > 0 AND salary_type='月給'")
    } else {
        "SELECT COUNT(*) FROM postings WHERE salary_min > 0 AND salary_type='月給'".to_string()
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

fn compute_holidays_percentile(db: &Db, pref: &str, _muni: &str, _emp_type: &str, holidays: i64) -> Option<f64> {
    // 都道府県レベルで計算
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND annual_holidays > 0 AND annual_holidays <= ?2".to_string(),
         vec![pref.to_string(), holidays.to_string()])
    } else {
        ("SELECT COUNT(*) FROM postings WHERE annual_holidays > 0 AND annual_holidays <= ?1".to_string(),
         vec![holidays.to_string()])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !pref.is_empty() {
        "SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND annual_holidays > 0"
    } else {
        "SELECT COUNT(*) FROM postings WHERE annual_holidays > 0"
    };
    let tp: Vec<String> = if !pref.is_empty() { vec![pref.to_string()] } else { vec![] };
    let tpp: Vec<&dyn rusqlite::types::ToSql> = tp.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let total = db.query_scalar::<i64>(total_sql, &tpp).unwrap_or(0);

    if total > 0 { Some((below as f64 / total as f64) * 100.0) } else { None }
}

fn compute_bonus_percentile(db: &Db, pref: &str, _muni: &str, _emp_type: &str, bonus: f64) -> Option<f64> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND bonus_months > 0 AND bonus_months <= ?2".to_string(),
         vec![pref.to_string(), format!("{:.1}", bonus)])
    } else {
        ("SELECT COUNT(*) FROM postings WHERE bonus_months > 0 AND bonus_months <= ?1".to_string(),
         vec![format!("{:.1}", bonus)])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let below = db.query_scalar::<i64>(&sql, &p).unwrap_or(0);

    let total_sql = if !pref.is_empty() {
        "SELECT COUNT(*) FROM postings WHERE prefecture=?1 AND bonus_months > 0"
    } else {
        "SELECT COUNT(*) FROM postings WHERE bonus_months > 0"
    };
    let tp: Vec<String> = if !pref.is_empty() { vec![pref.to_string()] } else { vec![] };
    let tpp: Vec<&dyn rusqlite::types::ToSql> = tp.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let total = db.query_scalar::<i64>(total_sql, &tpp).unwrap_or(0);

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
        return None; // 全国レベルでは非表示
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = db.query(&sql, &p).unwrap_or_default();
    rows.first().map(|r| (get_f64(r, "avg_score"), get_str(r, "grade").to_string()))
}

fn fetch_comp_rank(db: &Db, pref: &str, muni: &str, emp_type: &str) -> Option<(f64, String)> {
    if !table_exists(db, "v2_compensation_package") { return None; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT composite_score, rank_label FROM v2_compensation_package \
          WHERE prefecture=?1 AND municipality=?2 AND industry_raw='' AND emp_group=?3 LIMIT 1".to_string(),
         vec![pref.to_string(), muni.to_string(), emp_type.to_string()])
    } else if !pref.is_empty() {
        ("SELECT composite_score, rank_label FROM v2_compensation_package \
          WHERE prefecture=?1 AND municipality='' AND industry_raw='' AND emp_group=?2 LIMIT 1".to_string(),
         vec![pref.to_string(), emp_type.to_string()])
    } else {
        return None;
    };

    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    let rows = db.query(&sql, &p).unwrap_or_default();
    rows.first().map(|r| (get_f64(r, "composite_score"), get_str(r, "rank_label").to_string()))
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

fn render_improvement_suggestions(_salary: i64, holidays: i64, bonus: f64, salary_pct: f64, _pref: &str) -> String {
    let mut suggestions = Vec::new();

    if salary_pct < 25.0 {
        suggestions.push("給与水準が下位25%に位置しています。競争力向上には月給の見直しを検討してください。");
    } else if salary_pct < 50.0 {
        suggestions.push("給与水準は中位以下です。地域の給与トレンドに注目してください。");
    }

    if holidays > 0 && holidays < 105 {
        suggestions.push("年間休日105日未満は求職者にとって不利な条件です。最低でも105日以上を推奨します。");
    }

    if bonus <= 0.0 {
        suggestions.push("賞与が未設定です。賞与の明示は応募率向上に効果的です。");
    }

    if salary_pct >= 75.0 && holidays >= 120 && bonus >= 3.0 {
        suggestions.push("優良な求人条件です。求人原稿の質（具体的な仕事内容の記載等）で差別化を図りましょう。");
    }

    if suggestions.is_empty() {
        return String::new();
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card border-l-4 border-blue-500">
        <h4 class="text-sm font-medium text-blue-400 mb-2">改善提案</h4>
        <ul class="space-y-1">"#);
    for s in &suggestions {
        html.push_str(&format!(
            r#"<li class="text-xs text-slate-300 flex items-start gap-2">
                <span class="text-blue-400 mt-0.5">▸</span><span>{s}</span>
            </li>"#
        ));
    }
    html.push_str("</ul></div>");
    html
}
