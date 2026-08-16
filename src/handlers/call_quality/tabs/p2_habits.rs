//! 架電クオリティ P2「習慣の差」タブ（GAS 版 `page-p2` の移植）
//!
//! 2026-08-16。GAS 版でこのタブは **2ページぶんが1画面に統合**されている
//! （2026-08-12 にユーザー判断で「プロセス(P5)」を「習慣の差(P2)」へ組込）。
//! そのため canvas 17個・見出し19個と、16タブ中で最大規模になっている。
//!
//! # 実装したパネル（GAS 版の canvas ID → 本ファイルの出力フィールド）
//!
//! | GAS canvas / 要素            | 出力フィールド            | 元シート            |
//! |------------------------------|---------------------------|---------------------|
//! | `p2-scorecards`              | `scorecards`              | 月次明細            |
//! | `chart-na-ontime`            | `rankings[na_ontime]`     | 月次明細            |
//! | `chart-na-done`              | `rankings[na_done]`       | 月次明細            |
//! | `chart-rs-ontime`            | `rankings[rs_ontime]`     | 月次明細            |
//! | `chart-rs-done`              | `rankings[rs_done]`       | 月次明細            |
//! | `chart-new-open`             | `rankings[owner_fluidity]`| 月次明細            |
//! | `chart-company-first-call`   | `rankings[new_pioneering]`| 月次明細            |
//! | `chart-scatter-na-apo`       | `scatters[na_apo]`        | 月次明細            |
//! | `chart-scatter-callpd-apo`   | `scatters[callpd_apo]`    | 月次明細            |
//! | `chart-p2-touch-dist`        | `touch_distribution`      | N回目架電分析       |
//! | `chart-p2-recycle-interval`  | `recycle_interval`        | リサイクル間隔      |
//! | `chart-p2-compliance-pattern`| `compliance`              | コンプライアンスパターン |
//! | `p5-scorecards`              | `process_scorecards`      | 月次明細            |
//! | `chart-stage-adv`            | `rankings[stage_advance]` | 月次明細            |
//! | `chart-scatter-stage-apo`    | `scatters[stage_apo]`     | 月次明細            |
//! | `chart-p5-sankey`            | `funnel`                  | ファネル4段        |
//! | `chart-p5-stage-dwell`       | `stage_dwell`             | 滞留日数            |
//! | `p5-on-the-spot-cards` / `chart-p5-on-the-spot` | `on_the_spot` | その場失注 owner月次 |
//! | `p5-trans-kpis` / `p5-trans-sankey` / `p5-trans-leadtime-table` | `stage_transition` | 商談遷移_集計 / 商談遷移_クロス |
//!
//! # 未実装（黙って省略していないので、必要なら追加すること）
//!
//! - **ベンチマークの「業界上位」破線**（GAS `INDUSTRY_BENCHMARKS`）。
//!   全社平均 / 上位25% は `benchmark` として返しているが、業界ベンチは
//!   GAS 側にハードコードされた定数（出典未記載）で、シートにも Python 側にも
//!   実体が無い。値の出所を確認できないまま移すと「根拠のある数字」に
//!   見えてしまうため、意図的に持ってきていない。
//! - **チャートの見た目に属する処理**（`adjustChartHeight` のバー高さ計算、
//!   Chart.js の色・Matrix コントローラ有無のフォールバック、Sankey ライブラリの
//!   ロード判定）。これらは描画側の責務で、サーバ集計には要らない。
//! - **選択メンバーの前後3名だけを強調する Contextual Positioning**
//!   （GAS `renderApoRank` にはあるが、P2 の `drawRateRank` には無い）。
//!   P2 のランキングは全員分返す仕様なので、強調は画面側で行う。
//! - **`chart-p2-compliance-pattern` の Matrix→表フォールバック**。表示形態の話。
//! - **その場失注の Deal 重複排除**。GAS 側コメントにも
//!   「Deal重複排除は server 側責務」とあるが、シート
//!   「その場失注 owner月次」は既に owner×月の集計値しか持たないため、
//!   このレイヤでは重複排除できない（Python `on_the_spot_lost.py` の責務）。
//!
//! # GAS 版と計算式が違う箇所（勝手に直さず、ここに全部書く）
//!
//! 1. **散布図の足切り**。GAS は X 軸の分母（`na_due` / `deal_touched`）に加えて
//!    `call_count >= 100`(HubSpot Call) で足切りするが、Y 軸のアポ率は
//!    `zoom_dial_count` を分母にしている。**足切りと率で別の分母を混ぜている**。
//!    2026-08-13 に `renderApoRank` は `den`(率と同じ分母)へ是正済みだが、
//!    散布図2枚と `chart-scatter-stage-apo` は取り残されていた。
//!    本実装は率と同じ分母（=`apo_denominator()`）で足切りする。
//! 2. **N回目架電・リサイクル間隔のアポ率**。GAS はシートの `apo_rate` 列
//!    （小数4桁に丸め済み）を件数で加重平均している。本実装は
//!    `SUM(apo_count) / SUM(total)` を直接計算する。丸め由来の差（最大 0.005pt）
//!    が消えるだけで、意味は同じ。**率の平均でなく分子総和÷分母総和**という
//!    プロジェクトの既存規律にも合う。
//! 3. **コンプライアンスパターンの期間フィルタ**。GAS はこのパネルだけ
//!    期間フィルタを無視して全期間を使う（同じ画面の他パネルは期間で絞る）。
//!    本実装は他パネルと同じ期間で絞る。GAS と同じ数字が欲しいときは
//!    `from`/`to` を空にすること。
//! 4. **コンプライアンスパターンの粒度**。GAS は「メンバー別」と説明しながら、
//!    実際にはシート行（owner × year_month）をそのまま1点として扱っており、
//!    同じ人が月数だけ重複して並ぶ。Z-score も行集合に対して計算される。
//!    計算は変えずに残したうえで、`year_month` を返して誤読を防ぐ。
//!    owner に畳むかどうかは指標の意味が変わるのでユーザー判断（未確認）。
//! 5. **その場失注のスコープ**。GAS はメンバー未選択時に**全ロール**を含める
//!    （BPO/コンサルが混ざる）。tabs/mod.rs の約束5に従い role=sales を既定にした。
//! 6. **ファネル / 滞留日数 / 商談遷移**。GAS はここにメンバー・PL・期間の
//!    フィルタを一切かけない（ファネルだけ独自の期間セレクタを持つ）。
//!    同じ挙動を保っている。かかっていない旨を `scope_note` で返す。
//! 7. **N回目架電の「20+」バケット**。シートの `attempt_no` 最終行は文字列 `"20+"`。
//!    GAS は `Number("20+")` が NaN → 0 になり `no >= 1` を満たさないので、
//!    この行を毎回黙って捨てていた（画面の 20+ は常に空）。実データでは
//!    `__all__` で 7,592架電 / 84アポ = 1.11% ある。ここでは拾う。
//! 8. **ファネルの単調クランプの見せ方**。GAS は「クランプしたか」を bool 1つで
//!    返す。実データ(2025-12以降)の生値は meeting=5,226 / opp=475 / won=6,364 で、
//!    opp が won の 1/13 という逆転のため **6段のうち3段が同値(6,364)に潰れる**。
//!    どこが実測でどこが引き上げた値かが bool では分からないので、
//!    `clamped_stages`(書き換えた段名) と `raw_stages`(生値) も返す。
//!    クランプそのものの計算は GAS と同じ。
//!    なお opp と won の母集団の食い違いは Python `funnel.py` 側の問題なので、
//!    ここでは直していない（表示で隠さないようにしただけ）。
//!
//! # 元データの注意
//!
//! - 土台シートは `月次明細`（Python `features.py` → `feature_monthly.csv`、
//!   owner × year_month × pipeline で 2,706行）。`sheets.rs` の `KNOWN_SHEETS` は
//!   GAS の `buildSheetResponse_` 経由の 69枚しか列挙していないため、
//!   `月次明細` / `メンバーマスタ` / `都道府県月次` はそこに載っていない。
//!   `SheetStore::get` はシート名を文字列で受けるので取得自体は問題ない。
//! - アポは 2026-08-13 に「**架電記録が無くても計上する**」定義へ変更済み。
//!   よって「HubSpot Call が 0 の担当者」を集計から落としてはいけない。

use std::cmp::Ordering;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_MONTHLY: &str = "月次明細";
const SHEET_PREFECTURE_MONTHLY: &str = "都道府県月次";
const SHEET_MEMBERS: &str = "メンバーマスタ";
const SHEET_TOUCH_DIST: &str = "N回目架電分析";
const SHEET_RECYCLE: &str = "リサイクル間隔";
const SHEET_COMPLIANCE: &str = "コンプライアンスパターン";
const SHEET_FUNNEL: &str = "ファネル4段";
const SHEET_STAGE_DWELL: &str = "滞留日数";
const SHEET_ON_THE_SPOT: &str = "その場失注 owner月次";
const SHEET_TRANS_SUMMARY: &str = "商談遷移_集計";
const SHEET_TRANS_CROSS: &str = "商談遷移_クロス";

// ---------------------------------------------------------------- 足切り閾値
//
// GAS 版 javascript.html の同名定数と一致させること。
// 少サンプルで率が跳ねる（例: 4架電1アポ=25%）のを防ぐための下限。

/// NA / 再架電の「期日到来」件数の下限
const MIN_DUE_FOR_RATE: f64 = 100.0;
/// タッチ案件 / ステージ到達 の下限
const MIN_TOUCHED_FOR_RATE: f64 = 50.0;
/// アポ率側の分母（通常は Zoom 発信数）の下限
const MIN_CALLS_FOR_RATE: f64 = 100.0;
/// 滞留日数で「信頼できる」とみなすサンプル数
const MIN_SAMPLES_FOR_DWELL: f64 = 10.0;

/// 上位N件で切るパネルの上限。切ったら必ず `truncated` を立てる。
const TOP_N_COMPLIANCE: usize = 30;
const TOP_N_DWELL: usize = 30;

/// ファネルの「架電」が取れ始める月。Zoom Phone 連携開始が 2025-12。
/// GAS `FUNNEL_DIAL_START_YM` と一致させること。
const FUNNEL_DIAL_START_YM: &str = "2025-12";

// ---------------------------------------------------------------- 入力

/// 画面上部のフィルタ。すべて省略可（省略 = 絞らない）。
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct P2Query {
    /// 期間開始 "YYYY-MM-DD"。月次明細の date は月初日固定なので実質 YYYY-MM。
    pub from: Option<String>,
    /// 期間終了 "YYYY-MM-DD"
    pub to: Option<String>,
    /// パイプライン。`__all__` または空で絞らない。
    pub pipeline: Option<String>,
    /// メンバー選択（カンマ区切り owner_id）。空なら role=sales を既定スコープにする。
    pub owners: Option<String>,
    /// 都道府県。`__all__` / 空以外を指定すると土台シートが「都道府県月次」に切り替わり、
    /// **アポ率の分母も Zoom 発信数 → HubSpot Call 数へ変わる**（都道府県月次に
    /// Zoom 行動量が無いため）。どちらで割ったかは `apo_denominator_label` で返す。
    pub prefecture: Option<String>,
    /// ファネルの期間セレクタ: `since-zoom`(既定) / `3m` / `6m` / `12m` / `all`
    pub funnel_period: Option<String>,
    /// 商談遷移のクロス絞込（業界 JSIC 大分類）。`__all__` / 空で絞らない。
    pub trans_industry: Option<String>,
    /// 商談遷移のクロス絞込（従業員規模）。`__all__` / 空で絞らない。
    pub trans_size: Option<String>,
    /// 「今月」を明示指定する（テスト用 / 監査用）。省略時は実行時のローカル日付。
    /// ファネルの `3m` / `6m` / `12m` の起点計算にだけ使う。
    pub today_ym: Option<String>,
}

// **`industry` / `size_band` はここに無い**。商談遷移のクロス絞込は
// `trans_industry` / `trans_size`。検証担当が `industry=製造業&size_band=…` と
// 書いて 200 が返り、KPI もペアも全部それらしい数値だったため
// 「業界: 製造業」と表示しながら全業界の数字を出していることに気づけなかった。
// 今はこの一覧に無い名前が `ignored_params` に出る。
crate::accepted_params!(P2Query, p2_query_accepted =>
    "from", "to", "pipeline", "owners", "prefecture",
    "funnel_period", "trans_industry", "trans_size", "today_ym");

impl P2Query {
    fn selected_owners(&self) -> Vec<String> {
        self.owners
            .as_deref()
            .unwrap_or("")
            .split(',')
            .map(|s| s.trim())
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect()
    }

    /// 都道府県モードか。true なら土台シートも分母も切り替わる。
    fn prefecture_mode(&self) -> Option<&str> {
        match self.prefecture.as_deref() {
            Some(p) if !p.is_empty() && p != "__all__" => Some(p),
            _ => None,
        }
    }

