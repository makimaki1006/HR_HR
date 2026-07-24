//! キーワード需要の季節性・年間ローテカレンダー・需要集中度(決定論、Web/IO なし)。
//!
//! Python 版 `scripts/job_creation_media_engine/demand_insights.py` の
//! `compute_seasonality` / `compute_recent_trend` / `build_prep_month_candidates` を忠実移植し、
//! さらに複数キーワードを横断する「年間ローテカレンダー」と「需要集中度(HHI)」を足す。
//!
//! すべて純粋関数。入力は [`crate::media_engine::google_ads::KeywordMetric`](月次内訳付き)または
//! 月別系列 `&[(String, i64)]`。SerpApi は一切呼ばない。

use serde_json::{json, Map, Value};

use crate::media_engine::google_ads::KeywordMetric;

/// ピーク判定: 中央値のこの倍率以上(Python `DEFAULT_PEAK_RATIO`)。
pub const DEFAULT_PEAK_RATIO: f64 = 1.3;
/// トレンド閾値: 直近3ヶ月 vs その前3ヶ月の変化率(±10%、Python `DEFAULT_TREND_THRESHOLD`)。
pub const DEFAULT_TREND_THRESHOLD: f64 = 0.1;
/// ローテカレンダーの既定ノイズフロア(avg_monthly がこれ未満の KW は除外)。
pub const DEFAULT_NOISE_FLOOR: i64 = 50;

/// 小数 dp 桁で丸める。Python `round()` と同じ round-half-to-even にするため、
/// Rust の `{:.*}` フォーマッタ(ties-to-even)を経由して文字列→f64 で戻す。
/// (例: round_dp(2.125, 2)=2.12。素朴な (x*m).round()/m は 2.13 になり Python と乖離する)
fn round_dp(x: f64, dp: usize) -> f64 {
    format!("{x:.dp$}").parse::<f64>().unwrap_or(x)
}

fn f64_val(x: f64) -> Value {
    serde_json::Number::from_f64(x)
        .map(Value::Number)
        .unwrap_or(Value::Null)
}

/// 2桁丸め済みの数を Python `str(float)` 互換で文字列化する(reason 用)。
/// 例: 7.0→"7.0"、1.3→"1.3"、1.33→"1.33"。
fn py_num_2dp(x: f64) -> String {
    let s = format!("{:.2}", x);
    let t = s.trim_end_matches('0').trim_end_matches('.');
    if t.contains('.') {
        t.to_string()
    } else {
        format!("{t}.0")
    }
}

/// "2025-07" → "2025-06"。年跨ぎ("2026-01" → "2025-12")も扱う(Python `_prev_month`)。
pub fn prev_month(month: &str) -> String {
    let parts: Vec<&str> = month.split('-').collect();
    if parts.len() != 2 {
        return month.to_string();
    }
    let (year, mon) = match (parts[0].parse::<i64>(), parts[1].parse::<i64>()) {
        (Ok(y), Ok(m)) => (y, m),
        _ => return month.to_string(),
    };
    let (mut y, mut m) = (year, mon - 1);
    if m == 0 {
        m = 12;
        y -= 1;
    }
    format!("{y:04}-{m:02}")
}

/// 月別系列の中央値(偶数個は中央2値の平均、Python `statistics.median`)。空なら None。
fn median(values: &[i64]) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    let mut v = values.to_vec();
    v.sort_unstable();
    let n = v.len();
    if n % 2 == 1 {
        Some(v[n / 2] as f64)
    } else {
        Some((v[n / 2 - 1] as f64 + v[n / 2] as f64) / 2.0)
    }
}

/// 季節性(ピーク月/ボトム月)。Python `compute_seasonality` 移植。
#[derive(Debug, Clone, PartialEq)]
pub struct Seasonality {
    pub median: Option<f64>,
    pub peak_months: Vec<String>,
    /// (月, 検索数)。peak_months と同じ順序。
    pub peak_values: Vec<(String, i64)>,
    pub bottom_month: Option<String>,
    pub bottom_value: Option<i64>,
}

/// 月別系列(月昇順にソートして評価)からピーク月(中央値×peak_ratio 以上)とボトム月を出す。
pub fn compute_seasonality(monthly_12m: &[(String, i64)], peak_ratio: f64) -> Seasonality {
    let mut rows: Vec<(String, i64)> = monthly_12m.to_vec();
    rows.sort_by(|a, b| a.0.cmp(&b.0)); // Python: sorted by month 文字列
    let values: Vec<i64> = rows.iter().map(|(_, v)| *v).collect();
    let med = match median(&values) {
        Some(m) => m,
        None => {
            return Seasonality {
                median: None,
                peak_months: Vec::new(),
                peak_values: Vec::new(),
                bottom_month: None,
                bottom_value: None,
            }
        }
    };
    let threshold = med * peak_ratio;
    let mut peak_months = Vec::new();
    let mut peak_values = Vec::new();
    let mut bottom_month: Option<String> = None;
    let mut bottom_value: Option<i64> = None;
    for (month, value) in &rows {
        if month.is_empty() {
            continue;
        }
        if med > 0.0 && (*value as f64) >= threshold {
            peak_months.push(month.clone());
            peak_values.push((month.clone(), *value));
        }
        // Python: bottom_value is None or value < bottom_value(先勝ち)。
        if bottom_value.is_none() || *value < bottom_value.unwrap() {
            bottom_value = Some(*value);
            bottom_month = Some(month.clone());
        }
    }
    Seasonality {
        median: Some(med),
        peak_months,
        peak_values,
        bottom_month,
        bottom_value,
    }
}

