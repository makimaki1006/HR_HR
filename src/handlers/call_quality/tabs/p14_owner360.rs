//! コンサル担当者 360°ビュー（GAS 版 `page-p14` の移植）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p14">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP14Consultant360` / `_p14PopulateSelector` / `_drawP14Summary` /
//!          `_p14DrawNpsTrend` / `_p14DrawMonthlyTrend` / `_p14RenderPrefectureChart` /
//!          `_p14DrawContactHeatmap`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getConsultant360Kpi` / `getConsultant360Deals` / `getConsultant360Prefecture` /
//!          `getConsultantMonthlyTrend` / `getConsultantContactLog`）
//!
//! **担当者を選択して深掘りする画面**であり、上部フィルタ(期間/PL/メンバー/都道府県)は
//! 反映しない（GAS 版と同じ、`consultant_id` セレクタ駆動）。
//!
//! ------------------------------------------------------------------
//! 顧客名が取れないときの規約
//! ------------------------------------------------------------------
//! GAS 版 `_drawP14Summary`(16839行) は `customer_label || dealId` で
//! **Deal ID をそのまま名前として返していた**。ここでは他タブ(p8 の
//! `(名称未取得 / Deal N)`)と同じ規約に統一し、**名前でないと分かる形にする**。
//!
//! ------------------------------------------------------------------
//! GAS 版に入っていた誤りを持ち込まないこと
//! ------------------------------------------------------------------
//! KPI カードの churn_rate/retention_rate/avg_risk_score/avg_monthly_contact を
//! GAS は `parseFloat(kpi.churn_rate) || 0` のように **空文字や非数値を無条件で 0 にしていた**
//! （= 「値が無い」と「0」を区別できない）。ここでは `opt_num()` で空文字は `None` にし、
//! 0% と誤読させない(`tabs/mod.rs` 約束2)。churn_rate/retention_rate 自体は
//! 失敗3種(churn_failure/churn_total 等)の合成でありここでは再計算せず、
//! Python バッチが書いた値をそのまま使う(GAS と同じ。B層のような単純な
//! 「継続÷決着」ではなく再計算の根拠を実データで確認していないため)。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. 成果スコア/手入力5指標(継続意向・応募者満足・採用者満足・コンサル評価・充足率)の
//!    健全性カード（旧 `p14-health-cards`）
//!    → 移植しない。GAS 側で 2026-06-05 にユーザー指示で**既に全廃**済み
//!      （`healthCards.innerHTML = ''` 固定、band関数もコメントで「未使用」）。
//! 2. 担当者セレクタの名前/メール検索フィルタ（GAS `p14-search`）
//!    → 実装しない。`consultants` を全件返すので、検索・絞り込みはフロント側の
//!      責務とする（サーバは「担当者一覧を作るためのデータ」を返すだけ）。
//! 3. 接触ログのヒートマップ配色・グリッド化（週×案件のマス目とグラデーション）
//!    → 未実装。`contact_log` は「Deal × 週」の生データのみ返し、グリッド化・
//!      色分けはフロント側の責務とする(他タブの時間帯ヒートと同方針)。
//! 4. NPS推移・都道府県棒グラフ・月次推移チャートの描画そのもの(Chart.js)
//!    → 対象外。データ(`nps_trend` / `prefecture` / `monthly_trend`)のみ返す。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_KPI: &str = "コンサル担当者360_KPI";
const SHEET_DEALS: &str = "コンサル担当者360_Deal一覧";
const SHEET_PREFECTURE: &str = "コンサル担当者360_都道府県";
const SHEET_MONTHLY_TREND: &str = "コンサル担当者月次推移";
const SHEET_CONTACT_LOG: &str = "コンサル接触ログ_週次";

/// Deal 一覧の表示上限(安全弁。実データは1担当あたり数十〜百件程度)
const DEALS_LIMIT: usize = 500;

// ---------------------------------------------------------------- 小道具

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 「値が無い」と「0」を区別したいときに使う。空文字は None。
fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

/// シートの真偽値。Python バッチは `True`/`False` と書く(先頭大文字)ため大小無視で比較する。
fn truthy(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("true")
}

/// 顧客名。取れない場合は Deal ID をそのまま名前として返さない(ファイル冒頭の規約)。
fn deal_label(raw: &str, deal_id: &str) -> String {
    let l = raw.trim();
    if l.is_empty() {
        format!("(名称未取得 / Deal {deal_id})")
    } else {
        l.to_string()
    }
}

fn hubspot_deal_url(deal_id: &str) -> String {
    // ポータル 23708633 = リクロジ事業部
    format!("https://app.hubspot.com/contacts/23708633/deal/{deal_id}")
}

// ---------------------------------------------------------------- バンド(帯)

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Band {
    Good,
    Warn,
    Bad,
    Neutral,
}

// ============================================================ KPIカード

#[derive(Debug, Serialize)]
pub struct KpiCards {
    pub active_deal_count: f64,
    pub churn_failure: f64,
    pub churn_total: f64,
    pub continuing: f64,
    pub success_fulfilled: f64,
    /// シート `churn_rate`(0..1) を % に変換。空文字は None(0%と誤読させない)
    pub churn_rate_pct: Option<f64>,
    pub retention_rate_pct: Option<f64>,
    pub critical_pred_count: f64,
    pub high_pred_count: f64,
    pub task_alert_count: f64,
    pub avg_risk_score: Option<f64>,
    pub avg_monthly_contact: Option<f64>,
    pub last_activity_at: String,
    pub consultant_email: String,
    pub churn_band: Band,
    pub critical_band: Band,
    pub high_band: Band,
    pub task_band: Band,
    pub contact_band: Band,
    pub risk_band: Band,
}

