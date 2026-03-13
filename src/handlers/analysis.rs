//! V2独自分析ハンドラー
//! Phase 1: C-4(欠員補充率), S-2(地域レジリエンス), C-1(透明性スコア)
//! Phase 1B: 給与構造分析, 給与競争力指数, 報酬パッケージ総合評価
//! Phase 2: L-1(テキスト温度計), L-3(異業種競合), A-1(異常値), S-1(カスケード)
//! Phase 2B: 求人原稿品質分析, キーワードプロファイル
//! Phase 3: 企業採用戦略4象限, 雇用者集中度(独占力), 空間的ミスマッチ
//! Phase 4: 外部データ統合（最低賃金マスタ, 最低賃金違反, 地域ベンチマーク）
//! Phase 5: 予測・推定（充足困難度, 地域間流動性, 給与分位表）
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

    // Phase 1B: 給与分析
    let salary_structure = fetch_salary_structure(db, pref, muni);
    let salary_comp = fetch_salary_competitiveness(db, pref, muni);
    let compensation = fetch_compensation_package(db, pref, muni);

    // Phase 2B: テキスト分析
    let text_quality = fetch_text_quality(db, pref, muni);
    let keyword_profile = fetch_keyword_profile(db, pref, muni);

    // Phase 3: 市場構造
    let employer_strategy = fetch_employer_strategy(db, pref, muni);
    let monopsony = fetch_monopsony_data(db, pref, muni);
    let spatial = if !muni.is_empty() { fetch_spatial_mismatch(db, pref, muni) } else { vec![] };

    let mut html = String::with_capacity(64_000);

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

    // Phase 1B: 給与分析
    if !salary_structure.is_empty() {
        html.push_str(&render_salary_structure_section(&salary_structure));
    }
    if !salary_comp.is_empty() {
        html.push_str(&render_salary_competitiveness_section(&salary_comp));
    }
    if !compensation.is_empty() {
        html.push_str(&render_compensation_section(&compensation));
    }

    // Phase 2B: テキスト分析
    if !text_quality.is_empty() {
        html.push_str(&render_text_quality_section(&text_quality));
    }
    if !keyword_profile.is_empty() {
        html.push_str(&render_keyword_profile_section(&keyword_profile));
    }

    // Phase 3: 市場構造
    if !employer_strategy.is_empty() {
        html.push_str(&render_employer_strategy_section(&employer_strategy));
    }
    if !monopsony.is_empty() {
        html.push_str(&render_monopsony_section(&monopsony));
    }
    if !spatial.is_empty() {
        html.push_str(&render_spatial_mismatch_section(&spatial));
    }

    // Phase 4: 外部データ統合分析
    let minimum_wage = fetch_minimum_wage(db, pref);
    let wage_compliance = fetch_wage_compliance(db, pref, muni);
    let region_benchmark = fetch_region_benchmark(db, pref, muni);

    if !minimum_wage.is_empty() || !wage_compliance.is_empty() || !region_benchmark.is_empty() {
        html.push_str(r#"<div class="border-t border-slate-700 my-6 pt-4">
            <h3 class="text-lg font-semibold text-slate-300 mb-4">外部データ統合分析</h3></div>"#);
    }
    if !minimum_wage.is_empty() {
        html.push_str(&render_minimum_wage_section(&minimum_wage, pref));
    }
    if !wage_compliance.is_empty() {
        html.push_str(&render_wage_compliance_section(&wage_compliance));
    }
    if !region_benchmark.is_empty() {
        html.push_str(&render_region_benchmark_section(&region_benchmark));
    }

    // Phase 5: 予測・推定
    let fulfillment = fetch_fulfillment_summary(db, pref, muni);
    let mobility = fetch_mobility_estimate(db, pref, muni);
    let shadow_wage = fetch_shadow_wage(db, pref, muni);

    if !fulfillment.is_empty() || !mobility.is_empty() || !shadow_wage.is_empty() {
        html.push_str(r#"<div class="border-t border-slate-700 my-6 pt-4">
            <h3 class="text-lg font-semibold text-slate-300 mb-4">予測・推定分析</h3></div>"#);
    }
    if !fulfillment.is_empty() {
        html.push_str(&render_fulfillment_section(&fulfillment));
    }
    if !mobility.is_empty() {
        html.push_str(&render_mobility_section(&mobility));
    }
    if !shadow_wage.is_empty() {
        html.push_str(&render_shadow_wage_section(&shadow_wage));
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

// ======== データ取得: Phase 1B（給与分析） ========

fn table_exists(db: &Db, name: &str) -> bool {
    db.query_scalar::<i64>(
        &format!("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='{}'", name), &[]
    ).unwrap_or(0) > 0
}

fn fetch_salary_structure(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_salary_structure") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, salary_type, total_count, avg_salary_min, avg_salary_max, \
          median_salary_min, p25_salary_min, p75_salary_min, p90_salary_min, \
          salary_spread, avg_bonus_months, estimated_annual_min \
          FROM v2_salary_structure WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group, salary_type".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, salary_type, total_count, avg_salary_min, avg_salary_max, \
          median_salary_min, p25_salary_min, p75_salary_min, p90_salary_min, \
          salary_spread, avg_bonus_months, estimated_annual_min \
          FROM v2_salary_structure WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group, salary_type".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, salary_type, SUM(total_count) as total_count, \
          AVG(avg_salary_min) as avg_salary_min, AVG(avg_salary_max) as avg_salary_max, \
          AVG(median_salary_min) as median_salary_min, AVG(p25_salary_min) as p25_salary_min, \
          AVG(p75_salary_min) as p75_salary_min, AVG(p90_salary_min) as p90_salary_min, \
          AVG(salary_spread) as salary_spread, AVG(avg_bonus_months) as avg_bonus_months, \
          AVG(estimated_annual_min) as estimated_annual_min \
          FROM v2_salary_structure WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_salary_competitiveness(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_salary_competitiveness") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, local_avg_salary, national_avg_salary, competitiveness_index, \
          percentile_rank, sample_count \
          FROM v2_salary_competitiveness WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, local_avg_salary, national_avg_salary, competitiveness_index, \
          percentile_rank, sample_count \
          FROM v2_salary_competitiveness WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, AVG(local_avg_salary) as local_avg_salary, \
          AVG(national_avg_salary) as national_avg_salary, \
          AVG(competitiveness_index) as competitiveness_index, \
          AVG(percentile_rank) as percentile_rank, SUM(sample_count) as sample_count \
          FROM v2_salary_competitiveness WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_compensation_package(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_compensation_package") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_salary_min, avg_annual_holidays, avg_bonus_months, \
          salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label \
          FROM v2_compensation_package WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_salary_min, avg_annual_holidays, avg_bonus_months, \
          salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label \
          FROM v2_compensation_package WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_salary_min) as avg_salary_min, AVG(avg_annual_holidays) as avg_annual_holidays, \
          AVG(avg_bonus_months) as avg_bonus_months, \
          AVG(salary_pctile) as salary_pctile, AVG(holidays_pctile) as holidays_pctile, \
          AVG(bonus_pctile) as bonus_pctile, AVG(composite_score) as composite_score, \
          '' as rank_label \
          FROM v2_compensation_package WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 2B（テキスト分析） ========

fn fetch_text_quality(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_text_quality") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_char_count, avg_unique_char_ratio, \
          avg_kanji_ratio, avg_numeric_ratio, avg_punctuation_density, information_score \
          FROM v2_text_quality WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_char_count, avg_unique_char_ratio, \
          avg_kanji_ratio, avg_numeric_ratio, avg_punctuation_density, information_score \
          FROM v2_text_quality WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_char_count) as avg_char_count, AVG(avg_unique_char_ratio) as avg_unique_char_ratio, \
          AVG(avg_kanji_ratio) as avg_kanji_ratio, AVG(avg_numeric_ratio) as avg_numeric_ratio, \
          AVG(avg_punctuation_density) as avg_punctuation_density, AVG(information_score) as information_score \
          FROM v2_text_quality WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_keyword_profile(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_keyword_profile") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, keyword_category, density, avg_count \
          FROM v2_keyword_profile WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group, keyword_category".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, keyword_category, density, avg_count \
          FROM v2_keyword_profile WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group, keyword_category".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, keyword_category, AVG(density) as density, AVG(avg_count) as avg_count \
          FROM v2_keyword_profile WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group, keyword_category ORDER BY emp_group, keyword_category".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 3（市場構造） ========

fn fetch_employer_strategy(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_employer_strategy_summary") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, strategy_type, count, pct \
          FROM v2_employer_strategy_summary WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group, strategy_type".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, strategy_type, count, pct \
          FROM v2_employer_strategy_summary WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group, strategy_type".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, strategy_type, SUM(count) as count, \
          AVG(pct) as pct \
          FROM v2_employer_strategy_summary WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group, strategy_type ORDER BY emp_group, strategy_type".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_monopsony_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_monopsony_index") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_name, top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_name, top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_postings) as total_postings, \
          SUM(unique_facilities) as unique_facilities, \
          AVG(hhi) as hhi, '' as concentration_level, \
          '' as top1_name, AVG(top1_share) as top1_share, \
          AVG(top3_share) as top3_share, AVG(top5_share) as top5_share, AVG(gini) as gini \
          FROM v2_monopsony_index WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

fn fetch_spatial_mismatch(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_spatial_mismatch") { return vec![]; }

    // 空間ミスマッチは市区町村レベルのみ（industry_rawフィルタなし）
    let sql = "SELECT emp_group, posting_count, avg_salary_min, \
          accessible_postings_30km, accessible_avg_salary_30km, \
          accessible_postings_60km, salary_gap_vs_accessible, isolation_score \
          FROM v2_spatial_mismatch WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string();
    let params = vec![pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== HTML描画: Phase 1B（給与分析） ========

fn salary_color(salary_min: f64) -> &'static str {
    if salary_min > 300000.0 { "#22c55e" } else if salary_min > 200000.0 { "#3b82f6" } else { "#94a3b8" }
}

fn rank_badge_color(rank: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match rank {
        "S" => ("#fbbf24", "#1e293b"),  // gold
        "A" => ("#10b981", "#ffffff"),  // emerald
        "B" => ("#3b82f6", "#ffffff"),  // blue
        "C" => ("#f59e0b", "#1e293b"),  // amber
        "D" => ("#64748b", "#ffffff"),  // slate
        _ => ("#475569", "#ffffff"),
    }
}

fn render_salary_structure_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">💰 給与構造分析</h3>
        <p class="text-xs text-slate-500 mb-4">雇用形態別の給与分布（P25-P75四分位範囲）と推定年収。給与スプレッドが大きいほど交渉余地が広い市場です。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>雇用形態</th><th>給与種別</th><th class="text-right">件数</th><th class="text-right">平均下限</th><th class="text-right">中央値</th><th class="text-right">P25</th><th class="text-right">P75</th><th class="text-right">P90</th><th class="text-right">スプレッド</th><th class="text-right">賞与</th><th class="text-right">推定年収</th><th style="width:120px">分布</th></tr></thead><tbody>"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let stype = get_str(row, "salary_type");
        let total = get_i64(row, "total_count");
        let avg_min = get_f64(row, "avg_salary_min");
        let median = get_f64(row, "median_salary_min");
        let p25 = get_f64(row, "p25_salary_min");
        let p75 = get_f64(row, "p75_salary_min");
        let p90 = get_f64(row, "p90_salary_min");
        let spread = get_f64(row, "salary_spread");
        let bonus = get_f64(row, "avg_bonus_months");
        let annual = get_f64(row, "estimated_annual_min");
        let sc = salary_color(avg_min);

        // P25-P75範囲バー（全体を0-P90として描画）
        let bar_html = if p90 > 0.0 {
            let left_pct = (p25 / p90 * 100.0).min(100.0).max(0.0);
            let width_pct = ((p75 - p25) / p90 * 100.0).min(100.0 - left_pct).max(0.0);
            let median_pct = (median / p90 * 100.0).min(100.0).max(0.0);
            format!(r#"<div class="w-full bg-slate-700 rounded h-3 relative">
                <div class="rounded h-3 opacity-60" style="position:absolute;left:{left_pct:.0}%;width:{width_pct:.0}%;background:{sc}"></div>
                <div style="position:absolute;left:{median_pct:.0}%;top:0;bottom:0;width:2px;background:#ffffff"></div>
            </div>"#)
        } else {
            String::new()
        };

        let annual_s = if annual > 0.0 { format!("{}万", (annual / 10000.0) as i64) } else { "-".to_string() };
        let bonus_s = if bonus > 0.0 { format!("{bonus:.1}月") } else { "-".to_string() };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300">{grp}</td>
            <td class="text-slate-400">{stype}</td>
            <td class="text-right text-slate-400">{total_s}</td>
            <td class="text-right" style="color:{sc}">{avg_s}</td>
            <td class="text-right text-white">{med_s}</td>
            <td class="text-right text-slate-400">{p25_s}</td>
            <td class="text-right text-slate-400">{p75_s}</td>
            <td class="text-right text-slate-400">{p90_s}</td>
            <td class="text-right text-amber-400">{spread:.0}</td>
            <td class="text-right text-cyan-400">{bonus_s}</td>
            <td class="text-right text-emerald-400">{annual_s}</td>
            <td>{bar_html}</td></tr>"#,
            total_s = format_number(total),
            avg_s = format_number(avg_min as i64),
            med_s = format_number(median as i64),
            p25_s = format_number(p25 as i64),
            p75_s = format_number(p75 as i64),
            p90_s = format_number(p90 as i64),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}

fn render_salary_competitiveness_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📊 給与競争力指数</h3>
        <p class="text-xs text-slate-500 mb-4">地域の平均給与を全国平均と比較。プラスなら全国より高水準、マイナスなら低水準です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let local = get_f64(row, "local_avg_salary");
        let national = get_f64(row, "national_avg_salary");
        let ci = get_f64(row, "competitiveness_index");
        let pctile = get_f64(row, "percentile_rank");
        let sample = get_i64(row, "sample_count");

        let ci_color = if ci >= 0.0 { "#22c55e" } else { "#ef4444" };
        let ci_sign = if ci >= 0.0 { "+" } else { "" };
        let pctile_color = if pctile >= 75.0 { "#22c55e" } else if pctile >= 50.0 { "#3b82f6" } else if pctile >= 25.0 { "#eab308" } else { "#ef4444" };

        // 地域 vs 全国 比較バー
        let max_sal = local.max(national).max(1.0);
        let local_w = (local / max_sal * 100.0).min(100.0);
        let national_w = (national / max_sal * 100.0).min(100.0);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="text-2xl font-bold mb-1" style="color:{ci_color}">{ci_sign}{ci:.1}%</div>
                <div class="text-xs mb-3" style="color:{pctile_color}">全国 {pctile:.0} パーセンタイル</div>
                <div class="space-y-2 text-xs">
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>地域平均</span><span class="text-white">{local_s}円</span></div>
                        <div class="w-full bg-slate-700 rounded h-2"><div class="rounded h-2 bg-emerald-500" style="width:{local_w:.1}%"></div></div>
                    </div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>全国平均</span><span class="text-white">{national_s}円</span></div>
                        <div class="w-full bg-slate-700 rounded h-2"><div class="rounded h-2 bg-blue-500" style="width:{national_w:.1}%"></div></div>
                    </div>
                    <div class="flex justify-between text-slate-400 mt-2"><span>サンプル数</span><span class="text-white">{sample_s}</span></div>
                </div>
            </div>"#,
            local_s = format_number(local as i64),
            national_s = format_number(national as i64),
            sample_s = format_number(sample),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_compensation_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🏆 報酬パッケージ総合評価</h3>
        <p class="text-xs text-slate-500 mb-4">給与・休日・賞与の3軸で地域の報酬水準を全国パーセンタイルで総合評価（S/A/B/C/Dランク）。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let avg_sal = get_f64(row, "avg_salary_min");
        let holidays = get_f64(row, "avg_annual_holidays");
        let bonus = get_f64(row, "avg_bonus_months");
        let sp = get_f64(row, "salary_pctile");
        let hp = get_f64(row, "holidays_pctile");
        let bp = get_f64(row, "bonus_pctile");
        let composite = get_f64(row, "composite_score");
        let rank = get_str(row, "rank_label");
        let (badge_bg, badge_fg) = rank_badge_color(rank);

        let composite_w = (composite * 100.0).min(100.0).max(0.0);
        let composite_color = if composite >= 0.7 { "#22c55e" } else if composite >= 0.5 { "#3b82f6" } else if composite >= 0.3 { "#eab308" } else { "#ef4444" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="flex items-center justify-between mb-3">
                    <div class="text-sm font-semibold text-white">{grp}</div>
                    <span class="px-3 py-1 rounded-full text-sm font-bold" style="background:{badge_bg};color:{badge_fg}">{rank_display}</span>
                </div>
                <div class="text-xs text-slate-400 mb-3">総合スコア</div>
                <div class="flex items-baseline gap-2 mb-2">
                    <span class="text-2xl font-bold" style="color:{composite_color}">{composite:.2}</span>
                    <span class="text-xs text-slate-400">/ 1.00</span>
                </div>
                <div class="w-full bg-slate-700 rounded h-2 mb-4"><div class="rounded h-2" style="width:{composite_w:.0}%;background:{composite_color}"></div></div>
                <div class="space-y-3 text-xs">
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>給与（{sal_s}円）</span><span>P{sp:.0}</span></div>
                        {sp_bar}
                    </div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>休日（{hol_s}日）</span><span>P{hp:.0}</span></div>
                        {hp_bar}
                    </div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>賞与（{bonus:.1}月）</span><span>P{bp:.0}</span></div>
                        {bp_bar}
                    </div>
                    <div class="flex justify-between text-slate-400 mt-2 pt-2 border-t border-slate-700"><span>求人数</span><span class="text-white">{total_s}</span></div>
                </div>
            </div>"#,
            rank_display = if rank.is_empty() { "-" } else { rank },
            sal_s = format_number(avg_sal as i64),
            hol_s = format!("{holidays:.0}"),
            sp_bar = pct_bar(sp / 100.0, "#22c55e"),
            hp_bar = pct_bar(hp / 100.0, "#3b82f6"),
            bp_bar = pct_bar(bp / 100.0, "#f59e0b"),
            total_s = format_number(total),
        ));
    }

    html.push_str("</div></div>");
    html
}

// ======== HTML描画: Phase 2B（テキスト分析） ========

fn info_score_color(score: f64) -> &'static str {
    if score >= 0.8 { "#22c55e" } else if score >= 0.6 { "#3b82f6" } else if score >= 0.4 { "#eab308" } else { "#ef4444" }
}

fn render_text_quality_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📝 求人原稿品質分析</h3>
        <p class="text-xs text-slate-500 mb-4">求人原稿の文字数・語彙多様性・漢字比率等から情報スコアを算出。高いほど情報量が多く具体的な原稿です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let chars = get_f64(row, "avg_char_count");
        let unique_ratio = get_f64(row, "avg_unique_char_ratio");
        let kanji = get_f64(row, "avg_kanji_ratio");
        let numeric = get_f64(row, "avg_numeric_ratio");
        let punct = get_f64(row, "avg_punctuation_density");
        let info = get_f64(row, "information_score");
        let ic = info_score_color(info);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{ic}">{info:.2}</span>
                    <span class="text-xs text-slate-400">情報スコア</span>
                </div>
                {info_bar}
                <div class="mt-3 flex flex-wrap gap-2">
                    <span class="px-2 py-0.5 rounded bg-slate-700 text-slate-300 text-xs">{chars:.0}字</span>
                    <span class="px-2 py-0.5 rounded bg-slate-700 text-slate-300 text-xs">語彙{unique_pct}</span>
                    <span class="px-2 py-0.5 rounded bg-slate-700 text-slate-300 text-xs">漢字{kanji_pct}</span>
                    <span class="px-2 py-0.5 rounded bg-slate-700 text-slate-300 text-xs">数値{num_pct}</span>
                    <span class="px-2 py-0.5 rounded bg-slate-700 text-slate-300 text-xs">句読点{punct:.2}</span>
                </div>
                <div class="mt-2 text-xs text-slate-400">({total_s} 件)</div>
            </div>"#,
            info_bar = pct_bar(info, ic),
            unique_pct = pct(unique_ratio),
            kanji_pct = pct(kanji),
            num_pct = pct(numeric),
            total_s = format_number(total),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn keyword_category_label(cat: &str) -> &str {
    match cat {
        "urgent" => "急募系",
        "inexperienced" => "未経験系",
        "benefits" => "待遇系",
        "wlb" => "WLB系",
        "growth" => "成長系",
        "stability" => "安定系",
        _ => cat,
    }
}

fn keyword_category_color(cat: &str) -> &str {
    match cat {
        "urgent" => "#ef4444",
        "inexperienced" => "#f97316",
        "benefits" => "#22c55e",
        "wlb" => "#3b82f6",
        "growth" => "#a855f7",
        "stability" => "#14b8a6",
        _ => "#94a3b8",
    }
}

fn render_keyword_profile_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🔍 求人キーワード分析</h3>
        <p class="text-xs text-slate-500 mb-4">求人原稿に頻出するキーワードを6カテゴリに分類。密度が高いほどその傾向の求人が多い地域です。</p>"#);

    // 雇用形態でグループ化
    let mut groups: HashMap<String, Vec<&Row>> = HashMap::new();
    for row in data {
        let grp = get_str(row, "emp_group").to_string();
        groups.entry(grp).or_default().push(row);
    }

    let mut sorted_groups: Vec<_> = groups.into_iter().collect();
    sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));

    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#);

    for (grp, rows) in &sorted_groups {
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-3">{grp}</div>
                <div class="space-y-2">"#
        ));

        for row in rows {
            let cat = get_str(row, "keyword_category");
            let density = get_f64(row, "density");
            let avg_cnt = get_f64(row, "avg_count");
            let label = keyword_category_label(cat);
            let color = keyword_category_color(cat);
            // 密度バー（max 30‰としてスケーリング）
            let bar_w = (density / 30.0 * 100.0).min(100.0).max(0.0);

            html.push_str(&format!(
                r#"<div>
                    <div class="flex justify-between text-xs mb-1">
                        <span style="color:{color}">{label}</span>
                        <span class="text-slate-400">{density:.1}‰ (平均{avg_cnt:.1}回)</span>
                    </div>
                    <div class="w-full bg-slate-700 rounded h-2"><div class="rounded h-2" style="width:{bar_w:.1}%;background:{color}"></div></div>
                </div>"#
            ));
        }

        html.push_str("</div></div>");
    }

    html.push_str("</div></div>");
    html
}

