//! P8「コンサル接触」タブ（GAS 版 `page-p8` の移植）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p8">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP8Benchmark` / `_drawP8Benchmark` / `renderP8Contribution` /
//!          `_drawP8Contribution` / `_renderP8NoCallList` / `_drawP8ActivityFromData`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getConsultingCallerBenchmark*` / `getConsultingContactContribution*` /
//!          `getConsultingContactWeekly` / `getConsultingActivityMonthly`）
//!
//! **接触の定義**: 電話(Call)のみ。Email/MTG は接触に数えない。
//! 現場の定義が「行動量を測る指標はコール数のみ」なので、Email/MTG/合計は
//! 画面でも payload でも **補足** と明示する（`ActivityRow` のコメント参照）。
//!
//! ------------------------------------------------------------------
//! 移植した領域（GAS の DOM id → このファイルの出力）
//! ------------------------------------------------------------------
//!   A層 コンサル別ベンチマーク
//!     p8-bench-kpis           → `BenchPanel::kpis`
//!     p8-bench-scatter        → `BenchPanel::scatter`（canvas 1）
//!     p8-bench-table          → `BenchPanel::table`（並び替え可）
//!     p8-bench-monthly-trend  → `BenchPanel::monthly`（canvas 2）
//!     p8-bench-status         → `BenchPanel::status`
//!   B層 案件別 接触量×継続/解約 寄与率
//!     p8-contrib-table        → `ContribPanel::bins`
//!     p8-contrib-bar          → `ContribPanel::bins`（同じ bins を棒で描く。canvas 3）
//!     p8-contrib-scatter      → `ContribPanel::scatter`（canvas 4）
//!     p8-contrib-status       → `ContribPanel::stats`
//!   C-1 7日以上未架電
//!     p8-c1-kpis              → `NoCallPanel::kpis`
//!     p8-no-call-list         → `NoCallPanel::rows`（高さ固定+スクロール）
//!   C-3 Call 月次推移 + 案件別ランキング
//!     p8-activity-kpis        → `ActivityPanel::kpis`
//!     p8-activity-monthly-trend → `ActivityPanel::monthly_call`（canvas 5）
//!     p8-activity-ranking     → `ActivityPanel::ranking`
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. **C-2 電話接触ヒートマップ**（旧 canvas `p8-heatmap`）
//!    → 移植しない。GAS 側で 2026-08-12 に**削除済み**（`renderP8ConsultingContact`
//!      の「C-2 ヒートマップ削除に伴い canvas 依存を解除」参照）。撤去済み機能なので
//!      移植対象外。
//! 2. **旧①全社収益サマリ**（`p8-rev-kpis` / `p8-rev-mrr` / `p8-rev-flow` /
//!    `p8-rev-status`、canvas 2枚）
//!    → 移植しない。index.html で `style="display:none"`。「要素IDは javascript.html が
//!      参照するため残す」とコメントされた**互換用の死んだDOM**であり、画面に出ていない。
//! 3. **旧2週連続ゼロ系**（`p8-scorecards` / `p8-alert-list`）
//!    → 移植しない。同じく display:none。C-1「7日未架電」に置き換え済み（2026-06-25）。
//! 4. **担当者別ランキング/ヒートマップ**（`p8-owner-rank-month` /
//!    `p8-owner-activity-ranking` / `p8-owner-activity-heatmap`、canvas 1枚）
//!    → 移植しない。display:none。A層(コンサル別ベンチマーク)と機能重複のため隠された。
//! 5. **Deal×月ヒートマップ**（`p8-activity-deal-month-heatmap`、canvas 1枚）
//!    → 移植しない。display:none。「A層と機能重複するため隠す」と index.html に明記。
//! 6. **行動量の種別セレクタ**（`p8-activity-metric`）
//!    → 移植しない。display:none かつ `call_count` 固定。C-3 は Call 主軸に確定済み。
//! 7. **アラート除外の追加/解除**（GAS `addAlertException` / `removeAlertException`）
//!    → 実装しない。**スプレッドシートへの書き込み**であり、この担当範囲(本番書込禁止)外。
//!      読み取り側（除外リストを反映して C-1 から外す）は実装済み。
//!
//! canvas は GAS 側に 9枚あるが、実際に表示されているのは上記 5枚。残る 4枚は
//! 上記 2/4/5 の display:none 分。
//!
//! ------------------------------------------------------------------
//! GAS と意図的に違えた点（完了条件4）
//! ------------------------------------------------------------------
//! - **率は 0..1 でなく % (0..100) で返す**。GAS は解約率だけ 0..1、継続率だけ % と
//!   混在していた。`tabs::rate()` に合わせて全て % に統一する。
//!   散布図の y 軸上限は 1 → 100 になる（表示は同じ）。
//! - **率は件数から計算し直す**。GAS はシートの `continuation_rate` 列をそのまま使う。
//!   実データでは `n_continued / n_decided` と完全一致することを確認済み
//!   （例: 藤中 6/7 = 0.857142857… がシート値と一致）。計算し直す理由は
//!   **決着0件のときに 0% でなく null を返すため**（tabs/mod.rs の約束2）。
//! - **並びは Rust の安定ソート**で、同値のときシートの行順を保つ。GAS は
//!   `Array.prototype.sort`(ES2019 以降は安定) に依存していたので挙動は同じ。
//! - **コンサル名の並び替えは Unicode コードポイント順**。GAS は
//!   `localeCompare(_, 'ja')` を使う。漢字の読み順にはどちらもならないが、
//!   同じ入力でも並びが一致しない場合がある（表示順のみの差で、数値は不変）。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

use super::{rate, SourceInfo, TabPayload};

// ---------------------------------------------------------------- シート名

const SHEET_BENCH: &str = "コンサル別ベンチマーク";
const SHEET_BENCH_MONTHLY: &str = "コンサル別ベンチマーク_月別";
const SHEET_BENCH_META: &str = "コンサル別ベンチマーク_統計";
const SHEET_CONTRIB: &str = "コンサル接触寄与率";
const SHEET_CONTRIB_DIST: &str = "コンサル接触寄与率_分布";
const SHEET_CONTRIB_META: &str = "コンサル接触寄与率_統計";
const SHEET_WEEKLY: &str = "コンサル接触率_週次";
const SHEET_ACTIVITY: &str = "コンサル行動量_月次";
/// GAS `getAlertExceptions()` が読む「この案件はもうアラートに出さない」台帳。
/// Python 側(`consulting_display.load_alert_exceptions`)も同じシートを読む。
/// **無いことがある**（一度も除外操作をしていない環境）ので、取得失敗は空扱いにする。
const SHEET_ALERT_EXCEPTIONS: &str = "アラート除外リスト";

// ---------------------------------------------------------------- 上限・閾値

/// A層 表の表示件数（GAS `sorted.slice(0, 50)`）。実データ 28名なので通常は切れない。
const BENCH_TABLE_LIMIT: usize = 50;
/// A層 表で「率が振れやすい」と薄字にする決着件数の下限（GAS `MIN_DECIDED`）。
const MIN_DECIDED: f64 = 10.0;
/// A層 月別推移の表示月数（GAS `validMonths.slice(-24)`）。
const BENCH_MONTHLY_MONTHS: usize = 24;
/// B層 散布図の点数上限。GAS は全件返していた。実データ 2,535件なので通常は切れないが、
/// 黙って増え続けないよう旗を立てられるようにしておく（約束3）。
const CONTRIB_SCATTER_LIMIT: usize = 3000;
/// C-1 未架電リストの表示件数（GAS `MAX_ROWS`）。
const NO_CALL_LIMIT: usize = 200;
/// C-1 で「重症」とみなす経過日数（GAS は 14日以上を bad 色で出す）。
const NO_CALL_CRITICAL_DAYS: i64 = 14;
/// C-3 月次推移の表示月数（GAS `months.slice(-12)`）。
const ACTIVITY_MONTHS: usize = 12;
/// C-3 ランキングの表示件数（GAS `.slice(0, 20)`）。
const ACTIVITY_RANK_LIMIT: usize = 20;

// ---------------------------------------------------------------- 小道具

