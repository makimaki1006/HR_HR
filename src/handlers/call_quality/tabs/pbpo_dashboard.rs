//! BPOダッシュボード（GAS 版 `page-pbpo` の移植）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-pbpo">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderBPO` / `_bpoRenderKPI` / `_bpoRender` / `_bpoRenderDQ` /
//!          `_bpoRenderNA` / `_bpoLoadHeatmap` / `_bpoRenderHeatPair`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getRawSheet('BPOコーラー' 等)` / `getBpoTimeHeatmap`）
//!
//! ------------------------------------------------------------------
//! ⚠ このタブだけ営業スコープ(role=sales)に絞らない
//! ------------------------------------------------------------------
//! 他タブ(`p1_members` 等)は既定で role=sales の担当者だけに絞るが、
//! **このタブは BPO 専用画面**であり、そもそも絞り込みの対象が BPO。
//! ここで読む5シート(BPOコーラー/BPOデータ品質/BPOネクストアクション/
//! BPOネクストアクション明細/時間帯ヒート_BPO)は Python バッチ側が
//! 元から BPO コーラーの行しか作らないため、`sales_owners` のような
//! 絞り込み引数は**意図的に持たせていない**（持たせると BPO 全員が消える事故になる）。
//!
//! # BPO実働PL / 管理PL
//! BPO 実働パイプラインは `753186575`。管理PL `753214833` は集計対象外
//! （このタブが読むシートは Python バッチ側で既に実働PLのみに絞って生成済み）。
//! アポは `bpo_appo_date` の有無で判定（Python 側の定義。ここでは件数列をそのまま使う）。
//!
//! # 「接触率」は出さない
//! 過去月を再構成できないため接触率は設計上算出不可（ユーザー確定事項）。
//! 通話時間の閾値(30/90/360秒)は「繋がった」の代理指標であり、接触率ではない。
//! 「接続率」という語も使わない（表記ゆれで誤解を招くため、社内では
//! 「Call記録率」に統一する方針。本タブは元々そのどちらも出さない）。
//!
//! ------------------------------------------------------------------
//! GAS 版に入っていた誤りを持ち込まないこと
//! ------------------------------------------------------------------
//! シート「BPOコーラー」の 通話90秒以上率/通話360秒以上率/アポ率 列は
//! Python 側の値をそのまま信じると**架電0件でもアポ率が "0" になっている行が実在する**
//! （実データ確認 2026-08-16: 久保 寿代 2026-01〜04 は 架電数=0 なのに アポ数=17 で、
//! シート上のアポ率列は "0"）。GAS はこの列をそのまま画面に出していたため誤り。
//! ここでは**件数列から re-compute** し、分母0は None にする(`tabs/mod.rs` 約束2)。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! 1. 列見出しクリックでの並び替え（GAS `_bpoSort`）
//!    → 未実装。既定=担当者名の昇順(中立)のみ返す。GAS も既定は中立
//!      （「数値は事実のみ(順位・評価なし)」というコメントあり）。並び替えUIは
//!      フロント側の責務として持たせる（他タブの表と同じ方針）。
//! 2. ヒートマップの色スケール(intensity)計算
//!    → 未実装。セルの値(率・件数)のみ返し、色付けはフロント側の責務とする
//!      （他タブの時間帯ヒートも同方針）。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_CALLER: &str = "BPOコーラー";
const SHEET_DQ: &str = "BPOデータ品質";
const SHEET_NA: &str = "BPOネクストアクション";
const SHEET_NA_DETAIL: &str = "BPOネクストアクション明細";
const SHEET_HEATMAP: &str = "時間帯ヒート_BPO";

/// 「参考」扱いにする架電数の下限（GAS `SMALL_N`）。これ未満は率がブレやすい。
const SMALL_N_CALLS: f64 = 200.0;
/// ヒートマップで「参考」扱いにする架電数の下限（GAS `dl < 100`）。
const SMALL_N_HEATMAP: f64 = 100.0;

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// 年月を "YYYY-MM" に正規化する（GAS `_bpoM`）。
/// スプシ日付化で末尾に時刻等が付くケースへの対策として先頭7文字を取る。
fn norm_month(s: &str) -> String {
    let t = s.trim();
    if t.len() >= 7 {
        t[..7].to_string()
    } else {
        t.to_string()
    }
}

