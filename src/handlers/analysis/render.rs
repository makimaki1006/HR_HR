//! HTML描画関数（全 render_* 関数 + render_subtab_1..6）

use serde_json::Value;
use std::collections::HashMap;

use super::super::helpers::{
    cross_nav, escape_html, format_number, get_f64, get_i64, get_str_html, pct, pct_bar,
    truncate_str,
};
use super::fetch::*;
use super::helpers::{
    concentration_badge, evenness_color, get_str, info_score_color, keyword_category_color,
    keyword_category_label, rank_badge_color, salary_color, strategy_color, temp_color,
    transparency_color, vacancy_color,
};

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

// ======== サブタブ描画関数 ========

/// サブタブ1: 求人動向（vacancy, vacancy_by_industry, resilience, transparency）
pub(crate) fn render_subtab_1(db: &Db, pref: &str, muni: &str) -> String {
    let vacancy = fetch_vacancy_data(db, pref, muni);
    let vacancy_by_industry = fetch_vacancy_by_industry(db, pref, muni);
    let resilience = fetch_resilience_data(db, pref, muni);
    let transparency = fetch_transparency_data(db, pref, muni);

    let mut html = String::with_capacity(16_000);
    html.push_str(r#"<div class="space-y-6">"#);
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 text-xs text-slate-500 mb-2">関連: {} {}</div>"#,
        cross_nav("/tab/overview", "地域概況"),
        cross_nav("/tab/balance", "企業分析"),
    ));

    html.push_str(&render_vacancy_section(&vacancy, &vacancy_by_industry));
    html.push_str(&render_resilience_section(&resilience));
    html.push_str(&render_transparency_section(&transparency));

    html.push_str("</div>");
    html
}

