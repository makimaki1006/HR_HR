//! 求人票のキーワード (全体の割合の帯と、人気求人との差)。
//!
//! 語は Indeed の求人カードに付くキーワードのまま。見出し・本文は読まない。
//! 数え方 (1 求人 1 回、人気・超人気・日数・新着・数値だけのバッジを除く) は「人気の分かれ方」と同じ。
use crate::handlers::helpers::escape_html;
use serde::Serialize;
use ts_rs::TS;

/// 人気求人が無いときの一文 (SP 版 CSV 以外)。
pub const POPULARITY_ONLY_SP: &str = "人気・超人気の表示があるSP版CSVのときだけ表示されます";

/// この件数未満のタグは帯に入れず、まとめて語の数だけ出す。
pub const MIN_BAND_COUNT: u32 = 3;
/// 帯の境界 (全体の割合)。
const MOST_PCT: u64 = 50;
const HALF_PCT: u64 = 20;
/// 右のグラフに出す語数 (多い側・少ない側それぞれ)。
const GAP_ROWS: usize = 10;
/// 左のグラフに帯ごとに出す語数。
const CHART_ROWS_PER_BAND: usize = 7;
/// 表に帯ごとに出す語数。
const TABLE_ROWS_PER_BAND: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TagUsageRow {
    pub word: String,
    /// そのキーワードが付く求人数 (1 求人 1 回)。
    pub n_tag: u32,
    /// n_tag / n × 100。
    pub share_pct: f64,
    /// 人気または超人気の求人のうち、そのタグが付く求人数。人気求人が無いときは 0。
    pub popular_tag_n: u32,
    /// popular_tag_n / popular_n × 100。人気求人が無いときは null。
    pub popular_share_pct: Option<f64>,
    /// 人気求人での割合 − 全体の割合 (パーセントポイント)。人気求人が無いときは null。
    pub gap_pt: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TagBand {
    /// `most` / `half` / `some`。
    pub key: String,
    pub label: String,
    /// 割合の範囲の説明。
    pub range: String,
    pub rows: Vec<TagUsageRow>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TagUsage {
    /// 重複排除後の求人数。
    pub n: u32,
    /// 人気または超人気のタグが付く求人数。
    pub popular_n: u32,
    /// 3 つの帯 (ほとんど / 半数前後 / 一部)。語が無い帯も残す。
    pub bands: Vec<TagBand>,
    /// 件数 1〜2 で帯に入れなかった語の数。
    pub minor_word_count: u32,
    /// 人気求人で全体より割合が高い語 (件数 3 以上、差の大きい順、上位 10)。
    pub popular_up: Vec<TagUsageRow>,
    /// 人気求人で全体より割合が低い語 (件数 3 以上、差が小さい順、上位 10)。
    pub popular_down: Vec<TagUsageRow>,
}

fn band(key: &str, label: &str, range: &str) -> TagBand {
    TagBand {
        key: key.to_owned(),
        label: label.to_owned(),
        range: range.to_owned(),
        rows: Vec::new(),
    }
}

/// `all` / `pop` はキーワードの求人数 (1 求人 1 回)。`n` / `popular_n` はその母数。
pub(super) fn build(
    all: &[(String, usize)],
    n: usize,
    pop: &[(String, usize)],
    popular_n: usize,
) -> TagUsage {
    let mut bands = vec![
        band("most", "ほとんどの求人が付けている", "全体の50%以上"),
        band("half", "半数前後", "全体の20%以上50%未満"),
        band("some", "一部の求人だけ", "全体の20%未満（3件以上）"),
    ];
    let mut minor = 0u32;
    let mut flat: Vec<TagUsageRow> = Vec::new();
    for (word, count) in all {
        if !crate::handlers::survey::popularity_split::is_condition_tag(word) {
            continue;
        }
        let count = *count as u64;
        if count == 0 || n == 0 {
            continue;
        }
        let pop_count = pop.iter().find(|(w, _)| w == word).map_or(0, |(_, c)| *c);
        let share = count as f64 / n as f64 * 100.0;
        let popular_share = (popular_n > 0).then(|| pop_count as f64 / popular_n as f64 * 100.0);
        let row = TagUsageRow {
            word: word.clone(),
            n_tag: count as u32,
            share_pct: share,
            popular_tag_n: pop_count as u32,
            popular_share_pct: popular_share,
            gap_pt: popular_share.map(|p| p - share),
        };
        if count < u64::from(MIN_BAND_COUNT) {
            minor += 1;
            continue;
        }
        // 整数どうしで比べる (50% ちょうど・20% ちょうどを浮動小数に任せない)。
        let idx = if count * 100 >= MOST_PCT * n as u64 {
            0
        } else if count * 100 >= HALF_PCT * n as u64 {
            1
        } else {
            2
        };
        bands[idx].rows.push(row.clone());
        flat.push(row);
    }
    for b in &mut bands {
        b.rows.sort_by(|a, c| {
            let by_gap = match (a.gap_pt, c.gap_pt) {
                (Some(x), Some(y)) => y.total_cmp(&x),
                _ => std::cmp::Ordering::Equal,
            };
            by_gap
                .then(c.n_tag.cmp(&a.n_tag))
                .then_with(|| a.word.cmp(&c.word))
        });
    }
    let mut up: Vec<TagUsageRow> = flat
        .iter()
        .filter(|r| r.gap_pt.is_some_and(|g| g > 0.0))
        .cloned()
        .collect();
    up.sort_by(|a, c| {
        c.gap_pt
            .unwrap_or(0.0)
            .total_cmp(&a.gap_pt.unwrap_or(0.0))
            .then(c.n_tag.cmp(&a.n_tag))
            .then_with(|| a.word.cmp(&c.word))
    });
    up.truncate(GAP_ROWS);
    let mut down: Vec<TagUsageRow> = flat
        .iter()
        .filter(|r| r.gap_pt.is_some_and(|g| g < 0.0))
        .cloned()
        .collect();
    down.sort_by(|a, c| {
        a.gap_pt
            .unwrap_or(0.0)
            .total_cmp(&c.gap_pt.unwrap_or(0.0))
            .then(c.n_tag.cmp(&a.n_tag))
            .then_with(|| a.word.cmp(&c.word))
    });
    down.truncate(GAP_ROWS);
    TagUsage {
        n: n as u32,
        popular_n: popular_n as u32,
        bands,
        minor_word_count: minor,
        popular_up: up,
        popular_down: down,
    }
}

fn signed(value: f64) -> String {
    if value.abs() < 0.05 {
        "0.0".into()
    } else {
        format!("{value:+.1}")
    }
}

fn short(word: &str) -> String {
    if word.chars().count() > 10 {
        format!("{}…", word.chars().take(9).collect::<String>())
    } else {
        word.to_owned()
    }
}

fn row_title(r: &TagUsageRow) -> String {
    let mut t = format!(
        "{}: {}件・{:.1}%",
        escape_html(&r.word),
        r.n_tag,
        r.share_pct
    );
    if let (Some(p), Some(g)) = (r.popular_share_pct, r.gap_pt) {
        t.push_str(&format!(
            " / 人気求人 {}件・{p:.1}% / 差 {}",
            r.popular_tag_n,
            signed(g)
        ));
    }
    t
}

// 左のグラフの寸法 (React の tagUsageLayout.ts と同じ値)。
const W: f64 = 440.0;
const BAND_HEAD: f64 = 26.0;
const ROW_STEP: f64 = 22.0;
const BAND_GAP: f64 = 8.0;
const TOP: f64 = 6.0;
const AXIS_ROOM: f64 = 24.0;

/// 左のグラフ: 帯ごとの語の数と、各帯の語を割合の棒で並べる。
pub(super) fn bands_chart(html: &mut String, u: &TagUsage) {
    html.push_str("<figure class=\"chart tag-usage\"><figcaption>求人票キーワード調査（全体の割合の帯別）</figcaption>");
    if u.n == 0 || u.bands.iter().all(|b| b.rows.is_empty()) {
        html.push_str("<p class=\"note\">集計できるキーワードがありません</p></figure>");
        return;
    }
    let mut body = String::new();
    let mut y = TOP;
    let grid_top = y;
    for b in &u.bands {
        body.push_str(&format!(
            "<text x=\"4\" y=\"{}\" font-size=\"13\" font-weight=\"bold\">{} {}語（{}）</text>",
            y + 16.0,
            escape_html(&b.label),
            b.rows.len(),
            escape_html(&b.range)
        ));
        y += BAND_HEAD;
        if b.rows.is_empty() {
            body.push_str(&format!(
                "<text x=\"140\" y=\"{}\" font-size=\"12\">該当する語はありません</text>",
                y + 12.0
            ));
            y += ROW_STEP;
        }
        for r in b.rows.iter().take(CHART_ROWS_PER_BAND) {
            let width = r.share_pct * 2.4;
            body.push_str(&format!(
                "<text x=\"135\" y=\"{}\" text-anchor=\"end\" font-size=\"12\"><title>{}</title>{}</text><rect x=\"140\" y=\"{}\" width=\"{width:.3}\" height=\"12\" fill=\"#006666\"><title>{}</title></rect><text x=\"{:.3}\" y=\"{}\" font-size=\"11\">{:.1}%</text>",
                y + 12.0,
                escape_html(&r.word),
                escape_html(&short(&r.word)),
                y + 2.0,
                row_title(r),
                144.0 + width,
                y + 12.0,
                r.share_pct
            ));
            y += ROW_STEP;
        }
        if b.rows.len() > CHART_ROWS_PER_BAND {
            body.push_str(&format!(
                "<text x=\"140\" y=\"{}\" font-size=\"11\">ほか {} 語</text>",
                y + 10.0,
                b.rows.len() - CHART_ROWS_PER_BAND
            ));
            y += 16.0;
        }
        y += BAND_GAP;
    }
    let grid_bottom = y;
    let height = y + AXIS_ROOM;
    let mut grid = String::new();
    for tick in [0, 50, 100] {
        let x = 140.0 + 2.4 * f64::from(tick);
        grid.push_str(&format!(
            "<path d=\"M {x} {grid_top} V {grid_bottom}\" stroke=\"#d5d9dd\"/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">{tick}%</text>",
            grid_bottom + 14.0
        ));
    }
    html.push_str(&format!(
        "<svg viewBox=\"0 0 {W} {height}\" role=\"img\" aria-label=\"キーワードを全体の割合で3つの帯に分けた語と割合。横軸0から100パーセント\"><title>キーワードの割合の帯別（全体 {} 件）</title>{grid}{body}</svg>",
        u.n
    ));
    html.push_str(&format!(
        "<p class=\"note\">全体 {} 件のうち、その語が付く求人の割合。件数1〜2の語（{}語）は帯に入れていません。</p></figure>",
        u.n, u.minor_word_count
    ));
}

const GAP_CENTER: f64 = 285.0;
const GAP_HALF: f64 = 100.0;

/// 右のグラフの軸の端 (ポイント)。最大の差を切り上げ、最小 10。
pub fn gap_axis_max(u: &TagUsage) -> f64 {
    let m = u
        .popular_up
        .iter()
        .chain(&u.popular_down)
        .filter_map(|r| r.gap_pt)
        .fold(0.0_f64, |a, g| a.max(g.abs()));
    ((m / 5.0).ceil() * 5.0).max(10.0)
}

/// 右のグラフ: 人気求人に多い語・少ない語 (差の棒、0 を中心に左右)。
pub(super) fn gap_chart(html: &mut String, u: &TagUsage) {
    html.push_str(
        "<figure class=\"chart tag-usage\"><figcaption>人気求人に多い語・少ない語</figcaption>",
    );
    if u.popular_n == 0 {
        html.push_str(&format!(
            "<p class=\"note\">{POPULARITY_ONLY_SP}</p></figure>"
        ));
        return;
    }
    let max = gap_axis_max(u);
    let per = GAP_HALF / max;
    let mut body = String::new();
    let mut y = TOP;
    let top = y;
    for (title, rows) in [
        ("人気求人に多い語", &u.popular_up),
        ("人気求人に少ない語", &u.popular_down),
    ] {
        body.push_str(&format!(
            "<text x=\"4\" y=\"{}\" font-size=\"13\" font-weight=\"bold\">{title} {}語</text>",
            y + 16.0,
            rows.len()
        ));
        y += BAND_HEAD;
        if rows.is_empty() {
            body.push_str(&format!(
                "<text x=\"{GAP_CENTER}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">該当する語はありません</text>",
                y + 12.0
            ));
            y += ROW_STEP;
        }
        for r in rows {
            let g = r.gap_pt.unwrap_or(0.0);
            let w = g.abs() * per;
            let (x, tx, anchor) = if g >= 0.0 {
                (GAP_CENTER, GAP_CENTER + w + 4.0, "start")
            } else {
                (GAP_CENTER - w, GAP_CENTER - w - 4.0, "end")
            };
            let color = if g >= 0.0 { "#c0504d" } else { "#4472c4" };
            body.push_str(&format!(
                "<text x=\"120\" y=\"{}\" text-anchor=\"end\" font-size=\"12\"><title>{}</title>{}</text><rect x=\"{x:.3}\" y=\"{}\" width=\"{w:.3}\" height=\"12\" fill=\"{color}\"><title>{}</title></rect><text x=\"{tx:.3}\" y=\"{}\" text-anchor=\"{anchor}\" font-size=\"11\">{}</text>",
                y + 12.0,
                escape_html(&r.word),
                escape_html(&short(&r.word)),
                y + 2.0,
                row_title(r),
                y + 12.0,
                signed(g)
            ));
            y += ROW_STEP;
        }
        y += BAND_GAP;
    }
    let bottom = y;
    let height = y + AXIS_ROOM;
    let mut axis = String::new();
    for (v, label) in [
        (-max, format!("-{max:.0}")),
        (0.0, "0".to_owned()),
        (max, format!("+{max:.0}")),
    ] {
        let x = GAP_CENTER + v * per;
        axis.push_str(&format!(
            "<path d=\"M {x} {top} V {bottom}\" stroke=\"#d5d9dd\"/><text x=\"{x}\" y=\"{}\" text-anchor=\"middle\" font-size=\"12\">{label}</text>",
            bottom + 14.0
        ));
    }
    html.push_str(&format!(
        "<svg viewBox=\"0 0 {W} {height}\" role=\"img\" aria-label=\"人気求人での割合から全体の割合を引いた差。人気求人に多い語と少ない語の上位{GAP_ROWS}語\"><title>人気求人と全体の割合の差（人気求人 {} 件 / 全体 {} 件）</title>{axis}{body}</svg>",
        u.popular_n, u.n
    ));
    html.push_str(&format!(
        "<p class=\"note\">棒は人気求人での割合から全体の割合を引いた差（ポイント）。人気求人 {} 件、全体 {} 件。件数3以上の語が対象。</p></figure>",
        u.popular_n, u.n
    ));
}

/// 「キーワード」タブ。左に帯別の表 3 枚、右に 2 つのグラフ。
pub(super) fn render_page(html: &mut String, u: &TagUsage) {
    html.push_str("<section class=\"excel-dashboard kw-tab\" aria-label=\"キーワード\"><aside class=\"summary\"><h1>キーワード</h1>");
    tables(html, u);
    html.push_str("</aside><div class=\"charts\">");
    bands_chart(html, u);
    gap_chart(html, u);
    html.push_str("</div></section>");
}

/// 3 つの帯の表 (語・件数・全体の割合・人気求人での割合・差)。n を必ず添える。
fn tables(html: &mut String, u: &TagUsage) {
    let has_popular = u.popular_n > 0;
    html.push_str("<h2>求人票キーワード調査（帯別）</h2>");
    for b in &u.bands {
        html.push_str(&format!(
            "<h3 class=\"band-title\">{} {}語（{}）</h3><table class=\"words\"><thead><tr><th>語</th><th>件数</th><th>全体の割合</th>{}</tr></thead><tbody>",
            escape_html(&b.label),
            b.rows.len(),
            escape_html(&b.range),
            if has_popular { "<th>人気求人での割合</th><th>差(pt)</th>" } else { "" }
        ));
        for r in b.rows.iter().take(TABLE_ROWS_PER_BAND) {
            let extra = match (r.popular_share_pct, r.gap_pt) {
                (Some(p), Some(g)) if has_popular => {
                    format!("<td>{p:.1}%</td><td>{}</td>", signed(g))
                }
                _ => String::new(),
            };
            html.push_str(&format!(
                "<tr><td>{}</td><td>{}</td><td>{:.1}%</td>{extra}</tr>",
                escape_html(&r.word),
                r.n_tag,
                r.share_pct
            ));
        }
        if b.rows.is_empty() {
            html.push_str(&format!(
                "<tr><td colspan=\"{}\">該当する語はありません</td></tr>",
                if has_popular { 5 } else { 3 }
            ));
        }
        if b.rows.len() > TABLE_ROWS_PER_BAND {
            html.push_str(&format!(
                "<tr><td colspan=\"{}\">ほか {} 語</td></tr>",
                if has_popular { 5 } else { 3 },
                b.rows.len() - TABLE_ROWS_PER_BAND
            ));
        }
        html.push_str("</tbody></table>");
    }
    html.push_str(&format!(
        "<p class=\"note word-basis\">母数：全体 {} 件 / 人気求人 {} 件（人気または超人気の表示がある求人）。件数1〜2の語（{}語）は帯に入れていません。差は人気求人での割合−全体の割合。</p>",
        u.n, u.popular_n, u.minor_word_count
    ));
    if !has_popular {
        html.push_str(&format!(
            "<p class=\"note word-basis\">{POPULARITY_ONLY_SP}</p>"
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(v: &[(&str, usize)]) -> Vec<(String, usize)> {
        v.iter().map(|(w, n)| ((*w).to_owned(), *n)).collect()
    }

    fn band_words<'a>(u: &'a TagUsage, key: &str) -> Vec<&'a str> {
        u.bands
            .iter()
            .find(|b| b.key == key)
            .unwrap()
            .rows
            .iter()
            .map(|r| r.word.as_str())
            .collect()
    }

    #[test]
    fn band_edges_are_inclusive_and_counts_below_three_are_minor() {
        // n = 10: 5 件 = 50% ちょうど → ほとんど。2 件 = 20% ちょうど でも件数 2 なので帯に入れない。
        let all = pairs(&[("A", 5), ("B", 4), ("C", 3), ("D", 2), ("E", 1)]);
        let u = build(&all, 10, &[], 0);
        assert_eq!(band_words(&u, "most"), ["A"]);
        assert_eq!(band_words(&u, "half"), ["B", "C"]);
        assert!(band_words(&u, "some").is_empty());
        assert_eq!(u.minor_word_count, 2);
        // n = 15: 3 件 = 20% ちょうど → 半数前後。n = 16 なら 3 件は 20% 未満 → 一部。
        assert_eq!(
            band_words(&build(&pairs(&[("A", 3)]), 15, &[], 0), "half"),
            ["A"]
        );
        assert_eq!(
            band_words(&build(&pairs(&[("A", 3)]), 16, &[], 0), "some"),
            ["A"]
        );
        // 件数 3 は帯に入る / 2 は入らない (n が大きい場合)。
        let u = build(&pairs(&[("A", 3), ("B", 2)]), 100, &[], 0);
        assert_eq!(band_words(&u, "some"), ["A"]);
        assert_eq!(u.minor_word_count, 1);
    }

    #[test]
    fn gap_sign_order_and_percentages() {
        // n = 20, 人気 4 件。A: 全体 10 (50%) 人気 4 (100%) 差 +50。B: 全体 10 (50%) 人気 0 差 -50。
        // C: 全体 6 (30%) 人気 2 (50%) 差 +20。
        let all = pairs(&[("A", 10), ("B", 10), ("C", 6)]);
        let pop = pairs(&[("A", 4), ("C", 2)]);
        let u = build(&all, 20, &pop, 4);
        assert_eq!((u.n, u.popular_n), (20, 4));
        let most = &u.bands[0].rows;
        assert_eq!(most[0].word, "A"); // 差が大きい順
        assert_eq!(most[1].word, "B");
        assert_eq!(most[0].gap_pt, Some(50.0));
        assert_eq!(most[0].popular_share_pct, Some(100.0));
        assert_eq!(most[1].gap_pt, Some(-50.0));
        assert_eq!((most[1].popular_tag_n, most[1].n_tag), (0, 10));
        assert_eq!(u.bands[1].rows[0].word, "C");
        assert_eq!(u.bands[1].rows[0].gap_pt, Some(20.0));
        let up: Vec<_> = u.popular_up.iter().map(|r| r.word.as_str()).collect();
        let down: Vec<_> = u.popular_down.iter().map(|r| r.word.as_str()).collect();
        assert_eq!(up, ["A", "C"]);
        assert_eq!(down, ["B"]);
    }

    #[test]
    fn without_popular_jobs_gap_is_null_and_order_is_by_count() {
        let all = pairs(&[("B", 6), ("A", 8), ("C", 6)]);
        let u = build(&all, 10, &[], 0);
        assert!(u.bands[0]
            .rows
            .iter()
            .all(|r| r.gap_pt.is_none() && r.popular_share_pct.is_none() && r.popular_tag_n == 0));
        assert_eq!(band_words(&u, "most"), ["A", "B", "C"]);
        assert!(u.popular_up.is_empty() && u.popular_down.is_empty());
        let mut html = String::new();
        gap_chart(&mut html, &u);
        assert!(html.contains(POPULARITY_ONLY_SP));
        assert!(!html.contains("<svg"));
    }

    #[test]
    fn non_condition_tags_are_dropped() {
        let all = pairs(&[("新着", 9), ("3日前", 9), ("30+", 9), ("賞与あり", 5)]);
        let u = build(&all, 10, &[], 0);
        assert_eq!(band_words(&u, "most"), ["賞与あり"]);
    }

    #[test]
    fn gap_lists_keep_only_ten_and_skip_minor_words() {
        let mut all: Vec<(String, usize)> = (0..14).map(|i| (format!("w{i:02}"), 4)).collect();
        all.push(("rare".into(), 2));
        let pop: Vec<(String, usize)> = (0..14)
            .map(|i| (format!("w{i:02}"), i % 4))
            .chain([("rare".to_owned(), 2)])
            .collect();
        let u = build(&all, 20, &pop, 4);
        assert_eq!(u.popular_up.len() + u.popular_down.len() <= 20, true);
        assert!(u.popular_up.iter().all(|r| r.word != "rare"));
        assert!(u.popular_up.len() <= 10 && u.popular_down.len() <= 10);
    }

    #[test]
    fn html_shows_n_hover_titles_and_signed_gap() {
        let all = pairs(&[("A", 10), ("B", 10)]);
        let pop = pairs(&[("A", 4)]);
        let u = build(&all, 20, &pop, 4);
        let mut t = String::new();
        tables(&mut t, &u);
        assert!(t.contains("<td>A</td><td>10</td><td>50.0%</td><td>100.0%</td><td>+50.0</td>"));
        assert!(t.contains("<td>B</td><td>10</td><td>50.0%</td><td>0.0%</td><td>-50.0</td>"));
        assert!(t.contains("全体 20 件 / 人気求人 4 件"));
        let mut c = String::new();
        bands_chart(&mut c, &u);
        assert!(c.contains("<title>A: 10件・50.0%"));
        let mut g = String::new();
        gap_chart(&mut g, &u);
        assert!(g.contains("差 +50.0"));
        assert!(g.contains("<title>B: 10件・50.0% / 人気求人 0件・0.0% / 差 -50.0</title>"));
    }
}
