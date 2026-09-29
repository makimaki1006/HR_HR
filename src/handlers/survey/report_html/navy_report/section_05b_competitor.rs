//! Section 05B - 競合調査 (Indeed 掲載求人のスクレイプ) — 2026-09-29 追加
//!
//! Excel 版「競合調査」ダッシュボード (仕様: claudedocs/INDEED_COMPETITOR_REPORT_SPEC_2026-09-29.md
//! §1.1 E3〜E6) の問いを、アプリ側の既存集計で答える章。読み方・換算・集計は
//! upload.rs / salary_parser.rs / aggregator.rs を正とし、Excel の式には合わせない。
//!
//! ## 構成
//! - §05B-1 相場の代表値 (E3): §03 表 3-A への参照 (重複表示しない)
//! - §05B-2 人気求人 vs 全体 (E4): 表 5B-A。Indeed (SP) 由来のみが母数。下限・上限別に
//!   平均 / 中央値と差 (人気 − 全体)。n < 5 は「n不足」。単位はモードの主単位のみ
//! - §05B-3 タグ全体の出現数 (E5): 図 5B-1 (横棒) + 表 5B-B (件数・占有率)
//! - §05B-4 検索上位 N 件のタグ (E6): 図 5B-2 + 表 5B-C。取り込み順 (重複排除後) の
//!   先頭 N 件。N は `?top_n=` (既定 45、1〜200)。分母 = min(N, 件数)
//!
//! HW データは使わない (CSV のみ)。人気タグ・タグの出現と給与・応募の関係は相関の
//! 参考であり、因果は示さない。

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::{BoundStats, SurveyAggregation, COMPETITOR_HEAD_MAX};
use super::common::push_page_head;

/// 検索上位 N 件の既定値 (Excel 版の 45 件)。
pub(crate) const TOP_N_DEFAULT: usize = 45;
/// 人気比較で値を出す最小件数 (§05 人気度と同じ)。
const N_MIN: usize = 5;
/// 図 5B-1 / 表 5B-B に出すタグの最大行数 (by_tags と同じ上位 30)。
const TAG_ROWS_MAX: usize = 30;
/// 図 5B-2 / 表 5B-C に出すタグの最大行数。
const HEAD_ROWS_MAX: usize = 30;

/// `?top_n=` の値を 1〜`COMPETITOR_HEAD_MAX` の件数に直す。
///
/// - 未指定 / 空 / 数値でない / 0 以下 → 既定 45
/// - 上限超 (例 999) → 上限 200
pub(crate) fn parse_top_n(raw: Option<&str>) -> usize {
    match raw.map(str::trim).and_then(|s| s.parse::<i64>().ok()) {
        Some(v) if v >= 1 => (v as usize).min(COMPETITOR_HEAD_MAX),
        _ => TOP_N_DEFAULT,
    }
}

/// 競合調査章を描画する。Indeed 由来の求人が 0 件なら何も出さない。
pub(crate) fn render_navy_section_competitor(
    html: &mut String,
    agg: &SurveyAggregation,
    top_n: usize,
) {
    let comp = &agg.competitor;
    if agg.total_count == 0 || comp.indeed_count == 0 {
        return;
    }
    let total = agg.total_count;

    html.push_str("<section class=\"page-navy\" id=\"navy-competitor\" role=\"region\" aria-label=\"競合調査\">\n");
    push_page_head(
        html,
        "SECTION 05B",
        "競合調査 (Indeed 掲載求人)",
        "相場の代表値・人気求人の給与・打ち出しているタグ・検索上位の求人のタグ",
    );

    let other = total.saturating_sub(comp.indeed_count);
    let other_note = if other > 0 {
        format!(
            " (うち Indeed 以外の媒体 {} 件を含む)",
            format_number(other as i64)
        )
    } else {
        String::new()
    };
    html.push_str(&format!(
        "<p class=\"note\">※ 出典: Indeed 掲載求人のスクレイプ (アップロード CSV、重複排除後 n={}{})。\
         Indeed に掲載された求人のみが対象で、求人市場全体ではありません。\
         ハローワーク等の公的統計は使っていません。</p>\n",
        format_number(total as i64),
        other_note,
    ));

    render_reference_block(html, agg);
    render_popularity_compare(html, agg);
    render_tag_ranking(html, agg);
    render_head_tags(html, agg, top_n);

    html.push_str(
        "<p class=\"note\">※ タグの出現や人気タグと給与の差は、掲載内容の傾向を示す参考値です。\
         タグや給与が応募数・人気を決めることを示すものではありません (相関であり因果ではありません)。</p>\n",
    );
    html.push_str("</section>\n");
}

