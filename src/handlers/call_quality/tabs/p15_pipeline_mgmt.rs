//! 案件マネジメント（GAS 版 `page-p15` 未来案件マネジメント の移植）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p15">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP15FuturePipeline` / `_p15PopulateSelector` / `_p15DrawSummaryCards` /
//!          `_p15DrawMonthChart` / `_p15DrawRevenueChart` / `_p15DrawMatrix` /
//!          `_p15DrawDealsTable`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getFuturePipelineMonthly` / `getFuturePipelineSummary` / `getFuturePipelineDeals`）
//!
//! 向こう半年(今月+1〜+6)の保有/満了/高リスク Deal と Revenue at Risk を可視化する。
//! **本タブは上部フィルタを反映しない**（担当者セレクタ駆動、GAS 版と同じ）。
//! `consultant_id` 未指定 = 「全担当者ビュー」（GAS の `P15_VIEW_ALL` 相当）。
//!
//! ------------------------------------------------------------------
//! GAS 版と意図的に違えた点
//! ------------------------------------------------------------------
//! - **優先度チェックボックスの「全て外すと4種全部を返す」フォールバック
//!   （GAS `_p15DrawDealsTable`: `if (checked.length===0) checked=[全4種]`）は実装しない。**
//!   呼び出し側が空配列を明示的に渡した場合は「該当なし」を意味すると解釈する方が
//!   自然で、暗黙のフォールバックは呼び出し元の意図を推測することになるため。
//!   **パラメータ省略時の既定**は GAS の初期チェック状態と同じ
//!   `immediate` / `high` / `medium`（`watch` は既定で外れている、index.html のチェックボックス初期値）。
//! - **満了金額の「継続見込」内訳がマイナスにならないよう `max(0, …)` で clamp する。**
//!   GAS も同じ式(`Math.max(0, exp - hr)`)を使っているが、明示しておく
//!   （高リスク金額が満了金額を超えるデータ不整合が将来起きても負の棒グラフを出さない）。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. 月別スタック棒グラフ・Revenue at Riskグラフ・マトリクスの**描画そのもの**(Chart.js)
//!    → 対象外。`month_chart` / `revenue_chart` / `matrix` はデータのみ返し、
//!      描画・配色はフロント側の責務(他タブと同方針)。
//! 2. 担当者セレクタの名前検索フィルタ(GAS `p15-search`)
//!    → 実装しない。`consultants` を全件返すので検索はフロント側の責務。

use std::collections::{HashMap, HashSet};
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

/// アクション必要Deal一覧の表示上限(GAS `showRows = deals.slice(0, 100)`)
const DEALS_LIMIT: usize = 100;

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

