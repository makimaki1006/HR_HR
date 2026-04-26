//! 分割: render/subtab1_recruit_trend.rs (物理移動・内容変更なし)

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{
    cross_nav, format_number, get_f64, get_i64, get_str_html, pct, pct_bar, truncate_str,
};
use super::super::fetch::*;
use super::super::helpers::{evenness_color, get_str, transparency_color, vacancy_color};

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;


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

pub(super) fn render_vacancy_section(data: &[Row], by_industry: &[Row]) -> String {
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
            r##"<div class="echart" style="height:250px;" data-chart-config='{{"tooltip":{{"trigger":"axis","axisPointer":{{"type":"shadow"}}}},"legend":{{"orient":"horizontal","bottom":0,"textStyle":{{"color":"#94a3b8","fontSize":11}}}},"grid":{{"left":"20%","right":"5%","top":"10%","bottom":"15%"}},"xAxis":{{"type":"value","axisLabel":{{"formatter":"{{value}}%","color":"#94a3b8"}}}},"yAxis":{{"type":"category","data":[{labels}],"axisLabel":{{"color":"#94a3b8"}}}},"series":[{{"name":"欠員補充率","type":"bar","data":[{vr}],"itemStyle":{{"color":"#ef4444"}}}},{{"name":"増員率","type":"bar","data":[{gr}],"itemStyle":{{"color":"#22c55e"}}}}]}}'></div>"##,
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
            <thead><tr><th>業種</th><th class="text-center">雇用形態</th><th class="text-right">件数</th><th class="text-right">欠員補充率</th><th class="text-right">増員率</th><th style="width:100px"></th></tr></thead><tbody>"#);

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

pub(super) fn render_resilience_section(data: &[Row]) -> String {
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

pub(super) fn render_transparency_section(data: &[Row]) -> String {
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
