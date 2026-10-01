//! 営業KPI: Google Sheets の代わりに TSV から読む経路（ローカル専用、2026-09-30）
//!
//! `GET /api/sales-kpi/data` は Sheets が未設定だと 503 になる。旧画面（`/sales-kpi`）と
//! React 画面（`/app/sales-kpi`）を**同じ材料で**起動して表示値を突き合わせるために、
//! `tests/fixtures/sales_kpi/*.tsv` から `Sheets` を組む入口をここに置く。
//!
//! 有効になるのは環境変数 `SALES_KPI_FIXTURE_DIR` があるときだけ。**本番には置かない**
//! （置かなければ従来どおり Sheets を読む。`routes::data` の先頭で分岐する）。
//!
//! ```text
//!   SALES_KPI_FIXTURE_DIR=tests/fixtures/sales_kpi     # TSV の置き場
//!   SALES_KPI_FIXTURE_TODAY=2026-09-04                 # 判定日 (省略時は実際の今日)
//! ```
//!
//! TSV の読み方は `tests.rs` / `examples/dump_sales_kpi.rs` と同じ（ここに一本化した）。

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use anyhow::{Context, Result};
use chrono::NaiveDate;

use super::{
    empty_sheet, Sheets, SHEET_APO, SHEET_CYOMI, SHEET_KADEN, SHEET_KADEN_BY_OWNER,
    SHEET_KADEN_LIST, SHEET_KETTEI, SHEET_LIST_STOCK, SHEET_MEMBER, SHEET_META, SHEET_SHODAN,
    SHEET_WEEKLY,
};
use crate::handlers::call_quality::sheets::SheetData;

/// TSV の置き場。あれば Sheets を読まずにここから組む。
pub const ENV_DIR: &str = "SALES_KPI_FIXTURE_DIR";
/// 判定日（`yyyy-MM-dd`）。無ければ実際の今日（JST）。
pub const ENV_TODAY: &str = "SALES_KPI_FIXTURE_TODAY";

/// `SALES_KPI_FIXTURE_DIR` が空でなければそのパス。
pub fn dir_from_env() -> Option<PathBuf> {
    std::env::var(ENV_DIR)
        .ok()
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

/// `SALES_KPI_FIXTURE_TODAY` を日付として読む。無い・読めなければ `None`。
pub fn today_from_env() -> Option<NaiveDate> {
    std::env::var(ENV_TODAY)
        .ok()
        .and_then(|s| NaiveDate::parse_from_str(s.trim(), "%Y-%m-%d").ok())
}

/// タブ区切りの文字列を 1 枚のシートにする。見出しの BOM は落とし、短い行は空文字で埋める。
pub fn sheet_from_tsv(text: &str) -> SheetData {
    let mut lines = text.lines();
    let header: Vec<String> = lines
        .next()
        .unwrap_or("")
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
    SheetData {
        header,
        rows,
        fetched_at: Instant::now(),
    }
}

/// `<dir>/<シート名>.tsv` を読む。無ければ `Err`。
pub fn load_tsv(dir: &Path, name: &str) -> Result<Arc<SheetData>> {
    let path = dir.join(format!("{name}.tsv"));
    let text = std::fs::read_to_string(&path)
        .with_context(|| format!("テストデータが読めません {}", path.display()))?;
    Ok(Arc::new(sheet_from_tsv(&text)))
}

/// ディレクトリの TSV から `Sheets` を組む。
///
/// `load()` と同じく、あとから足した 4 枚（週次・担当者別・決定者・リスト在庫）は
/// ファイルが無くても空で通す。それ以外は無ければ `Err`。
pub fn sheets_from_dir(dir: &Path) -> Result<Sheets> {
    let optional = |name: &str| match load_tsv(dir, name) {
        Ok(data) => data,
        Err(e) => {
            tracing::warn!("fixture「{name}」が無いので空で出します: {e:#}");
            empty_sheet()
        }
    };
    Ok(Sheets {
        shodan: load_tsv(dir, SHEET_SHODAN)?,
        apo: load_tsv(dir, SHEET_APO)?,
        cyomi: load_tsv(dir, SHEET_CYOMI)?,
        kaden: load_tsv(dir, SHEET_KADEN)?,
        kaden_list: load_tsv(dir, SHEET_KADEN_LIST)?,
        kaden_by_owner: optional(SHEET_KADEN_BY_OWNER),
        member: load_tsv(dir, SHEET_MEMBER)?,
        meta: load_tsv(dir, SHEET_META)?,
        weekly: optional(SHEET_WEEKLY),
        kettei: optional(SHEET_KETTEI),
        list_stock: optional(SHEET_LIST_STOCK),
        // fixture は常に手元のファイルなので「全部キャッシュから」と同じ扱い。
        all_cached: true,
    })
}
