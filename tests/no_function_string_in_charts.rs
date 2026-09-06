//! 図の設定に JavaScript の式を文字列で入れていないか、リポジトリ全体で見張る。
//!
//! # なぜ必要か
//! ECharts の設定は `data-chart-config` という HTML 属性に入れた JSON で渡している。
//! JSON に関数は書けないので、式を渡したいときは文字列にするしかない。
//! それを関数に戻す処理は `static/js/app.js` にしか無く、顧客レポートや
//! insight レポートの初期化経路には無い。戻らなければ ECharts は文字列を
//! 解釈できず、**その部分が黙って描かれない**。
//!
//! すでに 2 回踏んでいる。
//!   * 2026-05-13 人口ピラミッドの X 軸 formatter。本番 PDF で文字列がそのまま出た
//!   * 2026-09-06 Indeed 散布図の symbolSize。点が 1 つも描かれず軸と凡例だけ出た
//!
//! どちらも「要素がある・初期化済み・データが入っている」検査は素通りした。
//! 出どころを断つほうが確実なので、ソースの側で止める。

use std::fs;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<(String, usize, String)>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().map(|x| x == "rs").unwrap_or(false) {
            let Ok(text) = fs::read_to_string(&p) else {
                continue;
            };
            for (i, line) in text.lines().enumerate() {
                // JSON の文字列値として function が始まる形だけを見る。
                // `forEach(function (el)` のような素の JavaScript は対象外
                if line.contains("\\\"function") {
                    out.push((p.display().to_string(), i + 1, line.trim().to_string()));
                }
            }
        }
    }
}

#[test]
fn 図の設定にjavascriptの式を文字列で入れていない() {
    let mut hits = Vec::new();
    walk(Path::new("src"), &mut hits);
    // このファイル自身の説明文とテストは除く
    hits.retain(|(f, _, _)| !f.contains("no_function_string_in_charts"));
    assert!(
        hits.is_empty(),
        "図の設定に JavaScript の式が文字列で入っています。\
         関数に戻す処理が無い経路では描画されません。値は Rust 側で確定させてください。\n{}",
        hits.iter()
            .map(|(f, l, s)| format!("  {f}:{l}  {}", &s[..s.len().min(110)]))
            .collect::<Vec<_>>()
            .join("\n")
    );
}