    fn pipeline_filter(&self) -> Option<&str> {
        match self.pipeline.as_deref() {
            Some(p) if !p.is_empty() && p != "__all__" => Some(p),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- 出力

/// 率カード1枚。**率は必ず Option**（分母0を 0% と読ませない）。
#[derive(Debug, Serialize, PartialEq)]
pub struct MetricCard {
    pub key: &'static str,
    pub label: &'static str,
    /// `percent` = %表示 / `ratio` = 倍率表示 / `count` = 実数
    pub kind: &'static str,
    /// percent なら 0-100、ratio なら分子÷分母、count なら実数。分母0は None。
    pub value: Option<f64>,
    pub numerator: f64,
    pub denominator: f64,
    pub note: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RankRow {
    pub owner_id: String,
    pub name: String,
    pub numerator: f64,
    pub denominator: f64,
    /// 率(%)。足切り済みなので通常 Some だが、型で 0% と区別できるようにしておく。
    pub value: Option<f64>,
    /// 1始まりの順位
    pub rank: usize,
}

/// 全社平均 / 上位25%。GAS `quantilesOf` と同じ定義（Q3 は `floor(n*0.75)` 番目）。
#[derive(Debug, Serialize, PartialEq)]
pub struct Benchmark {
    pub mean: Option<f64>,
    pub q3: Option<f64>,
    /// 足切りを通過した人数
    pub n: usize,
}

#[derive(Debug, Serialize)]
pub struct RateRanking {
    pub key: &'static str,
    pub title: &'static str,
    /// 分子・分母に使った列名（画面の説明文と突き合わせられるように返す）
    pub numerator_column: &'static str,
    pub denominator_column: &'static str,
    pub min_denominator: f64,
    pub rows: Vec<RankRow>,
    pub benchmark: Benchmark,
    /// 上位Nで切ったか。P2 は全員分表示なので通常 false。
    pub truncated: bool,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ScatterPoint {
    pub owner_id: String,
    pub name: String,
    pub x: f64,
    pub y: f64,
}

#[derive(Debug, Serialize)]
pub struct Scatter {
    pub key: &'static str,
    pub title: &'static str,
    pub x_label: &'static str,
    pub y_label: String,
    pub points: Vec<ScatterPoint>,
    /// 足切りの説明（何を満たした人だけが点になっているか）
    pub filter_note: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TouchBucket {
    /// "1".."19" / "20+"
    pub label: String,
    pub apo_count: f64,
    pub total: f64,
    /// アポ率(%)。分母0は None。
    pub apo_rate: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct TouchDistribution {
    /// 実際に使ったパイプライン行（PL選択が無ければ `__all__`）
    pub pipeline_used: String,
    /// 選択PLの行が無くて `__all__` に落ちたか
    pub fell_back_to_all: bool,
    pub buckets: Vec<TouchBucket>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct RecycleBucket {
    pub bucket: String,
    pub apo_next: f64,
    pub total_next: f64,
    pub apo_rate: Option<f64>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct ComplianceRow {
    pub owner_id: String,
    pub name: String,
    /// GAS は owner 別と称しつつ月別行をそのまま並べる。誤読防止に月を返す。
    pub year_month: String,
    /// 連続違反日数 max
    pub streak_max: f64,
    /// 連続違反日数 avg
    pub streak_avg: f64,
    /// 月末駆け込み率(%)
    pub month_end_rush: f64,
    /// 遵守傾向の悪化(drift)
    pub drift: f64,
    pub z_streak_max: f64,
    pub z_streak_avg: f64,
    pub z_month_end_rush: f64,
    pub z_drift: f64,
    /// 4指標のZ合計（並び順のキー）
    pub z_sum: f64,
}

#[derive(Debug, Serialize)]
pub struct CompliancePattern {
    pub rows: Vec<ComplianceRow>,
    /// 足切り前の対象行数
    pub total_rows: usize,
    pub truncated: bool,
    pub note: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FunnelStage {
    pub key: &'static str,
    pub label: &'static str,
    pub value: f64,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct FunnelFlow {
    pub from: &'static str,
    pub to: &'static str,
    pub flow: f64,
}

#[derive(Debug, Serialize)]
pub struct Funnel {
    /// 実際に使った期間（`all` なら下限なし）
    pub since: String,
    pub until: String,
    pub stages: Vec<FunnelStage>,
    pub flows: Vec<FunnelFlow>,
    /// 単調クランプ（上流段が下流段を下回ったので引き上げた）が起きたか
    pub clamped: bool,
    /// クランプで**値を書き換えた段**。実データ(2025-12以降)では
    /// 生値 meeting=5,226 / opp=475 / won=6,364 で、opp が won の 1/13 という
    /// 逆転が起きており、meeting と opp が won に引き上げられて 3段が同値になる。
    /// bool 1つだと「どこが実測でどこが作り物か」が画面から分からないので段名で返す。
    pub clamped_stages: Vec<&'static str>,
    /// クランプ前の生値（実測はこちら。画面で並べて出せるようにする）
    pub raw_stages: Vec<FunnelStage>,
    /// 期間内に架電(dial)の実績行があったか。false なら「架電が取れない期間」
    pub dial_available: bool,
    pub scope_note: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct DwellRow {
    pub label: String,
    pub pipeline: String,
    pub stage: String,
    pub median_days: f64,
    pub p25_days: Option<f64>,
    pub p75_days: Option<f64>,
    pub n_samples: Option<f64>,
    /// ボトルネックZ。シートに `bottleneck_z` が無ければ median から自前計算する。
    pub z: Option<f64>,
    /// n < 10。画面で薄色にするための旗（黙って混ぜない）
    pub low_confidence: bool,
}

#[derive(Debug, Serialize)]
pub struct StageDwell {
    pub rows: Vec<DwellRow>,
    pub total_rows: usize,
    pub truncated: bool,
    /// シートに bottleneck_z が無く、median から自前計算したか
    pub z_self_computed: bool,
    pub scope_note: &'static str,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct OnTheSpotRow {
    pub owner_id: String,
    pub name: String,
    pub count: f64,
}

#[derive(Debug, Serialize)]
pub struct OnTheSpot {
    pub total_count: f64,
    pub owner_count: usize,
    /// 発生 owner あたり平均。owner 0 名なら None。
    pub avg_per_owner: Option<f64>,
    pub rows: Vec<OnTheSpotRow>,
    pub from_ym: String,
    pub to_ym: String,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TransitionPair {
    pub stage_from: String,
    pub stage_to: String,
    pub from_label: String,
    pub to_label: String,
    pub count: f64,
    /// from 内シェア(%)。クロス絞込時は絞込後で再計算する。
    pub share_within_from: Option<f64>,
    pub lead_time_p25: Option<f64>,
    pub lead_time_p50: Option<f64>,
    pub lead_time_p75: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct StageTransition {
    /// クロス絞込が効いているか（効いていれば p25/p75 は取れない）
    pub cross_active: bool,
    pub industry: String,
    pub size_band: String,
    pub kpis: Vec<MetricCard>,
    pub pairs: Vec<TransitionPair>,
    pub total_transitions: f64,
    /// 主要ステージ以外を落とす前の遷移ペア数
    pub pairs_before_filter: usize,
    /// セレクタ用の候補一覧（並びは安定）
    pub industries: Vec<String>,
    pub size_bands: Vec<String>,
    pub scope_note: &'static str,
}

#[derive(Debug, Serialize)]
pub struct P2HabitsData {
    /// 期間・PL・メンバー/ロールの適用結果（何を見ているかを画面に出すため）
    pub scope: ScopeInfo,
    pub scorecards: Vec<MetricCard>,
    pub rankings: Vec<RateRanking>,
    pub scatters: Vec<Scatter>,
    pub touch_distribution: TouchDistribution,
    pub recycle_interval: Vec<RecycleBucket>,
    pub compliance: CompliancePattern,
    pub process_scorecards: Vec<MetricCard>,
    pub funnel: Funnel,
    pub stage_dwell: StageDwell,
    pub on_the_spot: OnTheSpot,
    pub stage_transition: StageTransition,
}

/// **実際に効いたフィルタを全部返す**、が原則。
///
/// 2026-08-17 拡張。それまでこの構造体は `from`/`to`/`pipeline`/`prefecture` の
/// 4つしか返しておらず、`trans_industry` / `trans_size` / `funnel_period` /
/// `today_ym` は「送ったが応答に痕跡が無い」状態だった。
/// クロス絞込の取り違え（`industry` / `size_band` と書いた事故）に気づけたのは
/// `stage_transition.cross_active: false` が返っていたおかげ。**あれを他の
/// フィルタにも広げたのがこの4フィールド**。
///
/// 値は「送られてきた生の文字列」ではなく **実際に効いた値**を返す。
/// 例: `funnel_period` は未指定でも `since-zoom` と返す（既定が効いている、が事実）。
/// 生の入力そのものは `TabPayload::ignored_params` と合わせて見れば復元できる。
#[derive(Debug, Serialize)]
pub struct ScopeInfo {
    pub from: String,
    pub to: String,
    pub pipeline: String,
    pub prefecture: String,
    /// メンバー個別選択があったか。無ければ role=sales が既定スコープ。
    pub member_selected: bool,
    pub owner_count: usize,
    /// 「Zoom発信数が分母」/「HubSpot Call数が分母」。単独表記(ただの「アポ率」)は
    /// プロジェクト規約で禁止なので、必ず画面に添えること。
    pub apo_denominator_label: &'static str,
    pub matched_rows: usize,
    /// 商談遷移のクロス絞込（業界 JSIC 大分類）。絞っていなければ `__all__`。
    /// **`stage_transition` にしか効かない**。上段の KPI・ランキング・散布図は
    /// この値に関係なく全件のまま（GAS 版と同じ挙動）。
    pub trans_industry: String,
    /// 商談遷移のクロス絞込（従業員規模）。絞っていなければ `__all__`。同上。
    pub trans_size: String,
    /// ファネルの期間セレクタ。未指定なら既定の `since-zoom` を返す
    /// （「指定しなかった」ではなく「since-zoom が効いた」が事実）。
    /// **`funnel` にしか効かない**。
    pub funnel_period: String,
    /// ファネルの相対期間（3m/6m/12m）の起点に使った「当月」。
    /// 明示指定が無ければ実行時の JST 当月。`funnel_period` が
    /// `since-zoom` / `all` のときは起点計算に使われない。
    pub funnel_today_ym: String,
}

// ---------------------------------------------------------------- 小道具

/// 数値化。空文字・非数値は 0.0。カンマ区切りも受ける。
fn num(s: &str) -> f64 {
    let t = s.trim();
    if t.is_empty() {
        return 0.0;
    }
    t.replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 空文字を None にして数値化（「値が無い」と「0」を区別する列で使う）。
fn num_opt(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    t.replace(',', "").parse::<f64>().ok()
}

fn cell<'a>(row: &'a [Arc<str>], idx: Option<usize>) -> &'a str {
    idx.and_then(|i| row.get(i)).map(|s| s.as_ref()).unwrap_or("")
}

fn numv(row: &[Arc<str>], idx: Option<usize>) -> f64 {
    num(cell(row, idx))
}

/// 列名の候補を順に試して最初に見つかった添字を返す。
/// Python 側のスキーマ揺れ（`n_samples` / `count` / `sample_size` / `n` 等）を吸収する。
fn col_any(d: &SheetData, names: &[&str]) -> Option<usize> {
    names.iter().find_map(|n| d.col(n))
}

/// 降順比較。NaN は「等しい」に落として並びが壊れないようにする。
fn desc(a: f64, b: f64) -> Ordering {
    b.partial_cmp(&a).unwrap_or(Ordering::Equal)
}

/// 分子÷分母（%にしない倍率）。分母0は None。
fn ratio(numerator: f64, denominator: f64) -> Option<f64> {
    if denominator > 0.0 {
        Some(numerator / denominator)
    } else {
        None
    }
}

/// 母集団標準偏差での Z-score。GAS `zscores` と同じ（n で割る。n-1 ではない）。
/// sd=0（全員同値）のときは全員0を返す。
fn zscores(values: &[f64]) -> Vec<f64> {
    if values.is_empty() {
        return Vec::new();
    }
    let n = values.len() as f64;
    let mean = values.iter().sum::<f64>() / n;
    let var = values.iter().map(|v| (v - mean) * (v - mean)).sum::<f64>() / n;
    let sd = var.sqrt();
    if sd > 0.0 {
        values.iter().map(|v| (v - mean) / sd).collect()
    } else {
        vec![0.0; values.len()]
    }
}

/// 率の配列から 平均 / 上位25%(Q3) / 人数。GAS `quantilesOf` と同じ定義。
fn quantiles(rates: &[f64]) -> Benchmark {
    if rates.is_empty() {
        return Benchmark {
            mean: None,
            q3: None,
            n: 0,
        };
    }
    let mean = rates.iter().sum::<f64>() / rates.len() as f64;
    let mut sorted = rates.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
    let mut idx = (sorted.len() as f64 * 0.75).floor() as usize;
    if idx >= sorted.len() {
        idx = sorted.len() - 1;
    }
    Benchmark {
        mean: Some(mean),
        q3: Some(sorted[idx]),
        n: rates.len(),
    }
}

// ---------------------------------------------------------------- メンバー

#[derive(Debug, Clone)]
struct Member {
    name: String,
    role: String,
}

/// 「メンバーマスタ」から owner_id → (名前, role)。
/// role は sales / consultant / bpo / other（実データ 402名: other 290 / bpo 41 / sales 40 / consultant 31）。
fn load_members(d: &SheetData) -> HashMap<String, Member> {
    let c_id = d.col("owner_id");
    let c_name = d.col("name");
    let c_role = d.col("role");
    let mut out = HashMap::new();
    for row in &d.rows {
        let id = cell(row, c_id).trim().to_string();
        if id.is_empty() {
            continue;
        }
        let name = cell(row, c_name).trim().to_string();
        let role = cell(row, c_role).trim().to_string();
        out.insert(
            id.clone(),
            Member {
                name: if name.is_empty() { id } else { name },
                role: if role.is_empty() {
                    "other".to_string()
                } else {
                    role
                },
            },
        );
    }
    out
}

fn member_name(members: &HashMap<String, Member>, id: &str) -> String {
    members
        .get(id)
        .map(|m| m.name.clone())
        .unwrap_or_else(|| id.to_string())
}

// ---------------------------------------------------------------- 月次明細

/// 月次明細（= Python `feature_monthly.csv`）1行ぶんの、このタブが使う値。
///
/// 列は**すべて名前で引く**。`features.py` 側で列が増減しても位置ずれで壊れない。
#[derive(Debug, Default, Clone)]
struct MonthlyRow {
    owner_id: String,
    // year_month / pipeline は集計値ではないので owner 集約後は使わない。
    // 絞り込みの検証（どの月・どのPLが残ったか）で読むので保持しておく。
    #[allow(dead_code)]
    year_month: String,
    #[allow(dead_code)]
    pipeline: String,
    call_count: f64,
    zoom_dial_count: f64,
    apo_count: f64,
    apo_count_first: f64,
    apo_count_repeat: f64,
    deal_touched: f64,
    new_open_deals: f64,
    company_first_call_count: f64,
    high_call_deals: f64,
    stage_advanced: f64,
    stage_entered: f64,
    na_due: f64,
    na_done_ontime: f64,
    na_done_total: f64,
    rs_due: f64,
    rs_done_ontime: f64,
    rs_done_total: f64,
}

/// 月次明細の列添字をまとめて1回だけ解決する。
/// （`SheetData::get` は呼ぶたびにヘッダを線形探索するので、行ループの中で使わない）
struct MonthlyCols {
    owner_id: Option<usize>,
    year_month: Option<usize>,
    pipeline: Option<usize>,
    prefecture: Option<usize>,
    call_count: Option<usize>,
    zoom_dial_count: Option<usize>,
    apo_count: Option<usize>,
    apo_count_first: Option<usize>,
    apo_count_repeat: Option<usize>,
    deal_touched: Option<usize>,
    new_open_deals: Option<usize>,
    company_first_call_count: Option<usize>,
    high_call_deals: Option<usize>,
    stage_advanced: Option<usize>,
    stage_entered: Option<usize>,
    na_due: Option<usize>,
    na_done_ontime: Option<usize>,
    na_done_total: Option<usize>,
    rs_due: Option<usize>,
    rs_done_ontime: Option<usize>,
    rs_done_total: Option<usize>,
}

impl MonthlyCols {
    fn resolve(d: &SheetData) -> Self {
        Self {
            owner_id: d.col("owner_id"),
            year_month: d.col("year_month"),
            pipeline: d.col("pipeline"),
            prefecture: d.col("prefecture"),
            call_count: d.col("call_count"),
            zoom_dial_count: d.col("zoom_dial_count"),
            apo_count: d.col("apo_count"),
            apo_count_first: d.col("apo_count_first"),
            apo_count_repeat: d.col("apo_count_repeat"),
            deal_touched: d.col("deal_touched"),
            new_open_deals: d.col("new_open_deals"),
            company_first_call_count: d.col("company_first_call_count"),
            high_call_deals: d.col("high_call_deals"),
            stage_advanced: d.col("stage_advanced"),
            stage_entered: d.col("stage_entered"),
            na_due: d.col("na_due"),
            na_done_ontime: d.col("na_done_ontime"),
            na_done_total: d.col("na_done_total"),
            rs_due: d.col("rs_due"),
            rs_done_ontime: d.col("rs_done_ontime"),
            rs_done_total: d.col("rs_done_total"),
        }
    }
}

/// 期間・PL・都道府県・メンバー/ロールで絞り、`MonthlyRow` に落とす。
///
/// GAS の `filteredRows()` → `scopeSalesRows()` に相当。順序も同じ:
///   1. 期間 (date = `year_month` + "-01" で比較。GAS も月初日固定の値と比較している)
///   2. パイプライン
///   3. 都道府県（都道府県モード時のみ。土台シート自体が別）
///   4. メンバー選択 → あればその人だけ / 無ければ **role=sales のみ**
fn scope_monthly(
    d: &SheetData,
    members: &HashMap<String, Member>,
    q: &P2Query,
) -> Vec<MonthlyRow> {
    let c = MonthlyCols::resolve(d);
    let selected = q.selected_owners();
    let sel: Option<Vec<String>> = if selected.is_empty() {
        None
    } else {
        Some(selected)
    };
    let pref = q.prefecture_mode();
    let pipeline = q.pipeline_filter();
    let from = q.from.as_deref().unwrap_or("");
    let to = q.to.as_deref().unwrap_or("");

    let mut out = Vec::new();
    for row in &d.rows {
        let ym = cell(row, c.year_month).trim();
        if ym.is_empty() {
            continue;
        }
        // GAS は "YYYY-MM-01" 形式の date 列を作って from/to と文字列比較する。
        // シートには date 列が無い（Code.gs が実行時に足している）ので、ここで作る。
        let date = format!("{}-01", &ym[..ym.len().min(7)]);
        if !from.is_empty() && date.as_str() < from {
            continue;
        }
        if !to.is_empty() && date.as_str() > to {
            continue;
        }
        if let Some(p) = pipeline {
            if cell(row, c.pipeline) != p {
                continue;
            }
        }
        if let Some(pv) = pref {
            if cell(row, c.prefecture) != pv {
                continue;
            }
        }
        let owner_id = cell(row, c.owner_id).trim().to_string();
        match sel.as_ref() {
            Some(ids) => {
                if !ids.iter().any(|x| x == &owner_id) {
                    continue;
                }
            }
            None => {
                // 約束5: メンバー未選択時の既定は role=sales。
                // ここを緩めると BPO/コンサルが混ざってアポ率が希釈される
                // （GAS 側で 141名混在 → 0.93%→0.63% になった事故がある）。
                let is_sales = members
                    .get(&owner_id)
                    .map(|m| m.role == "sales")
                    .unwrap_or(false);
                if !is_sales {
                    continue;
                }
            }
        }

        out.push(MonthlyRow {
            owner_id,
            year_month: ym[..ym.len().min(7)].to_string(),
            pipeline: cell(row, c.pipeline).to_string(),
            call_count: numv(row, c.call_count),
            zoom_dial_count: numv(row, c.zoom_dial_count),
            apo_count: numv(row, c.apo_count),
            apo_count_first: numv(row, c.apo_count_first),
            apo_count_repeat: numv(row, c.apo_count_repeat),
            deal_touched: numv(row, c.deal_touched),
            new_open_deals: numv(row, c.new_open_deals),
            company_first_call_count: numv(row, c.company_first_call_count),
            high_call_deals: numv(row, c.high_call_deals),
            stage_advanced: numv(row, c.stage_advanced),
            stage_entered: numv(row, c.stage_entered),
            na_due: numv(row, c.na_due),
            na_done_ontime: numv(row, c.na_done_ontime),
            na_done_total: numv(row, c.na_done_total),
            rs_due: numv(row, c.rs_due),
            rs_done_ontime: numv(row, c.rs_done_ontime),
            rs_done_total: numv(row, c.rs_done_total),
        });
    }
    out
}

/// owner ごとの合算。GAS `sumByOwner` に相当。
/// キーは owner_id、値は `MonthlyRow` の数値部を足しあげたもの。
fn sum_by_owner(rows: &[MonthlyRow]) -> Vec<MonthlyRow> {
    let mut acc: HashMap<String, MonthlyRow> = HashMap::new();
    for r in rows {
        let e = acc.entry(r.owner_id.clone()).or_insert_with(|| MonthlyRow {
            owner_id: r.owner_id.clone(),
            ..Default::default()
        });
        e.call_count += r.call_count;
        e.zoom_dial_count += r.zoom_dial_count;
        e.apo_count += r.apo_count;
        e.apo_count_first += r.apo_count_first;
        e.apo_count_repeat += r.apo_count_repeat;
        e.deal_touched += r.deal_touched;
        e.new_open_deals += r.new_open_deals;
        e.company_first_call_count += r.company_first_call_count;
        e.high_call_deals += r.high_call_deals;
        e.stage_advanced += r.stage_advanced;
        e.stage_entered += r.stage_entered;
        e.na_due += r.na_due;
        e.na_done_ontime += r.na_done_ontime;
        e.na_done_total += r.na_done_total;
        e.rs_due += r.rs_due;
        e.rs_done_ontime += r.rs_done_ontime;
        e.rs_done_total += r.rs_done_total;
    }
    let mut v: Vec<MonthlyRow> = acc.into_values().collect();
    // HashMap の反復順をそのまま返さない（約束4）
    v.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    v
}

/// 全行の合算。GAS `totalsOf` に相当。
fn totals_of(rows: &[MonthlyRow]) -> MonthlyRow {
    let mut t = MonthlyRow::default();
    for r in rows {
        t.call_count += r.call_count;
        t.zoom_dial_count += r.zoom_dial_count;
        t.apo_count += r.apo_count;
        t.apo_count_first += r.apo_count_first;
        t.apo_count_repeat += r.apo_count_repeat;
        t.deal_touched += r.deal_touched;
        t.new_open_deals += r.new_open_deals;
        t.company_first_call_count += r.company_first_call_count;
        t.high_call_deals += r.high_call_deals;
        t.stage_advanced += r.stage_advanced;
        t.stage_entered += r.stage_entered;
        t.na_due += r.na_due;
        t.na_done_ontime += r.na_done_ontime;
        t.na_done_total += r.na_done_total;
        t.rs_due += r.rs_due;
        t.rs_done_ontime += r.rs_done_ontime;
        t.rs_done_total += r.rs_done_total;
    }
    t
}

/// アポ率の分母。定義上は Zoom 行動量（`feedback_apo_rate_definition`）。
/// 都道府県モードのときだけ、都道府県月次に Zoom 行動量が無いので HubSpot Call に落ちる。
///
/// **どちらで割ったかは必ず画面に出すこと**（ただの「アポ率」という単独表記は規約で禁止）。
fn apo_denominator(row: &MonthlyRow, pref_mode: bool) -> f64 {
    if pref_mode {
        row.call_count
    } else if row.zoom_dial_count > 0.0 {
        row.zoom_dial_count
    } else {
        row.call_count
    }
}

fn apo_denominator_label(pref_mode: bool) -> &'static str {
    if pref_mode {
        "HubSpot Call数が分母（都道府県別データには Zoom 行動量が無いため）"
    } else {
        "Zoom発信数が分母"
    }
}

// ---------------------------------------------------------------- パネル: スコアカード

/// P2 スコアカード7枚。GAS `renderP2Scorecards`。
pub fn build_scorecards(t: &MonthlyRow) -> Vec<MetricCard> {
    vec![
        MetricCard {
            key: "na_ontime",
            label: "NA遵守率",
            kind: "percent",
            value: rate(t.na_done_ontime, t.na_due),
            numerator: t.na_done_ontime,
            denominator: t.na_due,
            note: "期日内NA消化 ÷ NA期日到来",
        },
        MetricCard {
            key: "na_done",
            label: "NA消化率",
            kind: "percent",
            value: rate(t.na_done_total, t.na_due),
            numerator: t.na_done_total,
            denominator: t.na_due,
            note: "NA消化合計 ÷ NA期日到来（遅れも含む）",
        },
        MetricCard {
            key: "rs_ontime",
            label: "再架電日 遵守率",
            kind: "percent",
            value: rate(t.rs_done_ontime, t.rs_due),
            numerator: t.rs_done_ontime,
            denominator: t.rs_due,
            note: "期日内rs消化 ÷ rs期日到来",
        },
        MetricCard {
            key: "rs_done",
            label: "再架電日 消化率",
            kind: "percent",
            value: rate(t.rs_done_total, t.rs_due),
            numerator: t.rs_done_total,
            denominator: t.rs_due,
            note: "rs消化合計 ÷ rs期日到来（遅れも含む）",
        },
        MetricCard {
            key: "owner_fluidity",
            label: "担当流動率",
            kind: "percent",
            value: rate(t.new_open_deals, t.deal_touched),
            numerator: t.new_open_deals,
            denominator: t.deal_touched,
            // 旧称「新規開拓率」。2026-05-25 に rename。会社視点の新規開拓ではない。
            note: "自分にとっての初架電率（新規オープンDeal ÷ タッチ案件）",
        },
        MetricCard {
            key: "new_pioneering",
            label: "新規開拓率（全社初架電率）",
            kind: "percent",
            value: rate(t.company_first_call_count, t.deal_touched),
            numerator: t.company_first_call_count,
            denominator: t.deal_touched,
            note: "全社で初めてその Deal に架電した割合（全社初架電 ÷ タッチ案件）",
        },
        MetricCard {
            key: "calls_per_deal",
            label: "1案件あたり架電",
            kind: "ratio",
            value: ratio(t.call_count, t.deal_touched),
            numerator: t.call_count,
            denominator: t.deal_touched,
            note: "架電数 ÷ タッチ案件",
        },
    ]
}

/// プロセス（旧P5）スコアカード4枚。GAS `renderP5Scorecards`。
pub fn build_process_scorecards(t: &MonthlyRow) -> Vec<MetricCard> {
    vec![
        MetricCard {
            key: "stage_advance",
            label: "ステージ移行率",
            kind: "percent",
            value: rate(t.stage_advanced, t.stage_entered),
            numerator: t.stage_advanced,
            denominator: t.stage_entered,
            note: "前進 ÷ 到達",
        },
        MetricCard {
            key: "stage_entered",
            label: "ステージ到達数",
            kind: "count",
            value: Some(t.stage_entered),
            numerator: t.stage_entered,
            denominator: 0.0,
            note: "対象期間合計",
        },
        MetricCard {
            key: "stage_advanced",
            label: "ステージ前進数",
            kind: "count",
            value: Some(t.stage_advanced),
            numerator: t.stage_advanced,
            denominator: 0.0,
            note: "対象期間合計",
        },
        MetricCard {
            key: "high_call_deals",
            label: "高架電Deal総数",
            kind: "count",
            value: Some(t.high_call_deals),
            numerator: t.high_call_deals,
            denominator: 0.0,
            note: "深掘りされたDeal数",
        },
    ]
}

// ---------------------------------------------------------------- パネル: 率ランキング

/// 率系ランキング1枚。GAS `drawRateRank`。
///
/// - `min_den` 未満のメンバーは**除外**（少サンプルで率が跳ねるため）
/// - 率降順。同率は owner_id 昇順で固定（毎回同じ並びで返す）
/// - GAS 版は Top25 制限を 2026-05-25 に撤廃して全員分表示。ここも全員返す。
pub fn build_rate_ranking(
    key: &'static str,
    title: &'static str,
    num_col: &'static str,
    den_col: &'static str,
    min_den: f64,
    owners: &[MonthlyRow],
    members: &HashMap<String, Member>,
    pick: impl Fn(&MonthlyRow) -> (f64, f64),
) -> RateRanking {
    let mut rows: Vec<RankRow> = owners
        .iter()
        .filter_map(|o| {
            let (n, d) = pick(o);
            if d < min_den {
                return None;
            }
            Some(RankRow {
                owner_id: o.owner_id.clone(),
                name: member_name(members, &o.owner_id),
                numerator: n,
                denominator: d,
                value: rate(n, d),
                rank: 0,
            })
        })
        .collect();

    let rates: Vec<f64> = rows.iter().filter_map(|r| r.value).collect();
    let benchmark = quantiles(&rates);

    rows.sort_by(|a, b| {
        desc(a.value.unwrap_or(f64::MIN), b.value.unwrap_or(f64::MIN))
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    for (i, r) in rows.iter_mut().enumerate() {
        r.rank = i + 1;
    }

    RateRanking {
        key,
        title,
        numerator_column: num_col,
        denominator_column: den_col,
        min_denominator: min_den,
        rows,
        benchmark,
        // 全員分返すので切っていない。切るようにしたらここを true にすること。
        truncated: false,
    }
}

pub fn build_rankings(owners: &[MonthlyRow], members: &HashMap<String, Member>) -> Vec<RateRanking> {
    vec![
        build_rate_ranking(
            "na_ontime",
            "NA期日 遵守率",
            "na_done_ontime",
            "na_due",
            MIN_DUE_FOR_RATE,
            owners,
            members,
            |o| (o.na_done_ontime, o.na_due),
        ),
        build_rate_ranking(
            "na_done",
            "NA期日 消化率",
            "na_done_total",
            "na_due",
            MIN_DUE_FOR_RATE,
            owners,
            members,
            |o| (o.na_done_total, o.na_due),
        ),
        build_rate_ranking(
            "rs_ontime",
            "再架電日 遵守率",
            "rs_done_ontime",
            "rs_due",
            MIN_DUE_FOR_RATE,
            owners,
            members,
            |o| (o.rs_done_ontime, o.rs_due),
        ),
        build_rate_ranking(
            "rs_done",
            "再架電日 消化率",
            "rs_done_total",
            "rs_due",
            MIN_DUE_FOR_RATE,
            owners,
            members,
            |o| (o.rs_done_total, o.rs_due),
        ),
        build_rate_ranking(
            "owner_fluidity",
            "担当流動率（旧: 新規開拓率）",
            "new_open_deals",
            "deal_touched",
            MIN_TOUCHED_FOR_RATE,
            owners,
            members,
            |o| (o.new_open_deals, o.deal_touched),
        ),
        build_rate_ranking(
            "new_pioneering",
            "新規開拓率（全社初架電率）",
            "company_first_call_count",
            "deal_touched",
            MIN_TOUCHED_FOR_RATE,
            owners,
            members,
            |o| (o.company_first_call_count, o.deal_touched),
        ),
        // プロセス（旧P5）側。同じ形なのでここに並べる。
        build_rate_ranking(
            "stage_advance",
            "ステージ移行率（前進 ÷ 到達）",
            "stage_advanced",
            "stage_entered",
            MIN_TOUCHED_FOR_RATE,
            owners,
            members,
            |o| (o.stage_advanced, o.stage_entered),
        ),
    ]
}

// ---------------------------------------------------------------- パネル: 散布図

/// 散布図3枚。GAS `renderP2Scatters` / `renderP5Scatter`。
///
/// **GAS との差（意図的）**: GAS は足切りを `call_count >= 100`(HubSpot Call) で
/// 行いながら、Y 軸のアポ率は Zoom 発信を分母にしている。分母が混ざっているので、
/// Zoom 発信の少ない担当者が足切りを通過して不安定な率のまま点になる。
/// ここでは **率と同じ分母**（`apo_denominator`）で足切りする。
pub fn build_scatters(
    owners: &[MonthlyRow],
    members: &HashMap<String, Member>,
    pref_mode: bool,
) -> Vec<Scatter> {
    let y_label = format!("アポ率(%) ※{}", apo_denominator_label(pref_mode));
    let cut_note = format!(
        "アポ率の分母（{}）が {} 件以上",
        apo_denominator_label(pref_mode),
        MIN_CALLS_FOR_RATE as i64
    );

    let mut na_apo = Vec::new();
    let mut callpd_apo = Vec::new();
    let mut stage_apo = Vec::new();

    for o in owners {
        let den = apo_denominator(o, pref_mode);
        if den < MIN_CALLS_FOR_RATE {
            continue;
        }
        let y = match rate(o.apo_count, den) {
            Some(v) => v,
            None => continue,
        };
        let name = member_name(members, &o.owner_id);

        if o.na_due >= MIN_DUE_FOR_RATE {
            if let Some(x) = rate(o.na_done_ontime, o.na_due) {
                na_apo.push(ScatterPoint {
                    owner_id: o.owner_id.clone(),
                    name: name.clone(),
                    x,
                    y,
                });
            }
        }
        if o.deal_touched >= MIN_TOUCHED_FOR_RATE {
            if let Some(x) = ratio(o.call_count, o.deal_touched) {
                callpd_apo.push(ScatterPoint {
                    owner_id: o.owner_id.clone(),
                    name: name.clone(),
                    x,
                    y,
                });
            }
        }
        if o.stage_entered >= MIN_TOUCHED_FOR_RATE {
            if let Some(x) = rate(o.stage_advanced, o.stage_entered) {
                stage_apo.push(ScatterPoint {
                    owner_id: o.owner_id.clone(),
                    name,
                    x,
                    y,
                });
            }
        }
    }

    // 並びを安定させる（点の順序が変わると凡例や色割当がぶれる）
    for v in [&mut na_apo, &mut callpd_apo, &mut stage_apo] {
        v.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    }

    vec![
        Scatter {
            key: "na_apo",
            title: "NA遵守率 × アポ率",
            x_label: "NA遵守率(%)",
            y_label: y_label.clone(),
            points: na_apo,
            filter_note: format!("NA期日到来 {} 件以上 かつ {}", MIN_DUE_FOR_RATE as i64, cut_note),
        },
        Scatter {
            key: "callpd_apo",
            title: "1案件あたり架電 × アポ率",
            x_label: "1案件あたり架電数",
            y_label: y_label.clone(),
            points: callpd_apo,
            filter_note: format!(
                "タッチ案件 {} 件以上 かつ {}",
                MIN_TOUCHED_FOR_RATE as i64, cut_note
            ),
        },
        Scatter {
            key: "stage_apo",
            title: "ステージ移行率 × アポ率",
            x_label: "ステージ移行率(%)",
            y_label,
            points: stage_apo,
            filter_note: format!(
                "ステージ到達 {} 件以上 かつ {}",
                MIN_TOUCHED_FOR_RATE as i64, cut_note
            ),
        },
    ]
}

// ---------------------------------------------------------------- パネル: N回目架電

/// N回目架電分析。GAS `_drawP2TouchDistFromData`。
///
/// シート「N回目架電分析」は `attempt_no, pipeline, total, connect_count,
/// apo_count, connect_rate, apo_rate`。PL 行（`__all__` + 実PL3種）を持つ。
///
/// - PL 選択があればその行、無ければ `__all__` 行。
/// - 選択PLの行が1本も無ければ `__all__` に落として `fell_back_to_all` を立てる
///   （黙って空グラフにしない）。
/// - attempt_no は 1..19 と 20+ に畳む。**シート側は既に "20+" という文字列行を
///   持っている**（`__all__` で 7,592架電 / 84アポ = 1.11%）。
///   GAS は `Math.round(num("20+"))` → `Number("20+")` が NaN → 0 になり
///   `no >= 1` を満たさず、この行を毎回黙って捨てていた。
///   結果、画面の「20+」バケットは常に空。ここでは文字列 "20+" も受けて拾う。
/// - **接続率(Call記録率)は出さない**。2026-05-26 に「実態が Zoom→HubSpot 突合率で
///   誤解を生む」として GAS から削除済み。移植でも復活させない。
pub fn build_touch_distribution(d: &SheetData, pipeline: Option<&str>) -> TouchDistribution {
    let c_attempt = d.col("attempt_no");
    let c_pipeline = d.col("pipeline");
    let c_total = col_any(d, &["total", "count"]);
    let c_apo = d.col("apo_count");

    let target = pipeline.unwrap_or("__all__");
    let has_target = c_pipeline.is_some()
        && d.rows.iter().any(|r| cell(r, c_pipeline) == target);
    let (used, fell_back) = if has_target {
        (target.to_string(), false)
    } else {
        ("__all__".to_string(), pipeline.is_some())
    };

    // 1..19 + "20+" の 20 バケット固定
    let mut acc: Vec<(f64, f64)> = vec![(0.0, 0.0); 20];
    for row in &d.rows {
        if c_pipeline.is_some() && cell(row, c_pipeline) != used {
            continue;
        }
        // "20+" のような数値でない表記も受ける（GAS はここで捨てていた）
        let raw = cell(row, c_attempt).trim();
        let idx = if raw.ends_with('+') {
            match raw.trim_end_matches('+').parse::<f64>() {
                Ok(v) if v >= 20.0 => 19,
                Ok(v) if v >= 1.0 => (v as usize) - 1,
                _ => continue,
            }
        } else {
            let no = num(raw).round();
            if no < 1.0 {
                continue;
            }
            if no >= 20.0 {
                19
            } else {
                (no as usize) - 1
            }
        };
        acc[idx].0 += numv(row, c_apo);
        acc[idx].1 += numv(row, c_total);
    }

    let buckets = acc
        .iter()
        .enumerate()
        .map(|(i, (apo, total))| TouchBucket {
            label: if i == 19 {
                "20+".to_string()
            } else {
                (i + 1).to_string()
            },
            apo_count: *apo,
            total: *total,
            // 分母0（そのN回目に到達した Deal が無い）は 0% でなく null
            apo_rate: rate(*apo, *total),
        })
        .collect();

    TouchDistribution {
        pipeline_used: used,
        fell_back_to_all: fell_back,
        buckets,
    }
}

// ---------------------------------------------------------------- パネル: リサイクル間隔

/// バケットの並び。シート側は「1-3日」のように末尾に「日」が付く。
const RECYCLE_ORDER: &[&str] = &["1-3", "4-7", "8-14", "15-30", "31-60", "61+"];

/// リサイクル間隔別アポ率。GAS `_drawP2RecycleIntervalFromData`。
///
/// シート「リサイクル間隔」は `interval_bucket, total_next, apo_next, apo_rate`。
/// **順序はシートの並びに依存させず `RECYCLE_ORDER` で固定**する
/// （1-3 / 4-7 / … / 61+ の順でないと「短い間隔ほど良いのか」が読めない）。
pub fn build_recycle_interval(d: &SheetData) -> Vec<RecycleBucket> {
    let c_bucket = col_any(d, &["interval_bucket", "bucket"]);
    let c_total = col_any(d, &["total_next", "count"]);
    let c_apo = col_any(d, &["apo_next"]);

    let mut acc: HashMap<String, (f64, f64)> = HashMap::new();
    for row in &d.rows {
        let raw = cell(row, c_bucket).trim();
        // 末尾の「日」を落として正規化（"1-3日" → "1-3"）
        let b = raw.trim_end_matches('日').to_string();
        if !RECYCLE_ORDER.contains(&b.as_str()) {
            continue;
        }
        let e = acc.entry(b).or_insert((0.0, 0.0));
        e.0 += numv(row, c_apo);
        e.1 += numv(row, c_total);
    }

    RECYCLE_ORDER
        .iter()
        .filter_map(|b| {
            acc.get(*b).map(|(apo, total)| RecycleBucket {
                bucket: (*b).to_string(),
                apo_next: *apo,
                total_next: *total,
                apo_rate: rate(*apo, *total),
            })
        })
        .collect()
}

// ---------------------------------------------------------------- パネル: コンプライアンス

/// コンプライアンスパターン。GAS `_drawP2CompliancePatternFromData`。
///
/// シート「コンプライアンスパターン」は `owner_id, year_month,
/// na_breach_streak_days, na_breach_streak_avg_days, end_of_month_concentration,
/// compliance_drift`（実データ 1,224行 = owner × 月）。
///
/// 注意（モジュール冒頭の「差」3・4 と対応）:
///   - GAS は期間フィルタを効かせないが、ここでは他パネルと揃えて効かせる。
///   - GAS は行（owner×月）をそのまま1点として扱う。計算はそのまま残し、
///     `year_month` を返して「同じ人が何度も出る」ことが分かるようにする。
pub fn build_compliance(
    d: &SheetData,
    members: &HashMap<String, Member>,
    q: &P2Query,
) -> CompliancePattern {
    let c_owner = d.col("owner_id");
    let c_ym = d.col("year_month");
    let c_max = col_any(d, &["na_breach_streak_days", "max_consecutive_violation_days"]);
    let c_avg = col_any(
        d,
        &[
            "na_breach_streak_avg_days",
            "avg_consecutive_violation_days",
            "na_breach_streak_mean_days",
        ],
    );
    let c_rush = col_any(d, &["end_of_month_concentration", "month_end_rush_rate"]);
    let c_drift = col_any(d, &["compliance_drift", "drift"]);

    let selected = q.selected_owners();
    let from = q.from.as_deref().unwrap_or("");
    let to = q.to.as_deref().unwrap_or("");

    struct Raw {
        owner_id: String,
        year_month: String,
        max: f64,
        avg: f64,
        rush: f64,
        drift: f64,
    }

    let mut raws: Vec<Raw> = Vec::new();
    for row in &d.rows {
        let ym = cell(row, c_ym).trim();
        let ym = &ym[..ym.len().min(7)];
        if !ym.is_empty() {
            let date = format!("{ym}-01");
            if !from.is_empty() && date.as_str() < from {
                continue;
            }
            if !to.is_empty() && date.as_str() > to {
                continue;
            }
        }
        let owner_id = cell(row, c_owner).trim().to_string();
        if !selected.is_empty() && !selected.iter().any(|x| x == &owner_id) {
            continue;
        }
        // 月末駆け込み率は 0-1 で入っていることがあるので % に揃える。
        // GAS も同じ「1.5 以下なら割合とみなす」判定を使っている。
        let mut rush = numv(row, c_rush);
        if rush > 0.0 && rush <= 1.5 {
            rush *= 100.0;
        }
        let r = Raw {
            owner_id,
            year_month: ym.to_string(),
            max: numv(row, c_max),
            avg: numv(row, c_avg),
            rush,
            drift: numv(row, c_drift),
        };
        // 4指標すべて無風の行は載せない（GAS と同じ）
        if r.max > 0.0 || r.avg > 0.0 || r.rush > 0.0 || r.drift.abs() > 0.0 {
            raws.push(r);
        }
    }

    let z_max = zscores(&raws.iter().map(|r| r.max).collect::<Vec<_>>());
    let z_avg = zscores(&raws.iter().map(|r| r.avg).collect::<Vec<_>>());
    let z_rush = zscores(&raws.iter().map(|r| r.rush).collect::<Vec<_>>());
    let z_drift = zscores(&raws.iter().map(|r| r.drift).collect::<Vec<_>>());

    let mut rows: Vec<ComplianceRow> = raws
        .iter()
        .enumerate()
        .map(|(i, r)| ComplianceRow {
            owner_id: r.owner_id.clone(),
            name: member_name(members, &r.owner_id),
            year_month: r.year_month.clone(),
            streak_max: r.max,
            streak_avg: r.avg,
            month_end_rush: r.rush,
            drift: r.drift,
            z_streak_max: z_max[i],
            z_streak_avg: z_avg[i],
            z_month_end_rush: z_rush[i],
            z_drift: z_drift[i],
            z_sum: z_max[i] + z_avg[i] + z_rush[i] + z_drift[i],
        })
        .collect();

    // Z合計の降順。同値は owner_id → year_month で固定して並びを安定させる。
    rows.sort_by(|a, b| {
        desc(a.z_sum, b.z_sum)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
            .then_with(|| a.year_month.cmp(&b.year_month))
    });

    let total_rows = rows.len();
    let truncated = total_rows > TOP_N_COMPLIANCE;
    rows.truncate(TOP_N_COMPLIANCE);

    CompliancePattern {
        rows,
        total_rows,
        truncated,
        note: "1行 = owner × 年月。GAS 版と同じ粒度のため、同じ担当者が月数だけ並ぶ。\
               Z-score も行集合に対して計算している（母集団標準偏差）。",
    }
}

// ---------------------------------------------------------------- パネル: ファネル

const FUNNEL_STAGES: &[(&str, &str, &str)] = &[
    ("dial", "架電", "dial_count"),
    ("connect", "Call記録", "connect_count"),
    ("conv30s", "30秒会話", "conversation_30s_count"),
    ("meeting", "商談", "meeting_count"),
    ("opp", "案件化", "opp_count"),
    ("won", "成約", "won_count"),
];

/// ファネルの期間セレクタ → (since, until)。GAS `_p5FunnelPeriodRange`。
/// `3m` は「当月含め3ヶ月」なので 2ヶ月前が下限。
fn funnel_range(period: Option<&str>, today_ym: &str) -> (String, String) {
    fn ym_offset(today_ym: &str, back_months: i32) -> String {
        // "YYYY-MM" を数値化して月単位で戻す（chrono を使わずに済み、テストしやすい）
        let y: i32 = today_ym.get(..4).and_then(|s| s.parse().ok()).unwrap_or(0);
        let m: i32 = today_ym.get(5..7).and_then(|s| s.parse().ok()).unwrap_or(1);
        if y == 0 {
            return today_ym.to_string();
        }
        let total = y * 12 + (m - 1) - back_months;
        let ny = total.div_euclid(12);
        let nm = total.rem_euclid(12) + 1;
        format!("{ny:04}-{nm:02}")
    }
    // 正規化は `funnel_period_effective` に一本化する（`ScopeInfo` が返す値と
    // 実際に効く範囲が食い違わないようにするため。ここで独自に match すると
    // 「応答は 6m と言っているのに since-zoom で集計していた」が起こりうる）。
    match funnel_period_effective(period) {
        "all" => ("all".to_string(), String::new()),
        "3m" => (ym_offset(today_ym, 2), String::new()),
        "6m" => (ym_offset(today_ym, 5), String::new()),
        "12m" => (ym_offset(today_ym, 11), String::new()),
        // 既定: Zoom Phone 連携開始以降
        _ => (FUNNEL_DIAL_START_YM.to_string(), String::new()),
    }
}

/// 営業ファネル6段。GAS `getFunnel` + `_drawP5SankeyFromData`。
///
/// - シート「ファネル4段」（名前は4段だが実際は6段ぶんの列を持つ）
/// - **メンバー・PL・ロールのフィルタはかからない**（GAS も同じ）。
///   `dial` は Zoom 集約値、`connect` 以降は owner 別 Call 記録の合算で
///   母集団が違うため、dial→connect の段差は実コンバージョンではない。
/// - 単調クランプ: 上流段が下流段を下回ったら上流を引き上げる（`opp < won` の
///   逆転が実際に起きる）。**下流の実数は毀損しない**。起きたら `clamped` を立てる。
pub fn build_funnel(d: &SheetData, q: &P2Query, today_ym: &str) -> Funnel {
    let c_ym = d.col("year_month");
    let cols: Vec<Option<usize>> = FUNNEL_STAGES.iter().map(|(_, _, c)| d.col(c)).collect();

    let (since, until) = funnel_range(q.funnel_period.as_deref(), today_ym);
    let lo: Option<&str> = if since == "all" { None } else { Some(&since) };
    let hi: Option<&str> = if until.is_empty() { None } else { Some(&until) };

    let mut totals = vec![0.0f64; FUNNEL_STAGES.len()];
    let mut dial_rows_in_range = 0usize;

    for row in &d.rows {
        let ym_full = cell(row, c_ym).trim();
        let ym = &ym_full[..ym_full.len().min(7)];
        if let Some(l) = lo {
            if !ym.is_empty() && ym < l {
                continue;
            }
        }
        if let Some(h) = hi {
            if !ym.is_empty() && ym > h {
                continue;
            }
        }
        for (i, ci) in cols.iter().enumerate() {
            let v = numv(row, *ci);
            totals[i] += v;
            if i == 0 && v > 0.0 {
                dial_rows_in_range += 1;
            }
        }
    }

    let raw_stages: Vec<FunnelStage> = FUNNEL_STAGES
        .iter()
        .enumerate()
        .map(|(i, (key, label, _))| FunnelStage {
            key,
            label,
            value: totals[i],
        })
        .collect();

    let mut clamped_stages: Vec<&'static str> = Vec::new();
    for k in (0..FUNNEL_STAGES.len() - 1).rev() {
        if totals[k] < totals[k + 1] {
            totals[k] = totals[k + 1];
            clamped_stages.push(FUNNEL_STAGES[k].0);
        }
    }
    // 上流に向かって走査したので、返すときは段の順に揃える
    clamped_stages.reverse();
    let clamped = !clamped_stages.is_empty();

    let stages = FUNNEL_STAGES
        .iter()
        .enumerate()
        .map(|(i, (key, label, _))| FunnelStage {
            key,
            label,
            value: totals[i],
        })
        .collect();
    let flows = (0..FUNNEL_STAGES.len() - 1)
        .map(|i| FunnelFlow {
            from: FUNNEL_STAGES[i].1,
            to: FUNNEL_STAGES[i + 1].1,
            flow: totals[i + 1],
        })
        .collect();

    Funnel {
        since,
        until,
        stages,
        flows,
        clamped,
        clamped_stages,
        raw_stages,
        dial_available: dial_rows_in_range > 0,
        scope_note: "メンバー・パイプライン・ロールのフィルタは未適用（GAS 版と同じ）。\
                     dial は Zoom 集約値、connect 以降は owner 別 Call 記録の合算で母集団が異なる。",
    }
}

// ---------------------------------------------------------------- パネル: 滞留日数

/// ステージ滞留日数。GAS `_drawP5StageDwellFromData`。
///
/// シート「滞留日数」は `pipeline, dealstage, stage_label, n_samples,
/// median_days, p25_days, p75_days, bottleneck_z, severity`。
/// メンバー・期間フィルタは持たない（シート自体が pipeline × stage 粒度）。
pub fn build_stage_dwell(d: &SheetData) -> StageDwell {
    let c_pipeline = d.col("pipeline");
    let c_stage = col_any(d, &["stage_label", "dealstage", "stage"]);
    let c_median = d.col("median_days");
    let c_p25 = d.col("p25_days");
    let c_p75 = d.col("p75_days");
    let c_n = col_any(d, &["n_samples", "count", "sample_size", "n"]);
    let c_z = col_any(d, &["bottleneck_z", "bottleneck_score"]);

    let mut rows: Vec<DwellRow> = Vec::new();
    for row in &d.rows {
        let median = numv(row, c_median);
        // median 0 以下は「まだ滞留が測れていない」ので載せない（GAS と同じ）
        if median <= 0.0 {
            continue;
        }
        let pipeline = cell(row, c_pipeline).trim().to_string();
        let stage = {
            let s = cell(row, c_stage).trim();
            if s.is_empty() { "?".to_string() } else { s.to_string() }
        };
        let n = num_opt(cell(row, c_n));
        rows.push(DwellRow {
            label: format!(
                "{} / {}",
                if pipeline.is_empty() { "?" } else { &pipeline },
                stage
            ),
            pipeline,
            stage,
            median_days: median,
            p25_days: num_opt(cell(row, c_p25)),
            p75_days: num_opt(cell(row, c_p75)),
            n_samples: n,
            z: num_opt(cell(row, c_z)),
            low_confidence: n.map(|v| v < MIN_SAMPLES_FOR_DWELL).unwrap_or(false),
        });
    }

    // シートに bottleneck_z が1つも無ければ median の Z-score を自前計算する
    let z_self_computed = !rows.iter().any(|r| r.z.is_some());
    if z_self_computed && !rows.is_empty() {
        let zs = zscores(&rows.iter().map(|r| r.median_days).collect::<Vec<_>>());
        for (i, r) in rows.iter_mut().enumerate() {
            r.z = Some(zs[i]);
        }
    }

    // 滞留の長い順。同値はラベルで固定。
    rows.sort_by(|a, b| desc(a.median_days, b.median_days).then_with(|| a.label.cmp(&b.label)));
    let total_rows = rows.len();
    let truncated = total_rows > TOP_N_DWELL;
    rows.truncate(TOP_N_DWELL);

    StageDwell {
        rows,
        total_rows,
        truncated,
        z_self_computed,
        scope_note: "期間・メンバー・PL のフィルタは未適用（シートが pipeline × stage 粒度のため）。",
    }
}

// ---------------------------------------------------------------- パネル: その場失注

/// その場失注（商談PL → 商談済リードPL へ進捗確認を経ずに直行した Deal）。
/// GAS `_drawP5OnTheSpotLost`。
///
/// シート「その場失注 owner月次」は `owner_id, year_month,
/// on_the_spot_lost_count, deal_count_total`。
///
/// **GAS との差（意図的）**: GAS はメンバー未選択時に全ロールを含めるため
/// BPO/コンサルが混ざる。約束5に従い role=sales を既定スコープにした。
pub fn build_on_the_spot(
    d: &SheetData,
    members: &HashMap<String, Member>,
    q: &P2Query,
) -> OnTheSpot {
    let c_owner = d.col("owner_id");
    let c_ym = d.col("year_month");
    let c_count = col_any(d, &["on_the_spot_lost_count", "count", "lost_count"]);

    let selected = q.selected_owners();
    let from_ym = q
        .from
        .as_deref()
        .filter(|s| s.len() >= 7)
        .map(|s| s[..7].to_string())
        .unwrap_or_default();
    let to_ym = q
        .to
        .as_deref()
        .filter(|s| s.len() >= 7)
        .map(|s| s[..7].to_string())
        .unwrap_or_default();

    let mut by_owner: HashMap<String, f64> = HashMap::new();
    let mut total = 0.0f64;
    for row in &d.rows {
        let ym_full = cell(row, c_ym).trim();
        let ym = &ym_full[..ym_full.len().min(7)];
        if !from_ym.is_empty() && !ym.is_empty() && ym < from_ym.as_str() {
            continue;
        }
        if !to_ym.is_empty() && !ym.is_empty() && ym > to_ym.as_str() {
            continue;
        }
        let owner_id = cell(row, c_owner).trim().to_string();
        if selected.is_empty() {
            if members.get(&owner_id).map(|m| m.role.as_str()) != Some("sales") {
                continue;
            }
        } else if !selected.iter().any(|x| x == &owner_id) {
            continue;
        }
        let c = numv(row, c_count);
        *by_owner.entry(owner_id).or_insert(0.0) += c;
        total += c;
    }

    let mut rows: Vec<OnTheSpotRow> = by_owner
        .into_iter()
        .map(|(owner_id, count)| OnTheSpotRow {
            name: member_name(members, &owner_id),
            owner_id,
            count,
        })
        .collect();
    // 件数降順。同値は owner_id で固定。
    rows.sort_by(|a, b| desc(a.count, b.count).then_with(|| a.owner_id.cmp(&b.owner_id)));

    let owner_count = rows.len();
    OnTheSpot {
        total_count: total,
        owner_count,
        // 発生 owner が 0 名なら「平均 0 件」ではなく「算出不能」
        avg_per_owner: ratio(total, owner_count as f64),
        rows,
        from_ym,
        to_ym,
    }
}

// ---------------------------------------------------------------- パネル: 商談PL 遷移

/// 商談PL 関連の主要ステージ。GAS `_P5_TRANS_MAIN_STAGES` からそのまま移した。
///
/// これは「運用で変わる設定値」ではなく、**画面に出すステージの選定**
/// （アーナ管理/計上等の他PLステージと「未知(...)」を落とす）。
/// HubSpot 側でステージを増やしたら Python 側の出力と合わせてここも見直すこと。
const TRANS_MAIN_STAGES: &[&str] = &[
    // アポ前PL のアポ化（商談PL への入口）
    "1332175104",
    "51997752",
    // 商談PL (21724969)
    "52035886",
    "71794963",
    "1074024975",
    "52035887",
    "52035888",
    "52035889",
    "52035890",
    "52035891",
    "52017683",
    // 商談済リードPL (62583420) 失注系
    "155012220",
    "155012221",
    "155012222",
    "155012223",
    "155012224",
];

/// ステージID → 日本語ラベルの保険。GAS `P5_TRANS_STAGE_FALLBACK_LABEL` と同じ。
/// シートに `stage_from_label` / `stage_to_label` があればそちらを優先する。
fn trans_fallback_label(stage_id: &str) -> Option<&'static str> {
    Some(match stage_id {
        "1332175104" => "日程確保済み(アポ化)",
        "51997752" => "TEL_アポ日確定(アポ化)",
        "52035886" => "アポ日確定",
        "71794963" => "日程再調整中",
        "1074024975" => "大分BPO(IS失注)",
        "52035887" => "進捗確認(商談実施済)",
        "52035888" => "Dヨミ(10%)",
        "52035889" => "Cヨミ(30%)",
        "52035890" => "Bヨミ(70%)",
        "52035891" => "Aヨミ(90%)",
        "52017683" => "成約",
        "155012220" | "155012221" | "155012222" | "155012223" | "155012224" => {
            "商談済リード(失注)"
        }
        _ => return None,
    })
}

/// 表示ラベルの正規化。GAS `_dispLbl` + `_p5TransLabel`。
/// 空 / 数字のみ / 英字のみ / 「未知(」始まり は fallback マップを引き直す。
fn trans_label(stage_id: &str, provided: &str) -> String {
    let l = provided.trim();
    let looks_bad = l.is_empty()
        || l.chars().all(|c| c.is_ascii_digit())
        || l.chars().all(|c| c.is_ascii_alphabetic() || c == '_')
        || l.starts_with("未知(");
    if !looks_bad {
        return l.to_string();
    }
    trans_fallback_label(stage_id)
        .map(|s| s.to_string())
        .unwrap_or_else(|| {
            if stage_id.is_empty() {
                "(不明)".to_string()
            } else {
                stage_id.to_string()
            }
        })
}

fn is_main_stage(stage_id: &str, label: &str) -> bool {
    if TRANS_MAIN_STAGES.contains(&stage_id) {
        return true;
    }
    if label.starts_with("未知(") {
        return false;
    }
    label == "商談済リードPL(失注系)" || label == "商談済リード(失注)"
}

/// 失注系か。GAS `_isLostStage`。
fn is_lost_stage(stage_id: &str, label: &str) -> bool {
    if label.contains("失注") || label.contains("商談済リード") {
        return true;
    }
    matches!(
        stage_id,
        "155012220" | "155012221" | "155012222" | "155012223" | "155012224"
    )
}

/// 商談PL 遷移分析。GAS `_drawP5TransitionFromData`。
///
/// - 業界/規模が未指定 → 「商談遷移_集計」をそのまま（share / p25 / p50 / p75 が揃う）
/// - 指定あり → 「商談遷移_クロス」を絞り込んで (from,to) で再集計。
///   share は絞込後の from 内で再計算、p50 は件数加重平均、**p25/p75 は取れない**
///   （クロス側に列が無いため。None を返す。0 で埋めない）。
/// - 主要ステージ以外と、ラベル同一の自己遷移（失注5ステージを1ラベルに統合した副作用）を落とす。
/// クロス絞込の値を正規化する。空 / `__all__` は「絞らない」。
///
/// **`build_stage_transition` と `ScopeInfo` の両方がこれを使う**。
/// 別々に書くと「絞ったつもりの表示」と「実際の絞り込み」がずれる。
fn cross_filter(v: Option<&str>) -> Option<&str> {
    v.filter(|s| !s.is_empty() && *s != "__all__")
}

/// 応答に載せる用。絞っていなければ `__all__` を返す（空文字にしない —
/// 「値が無い」と「全件」を画面で区別できなくなるため）。
fn cross_value(v: Option<&str>) -> String {
    cross_filter(v).unwrap_or("__all__").to_string()
}

/// ファネル期間セレクタの**実際に効いた値**。
///
/// `funnel_range` は未知の文字列を既定（`since-zoom`）へ落とすので、
/// ここも同じ規則で正規化する。`?funnel_period=6M` のような大文字違いが
/// 黙って `since-zoom` になっていたことが、これで応答から分かる。
fn funnel_period_effective(v: Option<&str>) -> &'static str {
    match v.unwrap_or("since-zoom") {
        "all" => "all",
        "3m" => "3m",
        "6m" => "6m",
        "12m" => "12m",
        _ => "since-zoom",
    }
}

pub fn build_stage_transition(
    summary: &SheetData,
    cross: &SheetData,
    q: &P2Query,
) -> StageTransition {
    let ind_val = cross_filter(q.trans_industry.as_deref());
    let size_val = cross_filter(q.trans_size.as_deref());
    let cross_active = ind_val.is_some() || size_val.is_some();

    // --- セレクタ候補（並びを安定させる） ---
    let xc_ind = cross.col("industry_jsic");
    let xc_size = cross.col("size_band");
    let mut industries: Vec<String> = cross
        .rows
        .iter()
        .map(|r| cell(r, xc_ind).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    industries.sort();
    industries.dedup();
    let mut size_bands: Vec<String> = cross
        .rows
        .iter()
        .map(|r| cell(r, xc_size).trim().to_string())
        .filter(|s| !s.is_empty())
        .collect();
    size_bands.sort();
    size_bands.dedup();
    // 規模は「10-49人」のように先頭の数値で並べたいので数値優先ソート（GAS と同じ意図）
    size_bands.sort_by(|a, b| {
        let n = |s: &str| -> i64 {
            let digits: String = s.chars().skip_while(|c| !c.is_ascii_digit())
                .take_while(|c| c.is_ascii_digit())
                .collect();
            digits.parse::<i64>().unwrap_or(0)
        };
        n(a).cmp(&n(b)).then_with(|| a.cmp(b))
    });

    let mut pairs: Vec<TransitionPair> = if cross_active {
        let c_from = cross.col("stage_from");
        let c_to = cross.col("stage_to");
        let c_from_l = cross.col("stage_from_label");
        let c_to_l = cross.col("stage_to_label");
        let c_count = cross.col("transition_count");
        let c_p50 = cross.col("lead_time_p50_days");

        // (from, to) で畳む。p50 は件数加重平均で近似する。
        let mut acc: HashMap<(String, String), (String, String, f64, f64, f64)> = HashMap::new();
        for row in &cross.rows {
            if let Some(v) = ind_val {
                if cell(row, xc_ind) != v {
                    continue;
                }
            }
            if let Some(v) = size_val {
                if cell(row, xc_size) != v {
                    continue;
                }
            }
            let from = cell(row, c_from).trim().to_string();
            let to = cell(row, c_to).trim().to_string();
            let cnt = numv(row, c_count);
            let e = acc.entry((from.clone(), to.clone())).or_insert_with(|| {
                (
                    trans_label(&from, cell(row, c_from_l)),
                    trans_label(&to, cell(row, c_to_l)),
                    0.0,
                    0.0,
                    0.0,
                )
            });
            e.2 += cnt;
            if let Some(p50) = num_opt(cell(row, c_p50)) {
                if cnt > 0.0 {
                    e.3 += p50 * cnt;
                    e.4 += cnt;
                }
            }
        }
        // 絞込後の from 内シェアを計算し直す
        let mut from_totals: HashMap<String, f64> = HashMap::new();
        for ((from, _), v) in acc.iter() {
            *from_totals.entry(from.clone()).or_insert(0.0) += v.2;
        }
        acc.into_iter()
            .map(|((from, to), (from_label, to_label, count, p50n, p50d))| TransitionPair {
                share_within_from: rate(count, *from_totals.get(&from).unwrap_or(&0.0)),
                stage_from: from,
                stage_to: to,
                from_label,
                to_label,
                count,
                lead_time_p50: ratio(p50n, p50d),
                // クロスシートは p25/p75 を持たない。0 で埋めず「無い」と返す。
                lead_time_p25: None,
                lead_time_p75: None,
            })
            .collect()
    } else {
        let c_from = summary.col("stage_from");
        let c_to = summary.col("stage_to");
        let c_from_l = summary.col("stage_from_label");
        let c_to_l = summary.col("stage_to_label");
        let c_count = summary.col("transition_count");
        let c_share = summary.col("share_within_from_pct");
        let c_p25 = summary.col("lead_time_p25_days");
        let c_p50 = summary.col("lead_time_p50_days");
        let c_p75 = summary.col("lead_time_p75_days");

        summary
            .rows
            .iter()
            .map(|row| {
                let from = cell(row, c_from).trim().to_string();
                let to = cell(row, c_to).trim().to_string();
                TransitionPair {
                    from_label: trans_label(&from, cell(row, c_from_l)),
                    to_label: trans_label(&to, cell(row, c_to_l)),
                    stage_from: from,
                    stage_to: to,
                    count: numv(row, c_count),
                    share_within_from: num_opt(cell(row, c_share)),
                    lead_time_p25: num_opt(cell(row, c_p25)),
                    lead_time_p50: num_opt(cell(row, c_p50)),
                    lead_time_p75: num_opt(cell(row, c_p75)),
                }
            })
            .collect()
    };

    let pairs_before_filter = pairs.len();
    pairs.retain(|p| {
        is_main_stage(&p.stage_from, &p.from_label) && is_main_stage(&p.stage_to, &p.to_label)
    });
    // ラベル統合の副作用で出る自己遷移（商談済リードPL 内部5ステージ → 1ラベル）を落とす
    pairs.retain(|p| !(!p.from_label.is_empty() && p.from_label == p.to_label));

    // 件数降順。同値は from→to で固定。
    pairs.sort_by(|a, b| {
        desc(a.count, b.count)
            .then_with(|| a.stage_from.cmp(&b.stage_from))
            .then_with(|| a.stage_to.cmp(&b.stage_to))
    });

    let total_transitions: f64 = pairs.iter().map(|p| p.count).sum();
    let kpis = build_transition_kpis(&pairs);

    StageTransition {
        cross_active,
        industry: ind_val.unwrap_or("__all__").to_string(),
        size_band: size_val.unwrap_or("__all__").to_string(),
        kpis,
        pairs,
        total_transitions,
        pairs_before_filter,
        industries,
        size_bands,
        scope_note: "期間・メンバー・PL のフィルタは未適用（GAS 版と同じ）。\
                     クロス絞込時は p25/p75 が取れず null になる。",
    }
}

/// 遷移KPI 4枚。GAS の `p5-trans-kpis`。
/// ステージは ID とラベルの**どちらでも**照合する（GAS `findPair` と同じ）。
pub fn build_transition_kpis(pairs: &[TransitionPair]) -> Vec<MetricCard> {
    let matches_from = |p: &TransitionPair, key: &str| p.stage_from == key || p.from_label == key;
    let matches_to = |p: &TransitionPair, key: &str| p.stage_to == key || p.to_label == key;

    let from_total = |keys: &[&str]| -> f64 {
        pairs
            .iter()
            .filter(|p| keys.iter().any(|k| matches_from(p, k)))
            .map(|p| p.count)
            .sum()
    };
    let find_pair = |from_keys: &[&str], to_keys: &[&str]| -> f64 {
        pairs
            .iter()
            .find(|p| {
                from_keys.iter().any(|k| matches_from(p, k))
                    && to_keys.iter().any(|k| matches_to(p, k))
            })
            .map(|p| p.count)
            .unwrap_or(0.0)
    };

    // 「アポ日確定」= ラベル or stage_id 52035886
    const APO: &[&str] = &["アポ日確定", "52035886"];
    // 「進捗確認(商談実施済)」= ラベル or stage_id 52035887。
    // GAS は '進捗確認 (商談実施済)'（半角スペースあり）で引いており、
    // Python 側ラベルが '進捗確認(商談実施済)'（スペース無し）だと ID 側でしか
    // 当たらない。両表記を候補に入れて取りこぼさないようにする。
    const PROG: &[&str] = &["進捗確認(商談実施済)", "進捗確認 (商談実施済)", "52035887"];
    const A_YOMI: &[&str] = &["Aヨミ(90%)", "52035891"];

    let apo_total = from_total(APO);
    let apo_to_prog = find_pair(APO, PROG);
    let apo_to_lost: f64 = pairs
        .iter()
        .filter(|p| APO.iter().any(|k| matches_from(p, k)))
        .filter(|p| is_lost_stage(&p.stage_to, &p.to_label))
        .map(|p| p.count)
        .sum();
    let prog_total = from_total(PROG);
    let prog_to_a = find_pair(PROG, A_YOMI);

    // アポ確定起点の p50 を件数加重平均
    let mut lt_num = 0.0f64;
    let mut lt_den = 0.0f64;
    for p in pairs
        .iter()
        .filter(|p| APO.iter().any(|k| matches_from(p, k)))
    {
        if let Some(p50) = p.lead_time_p50 {
            if p.count > 0.0 {
                lt_num += p50 * p.count;
                lt_den += p.count;
            }
        }
    }

    vec![
        MetricCard {
            key: "apo_to_progress",
            label: "アポ確定→進捗確認 率",
            kind: "percent",
            value: rate(apo_to_prog, apo_total),
            numerator: apo_to_prog,
            denominator: apo_total,
            note: "アポ確定のうち商談実施済へ進んだ割合",
        },
        MetricCard {
            key: "apo_to_lost",
            label: "アポ確定→即失注 率",
            kind: "percent",
            value: rate(apo_to_lost, apo_total),
            numerator: apo_to_lost,
            denominator: apo_total,
            note: "商談済リードPL へ直行した割合（低いほど良い）",
        },
        MetricCard {
            key: "progress_to_a",
            label: "進捗確認→Aヨミ 直行率",
            kind: "percent",
            value: rate(prog_to_a, prog_total),
            numerator: prog_to_a,
            denominator: prog_total,
            note: "進捗確認のうち Aヨミ(90%) へ進んだ割合",
        },
        MetricCard {
            key: "apo_lead_time",
            label: "アポ確定起点 平均リードタイム",
            kind: "ratio",
            value: ratio(lt_num, lt_den),
            numerator: lt_num,
            denominator: lt_den,
            note: "件数加重平均 p50（日）。短いほど商談進行が速い",
        },
    ]
}

// ---------------------------------------------------------------- ハンドラ

/// このタブが読むシート。取得失敗しても他パネルを巻き添えにしないよう、
/// 1枚ずつ取って空シートで代替する。
async fn get_or_empty(
    store: &SheetStore,
    client: &SheetsClient,
    name: &str,
    sources: &mut Vec<SourceInfo>,
) -> Arc<SheetData> {
    match store.get(client, name).await {
        Ok((d, from_cache)) => {
            sources.push(SourceInfo {
                sheet: name.to_string(),
                total_rows: d.rows.len(),
                matched_rows: d.rows.len(),
                from_cache,
                age_secs: d.fetched_at.elapsed().as_secs(),
            });
            d
        }
        Err(e) => {
            // シート未生成（Python バッチ未実行）は実際に起きる。
            // ここで握りつぶすが、**空だったことは sources に必ず残す**。
            tracing::warn!("架電クオリティP2: シート「{name}」を読めなかった: {e:#}");
            sources.push(SourceInfo {
                sheet: format!("{name}（取得失敗: {e}）"),
                total_rows: 0,
                matched_rows: 0,
                from_cache: false,
                age_secs: 0,
            });
            Arc::new(SheetData {
                header: Vec::new(),
                rows: Vec::new(),
                fetched_at: Instant::now(),
            })
        }
    }
}

pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: P2Query,
) -> Result<TabPayload<P2HabitsData>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    // 都道府県を選ぶと土台シートごと入れ替わる（GAS `filteredRows` と同じ）
    let base_sheet = if q.prefecture_mode().is_some() {
        SHEET_PREFECTURE_MONTHLY
    } else {
        SHEET_MONTHLY
    };

    let members_sheet = get_or_empty(store, client, SHEET_MEMBERS, &mut sources).await;
    let members = load_members(&members_sheet);

    let monthly = get_or_empty(store, client, base_sheet, &mut sources).await;
    let rows = scope_monthly(&monthly, &members, &q);
    // 絞り込み後の行数を、土台シートの SourceInfo に反映する
    if let Some(s) = sources.iter_mut().find(|s| s.sheet == base_sheet) {
        s.matched_rows = rows.len();
    }

    let pref_mode = q.prefecture_mode().is_some();
    let owners = sum_by_owner(&rows);
    let totals = totals_of(&rows);

    let touch_sheet = get_or_empty(store, client, SHEET_TOUCH_DIST, &mut sources).await;
    let recycle_sheet = get_or_empty(store, client, SHEET_RECYCLE, &mut sources).await;
    let compliance_sheet = get_or_empty(store, client, SHEET_COMPLIANCE, &mut sources).await;
    let funnel_sheet = get_or_empty(store, client, SHEET_FUNNEL, &mut sources).await;
    let dwell_sheet = get_or_empty(store, client, SHEET_STAGE_DWELL, &mut sources).await;
    let spot_sheet = get_or_empty(store, client, SHEET_ON_THE_SPOT, &mut sources).await;
    let trans_sum_sheet = get_or_empty(store, client, SHEET_TRANS_SUMMARY, &mut sources).await;
    let trans_cross_sheet = get_or_empty(store, client, SHEET_TRANS_CROSS, &mut sources).await;

    // ファネルの相対期間（3m/6m/12m）の起点。明示指定が無ければ実行時のローカル日付。
    let today_ym = q
        .today_ym
        .clone()
        .unwrap_or_else(|| super::jst_current_ym());

    let data = P2HabitsData {
        scope: ScopeInfo {
            from: q.from.clone().unwrap_or_default(),
            to: q.to.clone().unwrap_or_default(),
            pipeline: q.pipeline.clone().unwrap_or_else(|| "__all__".to_string()),
            prefecture: q.prefecture.clone().unwrap_or_else(|| "__all__".to_string()),
            member_selected: !q.selected_owners().is_empty(),
            owner_count: owners.len(),
            apo_denominator_label: apo_denominator_label(pref_mode),
            matched_rows: rows.len(),
            // 以下4つは「実際に効いた値」。`build_stage_transition` /
            // `build_funnel` と同じ正規化規則を使う（別々に書くと第二の
            // 定義箇所になるので、判定は `cross_value` / `funnel_period_effective`
            // に寄せてある）。
            trans_industry: cross_value(q.trans_industry.as_deref()),
            trans_size: cross_value(q.trans_size.as_deref()),
            funnel_period: funnel_period_effective(q.funnel_period.as_deref()).to_string(),
            funnel_today_ym: today_ym.clone(),
        },
        scorecards: build_scorecards(&totals),
        rankings: build_rankings(&owners, &members),
        scatters: build_scatters(&owners, &members, pref_mode),
        touch_distribution: build_touch_distribution(&touch_sheet, q.pipeline_filter()),
        recycle_interval: build_recycle_interval(&recycle_sheet),
        compliance: build_compliance(&compliance_sheet, &members, &q),
        process_scorecards: build_process_scorecards(&totals),
        funnel: build_funnel(&funnel_sheet, &q, &today_ym),
        stage_dwell: build_stage_dwell(&dwell_sheet),
        on_the_spot: build_on_the_spot(&spot_sheet, &members, &q),
        stage_transition: build_stage_transition(&trans_sum_sheet, &trans_cross_sheet, &q),
    };

    Ok(TabPayload {
        data,
        sources,
        elapsed_ms: started.elapsed().as_millis(),
        // ルータが後乗せする（タブ側は生のクエリ文字列を知らない）
        ignored_params: Vec::new(),
    })
}

// ---------------------------------------------------------------- テスト

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

    fn members_sheet() -> SheetData {
        sheet(
            &["owner_id", "name", "role"],
            &[
                &["1", "営業A", "sales"],
                &["2", "営業B", "sales"],
                &["3", "BPO C", "bpo"],
                &["4", "コンサルD", "consultant"],
            ],
        )
    }

    /// 月次明細の最小セット。列順は実シート（feature_monthly.csv）と同じにしてある。
    fn monthly_sheet() -> SheetData {
        sheet(
            &[
                "owner_id",
                "year_month",
                "pipeline",
                "call_count",
                "deal_touched",
                "new_open_deals",
                "high_call_deals",
                "stage_advanced",
                "stage_entered",
                "na_due",
                "na_done_ontime",
                "na_done_total",
                "rs_due",
                "rs_done_ontime",
                "rs_done_total",
                "apo_count",
                "company_first_call_count",
                "zoom_dial_count",
            ],
            &[
                // 営業A: NA期日 200 / 期日内 100 → 50%
                &[
                    "1", "2026-06", "PL_A", "500", "100", "40", "10", "60", "100", "200", "100",
                    "150", "300", "60", "90", "10", "20", "800",
                ],
                // 営業B: NA期日 100 / 期日内 10 → 10%
                &[
                    "2", "2026-06", "PL_A", "400", "80", "20", "5", "20", "80", "100", "10", "40",
                    "150", "30", "50", "4", "8", "600",
                ],
                // BPO C: role=bpo。メンバー未選択なら混ざってはいけない
                &[
                    "3", "2026-06", "PL_A", "9000", "900", "900", "90", "900", "900", "900", "900",
                    "900", "900", "900", "900", "900", "900", "9000",
                ],
                // 営業A の別月（期間フィルタ用）
                &[
                    "1", "2026-07", "PL_B", "100", "20", "5", "1", "10", "20", "50", "25", "30",
                    "40", "10", "20", "3", "2", "200",
                ],
            ],
        )
    }

    fn q_default() -> P2Query {
        P2Query::default()
    }

    // ---- 約束2: 分母0は 0% でなく None ----

    #[test]
    fn 分母0のカードは0パーセントでなくnull() {
        // 架電記録が無くてもアポは計上される（2026-08-13 定義変更）。
        // そのため「NA期日0件・アポあり」の担当者は実在する。0% と出してはいけない。
        let t = MonthlyRow {
            na_due: 0.0,
            na_done_ontime: 0.0,
            deal_touched: 0.0,
            call_count: 120.0,
            ..Default::default()
        };
        let cards = build_scorecards(&t);
        let na = cards.iter().find(|c| c.key == "na_ontime").unwrap();
        assert_eq!(na.value, None, "NA期日0件を遵守率0%と表示してはいけない");
        let cpd = cards.iter().find(|c| c.key == "calls_per_deal").unwrap();
        assert_eq!(cpd.value, None, "タッチ案件0件で 1案件あたり架電 0 と出さない");
    }

    #[test]
    fn n回目架電の分母0バケットはnullで返る() {
        let d = sheet(
            &["attempt_no", "pipeline", "total", "apo_count"],
            &[&["1", "__all__", "100", "3"], &["2", "__all__", "0", "0"]],
        );
        let td = build_touch_distribution(&d, None);
        assert_eq!(td.buckets.len(), 20, "1..19 + 20+ の固定20バケット");
        assert_eq!(td.buckets[0].apo_rate, Some(3.0));
        assert_eq!(
            td.buckets[1].apo_rate, None,
            "到達0件のバケットを 0% と描かせない"
        );
    }

    // ---- 約束5: 営業スコープの既定は role=sales ----

    #[test]
    fn メンバー未選択ならbpoとコンサルは混ざらない() {
        let m = load_members(&members_sheet());
        let rows = scope_monthly(&monthly_sheet(), &m, &q_default());
        let ids: Vec<&str> = rows.iter().map(|r| r.owner_id.as_str()).collect();
        assert!(!ids.contains(&"3"), "role=bpo が混ざっている");
        assert_eq!(rows.len(), 3, "営業A×2ヶ月 + 営業B×1ヶ月");
        // 混ざると分母が膨らんでアポ率が希釈される。実際に GAS で起きた事故。
        let t = totals_of(&rows);
        assert_eq!(t.call_count, 500.0 + 400.0 + 100.0);
    }

    #[test]
    fn メンバー選択時はロールを問わずその人だけになる() {
        let m = load_members(&members_sheet());
        let q = P2Query {
            owners: Some("3".into()),
            ..Default::default()
        };
        let rows = scope_monthly(&monthly_sheet(), &m, &q);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].owner_id, "3", "明示選択なら BPO でも見られる");
    }

    // ---- 絞り込み ----

    #[test]
    fn 期間とパイプラインで絞れる() {
        let m = load_members(&members_sheet());
        let q = P2Query {
            from: Some("2026-07-01".into()),
            to: Some("2026-07-01".into()),
            ..Default::default()
        };
        let rows = scope_monthly(&monthly_sheet(), &m, &q);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].year_month, "2026-07");

        let q2 = P2Query {
            pipeline: Some("PL_B".into()),
            ..Default::default()
        };
        let rows2 = scope_monthly(&monthly_sheet(), &m, &q2);
        assert_eq!(rows2.len(), 1);
        assert_eq!(rows2[0].pipeline, "PL_B");

        // "__all__" は「絞らない」であって、値が __all__ の行を探すのではない
        let q3 = P2Query {
            pipeline: Some("__all__".into()),
            ..Default::default()
        };
        assert_eq!(scope_monthly(&monthly_sheet(), &m, &q3).len(), 3);
    }

    // ---- 足切り ----

    #[test]
    fn 率ランキングは分母不足を除外する() {
        let m = load_members(&members_sheet());
        let rows = scope_monthly(&monthly_sheet(), &m, &q_default());
        let owners = sum_by_owner(&rows);
        let rk = build_rankings(&owners, &m);
        let na = rk.iter().find(|r| r.key == "na_ontime").unwrap();
        // 営業A: na_due 200+50=250 → 通過 / 営業B: 100 → ちょうど通過
        assert_eq!(na.rows.len(), 2);
        assert_eq!(na.min_denominator, MIN_DUE_FOR_RATE);
        // 営業A = (100+25)/250 = 50%、営業B = 10/100 = 10%
        assert_eq!(na.rows[0].owner_id, "1");
        assert_eq!(na.rows[0].value, Some(50.0));
        assert_eq!(na.rows[0].rank, 1);
        assert_eq!(na.rows[1].value, Some(10.0));
    }

    #[test]
    fn 散布図の足切りは率と同じ分母で行う() {
        // GAS は HubSpot Call で足切りしつつ Zoom 発信で割っていた（分母の取り違え）。
        // Zoom 発信 50件しか無い担当者は、HubSpot Call が 999件あっても点にしない。
        let m = load_members(&members_sheet());
        let owners = vec![
            MonthlyRow {
                owner_id: "1".into(),
                call_count: 999.0,
                zoom_dial_count: 50.0, // 率の分母は 50 → 足切り未満
                apo_count: 5.0,
                na_due: 200.0,
                na_done_ontime: 100.0,
                deal_touched: 100.0,
                stage_entered: 100.0,
                stage_advanced: 50.0,
                ..Default::default()
            },
            MonthlyRow {
                owner_id: "2".into(),
                call_count: 10.0,
                zoom_dial_count: 400.0, // 率の分母は 400 → 通過
                apo_count: 4.0,
                na_due: 200.0,
                na_done_ontime: 20.0,
                deal_touched: 100.0,
                stage_entered: 100.0,
                stage_advanced: 10.0,
                ..Default::default()
            },
        ];
        let sc = build_scatters(&owners, &m, false);
        let na = sc.iter().find(|s| s.key == "na_apo").unwrap();
        assert_eq!(na.points.len(), 1, "Zoom発信50件の担当者は足切りで落ちる");
        assert_eq!(na.points[0].owner_id, "2");
        assert_eq!(na.points[0].y, 1.0, "4/400 = 1%");
        assert!(
            na.y_label.contains("Zoom発信数が分母"),
            "どちらの分母で割ったかをラベルに必ず出す（単独表記は規約で禁止）"
        );
    }

    // ---- 約束3: 上限で切ったら truncated ----

    #[test]
    fn コンプライアンスは30件で切りtruncatedを立てる() {
        let mut rows: Vec<Vec<String>> = Vec::new();
        for i in 0..35 {
            rows.push(vec![
                format!("{}", i + 1),
                "2026-06".to_string(),
                format!("{}", i + 1), // streak_max
                "1".to_string(),
                "0.5".to_string(),
                "0.1".to_string(),
            ]);
        }
        let refs: Vec<Vec<&str>> = rows
            .iter()
            .map(|r| r.iter().map(|s| s.as_str()).collect())
            .collect();
        let slices: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        let d = sheet(
            &[
                "owner_id",
                "year_month",
                "na_breach_streak_days",
                "na_breach_streak_avg_days",
                "end_of_month_concentration",
                "compliance_drift",
            ],
            &slices,
        );
        let m = load_members(&members_sheet());
        let c = build_compliance(&d, &m, &q_default());
        assert_eq!(c.rows.len(), TOP_N_COMPLIANCE);
        assert_eq!(c.total_rows, 35);
        assert!(c.truncated, "黙って上位30件にしない");
        // 0-1 の値は % に揃える（0.5 → 50%）
        assert_eq!(c.rows[0].month_end_rush, 50.0);
    }

    // ---- 約束4: 並びの安定 ----

    #[test]
    fn 同率でも並びが安定する() {
        let m = load_members(&members_sheet());
        // 2人が完全に同率。HashMap の反復順に引きずられると毎回並びが変わる。
        let mk = |id: &str| MonthlyRow {
            owner_id: id.into(),
            na_due: 100.0,
            na_done_ontime: 50.0,
            ..Default::default()
        };
        let owners = vec![mk("2"), mk("1")];
        let a = build_rankings(&owners, &m);
        let owners2 = vec![mk("1"), mk("2")];
        let b = build_rankings(&owners2, &m);
        let ids = |r: &Vec<RateRanking>| -> Vec<String> {
            r.iter()
                .find(|x| x.key == "na_ontime")
                .unwrap()
                .rows
                .iter()
                .map(|x| x.owner_id.clone())
                .collect()
        };
        assert_eq!(ids(&a), vec!["1", "2"], "同率は owner_id 昇順で固定");
        assert_eq!(ids(&a), ids(&b), "入力順が変わっても同じ並びで返る");
    }

    #[test]
    fn owner集約の並びが安定する() {
        let rows = vec![
            MonthlyRow {
                owner_id: "9".into(),
                call_count: 1.0,
                ..Default::default()
            },
            MonthlyRow {
                owner_id: "1".into(),
                call_count: 2.0,
                ..Default::default()
            },
            MonthlyRow {
                owner_id: "9".into(),
                call_count: 3.0,
                ..Default::default()
            },
        ];
        let o = sum_by_owner(&rows);
        assert_eq!(o.len(), 2);
        assert_eq!(o[0].owner_id, "1");
        assert_eq!(o[1].owner_id, "9");
        assert_eq!(o[1].call_count, 4.0, "同じ owner の行は足しあげる");
    }

    // ---- 各パネル ----

    #[test]
    fn リサイクル間隔はシート順でなく固定順で返る() {
        // シート側の並びが崩れても「1-3 → 61+」の順を保つ。順序が崩れると
        // 「間隔が短いほど良いのか」が読めなくなる。
        let d = sheet(
            &["interval_bucket", "total_next", "apo_next"],
            &[
                &["61+日", "8215", "155"],
                &["1-3日", "31224", "390"],
                &["15-30日", "22376", "314"],
                &["不明", "10", "1"],
            ],
        );
        let r = build_recycle_interval(&d);
        assert_eq!(
            r.iter().map(|x| x.bucket.as_str()).collect::<Vec<_>>(),
            vec!["1-3", "15-30", "61+"],
            "既知バケットのみを固定順で返す"
        );
        // 実データ: 390/31224 = 1.2490...%
        let first = &r[0];
        assert!((first.apo_rate.unwrap() - 1.2490_f64).abs() < 0.001);
    }

    #[test]
    fn ファネルは単調クランプしてフラグを立てる() {
        // opp < won の逆転は実データで起きる（funnel.py 側で母集団が違うため）。
        // 上流を引き上げるが、成約(won)の実数は絶対に減らさない。
        let d = sheet(
            &[
                "year_month",
                "dial_count",
                "connect_count",
                "conversation_30s_count",
                "meeting_count",
                "opp_count",
                "won_count",
            ],
            &[&["2026-06", "1000", "600", "300", "100", "10", "40"]],
        );
        let q = P2Query {
            funnel_period: Some("all".into()),
            ..Default::default()
        };
        let f = build_funnel(&d, &q, "2026-08");
        assert!(f.clamped);
        let opp = f.stages.iter().find(|s| s.key == "opp").unwrap();
        let won = f.stages.iter().find(|s| s.key == "won").unwrap();
        assert_eq!(won.value, 40.0, "下流(成約)の実数を毀損しない");
        assert_eq!(opp.value, 40.0, "上流を下流に合わせて引き上げる");
        assert!(f.dial_available);
        // どの段を書き換えたかを名指しする（bool 1つだと作り物の段が画面から分からない）
        assert_eq!(f.clamped_stages, vec!["opp"]);
        let raw_opp = f.raw_stages.iter().find(|s| s.key == "opp").unwrap();
        assert_eq!(raw_opp.value, 10.0, "生値は書き換えずに併記する");
        // 引き上げが必要ない段は clamped_stages に入らない
        assert!(!f.clamped_stages.contains(&"dial"));
    }

    #[test]
    fn n回目架電の20プラス行を落とさない() {
        // シートの attempt_no は最終行だけ文字列 "20+"。GAS は Number("20+")=NaN → 0 で
        // 判定から落ちるため、この行(実データで __all__ 7,592架電/84アポ)が
        // 毎回消えていた。数値でない表記も拾う。
        let d = sheet(
            &["attempt_no", "pipeline", "total", "apo_count"],
            &[
                &["19", "__all__", "1000", "10"],
                &["20+", "__all__", "7592", "84"],
            ],
        );
        let td = build_touch_distribution(&d, None);
        assert_eq!(td.buckets[19].label, "20+");
        assert_eq!(td.buckets[19].total, 7592.0, "20+ 行を黙って捨てない");
        assert!((td.buckets[19].apo_rate.unwrap() - 1.1064_f64).abs() < 0.001);
        assert_eq!(td.buckets[18].total, 1000.0, "19 は 19 のまま");
    }

    #[test]
    fn ファネルの期間セレクタが当月含みで効く() {
        // "3m" = 当月含め3ヶ月 → 2ヶ月前が下限（GAS `ymOffset(2)` と同じ）
        assert_eq!(funnel_range(Some("3m"), "2026-08").0, "2026-06");
        assert_eq!(funnel_range(Some("12m"), "2026-08").0, "2025-09");
        // 年跨ぎで月が壊れないこと
        assert_eq!(funnel_range(Some("6m"), "2026-02").0, "2025-09");
        assert_eq!(funnel_range(Some("all"), "2026-08").0, "all");
        assert_eq!(funnel_range(None, "2026-08").0, FUNNEL_DIAL_START_YM);
    }

    #[test]
    fn 滞留日数は中央値降順で30件に切る() {
        let mut raw: Vec<Vec<String>> = Vec::new();
        for i in 0..33 {
            raw.push(vec![
                "PL".to_string(),
                format!("stage{i:02}"),
                format!("{}", i + 1), // median_days
                "5".to_string(),      // n_samples（全部 10未満 = 低信頼）
            ]);
        }
        let refs: Vec<Vec<&str>> = raw
            .iter()
            .map(|r| r.iter().map(|s| s.as_str()).collect())
            .collect();
        let slices: Vec<&[&str]> = refs.iter().map(|r| r.as_slice()).collect();
        let d = sheet(
            &["pipeline", "stage_label", "median_days", "n_samples"],
            &slices,
        );
        let s = build_stage_dwell(&d);
        assert_eq!(s.rows.len(), TOP_N_DWELL);
        assert_eq!(s.total_rows, 33);
        assert!(s.truncated);
        assert_eq!(s.rows[0].median_days, 33.0, "滞留の長い順");
        assert!(s.rows[0].low_confidence, "n<10 は低信頼として旗を立てる");
        assert!(s.z_self_computed, "bottleneck_z 列が無いので自前計算に落ちる");
    }

    #[test]
    fn その場失注は営業既定スコープで集計しowner0名なら平均をnullにする() {
        let d = sheet(
            &["owner_id", "year_month", "on_the_spot_lost_count"],
            &[
                &["1", "2026-06", "3"],
                &["1", "2026-07", "2"],
                &["2", "2026-06", "4"],
                &["3", "2026-06", "99"], // BPO。未選択時は入らない
            ],
        );
        let m = load_members(&members_sheet());
        let s = build_on_the_spot(&d, &m, &q_default());
        assert_eq!(s.total_count, 9.0, "BPO の 99 件が混ざっていない");
        assert_eq!(s.owner_count, 2);
        assert_eq!(s.rows[0].owner_id, "1", "件数降順");
        assert_eq!(s.rows[0].count, 5.0);
        assert_eq!(s.avg_per_owner, Some(4.5));

        // 期間で全部落ちたケース → 平均は 0 ではなく null
        let q = P2Query {
            from: Some("2030-01-01".into()),
            ..Default::default()
        };
        let empty = build_on_the_spot(&d, &m, &q);
        assert_eq!(empty.owner_count, 0);
        assert_eq!(empty.avg_per_owner, None, "0名を「平均0件」と出さない");
    }

    #[test]
    fn 商談遷移は主要ステージ以外と自己遷移を落とす() {
        let summary = sheet(
            &[
                "stage_from",
                "stage_to",
                "stage_from_label",
                "stage_to_label",
                "transition_count",
                "share_within_from_pct",
                "lead_time_p25_days",
                "lead_time_p50_days",
                "lead_time_p75_days",
            ],
            &[
                // アポ日確定 → 進捗確認
                &[
                    "52035886", "52035887", "アポ日確定", "進捗確認(商談実施済)", "100", "62.5",
                    "1", "3", "9",
                ],
                // アポ日確定 → 失注（即失注）
                &[
                    "52035886",
                    "155012220",
                    "アポ日確定",
                    "商談済リード(失注)",
                    "60",
                    "37.5",
                    "2",
                    "5",
                    "12",
                ],
                // 失注 → 失注（ラベル統合の副作用。落とす）
                &[
                    "155012220",
                    "155012221",
                    "商談済リード(失注)",
                    "商談済リード(失注)",
                    "30",
                    "100",
                    "",
                    "",
                    "",
                ],
                // 他PLのステージ（主要ステージ外。落とす）
                &[
                    "99999999", "88888888", "未知(99999999)", "未知(88888888)", "500", "100", "",
                    "", "",
                ],
            ],
        );
        let cross = sheet(
            &["industry_jsic", "size_band", "stage_from", "stage_to", "transition_count"],
            &[&["運輸業", "50-99人", "52035886", "52035887", "5"]],
        );
        let t = build_stage_transition(&summary, &cross, &q_default());
        assert!(!t.cross_active);
        assert_eq!(t.pairs_before_filter, 4);
        assert_eq!(t.pairs.len(), 2, "自己遷移と他PLステージを落とす");
        assert_eq!(t.total_transitions, 160.0);

        // KPI: アポ確定 160件中 100件が進捗確認 → 62.5% / 即失注 60件 → 37.5%
        let prog = t.kpis.iter().find(|k| k.key == "apo_to_progress").unwrap();
        assert_eq!(prog.value, Some(62.5));
        let lost = t.kpis.iter().find(|k| k.key == "apo_to_lost").unwrap();
        assert_eq!(lost.value, Some(37.5));
        // 進捗確認 起点の遷移が1本も無い → 分母0なので「0%」ではなく null
        let a = t.kpis.iter().find(|k| k.key == "progress_to_a").unwrap();
        assert_eq!(a.value, None, "分母0の直行率を 0% と表示してはいけない");
        // リードタイム: (3*100 + 5*60) / 160 = 3.75日
        let lt = t.kpis.iter().find(|k| k.key == "apo_lead_time").unwrap();
        assert_eq!(lt.value, Some(3.75));
    }

    #[test]
    fn 商談遷移のクロス絞込ではp25とp75をnullにする() {
        let summary = sheet(
            &[
                "stage_from",
                "stage_to",
                "stage_from_label",
                "stage_to_label",
                "transition_count",
                "share_within_from_pct",
                "lead_time_p25_days",
                "lead_time_p50_days",
                "lead_time_p75_days",
            ],
            &[&[
                "52035886",
                "52035887",
                "アポ日確定",
                "進捗確認(商談実施済)",
                "100",
                "100",
                "1",
                "3",
                "9",
            ]],
        );
        let cross = sheet(
            &[
                "industry_jsic",
                "size_band",
                "stage_from",
                "stage_to",
                "stage_from_label",
                "stage_to_label",
                "transition_count",
                "lead_time_p50_days",
            ],
            &[
                &["運輸業", "50-99人", "52035886", "52035887", "", "", "8", "4"],
                &["運輸業", "100-499人", "52035886", "52035887", "", "", "2", "9"],
                &["建設業", "50-99人", "52035886", "52035887", "", "", "99", "1"],
            ],
        );
        let q = P2Query {
            trans_industry: Some("運輸業".into()),
            ..Default::default()
        };
        let t = build_stage_transition(&summary, &cross, &q);
        assert!(t.cross_active);
        assert_eq!(t.pairs.len(), 1);
        assert_eq!(t.pairs[0].count, 10.0, "建設業は絞込で除外");
        // ラベル列が空でも fallback マップで日本語化される
        assert_eq!(t.pairs[0].from_label, "アポ日確定");
        // p50 は件数加重平均 (4*8 + 9*2)/10 = 5.0
        assert_eq!(t.pairs[0].lead_time_p50, Some(5.0));
        assert_eq!(t.pairs[0].lead_time_p25, None, "クロス側に p25 は無い。0で埋めない");
        assert_eq!(t.pairs[0].lead_time_p75, None);
        // 絞込後の from 内シェアは 100%
        assert_eq!(t.pairs[0].share_within_from, Some(100.0));
        // セレクタ候補は安定した並びで返る
        assert_eq!(t.industries, vec!["建設業", "運輸業"]);
        assert_eq!(t.size_bands, vec!["50-99人", "100-499人"]);
    }

    #[test]
    fn n回目架電はpl選択が無ければallに落ちる() {
        let d = sheet(
            &["attempt_no", "pipeline", "total", "apo_count"],
            &[
                &["1", "__all__", "1000", "20"],
                &["1", "商談PL", "100", "5"],
                &["25", "__all__", "50", "1"],
            ],
        );
        let all = build_touch_distribution(&d, None);
        assert_eq!(all.pipeline_used, "__all__");
        assert!(!all.fell_back_to_all);
        assert_eq!(all.buckets[0].total, 1000.0);
        // attempt_no 25 は 20+ バケットへ
        assert_eq!(all.buckets[19].total, 50.0);

        let picked = build_touch_distribution(&d, Some("商談PL"));
        assert_eq!(picked.pipeline_used, "商談PL");
        assert_eq!(picked.buckets[0].total, 100.0);

        // 存在しない PL を指定 → __all__ に落ちるが、落ちたことを必ず知らせる
        let missing = build_touch_distribution(&d, Some("存在しないPL"));
        assert_eq!(missing.pipeline_used, "__all__");
        assert!(missing.fell_back_to_all, "黙って別PLの数字を出さない");
    }

    #[test]
    fn ベンチマークはgas版と同じ定義で計算する() {
        // quantilesOf: 平均 と floor(n*0.75) 番目（0始まり）を Q3 とする
        let b = quantiles(&[10.0, 20.0, 30.0, 40.0]);
        assert_eq!(b.n, 4);
        assert_eq!(b.mean, Some(25.0));
        assert_eq!(b.q3, Some(40.0), "floor(4*0.75)=3 → 4番目の値");
        let empty = quantiles(&[]);
        assert_eq!(empty.mean, None, "対象0名を平均0%と出さない");
    }

    #[test]
    fn zスコアは母集団標準偏差で全員同値なら0になる() {
        let z = zscores(&[5.0, 5.0, 5.0]);
        assert_eq!(z, vec![0.0, 0.0, 0.0], "sd=0 でゼロ除算しない");
        let z2 = zscores(&[0.0, 10.0]);
        assert_eq!(z2, vec![-1.0, 1.0]);
    }

    #[test]
    fn 都道府県モードでは分母がhubspot_callに切り替わる() {
        // 都道府県月次には Zoom 行動量が無いので分母が変わる。
        // 変わったことをラベルで必ず出す（「アポ率」の単独表記は規約で禁止）。
        let r = MonthlyRow {
            call_count: 300.0,
            zoom_dial_count: 900.0,
            ..Default::default()
        };
        assert_eq!(apo_denominator(&r, false), 900.0);
        assert_eq!(apo_denominator(&r, true), 300.0);
        assert!(apo_denominator_label(true).contains("HubSpot Call"));
        // Zoom 発信の記録が無い担当者は HubSpot Call に落ちる（GAS と同じ）
        let no_zoom = MonthlyRow {
            call_count: 120.0,
            zoom_dial_count: 0.0,
            ..Default::default()
        };
        assert_eq!(apo_denominator(&no_zoom, false), 120.0);
    }

    #[test]
    fn 列は名前で引くので列順が変わっても壊れない() {
        // feature_monthly.csv の列は Python 側の都合で増減する。
        // 位置決め打ちだと G列を会社名と誤読した事故の再来になる。
        let shuffled = sheet(
            &["year_month", "na_due", "owner_id", "na_done_ontime", "pipeline"],
            &[&["2026-06", "200", "1", "100", "PL_A"]],
        );
        let m = load_members(&members_sheet());
        let rows = scope_monthly(&shuffled, &m, &q_default());
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].owner_id, "1");
        assert_eq!(rows[0].na_due, 200.0);
        assert_eq!(rows[0].na_done_ontime, 100.0);
        // 無い列（zoom_dial_count 等）は 0 として扱う（パニックしない）
        assert_eq!(rows[0].zoom_dial_count, 0.0);
    }
}
