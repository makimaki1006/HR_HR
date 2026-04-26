//! 分割: render/subtab2_salary.rs (物理移動・内容変更なし)

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{cross_nav, format_number, get_f64, get_i64, pct_bar};
use super::super::fetch::*;
use super::super::helpers::{get_str, rank_badge_color, salary_color};

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

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

pub(super) fn render_salary_structure_section(data: &[Row]) -> String {
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

pub(super) fn render_salary_competitiveness_section(data: &[Row]) -> String {
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

pub(super) fn render_compensation_section(data: &[Row]) -> String {
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
