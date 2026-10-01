//! 営業KPI: `GET /api/sales-kpi/data` の応答型（React 移行 W2、2026-09-30）
//!
//! `build_payload()` が組み立てる JSON の形を、そのまま struct にしたもの。
//! それまでは `serde_json::Value` (`json!`) で組んでいた。置き換えで
//! **JSON は 1 バイトも変えていない**。`tests::payloadのjsonは置換前のスナップショットと一致する`
//! が、置き換える前のコードで書き出した `tests/fixtures/sales_kpi/payload_2026-09-04.json`
//! と `serde_json::to_string` の結果を丸ごと突き合わせる。
//!
//! 🔴 **フィールドの並びは旧 `json!` の並びのまま**にしてある。serde はフィールドを
//!    宣言順に書くので、並びを変えると JSON のキー順が変わり、上のテストが落ちる。
//!    キーが動的な map は `BTreeMap`（キー順）か [`OrderedMap`]（挿入順）で、
//!    旧コードがどちらの順で出していたかに合わせてある。
//!
//! ts-rs で `frontend/src/generated/SalesKpi*.ts` に書き出す
//! （`handlers::app_api::tests::export_ts_bindings`）。生成物の名前は他の画面の型と
//! 衝突しないよう、すべて `SalesKpi` を前置する（`#[ts(rename)]`）。
//! `i64` は JSON では普通の数なので、書き出しは `with_large_int("number")` で
//! `bigint` にしない。

use std::collections::BTreeMap;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};
use ts_rs::TS;

use super::{Counts, DealRow, KadenPeriod, Person};

/// 画面がそのまま使える JSON 1 本。`build_payload()` の戻り値。
#[derive(Debug, Clone, Serialize, TS)]
pub struct SalesKpiData {
    /// `KPI営業_取得条件` の「取得時刻」（`yyyy-MM-dd HH:mm`）。無ければ判定日の `yyyy-MM-dd`。
    pub generated_at: String,
    pub week: DateSpan,
    pub next_week: DateSpan,
    pub stale_days: i64,
    pub bpo_rule: &'static str,
    pub teams: Vec<String>,
    pub by_team: BTreeMap<String, Counts>,
    pub by_person: BTreeMap<String, Counts>,
    pub people: Vec<Person>,
    pub bpo_total: Counts,
    pub stale: Vec<DealRow>,
    pub week_deals: Vec<DealRow>,
    pub next_week_deals: Vec<DealRow>,
    pub anq_missing: Vec<DealRow>,
    pub cyomi_stale: Vec<DealRow>,
    /// 「今月の成績」カードの内訳の行（件数と同じ行・同じ述語から作る）。
    pub card_deals: CardDeals,
    /// 商談の集計から外した件数。`件数` と HubSpotチーム別。
    pub excluded: Counts,
    pub kaden: Kaden,
    pub kaden_base: i64,
    pub kettei: Kettei,
    pub list_stock: ListStock,
    pub calls: Calls,
    pub snapshots: Vec<Snapshot>,
    pub meta: BTreeMap<String, String>,
    pub from_cache: bool,
}

/// カード内訳の行。pool = ③ 当月の母集団、apo = ① アポシート全行、cyomi = ⑨ Cヨミシート全行。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiCardDeals")]
pub struct CardDeals {
    pub pool: Vec<DealRow>,
    pub apo: Vec<DealRow>,
    pub cyomi: Vec<DealRow>,
}

/// `yyyy-MM-dd` の両端を含む範囲。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiDateSpan")]
pub struct DateSpan {
    pub start: String,
    pub end: String,
}

// ---------------------------------------------------------------- 架電リスト

