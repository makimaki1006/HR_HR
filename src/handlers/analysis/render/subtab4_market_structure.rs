//! 分割: render/subtab4_market_structure.rs (物理移動・内容変更なし)

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{
    cross_nav, escape_html, format_number, get_f64, get_i64, get_str_html, pct, pct_bar,
    truncate_str,
};
use super::super::fetch::*;
use super::super::helpers::{concentration_badge, get_str, strategy_color, vacancy_color};

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

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

pub(super) fn render_employer_strategy_section(data: &[Row]) -> String {
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

pub(super) fn render_monopsony_section(data: &[Row]) -> String {
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

pub(super) fn render_spatial_mismatch_section(data: &[Row]) -> String {
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

pub(super) fn render_competition_section(data: &[Row]) -> String {
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

pub(super) fn render_cascade_section(data: &[Row]) -> String {
    let mut html = String::new();
    html.push_str(r#"<div class="stat-card">
        <h3 class="text-sm text-slate-400 mb-1">カスケード分析（業種別総合）</h3>
        <p class="text-xs text-slate-500 mb-4">業種×雇用形態ごとの求人数・給与・休日・欠員補充率を一覧比較。人材争奪の全体像を把握できます。</p>
        <div style="overflow-x:auto;"><table class="data-table text-xs">
        <thead><tr><th>業種</th><th class="text-center">雇用形態</th><th class="text-right">求人数</th><th class="text-right">施設数</th><th class="text-right">平均給与</th><th class="text-right">年間休日</th><th class="text-right">欠員補充率</th><th style="width:80px"></th></tr></thead><tbody>"#);

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