/// 数値化。空文字・非数値は 0。GAS の `num()` と同じ挙動。
fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 「値が無い」と「0」を区別したいときに使う。空文字は None。
/// C-1 の `days_since_last_call`（Call が1件も無い Deal は空欄）で必要。
fn opt_num(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

/// シートの真偽値。GAS は `flag === true || String(flag).toLowerCase() === 'true'`。
/// Python バッチが `True` と書くので小文字化して比較する。"1" は真としない（GAS と同じ）。
fn truthy(s: &str) -> bool {
    s.trim().eq_ignore_ascii_case("true")
}

/// `metric, value` 2列のメタシートを辞書にする。
/// GAS 側も `rows.forEach(r => meta[r.metric] = r.value)` と同じ形にしている。
fn meta_map(d: &SheetData) -> HashMap<String, String> {
    let mut m = HashMap::new();
    for row in &d.rows {
        let k = d.get(row, "metric").trim();
        if k.is_empty() {
            continue;
        }
        m.insert(k.to_string(), d.get(row, "value").trim().to_string());
    }
    m
}

fn meta_num(m: &HashMap<String, String>, k: &str) -> Option<f64> {
    m.get(k).and_then(|v| opt_num(v))
}

fn meta_str(m: &HashMap<String, String>, k: &str) -> String {
    m.get(k).cloned().unwrap_or_else(|| "―".to_string())
}

// ---------------------------------------------------------------- 相関の翻訳

/// 相関係数を現場が読める日本語にする（GAS `_corrWord`、2026-08-13 レビュー C6）。
///
/// 「r がマイナスなのはイメージが湧かない」への対応。向き(多い/少ない)と
/// 強さ(ほぼ無関係〜強い)の2軸で表現し、**因果と読ませない**ため
/// 「〜すれば〜なる」という言い方は絶対にしない。
fn corr_word(r: Option<f64>) -> String {
    let v = match r {
        Some(v) if v.is_finite() => v,
        _ => return "データ不足".to_string(),
    };
    let a = v.abs();
    let strength = if a < 0.2 {
        "ほぼ関係なし"
    } else if a < 0.4 {
        "わずかに関係あり"
    } else if a < 0.6 {
        "そこそこ関係あり"
    } else if a < 0.8 {
        "はっきり関係あり"
    } else {
        "強く関係あり"
    };
    if a < 0.2 {
        return strength.to_string();
    }
    if v > 0.0 {
        format!("コールが多いほど継続率が高い（{strength}）")
    } else {
        format!("コールが多いほど継続率が低い（{strength}）")
    }
}

/// 相関の1件ぶん。生の r は括弧内の補足として残し、主表示は日本語にする。
#[derive(Debug, Serialize)]
pub struct Correlation {
    /// 日本語の言い換え（主表示）
    pub word: String,
    /// 生値。画面では「（r=-0.630）」のように括弧内へ
    pub r: Option<f64>,
    /// 因果と読ませないための注記。画面から消さないこと。
    pub caution: &'static str,
}

impl Correlation {
    fn new(r: Option<f64>) -> Self {
        Self {
            word: corr_word(r),
            r,
            caution: "相関であって因果ではありません。どちらが原因かは分かりません",
        }
    }
}

// ---------------------------------------------------------------- X軸の上限

/// 散布図 X 軸の上限と、上限を超えて描画されない点の数。
///
/// 2026-08-13 レビュー C13/C14 の是正。2026-08-16 に実データで再計算した値:
///   B層「コンサル接触寄与率_分布」 n=2,535 / p50 1.09 / p95 5.05 / 最大 16.19
///     → 上限 5.1、**軸外 124件**。上限なしだと最大値に軸が3倍伸び、
///       ボリュームゾーン(0〜2)が左端に潰れて「全部0」に見えていた。
///   A層「コンサル別ベンチマーク」 n=28 / p50 3.05 / p95 4.80 / 最大 4.93
///     → 上限 5.0、軸外 0名。現状の分布では効かないが、外れ値が出たときの保険。
///
/// **軸外になった件数は必ずラベルに出す**（黙って隠さない）。
/// 注意: 件数が少ないと p95 の添字が最大値そのものを指すため上限は効かない
/// （GAS `_benchXMax` と同じ挙動。母数が小さいうちは切る必要も無い）。
#[derive(Debug, Serialize)]
pub struct XAxisCap {
    pub min: f64,
    pub max: f64,
    /// max を超えて描画されない点の数。0 なら軸ラベルに注記は出ない。
    pub over_count: usize,
    /// 軸ラベル本文（注記込み）
    pub title: String,
}

/// GAS `_benchXMax` の移植。
/// 95パーセンタイルを 0.1 刻みで切り上げ、最低でも 5 は確保する。
fn x_max(xs: &[f64]) -> f64 {
    if xs.is_empty() {
        return 5.0;
    }
    let mut v: Vec<f64> = xs.to_vec();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    // GAS: xs[Math.min(len-1, Math.floor(len*0.95))]
    let idx = (((v.len() as f64) * 0.95).floor() as usize).min(v.len() - 1);
    let p95 = v[idx];
    let capped = (p95 * 10.0).ceil() / 10.0;
    if capped > 5.0 {
        capped
    } else {
        5.0
    }
}

/// GAS `_benchXTitle` の移植。軸外の点数を必ずラベルへ出す。
fn x_axis_cap(xs: &[f64], base: &str, unit_ja: &str) -> XAxisCap {
    let mx = x_max(xs);
    let over = xs.iter().filter(|x| **x > mx).count();
    let title = if over > 0 {
        format!("{base} ※ {mx}超の{over}{unit_ja}は軸外のため非表示（表には含まれます）")
    } else {
        base.to_string()
    };
    XAxisCap {
        min: 0.0,
        max: mx,
        over_count: over,
        title,
    }
}

// ================================================================== A層

/// A層 表の並び替えキー。GAS の `data-benchsort` 属性値と1対1に対応させる。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum BenchSortKey {
    Name,
    NCohort,
    NContinued,
    NChurned,
    NActivePending,
    ContinuationRate,
    ChurnRate,
    CallsPerDealPerMonth,
}

impl BenchSortKey {
    fn parse(s: &str) -> Option<Self> {
        Some(match s.trim() {
            "name" => Self::Name,
            "n_cohort" => Self::NCohort,
            "n_continued" => Self::NContinued,
            "n_churned" => Self::NChurned,
            "n_active_pending" => Self::NActivePending,
            "continuation_rate" => Self::ContinuationRate,
            "churn_rate" => Self::ChurnRate,
            "calls_per_deal_per_month" => Self::CallsPerDealPerMonth,
            _ => return None,
        })
    }

    /// その列を初めて押したときの向き。
    /// GAS: 名前と解約率は昇順(あ→ん / 低い順)、それ以外は降順(多い順)から始める。
    fn default_dir(self) -> SortDir {
        match self {
            Self::Name | Self::ChurnRate => SortDir::Asc,
            _ => SortDir::Desc,
        }
    }