/// サブタブ2: 給与分析（salary_structure, salary_competitiveness, compensation）
pub(crate) fn render_subtab_2(db: &Db, pref: &str, muni: &str) -> String {
    let salary_structure = fetch_salary_structure(db, pref, muni);
    let salary_comp = fetch_salary_competitiveness(db, pref, muni);
    let compensation = fetch_compensation_package(db, pref, muni);

    // 正社員データの有無をチェック（フォールバック判定用）
    let has_seishain = salary_structure
        .iter()
        .any(|r| get_str(r, "emp_group").contains("正社員"));
    let is_fallback = !has_seishain && !salary_structure.is_empty();

    let mut html = String::with_capacity(12_000);
    html.push_str(r#"<div class="space-y-6">"#);
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 text-xs text-slate-500 mb-2">関連: {} {}</div>"#,
        cross_nav("/tab/workstyle", "求人条件の給与帯"),
        cross_nav("/tab/diagnostic", "市場診断ツール"),
    ));

    // 正社員データなしの場合、パートデータで代替表示する旨を通知
    if is_fallback {
        html.push_str(r#"<div class="bg-amber-900/30 border border-amber-700 rounded-lg p-3 mb-4 text-sm text-amber-300">※正社員データなし。パートデータを表示しています</div>"#);
    }

    if !salary_structure.is_empty() {
        html.push_str(&render_salary_structure_section(&salary_structure));
    }
    if !salary_comp.is_empty() {
        html.push_str(&render_salary_competitiveness_section(&salary_comp));
    }
    if !compensation.is_empty() {
        html.push_str(&render_compensation_section(&compensation));
    }
    if salary_structure.is_empty() && salary_comp.is_empty() && compensation.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-sm">給与分析データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

/// サブタブ3: テキスト分析（text_quality, keyword_profile, temperature）
pub(crate) fn render_subtab_3(db: &Db, pref: &str, muni: &str) -> String {
    let text_quality = fetch_text_quality(db, pref, muni);
    let keyword_profile = fetch_keyword_profile(db, pref, muni);
    let temperature = fetch_temperature_data(db, pref, muni);

    let mut html = String::with_capacity(12_000);
    html.push_str(r#"<div class="space-y-6">"#);

    if !text_quality.is_empty() {
        html.push_str(&render_text_quality_section(&text_quality));
    }
    if !keyword_profile.is_empty() {
        html.push_str(&render_keyword_profile_section(&keyword_profile));
    }
    if !temperature.is_empty() {
        html.push_str(&render_temperature_section(&temperature));
    }
    if text_quality.is_empty() && keyword_profile.is_empty() && temperature.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-sm">テキスト分析データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

/// サブタブ4: 市場構造（employer_strategy, monopsony, spatial_mismatch, competition, cascade）
pub(crate) fn render_subtab_4(db: &Db, pref: &str, muni: &str) -> String {
    let employer_strategy = fetch_employer_strategy(db, pref, muni);
    let monopsony = fetch_monopsony_data(db, pref, muni);
    let spatial = if !muni.is_empty() {
        fetch_spatial_mismatch(db, pref, muni)
    } else {
        vec![]
    };
    let competition = fetch_competition_data(db, pref);
    let cascade = fetch_cascade_data(db, pref, muni);

    let mut html = String::with_capacity(16_000);
    html.push_str(r#"<div class="space-y-6">"#);
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 text-xs text-slate-500 mb-2">関連: {} {}</div>"#,
        cross_nav("/tab/balance", "企業規模・産業"),
        cross_nav("/tab/competitive", "詳細検索"),
    ));

    if !employer_strategy.is_empty() {
        html.push_str(&render_employer_strategy_section(&employer_strategy));
    }
    if !monopsony.is_empty() {
        html.push_str(&render_monopsony_section(&monopsony));
    }
    if !spatial.is_empty() {
        html.push_str(&render_spatial_mismatch_section(&spatial));
    }
    if !competition.is_empty() {
        html.push_str(&render_competition_section(&competition));
    }
    if !cascade.is_empty() {
        html.push_str(&render_cascade_section(&cascade));
    }
    if employer_strategy.is_empty()
        && monopsony.is_empty()
        && spatial.is_empty()
        && competition.is_empty()
        && cascade.is_empty()
    {
        html.push_str(r#"<p class="text-slate-500 text-sm">市場構造データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

/// サブタブ5: 異常値・外部（anomaly, minimum_wage, wage_compliance, prefecture_stats, population, region_benchmark + 新外部データ7セクション）
pub(crate) fn render_subtab_5(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> String {
    let anomaly = fetch_anomaly_data(db, pref, muni);
    let minimum_wage = fetch_minimum_wage(db, pref);
    let wage_compliance = fetch_wage_compliance(db, pref, muni);
    let prefecture_stats = fetch_prefecture_stats(db, turso, pref);
    let population = fetch_population_data(db, turso, pref, muni);
    let pyramid = fetch_population_pyramid(db, turso, pref, muni);
    let migration = fetch_migration_data(db, turso, pref, muni);
    let daytime = fetch_daytime_population(db, turso, pref, muni);
    let job_openings_ratio = fetch_job_openings_ratio(db, turso, pref);
    let labor_stats = fetch_labor_stats(db, turso, pref);
    let establishments = fetch_establishments(db, turso, pref);
    let turnover = fetch_turnover(db, turso, pref);
    let household_spending = fetch_household_spending(db, turso, pref);
    let business_dynamics = fetch_business_dynamics(db, turso, pref);
    let climate = fetch_climate(db, turso, pref);
    let care_demand = fetch_care_demand(db, turso, pref);
    let region_benchmark = fetch_region_benchmark(db, pref, muni);
    // ── 新外部データセクション（Phase 4-7） ──
    let education_data = fetch_education(db, turso, pref);
    let household_type = fetch_household_type(db, turso, pref);
    let foreign_residents = fetch_foreign_residents(db, turso, pref);
    let land_price = fetch_land_price(db, turso, pref);
    let car_ownership = fetch_car_ownership(db, turso, pref);
    let internet_usage = fetch_internet_usage(db, turso, pref);
    let social_life = fetch_social_life(db, turso, pref);
    let boj_tankan = fetch_boj_tankan(db, turso);

    let mut html = String::with_capacity(40_000);
    html.push_str(r#"<div class="space-y-6">"#);
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 text-xs text-slate-500 mb-2">関連: {} {}</div>"#,
        cross_nav("/tab/trend", "時系列トレンド"),
        cross_nav("/tab/overview", "地域概況"),
    ));

    if !anomaly.is_empty() {
        html.push_str(&render_anomaly_section(&anomaly));
    }

    let has_external = !minimum_wage.is_empty()
        || !wage_compliance.is_empty()
        || !prefecture_stats.is_empty()
        || !population.is_empty()
        || !region_benchmark.is_empty()
        || !job_openings_ratio.is_empty()
        || !labor_stats.is_empty()
        || !establishments.is_empty()
        || !turnover.is_empty()
        || !household_spending.is_empty()
        || !business_dynamics.is_empty()
        || !climate.is_empty()
        || !care_demand.is_empty()
        || !education_data.is_empty()
        || !household_type.is_empty()
        || !foreign_residents.is_empty()
        || !land_price.is_empty()
        || !car_ownership.is_empty()
        || !internet_usage.is_empty()
        || !social_life.is_empty()
        || !boj_tankan.is_empty();

    if has_external {
        html.push_str(
            r#"<div class="border-t border-slate-700 my-4 pt-4">
            <h3 class="text-lg font-semibold text-slate-300 mb-4">外部データ統合分析</h3></div>"#,
        );
    }
    if !minimum_wage.is_empty() {
        html.push_str(&render_minimum_wage_section(&minimum_wage, pref));
    }
    if !wage_compliance.is_empty() {
        html.push_str(&render_wage_compliance_section(&wage_compliance));
    }
    if !prefecture_stats.is_empty() {
        html.push_str(&render_prefecture_stats_section(&prefecture_stats, pref));
    }
    if !job_openings_ratio.is_empty() {
        html.push_str(&render_job_openings_ratio_section(
            &job_openings_ratio,
            pref,
        ));
    }
    if !labor_stats.is_empty() {
        html.push_str(&render_labor_stats_section(&labor_stats, pref));
    }
    if !population.is_empty() {
        html.push_str(&render_population_section(&population, &pyramid));
    }
    if !migration.is_empty() || !daytime.is_empty() {
        html.push_str(&render_demographics_section(&migration, &daytime));
    }
    if !establishments.is_empty() {
        html.push_str(&render_establishment_section(&establishments, pref));
    }
    if !turnover.is_empty() {
        html.push_str(&render_turnover_section(&turnover, pref));
    }
    if !household_spending.is_empty() {
        html.push_str(&render_household_spending_section(
            &household_spending,
            pref,
        ));
    }
    if !business_dynamics.is_empty() {
        html.push_str(&render_business_dynamics_section(&business_dynamics, pref));
    }
    if !climate.is_empty() {
        html.push_str(&render_climate_section(&climate, pref));
    }
    if !care_demand.is_empty() {
        html.push_str(&render_care_demand_section(&care_demand, pref));
    }
    // ── デモグラフィック新セクション ──
    if !education_data.is_empty() {
        html.push_str(&render_education_section(&education_data, pref));
    }
    if !household_type.is_empty() {
        html.push_str(&render_household_type_section(&household_type, pref));
    }
    if !foreign_residents.is_empty() {
        html.push_str(&render_foreign_residents_section(&foreign_residents, pref));
    }
    // ── ジオグラフィック新セクション ──
    if !land_price.is_empty() {
        html.push_str(&render_land_price_section(&land_price, pref));
    }
    if !car_ownership.is_empty() || !internet_usage.is_empty() {
        html.push_str(&render_regional_infra_section(
            &car_ownership,
            &internet_usage,
            pref,
        ));
    }
    // ── サイコグラフィック新セクション ──
    if !social_life.is_empty() {
        html.push_str(&render_social_life_section(&social_life, pref));
    }
    // ── マクロ経済セクション ──
    if !boj_tankan.is_empty() {
        html.push_str(&render_boj_tankan_section(&boj_tankan));
    }
    if !region_benchmark.is_empty() {
        html.push_str(&render_region_benchmark_section(&region_benchmark));
    }

    if anomaly.is_empty() && !has_external {
        html.push_str(r#"<p class="text-slate-500 text-sm">異常値・外部データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

/// サブタブ6: 予測・推定（fulfillment, mobility, shadow_wage）
pub(crate) fn render_subtab_6(db: &Db, pref: &str, muni: &str) -> String {
    let fulfillment = fetch_fulfillment_summary(db, pref, muni);
    let mobility = fetch_mobility_estimate(db, pref, muni);
    let shadow_wage = fetch_shadow_wage(db, pref, muni);

    let mut html = String::with_capacity(12_000);
    html.push_str(r#"<div class="space-y-6">"#);

    if !fulfillment.is_empty() {
        html.push_str(&render_fulfillment_section(&fulfillment));
    }
    if !mobility.is_empty() {
        html.push_str(&render_mobility_section(&mobility));
    }
    if !shadow_wage.is_empty() {
        html.push_str(&render_shadow_wage_section(&shadow_wage));
    }

    if fulfillment.is_empty() && mobility.is_empty() && shadow_wage.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-sm">予測・推定データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

// ======== HTML描画: Phase 1 ========

fn render_vacancy_section(data: &[Row], by_industry: &[Row]) -> String {
    if data.is_empty() {
        return r#"<div class="stat-card"><h3 class="text-sm text-slate-400 mb-2">欠員補充率</h3><p class="text-slate-500 text-sm">データがありません</p></div>"#.to_string();
    }

    // ECharts用データ収集
    let mut chart_labels = vec![];
    let mut vacancy_rates = vec![];
    let mut growth_rates = vec![];
    for row in data {
        chart_labels.push(get_str(row, "emp_group").to_string());
        vacancy_rates.push(format!("{:.1}", get_f64(row, "vacancy_rate") * 100.0));
        growth_rates.push(format!("{:.1}", get_f64(row, "growth_rate") * 100.0));
    }

    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">欠員補充率</h3>
        <p class="text-xs text-slate-500 mb-4">求人理由が「欠員補充」の割合。高いほど人材が定着しにくい地域・業種です。</p>"#);

    // ECharts 横棒グラフ: 欠員率 vs 増員率
    if data.len() > 1 {
        let labels_json: Vec<String> = chart_labels.iter().map(|l| format!("\"{}\"", l)).collect();
        html.push_str(&format!(
            r##"<div class="echart" style="height:250px;" data-chart-config='{{"tooltip":{{"trigger":"axis","axisPointer":{{"type":"shadow"}}}},"legend":{{"orient":"horizontal","bottom":0,"textStyle":{{"color":"#94a3b8","fontSize":11}}}},"grid":{{"left":"20%","right":"5%","top":"10%","bottom":"15%"}},"xAxis":{{"type":"value","axisLabel":{{"formatter":"{{value}}%","color":"#94a3b8"}}}},"yAxis":{{"type":"category","data":[{labels}],"axisLabel":{{"color":"#94a3b8"}}}},"series":[{{"name":"欠員率","type":"bar","data":[{vr}],"itemStyle":{{"color":"#ef4444"}}}},{{"name":"増員率","type":"bar","data":[{gr}],"itemStyle":{{"color":"#22c55e"}}}}]}}'></div>"##,
            labels = labels_json.join(","),
            vr = vacancy_rates.join(","),
            gr = growth_rates.join(","),
        ));
    }

    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "total_count");
        let vr = get_f64(row, "vacancy_rate");
        let gr = get_f64(row, "growth_rate");
        let vac_cnt = get_i64(row, "vacancy_count");
        let grow_cnt = get_i64(row, "growth_count");
        let new_cnt = get_i64(row, "new_facility_count");
        let vc = vacancy_color(vr);
        let new_rate = if total > 0 {
            new_cnt as f64 / total as f64
        } else {
            0.0
        };

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
            let ind = get_str_html(row, "industry_raw");
            let grp = get_str_html(row, "emp_group");
            let n = get_i64(row, "total_count");
            let vr = get_f64(row, "vacancy_rate");
            let gr = get_f64(row, "growth_rate");
            let vc = vacancy_color(vr);
            let ind_short = truncate_str(&ind, 18);

            html.push_str(&format!(
                r#"<tr><td class="text-slate-300" title="{ind}">{ind_short}</td>
                <td class="text-center text-slate-400">{grp}</td>
                <td class="text-right text-slate-400">{n_s}</td>
                <td class="text-right" style="color:{vc}">{vr_s}</td>
                <td class="text-right text-cyan-400">{gr_s}</td>
                <td>{bar}</td></tr>"#,
                n_s = format_number(n),
                vr_s = pct(vr),
                gr_s = pct(gr),
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
        <p class="text-xs text-slate-500 mb-4">産業の分散度を評価。業界分散度が高いほど特定産業への依存リスクが低い健全な雇用構造です。</p>"#);

    // ECharts レーダーチャート: 4指標 × 雇用形態
    if data.len() > 1 {
        let colors = ["#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#8b5cf6"];
        // 正規化用の最大値を計算
        let max_ind: f64 = data
            .iter()
            .map(|r| get_f64(r, "industry_count"))
            .fold(1.0_f64, f64::max);
        let max_hhi: f64 = data
            .iter()
            .map(|r| get_f64(r, "hhi"))
            .fold(1.0_f64, f64::max);

        let mut series_json = vec![];
        for (i, row) in data.iter().enumerate() {
            let grp = get_str(row, "emp_group");
            let shannon = get_f64(row, "shannon_index");
            let evenness = get_f64(row, "evenness");
            let n_ind = get_f64(row, "industry_count");
            let hhi = get_f64(row, "hhi");
            // 正規化: 多様性(shannon max~4), 均等度(0-1→0-100), 産業数(比率), 分散度(HHI逆数)
            let diversity_norm = (shannon / 4.0 * 100.0).min(100.0);
            let evenness_norm = evenness * 100.0;
            let industry_norm = if max_ind > 0.0 {
                n_ind / max_ind * 100.0
            } else {
                0.0
            };
            let dispersion_norm = if max_hhi > 0.0 {
                (1.0 - hhi / max_hhi) * 100.0
            } else {
                50.0
            };
            let c = colors[i % colors.len()];
            series_json.push(format!(
                r#"{{"name":"{grp}","type":"radar","data":[{{"value":[{d:.1},{e:.1},{ind:.1},{disp:.1}],"name":"{grp}"}}],"lineStyle":{{"color":"{c}"}},"itemStyle":{{"color":"{c}"}},"areaStyle":{{"color":"{c}","opacity":0.15}}}}"#,
                d = diversity_norm, e = evenness_norm, ind = industry_norm, disp = dispersion_norm,
            ));
        }

        html.push_str(&format!(
            r##"<div class="echart" style="height:280px;" data-chart-config='{{"tooltip":{{}},"legend":{{"orient":"horizontal","bottom":0,"textStyle":{{"color":"#94a3b8","fontSize":11}}}},"radar":{{"indicator":[{{"name":"多様性","max":100}},{{"name":"均等度","max":100}},{{"name":"産業数","max":100}},{{"name":"分散度","max":100}}],"axisName":{{"color":"#94a3b8"}},"splitArea":{{"areaStyle":{{"color":["rgba(30,41,59,0.3)","rgba(30,41,59,0.5)"]}}}}}},"series":[{series}]}}'></div>"##,
            series = series_json.join(","),
        ));
    }

    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str_html(row, "emp_group");
        let total = get_i64(row, "total_count");
        let n_ind = get_i64(row, "industry_count");
        let shannon = get_f64(row, "shannon_index");
        let evenness = get_f64(row, "evenness");
        let top_ind = get_str_html(row, "top_industry");
        let top_share = get_f64(row, "top_industry_share");
        let hhi = get_f64(row, "hhi");
        let ec = evenness_color(evenness);
        let label = if evenness >= 0.7 {
            "分散（良好）"
        } else if evenness >= 0.5 {
            "やや集中"
        } else {
            "集中（リスク）"
        };
        let bar_html = pct_bar(evenness, ec);
        let top_ind_short = truncate_str(&top_ind, 12);
        let top_share_s = pct(top_share);
        let total_s = format_number(total);

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{ec}">{evenness:.2}</span>
                    <span class="text-xs text-slate-400">業界分散度</span>
                </div>
                <div class="text-xs mb-3" style="color:{ec}">{label}</div>
                {bar_html}
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>多様性スコア</span><span class="text-white">{shannon:.3}</span></div>
                    <div class="flex justify-between text-slate-400"><span>産業数</span><span class="text-white">{n_ind}</span></div>
                    <div class="flex justify-between text-slate-400"><span>集中度</span><span class="text-white">{hhi:.0}</span></div>
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
            avg_s = pct(avg),
            total_s = format_number(total),
            bar = pct_bar(avg, tc),
        ));
    }

    html.push_str(
        r#"</div><h4 class="text-xs text-slate-400 mb-2">項目別開示率</h4>
        <div style="overflow-x:auto;"><table class="data-table text-xs"><thead><tr><th>項目</th>"#,
    );

    for row in data {
        html.push_str(&format!(
            r#"<th class="text-center">{}</th>"#,
            get_str(row, "emp_group")
        ));
    }
    html.push_str("</tr></thead><tbody>");

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

// ======== HTML描画: Phase 2 ========

fn render_temperature_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">テキスト温度計</h3>
        <p class="text-xs text-slate-500 mb-4">求人原稿中の「急募」「未経験歓迎」等の緊急ワード密度と「経験者優遇」「即戦力」等の選択ワード密度の差分。温度が高い＝人手不足で条件緩和傾向。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let total = get_i64(row, "sample_count");
        let avg_t = get_f64(row, "temperature");
        let urg = get_f64(row, "urgency_density");
        let sel = get_f64(row, "selectivity_density");
        let tc = temp_color(avg_t);

        let temp_label = if avg_t >= 0.5 {
            "人手不足（条件緩和）"
        } else if avg_t >= 0.2 {
            "やや緩和"
        } else if avg_t >= -0.2 {
            "標準"
        } else {
            "選り好み（高選択性）"
        };

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
        let band = get_str_html(row, "salary_band");
        let edu = get_str_html(row, "education_group");
        let grp = get_str_html(row, "emp_group");
        let n = get_i64(row, "total_postings");
        let ic = get_f64(row, "industry_count");
        let tops = get_str_html(row, "top_industries");
        let tops_short = truncate_str(&tops, 30);

        let ic_color = if ic >= 30.0 {
            "#ef4444"
        } else if ic >= 15.0 {
            "#f97316"
        } else {
            "#eab308"
        };

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
        let ind = get_str_html(row, "industry_raw");
        let grp = get_str_html(row, "emp_group");
        let n = get_i64(row, "posting_count");
        let fac = get_i64(row, "facility_count");
        let avg_sal = get_f64(row, "avg_salary_min");
        let holidays = get_f64(row, "avg_annual_holidays");
        let vr = get_f64(row, "vacancy_rate");
        let vc = vacancy_color(vr);
        let ind_short = truncate_str(&ind, 18);

        let sal_s = if avg_sal > 0.0 {
            format!("{}円", format_number(avg_sal as i64))
        } else {
            "-".to_string()
        };
        let hol_s = if holidays > 0.0 {
            format!("{holidays:.0}日")
        } else {
            "-".to_string()
        };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300" title="{ind}">{ind_short}</td>
            <td class="text-center text-slate-400">{grp}</td>
            <td class="text-right text-white">{n_s}</td>
            <td class="text-right text-slate-400">{fac_s}</td>
            <td class="text-right text-emerald-400">{sal_s}</td>
            <td class="text-right text-cyan-400">{hol_s}</td>
            <td class="text-right" style="color:{vc}">{vr_s}</td>
            <td>{bar}</td></tr>"#,
            n_s = format_number(n),
            fac_s = format_number(fac),
            vr_s = pct(vr),
            bar = pct_bar(vr, vc),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}

fn render_anomaly_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">異常値検出</h3>
        <p class="text-xs text-slate-500 mb-4">地域平均から大きく外れた求人の割合。異常値が多い指標は地域内の格差が大きいことを示します。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>指標</th><th>雇用形態</th><th class="text-right">件数</th><th class="text-right">異常値数</th><th class="text-right">異常率</th><th class="text-right">平均</th><th class="text-right">バラツキ</th><th class="text-right">高異常</th><th class="text-right">低異常</th></tr></thead><tbody>"#);

    let metric_labels: HashMap<&str, &str> = [
        ("salary_min", "最低給与"),
        ("employee_count", "従業員数"),
        ("annual_holidays", "年間休日"),
        ("bonus_months", "賞与月数"),
    ]
    .into_iter()
    .collect();

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

        let rc = if rate >= 0.1 {
            "#ef4444"
        } else if rate >= 0.05 {
            "#f97316"
        } else {
            "#eab308"
        };

        // 指標によって表示形式を変える
        let avg_s = if metric == "salary_min" {
            format!("{}円", format_number(avg as i64))
        } else if metric == "bonus_months" {
            format!("{avg:.1}月")
        } else {
            format!("{avg:.0}")
        };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300">{label}</td><td class="text-slate-400">{grp}</td>
            <td class="text-right text-slate-400">{total_s}</td>
            <td class="text-right" style="color:{rc}">{anom_s}</td>
            <td class="text-right" style="color:{rc}">{rate_s}</td>
            <td class="text-right text-white">{avg_s}</td>
            <td class="text-right text-slate-400">{std:.0}</td>
            <td class="text-right text-red-400">{high_s}</td>
            <td class="text-right text-blue-400">{low_s}</td></tr>"#,
            total_s = format_number(total),
            anom_s = format_number(anom),
            rate_s = pct(rate),
            high_s = format_number(high),
            low_s = format_number(low),
        ));
    }

    html.push_str("</tbody></table></div></div>");
    html
}

// ======== HTML描画: Phase 1B（給与分析） ========

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
            format!(
                r#"<div class="w-full bg-slate-700 rounded h-3 relative">
                <div class="rounded h-3 opacity-60" style="position:absolute;left:{left_pct:.0}%;width:{width_pct:.0}%;background:{sc}"></div>
                <div style="position:absolute;left:{median_pct:.0}%;top:0;bottom:0;width:2px;background:#ffffff"></div>
            </div>"#
            )
        } else {
            String::new()
        };

        // 時給/月給の単位を明示
        let is_hourly = stype.contains("時給");
        let annual_s = if is_hourly {
            "-".to_string()
        } else if annual > 0.0 {
            format!("{}万", (annual / 10000.0) as i64)
        } else {
            "-".to_string()
        };
        let bonus_s = if bonus > 0.0 {
            format!("{bonus:.1}月")
        } else {
            "-".to_string()
        };

        html.push_str(&format!(
            r#"<tr><td class="text-slate-300">{grp}</td>
            <td class="text-slate-400">{stype_display}</td>
            <td class="text-right text-slate-400">{total_s}</td>
            <td class="text-right" style="color:{sc}">{avg_s}{unit}</td>
            <td class="text-right text-white">{med_s}{unit}</td>
            <td class="text-right text-slate-400">{p25_s}{unit}</td>
            <td class="text-right text-slate-400">{p75_s}{unit}</td>
            <td class="text-right text-slate-400">{p90_s}{unit}</td>
            <td class="text-right text-amber-400">{spread:.0}</td>
            <td class="text-right text-cyan-400">{bonus_s}</td>
            <td class="text-right text-emerald-400">{annual_s}</td>
            <td>{bar_html}</td></tr>"#,
            stype_display = stype,
            unit = if is_hourly {
                "<span class='text-[10px] text-slate-500'>/時</span>"
            } else {
                ""
            },
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
    let mut html = String::with_capacity(5_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📊 給与競争力指数</h3>
        <p class="text-xs text-slate-500 mb-4">地域の平均給与を全国平均と比較。プラスなら全国より高水準、マイナスなら低水準です。</p>"#);

    // ECharts ゲージチャート: パーセンタイルランク
    if !data.is_empty() {
        let colors = ["#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#8b5cf6"];
        let n = data.len();
        let axis_color_arr =
            r##"[[0.25,"#ef4444"],[0.5,"#eab308"],[0.75,"#3b82f6"],[1,"#22c55e"]]"##;
        let title_color = "#94a3b8";
        let mut series_json = vec![];
        for (i, row) in data.iter().enumerate() {
            let grp = get_str(row, "emp_group");
            let pctile = get_f64(row, "percentile_rank");
            let c = colors[i % colors.len()];
            let center_x = if n > 1 {
                (100.0 / (n as f64 + 1.0)) * (i as f64 + 1.0)
            } else {
                50.0
            };
            let s = format!(
                r#"{{"type":"gauge","center":["{cx:.0}%","55%"],"radius":"70%","startAngle":200,"endAngle":-20,"min":0,"max":100,"detail":{{"formatter":"P{{value}}","fontSize":14,"color":"{c}","offsetCenter":[0,"70%"]}},"title":{{"fontSize":11,"color":"{tc}","offsetCenter":[0,"90%"]}},"data":[{{"value":{pctile:.0},"name":"{grp}"}}],"axisLine":{{"lineStyle":{{"width":12,"color":{acol}}}}},"axisTick":{{"show":false}},"splitLine":{{"show":false}},"axisLabel":{{"show":false}},"pointer":{{"length":"60%","width":4,"itemStyle":{{"color":"{c}"}}}},"progress":{{"show":false}}}}"#,
                cx = center_x,
                tc = title_color,
                acol = axis_color_arr,
            );
            series_json.push(s);
        }
        let chart_h = if n <= 2 { 200 } else { 220 };
        html.push_str(&format!(
            r##"<div class="echart" style="height:{}px;" data-chart-config='{{"series":[{}]}}'></div>"##,
            chart_h,
            series_json.join(","),
        ));
    }

    html.push_str(r#"<div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str(row, "emp_group");
        let local = get_f64(row, "local_avg_salary");
        let national = get_f64(row, "national_avg_salary");
        let ci = get_f64(row, "competitiveness_index");
        let pctile = get_f64(row, "percentile_rank");
        let sample = get_i64(row, "sample_count");

        let ci_color = if ci >= 0.0 { "#22c55e" } else { "#ef4444" };
        let ci_sign = if ci >= 0.0 { "+" } else { "" };
        let pctile_color = if pctile >= 75.0 {
            "#22c55e"
        } else if pctile >= 50.0 {
            "#3b82f6"
        } else if pctile >= 25.0 {
            "#eab308"
        } else {
            "#ef4444"
        };

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
        let composite_color = if composite >= 0.7 {
            "#22c55e"
        } else if composite >= 0.5 {
            "#3b82f6"
        } else if composite >= 0.3 {
            "#eab308"
        } else {
            "#ef4444"
        };

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
            let cat_raw = get_str(row, "keyword_category");
            let density = get_f64(row, "density");
            let avg_cnt = get_f64(row, "avg_count_per_posting");
            let label = escape_html(keyword_category_label(cat_raw));
            let color = keyword_category_color(cat_raw);
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
            let stype_raw = get_str(row, "strategy_type");
            let stype = escape_html(stype_raw);
            let count = get_i64(row, "count");
            let pct_val = get_f64(row, "pct");
            let (bg, fg) = strategy_color(stype_raw);

            html.push_str(&format!(
                r#"<div class="rounded-lg p-3 text-center" style="background:{bg}">
                    <div class="text-xs font-semibold mb-1" style="color:{fg}">{stype}</div>
                    <div class="text-lg font-bold" style="color:{fg}">{pct_s}</div>
                    <div class="text-xs" style="color:{fg};opacity:0.7">{count_s}件</div>
                </div>"#,
                pct_s = format!("{:.1}%", pct_val),
                count_s = format_number(count),
            ));
        }

        html.push_str("</div></div>");
    }

    html.push_str("</div></div>");
    html
}

fn render_monopsony_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">⚖️ 雇用者集中度（独占力）</h3>
        <p class="text-xs text-slate-500 mb-4">雇用市場の集中度を評価。数値が高いほど少数企業に求人が偏り、求職者の選択肢が限られます。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    for row in data {
        let grp = get_str_html(row, "emp_group");
        let total = get_i64(row, "total_postings");
        let facilities = get_i64(row, "unique_facilities");
        let hhi = get_f64(row, "hhi");
        let level = get_str(row, "concentration_level");
        let top1_share = get_f64(row, "top1_share");
        let top3_share = get_f64(row, "top3_share");
        let top5_share = get_f64(row, "top5_share");
        let gini = get_f64(row, "gini");

        let (badge_bg, badge_fg) = concentration_badge(level);

        // HHI ゲージ（0-10000スケール、>2500で高度集中）
        let hhi_w = (hhi / 10000.0 * 100.0).min(100.0).max(0.0);
        let hhi_color = if hhi >= 2500.0 {
            "#ef4444"
        } else if hhi >= 1500.0 {
            "#f97316"
        } else if hhi >= 1000.0 {
            "#eab308"
        } else {
            "#22c55e"
        };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="flex items-center justify-between mb-3">
                    <div class="text-sm font-semibold text-white">{grp}</div>
                    <span class="px-2 py-0.5 rounded text-xs font-semibold" style="background:{badge_bg};color:{badge_fg}">{level_display}</span>
                </div>
                <div class="flex items-baseline gap-2 mb-1">
                    <span class="text-2xl font-bold" style="color:{hhi_color}">{hhi:.0}</span>
                    <span class="text-xs text-slate-400">集中度</span>
                </div>
                <div class="w-full bg-slate-700 rounded h-2 mb-3"><div class="rounded h-2" style="width:{hhi_w:.1}%;background:{hhi_color}"></div></div>
                <div class="space-y-2 text-xs">
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
                    <div class="flex justify-between text-slate-400 pt-2 border-t border-slate-700"><span>格差指数</span><span class="text-white">{gini:.3}</span></div>
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

        let iso_color = if isolation >= 0.7 {
            "#ef4444"
        } else if isolation >= 0.4 {
            "#f97316"
        } else if isolation >= 0.2 {
            "#eab308"
        } else {
            "#22c55e"
        };
        let iso_label = if isolation >= 0.7 {
            "高孤立（求人砂漠）"
        } else if isolation >= 0.4 {
            "やや孤立"
        } else if isolation >= 0.2 {
            "標準"
        } else {
            "アクセス良好"
        };

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
        html.push_str(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
            <h4 class="text-xs font-semibold text-emerald-400 mb-3">上位10都道府県</h4>"#,
        );
        for row in data.iter().take(10) {
            let prefecture = get_str(row, "prefecture");
            let wage = get_f64(row, "hourly_min_wage");
            let ratio = if national_avg > 0.0 {
                wage / national_avg
            } else {
                1.0
            };
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
        html.push_str(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
            <h4 class="text-xs font-semibold text-rose-400 mb-3">下位10都道府県</h4>"#,
        );
        let bottom: Vec<&Row> = data.iter().rev().take(10).collect();
        for row in &bottom {
            let prefecture = get_str(row, "prefecture");
            let wage = get_f64(row, "hourly_min_wage");
            let ratio = if national_avg > 0.0 {
                wage / national_avg
            } else {
                1.0
            };
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

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 厚生労働省「地域別最低賃金の全国一覧」(2025年度)</p>"#);
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

        let rate_color = if below_rate > 0.05 {
            "#ef4444"
        } else if below_rate > 0.01 {
            "#f97316"
        } else if below_rate > 0.0 {
            "#eab308"
        } else {
            "#22c55e"
        };
        let rate_label = if below_rate > 0.05 {
            "要改善"
        } else if below_rate > 0.01 {
            "注意"
        } else if below_rate > 0.0 {
            "微量"
        } else {
            "適正"
        };

        // 平均時給 vs 最低賃金の比較バー
        let wage_ratio = if min_wage > 0.0 {
            (avg_wage / min_wage * 100.0).min(200.0)
        } else {
            100.0
        };
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

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 厚生労働省「地域別最低賃金」+ ハローワーク求人データ</p>"#);
    html.push_str("</div></div>");
    html
}

/// 有効求人倍率の年度次推移チャート（SVGインライン描画）
/// 全国（グレー破線）+ 選択都道府県（青実線）を比較表示
fn render_job_openings_ratio_section(data: &[Row], pref: &str) -> String {
    // データを全国と都道府県に分離
    let mut national: Vec<(&str, f64)> = Vec::new();
    let mut regional: Vec<(&str, f64)> = Vec::new();

    for row in data {
        let p = get_str(row, "prefecture");
        let fy = get_str(row, "fiscal_year");
        let ratio = get_f64(row, "ratio_total");
        if fy.is_empty() || ratio <= 0.0 {
            continue;
        }
        if p == "全国" {
            national.push((fy, ratio));
        } else {
            regional.push((fy, ratio));
        }
    }

    if national.is_empty() && regional.is_empty() {
        return String::new();
    }

    // Y軸の範囲を自動計算
    let all_ratios: Vec<f64> = national
        .iter()
        .chain(regional.iter())
        .map(|(_, r)| *r)
        .collect();
    let y_min_raw = all_ratios.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max_raw = all_ratios.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // 余白を持たせる（0.1刻みで切り捨て/切り上げ）
    let y_min = ((y_min_raw - 0.1) * 10.0).floor() / 10.0;
    let y_max = ((y_max_raw + 0.1) * 10.0).ceil() / 10.0;
    let y_min = if y_min < 0.0 { 0.0 } else { y_min };
    let y_range = y_max - y_min;
    if y_range <= 0.0 {
        return String::new();
    }

    // SVG描画エリアの定義
    let svg_w: f64 = 800.0;
    let svg_h: f64 = 300.0;
    let margin_left: f64 = 60.0;
    let margin_right: f64 = 70.0;
    let margin_top: f64 = 20.0;
    let margin_bottom: f64 = 50.0;
    let plot_w = svg_w - margin_left - margin_right;
    let plot_h = svg_h - margin_top - margin_bottom;

    // 座標変換ヘルパー
    let x_pos = |i: usize, total: usize| -> f64 {
        if total <= 1 {
            return margin_left + plot_w / 2.0;
        }
        margin_left + (i as f64) / ((total - 1) as f64) * plot_w
    };
    let y_pos = |val: f64| -> f64 { margin_top + plot_h - ((val - y_min) / y_range) * plot_h };

    let mut svg = String::with_capacity(8_000);
    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {svg_w} {svg_h}" preserveAspectRatio="xMidYMid meet" style="width:100%;height:auto;max-height:320px;">"#
    ));

    // 背景
    svg.push_str(&format!(
        r#"<rect x="0" y="0" width="{svg_w}" height="{svg_h}" fill="transparent"/>"#
    ));

    // グリッド線（水平方向）
    let grid_steps = 5;
    for i in 0..=grid_steps {
        let val = y_min + y_range * (i as f64) / (grid_steps as f64);
        let y = y_pos(val);
        svg.push_str(&format!(
            r##"<line x1="{margin_left}" y1="{y:.1}" x2="{}" y2="{y:.1}" stroke="#334155" stroke-width="0.5"/>"##,
            margin_left + plot_w
        ));
        // Y軸ラベル
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="end" fill="#94a3b8" font-size="11">{:.2}</text>"##,
            margin_left - 8.0,
            y + 4.0,
            val
        ));
    }

    // X軸ラベル（年度ごと）— 全国データ基準で表示
    let x_labels = if !national.is_empty() {
        &national
    } else {
        &regional
    };
    let total_points = x_labels.len();
    for (i, (fy, _)) in x_labels.iter().enumerate() {
        let x = x_pos(i, total_points);
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{}" text-anchor="middle" fill="#94a3b8" font-size="10">{fy}</text>"##,
            svg_h - 10.0
        ));
    }

    // 全国ライン（グレー破線）
    if national.len() >= 2 {
        let mut path = String::new();
        for (i, (_, ratio)) in national.iter().enumerate() {
            let x = x_pos(i, national.len());
            let y = y_pos(*ratio);
            if i == 0 {
                path.push_str(&format!("M{x:.1},{y:.1}"));
            } else {
                path.push_str(&format!(" L{x:.1},{y:.1}"));
            }
        }
        svg.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="#9ca3af" stroke-width="1.5" stroke-dasharray="6,3"/>"##
        ));
        // データポイント（丸）
        for (i, (ym, ratio)) in national.iter().enumerate() {
            let x = x_pos(i, national.len());
            let y = y_pos(*ratio);
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="2.5" fill="#9ca3af"><title>全国 {ym}: {ratio:.2}</title></circle>"##
            ));
        }
        // 最新値ラベル
        if let Some((_ym, ratio)) = national.last() {
            let x = x_pos(national.len() - 1, national.len());
            let y = y_pos(*ratio);
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="#9ca3af" font-size="11" font-weight="bold">{ratio:.2}</text>"##,
                x + 6.0, y + 4.0
            ));
        }
    }

    // 都道府県ライン（青実線）
    if regional.len() >= 2 {
        let mut path = String::new();
        for (i, (_, ratio)) in regional.iter().enumerate() {
            let x = x_pos(i, regional.len());
            let y = y_pos(*ratio);
            if i == 0 {
                path.push_str(&format!("M{x:.1},{y:.1}"));
            } else {
                path.push_str(&format!(" L{x:.1},{y:.1}"));
            }
        }
        svg.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="#3b82f6" stroke-width="2"/>"##
        ));
        // データポイント（丸）
        for (i, (ym, ratio)) in regional.iter().enumerate() {
            let x = x_pos(i, regional.len());
            let y = y_pos(*ratio);
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="3" fill="#3b82f6"><title>{} {ym}: {ratio:.2}</title></circle>"##,
                escape_html(pref)
            ));
        }
        // 最新値ラベル
        if let Some((_ym, ratio)) = regional.last() {
            let x = x_pos(regional.len() - 1, regional.len());
            let y = y_pos(*ratio);
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="#3b82f6" font-size="11" font-weight="bold">{ratio:.2}</text>"##,
                x + 6.0, y - 6.0
            ));
        }
    }

    svg.push_str("</svg>");

    // 凡例
    let pref_label = if pref.is_empty() {
        "（都道府県を選択してください）"
    } else {
        pref
    };
    let legend = format!(
        r##"<div class="flex justify-center gap-6 mt-2 text-xs">
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#9ca3af" stroke-width="1.5" stroke-dasharray="4,2"/></svg>
                <span class="text-slate-400">全国</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#3b82f6" stroke-width="2"/></svg>
                <span class="text-slate-400">{}</span>
            </span>
        </div>"##,
        escape_html(pref_label)
    );

    // stat-card で包む
    let scope_label = if pref.is_empty() { "全国" } else { pref };
    let mut html = String::with_capacity(svg.len() + 500);
    html.push_str(&format!(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📈 有効求人倍率推移 <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">有効求人倍率の年度次推移（全国比較）。外部統計データ。</p>"#, escape_html(scope_label)));
    html.push_str(&svg);
    html.push_str(&legend);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 社会・人口統計体系（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 賃金・労働時間の推移（男女別月給の折れ線グラフ）
fn render_labor_stats_section(data: &[Row], pref: &str) -> String {
    // データを全国と都道府県に分離し、月給（千円）を抽出
    let mut nat_male: Vec<(String, f64)> = Vec::new();
    let mut nat_female: Vec<(String, f64)> = Vec::new();
    let mut reg_male: Vec<(String, f64)> = Vec::new();
    let mut reg_female: Vec<(String, f64)> = Vec::new();

    for row in data {
        let p = get_str(row, "prefecture");
        let fy = get_str(row, "fiscal_year").to_string();
        let sm = get_f64(row, "monthly_salary_male");
        let sf = get_f64(row, "monthly_salary_female");
        if fy.is_empty() {
            continue;
        }
        if p == "全国" {
            if sm > 0.0 {
                nat_male.push((fy.clone(), sm));
            }
            if sf > 0.0 {
                nat_female.push((fy.clone(), sf));
            }
        } else {
            if sm > 0.0 {
                reg_male.push((fy.clone(), sm));
            }
            if sf > 0.0 {
                reg_female.push((fy.clone(), sf));
            }
        }
    }

    // データがなければ非表示
    if nat_male.is_empty() && reg_male.is_empty() && nat_female.is_empty() && reg_female.is_empty()
    {
        return String::new();
    }

    // 全系列の値を収集してY軸範囲を計算
    let all_vals: Vec<f64> = nat_male
        .iter()
        .chain(nat_female.iter())
        .chain(reg_male.iter())
        .chain(reg_female.iter())
        .map(|(_, v)| *v)
        .collect();
    if all_vals.is_empty() {
        return String::new();
    }

    let y_min_raw = all_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max_raw = all_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    // 余白を持たせる（10千円刻み）
    let y_min = ((y_min_raw - 10.0) / 10.0).floor() * 10.0;
    let y_min = if y_min < 0.0 { 0.0 } else { y_min };
    let y_max = ((y_max_raw + 10.0) / 10.0).ceil() * 10.0;
    let y_range = y_max - y_min;
    if y_range <= 0.0 {
        return String::new();
    }

    // SVG描画エリア
    let svg_w: f64 = 800.0;
    let svg_h: f64 = 300.0;
    let ml: f64 = 60.0; // margin_left
    let mr: f64 = 70.0; // margin_right
    let mt: f64 = 20.0; // margin_top
    let mb: f64 = 50.0; // margin_bottom
    let pw = svg_w - ml - mr;
    let ph = svg_h - mt - mb;

    let x_pos = |i: usize, total: usize| -> f64 {
        if total <= 1 {
            return ml + pw / 2.0;
        }
        ml + (i as f64) / ((total - 1) as f64) * pw
    };
    let y_pos = |val: f64| -> f64 { mt + ph - ((val - y_min) / y_range) * ph };

    let mut svg = String::with_capacity(10_000);
    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {svg_w} {svg_h}" preserveAspectRatio="xMidYMid meet" style="width:100%;height:auto;max-height:320px;">"#
    ));
    svg.push_str(&format!(
        r#"<rect x="0" y="0" width="{svg_w}" height="{svg_h}" fill="transparent"/>"#
    ));

    // グリッド線
    let grid_steps = 5;
    for i in 0..=grid_steps {
        let val = y_min + y_range * (i as f64) / (grid_steps as f64);
        let y = y_pos(val);
        svg.push_str(&format!(
            r##"<line x1="{ml}" y1="{y:.1}" x2="{}" y2="{y:.1}" stroke="#334155" stroke-width="0.5"/>"##,
            ml + pw
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="end" fill="#94a3b8" font-size="11">{:.0}</text>"##,
            ml - 8.0,
            y + 4.0,
            val
        ));
    }

    // X軸ラベル — 全国男性基準（最も多いデータ）
    let x_labels = if !nat_male.is_empty() {
        &nat_male
    } else if !reg_male.is_empty() {
        &reg_male
    } else if !nat_female.is_empty() {
        &nat_female
    } else {
        &reg_female
    };
    let total_pts = x_labels.len();
    for (i, (fy, _)) in x_labels.iter().enumerate() {
        let x = x_pos(i, total_pts);
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{}" text-anchor="middle" fill="#94a3b8" font-size="10">{fy}</text>"##,
            svg_h - 10.0
        ));
    }

    // 折れ線描画ヘルパー
    let draw_line = |svg: &mut String,
                     series: &[(String, f64)],
                     color: &str,
                     dash: bool,
                     w: &str| {
        if series.len() < 2 {
            return;
        }
        let mut path = String::new();
        for (i, (_, v)) in series.iter().enumerate() {
            let x = x_pos(i, series.len());
            let y = y_pos(*v);
            if i == 0 {
                path.push_str(&format!("M{x:.1},{y:.1}"));
            } else {
                path.push_str(&format!(" L{x:.1},{y:.1}"));
            }
        }
        let dash_attr = if dash {
            r#" stroke-dasharray="6,3""#
        } else {
            ""
        };
        svg.push_str(&format!(
            r##"<path d="{path}" fill="none" stroke="{color}" stroke-width="{w}"{dash_attr}/>"##
        ));
        // データポイント
        for (i, (fy, v)) in series.iter().enumerate() {
            let x = x_pos(i, series.len());
            let y = y_pos(*v);
            let r = if dash { "2.5" } else { "3" };
            svg.push_str(&format!(
                r##"<circle cx="{x:.1}" cy="{y:.1}" r="{r}" fill="{color}"><title>{fy}: {v:.0}千円</title></circle>"##
            ));
        }
        // 最新値ラベル
        if let Some((_, v)) = series.last() {
            let x = x_pos(series.len() - 1, series.len());
            let y = y_pos(*v);
            svg.push_str(&format!(
                r##"<text x="{}" y="{}" fill="{color}" font-size="11" font-weight="bold">{v:.0}</text>"##,
                x + 6.0, y + 4.0
            ));
        }
    };

    // 全国男性（青破線）
    draw_line(&mut svg, &nat_male, "#60a5fa", true, "1.5");
    // 全国女性（ピンク破線）
    draw_line(&mut svg, &nat_female, "#f9a8d4", true, "1.5");
    // 都道府県男性（青実線）
    draw_line(&mut svg, &reg_male, "#3b82f6", false, "2");
    // 都道府県女性（ピンク実線）
    draw_line(&mut svg, &reg_female, "#ec4899", false, "2");

    svg.push_str("</svg>");

    // 凡例
    let pref_label = if pref.is_empty() {
        "（都道府県を選択してください）"
    } else {
        pref
    };
    let legend = format!(
        r##"<div class="flex flex-wrap justify-center gap-4 mt-2 text-xs">
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#3b82f6" stroke-width="2"/></svg>
                <span class="text-slate-400">{p} 男性</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#ec4899" stroke-width="2"/></svg>
                <span class="text-slate-400">{p} 女性</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#60a5fa" stroke-width="1.5" stroke-dasharray="4,2"/></svg>
                <span class="text-slate-400">全国 男性</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#f9a8d4" stroke-width="1.5" stroke-dasharray="4,2"/></svg>
                <span class="text-slate-400">全国 女性</span>
            </span>
        </div>"##,
        p = escape_html(pref_label)
    );

    // stat-cardで包む
    let mut html = String::with_capacity(svg.len() + 600);
    let scope_label = if pref.is_empty() { "全国" } else { pref };
    html.push_str(&format!(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📊 賃金・労働時間の推移 <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">きまって支給する現金給与月額（男女別）の推移。外部統計データ。</p>"#, escape_html(scope_label)));
    html.push_str(&svg);
    html.push_str(&legend);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 社会・人口統計体系（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

fn render_prefecture_stats_section(data: &[Row], pref: &str) -> String {
    let mut html = String::with_capacity(6_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📊 都道府県別外部指標</h3>
        <p class="text-xs text-slate-500 mb-4">労働力調査・就業構造基本調査・賃金構造基本統計調査等の公的統計データ。</p>"#);

    if !pref.is_empty() && data.len() == 1 {
        let row = &data[0];
        let unemp = get_f64(row, "unemployment_rate");
        let desire = get_f64(row, "job_change_desire_rate");
        let non_reg = get_f64(row, "non_regular_rate");
        let wage = get_f64(row, "avg_monthly_wage");
        let price = get_f64(row, "price_index");
        let fulfill = get_f64(row, "fulfillment_rate");
        let real_wage = get_f64(row, "real_wage_index");

        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);

        let cards: [(&str, String, &str, &str); 7] = [
            (
                "完全失業率",
                format!("{unemp:.1}%"),
                "顕在求職者率",
                "#ef4444",
            ),
            (
                "転職希望者比率",
                format!("{desire:.1}%"),
                "潜在労働力流動性",
                "#f59e0b",
            ),
            (
                "非正規雇用比率",
                format!("{non_reg:.1}%"),
                "就業構造",
                "#8b5cf6",
            ),
            (
                "平均所定内給与",
                format!("{wage:.0}千円"),
                "賃金構造基本統計",
                "#22c55e",
            ),
            (
                "物価地域差指数",
                format!("{price:.1}"),
                "全国=100",
                "#06b6d4",
            ),
            (
                "有効求人充足率",
                format!("{fulfill:.1}%"),
                "充足数/新規求人",
                "#3b82f6",
            ),
            (
                "実質賃金力",
                format!("{real_wage:.1}千円"),
                "物価調整後",
                "#ec4899",
            ),
        ];

        for (label, value, sub, color) in &cards {
            html.push_str(&format!(
                r#"<div class="bg-navy-700/50 rounded-lg p-3 text-center">
                    <div class="text-xs text-slate-500 mb-1">{label}</div>
                    <div class="text-xl font-bold" style="color:{color}">{value}</div>
                    <div class="text-xs text-slate-600 mt-1">{sub}</div>
                </div>"#
            ));
        }
        html.push_str("</div>");
    } else if data.len() > 1 {
        // 全国一覧: テーブル形式
        html.push_str(r#"<div style="overflow-x:auto;max-height:400px;overflow-y:auto;"><table class="data-table text-xs">
            <thead><tr>
                <th>都道府県</th><th class="text-right">失業率</th><th class="text-right">転職希望</th>
                <th class="text-right">非正規</th><th class="text-right">平均賃金</th>
                <th class="text-right">物価</th><th class="text-right">充足率</th><th class="text-right">実質賃金</th>
            </tr></thead><tbody>"#);
        for row in data {
            let p = get_str(row, "prefecture");
            html.push_str(&format!(
                r#"<tr><td class="text-slate-300">{p}</td>
                <td class="text-right">{:.1}%</td><td class="text-right">{:.1}%</td>
                <td class="text-right">{:.1}%</td><td class="text-right">{:.0}</td>
                <td class="text-right">{:.1}</td><td class="text-right">{:.1}%</td>
                <td class="text-right">{:.1}</td></tr>"#,
                get_f64(row, "unemployment_rate"),
                get_f64(row, "job_change_desire_rate"),
                get_f64(row, "non_regular_rate"),
                get_f64(row, "avg_monthly_wage"),
                get_f64(row, "price_index"),
                get_f64(row, "fulfillment_rate"),
                get_f64(row, "real_wage_index"),
            ));
        }
        html.push_str("</tbody></table></div>");
    }

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 労働力調査(総務省統計局,2024)、就業構造基本調査(総務省,2022)、賃金構造基本統計調査(厚労省,2024)、小売物価統計調査(総務省,2024)、職業安定業務統計(厚労省)</p>"#);
    html.push_str("</div>");
    html
}

fn render_population_section(pop_data: &[Row], pyramid: &[Row]) -> String {
    let mut html = String::with_capacity(8_000);
    html.push_str(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">👥 人口構成</h3>
        <p class="text-xs text-slate-500 mb-4">住民基本台帳に基づく人口・年齢構成データ。</p>"#,
    );

    if let Some(row) = pop_data.first() {
        let total = get_i64(row, "total_population");
        let male = get_i64(row, "male_population");
        let female = get_i64(row, "female_population");
        let aging = get_f64(row, "aging_rate");
        let working = get_f64(row, "working_age_rate");
        let youth = get_f64(row, "youth_rate");

        // 年齢3区分バー
        let aging_color = if aging >= 35.0 {
            "#ef4444"
        } else if aging >= 28.0 {
            "#f97316"
        } else {
            "#22c55e"
        };

        html.push_str(&format!(
            r#"<div class="grid grid-cols-2 md:grid-cols-3 gap-3 mb-4">
                <div class="bg-navy-700/50 rounded-lg p-3 text-center">
                    <div class="text-xs text-slate-500 mb-1">総人口</div>
                    <div class="text-2xl font-bold text-white">{total_s}</div>
                    <div class="text-xs text-slate-500 mt-1">男{male_s} / 女{female_s}</div>
                </div>
                <div class="bg-navy-700/50 rounded-lg p-3 text-center">
                    <div class="text-xs text-slate-500 mb-1">高齢化率</div>
                    <div class="text-2xl font-bold" style="color:{aging_color}">{aging:.1}%</div>
                    <div class="text-xs text-slate-500 mt-1">65歳以上</div>
                </div>
                <div class="bg-navy-700/50 rounded-lg p-3 text-center">
                    <div class="text-xs text-slate-500 mb-1">生産年齢人口</div>
                    <div class="text-2xl font-bold text-blue-400">{working:.1}%</div>
                    <div class="text-xs text-slate-500 mt-1">15-64歳</div>
                </div>
            </div>"#,
            total_s = format_number(total),
            male_s = format_number(male),
            female_s = format_number(female),
        ));

        // 年齢3区分の積み上げバー
        html.push_str(&format!(
            r#"<div class="mb-4">
                <div class="text-xs text-slate-500 mb-1">年齢3区分構成</div>
                <div class="w-full flex rounded h-5 overflow-hidden">
                    <div class="h-5 bg-cyan-500" style="width:{youth:.1}%" title="年少({youth:.1}%)"></div>
                    <div class="h-5 bg-blue-500" style="width:{working:.1}%" title="生産年齢({working:.1}%)"></div>
                    <div class="h-5 bg-orange-500" style="width:{aging:.1}%" title="高齢({aging:.1}%)"></div>
                </div>
                <div class="flex justify-between text-xs mt-1">
                    <span class="text-cyan-400">年少 {youth:.1}%</span>
                    <span class="text-blue-400">生産年齢 {working:.1}%</span>
                    <span class="text-orange-400">高齢 {aging:.1}%</span>
                </div>
            </div>"#
        ));
    }

    // 人口ピラミッドチャート（横棒グラフ左右対称）
    if !pyramid.is_empty() {
        let max_count: i64 = pyramid
            .iter()
            .map(|r| get_i64(r, "male_count").max(get_i64(r, "female_count")))
            .max()
            .unwrap_or(1);

        html.push_str(r#"<div class="mt-4">
            <div class="text-xs text-slate-400 font-semibold mb-2">人口ピラミッド</div>
            <div class="flex text-xs text-slate-500 mb-1"><span class="w-1/2 text-right pr-1 text-blue-400">男性</span><span class="w-1/2 pl-1 text-pink-400">女性</span></div>"#);

        // 下から上に表示（若い方が下）
        for row in pyramid.iter().rev() {
            let ag = get_str(row, "age_group");
            let m = get_i64(row, "male_count");
            let f = get_i64(row, "female_count");
            let m_pct = if max_count > 0 {
                m as f64 / max_count as f64 * 100.0
            } else {
                0.0
            };
            let f_pct = if max_count > 0 {
                f as f64 / max_count as f64 * 100.0
            } else {
                0.0
            };

            html.push_str(&format!(
                r#"<div class="flex items-center" style="height:1.1rem;margin-bottom:1px">
                    <div class="text-right text-slate-500 pr-1 shrink-0" style="width:2.2rem;font-size:0.65rem">{ag}</div>
                    <div class="flex-1 flex justify-end"><div class="rounded-l" style="height:0.6rem;width:{m_pct:.1}%;background:rgba(59,130,246,0.7)"></div></div>
                    <div style="width:2px;height:0.6rem;background:#475569;margin:0 2px"></div>
                    <div class="flex-1"><div class="rounded-r" style="height:0.6rem;width:{f_pct:.1}%;background:rgba(236,72,153,0.7)"></div></div>
                </div>"#
            ));
        }
        html.push_str("</div>");
    }

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 国勢調査(総務省,2020) + SSDSE-A按分推計</p>"#);
    html.push_str("</div>");
    html
}

fn render_demographics_section(migration: &[Row], daytime: &[Row]) -> String {
    let mut html = String::with_capacity(4_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🔄 人口動態</h3>
        <p class="text-xs text-slate-500 mb-4">社会増減（転入転出）と昼夜間人口比から人の流れを把握します。</p>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#);

    // 社会動態
    if let Some(row) = migration.first() {
        let inflow = get_i64(row, "inflow");
        let outflow = get_i64(row, "outflow");
        let net = get_i64(row, "net_migration");
        let rate = get_f64(row, "net_migration_rate");
        let net_color = if net >= 0 { "#22c55e" } else { "#ef4444" };
        let net_sign = if net >= 0 { "+" } else { "" };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-xs text-slate-500 mb-2">社会増減</div>
                <div class="text-2xl font-bold mb-1" style="color:{net_color}">{net_sign}{net_s}</div>
                <div class="text-xs" style="color:{net_color}">{net_sign}{rate:.1}‰</div>
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>転入</span><span class="text-emerald-400">{in_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>転出</span><span class="text-red-400">{out_s}</span></div>
                </div>
            </div>"#,
            net_s = format_number(net.abs()),
            in_s = format_number(inflow),
            out_s = format_number(outflow),
        ));
    }

    // 昼夜間人口
    if let Some(row) = daytime.first() {
        let night = get_i64(row, "nighttime_pop");
        let day = get_i64(row, "daytime_pop");
        let ratio = get_f64(row, "day_night_ratio");
        let ratio_color = if ratio >= 100.0 { "#22c55e" } else { "#f59e0b" };
        let label = if ratio >= 100.0 {
            "昼間流入超過"
        } else {
            "昼間流出超過"
        };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-xs text-slate-500 mb-2">昼夜間人口比率</div>
                <div class="text-2xl font-bold mb-1" style="color:{ratio_color}">{ratio:.1}%</div>
                <div class="text-xs" style="color:{ratio_color}">{label}</div>
                <div class="mt-3 space-y-1 text-xs">
                    <div class="flex justify-between text-slate-400"><span>夜間人口</span><span class="text-white">{night_s}</span></div>
                    <div class="flex justify-between text-slate-400"><span>昼間人口</span><span class="text-white">{day_s}</span></div>
                </div>
            </div>"#,
            night_s = format_number(night),
            day_s = format_number(day),
        ));
    }

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: SSDSE-A(統計センター) - 住民基本台帳(2023)/国勢調査(2020)</p>"#);
    html.push_str("</div></div>");
    html
}

/// 産業別事業所数（横棒グラフ、上位15産業）
fn render_establishment_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }

    // 上位15産業に絞る（fetch側でDESCソート済み）
    let top: Vec<&Row> = data.iter().take(15).collect();
    if top.is_empty() {
        return String::new();
    }

    // 最大値を取得（バー幅の基準）
    let max_count = top
        .iter()
        .map(|r| get_i64(r, "establishment_count"))
        .max()
        .unwrap_or(1)
        .max(1);

    // 基準年を取得
    let ref_year = top
        .first()
        .map(|r| get_str(r, "reference_year").to_string())
        .unwrap_or_default();

    let pref_label = if pref.is_empty() { "全国" } else { pref };

    let mut html = String::with_capacity(6_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🏢 産業別事業所数 <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">産業大分類別の事業所数（経済センサス{}）。外部統計データ。</p>
        <div class="space-y-2">"#,
        escape_html(pref_label),
        if ref_year.is_empty() { "2021年".to_string() } else { format!("{}年", escape_html(&ref_year)) }
    ));

    for row in &top {
        let industry = get_str(row, "industry");
        let count = get_i64(row, "establishment_count");
        let pct_width = (count as f64 / max_count as f64 * 100.0).min(100.0);
        let count_str = format_number(count);
        let ind_escaped = escape_html(industry);

        html.push_str(&format!(
            r#"<div class="flex items-center gap-2">
                <div class="w-32 text-xs text-slate-400 truncate text-right flex-shrink-0" title="{ind_escaped}">{ind_short}</div>
                <div class="flex-1 rounded-full h-5 relative overflow-hidden" style="background:rgba(15,23,42,0.6)">
                    <div class="h-full rounded-full" style="width:{pct_width:.1}%;background:linear-gradient(to right,#2563eb,#06b6d4)"></div>
                </div>
                <div class="w-20 text-xs text-slate-300 text-right flex-shrink-0">{count_str}</div>
            </div>"#,
            ind_short = truncate_str(industry, 12),
        ));
    }

    html.push_str(r#"</div>
        <p class="text-xs text-slate-600 mt-3 italic">出典: 総務省「経済センサス-活動調査」（2021年） e-Stat API ※外部統計データ</p>
    </div>"#);
    html
}

