//! Same-keyword, same-scale comparison for the Excel-based standalone report.
use super::competitor_model::KeywordRow;
use crate::handlers::helpers::escape_html;
use serde::Serialize;
use ts_rs::TS;

/// 先頭 N 件と全体の占有率の比較 (同じ語・同じ 0〜100% の軸)。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct KeywordComparison {
    /// 先頭の求人数 (比較の母数)。0 のとき `rows` は空。
    pub head_n: u32,
    /// 全体の求人数。
    pub all_n: u32,
    /// 先頭で件数の多い順の上位 20 語。
    pub rows: Vec<ComparisonRow>,
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
}

/// チャートに出す語数。表は先頭 10 語。
const CHART_WORDS: usize = 20;

pub(super) fn build_comparison(
    head: &[(String, usize)],
    head_n: usize,
    all: &[(String, usize)],
    all_n: usize,
) -> KeywordComparison {
    let rows = if head_n == 0 {
        Vec::new()
    } else {
        head.iter()
            .take(CHART_WORDS)
            .map(|(word, count)| {
                // 全体に語が無いのは先頭の件数と矛盾するので、0 と決めずに欠測のままにする。
                let all_count = all.iter().find(|(w, _)| w == word).map(|(_, n)| *n);
                ComparisonRow {
                    word: word.clone(),
                    head_count: *count as u32,
                    head_share_pct: *count as f64 / head_n as f64 * 100.0,
                    all_count: all_count.map(|n| n as u32),
                    all_share_pct: all_count
                        .filter(|_| all_n > 0)
                        .map(|n| n as f64 / all_n as f64 * 100.0),
                }
            })
            .collect()
    };
    KeywordComparison {
        head_n: head_n as u32,
        all_n: all_n as u32,
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

pub(super) fn table(html: &mut String, cmp: &KeywordComparison) {
    let (head_n, all_n) = (cmp.head_n, cmp.all_n);
    html.push_str(&format!("<h2>求人票ワード調査（先頭 {head_n} 件）</h2><table class=\"words word-comparison\"><thead><tr><th>上位10語</th><th>件数</th><th>先頭率</th><th>全体率</th><th>差(pt)</th></tr></thead><tbody>"));
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
        html.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{:.1}%</td><td>{all}</td><td>{delta}</td></tr>",
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
        html.push_str(&format!("<tr><td colspan=\"5\">{message}</td></tr>"));
    }
    html.push_str(&format!("</tbody></table><p class=\"note word-basis\">母数：先頭 {head_n} 件 / 全体 {all_n} 件。先頭は収録順、差は先頭率−全体率。</p>"));
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
    html.push_str(&format!("<svg data-series=\"keyword-head\" viewBox=\"0 0 440 715\" role=\"img\" aria-label=\"先頭 {head_n} 件と全体 {all_n} 件の占有率比較。先頭の上位20語。横軸0から100パーセント\"><title>訴求語の占有率比較：先頭 {head_n} 件 / 全体 {all_n} 件</title><rect x=\"135\" y=\"8\" width=\"12\" height=\"8\" fill=\"#006666\"/><text x=\"152\" y=\"16\" font-size=\"12\">全体</text><rect x=\"222\" y=\"8\" width=\"12\" height=\"8\" fill=\"#4472c4\"/><text x=\"239\" y=\"16\" font-size=\"12\">先頭 {head_n} 件</text>"));
    for tick in [0, 25, 50, 75, 100] {
        let x = 135.0 + 240.0 * tick as f64 / 100.0;
        html.push_str(&format!("<path d=\"M {x} 38 V 679\" stroke=\"#d5d9dd\"/><text x=\"{x}\" y=\"705\" text-anchor=\"middle\" font-size=\"12\">{tick}%</text>"));
    }
    for (i, row) in rows.iter().enumerate() {
        let y = 42 + i * 32;
        let label = if row.word.chars().count() > 10 {
            format!("{}…", row.word.chars().take(9).collect::<String>())
        } else {
            row.word.clone()
        };
        html.push_str(&format!("<text x=\"125\" y=\"{}\" text-anchor=\"end\" font-size=\"12\"><title>{}</title>{}</text>", y + 15, escape_html(&row.word), escape_html(&label)));
        for (offset, value, color, group) in [
            (0, row.all_share_pct, "#006666", "全体"),
            (11, Some(row.head_share_pct), "#4472c4", "先頭"),
        ] {
            let bar_y = y + offset;
            if let Some(value) = value {
                let width = value * 2.4;
                html.push_str(&format!("<rect x=\"135\" y=\"{bar_y}\" width=\"{width:.3}\" height=\"8\" fill=\"{color}\"><title>{} / {group}: {value:.1}%</title></rect><text x=\"{}\" y=\"{}\" font-size=\"10\">{value:.1}%</text>",escape_html(&row.word),139.0 + width,bar_y + 8));
            } else {
                html.push_str(&format!(
                    "<text x=\"139\" y=\"{}\" font-size=\"10\">—</text>",
                    bar_y + 8
                ));
            }
        }
    }
    html.push_str("</svg>");
    let data = serde_json::json!({"mode":"comparison","allN":all_n,"headN":head_n,
        "title":"訴求語の占有率比較","rows":rows.iter().map(|r|serde_json::json!({
            "word":r.word,"head":r.head_count,"all":r.all_share_pct.and(r.all_count)
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
        let cmp = build_comparison(&head, 4, &all, 10);
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
        table(&mut html, &build_comparison(&words, 4, &words, 4));
        assert!(html.contains("<td>50.0%</td><td>50.0%</td><td>0.0</td>"));
        let mut missing = String::new();
        table(&mut missing, &build_comparison(&words, 4, &[], 0));
        assert!(missing.contains("<td>50.0%</td><td>—</td><td>—</td>"));
        assert!(build_comparison(&words, 0, &words, 4).rows.is_empty());
    }

    #[test]
    fn word_absent_from_all_stays_missing_not_zero() {
        let cmp = build_comparison(&pairs(&[("A", 1)]), 2, &pairs(&[("B", 5)]), 10);
        assert_eq!(
            (cmp.rows[0].all_count, cmp.rows[0].all_share_pct),
            (None, None)
        );
    }

    #[test]
    fn chart_uses_common_percent_scale_and_escapes_labels() {
        let words = pairs(&[("<script>alert(1)</script>", 1)]);
        let mut html = String::new();
        chart(&mut html, &build_comparison(&words, 2, &words, 4));
        assert!(html.contains("width=\"60.000\"")); // 25% of a 240px axis.
        assert!(html.contains("width=\"120.000\"")); // 50%, same axis.
        assert!(html.contains(">100%</text>"));
        assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
        assert!(!html.contains("<script>alert(1)</script>"));
    }

    #[test]
    fn chart_shows_up_to_twenty_words() {
        let words: Vec<(String, usize)> = (0..30).map(|i| (format!("w{i:02}"), 30 - i)).collect();
        let cmp = build_comparison(&words, 40, &words, 40);
        assert_eq!(cmp.rows.len(), 20);
        let mut html = String::new();
        chart(&mut html, &cmp);
        assert_eq!(html.matches("data-series=\"keyword-head\"").count(), 1);
        assert!(html.contains("w19") && !html.contains("w20"));
    }
}