/// 直近トレンド(直近3ヶ月平均 vs その前3ヶ月平均)。Python `compute_recent_trend` 移植。
#[derive(Debug, Clone, PartialEq)]
pub struct RecentTrend {
    /// "増加"/"減少"/"横ばい"/"データ不足"。
    pub trend: String,
    pub recent3_avg: Option<f64>,
    pub prior3_avg: Option<f64>,
    pub change_ratio: Option<f64>,
    pub recent3_months: Vec<String>,
}

fn mean_i64(vals: &[i64]) -> f64 {
    let sum: i64 = vals.iter().sum();
    sum as f64 / vals.len() as f64
}

/// 6ヶ月未満は「データ不足」。直近3 vs 前3 の変化率で増/減/横ばいを判定する。
pub fn compute_recent_trend(monthly_12m: &[(String, i64)], threshold: f64) -> RecentTrend {
    let mut rows: Vec<(String, i64)> = monthly_12m.to_vec();
    rows.sort_by(|a, b| a.0.cmp(&b.0));
    let n = rows.len();
    if n < 6 {
        let recent3_months = rows.iter().rev().take(3).rev().map(|(m, _)| m.clone()).collect();
        return RecentTrend {
            trend: "データ不足".to_string(),
            recent3_avg: None,
            prior3_avg: None,
            change_ratio: None,
            recent3_months,
        };
    }
    let recent3: Vec<i64> = rows[n - 3..].iter().map(|(_, v)| *v).collect();
    let prior3: Vec<i64> = rows[n - 6..n - 3].iter().map(|(_, v)| *v).collect();
    let recent3_avg = mean_i64(&recent3);
    let prior3_avg = mean_i64(&prior3);
    let recent3_months: Vec<String> = rows[n - 3..].iter().map(|(m, _)| m.clone()).collect();

    let (trend, change_ratio) = if prior3_avg == 0.0 {
        let t = if recent3_avg == 0.0 { "横ばい" } else { "増加" };
        (t.to_string(), None)
    } else {
        let cr = (recent3_avg - prior3_avg) / prior3_avg;
        let t = if cr > threshold {
            "増加"
        } else if cr < -threshold {
            "減少"
        } else {
            "横ばい"
        };
        (t.to_string(), Some(round_dp(cr, 3)))
    };
    RecentTrend {
        trend,
        recent3_avg: Some(round_dp(recent3_avg, 1)),
        prior3_avg: Some(round_dp(prior3_avg, 1)),
        change_ratio,
        recent3_months,
    }
}

/// 「仕込み月」候補(ピーク月の1ヶ月前)。Python `build_prep_month_candidates` 移植。
fn build_prep_month_candidates(
    peak_months: &[String],
    median: Option<f64>,
    peak_values: &[(String, i64)],
    peak_ratio: f64,
) -> Vec<Value> {
    let pv: std::collections::HashMap<&str, i64> =
        peak_values.iter().map(|(m, v)| (m.as_str(), *v)).collect();
    let mut out = Vec::new();
    for peak_month in peak_months {
        let peak_value = pv.get(peak_month.as_str()).copied();
        let ratio = match (peak_value, median) {
            (Some(pval), Some(med)) if med != 0.0 => Some(round_dp(pval as f64 / med, 2)),
            _ => None,
        };
        let prep_month = prev_month(peak_month);
        let reason = match (ratio, peak_value) {
            (Some(r), Some(_)) => format!(
                "{peak_month}の検索数は中央値の{}倍(基準{}倍以上)。出稿の仕込みは1ヶ月前の{prep_month}が候補として考えられる(断定ではない)。",
                py_num_2dp(r),
                py_num_2dp(peak_ratio),
            ),
            _ => String::new(),
        };
        let mut o = Map::new();
        o.insert("peak_month".to_string(), Value::from(peak_month.clone()));
        o.insert("prep_month".to_string(), Value::from(prep_month));
        o.insert(
            "peak_value".to_string(),
            peak_value.map(Value::from).unwrap_or(Value::Null),
        );
        o.insert("median".to_string(), median.map(f64_val).unwrap_or(Value::Null));
        o.insert(
            "ratio_vs_median".to_string(),
            ratio.map(f64_val).unwrap_or(Value::Null),
        );
        o.insert("reason".to_string(), Value::from(reason));
        out.push(Value::Object(o));
    }
    out
}

