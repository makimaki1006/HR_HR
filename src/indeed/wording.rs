//! 変化の言い方。社内タブと顧客レポートで同じ文になるよう、ここだけが文を作る。
//!
//! # なぜ 1 か所にまとめるか
//! 以前、一覧用と詳細用で別々に文を作っていたため、同じ行なのに一覧では
//! 「毎月およそ 1.7% ずつ」、詳細では「振れが大きく一本調子ではない」と
//! 食い違っていた（表示 1,174 行のうち 1,138 行が該当）。
//! 画面ごとに書き分けたくなっても、必ずここに足すこと。
//!
//! # 移植元
//! `scripts/indeed_trend_fit.js`。`tests/indeed_golden_test.rs` で文字列まで突き合わせている。

use super::trend::{Fit, Level};

/// 「1 求人あたり」のように、増えると良い／悪いが決まっている指標の言い分け。
pub struct Words {
    /// 増えたときの言い方（例: "集まりやすくなって"）
    pub up: &'static str,
    /// 減ったときの言い方（例: "集まりにくくなって"）
    pub down: &'static str,
}

/// 求人数など、増減がそのまま意味になるもの
pub const W_JOB: Words = Words {
    up: "増えて",
    down: "減って",
};
/// 1 求人あたりに見た人数。増えるほど集めやすい
pub const W_SEEK: Words = Words {
    up: "集まりやすくなって",
    down: "集まりにくくなって",
};

/// 小数第 1 位まで。JS の `toFixed(1)` と同じ丸め方に合わせる。
fn f1(v: f64) -> String {
    format!("{:.1}", v)
}
/// 整数まで。JS の `toFixed(0)` と同じ丸め方に合わせる。
fn f0(v: f64) -> String {
    format!("{:.0}", v)
}

/// 一覧の「動き方」列に出す短い名前。
pub fn trend_label(fit: Option<&Fit>) -> &'static str {
    let Some(f) = fit else {
        return "データ不足";
    };
    let up = f.slope_pct > 0.0;
    if f.level == Level::None {
        return "月ごとにばらつく";
    }
    // 全体としては動いているが月ごとの振れが大きい場合。
    // 「緩やかに増加」と書くと毎月少しずつ増えていると誤解される。
    if !f.steady {
        return if up { "振れながら増えた" } else { "振れながら減った" };
    }
    if f.level == Level::Mild {
        return if up { "緩やかに増加" } else { "緩やかに減少" };
    }
    if up {
        "増え続けている"
    } else {
        "減り続けている"
    }
}

/// 一覧の 1 行に収まる短い言い方。`describe_trend` と結論が食い違わないこと。
pub fn short_trend(fit: Option<&Fit>, words: &Words) -> String {
    let Some(f) = fit else {
        return "比べられるだけの月数がありません".to_string();
    };
    let sc = f0(f.scatter_pct);
    if f.level == Level::None {
        return format!("月ごとに {sc}% ほど上下するだけで、向きは定まりません");
    }
    let up = f.slope_pct > 0.0;
    let dir = if up { words.up } else { words.down };
    let per = f1(f.slope_pct.abs());
    let tot = f0(f.total_pct.abs());
    if !f.steady {
        return format!(
            "期間全体では {tot}% {dir}いますが、\
             月ごとの上下（±{sc}%）が毎月の動き（約 {per}%）より大きく、一本調子ではありません"
        );
    }
    let strength = if f.level == Level::Mild { "緩やかに" } else { "" };
    format!("{strength}{dir}います（毎月およそ {per}% ずつ）")
}

/// 一文での説明。数字は残しつつ、専門用語を避ける。
///
/// `months` を渡すと、直線から大きく外れた月を名指しする。
///
/// # 直した点
/// 以前は振れの大きさに関わらず「月ごとの上下は X% ほどなので、増え続けていると
/// 見てよさそうです」と書いていた。上下が 44% あっても同じ文が出ており、
/// 振れの大きさを結論の根拠にしていた。逆である。
pub fn describe_trend(fit: Option<&Fit>, what: &str, months: Option<&[String]>) -> String {
    let name = if what.is_empty() { "この数値" } else { what };
    let Some(f) = fit else {
        return format!("{name}は、比べられるだけの月数がありません。");
    };
    let up = f.slope_pct > 0.0;
    let per = f1(f.slope_pct.abs());
    let tot = f0(f.total_pct.abs());
    let sc = f0(f.scatter_pct);

    if f.level == Level::None {
        return format!("{name}は月ごとに {sc}% ほど上下していて、増えているとも減っているとも言えません。");
    }

    // 跳ねた月があれば名指しする
    let spike = match (f.outliers.first(), months) {
        (Some(o), Some(ms)) => match ms.get(o.index) {
            Some(m) => {
                let how = if o.ratio > 0.0 { "跳ねて" } else { "落ち込んで" };
                format!("{m} が大きく{how}います。")
            }
            None => String::new(),
        },
        (Some(_), None) => "途中に大きく外れた月があります。".to_string(),
        _ => String::new(),
    };

    if !f.steady {
        // 一本調子ではない。「毎月 X% ずつ」とは書かない。
        // 上下の大きさは「大きい」と決めつけず、毎月の動きと比べて示す。
        let verb = if up { "増えました" } else { "減りました" };
        let verb2 = if up { "増えた" } else { "減った" };
        let tail = if spike.is_empty() {
            String::new()
        } else {
            format!(" {spike}")
        };
        return format!(
            "{name}はこの期間で {tot}% {verb}が、\
             月ごとの上下（±{sc}%）が毎月の動き（約 {per}%）より大きく、\
             毎月少しずつ{verb2}わけではありません。{tail}"
        );
    }

    let strength = if f.level == Level::Mild { "緩やかに、" } else { "" };
    let verb = if up { "増えて" } else { "減って" };
    let noun = if up { "増加" } else { "減少" };
    let verb2 = if up { "増え" } else { "減り" };
    format!(
        "{name}は{strength}毎月およそ {per}% ずつ{verb}います\
         （この期間で {tot}% {noun}）。\
         月ごとの上下は {sc}% ほどで、{verb2}方はおおむね一定です。"
    )
}

