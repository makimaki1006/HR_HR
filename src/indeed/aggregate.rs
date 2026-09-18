//! Indeed データの集計。
//!
//! # ここを通さない数字を画面に出さない
//! 社内タブと顧客レポートは、必ずこのモジュールが返した値だけを表示する。
//! 画面側で割り算や丸めをやり直すと、同じ指標が 2 つの値を持つ。
//!
//! # 「なぜ」に答えられる形にする
//! 1 求人あたりに見た人数が減ったとき、原因は 2 つしかない。
//! 見た人が減ったか、求人が増えたか。さらに求人が増えた原因も 2 つ。
//! 募集する企業が増えたか、1 社あたりの本数が増えたか。
//! この分解を先に持っておくと、現場が推測で語らずに済む。

use super::data::{PrefSeries, Series, Snapshot};
use super::trend::{fit_trend, Fit, Level};
use super::wording::{describe_trend, short_trend, trend_label, Words, W_JOB, W_SEEK};

/// 1 つの指標について、並び・傾向・文章までひとまとめにしたもの。
pub struct Metric {
    /// 画面に出す名前
    pub label: String,
    /// 月ごとの値。欠測は None のまま
    pub series: Vec<Option<f64>>,
    /// 対数で引いた直線。月数が足りなければ None
    pub fit: Option<Fit>,
    /// 最初に取れている月の値
    pub first: Option<f64>,
    /// 最後に取れている月の値
    pub latest: Option<f64>,
    /// 期間全体で何 % 変わったか（直線に沿った変化）
    pub change_pct: Option<f64>,
    /// 先月からの変化。直近 2 か月の素の比
    ///
    /// [`Self::change_pct`] とは別のものなので、画面では必ず別の名前で出す。
    /// 期間全体はならした線、先月比は 2 点だけ。混ぜると同じ指標に 2 つの値が並ぶ。
    pub mom_pct: Option<f64>,
    /// 前年同月からの変化。12 か月前と比べた素の比
    pub yoy_pct: Option<f64>,
    /// 一覧に出す短い言い方
    pub short: String,
    /// 詳細に出す 1 文
    pub sentence: String,
    /// 「ゆるやかに増加」などの札
    pub label_trend: &'static str,
}

/// 「いちばん新しい月」と「その n か月前」を比べて、変化率を出す。
///
/// # 欠測はずらさない
/// n か月前が欠測のときに、その隣の月で代用してはいけない。
/// 「前年同月比」と言いながら 11 か月前と比べることになる。取れなければ出さない。
fn ratio_back(series: &[Option<f64>], n: usize) -> Option<f64> {
    let last = series.iter().rposition(|v| v.is_some())?;
    let prev = last.checked_sub(n)?;
    let (a, b) = (series[prev]?, series[last]?);
    if a > 0.0 {
        Some((b / a - 1.0) * 100.0)
    } else {
        None
    }
}

impl Metric {
    fn build(label: &str, series: Vec<Option<f64>>, words: &Words, months: &[String]) -> Self {
        let fit = fit_trend(&series);
        let first = series.iter().flatten().next().copied();
        let latest = series.iter().flatten().next_back().copied();
        // 期間全体の変化は、文章と同じ出どころを使う。
        //
        // 文章（describe_trend）は直線に沿った変化 total_pct を語る。ここで
        // 「最後 ÷ 最初 - 1」を別に出すと、同じ指標に 2 つの値が並ぶ。
        // 実際に「26% 増えました」の隣に「+22.3%」が出ていた。
        // 直線に沿った方が端の月のブレに引きずられないので、そちらに揃える。
        //
        // 直線が引けない（月数が足りない）ときは、変化率も出さない。
        // 2 点しかない並びから「最後 ÷ 最初」を出すと、文章が
        // 「比べられるだけの月数がありません」と言っている隣で
        // -56.9% のような具体的な数字が出て、読む人が混乱する。
        // 直線に沿った期間全体の変化。向きが定まらない指標でも値そのものは出す。
        //
        // 一度「向きが定まらないなら数字も出さない」に倒したが、14 か月に
        // なった時点で主要 3 指標すべてが「向き不明」（傾き/ばらつき比 0.11〜0.22、
        // しきい値 1.8）になり、画面から数字が全部消えた。過剰だった。
        // 数字は出したうえで、[`Metric::label_trend`] の「月ごとにばらつく」と
        // [`Overview::why`] の断りで、向きが定まらないことを伝える。
        let change_pct = fit.as_ref().map(|f| f.total_pct);
        // 「先月と比べてどうか」「去年の同じ月と比べてどうか」は、
        // ならした線ではなく素の比で答える。読む人が数えられる形にするため。
        let mom_pct = ratio_back(&series, 1);
        let yoy_pct = ratio_back(&series, 12);
        Self {
            short: short_trend(fit.as_ref(), words),
            sentence: describe_trend(fit.as_ref(), label, Some(months)),
            label_trend: trend_label(fit.as_ref()),
            label: label.to_string(),
            series,
            fit,
            first,
            latest,
            change_pct,
            mom_pct,
            yoy_pct,
        }
    }

