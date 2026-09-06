//! 検索ボリュームの「暦月ぐせ」。
//!
//! # 何を測るか
//! 「介護 求人」のような語が月にどれだけ検索されたかの並びから、
//! 暦月（1〜12 月）ごとの高さを出す。年間平均を 1.0 とした比なので、
//! 検索数の桁が違う職種どうしでも並べて比べられる。
//!
//! # variant='job' だけを読む理由
//! 同じ表には「職種名だけ」で検索された数（`variant='name'`）も入っている。
//! こちらは求人を探す以外の検索を多く含む（分析層を作る
//! `scripts/indeed_build_insights.js` は、飲食店 4,090,000 に対して
//! 「飲食店 求人」5,400 という差を根拠にこの 2 つを分けている）。
//! 採用の話に使うのは「職種名＋求人」のほうなので、`job` だけを読む。
//!
//! # 指数は表から読まずに自分で出す
//! 表には `peak_month` などが既に入っているが、指数そのものは入っていない。
//! 指数だけ自前で出して山の月を表から読むと、両者が食い違ったときに
//! 「山の月」と「指数がいちばん高い月」が別々の月を指しかねない。
//! 辻褄を保つため、指数・山・谷・山の高さの 4 つとも `series` から計算する。
//! 表の値との一致はテストで測っている（2026-09-06 時点の実データでは
//! 84 職種すべてで山も谷も一致した）。
//!
//! # 読むときの限界
//! * 元は検索エンジンの推定値で、月ごとに丸められている。細かい上下は読めない
//! * 4 年ぶんをならした値なので、ある年だけの事情は薄まる。
//!   逆に 4 年とも同じ月に起きた出来事があれば、季節のくせと区別できない
//! * 検索数であって応募数ではない

use anyhow::{anyhow, Result};

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::get_str;

/// 1 職種ぶんの季節性。
#[derive(Debug, Clone)]
pub struct TitleSeason {
    pub title: String,
    /// 暦月 1〜12 の指数。年間平均を 1.0 としたときの比。欠測は None
    pub index: [Option<f64>; 12],
    /// いちばん高い暦月（1〜12）
    pub peak_month: Option<u32>,
    /// いちばん低い暦月
    pub trough_month: Option<u32>,
    /// 山が年間平均の何倍か
    pub peak_ratio: Option<f64>,
    /// 何年ぶんの観測から出したか
    pub years: usize,
    /// 1 か月あたりの平均検索数。
    ///
    /// # なぜ持つのか
    /// 検索ボリュームは粗いきざみで報告される。検索数が少ない職種ほど
    /// きざみの影響が大きく、**季節の波が大きく見える**。実測では
    /// 月 50 回未満の 16 職種の peak_ratio が中央 1.28、月 1000 回以上の
    /// 20 職種では中央 1.10 で、データが良いほど波が小さい。
    /// 波を出してよいかの判断にこの値がいる。
    pub avg_monthly: f64,
}

impl TitleSeason {
    /// 月ラベルと値の並びから 1 職種ぶんを組み立てる。
    ///
    /// `months` と `values` は同じ長さで、同じ位置どうしが対応している前提。
    /// DB を経由せずに計算だけを確かめられるよう、ここを入口として分けてある。
    pub fn from_series(title: impl Into<String>, months: &[&str], values: &[Option<f64>]) -> Self {
        let index = calendar_index(months, values);
        let peak = highest(&index);
        let trough = lowest(&index);
        Self {
            title: title.into(),
            index,
            peak_month: peak.map(|(m, _)| m),
            trough_month: trough.map(|(m, _)| m),
            peak_ratio: peak.map(|(_, v)| v),
            years: years_of(months),
            avg_monthly: {
                let v: Vec<f64> = values
                    .iter()
                    .flatten()
                    .copied()
                    .filter(|x| x.is_finite())
                    .collect();
                if v.is_empty() {
                    0.0
                } else {
                    v.iter().sum::<f64>() / v.len() as f64
                }
            },
        }
    }
}

/// 暦月ごとの指数を出す。
///
/// `index[m - 1]` は「その暦月に当たる観測の平均 ÷ 全期間の平均」。
///
/// 欠測は平均に混ぜない。0 として足すと、たまたま取れなかった月が
/// 「その月は検索されない」に化ける。
///
/// 全期間の平均が 0 以下のときは比が意味を持たないので、12 か月とも `None` を返す。
/// 読めた観測が 1 つも無いときも同じ。
pub fn calendar_index(months: &[&str], values: &[Option<f64>]) -> [Option<f64>; 12] {
    let mut sums = [0.0f64; 12];
    let mut counts = [0usize; 12];
    let mut total = 0.0;
    let mut n = 0usize;

    for (label, value) in months.iter().zip(values.iter()) {
        let (Some(m), Some(v)) = (calendar_month(label), *value) else {
            continue;
        };
        if !v.is_finite() {
            continue;
        }
        let slot = (m - 1) as usize;
        sums[slot] += v;
        counts[slot] += 1;
        total += v;
        n += 1;
    }

    if n == 0 {
        return [None; 12];
    }
    let mean = total / n as f64;
    if mean <= 0.0 {
        return [None; 12];
    }

    let mut index = [None; 12];
    for ((slot, sum), count) in index.iter_mut().zip(sums.iter()).zip(counts.iter()) {
        if *count > 0 {
            *slot = Some((sum / *count as f64) / mean);
        }
    }
    index
}

