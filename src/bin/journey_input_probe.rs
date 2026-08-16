//! 応募者ジャーニー診断の「LLM を使わない部分」を実データで通す一時的な検査。
//!
//! Gemini を呼ばずに、競合CSV解析・比較母集団の判定・口コミCSV解析・根拠番号の
//! 割り当てまでを実ファイルで確認する。コミット対象ではない。
//!
//! 実行:
//!   cargo run --release --bin journey_input_probe -- <競合CSV> <口コミCSV>

use rust_dashboard::job_gen::journey;

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 {
        anyhow::bail!("使い方: journey_input_probe <競合CSV> <口コミCSV>");
    }
    let competitor_path = &args[0];
    let review_path = &args[1];

    let competitor_bytes = std::fs::read(competitor_path)?;
    let review_bytes = std::fs::read(review_path)?;
    let competitor_name = file_name(competitor_path);
    let review_name = file_name(review_path);

    println!("== 競合求人CSVの解析 ==");
    let competitor = journey::summarize_competitor_csv(
        &competitor_bytes,
        &competitor_name,
        Some("2026-07-28".into()),
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&competitor)?);

    println!("\n== 比較母集団の判定（顧客: 群馬県 前橋市 / 大型ドライバー / 正社員）==");
    let (cohort, cohort_summary) = journey::build_comparison_cohort(
        &competitor_bytes,
        &competitor_name,
        Some("2026-07-28".into()),
        "≪日勤≫大型ドライバー",
        "ドライバー",
        &[
            "大型ドライバー".to_string(),
            "トラックドライバー".to_string(),
            "ドライバー".to_string(),
        ],
        "群馬県",
        "前橋市",
        "正社員",
        "",
    )
    .map_err(|e| anyhow::anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&cohort)?);
    println!(
        "母集団として採用された件数: {}",
        cohort_summary.as_ref().map(|s| s.record_count).unwrap_or(0)
    );

    println!("\n== 口コミCSVの解析 ==");
    let reviews =
        journey::summarize_review_csv(&review_bytes, &review_name, Some("2026-08-03".into()))
            .map_err(|e| anyhow::anyhow!(e))?;
    println!("{}", serde_json::to_string_pretty(&reviews)?);

    println!("\n== 根拠番号の割り当て ==");
    let refs = journey::allowed_evidence_refs(
        &[],
        &[],
        cohort_summary.as_ref().unwrap_or(&competitor),
        &reviews,
        &[],
    );
    let mut refs: Vec<String> = refs.into_iter().collect();
    refs.sort();
    println!("許可される根拠番号 {} 件: {:?}", refs.len(), refs);

    Ok(())
}

fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("input.csv")
        .to_string()
}