/// シート「コンサル担当者360_KPI」の1行からKPIカードを組む。
/// 列: consultant_id,consultant_name,consultant_email,active_deal_count,churn_failure,
///     churn_total,continuing,success_fulfilled,churn_rate,retention_rate,fulfillment_rate,
///     avg_risk_score,critical_pred_count,high_pred_count,task_alert_count,
///     avg_monthly_contact,last_activity_at,nps_round_json 他
pub fn build_kpi_cards(d: &SheetData, row: &[Arc<str>]) -> KpiCards {
    let churn_rate_pct = opt_num(d.get(row, "churn_rate")).map(|v| v * 100.0);
    let retention_rate_pct = opt_num(d.get(row, "retention_rate")).map(|v| v * 100.0);
    let avg_risk_score = opt_num(d.get(row, "avg_risk_score"));
    let avg_monthly_contact = opt_num(d.get(row, "avg_monthly_contact"));
    let critical = num(d.get(row, "critical_pred_count"));
    let high = num(d.get(row, "high_pred_count"));
    let task = num(d.get(row, "task_alert_count"));

    KpiCards {
        active_deal_count: num(d.get(row, "active_deal_count")),
        churn_failure: num(d.get(row, "churn_failure")),
        churn_total: num(d.get(row, "churn_total")),
        continuing: num(d.get(row, "continuing")),
        success_fulfilled: num(d.get(row, "success_fulfilled")),
        churn_rate_pct,
        retention_rate_pct,
        critical_pred_count: critical,
        high_pred_count: high,
        task_alert_count: task,
        avg_risk_score,
        avg_monthly_contact,
        last_activity_at: {
            let v = d.get(row, "last_activity_at").trim();
            if v.is_empty() { "―".to_string() } else { v.to_string() }
        },
        consultant_email: d.get(row, "consultant_email").to_string(),
        // 2026-08-17 是正: 閾値35%/25%で赤/橙/緑を付けていた（51名中32名が赤）。
        //   GAS(javascript.html:16771)は churnBand を計算しておきながら
        //   **カードにはリテラル `'neutral'` を渡している**。書き忘れではなく判断で、
        //   副文に理由が書かれている:
        //     「分母=累計(未決着active含む)。**交絡(顧客層)含み優劣評価でない**」
        //   担当者ごとに顧客層が違うので、率の高低を担当者の優劣として
        //   読ませない、という判断。色を付けるとその判断が消える。
        //   このプロジェクトには「負相関を片方向の因果に決めない」という
        //   明文の規律があり、これはその系列。
        churn_band: Band::Neutral,
        critical_band: if critical >= 5.0 {
            Band::Bad
        } else if critical >= 2.0 {
            Band::Warn
        } else {
            Band::Good
        },
        high_band: if high > 0.0 { Band::Warn } else { Band::Good },
        task_band: if task >= 3.0 {
            Band::Bad
        } else if task >= 1.0 {
            Band::Warn
        } else {
            Band::Good
        },
        contact_band: match avg_monthly_contact {
            None => Band::Neutral,
            Some(v) if v >= 5.0 => Band::Good,
            Some(v) if v >= 2.0 => Band::Warn,
            Some(_) => Band::Bad,
        },
        // 上の churn_band と同じ理由で無色。GAS(javascript.html:16781)も
        //   riskBand を計算したうえでリテラル `'neutral'` を渡している。
        //   副文:「稼働中の案件の risk_score 平均 (0-100)・
        //          **交絡含み優劣評価でない**」
        risk_band: Band::Neutral,
    }
}

// ============================================================ NPS定期回推移

#[derive(Debug, Serialize)]
pub struct NpsPoint {
    /// 定期回のラベル(例: "定期①")
    pub period: String,
    pub avg: f64,
    pub n: f64,
}

/// `nps_round_json` = `[{"r": "定期①", "avg": 6.78, "n": 64}, ...]` をパースする。
/// パース不能・空文字は空配列(GAS `_p14DrawNpsTrend` の try/catch と同じ挙動)。
fn parse_nps_round_json(raw: &str) -> Vec<NpsPoint> {
    let t = raw.trim();
    if t.is_empty() {
        return Vec::new();
    }
    let v: serde_json::Value = match serde_json::from_str(t) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };
    let Some(arr) = v.as_array() else { return Vec::new() };
    arr.iter()
        .filter_map(|item| {
            let obj = item.as_object()?;
            let period = obj.get("r").and_then(|v| v.as_str()).unwrap_or("").to_string();
            let avg = obj.get("avg").and_then(|v| v.as_f64())?;
            let n = obj.get("n").and_then(|v| v.as_f64()).unwrap_or(0.0);
            Some(NpsPoint { period, avg, n })
        })
        .collect()
}

// ============================================================ 都道府県分布

/// 北海道→沖縄県の表示順(GAS `P14_PREFECTURES_47`、javascript.html 15947-15956行)。
const PREFECTURES_47: [&str; 47] = [
    "北海道", "青森県", "岩手県", "宮城県", "秋田県", "山形県", "福島県", "茨城県", "栃木県", "群馬県",
    "埼玉県", "千葉県", "東京都", "神奈川県", "新潟県", "富山県", "石川県", "福井県", "山梨県", "長野県",
    "岐阜県", "静岡県", "愛知県", "三重県", "滋賀県", "京都府", "大阪府", "兵庫県", "奈良県", "和歌山県",
    "鳥取県", "島根県", "岡山県", "広島県", "山口県", "徳島県", "香川県", "愛媛県", "高知県", "福岡県",
    "佐賀県", "長崎県", "熊本県", "大分県", "宮崎県", "鹿児島県", "沖縄県",
];

