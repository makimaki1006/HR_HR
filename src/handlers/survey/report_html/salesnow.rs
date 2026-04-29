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
    html.push_str("<h2 id=\"region-featured-title\">第5章 地域注目企業 (規模の大きい順)</h2>\n");
    // 2026-04-29 中立化:
    //   ユーザー指摘「掲載企業は提出先企業自身を含む可能性があるため、敵対的表現は不適切」に対応。
    //   「採用活動度 高/中/低」のような評価ラベルや、「採用活発」「逆方向」等の対立的解釈を排除。
    //   本セクションは <strong>地域内ベンチマーク参考</strong>として位置付ける。
    // feedback_correlation_not_causation.md / feedback_hw_data_scope.md 準拠継続。
    html.push_str(
        "<p class=\"section-sowhat\" contenteditable=\"true\" spellcheck=\"false\">\
        \u{203B} 地域内で従業員数の多い 30 社を <strong>地域内ベンチマーク参考</strong>として整理しています。\
        掲載企業には貴社・貴社の取引先・関連会社が含まれる可能性があり、敵対視や差別化を意図しません。\
        HW 求人件数が多い法人は採用活動が活発な可能性がありますが、\
        反対に採用が難航しているために HW にも掲載しているケース (求人が長く埋まらず継続掲載されている等) も含まれるため、両方向の解釈に注意してください。\
        売上規模・人員推移は外部企業 DB の参照時点に依存し、組織改編 (合併・分社) ・統計粒度 (連結⇄単体) の揺らぎを含む参考値です。\
        本セクションの数値は相関の観測であり、因果関係や採用優劣を主張するものではありません。</p>\n",
    );
    // 組織改編・粒度ゆらぎ注記をグレーバナーで強調 (UI-3)
    html.push_str(
        "<div class=\"report-banner-gray\" role=\"note\">\
         \u{1F503} <strong>注記</strong>: 売上 / 人員推移は外部企業 DB の参照時点に依存します。\
         直近の組織改編（合併・分社）・統計粒度の揺らぎ（連結⇄単体）により実態と乖離する場合があります。\
         また、HW industry_mapping の confidence < 0.7 の業種分類は推定を含みます。</div>\n",
    );
    // 「観測指標」凡例 (採点的色分けではなく、HW 求人 + 人員推移 の参考指標として中立記述)
    html.push_str("<div style=\"display:flex;flex-wrap:wrap;gap:12px;margin:6px 0 8px;font-size:9.5pt;color:#475569;\">");
    html.push_str(
        "<span>\u{2139} <strong>観測指標</strong>: HW 求人数 × 1 年人員推移 を合成した参考値です。\
         値の大小は調査時点の公開情報の <em>多寡を機械的に示すもの</em>であり、企業の優劣評価ではありません。</span>",
    );
    html.push_str("</div>\n");
    // 表番号 (表 5-1)
    html.push_str(&render_table_number(
        5,
        1,
        "地域企業 一覧（従業員数の多い 30 社、ベンチマーク参考）",
    ));
    html.push_str(&render_reading_callout(
        "「観測指標」列は HW 求人数と 1 年人員推移を合成した参考値です。\
         値が大きい企業は調査時点の公開情報量が多いことを示すのみで、採用成功や経営優劣を意味しません。\
         本表は地域全体の構造把握用ベンチマークとして参照してください。",
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
        "観測指標", // 2026-04-29 中立化: 「採用活動度」(評価語) → 「観測指標」(中立)
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

/// 地域企業の 4 軸ベンチマーク (規模上位 / 中規模層 / 人員拡大期 / 求人積極期)
///
/// 2026-04-29 追加 → 同日改訂 (中立化):
/// ユーザー指摘「掲載企業は提出先企業自身を含む可能性が高く、敵対表現は不適切」に対応。
/// 「競合分析」「採用活発」のような評価的表現を、純粋な記述的セグメントに変更:
///   - 規模上位 (employee_count Top)
///   - 中規模層 (50-300 名)
///   - 人員拡大期 (1y 推移 +10% 以上)
///   - 求人積極期 (HW 求人 5 件以上)
/// **本セクションは地域内の自社ポジション確認用ベンチマーク**であり、
/// 個別企業の評価・優劣判定を目的としない。
pub(super) fn render_section_company_segments(
    html: &mut String,
    segments: &super::super::super::company::fetch::RegionalCompanySegments,
) {
    if segments.is_empty() {
        return;
    }
    html.push_str(
        "<section class=\"section\" role=\"region\" aria-labelledby=\"region-segments-title\">\n",
    );
    html.push_str("<h2 id=\"region-segments-title\">第5章 地域企業 ベンチマーク (規模・人員推移・求人動向)</h2>\n");
    html.push_str(
        "<p class=\"section-sowhat\">\
        \u{203B} 地域内の企業を **規模・人員推移・求人動向** の 3 観点で抽出し、それぞれ最大 10 社ずつ提示します。\
        本セクションは <strong>自社が地域内でどの位置にあるかを確認するためのベンチマーク</strong>であり、\
        個別企業の優劣評価や敵対視を目的としません。\
        掲載企業には貴社・貴社の取引先・関連会社が含まれる可能性があります。\
        各指標は外部企業 DB (SalesNow) と HW 公開求人データ由来の参考値で、\
        調査時点の公開情報に基づきます。</p>\n",
    );

    // 規模分布ヒストグラム (簡易バーチャート、テキストベース)
    let hist = segments.size_histogram();
    let total: usize = hist.iter().map(|(_, n)| n).sum();
    if total > 0 {
        html.push_str(&render_table_number(
            5,
            0,
            "セグメント企業の規模分布 (4 セグメント抽出後の重複除去ベース)",
        ));
        html.push_str("<div class=\"size-histogram\" style=\"margin:6px 0 12px;\">\n");
        for (label, count) in hist.iter() {
            let pct = if total > 0 {
                (*count as f64 / total as f64 * 100.0).round() as u32
            } else {
                0
            };
            html.push_str(&format!(
                "<div style=\"display:flex;align-items:center;gap:8px;margin:3px 0;font-size:10pt;\">\
                 <span style=\"width:90px;\">{}</span>\
                 <div style=\"flex:1;background:#f0f4f8;height:14px;border-radius:3px;position:relative;\">\
                 <div style=\"background:#3b82f6;width:{}%;height:100%;border-radius:3px;\"></div>\
                 </div>\
                 <span style=\"width:60px;text-align:right;font-size:9.5pt;color:#6b7280;\">{} 社 ({:.0}%)</span>\
                 </div>\n",
                escape_html(label),
                pct,
                count,
                pct
            ));
        }
        html.push_str("</div>\n");
    }

    // 4 セグメントカード (中立的な記述ラベルに統一)
    // 評価的表現「急成長」「採用活発」「競合」「リスク」は意図的に回避し、
    // 純粋な観測カテゴリ (拡大期 / 積極期 / 規模帯) で記述する。
    let segment_blocks: [(&str, &str, &str, &[NearbyCompany], &str); 4] = [
        (
            "🏢 規模の大きい層",
            "従業員数の多い 10 社",
            "地域内で従業員規模の大きい企業群。自社が同規模帯にある場合のベンチマーク参考。",
            &segments.large,
            "regional-segment-large",
        ),
        (
            "🏬 中規模層 (50-300 名)",
            "50-300 名規模 10 社",
            "地域内の中規模帯。自社が同帯にある場合の隣接ベンチマーク参考。",
            &segments.mid,
            "regional-segment-mid",
        ),
        (
            "📈 人員拡大期 (1y +10% 超)",
            "過去 1 年で人員が +10% 以上増加した 10 社",
            "公開情報上、過去 1 年で人員が拡大している地域企業群。地域全体の採用市況の参考指標。",
            &segments.growth,
            "regional-segment-growth",
        ),
        (
            "🎯 求人積極期 (HW 5 件以上)",
            "ハローワークで 5 件以上の求人を継続している 10 社",
            "公開情報上、HW で複数件の求人を出している地域企業群。地域の採用動向の参考指標。",
            &segments.hiring,
            "regional-segment-hiring",
        ),
    ];

    for (label, subtitle, hint, list, testid) in segment_blocks.iter() {
        if list.is_empty() {
            continue;
        }
        html.push_str(&format!(
            "<div data-testid=\"{}\" style=\"margin-top:14px;\">\n\
             <h3 style=\"font-size:12.5pt;margin:6px 0 2px;\">{} <span style=\"font-size:10pt;color:#6b7280;font-weight:400;\">— {}</span></h3>\n\
             <p style=\"font-size:9.5pt;color:#374151;margin:0 0 6px;\">\u{203B} {}</p>\n",
            testid, escape_html(label), escape_html(subtitle), escape_html(hint),
        ));
        html.push_str("<table class=\"data-table report-zebra\" style=\"font-size:10pt;\">\n");
        html.push_str(
            "<thead><tr>\
             <th>#</th>\
             <th>企業名</th>\
             <th>業種</th>\
             <th class=\"num\">従業員</th>\
             <th class=\"num\">売上</th>\
             <th class=\"num\">人員 1y</th>\
             <th class=\"num\">HW 求人</th>\
             </tr></thead>\n<tbody>\n",
        );
        for (i, c) in list.iter().enumerate() {
            let sales_cell = format_sales_cell(c.sales_amount, &c.sales_range);
            let delta_cell = format_delta_cell(c.employee_delta_1y * 100.0); // 0.10 → 10.0%
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{}</td>\
                 <td class=\"num\">{}</td><td class=\"num\">{}</td>\
                 <td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
                i + 1,
                escape_html(&c.company_name),
                escape_html(&c.sn_industry),
                format_number(c.employee_count),
                sales_cell,
                delta_cell,
                if c.hw_posting_count > 0 {
                    format!("{}", c.hw_posting_count)
                } else {
                    "—".to_string()
                },
            ));
        }
        html.push_str("</tbody></table>\n</div>\n");
    }

    // 注記 (中立化済み・敵対表現を排除)
    html.push_str(
        "<div class=\"caveat\" style=\"font-size:9.5pt;color:#475569;margin-top:12px;padding:8px 12px;background:#f8fafc;border-left:3px solid #94a3b8;border-radius:3px;\">\
         <p style=\"margin:0 0 6px;\"><strong>⚠ ベンチマーク利用上の注意</strong></p>\
         <ul style=\"margin:0;padding-left:20px;line-height:1.7;\">\
         <li>各カテゴリは規模・人員推移・HW 求人数の閾値で機械的に抽出した <strong>地域全体の構造の参考値</strong>であり、\
             個別企業の経営判断や採用優劣を評価するものではありません。</li>\
         <li><strong>掲載企業には貴社・貴社の取引先・関連会社・グループ企業が含まれる可能性があります。</strong>\
             本表は調査時点の公開情報 (外部企業 DB + HW 公開求人) を機械的に集計したもので、敵対視や差別化を意図しません。</li>\
         <li>「人員拡大期」は 1 年人員推移ベースで、組織改編 (合併・分社) ・連結⇄単体の切替の影響を含む可能性があります。\
             実態の人員拡大かどうかは個別に確認してください。</li>\
         <li>「求人積極期」は HW 掲載求人ベースで、職業紹介事業者経由・自社サイト求人・非公開求人は含まれません。\
             採用活動の全体像を表すものではありません。</li>\
         <li>本セクションは <strong>自社のポジション確認用ベンチマーク</strong>であり、\
             相関の可視化に留まります。因果関係や採用成否を主張するものではありません。</li>\
         </ul>\
         </div>\n",
    );

    html.push_str("</section>\n");
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
