//! Rust に移した集計が、いま動いている JS と同じ数字・同じ文を出すことを固定する。
//!
//! # なぜ要るか
//! 顧客レポート（`scripts/indeed_newsletter.js`）と V2（Rust）で実装が 2 つになる。
//! ここがずれると、社内タブと顧客に渡した資料で違うことを言い始める。
//!
//! # 金型の作り直し
//! ```text
//! node scripts/indeed_golden.js   → tests/fixtures/indeed_golden.json
//! ```
//! JS 側の判定や言い回しを直したら、作り直してこのテストを通すこと。
//! 通らないまま金型だけ更新するのは、ずれを見えなくするだけなので禁止。

use rust_dashboard::indeed::trend::fit_trend;
use rust_dashboard::indeed::wording::{describe_trend, short_trend, trend_label, Words};
use serde_json::Value;

const W_SEEK: Words = Words {
    up: "集まりやすくなって",
    down: "集まりにくくなって",
};

fn golden() -> Value {
    let raw = std::fs::read_to_string("tests/fixtures/indeed_golden.json")
        .expect("tests/fixtures/indeed_golden.json が無い。node scripts/indeed_golden.js で作る");
    serde_json::from_str(&raw).expect("金型が JSON として読めない")
}

fn months(g: &Value) -> Vec<String> {
    g["months"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect()
}

/// 許容は ln / exp の実装差（最後の桁）に見合う幅。
/// これより広げるなら、なぜ広げてよいのかを書くこと。広げれば本物のずれも通る。
fn close(a: f64, b: f64, tol: f64, what: &str) {
    assert!(
        (a - b).abs() <= tol,
        "{what}: Rust {a} / JS {b}（許容 {tol}）"
    );
}

#[test]
/// 変化の判定と言い回しが JS と一致する
fn test_trend_matches_js_golden() {
    let g = golden();
    let ms = months(&g);
    let trend = g["trend"].as_object().unwrap();
    assert!(!trend.is_empty(), "金型に系列が入っていない");

    for (name, case) in trend {
        let input: Vec<Option<f64>> = case["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64())
            .collect();
        let fit = fit_trend(&input);

        // 当てはめられるかどうかが一致すること
        let js_fit = &case["fit"];
        if js_fit.is_null() {
            assert!(fit.is_none(), "{name}: JS は当てはめないが Rust は当てはめた");
        } else {
            let f = fit.as_ref().unwrap_or_else(|| panic!("{name}: Rust だけ当てはまらない"));
            close(f.slope_pct, js_fit["slopePct"].as_f64().unwrap(), 1e-9, &format!("{name} 毎月"));
            close(f.total_pct, js_fit["totalPct"].as_f64().unwrap(), 1e-9, &format!("{name} 期間全体"));
            close(f.scatter_pct, js_fit["scatterPct"].as_f64().unwrap(), 1e-9, &format!("{name} 上下"));
            assert_eq!(f.level.as_str(), js_fit["level"].as_str().unwrap(), "{name} 判定");
            assert_eq!(f.steady, js_fit["steady"].as_bool().unwrap(), "{name} 一本調子");
            assert_eq!(f.n, js_fit["n"].as_u64().unwrap() as usize, "{name} 点の数");

            let js_out = js_fit["outliers"].as_array().unwrap();
            assert_eq!(f.outliers.len(), js_out.len(), "{name} 外れ月の数");
            for (i, o) in f.outliers.iter().enumerate() {
                assert_eq!(
                    o.index,
                    js_out[i]["index"].as_u64().unwrap() as usize,
                    "{name} 外れ月 {i} の位置"
                );
                close(
                    o.ratio,
                    js_out[i]["ratio"].as_f64().unwrap(),
                    1e-9,
                    &format!("{name} 外れ月 {i} のずれ"),
                );
            }
        }

        // 言い回しは 1 文字も違ってはいけない
        assert_eq!(
            trend_label(fit.as_ref()),
            case["label"].as_str().unwrap(),
            "{name} の動き方"
        );
        assert_eq!(
            short_trend(fit.as_ref(), &W_SEEK),
            case["short"].as_str().unwrap(),
            "{name} の一覧の一文"
        );
        let m = if input.len() == ms.len() { Some(&ms[..]) } else { None };
        assert_eq!(
            describe_trend(fit.as_ref(), "求人数", m),
            case["describe"].as_str().unwrap(),
            "{name} の詳細の一文"
        );
    }
}

#[test]
/// 金型に比較の基準が入っている
fn test_golden_has_compare_months() {
    // データを更新したのに金型を作り直していない、という取り違えを防ぐ
    let g = golden();
    let c = &g["compare"];
    assert!(c["now"].as_str().is_some(), "対象月が無い");
    assert!(c["prev"].as_str().is_some(), "先月が無い");
    let ms = months(&g);
    assert_eq!(ms.last().unwrap(), c["now"].as_str().unwrap(), "対象月が月の並びの末尾と違う");
}

#[test]
/// 全国値と業界合計が桁違いになっていない
fn test_nation_matches_industry_sum() {
    // 5 業界に入らない分類（農林水産業など）があるぶん、業界合計は全国より少し小さい。
    // 桁違いになっていたら、集計の取り違えを疑う。
    let g = golden();
    let nation = g["nation"]["job"].as_f64().unwrap();
    let sum: f64 = g["industries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x["job"].as_f64().unwrap())
        .sum();
    assert!(sum <= nation, "業界合計 {sum} が全国 {nation} を超えている");
    assert!(
        sum >= nation * 0.99,
        "業界合計 {sum} が全国 {nation} から離れすぎている（5 業界の外は 1% 未満のはず）"
    );
}

#[test]
/// 顧客向けの経路は既定で閉じている
fn test_public_report_closed_by_default() {
    std::env::remove_var("INDEED_PUBLIC");
    assert!(
        !rust_dashboard::indeed::public_report_enabled(),
        "利用条件の確認が済むまで、顧客向けの経路は閉じていること"
    );
}
