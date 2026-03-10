//! V2独自分析ハンドラー
//! Phase 1: C-4(欠員補充率), S-2(地域レジリエンス), C-1(透明性スコア)
//! 全指標は雇用形態（正社員/パート/その他）でセグメント化

use axum::extract::State;
use axum::response::Html;
use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;
use tower_sessions::Session;

use crate::AppState;
use super::overview::{get_session_filters, make_location_label, render_no_db_data, format_number};

/// タブ: 企業分析（V2独自分析）
pub async fn tab_analysis(
    State(state): State<Arc<AppState>>,
    session: Session,
) -> Html<String> {
    let filters = get_session_filters(&session).await;

    let db = match &state.hw_db {
        Some(db) => db,
        None => return Html(render_no_db_data("企業分析")),
    };

    let cache_key = format!("v2analysis_{}_{}_{}",
        filters.industry_cache_key(), filters.prefecture, filters.municipality);
    if let Some(cached) = state.cache.get(&cache_key) {
        if let Some(html) = cached.as_str() {
            return Html(html.to_string());
        }
    }

    let location = make_location_label(&filters.prefecture, &filters.municipality);
    let industry = filters.industry_label();

    // v2テーブルからデータ取得
    let vacancy = fetch_vacancy_data(db, &filters.prefecture, &filters.municipality);
    let resilience = fetch_resilience_data(db, &filters.prefecture, &filters.municipality);
    let transparency = fetch_transparency_data(db, &filters.prefecture, &filters.municipality);

    let html = render_analysis(&industry, &location, &vacancy, &resilience, &transparency);

    state.cache.set(cache_key, Value::String(html.clone()));
    Html(html)
}

// --- データ取得 ---

type Row = HashMap<String, Value>;

fn fetch_vacancy_data(db: &crate::db::local_sqlite::LocalDb, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params) = if !muni.is_empty() {
        (
            "SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
             FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
             FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国: 都道府県集計を全てSUMして算出
        (
            "SELECT emp_group, SUM(total_count) as total_count, SUM(vacancy_count) as vacancy_count, \
             SUM(growth_count) as growth_count, SUM(new_facility_count) as new_facility_count, \
             CAST(SUM(vacancy_count) AS REAL) / SUM(total_count) as vacancy_rate, \
             CAST(SUM(growth_count) AS REAL) / SUM(total_count) as growth_rate \
             FROM v2_vacancy_rate WHERE municipality = '' AND industry_raw = '' \
             GROUP BY emp_group ORDER BY emp_group".to_string(),
            vec![],
        )
    };

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = params.iter()
        .map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &params_ref).unwrap_or_default()
}

fn fetch_resilience_data(db: &crate::db::local_sqlite::LocalDb, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params) = if !muni.is_empty() {
        (
            "SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
             top_industry, top_industry_share, hhi \
             FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = ?2 \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
             top_industry, top_industry_share, hhi \
             FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = '' \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国はレジリエンス意味がないので都道府県TOP10
        (
            "SELECT prefecture as emp_group, total_count, industry_count, shannon_index, evenness, \
             top_industry, top_industry_share, hhi \
             FROM v2_regional_resilience WHERE municipality = '' AND emp_group = '正社員' \
             ORDER BY shannon_index DESC LIMIT 10".to_string(),
            vec![],
        )
    };

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = params.iter()
        .map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &params_ref).unwrap_or_default()
}

fn fetch_transparency_data(db: &crate::db::local_sqlite::LocalDb, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params) = if !muni.is_empty() {
        (
            "SELECT emp_group, total_count, avg_transparency, median_transparency, \
             disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
             disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
             disclosure_parttime_ratio, disclosure_founding_year \
             FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string(), muni.to_string()],
        )
    } else if !pref.is_empty() {
        (
            "SELECT emp_group, total_count, avg_transparency, median_transparency, \
             disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
             disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
             disclosure_parttime_ratio, disclosure_founding_year \
             FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
             ORDER BY emp_group".to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT emp_group, SUM(total_count) as total_count, \
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
             GROUP BY emp_group ORDER BY emp_group".to_string(),
            vec![],
        )
    };

    let params_ref: Vec<&dyn rusqlite::types::ToSql> = params.iter()
        .map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &params_ref).unwrap_or_default()
}

// --- ヘルパー ---

fn get_f64(row: &Row, key: &str) -> f64 {
    row.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
}

fn get_i64(row: &Row, key: &str) -> i64 {
    row.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
}

fn get_str<'a>(row: &'a Row, key: &str) -> &'a str {
    row.get(key).and_then(|v| v.as_str()).unwrap_or("")
}

fn pct(v: f64) -> String {
    format!("{:.1}%", v * 100.0)
}

fn pct_bar(v: f64, color: &str) -> String {
    let w = (v * 100.0).min(100.0).max(0.0);
    format!(
        r#"<div class="w-full bg-slate-700 rounded h-2"><div class="rounded h-2" style="width:{w:.1}%;background:{color}"></div></div>"#
    )
}

