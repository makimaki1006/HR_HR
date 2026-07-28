//! Section 05 - 人気度シグナル / Indeed (SP) 表示優先度スコア 集計
//!
//! Indeed (SP) スマートフォン版 CSV の `css-u74ql7` 列から抽出した
//! 「人気」「超人気」タグの集計を表示する。Indeed (SP) 以外のソースでは
//! 全件タグなしになるため、`popular_count + super_popular_count == 0` で
//! セクションごとスキップする。
//!
//! ## 構成
//! - §05-1 サマリー: 件数 / 比率 KPI 5 枚
//! - §05-2 月給 比較: 人気タグ あり vs なし の中央値比較
//! - §05-3 人気タグ別 給与統計: 超人気/人気/タグなし の下限・上限 平均/中央値/最頻値
//! - §05-4 人気タグ別 年間休日統計: 超人気/人気/タグなし の 件数/中央値/平均/P25/P75
//!   (2026-07-28 復活。集計層に 3 区分別の HolidayStats を追加して実装)
//!
//! ## 設計メモ
//! - 「人気タグ」は Indeed 内部の表示優先度スコアにすぎず、給与差・休日差は
//!   直接的な因果関係を意味しない (相関≠因果)。so-what は最小限に留める。

// 一部 helper 関数は test 用、または将来拡張のために定義済み (使用されていないものは dead_code)
#![allow(dead_code)]

use super::super::super::super::helpers::{escape_html, format_number};
use super::super::super::aggregator::{HolidayStats, SalaryStats, SurveyAggregation};
use super::common::{push_kpi_card_simple, push_page_head};

/// 人気度シグナル セクションを描画。
///
/// `agg.popularity.popular_count == 0 && agg.popularity.super_popular_count == 0`
/// ならスキップ (Indeed SP 以外 / 集計対象なし)。
pub(crate) fn render_navy_section_popularity(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    if pop.popular_count == 0 && pop.super_popular_count == 0 {
        return;
    }

    html.push_str("<section class=\"page-navy navy-popularity\" role=\"region\">\n");
    push_page_head(
        html,
        "SECTION 05",
        "人気度シグナル",
        "「人気」「超人気」ラベルの集計 — 付与基準は非公開の参考指標",
    );

    render_summary_kpi(html, agg);
    render_comparison_block(html, agg);
    render_salary_stats_block(html, agg);
    render_holiday_stats_block(html, agg);

    // Finding #9 (2026-07-01): 印刷崩れ対策 — .navy-popularity スコープで改ページ制御
    // rank5 fix: table セレクタを除去し .kpi-row のみ残す (table は別ページ跨ぎを許容)
    html.push_str(
        "<style>\
         @media print {\
           .navy-popularity .kpi-row { break-inside: avoid; page-break-inside: avoid; }\
         }\
         </style>\n",
    );

    html.push_str("</section>\n");
}

