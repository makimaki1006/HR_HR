//! V2独自分析ハンドラー
//! Phase 1: C-4(欠員補充率), S-2(地域レジリエンス), C-1(透明性スコア)
//! Phase 2: L-1(テキスト温度計), L-3(異業種競合), A-1(異常値), S-1(カスケード)
//! 全指標は雇用形態（正社員/パート/その他）でセグメント化

use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::overview::{get_session_filters, make_location_label, render_no_db_data, format_number};

type Db = crate::db::local_sqlite::LocalDb;
type Row = HashMap<String, Value>;

/// HTMXパーシャル: V2独自分析（企業分析タブ末尾に遅延読み込み）
pub async fn tab_analysis(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("雇用形態別分析")),
    };

    let cache_key = format!("v2analysis_{}_{}_{}",
        filters.industry_cache_key(), filters.prefecture, filters.municipality);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let pref = &filters.prefecture;
    let muni = &filters.municipality;
    let location = make_location_label(pref, muni);
    let industry = filters.industry_label();

    // Phase 1 データ取得
    let vacancy = fetch_vacancy_data(db, pref, muni);
    let vacancy_by_industry = fetch_vacancy_by_industry(db, pref, muni);
    let resilience = fetch_resilience_data(db, pref, muni);
    let transparency = fetch_transparency_data(db, pref, muni);

    // Phase 2 データ取得（テーブル存在確認付き）
    let temperature = fetch_temperature_data(db, pref, muni);
    let competition = fetch_competition_data(db, pref);
    let anomaly = fetch_anomaly_data(db, pref, muni);
    let cascade = fetch_cascade_data(db, pref, muni);

    let mut html = String::with_capacity(32_000);

    html.push_str(&format!(
        r#"<div class="space-y-6">
        <h2 class="text-xl font-bold text-white">雇用形態別 市場構造分析 <span class="text-blue-400 text-base font-normal">{location} {industry}</span></h2>
        <p class="text-xs text-slate-500">正社員/パートで分けた求人市場の構造指標です</p>"#
    ));

    // Phase 1
    html.push_str(&render_vacancy_section(&vacancy, &vacancy_by_industry));
    html.push_str(&render_resilience_section(&resilience));
    html.push_str(&render_transparency_section(&transparency));

    // Phase 2
    if !temperature.is_empty() {
        html.push_str(&render_temperature_section(&temperature));
    }
    if !competition.is_empty() {
        html.push_str(&render_competition_section(&competition));
    }
    if !anomaly.is_empty() {
        html.push_str(&render_anomaly_section(&anomaly));
    }
    if !cascade.is_empty() {
        html.push_str(&render_cascade_section(&cascade));
    }

    html.push_str("</div>");

    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

// ======== データ取得: Phase 1 ========

fn fetch_vacancy_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, SUM(vacancy_count) as vacancy_count, \
          SUM(growth_count) as growth_count, SUM(new_facility_count) as new_facility_count, \
          CAST(SUM(vacancy_count) AS REAL) / SUM(total_count) as vacancy_rate, \
          CAST(SUM(growth_count) AS REAL) / SUM(total_count) as growth_rate \
          FROM v2_vacancy_rate WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// C-4 業種別: 正社員とパートの欠員補充率上位10業種
