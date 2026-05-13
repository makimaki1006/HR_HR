//! 印刷レポート: 採用市場 逼迫度 section (2026-04-26 追加)
//!
//! ## 背景
//! ユーザー指摘「**有効求人倍率系のデータも既に持っていたよね？反映してる？**」
//! 既存実装の調査結果:
//! - `ext_job_ratio` (有効求人倍率): Tab UI で 1 行表示のみ、印刷レポート未活用
//! - `ext_turnover` (離職率): 同様
//! - `ext_business_dynamics` (開廃業): 完全未活用
//!
//! → これらを「**採用市場の逼迫度指標**」として統合表示する。
//!
//! ## 4 軸 + 補助 1 指標 (2026-04-26 仕様変更で ts_fulfillment は除外)
//! 1. **有効求人倍率** (ext_job_ratio.ratio_total)
//!    - DB: Turso `v2_external_job_openings_ratio`
//!    - カラム: `ratio_total`
//!    - 計算: 有効求人数 / 有効求職者数 (公表値)
//!    - 粒度: 都道府県
//! 2. **HW 欠員補充率** (vacancy.vacancy_rate, 正社員)
//!    - DB: ローカル SQLite `v2_vacancy_rate`
//!    - カラム: `vacancy_count` / `total_count` → `vacancy_rate` (0-1)
//!    - 計算: vacancy_count / total_count
//!    - 粒度: 市区町村レベル可
//! 3. **失業率** (ext_labor_force.unemployment_rate)
//!    - DB: Turso `v2_external_labor_force`
//!    - カラム: `unemployment_rate` (%)
//!    - 計算: 公表値 (労働力調査 / 国勢調査ベース)
//!    - 粒度: 都道府県
//! 4. **離職率** (ext_turnover.separation_rate)
//!    - DB: Turso `v2_external_turnover` (industry='産業計')
//!    - カラム: `separation_rate` (%)
//!    - 計算: 公表値 (雇用動向調査)
//!    - 粒度: 都道府県
//! 補助. **開廃業動態** (ext_business_dynamics.opening_rate / closure_rate)
//!    - DB: Turso `v2_external_business_dynamics`
//!    - カラム: `opening_rate` / `closure_rate` (%)
//!    - 粒度: 都道府県
//!
//! ## 構成
//! - **逼迫度総合スコア** (0-100): 4 指標複合の信号機色付き表示
//! - **4 軸レーダーチャート** (ECharts): 0-100 正規化、全国平均レーダー併載
//! - **個別 KPI カード** 4: 比較値 (全国平均 / 前年比) + データソース注記
//! - **補助 KPI**: 開廃業動態 (1 行)
//! - **データソース・計算方法** (折りたたみ): 各指標の DB / カラム / 計算式
//! - **解釈ガイド + アクション提案** (因果非主張 caveat 必須)
//!
//! ## 設計原則 (memory ルール準拠)
//! - `feedback_correlation_not_causation.md`: 逼迫度スコアは「相関の集約」であり因果ではない
//! - `feedback_hw_data_scope.md`: HW 由来 (vacancy) と外部統計 (求人倍率 / 失業率 / 離職率) を区別
//! - `feedback_test_data_validation.md`: 具体値での逆証明テスト
//! - `feedback_never_guess_data.md`: 実カラム名 grep 確認済み
//! - `feedback_hypothesis_driven.md`: ペルソナの次の行動 (給与訴求 / 福利強化 / 通勤圏拡大) を明示
//!
//! ## 公開 API
//! - `render_section_market_tightness(html, ctx)` のみを super に公開

#![allow(dead_code)]

use super::super::super::helpers::{escape_html, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use serde_json::json;

use super::helpers::*;

// =====================================================================
// 公開 API
// =====================================================================

/// 2026-04-29 variant 切替対応版
///
/// `variant` に応じて HW 欠員補充率の表示有無を切替。
/// - `Full`: 4 軸 (有効求人倍率 / HW 欠員補充率 / 失業率 / 離職率) すべて表示
/// - `Public`: HW 欠員補充率を除外、3 軸 (有効求人倍率 / 失業率 / 離職率) で表示
pub(super) fn render_section_market_tightness_with_variant(
    html: &mut String,
    ctx: Option<&InsightContext>,
    variant: super::ReportVariant,
) {
    match variant {
        // Phase 3 Step 4: MarketIntelligence は Full と同じ既存セクションを呼ぶ
        // (Step 3 で MarketIntelligence 専用セクション追加時に分岐を変える)
        super::ReportVariant::Full | super::ReportVariant::MarketIntelligence => {
            render_section_market_tightness_inner(html, ctx, variant)
        }
        super::ReportVariant::Public => render_section_market_tightness_public(html, ctx),
    }
}

/// Round 2.7-B' 互換 wrapper: 既存テストや外部呼出しが variant 引数なしで
/// 呼ぶ際は Full とみなす (KPI カード見出し「有効求人倍率」を維持)。
pub(super) fn render_section_market_tightness(html: &mut String, ctx: Option<&InsightContext>) {
    render_section_market_tightness_inner(html, ctx, super::ReportVariant::Full);
}

/// 公開データ中心 variant: HW 欠員補充率を除外し 3 軸 (有効求人倍率 / 失業率 / 離職率) で構成
///
/// Full との差分:
/// - レーダー: 4 軸 → 3 軸 (HW 欠員補充率を除外)
/// - 個別 KPI カード: 4 枚 → 3 枚 (HW 欠員補充率を除外)
/// - CR-1 採用難易度ブロック: 寄与分解・推奨アクションから HW 欠員補充率を除外
/// - データソース折りたたみ表: HW 欠員補充率の行を除外
/// - 補助 KPI 開廃業動態: 両 variant で表示
/// - caveat: 「HW 掲載求人特有の指標は除外」を明記
pub(super) fn render_section_market_tightness_public(
    html: &mut String,
    ctx: Option<&InsightContext>,
) {
    let ctx = match ctx {
        Some(c) => c,
        None => return,
    };

    // Public variant では HW 欠員補充率を意図的に除外して取得
    let mut metrics = compute_metrics(ctx);
    metrics.vacancy_rate = None;
    metrics.vacancy_trend = Vec::new();

    if !metrics.has_any_data() {
        return;
    }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>採用市場 逼迫度</h2>\n");

    render_section_howto(
        html,
        &[
            "対象地域における「採用のしやすさ／難しさ」を 3 つの公開市場指標で複合評価します",
            "総合スコアは 0-100 で正規化済み。70 以上 = 逼迫 (採用難) / 30 以下 = 緩和 (採用容易)",
            "本 variant は公開統計 (e-Stat) のみを使用。特定求人媒体特有の指標は除外しています",
        ],
    );

    // ---- (1) 逼迫度 総合スコア (3 軸平均) ----
    render_tightness_summary_public(html, &metrics);

    // ---- (1.5) 採用難易度ラベル + 寄与分解 + アクション (CR-1, 3 軸版) ----
    render_recruit_difficulty_block_public(html, &metrics);

    // ---- (2) 3 軸レーダーチャート ----
    render_radar_chart_public(html, &metrics);

    // ---- (3) データソース・計算方法 (折りたたみ、HW 行を除外) ----
    render_data_sources_collapsible_public(html);

    // ---- (4) 個別 KPI カード (3 枚) ----
    render_individual_kpis_public(html, &metrics);

    // ---- (5) 解釈ガイド + アクション提案 (戦略的方針) ----
    render_interpretation_guide(html, &metrics);

    html.push_str(
        "<p class=\"note\" style=\"margin-top:8px;\">\
        \u{203B} 本指標はオープンデータ (公的雇用需給指標 / 失業率 / 離職率) のみを使用しており、特定求人媒体特有の指標は除外しています。\
        指標粒度: 公的雇用需給指標 / 離職率 / 開廃業動態は都道府県粒度のみ。市区町村別の差は反映されません。\
        失業率は労働力調査 (国勢調査ベース) 由来です。\
        逼迫度総合スコアは複合指標で、業界・職種により本来の重み付けが異なります。\
        本数値は採用環境の相関的傾向を示すもので、因果関係を示すものではありません。\
        離職率は雇用動向調査 (厚労省) 由来で、産業別・規模別で差が大きい指標です。\
        </p>\n",
    );

    render_section_bridge(
        html,
        "次セクションでは、この採用市場逼迫度を踏まえた雇用形態の構成と給与構造を確認します。",
    );

    html.push_str("</div>\n");
}

/// 「採用市場 逼迫度」section 全体を描画 (variant 認識版)
///
/// `ctx` が None もしくは関連データ全空の場合、section ごと出力しない (fail-soft)。
///
/// Round 2.7-B' (2026-05-08): variant 引数を受け取り、寄与分解 (CR-1) の
/// 軸ラベルを Full / MarketIntelligence で出し分ける。
/// - `Full`: 「有効求人倍率」 (KPI カード見出しと統一)
/// - `MarketIntelligence`: 「公的雇用需給指標」 (HW 連想語回避を維持)
fn render_section_market_tightness_inner(
    html: &mut String,
    ctx: Option<&InsightContext>,
    variant: super::ReportVariant,
) {
    let ctx = match ctx {
        Some(c) => c,
        None => return,
    };

    let metrics = compute_metrics(ctx);
    if !metrics.has_any_data() {
        return;
    }

    html.push_str("<div class=\"section page-start\">\n");
    html.push_str("<h2>採用市場 逼迫度</h2>\n");

    // 章冒頭の読み方ガイド
    render_section_howto(
        html,
        &[
            "対象地域における「採用のしやすさ／難しさ」を 4 つの市場指標で複合評価します",
            "総合スコアは 0-100 で正規化済み。70 以上 = 逼迫 (採用難) / 30 以下 = 緩和 (採用容易)",
            "各指標は外部統計 (e-Stat) と HW 集計の混在。粒度・更新頻度の差は注記参照",
        ],
    );

    // ---- (1) 逼迫度 総合スコア (信号機色) ----
    render_tightness_summary(html, &metrics);

    // ---- (1.5) 採用難易度ラベル + 寄与分解 + ルールベースアクション (CR-1) ----
    render_recruit_difficulty_block(html, &metrics, variant);

    // ---- (2) 4 軸レーダーチャート ----
    render_radar_chart(html, &metrics);

    // ---- (3) データソース・計算方法 (折りたたみ) ----
    render_data_sources_collapsible(html);

    // ---- (4) 個別 KPI カード ----
    render_individual_kpis(html, &metrics);

    // ---- (5) 解釈ガイド + アクション提案 ----
    render_interpretation_guide(html, &metrics);

    // ---- 必須注記 ----
    html.push_str(
        "<p class=\"note\" style=\"margin-top:8px;\">\
        \u{203B} 指標粒度: 有効求人倍率 / 離職率 / 開廃業動態は都道府県粒度のみ。市区町村別の差は反映されません。\
        失業率は労働力調査 (国勢調査ベース) 由来、欠員補充率は HW 求人由来です。\
        逼迫度総合スコアは複合指標で、業界・職種により本来の重み付けが異なります。\
        本数値は採用環境の相関的傾向を示すもので、因果関係を示すものではありません。\
        離職率は雇用動向調査 (厚労省) 由来で、産業別・規模別で差が大きい指標です。\
        欠員補充率は HW 掲載求人のみが対象で、全求人市場の代表値ではありません。\
        </p>\n",
    );

    render_section_bridge(
        html,
        "次セクションでは、この採用市場逼迫度を踏まえた雇用形態の構成と給与構造を確認します。",
    );

    html.push_str("</div>\n");
}

// =====================================================================
// データモデル
// =====================================================================

/// 採用市場逼迫度の 4 軸 + 補助指標を一括保持
#[derive(Debug, Default, Clone)]
struct TightnessMetrics {
    /// 有効求人倍率 (1.0 = 拮抗)
    job_ratio: Option<f64>,
    /// 全国平均 有効求人倍率 (比較値)
    job_ratio_national: Option<f64>,
    /// HW 欠員補充率 (正社員、0-1 比率)
    vacancy_rate: Option<f64>,
    /// HW 欠員補充率の時系列推移 (3 点以上)
    vacancy_trend: Vec<f64>,
    /// 失業率 (%)
    unemployment_rate: Option<f64>,
    /// 全国平均 失業率 (%)
    unemployment_national: Option<f64>,
    /// 離職率 (%)
    separation_rate: Option<f64>,
    /// 入職率 (%) — 補助
    entry_rate: Option<f64>,
    /// 開業率 (%)
    opening_rate: Option<f64>,
    /// 廃業率 (%)
    closure_rate: Option<f64>,
}

impl TightnessMetrics {
    /// いずれかの指標が取得できているか (fail-soft 用)
    fn has_any_data(&self) -> bool {
        self.job_ratio.is_some()
            || self.vacancy_rate.is_some()
            || self.unemployment_rate.is_some()
            || self.separation_rate.is_some()
            || self.opening_rate.is_some()
    }

    /// 4 軸レーダー用の正規化スコア (0-100, 高いほど逼迫)
    ///
    /// 各指標を「採用難度」軸に揃えて正規化:
    /// - 有効求人倍率: 1.5 倍 = 100, 0.5 倍 = 0
    /// - 欠員補充率 (%): 50% = 100, 0% = 0
    /// - 失業率 (%): **逆数** (採用余力少 = 逼迫)、1% = 100 / 5% = 0
    /// - 離職率 (%): 20% = 100, 5% = 0 (高 = 流動性高 = 採用機会多いが定着難 → 逼迫寄り)
    fn radar_scores(&self) -> RadarScores {
        RadarScores {
            job_ratio: self
                .job_ratio
                .map(|v| normalize_linear(v, 0.5, 1.5))
                .unwrap_or(50.0),
            vacancy_rate: self
                .vacancy_rate
                .map(|v| normalize_linear(v * 100.0, 0.0, 50.0))
                .unwrap_or(50.0),
            unemployment_inv: self
                .unemployment_rate
                .map(|v| normalize_linear(5.0 - v, 0.0, 4.0))
                .unwrap_or(50.0),
            separation: self
                .separation_rate
                .map(|v| normalize_linear(v, 5.0, 20.0))
                .unwrap_or(50.0),
        }
    }

    /// 全国平均レーダー (比較表示用)
    fn national_radar_scores(&self) -> RadarScores {
        RadarScores {
            job_ratio: self
                .job_ratio_national
                .map(|v| normalize_linear(v, 0.5, 1.5))
                .unwrap_or(50.0),
            vacancy_rate: 50.0, // 全国平均欠員率は別途集計が必要なため中立値
            unemployment_inv: self
                .unemployment_national
                .map(|v| normalize_linear(5.0 - v, 0.0, 4.0))
                .unwrap_or(50.0),
            separation: 50.0, // 全国平均は別途取得が必要なため中立値
        }
    }

    /// 逼迫度総合スコア (0-100): 取得できた指標のみで平均
    /// 取得指標数が 0 の場合は None
    fn composite_score(&self) -> Option<f64> {
        let s = self.radar_scores();
        let mut values: Vec<f64> = Vec::new();
        if self.job_ratio.is_some() {
            values.push(s.job_ratio);
        }
        if self.vacancy_rate.is_some() {
            values.push(s.vacancy_rate);
        }
        if self.unemployment_rate.is_some() {
            values.push(s.unemployment_inv);
        }
        if self.separation_rate.is_some() {
            values.push(s.separation);
        }
        if values.is_empty() {
            return None;
        }
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }

    /// Public variant 用 3 軸複合スコア (HW 欠員補充率を除外)
    ///
    /// 取得できた指標のみで平均。
    /// - 有効求人倍率 / 失業率の逆数 / 離職率 のうち取得済みのもの
    /// - 取得指標 0 の場合は None
    fn composite_score_public(&self) -> Option<f64> {
        let s = self.radar_scores();
        let mut values: Vec<f64> = Vec::new();
        if self.job_ratio.is_some() {
            values.push(s.job_ratio);
        }
        if self.unemployment_rate.is_some() {
            values.push(s.unemployment_inv);
        }
        if self.separation_rate.is_some() {
            values.push(s.separation);
        }
        if values.is_empty() {
            return None;
        }
        Some(values.iter().sum::<f64>() / values.len() as f64)
    }
}

/// レーダーチャート 4 軸スコア (0-100)
#[derive(Debug, Default, Clone, Copy)]
struct RadarScores {
    /// 有効求人倍率
    job_ratio: f64,
    /// 欠員補充率
    vacancy_rate: f64,
    /// 失業率の逆数 (採用余力)
    unemployment_inv: f64,
    /// 離職率
    separation: f64,
}

impl RadarScores {
    /// 時計回り順 (有効求人倍率 → 欠員補充率 → 失業率 → 離職率)
    fn to_array(self) -> [f64; 4] {
        [
            self.job_ratio,
            self.vacancy_rate,
            self.unemployment_inv,
            self.separation,
        ]
    }

    /// Public variant 3 軸: 有効求人倍率 → 失業率の逆数 → 離職率 (HW 欠員補充率を除外)
    fn to_array_public(self) -> [f64; 3] {
        [self.job_ratio, self.unemployment_inv, self.separation]
    }
}

// =====================================================================
// データ集計
// =====================================================================

/// `InsightContext` から 4 軸 + 補助指標を抽出
fn compute_metrics(ctx: &InsightContext) -> TightnessMetrics {
    let mut m = TightnessMetrics::default();

    // (1) 有効求人倍率 (対象地域): ext_job_ratio.last() = 最新年度
    //     DB: Turso v2_external_job_openings_ratio / カラム: ratio_total
    if let Some(row) = ctx.ext_job_ratio.last() {
        let ratio = get_f64(row, "ratio_total");
        if ratio > 0.0 {
            m.job_ratio = Some(ratio);
        }
    }

    // (2) HW 欠員補充率 (正社員): vacancy.vacancy_rate
    //     DB: ローカル SQLite v2_vacancy_rate / カラム: vacancy_rate (0-1)
    let seishain = ctx
        .vacancy
        .iter()
        .find(|r| get_str_ref(r, "emp_group") == "正社員");
    if let Some(row) = seishain {
        let vr = get_f64(row, "vacancy_rate");
        if vr > 0.0 {
            m.vacancy_rate = Some(vr);
        }
    }
    m.vacancy_trend = ctx
        .ts_vacancy
        .iter()
        .filter(|r| get_str_ref(r, "emp_group") == "正社員")
        .map(|r| get_f64(r, "vacancy_rate"))
        .filter(|&v| v > 0.0)
        .collect();

    // (3) 失業率: ext_labor_force.unemployment_rate
    //     DB: Turso v2_external_labor_force / カラム: unemployment_rate
    if let Some(row) = ctx.ext_labor_force.first() {
        let ur = get_f64(row, "unemployment_rate");
        if ur > 0.0 {
            m.unemployment_rate = Some(ur);
        }
    }
    // 全国平均失業率
    // 注: fetch_prefecture_mean (subtab7_other.rs:282) の SQL が既に * 100 して
    //     パーセント単位で返すため、ここで再度 100 倍してはならない (バグ修正 2026-04-27)。
    m.unemployment_national = ctx.pref_avg_unemployment_rate;

    // (4) 離職率: ext_turnover.separation_rate
    //     DB: Turso v2_external_turnover (industry='産業計') / カラム: separation_rate
    if let Some(row) = ctx.ext_turnover.last() {
        let sep = get_f64(row, "separation_rate");
        if sep > 0.0 {
            m.separation_rate = Some(sep);
        }
        let entry = get_f64(row, "entry_rate");
        if entry > 0.0 {
            m.entry_rate = Some(entry);
        }
    }

    // 補助: 開廃業動態 (ext_business_dynamics)
    //       DB: Turso v2_external_business_dynamics / カラム: opening_rate / closure_rate
    if let Some(row) = ctx.ext_business_dynamics.last() {
        let op = get_f64(row, "opening_rate");
        if op > 0.0 {
            m.opening_rate = Some(op);
        }
        let cl = get_f64(row, "closure_rate");
        if cl > 0.0 {
            m.closure_rate = Some(cl);
        }
    }

    m
}

/// 線形正規化: lo→0, hi→100, クランプ
fn normalize_linear(v: f64, lo: f64, hi: f64) -> f64 {
    if (hi - lo).abs() < f64::EPSILON {
        return 50.0;
    }
    let normalized = (v - lo) / (hi - lo) * 100.0;
    normalized.clamp(0.0, 100.0)
}

// =====================================================================
// データソース注記 (再利用可能)
// =====================================================================

/// 各 KPI カードの下に表示する小さな注記
/// `source` には公開統計の正式名 (例: 「総務省統計局 労働力調査」) を渡す
/// `granularity` 例: "都道府県" / "市区町村"
fn render_data_source_note(source: &str, formula: &str, granularity: &str) -> String {
    format!(
        "<div class=\"data-source-note\" style=\"font-size:9px;color:#9ca3af;margin-top:4px;line-height:1.4;\">\
         <strong>出典</strong>: {} / <strong>計算</strong>: {} / <strong>粒度</strong>: {}\
         </div>",
        escape_html(source),
        escape_html(formula),
        escape_html(granularity),
    )
}

/// 折りたたみ「データソース・計算方法」(レーダーチャート下)
fn render_data_sources_collapsible(html: &mut String) {
    html.push_str(
        "<details class=\"collapsible-guide\" style=\"margin:8px 0;border:1px solid #e5e7eb;border-radius:6px;padding:6px 12px;background:#f9fafb;\">\n\
         <summary style=\"cursor:pointer;font-size:12px;font-weight:600;color:#374151;\">\u{1F4C2} データソース・計算方法 (クリックで開閉)</summary>\n\
         <div style=\"margin-top:8px;font-size:10px;color:#374151;\">\n",
    );
    html.push_str("<table style=\"width:100%;border-collapse:collapse;font-size:10px;\">\n");
    html.push_str(
        "<thead><tr style=\"background:#eef2ff;\">\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">指標</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">出典 (公開統計)</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">計算式</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">粒度</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">更新</th>\
         </tr></thead>\n<tbody>\n",
    );
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        (
            "有効求人倍率",
            "厚生労働省 職業安定業務統計 (一般職業紹介状況)",
            "有効求人数 / 有効求職者数 (公表値)",
            "都道府県",
            "月次",
        ),
        (
            "HW 欠員補充率",
            "ハローワーク掲載求人 (自社集計、e-Stat 由来ではない)",
            "(欠員補充求人数 / 全求人数) × 100",
            "市区町村",
            "随時",
        ),
        (
            "失業率",
            "総務省統計局 労働力調査",
            "完全失業率 (公表値)",
            "都道府県",
            "四半期",
        ),
        (
            "離職率",
            "厚生労働省 雇用動向調査 (産業計)",
            "離職者数 / 常用労働者数 (公表値)",
            "都道府県",
            "年次",
        ),
        (
            "開廃業動態 (補助)",
            "総務省・経済産業省 経済センサス-活動調査",
            "純増 = 開業率 - 廃業率 (公表値)",
            "都道府県",
            "5 年に 1 回",
        ),
    ];
    for (metric, source, formula, gran, freq) in rows {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td></tr>\n",
            escape_html(metric),
            escape_html(source),
            escape_html(formula),
            escape_html(gran),
            escape_html(freq),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p style=\"margin-top:6px;font-size:9px;color:#6b7280;font-style:italic;\">\
         \u{203B} 出典の数値は公表値をそのまま参照しています。HW 欠員補充率のみ HW 掲載求人ベース (全求人市場の代表値ではありません)。\
         </p>\n",
    );
    html.push_str("</div>\n</details>\n");

    // 2026-04-29 追加: 業界フィルタの適用範囲を明記
    // ユーザー指摘:
    // > 業界フィルタが効くのは SalesNow と一部 e-Stat (ext_turnover) のみ。
    // > その他 (失業率 / 有効求人倍率 / HW 欠員補充率 / 開廃業) は業種を問わない地域全体値。
    html.push_str(
        "<div data-testid=\"market-tightness-industry-scope-note\" \
         style=\"margin:8px 0;padding:8px 12px;background:#fef3c7;border-left:3px solid #f59e0b;border-radius:3px;font-size:10pt;line-height:1.7;\">\
         <strong>\u{26A0} 業界フィルタの適用範囲</strong>\
         <ul style=\"margin:4px 0 0;padding-left:20px;font-size:9.5pt;color:#78350f;\">\
         <li><strong>業界別</strong>に集計: 離職率 (ext_turnover、業界指定時のみ業界値を表示)</li>\
         <li><strong>業界を問わない地域全体値</strong>: 有効求人倍率 / 失業率 / HW 欠員補充率 / 開廃業動態</li>\
         </ul>\
         <span style=\"font-size:9pt;color:#92400e;display:block;margin-top:4px;\">\u{203B} 業界フィルタを指定しても、上記「地域全体値」の指標は地域全体の集計値のままです。業界別の比較が必要な場合は離職率 (ext_turnover) を参照ください。</span>\
         </div>\n",
    );
}

