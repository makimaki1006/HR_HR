//! 案件マネジメント（GAS 版 `page-p15`、未来案件マネジメント）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p15">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP15FuturePipeline` / `_p15TryDraw` / `_p15DrawSummaryCards` /
//!          `_p15DrawMonthChart` / `_p15DrawRevenueChart` / `_p15DrawMatrix` /
//!          `_p15DrawDealsTable` / `_p15GetMonthly` / `_p15GetDeals`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getFuturePipelineMonthly` / `getFuturePipelineSummary` / `getFuturePipelineDeals`）
//!
//! 用途: コンサル担当者が「来月、再来月、半年後の案件が今どうなるか、何をすべきか」を
//! 1画面で確認するマネジメントタブ。**担当者セレクタ駆動で、上部フィルタ
//! （期間/PL/メンバー/都道府県）は反映しない**（GAS 版と同じ、index.html の注記どおり）。
//!
//! ------------------------------------------------------------------
//! 使用シート
//! ------------------------------------------------------------------
//! 「コンサル未来案件_月次」（consultant × 今月+1〜+6 の月次集計）
//!   列: consultant_id, consultant_name, year_month, holding_count, expiring_count,
//!       high_risk_count, holding_amount, expiring_amount, revenue_at_risk,
//!       plan_breakdown_json
//! 「コンサル未来案件_担当者サマリ」（consultant 別の半年合計）
//!   列: consultant_id, consultant_name, total_holding, total_expiring_6m,
//!       total_high_risk_6m, total_amount_at_risk_6m, max_month_expiring,
//!       max_month_amount_at_risk
//! 「コンサル未来案件_Deal一覧」（半年以内満了予定・稼働中の案件）
//!   列: deal_id, consultant_id, consultant_name, customer_label, prefecture,
//!       contract_start_date, contract_expiration_date, days_to_expiration,
//!       contract_period, contract_plan, contract_type, amount, churn_proba_90d,
//!       risk_level, latest_nps, latest_sufficiency, action_priority
//!
//! `action_priority`（immediate/high/medium/watch）は Python バッチ
//! （`consulting_future_pipeline.py`）が算出済みの列をそのまま使う。ロジックの再掲:
//!   immediate = 残30日以内 & churn_proba>=50%
//!   high      = 残60日以内 & churn_proba>=50% OR 残30日以内 & 高金額(>500K)
//!   medium    = 残90日以内 & churn_proba>=30%
//!   watch     = それ以外
//! LightGBM churn_proba_90d はリーク除去後 AUC≈0.72 の粗い補助であり、確定値ではない
//! （index.html 1937行の注記どおり）。
//!
//! ------------------------------------------------------------------
//! GAS 版との計算差分（意図的な変更点）
//! ------------------------------------------------------------------
//! 月別チャート・Revenue at Risk チャートの「中立」層（`expiring_neutral_count` /
//! `expiring_neutral_amount`）は GAS 側でチャート描画直前に計算していたのを
//! `MonthlyPoint` の集計時点に前出しした（サーバ側で完結させる、という約束5のため）。
//! 値そのものは GAS `max(0, exp - hr)` / `max(0, expAmt - risk)` と同じ。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. 担当者名の部分一致検索（GAS `p15-search`）
//!    → 実装しない。`consultants` は全件返す（実データで数十名程度）ので、
//!      検索・絞り込みはフロント側の責務とする（p14 と同方針）。
//! 2. 優先度チェックボックスの初期状態（GAS は immediate/high/medium が既定 checked、
//!    watch は既定 unchecked）
//!    → サーバ実装の対象外。これは DOM の初期表示状態であり、
//!      `priority` 未指定時は素の4種全件を返す（`parse_priorities` 参照）。
//!      既定でどれを表示するかの UI 判断はフロント側の責務。
//! 3. マトリクスパネルの表示/非表示切替（GAS は「全担当者ビュー」時のみ表示）
//!    → サーバは常に `matrix` を返す。表示制御はフロント側の責務
//!      （担当者を選択しているかどうかは `selected_consultant_id` で判定できる）。
//! 4. `plan_breakdown_json`（プラン内訳）列
//!    → 未移植。GAS 版 P15 画面にもこの列を描画する箇所が存在しない
//!      （Code.gs コメントに列挙されているだけの未使用列）。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_MONTHLY: &str = "コンサル未来案件_月次";
const SHEET_SUMMARY: &str = "コンサル未来案件_担当者サマリ";
const SHEET_DEALS: &str = "コンサル未来案件_Deal一覧";

