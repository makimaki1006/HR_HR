//! 架電クオリティ P7: データブラウザ(任意シート閲覧・検索・ソート・ページング・CSV出力・簡易可視化)
//!
//! 2026-08-16 移植。GAS 版 (`page-p7`, index.html:939-1066) は
//! `getRawSheet(sheetName)` (Code.gs:1876) でシート全行を一括取得し、
//! 検索・列フィルタ・ソート・ページングは**すべてブラウザ側の JS**
//! (javascript.html P7_STATE 一式)が行っていた。
//!
//! 依頼メッセージの実測値: 「月次明細」シート 96,898行×39列 を毎回丸ごと送って
//! **9.48秒**。データブラウザは現行 GAS で最も重いタブ。ページサイズの
//! ドロップダウン(50/100/500/**全表示**)は表示件数を絞るだけで、
//! ネットワーク転送は常に全行だった(全表示は見た目の話で、実害は変わらない)。
//!
//! ここでの移植方針: 検索・列フィルタ・ソート・ページングを**すべてサーバ側**で行い、
//! ブラウザには「今見せる分だけ」を返す(約束6)。「全表示」は本ファイルでは
//! **実装しない**(1ページ最大 `MAX_PAGE_SIZE` 件にハードクランプし、
//! `truncated` を立てて画面に伝える。これでも生データ丸投げの100倍以上軽い)。
//! CSV エクスポートだけは機能の性質上「絞り込み後の全件」が要件なので別関数
//! `build_csv` として分離し、そちらは別枠の上限 `CSV_EXPORT_MAX_ROWS` を持つ
//! (ブラウズ用の上限よりずっと大きいが、無制限ではない)。
//!
//! 参照元:
//!   画面   scripts/gas/call_quality_app/index.html      id="page-p7" (939-1066行)
//!   取得   scripts/gas/call_quality_app/Code.gs          getRawSheetList/getRawSheet/RAW_SHEET_NAMES (1768-1900行)
//!   描画   scripts/gas/call_quality_app/javascript.html  P7_STATE 一式、特に
//!            _applySort (13223-13253行付近、数値/文字列自動判定・空値は末尾)
//!
//! 未実装(黙って省略していない項目):
//!   - 列フィルタモーダルの「値一覧(distinct values)取得」API。GAS 版はロード済み
//!     全行から選択肢を作っていたが、サーバ側ページングに変えたことで
//!     「この列に存在する値の一覧」を別途返す仕組みが要る。本ファイルは
//!     「値の配列を受け取って絞り込む」側(`RowFilter::filters`)のみ実装し、
//!     選択肢そのものを返すエンドポイントは範囲外(ページ内のデータから作るか、
//!     専用エンドポイントを足すか、方針決め後に追加)。
//!   - ソート状態の「3クリック目で解除」というUI遷移そのもの。API的には
//!     `sort_col: None` を渡せば無ソートになるので、状態遷移はフロント側の責務として
//!     機能上の制限にはならない。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};
use crate::handlers::call_quality::query_audit::ValueAudit;

use super::{SourceInfo, TabPayload};

/// 1ページの最大行数。GAS 版のページサイズ選択肢の最大(500)に合わせてクランプする。
/// 「全表示」オプションはここでは提供しない(理由は本ファイル冒頭のコメント参照)。
pub const MAX_PAGE_SIZE: usize = 500;

/// CSV エクスポートの上限行数。エクスポートは性質上「絞り込み後の全件」が要件だが、
/// 無制限にすると生データ丸投げと同じ問題が起きるため上限を設ける。
pub const CSV_EXPORT_MAX_ROWS: usize = 50_000;

