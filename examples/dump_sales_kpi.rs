//! 営業KPI の API が返す JSON を、テスト用データからファイルへ書き出す。
//!
//! 使いみち:
//!   画面（`templates/tabs/sales_kpi.html`）が実際に描けるかを、
//!   Sheets の資格情報なしで確かめる。書き出した JSON を簡易サーバで
//!   `/api/sales-kpi/data` として返せば、本物と同じ fetch 経路を通せる。
//!
//! 使い方:
//!   cargo run --example dump_sales_kpi -- out.json [YYYY-MM-DD] [--negtype]
//!
//! `--negtype` を付けると、商談・アポ・Cヨミの 3 シートの末尾に「商談種別」列を足す
//! （内部値・ラベル・空・空白つき・定義外を取り混ぜる。画面確認用。本物のシートには触れない）。

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

/// 取り混ぜた「商談種別」の生の値。内部値（代表者商談・担当者商談）とラベル（決裁者商談・非決裁者商談）、
/// 空、前後に空白つき、定義外。行の通し番号で順に回す。
const NEGTYPE_CYCLE: [&str; 9] = [
    "代表者商談",
    "担当者商談",
    "決裁者商談",
    "非決裁者商談",
    "",
    " 担当者商談 ",
    "新種別",
    // `;` 区切りの複数値(1 件として数え、各部分をラベルに直して表示する)。末尾の `;` は空の部分
    "担当者商談;代表者商談",
    "担当者商談;",
];

fn with_negtype(sheet: &SheetData, shift: usize) -> Arc<SheetData> {
    let mut header = sheet.header.clone();
    header.push("商談種別".to_string());
    let rows = sheet
        .rows
        .iter()
        .enumerate()
        .map(|(i, r)| {
            let mut r2 = r.clone();
            r2.push(Arc::from(NEGTYPE_CYCLE[(i + shift) % NEGTYPE_CYCLE.len()]));
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
    let negtype = std::env::args().any(|a| a == "--negtype");
    let mut args = std::env::args().skip(1).filter(|a| a != "--negtype");
    let out = args
        .next()
        .unwrap_or_else(|| "sales_kpi_payload.json".to_string());
    let day = args
        .next()
        .and_then(|s| NaiveDate::parse_from_str(&s, "%Y-%m-%d").ok())
        .unwrap_or_else(|| NaiveDate::from_ymd_opt(2026, 9, 4).unwrap());

    let (shodan, apo, cyomi) = (
        load_tsv("KPI営業_商談"),
        load_tsv("KPI営業_アポ"),
        load_tsv("KPI営業_Cヨミ"),
    );
    let (shodan, apo, cyomi) = if negtype {
        (
            with_negtype(&shodan, 0),
            with_negtype(&apo, 3),
            with_negtype(&cyomi, 5),
        )
    } else {
        (shodan, apo, cyomi)
    };
    let sheets = Sheets {
        shodan,
        apo,
        cyomi,
        kaden: load_tsv("KPI営業_架電日次"),
        kaden_list: load_tsv("KPI営業_架電リスト"),
        // 2026-09-17: Sheets にあとから足された 3 つ。
        // cargo build は examples を組み立てないので、
        // 足し忘れても気づけるのは cargo clippy --all-targets だけだった。
        kaden_by_owner: load_tsv("KPI営業_架電リスト_担当別"),
        member: load_tsv("KPI営業_メンバー"),
        meta: load_tsv("KPI営業_取得条件"),
        weekly: load_tsv("KPI営業_週次"),
        kettei: load_tsv("KPI営業_決定者"),
        // 2026-09-29 追加。
        list_stock: load_tsv("KPI営業_リスト在庫"),
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