/// 1 キーワードを分析し Python `analyze_keyword` 互換の JSON を返す(パリティ用)。
///
/// monthly が空 or avg_monthly が None のとき、または中央値が出せないときは None。
pub fn analyze_keyword(
    keyword: &str,
    avg_monthly: Option<i64>,
    monthly_12m: &[(String, i64)],
) -> Option<Value> {
    if monthly_12m.is_empty() || avg_monthly.is_none() {
        return None;
    }
    let seasonality = compute_seasonality(monthly_12m, DEFAULT_PEAK_RATIO);
    seasonality.median?;
    let trend = compute_recent_trend(monthly_12m, DEFAULT_TREND_THRESHOLD);
    let prep = build_prep_month_candidates(
        &seasonality.peak_months,
        seasonality.median,
        &seasonality.peak_values,
        DEFAULT_PEAK_RATIO,
    );

    let mut peak_values_obj = Map::new();
    for (m, v) in &seasonality.peak_values {
        peak_values_obj.insert(m.clone(), Value::from(*v));
    }
    let trend_detail = json!({
        "trend": trend.trend,
        "recent3_avg": trend.recent3_avg.map(f64_val).unwrap_or(Value::Null),
        "prior3_avg": trend.prior3_avg.map(f64_val).unwrap_or(Value::Null),
        "change_ratio": trend.change_ratio.map(f64_val).unwrap_or(Value::Null),
        "recent3_months": trend.recent3_months,
    });

    Some(json!({
        "keyword": keyword,
        "avg_monthly_searches": avg_monthly,
        "median_monthly_searches": seasonality.median.map(f64_val).unwrap_or(Value::Null),
        "peak_months": seasonality.peak_months,
        "peak_values": Value::Object(peak_values_obj),
        "bottom_month": seasonality.bottom_month,
        "bottom_value": seasonality.bottom_value,
        "recent_trend": trend.trend,
        "recent_trend_detail": trend_detail,
        "prep_month_candidates": prep,
    }))
}

// ---------------------------------------------------------------------------
// API 用の簡易季節性サマリ / ピーク月(argmax)
// ---------------------------------------------------------------------------

/// 検索数が最大の月(=ピーク月、argmax)。同値は月昇順で先勝ち。空なら None。
///
/// Python の閾値ベース peak_months(複数スパイク)とは別に、ローテカレンダーや
/// API サマリで「この KW の主ピーク月」を 1 つ選ぶための決定論 argmax。
pub fn peak_month_argmax(monthly_12m: &[(String, i64)]) -> Option<String> {
    let mut rows: Vec<(String, i64)> = monthly_12m.to_vec();
    rows.sort_by(|a, b| a.0.cmp(&b.0)); // 月昇順
    // 同値は早い月を採るため strict `>` で先勝ち(max_by_key は同値で後勝ちのため使わない)。
    let mut best: Option<(String, i64)> = None;
    for (m, v) in rows {
        if best.as_ref().map(|(_, bv)| v > *bv).unwrap_or(true) {
            best = Some((m, v));
        }
    }
    best.map(|(m, _)| m)
}

/// API 返却用の簡易季節性 {peak_month, bottom_month, trend, 仕込み月}。
pub fn seasonality_summary(km: &KeywordMetric) -> Value {
    let peak = peak_month_argmax(&km.monthly_12m);
    let season = compute_seasonality(&km.monthly_12m, DEFAULT_PEAK_RATIO);
    let trend = compute_recent_trend(&km.monthly_12m, DEFAULT_TREND_THRESHOLD);
    let shikomi = peak.as_deref().map(prev_month);
    json!({
        "peak_month": peak,
        "bottom_month": season.bottom_month,
        "trend": trend.trend,
        "仕込み月": shikomi,
    })
}

// ---------------------------------------------------------------------------
// 年間ローテカレンダー
// ---------------------------------------------------------------------------

/// 月(1-12)ごとの「その月がピークのキーワード」集合。
#[derive(Debug, Clone, PartialEq)]
pub struct RotationMonth {
    pub month: u32,
    pub top_keywords: Vec<String>,
}

/// 年間ローテカレンダー(12ヶ月分の推し KW)と、ノイズフロア未満で除外した KW。
#[derive(Debug, Clone, PartialEq)]
pub struct RotationCalendar {
    pub calendar: Vec<RotationMonth>,
    pub excluded: Vec<String>,
}

/// 「YYYY-MM」から月番号(1-12)を取り出す。
fn month_num_of(ym: &str) -> Option<u32> {
    ym.split('-').nth(1).and_then(|m| m.parse::<u32>().ok())
}