// ============================================================================
// §05-1 サマリー KPI
// ============================================================================
fn render_summary_kpi(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    html.push_str("<div class=\"block-title\">§05-1 &nbsp;サマリー</div>\n");
    html.push_str("<div class=\"kpi-row\">\n");

    push_kpi_card_simple(
        html,
        "人気タグ件数",
        &format!("{} 件", format_number(pop.popular_count as i64)),
        "「人気」ラベル付与",
    );
    push_kpi_card_simple(
        html,
        "超人気タグ件数",
        &format!("{} 件", format_number(pop.super_popular_count as i64)),
        "「超人気」ラベル付与",
    );
    // 2026-07-01 Finding #2: 分母を IndeedSp 由来件数に明示。
    push_kpi_card_simple(
        html,
        "人気タグ比率",
        &format!("{:.1}%", pop.popular_ratio * 100.0),
        &format!(
            "対象求人 {} 件中 (人気+超人気)",
            format_number(pop.indeed_sp_total as i64)
        ),
    );

    // 月給差 (人気あり - なし) を補助 KPI として表示
    // Finding #5 (2026-07-01): 両群 n >= 5 を満たさない場合は "— (n不足)" に
    const N_MIN: usize = 5;
    // Finding #8 (2026-07-01): 月給差を万円表示に変更 (6-7 桁オーバーフロー解消)。
    let salary_diff_text = if pop.popular_n_salary >= N_MIN && pop.non_popular_n_salary >= N_MIN {
        match (pop.popular_salary_median, pop.non_popular_salary_median) {
            (Some(p), Some(n)) => {
                let diff = p - n;
                let diff_man = diff as f64 / 10_000.0;
                let sign = if diff >= 0 { "+" } else { "" };
                format!("{}{:.1} 万円", sign, diff_man)
            }
            _ => "—".to_string(),
        }
    } else {
        "— (n不足)".to_string()
    };
    let salary_diff_foot = format!(
        "人気タグ あり − なし (Monthly のみ) / 人気 n={} / なし n={}",
        pop.popular_n_salary, pop.non_popular_n_salary
    );
    push_kpi_card_simple(html, "月給中央値差", &salary_diff_text, &salary_diff_foot);

    // 2026-07-27 item14: 年間休日は人気タグとの 3 区分 (超人気/人気/タグなし) 別集計が
    //   集計層に無く、人気との紐づけが読み取れない 2 区分比較は誤解を生むため §05 から
    //   外した (給与は §05-3 で 3 区分別に提示)。年間休日の KPI カード / 比較行は非表示。

    html.push_str("</div>\n");
    // rank8: 超人気逆転の注記 + 効果約束の緩和
    html.push_str(
        "<p class=\"note\">※ 超人気タグは n が小さい場合が多く、\
         下限中央値がタグなしを下回ることがあります。\
         月給差は相関の参考値であり、因果関係および一貫した正の関係を示すものではありません。</p>\n",
    );
    // 2026-07-27 item13: 人気・超人気の付与要因は給料・年間休日に限らない旨を明記。
    html.push_str(
        "<p class=\"note\">※ 「人気」「超人気」の付与には、給料や年間休日以外にも\
         多くの要因(掲載内容・応募状況・閲覧動向など)が関わります。\
         給与差だけで人気の理由を説明できるものではありません。</p>\n",
    );
}

// ============================================================================
// §05-2 月給・年間休日 比較
// ============================================================================
fn render_comparison_block(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    // 比較可能な指標が 1 つもなければスキップ
    // 2026-07-27 item14: 年間休日比較は人気タグとの紐づけが読めないため §05 から除外。
    let has_salary = pop.popular_salary_median.is_some() || pop.non_popular_salary_median.is_some();
    if !has_salary {
        return;
    }

    // Finding #5 (2026-07-01): n < 5 の場合は値非表示 (n 数は列ヘッダに併記)
    const N_MIN_TABLE: usize = 5;
    html.push_str("<div class=\"block-title\">§05-2 &nbsp;月給 比較 (中央値)</div>\n");

    // rank29: ヘッダの単一 n を廃止。各指標行に実 n を個別に併記する。
    html.push_str(
        "<table class=\"table-navy\" style=\"table-layout:fixed;width:100%;\">\n\
         <colgroup>\
         <col style=\"width:30%;\">\
         <col style=\"width:35%;\">\
         <col style=\"width:35%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>指標</th>\
         <th style=\"text-align:right;\">人気タグ あり</th>\
         <th style=\"text-align:right;\">人気タグ なし</th>\
         </tr></thead>\n<tbody>\n",
    );

    if has_salary {
        // rank29: 各行に対応する実 n を併記する
        let pop_val = if pop.popular_n_salary >= N_MIN_TABLE {
            format!(
                "{} (n={})",
                format_salary_yen(pop.popular_salary_median),
                pop.popular_n_salary
            )
        } else {
            format!("— (n={})", pop.popular_n_salary)
        };
        let non_val = if pop.non_popular_n_salary >= N_MIN_TABLE {
            format!(
                "{} (n={})",
                format_salary_yen(pop.non_popular_salary_median),
                pop.non_popular_n_salary
            )
        } else {
            format!("— (n={})", pop.non_popular_n_salary)
        };
        html.push_str(&format!(
            "<tr>\
             <td>月給 中央値 (Monthly のみ)</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
             </tr>\n",
            pop_val, non_val,
        ));
    }
    // 2026-07-27 item14: 年間休日行は §05 から除外 (人気タグとの 3 区分別集計が無く
    //   2 区分比較では人気との紐づけが読めないため)。
    html.push_str("</tbody></table>\n");

    // so-what は相関≠因果リスク回避のため最小限 / rank8: 表記を断定から中立化
    html.push_str(
        "<p class=\"note\">※ 「人気」「超人気」は求人掲載に付与されるラベル(付与基準は非公開)。\
         給与との差分は相関の参考値であり、因果関係は示しません。\
         超人気タグ(n が小さい)は下限中央値がタグなしより低い場合があります。</p>\n",
    );
}

