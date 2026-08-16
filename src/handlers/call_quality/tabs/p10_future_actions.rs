//! 未来アクション（GAS 版 `page-p10`）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p10">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP10FutureActions` / `_drawP10FutureActions` / `_drawP10Table` /
//!          `renderKgiActionParts` / `_drawP9PhaseSection` / `_drawP9AlertSection`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getConsultingFutureActions` / `getConsultingPhaseKpi` / `getConsultingTaskAlerts`）
//!
//! 2026-08-12 に旧「コンサルKGI」タブが解体され、契約フェーズ予兆アラート([0])と
//! タスク漏れリスト([3])がこのタブへ移設された（`renderKgiActionParts` 参照）。
//! 画面には3パネルが縦に並ぶ:
//!   本体  次回アクション予定日ベースの ToDo（期限超過/今日/今週/来週/再来週以降/予定なし）
//!   [0]  契約フェーズ予兆アラート（満了の生存ラインに対する進捗）
//!   [3]  タスク漏れリスト（MTG後フォロー無し/接触ゼロ/NA期日切れ の3カテゴリ）
//!
//! ------------------------------------------------------------------
//! 移植した領域（GAS の DOM id → このファイルの出力）
//! ------------------------------------------------------------------
//!   p10-kpis / p10-category-chart / p10-actions-table / p10-status / p10-action-filter
//!     → `ActionsPanel`
//!   p9-phase-kpis / p9-phase-table → `PhasePanel`
//!   p9-alerts-legend / p9-alerts-kpis / p9-alerts-table / p9-alerts-category
//!     → `TaskAlertsPanel`
//!
//! ------------------------------------------------------------------
//! 実データで確認された事実（2026-08-13 是正、団員の指示どおり反映）
//! ------------------------------------------------------------------
//! 2026-06-08 断面 533件で実測: 次回アクション日の入力は **25.1%(134件)** のみ。
//! うち **132件(98.5%)が期限超過**で、未来の予定として機能していたのは **2件**。
//! 「入力率」だけでは「入っている分は使えている」と誤読されるため、`ActionsKpis` は
//! 入力率(`fill_rate_pct`)と「予定日のうち未来の予定」(`future_among_dated_pct`)の
//! **両方**を返す（片方だけでは不十分、という団員指摘への対応）。
//!
//! ------------------------------------------------------------------
//! 抽出元（どこから値を取っているか）
//! ------------------------------------------------------------------
//! 「コンサル未来アクション」シートの `next_action_date` / `action_detail` は、
//! Python `consulting_activity_daily.py` の `_next_action_value()` /
//! `_action_detail_value()` が HubSpot Deal の複数プロパティを優先順位で辿って
//! **既に1つの値へ解決した後の列**（生の `date_of_next_action` 等はこのシートに列として
//! 存在しない）。したがって行ごとに「どの生プロパティ由来か」はこのシートからは
//! 再現できない。`ActionsPanel::date_priority_note` / `content_priority_note` に
//! Python 側で使われている優先順位をそのまま明記して透明性を担保する。
//!
//! 行ごとの `*_source` フィールドは、**このシートに実在する列**（`next_action_date` /
//! `date_of_next_action` / `due_date` / `action_date` 等、GAS 版 `_p10Field` の候補列と
//! 同じ並び）のうちどれが採用されたかを返す。現状は候補の先頭列（`next_action_date` /
//! `action_detail`）以外が存在しないため、実質的に常にそれが source になる。
//! 将来シートに生プロパティ列が追加されたときのための後方互換のフォールバックでもある
//! （GAS 版 `_p10Field` と同じ役割）。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! なし。GAS 版 P10 + 旧KGI[0][3] の可視領域はすべて上記3パネルに移植済み。

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use chrono::{Datelike, NaiveDate};
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_ACTIONS: &str = "コンサル未来アクション";
const SHEET_PHASE: &str = "コンサルフェーズKPI";
const SHEET_ALERTS: &str = "コンサルタスク漏れ";

/// 要対応ランキングの表示件数（GAS `actionable.slice(0, 50)`）
const PHASE_ACTIONABLE_LIMIT: usize = 50;
/// bucket 内の表示件数上限（GAS `list.slice(0, 300)`）
const BUCKET_ROW_LIMIT: usize = 300;

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

