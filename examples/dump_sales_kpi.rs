//! 営業KPI の API が返す JSON を、テスト用データからファイルへ書き出す。
//!
//! 使いみち:
//!   画面（`templates/tabs/sales_kpi.html`）が実際に描けるかを、
//!   Sheets の資格情報なしで確かめる。書き出した JSON を簡易サーバで
//!   `/api/sales-kpi/data` として返せば、本物と同じ fetch 経路を通せる。
//!
//! 使い方:
//!   cargo run --example dump_sales_kpi -- out.json [YYYY-MM-DD]

use std::sync::Arc;
use std::time::Instant;

use chrono::NaiveDate;
use rust_dashboard::handlers::call_quality::sheets::SheetData;
use rust_dashboard::handlers::sales_kpi::{routes::build_payload, Sheets};

fn load_tsv(name: &str) -> Arc<SheetData> {
    let path = format!(
        "{}/tests/fixtures/sales_kpi/{name}.tsv",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("テストデータが読めません {path}: {e}"));
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .expect("見出し行がありません")
        .split('\t')
        .map(|s| s.trim_start_matches('\u{feff}').to_string())
        .collect();
    let rows: Vec<Vec<Arc<str>>> = lines
        .filter(|l| !l.trim().is_empty())
        .map(|l| {
            let mut cells: Vec<Arc<str>> = l.split('\t').map(Arc::from).collect();
            cells.resize(header.len(), Arc::from(""));
            cells
        })
        .collect();
    Arc::new(SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    })
}

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .unwrap_or_else(|| "sales_kpi_payload.json".to_string());
    let day = args
        .next()
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());

    let sheets = Sheets {
        shodan: load_tsv("KPI営業_商談"),
        apo: load_tsv("KPI営業_アポ"),
        cyomi: load_tsv("KPI営業_Cヨミ"),
        kaden: load_tsv("KPI営業_架電日次"),
        kaden_list: load_tsv("KPI営業_架電リスト"),
        member: load_tsv("KPI営業_メンバー"),
        meta: load_tsv("KPI営業_取得条件"),
        all_cached: true,
    };
    let body = build_payload(&sheets, day);
    std::fs::write(&out, serde_json::to_string(&body)?)?;
    println!(
        "{out} に書き出しました（{day} 基準・{} バイト）",
        std::fs::metadata(&out)?.len()
    );
    Ok(())
}