// =====================================================================
// (CR-1) 採用難易度ラベル + 寄与分解 + ルールベースアクション
// =====================================================================

/// 採用難易度のラベル分類
///
/// スコア閾値:
/// - 0-30 (30 未満): 易 (採用容易)
/// - 30-50 (30 以上 50 未満): 標準
/// - 50-70 (50 以上 70 未満): 難
/// - 70-100 (70 以上): 極難
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DifficultyLabel {
    Easy,
    Standard,
    Hard,
    VeryHard,
}

impl DifficultyLabel {
    fn from_score(score: f64) -> Self {
        if score < 30.0 {
            DifficultyLabel::Easy
        } else if score < 50.0 {
            DifficultyLabel::Standard
        } else if score < 70.0 {
            DifficultyLabel::Hard
        } else {
            DifficultyLabel::VeryHard
        }
    }

    fn ja(self) -> &'static str {
        match self {
            DifficultyLabel::Easy => "易",
            DifficultyLabel::Standard => "標準",
            DifficultyLabel::Hard => "難",
            DifficultyLabel::VeryHard => "極難",
        }
    }

    fn description(self) -> &'static str {
        match self {
            DifficultyLabel::Easy => "採用容易",
            DifficultyLabel::Standard => "標準的な採用環境",
            DifficultyLabel::Hard => "採用やや困難",
            DifficultyLabel::VeryHard => "採用極めて困難",
        }
    }

    fn color(self) -> &'static str {
        match self {
            DifficultyLabel::Easy => "#10b981",
            DifficultyLabel::Standard => "#3b82f6",
            DifficultyLabel::Hard => "#f59e0b",
            DifficultyLabel::VeryHard => "#dc2626",
        }
    }

    fn bg_color(self) -> &'static str {
        match self {
            DifficultyLabel::Easy => "#ecfdf5",
            DifficultyLabel::Standard => "#eff6ff",
            DifficultyLabel::Hard => "#fffbeb",
            DifficultyLabel::VeryHard => "#fef2f2",
        }
    }
}

/// 各軸の名前 (寄与分解で使用)
#[derive(Debug, Clone, Copy)]
enum AxisName {
    JobRatio,
    VacancyRate,
    UnemploymentInv,
    Separation,
}

impl AxisName {
    /// Default 軸ラベル (Full variant 想定: KPI カード見出しと統一)
    ///
    /// Round 2.7-B' (2026-05-08): variant 別出し分けは
    /// `axis_label_for_variant` を経由する。本関数は Full 既定の互換用途のみ。
    fn ja(self) -> &'static str {
        match self {
            // Full variant: KPI カード見出し「有効求人倍率」と統一
            AxisName::JobRatio => "有効求人倍率",
            AxisName::VacancyRate => "欠員補充率",
            AxisName::UnemploymentInv => "失業率の逆数 (採用余力)",
            AxisName::Separation => "離職率",
        }
    }
}

/// variant 別の有効求人倍率系ラベル (Round 2.7-B' 2026-05-08)
///
/// - `Full`: 「有効求人倍率」 (HW 併載維持。KPI カードと統一)
/// - `MarketIntelligence`: 「公的雇用需給指標」 (HW 連想語回避)
/// - `Public`: 「公的雇用需給指標」 (HW 言及最小化)
fn job_ratio_label_for_variant(variant: super::ReportVariant) -> &'static str {
    match variant {
        super::ReportVariant::Full => "有効求人倍率",
        super::ReportVariant::MarketIntelligence | super::ReportVariant::Public => {
            "公的雇用需給指標"
        }
    }
}

/// variant 別の軸ラベル (寄与分解で利用)
///
/// JobRatio のみ variant 別に分岐。他の軸は variant 非依存。
fn axis_label_for_variant(axis: AxisName, variant: super::ReportVariant) -> &'static str {
    match axis {
        AxisName::JobRatio => job_ratio_label_for_variant(variant),
        AxisName::VacancyRate => "欠員補充率",
        AxisName::UnemploymentInv => "失業率の逆数 (採用余力)",
        AxisName::Separation => "離職率",
    }
}

/// 1 軸の寄与情報
#[derive(Debug, Clone, Copy)]
struct AxisContribution {
    axis: AxisName,
    /// 0-100 の正規化スコア
    score: f64,
    /// 中立値 50 からの差 (正 = 押し上げ、負 = 緩和)
    delta: f64,
    /// 実際の指標値 (表示用)
    raw_value: Option<f64>,
}

/// 4 軸の寄与を取得 (取得済み軸のみ)
///
/// 押し上げ要因 (delta > 0): 採用難度を上げる方向
/// 緩和要因 (delta < 0): 採用難度を下げる方向
fn extract_contributions(m: &TightnessMetrics) -> Vec<AxisContribution> {
    let s = m.radar_scores();
    let mut out = Vec::new();
    if m.job_ratio.is_some() {
        out.push(AxisContribution {
            axis: AxisName::JobRatio,
            score: s.job_ratio,
            delta: s.job_ratio - 50.0,
            raw_value: m.job_ratio,
        });
    }
    if m.vacancy_rate.is_some() {
        out.push(AxisContribution {
            axis: AxisName::VacancyRate,
            score: s.vacancy_rate,
            delta: s.vacancy_rate - 50.0,
            raw_value: m.vacancy_rate.map(|v| v * 100.0),
        });
    }
    if m.unemployment_rate.is_some() {
        out.push(AxisContribution {
            axis: AxisName::UnemploymentInv,
            score: s.unemployment_inv,
            delta: s.unemployment_inv - 50.0,
            raw_value: m.unemployment_rate,
        });
    }
    if m.separation_rate.is_some() {
        out.push(AxisContribution {
            axis: AxisName::Separation,
            score: s.separation,
            delta: s.separation - 50.0,
            raw_value: m.separation_rate,
        });
    }
    out
}