fn deal_label(deal_id: &str, deal_label: &str, customer_label: &str) -> String {
    for v in [deal_label, customer_label] {
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

/// GAS `_p10Field` の移植: 候補列を順に見て最初の非空値を返す。
/// 戻り値は (値, 採用した列名)。どれも空なら ("", "")。
fn pick_field(data: &SheetData, row: &[Arc<str>], candidates: &[&str]) -> (String, String) {
    for c in candidates {
        let v = data.get(row, c).trim();
        if !v.is_empty() {
            return (v.to_string(), (*c).to_string());
        }
    }
    (String::new(), String::new())
}

/// "YYYY-MM-DD"（先頭10文字。時刻付きも許容）を日付にする。GAS `new Date(s)` の簡易版。
fn parse_date(s: &str) -> Option<NaiveDate> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }
    let d = if s.len() >= 10 { &s[..10] } else { s };
    NaiveDate::parse_from_str(d, "%Y-%m-%d").ok()
}

/// 月を加算する。JS の `Date.setMonth` は存在しない日をロールオーバーする
/// （例: 1/31 + 1ヶ月 → 3/3）が、ここでは月末に丸める（例: 1/31 + 1ヶ月 → 2/28）。
/// 契約満了日の計算でこの差が出るのは「31日始まり×奇数月契約」のような稀なケースのみ。
fn add_months_clamped(d: NaiveDate, months: i64) -> NaiveDate {
    let total = d.year() as i64 * 12 + (d.month() as i64 - 1) + months;
    let year = total.div_euclid(12) as i32;
    let month = (total.rem_euclid(12) + 1) as u32;
    let mut day = d.day();
    loop {
        if let Some(nd) = NaiveDate::from_ymd_opt(year, month, day) {
            return nd;
        }
        if day <= 1 {
            return NaiveDate::from_ymd_opt(year, month, 1).unwrap();
        }
        day -= 1;
    }
}

// ================================================================== 本体（次回アクション）

/// bucket の並び順・色（画面の色は持たない。フロントの責務）。
const BUCKET_ORDER: [&str; 6] = ["期限超過", "今日", "今週", "来週", "再来週以降", "予定なし"];

/// GAS `_p10NormalizeBucket` の移植（フォールバック部分）。
/// シート自身の `bucket` 列が空のときだけ、候補列から予定日を拾って計算する。
fn compute_bucket_fallback(due: Option<NaiveDate>, today: NaiveDate) -> &'static str {
    let Some(d) = due else {
        return "予定なし";
    };
    let diff = (d - today).num_days();
    if diff < 0 {
        return "期限超過";
    }
    if diff == 0 {
        return "今日";
    }
    let dow = today.weekday().num_days_from_sunday() as i64;
    let to_sunday = if dow == 0 { 0 } else { 7 - dow };
    if diff <= to_sunday {
        "今週"
    } else if diff <= to_sunday + 7 {
        "来週"
    } else {
        "再来週以降"
    }
}

#[derive(Debug, Serialize, Clone)]
pub struct ActionRow {
    pub deal_id: String,
    pub customer_label: String,
    pub owner_id: String,
    pub owner_label: String,
    pub stage: String,
    pub bucket: String,
    /// 予定日の表示値（空文字なら未入力）
    pub next_action_date: String,
    /// 採用した列名（"next_action_date" 等）。空文字なら値なし
    pub next_action_date_source: String,
    pub days_to_action: Option<f64>,
    pub latest_contact_date: String,
    pub days_since_contact: Option<f64>,
    /// 200字で切ったアクション内容
    pub action_detail: String,
    pub action_detail_source: String,
}

#[derive(Debug, Serialize, Default)]
pub struct BucketCounts {
    pub overdue: usize,
    pub today: usize,
    pub this_week: usize,
    pub next_week: usize,
    pub later: usize,
    pub no_plan: usize,
}