/// 入職率・離職率の推移（医療・福祉産業、SVG折れ線グラフ）
fn render_turnover_section(data: &[Row], pref: &str) -> String {
    // データを全国と都道府県に分離
    let mut nat_entry: Vec<(String, f64)> = Vec::new();
    let mut nat_sep: Vec<(String, f64)> = Vec::new();
    let mut reg_entry: Vec<(String, f64)> = Vec::new();
    let mut reg_sep: Vec<(String, f64)> = Vec::new();

    for row in data {
        let p = get_str(row, "prefecture");
        let fy = get_str(row, "fiscal_year").to_string();
        let entry = get_f64(row, "entry_rate");
        let sep = get_f64(row, "separation_rate");
        if fy.is_empty() {
            continue;
        }
        if p == "全国" {
            if entry > 0.0 {
                nat_entry.push((fy.clone(), entry));
            }
            if sep > 0.0 {
                nat_sep.push((fy, sep));
            }
        } else {
            if entry > 0.0 {
                reg_entry.push((fy.clone(), entry));
            }
            if sep > 0.0 {
                reg_sep.push((fy, sep));
            }
        }
    }

    // 入職率・離職率ともに空ならセクション非表示
    if nat_entry.is_empty() && nat_sep.is_empty() && reg_entry.is_empty() && reg_sep.is_empty() {
        return String::new();
    }

    // Y軸の範囲を自動計算（全系列の最大最小）
    let all_vals: Vec<f64> = nat_entry
        .iter()
        .chain(nat_sep.iter())
        .chain(reg_entry.iter())
        .chain(reg_sep.iter())
        .map(|(_, v)| *v)
        .collect();
    let y_min_raw = all_vals.iter().cloned().fold(f64::INFINITY, f64::min);
    let y_max_raw = all_vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
    let y_min = ((y_min_raw - 1.0) * 1.0).floor().max(0.0);
    let y_max = ((y_max_raw + 1.0) * 1.0).ceil();
    let y_range = y_max - y_min;
    if y_range <= 0.0 {
        return String::new();
    }

    // SVG描画パラメータ
    let svg_w: f64 = 800.0;
    let svg_h: f64 = 300.0;
    let ml: f64 = 60.0; // margin_left
    let mr: f64 = 70.0; // margin_right
    let mt: f64 = 20.0; // margin_top
    let mb: f64 = 50.0; // margin_bottom
    let pw = svg_w - ml - mr; // plot_width
    let ph = svg_h - mt - mb; // plot_height

    // X軸基準（全系列のfiscal_yearを統合して一意な昇順リストを作成）
    let x_labels: Vec<String> = {
        let mut all_fy: Vec<String> = nat_entry
            .iter()
            .chain(nat_sep.iter())
            .chain(reg_entry.iter())
            .chain(reg_sep.iter())
            .map(|(fy, _)| fy.clone())
            .collect();
        all_fy.sort();
        all_fy.dedup();
        all_fy
    };
    let total_pts = x_labels.len();
    if total_pts == 0 {
        return String::new();
    }

    // 座標変換クロージャ
    let x_pos = |fy: &str| -> Option<f64> {
        x_labels.iter().position(|f| f == fy).map(|i| {
            if total_pts <= 1 {
                ml + pw / 2.0
            } else {
                ml + (i as f64) / ((total_pts - 1) as f64) * pw
            }
        })
    };
    let y_pos = |val: f64| -> f64 { mt + ph - ((val - y_min) / y_range) * ph };

    let mut svg = String::with_capacity(10_000);
    svg.push_str(&format!(
        r#"<svg viewBox="0 0 {svg_w} {svg_h}" preserveAspectRatio="xMidYMid meet" style="width:100%;height:auto;max-height:320px;">"#
    ));
    svg.push_str(&format!(
        r#"<rect x="0" y="0" width="{svg_w}" height="{svg_h}" fill="transparent"/>"#
    ));

    // グリッド線（水平方向）
    let grid_steps = 5;
    for i in 0..=grid_steps {
        let val = y_min + y_range * (i as f64) / (grid_steps as f64);
        let y = y_pos(val);
        svg.push_str(&format!(
            r##"<line x1="{ml}" y1="{y:.1}" x2="{}" y2="{y:.1}" stroke="#334155" stroke-width="0.5"/>"##,
            ml + pw
        ));
        svg.push_str(&format!(
            r##"<text x="{}" y="{}" text-anchor="end" fill="#94a3b8" font-size="11">{:.1}%</text>"##,
            ml - 8.0, y + 4.0, val
        ));
    }

    // X軸ラベル
    for (i, fy) in x_labels.iter().enumerate() {
        let x = if total_pts <= 1 {
            ml + pw / 2.0
        } else {
            ml + (i as f64) / ((total_pts - 1) as f64) * pw
        };
        svg.push_str(&format!(
            r##"<text x="{x:.1}" y="{}" text-anchor="middle" fill="#94a3b8" font-size="10">{fy}</text>"##,
            svg_h - 10.0
        ));
    }

    // 折れ線描画（系列ごと）
    // (データ, 線色, 破線パターン, ポイント半径, ラベル名)
    let reg_entry_label = format!("{}入職率", if pref.is_empty() { "" } else { pref });
    let reg_sep_label = format!("{}離職率", if pref.is_empty() { "" } else { pref });
    let series: Vec<(&Vec<(String, f64)>, &str, &str, f64, &str)> = vec![
        (&nat_entry, "#22c55e", "6,3", 2.5, "全国入職率"),
        (&nat_sep, "#ef4444", "6,3", 2.5, "全国離職率"),
        (&reg_entry, "#22c55e", "", 3.0, &reg_entry_label),
        (&reg_sep, "#ef4444", "", 3.0, &reg_sep_label),
    ];

    for (pts, color, dash, radius, label) in &series {
        if pts.len() < 2 {
            continue;
        }
        let mut path = String::new();
        for (fy, val) in pts.iter() {
            if let Some(x) = x_pos(fy) {
                let y = y_pos(*val);
                if path.is_empty() {
                    path.push_str(&format!("M{x:.1},{y:.1}"));
                } else {
                    path.push_str(&format!(" L{x:.1},{y:.1}"));
                }
            }
        }
        if !path.is_empty() {
            let dash_attr = if dash.is_empty() {
                String::new()
            } else {
                format!(r#" stroke-dasharray="{dash}""#)
            };
            let width = if dash.is_empty() { "2" } else { "1.5" };
            svg.push_str(&format!(
                r##"<path d="{path}" fill="none" stroke="{color}" stroke-width="{width}"{dash_attr}/>"##
            ));
        }
        // データポイント
        for (fy, val) in pts.iter() {
            if let Some(x) = x_pos(fy) {
                let y = y_pos(*val);
                svg.push_str(&format!(
                    r##"<circle cx="{x:.1}" cy="{y:.1}" r="{radius}" fill="{color}"><title>{label} {fy}: {val:.1}%</title></circle>"##
                ));
            }
        }
        // 最新値ラベル
        if let Some((fy, val)) = pts.last() {
            if let Some(x) = x_pos(fy) {
                let y = y_pos(*val);
                svg.push_str(&format!(
                    r##"<text x="{}" y="{}" fill="{color}" font-size="11" font-weight="bold">{val:.1}%</text>"##,
                    x + 6.0, y + 4.0
                ));
            }
        }
    }

    svg.push_str("</svg>");

    // 凡例
    let pref_label = if pref.is_empty() {
        "（都道府県を選択）"
    } else {
        pref
    };
    let legend = format!(
        r##"<div class="flex flex-wrap justify-center gap-4 mt-2 text-xs">
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#22c55e" stroke-width="2"/></svg>
                <span class="text-slate-400">{p} 入職率</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#ef4444" stroke-width="2"/></svg>
                <span class="text-slate-400">{p} 離職率</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#22c55e" stroke-width="1.5" stroke-dasharray="4,2"/></svg>
                <span class="text-slate-400">全国 入職率</span>
            </span>
            <span class="flex items-center gap-1">
                <svg width="20" height="10"><line x1="0" y1="5" x2="20" y2="5" stroke="#ef4444" stroke-width="1.5" stroke-dasharray="4,2"/></svg>
                <span class="text-slate-400">全国 離職率</span>
            </span>
        </div>"##,
        p = escape_html(pref_label)
    );

    // stat-card で包む
    let scope_label = if pref.is_empty() { "全国" } else { pref };
    let mut html = String::with_capacity(svg.len() + 600);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📉 入職率・離職率の推移（医療・福祉） <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">医療・福祉産業における入職率と離職率の年度次推移。外部統計データ。</p>"#,
        escape_html(scope_label)
    ));
    html.push_str(&svg);
    html.push_str(&legend);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 雇用動向調査（厚生労働省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 消費支出プロファイル（横棒グラフ、CSSベース）
