//! 分割: report_html/salesnow.rs (物理移動・内容変更なし)

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

/// 地域注目企業テーブル
/// Why: 求人市場分析レポートから実際にアプローチ可能な企業リストへ繋げる
/// How: employee_count 降順で従業員数の多い 30 社を印刷レポートに追加
///
/// 2026-04-24 追加要件 3: 表示項目刷新
/// - 削除: 信用スコア (credit_score) — struct には残すが UI 非表示
/// - 追加: 売上 (sales_amount / sales_range) / 1年人員推移 / 3ヶ月人員推移
///
/// 関数名は呼出側の互換のため残す（UI 表示文言のみ「地域注目企業」に統一）
pub(super) fn render_section_salesnow_companies(html: &mut String, companies: &[NearbyCompany]) {
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"region-featured-title\">\n",
    );
    html.push_str("<h2 id=\"region-featured-title\">第5章 地域注目企業</h2>\n");
    // feedback_correlation_not_causation.md 準拠:
    //   「HW多 = 採用活発」のような因果断定を避け、両方向の解釈余地を明示する。
    //   採用困難ゆえに HW にも掲載しているケース（逆方向）も含まれることを注記。
    // feedback_hw_data_scope.md 準拠:
    //   組織改編・統計粒度の揺らぎを含む参考値であることを明示。
    html.push_str(
        "<p class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">\
        \u{203B} 地域内で従業員数の多い 30 社を整理しています。\
        HW 求人件数が多い法人は採用活動が活発な可能性がありますが、\
        反対に採用が難航しているために HW にも掲載しているケースも含まれるため、両方向の解釈に注意してください。\
        売上規模・人員推移は外部企業 DB 由来の参考値で、直近の組織改編や統計粒度による揺らぎを含む点にご留意ください。\
        本セクションの数値は相関の観測であり、因果関係を主張するものではありません。</p>\n",
    );
    // 組織改編・粒度ゆらぎ注記をグレーバナーで強調 (UI-3)
    html.push_str(
        "<div class=\"report-banner-gray\" role=\"note\">\
         \u{1F503} <strong>注記</strong>: 売上 / 人員推移は外部企業 DB の参照時点に依存します。\
         直近の組織改編（合併・分社）・統計粒度の揺らぎ（連結⇄単体）により実態と乖離する場合があります。\
         また、HW industry_mapping の confidence < 0.7 の業種分類は推定を含みます。</div>\n",
    );
    // 採用活動度スコアの読み方を凡例として提示
    html.push_str("<div style=\"display:flex;flex-wrap:wrap;gap:12px;margin:6px 0 8px;\">");
    html.push_str(&render_legend_emoji(
        ReportSeverity::Critical,
        "採用活動度 高（HW件数多×人員増）",
    ));
    html.push_str(&render_legend_emoji(
        ReportSeverity::Warning,
        "採用活動度 中（中規模採用）",
    ));
    html.push_str(&render_legend_emoji(
        ReportSeverity::Info,
        "採用活動度 低（参考値）",
    ));
    html.push_str("</div>\n");
    // 表番号 (表 5-1) — 「ランキング」「上位」は禁止ワードのため、別表現を採用
    html.push_str(&render_table_number(
        5,
        1,
        "地域注目企業 一覧（従業員数の多い 30 社）",
    ));
    html.push_str(&render_reading_callout(
        "「採用活動度」列は HW 求人数と 1 年人員推移を合成した参考スコアです。\
         スコアが高くても採用成功を意味するわけではなく、求人を出しているという観測のみを示します。\
         接触判断は他指標と併せて行ってください。",
    ));
    html.push_str("<table class=\"data-table report-zebra\">\n");
    html.push_str("<thead><tr>");
    for h in [
        "番号",
        "企業名",
        "都道府県",
        "業種",
        "従業員数",
        "売上",
        "1年人員推移",
        "3ヶ月人員推移",
        "HW求人数",
        "採用活動度",
    ] {
        html.push_str(&format!("<th>{}</th>", escape_html(h)));
    }
    html.push_str("</tr></thead><tbody>\n");
    // 採用活動度の最大値（テーブル内 normalize 用）
    let max_score: f64 = companies
        .iter()
        .take(30)
        .map(|c| compute_recruitment_score(c))
        .fold(0.0_f64, f64::max);
    for (i, c) in companies.iter().take(30).enumerate() {
        html.push_str("<tr>");
        html.push_str(&format!("<td>{}</td>", i + 1));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.company_name)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.prefecture)));
        html.push_str(&format!("<td>{}</td>", escape_html(&c.sn_industry)));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.employee_count)
        ));
        let sales_cell = format_sales_cell(c.sales_amount, &c.sales_range);
        html.push_str(&format!("<td class=\"right\">{}</td>", sales_cell));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_1y)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_delta_cell(c.employee_delta_3m)
        ));
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_number(c.hw_posting_count)
        ));
        // 採用活動度スコアセル (UI-3 強化)
        let score = compute_recruitment_score(c);
        html.push_str(&format!(
            "<td class=\"right\">{}</td>",
            format_recruitment_score_cell(score, max_score)
        ));
        html.push_str("</tr>\n");
    }
    html.push_str("</tbody></table>\n");
    // 表 5-1 補足
    html.push_str(
        "<p class=\"note\">採用活動度 = HW求人数（log1p 正規化）+ max(1年人員推移, 0) × 0.5。\
         スコアは表内 30 社内での相対値（0〜100）として表示されます。</p>\n",
    );
    html.push_str("</section>\n");
}