/// アクション必要 Deal 一覧の表示上限（GAS `deals.slice(0, 100)`）
const DEALS_TABLE_LIMIT: usize = 100;

const ALL_PRIORITIES: [&str; 4] = ["immediate", "high", "medium", "watch"];

// ---------------------------------------------------------------- 小道具

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.replace(',', "").parse::<f64>().ok()
    }
}

fn consultant_label(name: &str, id: &str) -> String {
    let n = name.trim();
    if n.is_empty() { id.to_string() } else { n.to_string() }
}

/// action_priority の並び順。未知の値は末尾（GAS `prioOrder[...] != null ? ... : 9`）
fn priority_rank(p: &str) -> u8 {
    match p {
        "immediate" => 0,
        "high" => 1,
        "medium" => 2,
        "watch" => 3,
        _ => 9,
    }
}

// ================================================================== 月次

#[derive(Debug, Serialize, Clone, Default)]
pub struct MonthlyPoint {
    pub year_month: String,
    pub holding_count: f64,
    pub expiring_count: f64,
    pub high_risk_count: f64,
    pub holding_amount: f64,
    pub expiring_amount: f64,
    pub revenue_at_risk: f64,
    /// 満了予定件数のうち高リスクでない件数（スタック棒の下層。GAS `max(0, exp-hr)`）
    pub expiring_neutral_count: f64,
    /// 満了金額のうち高リスクでない金額（スタック棒の下層。GAS `max(0, expAmt-risk)`）
    pub expiring_neutral_amount: f64,
}

impl MonthlyPoint {
    fn finalize(mut self) -> Self {
        self.expiring_neutral_count = (self.expiring_count - self.high_risk_count).max(0.0);
        self.expiring_neutral_amount = (self.expiring_amount - self.revenue_at_risk).max(0.0);
        self
    }
}

/// 「コンサル未来案件_月次」から対象 consultant（未指定なら全担当合算）の月次行を
/// 年月昇順で返す。GAS `_p15GetMonthly` の移植。
fn monthly_for(data: &SheetData, cid: Option<&str>) -> Vec<MonthlyPoint> {
    let mut acc: HashMap<String, MonthlyPoint> = HashMap::new();
    for row in &data.rows {
        if let Some(id) = cid {
            if data.get(row, "consultant_id") != id {
                continue;
            }
        }
        let ym = data.get(row, "year_month").to_string();
        if ym.is_empty() {
            continue;
        }
        let e = acc.entry(ym.clone()).or_insert_with(|| MonthlyPoint { year_month: ym, ..Default::default() });
        e.holding_count += num(data.get(row, "holding_count"));
        e.expiring_count += num(data.get(row, "expiring_count"));
        e.high_risk_count += num(data.get(row, "high_risk_count"));
        e.holding_amount += num(data.get(row, "holding_amount"));
        e.expiring_amount += num(data.get(row, "expiring_amount"));
        e.revenue_at_risk += num(data.get(row, "revenue_at_risk"));
    }
    let mut v: Vec<MonthlyPoint> = acc.into_values().map(MonthlyPoint::finalize).collect();
    v.sort_by(|a, b| a.year_month.cmp(&b.year_month));
    v
}

// ================================================================== 担当者サマリ/セレクタ

#[derive(Debug, Serialize, Clone)]
pub struct ConsultantOption {
    pub consultant_id: String,
    pub consultant_name: String,
    pub total_amount_at_risk_6m: f64,
    pub total_high_risk_6m: f64,
}

