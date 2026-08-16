//! 行動量分析（GAS 版 `page-p11`、旧「勝ち筋分析」）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p11">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP11WinningPatterns` / `_drawP11` / `_p11OwnerAgg` / `_p11Score` /
//!          `_drawP11TopBottom` / `_drawP11OwnerRanking` / `_drawP11StageOutcome` / `_drawP11Table`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getConsultingWinningPatterns` / `getConsultingActivityOwnerDaily` /
//!          `getConsultingActivityDaily`）
//!
//! GAS 版画面の注記どおり: アクティブな納品管理PL Deal（解約済・継続済・マーケ関連・
//! CSその他は除外）のステージ別「行動量（メール＋電話＋MTGの合計）」。MTG件数は
//! 現状データ取得できておらず実質メール＋電話。このタブは Python バッチが既に
//! 対象を絞り込んだ後のシートを読むだけで、Rust 側で追加のDealフィルタは行わない。
//!
//! ------------------------------------------------------------------
//! 使用シート
//! ------------------------------------------------------------------
//! 「コンサル勝ち筋分析」（owner × year_month × stage × outcome の集計。
//!   **deal_id / customer 列は存在しない**）
//!   列: owner_id, owner_name, year_month, stage_label, outcome_status,
//!       active_deal_count, touched_days, email_count, call_count, mtg_count,
//!       other_count, total_count, mtg_ratio
//! 「コンサル行動量_担当日次」（owner × 日 の集計。stage/outcome 列は無い）
//!   列: owner_id, owner_name, event_date, active_deal_count, email_count,
//!       call_count, mtg_count, other_count, total_count
//! 「コンサル行動量_日次」（deal × 日 明細。KPIカードの件数表示のみに使用、GAS と同じ）
//!   列: deal_id, deal_label, customer_name, customer_label, owner_id, owner_name,
//!       pipeline_label, stage_label, event_date, email_count, call_count,
//!       mtg_count, other_count, total_count
//!
//! GAS 版はオーナー集計の元データを「勝ち筋」優先・空なら「担当日次」に
//! フォールバックする（`_drawP11` の `source = rows.length ? rows : ownerRows`）。
//! ここでも同じ規則を使う（`pick_source`）。一方 Stage/Outcome 分布と明細テーブルは
//! GAS 側も常に「勝ち筋」だけを見ており（`_drawP11StageOutcome(rows)` /
//! `_drawP11Table(rows)` の `rows` は winning 固定）、ownerDaily にはフォールバックしない
//! （stage/outcome 列自体が無いため）。
//!
//! ------------------------------------------------------------------
//! 団員の明示指示によるランキング方式の変更（2026-08-16）
//! ------------------------------------------------------------------
//! GAS 版 `_p11Score` は outcome_status（継続確定=100 / 解約=0）＋行動量の
//! ハイブリッドスコアで Owner ランキング・上位下位比較・明細を並べていた。
//! 一方、同アプリ内 P8 タブでは 2026-08-13 のレビュー（C18、
//! `docs\wbs_outputs\ダッシュボード機能削減\観点_コンサル.md` の C-3）で
//! 「行動量を測る唯一の指標はコール数（メールが多くても電話が低ければ評価しない）」
//! と現場定義が確定し、Call主軸ランキング＋他指標は補足、という形に是正済み
//! （`javascript.html` 8357-8401行）。
//!
//! 団員の指示で **このタブのランキング（Owner ランキング／上位下位比較／明細）も
//! 同じ Call 主軸に統一する**（P11 独自の outcome_status ベーススコアはランキング
//! キーとして採用しない）。Email／MTG／合計／成果スコア（GAS 互換の参考値として残す）は
//! すべて補足フィールドとして返し、**並び替えのキーには使わない**
//! （各構造体のフィールドコメントに「補足」と明記）。
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! なし。GAS 版 P11 の可視領域（KPI／上位下位比較／Owner ランキング／
//! Stage・Outcome 傾向／勝ち筋明細）はすべて移植済み。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::Serialize;