/// 各キーワードのピーク月(argmax)を求め、月(1-12)ごとに推し KW を集約する。
///
/// avg_monthly < noise_floor(または欠損/月次データ無し)の KW は excluded に回す。
/// 各月内は当該ピーク月の検索数が大きい順(同値は keyword 昇順)に並べる。
/// カレンダーは常に 1..=12 の全月を含む(該当なしは空)。
pub fn rotation_calendar(kws: &[KeywordMetric], noise_floor: i64) -> RotationCalendar {
    // month(1-12) -> Vec<(keyword, peak_value)>
    let mut by_month: std::collections::BTreeMap<u32, Vec<(String, i64)>> =
        std::collections::BTreeMap::new();
    let mut excluded = Vec::new();
    for km in kws {
        let avg = km.avg_monthly.unwrap_or(0);
        if avg < noise_floor || km.monthly_12m.is_empty() {
            excluded.push(km.keyword.clone());
            continue;
        }
        let peak = match peak_month_argmax(&km.monthly_12m) {
            Some(p) => p,
            None => {
                excluded.push(km.keyword.clone());
                continue;
            }
        };
        let peak_val = km
            .monthly_12m
            .iter()
            .find(|(m, _)| *m == peak)
            .map(|(_, v)| *v)
            .unwrap_or(0);
        if let Some(mn) = month_num_of(&peak) {
            by_month.entry(mn).or_default().push((km.keyword.clone(), peak_val));
        }
    }
    let calendar = (1..=12u32)
        .map(|month| {
            let mut entries = by_month.remove(&month).unwrap_or_default();
            // 検索数降順、同値は keyword 昇順。
            entries.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
            RotationMonth {
                month,
                top_keywords: entries.into_iter().map(|(k, _)| k).collect(),
            }
        })
        .collect();
    RotationCalendar { calendar, excluded }
}

/// ローテカレンダーを API 返却用 JSON 配列へ。
pub fn rotation_calendar_json(rc: &RotationCalendar) -> Value {
    let cal: Vec<Value> = rc
        .calendar
        .iter()
        .map(|rm| json!({"month": rm.month, "top_keywords": rm.top_keywords}))
        .collect();
    Value::Array(cal)
}

// ---------------------------------------------------------------------------
// 需要集中度(HHI)
// ---------------------------------------------------------------------------

/// 需要集中度(HHI / 実効キーワード数 / 最大シェア)。
#[derive(Debug, Clone, PartialEq)]
pub struct Concentration {
    pub hhi: Option<f64>,
    pub effective_keywords: Option<f64>,
    pub top_keyword_share: Option<f64>,
    pub top_keyword: Option<String>,
}

/// avg_monthly(>0)から HHI を算出する。`sensitivity::compute_demand_concentration` と
/// 同じ HHI 数式(share^2 総和・6桁丸め、実効数=1/HHI・4桁丸め、最大シェア=6桁丸め)。
/// SERP 不要でキーワード単独から測れる版。
pub fn concentration_from_metrics(kws: &[KeywordMetric]) -> Concentration {
    // 検索量 > 0 のものだけ。
    let mut eff: Vec<(String, f64)> = kws
        .iter()
        .filter_map(|k| k.avg_monthly.filter(|v| *v > 0).map(|v| (k.keyword.clone(), v as f64)))
        .collect();
    let total: f64 = eff.iter().map(|(_, v)| v).sum();
    if total <= 0.0 {
        return Concentration {
            hhi: None,
            effective_keywords: None,
            top_keyword_share: None,
            top_keyword: None,
        };
    }
    // 最大シェア(タイは keyword 昇順で決定的)。
    eff.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap().then_with(|| a.0.cmp(&b.0)));
    let hhi = round_dp(eff.iter().map(|(_, v)| (v / total).powi(2)).sum::<f64>(), 6);
    Concentration {
        hhi: Some(hhi),
        effective_keywords: Some(round_dp(1.0 / hhi, 4)),
        top_keyword_share: Some(round_dp(eff[0].1 / total, 6)),
        top_keyword: Some(eff[0].0.clone()),
    }
}

// ---------------------------------------------------------------------------
// 長期(最大 48ヶ月)データの分析 — 前年同月比 / 反復ピーク月
// ---------------------------------------------------------------------------

/// 「YYYY-MM」→ `(年, 月)`。形式不正・月が 1..=12 外なら `None`。
fn split_ym(ym: &str) -> Option<(i32, u32)> {
    let mut it = ym.split('-');
    let y = it.next()?.trim().parse::<i32>().ok()?;
    let m = it.next()?.trim().parse::<u32>().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    Some((y, m))
}

/// 月次系列を 年 → (月 → 検索数)へ束ねる。同一年月の重複は先勝ち。
fn group_by_year(
    monthly: &[(String, i64)],
) -> std::collections::BTreeMap<i32, std::collections::BTreeMap<u32, i64>> {
    let mut out: std::collections::BTreeMap<i32, std::collections::BTreeMap<u32, i64>> =
        std::collections::BTreeMap::new();
    for (ym, v) in monthly {
        if let Some((y, m)) = split_ym(ym) {
            out.entry(y).or_default().entry(m).or_insert(*v);
        }
    }
    out
}