/// 担当者セレクタ用の一覧。`total_amount_at_risk_6m` 降順（GAS `_p15PopulateSelector`）。
/// 名前の部分一致検索はフロント側の責務（ファイル冒頭「未実装」参照）。
pub fn build_consultant_options(summary: &SheetData) -> Vec<ConsultantOption> {
    let mut v: Vec<ConsultantOption> = summary
        .rows
        .iter()
        .map(|r| {
            let consultant_id = summary.get(r, "consultant_id").to_string();
            ConsultantOption {
                consultant_name: consultant_label(summary.get(r, "consultant_name"), &consultant_id),
                total_amount_at_risk_6m: num(summary.get(r, "total_amount_at_risk_6m")),
                total_high_risk_6m: num(summary.get(r, "total_high_risk_6m")),
                consultant_id,
            }
        })
        .collect();
    v.sort_by(|a, b| {
        b.total_amount_at_risk_6m
            .partial_cmp(&a.total_amount_at_risk_6m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.consultant_id.cmp(&b.consultant_id))
    });
    v
}

// ================================================================== KPIスコアカード

#[derive(Debug, Serialize)]
pub struct SummaryKpis {
    /// 「現在保有 Deal」。選択担当者ありならサマリのその人の値、無ければ全担当合算
    pub total_holding: f64,
    /// 「半年合計 満了予定」。月次(対象範囲)の expiring_count 合計
    pub total_expiring_6m: f64,
    /// 「半年合計 高リスク」。月次の high_risk_count 合計
    pub total_high_risk_6m: f64,
    /// 「Revenue at Risk」。月次の revenue_at_risk 合計
    pub revenue_at_risk_6m: f64,
    /// action_priority=immediate の Deal 件数（優先度フィルタ適用前、対象担当者スコープ内）
    pub immediate_count: usize,
    /// action_priority=high の Deal 件数（同上）
    pub high_count: usize,
}

/// GAS `_p15DrawSummaryCards` の移植。`total_holding` だけ月次でなくサマリシート由来
/// （月次には holding_count が「その月初時点」の値として月ごとにあるため、KPIカードは
/// サマリの `total_holding`＝直近値を使う。GAS も同じ二重ソース構成）。
pub fn build_summary_kpis(monthly: &[MonthlyPoint], deals_in_scope: &[DealActionRow], summary: &SheetData, cid: Option<&str>) -> SummaryKpis {
    let total_holding = match cid {
        Some(id) => summary
            .rows
            .iter()
            .find(|r| summary.get(r, "consultant_id") == id)
            .map(|r| num(summary.get(r, "total_holding")))
            .unwrap_or(0.0),
        None => summary.rows.iter().map(|r| num(summary.get(r, "total_holding"))).sum(),
    };
    SummaryKpis {
        total_holding,
        total_expiring_6m: monthly.iter().map(|m| m.expiring_count).sum(),
        total_high_risk_6m: monthly.iter().map(|m| m.high_risk_count).sum(),
        revenue_at_risk_6m: monthly.iter().map(|m| m.revenue_at_risk).sum(),
        immediate_count: deals_in_scope.iter().filter(|d| d.action_priority == "immediate").count(),
        high_count: deals_in_scope.iter().filter(|d| d.action_priority == "high").count(),
    }
}

// ================================================================== コンサル×月 マトリクス

#[derive(Debug, Serialize)]
pub struct MatrixCell {
    pub year_month: String,
    pub holding_count: f64,
    pub expiring_count: f64,
    pub high_risk_count: f64,
}

#[derive(Debug, Serialize)]
pub struct MatrixRow {
    pub consultant_id: String,
    pub consultant_name: String,
    /// `year_months` と同じ並び。データが無い月は0埋め（GAS と同じ）
    pub cells: Vec<MatrixCell>,
    pub total_expiring_6m: f64,
    pub total_high_risk_6m: f64,
    pub total_amount_at_risk_6m: f64,
}

#[derive(Debug, Serialize)]
pub struct MatrixPanel {
    /// 列ヘッダ（年月昇順）
    pub year_months: Vec<String>,
    /// consultant 並びは `total_amount_at_risk_6m` 降順（GAS `_p15DrawMatrix`）
    pub rows: Vec<MatrixRow>,
}