// ======== HTML描画: Phase 3（市場構造） ========

fn strategy_color(stype: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match stype {
        "プレミアム型" => ("#065f46", "#6ee7b7"),     // emerald dark bg
        "給与一本勝負型" => ("#1e3a5f", "#93c5fd"),   // blue dark bg
        "福利厚生重視型" => ("#78350f", "#fcd34d"),    // amber dark bg
        "コスト優先型" => ("#334155", "#94a3b8"),      // slate dark bg
        _ => ("#1e293b", "#cbd5e1"),
    }
}

fn render_employer_strategy_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🎯 企業採用戦略の4象限</h3>
        <p class="text-xs text-slate-500 mb-4">給与×福利厚生の2軸で企業の採用戦略を4分類。プレミアム型が多い地域は人材獲得競争が激しい傾向です。</p>"#);

    // 雇用形態でグループ化
    let mut groups: HashMap<String, Vec<&Row>> = HashMap::new();
    for row in data {
        let grp = get_str(row, "emp_group").to_string();
        groups.entry(grp).or_default().push(row);
    }

    let mut sorted_groups: Vec<_> = groups.into_iter().collect();
    sorted_groups.sort_by(|a, b| a.0.cmp(&b.0));

    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#);

    for (grp, rows) in &sorted_groups {
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-3">{grp}</div>
                <div class="grid grid-cols-2 gap-2">"#
        ));

        for row in rows {
            let stype = get_str(row, "strategy_type");
            let count = get_i64(row, "count");
            let pct_val = get_f64(row, "pct");
            let (bg, fg) = strategy_color(stype);

            html.push_str(&format!(
                r#"<div class="rounded-lg p-3 text-center" style="background:{bg}">
                    <div class="text-xs font-semibold mb-1" style="color:{fg}">{stype}</div>
                    <div class="text-lg font-bold" style="color:{fg}">{pct_s}</div>
                    <div class="text-xs" style="color:{fg};opacity:0.7">{count_s}件</div>
                </div>"#,
                pct_s = pct(pct_val),
                count_s = format_number(count),
            ));
        }

        html.push_str("</div></div>");
    }

    html.push_str("</div></div>");
    html
}

