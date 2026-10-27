//! 競合調査の「Google検索需要」ページ。複数の検索語を、最大 48 か月・年ごとの重ね合わせ・要約表で見せる。
//!
//! 数値の計算 (`summarize` / `yearly_lines` / `pivot`) は純粋関数で、描画から分けてテストする。
use std::collections::BTreeMap;

use super::common::push_page_head;
use super::competitor_keyword_analysis::multi_line_chart;
use super::competitor_model::{
    GoogleDemand, GoogleKeyword, GoogleSeason, GoogleSection, GoogleSuggestions, IndeedSeason,
    MarketSeason,
};
use crate::handlers::helpers::{escape_html, format_number};

/// 検索語ごとの要約。`yoy_pct` は直近 12 か月とその前の 12 か月の両方が全て数値のときだけ。
#[derive(Debug, Clone, PartialEq)]
pub(super) struct Summary {
    pub avg_monthly: Option<f64>,
    pub last12: Option<f64>,
    pub yoy_pct: Option<f64>,
    pub peak_month: Option<u32>,
}

fn parse_month(month: &str) -> Option<(i32, u32)> {
    let (y, m) = month.split_once('-')?;
    let m: u32 = m.get(..2).unwrap_or(m).parse().ok()?;
    ((1..=12).contains(&m)).then_some((y.parse().ok()?, m))
}

fn window_sum(values: &[Option<f64>]) -> Option<f64> {
    values.iter().copied().sum::<Option<f64>>()
}

pub(super) fn summarize(k: &GoogleKeyword) -> Summary {
    let values: Vec<Option<f64>> = k.monthly.iter().map(|m| m.search_volume).collect();
    let n = values.len();
    let last12 = (n >= 12).then(|| window_sum(&values[n - 12..])).flatten();
    let prev12 = (n >= 24)
        .then(|| window_sum(&values[n - 24..n - 12]))
        .flatten();
    let yoy_pct = match (last12, prev12) {
        (Some(now), Some(before)) if before > 0.0 => Some((now / before - 1.0) * 100.0),
        _ => None,
    };
    // 暦月ごとの平均 (2 年以上) が最大の月。2 年未満は最大の月。同じなら早い暦月。
    let mut by_month: BTreeMap<u32, (f64, u32)> = BTreeMap::new();
    for m in &k.monthly {
        if let (Some((_, cal)), Some(v)) = (parse_month(&m.month), m.search_volume) {
            let e = by_month.entry(cal).or_insert((0.0, 0));
            e.0 += v;
            e.1 += 1;
        }
    }
    let peak_month = if n >= 24 {
        by_month
            .iter()
            .map(|(cal, (sum, count))| (*cal, sum / f64::from(*count)))
            .fold(None, |best: Option<(u32, f64)>, cur| match best {
                Some(b) if b.1 >= cur.1 => Some(b),
                _ => Some(cur),
            })
            .map(|(cal, _)| cal)
    } else {
        k.monthly
            .iter()
            .filter_map(|m| Some((parse_month(&m.month)?.1, m.search_volume?)))
            .fold(None, |best: Option<(u32, f64)>, cur| match best {
                Some(b) if b.1 >= cur.1 => Some(b),
                _ => Some(cur),
            })
            .map(|(cal, _)| cal)
    };
    Summary {
        avg_monthly: k.avg_monthly,
        last12,
        yoy_pct,
        peak_month,
    }
}

/// 暦年ごとの 1〜12 月 (古い年から)。その年に無い月は None。
pub(super) fn yearly_lines(k: &GoogleKeyword) -> Vec<(String, Vec<Option<f64>>)> {
    let mut years: BTreeMap<i32, Vec<Option<f64>>> = BTreeMap::new();
    for m in &k.monthly {
        if let Some((year, cal)) = parse_month(&m.month) {
            years.entry(year).or_insert_with(|| vec![None; 12])[cal as usize - 1] = m.search_volume;
        }
    }
    years.into_iter().map(|(y, v)| (y.to_string(), v)).collect()
}

