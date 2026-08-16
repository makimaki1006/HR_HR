//! リスクボード（GAS 版 `page-prisk`）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-prisk">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderRiskBoard` / `_priskDraw` / `renderKgiRiskParts` /
//!          `_drawP9HealthSection` / `renderP9RiskScore` / `_drawP9RiskSection` /
//!          `_drawP9MatrixSection`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getRiskBoard` / `getCustomerHealthActive` / `getCustomerHealth` /
//!          `getCustomerRiskScore` / `getConsultingKgiMatrix`）
//!
//! 2026-08-12 に旧「コンサルKGI」タブが解体され、健全性/リスクスコア/マトリクスの
//! 3セクションがこのタブへ移設された（`renderKgiRiskParts` 参照）。
//! 画面には4パネルが縦に並ぶ:
//!   [板] リスク統合ボード本体（4軸統合、赤軸数で集約）
//!   [1]  顧客健全性スコア（NPS+成果スコア、0-100）
//!   [1.5] リスクスコア（代理指標、稼働中の案件限定）
//!   [2]  コンサル×顧客マトリクス（接触量 × リスクスコア、4象限）
//!
//! ------------------------------------------------------------------
//! 移植した領域（GAS の DOM id → このファイルの出力）
//! ------------------------------------------------------------------
//!   prisk-kpis / prisk-table / prisk-status  → `BoardPanel`
//!   p9-health-kpis / p9-health-trend / p9-health-top / p9-health-bottom → `HealthPanel`
//!   p9-risk-kpis / p9-risk-level-chart / p9-risk-top → `RiskScorePanel`
//!   p9-matrix-kpis / p9-matrix-scatter → `MatrixPanel`
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. **行クリック→P13カルテへドリル**（GAS `_gotoP13Deal`）
//!    → 移植しない。これは画面遷移の UI 挙動であり、フロント側の責務。
//!      `BoardRow::deal_id` を返しているので、フロントはそれを使って遷移を実装できる。
//!
//! ------------------------------------------------------------------
//! GAS と意図的に違えた点（完了条件4）
//! ------------------------------------------------------------------
//! - **`sort=expiry` のとき `days_to_expiry === 0` を空欄扱いしない**。
//!   GAS は `_priskNum(a.days_to_expiry || 99999)` と書いており、JS の `||` は
//!   `0` も falsy として扱うため「満了まで0日」の案件が最遅として99999日扱いになり、
//!   本来最優先で出るべき案件が末尾に沈む不具合がある。ここでは「値が空/欠損のときだけ」
//!   99999 とし、0はそのまま使う。
//! - **`sort=proba` のとき欠損は0点扱い**（GAS `_priskNum` が NaN→0 にする挙動を維持）。
//!   モデル未算出の Deal を「解約確率0%」と誤読させないよう、`ax2_model_proba` 自体は
//!   `Option<f64>` のまま返す（表示側で「-」にできるように）。ソート用の0扱いとは分離。
//! - **顧客名が取れないとき Deal ID をそのまま名前として返さない**
//!   （GAS `displayDealLabel` の 2026-08-13 是正を踏襲、団員からの要望どおり）。
//!   `"(名称未取得 / Deal {id})"` の形にする。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_BOARD: &str = "リスク統合ボード";
const SHEET_HEALTH_ACTIVE: &str = "コンサル健全性_月次_active";
/// active 限定シートが0件のときのフォールバック（GAS `getCustomerHealth()`）。
const SHEET_HEALTH_FALLBACK: &str = "コンサル健全性_月次";
const SHEET_RISK_SCORE: &str = "コンサルリスクスコア_月次";
const SHEET_MATRIX: &str = "コンサルKGIマトリクス";

/// 健全性トップ/ボトムの表示件数（GAS `slice(0,10)` / `slice(-10)`）
const HEALTH_RANK_LIMIT: usize = 10;
/// 健全性推移の表示月数（GAS `months.slice(-12)`）
const HEALTH_TREND_MONTHS: usize = 12;
/// リスクスコア Top の表示件数（GAS `slice(0,20)`）
const RISK_TOP_LIMIT: usize = 20;