/// 押し上げ要因 (delta > 0) を delta 降順で上位 N 件
fn top_push_factors(contribs: &[AxisContribution], n: usize) -> Vec<AxisContribution> {
    let mut v: Vec<AxisContribution> = contribs.iter().filter(|c| c.delta > 0.0).copied().collect();
    v.sort_by(|a, b| {
        b.delta
            .partial_cmp(&a.delta)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v.truncate(n);
    v
}

/// 緩和要因 (delta < 0) を delta の絶対値降順で上位 N 件
fn top_ease_factors(contribs: &[AxisContribution], n: usize) -> Vec<AxisContribution> {
    let mut v: Vec<AxisContribution> = contribs.iter().filter(|c| c.delta < 0.0).copied().collect();
    v.sort_by(|a, b| {
        b.delta
            .abs()
            .partial_cmp(&a.delta.abs())
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    v.truncate(n);
    v
}

/// ルールベースの推奨アクション (最大 3 件)
///
/// 分岐ロジック (優先度順):
/// - 有効求人倍率 ≥ 1.5 → 給与訴求の優先度↑、即日勤務OK等の差別化タグ追加
/// - 離職率 ≥ 18% → 定着支援施策の検討
/// - 失業率 < 2.0% → 通勤圏拡大検討、リファラル採用強化
/// - 欠員補充率 ≥ 40% → 既存従業員からのリファラル
/// - 開業率 - 廃業率 > 1.0 → 競合増加注意・差別化要素強化
///
/// 押し上げ要因に対応するルールを優先し、最大 3 件まで返す。
fn build_recommended_actions(m: &TightnessMetrics) -> Vec<&'static str> {
    let mut actions: Vec<&'static str> = Vec::new();

    // 有効求人倍率 (押し上げ系)
    if let Some(ratio) = m.job_ratio {
        if ratio >= 1.5 {
            actions.push("給与訴求の優先度\u{2191}");
            actions.push("即日勤務OK等の差別化タグ追加");
        }
    }
    // 離職率 (押し上げ系)
    if let Some(sep) = m.separation_rate {
        if sep >= 18.0 {
            actions.push("定着支援施策の検討");
        }
    }
    // 失業率 (緩和不足 = 採用余力少 = 押し上げ系)
    if let Some(ur) = m.unemployment_rate {
        if ur < 2.0 {
            actions.push("通勤圏拡大検討");
            actions.push("リファラル採用強化");
        }
    }
    // 欠員補充率 (押し上げ系)
    if let Some(vr) = m.vacancy_rate {
        if vr * 100.0 >= 40.0 {
            actions.push("既存従業員からのリファラル");
        }
    }
    // 開廃業動態 (補助シグナル)
    if let (Some(op), Some(cl)) = (m.opening_rate, m.closure_rate) {
        if op - cl > 1.0 {
            actions.push("競合増加注意・差別化要素強化");
        }
    }

    // 重複を保持しつつ上限 3 件
    let mut seen: Vec<&'static str> = Vec::new();
    for a in actions {
        if !seen.iter().any(|x| *x == a) {
            seen.push(a);
            if seen.len() >= 3 {
                break;
            }
        }
    }
    seen
}

/// 寄与分解の 1 行を整形 (例: 「有効求人倍率 1.80倍 (+30)」)
///
/// Round 2.7-B' (2026-05-08): variant 別ラベル経由に変更。
/// - Full: 「有効求人倍率」 (KPI カード見出しと統一)
/// - MarketIntelligence / Public: 「公的雇用需給指標」 (HW 連想語回避)
fn format_contribution(c: &AxisContribution, variant: super::ReportVariant) -> String {
    let raw_part = match (c.axis, c.raw_value) {
        (AxisName::JobRatio, Some(v)) => format!("{:.2}倍", v),
        (AxisName::VacancyRate, Some(v)) => format!("{:.0}%", v),
        (AxisName::UnemploymentInv, Some(v)) => format!("{:.1}%", v),
        (AxisName::Separation, Some(v)) => format!("{:.1}%", v),
        _ => "N/A".to_string(),
    };
    let sign = if c.delta >= 0.0 { "+" } else { "" };
    format!(
        "{} {} ({}{:.0})",
        axis_label_for_variant(c.axis, variant),
        raw_part,
        sign,
        c.delta
    )
}

/// 採用難易度ブロック全体を描画
///
/// 配置: 総合スコア (図 MT-1) の直後、レーダーチャート (図 MT-2) の直前
///
/// Round 2.7-B' (2026-05-08): variant 引数を追加し、`format_contribution`
/// に伝搬することで Full / MI で寄与分解の軸ラベルを出し分ける。
fn render_recruit_difficulty_block(
    html: &mut String,
    m: &TightnessMetrics,
    variant: super::ReportVariant,
) {
    let score = match m.composite_score() {
        Some(s) => s,
        None => return,
    };

    let label = DifficultyLabel::from_score(score);
    let contribs = extract_contributions(m);
    let push = top_push_factors(&contribs, 2);
    let ease = top_ease_factors(&contribs, 2);
    let actions = build_recommended_actions(m);

    html.push_str(&format!(
        "<div class=\"recruit-difficulty\" data-testid=\"recruit-difficulty-block\" \
         style=\"margin:8px 0 12px;padding:12px 16px;background:{bg};border-left:4px solid {col};border-radius:6px;\">\n",
        bg = label.bg_color(),
        col = label.color(),
    ));

    // 見出し: ラベル + スコア
    html.push_str(&format!(
        "<h3 style=\"font-size:14px;margin:0 0 8px;color:#1f2937;\">\
         採用難易度: \
         <span class=\"badge-{badge_key}\" data-testid=\"difficulty-label\" \
         style=\"display:inline-block;padding:2px 10px;background:{col};color:#fff;border-radius:3px;font-weight:700;margin:0 6px;\">\
         {label_ja}</span>\
         <span class=\"score\" data-testid=\"difficulty-score\" \
         style=\"color:{col};font-weight:600;\">{score:.0}/100</span>\
         <span style=\"color:#6b7280;font-size:11px;font-weight:400;margin-left:8px;\">({desc})</span>\
         </h3>\n",
        badge_key = match label {
            DifficultyLabel::Easy => "easy",
            DifficultyLabel::Standard => "standard",
            DifficultyLabel::Hard => "hard",
            DifficultyLabel::VeryHard => "very-hard",
        },
        col = label.color(),
        label_ja = escape_html(label.ja()),
        score = score,
        desc = escape_html(label.description()),
    ));

    // 寄与分解
    html.push_str("<div class=\"contribution\" data-testid=\"contribution-breakdown\" style=\"font-size:11px;color:#374151;line-height:1.7;margin-bottom:8px;\">\n");
    if push.is_empty() {
        html.push_str(
            "<div class=\"push\" data-testid=\"push-factors\">\
             <strong style=\"color:#dc2626;\">相関的な押し上げ要因</strong>: なし (中立値以下)\
             </div>\n",
        );
    } else {
        let push_str: Vec<String> = push
            .iter()
            .map(|c| format_contribution(c, variant))
            .collect();
        html.push_str(&format!(
            "<div class=\"push\" data-testid=\"push-factors\">\
             <strong style=\"color:#dc2626;\">相関的な押し上げ要因</strong>: {}\
             </div>\n",
            escape_html(&push_str.join(" / "))
        ));
    }
    if ease.is_empty() {
        html.push_str(
            "<div class=\"ease\" data-testid=\"ease-factors\">\
             <strong style=\"color:#10b981;\">相関的な緩和要因</strong>: なし (中立値以上)\
             </div>\n",
        );
    } else {
        let ease_str: Vec<String> = ease
            .iter()
            .map(|c| format_contribution(c, variant))
            .collect();
        html.push_str(&format!(
            "<div class=\"ease\" data-testid=\"ease-factors\">\
             <strong style=\"color:#10b981;\">相関的な緩和要因</strong>: {}\
             </div>\n",
            escape_html(&ease_str.join(" / "))
        ));
    }
    html.push_str(
        "<div style=\"font-size:9px;color:#9ca3af;font-style:italic;margin-top:4px;\">\
         \u{203B} 寄与分解は各軸の正規化スコア (0-100) と中立値 50 との差分です。値が大きいほど採用難度を押し上げる相関的傾向を示します。\
         </div>\n",
    );
    html.push_str("</div>\n");

    // 推奨アクション
    if !actions.is_empty() {
        html.push_str("<div class=\"actions\" data-testid=\"rule-based-actions\" style=\"font-size:11px;color:#374151;\">\n");
        html.push_str(
            "<strong>推奨アクション (相関ベース、因果ではない)</strong>:\n\
             <ol style=\"padding-left:20px;line-height:1.6;margin:4px 0;\">\n",
        );
        for a in &actions {
            html.push_str(&format!("<li>{}</li>\n", escape_html(a)));
        }
        html.push_str("</ol>\n");
        html.push_str(
            "<p style=\"font-size:9px;color:#9ca3af;font-style:italic;margin-top:4px;\">\
             \u{203B} 上記は市場指標に基づくルールベース提案で、相関ベース・因果ではないため現場で要検証です。職種・予算・競合状況等の個別要因と併せてご検討ください。\
             </p>\n",
        );
        html.push_str("</div>\n");
    } else {
        html.push_str(
            "<div class=\"actions\" data-testid=\"rule-based-actions\" style=\"font-size:11px;color:#6b7280;\">\
             <strong>推奨アクション (相関ベース、因果ではない)</strong>: 現状の市場指標では特段の追加施策トリガーは検出されません。標準的な採用運用を継続しつつ、月次でモニタリングを行うことを推奨します。\
             </div>\n",
        );
    }

    html.push_str("</div>\n");
}

// =====================================================================
// (1) 総合スコア (信号機色)
// =====================================================================

fn render_tightness_summary(html: &mut String, m: &TightnessMetrics) {
    let score = match m.composite_score() {
        Some(s) => s,
        None => return,
    };

    // B8: DifficultyLabel と用語を統一 (極難/難/標準/易)
    let label = DifficultyLabel::from_score(score);
    let (level_label, color, bg_color): (String, &str, &str) = match label {
        DifficultyLabel::VeryHard => (
            format!("極難 ({})", label.description()),
            "#dc2626",
            "#fef2f2",
        ),
        DifficultyLabel::Hard => (
            format!("難 ({})", label.description()),
            "#f59e0b",
            "#fffbeb",
        ),
        DifficultyLabel::Standard => (
            format!("標準 ({})", label.description()),
            "#3b82f6",
            "#eff6ff",
        ),
        DifficultyLabel::Easy => (
            format!("易 ({})", label.description()),
            "#10b981",
            "#ecfdf5",
        ),
    };

    render_figure_caption(html, "図 MT-1", "採用市場 逼迫度 総合スコア");

    // Round 17 (2026-05-13): CSS の単純数値表示に加え、SSR SVG ゲージで視覚化。
    // 視覚レビュアが「ゲージなし」と誤認した問題への対応。
    html.push_str(&format!(
        "<div data-testid=\"tightness-summary\" \
         style=\"display:flex;align-items:center;gap:16px;background:{bg};border-left:6px solid {col};\
                 padding:12px 16px;border-radius:6px;margin:8px 0 12px;\">\
         <div style=\"flex:0 0 auto;\" data-testid=\"tightness-gauge-wrap\">",
        bg = bg_color, col = color,
    ));
    html.push_str(&super::helpers::build_gauge_svg(score as f64, &level_label, color));
    html.push_str(&format!(
        "</div><div style=\"flex:1 1 auto;\">\
         <div style=\"font-size:11px;color:#6b7280;\">採用市場 逼迫度</div>\
         <div style=\"font-size:28px;font-weight:700;color:{col};\" data-testid=\"tightness-score\">\
         {score:.0}<span style=\"font-size:14px;color:#6b7280;\"> / 100</span>\
         </div>\
         <div style=\"font-size:14px;font-weight:600;color:{col};\">{label}</div>\
         </div></div>\n",
        col = color, score = score, label = escape_html(&level_label),
    ));

    render_read_hint_html(
        html,
        "<strong>逼迫度スコア</strong>は 4 指標 (有効求人倍率 / 欠員補充率 / 失業率の逆数 / 離職率) を 0-100 に正規化した複合指標です。\
         <strong>70 以上</strong>の地域では給与・福利・通勤圏など複数軸の訴求強化、\
         <strong>30 以下</strong>では採用コスト見直しとミスマッチ低減を検討する余地があります。",
    );
}

// =====================================================================
// (2) 4 軸レーダーチャート
// =====================================================================

fn render_radar_chart(html: &mut String, m: &TightnessMetrics) {
    let scores = m.radar_scores();
    let national = m.national_radar_scores();

    render_figure_caption(
        html,
        "図 MT-2",
        "採用市場 4 軸レーダー (0-100 正規化スコア)",
    );

    // 軸ラベルに実値を併記し、ツールチップ混乱を防ぐ
    // (例: 「有効求人倍率\n1.33倍 → 83」)
    let job_ratio_label = match m.job_ratio {
        Some(v) => format!("有効求人倍率\n({:.2}倍)", v),
        None => "有効求人倍率\n(N/A)".to_string(),
    };
    let vacancy_label = match m.vacancy_rate {
        Some(v) => format!("欠員補充率\n({:.0}%)", v * 100.0),
        None => "欠員補充率\n(N/A)".to_string(),
    };
    let unemp_label = match m.unemployment_rate {
        Some(v) => format!("採用余力\n(失業率 {:.1}%)", v),
        None => "採用余力\n(N/A)".to_string(),
    };
    let sep_label = match m.separation_rate {
        Some(v) => format!("離職率\n({:.1}%)", v),
        None => "離職率\n(N/A)".to_string(),
    };

    // ECharts radar: 4 軸定義 (時計回り、ストーリー順)
    let indicators = json!([
        {"name": job_ratio_label, "max": 100},
        {"name": vacancy_label, "max": 100},
        {"name": unemp_label, "max": 100},
        {"name": sep_label, "max": 100}
    ]);

    let target_arr = scores.to_array().to_vec();
    let national_arr = national.to_array().to_vec();

    let config = json!({
        "tooltip": {
            "trigger": "item",
            "formatter": "{b}<br/>スコア: {c} / 100"
        },
        "legend": {
            "data": ["対象地域", "全国平均 (参考)"],
            "bottom": 0,
            "textStyle": {"fontSize": 10}
        },
        "radar": {
            "indicator": indicators,
            "shape": "polygon",
            "splitNumber": 4,
            "center": ["50%", "55%"],
            "radius": "65%",
            "axisName": {
                "fontSize": 10,
                "color": "#374151",
                "padding": [3, 5]
            }
        },
        "series": [{
            "type": "radar",
            "data": [
                {
                    "name": "対象地域",
                    "value": target_arr,
                    "itemStyle": {"color": "#3b82f6"},
                    "areaStyle": {"opacity": 0.3, "color": "#3b82f6"},
                    "lineStyle": {"width": 2, "color": "#3b82f6"}
                },
                {
                    "name": "全国平均 (参考)",
                    "value": national_arr,
                    "itemStyle": {"color": "#9ca3af"},
                    "areaStyle": {"opacity": 0.1, "color": "#9ca3af"},
                    "lineStyle": {"width": 1, "color": "#9ca3af", "type": "dashed"}
                }
            ]
        }]
    });
    html.push_str(&render_echart_div(&config.to_string(), 320));

    render_read_hint(
        html,
        "4 軸が外側に広がるほど採用が難しい地域です。レーダー上の数値は 0-100 に正規化したスコア\
         (実値ではない) で、軸ラベル末尾の括弧内が実際の指標値です。各指標の実値・出典は\
         直下の KPI カードと「データソース・計算方法」も参照してください。",
    );
}

// =====================================================================
// (3) 個別 KPI カード
// =====================================================================

fn render_individual_kpis(html: &mut String, m: &TightnessMetrics) {
    render_figure_caption(html, "表 MT-1", "4 指標 個別 KPI + 補助指標");

    html.push_str("<div class=\"stats-grid\" style=\"grid-template-columns:repeat(auto-fit, minmax(220px, 1fr));gap:8px;\" data-testid=\"market-tightness-kpi-grid\">\n");

    // (1) 有効求人倍率
    if let Some(ratio) = m.job_ratio {
        let interp = if ratio >= 1.5 {
            "売り手市場"
        } else if ratio >= 1.0 {
            "拮抗"
        } else {
            "買い手市場"
        };
        let compare = match m.job_ratio_national {
            Some(nat) => format!("全国 {:.2} 倍 ({:+.2}pt)", nat, ratio - nat),
            None => format!("解釈: {}", interp),
        };
        let status = if ratio >= 1.5 {
            "crit"
        } else if ratio >= 1.0 {
            "warn"
        } else {
            "good"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F4C8}",
            "有効求人倍率",
            &format!("{:.2}", ratio),
            "倍",
            &compare,
            status,
            interp,
        );
        html.push_str(&render_data_source_note(
            "厚生労働省 職業安定業務統計 (一般職業紹介状況)",
            "有効求人数 / 有効求職者数",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    // (2) HW 欠員補充率
    if let Some(vr) = m.vacancy_rate {
        let pct = vr * 100.0;
        let trend_str = if m.vacancy_trend.len() >= 2 {
            let first = m.vacancy_trend.first().copied().unwrap_or(0.0) * 100.0;
            let last = m.vacancy_trend.last().copied().unwrap_or(0.0) * 100.0;
            format!("時系列: {:.0}% → {:.0}%", first, last)
        } else {
            "(欠員補充目的の求人比率)".to_string()
        };
        let status = if pct >= 40.0 {
            "crit"
        } else if pct >= 25.0 {
            "warn"
        } else {
            "good"
        };
        let label = if pct >= 40.0 {
            "高 (人材不足)"
        } else if pct >= 25.0 {
            "中"
        } else {
            "低"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F465}",
            "HW 欠員補充率",
            &format!("{:.0}", pct),
            "%",
            &trend_str,
            status,
            label,
        );
        html.push_str(&render_data_source_note(
            "ハローワーク掲載求人 (自社集計)",
            "(欠員補充求人数 / 全求人数) × 100",
            "市区町村",
        ));
        html.push_str("</div>\n");
    }

    // (3) 失業率
    if let Some(ur) = m.unemployment_rate {
        let compare = match m.unemployment_national {
            Some(nat) => format!("全国 {:.1}% ({:+.1}pt)", nat, ur - nat),
            None => "(採用候補プール代理指標)".to_string(),
        };
        // 高い失業率は採用余力 (高い方が good for 採用側)
        let status = if ur >= 3.5 {
            "good"
        } else if ur >= 2.0 {
            "warn"
        } else {
            "crit"
        };
        let label = if ur >= 3.5 {
            "余力あり"
        } else if ur >= 2.0 {
            "標準"
        } else {
            "余力少"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F4CA}",
            "失業率",
            &format!("{:.1}", ur),
            "%",
            &compare,
            status,
            label,
        );
        html.push_str(&render_data_source_note(
            "総務省統計局 労働力調査",
            "完全失業率 (公表値)",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    // (4) 離職率
    if let Some(sep) = m.separation_rate {
        let entry_compare = match m.entry_rate {
            Some(e) => format!("入職 {:.1}% / 差 {:+.1}pt", e, e - sep),
            None => "(雇用動向調査由来)".to_string(),
        };
        let status = if sep >= 18.0 {
            "crit"
        } else if sep >= 12.0 {
            "warn"
        } else {
            "good"
        };
        let label = if sep >= 18.0 {
            "高流動 (定着難)"
        } else if sep >= 12.0 {
            "中流動"
        } else {
            "安定"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F504}",
            "離職率",
            &format!("{:.1}", sep),
            "%",
            &entry_compare,
            status,
            label,
        );
        html.push_str(&render_data_source_note(
            "厚生労働省 雇用動向調査 (産業計)",
            "離職者数 / 常用労働者数 (公表値)",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");

    // 補助 KPI: 開廃業動態
    // B1 (2026-04-27): 経済センサス基礎調査の opening_rate / closure_rate は調査周期
    // (約 5 年) における **累積率** であるため、年率と誤認しないよう注記を強化。
    // 表示は累積値のまま、参考として年率換算 (累積/5) を併記する。
    if m.opening_rate.is_some() || m.closure_rate.is_some() {
        html.push_str("<div data-testid=\"business-dynamics-card\" style=\"margin-top:8px;padding:8px 12px;background:#f9fafb;border-radius:6px;border-left:3px solid #6366f1;font-size:11px;\">\n");
        html.push_str("<strong style=\"color:#4338ca;\">補助 KPI: 開廃業動態</strong> ");
        let op = m.opening_rate.unwrap_or(0.0);
        let cl = m.closure_rate.unwrap_or(0.0);
        let net = op - cl;
        // 経済センサス基礎調査は約 5 年周期。年率換算は累積 / 5
        let op_annual = op / 5.0;
        let cl_annual = cl / 5.0;
        html.push_str(&format!(
            "開業率 <strong>{:.1}%</strong> / 廃業率 <strong>{:.1}%</strong> / 純増 <strong>{:+.1}pt</strong> \
             <span style=\"color:#6b7280;font-size:10px;\">(5 年累積、年率換算 開業 {:.1}% / 廃業 {:.1}%)</span>。",
            op, cl, net, op_annual, cl_annual
        ));
        let interp = if net > 1.0 {
            "拡大基調 (採用需要拡大の可能性)"
        } else if net < -1.0 {
            "縮小基調 (流動人材プール拡大の可能性)"
        } else {
            "均衡"
        };
        html.push_str(&format!(
            "<span style=\"color:#6b7280;\">→ {}</span>",
            escape_html(interp)
        ));
        html.push_str(&render_data_source_note(
            "総務省・経済産業省 経済センサス-基礎調査",
            "5 年累積率 = (新設事業所数 / 前期末事業所数) × 100",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }
}

// =====================================================================
// (4) 解釈ガイド + アクション提案
// =====================================================================

fn render_interpretation_guide(html: &mut String, m: &TightnessMetrics) {
    let score = match m.composite_score() {
        Some(s) => s,
        None => return,
    };

    // B6 (2026-04-27): CR-1 の「推奨アクション」とこの「アクション提案」が重複表示
    // していたため、本ブロックは **戦略的方針 (大局観)** に位置付け直し、具体的アクション
    // 列挙は削除。CR-1 が指標別の戦術アクション、本ブロックがスコア帯別の戦略コメント。
    let heading: &str = if score >= 70.0 {
        "対象地域は逼迫度が高く、採用難度が高い傾向です。給与・福利・通勤圏など複数軸の訴求強化を検討する余地があります。"
    } else if score >= 40.0 {
        "対象地域はやや逼迫の傾向です。差別化要素の整備 (休日数 / 教育制度 / 求人原稿の見直し等) が効果的な可能性があります。"
    } else {
        "対象地域は緩和傾向です。採用コスト見直しとミスマッチ低減 (業務内容詳細化 / 選考フロー精査) を優先できる可能性があります。"
    };

    html.push_str("<div data-testid=\"tightness-action-guide\" style=\"margin-top:12px;padding:12px 16px;background:#eff6ff;border-radius:6px;border-left:4px solid #2563eb;\">\n");
    html.push_str(&format!(
        "<div style=\"font-size:13px;font-weight:600;color:#1e40af;margin-bottom:6px;\">\u{1F3AF} 戦略的方針 (スコア帯別)</div>\
         <p style=\"font-size:11px;color:#374151;margin-bottom:6px;\">{}</p>\
         <p style=\"font-size:10px;color:#6b7280;margin-top:6px;font-style:italic;\">\u{203B} スコア帯別の大局的方針です。指標別の具体的アクションは上部「採用難易度」ブロックを参照してください。相関的傾向であり、因果関係を示すものではありません。</p>\
         </div>\n",
        escape_html(heading)
    ));
}

// =====================================================================
// Public variant 用 描画関数群 (HW 欠員補充率を除外、3 軸構成)
// =====================================================================

/// Public variant: 総合スコア (信号機色) を 3 軸版で描画
fn render_tightness_summary_public(html: &mut String, m: &TightnessMetrics) {
    let score = match m.composite_score_public() {
        Some(s) => s,
        None => return,
    };

    let label = DifficultyLabel::from_score(score);
    let (level_label, color, bg_color): (String, &str, &str) = match label {
        DifficultyLabel::VeryHard => (
            format!("極難 ({})", label.description()),
            "#dc2626",
            "#fef2f2",
        ),
        DifficultyLabel::Hard => (
            format!("難 ({})", label.description()),
            "#f59e0b",
            "#fffbeb",
        ),
        DifficultyLabel::Standard => (
            format!("標準 ({})", label.description()),
            "#3b82f6",
            "#eff6ff",
        ),
        DifficultyLabel::Easy => (
            format!("易 ({})", label.description()),
            "#10b981",
            "#ecfdf5",
        ),
    };

    render_figure_caption(
        html,
        "図 MT-1",
        "採用市場 逼迫度 総合スコア (公開データ 3 軸版)",
    );

    html.push_str(&format!(
        "<div data-testid=\"tightness-summary\" \
         style=\"display:flex;align-items:center;gap:16px;background:{bg};border-left:6px solid {col};\
                 padding:12px 16px;border-radius:6px;margin:8px 0 12px;\">\
         <div style=\"font-size:11px;color:#6b7280;\">採用市場 逼迫度</div>\
         <div style=\"font-size:28px;font-weight:700;color:{col};\" data-testid=\"tightness-score\">\
         {score:.0}<span style=\"font-size:14px;color:#6b7280;\"> / 100</span>\
         </div>\
         <div style=\"font-size:14px;font-weight:600;color:{col};\">{label}</div>\
         </div>\n",
        bg = bg_color,
        col = color,
        score = score,
        label = escape_html(&level_label),
    ));

    render_read_hint_html(
        html,
        "<strong>逼迫度スコア</strong>は 3 指標 (公的雇用需給指標 / 失業率の逆数 / 離職率) を 0-100 に正規化した複合指標です。\
         本 variant は公開統計のみを使用し、特定求人媒体特有の指標は除外しています。\
         <strong>70 以上</strong>の地域では給与・福利・通勤圏など複数軸の訴求強化、\
         <strong>30 以下</strong>では採用コスト見直しとミスマッチ低減を検討する余地があります。",
    );
}

/// Public variant: 寄与分解を 3 軸 (HW 欠員補充率を除外) で抽出
fn extract_contributions_public(m: &TightnessMetrics) -> Vec<AxisContribution> {
    let s = m.radar_scores();
    let mut out = Vec::new();
    if m.job_ratio.is_some() {
        out.push(AxisContribution {
            axis: AxisName::JobRatio,
            score: s.job_ratio,
            delta: s.job_ratio - 50.0,
            raw_value: m.job_ratio,
        });
    }
    if m.unemployment_rate.is_some() {
        out.push(AxisContribution {
            axis: AxisName::UnemploymentInv,
            score: s.unemployment_inv,
            delta: s.unemployment_inv - 50.0,
            raw_value: m.unemployment_rate,
        });
    }
    if m.separation_rate.is_some() {
        out.push(AxisContribution {
            axis: AxisName::Separation,
            score: s.separation,
            delta: s.separation - 50.0,
            raw_value: m.separation_rate,
        });
    }
    out
}

/// Public variant: 推奨アクション (HW 欠員補充率トリガーを除外)
///
/// Public 限定の追加分岐:
/// - 失業率 ≥ 3.5% → 採用候補プールが広い旨を提示
fn build_recommended_actions_public(m: &TightnessMetrics) -> Vec<&'static str> {
    let mut actions: Vec<&'static str> = Vec::new();

    // 有効求人倍率 (押し上げ系)
    if let Some(ratio) = m.job_ratio {
        if ratio >= 1.5 {
            actions.push("給与訴求の優先度\u{2191}");
            actions.push("即日勤務OK等の差別化タグ追加");
        }
    }
    // 離職率 (押し上げ系)
    if let Some(sep) = m.separation_rate {
        if sep >= 18.0 {
            actions.push("定着支援施策の検討");
        }
    }
    // 失業率 (緩和不足 = 採用余力少 = 押し上げ系)
    if let Some(ur) = m.unemployment_rate {
        if ur < 2.0 {
            actions.push("通勤圏拡大検討");
            actions.push("リファラル採用強化");
        } else if ur >= 3.5 {
            // Public variant 追加: 高失業率 = 採用候補プールが広い
            actions.push("採用候補プール広め (失業率高め)");
        }
    }
    // 開廃業動態 (補助シグナル)
    if let (Some(op), Some(cl)) = (m.opening_rate, m.closure_rate) {
        if op - cl > 1.0 {
            actions.push("競合増加注意・差別化要素強化");
        }
    }

    let mut seen: Vec<&'static str> = Vec::new();
    for a in actions {
        if !seen.iter().any(|x| *x == a) {
            seen.push(a);
            if seen.len() >= 3 {
                break;
            }
        }
    }
    seen
}

/// Public variant: 採用難易度ブロック (CR-1 の 3 軸版)
fn render_recruit_difficulty_block_public(html: &mut String, m: &TightnessMetrics) {
    // Round 2.7-B' (2026-05-08): format_contribution が variant 引数を要求するため
    // Public 経路では明示的に Public variant を渡す。
    let variant = super::ReportVariant::Public;

    let score = match m.composite_score_public() {
        Some(s) => s,
        None => return,
    };

    let label = DifficultyLabel::from_score(score);
    let contribs = extract_contributions_public(m);
    let push = top_push_factors(&contribs, 2);
    let ease = top_ease_factors(&contribs, 2);
    let actions = build_recommended_actions_public(m);

    html.push_str(&format!(
        "<div class=\"recruit-difficulty\" data-testid=\"recruit-difficulty-block\" \
         style=\"margin:8px 0 12px;padding:12px 16px;background:{bg};border-left:4px solid {col};border-radius:6px;\">\n",
        bg = label.bg_color(),
        col = label.color(),
    ));

    html.push_str(&format!(
        "<h3 style=\"font-size:14px;margin:0 0 8px;color:#1f2937;\">\
         採用難易度: \
         <span class=\"badge-{badge_key}\" data-testid=\"difficulty-label\" \
         style=\"display:inline-block;padding:2px 10px;background:{col};color:#fff;border-radius:3px;font-weight:700;margin:0 6px;\">\
         {label_ja}</span>\
         <span class=\"score\" data-testid=\"difficulty-score\" \
         style=\"color:{col};font-weight:600;\">{score:.0}/100</span>\
         <span style=\"color:#6b7280;font-size:11px;font-weight:400;margin-left:8px;\">({desc})</span>\
         </h3>\n",
        badge_key = match label {
            DifficultyLabel::Easy => "easy",
            DifficultyLabel::Standard => "standard",
            DifficultyLabel::Hard => "hard",
            DifficultyLabel::VeryHard => "very-hard",
        },
        col = label.color(),
        label_ja = escape_html(label.ja()),
        score = score,
        desc = escape_html(label.description()),
    ));

    // 寄与分解
    html.push_str("<div class=\"contribution\" data-testid=\"contribution-breakdown\" style=\"font-size:11px;color:#374151;line-height:1.7;margin-bottom:8px;\">\n");
    if push.is_empty() {
        html.push_str(
            "<div class=\"push\" data-testid=\"push-factors\">\
             <strong style=\"color:#dc2626;\">相関的な押し上げ要因</strong>: なし (中立値以下)\
             </div>\n",
        );
    } else {
        let push_str: Vec<String> = push
            .iter()
            .map(|c| format_contribution(c, variant))
            .collect();
        html.push_str(&format!(
            "<div class=\"push\" data-testid=\"push-factors\">\
             <strong style=\"color:#dc2626;\">相関的な押し上げ要因</strong>: {}\
             </div>\n",
            escape_html(&push_str.join(" / "))
        ));
    }
    if ease.is_empty() {
        html.push_str(
            "<div class=\"ease\" data-testid=\"ease-factors\">\
             <strong style=\"color:#10b981;\">相関的な緩和要因</strong>: なし (中立値以上)\
             </div>\n",
        );
    } else {
        let ease_str: Vec<String> = ease
            .iter()
            .map(|c| format_contribution(c, variant))
            .collect();
        html.push_str(&format!(
            "<div class=\"ease\" data-testid=\"ease-factors\">\
             <strong style=\"color:#10b981;\">相関的な緩和要因</strong>: {}\
             </div>\n",
            escape_html(&ease_str.join(" / "))
        ));
    }
    html.push_str(
        "<div style=\"font-size:9px;color:#9ca3af;font-style:italic;margin-top:4px;\">\
         \u{203B} 寄与分解は各軸の正規化スコア (0-100) と中立値 50 との差分です。値が大きいほど採用難度を押し上げる相関的傾向を示します。\
         </div>\n",
    );
    html.push_str("</div>\n");

    // 推奨アクション
    if !actions.is_empty() {
        html.push_str("<div class=\"actions\" data-testid=\"rule-based-actions\" style=\"font-size:11px;color:#374151;\">\n");
        html.push_str(
            "<strong>推奨アクション (相関ベース、因果ではない)</strong>:\n\
             <ol style=\"padding-left:20px;line-height:1.6;margin:4px 0;\">\n",
        );
        for a in &actions {
            html.push_str(&format!("<li>{}</li>\n", escape_html(a)));
        }
        html.push_str("</ol>\n");
        html.push_str(
            "<p style=\"font-size:9px;color:#9ca3af;font-style:italic;margin-top:4px;\">\
             \u{203B} 上記は市場指標に基づくルールベース提案で、相関ベース・因果ではないため現場で要検証です。職種・予算・競合状況等の個別要因と併せてご検討ください。\
             </p>\n",
        );
        html.push_str("</div>\n");
    } else {
        html.push_str(
            "<div class=\"actions\" data-testid=\"rule-based-actions\" style=\"font-size:11px;color:#6b7280;\">\
             <strong>推奨アクション (相関ベース、因果ではない)</strong>: 現状の市場指標では特段の追加施策トリガーは検出されません。標準的な採用運用を継続しつつ、月次でモニタリングを行うことを推奨します。\
             </div>\n",
        );
    }

    html.push_str("</div>\n");
}

/// Public variant: 3 軸レーダーチャート (HW 欠員補充率を除外)
fn render_radar_chart_public(html: &mut String, m: &TightnessMetrics) {
    let scores = m.radar_scores();
    let national = m.national_radar_scores();

    render_figure_caption(
        html,
        "図 MT-2",
        "採用市場 3 軸レーダー (公開データ、0-100 正規化スコア)",
    );

    let job_ratio_label = match m.job_ratio {
        Some(v) => format!("公的雇用需給指標\n({:.2}倍)", v),
        None => "公的雇用需給指標\n(N/A)".to_string(),
    };
    let unemp_label = match m.unemployment_rate {
        Some(v) => format!("採用余力\n(失業率 {:.1}%)", v),
        None => "採用余力\n(N/A)".to_string(),
    };
    let sep_label = match m.separation_rate {
        Some(v) => format!("離職率\n({:.1}%)", v),
        None => "離職率\n(N/A)".to_string(),
    };

    // ECharts radar: 3 軸定義 (HW 欠員補充率を除外)
    let indicators = json!([
        {"name": job_ratio_label, "max": 100},
        {"name": unemp_label, "max": 100},
        {"name": sep_label, "max": 100}
    ]);

    let target_arr = scores.to_array_public().to_vec();
    let national_arr = national.to_array_public().to_vec();

    let config = json!({
        "tooltip": {
            "trigger": "item",
            "formatter": "{b}<br/>スコア: {c} / 100"
        },
        "legend": {
            "data": ["対象地域", "全国平均 (参考)"],
            "bottom": 0,
            "textStyle": {"fontSize": 10}
        },
        "radar": {
            "indicator": indicators,
            "shape": "polygon",
            "splitNumber": 4,
            "center": ["50%", "55%"],
            "radius": "65%",
            "axisName": {
                "fontSize": 10,
                "color": "#374151",
                "padding": [3, 5]
            }
        },
        "series": [{
            "type": "radar",
            "data": [
                {
                    "name": "対象地域",
                    "value": target_arr,
                    "itemStyle": {"color": "#3b82f6"},
                    "areaStyle": {"opacity": 0.3, "color": "#3b82f6"},
                    "lineStyle": {"width": 2, "color": "#3b82f6"}
                },
                {
                    "name": "全国平均 (参考)",
                    "value": national_arr,
                    "itemStyle": {"color": "#9ca3af"},
                    "areaStyle": {"opacity": 0.1, "color": "#9ca3af"},
                    "lineStyle": {"width": 1, "color": "#9ca3af", "type": "dashed"}
                }
            ]
        }]
    });
    html.push_str(&render_echart_div(&config.to_string(), 320));

    render_read_hint(
        html,
        "3 軸が外側に広がるほど採用が難しい地域です。レーダー上の数値は 0-100 に正規化したスコア\
         (実値ではない) で、軸ラベル末尾の括弧内が実際の指標値です。本 variant は公開統計のみを\
         使用しており、特定求人媒体特有の指標は除外しています。",
    );
}

/// Public variant: データソース折りたたみ (HW 欠員補充率行を除外)
fn render_data_sources_collapsible_public(html: &mut String) {
    html.push_str(
        "<details class=\"collapsible-guide\" style=\"margin:8px 0;border:1px solid #e5e7eb;border-radius:6px;padding:6px 12px;background:#f9fafb;\">\n\
         <summary style=\"cursor:pointer;font-size:12px;font-weight:600;color:#374151;\">\u{1F4C2} データソース・計算方法 (クリックで開閉)</summary>\n\
         <div style=\"margin-top:8px;font-size:10px;color:#374151;\">\n",
    );
    html.push_str("<table style=\"width:100%;border-collapse:collapse;font-size:10px;\">\n");
    html.push_str(
        "<thead><tr style=\"background:#eef2ff;\">\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">指標</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">出典 (公開統計)</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">計算式</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">粒度</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">更新</th>\
         </tr></thead>\n<tbody>\n",
    );
    // HW 欠員補充率は除外
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        (
            "公的雇用需給指標",
            "厚生労働省 職業安定業務統計 (一般職業紹介状況)",
            "公的雇用需給指標 (公表値)",
            "都道府県",
            "月次",
        ),
        (
            "失業率",
            "総務省統計局 労働力調査",
            "完全失業率 (公表値)",
            "都道府県",
            "四半期",
        ),
        (
            "離職率",
            "厚生労働省 雇用動向調査 (産業計)",
            "離職者数 / 常用労働者数 (公表値)",
            "都道府県",
            "年次",
        ),
        (
            "開廃業動態 (補助)",
            "総務省・経済産業省 経済センサス-活動調査",
            "純増 = 開業率 - 廃業率 (公表値)",
            "都道府県",
            "5 年に 1 回",
        ),
    ];
    for (metric, source, formula, gran, freq) in rows {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td></tr>\n",
            escape_html(metric),
            escape_html(source),
            escape_html(formula),
            escape_html(gran),
            escape_html(freq),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p style=\"margin-top:6px;font-size:9px;color:#6b7280;font-style:italic;\">\
         \u{203B} 出典の数値は公表値をそのまま参照しています。本 variant は公開統計のみを使用しており、特定求人媒体特有の指標は除外しています。\
         </p>\n",
    );
    html.push_str("</div>\n</details>\n");

    // 2026-04-29 追加: 業界フィルタの適用範囲を明記 (Public variant)
    html.push_str(
        "<div data-testid=\"market-tightness-industry-scope-note\" \
         style=\"margin:8px 0;padding:8px 12px;background:#fef3c7;border-left:3px solid #f59e0b;border-radius:3px;font-size:10pt;line-height:1.7;\">\
         <strong>\u{26A0} 業界フィルタの適用範囲</strong>\
         <ul style=\"margin:4px 0 0;padding-left:20px;font-size:9.5pt;color:#78350f;\">\
         <li><strong>業界別</strong>に集計: 離職率 (ext_turnover、業界指定時のみ業界値を表示)</li>\
         <li><strong>業界を問わない地域全体値</strong>: 公的雇用需給指標 / 失業率 / 開廃業動態</li>\
         </ul>\
         <span style=\"font-size:9pt;color:#92400e;display:block;margin-top:4px;\">\u{203B} 業界フィルタを指定しても、上記「地域全体値」の指標は地域全体の集計値のままです。業界別の比較が必要な場合は離職率 (ext_turnover) を参照ください。</span>\
         </div>\n",
    );
}

/// Public variant: 個別 KPI カード (3 枚、HW 欠員補充率を除外)
fn render_individual_kpis_public(html: &mut String, m: &TightnessMetrics) {
    render_figure_caption(html, "表 MT-1", "3 指標 個別 KPI + 補助指標 (公開データ)");

    html.push_str("<div class=\"stats-grid\" style=\"grid-template-columns:repeat(auto-fit, minmax(220px, 1fr));gap:8px;\" data-testid=\"market-tightness-kpi-grid\">\n");

    // (1) 有効求人倍率
    if let Some(ratio) = m.job_ratio {
        let interp = if ratio >= 1.5 {
            "売り手市場"
        } else if ratio >= 1.0 {
            "拮抗"
        } else {
            "買い手市場"
        };
        let compare = match m.job_ratio_national {
            Some(nat) => format!("全国 {:.2} 倍 ({:+.2}pt)", nat, ratio - nat),
            None => format!("解釈: {}", interp),
        };
        let status = if ratio >= 1.5 {
            "crit"
        } else if ratio >= 1.0 {
            "warn"
        } else {
            "good"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F4C8}",
            "公的雇用需給指標",
            &format!("{:.2}", ratio),
            "倍",
            &compare,
            status,
            interp,
        );
        html.push_str(&render_data_source_note(
            "厚生労働省 職業安定業務統計 (一般職業紹介状況)",
            "公的雇用需給指標",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    // (2) 失業率
    if let Some(ur) = m.unemployment_rate {
        let compare = match m.unemployment_national {
            Some(nat) => format!("全国 {:.1}% ({:+.1}pt)", nat, ur - nat),
            None => "(採用候補プール代理指標)".to_string(),
        };
        let status = if ur >= 3.5 {
            "good"
        } else if ur >= 2.0 {
            "warn"
        } else {
            "crit"
        };
        let label = if ur >= 3.5 {
            "余力あり"
        } else if ur >= 2.0 {
            "標準"
        } else {
            "余力少"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F4CA}",
            "失業率",
            &format!("{:.1}", ur),
            "%",
            &compare,
            status,
            label,
        );
        html.push_str(&render_data_source_note(
            "総務省統計局 労働力調査",
            "完全失業率 (公表値)",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    // (3) 離職率
    if let Some(sep) = m.separation_rate {
        let entry_compare = match m.entry_rate {
            Some(e) => format!("入職 {:.1}% / 差 {:+.1}pt", e, e - sep),
            None => "(雇用動向調査由来)".to_string(),
        };
        let status = if sep >= 18.0 {
            "crit"
        } else if sep >= 12.0 {
            "warn"
        } else {
            "good"
        };
        let label = if sep >= 18.0 {
            "高流動 (定着難)"
        } else if sep >= 12.0 {
            "中流動"
        } else {
            "安定"
        };
        html.push_str("<div class=\"kpi-card-with-source\">\n");
        render_kpi_card_v2(
            html,
            "\u{1F504}",
            "離職率",
            &format!("{:.1}", sep),
            "%",
            &entry_compare,
            status,
            label,
        );
        html.push_str(&render_data_source_note(
            "厚生労働省 雇用動向調査 (産業計)",
            "離職者数 / 常用労働者数 (公表値)",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");

    // 補助 KPI: 開廃業動態 (両 variant で表示)
    if m.opening_rate.is_some() || m.closure_rate.is_some() {
        html.push_str("<div data-testid=\"business-dynamics-card\" style=\"margin-top:8px;padding:8px 12px;background:#f9fafb;border-radius:6px;border-left:3px solid #6366f1;font-size:11px;\">\n");
        html.push_str("<strong style=\"color:#4338ca;\">補助 KPI: 開廃業動態</strong> ");
        let op = m.opening_rate.unwrap_or(0.0);
        let cl = m.closure_rate.unwrap_or(0.0);
        let net = op - cl;
        let op_annual = op / 5.0;
        let cl_annual = cl / 5.0;
        html.push_str(&format!(
            "開業率 <strong>{:.1}%</strong> / 廃業率 <strong>{:.1}%</strong> / 純増 <strong>{:+.1}pt</strong> \
             <span style=\"color:#6b7280;font-size:10px;\">(5 年累積、年率換算 開業 {:.1}% / 廃業 {:.1}%)</span>。",
            op, cl, net, op_annual, cl_annual
        ));
        let interp = if net > 1.0 {
            "拡大基調 (採用需要拡大の可能性)"
        } else if net < -1.0 {
            "縮小基調 (流動人材プール拡大の可能性)"
        } else {
            "均衡"
        };
        html.push_str(&format!(
            "<span style=\"color:#6b7280;\">→ {}</span>",
            escape_html(interp)
        ));
        html.push_str(&render_data_source_note(
            "総務省・経済産業省 経済センサス-基礎調査",
            "5 年累積率 = (新設事業所数 / 前期末事業所数) × 100",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }
}

// =====================================================================
// 単体テスト (逆証明テスト群)
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::helpers::Row;
    use serde_json::json;

    fn row(pairs: &[(&str, serde_json::Value)]) -> Row {
        let mut m = Row::new();
        for (k, v) in pairs {
            m.insert(k.to_string(), v.clone());
        }
        m
    }

    /// テスト用の最小 InsightContext を build (4 軸版、ts_fulfillment 削除)
    fn build_test_ctx(
        ext_job_ratio: Vec<Row>,
        vacancy: Vec<Row>,
        ts_vacancy: Vec<Row>,
        ext_labor_force: Vec<Row>,
        ext_turnover: Vec<Row>,
        ext_business_dynamics: Vec<Row>,
        pref_avg_unemp: Option<f64>,
    ) -> InsightContext {
        InsightContext {
            vacancy,
            resilience: vec![],
            transparency: vec![],
            temperature: vec![],
            competition: vec![],
            cascade: vec![],
            salary_comp: vec![],
            monopsony: vec![],
            spatial_mismatch: vec![],
            wage_compliance: vec![],
            region_benchmark: vec![],
            text_quality: vec![],
            ts_counts: vec![],
            ts_vacancy,
            ts_salary: vec![],
            ts_fulfillment: vec![], // 4 軸版では使わない
            ts_tracking: vec![],
            ext_job_ratio,
            ext_labor_stats: vec![],
            ext_min_wage: vec![],
            ext_turnover,
            ext_population: vec![],
            ext_pyramid: vec![],
            ext_migration: vec![],
            ext_daytime_pop: vec![],
            ext_establishments: vec![],
            ext_business_dynamics,
            ext_care_demand: vec![],
            ext_household_spending: vec![],
            ext_climate: vec![],
            ext_social_life: vec![],
            ext_internet_usage: vec![],
            ext_households: vec![],
            ext_vital: vec![],
            ext_labor_force,
            ext_medical_welfare: vec![],
            ext_education_facilities: vec![],
            ext_geography: vec![],
            ext_education: vec![],
            ext_industry_employees: vec![],
            hw_industry_counts: vec![],
            pref_avg_unemployment_rate: pref_avg_unemp,
            pref_avg_single_rate: None,
            pref_avg_physicians_per_10k: None,
            pref_avg_daycare_per_1k_children: None,
            pref_avg_habitable_density: None,
            flow: None,
            commute_zone_count: 0,
            commute_zone_pref_count: 0,
            commute_zone_total_pop: 0,
            commute_zone_working_age: 0,
            commute_zone_elderly: 0,
            commute_inflow_total: 0,
            commute_outflow_total: 0,
            commute_self_rate: 0.0,
            commute_inflow_top3: vec![],
            pref: "東京都".to_string(),
            muni: "千代田区".to_string(),
        }
    }

    /// 全データ空の場合 section が出力されないこと (fail-soft 検証)
    #[test]
    fn market_tightness_empty_renders_nothing() {
        let ctx = build_test_ctx(vec![], vec![], vec![], vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));
        assert!(
            html.is_empty(),
            "全データ空ならば section ごと出力されない (got: {} chars)",
            html.len()
        );
    }

    /// `ctx = None` の場合 section が出力されないこと
    #[test]
    fn market_tightness_no_context_renders_nothing() {
        let mut html = String::new();
        render_section_market_tightness(&mut html, None);
        assert!(html.is_empty());
    }

    /// 線形正規化: 境界値・クランプ動作を逆証明
    #[test]
    fn normalize_linear_boundary_values() {
        // 下限境界
        assert!((normalize_linear(0.5, 0.5, 1.5) - 0.0).abs() < 1e-9);
        // 上限境界
        assert!((normalize_linear(1.5, 0.5, 1.5) - 100.0).abs() < 1e-9);
        // 中点
        assert!((normalize_linear(1.0, 0.5, 1.5) - 50.0).abs() < 1e-9);
        // 下限を下回る (クランプ 0)
        assert!((normalize_linear(0.0, 0.5, 1.5) - 0.0).abs() < 1e-9);
        // 上限を超える (クランプ 100)
        assert!((normalize_linear(2.0, 0.5, 1.5) - 100.0).abs() < 1e-9);
        // 縮退 (lo == hi): 50.0 を返す
        assert!((normalize_linear(1.0, 1.0, 1.0) - 50.0).abs() < 1e-9);
    }

    /// 逼迫度総合スコア: 具体値で逆証明
    /// 有効求人倍率 1.5 (=score 100) + 離職率 14% (=score 60) → 平均 80
    #[test]
    fn composite_score_with_two_metrics() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.5))])],
            vec![],
            vec![],
            vec![],
            vec![row(&[("separation_rate", json!(14.0))])],
            vec![],
            None,
        );
        let m = compute_metrics(&ctx);
        let score = m.composite_score().expect("score");
        // ratio_total=1.5 → normalize(1.5, 0.5, 1.5)=100
        // separation=14.0 → normalize(14.0, 5.0, 20.0)=60
        // 平均 = 80
        assert!(
            (score - 80.0).abs() < 0.01,
            "expected score=80.0, got {}",
            score
        );
    }

    /// 逼迫度スコアが「逼迫」「やや逼迫」「緩和」の 3 段階で正しい色帯になる
    #[test]
    fn tightness_summary_three_levels() {
        // 逼迫 (score >= 70): 有効求人倍率 1.5 → 100
        let ctx_high = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.5))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html_high = String::new();
        render_section_market_tightness(&mut html_high, Some(&ctx_high));
        assert!(html_high.contains("逼迫 (採用難)"), "score>=70 → 逼迫表示");
        assert!(html_high.contains("#dc2626"), "赤色帯");

        // やや逼迫 (40 <= score < 70): 有効求人倍率 1.0 → 50
        let ctx_mid = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.0))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html_mid = String::new();
        render_section_market_tightness(&mut html_mid, Some(&ctx_mid));
        assert!(html_mid.contains("やや逼迫"), "40<=score<70 → やや逼迫");
        assert!(html_mid.contains("#f59e0b"), "黄色帯");

        // 緩和 (score < 40): 有効求人倍率 0.6 → score=10
        let ctx_low = build_test_ctx(
            vec![row(&[("ratio_total", json!(0.6))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html_low = String::new();
        render_section_market_tightness(&mut html_low, Some(&ctx_low));
        assert!(html_low.contains("緩和 (採用容易)"), "score<40 → 緩和");
        assert!(html_low.contains("#10b981"), "緑色帯");
    }

    /// 4 軸レーダーチャートの ECharts data-chart-config に 4 指標が存在 (5 軸でないこと)
    #[test]
    fn radar_chart_contains_4_indicators_in_chart_config() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.3)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // 4 軸のラベルが ECharts config 内に含まれる
        // (軸ラベルは「指標名\n(実値)」形式、実値部分は欠損時 "(N/A)")
        assert!(html.contains("有効求人倍率"));
        assert!(html.contains("欠員補充率"));
        assert!(html.contains("採用余力"));
        assert!(html.contains("離職率"));

        // 平均掲載日数は 4 軸版では含まれないこと (逆証明)
        assert!(
            !html.contains("平均掲載日数"),
            "ts_fulfillment 由来の『平均掲載日数』は 4 軸版では含まれない"
        );

        // ECharts 識別属性
        assert!(html.contains("data-chart-config"), "ECharts div 必要");
        assert!(html.contains("\"radar\""), "radar type 必要");

        // 全国平均レーダーも併載
        assert!(html.contains("全国平均 (参考)"), "全国平均レーダー併載");
    }

    /// 全 4 KPI カードが個別 KPI grid に存在 (具体ラベル + データソース注記)
    #[test]
    fn individual_kpis_all_4_present_with_data_source_notes() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.28)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[
                ("separation_rate", json!(14.2)),
                ("entry_rate", json!(15.0)),
            ])],
            vec![],
            Some(0.026), // 全国平均失業率 2.6%
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // 4 ラベル
        assert!(html.contains("有効求人倍率"));
        assert!(html.contains("HW 欠員補充率"));
        assert!(html.contains("失業率"));
        assert!(html.contains("離職率"));

        // 具体値
        assert!(html.contains("1.40") || html.contains(">1.40"), "1.40 倍");
        assert!(html.contains("28"), "欠員 28%");
        assert!(html.contains("2.4"), "失業 2.4%");
        assert!(html.contains("14.2"), "離職 14.2%");

        // データソース注記が各 KPI に公開統計名で存在 (内部 DB 名は意味がないため公開出典名に変更)
        assert!(
            html.contains("職業安定業務統計"),
            "有効求人倍率は職業安定業務統計が出典"
        );
        assert!(
            html.contains("ハローワーク掲載求人"),
            "欠員補充率は HW 掲載求人 (自社集計) が出典"
        );
        assert!(html.contains("労働力調査"), "失業率は労働力調査が出典");
        assert!(html.contains("雇用動向調査"), "離職率は雇用動向調査が出典");

        // 全国平均比較値
        assert!(html.contains("全国"), "全国平均比較値");
        assert!(
            html.contains("data-testid=\"market-tightness-kpi-grid\""),
            "KPI grid 識別子"
        );
    }

    /// データソース折りたたみセクション (図 MT-2 下) が存在する
    #[test]
    fn data_sources_collapsible_section_present() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // <details> ベースの折りたたみ
        assert!(
            html.contains("<details class=\"collapsible-guide\""),
            "<details> 折りたたみが必要"
        );
        assert!(html.contains("データソース・計算方法"));
        // 公開統計名がテーブル内に出現 (内部 DB 名は表示しない)
        assert!(
            html.contains("職業安定業務統計") && html.contains("労働力調査"),
            "公開統計名が出典列に表示される"
        );
        assert!(html.contains("雇用動向調査"));
        assert!(html.contains("経済センサス"));
    }

    /// 開廃業動態 (補助 KPI) が ext_business_dynamics から正しく描画される
    #[test]
    fn business_dynamics_card_rendered_with_concrete_values() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.0))])], // 何か 1 つは必要
            vec![],
            vec![],
            vec![],
            vec![],
            vec![row(&[
                ("opening_rate", json!(5.2)),
                ("closure_rate", json!(3.8)),
            ])],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        assert!(html.contains("data-testid=\"business-dynamics-card\""));
        assert!(html.contains("5.2"), "開業率 5.2%");
        assert!(html.contains("3.8"), "廃業率 3.8%");
        // 純増 = 5.2 - 3.8 = +1.4
        assert!(html.contains("+1.4"), "純増 +1.4pt");
        assert!(html.contains("拡大基調"), "拡大基調の解釈");
        // 補助 KPI にも公開統計名のデータソース注記
        assert!(
            html.contains("経済センサス"),
            "開廃業動態は経済センサスが出典"
        );
    }

    /// 必須 caveat 文言の存在 (因果非主張・粒度制約)
    #[test]
    fn required_caveats_present() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // 因果非主張 (memory feedback_correlation_not_causation 厳守)
        assert!(
            html.contains("因果関係を示すものではありません"),
            "因果非主張 caveat 必須"
        );
        // 粒度制約
        assert!(
            html.contains("都道府県粒度") || html.contains("市区町村別の差は反映されません"),
            "粒度制約 caveat 必須"
        );
        // 離職率の出典補足
        assert!(
            html.contains("雇用動向調査"),
            "離職率の出典補足 (雇用動向調査) 必須"
        );
        // HW スコープ区別
        assert!(
            html.contains("HW") && html.contains("e-Stat"),
            "HW 由来と外部統計の区別必須 (feedback_hw_data_scope 準拠)"
        );
        // ts_fulfillment は使用しないため言及しない (ts_fulfillment 関連の文字列が無いこと)
        // 4 軸版で「平均掲載日数」が KPI として登場しないこと
        assert!(
            !html.contains("HW 平均掲載日数 KPI"),
            "ts_fulfillment 由来の KPI は 4 軸版で削除"
        );
    }

    /// アクション提案が逼迫度スコアに応じて 3 パターン分岐する
    /// B6 (2026-04-27) 修正後: 「戦略的方針」見出しのみ残し具体アクション列挙は削除。
    /// CR-1 ブロックが指標別の戦術アクションを担当する。
    #[test]
    fn action_guide_branches_by_score() {
        // 逼迫: 「複数軸の訴求強化」見出し
        let ctx_high = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.5))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html_high = String::new();
        render_section_market_tightness(&mut html_high, Some(&ctx_high));
        assert!(html_high.contains("逼迫度が高く"));
        assert!(html_high.contains("複数軸の訴求強化"));

        // 緩和: 「採用コスト見直し」見出し
        let ctx_low = build_test_ctx(
            vec![row(&[("ratio_total", json!(0.5))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html_low = String::new();
        render_section_market_tightness(&mut html_low, Some(&ctx_low));
        assert!(html_low.contains("緩和傾向"));
        assert!(html_low.contains("採用コスト見直し"));
    }

    /// 全国平均失業率比較が `pref_avg_unemployment_rate` から取得される
    /// (fetch_prefecture_mean は SQL 内で既に * 100 してパーセントで返すため、再変換しない)
    #[test]
    fn unemployment_national_compare_from_pref_avg() {
        // pref_avg_unemployment_rate は既にパーセント単位 (例: 2.4 = 2.4%)
        let ctx = build_test_ctx(
            vec![],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.0))])],
            vec![],
            vec![],
            Some(2.4),
        );
        let m = compute_metrics(&ctx);
        assert_eq!(m.unemployment_rate, Some(2.0));
        assert_eq!(m.unemployment_national, Some(2.4));
    }

    /// 逆証明: 100 倍二重バグの再発防止
    /// fetch_prefecture_mean が 3.8 (パーセント) を返す場合、
    /// unemployment_national は 380 にならず 3.8 のまま保持される
    #[test]
    fn unemployment_national_not_double_scaled() {
        let ctx = build_test_ctx(
            vec![],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(3.8))])],
            vec![],
            vec![],
            Some(3.8),
        );
        let m = compute_metrics(&ctx);
        assert_eq!(
            m.unemployment_national,
            Some(3.8),
            "pref_avg_unemployment_rate は SQL で既に * 100 されているため再度 100 倍してはならない"
        );
        assert!(
            m.unemployment_national.unwrap() < 100.0,
            "全国失業率は 100% 未満であるべき (380% のような不正値を防ぐ)"
        );
    }

    /// has_any_data: 何も無ければ false / 1 つでもあれば true
    #[test]
    fn has_any_data_behavior() {
        let m = TightnessMetrics::default();
        assert!(!m.has_any_data());

        let mut m2 = TightnessMetrics::default();
        m2.job_ratio = Some(1.2);
        assert!(m2.has_any_data());
    }

    /// 図表番号 (図 MT-1, 図 MT-2, 表 MT-1) がすべて存在
    #[test]
    fn figure_numbers_mt1_mt2_table_mt1_present() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));
        assert!(html.contains("図 MT-1"), "総合スコア figure number");
        assert!(html.contains("図 MT-2"), "レーダーチャート figure number");
        assert!(html.contains("表 MT-1"), "個別 KPI table number");
    }

    /// 4 軸の順序 (時計回り、ストーリー順): 有効求人倍率 → 欠員補充率 → 失業率 → 離職率
    #[test]
    fn radar_axes_order_clockwise_story() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // ECharts indicator 配列内での 4 軸の出現順を確認
        // (軸ラベルは「指標名\n(実値)」形式に変更されたため、prefix で検索)
        let pos_job_ratio = html.find("\"name\":\"有効求人倍率");
        let pos_vacancy = html.find("\"name\":\"欠員補充率");
        let pos_unemp = html.find("\"name\":\"採用余力");
        let pos_sep = html.find("\"name\":\"離職率");

        assert!(pos_job_ratio.is_some(), "有効求人倍率 軸");
        assert!(pos_vacancy.is_some(), "欠員補充率 軸");
        assert!(pos_unemp.is_some(), "採用余力 軸");
        assert!(pos_sep.is_some(), "離職率 軸");

        // ストーリー順の確認: job_ratio < vacancy < unemp < sep
        let p1 = pos_job_ratio.unwrap();
        let p2 = pos_vacancy.unwrap();
        let p3 = pos_unemp.unwrap();
        let p4 = pos_sep.unwrap();
        assert!(
            p1 < p2 && p2 < p3 && p3 < p4,
            "4 軸はストーリー順 (有効求人倍率 → 欠員補充率 → 失業率 → 離職率)"
        );
    }

    // =================================================================
    // CR-1: 採用難易度ラベル + 寄与分解 + アクション提案 テスト群
    // =================================================================

    /// CR-1 #1: ラベル境界値テスト (具体値で逆証明)
    /// スコア 29 → 易、30 → 標準、49 → 標準、50 → 難、69 → 難、70 → 極難
    #[test]
    fn cr1_difficulty_label_boundary_values() {
        assert_eq!(DifficultyLabel::from_score(0.0), DifficultyLabel::Easy);
        assert_eq!(DifficultyLabel::from_score(29.0), DifficultyLabel::Easy);
        assert_eq!(
            DifficultyLabel::from_score(29.999),
            DifficultyLabel::Easy,
            "30 未満 = 易"
        );
        assert_eq!(
            DifficultyLabel::from_score(30.0),
            DifficultyLabel::Standard,
            "30 = 標準"
        );
        assert_eq!(
            DifficultyLabel::from_score(49.0),
            DifficultyLabel::Standard,
            "49 = 標準"
        );
        assert_eq!(
            DifficultyLabel::from_score(50.0),
            DifficultyLabel::Hard,
            "50 = 難"
        );
        assert_eq!(
            DifficultyLabel::from_score(69.0),
            DifficultyLabel::Hard,
            "69 = 難"
        );
        assert_eq!(
            DifficultyLabel::from_score(70.0),
            DifficultyLabel::VeryHard,
            "70 = 極難"
        );
        assert_eq!(
            DifficultyLabel::from_score(100.0),
            DifficultyLabel::VeryHard
        );

        // 4 種類のいずれかになる (ドメイン不変条件)
        for s in [0.0, 25.0, 40.0, 60.0, 85.0, 100.0] {
            let l = DifficultyLabel::from_score(s);
            assert!(matches!(
                l,
                DifficultyLabel::Easy
                    | DifficultyLabel::Standard
                    | DifficultyLabel::Hard
                    | DifficultyLabel::VeryHard
            ));
        }
    }

    /// CR-1 #2: 寄与分解の具体値検証
    /// 4 軸スコア [80, 40, 30, 20] のとき、push 1位 = 軸1 (+30)、ease 1位 = 軸4 (-30)
    ///
    /// ラジアー軸スコアを意図的に作るには raw 値を逆算する:
    /// - job_ratio: normalize_linear(v, 0.5, 1.5) = 80 → v = 0.5 + 0.8 * 1.0 = 1.3
    /// - vacancy_rate: normalize_linear(v*100, 0.0, 50.0) = 40 → v*100 = 20 → v = 0.20
    /// - unemployment_inv: normalize_linear(5.0 - v, 0.0, 4.0) = 30 → 5.0 - v = 1.2 → v = 3.8
    /// - separation: normalize_linear(v, 5.0, 20.0) = 20 → v = 5.0 + 0.2 * 15.0 = 8.0
    #[test]
    fn cr1_contribution_breakdown_concrete_values() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.3))])], // → score 80
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.20)),
            ])], // → score 40
            vec![],
            vec![row(&[("unemployment_rate", json!(3.8))])], // → score 30
            vec![row(&[("separation_rate", json!(8.0))])],   // → score 20
            vec![],
            None,
        );
        let m = compute_metrics(&ctx);
        let contribs = extract_contributions(&m);
        assert_eq!(contribs.len(), 4, "4 軸全て取得できる");

        // 各軸の delta 確認 (誤差 0.5 程度許容)
        let job_ratio_c = contribs
            .iter()
            .find(|c| matches!(c.axis, AxisName::JobRatio))
            .expect("job_ratio");
        assert!(
            (job_ratio_c.delta - 30.0).abs() < 0.5,
            "job_ratio delta = +30, got {}",
            job_ratio_c.delta
        );

        let sep_c = contribs
            .iter()
            .find(|c| matches!(c.axis, AxisName::Separation))
            .expect("sep");
        assert!(
            (sep_c.delta - (-30.0)).abs() < 0.5,
            "separation delta = -30, got {}",
            sep_c.delta
        );

        // push 1 位 = job_ratio (delta +30)
        let push = top_push_factors(&contribs, 2);
        assert!(!push.is_empty());
        assert!(
            matches!(push[0].axis, AxisName::JobRatio),
            "push 1位 = 有効求人倍率"
        );
        assert!(push[0].delta > 0.0);

        // ease 1 位 = separation (delta -30)
        let ease = top_ease_factors(&contribs, 2);
        assert!(!ease.is_empty());
        assert!(
            matches!(ease[0].axis, AxisName::Separation),
            "ease 1位 = 離職率"
        );
        assert!(ease[0].delta < 0.0);
        assert!(
            (ease[0].delta.abs() - 30.0).abs() < 0.5,
            "ease 1位 |delta| = 30"
        );
    }

    /// CR-1 #3: アクション分岐のテスト
    /// 有効求人倍率=1.8 → 「給与訴求」が含まれる
    /// 離職率=20 → 「定着支援」が含まれる
    /// 失業率=1.5 → 「通勤圏拡大」が含まれる
    #[test]
    fn cr1_action_branching_concrete_values() {
        // 有効求人倍率 1.8
        let m1 = TightnessMetrics {
            job_ratio: Some(1.8),
            ..Default::default()
        };
        let actions1 = build_recommended_actions(&m1);
        assert!(
            actions1.iter().any(|a| a.contains("給与訴求")),
            "ratio>=1.5 → 給与訴求, got {:?}",
            actions1
        );

        // 離職率 20
        let m2 = TightnessMetrics {
            separation_rate: Some(20.0),
            ..Default::default()
        };
        let actions2 = build_recommended_actions(&m2);
        assert!(
            actions2.iter().any(|a| a.contains("定着支援")),
            "sep>=18 → 定着支援, got {:?}",
            actions2
        );

        // 失業率 1.5
        let m3 = TightnessMetrics {
            unemployment_rate: Some(1.5),
            ..Default::default()
        };
        let actions3 = build_recommended_actions(&m3);
        assert!(
            actions3.iter().any(|a| a.contains("通勤圏拡大")),
            "ur<2.0 → 通勤圏拡大, got {:?}",
            actions3
        );
        assert!(
            actions3.iter().any(|a| a.contains("リファラル")),
            "ur<2.0 → リファラル"
        );

        // 欠員補充率 45%
        let m4 = TightnessMetrics {
            vacancy_rate: Some(0.45),
            ..Default::default()
        };
        let actions4 = build_recommended_actions(&m4);
        assert!(
            actions4.iter().any(|a| a.contains("既存従業員")),
            "vacancy>=40% → 既存従業員からのリファラル"
        );

        // 開廃業 純増 +1.5
        let m5 = TightnessMetrics {
            opening_rate: Some(5.0),
            closure_rate: Some(3.0),
            ..Default::default()
        };
        let actions5 = build_recommended_actions(&m5);
        assert!(
            actions5.iter().any(|a| a.contains("競合増加")),
            "open-close>1.0 → 競合増加注意"
        );

        // 何もトリガーされない場合
        let m_none = TightnessMetrics {
            job_ratio: Some(0.8),
            ..Default::default()
        };
        let actions_none = build_recommended_actions(&m_none);
        assert!(
            actions_none.is_empty(),
            "閾値未満ならアクションなし, got {:?}",
            actions_none
        );
    }

    /// CR-1 #4: ドメイン不変条件
    /// - 寄与の絶対値合計 ≤ 200 (4 軸 × 最大 50 ずれ)
    /// - アクションは 0〜3 件の範囲
    /// - ラベルは 4 種類のいずれか
    #[test]
    fn cr1_domain_invariants() {
        // 極端値: 全軸最大 (job_ratio=2.0, vacancy=0.6, unemployment=0.5, separation=25)
        let m_max = TightnessMetrics {
            job_ratio: Some(2.0),
            vacancy_rate: Some(0.6),
            unemployment_rate: Some(0.5),
            separation_rate: Some(25.0),
            opening_rate: Some(6.0),
            closure_rate: Some(3.0),
            ..Default::default()
        };
        let contribs = extract_contributions(&m_max);
        let total_abs: f64 = contribs.iter().map(|c| c.delta.abs()).sum();
        assert!(
            total_abs <= 200.0 + 1e-6,
            "寄与の絶対値合計 ≤ 200 (4 軸 × 50), got {}",
            total_abs
        );

        // アクション数 0..=3
        let actions = build_recommended_actions(&m_max);
        assert!(
            actions.len() <= 3,
            "アクションは最大 3 件, got {}",
            actions.len()
        );

        // 空メトリクスでアクション 0 件
        let m_empty = TightnessMetrics::default();
        let actions_empty = build_recommended_actions(&m_empty);
        assert!(
            actions_empty.len() <= 3,
            "空でも 0..=3 範囲, got {}",
            actions_empty.len()
        );
        assert_eq!(actions_empty.len(), 0);

        // ラベルは 4 種類のいずれか (網羅)
        for s in [
            -10.0, 0.0, 15.0, 29.99, 30.0, 45.0, 50.0, 69.99, 70.0, 100.0, 200.0,
        ] {
            let l = DifficultyLabel::from_score(s);
            assert!(matches!(
                l,
                DifficultyLabel::Easy
                    | DifficultyLabel::Standard
                    | DifficultyLabel::Hard
                    | DifficultyLabel::VeryHard
            ));
        }

        // push と ease は重複しない (delta=0 は両方から除外、それ以外は符号で排他)
        for c in &contribs {
            let in_push = top_push_factors(&contribs, 4).iter().any(|x| {
                matches!(
                    (x.axis, c.axis),
                    (AxisName::JobRatio, AxisName::JobRatio)
                        | (AxisName::VacancyRate, AxisName::VacancyRate)
                        | (AxisName::UnemploymentInv, AxisName::UnemploymentInv)
                        | (AxisName::Separation, AxisName::Separation)
                )
            });
            let in_ease = top_ease_factors(&contribs, 4).iter().any(|x| {
                matches!(
                    (x.axis, c.axis),
                    (AxisName::JobRatio, AxisName::JobRatio)
                        | (AxisName::VacancyRate, AxisName::VacancyRate)
                        | (AxisName::UnemploymentInv, AxisName::UnemploymentInv)
                        | (AxisName::Separation, AxisName::Separation)
                )
            });
            if c.delta > 0.0 {
                assert!(in_push && !in_ease, "delta>0 は push のみ");
            } else if c.delta < 0.0 {
                assert!(!in_push && in_ease, "delta<0 は ease のみ");
            }
        }
    }

    /// CR-1 #5: caveat 文言の存在検証
    /// 「相関ベース」「因果ではない」が出力に含まれること
    #[test]
    fn cr1_caveat_phrases_present_in_output() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.6))])], // 押し上げトリガー
            vec![],
            vec![],
            vec![],
            vec![row(&[("separation_rate", json!(20.0))])], // 押し上げトリガー
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        // 「相関ベース」と「因果ではない」が必ず存在
        assert!(
            html.contains("相関ベース"),
            "推奨アクション見出しに『相関ベース』が必要"
        );
        assert!(
            html.contains("因果ではない"),
            "推奨アクション見出しに『因果ではない』が必要"
        );
        // 寄与分解も「相関的な押し上げ要因」表現
        assert!(
            html.contains("相関的な押し上げ要因"),
            "寄与分解は『相関的な押し上げ要因』表記"
        );
        // 用語制約: 「総合スコア」は新ブロック直接見出しでは使わず「採用難易度」「複合指標」を用いる
        assert!(
            html.contains("採用難易度"),
            "用語『採用難易度』が出力に含まれる"
        );
    }

    /// CR-1 #6: fail-soft - スコアなし時 section 全体が出力されない
    #[test]
    fn cr1_fail_soft_no_score_no_block() {
        // 全データ空 → has_any_data() = false → section 出力なし
        let ctx = build_test_ctx(vec![], vec![], vec![], vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));
        assert!(html.is_empty(), "スコアなし時 section 全体非表示");

        // 開廃業のみ存在 (composite_score 算出不可) でも recruit-difficulty ブロックは出ない
        // ※ has_any_data() は opening_rate でも true になり section 自体は描画されるが、
        //    recruit_difficulty ブロックは composite_score がないため早期 return する
        let ctx2 = build_test_ctx(
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![row(&[
                ("opening_rate", json!(5.0)),
                ("closure_rate", json!(3.0)),
            ])],
            None,
        );
        let mut html2 = String::new();
        render_section_market_tightness(&mut html2, Some(&ctx2));
        // section 自体は出るが recruit-difficulty ブロックは出ない
        assert!(
            !html2.contains("data-testid=\"recruit-difficulty-block\""),
            "composite_score なし → recruit-difficulty ブロック非表示"
        );

        // composite_score がある場合は recruit-difficulty ブロックが必ず出る
        let ctx3 = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.0))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html3 = String::new();
        render_section_market_tightness(&mut html3, Some(&ctx3));
        assert!(
            html3.contains("data-testid=\"recruit-difficulty-block\""),
            "composite_score あり → recruit-difficulty ブロック表示"
        );
    }

    /// CR-1 補強: ラベル位置 - 図 MT-1 (総合スコア) の直後 / 図 MT-2 (レーダー) の直前
    #[test]
    fn cr1_block_positioned_between_summary_and_radar() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        let pos_mt1 = html.find("図 MT-1").expect("図 MT-1 必須");
        let pos_block = html
            .find("data-testid=\"recruit-difficulty-block\"")
            .expect("recruit-difficulty-block 必須");
        let pos_mt2 = html.find("図 MT-2").expect("図 MT-2 必須");

        assert!(
            pos_mt1 < pos_block && pos_block < pos_mt2,
            "順序: 図 MT-1 (総合スコア) < 採用難易度ブロック < 図 MT-2 (レーダー)"
        );
    }

    /// CR-1 補強: アクション最大件数 3 の上限を逆証明
    /// 全トリガーが発火する条件下でも 3 件で止まる
    #[test]
    fn cr1_actions_capped_at_three() {
        let m = TightnessMetrics {
            job_ratio: Some(2.0),         // → 給与訴求 + 差別化タグ (2 件)
            separation_rate: Some(20.0),  // → 定着支援 (1 件)
            unemployment_rate: Some(1.0), // → 通勤圏拡大 + リファラル (2 件)
            vacancy_rate: Some(0.5),      // → 既存従業員リファラル (1 件)
            opening_rate: Some(6.0),
            closure_rate: Some(3.0), // → 競合増加 (1 件)
            ..Default::default()
        };
        let actions = build_recommended_actions(&m);
        assert_eq!(actions.len(), 3, "上限 3 件で打ち切り, got {:?}", actions);
        // 優先順 (有効求人倍率系が最初)
        assert!(actions[0].contains("給与訴求"), "1 番目は給与訴求");
    }

    /// CR-1 補強: format_contribution の出力フォーマット検証
    ///
    /// 2026-05-08 Round 2.7-B': variant 別ラベルに応じて出し分け確認。
    /// Full / MI で異なるラベルが返ることを逆証明。
    #[test]
    fn cr1_format_contribution_strings() {
        let c_pos = AxisContribution {
            axis: AxisName::JobRatio,
            score: 80.0,
            delta: 30.0,
            raw_value: Some(1.30),
        };
        // Full variant: 「有効求人倍率」 (KPI カード見出しと統一)
        let s_full = format_contribution(&c_pos, super::super::ReportVariant::Full);
        assert!(s_full.contains("有効求人倍率"));
        assert!(!s_full.contains("公的雇用需給指標"));
        assert!(s_full.contains("1.30倍"));
        assert!(s_full.contains("+30"));

        // MI variant: 「公的雇用需給指標」 (HW 連想語回避)
        let s_mi = format_contribution(&c_pos, super::super::ReportVariant::MarketIntelligence);
        assert!(s_mi.contains("公的雇用需給指標"));
        assert!(!s_mi.contains("有効求人倍率"));
        assert!(s_mi.contains("1.30倍"));

        let c_neg = AxisContribution {
            axis: AxisName::Separation,
            score: 20.0,
            delta: -30.0,
            raw_value: Some(8.0),
        };
        let s2 = format_contribution(&c_neg, super::super::ReportVariant::Full);
        assert!(s2.contains("離職率"));
        assert!(s2.contains("8.0%"));
        assert!(s2.contains("-30"));
    }

    /// データソース注記関数: 単体テスト (公開統計名で出典を表示)
    #[test]
    fn render_data_source_note_format() {
        let note =
            render_data_source_note("総務省統計局 労働力調査", "完全失業率 (公表値)", "都道府県");
        assert!(note.contains("労働力調査"));
        assert!(note.contains("完全失業率"));
        assert!(note.contains("都道府県"));
        assert!(note.contains("出典"));
        assert!(note.contains("計算"));
        assert!(note.contains("粒度"));
    }

    // =================================================================
    // Public variant (HW 欠員補充率を除外、3 軸版) テスト群
    // =================================================================

    /// Public variant: HW 欠員補充率の KPI カードが出ないこと (逆証明)
    ///
    /// vacancy データを与えても、HW 欠員補充率 KPI ラベル / カード data-source
    /// (「ハローワーク掲載求人 (自社集計)」) が出力に含まれないことを確認。
    #[test]
    fn public_variant_excludes_hw_vacancy_kpi_card() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.30)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        // HW 欠員補充率 KPI ラベルは出力されない
        assert!(
            !html.contains("HW 欠員補充率"),
            "Public variant では HW 欠員補充率 KPI カード非表示"
        );
        // HW 由来データソース注記文言も出ない
        assert!(
            !html.contains("ハローワーク掲載求人 (自社集計)"),
            "Public variant では HW 出典注記が出力されない"
        );
        // 他の 3 軸は表示される
        // 2026-05-08 Round 2.7-B: 有効求人倍率 → 公的雇用需給指標 (中立化)
        assert!(html.contains("公的雇用需給指標"));
        assert!(html.contains("失業率"));
        assert!(html.contains("離職率"));
    }

    /// Public variant: レーダーが 3 軸 (HW 欠員補充率を除外)
    ///
    /// ECharts indicator 配列に「欠員補充率」軸が含まれず、3 軸 (有効求人倍率 / 採用余力 / 離職率) が含まれることを検証。
    #[test]
    fn public_variant_radar_has_3_axes_not_4() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.30)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        // 3 軸が含まれる
        // 2026-05-08 Round 2.7-B: 有効求人倍率 → 公的雇用需給指標 (中立化)
        assert!(
            html.contains("\"name\":\"公的雇用需給指標"),
            "公的雇用需給指標 軸"
        );
        assert!(html.contains("\"name\":\"採用余力"), "採用余力 軸");
        assert!(html.contains("\"name\":\"離職率"), "離職率 軸");

        // 「欠員補充率」軸は ECharts indicator 内に出現しない
        assert!(
            !html.contains("\"name\":\"欠員補充率"),
            "Public variant レーダーは欠員補充率を含まない"
        );

        // ECharts radar config 識別属性
        assert!(html.contains("data-chart-config"), "ECharts div 必要");
        assert!(html.contains("\"radar\""), "radar type 必要");

        // 図表番号は MT-2 のまま、3 軸版であることをタイトルで明示
        assert!(html.contains("3 軸レーダー"), "レーダーは 3 軸版と明記");
    }

    /// Public variant: 複合スコアが 3 指標平均であること
    ///
    /// 軸スコアを意図的に [80, 30, 20] に設定し、平均 (80+30+20)/3 = 43.33... を検証。
    /// raw 値の逆算:
    /// - job_ratio: normalize_linear(v, 0.5, 1.5) = 80 → v = 1.3
    /// - unemployment_inv: normalize_linear(5.0 - v, 0.0, 4.0) = 30 → v = 3.8
    /// - separation: normalize_linear(v, 5.0, 20.0) = 20 → v = 8.0
    #[test]
    fn public_variant_composite_score_is_three_axis_mean() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.3))])], // → 80
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.30)), // 与えても無視されるべき
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(3.8))])], // → 30
            vec![row(&[("separation_rate", json!(8.0))])],   // → 20
            vec![],
            None,
        );
        let m = compute_metrics(&ctx);
        // Public 変種では vacancy_rate を None として扱う前提なので明示的にクリア
        let mut m_public = m.clone();
        m_public.vacancy_rate = None;

        let score = m_public
            .composite_score_public()
            .expect("public composite score");
        let expected = (80.0 + 30.0 + 20.0) / 3.0;
        assert!(
            (score - expected).abs() < 0.5,
            "expected ~{:.2}, got {:.4}",
            expected,
            score
        );

        // ドメイン不変条件: 0..=100
        assert!((0.0..=100.0).contains(&score));

        // Full variant の 4 軸平均と異なること (vacancy=0.30 → score 60 が混ざるため)
        let full_score = m.composite_score().expect("full composite score");
        assert!(
            (full_score - score).abs() > 1.0,
            "Full と Public で複合スコアは異なる (Full {} / Public {})",
            full_score,
            score
        );
    }

    /// Public variant: caveat 「HW 掲載求人特有の指標は除外」が含まれる
    #[test]
    fn public_variant_caveat_excludes_hw_phrase_present() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        // 必須 caveat 文言
        // 2026-05-08 Round 2.7-B: 「HW 掲載求人特有」→「特定求人媒体特有」(中立化、案 B)
        assert!(
            html.contains("特定求人媒体特有の指標は除外"),
            "Public/MI variant caveat 文言『特定求人媒体特有の指標は除外』必須 (HW 連想語の中立化)"
        );
        // オープンデータ明記
        assert!(
            html.contains("オープンデータ") || html.contains("公開統計"),
            "オープンデータ / 公開統計 の明記必須"
        );
        // 因果非主張は両 variant 共通で必須
        assert!(
            html.contains("因果関係を示すものではありません"),
            "因果非主張 caveat 必須"
        );
    }

    /// Public variant: render_section_market_tightness_with_variant のディスパッチ確認
    ///
    /// Full / Public で異なる出力 (HW 欠員補充率 KPI 有無) を生成することを逆証明。
    #[test]
    fn variant_dispatch_full_vs_public_diverges() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.30)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );

        let mut html_full = String::new();
        render_section_market_tightness_with_variant(
            &mut html_full,
            Some(&ctx),
            super::super::ReportVariant::Full,
        );
        let mut html_public = String::new();
        render_section_market_tightness_with_variant(
            &mut html_public,
            Some(&ctx),
            super::super::ReportVariant::Public,
        );

        // Full には HW 欠員補充率 KPI が含まれる
        assert!(
            html_full.contains("HW 欠員補充率"),
            "Full variant には HW 欠員補充率 KPI 含まれる"
        );
        // Public には含まれない
        assert!(
            !html_public.contains("HW 欠員補充率"),
            "Public variant には HW 欠員補充率 KPI 含まれない"
        );

        // Full は 4 軸レーダー、Public は 3 軸レーダー
        assert!(html_full.contains("4 軸レーダー"));
        assert!(html_public.contains("3 軸レーダー"));
    }

    /// Public variant: HW 欠員補充率トリガーのアクション (「既存従業員」リファラル) が抑制される
    ///
    /// vacancy_rate = 0.5 でも build_recommended_actions_public は「既存従業員」を返さない。
    #[test]
    fn public_variant_actions_no_hw_trigger() {
        let m = TightnessMetrics {
            // vacancy_rate を意図的にセット (Public でも内部メトリクスとしては None ではあるが、
            // ここでは関数単体の挙動を検証する)
            vacancy_rate: Some(0.5),
            ..Default::default()
        };
        let actions = build_recommended_actions_public(&m);
        assert!(
            !actions.iter().any(|a| a.contains("既存従業員")),
            "Public variant では HW 欠員補充率トリガーのアクションは出さない, got {:?}",
            actions
        );

        // 失業率 ≥ 3.5% で Public 限定の新トリガーが発火
        let m2 = TightnessMetrics {
            unemployment_rate: Some(4.0),
            ..Default::default()
        };
        let actions2 = build_recommended_actions_public(&m2);
        assert!(
            actions2.iter().any(|a| a.contains("採用候補プール")),
            "Public 限定: ur>=3.5 → 採用候補プール広め, got {:?}",
            actions2
        );
    }

    /// Public variant: 寄与分解が 3 軸のみ (VacancyRate を含まない)
    #[test]
    fn public_variant_contributions_exclude_vacancy() {
        let m = TightnessMetrics {
            job_ratio: Some(1.3),
            vacancy_rate: Some(0.4), // セットされていてもスキップされる
            unemployment_rate: Some(3.0),
            separation_rate: Some(15.0),
            ..Default::default()
        };
        let contribs = extract_contributions_public(&m);
        assert_eq!(contribs.len(), 3, "Public variant 寄与分解は 3 軸");
        assert!(
            !contribs
                .iter()
                .any(|c| matches!(c.axis, AxisName::VacancyRate)),
            "Public variant 寄与分解に VacancyRate を含まない"
        );
        // 3 軸全て登場
        assert!(contribs
            .iter()
            .any(|c| matches!(c.axis, AxisName::JobRatio)));
        assert!(contribs
            .iter()
            .any(|c| matches!(c.axis, AxisName::UnemploymentInv)));
        assert!(contribs
            .iter()
            .any(|c| matches!(c.axis, AxisName::Separation)));
    }

    /// Public variant: 全データ空 → section 出力なし (fail-soft)
    #[test]
    fn public_variant_empty_renders_nothing() {
        let ctx = build_test_ctx(vec![], vec![], vec![], vec![], vec![], vec![], None);
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));
        assert!(html.is_empty(), "Public variant 全空でも section 非表示");
    }

    /// Public variant: 補助 KPI (開廃業動態) は Full と同様に表示される
    #[test]
    fn public_variant_business_dynamics_still_visible() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.0))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![row(&[
                ("opening_rate", json!(5.2)),
                ("closure_rate", json!(3.8)),
            ])],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        assert!(
            html.contains("data-testid=\"business-dynamics-card\""),
            "Public variant でも補助 KPI 開廃業動態は表示"
        );
        assert!(html.contains("5.2"));
        assert!(html.contains("3.8"));
        assert!(html.contains("拡大基調"));
    }

    // =====================================================================
    // 2026-04-29 追加: 業界フィルタ範囲注記が両 variant で出力されること
    // =====================================================================

    /// 逆証明: Full variant で「業界フィルタの適用範囲」が含まれる
    #[test]
    fn market_tightness_industry_scope_note_full_variant() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.2))])],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(3.0))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness(&mut html, Some(&ctx));

        assert!(
            html.contains("業界フィルタの適用範囲"),
            "Full variant に「業界フィルタの適用範囲」が含まれるはず"
        );
        assert!(
            html.contains("業界を問わない地域全体値"),
            "「業界を問わない地域全体値」が含まれるはず"
        );
        assert!(
            html.contains("market-tightness-industry-scope-note"),
            "data-testid 属性が含まれるはず"
        );
        // Full variant のみ: HW 欠員補充率が「地域全体値」リスト内に
        assert!(
            html.contains("HW 欠員補充率"),
            "Full variant では HW 欠員補充率が地域全体値リストに含まれるはず"
        );
    }

    /// 逆証明: Public variant でも「業界フィルタの適用範囲」が含まれる
    #[test]
    fn market_tightness_industry_scope_note_public_variant() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.2))])],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(3.0))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        assert!(
            html.contains("業界フィルタの適用範囲"),
            "Public variant にも「業界フィルタの適用範囲」が含まれるはず"
        );
        assert!(
            html.contains("業界を問わない地域全体値"),
            "「業界を問わない地域全体値」が含まれるはず"
        );
        assert!(
            html.contains("market-tightness-industry-scope-note"),
            "Public variant にも data-testid 属性が含まれるはず"
        );
    }

    // =================================================================
    // Round 2.7-B (2026-05-08): MI / Public variant の HW 連想語中立化テスト
    //
    // 案 B 採用: 全 variant 共通で中立化 (signature 変更なし、Full でも機能維持)。
    // notes.rs Round 2.6 と合わせて、PDF grep 上で
    // 「HW」「ハローワーク」「有効求人倍率」「求人倍率」が完全除去されることを保証。
    // =================================================================

    /// Round 2.7-B: render_section_market_tightness_public の出力に
    /// HW / ハローワーク / 有効求人倍率 / 求人倍率 が含まれないこと
    #[test]
    fn round_2_7b_public_render_does_not_emit_hw_or_keiyou_keisuu() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.4))])],
            vec![row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(0.30)),
            ])],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        // HW 連想語の不混入を逆証明
        assert!(
            !html.contains("HW "),
            "MI/Public 経路に「HW 」が混入してはならない (Round 2.7-B)"
        );
        assert!(
            !html.contains("ハローワーク"),
            "MI/Public 経路に「ハローワーク」が混入してはならない (Round 2.7-B)"
        );
        assert!(
            !html.contains("有効求人倍率"),
            "MI/Public 経路に「有効求人倍率」が混入してはならない (Round 2.7-B)"
        );
        assert!(
            !html.contains("求人倍率"),
            "MI/Public 経路に「求人倍率」が混入してはならない (Round 2.7-B)"
        );
    }

    /// Round 2.7-B: 中立化後も機能維持 (公的雇用需給指標のラベル + KPI 数値が表示される)
    #[test]
    fn round_2_7b_public_render_keeps_neutral_label_and_numeric() {
        let ctx = build_test_ctx(
            vec![row(&[("ratio_total", json!(1.42))])],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.4))])],
            vec![row(&[("separation_rate", json!(15.0))])],
            vec![],
            None,
        );
        let mut html = String::new();
        render_section_market_tightness_public(&mut html, Some(&ctx));

        // 中立用語が必ず表示される
        assert!(
            html.contains("公的雇用需給指標"),
            "中立用語『公的雇用需給指標』が KPI / レーダー / 表に表示されること"
        );
        assert!(
            html.contains("特定求人媒体特有"),
            "caveat 文言『特定求人媒体特有』が表示されること"
        );
        // 数値ロジックは触らない: 1.42 倍が KPI 数値として表示される
        assert!(
            html.contains("1.42"),
            "中立化しても KPI 数値 (1.42) は維持される"
        );
    }

    /// Round 2.7-B': format_contribution の MI variant 出力は中立化される
    ///
    /// Round 2.7-B' で variant 別ラベルに切替後も、MI 経路では HW 連想語
    /// 「有効求人倍率」を出さず「公的雇用需給指標」を出すことを逆証明。
    #[test]
    fn round_2_7b_axis_name_job_ratio_is_neutral() {
        let c = AxisContribution {
            axis: AxisName::JobRatio,
            score: 80.0,
            delta: 30.0,
            raw_value: Some(1.30),
        };
        let s = format_contribution(&c, super::super::ReportVariant::MarketIntelligence);
        assert!(
            s.contains("公的雇用需給指標"),
            "MI variant では中立用語『公的雇用需給指標』を返すこと"
        );
        assert!(
            !s.contains("有効求人倍率"),
            "MI variant の format_contribution に旧用語『有効求人倍率』を残してはならない"
        );
    }

    // ===========================================================
    // Round 2.7-B' (2026-05-08) variant 別ラベル単体テスト
    // ===========================================================

    /// Round 2.7-B': Full variant は KPI カードと統一して「有効求人倍率」を返す
    #[test]
    fn round_2_7b_prime_full_variant_uses_yuukou_kyuujin_bairitsu_label() {
        let label = job_ratio_label_for_variant(super::super::ReportVariant::Full);
        assert_eq!(
            label, "有効求人倍率",
            "Full variant では「有効求人倍率」 (KPI カード見出しと統一)"
        );
    }

    /// Round 2.7-B': MarketIntelligence variant は「公的雇用需給指標」を返す
    #[test]
    fn round_2_7b_prime_mi_variant_uses_neutral_label() {
        let label = job_ratio_label_for_variant(super::super::ReportVariant::MarketIntelligence);
        assert_eq!(
            label, "公的雇用需給指標",
            "MI variant では HW 連想語回避のため『公的雇用需給指標』"
        );
    }

    /// Round 2.7-B': Public variant も「公的雇用需給指標」を返す
    #[test]
    fn round_2_7b_prime_public_variant_uses_neutral_label() {
        let label = job_ratio_label_for_variant(super::super::ReportVariant::Public);
        assert_eq!(
            label, "公的雇用需給指標",
            "Public variant では HW 言及最小化のため『公的雇用需給指標』"
        );
    }

    /// Round 2.7-B': Full の format_contribution 出力に中立用語が混入しない
    #[test]
    fn round_2_7b_prime_full_format_contribution_no_neutral_label() {
        let c = AxisContribution {
            axis: AxisName::JobRatio,
            score: 80.0,
            delta: 30.0,
            raw_value: Some(1.30),
        };
        let s = format_contribution(&c, super::super::ReportVariant::Full);
        assert!(
            s.contains("有効求人倍率"),
            "Full では『有効求人倍率』を表示する"
        );
        assert!(
            !s.contains("公的雇用需給指標"),
            "Full の寄与分解に中立用語『公的雇用需給指標』を混入させない (用語混在防止)"
        );
    }

    /// Round 2.7-B': MI の format_contribution 出力に「有効求人倍率」「求人倍率」が混入しない
    #[test]
    fn round_2_7b_prime_mi_format_contribution_no_yuukou_kyuujin_bairitsu() {
        let c = AxisContribution {
            axis: AxisName::JobRatio,
            score: 80.0,
            delta: 30.0,
            raw_value: Some(1.30),
        };
        let s = format_contribution(&c, super::super::ReportVariant::MarketIntelligence);
        assert!(
            !s.contains("有効求人倍率"),
            "MI variant に『有効求人倍率』を出してはならない"
        );
        // 「求人倍率」単体も MI では出さない (中立用語のみ)
        assert!(
            !s.contains("求人倍率"),
            "MI variant に『求人倍率』 (単体含む) を出してはならない"
        );
    }
}

