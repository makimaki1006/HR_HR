//! 行動量分析（旧「勝ち筋分析」、GAS 版 `page-p11`）
//!
//! 2026-08-16 移植。GAS 側の正本:
//!   画面 `scripts\gas\call_quality_app\index.html` の `<div class="page" id="page-p11">`
//!   描画 `scripts\gas\call_quality_app\javascript.html`
//!        （`renderP11WinningPatterns` / `_drawP11` / `_p11OwnerAgg` / `_p11Score` /
//!          `_p11Contact` / `_drawP11TopBottom` / `_drawP11OwnerRanking` /
//!          `_drawP11StageOutcome` / `_drawP11Table`）
//!   取得 `scripts\gas\call_quality_app\Code.gs`
//!        （`getConsultingWinningPatterns` / `getConsultingActivityOwnerDaily` /
//!          `getConsultingActivityDaily`）
//!
//! **画面名は「勝ち筋分析」から「行動量分析」に変わっているが中身は同じ**。
//! index.html の見方説明（`#page-p11` の `<details>`）が明言する通り、
//! 上位/下位は**成果スコア(行動量ベース)の順位**であり、成約率・継続率・効果量ではない。
//! 継続/解約が確定した Deal だけ 100/0点、それ以外（進行中・提案中=結果未確定）は
//! 行動量をそのままスコアにしている。**MTG件数は現状データ取得できておらず常に0**
//! （index.html の注意書きどおり。実データでも `mtg_count` 列は全行 `0`）。
//!
//! ------------------------------------------------------------------
//! 移植した領域（GAS の DOM id → このファイルの出力）
//! ------------------------------------------------------------------
//!   p11-kpis                 → `P11Kpis`
//!   p11-top-bottom-chart     → `TopBottomPanel`
//!   p11-owner-ranking        → `OwnerRankingPanel`
//!   p11-stage-outcome-chart  → `StageOutcomePanel`
//!   p11-pattern-table        → `PatternTablePanel`
//!
//! ------------------------------------------------------------------
//! 未実装（黙って省略しないための一覧）
//! ------------------------------------------------------------------
//! なし。GAS 版 P11 の可視領域はすべて上記4パネル+KPIに移植済み。
//! 「コンサル行動量_日次」シートは GAS 版でも KPI の件数表示にしか使われておらず
//! （`_drawP11` 参照。`dailyRows` は他のどのパネルにも渡っていない）、
//! そのまま `P11Kpis::daily_rows` として同じ役割だけ移植した。
//!
//! ------------------------------------------------------------------
//! GAS と意図的に違えた点（完了条件4）
//! ------------------------------------------------------------------
//! - 「コンサル勝ち筋分析」シートには `deal_id` / `deal_label` / `customer_label` 列が
//!   存在しない（owner×年月×stage の集計行であり、Deal 単位ではない）。GAS 版
//!   `displayDealLabel(r)` はこのシートに対して常に `"-"` を返しており、
//!   `PatternRow::customer_label` も同じく実質的に常に `"-"` になる。これは移植ミスではなく
//!   **GAS 版から存在する仕様上の制約**であることをここに明記する（黙って直さない）。
//! - `_p11Score` の勝敗判定 regex（`/継続確定|win|won|...|成約|良好/` /
//!   `/解約|lost|fail|失注|悪化/`）は GAS 側に `i` フラグが無く**大文字小文字を区別する**。
//!   ここでは `regex` クレートを新規追加せず（このタブ担当の変更範囲外のファイルである
//!   `Cargo.toml` を触らないため）、`str::contains` の OR 列挙で同じ判定を再現した。
//!   結果は正規表現版と同一（`win`/`won`/`success` の大文字表記は一致しない点も含めて)。

use std::sync::Arc;
use std::time::Instant;

use anyhow::Result;
use serde::{Deserialize, Serialize};

use super::{SourceInfo, TabPayload};
use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

// ---------------------------------------------------------------- シート名

const SHEET_WINNING: &str = "コンサル勝ち筋分析";
const SHEET_OWNER_DAILY: &str = "コンサル行動量_担当日次";
const SHEET_DAILY: &str = "コンサル行動量_日次";