    /// 画面の見出し。GAS の `_bth()` 呼び出しと同じ並び・同じ文言。
    fn label(self) -> &'static str {
        match self {
            Self::Name => "コンサル",
            Self::NCohort => "コホート",
            Self::NContinued => "継続",
            Self::NChurned => "解約",
            Self::NActivePending => "結果待ち",
            Self::ContinuationRate => "継続率",
            Self::ChurnRate => "解約率",
            Self::CallsPerDealPerMonth => "月平均Call",
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::NCohort => "n_cohort",
            Self::NContinued => "n_continued",
            Self::NChurned => "n_churned",
            Self::NActivePending => "n_active_pending",
            Self::ContinuationRate => "continuation_rate",
            Self::ChurnRate => "churn_rate",
            Self::CallsPerDealPerMonth => "calls_per_deal_per_month",
        }
    }

    fn all() -> [Self; 8] {
        [
            Self::Name,
            Self::NCohort,
            Self::NContinued,
            Self::NChurned,
            Self::NActivePending,
            Self::ContinuationRate,
            Self::ChurnRate,
            Self::CallsPerDealPerMonth,
        ]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum SortDir {
    Asc,
    Desc,
}

impl SortDir {
    fn parse(s: &str) -> Option<Self> {
        match s.trim().to_ascii_lowercase().as_str() {
            "asc" | "1" | "up" => Some(Self::Asc),
            "desc" | "-1" | "down" => Some(Self::Desc),
            _ => None,
        }
    }
}

/// 並び替え可能な列の一覧。画面はこれを見て見出しを作る（見出し文言をフロントに二重定義しない）。
#[derive(Debug, Serialize)]
pub struct SortColumn {
    pub key: &'static str,
    pub label: &'static str,
    /// 数値列は右寄せ。名前だけ左寄せ。
    pub numeric: bool,
}

#[derive(Debug, Serialize)]
pub struct BenchKpis {
    /// 直近12ヶ月・決着3件以上のコンサル人数
    pub n_consultants: usize,
    /// 月単位で見た「月平均Call」と「継続率」の連動
    pub pearson_monthly: Correlation,
    /// 順位で見た場合（外れ値の影響を受けにくい）
    pub spearman_monthly: Correlation,
    pub n_cohort_total: f64,
    pub n_continued_total: f64,
    pub n_churned_total: f64,
    pub n_active_pending_total: f64,
    /// Python バッチがこのシートを作った時刻
    pub generated_at: String,
}

/// 散布図の1点＝1コンサル。
#[derive(Debug, Serialize)]
pub struct BenchPoint {
    pub consultant_id: String,
    pub label: String,
    /// 横軸: 1案件あたり月平均Call（sum(call_post) / sum(契約月数)）
    pub x_calls_per_deal_per_month: f64,
    /// 縦軸: 解約率(%)。低いほど良い。決着0件なら null
    pub y_churn_rate_pct: Option<f64>,
    pub n_decided: f64,
    pub n_continued: f64,
    pub n_churned: f64,
    pub n_active_pending: f64,
    /// 点の大きさに使う（GAS: min(14, 4+sqrt(n_decided))）
    pub radius: f64,
    /// X軸上限を超えていて描画されない点。件数は `XAxisCap::over_count` にも出る。
    pub beyond_x_max: bool,
}

#[derive(Debug, Serialize)]
pub struct BenchScatter {
    pub points: Vec<BenchPoint>,
    pub x_axis: XAxisCap,
    /// 縦軸は 0〜100(%)。GAS は 0〜1 だった（このファイル冒頭の「意図的に違えた点」参照）
    pub y_max_pct: f64,
    pub y_title: &'static str,
}

#[derive(Debug, Serialize)]
pub struct BenchRow {
    pub rank: usize,
    pub consultant_id: String,
    pub consultant_name: String,
    pub n_cohort: f64,
    pub n_continued: f64,
    pub n_churned: f64,
    pub n_active_pending: f64,
    /// 決着 = 継続 + 解約（結果待ちは含めない）
    pub n_decided: f64,
    /// 継続 ÷ 決着。決着0件なら null（0% と区別する）
    pub continuation_rate_pct: Option<f64>,
    /// 解約 ÷ 決着。決着0件なら null
    pub churn_rate_pct: Option<f64>,
    pub calls_per_deal_per_month: f64,
    /// 決着件数が MIN_DECIDED 未満。画面では薄字＋※で出し、
    /// 率だけを見て優劣を判断させない（行は消さない＝事実は見せる）。
    pub low_sample: bool,
}

#[derive(Debug, Serialize)]
pub struct BenchSortState {
    pub key: BenchSortKey,
    pub dir: SortDir,
}

#[derive(Debug, Serialize)]
pub struct BenchTable {
    pub rows: Vec<BenchRow>,
    /// 絞り込み前の全コンサル数
    pub total_rows: usize,
    /// limit で切ったか（約束3: 黙って上位N件にしない）
    pub truncated: bool,
    pub limit: usize,
    pub min_decided: f64,
    pub sort: BenchSortState,
    /// 並び替え可能な列。画面はこの順に見出しを出す。
    pub sortable_columns: Vec<SortColumn>,
    /// ※の意味を推測させないための凡例
    pub legend: String,
}

/// 月別コホートの1点。
#[derive(Debug, Serialize)]
pub struct BenchMonthlyPoint {
    pub expiration_month: String,
    pub n_cohort: f64,
    pub n_continued: f64,
    pub n_churned: f64,
    pub n_active_pending: f64,
    pub n_decided: f64,
    pub continuation_rate_pct: Option<f64>,
    pub calls_per_deal_per_month: f64,
}

#[derive(Debug, Serialize)]
pub struct BenchPanel {
    pub kpis: BenchKpis,
    pub scatter: BenchScatter,
    pub table: BenchTable,
    pub monthly: Vec<BenchMonthlyPoint>,
    pub status: String,
}

/// A層を組み立てる。
///
/// シートと列:
///   「コンサル別ベンチマーク」   consultant_id / consultant_name / n_cohort /
///                               n_continued / n_churned / n_active_pending /
///                               n_decided / continuation_rate / sum_calls_post /
///                               sum_months / calls_per_deal_per_month
///   「コンサル別ベンチマーク_月別」 expiration_month / n_cohort / n_continued /
///                               n_churned / n_active_pending / sum_calls_post /
///                               sum_months / n_decided / continuation_rate /
///                               calls_per_deal_per_month
///   「コンサル別ベンチマーク_統計」 metric / value
///
/// 集計:
///   - 継続率/解約率は **n_continued, n_churned から計算し直す**（決着0件は null）。
///     シートの `continuation_rate` 列とは決着>0のとき一致することを実データで確認済み。
///   - 散布図の X 上限は 95パーセンタイル（`x_max`）。超過点は `beyond_x_max`。
///   - 表は既定「解約率の低い順」。`sort` で列と向きを差し替える。
pub fn build_bench(
    bench: &SheetData,
    monthly: &SheetData,
    meta: &SheetData,
    sort: &BenchSortState,
) -> BenchPanel {
    let m = meta_map(meta);

    // --- 各コンサルの行を読む（列は名前で引く） ---
    struct Raw {
        id: String,
        name: String,
        n_cohort: f64,
        n_continued: f64,
        n_churned: f64,
        n_pending: f64,
        cpdpm: f64,
    }
    let raws: Vec<Raw> = bench
        .rows
        .iter()
        .map(|r| Raw {
            id: bench.get(r, "consultant_id").to_string(),
            name: {
                let n = bench.get(r, "consultant_name").trim().to_string();
                if n.is_empty() {
                    bench.get(r, "consultant_id").to_string()
                } else {
                    n
                }
            },
            n_cohort: num(bench.get(r, "n_cohort")),
            n_continued: num(bench.get(r, "n_continued")),
            n_churned: num(bench.get(r, "n_churned")),
            n_pending: num(bench.get(r, "n_active_pending")),
            cpdpm: num(bench.get(r, "calls_per_deal_per_month")),
        })
        .collect();

    // --- 散布図 ---
    let xs: Vec<f64> = raws.iter().map(|r| r.cpdpm).collect();
    let axis = x_axis_cap(&xs, "1案件あたり 月平均Call数 (HubSpotログのみ)", "名");
    let points: Vec<BenchPoint> = raws
        .iter()
        .map(|r| {
            let decided = r.n_continued + r.n_churned;
            BenchPoint {
                consultant_id: r.id.clone(),
                label: r.name.clone(),
                x_calls_per_deal_per_month: r.cpdpm,
                y_churn_rate_pct: rate(r.n_churned, decided),
                n_decided: decided,
                n_continued: r.n_continued,
                n_churned: r.n_churned,
                n_active_pending: r.n_pending,
                // GAS: Math.min(14, 4 + sqrt(n_decided))
                radius: (4.0 + decided.max(0.0).sqrt()).min(14.0),
                beyond_x_max: r.cpdpm > axis.max,
            }
        })
        .collect();

    // --- 表 ---
    // 並び替えは安定ソート。同値のときシートの行順を保つ（GAS の Array.sort と同じ）。
    let mut order: Vec<usize> = (0..raws.len()).collect();
    order.sort_by(|&a, &b| {
        let (ra, rb) = (&raws[a], &raws[b]);
        let ord = match sort.key {
            BenchSortKey::Name => ra.name.cmp(&rb.name),
            _ => {
                let f = |r: &Raw| -> f64 {
                    let decided = r.n_continued + r.n_churned;
                    match sort.key {
                        BenchSortKey::NCohort => r.n_cohort,
                        BenchSortKey::NContinued => r.n_continued,
                        BenchSortKey::NChurned => r.n_churned,
                        BenchSortKey::NActivePending => r.n_pending,
                        // 率が出せない(決着0件)行は 0 として並べる。
                        // GAS も NaN を 0 に倒していた。表示は null のままなので
                        // 「0%」と誤読させることはない。
                        BenchSortKey::ContinuationRate => {
                            rate(r.n_continued, decided).unwrap_or(0.0)
                        }
                        BenchSortKey::ChurnRate => rate(r.n_churned, decided).unwrap_or(0.0),
                        BenchSortKey::CallsPerDealPerMonth => r.cpdpm,
                        BenchSortKey::Name => 0.0,
                    }
                };
                f(ra)
                    .partial_cmp(&f(rb))
                    .unwrap_or(std::cmp::Ordering::Equal)
            }
        };
        match sort.dir {
            SortDir::Asc => ord,
            SortDir::Desc => ord.reverse(),
        }
    });

    let total_rows = raws.len();
    let truncated = total_rows > BENCH_TABLE_LIMIT;
    let rows: Vec<BenchRow> = order
        .iter()
        .take(BENCH_TABLE_LIMIT)
        .enumerate()
        .map(|(i, &idx)| {
            let r = &raws[idx];
            let decided = r.n_continued + r.n_churned;
            BenchRow {
                rank: i + 1,
                consultant_id: r.id.clone(),
                consultant_name: r.name.clone(),
                n_cohort: r.n_cohort,
                n_continued: r.n_continued,
                n_churned: r.n_churned,
                n_active_pending: r.n_pending,
                n_decided: decided,
                continuation_rate_pct: rate(r.n_continued, decided),
                churn_rate_pct: rate(r.n_churned, decided),
                calls_per_deal_per_month: r.cpdpm,
                low_sample: decided < MIN_DECIDED,
            }
        })
        .collect();

    let table = BenchTable {
        rows,
        total_rows,
        truncated,
        limit: BENCH_TABLE_LIMIT,
        min_decided: MIN_DECIDED,
        sort: BenchSortState {
            key: sort.key,
            dir: sort.dir,
        },
        sortable_columns: BenchSortKey::all()
            .iter()
            .map(|k| SortColumn {
                key: k.as_str(),
                label: k.label(),
                numeric: !matches!(k, BenchSortKey::Name),
            })
            .collect(),
        legend: format!(
            "※ = 決着件数が{}件未満。率が大きく振れるため順位をそのまま実力とみなせません\
             (継続率 = 継続 ÷ 決着。結果待ちは分母に含みません)。\
             列の見出しをクリックすると並び替えできます（もう一度で昇順⇔降順）。",
            MIN_DECIDED as i64
        ),
    };

    // --- 月別コホート推移（判定済み n_decided>0 のみ・直近24ヶ月） ---
    let mut valid: Vec<BenchMonthlyPoint> = monthly
        .rows
        .iter()
        .filter(|r| num(monthly.get(r, "n_decided")) > 0.0)
        .map(|r| {
            let cont = num(monthly.get(r, "n_continued"));
            let churn = num(monthly.get(r, "n_churned"));
            let decided = num(monthly.get(r, "n_decided"));
            BenchMonthlyPoint {
                expiration_month: monthly.get(r, "expiration_month").to_string(),
                n_cohort: num(monthly.get(r, "n_cohort")),
                n_continued: cont,
                n_churned: churn,
                n_active_pending: num(monthly.get(r, "n_active_pending")),
                n_decided: decided,
                continuation_rate_pct: rate(cont, decided),
                calls_per_deal_per_month: num(monthly.get(r, "calls_per_deal_per_month")),
            }
        })
        .collect();
    if valid.len() > BENCH_MONTHLY_MONTHS {
        valid.drain(..valid.len() - BENCH_MONTHLY_MONTHS);
    }

    let pearson = meta_num(&m, "pearson_r_monthly");
    let spearman = meta_num(&m, "spearman_r_monthly");
    let n_cohort_total = meta_num(&m, "n_cohort_total").unwrap_or(0.0);
    let n_continued_total = meta_num(&m, "n_continued_total").unwrap_or(0.0);
    let n_churned_total = meta_num(&m, "n_churned_total").unwrap_or(0.0);
    let n_pending_total = meta_num(&m, "n_active_pending_total").unwrap_or(0.0);
    let generated_at = meta_str(&m, "generated_at");

    let status = format!(
        "n={}コンサル (決着≥3件 直近12ヶ月) / 月単位 r={} / cohort全体 {} \
         (継続 {} / 解約 {} / 結果待ち {}) / generated {}",
        total_rows,
        pearson.map(|v| format!("{v:.3}")).unwrap_or("―".into()),
        n_cohort_total as i64,
        n_continued_total as i64,
        n_churned_total as i64,
        n_pending_total as i64,
        generated_at,
    );

    BenchPanel {
        kpis: BenchKpis {
            n_consultants: total_rows,
            pearson_monthly: Correlation::new(pearson),
            spearman_monthly: Correlation::new(spearman),
            n_cohort_total,
            n_continued_total,
            n_churned_total,
            n_active_pending_total: n_pending_total,
            generated_at,
        },
        scatter: BenchScatter {
            points,
            x_axis: axis,
            y_max_pct: 100.0,
            y_title: "解約率 (低いほど良い)",
        },
        table,
        monthly: valid,
        status,
    }
}

// ================================================================== B層

#[derive(Debug, Serialize)]
pub struct ContribBin {
    pub bin: String,
    pub n_matured: f64,
    pub n_churned: f64,
    pub share_matured_pct: f64,
    pub share_churned_pct: f64,
    /// 解約群シェア − 満了群シェア。正=この Call 水準で解約が多い
    pub churn_minus_matured_pt: f64,
}

#[derive(Debug, Serialize)]
pub struct DealPoint {
    pub deal_id: String,
    pub label: String,
    /// 横軸: 月平均Call
    pub x_calls_per_month: f64,
    /// 縦軸: 契約継続月数（createdate→今日）
    pub y_months: f64,
    pub beyond_x_max: bool,
}

#[derive(Debug, Serialize)]
pub struct ContribScatter {
    pub matured: Vec<DealPoint>,
    pub churned: Vec<DealPoint>,
    pub x_axis: XAxisCap,
    pub y_title: &'static str,
    /// 上限で切ったか（実データ 2,535件では切れない）
    pub truncated: bool,
    pub limit: usize,
    /// 切る前の点数
    pub total_points: usize,
}

#[derive(Debug, Serialize)]
pub struct ContribStats {
    pub n_matured: f64,
    pub n_churned: f64,
    pub median_matured_cpm: Option<f64>,
    pub median_churned_cpm: Option<f64>,
    pub pearson: Correlation,
    pub spearman: Correlation,
    pub generated_at: String,
    pub status: String,
}

#[derive(Debug, Serialize)]
pub struct ContribPanel {
    /// 寄与率テーブルと分布バーは同じ bins を使う（GAS も同一データを2表現している）
    pub bins: Vec<ContribBin>,
    pub scatter: ContribScatter,
    pub stats: ContribStats,
}

/// B層を組み立てる。
///
/// シートと列:
///   「コンサル接触寄与率」     bin / n_matured / n_churned / share_matured_pct /
///                             share_churned_pct / churn_minus_matured_pt
///   「コンサル接触寄与率_分布」 deal_id / deal_label / consultant_id / consultant_name /
///                             is_matured / is_churned / call_post / months /
///                             calls_per_month / bin
///   「コンサル接触寄与率_統計」 metric / value
///
/// 集計:
///   - bins はシートの値をそのまま渡す（Python 側 `consulting_contact_contribution.py`
///     が算出済み。ここで再計算するとシートと画面で数字が割れる）。
///   - 散布図は満了/解約の2群に分ける。**どちらの旗も立っていない行(進行中)は除外**
///     （GAS と同じ。結果待ちを継続扱いにしない）。
///   - X 上限は A層と同じ 95パーセンタイル。
pub fn build_contribution(bins: &SheetData, dist: &SheetData, meta: &SheetData) -> ContribPanel {
    let bin_rows: Vec<ContribBin> = bins
        .rows
        .iter()
        .map(|r| ContribBin {
            bin: bins.get(r, "bin").to_string(),
            n_matured: num(bins.get(r, "n_matured")),
            n_churned: num(bins.get(r, "n_churned")),
            share_matured_pct: num(bins.get(r, "share_matured_pct")),
            share_churned_pct: num(bins.get(r, "share_churned_pct")),
            churn_minus_matured_pt: num(bins.get(r, "churn_minus_matured_pt")),
        })
        .collect();

    // 満了群 / 解約群。両方 false の行(進行中)は結果待ちなので落とす。
    struct Pt {
        matured: bool,
        p: DealPoint,
    }
    let all: Vec<Pt> = dist
        .rows
        .iter()
        .filter_map(|r| {
            let is_m = truthy(dist.get(r, "is_matured"));
            let is_c = truthy(dist.get(r, "is_churned"));
            if !is_m && !is_c {
                return None;
            }
            let deal_id = dist.get(r, "deal_id").to_string();
            let label = {
                let l = dist.get(r, "deal_label").trim().to_string();
                if l.is_empty() {
                    format!("(名称未取得 / Deal {deal_id})")
                } else {
                    l
                }
            };
            Some(Pt {
                matured: is_m,
                p: DealPoint {
                    deal_id,
                    label,
                    x_calls_per_month: num(dist.get(r, "calls_per_month")),
                    y_months: num(dist.get(r, "months")),
                    beyond_x_max: false, // 軸を決めてから埋める
                },
            })
        })
        .collect();

    let total_points = all.len();
    let truncated = total_points > CONTRIB_SCATTER_LIMIT;
    let xs: Vec<f64> = all.iter().map(|q| q.p.x_calls_per_month).collect();
    let axis = x_axis_cap(&xs, "月平均Call数", "件");

    let mut matured = Vec::new();
    let mut churned = Vec::new();
    for mut q in all.into_iter().take(CONTRIB_SCATTER_LIMIT) {
        q.p.beyond_x_max = q.p.x_calls_per_month > axis.max;
        if q.matured {
            matured.push(q.p);
        } else {
            churned.push(q.p);
        }
    }

    let m = meta_map(meta);
    let n_matured = meta_num(&m, "n_matured").unwrap_or(0.0);
    let n_churned = meta_num(&m, "n_churned").unwrap_or(0.0);
    let med_m = meta_num(&m, "median_matured_cpm");
    let med_c = meta_num(&m, "median_churned_cpm");
    let pr = meta_num(&m, "pearson_r");
    let sr = meta_num(&m, "spearman_r");
    let fmt2 = |v: Option<f64>| v.map(|x| format!("{x:.2}")).unwrap_or("―".into());
    let fmt3 = |v: Option<f64>| v.map(|x| format!("{x:.3}")).unwrap_or("―".into());
    let status = format!(
        "満了={}件 (中央値 月平均Call {}) / 解約={}件 (中央値 月平均Call {}) / \
         Pearson r={} / Spearman r={}",
        n_matured as i64,
        fmt2(med_m),
        n_churned as i64,
        fmt2(med_c),
        fmt3(pr),
        fmt3(sr),
    );

    ContribPanel {
        bins: bin_rows,
        scatter: ContribScatter {
            matured,
            churned,
            x_axis: axis,
            y_title: "契約継続月数 (createdate→今日)",
            truncated,
            limit: CONTRIB_SCATTER_LIMIT,
            total_points,
        },
        stats: ContribStats {
            n_matured,
            n_churned,
            median_matured_cpm: med_m,
            median_churned_cpm: med_c,
            pearson: Correlation::new(pr),
            spearman: Correlation::new(sr),
            generated_at: meta_str(&m, "generated_at"),
            status,
        },
    }
}

// ================================================================== C-1

#[derive(Debug, Serialize)]
pub struct NoCallKpis {
    /// 7日以上未架電の総数（表示件数ではなく実数。リストが切れても件数は必ず分かる）
    pub total: usize,
    /// うち Call 記録が1件も無い（作成日以降に通話履歴なし）
    pub no_call_record: usize,
    /// うち 最終Callから7日以上経過
    pub over_7days: usize,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NoCallSeverity {
    /// Call が1件も無い
    NoCallRecord,
    /// 14日以上
    Critical,
    /// 7〜13日
    Warning,
}

#[derive(Debug, Serialize)]
pub struct NoCallRow {
    pub rank: usize,
    pub deal_id: String,
    /// 取引先/案件名
    pub name: String,
    pub owner: String,
    /// Call が無い Deal は null（"(なし)" のような文字列を作らない）
    pub last_call_date: Option<String>,
    /// 経過日数。Call が無い Deal は null で末尾に並ぶ
    pub days_since_last_call: Option<f64>,
    pub severity: NoCallSeverity,
    pub hubspot_url: String,
}

/// 画面の描画契約。GAS 側の是正（レビュー C15）に合わせる。
/// 140件が縦に並んで画面を圧迫していたので、**高さを固定してスクロール**させ、
/// 件数は KPI カード側で別途明示する。
#[derive(Debug, Serialize)]
pub struct NoCallRender {
    pub max_height_px: u32,
    pub sticky_header: bool,
    pub note: &'static str,
}

#[derive(Debug, Serialize)]
pub struct NoCallPanel {
    pub kpis: NoCallKpis,
    pub rows: Vec<NoCallRow>,
    /// 表示した件数
    pub shown: usize,
    /// limit で切ったか
    pub truncated: bool,
    pub limit: usize,
    /// 除外リストで落とした件数（黙って消さない）
    pub excluded_count: usize,
    pub render: NoCallRender,
}

/// C-1 を組み立てる。
///
/// シート「コンサル接触率_週次」の列:
///   deal_id / deal_label / customer_name / customer_label / owner_id / owner_name /
///   pipeline_label / stage_label / week_start / ... / last_call_date /
///   days_since_last_call / alert_7day_no_call
///
/// 集計:
///   - `alert_7day_no_call` が真の行を **Deal 単位で1件に畳む**（週次行が複数あるが
///     アラート系の値は Deal 単位で同値。GAS も最初の1行を使う）。
///   - 「アラート除外リスト」に載っている deal_id は落とす。
///   - 経過日数の降順。**Call が1件も無い(空欄)行は末尾**（0日扱いにして先頭に出さない）。
pub fn build_no_call(weekly: &SheetData, excluded: &HashSet<String>) -> NoCallPanel {
    // 出現順を保ったまま Deal 単位に畳む
    let mut seen: HashSet<String> = HashSet::new();
    // 除外で落とした Deal も重複排除して数える（週次行の数ではなく案件数を出す）
    let mut dropped: HashSet<String> = HashSet::new();
    let mut picked: Vec<(String, &Vec<Arc<str>>)> = Vec::new();

    for r in &weekly.rows {
        let did = weekly.get(r, "deal_id").trim().to_string();
        if did.is_empty() {
            continue;
        }
        if !truthy(weekly.get(r, "alert_7day_no_call")) {
            continue;
        }
        if excluded.contains(&did) {
            dropped.insert(did);
            continue;
        }
        if seen.insert(did.clone()) {
            picked.push((did, r));
        }
    }
    let excluded_count = dropped.len();

    // 経過日数の降順。None(Call 0件) は末尾。安定ソートなので同値はシート順を保つ。
    picked.sort_by(|a, b| {
        let av = opt_num(weekly.get(a.1, "days_since_last_call"));
        let bv = opt_num(weekly.get(b.1, "days_since_last_call"));
        match (av, bv) {
            (None, None) => std::cmp::Ordering::Equal,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (Some(_), None) => std::cmp::Ordering::Less,
            (Some(x), Some(y)) => y.partial_cmp(&x).unwrap_or(std::cmp::Ordering::Equal),
        }
    });

    let total = picked.len();
    let no_call_record = picked
        .iter()
        .filter(|(_, r)| opt_num(weekly.get(r, "days_since_last_call")).is_none())
        .count();
    let over_7days = total - no_call_record;
    let truncated = total > NO_CALL_LIMIT;

    let rows: Vec<NoCallRow> = picked
        .iter()
        .take(NO_CALL_LIMIT)
        .enumerate()
        .map(|(i, (did, r))| {
            let days = opt_num(weekly.get(r, "days_since_last_call"));
            // 名前は customer_label → customer_name → deal_label → deal_id の順（GAS と同じ）
            let name = ["customer_label", "customer_name", "deal_label"]
                .iter()
                .map(|k| weekly.get(r, k).trim())
                .find(|v| !v.is_empty())
                .map(|v| v.to_string())
                .unwrap_or_else(|| format!("(名称未取得 / Deal {did})"));
            let owner = {
                let o = weekly.get(r, "owner_name").trim();
                if o.is_empty() {
                    let id = weekly.get(r, "owner_id").trim();
                    if id.is_empty() {
                        "-".to_string()
                    } else {
                        id.to_string()
                    }
                } else {
                    o.to_string()
                }
            };
            let last = {
                let v = weekly.get(r, "last_call_date").trim();
                if v.is_empty() {
                    None
                } else {
                    Some(v.to_string())
                }
            };
            NoCallRow {
                rank: i + 1,
                deal_id: did.clone(),
                name,
                owner,
                last_call_date: last,
                days_since_last_call: days,
                severity: match days {
                    None => NoCallSeverity::NoCallRecord,
                    Some(d) if d >= NO_CALL_CRITICAL_DAYS as f64 => NoCallSeverity::Critical,
                    Some(_) => NoCallSeverity::Warning,
                },
                // ポータル 23708633 = リクロジ事業部
                hubspot_url: format!("https://app.hubspot.com/contacts/23708633/deal/{did}"),
            }
        })
        .collect();

    NoCallPanel {
        kpis: NoCallKpis {
            total,
            no_call_record,
            over_7days,
        },
        shown: rows.len(),
        rows,
        truncated,
        limit: NO_CALL_LIMIT,
        excluded_count,
        render: NoCallRender {
            max_height_px: 340,
            sticky_header: true,
            note: "リストは高さ固定でスクロールします。総件数は上の KPI カードを見てください",
        },
    }
}

// ================================================================== C-3

#[derive(Debug, Serialize)]
pub struct ActivityKpis {
    /// この KPI が指す月（year_month の最大値）
    pub year_month: String,
    /// **その月がまだ進行中か**。
    ///
    /// 2026-08-17 追加。ここは「直近月の実数」を出すパネルなので、
    /// 全社サマリのように当月を消してしまうと見たいものが見えなくなる。
    /// ただし進行中の当月は集計が途中で、前月より大幅に低い値がそのまま出る
    /// （実測: 2026-08 の Call 738 が、月末値のように並ぶ）。
    /// **利用側が「途中集計だ」と知る手段が無かった**ので旗を返す。
    /// 数字は消さず、読み方だけ添える。
    pub is_partial: bool,
    /// 主軸。行動量を測る指標はコール数のみ、というのが現場の定義
    pub call: f64,
    /// 以下は補足（順位にも評価にも使わない）
    pub email: f64,
    pub mtg_total: f64,
    pub mtg_meeting: f64,
    pub mtg_email_track: f64,
    pub mtg_recording_track: f64,
    pub total: f64,
}

#[derive(Debug, Serialize)]
pub struct MonthlyCall {
    pub year_month: String,
    pub call_count: f64,
    /// 進行中の当月か。折れ線の最終点が落ち込んで見える理由を画面で説明するため。
    pub is_partial: bool,
}

#[derive(Debug, Serialize)]
pub struct ActivityRow {
    pub rank: usize,
    pub deal_id: String,
    pub deal_label: String,
    pub owner: String,
    /// 「PL / ステージ」
    pub pipeline_stage: String,
    /// **この順位の基準**
    pub call_count: f64,
    /// 以下 3つは補足。順位には使っていない
    pub email_count: f64,
    pub mtg_total: f64,
    pub total_count: f64,
    pub hubspot_url: String,
}

#[derive(Debug, Serialize)]
pub struct ActivityPanel {
    pub latest_month: String,
    /// 推移に出す月（直近12ヶ月）
    pub months: Vec<String>,
    pub kpis: ActivityKpis,
    /// Call 月次推移（Deal 全件合算）
    pub monthly_call: Vec<MonthlyCall>,
    pub ranking: Vec<ActivityRow>,
    pub ranking_total: usize,
    pub ranking_truncated: bool,
    pub ranking_limit: usize,
    /// 対象 Deal 数（重複排除）
    pub deal_count: usize,
    pub ranking_note: &'static str,
}

/// C-3 を組み立てる。
///
/// シート「コンサル行動量_月次」の列:
///   deal_id / deal_label / customer_id / customer_label / owner_id / owner_name /
///   pipeline_label / stage_label / year_month / call_count / email_count /
///   mtg_count / mtg_count_email_track / mtg_count_recording_track / total_count
///
/// 集計:
///   - 直近月 = year_month の最大値（文字列 "YYYY-MM" なので辞書順＝時系列順）。
///   - 推移は直近12ヶ月ぶんの **call_count のみ** を Deal 全件で合算。
///     GAS は種別セレクタで切替できたが、現在は display:none の `call_count` 固定。
///   - ランキングは直近月の行を **call_count 降順**で上位20件。
///     Email/MTG/合計は同じ行から拾うが、順位には一切使わない。
pub fn build_activity(act: &SheetData) -> ActivityPanel {
    // 月の一覧（昇順・重複排除）
    let mut months: Vec<String> = {
        let mut s: HashSet<&str> = HashSet::new();
        for r in &act.rows {
            let ym = act.get(r, "year_month").trim();
            if !ym.is_empty() {
                s.insert(ym);
            }
        }
        let mut v: Vec<String> = s.into_iter().map(|x| x.to_string()).collect();
        v.sort();
        v
    };
    let latest = months.last().cloned().unwrap_or_default();
    if months.len() > ACTIVITY_MONTHS {
        months.drain(..months.len() - ACTIVITY_MONTHS);
    }
    let recent: HashSet<&str> = months.iter().map(|s| s.as_str()).collect();

    // 直近月の行
    let last_rows: Vec<&Vec<Arc<str>>> = act
        .rows
        .iter()
        .filter(|r| act.get(r, "year_month").trim() == latest)
        .collect();

    // KPI（直近月の合算）
    let mut k = ActivityKpis {
        is_partial: super::jst_current_ym() == latest,
        year_month: latest.clone(),
        call: 0.0,
        email: 0.0,
        mtg_total: 0.0,
        mtg_meeting: 0.0,
        mtg_email_track: 0.0,
        mtg_recording_track: 0.0,
        total: 0.0,
    };
    for r in &last_rows {
        k.call += num(act.get(r, "call_count"));
        k.email += num(act.get(r, "email_count"));
        k.mtg_meeting += num(act.get(r, "mtg_count"));
        k.mtg_email_track += num(act.get(r, "mtg_count_email_track"));
        k.mtg_recording_track += num(act.get(r, "mtg_count_recording_track"));
        k.total += num(act.get(r, "total_count"));
    }
    k.mtg_total = k.mtg_meeting + k.mtg_email_track + k.mtg_recording_track;

    // 月次推移（Call のみ）。月が0件でも 0 の点を出す（線が飛ばないように）
    let mut by_month: HashMap<&str, f64> = recent.iter().map(|m| (*m, 0.0)).collect();
    for r in &act.rows {
        let ym = act.get(r, "year_month").trim();
        if let Some(v) = by_month.get_mut(ym) {
            *v += num(act.get(r, "call_count"));
        }
    }
    let monthly_call: Vec<MonthlyCall> = months
        .iter()
        .map(|m| MonthlyCall {
            is_partial: &super::jst_current_ym() == m,
            year_month: m.clone(),
            call_count: *by_month.get(m.as_str()).unwrap_or(&0.0),
        })
        .collect();

    // ランキング（Call 降順・安定ソートで同値はシート順）
    let mut idx: Vec<&Vec<Arc<str>>> = last_rows.clone();
    idx.sort_by(|a, b| {
        let av = num(act.get(a, "call_count"));
        let bv = num(act.get(b, "call_count"));
        bv.partial_cmp(&av).unwrap_or(std::cmp::Ordering::Equal)
    });
    let ranking_total = idx.len();
    let ranking: Vec<ActivityRow> = idx
        .iter()
        .take(ACTIVITY_RANK_LIMIT)
        .enumerate()
        .map(|(i, r)| {
            let did = act.get(r, "deal_id").trim().to_string();
            let label = ["deal_label", "customer_label", "customer_name"]
                .iter()
                .map(|c| act.get(r, c).trim())
                .find(|v| !v.is_empty())
                .map(|v| v.to_string())
                // 2026-08-13 レビュー C28: 「Deal 12345」だと ID が案件名に見える。
                // 名前でないと分かる表記にする。
                .unwrap_or_else(|| format!("(名称未取得 / Deal {did})"));
            let owner = {
                let o = act.get(r, "owner_name").trim();
                if o.is_empty() {
                    let id = act.get(r, "owner_id").trim();
                    if id.is_empty() {
                        "-".to_string()
                    } else {
                        id.to_string()
                    }
                } else {
                    o.to_string()
                }
            };
            let ps = {
                let p = act.get(r, "pipeline_label").trim();
                let s = act.get(r, "stage_label").trim();
                match (p.is_empty(), s.is_empty()) {
                    (false, false) => format!("{p} / {s}"),
                    (false, true) => p.to_string(),
                    (true, false) => s.to_string(),
                    (true, true) => "-".to_string(),
                }
            };
            ActivityRow {
                rank: i + 1,
                deal_label: label,
                owner,
                pipeline_stage: ps,
                call_count: num(act.get(r, "call_count")),
                email_count: num(act.get(r, "email_count")),
                mtg_total: num(act.get(r, "mtg_count"))
                    + num(act.get(r, "mtg_count_email_track"))
                    + num(act.get(r, "mtg_count_recording_track")),
                total_count: num(act.get(r, "total_count")),
                hubspot_url: format!("https://app.hubspot.com/contacts/23708633/deal/{did}"),
                deal_id: did,
            }
        })
        .collect();

    let deal_count = {
        let mut s: HashSet<&str> = HashSet::new();
        for r in &act.rows {
            s.insert(act.get(r, "deal_id"));
        }
        s.len()
    };

    ActivityPanel {
        latest_month: latest,
        months,
        kpis: k,
        monthly_call,
        ranking,
        ranking_total,
        ranking_truncated: ranking_total > ACTIVITY_RANK_LIMIT,
        ranking_limit: ACTIVITY_RANK_LIMIT,
        deal_count,
        ranking_note: "直近月の Call 件数が多い順に上位20件。Email / MTG / 合計は \
                       状況把握のための補足で、順位には使っていません",
    }
}

// ================================================================== 全体

/// 画面上部に固定で出す注意書き。**消さないこと**。
/// 負の相関を「架電を減らせば継続率が上がる」と読み替えられた事故への対策で、
/// GAS 側では index.html にベタ書きされていた。文言を1箇所に集約する。
const CAUSALITY_NOTES: &[&str] = &[
    "接触の定義は電話(Call)のみ。Email/MTG は含めません。",
    "Call は HubSpot ログのみを数えます。社用携帯経由は記録されないため、\
     「Call が少ない」は「ログ経路を使っていない」可能性を含みます。",
    "月単位で「月平均Call が多い月ほど継続率が低い」という負の相関が出ますが、\
     ①解約リスクの高い月に架電を集中投入している（防衛架電が効いている） \
     ②解約しそうな案件に架電が集中するため見かけ上そう見える（逆因果） の \
     どちらも等しく成り立ちます。観測データだけでは決まりません。",
    "「架電を減らせば継続率が上がる」とは読まないでください。",
    "A層の解約率は「その月に契約終了日が到来した Deal」を母数にします。\
     解約分析タブ「C. コンサル担当別 解約状況」は保有 Deal 全体が母数で、別物です。",
];

#[derive(Debug, Serialize)]
pub struct P8Data {
    pub bench: BenchPanel,
    pub contribution: ContribPanel,
    pub no_call: NoCallPanel,
    pub activity: ActivityPanel,
    pub notes: &'static [&'static str],
}

/// 画面から受け取るパラメータ。
/// このタブは上部フィルタ(期間/PL/メンバー/都道府県)を**反映しない**
/// （Python 側で全期間集計済のため）。GAS 版と同じ。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct P8Query {
    /// A層の表の並び替え列。既定 `churn_rate`
    pub bench_sort_key: Option<String>,
    /// `asc` / `desc`。省略時は列ごとの自然な向き
    pub bench_sort_dir: Option<String>,
}