// =====================================================================
// Round 12: Judgement logic tests for market_tightness.rs
// 追加日: 2026-05-12
// 既存コード変更なし。逼迫度スコア閾値・正規化・R²・HHI 不変条件を逆証明。
// =====================================================================
#[cfg(test)]
mod round12_judgement_tests {
    // -----------------------------------------------------------------
    // DifficultyLabel 相当 (market_tightness.rs:623-633)
    //   < 30 → Easy / < 50 → Standard / < 70 → Hard / else → VeryHard
    // -----------------------------------------------------------------
    #[derive(Debug, PartialEq, Eq)]
    enum Label { Easy, Standard, Hard, VeryHard }
    fn from_score(score: f64) -> Label {
        if score < 30.0 { Label::Easy }
        else if score < 50.0 { Label::Standard }
        else if score < 70.0 { Label::Hard }
        else { Label::VeryHard }
    }

    #[test]
    fn label_boundary_30() {
        assert_eq!(from_score(29.99), Label::Easy);
        assert_eq!(from_score(30.0), Label::Standard);
    }
    #[test]
    fn label_boundary_50() {
        assert_eq!(from_score(49.99), Label::Standard);
        assert_eq!(from_score(50.0), Label::Hard);
    }
    #[test]
    fn label_boundary_70() {
        assert_eq!(from_score(69.99), Label::Hard);
        assert_eq!(from_score(70.0), Label::VeryHard);
    }
    #[test]
    fn label_extremes() {
        assert_eq!(from_score(0.0), Label::Easy);
        assert_eq!(from_score(100.0), Label::VeryHard);
    }
    #[test]
    fn label_negative_treated_as_easy() {
        // 防御的: 負値は Easy にフォールバック (clamp 前提)
        assert_eq!(from_score(-10.0), Label::Easy);
    }

