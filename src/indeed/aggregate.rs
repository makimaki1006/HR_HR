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
        assert!((mom - 100.0 * (130.0 / 127.5 - 1.0)).abs() < 1e-9, "先月比が {mom}");

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
pub fn industry_series(snap: &Snapshot) -> Vec<(String, Series, Vec<String>)> {
    use super::industry;
    let n = snap.n_months();
    let mut order: Vec<String> = industry::INDUSTRIES.iter().map(|i| i.name.to_string()).collect();
    order.push(industry::OUTSIDE.to_string());

    let mut acc: HashMapAlias = std::collections::HashMap::new();
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

    order
        .into_iter()
        .filter_map(|name| {
            let (s, mut names) = acc.remove(&name)?;
            // 職種は求人数の多い順。名前を出すときに上から使う
            names.sort_by(|a, b| {
                let ja = snap
                    .by_title
                    .get(a)
                    .and_then(|x| Snapshot::last_of(&x.job))
                    .unwrap_or(0.0);
                let jb = snap
                    .by_title
                    .get(b)
                    .and_then(|x| Snapshot::last_of(&x.job))
                    .unwrap_or(0.0);
                jb.total_cmp(&ja)
            });
            Some((name, s, names))
        })
        .collect()
}

type HashMapAlias = std::collections::HashMap<String, (Series, Vec<String>)>;

/// 5 業界＋外の定点表。並びは見本と同じ（業界の並び順は固定）。
pub fn industry_table(snap: &Snapshot) -> Vec<IndustryRow> {
    use super::industry;
    let months = &snap.meta.months;
    let all = Snapshot::last_of(&snap.nation.job);
    industry_series(snap)
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