impl BucketCounts {
    fn add(&mut self, bucket: &str) {
        match bucket {
            "期限超過" => self.overdue += 1,
            "今日" => self.today += 1,
            "今週" => self.this_week += 1,
            "来週" => self.next_week += 1,
            "再来週以降" => self.later += 1,
            _ => self.no_plan += 1,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct BucketGroup {
    pub key: String,
    pub rows: Vec<ActionRow>,
    pub total: usize,
    /// `BUCKET_ROW_LIMIT` で切ったか（約束3: 黙って上位N件にしない）
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct ActionsKpis {
    pub counts: BucketCounts,
    pub total: usize,
    /// 次回アクション日が入っている件数（bucket≠予定なし）
    pub with_date: usize,
    /// 入力率(%)。全件が「予定なし」なら None
    pub fill_rate_pct: Option<f64>,
    /// 予定日が入っている行のうち、期限超過でない割合(%)。
    /// 2026-06-08断面の実測(25.1%入力・うち98.5%が期限超過・未来2件)を裏付ける指標。
    /// with_date が 0 なら None
    pub future_among_dated_pct: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct ActionsPanel {
    pub kpis: ActionsKpis,
    /// 空バケットは除外。`BUCKET_ORDER` の順
    pub groups: Vec<BucketGroup>,
    pub owner_filter: Option<String>,
    /// (owner_id, owner_label) セレクタ用、重複無し・名前順
    pub owners: Vec<(String, String)>,
    pub date_priority_note: &'static str,
    pub content_priority_note: &'static str,
}

const DATE_PRIORITY_NOTE: &str =
    "予定日は Python 側 (_next_action_value) が HubSpot Deal の \
     date_of_next_action → next_action_date → hs_task_due_date の順に、\
     最初に値がある列を採用して「コンサル未来アクション」シートの next_action_date 列に \
     書き込み済み。このシートには生プロパティ列(date_of_next_action 等)は存在しない。";

const CONTENT_PRIORITY_NOTE: &str =
    "アクション内容は Python 側 (_action_detail_value) が HubSpot Deal の \
     rikulogi_2 → custom_latest_mtg_next_steps → actionnaiyou → nextaction → ai_next_actions \
     の順に、最初に値がある列を採用して action_detail 列に書き込み済み(200字で切る)。\
     ai_next_actions が採用された場合のみ Zoom AI Companion 等の自動生成由来。";

pub fn build_actions(data: &SheetData, q: &P10Query, today: NaiveDate) -> ActionsPanel {
    let mut rows: Vec<ActionRow> = Vec::with_capacity(data.rows.len());
    for row in &data.rows {
        let deal_id = data.get(row, "deal_id").to_string();
        let owner_id = {
            let (v, _) = pick_field(data, row, &["consultant_id", "owner_id"]);
            v
        };
        let owner_label = {
            let (v, _) = pick_field(data, row, &["consultant_name", "owner_name", "deal_owner_name"]);
            if v.is_empty() { owner_id.clone() } else { v }
        };

        let (next_action_date, date_src) =
            pick_field(data, row, &["next_action_date", "date_of_next_action", "due_date", "action_date"]);
        let (days_to_action_s, _) =
            pick_field(data, row, &["days_to_action", "days_until_next_action", "days_until"]);
        let (action_detail_raw, content_src) = pick_field(
            data,
            row,
            &[
                "action_detail",
                "rikulogi_2",
                "custom_latest_mtg_next_steps",
                "actionnaiyou",
                "nextaction",
                "ai_next_actions",
            ],
        );
        let action_detail: String = action_detail_raw.chars().take(200).collect();

        let sheet_bucket = data.get(row, "bucket").trim();
        let bucket = if !sheet_bucket.is_empty() {
            sheet_bucket.to_string()
        } else {
            let (due_s, _) = pick_field(
                data,
                row,
                &["due_date", "action_date", "next_action_date", "date_of_next_action", "scheduled_date", "target_date"],
            );
            compute_bucket_fallback(parse_date(&due_s), today).to_string()
        };

        rows.push(ActionRow {
            deal_id: deal_id.clone(),
            customer_label: deal_label(&deal_id, data.get(row, "deal_label"), data.get(row, "customer_label")),
            owner_id,
            owner_label,
            stage: pipeline_stage(data.get(row, "pipeline_label"), data.get(row, "stage_label")),
            bucket,
            next_action_date,
            next_action_date_source: date_src,
            days_to_action: opt_num(&days_to_action_s),
            latest_contact_date: data.get(row, "latest_contact_date").to_string(),
            days_since_contact: opt_num(data.get(row, "days_since_contact")),
            action_detail,
            action_detail_source: content_src,
        });
    }

    // 担当セレクタ（重複無し・名前順。GAS `_fillP10OwnerFilter` と同じ）
    let mut owners: Vec<(String, String)> = Vec::new();
    {
        let mut seen = std::collections::HashSet::new();
        for r in &rows {
            if r.owner_id.is_empty() || seen.contains(&r.owner_id) {
                continue;
            }
            seen.insert(r.owner_id.clone());
            owners.push((r.owner_id.clone(), r.owner_label.clone()));
        }
        owners.sort_by(|a, b| a.1.cmp(&b.1));
    }

    let owner_filter = q
        .owner
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string);

    let filtered: Vec<ActionRow> = match owner_filter.as_deref() {
        Some(f) => rows
            .into_iter()
            .filter(|r| r.owner_id == f || r.owner_label == f)
            .collect(),
        None => rows,
    };

    let mut counts = BucketCounts::default();
    for r in &filtered {
        counts.add(&r.bucket);
    }
    let total = filtered.len();
    let with_date = total - counts.no_plan;
    let fill_rate_pct = rate(with_date as f64, total as f64);
    let future_among_dated_pct = rate((with_date - counts.overdue) as f64, with_date as f64);

    // bucket でグルーピング（GAS `_drawP10Table`）
    let mut groups: Vec<BucketGroup> = Vec::new();
    for key in BUCKET_ORDER {
        let mut list: Vec<ActionRow> = filtered.iter().filter(|r| r.bucket == key).cloned().collect();
        if list.is_empty() {
            continue;
        }
        if key == "予定なし" {
            // 放置日数(最終接触からの経過)降順。GAS: 大きい(長く放置)ものを上に
            list.sort_by(|a, b| {
                let na = a.days_since_contact.unwrap_or(-1.0);
                let nb = b.days_since_contact.unwrap_or(-1.0);
                nb.partial_cmp(&na).unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.deal_id.cmp(&b.deal_id))
            });
        } else {
            // 残日数昇順(より急ぐものが上)。GAS: 欠損は 999999 扱い
            list.sort_by(|a, b| {
                let na = a.days_to_action.unwrap_or(999999.0);
                let nb = b.days_to_action.unwrap_or(999999.0);
                na.partial_cmp(&nb).unwrap_or(std::cmp::Ordering::Equal)
                    .then_with(|| a.deal_id.cmp(&b.deal_id))
            });
        }
        let total_in_bucket = list.len();
        let truncated = total_in_bucket > BUCKET_ROW_LIMIT;
        list.truncate(BUCKET_ROW_LIMIT);
        groups.push(BucketGroup {
            key: key.to_string(),
            rows: list,
            total: total_in_bucket,
            truncated,
        });
    }

    ActionsPanel {
        kpis: ActionsKpis {
            counts,
            total,
            with_date,
            fill_rate_pct,
            future_among_dated_pct,
        },
        groups,
        owner_filter,
        owners,
        date_priority_note: DATE_PRIORITY_NOTE,
        content_priority_note: CONTENT_PRIORITY_NOTE,
    }
}

// ================================================================== [0] 契約フェーズ予兆アラート

#[derive(Debug, Serialize, Clone)]
pub struct PhaseRow {
    pub deal_id: String,
    pub customer_label: String,
    pub consultant_name: String,
    pub contract_type: String,
    pub contract_period: String,
    pub phase_bucket: String,
    pub phase_pct: f64,
    /// 契約開始日+契約月数から算出した満了までの残日数。負値=満了済み。算出不可なら None
    pub days_to_expiry: Option<i64>,
    pub latest_nps: Option<f64>,
    pub nps_base_line: Option<f64>,
    pub contact_last30: Option<f64>,
    pub contact_expected: Option<f64>,
    pub overall_flag: String,
    pub alert_msg: String,
}

#[derive(Debug, Serialize, Default)]
pub struct PhaseFlagCounts {
    pub urgent: usize,   // 🚨緊急
    pub warning: usize,  // ⚠️警告
    pub caution: usize,  // 🟡注意
    pub healthy: usize,  // 🟢健全
    pub unknown: usize,  // ⚪判定不可
}

#[derive(Debug, Serialize)]
pub struct PhasePanel {
    pub flag_counts: PhaseFlagCounts,
    pub target_count: usize,
    /// 🚨緊急→⚠️警告→🟡注意 のみ、フェーズ進捗(終盤に近い順)でソート。上限50件
    pub actionable: Vec<PhaseRow>,
    pub actionable_total: usize,
    pub truncated: bool,
}

fn flag_rank(flag: &str) -> u8 {
    match flag {
        "🚨緊急" => 0,
        "⚠️警告" => 1,
        "🟡注意" => 2,
        "🟢健全" => 3,
        _ => 4,
    }
}

pub fn build_phase(data: &SheetData, today: NaiveDate) -> PhasePanel {
    let rows: Vec<PhaseRow> = data
        .rows
        .iter()
        .map(|row| {
            let deal_id = data.get(row, "deal_id").to_string();
            let days_to_expiry = {
                let start = data.get(row, "contract_start_date");
                let months = data.get(row, "contract_period");
                parse_date(start).and_then(|s| {
                    let m: i64 = months.trim().parse().ok()?;
                    if m == 0 {
                        return None;
                    }
                    Some((add_months_clamped(s, m) - today).num_days())
                })
            };
            PhaseRow {
                customer_label: deal_label(&deal_id, data.get(row, "deal_label"), data.get(row, "customer_label")),
                deal_id,
                consultant_name: data.get(row, "consultant_name").to_string(),
                contract_type: data.get(row, "contract_type").to_string(),
                contract_period: data.get(row, "contract_period").to_string(),
                phase_bucket: data.get(row, "phase_bucket").to_string(),
                phase_pct: num(data.get(row, "phase_pct")),
                days_to_expiry,
                latest_nps: opt_num(data.get(row, "latest_nps")),
                nps_base_line: opt_num(data.get(row, "nps_base_line")),
                contact_last30: opt_num(data.get(row, "contact_last30")),
                contact_expected: opt_num(data.get(row, "contact_expected")),
                overall_flag: data.get(row, "overall_flag").to_string(),
                alert_msg: data.get(row, "alert_msg").to_string(),
            }
        })
        .collect();

    let mut flag_counts = PhaseFlagCounts::default();
    for r in &rows {
        match r.overall_flag.as_str() {
            "🚨緊急" => flag_counts.urgent += 1,
            "⚠️警告" => flag_counts.warning += 1,
            "🟡注意" => flag_counts.caution += 1,
            "🟢健全" => flag_counts.healthy += 1,
            _ => flag_counts.unknown += 1,
        }
    }

    let mut actionable: Vec<PhaseRow> = rows
        .iter()
        .filter(|r| matches!(r.overall_flag.as_str(), "🚨緊急" | "⚠️警告" | "🟡注意"))
        .cloned()
        .collect();
    actionable.sort_by(|a, b| {
        flag_rank(&a.overall_flag)
            .cmp(&flag_rank(&b.overall_flag))
            .then_with(|| b.phase_pct.partial_cmp(&a.phase_pct).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let actionable_total = actionable.len();
    let truncated = actionable_total > PHASE_ACTIONABLE_LIMIT;
    actionable.truncate(PHASE_ACTIONABLE_LIMIT);

    PhasePanel {
        flag_counts,
        target_count: rows.len(),
        actionable,
        actionable_total,
        truncated,
    }
}

// ================================================================== [3] タスク漏れリスト

#[derive(Debug, Serialize, Clone)]
pub struct AlertRow {
    pub deal_id: String,
    pub customer_label: String,
    pub owner_label: String,
    pub pipeline_stage: String,
    pub category: String,
    pub category_label: String,
    pub days_since: Option<f64>,
    pub last_contact_date: String,
    pub days_since_mtg: Option<f64>,
    pub last_mtg_date: String,
}

#[derive(Debug, Serialize, Default)]
pub struct AlertCategoryCounts {
    pub mtg_no_followup: usize,
    pub contact_zero_2week: usize,
    pub na_overdue_no_action: usize,
}

#[derive(Debug, Serialize)]
pub struct TaskAlertsPanel {
    /// カテゴリ件数は絞り込み前の全体で数える(GAS `counts` は絞込前 rows を見る)
    pub category_counts: AlertCategoryCounts,
    pub total: usize,
    pub selected_category: Option<String>,
    /// フィルタ後・経過日数(days_since)降順。GAS 版は2026-08-13以降、上限なしで全件描画
    pub rows: Vec<AlertRow>,
}

pub fn build_alerts(data: &SheetData, q: &P10Query) -> TaskAlertsPanel {
    let rows: Vec<AlertRow> = data
        .rows
        .iter()
        .map(|row| {
            let deal_id = data.get(row, "deal_id").to_string();
            AlertRow {
                customer_label: deal_label(&deal_id, data.get(row, "deal_label"), data.get(row, "customer_label")),
                deal_id,
                owner_label: {
                    let (v, _) = pick_field(data, row, &["owner_name", "deal_owner_name", "owner_id"]);
                    v
                },
                pipeline_stage: pipeline_stage(data.get(row, "pipeline_label"), data.get(row, "stage_label")),
                category: data.get(row, "category").to_string(),
                category_label: data.get(row, "category_label").to_string(),
                days_since: opt_num(data.get(row, "days_since")),
                last_contact_date: data.get(row, "last_contact_date").to_string(),
                days_since_mtg: opt_num(data.get(row, "days_since_mtg")),
                last_mtg_date: data.get(row, "last_mtg_date").to_string(),
            }
        })
        .collect();

    let mut category_counts = AlertCategoryCounts::default();
    for r in &rows {
        match r.category.as_str() {
            "mtg_no_followup" => category_counts.mtg_no_followup += 1,
            "contact_zero_2week" => category_counts.contact_zero_2week += 1,
            "na_overdue_no_action" => category_counts.na_overdue_no_action += 1,
            _ => {}
        }
    }

    let selected_category = q
        .alert_category
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty() && *s != "__all__")
        .map(str::to_string);

    let mut shown: Vec<AlertRow> = match selected_category.as_deref() {
        Some(c) => rows.iter().filter(|r| r.category == c).cloned().collect(),
        None => rows.clone(),
    };
    shown.sort_by(|a, b| {
        b.days_since
            .unwrap_or(0.0)
            .partial_cmp(&a.days_since.unwrap_or(0.0))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });

    TaskAlertsPanel {
        category_counts,
        total: rows.len(),
        selected_category,
        rows: shown,
    }
}

// ================================================================== ハンドラ

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct P10Query {
    /// consultant_id/owner_id または表示名との一致でフィルタ（GAS `p10-action-filter`）
    pub owner: Option<String>,
    /// "__all__"(既定) / "mtg_no_followup" / "contact_zero_2week" / "na_overdue_no_action"
    pub alert_category: Option<String>,
}

// 担当者の絞込は `owner`（**`owners` ではない**。p0/p1/p3/ptf は複数形）。
// 単複の取り違えは 200 が返って全担当者の数字が出るので、一覧に無い方が
// `ignored_params` に載る。
crate::accepted_params!(P10Query, p10_query_accepted => "owner", "alert_category");

#[derive(Debug, Serialize)]
pub struct P10Data {
    pub actions: ActionsPanel,
    pub phase: PhasePanel,
    pub alerts: TaskAlertsPanel,
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

pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: P10Query,
) -> Result<TabPayload<P10Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();
    // 2026-08-17 是正: `chrono::Local::now()` だと本番(UTC)で
    //   **毎朝 00:00〜09:00 JST の9時間、日付が1日ずれる**。
    //   `today` は「期限切れ/今日/今週/来週」の振り分けを決めるので、
    //   バケットが丸ごと1つずれる。未来アクションを見るのはまさにその時間帯。
    let today = super::jst_today();

    let actions_data = load(client, store, SHEET_ACTIONS, &mut sources).await?;
    let phase_data = load(client, store, SHEET_PHASE, &mut sources).await?;
    let alerts_data = load(client, store, SHEET_ALERTS, &mut sources).await?;

    let actions = build_actions(&actions_data, &q, today);
    let phase = build_phase(&phase_data, today);
    let alerts = build_alerts(&alerts_data, &q);

    set_matched(&mut sources, SHEET_ACTIONS, actions.kpis.total);
    set_matched(&mut sources, SHEET_ALERTS, alerts.rows.len());

    Ok(TabPayload {
        data: P10Data { actions, phase, alerts },
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

    fn arc_row(vals: &[&str]) -> Vec<Arc<str>> {
        vals.iter().map(|v| Arc::from(*v)).collect()
    }

    fn actions_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        let header = vec![
            "deal_id", "deal_label", "customer_label", "consultant_id", "consultant_name",
            "owner_id", "owner_name", "pipeline_label", "stage_label",
            "latest_contact_date", "days_since_contact", "next_action_date",
            "days_until_next_action", "bucket", "days_to_action", "risk_level",
            "recommended_action", "action_detail",
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

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 8, 16).unwrap()
    }

    #[test]
    fn 名前が取れないときdeal_idをそのまま名前にしない() {
        assert_eq!(deal_label("999", "", ""), "(名称未取得 / Deal 999)");
    }

    #[test]
    fn 予定日入力率と未来の予定は別指標で両方返す() {
        // 実測(2026-06-08断面)を模した縮小版: 4件中1件だけ予定日あり、それも期限超過
        let d = actions_sheet(vec![
            vec!["1", "A", "A", "c1", "田中", "1", "田中", "PL", "St", "", "", "2025-01-01", "-500", "期限超過", "-500", "", "", ""],
            vec!["2", "B", "B", "c1", "田中", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", ""],
            vec!["3", "C", "C", "c1", "田中", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", ""],
            vec!["4", "D", "D", "c1", "田中", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", ""],
        ]);
        let q = P10Query::default();
        let panel = build_actions(&d, &q, today());
        assert_eq!(panel.kpis.with_date, 1);
        assert_eq!(panel.kpis.fill_rate_pct, Some(25.0));
        assert_eq!(panel.kpis.future_among_dated_pct, Some(0.0), "唯一の予定日も期限超過なので未来の予定は0%");
    }

    #[test]
    fn 予定日が無いときfuture_among_datedはnone() {
        let d = actions_sheet(vec![vec![
            "1", "A", "A", "c1", "田中", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", "",
        ]]);
        let panel = build_actions(&d, &P10Query::default(), today());
        assert_eq!(panel.kpis.future_among_dated_pct, None);
    }

    #[test]
    fn bucketは自身の列を優先しフォールバック計算しない() {
        let d = actions_sheet(vec![vec![
            "1", "A", "A", "c1", "田中", "1", "田中", "PL", "St", "", "", "2099-01-01", "999", "今日", "0", "", "", "",
        ]]);
        let panel = build_actions(&d, &P10Query::default(), today());
        assert_eq!(panel.groups[0].key, "今日", "シート自身のbucket列(今日)を優先する");
    }

    #[test]
    fn bucket列が空ならフォールバック計算する() {
        let d = actions_sheet(vec![vec![
            "1", "A", "A", "", "", "1", "田中", "PL", "St", "", "", "2026-08-16", "", "", "", "", "", "",
        ]]);
        let panel = build_actions(&d, &P10Query::default(), today());
        assert_eq!(panel.groups[0].key, "今日", "予定日が今日ならフォールバックで「今日」になる");
    }

    #[test]
    fn action_detailは200字で切る() {
        let long = "あ".repeat(250);
        let d = actions_sheet(vec![vec![
            "1", "A", "A", "", "", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", &long,
        ]]);
        let panel = build_actions(&d, &P10Query::default(), today());
        let row = panel.groups[0].rows.iter().find(|r| r.deal_id == "1").unwrap();
        assert_eq!(row.action_detail.chars().count(), 200);
    }

    #[test]
    fn 担当フィルタはidと表示名どちらでも一致する() {
        let d = actions_sheet(vec![
            vec!["1", "A", "A", "c1", "田中", "1", "田中", "PL", "St", "", "", "", "", "予定なし", "", "", "", ""],
            vec!["2", "B", "B", "c2", "鈴木", "2", "鈴木", "PL", "St", "", "", "", "", "予定なし", "", "", "", ""],
        ]);
        let q = P10Query { owner: Some("田中".into()), alert_category: None };
        let panel = build_actions(&d, &q, today());
        assert_eq!(panel.kpis.total, 1);
    }

    #[test]
    fn フェーズフラグの件数を数える() {
        let header = vec![
            "deal_id", "deal_label", "customer_label", "consultant_name", "contract_type",
            "contract_period", "contract_start_date", "phase_bucket", "phase_pct",
            "elapsed_months", "latest_nps", "nps_base_line", "nps_trend", "nps_flag",
            "contact_last30", "contact_expected", "days_since_contact", "contact_flag",
            "seika_status", "overall_flag", "alert_msg",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let d = SheetData {
            header,
            rows: vec![
                arc_row(&["1", "A", "A", "田中", "新規", "6", "2026-01-01", "終盤", "0.9", "", "3", "6", "", "", "1", "2", "5", "", "", "🚨緊急", "満了間近"]),
                arc_row(&["2", "B", "B", "田中", "新規", "6", "2026-06-01", "序盤", "0.1", "", "8", "6", "", "", "3", "2", "1", "", "", "🟢健全", ""]),
            ],
            fetched_at: Instant::now(),
        };
        let panel = build_phase(&d, today());
        assert_eq!(panel.flag_counts.urgent, 1);
        assert_eq!(panel.flag_counts.healthy, 1);
        assert_eq!(panel.actionable.len(), 1, "actionableは緊急/警告/注意のみ");
    }

    #[test]
    fn 満了までの残日数を契約開始日と期間から算出する() {
        let start = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap();
        let expiry = add_months_clamped(start, 6);
        assert_eq!(expiry, NaiveDate::from_ymd_opt(2026, 7, 1).unwrap());
    }

    #[test]
    fn アラートのカテゴリ件数は絞り込み前で数える() {
        let header = vec![
            "deal_id", "deal_label", "customer_id", "customer_label", "owner_id", "owner_name",
            "pipeline_label", "stage_label", "category", "category_label", "last_contact_date",
            "days_since", "last_mtg_date", "days_since_mtg",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let d = SheetData {
            header,
            rows: vec![
                arc_row(&["1", "A", "", "A", "1", "田中", "PL", "St", "mtg_no_followup", "MTG後フォロー無し", "2026-08-01", "10", "", ""]),
                arc_row(&["2", "B", "", "B", "1", "田中", "PL", "St", "contact_zero_2week", "接触ゼロ警告", "2026-08-10", "3", "", ""]),
            ],
            fetched_at: Instant::now(),
        };
        let q = P10Query { owner: None, alert_category: Some("mtg_no_followup".into()) };
        let panel = build_alerts(&d, &q);
        assert_eq!(panel.category_counts.mtg_no_followup, 1);
        assert_eq!(panel.category_counts.contact_zero_2week, 1, "絞り込み後でも全体件数は変わらない");
        assert_eq!(panel.rows.len(), 1, "表示行はフィルタ後の1件");
    }

    #[test]
    fn アラートは経過日数降順で並ぶ() {
        let header = vec![
            "deal_id", "deal_label", "customer_id", "customer_label", "owner_id", "owner_name",
            "pipeline_label", "stage_label", "category", "category_label", "last_contact_date",
            "days_since", "last_mtg_date", "days_since_mtg",
        ]
        .into_iter()
        .map(String::from)
        .collect();
        let d = SheetData {
            header,
            rows: vec![
                arc_row(&["1", "A", "", "A", "1", "田中", "PL", "St", "mtg_no_followup", "x", "2026-08-01", "3", "", ""]),
                arc_row(&["2", "B", "", "B", "1", "田中", "PL", "St", "mtg_no_followup", "x", "2026-07-01", "30", "", ""]),
            ],
            fetched_at: Instant::now(),
        };
        let panel = build_alerts(&d, &P10Query::default());
        assert_eq!(panel.rows[0].deal_id, "2", "経過日数の大きい方が先");
    }
}
