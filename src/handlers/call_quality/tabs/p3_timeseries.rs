//! 架電クオリティ: 時系列（GAS 版 page-p3）
//!
//! 2026-08-16 移植。GAS 版 index.html `<div class="page" id="page-p3">` の構成:
//!   スコアカード(直近完了月/前月比) / 月次 架電数推移 / 月次 アポ率推移 /
//!   曜日別 架電数 / 曜日別 アポ率 / メンバー別 月次架電推移(ヒートマップ) /
//!   アポ取得月ごとの その後の進み具合(コホート) / 時間帯×曜日 ヒートマップ /
//!   アポ率 母数2版比較
//!
//! 読むシートと対応（どのパネルがどこから来ているか）:
//!   「月次明細」          → スコアカード / 月次架電数 / 月次アポ率 / メンバー別ヒート
//!   「曜日別集計」        → 曜日別 架電数 / 曜日別 アポ率
//!   「コホート分析」      → アポ取得月ごとの その後の進み具合
//!   「時間帯ヒート_クロス」→ 時間帯×曜日 ヒートマップ（`heatmap::handle` を再利用）
//!   「時間帯ヒート」      → アポ率 母数2版比較（zoom_dial_count はこのシートにしかない）
//!
//! ## 未実装（黙って省略しない）
//!
//! - **アポ定義変更の段差注記**（GAS `renderApoDefNote`）: 2026-08-13 のアポ定義変更で
//!   「今日 −190日」より前の月が旧定義のまま残る、という注記。境界月は実行時に決まる
//!   表示専用テキストで、集計値には影響しない。サーバ側の責務ではないと判断して外した
//!   （画面側で `features.py` の `_monthly_since` と同じ 190日から算出すること）。
//! - **時間帯ヒートの「Call記録率」指標**: `heatmap::HeatCell` が dial/connect/apo を
//!   そのまま返すので、率の切替（架電回数 / Call記録率 / アポ率）は画面側で計算できる。
//!   サーバ側で指標を1つに固定すると切替のたびに往復が要るため、あえて生値のまま返す。
//! - **母数2版比較の 業界/都道府県クロス絞込**: 「時間帯ヒート_クロス」に
//!   `zoom_dial_count` 列が無く、Zoom発信の母数が取れない。GAS 版も同じ理由で
//!   クロス絞込中は Zoom 側をフォールバック表示にしていた。ここでは
//!   **HubSpot 側だけ絞り込まれた片肺の比較を出さない**ため、パネルごと
//!   「利用不可＋理由」を返す（GAS との差異。理由は `DenominatorCompare` を参照）。
//! - **都道府県モードの行の差し替え**: GAS はここで参照シートを
//!   「都道府県月次」へ切り替えていた。本実装は p0/p1 と同じく
//!   **分母だけ HubSpot Call に切り替える**（`apo_denominator`）。
//!   都道府県で行を絞る実装は「都道府県月次」シートの移植が要るため未着手。

use std::collections::HashMap;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::heatmap::{self, HeatCell, HeatmapCache, HeatmapQuery};
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

/// メンバー別ヒートマップに載せる人数。GAS 版と同じ「架電数Top20」。
/// 切ったら `truncated` を立てる（約束3）。
const MEMBER_HEATMAP_TOP_N: usize = 20;

/// コホート表に載せるコホート月数。GAS 版と同じ「直近6ヶ月」。
const COHORT_MONTHS: usize = 6;
/// コホート表の横軸の最大経過月。GAS 版 `maxLag = 5`（アポ当月〜5ヶ月後の6列）。
const COHORT_MAX_LAG: u8 = 5;

/// 曜日別アポ率の最低架電数。
///
/// GAS 版 `MIN_DOW_CALLS = 100`。土日は母数が極小（土30件/日5件）で、
/// アポ帰属日（stage 変更日）と架電日が一致しないため率が 100% を超える等の
/// 異常値になる。他チャートの足切り（MIN_CALLS_FOR_RATE=100）と揃えてある。
///
/// **GAS 版はこの行を配列から落としていたが、ここでは落とさず `thin` を立てる。**
/// p1 で確立した「合計と内訳を一致させる。事実は見せて順位だけ鵜呑みにさせない」に合わせる。
const MIN_CALLS_FOR_DOW_RATE: f64 = 100.0;

const WEEKDAY_LABELS: [&str; 7] = ["月", "火", "水", "木", "金", "土", "日"];

// ------------------------------------------------------------------ クエリ

#[derive(Debug, Default, Deserialize)]
pub struct TimeseriesQuery {
    /// 期間の下限 (YYYY-MM, 含む)。未指定なら制限なし。
    pub from_ym: Option<String>,
    /// 期間の上限 (YYYY-MM, 含む)。未指定なら制限なし。
    pub to_ym: Option<String>,
    /// 都道府県。指定すると**アポ率の分母が Zoom発信 → HubSpot Call に切り替わる**。
    /// p0/p1 の `apo_denominator()` と同じ規則（是正3）。
    pub prefecture: Option<String>,
    /// 業種。時間帯×曜日ヒートマップのクロス絞込にのみ効く。
    pub industry: Option<String>,
    /// カンマ区切り owner_id。未指定なら role=sales のみ（約束5）。
    pub owners: Option<String>,
    /// コホート表の指標。`progression`(既定) / `retention`。
    /// 値は英語キーで受けるが、**返すラベルは日本語**（是正2）。
    pub cohort_metric: Option<String>,
    /// 「進行中の当月」を判定する基準月 (YYYY-MM)。未指定なら実行時の当月。
    /// テストから固定値を入れるために外出ししてある。
    pub current_ym: Option<String>,
}

// ------------------------------------------------------------------ 返却型

#[derive(Debug, Serialize)]
pub struct TimeseriesData {
    pub scorecard: Scorecard,
    /// 月次 架電数推移 / 月次 アポ率推移（同じ配列を2チャートで使う）
    pub monthly: Vec<MonthlyPoint>,
    /// アポ率の分母が何だったか。「アポ率」の単独表記は運用ルールで禁止（是正3）。
    pub apo_denominator_label: String,
    /// 曜日別（架電数チャート・アポ率チャート共通の7行）
    pub weekday: Vec<WeekdayPoint>,
    /// 曜日別アポ率の足切り。画面はこの値未満（`thin=true`）を率チャートから外す。
    pub weekday_min_calls: f64,
    /// 曜日別シートの分母。Zoom発信の列が無いので HubSpot Call 固定。
    pub weekday_denominator_label: String,
    /// 曜日別は全期間・全PL・全メンバーのスナップショット（フィルタ非反映）。
    pub weekday_note: String,
    pub member_heatmap: MemberHeatmap,
    pub cohort: Cohort,
    pub hour_weekday: HourWeekday,
    pub apo_denominator_compare: DenominatorCompare,
    /// この応答で実際に使われたスコープの説明（誰を集計したか）。
    pub scope_label: String,
}