use super::{rate, SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_WINNING: &str = "コンサル勝ち筋分析";
const SHEET_OWNER_DAILY: &str = "コンサル行動量_担当日次";
const SHEET_DAILY: &str = "コンサル行動量_日次";

/// Owner ランキング表示件数（GAS `agg.slice(0, 15)`）
const OWNER_RANKING_LIMIT: usize = 15;
/// 上位/下位比較の人数（GAS `Math.min(5, agg.length)`）
const TOP_BOTTOM_N: usize = 5;
/// Stage/Outcome 傾向の表示件数（GAS `.slice(0, 12)`）
const STAGE_OUTCOME_LIMIT: usize = 12;
/// 勝ち筋明細の表示件数（GAS `.slice(0, 200)`）
const PATTERN_TABLE_LIMIT: usize = 200;

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

/// GAS `_p11Score` の移植（実データに存在する候補列 `win_rate` 等は無いため、
/// outcome_status ベースの分岐からそのまま始める）。**参考値**であり
/// ランキングのキーには使わない（ファイル冒頭の方針を参照）。
fn score_for_row(outcome_status: &str, contact: f64) -> f64 {
    if outcome_status.contains("継続確定") {
        100.0
    } else if outcome_status.contains("解約") {
        0.0
    } else {
        contact
    }
}

// ================================================================== Owner集計

#[derive(Debug, Clone, Serialize)]
pub struct OwnerActivity {
    pub owner_id: String,
    pub owner_label: String,
    /// 集計対象の行数（何ヶ月×ステージぶんを合算したか）
    pub row_count: usize,
    /// ランキングの唯一の基準（コール数）
    pub call_count: f64,
    /// 補足。ランキングには使わない
    pub email_count: f64,
    /// 補足。ランキングには使わない
    pub mtg_count: f64,
    /// 補足。ランキングには使わない
    pub other_count: f64,
    /// 補足。ランキングには使わない
    pub total_count: f64,
    /// 補足。合計に占める Call の割合。合計0なら None（0%と誤読させない）
    pub call_share_pct: Option<f64>,
    /// 補足（GAS 互換の成果スコア平均、参考値）。ランキングには使わない
    pub avg_score: f64,
}

/// シートを owner 単位に畳む。`コンサル勝ち筋分析`・`コンサル行動量_担当日次` の
/// どちらでも動く（後者には outcome_status/stage_label 列が無いため、
/// `score_for_row` は常に行動量フォールバックになる）。
fn aggregate_owners(data: &SheetData) -> Vec<OwnerActivity> {
    struct Acc {
        label: String,
        rows: usize,
        email: f64,
        call: f64,
        mtg: f64,
        other: f64,
        total: f64,
        score_sum: f64,
    }

    let mut acc: HashMap<String, Acc> = HashMap::new();
    for row in &data.rows {
        let owner_id = data.get(row, "owner_id").to_string();
        if owner_id.is_empty() {
            continue;
        }
        let owner_label = {
            let n = data.get(row, "owner_name").trim();
            if n.is_empty() { owner_id.clone() } else { n.to_string() }
        };
        let email = num(data.get(row, "email_count"));
        let call = num(data.get(row, "call_count"));
        let mtg = num(data.get(row, "mtg_count"));
        let other = num(data.get(row, "other_count"));
        let total_raw = data.get(row, "total_count").trim();
        let total = if total_raw.is_empty() { email + call + mtg + other } else { num(total_raw) };
        let score = score_for_row(data.get(row, "outcome_status"), total);

        let e = acc.entry(owner_id).or_insert_with(|| Acc {
            label: owner_label,
            rows: 0,
            email: 0.0,
            call: 0.0,
            mtg: 0.0,
            other: 0.0,
            total: 0.0,
            score_sum: 0.0,
        });
        e.rows += 1;
        e.email += email;
        e.call += call;
        e.mtg += mtg;
        e.other += other;
        e.total += total;
        e.score_sum += score;
    }

    let mut out: Vec<OwnerActivity> = acc
        .into_iter()
        .map(|(owner_id, a)| OwnerActivity {
            call_count: a.call,
            email_count: a.email,
            mtg_count: a.mtg,
            other_count: a.other,
            total_count: a.total,
            call_share_pct: rate(a.call, a.total),
            avg_score: if a.rows > 0 { a.score_sum / a.rows as f64 } else { 0.0 },
            row_count: a.rows,
            owner_id,
            owner_label: a.label,
        })
        .collect();

    // 安定化（HashMap の反復順を返さない）
    out.sort_by(|a, b| a.owner_id.cmp(&b.owner_id));
    out
}

/// GAS `_drawP11` の `source = rows.length ? rows : ownerRows` と同じ規則。
/// 「勝ち筋」が空なら「担当日次」にフォールバックする。
fn pick_source<'a>(winning: &'a [OwnerActivity], owner_daily: &'a [OwnerActivity]) -> (&'a [OwnerActivity], &'static str) {
    if !winning.is_empty() {
        (winning, SHEET_WINNING)
    } else {
        (owner_daily, SHEET_OWNER_DAILY)
    }
}