    // 指数（`indexed`）はここにあったが、2026-09 に削除した。
    //
    // 基準は「その系列にデータがある最初の月」で、固定の月ではなかった。
    // 分析に使う月は取得が 98% 以上そろった月から毎回決め直しているので
    // （`scripts/indeed_build_insights.js`）、古い月を 1 つ取り込むだけで
    // 基準が前にずれ、過去に出した数字が全部書き換わっていた。
    // 実測では基準を 2025-07 から 2025-08 に動かすと軽作業が 105.9 → 98.5 になり、
    // 実数が 1 件も動いていないのに「増えた」が「減った」に反転した。
    // 先月の画面と今月の画面を並べられないので、図はすべて実数に戻してある
    // （`render::raw_line_chart` / `render::small_multiples` / `render::dual_line_chart`）。
    // 規模差で重ねられないときは指数ではなく図を分けること。
}

/// 全体、または 1 つの業界・職種の姿。
pub struct Overview {
    /// 何の集計か（「全国ぜんぶ」「介護」など）
    pub name: String,
    /// 求人数
    pub job: Metric,
    /// 求人を見た人数。応募数ではない
    pub ctk: Metric,
    /// 募集している企業の数
    pub emp: Metric,
    /// 1 求人あたりに見た人数
    pub spp: Metric,
    /// 1 社あたりの求人数
    pub ppe: Metric,
}

/// 求人の数の伸びが「会社が増えたぶん」と「1 社あたりが増えたぶん」のどちらから来たか。
///
/// `求人の数 = 募集した企業の数 × 1 社あたりの本数` の分解をそのまま数字にしたもの。
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Breakdown {
    /// 求人の数の変化率（%）
    pub job_pct: f64,
    /// 募集した企業の数の変化率（%）
    pub emp_pct: f64,
    /// 1 社あたりの本数の変化率（%）
    pub per_pct: f64,
    /// 伸びのうち「会社が増えたぶん」の割合（%）。
    /// **0 未満や 100 超もありうる**（2 つの要素が逆を向いた場合）。
    pub emp_share: f64,
}

/// 求人の数がこれ未満しか動いていない職種は分解しない（%）。
///
/// 分母が小さいと割合が跳ねる。実測で清掃スタッフは求人 -0.1% / 企業 -4.2% で、
/// 割合が 4000% を超えた。動いていないものを分解しても読むものが無い。
const BREAKDOWN_FLOOR: f64 = 3.0;

/// 両端 3 か月ずつの平均で変化率を出す。端の 1 か月のぶれに引きずられないため。
fn ends_change(v: &[Option<f64>]) -> Option<f64> {
    const K: usize = 3;
    let x: Vec<f64> = v
        .iter()
        .flatten()
        .copied()
        .filter(|n| n.is_finite())
        .collect();
    if x.len() < K * 2 {
        return None;
    }
    let head = x[..K].iter().sum::<f64>() / K as f64;
    let tail = x[x.len() - K..].iter().sum::<f64>() / K as f64;
    if head <= 0.0 {
        return None;
    }
    Some((tail / head - 1.0) * 100.0)
}