// ============================================================================
// §05-3 人気タグ別 給与統計 (月給下限・上限 の 平均/中央値/最頻値)
// ============================================================================

/// §05-3 を描画。3 グループ全て n=0 なら全体スキップ。
fn render_salary_stats_block(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    let sp = &pop.super_popular_salary_stats;
    let pp = &pop.popular_salary_stats;
    let np = &pop.non_popular_salary_stats;

    // 3 グループ全て n=0 なら スキップ
    if sp.n == 0 && pp.n == 0 && np.n == 0 {
        return;
    }

    html.push_str(
        "<div class=\"block-title\">\
         §05-3 &nbsp;人気タグ別 給与統計 (月給下限・上限 の 平均/中央値/最頻値)\
         </div>\n",
    );

    html.push_str(
        "<table class=\"table-navy\" \
         style=\"table-layout:fixed;width:100%;font-size:0.82em;\">\n\
         <colgroup>\
         <col style=\"width:13%;\">\
         <col style=\"width:8%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:13%;\">\
         <col style=\"width:14%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th rowspan=\"2\">グループ</th>\
         <th rowspan=\"2\" style=\"text-align:right;\">n</th>\
         <th colspan=\"3\" style=\"text-align:center;\">下限 (月給)</th>\
         <th colspan=\"3\" style=\"text-align:center;\">上限 (月給)</th>\
         </tr>\
         <tr>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">最頻値</th>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">最頻値</th>\
         </tr></thead>\n<tbody>\n",
    );

    // グループ行ラベル
    let groups: &[(&str, &SalaryStats)] = &[("超人気", sp), ("人気", pp), ("タグなし", np)];
    for (label, stats) in groups {
        if stats.n == 0 {
            html.push_str(&format!(
                "<tr style=\"color:#9ca3af;\">\
                 <td>{}</td>\
                 <td style=\"text-align:right;\">0</td>\
                 <td colspan=\"6\" style=\"text-align:center;\">— (n=0)</td>\
                 </tr>\n",
                escape_html(label),
            ));
        } else {
            let fmt = |v: Option<i64>| -> String {
                match v {
                    Some(x) => format!("{:.1} 万円", x as f64 / 10_000.0),
                    None => "—".to_string(),
                }
            };
            html.push_str(&format!(
                "<tr>\
                 <td>{}</td>\
                 <td style=\"text-align:right;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 </tr>\n",
                escape_html(label),
                format_number(stats.n as i64),
                fmt(stats.min_mean),
                fmt(stats.min_median),
                fmt(stats.min_mode),
                fmt(stats.max_mean),
                fmt(stats.max_median),
                fmt(stats.max_mode),
            ));
        }
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p class=\"note\">※ 月給 Monthly 給与のみ対象。\
         下限・上限は求人掲載の給与範囲を示します (同一求人でも下限のみ掲載の場合あり)。\
         最頻値は 5 万円刻みビン集計。</p>\n",
    );
}

// ============================================================================
// §05-4 人気タグ別 年間休日統計 (件数・中央値・平均・P25/P75)
// ============================================================================

/// 表示側で「件数僅少」と注記する閾値 (この未満は参考値扱い)。
const HOLIDAY_N_MIN: usize = 5;