#[derive(Debug, Serialize)]
pub struct OwnerRankingPanel {
    pub rows: Vec<OwnerActivity>,
    /// 上限適用前の対象Owner数
    pub total: usize,
    /// `OWNER_RANKING_LIMIT` で切ったか（約束3: 黙って上位N件にしない）
    pub truncated: bool,
}

/// Owner ランキング。**Call 降順が唯一の基準**（同数は owner_id で安定化）。
pub fn build_owner_ranking(source: &[OwnerActivity]) -> OwnerRankingPanel {
    let mut v: Vec<OwnerActivity> = source.to_vec();
    v.sort_by(|a, b| {
        b.call_count
            .partial_cmp(&a.call_count)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    let total = v.len();
    let truncated = total > OWNER_RANKING_LIMIT;
    v.truncate(OWNER_RANKING_LIMIT);
    OwnerRankingPanel { rows: v, total, truncated }
}

// ================================================================== 上位/下位比較

#[derive(Debug, Serialize)]
pub struct GroupAverage {
    pub label: &'static str,
    pub n: usize,
    /// このグループを選んだ基準（Call 平均）
    pub avg_call: f64,
    /// 補足
    pub avg_email: f64,
    /// 補足
    pub avg_mtg: f64,
    /// 補足
    pub avg_total: f64,
    /// 補足（成果スコア平均、参考値）
    pub avg_score: f64,
}

#[derive(Debug, Serialize)]
pub struct TopBottomComparison {
    pub top: GroupAverage,
    pub bottom: GroupAverage,
}

fn avg_group(label: &'static str, g: &[&OwnerActivity]) -> GroupAverage {
    let denom = if g.is_empty() { 1.0 } else { g.len() as f64 };
    GroupAverage {
        label,
        n: g.len(),
        avg_call: g.iter().map(|o| o.call_count).sum::<f64>() / denom,
        avg_email: g.iter().map(|o| o.email_count).sum::<f64>() / denom,
        avg_mtg: g.iter().map(|o| o.mtg_count).sum::<f64>() / denom,
        avg_total: g.iter().map(|o| o.total_count).sum::<f64>() / denom,
        avg_score: g.iter().map(|o| o.avg_score).sum::<f64>() / denom,
    }
}

/// 上位5名 / 下位5名（Call 基準）の平均比較。GAS `_drawP11TopBottom` の移植だが
/// 選定基準を成果スコアから Call に変更（ファイル冒頭の方針）。
pub fn build_top_bottom(source: &[OwnerActivity]) -> TopBottomComparison {
    let mut sorted: Vec<&OwnerActivity> = source.iter().collect();
    sorted.sort_by(|a, b| {
        b.call_count
            .partial_cmp(&a.call_count)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
    });
    let n = sorted.len();
    let take = TOP_BOTTOM_N.min(n);
    let top: Vec<&OwnerActivity> = sorted[..take].to_vec();
    let bottom: Vec<&OwnerActivity> = sorted[n - take..].iter().rev().copied().collect();
    TopBottomComparison {
        top: avg_group("上位", &top),
        bottom: avg_group("下位", &bottom),
    }
}

// ================================================================== Stage/Outcome傾向

#[derive(Debug, Serialize)]
pub struct StageOutcomeCount {
    pub stage: String,
    pub outcome_status: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct StageOutcomePanel {
    pub rows: Vec<StageOutcomeCount>,
    /// 絞り込み前の組み合わせ総数
    pub total_combinations: usize,
    /// `STAGE_OUTCOME_LIMIT` で切ったか（約束3: 黙って上位N件にしない）
    pub truncated: bool,
}

/// 「Stage × Outcome区分」の件数分布。GAS `_drawP11StageOutcome` の移植。
/// 常に「勝ち筋」シート由来（`コンサル行動量_担当日次` には stage/outcome 列が無い）。
pub fn build_stage_outcome(winning: &SheetData) -> StageOutcomePanel {
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for row in &winning.rows {
        let stage = {
            let s = winning.get(row, "stage_label").trim();
            if s.is_empty() { "Stage未設定".to_string() } else { s.to_string() }
        };
        let outcome = {
            let o = winning.get(row, "outcome_status").trim();
            if o.is_empty() { "未分類".to_string() } else { o.to_string() }
        };
        *counts.entry((stage, outcome)).or_insert(0) += 1;
    }
    let total_combinations = counts.len();
    let mut rows: Vec<StageOutcomeCount> = counts
        .into_iter()
        .map(|((stage, outcome_status), count)| StageOutcomeCount { stage, outcome_status, count })
        .collect();
    // 件数降順。同数は stage → outcome_status で安定化
    rows.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.stage.cmp(&b.stage))
            .then_with(|| a.outcome_status.cmp(&b.outcome_status))
    });
    let truncated = total_combinations > STAGE_OUTCOME_LIMIT;
    rows.truncate(STAGE_OUTCOME_LIMIT);
    StageOutcomePanel { rows, total_combinations, truncated }
}

