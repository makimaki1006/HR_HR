//! 分割: report_html/scatter.rs (物理移動・内容変更なし)

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

pub(super) fn render_section_scatter(html: &mut String, agg: &SurveyAggregation) {
    if agg.scatter_min_max.len() < 6 {
        return;
    }

    html.push_str("<div class=\"section\">\n");
    html.push_str("<h2>相関分析（散布図）</h2>\n");
    html.push_str(
        "<p style=\"font-size:9pt;color:#555;margin:0 0 8px;\">\
        <strong>【読み方ガイド】</strong>各点が1件の求人。回帰線（赤破線）は全体傾向。\
        R²（決定係数）は0〜1で、1に近いほど相関が強い。\
    </p>\n",
    );

    render_section_howto(
        html,
        &[
            "各点が 1 求人。横軸=下限月給、縦軸=上限月給で表示",
            "赤の破線は線形回帰の傾き。点が線に近いほど下限と上限の関係は安定",
            "R² の閾値目安: > 0.5 強い相関 / 0.3-0.5 中程度 / < 0.3 弱い相関",
        ],
    );

    // ECharts scatter データ生成（最大200点）
    html.push_str("<h3>月給下限 vs 上限</h3>\n");
    render_figure_caption(
        html,
        "図 5-1",
        "月給 下限 × 上限 散布図（回帰線オーバーレイ）",
    );

    // 異常値除外: 5万〜200万円の妥当な範囲、かつ上限≧下限
    // （時給や年収の月給換算ミスによる外れ値を排除）
    let filtered_points: Vec<&ScatterPoint> = agg
        .scatter_min_max
        .iter()
        .filter(|p| {
            let x_man = p.x as f64 / 10_000.0;
            let y_man = p.y as f64 / 10_000.0;
            (5.0..=200.0).contains(&x_man) && (5.0..=200.0).contains(&y_man) && y_man >= x_man
        })
        .collect();

    if filtered_points.len() < 6 {
        html.push_str("<p style=\"font-size:9pt;color:#888;\">有効なデータ点が不足しているため散布図を省略しました。</p>\n");
        html.push_str("</div>\n");
        return;
    }

    // Round 17 (2026-05-13): ECharts → SSR SVG。
    // データ整合性 fix (B-P1-1): 描画 (filtered_points) と回帰線で同じソース (5-200 万円フィルタ後)
    // を使うため、ローカルで再 regression を計算する。aggregator.regression は時給混入で歪んでいる。
    let points_yen: Vec<(f64, f64)> = filtered_points
        .iter()
        .take(500)
        .map(|p| (p.x as f64, p.y as f64))
        .collect();
    let local_regression = compute_simple_regression(&points_yen);
    html.push_str(&build_scatter_svg(&points_yen, local_regression));

    if let Some(reg) = &agg.regression_min_max {
        let strength = if reg.r_squared > 0.7 {
            "強い相関"
        } else if reg.r_squared > 0.4 {
            "中程度の相関"
        } else {
            "弱い相関"
        };
        // 強さに応じた色（緑=強、橙=中、グレー=弱）
        let strength_color = if reg.r_squared > 0.5 {
            "#10b981"
        } else if reg.r_squared >= 0.3 {
            "#f59e0b"
        } else {
            "#94a3b8"
        };
        html.push_str(&format!(
            "<p style=\"font-size:9px;color:#666;\">データ点: {}件（表示: {}件、異常値除外後）/ \
             R\u{00B2} = <span style=\"color:{};font-weight:700;\">{:.3}</span>（{}）/ \
             傾き = {:.3} / 切片 = {}円</p>\n",
            agg.scatter_min_max.len(),
            filtered_points.len(),
            strength_color,
            reg.r_squared,
            strength,
            reg.slope,
            format_number(reg.intercept as i64),
        ));

        // 表 5-1: 回帰分析サマリ + R² 閾値ガイド
        render_figure_caption(html, "表 5-1", "回帰分析サマリ + R² 閾値ガイド");
        html.push_str("<table class=\"sortable-table zebra\">\n");
        html.push_str("<thead><tr><th>項目</th><th style=\"text-align:right\">値</th><th>意味</th></tr></thead>\n<tbody>\n");
        html.push_str(&format!(
            "<tr><td>R\u{00B2}（決定係数）</td><td class=\"num\" style=\"color:{};font-weight:700;\">{:.3}</td>\
             <td style=\"font-size:9pt;color:#666\">{} （> 0.5: 強 / 0.3-0.5: 中 / < 0.3: 弱）</td></tr>\n",
            strength_color, reg.r_squared, strength
        ));
        html.push_str(&format!(
            "<tr><td>傾き (slope)</td><td class=\"num\">{:.3}</td>\
             <td style=\"font-size:9pt;color:#666\">下限が 1 円増えると上限が {:.3} 円増える傾向</td></tr>\n",
            reg.slope, reg.slope
        ));
        html.push_str(&format!(
            "<tr><td>切片 (intercept)</td><td class=\"num\">{}円</td>\
             <td style=\"font-size:9pt;color:#666\">下限 0 円のときの推定上限（参考値）</td></tr>\n",
            format_number(reg.intercept as i64)
        ));
        html.push_str(&format!(
            "<tr><td>有効サンプル</td><td class=\"num\">{}件</td>\
             <td style=\"font-size:9pt;color:#666\">5万〜200万円かつ y\u{2265}x の妥当範囲</td></tr>\n",
            format_number(filtered_points.len() as i64)
        ));
        html.push_str("</tbody></table>\n");

        // 読み方ヒント（相関≠因果を明記）
        render_read_hint_html(
            html,
            &format!(
                "下限と上限の関係は <strong>{}</strong>（R\u{00B2}={:.3}）。\
                 これは「下限が高い求人は上限も高い」という<strong>相関</strong>であり、\
                 因果関係を示すものではありません。給与レンジ設計の参考傾向としてご利用ください。",
                strength, reg.r_squared
            ),
        );
    }

    render_section_bridge(
        html,
        "次セクションでは、地域（都道府県・市区町村）ごとの給与水準を比較します。",
    );

    html.push_str("</div>\n");
}
