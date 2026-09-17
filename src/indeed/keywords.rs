//! 検索語まわりのデータ。職種 1 つぶんを引く。
//!
//! # 2 つの表は別のものを見ている
//! * [`term_shifts`] … 語ごとのシェアが、期間の頭と終わりでどう違ったか
//! * [`attr_months`] … 探している人の属性の割合が、月ごとにどう並んでいるか
//!
//! # これは期間比較であって、傾向ではない
//! `insight_kw_term_shift` の前後は、作り（`scripts/indeed_build_insights.js`）を読むと
//! こうなっている:
//!
//! ```text
//! const a = win(fullMonths.slice(0, WIN));   // 分析に使う月の「最初」の 3 か月
//! const b = win(fullMonths.slice(-WIN));     // 同じく「最後」の 3 か月
//! diffs.push({ s, sa, sb, d: sb - sa, ca: b.sum.get(s) || 0 });
//! ```
//!
//! `fullMonths` は昇順なので、比べているのは **収録範囲の先頭 3 か月と末尾 3 か月**。
//! 2026-09 時点のデータでは 2025-07〜09 と 2026-06〜08 にあたり、およそ 1 年離れている。
//! 直前の四半期との比較ではないし、間の月は一切見ていない。
//! したがって「じわじわ増えた」のか「途中で跳ねて戻った」のかは、この表からは言えない。
//! 差が大きいことは分かるが、その差が続くかどうかは分からない。
//!
//! # 全部の語が入っているわけではない
//! 収録は、どちらかの期間で 1% 以上あった語のうち、差の絶対値が大きい上位 14 語だけ。
//! 実データでも 1 職種あたり 7〜14 行しかない。「載っていない語は動かなかった」ではなく
//! 「載っていない語は、動きが上位 14 語に届かなかったか、そもそも 1% に満たなかった」。
//!
//! # 属性の割合は足しても 100% にならない
//! 8 つは互いに排他ではなく、それぞれ独立に判定している。1 つの語が「シニア」と
//! 「未経験」の両方に数えられることがある。実データで 8 つの合計を取ると
//! 平均 39.4%、最大 104.2%（1,477 行中 9 行が 100% 超）。円グラフの材料にはならない。

use anyhow::{anyhow, Result};

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64_opt, get_str};

/// 検索語の入れ替わり 1 行ぶん。
///
/// 割合はすべてポイント（%）。`diff_pt` は `after_pct - before_pct` で、
/// 作り手側で引き算済みの値をそのまま持つ（自分で引き直すと丸めがずれる）。
#[derive(Debug, Clone)]
pub struct TermShift {
    pub term: String,
    pub before_pct: Option<f64>,
    pub after_pct: Option<f64>,
    pub diff_pt: Option<f64>,
    /// 後半 3 か月ぶんのクリック合計。月あたりではない。
    /// DB では整数だが、画面側で割合と一緒に扱えるよう f64 で渡す
    /// （実データの最大は 827,961 で、f64 で正確に表せる範囲）
    pub clicks_after: Option<f64>,
}

/// 属性の内訳 1 か月ぶん。
#[derive(Debug, Clone)]
pub struct AttrMonth {
    /// "YYYY-MM"
    pub month: String,
    /// 8 属性の割合(%)。並びは [`ATTR_LABELS`] と同じ
    pub pct: [Option<f64>; 8],
    /// この月の語数。少ないと割合が大きく振れる
    ///
    /// # なぜこれを画面まで持って行くか
    /// Indeed は県ごとに上位 10 語しか返さない。語数が薄い職種では、割合が
    /// 「実際に減った」のか「上位 10 語から落ちて見えなくなった」のかを区別できない。
    ///
    /// 実データで確かめた。職種ごとに、月をまたいだ `pct_senior` の最大−最小を取り、
    /// その職種の `term_count` の中央値で分けると:
    ///
    /// * 中央値 20 未満（7 職種）… 振れ幅の中央値 12.2pt
    /// * 中央値 60 以上（52 職種）… 同 3.1pt
    ///
    /// 薄いほうの標本が 7 職種しかないので、境目の 20 や 60 という数字に
    /// 強い根拠があるわけではない。ただ「薄い月ほど大きく振れる」向きは出ている。
    /// 何語未満を落とすかは、この値を見て画面側で決める。
    pub term_count: Option<f64>,
}