/// 全検索語の月を合わせた軸 (昇順) と、軸に揃えた系列。
fn aligned(keywords: &[GoogleKeyword]) -> (Vec<String>, Vec<(String, Vec<Option<f64>>)>) {
    let mut months: Vec<String> = keywords
        .iter()
        .flat_map(|k| k.monthly.iter().map(|m| m.month.clone()))
        .collect();
    months.sort();
    months.dedup();
    let lines = keywords
        .iter()
        .map(|k| {
            let values = months
                .iter()
                .map(|m| {
                    k.monthly
                        .iter()
                        .find(|x| &x.month == m)
                        .and_then(|x| x.search_volume)
                })
                .collect();
            (k.keyword.clone(), values)
        })
        .collect();
    (months, lines)
}

fn compact(s: &str) -> String {
    s.split_whitespace().collect()
}

fn int(v: Option<f64>) -> String {
    v.map(|v| format_number(v.round() as i64))
        .unwrap_or_else(|| "—".into())
}

fn number(value: Option<f64>) -> String {
    value
        .map(|n| {
            if n.fract() == 0.0 {
                format_number(n as i64)
            } else {
                format!("{n:.2}")
            }
        })
        .unwrap_or_else(|| "—".into())
}

fn competition_ja(raw: &str) -> String {
    match raw {
        "HIGH" => "高".into(),
        "MEDIUM" => "中".into(),
        "LOW" => "低".into(),
        "" | "UNSPECIFIED" | "UNKNOWN" => "—".into(),
        other => escape_html(other),
    }
}

pub(super) fn render_google(html: &mut String, data: &GoogleSection) {
    html.push_str("<section class=\"page-navy\" id=\"competitor-google\">");
    push_page_head(
        html,
        "検索需要",
        "Google広告APIの検索需要",
        "月間検索数と推移",
    );
    html.push_str("<p class=\"note\">出典：Google広告 Keyword Planner API｜検索数は推定値。広告競合度は広告主間の競合です。</p>");
    let (keyword, region, demand, suggestions, season) = match data {
        GoogleSection::NotRequested { message } | GoogleSection::Error { message } => {
            html.push_str(&format!("<p>{}</p></section>", escape_html(message)));
            return;
        }
        GoogleSection::Ok {
            keyword,
            region,
            demand,
            suggestions,
            season,
        } => (keyword, region, demand, suggestions, season),
    };
    html.push_str(&format!(
        "<p>検索語: {} / 指定地域: {}</p>",
        escape_html(keyword),
        if region.is_empty() {
            "全国".into()
        } else {
            escape_html(region)
        }
    ));
    if let GoogleDemand::Ok {
        region_name,
        keywords,
    } = demand
    {
        match region_name {
            None if !region.is_empty() => html.push_str(
                "<p class=\"note\">指定地域を解決できなかったため全国の検索需要です。</p>",
            ),
            Some(name) => html.push_str(&format!(
                "<p class=\"note\">取得地域: {}</p>",
                escape_html(name)
            )),
            None => {}
        }
        render_demand(html, keyword, keywords);
        if let Some(season) = season {
            render_season(html, season);
        }
    } else {
        // 外部APIの生エラーは資格情報を含む可能性があるので本文に転記しない。
        html.push_str("<p class=\"note\">Google検索需要を取得できませんでした。</p>");
    }
    // 取得できなかった関連語は空の見出しを出さない。
    if let GoogleSuggestions::Ok {
        region_name,
        suggestions,
    } = suggestions
    {
        html.push_str("<div class=\"block-title\">関連キーワード（上位20件）</div>");
        match region_name {
            None => html.push_str("<p class=\"note\">関連キーワードの取得地域：全国（地域指定なし・地域未解決）。指定地域の需要とは限りません。</p>"),
            Some(name) => html.push_str(&format!(
                "<p class=\"note\">関連キーワードの取得地域：{}</p>",
                escape_html(name)
            )),
        }
        html.push_str("<table class=\"table-navy\"><thead><tr><th>関連語</th><th>平均月間検索数</th></tr></thead><tbody>");
        if suggestions.is_empty() {
            html.push_str("<tr><td colspan=\"2\">関連キーワードのデータがありません。需要0を意味しません。</td></tr>");
        }
        for row in suggestions {
            html.push_str(&format!(
                "<tr><td>{}</td><td class=\"num\">{}</td></tr>",
                escape_html(&row.keyword),
                number(row.avg_monthly)
            ));
        }
        html.push_str(
            "</tbody></table><p class=\"note\">求人票の訴求語と検索語を比較できます。</p>",
        );
    }
    html.push_str("</section>");
}

