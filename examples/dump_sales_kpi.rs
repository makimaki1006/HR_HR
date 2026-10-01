//! 営業KPI の API が返す JSON を、テスト用データからファイルへ書き出す。
//!
//! 使いみち:
//!   画面（`templates/tabs/sales_kpi.html` / React `frontend/src/screens/sales-kpi/`）が
//!   実際に描けるかを、Sheets の資格情報なしで確かめる。書き出した JSON を簡易サーバで
//!   `/api/sales-kpi/data` として返せば、本物と同じ fetch 経路を通せる。
//!
//!   サーバごと fixture で起動するなら、環境変数 `SALES_KPI_FIXTURE_DIR` /
//!   `SALES_KPI_FIXTURE_TODAY`（`src/handlers/sales_kpi/fixture.rs`）の方が手早い。
//!
//! 使い方:
//!   cargo run --example dump_sales_kpi -- out.json [YYYY-MM-DD]

use std::path::Path;

use chrono::NaiveDate;
use rust_dashboard::handlers::sales_kpi::{fixture, routes::build_payload};

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let out = args
        .next()
        .unwrap_or_else(|| "sales_kpi_payload.json".to_string());
    let day = args
        .next()
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());

    // 2026-09-17: Sheets にあとから足された 3 つ（担当者別・決定者・リスト在庫）も含めて、
    // TSV の読み込みは `fixture::sheets_from_dir` に一本化した（テストと同じ経路）。
    // cargo build は examples を組み立てないので、足し忘れに気づけるのは
    // cargo clippy --all-targets だけだった。
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/sales_kpi");
    let sheets = fixture::sheets_from_dir(&dir)?;
    let body = build_payload(&sheets, day);
    std::fs::write(&out, serde_json::to_string(&body)?)?;
    println!(
        "{out} に書き出しました（{day} 基準・{} バイト）",
        std::fs::metadata(&out)?.len()
    );
    Ok(())
}