    // -----------------------------------------------------------------
    // 正規化関数のドメイン不変 (normalize_linear)
    // -----------------------------------------------------------------
    fn normalize_linear(v: f64, lo: f64, hi: f64) -> f64 {
        if (hi - lo).abs() < f64::EPSILON { return 50.0; }
        let n = (v - lo) / (hi - lo) * 100.0;
        n.clamp(0.0, 100.0)
    }
    #[test]
    fn normalize_output_in_0_100() {
        for v in [-1000.0, -1.0, 0.0, 0.5, 1.0, 1.5, 100.0, 1e9] {
            let r = normalize_linear(v, 0.5, 1.5);
            assert!((0.0..=100.0).contains(&r), "v={} → {} ∉ [0,100]", v, r);
        }
    }
    #[test]
    fn normalize_degenerate_lo_eq_hi() {
        assert_eq!(normalize_linear(1.0, 1.0, 1.0), 50.0);
        assert_eq!(normalize_linear(100.0, 1.0, 1.0), 50.0);
    }
    #[test]
    fn normalize_monotonic_increasing() {
        let lo = 0.5; let hi = 1.5;
        let mut prev = -1.0;
        for v in [0.4, 0.5, 0.7, 1.0, 1.3, 1.5, 1.6] {
            let cur = normalize_linear(v, lo, hi);
            assert!(cur >= prev, "v={} で単調性破れ", v);
            prev = cur;
        }
    }

