//! 架電クオリティ: ターゲット分析（GAS 版 page-ptf）
//!
//! 2026-08-16 移植。GAS 版 index.html `<div class="page" id="page-ptf">` の構成は
//! 大きく2系統に分かれる。
//!
//! ## A. HubSpot をライブ照会する系（本ファイルでは**未実装**）
//!
//! ①②③ の主役パネルは、シートではなく **HubSpot Search API を毎回叩いて**数える。
//! 元になるシートが存在しないため、シート層（`SheetStore`）だけでは移植できない。
//! HubSpot クライアントが Rust 側に載ってから着手すること。
//!
//! - **① 条件ビルダー + 該当 vs それ以外 ファネル対比**
//!   （GAS `getTargetFunnel` / `_tfCounts_`）
//!   取引プロパティの AND 条件（最大4件。HubSpot の filterGroup は6フィルタ上限で、
//!   成約スコープが固定2フィルタを使うため残り4）で母数→架電→アポ/商談→成約を数え、
//!   該当と「それ以外」を対比する。**未実装の理由: HubSpot ライブ照会**。
//! - **② 週次時系列乖離（直近8週）**（GAS `getTargetTimeseries`）
//!   接触=`notes_last_contacted` / 商談=`scheduled_business_meeting_date` /
//!   成約=計上Deal `createdate` / BPOアポ=`bpo_appo_date` が各週に入る件数のシェア推移。
//!   **未実装の理由: HubSpot ライブ照会**。
//! - **③ 経営方針ターゲット（保存・実績アラート）**
//!   （GAS `saveTargetPolicy` / `getTargetPolicies` / `evaluateTargetPolicies`）
//!   ①の条件＋目標シェア% を全利用者で共有保存し、実績シェアと対比する。
//!   **未実装の理由: HubSpot ライブ照会 + 保存先（GAS は PropertiesService）が
//!   Rust 側に無い**。保存先を決めずに移すと「運用で変わる値」の置き場が増える。
//! - **条件ビルダーのプロパティ一覧・候補値**
//!   （GAS `getFilterableProperties` / `getPropertyValues`）**未実装の理由: 同上**。
//! - **業種別アクティブ観測（動的版）**（GAS `getSegmentObservation` ほか）
//!   業界グループ・規模バンドをユーザーが組み替えられる版。Turso への SQL と、
//!   グループ定義／規模バンドの**保存**（`saveSegmentGroups` / `saveSizeBins`）が要る。
//!   **未実装の理由: Turso 経路 + 設定の保存先が未定**。
//!   （静的シート「業種別アクティブ観測」だけを読んで“動的版”を名乗らせない）
//! - **コンタクト属性別アポ率**（GAS `drawSegmentChart('contact_attr')`）
//!   クロス集計シートに属性次元が無く、GAS でも絞込中は全社集計のまま出していた。
//!   **未実装の理由: 「セグメント_クロス」に該当次元が無い**。
//!
//! ## B. シートを読む系（本ファイルで実装済み）
//!
//! | パネル | シート |
//! |---|---|
//! | 追いかけ停止候補 | 「追いかけ停止候補」 |
//! | 現場ベース成約率 | 「成約率_現場ベース」 |
//! | BPO貢献追跡 | 「BPO貢献追跡」 |
//! | 通話時間バケット | 「通話時間バケット」 |
//! | ステージ反復分析 | 「ステージ反復分析」 |
//! | コーラー行動パターン | 「コーラー行動パターン」 |
//! | セグメント別（都道府県/業界/規模） | 「セグメント_クロス」 |
//!
//! GAS 版は details を開いたときに初めて読む遅延ロードなので、ここでも
//! **1リクエスト = 1パネル**にして、開かれていない details のためにシートを
//! 取りに行かないようにしてある（`panel` クエリで指定）。
//!
//! ## GAS 版から直した点
//!
//! - **セグメント別アポ率の軸ラベルが「※Zoom発信が分母」になっていたが、
//!   計算は `apo_count / call_count`（HubSpot Call）だった。**
//!   「セグメント_クロス」に Zoom発信の列が無いので Zoom で割れるはずがない。
//!   ラベルを実態（HubSpot Call）に合わせた。
//! - **セグメント別に営業スコープが掛かっていなかった。**
//!   `tabs/mod.rs` の約束5（営業スコープの既定は role=sales）に合わせ、
//!   owner_id 列を持つ「セグメント_クロス」では role=sales に絞る。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};
use crate::handlers::call_quality::query_audit::ValueAudit;

/// 追いかけ停止候補の表示上限。GAS 版と同じ 500 件。
const CHASE_LIMIT: usize = 500;
/// ステージ反復「撤退候補案件」の表示上限。GAS 版と同じ 300 件。
const REVISIT_DEAL_LIMIT: usize = 300;
/// ステージ反復「PL×ステージ反復統計」の表示上限。GAS 版と同じ 20 件。
const REVISIT_STAGE_LIMIT: usize = 20;
/// セグメント別アポ率の最低架電数。GAS 版 `MIN_CALLS_FOR_SEGMENT = 100`。
const MIN_CALLS_FOR_SEGMENT: f64 = 100.0;
/// セグメント別アポ率の表示件数。GAS 版と同じ上位25。
const SEGMENT_TOP_N: usize = 25;

// ------------------------------------------------------------------ クエリ

#[derive(Debug, Default, Deserialize, Serialize)]
pub struct TargetQuery {
    /// 表示するパネル。未指定は `chase_stop`。
    /// `chase_stop` / `field_close_rate` / `bpo_contribution` /
    /// `call_duration` / `stage_revisit` / `caller_behavior` / `segment`
    pub panel: Option<String>,

    /// カンマ区切り owner_id。未指定なら role=sales のみ（約束5）。
    pub owners: Option<String>,

    // --- 追いかけ停止候補 ---
    /// 失注商談の最低回数。GAS 既定 3。
    pub min_failed_shoudan: Option<f64>,
    /// 会社が別案件で成約済みの案件を除く。GAS 既定 true。
    pub exclude_houjin_won: Option<bool>,

    // --- 現場ベース成約率 / BPO貢献追跡 ---
    /// 集計期間。`6m` / `12m` / `all`。GAS 既定は成約率=`all`、BPO貢献=`all`。
    pub range: Option<String>,

    // --- ステージ反復分析 ---
    /// 不通系ステージ訪問の最低回数。GAS 既定 3。
    pub min_negative_visits: Option<f64>,

    // --- コーラー行動パターン ---
    /// `sales` / `bpo` / `consultant`。GAS 既定 `sales`。
    pub role: Option<String>,
    /// 並び替えに使う列名。GAS 既定は「総架電」。
    pub sort_by: Option<String>,

    // --- セグメント別 クロス絞込 ---
    pub industry: Option<String>,
    pub size_band: Option<String>,
    pub prefecture: Option<String>,
}

// **このタブだけ `industry` / `size_band`**（p2 の商談遷移は `trans_industry` /
// `trans_size`）。名前が似ていて画面をまたぐと取り違える。どちらのタブでも
// 相手側の名前は `ignored_params` に出る。
crate::accepted_params!(TargetQuery, target_query_accepted =>
    "panel", "owners", "min_failed_shoudan", "exclude_houjin_won", "range",
    "min_negative_visits", "role", "sort_by", "industry", "size_band", "prefecture");

// ------------------------------------------------------------------ 返却型

/// パネルごとに形が違うので、`panel` タグ付きで返す。
#[derive(Debug, Serialize)]
#[serde(tag = "panel", rename_all = "snake_case")]
pub enum TargetData {
    ChaseStop(ChaseStopData),
    FieldCloseRate(FieldCloseRateData),
    BpoContribution(BpoContributionData),
    CallDuration(CallDurationData),
    StageRevisit(StageRevisitData),
    CallerBehavior(CallerBehaviorData),
    Segment(SegmentData),
    /// 未実装 or 未知のパネル名。**黙って空を返さない。**
    Unavailable(UnavailablePanel),
}

#[derive(Debug, Serialize)]
pub struct UnavailablePanel {
    pub requested: String,
    pub reason: String,
    /// このタブで返せるパネル名
    pub available: Vec<String>,
    /// 移植できていないパネルと、その理由
    pub not_implemented: Vec<NotImplemented>,
}