/// カンマ区切りの並びを数字に開く。
///
/// 空の要素と数字として読めない要素は欠測（`None`）にする。
/// 2026-09-06 時点の実データに空要素は 1 つも無いが、元は外部サービスの
/// 推定値なので、欠けたときに 0 として混ざらないようにしておく。
pub fn parse_series(series: &str) -> Vec<Option<f64>> {
    series
        .split(',')
        .map(|s| {
            let s = s.trim();
            if s.is_empty() {
                return None;
            }
            s.parse::<f64>().ok().filter(|v| v.is_finite())
        })
        .collect()
}

/// 「2026-07」の形の月ラベルから暦月（1〜12）を取り出す。読めなければ `None`。
fn calendar_month(label: &str) -> Option<u32> {
    let m: u32 = label.get(5..7)?.parse().ok()?;
    if (1..=12).contains(&m) {
        Some(m)
    } else {
        None
    }
}

/// 何年ぶりの並びか。
///
/// 月の本数を 12 で割った切り捨て。端数の月は数えない。
/// 欠測があっても本数は減らないので、これは「何か月ぶんの並びか」であって
/// 「欠測なくそろっているか」ではない。
fn years_of(months: &[&str]) -> usize {
    months.len() / 12
}

/// 指数がいちばん高い暦月とその値。
///
/// 同じ高さが並んだときは早い月を採る。どちらを採るかで答えが変わるので、
/// 決め方を固定しておく（分析層を作る JS 側も先勝ちで揃えてある）。
fn highest(index: &[Option<f64>; 12]) -> Option<(u32, f64)> {
    let mut best: Option<(u32, f64)> = None;
    for (i, v) in index.iter().enumerate() {
        let Some(v) = *v else { continue };
        if best.is_none_or(|(_, b)| v > b) {
            best = Some((i as u32 + 1, v));
        }
    }
    best
}

/// 指数がいちばん低い暦月とその値。同点は早い月を採る（[`highest`] と同じ）。
fn lowest(index: &[Option<f64>; 12]) -> Option<(u32, f64)> {
    let mut worst: Option<(u32, f64)> = None;
    for (i, v) in index.iter().enumerate() {
        let Some(v) = *v else { continue };
        if worst.is_none_or(|(_, w)| v < w) {
            worst = Some((i as u32 + 1, v));
        }
    }
    worst
}

