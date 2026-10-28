//! 競合調査の「暦月の型(全国)」。主語の検索語 1 本の全国の値だけで、暦月ごとの高さを出す。
//!
//! # 全国の値だけを使う理由
//! Keyword Planner の検索数は丸められる (隣り合う値が約 22〜25% 違うきざみ)。月 1,000 回未満の職種や
//! 都道府県単位の値 (全国の 1/10〜1/50) では、1 段のずれが季節の振幅と同じ大きさになる。
//!
//! # 「言える」かの判定 (3 つすべて)
//! (a) 全国の月平均 1,000 回以上、(b) 完全な年が 3 年以上あり、各年の山の月が ±1 か月 (12 月と 1 月は隣) に収まる、
//! (c) 季節の振幅 (暦月の指数の平均の最大 − 最小) が、年ごとのばらつき (暦月ごとの年間の最大 − 最小の中央値) より大きい。
//! 満たさないときは「月ごとの傾向は出せません」と事実だけを書き、市場全体の型を代わりに主として見せる。
use serde_json::Value;

use super::competitor_model::{
    GoogleSeason, IndeedPoint, IndeedRow, IndeedSeason, IndeedSeries, MarketMonth, MarketSeason,
    SeasonMonth, SeasonNext,
};
use crate::handlers::helpers::format_number;
use crate::indeed::season::{full_year_indices, YearIndex};

/// 月平均がこれ以上の検索語だけ、時期の型を言う。
pub(crate) const MIN_MONTHLY_AVG: f64 = 1000.0;
/// 完全な年がこれ以上あるときだけ、時期の型を言う。
pub(crate) const MIN_FULL_YEARS: usize = 3;

pub(crate) struct SeasonInput<'a> {
    pub keyword: &'a str,
    /// 全国の月次 (月ラベル "YYYY-MM"、検索数)。古い順。
    pub months: &'a [(String, Option<f64>)],
    /// 今の月 (JST、1〜12)。
    pub current_month: u32,
    /// 市場全体の暦月の指数 (年平均 100)。職種の数と一緒に。
    pub market: Option<([f64; 12], usize)>,
}

/// 円環 (12 月と 1 月は隣) での月の距離。
fn circ_dist(a: u32, b: u32) -> u32 {
    let d = a.abs_diff(b);
    d.min(12 - d)
}

/// 各年の山の月が、ある月を中心に ±1 か月に収まるか。
fn peaks_agree(peaks: &[u32]) -> bool {
    (1..=12).any(|c| peaks.iter().all(|p| circ_dist(*p, c) <= 1))
}

fn median(mut v: Vec<f64>) -> Option<f64> {
    if v.is_empty() {
        return None;
    }
    v.sort_by(f64::total_cmp);
    let n = v.len();
    Some(if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    })
}

/// 同値は早い月を採る。
fn argmax(v: &[f64]) -> Option<u32> {
    let mut best: Option<(usize, f64)> = None;
    for (i, x) in v.iter().enumerate() {
        if best.is_none_or(|(_, b)| *x > b) {
            best = Some((i, *x));
        }
    }
    best.map(|(i, _)| i as u32 + 1)
}

fn argmin(v: &[f64]) -> Option<u32> {
    let mut best: Option<(usize, f64)> = None;
    for (i, x) in v.iter().enumerate() {
        if best.is_none_or(|(_, b)| *x < b) {
            best = Some((i, *x));
        }
    }
    best.map(|(i, _)| i as u32 + 1)
}

fn plus(v: f64) -> String {
    format!("+{}%", v.round() as i64)
}

fn minus(v: f64) -> String {
    format!("−{}%", v.round() as i64)
}