/// 属性の表示名。順序は [`AttrMonth::pct`] と対応する
///
/// DB の列順（`pct_condition, pct_senior, pct_homemaker, pct_student,
/// pct_foreign, pct_inexperienced, pct_language, pct_qualified`）と同じ並び。
/// 言い回しは [`super::detail::Attrs::shares`] に合わせてある。同じ数字が
/// 画面によって違う名前で出ると、読む側は別の指標だと思ってしまう。
pub const ATTR_LABELS: [&str; 8] = [
    "働き方の条件",
    "シニア",
    "主婦・主夫",
    "学生",
    "外国人",
    "未経験",
    "語学",
    "資格",
];

/// [`AttrMonth::pct`] に詰める順の DB 列名。[`ATTR_LABELS`] と 1 対 1。
const ATTR_COLUMNS: [&str; 8] = [
    "pct_condition",
    "pct_senior",
    "pct_homemaker",
    "pct_student",
    "pct_foreign",
    "pct_inexperienced",
    "pct_language",
    "pct_qualified",
];

/// 検索語の入れ替わりを、増えた語から順に返す。
///
/// 職種が無ければ空を返す。エラーにはしない。属性のほうにしか載っていない職種が
/// 実データで 21 件あり、「引けなかった」と「そもそも無い」を呼び分けても
/// 画面側ですることは同じ（その節を出さない）だった。
///
/// 並べ替えは SQL ではなく手元でやる。SQLite の `ORDER BY ... DESC` は NULL を
/// 最後に置くが、その挙動に寄りかかると、後で並びを変えたときに欠測の居場所が
/// 黙って動く。欠測は「増えても減ってもいない」ではないので、末尾に固定する。
pub fn term_shifts(db: &LocalDb, title: &str) -> Result<Vec<TermShift>> {
    let rows = db
        .query(
            "SELECT search_term, share_before, share_after, share_diff, clicks_after \
             FROM insight_kw_term_shift WHERE norm_title = ?1",
            &[&title],
        )
        .map_err(|e| anyhow!("検索語の入れ替わりを読めませんでした（{title}）: {e}"))?;

    let mut out: Vec<TermShift> = rows
        .iter()
        .map(|r| TermShift {
            term: get_str(r, "search_term"),
            before_pct: get_f64_opt(r, "share_before"),
            after_pct: get_f64_opt(r, "share_after"),
            diff_pt: get_f64_opt(r, "share_diff"),
            clicks_after: get_f64_opt(r, "clicks_after"),
        })
        .collect();

    // 増えた語が先。欠測は 0 とみなさず末尾へ
    out.sort_by(|a, b| {
        b.diff_pt
            .unwrap_or(f64::NEG_INFINITY)
            .total_cmp(&a.diff_pt.unwrap_or(f64::NEG_INFINITY))
    });
    Ok(out)
}