/// GAS `_p15DrawMatrix` の移植。表示/非表示（全担当者ビュー限定）はフロント側の責務
/// （ファイル冒頭「未実装」参照）なので、ここでは常に全データを返す。
pub fn build_matrix(monthly: &SheetData, summary: &SheetData) -> MatrixPanel {
    let mut year_months: Vec<String> = monthly
        .rows
        .iter()
        .map(|r| monthly.get(r, "year_month").to_string())
        .filter(|s| !s.is_empty())
        .collect();
    year_months.sort();
    year_months.dedup();

    let mut by_consultant: HashMap<String, HashMap<String, (f64, f64, f64)>> = HashMap::new();
    for r in &monthly.rows {
        let cid = monthly.get(r, "consultant_id").to_string();
        if cid.is_empty() {
            continue;
        }
        let ym = monthly.get(r, "year_month").to_string();
        by_consultant.entry(cid).or_default().insert(
            ym,
            (
                num(monthly.get(r, "holding_count")),
                num(monthly.get(r, "expiring_count")),
                num(monthly.get(r, "high_risk_count")),
            ),
        );
    }

    let mut rows: Vec<MatrixRow> = summary
        .rows
        .iter()
        .map(|r| {
            let consultant_id = summary.get(r, "consultant_id").to_string();
            let cell_map = by_consultant.get(&consultant_id);
            let cells: Vec<MatrixCell> = year_months
                .iter()
                .map(|ym| {
                    let (holding, expiring, high_risk) = cell_map.and_then(|m| m.get(ym)).copied().unwrap_or((0.0, 0.0, 0.0));
                    MatrixCell { year_month: ym.clone(), holding_count: holding, expiring_count: expiring, high_risk_count: high_risk }
                })
                .collect();
            MatrixRow {
                consultant_name: consultant_label(summary.get(r, "consultant_name"), &consultant_id),
                total_expiring_6m: num(summary.get(r, "total_expiring_6m")),
                total_high_risk_6m: num(summary.get(r, "total_high_risk_6m")),
                total_amount_at_risk_6m: num(summary.get(r, "total_amount_at_risk_6m")),
                consultant_id,
                cells,
            }
        })
        .collect();
    rows.sort_by(|a, b| {
        b.total_amount_at_risk_6m
            .partial_cmp(&a.total_amount_at_risk_6m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.consultant_id.cmp(&b.consultant_id))
    });

    MatrixPanel { year_months, rows }
}

// ================================================================== アクション必要Deal一覧

#[derive(Debug, Serialize, Clone)]
pub struct DealActionRow {
    pub deal_id: String,
    pub consultant_id: String,
    pub consultant_name: String,
    pub customer_label: String,
    pub prefecture: String,
    pub contract_expiration_date: String,
    pub days_to_expiration: Option<f64>,
    pub contract_plan: String,
    pub contract_type: String,
    pub amount: f64,
    /// 0..100(%)。churn_proba_90d が空文字なら None(0%と誤読させない)
    pub churn_proba_90d_pct: Option<f64>,
    pub risk_level: String,
    pub latest_nps: Option<f64>,
    pub latest_sufficiency: Option<f64>,
    /// Python バッチ算出済み。空なら "watch" 扱い（GAS `String(r.action_priority || 'watch')`）
    pub action_priority: String,
}

