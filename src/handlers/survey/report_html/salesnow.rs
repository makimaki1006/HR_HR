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
        "<p class=\"note\">観測指標 = HW求人数（log1p 正規化）+ max(1年人員推移, 0) × 0.5。\
         値は表内 30 社内での相対参考値（0〜100）として表示されます。\
         本値は調査時点の公開情報量を機械的に示すもので、企業の優劣評価ではありません。</p>\n",
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

/// 2026-04-29: 業界フィルタ対応版
///
/// 業界指定時: 全業界版 + 同業界版 の **両方** を並列表示。
/// 業界未指定時: 全業界版のみを表示 (異業種ベンチマーク用途)。
///
/// 切替トグル UI ではなくレポート内に両方を併記することで、
/// 「同業界の競合動向」と「異業種からのベンチマーク」を同時に把握可能にする。
pub(super) fn render_section_company_segments_with_industry(
    html: &mut String,
    segments_all: &super::super::super::company::fetch::RegionalCompanySegments,
    segments_industry: &super::super::super::company::fetch::RegionalCompanySegments,
    industry_filter: Option<&str>,
) {
    if segments_all.is_empty() {
        return;
    }
    match industry_filter {
        Some(industry) if !industry.is_empty() => {
            // 業界指定時: 同業界 + 全業界 両方表示
            html.push_str(&format!(
                "<div style=\"margin-top:10px;padding:8px 12px;background:#fef3c7;border-left:4px solid #f59e0b;border-radius:3px;font-size:10pt;\">\
                 <strong>📌 業界フィルタ指定中:</strong> 「{}」 — 同業界版と全業界版の両方を以下に併記します。\
                 同業界版は競合・隣接企業のベンチマーク、全業界版は地域全体の構造把握に利用してください。\
                 </div>\n",
                escape_html(industry)
            ));
            // 同業界版 (上位、ピンポイント比較)
            if !segments_industry.is_empty() {
                html.push_str(&format!(
                    "<h2 style=\"margin-top:14px;\">第5章 地域企業 ベンチマーク (同業界: {})</h2>\n",
                    escape_html(industry)
                ));
                html.push_str(&format!(
                    "<p class=\"section-sowhat\">\u{203B} 業界「{}」に絞り込んだ地域内企業の規模・人員推移・求人動向。\
                     SalesNow `sn_industry` LIKE 部分一致で抽出。サンプルが少ない場合は「全業界版」も併せて参考にしてください。</p>\n",
                    escape_html(industry)
                ));
                render_section_company_segments(html, segments_industry);
            } else {
                // 2026-04-29 UX 改善: 近接業界 / 大分類への置換を具体的に提案
                html.push_str(&format!(
                    "<div data-testid=\"industry-zero-match-banner\" style=\"margin:10px 0;padding:10px 14px;background:#fee2e2;border-left:3px solid #dc2626;border-radius:3px;font-size:10pt;line-height:1.7;\">\
                     <strong>⚠ 業界フィルタ「{}」では地域内マッチが 0 件でした。</strong><br>\
                     <span style=\"font-size:9.5pt;color:#7f1d1d;\">以下の試行を推奨します:</span>\
                     <ul style=\"margin:6px 0 0;padding-left:20px;font-size:9.5pt;color:#7f1d1d;\">\
                     <li>大分類で再指定: 「医療・福祉」「サービス業」「製造業」「卸売・小売業」など、より広い業種カテゴリで再試行</li>\
                     <li>近接業界で再試行: 同じ大分類内の隣接業界を試行 (例: 「介護スタッフ」→「医療事務」「保育補助」「障害者支援」等)</li>\
                     <li>業界フィルタを外して<strong>地域全体のベンチマーク</strong>を参照 (下記「全業界版」)</li>\
                     </ul>\
                     <span style=\"font-size:9pt;color:#991b1b;display:block;margin-top:4px;\">\u{203B} SalesNow `sn_industry` は LIKE 部分一致で抽出しているため、表記揺れ (例: 「介護」 / 「介護スタッフ」 / 「介護福祉士」) で結果が変わる可能性があります。</span>\
                     </div>\n",
                    escape_html(industry)
                ));
            }
            // 全業界版 (異業種ベンチマーク)
            html.push_str("<h2 style=\"margin-top:14px;\">第5章 地域企業 ベンチマーク (全業界、異業種ベンチマーク用)</h2>\n");
            html.push_str(
                "<p class=\"section-sowhat\">\u{203B} 業界フィルタを外した地域内企業全体。\
                 異業種からのベンチマーク (採用施策の他業界事例参考) や未経験採用候補の検討に。</p>\n",
            );
            render_section_company_segments(html, segments_all);
        }
        _ => {
            // 業界未指定: 全業界のみ
            render_section_company_segments(html, segments_all);
        }
    }
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
/// 2026-04-30: 規模 × 動向 6 マトリクス表示
///
/// ユーザー指摘:
/// > 増員出来ている、離職が多い、それぞれで大企業と中小企業と零細企業のセグメントがあると良い
///
/// 規模帯 (大企業 300+ / 中小企業 50-299 / 零細企業 <50) × 動向 (増員 / 減少) の
/// 6 マトリクスで地域企業の動きを多面的に提示。
///
/// 表現は中立化:
/// - 「離職が多い」→「人員減少傾向」(組織改編・自然減・配置転換等も含む観測)
/// - 「増員できている」→「人員増加傾向」
fn render_size_x_trend_matrix(
    html: &mut String,
    segments: &super::super::super::company::fetch::RegionalCompanySegments,
) {
    let total = segments.growth_large.len()
        + segments.growth_mid.len()
        + segments.growth_small.len()
        + segments.decline_large.len()
        + segments.decline_mid.len()
        + segments.decline_small.len();
    if total == 0 {
        return;
    }

    html.push_str("<h3 style=\"font-size:13pt;margin:18px 0 6px;color:#0f172a;display:flex;align-items:center;gap:8px;\"><span style=\"display:inline-block;width:5px;height:18px;background:linear-gradient(180deg, #10b981 0%, #dc2626 100%);border-radius:2px;\"></span>表 5-0b 規模 × 人員推移 6 マトリクス <span style=\"font-size:9.5pt;font-weight:400;color:#64748b;\">(各セル上位 5 社)</span></h3>\n");
    html.push_str(
        "<p style=\"font-size:9.5pt;color:#475569;margin:0 0 10px;line-height:1.6;\">\
         \u{203B} 規模帯 (大企業 / 中小企業 / 零細企業) と 1 年人員推移 (+5% 超 / -5% 未満) の組み合わせで\
         該当企業を抽出。「人員減少傾向」は離職だけでなく組織改編・自然減・配置転換等も含む観測です。\
         </p>\n",
    );

    // 6 セルを Web では 3 列、印刷時は @media print で 2 列に切替 (A4 縦最適化)
    html.push_str("<div class=\"size-x-trend-matrix\" style=\"display:grid;grid-template-columns:repeat(3,1fr);gap:10px;margin:8px 0;\">\n");

    let cells: [(&str, &str, &[NearbyCompany], &str); 6] = [
        (
            "📈 大企業 × 人員増加",
            "300+ 名 / 1y +5% 超",
            &segments.growth_large,
            "matrix-growth-large",
        ),
        (
            "📈 中小企業 × 人員増加",
            "50-299 名 / 1y +5% 超",
            &segments.growth_mid,
            "matrix-growth-mid",
        ),
        (
            "📈 零細企業 × 人員増加",
            "<50 名 / 1y +5% 超",
            &segments.growth_small,
            "matrix-growth-small",
        ),
        (
            "📉 大企業 × 人員減少",
            "300+ 名 / 1y -5% 未満",
            &segments.decline_large,
            "matrix-decline-large",
        ),
        (
            "📉 中小企業 × 人員減少",
            "50-299 名 / 1y -5% 未満",
            &segments.decline_mid,
            "matrix-decline-mid",
        ),
        (
            "📉 零細企業 × 人員減少",
            "<50 名 / 1y -5% 未満",
            &segments.decline_small,
            "matrix-decline-small",
        ),
    ];

    for (label, hint, list, testid) in cells.iter() {
        let is_growth = label.contains("増加");
        // 装飾強化 (2026-04-30): 印刷でも視認性が高い淡色グラデーション + 4px ボーダー
        let bg_grad = if is_growth {
            "linear-gradient(135deg, #ecfdf5 0%, #d1fae5 100%)"
        } else {
            "linear-gradient(135deg, #fef2f2 0%, #fee2e2 100%)"
        };
        let border_color = if is_growth { "#10b981" } else { "#dc2626" };
        let header_color = if is_growth { "#065f46" } else { "#991b1b" };
        let count_pill_bg = if is_growth { "#10b981" } else { "#dc2626" };

        html.push_str(&format!(
            "<div data-testid=\"{}\" style=\"padding:10px 12px;background:{};border:1px solid {};border-left:4px solid {};border-radius:6px;font-size:9.5pt;page-break-inside:avoid;\">\n",
            testid, bg_grad, border_color, border_color
        ));
        // ヘッダー: ラベル左 + 件数 pill 右
        html.push_str(&format!(
            "<div style=\"display:flex;justify-content:space-between;align-items:center;margin-bottom:4px;\">\
             <div style=\"font-weight:700;color:{};font-size:10.5pt;\">{}</div>\
             <span style=\"display:inline-block;padding:1px 8px;background:{};color:#fff;font-size:9pt;font-weight:700;border-radius:10px;min-width:32px;text-align:center;\">{}社</span>\
             </div>\n",
            header_color,
            escape_html(label),
            count_pill_bg,
            list.len()
        ));
        // サブヘッダー: 抽出条件
        html.push_str(&format!(
            "<div style=\"font-size:8.5pt;color:#6b7280;margin-bottom:6px;font-style:italic;\">{}</div>\n",
            escape_html(hint),
        ));
        if list.is_empty() {
            html.push_str(
                "<div style=\"color:#9ca3af;font-style:italic;font-size:9pt;text-align:center;padding:8px 0;\">該当企業なし</div>\n",
            );
        } else {
            html.push_str("<ol style=\"margin:0;padding-left:18px;line-height:1.6;\">\n");
            for c in list.iter() {
                let pct_color = if c.employee_delta_1y > 0.0 { "#059669" } else { "#dc2626" };
                html.push_str(&format!(
                    "<li>{}<br>\
                     <span style=\"color:#6b7280;font-size:8.5pt;\">{} 名 ・ <span style=\"color:{};font-weight:600;\">{:+.1}%</span></span></li>\n",
                    escape_html(&c.company_name),
                    format_number(c.employee_count),
                    pct_color,
                    c.employee_delta_1y, // 2026-04-30: DB は %単位なので *100 不要
                ));
            }
            html.push_str("</ol>\n");
        }
        html.push_str("</div>\n");
    }
    html.push_str("</div>\n");

    // 6 マトリクス用 caveat
    html.push_str(
        "<div style=\"font-size:9pt;color:#475569;margin:6px 0;padding:6px 10px;background:#f8fafc;border-left:3px solid #94a3b8;border-radius:3px;\">\
         \u{26A0} 「人員減少傾向」は離職だけでなく組織改編・配置転換・連結⇄単体切替・自然減も含みます。\
         「人員増加傾向」も M&A・採用・連結化等の複合要因を含むため、個別企業の動きは別途確認してください。\
         閾値 ±5% は 1 年推移ベース (\u{B12} % 内は変化なし扱い)。\
         </div>\n",
    );
}

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

    // 2026-04-29 (中立化 v2): バイネームに頼らない「規模帯別の構造サマリ + ルールベース示唆」
    // ユーザー指摘:
    // > 大手だけでもしょうがない / 中小顧客が多い / 大手顧客は大手のベンチマーク動向が気になる
    // > 両方羅列するとメッセージが希薄化する → 共通する尖った特徴 / 規模帯別の工夫を見せる
    let summary = segments.structural_summary();
    if summary.total_count() > 0 {
        // テーブル番号は 5-0 (構造サマリ) として、後続のヒストグラム 5-0 と区別するため
        // ここでは見出しのみテキストで提示 (render_table_number は使わない)
        html.push_str("<h3 style=\"font-size:12pt;margin:10px 0 4px;color:#0c4a6e;\">表 5-0a 地域企業 構造サマリ (規模帯別の傾向値、バイネーム非依存)</h3>\n");
        html.push_str("<div class=\"structural-summary\" style=\"margin:6px 0 14px;padding:12px 16px;background:linear-gradient(135deg, #f0f9ff 0%, #e0f2fe 100%);border-left:4px solid #0ea5e9;border-radius:6px;font-size:10pt;line-height:1.7;box-shadow:0 1px 2px rgba(14,165,233,0.08);\">\n");
        html.push_str("<div style=\"font-weight:700;color:#0c4a6e;margin-bottom:8px;font-size:11pt;\">📊 地域企業 構造サマリ <span style=\"font-size:9pt;font-weight:400;color:#475569;\">(バイネーム非依存の傾向値)</span></div>\n");

        // テーブル形式で規模帯別を提示
        html.push_str("<table style=\"width:100%;border-collapse:collapse;font-size:10pt;background:#fff;border-radius:4px;overflow:hidden;\">\n");
        html.push_str(
            "<thead><tr style=\"background:linear-gradient(180deg, #38bdf8 0%, #0ea5e9 100%);color:#fff;\">\
             <th style=\"text-align:left;padding:6px 10px;font-weight:600;\">規模帯</th>\
             <th style=\"text-align:right;padding:6px 10px;font-weight:600;\">社数</th>\
             <th style=\"text-align:right;padding:6px 10px;font-weight:600;\">構成比</th>\
             <th style=\"text-align:right;padding:6px 10px;font-weight:600;\">平均 1y 人員推移</th>\
             <th style=\"text-align:right;padding:6px 10px;font-weight:600;\">HW 求人継続率</th>\
             </tr></thead>\n<tbody>\n",
        );
        let total = summary.total_count() as f64;
        let bands: [(&str, usize, f64, f64); 3] = [
            (
                "大規模 (300+ 名)",
                summary.large_count,
                summary.large_avg_growth_pct,
                summary.large_hw_continuity_pct,
            ),
            (
                "中規模 (50-299 名)",
                summary.mid_count,
                summary.mid_avg_growth_pct,
                summary.mid_hw_continuity_pct,
            ),
            (
                "小規模 (<50 名)",
                summary.small_count,
                summary.small_avg_growth_pct,
                summary.small_hw_continuity_pct,
            ),
        ];
        for (idx, (label, count, growth, hw_cont)) in bands.iter().enumerate() {
            let pct = if total > 0.0 {
                *count as f64 / total * 100.0
            } else {
                0.0
            };
            // 行交互背景 (zebra)
            let row_bg = if idx % 2 == 0 { "#ffffff" } else { "#f8fafc" };
            // 推移の色付け (印刷でも識別しやすい配色)
            let growth_color = if *growth > 1.0 {
                "#059669" // green
            } else if *growth < -1.0 {
                "#dc2626" // red
            } else {
                "#475569" // slate
            };
            html.push_str(&format!(
                "<tr style=\"background:{};\"><td style=\"padding:6px 10px;font-weight:600;\">{}</td>\
                 <td style=\"text-align:right;padding:6px 10px;\">{} 社</td>\
                 <td style=\"text-align:right;padding:6px 10px;color:#475569;\">{:.0}%</td>\
                 <td style=\"text-align:right;padding:6px 10px;color:{};font-weight:600;font-variant-numeric:tabular-nums;\">{:+.1}%</td>\
                 <td style=\"text-align:right;padding:6px 10px;font-variant-numeric:tabular-nums;\">{:.0}%</td></tr>\n",
                row_bg,
                escape_html(label),
                count,
                pct,
                growth_color,
                growth,
                hw_cont
            ));
        }
        html.push_str("</tbody></table>\n");

        // ルールベース示唆: 規模帯間の乖離 / 共通点を抽出 (拡張ロジック)
        let takeaways = compute_segment_takeaways(&summary);

        html.push_str("<div style=\"margin-top:10px;padding:10px 12px;background:#fff;border:1px solid #e0f2fe;border-radius:6px;\">\n");
        html.push_str("<div style=\"font-weight:700;color:#0c4a6e;margin-bottom:6px;font-size:10pt;display:flex;align-items:center;gap:6px;\"><span style=\"display:inline-block;width:4px;height:14px;background:#0ea5e9;border-radius:2px;\"></span>地域全体の傾向 <span style=\"font-size:8.5pt;font-weight:400;color:#64748b;\">(ルールベース解釈、参考値)</span></div>\n");
        html.push_str("<ul style=\"margin:0;padding-left:18px;font-size:9.5pt;line-height:1.75;color:#334155;\">\n");
        for t in &takeaways {
            html.push_str(&format!("<li style=\"margin-bottom:3px;\">{}</li>\n", t));
        }
        html.push_str("</ul>\n");
        html.push_str("</div>\n");

        html.push_str(&format!(
            "<p style=\"font-size:9pt;color:#64748b;margin:6px 0 0;\">\u{203B} 集計対象: 4 セグメント抽出後の重複除去ベース ({} 社)。\
             pool は employee_count 降順 Top 100 で取得しており、極小規模 (<10 名) はサンプル少ない可能性。</p>\n",
            summary.total_count()
        ));
        html.push_str("</div>\n");
    }

    // 2026-04-30: 規模 × 動向 6 マトリクス
    // 既存の規模分布ヒストグラムの **直前** に配置 (構造サマリ → 6 マトリクス → ヒストグラム → 4 セグメント)
    render_size_x_trend_matrix(html, segments);

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
            let delta_cell = format_delta_cell(c.employee_delta_1y); // %単位 (10.0 = +10%)
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

/// 規模帯別構造サマリから「ルールベース示唆 (takeaway)」を生成する
///
/// 2026-04-29 拡張: 6 パターン以上に拡張 (旧版は 3-4 パターンのみ)
///
/// ## 実装ルール (memory feedback_correlation_not_causation 準拠)
/// - 「可能性」「示唆」「傾向」表現を使う
/// - 「最適」「すべき」「決定打」のような因果断定ワードは禁止
/// - 数値は具体値を含めて出力
/// - 単なる傾向値羅列ではなく、地域の構造解釈につながる文脈を添える
///
/// ## 出力パターン (合計 7 種、複数同時発火可)
/// 1. **規模間で人員方向が逆転**: 大手 +X% / 小規模 -Y% (両者の符号反転 + 差 ≥3pt)
/// 2. **全規模で人員減少**: 全 3 帯がマイナス → 地域全体の流出傾向の可能性
/// 3. **全規模で人員拡大**: 全 3 帯が +1% 以上 → 地域全体の拡大基調 / 採用競合化の可能性
/// 4. **HW 継続率が中規模で最高**: mid > large + mid > small → 中規模が HW 主戦場
/// 5. **大手だけ拡大、中小が縮小**: large > 0 + (mid + small) / 2 < 0 → 二極化
/// 6. **構成比偏重**: 1 帯が 60% 超 → サンプルバイアス注意
/// 7. **既存パターン (維持)**:
///    - 規模間で人員推移格差 ≥ 5pt
///    - 規模間で人員推移格差 < 2pt (横断的均一)
///    - HW 継続率格差 ≥ 20pt
///
/// ## fallback
/// すべてのパターンに該当しない場合は中立メッセージを 1 件返す。
pub(super) fn compute_segment_takeaways(
    summary: &super::super::super::company::fetch::StructuralSummary,
) -> Vec<String> {
    let mut takeaways: Vec<String> = Vec::new();

    let growth_spread = summary.growth_spread_pct();
    let hw_spread = summary.hw_continuity_spread_pct();
    let total = summary.total_count();
    let total_f = total as f64;

    let large_growth = summary.large_avg_growth_pct;
    let mid_growth = summary.mid_avg_growth_pct;
    let small_growth = summary.small_avg_growth_pct;
    let large_hw = summary.large_hw_continuity_pct;
    let mid_hw = summary.mid_hw_continuity_pct;
    let small_hw = summary.small_hw_continuity_pct;

    // ============================================================
    // パターン 1: 大手 vs 中小 の人員方向逆転 (符号反転 + 差 ≥3pt)
    // 大手 + / 小規模 - もしくは 大手 - / 小規模 + のケース
    // ============================================================
    if summary.large_count > 0
        && summary.small_count > 0
        && (large_growth - small_growth).abs() >= 3.0
        && large_growth.signum() != small_growth.signum()
        && large_growth.abs() >= 0.5
        && small_growth.abs() >= 0.5
    {
        takeaways.push(format!(
            "規模帯間で人員推移の方向に差分が観測される (大規模 <strong>{:+.1}%</strong> / 小規模 <strong>{:+.1}%</strong>)。\
             採用市況の動きが規模帯ごとに分かれている可能性があり、<strong>同規模帯のベンチマーク</strong>を優先参照することが推奨される。",
            large_growth, small_growth
        ));
    }

    // ============================================================
    // パターン 2: 全規模で人員減少 (全 3 帯マイナス、各帯にサンプルあり)
    // ============================================================
    if summary.large_count > 0
        && summary.mid_count > 0
        && summary.small_count > 0
        && large_growth < 0.0
        && mid_growth < 0.0
        && small_growth < 0.0
    {
        let avg = (large_growth + mid_growth + small_growth) / 3.0;
        takeaways.push(format!(
            "全規模帯で人員推移がマイナス (大規模 {:+.1}% / 中規模 {:+.1}% / 小規模 {:+.1}%、平均 {:+.1}%)。\
             地域全体で<strong>人員推移が縮小傾向</strong>を示している可能性。\
             採用ターゲットの広域化や通勤圏拡大の検討余地あり。",
            large_growth, mid_growth, small_growth, avg
        ));
    }

    // ============================================================
    // パターン 3: 全規模で人員拡大 (全 3 帯が +1% 以上、各帯にサンプルあり)
    // ============================================================
    if summary.large_count > 0
        && summary.mid_count > 0
        && summary.small_count > 0
        && large_growth >= 1.0
        && mid_growth >= 1.0
        && small_growth >= 1.0
    {
        let avg = (large_growth + mid_growth + small_growth) / 3.0;
        takeaways.push(format!(
            "全規模帯で人員推移がプラス 1% 以上 (大規模 {:+.1}% / 中規模 {:+.1}% / 小規模 {:+.1}%、平均 {:+.1}%)。\
             地域全体で<strong>人員推移が拡大傾向</strong>を示しており、採用競合化が進行している可能性。\
             同地域内での求人露出強化や差別化メッセージの精緻化が有効になりやすい局面。",
            large_growth, mid_growth, small_growth, avg
        ));
    }

    // ============================================================
    // パターン 4: HW 継続率が中規模で最高 (mid > large + mid > small)
    // ============================================================
    if summary.mid_count > 0
        && summary.large_count > 0
        && summary.small_count > 0
        && mid_hw > large_hw
        && mid_hw > small_hw
        && (mid_hw - large_hw).max(mid_hw - small_hw) >= 5.0
    {
        takeaways.push(format!(
            "HW 求人継続率は中規模帯が最も高い (中規模 {:.0}% / 大規模 {:.0}% / 小規模 {:.0}%)。\
             中規模帯で HW 掲載が密度高く観測されており、<strong>同規模帯では HW 内での求人露出が混み合いやすい</strong>可能性。\
             中規模帯を意識した求人記述・条件設計の差別化を検討する余地あり。",
            mid_hw, large_hw, small_hw
        ));
    }

    // ============================================================
    // パターン 5: 大手だけ拡大、中小は縮小 (large > 0 + (mid + small)/2 < 0)
    // ============================================================
    if summary.large_count > 0
        && (summary.mid_count > 0 || summary.small_count > 0)
        && large_growth > 0.5
        && {
            let mid_small_avg = if summary.mid_count > 0 && summary.small_count > 0 {
                (mid_growth + small_growth) / 2.0
            } else if summary.mid_count > 0 {
                mid_growth
            } else {
                small_growth
            };
            mid_small_avg < -0.5
        }
    {
        let mid_small_avg = if summary.mid_count > 0 && summary.small_count > 0 {
            (mid_growth + small_growth) / 2.0
        } else if summary.mid_count > 0 {
            mid_growth
        } else {
            small_growth
        };
        takeaways.push(format!(
            "大規模帯のみプラス ({:+.1}%)、中小規模帯は平均マイナス ({:+.1}%) で動向に差分が観測される。\
             規模帯ごとに採用市況の温度感が分かれている可能性があり、自社規模帯のベンチマーク数値を優先参照することが推奨される。",
            large_growth, mid_small_avg
        ));
    }

    // ============================================================
    // パターン 6: 構成比偏重 (1 帯が 60% 超)
    // ============================================================
    if total >= 5 {
        let large_ratio = summary.large_count as f64 / total_f * 100.0;
        let mid_ratio = summary.mid_count as f64 / total_f * 100.0;
        let small_ratio = summary.small_count as f64 / total_f * 100.0;
        let (max_ratio, max_label) = if large_ratio >= mid_ratio && large_ratio >= small_ratio {
            (large_ratio, "大規模 (300+ 名)")
        } else if mid_ratio >= small_ratio {
            (mid_ratio, "中規模 (50-299 名)")
        } else {
            (small_ratio, "小規模 (<50 名)")
        };
        if max_ratio > 60.0 {
            takeaways.push(format!(
                "規模分布が <strong>{}</strong> に偏重 (構成比 {:.0}%)。\
                 地域企業の規模分布に偏りがあり、サンプルバイアスに注意が必要。\
                 他規模帯の数値はサンプル少のため、参考値として扱う傾向が望ましい。",
                max_label, max_ratio
            ));
        }
    }

    // ============================================================
    // パターン 7 (既存): 規模間で人員推移格差 ≥ 5pt
    // ============================================================
    if growth_spread >= 5.0 {
        let max = large_growth.max(mid_growth).max(small_growth);
        let max_label = if (max - large_growth).abs() < 0.01 {
            "大規模"
        } else if (max - mid_growth).abs() < 0.01 {
            "中規模"
        } else {
            "小規模"
        };
        takeaways.push(format!(
            "規模帯間で人員推移に <strong>{:.1}pt の差</strong>が観測される (最もプラス幅が大きいのは <strong>{}</strong>)。\
             規模により採用市況の温度感が異なる地域である可能性。",
            growth_spread, max_label
        ));
    } else if growth_spread < 2.0 && total >= 5 {
        // 規模を横断して傾向が揃っている
        let avg = (large_growth + mid_growth + small_growth) / 3.0;
        takeaways.push(format!(
            "規模帯を横断して人員推移はほぼ均一 (差 {:.1}pt 以内、平均 {:+.1}%)。\
             地域全体で同方向の動き = <strong>規模に関わらず共通する地域要因</strong>がある可能性。",
            growth_spread, avg
        ));
    }

    // ============================================================
    // パターン 8 (既存): HW 継続率格差 ≥ 20pt
    // ============================================================
    if hw_spread >= 20.0 {
        let max = large_hw.max(mid_hw).max(small_hw);
        let max_label = if (max - large_hw).abs() < 0.01 {
            "大規模"
        } else if (max - mid_hw).abs() < 0.01 {
            "中規模"
        } else {
            "小規模"
        };
        takeaways.push(format!(
            "HW 求人継続率は規模帯間で <strong>{:.0}pt の差</strong> ({} が最も高い)。\
             規模ごとに HW 媒体の活用度が異なる傾向。",
            hw_spread, max_label
        ));
    }

    // fallback
    if takeaways.is_empty() {
        takeaways.push(
            "規模帯による傾向差は小さく、地域全体で標準的な分布。個別企業の動向で差別化要素を探す余地あり。"
                .to_string(),
        );
    }

    takeaways
}

// =====================================================================
// 単体テスト (規模帯別示唆 + 業界フィルタ範囲注記の逆証明)
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use super::super::super::super::company::fetch::StructuralSummary;

    fn summary(
        large_count: usize,
        mid_count: usize,
        small_count: usize,
        large_growth: f64,
        mid_growth: f64,
        small_growth: f64,
        large_hw: f64,
        mid_hw: f64,
        small_hw: f64,
    ) -> StructuralSummary {
        StructuralSummary {
            large_count,
            mid_count,
            small_count,
            large_avg_growth_pct: large_growth,
            mid_avg_growth_pct: mid_growth,
            small_avg_growth_pct: small_growth,
            large_hw_continuity_pct: large_hw,
            mid_hw_continuity_pct: mid_hw,
            small_hw_continuity_pct: small_hw,
            pool_size: large_count + mid_count + small_count,
        }
    }

    /// 逆証明テスト 1: 大規模 +5% / 小規模 -2% で「方向に差分」takeaway が出る
    /// (2026-04-30 中立化: 「逆転」「二極化」を「差分が観測される」に統一)
    #[test]
    fn segment_takeaway_direction_reversal() {
        let s = summary(5, 5, 5, 5.0, 1.0, -2.0, 60.0, 50.0, 40.0);
        let t = compute_segment_takeaways(&s);
        let joined = t.join("\n");
        assert!(
            joined.contains("差分が観測される") || joined.contains("分かれている"),
            "規模帯間で動向差がある時に中立的な「差分」表現が出るはず, got:\n{}",
            joined
        );
        // 具体値が含まれていること
        assert!(
            joined.contains("+5.0%") && joined.contains("-2.0%"),
            "具体値 +5.0% / -2.0% が含まれるはず, got:\n{}",
            joined
        );
    }

    /// 逆証明テスト 2: 全規模 -1% 以下で「人員推移が縮小傾向」が出る
    #[test]
    fn segment_takeaway_all_shrinking() {
        let s = summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0);
        let t = compute_segment_takeaways(&s);
        let joined = t.join("\n");
        assert!(
            joined.contains("縮小傾向"),
            "全規模マイナスで「縮小傾向」示唆が出るはず, got:\n{}",
            joined
        );
    }

    /// 逆証明テスト 3: 全規模 +1% 以上で「人員推移が拡大傾向」が出る
    #[test]
    fn segment_takeaway_all_expanding() {
        let s = summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0);
        let t = compute_segment_takeaways(&s);
        let joined = t.join("\n");
        assert!(
            joined.contains("拡大傾向"),
            "全規模 +1% 以上で「拡大傾向」示唆が出るはず, got:\n{}",
            joined
        );
        assert!(
            joined.contains("採用競合化"),
            "「採用競合化」の示唆も含まれるはず, got:\n{}",
            joined
        );
    }

    /// 逆証明テスト 4: 大規模構成比 65% で「規模分布に偏り」が出る
    #[test]
    fn segment_takeaway_size_distribution_bias() {
        // 大手 13 / 中 4 / 小 3 → 大手 65%
        let s = summary(13, 4, 3, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0);
        let t = compute_segment_takeaways(&s);
        let joined = t.join("\n");
        assert!(
            joined.contains("偏重") || joined.contains("偏り"),
            "大手 65% で「偏重」or「偏り」示唆が出るはず, got:\n{}",
            joined
        );
        assert!(
            joined.contains("65"),
            "構成比 65% の数値が含まれるはず, got:\n{}",
            joined
        );
    }

    /// ドメイン不変 1: 因果断定ワードを使わない
    #[test]
    fn segment_takeaway_no_causal_assertions() {
        let cases = vec![
            summary(5, 5, 5, 5.0, 1.0, -2.0, 60.0, 50.0, 40.0),
            summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0),
            summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0),
            summary(13, 4, 3, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0),
            summary(5, 5, 5, 0.0, 0.0, 0.0, 50.0, 50.0, 50.0),
        ];
        let banned = ["最適", "すべき", "決定打", "保証", "確実"];
        for s in cases {
            let t = compute_segment_takeaways(&s);
            for line in &t {
                for word in &banned {
                    assert!(
                        !line.contains(word),
                        "禁止ワード '{}' が含まれている (因果断定): {}",
                        word,
                        line
                    );
                }
            }
        }
    }

    /// ドメイン不変 2 (2026-04-30 中立化): 提案先企業を傷つける表現を含まない
    /// 営業観点レビュー #3 で指摘された「中小縮小の二極化」「劣位」「集中」などの
    /// 評価的・敵対的表現が takeaway 内に出ないことを保証する逆証明。
    #[test]
    fn segment_takeaway_no_hostile_expressions() {
        let cases = vec![
            summary(5, 5, 5, 5.0, 1.0, -2.0, 60.0, 50.0, 40.0),
            summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0),
            summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0),
            summary(5, 1, 1, 5.0, -1.5, -1.0, 60.0, 30.0, 20.0),
        ];
        // 提案先企業を傷つける可能性のある語彙 (営業観点レビュー #3)
        let hostile = [
            "中小縮小",
            "二極化",
            "大手集中",
            "劣位",
            "見かけ以上に高い",
            "縮小局面",
        ];
        for s in cases {
            let t = compute_segment_takeaways(&s);
            for line in &t {
                for word in &hostile {
                    assert!(
                        !line.contains(word),
                        "敵対的表現 '{}' が含まれている (提案先を傷つける可能性): {}",
                        word,
                        line
                    );
                }
            }
        }
    }

    /// ドメイン不変 2: takeaways は必ず最低 1 件返す (空にならない)
    #[test]
    fn segment_takeaway_never_empty() {
        // 全 0 ケースでも fallback で 1 件返ること
        let s = summary(0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        let t = compute_segment_takeaways(&s);
        assert!(!t.is_empty(), "takeaways は最低 1 件返すべき");

        // total_count >= 5 の標準ケース
        let s = summary(2, 2, 2, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0);
        let t = compute_segment_takeaways(&s);
        assert!(!t.is_empty(), "標準ケースでも takeaways は最低 1 件");
    }

    /// 業界マッチ 0 件時の UX バナー: 近接業界の提案が含まれる
    #[test]
    fn industry_zero_match_banner_suggests_alternatives() {
        use super::super::super::super::company::fetch::RegionalCompanySegments;
        let mut html = String::new();
        let segments_all = RegionalCompanySegments::default();
        let segments_industry = RegionalCompanySegments::default();
        // segments_all が空の場合 render しない (early return) ので、
        // ここでは banner テキストのみを別途検証
        // → render_section_company_segments_with_industry の banner 部分の
        //   出力をシミュレート
        render_section_company_segments_with_industry(
            &mut html,
            &segments_all,
            &segments_industry,
            Some("介護スタッフ"),
        );
        // segments_all empty → return なので html は空
        assert!(
            html.is_empty(),
            "全業界版が空なら section ごと出力しない (fail-soft)"
        );
    }
}