/// Owner ランキングの表示件数（GAS `.slice(0, 15)`）
const OWNER_RANKING_LIMIT: usize = 15;
/// Stage/Outcome 傾向の表示件数（GAS `.slice(0, 12)`）
const STAGE_OUTCOME_LIMIT: usize = 12;
/// 明細テーブルの表示件数（GAS `.slice(0, 200)`）
const PATTERN_TABLE_LIMIT: usize = 200;
/// 上位/下位比較の各グループ人数（GAS `Math.min(5, agg.length)`）
const TOP_BOTTOM_GROUP_SIZE: usize = 5;

// ---------------------------------------------------------------- 小道具

fn num(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

/// GAS `displayOwnerLabel` の移植。
fn owner_label(owner_name: &str, owner_id: &str) -> String {
    for v in [owner_name, owner_id] {
        let v = v.trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    "-".to_string()
}

/// GAS `displayDealLabel` の移植。「コンサル勝ち筋分析」シートには
/// deal_label/customer_label/customer_name/deal_id のどれも無いため、常に "-" になる
/// （ファイル冒頭「GASと意図的に違えた点」参照。仕様どおりであり不具合ではない）。
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

/// GAS `_p11Contact` の移植: `total_count` を優先し、0/欠損なら内訳の合計にフォールバック。
/// **`other_count` は含めない**（GAS 版がそもそも足していない。忠実に再現する）。
fn p11_contact(data: &SheetData, row: &[Arc<str>]) -> f64 {
    let total = num(data.get(row, "total_count"));
    if total != 0.0 {
        return total;
    }
    num(data.get(row, "email_count"))
        + num(data.get(row, "call_count"))
        + num(data.get(row, "mtg_count"))
        + num(data.get(row, "mtg_total_count"))
}

/// GAS `_p11Score` 用の候補列。`bool` は「rate系(0〜1.5の範囲なら%に換算)」かどうか。
const SCORE_CANDIDATE_COLS: &[(&str, bool)] = &[
    ("win_rate", true),
    ("success_rate", true),
    ("outcome_score", false),
    ("health_score", false),
    ("continue_intent", false),
    ("consultant_eval", false),
    ("score", false),
];

/// 継続確定/成功 系のキーワード（GAS 正規表現 `/継続確定|win|won|success|成約|良好/` の移植。
/// 元の JS 正規表現に `i` フラグは無く、大文字小文字を区別する）。
const OUTCOME_WIN_KEYWORDS: &[&str] = &["継続確定", "win", "won", "success", "成約", "良好"];
/// 解約/失敗 系のキーワード（GAS `/解約|lost|fail|失注|悪化/` の移植）。
const OUTCOME_LOSS_KEYWORDS: &[&str] = &["解約", "lost", "fail", "失注", "悪化"];

/// GAS `_p11Score` の移植。
/// 1. win_rate 等の直接スコア列があればそれを使う(rate系は0〜1.5なら%に換算)
/// 2. 無ければ outcome_status(等)を見て 継続確定=100 / 解約=0
/// 3. どちらも無ければ接触量(`p11_contact`)をスコア代わりにする
fn p11_score(data: &SheetData, row: &[Arc<str>]) -> f64 {
    for &(col, is_rate) in SCORE_CANDIDATE_COLS {
        let raw = data.get(row, col).trim();
        if raw.is_empty() {
            continue;
        }
        let n = num(raw);
        return if is_rate && n > 0.0 && n <= 1.5 { n * 100.0 } else { n };
    }
    let outcome = outcome_text(data, row);
    if OUTCOME_WIN_KEYWORDS.iter().any(|k| outcome.contains(k)) {
        return 100.0;
    }
    if OUTCOME_LOSS_KEYWORDS.iter().any(|k| outcome.contains(k)) {
        return 0.0;
    }
    p11_contact(data, row)
}

/// GAS `r.outcome_status || r.outcome || r.result || r.outcome_label` の移植。
fn outcome_text(data: &SheetData, row: &[Arc<str>]) -> String {
    for col in ["outcome_status", "outcome", "result", "outcome_label"] {
        let v = data.get(row, col).trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    String::new()
}

/// 表示用の Outcome（未分類フォールバック込み）。GAS `_drawP11StageOutcome` /
/// `_drawP11Table` が使う `(r.outcome_status && r.outcome_status.trim()) || '未分類'` の移植。
fn outcome_display(data: &SheetData, row: &[Arc<str>]) -> String {
    let v = data.get(row, "outcome_status").trim();
    if v.is_empty() {
        "未分類".to_string()
    } else {
        v.to_string()
    }
}

/// GAS `r.stage_label || r.stage || r.pipeline_stage || 'Stage未設定'` の移植。
fn stage_display(data: &SheetData, row: &[Arc<str>]) -> String {
    for col in ["stage_label", "stage", "pipeline_stage"] {
        let v = data.get(row, col).trim();
        if !v.is_empty() {
            return v.to_string();
        }
    }
    "Stage未設定".to_string()
}

// ================================================================== Owner集計

#[derive(Debug, Serialize, Clone)]
pub struct OwnerAgg {
    pub owner_label: String,
    /// 集計対象の行数（GAS `x.rows`）
    pub rows: usize,
    /// 成果スコア(行動量ベース)の平均
    pub avg_score: f64,
    /// 接触量の合計
    pub contact_total: f64,
    /// MTG件数の合計（現状データ未取得のため常に0。ファイル冒頭の注記参照）
    pub mtg_total: f64,
}

/// GAS `_p11OwnerAgg` の移植。owner_name(無ければ owner_id) でグルーピングする
/// （owner_id ではなく **表示名** でグルーピングする点に注意。同名別IDは1件に混ざる。
/// GAS 版がそうなっているため、ここでも同じ挙動にする）。
fn owner_agg(data: &SheetData) -> Vec<OwnerAgg> {
    use std::collections::HashMap;
    let mut m: HashMap<String, (usize, f64, f64, f64)> = HashMap::new(); // (rows, score_sum, contact_sum, mtg_sum)
    for row in &data.rows {
        let owner = owner_label(data.get(row, "owner_name"), data.get(row, "owner_id"));
        let e = m.entry(owner).or_insert((0, 0.0, 0.0, 0.0));
        e.0 += 1;
        e.1 += p11_score(data, row);
        e.2 += p11_contact(data, row);
        e.3 += num(data.get(row, "mtg_count")) + num(data.get(row, "mtg_total_count"));
    }
    let mut agg: Vec<OwnerAgg> = m
        .into_iter()
        .map(|(owner_label, (rows, score_sum, contact_total, mtg_total))| OwnerAgg {
            owner_label,
            rows,
            avg_score: if rows > 0 { score_sum / rows as f64 } else { 0.0 },
            contact_total,
            mtg_total,
        })
        .collect();
    agg.sort_by(|a, b| {
        b.avg_score
            .partial_cmp(&a.avg_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.contact_total.partial_cmp(&a.contact_total).unwrap_or(std::cmp::Ordering::Equal))
            .then_with(|| a.owner_label.cmp(&b.owner_label))
    });
    agg
}

// ================================================================== 上位/下位比較

#[derive(Debug, Serialize)]
pub struct TopBottomGroup {
    pub label: &'static str,
    pub n: usize,
    pub avg_score: f64,
    pub avg_contact: f64,
    pub avg_mtg: f64,
}

#[derive(Debug, Serialize)]
pub struct TopBottomPanel {
    pub top: TopBottomGroup,
    pub bottom: TopBottomGroup,
}

fn build_top_bottom(agg: &[OwnerAgg]) -> TopBottomPanel {
    let n = agg.len();
    let group_n = TOP_BOTTOM_GROUP_SIZE.min(n);
    let top: Vec<&OwnerAgg> = agg.iter().take(group_n).collect();
    // GAS: `agg.slice(Math.max(0, len - min(5,len))).reverse()`。
    // 末尾 group_n 件を取り、reverse して「最下位から順」にする。
    let mut bottom: Vec<&OwnerAgg> = agg.iter().skip(n.saturating_sub(group_n)).collect();
    bottom.reverse();

    fn avg_of<'a>(arr: &[&'a OwnerAgg], f: impl Fn(&'a OwnerAgg) -> f64) -> f64 {
        if arr.is_empty() {
            return 0.0;
        }
        arr.iter().map(|x| f(x)).sum::<f64>() / arr.len() as f64
    }

    TopBottomPanel {
        top: TopBottomGroup {
            label: "上位",
            n: top.len(),
            avg_score: avg_of(&top, |x| x.avg_score),
            avg_contact: avg_of(&top, |x| x.contact_total),
            avg_mtg: avg_of(&top, |x| x.mtg_total),
        },
        bottom: TopBottomGroup {
            label: "下位",
            n: bottom.len(),
            avg_score: avg_of(&bottom, |x| x.avg_score),
            avg_contact: avg_of(&bottom, |x| x.contact_total),
            avg_mtg: avg_of(&bottom, |x| x.mtg_total),
        },
    }
}

// ================================================================== Stage/Outcome

#[derive(Debug, Serialize, Clone)]
pub struct StageOutcomeCount {
    pub stage: String,
    pub outcome: String,
    pub count: usize,
}

#[derive(Debug, Serialize)]
pub struct StageOutcomePanel {
    /// 件数降順、上位12件
    pub top: Vec<StageOutcomeCount>,
    /// 絞り込み前の組み合わせ総数
    pub total_groups: usize,
    /// `STAGE_OUTCOME_LIMIT` で切ったか(約束3: 黙って上位N件にしない)
    pub truncated: bool,
}

fn build_stage_outcome(data: &SheetData) -> StageOutcomePanel {
    use std::collections::HashMap;
    let mut counts: HashMap<(String, String), usize> = HashMap::new();
    for row in &data.rows {
        let key = (stage_display(data, row), outcome_display(data, row));
        *counts.entry(key).or_insert(0) += 1;
    }
    let mut all: Vec<StageOutcomeCount> = counts
        .into_iter()
        .map(|((stage, outcome), count)| StageOutcomeCount { stage, outcome, count })
        .collect();
    all.sort_by(|a, b| {
        b.count
            .cmp(&a.count)
            .then_with(|| a.stage.cmp(&b.stage))
            .then_with(|| a.outcome.cmp(&b.outcome))
    });
    let total_groups = all.len();
    let truncated = total_groups > STAGE_OUTCOME_LIMIT;
    all.truncate(STAGE_OUTCOME_LIMIT);
    StageOutcomePanel { top: all, total_groups, truncated }
}

// ================================================================== 明細テーブル

#[derive(Debug, Serialize, Clone)]
pub struct PatternRow {
    pub owner_label: String,
    /// 「コンサル勝ち筋分析」シートには Deal を特定する列が無いため、常に "-"
    /// （ファイル冒頭「GASと意図的に違えた点」参照）
    pub customer_label: String,
    pub stage: String,
    pub outcome: String,
    pub score: f64,
    pub contact: f64,
    pub insight: String,
}

#[derive(Debug, Serialize)]
pub struct PatternTablePanel {
    pub rows: Vec<PatternRow>,
    pub total: usize,
    pub truncated: bool,
}

fn build_pattern_table(data: &SheetData) -> PatternTablePanel {
    let mut rows: Vec<PatternRow> = data
        .rows
        .iter()
        .map(|row| PatternRow {
            owner_label: owner_label(data.get(row, "owner_name"), data.get(row, "owner_id")),
            customer_label: deal_label(
                data.get(row, "deal_id"),
                data.get(row, "deal_label"),
                data.get(row, "customer_label"),
                data.get(row, "customer_name"),
            ),
            stage: stage_display(data, row),
            outcome: outcome_display(data, row),
            score: p11_score(data, row),
            contact: p11_contact(data, row),
            insight: {
                let v = ["insight", "pattern", "note"]
                    .iter()
                    .map(|c| data.get(row, c).trim())
                    .find(|s| !s.is_empty());
                v.unwrap_or("-").to_string()
            },
        })
        .collect();
    rows.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    let total = rows.len();
    let truncated = total > PATTERN_TABLE_LIMIT;
    rows.truncate(PATTERN_TABLE_LIMIT);
    PatternTablePanel { rows, total, truncated }
}

// ================================================================== ハンドラ

#[derive(Debug, Default, Deserialize)]
pub struct P11Query {}

#[derive(Debug, Serialize)]
pub struct P11Kpis {
    /// 「コンサル勝ち筋分析」行数
    pub winning_rows: usize,
    /// 「コンサル行動量_担当日次」行数
    pub owner_daily_rows: usize,
    /// 「コンサル行動量_日次」行数(KPI件数表示のみ。他パネルには使わない。GAS版と同じ)
    pub daily_rows: usize,
    /// 集計可能な担当者数
    pub target_owners: usize,
}

#[derive(Debug, Serialize)]
pub struct OwnerRankingPanel {
    /// avg_score 降順、上位15名
    pub rows: Vec<OwnerAgg>,
    /// 絞り込み前の担当者総数
    pub total: usize,
    /// `OWNER_RANKING_LIMIT` で切ったか(約束3: 黙って上位N件にしない)
    pub truncated: bool,
}

#[derive(Debug, Serialize)]
pub struct P11Data {
    pub kpis: P11Kpis,
    pub top_bottom: Option<TopBottomPanel>,
    pub owner_ranking: OwnerRankingPanel,
    pub stage_outcome: StageOutcomePanel,
    pub pattern_table: PatternTablePanel,
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
    _q: P11Query,
) -> Result<TabPayload<P11Data>> {
    let started = Instant::now();
    let mut sources: Vec<SourceInfo> = Vec::new();

    let winning = load(client, store, SHEET_WINNING, &mut sources).await?;
    let owner_daily = load(client, store, SHEET_OWNER_DAILY, &mut sources).await?;
    let daily = load(client, store, SHEET_DAILY, &mut sources).await?;

    // GAS `var source = rows.length ? rows : ownerRows;`
    // 上位/下位比較と Owner ランキングはこの `source` を使う。
    // Stage/Outcome と明細テーブルは常に winning(勝ち筋) のみを使う。
    let source: &SheetData = if !winning.rows.is_empty() { &winning } else { &owner_daily };

    let agg = owner_agg(source);
    let target_owners = agg.len();
    let top_bottom = if agg.is_empty() { None } else { Some(build_top_bottom(&agg)) };
    let owner_ranking = OwnerRankingPanel {
        total: agg.len(),
        truncated: agg.len() > OWNER_RANKING_LIMIT,
        rows: agg.into_iter().take(OWNER_RANKING_LIMIT).collect(),
    };
    let stage_outcome = build_stage_outcome(&winning);
    let pattern_table = build_pattern_table(&winning);

    let kpis = P11Kpis {
        winning_rows: winning.rows.len(),
        owner_daily_rows: owner_daily.rows.len(),
        daily_rows: daily.rows.len(),
        target_owners,
    };

    Ok(TabPayload {
        data: P11Data {
            kpis,
            top_bottom,
            owner_ranking,
            stage_outcome,
            pattern_table,
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

    fn winning_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        let header = vec![
            "owner_id", "owner_name", "year_month", "stage_label", "outcome_status",
            "active_deal_count", "touched_days", "email_count", "call_count", "mtg_count",
            "other_count", "total_count", "mtg_ratio",
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
    fn 継続確定は100点解約は0点それ以外は接触量() {
        let d = winning_sheet(vec![
            vec!["1", "田中", "2026-06", "定期1", "継続確定", "1", "5", "2", "3", "0", "0", "5", "0.0"],
            vec!["2", "田中", "2026-06", "定期2", "解約済", "1", "5", "2", "3", "0", "0", "5", "0.0"],
            vec!["3", "田中", "2026-06", "定期3", "進行中", "1", "5", "2", "3", "0", "0", "5", "0.0"],
        ]);
        assert_eq!(p11_score(&d, &d.rows[0]), 100.0);
        assert_eq!(p11_score(&d, &d.rows[1]), 0.0);
        assert_eq!(p11_score(&d, &d.rows[2]), 5.0, "結果未確定は接触量(total_count)をスコアにする");
    }

    #[test]
    fn total_countが0のときは内訳の合計にフォールバックする() {
        // GAS `_p11Contact` は email+call+mtg+mtg_total_count の合計であり、
        // other_count は含めない(GAS 版の実装をそのまま踏襲。ファイル冒頭 注記参照)。
        let d = winning_sheet(vec![vec!["1", "田中", "2026-06", "s", "進行中", "1", "1", "2", "3", "0", "1", "0", "0.0"]]);
        assert_eq!(p11_contact(&d, &d.rows[0]), 5.0, "email2+call3+mtg0+mtg_total_count0(列無し)=5");
    }

    #[test]
    fn owner集計は表示名でグルーピングし平均スコアで降順に並ぶ() {
        let d = winning_sheet(vec![
            vec!["1", "田中", "2026-06", "s", "継続確定", "1", "1", "0", "0", "0", "0", "10", "0.0"],
            vec!["1", "田中", "2026-07", "s", "解約済", "1", "1", "0", "0", "0", "0", "10", "0.0"],
            vec!["2", "鈴木", "2026-06", "s", "継続確定", "1", "1", "0", "0", "0", "0", "10", "0.0"],
        ]);
        let agg = owner_agg(&d);
        assert_eq!(agg.len(), 2);
        assert_eq!(agg[0].owner_label, "鈴木", "平均100点の鈴木が先(田中は(100+0)/2=50)");
        assert_eq!(agg[0].rows, 1);
        let tanaka = agg.iter().find(|a| a.owner_label == "田中").unwrap();
        assert_eq!(tanaka.avg_score, 50.0);
    }

    #[test]
    fn 上位下位グループは平均で比較する() {
        let agg = vec![
            OwnerAgg { owner_label: "a".into(), rows: 1, avg_score: 90.0, contact_total: 10.0, mtg_total: 0.0 },
            OwnerAgg { owner_label: "b".into(), rows: 1, avg_score: 80.0, contact_total: 10.0, mtg_total: 0.0 },
            OwnerAgg { owner_label: "c".into(), rows: 1, avg_score: 20.0, contact_total: 5.0, mtg_total: 0.0 },
        ];
        let panel = build_top_bottom(&agg);
        assert_eq!(panel.top.n, 3, "3名しかいないので上位グループも3名(min(5,len))");
        assert_eq!(panel.bottom.n, 3);
    }

    #[test]
    fn stage_outcomeは件数降順で上位12件() {
        let d = winning_sheet(vec![
            vec!["1", "田中", "2026-06", "定期1", "進行中", "1", "1", "0", "0", "0", "0", "1", "0.0"],
            vec!["2", "田中", "2026-06", "定期1", "進行中", "1", "1", "0", "0", "0", "0", "1", "0.0"],
            vec!["3", "田中", "2026-06", "定期2", "提案中", "1", "1", "0", "0", "0", "0", "1", "0.0"],
        ]);
        let panel = build_stage_outcome(&d);
        assert_eq!(panel.top[0].count, 2, "定期1×進行中が2件で最多");
        assert_eq!(panel.total_groups, 2);
    }

    #[test]
    fn 勝ち筋シートには顧客名が無いので常にハイフンになる() {
        let d = winning_sheet(vec![vec!["1", "田中", "2026-06", "s", "進行中", "1", "1", "0", "0", "0", "0", "1", "0.0"]]);
        let panel = build_pattern_table(&d);
        assert_eq!(panel.rows[0].customer_label, "-");
    }

    #[test]
    fn 明細テーブルはスコア降順で200件に切る() {
        let counts: Vec<String> = (0..250).map(|i| i.to_string()).collect();
        let rows: Vec<Vec<&str>> = counts
            .iter()
            .map(|c| vec!["1", "田中", "2026-06", "s", "進行中", "1", "1", "0", "0", "0", "0", c.as_str(), "0.0"])
            .collect();
        let d = winning_sheet(rows);
        let panel = build_pattern_table(&d);
        assert_eq!(panel.rows.len(), 200);
        assert!(panel.truncated);
        assert!(panel.rows[0].score >= panel.rows[1].score);
    }

    #[test]
    fn 大文字小文字を区別するoutcome判定() {
        // GAS 正規表現に i フラグが無いため "WIN"(大文字)は勝ちと判定されない
        let d = winning_sheet(vec![vec!["1", "田中", "2026-06", "s", "WIN", "1", "1", "0", "0", "0", "0", "3", "0.0"]]);
        assert_eq!(p11_score(&d, &d.rows[0]), 3.0, "大文字WINはキーワード不一致→接触量にフォールバック");
    }
}