fn render_household_spending_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }

    // 「消費支出」（総計）を除外し、内訳カテゴリのみ上位10件に絞る
    let filtered: Vec<&Row> = data
        .iter()
        .filter(|r| {
            let cat = get_str(r, "category");
            cat != "消費支出" && !cat.is_empty()
        })
        .take(10)
        .collect();
    if filtered.is_empty() {
        return String::new();
    }

    // 最大値を取得（バー幅の基準）
    let max_amount = filtered
        .iter()
        .map(|r| get_f64(r, "monthly_amount"))
        .fold(0.0_f64, f64::max)
        .max(1.0);

    // 基準年を取得
    let ref_year = filtered
        .first()
        .map(|r| get_str(r, "reference_year").to_string())
        .unwrap_or_default();

    let pref_label = if pref.is_empty() { "全国" } else { pref };

    let mut html = String::with_capacity(6_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">💰 消費支出プロファイル <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">二人以上世帯の消費支出内訳。求人票の福利厚生設計の参考に。</p>
        <div class="space-y-2">"#,
        escape_html(pref_label)
    ));

    // 色パレット（インラインスタイル: Tailwindプリコンパイルに未定義のグラデーション対策）
    let colors = [
        "background:linear-gradient(to right,#f59e0b,#facc15)",
        "background:linear-gradient(to right,#10b981,#14b8a6)",
        "background:linear-gradient(to right,#3b82f6,#22d3ee)",
        "background:linear-gradient(to right,#a855f7,#8b5cf6)",
        "background:linear-gradient(to right,#f43f5e,#ec4899)",
        "background:linear-gradient(to right,#f97316,#f59e0b)",
        "background:linear-gradient(to right,#84cc16,#22c55e)",
        "background:linear-gradient(to right,#0ea5e9,#60a5fa)",
        "background:linear-gradient(to right,#d946ef,#c084fc)",
        "background:linear-gradient(to right,#6366f1,#60a5fa)",
    ];

    for (i, row) in filtered.iter().enumerate() {
        let cat = get_str(row, "category");
        let amount = get_f64(row, "monthly_amount");
        let pct_width = (amount / max_amount * 100.0).min(100.0);
        let amount_str = format_number(amount as i64);
        let cat_escaped = escape_html(cat);
        let color = colors.get(i).unwrap_or(&colors[0]);

        html.push_str(&format!(
            r#"<div class="flex items-center gap-2">
                <div class="w-28 text-xs text-slate-400 truncate text-right flex-shrink-0" title="{cat_escaped}">{cat_short}</div>
                <div class="flex-1 rounded-full h-5 relative overflow-hidden" style="background:rgba(15,23,42,0.6)">
                    <div class="h-full rounded-full" style="width:{pct_width:.1}%;{color}"></div>
                </div>
                <div class="w-24 text-xs text-slate-300 text-right flex-shrink-0">{amount_str}円</div>
            </div>"#,
            cat_short = truncate_str(cat, 10),
        ));
    }

    html.push_str(r#"</div>"#);
    html.push_str(&format!(
        r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 家計調査（総務省） e-Stat API{yr} ※外部統計データ</p>"#,
        yr = if ref_year.is_empty() { String::new() } else { format!("（{}年）", escape_html(&ref_year)) }
    ));
    html.push_str("</div>");
    html
}