/// 求人の数と企業の数から、伸びの内訳を出す。測れないときは `None`。
///
/// # なぜ符号の組み合わせ（4 象限）にしないのか
/// 最初は「求人数↑ × 企業数↑ なら新規参入」と読む形を考えたが、実データで破綻する。
/// 一般事務は求人 +38.3% / 企業 +2.6% で、符号だけ見ると「新規参入」になるが、
/// 実際は **1 社あたりが +34.8%** で、伸びのほぼ全部が既存企業の増産だった。
/// 伸びの**どちらがどれだけ効いたか**を見ないと読み違える。
///
/// 割合は対数で出す。`求人 = 企業 × 1社あたり` は掛け算なので、
/// 対数を取ると足し算になり、そのまま寄与の割合として割れる。
pub fn growth_breakdown(job: &[Option<f64>], emp: &[Option<f64>]) -> Option<Breakdown> {
    let job_pct = ends_change(job)?;
    let emp_pct = ends_change(emp)?;
    if job_pct.abs() < BREAKDOWN_FLOOR {
        return None;
    }
    // -100% 以下は対数が取れない（企業数が 0 になった等）
    if job_pct <= -100.0 || emp_pct <= -100.0 {
        return None;
    }
    let per_pct = ((1.0 + job_pct / 100.0) / (1.0 + emp_pct / 100.0) - 1.0) * 100.0;
    let lj = (1.0 + job_pct / 100.0).ln();
    if lj.abs() < 1e-9 {
        return None;
    }
    let emp_share = (1.0 + emp_pct / 100.0).ln() / lj * 100.0;
    if !emp_share.is_finite() {
        return None;
    }
    Some(Breakdown {
        job_pct,
        emp_pct,
        per_pct,
        emp_share,
    })
}

impl Breakdown {
    /// 数字をそのまま言葉にする。良し悪しの判断は入れない。
    pub fn reading(&self) -> &'static str {
        match self.emp_share {
            s if s < 0.0 => "会社の数は減りましたが、いまいる会社が本数を増やして押し上げました",
            s if s <= 30.0 => "いまいる会社が本数を増やしたぶんが大きいです",
            s if s < 70.0 => "会社が増えたぶんと、1 社あたりが増えたぶんが半々です",
            s if s <= 100.0 => "新しく募集を始めた会社が増えたぶんが大きいです",
            _ => "会社は増えましたが、1 社あたりの本数はむしろ減りました",
        }
    }
}

impl Overview {
    /// 並びから作る。
    pub fn from_series(name: &str, s: &Series, months: &[String]) -> Self {
        Self {
            name: name.to_string(),
            job: Metric::build("求人の数", s.job.clone(), &W_JOB, months),
            ctk: Metric::build("求人を見た人数", s.ctk.clone(), &W_JOB, months),
            emp: Metric::build("募集している企業の数", s.emp.clone(), &W_JOB, months),
            spp: Metric::build(
                "1 求人あたりに見た人数",
                s.seekers_per_posting(),
                &W_SEEK,
                months,
            ),
            ppe: Metric::build(
                "1 社あたりの求人数",
                s.postings_per_employer(),
                &W_JOB,
                months,
            ),
        }
    }

    /// 「なぜそうなったか」を、推測ではなく割り算で説明する。
    ///
    /// 1 求人あたり = 見た人数 ÷ 求人数 なので、この 2 つの動きで説明が尽きる。
    /// さらに 求人数 = 募集企業数 × 1 社あたり本数 に分かれる。
    pub fn why(&self) -> String {
        let (Some(spp), Some(ctk), Some(job)) = (
            self.spp.change_pct,
            self.ctk.change_pct,
            self.job.change_pct,
        ) else {
            return format!("{}は、比べられるだけの月数がそろっていません。", self.name);
        };
        let dir = if spp >= 0.0 {
            "集まりやすく"
        } else {
            "集まりにくく"
        };
        // 「見た人が減ったからではなく」を決め打ちで書いていたため、
        // 効いているのが見た人数のときに「見た人が減ったからではなく、
        // 見た人数の変化のほうが効いています」という自己矛盾になっていた。
        // 実測で 6 県中 4 県が該当。どちらが効いているかだけを述べる
        let bigger_ctk = ctk.abs() >= job.abs();
        let mut s = format!(
            "{}では、1 求人あたりに見た人数がこの期間で {:+.1}% 動きました。             内訳は、求人を見た人数が {:+.1}%、求人の数が {:+.1}% です。             効いているのは{}のほうで、{:+.1}% と {:+.1}% の差がそのまま出ています。",
            self.name,
            spp,
            ctk,
            job,
            if bigger_ctk { "見た人数" } else { "求人数" },
            if bigger_ctk { ctk } else { job },
            if bigger_ctk { job } else { ctk }
        );
        if let (Some(emp), Some(ppe)) = (self.emp.change_pct, self.ppe.change_pct) {
            s.push_str(&format!(
                " 求人の数はさらに、募集した企業の数が {:+.1}%、1 社あたりの本数が {:+.1}% に分かれます。",
                emp, ppe
            ));
        }
        s.push_str(&format!(" 全体としては{}なっています。", dir));

        // 月ごとの上下が傾きより大きい指標があるときは、必ず断る。
        // 数字だけ見せると「そう動いている」と読まれるが、この判定では
        // 向きそのものが定まっていない。
        let unsure: Vec<&str> = [&self.job, &self.ctk, &self.emp, &self.spp]
            .iter()
            .filter(|m| {
                m.fit
                    .as_ref()
                    .map(|f| f.level == Level::None)
                    .unwrap_or(false)
            })
            .map(|m| m.label.as_str())
            .collect();
        if !unsure.is_empty() {
            s.push_str(&format!(
                " ただし{}は、月ごとの上下が毎月の動きより大きく、期間全体の向きは定まっていません。\
                 上の % は月ごとの上下をならした線に沿った変化で、\
                 その方向に動き続けていることを示すものではありません。",
                unsure.join("・")
            ));
        }
        s
    }
}