// ================================================================== 勝ち筋明細

#[derive(Debug, Serialize, Clone)]
pub struct PatternRow {
    pub owner_id: String,
    pub owner_label: String,
    pub year_month: String,
    pub stage: String,
    pub outcome_status: String,
    /// 順位の基準（唯一の指標）
    pub call_count: f64,
    /// 補足
    pub email_count: f64,
    /// 補足
    pub mtg_count: f64,
    /// 補足
    pub other_count: f64,
    /// 補足
    pub total_count: f64,
    pub touched_days: Option<f64>,
    pub mtg_ratio: Option<f64>,
    /// 補足（成果スコア、参考値）
    pub score: f64,
    /// GAS 版の「顧客」列相当。「コンサル勝ち筋分析」に deal_id/customer 列は
    /// 存在しないため常に "-"（推測で埋めない）
    pub customer_label: &'static str,
}

#[derive(Debug, Serialize)]
pub struct PatternPanel {
    pub rows: Vec<PatternRow>,
    pub total: usize,
    pub truncated: bool,
}

/// 勝ち筋明細テーブル。GAS `_drawP11Table` の移植だが並び順を成果スコアから
/// Call に変更（ファイル冒頭の方針）。
pub fn build_pattern_table(winning: &SheetData) -> PatternPanel {
    let mut rows: Vec<PatternRow> = winning
        .rows
        .iter()
        .map(|row| {
            let owner_id = winning.get(row, "owner_id").to_string();
            let owner_label = {
                let n = winning.get(row, "owner_name").trim();
                if n.is_empty() { owner_id.clone() } else { n.to_string() }
            };
            let email = num(winning.get(row, "email_count"));
            let call = num(winning.get(row, "call_count"));
            let mtg = num(winning.get(row, "mtg_count"));
            let other = num(winning.get(row, "other_count"));
            let total_raw = winning.get(row, "total_count").trim();
            let total = if total_raw.is_empty() { email + call + mtg + other } else { num(total_raw) };
            let outcome_status = winning.get(row, "outcome_status").to_string();
            PatternRow {
                owner_id,
                owner_label,
                year_month: winning.get(row, "year_month").to_string(),
                stage: {
                    let s = winning.get(row, "stage_label").trim();
                    if s.is_empty() { "Stage未設定".to_string() } else { s.to_string() }
                },
                score: score_for_row(&outcome_status, total),
                outcome_status: if outcome_status.trim().is_empty() { "未分類".to_string() } else { outcome_status },
                call_count: call,
                email_count: email,
                mtg_count: mtg,
                other_count: other,
                total_count: total,
                touched_days: opt_num(winning.get(row, "touched_days")),
                mtg_ratio: opt_num(winning.get(row, "mtg_ratio")),
                customer_label: "-",
            }
        })
        .collect();

    let total = rows.len();
    rows.sort_by(|a, b| {
        b.call_count
            .partial_cmp(&a.call_count)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.owner_id.cmp(&b.owner_id))
            .then_with(|| a.year_month.cmp(&b.year_month))
    });
    let truncated = total > PATTERN_TABLE_LIMIT;
    rows.truncate(PATTERN_TABLE_LIMIT);
    PatternPanel { rows, total, truncated }
}

// ================================================================== KPI

#[derive(Debug, Serialize)]
pub struct P11Kpis {
    /// 「行動量レコード」＝勝ち筋シートの行数
    pub winning_rows: usize,
    /// 「担当日次」の行数
    pub owner_daily_rows: usize,
    /// 「日次明細」の行数
    pub daily_rows: usize,
    /// 集計可能な対象Owner数（実際に使われたソースから算出）
    pub owner_count: usize,
    /// KPI/ランキングの集計元になったシート名（"勝ち筋" が空なら "担当日次" にフォールバック）
    pub source_sheet: &'static str,
}