pub(crate) fn build(input: &SeasonInput) -> Option<GoogleSeason> {
    if input.keyword.trim().is_empty() || input.months.is_empty() {
        return None;
    }
    let labels: Vec<&str> = input.months.iter().map(|(m, _)| m.as_str()).collect();
    let values: Vec<Option<f64>> = input.months.iter().map(|(_, v)| *v).collect();
    let years: Vec<YearIndex> = full_year_indices(&labels, &values);
    let observed: Vec<f64> = values.iter().flatten().copied().collect();
    let monthly_avg =
        (!observed.is_empty()).then(|| observed.iter().sum::<f64>() / observed.len() as f64);

    let months: Vec<SeasonMonth> = if years.is_empty() {
        Vec::new()
    } else {
        (0..12)
            .map(|m| {
                let xs: Vec<f64> = years.iter().map(|y| y.index[m]).collect();
                SeasonMonth {
                    month: m as u32 + 1,
                    mean: xs.iter().sum::<f64>() / xs.len() as f64,
                    min: xs.iter().copied().fold(f64::INFINITY, f64::min),
                    max: xs.iter().copied().fold(f64::NEG_INFINITY, f64::max),
                }
            })
            .collect()
    };

    let means: Vec<f64> = months.iter().map(|m| m.mean).collect();
    let peak_month = argmax(&means);
    let low_month = argmin(&means);

    let enough_volume = monthly_avg.is_some_and(|a| a >= MIN_MONTHLY_AVG);
    let enough_years = years.len() >= MIN_FULL_YEARS;
    let year_peaks: Vec<u32> = years.iter().filter_map(|y| argmax(&y.index)).collect();
    let peaks_ok = enough_years && peaks_agree(&year_peaks);
    let amplitude = match (peak_month, low_month) {
        (Some(p), Some(l)) => Some(means[p as usize - 1] - means[l as usize - 1]),
        _ => None,
    };
    let spread = median(months.iter().map(|m| m.max - m.min).collect());
    let amplitude_ok = matches!((amplitude, spread), (Some(a), Some(s)) if a > s);
    let reliable = enough_volume && enough_years && peaks_ok && amplitude_ok;

    let avg_text = monthly_avg
        .map(|a| format_number(a.round() as i64))
        .unwrap_or_else(|| "—".into());
    let headline = match (reliable, peak_month, low_month) {
        (true, Some(p), Some(l)) => format!(
            "例年、{p}月ごろに最も多く（年平均より {}）、{l}月ごろに最も少ない（年平均より {}）。",
            plus(means[p as usize - 1] - 100.0),
            minus(100.0 - means[l as usize - 1])
        ),
        _ => format!(
            "この職種の検索は、年による差が季節の差より大きく、月ごとの傾向は出せません。（全国の月平均 {avg_text} 回・完全な年 {} 年）",
            years.len()
        ),
    };
    let caveat = (!reliable).then(|| {
        let mut why: Vec<String> = Vec::new();
        if !enough_volume {
            why.push(format!(
                "月平均が {} 回に届かず、検索数の丸めの影響が季節の差と同じ大きさになります",
                format_number(MIN_MONTHLY_AVG as i64)
            ));
        }
        if !enough_years {
            why.push(format!("完全な年が {MIN_FULL_YEARS} 年に満たない"));
        } else if !peaks_ok {
            why.push("年ごとに山の月が違う".into());
        }
        if enough_years && !amplitude_ok {
            why.push("季節の差が年ごとのばらつきを上回らない".into());
        }
        format!("{}。", why.join("。"))
    });

    let index_of = |m: u32| -> Option<f64> { means.get(m as usize - 1).copied() };
    let cur = input.current_month.clamp(1, 12);
    let next_three: Vec<SeasonNext> = (1..=3)
        .filter_map(|k| {
            let m = (cur - 1 + k) % 12 + 1;
            Some(SeasonNext {
                month: m,
                index: index_of(m)?,
            })
        })
        .collect();

    let market = input.market.map(|(idx, n)| {
        let mm: Vec<f64> = idx.to_vec();
        let (p, l) = (argmax(&mm), argmin(&mm));
        let headline = match (p, l) {
            (Some(p), Some(l)) => format!(
                "職種全体の傾向では、{p}月ごろに多く（年平均より {}）、{l}月ごろに少ない（年平均より {}）。個々の職種では当てはまらないことがあります。",
                plus(mm[p as usize - 1] - 100.0),
                minus(100.0 - mm[l as usize - 1])
            ),
            _ => String::new(),
        };
        MarketSeason {
            months: mm
                .iter()
                .enumerate()
                .map(|(i, v)| MarketMonth {
                    month: i as u32 + 1,
                    mean: *v,
                })
                .collect(),
            n_series: n as u32,
            peak_month: p,
            low_month: l,
            headline,
        }
    });

    Some(GoogleSeason {
        keyword: input.keyword.to_owned(),
        scope: "全国".into(),
        monthly_avg,
        full_years: years.iter().map(|y| y.year).collect(),
        months,
        reliable,
        headline,
        caveat,
        peak_month,
        low_month,
        next_three,
        current_month: cur,
        current_index: index_of(cur),
        market,
        indeed: None,
    })
}