/// 属性の内訳を月の昇順で返す。
///
/// 職種が無ければ空を返す。
///
/// 月は "YYYY-MM" の固定幅なので、文字列として並べれば時系列の順になる。
/// 日付に直さずに `ORDER BY report_month` で済ませているのはそのため。
/// 桁が変わる形式（"2026-9" など）が混ざったら、この前提は崩れる。
pub fn attr_months(db: &LocalDb, title: &str) -> Result<Vec<AttrMonth>> {
    let rows = db
        .query(
            "SELECT report_month, term_count, pct_condition, pct_senior, pct_homemaker, \
                    pct_student, pct_foreign, pct_inexperienced, pct_language, pct_qualified \
             FROM insight_kw_attr_trend WHERE norm_title = ?1 \
             ORDER BY report_month ASC",
            &[&title],
        )
        .map_err(|e| anyhow!("属性の内訳を読めませんでした（{title}）: {e}"))?;

    Ok(rows
        .iter()
        .map(|r| {
            let mut pct = [None; 8];
            for (slot, col) in pct.iter_mut().zip(ATTR_COLUMNS) {
                *slot = get_f64_opt(r, col);
            }
            AttrMonth {
                month: get_str(r, "report_month"),
                pct,
                term_count: get_f64_opt(r, "term_count"),
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    const DB: &str = "data/indeed_insights.db";
    const GZ: &str = "data/indeed_insights.db.gz";

    /// 同梱の gz から実体を用意する。本番の起動時と同じ手順。
    ///
    /// 実体があるときは触らない。`decompress_db_if_needed` は両方あると
    /// 既存の実体を消してから解凍し直すので、無条件に呼ぶと同じ DB を
    /// 見ている他のテストの足元を抜くことになる。
    fn open_db() -> LocalDb {
        assert!(
            Path::new(GZ).exists(),
            "{GZ} がありません。Docker イメージに積む同梱物なので、\n             消えているとデプロイしてもタブが空になります"
        );
        // 存在確認ごとロックの中でやる。外で確かめると、
        // 別スレッドが書いている途中のファイルを「在る」と見てしまう
        crate::ensure_db_from_gz(DB);
        LocalDb::new(DB).expect("Indeed 分析 DB を開けませんでした")
    }

    /// その表に載っている職種を全部。1 職種だけ見て通すと、
    /// たまたま素直な職種を引いていただけということが起きる。
    fn titles(db: &LocalDb, table: &str) -> Vec<String> {
        let rows = db
            .query(
                &format!("SELECT DISTINCT norm_title FROM {table} ORDER BY norm_title"),
                &[],
            )
            .expect("職種名を読めませんでした");
        assert!(!rows.is_empty(), "{table} に職種が 1 つもありません");
        rows.iter().map(|r| get_str(r, "norm_title")).collect()
    }

    #[test]
    fn 検索語の入れ替わりは増えた順に返る() {
        let db = open_db();
        let mut checked = 0;
        for t in titles(&db, "insight_kw_term_shift") {
            let rows = term_shifts(&db, &t).expect("読めませんでした");
            assert!(!rows.is_empty(), "{t} が空になりました");
            for w in rows.windows(2) {
                let a = w[0].diff_pt.unwrap_or(f64::NEG_INFINITY);
                let b = w[1].diff_pt.unwrap_or(f64::NEG_INFINITY);
                assert!(
                    a >= b,
                    "{t}: {} ({a}) の後ろに {} ({b}) が来ています",
                    w[0].term,
                    w[1].term
                );
            }
            checked += 1;
        }
        assert!(
            checked >= 100,
            "{checked} 職種しか見ていません（104 のはず）"
        );
    }

    #[test]
    fn 入れ替わりの前後シェアと差が整合する() {
        let db = open_db();
        // 引き算をやり直すのではなく、作り手側の diff が前後と食い違わないかを見る。
        // 食い違えば、どちらかの列が別の集計で埋まっている
        let mut worst = 0.0_f64;
        let mut worst_where = String::new();
        let mut checked = 0;
        for t in titles(&db, "insight_kw_term_shift") {
            for r in term_shifts(&db, &t).expect("読めませんでした") {
                let (Some(b), Some(a), Some(d)) = (r.before_pct, r.after_pct, r.diff_pt) else {
                    // 欠測はここでは判定しない。0 に潰していないことは
                    // 別のテスト（母数の薄い月）と型（Option）で担保する
                    continue;
                };
                let err = (a - b - d).abs();
                if err > worst {
                    worst = err;
                    worst_where = format!("{t} / {}", r.term);
                }
                checked += 1;
            }
        }
        assert!(
            checked > 1_000,
            "{checked} 行しか見ていません（1,378 のはず）"
        );
        assert!(
            worst <= 0.1,
            "after - before と diff が {worst}pt ずれています（{worst_where}）"
        );
    }

    #[test]
    fn 属性は月の昇順で返る() {
        let db = open_db();
        let mut checked = 0;
        for t in titles(&db, "insight_kw_attr_trend") {
            let rows = attr_months(&db, &t).expect("読めませんでした");
            assert!(!rows.is_empty(), "{t} が空になりました");
            for w in rows.windows(2) {
                assert!(
                    w[0].month < w[1].month,
                    "{t}: {} の後ろに {} が来ています",
                    w[0].month,
                    w[1].month
                );
            }
            // 並びの前提（"YYYY-MM" の固定幅）が崩れていないことも一緒に見る
            for r in &rows {
                assert_eq!(r.month.len(), 7, "{t}: 月の形が違います（{}）", r.month);
            }
            checked += 1;
        }
        assert!(
            checked >= 120,
            "{checked} 職種しか見ていません（125 のはず）"
        );
    }

    #[test]
    fn 属性の割合は0から100の範囲に収まる() {
        let db = open_db();
        // 値が 1 度も入らない枠があれば、列名の綴りが DB と食い違っている。
        // 名前で引いている以上、綴りを外しても例外は出ず全部 None になるだけなので、
        // 範囲を見るついでに「そもそも埋まっているか」を数える
        let mut filled = [0_usize; 8];
        for t in titles(&db, "insight_kw_attr_trend") {
            for r in attr_months(&db, &t).expect("読めませんでした") {
                for (i, v) in r.pct.iter().enumerate() {
                    let Some(v) = v else { continue };
                    filled[i] += 1;
                    assert!(
                        (0.0..=100.0).contains(v),
                        "{t} / {} の{}が {v}% です",
                        r.month,
                        ATTR_LABELS[i]
                    );
                }
            }
        }
        for (i, n) in filled.iter().enumerate() {
            assert!(
                *n > 0,
                "{}（{}）に値が 1 つも入りませんでした。列名が DB と合っていません",
                ATTR_LABELS[i],
                ATTR_COLUMNS[i]
            );
        }
    }

    #[test]
    fn 母数の薄い月が識別できる() {
        let db = open_db();
        // 母数が取れていなければ、画面側は薄い月を落とす判断ができない。
        // 「1 行でもある」ではなく「大半の行にある」ことを見る
        let mut with = 0;
        let mut total = 0;
        let mut thinnest = f64::INFINITY;
        for t in titles(&db, "insight_kw_attr_trend") {
            for r in attr_months(&db, &t).expect("読めませんでした") {
                total += 1;
                if let Some(n) = r.term_count {
                    with += 1;
                    assert!(n > 0.0, "{t} / {} の語数が {n} です", r.month);
                    thinnest = thinnest.min(n);
                }
            }
        }
        assert!(total > 1_000, "{total} 行しか見ていません（1,477 のはず）");
        assert!(
            with * 10 >= total * 9,
            "語数が入っているのは {with}/{total} 行しかありません"
        );
        // 薄い月が実際に混ざっていること。全部が厚いなら、
        // 画面側で落とす仕掛けは要らないことになる
        assert!(
            thinnest < 20.0,
            "いちばん薄い月でも語数 {thinnest} で、落とす対象がありません"
        );
    }

    #[test]
    fn 存在しない職種は空を返す() {
        let db = open_db();
        // 職種の綴りが変わったときに、画面を落とさず節を消すだけで済ませたい
        for t in [
            "",
            "そんな職種はない",
            "'; DROP TABLE insight_kw_term_shift; --",
        ] {
            assert!(
                term_shifts(&db, t)
                    .expect("エラーにしてはいけません")
                    .is_empty(),
                "{t} で入れ替わりが返りました"
            );
            assert!(
                attr_months(&db, t)
                    .expect("エラーにしてはいけません")
                    .is_empty(),
                "{t} で属性が返りました"
            );
        }
    }
}

/// 属性の内訳を、期間の頭と終わりで比べた結果。
pub struct AttrChange {
    /// 最初の 3 か月の平均（%）
    pub before: [Option<f64>; 8],
    /// 直近 3 か月の平均（%）
    pub after: [Option<f64>; 8],
    /// 母数（語数）の中央値。小さいほど割合が振れる
    pub term_count_median: Option<f64>,
    /// 平均に使った月数（頭・終わりとも同じ）
    pub window: usize,
}

/// 頭と終わりの 3 か月を平均して比べる。
///
/// # なぜ最初の月と最新の月を直接比べないのか
/// 月ごとの割合はよく振れる。実測では、母数（語数）が 20 未満の職種で
/// シニア比率の振れ幅が中央 12.2pt、20〜60 で 4.6pt、60 以上で 3.1pt だった。
/// 端の 1 か月だけを取ると、たまたま高い月と低い月を選んだだけで
/// 大きな変化に見えてしまう。実際「研磨作業のシニアが 19.5% から 32.0% へ」は
/// 端点どうしの比較で、途中は 14.0〜23.5 を行き来している。
/// 3 か月ならすと、この職種は 17.8% → 27.0% になる。
///
/// 検索語の入れ替わり（`insight_kw_term_shift`）も生成側で同じ作りにしてあるので、
/// 2 つの図の「前」と「後」の意味がそろう。
pub fn attr_change(months: &[AttrMonth]) -> Option<AttrChange> {
    let window = 3usize;
    if months.len() < window * 2 {
        return None;
    }
    let mean_of = |slice: &[AttrMonth], i: usize| -> Option<f64> {
        let v: Vec<f64> = slice
            .iter()
            .filter_map(|m| m.pct[i])
            .filter(|x| x.is_finite())
            .collect();
        if v.is_empty() {
            None
        } else {
            Some(v.iter().sum::<f64>() / v.len() as f64)
        }
    };
    let head = &months[..window];
    let tail = &months[months.len() - window..];
    let mut before = [None; 8];
    let mut after = [None; 8];
    for i in 0..8 {
        before[i] = mean_of(head, i);
        after[i] = mean_of(tail, i);
    }
    let mut tc: Vec<f64> = months
        .iter()
        .filter_map(|m| m.term_count)
        .filter(|x| x.is_finite())
        .collect();
    tc.sort_by(|a, b| a.total_cmp(b));
    let term_count_median = if tc.is_empty() {
        None
    } else {
        Some(tc[tc.len() / 2])
    };
    Some(AttrChange {
        before,
        after,
        term_count_median,
        window,
    })
}

/// 検索語 1 つぶんの月次シェア。
pub struct TermSeries {
    pub term: String,
    /// 月ごとのシェア(%)。月の並びは [`term_monthly`] が返す months と対応する
    pub pct: Vec<Option<f64>>,
    /// 直近で値のある月のシェア。並べ替えに使う
    pub latest: Option<f64>,
    /// 期間の頭と終わりの差（ポイント）
    pub diff_pt: Option<f64>,
}

/// 語ごとのシェアを月次で読む。
///
/// # なぜ 2 期間の比較（[`term_shifts`]）と別に持つのか
/// 前 3 か月と直近 3 か月の平均どうしでは 2 点しか無く、
/// 「いつから動いたのか」「まだ続いているのか」が読めない。
/// 実データの「事務」は 45.3 → 46.4 → 46.3 → 44.8 → 41.4 → 40.8 → 34.9 →
/// 36.6 → 37.9 → 35.7 → 35.8 → 31.9 → 29.8 → 26.8 と、2025-09 から下がり続けている。
/// 2 点に丸めると「46.1 → 29.8」としか見えず、動きが続いているのかが分からない。
///
/// # どの語まで出すか
/// 生成側で「どの月でも 1% に届かない語」を落としてある。
/// Indeed は県ごとに上位 10 語しか返さないため、小さい語は
/// 「消えた」のか「圏外に落ちた」のか区別できない。
pub fn term_monthly(db: &LocalDb, title: &str) -> Result<(Vec<String>, Vec<TermSeries>)> {
    let rows = db
        .query(
            "SELECT report_month, search_term, share_pct              FROM insight_kw_term_monthly WHERE norm_title = ?1              ORDER BY report_month, search_term",
            &[&title],
        )
        .map_err(|e| anyhow!("検索語の月次を読めませんでした（{title}）: {e}"))?;
    if rows.is_empty() {
        return Ok((Vec::new(), Vec::new()));
    }
    let mut months: Vec<String> = Vec::new();
    let mut by: std::collections::HashMap<String, std::collections::HashMap<String, f64>> =
        std::collections::HashMap::new();
    for r in &rows {
        let m = get_str(r, "report_month");
        if !months.contains(&m) {
            months.push(m.clone());
        }
        let t = get_str(r, "search_term");
        if let Some(v) = get_f64_opt(r, "share_pct") {
            by.entry(t).or_default().insert(m, v);
        }
    }
    months.sort();
    let mut out: Vec<TermSeries> = by
        .into_iter()
        .map(|(term, m)| {
            let pct: Vec<Option<f64>> = months.iter().map(|x| m.get(x).copied()).collect();
            let first = pct.iter().flatten().next().copied();
            let latest = pct.iter().flatten().next_back().copied();
            TermSeries {
                term,
                diff_pt: match (first, latest) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None,
                },
                latest,
                pct,
            }
        })
        .collect();
    // 直近のシェアが大きい順。画面では上から数本だけ出す
    out.sort_by(|a, b| b.latest.unwrap_or(0.0).total_cmp(&a.latest.unwrap_or(0.0)));
    Ok((months, out))
}
