//! PoC 実測: 時間帯ヒート_クロス をサーバ側で集計すると何が変わるか
//!
//! 2026-08-14。検証したいのは1点:
//!   「200,680行をブラウザに送って JS で集計する」現行 GAS に対し、
//!   「常駐メモリで持ってサーバ側で畳み、必要分だけ返す」とどう変わるか。
//!
//! Sheets を叩かずローカル CSV を入力にする。理由:
//!   - 測りたいのは **集計と返却量** であって Sheets API のレイテンシではない
//!     （Sheets 取得の数秒は言語非依存で、GAS でも Rust でも同じ）
//!   - 認証情報を使わずに繰り返し測れる
//!
//! 実行:
//!   cargo run --example poc_heatmap_bench -- <csvパス>

use std::sync::Arc;
use std::time::Instant;

use rust_dashboard::handlers::call_quality::heatmap::{aggregate, CrossRow, HeatmapQuery};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("CSVパスを渡してください")?;

    // ---- 読込（Sheets 取得の代わり。これは GAS/Rust で差がつかない部分） ----
    let t0 = Instant::now();
    let text = std::fs::read_to_string(&path)?;
    let mut lines = text.lines();
    let header: Vec<&str> = lines
        .next()
        .ok_or("空ファイル")?
        .trim_start_matches('\u{feff}')
        .split(',')
        .map(|s| s.trim())
        .collect();
    let idx = |n: &str| header.iter().position(|h| *h == n);
    let (i_owner, i_wd, i_hr, i_pref, i_ind, i_dial, i_conn, i_apo) = (
        idx("owner_id"),
        idx("weekday"),
        idx("hour"),
        idx("prefecture"),
        idx("industry"),
        idx("dial_count"),
        idx("connect_count"),
        idx("apo_count"),
    );

    let mut interner: std::collections::HashMap<String, Arc<str>> = std::collections::HashMap::new();
    let mut rows: Vec<CrossRow> = Vec::new();
    let mut skipped = 0usize;
    for line in lines {
        if line.trim().is_empty() {
            continue;
        }
        let f: Vec<&str> = line.split(',').collect();
        let g = |i: Option<usize>| -> &str { i.and_then(|i| f.get(i)).copied().unwrap_or("").trim() };
        let wd = g(i_wd).parse::<u8>().unwrap_or(255);
        let hr = g(i_hr).parse::<u8>().unwrap_or(255);
        if wd > 6 || hr > 23 {
            skipped += 1;
            continue;
        }
        let mut intern = |s: &str| -> Arc<str> {
            if let Some(a) = interner.get(s) {
                return Arc::clone(a);
            }
            let a: Arc<str> = Arc::from(s);
            interner.insert(s.to_string(), Arc::clone(&a));
            a
        };
        let n = |s: &str| s.parse::<f64>().ok().map(|v| v as u32).unwrap_or(0);
        rows.push(CrossRow {
            owner_id: g(i_owner).parse::<u64>().unwrap_or(0),
            weekday: wd,
            hour: hr,
            prefecture: intern(g(i_pref)),
            industry: intern(g(i_ind)),
            dial_count: n(g(i_dial)),
            connect_count: n(g(i_conn)),
            apo_count: n(g(i_apo)),
        });
    }
    let load_ms = t0.elapsed().as_millis();

    println!("== 入力 ==");
    println!("  行数            : {}", rows.len());
    println!("  読込不能で除外  : {}", skipped);
    println!("  読込+正規化     : {} ms （常駐後は2回目以降0。GASは毎回これが要る）", load_ms);
    println!("  ユニーク都道府県/業種: {}", interner.len());

    // ---- 現行GASが送っている量（=生データ全件をJSONにした場合） ----
    let raw_json_bytes: usize = rows
        .iter()
        .map(|r| {
            // {"owner_id":N,"weekday":N,"hour":N,"prefecture":"..","industry":"..",
            //  "dial_count":N,"connect_count":N,"apo_count":N}
            90 + r.prefecture.len() + r.industry.len()
        })
        .sum();

    // ---- サーバ側集計（絞り込みなし） ----
    let t1 = Instant::now();
    let (cells, used) = aggregate(&rows, &HeatmapQuery::default());
    let agg_ms = t1.elapsed().as_micros();
    let cells_json = serde_json::to_string(&cells)?;

    println!("\n== 集計（絞り込みなし） ==");
    println!("  集計時間        : {} μs", agg_ms);
    println!("  集計に使った行  : {}", used);
    println!("  返すセル数      : {}", cells.len());
    println!("  返却バイト数    : {} bytes", cells_json.len());
    println!("  現行相当(全件)  : {} bytes", raw_json_bytes);
    println!(
        "  削減            : {:.1}% ({}分の1)",
        (1.0 - cells_json.len() as f64 / raw_json_bytes as f64) * 100.0,
        raw_json_bytes / cells_json.len().max(1)
    );

    // ---- 絞り込みあり（サーバ側でやる意味が出るケース） ----
    let prefs: Vec<Arc<str>> = {
        let mut v: Vec<Arc<str>> = rows.iter().map(|r| Arc::clone(&r.prefecture)).collect();
        v.sort();
        v.dedup();
        v.into_iter().take(3).collect()
    };
    println!("\n== 集計（都道府県で絞り込み） ==");
    for p in prefs {
        let q = HeatmapQuery {
            prefecture: Some(p.to_string()),
            ..Default::default()
        };
        let t = Instant::now();
        let (c, u) = aggregate(&rows, &q);
        let json = serde_json::to_string(&c)?;
        println!(
            "  {:<10} : {:>6} μs / {:>3} セル / {:>6} bytes / 元 {:>7} 行",
            p,
            t.elapsed().as_micros(),
            c.len(),
            json.len(),
            u
        );
    }

    // ---- 繰り返し集計（絞り込みを変えるたびに何が起きるか） ----
    let t2 = Instant::now();
    const N: usize = 100;
    for _ in 0..N {
        let _ = aggregate(&rows, &HeatmapQuery::default());
    }
    println!(
        "\n== 絞り込みを{}回変えた場合 ==\n  合計 {} ms（1回あたり {:.2} ms）",
        N,
        t2.elapsed().as_millis(),
        t2.elapsed().as_millis() as f64 / N as f64
    );
    println!("  ※ 現行GASはこの都度、全件をブラウザで再集計している");

    Ok(())
}
