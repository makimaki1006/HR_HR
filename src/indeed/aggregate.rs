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
use super::trend::{fit_trend, Fit};
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
    /// 期間全体で何 % 変わったか
    pub change_pct: Option<f64>,
    /// 一覧に出す短い言い方
    pub short: String,
    /// 詳細に出す 1 文
    pub sentence: String,
    /// 「ゆるやかに増加」などの札
    pub label_trend: &'static str,
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
        let change_pct = fit.as_ref().map(|f| f.total_pct);
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
        }
    }

    /// 基準月を 100 とした指数。業界どうしを同じ物差しで並べるために使う。
    ///
    /// 業界ごとに軸を自動調整すると、伸びの違う業界の図が全部同じ形に見える。
    pub fn indexed(&self) -> Vec<Option<f64>> {
        let Some(base) = self.series.iter().flatten().next().copied() else {
            return vec![None; self.series.len()];
        };
        if base <= 0.0 {
            return vec![None; self.series.len()];
        }
        self.series
            .iter()
            .map(|v| v.map(|x| x / base * 100.0))
            .collect()
    }
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
            return format!(
                "{}は、比べられるだけの月数がそろっていません。",
                self.name
            );
        };
        let dir = if spp >= 0.0 {
            "集まりやすく"
        } else {
            "集まりにくく"
        };
        let mut s = format!(
            "{}では、1 求人あたりに見た人数がこの期間で {:+.1}% 動きました。\
             内訳は、求人を見た人数が {:+.1}%、求人の数が {:+.1}% です。\
             見た人が減ったからではなく、{}のほうが効いています。",
            self.name,
            spp,
            ctk,
            job,
            if ctk.abs() >= job.abs() {
                "見た人数の変化"
            } else {
                "求人数の変化"
            }
        );
        if let (Some(emp), Some(ppe)) = (self.emp.change_pct, self.ppe.change_pct) {
            s.push_str(&format!(
                " 求人の数はさらに、募集した企業の数が {:+.1}%、1 社あたりの本数が {:+.1}% に分かれます。",
                emp, ppe
            ));
        }
        s.push_str(&format!(" 全体としては{}なっています。", dir));
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
