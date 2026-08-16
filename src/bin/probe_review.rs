//! 口コミCSV解析 (`summarize_review_csv`) の形式前提を実測する一時的な検査。
//!
//! 2026-08-03 の競合CSVパーサ事故 (1列目が空なら行を捨てる) と同型のバグが
//! 口コミ側に残っていないかを、列順・列名・エンコーディング・打ち切り・
//! 決定性の観点で通す。コミット対象ではない。
//!
//! 実行: cargo run --release --bin probe_review

use rust_dashboard::job_gen::journey::{
    allowed_evidence_refs, summarize_competitor_csv, summarize_review_csv, ReviewSummary,
};

const COMPETITOR_CSV: &str = "css-1hwmqh1,css-bxyec3 href,css-bxyec3,css-lx9x6g,css-14qk2ra,css-18rxko3,css-18rxko3 (2),jobsearch-JobCard-tag,css-1vlebyu,css-u74ql7\n正社員,https://example.com/1,販売スタッフ,店舗販売,会社A,東京都 大田区,月給 300000円,研修あり,仕事内容,人気\n";

fn report(label: &str, bytes: &[u8]) -> Option<ReviewSummary> {
    println!("\n----- {label} -----");
    match summarize_review_csv(bytes, "reviews.csv", Some("2026-08-03".into())) {
        Ok(summary) => {
            println!(
                "OK encoding={} total_rows={} text_rows={} blank={} dup={} evidence={} risk_flagged={} sampled_risk={} sampled_other={}",
                summary.encoding,
                summary.total_rows,
                summary.text_rows,
                summary.blank_text_rows,
                summary.duplicate_text_rows,
                summary.evidence_sampled_rows,
                summary.risk_flagged_text_rows,
                summary.sampled_risk_rows,
                summary.sampled_other_rows,
            );
            let refs = summary
                .evidence
                .iter()
                .map(|e| e.source_ref.clone())
                .collect::<Vec<_>>();
            println!("R番号: {}", refs.join(","));
            for evidence in summary.evidence.iter().take(4) {
                println!(
                    "  {} | date={:?} | react={:?} | text={:?}",
                    evidence.source_ref,
                    evidence.posted_relative,
                    evidence.reactions,
                    truncate(&evidence.text, 60)
                );
            }
            if summary.evidence.len() > 4 {
                println!("  ... (残り {} 件)", summary.evidence.len() - 4);
            }
            Some(summary)
        }
        Err(e) => {
            println!("ERR {e}");
            None
        }
    }
}

fn truncate(value: &str, limit: usize) -> String {
    let cleaned = value.replace('\n', "\\n");
    let mut chars = cleaned.chars();
    let head = chars.by_ref().take(limit).collect::<String>();
    if chars.next().is_some() {
        format!("{head}…({}字)", cleaned.chars().count())
    } else {
        head
    }
}

fn to_cp932(text: &str) -> Vec<u8> {
    let (bytes, _, had_errors) = encoding_rs::SHIFT_JIS.encode(text);
    assert!(!had_errors, "cp932 に変換できない文字が含まれる");
    bytes.into_owned()
}