#[derive(Debug, Serialize, Default)]
pub struct Scorecard {
    /// 直近**完了**月（進行中の当月は入らない）
    pub last_month: Option<String>,
    pub prev_month: Option<String>,
    pub last_call_count: f64,
    /// 分母0なら null
    pub last_apo_rate: Option<f64>,
    /// 前月比（架電数）。前月が無ければ null
    pub delta_call_count: Option<f64>,
    /// 前月比（アポ率, pt）。どちらかが null なら null
    pub delta_apo_rate_pt: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct MonthlyPoint {
    pub year_month: String,
    pub call_count: f64,
    pub zoom_dial_count: f64,
    pub apo_count: f64,
    /// 率の分母そのもの
    pub denominator: f64,
    /// 分母0なら null（0% にしない = 約束2）
    pub apo_rate: Option<f64>,
}

#[derive(Debug, Serialize, Clone)]
pub struct WeekdayPoint {
    /// 0=月 .. 6=日（Python `datetime.weekday()` 準拠）。
    /// GAS 版はここで JS `Date.getDay()`(日=0) と混同し、全曜日が1つズレていた事故がある。
    pub weekday: u8,
    pub label: String,
    pub call_count: f64,
    pub apo_count: f64,
    /// 分母0なら null
    pub apo_rate: Option<f64>,
    pub na_due: f64,
    pub active_days: f64,
    /// 架電数が足切り未満。率チャートはこの行を出さないこと。
    pub thin: bool,
}

#[derive(Debug, Serialize)]
pub struct MemberHeatmap {
    /// 列（月）。昇順。
    pub months: Vec<String>,
    /// 行（メンバー）。架電数合計の降順、同数は owner_id で安定化。
    pub members: Vec<MemberSeries>,
    /// Top N で切ったか（約束3）
    pub truncated: bool,
    pub top_n: usize,
    /// 切る前の人数
    pub total_members: usize,
}

#[derive(Debug, Serialize)]
pub struct MemberSeries {
    pub owner_id: String,
    pub total_call_count: f64,
    /// `months` と同じ長さ。欠測は 0（架電が無かった月）。
    pub call_counts: Vec<f64>,
}

#[derive(Debug, Serialize)]
pub struct Cohort {
    /// 指標の**日本語**名（是正2）。英語キーは画面に出さない。
    pub metric_label: String,
    /// 横軸ラベル。`アポ当月 / 1ヶ月後 / 2ヶ月後 ...`（是正2）
    pub lag_labels: Vec<String>,
    /// 縦軸（アポが取れた月）。昇順。
    pub rows: Vec<CohortRow>,
    /// 直近6ヶ月で切ったか（約束3）
    pub truncated: bool,
    pub total_cohort_months: usize,
    /// このコホートの起点。GAS 版と同じく anchor='apo' のみ。
    pub anchor: String,
    /// 画面の説明文に使う一行（現場が読んで分かる言い方）
    pub description: String,
}

#[derive(Debug, Serialize)]
pub struct CohortRow {
    /// アポが取れた月 (YYYY-MM)
    pub cohort_month: String,
    /// `lag_labels` と同じ長さ。データが無いマスは null（0% にしない）。
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Serialize)]
pub struct HourWeekday {
    /// 曜日×時間帯のセル。値のあるセルのみ（`heatmap::aggregate` の仕様）。
    pub cells: Vec<HeatCell>,
    /// 絞り込み後に集計へ載った元行数
    pub source_rows: usize,
    /// 「時間帯ヒート_クロス」の総行数
    pub total_rows: usize,
    /// 常駐キャッシュから返したか。
    /// **注**: `heatmap::HeatmapResponse` は取得時刻を公開していないため、
    /// このパネルだけ `SourceInfo`(age_secs) を出せない。ここで代替して返す。
    pub from_cache: bool,
    /// 誰を集計したか（是正1: メンバー未選択時に BPO/コンサルを混ぜない）
    pub scope_label: String,
}

#[derive(Debug, Serialize)]
pub struct DenominatorCompare {
    /// このパネルが成立するか。false なら `cells` は空で `reason` に理由が入る。
    pub applicable: bool,
    pub reason: Option<String>,
    pub cells: Vec<DenominatorCell>,
    /// ① の分母合計（HubSpot コールログ）
    pub total_hubspot_dial: f64,
    /// ② の分母合計（Zoom Phone 発信）
    pub total_zoom_dial: f64,
    pub scope_label: String,
}

#[derive(Debug, Serialize)]
pub struct DenominatorCell {
    pub weekday: u8,
    pub weekday_label: String,
    pub hour: u8,
    pub apo_count: f64,
    pub hubspot_dial: f64,
    pub zoom_dial: f64,
    /// ① アポ ÷ HubSpot コールログ。分母0なら null
    pub apo_rate_hubspot: Option<f64>,
    /// ② アポ ÷ Zoom Phone 発信。分母0なら null
    pub apo_rate_zoom: Option<f64>,
}

// ------------------------------------------------------------------ 小道具

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// アポ率の分母。p0/p1 の `apo_denominator` と同一規則（揃えること、と指示あり）。
///   都道府県モード → HubSpot Call
///   通常          → Zoom発信（無ければ HubSpot Call にフォールバック）
fn apo_denominator(call: f64, zoom: f64, pref_mode: bool) -> f64 {
    if pref_mode {
        call
    } else if zoom > 0.0 {
        zoom
    } else {
        call
    }
}

fn denominator_label(pref_mode: bool) -> String {
    if pref_mode {
        "HubSpot Call（都道府県で絞り込み中）".to_string()
    } else {
        "Zoom発信（行動量分母）".to_string()
    }
}

fn pref_mode_of(q: &TimeseriesQuery) -> bool {
    q.prefecture
        .as_deref()
        .map(|p| !p.is_empty())
        .unwrap_or(false)
}

/// 集計対象の owner_id を決める。
///
/// - `owners` 指定あり              → その人だけ
/// - 指定なし & `sales_owners` あり → role=sales だけ（約束5・是正1）
/// - 指定なし & `sales_owners` なし → `None` = 絞らない
///
/// **`Some(空ベクタ)` は「誰も該当しない」を意味する。**
/// ここを `None`（絞らない）に丸めると、sales の一覧が空だったときに
/// 141名全員が混ざる — GAS 版でアポ率が 0.93%→0.63% に希釈された事故そのものになる。
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

/// 進行中の当月か。GAS `_isPartialMonth` と同じで、先頭7文字 (YYYY-MM) で判定する。
fn is_partial_month(ym: &str, current_ym: &str) -> bool {
    ym.len() >= 7 && &ym[..7] == current_ym
}

