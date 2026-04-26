//! 分割: render/subtab6_forecast.rs (物理移動・内容変更なし)

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{format_number, get_f64, get_i64};
use super::super::fetch::*;
use super::super::helpers::{get_str};

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;


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

pub(super) fn render_fulfillment_section(data: &[Row]) -> String {
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

pub(super) fn render_mobility_section(data: &[Row]) -> String {
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

pub(super) fn render_shadow_wage_section(data: &[Row]) -> String {
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