/// §05-4 を描画。3 グループ全て n=0 なら全体スキップ。
///
/// 給与側 (§05-3) と同じ 3 区分 (超人気/人気/タグなし) の表構造で、
/// 年間休日の 件数・中央値・平均・P25・P75 を並べる。各区分の分母は
/// 「年間休日を抽出できた件数」であり、抽出できなかった求人は除外している
/// (分母の透明性のため n を明示)。
fn render_holiday_stats_block(html: &mut String, agg: &SurveyAggregation) {
    let pop = &agg.popularity;
    let sp = &pop.super_popular_holiday_stats;
    let pp = &pop.popular_holiday_stats;
    let np = &pop.non_popular_holiday_stats;

    // 3 グループ全て n=0 なら スキップ (年間休日を 1 件も抽出できていない)
    if sp.n == 0 && pp.n == 0 && np.n == 0 {
        return;
    }

    html.push_str(
        "<div class=\"block-title\">\
         §05-4 &nbsp;人気タグ別 年間休日統計 (件数・中央値・平均・四分位)\
         </div>\n",
    );

    html.push_str(
        "<table class=\"table-navy\" \
         style=\"table-layout:fixed;width:100%;font-size:0.82em;\">\n\
         <colgroup>\
         <col style=\"width:16%;\">\
         <col style=\"width:20%;\">\
         <col style=\"width:16%;\">\
         <col style=\"width:16%;\">\
         <col style=\"width:16%;\">\
         <col style=\"width:16%;\">\
         </colgroup>\n\
         <thead><tr>\
         <th>グループ</th>\
         <th style=\"text-align:right;\">休日データあり件数</th>\
         <th style=\"text-align:right;\">中央値</th>\
         <th style=\"text-align:right;\">平均</th>\
         <th style=\"text-align:right;\">P25</th>\
         <th style=\"text-align:right;\">P75</th>\
         </tr></thead>\n<tbody>\n",
    );

    let groups: &[(&str, &HolidayStats)] = &[("超人気", sp), ("人気", pp), ("タグなし", np)];
    for (label, stats) in groups {
        if stats.n == 0 {
            html.push_str(&format!(
                "<tr style=\"color:#9ca3af;\">\
                 <td>{}</td>\
                 <td style=\"text-align:right;\">0</td>\
                 <td colspan=\"4\" style=\"text-align:center;\">— (データなし)</td>\
                 </tr>\n",
                escape_html(label),
            ));
        } else {
            // n < 5 は参考値 (件数僅少) として件数セルに注記を添える。
            let n_cell = if stats.n < HOLIDAY_N_MIN {
                format!("{} 件<br><span style=\"font-size:0.8em;color:#9ca3af;\">参考値(件数僅少)</span>",
                    format_number(stats.n as i64))
            } else {
                format!("{} 件", format_number(stats.n as i64))
            };
            html.push_str(&format!(
                "<tr>\
                 <td>{}</td>\
                 <td style=\"text-align:right;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 <td style=\"text-align:right;white-space:nowrap;\">{}</td>\
                 </tr>\n",
                escape_html(label),
                n_cell,
                format_days(stats.median),
                format_days(stats.mean),
                format_days(stats.p25),
                format_days(stats.p75),
            ));
        }
    }
    html.push_str("</tbody></table>\n");

    // 中立表現: 超人気/人気 の中央値をタグなしと比較 (両群 n>=5 のときのみ)。
    // 差の有無を「確認できました/できませんでした」型で記述し、因果は断定しない。
    let cmp_note = build_holiday_comparison_note(sp, pp, np);
    if !cmp_note.is_empty() {
        html.push_str(&format!("<p class=\"note\">{}</p>\n", cmp_note));
    }

    // 分母の透明性 + 因果を断定しない注記。
    html.push_str(
        "<p class=\"note\">※ 各区分の分母は年間休日を抽出できた求人のみ (抽出できなかった求人は除外)。\
         P25/P75 は第 1・第 3 四分位 (日)。件数が少ない区分 (5 件未満) は参考値です。</p>\n",
    );
    // 2026-07-27 item13 の注記を維持: 人気・超人気の付与要因は給料・年間休日に限らない。
    html.push_str(
        "<p class=\"note\">※ 「人気」「超人気」の付与には、給料や年間休日以外にも\
         多くの要因(掲載内容・応募状況・閲覧動向など)が関わります。\
         年間休日の差だけで人気の理由を説明できるものではありません。</p>\n",
    );
}