/// 架電リスト（アポ前パイプライン）の状態。`routes::kaden_list_block` が作る。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKaden")]
pub struct Kaden {
    pub composition: Vec<KadenComposition>,
    /// 画面のカードが使う「全社」。営業チームの合計（担当者別シートが無ければリスト全体）。
    pub base: i64,
    pub cls: Counts,
    pub total: i64,
    pub fill: Counts,
    /// アポ前パイプライン全体（従来の「全社」）。注記と母数の推移に使う。
    pub all: KadenAll,
    /// まだ営業チームに配られていない分。
    pub unassigned: KadenUnassigned,
    pub by_person: BTreeMap<String, Counts>,
    pub by_team: BTreeMap<String, Counts>,
    pub no_owner: Counts,
    pub counted_base: i64,
    pub has_by_owner: bool,
    /// 前の週の記録との差。記録が無ければ `null`。
    pub base_trend: Option<KadenBaseTrend>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKadenComposition")]
pub struct KadenComposition {
    pub stage: String,
    pub cls: String,
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKadenAll")]
pub struct KadenAll {
    pub cls: Counts,
    pub base: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKadenUnassigned")]
pub struct KadenUnassigned {
    pub cls: Counts,
    pub base: i64,
    pub no_owner: i64,
    pub people: Vec<UnassignedPerson>,
}

/// まだ配られていない在庫を持っている人。`hsTeam` は空でもキーを出す（`Person` と違う）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiUnassignedPerson")]
pub struct UnassignedPerson {
    pub id: String,
    pub name: String,
    pub team: String,
    #[serde(rename = "hsTeam")]
    pub hs_team: String,
    pub base: i64,
    #[serde(rename = "未架電")]
    pub not_called: i64,
    #[serde(rename = "未接触")]
    pub not_reached: i64,
    #[serde(rename = "接触済み")]
    pub reached: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKadenBaseTrend")]
pub struct KadenBaseTrend {
    pub week: String,
    pub week_start: String,
    pub base: i64,
    pub diff: i64,
}

// ---------------------------------------------------------------- 決定者・決裁者

/// 決定者・決裁者の入力状況。`routes::kettei_block` が作る。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKettei")]
pub struct Kettei {
    /// いつ時点の入力件数か（`yyyy-MM-dd`）。行が無ければ `null`。
    pub date: Option<String>,
    /// 「増加」が何との差か。1日ぶんしか無ければ `null`。
    pub prev_date: Option<String>,
    /// 画面が出す列（`KETTEI_COLS` の右側）。
    pub cols: Vec<&'static str>,
    pub rows: Vec<KetteiRow>,
    /// 担当者が入っていない取引ぶん。無ければ `null`。
    pub no_owner: Option<KetteiCells>,
}

/// 1 人ぶん（または担当なしぶん）の数字。
///
/// 🔴 列の並びは `KETTEI_COLS` と同じ（決定者名 / 決定者役職 / 決裁者名 / 決裁者役職）。
///    `tests::決定者の列は表と行で揃っている` が、この struct のキーと `KETTEI_COLS` を突き合わせる。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKetteiCells")]
pub struct KetteiCells {
    #[serde(rename = "決定者名")]
    pub decider_name: i64,
    #[serde(rename = "決定者役職")]
    pub decider_title: i64,
    #[serde(rename = "決裁者名")]
    pub approver_name: i64,
    #[serde(rename = "決裁者役職")]
    pub approver_title: i64,
    #[serde(rename = "合計")]
    pub total: i64,
    /// 前の記録との差。前の記録が無い担当者は `null`（0 ではない）。
    #[serde(rename = "増加")]
    pub grew: Option<i64>,
}

impl KetteiCells {
    /// `kettei_counts()` の数え上げから作る。キーは `KETTEI_COLS` の右側と `合計`。
    pub fn from_counts(counts: &Counts, grew: Option<i64>) -> Self {
        let get = |key: &str| counts.get(key).copied().unwrap_or(0);
        Self {
            decider_name: get("決定者名"),
            decider_title: get("決定者役職"),
            approver_name: get("決裁者名"),
            approver_title: get("決裁者役職"),
            total: get("合計"),
            grew,
        }
    }
}

/// 担当者 1 人の行。数字（`KetteiCells`）を先に、担当者の情報を後ろに出す。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiKetteiRow")]
pub struct KetteiRow {
    #[serde(flatten)]
    pub cells: KetteiCells,
    pub owner: String,
    #[serde(rename = "ownerName")]
    pub owner_name: String,
    pub team: String,
    #[serde(rename = "hsTeam")]
    pub hs_team: String,
}

// ---------------------------------------------------------------- リストの在庫

/// 新規営業のリストの在庫。`list_stock_of()` が作り、`routes::list_stock_block` が `trend` を足す。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiListStock")]
pub struct ListStock {
    /// 企業人数で絞らない数を表す帯の名前（`STOCK_ALL_BANDS`）。
    pub all_band: &'static str,
    pub bands: Vec<String>,
    pub has_named: bool,
    pub lists: Vec<StockList>,
    /// 前の週の記録。無ければ `null`。
    pub trend: Option<StockTrend>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiStockList")]
pub struct StockList {
    pub name: String,
    /// 帯 → 件数（リスト全体）。
    pub total: Counts,
    pub total_named: Counts,
    pub groups: Vec<StockGroup>,
    /// 合計 − 内訳の和。
    pub other: Counts,
    pub other_named: Counts,
    pub band_gap: i64,
    pub band_gap_named: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiStockGroup")]
pub struct StockGroup {
    pub kind: String,
    pub name: String,
    pub counts: Counts,
    pub named: Counts,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiStockTrend")]
pub struct StockTrend {
    pub week: String,
    pub week_start: String,
    /// リスト名 → 全体／アクティブ／保管。週次シートの見出しの順。
    #[ts(as = "BTreeMap<String, StockTrendList>")]
    pub lists: OrderedMap<StockTrendList>,
}

/// 週次シートに残っている在庫（企業人数で絞らない数だけ）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiStockTrendList")]
pub struct StockTrendList {
    #[serde(rename = "全体")]
    pub whole: i64,
    #[serde(rename = "アクティブ")]
    pub active: i64,
    #[serde(rename = "保管")]
    pub stored: i64,
}

// ---------------------------------------------------------------- Zoom の架電

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiCalls")]
pub struct Calls {
    /// シートに入っている最後の日（`last_day` と同じ。古い画面との互換で残している）。
    pub generated_at: String,
    pub last_day: String,
    pub last_day_partial: bool,
    /// いつ Zoom から取ったか（`KPI営業_取得条件` の「架電の取得時刻」）。無ければ空。
    pub fetched_at: String,
    pub rule: CallsRule,
    pub periods: CallPeriods,
    pub daily: Vec<CallsDaily>,
    pub people: Vec<Person>,
    pub unmatched_by_dept: BTreeMap<String, i64>,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiCallsRule")]
pub struct CallsRule {
    pub calls: &'static str,
    pub connected: &'static str,
    pub long: &'static str,
    pub join: &'static str,
}

/// 期間ごとの集計。キー名が画面の chip と対応する（`this_week` が既定）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiCallPeriods")]
pub struct CallPeriods {
    pub today: KadenPeriod,
    pub yesterday: KadenPeriod,
    pub this_week: KadenPeriod,
    pub prev_week_same: KadenPeriod,
    pub prev_week: KadenPeriod,
    pub this_month: KadenPeriod,
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiCallsDaily")]
pub struct CallsDaily {
    pub date: String,
    pub calls: i64,
    pub connected: i64,
    pub long: i64,
}

// ---------------------------------------------------------------- 週次の記録

/// `KPI営業_週次` の 1 行。`snapshots_of()` が作る。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiSnapshot")]
pub struct Snapshot {
    /// `2026-W36` の形。
    pub week: String,
    pub taken_at: String,
    pub week_start: String,
    /// 当月に商談予定日があるもの（月内は積み上がり、月初に入れ替わる）。
    pub totals: SnapshotTotals,
    pub stale: i64,
    pub anq_missing: i64,
    pub cyomi_stale: i64,
    pub kaden_called: i64,
    pub kaden_base: i64,
    /// 架電数だけは「まだ無い」と「0件」を分ける。無ければ `null`。
    pub zoom_called: Option<i64>,
    pub zoom_days: i64,
    pub zoom_partial: bool,
    /// その週（月〜日）に商談予定日があるもの。列が無い古い行は `null`。
    pub week_totals: Option<SnapshotWeekTotals>,
    pub week_partial: bool,
    /// リスト名 → 全体／アクティブ／保管。列が無い古い行は `null`。
    #[ts(as = "Option<BTreeMap<String, StockTrendList>>")]
    pub list_stock: Option<OrderedMap<StockTrendList>>,
}

/// 週次シートの列 → 画面が読むキー（当月ぶん）。
///
/// 🔴 シートの見出し（`from_row` の左側）は Python 側（Hubspot リポジトリ
/// `scripts/sales_kpi/sync_daily.py` の `WEEKLY_HEADER`）と対で決まっている。
/// 片方だけ変えると値が 0 で並ぶ。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiSnapshotTotals")]
pub struct SnapshotTotals {
    pub pool: i64,
    #[serde(rename = "実施")]
    pub done: i64,
    #[serde(rename = "未実施")]
    pub not_done: i64,
    #[serde(rename = "未処理")]
    pub stuck: i64,
    #[serde(rename = "これから")]
    pub upcoming: i64,
    #[serde(rename = "要判定")]
    pub unknown: i64,
    pub apo: i64,
    pub cyomi: i64,
    pub bpo_pool: i64,
}

/// 週次シートの「週_」列 → 画面が読むキー（その週ぶん、2026-09-07 追加）。
#[derive(Debug, Clone, Serialize, TS)]
#[ts(rename = "SalesKpiSnapshotWeekTotals")]
pub struct SnapshotWeekTotals {
    pub pool: i64,
    #[serde(rename = "実施")]
    pub done: i64,
    #[serde(rename = "未実施")]
    pub not_done: i64,
    #[serde(rename = "未処理")]
    pub stuck: i64,
    #[serde(rename = "これから")]
    pub upcoming: i64,
    #[serde(rename = "要判定")]
    pub unknown: i64,
}

// ---------------------------------------------------------------- 挿入順の map

/// 挿入順を保つ map。JSON では object になる。
///
/// 旧コードが `serde_json::Map`（`preserve_order`）で組んでいた所に使う。
/// `BTreeMap` にするとキー順に並び替わって JSON が変わる（週次シートの
/// 「リスト_<名前>_全体」列は見出しの順で出していた）。
///
/// TS 型は持たない。使う側のフィールドで `#[ts(as = "BTreeMap<String, V>")]` を付ける。
#[derive(Debug, Clone, Default)]
pub struct OrderedMap<V>(pub Vec<(String, V)>);

impl<V> OrderedMap<V> {
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// 同じキーがあれば**その場所で**値を入れ替える（`serde_json::Map::insert` と同じ）。
    pub fn insert(&mut self, key: String, value: V) {
        match self.0.iter_mut().find(|(k, _)| *k == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
    }

    pub fn get(&self, key: &str) -> Option<&V> {
        self.0.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &V)> {
        self.0.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl<V: Serialize> Serialize for OrderedMap<V> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}