#[derive(Debug, Serialize)]
pub struct NotImplemented {
    pub name: String,
    pub reason: String,
}

// --- 追いかけ停止候補 ---

#[derive(Debug, Serialize)]
pub struct ChaseStopData {
    pub rows: Vec<ChaseStopRow>,
    /// 絞り込み後の件数（`rows` は上限で切られていることがある）
    pub matched: usize,
    pub truncated: bool,
    pub limit: usize,
    pub min_failed_shoudan: f64,
    pub exclude_houjin_won: bool,
    pub definition: String,
}

#[derive(Debug, Serialize)]
pub struct ChaseStopRow {
    pub deal_id: String,
    pub deal_name: String,
    pub houjin: String,
    pub failed_shoudan: f64,
    pub last_lost_date: String,
    pub current_pipeline: String,
    /// 会社が別案件で成約済み（要個別判断）
    pub houjin_won: bool,
    pub owner_id: String,
}

// --- 現場ベース成約率 ---

#[derive(Debug, Serialize)]
pub struct FieldCloseRateData {
    pub range: String,
    pub range_label: String,
    pub available_ranges: Vec<String>,
    pub overall: Option<FieldRateRow>,
    pub by_size: Vec<FieldRateRow>,
    pub by_plan: Vec<FieldRateRow>,
    pub by_period: Vec<FieldRateRow>,
    pub by_plan_period: Vec<FieldRateRow>,
    pub monthly: Vec<FieldRateRow>,
    /// 分母が何かを必ず出す。ファネル側（案件単位）と数字が違う理由がここにある。
    pub denominator_label: String,
    pub caveat: String,
}

#[derive(Debug, Serialize)]
pub struct FieldRateRow {
    pub segment: String,
    pub shoudan_count: f64,
    pub won_count: f64,
    /// 成約 ÷ 商談回数。分母0なら null（約束2）
    pub close_rate: Option<f64>,
    pub monthly_price: String,
    pub total_estimate: String,
    /// 「不明」= サブスク新規成約で企業規模が紐付かない山。率の比較に使えない。
    pub size_unknown: bool,
}

// --- BPO貢献追跡 ---

#[derive(Debug, Serialize)]
pub struct BpoContributionData {
    pub range: String,
    pub range_label: String,
    pub available_ranges: Vec<String>,
    /// 源泉別 商談→成約（営業全体 / BPO起点 / 営業独自 など）
    pub by_origin: Vec<BpoOriginRow>,
    /// 源泉別 月次推移
    pub monthly: Vec<BpoMonthlyRow>,
    /// 観測完了率50%未満の行があるか（成約率を意思決定に使えない）
    pub has_immature_observation: bool,
    pub caveat: String,
}

#[derive(Debug, Serialize)]
pub struct BpoOriginRow {
    pub segment: String,
    pub shoudan_deals: f64,
    pub won: f64,
    pub bpo_lost: f64,
    pub in_progress: f64,
    pub observed: f64,
    /// 観測完了率(%)。分母0なら null
    pub observation_rate: Option<f64>,
    /// 成約率（観測済ベース, %）。分母0なら null
    pub close_rate_observed: Option<f64>,
    /// BPO貢献率(%)。シート側で算出済みの値をそのまま渡す（分母定義がシート側にある）
    pub bpo_contribution_rate: Option<f64>,
    /// 観測完了率50%未満。信頼区間が広く意思決定に使えない。
    pub immature: bool,
}

#[derive(Debug, Serialize)]
pub struct BpoMonthlyRow {
    pub year_month: String,
    pub segment: String,
    pub shoudan_deals: f64,
    pub won: f64,
    pub close_rate: Option<f64>,
}

// --- 通話時間バケット ---