/// 超人気/人気 の年間休日中央値をタグなしと比較し、中立的な差分注記を組み立てる。
///
/// - 比較対象は両群とも n >= `HOLIDAY_N_MIN` かつ中央値が Some のときのみ。
/// - 差があれば「+N 日／−N 日の差が確認できました」、同値なら「差は確認できませんでした」。
/// - 高低の理由 (因果) には言及しない。比較不能なら空文字を返す (注記なし)。
fn build_holiday_comparison_note(
    sp: &HolidayStats,
    pp: &HolidayStats,
    np: &HolidayStats,
) -> String {
    // タグなしの中央値が比較基準。n 不足または欠損なら比較しない。
    let base = match (np.n >= HOLIDAY_N_MIN, np.median) {
        (true, Some(v)) => v,
        _ => return String::new(),
    };
    let mut parts: Vec<String> = Vec::new();
    for (label, stats) in [("超人気", sp), ("人気", pp)] {
        if stats.n >= HOLIDAY_N_MIN {
            if let Some(v) = stats.median {
                let diff = v - base;
                if diff == 0 {
                    parts.push(format!("{}区分はタグなしと中央値の差は確認できませんでした", label));
                } else {
                    let sign = if diff > 0 { "+" } else { "−" };
                    parts.push(format!(
                        "{}区分はタグなしと比べ中央値で {}{} 日の差が確認できました",
                        label,
                        sign,
                        diff.abs()
                    ));
                }
            }
        }
    }
    if parts.is_empty() {
        String::new()
    } else {
        format!("※ {}(相関の参考値であり、差の理由・因果は示しません)。", parts.join("、"))
    }
}

// Finding #8 (2026-07-01): 月給中央値を万円表示に変更 (§05-2 比較表も統一)。
fn format_salary_yen(v: Option<i64>) -> String {
    match v {
        Some(x) => format!("{:.1} 万円", x as f64 / 10_000.0),
        None => "—".to_string(),
    }
}