/// 「コンサル未来案件_Deal一覧」から対象 consultant（未指定なら全件）の行を返す
/// （並び替え前。GAS `_p15GetDeals`）。
fn deals_for(data: &SheetData, cid: Option<&str>) -> Vec<DealActionRow> {
    data.rows
        .iter()
        .filter(|r| cid.map(|id| data.get(r, "consultant_id") == id).unwrap_or(true))
        .map(|r| {
            let action_priority = {
                let p = data.get(r, "action_priority").trim();
                if p.is_empty() { "watch".to_string() } else { p.to_string() }
            };
            DealActionRow {
                deal_id: data.get(r, "deal_id").to_string(),
                consultant_id: data.get(r, "consultant_id").to_string(),
                consultant_name: data.get(r, "consultant_name").to_string(),
                customer_label: data.get(r, "customer_label").to_string(),
                prefecture: data.get(r, "prefecture").to_string(),
                contract_expiration_date: data.get(r, "contract_expiration_date").to_string(),
                days_to_expiration: opt_num(data.get(r, "days_to_expiration")),
                contract_plan: data.get(r, "contract_plan").to_string(),
                contract_type: data.get(r, "contract_type").to_string(),
                amount: num(data.get(r, "amount")),
                churn_proba_90d_pct: opt_num(data.get(r, "churn_proba_90d")).map(|v| v * 100.0),
                risk_level: data.get(r, "risk_level").to_string(),
                latest_nps: opt_num(data.get(r, "latest_nps")),
                latest_sufficiency: opt_num(data.get(r, "latest_sufficiency")),
                action_priority,
            }
        })
        .collect()
}

/// `priority` クエリパラメータ（カンマ区切り）をパースする。
/// 未指定・空・全部空文字なら4種全部を返す（GAS `checked.length === 0` と同じ規則。
/// ただし GAS の DOM 初期状態は watch のみ unchecked——ファイル冒頭「未実装2」参照）。
fn parse_priorities(raw: Option<&str>) -> Vec<String> {
    let v: Vec<String> = raw
        .unwrap_or("")
        .split(',')
        .map(|t| t.trim().to_string())
        .filter(|t| !t.is_empty())
        .collect();
    if v.is_empty() {
        ALL_PRIORITIES.iter().map(|s| s.to_string()).collect()
    } else {
        v
    }
}

#[derive(Debug, Serialize)]
pub struct DealsPanel {
    pub rows: Vec<DealActionRow>,
    /// 優先度フィルタ適用後・表示上限適用前の件数
    pub total: usize,
    /// `DEALS_TABLE_LIMIT` で切ったか
    pub truncated: bool,
    pub limit: usize,
    pub applied_priorities: Vec<String>,
}