/// Indeed が山の月のずれを出すのに要る月数。
pub(crate) const MIN_INDEED_MONTHS_FOR_SHIFT: u32 = 24;

fn parse_ym(label: &str) -> Option<(i32, u32)> {
    let y: i32 = label.get(..4)?.parse().ok()?;
    let m: u32 = label.get(5..7)?.parse().ok()?;
    (label.get(4..5) == Some("-") && (1..=12).contains(&m)).then_some((y, m))
}

/// 円環での符号つきの差 (p − g)。−5〜+6。
fn signed_month_diff(p: u32, g: u32) -> i32 {
    let d = (p as i32 - g as i32).rem_euclid(12);
    if d > 6 {
        d - 12
    } else {
        d
    }
}

/// Indeed の閲覧数・求人数を指数にして暦月に振り分ける。Indeed に値が 1 つも無ければ None。
/// `google_peak` は Google の型が言えるときの山の月 (言えないときは None)。
pub(crate) fn indeed_overlay(rows: &[IndeedRow], google_peak: Option<u32>) -> Option<IndeedSeason> {
    let pick: [(&str, fn(&IndeedRow) -> Option<f64>); 2] =
        [("閲覧数", |r| r.ctk), ("求人数", |r| r.job)];
    let mut series = Vec::new();
    let mut month_set: std::collections::BTreeSet<(i32, u32)> = Default::default();
    let mut per_month: Vec<[u32; 12]> = Vec::new();
    for (label, get) in pick {
        let obs: Vec<((i32, u32), f64)> = rows
            .iter()
            .filter_map(|r| {
                let v = get(r).filter(|v| v.is_finite() && *v >= 0.0)?;
                Some((parse_ym(&r.month)?, v))
            })
            .collect();
        if obs.is_empty() {
            continue;
        }
        let mean = obs.iter().map(|(_, v)| v).sum::<f64>() / obs.len() as f64;
        if mean <= 0.0 {
            continue;
        }
        let mut counts = [0u32; 12];
        let mut points: Vec<IndeedPoint> = obs
            .iter()
            .map(|((year, month), v)| {
                counts[*month as usize - 1] += 1;
                month_set.insert((*year, *month));
                IndeedPoint {
                    month: *month,
                    year: *year,
                    index: v / mean * 100.0,
                }
            })
            .collect();
        points.sort_by_key(|p| (p.month, p.year));
        per_month.push(counts);
        series.push(IndeedSeries {
            label: label.into(),
            points,
            peak_month: None,
            shift_from_google: None,
        });
    }
    let first = *month_set.first()?;
    let last = *month_set.last()?;
    let n_months = month_set.len() as u32;
    let points_per_month: Vec<u32> = (0..12)
        .map(|m| per_month.iter().map(|c| c[m]).max().unwrap_or(0))
        .collect();
    let seen: Vec<u32> = points_per_month
        .iter()
        .copied()
        .filter(|n| *n > 0)
        .collect();
    let (lo, hi) = (
        seen.iter().copied().min().unwrap_or(0),
        seen.iter().copied().max().unwrap_or(0),
    );
    let fmt = |(y, m): (i32, u32)| format!("{y}-{m:02}");
    let points_text = if lo == hi {
        format!("{lo} 点")
    } else {
        format!("{lo}〜{hi} 点")
    };
    let span_note = format!(
        "Indeed は {}〜{} の {n_months} か月分で、同じ月のデータは {points_text}です。",
        fmt(first),
        fmt(last)
    );
    let shift_note = if n_months < MIN_INDEED_MONTHS_FOR_SHIFT {
        "Indeed が 24 か月分たまると、山の月と Google とのずれを表示します。".to_owned()
    } else {
        for s in &mut series {
            let mut sum = [0.0f64; 12];
            let mut cnt = [0u32; 12];
            for p in &s.points {
                sum[p.month as usize - 1] += p.index;
                cnt[p.month as usize - 1] += 1;
            }
            let avg: Vec<f64> = (0..12)
                .map(|i| {
                    if cnt[i] > 0 {
                        sum[i] / f64::from(cnt[i])
                    } else {
                        f64::NEG_INFINITY
                    }
                })
                .collect();
            s.peak_month = argmax(&avg);
            s.shift_from_google = match (s.peak_month, google_peak) {
                (Some(p), Some(g)) => Some(signed_month_diff(p, g)),
                _ => None,
            };
        }
        match google_peak {
            None => "Google の月ごとの傾向を出せないため、山の月のずれは出しません。".to_owned(),
            Some(g) => {
                let parts: Vec<String> = series
                    .iter()
                    .filter_map(|s| {
                        let (p, d) = (s.peak_month?, s.shift_from_google?);
                        let diff = if d == 0 {
                            "差なし".to_owned()
                        } else {
                            format!("{d:+} か月")
                        };
                        Some(format!("{} {p}月（Google との差 {diff}）", s.label))
                    })
                    .collect();
                format!("山の月は、Google {g}月、Indeed {}。", parts.join("、"))
            }
        }
    };
    Some(IndeedSeason {
        series,
        first_month: fmt(first),
        last_month: fmt(last),
        n_months,
        points_per_month,
        span_note,
        shift_note,
    })
}