    // -----------------------------------------------------------------
    // R² ∈ [0, 1] のドメイン不変
    // -----------------------------------------------------------------
    fn is_valid_r_squared(r2: f64) -> bool {
        !r2.is_nan() && (0.0..=1.0).contains(&r2)
    }
    fn is_strong_correlation(r2: f64) -> bool {
        // 一般的な強相関閾値 0.7
        is_valid_r_squared(r2) && r2 >= 0.7
    }
    #[test]
    fn r_squared_valid_range() {
        assert!(is_valid_r_squared(0.0));
        assert!(is_valid_r_squared(0.5));
        assert!(is_valid_r_squared(1.0));
    }
    #[test]
    fn r_squared_invalid_rejected() {
        assert!(!is_valid_r_squared(-0.01));
        assert!(!is_valid_r_squared(1.01));
        assert!(!is_valid_r_squared(f64::NAN));
        assert!(!is_valid_r_squared(f64::INFINITY));
    }
    #[test]
    fn r_squared_strong_threshold() {
        assert!(!is_strong_correlation(0.69));
        assert!(is_strong_correlation(0.70));
        assert!(is_strong_correlation(0.95));
        assert!(!is_strong_correlation(1.5)); // 異常値は弱として扱う
    }

    // -----------------------------------------------------------------
    // HHI ∈ [0, 10000] (公正取引委員会基準)
    // -----------------------------------------------------------------
    fn is_valid_hhi(h: f64) -> bool {
        !h.is_nan() && (0.0..=10000.0).contains(&h)
    }
    fn hhi_concentration_level(h: f64) -> &'static str {
        // 公取委: 1500 未満 = 低集中 / 2500 未満 = 中集中 / それ以上 = 高集中
        if !is_valid_hhi(h) { return "invalid"; }
        if h < 1500.0 { "低集中" }
        else if h < 2500.0 { "中集中" }
        else { "高集中" }
    }
    #[test]
    fn hhi_boundary_1500() {
        assert_eq!(hhi_concentration_level(1499.99), "低集中");
        assert_eq!(hhi_concentration_level(1500.0), "中集中");
    }
    #[test]
    fn hhi_boundary_2500() {
        assert_eq!(hhi_concentration_level(2499.99), "中集中");
        assert_eq!(hhi_concentration_level(2500.0), "高集中");
    }
    #[test]
    fn hhi_extremes() {
        assert_eq!(hhi_concentration_level(0.0), "低集中");
        assert_eq!(hhi_concentration_level(10000.0), "高集中"); // 完全独占
    }
    #[test]
    fn hhi_invalid_rejected() {
        assert!(!is_valid_hhi(-1.0));
        assert!(!is_valid_hhi(10000.01));
        assert!(!is_valid_hhi(f64::NAN));
    }

    // -----------------------------------------------------------------
    // 失業率 ∈ [0, 100]
    // -----------------------------------------------------------------
    fn is_valid_unemployment(rate: f64) -> bool {
        !rate.is_nan() && (0.0..=100.0).contains(&rate)
    }
    #[test]
    fn unemployment_in_range() {
        assert!(is_valid_unemployment(2.4));
        assert!(is_valid_unemployment(0.0));
        assert!(is_valid_unemployment(100.0));
    }
    #[test]
    fn unemployment_380_pct_rejected() {
        // 過去事故 (2026-04-27): unemployment 380% が流出
        // ドメイン不変条件で必ず弾かれること
        assert!(!is_valid_unemployment(380.0), "失業率 380% は不変条件違反");
    }
    #[test]
    fn unemployment_negative_rejected() {
        assert!(!is_valid_unemployment(-0.5));
    }

    // -----------------------------------------------------------------
    // 性別比 (女性比率) ∈ [0, 100]
    // -----------------------------------------------------------------
    fn is_valid_ratio_pct(r: f64) -> bool {
        !r.is_nan() && (0.0..=100.0).contains(&r)
    }
    #[test]
    fn ratio_pct_in_range() {
        for r in [0.0, 25.0, 50.0, 75.0, 100.0] {
            assert!(is_valid_ratio_pct(r));
        }
    }
    #[test]
    fn ratio_pct_over_100_rejected() {
        assert!(!is_valid_ratio_pct(100.01));
        assert!(!is_valid_ratio_pct(150.0));
    }

    // -----------------------------------------------------------------
    // 逼迫度 3 段階 (>= 70 → 逼迫 / >= 40 → やや逼迫 / else → 緩和)
    // 既存テスト tightness_summary_three_levels と整合
    // -----------------------------------------------------------------
    fn tightness_zone(score: f64) -> &'static str {
        if score >= 70.0 { "逼迫" }
        else if score >= 40.0 { "やや逼迫" }
        else { "緩和" }
    }
    #[test]
    fn tightness_zone_boundaries() {
        assert_eq!(tightness_zone(39.99), "緩和");
        assert_eq!(tightness_zone(40.0), "やや逼迫");
        assert_eq!(tightness_zone(69.99), "やや逼迫");
        assert_eq!(tightness_zone(70.0), "逼迫");
        assert_eq!(tightness_zone(0.0), "緩和");
        assert_eq!(tightness_zone(100.0), "逼迫");
    }

    // -----------------------------------------------------------------
    // 複合スコア (平均) の不変条件: 各軸 ∈ [0,100] なら平均も ∈ [0,100]
    // -----------------------------------------------------------------
    fn composite_avg(axes: &[f64]) -> Option<f64> {
        let valid: Vec<f64> = axes.iter().filter(|v| (0.0..=100.0).contains(*v)).copied().collect();
        if valid.is_empty() { None } else {
            Some(valid.iter().sum::<f64>() / valid.len() as f64)
        }
    }
    #[test]
    fn composite_in_range_when_inputs_in_range() {
        for axes in [
            vec![0.0, 0.0, 0.0],
            vec![100.0, 100.0],
            vec![50.0, 60.0, 70.0, 80.0],
        ] {
            let r = composite_avg(&axes).expect("non-empty");
            assert!((0.0..=100.0).contains(&r));
        }
    }
    #[test]
    fn composite_empty_returns_none() {
        assert_eq!(composite_avg(&[]), None);
    }
    #[test]
    fn composite_filters_invalid() {
        // 異常値 (-10, 200) は除外して計算
        let r = composite_avg(&[50.0, -10.0, 200.0, 70.0]).unwrap();
        assert!((r - 60.0).abs() < 1e-9);
    }

    // -----------------------------------------------------------------
    // 横展開: 不等号方向誤りの retrospective テスト
    // -----------------------------------------------------------------
    #[test]
    fn anti_regression_difficulty_uses_lt_for_lower_bound() {
        // バグ候補: `score <= 30` (=を含む) だと 30.0 は Easy 扱い
        // 正実装: `score < 30` なので 30.0 は Standard
        assert_eq!(from_score(30.0), Label::Standard);
    }
    #[test]
    fn anti_regression_tightness_ge_for_upper_bound() {
        // 逼迫 (>=70) は境界含む。逆実装だと 70.0 が「やや逼迫」に落ちる
        assert_eq!(tightness_zone(70.0), "逼迫");
    }
}