fn render_demand(html: &mut String, seed: &str, keywords: &[GoogleKeyword]) {
    html.push_str("<table class=\"table-navy\"><thead><tr><th>検索語</th><th>平均月間検索数</th><th>直近12か月の合計</th><th>前年同期比</th><th>ピーク月</th><th>広告競合度</th></tr></thead><tbody>");
    if keywords.is_empty() {
        html.push_str("<tr><td colspan=\"6\">検索需要のデータがありません。</td></tr>");
    }
    for row in keywords {
        let s = summarize(row);
        html.push_str(&format!(
            "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
            escape_html(&row.keyword),
            number(s.avg_monthly),
            int(s.last12),
            s.yoy_pct.map(|v| format!("{v:+.1}%")).unwrap_or_else(|| "—".into()),
            s.peak_month.map(|m| format!("{m}月")).unwrap_or_else(|| "—".into()),
            competition_ja(&row.competition)
        ));
    }
    html.push_str("</tbody></table>");
    if keywords.is_empty() {
        return;
    }
    let (months, lines) = aligned(keywords);
    multi_line_chart(
        html,
        "検索語ごとの月間検索数の推移",
        &months,
        &lines,
        "回/月",
    );
    let main = keywords
        .iter()
        .find(|k| compact(&k.keyword) == compact(seed))
        .unwrap_or(&keywords[0]);
    let yearly = yearly_lines(main);
    if yearly.is_empty() {
        return;
    }
    let axis: Vec<String> = (1..=12).map(|m| format!("{m}月")).collect();
    multi_line_chart(
        html,
        &format!("{}：年ごとの重ね合わせ", main.keyword),
        &axis,
        &yearly,
        "回/月",
    );
    html.push_str(&format!(
        "<div class=\"block-title\">{} の月別検索数（年別）</div><table class=\"table-navy\"><thead><tr><th>月</th>",
        escape_html(&main.keyword)
    ));
    for (year, _) in &yearly {
        html.push_str(&format!("<th>{}</th>", escape_html(year)));
    }
    html.push_str("</tr></thead><tbody>");
    for (i, label) in axis.iter().enumerate() {
        html.push_str(&format!("<tr><td>{label}</td>"));
        for (_, values) in &yearly {
            html.push_str(&format!("<td class=\"num\">{}</td>", number(values[i])));
        }
        html.push_str("</tr>");
    }
    html.push_str("</tbody></table>");
}

/// 季節の型の折れ線 1 本分 (暦月 1〜12)。`range` は年ごとの最小〜最大 (市場全体は無し)。
struct SeasonLine {
    month: u32,
    mean: f64,
    range: Option<(f64, f64)>,
}

const IDX_LEFT: f64 = 85.0;
const IDX_W: f64 = 791.0;
const IDX_TOP: f64 = 24.0;
const IDX_H: f64 = 180.0;

