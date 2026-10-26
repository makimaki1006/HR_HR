//! Same-keyword, same-scale comparison for the Excel-based standalone report.
use super::competitor_model::KeywordRow;
use crate::handlers::helpers::escape_html;
use serde::Serialize;
use ts_rs::TS;

/// 先頭 N 件・全体・超人気・人気の占有率の比較 (同じ語・同じ 0〜100% の軸)。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct KeywordComparison {
    /// 先頭の求人数 (比較の母数)。0 のとき `rows` は空。
    pub head_n: u32,
    /// 全体の求人数。
    pub all_n: u32,
    /// 超人気の求人数 (占有率の母数)。0 のとき超人気の系列は出さない (SP 版 CSV 以外・該当なし)。
    pub super_popular_n: u32,
    /// 人気の求人数 (占有率の母数)。0 のとき人気の系列は出さない。
    pub popular_n: u32,
    /// 先頭で件数の多い順の上位 20 語。
    pub rows: Vec<ComparisonRow>,
}

/// 超人気・人気の件数をこの数未満のとき「参考値」と添える。
pub const SMALL_CLASS_N: u32 = 5;
/// SP 版 CSV 以外では超人気・人気の系列が無いことを伝える一文。
pub const POPULARITY_ONLY_SP: &str = "人気・超人気のタグがあるSP版CSVのときだけ表示されます";
/// 件数が少ない系列に添える注記。
pub const SMALL_N_NOTE: &str = "件数が少ないため参考値";

/// 超人気・人気の求人の中での語の件数と母数 (集計済み)。
#[derive(Debug, Clone, Copy, Default)]
pub(super) struct PopularityCounts<'a> {
    pub super_n: usize,
    pub super_counts: &'a [(String, usize)],
    pub popular_n: usize,
    pub popular_counts: &'a [(String, usize)],
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ComparisonRow {
    pub word: String,
    pub head_count: u32,
    /// 先頭件数 / 先頭の求人数 × 100。
    pub head_share_pct: f64,
    /// 全体にその語の件数が無い・全体の母数が 0 のときは null (0 とは言えないので欠測のまま)。
    pub all_count: Option<u32>,
    pub all_share_pct: Option<f64>,
    /// 超人気の求人の中でその語を含む件数。超人気の求人が無いときは null (0 にしない)。
    pub super_popular_count: Option<u32>,
    /// 超人気の件数 / 超人気の求人数 × 100。
    pub super_popular_share_pct: Option<f64>,
    /// 人気の求人の中でその語を含む件数。人気の求人が無いときは null。
    pub popular_count: Option<u32>,
    /// 人気の件数 / 人気の求人数 × 100。
    pub popular_share_pct: Option<f64>,
}

fn class_share(counts: &[(String, usize)], n: usize, word: &str) -> (Option<u32>, Option<f64>) {
    if n == 0 {
        return (None, None);
    }
    let c = counts
        .iter()
        .find(|(w, _)| w == word)
        .map_or(0, |(_, c)| *c);
    (Some(c as u32), Some(c as f64 / n as f64 * 100.0))
}

/// チャートに出す語数。表は先頭 10 語。
const CHART_WORDS: usize = 20;

pub(super) fn build_comparison(
    head: &[(String, usize)],
    head_n: usize,
    all: &[(String, usize)],
    all_n: usize,
    pop: &PopularityCounts<'_>,
) -> KeywordComparison {
    let rows = if head_n == 0 {
        Vec::new()
    } else {
        head.iter()
            .take(CHART_WORDS)
            .map(|(word, count)| {
                // 全体に語が無いのは先頭の件数と矛盾するので、0 と決めずに欠測のままにする。
                let all_count = all.iter().find(|(w, _)| w == word).map(|(_, n)| *n);
                let (super_popular_count, super_popular_share_pct) =
                    class_share(pop.super_counts, pop.super_n, word);
                let (popular_count, popular_share_pct) =
                    class_share(pop.popular_counts, pop.popular_n, word);
                ComparisonRow {
                    word: word.clone(),
                    head_count: *count as u32,
                    head_share_pct: *count as f64 / head_n as f64 * 100.0,
                    all_count: all_count.map(|n| n as u32),
                    all_share_pct: all_count
                        .filter(|_| all_n > 0)
                        .map(|n| n as f64 / all_n as f64 * 100.0),
                    super_popular_count,
                    super_popular_share_pct,
                    popular_count,
                    popular_share_pct,
                }
            })
            .collect()
    };
    KeywordComparison {
        head_n: head_n as u32,
        all_n: all_n as u32,
        super_popular_n: pop.super_n as u32,
        popular_n: pop.popular_n as u32,
        rows,
    }
}

