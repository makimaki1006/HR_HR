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
//!   cargo run --example dump_sales_kpi -- out.json [YYYY-MM-DD] [--attr]
//!
//! `--attr` を付けると、商談・アポ・Cヨミの 3 シートの末尾に「商談属性」列を足す
//! （既知の 3 値・空・空白つき・定義外・複数値を取り混ぜる。画面確認用。本物のシートには触れない）。

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use chrono::NaiveDate;
use rust_dashboard::handlers::call_quality::sheets::SheetData;
use rust_dashboard::handlers::sales_kpi::{fixture, routes::build_payload};

/// 取り混ぜた「商談属性」の生の値。既知 3 値（決裁者商談・決定者商談・担当者商談）、空、前後に空白つき、
/// 定義外（旧「商談種別」の内部値「代表者商談」を含む）、`;` 区切りの複数値（1 件として数える）。
/// 行の通し番号で順に回す。
const ATTR_CYCLE: [&str; 9] = [
    "決裁者商談",
    "決定者商談",
    "担当者商談",
    "",
    " 決定者商談 ",
    "新属性",
    "決裁者商談;担当者商談",
    "決裁者商談;",
    "代表者商談",
];

fn with_attr(sheet: &SheetData, shift: usize) -> Arc<SheetData> {
    let mut header = sheet.header.clone();
    header.push("商談属性".to_string());
    let rows = sheet
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut r2 = r.clone();
            r2.push(Arc::from(ATTR_CYCLE[(i + shift) % ATTR_CYCLE.len()]));
            r2
        })
        .collect();
    Arc::new(SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    })
}

fn main() -> anyhow::Result<()> {
    let attr = std::env::args().any(|a| a == "--attr");
    let mut args = std::env::args().skip(1).filter(|a| a != "--attr");
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
    let mut sheets = fixture::sheets_from_dir(&dir)?;
    if attr {
        sheets.shodan = with_attr(&sheets.shodan, 0);
        sheets.apo = with_attr(&sheets.apo, 3);
        sheets.cyomi = with_attr(&sheets.cyomi, 5);
    }
    let body = build_payload(&sheets, day);
    std::fs::write(&out, serde_json::to_string(&body)?)?;
    println!(
        "{out} に書き出しました（{day} 基準・{} バイト）",
        std::fs::metadata(&out)?.len()
    );
    Ok(())
}