fn deal_label(raw: &str, deal_id: &str) -> String {
    let l = raw.trim();
    if l.is_empty() {
        format!("(名称未取得 / Deal {deal_id})")
    } else {
        l.to_string()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Good,
    Warn,
    Bad,
}

// ============================================================ 優先度

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ActionPriority {
    Immediate,
    High,
    Medium,
    Watch,
    /// シート上の値がどれにも一致しない(想定外データ)。落とさず可視化できるよう残す
    Unknown,
}

impl ActionPriority {
    fn parse(s: &str) -> Self {
        match s.trim() {
            "immediate" => Self::Immediate,
            "high" => Self::High,
            "medium" => Self::Medium,
            "watch" => Self::Watch,
            _ => Self::Unknown,
        }
    }

    /// ソート順(小さいほど優先)。GAS `prioOrder`
    fn order(self) -> u8 {
        match self {
            Self::Immediate => 0,
            Self::High => 1,
            Self::Medium => 2,
            Self::Watch => 3,
            Self::Unknown => 9,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Immediate => "immediate",
            Self::High => "high",
            Self::Medium => "medium",
            Self::Watch => "watch",
            Self::Unknown => "unknown",
        }
    }
}

/// GAS 初期チェック状態(immediate/high/medium はチェック済、watchは未チェック)。
/// パラメータ省略時の既定値として使う(ファイル冒頭「意図的に違えた点」を参照)。
fn default_priorities() -> Vec<ActionPriority> {
    vec![ActionPriority::Immediate, ActionPriority::High, ActionPriority::Medium]
}

fn parse_priorities(s: Option<&str>) -> Vec<ActionPriority> {
    match s {
        None => default_priorities(),
        Some(raw) => raw
            .split(',')
            .map(|t| ActionPriority::parse(t.trim()))
            .filter(|p| *p != ActionPriority::Unknown)
            .collect(),
    }
}

// ============================================================ 月次集計

#[derive(Debug, Clone, Serialize)]
pub struct MonthlyPoint {
    pub year_month: String,
    pub holding_count: f64,
    pub expiring_count: f64,
    pub high_risk_count: f64,
    pub holding_amount: f64,
    pub expiring_amount: f64,
    pub revenue_at_risk: f64,
}

/// シート「コンサル未来案件_月次」(列: consultant_id,consultant_name,year_month,
/// holding_count,expiring_count,high_risk_count,holding_amount,expiring_amount,
/// revenue_at_risk,plan_breakdown_json)を月単位に集約する。
///
/// `consultant_id` が `Some` なら選択コンサルの行のみ、`None` なら全コンサルを
/// year_month で合算する(GAS `_p15GetMonthly` と同じ挙動)。
fn build_monthly(d: &SheetData, consultant_id: Option<&str>) -> Vec<MonthlyPoint> {
    // [holding_count, expiring_count, high_risk_count, holding_amount, expiring_amount, revenue_at_risk]
    let mut acc: HashMap<String, [f64; 6]> = HashMap::new();
    for r in &d.rows {
        if let Some(cid) = consultant_id {
            if d.get(r, "consultant_id") != cid {
                continue;
            }
        }
        let ym = d.get(r, "year_month").trim();
        if ym.is_empty() {
            continue;
        }
        let e = acc.entry(ym.to_string()).or_insert([0.0; 6]);
        e[0] += num(d.get(r, "holding_count"));
        e[1] += num(d.get(r, "expiring_count"));
        e[2] += num(d.get(r, "high_risk_count"));
        e[3] += num(d.get(r, "holding_amount"));
        e[4] += num(d.get(r, "expiring_amount"));
        e[5] += num(d.get(r, "revenue_at_risk"));
    }

    let mut months: Vec<String> = acc.keys().cloned().collect();
    months.sort();
    months
        .into_iter()
        .map(|ym| {
            let v = acc[&ym];
            MonthlyPoint {
                year_month: ym,
                holding_count: v[0],
                expiring_count: v[1],
                high_risk_count: v[2],
                holding_amount: v[3],
                expiring_amount: v[4],
                revenue_at_risk: v[5],
            }
        })
        .collect()
}

// ============================================================ 月別チャート / Revenue at Riskチャート

#[derive(Debug, Serialize)]
pub struct MonthChartPoint {
    pub year_month: String,
    pub holding_count: f64,
    /// 満了予定のうち高リスクでない分(スタック下層、負にはしない)
    pub expiring_neutral: f64,
    pub high_risk_count: f64,
}

fn build_month_chart(monthly: &[MonthlyPoint]) -> Vec<MonthChartPoint> {
    monthly
        .iter()
        .map(|m| MonthChartPoint {
            year_month: m.year_month.clone(),
            holding_count: m.holding_count,
            expiring_neutral: (m.expiring_count - m.high_risk_count).max(0.0),
            high_risk_count: m.high_risk_count,
        })
        .collect()
}

#[derive(Debug, Serialize)]
pub struct RevenueChartPoint {
    pub year_month: String,
    /// 満了金額のうち高リスク金額でない分(負にはしない)
    pub expiring_amount_neutral: f64,
    pub revenue_at_risk: f64,
}

fn build_revenue_chart(monthly: &[MonthlyPoint]) -> Vec<RevenueChartPoint> {
    monthly
        .iter()
        .map(|m| RevenueChartPoint {
            year_month: m.year_month.clone(),
            expiring_amount_neutral: (m.expiring_amount - m.revenue_at_risk).max(0.0),
            revenue_at_risk: m.revenue_at_risk,
        })
        .collect()
}

// ============================================================ コンサル×月 マトリクス

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
    pub cells: Vec<MatrixCell>,
    pub total_expiring_6m: f64,
    pub total_high_risk_6m: f64,
    pub total_amount_at_risk_6m: f64,
}