fn signed_points(value: f64) -> String {
    if value.abs() < 0.05 {
        "0.0".into()
    } else {
        format!("{value:+.1}")
    }
}

/// 注記の文 (描画する系列の数に応じて変わる)。無ければ空。
fn series_notes(cmp: &KeywordComparison) -> Vec<String> {
    let mut notes = Vec::new();
    match (cmp.super_popular_n, cmp.popular_n) {
        (0, 0) => notes.push(POPULARITY_ONLY_SP.to_owned()),
        (0, _) => notes.push("超人気の求人がないため、超人気は表示していません".to_owned()),
        (_, 0) => notes.push("人気の求人がないため、人気は表示していません".to_owned()),
        _ => {}
    }
    for (label, n) in [("超人気", cmp.super_popular_n), ("人気", cmp.popular_n)] {
        if (1..SMALL_CLASS_N).contains(&n) {
            notes.push(format!("{label}（{n}件）は{SMALL_N_NOTE}です"));
        }
    }
    notes
}

pub(super) fn table(html: &mut String, cmp: &KeywordComparison) {
    let (head_n, all_n) = (cmp.head_n, cmp.all_n);
    let (has_super, has_popular) = (cmp.super_popular_n > 0, cmp.popular_n > 0);
    let extra_head = format!(
        "{}{}",
        if has_super {
            "<th>超人気率</th>"
        } else {
            ""
        },
        if has_popular {
            "<th>人気率</th>"
        } else {
            ""
        }
    );
    let columns = 5 + usize::from(has_super) + usize::from(has_popular);
    html.push_str(&format!("<h2>求人票ワード調査（先頭 {head_n} 件）</h2><table class=\"words word-comparison\"><thead><tr><th>上位10語</th><th>件数</th><th>先頭率</th><th>全体率</th><th>差(pt)</th>{extra_head}</tr></thead><tbody>"));
    for row in cmp.rows.iter().take(10) {
        let (all, delta) = row.all_share_pct.map_or_else(
            || ("—".into(), "—".into()),
            |all| {
                (
                    format!("{all:.1}%"),
                    signed_points(row.head_share_pct - all),
                )
            },
        );
        let pct = |v: Option<f64>| v.map_or_else(|| "—".to_owned(), |v| format!("{v:.1}%"));
        let mut extra = String::new();
        if has_super {
            extra.push_str(&format!("<td>{}</td>", pct(row.super_popular_share_pct)));
        }
        if has_popular {
            extra.push_str(&format!("<td>{}</td>", pct(row.popular_share_pct)));
        }
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{:.1}%</td><td>{all}</td><td>{delta}</td>{extra}</tr>",
            escape_html(&row.word),
            row.head_count,
            row.head_share_pct
        ));
    }
    if cmp.rows.is_empty() {
        let message = if head_n == 0 {
            "取り込み順の比較データがありません"
        } else {
            "先頭の求人にキーワードがありません"
        };
        html.push_str(&format!(
            "<tr><td colspan=\"{columns}\">{message}</td></tr>"
        ));
    }
    html.push_str(&format!("</tbody></table><p class=\"note word-basis\">母数：先頭 {head_n} 件 / 全体 {all_n} 件。先頭は収録順、差は先頭率−全体率。</p>"));
    if has_super || has_popular {
        html.push_str(&format!(
            "<p class=\"note word-basis\">超人気 {} 件 / 人気 {} 件の中での占有率。</p>",
            cmp.super_popular_n, cmp.popular_n
        ));
    }
}