// ============================================================================
// §05B-1 相場の代表値 (E3)
// ============================================================================
fn render_reference_block(html: &mut String, agg: &SurveyAggregation) {
    html.push_str("<div class=\"block-title\">§05B-1 &nbsp;相場の代表値</div>\n");
    let unit = if agg.is_hourly {
        "時給 (円/時)"
    } else {
        "月給 (万円)"
    };
    html.push_str(&format!(
        "<p class=\"note\">給与の平均・中央値・最頻値 (下限・上限別) は §03「給与分布 統計」の\
         表 3-A と図 3-2 / 3-4 に掲載しています ({} の求人が対象)。本章では重複して掲載しません。</p>\n",
        unit
    ));
}

// ============================================================================
// §05B-2 人気求人 vs 全体 (E4)
// ============================================================================
fn fmt_money(v: i64, hourly: bool) -> String {
    if hourly {
        format!("{} 円/時", format_number(v))
    } else {
        format!("{:.1} 万円", v as f64 / 10_000.0)
    }
}

fn fmt_diff(d: i64, hourly: bool) -> String {
    let sign = if d > 0 {
        "+"
    } else if d < 0 {
        "−"
    } else {
        "±"
    };
    format!("{}{}", sign, fmt_money(d.abs(), hourly))
}

/// 1 セルの値。n < N_MIN なら「— (n不足)」。
fn stat_cell(v: Option<i64>, n: usize, hourly: bool) -> String {
    if n < N_MIN {
        return "— (n不足)".to_string();
    }
    match v {
        Some(x) => fmt_money(x, hourly),
        None => "—".to_string(),
    }
}

fn diff_cell(
    pop: Option<i64>,
    pop_n: usize,
    all: Option<i64>,
    all_n: usize,
    hourly: bool,
) -> String {
    if pop_n < N_MIN || all_n < N_MIN {
        return "— (n不足)".to_string();
    }
    match (pop, all) {
        (Some(p), Some(a)) => fmt_diff(p - a, hourly),
        _ => "—".to_string(),
    }
}

fn render_popularity_compare(html: &mut String, agg: &SurveyAggregation) {
    let comp = &agg.competitor;
    let hourly = comp.pop_is_hourly;
    let all: &BoundStats = &comp.pop_all;
    let pop: &BoundStats = &comp.pop_popular;
    html.push_str("<div class=\"block-title block-title-spaced\">§05B-2 &nbsp;人気求人 vs 全体 (表 5B-A)</div>\n");
    if all.min_n == 0 && all.max_n == 0 {
        html.push_str(
            "<p class=\"note\">※ 「人気」「超人気」タグは Indeed スマートフォン版 (SP) の CSV でのみ取得できます。\
             今回の CSV には該当する給与データがないため、比較表は出していません。</p>\n",
        );
        return;
    }
    let unit_label = if hourly {
        "時給 円/時"
    } else {
        "月給 万円"
    };
    html.push_str(&format!(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;font-size:0.85em;\">\n\
         <colgroup><col style=\"width:22%;\"><col style=\"width:8%;\"><col style=\"width:14%;\">\
         <col style=\"width:14%;\"><col style=\"width:8%;\"><col style=\"width:17%;\">\
         <col style=\"width:17%;\"></colgroup>\
         <thead><tr>\
         <th rowspan=\"2\">区分</th>\
         <th colspan=\"3\" style=\"text-align:center;\">下限・{u}</th>\
         <th colspan=\"3\" style=\"text-align:center;\">上限・{u}</th>\
         </tr><tr>\
         <th style=\"text-align:right;\">n</th><th style=\"text-align:right;\">平均</th><th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">n</th><th style=\"text-align:right;\">平均</th><th style=\"text-align:right;\">中央値</th>\
         </tr></thead>\n<tbody>\n",
        u = unit_label
    ));
    let row = |label: &str, s: &BoundStats| -> String {
        format!(
            "<tr><td>{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            label,
            format_number(s.min_n as i64),
            stat_cell(s.min_mean, s.min_n, hourly),
            stat_cell(s.min_median, s.min_n, hourly),
            format_number(s.max_n as i64),
            stat_cell(s.max_mean, s.max_n, hourly),
            stat_cell(s.max_median, s.max_n, hourly),
        )
    };
    html.push_str(&row("全体 (Indeed SP 全件)", all));
    html.push_str(&row("人気求人 (人気・超人気)", pop));
    html.push_str(&format!(
        "<tr class=\"bold\"><td>差 (人気 − 全体)</td>\
         <td class=\"num\">—</td><td class=\"num\">{}</td><td class=\"num\">{}</td>\
         <td class=\"num\">—</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
        diff_cell(pop.min_mean, pop.min_n, all.min_mean, all.min_n, hourly),
        diff_cell(pop.min_median, pop.min_n, all.min_median, all.min_n, hourly),
        diff_cell(pop.max_mean, pop.max_n, all.max_mean, all.max_n, hourly),
        diff_cell(pop.max_median, pop.max_n, all.max_median, all.max_n, hourly),
    ));
    html.push_str("</tbody></table>\n");
    let unit_note = if hourly {
        "時給の求人のみ (月給・日給・年俸の求人は含めず、月給換算もしていません)"
    } else {
        "月給の求人のみ (時給・日給・年俸の求人は含めていません)"
    };
    html.push_str(&format!(
        "<p class=\"note\">※ 母数は Indeed (SP) 由来の求人。対象は{}。\
         下限と上限は別々に数えています (上限の記載がない求人は上限の n に入りません)。\
         n が 5 未満の区分は値を出さず「n不足」としています。\
         「人気」「超人気」の付与基準は非公開で、給与以外の要因も関わります。</p>\n",
        unit_note
    ));
}