// ================================================================== 全体

#[derive(Debug, Serialize)]
pub struct P11Data {
    pub kpis: P11Kpis,
    pub top_bottom: TopBottomComparison,
    pub owner_ranking: OwnerRankingPanel,
    pub stage_outcome: StageOutcomePanel,
    pub pattern_table: PatternPanel,
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

/// GAS 版は上部フィルタ（期間/PL/メンバー/都道府県）を反映しない
/// （`renderP11WinningPatterns()` は引数なしで呼ばれる。P13/P14/P15 と同様のパターン）。
/// そのためこのタブに Query 引数は無い。
pub async fn handle(client: &SheetsClient, store: &SheetStore) -> Result<TabPayload<P11Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let winning_data = load(client, store, SHEET_WINNING, &mut sources).await?;
    let owner_daily_data = load(client, store, SHEET_OWNER_DAILY, &mut sources).await?;
    let daily_data = load(client, store, SHEET_DAILY, &mut sources).await?;

    let winning_owners = aggregate_owners(&winning_data);
    let owner_daily_owners = aggregate_owners(&owner_daily_data);
    let (source, source_sheet) = pick_source(&winning_owners, &owner_daily_owners);

    let owner_ranking = build_owner_ranking(source);
    let top_bottom = build_top_bottom(source);
    let stage_outcome = build_stage_outcome(&winning_data);
    let pattern_table = build_pattern_table(&winning_data);

    let kpis = P11Kpis {
        winning_rows: winning_data.rows.len(),
        owner_daily_rows: owner_daily_data.rows.len(),
        daily_rows: daily_data.rows.len(),
        owner_count: source.len(),
        source_sheet,
    };

    Ok(TabPayload {
        data: P11Data { kpis, top_bottom, owner_ranking, stage_outcome, pattern_table },
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

    const WINNING_HEADER: [&str; 13] = [
        "owner_id", "owner_name", "year_month", "stage_label", "outcome_status",
        "active_deal_count", "touched_days", "email_count", "call_count", "mtg_count",
        "other_count", "total_count", "mtg_ratio",
    ];

    fn winning_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: WINNING_HEADER.iter().map(|s| s.to_string()).collect(),
            rows: rows.into_iter().map(|r| arc_row(&r)).collect(),
            fetched_at: Instant::now(),
        }
    }

    #[test]
    fn owner_ranking_はcall数の降順でメールが多くても順位を上げない() {
        // 「行動量を測る唯一の指標はコール数」(C18/C-3) の検証。
        // owner "email_heavy" は email が圧倒的に多いが call は少ない。
        let d = winning_sheet(vec![
            vec!["email_heavy", "田中", "2026-05", "St", "進行中", "1", "10", "500", "5", "0", "0", "505", "0"],
            vec!["call_heavy", "鈴木", "2026-05", "St", "進行中", "1", "10", "10", "300", "0", "0", "310", "0"],
        ]);
        let owners = aggregate_owners(&d);
        let panel = build_owner_ranking(&owners);
        assert_eq!(panel.rows[0].owner_id, "call_heavy", "call_countが多いほうが1位でなければならない");
        assert_eq!(panel.rows[1].owner_id, "email_heavy");
        assert!(!panel.truncated, "2名しかいないので上限15に収まる");
    }

    #[test]
    fn owner_rankingは上限15件で切りtruncatedが立つ() {
        let ids: Vec<String> = (0..20).map(|i| format!("o{i}")).collect();
        let rows: Vec<Vec<Arc<str>>> = ids
            .iter()
            .map(|id| arc_row(&[id.as_str(), "田中", "2026-05", "St", "進行中", "1", "1", "0", "1", "0", "0", "1", "0"]))
            .collect();
        let d = SheetData {
            header: WINNING_HEADER.iter().map(|s| s.to_string()).collect(),
            rows,
            fetched_at: Instant::now(),
        };
        let owners = aggregate_owners(&d);
        let panel = build_owner_ranking(&owners);
        assert_eq!(panel.rows.len(), OWNER_RANKING_LIMIT);
        assert_eq!(panel.total, 20);
        assert!(panel.truncated);
    }