fn fetch_vacancy_by_industry(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (filter, params): (String, Vec<String>) = if !muni.is_empty() {
        ("prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 業種集計
        ("municipality = '' AND length(industry_raw) > 0".to_string(), vec![])
    };

    let sql = format!(
        "SELECT industry_raw, emp_group, total_count, vacancy_rate, growth_rate \
         FROM v2_vacancy_rate WHERE {filter} AND total_count >= 30 \
         ORDER BY vacancy_rate DESC LIMIT 30"
    );
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_resilience_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT prefecture as emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE municipality = '' AND emp_group = '正社員' \
          ORDER BY shannon_index DESC LIMIT 10".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_transparency_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_transparency) as avg_transparency, AVG(median_transparency) as median_transparency, \
          AVG(disclosure_annual_holidays) as disclosure_annual_holidays, \
          AVG(disclosure_bonus_months) as disclosure_bonus_months, \
          AVG(disclosure_employee_count) as disclosure_employee_count, \
          AVG(disclosure_capital) as disclosure_capital, \
          AVG(disclosure_overtime) as disclosure_overtime, \
          AVG(disclosure_female_ratio) as disclosure_female_ratio, \
          AVG(disclosure_parttime_ratio) as disclosure_parttime_ratio, \
          AVG(disclosure_founding_year) as disclosure_founding_year \
          FROM v2_transparency_score WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 2 ========

fn fetch_temperature_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    // テーブル存在確認
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_text_temperature'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_temperature, median_temperature, \
          avg_urgency_density, avg_selectivity_density \
          FROM v2_text_temperature WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_temperature, median_temperature, \
          avg_urgency_density, avg_selectivity_density \
          FROM v2_text_temperature WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_temperature) as avg_temperature, AVG(median_temperature) as median_temperature, \
          AVG(avg_urgency_density) as avg_urgency_density, AVG(avg_selectivity_density) as avg_selectivity_density \
          FROM v2_text_temperature WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_competition_data(db: &Db, pref: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cross_industry_competition'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT salary_band, education_group, emp_group, total_postings, industry_count, top_industries \
          FROM v2_cross_industry_competition WHERE prefecture = ?1 AND total_postings >= 10 \
          ORDER BY industry_count DESC LIMIT 30".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT salary_band, education_group, emp_group, \
          SUM(total_postings) as total_postings, AVG(industry_count) as industry_count, '' as top_industries \
          FROM v2_cross_industry_competition WHERE total_postings >= 10 \
          GROUP BY salary_band, education_group, emp_group \
          ORDER BY AVG(industry_count) DESC LIMIT 30".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_anomaly_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_anomaly_stats'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, metric_name, total_count, anomaly_count, anomaly_rate, \
          avg_value, stddev_value, anomaly_high_count, anomaly_low_count \
          FROM v2_anomaly_stats WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group, metric_name".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, metric_name, total_count, anomaly_count, anomaly_rate, \
          avg_value, stddev_value, anomaly_high_count, anomaly_low_count \
          FROM v2_anomaly_stats WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group, metric_name".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, metric_name, SUM(total_count) as total_count, \
          SUM(anomaly_count) as anomaly_count, \
          CAST(SUM(anomaly_count) AS REAL) / SUM(total_count) as anomaly_rate, \
          AVG(avg_value) as avg_value, AVG(stddev_value) as stddev_value, \
          SUM(anomaly_high_count) as anomaly_high_count, SUM(anomaly_low_count) as anomaly_low_count \
          FROM v2_anomaly_stats WHERE municipality = '' \
          GROUP BY emp_group, metric_name ORDER BY emp_group, metric_name".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_cascade_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cascade_summary'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0 \
          AND posting_count >= 20 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0 \
          AND posting_count >= 30 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 業種別サマリー（各雇用形態の上位）
        ("SELECT emp_group, industry_raw, SUM(posting_count) as posting_count, \
          SUM(facility_count) as facility_count, \
          AVG(avg_salary_min) as avg_salary_min, AVG(median_salary_min) as median_salary_min, \
          AVG(avg_employee_count) as avg_employee_count, AVG(avg_annual_holidays) as avg_annual_holidays, \
          CAST(SUM(vacancy_rate * posting_count) AS REAL) / SUM(posting_count) as vacancy_rate \
          FROM v2_cascade_summary WHERE municipality = '' AND length(industry_raw) > 0 \
          GROUP BY emp_group, industry_raw HAVING SUM(posting_count) >= 100 \
          ORDER BY SUM(posting_count) DESC LIMIT 20".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== ヘルパー ========

fn get_f64(row: &Row, key: &str) -> f64 {
    row.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}
fn get_i64(row: &Row, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}
fn get_str<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}
fn pct(v: f64) -> String { format!("{:.1}%", v * 100.0) }

fn pct_bar(v: f64, color: &str) -> String {
    let w = (v * 100.0).min(100.0).max(0.0);
    format!(r#"<div class="w-full bg-slate-700 rounded h-2"><div class="rounded h-2" style="width:{w:.1}%;background:{color}"></div></div>"#)
}

fn vacancy_color(rate: f64) -> &'static str {
    if rate >= 0.4 { "#ef4444" } else if rate >= 0.3 { "#f97316" } else if rate >= 0.2 { "#eab308" } else { "#22c55e" }
}
fn transparency_color(score: f64) -> &'static str {
    if score >= 0.8 { "#22c55e" } else if score >= 0.6 { "#eab308" } else if score >= 0.4 { "#f97316" } else { "#ef4444" }
}
fn evenness_color(ev: f64) -> &'static str {
    if ev >= 0.7 { "#22c55e" } else if ev >= 0.5 { "#eab308" } else { "#f97316" }
}
fn temp_color(t: f64) -> &'static str {
    if t >= 5.0 { "#ef4444" } else if t >= 2.0 { "#f97316" } else if t >= 0.0 { "#eab308" } else { "#3b82f6" }
}