/// 分類（業界）の定点表 1 行分。
#[derive(Clone)]
pub struct CategoryRow {
    pub name: String,
    /// この分類に入る職種の数
    pub titles: usize,
    /// 最新月の求人数
    pub job_latest: Option<f64>,
    /// 期間全体の求人数の変化
    pub job_change_pct: Option<f64>,
    /// 期間全体の「見た人数」の変化
    pub ctk_change_pct: Option<f64>,
    /// 最新月の 1 求人あたり
    pub spp_latest: Option<f64>,
    /// 期間全体の 1 求人あたりの変化
    pub spp_change_pct: Option<f64>,
    /// 動き方の札
    pub trend: &'static str,
    /// 一覧に出す短い言い方
    pub short: String,
    /// 一本調子かどうか。false なら「毎月ずつ」と書いてはいけない
    pub steady: bool,
}

/// 20 分類の定点表を作る。求人数の多い順。
/// 20 分類の定点表。県を指定すればその県のぶん。
///
/// # なぜ県でも作るのか
/// タブに分けたあと、県を選ぶと分類の表が丸ごと消えていた。
/// 中身は `insight_title_pref` に県別で入っているので集約すれば出せる。
///
/// # 県のときは「そろっている職種」だけ
/// その県で月が欠けている職種を混ぜると、母集団が月で変わって先月比が壊れる。
pub fn category_table_at(snap: &Snapshot, pref: Option<&str>) -> Vec<CategoryRow> {
    let Some(p) = pref else {
        return category_table(snap);
    };
    let months = &snap.meta.months;
    let n = snap.n_months();
    let cat_of = |name: &str| -> String {
        snap.titles
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.category.clone())
            .unwrap_or_default()
    };
    let mut acc: std::collections::HashMap<String, (Series, usize)> =
        std::collections::HashMap::new();
    for r in pref_rows(snap, p) {
        if r.series.job.iter().any(|v| v.is_none()) {
            continue;
        }
        let c = cat_of(&r.title);
        if c.is_empty() {
            continue;
        }
        let e = acc.entry(c).or_insert_with(|| (Series::blank_public(n), 0));
        e.0.merge_public(&r.series);
        e.1 += 1;
    }
    let mut rows: Vec<CategoryRow> = acc
        .into_iter()
        .map(|(name, (s, titles))| {
            let ov = Overview::from_series(&name, &s, months);
            CategoryRow {
                titles,
                job_latest: ov.job.latest,
                job_change_pct: ov.job.change_pct,
                ctk_change_pct: ov.ctk.change_pct,
                spp_latest: ov.spp.latest,
                spp_change_pct: ov.spp.change_pct,
                trend: ov.job.label_trend,
                short: ov.job.short.clone(),
                steady: ov.job.fit.as_ref().map(|f| f.steady).unwrap_or(false),
                name,
            }
        })
        .collect();
    // 求人数の多い順。全国の並びと同じ見え方にする
    rows.sort_by(|a, b| {
        b.job_latest
            .unwrap_or(0.0)
            .total_cmp(&a.job_latest.unwrap_or(0.0))
    });
    rows
}

