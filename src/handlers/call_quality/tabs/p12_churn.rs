//! P12: 解約分析
//!
//! GAS 版の移植元:
//!   - 画面: `scripts/gas/call_quality_app/index.html` `#page-p12` (~1562-1684行)
//!   - 描画: `scripts/gas/call_quality_app/javascript.html` `_drawP12A/B/C/D/Metrics`
//!            (~10574-11212行)
//!   - 取得: `scripts/gas/call_quality_app/Code.gs`
//!            `getChurnPattern/ByConsultant/BySegment/Prediction/Metrics` (~1547-1608行)
//!
//! 読むシート (5枚。全て `sheets::KNOWN_SHEETS` に登録済み):
//!   - 解約_理由パターン       (A: 6 stage 行動量比較)
//!   - 解約_コンサル担当別     (C: consultant ランキング)
//!   - 解約_業界規模マトリクス (D: JSIC × size_band × prefecture)
//!   - 解約_active予測         (B: LightGBM 3クラス、稼働中Deal)
//!   - 解約_モデル指標         (B付録: AUC/Precision/特徴量重要度)
//!
//! **LightGBM モデルの再学習・再予測はしない**。Python 側
//! (`scripts/call_quality_monitor/churn_prediction_model.py` 他) が
//! 算出済みの確率・指標値をシートから読んで整形するだけ。
//!
//! 未実装パネル: なし。5シートとも `sheets::KNOWN_SHEETS` 経由で取得可能なため
//! GAS 版の A/B/B付録/C/D を全てそのまま移植した。
//!
//! D (業界×規模マトリクス) の設計だけ GAS 版と処理場所が違う:
//!   GAS 版は「業界×規模×都道府県」の生の3次元集計行(~70-100行)を丸ごとブラウザへ送り、
//!   軸セレクタ切替のたびにブラウザ側で3軸目を合算して比率を再計算していた。
//!   本実装は**3つの軸の組み合わせ(業界×規模 / 業界×都道府県 / 規模×都道府県)を
//!   サーバ側で先に集計**し、3パターンぶんの小さな行列(合計でも数百セル)を返す。
//!   セル内の最低件数フィルタ(n≥5 等)は表示側の閾値選択なので集計をやり直す必要が
//!   なく、そのままクライアント側の閾値切替に委ねる(計算式自体は GAS 版と同一)。

use std::collections::HashMap;

use anyhow::Result;
use serde::Serialize;

use crate::db::sheets_client::SheetsClient;
use crate::handlers::call_quality::sheets::{SheetData, SheetStore};

use super::{SourceInfo, TabPayload};

// ---------------------------------------------------------------- 数値パース
// シート値は全て文字列で来る。空文字は「値なし」であって 0 ではないので、
// Option を返す版と、集計都合上どうしても既定値が要る版(件数など)を分ける。

fn pf(s: &str) -> f64 {
    s.trim().replace(',', "").parse::<f64>().unwrap_or(0.0)
}

fn pf_opt(s: &str) -> Option<f64> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.replace(',', "").parse::<f64>().ok()
    }
}

fn pu32(s: &str) -> u32 {
    s.trim().parse::<f64>().map(|v| v as u32).unwrap_or(0)
}

fn pu32_opt(s: &str) -> Option<u32> {
    let t = s.trim();
    if t.is_empty() {
        None
    } else {
        t.parse::<f64>().ok().map(|v| v as u32)
    }
}

// ============================================================== A: 解約理由パターン

/// 「解約_理由パターン」1行。GAS 側は加工せずそのままテーブル/チャート化するので、
/// ここでも列を読み替える以上のことはしない。
#[derive(Debug, Clone, Serialize)]
pub struct ChurnPatternRow {
    pub stage_id: String,
    pub stage_label: String,
    pub deals_count: u32,
    pub avg_total_contact: f64,
    pub avg_call: f64,
    pub avg_email: f64,
    pub avg_mtg: f64,
    /// 空欄あり(MTG間隔が計算不能な stage が存在する実データ)
    pub avg_mtg_interval_days: Option<f64>,
    pub avg_customer_lifetime_days: f64,
    /// 空欄あり(NPS 未取得の stage)
    pub avg_nps: Option<f64>,
    pub avg_continue_intent: Option<f64>,
}

