//! 給与の「幅」から、その職種が給与で差を付けられるかを判定する（2026-09-27）。
//!
//! # なぜ要るか
//! `insight_salary` には `min_salary` / `median_salary` / `max_salary` が
//! 4,761 行すべてに入っているが、画面が使っているのは中央値だけだった
//! （`detail.rs:220` で `median_salary` のみ）。
//!
//! 中央値だけだと「相場はいくらか」しか言えない。**幅があると
//! 「その職種は給与で差を付けられるのか」が言える。**
//!
//! # 実測（2026-09-27、県が 20 以上そろっている 108 職種×給与形態）
//!
//! ```text
//! 幅が広い                        幅が狭い
//!   配送ドライバー   3.74 倍          用務スタッフ    0.92 倍
//!   トラックドライバー 2.89 倍          給食調理       0.93 倍
//!   建築現場監督     2.60 倍          オフィスビル清掃  0.93 倍
//! 中央 1.39 倍
//! ```
//!
//! 幅が狭い職種では、いくら出しても相場から外れられない。
//! 幅が広い職種では、出し方次第で他社と差が付く。
//!
//! # 「幅」の測り方
//! `(max - min) / median` を県ごとに出して、その中央値を取る。
//! 県ごとの絶対額は地域差があるので、中央値で割って揃える。
//!
//! 県の平均ではなく中央値にしているのは、1 県の極端な値
//! （船舶乗務員の栃木県で 123,000〜1,012,000 円のような）に
//! 全体が引きずられないようにするため。

/// 県が何県そろっていれば判定に使うか。
///
/// 47 県のうち 20 県。これを下回る職種は、たまたま数県で広かっただけの
/// 可能性が高い。実測では 108 職種×給与形態がこの条件を満たす。
pub const MIN_PREFS: usize = 20;

/// 「幅が広い」と言える境目。実測の中央値。
///
/// 1.39 は「最小から最大までが、中央値の 1.39 倍ぶん開いている」という意味。
/// 職種の半分がこれより上、半分が下になる。
pub const WIDE_THRESHOLD: f64 = 1.39;

/// 1 つの職種×給与形態の、給与の広がり。
#[derive(Debug, Clone, PartialEq)]
pub struct WageSpread {
    /// "HOURLY" / "MONTHLY" / "DAILY" / "YEARLY" / "WEEKLY"
    pub period: String,
    /// 判定に使った県の数
    pub prefs: usize,
    /// 県ごとの中央値の、その中央値
    pub median: f64,
    /// (max - min) / median の中央値
    pub spread: f64,
}

impl WageSpread {
    /// 給与で差を付けられる職種か。
    pub fn is_wide(&self) -> bool {
        self.spread > WIDE_THRESHOLD
    }

    /// 画面に出す一文。
    ///
    /// # 断定しない
    /// 幅が広いことは「給与を上げれば採れる」を意味しない。
    /// 言えるのは「他社が出している額に開きがある」ところまで。
    pub fn reading(&self) -> String {
        let unit = period_label(&self.period);
        if self.is_wide() {
            format!(
                "この職種の{unit}は、同じ県の中でも下から上まで中央値の {:.1} 倍ぶん開いています\
                 （{} 県で見た中央値）。出す額で他社と差が付く余地があります。",
                self.spread, self.prefs
            )
        } else {
            format!(
                "この職種の{unit}は、同じ県の中での開きが中央値の {:.1} 倍ぶんしかありません\
                 （{} 県で見た中央値）。額を上げても相場から外れにくいので、\
                 条件や書き方で差を付けるほうが現実的です。",
                self.spread, self.prefs
            )
        }
    }
}

/// `salary_period` を日本語にする。
///
/// 画面に "HOURLY" と出さない（社内語・英語を表に出さない方針）。
pub fn period_label(period: &str) -> &'static str {
    match period {
        "HOURLY" => "時給",
        "DAILY" => "日給",
        "WEEKLY" => "週給",
        "MONTHLY" => "月給",
        "YEARLY" => "年収",
        _ => "給与",
    }
}

/// 県ごとの (下限, 中央, 上限) から広がりを出す。
///
/// 県が [`MIN_PREFS`] に満たないときは `None`。
/// 中央値が 0 以下の県は、割り算が壊れるので落とす。
pub fn spread_of(period: &str, rows: &[(f64, f64, f64)]) -> Option<WageSpread> {
    let mut ratios: Vec<f64> = Vec::with_capacity(rows.len());
    let mut medians: Vec<f64> = Vec::with_capacity(rows.len());
    for &(lo, md, hi) in rows {
        if md <= 0.0 || hi < lo {
            continue;
        }
        ratios.push((hi - lo) / md);
        medians.push(md);
    }
    if ratios.len() < MIN_PREFS {
        return None;
    }
    Some(WageSpread {
        period: period.to_string(),
        prefs: ratios.len(),
        median: median_of(&mut medians),
        spread: median_of(&mut ratios),
    })
}