/// 欠員率→色（高いほど赤）
fn vacancy_color(rate: f64) -> &'static str {
    if rate >= 0.4 { "#ef4444" }      // red
    else if rate >= 0.3 { "#f97316" }  // orange
    else if rate >= 0.2 { "#eab308" }  // yellow
    else { "#22c55e" }                 // green
}

/// 透明度→色（高いほど緑）
fn transparency_color(score: f64) -> &'static str {
    if score >= 0.8 { "#22c55e" }
    else if score >= 0.6 { "#eab308" }
    else if score >= 0.4 { "#f97316" }
    else { "#ef4444" }
}

/// 均等度→色（高いほど緑）
fn evenness_color(ev: f64) -> &'static str {
    if ev >= 0.7 { "#22c55e" }
    else if ev >= 0.5 { "#eab308" }
    else { "#f97316" }
}

// --- HTML描画 ---

fn render_analysis(
    industry: &str,
    location: &str,
    vacancy: &[Row],
    resilience: &[Row],
    transparency: &[Row],
) -> String {
    let mut html = String::with_capacity(16_000);

    html.push_str(&format!(
        r#"<div class="space-y-6">
        <h2 class="text-xl font-bold text-white">企業分析 <span class="text-blue-400 text-base font-normal">{location} {industry}</span></h2>
        <p class="text-xs text-slate-500">雇用形態別に求人市場の構造を分析します</p>"#
    ));

    // C-4: 欠員補充率セクション
    html.push_str(&render_vacancy_section(vacancy));

    // S-2: 地域レジリエンスセクション
    html.push_str(&render_resilience_section(resilience));

    // C-1: 透明性スコアセクション
    html.push_str(&render_transparency_section(transparency));

    html.push_str("</div>");
    html
}

fn render_vacancy_section(data: &[Row]) -> String {
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

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{vc}">{}</div>
                <div class="text-xs text-slate-400 mb-3">({} / {} 件)</div>
                {}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400">
                        <span>増員</span><span class="text-cyan-400">{} ({}件)</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>新規設立</span><span class="text-emerald-400">{} ({}件)</span>
                    </div>
                </div>
            </div>"#,
            pct(vr),
            format_number(vac_cnt), format_number(total),
            pct_bar(vr, vc),
            pct(gr), format_number(grow_cnt),
            pct(new_cnt as f64 / total as f64), format_number(new_cnt),
        ));
    }

    html.push_str("</div></div>");
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

        // 均等度の判定ラベル
        let label = if evenness >= 0.7 { "分散（良好）" }
            else if evenness >= 0.5 { "やや集中" }
            else { "集中（リスク）" };

        let bar_html = pct_bar(evenness, ec);
        let top_ind_short = if top_ind.chars().count() > 12 {
            top_ind.chars().take(12).collect::<String>() + "…"
        } else {
            top_ind.to_string()
        };
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
                    <div class="flex justify-between text-slate-400">
                        <span>Shannon指数</span><span class="text-white">{shannon:.3}</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>産業数</span><span class="text-white">{n_ind}</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>HHI</span><span class="text-white">{hhi:.4}</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>最大産業</span><span class="text-amber-400 truncate ml-1" title="{top_ind}">{top_ind_short}</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>最大シェア</span><span class="text-amber-400">{top_share_s}</span>
                    </div>
                    <div class="flex justify-between text-slate-400">
                        <span>求人数</span><span class="text-white">{total_s}</span>
                    </div>
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

    // KPIカード
    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let avg = get_f64(row, "avg_transparency");
        let tc = transparency_color(avg);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{tc}">{}</div>
                <div class="text-xs text-slate-400 mb-2">平均開示率 ({} 件)</div>
                {}
            </div>"#,
            pct(avg), format_number(total), pct_bar(avg, tc),
        ));
    }

    html.push_str(r#"</div>
        <h4 class="text-xs text-slate-400 mb-2">項目別開示率</h4>
        <div style="overflow-x:auto;">
        <table class="data-table text-xs">
            <thead><tr>
                <th>項目</th>"#);

    // ヘッダー: 雇用形態
    for row in data {
        let grp = get_str(row, "emp_group");
        html.push_str(&format!(r#"<th class="text-center">{grp}</th>"#));
    }
    html.push_str("</tr></thead><tbody>");

    // 項目行
    let items = [
        ("disclosure_annual_holidays", "年間休日"),
        ("disclosure_bonus_months", "賞与（月数）"),
        ("disclosure_employee_count", "従業員数"),
        ("disclosure_capital", "資本金"),
        ("disclosure_overtime", "残業時間"),
        ("disclosure_female_ratio", "女性従業員数"),
        ("disclosure_parttime_ratio", "パート従業員数"),
        ("disclosure_founding_year", "設立年"),
    ];

    for (key, label) in &items {
        html.push_str(&format!(r#"<tr><td class="text-slate-300">{label}</td>"#));
        for row in data {
            let v = get_f64(row, key);
            let c = transparency_color(v);
            html.push_str(&format!(
                r#"<td class="text-center"><span style="color:{c}">{}</span></td>"#,
                pct(v)
            ));
        }
        html.push_str("</tr>");
    }

    html.push_str("</tbody></table></div></div>");
    html
}