fn parse_pattern_rows(data: &SheetData) -> Vec<ChurnPatternRow> {
    data.rows
        .iter()
        .map(|row| ChurnPatternRow {
            stage_id: data.get(row, "stage_id").to_string(),
            stage_label: data.get(row, "stage_label").to_string(),
            deals_count: pu32(data.get(row, "deals_count")),
            avg_total_contact: pf(data.get(row, "avg_total_contact")),
            avg_call: pf(data.get(row, "avg_call")),
            avg_email: pf(data.get(row, "avg_email")),
            avg_mtg: pf(data.get(row, "avg_mtg")),
            avg_mtg_interval_days: pf_opt(data.get(row, "avg_mtg_interval_days")),
            avg_customer_lifetime_days: pf(data.get(row, "avg_customer_lifetime_days")),
            avg_nps: pf_opt(data.get(row, "avg_nps")),
            avg_continue_intent: pf_opt(data.get(row, "avg_continue_intent")),
        })
        .collect()
}

// ============================================================== B: 解約リスク予測 Top20

/// 「解約_active予測」1行のうち画面表示に使う列だけ。
#[derive(Debug, Clone, Serialize)]
pub struct ChurnPredictionRow {
    pub deal_id: String,
    pub deal_name: String,
    pub customer_id: String,
    pub consultant_name: String,
    pub stage_label: String,
    pub industry_jsic: String,
    pub size_band: String,
    pub deal_age_months: f64,
    pub days_since_last_activity: f64,
    /// 表示する「失敗解約proba」。GAS 側と同じく
    /// recommended_risk_score(strict, AUC約0.72) を優先し、無ければ bad_churn_proba にフォールバック
    /// (javascript.html 10724行)。
    pub bad_proba: f64,
    /// 参考値。good churn は recall≈0.05 で実質予測不能(色分けもしない、GAS 側踏襲)。
    pub good_proba: f64,
    pub continue_proba: f64,
    pub predicted_class: String,
    pub intervention_priority: String,
    pub nps_alert_msg: String,
    /// top_factors 優先、無ければ旧列 top_features にフォールバック(javascript.html 10764行)。
    pub top_factors: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct InterventionCounts {
    /// 失敗解約proba >= 0.7 → 即介入で救出
    pub critical_rescue: u32,
    /// 失敗解約proba 0.4-0.7 → 監視
    pub watch_bad: u32,
    /// 充足解約proba >= 0.7 → 他求人掘り起こし(現データでは常時0件想定)
    pub dig_demand: u32,
    /// 継続proba >= 0.7
    pub stable: u32,
    /// 上記いずれにも該当しない(閾値の境界)
    pub mixed: u32,
}

#[derive(Debug, Clone, Serialize)]
pub struct ChurnPredictionSection {
    /// bad_proba 降順で上位20件(GAS 版のステータス文言「recommended_risk_score=strict 降順」と同じ基準で明示的に再ソートする)
    pub top20: Vec<ChurnPredictionRow>,
    pub total_active_deals: usize,
    pub counts: InterventionCounts,
    /// 稼働中Deal数が20件を超えていて上位20件に切り詰めた場合 true(黙って切らない)
    pub truncated: bool,
    pub auc_bad: Option<f64>,
    pub auc_good: Option<f64>,
    pub accuracy_overall: Option<f64>,
    pub model_type: String,
    pub n_train: u32,
}

fn resolve_bad_proba(data: &SheetData, row: &[std::sync::Arc<str>]) -> f64 {
    pf_opt(data.get(row, "recommended_risk_score")).unwrap_or_else(|| pf(data.get(row, "bad_churn_proba")))
}

fn build_prediction_section(pred: &SheetData, metrics: &SheetData) -> ChurnPredictionSection {
    let mut counts = InterventionCounts::default();
    let mut rows: Vec<(f64, ChurnPredictionRow)> = Vec::with_capacity(pred.rows.len());

    for row in &pred.rows {
        let prio = pred.get(row, "intervention_priority").to_string();
        match prio.as_str() {
            "CRITICAL_RESCUE" => counts.critical_rescue += 1,
            "WATCH_BAD" => counts.watch_bad += 1,
            "DIG_DEMAND" => counts.dig_demand += 1,
            "STABLE" => counts.stable += 1,
            _ => counts.mixed += 1,
        }

        let bad_proba = resolve_bad_proba(pred, row);
        let consultant_name = {
            let n = pred.get(row, "consultant_name");
            if n.trim().is_empty() {
                pred.get(row, "consultant_id").to_string()
            } else {
                n.to_string()
            }
        };
        let top_factors = {
            let t = pred.get(row, "top_factors");
            if t.trim().is_empty() {
                pred.get(row, "top_features").to_string()
            } else {
                t.to_string()
            }
        };

        rows.push((
            bad_proba,
            ChurnPredictionRow {
                deal_id: pred.get(row, "deal_id").to_string(),
                deal_name: pred.get(row, "deal_name").to_string(),
                customer_id: pred.get(row, "customer_id").to_string(),
                consultant_name,
                stage_label: pred.get(row, "stage_label").to_string(),
                industry_jsic: pred.get(row, "industry_jsic").to_string(),
                size_band: pred.get(row, "size_band").to_string(),
                deal_age_months: pf(pred.get(row, "deal_age_months")),
                days_since_last_activity: pf(pred.get(row, "days_since_last_activity")),
                bad_proba,
                good_proba: pf(pred.get(row, "good_churn_proba")),
                continue_proba: pf(pred.get(row, "continue_proba")),
                predicted_class: pred.get(row, "predicted_class").to_string(),
                intervention_priority: prio,
                nps_alert_msg: pred.get(row, "nps_alert_msg").to_string(),
                top_factors,
            },
        ));
    }

    // bad_proba 降順(タイブレークは安定ソートで元の行順を保つ)
    rows.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    let truncated = rows.len() > 20;
    let top20 = rows.into_iter().take(20).map(|(_, r)| r).collect();

    let first_metric = metrics.rows.first();
    let (auc_bad, auc_good, accuracy_overall, model_type, n_train) = match first_metric {
        Some(m) => (
            pf_opt(metrics.get(m, "auc_bad")),
            pf_opt(metrics.get(m, "auc_good")),
            pf_opt(metrics.get(m, "accuracy_overall")),
            metrics.get(m, "model_type").to_string(),
            pu32(metrics.get(m, "n_train")),
        ),
        None => (None, None, None, String::new(), 0),
    };

    ChurnPredictionSection {
        total_active_deals: pred.rows.len(),
        counts,
        truncated,
        top20,
        auc_bad,
        auc_good,
        accuracy_overall,
        model_type,
        n_train,
    }
}

// ============================================================== C: コンサル担当別ランキング

/// 「解約_コンサル担当別」1行。
#[derive(Debug, Clone, Serialize)]
pub struct ConsultantChurnRow {
    pub consultant_id: String,
    pub consultant_name: String,
    pub total_deals: u32,
    pub active_deals: u32,
    pub churn_deals: u32,
    pub sufficiency_deals: u32,
    pub continue_deals: u32,
    pub market_deals: u32,
    /// 分母 = 担当の保有Deal(納品転帰ベース、active含む)。GAS 版と同一(javascript.html 10614行 desc)
    pub churn_rate: f64,
    pub continue_rate: f64,
    pub sufficiency_rate: f64,
    pub avg_call_per_deal: f64,
    /// total_deals >= 5 のみ付与(GAS 側 Python がランキング対象外は空欄で出力)
    pub rank_churn: Option<u32>,
    pub rank_continue: Option<u32>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ConsultantRankingSection {
    /// 解約率 上位10 (担当Deal>=5のみが対象、降順)
    pub top10: Vec<ConsultantChurnRow>,
    /// 解約率 下位10 (担当Deal>=5のみが対象、昇順=最も低い担当から)
    pub bottom10: Vec<ConsultantChurnRow>,
    /// 全担当(フィルタなし)。担当数(total_deals)降順 — テーブル表示用
    pub all: Vec<ConsultantChurnRow>,
    /// ランキング対象(担当Deal>=5)の人数
    pub eligible_count: usize,
}

fn parse_consultant_rows(data: &SheetData) -> Vec<ConsultantChurnRow> {
    data.rows
        .iter()
        .map(|row| ConsultantChurnRow {
            consultant_id: data.get(row, "consultant_id").to_string(),
            consultant_name: data.get(row, "consultant_name").to_string(),
            total_deals: pu32(data.get(row, "total_deals")),
            active_deals: pu32(data.get(row, "active_deals")),
            churn_deals: pu32(data.get(row, "churn_deals")),
            sufficiency_deals: pu32(data.get(row, "sufficiency_deals")),
            continue_deals: pu32(data.get(row, "continue_deals")),
            market_deals: pu32(data.get(row, "market_deals")),
            churn_rate: pf(data.get(row, "churn_rate")),
            continue_rate: pf(data.get(row, "continue_rate")),
            sufficiency_rate: pf(data.get(row, "sufficiency_rate")),
            avg_call_per_deal: pf(data.get(row, "avg_call_per_deal")),
            rank_churn: pu32_opt(data.get(row, "rank_churn")),
            rank_continue: pu32_opt(data.get(row, "rank_continue")),
        })
        .collect()
}

/// javascript.html `_drawP12C`(10866-10960行)を移植。
/// 担当Deal>=5 のみランキング対象、churn_rate 降順で top10/bottom10 を切り出す。
fn build_consultant_section(rows: Vec<ConsultantChurnRow>) -> ConsultantRankingSection {
    let mut eligible: Vec<ConsultantChurnRow> =
        rows.iter().filter(|r| r.total_deals >= 5).cloned().collect();
    eligible.sort_by(|a, b| b.churn_rate.partial_cmp(&a.churn_rate).unwrap_or(std::cmp::Ordering::Equal));

    let top_n = eligible.len().min(10);
    let top10 = eligible[..top_n].to_vec();
    // GAS: bottom = eligible.slice(-topN).reverse() → 降順配列の末尾topN件を取り、
    // 反転して「最も解約率が低い担当」から並べる(javascript.html 10885行)。
    let bottom10: Vec<ConsultantChurnRow> = eligible[eligible.len() - top_n..]
        .iter()
        .rev()
        .cloned()
        .collect();

    let mut all = rows;
    all.sort_by(|a, b| b.total_deals.cmp(&a.total_deals));

    ConsultantRankingSection {
        top10,
        bottom10,
        all,
        eligible_count: eligible.len(),
    }
}

// ============================================================== D: 業界×規模マトリクス

/// 「解約_業界規模マトリクス」1行(業界×規模×都道府県の3次元 sparse 集計)。
struct SegmentSourceRow {
    industry_jsic: String,
    size_band: String,
    prefecture: String,
    total_deals: u32,
    bad_churn_count: u32,
    good_churn_count: u32,
    continuing_count: u32,
}

fn parse_segment_rows(data: &SheetData) -> Vec<SegmentSourceRow> {
    data.rows
        .iter()
        .map(|row| SegmentSourceRow {
            industry_jsic: data.get(row, "industry_jsic").to_string(),
            size_band: data.get(row, "size_band").to_string(),
            prefecture: data.get(row, "prefecture").to_string(),
            total_deals: pu32(data.get(row, "total_deals")),
            bad_churn_count: pu32(data.get(row, "bad_churn_count")),
            good_churn_count: pu32(data.get(row, "good_churn_count")),
            continuing_count: pu32(data.get(row, "continuing_count")),
        })
        .collect()
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentCell {
    pub row_key: String,
    pub col_key: String,
    pub total: u32,
    pub bad: u32,
    pub good: u32,
    pub continuing: u32,
    /// 失敗3種のみの解約率(%)。GAS 版は 0-1 の小数で保持し表示直前に *100 するが、
    /// 本実装は共通ヘルパ `rate()` に合わせてパーセント(0-100)で返す。
    /// total は常に > 0 のセルしか作らないので None にはならない
    /// (rule2 の「分母0→None」は本行列では発生しない状況だが、規約に合わせ Option で統一する)。
    pub bad_rate: Option<f64>,
    pub good_rate: Option<f64>,
    /// 失敗3種+充足 の合算解約率(%)
    pub total_rate: Option<f64>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SegmentMatrix {
    /// "industry_size" | "industry_pref" | "size_pref"
    pub axis: &'static str,
    pub row_label: &'static str,
    pub col_label: &'static str,
    pub row_keys: Vec<String>,
    pub col_keys: Vec<String>,
    pub cells: Vec<SegmentCell>,
}

/// javascript.html `_redrawP12D`(10997-11130行)を移植。
/// 3軸目を合算してから比率を再計算する(合算前の行ごとの率を平均してはいけない)。
fn aggregate_segment(
    rows: &[SegmentSourceRow],
    axis: &'static str,
    row_label: &'static str,
    col_label: &'static str,
    row_of: impl Fn(&SegmentSourceRow) -> &str,
    col_of: impl Fn(&SegmentSourceRow) -> &str,
    sort_col_by_total_desc: bool,
) -> SegmentMatrix {
    struct Agg {
        total: u32,
        bad: u32,
        good: u32,
        cont: u32,
    }
    let mut agg: HashMap<(String, String), Agg> = HashMap::new();
    for r in rows {
        let k = (row_of(r).to_string(), col_of(r).to_string());
        let e = agg.entry(k).or_insert(Agg { total: 0, bad: 0, good: 0, cont: 0 });
        e.total += r.total_deals;
        e.bad += r.bad_churn_count;
        e.good += r.good_churn_count;
        e.cont += r.continuing_count;
    }

    let mut row_keys: Vec<String> = agg.keys().map(|(r, _)| r.clone()).collect();
    row_keys.sort();
    row_keys.dedup();
    let mut col_keys: Vec<String> = agg.keys().map(|(_, c)| c.clone()).collect();
    col_keys.sort();
    col_keys.dedup();

    if sort_col_by_total_desc {
        let mut col_totals: HashMap<String, u32> = HashMap::new();
        for ((_, c), a) in agg.iter() {
            *col_totals.entry(c.clone()).or_insert(0) += a.total;
        }
        col_keys.sort_by(|a, b| col_totals.get(b).unwrap_or(&0).cmp(col_totals.get(a).unwrap_or(&0)));
    }

    // セルは (row_keys × col_keys) を安定した二重ループで並べる(HashMap の反復順に依存しない)
    let mut cells = Vec::new();
    for rk in &row_keys {
        for ck in &col_keys {
            if let Some(a) = agg.get(&(rk.clone(), ck.clone())) {
                if a.total == 0 {
                    continue;
                }
                let t = a.total as f64;
                cells.push(SegmentCell {
                    row_key: rk.clone(),
                    col_key: ck.clone(),
                    total: a.total,
                    bad: a.bad,
                    good: a.good,
                    continuing: a.cont,
                    bad_rate: Some(a.bad as f64 / t * 100.0),
                    good_rate: Some(a.good as f64 / t * 100.0),
                    total_rate: Some((a.bad + a.good) as f64 / t * 100.0),
                });
            }
        }
    }

    SegmentMatrix { axis, row_label, col_label, row_keys, col_keys, cells }
}

fn build_segment_matrices(rows: &[SegmentSourceRow]) -> Vec<SegmentMatrix> {
    vec![
        aggregate_segment(
            rows,
            "industry_size",
            "業界 (JSIC)",
            "規模",
            |r| &r.industry_jsic,
            |r| &r.size_band,
            false,
        ),
        aggregate_segment(
            rows,
            "industry_pref",
            "業界 (JSIC)",
            "都道府県",
            |r| &r.industry_jsic,
            |r| &r.prefecture,
            true,
        ),
        aggregate_segment(
            rows,
            "size_pref",
            "規模",
            "都道府県",
            |r| &r.size_band,
            |r| &r.prefecture,
            true,
        ),
    ]
}

// ============================================================== B付録: モデル指標

#[derive(Debug, Clone, Serialize)]
pub struct ModelMetricEntry {
    pub key: String,
    pub value: Option<String>,
    pub note: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureImportance {
    pub feature: String,
    pub importance: f64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ModelMetricsSection {
    pub entries: Vec<ModelMetricEntry>,
    pub top_features: Vec<FeatureImportance>,
}

/// javascript.html `_drawP12Metrics`(11133-11212行)の displayOrder / notes を移植。
const METRIC_DISPLAY_ORDER: &[(&str, &str)] = &[
    ("trained_at", "学習実行日時"),
    ("model_type", "モデル種別 (LightGBM or fallback)"),
    ("n_train", "学習サンプル合計"),
    ("n_test", "テストサンプル数"),
    ("n_features", "特徴量数 (現行の次元数。値列を参照)"),
    ("n_train_continue", "継続済 学習件数 (label=0)"),
    ("n_train_bad", "Bad churn(strict)学習件数 (成果不足+架電禁止。会社方針=外生離脱は別クラス)"),
    ("n_train_good", "Good churn 学習件数 (充足)"),
    ("auc_bad", "Bad churn OvR AUC (1-vs-rest)"),
    ("auc_good", "Good churn OvR AUC (1-vs-rest)"),
    ("auc_continue", "継続 OvR AUC (1-vs-rest)"),
    ("auc_macro", "4クラス(継続/失敗(strict)/充足/会社方針) AUC macro 平均"),
    ("accuracy_overall", "4クラス全体 accuracy"),
    ("precision_bad", "Bad churn precision"),
    ("recall_bad", "Bad churn recall"),
    ("f1_bad", "Bad churn F1"),
    ("precision_good", "Good churn precision"),
    ("recall_good", "Good churn recall"),
    ("f1_good", "Good churn F1"),
    ("auc", "既存互換 AUC (bad+good vs continue)"),
    ("precision", "既存互換 precision (= precision_bad)"),
    ("recall", "既存互換 recall (= recall_bad)"),
    ("f1", "既存互換 F1 (= f1_bad)"),
    ("positive_rate_train", "学習データ中の解約 (bad+good) 比率"),
    ("train_auc_bad", "学習データ AUC (Bad/strict)"),
    ("overfit_gap_bad", "過学習ギャップ = train_auc_bad − auc_bad (大きいほど過学習)"),
    (
        "eval_method",
        "評価方式 (StratifiedKFold OOF。同一顧客が train/test 両側に入りうるため GroupKFold(顧客) 比で約+0.01 楽観の可能性)",
    ),
    ("cv_n_splits", "CV 分割数"),
];

fn build_metrics_section(data: &SheetData) -> ModelMetricsSection {
    let Some(row) = data.rows.first() else {
        return ModelMetricsSection { entries: Vec::new(), top_features: Vec::new() };
    };

    let entries = METRIC_DISPLAY_ORDER
        .iter()
        .map(|(key, note)| {
            let v = data.get(row, key);
            ModelMetricEntry {
                key: key.to_string(),
                value: if v.trim().is_empty() { None } else { Some(v.to_string()) },
                note: note.to_string(),
            }
        })
        .collect();

    // top10_features_importance = JSON 文字列 `[["feature", 123.4], ...]`
    let raw = data.get(row, "top10_features_importance");
    let top_features = serde_json::from_str::<Vec<(String, f64)>>(raw)
        .unwrap_or_default()
        .into_iter()
        .map(|(feature, importance)| FeatureImportance { feature, importance })
        .collect();

    ModelMetricsSection { entries, top_features }
}

// ============================================================== 統合ハンドラ

#[derive(Debug, Clone, Serialize)]
pub struct ChurnAnalysisData {
    pub pattern: Vec<ChurnPatternRow>,
    pub prediction: ChurnPredictionSection,
    pub consultants: ConsultantRankingSection,
    /// [業界×規模, 業界×都道府県, 規模×都道府県] の3パターン
    pub segment_matrices: Vec<SegmentMatrix>,
    pub metrics: ModelMetricsSection,
}

pub async fn get_churn_analysis(
    client: &SheetsClient,
    store: &SheetStore,
) -> Result<TabPayload<ChurnAnalysisData>> {
    let started = std::time::Instant::now();

    let (pattern_data, pattern_cached) = store.get(client, "解約_理由パターン").await?;
    let (consultant_data, consultant_cached) = store.get(client, "解約_コンサル担当別").await?;
    let (segment_data, segment_cached) = store.get(client, "解約_業界規模マトリクス").await?;
    let (prediction_data, prediction_cached) = store.get(client, "解約_active予測").await?;
    let (metrics_data, metrics_cached) = store.get(client, "解約_モデル指標").await?;

    let pattern = parse_pattern_rows(&pattern_data);
    let consultants = build_consultant_section(parse_consultant_rows(&consultant_data));
    let segment_matrices = build_segment_matrices(&parse_segment_rows(&segment_data));
    let prediction = build_prediction_section(&prediction_data, &metrics_data);
    let metrics = build_metrics_section(&metrics_data);

    let src = |sheet: &str, d: &SheetData, cached: bool| SourceInfo {
        sheet: sheet.to_string(),
        total_rows: d.rows.len(),
        matched_rows: d.rows.len(), // 本タブは全期間・全データ対象で絞り込み無し(index.html 1569行)
        from_cache: cached,
        age_secs: d.fetched_at.elapsed().as_secs(),
    };

    Ok(TabPayload {
        data: ChurnAnalysisData { pattern, prediction, consultants, segment_matrices, metrics },
        sources: vec![
            src("解約_理由パターン", &pattern_data, pattern_cached),
            src("解約_コンサル担当別", &consultant_data, consultant_cached),
            src("解約_業界規模マトリクス", &segment_data, segment_cached),
            src("解約_active予測", &prediction_data, prediction_cached),
            src("解約_モデル指標", &metrics_data, metrics_cached),
        ],
        elapsed_ms: started.elapsed().as_millis(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::time::Instant;

    fn sheet(header: &[&str], rows: Vec<Vec<&str>>) -> SheetData {
        SheetData {
            header: header.iter().map(|s| s.to_string()).collect(),
            rows: rows
                .into_iter()
                .map(|r| r.into_iter().map(Arc::from).collect())
                .collect(),
            fetched_at: Instant::now(),
        }
    }

    // ---- A: パターン ----

    #[test]
    fn パターン行の空欄はnoneになる() {
        let d = sheet(
            &["stage_id", "stage_label", "deals_count", "avg_total_contact", "avg_call", "avg_email",
              "avg_mtg", "avg_mtg_interval_days", "avg_customer_lifetime_days", "avg_nps", "avg_continue_intent"],
            vec![vec!["52016159", "解約済(成果不足)", "614", "28.93", "28.93", "0.0", "0.0", "", "328.65", "3.88", "1.4"]],
        );
        let rows = parse_pattern_rows(&d);
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].avg_mtg_interval_days, None, "実データにMTG間隔空欄のstageが存在する");
        assert_eq!(rows[0].deals_count, 614);
    }

    // ---- B: 予測 ----

    fn pred_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        sheet(
            &["deal_id", "deal_name", "customer_id", "consultant_id", "consultant_name", "stage_label",
              "industry_jsic", "size_band", "deal_age_months", "days_since_last_activity",
              "recommended_risk_score", "bad_churn_proba", "good_churn_proba", "continue_proba",
              "predicted_class", "intervention_priority", "nps_alert_msg", "top_factors", "top_features"],
            rows,
        )
    }

    #[test]
    fn 予測はbad_probaの降順で並ぶ() {
        let pred = pred_sheet(vec![
            vec!["1", "A社", "c1", "u1", "山田", "定期1", "H", "06", "10", "5", "0.2", "0.2", "0.01", "0.7", "継続", "STABLE", "", "", ""],
            vec!["2", "B社", "c2", "u1", "山田", "定期1", "H", "06", "10", "5", "0.9", "0.9", "0.01", "0.05", "失敗解約", "CRITICAL_RESCUE", "", "", ""],
        ]);
        let metrics = sheet(&["auc_bad"], vec![]);
        let section = build_prediction_section(&pred, &metrics);
        assert_eq!(section.top20[0].deal_id, "2", "recommended_risk_score最大の行が先頭");
        assert_eq!(section.counts.critical_rescue, 1);
        assert_eq!(section.counts.stable, 1);
        assert!(!section.truncated);
    }

    #[test]
    fn recommended_risk_score空欄はbad_churn_probaにフォールバックする() {
        let pred = pred_sheet(vec![
            vec!["1", "A社", "c1", "u1", "山田", "定期1", "H", "06", "10", "5", "", "0.55", "0.01", "0.4", "失敗解約", "WATCH_BAD", "", "", ""],
        ]);
        let metrics = sheet(&["auc_bad"], vec![]);
        let section = build_prediction_section(&pred, &metrics);
        assert_eq!(section.top20[0].bad_proba, 0.55);
    }

    #[test]
    fn 件数が21件以上ならtruncatedが立つ() {
        let mut rows = Vec::new();
        for i in 0..25 {
            rows.push(vec![
                Box::leak(i.to_string().into_boxed_str()) as &str,
                "A社", "c1", "u1", "山田", "定期1", "H", "06", "10", "5",
                "0.5", "0.5", "0.01", "0.4", "失敗解約", "WATCH_BAD", "", "", "",
            ]);
        }
        let pred = pred_sheet(rows);
        let metrics = sheet(&["auc_bad"], vec![]);
        let section = build_prediction_section(&pred, &metrics);
        assert_eq!(section.top20.len(), 20, "表示は上位20件");
        assert_eq!(section.total_active_deals, 25, "対象数は全件");
        assert!(section.truncated, "20件を超えたら黙って切らずtruncatedを立てる");
    }

    // ---- C: コンサル担当別 ----

    fn consultant_row(id: &str, name: &str, total: &str, rate: &str) -> Vec<&'static str> {
        vec![
            Box::leak(id.to_string().into_boxed_str()),
            Box::leak(name.to_string().into_boxed_str()),
            Box::leak(total.to_string().into_boxed_str()),
            "0", "0", "0", "0", "0",
            Box::leak(rate.to_string().into_boxed_str()),
            "0", "0", "0", "", "",
        ]
    }

    fn consultant_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        sheet(
            &["consultant_id", "consultant_name", "total_deals", "active_deals", "churn_deals",
              "sufficiency_deals", "continue_deals", "market_deals", "churn_rate", "continue_rate",
              "sufficiency_rate", "avg_call_per_deal", "rank_churn", "rank_continue"],
            rows,
        )
    }

    #[test]
    fn 担当5件未満はランキング対象外() {
        let d = consultant_sheet(vec![
            consultant_row("1", "A", "10", "0.5"),
            consultant_row("2", "B", "4", "0.9"), // 5件未満なので top10/bottom10 から除外
        ]);
        let section = build_consultant_section(parse_consultant_rows(&d));
        assert_eq!(section.eligible_count, 1);
        assert_eq!(section.all.len(), 2, "全担当テーブルにはフィルタなしで両方載る");
    }

    #[test]
    fn top10とbottom10は解約率で正しい向きに並ぶ() {
        let d = consultant_sheet(vec![
            consultant_row("1", "高解約", "10", "0.8"),
            consultant_row("2", "中解約", "10", "0.5"),
            consultant_row("3", "低解約", "10", "0.1"),
        ]);
        let section = build_consultant_section(parse_consultant_rows(&d));
        assert_eq!(section.top10[0].consultant_name, "高解約", "上位=解約率が高い順");
        assert_eq!(section.bottom10[0].consultant_name, "低解約", "下位=解約率が低い順(昇順)");
    }

    // ---- D: 業界×規模マトリクス ----

    fn segment_row(ind: &str, size: &str, pref: &str, total: &str, bad: &str, good: &str, cont: &str) -> Vec<&'static str> {
        vec![
            Box::leak(ind.to_string().into_boxed_str()),
            Box::leak(size.to_string().into_boxed_str()),
            Box::leak(pref.to_string().into_boxed_str()),
            Box::leak(total.to_string().into_boxed_str()),
            Box::leak(cont.to_string().into_boxed_str()),
            Box::leak(bad.to_string().into_boxed_str()),
            Box::leak(good.to_string().into_boxed_str()),
            "0", "0", "0",
        ]
    }

    fn segment_sheet(rows: Vec<Vec<&str>>) -> SheetData {
        sheet(
            &["industry_jsic", "size_band", "prefecture", "total_deals", "continuing_count",
              "bad_churn_count", "good_churn_count", "bad_churn_rate", "good_churn_rate", "total_churn_rate"],
            rows,
        )
    }

    #[test]
    fn 三軸目を合算してから率を再計算する() {
        // 同じ (業界, 規模) で都道府県違いの2行 → 合算後の母数で再計算されるべき
        // (行ごとの率を単純平均してはいけない)
        let d = segment_sheet(vec![
            segment_row("H運輸業", "06", "東京都", "10", "5", "0", "5"),
            segment_row("H運輸業", "06", "大阪府", "10", "0", "0", "10"),
        ]);
        let rows = parse_segment_rows(&d);
        let matrices = build_segment_matrices(&rows);
        let m = matrices.iter().find(|m| m.axis == "industry_size").unwrap();
        assert_eq!(m.cells.len(), 1, "業界×規模では都道府県が畳まれて1セル");
        let cell = &m.cells[0];
        assert_eq!(cell.total, 20);
        assert_eq!(cell.bad, 5);
        assert_eq!(cell.bad_rate, Some(25.0), "5/20*100。単純平均(50%)ではない");
    }

    #[test]
    fn 行と列のキーは安定してソートされる() {
        let d = segment_sheet(vec![
            segment_row("Z業", "01", "東京都", "5", "0", "0", "5"),
            segment_row("A業", "02", "東京都", "5", "0", "0", "5"),
        ]);
        let rows = parse_segment_rows(&d);
        let matrices = build_segment_matrices(&rows);
        let m = matrices.iter().find(|m| m.axis == "industry_size").unwrap();
        assert_eq!(m.row_keys, vec!["A業".to_string(), "Z業".to_string()], "業界名の昇順で安定");
    }

    #[test]
    fn 都道府県軸は件数降順に並ぶ() {
        let d = segment_sheet(vec![
            segment_row("A業", "01", "少数県", "3", "0", "0", "3"),
            segment_row("A業", "01", "多数県", "50", "0", "0", "50"),
        ]);
        let rows = parse_segment_rows(&d);
        let matrices = build_segment_matrices(&rows);
        let m = matrices.iter().find(|m| m.axis == "industry_pref").unwrap();
        assert_eq!(m.col_keys[0], "多数県", "都道府県は合計件数の降順(javascript.html 11061行と同じ)");
    }

    // ---- B付録: モデル指標 ----

    #[test]
    fn 特徴量重要度をパースできる() {
        let d = sheet(
            &["auc_bad", "top10_features_importance"],
            vec![vec!["0.72", r#"[["calls_per_month", 2287.5], ["naite", 1762.2]]"#]],
        );
        let m = build_metrics_section(&d);
        assert_eq!(m.top_features.len(), 2);
        assert_eq!(m.top_features[0].feature, "calls_per_month");
    }

    #[test]
    fn モデル指標行が無ければ空を返す() {
        let d = sheet(&["auc_bad"], vec![]);
        let m = build_metrics_section(&d);
        assert!(m.entries.is_empty());
        assert!(m.top_features.is_empty());
    }
}