/// 全担当者ビュー専用のマトリクスを組む。年月の一覧は月次シート全体から作る。
/// 呼び出し元は「全担当者ビュー(consultant_id 未指定)」のときのみ呼ぶこと
/// （選択担当者ありのときは意味を持たないため空を返す運用、`handle` を参照）。
fn build_matrix(monthly: &SheetData, summary: &SheetData) -> (Vec<MatrixRow>, Vec<String>) {
    let mut ym_set: HashSet<String> = HashSet::new();
    for r in &monthly.rows {
        let ym = monthly.get(r, "year_month").trim();
        if !ym.is_empty() {
            ym_set.insert(ym.to_string());
        }
    }
    let mut yms: Vec<String> = ym_set.into_iter().collect();
    yms.sort();

    let mut cell_map: HashMap<(String, String), (f64, f64, f64)> = HashMap::new();
    for r in &monthly.rows {
        let cid = monthly.get(r, "consultant_id").to_string();
        let ym = monthly.get(r, "year_month").trim().to_string();
        if cid.is_empty() || ym.is_empty() {
            continue;
        }
        cell_map.insert(
            (cid, ym),
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
            let cid = summary.get(r, "consultant_id").to_string();
            let cells = yms
                .iter()
                .map(|ym| {
                    let (h, e, hr) = cell_map.get(&(cid.clone(), ym.clone())).copied().unwrap_or((0.0, 0.0, 0.0));
                    MatrixCell {
                        year_month: ym.clone(),
                        holding_count: h,
                        expiring_count: e,
                        high_risk_count: hr,
                    }
                })
                .collect();
            MatrixRow {
                consultant_name: {
                    let n = summary.get(r, "consultant_name").trim();
                    if n.is_empty() { cid.clone() } else { n.to_string() }
                },
                cells,
                total_expiring_6m: num(summary.get(r, "total_expiring_6m")),
                total_high_risk_6m: num(summary.get(r, "total_high_risk_6m")),
                total_amount_at_risk_6m: num(summary.get(r, "total_amount_at_risk_6m")),
                consultant_id: cid,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        b.total_amount_at_risk_6m
            .partial_cmp(&a.total_amount_at_risk_6m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.consultant_id.cmp(&b.consultant_id))
    });

    (rows, yms)
}

// ============================================================ アクション必要Deal一覧

#[derive(Debug, Serialize)]
pub struct FutureDealRow {
    pub deal_id: String,
    pub consultant_id: String,
    pub consultant_name: String,
    pub label: String,
    pub prefecture: String,
    pub contract_start_date: String,
    pub contract_expiration_date: String,
    pub days_to_expiration: Option<f64>,
    pub contract_period: String,
    pub contract_plan: String,
    pub contract_type: String,
    pub amount: f64,
    /// 0..100(%)。GAS `(proba*100).toFixed(0)+'%'` に合わせて % 表記
    pub churn_proba_90d_pct: Option<f64>,
    pub risk_level: String,
    pub latest_nps: Option<f64>,
    pub latest_sufficiency: Option<f64>,
    pub action_priority: ActionPriority,
    pub hubspot_url: String,
}

#[derive(Debug, Serialize)]
pub struct DealsTable {
    pub rows: Vec<FutureDealRow>,
    /// 絞り込み後・上限で切る前の件数
    pub total_rows: usize,
    pub truncated: bool,
    pub limit: usize,
}

/// シート「コンサル未来案件_Deal一覧」(列: deal_id,consultant_id,consultant_name,
/// customer_label,prefecture,contract_start_date,contract_expiration_date,
/// days_to_expiration,contract_period,contract_plan,contract_type,amount,
/// churn_proba_90d,risk_level,latest_nps,latest_sufficiency,action_priority)から、
/// (任意で)担当者 + 優先度で絞り込み、優先度→満了日昇順でソートして上位100件を返す
/// (GAS `_p15DrawDealsTable` と同じ仕様)。
fn build_deals(d: &SheetData, consultant_id: Option<&str>, priorities: &[ActionPriority]) -> DealsTable {
    let mut filtered: Vec<&Vec<Arc<str>>> = d
        .rows
        .iter()
        .filter(|r| {
            if let Some(cid) = consultant_id {
                if d.get(r, "consultant_id") != cid {
                    return false;
                }
            }
            let p = ActionPriority::parse(d.get(r, "action_priority"));
            priorities.contains(&p)
        })
        .collect();

    filtered.sort_by(|a, b| {
        let pa = ActionPriority::parse(d.get(a, "action_priority")).order();
        let pb = ActionPriority::parse(d.get(b, "action_priority")).order();
        if pa != pb {
            return pa.cmp(&pb);
        }
        d.get(a, "contract_expiration_date").cmp(d.get(b, "contract_expiration_date"))
    });

    let total_rows = filtered.len();
    let truncated = total_rows > DEALS_LIMIT;
    let rows: Vec<FutureDealRow> = filtered
        .iter()
        .take(DEALS_LIMIT)
        .map(|r| {
            let deal_id = d.get(r, "deal_id").to_string();
            let label = deal_label(d.get(r, "customer_label"), &deal_id);
            FutureDealRow {
                consultant_id: d.get(r, "consultant_id").to_string(),
                consultant_name: {
                    let n = d.get(r, "consultant_name").trim();
                    if n.is_empty() { "-".to_string() } else { n.to_string() }
                },
                label,
                prefecture: d.get(r, "prefecture").to_string(),
                contract_start_date: d.get(r, "contract_start_date").to_string(),
                contract_expiration_date: d.get(r, "contract_expiration_date").to_string(),
                days_to_expiration: opt_num(d.get(r, "days_to_expiration")),
                contract_period: d.get(r, "contract_period").to_string(),
                contract_plan: d.get(r, "contract_plan").to_string(),
                contract_type: d.get(r, "contract_type").to_string(),
                amount: num(d.get(r, "amount")),
                churn_proba_90d_pct: opt_num(d.get(r, "churn_proba_90d")).map(|v| v * 100.0),
                risk_level: d.get(r, "risk_level").to_string(),
                latest_nps: opt_num(d.get(r, "latest_nps")),
                latest_sufficiency: opt_num(d.get(r, "latest_sufficiency")),
                action_priority: ActionPriority::parse(d.get(r, "action_priority")),
                hubspot_url: format!("https://app.hubspot.com/contacts/23708633/deal/{deal_id}"),
                deal_id,
            }
        })
        .collect();

    DealsTable { rows, total_rows, truncated, limit: DEALS_LIMIT }
}

// ============================================================ KPIスコアカード

#[derive(Debug, Serialize)]
pub struct SummaryCards {
    pub total_holding: f64,
    pub total_expiring_6m: f64,
    pub total_high_risk_6m: f64,
    pub total_revenue_at_risk_6m: f64,
    pub immediate_count: usize,
    pub high_count: usize,
    pub high_risk_band: Band,
    pub action_band: Band,
}

/// KPIカード6枚を組む(GAS `_p15DrawSummaryCards`)。
/// `total_holding` は選択担当者があればサマリシートの該当行、無ければサマリ全行を合算。
/// `immediate_count`/`high_count` は**優先度フィルタを適用する前**の全件から数える
/// （チェックボックスで隠しても実数が変わって見えると誤解を招くため。GAS も同様に
/// `deals` = `_p15GetDeals(P15_SELECTED)` を使い、優先度チェックボックスの影響を受けない）。
fn build_summary(monthly: &[MonthlyPoint], summary: &SheetData, deals: &SheetData, consultant_id: Option<&str>) -> SummaryCards {
    let total_holding = match consultant_id {
        Some(cid) => summary
            .rows
            .iter()
            .find(|r| summary.get(r, "consultant_id") == cid)
            .map(|r| num(summary.get(r, "total_holding")))
            .unwrap_or(0.0),
        None => summary.rows.iter().map(|r| num(summary.get(r, "total_holding"))).sum(),
    };

    let total_exp: f64 = monthly.iter().map(|m| m.expiring_count).sum();
    let total_hr: f64 = monthly.iter().map(|m| m.high_risk_count).sum();
    let total_rev: f64 = monthly.iter().map(|m| m.revenue_at_risk).sum();

    let mut immediate_count = 0usize;
    let mut high_count = 0usize;
    for r in &deals.rows {
        if let Some(cid) = consultant_id {
            if deals.get(r, "consultant_id") != cid {
                continue;
            }
        }
        match ActionPriority::parse(deals.get(r, "action_priority")) {
            ActionPriority::Immediate => immediate_count += 1,
            ActionPriority::High => high_count += 1,
            _ => {}
        }
    }

    let high_risk_band = if total_hr >= 10.0 {
        Band::Bad
    } else if total_hr > 0.0 {
        Band::Warn
    } else {
        Band::Good
    };
    let action_band = if immediate_count > 0 {
        Band::Bad
    } else if high_count > 0 {
        Band::Warn
    } else {
        Band::Good
    };

    SummaryCards {
        total_holding,
        total_expiring_6m: total_exp,
        total_high_risk_6m: total_hr,
        total_revenue_at_risk_6m: total_rev,
        immediate_count,
        high_count,
        high_risk_band,
        action_band,
    }
}

// ============================================================ 全体

#[derive(Debug, Default, Deserialize)]
pub struct P15Query {
    /// 選択担当者。未指定 = 全担当者ビュー(GAS `P15_VIEW_ALL`)
    pub consultant_id: Option<String>,
    /// アクション必要Deal一覧の優先度フィルタ。カンマ区切り(immediate,high,medium,watch)。
    /// 省略時は既定(immediate/high/medium、GASの初期チェック状態)
    pub priority: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct ConsultantOption {
    pub consultant_id: String,
    pub consultant_name: String,
    pub total_amount_at_risk_6m: f64,
    pub total_high_risk_6m: f64,
}

#[derive(Debug, Serialize)]
pub struct P15Data {
    /// 担当者セレクタ用の一覧。total_amount_at_risk_6m 降順(GAS `_p15PopulateSelector`)
    pub consultants: Vec<ConsultantOption>,
    pub selected_consultant_id: Option<String>,
    pub summary: SummaryCards,
    pub month_chart: Vec<MonthChartPoint>,
    pub revenue_chart: Vec<RevenueChartPoint>,
    /// 全担当者ビュー(`selected_consultant_id` が None)のときのみ埋まる。選択時は空配列
    pub matrix: Vec<MatrixRow>,
    pub matrix_months: Vec<String>,
    pub deals: DealsTable,
    pub priority_filter: Vec<&'static str>,
}

async fn load(client: &SheetsClient, store: &SheetStore, name: &str, sources: &mut Vec<SourceInfo>) -> Result<Arc<SheetData>> {
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

/// ハンドラ本体。3シートを読み、KPIカード/月別チャート/Revenue at Riskチャート/
/// (全担当者ビューのみ)マトリクス/アクション必要Deal一覧を組んで返す。
pub async fn handle(client: &SheetsClient, store: &SheetStore, q: P15Query) -> Result<TabPayload<P15Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let monthly_sheet = load(client, store, SHEET_MONTHLY, &mut sources).await?;
    let summary_sheet = load(client, store, SHEET_SUMMARY, &mut sources).await?;
    let deals_sheet = load(client, store, SHEET_DEALS, &mut sources).await?;

    let cid = q.consultant_id.as_deref().filter(|s| !s.is_empty());
    let priorities = parse_priorities(q.priority.as_deref());

    let mut consultants: Vec<ConsultantOption> = summary_sheet
        .rows
        .iter()
        .map(|r| {
            let consultant_id = summary_sheet.get(r, "consultant_id").to_string();
            ConsultantOption {
                consultant_name: {
                    let n = summary_sheet.get(r, "consultant_name").trim();
                    if n.is_empty() { consultant_id.clone() } else { n.to_string() }
                },
                total_amount_at_risk_6m: num(summary_sheet.get(r, "total_amount_at_risk_6m")),
                total_high_risk_6m: num(summary_sheet.get(r, "total_high_risk_6m")),
                consultant_id,
            }
        })
        .collect();
    consultants.sort_by(|a, b| {
        b.total_amount_at_risk_6m
            .partial_cmp(&a.total_amount_at_risk_6m)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.consultant_id.cmp(&b.consultant_id))
    });

    let monthly = build_monthly(&monthly_sheet, cid);
    let summary = build_summary(&monthly, &summary_sheet, &deals_sheet, cid);
    let month_chart = build_month_chart(&monthly);
    let revenue_chart = build_revenue_chart(&monthly);

    // マトリクスは全担当者ビュー(cid未指定)のときだけ意味を持つ(GAS `P15_VIEW_ALL`)。
    // 選択時に計算しても使われないため、空で返して計算量を節約する。
    let (matrix, matrix_months) = if cid.is_none() {
        build_matrix(&monthly_sheet, &summary_sheet)
    } else {
        (Vec::new(), Vec::new())
    };

    let deals_table = build_deals(&deals_sheet, cid, &priorities);

    set_matched(&mut sources, SHEET_DEALS, deals_table.total_rows);
    set_matched(&mut sources, SHEET_MONTHLY, monthly.len());

    Ok(TabPayload {
        data: P15Data {
            consultants,
            selected_consultant_id: cid.map(|s| s.to_string()),
            summary,
            month_chart,
            revenue_chart,
            matrix,
            matrix_months,
            deals: deals_table,
            priority_filter: priorities.iter().map(|p| p.as_str()).collect(),
        },
        sources,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

// ================================================================== テスト

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(header: &[&str], rows: &[&[&str]]) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .iter()
                .map(|r| r.iter().map(|c| Arc::from(*c)).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    const DEALS_HEADER: [&str; 17] = [
        "deal_id", "consultant_id", "consultant_name", "customer_label", "prefecture",
        "contract_start_date", "contract_expiration_date", "days_to_expiration",
        "contract_period", "contract_plan", "contract_type", "amount", "churn_proba_90d",
        "risk_level", "latest_nps", "latest_sufficiency", "action_priority",
    ];

    #[test]
    fn 顧客名が取れないときはdeal_idをそのまま名前にしない() {
        let d = sheet(
            &DEALS_HEADER,
            &[&[
                "777", "1", "藤巻", "", "東京都", "2026-01-01", "2026-07-01", "5",
                "6", "スタンダード", "更新", "500000", "0.6", "high", "", "", "immediate",
            ]],
        );
        let table = build_deals(&d, None, &default_priorities());
        assert_eq!(table.rows[0].label, "(名称未取得 / Deal 777)");
    }

    #[test]
    fn 月次集計は担当者別と全体合算どちらも正しい() {
        let d = sheet(
            &["consultant_id", "consultant_name", "year_month", "holding_count", "expiring_count",
              "high_risk_count", "holding_amount", "expiring_amount", "revenue_at_risk", "plan_breakdown_json"],
            &[
                &["1", "藤巻", "2026-07", "10", "3", "1", "1000000", "300000", "100000", "{}"],
                &["2", "他人", "2026-07", "5", "2", "0", "500000", "200000", "0", "{}"],
            ],
        );
        let mine = build_monthly(&d, Some("1"));
        assert_eq!(mine.len(), 1);
        assert_eq!(mine[0].holding_count, 10.0);

        let all = build_monthly(&d, None);
        assert_eq!(all.len(), 1, "同一年月は合算される");
        assert_eq!(all[0].holding_count, 15.0);
        assert_eq!(all[0].expiring_count, 5.0);
    }

    #[test]
    fn 満了金額の継続見込内訳はマイナスにならない() {
        // データ不整合(revenue_at_riskが expiring_amount を超える)があっても負を出さない
        let monthly = vec![MonthlyPoint {
            year_month: "2026-07".to_string(),
            holding_count: 0.0,
            expiring_count: 0.0,
            high_risk_count: 0.0,
            holding_amount: 0.0,
            expiring_amount: 100.0,
            revenue_at_risk: 150.0,
        }];
        let chart = build_revenue_chart(&monthly);
        assert_eq!(chart[0].expiring_amount_neutral, 0.0, "負にせずclampする");
    }

    #[test]
    fn dealsは優先度と満了日の昇順でソートされる() {
        let d = sheet(
            &DEALS_HEADER,
            &[
                &["1", "1", "藤巻", "A", "東京都", "", "2026-08-01", "", "", "", "", "0", "0.5", "high", "", "", "medium"],
                &["2", "1", "藤巻", "B", "東京都", "", "2026-07-01", "", "", "", "", "0", "0.9", "critical", "", "", "immediate"],
                &["3", "1", "藤巻", "C", "東京都", "", "2026-07-15", "", "", "", "", "0", "0.9", "critical", "", "", "immediate"],
            ],
        );
        let table = build_deals(&d, None, &[ActionPriority::Immediate, ActionPriority::Medium]);
        let ids: Vec<&str> = table.rows.iter().map(|r| r.deal_id.as_str()).collect();
        assert_eq!(ids, vec!["2", "3", "1"], "immediateが先、同優先度内は満了日昇順");
    }

    #[test]
    fn 優先度フィルタは指定されたものだけ含む_全て外れたら空() {
        let d = sheet(
            &DEALS_HEADER,
            &[&["1", "1", "藤巻", "A", "東京都", "", "2026-07-01", "", "", "", "", "0", "0.1", "low", "", "", "watch"]],
        );
        // GASの「全部外れたら全4種にフォールバック」はしない(ファイル冒頭の注記)。空を渡せば空で返る。
        let table = build_deals(&d, None, &[]);
        assert_eq!(table.rows.len(), 0, "空の優先度リストはフォールバックせず0件");
    }

    #[test]
    fn 全担当者ビューでのみマトリクスが埋まる() {
        let monthly = sheet(
            &["consultant_id", "consultant_name", "year_month", "holding_count", "expiring_count",
              "high_risk_count", "holding_amount", "expiring_amount", "revenue_at_risk", "plan_breakdown_json"],
            &[&["1", "藤巻", "2026-07", "10", "3", "1", "0", "0", "0", "{}"]],
        );
        let summary = sheet(
            &["consultant_id", "consultant_name", "total_holding", "total_expiring_6m",
              "total_high_risk_6m", "total_amount_at_risk_6m", "max_month_expiring", "max_month_amount_at_risk"],
            &[&["1", "藤巻", "10", "3", "1", "100000", "", ""]],
        );
        let (rows, months) = build_matrix(&monthly, &summary);
        assert_eq!(rows.len(), 1);
        assert_eq!(months, vec!["2026-07".to_string()]);
    }

    #[test]
    fn デフォルトの優先度はwatchを含まない() {
        // GAS index.html の初期チェック状態(immediate/high/medium checked, watch unchecked)に合わせる
        let d = default_priorities();
        assert!(d.contains(&ActionPriority::Immediate));
        assert!(d.contains(&ActionPriority::High));
        assert!(d.contains(&ActionPriority::Medium));
        assert!(!d.contains(&ActionPriority::Watch));
    }

    #[test]
    fn サマリのimmediate件数は優先度フィルタの影響を受けない() {
        let deals = sheet(
            &DEALS_HEADER,
            &[
                &["1", "1", "藤巻", "A", "", "", "", "", "", "", "", "0", "0", "", "", "", "immediate"],
                &["2", "1", "藤巻", "B", "", "", "", "", "", "", "", "0", "0", "", "", "", "watch"],
            ],
        );
        let monthly: Vec<MonthlyPoint> = Vec::new();
        let summary = sheet(
            &["consultant_id", "consultant_name", "total_holding", "total_expiring_6m",
              "total_high_risk_6m", "total_amount_at_risk_6m", "max_month_expiring", "max_month_amount_at_risk"],
            &[],
        );
        let cards = build_summary(&monthly, &summary, &deals, Some("1"));
        assert_eq!(cards.immediate_count, 1, "watchの1件を含む全件から数える(フィルタ前)");
    }
}