/// データ品質シートの「赤字強調」判定（GAS `_bpoRenderDQ` の `bad` 判定）。
fn bad_dq_value(item: &str, value: &str) -> bool {
    let hay = format!("{item}{value}");
    hay.contains("不可") || hay.contains("ゴミ") || hay.contains("未入力")
}

// ============================================================ ① 全体KPI(月次)

#[derive(Debug, Serialize)]
pub struct MonthlyKpiRow {
    pub year_month: String,
    pub dial_count: f64,
    pub apo_count: f64,
    pub apo_rate: Option<f64>,
    pub dur90_count: f64,
    pub dur90_rate: Option<f64>,
    pub dur360_count: f64,
    pub dur360_rate: Option<f64>,
    /// 当月(集計途中)かどうか。画面で「(途中)」を出すための旗
    pub is_partial: bool,
}

#[derive(Debug, Serialize)]
pub struct MonthlyKpiPanel {
    pub rows: Vec<MonthlyKpiRow>,
    /// 当月を除いた最新の確定月。個人別パネルの既定選択に使う
    pub latest_complete_month: String,
    pub current_month: String,
}

/// シート「BPOコーラー」を年月で集約する。
///
/// 列: 担当者,内線,年月,架電数,通話時間h,通話90秒以上率,通話90秒以上数,
///     通話360秒以上率,通話360秒以上数,アポ数,アポ率
///
/// **率は列をそのまま信じず、件数の合計から re-compute する**（ファイル冒頭の注記）。
fn build_monthly_kpi(d: &SheetData, current_month: &str) -> MonthlyKpiPanel {
    // [架電, アポ, 90秒以上, 360秒以上]
    let mut acc: HashMap<String, [f64; 4]> = HashMap::new();
    for row in &d.rows {
        let ym = norm_month(d.get(row, "年月"));
        if ym.is_empty() {
            continue;
        }
        let e = acc.entry(ym).or_insert([0.0; 4]);
        e[0] += num(d.get(row, "架電数"));
        e[1] += num(d.get(row, "アポ数"));
        e[2] += num(d.get(row, "通話90秒以上数"));
        e[3] += num(d.get(row, "通話360秒以上数"));
    }

    let mut months: Vec<String> = acc.keys().cloned().collect();
    months.sort();

    let latest_complete = months
        .iter()
        .rev()
        .find(|m| m.as_str() != current_month)
        .cloned()
        .or_else(|| months.last().cloned())
        .unwrap_or_default();

    let rows = months
        .into_iter()
        .map(|ym| {
            let v = acc[&ym];
            MonthlyKpiRow {
                is_partial: ym == current_month,
                apo_rate: rate(v[1], v[0]),
                dur90_rate: rate(v[2], v[0]),
                dur360_rate: rate(v[3], v[0]),
                year_month: ym,
                dial_count: v[0],
                apo_count: v[1],
                dur90_count: v[2],
                dur360_count: v[3],
            }
        })
        .collect();

    MonthlyKpiPanel {
        rows,
        latest_complete_month: latest_complete,
        current_month: current_month.to_string(),
    }
}

// ============================================================ ② 個人別(月次)

#[derive(Debug, Serialize)]
pub struct CallerRow {
    pub name: String,
    pub extension: String,
    pub year_month: String,
    pub dial_count: f64,
    pub talk_hours: f64,
    pub dur90_count: f64,
    pub dur90_rate: Option<f64>,
    pub dur360_count: f64,
    pub dur360_rate: Option<f64>,
    pub apo_count: f64,
    pub apo_rate: Option<f64>,
    /// 架電数が `SMALL_N_CALLS` 未満 = 参考値(率がブレやすい)。行は消さない
    pub thin: bool,
}

#[derive(Debug, Serialize)]
pub struct IndividualPanel {
    pub year_month: String,
    pub rows: Vec<CallerRow>,
    /// 選択可能な年月一覧(降順)。フロントのセレクタ用
    pub available_months: Vec<String>,
    pub small_n_threshold: f64,
}