#[derive(Debug, Serialize)]
pub struct CallDurationData {
    pub rows: Vec<CallDurationRow>,
    /// バケットの並び（短い順）。画面はこの順に列を出す。
    pub bucket_labels: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct CallDurationRow {
    pub axis: String,
    pub segment: String,
    pub year_month: String,
    pub owner_id: String,
    pub name: String,
    pub total: f64,
    pub buckets: Vec<CallDurationBucket>,
}

#[derive(Debug, Serialize)]
pub struct CallDurationBucket {
    pub label: String,
    pub count: f64,
    /// 同月内総架電に対する率(%)。分母0なら null（約束2）
    pub share: Option<f64>,
}

// --- ステージ反復分析 ---

#[derive(Debug, Serialize)]
pub struct StageRevisitData {
    /// PL×ステージ別 訪問・反復状況（関連案件数の多い順）
    pub stages: Vec<RevisitStageRow>,
    pub stages_truncated: bool,
    pub stages_total: usize,
    /// 不通×多回架電 撤退候補
    pub deals: Vec<RevisitDealRow>,
    pub deals_truncated: bool,
    pub deals_matched: usize,
    pub min_negative_visits: f64,
    pub definition: String,
}

#[derive(Debug, Serialize)]
pub struct RevisitStageRow {
    pub pipeline: String,
    pub stage_name: String,
    pub related_deals: f64,
    pub repeated_deals: f64,
    pub max_visits: f64,
}

#[derive(Debug, Serialize)]
pub struct RevisitDealRow {
    pub deal_id: String,
    pub deal_name: String,
    pub owner_id: String,
    pub pipeline: String,
    pub last_stage: String,
    pub negative_visits: f64,
    pub stage_transitions: f64,
    pub call_count: f64,
}

// --- コーラー行動パターン ---

#[derive(Debug, Serialize)]
pub struct CallerBehaviorData {
    pub role: String,
    pub sort_by: String,
    pub rows: Vec<CallerBehaviorRow>,
    /// 行動タイプの人数（集中型 / 標準型 / 分散型）
    pub type_counts: Vec<(String, usize)>,
    pub note: String,
}

#[derive(Debug, Serialize)]
pub struct CallerBehaviorRow {
    pub owner_id: String,
    pub name: String,
    pub role: String,
    pub total_calls: f64,
    pub unique_deals: f64,
    pub calls_per_deal: f64,
    pub hhi_x100: f64,
    pub top10_share: f64,
    pub long_call_share: f64,
    pub behavior_type: String,
}

// --- セグメント別 ---

#[derive(Debug, Serialize)]
pub struct SegmentData {
    pub by_prefecture: Vec<SegmentRow>,
    pub by_industry: Vec<SegmentRow>,
    pub by_size: Vec<SegmentRow>,
    /// クロス絞込が効いているか（業界 / 規模 / 都道府県 のいずれか指定）
    pub cross_active: bool,
    pub cross_label: String,
    pub min_calls: f64,
    pub top_n: usize,
    /// **GAS 版のラベルは「※Zoom発信が分母」だったが実装は HubSpot Call だった。**
    /// このシートに Zoom発信の列は無い。実態に合わせたラベルを返す。
    pub denominator_label: String,
    /// 誰を集計したか（約束5）
    pub scope_label: String,
}

#[derive(Debug, Serialize)]
pub struct SegmentRow {
    pub value: String,
    pub call_count: f64,
    pub apo_count: f64,
    /// 分母0なら null（ここは足切り 100 があるので実際には出ないが規約に合わせる）
    pub apo_rate: Option<f64>,
    /// 規模軸の「不明」（サブスク新規で企業規模が紐付かない）。他と並べて順位比較しない。
    pub is_unknown_bucket: bool,
}

// ------------------------------------------------------------------ 小道具

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 空欄を 0 に丸めずに返す。シートの空欄は「値なし」であって 0 ではない。
fn num_opt(s: &str) -> Option<f64> {
    let t = s.trim().replace(',', "");
    if t.is_empty() {
        return None;
    }
    t.parse::<f64>().ok()
}

/// 集計対象の owner_id を決める。
///
/// p3_timeseries.rs にも同じ関数がある。`tabs/mod.rs` は 1ファイル1担当の運用なので、
/// 共通化して片方の都合でもう片方が壊れる状態を作らず、意図的に複製している。
/// 共通化するなら `tabs/mod.rs` へ上げること（担当者が別なので今は触らない）。
///
/// **`Some(空ベクタ)` は「誰も該当しない」。`None`（絞らない）に丸めない。**
fn resolve_scope(owners: Option<&str>, sales_owners: Option<&Vec<String>>) -> Option<Vec<String>> {
    if let Some(s) = owners {
        let v: Vec<String> = s
            .split(',')
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .collect();
        if !v.is_empty() {
            return Some(v);
        }
    }
    sales_owners.cloned()
}

fn scope_label(owners: Option<&str>, scope: Option<&Vec<String>>) -> String {
    let explicit = owners
        .map(|s| s.split(',').any(|t| !t.trim().is_empty()))
        .unwrap_or(false);
    match scope {
        Some(v) if explicit => format!("選択メンバー {}名", v.len()),
        Some(v) => format!("営業(role=sales) {}名", v.len()),
        None => "絞り込みなし（役割一覧が渡されていない）".to_string(),
    }
}

fn in_scope(scope: Option<&Vec<String>>, owner: &str) -> bool {
    match scope {
        Some(ids) => ids.iter().any(|i| i == owner),
        None => true,
    }
}

/// セグメント値が「不明」を意味するか。GAS `isUnknownSegmentValue` と同じ判定。
///
/// 業界マスタの欠損が「一」という1文字で入っており、これを弾かないと
/// 業界別アポ率の最上位が「一」になる（GAS 版で実際に起きた）。
fn is_unknown_segment_value(v: &str) -> bool {
    let s = v.trim();
    if s.is_empty() || s.starts_with('_') || s == "一" {
        return true;
    }
    let lower = s.to_ascii_lowercase();
    matches!(
        lower.as_str(),
        "unknown" | "null" | "none" | "na" | "n/a" | "-"
    ) || matches!(s, "不明" | "(空欄)" | "（空欄）" | "空欄")
}

/// 規模軸の「不明」バケットか。GAS `isSizeUnknown`（`00_不明` / `不明`）。
/// **こちらは除外せず、別扱いで残す**（サブスク新規の山がどれだけあるかが情報）。
fn is_size_unknown(v: &str) -> bool {
    let s = v.trim();
    s == "00_不明" || s == "00 不明" || s == "不明"
}

fn not_implemented_list() -> Vec<NotImplemented> {
    vec![
        NotImplemented {
            name: "① ターゲット条件ビルダー + 該当 vs それ以外 ファネル対比".into(),
            reason: "HubSpot Search API のライブ照会（GAS getTargetFunnel）。元シートが無い".into(),
        },
        NotImplemented {
            name: "② 時系列乖離（週次・直近8週）".into(),
            reason: "HubSpot Search API のライブ照会（GAS getTargetTimeseries）".into(),
        },
        NotImplemented {
            name: "③ 経営方針ターゲット（保存・実績アラート）".into(),
            reason: "HubSpot ライブ照会 + 保存先（GAS は PropertiesService）が Rust 側に無い".into(),
        },
        NotImplemented {
            name: "条件ビルダーのプロパティ一覧・候補値".into(),
            reason: "HubSpot ライブ照会（GAS getFilterableProperties / getPropertyValues）".into(),
        },
        NotImplemented {
            name: "業種別アクティブ観測（動的版・業界/規模バンド組替）".into(),
            reason: "Turso への SQL + グループ定義／規模バンドの保存先が未定".into(),
        },
        NotImplemented {
            name: "コンタクト属性別アポ率".into(),
            reason: "「セグメント_クロス」に属性次元の列が無い".into(),
        },
    ]
}

fn available_panels() -> Vec<String> {
    vec![
        "chase_stop".into(),
        "field_close_rate".into(),
        "bpo_contribution".into(),
        "call_duration".into(),
        "stage_revisit".into(),
        "caller_behavior".into(),
        "segment".into(),
    ]
}

// -------------------------------------- 追いかけ停止候補（「追いかけ停止候補」）

/// 使う列: deal_id / dealname / houjin / failed_shoudan / last_lost_date /
///         current_pipeline / houjin_won / owner_id
///
/// 「商談到達 → 失注（商談済リードへ転落）」の繰り返し回数がシートに入っている。
/// 成約すれば計上PLへ抜けて循環が止まるので、積み上がり＝決まっていない証拠。
pub fn collect_chase_stop(
    data: &SheetData,
    min_failed: f64,
    exclude_won: bool,
) -> (Vec<ChaseStopRow>, usize) {
    let mut rows: Vec<ChaseStopRow> = Vec::new();

    for row in &data.rows {
        let failed = num(data.get(row, "failed_shoudan"));
        if failed < min_failed {
            continue;
        }
        let won = data.get(row, "houjin_won").trim() == "1";
        if exclude_won && won {
            continue;
        }
        rows.push(ChaseStopRow {
            deal_id: data.get(row, "deal_id").to_string(),
            deal_name: data.get(row, "dealname").to_string(),
            houjin: data.get(row, "houjin").to_string(),
            failed_shoudan: failed,
            last_lost_date: data.get(row, "last_lost_date").to_string(),
            current_pipeline: data.get(row, "current_pipeline").to_string(),
            houjin_won: won,
            owner_id: data.get(row, "owner_id").to_string(),
        });
    }

    // 失注回数の多い順。同数は deal_id で安定化（約束4）
    rows.sort_by(|a, b| {
        b.failed_shoudan
            .partial_cmp(&a.failed_shoudan)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });

    let matched = rows.len();
    rows.truncate(CHASE_LIMIT);
    (rows, matched)
}

// ------------------------------------ 現場ベース成約率（「成約率_現場ベース」）

/// 使う列: range / range_label / 軸 / セグメント / 商談回数 / 成約 / 月単価 / 総額目安
///
/// シートの `成約率` 列（丸め済み）は使わず `成約 ÷ 商談回数` で引き直す。
/// 率は 1pt 未満でも「乖離」として扱う運用なので、丸めた値を経由させない。
/// また分母0のときシートは空欄だが、ここでは `None` を明示する（約束2）。
pub fn collect_field_close_rate(data: &SheetData, range: &str) -> (FieldCloseRateData, usize) {
    let mut available: Vec<String> = Vec::new();
    let mut range_label = String::new();
    let mut overall: Option<FieldRateRow> = None;
    let (mut by_size, mut by_plan, mut by_period, mut by_plan_period, mut monthly) =
        (Vec::new(), Vec::new(), Vec::new(), Vec::new(), Vec::new());
    let mut matched = 0usize;

    // range 列が無い旧スキーマでは全行を対象にする（GAS のフォールバックと同じ）
    let has_range = data.col("range").is_some();

    for row in &data.rows {
        let r = data.get(row, "range").trim().to_string();
        if has_range && !r.is_empty() && !available.contains(&r) {
            available.push(r.clone());
        }
        if has_range && r != range {
            continue;
        }
        matched += 1;
        if range_label.is_empty() {
            range_label = data.get(row, "range_label").to_string();
        }

        let segment = data.get(row, "セグメント").to_string();
        let shoudan = num(data.get(row, "商談回数"));
        let won = num(data.get(row, "成約"));
        let item = FieldRateRow {
            size_unknown: segment.contains("不明"),
            segment,
            shoudan_count: shoudan,
            won_count: won,
            close_rate: rate(won, shoudan),
            monthly_price: data.get(row, "月単価").to_string(),
            total_estimate: data.get(row, "総額目安").to_string(),
        };

        match data.get(row, "軸").trim() {
            "全体" => overall = Some(item),
            "規模" => by_size.push(item),
            "成約プラン構成" => by_plan.push(item),
            "成約契約期間構成" => by_period.push(item),
            "成約プラン×期間" => by_plan_period.push(item),
            "月別" => monthly.push(item),
            // 未知の軸は捨てずに「月別」等へ混ぜない。落ちたことが分かるよう何もしない。
            _ => {}
        }
    }

    available.sort();
    // 月別は年月昇順で安定させる（約束4）
    monthly.sort_by(|a, b| a.segment.cmp(&b.segment));

    (
        FieldCloseRateData {
            range: range.to_string(),
            range_label,
            available_ranges: available,
            overall,
            by_size,
            by_plan,
            by_period,
            by_plan_period,
            monthly,
            denominator_label: "商談回数（再商談も数えるイベント単位）".to_string(),
            caveat:
                "規模別の成約率は分解できない。成約（計上新規）の 98.6% がサブスク新規で企業規模が紐付かず\
                 「不明」に落ちるため、名前付き規模の成約率は過小に出る。妥当なのは商談回数の分布のみ。"
                    .to_string(),
        },
        matched,
    )
}

// ----------------------------------------- BPO貢献追跡（「BPO貢献追跡」）

/// 使う列: range / range_label / 軸 / セグメント / 商談Deal数 / 成約 / BPO失注 /
///         進行中 / 商談_観測済 / 観測完了率 / 成約率_観測済 / BPO貢献率 / 年月
///
/// 「観測完了率」は 180日未経過の商談を除いた割合。50%未満の行は
/// 成約率の信頼区間が広く、意思決定に使えない（商談→成約ラグ中央値149日, P75=428日）。
pub fn collect_bpo_contribution(data: &SheetData, range: &str) -> (BpoContributionData, usize) {
    let mut available: Vec<String> = Vec::new();
    let mut range_label = String::new();
    let mut by_origin: Vec<BpoOriginRow> = Vec::new();
    let mut monthly: Vec<BpoMonthlyRow> = Vec::new();
    let mut matched = 0usize;
    let has_range = data.col("range").is_some();

    for row in &data.rows {
        let r = data.get(row, "range").trim().to_string();
        if has_range && !r.is_empty() && !available.contains(&r) {
            available.push(r.clone());
        }
        if has_range && r != range {
            continue;
        }
        matched += 1;
        if range_label.is_empty() {
            range_label = data.get(row, "range_label").to_string();
        }

        let segment = data.get(row, "セグメント").to_string();
        let shoudan = num(data.get(row, "商談Deal数"));
        let won = num(data.get(row, "成約"));

        match data.get(row, "軸").trim() {
            "源泉別 商談→成約" => {
                let observed = num(data.get(row, "商談_観測済"));
                let obs_rate = rate(observed, shoudan);
                by_origin.push(BpoOriginRow {
                    segment,
                    shoudan_deals: shoudan,
                    won,
                    bpo_lost: num(data.get(row, "BPO失注")),
                    in_progress: num(data.get(row, "進行中")),
                    observed,
                    observation_rate: obs_rate,
                    close_rate_observed: rate(won, observed),
                    // 貢献率は「営業全体の成約に占める BPO 起点の割合」で、
                    // 分母がこの行に無い。シート側の算出値をそのまま渡す。
                    bpo_contribution_rate: num_opt(data.get(row, "BPO貢献率")),
                    immature: obs_rate.map(|v| v < 50.0).unwrap_or(true),
                });
            }
            "源泉別 月次推移" => {
                monthly.push(BpoMonthlyRow {
                    year_month: data.get(row, "年月").to_string(),
                    segment,
                    shoudan_deals: shoudan,
                    won,
                    close_rate: rate(won, shoudan),
                });
            }
            _ => {}
        }
    }

    available.sort();
    // 年月 → セグメント で安定させる（約束4）
    monthly.sort_by(|a, b| {
        a.year_month
            .cmp(&b.year_month)
            .then_with(|| a.segment.cmp(&b.segment))
    });
    by_origin.sort_by(|a, b| a.segment.cmp(&b.segment));

    let has_immature = by_origin.iter().any(|r| r.immature);

    (
        BpoContributionData {
            range: range.to_string(),
            range_label,
            available_ranges: available,
            by_origin,
            monthly,
            has_immature_observation: has_immature,
            caveat:
                "BPO起点判定は商談Deal の owner 変更履歴に role=bpo が含まれるか（入力フラグ非依存）。\
                 観測完了率が50%未満の行は成約率を意思決定に使わないこと。\
                 直近月は商談→成約のラグ（2〜3ヶ月）で成約率が低く出る。"
                    .to_string(),
        },
        matched,
    )
}

// --------------------------------------- 通話時間バケット（「通話時間バケット」）

/// バケット列名（シートの見出しそのまま。短い順）。
/// 「90秒以上 (有意会話)」だけ半角スペースが入っているので、**位置ではなく
/// この文字列で引く**こと（約束1）。
const DURATION_BUCKETS: [&str; 5] = [
    "0-30秒(不通/即切り)",
    "30-90秒(短接触)",
    "90秒以上 (有意会話)",
    "300秒以上(深い会話)",
    "600秒以上(商談相当)",
];

/// 使う列: 軸 / セグメント / 年月 / owner_id / name / 合計 / 各バケット
///
/// シートには `<バケット>_率` 列もあるが、率は `count / 合計` で引き直す
/// （分母0を 0% にしないため。約束2）。
pub fn collect_call_duration(data: &SheetData) -> (CallDurationData, usize) {
    let mut rows: Vec<CallDurationRow> = Vec::new();

    for row in &data.rows {
        let total = num(data.get(row, "合計"));
        let buckets: Vec<CallDurationBucket> = DURATION_BUCKETS
            .iter()
            .map(|label| {
                let c = num(data.get(row, label));
                CallDurationBucket {
                    label: (*label).to_string(),
                    count: c,
                    share: rate(c, total),
                }
            })
            .collect();
        rows.push(CallDurationRow {
            axis: data.get(row, "軸").to_string(),
            segment: data.get(row, "セグメント").to_string(),
            year_month: data.get(row, "年月").to_string(),
            owner_id: data.get(row, "owner_id").to_string(),
            name: data.get(row, "name").to_string(),
            total,
            buckets,
        });
    }

    // 軸 → 年月 → セグメント で安定させる（約束4）
    rows.sort_by(|a, b| {
        a.axis
            .cmp(&b.axis)
            .then_with(|| a.year_month.cmp(&b.year_month))
            .then_with(|| a.segment.cmp(&b.segment))
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });

    let matched = rows.len();
    (
        CallDurationData {
            rows,
            bucket_labels: DURATION_BUCKETS.iter().map(|s| s.to_string()).collect(),
        },
        matched,
    )
}

// ------------------------------------- ステージ反復分析（「ステージ反復分析」）

/// 使う列（セクションで意味が変わる2部構成のシート）:
///   セクション='PL×ステージ反復統計' → pipeline / ステージ名 / 関連Deal数 /
///                                      反復(2回以上)Deal数 / 最大訪問回数
///   セクション='撤退候補案件'          → deal_id / dealname / owner_id / pipeline /
///                                      最終stage / 不通系訪問回数 / 総ステージ遷移 / 架電回数
pub fn collect_stage_revisit(data: &SheetData, min_negative: f64) -> (StageRevisitData, usize) {
    let mut stages: Vec<RevisitStageRow> = Vec::new();
    let mut deals: Vec<RevisitDealRow> = Vec::new();
    let mut matched = 0usize;

    for row in &data.rows {
        match data.get(row, "セクション").trim() {
            "PL×ステージ反復統計" => {
                stages.push(RevisitStageRow {
                    pipeline: data.get(row, "pipeline").to_string(),
                    stage_name: data.get(row, "ステージ名").to_string(),
                    related_deals: num(data.get(row, "関連Deal数")),
                    repeated_deals: num(data.get(row, "反復(2回以上)Deal数")),
                    max_visits: num(data.get(row, "最大訪問回数")),
                });
                matched += 1;
            }
            "撤退候補案件" => {
                let neg = num(data.get(row, "不通系訪問回数"));
                if neg < min_negative {
                    continue;
                }
                deals.push(RevisitDealRow {
                    deal_id: data.get(row, "deal_id").to_string(),
                    deal_name: data.get(row, "dealname").to_string(),
                    owner_id: data.get(row, "owner_id").to_string(),
                    pipeline: data.get(row, "pipeline").to_string(),
                    last_stage: data.get(row, "最終stage").to_string(),
                    negative_visits: neg,
                    stage_transitions: num(data.get(row, "総ステージ遷移")),
                    call_count: num(data.get(row, "架電回数")),
                });
                matched += 1;
            }
            _ => {}
        }
    }

    // 訪問数の多い順。同数は名前で安定化（約束4）
    stages.sort_by(|a, b| {
        b.related_deals
            .partial_cmp(&a.related_deals)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.pipeline.cmp(&b.pipeline))
            .then_with(|| a.stage_name.cmp(&b.stage_name))
    });
    let stages_total = stages.len();
    let stages_truncated = stages_total > REVISIT_STAGE_LIMIT;
    stages.truncate(REVISIT_STAGE_LIMIT);