/// 仕組みの説明。知りたい人だけが読む場所に置く用。
pub fn explain_method() -> &'static str {
    "毎月の数値に直線を当てはめ、「1 か月あたり何 % 動いているか」と\
     「その直線からどれだけ散らばっているか」を分けて計算しています。\
     散らばりに対して動きが十分大きいときだけ「増え続けている／減り続けている」と表示します。\
     この方法だと、求人数が少ない職種でも、動きが一定であれば拾えます。\
     月ごとの上下が大きいときは「毎月○% ずつ」とは書かず、\
     一本調子ではないことと、大きく外れた月を示します。"
}

#[cfg(test)]
mod tests {
    use super::super::trend::fit_trend;
    use super::*;

    fn geo(start: f64, rate: f64, n: usize) -> Vec<Option<f64>> {
        (0..n).map(|i| Some(start * rate.powi(i as i32))).collect()
    }
    fn bumpy() -> Vec<Option<f64>> {
        [45.0, 55.0, 62.0, 72.0, 85.0, 78.0, 80.0, 85.0, 173.0, 130.0, 68.0, 65.0, 85.0]
            .iter()
            .map(|x| Some(*x))
            .collect()
    }
    fn months13() -> Vec<String> {
        [
            "2025-07", "2025-08", "2025-09", "2025-10", "2025-11", "2025-12", "2026-01",
            "2026-02", "2026-03", "2026-04", "2026-05", "2026-06", "2026-07",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    /// 一本調子でなければ「毎月およそ」とは書かない
    fn test_no_monthly_rate_when_not_steady() {
        let f = fit_trend(&bumpy());
        let s = short_trend(f.as_ref(), &W_SEEK);
        assert!(!s.contains("毎月およそ"), "{s}");
        assert!(s.contains("一本調子ではありません"), "{s}");
    }

    #[test]
    /// 一本調子なら「毎月およそ」と書く
    fn test_monthly_rate_when_steady() {
        let f = fit_trend(&geo(100.0, 0.95, 13));
        let s = short_trend(f.as_ref(), &W_SEEK);
        assert!(s.contains("毎月およそ 5.0% ずつ"), "{s}");
    }

    #[test]
    /// 跳ねた月を名指しする
    fn test_names_the_spike_month() {
        let m = months13();
        let f = fit_trend(&bumpy());
        let s = describe_trend(f.as_ref(), "求人数", Some(&m));
        assert!(s.contains("2026-03"), "{s}");
        assert!(s.contains("跳ねて"), "{s}");
    }

    #[test]
    /// 一覧と詳細で結論が食い違わない
    fn test_short_and_describe_agree() {
        let m = months13();
        for v in [
            bumpy(),
            geo(100.0, 0.95, 13),
            geo(100.0, 1.08, 13),
            vec![Some(3.0); 13],
        ] {
            let f = fit_trend(&v);
            let a = short_trend(f.as_ref(), &W_SEEK).contains("毎月およそ");
            let b = describe_trend(f.as_ref(), "求人数", Some(&m)).contains("毎月およそ");
            assert_eq!(a, b, "一覧={a} 詳細={b} 系列={v:?}");
        }
    }

    #[test]
    /// 振れの大きさを結論の根拠にしない
    fn test_scatter_is_not_used_as_evidence() {
        let m = months13();
        let f = fit_trend(&bumpy());
        let s = describe_trend(f.as_ref(), "求人数", Some(&m));
        assert!(!s.contains("見てよさそうです"), "{s}");
    }

    #[test]
    /// データが無くても壊れない
    fn test_handles_missing_fit() {
        assert!(!short_trend(None, &W_SEEK).is_empty());
        assert!(!describe_trend(None, "求人数", None).is_empty());
        assert_eq!(trend_label(None), "データ不足");
    }
}