/// P7 データブラウザで閲覧可能なシートのホワイトリスト。
/// GAS 版 `RAW_SHEET_NAMES` (Code.gs 1768-1849行) をそのまま移植した。
/// 無関係シートの参照防止が目的("2026-05-31 セル上限対策" のコメントも含め
/// GAS 側の経緯をそのまま残す)。
pub const ALLOWED_SHEETS: &[&str] = &[
    // 2026-05-31 セル上限対策: 'セグメント明細' は Python 側でスプシ push を
    // 停止した(約355万セル/チャート未参照)。スプシに存在しないため除外
    // (GAS版に合わせてここにも含めない)。
    "日次明細", "セグメント月次集計", "月次明細", "メンバーマスタ",
    "最新サマリ", "異常検知", "Deal Health", "Deal Health owner月次", "月末予測",
    "時間帯ヒート", "N回目架電分析", "リサイクル間隔", "コンプライアンスパターン",
    "Recency owner月次", "滞留日数", "コホート分析",
    "新規/既存/リサイクル", "ファネル4段",
    "曜日別集計", "曜日別 owner別", "都道府県月次",
    "その場失注 owner月次",
    "時間帯ヒート_クロス",
    "セグメント_クロス",
    "kpi_targets",
    "コンサル接触率_週次",
    "コンサル行動量_月次",
    "コンサル行動量_担当月次",
    "商談遷移_集計",
    "商談遷移_クロス",
    "コンサル健全性_月次",
    "コンサル健全性_月次_active",
    "コンサルタスク漏れ",
    "コンサルKGIマトリクス",
    "コンサル行動量_日次",
    "コンサル行動量_担当日次",
    "コンサル未来アクション",
    "コンサル勝ち筋分析",
    "コンサルリスクスコア_月次",
    "コンサルフェーズKPI",
    "リスク統合ボード",
    "コンサル接触ログ_週次",
    "コンサル担当者月次推移",
    "解約_理由パターン",
    "解約_コンサル担当別",
    "解約_業界規模マトリクス",
    "解約_active予測",
    "解約_モデル指標",
    "コンサルMTGタイムライン",
    "コンサル担当者360_KPI",
    "コンサル担当者360_Deal一覧",
    "BPOコーラー",
    "BPOデータ品質",
    "BPOネクストアクション",
    "BPOネクストアクション明細",
    "追いかけ停止候補",
    "成約率_現場ベース",
    "月次接触規模",
    "BPO貢献追跡",
    "通話時間バケット",
    "ステージ反復分析",
    "コーラー行動パターン",
    "業種別アクティブ観測",
    "事前集計cube",
    "業種グループ定義",
    "規模バンド定義",
    // 2026-08-17 追加: **画面が読んでいるのに、この一覧から見られなかった16枚**。
    //   GAS の `RAW_SHEET_NAMES` をそのまま移植したが、GAS 側のこの一覧は
    //   コンサル系・求人応募系のタブが増えたときに更新されておらず、
    //   「スプシをそのまま見る」ための画面から**画面が使っているシートが
    //   引けない**状態だった。検証で退避を取ったとき、この16枚だけが
    //   古いまま残り、7日前のデータと突合しかけた（実測: コンサル別
    //   ベンチマーク_統計 の n_cohort_total が 2,501 対 2,749 で 248件差）。
    "時間帯ヒート_BPO",
    "コンサル接触ロールアップ",
    "コンサル接触寄与率",
    "コンサル接触寄与率_分布",
    "コンサル接触寄与率_統計",
    "コンサル別ベンチマーク",
    "コンサル別ベンチマーク_月別",
    "コンサル別ベンチマーク_統計",
    "コンサル担当者360_都道府県",
    "コンサル未来案件_月次",
    "コンサル未来案件_Deal一覧",
    "コンサル未来案件_担当者サマリ",
    "求人応募_KPI",
    "求人応募_媒体月次",
    "求人応募_Deal健全性",
    "求人応募_データ品質",
    // p8 の C-1 が読む運用シート。現場が除外Dealを手で追加するので、
    // browse から中身を確認できないと「なぜこの案件が出ないのか」が追えない。
    "アラート除外リスト",
    // 2026-09-05 追加: 営業KPI(`/sales-kpi`)が読む7枚。
    //   画面の数字を裏取りしたいときに、ここから直接見られるようにしておく。
    //   Python の日次同期(scripts/sales_kpi/sync_daily.py)が書く。
    "KPI営業_商談", "KPI営業_アポ", "KPI営業_Cヨミ",
    "KPI営業_架電日次", "KPI営業_架電リスト", "KPI営業_メンバー", "KPI営業_取得条件",
    // 2026-09-07 追加: 週次スナップショット。集計済みの値を持つ唯一のシートなので、
    //   画面の「先週との比べ方」が疑わしいときに元の行をここから直接見られるようにする。
    "KPI営業_週次",
];

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn check_sheet_allowed(sheet: &str) -> Result<()> {
    if !ALLOWED_SHEETS.contains(&sheet) {
        bail!("許可されていないシート名: {sheet}");
    }
    Ok(())
}

/// フロントが起動時に呼ぶ、シート選択肢の一覧(GAS版 `getRawSheetList`相当)。
pub fn list_sheets() -> Vec<&'static str> {
    ALLOWED_SHEETS.to_vec()
}

// ------------------------------------------------------------- 共通: 絞り込み

/// 検索・列フィルタの指定。ブラウズ/CSVエクスポート/簡易可視化で共通に使う。
#[derive(Debug, Default, Deserialize, Serialize, Clone)]
pub struct RowFilter {
    /// 全文検索(全列 OR、大小無視)。空/None なら絞らない。
    #[serde(default)]
    pub search: Option<String>,
    // 注: `#[serde(skip_serializing_if = ...)]` を足さないこと。
    // query_audit の腐り検出テストが「既定値でも全キーが出る」前提で
    // フィールド一覧を取っているため、キーが消えると実在するフィールドを
    // 「無い」と誤判定する。
    /// 列名 → 複数値(同一列内はOR)。列間はAND。値配列が空の列は無視する。
    #[serde(default)]
    pub filters: HashMap<String, Vec<String>>,
}

