//! 給与パースの実データ監査用プローブ (2026-08-03)。
//!
//! 本番の `upload::parse_csv_bytes` → `salary_parser::parse_salary` を通し、
//! 「入力文字列 → 解釈結果 (単位種別・下限・上限・月額換算・グループ)」を TSV で出力する。
//! コミット対象ではない。
//!
//! 実行:
//!   cargo run --release --bin probe_salary -- <CSV> [<CSV> ...]

use rust_dashboard::handlers::survey::salary_parser::SalaryType;
use rust_dashboard::handlers::survey::upload;

fn type_name(t: &SalaryType) -> &'static str {
    match t {
        SalaryType::Hourly => "時給",
        SalaryType::Daily => "日給",
        SalaryType::Weekly => "週給",
        SalaryType::Monthly => "月給",
        SalaryType::Annual => "年収",
    }
}

fn main() -> anyhow::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.is_empty() {
        anyhow::bail!("使い方: probe_salary <CSV> [<CSV> ...]");
    }

    println!(
        "file\trow_index\tsource\tencoding\temployment_type\tsalary_raw\tsalary_type\tmin_value\tmax_value\thas_range\tunified_monthly\tunified_annual\trange_category\tconfidence\tbonus_months\tin_aggregation"
    );

    for path in &args {
        let bytes = std::fs::read(path)?;
        let name = std::path::Path::new(path)
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| path.clone());
        let (_decoded, encoding) = upload::decode_csv_bytes(&bytes);
        let records = match upload::parse_csv_bytes(&bytes, None) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("PARSE_ERROR\t{}\t{}", name, e);
                continue;
            }
        };
        for r in &records {
            let p = &r.salary_parsed;
            // journey.rs:270 の集計採用条件と同一
            let in_agg = p
                .unified_monthly
                .map(|m| (50_000..=2_000_000).contains(&m))
                .unwrap_or(false);
            println!(
                "{}\t{}\t{:?}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{:.2}\t{}\t{}",
                name,
                r.row_index,
                r.source,
                encoding,
                r.employment_type.replace('\t', " "),
                r.salary_raw.replace('\t', " ").replace('\n', " "),
                type_name(&p.salary_type),
                p.min_value.map(|v| v.to_string()).unwrap_or_default(),
                p.max_value.map(|v| v.to_string()).unwrap_or_default(),
                p.has_range,
                p.unified_monthly.map(|v| v.to_string()).unwrap_or_default(),
                p.unified_annual.map(|v| v.to_string()).unwrap_or_default(),
                p.range_category.clone().unwrap_or_default(),
                p.confidence,
                p.bonus_months.map(|v| v.to_string()).unwrap_or_default(),
                in_agg
            );
        }
        eprintln!(
            "OK\t{}\trecords={}\tencoding={}",
            name,
            records.len(),
            encoding
        );
    }
    Ok(())
}