/// 選択月のシート「BPOコーラー」行を担当者単位で返す。
///
/// **順位付けはしない**（GAS コメント「数値は事実のみ(順位・評価なし)」）。
/// 既定の並びは担当者名の昇順(中立)。列見出しクリックでの並び替えはフロント側の責務。
fn build_individual(d: &SheetData, month: &str) -> IndividualPanel {
    let mut months_set: HashSet<String> = HashSet::new();
    let mut rows: Vec<CallerRow> = Vec::new();

    for row in &d.rows {
        let ym = norm_month(d.get(row, "年月"));
        if ym.is_empty() {
            continue;
        }
        months_set.insert(ym.clone());
        if ym != month {
            continue;
        }
        let dial = num(d.get(row, "架電数"));
        let dur90 = num(d.get(row, "通話90秒以上数"));
        let dur360 = num(d.get(row, "通話360秒以上数"));
        let apo = num(d.get(row, "アポ数"));
        rows.push(CallerRow {
            name: d.get(row, "担当者").to_string(),
            extension: d.get(row, "内線").to_string(),
            year_month: ym,
            dial_count: dial,
            talk_hours: num(d.get(row, "通話時間h")),
            dur90_count: dur90,
            dur90_rate: rate(dur90, dial),
            dur360_count: dur360,
            dur360_rate: rate(dur360, dial),
            apo_count: apo,
            apo_rate: rate(apo, dial),
            thin: dial < SMALL_N_CALLS,
        });
    }

    // 並びを安定させる(中立=担当者名昇順)
    rows.sort_by(|a, b| a.name.cmp(&b.name));

    let mut available_months: Vec<String> = months_set.into_iter().collect();
    available_months.sort();
    available_months.reverse();

    IndividualPanel {
        year_month: month.to_string(),
        rows,
        available_months,
        small_n_threshold: SMALL_N_CALLS,
    }
}

// ============================================================ ③ データ品質

#[derive(Debug, Serialize)]
pub struct DataQualityRow {
    pub item: String,
    pub value: String,
    pub memo: String,
    /// 値または項目名に「不可/ゴミ/未入力」を含む = 赤字強調(GAS `bad` 判定)
    pub bad: bool,
}

/// シート「BPOデータ品質」（列: 項目,値,メモ）をそのまま返す。
fn build_data_quality(d: &SheetData) -> Vec<DataQualityRow> {
    d.rows
        .iter()
        .map(|row| {
            let item = d.get(row, "項目").to_string();
            let value = d.get(row, "値").to_string();
            let memo = d.get(row, "メモ").to_string();
            let bad = bad_dq_value(&item, &value);
            DataQualityRow { item, value, memo, bad }
        })
        .collect()
}

// ============================================================ ④ ネクストアクション健全性

#[derive(Debug, Serialize)]
pub struct NaStageRow {
    pub stage: String,
    pub category: String,
    pub count: f64,
    /// シート列「次回架電日設定率%」をそのまま数値化(空文字は None)
    pub next_call_set_rate_pct: Option<f64>,
    pub unset_count: f64,
    pub overdue_count: f64,
}

#[derive(Debug, Serialize)]
pub struct NaDetailRow {
    pub elapsed_days: f64,
    pub stage: String,
    pub deal_name: String,
    pub next_due_date: String,
    pub last_call_date: String,
    pub phone: String,
}

#[derive(Debug, Serialize)]
pub struct NaHealthPanel {
    /// 「要フォロー」区分の件数合計
    pub open_follow_count: f64,
    pub unset_count: f64,
    pub overdue_count: f64,
    /// 「要フォロー」区分のステージのみ(「対象外(次回日不要)」は除く)
    pub stages: Vec<NaStageRow>,
    /// 期限超過の明細(経過日数降順)
    pub details: Vec<NaDetailRow>,
    /// シートに埋め込まれた計測日メモ(ステージ列が「計測日:」で始まる行、displayOrder=-1)
    pub measured_at_note: Option<String>,
}

