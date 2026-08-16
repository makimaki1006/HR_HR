//! 事実照合(引用の実在チェック)の実データ再現プローブ。
//!
//! 2026-08-04: 沖縄の実運用テストで「手当 引用不一致」が出た。原文には
//! 「・通勤手当（実費支給／規定あり）…」が確実に存在するのに rejected になる
//! 原因を、本番と同じ HTML→テキスト正規化経路で特定する。コミット対象ではない。
//!
//! 実行: cargo run --release --bin probe_fact_verify -- <求人HTML> "<引用文>"

use rust_dashboard::job_gen::inputs::{self, InputKind};

fn normalize_like_verify(s: &str) -> String {
    // fact_extract::normalize_text は非公開のため同手順を再現
    // (全角数字→ASCII、全角カンマ等→ASCII、空白全除去、カンマ除去)
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        let mapped = match ch {
            '０'..='９' => char::from_u32(ch as u32 - 0xFEE0).unwrap_or(ch),
            '，' => ',',
            '．' => '.',
            '：' => ':',
            _ => ch,
        };
        if !mapped.is_whitespace() {
            out.push(mapped);
        }
    }
    out.replace(',', "")
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        anyhow::bail!("使い方: probe_fact_verify <求人HTML> <引用文>");
    }
    let html = std::fs::read_to_string(&args[0])?;
    let quote = &args[1];

    let jobs = inputs::normalize(InputKind::Html, Some(html), None, None).await?;
    println!("正規化ジョブ数: {}", jobs.len());
    let source = &jobs[0].source_text;
    println!("source_text 長: {}字", source.chars().count());

    // 本物の verify を、実際の引用文で通す (2026-08-05: 中抜き引用対応の検証)
    let raw = serde_json::json!({
        "allowances": {"value": "通勤手当・資格手当・時間外手当", "evidence_quote": quote}
    });
    let facts = rust_dashboard::job_gen::fact_extract::verify(source, &raw);
    println!(
        "\n本物のverify結果: status={} value={:?}",
        facts["allowances"].status, facts["allowances"].value
    );

    let ns = normalize_like_verify(source);
    let nq = normalize_like_verify(quote);
    println!("旧方式(連続一致のみ)なら: {}", ns.contains(&nq));

    // 一致しない場合、どこで割れるかを二分探索的に特定
    if !ns.contains(&nq) {
        let chars: Vec<char> = nq.chars().collect();
        let mut ok = 0;
        for len in (1..=chars.len()).rev() {
            let prefix: String = chars[..len].iter().collect();
            if ns.contains(&prefix) {
                ok = len;
                break;
            }
        }
        let matched: String = chars[..ok].iter().collect();
        let broken: String = chars[ok..(ok + 12).min(chars.len())].iter().collect();
        println!(
            "\n一致する最長プレフィックス({ok}字): …{}",
            &matched
                .chars()
                .rev()
                .take(30)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>()
        );
        println!("割れた直後の引用側: {broken:?}");
        // 原文側の対応箇所を表示
        if let Some(pos) = ns.find(
            &matched
                .chars()
                .rev()
                .take(20)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>(),
        ) {
            let tail: String = ns
                .chars()
                .skip(ns[..pos].chars().count())
                .take(60)
                .collect();
            println!("原文側の対応箇所: {tail:?}");
        }
        // 「通勤手当」周辺の原文を表示
        if let Some(p) = source.find("通勤手当") {
            let start = source[..p].chars().count().saturating_sub(30);
            let ctx: String = source.chars().skip(start).take(160).collect();
            println!("\n原文の「通勤手当」周辺(生):\n{ctx}");
        } else {
            println!("\n原文に「通勤手当」が存在しない");
        }
    }
    Ok(())
}
