//! Indeed 分析データの読み出しと集計を、実データで検証する。
//!
//! # なぜ「要素があること」を確かめないのか
//! 表が出ていることと、表の中身が正しいことは別。過去に「canvas がある」だけを
//! 見て、中身が空のチャートを 19 枚見逃した。ここでは成り立つべき関係
//! （足し算・割り算・範囲）を確かめる。壊れていれば数字が関係を満たさなくなる。
//!
//! # データが無ければ落とす
//! 同梱物 `data/indeed_insights.db.gz` はリポジトリに入っている。
//! 無ければ同梱が壊れているので、黙って飛ばさずに失敗させる。

use std::path::Path;

use rust_dashboard::db::local_sqlite::LocalDb;
use rust_dashboard::indeed::aggregate::{category_table, nation_overview, title_table};
use rust_dashboard::indeed::data::{load, Series, Snapshot};

const DB: &str = "data/indeed_insights.db";
const GZ: &str = "data/indeed_insights.db.gz";

/// 同梱の gz から実体を用意する。本番の起動時と同じ手順。
fn open_db() -> LocalDb {
    if !Path::new(DB).exists() {
        assert!(
            Path::new(GZ).exists(),
            "{GZ} がありません。Docker イメージに積むファイルなので、\
             消えているとデプロイしてもタブが空になります"
        );
        rust_dashboard::decompress_db_if_needed(DB);
    }
    LocalDb::new(DB).expect("Indeed 分析 DB を開けませんでした")
}

fn snap() -> Snapshot {
    load(&open_db()).expect("Indeed 分析 DB を読めませんでした")
}

/// 月の並びが全部そろっていること。
///
/// ここがずれると、傾向線が実際より急にも緩やかにもなる。
#[test]
fn 月の並びの長さが全部そろっている() {
    let s = snap();
    let n = s.n_months();
    assert!(n >= 12, "月が {n} 本しかありません（13 本前後のはず）");

    for (name, series) in &s.by_title {
        assert_eq!(series.job.len(), n, "{name} の求人数の並びが {n} 本でない");
        assert_eq!(series.ctk.len(), n, "{name} の見た人数の並びが {n} 本でない");
        assert_eq!(series.emp.len(), n, "{name} の企業数の並びが {n} 本でない");
    }
    for p in &s.by_pref {
        assert_eq!(
            p.series.job.len(),
            n,
            "{}／{} の並びが {n} 本でない",
            p.prefecture,
            p.title
        );
    }
}

/// 全国 = 分類ごとの合計。
///
/// 分類の割り当てが漏れると、定点表の合計が全体と合わなくなる。
#[test]
fn 全国は分類の合計と一致する() {
    let s = snap();
    let n = s.n_months();
    for i in 0..n {
        let by_cat: f64 = s
            .by_category
            .values()
            .filter_map(|c| c.job.get(i).copied().flatten())
            .sum();
        let nation = s.nation.job.get(i).copied().flatten().unwrap_or(0.0);
        assert!(
            (by_cat - nation).abs() < 1.0,
            "{} 月目: 分類の合計 {by_cat} と全国 {nation} が食い違う",
            i + 1
        );
    }
}

/// 分類に入る職種の数を足すと、職種の総数になる。
#[test]
fn 分類の職種数の合計が職種の総数と一致する() {
    let s = snap();
    let sum: usize = s.category_titles.values().map(|v| v.len()).sum();
    assert_eq!(
        sum,
        s.titles.len(),
        "分類に割り当てた職種の数 {sum} と職種の総数 {} が食い違う",
        s.titles.len()
    );
}

/// 1 求人あたり = 見た人数 ÷ 求人数。
///
/// この割り算が崩れると、レポートの「なぜ」の説明が根拠を失う。
#[test]
fn 一求人あたりは見た人数を求人数で割ったものである() {
    let s = snap();
    let spp = s.nation.seekers_per_posting();
    for i in 0..s.n_months() {
        let (Some(j), Some(c), Some(v)) = (
            s.nation.job[i],
            s.nation.ctk[i],
            spp.get(i).copied().flatten(),
        ) else {
            continue;
        };
        let expect = c / j;
        assert!(
            (v - expect).abs() < 1e-9,
            "{} 月目: 1 求人あたり {v} が {c} ÷ {j} = {expect} と合わない",
            i + 1
        );
    }
}