/// 期間フィルタ。`ym` は "YYYY-MM" または "YYYY-MM-DD"（先頭7文字で比較）。
fn in_period(ym: &str, from: Option<&str>, to: Option<&str>) -> bool {
    if ym.len() < 7 {
        return false;
    }
    let key = &ym[..7];
    if let Some(f) = from {
        if !f.is_empty() && key < &f[..f.len().min(7)] {
            return false;
        }
    }
    if let Some(t) = to {
        if !t.is_empty() && key > &t[..t.len().min(7)] {
            return false;
        }
    }
    true
}

// ------------------------------------------------- 月次（シート「月次明細」）

/// 「月次明細」から 月次推移 + メンバー別ヒートマップ を作る。
///
/// 使う列: owner_id / year_month / call_count / zoom_dial_count / apo_count
/// いずれも**列名で引く**（約束1）。
///
/// GAS 版との差:
///   GAS の `renderHeatmap` は月軸から当月を外す一方、owner の合計 `byOM` には
///   当月を混ぜていたため、**Top20 の選抜だけ当月込み**で行われていた。
///   ここでは当月を最初に落として一貫させる。
pub fn collect_monthly(
    data: &SheetData,
    q: &TimeseriesQuery,
    sales_owners: Option<&Vec<String>>,
    current_ym: &str,
) -> (Vec<MonthlyPoint>, MemberHeatmap, usize) {
    let pref_mode = pref_mode_of(q);
    let scope = resolve_scope(q.owners.as_deref(), sales_owners);

    // 年月 → [call, zoom, apo]
    let mut by_month: HashMap<String, [f64; 3]> = HashMap::new();
    // owner → 年月 → call
    let mut by_owner_month: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let mut owner_total: HashMap<String, f64> = HashMap::new();
    let mut matched = 0usize;

    for row in &data.rows {
        let owner = data.get(row, "owner_id");
        if owner.is_empty() || !in_scope(scope.as_ref(), owner) {
            continue;
        }
        let ym_raw = data.get(row, "year_month");
        if ym_raw.len() < 7 {
            continue;
        }
        // 進行中の当月は他の P3 チャートと不整合になるため除外（GAS と同じ）
        if is_partial_month(ym_raw, current_ym) {
            continue;
        }
        if !in_period(ym_raw, q.from_ym.as_deref(), q.to_ym.as_deref()) {
            continue;
        }
        let ym = ym_raw[..7].to_string();

        let call = num(data.get(row, "call_count"));
        let zoom = num(data.get(row, "zoom_dial_count"));
        let apo = num(data.get(row, "apo_count"));

        let e = by_month.entry(ym.clone()).or_insert([0.0; 3]);
        e[0] += call;
        e[1] += zoom;
        e[2] += apo;

        *by_owner_month
            .entry(owner.to_string())
            .or_default()
            .entry(ym)
            .or_insert(0.0) += call;
        *owner_total.entry(owner.to_string()).or_insert(0.0) += call;

        matched += 1;
    }

    // 年月昇順で安定させる（HashMap の反復順を返さない = 約束4）
    let mut months: Vec<String> = by_month.keys().cloned().collect();
    months.sort();

    let monthly: Vec<MonthlyPoint> = months
        .iter()
        .map(|ym| {
            let v = by_month[ym];
            let den = apo_denominator(v[0], v[1], pref_mode);
            MonthlyPoint {
                year_month: ym.clone(),
                call_count: v[0],
                zoom_dial_count: v[1],
                apo_count: v[2],
                denominator: den,
                apo_rate: rate(v[2], den),
            }
        })
        .collect();

    // --- メンバー別ヒートマップ: 架電数合計の降順 Top20 ---
    let total_members = owner_total.len();
    let mut ranked: Vec<(String, f64)> = owner_total.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0)) // 同数は owner_id で安定化（約束4）
    });
    let truncated = ranked.len() > MEMBER_HEATMAP_TOP_N;
    ranked.truncate(MEMBER_HEATMAP_TOP_N);

    let members: Vec<MemberSeries> = ranked
        .into_iter()
        .map(|(id, total)| {
            let per = by_owner_month.get(&id);
            MemberSeries {
                call_counts: months
                    .iter()
                    .map(|m| per.and_then(|p| p.get(m)).copied().unwrap_or(0.0))
                    .collect(),
                owner_id: id,
                total_call_count: total,
            }
        })
        .collect();

    let heatmap = MemberHeatmap {
        months,
        members,
        truncated,
        top_n: MEMBER_HEATMAP_TOP_N,
        total_members,
    };

    (monthly, heatmap, matched)
}

/// 直近完了月と前月比。`monthly` は年月昇順であることが前提。
fn build_scorecard(monthly: &[MonthlyPoint]) -> Scorecard {
    let last = match monthly.last() {
        Some(m) => m,
        None => return Scorecard::default(),
    };
    let prev = if monthly.len() >= 2 {
        monthly.get(monthly.len() - 2)
    } else {
        None
    };
    Scorecard {
        last_month: Some(last.year_month.clone()),
        prev_month: prev.map(|p| p.year_month.clone()),
        last_call_count: last.call_count,
        last_apo_rate: last.apo_rate,
        delta_call_count: prev.map(|p| last.call_count - p.call_count),
        // 「分母0で率が出ない月」との差分は出せない。0pt と書かず null にする（約束2）
        delta_apo_rate_pt: match (last.apo_rate, prev.and_then(|p| p.apo_rate)) {
            (Some(a), Some(b)) => Some(a - b),
            _ => None,
        },
    }
}

// --------------------------------------------- 曜日別（シート「曜日別集計」）

/// 「曜日別集計」から曜日別の架電数・アポ率を作る。
///
/// 使う列: weekday / dow_name / call_count / apo_count / na_due / active_days
///
/// このシートは Python 側で**日次明細を weekday で再集計した全期間スナップショット**で、
/// owner 列も期間列も持たない。したがって**期間・メンバー・都道府県のフィルタは反映されない**。
/// GAS 版も同じ（画面に「フィルタ非反映」と明記されている）。
///
/// アポ率は `apo_rate` 列（0-1 の小数）ではなく `apo_count / call_count` から
/// 引き直す。シート側は小数第4位で丸められており（例 0.0138 = 1.38%）、
/// 実値 1054/76646 = 1.3751% とは 0.005pt ずれる。率は 1pt 未満でも
/// 「乖離」として扱う運用なので、丸めた値を経由させない。
pub fn collect_weekday(data: &SheetData) -> (Vec<WeekdayPoint>, usize) {
    let mut out: Vec<WeekdayPoint> = Vec::new();
    let mut matched = 0usize;

    for row in &data.rows {
        let wd = match data.get(row, "weekday").trim().parse::<u8>() {
            Ok(v) if v <= 6 => v,
            // 曜日が読めない行は黙って0扱いにしない（月曜へ混ぜ込むと全曜日が壊れる）
            _ => continue,
        };
        let call = num(data.get(row, "call_count"));
        let apo = num(data.get(row, "apo_count"));
        let label = {
            let n = data.get(row, "dow_name").trim();
            if n.is_empty() {
                WEEKDAY_LABELS[wd as usize].to_string()
            } else {
                n.to_string()
            }
        };
        out.push(WeekdayPoint {
            weekday: wd,
            label,
            call_count: call,
            apo_count: apo,
            apo_rate: rate(apo, call),
            na_due: num(data.get(row, "na_due")),
            active_days: num(data.get(row, "active_days")),
            thin: call < MIN_CALLS_FOR_DOW_RATE,
        });
        matched += 1;
    }

    // 月→日の順に安定させる（約束4）
    out.sort_by_key(|w| w.weekday);
    (out, matched)
}