// このタブは上部フィルタ（期間/PL/メンバー/都道府県）を**反映しない**。
// つまり `?from=…&to=…` は 200 で返るが一切効かない。まさに検証担当が踏んだ
// 1回目の罠の形なので、それらは `ignored_params` に出るのが正しい。
crate::accepted_params!(P8Query, p8_query_accepted =>
    "bench_sort_key", "bench_sort_dir");

impl P8Query {
    fn sort_state(&self) -> BenchSortState {
        let key = self
            .bench_sort_key
            .as_deref()
            .and_then(BenchSortKey::parse)
            // 既定は「解約率の低い順」（GAS `_p8BenchSort = {key:'churn_rate', dir:1}`）
            .unwrap_or(BenchSortKey::ChurnRate);
        let dir = self
            .bench_sort_dir
            .as_deref()
            .and_then(SortDir::parse)
            .unwrap_or_else(|| key.default_dir());
        BenchSortState { key, dir }
    }
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
        // 絞り込みのあるシートは後で `set_matched` で上書きする
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

/// 「アラート除外リスト」を読む。
/// このシートは**存在しないことがある**（一度も除外操作をしていない環境）ので、
/// 取得失敗はエラーにせず空集合として扱う（GAS も try/catch で [] を返している）。
async fn load_excluded(client: &SheetsClient, store: &SheetStore) -> HashSet<String> {
    match store.get(client, SHEET_ALERT_EXCEPTIONS).await {
        Ok((d, _)) => d
            .rows
            .iter()
            .map(|r| d.get(r, "deal_id").trim().to_string())
            .filter(|s| !s.is_empty())
            .collect(),
        Err(_) => HashSet::new(),
    }
}

/// ハンドラ本体。8シート（+除外リスト）を読み、4パネルを組んで返す。
pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: P8Query,
) -> Result<TabPayload<P8Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let bench = load(client, store, SHEET_BENCH, &mut sources).await?;
    let bench_monthly = load(client, store, SHEET_BENCH_MONTHLY, &mut sources).await?;
    let bench_meta = load(client, store, SHEET_BENCH_META, &mut sources).await?;
    let contrib = load(client, store, SHEET_CONTRIB, &mut sources).await?;
    let contrib_dist = load(client, store, SHEET_CONTRIB_DIST, &mut sources).await?;
    let contrib_meta = load(client, store, SHEET_CONTRIB_META, &mut sources).await?;
    let weekly = load(client, store, SHEET_WEEKLY, &mut sources).await?;
    let activity = load(client, store, SHEET_ACTIVITY, &mut sources).await?;

    let excluded = load_excluded(client, store).await;

    let bench_panel = build_bench(&bench, &bench_monthly, &bench_meta, &q.sort_state());
    let contrib_panel = build_contribution(&contrib, &contrib_dist, &contrib_meta);
    let no_call_panel = build_no_call(&weekly, &excluded);
    let activity_panel = build_activity(&activity);

    // 絞り込みのあるシートは「何行が対象になったか」を実数で返す
    set_matched(&mut sources, SHEET_WEEKLY, no_call_panel.kpis.total);
    set_matched(&mut sources, SHEET_ACTIVITY, activity_panel.ranking_total);
    set_matched(
        &mut sources,
        SHEET_CONTRIB_DIST,
        contrib_panel.scatter.total_points,
    );
    set_matched(
        &mut sources,
        SHEET_BENCH_MONTHLY,
        bench_panel.monthly.len(),
    );

    Ok(TabPayload {
        data: P8Data {
            bench: bench_panel,
            contribution: contrib_panel,
            no_call: no_call_panel,
            activity: activity_panel,
            notes: CAUSALITY_NOTES,
        },
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

    /// 実データ(consulting_caller_benchmark.csv)と同じ列構成のベンチマークシート
    fn bench_sheet() -> SheetData {
        sheet(
            &[
                "consultant_id",
                "consultant_name",
                "n_cohort",
                "n_continued",
                "n_churned",
                "n_active_pending",
                "n_decided",
                "continuation_rate",
                "calls_per_deal_per_month",
            ],
            &[
                // 決着5件(<10) → low_sample。解約率 0%
                &["1", "浅川 紘誠", "5", "5", "0", "0", "5", "1.0", "4.79"],
                // 決着20件 → 解約率 50%
                &["2", "藤中 萌衣", "22", "10", "10", "2", "20", "0.5", "2.96"],
                // 決着0件（結果待ちだけ）→ 率は null
                &["3", "結果待ち 太郎", "3", "0", "0", "3", "0", "", "1.20"],
                // 外れ値。95パーセンタイル上限の検証用
                &["4", "外れ 値男", "12", "6", "6", "0", "12", "0.5", "16.19"],
            ],
        )
    }

    fn empty_meta() -> SheetData {
        sheet(&["metric", "value"], &[])
    }

    fn sorted(dir: SortDir, key: BenchSortKey) -> BenchPanel {
        build_bench(
            &bench_sheet(),
            &sheet(&["expiration_month", "n_decided"], &[]),
            &empty_meta(),
            &BenchSortState { key, dir },
        )
    }

    #[test]
    fn 決着0件の継続率と解約率はnoneで返る() {
        // 「決着が無い＝解約率0%」と読ませない（tabs/mod.rs の約束2）
        let p = sorted(SortDir::Asc, BenchSortKey::ChurnRate);
        let pending = p
            .table
            .rows
            .iter()
            .find(|r| r.consultant_name == "結果待ち 太郎")
            .expect("結果待ちの行が消えていない");
        assert_eq!(pending.churn_rate_pct, None);
        assert_eq!(pending.continuation_rate_pct, None);
        assert_eq!(pending.n_decided, 0.0);

        let point = p
            .scatter
            .points
            .iter()
            .find(|q| q.label == "結果待ち 太郎")
            .unwrap();
        assert_eq!(point.y_churn_rate_pct, None);
    }

    #[test]
    fn 決着件数が少ない行に旗が立つ() {
        // 決着6件で継続率100%が1位、決着67件で70.1%が4位…という誤読を防ぐ旗
        let p = sorted(SortDir::Asc, BenchSortKey::ChurnRate);
        let asa = p
            .table
            .rows
            .iter()
            .find(|r| r.consultant_name == "浅川 紘誠")
            .unwrap();
        assert!(asa.low_sample, "決着5件(<10)は low_sample");
        let fuji = p
            .table
            .rows
            .iter()
            .find(|r| r.consultant_name == "藤中 萌衣")
            .unwrap();
        assert!(!fuji.low_sample, "決着20件は low_sample にしない");
        // 行は消さない（事実は見せる）
        assert_eq!(p.table.rows.len(), 4);
    }

    #[test]
    fn ベンチ表は列と向きで並び替えできる() {
        // 既定 = 解約率の昇順
        let def = build_bench(
            &bench_sheet(),
            &sheet(&["expiration_month", "n_decided"], &[]),
            &empty_meta(),
            &P8Query::default().sort_state(),
        );
        assert_eq!(def.table.sort.key, BenchSortKey::ChurnRate);
        assert_eq!(def.table.sort.dir, SortDir::Asc);
        let names: Vec<&str> = def
            .table
            .rows
            .iter()
            .map(|r| r.consultant_name.as_str())
            .collect();
        // 0%(浅川) と null(結果待ち→0扱い) が先、50% の2人が後
        assert_eq!(names[0], "浅川 紘誠");
        assert!(names[2] == "藤中 萌衣" || names[2] == "外れ 値男");

        // 月平均Call の降順
        let by_call = sorted(SortDir::Desc, BenchSortKey::CallsPerDealPerMonth);
        assert_eq!(by_call.table.rows[0].consultant_name, "外れ 値男");
        assert_eq!(by_call.table.rows[1].consultant_name, "浅川 紘誠");

        // 解約数の降順
        let by_churn = sorted(SortDir::Desc, BenchSortKey::NChurned);
        assert_eq!(by_churn.table.rows[0].n_churned, 10.0);
    }

    #[test]
    fn 同値の行はシート順を保って並びが安定する() {
        // 藤中と外れ値男はどちらも解約率50%。毎回同じ並びで返ること
        let a = sorted(SortDir::Asc, BenchSortKey::ChurnRate);
        let b = sorted(SortDir::Asc, BenchSortKey::ChurnRate);
        let names = |p: &BenchPanel| -> Vec<String> {
            p.table
                .rows
                .iter()
                .map(|r| r.consultant_name.clone())
                .collect()
        };
        assert_eq!(names(&a), names(&b), "同じ入力なら同じ並び");
        let n = names(&a);
        let i_fuji = n.iter().position(|x| x == "藤中 萌衣").unwrap();
        let i_hazure = n.iter().position(|x| x == "外れ 値男").unwrap();
        assert!(i_fuji < i_hazure, "同値のときシートの行順を保つ");
    }

    /// 20名は月平均Call 1.0、1名だけ 16.19 の外れ値。
    /// 実データ(B層 2,535件で軸外124件)と同じ「右に裾を引く」形を最小構成で作る。
    fn bench_sheet_with_outlier() -> SheetData {
        let mut rows: Vec<Vec<String>> = (0..20)
            .map(|i| {
                vec![
                    format!("{i}"),
                    format!("平均{i}"),
                    "10".into(),
                    "5".into(),
                    "5".into(),
                    "0".into(),
                    "10".into(),
                    "0.5".into(),
                    "1.0".into(),
                ]
            })
            .collect();
        rows.push(vec![
            "99".into(),
            "外れ 値男".into(),
            "12".into(),
            "6".into(),
            "6".into(),
            "0".into(),
            "12".into(),
            "0.5".into(),
            "16.19".into(),
        ]);
        let refs: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|s| s.as_str()).collect())
            .collect();
        let slices: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        sheet(
            &[
                "consultant_id",
                "consultant_name",
                "n_cohort",
                "n_continued",
                "n_churned",
                "n_active_pending",
                "n_decided",
                "continuation_rate",
                "calls_per_deal_per_month",
            ],
            &slices,
        )
    }

    #[test]
    fn 散布図のx軸は95パーセンタイルで切り軸外件数を出す() {
        let p = build_bench(
            &bench_sheet_with_outlier(),
            &sheet(&["expiration_month", "n_decided"], &[]),
            &empty_meta(),
            &P8Query::default().sort_state(),
        );
        assert!(
            p.scatter.x_axis.max < 16.19,
            "最大値に軸を合わせない: max={}",
            p.scatter.x_axis.max
        );
        assert_eq!(p.scatter.x_axis.over_count, 1, "軸外は外れ値男の1名");
        assert!(
            p.scatter.x_axis.title.contains("軸外"),
            "軸外の件数をラベルに必ず出す: {}",
            p.scatter.x_axis.title
        );
        let hazure = p
            .scatter
            .points
            .iter()
            .find(|q| q.label == "外れ 値男")
            .unwrap();
        assert!(hazure.beyond_x_max, "点自体は消さず旗を立てる");
        assert_eq!(p.table.total_rows, 21, "軸外の点も表からは消さない");

        // 上限を超える点が無いときは注記を出さない（但し書きを無駄に増やさない）
        let clean = x_axis_cap(&[1.0, 2.0, 3.0], "月平均Call数", "件");
        assert_eq!(clean.over_count, 0);
        assert!(!clean.title.contains("軸外"));

        // 最低5は確保する（全員が低い値でも軸が潰れない）
        assert_eq!(x_max(&[0.1, 0.2, 0.3]), 5.0);
        // 0.1刻みで切り上げ
        assert!((x_max(&[1.0, 2.0, 3.0, 9.01]) - 9.1).abs() < 1e-9);
        // 空でも落ちない
        assert_eq!(x_max(&[]), 5.0);
        // 件数が少ないと p95 の添字が最大値を指すので上限は効かない。
        // GAS `_benchXMax` と同じ挙動であることを明示しておく（バグではない）。
        assert!((x_max(&[1.0, 1.0, 16.19]) - 16.2).abs() < 1e-9);
    }

    #[test]
    fn 相関係数は日本語に翻訳され因果と読ませない() {
        // 実データの pearson_r_monthly = -0.630
        assert_eq!(
            corr_word(Some(-0.630)),
            "コールが多いほど継続率が低い（はっきり関係あり）"
        );
        assert_eq!(
            corr_word(Some(0.45)),
            "コールが多いほど継続率が高い（そこそこ関係あり）"
        );
        // 弱すぎるときは向きを言わない（実データの案件別 pearson_r = 0.041）
        assert_eq!(corr_word(Some(0.041)), "ほぼ関係なし");
        assert_eq!(corr_word(None), "データ不足");
        let c = Correlation::new(Some(-0.630));
        assert_eq!(c.r, Some(-0.630), "生の r は括弧内の補足として残す");
        assert!(c.caution.contains("因果ではありません"));
    }

    fn weekly_sheet() -> SheetData {
        sheet(
            &[
                "deal_id",
                "customer_label",
                "owner_name",
                "last_call_date",
                "days_since_last_call",
                "alert_7day_no_call",
            ],
            &[
                // 同じ Deal が週次で複数行に出る → 1件に畳む
                &["100", "A社", "鶴見", "2026-06-02", "24", "True"],
                &["100", "A社", "鶴見", "2026-06-02", "24", "True"],
                // Call が1度も無い（空欄）→ 末尾へ
                &["200", "B社", "永田", "", "", "True"],
                // 7日台
                &["300", "C社", "酒匂", "2026-08-08", "8", "True"],
                // アラート対象外
                &["400", "D社", "浅川", "2026-08-15", "1", "False"],
                // 除外リスト入り
                &["500", "E社", "藤中", "2026-07-01", "46", "True"],
            ],
        )
    }

    #[test]
    fn 未架電リストはdeal単位に畳まれcall無しが末尾に来る() {
        let p = build_no_call(&weekly_sheet(), &HashSet::new());
        assert_eq!(p.kpis.total, 4, "100/200/300/500 の4件（400はフラグfalse）");
        assert_eq!(p.kpis.no_call_record, 1);
        assert_eq!(p.kpis.over_7days, 3);
        let ids: Vec<&str> = p.rows.iter().map(|r| r.deal_id.as_str()).collect();
        // 経過日数の降順: 500(46) → 100(24) → 300(8) → 200(Call無し)
        assert_eq!(ids, vec!["500", "100", "300", "200"]);
        assert_eq!(p.rows[0].days_since_last_call, Some(46.0));
        assert_eq!(
            p.rows[3].days_since_last_call, None,
            "Call が無い行を 0日 にしない"
        );
        assert!(p.rows[3].last_call_date.is_none());
        // 14日以上は重症、7〜13日は警告
        assert!(matches!(p.rows[1].severity, NoCallSeverity::Critical));
        assert!(matches!(p.rows[2].severity, NoCallSeverity::Warning));
        assert!(matches!(p.rows[3].severity, NoCallSeverity::NoCallRecord));
        // 高さ固定+スクロールの契約
        assert_eq!(p.render.max_height_px, 340);
        assert!(!p.truncated);
    }

    #[test]
    fn 未架電リストは除外リストを反映し件数も出す() {
        let mut ex = HashSet::new();
        ex.insert("500".to_string());
        let p = build_no_call(&weekly_sheet(), &ex);
        assert_eq!(p.kpis.total, 3);
        assert_eq!(p.excluded_count, 1, "黙って消さず件数を出す");
        assert!(p.rows.iter().all(|r| r.deal_id != "500"));
    }

    fn activity_sheet() -> SheetData {
        sheet(
            &[
                "deal_id",
                "deal_label",
                "owner_name",
                "pipeline_label",
                "stage_label",
                "year_month",
                "call_count",
                "email_count",
                "mtg_count",
                "mtg_count_email_track",
                "mtg_count_recording_track",
                "total_count",
            ],
            &[
                &["1", "A社", "鶴見", "リクロジ_納品管理", "定期1", "2026-07", "5", "1", "0", "0", "0", "6"],
                // 直近月。Email は多いが Call が少ない → 上位に来てはいけない
                &["1", "A社", "鶴見", "リクロジ_納品管理", "定期1", "2026-08", "2", "30", "1", "0", "0", "33"],
                &["2", "B社", "永田", "リクロジ_納品管理", "定期2", "2026-08", "9", "0", "0", "1", "0", "10"],
                &["3", "", "", "", "", "2026-08", "4", "2", "0", "0", "2", "8"],
            ],
        )
    }

    #[test]
    fn c3ランキングはcallが主軸でemailの多さに引っ張られない() {
        let p = build_activity(&activity_sheet());
        assert_eq!(p.latest_month, "2026-08");
        let order: Vec<&str> = p.ranking.iter().map(|r| r.deal_id.as_str()).collect();
        assert_eq!(order, vec!["2", "3", "1"], "Call 9 > 4 > 2 の順");
        // Email 30件の A社が先頭に来ていないこと（行動量の指標はコール数のみ）
        assert_eq!(p.ranking[0].deal_id, "2");
        assert_eq!(p.ranking[0].call_count, 9.0);
        // 補足列は拾うが順位に使わない
        assert_eq!(p.ranking[2].email_count, 30.0);
        assert_eq!(p.ranking[1].mtg_total, 2.0);
        assert!(p.ranking_note.contains("順位には使っていません"));
        // 名称が取れない行は ID を案件名のように見せない
        assert_eq!(p.ranking[1].deal_label, "(名称未取得 / Deal 3)");

        // KPI は直近月の合算（7月の Call 5 を混ぜない）
        assert_eq!(p.kpis.call, 15.0);
        assert_eq!(p.kpis.year_month, "2026-08");
        assert_eq!(p.kpis.mtg_total, 4.0);

        // 月次推移は直近12ヶ月ぶん、Call のみ
        assert_eq!(p.monthly_call.len(), 2);
        assert_eq!(p.monthly_call[0].year_month, "2026-07");
        assert_eq!(p.monthly_call[0].call_count, 5.0);
        assert_eq!(p.monthly_call[1].call_count, 15.0);
        assert_eq!(p.deal_count, 3);
    }

    #[test]
    fn b層は結果待ちを群に入れず分布と統計を返す() {
        let bins = sheet(
            &[
                "bin",
                "n_matured",
                "n_churned",
                "share_matured_pct",
                "share_churned_pct",
                "churn_minus_matured_pt",
            ],
            &[
                &["0回/月", "142", "42", "11.33", "3.28", "-8.06"],
                &["1.0-3.0回/月", "448", "563", "35.75", "43.92", "8.16"],
            ],
        );
        let dist = sheet(
            &[
                "deal_id",
                "deal_label",
                "is_matured",
                "is_churned",
                "calls_per_month",
                "months",
            ],
            &[
                &["11", "満了A", "True", "False", "1.0", "41.0"],
                &["12", "解約B", "False", "True", "0.14", "41.0"],
                // 進行中(どちらも False) は散布図に載せない
                &["13", "進行中C", "False", "False", "2.0", "3.0"],
                // 外れ値
                &["14", "解約D", "False", "True", "16.19", "12.0"],
            ],
        );
        let p = build_contribution(&bins, &dist, &empty_meta());
        assert_eq!(p.bins.len(), 2);
        assert_eq!(p.bins[1].churn_minus_matured_pt, 8.16);
        assert_eq!(
            p.scatter.total_points, 3,
            "進行中は結果待ちなので群に入れない"
        );
        assert_eq!(p.scatter.matured.len(), 1);
        assert_eq!(p.scatter.churned.len(), 2);
        // 有効3点では p95 の添字が最大値を指すので上限は効かず、軸外0件になる。
        // 実データ(2,535件)で124件が軸外になる挙動は
        // `散布図のx軸は95パーセンタイルで切り軸外件数を出す` で検証している。
        assert_eq!(p.scatter.x_axis.over_count, 0);
        assert!(!p.scatter.x_axis.title.contains("軸外"));
        assert_eq!(p.scatter.y_title, "契約継続月数 (createdate→今日)");
        assert!(!p.scatter.truncated);
        // 統計が空でも落ちず「―」で返る
        assert_eq!(p.stats.pearson.word, "データ不足");
        assert!(p.stats.status.contains("Pearson r=―"));
    }

    #[test]
    fn 上限で切ったらtruncatedが立つ() {
        // 約束3: 黙って上位N件にしない
        let mut rows: Vec<Vec<String>> = Vec::new();
        for i in 0..(NO_CALL_LIMIT + 5) {
            rows.push(vec![
                format!("{i}"),
                "X社".into(),
                "担当".into(),
                "2026-08-01".into(),
                format!("{}", i + 7),
                "True".into(),
            ]);
        }
        let refs: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|s| s.as_str()).collect())
            .collect();
        let slices: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        let d = sheet(
            &[
                "deal_id",
                "customer_label",
                "owner_name",
                "last_call_date",
                "days_since_last_call",
                "alert_7day_no_call",
            ],
            &slices,
        );
        let p = build_no_call(&d, &HashSet::new());
        assert_eq!(p.kpis.total, NO_CALL_LIMIT + 5, "総件数は削らず必ず出す");
        assert_eq!(p.shown, NO_CALL_LIMIT);
        assert!(p.truncated);
    }
}