fn truncate_str(s: &str, max_chars: usize) -> String {
    if s.chars().count() > max_chars {
        s.chars().take(max_chars).collect::<String>() + "…"
    } else {
        s.to_string()
    }
}

// ======== HTML描画: Phase 1 ========

fn render_vacancy_section(data: &[Row], by_industry: &[Row]) -> String {
    if data.is_empty() {
        return r#"<div class="stat-card"><h3 class="text-sm text-slate-400 mb-2">欠員補充率</h3><p class="text-slate-500 text-sm">データがありません</p></div>"#.to_string();
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">欠員補充率</h3>
        <p class="text-xs text-slate-500 mb-4">求人理由が「欠員補充」の割合。高いほど人材が定着しにくい地域・業種です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let vr = get_f64(row, "vacancy_rate");
        let gr = get_f64(row, "growth_rate");
        let vac_cnt = get_i64(row, "vacancy_count");
        let grow_cnt = get_i64(row, "growth_count");
        let new_cnt = get_i64(row, "new_facility_count");
        let vc = vacancy_color(vr);
        let new_rate = if total > 0 { new_cnt as f64 / total as f64 } else { 0.0 };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{vc}">{vr_s}</div>
                <div class="text-xs text-slate-400 mb-3">({vac_s} / {total_s} 件)</div>
                {bar}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>増員</span><span class="text-cyan-400">{gr_s} ({grow_s}件)</span></div>
                    <div class="flex justify-between text-slate-400"><span>新規設立</span><span class="text-emerald-400">{nr_s} ({new_s}件)</span></div>
                </div>
            </div>"#,
            vr_s = pct(vr), vac_s = format_number(vac_cnt), total_s = format_number(total),
            bar = pct_bar(vr, vc), gr_s = pct(gr), grow_s = format_number(grow_cnt),
            nr_s = pct(new_rate), new_s = format_number(new_cnt),
        ));
    }
    html.push_str("</div>");

    // 業種別ランキング（正社員/パート別）
    if !by_industry.is_empty() {
        html.push_str(r#"<h4 class="text-xs text-slate-400 mt-4 mb-2">業種別 欠員補充率ランキング（n≥30）</h4>
            <div style="overflow-x:auto;"><table class="data-table text-xs">
            <thead><tr><th>業種</th><th class="text-center">雇用形態</th><th class="text-right">件数</th><th class="text-right">欠員率</th><th class="text-right">増員率</th><th style="width:100px"></th></tr></thead><tbody>"#);

        for row in by_industry.iter().take(15) {
            let ind = get_str(row, "industry_raw");
            let grp = get_str(row, "emp_group");
            let n = get_i64(row, "total_count");
            let vr = get_f64(row, "vacancy_rate");
            let gr = get_f64(row, "growth_rate");
            let vc = vacancy_color(vr);
            let ind_short = truncate_str(ind, 18);

            html.push_str(&format!(
                r#"<tr><td class="text-slate-300" title="{ind}">{ind_short}</td>
                <td class="text-center text-slate-400">{grp}</td>
                <td class="text-right text-slate-400">{n_s}</td>
                <td class="text-right" style="color:{vc}">{vr_s}</td>
                <td class="text-right text-cyan-400">{gr_s}</td>
                <td>{bar}</td></tr>"#,
                n_s = format_number(n), vr_s = pct(vr), gr_s = pct(gr),
                bar = pct_bar(vr, vc),
            ));
        }
        html.push_str("</tbody></table></div>");
    }

    html.push_str("</div>");
    html
}

