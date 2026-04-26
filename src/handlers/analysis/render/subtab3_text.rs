//! 分割: render/subtab3_text.rs (物理移動・内容変更なし)

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::{
    escape_html, format_number, get_f64, get_i64, pct, pct_bar,
};
use super::super::fetch::*;
use super::super::helpers::{
    get_str, info_score_color, keyword_category_color, keyword_category_label, temp_color,
};

#[allow(dead_code)]
type Db = crate::db::local_sqlite::LocalDb;
#[allow(dead_code)]
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;


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

pub(super) fn render_text_quality_section(data: &[Row]) -> String {
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

pub(super) fn render_keyword_profile_section(data: &[Row]) -> String {
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

pub(super) fn render_temperature_section(data: &[Row]) -> String {
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