#[derive(Debug, Serialize)]
pub struct PrefectureRow {
    pub prefecture: String,
    pub deal_count: f64,
    pub active_deal_count: f64,
}

/// シート「コンサル担当者360_都道府県」(列: consultant_id,consultant_name,prefecture,
/// deal_count,active_deal_count)から選択コンサルの47都道府県分布を作る。
/// 0件の都道府県も padding して返す(GAS 版と同じ、棒グラフの並びを固定するため)。
/// 「不明」が実在する場合のみ末尾に追加する。
pub fn build_prefecture(d: &SheetData, consultant_id: &str) -> Vec<PrefectureRow> {
    let mut map: HashMap<String, (f64, f64)> = HashMap::new();
    for row in &d.rows {
        if d.get(row, "consultant_id") != consultant_id {
            continue;
        }
        let p = d.get(row, "prefecture").to_string();
        if p.is_empty() {
            continue;
        }
        let e = map.entry(p).or_insert((0.0, 0.0));
        e.0 += num(d.get(row, "deal_count"));
        e.1 += num(d.get(row, "active_deal_count"));
    }

    let mut out: Vec<PrefectureRow> = PREFECTURES_47
        .iter()
        .map(|p| {
            let (deal, active) = map.get(*p).copied().unwrap_or((0.0, 0.0));
            PrefectureRow {
                prefecture: p.to_string(),
                deal_count: deal,
                active_deal_count: active,
            }
        })
        .collect();
    if let Some((deal, active)) = map.get("不明") {
        out.push(PrefectureRow {
            prefecture: "不明".to_string(),
            deal_count: *deal,
            active_deal_count: *active,
        });
    }
    out
}

// ============================================================ 月次推移

#[derive(Debug, Serialize)]
pub struct MonthlyTrendRow {
    pub month: String,
    pub mrr: f64,
    pub won_new: f64,
    pub won_renewal: f64,
    pub held_deals: f64,
    pub expiring: f64,
    pub renewed: f64,
    /// シート `continuation_rate`(0..1) を % に変換。空文字は None
    pub continuation_rate_pct: Option<f64>,
    pub churned_mrr: f64,
    /// 当月(集計途中)かどうか。画面で「●進行中」を出すための旗(GAS `_isPartialMonth`)
    pub is_partial: bool,
}

/// シート「コンサル担当者月次推移」(列: consultant_id,consultant_name,month,mrr,
/// won_new,won_renewal,held_deals,expiring,renewed,continuation_rate,churned_mrr)から
/// 選択コンサルの月次行を月昇順で返す。
/// 月ラベルを `(年, 月)` に読む。読めなければ None。
///
/// 2026-08-17 追加。シートには `2026-08` と `2026-8` が混在している
/// （書込が USER_ENTERED なので Google スプシが日付として解釈し、
/// `yyyy-m` で描き戻す。1〜9月だけゼロ埋めが落ちる）。
///
/// **文字列比較のままだと `2026-5` が `2026-08` より後ろに来る。**
/// 実測: 51名中30名でラベルが崩れ、**16名は折れ線の最終点が誤り**。
/// 2026-08 までデータがある担当者のグラフが 2026-1 で終わっていた。
///
/// 生成側（`backup.py`）は 2026-08-17 に是正済みだが、シートに何が
/// 入っていても壊れないようにここでも読めるようにしておく。
/// 同じパースを `is_partial` の判定にも使う。**片方だけ直すと再発する。**
fn parse_ym(s: &str) -> Option<(i32, u32)> {
    let t = s.trim();
    let (y, m) = t.split_once('-')?;
    let y: i32 = y.trim().parse().ok()?;
    let m: u32 = m.trim().parse().ok()?;
    if !(1..=12).contains(&m) {
        return None;
    }
    Some((y, m))
}

/// 2つの月ラベルが同じ月を指すか。`2026-8` と `2026-08` を同一とみなす。
fn same_ym(a: &str, b: &str) -> bool {
    match (parse_ym(a), parse_ym(b)) {
        (Some(x), Some(y)) => x == y,
        // 読めないものは従来どおり文字列で比べる（勝手に一致させない）
        _ => a.trim() == b.trim(),
    }
}