/// 優先度フィルタ → 並び替え（優先度順→満了日昇順）→ 上限カット。GAS `_p15DrawDealsTable`。
pub fn build_deals_panel(all: Vec<DealActionRow>, priorities: &[String]) -> DealsPanel {
    let mut filtered: Vec<DealActionRow> = all
        .into_iter()
        .filter(|d| priorities.iter().any(|p| p == &d.action_priority))
        .collect();
    filtered.sort_by(|a, b| {
        priority_rank(&a.action_priority)
            .cmp(&priority_rank(&b.action_priority))
            .then_with(|| a.contract_expiration_date.cmp(&b.contract_expiration_date))
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let total = filtered.len();
    let truncated = total > DEALS_TABLE_LIMIT;
    filtered.truncate(DEALS_TABLE_LIMIT);
    DealsPanel {
        rows: filtered,
        total,
        truncated,
        limit: DEALS_TABLE_LIMIT,
        applied_priorities: priorities.to_vec(),
    }
}

// ================================================================== 全体

#[derive(Debug, Default, Deserialize)]
pub struct P15Query {
    /// 選択 consultant_id。未指定または空文字なら「全担当者」ビュー(GAS 既定 `P15_VIEW_ALL=true`)
    pub consultant_id: Option<String>,
    /// action_priority フィルタ(カンマ区切り、`immediate,high,medium,watch` の部分集合)。
    /// 未指定なら4種全部
    pub priority: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct P15Data {
    pub kpis: SummaryKpis,
    pub monthly: Vec<MonthlyPoint>,
    pub consultants: Vec<ConsultantOption>,
    pub matrix: MatrixPanel,
    pub deals: DealsPanel,
    pub selected_consultant_id: Option<String>,
}

async fn load(
    client: &SheetsClient,
    store: &SheetStore,
    name: &str,
    sources: &mut Vec<SourceInfo>,
) -> Result<Arc<SheetData>> {
    let (d, from_cache) = store.get(client, name).await?;
    sources.push(SourceInfo {
        sheet: name.to_string(),
        total_rows: d.rows.len(),
        matched_rows: d.rows.len(),
        from_cache,
        age_secs: d.fetched_at.elapsed().as_secs(),
    });
    Ok(d)
}

fn set_matched(sources: &mut [SourceInfo], sheet: &str, n: usize) {
    if let Some(s) = sources.iter_mut().find(|s| s.sheet == sheet) {
        s.matched_rows = n;
    }
}

pub async fn handle(client: &SheetsClient, store: &SheetStore, q: P15Query) -> Result<TabPayload<P15Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let monthly_data = load(client, store, SHEET_MONTHLY, &mut sources).await?;
    let summary_data = load(client, store, SHEET_SUMMARY, &mut sources).await?;
    let deals_data = load(client, store, SHEET_DEALS, &mut sources).await?;

    let cid = q.consultant_id.as_deref().map(str::trim).filter(|s| !s.is_empty());

    let monthly = monthly_for(&monthly_data, cid);
    let deals_in_scope = deals_for(&deals_data, cid);
    let priorities = parse_priorities(q.priority.as_deref());
    let deals_panel = build_deals_panel(deals_in_scope.clone(), &priorities);

    let kpis = build_summary_kpis(&monthly, &deals_in_scope, &summary_data, cid);
    let consultants = build_consultant_options(&summary_data);
    let matrix = build_matrix(&monthly_data, &summary_data);

    set_matched(&mut sources, SHEET_MONTHLY, monthly_data.rows.iter().filter(|r| cid.map(|id| monthly_data.get(r, "consultant_id") == id).unwrap_or(true)).count());
    set_matched(&mut sources, SHEET_DEALS, deals_in_scope.len());

    Ok(TabPayload {
        data: P15Data {
            kpis,
            monthly,
            consultants,
            matrix,
            deals: deals_panel,
            selected_consultant_id: cid.map(|s| s.to_string()),
        },
        sources,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

// ================================================================== テスト

#[cfg(test)]
mod tests {
    use super::*;

    fn arc_row(vals: &[&str]) -> Vec<Arc<str>> {
        vals.iter().map(|v| Arc::from(*v)).collect()
    }

    const MONTHLY_HEADER: [&str; 10] = [
        "consultant_id", "consultant_name", "year_month", "holding_count", "expiring_count",
        "high_risk_count", "holding_amount", "expiring_amount", "revenue_at_risk", "plan_breakdown_json",
    ];

    fn monthly_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: MONTHLY_HEADER.iter().map(|s| s.to_string()).collect(),
            rows: rows.into_iter().map(|r| arc_row(&r)).collect(),
            fetched_at: Instant::now(),
        }
    }

    const SUMMARY_HEADER: [&str; 8] = [
        "consultant_id", "consultant_name", "total_holding", "total_expiring_6m",
        "total_high_risk_6m", "total_amount_at_risk_6m", "max_month_expiring", "max_month_amount_at_risk",
    ];

    fn summary_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: SUMMARY_HEADER.iter().map(|s| s.to_string()).collect(),
            rows: rows.into_iter().map(|r| arc_row(&r)).collect(),
            fetched_at: Instant::now(),
        }
    }

    const DEALS_HEADER: [&str; 16] = [
        "deal_id", "consultant_id", "consultant_name", "customer_label", "prefecture",
        "contract_start_date", "contract_expiration_date", "days_to_expiration",
        "contract_period", "contract_plan", "contract_type", "amount", "churn_proba_90d",
        "risk_level", "latest_nps", "action_priority",
    ];

    fn deals_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: DEALS_HEADER.iter().map(|s| s.to_string()).collect(),
            rows: rows.into_iter().map(|r| arc_row(&r)).collect(),
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn 月次は担当者指定で絞り込まれ年月昇順になる() {
        let d = monthly_sheet(vec![
            vec!["1", "田中", "2026-08", "10", "2", "1", "100", "50", "20", "{}"],
            vec!["1", "田中", "2026-07", "12", "3", "0", "110", "60", "0", "{}"],
            vec!["2", "鈴木", "2026-07", "5", "1", "1", "50", "30", "30", "{}"],
        ]);
        let m = monthly_for(&d, Some("1"));
        assert_eq!(m.len(), 2);
        assert_eq!(m[0].year_month, "2026-07", "年月昇順");
        assert_eq!(m[1].year_month, "2026-08");
    }

    #[test]
    fn 月次は担当者未指定で全員合算する() {
        let d = monthly_sheet(vec![
            vec!["1", "田中", "2026-07", "10", "2", "1", "100", "50", "20", "{}"],
            vec!["2", "鈴木", "2026-07", "5", "1", "0", "50", "30", "0", "{}"],
        ]);
        let m = monthly_for(&d, None);
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].holding_count, 15.0);
        assert_eq!(m[0].expiring_count, 3.0);
    }

    #[test]
    fn 中立層は満了からハイリスクを引いた値でマイナスにならない() {
        let d = monthly_sheet(vec![vec!["1", "田中", "2026-07", "10", "2", "5", "100", "50", "80", "{}"]]);
        let m = monthly_for(&d, Some("1"));
        // high_risk(5) > expiring(2) のような矛盾データでも 0 未満にしない
        assert_eq!(m[0].expiring_neutral_count, 0.0);
        assert_eq!(m[0].expiring_neutral_amount, 0.0);
    }

    #[test]
    fn 優先度未指定は4種全部を返す() {
        let p = parse_priorities(None);
        assert_eq!(p, vec!["immediate", "high", "medium", "watch"]);
    }

    #[test]
    fn 優先度指定はカンマ区切りでパースされる() {
        let p = parse_priorities(Some("immediate,high"));
        assert_eq!(p, vec!["immediate", "high"]);
    }

    #[test]
    fn deal一覧は優先度順_満了日昇順で並ぶ() {
        let deals = vec![
            DealActionRow { deal_id: "1".into(), consultant_id: "1".into(), consultant_name: "田中".into(), customer_label: "A".into(), prefecture: "".into(), contract_expiration_date: "2026-09-01".into(), days_to_expiration: Some(20.0), contract_plan: "".into(), contract_type: "".into(), amount: 0.0, churn_proba_90d_pct: Some(60.0), risk_level: "high".into(), latest_nps: None, latest_sufficiency: None, action_priority: "high".into() },
            DealActionRow { deal_id: "2".into(), consultant_id: "1".into(), consultant_name: "田中".into(), customer_label: "B".into(), prefecture: "".into(), contract_expiration_date: "2026-08-20".into(), days_to_expiration: Some(5.0), contract_plan: "".into(), contract_type: "".into(), amount: 0.0, churn_proba_90d_pct: Some(80.0), risk_level: "critical".into(), latest_nps: None, latest_sufficiency: None, action_priority: "immediate".into() },
        ];
        let panel = build_deals_panel(deals, &["immediate".to_string(), "high".to_string(), "medium".to_string(), "watch".to_string()]);
        assert_eq!(panel.rows[0].deal_id, "2", "immediateがhighより先(満了日が近くても優先度が先)");
        assert_eq!(panel.rows[1].deal_id, "1");
    }

    #[test]
    fn deal一覧は優先度フィルタで絞り込める() {
        let d = deals_sheet(vec![
            vec!["1", "1", "田中", "A", "東京都", "", "2026-09-01", "20", "", "", "", "100", "0.6", "high", "", "high"],
            vec!["2", "1", "田中", "B", "東京都", "", "2026-08-20", "5", "", "", "", "200", "0.8", "critical", "", "immediate"],
        ]);
        let all = deals_for(&d, None);
        let panel = build_deals_panel(all, &["immediate".to_string()]);
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].deal_id, "2");
    }

    #[test]
    fn action_priorityが空文字ならwatch扱い() {
        let d = deals_sheet(vec![vec!["1", "1", "田中", "A", "", "", "", "", "", "", "", "0", "", "", "", ""]]);
        let rows = deals_for(&d, None);
        assert_eq!(rows[0].action_priority, "watch");
    }

    #[test]
    fn churn確率の空文字はnoneで0パーセントと誤読させない() {
        let d = deals_sheet(vec![vec!["1", "1", "田中", "A", "", "", "", "", "", "", "", "0", "", "", "", "watch"]]);
        let rows = deals_for(&d, None);
        assert_eq!(rows[0].churn_proba_90d_pct, None);
    }

    #[test]
    fn 保有件数は担当者選択時サマリのその人の値を使う() {
        let summary = summary_sheet(vec![
            vec!["1", "田中", "10", "3", "1", "50000", "2026-07", "2026-07:50000"],
            vec!["2", "鈴木", "20", "5", "2", "80000", "2026-08", "2026-08:80000"],
        ]);
        let monthly: Vec<MonthlyPoint> = vec![];
        let kpis = build_summary_kpis(&monthly, &[], &summary, Some("1"));
        assert_eq!(kpis.total_holding, 10.0);
    }

    #[test]
    fn 保有件数は担当者未選択で全員合算する() {
        let summary = summary_sheet(vec![
            vec!["1", "田中", "10", "3", "1", "50000", "2026-07", "2026-07:50000"],
            vec!["2", "鈴木", "20", "5", "2", "80000", "2026-08", "2026-08:80000"],
        ]);
        let monthly: Vec<MonthlyPoint> = vec![];
        let kpis = build_summary_kpis(&monthly, &[], &summary, None);
        assert_eq!(kpis.total_holding, 30.0);
    }

    #[test]
    fn マトリクスはconsultant並びがrevenue_at_risk降順で欠測月は0埋め() {
        // consultant "2" には 2026-08 の行が無い(欠測) → 0埋めされることを確認する
        let monthly = monthly_sheet(vec![
            vec!["1", "田中", "2026-07", "10", "2", "1", "100", "50", "20", "{}"],
            vec!["1", "田中", "2026-08", "9", "1", "0", "90", "40", "0", "{}"],
            vec!["2", "鈴木", "2026-07", "5", "1", "0", "50", "30", "0", "{}"],
        ]);
        let summary = summary_sheet(vec![
            vec!["1", "田中", "10", "3", "1", "20000", "2026-07", "2026-07:20000"],
            vec!["2", "鈴木", "5", "1", "0", "90000", "2026-07", "2026-07:90000"],
        ]);
        let matrix = build_matrix(&monthly, &summary);
        assert_eq!(matrix.year_months, vec!["2026-07", "2026-08"]);
        assert_eq!(matrix.rows[0].consultant_id, "2", "revenue_at_risk_6mが大きい鈴木が先");
        let tanaka = matrix.rows.iter().find(|r| r.consultant_id == "1").unwrap();
        let aug = tanaka.cells.iter().find(|c| c.year_month == "2026-08").unwrap();
        assert_eq!(aug.holding_count, 9.0);
        let suzuki = matrix.rows.iter().find(|r| r.consultant_id == "2").unwrap();
        let suzuki_aug = suzuki.cells.iter().find(|c| c.year_month == "2026-08").unwrap();
        assert_eq!(suzuki_aug.holding_count, 0.0, "データが無い月は0埋め");
    }

    #[test]
    fn 表示上限で切ったらtruncatedが立つ() {
        let deals: Vec<DealActionRow> = (0..150)
            .map(|i| DealActionRow {
                deal_id: format!("{i}"),
                consultant_id: "1".into(),
                consultant_name: "田中".into(),
                customer_label: "A".into(),
                prefecture: "".into(),
                contract_expiration_date: format!("2026-09-{:02}", (i % 28) + 1),
                days_to_expiration: None,
                contract_plan: "".into(),
                contract_type: "".into(),
                amount: 0.0,
                churn_proba_90d_pct: None,
                risk_level: "".into(),
                latest_nps: None,
                latest_sufficiency: None,
                action_priority: "watch".into(),
            })
            .collect();
        let panel = build_deals_panel(deals, &["watch".to_string()]);
        assert_eq!(panel.rows.len(), DEALS_TABLE_LIMIT);
        assert_eq!(panel.total, 150);
        assert!(panel.truncated);
    }
}
