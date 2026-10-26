//! 競合調査の Indeed 採用レポートに付ける「キーワードの推移・分析」。
//! 入力は Indeed の分析スナップショットから読んだ値 (`keyword_raw`) だけで、外部 API は呼ばない。
//! 4 つの部分はそれぞれ独立に欠けてよく、欠けたときは 1 文の説明だけを返す。
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use crate::handlers::helpers::{escape_html, format_number};

/// 検索語の表に出す件数 (増えた語・減った語それぞれ)。
const SHIFT_ROWS: usize = 5;
/// 検索語の月次の線に出す本数。
const TERM_LINES: usize = 5;
/// 検索語の数がこれ未満の職種は、割合が大きく振れる (keywords.rs の調べ)。
const THIN_TERM_COUNT: f64 = 20.0;

const SEARCH_SCOPE: &str = "全国の検索数（都道府県別ではありません）";
const TERM_SCOPE: &str = "全国・この職種の検索語に占める割合";

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct KeywordAnalysis {
    pub search_trend: SearchTrendPart,
    pub term_shifts: TermShiftsPart,
    pub pref_terms: PrefTermsPart,
    pub overlay: OverlayPart,
    /// 表の下に出す短い注意書き。
    pub caveats: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum SearchTrendPart {
    Unavailable {
        message: String,
    },
    Ok {
        scope: String,
        months: Vec<String>,
        series: Vec<SearchSeries>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SearchSeries {
    /// 検索語そのまま (例: 「◯◯ 求人」)。
    pub label: String,
    /// "job" (職種名＋求人) か "name" (職種名のみ)。
    pub variant: String,
    /// `months` と同じ並び。欠測は null。
    pub values: Vec<Option<f64>>,
    pub latest: Option<f64>,
    pub latest_month: String,
    /// 前年比 (%)。
    pub yoy_pct: Option<f64>,
    /// ピークの暦月 (1〜12)。
    pub peak_month: Option<u32>,
    /// 広告の競合の強さ (HIGH / MEDIUM / LOW)。
    pub competition: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum TermShiftsPart {
    Unavailable {
        message: String,
    },
    Ok {
        scope: String,
        /// 比べた 2 つの期間 (例 "2025-07〜2025-09")。
        before_period: String,
        after_period: String,
        rising: Vec<TermShiftRow>,
        falling: Vec<TermShiftRow>,
        months: Vec<String>,
        lines: Vec<TermLine>,
        /// 検索語の数が少なく割合が振れやすいときの注意。
        thin_note: Option<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TermShiftRow {
    pub term: String,
    pub before_pct: Option<f64>,
    pub after_pct: Option<f64>,
    /// 割合の変化 (ポイント)。
    pub diff_pt: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct TermLine {
    pub term: String,
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PrefTermsPart {
    Unavailable {
        message: String,
    },
    Ok {
        prefecture: String,
        month: String,
        /// 比べた月。比べられないときは null。
        compare_month: Option<String>,
        /// "12か月前" または "収録の最初の月"。
        compare_kind: Option<String>,
        /// 一覧に載った語のクリック合計。
        listed_clicks: i64,
        rows: Vec<PrefTermRow>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PrefTermRow {
    pub rank: u32,
    pub term: String,
    pub clicks: i64,
    /// 一覧に載った語の合計に占める割合 (%)。
    pub share_pct: f64,
    pub compare_share_pct: Option<f64>,
    /// 比べた月から割合がどれだけ動いたか (ポイント)。比べた月に無い語は null。
    pub change_pt: Option<f64>,
    /// 比べた月の一覧に載っていない (比べる月があるときだけ true)。
    pub out_of_range: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum OverlayPart {
    Unavailable {
        message: String,
    },
    Ok {
        base_month: String,
        months: Vec<String>,
        indeed_label: String,
        google_label: String,
        /// 最初の重なる月を 100 とした指数。
        indeed_index: Vec<f64>,
        google_index: Vec<f64>,
        indeed_raw: Vec<f64>,
        google_raw: Vec<f64>,
        caption: String,
    },
}

// ---------------------------------------------------------------- 組み立て

fn str_of(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn num_of(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

fn nums_of(v: &Value) -> Vec<Option<f64>> {
    v.as_array()
        .into_iter()
        .flatten()
        .map(Value::as_f64)
        .collect()
}

fn strs_of(v: &Value) -> Vec<String> {
    v.as_array()
        .into_iter()
        .flatten()
        .filter_map(|x| x.as_str().map(str::to_owned))
        .collect()
}

/// Indeed 部分の JSON (`rows` と `keyword_raw` を含む) から作る。`keyword_raw` が無ければ None。
pub(crate) fn build(data: &Value) -> Option<Box<KeywordAnalysis>> {
    let raw = data.get("keyword_raw").filter(|v| v.is_object())?;
    let title = str_of(data, "title");
    let region = str_of(data, "region");
    let search_trend = search_trend(&title, &raw["search"]);
    let google_job = match &search_trend {
        SearchTrendPart::Ok { months, series, .. } => series
            .iter()
            .find(|s| s.variant == "job")
            .map(|s| (months.clone(), s.label.clone(), s.values.clone())),
        SearchTrendPart::Unavailable { .. } => None,
    };
    let indeed_ctk: Vec<(String, Option<f64>)> = data["rows"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| (str_of(r, "month"), num_of(r, "ctk")))
        .collect();
    let entries: Vec<PrefMonthEntry> = raw["pref_months"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|e| PrefMonthEntry {
            month: str_of(e, "month"),
            available: e["kw_available"].as_i64().unwrap_or(0) != 0,
            top_keywords: str_of(e, "top_keywords"),
        })
        .collect();
    let prefecture = str_of(raw, "prefecture");
    Some(Box::new(KeywordAnalysis {
        search_trend,
        term_shifts: term_shifts_part(raw),
        pref_terms: pref_terms(&prefecture, &entries),
        overlay: overlay_part(&region, google_job, &indeed_ctk),
        caveats: vec![
            "Indeedの行動データのみで、労働市場全体ではありません。閲覧数は応募数ではありません。".to_owned(),
            "Googleの検索数は検索エンジンの推定値で、丸めた値です。「職種名 求人」を求職の目安にしています。".to_owned(),
        ],
    }))
}

fn search_trend(title: &str, rows: &Value) -> SearchTrendPart {
    let mut series: Vec<SearchSeries> = Vec::new();
    // 月の並びは職種名＋求人 (job) を先頭に、無ければ見つかった順。
    let mut ordered: Vec<&Value> = rows.as_array().into_iter().flatten().collect();
    ordered.sort_by_key(|r| u8::from(r["variant"].as_str() != Some("job")));
    let mut months: Vec<String> = Vec::new();
    for r in &ordered {
        for m in strs_of(&r["months"]) {
            if !months.contains(&m) {
                months.push(m);
            }
        }
    }
    months.sort();
    for r in &ordered {
        let variant = str_of(r, "variant");
        let own_months = strs_of(&r["months"]);
        let own_values = nums_of(&r["series"]);
        if own_months.is_empty() || own_months.len() != own_values.len() {
            continue;
        }
        let values: Vec<Option<f64>> = months
            .iter()
            .map(|m| {
                own_months
                    .iter()
                    .position(|x| x == m)
                    .and_then(|i| own_values[i])
            })
            .collect();
        series.push(SearchSeries {
            label: if variant == "job" {
                format!("{title} 求人")
            } else {
                title.to_owned()
            },
            variant,
            values,
            latest: num_of(r, "latest"),
            latest_month: str_of(r, "latest_month"),
            yoy_pct: num_of(r, "yoy_pct"),
            peak_month: r["peak_month"]
                .as_i64()
                .filter(|m| (1..=12).contains(m))
                .and_then(|m| u32::try_from(m).ok()),
            competition: str_of(r, "competition"),
        });
    }
    if series.is_empty() {
        return SearchTrendPart::Unavailable {
            message: "この職種のGoogle検索数のデータはありません。".to_owned(),
        };
    }
    SearchTrendPart::Ok {
        scope: SEARCH_SCOPE.to_owned(),
        months,
        series,
    }
}

struct ShiftIn {
    term: String,
    before: Option<f64>,
    after: Option<f64>,
    diff: Option<f64>,
}

/// 割合の変化が大きい順に、増えた語と減った語を上位 `n` 語ずつ返す。
/// 変化が 0 の語と、変化の欠測した語は載せない。同じ変化は語の五十音順 (文字コード順)。
fn rank_shifts(rows: &[ShiftIn], n: usize) -> (Vec<TermShiftRow>, Vec<TermShiftRow>) {
    let all: Vec<TermShiftRow> = rows
        .iter()
        .filter_map(|r| {
            Some(TermShiftRow {
                term: r.term.clone(),
                before_pct: r.before,
                after_pct: r.after,
                diff_pt: r.diff.filter(|d| d.is_finite())?,
            })
        })
        .collect();
    let mut rising: Vec<TermShiftRow> = all.iter().filter(|r| r.diff_pt > 0.0).cloned().collect();
    rising.sort_by(|a, b| {
        b.diff_pt
            .total_cmp(&a.diff_pt)
            .then_with(|| a.term.cmp(&b.term))
    });
    rising.truncate(n);
    let mut falling: Vec<TermShiftRow> = all.into_iter().filter(|r| r.diff_pt < 0.0).collect();
    falling.sort_by(|a, b| {
        a.diff_pt
            .total_cmp(&b.diff_pt)
            .then_with(|| a.term.cmp(&b.term))
    });
    falling.truncate(n);
    (rising, falling)
}

/// 直近の割合が大きい上位 `n` 語の月ごとの割合。
fn top_lines(terms: &[TermLine], n: usize) -> Vec<TermLine> {
    let latest = |t: &TermLine| {
        t.values
            .iter()
            .flatten()
            .next_back()
            .copied()
            .unwrap_or(0.0)
    };
    let mut v: Vec<TermLine> = terms.to_vec();
    v.sort_by(|a, b| {
        latest(b)
            .total_cmp(&latest(a))
            .then_with(|| a.term.cmp(&b.term))
    });
    v.truncate(n);
    v
}

fn term_shifts_part(raw: &Value) -> TermShiftsPart {
    let shifts: Vec<ShiftIn> = raw["shifts"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|r| ShiftIn {
            term: str_of(r, "term"),
            before: num_of(r, "before"),
            after: num_of(r, "after"),
            diff: num_of(r, "diff"),
        })
        .collect();
    let months = strs_of(&raw["term_monthly"]["months"]);
    let terms: Vec<TermLine> = raw["term_monthly"]["terms"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|t| TermLine {
            term: str_of(t, "term"),
            values: nums_of(&t["pct"]),
        })
        .filter(|t| t.values.len() == months.len())
        .collect();
    if shifts.is_empty() && terms.is_empty() {
        return TermShiftsPart::Unavailable {
            message: "この職種のIndeedの検索語の推移データはありません。".to_owned(),
        };
    }
    let (rising, falling) = rank_shifts(&shifts, SHIFT_ROWS);
    // 変化の表は収録範囲の最初と最後の 3 か月どうしの比較。
    let period = |s: &[String]| match (s.first(), s.last()) {
        (Some(a), Some(b)) => format!("{a}〜{b}"),
        _ => String::new(),
    };
    let (before_period, after_period) = if months.len() >= 6 {
        (period(&months[..3]), period(&months[months.len() - 3..]))
    } else {
        (String::new(), String::new())
    };
    let thin_note = num_of(raw, "term_count_latest")
        .filter(|c| *c < THIN_TERM_COUNT)
        .map(|c| {
            format!(
                "この職種は最新月の検索語が{}語と少なく、割合が大きく振れることがあります。",
                format_number(c as i64)
            )
        });
    TermShiftsPart::Ok {
        scope: TERM_SCOPE.to_owned(),
        before_period,
        after_period,
        rising,
        falling,
        lines: top_lines(&terms, TERM_LINES),
        months,
        thin_note,
    }
}

struct PrefMonthEntry {
    month: String,
    available: bool,
    top_keywords: String,
}

/// "2026-08" の `delta` か月後。読めなければ None。
fn shift_month(month: &str, delta: i32) -> Option<String> {
    let y: i32 = month.get(0..4)?.parse().ok()?;
    let m: i32 = month.get(5..7)?.parse().ok()?;
    if !(1..=12).contains(&m) || month.get(4..5) != Some("-") {
        return None;
    }
    let idx = y * 12 + (m - 1) + delta;
    Some(format!(
        "{:04}-{:02}",
        idx.div_euclid(12),
        idx.rem_euclid(12) + 1
    ))
}

fn parsed(entry: &PrefMonthEntry) -> Vec<(String, i64)> {
    if entry.available {
        crate::indeed::detail::parse_keywords(&entry.top_keywords)
    } else {
        Vec::new()
    }
}

fn shares(list: &[(String, i64)]) -> (i64, Vec<f64>) {
    let total: i64 = list.iter().map(|(_, c)| *c).sum();
    let v = list
        .iter()
        .map(|(_, c)| {
            if total > 0 {
                *c as f64 * 100.0 / total as f64
            } else {
                0.0
            }
        })
        .collect();
    (total, v)
}

/// 選んだ都道府県の検索語。最新月の上位語と、12 か月前 (無ければ最初の月) との割合の差。
fn pref_terms(prefecture: &str, entries: &[PrefMonthEntry]) -> PrefTermsPart {
    if prefecture.is_empty() {
        return PrefTermsPart::Unavailable {
            message: "都道府県を選ぶと、その都道府県の検索語を表示します（全国では表示しません）。"
                .to_owned(),
        };
    }
    let mut usable: Vec<(&PrefMonthEntry, Vec<(String, i64)>)> = entries
        .iter()
        .map(|e| (e, parsed(e)))
        .filter(|(_, l)| !l.is_empty())
        .collect();
    usable.sort_by(|a, b| a.0.month.cmp(&b.0.month));
    let Some((latest, list)) = usable.last() else {
        return PrefTermsPart::Unavailable {
            message: "この都道府県の検索語はIndeedから提供されていません。".to_owned(),
        };
    };
    let (listed_clicks, share) = shares(list);
    let earlier = shift_month(&latest.month, -12)
        .and_then(|m| usable.iter().find(|(e, _)| e.month == m))
        .map(|x| (x, "12か月前"))
        .or_else(|| {
            usable
                .first()
                .filter(|(e, _)| e.month != latest.month)
                .map(|x| (x, "収録の最初の月"))
        });
    let compare = earlier.map(|((e, l), kind)| (e.month.clone(), kind, l, shares(l).1));
    let rows = list
        .iter()
        .enumerate()
        .map(|(i, (term, clicks))| {
            let prev = compare
                .as_ref()
                .and_then(|(_, _, l, s)| l.iter().position(|(t, _)| t == term).map(|j| s[j]));
            PrefTermRow {
                rank: u32::try_from(i + 1).unwrap_or(u32::MAX),
                term: term.clone(),
                clicks: *clicks,
                share_pct: share[i],
                compare_share_pct: prev,
                change_pt: prev.map(|p| share[i] - p),
                out_of_range: compare.is_some() && prev.is_none(),
            }
        })
        .collect();
    PrefTermsPart::Ok {
        prefecture: prefecture.to_owned(),
        month: latest.month.clone(),
        compare_month: compare.as_ref().map(|c| c.0.clone()),
        compare_kind: compare.as_ref().map(|c| c.1.to_owned()),
        listed_clicks,
        rows,
    }
}

/// 重なる月 (両方に値がある月) を取り、最初の重なる月を 100 とした指数を返す。
fn overlay_part(
    region: &str,
    google: Option<(Vec<String>, String, Vec<Option<f64>>)>,
    indeed: &[(String, Option<f64>)],
) -> OverlayPart {
    let Some((g_months, g_label, g_values)) = google else {
        return OverlayPart::Unavailable {
            message: "Google検索数のデータがないため、重ねて表示できません。".to_owned(),
        };
    };
    let overlap = overlap_months(indeed, &g_months, &g_values);
    if overlap.len() < 2 {
        return OverlayPart::Unavailable {
            message: "Indeedの閲覧数とGoogle検索数で重なる月が2か月に満たないため、重ねて表示できません。"
                .to_owned(),
        };
    }
    let base = &overlap[0];
    if base.1 <= 0.0 || base.2 <= 0.0 {
        return OverlayPart::Unavailable {
            message: "最初の重なる月の値が0のため、指数にできません。".to_owned(),
        };
    }
    let region_label = if region.is_empty() { "全国" } else { region };
    OverlayPart::Ok {
        base_month: base.0.clone(),
        indeed_index: overlap.iter().map(|o| o.1 / base.1 * 100.0).collect(),
        google_index: overlap.iter().map(|o| o.2 / base.2 * 100.0).collect(),
        indeed_raw: overlap.iter().map(|o| o.1).collect(),
        google_raw: overlap.iter().map(|o| o.2).collect(),
        months: overlap.iter().map(|o| o.0.clone()).collect(),
        indeed_label: format!("Indeedの求人を見た人数（{region_label}）"),
        google_label: format!("Googleの検索数 「{g_label}」（全国）"),
        caption: "Indeedは選んだ都道府県、Googleは全国。閲覧数は応募数ではありません。".to_owned(),
    }
}

/// (月, Indeed の値, Google の値)。月の昇順。
fn overlap_months(
    indeed: &[(String, Option<f64>)],
    g_months: &[String],
    g_values: &[Option<f64>],
) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = indeed
        .iter()
        .filter_map(|(m, v)| {
            let v = (*v)?;
            let i = g_months.iter().position(|x| x == m)?;
            Some((m.clone(), v, g_values.get(i).copied().flatten()?))
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

// ---------------------------------------------------------------- 旧 HTML / PDF の描画

const PALETTE: [&str; 5] = ["#007d79", "#4472c4", "#8567a5", "#b37d20", "#c0504d"];

fn pt(v: f64) -> String {
    format!("{v:+.1}")
}

fn pct1(v: Option<f64>) -> String {
    v.map(|v| format!("{v:.1}%")).unwrap_or_else(|| "—".into())
}

fn int(v: Option<f64>) -> String {
    v.map(|v| format_number(v as i64))
        .unwrap_or_else(|| "—".into())
}

/// 複数の折れ線を 1 枚の SVG にする。凡例は SVG の中に入れる。欠測の月は線を切る。
pub(super) fn multi_line_chart(
    html: &mut String,
    label: &str,
    months: &[String],
    lines: &[(String, Vec<Option<f64>>)],
    unit: &str,
) {
    let peak = lines
        .iter()
        .flat_map(|(_, v)| v.iter().flatten())
        .copied()
        .filter(|v| v.is_finite() && *v >= 0.0)
        .fold(None, |a: Option<f64>, v| Some(a.map_or(v, |x| x.max(v))));
    let Some(peak) = peak else { return };
    let step = (peak / 4.0).ceil().max(1.0);
    let ymax = step * 4.0;
    let legend_h = 22.0 * lines.len() as f64;
    let height = 280.0 + legend_h;
    html.push_str(&format!(
        "<figure class=\"trend-card\"><figcaption>{}</figcaption><div class=\"trend-plot\"><svg viewBox=\"0 0 900 {height}\" role=\"img\" aria-label=\"{}\"><title>{}</title>",
        escape_html(label), escape_html(label), escape_html(label)
    ));
    for i in 0..=4 {
        let y = 24.0 + 212.0 * (1.0 - f64::from(i) / 4.0);
        html.push_str(&format!(
            "<path d=\"M85 {y} H876\" stroke=\"#e1e8ec\"/><text x=\"75\" y=\"{}\" font-size=\"14\" text-anchor=\"end\">{}</text>",
            y + 5.0,
            format_number((step * f64::from(i)) as i64)
        ));
    }
    let last = months.len().saturating_sub(1).max(1) as f64;
    for (k, (name, values)) in lines.iter().enumerate() {
        let color = PALETTE[k % PALETTE.len()];
        let mut run: Vec<String> = Vec::new();
        let flush = |run: &mut Vec<String>, html: &mut String| {
            if run.len() > 1 {
                html.push_str(&format!(
                    "<polyline points=\"{}\" fill=\"none\" stroke=\"{color}\" stroke-width=\"3\"/>",
                    run.join(" ")
                ));
            }
            run.clear();
        };
        for (i, v) in values.iter().enumerate() {
            let x = 85.0 + 791.0 * i as f64 / last;
            match v.filter(|v| v.is_finite() && *v >= 0.0) {
                Some(v) => run.push(format!("{x:.3},{:.3}", 24.0 + 212.0 * (1.0 - v / ymax))),
                None => flush(&mut run, html),
            }
        }
        flush(&mut run, html);
        let ly = 292.0 + 22.0 * k as f64;
        html.push_str(&format!(
            "<rect x=\"85\" y=\"{}\" width=\"22\" height=\"4\" fill=\"{color}\"/><text x=\"115\" y=\"{}\" font-size=\"14\">{}</text>",
            ly - 6.0,
            ly,
            escape_html(name)
        ));
    }
    // 最後の月を必ず表示し、そこから等間隔にさかのぼる (最後の 2 つが重ならないように)。
    let every = months.len().div_ceil(8).max(1);
    let last_i = months.len().saturating_sub(1);
    for (i, m) in months.iter().enumerate() {
        if (last_i - i).is_multiple_of(every) {
            let anchor = if i == last_i { "end" } else { "middle" };
            html.push_str(&format!(
                "<text x=\"{:.3}\" y=\"268\" text-anchor=\"{anchor}\" font-size=\"14\">{}</text>",
                85.0 + 791.0 * i as f64 / last,
                escape_html(m)
            ));
        }
    }
    html.push_str(&format!(
        "</svg></div><p class=\"note\">単位：{}</p></figure>",
        escape_html(unit)
    ));
}

pub(super) fn render_html(html: &mut String, ka: &KeywordAnalysis) {
    // PDF では Indeed の 1 ページに収まらないので、この区画だけ別のページにする (competitor_pdf.rs)。
    html.push_str("<section class=\"page-navy\" id=\"competitor-keyword-analysis\" data-pdf-page=\"indeed-keywords\"><h3 class=\"block-title\">キーワードの推移・分析</h3>");
    // A
    html.push_str("<div class=\"kw-part\"><h4>Google検索数の推移</h4>");
    match &ka.search_trend {
        SearchTrendPart::Unavailable { message } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(message)))
        }
        SearchTrendPart::Ok {
            scope,
            months,
            series,
        } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(scope)));
            let lines: Vec<_> = series
                .iter()
                .map(|s| (s.label.clone(), s.values.clone()))
                .collect();
            multi_line_chart(html, "Google検索数の推移", months, &lines, "回/月");
            html.push_str("<table class=\"table-navy\"><thead><tr><th>検索語</th><th>直近の月</th><th>検索数</th><th>前年比</th><th>ピーク月</th><th>競合の強さ</th></tr></thead><tbody>");
            for s in series {
                html.push_str(&format!(
                    "<tr><td>{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td>{}</td><td>{}</td></tr>",
                    escape_html(&s.label),
                    escape_html(&s.latest_month),
                    int(s.latest),
                    s.yoy_pct.map(|v| format!("{v:+.1}%")).unwrap_or_else(|| "—".into()),
                    s.peak_month.map(|m| format!("{m}月")).unwrap_or_else(|| "—".into()),
                    competition_ja(&s.competition)
                ));
            }
            html.push_str("</tbody></table>");
        }
    }
    html.push_str("</div>");
    // B
    html.push_str("<div class=\"kw-part\"><h4>Indeedの検索語の伸びと落ち</h4>");
    match &ka.term_shifts {
        TermShiftsPart::Unavailable { message } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(message)))
        }
        TermShiftsPart::Ok {
            scope,
            before_period,
            after_period,
            rising,
            falling,
            months,
            lines,
            thin_note,
        } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(scope)));
            if !before_period.is_empty() {
                html.push_str(&format!(
                    "<p class=\"note\">{}と{}の割合を比べています。</p>",
                    escape_html(before_period),
                    escape_html(after_period)
                ));
            }
            for (title, rows) in [("伸びた検索語", rising), ("落ちた検索語", falling)] {
                html.push_str(&format!("<table class=\"table-navy\"><thead><tr><th>{title}</th><th>前の割合</th><th>後の割合</th><th>変化(ポイント)</th></tr></thead><tbody>"));
                if rows.is_empty() {
                    html.push_str("<tr><td colspan=\"4\">該当する検索語がありません。</td></tr>");
                }
                for r in rows {
                    html.push_str(&format!(
                        "<tr><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
                        escape_html(&r.term), pct1(r.before_pct), pct1(r.after_pct), pt(r.diff_pt)
                    ));
                }
                html.push_str("</tbody></table>");
            }
            let l: Vec<_> = lines
                .iter()
                .map(|t| (t.term.clone(), t.values.clone()))
                .collect();
            multi_line_chart(html, "検索語の割合の推移", months, &l, "%");
            html.push_str("<p class=\"note\">検索語はIndeedが返す上位の語だけです。圏外に落ちた語は、減ったのか見えなくなったのか区別できません。</p>");
            if let Some(n) = thin_note {
                html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(n)));
            }
        }
    }
    html.push_str("</div>");
    // C
    html.push_str("<div class=\"kw-part\"><h4>選んだ都道府県の検索語</h4>");
    match &ka.pref_terms {
        PrefTermsPart::Unavailable { message } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(message)))
        }
        PrefTermsPart::Ok {
            prefecture,
            month,
            compare_month,
            compare_kind,
            rows,
            ..
        } => {
            html.push_str(&format!(
                "<p class=\"note\">{}・{}の上位の検索語。割合は一覧に載った語の合計に占める割合です。</p>",
                escape_html(prefecture), escape_html(month)
            ));
            if let (Some(cm), Some(ck)) = (compare_month, compare_kind) {
                html.push_str(&format!(
                    "<p class=\"note\">比べる月：{}（{}）</p>",
                    escape_html(cm),
                    escape_html(ck)
                ));
            }
            html.push_str("<table class=\"table-navy\"><thead><tr><th>順位</th><th>検索語</th><th>クリック数</th><th>割合</th><th>比べる月の割合</th><th>変化(ポイント)</th></tr></thead><tbody>");
            for r in rows {
                let (cmp, chg) = if r.out_of_range {
                    ("圏外".to_owned(), "圏外".to_owned())
                } else {
                    (
                        pct1(r.compare_share_pct),
                        r.change_pt.map(pt).unwrap_or_else(|| "—".into()),
                    )
                };
                html.push_str(&format!(
                    "<tr><td class=\"num\">{}</td><td>{}</td><td class=\"num\">{}</td><td class=\"num\">{:.1}%</td><td class=\"num\">{}</td><td class=\"num\">{}</td></tr>",
                    r.rank, escape_html(&r.term), format_number(r.clicks), r.share_pct, cmp, chg
                ));
            }
            html.push_str("</tbody></table>");
        }
    }
    html.push_str("</div>");
    // D
    html.push_str("<div class=\"kw-part\"><h4>Indeed閲覧数とGoogle検索数の重ね表示</h4>");
    match &ka.overlay {
        OverlayPart::Unavailable { message } => {
            html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(message)))
        }
        OverlayPart::Ok {
            base_month,
            months,
            indeed_label,
            google_label,
            indeed_index,
            google_index,
            caption,
            ..
        } => {
            html.push_str(&format!(
                "<p class=\"note\">{}</p><p class=\"note\">{}を100とした指数です。</p>",
                escape_html(caption),
                escape_html(base_month)
            ));
            let lines = vec![
                (
                    indeed_label.clone(),
                    indeed_index.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
                ),
                (
                    google_label.clone(),
                    google_index.iter().map(|v| Some(*v)).collect::<Vec<_>>(),
                ),
            ];
            multi_line_chart(html, "閲覧数と検索数の指数", months, &lines, "指数");
        }
    }
    html.push_str("</div>");
    html.push_str("<div class=\"kw-caveats\">");
    for c in &ka.caveats {
        html.push_str(&format!("<p class=\"note\">{}</p>", escape_html(c)));
    }
    html.push_str("</div></section>");
}