/// 取得側が `google` の JSON に載せる `season_source` から作る。
/// `{ keyword, months: [{month, search_volume}], current_month, market: {months:[12 個], n_series} | null }`
pub(crate) fn from_value(src: &Value) -> Option<GoogleSeason> {
    let keyword = src["keyword"].as_str()?;
    let months: Vec<(String, Option<f64>)> = src["months"]
        .as_array()?
        .iter()
        .filter_map(|m| Some((m["month"].as_str()?.to_owned(), m["search_volume"].as_f64())))
        .collect();
    let market = (|| {
        let arr = src["market"]["months"].as_array()?;
        let v: Vec<f64> = arr.iter().filter_map(Value::as_f64).collect();
        let idx: [f64; 12] = v.try_into().ok()?;
        Some((idx, src["market"]["n_series"].as_u64()? as usize))
    })();
    build(&SeasonInput {
        keyword,
        months: &months,
        current_month: u32::try_from(src["current_month"].as_u64()?).ok()?,
        market,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn series(
        years: std::ops::Range<i32>,
        f: impl Fn(i32, u32) -> Option<f64>,
    ) -> Vec<(String, Option<f64>)> {
        let mut out = Vec::new();
        for y in years {
            for m in 1..=12u32 {
                out.push((format!("{y}-{m:02}"), f(y, m)));
            }
        }
        out
    }

    fn input(months: &[(String, Option<f64>)], cur: u32) -> SeasonInput<'_> {
        SeasonInput {
            keyword: "飲食店 求人",
            months,
            current_month: cur,
            market: None,
        }
    }

    /// 3 月が山、8 月が谷の明瞭な型。
    fn clear(base: f64) -> impl Fn(i32, u32) -> Option<f64> {
        move |_, m| {
            Some(
                base * match m {
                    3 => 1.3,
                    8 => 0.7,
                    _ => 1.0,
                },
            )
        }
    }

    #[test]
    fn index_is_relative_to_each_years_average() {
        let s = series(2022..2026, clear(2000.0));
        let r = build(&input(&s, 5)).unwrap();
        // 年平均 = 2000 (10 + 1.3 + 0.7 = 12)。3 月 = 2600 → 130、8 月 = 1400 → 70。
        assert_eq!(r.full_years, vec![2022, 2023, 2024, 2025]);
        let mar = &r.months[2];
        assert!((mar.mean - 130.0).abs() < 1e-9 && (mar.min - 130.0).abs() < 1e-9);
        assert!((r.months[7].mean - 70.0).abs() < 1e-9);
        assert!(r.reliable);
        assert_eq!(r.peak_month, Some(3));
        assert_eq!(r.low_month, Some(8));
        assert_eq!(
            r.headline,
            "例年、3月ごろに最も多く（年平均より +30%）、8月ごろに最も少ない（年平均より −30%）。"
        );
        assert!(r.caveat.is_none());
    }

    #[test]
    fn only_complete_years_are_used() {
        // 2022-08 始まりの 48 か月 = 完全な年は 2023・2024・2025 の 3 年だけ。
        let mut s: Vec<(String, Option<f64>)> = series(2022..2027, clear(2000.0))
            .into_iter()
            .skip(7)
            .take(48)
            .collect();
        assert_eq!(s[0].0, "2022-08");
        let r = build(&input(&s, 1)).unwrap();
        assert_eq!(r.full_years, vec![2023, 2024, 2025]);
        // 完全な年の中に欠けが 1 つあればその年は外れる。
        s[10].1 = None; // 2023-06
        let r = build(&input(&s, 1)).unwrap();
        assert_eq!(r.full_years, vec![2024, 2025]);
        assert!(!r.reliable);
        assert!(r.caveat.unwrap().contains("完全な年が 3 年に満たない"));
    }

    #[test]
    fn low_volume_fails_condition_a() {
        let s = series(2022..2026, clear(900.0));
        let r = build(&input(&s, 5)).unwrap();
        assert!(!r.reliable);
        assert!(r.headline.contains("月ごとの傾向は出せません"));
        assert!(r.headline.contains("全国の月平均 900 回・完全な年 4 年"));
        assert!(r.caveat.unwrap().contains("月平均が 1,000 回に届かず"));
        assert!(!r.headline.contains("ごろに最も多く"));
        assert_eq!(r.months.len(), 12);
    }

    #[test]
    fn peaks_in_different_months_fail_condition_b() {
        let peak = |y: i32| [2, 5, 8, 11][(y - 2022) as usize];
        let s = series(2022..2026, |y, m| {
            Some(if m == peak(y) { 4000.0 } else { 2000.0 })
        });
        let r = build(&input(&s, 1)).unwrap();
        assert!(!r.reliable);
        assert!(r.caveat.unwrap().contains("年ごとに山の月が違う"));
    }

    #[test]
    fn peak_agreement_is_circular() {
        assert!(peaks_agree(&[12, 1, 12, 1]));
        assert!(peaks_agree(&[2, 3, 4, 3]));
        assert!(!peaks_agree(&[2, 5, 3]));
        assert!(!peaks_agree(&[1, 3, 5]));
        // 12 月と 2 月は 2 か月離れているが、1 月を中心にすれば ±1 に収まる。
        assert!(peaks_agree(&[12, 2, 1]));
    }

    #[test]
    fn noise_larger_than_season_fails_condition_c() {
        // 3 月が毎年山 (山の月は一致) だが、他の月が年ごとに大きく揺れる。
        let s = series(2022..2026, |y, m| {
            let wobble = if (y + m as i32) % 2 == 0 { 1.6 } else { 0.4 };
            Some(2000.0 * if m == 3 { 1.7 } else { wobble })
        });
        let r = build(&input(&s, 1)).unwrap();
        assert!(!r.reliable, "{r:?}");
        let c = r.caveat.unwrap();
        assert!(c.contains("年ごとのばらつきを上回らない"), "{c}");
        assert!(!c.contains("山の月が違う"), "{c}");
    }

    #[test]
    fn three_complete_years_suffice_but_two_do_not() {
        let s = series(2023..2026, clear(2000.0));
        assert!(build(&input(&s, 1)).unwrap().reliable);
        let s = series(2024..2026, clear(2000.0));
        assert!(!build(&input(&s, 1)).unwrap().reliable);
    }

    #[test]
    fn current_month_and_next_three_wrap_around_the_year() {
        let s = series(2022..2026, clear(2000.0));
        let r = build(&input(&s, 11)).unwrap();
        assert_eq!(r.current_month, 11);
        assert_eq!(r.current_index.map(|v| v.round()), Some(100.0));
        let months: Vec<u32> = r.next_three.iter().map(|n| n.month).collect();
        assert_eq!(months, vec![12, 1, 2]);
        let r = build(&input(&s, 12)).unwrap();
        let months: Vec<u32> = r.next_three.iter().map(|n| n.month).collect();
        assert_eq!(months, vec![1, 2, 3]);
        assert!((r.next_three[2].index - 130.0).abs() < 1e-9);
    }

    #[test]
    fn market_is_passed_through_with_its_own_headline() {
        let s = series(2022..2026, clear(2000.0));
        let mut idx = [100.0; 12];
        idx[2] = 109.0;
        idx[10] = 83.0;
        let r = build(&SeasonInput {
            market: Some((idx, 84)),
            ..input(&s, 1)
        })
        .unwrap();
        let m = r.market.unwrap();
        assert_eq!(m.n_series, 84);
        assert_eq!((m.peak_month, m.low_month), (Some(3), Some(11)));
        assert!(m
            .headline
            .contains("3月ごろに多く（年平均より +9%）、11月ごろに少ない（年平均より −17%）"));
    }

    #[test]
    fn nothing_when_keyword_empty_or_no_national_data() {
        let s = series(2022..2026, clear(2000.0));
        let mut i = input(&s, 1);
        i.keyword = "  ";
        assert!(build(&i).is_none());
        assert!(build(&input(&[], 1)).is_none());
        assert!(from_value(&serde_json::json!({})).is_none());
        assert!(from_value(&serde_json::json!({"keyword":"a","current_month":3})).is_none());
        assert!(
            from_value(&serde_json::json!({"keyword":"","months":[],"current_month":3})).is_none()
        );
    }

    fn irow(month: &str, job: Option<f64>, ctk: Option<f64>) -> IndeedRow {
        IndeedRow {
            month: month.into(),
            job,
            ctk,
            emp: None,
            spp: None,
        }
    }

    fn months_from(start: (i32, u32), n: usize) -> Vec<String> {
        let (mut y, mut m) = start;
        (0..n)
            .map(|_| {
                let s = format!("{y}-{m:02}");
                m += 1;
                if m > 12 {
                    m = 1;
                    y += 1;
                }
                s
            })
            .collect()
    }

    #[test]
    fn indeed_index_is_relative_to_series_mean_and_same_month_keeps_two_points() {
        // 14 か月 2025-05〜2026-06。5・6 月だけ 2 年分ある。
        let rows: Vec<IndeedRow> = months_from((2025, 5), 14)
            .iter()
            .enumerate()
            .map(|(i, m)| irow(m, Some(100.0), Some(if i == 0 { 200.0 } else { 100.0 })))
            .collect();
        let o = indeed_overlay(&rows, None).unwrap();
        let ctk = &o.series[0];
        assert_eq!(ctk.label, "閲覧数");
        let first = ctk
            .points
            .iter()
            .find(|p| p.month == 5 && p.year == 2025)
            .unwrap();
        assert!((first.index - 200.0 / (1500.0 / 14.0) * 100.0).abs() < 1e-9);
        let may: Vec<i32> = ctk
            .points
            .iter()
            .filter(|p| p.month == 5)
            .map(|p| p.year)
            .collect();
        assert_eq!(may, vec![2025, 2026]);
        assert_eq!(o.points_per_month, vec![1, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1]);
        assert_eq!(o.n_months, 14);
        assert_eq!(
            o.span_note,
            "Indeed は 2025-05〜2026-06 の 14 か月分で、同じ月のデータは 1〜2 点です。"
        );
        assert!(o.series[1]
            .points
            .iter()
            .all(|p| (p.index - 100.0).abs() < 1e-9));
        assert_eq!(
            o.shift_note,
            "Indeed が 24 か月分たまると、山の月と Google とのずれを表示します。"
        );
        assert!(ctk.peak_month.is_none() && ctk.shift_from_google.is_none());
    }

    #[test]
    fn indeed_shift_appears_with_24_months() {
        let rows: Vec<IndeedRow> = months_from((2024, 7), 24)
            .iter()
            .map(|m| {
                let v = if m.ends_with("-05") { 300.0 } else { 100.0 };
                irow(m, Some(100.0), Some(v))
            })
            .collect();
        let o = indeed_overlay(&rows, Some(3)).unwrap();
        assert_eq!(o.n_months, 24);
        assert_eq!(o.series[0].peak_month, Some(5));
        assert_eq!(o.series[0].shift_from_google, Some(2));
        assert!(
            o.shift_note.contains("Google 3月")
                && o.shift_note.contains("閲覧数 5月（Google との差 +2 か月）"),
            "{}",
            o.shift_note
        );
        let o = indeed_overlay(&rows, None).unwrap();
        assert!(o.series[0].shift_from_google.is_none());
        assert!(o.shift_note.contains("出しません"));
    }

    #[test]
    fn signed_month_diff_wraps() {
        assert_eq!(signed_month_diff(1, 12), 1);
        assert_eq!(signed_month_diff(12, 1), -1);
        assert_eq!(signed_month_diff(9, 3), 6);
        assert_eq!(signed_month_diff(3, 3), 0);
    }

    #[test]
    fn indeed_without_values_gives_no_overlay() {
        assert!(indeed_overlay(&[], None).is_none());
        assert!(indeed_overlay(&[irow("2026-01", None, None)], None).is_none());
        let o = indeed_overlay(&[irow("2026-01", None, Some(50.0))], None).unwrap();
        assert_eq!(o.series.len(), 1);
        assert_eq!(o.series[0].label, "閲覧数");
    }

    /// 実データ (読み取りだけ)。飲食店と事務の判定と市場全体の型を出して見る。
    #[test]
    fn real_data_restaurant_vs_office_work() {
        const DB: &str = "data/indeed_insights.db";
        if !std::path::Path::new("data/indeed_insights.db.gz").exists() {
            return;
        }
        crate::ensure_db_from_gz(DB);
        let db = crate::db::local_sqlite::LocalDb::new(DB).expect("open");
        let list = crate::indeed::season::load(&db).expect("load");
        let market = crate::indeed::season::market_calendar(&list);
        let (idx, n) = market.expect("市場全体");
        println!("market n={n} idx={:?}", idx.map(|v| v.round()));
        assert!(n >= 10);
        for name in ["飲食店", "事務"] {
            let t = list.iter().find(|t| t.title == name).expect(name);
            let months: Vec<(String, Option<f64>)> = t
                .months
                .iter()
                .cloned()
                .zip(t.series.iter().copied())
                .collect();
            let r = build(&SeasonInput {
                keyword: &format!("{name} 求人"),
                months: &months,
                current_month: 10,
                market: Some((idx, n)),
            })
            .unwrap();
            println!(
                "{name}: reliable={} avg={:?} years={:?}\n  {}\n  caveat={:?}\n  means={:?}",
                r.reliable,
                r.monthly_avg.map(f64::round),
                r.full_years,
                r.headline,
                r.caveat,
                r.months.iter().map(|m| m.mean.round()).collect::<Vec<_>>()
            );
        }
    }

    #[test]
    fn from_value_reads_months_and_market() {
        let months: Vec<_> = series(2022..2026, clear(2000.0))
            .into_iter()
            .map(|(m, v)| serde_json::json!({"month": m, "search_volume": v}))
            .collect();
        let v = serde_json::json!({
            "keyword":"飲食店 求人","months":months,"current_month":6,
            "market":{"months":[100,100,109,100,109,100,100,94,100,100,93,83],"n_series":84}
        });
        let r = from_value(&v).unwrap();
        assert_eq!(r.current_month, 6);
        assert_eq!(r.market.unwrap().n_series, 84);
        assert!(r.reliable);
    }
}