/// 事業所動態セクション（開業率 vs 廃業率の横棒グラフ）
fn render_business_dynamics_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }

    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // バーの最大幅を決定するため、開業率・廃業率の最大値を取得
    let max_rate = data
        .iter()
        .flat_map(|r| vec![get_f64(r, "opening_rate"), get_f64(r, "closure_rate")])
        .fold(0.0_f64, f64::max)
        .max(0.1);

    let mut html = String::with_capacity(6_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🏗️ 事業所動態（開業率・廃業率） <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">事業所の開業率と廃業率の推移。地域の産業活力を示す指標。</p>
        <div class="space-y-3">"#,
        escape_html(pref_label)
    ));

    for row in data {
        let fy = get_str(row, "fiscal_year");
        let opening = get_f64(row, "opening_rate");
        let closure = get_f64(row, "closure_rate");
        let net = get_i64(row, "net_change");
        let open_w = (opening / max_rate * 100.0).min(100.0);
        let close_w = (closure / max_rate * 100.0).min(100.0);
        let net_color = if net >= 0 { "#22c55e" } else { "#ef4444" };
        let net_sign = if net >= 0 { "+" } else { "" };

        html.push_str(&format!(
            r#"<div class="flex items-center gap-2">
                <div class="w-16 text-xs text-slate-400 text-right flex-shrink-0">{fy}</div>
                <div class="flex-1 space-y-1">
                    <div class="flex items-center gap-1">
                        <div class="w-10 text-xs text-slate-500 text-right">開業</div>
                        <div class="flex-1 bg-navy-800/60 rounded-full h-4 relative overflow-hidden">
                            <div class="h-full rounded-full" style="width:{open_w:.1}%;background:linear-gradient(to right,#10b981,#4ade80)"></div>
                        </div>
                        <div class="w-14 text-xs text-emerald-400 text-right">{opening:.2}%</div>
                    </div>
                    <div class="flex items-center gap-1">
                        <div class="w-10 text-xs text-slate-500 text-right">廃業</div>
                        <div class="flex-1 bg-navy-800/60 rounded-full h-4 relative overflow-hidden">
                            <div class="h-full rounded-full" style="width:{close_w:.1}%;background:linear-gradient(to right,#f43f5e,#f87171)"></div>
                        </div>
                        <div class="w-14 text-xs text-rose-400 text-right">{closure:.2}%</div>
                    </div>
                </div>
                <div class="w-20 text-xs text-right flex-shrink-0" style="color:{net_color}">{net_sign}{net_val}</div>
            </div>"#,
            fy = escape_html(fy),
            net_val = format_number(net),
        ));
    }

    html.push_str(r#"</div>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 経済センサス（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 気象特性セクション（最新年度のサマリーカード、2x3グリッド）
fn render_climate_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }

    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // 最新年度のデータ（最後の行）を使用
    let latest = data.last().unwrap();
    let fy = get_str(latest, "fiscal_year");

    let avg_temp = get_f64(latest, "avg_temperature");
    let max_temp = get_f64(latest, "max_temperature");
    let min_temp = get_f64(latest, "min_temperature");
    let snow = get_f64(latest, "snow_days");
    let sun = get_f64(latest, "sunshine_hours");
    let rain = get_f64(latest, "precipitation");

    let mut html = String::with_capacity(4_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🌡️ 気象特性 <span class="text-blue-400">【{}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">主要気象指標のサマリー（{}年度）。通勤・勤務環境の参考に。</p>
        <div class="grid grid-cols-2 md:grid-cols-3 gap-3">"#,
        escape_html(pref_label),
        escape_html(fy),
    ));

    // 6つの指標カード
    let indicators: [(&str, &str, String, &str); 6] = [
        ("年平均気温", "🌡️", format!("{:.1}℃", avg_temp), "#3b82f6"),
        ("最高気温", "☀️", format!("{:.1}℃", max_temp), "#ef4444"),
        ("最低気温", "❄️", format!("{:.1}℃", min_temp), "#06b6d4"),
        ("降雪日数", "🌨️", format!("{:.0}日", snow), "#94a3b8"),
        ("日照時間", "☀️", format!("{:.0}時間", sun), "#f59e0b"),
        ("年間降水量", "🌧️", format!("{:.0}mm", rain), "#6366f1"),
    ];

    for (label, icon, value, color) in &indicators {
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-3 text-center">
                <div class="text-lg mb-1">{icon}</div>
                <div class="text-xs text-slate-500 mb-1">{label}</div>
                <div class="text-lg font-bold" style="color:{color}">{value}</div>
            </div>"#,
        ));
    }

    html.push_str(r#"</div>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 社会・人口統計体系（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 介護需要推移セクション（SVG折れ線グラフ: 介護給付件数）
fn render_care_demand_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }

    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // 介護保険給付件数を抽出
    let points: Vec<(String, f64)> = data
        .iter()
        .map(|r| {
            let fy = get_str(r, "fiscal_year").to_string();
            let cases = get_f64(r, "insurance_benefit_cases");
            (fy, cases)
        })
        .filter(|(_, v)| *v > 0.0)
        .collect();

    if points.is_empty() {
        return String::new();
    }

    let min_val = points.iter().map(|(_, v)| *v).fold(f64::MAX, f64::min);
    let max_val = points.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max);
    let range = (max_val - min_val).max(1.0);

    // SVGパラメータ
    let svg_w: f64 = 800.0;
    let svg_h: f64 = 300.0;
    let pad_l: f64 = 80.0;
    let pad_r: f64 = 20.0;
    let pad_t: f64 = 20.0;
    let pad_b: f64 = 40.0;
    let chart_w = svg_w - pad_l - pad_r;
    let chart_h = svg_h - pad_t - pad_b;
    let n = points.len();

    // 折れ線のパスを生成
    let mut path = String::new();
    let mut circles = String::new();
    let mut x_labels = String::new();

    for (i, (fy, val)) in points.iter().enumerate() {
        let x = pad_l
            + if n > 1 {
                chart_w * (i as f64) / ((n - 1) as f64)
            } else {
                chart_w / 2.0
            };
        let y = pad_t + chart_h - chart_h * (val - min_val) / range;

        if i == 0 {
            path.push_str(&format!("M{:.1},{:.1}", x, y));
        } else {
            path.push_str(&format!(" L{:.1},{:.1}", x, y));
        }

        // データ点の円
        circles.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"4\" fill=\"#3b82f6\" stroke=\"#1e3a5f\" stroke-width=\"1.5\"/>",
            x, y
        ));

        // X軸ラベル（年度）
        x_labels.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{}\" text-anchor=\"middle\" fill=\"#94a3b8\" font-size=\"11\">{}</text>",
            x, svg_h - 5.0, escape_html(fy)
        ));
    }

    // Y軸ラベル（5本の目盛線）
    let mut y_guides = String::new();
    for i in 0..5 {
        let frac = i as f64 / 4.0;
        let val = min_val + range * frac;
        let y = pad_t + chart_h - chart_h * frac;
        // 千件単位で表示
        let label = if val >= 1_000_000.0 {
            format!("{:.0}万", val / 10_000.0)
        } else if val >= 1_000.0 {
            format!("{:.0}千", val / 1_000.0)
        } else {
            format!("{:.0}", val)
        };
        y_guides.push_str(&format!(
            "<line x1=\"{}\" y1=\"{:.1}\" x2=\"{}\" y2=\"{:.1}\" stroke=\"#334155\" stroke-width=\"0.5\" stroke-dasharray=\"4\"/>\
            <text x=\"{}\" y=\"{:.1}\" text-anchor=\"end\" fill=\"#94a3b8\" font-size=\"11\" dominant-baseline=\"middle\">{}</text>",
            pad_l, y, svg_w - pad_r, y,
            pad_l - 8.0, y, label
        ));
    }

    let mut html = String::with_capacity(6_000);
    html.push_str(&format!(
        "<div class=\"stat-card\">\
        <h3 class=\"text-sm text-slate-400 mb-1\">🏥 介護需要の推移 <span class=\"text-blue-400\">【{}】</span></h3>\
        <p class=\"text-xs text-slate-500 mb-4\">介護保険給付件数の推移。求人需要の先行指標。</p>\
        <svg viewBox=\"0 0 {} {}\" class=\"w-full\" style=\"max-height:300px\">\
        {}\
        <path d=\"{}\" fill=\"none\" stroke=\"#3b82f6\" stroke-width=\"2.5\" stroke-linejoin=\"round\"/>\
        {}\
        {}\
        </svg>",
        escape_html(pref_label),
        svg_w, svg_h,
        y_guides,
        path,
        circles,
        x_labels,
    ));
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 社会・人口統計体系（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