fn main() {
    let paths: Vec<String> = std::env::args().skip(1).collect();
    if !paths.is_empty() {
        println!("=== 実ファイル ===");
        for path in &paths {
            match std::fs::read(path) {
                Ok(bytes) => {
                    report(&format!("実ファイル {path}"), &bytes);
                }
                Err(e) => println!("\n----- 実ファイル {path} -----\n読み込み失敗: {e}"),
            }
        }
    }

    println!("=== (a) 既存テストと同じ Google マップスクレイプ列構成 ===");
    let baseline = "\u{feff}yC3ZMb href,Vpc5Fe,GSM50,y3Ibjb,OA1nbd,uo5PT\n\
https://example.com/a,投稿者A,3 件,4 か月前,安全教育が気になります,❤️1\n\
https://example.com/b,投稿者B,2 件,1 年前,,\n\
https://example.com/c,投稿者C,1 件,2 年前,雰囲気は明るいです,\n";
    let base_summary = report("(a) 正常系", baseline.as_bytes());

    println!("\n=== (b) 列順の入れ替え ===");
    let swapped = "OA1nbd,uo5PT,y3Ibjb,Vpc5Fe,GSM50,yC3ZMb href\n\
安全教育が気になります,❤️1,4 か月前,投稿者A,3 件,https://example.com/a\n\
,,1 年前,投稿者B,2 件,https://example.com/b\n\
雰囲気は明るいです,,2 年前,投稿者C,1 件,https://example.com/c\n";
    report("(b) 列順入れ替え", swapped.as_bytes());

    println!("\n=== (c) 本文列名のバリエーション ===");
    for name in [
        "OA1nbd",
        "口コミ本文",
        "口コミ",
        "レビュー本文",
        "review_text",
        "review",
        "text",
        "本文",
        "comment",
        "コメント",
        "口コミ内容",
        "レビュー",
        "内容",
        "content",
        "snippet",
        "Review Text",
        "review text",
        " OA1nbd ",
        "OA1NBD",
        "\"OA1nbd\"",
    ] {
        let csv = format!("{name},y3Ibjb\n残業が多いです,1 年前\n");
        match summarize_review_csv(csv.as_bytes(), "reviews.csv", None) {
            Ok(s) => println!("  {name:<14} -> OK text_rows={}", s.text_rows),
            Err(e) => println!("  {name:<14} -> ERR {e}"),
        }
    }

    println!("\n=== (c2) 紛らわしい列が本文列より前にある場合 ===");
    // 実スクレイプで「レビューURL」列に review、「取得日」列に date を使う想定。
    let decoy = "review,date,OA1nbd,y3Ibjb\n\
https://maps.google.com/r/1,2026-08-03,残業が多く休みも取りづらいです,3 か月前\n";
    report("(c2) review/date 列が先頭にある", decoy.as_bytes());

    println!("\n=== (c3) 他ツールのエクスポート想定 (本文列は拾えるが日付/反応列名が違う) ===");
    // Apify Google Maps Reviews Scraper 相当の列名 (camelCase)。
    let apify = "name,stars,text,publishedAtDate,publishAt,likesCount,reviewUrl\n\
投稿者A,2,残業が多く休みも取りづらいです,2026-04-01T00:00:00.000Z,4 months ago,3,https://example.com/a\n";
    report("(c3) Apify風 camelCase", apify.as_bytes());
    // Outscraper 相当 (snake_case)。
    let outscraper = "query,name,review_id,author_title,review_text,review_rating,review_datetime_utc,review_likes\n\
会社A,会社A,r1,投稿者A,残業が多く休みも取りづらいです,2,04/01/2026 09:00:00,3\n";
    report("(c3) Outscraper風 snake_case", outscraper.as_bytes());

    println!("\n=== (d) CP932 (Shift-JIS) ===");
    let sjis_source = "OA1nbd,y3Ibjb\n給与が低く残業が多いです,1 年前\n人間関係は良好です,2 年前\n";
    report("(d) CP932", &to_cp932(sjis_source));

    println!("\n=== (e) 長文・絵文字・改行入り ===");
    let long_text = "残業".repeat(2500); // 5000 字
    let with_newline = "1行目です\n2行目です\n3行目です";
    let emoji = "とても良い職場です👨‍👩‍👧‍👦🎉🙆‍♀️";
    let long_csv = format!(
        "OA1nbd,y3Ibjb\n\"{long_text}\",1 年前\n\"{with_newline}\",2 年前\n{emoji},3 年前\n"
    );
    if let Some(summary) = report("(e) 長文/改行/絵文字", long_csv.as_bytes()) {
        for evidence in &summary.evidence {
            println!(
                "  {} text_chars={} 末尾={:?}",
                evidence.source_ref,
                evidence.text.chars().count(),
                evidence
                    .text
                    .chars()
                    .rev()
                    .take(3)
                    .collect::<String>()
                    .chars()
                    .rev()
                    .collect::<String>()
            );
        }
    }

    println!("\n=== (f) REVIEW_EVIDENCE_LIMIT(40) 超え ===");
    // f-1: 後方のネガティブがリスク語を含む場合
    let mut rows = String::from("OA1nbd,y3Ibjb\n");
    for index in 1..=44 {
        rows.push_str(&format!("普通に働けている職場です{index},{index} か月前\n"));
    }
    rows.push_str("残業が多くパワハラもあり最悪でした,45 か月前\n");
    report("(f-1) 45行目にリスク語ありネガティブ", rows.as_bytes());

    // f-2: 後方のネガティブがリスク語辞書に無い表現の場合
    let mut rows2 = String::from("OA1nbd,y3Ibjb\n");
    for index in 1..=44 {
        rows2.push_str(&format!("普通に働けている職場です{index},{index} か月前\n"));
    }
    rows2.push_str("二度と行きたくない。応募はおすすめしません,45 か月前\n");
    if let Some(summary) = report("(f-2) 45行目にリスク語なしネガティブ", rows2.as_bytes())
    {
        let kept = summary
            .evidence
            .iter()
            .any(|evidence| evidence.source_ref == "R45");
        println!("  → R45 が evidence に残ったか: {kept}");
    }

    // f-3: ポジティブ本文にリスク語が含まれる場合 (誤検知)
    let false_positive = "OA1nbd\n残業はほとんどなく休日も多い、給与も納得できる良い会社です\n";
    if let Some(summary) = report(
        "(f-3) 「残業なし」等のポジティブ",
        false_positive.as_bytes(),
    ) {
        println!(
            "  → risk_flagged_text_rows={} (0 が期待値)",
            summary.risk_flagged_text_rows
        );
    }

    println!("\n=== (g) 退化ケース ===");
    report("(g-1) 空ファイル", b"");
    report("(g-2) ヘッダのみ", "OA1nbd,y3Ibjb\n".as_bytes());
    report("(g-3) 1行だけ(ヘッダなし)", "残業が多いです\n".as_bytes());
    report(
        "(g-4) ヘッダなし・2行",
        "残業が多いです,1 年前\n休日は取りやすい,2 年前\n".as_bytes(),
    );
    report(
        "(g-5) 列数が足りない行",
        "OA1nbd,y3Ibjb,uo5PT\n残業が多い,1 年前,❤️1\n休日は多い\n給与は普通,3 年前,\n".as_bytes(),
    );
    report(
        "(g-6) 列数が多い行(クオート漏れ想定)",
        "OA1nbd,y3Ibjb\n本文A,1 年前\n本文B, 実は本文の続き,2 年前\n".as_bytes(),
    );
    report(
        "(g-7) ヘッダに空セルが混じる",
        ",OA1nbd,y3Ibjb\n1,残業が多いです,1 年前\n2,休日は多いです,2 年前\n".as_bytes(),
    );
    report(
        "(g-8) 重複本文",
        "OA1nbd,y3Ibjb\n同じ口コミ,1 年前\n同じ 口コミ,2 年前\n別の口コミ,3 年前\n".as_bytes(),
    );

    println!("\n=== R番号の決定性 (同一CSVを2回) ===");
    let mut big = String::from("OA1nbd,y3Ibjb\n");
    for index in 1..=120 {
        let text = if index % 7 == 0 {
            format!("残業が多く不満です{index}")
        } else {
            format!("普通の職場です{index}")
        };
        big.push_str(&format!("{text},{index} か月前\n"));
    }
    let mut runs = Vec::new();
    for _ in 0..5 {
        let summary = summarize_review_csv(big.as_bytes(), "reviews.csv", None).expect("big");
        runs.push(
            summary
                .evidence
                .iter()
                .map(|e| e.source_ref.clone())
                .collect::<Vec<_>>(),
        );
    }
    let all_same = runs.windows(2).all(|pair| pair[0] == pair[1]);
    println!("5回の R番号列が完全一致: {all_same}");
    println!("1回目: {}", runs[0].join(","));

    println!("\n=== allowed_evidence_refs との整合 ===");
    let competitor = summarize_competitor_csv(COMPETITOR_CSV.as_bytes(), "competitors.csv", None)
        .expect("competitor");
    if let Some(summary) = base_summary {
        let allowed = allowed_evidence_refs(&[], &[], &competitor, &summary, &[]);
        let mut sorted = allowed.into_iter().collect::<Vec<_>>();
        sorted.sort();
        println!("allowed = {sorted:?}");
    }
    let big_summary = summarize_review_csv(big.as_bytes(), "reviews.csv", None).expect("big");
    let allowed = allowed_evidence_refs(&[], &[], &competitor, &big_summary, &[]);
    let review_refs = allowed
        .iter()
        .filter(|reference| reference.starts_with('R'))
        .count();
    println!(
        "120行入力: total_rows={} text_rows={} allowedのR番号={} 件 (打ち切り後のみ許可されるか)",
        big_summary.total_rows, big_summary.text_rows, review_refs
    );
}