// ------------------------------------------- コホート（シート「コホート分析」）

/// 指標キー → (シート列名, 日本語ラベル)。
///
/// **是正2: 英語の指標名を画面に持ち込まない。**
/// `apo_rate` は anchor='apo' のコホートでは定義上つねに 100% になる（自明）ため、
/// GAS 版でも 2026-06-06 に選択肢から外してある。ここでも受け付けない。
fn cohort_metric_of(key: Option<&str>) -> (&'static str, &'static str) {
    match key.unwrap_or("progression").trim() {
        "retention" | "retention_rate" => ("retention_rate", "まだ残っている割合"),
        _ => ("progression_rate", "次に進んだ割合"),
    }
}

fn cohort_lag_label(lag: u8) -> String {
    if lag == 0 {
        "アポ当月".to_string()
    } else {
        format!("{lag}ヶ月後")
    }
}

/// 率の表記ゆれを % に統一する。
/// シートは 0-1 の小数（0.9613）で入るが、過去に 0-100 で入っていた列もあるため
/// GAS 版と同じ「1.5 以下なら小数とみなす」判定を踏襲する。
fn normalize_rate(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        return None;
    }
    let n: f64 = t.replace(',', "").parse().ok()?;
    if !n.is_finite() {
        return None;
    }
    Some(if n <= 1.5 { n * 100.0 } else { n })
}

/// 「コホート分析」から「アポ取得月ごとの その後の進み具合」を作る。
///
/// 使う列: anchor / cohort_month / elapsed_month / progression_rate / retention_rate
/// （旧スキーマ互換で `lag` / `rate` 列も見る）
pub fn collect_cohort(data: &SheetData, metric_key: Option<&str>) -> (Cohort, usize) {
    let (col, label) = cohort_metric_of(metric_key);

    // (cohort_month, lag) → 値
    let mut cells: HashMap<(String, u8), f64> = HashMap::new();
    let mut cohort_months: Vec<String> = Vec::new();
    let mut seen: HashMap<String, ()> = HashMap::new();
    let mut matched = 0usize;

    for row in &data.rows {
        // GAS 版と同じく anchor='apo' のみ（アポ起点コホート）
        if !data.get(row, "anchor").trim().eq_ignore_ascii_case("apo") {
            continue;
        }
        let cm = data.get(row, "cohort_month").trim();
        if cm.len() < 7 {
            continue;
        }
        let cm = cm[..7].to_string();

        // 実シートの列名は elapsed_month。旧 lag 列も後方互換で見る。
        let lag_raw = {
            let e = data.get(row, "elapsed_month").trim();
            if e.is_empty() {
                data.get(row, "lag").trim()
            } else {
                e
            }
        };
        let lag: u8 = match lag_raw.parse::<f64>() {
            Ok(v) if v >= 0.0 && v <= COHORT_MAX_LAG as f64 => v as u8,
            _ => continue,
        };

        // 選択指標 → 旧 rate 列 の順にフォールバック（GAS `pickMetric` と同じ）
        let v = normalize_rate(data.get(row, col)).or_else(|| normalize_rate(data.get(row, "rate")));
        matched += 1;
        if let Some(v) = v {
            cells.insert((cm.clone(), lag), v);
        }
        if seen.insert(cm.clone(), ()).is_none() {
            cohort_months.push(cm);
        }
    }

    cohort_months.sort();
    let total = cohort_months.len();
    let truncated = total > COHORT_MONTHS;
    // 直近 6 コホートに絞る（新しい方を残す）
    if truncated {
        cohort_months = cohort_months.split_off(total - COHORT_MONTHS);
    }

    let lag_labels: Vec<String> = (0..=COHORT_MAX_LAG).map(cohort_lag_label).collect();
    let rows: Vec<CohortRow> = cohort_months
        .into_iter()
        .map(|cm| CohortRow {
            values: (0..=COHORT_MAX_LAG)
                .map(|l| cells.get(&(cm.clone(), l)).copied())
                .collect(),
            cohort_month: cm,
        })
        .collect();

    (
        Cohort {
            metric_label: label.to_string(),
            lag_labels,
            rows,
            truncated,
            total_cohort_months: total,
            anchor: "apo".to_string(),
            description:
                "「◯月にアポが取れた案件は、その後ちゃんと前に進んだか」を月ごとに並べた表です。\
                 タテ=アポが取れた月 / ヨコ=そこから何ヶ月後か。薄いマスはフォローが止まっているサインです。"
                    .to_string(),
        },
        matched,
    )
}

// ------------------------- アポ率 母数2版比較（シート「時間帯ヒート」）

/// 「時間帯ヒート」から 曜日×時間帯 の アポ／HubSpot Call／Zoom発信 を集計する。
///
/// 使う列: owner_id / weekday / hour / dial_count / apo_count / zoom_dial_count
///
/// **なぜ「時間帯ヒート_クロス」ではなくこちらを読むのか**:
/// `zoom_dial_count` はこのシートにしか無い（クロス側は owner/曜日/時/都道府県/業種/
/// dial/connect/apo の8列だけ）。母数2版比較は Zoom発信が要るので、
/// 時間帯×曜日ヒートマップ（クロス側・`heatmap::aggregate` 再利用）とは別に読む。
///
/// **是正1**: `scope` で営業に絞る。GAS 版はメンバー未選択時に絞りが一切かからず、
/// owner 141名 (sales30/bpo32/consultant29/other50) が混ざって
/// アポ率が sales 単独 0.93% → 混在 0.63% に希釈されていた。
pub fn collect_denominator_compare(
    data: &SheetData,
    scope: Option<&Vec<String>>,
) -> (Vec<DenominatorCell>, f64, f64, usize) {
    // (weekday, hour) → [apo, hubspot_dial, zoom_dial]
    let mut acc: HashMap<(u8, u8), [f64; 3]> = HashMap::new();
    let mut matched = 0usize;

    for row in &data.rows {
        let owner = data.get(row, "owner_id");
        if !in_scope(scope, owner) {
            continue;
        }
        let wd = match data.get(row, "weekday").trim().parse::<u8>() {
            Ok(v) if v <= 6 => v,
            _ => continue,
        };
        let hr = match data.get(row, "hour").trim().parse::<u8>() {
            Ok(v) if v <= 23 => v,
            _ => continue,
        };
        let e = acc.entry((wd, hr)).or_insert([0.0; 3]);
        e[0] += num(data.get(row, "apo_count"));
        e[1] += num(data.get(row, "dial_count"));
        e[2] += num(data.get(row, "zoom_dial_count"));
        matched += 1;
    }

    let mut total_hs = 0.0;
    let mut total_zoom = 0.0;
    let mut keys: Vec<(u8, u8)> = acc.keys().copied().collect();
    // 曜日→時間帯 の順に安定させる（約束4）
    keys.sort();

    let cells: Vec<DenominatorCell> = keys
        .into_iter()
        .map(|(wd, hr)| {
            let v = acc[&(wd, hr)];
            total_hs += v[1];
            total_zoom += v[2];
            DenominatorCell {
                weekday: wd,
                weekday_label: WEEKDAY_LABELS[wd as usize].to_string(),
                hour: hr,
                apo_count: v[0],
                hubspot_dial: v[1],
                zoom_dial: v[2],
                // 分母0は null。「その時間帯に架電0だからアポ率0%」と読ませない（約束2）
                apo_rate_hubspot: rate(v[0], v[1]),
                apo_rate_zoom: rate(v[0], v[2]),
            }
        })
        .collect();

    (cells, total_hs, total_zoom, matched)
}