// ---------------------------------------------------------------- 小道具

/// 数値化。空文字・非数値は 0。GAS `num()` / `_priskNum()` と同じ挙動。
fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 値が無いことと 0 を区別する。空文字は None。
fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.replace(',', "").parse::<f64>().ok()
    }
}

/// GAS `displayDealLabel` の移植（2026-08-13 是正込み）。
/// 名前が取れないとき Deal ID をそのまま返さない。
fn deal_label(deal_id: &str, deal_label: &str, customer_label: &str, customer_name: &str) -> String {
    for v in [deal_label, customer_label, customer_name] {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    let id = deal_id.trim();
    if !id.is_empty() {
        format!("(名称未取得 / Deal {id})")
    } else {
        "-".to_string()
    }
}

/// GAS `displayOwnerLabel` の移植。
fn owner_label(owner_name: &str, deal_owner_name: &str, owner_id: &str) -> String {
    for v in [owner_name, deal_owner_name, owner_id] {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    "-".to_string()
}

/// GAS `displayPipelineStage` の移植。
fn pipeline_stage(pipeline_label: &str, stage_label: &str) -> String {
    let p = pipeline_label.trim();
    let s = stage_label.trim();
    match (p.is_empty(), s.is_empty()) {
        (false, false) => format!("{p} / {s}"),
        (false, true) => p.to_string(),
        (true, false) => s.to_string(),
        (true, true) => "-".to_string(),
    }
}

/// `health_score` シートの `drop` 列。GAS は
/// `v === true || v === 'true' || v === 'True' || v === 'TRUE' || v === 1 || v === '1'`
/// を真とする（`consulting_task_alerts.py` 系の bool 列とは判定基準が違う点に注意）。
fn is_drop(s: &str) -> bool {
    let t = s.trim();
    t.eq_ignore_ascii_case("true") || t == "1"
}

// ================================================================== 板本体

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskSort {
    Red,
    Rar,
    Expiry,
    Proba,
}

impl RiskSort {
    fn parse(s: Option<&str>) -> Self {
        match s.map(str::trim) {
            Some("rar") => Self::Rar,
            Some("expiry") => Self::Expiry,
            Some("proba") => Self::Proba,
            _ => Self::Red,
        }
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct BoardRow {
    pub deal_id: String,
    pub customer_label: String,
    pub consultant_name: String,
    pub contract_type: String,
    pub contract_period: String,
    pub phase_bucket: String,
    /// 満了までの日数。空/欠損は None（0日はそのまま0）
    pub days_to_expiry: Option<f64>,
    pub amount: f64,
    /// ①関係性の判定（絵文字フラグ、例: 🚨緊急）
    pub ax1_relation: String,
    pub latest_nps: Option<f64>,
    /// ②モデル解約確率(%)。未算出は None
    pub ax2_model_proba: Option<f64>,
    /// ③放置の判定（絵文字フラグ）
    pub ax3_contact: String,
    pub days_since_contact: Option<f64>,
    /// ④収益 Revenue at Risk（円）
    pub ax4_revenue_at_risk: f64,
    /// 4軸中いくつが赤か(0-4)
    pub red_count: i32,
    /// 🔴最優先 / 🟠要注意 / 🟡監視 / 🟢安定
    pub overall_band: String,
    pub main_factor: String,
}

#[derive(Debug, Serialize, Default)]
pub struct BandCounts {
    pub critical: usize,
    pub warning: usize,
    pub watch: usize,
    pub stable: usize,
}

#[derive(Debug, Serialize)]
pub struct BoardPanel {
    /// フィルタ・ソート後の表示行
    pub rows: Vec<BoardRow>,
    /// 絞り込み前の全件数
    pub total_rows: usize,
    /// バンド件数は**絞り込み前の全体**で出す（GAS `bandCnt` は `all` を見る。
    /// 担当や最低赤軸数を選んでも「全体で何件危険か」は変わらないようにするため）
    pub band_counts: BandCounts,
    pub sort: RiskSort,
    pub min_red: i32,
    pub consultant_filter: Option<String>,
    /// 担当セレクタの選択肢（重複無し・五十音でなくコードポイント順）
    pub consultants: Vec<String>,
}

pub fn collect_board(data: &SheetData) -> Vec<BoardRow> {
    data.rows
        .iter()
        .map(|row| {
            let deal_id = data.get(row, "deal_id").to_string();
            BoardRow {
                customer_label: deal_label(&deal_id, "", data.get(row, "customer_label"), ""),
                deal_id,
                consultant_name: data.get(row, "consultant_name").to_string(),
                contract_type: data.get(row, "contract_type").to_string(),
                contract_period: data.get(row, "contract_period").to_string(),
                phase_bucket: data.get(row, "phase_bucket").to_string(),
                days_to_expiry: opt_num(data.get(row, "days_to_expiry")),
                amount: num(data.get(row, "amount")),
                ax1_relation: data.get(row, "ax1_relation").to_string(),
                latest_nps: opt_num(data.get(row, "latest_nps")),
                ax2_model_proba: opt_num(data.get(row, "ax2_model_proba")),
                ax3_contact: data.get(row, "ax3_contact").to_string(),
                days_since_contact: opt_num(data.get(row, "days_since_contact")),
                ax4_revenue_at_risk: num(data.get(row, "ax4_revenue_at_risk")),
                red_count: num(data.get(row, "red_count")) as i32,
                overall_band: data.get(row, "overall_band").to_string(),
                main_factor: data.get(row, "main_factor").to_string(),
            }
        })
        .collect()
}

fn band_counts(rows: &[BoardRow]) -> BandCounts {
    let mut c = BandCounts::default();
    for r in rows {
        // GAS `bandCnt` は `indexOf(b) >= 0`（部分一致）。overall_band は
        // 絵文字+ラベルの完全形（例:「🔴最優先」）でしか来ないため contains で揃える。
        if r.overall_band.contains("最優先") {
            c.critical += 1;
        } else if r.overall_band.contains("要注意") {
            c.warning += 1;
        } else if r.overall_band.contains("監視") {
            c.watch += 1;
        } else if r.overall_band.contains("安定") {
            c.stable += 1;
        }
    }
    c
}

fn sort_board(mut rows: Vec<BoardRow>, sort: RiskSort) -> Vec<BoardRow> {
    use std::cmp::Ordering;
    match sort {
        RiskSort::Rar => rows.sort_by(|a, b| {
            b.ax4_revenue_at_risk
                .partial_cmp(&a.ax4_revenue_at_risk)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.deal_id.cmp(&b.deal_id))
        }),
        RiskSort::Expiry => rows.sort_by(|a, b| {
            let av = a.days_to_expiry.unwrap_or(99999.0);
            let bv = b.days_to_expiry.unwrap_or(99999.0);
            av.partial_cmp(&bv)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.deal_id.cmp(&b.deal_id))
        }),
        RiskSort::Proba => rows.sort_by(|a, b| {
            let av = a.ax2_model_proba.unwrap_or(0.0);
            let bv = b.ax2_model_proba.unwrap_or(0.0);
            bv.partial_cmp(&av)
                .unwrap_or(Ordering::Equal)
                .then_with(|| a.deal_id.cmp(&b.deal_id))
        }),
        RiskSort::Red => rows.sort_by(|a, b| {
            b.red_count
                .cmp(&a.red_count)
                .then_with(|| {
                    b.ax4_revenue_at_risk
                        .partial_cmp(&a.ax4_revenue_at_risk)
                        .unwrap_or(Ordering::Equal)
                })
                .then_with(|| a.deal_id.cmp(&b.deal_id))
        }),
    }
    rows
}

pub fn build_board(data: &SheetData, q: &PriskQuery) -> BoardPanel {
    let all = collect_board(data);
    let band_counts = band_counts(&all);

    let mut names: Vec<String> = all
        .iter()
        .map(|r| r.consultant_name.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect();
    names.sort();
    names.dedup();

    let min_red = q.min_red.unwrap_or(0);
    let sort = RiskSort::parse(q.sort.as_deref());
    let consultant_filter = q
        .consultant
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let mut rows = all.clone();
    if let Some(c) = consultant_filter.as_deref() {
        rows.retain(|r| r.consultant_name == c);
    }
    rows.retain(|r| r.red_count >= min_red);
    let rows = sort_board(rows, sort);

    BoardPanel {
        rows,
        total_rows: all.len(),
        band_counts,
        sort,
        min_red,
        consultant_filter,
        consultants: names,
    }
}

// ================================================================== [1] 健全性

#[derive(Debug, Serialize, Clone)]
pub struct HealthRow {
    pub deal_id: String,
    pub deal_label: String,
    pub owner_label: String,
    pub pipeline_stage: String,
    pub year_month: String,
    pub health_score: f64,
    pub prev_score: Option<f64>,
    pub drop: bool,
}

#[derive(Debug, Serialize)]
pub struct HealthMonthPoint {
    pub year_month: String,
    /// その月にデータが無ければ None（0点と混同させない。GAS はグラフの `spanGaps` で対応）
    pub avg_health_score: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct HealthPanel {
    /// 実際に使ったシート。active が0件のときだけフォールバックに切り替わる
    pub sheet_used: &'static str,
    pub latest_month: Option<String>,
    pub avg_health_score: Option<f64>,
    pub drop_count: usize,
    pub target_count: usize,
    pub top10: Vec<HealthRow>,
    pub bottom10: Vec<HealthRow>,
    pub monthly_trend: Vec<HealthMonthPoint>,
}

pub fn collect_health(data: &SheetData) -> Vec<HealthRow> {
    data.rows
        .iter()
        .map(|row| HealthRow {
            deal_id: data.get(row, "deal_id").to_string(),
            deal_label: deal_label(
                data.get(row, "deal_id"),
                data.get(row, "deal_label"),
                data.get(row, "customer_label"),
                "",
            ),
            owner_label: owner_label(data.get(row, "owner_name"), "", data.get(row, "owner_id")),
            pipeline_stage: pipeline_stage(data.get(row, "pipeline_label"), data.get(row, "stage_label")),
            year_month: data.get(row, "year_month").trim().to_string(),
            health_score: num(data.get(row, "health_score")),
            prev_score: opt_num(data.get(row, "prev_score")),
            drop: is_drop(data.get(row, "drop")),
        })
        .collect()
}

fn year_months(data: &SheetData) -> Vec<String> {
    let mut set: Vec<String> = data
        .rows
        .iter()
        .map(|r| data.get(r, "year_month").trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    set.sort();
    set.dedup();
    set
}

pub fn build_health(data: &SheetData, sheet_used: &'static str) -> HealthPanel {
    let rows = collect_health(data);
    let months = year_months(data);
    let latest_month = months.last().cloned();

    let latest_rows: Vec<&HealthRow> = match latest_month.as_deref() {
        Some(m) => rows.iter().filter(|r| r.year_month == m).collect(),
        None => Vec::new(),
    };

    let avg_health_score = if latest_rows.is_empty() {
        None
    } else {
        Some(latest_rows.iter().map(|r| r.health_score).sum::<f64>() / latest_rows.len() as f64)
    };
    let drop_count = latest_rows.iter().filter(|r| r.drop).count();
    let target_count = latest_rows.len();

    let mut desc: Vec<HealthRow> = latest_rows.iter().map(|r| (*r).clone()).collect();
    desc.sort_by(|a, b| {
        b.health_score
            .partial_cmp(&a.health_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let top10 = desc.iter().take(HEALTH_RANK_LIMIT).cloned().collect();

    let mut asc: Vec<HealthRow> = latest_rows.iter().map(|r| (*r).clone()).collect();
    asc.sort_by(|a, b| {
        a.health_score
            .partial_cmp(&b.health_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let bottom10 = asc.iter().take(HEALTH_RANK_LIMIT).cloned().collect();

    // 直近12ヶ月の全社平均（全行対象。latest_rows に絞らない）
    let mut by_month: HashMap<String, (f64, usize)> = HashMap::new();
    for row in &data.rows {
        let m = data.get(row, "year_month").trim().to_string();
        if m.is_empty() {
            continue;
        }
        let e = by_month.entry(m).or_insert((0.0, 0));
        e.0 += num(data.get(row, "health_score"));
        e.1 += 1;
    }
    let recent: Vec<String> = months
        .iter()
        .rev()
        .take(HEALTH_TREND_MONTHS)
        .rev()
        .cloned()
        .collect();
    let monthly_trend = recent
        .into_iter()
        .map(|m| {
            let avg = by_month.get(&m).map(|(sum, n)| sum / *n as f64);
            HealthMonthPoint {
                year_month: m,
                avg_health_score: avg,
            }
        })
        .collect();

    HealthPanel {
        sheet_used,
        latest_month,
        avg_health_score,
        drop_count,
        target_count,
        top10,
        bottom10,
        monthly_trend,
    }
}

// ================================================================== [1.5] リスクスコア

#[derive(Debug, Serialize, Clone)]
pub struct RiskScoreRow {
    pub deal_id: String,
    pub deal_label: String,
    pub owner_label: String,
    pub pipeline_stage: String,
    pub risk_score: f64,
    /// critical / high / medium / low。シートの表記が想定外なら low に丸める
    /// （GAS `if (!(lvl in lvlCounts)) lvl = 'low'` と同じ）
    pub risk_level: String,
    pub days_since_last_contact: Option<f64>,
    pub call_all: Option<f64>,
    pub call_post: Option<f64>,
    pub oubo_per_posting: Option<f64>,
    pub top_drivers: String,
}

#[derive(Debug, Serialize, Default)]
pub struct RiskLevelCounts {
    pub critical: usize,
    pub high: usize,
    pub medium: usize,
    pub low: usize,
}

#[derive(Debug, Serialize)]
pub struct RiskScorePanel {
    pub total: usize,
    pub avg_risk_score: Option<f64>,
    pub level_counts: RiskLevelCounts,
    pub top20: Vec<RiskScoreRow>,
}

fn normalize_risk_level(s: &str) -> String {
    match s.trim().to_ascii_lowercase().as_str() {
        "critical" => "critical".to_string(),
        "high" => "high".to_string(),
        "medium" => "medium".to_string(),
        "low" => "low".to_string(),
        _ => "low".to_string(),
    }
}

pub fn build_risk_score(data: &SheetData) -> RiskScorePanel {
    let rows: Vec<RiskScoreRow> = data
        .rows
        .iter()
        .map(|row| RiskScoreRow {
            deal_id: data.get(row, "deal_id").to_string(),
            deal_label: deal_label(
                data.get(row, "deal_id"),
                data.get(row, "deal_label"),
                data.get(row, "customer_label"),
                "",
            ),
            owner_label: owner_label(data.get(row, "owner_name"), "", data.get(row, "owner_id")),
            pipeline_stage: pipeline_stage(data.get(row, "pipeline_label"), data.get(row, "stage_label")),
            risk_score: num(data.get(row, "risk_score")),
            risk_level: normalize_risk_level(data.get(row, "risk_level")),
            days_since_last_contact: opt_num(data.get(row, "days_since_last_contact")),
            call_all: opt_num(data.get(row, "call_all")),
            call_post: opt_num(data.get(row, "call_post")),
            oubo_per_posting: opt_num(data.get(row, "oubo_per_posting")),
            top_drivers: data.get(row, "top_drivers").to_string(),
        })
        .collect();

    let mut level_counts = RiskLevelCounts::default();
    for r in &rows {
        match r.risk_level.as_str() {
            "critical" => level_counts.critical += 1,
            "high" => level_counts.high += 1,
            "medium" => level_counts.medium += 1,
            _ => level_counts.low += 1,
        }
    }
    let avg_risk_score = if rows.is_empty() {
        None
    } else {
        Some(rows.iter().map(|r| r.risk_score).sum::<f64>() / rows.len() as f64)
    };

    let mut sorted = rows.clone();
    sorted.sort_by(|a, b| {
        b.risk_score
            .partial_cmp(&a.risk_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let top20 = sorted.into_iter().take(RISK_TOP_LIMIT).collect();

    RiskScorePanel {
        total: rows.len(),
        avg_risk_score,
        level_counts,
        top20,
    }
}

// ================================================================== [2] マトリクス

#[derive(Debug, Serialize, Clone)]
pub struct MatrixPoint {
    pub deal_id: String,
    pub deal_label: String,
    pub owner_label: String,
    pub pipeline_stage: String,
    /// X: 接触量(Email+Call+MTG 総件数)
    pub x_contact_count: f64,
    /// Y: リスクスコア(0-100、高いほど悪い)。新 risk_score 列優先、
    /// 無ければ旧 continue_intent 列にフォールバック(GAS と同じキャッシュ互換措置)
    pub y_risk_score: f64,
    pub risk_level: String,
    /// right-top / left-top / right-bottom / left-bottom
    pub quadrant: String,
}

#[derive(Debug, Serialize, Default)]
pub struct QuadrantCounts {
    pub right_top: usize,
    pub left_top: usize,
    pub right_bottom: usize,
    pub left_bottom: usize,
}

#[derive(Debug, Serialize)]
pub struct MatrixPanel {
    pub points: Vec<MatrixPoint>,
    pub quadrant_counts: QuadrantCounts,
    /// 中央値。GAS と同じく偶数件でも2値平均せず `floor(len/2)` 番目の値を使う
    pub median_x: f64,
    pub median_y: f64,
}

fn median_gas_style(mut xs: Vec<f64>) -> f64 {
    if xs.is_empty() {
        return 0.0;
    }
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    xs[xs.len() / 2]
}

pub fn build_matrix(data: &SheetData) -> MatrixPanel {
    let points: Vec<MatrixPoint> = data
        .rows
        .iter()
        .map(|row| {
            let risk_score = data.get(row, "risk_score");
            let y = if !risk_score.trim().is_empty() {
                num(risk_score)
            } else {
                num(data.get(row, "continue_intent"))
            };
            MatrixPoint {
                deal_id: data.get(row, "deal_id").to_string(),
                deal_label: deal_label(
                    data.get(row, "deal_id"),
                    data.get(row, "deal_label"),
                    data.get(row, "customer_label"),
                    "",
                ),
                owner_label: owner_label(data.get(row, "owner_name"), "", data.get(row, "owner_id")),
                pipeline_stage: pipeline_stage(data.get(row, "pipeline_label"), data.get(row, "stage_label")),
                x_contact_count: num(data.get(row, "contact_count")),
                y_risk_score: y,
                risk_level: data.get(row, "risk_level").to_string(),
                quadrant: data.get(row, "quadrant").to_string(),
            }
        })
        .collect();

    let mut quadrant_counts = QuadrantCounts::default();
    for p in &points {
        match p.quadrant.as_str() {
            "right-top" => quadrant_counts.right_top += 1,
            "left-top" => quadrant_counts.left_top += 1,
            "right-bottom" => quadrant_counts.right_bottom += 1,
            "left-bottom" => quadrant_counts.left_bottom += 1,
            _ => {}
        }
    }

    let median_x = median_gas_style(points.iter().map(|p| p.x_contact_count).collect());
    let median_y = median_gas_style(points.iter().map(|p| p.y_risk_score).collect());

    MatrixPanel {
        points,
        quadrant_counts,
        median_x,
        median_y,
    }
}

// ================================================================== 4軸の注記

/// 画面の説明文の要点だけ抜粋。フロントが再実装で文言を落とさないための保険。
/// フル文面は GAS 版 `index.html` (`#page-prisk` の `.desc`) を正本とする。
pub const RISK_AXIS_NOTES: &[&str] = &[
    "①関係性 = NPS×契約フェーズ。NPSが取得できている約73%でのみ判定可能。残り約27%は判定不可(赤にできないだけで安全とは限らない)",
    "②モデル = LightGBM解約確率(50%以上で赤)。粗い補助(AUC約0.72)。過信禁物・自動判断には使わない",
    "③放置 = 最終接触からの経過(30日超/記録なしで赤)",
    "④収益 = Revenue at Risk(金額×解約確率=期待損失)。満了60日以内×高額(50万円以上)で赤。案件マネジメントタブのRevenue at Riskとは定義が異なる",
];

// ================================================================== ハンドラ

#[derive(Debug, Default, Deserialize)]
pub struct PriskQuery {
    pub consultant: Option<String>,
    pub min_red: Option<i32>,
    pub sort: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PriskData {
    pub board: BoardPanel,
    pub health: HealthPanel,
    pub risk_score: RiskScorePanel,
    pub matrix: MatrixPanel,
    pub risk_axis_notes: &'static [&'static str],
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

pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: PriskQuery,
) -> Result<TabPayload<PriskData>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let board_data = load(client, store, SHEET_BOARD, &mut sources).await?;
    let health_active = load(client, store, SHEET_HEALTH_ACTIVE, &mut sources).await?;
    let risk_score_data = load(client, store, SHEET_RISK_SCORE, &mut sources).await?;
    let matrix_data = load(client, store, SHEET_MATRIX, &mut sources).await?;

    // GAS: active が0件のときだけ全期間版にフォールバック
    let health = if health_active.rows.is_empty() {
        let fb = load(client, store, SHEET_HEALTH_FALLBACK, &mut sources).await?;
        build_health(&fb, SHEET_HEALTH_FALLBACK)
    } else {
        build_health(&health_active, SHEET_HEALTH_ACTIVE)
    };

    let board = build_board(&board_data, &q);
    let risk_score = build_risk_score(&risk_score_data);
    let matrix = build_matrix(&matrix_data);

    // 絞り込みのあるパネルは「何行が表示対象になったか」を実数で返す
    if let Some(s) = sources.iter_mut().find(|s| s.sheet == SHEET_BOARD) {
        s.matched_rows = board.rows.len();
    }

    Ok(TabPayload {
        data: PriskData {
            board,
            health,
            risk_score,
            matrix,
            risk_axis_notes: RISK_AXIS_NOTES,
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

    fn board_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        let header = vec![
            "deal_id", "customer_label", "consultant_name", "contract_type",
            "contract_period", "phase_bucket", "days_to_expiry", "amount",
            "ax1_relation", "latest_nps", "ax2_model_proba", "ax3_contact",
            "days_since_contact", "ax4_revenue_at_risk", "red_count",
            "overall_band", "main_factor",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        SheetData {
            header,
            rows: rows.into_iter().map(|r| arc_row(&r)).collect(),
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn 名前が取れないときdeal_idをそのまま名前にしない() {
        let s = deal_label("123", "", "", "");
        assert_eq!(s, "(名称未取得 / Deal 123)");
    }

    #[test]
    fn 名前があればそれを使う() {
        assert_eq!(deal_label("123", "", "株式会社テスト", ""), "株式会社テスト");
    }

    #[test]
    fn バンド件数は絞り込み前の全体で数える() {
        let d = board_sheet(vec![
            vec!["1", "A", "田中", "新規", "6", "終盤", "10", "100", "🚨", "3", "60", "🟢", "5", "500000", "3", "🔴最優先", "-"],
            vec!["2", "B", "鈴木", "新規", "6", "序盤", "50", "100", "🟢", "8", "10", "🟢", "1", "10000", "0", "🟢安定", "-"],
        ]);
        let q = PriskQuery { consultant: Some("田中".into()), min_red: Some(3), sort: None };
        let panel = build_board(&d, &q);
        // 担当=田中で絞っても、バンド件数(全体)は2件とも反映される
        assert_eq!(panel.band_counts.critical, 1);
        assert_eq!(panel.band_counts.stable, 1);
        assert_eq!(panel.rows.len(), 1, "表示行は絞り込み後の1件のみ");
    }

    #[test]
    fn 満了0日をexpirySortで末尾に沈めない() {
        // GAS の `a.days_to_expiry || 99999` バグを再現しない
        let d = board_sheet(vec![
            vec!["1", "A", "x", "", "", "", "0", "0", "", "", "", "", "", "0", "0", "🟢安定", ""],
            vec!["2", "B", "x", "", "", "", "10", "0", "", "", "", "", "", "0", "0", "🟢安定", ""],
        ]);
        let q = PriskQuery { consultant: None, min_red: None, sort: Some("expiry".into()) };
        let panel = build_board(&d, &q);
        assert_eq!(panel.rows[0].deal_id, "1", "0日は99999扱いにせず最優先で出す");
    }

    #[test]
    fn expiry欠損は最後に並ぶ() {
        let d = board_sheet(vec![
            vec!["1", "A", "x", "", "", "", "", "0", "", "", "", "", "", "0", "0", "🟢安定", ""],
            vec!["2", "B", "x", "", "", "", "10", "0", "", "", "", "", "", "0", "0", "🟢安定", ""],
        ]);
        let q = PriskQuery { consultant: None, min_red: None, sort: Some("expiry".into()) };
        let panel = build_board(&d, &q);
        assert_eq!(panel.rows[0].deal_id, "2");
        assert_eq!(panel.rows[1].deal_id, "1", "欠損は99999扱いで末尾");
    }

    #[test]
    fn red_countの既定ソートは赤軸数優先rar劣後() {
        let d = board_sheet(vec![
            vec!["1", "A", "x", "", "", "", "", "0", "", "", "", "", "", "100", "1", "🟡監視", ""],
            vec!["2", "B", "x", "", "", "", "", "0", "", "", "", "", "", "500", "2", "🟠要注意", ""],
        ]);
        let q = PriskQuery::default();
        let panel = build_board(&d, &q);
        assert_eq!(panel.rows[0].deal_id, "2", "赤軸数2の方が先");
    }

    #[test]
    fn drop列の各表記形式を真として扱う() {
        for v in ["true", "True", "TRUE", "1"] {
            assert!(is_drop(v), "{v} は真として扱われるべき");
        }
        assert!(!is_drop("false"));
        assert!(!is_drop(""));
    }

    #[test]
    fn risk_levelの想定外表記はlowに丸める() {
        assert_eq!(normalize_risk_level("critical"), "critical");
        assert_eq!(normalize_risk_level("unknown"), "low");
        assert_eq!(normalize_risk_level(""), "low");
    }

    #[test]
    fn マトリクスは象限件数を数える() {
        let header = vec![
            "deal_id", "deal_label", "customer_label", "owner_id", "owner_name",
            "pipeline_label", "stage_label", "contact_count", "risk_score",
            "risk_level", "continue_intent", "quadrant",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let d = SheetData {
            header,
            rows: vec![
                arc_row(&["1", "A", "A", "1", "田中", "PL", "St", "10", "80", "high", "", "left-top"]),
                arc_row(&["2", "B", "B", "1", "田中", "PL", "St", "50", "20", "low", "", "right-bottom"]),
            ],
            fetched_at: Instant::now(),
        };
        let panel = build_matrix(&d);
        assert_eq!(panel.quadrant_counts.left_top, 1);
        assert_eq!(panel.quadrant_counts.right_bottom, 1);
    }

    #[test]
    fn マトリクスはrisk_score優先でcontinue_intentにフォールバック() {
        let header = vec![
            "deal_id", "deal_label", "customer_label", "owner_id", "owner_name",
            "pipeline_label", "stage_label", "contact_count", "risk_score",
            "risk_level", "continue_intent", "quadrant",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let d = SheetData {
            header,
            rows: vec![arc_row(&["1", "A", "A", "1", "田中", "PL", "St", "10", "", "low", "42", "left-top"])],
            fetched_at: Instant::now(),
        };
        let panel = build_matrix(&d);
        assert_eq!(panel.points[0].y_risk_score, 42.0, "risk_score空欄はcontinue_intentへフォールバック");
    }
}