pub(super) fn chart(html: &mut String, cmp: &KeywordComparison) {
    let (head_n, all_n) = (cmp.head_n, cmp.all_n);
    html.push_str(
        "<figure class=\"chart keyword-comparison\"><figcaption>訴求語の占有率比較（上位20語）</figcaption>",
    );
    let rows = &cmp.rows;
    if rows.is_empty() {
        html.push_str("<p class=\"note\">比較できるキーワードデータがありません</p></figure>");
        return;
    }
    let extra = cmp.super_popular_n > 0 || cmp.popular_n > 0;
    // 超人気・人気の系列があるときだけ、凡例を 2 行にして棒を 4 本に細くする。
    let (shift, height) = if extra { (16, 731) } else { (0, 715) };
    let top = 42 + shift;
    let mut legend: Vec<(&str, String, &str)> = if extra {
        vec![
            ("全体", format!("全体 {all_n}件"), "#006666"),
            ("先頭", format!("先頭 {head_n}件"), "#4472c4"),
        ]
    } else {
        vec![
            ("全体", "全体".to_owned(), "#006666"),
            ("先頭", format!("先頭 {head_n} 件"), "#4472c4"),
        ]
    };
    if cmp.super_popular_n > 0 {
        legend.push((
            "超人気",
            format!("超人気 {}件", cmp.super_popular_n),
            "#c0504d",
        ));
    }
    if cmp.popular_n > 0 {
        legend.push(("人気", format!("人気 {}件", cmp.popular_n), "#e69f00"));
    }
    html.push_str(&format!("<svg data-series=\"keyword-head\" viewBox=\"0 0 440 {height}\" role=\"img\" aria-label=\"先頭 {head_n} 件と全体 {all_n} 件の占有率比較。先頭の上位20語。横軸0から100パーセント\"><title>訴求語の占有率比較：先頭 {head_n} 件 / 全体 {all_n} 件</title>"));
    for (i, (_, text, color)) in legend.iter().enumerate() {
        let (x, y) = if extra {
            (135 + (i % 2) * 105, 8 + (i / 2) * 16)
        } else {
            (135 + i * 87, 8)
        };
        html.push_str(&format!("<rect x=\"{x}\" y=\"{y}\" width=\"12\" height=\"8\" fill=\"{color}\"/><text x=\"{}\" y=\"{}\" font-size=\"12\">{text}</text>", x + 17, y + 8));
    }
    for tick in [0, 25, 50, 75, 100] {
        let x = 135.0 + 240.0 * tick as f64 / 100.0;
        html.push_str(&format!("<path d=\"M {x} {} V {}\" stroke=\"#d5d9dd\"/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">{tick}%</text>", 38 + shift, 679 + shift, 705 + shift));
    }
    for (i, row) in rows.iter().enumerate() {
        let y = top + i * 32;
        let label = if row.word.chars().count() > 10 {
            format!("{}…", row.word.chars().take(9).collect::<String>())
        } else {
            row.word.clone()
        };
        html.push_str(&format!("<text x=\"125\" y=\"{}\" text-anchor=\"end\" font-size=\"12\"><title>{}</title>{}</text>", y + 15, escape_html(&row.word), escape_html(&label)));
        let mut bars: Vec<(usize, Option<f64>, &str, &str)> = vec![
            (0, row.all_share_pct, "#006666", "全体"),
            (
                if extra { 7 } else { 11 },
                Some(row.head_share_pct),
                "#4472c4",
                "先頭",
            ),
        ];
        if cmp.super_popular_n > 0 {
            bars.push((14, row.super_popular_share_pct, "#c0504d", "超人気"));
        }
        if cmp.popular_n > 0 {
            bars.push((
                if cmp.super_popular_n > 0 { 21 } else { 14 },
                row.popular_share_pct,
                "#e69f00",
                "人気",
            ));
        }
        let bar_h = if extra { 6 } else { 8 };
        for (offset, value, color, group) in bars {
            let bar_y = y + offset;
            if let Some(value) = value {
                let width = value * 2.4;
                html.push_str(&format!("<rect x=\"135\" y=\"{bar_y}\" width=\"{width:.3}\" height=\"{bar_h}\" fill=\"{color}\"><title>{} / {group}: {value:.1}%</title></rect><text x=\"{}\" y=\"{}\" font-size=\"10\">{value:.1}%</text>",escape_html(&row.word),139.0 + width,bar_y + bar_h));
            } else {
                html.push_str(&format!(
                    "<text x=\"139\" y=\"{}\" font-size=\"10\">—</text>",
                    bar_y + bar_h
                ));
            }
        }
    }
    html.push_str("</svg>");
    for note in series_notes(cmp) {
        html.push_str(&format!("<p class=\"note\">{note}</p>"));
    }
    let opt = |v: Option<u32>| v.map_or(serde_json::Value::Null, |n| serde_json::json!(n));
    let data = serde_json::json!({"mode":"comparison","allN":all_n,"headN":head_n,
        "superN":cmp.super_popular_n,"popularN":cmp.popular_n,
        "title":"訴求語の占有率比較","rows":rows.iter().map(|r|serde_json::json!({
            "word":r.word,"head":r.head_count,"all":r.all_share_pct.and(r.all_count),
            "super":opt(r.super_popular_count),"popular":opt(r.popular_count)
        })).collect::<Vec<_>>()});
    attach_data(html, "keyword-head", data);
    html.push_str("</figure>");
}

pub(super) fn all_chart(html: &mut String, all: &[KeywordRow]) {
    html.push_str("<figure class=\"chart keyword-comparison\"><figcaption>求人票キーワード調査（全体・上位20語）</figcaption>");
    if all.is_empty() {
        html.push_str("<p class=\"note\">集計できるキーワードがありません</p></figure>");
        return;
    }
    let max = all
        .iter()
        .take(20)
        .map(|r| r.count as usize)
        .max()
        .unwrap_or(1)
        .max(1);
    html.push_str("<svg data-series=\"keyword-all\" viewBox=\"0 0 440 715\" role=\"img\" aria-label=\"全体の上位20語を含む求人数\"><title>全体の訴求語・求人数</title>");
    for (i, KeywordRow { word, count, .. }) in all.iter().take(20).enumerate() {
        let n = *count as usize;
        let y = 32 + i * 32;
        let label = if word.chars().count() > 10 {
            format!("{}…", word.chars().take(9).collect::<String>())
        } else {
            word.clone()
        };
        let width = n as f64 / max as f64 * 230.0;
        html.push_str(&format!("<text x=\"140\" y=\"{}\" text-anchor=\"end\" font-size=\"14\"><title>{}</title>{}</text><rect x=\"150\" y=\"{y}\" width=\"{width:.3}\" height=\"18\" fill=\"#006666\"><title>{}: {n}件</title></rect><text x=\"{}\" y=\"{}\" font-size=\"13\">{n}件</text>",y+14,escape_html(word),escape_html(&label),escape_html(word),width+155.0,y+14));
    }
    html.push_str(
        "<text x=\"150\" y=\"705\" font-size=\"13\">棒の長さ：その語を含む求人数</text></svg>",
    );
    attach_data(
        html,
        "keyword-all",
        serde_json::json!({"mode":"all","title":"全体の訴求語・求人数",
        "rows":all.iter().take(20).map(|r|serde_json::json!({"word":r.word,"all":r.count})).collect::<Vec<_>>() }),
    );
    html.push_str("</figure>");
}

fn attach_data(html: &mut String, id: &str, data: serde_json::Value) {
    let json = data
        .to_string()
        .replace('<', "\\u003c")
        .replace('>', "\\u003e")
        .replace('&', "\\u0026");
    html.push_str(&format!(
        "<script type=\"application/json\" id=\"{id}\">{json}</script>"
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(v: &[(&str, usize)]) -> Vec<(String, usize)> {
        v.iter().map(|(w, n)| ((*w).to_owned(), *n)).collect()
    }

    #[test]
    fn different_denominators_use_percentage_points_and_keep_sign() {
        let head = pairs(&[("研修あり", 3), ("賞与あり", 1)]);
        let all = pairs(&[("研修あり", 4), ("賞与あり", 8)]);
        let cmp = build_comparison(&head, 4, &all, 10, &PopularityCounts::default());
        assert_eq!(
            (cmp.rows[0].head_share_pct, cmp.rows[0].all_share_pct),
            (75.0, Some(40.0))
        );
        let mut html = String::new();
        table(&mut html, &cmp);
        assert!(html.contains("<td>3</td><td>75.0%</td><td>40.0%</td><td>+35.0</td>"));
        assert!(html.contains("<td>1</td><td>25.0%</td><td>80.0%</td><td>-55.0</td>"));
    }

    #[test]
    fn identical_population_has_zero_gap_and_missing_is_not_zero() {
        let words = pairs(&[("A", 2)]);
        let mut html = String::new();
        table(
            &mut html,
            &build_comparison(&words, 4, &words, 4, &PopularityCounts::default()),
        );
        assert!(html.contains("<td>50.0%</td><td>50.0%</td><td>0.0</td>"));
        let mut missing = String::new();
        table(
            &mut missing,
            &build_comparison(&words, 4, &[], 0, &PopularityCounts::default()),
        );
        assert!(missing.contains("<td>50.0%</td><td>—</td><td>—</td>"));
        assert!(
            build_comparison(&words, 0, &words, 4, &PopularityCounts::default())
                .rows
                .is_empty()
        );
    }

    #[test]
    fn word_absent_from_all_stays_missing_not_zero() {
        let cmp = build_comparison(
            &pairs(&[("A", 1)]),
            2,
            &pairs(&[("B", 5)]),
            10,
            &PopularityCounts::default(),
        );
        assert_eq!(
            (cmp.rows[0].all_count, cmp.rows[0].all_share_pct),
            (None, None)
        );
    }

    #[test]
    fn chart_uses_common_percent_scale_and_escapes_labels() {
        let words = pairs(&[("<script>alert(1)</script>", 1)]);
        let mut html = String::new();
        chart(
            &mut html,
            &build_comparison(&words, 2, &words, 4, &PopularityCounts::default()),
        );
        assert!(html.contains("width=\"60.000\"")); // 25% of a 240px axis.
        assert!(html.contains("width=\"120.000\"")); // 50%, same axis.
        assert!(html.contains(">100%</text>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn chart_shows_up_to_twenty_words() {
        let words: Vec<(String, usize)> = (0..30).map(|i| (format!("w{i:02}"), 30 - i)).collect();
        let cmp = build_comparison(&words, 40, &words, 40, &PopularityCounts::default());
        assert_eq!(cmp.rows.len(), 20);
        let mut html = String::new();
        chart(&mut html, &cmp);
        assert_eq!(html.matches("data-series=\"keyword-head\"").count(), 1);
        assert!(html.contains("w19") && !html.contains("w20"));
    }

    fn pop<'a>(
        super_n: usize,
        super_counts: &'a [(String, usize)],
        popular_n: usize,
        popular_counts: &'a [(String, usize)],
    ) -> PopularityCounts<'a> {
        PopularityCounts {
            super_n,
            super_counts,
            popular_n,
            popular_counts,
        }
    }

    #[test]
    fn popularity_series_use_their_own_denominators() {
        let head = pairs(&[("X", 4), ("Y", 1)]);
        let all = pairs(&[("X", 4), ("Y", 3)]);
        let sup = pairs(&[("X", 1)]);
        let popular = pairs(&[("X", 2)]);
        let cmp = build_comparison(&head, 10, &all, 10, &pop(2, &sup, 3, &popular));
        assert_eq!((cmp.super_popular_n, cmp.popular_n), (2, 3));
        let x = &cmp.rows[0];
        assert_eq!(
            (x.super_popular_count, x.super_popular_share_pct),
            (Some(1), Some(50.0))
        );
        assert_eq!(x.popular_count, Some(2));
        assert!((x.popular_share_pct.unwrap() - 200.0 / 3.0).abs() < 1e-9);
        // 区分があって語が無いときは 0 件 (欠測ではない)。
        let y = &cmp.rows[1];
        assert_eq!(
            (y.super_popular_count, y.super_popular_share_pct),
            (Some(0), Some(0.0))
        );
    }

    #[test]
    fn empty_classes_are_omitted_and_explained() {
        let words = pairs(&[("A", 2)]);
        let cmp = build_comparison(&words, 4, &words, 4, &PopularityCounts::default());
        assert_eq!(cmp.rows[0].super_popular_share_pct, None);
        assert_eq!(cmp.rows[0].popular_share_pct, None);
        let mut html = String::new();
        chart(&mut html, &cmp);
        assert!(html.contains(POPULARITY_ONLY_SP));
        assert!(html.contains("viewBox=\"0 0 440 715\""));
        assert!(!html.contains("超人気 "));
        assert!(html.contains("\"superN\":0"));
    }

    #[test]
    fn four_series_chart_and_table_show_each_n_and_small_n_note() {
        let words = pairs(&[("A", 2)]);
        let sup = pairs(&[("A", 1)]);
        let popular = pairs(&[("A", 3)]);
        let cmp = build_comparison(&words, 4, &words, 8, &pop(2, &sup, 6, &popular));
        let mut html = String::new();
        chart(&mut html, &cmp);
        assert!(html.contains("全体 8件") && html.contains("先頭 4件"));
        assert!(html.contains("超人気 2件") && html.contains("人気 6件"));
        assert!(html.contains("viewBox=\"0 0 440 731\""));
        assert!(html.contains("A / 超人気: 50.0%") && html.contains("A / 人気: 50.0%"));
        // 超人気は 2 件 (5 件未満) なので参考値、人気は 6 件なので注記なし。
        assert!(html.contains("超人気（2件）は件数が少ないため参考値です"));
        assert!(!html.contains("人気（6件）"));
        assert!(!html.contains(POPULARITY_ONLY_SP));
        let mut table_html = String::new();
        table(&mut table_html, &cmp);
        assert!(table_html.contains("<th>超人気率</th><th>人気率</th>"));
        assert!(table_html
            .contains("<td>50.0%</td><td>25.0%</td><td>+25.0</td><td>50.0%</td><td>50.0%</td>"));
    }
}