fn concentration_badge(level: &str) -> (&'static str, &'static str) {
    // (背景色, テキスト色)
    match level {
        "高度集中" => ("#991b1b", "#fca5a5"),
        "中度集中" => ("#92400e", "#fcd34d"),
        "低度集中" => ("#166534", "#86efac"),
        "競争的" => ("#1e3a5f", "#93c5fd"),
        _ => ("#334155", "#94a3b8"),
    }
}

fn render_monopsony_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">⚖️ 雇用者集中度（独占力）</h3>
        <p class="text-xs text-slate-500 mb-4">HHI（ハーフィンダール・ハーシュマン指数）で雇用市場の独占度を評価。集中度が高いほど求職者の選択肢が限られます。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_postings");
        let facilities = get_i64(row, "unique_facilities");
        let hhi = get_f64(row, "hhi");
        let level = get_str(row, "concentration_level");
        let top1 = get_str(row, "top1_name");
        let top1_share = get_f64(row, "top1_share");
        let top3_share = get_f64(row, "top3_share");
        let top5_share = get_f64(row, "top5_share");
        let gini = get_f64(row, "gini");

        let (badge_bg, badge_fg) = concentration_badge(level);
        let top1_short = truncate_str(top1, 16);

        // HHI ゲージ（0-10000スケール、>2500で高度集中）
        let hhi_w = (hhi / 10000.0 * 100.0).min(100.0).max(0.0);
        let hhi_color = if hhi >= 2500.0 { "#ef4444" } else if hhi >= 1500.0 { "#f97316" } else if hhi >= 1000.0 { "#eab308" } else { "#22c55e" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="flex items-center justify-between mb-3">
                    <div class="text-sm font-semibold text-white">{grp}</div>
                    <span class="px-2 py-0.5 rounded text-xs font-semibold" style="background:{badge_bg};color:{badge_fg}">{level_display}</span>
                </div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{hhi_color}">{hhi:.0}</span>
                    <span class="text-xs text-slate-400">HHI</span>
                </div>
                <div class="w-full bg-slate-700 rounded h-2 mb-3"><div class="rounded h-2" style="width:{hhi_w:.1}%;background:{hhi_color}"></div></div>
                <div class="space-y-2 text-xs">
                    <div class="flex justify-between text-slate-400"><span>最大雇用者</span><span class="text-amber-400 truncate ml-1" title="{top1}">{top1_short}</span></div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>Top1シェア</span><span>{top1_pct}</span></div>
                        {top1_bar}
                    </div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>Top3シェア</span><span>{top3_pct}</span></div>
                        {top3_bar}
                    </div>
                    <div>
                        <div class="flex justify-between text-slate-400 mb-1"><span>Top5シェア</span><span>{top5_pct}</span></div>
                        {top5_bar}
                    </div>
                    <div class="flex justify-between text-slate-400 pt-2 border-t border-slate-700"><span>Gini係数</span><span class="text-white">{gini:.3}</span></div>
                    <div class="flex justify-between text-slate-400"><span>施設数</span><span class="text-white">{fac_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>求人数</span><span class="text-white">{total_s}</span></div>
                </div>
            </div>"#,
            level_display = if level.is_empty() { "-" } else { level },
            top1_pct = pct(top1_share),
            top1_bar = pct_bar(top1_share, "#f59e0b"),
            top3_pct = pct(top3_share),
            top3_bar = pct_bar(top3_share, "#f97316"),
            top5_pct = pct(top5_share),
            top5_bar = pct_bar(top5_share, "#ef4444"),
            fac_s = format_number(facilities),
            total_s = format_number(total),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_spatial_mismatch_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(3_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📍 空間的ミスマッチ分析</h3>
        <p class="text-xs text-slate-500 mb-4">当該市区町村の求人と近隣30km/60km圏の求人を比較。孤立スコアが高いほど周辺に選択肢が少ない「求人砂漠」地域です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let count = get_i64(row, "posting_count");
        let avg_sal = get_f64(row, "avg_salary_min");
        let acc_30 = get_i64(row, "accessible_postings_30km");
        let acc_sal_30 = get_f64(row, "accessible_avg_salary_30km");
        let acc_60 = get_i64(row, "accessible_postings_60km");
        let sal_gap = get_f64(row, "salary_gap_vs_accessible");
        let isolation = get_f64(row, "isolation_score");

        let iso_color = if isolation >= 0.7 { "#ef4444" } else if isolation >= 0.4 { "#f97316" } else if isolation >= 0.2 { "#eab308" } else { "#22c55e" };
        let iso_label = if isolation >= 0.7 { "高孤立（求人砂漠）" } else if isolation >= 0.4 { "やや孤立" } else if isolation >= 0.2 { "標準" } else { "アクセス良好" };

        let gap_color = if sal_gap >= 0.0 { "#22c55e" } else { "#ef4444" };
        let gap_sign = if sal_gap >= 0.0 { "+" } else { "" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{iso_color}">{isolation:.2}</span>
                    <span class="text-xs text-slate-400">孤立スコア</span>
                </div>
                <div class="text-xs mb-3" style="color:{iso_color}">{iso_label}</div>
                {iso_bar}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>当地の求人数</span><span class="text-white">{count_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>当地の平均給与</span><span class="text-white">{sal_s}円</span></div>
                    <div class="flex justify-between text-slate-400 pt-1 border-t border-slate-700"><span>30km圏 求人数</span><span class="text-cyan-400">{acc30_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>30km圏 平均給与</span><span class="text-cyan-400">{accsal30_s}円</span></div>
                    <div class="flex justify-between text-slate-400"><span>60km圏 求人数</span><span class="text-blue-400">{acc60_s}</span></div>
                    <div class="flex justify-between text-slate-400 pt-1 border-t border-slate-700"><span>給与ギャップ</span><span style="color:{gap_color}">{gap_sign}{sal_gap:.1}%</span></div>
                </div>
            </div>"#,
            iso_bar = pct_bar(isolation, iso_color),
            count_s = format_number(count),
            sal_s = format_number(avg_sal as i64),
            acc30_s = format_number(acc_30),
            accsal30_s = format_number(acc_sal_30 as i64),
            acc60_s = format_number(acc_60),
        ));
    }

    html.push_str("</div></div>");
    html
}

// ======== データ取得: Phase 4（外部データ統合） ========

/// Phase 4-1: 最低賃金マスタ
fn fetch_minimum_wage(db: &Db, pref: &str) -> Vec<Row> {
    if !table_exists(db, "v2_external_minimum_wage") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// Phase 4-2: 最低賃金違反チェック
fn fetch_wage_compliance(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_wage_compliance") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_hourly_postings, min_wage, below_min_count, below_min_rate, \
          avg_hourly_wage, median_hourly_wage \
          FROM v2_wage_compliance WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_hourly_postings, min_wage, below_min_count, below_min_rate, \
          avg_hourly_wage, median_hourly_wage \
          FROM v2_wage_compliance WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_hourly_postings) as total_hourly_postings, \
          AVG(min_wage) as min_wage, SUM(below_min_count) as below_min_count, \
          CAST(SUM(below_min_count) AS REAL) / SUM(total_hourly_postings) as below_min_rate, \
          AVG(avg_hourly_wage) as avg_hourly_wage, AVG(median_hourly_wage) as median_hourly_wage \
          FROM v2_wage_compliance WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// Phase 4-3: 地域ベンチマーク（6軸レーダー用）
fn fetch_region_benchmark(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_region_benchmark") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, posting_activity, salary_competitiveness, talent_retention, \
          industry_diversity, info_transparency, text_temperature, composite_benchmark \
          FROM v2_region_benchmark WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, posting_activity, salary_competitiveness, talent_retention, \
          industry_diversity, info_transparency, text_temperature, composite_benchmark \
          FROM v2_region_benchmark WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, AVG(posting_activity) as posting_activity, \
          AVG(salary_competitiveness) as salary_competitiveness, \
          AVG(talent_retention) as talent_retention, \
          AVG(industry_diversity) as industry_diversity, \
          AVG(info_transparency) as info_transparency, \
          AVG(text_temperature) as text_temperature, \
          AVG(composite_benchmark) as composite_benchmark \
          FROM v2_region_benchmark WHERE municipality = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 5（予測・推定） ========

/// Phase 5-1: 充足困難度予測
fn fetch_fulfillment_summary(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_fulfillment_summary") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_score, grade_a_pct, grade_b_pct, grade_c_pct, grade_d_pct \
          FROM v2_fulfillment_summary WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_score, grade_a_pct, grade_b_pct, grade_c_pct, grade_d_pct \
          FROM v2_fulfillment_summary WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_score) as avg_score, AVG(grade_a_pct) as grade_a_pct, \
          AVG(grade_b_pct) as grade_b_pct, AVG(grade_c_pct) as grade_c_pct, \
          AVG(grade_d_pct) as grade_d_pct \
          FROM v2_fulfillment_summary WHERE municipality = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// Phase 5-2: 地域間流動性推定（市区町村選択時のみ）
fn fetch_mobility_estimate(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if muni.is_empty() { return vec![]; }
    if !table_exists(db, "v2_mobility_estimate") { return vec![]; }

    let sql = "SELECT emp_group, local_postings, local_avg_salary, gravity_attractiveness, \
               gravity_outflow, net_gravity, top3_destinations \
               FROM v2_mobility_estimate WHERE prefecture = ?1 AND municipality = ?2 \
               ORDER BY emp_group";
    let params = vec![pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(sql, &p).unwrap_or_default()
}

/// Phase 5-3: 給与分位テーブル
fn fetch_shadow_wage(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_shadow_wage") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, salary_type, total_count, p10, p25, p50, p75, p90, mean, stddev, iqr \
          FROM v2_shadow_wage WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group, salary_type".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, salary_type, total_count, p10, p25, p50, p75, p90, mean, stddev, iqr \
          FROM v2_shadow_wage WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group, salary_type".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, salary_type, SUM(total_count) as total_count, \
          AVG(p10) as p10, AVG(p25) as p25, AVG(p50) as p50, AVG(p75) as p75, AVG(p90) as p90, \
          AVG(mean) as mean, AVG(stddev) as stddev, AVG(iqr) as iqr \
          FROM v2_shadow_wage WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== HTML描画: Phase 4（外部データ統合） ========

fn render_minimum_wage_section(data: &[Row], pref: &str) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">💴 最低賃金マスタ</h3>
        <p class="text-xs text-slate-500 mb-4">都道府県別の地域別最低賃金（時給）。全国加重平均との比較で地域の賃金水準を把握します。</p>"#);

    // 全国平均を計算
    let national_avg = if data.len() > 1 {
        let sum: f64 = data.iter().map(|r| get_f64(r, "hourly_min_wage")).sum();
        sum / data.len() as f64
    } else if data.len() == 1 {
        get_f64(&data[0], "hourly_min_wage")
    } else {
        0.0
    };

    if !pref.is_empty() && data.len() == 1 {
        // 単一都道府県: 大きな数値表示
        let row = &data[0];
        let wage = get_f64(row, "hourly_min_wage");
        let prefecture = get_str(row, "prefecture");

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-6 text-center">
                <div class="text-sm text-slate-400 mb-2">{prefecture} の最低賃金</div>
                <div class="text-4xl font-bold text-white mb-1">{wage_s}<span class="text-lg text-slate-400">円/時</span></div>
                <p class="text-xs text-slate-500 mt-2">※全国データ選択時に他県との比較が表示されます</p>
            </div>"#,
            wage_s = format_number(wage as i64),
        ));
    } else if data.len() > 1 {
        // 全国: 上位10 / 下位10
        html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#);

        // 上位10
        html.push_str(r#"<div class="bg-navy-700/50 rounded-lg p-4">
            <h4 class="text-xs font-semibold text-emerald-400 mb-3">上位10都道府県</h4>"#);
        for row in data.iter().take(10) {
            let prefecture = get_str(row, "prefecture");
            let wage = get_f64(row, "hourly_min_wage");
            let ratio = if national_avg > 0.0 { wage / national_avg } else { 1.0 };
            let bar_w = (ratio * 50.0).min(100.0).max(0.0);
            html.push_str(&format!(
                r#"<div class="flex items-center gap-2 mb-1">
                    <span class="text-xs text-slate-300 w-16 shrink-0">{pref_name}</span>
                    <div class="flex-1 bg-slate-700 rounded h-3"><div class="rounded h-3 bg-emerald-500" style="width:{bar_w:.1}%"></div></div>
                    <span class="text-xs text-emerald-400 w-14 text-right">{wage_s}円</span>
                </div>"#,
                pref_name = truncate_str(prefecture, 6),
                wage_s = format_number(wage as i64),
            ));
        }
        html.push_str("</div>");

        // 下位10
        html.push_str(r#"<div class="bg-navy-700/50 rounded-lg p-4">
            <h4 class="text-xs font-semibold text-rose-400 mb-3">下位10都道府県</h4>"#);
        let bottom: Vec<&Row> = data.iter().rev().take(10).collect();
        for row in &bottom {
            let prefecture = get_str(row, "prefecture");
            let wage = get_f64(row, "hourly_min_wage");
            let ratio = if national_avg > 0.0 { wage / national_avg } else { 1.0 };
            let bar_w = (ratio * 50.0).min(100.0).max(0.0);
            html.push_str(&format!(
                r#"<div class="flex items-center gap-2 mb-1">
                    <span class="text-xs text-slate-300 w-16 shrink-0">{pref_name}</span>
                    <div class="flex-1 bg-slate-700 rounded h-3"><div class="rounded h-3 bg-rose-500" style="width:{bar_w:.1}%"></div></div>
                    <span class="text-xs text-rose-400 w-14 text-right">{wage_s}円</span>
                </div>"#,
                pref_name = truncate_str(prefecture, 6),
                wage_s = format_number(wage as i64),
            ));
        }
        html.push_str("</div></div>");

        html.push_str(&format!(
            r#"<div class="text-center text-xs text-slate-500 mt-2">全国平均: {avg_s}円/時（{n}都道府県）</div>"#,
            avg_s = format_number(national_avg as i64),
            n = data.len(),
        ));
    }

    html.push_str("</div>");
    html
}

fn render_wage_compliance_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">⚠️ 最低賃金違反率</h3>
        <p class="text-xs text-slate-500 mb-4">時給換算で最低賃金を下回る求人の割合。違反率が高い雇用形態・地域は労働条件の改善が急務です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_hourly_postings");
        let min_wage = get_f64(row, "min_wage");
        let below_count = get_i64(row, "below_min_count");
        let below_rate = get_f64(row, "below_min_rate");
        let avg_wage = get_f64(row, "avg_hourly_wage");
        let median_wage = get_f64(row, "median_hourly_wage");

        let rate_color = if below_rate > 0.05 { "#ef4444" } else if below_rate > 0.01 { "#f97316" } else if below_rate > 0.0 { "#eab308" } else { "#22c55e" };
        let rate_label = if below_rate > 0.05 { "要改善" } else if below_rate > 0.01 { "注意" } else if below_rate > 0.0 { "微量" } else { "適正" };

        // 平均時給 vs 最低賃金の比較バー
        let wage_ratio = if min_wage > 0.0 { (avg_wage / min_wage * 100.0).min(200.0) } else { 100.0 };
        let min_ratio = 50.0_f64;

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{rate_color}">{rate_s}</span>
                    <span class="text-xs text-slate-400">違反率</span>
                </div>
                <div class="text-xs mb-3" style="color:{rate_color}">{rate_label}（{below_s} / {total_s} 件）</div>
                {rate_bar}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>最低賃金</span><span class="text-white">{min_s}円</span></div>
                    <div class="flex justify-between text-slate-400"><span>平均時給</span><span class="text-emerald-400">{avg_s}円</span></div>
                    <div class="flex justify-between text-slate-400"><span>中央値時給</span><span class="text-cyan-400">{med_s}円</span></div>
                </div>
                <div class="mt-2">
                    <div class="text-xs text-slate-500 mb-1">平均時給 vs 最低賃金</div>
                    <div class="w-full bg-slate-700 rounded h-3 relative">
                        <div class="absolute rounded h-3 bg-emerald-500/70" style="width:{wage_bar:.1}%"></div>
                        <div class="absolute h-3 w-0.5 bg-red-500" style="left:{min_bar:.1}%"></div>
                    </div>
                    <div class="flex justify-between text-xs text-slate-500 mt-0.5">
                        <span>0</span><span class="text-red-400">最低賃金</span><span>2x</span>
                    </div>
                </div>
            </div>"#,
            rate_s = pct(below_rate),
            below_s = format_number(below_count),
            total_s = format_number(total),
            rate_bar = pct_bar(below_rate, rate_color),
            min_s = format_number(min_wage as i64),
            avg_s = format_number(avg_wage as i64),
            med_s = format_number(median_wage as i64),
            wage_bar = wage_ratio / 2.0,
            min_bar = min_ratio,
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_region_benchmark_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(5_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🎯 地域ベンチマーク（6軸）</h3>
        <p class="text-xs text-slate-500 mb-4">6つの指標で地域の求人市場を総合評価。各軸0-100のスケールで、スコアが高いほど当該地域が優位です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    let axis_labels: [(&str, &str, &str); 6] = [
        ("posting_activity", "求人活動量", "#3b82f6"),
        ("salary_competitiveness", "給与競争力", "#22c55e"),
        ("talent_retention", "人材定着度", "#8b5cf6"),
        ("industry_diversity", "産業多様性", "#f59e0b"),
        ("info_transparency", "情報透明性", "#06b6d4"),
        ("text_temperature", "原稿温度", "#ec4899"),
    ];

    for row in data {
        let grp = get_str(row, "emp_group");
        let composite = get_f64(row, "composite_benchmark");
        let comp_color = if composite >= 70.0 { "#22c55e" } else if composite >= 50.0 { "#eab308" } else { "#ef4444" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-3">
                    <span class="text-2xl font-bold" style="color:{comp_color}">{composite:.1}</span>
                    <span class="text-xs text-slate-400">総合スコア</span>
                </div>"#
        ));

        // 6軸の水平バー
        for (key, label, color) in &axis_labels {
            let val = get_f64(row, key);
            let bar_w = val.min(100.0).max(0.0);
            html.push_str(&format!(
                r#"<div class="flex items-center gap-2 mb-1.5">
                    <span class="text-xs text-slate-400 w-20 shrink-0">{label}</span>
                    <div class="flex-1 bg-slate-700 rounded h-2.5"><div class="rounded h-2.5" style="width:{bar_w:.1}%;background:{color}"></div></div>
                    <span class="text-xs text-slate-300 w-8 text-right">{val:.0}</span>
                </div>"#
            ));
        }

        html.push_str("</div>");
    }

    html.push_str("</div>");
    html.push_str(r#"<p class="text-xs text-slate-600 mt-2">※EChartsレーダーチャートは今後追加予定</p>"#);
    html.push_str("</div>");
    html
}

// ======== HTML描画: Phase 5（予測・推定） ========

fn render_fulfillment_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🔮 充足困難度予測</h3>
        <p class="text-xs text-slate-500 mb-4">求人条件・地域特性から充足の難しさを0-100で予測。スコアが高いほど人材確保が困難と推定されます。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let score = get_f64(row, "avg_score");
        let a_pct = get_f64(row, "grade_a_pct");
        let b_pct = get_f64(row, "grade_b_pct");
        let c_pct = get_f64(row, "grade_c_pct");
        let d_pct = get_f64(row, "grade_d_pct");

        let score_color = if score >= 75.0 { "#ef4444" } else if score >= 50.0 { "#f59e0b" } else { "#22c55e" };
        let score_label = if score >= 75.0 { "充足困難" } else if score >= 50.0 { "やや困難" } else { "充足容易" };

        let a_w = a_pct.min(100.0).max(0.0);
        let b_w = b_pct.min(100.0).max(0.0);
        let c_w = c_pct.min(100.0).max(0.0);
        let d_w = d_pct.min(100.0).max(0.0);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{score_color}">{score:.1}</span>
                    <span class="text-xs text-slate-400">/ 100</span>
                </div>
                <div class="text-xs mb-3" style="color:{score_color}">{score_label}（{total_s}件）</div>
                <div class="mb-3">
                    <div class="text-xs text-slate-500 mb-1">グレード分布</div>
                    <div class="w-full flex rounded h-4 overflow-hidden">
                        <div class="h-4 bg-emerald-500" style="width:{a_w:.1}%" title="A（容易）"></div>
                        <div class="h-4 bg-blue-500" style="width:{b_w:.1}%" title="B（標準）"></div>
                        <div class="h-4 bg-amber-500" style="width:{c_w:.1}%" title="C（やや困難）"></div>
                        <div class="h-4 bg-red-500" style="width:{d_w:.1}%" title="D（困難）"></div>
                    </div>
                    <div class="flex justify-between text-xs mt-1">
                        <span class="text-emerald-400">A {a_s}</span>
                        <span class="text-blue-400">B {b_s}</span>
                        <span class="text-amber-400">C {c_s}</span>
                        <span class="text-red-400">D {d_s}</span>
                    </div>
                </div>
            </div>"#,
            total_s = format_number(total),
            a_s = pct(a_pct / 100.0),
            b_s = pct(b_pct / 100.0),
            c_s = pct(c_pct / 100.0),
            d_s = pct(d_pct / 100.0),
        ));
    }

    html.push_str("</div></div>");
    html
}

fn render_mobility_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🌐 地域間流動性推定</h3>
        <p class="text-xs text-slate-500 mb-4">重力モデルに基づく人材の流入/流出推定。正値は人材を引き付ける力、負値は流出リスクを示します。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let local_postings = get_i64(row, "local_postings");
        let local_sal = get_f64(row, "local_avg_salary");
        let attractiveness = get_f64(row, "gravity_attractiveness");
        let outflow = get_f64(row, "gravity_outflow");
        let net = get_f64(row, "net_gravity");
        let top3 = get_str(row, "top3_destinations");

        let net_color = if net >= 0.0 { "#22c55e" } else { "#ef4444" };
        let net_arrow = if net >= 0.0 { "+" } else { "-" };
        let net_label = if net >= 0.0 { "流入超過（人材吸引力あり）" } else { "流出超過（人材流出リスク）" };

        let destinations: Vec<&str> = top3.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()).collect();

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{net_color}">{net_arrow}{net_abs:.2}</span>
                    <span class="text-xs text-slate-400">ネット重力</span>
                </div>
                <div class="text-xs mb-3" style="color:{net_color}">{net_label}</div>
                <div class="space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>求人数</span><span class="text-white">{post_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>平均給与</span><span class="text-white">{sal_s}円</span></div>
                    <div class="flex justify-between text-slate-400"><span>吸引力</span><span class="text-emerald-400">{attractiveness:.2}</span></div>
                    <div class="flex justify-between text-slate-400"><span>流出力</span><span class="text-red-400">{outflow:.2}</span></div>
                </div>"#,
            net_abs = net.abs(),
            post_s = format_number(local_postings),
            sal_s = format_number(local_sal as i64),
        ));

        if !destinations.is_empty() {
            html.push_str(r#"<div class="mt-2 pt-2 border-t border-slate-700">
                <div class="text-xs text-slate-500 mb-1">主要流出先</div>"#);
            for (i, dest) in destinations.iter().enumerate().take(3) {
                html.push_str(&format!(
                    r#"<div class="text-xs text-slate-300">{}. {}</div>"#, i + 1, dest
                ));
            }
            html.push_str("</div>");
        }

        html.push_str("</div>");
    }

    html.push_str("</div></div>");
    html
}