fn render_resilience_section(data: &[Row]) -> String {
    if data.is_empty() {
        return r#"<div class="stat-card"><h3 class="text-sm text-slate-400 mb-2">地域レジリエンス</h3><p class="text-slate-500 text-sm">データがありません</p></div>"#.to_string();
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">地域レジリエンス（産業多様性）</h3>
        <p class="text-xs text-slate-500 mb-4">Shannon多様性指数で産業の分散度を評価。均等度が高いほど特定産業への依存リスクが低い健全な雇用構造です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let n_ind = get_i64(row, "industry_count");
        let shannon = get_f64(row, "shannon_index");
        let evenness = get_f64(row, "evenness");
        let top_ind = get_str(row, "top_industry");
        let top_share = get_f64(row, "top_industry_share");
        let hhi = get_f64(row, "hhi");
        let ec = evenness_color(evenness);
        let label = if evenness >= 0.7 { "分散（良好）" } else if evenness >= 0.5 { "やや集中" } else { "集中（リスク）" };
        let bar_html = pct_bar(evenness, ec);
        let top_ind_short = truncate_str(top_ind, 12);
        let top_share_s = pct(top_share);
        let total_s = format_number(total);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{ec}">{evenness:.2}</span>
                    <span class="text-xs text-slate-400">均等度</span>
                </div>
                <div class="text-xs mb-3" style="color:{ec}">{label}</div>
                {bar_html}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>Shannon指数</span><span class="text-white">{shannon:.3}</span></div>
                    <div class="flex justify-between text-slate-400"><span>産業数</span><span class="text-white">{n_ind}</span></div>
                    <div class="flex justify-between text-slate-400"><span>HHI</span><span class="text-white">{hhi:.4}</span></div>
                    <div class="flex justify-between text-slate-400"><span>最大産業</span><span class="text-amber-400 truncate ml-1" title="{top_ind}">{top_ind_short}</span></div>
                    <div class="flex justify-between text-slate-400"><span>最大シェア</span><span class="text-amber-400">{top_share_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>求人数</span><span class="text-white">{total_s}</span></div>
                </div>
            </div>"#,
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_transparency_section(data: &[Row]) -> String {
    if data.is_empty() {
        return r#"<div class="stat-card"><h3 class="text-sm text-slate-400 mb-2">透明性スコア</h3><p class="text-slate-500 text-sm">データがありません</p></div>"#.to_string();
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">透明性スコア（情報開示率）</h3>
        <p class="text-xs text-slate-500 mb-4">任意開示項目（年休・賞与・残業等）の記載率。スコアが低い求人は重要情報を隠している可能性があります。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4 mb-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let avg = get_f64(row, "avg_transparency");
        let tc = transparency_color(avg);
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{tc}">{avg_s}</div>
                <div class="text-xs text-slate-400 mb-2">平均開示率 ({total_s} 件)</div>
                {bar}
            </div>"#,
            avg_s = pct(avg), total_s = format_number(total), bar = pct_bar(avg, tc),
        ));
    }

    html.push_str(r#"</div><h4 class="text-xs text-slate-400 mb-2">項目別開示率</h4>
        <div style="overflow-x:auto;"><table class="data-table text-xs"><thead><tr><th>項目</th>"#);

    for row in data {
        html.push_str(&format!(r#"<th class="text-center">{}</th>"#, get_str(row, "emp_group")));
    }
    html.push_str("</tr></thead><tbody>");

    let items = [
        ("disclosure_annual_holidays", "年間休日"), ("disclosure_bonus_months", "賞与（月数）"),
        ("disclosure_employee_count", "従業員数"), ("disclosure_capital", "資本金"),
        ("disclosure_overtime", "残業時間"), ("disclosure_female_ratio", "女性従業員数"),
        ("disclosure_parttime_ratio", "パート従業員数"), ("disclosure_founding_year", "設立年"),
    ];

    for (key, label) in &items {
        html.push_str(&format!(r#"<tr><td class="text-slate-300">{label}</td>"#));
        for row in data {
            let v = get_f64(row, key);
            let c = transparency_color(v);
            html.push_str(&format!(r#"<td class="text-center"><span style="color:{c}">{}</span></td>"#, pct(v)));
        }
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table></div></div>");
    html
}

// ======== HTML描画: Phase 2 ========

fn render_temperature_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">テキスト温度計</h3>
        <p class="text-xs text-slate-500 mb-4">求人原稿中の「急募」「未経験歓迎」等の緊急ワード密度と「経験者優遇」「即戦力」等の選択ワード密度の差分。温度が高い＝人手不足で条件緩和傾向。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let avg_t = get_f64(row, "avg_temperature");
        let urg = get_f64(row, "avg_urgency_density");
        let sel = get_f64(row, "avg_selectivity_density");
        let tc = temp_color(avg_t);

        let temp_label = if avg_t >= 5.0 { "人手不足（条件緩和）" }
            else if avg_t >= 2.0 { "やや緩和" }
            else if avg_t >= 0.0 { "標準" }
            else { "選り好み（高選択性）" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{tc}">{avg_t:.1}<span class="text-xs">‰</span></div>
                <div class="text-xs mb-3" style="color:{tc}">{temp_label}</div>
                <div class="mt-2 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>緊急ワード密度</span><span class="text-red-400">{urg:.2}‰</span></div>
                    <div class="flex justify-between text-slate-400"><span>選択ワード密度</span><span class="text-blue-400">{sel:.2}‰</span></div>
                    <div class="flex justify-between text-slate-400"><span>求人数</span><span class="text-white">{total_s}</span></div>
                </div>
            </div>"#,
            total_s = format_number(total),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_competition_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">異業種競合レーダー</h3>
        <p class="text-xs text-slate-500 mb-4">同じ給与帯×学歴×地域で求人を出している異業種の数。多いほど人材争奪が激しいセグメントです。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>給与帯</th><th>学歴</th><th>雇用形態</th><th class="text-right">求人数</th><th class="text-right">競合業種数</th><th>主な業種</th></tr></thead><tbody>"#);

    for row in data.iter().take(20) {
        let band = get_str(row, "salary_band");
        let edu = get_str(row, "education_group");
        let grp = get_str(row, "emp_group");
        let n = get_i64(row, "total_postings");
        let ic = get_f64(row, "industry_count");
        let tops = get_str(row, "top_industries");
        let tops_short = truncate_str(tops, 30);

        let ic_color = if ic >= 30.0 { "#ef4444" } else if ic >= 15.0 { "#f97316" } else { "#eab308" };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300">{band}</td><td class="text-slate-300">{edu}</td>
            <td class="text-slate-400">{grp}</td><td class="text-right text-slate-400">{n_s}</td>
            <td class="text-right font-bold" style="color:{ic_color}">{ic:.0}</td>
            <td class="text-slate-500 truncate" style="max-width:200px" title="{tops}">{tops_short}</td></tr>"#,
            n_s = format_number(n),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}

fn render_cascade_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">カスケード分析（業種別総合）</h3>
        <p class="text-xs text-slate-500 mb-4">業種×雇用形態ごとの求人数・給与・休日・欠員率を一覧比較。人材争奪の全体像を把握できます。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>業種</th><th class="text-center">雇用形態</th><th class="text-right">求人数</th><th class="text-right">施設数</th><th class="text-right">平均給与</th><th class="text-right">年間休日</th><th class="text-right">欠員率</th><th style="width:80px"></th></tr></thead><tbody>"#);

    for row in data {
        let ind = get_str(row, "industry_raw");
        let grp = get_str(row, "emp_group");
        let n = get_i64(row, "posting_count");
        let fac = get_i64(row, "facility_count");
        let avg_sal = get_f64(row, "avg_salary_min");
        let holidays = get_f64(row, "avg_annual_holidays");
        let vr = get_f64(row, "vacancy_rate");
        let vc = vacancy_color(vr);
        let ind_short = truncate_str(ind, 18);

        let sal_s = if avg_sal > 0.0 { format!("{}円", format_number(avg_sal as i64)) } else { "-".to_string() };
        let hol_s = if holidays > 0.0 { format!("{holidays:.0}日") } else { "-".to_string() };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300" title="{ind}">{ind_short}</td>
            <td class="text-center text-slate-400">{grp}</td>
            <td class="text-right text-white">{n_s}</td>
            <td class="text-right text-slate-400">{fac_s}</td>
            <td class="text-right text-emerald-400">{sal_s}</td>
            <td class="text-right text-cyan-400">{hol_s}</td>
            <td class="text-right" style="color:{vc}">{vr_s}</td>
            <td>{bar}</td></tr>"#,
            n_s = format_number(n), fac_s = format_number(fac),
            vr_s = pct(vr), bar = pct_bar(vr, vc),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}

fn render_anomaly_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">異常値検出</h3>
        <p class="text-xs text-slate-500 mb-4">地域平均から2σ以上離れた求人の割合。異常値が多い指標は地域内の格差が大きいことを示します。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>指標</th><th>雇用形態</th><th class="text-right">件数</th><th class="text-right">異常値数</th><th class="text-right">異常率</th><th class="text-right">平均</th><th class="text-right">標準偏差</th><th class="text-right">高異常</th><th class="text-right">低異常</th></tr></thead><tbody>"#);

    let metric_labels: HashMap<&str, &str> = [
        ("salary_min", "最低給与"), ("employee_count", "従業員数"),
        ("annual_holidays", "年間休日"), ("bonus_months", "賞与月数"),
    ].into_iter().collect();

    for row in data {
        let grp = get_str(row, "emp_group");
        let metric = get_str(row, "metric_name");
        let label = metric_labels.get(metric).unwrap_or(&metric);
        let total = get_i64(row, "total_count");
        let anom = get_i64(row, "anomaly_count");
        let rate = get_f64(row, "anomaly_rate");
        let avg = get_f64(row, "avg_value");
        let std = get_f64(row, "stddev_value");
        let high = get_i64(row, "anomaly_high_count");
        let low = get_i64(row, "anomaly_low_count");

        let rc = if rate >= 0.1 { "#ef4444" } else if rate >= 0.05 { "#f97316" } else { "#eab308" };

        // 指標によって表示形式を変える
        let avg_s = if metric == "salary_min" { format!("{}円", format_number(avg as i64)) }
            else if metric == "bonus_months" { format!("{avg:.1}月") }
            else { format!("{avg:.0}") };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300">{label}</td><td class="text-slate-400">{grp}</td>
            <td class="text-right text-slate-400">{total_s}</td>
            <td class="text-right" style="color:{rc}">{anom_s}</td>
            <td class="text-right" style="color:{rc}">{rate_s}</td>
            <td class="text-right text-white">{avg_s}</td>
            <td class="text-right text-slate-400">{std:.0}</td>
            <td class="text-right text-red-400">{high_s}</td>
            <td class="text-right text-blue-400">{low_s}</td></tr>"#,
            total_s = format_number(total), anom_s = format_number(anom),
            rate_s = pct(rate), high_s = format_number(high), low_s = format_number(low),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}