/// 前年同月比(直近年 vs 前年)と各年の年計を返す。
///
/// 返却 JSON:
/// ```json
/// {
///   "years": [{"year":2025,"total":1200,"months_count":12}, ...],   // 年計(昇順)
///   "latest_year": 2026, "previous_year": 2025,                     // データ不足なら null
///   "monthly": [{"month":7,"latest":100,"previous":80,"change_ratio":0.25}, ...],
///   "common_months_count": 7,
///   "annual_change_ratio": 0.12   // 両年に共通する月だけを足し合わせた比較
/// }
/// ```
/// データ不足(年が 1 つしかない / 前年が無い / 前年が 0)のときは `0` ではなく `null`。
/// `annual_change_ratio` は「直近年の年計 ÷ 前年の年計」ではなく **両年に共通する月**
/// だけで比較する(直近年が期の途中でも歪まないようにするため)。
pub fn year_over_year(metric: &KeywordMetric) -> Value {
    let by_year = group_by_year(&metric.monthly_12m);
    let years: Vec<Value> = by_year
        .iter()
        .map(|(y, months)| {
            json!({
                "year": y,
                "total": months.values().sum::<i64>(),
                "months_count": months.len(),
            })
        })
        .collect();

    let latest_year = by_year.keys().next_back().copied();
    let previous_year = latest_year.filter(|y| by_year.contains_key(&(y - 1))).map(|y| y - 1);

    let (monthly, common, annual) = match (latest_year, previous_year) {
        (Some(ly), Some(py)) => {
            let cur = &by_year[&ly];
            let prev = &by_year[&py];
            let mut rows = Vec::new();
            let mut cur_sum = 0i64;
            let mut prev_sum = 0i64;
            let mut common_count = 0usize;
            for (m, v) in cur {
                let p = prev.get(m).copied();
                let ratio = match p {
                    Some(pv) if pv != 0 => Some(round_dp((*v - pv) as f64 / pv as f64, 3)),
                    _ => None, // 前年欠測 or 前年 0 → null(0 にしない)
                };
                if let Some(pv) = p {
                    cur_sum += *v;
                    prev_sum += pv;
                    common_count += 1;
                }
                rows.push(json!({
                    "month": m,
                    "latest": v,
                    "previous": p,
                    "change_ratio": ratio.map(f64_val).unwrap_or(Value::Null),
                }));
            }
            let annual = if common_count > 0 && prev_sum != 0 {
                Some(round_dp((cur_sum - prev_sum) as f64 / prev_sum as f64, 3))
            } else {
                None
            };
            (rows, common_count, annual)
        }
        // 単年しか無い(または月次ゼロ)→ 比較不能。
        _ => (Vec::new(), 0usize, None),
    };

    json!({
        "years": years,
        "latest_year": latest_year,
        "previous_year": previous_year,
        "monthly": monthly,
        "common_months_count": if previous_year.is_some() { Value::from(common) } else { Value::Null },
        "annual_change_ratio": annual.map(f64_val).unwrap_or(Value::Null),
    })
}

/// その年の「上位25%(ピーク付近)」に入る月の集合を返す。
///
/// 条件は 2 つの AND:
/// 1. 上位 k = ceil(月数 / 4)(最低 1)番目に大きい値を閾値とし、それ以上であること
///    (同値は同順に扱うため 25% をわずかに超えることがある = 決定論)。
/// 2. その年の**中央値より厳密に大きい**こと。
///
/// 2 を足しているのは、同値が多い年(例: 1 月だけ突出し他 11 ヶ月が全部同じ値)で
/// 閾値が並の値に落ち、全月が「ピーク」に化けるのを防ぐため。完全に平坦な年は
/// ピーク無し(空集合)になる。その年の最大値が 0(全ゼロ)も空集合。
fn top_quartile_months(months: &std::collections::BTreeMap<u32, i64>) -> Vec<u32> {
    if months.is_empty() {
        return Vec::new();
    }
    let mut vals: Vec<i64> = months.values().copied().collect();
    vals.sort_unstable_by(|a, b| b.cmp(a)); // 降順
    if vals[0] <= 0 {
        return Vec::new();
    }
    let k = ((vals.len() as f64) / 4.0).ceil().max(1.0) as usize;
    let cutoff = vals[k - 1];
    let med = match median(&vals) {
        Some(m) => m,
        None => return Vec::new(),
    };
    months
        .iter()
        .filter(|(_, v)| **v >= cutoff && (**v as f64) > med)
        .map(|(m, _)| *m)
        .collect()
}

/// 複数年で「同じ月がその年の上位25%に入る」回数を数える。
///
/// 返却は `[{"month":3,"years_hit":3,"total_years":4}, ...]`(years_hit 降順 → 月昇順)。
/// - `total_years` は月次データを持つ年の数。
/// - **2 年以上ヒットした月だけ**を返す(=「繰り返し」の定義)。単年しか無ければ空配列。
pub fn recurring_peak_months(metric: &KeywordMetric) -> Value {
    let by_year = group_by_year(&metric.monthly_12m);
    let total_years = by_year.len();
    if total_years < 2 {
        return Value::Array(Vec::new());
    }
    let mut hits: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
    for months in by_year.values() {
        for m in top_quartile_months(months) {
            *hits.entry(m).or_insert(0) += 1;
        }
    }
    let mut rows: Vec<(u32, usize)> = hits.into_iter().filter(|(_, c)| *c >= 2).collect();
    // years_hit 降順 → 月昇順(決定論)。
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    Value::Array(
        rows.into_iter()
            .map(|(month, years_hit)| {
                json!({"month": month, "years_hit": years_hit, "total_years": total_years})
            })
            .collect(),
    )
}

