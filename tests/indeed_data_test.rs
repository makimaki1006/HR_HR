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

/// 職種詳細で出す「順位」と「全国比」の意味を固定する。
///
/// どちらも名前から意味が読めない。作り（scripts/indeed_build_insights.js）は
///   rows.sort((a,b) => b.seekers_per_posting - a.seekers_per_posting)  // 降順
///   rank = i + 1 / vs_national = その県の1求人あたり ÷ 全国の1求人あたり
/// なので、順位は「1求人あたりが多い順（1位＝いちばん集まりやすい）」、
/// 全国比は「全国平均を1とした比（差ではない）」。
/// 画面にはこの説明を書いて出している。ここが変わったら説明も直す必要がある。
#[test]
fn 職種詳細の順位と全国比が定義どおりである() {
    let db = open_db();
    let d = rust_dashboard::indeed::detail::load(&db, "配送ドライバー")
        .expect("読み込み")
        .expect("配送ドライバーが見つからない");

    assert!(!d.prefs.is_empty(), "都道府県の行が空です");

    // 1) 順位は 1..=比べた数 に収まる
    for p in &d.prefs {
        if let (Some(r), Some(of)) = (p.rank, p.of) {
            assert!(
                r >= 1 && r <= of,
                "{} の順位 {r} が 1〜{of} の外にある",
                p.prefecture
            );
        }
    }

    // 2) 順位が小さいほど 1 求人あたりが大きい（＝集まりやすい）
    let mut ranked: Vec<_> = d
        .prefs
        .iter()
        .filter(|p| p.rank.is_some() && p.spp.is_some())
        .collect();
    ranked.sort_by_key(|p| p.rank.unwrap());
    for w in ranked.windows(2) {
        let (a, b) = (&w[0], &w[1]);
        assert!(
            a.spp.unwrap() >= b.spp.unwrap() - 1e-9,
            "順位 {} の {}（{:.2}）より、順位 {} の {}（{:.2}）のほうが多い。\
             順位の向きが逆になっている",
            a.rank.unwrap(),
            a.prefecture,
            a.spp.unwrap(),
            b.rank.unwrap(),
            b.prefecture,
            b.spp.unwrap()
        );
    }

    // 3) 全国比は「比」。差なら 0 をまたぐが、比は必ず正になる
    for p in &d.prefs {
        if let Some(v) = p.vs_national {
            assert!(
                v > 0.0 && v < 20.0,
                "{} の全国比 {v} が比として現実的でない（差と取り違えていないか）",
                p.prefecture
            );
        }
    }
    // 全部が 1.0 付近に固まっていたら、比ではなく別のものを見ている疑い
    let spread = {
        let vs: Vec<f64> = d.prefs.iter().filter_map(|p| p.vs_national).collect();
        let (mn, mx) = vs.iter().fold((f64::MAX, f64::MIN), |(a, b), v| (a.min(*v), b.max(*v)));
        mx - mn
    };
    assert!(spread > 0.05, "全国比の幅が {spread} しかない。比になっていない疑い");

    // 4) 1 求人あたり = 見た人数 ÷ 求人数
    for p in &d.prefs {
        let (Some(j), Some(c), Some(s)) = (p.job, p.ctk, p.spp) else {
            continue;
        };
        if j <= 0.0 {
            continue;
        }
        assert!(
            (s - c / j).abs() < 0.05,
            "{} の 1 求人あたり {s} が {c} ÷ {j} と合わない",
            p.prefecture
        );
    }
}

/// 探している人の傾向は、足しても 100% にならないこと。
///
/// 1 つの語が複数に当てはまる（「主婦 未経験」など）ため。
/// 画面にもそう書いている。もし排他になったら説明を直す必要がある。
#[test]
fn 属性の割合は足しても百にならない() {
    let db = open_db();
    let d = rust_dashboard::indeed::detail::load(&db, "配送ドライバー")
        .expect("読み込み")
        .expect("見つからない");
    let a = d.attrs.expect("属性が無い");
    let sum: f64 = a.shares.iter().filter_map(|(_, v)| *v).sum();
    assert!(sum > 0.0, "属性がすべて空です");
    assert!(
        (sum - 100.0).abs() > 1.0,
        "足すと {sum}% とほぼ 100% になっている。排他の割合なら画面の説明を直すこと"
    );
}

/// 5 業界のまとめ方が、顧客に配った見本と同じであること。
///
/// # なぜ職種数まで見るか
/// 分類名を 1 文字打ち間違えても、コードは動く。その分類が丸ごと
/// 「5 業界の外」に落ちるだけで、業界の数字が静かに減る。
/// 見本（claudedocs/indeed_newsletter.html の「4.1 業界のまとめ方」）に
/// 書いてある職種数と突き合わせれば、打ち間違いはここで落ちる。
#[test]
fn 五業界のまとめ方が見本と一致する() {
    use rust_dashboard::indeed::industry;

    let s = snap();
    let mut count: std::collections::HashMap<&str, usize> = std::collections::HashMap::new();
    let mut outside = 0usize;
    for t in &s.titles {
        match industry::of_category(&t.category) {
            Some(name) => *count.entry(name).or_insert(0) += 1,
            None => outside += 1,
        }
    }

    // 見本に書いてある職種数
    for (name, want) in [
        ("物流・運輸", 17),
        ("製造・生産", 34),
        ("建設・設備・整備", 17),
        ("サービス・販売", 21),
        ("事務・管理", 11),
    ] {
        let got = count.get(name).copied().unwrap_or(0);
        assert_eq!(
            got, want,
            "{name} が {got} 職種。見本は {want} 職種。分類名の打ち間違いを疑うこと"
        );
    }

    // 外に出るのは農林水産と、Indeed 側の分類が実態と離れているもの。
    // 名簿ずれの修正で拾った職種（分類が引けないもの）もここに入る。
    assert!(
        (4..=8).contains(&outside),
        "5 業界の外が {outside} 職種。見本では 4 職種。増えすぎ・減りすぎを疑うこと"
    );

    // 全部の職種がどちらかに入っていること
    let inside: usize = count.values().sum();
    assert_eq!(
        inside + outside,
        s.titles.len(),
        "業界に振り分けた数と職種の総数が合わない"
    );
}