// ------------------------------------------------------------------ ハンドラ

/// P3 の全パネルを1応答で返す。
///
/// `heat_cache` は「時間帯ヒート_クロス」専用の常駐キャッシュ（`heatmap.rs`）。
/// 時間帯×曜日ヒートマップは `heatmap::handle` を通してそこの `aggregate` を
/// そのまま使う（作り直さない）。
pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    heat_cache: &HeatmapCache,
    q: TimeseriesQuery,
    sales_owners: Option<Vec<String>>,
) -> Result<TabPayload<TimeseriesData>> {
    let started = Instant::now();

    let current_ym = q
        .current_ym
        .clone()
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m").to_string());
    let pref_mode = pref_mode_of(&q);
    let scope = resolve_scope(q.owners.as_deref(), sales_owners.as_ref());
    let scope_text = scope_label(q.owners.as_deref(), scope.as_ref());
    // 「該当者0名」を「絞らない」に丸めない。空スコープなら何も集計しない。
    let scope_is_empty = scope.as_ref().map(|v| v.is_empty()).unwrap_or(false);

    let mut sources: Vec<SourceInfo> = Vec::new();

    // --- 月次明細 ---
    let (monthly_sheet, monthly_cached) = store.get(client, "月次明細").await?;
    let (monthly, member_heatmap, monthly_matched) =
        collect_monthly(&monthly_sheet, &q, sales_owners.as_ref(), &current_ym);
    sources.push(SourceInfo {
        sheet: "月次明細".to_string(),
        total_rows: monthly_sheet.rows.len(),
        matched_rows: monthly_matched,
        from_cache: monthly_cached,
        age_secs: monthly_sheet.fetched_at.elapsed().as_secs(),
    });

    // --- 曜日別集計 ---
    let (weekday_sheet, weekday_cached) = store.get(client, "曜日別集計").await?;
    let (weekday, weekday_matched) = collect_weekday(&weekday_sheet);
    sources.push(SourceInfo {
        sheet: "曜日別集計".to_string(),
        total_rows: weekday_sheet.rows.len(),
        matched_rows: weekday_matched,
        from_cache: weekday_cached,
        age_secs: weekday_sheet.fetched_at.elapsed().as_secs(),
    });

    // --- コホート分析 ---
    let (cohort_sheet, cohort_cached) = store.get(client, "コホート分析").await?;
    let (cohort, cohort_matched) = collect_cohort(&cohort_sheet, q.cohort_metric.as_deref());
    sources.push(SourceInfo {
        sheet: "コホート分析".to_string(),
        total_rows: cohort_sheet.rows.len(),
        matched_rows: cohort_matched,
        from_cache: cohort_cached,
        age_secs: cohort_sheet.fetched_at.elapsed().as_secs(),
    });

    // --- 時間帯×曜日 ヒートマップ（heatmap.rs を再利用） ---
    //
    // `HeatmapQuery.owners` は「空文字なら絞らない」実装なので、
    // スコープが空集合のときに渡すと全員が混ざる。ここで先に短絡する。
    let hour_weekday = if scope_is_empty {
        HourWeekday {
            cells: Vec::new(),
            source_rows: 0,
            total_rows: 0,
            from_cache: true,
            scope_label: format!("{scope_text}（該当者なし）"),
        }
    } else {
        let hq = HeatmapQuery {
            prefecture: q.prefecture.clone(),
            industry: q.industry.clone(),
            // 是正1: メンバー未選択でも role=sales に絞る
            owners: scope.as_ref().map(|v| v.join(",")),
        };
        let hm = heatmap::handle(client, heat_cache, hq).await?;
        HourWeekday {
            cells: hm.cells,
            source_rows: hm.source_rows,
            total_rows: hm.total_rows,
            from_cache: hm.from_cache,
            scope_label: scope_text.clone(),
        }
    };

    // --- アポ率 母数2版比較（時間帯ヒート） ---
    let cross_active = pref_mode
        || q.industry
            .as_deref()
            .map(|i| !i.is_empty())
            .unwrap_or(false);
    let compare = if cross_active {
        // GAS との差異（意図的）:
        //   GAS はクロス絞込中でも ①HubSpot版だけ絞り込んで描き、②Zoom版だけ
        //   フォールバック文言にしていた。分母だけ絞られた ① と 絞られていない ② を
        //   並べると「母数2版の比較」として読めないため、パネルごと落とす。
        DenominatorCompare {
            applicable: false,
            reason: Some(
                "業界・都道府県で絞り込み中は Zoom発信の母数が取れないため比較できません\
                 （「時間帯ヒート_クロス」に zoom_dial_count 列が無い）。全業界・全国で表示してください。"
                    .to_string(),
            ),
            cells: Vec::new(),
            total_hubspot_dial: 0.0,
            total_zoom_dial: 0.0,
            scope_label: scope_text.clone(),
        }
    } else {
        let (heat_sheet, heat_cached) = store.get(client, "時間帯ヒート").await?;
        let (cells, total_hs, total_zoom, matched) =
            collect_denominator_compare(&heat_sheet, scope.as_ref());
        sources.push(SourceInfo {
            sheet: "時間帯ヒート".to_string(),
            total_rows: heat_sheet.rows.len(),
            matched_rows: matched,
            from_cache: heat_cached,
            age_secs: heat_sheet.fetched_at.elapsed().as_secs(),
        });
        // Zoom発信が1件も無ければ、全セル null の②を並べても比較にならない
        let zoom_missing = total_zoom <= 0.0;
        DenominatorCompare {
            applicable: !zoom_missing,
            reason: if zoom_missing {
                Some(
                    "Zoom発信の母数が0件のため②が描けません（内線→担当の帰属が取れていない可能性）。"
                        .to_string(),
                )
            } else {
                None
            },
            cells,
            total_hubspot_dial: total_hs,
            total_zoom_dial: total_zoom,
            scope_label: scope_text.clone(),
        }
    };

    let scorecard = build_scorecard(&monthly);

    Ok(TabPayload {
        data: TimeseriesData {
            scorecard,
            monthly,
            apo_denominator_label: denominator_label(pref_mode),
            weekday,
            weekday_min_calls: MIN_CALLS_FOR_DOW_RATE,
            weekday_denominator_label:
                "HubSpot Call（「曜日別集計」シートに Zoom発信の列が無い）".to_string(),
            weekday_note:
                "日次明細を曜日で再集計した全期間スナップショット。期間・メンバー・都道府県のフィルタは反映されない。"
                    .to_string(),
            member_heatmap,
            cohort,
            hour_weekday,
            apo_denominator_compare: compare,
            scope_label: scope_text,
        },
        sources,
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    fn monthly_sheet(rows: Vec<(&str, &str, f64, f64, f64)>) -> SheetData {
        let header = vec![
            "owner_id".to_string(),
            "year_month".to_string(),
            "call_count".to_string(),
            "zoom_dial_count".to_string(),
            "apo_count".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(o, ym, c, z, a)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(o),
                    Arc::from(ym),
                    Arc::from(c.to_string().as_str()),
                    Arc::from(z.to_string().as_str()),
                    Arc::from(a.to_string().as_str()),
                ]
            })
            .collect();
        SheetData {
            header,
            rows,
            fetched_at: Instant::now(),
        }
    }

    fn weekday_sheet(rows: Vec<(u8, &str, f64, f64)>) -> SheetData {
        let header = vec![
            "weekday".to_string(),
            "dow_name".to_string(),
            "call_count".to_string(),
            "apo_count".to_string(),
            "na_due".to_string(),
            "active_days".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(w, n, c, a)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(w.to_string().as_str()),
                    Arc::from(n),
                    Arc::from(c.to_string().as_str()),
                    Arc::from(a.to_string().as_str()),
                    Arc::from("0"),
                    Arc::from("20"),
                ]
            })
            .collect();
        SheetData {
            header,
            rows,
            fetched_at: Instant::now(),
        }
    }

    fn cohort_sheet(rows: Vec<(&str, &str, u8, &str, &str)>) -> SheetData {
        let header = vec![
            "anchor".to_string(),
            "cohort_month".to_string(),
            "elapsed_month".to_string(),
            "progression_rate".to_string(),
            "retention_rate".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(an, cm, lag, prog, ret)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(an),
                    Arc::from(cm),
                    Arc::from(lag.to_string().as_str()),
                    Arc::from(prog),
                    Arc::from(ret),
                ]
            })
            .collect();
        SheetData {
            header,
            rows,
            fetched_at: Instant::now(),
        }
    }

    fn heat_sheet(rows: Vec<(&str, u8, u8, f64, f64, f64)>) -> SheetData {
        let header = vec![
            "owner_id".to_string(),
            "weekday".to_string(),
            "hour".to_string(),
            "dial_count".to_string(),
            "apo_count".to_string(),
            "zoom_dial_count".to_string(),
        ];
        let rows = rows
            .into_iter()
            .map(|(o, w, h, d, a, z)| -> Vec<Arc<str>> {
                vec![
                    Arc::from(o),
                    Arc::from(w.to_string().as_str()),
                    Arc::from(h.to_string().as_str()),
                    Arc::from(d.to_string().as_str()),
                    Arc::from(a.to_string().as_str()),
                    Arc::from(z.to_string().as_str()),
                ]
            })
            .collect();
        SheetData {
            header,
            rows,
            fetched_at: Instant::now(),
        }
    }

    fn q_at(current: &str) -> TimeseriesQuery {
        TimeseriesQuery {
            current_ym: Some(current.to_string()),
            ..Default::default()
        }
    }

    // ---------------------------------------------------------- 分母0 / null

    #[test]
    fn 月次の分母0はアポ率をnullにする() {
        // 架電0・Zoom0 でアポ3件。実データに存在する形。0% と表示してはいけない。
        let d = monthly_sheet(vec![("1", "2026-05", 0.0, 0.0, 3.0)]);
        let (m, _, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        assert_eq!(m.len(), 1);
        assert!(m[0].apo_rate.is_none());
    }

    #[test]
    fn 母数2版比較は分母ごとに独立してnullになる() {
        // HubSpot Call はあるが Zoom発信が0 のセル。①は率が出て②は null。
        let d = heat_sheet(vec![("1", 0, 10, 100.0, 2.0, 0.0)]);
        let (cells, hs, zoom, _) = collect_denominator_compare(&d, None);
        assert_eq!(cells.len(), 1);
        assert_eq!(cells[0].apo_rate_hubspot, Some(2.0));
        assert!(cells[0].apo_rate_zoom.is_none(), "Zoom発信0を 0% にしない");
        assert_eq!(hs, 100.0);
        assert_eq!(zoom, 0.0);
    }

    #[test]
    fn 曜日別の分母0はアポ率をnullにする() {
        let d = weekday_sheet(vec![(5, "土", 0.0, 1.0)]);
        let (w, _) = collect_weekday(&d);
        assert!(w[0].apo_rate.is_none());
    }

    // ------------------------------------------------------------ 営業スコープ

    #[test]
    fn 月次は既定で営業以外を除外する() {
        let d = monthly_sheet(vec![
            ("sales1", "2026-05", 100.0, 200.0, 2.0),
            ("bpo1", "2026-05", 900.0, 900.0, 1.0),
        ]);
        let sales = vec!["sales1".to_string()];
        let (m, _, matched) = collect_monthly(&d, &q_at("2026-08"), Some(&sales), "2026-08");
        assert_eq!(matched, 1, "BPO の行が混ざってはいけない");
        assert_eq!(m[0].zoom_dial_count, 200.0);
    }

    #[test]
    fn 母数2版比較も営業に絞られる() {
        // 是正1 の本体。GAS 版はここで 141名を混ぜてアポ率を 0.93%→0.63% に希釈していた。
        let d = heat_sheet(vec![
            ("sales1", 0, 10, 100.0, 1.0, 100.0),
            ("bpo1", 0, 10, 900.0, 1.0, 900.0),
        ]);
        let sales = vec!["sales1".to_string()];
        let (cells, _, _, matched) = collect_denominator_compare(&d, Some(&sales));
        assert_eq!(matched, 1);
        assert_eq!(cells[0].hubspot_dial, 100.0);
        assert_eq!(cells[0].apo_rate_hubspot, Some(1.0), "混ぜると 0.2% に薄まる");
    }

    #[test]
    fn スコープが空集合なら誰も集計されない() {
        // 「role=sales が0名」を「絞らない」に丸めると全員混ざる。丸めないこと。
        let d = heat_sheet(vec![("bpo1", 0, 10, 900.0, 5.0, 900.0)]);
        let empty: Vec<String> = Vec::new();
        let (cells, _, _, matched) = collect_denominator_compare(&d, Some(&empty));
        assert_eq!(matched, 0);
        assert!(cells.is_empty());
    }

    #[test]
    fn メンバー指定は営業スコープより優先される() {
        let sales = vec!["sales1".to_string()];
        let s = resolve_scope(Some("bpo1,bpo2"), Some(&sales)).unwrap();
        assert_eq!(s, vec!["bpo1".to_string(), "bpo2".to_string()]);
        // 空文字の owners は「未選択」扱い（画面の「全て」）
        let s2 = resolve_scope(Some(" , "), Some(&sales)).unwrap();
        assert_eq!(s2, vec!["sales1".to_string()]);
    }

    // ------------------------------------------------------------ 並びの安定

    #[test]
    fn 月次は年月昇順で安定する() {
        let d = monthly_sheet(vec![
            ("1", "2026-05", 100.0, 200.0, 2.0),
            ("1", "2026-03", 100.0, 200.0, 1.0),
            ("1", "2026-04", 100.0, 200.0, 3.0),
        ]);
        let a = collect_monthly(&d, &q_at("2026-08"), None, "2026-08").0;
        let b = collect_monthly(&d, &q_at("2026-08"), None, "2026-08").0;
        let ys: Vec<&str> = a.iter().map(|x| x.year_month.as_str()).collect();
        assert_eq!(ys, vec!["2026-03", "2026-04", "2026-05"]);
        assert_eq!(
            ys,
            b.iter().map(|x| x.year_month.as_str()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn 曜日別は月から日の順に安定する() {
        let d = weekday_sheet(vec![
            (3, "木", 500.0, 5.0),
            (0, "月", 500.0, 5.0),
            (6, "日", 500.0, 5.0),
        ]);
        let (w, _) = collect_weekday(&d);
        assert_eq!(
            w.iter().map(|x| x.weekday).collect::<Vec<_>>(),
            vec![0, 3, 6]
        );
    }

    #[test]
    fn メンバーヒートは架電数同数でもowner_idで安定する() {
        let d = monthly_sheet(vec![
            ("b", "2026-05", 100.0, 100.0, 1.0),
            ("a", "2026-05", 100.0, 100.0, 1.0),
        ]);
        let (_, h, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        assert_eq!(
            h.members.iter().map(|m| m.owner_id.as_str()).collect::<Vec<_>>(),
            vec!["a", "b"]
        );
    }

    // ------------------------------------------------------------ 切り捨て明示

    #[test]
    fn メンバーヒートはtop20で切りtruncatedを立てる() {
        let mut rows: Vec<(String, &str, f64, f64, f64)> = Vec::new();
        for i in 0..25 {
            rows.push((format!("o{i:02}"), "2026-05", (100 - i) as f64, 0.0, 0.0));
        }
        let refs: Vec<(&str, &str, f64, f64, f64)> = rows
            .iter()
            .map(|(a, b, c, d, e)| (a.as_str(), *b, *c, *d, *e))
            .collect();
        let d = monthly_sheet(refs);
        let (_, h, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        assert_eq!(h.members.len(), MEMBER_HEATMAP_TOP_N);
        assert!(h.truncated, "黙って上位20件にしない");
        assert_eq!(h.total_members, 25);
        assert_eq!(h.members[0].owner_id, "o00", "架電数降順");
    }

    #[test]
    fn コホートは直近6ヶ月で切りtruncatedを立てる() {
        let mut rows: Vec<(&str, String, u8, &str, &str)> = Vec::new();
        for m in 1..=8 {
            rows.push(("apo", format!("2026-{m:02}"), 0, "0.9", "0.6"));
        }
        let refs: Vec<(&str, &str, u8, &str, &str)> = rows
            .iter()
            .map(|(a, b, c, d, e)| (*a, b.as_str(), *c, *d, *e))
            .collect();
        let d = cohort_sheet(refs);
        let (c, _) = collect_cohort(&d, None);
        assert_eq!(c.rows.len(), COHORT_MONTHS);
        assert!(c.truncated);
        assert_eq!(c.total_cohort_months, 8);
        assert_eq!(c.rows[0].cohort_month, "2026-03", "新しい方を残す");
    }

    // ---------------------------------------------------------- コホート日本語

    #[test]
    fn コホートのラベルは日本語で返る() {
        // 是正2: progression_rate / M0 / M1 のような英語の指標名を画面に持ち込まない。
        let d = cohort_sheet(vec![
            ("apo", "2026-05", 0, "0.9613", "0.6673"),
            ("apo", "2026-05", 1, "0.5", "0.4"),
        ]);
        let (c, _) = collect_cohort(&d, None);
        assert_eq!(c.metric_label, "次に進んだ割合");
        assert_eq!(c.lag_labels[0], "アポ当月");
        assert_eq!(c.lag_labels[1], "1ヶ月後");
        assert_eq!(c.lag_labels[2], "2ヶ月後");

        let (c2, _) = collect_cohort(&d, Some("retention"));
        assert_eq!(c2.metric_label, "まだ残っている割合");
        // 0-1 の小数は % に直す（浮動小数の丸めがあるので誤差込みで見る）
        let v = c2.rows[0].values[0].expect("retention_rate が読めていない");
        assert!((v - 66.73).abs() < 1e-9, "実測は {v}");
    }

    #[test]
    fn コホートの空きマスはnullで返る() {
        // 「その月にはまだ到達していない」を 0% と描かない
        let d = cohort_sheet(vec![("apo", "2026-05", 0, "0.9", "0.6")]);
        let (c, _) = collect_cohort(&d, None);
        assert_eq!(c.rows[0].values[0], Some(90.0));
        assert!(c.rows[0].values[1].is_none());
        assert_eq!(c.rows[0].values.len(), (COHORT_MAX_LAG + 1) as usize);
    }

    #[test]
    fn コホートはアポ起点以外を混ぜない() {
        let d = cohort_sheet(vec![
            ("apo", "2026-05", 0, "0.9", "0.6"),
            ("call", "2026-05", 0, "0.1", "0.1"),
        ]);
        let (c, matched) = collect_cohort(&d, None);
        assert_eq!(matched, 1);
        assert_eq!(c.rows[0].values[0], Some(90.0));
    }

    // ------------------------------------------------------------ 分母ラベル

    #[test]
    fn 分母は既定でzoom発信で県指定でhubspot_callに変わる() {
        // 是正3: どちらで割ったかを必ずラベルで返す（「アポ率」単独表記の禁止）
        assert_eq!(apo_denominator(100.0, 300.0, false), 300.0);
        assert_eq!(apo_denominator(100.0, 300.0, true), 100.0);
        assert_eq!(apo_denominator(100.0, 0.0, false), 100.0, "Zoom無しはCallへ");
        assert!(denominator_label(false).contains("Zoom発信"));
        assert!(denominator_label(true).contains("HubSpot Call"));
    }

    #[test]
    fn 都道府県指定で月次アポ率の分母が切り替わる() {
        let d = monthly_sheet(vec![("1", "2026-05", 100.0, 400.0, 4.0)]);
        let base = collect_monthly(&d, &q_at("2026-08"), None, "2026-08").0;
        assert_eq!(base[0].apo_rate, Some(1.0), "4/400 = 1.00%");

        let q = TimeseriesQuery {
            current_ym: Some("2026-08".into()),
            prefecture: Some("東京都".into()),
            ..Default::default()
        };
        let pref = collect_monthly(&d, &q, None, "2026-08").0;
        assert_eq!(pref[0].apo_rate, Some(4.0), "4/100 = 4.00%");
    }

    // ------------------------------------------------------- 当月除外 / 期間

    #[test]
    fn 進行中の当月は月次から除外される() {
        let d = monthly_sheet(vec![
            ("1", "2026-07", 100.0, 100.0, 1.0),
            ("1", "2026-08", 5.0, 5.0, 0.0), // 数日分しかない当月
        ]);
        let (m, h, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        assert_eq!(m.len(), 1);
        assert_eq!(m[0].year_month, "2026-07");
        assert_eq!(
            h.months,
            vec!["2026-07".to_string()],
            "メンバーヒートの月軸も同じ扱いにする"
        );
        assert_eq!(
            h.members[0].total_call_count, 100.0,
            "GAS 版は Top20 の選抜にだけ当月を混ぜていた。ここでは混ぜない"
        );
    }

    #[test]
    fn 期間フィルタが効く() {
        let d = monthly_sheet(vec![
            ("1", "2026-03", 10.0, 10.0, 0.0),
            ("1", "2026-05", 20.0, 20.0, 0.0),
            ("1", "2026-07", 30.0, 30.0, 0.0),
        ]);
        let q = TimeseriesQuery {
            current_ym: Some("2026-08".into()),
            from_ym: Some("2026-05".into()),
            to_ym: Some("2026-07".into()),
            ..Default::default()
        };
        let (m, _, _) = collect_monthly(&d, &q, None, "2026-08");
        assert_eq!(
            m.iter().map(|x| x.year_month.as_str()).collect::<Vec<_>>(),
            vec!["2026-05", "2026-07"]
        );
    }

    // ---------------------------------------------------------- 曜日別足切り

    #[test]
    fn 曜日別の少サンプルは消さずに旗を立てる() {
        // GAS 版は配列から落としていた。ここでは行を残して thin を立て、
        // 率チャート側で外す（合計と内訳を一致させるため）。
        let d = weekday_sheet(vec![
            (0, "月", 76646.0, 1054.0),
            (6, "日", 5.0, 1.0), // 母数5でアポ1 = 20%。異常値。
        ]);
        let (w, _) = collect_weekday(&d);
        assert_eq!(w.len(), 2, "行は落とさない");
        assert!(!w[0].thin);
        assert!(w[1].thin, "架電100件未満は率チャートから外す旗を立てる");
        assert_eq!(MIN_CALLS_FOR_DOW_RATE, 100.0);
    }

    #[test]
    fn 曜日別アポ率は丸めた列でなく実値から引き直す() {
        // シートの apo_rate は小数第4位(0.0138)で丸められている。
        // 1054/76646 = 1.3751...% との差 0.005pt を持ち込まない。
        let d = weekday_sheet(vec![(0, "月", 76646.0, 1054.0)]);
        let (w, _) = collect_weekday(&d);
        let r = w[0].apo_rate.expect("架電76,646件あるので率は出る");
        // シート値 0.0138 → 1.38% と比べて 0.005pt 低い側が実値
        assert!((r - 1.3751).abs() < 1e-3, "実測は {r}");
        assert!(r < 1.38, "丸めた 1.38% をそのまま出してはいけない");
    }

    #[test]
    fn 曜日が読めない行は月曜に混ぜない() {
        let mut d = weekday_sheet(vec![(0, "月", 100.0, 1.0)]);
        d.rows.push(vec![
            Arc::from(""),
            Arc::from(""),
            Arc::from("9999"),
            Arc::from("9999"),
            Arc::from("0"),
            Arc::from("0"),
        ]);
        let (w, matched) = collect_weekday(&d);
        assert_eq!(matched, 1);
        assert_eq!(w.len(), 1);
        assert_eq!(w[0].call_count, 100.0);
    }

    // ------------------------------------------------------------ スコアカード

    #[test]
    fn スコアカードは直近完了月と前月比を返す() {
        let d = monthly_sheet(vec![
            ("1", "2026-06", 100.0, 100.0, 1.0), // 1.00%
            ("1", "2026-07", 200.0, 200.0, 6.0), // 3.00%
        ]);
        let (m, _, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        let s = build_scorecard(&m);
        assert_eq!(s.last_month.as_deref(), Some("2026-07"));
        assert_eq!(s.prev_month.as_deref(), Some("2026-06"));
        assert_eq!(s.delta_call_count, Some(100.0));
        assert_eq!(s.delta_apo_rate_pt, Some(2.0));
    }

    #[test]
    fn 前月の率がnullなら前月比もnull() {
        // 前月が分母0（率なし）。差分を 0pt と書かない。
        let d = monthly_sheet(vec![
            ("1", "2026-06", 0.0, 0.0, 0.0),
            ("1", "2026-07", 200.0, 200.0, 6.0),
        ]);
        let (m, _, _) = collect_monthly(&d, &q_at("2026-08"), None, "2026-08");
        let s = build_scorecard(&m);
        assert!(s.delta_apo_rate_pt.is_none());
        assert_eq!(s.delta_call_count, Some(200.0));
    }

    #[test]
    fn 母数2版比較のセルは曜日時間帯順に安定する() {
        let d = heat_sheet(vec![
            ("1", 1, 9, 10.0, 0.0, 20.0),
            ("1", 0, 15, 10.0, 0.0, 20.0),
            ("1", 0, 9, 10.0, 0.0, 20.0),
        ]);
        let (cells, _, _, _) = collect_denominator_compare(&d, None);
        assert_eq!(
            cells.iter().map(|c| (c.weekday, c.hour)).collect::<Vec<_>>(),
            vec![(0, 9), (0, 15), (1, 9)]
        );
        assert_eq!(cells[0].weekday_label, "月", "0=月（Python weekday 準拠）");
    }
}