/// シート「BPOネクストアクション」(列: displayOrder,ステージ,区分,件数,
/// 次回架電日設定率%,未設定数,期限超過数)と
/// 「BPOネクストアクション明細」(列: 経過日数,ステージ,取引名,次回予定日,最終架電日,電話)から
/// ④パネルを組み立てる。
///
/// **オープンDealに担当者キーが無いため個人別集計は不可**（GAS 側の注記どおり）。
/// ステージ別(組織単位)でのみ可視化する。
fn build_na_health(na: &SheetData, detail: &SheetData) -> NaHealthPanel {
    let mut measured_at_note = None;
    let mut stages = Vec::new();
    let mut open_follow = 0.0;
    let mut unset = 0.0;
    let mut overdue = 0.0;

    for row in &na.rows {
        let stage = na.get(row, "ステージ").to_string();
        if measured_at_note.is_none() && stage.starts_with("計測日") {
            measured_at_note = Some(stage.clone());
        }
        let category = na.get(row, "区分").to_string();
        if category != "要フォロー" {
            continue;
        }
        let count = num(na.get(row, "件数"));
        let u = num(na.get(row, "未設定数"));
        let o = num(na.get(row, "期限超過数"));
        open_follow += count;
        unset += u;
        overdue += o;
        stages.push(NaStageRow {
            stage,
            category,
            count,
            next_call_set_rate_pct: {
                let raw = na.get(row, "次回架電日設定率%").trim();
                if raw.is_empty() {
                    None
                } else {
                    Some(num(raw))
                }
            },
            unset_count: u,
            overdue_count: o,
        });
    }

    let mut details: Vec<NaDetailRow> = detail
        .rows
        .iter()
        .map(|row| NaDetailRow {
            elapsed_days: num(detail.get(row, "経過日数")),
            stage: detail.get(row, "ステージ").to_string(),
            deal_name: detail.get(row, "取引名").to_string(),
            next_due_date: detail.get(row, "次回予定日").to_string(),
            last_call_date: detail.get(row, "最終架電日").to_string(),
            phone: detail.get(row, "電話").to_string(),
        })
        .collect();
    // 経過日数の降順(再架電すべき先を先頭に)。安定ソートなので同値はシート順を保つ
    details.sort_by(|a, b| {
        b.elapsed_days
            .partial_cmp(&a.elapsed_days)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    NaHealthPanel {
        open_follow_count: open_follow,
        unset_count: unset,
        overdue_count: overdue,
        stages,
        details,
        measured_at_note,
    }
}

// ============================================================ ⑤ 効率のよい架電時間帯

#[derive(Debug, Serialize)]
pub struct HeatCell {
    pub weekday: u32,
    pub hour: u32,
    pub dial_count: f64,
    pub dur_ge_30_rate: Option<f64>,
    pub dur_ge_90_rate: Option<f64>,
    pub dur_ge_360_rate: Option<f64>,
    pub apo_rate: Option<f64>,
    /// 架電数が `SMALL_N_HEATMAP` 未満 = 参考(薄字)
    pub thin: bool,
}

#[derive(Debug, Serialize)]
pub struct HeatmapPanel {
    pub cells: Vec<HeatCell>,
    /// 全期間合算の総架電数(画面の母数表示に使う)
    pub total_dial: f64,
    pub small_n_threshold: f64,
}

/// シート「時間帯ヒート_BPO」(列: weekday,hour,dial_count,dur_ge_30,dur_ge_90,
/// dur_ge_360,apo_count)から曜日×時間帯セルを組み立てる。
///
/// 通話時間の閾値(30/90/360秒)は「繋がった」の代理指標であり接触率ではない
/// （BPOでは接触率そのものが設計上算出不可、ファイル冒頭の注記）。
/// 色スケール(intensity)の計算はフロント側の責務(未実装一覧を参照)。
fn build_heatmap(d: &SheetData) -> HeatmapPanel {
    let mut cells = Vec::new();
    let mut total = 0.0;
    for row in &d.rows {
        let dial = num(d.get(row, "dial_count"));
        total += dial;
        cells.push(HeatCell {
            weekday: num(d.get(row, "weekday")).round() as u32,
            hour: num(d.get(row, "hour")).round() as u32,
            dial_count: dial,
            dur_ge_30_rate: rate(num(d.get(row, "dur_ge_30")), dial),
            dur_ge_90_rate: rate(num(d.get(row, "dur_ge_90")), dial),
            dur_ge_360_rate: rate(num(d.get(row, "dur_ge_360")), dial),
            apo_rate: rate(num(d.get(row, "apo_count")), dial),
            thin: dial < SMALL_N_HEATMAP,
        });
    }
    // 並びを安定させる(曜日, 時間帯 昇順)。HashMap を経由しないので元々安定だが明示する
    cells.sort_by(|a, b| (a.weekday, a.hour).cmp(&(b.weekday, b.hour)));

    HeatmapPanel {
        cells,
        total_dial: total,
        small_n_threshold: SMALL_N_HEATMAP,
    }
}

// ============================================================ 全体

#[derive(Debug, Default, Deserialize)]
pub struct PbpoQuery {
    /// 個人別パネル(②)の対象月(YYYY-MM)。未指定なら直近の確定月(当月除く)
    pub year_month: Option<String>,
    /// テスト用の「現在月」上書き。省略時は実行時のローカル日付
    pub today_ym: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct PbpoData {
    pub monthly_kpi: MonthlyKpiPanel,
    pub individual: IndividualPanel,
    pub data_quality: Vec<DataQualityRow>,
    pub na_health: NaHealthPanel,
    pub heatmap: HeatmapPanel,
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

/// ③④のシートは「週次/手動更新」でありPythonバッチ未実行だと存在しないことがある。
/// 取得失敗はエラーにせず空シート扱いにする(GAS 側も getRawSheet の失敗時は空扱い)。
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
            tracing::warn!(
                "架電クオリティ pbpo: シート「{name}」を読めなかった(週次/手動更新のため未生成の可能性): {e:#}"
            );
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

fn set_matched(sources: &mut [SourceInfo], sheet: &str, n: usize) {
    if let Some(s) = sources.iter_mut().find(|s| s.sheet == sheet) {
        s.matched_rows = n;
    }
}

/// ハンドラ本体。5シートを読み、5パネルを組んで返す。
pub async fn handle(
    client: &SheetsClient,
    store: &SheetStore,
    q: PbpoQuery,
) -> Result<TabPayload<PbpoData>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    // シート「BPOコーラー」は本タブの主データなので取得失敗はエラーにする
    let caller = load(client, store, SHEET_CALLER, &mut sources).await?;
    let dq = get_or_empty(store, client, SHEET_DQ, &mut sources).await;
    let na = get_or_empty(store, client, SHEET_NA, &mut sources).await;
    let na_detail = get_or_empty(store, client, SHEET_NA_DETAIL, &mut sources).await;
    let heatmap = get_or_empty(store, client, SHEET_HEATMAP, &mut sources).await;

    let current_month = q
        .today_ym
        .clone()
        .unwrap_or_else(|| chrono::Local::now().format("%Y-%m").to_string());

    let monthly_kpi = build_monthly_kpi(&caller, &current_month);
    let selected_month = q
        .year_month
        .clone()
        .unwrap_or_else(|| monthly_kpi.latest_complete_month.clone());
    let individual = build_individual(&caller, &selected_month);
    let data_quality = build_data_quality(&dq);
    let na_health = build_na_health(&na, &na_detail);
    let heatmap_panel = build_heatmap(&heatmap);

    set_matched(&mut sources, SHEET_CALLER, individual.rows.len());

    Ok(TabPayload {
        data: PbpoData {
            monthly_kpi,
            individual,
            data_quality,
            na_health,
            heatmap: heatmap_panel,
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

    const CALLER_HEADER: [&str; 11] = [
        "担当者", "内線", "年月", "架電数", "通話時間h", "通話90秒以上率", "通話90秒以上数",
        "通話360秒以上率", "通話360秒以上数", "アポ数", "アポ率",
    ];

    #[test]
    fn 架電0でアポありでもアポ率はnone() {
        // 実データ(2026-08-16確認): 久保 寿代 2026-01 は 架電数=0 なのに アポ数=17。
        // シート上の「アポ率」列は "0" だが、ここでは件数から re-compute するので None。
        let d = sheet(
            &CALLER_HEADER,
            &[&["久保 寿代", "957", "2026-01", "0", "0", "0", "0", "0", "0", "17", "0"]],
        );
        let panel = build_monthly_kpi(&d, "2026-08");
        assert_eq!(panel.rows.len(), 1);
        assert_eq!(panel.rows[0].apo_rate, None, "分母(架電)0はNone。シート列の\"0\"を信じない");
        assert_eq!(panel.rows[0].apo_count, 17.0, "アポ件数自体は消さない");

        let ind = build_individual(&d, "2026-01");
        assert_eq!(ind.rows[0].apo_rate, None);
    }

    #[test]
    fn 全担当者が含まれる_sales_ownersでの絞り込みは存在しない() {
        // このタブはBPO専用画面であり、build_individual/build_monthly_kpiは
        // sales_ownersのような絞り込み引数を一切取らない(意図的な設計、ファイル冒頭の注記)。
        // 3名の異なる担当者が全員出力に残ることでその挙動を固定する。
        let d = sheet(
            &CALLER_HEADER,
            &[
                &["担当A", "100", "2026-06", "300", "10", "30", "90", "5", "15", "10", "3.3"],
                &["担当B", "101", "2026-06", "300", "10", "30", "90", "5", "15", "10", "3.3"],
                &["担当C(BPO)", "102", "2026-06", "300", "10", "30", "90", "5", "15", "10", "3.3"],
            ],
        );
        let ind = build_individual(&d, "2026-06");
        assert_eq!(ind.rows.len(), 3, "BPO担当者は全員残る(営業ロースターとの突合はしない)");
    }

    #[test]
    fn 架電数200未満は参考フラグが立つ() {
        let d = sheet(
            &CALLER_HEADER,
            &[&["少数", "100", "2026-06", "150", "5", "10", "15", "2", "3", "2", "1.3"]],
        );
        let ind = build_individual(&d, "2026-06");
        assert!(ind.rows[0].thin, "架電数200未満はthin");
    }

    #[test]
    fn 当月は集計途中フラグが立ち直近確定月は当月を除く() {
        let d = sheet(
            &CALLER_HEADER,
            &[
                &["A", "100", "2026-05", "500", "10", "30", "150", "5", "25", "5", "1.0"],
                &["A", "100", "2026-06", "100", "2", "6", "6", "1", "1", "1", "1.0"],
            ],
        );
        let panel = build_monthly_kpi(&d, "2026-06");
        let jun = panel.rows.iter().find(|r| r.year_month == "2026-06").unwrap();
        assert!(jun.is_partial);
        assert_eq!(panel.latest_complete_month, "2026-05", "当月を除いた直近月");
    }

    #[test]
    fn na要フォロー以外は集計対象外() {
        let na = sheet(
            &["displayOrder", "ステージ", "区分", "件数", "次回架電日設定率%", "未設定数", "期限超過数"],
            &[
                &["-1", "計測日: 2026-06-10", "週次/手動更新", "", "", "", ""],
                &["0", "未済", "対象外(次回日不要)", "4251", "1.6", "4181", "14"],
                &["3", "不在", "要フォロー", "3364", "70.8", "982", "375"],
            ],
        );
        let detail = sheet(&["経過日数", "ステージ", "取引名", "次回予定日", "最終架電日", "電話"], &[]);
        let panel = build_na_health(&na, &detail);
        assert_eq!(panel.stages.len(), 1, "対象外(次回日不要)は要フォロー集計に含めない");
        assert_eq!(panel.open_follow_count, 3364.0);
        assert_eq!(panel.measured_at_note.as_deref(), Some("計測日: 2026-06-10"));
    }

    #[test]
    fn na明細は経過日数降順で安定ソートされる() {
        let na = sheet(
            &["displayOrder", "ステージ", "区分", "件数", "次回架電日設定率%", "未設定数", "期限超過数"],
            &[],
        );
        let detail = sheet(
            &["経過日数", "ステージ", "取引名", "次回予定日", "最終架電日", "電話"],
            &[
                &["5", "不在", "A社", "2026-06-01", "2026-05-20", "090-1"],
                &["20", "不在", "B社", "2026-05-10", "2026-04-20", "090-2"],
            ],
        );
        let panel = build_na_health(&na, &detail);
        assert_eq!(panel.details[0].deal_name, "B社", "経過日数が大きい方(=再架電すべき先)が先頭");
    }

    #[test]
    fn ヒートマップは分母0でnone_少数はthin() {
        let d = sheet(
            &["weekday", "hour", "dial_count", "dur_ge_30", "dur_ge_90", "dur_ge_360", "apo_count"],
            &[
                &["0", "10", "0", "0", "0", "0", "0"],
                &["1", "11", "50", "20", "10", "2", "1"],
            ],
        );
        let panel = build_heatmap(&d);
        assert_eq!(panel.cells[0].dur_ge_30_rate, None, "架電0のセルはNone(0%と誤読させない)");
        assert!(panel.cells[1].thin, "架電50件は100未満なのでthin");
        assert_eq!(panel.total_dial, 50.0);
    }

    #[test]
    fn データ品質のbad判定() {
        let d = sheet(
            &["項目", "値", "メモ"],
            &[
                &["Aプロパティ充足率", "不可(未入力多数)", "要対応"],
                &["Bプロパティ充足率", "95%", "OK"],
            ],
        );
        let rows = build_data_quality(&d);
        assert!(rows[0].bad, "「不可」を含む値はbad");
        assert!(!rows[1].bad);
    }
}