    deals.sort_by(|a, b| {
        b.negative_visits
            .partial_cmp(&a.negative_visits)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.deal_id.cmp(&b.deal_id))
    });
    let deals_matched = deals.len();
    let deals_truncated = deals_matched > REVISIT_DEAL_LIMIT;
    deals.truncate(REVISIT_DEAL_LIMIT);

    (
        StageRevisitData {
            stages,
            stages_truncated,
            stages_total,
            deals,
            deals_truncated,
            deals_matched,
            min_negative_visits: min_negative,
            definition:
                "不通系ステージに規定回数以上戻り、かつ架電5回以上の案件を撤退候補とする。\
                 同じステージを何度も訪問＝現場の空回りの兆候。"
                    .to_string(),
        },
        matched,
    )
}

// ------------------------------- コーラー行動パターン（「コーラー行動パターン」）

/// 使う列: owner_id / name / role / 総架電 / ユニーク先(Deal数) / 1Deal平均架電 /
///         HHI*100(集中度) / Top10集中率% / 90秒以上率% / 行動タイプ
///
/// **`role` はこのシート自身が持っている**ので、営業スコープは owner_id ではなく
/// この列で絞る（GAS 版と同じ。既定は sales）。
/// `sort_by` が受け付ける列名。**`key` の match と同じ並び**にすること。
/// 片方だけ増やすと、増やした列が「解釈できない値」として報告されてしまう。
pub const CALLER_SORT_KEYS: [&str; 6] = [
    "総架電",
    "ユニーク先(Deal数)",
    "1Deal平均架電",
    "HHI*100(集中度)",
    "Top10集中率%",
    "90秒以上率%",
];

