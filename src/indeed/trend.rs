//! 月ごとの系列から「傾き」と「ばらつき」を分けて測る。
//!
//! # なぜ Rust に持ってきたか
//! 社内タブ (`/tab/indeed`) と顧客レポート (`/report/indeed`) の両方が同じ数字を出す必要がある。
//! 集計を 2 か所に持つと、片方だけ直したときに言うことが食い違う。
//! ここを唯一の実装にして、両方の画面がこれを呼ぶ。
//!
//! # 移植元
//! `scripts/indeed_trend_fit.js`。答え合わせの値は `tests/fixtures/indeed_golden.json` にあり、
//! `tests/indeed_golden_test.rs` で突き合わせている。JS 側を直したら金型を作り直すこと。
//!
//! # 測り方
//! 対数をとってから直線を当てる。求人数は「毎月 +5%」のように掛け算で動くので、
//! そのまま直線を当てると大きい月に引っ張られる。
//! 傾き（1 か月あたり何 %）と、直線からの散らばり（±何 %）を別々に出し、
//! 「向きがあるのか」と「一本調子か」を分けて判断できるようにする。

/// 直線から大きく外れた月。1 点跳ねただけで傾きが立つことがあるので、名指しできるようにする。
#[derive(Debug, Clone, PartialEq)]
pub struct Outlier {
    /// 系列の何番目か
    pub index: usize,
    /// 散らばり何個分ずれているか（符号つき）
    pub ratio: f64,
}

/// 判定の確からしさ。表示にそのまま使う文字列。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Clear,
    Mild,
    None,
}