fn render_shadow_wage_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(6_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📐 給与分位表</h3>
        <p class="text-xs text-slate-500 mb-4">給与分布の詳細（10/25/50/75/90パーセンタイル）。箱ひげ風バーでP25-P75の範囲を可視化します。</p>"#);

    html.push_str(r#"<div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr>
            <th>雇用形態</th><th>給与種別</th><th class="text-right">件数</th>
            <th class="text-right">P10</th><th class="text-right">P25</th>
            <th class="text-right">P50</th><th class="text-right">P75</th>
            <th class="text-right">P90</th><th class="text-right">平均</th>
            <th style="width:120px">分布</th>
        </tr></thead><tbody>"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let sal_type = get_str(row, "salary_type");
        let count = get_i64(row, "total_count");
        let p10 = get_f64(row, "p10");
        let p25 = get_f64(row, "p25");
        let p50 = get_f64(row, "p50");
        let p75 = get_f64(row, "p75");
        let p90 = get_f64(row, "p90");
        let mean = get_f64(row, "mean");

        // 箱ひげ風バー: P90を100%として P25-P75の範囲を表示
        let max_val = p90.max(1.0);
        let box_left = (p25 / max_val * 100.0).min(100.0);
        let box_width = ((p75 - p25) / max_val * 100.0).min(100.0 - box_left).max(0.0);
        let median_pos = (p50 / max_val * 100.0).min(100.0);

        html.push_str(&format!(
            r#"<tr>
                <td class="text-slate-300">{grp}</td>
                <td class="text-slate-400">{sal_type}</td>
                <td class="text-right text-slate-400">{count_s}</td>
                <td class="text-right text-slate-500">{p10_s}</td>
                <td class="text-right text-blue-400">{p25_s}</td>
                <td class="text-right text-white font-semibold">{p50_s}</td>
                <td class="text-right text-blue-400">{p75_s}</td>
                <td class="text-right text-slate-500">{p90_s}</td>
                <td class="text-right text-emerald-400">{mean_s}</td>
                <td>
                    <div class="w-full bg-slate-700 rounded h-3 relative">
                        <div class="absolute rounded h-3 bg-blue-500/40" style="left:{box_left:.1}%;width:{box_width:.1}%"></div>
                        <div class="absolute h-3 w-0.5 bg-white" style="left:{median_pos:.1}%"></div>
                    </div>
                </td>
            </tr>"#,
            count_s = format_number(count),
            p10_s = format_number(p10 as i64),
            p25_s = format_number(p25 as i64),
            p50_s = format_number(p50 as i64),
            p75_s = format_number(p75 as i64),
            p90_s = format_number(p90 as i64),
            mean_s = format_number(mean as i64),
        ));
    }

    html.push_str("</tbody></table></div>");
    html.push_str("</div>");
    html
}
