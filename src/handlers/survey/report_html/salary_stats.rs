//! 分割: report_html/salary_stats.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

use super::helpers::*;

pub(super) fn render_section_salary_stats(
    html: &mut String,
    agg: &SurveyAggregation,
    salary_min_values: &[i64],
    salary_max_values: &[i64],
) {
    let stats = match &agg.enhanced_stats {
        Some(s) => s,
        None => return, // 給与データなし → セクションスキップ
    };

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>給与分布 - 統計情報</h2>\n");

    // セクション冒頭ガイド
    render_section_howto(
        html,
        &[
            "中央値は外れ値の影響を受けにくいロバスト指標。実態は中央値で読む",
            "ヒストグラムの縦線は赤=平均、緑=中央値、紫=最頻値。3 線が近接していれば分布は対称",
            "IQR バー（Q1-Q3）の幅が広いほど給与レンジのばらつきが大きい",
        ],
    );

    // 3カード: 平均、中央値、給与範囲
    html.push_str("<div class=\"stats-grid\">\n");
    render_stat_box(html, "平均月給", &format_man_yen(stats.mean));
    render_stat_box(html, "中央値", &format_man_yen(stats.median));
    render_stat_box(
        html,
        "給与範囲",
        &format!(
            "{} 〜 {}",
            format_man_yen(stats.min),
            format_man_yen(stats.max)
        ),
    );
    html.push_str("</div>\n");

    // 図表番号: 表 3-1 給与統計サマリ（標準偏差、信頼性区分など含む詳細表）
    render_figure_caption(html, "表 3-1", "給与統計サマリ（外れ値除外後）");
    html.push_str("<table class=\"sortable-table zebra\">\n");
    html.push_str("<thead><tr><th>統計指標</th><th style=\"text-align:right\">値</th><th>備考</th></tr></thead>\n<tbody>\n");
    html.push_str(&format!(
        "<tr><td>件数 (n)</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">外れ値除外後の有効サンプル</td></tr>\n",
        format_number(stats.count as i64)
    ));
    html.push_str(&format!(
        "<tr><td>平均月給</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">外れ値の影響を受けやすい</td></tr>\n",
        format_man_yen(stats.mean)
    ));
    html.push_str(&format!(
        "<tr><td>中央値</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">ロバスト指標（推奨）</td></tr>\n",
        format_man_yen(stats.median)
    ));
    html.push_str(&format!(
        "<tr><td>標準偏差</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">ばらつきの目安</td></tr>\n",
        format_man_yen(stats.std_dev)
    ));
    html.push_str(&format!(
        "<tr><td>給与範囲</td><td class=\"num\">{} 〜 {}</td><td style=\"font-size:9pt;color:#666\">最小〜最大</td></tr>\n",
        format_man_yen(stats.min),
        format_man_yen(stats.max),
    ));
    if let Some(q) = &stats.quartiles {
        html.push_str(&format!(
            "<tr><td>第1四分位 (Q1)</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">下位 25% の境界</td></tr>\n",
            format_man_yen(q.q1)
        ));
        html.push_str(&format!(
            "<tr><td>第3四分位 (Q3)</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">高い側 25% の境界</td></tr>\n",
            format_man_yen(q.q3)
        ));
        html.push_str(&format!(
            "<tr><td>四分位範囲 (IQR)</td><td class=\"num\">{}</td><td style=\"font-size:9pt;color:#666\">中央 50% のレンジ幅</td></tr>\n",
            format_man_yen(q.iqr)
        ));
    }
    html.push_str("</tbody></table>\n");

    // IQR シェード補助バー（Q1-Q3 ハイライト + 中央値マーカー）
    if let Some(q) = &stats.quartiles {
        if stats.max > stats.min && q.q3 > q.q1 {
            let total_range = (stats.max - stats.min).max(1) as f64;
            let q1_pct = ((q.q1 - stats.min).max(0) as f64 / total_range * 100.0).clamp(0.0, 100.0);
            let q3_pct = ((q.q3 - stats.min).max(0) as f64 / total_range * 100.0).clamp(0.0, 100.0);
            let med_pct =
                ((stats.median - stats.min).max(0) as f64 / total_range * 100.0).clamp(0.0, 100.0);
            let shade_w = (q3_pct - q1_pct).max(2.0);
            render_figure_caption(
                html,
                "図 3-1",
                "IQR シェード（Q1-Q3 中央 50% レンジ + 中央値マーカー）",
            );
            html.push_str("<div class=\"iqr-bar\" aria-label=\"四分位範囲シェード\">\n");
            html.push_str(&format!(
                "<div class=\"iqr-shade\" style=\"left:{:.1}%;width:{:.1}%;\"></div>\n",
                q1_pct, shade_w
            ));
            html.push_str(&format!(
                "<div class=\"iqr-median\" style=\"left:{:.1}%;\"></div>\n",
                med_pct
            ));
            html.push_str("</div>\n");
            html.push_str(&format!(
                "<div class=\"iqr-bar-legend\"><span>min: {}</span>\
                 <span>Q1: {}</span><span>中央値: {}</span>\
                 <span>Q3: {}</span><span>max: {}</span></div>\n",
                format_man_yen(stats.min),
                format_man_yen(q.q1),
                format_man_yen(stats.median),
                format_man_yen(q.q3),
                format_man_yen(stats.max),
            ));
            render_read_hint(
                html,
                "青のシェードが Q1-Q3（中央 50%）。緑の縦線が中央値。シェード幅が狭いほど給与水準は揃っており、広いほど雇用形態や経験年数による分散が大きい傾向です。",
            );
        }
    }

    // 外れ値除外の前後比較（IQR 法による除外結果）
    if agg.outliers_removed_total > 0 || agg.salary_values_raw_count > 0 {
        let raw = agg.salary_values_raw_count;
        let removed = agg.outliers_removed_total;
        let kept = raw.saturating_sub(removed);
        let removed_pct = if raw > 0 {
            removed as f64 / raw as f64 * 100.0
        } else {
            0.0
        };
        render_figure_caption(html, "表 3-2", "外れ値除外の前後比較（IQR 法）");
        html.push_str("<table class=\"sortable-table zebra\">\n");
        html.push_str("<thead><tr><th>区分</th><th style=\"text-align:right\">件数</th><th style=\"text-align:right\">割合</th></tr></thead>\n<tbody>\n");
        html.push_str(&format!(
            "<tr><td>除外前（raw）</td><td class=\"num\">{}件</td><td class=\"num\">100.0%</td></tr>\n",
            format_number(raw as i64)
        ));
        html.push_str(&format!(
            "<tr><td>除外後（採用）</td><td class=\"num\">{}件</td><td class=\"num\">{:.1}%</td></tr>\n",
            format_number(kept as i64),
            100.0 - removed_pct
        ));
        html.push_str(&format!(
            "<tr><td>外れ値（除外）</td><td class=\"num negative\">{}件</td><td class=\"num\">{:.1}%</td></tr>\n",
            format_number(removed as i64),
            removed_pct
        ));
        html.push_str("</tbody></table>\n");
        render_read_hint(
            html,
            "外れ値除外により集計は安定しますが、除外件数が 10% を超える場合はデータ品質や雇用形態混在の可能性を確認してください。",
        );
    }

    // 信頼区間・四分位がある場合
    if let Some(ci) = &stats.bootstrap_ci {
        html.push_str(&format!(
            "<p class=\"note\">95%信頼区間: {} 〜 {} (Bootstrap法, n={})</p>\n",
            format_man_yen(ci.lower),
            format_man_yen(ci.upper),
            ci.sample_size
        ));
    }

    // 下限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_min_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<div class=\"salary-chart-block\">\n");
        html.push_str("<h3>下限給与の分布（20,000円刻み）</h3>\n");
        render_figure_caption(
            html,
            "図 3-2",
            "下限月給ヒストグラム（20,000円刻み・縦線=平均/中央値/最頻値）",
        );
        let (labels, values, _b) = build_salary_histogram(salary_min_values, 20_000);
        let mode_min_20k = compute_mode(salary_min_values, 20_000);
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_20k,
            20_000,
        );
        html.push_str(&render_echart_div(&config, 220));
        html.push_str("</div>\n");

        // 詳細分布（5,000円刻み）
        html.push_str("<div class=\"salary-chart-block\">\n");
        html.push_str("<h3>下限給与の分布（5,000円刻み）- 詳細</h3>\n");
        render_figure_caption(
            html,
            "図 3-3",
            "下限月給ヒストグラム（5,000円刻み・微細解像度）",
        );
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_min_values, 5_000);
        let mode_min_5k = compute_mode(salary_min_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#42A5F5",
            Some(stats.mean),
            Some(stats.median),
            mode_min_5k,
            5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
        html.push_str("</div>\n");
        render_read_hint(
            html,
            "20,000円刻みは全体傾向の把握に、5,000円刻みは「ちょうど 25 万円」「20 万円ちょうど」など切り良い設定への偏在を観察するのに適しています。",
        );
    }

    // 上限給与ヒストグラム（ECharts棒グラフ + markLine: 平均/中央値/最頻値）
    if !salary_max_values.is_empty() {
        // 生値分布（20,000円刻み）
        html.push_str("<div class=\"salary-chart-block salary-chart-page-start\">\n");
        html.push_str("<h3>上限給与の分布（20,000円刻み）</h3>\n");
        render_figure_caption(
            html,
            "図 3-4",
            "上限月給ヒストグラム（20,000円刻み・縦線=平均/中央値/最頻値）",
        );
        let (labels, values, _b) = build_salary_histogram(salary_max_values, 20_000);
        let mode_max_20k = compute_mode(salary_max_values, 20_000);
        // 図 3-4 は下限側の 20,000 円刻みヒストグラムと凡例表現を揃える。
        // 近接時の右上統計カードに切り替えると、この図だけ見た目が変わるため無効化する。
        let config = build_histogram_echart_config_with_stats_card(
            &labels,
            &values,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_20k,
            20_000,
            false,
        );
        html.push_str(&render_echart_div(&config, 220));
        html.push_str("</div>\n");

        // 詳細分布（5,000円刻み）
        html.push_str("<div class=\"salary-chart-block\">\n");
        html.push_str("<h3>上限給与の分布（5,000円刻み）- 詳細</h3>\n");
        render_figure_caption(
            html,
            "図 3-5",
            "上限月給ヒストグラム（5,000円刻み・微細解像度）",
        );
        let (labels_f, values_f, _bf) = build_salary_histogram(salary_max_values, 5_000);
        let mode_max_5k = compute_mode(salary_max_values, 5_000);
        let config = build_histogram_echart_config(
            &labels_f,
            &values_f,
            "#66BB6A",
            Some(stats.mean),
            Some(stats.median),
            mode_max_5k,
            5_000,
        );
        html.push_str(&render_echart_div(&config, 220));
        html.push_str("</div>\n");
    }

    render_section_bridge(
        html,
        "次セクションでは、下限と上限の関係を散布図と回帰線で確認します。\
         レンジの開き方が大きい（傾き > 1.3）場合は「経験・資格による給与差を強く打ち出す求人が多い」、\
         開き方が狭い（傾き < 1.1）場合は「給与水準が一律でスキル評価が反映されにくい」傾向を示唆します。\
         この後の採用市場逼迫度セクションでは、こうした給与構造を踏まえた「採用難易度」を確認します。",
    );

    html.push_str("</div>\n");
}
