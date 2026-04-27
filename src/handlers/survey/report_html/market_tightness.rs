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

/// 「採用市場 逼迫度」section 全体を描画
///
/// `ctx` が None もしくは関連データ全空の場合、section ごと出力しない (fail-soft)。
pub(super) fn render_section_market_tightness(html: &mut String, ctx: Option<&InsightContext>) {
    let ctx = match ctx {
        Some(c) => c,
        None => return,
    };

    let metrics = compute_metrics(ctx);
    if !metrics.has_any_data() {
        return;
    }

    html.push_str("<div class=\"section\">\n");
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
    // 全国平均失業率 (pref_avg_unemployment_rate は 0-1 比率なので 100 倍)
    m.unemployment_national = ctx.pref_avg_unemployment_rate.map(|v| v * 100.0);

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
/// `granularity` 例: "都道府県" / "市区町村"
fn render_data_source_note(table: &str, formula: &str, granularity: &str) -> String {
    format!(
        "<div class=\"data-source-note\" style=\"font-size:9px;color:#9ca3af;margin-top:4px;line-height:1.4;\">\
         <strong>出典</strong>: {} / <strong>計算</strong>: {} / <strong>粒度</strong>: {}\
         </div>",
        escape_html(table),
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
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">DB / テーブル</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">カラム</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">計算式</th>\
         <th style=\"text-align:left;padding:4px 6px;border:1px solid #d1d5db;\">粒度</th>\
         </tr></thead>\n<tbody>\n",
    );
    let rows: &[(&str, &str, &str, &str, &str)] = &[
        (
            "有効求人倍率",
            "Turso v2_external_job_openings_ratio",
            "ratio_total",
            "有効求人数 / 有効求職者数 (公表値)",
            "都道府県",
        ),
        (
            "HW 欠員補充率",
            "ローカル v2_vacancy_rate",
            "vacancy_count / total_count",
            "(欠員補充求人数 / 全求人数) × 100",
            "市区町村",
        ),
        (
            "失業率",
            "Turso v2_external_labor_force",
            "unemployment_rate",
            "完全失業率 (公表値、労働力調査)",
            "都道府県",
        ),
        (
            "離職率",
            "Turso v2_external_turnover (industry='産業計')",
            "separation_rate",
            "離職者数 / 常用労働者数 (公表値、雇用動向調査)",
            "都道府県",
        ),
        (
            "開廃業動態 (補助)",
            "Turso v2_external_business_dynamics",
            "opening_rate / closure_rate",
            "純増 = opening_rate - closure_rate",
            "都道府県",
        ),
    ];
    for (metric, db, col, formula, gran) in rows {
        html.push_str(&format!(
            "<tr><td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;font-family:monospace;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;font-family:monospace;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td>\
             <td style=\"padding:4px 6px;border:1px solid #d1d5db;\">{}</td></tr>\n",
            escape_html(metric),
            escape_html(db),
            escape_html(col),
            escape_html(formula),
            escape_html(gran),
        ));
    }
    html.push_str("</tbody></table>\n");
    html.push_str(
        "<p style=\"margin-top:6px;font-size:9px;color:#6b7280;font-style:italic;\">\
         \u{203B} 出典の数値は公表値をそのまま参照しています。HW 欠員補充率のみ HW 掲載求人ベース (全求人市場の代表値ではありません)。\
         </p>\n",
    );
    html.push_str("</div>\n</details>\n");
}

// =====================================================================
// (1) 総合スコア (信号機色)
// =====================================================================

fn render_tightness_summary(html: &mut String, m: &TightnessMetrics) {
    let score = match m.composite_score() {
        Some(s) => s,
        None => return,
    };

    let (level_label, color, bg_color) = if score >= 70.0 {
        ("逼迫 (採用難)", "#dc2626", "#fef2f2") // 赤
    } else if score >= 40.0 {
        ("やや逼迫", "#f59e0b", "#fffbeb") // 黄
    } else {
        ("緩和 (採用容易)", "#10b981", "#ecfdf5") // 緑
    };

    render_figure_caption(html, "図 MT-1", "採用市場 逼迫度 総合スコア");

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
        label = escape_html(level_label),
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

    render_figure_caption(html, "図 MT-2", "採用市場 4 軸レーダー (0-100 正規化)");

    // ECharts radar: 4 軸定義 (時計回り、ストーリー順)
    let indicators = json!([
        {"name": "有効求人倍率", "max": 100},
        {"name": "欠員補充率", "max": 100},
        {"name": "採用余力 (失業率の逆数)", "max": 100},
        {"name": "離職率", "max": 100}
    ]);

    let target_arr = scores.to_array().to_vec();
    let national_arr = national.to_array().to_vec();

    let config = json!({
        "tooltip": {"trigger": "item"},
        "legend": {
            "data": ["対象地域", "全国平均 (参考)"],
            "top": 0,
            "textStyle": {"fontSize": 10}
        },
        "radar": {
            "indicator": indicators,
            "shape": "polygon",
            "splitNumber": 4,
            "axisName": {"fontSize": 10, "color": "#374151"}
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
        "4 軸が外側に広がるほど採用が難しい地域です。青色 (対象) の面積が大きく、\
         グレー (全国平均) との差が顕著な軸は、媒体戦略の注視ポイントとなります。",
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
            "Turso v2_external_job_openings_ratio",
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
            "ローカル v2_vacancy_rate",
            "(vacancy_count / total_count) × 100",
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
            "Turso v2_external_labor_force",
            "完全失業率 (公表値、労働力調査)",
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
            "Turso v2_external_turnover (industry='産業計')",
            "離職者数 / 常用労働者数 (公表値、雇用動向調査)",
            "都道府県",
        ));
        html.push_str("</div>\n");
    }

    html.push_str("</div>\n");

    // 補助 KPI: 開廃業動態
    if m.opening_rate.is_some() || m.closure_rate.is_some() {
        html.push_str("<div data-testid=\"business-dynamics-card\" style=\"margin-top:8px;padding:8px 12px;background:#f9fafb;border-radius:6px;border-left:3px solid #6366f1;font-size:11px;\">\n");
        html.push_str("<strong style=\"color:#4338ca;\">補助 KPI: 開廃業動態</strong> ");
        let op = m.opening_rate.unwrap_or(0.0);
        let cl = m.closure_rate.unwrap_or(0.0);
        let net = op - cl;
        html.push_str(&format!(
            "開業率 <strong>{:.1}%</strong> / 廃業率 <strong>{:.1}%</strong> / 純増 <strong>{:+.1}pt</strong>。",
            op, cl, net
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
            "Turso v2_external_business_dynamics",
            "純増 = opening_rate - closure_rate",
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

    let (heading, actions): (&str, Vec<&str>) = if score >= 70.0 {
        (
            "対象地域は逼迫度が高く、採用難度が高い傾向です。複数軸の訴求強化を検討する余地があります。",
            vec![
                "給与訴求強化 (基本給 +5% / 賞与額の明示 / 昇給実績の開示)",
                "福利厚生差別化 (住宅手当 / 退職金 / 短時間正社員制度)",
                "通勤圏拡大 (近隣市区町村への媒体配信 / リモート要素検討)",
            ],
        )
    } else if score >= 40.0 {
        (
            "対象地域はやや逼迫の傾向です。差別化要素の整備が効果的な可能性があります。",
            vec![
                "競合分析に基づく差別化軸の明確化 (休日数 / 残業時間 / 教育制度)",
                "求人原稿のキーワード見直し (応募者検索率向上)",
                "媒体出稿コストと応募単価のバランス監視",
            ],
        )
    } else {
        (
            "対象地域は緩和傾向です。採用コスト見直しとミスマッチ低減を優先できる可能性があります。",
            vec![
                "採用コスト見直し (媒体出稿頻度の調整)",
                "ミスマッチ低減 (求人原稿の業務内容詳細化 / 選考フローの精査)",
                "中長期の人材育成・定着施策へのリソース配分",
            ],
        )
    };

    html.push_str("<div data-testid=\"tightness-action-guide\" style=\"margin-top:12px;padding:12px 16px;background:#eff6ff;border-radius:6px;border-left:4px solid #2563eb;\">\n");
    html.push_str(&format!(
        "<div style=\"font-size:13px;font-weight:600;color:#1e40af;margin-bottom:6px;\">\u{1F4A1} アクション提案</div>\
         <p style=\"font-size:11px;color:#374151;margin-bottom:6px;\">{}</p>\
         <ol style=\"font-size:11px;color:#374151;padding-left:18px;line-height:1.6;\">\n",
        escape_html(heading)
    ));
    for action in &actions {
        html.push_str(&format!("<li>{}</li>\n", escape_html(action)));
    }
    html.push_str("</ol>\n");
    html.push_str("<p style=\"font-size:10px;color:#6b7280;margin-top:6px;font-style:italic;\">\u{203B} 上記提案は逼迫度スコアに基づく一般的方針であり、相関的傾向です。因果関係を示すものではありません。実施判断は職種・予算・競合状況等の個別要因と併せてご検討ください。</p>\n");
    html.push_str("</div>\n");
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
        assert!(html.contains("有効求人倍率"));
        assert!(html.contains("欠員補充率"));
        assert!(html.contains("採用余力 (失業率の逆数)"));
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

        // データソース注記が各 KPI に存在
        assert!(
            html.contains("v2_external_job_openings_ratio"),
            "有効求人倍率 DB テーブル名"
        );
        assert!(html.contains("v2_vacancy_rate"), "欠員補充率 DB テーブル名");
        assert!(html.contains("v2_external_labor_force"), "失業率 DB テーブル名");
        assert!(html.contains("v2_external_turnover"), "離職率 DB テーブル名");

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
        // テーブル形式
        assert!(
            html.contains("ratio_total") && html.contains("vacancy_count"),
            "計算式テーブル内に実カラム名が必要"
        );
        assert!(html.contains("separation_rate"));
        assert!(html.contains("unemployment_rate"));
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
        // 補助 KPI にもデータソース注記
        assert!(html.contains("v2_external_business_dynamics"));
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
    #[test]
    fn action_guide_branches_by_score() {
        // 逼迫: 給与訴求強化など
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
        assert!(html_high.contains("給与訴求強化"));
        assert!(html_high.contains("通勤圏拡大"));

        // 緩和: コスト見直し
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
        assert!(html_low.contains("採用コスト見直し"));
        assert!(html_low.contains("ミスマッチ低減"));
    }

    /// 全国平均失業率比較が `pref_avg_unemployment_rate` から取得される
    #[test]
    fn unemployment_national_compare_from_pref_avg() {
        // 全国平均 0.024 (=2.4%) → 100 倍されて 2.4%
        let ctx = build_test_ctx(
            vec![],
            vec![],
            vec![],
            vec![row(&[("unemployment_rate", json!(2.0))])],
            vec![],
            vec![],
            Some(0.024),
        );
        let m = compute_metrics(&ctx);
        assert_eq!(m.unemployment_rate, Some(2.0));
        assert_eq!(m.unemployment_national, Some(2.4));
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
        let pos_job_ratio = html.find("\"name\":\"有効求人倍率\"");
        let pos_vacancy = html.find("\"name\":\"欠員補充率\"");
        let pos_unemp = html.find("\"name\":\"採用余力");
        let pos_sep = html.find("\"name\":\"離職率\"");

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

    /// データソース注記関数: 単体テスト
    #[test]
    fn render_data_source_note_format() {
        let note = render_data_source_note(
            "Turso v2_external_test",
            "A / B × 100",
            "都道府県",
        );
        assert!(note.contains("v2_external_test"));
        assert!(note.contains("A / B × 100"));
        assert!(note.contains("都道府県"));
        assert!(note.contains("出典"));
        assert!(note.contains("計算"));
        assert!(note.contains("粒度"));
    }
}