/// 検索+列フィルタを適用した行の参照を返す。ソート・ページングはしない
/// (browse/build_csv/aggregate_chart がそれぞれの用途に応じて後続処理する)。
///
/// 2026-08-17 追加: `audit` に**シートに無い列名で絞ろうとした**ことを記録する。
/// `filter_map(|(name, vals)| data.col(name).map(...))` は列が見つからないと
/// **その絞り込みごと落とす**。つまり `filters: {"ownr_id": ["123"]}` は
/// 400 にも 0件にもならず、**絞り込みが消えた全件**が 200 で返る。
/// 動機になった `deals_status=NONSENSE`（絞ったつもりで全件）と同じ形。
fn filter_rows<'a>(
    data: &'a SheetData,
    f: &RowFilter,
    audit: &mut ValueAudit,
) -> Vec<&'a Vec<Arc<str>>> {
    let needle = f
        .search
        .as_deref()
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty());

    // 報告の並びを安定させる（`HashMap` の反復順をそのまま出さない。約束4）。
    let mut unknown_cols: Vec<&str> = f
        .filters
        .iter()
        .filter(|(_, vals)| !vals.is_empty())
        .filter(|(name, _)| data.col(name).is_none())
        .map(|(name, _)| name.as_str())
        .collect();
    unknown_cols.sort_unstable();
    for name in unknown_cols {
        audit.no_match(
            "filters",
            name,
            "このシートに実在する列名",
            "この列の絞り込みは無視されます（絞り込まれていない行が混ざります）",
        );
    }

    let filter_cols: Vec<(usize, &Vec<String>)> = f
        .filters
        .iter()
        .filter(|(_, vals)| !vals.is_empty())
        .filter_map(|(name, vals)| data.col(name).map(|i| (i, vals)))
        .collect();

    data.rows
        .iter()
        .filter(|row| {
            if let Some(n) = &needle {
                let hit = row.iter().any(|c| c.to_lowercase().contains(n.as_str()));
                if !hit {
                    return false;
                }
            }
            filter_cols.iter().all(|(i, vals)| {
                row.get(*i)
                    .map(|c| vals.iter().any(|v| v.as_str() == c.as_ref()))
                    .unwrap_or(false)
            })
        })
        .collect()
}

// ------------------------------------------------------------------ ブラウズ

#[derive(Debug, Clone, Copy, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum SortDir {
    #[default]
    Asc,
    Desc,
}

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct BrowseQuery {
    pub sheet: String,
    #[serde(flatten)]
    pub filter: RowFilter,
    /// 無指定ならシートの元の並び(安定)のまま返す。
    pub sort_col: Option<String>,
    #[serde(default)]
    pub sort_dir: SortDir,
    /// 0始まり。
    #[serde(default)]
    pub page: usize,
    /// 未指定なら100(GAS版の既定値と合わせる)。MAX_PAGE_SIZE でクランプする。
    pub page_size: Option<usize>,
}

// `filter` は `#[serde(flatten)]` なので、ワイヤ上のキーは `search` / `filters` として
// トップレベルに現れる。腐り検出テストは `to_value` を見るので flatten 後の名前で
// 一致を確かめられる（`filter` と書くと落ちる。それが正しい）。
crate::accepted_params!(BrowseQuery, browse_query_accepted =>
    "sheet", "search", "filters", "sort_col", "sort_dir", "page", "page_size");

#[derive(Debug, Serialize)]
pub struct BrowseData {
    pub header: Vec<String>,
    pub rows: Vec<Vec<String>>,
    /// シート全体の行数(絞り込み前)。
    pub total_rows: usize,
    /// 検索・列フィルタ後の行数。
    pub matched_rows: usize,
    /// このページで実際に返した行数(rows.len() と同じ)。
    pub returned_rows: usize,
    pub page: usize,
    pub page_size: usize,
    pub total_pages: usize,
    /// 要求された page_size が MAX_PAGE_SIZE を超えていてクランプした場合 true。
    /// 「全N件中、絞り込みでM件、うち先頭L件を表示」の判断材料。
    pub truncated: bool,
    pub sort_col: Option<String>,
    pub sort_dir: SortDir,
}

/// 列の値の大半(先頭100件の非空値のうち70%以上)が数値ならその列は数値ソートする。
/// GAS版 `_isNumericColumn`/`_applySort` (javascript.html 13223-13253行付近) の移植。
fn is_numeric_column(rows: &[&Vec<Arc<str>>], col: usize) -> bool {
    let mut total = 0usize;
    let mut numeric = 0usize;
    for row in rows.iter().take(100) {
        let v = row.get(col).map(|s| s.as_ref()).unwrap_or("");
        if v.is_empty() {
            continue;
        }
        total += 1;
        if v.trim().parse::<f64>().is_ok() {
            numeric += 1;
        }
    }
    total > 0 && (numeric as f64 / total as f64) >= 0.7
}