pub fn category_table(snap: &Snapshot) -> Vec<CategoryRow> {
    let months = &snap.meta.months;
    snap.categories_by_size()
        .into_iter()
        .filter_map(|name| {
            let s = snap.by_category.get(&name)?;
            let ov = Overview::from_series(&name, s, months);
            Some(CategoryRow {
                titles: snap
                    .category_titles
                    .get(&name)
                    .map(|v| v.len())
                    .unwrap_or(0),
                job_latest: ov.job.latest,
                job_change_pct: ov.job.change_pct,
                ctk_change_pct: ov.ctk.change_pct,
                spp_latest: ov.spp.latest,
                spp_change_pct: ov.spp.change_pct,
                trend: ov.job.label_trend,
                short: ov.job.short.clone(),
                steady: ov.job.fit.as_ref().map(|f| f.steady).unwrap_or(false),
                name,
            })
        })
        .collect()
}

/// 職種の一覧 1 行分。
pub struct TitleRow {
    pub name: String,
    pub category: String,
    pub job_latest: Option<f64>,
    pub job_change_pct: Option<f64>,
    pub spp_latest: Option<f64>,
    pub spp_change_pct: Option<f64>,
    pub trend: &'static str,
    pub short: String,
    pub steady: bool,
}

/// 職種の一覧を作る。求人数の多い順。
pub fn title_table(snap: &Snapshot) -> Vec<TitleRow> {
    let months = &snap.meta.months;
    snap.titles_by_size()
        .into_iter()
        .filter_map(|t| {
            let s = snap.by_title.get(&t.name)?;
            let ov = Overview::from_series(&t.name, s, months);
            Some(TitleRow {
                name: t.name.clone(),
                category: t.category.clone(),
                job_latest: ov.job.latest,
                job_change_pct: ov.job.change_pct,
                spp_latest: ov.spp.latest,
                spp_change_pct: ov.spp.change_pct,
                trend: ov.job.label_trend,
                short: ov.job.short.clone(),
                steady: ov.job.fit.as_ref().map(|f| f.steady).unwrap_or(false),
            })
        })
        .collect()
}

/// 都道府県 1 つ分の姿。
///
/// 画面ごとにこの合算を書くと、社内タブと顧客レポートで数字がずれる。
/// 足し方（欠測を 0 で埋めない）もここ 1 箇所に閉じ込める。
pub fn pref_overview(snap: &Snapshot, pref: &str) -> Overview {
    let n = snap.n_months();
    let mut s = Series {
        job: vec![None; n],
        ctk: vec![None; n],
        emp: vec![None; n],
    };
    for row in pref_rows(snap, pref) {
        s.merge_public(&row.series);
    }
    Overview::from_series(pref, &s, &snap.meta.months)
}

/// ある都道府県の行だけを取り出す。
///
/// `by_pref` は都道府県・職種の順に並べてあるので、頭から 51,787 行を
/// 見に行かずに、範囲を二分探索で切り出す。1 リクエストで概要と一覧の
/// 2 回呼ばれるので、毎回の全走査は無駄。
pub fn pref_rows<'a>(snap: &'a Snapshot, pref: &str) -> &'a [PrefSeries] {
    let start = snap
        .by_pref
        .partition_point(|r| r.prefecture.as_str() < pref);
    let end = snap
        .by_pref
        .partition_point(|r| r.prefecture.as_str() <= pref);
    &snap.by_pref[start..end]
}

/// ある都道府県の、職種ごとの姿。
///
/// 社内タブと顧客レポートが同じ並びを見るための唯一の入口。
/// 画面ごとに `by_pref` を絞って組み立てると、片方だけ直す事故になる。
pub fn pref_title_overviews(snap: &Snapshot, pref: &str) -> Vec<(String, String, Overview)> {
    let months = &snap.meta.months;
    let cat_of = |name: &str| -> String {
        snap.titles
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.category.clone())
            .unwrap_or_default()
    };
    pref_rows(snap, pref)
        .iter()
        .map(|r| {
            (
                r.title.clone(),
                cat_of(&r.title),
                Overview::from_series(&r.title, &r.series, months),
            )
        })
        .collect()
}

/// 全国ぜんぶの姿。
pub fn nation_overview(snap: &Snapshot) -> Overview {
    Overview::from_series("全国のぜんぶの職種", &snap.nation, &snap.meta.months)
}