/// 暦月 1〜12 の指数 (年平均 100) の折れ線・最小〜最大の帯・今の月の印。Indeed の点は線でつながない。
fn season_chart(
    html: &mut String,
    label: &str,
    line_name: &str,
    color: &str,
    lines: &[SeasonLine],
    current_month: u32,
    indeed: Option<&IndeedSeason>,
) {
    if lines.is_empty() {
        return;
    }
    let mut lo = 100.0f64;
    let mut hi = 100.0f64;
    for l in lines {
        let (a, b) = l.range.unwrap_or((l.mean, l.mean));
        lo = lo.min(a).min(l.mean);
        hi = hi.max(b).max(l.mean);
    }
    if let Some(i) = indeed {
        for s in &i.series {
            for p in &s.points {
                lo = lo.min(p.index);
                hi = hi.max(p.index);
            }
        }
    }
    let lo = ((lo - 5.0) / 20.0).floor() * 20.0;
    let hi = ((hi + 5.0) / 20.0).ceil() * 20.0;
    let lo = lo.max(0.0);
    let span = (hi - lo).max(20.0);
    let y_of = |v: f64| IDX_TOP + IDX_H * (1.0 - (v - lo) / span);
    let x_of = |m: u32| IDX_LEFT + IDX_W * f64::from(m - 1) / 11.0;
    let height = 300.0;
    html.push_str(&format!(
        "<figure class=\"trend-card\"><figcaption>{}</figcaption><div class=\"trend-plot\"><svg viewBox=\"0 0 900 {height}\" role=\"img\" aria-label=\"{}\"><title>{}</title>",
        escape_html(label), escape_html(label), escape_html(label)
    ));
    let mut g = lo;
    while g <= hi + 0.001 {
        html.push_str(&format!(
            "<path d=\"M{IDX_LEFT} {y:.1} H876\" stroke=\"#e1e8ec\"/><text x=\"75\" y=\"{ty:.1}\" font-size=\"14\" text-anchor=\"end\">{}</text>",
            format_number(g as i64),
            y = y_of(g),
            ty = y_of(g) + 5.0
        ));
        g += 20.0;
    }
    // 今の月の印 (帯)。
    if (1..=12).contains(&current_month) {
        let x = x_of(current_month);
        html.push_str(&format!(
            "<rect x=\"{:.1}\" y=\"{IDX_TOP}\" width=\"36\" height=\"{IDX_H}\" fill=\"#f3e9c8\" opacity=\"0.7\"/><text x=\"{x:.1}\" y=\"{:.1}\" font-size=\"13\" text-anchor=\"middle\" fill=\"#8a6d1d\">今月</text>",
            x - 18.0,
            IDX_TOP + IDX_H + 36.0
        ));
    }
    // 年平均 100 の基準線。
    html.push_str(&format!(
        "<path d=\"M{IDX_LEFT} {y:.1} H876\" stroke=\"#8a949b\" stroke-dasharray=\"5 4\"/>",
        y = y_of(100.0)
    ));
    // 最小〜最大の帯。
    if lines.iter().all(|l| l.range.is_some()) {
        let upper: Vec<String> = lines
            .iter()
            .map(|l| format!("{:.1},{:.1}", x_of(l.month), y_of(l.range.unwrap().1)))
            .collect();
        let lower: Vec<String> = lines
            .iter()
            .rev()
            .map(|l| format!("{:.1},{:.1}", x_of(l.month), y_of(l.range.unwrap().0)))
            .collect();
        html.push_str(&format!(
            "<polygon points=\"{} {}\" fill=\"{color}\" opacity=\"0.16\" stroke=\"none\"/>",
            upper.join(" "),
            lower.join(" ")
        ));
    }
    let pts: Vec<String> = lines
        .iter()
        .map(|l| format!("{:.1},{:.1}", x_of(l.month), y_of(l.mean)))
        .collect();
    html.push_str(&format!(
        "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"3\"/>",
        pts.join(" ")
    ));
    let mut hits = String::new();
    for l in lines {
        let tip = match l.range {
            Some((a, b)) => format!(
                "{}月　年平均比 {}（最小 {}〜最大 {}）",
                l.month,
                l.mean.round() as i64,
                a.round() as i64,
                b.round() as i64
            ),
            None => format!("{}月　年平均比 {}", l.month, l.mean.round() as i64),
        };
        hits.push_str(&format!(
            "<circle cx=\"{:.1}\" cy=\"{:.1}\" r=\"9\" fill=\"transparent\" stroke=\"none\" pointer-events=\"all\"><title>{}</title></circle>",
            x_of(l.month),
            y_of(l.mean),
            escape_html(&tip)
        ));
    }
    // Indeed の点 (線でつながない)。閲覧数 = ひし形、求人数 = 四角。
    let mut legend = vec![(line_name.to_owned(), color.to_owned(), 'l')];
    if let Some(i) = indeed {
        for (k, s) in i.series.iter().enumerate() {
            let (c, shape) = if k == 0 {
                ("#c0504d", 'd')
            } else {
                ("#8567a5", 's')
            };
            legend.push((format!("Indeed {}", s.label), c.to_owned(), shape));
            for p in &s.points {
                let (x, y) = (x_of(p.month), y_of(p.index));
                let mark = if shape == 'd' {
                    format!(
                        "<path d=\"M{x:.1} {:.1} l5.5 5.5 l-5.5 5.5 l-5.5 -5.5 z\" fill=\"{c}\" stroke=\"#fff\" stroke-width=\"1\"/>",
                        y - 5.5
                    )
                } else {
                    format!(
                        "<rect x=\"{:.1}\" y=\"{:.1}\" width=\"9\" height=\"9\" fill=\"{c}\" stroke=\"#fff\" stroke-width=\"1\"/>",
                        x - 4.5,
                        y - 4.5
                    )
                };
                html.push_str(&mark);
                hits.push_str(&format!(
                    "<circle cx=\"{x:.1}\" cy=\"{y:.1}\" r=\"9\" fill=\"transparent\" stroke=\"none\" pointer-events=\"all\"><title>{}</title></circle>",
                    escape_html(&format!(
                        "{}月 {}年　{} 年平均比 {}",
                        p.month,
                        p.year,
                        s.label,
                        p.index.round() as i64
                    ))
                ));
            }
        }
    }
    for m in 1..=12u32 {
        html.push_str(&format!(
            "<text x=\"{:.1}\" y=\"{:.1}\" text-anchor=\"middle\" font-size=\"14\">{m}月</text>",
            x_of(m),
            IDX_TOP + IDX_H + 22.0
        ));
    }
    for (k, (name, c, shape)) in legend.iter().enumerate() {
        let lx = IDX_LEFT + 220.0 * k as f64;
        let ly = 272.0;
        let mark = match shape {
            'l' => format!(
                "<rect x=\"{lx}\" y=\"{}\" width=\"22\" height=\"4\" fill=\"{c}\"/>",
                ly - 6.0
            ),
            'd' => format!(
                "<path d=\"M{} {} l5.5 5.5 l-5.5 5.5 l-5.5 -5.5 z\" fill=\"{c}\"/>",
                lx + 11.0,
                ly - 10.0
            ),
            _ => format!(
                "<rect x=\"{}\" y=\"{}\" width=\"9\" height=\"9\" fill=\"{c}\"/>",
                lx + 6.0,
                ly - 9.0
            ),
        };
        html.push_str(&mark);
        html.push_str(&format!(
            "<text x=\"{}\" y=\"{ly}\" font-size=\"14\">{}</text>",
            lx + 30.0,
            escape_html(name)
        ));
    }
    html.push_str(&hits);
    html.push_str("</svg></div><p class=\"note\">縦軸は年平均を 100 とした指数です。</p></figure>");
}