/// 欠測を 0 で埋めていないこと。
///
/// 0 件と「取れていない」を混ぜると、傾向が実際より急に見える。
#[test]
fn 欠測は合計に数えられていない() {
    let all_none: Vec<Option<f64>> = vec![None; 5];
    assert_eq!(
        Series::total(&all_none),
        None,
        "全部欠測なのに合計が出てしまっている"
    );
    let mixed = vec![None, Some(2.0), None, Some(3.0)];
    assert_eq!(Series::total(&mixed), Some(5.0), "欠測を飛ばして足せていない");
}

/// 変化率が現実的な範囲に収まっていること。
///
/// 単位のずれ（比率と % の取り違え）は、100 倍の値になって表に出る。
/// 過去に 2 度起きているので、範囲そのものを固定する。
#[test]
fn 変化率が現実的な範囲に収まっている() {
    let s = snap();
    for row in category_table(&s) {
        if let Some(v) = row.job_change_pct {
            assert!(
                v > -100.0 && v < 1000.0,
                "{} の求人数の変化が {v}% と現実的でない（単位のずれの疑い）",
                row.name
            );
        }
        if let Some(v) = row.spp_latest {
            assert!(
                v >= 0.0 && v < 100_000.0,
                "{} の 1 求人あたりが {v} と現実的でない",
                row.name
            );
        }
    }
}

/// 一本調子でないものを「毎月ずつ」と書いていないこと。
///
/// 平均だけを見て「毎月およそ X% ずつ増えています」と書き、
/// 実際は月ごとの振れの方が大きい、という誤りが 1,138 行で出た。
#[test]
fn 振れが大きいものを毎月ずつとは書いていない() {
    let s = snap();
    let rows = title_table(&s);
    assert!(!rows.is_empty(), "職種が 1 つも取れていません");

    // 一本調子のときだけ出てよい言い回し。
    // 「毎月の動き（約 X%）より大きく」は逆に「一本調子でない」説明の中に出るので、
    // 「毎月」だけを見ると取り違える。実際に一度取り違えた。
    const STEADY_ONLY: &str = "毎月およそ";

    let (mut steady, mut wobbly) = (0, 0);
    for r in &rows {
        if r.steady {
            steady += 1;
        } else {
            wobbly += 1;
            assert!(
                !r.short.contains(STEADY_ONLY),
                "{} は一本調子でないのに「{}」と書いている",
                r.name,
                r.short
            );
            assert!(
                r.short.contains("一本調子ではありません")
                    || r.short.contains("向きは定まりません")
                    || r.short.contains("比べられるだけの月数がありません"),
                "{} は一本調子でないのに、そう読める断りが無い: {}",
                r.name,
                r.short
            );
        }
    }
    // 逆も確かめる。全部を「一本調子でない」にしてしまえばこのテストは通ってしまう。
    assert!(
        wobbly > 0,
        "一本調子でない職種が 1 つも無い。判定が働いていない疑いがある"
    );
    assert!(
        steady > 0,
        "一本調子の職種が 1 つも無い。全部を「振れが大きい」に倒していないか"
    );
}

/// 全体の「なぜ」の文が、実際の数字と向きを揃えていること。
#[test]
fn なぜの説明が数字と向きを揃えている() {
    let s = snap();
    let ov = nation_overview(&s);
    let why = ov.why();
    assert!(!why.is_empty(), "説明が空です");

    // 増えているのに「減った」と書いていないか
    if let Some(v) = ov.ctk.change_pct {
        let says_up = why.contains(&format!("{:+.1}%", v));
        assert!(
            says_up,
            "見た人数の変化 {v:+.1}% が説明文に出てこない: {why}"
        );
    }
    assert!(
        why.contains("求人を見た人数") && why.contains("求人の数"),
        "分解の内訳が説明に出ていない: {why}"
    );
}

/// 出どころと但し書きが空でないこと。
///
/// 顧客に配る紙にこれが載らないのは、数字が間違っているのと同じくらい困る。
#[test]
fn 出どころと但し書きがある() {
    let s = snap();
    assert!(!s.meta.source.is_empty(), "出どころが空です");
    assert!(!s.meta.caveat.is_empty(), "読むときの注意が空です");
    assert!(
        s.meta.caveat.contains("応募") || s.meta.caveat.contains("クリック"),
        "「クリックは応募ではない」という注意が入っていない: {}",
        s.meta.caveat
    );
}