fn median_of(v: &mut [f64]) -> f64 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = v.len();
    if n == 0 {
        return 0.0;
    }
    if n % 2 == 1 {
        v[n / 2]
    } else {
        (v[n / 2 - 1] + v[n / 2]) / 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // データを見ない検査にする。実データの中身が変わっても条件は変わらない。

    fn rows(n: usize, lo: f64, md: f64, hi: f64) -> Vec<(f64, f64, f64)> {
        (0..n).map(|_| (lo, md, hi)).collect()
    }

    #[test]
    fn 県が足りなければ判定しない() {
        assert!(spread_of("HOURLY", &rows(MIN_PREFS - 1, 1000.0, 1200.0, 2000.0)).is_none());
        assert!(spread_of("HOURLY", &rows(MIN_PREFS, 1000.0, 1200.0, 2000.0)).is_some());
    }

    #[test]
    fn 幅は中央値で割って揃える() {
        // 額が 10 倍違っても、開きの割合が同じなら同じ広がりになる
        let a = spread_of("HOURLY", &rows(25, 1000.0, 1200.0, 2200.0)).unwrap();
        let b = spread_of("MONTHLY", &rows(25, 10000.0, 12000.0, 22000.0)).unwrap();
        assert!(
            (a.spread - b.spread).abs() < 1e-9,
            "{} vs {}",
            a.spread,
            b.spread
        );
    }

    #[test]
    fn 中央値が零以下の県は落とす() {
        let mut v = rows(MIN_PREFS, 1000.0, 1200.0, 2000.0);
        v.extend(rows(5, 100.0, 0.0, 500.0)); // 割れない県
        let s = spread_of("HOURLY", &v).unwrap();
        assert_eq!(s.prefs, MIN_PREFS, "零の県を数に入れている");
    }

    #[test]
    fn 上下が逆の県は落とす() {
        let mut v = rows(MIN_PREFS, 1000.0, 1200.0, 2000.0);
        v.push((2000.0, 1200.0, 1000.0)); // 上限が下限より小さい
        let s = spread_of("HOURLY", &v).unwrap();
        assert_eq!(s.prefs, MIN_PREFS);
    }

    #[test]
    fn 一つの県の極端な値に引きずられない() {
        // 中央値で取るので、1 県だけ桁違いでも結果が動かない
        let base = rows(30, 1000.0, 1200.0, 2200.0);
        let mut v = base.clone();
        v.push((100.0, 1200.0, 999_999.0));
        let a = spread_of("HOURLY", &base).unwrap();
        let b = spread_of("HOURLY", &v).unwrap();
        assert!(
            (a.spread - b.spread).abs() < 0.05,
            "{} vs {}",
            a.spread,
            b.spread
        );
    }

    #[test]
    fn 広い狭いの境目で言い方が変わる() {
        let wide = spread_of("HOURLY", &rows(25, 1000.0, 1000.0, 3000.0)).unwrap();
        assert!(wide.is_wide());
        assert!(wide.reading().contains("差が付く余地"));

        let narrow = spread_of("HOURLY", &rows(25, 1100.0, 1200.0, 1300.0)).unwrap();
        assert!(!narrow.is_wide());
        assert!(narrow.reading().contains("条件や書き方"));
    }

    #[test]
    fn 給与形態は日本語で出す() {
        // 画面に HOURLY と出さない
        for (k, v) in [
            ("HOURLY", "時給"),
            ("MONTHLY", "月給"),
            ("DAILY", "日給"),
            ("YEARLY", "年収"),
        ] {
            assert_eq!(period_label(k), v);
        }
        let s = spread_of("HOURLY", &rows(25, 1000.0, 1200.0, 2000.0)).unwrap();
        assert!(
            !s.reading().contains("HOURLY"),
            "英語が表に出ている: {}",
            s.reading()
        );
    }

    #[test]
    fn 幅が広くても採れるとは言わない() {
        // 「給与を上げれば採れる」とは書かない。言えるのは他社の額に開きがあることまで
        let s = spread_of("HOURLY", &rows(25, 1000.0, 1000.0, 3000.0)).unwrap();
        let r = s.reading();
        for ng in ["採れ", "集ま", "応募が増え"] {
            assert!(!r.contains(ng), "言い過ぎ「{ng}」: {r}");
        }
    }
}