fn competition_ja(v: &str) -> &'static str {
    match v {
        "HIGH" => "強い",
        "MEDIUM" => "中程度",
        "LOW" => "弱い",
        _ => "—",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn shift(term: &str, before: f64, after: f64) -> ShiftIn {
        ShiftIn {
            term: term.into(),
            before: Some(before),
            after: Some(after),
            diff: Some(after - before),
        }
    }

    #[test]
    fn rising_and_falling_are_ranked_by_point_change() {
        let rows = vec![
            shift("a", 10.0, 12.5),
            shift("b", 5.0, 15.0),
            shift("c", 40.0, 30.0),
            shift("d", 20.0, 19.0),
            shift("e", 3.0, 3.0),
            ShiftIn {
                term: "f".into(),
                before: None,
                after: None,
                diff: None,
            },
        ];
        let (up, down) = rank_shifts(&rows, 5);
        assert_eq!(
            up.iter().map(|r| r.term.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );
        assert_eq!(up[0].diff_pt, 10.0);
        assert_eq!(up[1].diff_pt, 2.5);
        assert_eq!(
            down.iter().map(|r| r.term.as_str()).collect::<Vec<_>>(),
            ["c", "d"]
        );
        assert_eq!(down[0].diff_pt, -10.0);
        assert_eq!(rank_shifts(&rows, 1).0.len(), 1);
    }

    #[test]
    fn top_lines_use_latest_share() {
        let t = |n: &str, v: Vec<Option<f64>>| TermLine {
            term: n.into(),
            values: v,
        };
        let lines = top_lines(
            &[
                t("x", vec![Some(50.0), Some(10.0)]),
                t("y", vec![Some(1.0), Some(30.0)]),
                t("z", vec![Some(9.0), None]),
            ],
            2,
        );
        assert_eq!(
            lines.iter().map(|l| l.term.as_str()).collect::<Vec<_>>(),
            ["y", "x"]
        );
    }

    fn entry(month: &str, kw: &str) -> PrefMonthEntry {
        PrefMonthEntry {
            month: month.into(),
            available: true,
            top_keywords: kw.into(),
        }
    }

    #[test]
    fn prefecture_terms_compare_with_twelve_months_earlier_and_mark_out_of_range() {
        let entries = vec![
            entry("2025-08", "事務:60;正社員:40"),
            entry("2025-12", "事務:10"),
            entry("2026-08", "事務:34802;正社員:8048;在宅:1000"),
        ];
        let PrefTermsPart::Ok {
            month,
            compare_month,
            compare_kind,
            listed_clicks,
            rows,
            ..
        } = pref_terms("大阪府", &entries)
        else {
            panic!()
        };
        assert_eq!(month, "2026-08");
        assert_eq!(compare_month.as_deref(), Some("2025-08"));
        assert_eq!(compare_kind.as_deref(), Some("12か月前"));
        assert_eq!(listed_clicks, 43850);
        assert_eq!(rows[0].term, "事務");
        assert_eq!(rows[0].clicks, 34802);
        assert!((rows[0].share_pct - 34802.0 * 100.0 / 43850.0).abs() < 1e-9);
        assert_eq!(rows[0].compare_share_pct, Some(60.0));
        assert!((rows[0].change_pt.unwrap() - (34802.0 * 100.0 / 43850.0 - 60.0)).abs() < 1e-9);
        assert!(rows[2].out_of_range);
        assert_eq!(rows[2].change_pt, None);
        assert!(!rows[0].out_of_range);
    }

    #[test]
    fn prefecture_terms_fall_back_to_first_month() {
        let entries = vec![
            entry("2025-10", "事務:50;正社員:50"),
            entry("2026-08", "事務:75;正社員:25"),
        ];
        let PrefTermsPart::Ok {
            compare_month,
            compare_kind,
            rows,
            ..
        } = pref_terms("大阪府", &entries)
        else {
            panic!()
        };
        assert_eq!(compare_month.as_deref(), Some("2025-10"));
        assert_eq!(compare_kind.as_deref(), Some("収録の最初の月"));
        assert_eq!(rows[0].change_pt, Some(25.0));
        assert_eq!(rows[1].change_pt, Some(-25.0));
    }

    #[test]
    fn prefecture_terms_single_month_has_no_comparison() {
        let PrefTermsPart::Ok {
            compare_month,
            rows,
            ..
        } = pref_terms("大阪府", &[entry("2026-08", "事務:5")])
        else {
            panic!()
        };
        assert_eq!(compare_month, None);
        assert!(!rows[0].out_of_range);
        assert_eq!(rows[0].change_pt, None);
    }

    #[test]
    fn prefecture_terms_omitted_cases() {
        let PrefTermsPart::Unavailable { message } = pref_terms("", &[]) else {
            panic!()
        };
        assert_eq!(
            message,
            "都道府県を選ぶと、その都道府県の検索語を表示します（全国では表示しません）。"
        );
        let off = PrefMonthEntry {
            month: "2026-08".into(),
            available: false,
            top_keywords: "事務:5".into(),
        };
        let PrefTermsPart::Unavailable { message } = pref_terms("大阪府", &[off]) else {
            panic!()
        };
        assert_eq!(
            message,
            "この都道府県の検索語はIndeedから提供されていません。"
        );
    }

    #[test]
    fn month_shift_crosses_years() {
        assert_eq!(shift_month("2026-08", -12).as_deref(), Some("2025-08"));
        assert_eq!(shift_month("2026-01", -1).as_deref(), Some("2025-12"));
        assert_eq!(shift_month("bad", -1), None);
    }

    #[test]
    fn overlay_uses_overlapping_months_and_indexes_to_100() {
        // Google は 2026-07 まで、Indeed は 2026-08 まで → 重なるのは 2026-06 と 2026-07。
        let g_months: Vec<String> = ["2026-05", "2026-06", "2026-07"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let g_values = vec![Some(1000.0), Some(2000.0), Some(3000.0)];
        let indeed: Vec<(String, Option<f64>)> = vec![
            ("2026-06".into(), Some(500.0)),
            ("2026-07".into(), Some(750.0)),
            ("2026-08".into(), Some(900.0)),
        ];
        let OverlayPart::Ok {
            base_month,
            months,
            indeed_index,
            google_index,
            indeed_raw,
            google_raw,
            indeed_label,
            ..
        } = overlay_part(
            "大阪府",
            Some((g_months, "X 求人".into(), g_values)),
            &indeed,
        )
        else {
            panic!()
        };
        assert_eq!(base_month, "2026-06");
        assert_eq!(months, ["2026-06", "2026-07"]);
        assert_eq!(indeed_index, [100.0, 150.0]);
        assert_eq!(google_index, [100.0, 150.0]);
        assert_eq!(indeed_raw, [500.0, 750.0]);
        assert_eq!(google_raw, [2000.0, 3000.0]);
        assert_eq!(indeed_label, "Indeedの求人を見た人数（大阪府）");
    }

    #[test]
    fn overlay_skips_missing_months_and_reports_missing_parts() {
        let g_months: Vec<String> = ["2026-05", "2026-06", "2026-07"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        let g = Some((
            g_months.clone(),
            "X 求人".to_string(),
            vec![Some(10.0), None, Some(30.0)],
        ));
        let indeed: Vec<(String, Option<f64>)> = vec![
            ("2026-05".into(), Some(1.0)),
            ("2026-06".into(), Some(2.0)),
            ("2026-07".into(), Some(4.0)),
        ];
        let OverlayPart::Ok {
            months,
            indeed_index,
            google_index,
            ..
        } = overlay_part("", g, &indeed)
        else {
            panic!()
        };
        assert_eq!(months, ["2026-05", "2026-07"]);
        assert_eq!(indeed_index, [100.0, 400.0]);
        assert_eq!(google_index, [100.0, 300.0]);
        let OverlayPart::Unavailable { message } = overlay_part("", None, &indeed) else {
            panic!()
        };
        assert_eq!(
            message,
            "Google検索数のデータがないため、重ねて表示できません。"
        );
        let g1 = Some((g_months, "X 求人".to_string(), vec![Some(10.0), None, None]));
        let OverlayPart::Unavailable { message } = overlay_part("", g1, &indeed) else {
            panic!()
        };
        assert!(message.contains("2か月に満たない"));
    }

    fn raw_fixture() -> Value {
        json!({
          "title": "テスト職種", "region": "大阪府",
          "rows": [{"month":"2026-06","ctk":500.0},{"month":"2026-07","ctk":750.0}],
          "keyword_raw": {
            "prefecture": "大阪府",
            "term_count_latest": 12.0,
            "search": [
              {"variant":"name","months":"x","series":null},
              {"variant":"job","months":["2026-06","2026-07"],"series":[2000.0,3000.0],"latest":3000,"latest_month":"2026-07","yoy_pct":-14.4,"peak_month":1,"competition":"HIGH"}
            ],
            "shifts": [{"term":"一般事務","before":5.3,"after":19.9,"diff":14.6},{"term":"事務","before":46.1,"after":29.8,"diff":-16.3}],
            "term_monthly": {"months":["2026-03","2026-04","2026-05","2026-06","2026-07","2026-08"],
              "terms":[{"term":"事務","pct":[40.0,38.0,36.0,34.0,32.0,30.0]},{"term":"短い","pct":[1.0]}]},
            "pref_months": [{"month":"2026-08","kw_available":1,"top_keywords":"事務:34802;正社員:8048"}]
          }
        })
    }

    #[test]
    fn build_assembles_all_parts_from_raw() {
        let ka = build(&raw_fixture()).unwrap();
        let SearchTrendPart::Ok {
            scope,
            series,
            months,
            ..
        } = &ka.search_trend
        else {
            panic!()
        };
        assert_eq!(scope, "全国の検索数（都道府県別ではありません）");
        assert_eq!(months, &["2026-06", "2026-07"]);
        assert_eq!(series.len(), 1);
        assert_eq!(series[0].label, "テスト職種 求人");
        assert_eq!(series[0].latest, Some(3000.0));
        assert_eq!(series[0].peak_month, Some(1));
        let TermShiftsPart::Ok {
            rising,
            falling,
            before_period,
            after_period,
            lines,
            thin_note,
            ..
        } = &ka.term_shifts
        else {
            panic!()
        };
        assert_eq!(rising[0].term, "一般事務");
        assert_eq!(falling[0].diff_pt, -16.3);
        assert_eq!(before_period, "2026-03〜2026-05");
        assert_eq!(after_period, "2026-06〜2026-08");
        assert_eq!(lines.len(), 1);
        assert_eq!(
            thin_note.as_deref(),
            Some("この職種は最新月の検索語が12語と少なく、割合が大きく振れることがあります。")
        );
        assert!(matches!(&ka.pref_terms, PrefTermsPart::Ok { rows, .. } if rows.len() == 2));
        assert!(
            matches!(&ka.overlay, OverlayPart::Ok { months, .. } if months == &["2026-06", "2026-07"])
        );
    }

    #[test]
    fn build_returns_none_without_raw_and_degrades_per_part() {
        assert!(build(&json!({"title":"t","rows":[]})).is_none());
        let ka = build(&json!({"title":"t","region":"全国","rows":[],"keyword_raw":{"prefecture":"","search":[],"shifts":[],"term_monthly":{"months":[],"terms":[]},"pref_months":[]}})).unwrap();
        assert!(
            matches!(&ka.search_trend, SearchTrendPart::Unavailable { message } if message == "この職種のGoogle検索数のデータはありません。")
        );
        assert!(
            matches!(&ka.term_shifts, TermShiftsPart::Unavailable { message } if message == "この職種のIndeedの検索語の推移データはありません。")
        );
        assert!(matches!(&ka.pref_terms, PrefTermsPart::Unavailable { .. }));
        assert!(matches!(&ka.overlay, OverlayPart::Unavailable { .. }));
    }

    #[test]
    fn html_block_has_no_job_title_in_headings() {
        let ka = build(&raw_fixture()).unwrap();
        let mut html = String::new();
        render_html(&mut html, &ka);
        for h in [
            "キーワードの推移・分析",
            "Google検索数の推移",
            "Indeedの検索語の伸びと落ち",
            "選んだ都道府県の検索語",
            "Indeed閲覧数とGoogle検索数の重ね表示",
        ] {
            assert!(html.contains(h), "{h}");
        }
        // 見出し (h3/h4) に職種名は出ない。検索語の凡例には出る。
        for part in html.split("<h").skip(1) {
            let heading = part.split("</h").next().unwrap_or_default();
            assert!(!heading.contains("テスト職種"), "{heading}");
        }
        assert!(html.contains("テスト職種 求人"));
    }

    fn open_db() -> crate::db::local_sqlite::LocalDb {
        // 同梱の gz から実体を用意する (season.rs のテストと同じ手順)。
        crate::ensure_db_from_gz("data/indeed_insights.db");
        crate::db::local_sqlite::LocalDb::new("data/indeed_insights.db")
            .expect("Indeed 分析 DB を開けませんでした")
    }

    #[test]
    fn real_snapshot_builds_all_four_parts_for_osaka() {
        let db = open_db();
        let snap = crate::indeed::data::load(&db).expect("スナップショットを読めませんでした");
        let series = snap
            .by_pref
            .iter()
            .find(|r| r.title == "事務" && r.prefecture == "大阪府")
            .expect("事務×大阪府が無い");
        let rows: Vec<Value> = snap
            .meta
            .months
            .iter()
            .enumerate()
            .map(|(i, m)| json!({"month": m, "ctk": series.series.ctk.get(i).copied().flatten()}))
            .collect();
        let data = json!({
            "status": "ok", "title": "事務", "region": "大阪府", "rows": rows,
            "keyword_raw": crate::indeed::compkw::raw_context(&db, "事務", "大阪府"),
        });
        let ka = build(&data).unwrap();
        let SearchTrendPart::Ok { months, series, .. } = &ka.search_trend else {
            panic!()
        };
        assert_eq!(months.len(), 48);
        assert_eq!(months.first().map(String::as_str), Some("2022-08"));
        assert_eq!(months.last().map(String::as_str), Some("2026-07"));
        assert_eq!(
            series.iter().map(|s| s.label.as_str()).collect::<Vec<_>>(),
            ["事務 求人", "事務"]
        );
        assert_eq!(series[0].latest, Some(8100.0));
        assert_eq!(series[1].latest, Some(12100.0));
        assert_eq!(series[0].competition, "HIGH");
        let TermShiftsPart::Ok {
            rising,
            falling,
            lines,
            months,
            ..
        } = &ka.term_shifts
        else {
            panic!()
        };
        assert_eq!(months.len(), 14);
        assert!(rising.iter().all(|r| r.diff_pt > 0.0));
        assert!(falling.iter().all(|r| r.diff_pt < 0.0));
        assert!(rising.windows(2).all(|w| w[0].diff_pt >= w[1].diff_pt));
        assert!(!lines.is_empty() && lines.len() <= 5);
        let PrefTermsPart::Ok {
            month,
            compare_month,
            rows,
            ..
        } = &ka.pref_terms
        else {
            panic!()
        };
        assert_eq!(month, "2026-08");
        assert_eq!(compare_month.as_deref(), Some("2025-08"));
        assert_eq!(rows[0].term, "事務");
        assert!(rows.len() <= 10);
        // Google は 2026-07 まで、Indeed は 2025-07〜2026-08 → 重なるのは 2025-07〜2026-07 の 13 か月。
        let OverlayPart::Ok {
            months,
            indeed_index,
            google_index,
            ..
        } = &ka.overlay
        else {
            panic!()
        };
        assert_eq!(months.len(), 13);
        assert_eq!(indeed_index[0], 100.0);
        assert_eq!(google_index[0], 100.0);
    }

    #[test]
    fn national_choice_omits_prefecture_terms() {
        let db = open_db();
        let raw = crate::indeed::compkw::raw_context(&db, "事務", "");
        assert!(raw.get("pref_months").is_none());
        let ka =
            build(&json!({"title":"事務","region":"全国","rows":[],"keyword_raw":raw})).unwrap();
        assert!(matches!(ka.pref_terms, PrefTermsPart::Unavailable { .. }));
    }
}