pub fn build_monthly_trend(d: &SheetData, consultant_id: &str, current_month: &str) -> Vec<MonthlyTrendRow> {
    let mut rows: Vec<MonthlyTrendRow> = d
        .rows
        .iter()
        .filter(|r| d.get(r, "consultant_id") == consultant_id)
        .map(|r| {
            let month = d.get(r, "month").to_string();
            MonthlyTrendRow {
                is_partial: same_ym(&month, current_month),
                mrr: num(d.get(r, "mrr")),
                won_new: num(d.get(r, "won_new")),
                won_renewal: num(d.get(r, "won_renewal")),
                held_deals: num(d.get(r, "held_deals")),
                expiring: num(d.get(r, "expiring")),
                renewed: num(d.get(r, "renewed")),
                continuation_rate_pct: opt_num(d.get(r, "continuation_rate")).map(|v| v * 100.0),
                churned_mrr: num(d.get(r, "churned_mrr")),
                month,
            }
        })
        .collect();
    // 暦順に並べる。読めないラベルは末尾へ回し、その中では文字列順で安定させる。
    rows.sort_by(|a, b| match (parse_ym(&a.month), parse_ym(&b.month)) {
        (Some(x), Some(y)) => x.cmp(&y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => a.month.cmp(&b.month),
    });
    rows
}

// ============================================================ Deal一覧

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DealStatusFilter {
    All,
    Active,
    Continued,
    Churned,
}

impl DealStatusFilter {
    fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("all").trim() {
            "active" => Self::Active,
            "continued" => Self::Continued,
            "churned" => Self::Churned,
            _ => Self::All,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::All => "all",
            Self::Active => "active",
            Self::Continued => "continued",
            Self::Churned => "churned",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct DealRow {
    pub deal_id: String,
    pub label: String,
    pub is_active: bool,
    pub prefecture: String,
    pub pipeline_label: String,
    pub stage_label: String,
    pub deal_age_days: Option<f64>,
    pub days_since_last_contact: Option<f64>,
    pub risk_score: Option<f64>,
    /// active の Deal のみ意味を持つ(GAS: 非アクティブは "-" 表示)。0..100(%)
    pub churn_proba_90d_pct: Option<f64>,
    pub risk_level: String,
    pub has_task_alert: bool,
    pub task_alert_category: String,
    pub monthly_contact_avg: Option<f64>,
    pub latest_nps: Option<f64>,
    pub latest_nps_period: String,
    pub latest_seika: Option<f64>,
    pub latest_sufficiency: Option<f64>,
    pub latest_continue_intent: Option<f64>,
    pub hubspot_url: String,
}

#[derive(Debug, Serialize)]
pub struct DealsTable {
    pub status_filter: &'static str,
    pub rows: Vec<DealRow>,
    /// 状態フィルタ適用前・選択コンサルの全Deal数
    pub total_rows: usize,
    pub truncated: bool,
    pub limit: usize,
}

/// シート「コンサル担当者360_Deal一覧」から選択コンサルのDealを状態フィルタ+ソートして返す。
///
/// ソート: is_active(稼働中)優先 → churn_proba_90d 降順 → 最終接触経過日数 降順
/// （GAS `_drawP14Summary` のソート仕様と同じ）。
pub fn build_deals(d: &SheetData, consultant_id: &str, filter: DealStatusFilter) -> DealsTable {
    let mine: Vec<&Vec<Arc<str>>> = d
        .rows
        .iter()
        .filter(|r| d.get(r, "consultant_id") == consultant_id)
        .collect();
    let total_rows = mine.len();

    let mut filtered: Vec<&Vec<Arc<str>>> = mine
        .into_iter()
        .filter(|r| {
            let is_active = truthy(d.get(r, "is_active"));
            let stage = d.get(r, "stage_label");
            match filter {
                DealStatusFilter::All => true,
                DealStatusFilter::Active => is_active,
                DealStatusFilter::Churned => !is_active && stage.contains("解約"),
                DealStatusFilter::Continued => !is_active && stage.contains("継続"),
            }
        })
        .collect();

    filtered.sort_by(|a, b| {
        let aa = truthy(d.get(a, "is_active"));
        let bb = truthy(d.get(b, "is_active"));
        if aa != bb {
            return bb.cmp(&aa);
        }
        let ap = opt_num(d.get(a, "churn_proba_90d")).unwrap_or(0.0);
        let bp = opt_num(d.get(b, "churn_proba_90d")).unwrap_or(0.0);
        if ap != bp {
            return bp.partial_cmp(&ap).unwrap_or(std::cmp::Ordering::Equal);
        }
        let ad = opt_num(d.get(a, "days_since_last_contact")).unwrap_or(0.0);
        let bd = opt_num(d.get(b, "days_since_last_contact")).unwrap_or(0.0);
        bd.partial_cmp(&ad).unwrap_or(std::cmp::Ordering::Equal)
    });

    let truncated = filtered.len() > DEALS_LIMIT;
    let rows: Vec<DealRow> = filtered
        .iter()
        .take(DEALS_LIMIT)
        .map(|r| {
            let deal_id = d.get(r, "deal_id").to_string();
            let label = deal_label(d.get(r, "customer_label"), &deal_id);
            let is_active = truthy(d.get(r, "is_active"));
            DealRow {
                is_active,
                prefecture: d.get(r, "prefecture").to_string(),
                pipeline_label: d.get(r, "pipeline_label").to_string(),
                stage_label: d.get(r, "stage_label").to_string(),
                deal_age_days: opt_num(d.get(r, "deal_age_days")),
                days_since_last_contact: opt_num(d.get(r, "days_since_last_contact")),
                risk_score: opt_num(d.get(r, "risk_score")),
                churn_proba_90d_pct: if is_active {
                    opt_num(d.get(r, "churn_proba_90d")).map(|v| v * 100.0)
                } else {
                    None
                },
                risk_level: d.get(r, "risk_level").to_string(),
                has_task_alert: truthy(d.get(r, "has_task_alert")),
                task_alert_category: d.get(r, "task_alert_category").to_string(),
                monthly_contact_avg: opt_num(d.get(r, "monthly_contact_avg")),
                latest_nps: opt_num(d.get(r, "latest_nps")),
                latest_nps_period: d.get(r, "latest_nps_period").to_string(),
                latest_seika: opt_num(d.get(r, "latest_seika")),
                latest_sufficiency: opt_num(d.get(r, "latest_sufficiency")),
                latest_continue_intent: opt_num(d.get(r, "latest_continue_intent")),
                hubspot_url: hubspot_deal_url(&deal_id),
                deal_id,
                label,
            }
        })
        .collect();

    DealsTable {
        status_filter: filter.as_str(),
        rows,
        total_rows,
        truncated,
        limit: DEALS_LIMIT,
    }
}

// ============================================================ 接触ログ(案件×週)

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ContactStatusFilter {
    Active,
    All,
    Churned,
}

impl ContactStatusFilter {
    fn parse(s: Option<&str>) -> Self {
        match s.unwrap_or("active").trim() {
            "all" => Self::All,
            "churned" => Self::Churned,
            _ => Self::Active,
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::All => "all",
            Self::Churned => "churned",
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ContactWeekRow {
    pub week: String,
    pub call_count: f64,
    pub mtg_count: f64,
}

#[derive(Debug, Serialize)]
pub struct ContactDeal {
    pub deal_id: String,
    pub label: String,
    /// Deal一覧シート(コンサル担当者360_Deal一覧)から引いた状態。参照Dealが無ければ false
    pub is_active: bool,
    pub days_since_last_contact: Option<f64>,
    /// 週昇順
    pub weeks: Vec<ContactWeekRow>,
    pub total_call: f64,
    pub total_mtg: f64,
}

#[derive(Debug, Serialize)]
pub struct ContactLogPanel {
    pub status_filter: &'static str,
    /// 表示期間(週)。0 = 全期間
    pub weeks_shown: u32,
    /// Deal単位(週昇順)。グリッド化・配色はフロント側の責務(未実装一覧を参照)
    pub deals: Vec<ContactDeal>,
    /// 打ち切り前の件数。`truncated` が立ったとき「何件のうち何件か」を出すため。
    pub total_deals: usize,
    /// 上限で切ったか。黙って上位N件にしない（このリポジトリの約束 3.）
    pub truncated: bool,
    /// シート全体で最も新しい週(YYYY-MM-DD、月曜)
    pub max_week: String,
}

/// 接触ログの表示上限。GAS の `.slice(0, 60)` に合わせる。
const MAX_CONTACT_DEALS: usize = 60;

/// シート「コンサル接触ログ_週次」(列: consultant_id,consultant_name,deal_id,
/// customer_label,week,call_count,mtg_count)を選択コンサル+状態+期間で絞り、
/// Deal単位に畳んで返す。状態(is_active)は「コンサル担当者360_Deal一覧」から引く
/// (接触ログ自体には is_active が無いため)。
pub fn build_contact_log(
    log: &SheetData,
    deals: &SheetData,
    consultant_id: &str,
    filter: ContactStatusFilter,
    weeks_window: u32,
) -> ContactLogPanel {
    let mut deal_status: HashMap<String, (bool, Option<f64>)> = HashMap::new();
    for r in &deals.rows {
        if deals.get(r, "consultant_id") != consultant_id {
            continue;
        }
        let id = deals.get(r, "deal_id").to_string();
        deal_status.insert(
            id,
            (
                truthy(deals.get(r, "is_active")),
                opt_num(deals.get(r, "days_since_last_contact")),
            ),
        );
    }

    let mut max_week = String::new();
    for r in &log.rows {
        let w = log.get(r, "week").trim();
        if w > max_week.as_str() {
            max_week = w.to_string();
        }
    }

    let week_floor: Option<String> = if weeks_window == 0 || max_week.is_empty() {
        None
    } else {
        chrono::NaiveDate::parse_from_str(&max_week, "%Y-%m-%d")
            .ok()
            .map(|base| (base - chrono::Duration::days(7 * (weeks_window as i64 - 1))).format("%Y-%m-%d").to_string())
    };

    let mut by_deal: HashMap<String, (String, Vec<ContactWeekRow>)> = HashMap::new();
    for r in &log.rows {
        if log.get(r, "consultant_id") != consultant_id {
            continue;
        }
        let week = log.get(r, "week").trim().to_string();
        if week.is_empty() {
            continue;
        }
        if let Some(floor) = &week_floor {
            if week.as_str() < floor.as_str() {
                continue;
            }
        }
        let deal_id = log.get(r, "deal_id").trim().to_string();
        if deal_id.is_empty() {
            continue;
        }
        let label = log.get(r, "customer_label").trim().to_string();
        let e = by_deal.entry(deal_id).or_insert_with(|| (label, Vec::new()));
        e.1.push(ContactWeekRow {
            week,
            call_count: num(log.get(r, "call_count")),
            mtg_count: num(log.get(r, "mtg_count")),
        });
    }

    let mut deals_out: Vec<ContactDeal> = by_deal
        .into_iter()
        .filter_map(|(deal_id, (label, mut weeks))| {
            weeks.sort_by(|a, b| a.week.cmp(&b.week));
            let (is_active, days_since) = deal_status.get(&deal_id).copied().unwrap_or((false, None));
            match filter {
                ContactStatusFilter::All => {}
                ContactStatusFilter::Active => {
                    if !is_active {
                        return None;
                    }
                }
                ContactStatusFilter::Churned => {
                    if is_active {
                        return None;
                    }
                }
            }
            let total_call: f64 = weeks.iter().map(|w| w.call_count).sum();
            let total_mtg: f64 = weeks.iter().map(|w| w.mtg_count).sum();
            let label = deal_label(&label, &deal_id);
            Some(ContactDeal {
                is_active,
                days_since_last_contact: days_since,
                weeks,
                total_call,
                total_mtg,
                deal_id,
                label,
            })
        })
        .collect();

    // 2026-08-17 是正: Deal ID 昇順にしていた。
    //   決定性は得られるが、**このパネルの目的である「放置の可視化」が失われる**。
    //   GAS(javascript.html:16418-16424)は
    //     稼働中を優先 → 最終接触からの日数 降順 → 接触量 降順
    //   実測(松野 日向子・全期間): GAS の先頭は 46日放置の稼働中案件、
    //   旧実装の先頭は 37日の**非稼働**案件。最も放置されている案件が
    //   先頭に来ない = 見るべきものが埋もれる。
    //   決定性は最後に deal_id を足して担保する（同値でも順序が揺れない）。
    deals_out.sort_by(|a, b| {
        b.is_active
            .cmp(&a.is_active)
            .then_with(|| {
                b.days_since_last_contact
                    .partial_cmp(&a.days_since_last_contact)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| {
                (b.total_call + b.total_mtg)
                    .partial_cmp(&(a.total_call + a.total_mtg))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });

    // GAS は `.slice(0, 60)` で打ち切る。実測で 松野 日向子 は全期間 192件
    // 返っており、**132件は GAS では表示されないもの**だった。
    // 黙って切らず truncated を立てる（このリポジトリの約束 3.）。
    let total_deals = deals_out.len();
    let truncated = total_deals > MAX_CONTACT_DEALS;
    deals_out.truncate(MAX_CONTACT_DEALS);

    ContactLogPanel {
        status_filter: filter.as_str(),
        weeks_shown: weeks_window,
        deals: deals_out,
        total_deals,
        truncated,
        max_week,
    }
}

// ============================================================ 全体

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct P14Query {
    /// 選択担当者。未指定なら `selected` は None(画面はセレクタ一覧のみ表示)
    pub consultant_id: Option<String>,
    /// Deal一覧の状態フィルタ。all/active/continued/churned。省略時 all
    pub deals_status: Option<String>,
    /// 接触ログの対象。active/all/churned。省略時 active
    pub contact_status: Option<String>,
    /// 接触ログの表示期間(週)。16/26/52/0=全期間。省略時 26(GAS の既定値)
    pub contact_weeks: Option<u32>,
    /// テスト用の「現在月」上書き。省略時は実行時のローカル日付
    pub today_ym: Option<String>,
}

crate::accepted_params!(P14Query, p14_query_accepted =>
    "consultant_id", "deals_status", "contact_status", "contact_weeks", "today_ym");

#[derive(Debug, Serialize)]
pub struct ConsultantOption {
    pub consultant_id: String,
    pub consultant_name: String,
    pub consultant_email: String,
    pub active_deal_count: f64,
}

#[derive(Debug, Serialize)]
pub struct ConsultantDetail {
    pub consultant_id: String,
    pub consultant_name: String,
    pub kpi: KpiCards,
    pub nps_trend: Vec<NpsPoint>,
    pub prefecture: Vec<PrefectureRow>,
    pub monthly_trend: Vec<MonthlyTrendRow>,
    pub deals: DealsTable,
    pub contact_log: ContactLogPanel,
}

/// 画面に固定で出す注意書き。**消さないこと**。
///
/// 2026-08-17 追加。GAS では該当カードの副文に書かれていたが、移植時に
/// 落ちていた。同時に、GAS が意図的に無色にしていた2枚のカードに
/// Rust が色を付けており（51名中32名が赤）、**その色を付けない理由が
/// この注記そのもの**だった。注記を落とすと判断の根拠も一緒に消える。
const CAUTION_NOTES: &[&str] = &[
    "解約率の分母は累計（未決着の稼働中案件を含む）です。担当者ごとに顧客層が違うため、この率の高低を担当者の優劣として読まないでください。",
    "平均代理リスクは稼働中案件の risk_score の平均(0-100)です。これも顧客層の交絡を含むため、優劣評価には使えません。",
    "解約分析タブの解約率とは分母が違います。コンサル接触タブは「決着した案件のみ」を分母にしており、別物です。",
];

#[derive(Debug, Serialize)]
pub struct P14Data {
    /// 担当者セレクタ用の一覧。active_deal_count 降順(GAS `_p14PopulateSelector`)
    pub consultants: Vec<ConsultantOption>,
    pub selected: Option<ConsultantDetail>,
    /// 率の読み方についての注意書き。画面上部に固定で出す。
    pub notes: &'static [&'static str],
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

/// ハンドラ本体。5シートを読み、担当者一覧 + (選択時のみ)深掘りデータを返す。
pub async fn handle(client: &SheetsClient, store: &SheetStore, q: P14Query) -> Result<TabPayload<P14Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let kpi = load(client, store, SHEET_KPI, &mut sources).await?;
    let deals = load(client, store, SHEET_DEALS, &mut sources).await?;
    let prefecture = load(client, store, SHEET_PREFECTURE, &mut sources).await?;
    let monthly_trend = load(client, store, SHEET_MONTHLY_TREND, &mut sources).await?;
    let contact_log = load(client, store, SHEET_CONTACT_LOG, &mut sources).await?;

    let mut consultants: Vec<ConsultantOption> = kpi
        .rows
        .iter()
        .map(|r| {
            let consultant_id = kpi.get(r, "consultant_id").to_string();
            ConsultantOption {
                consultant_name: {
                    let n = kpi.get(r, "consultant_name").trim();
                    if n.is_empty() { consultant_id.clone() } else { n.to_string() }
                },
                consultant_email: kpi.get(r, "consultant_email").to_string(),
                active_deal_count: num(kpi.get(r, "active_deal_count")),
                consultant_id,
            }
        })
        .collect();
    // 2026-08-17 是正: 第2キーに consultant_id 昇順を足していた。
    //   GAS は `active_deal_count` 降順のみで、JS の sort は安定なので
    //   **同値はシートの行順が保たれる**。第2キーを足したせいで
    //   active=0 の同値が多い末尾で顔ぶれが入れ替わっていた（51名中22名で相違）。
    //   `sort_by` も安定ソートなので、キーを1本にすればシート行順が残る。
    consultants.sort_by(|a, b| {
        b.active_deal_count
            .partial_cmp(&a.active_deal_count)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let current_month = q
        .today_ym
        .clone()
        .unwrap_or_else(|| super::jst_current_ym());
    let deals_filter = DealStatusFilter::parse(q.deals_status.as_deref());
    let contact_filter = ContactStatusFilter::parse(q.contact_status.as_deref());
    let contact_weeks = q.contact_weeks.unwrap_or(26);

    let selected = match q.consultant_id.as_deref().filter(|s| !s.is_empty()) {
        None => None,
        Some(cid) => kpi.rows.iter().find(|r| kpi.get(r, "consultant_id") == cid).map(|kr| {
            let name = {
                let n = kpi.get(kr, "consultant_name").trim();
                if n.is_empty() { cid.to_string() } else { n.to_string() }
            };
            ConsultantDetail {
                consultant_id: cid.to_string(),
                consultant_name: name,
                kpi: build_kpi_cards(&kpi, kr),
                nps_trend: parse_nps_round_json(kpi.get(kr, "nps_round_json")),
                prefecture: build_prefecture(&prefecture, cid),
                monthly_trend: build_monthly_trend(&monthly_trend, cid, &current_month),
                deals: build_deals(&deals, cid, deals_filter),
                contact_log: build_contact_log(&contact_log, &deals, cid, contact_filter, contact_weeks),
            }
        }),
    };

    set_matched(
        &mut sources,
        SHEET_DEALS,
        selected.as_ref().map(|s| s.deals.total_rows).unwrap_or(0),
    );
    set_matched(
        &mut sources,
        SHEET_CONTACT_LOG,
        selected.as_ref().map(|s| s.contact_log.deals.len()).unwrap_or(0),
    );

    Ok(TabPayload {
        data: P14Data { consultants, selected, notes: CAUTION_NOTES },
        sources,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
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

    const DEALS_HEADER: [&str; 23] = [
        "consultant_id", "consultant_name", "deal_id", "customer_id", "customer_label",
        "pipeline_label", "stage_label", "is_active", "deal_age_days", "last_contact_at",
        "days_since_last_contact", "risk_score", "churn_proba_90d", "risk_level",
        "has_task_alert", "task_alert_category", "monthly_contact_avg", "prefecture",
        "latest_nps", "latest_nps_period", "latest_seika", "latest_sufficiency",
        "latest_continue_intent",
    ];

    #[test]
    fn 顧客名が取れないときはdeal_idをそのまま名前にしない() {
        let d = sheet(
            &DEALS_HEADER,
            &[&[
                "1", "藤巻", "999", "", "", "リクロジ_納品管理", "アクティブ", "True", "10", "",
                "", "0", "0", "", "False", "", "0", "東京都", "", "", "", "", "",
            ]],
        );
        let table = build_deals(&d, "1", DealStatusFilter::All);
        assert_eq!(table.rows[0].label, "(名称未取得 / Deal 999)");
        assert!(table.rows[0].label.contains("999"), "IDは失わない");
    }

    #[test]
    fn churn_rateの空文字はnoneで0パーセントと誤読させない() {
        // GAS版は `parseFloat(kpi.churn_rate) || 0` で空文字を無条件0にしていた誤り。
        let d = sheet(
            &["consultant_id", "consultant_name", "consultant_email", "active_deal_count",
              "churn_failure", "churn_total", "continuing", "success_fulfilled",
              "churn_rate", "retention_rate", "avg_risk_score", "critical_pred_count",
              "high_pred_count", "task_alert_count", "avg_monthly_contact", "last_activity_at"],
            &[&["1", "藤巻", "f@f-a-c.co.jp", "0", "0", "0", "0", "0", "", "", "", "0", "0", "0", "", ""]],
        );
        let row = &d.rows[0];
        let cards = build_kpi_cards(&d, row);
        assert_eq!(cards.churn_rate_pct, None);
        assert_eq!(cards.churn_band, Band::Neutral, "値が無ければneutral(good/badで断定しない)");
        assert_eq!(cards.avg_risk_score, None);
    }

    #[test]
    fn churn_rateは0から1を百分率に変換する() {
        let d = sheet(
            &["consultant_id", "consultant_name", "consultant_email", "active_deal_count",
              "churn_failure", "churn_total", "continuing", "success_fulfilled",
              "churn_rate", "retention_rate", "avg_risk_score", "critical_pred_count",
              "high_pred_count", "task_alert_count", "avg_monthly_contact", "last_activity_at"],
            &[&["1", "藤巻", "f@f-a-c.co.jp", "35", "45", "59", "48", "14", "0.4184", "0.5816", "22.06", "0", "3", "1", "8.1", "2026-06-08"]],
        );
        let row = &d.rows[0];
        let cards = build_kpi_cards(&d, row);
        assert!((cards.churn_rate_pct.unwrap() - 41.84).abs() < 1e-9);

        // 2026-08-17 変更: 以前は 35%以上=bad として赤を付けていた。
        //   GAS は churnBand を計算したうえで**カードにはリテラル `'neutral'`**
        //   を渡している。書き忘れではなく判断で、副文に理由が書かれている:
        //     「分母=累計(未決着active含む)。交絡(顧客層)含み優劣評価でない」
        //   実測では51名中32名が赤になり、担当者の優劣として読まれてしまう。
        //   **率は出す。色で優劣を示さない。**
        assert_eq!(
            cards.churn_band,
            Band::Neutral,
            "解約率は顧客層の交絡を含むので、色で優劣を示さない"
        );
        assert_eq!(
            cards.risk_band,
            Band::Neutral,
            "平均代理リスクも同じ理由で無色"
        );
        // 交絡の無い実数カードは従来どおり色を付ける（全部無色にしたのではない）
        assert_eq!(cards.task_band, Band::Warn, "タスク警告1件はwarn");
    }

    #[test]
    fn deal一覧のソートはアクティブ優先_churn降順_経過日数降順() {
        let d = sheet(
            &DEALS_HEADER,
            &[
                &["1", "藤巻", "1", "", "非アクティブ古い", "PL", "解約済", "False", "10", "", "500", "0", "0.1", "low", "False", "", "0", "", "", "", "", "", ""],
                &["1", "藤巻", "2", "", "アクティブ低risk", "PL", "アクティブ", "True", "10", "", "10", "0", "0.2", "low", "False", "", "0", "", "", "", "", "", ""],
                &["1", "藤巻", "3", "", "アクティブ高risk", "PL", "アクティブ", "True", "10", "", "10", "0", "0.8", "critical", "False", "", "0", "", "", "", "", "", ""],
            ],
        );
        let table = build_deals(&d, "1", DealStatusFilter::All);
        let ids: Vec<&str> = table.rows.iter().map(|r| r.deal_id.as_str()).collect();
        assert_eq!(ids, vec!["3", "2", "1"], "アクティブが先、アクティブ内はchurn降順、非アクティブは経過日数降順");
        assert_eq!(table.rows[0].churn_proba_90d_pct, Some(80.0));
        assert_eq!(table.rows[2].churn_proba_90d_pct, None, "非アクティブはchurn確率を出さない(GAS仕様)");
    }

    #[test]
    fn 都道府県は47件パディングされ不明があれば末尾に追加() {
        let d = sheet(
            &["consultant_id", "consultant_name", "prefecture", "deal_count", "active_deal_count"],
            &[
                &["1", "藤巻", "東京都", "10", "5"],
                &["1", "藤巻", "不明", "2", "1"],
                &["2", "他人", "大阪府", "99", "99"],
            ],
        );
        let rows = build_prefecture(&d, "1");
        assert_eq!(rows.len(), 48, "47都道府県 + 不明");
        let tokyo = rows.iter().find(|r| r.prefecture == "東京都").unwrap();
        assert_eq!(tokyo.deal_count, 10.0);
        let osaka = rows.iter().find(|r| r.prefecture == "大阪府").unwrap();
        assert_eq!(osaka.deal_count, 0.0, "他コンサルの分は混ぜない");
        assert_eq!(rows.last().unwrap().prefecture, "不明");
    }

    #[test]
    fn 接触ログは状態フィルタで絞り込める() {
        let log = sheet(
            &["consultant_id", "consultant_name", "deal_id", "customer_label", "week", "call_count", "mtg_count"],
            &[
                &["1", "藤巻", "10", "A社", "2026-06-01", "2", "0"],
                &["1", "藤巻", "20", "B社", "2026-06-01", "1", "1"],
            ],
        );
        let deals = sheet(
            &DEALS_HEADER,
            &[
                &["1", "藤巻", "10", "", "A社", "PL", "アクティブ", "True", "1", "", "1", "0", "0", "", "False", "", "0", "", "", "", "", "", ""],
                &["1", "藤巻", "20", "", "B社", "PL", "解約済", "False", "1", "", "1", "0", "0", "", "False", "", "0", "", "", "", "", "", ""],
            ],
        );
        let active_only = build_contact_log(&log, &deals, "1", ContactStatusFilter::Active, 0);
        assert_eq!(active_only.deals.len(), 1);
        assert_eq!(active_only.deals[0].deal_id, "10");

        let all = build_contact_log(&log, &deals, "1", ContactStatusFilter::All, 0);
        assert_eq!(all.deals.len(), 2);
    }

    #[test]
    fn 接触ログの週数ウィンドウで直近n週のみに絞れる() {
        let log = sheet(
            &["consultant_id", "consultant_name", "deal_id", "customer_label", "week", "call_count", "mtg_count"],
            &[
                &["1", "藤巻", "10", "A社", "2026-01-05", "1", "0"],
                &["1", "藤巻", "10", "A社", "2026-06-01", "3", "0"],
            ],
        );
        let deals = sheet(&DEALS_HEADER, &[]);
        let recent = build_contact_log(&log, &deals, "1", ContactStatusFilter::All, 4);
        assert_eq!(recent.deals[0].weeks.len(), 1, "直近4週なら古い週は落ちる");
        assert_eq!(recent.deals[0].weeks[0].week, "2026-06-01");

        let full = build_contact_log(&log, &deals, "1", ContactStatusFilter::All, 0);
        assert_eq!(full.deals[0].weeks.len(), 2, "0=全期間");
    }
}