impl Level {
    pub fn as_str(self) -> &'static str {
        match self {
            Level::Clear => "はっきりした傾向",
            Level::Mild => "緩やかな傾向",
            Level::None => "傾向とは言えない",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Fit {
    /// 月ごとの上下が、1 か月あたりの動きに比べて小さいか。
    /// false のときに「毎月○% ずつ」と書いてはいけない。
    pub steady: bool,
    /// 直線から外れた月。ずれの大きい順
    pub outliers: Vec<Outlier>,
    /// 1 か月あたり何 %
    pub slope_pct: f64,
    /// 期間全体で何 %（直線に沿った変化。端の月のブレに引きずられない）
    pub total_pct: f64,
    /// 直線からの散らばり（±何 %）
    pub scatter_pct: f64,
    /// 傾きが、その誤差の何倍か
    pub ratio: f64,
    pub level: Level,
    /// 計算に使えた点の数
    pub n: usize,
}

/// 外れ値と見なす境目。散らばり何個分か。
const OUTLIER_SIGMA: f64 = 1.8;
/// 当てはまりが完璧なとき、散らばりは計算上のごみ（1e-16 程度）になる。
/// そのまま「散らばり何個分か」を測るとごみ同士の割り算になり、
/// 実装の最後の桁が違うだけで外れ月が出たり出なかったりする。
/// 表示に出ない水準の散らばりでは、外れ月を判定しない。
const SIGMA_EPS: f64 = 1e-9;
/// 一本調子と見なす境目。散らばりが 1 か月の動きの何倍までか。
const STEADY_RATIO: f64 = 2.0;
/// これより点が少ないと当てはめない
const MIN_POINTS: usize = 4;

/// 系列に直線を当てる。点が足りない・動きが無いときは `None`。
///
/// `values` の `None` と 0 以下は、対数がとれないので飛ばす（欠測扱い）。
pub fn fit_trend(values: &[Option<f64>]) -> Option<Fit> {
    let pts: Vec<(f64, f64)> = values
        .iter()
        .enumerate()
        .filter_map(|(i, v)| match v {
            Some(x) if *x > 0.0 && x.is_finite() => Some((i as f64, x.ln())),
            _ => None,
        })
        .collect();
    if pts.len() < MIN_POINTS {
        return None;
    }

    let n = pts.len();
    let nf = n as f64;
    let mx = pts.iter().map(|p| p.0).sum::<f64>() / nf;
    let my = pts.iter().map(|p| p.1).sum::<f64>() / nf;
    let mut sxx = 0.0;
    let mut sxy = 0.0;
    for (x, y) in &pts {
        sxx += (x - mx) * (x - mx);
        sxy += (x - mx) * (y - my);
    }
    if sxx == 0.0 {
        return None;
    }

    let slope = sxy / sxx;
    let intercept = my - slope * mx;

    let mut ss = 0.0;
    for (x, y) in &pts {
        let e = y - (intercept + slope * x);
        ss += e * e;
    }
    let dof = n as i64 - 2;
    let sigma = if dof > 0 {
        (ss / dof as f64).sqrt()
    } else {
        0.0
    };
    let se_slope = sigma / sxx.sqrt();
    let ratio = if se_slope != 0.0 {
        slope.abs() / se_slope
    } else if slope != 0.0 {
        f64::INFINITY
    } else {
        0.0
    };

    let span = (values.len() - 1) as f64;

    // 直線から大きく外れた月。1 点が跳ねただけで傾きが立つことがある
    let mut outliers: Vec<Outlier> = Vec::new();
    for (x, y) in &pts {
        let resid = y - (intercept + slope * x);
        if sigma > SIGMA_EPS && (resid / sigma).abs() >= OUTLIER_SIGMA {
            outliers.push(Outlier {
                index: *x as usize,
                ratio: resid / sigma,
            });
        }
    }
    outliers.sort_by(|a, b| {
        b.ratio
            .abs()
            .partial_cmp(&a.ratio.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let slope_pct = (slope.exp() - 1.0) * 100.0;
    let scatter_pct = (sigma.exp() - 1.0) * 100.0;
    let steady = slope_pct.abs() > 0.0 && scatter_pct <= slope_pct.abs() * STEADY_RATIO;

    let level = if ratio >= 3.0 {
        Level::Clear
    } else if ratio >= 1.8 {
        Level::Mild
    } else {
        Level::None
    };

    Some(Fit {
        steady,
        outliers,
        slope_pct,
        total_pct: ((slope * span).exp() - 1.0) * 100.0,
        scatter_pct,
        ratio,
        level,
        n,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geo(start: f64, rate: f64, n: usize) -> Vec<Option<f64>> {
        (0..n).map(|i| Some(start * rate.powi(i as i32))).collect()
    }

    #[test]
    /// 点が足りなければ当てはめない
    fn test_too_few_points_returns_none() {
        assert!(fit_trend(&[Some(1.0), Some(2.0), Some(3.0)]).is_none());
    }

    #[test]
    /// 欠測と 0 以下は対数がとれないので飛ばす
    fn test_skips_missing_and_nonpositive() {
        let v = vec![Some(10.0), None, Some(0.0), Some(-5.0), Some(12.0), Some(13.0), Some(14.0)];
        let f = fit_trend(&v).expect("4 点あるので当てはまる");
        assert_eq!(f.n, 4);
    }

    #[test]
    /// 動きが無ければ「毎月○%ずつ」とは書けない
    fn test_flat_series_is_not_steady() {
        // 全部同じ値。傾き 0 なので「毎月○%ずつ」とは書けない
        let f = fit_trend(&vec![Some(3.0); 13]).unwrap();
        assert_eq!(f.slope_pct, 0.0);
        assert!(!f.steady);
    }

    #[test]
    /// 一定の割合で減る系列は一本調子
    fn test_geometric_decay_is_steady() {
        let f = fit_trend(&geo(100.0, 0.95, 13)).unwrap();
        assert!((f.slope_pct - -5.0).abs() < 1e-9);
        assert!(f.scatter_pct.abs() < 1e-9);
        assert!(f.steady);
        assert_eq!(f.level, Level::Clear);
        assert!(f.outliers.is_empty());
    }

    #[test]
    /// 1 点だけ跳ねた系列は一本調子にならず、外れ月が出る
    fn test_single_spike_breaks_steady_and_flags_outlier() {
        // 和歌山県のトラックドライバーの求人数。2026-03（8 番目）だけ跳ねている
        let v: Vec<Option<f64>> = [45.0, 55.0, 62.0, 72.0, 85.0, 78.0, 80.0, 85.0, 173.0, 130.0, 68.0, 65.0, 85.0]
            .iter()
            .map(|x| Some(*x))
            .collect();
        let f = fit_trend(&v).unwrap();
        assert!(!f.steady, "散らばり {} / 毎月 {}", f.scatter_pct, f.slope_pct);
        assert!(!f.outliers.is_empty());
        assert_eq!(f.outliers[0].index, 8);
        assert!(f.outliers[0].ratio > 0.0, "跳ね上がりなので符号は正");
    }

    #[test]
    /// 上下するだけの系列は向きが定まらない
    fn test_oscillating_series_has_no_direction() {
        let v: Vec<Option<f64>> = [100.0, 130.0, 90.0, 125.0, 95.0, 120.0, 100.0, 128.0, 92.0, 122.0, 98.0, 126.0, 105.0]
            .iter()
            .map(|x| Some(*x))
            .collect();
        let f = fit_trend(&v).unwrap();
        assert_eq!(f.level, Level::None);
        assert!(!f.steady);
    }
}