fn format_days(v: Option<i64>) -> String {
    match v {
        Some(x) => format!("{} 日", x),
        None => "—".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::super::super::super::aggregator::PopularityAnalysis;
    use super::*;

    fn agg_with_popularity() -> SurveyAggregation {
        SurveyAggregation {
            total_count: 20,
            popularity: PopularityAnalysis {
                popular_count: 4,
                super_popular_count: 2,
                none_count: 14,
                popular_ratio: 6.0 / 20.0,
                indeed_sp_total: 20,
                popular_salary_median: Some(280_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: Some(120),
                non_popular_holidays_median: Some(110),
                // Finding #5 (2026-07-01): n >= 5 で正常表示されることを検証
                popular_n_salary: 6,
                non_popular_n_salary: 14,
                popular_n_holidays: 6,
                non_popular_n_holidays: 14,
                ..Default::default()
            },
            ..Default::default()
        }
    }

    #[test]
    fn renders_full_section_with_popularity() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &agg_with_popularity());
        assert!(html.contains("SECTION 05"));
        assert!(html.contains("§05-1"));
        assert!(html.contains("§05-2"));
        assert!(html.contains("人気タグ件数"));
        assert!(html.contains("超人気タグ件数"));
        // 30% (6/20) を含む
        assert!(html.contains("30.0%"), "popular_ratio formatted");
        // Finding #8: 月給差は万円表示 (+2.0 万円)
        assert!(html.contains("+2.0 万円"), "salary diff in manyen");
        // 2026-07-27 item14: 年間休日の KPI/比較行は §05 から除外したため表示されない。
        assert!(!html.contains("年間休日中央値差"), "年間休日 KPI は非表示");
        assert!(!html.contains("年間休日 中央値"), "年間休日 比較行は非表示");
        assert!(!html.contains("120 日"), "年間休日値は非表示");
        assert!(!html.contains("110 日"), "年間休日値は非表示");
        // Finding #8: 比較表も万円表示
        assert!(html.contains("28.0 万円"));
        assert!(html.contains("26.0 万円"));
    }

    #[test]
    fn skips_when_no_popular_tags() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &SurveyAggregation::default());
        assert!(html.is_empty(), "no popular tag → skip section entirely");
    }

    #[test]
    fn skips_when_only_none_count() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 10,
            popularity: PopularityAnalysis {
                popular_count: 0,
                super_popular_count: 0,
                none_count: 10,
                popular_ratio: 0.0,
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.is_empty());
    }

    #[test]
    fn renders_with_only_popular_salary_missing_holidays() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 5,
            popularity: PopularityAnalysis {
                popular_count: 2,
                super_popular_count: 0,
                none_count: 3,
                popular_ratio: 0.4,
                indeed_sp_total: 5,
                popular_salary_median: Some(250_000),
                non_popular_salary_median: Some(240_000),
                popular_holidays_median: None,
                non_popular_holidays_median: None,
                // Finding #5: n >= 5 で月給表示が出ることを検証
                popular_n_salary: 5,
                non_popular_n_salary: 8,
                popular_n_holidays: 0,
                non_popular_n_holidays: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.contains("SECTION 05"));
        // Finding #8: 月給は万円表示
        assert!(html.contains("25.0 万円"), "250,000 → 25.0 万円");
        // holidays 行は出ない (両方 None)
        assert!(
            !html.contains("年間休日 中央値"),
            "holiday row should be absent when both None"
        );
    }

    #[test]
    fn salary_diff_negative_sign() {
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 5,
            popularity: PopularityAnalysis {
                popular_count: 1,
                super_popular_count: 0,
                none_count: 4,
                popular_ratio: 0.2,
                indeed_sp_total: 5,
                popular_salary_median: Some(200_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: None,
                non_popular_holidays_median: None,
                // Finding #5: n >= 5 で月給差が表示されることを検証
                popular_n_salary: 5,
                non_popular_n_salary: 10,
                popular_n_holidays: 0,
                non_popular_n_holidays: 0,
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // Finding #8: 200,000 - 260,000 = -60,000 → "-6.0 万円" (万円表示)
        assert!(
            html.contains("-6.0 万円"),
            "negative diff displayed in manyen"
        );
    }

    #[test]
    fn shows_insufficient_n_when_n_below_threshold() {
        // Finding #5: 両群 n < 5 の場合は "— (n不足)" を表示する
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 5,
            popularity: PopularityAnalysis {
                popular_count: 1,
                super_popular_count: 0,
                none_count: 4,
                popular_ratio: 0.2,
                indeed_sp_total: 5,
                popular_salary_median: Some(200_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: Some(115),
                non_popular_holidays_median: Some(108),
                popular_n_salary: 3,       // < 5
                non_popular_n_salary: 4,   // < 5
                popular_n_holidays: 2,     // < 5
                non_popular_n_holidays: 3, // < 5
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // KPI で n不足表示
        assert!(html.contains("n不足"), "n < 5 → insufficient-n indicator");
        // 差分 KPI に実値が出ない
        assert!(
            !html.contains("-6.0 万円"),
            "no diff value when n insufficient"
        );
        // 比較表にも n 表示 (n=3 or n=4)
        assert!(
            html.contains("n=3") || html.contains("n=2"),
            "table shows n"
        );
    }

    // =========================================================================
    // §05-3 テスト
    // =========================================================================

    /// 3 グループとも n >= 1 → §05-3 が描画される
    #[test]
    fn renders_popularity_salary_stats_section() {
        use super::super::super::super::aggregator::SalaryStats;
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 30,
            popularity: PopularityAnalysis {
                popular_count: 4,
                super_popular_count: 2,
                none_count: 24,
                popular_ratio: 6.0 / 30.0,
                indeed_sp_total: 30,
                popular_salary_median: Some(280_000),
                non_popular_salary_median: Some(260_000),
                popular_holidays_median: None,
                non_popular_holidays_median: None,
                popular_n_salary: 4,
                non_popular_n_salary: 24,
                popular_n_holidays: 0,
                non_popular_n_holidays: 0,
                super_popular_salary_stats: SalaryStats {
                    n: 2,
                    min_mean: Some(270_000),
                    min_median: Some(270_000),
                    min_mode: Some(250_000),
                    max_mean: Some(350_000),
                    max_median: Some(350_000),
                    max_mode: Some(350_000),
                },
                popular_salary_stats: SalaryStats {
                    n: 4,
                    min_mean: Some(280_000),
                    min_median: Some(280_000),
                    min_mode: Some(250_000),
                    max_mean: Some(360_000),
                    max_median: Some(360_000),
                    max_mode: Some(350_000),
                },
                non_popular_salary_stats: SalaryStats {
                    n: 24,
                    min_mean: Some(255_000),
                    min_median: Some(260_000),
                    min_mode: Some(250_000),
                    max_mean: Some(320_000),
                    max_median: Some(320_000),
                    max_mode: Some(300_000),
                },
                // 年間休日 3 区分統計はこのテストの対象外 (既定 = n=0 で §05-4 非描画)。
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // §05-3 見出しが存在する
        assert!(html.contains("§05-3"), "section 05-3 heading present");
        assert!(
            html.contains("人気タグ別 給与統計"),
            "section title present"
        );
        // 超人気グループの行が出る
        assert!(html.contains("超人気"), "super_popular group row");
        // 人気グループの行が出る
        assert!(html.contains(">人気<"), "popular group row");
        // タグなしグループの行が出る
        assert!(html.contains("タグなし"), "non_popular group row");
        // 万円表示
        assert!(html.contains("万円"), "manyen unit present");
        // 27.0 万円 (super_popular min_mean=270_000)
        assert!(
            html.contains("27.0 万円"),
            "super_popular min_mean formatted"
        );
        // 35.0 万円 (super_popular max_mean=350_000)
        assert!(
            html.contains("35.0 万円"),
            "super_popular max_mean formatted"
        );
    }

    /// 3 グループとも n=0 → §05-3 全体スキップ
    #[test]
    fn skips_popularity_salary_stats_when_all_zero() {
        use super::super::super::super::aggregator::SalaryStats;
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 10,
            popularity: PopularityAnalysis {
                popular_count: 3,
                super_popular_count: 2,
                none_count: 5,
                popular_ratio: 0.5,
                indeed_sp_total: 10,
                // salary_stats は全て n=0 (月給データなし)
                super_popular_salary_stats: SalaryStats::default(),
                popular_salary_stats: SalaryStats::default(),
                non_popular_salary_stats: SalaryStats::default(),
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        // §05 全体は描画される (popular_count > 0)
        assert!(html.contains("SECTION 05"), "section renders");
        // §05-3 はスキップされる
        assert!(
            !html.contains("§05-3"),
            "salary stats section skipped when all n=0"
        );
    }

    // =========================================================================
    // §05-4 年間休日 3 区分統計 テスト (2026-07-28)
    // =========================================================================

    fn agg_with_holiday_stats() -> SurveyAggregation {
        use super::super::super::super::aggregator::{HolidayStats, PopularityAnalysis};
        SurveyAggregation {
            total_count: 30,
            popularity: PopularityAnalysis {
                popular_count: 6,
                super_popular_count: 6,
                none_count: 18,
                popular_ratio: 12.0 / 30.0,
                indeed_sp_total: 30,
                super_popular_holiday_stats: HolidayStats {
                    n: 6,
                    median: Some(120),
                    mean: Some(121),
                    p25: Some(115),
                    p75: Some(125),
                },
                popular_holiday_stats: HolidayStats {
                    n: 6,
                    median: Some(118),
                    mean: Some(117),
                    p25: Some(112),
                    p75: Some(122),
                },
                non_popular_holiday_stats: HolidayStats {
                    n: 18,
                    median: Some(110),
                    mean: Some(109),
                    p25: Some(105),
                    p75: Some(115),
                },
                ..Default::default()
            },
            ..Default::default()
        }
    }

    /// §05-4 が 3 区分の年間休日統計を描画する
    #[test]
    fn renders_holiday_stats_section() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &agg_with_holiday_stats());
        assert!(html.contains("§05-4"), "section 05-4 heading present");
        assert!(
            html.contains("人気タグ別 年間休日統計"),
            "holiday stats title present"
        );
        // 各区分ラベル
        assert!(html.contains("超人気"), "super_popular row");
        assert!(html.contains(">人気<"), "popular row");
        assert!(html.contains("タグなし"), "non_popular row");
        // 分母 (件数) 明示: 超人気 n=6 / タグなし n=18
        assert!(html.contains("6 件"), "super_popular n=6");
        assert!(html.contains("18 件"), "non_popular n=18");
        // 日単位の中央値/四分位が出る
        assert!(html.contains("120 日"), "super_popular median 120");
        assert!(html.contains("110 日"), "non_popular median 110");
        assert!(html.contains("125 日"), "super_popular P75 125");
        // 中立の差分注記 (超人気 +10 日, 人気 +8 日)
        assert!(html.contains("差が確認できました"), "neutral diff wording");
        assert!(html.contains("+10 日"), "super_popular vs 非人気 diff");
    }

    /// 3 区分とも n=0 → §05-4 スキップ (但し popular_count>0 で §05 自体は描画)
    #[test]
    fn skips_holiday_stats_when_all_zero() {
        use super::super::super::super::aggregator::PopularityAnalysis;
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 10,
            popularity: PopularityAnalysis {
                popular_count: 3,
                super_popular_count: 2,
                none_count: 5,
                popular_ratio: 0.5,
                indeed_sp_total: 10,
                // holiday_stats は全て default (n=0)
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.contains("SECTION 05"), "section renders");
        assert!(
            !html.contains("§05-4"),
            "holiday stats section skipped when all n=0"
        );
    }

    /// n<5 の区分は「参考値(件数僅少)」注記が付き、差分注記の対象外になる
    #[test]
    fn holiday_stats_marks_small_n_as_reference() {
        use super::super::super::super::aggregator::{HolidayStats, PopularityAnalysis};
        let mut html = String::new();
        let agg = SurveyAggregation {
            total_count: 20,
            popularity: PopularityAnalysis {
                popular_count: 3,
                super_popular_count: 2,
                none_count: 15,
                popular_ratio: 5.0 / 20.0,
                indeed_sp_total: 20,
                // 超人気: n=2 (< 5) → 参考値
                super_popular_holiday_stats: HolidayStats {
                    n: 2,
                    median: Some(130),
                    mean: Some(130),
                    p25: Some(125),
                    p75: Some(135),
                },
                // 人気: n=0 (データなし)
                popular_holiday_stats: HolidayStats::default(),
                // タグなし: n=15
                non_popular_holiday_stats: HolidayStats {
                    n: 15,
                    median: Some(108),
                    mean: Some(108),
                    p25: Some(104),
                    p75: Some(112),
                },
                ..Default::default()
            },
            ..Default::default()
        };
        render_navy_section_popularity(&mut html, &agg);
        assert!(html.contains("§05-4"), "section renders (n>0 somewhere)");
        // n<5 区分の参考値注記
        assert!(html.contains("参考値(件数僅少)"), "small-n reference marker");
        // 人気 (n=0) はデータなし行
        assert!(html.contains("データなし"), "n=0 group shows データなし");
        // 超人気 n=2 (< 5) は差分注記の対象外 → 差分注記自体が出ない
        assert!(
            !html.contains("差が確認できました"),
            "small-n excluded from neutral diff note"
        );
    }

    /// 逆証明: 出力に因果を断定する語が含まれないこと。
    #[test]
    fn holiday_stats_has_no_causal_assertions() {
        let mut html = String::new();
        render_navy_section_popularity(&mut html, &agg_with_holiday_stats());
        // 断定語のブラックリスト (因果・理由の断定)
        for forbidden in [
            "人気の理由は",
            "人気だから",
            "が原因で",
            "ため人気",
            "によって人気",
            "休日が多いから人気",
            "人気の要因は",
        ] {
            assert!(
                !html.contains(forbidden),
                "断定語 '{}' が出力に含まれてはならない",
                forbidden
            );
        }
        // 中立表現 (差の有無を確認する型) は含まれる
        assert!(
            html.contains("確認できました") || html.contains("確認できませんでした"),
            "中立の差分表現が含まれる"
        );
        // 因果を示さない旨の明示注記
        assert!(
            html.contains("因果は示しません") || html.contains("説明できるものではありません"),
            "因果否定の注記が含まれる"
        );
    }
}