fn market_lines(m: &MarketSeason) -> Vec<SeasonLine> {
    m.months
        .iter()
        .map(|x| SeasonLine {
            month: x.month,
            mean: x.mean,
            range: None,
        })
        .collect()
}

/// 「暦月の型（全国）」。言えるときは職種の型を主に、言えないときは市場全体の型を主にする。
fn render_season(html: &mut String, s: &GoogleSeason) {
    html.push_str("<div class=\"block-title\">暦月の型（全国）</div>");
    html.push_str(&format!(
        "<p class=\"note\">{} の全国の検索数。完全な年（12 か月そろった年）{} 年分から、年平均を 100 とした暦月の指数を出しています。</p>",
        escape_html(&s.keyword),
        s.full_years.len()
    ));
    html.push_str(&format!(
        "<p><strong>{}</strong></p>",
        escape_html(&s.headline)
    ));
    if let Some(c) = &s.caveat {
        html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(c)));
    }
    let google_lines: Vec<SeasonLine> = s
        .months
        .iter()
        .map(|m| SeasonLine {
            month: m.month,
            mean: m.mean,
            range: Some((m.min, m.max)),
        })
        .collect();
    if !s.reliable {
        if let Some(m) = &s.market {
            html.push_str(&format!(
                "<p>{}（{} 職種の平均）</p>",
                escape_html(&m.headline),
                m.n_series
            ));
            season_chart(
                html,
                "職種全体の暦月の型",
                "職種全体の平均",
                "#5b6770",
                &market_lines(m),
                s.current_month,
                None,
            );
        }
    }
    let caption = if s.reliable {
        format!("{}：暦月の型", s.keyword)
    } else {
        format!("{}：暦月の指数（参考）", s.keyword)
    };
    season_chart(
        html,
        &caption,
        "Google 検索数の平均",
        "#007d79",
        &google_lines,
        s.current_month,
        s.indeed.as_ref(),
    );
    if let Some(i) = &s.indeed {
        html.push_str(&format!(
            "<p class=\"note\">{}</p>",
            escape_html(&i.span_note)
        ));
        html.push_str(&format!(
            "<p class=\"note\">{}</p>",
            escape_html(&i.shift_note)
        ));
    }
    if s.reliable {
        if !s.next_three.is_empty() {
            html.push_str("<table class=\"table-navy\"><thead><tr><th>月</th><th>年平均比</th></tr></thead><tbody>");
            if let Some(v) = s.current_index {
                html.push_str(&format!(
                    "<tr><td>{}月（今月）</td><td class=\"num\">{}</td></tr>",
                    s.current_month,
                    v.round() as i64
                ));
            }
            for n in &s.next_three {
                html.push_str(&format!(
                    "<tr><td>{}月</td><td class=\"num\">{}</td></tr>",
                    n.month,
                    n.index.round() as i64
                ));
            }
            html.push_str("</tbody></table>");
        }
        if let Some(m) = &s.market {
            html.push_str(&format!(
                "<p class=\"note\">{}（{} 職種の平均）</p>",
                escape_html(&m.headline),
                m.n_series
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::competitor_model::GoogleMonth;
    use super::*;

    fn kw(name: &str, start: (i32, u32), values: &[Option<f64>]) -> GoogleKeyword {
        let (mut y, mut m) = start;
        let mut monthly = Vec::new();
        for v in values {
            monthly.push(GoogleMonth {
                month: format!("{y}-{m:02}"),
                search_volume: *v,
            });
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
        GoogleKeyword {
            keyword: name.into(),
            avg_monthly: Some(100.0),
            competition: "HIGH".into(),
            monthly,
        }
    }

    #[test]
    fn yoy_needs_both_twelve_month_windows_complete() {
        // 2024-08〜2026-07 の 24 か月。前の 12 か月は 100、直近 12 か月は 150。
        let mut v: Vec<Option<f64>> = vec![Some(100.0); 12];
        v.extend(vec![Some(150.0); 12]);
        let s = summarize(&kw("a", (2024, 8), &v));
        assert_eq!(s.last12, Some(1800.0));
        assert_eq!(s.yoy_pct, Some(50.0));
        // 前年側が 1 か月欠ければ前年同期比は出さない (直近 12 か月の合計は出す)。
        v[3] = None;
        let s = summarize(&kw("a", (2024, 8), &v));
        assert_eq!(s.last12, Some(1800.0));
        assert_eq!(s.yoy_pct, None);
        // 直近側が欠ければ合計も出さない。
        v[3] = Some(100.0);
        v[20] = None;
        let s = summarize(&kw("a", (2024, 8), &v));
        assert_eq!(s.last12, None);
        assert_eq!(s.yoy_pct, None);
        // 12 か月だけなら合計は出るが前年同期比は「—」。
        let s = summarize(&kw("a", (2025, 8), &[Some(10.0); 12]));
        assert_eq!(s.last12, Some(120.0));
        assert_eq!(s.yoy_pct, None);
    }

    #[test]
    fn peak_month_uses_calendar_month_average_over_two_years() {
        // 2024-08 始まり。3 月は 2025 年 300 / 2026 年 100 で平均 200、
        // 11 月は 2024 年・2025 年とも 250 で平均 250。単月の最大は 3 月 (300) だが、暦月の平均が最大なのは 11 月 (250)。
        let mut v: Vec<Option<f64>> = vec![Some(10.0); 24];
        v[7] = Some(300.0); // 2025-03
        v[19] = Some(100.0); // 2026-03
        v[3] = Some(250.0); // 2024-11
        v[15] = Some(250.0); // 2025-11
        let s = summarize(&kw("a", (2024, 8), &v));
        assert_eq!(s.peak_month, Some(11));
        // 2 年未満は最大の単月。
        let mut short: Vec<Option<f64>> = vec![Some(5.0); 12];
        short[4] = Some(90.0); // 2025-12
        assert_eq!(summarize(&kw("a", (2025, 8), &short)).peak_month, Some(12));
    }

    #[test]
    fn yearly_lines_make_one_series_per_calendar_year() {
        // 2023-08〜2026-07 の 36 か月 = 4 つの暦年。
        let v: Vec<Option<f64>> = (0..36).map(|i| Some(f64::from(i))).collect();
        let lines = yearly_lines(&kw("a", (2023, 8), &v));
        let names: Vec<_> = lines.iter().map(|l| l.0.as_str()).collect();
        assert_eq!(names, ["2023", "2024", "2025", "2026"]);
        assert_eq!(lines[0].1[7], Some(0.0));
        assert_eq!(lines[0].1[0], None);
        assert_eq!(lines[3].1[6], Some(35.0));
        assert_eq!(lines[3].1[7], None);
    }

    fn data(keywords: serde_json::Value) -> GoogleSection {
        super::super::competitor_model::google_section(&serde_json::json!({
            "status":"ok","keyword":"製造 求人","region":"",
            "demand":{"status":"ok","region":null,"keywords":keywords},
            "suggestions":{"status":"ok","region":null,"suggestions":[]}
        }))
    }

    fn months_json(start: (i32, u32), n: usize) -> serde_json::Value {
        let (mut y, mut m) = start;
        let mut out = Vec::new();
        for i in 0..n {
            out.push(
                serde_json::json!({"month":format!("{y}-{m:02}"),"search_volume":100 + i as i64}),
            );
            m += 1;
            if m > 12 {
                m = 1;
                y += 1;
            }
        }
        serde_json::Value::Array(out)
    }

    #[test]
    fn html_has_hover_titles_year_overlay_and_pivot() {
        let section = data(serde_json::json!([
            {"keyword":"製造 求人","avg_monthly":210,"competition":"HIGH","monthly_12m":months_json((2022, 8), 48)},
            {"keyword":"工場 仕事","avg_monthly":90,"competition":"LOW","monthly_12m":months_json((2022, 8), 48)}
        ]));
        let mut html = String::new();
        render_google(&mut html, &section);
        // ホバー: 月・系列名・値。
        assert!(html.contains("<title>2022-08　製造 求人: 100回/月</title>"));
        assert!(html.contains("fill=\"transparent\""));
        // 年ごとの重ね合わせは 5 本 (2022〜2026)。ピボット表の列見出しも西暦。
        assert!(html.contains("年ごとの重ね合わせ"));
        assert!(html.contains("<title>1月　2026: 141回/月</title>"));
        for y in 2022..=2026 {
            assert!(html.contains(&format!("<th>{y}</th>")), "{y}");
        }
        assert!(
            !html.contains("月別検索数</div><table"),
            "縦長の月別表は廃止"
        );
        assert_eq!(html.matches("の月別検索数（年別）").count(), 1);
        // 要約表: 直近12か月合計 = 136..147 の和 (100+36..100+47) = 1,458? 計算は下で確認。
        let last12: i64 = (36..48).map(|i| 100 + i).sum();
        assert!(html.contains(&format_number(last12)));
        assert!(html.contains("前年同期比"));
    }

    fn season_section(base: f64, cur: u32, indeed_months: usize) -> GoogleSection {
        let mut months = Vec::new();
        for y in 2022..2026 {
            for m in 1..=12u32 {
                let f = match m {
                    3 => 1.3,
                    8 => 0.7,
                    _ => 1.0,
                };
                months.push(
                    serde_json::json!({"month":format!("{y}-{m:02}"),"search_volume":base*f}),
                );
            }
        }
        let g = serde_json::json!({
            "status":"ok","keyword":"飲食店 求人","region":"",
            "demand":{"status":"ok","region":null,"keywords":[]},
            "suggestions":{"status":"ok","region":null,"suggestions":[]},
            "season_source":{"keyword":"飲食店 求人","months":months,"current_month":cur,
                "market":{"months":[100,100,109,100,109,100,100,94,100,100,93,83],"n_series":84}}
        });
        let mut section = super::super::competitor_model::google_section(&g);
        if indeed_months > 0 {
            let (mut y, mut m) = (2025, 5u32);
            let rows: Vec<_> = (0..indeed_months)
                .map(|i| {
                    let r = super::super::competitor_model::IndeedRow {
                        month: format!("{y}-{m:02}"),
                        job: Some(100.0),
                        ctk: Some(if i == 0 { 200.0 } else { 100.0 }),
                        emp: None,
                        spp: None,
                    };
                    m += 1;
                    if m > 12 {
                        m = 1;
                        y += 1;
                    }
                    r
                })
                .collect();
            if let GoogleSection::Ok {
                season: Some(s), ..
            } = &mut section
            {
                s.indeed = super::super::competitor_season::indeed_overlay(&rows, None);
            }
        }
        section
    }

    #[test]
    fn season_block_reliable_shows_headline_hover_current_month_and_indeed_points() {
        let mut html = String::new();
        render_google(&mut html, &season_section(2000.0, 11, 14));
        assert!(html.contains("暦月の型（全国）"));
        assert!(html.contains(
            "例年、3月ごろに最も多く（年平均比 +30%）、8月ごろに最も少ない（年平均比 −30%）。"
        ));
        assert!(html.contains("<title>3月　年平均比 130（最小 130〜最大 130）</title>"));
        assert!(html.contains(">今月</text>"));
        // 今月 (11 月) の次の 3 か月。
        assert!(html.contains("<tr><td>11月（今月）</td><td class=\"num\">100</td></tr>"));
        assert!(html.contains("<tr><td>2月</td><td class=\"num\">100</td></tr>"));
        // Indeed の点 (2025-05 の閲覧数は年平均比 187) と事実の 1 文、24 か月未満の予告。
        assert!(html.contains("<title>5月 2025年　閲覧数 年平均比 187</title>"));
        assert!(html
            .contains("Indeed は 2025-05〜2026-06 の 14 か月分で、暦月ごとの点数は 1〜2 点です。"));
        assert!(html.contains("Indeed が 24 か月分たまると"));
        // 市場全体は 1 行の注記だけ (信頼できるときは主にしない)。
        assert!(!html.contains("職種全体の暦月の型"));
        assert!(html.contains("84 職種の平均"));
    }

    #[test]
    fn season_block_unreliable_shows_facts_and_market_as_main_without_indeed_line() {
        let mut html = String::new();
        render_google(&mut html, &season_section(900.0, 1, 0));
        assert!(html.contains("時期の型は出せません。（全国の月平均 900 回・完全な年 4 年）"));
        assert!(!html.contains("ごろに最も多く"));
        assert!(html.contains("職種全体の暦月の型"));
        assert!(html.contains("<title>3月　年平均比 109</title>"));
        // Indeed が無ければ Indeed の線も文も出ない。次の 3 か月の表も出さない。
        assert!(!html.contains("Indeed"));
        assert!(!html.contains("（今月）"));
        assert!(html.ends_with("</section>"));
    }

    #[test]
    fn season_block_absent_without_season_source() {
        let mut html = String::new();
        render_google(
            &mut html,
            &data(serde_json::json!([
                {"keyword":"製造 求人","avg_monthly":210,"competition":"HIGH","monthly_12m":months_json((2022, 8), 48)}
            ])),
        );
        assert!(!html.contains("暦月の型"));
    }

    #[test]
    fn html_does_not_break_with_only_twelve_months() {
        let section = data(serde_json::json!([
            {"keyword":"製造 求人","avg_monthly":210,"competition":"HIGH","monthly_12m":months_json((2025, 8), 12)}
        ]));
        let mut html = String::new();
        render_google(&mut html, &section);
        // 1 年に満たない系列でも要約表の前年同期比は「—」、直近12か月合計は出る。
        assert!(html.contains("<td class=\"num\">210</td><td class=\"num\">1,266</td><td class=\"num\">—</td><td>7月</td>"));
        assert!(html.contains("<th>2025</th><th>2026</th>"));
        assert!(html.ends_with("</section>"));
    }
}