/// 検索・列フィルタ・ソート・ページングを一括で行う本体。
pub fn browse(data: &SheetData, q: &BrowseQuery, audit: &mut ValueAudit) -> BrowseData {
    let mut matched = filter_rows(data, &q.filter, audit);
    let matched_rows = matched.len();

    // --- ソート(安定ソート。約束4) ---
    // 2026-08-17 追加: 存在しない列名を渡すと `data.col(sc)` が None になり
    //   **並べ替えが黙って行われない**。それなのに応答の `sort_col` には
    //   送られた値がそのまま返る（＝「その列で並べた」と読める）。
    //   並び順は「上位が誰か」を決めるので、効いていないと気づけないのは重い。
    if let Some(sc) = &q.sort_col {
        if data.col(sc).is_none() && !sc.trim().is_empty() {
            audit.no_match(
                "sort_col",
                sc,
                "このシートに実在する列名",
                "並べ替えは行われず、シートの元の並びのまま返します",
            );
        }
        if let Some(si) = data.col(sc) {
            let numeric = is_numeric_column(&matched, si);
            matched.sort_by(|a, b| {
                let va = a.get(si).map(|s| s.as_ref()).unwrap_or("");
                let vb = b.get(si).map(|s| s.as_ref()).unwrap_or("");
                let na = va.is_empty();
                let nb = vb.is_empty();
                // 空値は常に末尾(GAS版と同じ。ソート方向に関わらず)。
                let ord = if na && nb {
                    std::cmp::Ordering::Equal
                } else if na {
                    return std::cmp::Ordering::Greater;
                } else if nb {
                    return std::cmp::Ordering::Less;
                } else if numeric {
                    let ka = va.trim().parse::<f64>().unwrap_or(0.0);
                    let kb = vb.trim().parse::<f64>().unwrap_or(0.0);
                    ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal)
                } else {
                    va.cmp(vb)
                };
                if q.sort_dir == SortDir::Desc {
                    ord.reverse()
                } else {
                    ord
                }
            });
        }
    }

    // --- ページング(上限クランプ。約束3) ---
    let requested = q.page_size.unwrap_or(100);
    let page_size = requested.clamp(1, MAX_PAGE_SIZE);
    let truncated = requested > MAX_PAGE_SIZE;
    let total_pages = matched_rows.saturating_add(page_size - 1) / page_size.max(1);
    let total_pages = total_pages.max(1);
    let page = q.page.min(total_pages - 1);
    let start = page * page_size;
    let page_rows: Vec<Vec<String>> = matched
        .iter()
        .skip(start)
        .take(page_size)
        .map(|r| r.iter().map(|c| c.to_string()).collect())
        .collect();

    BrowseData {
        header: data.header.clone(),
        returned_rows: page_rows.len(),
        rows: page_rows,
        total_rows: data.rows.len(),
        matched_rows,
        page,
        page_size,
        total_pages,
        truncated,
        sort_col: q.sort_col.clone(),
        sort_dir: q.sort_dir,
    }
}

/// ハンドラ本体(ブラウズ)。
pub async fn handle_browse(
    store: &SheetStore,
    client: &SheetsClient,
    q: BrowseQuery,
) -> Result<TabPayload<BrowseData>> {
    let started = Instant::now();
    check_sheet_allowed(&q.sheet)?;
    let (data, from_cache) = store.get(client, &q.sheet).await?;
    let mut audit = ValueAudit::new();
    let out = browse(&data, &q, &mut audit);
    let sources = vec![SourceInfo {
        sheet: q.sheet.clone(),
        total_rows: data.rows.len(),
        matched_rows: out.matched_rows,
        from_cache,
        age_secs: data.fetched_at.elapsed().as_secs(),
    }];
    Ok(TabPayload {
        data: out,
        sources,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
        // こちらは**タブ側が詰める**。値の意味を知っているのはここだけ。
        invalid_values: audit.into_vec(),
    })
}

// -------------------------------------------------------------- CSVエクスポート

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct ExportQuery {
    pub sheet: String,
    #[serde(flatten)]
    pub filter: RowFilter,
}

crate::accepted_params!(ExportQuery, export_query_accepted =>
    "sheet", "search", "filters");

#[derive(Debug, Serialize)]
pub struct CsvExport {
    /// BOM付きCSV文字列(Excel文字化け対策。GAS版の仕様を踏襲)。
    pub csv: String,
    /// 実際にCSVへ書き出した行数。
    pub row_count: usize,
    /// 絞り込み後の全行数(row_count と異なれば CSV_EXPORT_MAX_ROWS で切っている)。
    pub matched_rows: usize,
    pub truncated: bool,
    /// 解釈できず捨てた引数名。空でも必ず出す。
    /// このエンドポイントだけ `TabPayload` を通らないので個別に持つ。
    /// ここが抜けていると「絞り込んだつもりの CSV」を全件 CSV と見分けられない。
    pub ignored_params: Vec<String>,
    /// 解釈できなかった**値**。空でも必ず出す（`TabPayload::invalid_values` と同じ約束）。
    /// CSV は落として Excel で開かれるので、`filters` の列名を間違えて
    /// 絞り込みが消えた全件 CSV を掴むと、後から気づく手段がまったく無い。
    pub invalid_values: Vec<crate::handlers::call_quality::query_audit::InvalidValue>,
}

fn csv_escape(s: &str) -> String {
    if s.contains(',') || s.contains('"') || s.contains('\n') || s.contains('\r') {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_string()
    }
}

fn csv_line<I: IntoIterator<Item = S>, S: AsRef<str>>(cells: I) -> String {
    cells
        .into_iter()
        .map(|c| csv_escape(c.as_ref()))
        .collect::<Vec<_>>()
        .join(",")
}

