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

fn distribution_mean(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let sum: i128 = values.iter().map(|v| *v as i128).sum();
    Some((sum / values.len() as i128) as i64)
}

fn distribution_median(values: &[i64]) -> Option<i64> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        Some(((sorted[mid - 1] as i128 + sorted[mid] as i128) / 2) as i64)
    } else {
        Some(sorted[mid])
    }
}

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
                "給与分布 boxplot（min / Q1 / 中央値 / Q3 / max）+ 補助 IQR シェード",
            );
            // Round 17 (2026-05-13): ECharts boxplot → SSR SVG に置換 (print emulate 対応)
            html.push_str(&build_boxplot_svg(stats.min, q.q1, stats.median, q.q3, stats.max));

            html.push_str("<div class=\"iqr-bar\" aria-label=\"四分位範囲シェード (補助)\">\n");
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

    // Round 20 (2026-05-13): 給与クラスタ分析 (docs/salary_cluster_analysis_design.md 準拠)
    // - 固定ビン幅ヒストグラム廃止 → 概観 1 chart (下限/上限 統合 10,000 円刻み) のみ残す
    // - 給与構造クラスタテーブル (3×3 = 最大 9 クラスタ、件数<5 は省略)
    // - クラスタ別 box plot 並列表示
    // - 顧客 (CSV 全体) 中央値の最近傍クラスタ判定 + So-What
    // - 上位 3 クラスタの動的ヒストグラム (Freedman-Diaconis rule)
    // - 上側外れ値求人 / 上限なし求人 別表

    // (1) 概観ヒストグラム: 下限 + 上限を 10,000 円刻みで 2 chart のみ
    if !salary_min_values.is_empty() {
        let min_mean = distribution_mean(salary_min_values);
        let min_median = distribution_median(salary_min_values);
        html.push_str("<div class=\"salary-chart-block\">\n");
        html.push_str("<h3>下限給与の分布（10,000円刻み・概観）</h3>\n");
        render_figure_caption(
            html, "図 3-2",
            "下限月給ヒストグラム（10,000円刻み・縦線=平均/中央値/最頻値）",
        );
        let mode_min = compute_mode(salary_min_values, 10_000);
        html.push_str(&build_histogram_svg(
            salary_min_values, 10_000, "#42A5F5", min_median, min_mean, mode_min,
        ));
        html.push_str("</div>\n");
    }
    if !salary_max_values.is_empty() {
        let max_mean = distribution_mean(salary_max_values);
        let max_median = distribution_median(salary_max_values);
        html.push_str("<div class=\"salary-chart-block salary-chart-page-start\">\n");
        html.push_str("<h3>上限給与の分布（10,000円刻み・概観）</h3>\n");
        render_figure_caption(
            html, "図 3-3",
            "上限月給ヒストグラム（10,000円刻み・縦線=平均/中央値/最頻値）",
        );
        let mode_max = compute_mode(salary_max_values, 10_000);
        html.push_str(&build_histogram_svg(
            salary_max_values, 10_000, "#66BB6A", max_median, max_mean, mode_max,
        ));
        html.push_str("</div>\n");
    }

    // (2) 給与構造クラスタ分析 (上限あり求人のペアから 3×3 クラスタを生成)
    let pairs: Vec<(i64, i64)> = agg
        .scatter_min_max
        .iter()
        .filter(|p| p.x > 0 && p.y >= p.x)
        .map(|p| (p.x, p.y))
        .collect();
    let clusters = compute_salary_clusters(&pairs);

    if !clusters.is_empty() {
        html.push_str("<div class=\"salary-chart-block salary-chart-page-start\">\n");
        html.push_str("<h3>給与構造クラスタ分析 (下限給与 × レンジ幅)</h3>\n");
        render_figure_caption(
            html, "表 3-A",
            "給与構造クラスタ別 件数 / P25 / P50 (中央値) / P75 / P90",
        );
        html.push_str(&build_cluster_table_html(
            &clusters,
            "クラスタ生成に必要な (下限, 上限) ペアが不足しています",
        ));

        // So-What コメント (求人群全体の中央値が最近いクラスタを判定)
        if let Some(min_median) = distribution_median(salary_min_values) {
            html.push_str(&cluster_so_what_text(&clusters, min_median));
        }

        // クラスタ boxplot 並列
        html.push_str("<div class=\"salary-chart-block salary-chart-page-start\">\n");
        render_figure_caption(
            html, "図 3-A",
            "給与構造クラスタ別 ボックスプロット (各クラスタの下限給与分布)",
        );
        html.push_str(&build_cluster_boxplots_svg(&clusters));
        html.push_str("</div>\n");

        html.push_str("</div>\n");

        // Round 21 (2026-05-13): 上位 3 クラスタの動的 bin ヒストグラム
        // Freedman-Diaconis rule で bin 幅自動算出。クラスタ内の分布形 (単峰/多峰) を可視化。
        let cluster_histograms = build_cluster_histograms_svg(&pairs, &clusters, 3);
        if !cluster_histograms.is_empty() {
            html.push_str(&cluster_histograms);
        }
    }

    // (3) 上限なし求人 別表 (salary_min_values 内、scatter_min_max にペアなしのもの)
    let pair_set: std::collections::HashSet<i64> =
        pairs.iter().map(|p| p.0).collect();
    let unpaired_count = salary_min_values.iter().filter(|v| !pair_set.contains(v)).count();
    if unpaired_count > 0 {
        html.push_str("<div class=\"salary-chart-block\">\n");
        html.push_str("<h3>上限なし求人 (下限のみ表記の求人)</h3>\n");
        html.push_str(&format!(
            "<p class=\"note\">対象: {} 件。上限給与が省略された求人は給与レンジ幅が不明なため、\
             上記クラスタ分析からは除外しています。下限のみの分布として参考表示します。</p>\n",
            unpaired_count,
        ));
    }

    // (4) 上側外れ値求人 別表 (Q3 + 1.5*IQR 超)
    let (_body, upper_outliers) = split_upper_outliers(salary_max_values);
    if !upper_outliers.is_empty() {
        html.push_str(&format!(
            "<div class=\"salary-chart-block\">\n\
             <h3>上側外れ値求人 (高待遇訴求 候補)</h3>\n\
             <p class=\"note\">上限給与が Q3 + 1.5×IQR を超える {} 件。\
             高単価・歩合・委託・管理職候補など特殊条件求人として参考表示。\
             クラスタ平均からは除外しています。</p>\n\
             </div>\n",
            upper_outliers.len(),
        ));
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn distribution_stats_use_each_salary_bound_not_overall_salary() {
        let min_values = vec![180_000, 200_000, 220_000, 240_000];
        let max_values = vec![280_000, 320_000, 400_000, 540_000];

        assert_eq!(distribution_mean(&min_values), Some(210_000));
        assert_eq!(distribution_median(&min_values), Some(210_000));
        assert_eq!(distribution_mean(&max_values), Some(385_000));
        assert_eq!(distribution_median(&max_values), Some(360_000));
    }
}