// ============================================================================
// §05B-3 タグ全体の出現数ランキング (E5)
// ============================================================================
fn pct(count: usize, denom: usize) -> f64 {
    if denom == 0 {
        0.0
    } else {
        count as f64 / denom as f64 * 100.0
    }
}

fn render_tag_ranking(html: &mut String, agg: &SurveyAggregation) {
    html.push_str(
        "<div class=\"block-title block-title-spaced\">§05B-3 &nbsp;タグ全体の出現数ランキング (図 5B-1 / 表 5B-B)</div>\n",
    );
    // 2026-09-29: 占有率 = そのタグを付けた求人の割合。分子は求人単位で重複排除した件数
    //   (CompetitorAnalysis::tag_counts_all)。by_tags は出現回数 (他画面用) のため使わない。
    let all = &agg.competitor.tag_counts_all;
    let tags = &all[..all.len().min(TAG_ROWS_MAX)];
    if tags.is_empty() {
        html.push_str("<p class=\"note\">※ 今回の CSV からタグを取得できませんでした。</p>\n");
        return;
    }
    let total = agg.total_count;
    let items: Vec<(String, f64, String)> = tags
        .iter()
        .map(|(t, c)| {
            (
                t.clone(),
                *c as f64,
                format!("{} 件 ({:.1}%)", format_number(*c as i64), pct(*c, total)),
            )
        })
        .collect();
    html.push_str(&build_hbar_svg(&items, "タグの出現件数 (全体)"));

    html.push_str(
        "<table class=\"table-navy\" style=\"width:100%;font-size:0.85em;\">\n\
         <thead><tr><th style=\"text-align:right;\">順位</th><th>タグ</th>\
         <th style=\"text-align:right;\">件数</th><th style=\"text-align:right;\">占有率</th></tr></thead>\n<tbody>\n",
    );
    for (i, (t, c)) in tags.iter().enumerate() {
        html.push_str(&format!(
            "<tr><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td></tr>\n",
            i + 1,
            escape_html(t),
            format_number(*c as i64),
            pct(*c, total),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p class=\"note\">※ 件数 = そのタグを付けた求人の数 (1 求人に同じタグが複数あっても 1 件)。\
         占有率 = 件数 ÷ 重複排除後の求人数 (n={})。上位 {} タグまで表示。\
         タグは CSV のタグ列をすべて結合して数えています (Excel 版のような 9 枠の制限はありません)。</p>\n",
        format_number(total as i64),
        tags.len(),
    ));
}

// ============================================================================
// §05B-4 検索上位 N 件のタグ (E6)
// ============================================================================

/// 先頭 `top_n` 件 (取り込み順、重複排除後) のタグ出現数と分母を返す。
/// 並び: 件数降順 → タグ名昇順。
pub(crate) fn head_tag_counts(
    agg: &SurveyAggregation,
    top_n: usize,
) -> (Vec<(String, usize)>, usize) {
    let head = &agg.competitor.head_tags;
    let denom = top_n.min(agg.total_count).min(head.len());
    let mut map: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    for tags in head.iter().take(denom) {
        for t in tags {
            *map.entry(t.as_str()).or_default() += 1;
        }
    }
    let mut v: Vec<(String, usize)> = map.into_iter().map(|(k, c)| (k.to_string(), c)).collect();
    v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    (v, denom)
}

fn render_head_tags(html: &mut String, agg: &SurveyAggregation, top_n: usize) {
    let total = agg.total_count;
    let shown_n = top_n.min(total);
    html.push_str(&format!(
        "<div class=\"block-title block-title-spaced\">§05B-4 &nbsp;検索上位 {} 件のタグ (図 5B-2 / 表 5B-C)</div>\n",
        shown_n
    ));
    if agg.competitor.head_tags.is_empty() {
        html.push_str(
            "<p class=\"note\">※ この集計には CSV の再アップロードが必要です (取り込み順の情報が保存されていません)。</p>\n",
        );
        return;
    }
    let (counts, denom) = head_tag_counts(agg, top_n);
    if counts.is_empty() {
        html.push_str(&format!(
            "<p class=\"note\">※ 上位 {} 件の求人にタグがありませんでした。</p>\n",
            denom
        ));
        return;
    }
    let all_count = |tag: &str| -> Option<usize> {
        agg.competitor
            .tag_counts_all
            .iter()
            .find(|(t, _)| t == tag)
            .map(|(_, c)| *c)
    };

    let rows: Vec<&(String, usize)> = counts.iter().take(HEAD_ROWS_MAX).collect();
    let pairs: Vec<(String, f64, f64)> = rows
        .iter()
        .take(20)
        .map(|(t, c)| {
            (
                t.clone(),
                pct(*c, denom),
                all_count(t).map(|a| pct(a, total)).unwrap_or(0.0),
            )
        })
        .collect();
    html.push_str(&build_pair_hbar_svg(&pairs, denom));

    html.push_str(&format!(
        "<table class=\"table-navy\" style=\"width:100%;font-size:0.85em;\">\n\
         <thead><tr><th style=\"text-align:right;\">順位</th><th>タグ</th>\
         <th style=\"text-align:right;\">上位 {d} 件の件数</th><th style=\"text-align:right;\">上位 {d} 件の占有率</th>\
         <th style=\"text-align:right;\">全体の件数</th><th style=\"text-align:right;\">全体の占有率</th>\
         <th style=\"text-align:right;\">差 (pt)</th></tr></thead>\n<tbody>\n",
        d = denom
    ));
    for (i, (t, c)) in rows.iter().enumerate() {
        let head_pct = pct(*c, denom);
        let (all_cell, all_pct_cell, diff_cell) = match all_count(t) {
            Some(a) => {
                let ap = pct(a, total);
                let d = head_pct - ap;
                let sign = if d > 0.05 {
                    "+"
                } else if d < -0.05 {
                    "−"
                } else {
                    "±"
                };
                (
                    format_number(a as i64),
                    format!("{:.1}%", ap),
                    format!("{}{:.1}", sign, d.abs()),
                )
            }
            None => ("—".to_string(), "—".to_string(), "—".to_string()),
        };
        html.push_str(&format!(
            "<tr><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td>\
             <td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>\n",
            i + 1,
            escape_html(t),
            format_number(*c as i64),
            head_pct,
            all_cell,
            all_pct_cell,
            diff_cell,
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(&format!(
        "<p class=\"note\">※ 上位 {d} 件 = CSV の取り込み順 (重複排除後) の先頭 {d} 件。\
         スクレイプ時の検索結果の並び順をそのまま使っており、Indeed の表示順位の仕組みを表すものではありません。\
         占有率の分母は {d} 件 (全体は n={t})。差 = 上位 {d} 件の占有率 − 全体の占有率。\
         件数は URL の <code>top_n</code> (1〜{max}、既定 {def}) で変えられます。</p>\n",
        d = denom,
        t = format_number(total as i64),
        max = COMPETITOR_HEAD_MAX,
        def = TOP_N_DEFAULT,
    ));
}

// ============================================================================
// SVG (ECharts は使わない)
// ============================================================================

/// 横棒 (1 系列)。`items` = (ラベル, 値, 右端に出す文字)。
fn build_hbar_svg(items: &[(String, f64, String)], aria: &str) -> String {
    let w = 720.0f64;
    let label_w = 170.0f64;
    let value_w = 110.0f64;
    let row_h = 18.0f64;
    let top = 8.0f64;
    let h = top * 2.0 + row_h * items.len() as f64;
    let max = items.iter().map(|x| x.1).fold(0.0f64, f64::max).max(1e-9);
    let bar_max = w - label_w - value_w - 8.0;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" role=\"img\" \
         aria-label=\"{a}\" style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n",
        w = w as i64,
        h = h as i64,
        a = escape_html(aria),
    );
    for (i, (label, v, text)) in items.iter().enumerate() {
        let y = top + row_h * i as f64;
        let bw = (v / max * bar_max).max(0.0);
        s.push_str(&format!(
            "<text x=\"{lx:.1}\" y=\"{ty:.1}\" font-size=\"10\" fill=\"#1F2D4D\" text-anchor=\"end\">{l}</text>\
             <rect x=\"{bx:.1}\" y=\"{ry:.1}\" width=\"{bw:.1}\" height=\"12\" fill=\"#1F2D4D\"/>\
             <text x=\"{vx:.1}\" y=\"{ty:.1}\" font-size=\"9.5\" fill=\"#3A3F4B\">{t}</text>\n",
            lx = label_w - 6.0,
            ty = y + 12.0,
            l = escape_html(label),
            bx = label_w,
            ry = y + 3.0,
            bw = bw,
            vx = label_w + bw + 4.0,
            t = escape_html(text),
        ));
    }
    s.push_str("</svg>\n");
    s
}

/// 横棒 (2 系列: 上位 N 件の占有率 / 全体の占有率)。値は % (0〜100)。
fn build_pair_hbar_svg(items: &[(String, f64, f64)], denom: usize) -> String {
    let w = 720.0f64;
    let label_w = 170.0f64;
    let value_w = 60.0f64;
    let row_h = 24.0f64;
    let top = 26.0f64;
    let h = top + 8.0 + row_h * items.len() as f64;
    let max = items
        .iter()
        .map(|x| x.1.max(x.2))
        .fold(0.0f64, f64::max)
        .max(1e-9);
    let bar_max = w - label_w - value_w - 8.0;
    let mut s = format!(
        "<svg viewBox=\"0 0 {w} {h}\" width=\"100%\" preserveAspectRatio=\"xMidYMid meet\" role=\"img\" \
         aria-label=\"上位 {d} 件と全体のタグ占有率\" style=\"display:block;background:var(--paper-pure);border:1px solid var(--rule-soft);\">\n\
         <rect x=\"{lx}\" y=\"8\" width=\"10\" height=\"8\" fill=\"#1F2D4D\"/>\
         <text x=\"{lx2}\" y=\"16\" font-size=\"9.5\" fill=\"#3A3F4B\">上位 {d} 件の占有率</text>\
         <rect x=\"{gx}\" y=\"8\" width=\"10\" height=\"8\" fill=\"#B9A26B\"/>\
         <text x=\"{gx2}\" y=\"16\" font-size=\"9.5\" fill=\"#3A3F4B\">全体の占有率</text>\n",
        w = w as i64,
        h = h as i64,
        d = denom,
        lx = label_w as i64,
        lx2 = label_w as i64 + 14,
        gx = label_w as i64 + 150,
        gx2 = label_w as i64 + 164,
    );
    for (i, (label, head, all)) in items.iter().enumerate() {
        let y = top + row_h * i as f64;
        let hw = head / max * bar_max;
        let aw = all / max * bar_max;
        s.push_str(&format!(
            "<text x=\"{lx:.1}\" y=\"{ty:.1}\" font-size=\"10\" fill=\"#1F2D4D\" text-anchor=\"end\">{l}</text>\
             <rect x=\"{bx:.1}\" y=\"{y1:.1}\" width=\"{hw:.1}\" height=\"9\" fill=\"#1F2D4D\"/>\
             <text x=\"{hx:.1}\" y=\"{y1t:.1}\" font-size=\"9\" fill=\"#3A3F4B\">{hv:.1}%</text>\
             <rect x=\"{bx:.1}\" y=\"{y2:.1}\" width=\"{aw:.1}\" height=\"9\" fill=\"#B9A26B\"/>\
             <text x=\"{ax:.1}\" y=\"{y2t:.1}\" font-size=\"9\" fill=\"#3A3F4B\">{av:.1}%</text>\n",
            lx = label_w - 6.0,
            ty = y + 14.0,
            l = escape_html(label),
            bx = label_w,
            y1 = y + 2.0,
            y1t = y + 10.0,
            hw = hw,
            hx = label_w + hw + 4.0,
            hv = head,
            y2 = y + 12.0,
            y2t = y + 20.0,
            aw = aw,
            ax = label_w + aw + 4.0,
            av = all,
        ));
    }
    s.push_str("</svg>\n");
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{aggregate_records, salary_fixture};
    use crate::handlers::survey::upload::SurveyRecord;

    /// 給与原文 1 行 1 求人 + タグ (Indeed SP)。row_index は行順。
    fn recs(rows: &[(&str, &str)]) -> Vec<SurveyRecord> {
        let lines: Vec<&str> = rows.iter().map(|(s, _)| *s).collect();
        let mut v = salary_fixture::records(&lines.join("\n"), "東京都 板橋区");
        for (r, (_, tags)) in v.iter_mut().zip(rows.iter()) {
            r.tags_raw = tags.to_string();
        }
        v
    }

    fn render(agg: &SurveyAggregation, top_n: usize) -> String {
        let mut html = String::new();
        render_navy_section_competitor(&mut html, agg, top_n);
        html
    }

    #[test]
    fn parse_top_n_clamps_out_of_range() {
        assert_eq!(parse_top_n(None), 45);
        assert_eq!(parse_top_n(Some("")), 45);
        assert_eq!(parse_top_n(Some("0")), 45);
        assert_eq!(parse_top_n(Some("-3")), 45);
        assert_eq!(parse_top_n(Some("abc")), 45);
        assert_eq!(parse_top_n(Some("999")), 200);
        assert_eq!(parse_top_n(Some("200")), 200);
        assert_eq!(parse_top_n(Some("1")), 1);
        assert_eq!(parse_top_n(Some(" 10 ")), 10);
    }

    /// 5 件中「交通費支給」4 件 → 占有率 80.0% (分母 = 重複排除後の求人数 5)
    #[test]
    fn tag_share_uses_dedup_record_count() {
        let s = "月給 25万円 ~ 30万円";
        let r = recs(&[
            (s, "交通費支給,未経験歓迎"),
            (s, "交通費支給"),
            (s, "交通費支給,資格不問"),
            (s, "交通費支給"),
            (s, "資格不問"),
        ]);
        let agg = aggregate_records(&r);
        let html = render(&agg, 45);
        assert!(html.contains("SECTION 05B"));
        assert!(html.contains("Indeed 掲載求人のスクレイプ"), "出典の明記");
        assert!(
            html.contains(
                "<tr><td class=\"num\">1</td><td>交通費支給</td><td class=\"num\">4</td><td class=\"num\">80.0%</td></tr>"
            ),
            "表 5B-B: 交通費支給 4 件 80.0%"
        );
        assert!(
            html.contains("<td>資格不問</td><td class=\"num\">2</td><td class=\"num\">40.0%</td>"),
            "表 5B-B: 資格不問 2 件 40.0%"
        );
        assert!(html.contains("4 件 (80.0%)"), "図 5B-1 の値ラベル");
    }

    /// 上位 N: row_index 順の先頭 N 件だけを数え、N+1 件目のタグは入らない。
    /// records の並びを逆順にしても row_index で並べ直して数える。
    #[test]
    fn head_counts_only_first_n_by_row_index() {
        let s = "月給 25万円 ~ 30万円";
        let mut rows: Vec<(&str, &str)> = vec![(s, "上位タグ"); 45];
        rows.push((s, "46件目タグ"));
        rows.extend(vec![(s, "下位タグ"); 4]);
        let mut r = recs(&rows);
        r.reverse();
        let agg = aggregate_records(&r);
        let (counts, denom) = head_tag_counts(&agg, 45);
        assert_eq!(denom, 45);
        assert_eq!(counts, vec![("上位タグ".to_string(), 45)]);
        let (counts46, denom46) = head_tag_counts(&agg, 46);
        assert_eq!(denom46, 46);
        assert!(counts46.contains(&("46件目タグ".to_string(), 1)));

        let html = render(&agg, 45);
        let start = html.find("§05B-4").unwrap();
        let sec = &html[start..];
        assert!(
            !sec.contains("46件目タグ"),
            "46 件目のタグが上位 45 に入っている"
        );
        // 上位 45 件の占有率 100.0%、全体 (50 件中 45 件) 90.0%、差 +10.0pt
        assert!(sec.contains(
            "<td>上位タグ</td><td class=\"num\">45</td><td class=\"num\">100.0%</td>\
             <td class=\"num\">45</td><td class=\"num\">90.0%</td><td class=\"num\">+10.0</td>"
        ));
    }

    /// N > 件数 のとき分母は件数 (10)。
    #[test]
    fn head_denominator_is_record_count_when_n_exceeds() {
        let s = "月給 25万円 ~ 30万円";
        let mut rows: Vec<(&str, &str)> = vec![(s, "駅近"); 3];
        rows.extend(vec![(s, "賞与あり"); 7]);
        let agg = aggregate_records(&recs(&rows));
        let (_, denom) = head_tag_counts(&agg, 45);
        assert_eq!(denom, 10);
        let html = render(&agg, 45);
        assert!(html.contains("検索上位 10 件のタグ"));
        assert!(html.contains("上位 10 件の占有率"));
        assert!(html.contains("<td>駅近</td><td class=\"num\">3</td><td class=\"num\">30.0%</td>"));
    }

    fn monthly_rows(popular: usize) -> Vec<(String, String)> {
        // 下限 20〜29 万円、上限 = 下限 + 5 万円。末尾 popular 件に人気系タグ。
        (0..10)
            .map(|i| {
                let lo = 20 + i;
                let tag = if i >= 10 - popular {
                    if i % 2 == 0 {
                        "人気,交通費支給"
                    } else {
                        "超人気"
                    }
                } else {
                    "交通費支給"
                };
                (format!("月給 {}万円 ~ {}万円", lo, lo + 5), tag.to_string())
            })
            .collect()
    }

    fn agg_of(rows: &[(String, String)]) -> SurveyAggregation {
        let r: Vec<(&str, &str)> = rows.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        aggregate_records(&recs(&r))
    }

    /// 人気 3 件 / 全体 10 件: 平均・中央値が手計算と一致。人気 n<5 は「n不足」。
    #[test]
    fn popularity_compare_hand_calc_and_n_shortage() {
        let agg = agg_of(&monthly_rows(3));
        let c = &agg.competitor;
        assert!(!c.pop_is_hourly);
        // 全体: 下限 20..29 万 → 平均・中央値 24.5 万、上限 25..34 万 → 29.5 万
        assert_eq!(
            c.pop_all,
            BoundStats {
                min_n: 10,
                min_mean: Some(245_000),
                min_median: Some(245_000),
                max_n: 10,
                max_mean: Some(295_000),
                max_median: Some(295_000),
            }
        );
        // 人気 (27/28/29 万): 平均・中央値 28 万、上限 33 万
        assert_eq!(
            c.pop_popular,
            BoundStats {
                min_n: 3,
                min_mean: Some(280_000),
                min_median: Some(280_000),
                max_n: 3,
                max_mean: Some(330_000),
                max_median: Some(330_000),
            }
        );
        let html = render(&agg, 45);
        assert!(html.contains(
            "<td>全体 (Indeed SP 全件)</td><td class=\"num\">10</td>\
             <td class=\"num\">24.5 万円</td><td class=\"num\">24.5 万円</td>"
        ));
        assert!(html.contains(
            "<td>人気求人 (人気・超人気)</td><td class=\"num\">3</td><td class=\"num\">— (n不足)</td>"
        ));
        assert!(html.contains(
            "<td>差 (人気 − 全体)</td><td class=\"num\">—</td><td class=\"num\">— (n不足)</td>"
        ));
    }

    /// 人気 5 件 / 全体 10 件: 差 = 27.0 − 24.5 = +2.5 万円 (下限)、上限 32.0 − 29.5 = +2.5 万円
    #[test]
    fn popularity_compare_diff_values() {
        let agg = agg_of(&monthly_rows(5));
        assert_eq!(agg.competitor.pop_popular.min_mean, Some(270_000));
        let html = render(&agg, 45);
        assert!(html.contains(
            "<td>人気求人 (人気・超人気)</td><td class=\"num\">5</td>\
             <td class=\"num\">27.0 万円</td><td class=\"num\">27.0 万円</td><td class=\"num\">5</td>\
             <td class=\"num\">32.0 万円</td><td class=\"num\">32.0 万円</td>"
        ));
        assert!(html.contains(
            "<td>差 (人気 − 全体)</td><td class=\"num\">—</td><td class=\"num\">+2.5 万円</td>\
             <td class=\"num\">+2.5 万円</td><td class=\"num\">—</td><td class=\"num\">+2.5 万円</td>\
             <td class=\"num\">+2.5 万円</td>"
        ));
    }

    /// 時給モード: 時給求人の円/時だけで比較し、月給換算値 (×167) や月給求人を混ぜない。
    #[test]
    fn popularity_compare_hourly_mode_uses_native_hourly() {
        let mut rows: Vec<(String, String)> = (0..10)
            .map(|i| {
                let lo = 1000 + i * 10;
                let tag = if i >= 5 { "人気" } else { "" };
                (format!("時給 {}円 ~ {}円", lo, lo + 200), tag.to_string())
            })
            .collect();
        rows.push(("月給 30万円 ~ 35万円".to_string(), "人気".to_string()));
        let agg = agg_of(&rows);
        assert!(agg.is_hourly);
        let c = &agg.competitor;
        assert!(c.pop_is_hourly);
        // 全体 下限 1000..1090 → 平均 1045。月給 30 万の 1 件は入らない
        assert_eq!(c.pop_all.min_n, 10);
        assert_eq!(c.pop_all.min_mean, Some(1045));
        // 人気 1050..1090 → 1070
        assert_eq!(c.pop_popular.min_n, 5);
        assert_eq!(c.pop_popular.min_mean, Some(1070));
        let html = render(&agg, 45);
        let sec = &html[html.find("§05B-2").unwrap()..html.find("§05B-3").unwrap()];
        assert!(sec.contains("下限・時給 円/時"));
        assert!(sec.contains("<td class=\"num\">1,045 円/時</td>"));
        assert!(
            sec.contains("<td class=\"num\">+25 円/時</td>"),
            "差 1070 − 1045"
        );
        // 月給換算値 1045×167=174,515 や月給値が円/時の表に出ないこと
        assert!(!sec.contains("174,515"));
        assert!(!sec.contains("万円</td>"));
        assert!(!sec.contains("300,000"));
    }

    /// 同じ求人に同じタグが 2 回あっても 1 件 (占有率 = そのタグを付けた求人の割合)。
    #[test]
    fn duplicate_tag_in_one_record_counts_once() {
        let s = "月給 25万円 ~ 30万円";
        let agg = aggregate_records(&recs(&[(s, "交通費支給,交通費支給"), (s, "資格不問")]));
        assert!(agg
            .competitor
            .tag_counts_all
            .contains(&("交通費支給".to_string(), 1)));
        assert_eq!(
            head_tag_counts(&agg, 45).0[0],
            ("交通費支給".to_string(), 1)
        );
        let html = render(&agg, 45);
        assert!(html.contains(
            "<td>交通費支給</td><td class=\"num\">1</td><td class=\"num\">50.0%</td></tr>"
        ));
        assert!(
            !html.contains("100.0%</td>"),
            "2 件中 1 件なので 100% にならない"
        );
    }

    /// 表 5B-B / 5B-C の占有率 (%) をすべて取り出す。
    fn share_cells(html: &str) -> Vec<f64> {
        let sec = &html[html.find("§05B-3").unwrap()..];
        sec.split("<td class=\"num\">")
            .skip(1)
            .filter_map(|c| c.split("%</td>").next().filter(|v| !v.contains('<')))
            .filter_map(|v| v.parse::<f64>().ok())
            .collect()
    }

    /// 不変条件: 全タグの占有率は 0〜100%。実データの給与原文 (f1: 時給 751 行 /
    /// f2: 月給 567 + 年俸 24 行) に、同一求人内の重複タグを含むタグを付けて確認する。
    #[test]
    fn all_tag_shares_within_0_100() {
        for (lines, loc) in [
            (salary_fixture::F1_HOURLY_MOSTLY, "東京都 板橋区"),
            (salary_fixture::F2_MONTHLY_WITH_ANNUAL, "大阪府 大阪市"),
        ] {
            let mut r = salary_fixture::records(lines, loc);
            for (i, rec) in r.iter_mut().enumerate() {
                rec.tags_raw = match i % 4 {
                    0 => "交通費支給,交通費支給,駅近",
                    1 => "交通費支給,資格不問,資格不問",
                    2 => "交通費支給、交通費支給/駅近",
                    _ => "交通費支給",
                }
                .to_string();
            }
            let agg = aggregate_records(&r);
            let total = agg.total_count;
            for (t, c) in &agg.competitor.tag_counts_all {
                assert!(*c <= total, "{t}: {c} > {total}");
            }
            for top_n in [1, 45, 200] {
                let (counts, denom) = head_tag_counts(&agg, top_n);
                for (t, c) in counts {
                    assert!(c <= denom, "{t}: {c} > {denom} (top_n={top_n})");
                }
                let shares = share_cells(&render(&agg, top_n));
                assert!(!shares.is_empty());
                for v in shares {
                    assert!((0.0..=100.0).contains(&v), "占有率 {v}% (top_n={top_n})");
                }
            }
            // 全件に付いている「交通費支給」は 100.0% ちょうど
            assert_eq!(
                agg.competitor.tag_counts_all[0],
                ("交通費支給".to_string(), total)
            );
        }
    }

    /// Indeed 以外 (求人ボックス等) だけの CSV では章を出さない。
    #[test]
    fn skipped_without_indeed_records() {
        let mut r = recs(&[("月給 25万円 ~ 30万円", "交通費支給")]);
        r[0].source = crate::handlers::survey::upload::CsvSource::JobBox;
        let agg = aggregate_records(&r);
        assert_eq!(agg.competitor.indeed_count, 0);
        assert!(render(&agg, 45).is_empty());
    }

    /// 全 variant で章と目次が出る (CSV のみの章、HW 不使用)。
    #[test]
    fn chapter_in_all_variants_with_toc() {
        use super::super::super::{render_survey_report_page_for_vrt, ReportVariant};
        use crate::handlers::survey::job_seeker::analyze_job_seeker;
        let r = recs(&[("月給 25万円 ~ 30万円", "交通費支給"); 6]);
        let agg = aggregate_records(&r);
        let seeker = analyze_job_seeker(&r);
        for v in [
            ReportVariant::Full,
            ReportVariant::Public,
            ReportVariant::MarketIntelligence,
            ReportVariant::Extended,
            ReportVariant::Sp,
            ReportVariant::Ver10,
        ] {
            let html = render_survey_report_page_for_vrt(
                &agg,
                &seeker,
                &agg.by_company,
                &agg.by_emp_type_salary,
                &agg.salary_min_values,
                &agg.salary_max_values,
                None,
                v,
            );
            assert!(html.contains("SECTION 05B"), "{:?}", v);
            assert!(
                html.contains("<span class=\"t-no\">05B</span>"),
                "{:?} 目次",
                v
            );
        }
    }
}