fn render_region_benchmark_section(data: &[Row]) -> String {
    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🎯 地域ベンチマーク（12軸）</h3>
        <p class="text-xs text-slate-500 mb-4">12の指標で地域の求人市場を総合評価。各軸0-100のスケールで、スコアが高いほど当該地域が優位です。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#);

    let axis_labels: [(&str, &str, &str); 12] = [
        ("salary_competitiveness", "給与競争力", "#22c55e"),
        ("job_market_tightness", "求人逼迫度", "#3b82f6"),
        ("wage_compliance", "賃金遵守率", "#10b981"),
        ("industry_diversity", "産業多様性", "#f59e0b"),
        ("info_transparency", "情報透明性", "#06b6d4"),
        ("text_urgency", "求人切迫度", "#ec4899"),
        ("posting_freshness", "求人鮮度", "#8b5cf6"),
        ("real_wage_power", "実質賃金力", "#14b8a6"),
        ("labor_fluidity", "労働力流動性", "#f97316"),
        ("working_age_ratio", "生産年齢比率", "#6366f1"),
        ("population_growth", "人口社会増減", "#84cc16"),
        ("foreign_workforce", "外国人労働力", "#a855f7"),
    ];

    for row in data {
        let grp = get_str(row, "emp_group");
        let composite = get_f64(row, "composite_benchmark");
        let comp_color = if composite >= 70.0 {
            "#22c55e"
        } else if composite >= 50.0 {
            "#eab308"
        } else {
            "#ef4444"
        };

        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4">
                <div class="text-sm font-semibold text-white mb-2">{grp}</div>
                <div class="flex items-baseline gap-2 mb-3">
                    <span class="text-2xl font-bold" style="color:{comp_color}">{composite:.1}</span>
                    <span class="text-xs text-slate-400">総合スコア</span>
                </div>"#
        ));

        // 12軸の水平バー
        for (key, label, color) in &axis_labels {
            let val = get_f64(row, key);
            if val <= 0.0 {
                // データなしの軸はスキップ
                continue;
            }
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

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 各軸の元データは上記セクション参照。軸8-12は外部統計(厚労省/総務省/統計センター)に基づく</p>"#);
    html.push_str("</div></div>");
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

        let score_color = if score >= 75.0 {
            "#ef4444"
        } else if score >= 50.0 {
            "#f59e0b"
        } else {
            "#22c55e"
        };
        let score_label = if score >= 75.0 {
            "充足困難"
        } else if score >= 50.0 {
            "やや困難"
        } else {
            "充足容易"
        };

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
                    <div class="w-full flex rounded h-5 overflow-hidden">
                        <div class="h-5" style="width:{a_w:.1}%;background:#22c55e" title="A（容易）"></div>
                        <div class="h-5" style="width:{b_w:.1}%;background:#3b82f6" title="B（標準）"></div>
                        <div class="h-5" style="width:{c_w:.1}%;background:#f59e0b" title="C（やや困難）"></div>
                        <div class="h-5" style="width:{d_w:.1}%;background:#ef4444" title="D（困難）"></div>
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
            a_s = format!("{:.1}%", a_pct),
            b_s = format!("{:.1}%", b_pct),
            c_s = format!("{:.1}%", c_pct),
            d_s = format!("{:.1}%", d_pct),
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
        let net_label = if net >= 0.0 {
            "流入超過（人材吸引力あり）"
        } else {
            "流出超過（人材流出リスク）"
        };

        let destinations: Vec<&str> = top3
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .collect();

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
            html.push_str(
                r#"<div class="mt-2 pt-2 border-t border-slate-700">
                <div class="text-xs text-slate-500 mb-1">主要流出先</div>"#,
            );
            for (i, dest) in destinations.iter().enumerate().take(3) {
                html.push_str(&format!(
                    r#"<div class="text-xs text-slate-300">{}. {}</div>"#,
                    i + 1,
                    dest
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

    html.push_str(
        r#"<div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr>
            <th>雇用形態</th><th>給与種別</th><th class="text-right">件数</th>
            <th class="text-right">P10</th><th class="text-right">P25</th>
            <th class="text-right">P50</th><th class="text-right">P75</th>
            <th class="text-right">P90</th><th class="text-right">平均</th>
            <th style="width:120px">分布</th>
        </tr></thead><tbody>"#,
    );

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
        let box_width = ((p75 - p25) / max_val * 100.0)
            .min(100.0 - box_left)
            .max(0.0);
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

// ======== サブタブ7: 通勤圏分析 ========

/// 通勤圏分析（30km圏内の都道府県またぎ市区町村 + 性別×年齢ピラミッド）
pub(crate) fn render_subtab_7(
    db: &crate::db::local_sqlite::LocalDb,
    turso: Option<&crate::db::turso_http::TursoDb>,
    pref: &str,
    muni: &str,
) -> String {
    use super::fetch::{
        fetch_commute_zone, fetch_commute_zone_pyramid, fetch_population_pyramid,
        fetch_spatial_mismatch,
    };

    let mut html = String::with_capacity(8_000);
    html.push_str(r#"<div class="space-y-4">"#);

    if muni.is_empty() {
        html.push_str(r#"<div class="stat-card text-center py-8">
            <p class="text-slate-400 text-sm">市区町村を選択すると通勤圏分析が表示されます</p>
            <p class="text-slate-500 text-xs mt-1">30km圏内の隣接市区町村（都道府県またぎ）を自動抽出します</p>
        </div></div>"#);
        return html;
    }

    // 30km圏内の市区町村を抽出
    let zone = fetch_commute_zone(db, pref, muni, 30.0);
    if zone.is_empty() {
        html.push_str(r#"<div class="stat-card"><p class="text-slate-400 text-sm">通勤圏データを取得できませんでした</p></div></div>"#);
        return html;
    }

    // 通勤圏ピラミッド集約
    let zone_pyramid = fetch_commute_zone_pyramid(db, turso, &zone);
    // 選択市区町村単体のピラミッド
    let local_pyramid = fetch_population_pyramid(db, turso, pref, muni);
    // 空間ミスマッチ
    let spatial = fetch_spatial_mismatch(db, pref, muni);

    // 都道府県カウント
    let mut pref_set = std::collections::HashSet::new();
    for m in &zone {
        pref_set.insert(m.prefecture.as_str());
    }
    let pref_count = pref_set.len();

    // 通勤圏人口集計
    let mut zone_total_pop: i64 = 0;
    let mut zone_working_age: i64 = 0;
    let mut zone_elderly: i64 = 0;
    for row in &zone_pyramid {
        let male = get_i64(row, "male_count");
        let female = get_i64(row, "female_count");
        let total = male + female;
        zone_total_pop += total;
        let age = get_str(row, "age_group");
        match age {
            "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49" | "50-54"
            | "55-59" | "60-64" | "10-19" | "20-29" | "30-39" | "40-49" | "50-59" | "60-69" => {
                zone_working_age += total
            }
            _ => {}
        }
        match age {
            "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => {
                zone_elderly += total
            }
            _ => {}
        }
    }
    let aging_rate = if zone_total_pop > 0 {
        zone_elderly as f64 / zone_total_pop as f64
    } else {
        0.0
    };

    // ヘッダー
    html.push_str(&format!(
        r#"<h3 class="text-lg font-semibold text-white">🌐 通勤圏分析 <span class="text-blue-400 text-base">{pref} {muni}</span> の30km圏内</h3>
        <p class="text-xs text-slate-500">圏内市区町村: {}件（{}県にまたがる）</p>"#,
        zone.len(), pref_count
    ));

    // KPIカード
    html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
    kpi(
        &mut html,
        "圏内総人口",
        &format!("{}人", format_number(zone_total_pop)),
        "text-blue-400",
    );
    kpi(
        &mut html,
        "生産年齢人口",
        &format!("{}人", format_number(zone_working_age)),
        "text-emerald-400",
    );
    kpi(
        &mut html,
        "高齢化率",
        &format!("{:.1}%", aging_rate * 100.0),
        if aging_rate > 0.30 {
            "text-red-400"
        } else if aging_rate > 0.25 {
            "text-amber-400"
        } else {
            "text-green-400"
        },
    );
    kpi(
        &mut html,
        "対象市区町村",
        &format!("{}件 / {}県", zone.len(), pref_count),
        "text-cyan-400",
    );
    html.push_str("</div>");

    // 蝶形ピラミッドチャート
    if !zone_pyramid.is_empty() {
        let chart = build_butterfly_pyramid(&zone_pyramid, &local_pyramid, muni);
        html.push_str(&format!(
            r#"<div class="stat-card">
                <h4 class="text-sm text-slate-400 mb-2">性別×年齢 人口ピラミッド（通勤圏 vs {muni}）</h4>
                <div class="echart" style="height:500px;" data-chart-config='{chart}'></div>
                <div class="text-xs text-slate-600 mt-1">※通勤圏(30km圏内)の全市区町村人口を合算</div>
            </div>"#
        ));
    }

    // 空間ミスマッチ情報
    if !spatial.is_empty() {
        html.push_str(
            r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">通勤圏求人市場</h4>"#,
        );
        html.push_str(r#"<div class="grid grid-cols-2 md:grid-cols-4 gap-3">"#);
        // 正社員優先、なければfirst()にフォールバック
        let (sm_row, is_fallback) =
            if let Some(row) = spatial.iter().find(|r| get_str(r, "emp_group") == "正社員") {
                (Some(row), false)
            } else {
                (spatial.first(), true)
            };
        if let Some(row) = sm_row {
            if is_fallback {
                let grp = get_str(row, "emp_group");
                html.push_str(&format!(r#"<p class="text-xs text-amber-400 mb-2">※正社員データなし。{}のデータを表示</p>"#, escape_html(grp)));
            }
            let acc30 = get_i64(row, "accessible_postings_30km");
            let local = get_i64(row, "posting_count");
            let gap = get_f64(row, "salary_gap_vs_accessible");
            let iso = get_f64(row, "isolation_score");
            kpi(
                &mut html,
                "30km圏内求人数",
                &format_number(acc30),
                "text-blue-400",
            );
            kpi(&mut html, "地元求人数", &format_number(local), "text-white");
            kpi(
                &mut html,
                "給与差(対圏内)",
                &format!("{:+.0}円", gap),
                if gap < 0.0 {
                    "text-red-400"
                } else {
                    "text-green-400"
                },
            );
            kpi(
                &mut html,
                "孤立スコア",
                &format!("{:.2}", iso),
                if iso > 0.5 {
                    "text-red-400"
                } else {
                    "text-green-400"
                },
            );
        }
        html.push_str("</div></div>");
    }

    // 圏内市区町村テーブル
    html.push_str(
        r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">圏内市区町村一覧</h4>"#,
    );
    html.push_str(r#"<div class="overflow-x-auto max-h-80"><table class="w-full text-xs">"#);
    html.push_str(
        r#"<thead><tr class="text-slate-500 border-b border-slate-700">
        <th class="text-left py-1 px-2">県</th>
        <th class="text-left py-1 px-2">市区町村</th>
        <th class="text-right py-1 px-2">距離</th>
    </tr></thead><tbody>"#,
    );
    for m in zone.iter().take(50) {
        let is_self = m.prefecture == pref && m.municipality == muni;
        let style = if is_self {
            r#" class="text-blue-400 font-medium""#
        } else {
            ""
        };
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800"><td class="py-1 px-2"{style}>{}</td><td class="py-1 px-2"{style}>{}</td><td class="text-right py-1 px-2">{:.1}km</td></tr>"#,
            escape_html(&m.prefecture), escape_html(&m.municipality), m.distance_km
        ));
    }
    html.push_str("</tbody></table></div></div>");

    // ======== 通勤フロー（実データ: 国勢調査OD） ========
    let inflow = fetch_commute_inflow(db, pref, muni);
    let outflow = fetch_commute_outflow(db, pref, muni);

    if !inflow.is_empty() || !outflow.is_empty() {
        let self_rate = fetch_self_commute_rate(db, pref, muni);

        // サンキーダイアグラム
        html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">🔄 通勤フロー（国勢調査実データ）</h4>"#);

        if !inflow.is_empty() || !outflow.is_empty() {
            let sankey = build_commute_sankey(&inflow, &outflow, muni);
            html.push_str(&format!(
                r#"<div class="echart" style="height:400px;" data-chart-config='{sankey}'></div>"#
            ));
        }

        // 地元就業率
        html.push_str(&format!(
            r#"<div class="text-xs text-slate-500 mt-2">地元就業率: {:.1}%（住民のうち同市区町村内で働く人の割合）※2020年国勢調査</div>"#,
            self_rate * 100.0
        ));
        html.push_str("</div>");

        // 流入元複合テーブル
        if !inflow.is_empty() {
            html.push_str(r#"<div class="stat-card"><h4 class="text-sm text-slate-400 mb-2">📥 通勤流入元 Top 10（実フロー）</h4>"#);
            html.push_str(r#"<div class="overflow-x-auto"><table class="w-full text-xs">"#);
            html.push_str(
                r#"<thead><tr class="text-slate-500 border-b border-slate-700">
                <th class="text-left py-1 px-2">流入元</th>
                <th class="text-right py-1 px-2">通勤者数</th>
                <th class="text-right py-1 px-2">男性</th>
                <th class="text-right py-1 px-2">女性</th>
            </tr></thead><tbody>"#,
            );
            for f in inflow.iter().take(10) {
                let loc = format!(
                    "{}{}",
                    escape_html(&f.partner_pref),
                    escape_html(&f.partner_muni)
                );
                let cross_pref = if f.partner_pref != pref { " 🔀" } else { "" };
                html.push_str(&format!(
                    r#"<tr class="border-b border-slate-800">
                        <td class="py-1 px-2 text-white">{loc}{cross_pref}</td>
                        <td class="text-right py-1 px-2 text-blue-400 font-mono">{}</td>
                        <td class="text-right py-1 px-2 text-slate-400">{}</td>
                        <td class="text-right py-1 px-2 text-slate-400">{}</td>
                    </tr>"#,
                    format_number(f.total_commuters),
                    format_number(f.male_commuters),
                    format_number(f.female_commuters),
                ));
            }
            html.push_str("</tbody></table></div>");
            html.push_str(r#"<div class="text-xs text-slate-600 mt-1">🔀 = 都道府県またぎ ※2020年国勢調査</div>"#);
            html.push_str("</div>");
        }
    }

    html.push_str("</div>");
    html
}

/// サンキーダイアグラムECharts JSON生成
fn build_commute_sankey(
    inflow: &[super::fetch::CommuteFlow],
    outflow: &[super::fetch::CommuteFlow],
    center_name: &str,
) -> String {
    use serde_json::json;

    let mut node_values: Vec<serde_json::Value> = vec![json!({"name": center_name})];
    let mut link_values: Vec<serde_json::Value> = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // 流入（左→中央）
    for f in inflow.iter().take(10) {
        let name = format!("{}{}", f.partner_pref, f.partner_muni);
        if !seen.contains(&name) {
            node_values.push(json!({"name": &name}));
            seen.insert(name.clone());
        }
        link_values.push(json!({
            "source": &name,
            "target": center_name,
            "value": f.total_commuters
        }));
    }

    // 流出（中央→右）
    for f in outflow.iter().take(10) {
        let name = format!("{}{}(流出)", f.partner_pref, f.partner_muni);
        if !seen.contains(&name) {
            node_values.push(json!({"name": &name}));
            seen.insert(name.clone());
        }
        link_values.push(json!({
            "source": center_name,
            "target": &name,
            "value": f.total_commuters
        }));
    }

    let config = json!({
        "tooltip": {"trigger": "item"},
        "series": [{
            "type": "sankey",
            "layout": "none",
            "emphasis": {"focus": "adjacency"},
            "nodeAlign": "justify",
            "data": node_values,
            "links": link_values,
            "lineStyle": {"color": "gradient", "curveness": 0.5},
            "label": {"color": "#e2e8f0", "fontSize": 10}
        }]
    });
    config.to_string().replace('\'', "&#39;")
}

/// 蝶形ピラミッドECharts JSON生成
fn build_butterfly_pyramid(zone: &[Row], local: &[Row], muni_name: &str) -> String {
    use serde_json::json;

    let ages: Vec<String> = zone
        .iter()
        .map(|r| get_str(r, "age_group").to_string())
        .collect();

    let zone_male: Vec<i64> = zone.iter().map(|r| -get_i64(r, "male_count")).collect();
    let zone_female: Vec<i64> = zone.iter().map(|r| get_i64(r, "female_count")).collect();

    // ローカルピラミッド（年齢順にマッチング）
    let local_map: std::collections::HashMap<String, (i64, i64)> = local
        .iter()
        .map(|r| {
            let age = get_str(r, "age_group").to_string();
            let m = get_i64(r, "male_count");
            let f = get_i64(r, "female_count");
            (age, (m, f))
        })
        .collect();

    let local_male: Vec<i64> = ages
        .iter()
        .map(|a| -local_map.get(a).map(|(m, _)| *m).unwrap_or(0))
        .collect();
    let local_female: Vec<i64> = ages
        .iter()
        .map(|a| local_map.get(a).map(|(_, f)| *f).unwrap_or(0))
        .collect();

    let legend_male_local = format!("男性({})", muni_name);
    let legend_female_local = format!("女性({})", muni_name);

    let config = json!({
        "tooltip": {"trigger": "axis", "axisPointer": {"type": "shadow"}},
        "legend": {
            "data": ["男性(通勤圏)", "女性(通勤圏)", &legend_male_local, &legend_female_local],
            "textStyle": {"color": "#94a3b8", "fontSize": 10},
            "bottom": 0
        },
        "grid": {"left": "3%", "right": "3%", "top": "3%", "bottom": "12%", "containLabel": true},
        "xAxis": {"type": "value"},
        "yAxis": {"type": "category", "data": ages, "axisTick": {"show": false}},
        "series": [
            {
                "name": "男性(通勤圏)",
                "type": "bar",
                "data": zone_male,
                "itemStyle": {"color": "rgba(59,130,246,0.7)"}
            },
            {
                "name": "女性(通勤圏)",
                "type": "bar",
                "data": zone_female,
                "itemStyle": {"color": "rgba(236,72,153,0.7)"}
            },
            {
                "name": &legend_male_local,
                "type": "bar",
                "data": local_male,
                "barGap": "-100%",
                "itemStyle": {"color": "rgba(59,130,246,0.3)"}
            },
            {
                "name": &legend_female_local,
                "type": "bar",
                "data": local_female,
                "barGap": "-100%",
                "itemStyle": {"color": "rgba(236,72,153,0.3)"}
            }
        ]
    });
    config.to_string().replace('\'', "&#39;")
}

fn kpi(html: &mut String, label: &str, value: &str, color: &str) {
    html.push_str(&format!(
        r#"<div class="stat-card text-center">
            <div class="text-lg font-bold {color}">{value}</div>
            <div class="text-xs text-slate-500">{label}</div>
        </div>"#
    ));
}

// ======== HTML描画: Phase 4-7（新外部データセクション） ========

/// 学歴分布セクション（横棒グラフ + テーブル）
fn render_education_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // 合計人数を計算して構成比を算出
    let total_all: i64 = data.iter().map(|r| get_i64(r, "total_count")).sum();
    if total_all == 0 {
        return String::new();
    }

    // ECharts 横棒グラフ用データ（学歴ラベル・構成比）
    let labels_json: Vec<String> = data
        .iter()
        .map(|r| format!("\"{}\"", escape_html(&get_str(r, "education_level"))))
        .collect();
    let pct_vals: Vec<String> = data
        .iter()
        .map(|r| {
            let cnt = get_i64(r, "total_count");
            format!("{:.1}", cnt as f64 / total_all as f64 * 100.0)
        })
        .collect();

    let chart_config = format!(
        r##"{{"tooltip":{{"trigger":"axis","axisPointer":{{"type":"shadow"}},"formatter":"{{b}}: {{c}}%"}},"grid":{{"left":"22%","right":"5%","top":"5%","bottom":"5%"}},"xAxis":{{"type":"value","axisLabel":{{"formatter":"{{value}}%","color":"#94a3b8"}},"max":100}},"yAxis":{{"type":"category","data":[{labels}],"axisLabel":{{"color":"#94a3b8","fontSize":11}}}},"series":[{{"type":"bar","data":[{vals}],"itemStyle":{{"color":"#6366f1"}},"label":{{"show":true,"position":"right","formatter":"{{c}}%","color":"#94a3b8","fontSize":10}}}}]}}"##,
        labels = labels_json.join(","),
        vals = pct_vals.join(","),
    );

    let mut html = String::with_capacity(4_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🎓 学歴分布（国勢調査2020年）<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">就業者・求職者層の学歴構成。採用基準策定や研修計画の参考指標。</p>
        <div class="echart" style="height:220px;" data-chart-config='{chart_config}'></div>"#,
        pref_label = escape_html(pref_label),
        chart_config = chart_config.replace('\'', "&#39;"),
    ));

    // テーブル
    html.push_str(r#"<table class="min-w-full text-sm mt-4"><thead><tr class="text-slate-400 border-b border-slate-700"><th class="text-left py-1 pr-3">学歴</th><th class="text-right py-1 pr-3">男性</th><th class="text-right py-1 pr-3">女性</th><th class="text-right py-1">合計</th></tr></thead><tbody>"#);
    for row in data {
        let level = escape_html(&get_str(row, "education_level"));
        let male = get_i64(row, "male_count");
        let female = get_i64(row, "female_count");
        let total = get_i64(row, "total_count");
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/30"><td class="py-1 pr-3 text-slate-300">{level}</td><td class="py-1 pr-3 text-right text-slate-400">{male}</td><td class="py-1 pr-3 text-right text-slate-400">{female}</td><td class="py-1 text-right text-slate-200 font-semibold">{total}</td></tr>"#,
            male = format_number(male),
            female = format_number(female),
            total = format_number(total),
        ));
    }
    html.push_str(r#"</tbody></table>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 国勢調査2020年 e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 世帯構成セクション（ドーナツチャート + テーブル）
fn render_household_type_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // ECharts ドーナツチャート用データ
    let pie_data: Vec<String> = data
        .iter()
        .map(|r| {
            let name = escape_html(&get_str(r, "household_type"));
            let cnt = get_i64(r, "count");
            format!(r#"{{"name":"{name}","value":{cnt}}}"#)
        })
        .collect();

    let chart_config = format!(
        r##"{{"tooltip":{{"trigger":"item","formatter":"{{b}}: {{c}}世帯 ({{d}}%)"}},"legend":{{"orient":"vertical","right":"5%","top":"center","textStyle":{{"color":"#94a3b8","fontSize":11}}}},"series":[{{"type":"pie","radius":["40%","70%"],"center":["40%","50%"],"data":[{data}],"label":{{"show":false}},"itemStyle":{{"borderRadius":4}}}}]}}"##,
        data = pie_data.join(","),
    );

    let mut html = String::with_capacity(3_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🏠 世帯構成（国勢調査2020年）<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">世帯類型の分布。単独世帯比率が高い地域では若年・単身向け求人ニーズが高い傾向。</p>
        <div class="echart" style="height:220px;" data-chart-config='{chart_config}'></div>"#,
        pref_label = escape_html(pref_label),
        chart_config = chart_config.replace('\'', "&#39;"),
    ));

    // テーブル
    html.push_str(r#"<table class="min-w-full text-sm mt-4"><thead><tr class="text-slate-400 border-b border-slate-700"><th class="text-left py-1 pr-3">世帯類型</th><th class="text-right py-1 pr-3">世帯数</th><th class="text-right py-1">構成比</th></tr></thead><tbody>"#);
    for row in data {
        let htype = escape_html(&get_str(row, "household_type"));
        let cnt = get_i64(row, "count");
        let ratio = get_f64(row, "ratio");
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/30"><td class="py-1 pr-3 text-slate-300">{htype}</td><td class="py-1 pr-3 text-right text-slate-400">{cnt}</td><td class="py-1 text-right text-slate-200">{ratio:.1}%</td></tr>"#,
            cnt = format_number(cnt),
        ));
    }
    html.push_str(r#"</tbody></table>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 国勢調査2020年 e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 在留外国人セクション（テーブルのみ）
fn render_foreign_residents_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    let mut html = String::with_capacity(3_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🌏 在留外国人（在留資格別）<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">在留資格別の外国人数。外国人労働者の雇用可能性や多文化対応ニーズの把握に活用。</p>"#,
        pref_label = escape_html(pref_label),
    ));

    html.push_str(r#"<table class="min-w-full text-sm"><thead><tr class="text-slate-400 border-b border-slate-700"><th class="text-left py-1 pr-3">在留資格</th><th class="text-right py-1 pr-3">人数</th><th class="text-right py-1">調査時点</th></tr></thead><tbody>"#);
    for row in data {
        let visa = escape_html(&get_str(row, "visa_status"));
        let cnt = get_i64(row, "count");
        let period = escape_html(&get_str(row, "survey_period"));
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/30"><td class="py-1 pr-3 text-slate-300">{visa}</td><td class="py-1 pr-3 text-right text-slate-200 font-semibold">{cnt}</td><td class="py-1 text-right text-slate-500 text-xs">{period}</td></tr>"#,
            cnt = format_number(cnt),
        ));
    }
    html.push_str(r#"</tbody></table>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 在留外国人統計（法務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 地価公示セクション（KPIカード3枚）
fn render_land_price_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // 用途別（住宅地・商業地・工業地）にデータを取り出す
    let land_uses = [("住宅地", "🏘️"), ("商業地", "🏬"), ("工業地", "🏭")];

    let mut html = String::with_capacity(3_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🏗️ 地価（国土交通省 地価公示）<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">用途別平均地価（円/m²）と前年比変動率。事業所開設コストや地域経済活力の参考指標。</p>
        <div class="grid grid-cols-1 md:grid-cols-3 gap-4">"#,
        pref_label = escape_html(pref_label),
    ));

    for (use_name, icon) in &land_uses {
        // 用途名が前方一致するデータを探す
        let row_opt = data.iter().find(|r| {
            let lu = get_str(r, "land_use");
            lu.contains(use_name)
        });
        if let Some(row) = row_opt {
            let price = get_f64(row, "avg_price_per_sqm");
            let yoy = get_f64(row, "yoy_change_pct");
            let year = get_i64(row, "year");
            let pts = get_i64(row, "point_count");
            // 前年比の色（プラスは緑、マイナスは赤）
            let yoy_color = if yoy >= 0.0 { "#22c55e" } else { "#ef4444" };
            let yoy_sign = if yoy >= 0.0 { "+" } else { "" };
            html.push_str(&format!(
                r#"<div class="bg-navy-700/50 rounded-lg p-4 text-center">
                    <div class="text-2xl mb-1">{icon}</div>
                    <div class="text-sm font-semibold text-slate-300 mb-2">{use_name}</div>
                    <div class="text-xl font-bold text-white">{price:.0}<span class="text-xs text-slate-400 ml-1">円/m²</span></div>
                    <div class="text-sm font-semibold mt-1" style="color:{yoy_color}">{yoy_sign}{yoy:.1}%</div>
                    <div class="text-xs text-slate-500 mt-1">{year}年 ({pts}地点)</div>
                </div>"#,
            ));
        } else {
            html.push_str(&format!(
                r#"<div class="bg-navy-700/50 rounded-lg p-4 text-center">
                    <div class="text-2xl mb-1">{icon}</div>
                    <div class="text-sm font-semibold text-slate-300 mb-2">{use_name}</div>
                    <div class="text-xs text-slate-500">データなし</div>
                </div>"#,
            ));
        }
    }

    html.push_str(r#"</div>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 地価公示（国土交通省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 地域インフラ指標セクション（自動車保有率 + ネット利用率 KPIカード）
fn render_regional_infra_section(car_data: &[Row], net_data: &[Row], pref: &str) -> String {
    if car_data.is_empty() && net_data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    let mut html = String::with_capacity(2_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🚗 地域インフラ指標<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">自動車保有率とインターネット利用率。通勤手段や採用チャネル（SNS・WEB）の有効性を判断する参考指標。</p>
        <div class="grid grid-cols-1 md:grid-cols-2 gap-4">"#,
        pref_label = escape_html(pref_label),
    ));

    // 自動車保有率KPI
    if let Some(row) = car_data.first() {
        let rate = get_f64(row, "cars_per_100people");
        let year = get_i64(row, "year");
        let color = if rate >= 70.0 {
            "#22c55e"
        } else if rate >= 50.0 {
            "#eab308"
        } else {
            "#94a3b8"
        };
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4 text-center">
                <div class="text-2xl mb-1">🚗</div>
                <div class="text-sm font-semibold text-slate-300 mb-2">自動車保有率</div>
                <div class="text-2xl font-bold" style="color:{color}">{rate:.1}<span class="text-sm text-slate-400 ml-1">台/100人</span></div>
                <div class="text-xs text-slate-500 mt-1">{year}年</div>
            </div>"#,
        ));
    }

    // ネット利用率KPI
    if let Some(row) = net_data.first() {
        let net_rate = get_f64(row, "internet_usage_rate");
        let sp_rate = get_f64(row, "smartphone_ownership_rate");
        let year = get_i64(row, "year");
        let color = if net_rate >= 80.0 {
            "#22c55e"
        } else if net_rate >= 60.0 {
            "#eab308"
        } else {
            "#ef4444"
        };
        html.push_str(&format!(
            r#"<div class="bg-navy-700/50 rounded-lg p-4 text-center">
                <div class="text-2xl mb-1">📱</div>
                <div class="text-sm font-semibold text-slate-300 mb-2">インターネット利用率</div>
                <div class="text-2xl font-bold" style="color:{color}">{net_rate:.1}<span class="text-sm text-slate-400 ml-1">%</span></div>
                <div class="text-xs text-slate-500 mt-1">スマートフォン保有率 {sp_rate:.1}% ({year}年)</div>
            </div>"#,
        ));
    }

    html.push_str(r#"</div>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 自動車保有台数統計（国土交通省）・通信利用動向調査（総務省） e-Stat API ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 住民の行動特性セクション（レーダーチャート: 社会生活基本調査）
fn render_social_life_section(data: &[Row], pref: &str) -> String {
    if data.is_empty() {
        return String::new();
    }
    let pref_label = if pref.is_empty() { "全国" } else { pref };

    // レーダー軸: カテゴリ別の行動者率を集約
    // category列でグループ化し、平均行動者率を算出
    let mut category_map: std::collections::HashMap<String, Vec<f64>> =
        std::collections::HashMap::new();
    for row in data {
        let cat = get_str(row, "category").to_string();
        let rate = get_f64(row, "participation_rate");
        category_map.entry(cat).or_default().push(rate);
    }
    // 各カテゴリの平均行動者率
    let mut categories: Vec<(String, f64)> = category_map
        .into_iter()
        .map(|(cat, vals)| {
            let avg = vals.iter().sum::<f64>() / vals.len() as f64;
            (cat, avg)
        })
        .collect();
    categories.sort_by(|a, b| a.0.cmp(&b.0));

    if categories.is_empty() {
        return String::new();
    }

    // ECharts レーダーチャート用データ
    let indicators: Vec<String> = categories
        .iter()
        .map(|(cat, _)| format!(r#"{{"name":"{}","max":100}}"#, escape_html(cat)))
        .collect();
    let values: Vec<String> = categories
        .iter()
        .map(|(_, v)| format!("{:.1}", v))
        .collect();

    let chart_config = format!(
        r##"{{"tooltip":{{}},"radar":{{"indicator":[{indicators}],"radius":"60%","axisName":{{"color":"#94a3b8","fontSize":11}},"splitLine":{{"lineStyle":{{"color":"#334155"}}}},"splitArea":{{"areaStyle":{{"color":["rgba(51,65,85,0.3)","rgba(51,65,85,0.1)"]}}}}}},"series":[{{"type":"radar","data":[{{"value":[{values}],"name":"行動者率 (%)","areaStyle":{{"color":"rgba(99,102,241,0.3)"}},"lineStyle":{{"color":"#6366f1"}},"itemStyle":{{"color":"#6366f1"}}}}]}}]}}"##,
        indicators = indicators.join(","),
        values = values.join(","),
    );

    let mut html = String::with_capacity(3_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">🧭 住民の行動特性（社会生活基本調査2021年）<span class="text-blue-400 ml-2">【{pref_label}】</span></h3>
        <p class="text-xs text-slate-500 mb-4">趣味・スポーツ・ボランティア・学習等の行動者率。地域住民の志向性・価値観を把握し、職場文化の設計や福利厚生施策の参考に。</p>
        <div class="echart" style="height:300px;" data-chart-config='{chart_config}'></div>"#,
        pref_label = escape_html(pref_label),
        chart_config = chart_config.replace('\'', "&#39;"),
    ));

    // 詳細テーブル（サブカテゴリあり）
    html.push_str(r#"<table class="min-w-full text-sm mt-4"><thead><tr class="text-slate-400 border-b border-slate-700"><th class="text-left py-1 pr-3">カテゴリ</th><th class="text-left py-1 pr-3">サブカテゴリ</th><th class="text-right py-1">行動者率</th></tr></thead><tbody>"#);
    for row in data {
        let cat = escape_html(&get_str(row, "category"));
        let sub = escape_html(&get_str(row, "subcategory"));
        let rate = get_f64(row, "participation_rate");
        html.push_str(&format!(
            r#"<tr class="border-b border-slate-800 hover:bg-slate-800/30"><td class="py-1 pr-3 text-slate-400 text-xs">{cat}</td><td class="py-1 pr-3 text-slate-300">{sub}</td><td class="py-1 text-right text-slate-200">{rate:.1}%</td></tr>"#,
        ));
    }
    html.push_str(r#"</tbody></table>"#);
    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 社会生活基本調査2021年（総務省統計局） ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

/// 日銀短観DIセクション（時系列ラインチャート）
fn render_boj_tankan_section(data: &[Row]) -> String {
    if data.is_empty() {
        return String::new();
    }

    // 製造業/非製造業 × 業況DI/雇用人員DIをライングラフで表示
    // survey_date でソートされた全国データ
    // di_type: "business" or "employment"
    // industry_code: "製造業" / "非製造業" など主要カテゴリを抽出

    // 日付一覧（ユニーク、昇順）
    let mut dates: Vec<String> = data
        .iter()
        .map(|r| get_str(r, "survey_date").to_string())
        .collect::<std::collections::HashSet<_>>()
        .into_iter()
        .collect();
    dates.sort();
    dates.dedup();

    // 主要産業コードのみ抽出（industry_j で製造業・非製造業を識別）
    let target_industries = ["製造業", "非製造業"];
    let target_di_types = ["business", "employment"];

    // (industry_j, di_type) → 時系列データのマップ
    let mut series_map: std::collections::HashMap<
        (String, String),
        std::collections::HashMap<String, f64>,
    > = std::collections::HashMap::new();

    for row in data {
        let industry_j = get_str(row, "industry_j").to_string();
        let di_type = get_str(row, "di_type").to_string();
        let survey_date = get_str(row, "survey_date").to_string();
        let di_value = get_f64(row, "di_value");

        // 対象産業・対象DI種別のみ収集
        if target_industries.iter().any(|t| industry_j.contains(t))
            && target_di_types.contains(&di_type.as_str())
        {
            series_map
                .entry((industry_j, di_type))
                .or_default()
                .insert(survey_date, di_value);
        }
    }

    if series_map.is_empty() || dates.is_empty() {
        return String::new();
    }

    // シリーズ定義（色とラベル）
    let series_defs = [
        (
            ("製造業".to_string(), "business".to_string()),
            "製造業 業況DI",
            "#3b82f6",
        ),
        (
            ("非製造業".to_string(), "business".to_string()),
            "非製造業 業況DI",
            "#22c55e",
        ),
        (
            ("製造業".to_string(), "employment".to_string()),
            "製造業 雇用人員DI",
            "#f59e0b",
        ),
        (
            ("非製造業".to_string(), "employment".to_string()),
            "非製造業 雇用人員DI",
            "#ec4899",
        ),
    ];

    let dates_json: Vec<String> = dates
        .iter()
        .map(|d| format!("\"{}\"", escape_html(d)))
        .collect();

    let mut series_json_list = Vec::new();
    for (key, label, color) in &series_defs {
        if let Some(date_map) = series_map.get(key) {
            let values: Vec<String> = dates
                .iter()
                .map(|d| {
                    if let Some(&v) = date_map.get(d.as_str()) {
                        format!("{:.1}", v)
                    } else {
                        "null".to_string()
                    }
                })
                .collect();
            series_json_list.push(format!(
                r#"{{"name":"{label}","type":"line","data":[{vals}],"smooth":true,"itemStyle":{{"color":"{color}"}},"lineStyle":{{"width":2}}}}"#,
                vals = values.join(","),
            ));
        }
    }

    if series_json_list.is_empty() {
        return String::new();
    }

    let chart_config = format!(
        r##"{{"tooltip":{{"trigger":"axis"}},"legend":{{"bottom":0,"textStyle":{{"color":"#94a3b8","fontSize":10}}}},"grid":{{"left":"5%","right":"3%","top":"5%","bottom":"15%","containLabel":true}},"xAxis":{{"type":"category","data":[{dates}],"axisLabel":{{"color":"#94a3b8","rotate":45,"fontSize":10}}}},"yAxis":{{"type":"value","axisLabel":{{"color":"#94a3b8"}},"splitLine":{{"lineStyle":{{"color":"#334155"}}}}}},"series":[{series}]}}"##,
        dates = dates_json.join(","),
        series = series_json_list.join(","),
    );

    let mut html = String::with_capacity(4_000);
    html.push_str(&format!(
        r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📈 業況判断DI（日銀短観）</h3>
        <p class="text-xs text-slate-500 mb-4">製造業・非製造業の業況DIと雇用人員DIの時系列推移。DIがプラスであれば「良い」超、マイナスは「悪い」超。採用タイミングの判断や競合企業の採用活況を把握する参考指標。</p>
        <div class="echart" style="height:300px;" data-chart-config='{chart_config}'></div>"#,
        chart_config = chart_config.replace('\'', "&#39;"),
    ));

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 日銀短観（全国企業短期経済観測調査）日本銀行 ※外部統計データ</p>"#);
    html.push_str("</div>");
    html
}

// ======== ユニットテスト: Phase 4-7 新外部データセクション ========

#[cfg(test)]
mod new_section_tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    // -------- テストヘルパー --------

    /// 文字列ペアから Row (HashMap<String, Value>) を生成する
    fn make_row(pairs: &[(&str, &str)]) -> Row {
        let mut map = HashMap::new();
        for &(k, v) in pairs {
            map.insert(k.to_string(), Value::String(v.to_string()));
        }
        map
    }

    /// 整数値を持つキーを追加した Row を生成する
    fn make_row_with_int(pairs: &[(&str, &str)], int_pairs: &[(&str, i64)]) -> Row {
        let mut map = make_row(pairs);
        for &(k, v) in int_pairs {
            map.insert(k.to_string(), Value::from(v));
        }
        map
    }

    /// f64 値を持つキーを追加した Row を生成する
    fn make_row_with_float(pairs: &[(&str, &str)], float_pairs: &[(&str, f64)]) -> Row {
        let mut map = make_row(pairs);
        for &(k, v) in float_pairs {
            map.insert(
                k.to_string(),
                Value::Number(serde_json::Number::from_f64(v).unwrap()),
            );
        }
        map
    }

    // ======================================================
    // 1. render_education_section
    // ======================================================

    /// 空データ → 空文字列を返す（境界条件）
    #[test]
    fn test_education_empty_returns_empty() {
        let result = render_education_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// total_count が全て 0 → 空文字列を返す
    #[test]
    fn test_education_all_zero_count_returns_empty() {
        let row = make_row_with_int(
            &[("education_level", "大学")],
            &[("total_count", 0), ("male_count", 0), ("female_count", 0)],
        );
        let result = render_education_section(&[row], "東京都");
        assert!(result.is_empty(), "total_count=0では空文字列を返すべき");
    }

    /// モックデータ → 学歴レベル文字列がHTMLに含まれる（逆証明: 含まれない場合は失敗）
    #[test]
    fn test_education_contains_level_value() {
        let row = make_row_with_int(
            &[("education_level", "大学院")],
            &[
                ("total_count", 500000),
                ("male_count", 300000),
                ("female_count", 200000),
            ],
        );
        let html = render_education_section(&[row], "東京都");
        assert!(html.contains("大学院"), "学歴レベルがHTMLに含まれるべき: {}", &html[..html.len().min(500)]);
    }

    /// 都道府県ラベルがHTMLに含まれる
    #[test]
    fn test_education_contains_pref_label() {
        let row = make_row_with_int(
            &[("education_level", "高校")],
            &[
                ("total_count", 1000000),
                ("male_count", 500000),
                ("female_count", 500000),
            ],
        );
        let html = render_education_section(&[row], "大阪府");
        assert!(html.contains("大阪府"), "都道府県ラベルがHTMLに含まれるべき");
    }

    /// ECharts クラスとデータ属性がHTMLに含まれる
    #[test]
    fn test_education_contains_echart_class() {
        let row = make_row_with_int(
            &[("education_level", "専門学校")],
            &[
                ("total_count", 200000),
                ("male_count", 100000),
                ("female_count", 100000),
            ],
        );
        let html = render_education_section(&[row], "愛知県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsの class='echart' と data-chart-config 属性が含まれるべき"
        );
    }

    /// 3桁区切りフォーマットで数値が表示される
    #[test]
    fn test_education_number_formatting() {
        let row = make_row_with_int(
            &[("education_level", "大学")],
            &[
                ("total_count", 3102649),
                ("male_count", 1600000),
                ("female_count", 1502649),
            ],
        );
        let html = render_education_section(&[row], "東京都");
        // format_number により「3,102,649」形式になる
        assert!(html.contains("3,102,649"), "total_countが3桁区切りでフォーマットされるべき: actual html snippets = {}", &html[..html.len().min(1000)]);
    }

    // ======================================================
    // 2. render_household_type_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_household_empty_returns_empty() {
        let result = render_household_type_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 世帯類型名がHTMLに含まれる
    #[test]
    fn test_household_contains_type_name() {
        let row = make_row_with_float(
            &[("household_type", "単独世帯")],
            &[("ratio", 35.2)],
        );
        let row = {
            let mut r = row;
            r.insert("count".to_string(), Value::from(5000000_i64));
            r
        };
        let html = render_household_type_section(&[row], "東京都");
        assert!(html.contains("単独世帯"), "世帯類型名がHTMLに含まれるべき");
    }

    /// EChartsドーナツチャートのマーカーが含まれる
    #[test]
    fn test_household_contains_pie_chart() {
        let mut row = make_row(&[("household_type", "核家族世帯")]);
        row.insert("count".to_string(), Value::from(3000000_i64));
        row.insert(
            "ratio".to_string(),
            Value::Number(serde_json::Number::from_f64(50.0).unwrap()),
        );
        let html = render_household_type_section(&[row], "神奈川県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsドーナツチャートが含まれるべき"
        );
    }

    /// 世帯数が3桁区切りでHTMLに含まれる
    #[test]
    fn test_household_count_formatted() {
        let mut row = make_row(&[("household_type", "単独世帯")]);
        row.insert("count".to_string(), Value::from(1234567_i64));
        row.insert(
            "ratio".to_string(),
            Value::Number(serde_json::Number::from_f64(25.0).unwrap()),
        );
        let html = render_household_type_section(&[row], "大阪府");
        assert!(html.contains("1,234,567"), "世帯数が3桁区切りでフォーマットされるべき");
    }

    // ======================================================
    // 3. render_foreign_residents_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_foreign_empty_returns_empty() {
        let result = render_foreign_residents_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 在留資格名がHTMLに含まれる
    #[test]
    fn test_foreign_contains_visa_status() {
        let row = make_row_with_int(
            &[("visa_status", "技術・人文知識・国際業務"), ("survey_period", "2023年")],
            &[("count", 987654)],
        );
        let html = render_foreign_residents_section(&[row], "東京都");
        assert!(
            html.contains("技術・人文知識・国際業務"),
            "在留資格名がHTMLに含まれるべき"
        );
    }

    /// 調査時点がHTMLに含まれる
    #[test]
    fn test_foreign_contains_survey_period() {
        let row = make_row_with_int(
            &[("visa_status", "永住者"), ("survey_period", "2022年6月末")],
            &[("count", 100000)],
        );
        let html = render_foreign_residents_section(&[row], "愛知県");
        assert!(html.contains("2022年6月末"), "調査時点がHTMLに含まれるべき");
    }

    /// 人数が3桁区切りでHTMLに含まれる
    #[test]
    fn test_foreign_count_formatted() {
        let row = make_row_with_int(
            &[("visa_status", "留学"), ("survey_period", "2023年")],
            &[("count", 1234567)],
        );
        let html = render_foreign_residents_section(&[row], "大阪府");
        assert!(html.contains("1,234,567"), "人数が3桁区切りでフォーマットされるべき");
    }

    // ======================================================
    // 4. render_land_price_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_land_price_empty_returns_empty() {
        let result = render_land_price_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 住宅地データ → 住宅地ラベルがHTMLに含まれる
    #[test]
    fn test_land_price_contains_residential_label() {
        let row = make_row_with_float(
            &[("land_use", "住宅地")],
            &[("avg_price_per_sqm", 250000.0), ("yoy_change_pct", 2.5)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(500_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        assert!(html.contains("住宅地"), "住宅地ラベルがHTMLに含まれるべき");
    }

    /// 前年比がプラスのとき緑色コードが含まれる
    #[test]
    fn test_land_price_positive_yoy_green_color() {
        let row = make_row_with_float(
            &[("land_use", "商業地")],
            &[("avg_price_per_sqm", 3000000.0), ("yoy_change_pct", 5.0)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(200_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        // プラスYoY → 緑色 (#22c55e)
        assert!(html.contains("#22c55e"), "前年比プラス時は緑色コードが含まれるべき");
    }

    /// 前年比がマイナスのとき赤色コードが含まれる
    #[test]
    fn test_land_price_negative_yoy_red_color() {
        let row = make_row_with_float(
            &[("land_use", "工業地")],
            &[("avg_price_per_sqm", 50000.0), ("yoy_change_pct", -1.5)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(100_i64));
            r
        };
        let html = render_land_price_section(&[row], "北海道");
        // マイナスYoY → 赤色 (#ef4444)
        assert!(html.contains("#ef4444"), "前年比マイナス時は赤色コードが含まれるべき");
    }

    /// マッチしない用途名 → 「データなし」が含まれる
    #[test]
    fn test_land_price_no_matching_land_use_shows_no_data() {
        // 「その他」という用途名は住宅地・商業地・工業地のいずれにも一致しない
        let row = make_row_with_float(
            &[("land_use", "その他")],
            &[("avg_price_per_sqm", 10000.0), ("yoy_change_pct", 0.0)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(10_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        assert!(html.contains("データなし"), "マッチしない用途名の場合「データなし」が表示されるべき");
    }

    // ======================================================
    // 5. render_regional_infra_section
    // ======================================================

    /// 両方空 → 空文字列を返す
    #[test]
    fn test_regional_infra_both_empty_returns_empty() {
        let result = render_regional_infra_section(&[], &[], "東京都");
        assert!(result.is_empty(), "car_data・net_data両方空では空文字列を返すべき");
    }

    /// car_dataのみあり → 自動車保有率KPIが含まれる
    #[test]
    fn test_regional_infra_car_data_only_renders() {
        let row = make_row_with_float(&[], &[("cars_per_100people", 75.5)]);
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[row], &[], "群馬県");
        assert!(html.contains("自動車保有率"), "自動車保有率KPIが含まれるべき");
        assert!(html.contains("群馬県"), "都道府県ラベルが含まれるべき");
    }

    /// 高い自動車保有率（≥70）→ 緑色コードが含まれる
    #[test]
    fn test_regional_infra_car_high_rate_green() {
        let mut row = HashMap::new();
        row.insert(
            "cars_per_100people".to_string(),
            Value::Number(serde_json::Number::from_f64(80.0).unwrap()),
        );
        row.insert("year".to_string(), Value::from(2022_i64));
        let html = render_regional_infra_section(&[row], &[], "栃木県");
        assert!(html.contains("#22c55e"), "保有率≥70の場合は緑色コードが含まれるべき");
    }

    /// net_dataのみあり → インターネット利用率KPIが含まれる
    #[test]
    fn test_regional_infra_net_data_only_renders() {
        let row = make_row_with_float(
            &[],
            &[("internet_usage_rate", 85.0), ("smartphone_ownership_rate", 78.0)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[], &[row], "神奈川県");
        assert!(html.contains("インターネット利用率"), "インターネット利用率KPIが含まれるべき");
    }

    /// 両方あり → 両方のKPIが含まれる
    #[test]
    fn test_regional_infra_both_data_renders_both_kpis() {
        let car_row = {
            let mut r = make_row_with_float(&[], &[("cars_per_100people", 65.0)]);
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let net_row = {
            let mut r = make_row_with_float(
                &[],
                &[("internet_usage_rate", 82.0), ("smartphone_ownership_rate", 75.0)],
            );
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[car_row], &[net_row], "埼玉県");
        assert!(html.contains("自動車保有率"), "自動車保有率KPIが含まれるべき");
        assert!(html.contains("インターネット利用率"), "インターネット利用率KPIが含まれるべき");
    }

    // ======================================================
    // 6. render_social_life_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_social_life_empty_returns_empty() {
        let result = render_social_life_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// カテゴリ名がHTMLに含まれる
    #[test]
    fn test_social_life_contains_category() {
        let row = make_row_with_float(
            &[("category", "スポーツ"), ("subcategory", "ジョギング・マラソン")],
            &[("participation_rate", 42.5)],
        );
        let html = render_social_life_section(&[row], "東京都");
        assert!(html.contains("スポーツ"), "カテゴリ名がHTMLに含まれるべき");
    }

    /// サブカテゴリ名がHTMLに含まれる
    #[test]
    fn test_social_life_contains_subcategory() {
        let row = make_row_with_float(
            &[("category", "趣味・娯楽"), ("subcategory", "読書")],
            &[("participation_rate", 60.0)],
        );
        let html = render_social_life_section(&[row], "京都府");
        assert!(html.contains("読書"), "サブカテゴリ名がHTMLに含まれるべき");
    }

    /// EChartsレーダーチャートのマーカーが含まれる
    #[test]
    fn test_social_life_contains_radar_chart() {
        let row = make_row_with_float(
            &[("category", "ボランティア"), ("subcategory", "地域行事")],
            &[("participation_rate", 25.3)],
        );
        let html = render_social_life_section(&[row], "兵庫県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsレーダーチャートが含まれるべき"
        );
    }

    /// 行動者率の具体値がHTMLに含まれる（逆証明: 存在チェックではなく値検証）
    #[test]
    fn test_social_life_participation_rate_in_html() {
        let row = make_row_with_float(
            &[("category", "学習・自己啓発"), ("subcategory", "外国語")],
            &[("participation_rate", 12.3)],
        );
        let html = render_social_life_section(&[row], "福岡県");
        assert!(html.contains("12.3"), "行動者率の具体値がHTMLに含まれるべき");
    }

    // ======================================================
    // 7. render_boj_tankan_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_boj_tankan_empty_returns_empty() {
        let result = render_boj_tankan_section(&[]);
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 対象外産業・対象外DI種別のみ → series_map空 → 空文字列を返す
    #[test]
    fn test_boj_tankan_non_target_industry_returns_empty() {
        // 「その他産業」はtarget_industriesに含まれないのでseries_mapは空になる
        let row = make_row_with_float(
            &[
                ("industry_j", "その他産業"),
                ("di_type", "business_condition"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", 10.0)],
        );
        let result = render_boj_tankan_section(&[row]);
        assert!(result.is_empty(), "対象外産業のみでは空文字列を返すべき");
    }

    /// 製造業 業況DI → EChartsチャートとタイトルが含まれる
    #[test]
    fn test_boj_tankan_manufacturing_business_condition_renders() {
        let row = make_row_with_float(
            &[
                ("industry_j", "製造業"),
                ("di_type", "business_condition"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", 15.0)],
        );
        let html = render_boj_tankan_section(&[row]);
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsチャートが含まれるべき"
        );
        assert!(html.contains("業況判断DI"), "タイトル「業況判断DI」が含まれるべき");
    }

    /// 非製造業 雇用人員DI → シリーズ名がchart_configに含まれる
    #[test]
    fn test_boj_tankan_non_manufacturing_employment_excess_renders() {
        let row = make_row_with_float(
            &[
                ("industry_j", "非製造業"),
                ("di_type", "employment_excess"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", -5.0)],
        );
        let html = render_boj_tankan_section(&[row]);
        // chart_config内に「非製造業 雇用人員DI」シリーズ名が含まれる
        assert!(
            html.contains("非製造業 雇用人員DI"),
            "非製造業 雇用人員DIのシリーズ名がHTMLに含まれるべき"
        );
    }

    /// 複数日付 → 時系列が昇順ソートされてchart_configに含まれる
    #[test]
    fn test_boj_tankan_multiple_dates_sorted() {
        let rows = vec![
            make_row_with_float(
                &[
                    ("industry_j", "製造業"),
                    ("di_type", "business_condition"),
                    ("survey_date", "2024Q3"),
                ],
                &[("di_value", 12.0)],
            ),
            make_row_with_float(
                &[
                    ("industry_j", "製造業"),
                    ("di_type", "business_condition"),
                    ("survey_date", "2024Q1"),
                ],
                &[("di_value", 8.0)],
            ),
        ];
        let html = render_boj_tankan_section(&rows);
        // 昇順ソート後は 2024Q1 が 2024Q3 より前に出現するはず
        let pos_q1 = html.find("2024Q1").expect("2024Q1がHTMLに含まれるべき");
        let pos_q3 = html.find("2024Q3").expect("2024Q3がHTMLに含まれるべき");
        assert!(
            pos_q1 < pos_q3,
            "調査日付は昇順ソートされて出力されるべき (2024Q1 < 2024Q3)"
        );
    }
}