    #[test]
    fn owner集計は同一ownerの複数行を合算する() {
        let d = winning_sheet(vec![
            vec!["1", "田中", "2026-04", "St1", "進行中", "1", "5", "1", "10", "0", "0", "11", "0"],
            vec!["1", "田中", "2026-05", "St2", "進行中", "1", "5", "2", "20", "0", "0", "22", "0"],
        ]);
        let owners = aggregate_owners(&d);
        assert_eq!(owners.len(), 1);
        assert_eq!(owners[0].call_count, 30.0);
        assert_eq!(owners[0].row_count, 2);
    }

    #[test]
    fn call_share_pctは合計0でnone() {
        let d = winning_sheet(vec![vec!["1", "田中", "2026-05", "St", "進行中", "0", "0", "0", "0", "0", "0", "0", "0"]]);
        let owners = aggregate_owners(&d);
        assert_eq!(owners[0].call_share_pct, None, "合計0を0%と誤読させない");
    }

    #[test]
    fn スコアはoutcome_statusで決まり継続確定は100解約は0() {
        assert_eq!(score_for_row("継続確定", 999.0), 100.0);
        assert_eq!(score_for_row("解約済", 999.0), 0.0);
        assert_eq!(score_for_row("進行中", 42.0), 42.0, "結果未確定は行動量そのまま");
    }

    #[test]
    fn 上位下位比較はcall基準で選ばれた平均を返す() {
        let d = winning_sheet(vec![
            vec!["a", "A", "2026-05", "St", "進行中", "1", "1", "0", "100", "0", "0", "100", "0"],
            vec!["b", "B", "2026-05", "St", "進行中", "1", "1", "0", "50", "0", "0", "50", "0"],
            vec!["c", "C", "2026-05", "St", "進行中", "1", "1", "0", "10", "0", "0", "10", "0"],
        ]);
        let owners = aggregate_owners(&d);
        let cmp = build_top_bottom(&owners);
        assert_eq!(cmp.top.n, 3, "対象3名なら上位グループもmin(5,3)=3名");
        // 上位(全員)平均=(100+50+10)/3、下位(全員をreverse)も同じ3名なので平均は同じになる
        assert!((cmp.top.avg_call - cmp.bottom.avg_call).abs() < 1e-9);
    }

    #[test]
    fn stage_outcomeは12件超で切りtruncatedが立つ() {
        let stages: Vec<String> = (0..15).map(|i| format!("Stage{i}")).collect();
        let rows: Vec<Vec<Arc<str>>> = stages
            .iter()
            .map(|stage| {
                arc_row(&["1", "田中", "2026-05", stage.as_str(), "進行中", "1", "1", "0", "1", "0", "0", "1", "0"])
            })
            .collect();
        let d = SheetData {
            header: WINNING_HEADER.iter().map(|s| s.to_string()).collect(),
            rows,
            fetched_at: Instant::now(),
        };
        let panel = build_stage_outcome(&d);
        assert_eq!(panel.rows.len(), STAGE_OUTCOME_LIMIT);
        assert_eq!(panel.total_combinations, 15);
        assert!(panel.truncated);
    }

    #[test]
    fn pattern_tableはcall降順で顧客列は常にハイフン() {
        let d = winning_sheet(vec![
            vec!["a", "A", "2026-05", "St", "進行中", "1", "1", "0", "5", "0", "0", "5", "0"],
            vec!["b", "B", "2026-05", "St", "進行中", "1", "1", "0", "50", "0", "0", "50", "0"],
        ]);
        let panel = build_pattern_table(&d);
        assert_eq!(panel.rows[0].owner_id, "b", "call_count降順");
        assert_eq!(panel.rows[0].customer_label, "-", "勝ち筋シートにcustomer列は無い");
        assert_eq!(panel.total, 2);
        assert!(!panel.truncated);
    }

    #[test]
    fn 勝ち筋が空なら担当日次にフォールバックする() {
        let empty = winning_sheet(vec![]);
        let owner_daily_header = [
            "owner_id", "owner_name", "event_date", "active_deal_count",
            "email_count", "call_count", "mtg_count", "other_count", "total_count",
        ];
        let owner_daily = SheetData {
            header: owner_daily_header.iter().map(|s| s.to_string()).collect(),
            rows: vec![arc_row(&["1", "田中", "2026-05-01", "1", "0", "3", "0", "0", "3"])],
            fetched_at: Instant::now(),
        };
        let winning_owners = aggregate_owners(&empty);
        let owner_daily_owners = aggregate_owners(&owner_daily);
        let (source, sheet) = pick_source(&winning_owners, &owner_daily_owners);
        assert_eq!(sheet, SHEET_OWNER_DAILY);
        assert_eq!(source.len(), 1);
    }
}