/// 職種ごとの季節性を読む。
///
/// 読むのは `variant='job'` だけ（理由はモジュールの説明を参照）。
///
/// # 月ラベルと数字の本数が合わない行は落とさずに失敗させる
/// 本数が違うと、どの数字がどの月のものか決められない。
/// 黙って捨てると職種が静かに減り、減ったことに誰も気づけない。
/// 分析層を作る側では月と値が同じ配列から出ているので、食い違いは
/// 配布物が壊れている印だと考えて、その場で止める。
pub fn load(db: &LocalDb) -> Result<Vec<TitleSeason>> {
    let rows = db
        .query(
            "SELECT norm_title, months, series FROM insight_search_trend \
             WHERE variant = 'job' ORDER BY norm_title",
            &[],
        )
        .map_err(|e| anyhow!(e))?;

    let mut out = Vec::with_capacity(rows.len());
    for r in &rows {
        let title = get_str(r, "norm_title");
        let months_raw = get_str(r, "months");
        let series_raw = get_str(r, "series");

        if months_raw.is_empty() || series_raw.is_empty() {
            return Err(anyhow!("{title}: 月の並びか数字の並びが空です"));
        }

        let months: Vec<&str> = months_raw.split(',').map(str::trim).collect();
        let values = parse_series(&series_raw);
        if months.len() != values.len() {
            return Err(anyhow!(
                "{title}: 月ラベルが {} 本に対して数字が {} 個あります。\
                 どの数字がどの月のものか決められません",
                months.len(),
                values.len()
            ));
        }

        out.push(TitleSeason::from_series(title, &months, &values));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::helpers::get_i64_opt;
    use std::collections::HashMap;
    use std::path::Path;

    const DB: &str = "data/indeed_insights.db";
    const GZ: &str = "data/indeed_insights.db.gz";

    /// 同梱の gz から実体を用意する。本番の起動時と同じ手順。
    ///
    /// 実体があるときに何もしないのは、`decompress_db_if_needed` が
    /// 「両方あれば実体を消して展開し直す」作りだから。
    /// 誰かが開いている実体を消さないよう、無いときだけ呼ぶ
    /// （`tests/indeed_data_test.rs` の `open_db` と同じ手順に合わせてある）。
    fn open_db() -> LocalDb {
        if !Path::new(DB).exists() {
            assert!(
                Path::new(GZ).exists(),
                "{GZ} がありません。Docker イメージに積む同梱物なので、\
                 消えているとデプロイしてもタブが空になります"
            );
            crate::decompress_db_if_needed(DB);
        }
        LocalDb::new(DB).expect("Indeed 分析 DB を開けませんでした")
    }

    /// 指数は「年間平均に対する比」なので、12 か月ならせば 1 に戻るはず。
    ///
    /// 割る相手を取り違えたり、欠測を 0 で埋めたりすると、ここがずれる。
    /// 暦月ごとの観測数が揃っていない年があれば 1 から少し外れるので、
    /// ぴったりは求めず ±0.02 で見る。
    #[test]
    fn 暦月の指数は平均するとほぼ1になる() {
        let seasons = load(&open_db()).expect("季節性を読めませんでした");
        assert!(!seasons.is_empty(), "1 件も返っていません");

        for s in &seasons {
            let vals: Vec<f64> = s.index.iter().flatten().copied().collect();
            assert_eq!(
                vals.len(),
                12,
                "{}: 指数が {} 個しかありません",
                s.title,
                vals.len()
            );
            let mean = vals.iter().sum::<f64>() / 12.0;
            assert!(
                (mean - 1.0).abs() <= 0.02,
                "{}: 指数の平均が {mean}（1.0 ± 0.02 のはず）",
                s.title
            );
        }
    }

    /// 山と谷が、表に入っている値と一致するか。
    ///
    /// 表の `peak_month` / `trough_month` は分析層を作る JS が別に計算したもの。
    /// 計算の仕方が違う可能性があるので全件一致は求めず、一致率を測って
    /// 8 割を下回ったら落とす。食い違った職種は名前と両方の値を出す
    /// （数だけ見ても、どちらが正しいのか調べようがない）。
    // 名前に含む「DB」が大文字なので snake case の注意が出る。表記を変えたくないので黙らせる
    #[allow(non_snake_case)]
    #[test]
    fn 山と谷がDBの値と一致する() {
        let db = open_db();
        let seasons = load(&db).expect("季節性を読めませんでした");

        let rows = db
            .query(
                "SELECT norm_title, peak_month, trough_month FROM insight_search_trend \
                 WHERE variant = 'job'",
                &[],
            )
            .expect("表を読めませんでした");
        let table: HashMap<String, (Option<i64>, Option<i64>)> = rows
            .iter()
            .map(|r| {
                (
                    get_str(r, "norm_title"),
                    (get_i64_opt(r, "peak_month"), get_i64_opt(r, "trough_month")),
                )
            })
            .collect();

        let mut checked = 0usize;
        let mut agreed = 0usize;
        let mut diffs: Vec<String> = Vec::new();

        for s in &seasons {
            // 表の側が空の職種は比べようがないので数に入れない
            let Some(&(Some(pm), Some(tm))) = table.get(&s.title) else {
                continue;
            };
            checked += 1;
            if s.peak_month == Some(pm as u32) && s.trough_month == Some(tm as u32) {
                agreed += 1;
            } else {
                diffs.push(format!(
                    "{}: 山 自前 {:?} / 表 {pm}、谷 自前 {:?} / 表 {tm}",
                    s.title, s.peak_month, s.trough_month
                ));
            }
        }

        assert!(checked > 0, "比べられる職種が 1 件もありませんでした");
        let rate = agreed as f64 / checked as f64 * 100.0;
        println!("山と谷の一致: {agreed}/{checked} = {rate:.1}%");
        for d in &diffs {
            println!("  食い違い {d}");
        }
        assert!(
            rate >= 80.0,
            "一致率 {rate:.1}% は低すぎます。どちらの計算が違うのか調べること"
        );
    }

    /// 欠測をその月の平均に 0 として混ぜないこと。
    ///
    /// 混ぜると、たまたま取れなかった月が「その月は検索されない」に化ける。
    /// DB を通さず、計算だけを取り出して確かめる。
    #[test]
    fn 欠測を0として扱わない() {
        // 2 年ぶん。全部同じ高さで、2 年目の 1 月だけ取れていない
        let months: Vec<String> = [2024, 2025]
            .iter()
            .flat_map(|y| (1..=12).map(move |m| format!("{y}-{m:02}")))
            .collect();
        let labels: Vec<&str> = months.iter().map(String::as_str).collect();
        let mut values = vec![Some(100.0); 24];
        values[12] = None; // 2025-01

        let index = calendar_index(&labels, &values);

        // 欠測を 0 と数えると 1 月だけ 0.5 付近まで落ちる。全部同じ高さのはず
        for (i, v) in index.iter().enumerate() {
            let v = v.unwrap_or_else(|| panic!("{} 月の指数が出ていません", i + 1));
            assert!(
                (v - 1.0).abs() < 1e-9,
                "{} 月の指数が {v}。欠測を 0 として数えていないか",
                i + 1
            );
        }

        // 文字列の側でも、空欄は 0 ではなく欠測として読む
        assert_eq!(
            parse_series("100,,100"),
            vec![Some(100.0), None, Some(100.0)]
        );
        assert_eq!(parse_series("100, ,x"), vec![Some(100.0), None, None]);
    }

    /// 返ってくる職種の数と、何年ぶんの観測かを固定する。
    ///
    /// ここが動いたら配布 DB が入れ替わったということなので、
    /// 数字を書き換える前に、増減が意図したものか確かめること。
    #[test]
    fn 職種数と年数() {
        let seasons = load(&open_db()).expect("季節性を読めませんでした");
        assert_eq!(
            seasons.len(),
            84,
            "variant='job' の職種数が変わりました（2026-09-06 時点は 84）"
        );
        for s in &seasons {
            assert_eq!(s.years, 4, "{}: {} 年ぶんになっています", s.title, s.years);
        }
    }

    /// 山の高さが、表に入っている `peak_ratio` とほぼ一致すること。
    ///
    /// 山の高さは自前の指数の最大値を使う。表の値と大きく離れるなら
    /// どちらかの計算が違うので、足し算の順序で出る程度の差 (1e-9) で見る。
    #[test]
    fn 山の高さが表の値とほぼ一致する() {
        let db = open_db();
        let seasons = load(&db).expect("季節性を読めませんでした");

        let rows = db
            .query(
                "SELECT norm_title, peak_ratio FROM insight_search_trend WHERE variant = 'job'",
                &[],
            )
            .expect("表を読めませんでした");
        let table: HashMap<String, Option<f64>> = rows
            .iter()
            .map(|r| {
                (
                    get_str(r, "norm_title"),
                    crate::handlers::helpers::get_f64_opt(r, "peak_ratio"),
                )
            })
            .collect();

        let mut worst = 0.0f64;
        let mut worst_title = String::new();
        for s in &seasons {
            let (Some(Some(want)), Some(got)) = (table.get(&s.title), s.peak_ratio) else {
                continue;
            };
            let d = (got - want).abs();
            if d > worst {
                worst = d;
                worst_title = format!("{}: 自前 {got} / 表 {want}", s.title);
            }
        }
        println!("山の高さのいちばん大きな差: {worst:e} {worst_title}");
        assert!(
            worst < 1e-9,
            "山の高さが表と食い違います（{worst_title}、差 {worst}）"
        );
    }
}

/// 全職種をならした季節の波。
///
/// # なぜ職種ごとではなく集約なのか
/// 職種ごとの波は、検索数が少ないほど大きく出る。実測では
///
///     月あたりの検索数   職種数   peak_ratio の中央
///          〜50           16          1.28
///        50〜100          17          1.26
///       100〜300          12          1.19
///       300〜1000         19          1.18
///       1000〜            20          1.10
///
/// と、**データが良いほど波が小さい**。これは季節の波ではなく、月ごとの
/// 報告のきざみの粗さを見ている。実際、施工管理技術者（月 7 回）は 48 か月の
/// 値が 0 か 10 しか無く、0 を除くと全月 1.00 になる。
///
/// 一方で職種をまたいでならすと、きざみのぶれが打ち消し合って
/// **3 月が山・12 月が谷**という形が残り、しかも検索数の多い職種に絞るほど
/// はっきりする（12 月が最小の職種は全体で 42%、月 1000 回以上では 65%）。
/// 集約だけを出すのはこのため。
pub fn overall(list: &[TitleSeason]) -> [Option<f64>; 12] {
    let mut sum = [0.0f64; 12];
    let mut cnt = [0usize; 12];
    for t in list {
        for (m, v) in t.index.iter().enumerate() {
            if let Some(x) = v {
                sum[m] += x;
                cnt[m] += 1;
            }
        }
    }
    let mut out = [None; 12];
    for ((o, s), c) in out.iter_mut().zip(sum.iter()).zip(cnt.iter()) {
        if *c > 0 {
            *o = Some(s / *c as f64);
        }
    }
    out
}