/// 採用活動度スコア（参考値）を算出
///
/// HW 求人件数（log1p で大規模企業のスケール緩和）+ 1 年人員増分 × 0.5 を合成。
/// 採用「活動」の参考指標であり、採用「成功」を意味しない。
pub(super) fn compute_recruitment_score(c: &NearbyCompany) -> f64 {
    let hw = (c.hw_posting_count.max(0) as f64).ln_1p();
    let delta = if c.employee_delta_1y.is_finite() {
        c.employee_delta_1y.max(0.0)
    } else {
        0.0
    };
    hw + delta * 0.5
}

/// 採用活動度スコアセル: スコア値 + ミニ bar（テーブル内相対）
///
/// max_score=0 の場合（全社 HW 件数 0 等）は "-" を返す。
pub(super) fn format_recruitment_score_cell(score: f64, max_score: f64) -> String {
    if !score.is_finite() || max_score <= 0.0 {
        return "-".to_string();
    }
    let pct = (score / max_score * 100.0).clamp(0.0, 100.0);
    // bar 幅 (px): 0..60
    let bar_width = (pct * 0.6).round() as u32;
    format!(
        "<span style=\"font-variant-numeric:tabular-nums;\">{:.1}</span>\
         <span class=\"report-mini-bar\" style=\"width:{}px;\" \
         role=\"img\" aria-label=\"スコア {:.1}（最大値比 {:.0}%）\"></span>",
        score, bar_width, score, pct,
    )
}

/// 売上セル整形: 売上金額と区分ラベルを 1 セル 2 行で表示
pub(super) fn format_sales_cell(amount: f64, range: &str) -> String {
    if amount <= 0.0 && range.is_empty() {
        return "-".to_string();
    }
    // 金額は百万円単位以上に丸めて表示
    let amount_display = if amount >= 1.0e9 {
        format!("{:.1} 億円", amount / 1.0e8)
    } else if amount >= 1.0e6 {
        format!("{:.0} 百万円", amount / 1.0e6)
    } else if amount > 0.0 {
        format!("{:.0} 円", amount)
    } else {
        "-".to_string()
    };
    let range_display = if range.is_empty() {
        String::new()
    } else {
        format!(
            "<br><span style=\"font-size:9pt;color:var(--c-text-muted);\">{}</span>",
            escape_html(range)
        )
    };
    format!("{}{}", escape_html(&amount_display), range_display)
}

/// 人員推移セル整形: 増減符号付き %、0 は横ばい
pub(super) fn format_delta_cell(pct: f64) -> String {
    // NaN / 極端値ガード
    if !pct.is_finite() {
        return "-".to_string();
    }
    let cls = if pct > 0.5 {
        "trend-up"
    } else if pct < -0.5 {
        "trend-down"
    } else {
        "trend-flat"
    };
    format!("<span class=\"{}\">{:+.1}%</span>", cls, pct)
}
