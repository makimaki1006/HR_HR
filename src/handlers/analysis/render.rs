//! HTML描画関数（全 render_* 関数 + render_subtab_1..6）

use std::collections::HashMap;
use serde_json::Value;

use super::super::helpers::{cross_nav, get_f64, get_i64, get_str_html, escape_html, format_number, pct, pct_bar, truncate_str};
use super::helpers::{
    get_str, vacancy_color, transparency_color, evenness_color, temp_color,
    salary_color, rank_badge_color, info_score_color,
    keyword_category_label, keyword_category_color,
    strategy_color, concentration_badge,
};
use super::fetch::*;

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

    let mut html = String::with_capacity(12_000);
    html.push_str(r#"<div class="space-y-6">"#);
    html.push_str(&format!(
        r#"<div class="flex items-center gap-3 text-xs text-slate-500 mb-2">関連: {} {}</div>"#,
        cross_nav("/tab/workstyle", "求人条件の給与帯"),
        cross_nav("/tab/diagnostic", "市場診断ツール"),
    ));

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
    let spatial = if !muni.is_empty() { fetch_spatial_mismatch(db, pref, muni) } else { vec![] };
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
    if employer_strategy.is_empty() && monopsony.is_empty() && spatial.is_empty()
        && competition.is_empty() && cascade.is_empty() {
        html.push_str(r#"<p class="text-slate-500 text-sm">市場構造データがありません</p>"#);
    }

    html.push_str("</div>");
    html
}

/// サブタブ5: 異常値・外部（anomaly, minimum_wage, wage_compliance, prefecture_stats, population, region_benchmark）
pub(crate) fn render_subtab_5(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> String {
    let anomaly = fetch_anomaly_data(db, pref, muni);
    let minimum_wage = fetch_minimum_wage(db, pref);
    let wage_compliance = fetch_wage_compliance(db, pref, muni);
    let prefecture_stats = fetch_prefecture_stats(db, turso, pref);
    let population = fetch_population_data(db, turso, pref, muni);
    let pyramid = fetch_population_pyramid(db, turso, pref, muni);
    let migration = fetch_migration_data(db, turso, pref, muni);
    let daytime = fetch_daytime_population(db, turso, pref, muni);
    let region_benchmark = fetch_region_benchmark(db, pref, muni);

    let mut html = String::with_capacity(32_000);
    html.push_str(r#"<div class="space-y-6">"#);

    if !anomaly.is_empty() {
        html.push_str(&render_anomaly_section(&anomaly));
    }

    let has_external = !minimum_wage.is_empty() || !wage_compliance.is_empty()
        || !prefecture_stats.is_empty() || !population.is_empty() || !region_benchmark.is_empty();

    if has_external {
        html.push_str(r#"<div class="border-t border-slate-700 my-4 pt-4">
            <h3 class="text-lg font-semibold text-slate-300 mb-4">外部データ統合分析</h3></div>"#);
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
    if !population.is_empty() {
        html.push_str(&render_population_section(&population, &pyramid));
    }
    if !migration.is_empty() || !daytime.is_empty() {
        html.push_str(&render_demographics_section(&migration, &daytime));
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
        <p class="text-xs text-slate-500 mb-4">産業の分散度を評価。業界分散度が高いほど特定産業への依存リスクが低い健全な雇用構造です。</p>"#);

    // ECharts レーダーチャート: 4指標 × 雇用形態
    if data.len() > 1 {
        let colors = ["#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#8b5cf6"];
        // 正規化用の最大値を計算
        let max_ind: f64 = data.iter().map(|r| get_f64(r, "industry_count") as f64).fold(1.0_f64, f64::max);
        let max_hhi: f64 = data.iter().map(|r| get_f64(r, "hhi")).fold(1.0_f64, f64::max);

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
            let industry_norm = if max_ind > 0.0 { n_ind / max_ind * 100.0 } else { 0.0 };
            let dispersion_norm = if max_hhi > 0.0 { (1.0 - hhi / max_hhi) * 100.0 } else { 50.0 };
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
        let label = if evenness >= 0.7 { "分散（良好）" } else if evenness >= 0.5 { "やや集中" } else { "集中（リスク）" };
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
        let total = get_i64(row, "sample_count");
        let avg_t = get_f64(row, "temperature");
        let urg = get_f64(row, "urgency_density");
        let sel = get_f64(row, "selectivity_density");
        let tc = temp_color(avg_t);

        let temp_label = if avg_t >= 0.5 { "人手不足（条件緩和）" }
            else if avg_t >= 0.2 { "やや緩和" }
            else if avg_t >= -0.2 { "標準" }
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
        let band = get_str_html(row, "salary_band");
        let edu = get_str_html(row, "education_group");
        let grp = get_str_html(row, "emp_group");
        let n = get_i64(row, "total_postings");
        let ic = get_f64(row, "industry_count");
        let tops = get_str_html(row, "top_industries");
        let tops_short = truncate_str(&tops, 30);

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
        let ind = get_str_html(row, "industry_raw");
        let grp = get_str_html(row, "emp_group");
        let n = get_i64(row, "posting_count");
        let fac = get_i64(row, "facility_count");
        let avg_sal = get_f64(row, "avg_salary_min");
        let holidays = get_f64(row, "avg_annual_holidays");
        let vr = get_f64(row, "vacancy_rate");
        let vc = vacancy_color(vr);
        let ind_short = truncate_str(&ind, 18);

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
        <p class="text-xs text-slate-500 mb-4">地域平均から大きく外れた求人の割合。異常値が多い指標は地域内の格差が大きいことを示します。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>指標</th><th>雇用形態</th><th class="text-right">件数</th><th class="text-right">異常値数</th><th class="text-right">異常率</th><th class="text-right">平均</th><th class="text-right">バラツキ</th><th class="text-right">高異常</th><th class="text-right">低異常</th></tr></thead><tbody>"#);

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
    let mut html = String::with_capacity(5_000);
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">📊 給与競争力指数</h3>
        <p class="text-xs text-slate-500 mb-4">地域の平均給与を全国平均と比較。プラスなら全国より高水準、マイナスなら低水準です。</p>"#);

    // ECharts ゲージチャート: パーセンタイルランク
    if !data.is_empty() {
        let colors = ["#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#8b5cf6"];
        let n = data.len();
        let axis_color_arr = r##"[[0.25,"#ef4444"],[0.5,"#eab308"],[0.75,"#3b82f6"],[1,"#22c55e"]]"##;
        let title_color = "#94a3b8";
        let mut series_json = vec![];
        for (i, row) in data.iter().enumerate() {
            let grp = get_str(row, "emp_group");
            let pctile = get_f64(row, "percentile_rank");
            let c = colors[i % colors.len()];
            let center_x = if n > 1 {
                (100.0 / (n as f64 + 1.0)) * (i as f64 + 1.0)
            } else { 50.0 };
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
        let hhi_color = if hhi >= 2500.0 { "#ef4444" } else if hhi >= 1500.0 { "#f97316" } else if hhi >= 1000.0 { "#eab308" } else { "#22c55e" };

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

    html.push_str(r#"<p class="text-xs text-slate-600 mt-3 italic">出典: 厚生労働省「地域別最低賃金」+ ハローワーク求人データ</p>"#);
    html.push_str("</div></div>");
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
            ("完全失業率", format!("{unemp:.1}%"), "顕在求職者率", "#ef4444"),
            ("転職希望者比率", format!("{desire:.1}%"), "潜在労働力流動性", "#f59e0b"),
            ("非正規雇用比率", format!("{non_reg:.1}%"), "就業構造", "#8b5cf6"),
            ("平均所定内給与", format!("{wage:.0}千円"), "賃金構造基本統計", "#22c55e"),
            ("物価地域差指数", format!("{price:.1}"), "全国=100", "#06b6d4"),
            ("有効求人充足率", format!("{fulfill:.1}%"), "充足数/新規求人", "#3b82f6"),
            ("実質賃金力", format!("{real_wage:.1}千円"), "物価調整後", "#ec4899"),
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
                get_f64(row, "unemployment_rate"), get_f64(row, "job_change_desire_rate"),
                get_f64(row, "non_regular_rate"), get_f64(row, "avg_monthly_wage"),
                get_f64(row, "price_index"), get_f64(row, "fulfillment_rate"),
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
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">👥 人口構成</h3>
        <p class="text-xs text-slate-500 mb-4">住民基本台帳に基づく人口・年齢構成データ。</p>"#);

    if let Some(row) = pop_data.first() {
        let total = get_i64(row, "total_population");
        let male = get_i64(row, "male_population");
        let female = get_i64(row, "female_population");
        let aging = get_f64(row, "aging_rate");
        let working = get_f64(row, "working_age_rate");
        let youth = get_f64(row, "youth_rate");

        // 年齢3区分バー
        let aging_color = if aging >= 35.0 { "#ef4444" } else if aging >= 28.0 { "#f97316" } else { "#22c55e" };

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
        let max_count: i64 = pyramid.iter()
            .map(|r| get_i64(r, "male_count").max(get_i64(r, "female_count")))
            .max().unwrap_or(1);

        html.push_str(r#"<div class="mt-4">
            <div class="text-xs text-slate-400 font-semibold mb-2">人口ピラミッド</div>
            <div class="flex text-xs text-slate-500 mb-1"><span class="w-1/2 text-right pr-1 text-blue-400">男性</span><span class="w-1/2 pl-1 text-pink-400">女性</span></div>"#);

        // 下から上に表示（若い方が下）
        for row in pyramid.iter().rev() {
            let ag = get_str(row, "age_group");
            let m = get_i64(row, "male_count");
            let f = get_i64(row, "female_count");
            let m_pct = if max_count > 0 { m as f64 / max_count as f64 * 100.0 } else { 0.0 };
            let f_pct = if max_count > 0 { f as f64 / max_count as f64 * 100.0 } else { 0.0 };

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
        let label = if ratio >= 100.0 { "昼間流入超過" } else { "昼間流出超過" };

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
        let comp_color = if composite >= 70.0 { "#22c55e" } else if composite >= 50.0 { "#eab308" } else { "#ef4444" };

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