/// 見出しの数字と、本文の数字が一致すること。
///
/// 顧客レポートで「求人の数はこの期間で 26% 増えました」という文の隣に、
/// 見出しの数字として「+22.3%」が出ていた。前者は直線に沿った変化、
/// 後者は最後÷最初で、別の計算だった。読む人には同じ指標に見える。
#[test]
fn 見出しの変化率と本文の変化率が一致する() {
    let s = snap();
    let ov = nation_overview(&s);

    for m in [&ov.job, &ov.ctk, &ov.emp, &ov.spp] {
        let Some(pct) = m.change_pct else { continue };
        // 文章は小数を落とした整数で「X% 増えました／減りました」と書く
        let expect = format!("{:.0}%", pct.abs());
        assert!(
            m.sentence.contains(&expect),
            "{} の見出しは {:+.1}% なのに、本文にその数字（{}）が出てこない: {}",
            m.label,
            pct,
            expect,
            m.sentence
        );
        // 向きも一致すること
        let says_up = m.sentence.contains("増え") || m.sentence.contains("集まりやすく");
        let says_down = m.sentence.contains("減り") || m.sentence.contains("集まりにくく");
        if pct > 0.5 {
            assert!(
                says_up && !says_down,
                "{} は {:+.1}% なのに本文が減ったと読める: {}",
                m.label,
                pct,
                m.sentence
            );
        } else if pct < -0.5 {
            assert!(
                says_down && !says_up,
                "{} は {:+.1}% なのに本文が増えたと読める: {}",
                m.label,
                pct,
                m.sentence
            );
        }
    }
}

/// 全国の合計が、元の表を直接足したものと一致すること。
///
/// # なぜ内輪の突き合わせでは足りないか
/// 「全国 = 分類の合計」は、両方が同じ理由で同じだけ欠けていても通る。
/// 実際に通っていた。insight_title（最新月にデータがある職種の名簿）だけを
/// 回して集計していたため、最新月に落ちた職種の過去の数字が全国から抜け、
/// 2025-11 の求人数が 58 件少なく出ていた。元の表と突き合わせる。
#[test]
fn 全国の合計が元の表と一致する() {
    let db = open_db();
    let s = load(&db).expect("読み込み");

    let rows = db
        .query(
            "SELECT report_month, SUM(job_count) AS job, SUM(ctk_count) AS ctk \
             FROM insight_title_pref GROUP BY report_month",
            &[],
        )
        .expect("元の表を読む");

    let mut checked = 0;
    for r in &rows {
        let m = r
            .get("report_month")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let Some(i) = s.meta.months.iter().position(|x| *x == m) else {
            continue;
        };
        for (key, got, name) in [
            ("job", s.nation.job[i], "求人数"),
            ("ctk", s.nation.ctk[i], "見た人数"),
        ] {
            let want = r.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0);
            let got = got.unwrap_or(0.0);
            assert!(
                (want - got).abs() < 1.0,
                "{m} の{name}: 元の表は {want} なのに全国は {got}（差 {}）",
                want - got
            );
        }
        checked += 1;
    }
    assert!(checked >= 12, "突き合わせた月が {checked} しかありません");
}

/// 傾向線が引けないときは、変化率も出さないこと。
///
/// 2 点しかない並びから「最後 ÷ 最初」を出すと、文章が
/// 「比べられるだけの月数がありません」と言っている隣に
/// -56.9% のような具体的な数字が並び、読む人が混乱する。
#[test]
fn 傾向線が引けないときは変化率も出さない() {
    let s = snap();
    let months = &s.meta.months;

    let mut none_fit = 0;
    for t in &s.titles {
        let Some(series) = s.by_title.get(&t.name) else {
            continue;
        };
        let ov = rust_dashboard::indeed::aggregate::Overview::from_series(&t.name, series, months);
        for m in [&ov.job, &ov.ctk, &ov.emp, &ov.spp, &ov.ppe] {
            if m.fit.is_none() {
                none_fit += 1;
                assert!(
                    m.change_pct.is_none(),
                    "{} の{}は傾向線が引けないのに変化率 {:?} を出している",
                    t.name,
                    m.label,
                    m.change_pct
                );
            }
        }
    }
    // 「1 つも該当が無いから通った」ではないことを示す。
    // 全部に傾向線が引けているなら、それはそれで確かめたい事実なので数を出す。
    println!("傾向線が引けなかった指標: {none_fit} 件");
}