/// いちばん増えた／減った分類を、向きを間違えずに取り出す。
///
/// 「いちばん増えたのは A（-3.1%）です」という文が出た事故があった。
/// 全部が減っているときに「増えた」と書かないよう、向きも一緒に返す。
pub fn extreme(rows: &[CategoryRow], want_top: bool) -> Option<(&CategoryRow, bool)> {
    let mut best: Option<&CategoryRow> = None;
    for r in rows {
        let Some(v) = r.job_change_pct else { continue };
        match best {
            None => best = Some(r),
            Some(b) => {
                let bv = b.job_change_pct.unwrap_or(0.0);
                let better = if want_top { v > bv } else { v < bv };
                if better {
                    best = Some(r);
                }
            }
        }
    }
    best.map(|r| {
        let positive = r.job_change_pct.unwrap_or(0.0) >= 0.0;
        (r, positive)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 6 か月ぶんの並びを作る。前半 3 か月が `from`、後半 3 か月が `to`。
    fn 並び(from: f64, to: f64) -> Vec<Option<f64>> {
        vec![
            Some(from),
            Some(from),
            Some(from),
            Some(to),
            Some(to),
            Some(to),
        ]
    }

    /// 符号の組み合わせ（4 象限）では読み違える、という実例を固定する。
    ///
    /// 一般事務は求人 +38.3% / 企業 +2.6%。符号だけ見ると「どちらも増えた＝新規参入」
    /// になるが、1 社あたりが +34.8% で、伸びのほぼ全部が既存企業の増産だった。
    /// 割合で見れば 8% しか会社ぶんが無いことが分かる。
    #[test]
    fn 符号が同じでも中身は違う() {
        let b = growth_breakdown(&並び(100.0, 138.3), &並び(100.0, 102.6)).expect("分解できるはず");
        assert!(b.job_pct > 38.0 && b.job_pct < 38.6, "求人 {}", b.job_pct);
        assert!(b.emp_pct > 2.5 && b.emp_pct < 2.7, "企業 {}", b.emp_pct);
        assert!(
            b.per_pct > 34.0 && b.per_pct < 35.5,
            "1 社あたり {}",
            b.per_pct
        );
        assert!(
            b.emp_share < 15.0,
            "会社ぶんの割合が {:.0}% と出た。符号だけ見て「新規参入」と読むのが誤り",
            b.emp_share
        );
        assert_eq!(b.reading(), "いまいる会社が本数を増やしたぶんが大きいです");

        // 逆に、伸びがほぼ全部が会社ぶんのケース（営業: 求人 +19.9 / 企業 +19.1）
        let c = growth_breakdown(&並び(100.0, 119.9), &並び(100.0, 119.1)).expect("分解できるはず");
        assert!(c.emp_share > 90.0, "会社ぶんの割合 {:.0}%", c.emp_share);
        assert_eq!(
            c.reading(),
            "新しく募集を始めた会社が増えたぶんが大きいです"
        );
    }

    /// 求人がほとんど動いていない職種は分解しない。
    ///
    /// 清掃スタッフは実測で求人 -0.1% / 企業 -4.2%。そのまま割ると
    /// 会社ぶんの割合が 4000% を超える。分母が小さいものを割ってはいけない。
    #[test]
    fn 動いていない職種は分解しない() {
        assert!(
            growth_breakdown(&並び(100.0, 99.9), &並び(100.0, 95.8)).is_none(),
            "求人 -0.1% でも分解してしまっている"
        );
        // 月が足りないものも出さない
        assert!(growth_breakdown(&[Some(1.0), Some(2.0)], &[Some(1.0), Some(2.0)]).is_none());
        // 欠測だらけ
        assert!(growth_breakdown(&[None; 8], &[None; 8]).is_none());
    }

    /// 2 つが逆を向いたときは、割合が 0〜100 の外に出る。言葉もそれに合わせる。
    #[test]
    fn 逆を向いたら割合は範囲の外に出る() {
        // 会社は減ったのに求人は増えた（フォークリフト: 求人 +4.5 / 企業 -4.2）
        let a = growth_breakdown(&並び(100.0, 104.5), &並び(100.0, 95.8)).expect("分解できるはず");
        assert!(
            a.emp_share < 0.0,
            "割合 {:.0}% が負になっていない",
            a.emp_share
        );
        assert_eq!(
            a.reading(),
            "会社の数は減りましたが、いまいる会社が本数を増やして押し上げました"
        );

        // 会社は増えたのに 1 社あたりは減った（調理スタッフ: 求人 +11.5 / 企業 +11.9）
        let b = growth_breakdown(&並び(100.0, 111.5), &並び(100.0, 111.9)).expect("分解できるはず");
        assert!(b.emp_share > 100.0, "割合 {:.0}%", b.emp_share);
        assert_eq!(
            b.reading(),
            "会社は増えましたが、1 社あたりの本数はむしろ減りました"
        );
    }

    /// 先月比・前年同月比は「素の比」で、欠測をずらして代用しない。
    #[test]
    fn 先月比と前年同月比はずらして代用しない() {
        // 13 か月ぶん。最後が 130、12 か月前が 100
        let mut v: Vec<Option<f64>> = (0..13).map(|i| Some(100.0 + i as f64 * 2.5)).collect();
        // 100 → 130 なので +30%
        let yoy = ratio_back(&v, 12).expect("前年同月比が出ていない");
        assert!((yoy - 30.0).abs() < 1e-9, "前年同月比が {yoy}");
        // 127.5 → 130 なので約 +1.96%
        let mom = ratio_back(&v, 1).expect("先月比が出ていない");
        assert!(
            (mom - 100.0 * (130.0 / 127.5 - 1.0)).abs() < 1e-9,
            "先月比が {mom}"
        );

        // 12 か月前が欠測なら、隣で代用せずに出さない
        v[0] = None;
        assert_eq!(
            ratio_back(&v, 12),
            None,
            "12 か月前が取れていないのに前年同月比を出している"
        );

        // 月数が足りなければ出さない
        let short = vec![Some(1.0), Some(2.0)];
        assert_eq!(ratio_back(&short, 12), None);
    }

    /// 期間全体の変化と、先月比・前年同月比は別物であること。
    ///
    /// 同じ数字を別の名前で 2 回出すと、読む人はどちらかが間違っていると思う。
    #[test]
    fn 期間全体と先月比は別の数字である() {
        let months: Vec<String> = (0..13).map(|i| format!("2025-{:02}", i + 1)).collect();
        // 途中で跳ねる並び。ならした線と、直近 2 点の比は一致しないはず
        let mut v: Vec<Option<f64>> = (0..13).map(|i| Some(100.0 + i as f64)).collect();
        v[12] = Some(200.0);
        let m = Metric::build("試し", v, &W_JOB, &months);
        let (Some(total), Some(mom)) = (m.change_pct, m.mom_pct) else {
            panic!("変化率が出ていない");
        };
        assert!(
            (total - mom).abs() > 1.0,
            "期間全体 {total} と先月比 {mom} が同じ値になっている"
        );
    }
}

// ---------------------------------------------------------------------------
// 5 業界
//
// Indeed の 20 分類のままでは、同じ会社の話が離れた場所に出て繋げられない。
// 顧客に配る見本と同じまとめ方（[`super::industry`]）で 5 つにする。
// 5 つに入らないものは無理に入れず、「5 業界の外」として別に数える。
// ---------------------------------------------------------------------------

/// 業界 1 つ分の姿。
pub struct IndustryRow {
    pub name: String,
    /// この業界に入る職種の数
    pub titles: usize,
    /// この業界の職種名（求人数の多い順）
    pub top_titles: Vec<String>,
    /// 最新月の求人数
    pub job_latest: Option<f64>,
    /// 全体に占める割合（%）
    pub share_pct: Option<f64>,
    /// 期間全体の求人数の変化
    pub job_change_pct: Option<f64>,
    /// 先月からの変化
    pub job_mom_pct: Option<f64>,
    /// 前年同月からの変化
    pub job_yoy_pct: Option<f64>,
    /// 最新月の 1 求人あたり
    pub spp_latest: Option<f64>,
    /// 期間全体の 1 求人あたりの変化
    pub spp_change_pct: Option<f64>,
    /// 動き方の札
    pub trend: &'static str,
    /// まとめた理由。外の枠には無い
    pub why: Option<&'static str>,
    /// この業界の見立てそのもの。
    ///
    /// 表・指数図・一行説明で同じものを使う。呼び出し側で作り直すと、
    /// 同じ業界の回帰を 1 リクエストで 3 回計算することになる。
    pub ov: Overview,
}

/// 業界ごとに足した並びを作る。「5 業界の外」も 1 つの枠として返す。
/// 業界ごとの月次を作る。県を指定すればその県のぶん。
///
/// # なぜ県でも作れるようにしたのか
/// タブに分けたあと、県を選ぶと「業界・分類」の面が丸ごと空になっていた。
/// 中身は `insight_title_pref` に県別で入っているので、集約すれば出せる。
/// 「作れないから消す」ではなく「作れるものは作る」。
///
/// # 県のときの「そろっている職種」の決め方
/// 全国では `Title::complete`（全期間そろっているか）で絞っている。
/// 県では職種ごとに取れている月が違うので、その県の系列が
/// 全月そろっているものだけを足す。混ぜると母集団が月で変わり、
/// 先月比が実態と関係なく動く。
pub fn industry_series_at(
    snap: &Snapshot,
    pref: Option<&str>,
) -> Vec<(String, Series, Vec<String>)> {
    use super::industry;
    let n = snap.n_months();
    let mut order: Vec<String> = industry::INDUSTRIES
        .iter()
        .map(|i| i.name.to_string())
        .collect();
    order.push(industry::OUTSIDE.to_string());

    let mut acc: HashMapAlias = std::collections::HashMap::new();
    let cat_of = |name: &str| -> String {
        snap.titles
            .iter()
            .find(|t| t.name == name)
            .map(|t| t.category.clone())
            .unwrap_or_default()
    };
    match pref {
        None => {
            for t in &snap.titles {
                // 合計は全期間そろっている職種だけ。途中から取り始めたものを混ぜると、
                // 母集団が月によって変わって比べられなくなる
                if !t.complete {
                    continue;
                }
                let key = industry::of_category(&t.category)
                    .unwrap_or(industry::OUTSIDE)
                    .to_string();
                let Some(s) = snap.by_title.get(&t.name) else {
                    continue;
                };
                let e = acc
                    .entry(key)
                    .or_insert_with(|| (Series::blank_public(n), Vec::new()));
                e.0.merge_public(s);
                e.1.push(t.name.clone());
            }
        }
        Some(p) => {
            for r in pref_rows(snap, p) {
                // その県で全月そろっている職種だけ
                if r.series.job.iter().any(|v| v.is_none()) {
                    continue;
                }
                let key = industry::of_category(&cat_of(&r.title))
                    .unwrap_or(industry::OUTSIDE)
                    .to_string();
                let e = acc
                    .entry(key)
                    .or_insert_with(|| (Series::blank_public(n), Vec::new()));
                e.0.merge_public(&r.series);
                e.1.push(r.title.clone());
            }
        }
    }

    order
        .into_iter()
        .filter_map(|name| {
            let (s, mut names) = acc.remove(&name)?;
            // 職種は求人数の多い順。名前を出すときに上から使う
            // 「求人数が多いのは」に使う並び。県を見ているときは
            // その県の求人数で並べる（全国の大きさで並べると別の話になる）
            let size = |name: &str| -> f64 {
                match pref {
                    None => snap
                        .by_title
                        .get(name)
                        .and_then(|x| Snapshot::last_of(&x.job))
                        .unwrap_or(0.0),
                    Some(p) => pref_rows(snap, p)
                        .iter()
                        .find(|r| r.title == name)
                        .and_then(|r| Snapshot::last_of(&r.series.job))
                        .unwrap_or(0.0),
                }
            };
            names.sort_by(|a, b| size(b).total_cmp(&size(a)));
            Some((name, s, names))
        })
        .collect()
}

type HashMapAlias = std::collections::HashMap<String, (Series, Vec<String>)>;

/// 5 業界＋外の定点表。並びは見本と同じ（業界の並び順は固定）。
pub fn industry_table_at(snap: &Snapshot, pref: Option<&str>) -> Vec<IndustryRow> {
    use super::industry;
    let months = &snap.meta.months;
    let all = match pref {
        None => Snapshot::last_of(&snap.nation.job),
        // 割合の分母はその県の合計。全国で割ると「東京都は全体の 3%」のような
        // 別の意味の数字になる
        Some(p) => {
            let mut t = 0.0;
            for r in pref_rows(snap, p) {
                t += Snapshot::last_of(&r.series.job).unwrap_or(0.0);
            }
            if t > 0.0 {
                Some(t)
            } else {
                None
            }
        }
    };
    industry_series_at(snap, pref)
        .into_iter()
        .map(|(name, s, names)| {
            let ov = Overview::from_series(&name, &s, months);
            IndustryRow {
                titles: names.len(),
                top_titles: names.into_iter().take(3).collect(),
                job_latest: ov.job.latest,
                share_pct: match (ov.job.latest, all) {
                    (Some(j), Some(a)) if a > 0.0 => Some(j / a * 100.0),
                    _ => None,
                },
                job_change_pct: ov.job.change_pct,
                job_mom_pct: ov.job.mom_pct,
                job_yoy_pct: ov.job.yoy_pct,
                spp_latest: ov.spp.latest,
                spp_change_pct: ov.spp.change_pct,
                trend: ov.job.label_trend,
                why: industry::why(&name),
                name,
                ov,
            }
        })
        .collect()
}