/// 絞り込み後の行を CSV_EXPORT_MAX_ROWS 件まで CSV 化する。
/// ブラウズと違って「絞り込み後の全件」が目的の機能なのでページングはしないが、
/// 無制限にはしない(理由は本ファイル冒頭のコメント参照)。
pub fn build_csv(data: &SheetData, f: &RowFilter, mut audit: ValueAudit) -> CsvExport {
    let rows = filter_rows(data, f, &mut audit);
    let matched_rows = rows.len();
    let truncated = matched_rows > CSV_EXPORT_MAX_ROWS;

    let mut out = String::new();
    out.push('\u{feff}'); // BOM
    out.push_str(&csv_line(data.header.iter().map(|s| s.as_str())));
    out.push_str("\r\n");
    let mut row_count = 0usize;
    for row in rows.iter().take(CSV_EXPORT_MAX_ROWS) {
        out.push_str(&csv_line(row.iter().map(|c| c.as_ref())));
        out.push_str("\r\n");
        row_count += 1;
    }

    CsvExport {
        csv: out,
        row_count,
        matched_rows,
        truncated,
        // ルータが後乗せする（ここは生のリクエストボディを知らない）
        ignored_params: Vec::new(),
        // 値の監査結果はこの関数の中で分かるので、ここで詰める。
        invalid_values: audit.into_vec(),
    }
}

/// ハンドラ本体(CSVエクスポート)。
pub async fn handle_export(
    store: &SheetStore,
    client: &SheetsClient,
    q: ExportQuery,
) -> Result<CsvExport> {
    check_sheet_allowed(&q.sheet)?;
    let (data, _from_cache) = store.get(client, &q.sheet).await?;
    Ok(build_csv(&data, &q.filter, ValueAudit::new()))
}

// -------------------------------------------------------------- クイック可視化

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Agg {
    Sum,
    Avg,
    Count,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct ChartQuery {
    pub sheet: String,
    #[serde(flatten)]
    pub filter: RowFilter,
    /// X軸(カテゴリ列)。
    pub x_col: String,
    /// Y軸(数値列)。agg=Count のときは無視してよい。
    pub y_col: Option<String>,
    pub agg: Agg,
    /// 上位何カテゴリ返すか(GAS版の選択肢: 10/20/50/100)。
    pub top_n: usize,
}

/// `Agg` には既定値が無い（`sum`/`avg`/`count` のどれかを必ず指定させる仕様）ので
/// `#[derive(Default)]` は付けない。**enum に既定を作ると、指定漏れが黙って
/// どれか1つの集計になる**（この作業で潰そうとしている無音ドロップそのもの）。
///
/// 代わりに手書きの `Default` を置く。用途は腐り検出テストだけ。
/// フィールドを足すとここがコンパイルエラーになるので、腐りが1段強く検出される。
impl Default for ChartQuery {
    fn default() -> Self {
        Self {
            sheet: String::new(),
            filter: RowFilter::default(),
            x_col: String::new(),
            y_col: None,
            agg: Agg::Count,
            top_n: 0,
        }
    }
}

crate::accepted_params!(ChartQuery, chart_query_accepted =>
    "sheet", "search", "filters", "x_col", "y_col", "agg", "top_n");

#[derive(Debug, Serialize)]
pub struct ChartBar {
    pub category: String,
    pub value: f64,
}

#[derive(Debug, Serialize)]
pub struct ChartData {
    /// 値の降順、上位 top_n 件。
    pub bars: Vec<ChartBar>,
    /// 絞り込み後(集計前)の行数。
    pub matched_rows: usize,
    /// カテゴリ数が top_n を超えて切った場合 true。
    pub truncated: bool,
}

/// フィルタ後の行を X軸列でグループ化し、Y軸列を sum/avg/count で集計する。
/// GAS版「クイック可視化」(index.html 1019-1059行)のサーバ側移植。
pub fn aggregate_chart(data: &SheetData, q: &ChartQuery, audit: &mut ValueAudit) -> Result<ChartData> {
    let xi = data
        .col(&q.x_col)
        .with_context(|| format!("X軸の列が見つかりません: {}", q.x_col))?;
    let yi = match q.agg {
        Agg::Count => None,
        _ => {
            let name = q.y_col.as_deref().unwrap_or("");
            Some(
                data.col(name)
                    .with_context(|| format!("Y軸の列が見つかりません: {name}"))?,
            )
        }
    };

    let rows = filter_rows(data, &q.filter, audit);
    let matched_rows = rows.len();

    let mut sum: HashMap<String, f64> = HashMap::new();
    let mut cnt: HashMap<String, u64> = HashMap::new();
    for row in &rows {
        let cat = row.get(xi).map(|s| s.as_ref()).unwrap_or("");
        if cat.is_empty() {
            continue;
        }
        let v = match yi {
            Some(i) => num(row.get(i).map(|s| s.as_ref()).unwrap_or("")),
            None => 0.0,
        };
        *sum.entry(cat.to_string()).or_insert(0.0) += v;
        *cnt.entry(cat.to_string()).or_insert(0) += 1;
    }

    let mut bars: Vec<ChartBar> = sum
        .keys()
        .map(|cat| {
            let value = match q.agg {
                Agg::Sum => sum[cat],
                Agg::Count => cnt[cat] as f64,
                Agg::Avg => {
                    let c = cnt[cat];
                    if c > 0 {
                        sum[cat] / c as f64
                    } else {
                        0.0
                    }
                }
            };
            ChartBar {
                category: cat.clone(),
                value,
            }
        })
        .collect();

    // 値の降順。同値はカテゴリ名で安定させる(約束4)。
    bars.sort_by(|a, b| {
        b.value
            .partial_cmp(&a.value)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.category.cmp(&b.category))
    });
    let top_n = q.top_n.max(1);
    let truncated = bars.len() > top_n;
    bars.truncate(top_n);

    Ok(ChartData {
        bars,
        matched_rows,
        truncated,
    })
}