pub fn collect_caller_behavior(
    data: &SheetData,
    role: &str,
    sort_by: &str,
) -> (CallerBehaviorData, usize) {
    let mut rows: Vec<CallerBehaviorRow> = Vec::new();

    for row in &data.rows {
        if data.get(row, "role").trim() != role {
            continue;
        }
        rows.push(CallerBehaviorRow {
            owner_id: data.get(row, "owner_id").to_string(),
            name: data.get(row, "name").to_string(),
            role: role.to_string(),
            total_calls: num(data.get(row, "総架電")),
            unique_deals: num(data.get(row, "ユニーク先(Deal数)")),
            calls_per_deal: num(data.get(row, "1Deal平均架電")),
            hhi_x100: num(data.get(row, "HHI*100(集中度)")),
            top10_share: num(data.get(row, "Top10集中率%")),
            long_call_share: num(data.get(row, "90秒以上率%")),
            behavior_type: data.get(row, "行動タイプ").to_string(),
        });
    }

    // 指定列の降順。未知の列名なら総架電で並べる（黙って HashMap 順にしない）
    let key = |r: &CallerBehaviorRow| -> f64 {
        match sort_by {
            "ユニーク先(Deal数)" => r.unique_deals,
            "1Deal平均架電" => r.calls_per_deal,
            "HHI*100(集中度)" => r.hhi_x100,
            "Top10集中率%" => r.top10_share,
            "90秒以上率%" => r.long_call_share,
            _ => r.total_calls,
        }
    };
    rows.sort_by(|a, b| {
        key(b)
            .partial_cmp(&key(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });

    // 行動タイプの人数。並びを固定する（HashMap の反復順を返さない）
    let mut type_counts: Vec<(String, usize)> = ["集中型", "標準型", "分散型"]
        .iter()
        .map(|t| {
            (
                (*t).to_string(),
                rows.iter().filter(|r| r.behavior_type == *t).count(),
            )
        })
        .collect();
    // 上の3種以外の値が入っていたら黙って捨てず末尾に足す
    let known = ["集中型", "標準型", "分散型"];
    let mut others: Vec<String> = rows
        .iter()
        .map(|r| r.behavior_type.clone())
        .filter(|t| !known.contains(&t.as_str()) && !t.is_empty())
        .collect();
    others.sort();
    others.dedup();
    for o in others {
        let n = rows.iter().filter(|r| r.behavior_type == o).count();
        type_counts.push((o, n));
    }

    let matched = rows.len();
    (
        CallerBehaviorData {
            role: role.to_string(),
            sort_by: sort_by.to_string(),
            rows,
            type_counts,
            note: "全期間累計。総架電50件未満は分析対象外（シート生成側で除外済み）。\
                   HHI×100 = 各Dealの架電シェアの2乗和×100。Top10% = 上位10Dealへの架電が全体に占める割合。"
                .to_string(),
        },
        matched,
    )
}

// ----------------------------------------- セグメント別（「セグメント_クロス」）

/// 使う列: owner_id / prefecture / industry / size_band / call_count / apo_count
///
/// GAS 版はここに営業スコープを掛けていなかった。`tabs/mod.rs` 約束5 に合わせて
/// role=sales へ絞る（owner_id 列があるので絞れる）。
pub fn collect_segment(
    data: &SheetData,
    q: &TargetQuery,
    scope: Option<&Vec<String>>,
) -> (SegmentData, usize) {
    // 2026-08-16 修正: クロージャだと戻り値のライフタイムが引数に縛られてコンパイルが通らない
    //   （`let f = |v: &Option<String>| -> Option<&str>` は入力の借用と出力の借用を
    //     結び付けられない）。素の関数にして、借用元が q であることを明示する。
    // "__all__" は画面の「全て」を表す番兵。空文字と同じく「絞らない」扱いにする。
    fn selected(v: &Option<String>) -> Option<&str> {
        v.as_deref().filter(|s| !s.is_empty() && *s != "__all__")
    }
    let (fi, fs, fp) = (
        selected(&q.industry),
        selected(&q.size_band),
        selected(&q.prefecture),
    );
    let cross_active = fi.is_some() || fs.is_some() || fp.is_some();

    // 次元 → 値 → [call, apo]。0=都道府県 / 1=業界 / 2=規模
    let mut acc: [HashMap<String, [f64; 2]>; 3] =
        [HashMap::new(), HashMap::new(), HashMap::new()];
    let mut matched = 0usize;

    for row in &data.rows {
        if !in_scope(scope, data.get(row, "owner_id")) {
            continue;
        }
        let pref = data.get(row, "prefecture");
        let ind = data.get(row, "industry");
        let size = data.get(row, "size_band");
        if let Some(v) = fi {
            if ind != v {
                continue;
            }
        }
        if let Some(v) = fs {
            if size != v {
                continue;
            }
        }
        if let Some(v) = fp {
            if pref != v {
                continue;
            }
        }
        let call = num(data.get(row, "call_count"));
        let apo = num(data.get(row, "apo_count"));
        for (i, key) in [pref, ind, size].into_iter().enumerate() {
            let e = acc[i].entry(key.to_string()).or_insert([0.0; 2]);
            e[0] += call;
            e[1] += apo;
        }
        matched += 1;
    }

    // 足切り → アポ率降順 → 上位N。
    // 規模軸の「不明」だけは除外せず残す（サブスク新規の山の大きさ自体が情報のため）。
    fn finish(map: &HashMap<String, [f64; 2]>, keep_size_unknown: bool) -> Vec<SegmentRow> {
        let mut v: Vec<SegmentRow> = Vec::new();
        for (k, val) in map.iter() {
            let (call, apo) = (val[0], val[1]);
            // 少サンプルは率が不安定（例: 4架電1アポ=25%）なので順位に出さない
            if call < MIN_CALLS_FOR_SEGMENT {
                continue;
            }
            let size_unknown = is_size_unknown(k.as_str());
            if !(keep_size_unknown && size_unknown) && is_unknown_segment_value(k.as_str()) {
                continue;
            }
            v.push(SegmentRow {
                value: k.clone(),
                call_count: call,
                apo_count: apo,
                apo_rate: rate(apo, call),
                is_unknown_bucket: size_unknown,
            });
        }
        // 率降順。同率は値名で安定化（約束4）
        v.sort_by(|a, b| {
            b.apo_rate
                .unwrap_or(0.0)
                .partial_cmp(&a.apo_rate.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.value.cmp(&b.value))
        });
        v.truncate(SEGMENT_TOP_N);
        v
    }

    let mut parts: Vec<String> = Vec::new();
    if let Some(v) = fi {
        parts.push(format!("業界={v}"));
    }
    if let Some(v) = fs {
        parts.push(format!("規模={v}"));
    }
    if let Some(v) = fp {
        parts.push(format!("都道府県={v}"));
    }

    (
        SegmentData {
            by_prefecture: finish(&acc[0], false),
            by_industry: finish(&acc[1], false),
            by_size: finish(&acc[2], true),
            cross_active,
            cross_label: if cross_active {
                format!("クロス絞込: {}", parts.join(" / "))
            } else {
                "クロス絞込: 未適用（単独次元の集計）".to_string()
            },
            min_calls: MIN_CALLS_FOR_SEGMENT,
            top_n: SEGMENT_TOP_N,
            // GAS 版は「※Zoom発信が分母」と書いていたが、このシートに Zoom発信の列は無い
            denominator_label: "HubSpot Call（「セグメント_クロス」に Zoom発信の列が無い）"
                .to_string(),
            scope_label: scope_label(q.owners.as_deref(), scope),
        },
        matched,
    )
}

// ------------------------------------------------------------------ ハンドラ

/// `panel` で指定された1パネルだけを集計して返す。
///
/// GAS 版は details を開いた時点で初めてシートを取りに行く（遅延ロード）。
/// ここでも同じにして、開かれていないパネルのためにシートを読まない。
pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: TargetQuery,
    sales_owners: Option<Vec<String>>,
) -> Result<TabPayload<TargetData>> {
    let started = Instant::now();
    let panel = q.panel.clone().unwrap_or_else(|| "chase_stop".to_string());
    let mut sources: Vec<SourceInfo> = Vec::new();
    // 2026-08-17 追加。`panel` 自体は未知の値なら `TargetData::Unavailable` で
    // 名指しして返すので、ここで二重に報告しない（狼少年を作らない）。
    let mut audit = ValueAudit::new();

    // Arc<SheetData> をそのまま渡すと参照の型合わせで悩むので、必要な値だけ受ける。
    let mut push_source =
        |name: &str, total_rows: usize, age_secs: u64, cached: bool, matched: usize| {
            sources.push(SourceInfo {
                sheet: name.to_string(),
                total_rows,
                matched_rows: matched,
                from_cache: cached,
                age_secs,
            });
        };

    let data = match panel.as_str() {
        "chase_stop" => {
            let (sheet, cached) = store.get(client, "追いかけ停止候補").await?;
            let min_failed = q.min_failed_shoudan.unwrap_or(3.0);
            let exclude_won = q.exclude_houjin_won.unwrap_or(true);
            let (rows, matched) = collect_chase_stop(&sheet, min_failed, exclude_won);
            push_source(
                "追いかけ停止候補",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::ChaseStop(ChaseStopData {
                truncated: matched > CHASE_LIMIT,
                rows,
                matched,
                limit: CHASE_LIMIT,
                min_failed_shoudan: min_failed,
                exclude_houjin_won: exclude_won,
                definition:
                    "取引レコードの動きから「商談到達 → 失注（商談済リードへ転落）」の繰り返し回数を数える。\
                     成約すれば計上PLへ抜けて循環が止まるので、失注循環の積み上がり＝決まっていない証拠。\
                     owner / 法人 / コンサルに依存せず、レコード履歴のみで判定する。"
                        .to_string(),
            })
        }
        "field_close_rate" => {
            let (sheet, cached) = store.get(client, "成約率_現場ベース").await?;
            let range = q.range.clone().unwrap_or_else(|| "all".to_string());
            let (d, matched) = collect_field_close_rate(&sheet, &range);
            push_source(
                "成約率_現場ベース",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::FieldCloseRate(d)
        }
        "bpo_contribution" => {
            let (sheet, cached) = store.get(client, "BPO貢献追跡").await?;
            let range = q.range.clone().unwrap_or_else(|| "all".to_string());
            let (d, matched) = collect_bpo_contribution(&sheet, &range);
            push_source(
                "BPO貢献追跡",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::BpoContribution(d)
        }
        "call_duration" => {
            let (sheet, cached) = store.get(client, "通話時間バケット").await?;
            let (d, matched) = collect_call_duration(&sheet);
            push_source(
                "通話時間バケット",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::CallDuration(d)
        }
        "stage_revisit" => {
            let (sheet, cached) = store.get(client, "ステージ反復分析").await?;
            let min_neg = q.min_negative_visits.unwrap_or(3.0);
            let (d, matched) = collect_stage_revisit(&sheet, min_neg);
            push_source(
                "ステージ反復分析",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::StageRevisit(d)
        }
        "caller_behavior" => {
            let (sheet, cached) = store.get(client, "コーラー行動パターン").await?;
            let role = q.role.clone().unwrap_or_else(|| "sales".to_string());
            // 2026-08-17 是正: `collect_caller_behavior` は未知の列名を黙って
            //   「総架電」へ落とすのに、応答の `sort_by` には**送られた値がそのまま
            //   返っていた**。つまり応答が「HHIで並べた」と言いながら総架電で
            //   並んでいる状態を、画面から見分ける方法が無い。
            //   落とす先は変えず、落としたことを名指しする。
            let sort_by = audit.choice(
                "sort_by",
                q.sort_by.as_deref(),
                &CALLER_SORT_KEYS.join(" | "),
                |v| {
                    CALLER_SORT_KEYS
                        .iter()
                        .find(|k| **k == v.trim())
                        .map(|k| (*k).to_string())
                },
                || ("総架電".to_string(), "総架電".to_string()),
            );
            let (d, matched) = collect_caller_behavior(&sheet, &role, &sort_by);
            push_source(
                "コーラー行動パターン",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::CallerBehavior(d)
        }
        "segment" => {
            let (sheet, cached) = store.get(client, "セグメント_クロス").await?;
            let scope = resolve_scope(q.owners.as_deref(), sales_owners.as_ref());
            let (d, matched) = collect_segment(&sheet, &q, scope.as_ref());
            push_source(
                "セグメント_クロス",
                sheet.rows.len(),
                sheet.fetched_at.elapsed().as_secs(),
                cached,
                matched,
            );
            TargetData::Segment(d)
        }
        other => TargetData::Unavailable(UnavailablePanel {
            requested: other.to_string(),
            reason: "このパネルはシート層だけでは移植できていない、または名前が誤っている。"
                .to_string(),
            available: available_panels(),
            not_implemented: not_implemented_list(),
        }),
    };

    Ok(TabPayload {
        data,
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
    use std::sync::Arc;

    fn sheet(header: &[&str], rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .into_iter()
                .map(|r| {
                    r.into_iter()
                        .map(|s: &str| -> Arc<str> { Arc::from(s) })
                        .collect::<Vec<Arc<str>>>()
                })
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    // ----------------------------------------------------------- 分母0 / null

    #[test]
    fn 商談回数0の成約率はnull() {
        // 「商談0だから成約率0%」と読ませない（約束2）
        let d = sheet(
            &[
                "range",
                "range_label",
                "軸",
                "セグメント",
                "商談回数",
                "成約",
                "月単価",
                "総額目安",
            ],
            vec![
                vec!["all", "全期間", "全体", "全体", "0", "0", "", ""],
                vec!["all", "全期間", "規模", "01_1-30", "200", "15", "", ""],
            ],
        );
        let (r, _) = collect_field_close_rate(&d, "all");
        assert!(r.overall.unwrap().close_rate.is_none());
        assert_eq!(r.by_size[0].close_rate, Some(7.5));
    }

    #[test]
    fn 総架電0の通話時間バケット率はnull() {
        let d = sheet(
            &[
                "軸",
                "セグメント",
                "年月",
                "owner_id",
                "name",
                "合計",
                "0-30秒(不通/即切り)",
                "30-90秒(短接触)",
                "90秒以上 (有意会話)",
                "300秒以上(深い会話)",
                "600秒以上(商談相当)",
            ],
            vec![
                vec!["全体", "全体", "2026-05", "", "", "0", "0", "0", "0", "0", "0"],
                vec![
                    "全体", "全体", "2026-06", "", "", "200", "100", "50", "30", "15", "5",
                ],
            ],
        );
        let (r, matched) = collect_call_duration(&d);
        assert_eq!(matched, 2);
        assert!(r.rows[0].buckets[0].share.is_none(), "合計0を 0% にしない");
        assert_eq!(r.rows[1].buckets[0].share, Some(50.0));
        assert_eq!(r.bucket_labels.len(), 5);
    }

    #[test]
    fn 観測済0のbpo成約率はnull() {
        let d = sheet(
            &[
                "range",
                "range_label",
                "軸",
                "セグメント",
                "商談Deal数",
                "成約",
                "BPO失注",
                "進行中",
                "商談_観測済",
                "BPO貢献率",
                "年月",
            ],
            vec![vec![
                "all", "全期間", "源泉別 商談→成約", "BPO起点", "100", "0", "5", "40", "0", "", "",
            ]],
        );
        let (r, _) = collect_bpo_contribution(&d, "all");
        // 観測済=0 なので「観測済のうち成約」は分母0 → null（0% にしない）
        assert!(r.by_origin[0].close_rate_observed.is_none());
        // 2026-08-16 修正: 期待値が誤っていた（実装が正しい）。
        //   観測率 = 商談_観測済 ÷ 商談Deal数 で、このデータは商談Deal数=100。
        //   分母があるので null ではなく **0%** が正しい。
        //   「分母0は null」の規約は分母が0のときの話であって、
        //   分子が0のときは 0% と出すのが正しい（両者を混同しない）。
        assert_eq!(
            r.by_origin[0].observation_rate,
            Some(0.0),
            "分母100・分子0なので観測率は0%。null ではない"
        );
        assert!(r.by_origin[0].immature, "観測できていない行は必ず旗を立てる");
        assert!(r.has_immature_observation);
        assert!(r.by_origin[0].bpo_contribution_rate.is_none(), "空欄を0にしない");
    }

    // ----------------------------------------------------------- 営業スコープ

    #[test]
    fn セグメント別は既定で営業以外を除外する() {
        // GAS 版はここに絞りが無かった。約束5 に合わせる。
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![
                vec!["sales1", "東京都", "E 製造業", "02_31-50", "1000", "10"],
                vec!["bpo1", "東京都", "E 製造業", "02_31-50", "9000", "9"],
            ],
        );
        let sales = vec!["sales1".to_string()];
        let (r, matched) = collect_segment(&d, &TargetQuery::default(), Some(&sales));
        assert_eq!(matched, 1);
        assert_eq!(r.by_prefecture[0].call_count, 1000.0);
        assert_eq!(r.by_prefecture[0].apo_rate, Some(1.0), "混ぜると 0.19% に薄まる");
        assert!(r.scope_label.contains("営業"));
    }

    #[test]
    fn セグメント別のスコープが空集合なら誰も集計されない() {
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![vec!["bpo1", "東京都", "E 製造業", "02_31-50", "9000", "9"]],
        );
        let empty: Vec<String> = Vec::new();
        let (r, matched) = collect_segment(&d, &TargetQuery::default(), Some(&empty));
        assert_eq!(matched, 0);
        assert!(r.by_prefecture.is_empty());
    }

    #[test]
    fn コーラー行動パターンはroleで絞る() {
        let d = sheet(
            &[
                "owner_id",
                "name",
                "role",
                "総架電",
                "ユニーク先(Deal数)",
                "1Deal平均架電",
                "HHI*100(集中度)",
                "Top10集中率%",
                "90秒以上率%",
                "行動タイプ",
            ],
            vec![
                vec!["1", "営業A", "sales", "500", "100", "5", "3", "20", "30", "標準型"],
                vec!["2", "BPO B", "bpo", "9000", "300", "30", "9", "60", "10", "集中型"],
            ],
        );
        let (r, matched) = collect_caller_behavior(&d, "sales", "総架電");
        assert_eq!(matched, 1);
        assert_eq!(r.rows[0].name, "営業A");
        // 行動タイプの並びは固定
        assert_eq!(
            r.type_counts.iter().map(|(k, _)| k.as_str()).collect::<Vec<_>>(),
            vec!["集中型", "標準型", "分散型"]
        );
        assert_eq!(r.type_counts[1].1, 1);
    }

    // ------------------------------------------------------------- 並びの安定

    #[test]
    fn 追いかけ停止候補は失注回数降順で同数はdeal_idで安定する() {
        let d = sheet(
            &[
                "deal_id",
                "dealname",
                "houjin",
                "failed_shoudan",
                "last_lost_date",
                "current_pipeline",
                "houjin_won",
                "owner_id",
            ],
            vec![
                vec!["b", "B社", "", "3", "2026-05-01", "pl", "0", "1"],
                vec!["a", "A社", "", "3", "2026-05-01", "pl", "0", "1"],
                vec!["c", "C社", "", "5", "2026-05-01", "pl", "0", "1"],
            ],
        );
        let (rows, matched) = collect_chase_stop(&d, 3.0, true);
        assert_eq!(matched, 3);
        assert_eq!(
            rows.iter().map(|r| r.deal_id.as_str()).collect::<Vec<_>>(),
            vec!["c", "a", "b"]
        );
    }

    #[test]
    fn セグメント別は同率でも値名で安定する() {
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![
                vec!["1", "大阪府", "E 製造業", "02_31-50", "1000", "10"],
                vec!["1", "東京都", "E 製造業", "02_31-50", "1000", "10"],
            ],
        );
        let a = collect_segment(&d, &TargetQuery::default(), None).0;
        let b = collect_segment(&d, &TargetQuery::default(), None).0;
        let ids: Vec<&str> = a.by_prefecture.iter().map(|r| r.value.as_str()).collect();
        assert_eq!(ids, vec!["大阪府", "東京都"]);
        assert_eq!(
            ids,
            b.by_prefecture
                .iter()
                .map(|r| r.value.as_str())
                .collect::<Vec<_>>()
        );
    }

    // --------------------------------------------------------------- 切り捨て

    #[test]
    fn ステージ反復の撤退候補は上限で切りtruncatedを立てる() {
        let header = [
            "セクション",
            "deal_id",
            "dealname",
            "owner_id",
            "pipeline",
            "最終stage",
            "不通系訪問回数",
            "総ステージ遷移",
            "架電回数",
            "ステージ名",
            "関連Deal数",
            "反復(2回以上)Deal数",
            "最大訪問回数",
        ];
        let ids: Vec<String> = (0..REVISIT_DEAL_LIMIT + 5)
            .map(|i| format!("d{i:04}"))
            .collect();
        let rows: Vec<Vec<&str>> = ids
            .iter()
            .map(|id| {
                vec![
                    "撤退候補案件",
                    id.as_str(),
                    "案件",
                    "1",
                    "pl",
                    "不通",
                    "5",
                    "10",
                    "8",
                    "",
                    "",
                    "",
                    "",
                ]
            })
            .collect();
        let d = sheet(&header, rows);
        let (r, _) = collect_stage_revisit(&d, 3.0);
        assert_eq!(r.deals.len(), REVISIT_DEAL_LIMIT);
        assert!(r.deals_truncated, "黙って上位300件にしない");
        assert_eq!(r.deals_matched, REVISIT_DEAL_LIMIT + 5);
    }

    #[test]
    fn 追いかけ停止候補の足切りと除外が効く() {
        let d = sheet(
            &[
                "deal_id",
                "dealname",
                "houjin",
                "failed_shoudan",
                "last_lost_date",
                "current_pipeline",
                "houjin_won",
                "owner_id",
            ],
            vec![
                vec!["a", "A社", "", "2", "", "pl", "0", "1"], // 足切り未満
                vec!["b", "B社", "", "4", "", "pl", "1", "1"], // 会社が別案件で成約済
                vec!["c", "C社", "", "4", "", "pl", "0", "1"],
            ],
        );
        let (rows, matched) = collect_chase_stop(&d, 3.0, true);
        assert_eq!(matched, 1);
        assert_eq!(rows[0].deal_id, "c");

        // 除外を外せば B社 も出る
        let (rows2, _) = collect_chase_stop(&d, 3.0, false);
        assert_eq!(rows2.len(), 2);
        assert!(rows2.iter().any(|r| r.houjin_won));
    }

    // --------------------------------------------------------- 不明値の扱い

    #[test]
    fn 業界の欠損値は除外し規模の不明は残す() {
        // 業界マスタの欠損「一」が業界別アポ率の1位に出る事故があった。
        // 一方で規模の「00_不明」はサブスク新規の山の大きさが情報なので残す。
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![
                vec!["1", "東京都", "一", "00_不明", "1000", "50"],
                vec!["1", "大阪府", "E 製造業", "02_31-50", "1000", "10"],
            ],
        );
        let (r, _) = collect_segment(&d, &TargetQuery::default(), None);
        assert_eq!(
            r.by_industry.iter().map(|x| x.value.as_str()).collect::<Vec<_>>(),
            vec!["E 製造業"],
            "「一」は業界ではなく欠損"
        );
        assert!(r.by_size.iter().any(|x| x.is_unknown_bucket));
        assert!(is_unknown_segment_value("_unknown"));
        assert!(is_unknown_segment_value(""));
        assert!(!is_unknown_segment_value("E 製造業"));
    }

    #[test]
    fn 少サンプルのセグメントは除外される() {
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![
                vec!["1", "鳥取県", "E 製造業", "02_31-50", "4", "1"], // 4架電1アポ=25%
                vec!["1", "東京都", "E 製造業", "02_31-50", "1000", "10"],
            ],
        );
        let (r, _) = collect_segment(&d, &TargetQuery::default(), None);
        assert_eq!(
            r.by_prefecture.iter().map(|x| x.value.as_str()).collect::<Vec<_>>(),
            vec!["東京都"],
            "架電100件未満は率が不安定なので順位に出さない"
        );
    }

    // ------------------------------------------------------------- クロス絞込

    #[test]
    fn セグメント別のクロス絞込が効く() {
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![
                vec!["1", "東京都", "E 製造業", "02_31-50", "1000", "10"],
                vec!["1", "東京都", "D 建設業", "02_31-50", "1000", "50"],
            ],
        );
        let q = TargetQuery {
            industry: Some("D 建設業".into()),
            ..Default::default()
        };
        let (r, matched) = collect_segment(&d, &q, None);
        assert_eq!(matched, 1);
        assert!(r.cross_active);
        assert!(r.cross_label.contains("D 建設業"));
        assert_eq!(r.by_prefecture[0].apo_rate, Some(5.0));
        // `__all__` は「絞らない」（画面の既定値）
        let q2 = TargetQuery {
            industry: Some("__all__".into()),
            ..Default::default()
        };
        let (r2, matched2) = collect_segment(&d, &q2, None);
        assert!(!r2.cross_active);
        assert_eq!(matched2, 2);
    }

    // ----------------------------------------------------- 未実装の明示

    #[test]
    fn 未実装パネルは黙って空を返さない() {
        // ①②③ は HubSpot ライブ照会でシートが無い。空配列で「該当なし」に見せない。
        let list = not_implemented_list();
        assert!(list.len() >= 6);
        assert!(list.iter().all(|n| !n.reason.is_empty()));
        assert!(list
            .iter()
            .any(|n| n.name.contains("ファネル対比") && n.reason.contains("HubSpot")));
        assert!(list.iter().any(|n| n.name.contains("経営方針ターゲット")));
        assert_eq!(available_panels().len(), 7);
    }

    #[test]
    fn 分母ラベルは実態のhubspot_callを返す() {
        // GAS 版のラベルは「※Zoom発信が分母」だが、このシートに Zoom発信の列は無い。
        let d = sheet(
            &[
                "owner_id",
                "prefecture",
                "industry",
                "size_band",
                "call_count",
                "apo_count",
            ],
            vec![vec!["1", "東京都", "E 製造業", "02_31-50", "1000", "10"]],
        );
        let (r, _) = collect_segment(&d, &TargetQuery::default(), None);
        assert!(r.denominator_label.contains("HubSpot Call"));
        assert!(!r.denominator_label.contains("Zoom発信が分母"));
    }

    #[test]
    fn 期間rangeで行が切り替わる() {
        let d = sheet(
            &[
                "range",
                "range_label",
                "軸",
                "セグメント",
                "商談回数",
                "成約",
                "月単価",
                "総額目安",
            ],
            vec![
                vec!["all", "全期間", "全体", "全体", "1000", "75", "", ""],
                vec!["6m", "直近6ヶ月", "全体", "全体", "200", "10", "", ""],
            ],
        );
        let (a, _) = collect_field_close_rate(&d, "all");
        assert_eq!(a.overall.as_ref().unwrap().close_rate, Some(7.5));
        assert_eq!(a.range_label, "全期間");
        let (b, matched) = collect_field_close_rate(&d, "6m");
        assert_eq!(matched, 1);
        assert_eq!(b.overall.as_ref().unwrap().close_rate, Some(5.0));
        assert_eq!(b.available_ranges, vec!["6m".to_string(), "all".to_string()]);
    }

    // ---- sort_by の不正値（2026-08-17 追加） ----

    #[test]
    fn 受け付けるsort_by一覧が実際の並べ替えキーと一致する() {
        // `CALLER_SORT_KEYS` と `collect_caller_behavior` の `match sort_by` は
        // 片方だけ増やすと、増やした列が「解釈できない値」として報告されてしまう。
        // 実際に並び順が変わることで一致を確かめる。
        let d = sheet(
            &["role", "owner_id", "name", "総架電", "ユニーク先(Deal数)", "1Deal平均架電",
              "HHI*100(集中度)", "Top10集中率%", "90秒以上率%", "行動タイプ"],
            vec![
                vec!["sales", "1", "A", "10", "5", "2", "90", "80", "70", "集中型"],
                vec!["sales", "2", "B", "20", "1", "1", "10", "20", "30", "分散型"],
            ],
        );
        // 総架電なら B が先、それ以外のキーでは A が先になるデータにしてある
        let (out, _) = collect_caller_behavior(&d, "sales", "総架電");
        assert_eq!(out.rows[0].owner_id, "2");
        for key in CALLER_SORT_KEYS.iter().filter(|k| **k != "総架電") {
            let (out, _) = collect_caller_behavior(&d, "sales", key);
            assert_eq!(out.rows[0].owner_id, "1", "{key} で並べ替えが効いていない");
        }
        // 未知の列は総架電へ落ちる（挙動は変えない）
        let (out, _) = collect_caller_behavior(&d, "sales", "NONSENSE");
        assert_eq!(out.rows[0].owner_id, "2", "未知の列は総架電の並び");
    }

    #[test]
    fn sort_byの不正値は総架電に落ちたことを記録する() {
        let mut a = ValueAudit::new();
        let used = a.choice(
            "sort_by",
            Some("NONSENSE"),
            &CALLER_SORT_KEYS.join(" | "),
            |v| CALLER_SORT_KEYS.iter().find(|k| **k == v.trim()).map(|k| (*k).to_string()),
            || ("総架電".to_string(), "総架電".to_string()),
        );
        assert_eq!(used, "総架電");
        let v = a.into_vec();
        assert_eq!(v.len(), 1);
        assert_eq!(v[0].used.as_deref(), Some("総架電"));

        // 陰性対照
        let mut a = ValueAudit::new();
        a.choice(
            "sort_by",
            Some("HHI*100(集中度)"),
            &CALLER_SORT_KEYS.join(" | "),
            |v| CALLER_SORT_KEYS.iter().find(|k| **k == v.trim()).map(|k| (*k).to_string()),
            || ("総架電".to_string(), "総架電".to_string()),
        );
        assert!(a.is_empty());
    }
}