/// 集中度を API 返却用 JSON へ。
pub fn concentration_json(c: &Concentration) -> Value {
    json!({
        "hhi": c.hhi.map(f64_val).unwrap_or(Value::Null),
        "effective_keywords": c.effective_keywords.map(f64_val).unwrap_or(Value::Null),
        "top_keyword_share": c.top_keyword_share.map(f64_val).unwrap_or(Value::Null),
        "top_keyword": c.top_keyword.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn km(keyword: &str, avg: Option<i64>, monthly: &[(&str, i64)]) -> KeywordMetric {
        KeywordMetric {
            keyword: keyword.to_string(),
            avg_monthly: avg,
            monthly_12m: monthly.iter().map(|(m, v)| (m.to_string(), *v)).collect(),
            competition: String::new(),
            competition_index: None,
            bid_low_yen: None,
            bid_high_yen: None,
            concepts: Vec::new(),
            is_brand: false,
        }
    }

    #[test]
    fn prev_month_wraps_year() {
        assert_eq!(prev_month("2025-07"), "2025-06");
        assert_eq!(prev_month("2026-01"), "2025-12");
    }

    #[test]
    fn median_even_and_odd() {
        assert_eq!(median(&[10, 20]), Some(15.0));
        assert_eq!(median(&[10, 20, 30]), Some(20.0));
        assert_eq!(median(&[]), None);
    }

    #[test]
    fn seasonality_flags_peaks_above_threshold() {
        // median=10, threshold=13。70/20/20/20 がピーク、他は 10。
        let monthly = [
            ("2025-07", 70), ("2025-08", 10), ("2025-09", 10), ("2025-10", 20),
            ("2025-11", 10), ("2025-12", 20), ("2026-01", 10), ("2026-02", 10),
            ("2026-03", 10), ("2026-04", 10), ("2026-05", 10), ("2026-06", 20),
        ].map(|(m, v)| (m.to_string(), v));
        let s = compute_seasonality(&monthly, DEFAULT_PEAK_RATIO);
        assert_eq!(s.median, Some(10.0));
        assert_eq!(s.peak_months, vec!["2025-07", "2025-10", "2025-12", "2026-06"]);
        assert_eq!(s.bottom_value, Some(10));
        assert_eq!(s.bottom_month.as_deref(), Some("2025-08")); // 先勝ちの最小
    }

    #[test]
    fn recent_trend_increase_and_data_short() {
        // 前3=[10,10,10]平均10, 直近3=[10,10,20]平均13.33 → +33% → 増加
        let monthly = [
            ("2025-07", 10), ("2025-08", 10), ("2025-09", 10),
            ("2025-10", 10), ("2025-11", 10), ("2025-12", 20),
        ].map(|(m, v)| (m.to_string(), v));
        let t = compute_recent_trend(&monthly, DEFAULT_TREND_THRESHOLD);
        assert_eq!(t.trend, "増加");
        assert_eq!(t.prior3_avg, Some(10.0));
        assert_eq!(t.recent3_avg, Some(13.3));
        assert_eq!(t.change_ratio, Some(0.333));
        // 6ヶ月未満はデータ不足。
        let short = [("2025-07".to_string(), 10), ("2025-08".to_string(), 20)];
        assert_eq!(compute_recent_trend(&short, DEFAULT_TREND_THRESHOLD).trend, "データ不足");
    }

    #[test]
    fn peak_month_argmax_ties_earliest() {
        let monthly = [("2025-09", 720), ("2025-07", 720), ("2025-08", 100)]
            .map(|(m, v)| (m.to_string(), v));
        // 720 が 2 つ → 早い月 2025-07。
        assert_eq!(peak_month_argmax(&monthly).as_deref(), Some("2025-07"));
    }

    #[test]
    fn rotation_calendar_groups_and_excludes_noise() {
        let kws = vec![
            km("A 求人", Some(500), &[("2025-03", 900), ("2025-04", 100)]), // peak 3月
            km("B 求人", Some(300), &[("2025-03", 200), ("2025-07", 800)]), // peak 7月
            km("C 求人", Some(10), &[("2025-03", 5), ("2025-04", 15)]),     // ノイズ除外
            km("D 求人", None, &[("2025-03", 5)]),                          // avg欠損→除外
        ];
        let rc = rotation_calendar(&kws, DEFAULT_NOISE_FLOOR);
        assert_eq!(rc.calendar.len(), 12);
        let march = rc.calendar.iter().find(|m| m.month == 3).unwrap();
        assert_eq!(march.top_keywords, vec!["A 求人"]);
        let july = rc.calendar.iter().find(|m| m.month == 7).unwrap();
        assert_eq!(july.top_keywords, vec!["B 求人"]);
        assert_eq!(rc.excluded, vec!["C 求人", "D 求人"]);
    }

    #[test]
    fn concentration_hhi_from_avg_monthly() {
        // 均等 4 KW(各 100)→ HHI=4*(0.25^2)=0.25, 実効=4, 最大シェア=0.25。
        let kws = vec![
            km("a", Some(100), &[]),
            km("b", Some(100), &[]),
            km("c", Some(100), &[]),
            km("d", Some(100), &[]),
        ];
        let c = concentration_from_metrics(&kws);
        assert_eq!(c.hhi, Some(0.25));
        assert_eq!(c.effective_keywords, Some(4.0));
        assert_eq!(c.top_keyword_share, Some(0.25));
        // 偏り: 900 と 100 → HHI=0.81+0.01=0.82, top_share=0.9。
        let skew = vec![km("big", Some(900), &[]), km("small", Some(100), &[])];
        let cs = concentration_from_metrics(&skew);
        assert_eq!(cs.hhi, Some(0.82));
        assert_eq!(cs.top_keyword_share, Some(0.9));
        assert_eq!(cs.top_keyword.as_deref(), Some("big"));
        // 全ゼロ → None。
        let zero = vec![km("z", Some(0), &[])];
        assert_eq!(concentration_from_metrics(&zero).hhi, None);
    }

    // --- 長期データ分析: 前年同月比 / 反復ピーク月 ---

    /// 年 y の 1..=12 月に `vals`(12 要素)を割り当てた ("YYYY-MM", v) 列を作る。
    fn year_series(y: i32, vals: [i64; 12]) -> Vec<(String, i64)> {
        (1..=12u32)
            .map(|m| (format!("{y:04}-{m:02}"), vals[(m - 1) as usize]))
            .collect()
    }

    #[test]
    fn year_over_year_compares_same_months() {
        // 2025: 全月 100。2026: 1-6月 だけ 120/50/100/100/100/100。
        let mut monthly = year_series(2025, [100; 12]);
        monthly.extend(
            [(1u32, 120i64), (2, 50), (3, 100), (4, 100), (5, 100), (6, 100)]
                .iter()
                .map(|(m, v)| (format!("2026-{m:02}"), *v)),
        );
        let m = km("kw", Some(100), &[]);
        let m = KeywordMetric { monthly_12m: monthly, ..m };
        let v = year_over_year(&m);
        assert_eq!(v["latest_year"], 2026);
        assert_eq!(v["previous_year"], 2025);
        // 年計(年ごと)。
        let years = v["years"].as_array().unwrap();
        assert_eq!(years.len(), 2);
        assert_eq!(years[0]["year"], 2025);
        assert_eq!(years[0]["total"], 1200);
        assert_eq!(years[0]["months_count"], 12);
        assert_eq!(years[1]["year"], 2026);
        assert_eq!(years[1]["total"], 570);
        // 同月比較は直近年の月だけ(6ヶ月)。
        let mon = v["monthly"].as_array().unwrap();
        assert_eq!(mon.len(), 6);
        assert_eq!(mon[0]["month"], 1);
        assert_eq!(mon[0]["latest"], 120);
        assert_eq!(mon[0]["previous"], 100);
        assert_eq!(mon[0]["change_ratio"], 0.2);
        assert_eq!(mon[1]["change_ratio"], -0.5); // 50 vs 100
        assert_eq!(mon[2]["change_ratio"], 0.0);  // 100 vs 100(同値は 0.0、null ではない)
        // 共通6ヶ月: 570 vs 600 → -0.05。
        assert_eq!(v["common_months_count"], 6);
        assert_eq!(v["annual_change_ratio"], -0.05);
    }

    #[test]
    fn year_over_year_null_when_insufficient() {
        // 単年のみ → 比較不能(0 ではなく null)。
        let single = KeywordMetric { monthly_12m: year_series(2026, [10; 12]), ..km("kw", Some(10), &[]) };
        let v = year_over_year(&single);
        assert_eq!(v["latest_year"], 2026);
        assert!(v["previous_year"].is_null());
        assert!(v["annual_change_ratio"].is_null());
        assert!(v["common_months_count"].is_null());
        assert!(v["monthly"].as_array().unwrap().is_empty());

        // 月次ゼロ件 → すべて null / 空。
        let empty = km("kw", Some(10), &[]);
        let v2 = year_over_year(&empty);
        assert!(v2["latest_year"].is_null());
        assert!(v2["annual_change_ratio"].is_null());
        assert!(v2["years"].as_array().unwrap().is_empty());

        // 連続しない年(2023 と 2026)→ 前年が無いので比較不能。
        let mut gap = year_series(2023, [10; 12]);
        gap.extend(year_series(2026, [20; 12]));
        let v3 = year_over_year(&KeywordMetric { monthly_12m: gap, ..km("kw", Some(10), &[]) });
        assert_eq!(v3["latest_year"], 2026);
        assert!(v3["previous_year"].is_null());
        assert!(v3["annual_change_ratio"].is_null());
    }

    #[test]
    fn year_over_year_prev_zero_is_null_not_zero() {
        // 前年が 0 の月は比率を出せない → null(∞ や 0 にしない)。
        let mut monthly = year_series(2025, [0; 12]);
        monthly.extend(year_series(2026, [100; 12]));
        let v = year_over_year(&KeywordMetric { monthly_12m: monthly, ..km("kw", Some(50), &[]) });
        let mon = v["monthly"].as_array().unwrap();
        assert_eq!(mon[0]["previous"], 0);
        assert!(mon[0]["change_ratio"].is_null());
        // 前年合計 0 → 年間比較も null。
        assert!(v["annual_change_ratio"].is_null());
        assert_eq!(v["common_months_count"], 12);
    }

    #[test]
    fn recurring_peaks_count_repeated_top_quartile_months() {
        // 12ヶ月 → 上位 k=3。3年とも 3月/4月/7月 が最大3値になるよう作る。
        let peak = |a: i64, b: i64, c: i64| -> [i64; 12] {
            let mut v = [10i64; 12];
            v[2] = a; // 3月
            v[3] = b; // 4月
            v[6] = c; // 7月
            v
        };
        let mut monthly = year_series(2024, peak(900, 800, 700));
        monthly.extend(year_series(2025, peak(950, 850, 750)));
        // 2026 は 3月/4月 は高いが 7月は平凡、代わりに 12月が高い。
        let mut v2026 = [10i64; 12];
        v2026[2] = 900;
        v2026[3] = 800;
        v2026[11] = 700;
        monthly.extend(year_series(2026, v2026));
        let m = KeywordMetric { monthly_12m: monthly, ..km("kw", Some(200), &[]) };

        let rp = recurring_peak_months(&m);
        let rows = rp.as_array().unwrap();
        // 3月と4月は 3年ヒット、7月は 2年ヒット。12月は 1年のみ → 除外。
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[0]["month"], 3);
        assert_eq!(rows[0]["years_hit"], 3);
        assert_eq!(rows[0]["total_years"], 3);
        assert_eq!(rows[1]["month"], 4);
        assert_eq!(rows[1]["years_hit"], 3);
        assert_eq!(rows[2]["month"], 7);
        assert_eq!(rows[2]["years_hit"], 2);
    }

    #[test]
    fn recurring_peaks_empty_for_single_year_or_flat_zero() {
        // 単年のみ → 空。
        let single = KeywordMetric { monthly_12m: year_series(2026, [10; 12]), ..km("kw", Some(10), &[]) };
        assert!(recurring_peak_months(&single).as_array().unwrap().is_empty());

        // 全ゼロの複数年 → ピーク無しで空(0 を「ピーク」にしない)。
        let mut zeros = year_series(2025, [0; 12]);
        zeros.extend(year_series(2026, [0; 12]));
        let z = KeywordMetric { monthly_12m: zeros, ..km("kw", Some(0), &[]) };
        assert!(recurring_peak_months(&z).as_array().unwrap().is_empty());

        // 完全に平坦(全月同値)の複数年 → 中央値超えの月が無いのでピーク無し(空)。
        let mut flat = year_series(2025, [10; 12]);
        flat.extend(year_series(2026, [10; 12]));
        let f = KeywordMetric { monthly_12m: flat, ..km("kw", Some(10), &[]) };
        assert!(recurring_peak_months(&f).as_array().unwrap().is_empty());

        // 1月だけ突出し他が全部同値 → 1月のみ(閾値が並の値に落ちて全月ピーク化しない)。
        let mut spike = [100i64; 12];
        spike[0] = 900;
        let mut sp = year_series(2025, spike);
        sp.extend(year_series(2026, spike));
        let s = KeywordMetric { monthly_12m: sp, ..km("kw", Some(160), &[]) };
        let rows = recurring_peak_months(&s);
        let rows = rows.as_array().unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0]["month"], 1);
        assert_eq!(rows[0]["years_hit"], 2);
    }

    #[test]
    fn top_quartile_uses_ceil_quarter() {
        // 12ヶ月 → k=3、8ヶ月 → k=2、3ヶ月 → k=1。
        let mut m: std::collections::BTreeMap<u32, i64> = std::collections::BTreeMap::new();
        for i in 1..=12u32 {
            m.insert(i, i as i64 * 10);
        }
        // 上位3 = 10月(100)/11月(110)/12月(120)。
        assert_eq!(top_quartile_months(&m), vec![10, 11, 12]);
        let mut s: std::collections::BTreeMap<u32, i64> = std::collections::BTreeMap::new();
        s.insert(1, 5);
        s.insert(2, 9);
        s.insert(3, 7);
        assert_eq!(top_quartile_months(&s), vec![2]); // k=1
    }

    #[test]
    fn py_num_2dp_matches_python_str() {
        assert_eq!(py_num_2dp(7.0), "7.0");
        assert_eq!(py_num_2dp(1.3), "1.3");
        assert_eq!(py_num_2dp(1.33), "1.33");
        assert_eq!(py_num_2dp(2.0), "2.0");
    }
}