/// ハンドラ本体(クイック可視化)。
pub async fn handle_chart(
    store: &SheetStore,
    client: &SheetsClient,
    q: ChartQuery,
) -> Result<TabPayload<ChartData>> {
    let started = Instant::now();
    check_sheet_allowed(&q.sheet)?;
    let (data, from_cache) = store.get(client, &q.sheet).await?;
    let mut audit = ValueAudit::new();
    let out = aggregate_chart(&data, &q, &mut audit)?;
    let sources = vec![SourceInfo {
        sheet: q.sheet.clone(),
        total_rows: data.rows.len(),
        matched_rows: out.matched_rows,
        from_cache,
        age_secs: data.fetched_at.elapsed().as_secs(),
    }];
    Ok(TabPayload {
        data: out,
        sources,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
        // こちらは**タブ側が詰める**。値の意味を知っているのはここだけ。
        invalid_values: audit.into_vec(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(header: &[&str], rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(Arc::from).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    fn sample() -> SheetData {
        sheet(
            &["owner", "prefecture", "dial"],
            vec![
                vec!["田中", "東京都", "100"],
                vec!["佐藤", "大阪府", "50"],
                vec!["鈴木", "東京都", "30"],
                vec!["高橋", "愛知県", "80"],
            ],
        )
    }

    fn q(sheet_name: &str) -> BrowseQuery {
        BrowseQuery {
            sheet: sheet_name.to_string(),
            filter: RowFilter::default(),
            sort_col: None,
            sort_dir: SortDir::Asc,
            page: 0,
            page_size: None,
        }
    }

    #[test]
    fn 許可されていないシート名は拒否される() {
        assert!(check_sheet_allowed("月次明細").is_ok());
        assert!(check_sheet_allowed("存在しないシート").is_err());
    }

    /// 画面が読んでいるシートは、必ずデータブラウザからも見られること。
    ///
    /// 2026-08-17 追加。**許可リストが2つあり、片方だけが更新されていた**。
    /// タブが読むシートの一覧(`sheets::KNOWN_SHEETS`)には入っているのに、
    /// データブラウザの一覧(`ALLOWED_SHEETS`)から16枚が漏れており、
    /// 「スプシをそのまま見る」画面から画面が使っているシートを引けなかった。
    ///
    /// 実害: 検証で生データの退避を取ったとき、その16枚だけが更新されず
    /// **7日前のデータと突合しかけた**（コンサル別ベンチマーク_統計 の
    /// n_cohort_total が 2,501 対 2,749 で 248件差）。
    ///
    /// 逆向き(ALLOWED にあって KNOWN に無い)は許す。メンバーマスタや
    /// 事前集計cube のように、タブが直接読まないが見たいシートがあるため。
    #[test]
    fn 画面が読むシートは全てデータブラウザから見られる() {
        let missing: Vec<&str> = crate::handlers::call_quality::sheets::KNOWN_SHEETS
            .iter()
            .copied()
            .filter(|s| !ALLOWED_SHEETS.contains(s))
            .collect();
        assert!(
            missing.is_empty(),
            "画面が読むのにデータブラウザから見られないシートがある: {missing:?}
             タブを増やしたら ALLOWED_SHEETS にも足すこと。"
        );
    }

    #[test]
    fn 全文検索は全列を横断してヒットする() {
        let d = sample();
        let mut query = q("月次明細");
        query.filter.search = Some("大阪".into());
        let r = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(r.matched_rows, 1);
        assert_eq!(r.rows[0][0], "佐藤");
    }

    #[test]
    fn 列フィルタは複数値をorで扱う() {
        let d = sample();
        let mut query = q("月次明細");
        query
            .filter
            .filters
            .insert("prefecture".into(), vec!["東京都".into(), "愛知県".into()]);
        let r = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(r.matched_rows, 3, "東京都 or 愛知県 の3件");
    }

    #[test]
    fn 数値列は数値としてソートされる() {
        let d = sample();
        let mut query = q("月次明細");
        query.sort_col = Some("dial".into());
        query.sort_dir = SortDir::Desc;
        let r = browse(&d, &query, &mut ValueAudit::new());
        // 文字列ソートなら "80" < "100" になってしまう(先頭文字'8'>'1')が、
        // 数値ソートなら 100,80,50,30 の順になる。
        assert_eq!(r.rows[0][2], "100");
        assert_eq!(r.rows[1][2], "80");
        assert_eq!(r.rows[3][2], "30");
    }

    #[test]
    fn 空値は昇順降順どちらでも末尾に来る() {
        let d = sheet(
            &["name", "score"],
            vec![
                vec!["A", "10"],
                vec!["B", ""],
                vec!["C", "5"],
            ],
        );
        let mut query = q("月次明細");
        query.sort_col = Some("score".into());
        query.sort_dir = SortDir::Desc;
        let r = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(r.rows.last().unwrap()[0], "B", "空値は降順でも末尾");
    }

    #[test]
    fn page_sizeが上限を超えたらクランプされtruncatedが立つ() {
        let d = sample();
        let mut query = q("月次明細");
        query.page_size = Some(MAX_PAGE_SIZE + 100);
        let r = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(r.page_size, MAX_PAGE_SIZE);
        assert!(r.truncated, "GAS版の「全表示」相当は提供しない(必ずクランプされる)");
    }

    #[test]
    fn ページングで先頭からn件ずつ返る() {
        let d = sample();
        let mut query = q("月次明細");
        query.page_size = Some(2);
        let p0 = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(p0.total_pages, 2);
        assert_eq!(p0.returned_rows, 2);
        query.page = 1;
        let p1 = browse(&d, &query, &mut ValueAudit::new());
        assert_eq!(p1.returned_rows, 2);
        assert_ne!(p0.rows, p1.rows);
    }

    #[test]
    fn csvはbomとカンマエスケープを含む() {
        let d = sheet(&["name", "note"], vec![vec!["田中", "a,b\"c"]]);
        let out = build_csv(&d, &RowFilter::default(), ValueAudit::new());
        assert!(out.csv.starts_with('\u{feff}'));
        assert!(out.csv.contains("\"a,b\"\"c\""), "カンマ・引用符を含む値はダブルクォートで囲みエスケープ");
        assert_eq!(out.row_count, 1);
        assert!(!out.truncated);
    }

    #[test]
    fn csvエクスポートは上限を超えるとtruncatedが立つ() {
        // CSV_EXPORT_MAX_ROWS を実際に超える件数を生成して検証する
        // (「生データ丸投げ」を防ぐ約束3の核心なので、弱いテストで済ませない)。
        let n = CSV_EXPORT_MAX_ROWS + 10;
        let rows: Vec<Vec<&str>> = (0..n).map(|_| vec!["x"]).collect();
        let d = sheet(&["v"], rows);
        let out = build_csv(&d, &RowFilter::default(), ValueAudit::new());
        assert_eq!(out.matched_rows, n, "絞り込み後の全件数は上限を超えていても正しく数える");
        assert_eq!(out.row_count, CSV_EXPORT_MAX_ROWS, "実際に書き出すのは上限まで");
        assert!(out.truncated);
    }

    #[test]
    fn クイック可視化はsumで降順集計する() {
        let d = sample();
        let query = ChartQuery {
            sheet: "月次明細".into(),
            filter: RowFilter::default(),
            x_col: "prefecture".into(),
            y_col: Some("dial".into()),
            agg: Agg::Sum,
            top_n: 10,
        };
        let out = aggregate_chart(&d, &query, &mut ValueAudit::new()).unwrap();
        assert_eq!(out.bars[0].category, "東京都");
        assert_eq!(out.bars[0].value, 130.0, "東京都=100+30");
        assert!(!out.truncated);
    }

    #[test]
    fn クイック可視化はtop_nで切りtruncatedが立つ() {
        let d = sample();
        let query = ChartQuery {
            sheet: "月次明細".into(),
            filter: RowFilter::default(),
            x_col: "prefecture".into(),
            y_col: Some("dial".into()),
            agg: Agg::Sum,
            top_n: 1,
        };
        let out = aggregate_chart(&d, &query, &mut ValueAudit::new()).unwrap();
        assert_eq!(out.bars.len(), 1);
        assert!(out.truncated);
    }

    #[test]
    fn クイック可視化のcountはy列不要() {
        let d = sample();
        let query = ChartQuery {
            sheet: "月次明細".into(),
            filter: RowFilter::default(),
            x_col: "prefecture".into(),
            y_col: None,
            agg: Agg::Count,
            top_n: 10,
        };
        let out = aggregate_chart(&d, &query, &mut ValueAudit::new()).unwrap();
        let tokyo = out.bars.iter().find(|b| b.category == "東京都").unwrap();
        assert_eq!(tokyo.value, 2.0, "東京都は2行");
    }

    #[test]
    fn 存在しない列名でaggregate_chartはエラーを返す() {
        let d = sample();
        let query = ChartQuery {
            sheet: "月次明細".into(),
            filter: RowFilter::default(),
            x_col: "存在しない列".into(),
            y_col: None,
            agg: Agg::Count,
            top_n: 10,
        };
        assert!(aggregate_chart(&d, &query, &mut ValueAudit::new()).is_err());
    }
    // ---- クエリの受け渡し方式を固定する（2026-08-16 追加） ----
    //
    // BrowseQuery は `filters: HashMap<String, Vec<String>>` を持つ。
    // これは **URL クエリ文字列（axum の `Query<T>` / serde_urlencoded）では復元できない**。
    // serde_urlencoded は「キー=値」の平坦な組しか扱えず、値が配列やマップになる型を
    // 拒否するため。`#[serde(flatten)]` を付けても解決しない。
    //
    // したがってこのタブのエンドポイントは **POST + JSON ボディ** で受けること。
    // GET + クエリ文字列にすると実行時に 400 で落ちる（コンパイルは通るので気づけない）。
    //
    // 下のテストは「JSON なら往復できる」ことを固定し、将来 GET に変えようとしたときに
    // ここを読んで気づけるようにするためのもの。
    #[test]
    fn browse_queryはjsonで往復できる() {
        let json = r#"{
            "sheet": "月次明細",
            "search": "東京",
            "filters": {"pipeline": ["A", "B"]},
            "sort_col": "call_count",
            "sort_dir": "desc",
            "page": 0,
            "page_size": 50
        }"#;
        let q: BrowseQuery = serde_json::from_str(json).expect("JSON なら復元できる");
        assert_eq!(q.sheet, "月次明細");
        assert_eq!(q.filter.search.as_deref(), Some("東京"));
        assert_eq!(
            q.filter.filters.get("pipeline").map(|v| v.len()),
            Some(2),
            "同一列の複数値(OR)が保持されること"
        );
    }

    #[test]
    fn filtersが空でも復元できる() {
        // 画面の初期表示（絞り込みなし）で落ちないこと
        let q: BrowseQuery =
            serde_json::from_str(r#"{"sheet":"月次明細"}"#).expect("最小構成で復元できる");
        assert!(q.filter.filters.is_empty());
        assert!(q.filter.search.is_none());
    }


    // ---- 存在しない列名を無音で無視しない（2026-08-17 追加） ----

    #[test]
    fn 存在しない列で絞ると絞り込みが消えることを名指しする() {
        // **これが一番危ない**。`filter_map(|(name,_)| data.col(name).map(..))` は
        // 列が見つからないと絞り込みごと落とすので、400 にも 0件にもならず
        // **絞り込みの消えた全件**が 200 で返る。
        // 動機になった `deals_status=NONSENSE`（絞ったつもりで全件）と同じ形。
        let d = sample();
        let mut query = q("月次明細");
        query.filter.filters.insert("prefecure".into(), vec!["東京都".into()]);
        let mut a = ValueAudit::new();
        let r = browse(&d, &query, &mut a);
        assert_eq!(r.matched_rows, 4, "挙動は変えない（全件のまま返す）");
        let v = a.into_vec();
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].param, "filters");
        assert_eq!(v[0].given, "prefecure");
        assert_eq!(v[0].used, None);
        assert!(v[0].message.contains("無視"));
    }

    #[test]
    fn 存在しない列でソートしても並べ替えたと言わない() {
        // `sort_col` は応答にそのまま返るので、効いていないことが画面から分からない。
        let d = sample();
        let mut query = q("月次明細");
        query.sort_col = Some("dail".into()); // dial の打ち間違い
        let mut a = ValueAudit::new();
        let r = browse(&d, &query, &mut a);
        assert_eq!(r.rows[0][0], "田中", "並べ替えは行われずシートの元順のまま");
        let v = a.into_vec();
        assert_eq!(v.len(), 1, "{v:?}");
        assert_eq!(v[0].param, "sort_col");
        assert_eq!(v[0].given, "dail");
    }

    #[test]
    fn 存在する列名では何も報告しない() {
        // **陰性対照**
        let d = sample();
        let mut query = q("月次明細");
        query.filter.filters.insert("prefecture".into(), vec!["東京都".into()]);
        query.sort_col = Some("dial".into());
        let mut a = ValueAudit::new();
        let r = browse(&d, &query, &mut a);
        assert_eq!(r.matched_rows, 2);
        assert!(a.is_empty(), "正しい列名では黙る");

        // 値が空の列フィルタは「絞らない」意味なので、列が無くても黙る
        let mut query = q("月次明細");
        query.filter.filters.insert("nonexistent".into(), vec![]);
        let mut a = ValueAudit::new();
        browse(&d, &query, &mut a);
        assert!(a.is_empty(), "値が空の絞り込みは元々無視される仕様");
    }

    #[test]
    fn 不明列の報告は並びが安定する() {
        // HashMap の反復順をそのまま出さない（約束4）
        let d = sample();
        let names = ["zzz", "aaa", "mmm"];
        let mut query = q("月次明細");
        for n in names {
            query.filter.filters.insert(n.into(), vec!["x".into()]);
        }
        let mut a = ValueAudit::new();
        browse(&d, &query, &mut a);
        let got: Vec<String> = a.into_vec().into_iter().map(|v| v.given).collect();
        assert_eq!(got, vec!["aaa".to_string(), "mmm".to_string(), "zzz".to_string()]);
    }

    #[test]
    fn csvエクスポートでも不明列を名指しする() {
        // CSV は落として Excel で開かれるので、絞り込みが消えた全件 CSV を掴むと
        // 後から気づく手段がまったく無い。
        let d = sample();
        let mut f = RowFilter::default();
        f.filters.insert("prefecure".into(), vec!["東京都".into()]);
        let out = build_csv(&d, &f, ValueAudit::new());
        assert_eq!(out.row_count, 4, "挙動は変えない");
        assert_eq!(out.invalid_values.len(), 1);
        assert_eq!(out.invalid_values[0].given, "prefecure");

        // 陰性対照
        let out = build_csv(&d, &RowFilter::default(), ValueAudit::new());
        assert!(out.invalid_values.is_empty());
    }

    #[test]
    fn クイック可視化でも不明列を名指しする() {
        let d = sample();
        let mut query = ChartQuery {
            sheet: "月次明細".into(),
            filter: RowFilter::default(),
            x_col: "prefecture".into(),
            y_col: None,
            agg: Agg::Count,
            top_n: 10,
        };
        query.filter.filters.insert("prefecure".into(), vec!["東京都".into()]);
        let mut a = ValueAudit::new();
        let out = aggregate_chart(&d, &query, &mut a).unwrap();
        assert_eq!(out.matched_rows, 4, "挙動は変えない");
        assert_eq!(a.into_vec().len(), 1);
    }
}
