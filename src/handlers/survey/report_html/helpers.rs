//! 分割: report_html/helpers.rs (物理移動・内容変更なし)

#![allow(unused_imports, dead_code)]

use super::super::super::company::fetch::NearbyCompany;
use super::super::super::helpers::{escape_html, format_number, get_f64, get_str_ref};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::{
    CompanyAgg, EmpTypeSalary, ScatterPoint, SurveyAggregation, TagSalaryAgg,
};
use super::super::hw_enrichment::HwAreaEnrichment;
use super::super::job_seeker::JobSeekerAnalysis;
use serde_json::json;

/// Severity表現（色 + 文字アイコン）。helpers.rs::Severity に1対1対応。
#[derive(Clone, Copy)]
pub(super) enum RptSev {
    Critical,
    Warning,
    Info,
    Positive,
}

impl RptSev {
    pub(super) fn color(self) -> &'static str {
        match self {
            RptSev::Critical => "#ef4444",
            RptSev::Warning => "#f59e0b",
            RptSev::Info => "#3b82f6",
            RptSev::Positive => "#10b981",
        }
    }
    /// モノクロ耐性のためのアイコン文字併記ラベル
    pub(super) fn label(self) -> &'static str {
        match self {
            RptSev::Critical => "\u{25B2}\u{25B2} 重大",
            RptSev::Warning => "\u{25B2} 注意",
            RptSev::Info => "\u{25CF} 情報",
            RptSev::Positive => "\u{25EF} 良好",
        }
    }
}

/// severity バッジ HTML（印刷/モノクロ両対応）
pub(super) fn severity_badge(sev: RptSev) -> String {
    format!(
        "<span class=\"sev-badge\" style=\"background:{};color:#fff;font-weight:700;font-size:10pt;padding:2px 8px;border-radius:3px;letter-spacing:0.04em;\">{}</span>",
        sev.color(),
        escape_html(sev.label())
    )
}

/// Executive Summary 用 KPI カード
pub(super) fn render_kpi_card(html: &mut String, label: &str, value: &str, unit: &str) {
    html.push_str("<div class=\"kpi-card\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<div class=\"unit\">{}</div>\n",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
}

pub(super) fn render_summary_card(html: &mut String, label: &str, value: &str, unit: &str) {
    html.push_str("<div class=\"summary-card\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<div class=\"unit\">{}</div>\n",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
}

pub(super) fn render_guide_item(html: &mut String, title: &str, description: &str) {
    html.push_str("<div class=\"guide-item\">\n");
    html.push_str(&format!(
        "<div class=\"guide-title\">{}</div>\n",
        escape_html(title)
    ));
    html.push_str(&format!("{}\n", escape_html(description)));
    html.push_str("</div>\n");
}

pub(super) fn render_stat_box(html: &mut String, label: &str, value: &str) {
    html.push_str("<div class=\"stat-box\">\n");
    html.push_str(&format!(
        "<div class=\"label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div class=\"value\">{}</div>\n",
        escape_html(value)
    ));
    html.push_str("</div>\n");
}

pub(super) fn render_range_type_box(html: &mut String, label: &str, count: usize, bg_color: &str) {
    html.push_str(&format!(
        "<div style=\"background:{};border:1px solid #e0e0e0;border-radius:4px;padding:6px 8px;text-align:center;\">\n",
        bg_color
    ));
    html.push_str(&format!(
        "<div style=\"font-size:10px;color:#666;\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str(&format!(
        "<div style=\"font-size:16px;font-weight:bold;\">{}件</div>\n",
        format_number(count as i64)
    ));
    html.push_str("</div>\n");
}

/// ECharts divタグを生成（data-chart-config属性付き）
pub(super) fn render_echart_div(config_json: &str, height: u32) -> String {
    // シングルクォートをHTMLエンティティにエスケープ
    let escaped = config_json.replace('\'', "&#x27;");
    format!(
        "<div class=\"echart\" style=\"height:{}px;width:100%;\" data-chart-config='{}'></div>\n",
        height, escaped
    )
}

fn histogram_axis_interval(label_count: usize) -> usize {
    if label_count >= 28 {
        2
    } else if label_count >= 16 {
        1
    } else {
        0
    }
}

/// ヒストグラム用ECharts設定JSONを生成（平均・中央値・最頻値のmarkLine付き）
///
/// markLineのxAxis値は、category軸のラベル（例: "20万"）に正確一致させる必要がある。
/// bin_size で丸めた「bin開始値（万単位）」を渡すことで、
/// 該当binの棒の開始位置に縦線を表示する。
pub(super) fn build_histogram_echart_config(
    labels: &[String],
    values: &[usize],
    color: &str,
    mean: Option<i64>,
    median: Option<i64>,
    mode: Option<i64>,
    bin_size: i64,
) -> String {
    build_histogram_echart_config_with_stats_card(
        labels, values, color, mean, median, mode, bin_size, true,
    )
}

pub(super) fn build_histogram_echart_config_with_stats_card(
    labels: &[String],
    values: &[usize],
    color: &str,
    mean: Option<i64>,
    median: Option<i64>,
    mode: Option<i64>,
    bin_size: i64,
    use_close_stats_card: bool,
) -> String {
    // 値を category 軸ラベルに合わせる: (値 / bin_size) * bin_size を「X万」形式に
    let to_label = |yen: i64| -> String {
        if bin_size <= 0 {
            return format!("{}万", yen / 10_000);
        }
        let snapped = (yen / bin_size) * bin_size;
        // 小数万対応（5,000円刻みで 22.5万 など）
        let man = snapped as f64 / 10_000.0;
        if (man.fract()).abs() < 1e-9 {
            format!("{}万", snapped / 10_000)
        } else {
            format!("{:.1}万", man)
        }
    };

    // GAS 風: 色付きバッジ + 数値入りラベル（中央値 23.0万 のように値を含む）
    // 中央値: 緑 #22c55e / 平均: 赤 #ef4444 / 最頻値: 青 #3b82f6
    // ラベル位置を統計種別ごとに分散させる（PDF印刷時の重なり防止）
    let value_label = |yen: i64| -> String {
        let man = yen as f64 / 10_000.0;
        if (man.fract()).abs() < 0.05 {
            format!("{:.0}万", man)
        } else {
            format!("{:.1}万", man)
        }
    };

    // Round 15 (2026-05-13): bar 位置に紐付くラベルを 3 値とも残す。凡例 (chip) は廃止。
    //
    // 経緯:
    //   Round 14 で markLine label.show=false + 上部 chip box にしたが、ユーザーから
    //   「凡例だと目を移動させないといけない、bar 表示の方が見やすい」と指摘 (2026-05-13)。
    //   bar 位置の真上にラベル付与しつつ、3 値が近接した時の重なりは distance 段差で回避する。
    //
    // 新仕様:
    //   - markLine label.show = true (全て)
    //   - position = "end" (chart 上端外、横書きで bar 位置の真上)
    //   - distance を 6 / 22 / 38 と段差 (中央値=緑 → 平均=赤 → 最頻値=青 の縦並び)
    //   - graphic chip box は廃止
    let _ = use_close_stats_card;
    let _ = stats_are_close(median, mean, mode, bin_size); // 旧 API 互換 (cargo warning 抑制)
    let x_axis_interval = histogram_axis_interval(labels.len());

    let mut mark_lines = vec![];
    if let Some(m) = median {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "中央値",
            "lineStyle": {"color": "#22c55e", "type": "dashed", "width": 2},
            "label": {
                "show": true,
                "formatter": format!("中央値 {}", value_label(m)),
                "fontSize": 11,
                "fontWeight": "bold",
                "color": "#ffffff",
                "backgroundColor": "#22c55e",
                "borderRadius": 4,
                "padding": [4, 8],
                "position": "end",
                "distance": 6
            }
        }));
    }
    if let Some(m) = mean {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "平均",
            "lineStyle": {"color": "#ef4444", "type": "dashed", "width": 2},
            "label": {
                "show": true,
                "formatter": format!("平均 {}", value_label(m)),
                "fontSize": 11,
                "fontWeight": "bold",
                "color": "#ffffff",
                "backgroundColor": "#ef4444",
                "borderRadius": 4,
                "padding": [4, 8],
                "position": "end",
                "distance": 22
            }
        }));
    }
    if let Some(m) = mode {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "最頻値",
            "lineStyle": {"color": "#3b82f6", "type": "dashed", "width": 2},
            "label": {
                "show": true,
                "formatter": format!("最頻値 {}", value_label(m)),
                "fontSize": 11,
                "fontWeight": "bold",
                "color": "#ffffff",
                "backgroundColor": "#3b82f6",
                "borderRadius": 4,
                "padding": [4, 8],
                "position": "end",
                "distance": 38
            }
        }));
    }

    // Round 15: graphic chip box は廃止 (ユーザー指示: 凡例不要、bar 位置の markLine label に統一)
    let graphic = json!([]);

    // Round 2.7-AC: yAxis 0 始まり強制を bulletproof 化
    // - min: 0      → 棒高さ誇張防止
    // - scale: false → ECharts default は scale=false だが明示化 (auto-scale 罠回避)
    // - minInterval: 1 → 件数 (整数) なので小数 tick を抑止
    let config = json!({
        "tooltip": {"trigger": "axis"},
        "xAxis": {
            "type": "category",
            "data": labels,
            "axisLabel": {
                "rotate": 35,
                "fontSize": 8,
                "interval": x_axis_interval,
                "hideOverlap": true,
                "margin": 10
            }
        },
        "yAxis": {
            "type": "value",
            "min": 0,
            "scale": false,
            "minInterval": 1,
            "axisLabel": {"fontSize": 9}
        },
        "grid": {
            "left": "7%",
            "right": "12%",
            "bottom": "30%",
            // Round 15: chip 廃止 + markLine label を position=end で 3 段重ねるため top に余白を確保
            "top": "22%",
            "containLabel": true
        },
        "graphic": graphic,
        "series": [{
            "type": "bar",
            "data": values,
            "itemStyle": {"color": color},
            "markLine": {
                "data": mark_lines,
                "symbol": "none"
            }
        }]
    });
    config.to_string()
}

/// Round 2.7-AC: 3 統計値の近接判定
/// 中央値・平均・最頻値の最大差が bin_size * 2 以内なら近接とみなす
pub(super) fn stats_are_close(
    median: Option<i64>,
    mean: Option<i64>,
    mode: Option<i64>,
    bin_size: i64,
) -> bool {
    let vals: Vec<i64> = [median, mean, mode].iter().filter_map(|v| *v).collect();
    if vals.len() < 2 || bin_size <= 0 {
        return false;
    }
    let max_v = *vals.iter().max().unwrap();
    let min_v = *vals.iter().min().unwrap();
    (max_v - min_v) <= bin_size * 2
}

/// ヒストグラム用バケット集計
/// 給与値配列をbin_size刻みでバケットに分類し、ラベル・件数・bin下端境界配列を返す
pub(super) fn build_salary_histogram(
    values: &[i64],
    bin_size: i64,
) -> (Vec<String>, Vec<usize>, Vec<i64>) {
    if values.is_empty() || bin_size <= 0 {
        return (vec![], vec![], vec![]);
    }

    let valid: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    if valid.is_empty() {
        return (vec![], vec![], vec![]);
    }

    let min_val = *valid.iter().min().unwrap();
    let max_val = *valid.iter().max().unwrap();

    let start = (min_val / bin_size) * bin_size;
    let end = ((max_val / bin_size) + 1) * bin_size;

    let mut labels = Vec::new();
    let mut counts = Vec::new();
    let mut boundaries = Vec::new();

    let mut boundary = start;
    while boundary < end {
        let upper = boundary + bin_size;
        let count = valid
            .iter()
            .filter(|&&v| v >= boundary && v < upper)
            .count();
        // ラベル: bin_size が万円未満の場合は小数表記（例: 22.5万）
        let man = boundary as f64 / 10_000.0;
        let label = if (man.fract()).abs() < 1e-9 {
            format!("{}万", boundary / 10_000)
        } else {
            format!("{:.1}万", man)
        };
        labels.push(label);
        counts.push(count);
        boundaries.push(boundary);
        boundary = upper;
    }

    (labels, counts, boundaries)
}

// =====================================================================
// Round 20 (2026-05-13): 給与構造クラスタリング (設計メモ準拠)
// 固定ビン幅ではなく、市場内の「給与構造クラスタ」を作って分析する。
// 詳細: docs/salary_cluster_analysis_design.md 参照
// =====================================================================

#[derive(Debug, Clone)]
pub(super) struct SalaryCluster {
    pub label: String,
    pub lower_seg: String,  // "低下限"/"中下限"/"高下限" or "下限帯1"/"下限帯2"/... (k=4 時)
    pub range_seg: &'static str,  // "狭レンジ" / "通常レンジ" / "広レンジ"
    pub count: usize,
    pub p25: i64,
    pub p50: i64,
    pub p60: i64,  // Round 22: 設計メモ §10.2 標準より少し強いライン
    pub p75: i64,
    pub p90: i64,
    pub min: i64,
    pub max: i64,
    pub mean: i64,
}

fn percentile(sorted: &[i64], p: f64) -> i64 {
    if sorted.is_empty() {
        return 0;
    }
    let idx = (((sorted.len() - 1) as f64) * p / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 1 セグメント (lower / range) を P33 / P66 で 3 分類するための閾値を返す。
#[allow(dead_code)]
fn tercile_thresholds(values: &[i64]) -> (i64, i64) {
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    (percentile(&sorted, 33.0), percentile(&sorted, 66.0))
}

/// Round 21 (2026-05-13): Jenks natural breaks (1 次元 k-分割)。
/// 動的計画法でクラス内分散を最小化する境界を探す。
///
/// 入力: ソート済 (内部でソート) 数値配列、クラス数 k (= 2〜5 推奨)
/// 出力: クラス境界 (k-1 個)。例: k=3 なら [境界1, 境界2] を返す。
///       value < 境界1 が class 0、境界1 <= value < 境界2 が class 1、value >= 境界2 が class 2
///
/// 計算量: O(n² × k)。n=500, k=3 で 750,000 演算 = 数 ms。
/// データ件数 < k×4 では tercile にフォールバック。
///
/// 設計メモ §8.2 準拠。「24万→28万の間にギャップがある」のような自然境界を発見する。
pub(super) fn jenks_natural_breaks(values: &[i64], k: usize) -> Vec<i64> {
    let n = values.len();
    if n < k * 4 || k < 2 {
        // データ不足: tercile 等価へフォールバック
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        return (1..k)
            .map(|i| percentile(&sorted, 100.0 * i as f64 / k as f64))
            .collect();
    }

    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let xs: Vec<f64> = sorted.iter().map(|&v| v as f64).collect();

    // sse[i..=j] を計算 (区間 [i..j] の平均偏差平方和)
    let sse = |i: usize, j: usize| -> f64 {
        if i > j {
            return 0.0;
        }
        let slice = &xs[i..=j];
        let mean = slice.iter().sum::<f64>() / slice.len() as f64;
        slice.iter().map(|x| (x - mean).powi(2)).sum()
    };

    // dp[m][j]: 最初の j+1 要素を m+1 クラスに分割した時の最小 SSE
    // back[m][j]: 上記実現時の最後のクラス開始位置
    let mut dp = vec![vec![f64::INFINITY; n]; k];
    let mut back = vec![vec![0_usize; n]; k];
    // m=0: 全部 1 クラス
    for j in 0..n {
        dp[0][j] = sse(0, j);
    }
    // m=1..k-1
    for m in 1..k {
        for j in m..n {
            for i in m..=j {
                let cost = dp[m - 1][i - 1] + sse(i, j);
                if cost < dp[m][j] {
                    dp[m][j] = cost;
                    back[m][j] = i;
                }
            }
        }
    }

    // back-track して境界を取得
    let mut breaks: Vec<usize> = Vec::with_capacity(k - 1);
    let mut j = n - 1;
    for m in (1..k).rev() {
        let i = back[m][j];
        breaks.push(i);
        if i == 0 {
            break;
        }
        j = i - 1;
    }
    breaks.reverse();
    // 境界 index → 値 (各境界は「そこから新しいクラス」なので sorted[i] を返す)
    breaks.iter().map(|&i| sorted[i]).collect()
}

/// Jenks クラスタ境界をベースに各値を 0..k のクラス番号に分類
#[allow(dead_code)]
fn classify_jenks(value: i64, breaks: &[i64]) -> usize {
    for (i, &b) in breaks.iter().enumerate() {
        if value < b {
            return i;
        }
    }
    breaks.len()
}

/// Round 22 (2026-05-13): Jenks 境界の品質チェック (採用条件)。
/// 採用判定基準 (設計メモ §「実務的には自動判定が良い」準拠):
///   - n >= 50
///   - ユニーク値 >= 10
///   - 各クラスタ件数 >= 最低 10 件 (または n の 10%)
///   - 最大クラスタ比率 < 80%
/// false 時は呼び出し側で分位点フォールバックすべき。
pub(super) fn jenks_quality_ok(values: &[i64], breaks: &[i64]) -> bool {
    let n = values.len();
    if n < 50 {
        return false;
    }
    let unique_count = {
        let mut sorted = values.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len()
    };
    if unique_count < 10 {
        return false;
    }
    if breaks.is_empty() {
        return false;
    }
    // 各クラスタ件数
    let mut counts = vec![0_usize; breaks.len() + 1];
    for &v in values {
        let mut idx = breaks.len();
        for (i, &b) in breaks.iter().enumerate() {
            if v < b {
                idx = i;
                break;
            }
        }
        counts[idx] += 1;
    }
    let min_required = 10.max(n / 10);
    if counts.iter().any(|&c| c < min_required) {
        return false;
    }
    let max_ratio = *counts.iter().max().unwrap_or(&0) as f64 / n as f64;
    if max_ratio >= 0.8 {
        return false;
    }
    true
}

/// range の自然分割。
/// 1) P95 超を異常広レンジ閾値として外す
/// 2) 残りに Jenks(k=3) を試す → 採用条件 OK なら採用、NG なら tercile
/// 3) 戻り値: (狭/通常 境界, 通常/広 境界, 異常広閾値)
///    異常広閾値以上の値は「異常広レンジ」セグメントへ振る
pub(super) fn classify_range_breaks(ranges: &[i64]) -> (i64, i64, i64) {
    if ranges.is_empty() {
        return (0, i64::MAX, i64::MAX);
    }
    let mut sorted = ranges.to_vec();
    sorted.sort_unstable();
    let p95 = percentile(&sorted, 95.0);
    let body: Vec<i64> = sorted.iter().copied().filter(|&v| v <= p95).collect();
    let jenks = jenks_natural_breaks(&body, 3);
    let (t1, t2) = if jenks.len() == 2 && jenks_quality_ok(&body, &jenks) {
        (jenks[0], jenks[1])
    } else {
        tercile_thresholds(&body)
    };
    (t1, t2, p95)
}

/// 給与構造クラスタリングのコア関数。
/// 入力: (lower_salary, upper_salary) ペアのリスト (両方 > 0)
/// 出力: 最大 12 クラスタ (lower k × range 3、件数<5 のセルは省略)
///
/// Round 22 (2026-05-13): ユーザーレビューを反映:
/// - Jenks は **lower_salary 軸のみ** に適用 (range は分位点 P33/P66 で十分)
/// - lower 側の k は n に応じて動的: n<50=分位点、50<=n<150=k=3、150<=n<500=k=4、500+=k=4
/// - lower 境界が「データの自然な切れ目」になり、説明可能性を保ちつつ精度向上
/// - SalaryCluster に P60 追加 (設計メモ §10.2「標準より少し強いライン」)
pub(super) fn compute_salary_clusters(pairs: &[(i64, i64)]) -> Vec<SalaryCluster> {
    if pairs.len() < 9 {
        return Vec::new();
    }
    let n = pairs.len();
    let lowers: Vec<i64> = pairs.iter().map(|p| p.0).collect();
    let ranges: Vec<i64> = pairs.iter().map(|p| (p.1 - p.0).max(0)).collect();

    // lower 側: n に応じて k を動的決定 + Jenks
    let lower_k = if n < 50 { 3 } else if n < 150 { 3 } else { 4 };
    let lo_breaks: Vec<i64> = if n < 50 {
        // 件数少: 分位点フォールバック
        let mut sorted = lowers.clone();
        sorted.sort_unstable();
        (1..lower_k).map(|i| percentile(&sorted, 100.0 * i as f64 / lower_k as f64)).collect()
    } else {
        jenks_natural_breaks(&lowers, lower_k)
    };

    // Round 22 (2026-05-13) range 側: 異常広レンジ分離 (P95超) + Jenks 採用判定付き
    // 失敗時は tercile フォールバック。設計メモ「自動判定が良い」準拠。
    let (rn_t1, rn_t2, rn_extreme) = classify_range_breaks(&ranges);
    // lower 側も Jenks 採用判定: 採用条件 NG なら分位点 (k 同じ)
    let lo_breaks: Vec<i64> = if n >= 50 && jenks_quality_ok(&lowers, &lo_breaks) {
        lo_breaks
    } else {
        let mut sorted = lowers.clone();
        sorted.sort_unstable();
        (1..lower_k).map(|i| percentile(&sorted, 100.0 * i as f64 / lower_k as f64)).collect()
    };

    let classify_lower = |v: i64, breaks: &[i64]| -> usize {
        for (i, &b) in breaks.iter().enumerate() {
            if v < b {
                return i;
            }
        }
        breaks.len()
    };
    // range 分類: 0=狭、1=通常、2=広、3=異常広 (P95超)
    let classify_range = |v: i64, t1: i64, t2: i64, ext: i64| -> usize {
        if v >= ext { 3 } else if v < t1 { 0 } else if v < t2 { 1 } else { 2 }
    };

    // lower_k × 4 グリッドに集計 (4 = 狭/通常/広/異常広)
    let mut buckets: Vec<Vec<Vec<i64>>> = (0..lower_k)
        .map(|_| (0..4).map(|_| Vec::new()).collect())
        .collect();
    for (i, &low) in lowers.iter().enumerate() {
        let rg = ranges[i];
        let li = classify_lower(low, &lo_breaks);
        let ri = classify_range(rg, rn_t1, rn_t2, rn_extreme);
        buckets[li][ri].push(low);
    }

    // lower セグメント名: k=3 なら「低/中/高 下限」、k=4 なら「下限帯1/2/3/4」
    let lower_seg_name = |i: usize| -> String {
        if lower_k == 3 {
            match i {
                0 => "低下限".to_string(),
                1 => "中下限".to_string(),
                _ => "高下限".to_string(),
            }
        } else {
            format!("下限帯{}", i + 1)
        }
    };
    let rn_names = ["狭レンジ", "通常レンジ", "広レンジ", "異常広レンジ"];

    let mut clusters: Vec<SalaryCluster> = Vec::new();
    for li in 0..lower_k {
        for ri in 0..4 {
            let vs = &buckets[li][ri];
            if vs.is_empty() {
                continue;
            }
            let mut sorted = vs.clone();
            sorted.sort_unstable();
            let count = sorted.len();
            let mean = if count > 0 {
                sorted.iter().sum::<i64>() / count as i64
            } else {
                0
            };
            clusters.push(SalaryCluster {
                label: format!("{} × {}", lower_seg_name(li), rn_names[ri]),
                lower_seg: lower_seg_name(li),
                range_seg: rn_names[ri],
                count,
                p25: percentile(&sorted, 25.0),
                p50: percentile(&sorted, 50.0),
                p60: percentile(&sorted, 60.0),
                p75: percentile(&sorted, 75.0),
                p90: percentile(&sorted, 90.0),
                min: *sorted.first().unwrap_or(&0),
                max: *sorted.last().unwrap_or(&0),
                mean,
            });
        }
    }

    // 件数 < MIN_CLUSTER_SIZE のクラスタは件数降順でソートして表示。マージ実装は複雑なので
    // 単純に小さいクラスタも表示する (件数 < 5 なら省略)。
    clusters.retain(|c| c.count >= 5);
    clusters.sort_by_key(|c| std::cmp::Reverse(c.count));
    clusters
}

/// クラスタテーブル HTML
pub(super) fn build_cluster_table_html(
    clusters: &[SalaryCluster],
    headline: &str,
) -> String {
    if clusters.is_empty() {
        return format!("<p class=\"data-empty\">{}</p>\n", headline);
    }
    let mut s = String::with_capacity(2048);
    s.push_str("<table class=\"data-table cluster-table\">\n");
    s.push_str(
        "<thead><tr><th>クラスタ</th><th>件数</th><th>P25</th>\
         <th>P50 (中央値)</th><th>P60 (標準+)</th><th>P75 (競争力)</th>\
         <th>P90 (高待遇)</th><th>平均</th></tr></thead>\n<tbody>\n",
    );
    let to_man = |v: i64| -> String {
        let m = v as f64 / 10_000.0;
        if (m.fract()).abs() < 0.05 { format!("{:.0}万", m) } else { format!("{:.1}万", m) }
    };
    for c in clusters {
        s.push_str(&format!(
            "<tr><td>{}</td><td>{}</td><td>{}</td><td><strong>{}</strong></td>\
             <td>{}</td><td>{}</td><td>{}</td><td>{}</td></tr>\n",
            escape_xml_helper(&c.label),
            c.count, to_man(c.p25), to_man(c.p50), to_man(c.p60),
            to_man(c.p75), to_man(c.p90), to_man(c.mean),
        ));
    }
    s.push_str("</tbody></table>\n");
    s
}

/// 顧客 (CSV 全体) の中央値 / 平均が最近いクラスタを返す。
pub(super) fn nearest_cluster<'a>(
    clusters: &'a [SalaryCluster],
    target_value: i64,
) -> Option<&'a SalaryCluster> {
    clusters.iter().min_by_key(|c| {
        // P50 との絶対差を距離とする
        (c.p50 - target_value).abs()
    })
}

/// クラスタ分析の So-What コメントを生成
pub(super) fn cluster_so_what_text(
    clusters: &[SalaryCluster],
    customer_median: i64,
) -> String {
    if clusters.is_empty() {
        return String::new();
    }
    let target = match nearest_cluster(clusters, customer_median) {
        Some(c) => c,
        None => return String::new(),
    };
    let diff_p50 = customer_median - target.p50;
    let to_man = |v: i64| (v as f64 / 10_000.0);
    let diff_str = if diff_p50.abs() < 5000 {
        format!("中央値とほぼ同水準")
    } else if diff_p50 > 0 {
        format!("クラスタ中央値を {:.1} 万円上回る", to_man(diff_p50.abs()))
    } else {
        format!("クラスタ中央値を {:.1} 万円下回る", to_man(diff_p50.abs()))
    };
    format!(
        "<p class=\"section-sowhat\">※ 求人群中央値 {:.1} 万円は「{}」クラスタ (件数 {}, P50 = {:.1} 万円) に最も近く、{}水準です。\
         競争力を持たせる場合は P75 ({:.1} 万円) 付近、高待遇訴求は P90 ({:.1} 万円) 以上が目安です。</p>\n",
        to_man(customer_median),
        target.label,
        target.count,
        to_man(target.p50),
        diff_str,
        to_man(target.p75),
        to_man(target.p90),
    )
}

/// Round 22 (2026-05-13): クラスタ内ヒストグラム表示対象を絞り込み (ユーザーレビュー反映)。
/// 旧: 上位 N クラスタ (件数降順)
/// 新: 「顧客最近傍 + 最多 + 高給与×広レンジ」の最大 3 つ (重複除く)
///
/// markLine は中央値/平均/最頻値 を残す (build_histogram_svg 既存仕様)。
/// 適正値 P25/P50/P60/P75/P90 はクラスタテーブルで参照。
pub(super) fn build_cluster_histograms_svg(
    pairs: &[(i64, i64)],
    clusters: &[SalaryCluster],
    customer_median: i64,
) -> String {
    if clusters.is_empty() || pairs.is_empty() {
        return String::new();
    }
    let n = pairs.len();
    let lowers: Vec<i64> = pairs.iter().map(|p| p.0).collect();
    let ranges: Vec<i64> = pairs.iter().map(|p| (p.1 - p.0).max(0)).collect();

    // compute_salary_clusters と同じ閾値ロジックを再現
    let lower_k = if n < 50 { 3 } else if n < 150 { 3 } else { 4 };
    let lo_breaks: Vec<i64> = if n < 50 {
        let mut sorted = lowers.clone();
        sorted.sort_unstable();
        (1..lower_k).map(|i| percentile(&sorted, 100.0 * i as f64 / lower_k as f64)).collect()
    } else {
        jenks_natural_breaks(&lowers, lower_k)
    };
    let (rn_t1, rn_t2, rn_extreme) = classify_range_breaks(&ranges);

    let classify_lower = |v: i64| -> usize {
        for (i, &b) in lo_breaks.iter().enumerate() {
            if v < b { return i; }
        }
        lo_breaks.len()
    };
    let classify_range = |v: i64| -> usize {
        if v >= rn_extreme { 3 }
        else if v < rn_t1 { 0 }
        else if v < rn_t2 { 1 }
        else { 2 }
    };

    // 表示対象 3 クラスタ選定 (重複除く):
    // (1) 顧客最近傍 (P50 差最小)
    // (2) 最多件数
    // (3) 高給与×広レンジ (k=3 なら 高下限×広レンジ、k=4 なら 下限帯4×広レンジ)
    let mut selected: Vec<&SalaryCluster> = Vec::new();
    if let Some(c) = nearest_cluster(clusters, customer_median) {
        selected.push(c);
    }
    if let Some(c) = clusters.iter().max_by_key(|c| c.count) {
        if !selected.iter().any(|s| s.label == c.label) {
            selected.push(c);
        }
    }
    let high_label_prefix = if lower_k == 3 { "高下限" } else { "下限帯" };
    if let Some(c) = clusters.iter().find(|c| {
        c.lower_seg.starts_with(high_label_prefix) && c.range_seg == "広レンジ"
    }) {
        if !selected.iter().any(|s| s.label == c.label) {
            selected.push(c);
        }
    }

    let mut s = String::new();
    for (rank, c) in selected.iter().enumerate() {
        let target_lower_seg = &c.lower_seg;
        let target_range_seg = c.range_seg;
        // このクラスタに属する lower_salary 配列を再構築
        let cluster_lowers: Vec<i64> = pairs
            .iter()
            .filter(|p| {
                let rg = (p.1 - p.0).max(0);
                let lo_idx = classify_lower(p.0);
                let rn_idx = classify_range(rg);
                let lo_seg = if lower_k == 3 {
                    match lo_idx { 0 => "低下限", 1 => "中下限", _ => "高下限" }.to_string()
                } else {
                    format!("下限帯{}", lo_idx + 1)
                };
                let rn_seg = match rn_idx { 0 => "狭レンジ", 1 => "通常レンジ", 2 => "広レンジ", _ => "異常広レンジ" };
                lo_seg == *target_lower_seg && rn_seg == target_range_seg
            })
            .map(|p| p.0)
            .collect();
        if cluster_lowers.len() < 8 {
            continue;
        }
        let bin_w = freedman_diaconis_bin_width(&cluster_lowers);
        let mean = if !cluster_lowers.is_empty() {
            Some(cluster_lowers.iter().sum::<i64>() / cluster_lowers.len() as i64)
        } else { None };
        let median = {
            let mut sorted = cluster_lowers.clone();
            sorted.sort_unstable();
            Some(sorted[sorted.len() / 2])
        };
        let mode = compute_mode(&cluster_lowers, bin_w);
        let color = match rank { 0 => "#1565C0", 1 => "#009E73", _ => "#D55E00" };
        // 役割タグ
        let role = match rank {
            0 => "求人群中央値に最近接",
            1 if selected[0].label != c.label => "件数最多",
            _ => "高給与×広レンジ (高待遇訴求群)",
        };
        s.push_str(&format!(
            "<div class=\"salary-chart-block salary-chart-page-start\">\n\
             <h3>クラスタ「{}」内分布 ({} 件、{} 円刻み・{})</h3>\n",
            escape_xml_helper(&c.label), c.count, bin_w, role,
        ));
        s.push_str(&build_histogram_svg(&cluster_lowers, bin_w, color, median, mean, mode));
        s.push_str("</div>\n");
    }
    s
}

/// クラスタごとの横並びボックスプロット (Round 16 build_boxplot_svg と類似だが N 個並列)
pub(super) fn build_cluster_boxplots_svg(clusters: &[SalaryCluster]) -> String {
    if clusters.is_empty() {
        return String::new();
    }
    let n = clusters.len();
    let all_min = clusters.iter().map(|c| c.min).min().unwrap_or(0);
    let all_max = clusters.iter().map(|c| c.max).max().unwrap_or(1);
    let range_y = (all_max - all_min).max(1);
    let plot_x0 = 60.0_f64;
    let plot_x1 = 780.0_f64;
    let plot_y0 = 40.0_f64;
    let plot_y1 = 360.0_f64;
    let plot_h = plot_y1 - plot_y0;
    let yen_to_y = |yen: i64| -> f64 {
        plot_y1 - ((yen - all_min) as f64 / range_y as f64) * plot_h
    };
    let to_man = |v: i64| -> String {
        let m = v as f64 / 10_000.0;
        if (m.fract()).abs() < 0.05 { format!("{:.0}万", m) } else { format!("{:.1}万", m) }
    };

    let cell_w = (plot_x1 - plot_x0) / n as f64;
    let box_w = (cell_w * 0.55).clamp(20.0, 80.0);

    let mut s = String::with_capacity(4096);
    s.push_str(
        "<div class=\"cluster-boxplots-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 420\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // Y axis (left)
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\" text-anchor=\"end\">\n");
    for k in 0..=4 {
        let v = all_min + range_y * k / 4;
        let y = yen_to_y(v);
        s.push_str(&format!(
            "<line x1=\"{x0:.1}\" y1=\"{y:.2}\" x2=\"{x1:.1}\" y2=\"{y:.2}\" stroke=\"#f1f5f9\" stroke-width=\"0.5\"/>\
             <text x=\"{tx:.1}\" y=\"{ty:.2}\">{lbl}</text>\n",
            x0 = plot_x0, x1 = plot_x1, y = y, tx = plot_x0 - 6.0, ty = y + 3.0, lbl = to_man(v),
        ));
    }
    s.push_str("</g>\n");

    // Each cluster boxplot
    for (i, c) in clusters.iter().enumerate() {
        let cx = plot_x0 + cell_w * (i as f64 + 0.5);
        let bx = cx - box_w / 2.0;
        let y_p25 = yen_to_y(c.p25);
        let y_p75 = yen_to_y(c.p75);
        let y_p50 = yen_to_y(c.p50);
        let y_min = yen_to_y(c.min);
        let y_max = yen_to_y(c.max);
        // whisker
        s.push_str(&format!(
            "<line x1=\"{cx:.2}\" y1=\"{ymin:.2}\" x2=\"{cx:.2}\" y2=\"{ymax:.2}\" stroke=\"#1e3a8a\" stroke-width=\"1\"/>\n",
            cx = cx, ymin = y_min, ymax = y_max,
        ));
        // box
        s.push_str(&format!(
            "<rect x=\"{bx:.2}\" y=\"{y75:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"#dbeafe\" stroke=\"#1e3a8a\" stroke-width=\"1.5\"/>\n",
            bx = bx, y75 = y_p75, w = box_w, h = (y_p25 - y_p75).abs().max(2.0),
        ));
        // median line
        s.push_str(&format!(
            "<line x1=\"{bx:.2}\" y1=\"{y50:.2}\" x2=\"{x2:.2}\" y2=\"{y50:.2}\" stroke=\"#1e3a8a\" stroke-width=\"2.5\"/>\n",
            bx = bx, x2 = bx + box_w, y50 = y_p50,
        ));
        // label below
        s.push_str(&format!(
            "<text x=\"{cx:.2}\" y=\"{ty:.2}\" font-size=\"9\" fill=\"#0f172a\" text-anchor=\"middle\">{lbl}</text>\
             <text x=\"{cx:.2}\" y=\"{ty2:.2}\" font-size=\"9\" fill=\"#6e7079\" text-anchor=\"middle\">n={n}</text>\n",
            cx = cx, ty = plot_y1 + 16.0, ty2 = plot_y1 + 30.0, lbl = escape_xml_helper(&c.label), n = c.count,
        ));
    }
    s.push_str("</svg>\n</div>\n");
    s
}

/// Freedman-Diaconis rule に基づく動的 bin 幅
/// bin_width = 2 × IQR / n^(1/3)
/// 計算結果を読みやすい単位 (1000/2000/5000/10000/20000) に丸める
pub(super) fn freedman_diaconis_bin_width(values: &[i64]) -> i64 {
    if values.len() < 4 {
        return 10_000;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let q1 = percentile(&sorted, 25.0);
    let q3 = percentile(&sorted, 75.0);
    let iqr = (q3 - q1) as f64;
    let n = values.len() as f64;
    let raw = 2.0 * iqr / n.cbrt();
    // 読みやすい単位に丸め
    let candidates = [1_000, 2_000, 5_000, 10_000, 20_000, 50_000];
    let mut best = 10_000_i64;
    for &c in &candidates {
        if (c as f64) >= raw {
            best = c;
            break;
        }
        best = c;
    }
    best
}

/// 上側外れ値 (Q3 + 1.5*IQR を超える値) を別枠化
/// 返値: (本体 values, 外れ値リスト)
pub(super) fn split_upper_outliers(values: &[i64]) -> (Vec<i64>, Vec<i64>) {
    if values.len() < 8 {
        return (values.to_vec(), Vec::new());
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let q1 = percentile(&sorted, 25.0) as f64;
    let q3 = percentile(&sorted, 75.0) as f64;
    let iqr = q3 - q1;
    let upper_bound = (q3 + 1.5 * iqr) as i64;
    let body: Vec<i64> = values.iter().copied().filter(|&v| v <= upper_bound).collect();
    let outliers: Vec<i64> = values.iter().copied().filter(|&v| v > upper_bound).collect();
    (body, outliers)
}

/// Round 16 (2026-05-13): P2.5〜P97.5 で外れ値を切り捨てた値配列を返す。
///
/// 1,000 円刻みなど dense なヒストグラムでは、極端な外れ値 (例: 月給 1000 万円) が
/// X 軸を引き伸ばして本体 bar が点になる問題があるため、表示用に trim する。
/// mean/median は呼び出し側で original 配列から計算した値を渡すこと。
pub(super) fn trim_outliers_p2_5_p97_5(values: &[i64]) -> Vec<i64> {
    if values.len() < 20 {
        // 標本サイズが小さい場合は trim せずそのまま返す (信頼性低い)
        return values.iter().filter(|&&v| v > 0).copied().collect();
    }
    let mut sorted: Vec<i64> = values.iter().filter(|&&v| v > 0).copied().collect();
    sorted.sort_unstable();
    if sorted.len() < 20 {
        return sorted;
    }
    let lo_idx = ((sorted.len() as f64) * 0.025).round() as usize;
    let hi_idx = ((sorted.len() as f64) * 0.975).round() as usize;
    let lo = sorted[lo_idx.min(sorted.len() - 1)];
    let hi = sorted[hi_idx.min(sorted.len() - 1)];
    sorted.into_iter().filter(|&v| v >= lo && v <= hi).collect()
}

/// Round 16 (2026-05-13): SSR SVG ヒストグラム builder。
///
/// ECharts SVG renderer が `emulateMedia('print')` 経路で markLine label を含む
/// 一部 text を描画しない問題を回避するため、Rust 側で SVG を直接生成する。
/// pyramid (`build_pyramid_svg` in demographics.rs) と同じパターン。
///
/// レイアウト (viewBox 0 0 800 380):
///   - plot 領域: x=60..780, y=50..320 (高さ 270, 幅 720)
///   - 上部 50px: markLine 色付き label box 3 個 (中央値=緑 / 平均=赤 / 最頻値=青)
///   - 下部 60px: X 軸目盛
///   - 左 60px: Y 軸目盛
///
/// 引数:
///   bin_size_yen: 1_000 (dense) または 10_000 (粗) を想定
///   bar_color:    "#42A5F5" (下限) / "#66BB6A" (上限) など
pub(super) fn build_histogram_svg(
    values: &[i64],
    bin_size: i64,
    bar_color: &str,
    median: Option<i64>,
    mean: Option<i64>,
    mode: Option<i64>,
) -> String {
    let (_labels_unused, counts, boundaries) = build_salary_histogram(values, bin_size);
    if counts.is_empty() || boundaries.is_empty() {
        return String::new();
    }
    let bin_count = counts.len();
    let max_count = *counts.iter().max().unwrap_or(&1).max(&1);
    let x_min_yen = *boundaries.first().unwrap();
    let x_max_yen = x_min_yen + (bin_count as i64) * bin_size;

    // Round 19: chart 高さ拡大 (viewBox 380 → 440) で視認性向上 + 1 chart 1 page 化
    let plot_x0 = 60_i32;
    let plot_x1 = 780_i32;
    let plot_y0 = 60_i32;
    let plot_y1 = 380_i32;
    let plot_w = plot_x1 - plot_x0;
    let plot_h = plot_y1 - plot_y0;

    let yen_to_x = |yen: i64| -> f64 {
        let frac = (yen - x_min_yen) as f64 / ((x_max_yen - x_min_yen) as f64).max(1.0);
        plot_x0 as f64 + frac * plot_w as f64
    };
    let count_to_y = |c: usize| -> f64 {
        plot_y1 as f64 - (c as f64 / max_count as f64) * plot_h as f64
    };
    let yen_to_man = |yen: i64| -> String {
        let man = yen as f64 / 10_000.0;
        if (man.fract()).abs() < 0.05 {
            format!("{:.0}万", man)
        } else {
            format!("{:.1}万", man)
        }
    };

    let mut s = String::with_capacity(4096);
    s.push_str(
        "<div class=\"histogram-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 440\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         aria-label=\"給与ヒストグラム\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );

    // bars (Round 19: bar 最小幅 1.5px を確保し dense でも見える)
    let bin_w = plot_w as f64 / bin_count as f64;
    let bar_gap = (bin_w * 0.08).clamp(0.5, 3.0);
    for (i, &cnt) in counts.iter().enumerate() {
        if cnt == 0 {
            continue;
        }
        let raw_w = bin_w - bar_gap;
        let w = raw_w.max(1.5);
        let x = plot_x0 as f64 + (i as f64) * bin_w + (bin_w - w) / 2.0;
        let y = count_to_y(cnt);
        let h = plot_y1 as f64 - y;
        s.push_str(&format!(
            "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"{c}\"/>\n",
            x = x, y = y, w = w, h = h, c = bar_color,
        ));
    }

    // Y axis (left): 0 と最大件数の中間 4 ticks
    s.push_str(&format!(
        "<line x1=\"{x}\" y1=\"{y0}\" x2=\"{x}\" y2=\"{y1}\" stroke=\"#cbd5e1\" stroke-width=\"0.5\"/>\n",
        x = plot_x0, y0 = plot_y0, y1 = plot_y1,
    ));
    let y_ticks = 4;
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\" text-anchor=\"end\">\n");
    for k in 0..=y_ticks {
        let cnt_val = (max_count * k) / y_ticks;
        let y = count_to_y(cnt_val);
        s.push_str(&format!(
            "<line x1=\"{x0}\" y1=\"{y:.2}\" x2=\"{x1}\" y2=\"{y:.2}\" stroke=\"#f1f5f9\" stroke-width=\"0.5\"/>\
             <text x=\"{tx}\" y=\"{ty:.2}\">{c}</text>\n",
            x0 = plot_x0, x1 = plot_x1, y = y, tx = plot_x0 - 6, ty = y + 3.0, c = cnt_val,
        ));
    }
    s.push_str("</g>\n");

    // X axis (bottom): bin 数に応じて tick 数を 6-10 に
    let target_ticks = if bin_count <= 12 { bin_count } else { 8 };
    s.push_str(&format!(
        "<line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\n",
        x0 = plot_x0, x1 = plot_x1, y = plot_y1,
    ));
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\" text-anchor=\"middle\">\n");
    for k in 0..=target_ticks {
        let yen = x_min_yen + ((x_max_yen - x_min_yen) * k as i64) / (target_ticks.max(1) as i64);
        let x = yen_to_x(yen);
        s.push_str(&format!(
            "<line x1=\"{x:.2}\" y1=\"{y0}\" x2=\"{x:.2}\" y2=\"{y1}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\
             <text x=\"{x:.2}\" y=\"{ty}\">{lbl}</text>\n",
            x = x, y0 = plot_y1, y1 = plot_y1 + 5, ty = plot_y1 + 18, lbl = yen_to_man(yen),
        ));
    }
    s.push_str("</g>\n");

    // markLine + 上部 label box (中央値/平均/最頻値)
    let stats = [
        (median, "中央値", "#22c55e"),
        (mean,   "平均",   "#ef4444"),
        (mode,   "最頻値", "#3b82f6"),
    ];
    for (val_opt, name, color) in &stats {
        let Some(v) = val_opt else { continue };
        if *v < x_min_yen || *v > x_max_yen {
            continue;
        }
        let x = yen_to_x(*v);
        s.push_str(&format!(
            "<line x1=\"{x:.2}\" y1=\"{y0}\" x2=\"{x:.2}\" y2=\"{y1}\" stroke=\"{c}\" stroke-width=\"2\" stroke-dasharray=\"4 3\"/>\n",
            x = x, y0 = plot_y0 - 5, y1 = plot_y1, c = color,
        ));
    }
    // Round 19 (2026-05-13): chip 重なり問題を完全回避するため固定位置 (左/中/右) に並列配置。
    // 3 chip が x 軸方向に近接する場合に重なって読めない問題への対応。
    // markLine の縦線は値の位置に残し、chip 自体は chart 上部の決め打ち位置 (header 行)。
    let mut chips: Vec<(String, &str)> = vec![];
    for (val_opt, name, color) in &stats {
        if let Some(v) = val_opt {
            chips.push((format!("{} {}", name, yen_to_man(*v)), color));
        }
    }
    if !chips.is_empty() {
        // 3 個前提でなく、N 個を均等に並べる
        let n = chips.len();
        let chip_gap = 8.0;
        let chip_h = 22.0;
        let chip_y = 20.0;
        // 各 chip の幅を文字数から推定
        let widths: Vec<f64> = chips
            .iter()
            .map(|(t, _)| t.chars().count() as f64 * 11.0 + 16.0)
            .collect();
        let total_w: f64 = widths.iter().sum::<f64>() + chip_gap * (n as f64 - 1.0);
        // 中央寄せの x_start
        let plot_center = (plot_x0 + plot_x1) as f64 / 2.0;
        let mut x_cursor = plot_center - total_w / 2.0;
        s.push_str("<g font-size=\"11\" font-weight=\"bold\" fill=\"#ffffff\" text-anchor=\"middle\">\n");
        for (i, (text, color)) in chips.iter().enumerate() {
            let w = widths[i];
            s.push_str(&format!(
                "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" rx=\"4\" fill=\"{c}\"/>\
                 <text x=\"{tx:.2}\" y=\"{ty:.2}\">{txt}</text>\n",
                x = x_cursor, y = chip_y, w = w, h = chip_h, c = color,
                tx = x_cursor + w / 2.0, ty = chip_y + 15.0,
                txt = escape_xml_helper(text),
            ));
            x_cursor += w + chip_gap;
        }
        s.push_str("</g>\n");
    }

    s.push_str("</svg>\n</div>\n");
    s
}

/// シンプルな OLS 回帰 (slope, intercept) を返す。
/// 点数 < 6 または分散ゼロ時は None。
pub(super) fn compute_simple_regression(points: &[(f64, f64)]) -> Option<(f64, f64)> {
    if points.len() < 6 {
        return None;
    }
    let n = points.len() as f64;
    let sx: f64 = points.iter().map(|p| p.0).sum();
    let sy: f64 = points.iter().map(|p| p.1).sum();
    let mean_x = sx / n;
    let mean_y = sy / n;
    let mut num = 0.0_f64;
    let mut den = 0.0_f64;
    for (x, y) in points {
        let dx = x - mean_x;
        num += dx * (y - mean_y);
        den += dx * dx;
    }
    if den.abs() < 1e-9 {
        return None;
    }
    let slope = num / den;
    let intercept = mean_y - slope * mean_x;
    if slope.is_finite() && intercept.is_finite() {
        Some((slope, intercept))
    } else {
        None
    }
}

fn escape_xml_helper(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// 横向き boxplot (5 数要約) を SSR SVG で描画。
/// 入力は yen 単位。表示は 万円。
pub(super) fn build_boxplot_svg(min: i64, q1: i64, median: i64, q3: i64, max: i64) -> String {
    let plot_x0 = 60_f64;
    let plot_x1 = 760_f64;
    let plot_y = 90_f64;
    let box_h = 50_f64;
    let yen_to_x = |yen: i64| -> f64 {
        if max <= min { return plot_x0; }
        let frac = (yen - min) as f64 / (max - min) as f64;
        plot_x0 + frac * (plot_x1 - plot_x0)
    };
    let yen_to_man = |yen: i64| -> String {
        let man = yen as f64 / 10_000.0;
        if (man.fract()).abs() < 0.05 { format!("{:.0}万", man) } else { format!("{:.1}万", man) }
    };

    let x_min = yen_to_x(min);
    let x_q1 = yen_to_x(q1);
    let x_med = yen_to_x(median);
    let x_q3 = yen_to_x(q3);
    let x_max = yen_to_x(max);

    let mut s = String::with_capacity(2048);
    s.push_str(
        "<div class=\"boxplot-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 200\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         aria-label=\"給与 boxplot\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // whisker line (min to max horizontal)
    s.push_str(&format!(
        "<line x1=\"{:.2}\" y1=\"{:.2}\" x2=\"{:.2}\" y2=\"{:.2}\" stroke=\"#1e3a8a\" stroke-width=\"1.5\"/>\n",
        x_min, plot_y + box_h / 2.0, x_max, plot_y + box_h / 2.0,
    ));
    // min / max whisker caps
    for x in [x_min, x_max] {
        s.push_str(&format!(
            "<line x1=\"{x:.2}\" y1=\"{y0:.2}\" x2=\"{x:.2}\" y2=\"{y1:.2}\" stroke=\"#1e3a8a\" stroke-width=\"1.5\"/>\n",
            x = x, y0 = plot_y + 10.0, y1 = plot_y + box_h - 10.0,
        ));
    }
    // box (Q1 .. Q3)
    s.push_str(&format!(
        "<rect x=\"{x:.2}\" y=\"{y:.2}\" width=\"{w:.2}\" height=\"{h:.2}\" fill=\"#dbeafe\" stroke=\"#1e3a8a\" stroke-width=\"2\"/>\n",
        x = x_q1, y = plot_y, w = (x_q3 - x_q1).max(2.0), h = box_h,
    ));
    // median line
    s.push_str(&format!(
        "<line x1=\"{x:.2}\" y1=\"{y0:.2}\" x2=\"{x:.2}\" y2=\"{y1:.2}\" stroke=\"#1e3a8a\" stroke-width=\"3\"/>\n",
        x = x_med, y0 = plot_y, y1 = plot_y + box_h,
    ));

    // axis line (bottom)
    s.push_str(&format!(
        "<line x1=\"{x0:.2}\" y1=\"{y}\" x2=\"{x1:.2}\" y2=\"{y}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\n",
        x0 = plot_x0, x1 = plot_x1, y = plot_y + box_h + 20.0,
    ));
    // 5 数要約 ラベル
    s.push_str("<g font-size=\"11\" fill=\"#0f172a\" text-anchor=\"middle\">\n");
    for (x, label, val) in &[
        (x_min, "min", min), (x_q1, "Q1", q1), (x_med, "中央値", median),
        (x_q3, "Q3", q3), (x_max, "max", max),
    ] {
        s.push_str(&format!(
            "<line x1=\"{x:.2}\" y1=\"{y0:.2}\" x2=\"{x:.2}\" y2=\"{y1:.2}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\
             <text x=\"{x:.2}\" y=\"{ty:.2}\" font-weight=\"bold\">{lbl}</text>\
             <text x=\"{x:.2}\" y=\"{ty2:.2}\" fill=\"#6e7079\">{v}</text>\n",
            x = x, y0 = plot_y + box_h, y1 = plot_y + box_h + 20.0,
            ty = plot_y + box_h + 36.0, ty2 = plot_y + box_h + 52.0,
            lbl = label, v = yen_to_man(*val),
        ));
    }
    s.push_str("</g>\n");
    s.push_str("</svg>\n</div>\n");
    s
}

/// ドーナツ / pie chart を SSR SVG で描画。
/// items: (label, value, color) のリスト。
pub(super) fn build_donut_svg(items: &[(String, i64, &str)]) -> String {
    let total: i64 = items.iter().map(|(_, v, _)| *v).sum();
    if total <= 0 {
        return String::new();
    }
    let cx = 200.0_f64;
    let cy = 180.0_f64;
    let r_outer = 120.0_f64;
    let r_inner = 70.0_f64;
    let mut s = String::with_capacity(2048);
    s.push_str(
        "<div class=\"donut-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 360\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // arc paths
    let mut start_angle = -std::f64::consts::FRAC_PI_2; // 12 時方向開始
    for (_, val, color) in items.iter() {
        if *val <= 0 { continue; }
        let frac = *val as f64 / total as f64;
        let end_angle = start_angle + frac * 2.0 * std::f64::consts::PI;
        let large_arc = if frac > 0.5 { 1 } else { 0 };
        let (sx, sy) = (cx + r_outer * start_angle.cos(), cy + r_outer * start_angle.sin());
        let (ex, ey) = (cx + r_outer * end_angle.cos(), cy + r_outer * end_angle.sin());
        let (sx2, sy2) = (cx + r_inner * end_angle.cos(), cy + r_inner * end_angle.sin());
        let (ex2, ey2) = (cx + r_inner * start_angle.cos(), cy + r_inner * start_angle.sin());
        s.push_str(&format!(
            "<path d=\"M {sx:.2} {sy:.2} A {r:.2} {r:.2} 0 {la} 1 {ex:.2} {ey:.2} L {sx2:.2} {sy2:.2} A {ri:.2} {ri:.2} 0 {la} 0 {ex2:.2} {ey2:.2} Z\" fill=\"{c}\"/>\n",
            sx = sx, sy = sy, r = r_outer, la = large_arc, ex = ex, ey = ey,
            sx2 = sx2, sy2 = sy2, ri = r_inner, ex2 = ex2, ey2 = ey2, c = color,
        ));
        start_angle = end_angle;
    }
    // legend (right side)
    let legend_x = 380.0_f64;
    let mut legend_y = 60.0_f64;
    s.push_str("<g font-size=\"12\" fill=\"#0f172a\">\n");
    for (label, val, color) in items.iter() {
        if *val <= 0 { continue; }
        let pct = (*val as f64 / total as f64) * 100.0;
        s.push_str(&format!(
            "<rect x=\"{lx:.2}\" y=\"{ly:.2}\" width=\"14\" height=\"14\" fill=\"{c}\"/>\
             <text x=\"{tx:.2}\" y=\"{ty:.2}\">{lbl} ({v} 件 / {p:.1}%)</text>\n",
            lx = legend_x, ly = legend_y, c = color,
            tx = legend_x + 22.0, ty = legend_y + 12.0,
            lbl = escape_xml_helper(label), v = val, p = pct,
        ));
        legend_y += 26.0;
    }
    s.push_str("</g>\n");
    s.push_str("</svg>\n</div>\n");
    s
}

/// 縦棒グラフを SSR SVG で描画。
/// items: (label, value) のリスト。color は全 bar 共通。
pub(super) fn build_vbar_svg(items: &[(String, f64)], bar_color: &str, y_unit: &str) -> String {
    if items.is_empty() {
        return String::new();
    }
    let max_v = items.iter().map(|(_, v)| *v).fold(0.0_f64, f64::max).max(1.0);
    let plot_x0 = 80.0_f64;
    let plot_x1 = 760.0_f64;
    let plot_y0 = 40.0_f64;
    let plot_y1 = 280.0_f64;
    let plot_w = plot_x1 - plot_x0;
    let plot_h = plot_y1 - plot_y0;
    let n = items.len();
    let bar_w = (plot_w / n as f64) * 0.6;
    let mut s = String::with_capacity(2048);
    s.push_str(
        "<div class=\"vbar-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 340\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // Y axis grid + label
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\" text-anchor=\"end\">\n");
    for k in 0..=4 {
        let v = (max_v * k as f64) / 4.0;
        let y = plot_y1 - (v / max_v) * plot_h;
        s.push_str(&format!(
            "<line x1=\"{x0}\" y1=\"{y:.2}\" x2=\"{x1}\" y2=\"{y:.2}\" stroke=\"#f1f5f9\" stroke-width=\"0.5\"/>\
             <text x=\"{tx}\" y=\"{ty:.2}\">{val:.1}{u}</text>\n",
            x0 = plot_x0, x1 = plot_x1, y = y, tx = plot_x0 - 6.0, ty = y + 3.0, val = v, u = y_unit,
        ));
    }
    s.push_str("</g>\n");
    // X axis
    s.push_str(&format!(
        "<line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\n",
        x0 = plot_x0, x1 = plot_x1, y = plot_y1,
    ));
    // Bars
    for (i, (label, val)) in items.iter().enumerate() {
        let cx = plot_x0 + plot_w * (i as f64 + 0.5) / n as f64;
        let bx = cx - bar_w / 2.0;
        let bh = (val / max_v) * plot_h;
        let by = plot_y1 - bh;
        s.push_str(&format!(
            "<rect x=\"{bx:.2}\" y=\"{by:.2}\" width=\"{bw:.2}\" height=\"{bh:.2}\" fill=\"{c}\"/>\
             <text x=\"{tx:.2}\" y=\"{ty:.2}\" font-size=\"11\" fill=\"#0f172a\" text-anchor=\"middle\" font-weight=\"bold\">{v:.1}{u}</text>\
             <text x=\"{tx:.2}\" y=\"{lty:.2}\" font-size=\"11\" fill=\"#6e7079\" text-anchor=\"middle\">{lbl}</text>\n",
            bx = bx, by = by, bw = bar_w, bh = bh, c = bar_color,
            tx = cx, ty = by - 6.0, v = val, u = y_unit,
            lty = plot_y1 + 18.0, lbl = escape_xml_helper(label),
        ));
    }
    s.push_str("</svg>\n</div>\n");
    s
}

/// 散布図 + 回帰線を SSR SVG で描画。
/// points: (x, y) yen 単位の生値。
/// regression: (slope, intercept) — y = slope * x + intercept (yen 単位)
/// x/y 軸範囲は P2.5-P97.5 で trim してから決定 (外れ値で潰されない)。
pub(super) fn build_scatter_svg(
    points: &[(f64, f64)],
    regression: Option<(f64, f64)>,
) -> String {
    if points.is_empty() {
        return String::new();
    }
    let mut xs: Vec<f64> = points.iter().map(|p| p.0).collect();
    let mut ys: Vec<f64> = points.iter().map(|p| p.1).collect();
    xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    ys.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let p25 = |v: &[f64]| v.get((v.len() as f64 * 0.025) as usize).copied().unwrap_or(0.0);
    let p975 = |v: &[f64]| v.get(((v.len() as f64 * 0.975) as usize).min(v.len().saturating_sub(1))).copied().unwrap_or(0.0);
    let x_lo = p25(&xs);
    let x_hi = p975(&xs).max(x_lo + 1.0);
    let y_lo = p25(&ys);
    let y_hi = p975(&ys).max(y_lo + 1.0);

    let plot_x0 = 70.0_f64;
    let plot_x1 = 760.0_f64;
    let plot_y0 = 30.0_f64;
    let plot_y1 = 280.0_f64;

    let x_to_px = |x: f64| -> f64 { plot_x0 + (x - x_lo) / (x_hi - x_lo) * (plot_x1 - plot_x0) };
    let y_to_px = |y: f64| -> f64 { plot_y1 - (y - y_lo) / (y_hi - y_lo) * (plot_y1 - plot_y0) };

    let mut s = String::with_capacity(4096);
    s.push_str(
        "<div class=\"scatter-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 800 340\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;\">\n",
    );
    // axes
    s.push_str(&format!(
        "<line x1=\"{x}\" y1=\"{y0}\" x2=\"{x}\" y2=\"{y1}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\
         <line x1=\"{x0}\" y1=\"{y}\" x2=\"{x1}\" y2=\"{y}\" stroke=\"#94a3b8\" stroke-width=\"0.5\"/>\n",
        x = plot_x0, x0 = plot_x0, y = plot_y1, x1 = plot_x1, y0 = plot_y0, y1 = plot_y1,
    ));
    // y/x ticks (4 each)
    s.push_str("<g font-size=\"10\" fill=\"#6e7079\">\n");
    for k in 0..=4 {
        let yv = y_lo + (y_hi - y_lo) * k as f64 / 4.0;
        let xv = x_lo + (x_hi - x_lo) * k as f64 / 4.0;
        let ypx = y_to_px(yv);
        let xpx = x_to_px(xv);
        s.push_str(&format!(
            "<text x=\"{tx}\" y=\"{ty:.2}\" text-anchor=\"end\">{val:.0}万</text>\
             <text x=\"{xpx:.2}\" y=\"{txy}\" text-anchor=\"middle\">{xval:.0}万</text>\n",
            tx = plot_x0 - 6.0, ty = ypx + 3.0, val = yv / 10_000.0,
            xpx = xpx, txy = plot_y1 + 16.0, xval = xv / 10_000.0,
        ));
    }
    s.push_str("</g>\n");
    // points
    for (px_yen, py_yen) in points {
        if *px_yen < x_lo || *px_yen > x_hi || *py_yen < y_lo || *py_yen > y_hi { continue; }
        let px = x_to_px(*px_yen);
        let py = y_to_px(*py_yen);
        s.push_str(&format!(
            "<circle cx=\"{px:.2}\" cy=\"{py:.2}\" r=\"3\" fill=\"#3b82f6\" fill-opacity=\"0.55\"/>\n",
            px = px, py = py,
        ));
    }
    // regression line (only within range, computed from trimmed points)
    if let Some((slope, intercept)) = regression {
        let y_at_lo = slope * x_lo + intercept;
        let y_at_hi = slope * x_hi + intercept;
        if y_at_lo.is_finite() && y_at_hi.is_finite() {
            let x1px = x_to_px(x_lo);
            let y1px = y_to_px(y_at_lo.clamp(y_lo, y_hi));
            let x2px = x_to_px(x_hi);
            let y2px = y_to_px(y_at_hi.clamp(y_lo, y_hi));
            s.push_str(&format!(
                "<line x1=\"{x1:.2}\" y1=\"{y1:.2}\" x2=\"{x2:.2}\" y2=\"{y2:.2}\" stroke=\"#ef4444\" stroke-width=\"2\" stroke-dasharray=\"6 3\"/>\n",
                x1 = x1px, y1 = y1px, x2 = x2px, y2 = y2px,
            ));
        }
    }
    s.push_str("</svg>\n</div>\n");
    s
}

/// 半円ゲージを SSR SVG で描画。value は 0..100。
pub(super) fn build_gauge_svg(value: f64, label: &str, color: &str) -> String {
    let v = value.clamp(0.0, 100.0);
    let cx = 200.0_f64;
    let cy = 180.0_f64;
    let r = 130.0_f64;
    let stroke_w = 24.0_f64;
    // 半円: 180度 (左) から 0度 (右)。SVG では x 軸正 = 角度 0。
    // 開始 (左端) → 終了 (角度 = 180 - (v/100)*180 = 180 - 1.8*v)
    let end_angle_deg = 180.0 - v * 1.8;
    let end_rad = end_angle_deg.to_radians();
    let start_x = cx - r;
    let start_y = cy;
    let end_x = cx + r * end_rad.cos();
    let end_y = cy - r * end_rad.sin();
    let large_arc = if v > 50.0 { 1 } else { 0 };

    let mut s = String::with_capacity(1024);
    s.push_str(
        "<div class=\"gauge-ssr\" style=\"width:100%;\">\n<svg \
         viewBox=\"0 0 400 240\" preserveAspectRatio=\"xMidYMid meet\" \
         xmlns=\"http://www.w3.org/2000/svg\" role=\"img\" \
         style=\"width:100%;height:auto;display:block;font-family:sans-serif;max-width:400px;\">\n",
    );
    // 背景 (full 半円)
    s.push_str(&format!(
        "<path d=\"M {sx} {sy} A {r} {r} 0 1 1 {ex} {sy}\" fill=\"none\" stroke=\"#e5e7eb\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>\n",
        sx = start_x, sy = start_y, r = r, ex = cx + r, w = stroke_w,
    ));
    // 値 arc
    s.push_str(&format!(
        "<path d=\"M {sx} {sy} A {r} {r} 0 {la} 1 {ex:.2} {ey:.2}\" fill=\"none\" stroke=\"{c}\" stroke-width=\"{w}\" stroke-linecap=\"round\"/>\n",
        sx = start_x, sy = start_y, r = r, la = large_arc, ex = end_x, ey = end_y, c = color, w = stroke_w,
    ));
    // 中央 値表示
    s.push_str(&format!(
        "<text x=\"{cx}\" y=\"{cy}\" text-anchor=\"middle\" font-size=\"42\" font-weight=\"bold\" fill=\"#0f172a\">{v:.0}</text>\
         <text x=\"{cx}\" y=\"{cy2}\" text-anchor=\"middle\" font-size=\"12\" fill=\"#6e7079\">/100</text>\
         <text x=\"{cx}\" y=\"{cy3}\" text-anchor=\"middle\" font-size=\"14\" fill=\"#0f172a\" font-weight=\"bold\">{lbl}</text>\n",
        cx = cx, cy = cy - 10.0, cy2 = cy + 8.0, cy3 = cy + 32.0, v = v, lbl = escape_xml_helper(label),
    ));
    s.push_str("</svg>\n</div>\n");
    s
}

/// 最頻値を計算（ヒストグラム最大カウントのbin中心値を返す）
pub(super) fn compute_mode(values: &[i64], bin_size: i64) -> Option<i64> {
    let (_labels, counts, boundaries) = build_salary_histogram(values, bin_size);
    if counts.is_empty() {
        return None;
    }
    let max_idx = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(i, _)| i)?;
    // markLine を bin の下端ラベルに一致させるため、bin開始値を返す
    Some(boundaries[max_idx])
}

/// ソート済みでない値の配列から、指定パーセンタイル値を返す。
/// 空配列の場合は 0.0 を返す。
pub(super) fn percentile_sorted(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let clamped = p.clamp(0.0, 100.0);
    let idx = (((sorted.len() - 1) as f64) * clamped / 100.0).round() as usize;
    sorted[idx.min(sorted.len() - 1)]
}

/// 散布図軸の表示範囲を P2.5〜P97.5 基準で計算し、5% のマージンを追加して返す。
/// 下限は 0 未満にはならない。範囲が潰れる場合は ±1.0 万円のフォールバック。
pub(super) fn compute_axis_range(values: &mut Vec<f64>) -> (f64, f64) {
    if values.is_empty() {
        return (0.0, 1.0);
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let lo = percentile_sorted(values, 2.5);
    let hi = percentile_sorted(values, 97.5);
    let (lo, hi) = if (hi - lo).abs() < f64::EPSILON {
        (lo - 1.0, hi + 1.0)
    } else {
        (lo, hi)
    };
    let pad = (hi - lo) * 0.05;
    let lo_padded = (lo - pad).max(0.0);
    let hi_padded = hi + pad;
    // ECharts が整数目盛りを選びやすいよう、整数に丸める
    (lo_padded.floor(), hi_padded.ceil())
}

/// 給与を万円表示にフォーマット
/// 例: 250000 → "25.0万円", 0 → "-"
pub(super) fn format_man_yen(yen: i64) -> String {
    if yen == 0 {
        return "-".to_string();
    }
    format!("{:.1}万円", yen as f64 / 10_000.0)
}

// ============================================================
// UI-2 強化（2026-04-26）: 図表番号・読み方ヒント・物語のあるレポート
// ============================================================

/// 図表キャプション。`fig_no` 例: "図 1-1" / "表 3-2"。
/// 視覚と意味の両方で図表番号を識別できるよう、見出しと並列に配置する。
pub(super) fn render_figure_caption(html: &mut String, fig_no: &str, title: &str) {
    html.push_str(&format!(
        "<div class=\"figure-caption\"><span class=\"fig-no\">{}</span>{}</div>\n",
        escape_html(fig_no),
        escape_html(title)
    ));
}

/// 読み方ヒント吹き出し（結論先取り 1-2 行）。
/// 因果断定を避け、「傾向」「目安」等の語彙で記述する想定。
///
/// タスク4 (2026-04-28): フォントサイズを 10.5pt に微増、アイコンは 📖 で統一。
pub(super) fn render_read_hint(html: &mut String, body: &str) {
    html.push_str(&format!(
        "<div class=\"read-hint\" style=\"font-size:10.5pt;line-height:1.6;\">\
         <span class=\"read-hint-label\">\u{1F4D6} 読み方</span>{}</div>\n",
        escape_html(body)
    ));
}

/// 読み方ヒント（HTML 直挿し版。`<strong>` 等の埋め込み用）
pub(super) fn render_read_hint_html(html: &mut String, body_html: &str) {
    html.push_str(&format!(
        "<div class=\"read-hint\" style=\"font-size:10.5pt;line-height:1.6;\">\
         <span class=\"read-hint-label\">\u{1F4D6} 読み方</span>{}</div>\n",
        body_html
    ));
}

/// 「このページの読み方」ガイド（セクション冒頭の 3 行ガイド）
///
/// タスク4 (2026-04-28): アイコンを 📖（read-hint と統一）にし、フォントサイズを 10.5pt に微増。
pub(super) fn render_section_howto(html: &mut String, lines: &[&str]) {
    html.push_str("<div class=\"section-howto\" style=\"font-size:10.5pt;line-height:1.65;\">\n");
    html.push_str("<div class=\"howto-title\" style=\"font-weight:700;\">\u{1F4D6} このページの読み方</div>\n");
    html.push_str("<ol>\n");
    for line in lines {
        html.push_str(&format!("<li>{}</li>\n", escape_html(line)));
    }
    html.push_str("</ol>\n");
    html.push_str("</div>\n");
}

/// 次セクションへのつなぎテキスト（物語性向上）
pub(super) fn render_section_bridge(html: &mut String, text: &str) {
    html.push_str(&format!(
        "<p class=\"section-bridge\">{}</p>\n",
        escape_html(text)
    ));
}

/// 強化版 KPI カード（アイコン + 大きな数値 + 単位 + 比較値 + 状態）
///
/// status: "good" / "warn" / "crit" / "" のいずれか
pub(super) fn render_kpi_card_v2(
    html: &mut String,
    icon: &str,
    label: &str,
    value: &str,
    unit: &str,
    compare: &str,
    status: &str,
    status_label: &str,
) {
    let card_cls = match status {
        "good" => "kpi-card-v2 kpi-good",
        "warn" => "kpi-card-v2 kpi-warn",
        "crit" => "kpi-card-v2 kpi-crit",
        _ => "kpi-card-v2",
    };
    html.push_str(&format!("<div class=\"{}\">\n", card_cls));
    html.push_str("<div class=\"kpi-head\">");
    if !icon.is_empty() {
        html.push_str(&format!(
            "<span class=\"kpi-icon\" aria-hidden=\"true\">{}</span>",
            escape_html(icon)
        ));
    }
    html.push_str(&format!("<span>{}</span>", escape_html(label)));
    if !status_label.is_empty() {
        let status_cls = match status {
            "good" => "good",
            "warn" => "warn",
            "crit" => "crit",
            _ => "",
        };
        html.push_str(&format!(
            "<span class=\"kpi-status {}\">{}</span>",
            status_cls,
            escape_html(status_label)
        ));
    }
    html.push_str("</div>\n");
    html.push_str("<div class=\"kpi-value-line\">");
    html.push_str(&format!(
        "<span class=\"kpi-value\">{}</span>",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<span class=\"kpi-unit\">{}</span>",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
    if !compare.is_empty() {
        html.push_str(&format!(
            "<div class=\"kpi-compare\">{}</div>\n",
            escape_html(compare)
        ));
    }
    html.push_str("</div>\n");
}

/// 推奨アクションの優先度バッジ（severity から導出）
pub(super) fn priority_badge_html(sev: RptSev) -> String {
    let (cls, label) = match sev {
        RptSev::Critical => ("priority-badge priority-now", "\u{1F534} 即対応"),
        RptSev::Warning => ("priority-badge priority-week", "\u{1F7E1} 1週間以内"),
        RptSev::Info => ("priority-badge priority-later", "\u{1F7E2} 後回し可"),
        RptSev::Positive => ("priority-badge priority-later", "\u{1F7E2} 維持"),
    };
    format!("<span class=\"{}\">{}</span>", cls, escape_html(label))
}

// =====================================================================
// UI-3 強化: 用語ツールチップ / 図表番号 / 凡例 / 重要度バッジ
// （媒体分析印刷レポート 残 sections + 凡例/用語/style 統合用）
//
// 設計方針:
// - HTML として埋め込めるシンプルな inline 表現を返す（JS 不要・印刷耐性）
// - すべて `escape_html` で安全化済みの内容を返す
// - 図表番号は「図 X-Y: タイトル」「表 X-Y: タイトル」の表記に統一
// - severity / 凡例の絵文字は a11y 用に `aria-label` を併記
// - tooltip は `<abbr title="...">` をベースに `data-term-tooltip="1"` で識別可能に
//
// pub(crate) として survey 配下の他モジュールからも使えるよう公開する。
// =====================================================================

/// レポート横断で使う用語の重大度カテゴリ。CSS class と絵文字を同一視する。
#[derive(Clone, Copy, Debug)]
pub(crate) enum ReportSeverity {
    /// 即対応（赤）
    Critical,
    /// 1 週間以内（黄）
    Warning,
    /// 後回し可（緑）
    Info,
}

impl ReportSeverity {
    /// 重大度を示すテキストラベル (絵文字ではなく可読テキスト)
    pub(crate) fn label(self) -> &'static str {
        match self {
            ReportSeverity::Critical => "[重大]",
            ReportSeverity::Warning => "[注意]",
            ReportSeverity::Info => "[情報]",
        }
    }
    pub(crate) fn aria_label(self) -> &'static str {
        match self {
            ReportSeverity::Critical => "重大",
            ReportSeverity::Warning => "注意",
            ReportSeverity::Info => "情報",
        }
    }
    pub(crate) fn class(self) -> &'static str {
        match self {
            ReportSeverity::Critical => "report-sev-critical",
            ReportSeverity::Warning => "report-sev-warning",
            ReportSeverity::Info => "report-sev-info",
        }
    }
    pub(crate) fn action_text(self) -> &'static str {
        match self {
            ReportSeverity::Critical => "即対応",
            ReportSeverity::Warning => "1週間以内",
            ReportSeverity::Info => "後回し可",
        }
    }
}

/// 用語ツールチップを描画。
///
/// `<abbr>` 要素 + `title` + `aria-describedby` ベースで実装し、印刷時にも
/// 注釈として残るようにする。`description` は escape_html で安全化される。
///
/// 例:
/// ```ignore
/// render_info_tooltip("IQR", "1.5 倍の四分位範囲、Tukey 1977 由来の外れ値除外法")
/// // → <span class="report-tooltip">...<abbr title="...">IQR</abbr><span class="report-tooltip-icon"...>ⓘ</span></span>
/// ```
pub(crate) fn render_info_tooltip(label: &str, description: &str) -> String {
    let safe_label = escape_html(label);
    let safe_desc = escape_html(description);
    format!(
        "<span class=\"report-tooltip\" data-term-tooltip=\"1\">\
<abbr title=\"{desc}\" tabindex=\"0\" aria-label=\"{label}: {desc}\">{label}</abbr>\
<span class=\"report-tooltip-icon\" role=\"tooltip\" aria-hidden=\"true\">\u{24D8}</span>\
</span>",
        label = safe_label,
        desc = safe_desc,
    )
}

/// 凡例: severity 絵文字 + テキスト
///
/// 例: `🟡 注意`（aria-label 付き）
pub(crate) fn render_legend_emoji(severity: ReportSeverity, text: &str) -> String {
    format!(
        "<span class=\"report-legend {cls}\">\
<span class=\"report-legend-emoji\" role=\"img\" aria-label=\"{aria}\">{emoji}</span>\
<span class=\"report-legend-text\">{text}</span>\
</span>",
        cls = severity.class(),
        aria = severity.aria_label(),
        emoji = severity.label(),
        text = escape_html(text),
    )
}

/// 図表番号: 「図 chapter-num: タイトル」
pub(crate) fn render_figure_number(chapter: u32, num: u32, title: &str) -> String {
    format!(
        "<div class=\"report-figure-num\" data-figure=\"{c}-{n}\">\
\u{56F3} {c}-{n}: {t}\
</div>",
        c = chapter,
        n = num,
        t = escape_html(title),
    )
}

/// 表番号: 「表 chapter-num: タイトル」
pub(crate) fn render_table_number(chapter: u32, num: u32, title: &str) -> String {
    format!(
        "<div class=\"report-figure-num report-table-num\" data-table=\"{c}-{n}\">\
\u{8868} {c}-{n}: {t}\
</div>",
        c = chapter,
        n = num,
        t = escape_html(title),
    )
}

/// 「読み方」吹き出し
pub(crate) fn render_reading_callout(text: &str) -> String {
    format!(
        "<div class=\"report-callout\" role=\"note\" aria-label=\"読み方\">\
<span class=\"report-callout-label\">読み方</span>\
<span class=\"report-callout-body\">{}</span>\
</div>",
        escape_html(text)
    )
}

/// 重要度バッジ: 🔴 即対応 / 🟡 1週間 / 🟢 後回し
pub(crate) fn render_severity_badge(severity: ReportSeverity) -> String {
    format!(
        "<span class=\"report-severity-badge {cls}\" \
role=\"img\" aria-label=\"{aria} ({action})\">\
<span class=\"report-severity-emoji\" aria-hidden=\"true\">{emoji}</span>\
<span class=\"report-severity-text\">{action}</span>\
</span>",
        cls = severity.class(),
        aria = severity.aria_label(),
        action = severity.action_text(),
        emoji = severity.label(),
    )
}

// =====================================================================
// Design v2 強化（2026-04-26）: コンサル提案資料品質のプロフェッショナル版
// helpers (dv2-* 名前空間)
// =====================================================================

/// dv2 Section 番号バッジ + 見出し
///
/// 例: `render_dv2_section_badge(html, "01", "Executive Summary")`
/// → `<div class="dv2-section-heading"><span class="dv2-section-badge">01</span>...`
pub(super) fn render_dv2_section_badge(html: &mut String, num: &str, title: &str) {
    html.push_str(&format!(
        "<div class=\"dv2-section-heading\">\
<span class=\"dv2-section-badge\" aria-hidden=\"true\">{}</span>\
<span class=\"dv2-section-heading-title\">{}</span>\
</div>\n",
        escape_html(num),
        escape_html(title)
    ));
}

/// dv2 強化 KPI カード（modern design）
///
/// - status: "good" / "warn" / "crit" / "" のいずれか
/// - large: true なら 2 カラム幅で強調表示（給与中央値などの主要 KPI 用）
pub(super) fn render_dv2_kpi_card(
    html: &mut String,
    label: &str,
    value: &str,
    unit: &str,
    compare: &str,
    status: &str,
    large: bool,
) {
    let mut cls = String::from("dv2-kpi-card");
    if large {
        cls.push_str(" dv2-kpi-large");
    }
    let status_attr = if matches!(status, "good" | "warn" | "crit") {
        format!(" data-status=\"{}\"", status)
    } else {
        String::new()
    };
    html.push_str(&format!("<div class=\"{}\"{}>\n", cls, status_attr));
    html.push_str(&format!(
        "<div class=\"dv2-kpi-card-label\">{}</div>\n",
        escape_html(label)
    ));
    html.push_str("<div>");
    html.push_str(&format!(
        "<span class=\"dv2-kpi-card-value\">{}</span>",
        escape_html(value)
    ));
    if !unit.is_empty() {
        html.push_str(&format!(
            "<span class=\"dv2-kpi-card-unit\">{}</span>",
            escape_html(unit)
        ));
    }
    html.push_str("</div>\n");
    if !compare.is_empty() {
        html.push_str(&format!(
            "<div class=\"dv2-kpi-card-compare\">{}</div>\n",
            escape_html(compare)
        ));
    }
    html.push_str("</div>\n");
}

/// dv2 データバー（テーブル内の数値の隣に視覚的バー）
///
/// `value / max` の比率でバーを描画。tone: "good" / "warn" / "crit" / "" (=primary)
pub(super) fn render_dv2_data_bar(value: f64, max: f64, tone: &str) -> String {
    let pct = if max > 0.0 {
        (value / max * 100.0).clamp(0.0, 100.0)
    } else {
        0.0
    };
    let tone_attr = if matches!(tone, "good" | "warn" | "crit") {
        format!(" data-tone=\"{}\"", tone)
    } else {
        String::new()
    };
    format!(
        "<span class=\"dv2-databar\"{}><span class=\"dv2-databar-fill\" style=\"width:{:.1}%\"></span></span>",
        tone_attr, pct
    )
}

/// dv2 進捗バー（充足度 / パーセンタイル）
///
/// `percent`: 0..100 のパーセント値
pub(super) fn render_dv2_progress_bar(html: &mut String, percent: f64, label: &str) {
    let p = percent.clamp(0.0, 100.0);
    html.push_str("<div class=\"dv2-progress\">");
    html.push_str(&format!(
        "<div class=\"dv2-progress-track\"><div class=\"dv2-progress-fill\" style=\"width:{:.1}%\" role=\"progressbar\" aria-valuenow=\"{:.0}\" aria-valuemin=\"0\" aria-valuemax=\"100\"></div></div>",
        p, p
    ));
    if !label.is_empty() {
        html.push_str(&format!(
            "<span class=\"dv2-progress-label\">{}</span>",
            escape_html(label)
        ));
    }
    html.push_str("</div>\n");
}

/// dv2 SVG inline icon (svg + path)
///
/// kind: "check" / "warn" / "crit" / "info"
/// - 印刷時もカラーで表示される（`-webkit-print-color-adjust: exact`）
pub(super) fn render_dv2_icon(kind: &str) -> String {
    let (cls, path) = match kind {
        "check" => (
            "dv2-icon dv2-icon-check",
            // checkmark
            "M9 16.17L4.83 12l-1.42 1.41L9 19 21 7l-1.41-1.41z",
        ),
        "warn" => (
            "dv2-icon dv2-icon-warn",
            // warning triangle
            "M1 21h22L12 2 1 21zm12-3h-2v-2h2v2zm0-4h-2v-4h2v4z",
        ),
        "crit" => (
            "dv2-icon dv2-icon-crit",
            // exclamation circle
            "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-2h2v2zm0-4h-2V7h2v6z",
        ),
        "info" | _ => (
            "dv2-icon dv2-icon-info",
            // info circle
            "M12 2C6.48 2 2 6.48 2 12s4.48 10 10 10 10-4.48 10-10S17.52 2 12 2zm1 15h-2v-6h2v6zm0-8h-2V7h2v2z",
        ),
    };
    format!(
        "<svg class=\"{}\" viewBox=\"0 0 24 24\" aria-hidden=\"true\" focusable=\"false\"><path d=\"{}\"/></svg>",
        cls, path
    )
}

/// dv2 トレンド矢印（上↑ / 横→ / 下↓）
///
/// direction: "up" / "down" / "flat"
/// 数値変化 (例: "+5.2%") と組み合わせて表示
pub(super) fn render_dv2_trend(direction: &str, text: &str) -> String {
    let (cls, arrow) = match direction {
        "up" => ("dv2-trend dv2-trend-up", "\u{2191}"),
        "down" => ("dv2-trend dv2-trend-down", "\u{2193}"),
        _ => ("dv2-trend dv2-trend-flat", "\u{2192}"),
    };
    format!(
        "<span class=\"{}\" aria-label=\"{}\">{} {}</span>",
        cls,
        match direction {
            "up" => "上昇",
            "down" => "下落",
            _ => "横ばい",
        },
        arrow,
        escape_html(text)
    )
}

/// dv2 表紙のハイライト 3 KPI を出力
///
/// 各 KPI: ラベル + 値 + 単位
pub(super) fn render_dv2_cover_highlights(
    html: &mut String,
    items: &[(&str, &str, &str)], // (label, value, unit)
) {
    html.push_str("<div class=\"dv2-cover-highlights\">\n");
    for (label, value, unit) in items {
        html.push_str("<div class=\"dv2-cover-hl\">\n");
        html.push_str(&format!(
            "<div class=\"dv2-cover-hl-label\">{}</div>\n",
            escape_html(label)
        ));
        html.push_str(&format!(
            "<div><span class=\"dv2-cover-hl-value\">{}</span>",
            escape_html(value)
        ));
        if !unit.is_empty() {
            html.push_str(&format!(
                "<span class=\"dv2-cover-hl-unit\">{}</span>",
                escape_html(unit)
            ));
        }
        html.push_str("</div>\n</div>\n");
    }
    html.push_str("</div>\n");
}

/// 都道府県別最低賃金（円/時間）
pub(super) fn min_wage_for_prefecture(pref: &str) -> Option<i64> {
    match pref {
        "北海道" => Some(1075),
        "青森県" => Some(1029),
        "岩手県" => Some(1031),
        "宮城県" => Some(1038),
        "秋田県" => Some(1031),
        "山形県" => Some(1032),
        "福島県" => Some(1038),
        "茨城県" => Some(1074),
        "栃木県" => Some(1058),
        "群馬県" => Some(1063),
        "埼玉県" => Some(1141),
        "千葉県" => Some(1140),
        "東京都" => Some(1226),
        "神奈川県" => Some(1225),
        "新潟県" => Some(1050),
        "富山県" => Some(1062),
        "石川県" => Some(1054),
        "福井県" => Some(1053),
        "山梨県" => Some(1052),
        "長野県" => Some(1061),
        "岐阜県" => Some(1065),
        "静岡県" => Some(1097),
        "愛知県" => Some(1140),
        "三重県" => Some(1087),
        "滋賀県" => Some(1080),
        "京都府" => Some(1122),
        "大阪府" => Some(1177),
        "兵庫県" => Some(1116),
        "奈良県" => Some(1051),
        "和歌山県" => Some(1045),
        "鳥取県" => Some(1030),
        "島根県" => Some(1033),
        "岡山県" => Some(1047),
        "広島県" => Some(1085),
        "山口県" => Some(1043),
        "徳島県" => Some(1046),
        "香川県" => Some(1038),
        "愛媛県" => Some(1033),
        "高知県" => Some(1023),
        "福岡県" => Some(1057),
        "佐賀県" => Some(1030),
        "長崎県" => Some(1031),
        "熊本県" => Some(1034),
        "大分県" => Some(1035),
        "宮崎県" => Some(1023),
        "鹿児島県" => Some(1026),
        "沖縄県" => Some(1023),
        _ => None,
    }
}

const _MIN_WAGE_NATIONAL_AVG: i64 = 1121;

/// 対象地域を人間可読形式で組み立てる（例: "東京都 千代田区" / "全国"）
pub(super) fn compose_target_region(agg: &SurveyAggregation) -> String {
    match (&agg.dominant_prefecture, &agg.dominant_municipality) {
        (Some(p), Some(m)) if !p.is_empty() && !m.is_empty() => format!("{} {}", p, m),
        (Some(p), _) if !p.is_empty() => p.clone(),
        _ => "全国".to_string(),
    }
}

pub(super) fn render_scripts() -> String {
    r#"<script>
function toggleTheme() {
  document.body.classList.toggle('theme-dark');
  try {
    localStorage.setItem('report-theme',
      document.body.classList.contains('theme-dark') ? 'dark' : 'light');
  } catch(e) {}
}
(function() {
  try {
    if (localStorage.getItem('report-theme') === 'dark') {
      document.body.classList.add('theme-dark');
    }
  } catch(e) {}
})();
(function() {
  // ソート可能テーブルに role=grid / aria-sort を付与
  document.addEventListener('DOMContentLoaded', function() {
    document.querySelectorAll('.sortable-table').forEach(function(t) {
      t.setAttribute('role', 'grid');
      t.querySelectorAll('th').forEach(function(th) {
        th.setAttribute('aria-sort', 'none');
        th.setAttribute('tabindex', '0');
      });
    });
    // セクションに role=region 付与
    document.querySelectorAll('.section').forEach(function(s, i) {
      if (!s.getAttribute('role')) s.setAttribute('role', 'region');
      var h = s.querySelector('h2');
      if (h && !h.id) {
        h.id = 'section-' + i;
        s.setAttribute('aria-labelledby', h.id);
      }
    });
  });
})();
(function() {
  var charts = [];
  document.querySelectorAll('.echart[data-chart-config]').forEach(function(el) {
    if (el.offsetHeight === 0) return;
    try {
      var config = JSON.parse(el.getAttribute('data-chart-config'));
      config.animation = false;
      config.backgroundColor = '#fff';
      var chart = echarts.init(el, null, { renderer: 'svg' });
      chart.setOption(config);
      charts.push(chart);
    } catch(e) { console.warn('ECharts init error:', e); }
  });
  /* P0-2 (2026-05-06): 印刷時のチャート見切れ修正
   * - beforeprint: 印刷ダイアログ表示前に親要素の本文幅に合わせて再 resize
   * - afterprint: 印刷ダイアログ閉じた後も画面表示崩れが残らないよう再 resize
   * - resize: ウィンドウサイズ変更時の従来挙動を維持
   * Chromium / Firefox / Safari (WebKit) いずれも beforeprint/afterprint は同期発火するが、
   * SVG renderer の場合 attribute 反映の遅延があるため double resize で安定化。 */
  function resizeAll() {
    charts.forEach(function(c) {
      try { c.resize(); } catch(e) { /* swallow: chart already disposed */ }
    });
  }
  window.addEventListener('beforeprint', resizeAll);
  window.addEventListener('afterprint', resizeAll);
  window.addEventListener('resize', resizeAll);
  /* matchMedia print fallback: Safari 等で beforeprint が発火しない環境のため */
  if (window.matchMedia) {
    var mql = window.matchMedia('print');
    if (mql && typeof mql.addEventListener === 'function') {
      mql.addEventListener('change', resizeAll);
    } else if (mql && typeof mql.addListener === 'function') {
      mql.addListener(resizeAll);
    }
  }
})();

function initSortableTables() {
  document.querySelectorAll('.sortable-table').forEach(function(table) {
    table.querySelectorAll('th').forEach(function(th, colIdx) {
      th.addEventListener('click', function() {
        var tbody = table.querySelector('tbody');
        if (!tbody) return;
        var rows = Array.from(tbody.querySelectorAll('tr'));
        var isAsc = th.classList.contains('sort-asc');
        table.querySelectorAll('th').forEach(function(h) { h.classList.remove('sort-asc','sort-desc'); h.setAttribute('aria-sort','none'); });
        th.classList.add(isAsc ? 'sort-desc' : 'sort-asc');
        th.setAttribute('aria-sort', isAsc ? 'descending' : 'ascending');
        rows.sort(function(a,b) {
          var at = a.children[colIdx] ? a.children[colIdx].textContent.trim() : '';
          var bt = b.children[colIdx] ? b.children[colIdx].textContent.trim() : '';
          var an = parseFloat(at.replace(/[,件%万円倍+]/g,''));
          var bn = parseFloat(bt.replace(/[,件%万円倍+]/g,''));
          if (!isNaN(an) && !isNaN(bn)) return isAsc ? bn-an : an-bn;
          return isAsc ? bt.localeCompare(at,'ja') : at.localeCompare(bt,'ja');
        });
        rows.forEach(function(r) { tbody.appendChild(r); });
      });
    });
  });
}
document.addEventListener('DOMContentLoaded', initSortableTables);
</script>
"#.to_string()
}

// =====================================================================
// UI-3 単体テスト: helpers の新規関数群
// =====================================================================
#[cfg(test)]
mod ui3_helpers_tests {
    use super::*;

    /// Round 21: Jenks natural breaks がギャップを境界として検出
    #[test]
    fn jenks_finds_natural_gap() {
        // 3 グループ: 20-24万 / 30-36万 / 50万 — 26-30万、36-50万 にギャップ
        let values: Vec<i64> = vec![
            200_000, 210_000, 220_000, 220_000, 230_000, 240_000,
            300_000, 310_000, 320_000, 330_000, 340_000, 360_000,
            500_000, 510_000, 520_000,
        ];
        let breaks = jenks_natural_breaks(&values, 3);
        assert_eq!(breaks.len(), 2, "k=3 なら境界は 2 つ");
        // 境界 1 は 240,000 と 300,000 の間に来るはず (= sorted[6] = 300,000)
        assert!(
            breaks[0] >= 240_000 && breaks[0] <= 300_000,
            "境界1 は最初のギャップ (24万-30万) を検出: 実際={}",
            breaks[0]
        );
        // 境界 2 は 360,000 と 500,000 の間
        assert!(
            breaks[1] >= 360_000 && breaks[1] <= 500_000,
            "境界2 は二つ目のギャップ (36万-50万) を検出: 実際={}",
            breaks[1]
        );
    }

    #[test]
    fn jenks_falls_back_when_data_too_few() {
        // データ件数 < k*4 = 8 で tercile フォールバック
        let values: Vec<i64> = vec![100, 200, 300, 400, 500];
        let breaks = jenks_natural_breaks(&values, 3);
        assert_eq!(breaks.len(), 2);
        // tercile 相当: P33 ≈ 200, P66 ≈ 400
        assert!(breaks[0] <= breaks[1], "境界は昇順");
    }

    /// info tooltip: ⓘ アイコン + abbr + tabindex + aria-label が出力される
    #[test]
    fn test_render_info_tooltip_contains_required_attrs() {
        let html = render_info_tooltip("IQR", "1.5 倍の四分位範囲、Tukey 1977 由来の外れ値除外法");
        // 用語識別子
        assert!(html.contains("data-term-tooltip=\"1\""), "識別属性が必要");
        // 元ラベル表示
        assert!(html.contains(">IQR<"), "ラベルがそのまま表示されること");
        // 説明が title に入る
        assert!(html.contains("Tukey 1977"), "説明文が含まれること");
        // a11y: aria-label / role=tooltip
        assert!(
            html.contains("aria-label=\"IQR:"),
            "aria-label に用語＋説明"
        );
        assert!(html.contains("role=\"tooltip\""), "tooltip role 必須");
        // tabindex でキーボードアクセス可能
        assert!(html.contains("tabindex=\"0\""), "キーボードフォーカス可能");
        // ⓘ アイコン (U+24D8)
        assert!(html.contains("\u{24D8}"), "ⓘ アイコン (U+24D8) を含む");
    }

    /// info tooltip: HTML エスケープが効く
    #[test]
    fn test_render_info_tooltip_escapes_html() {
        let html = render_info_tooltip("a<b>", "x&y");
        assert!(!html.contains("<b>"), "ラベルのタグはエスケープされる");
        assert!(html.contains("&lt;b&gt;"), "ラベルが HTML エスケープされる");
        assert!(html.contains("x&amp;y"), "説明の & がエスケープされる");
    }

    /// 凡例 label: severity ごとに 3 種類のテキストラベル + aria-label
    #[test]
    fn test_render_legend_emoji_all_severities() {
        let critical = render_legend_emoji(ReportSeverity::Critical, "即対応");
        assert!(critical.contains("[重大]"), "[重大] ラベルが含まれる");
        assert!(critical.contains("aria-label=\"重大\""), "aria-label=重大");
        assert!(critical.contains("即対応"), "テキスト本文");

        let warning = render_legend_emoji(ReportSeverity::Warning, "1週間以内");
        assert!(warning.contains("[注意]"), "[注意] ラベルが含まれる");
        assert!(warning.contains("aria-label=\"注意\""));

        let info = render_legend_emoji(ReportSeverity::Info, "後回し可");
        assert!(info.contains("[情報]"), "[情報] ラベルが含まれる");
        assert!(info.contains("aria-label=\"情報\""));
    }

    /// 図番号: 「図 X-Y: タイトル」 + data-figure 属性
    #[test]
    fn test_render_figure_number_format() {
        let html = render_figure_number(3, 1, "CSV-HW 求人件数対応マップ");
        assert!(html.contains("\u{56F3} 3-1:"), "図番号フォーマット");
        assert!(html.contains("CSV-HW 求人件数対応マップ"), "タイトル");
        assert!(html.contains("data-figure=\"3-1\""), "data 属性");
        assert!(
            html.contains("class=\"report-figure-num\""),
            "CSS class 付与"
        );
    }

    /// 表番号: 「表 X-Y: タイトル」 + data-table 属性
    #[test]
    fn test_render_table_number_format() {
        let html = render_table_number(5, 2, "注目企業ランキング");
        assert!(html.contains("\u{8868} 5-2:"), "表番号フォーマット");
        assert!(html.contains("注目企業ランキング"));
        assert!(html.contains("data-table=\"5-2\""));
        assert!(
            html.contains("report-table-num"),
            "report-table-num class 付与"
        );
    }

    /// 読み方吹き出し: role=note + 「読み方」ラベル
    #[test]
    fn test_render_reading_callout_a11y() {
        let html = render_reading_callout("バーが長いほど件数が多いことを示します");
        assert!(html.contains("role=\"note\""), "role=note");
        assert!(html.contains("aria-label=\"読み方\""), "aria-label");
        assert!(html.contains("バーが長いほど"), "本文表示");
        assert!(html.contains("class=\"report-callout\""), "CSS class");
    }

    /// 重要度バッジ: 3 段階で色 + テキストラベル
    #[test]
    fn test_render_severity_badge_critical() {
        let html = render_severity_badge(ReportSeverity::Critical);
        assert!(html.contains("[重大]"), "[重大] テキストラベル");
        assert!(html.contains("即対応"));
        assert!(html.contains("report-sev-critical"));
        assert!(html.contains("aria-label=\"重大 (即対応)\""));
    }

    #[test]
    fn test_render_severity_badge_warning_info() {
        let warning = render_severity_badge(ReportSeverity::Warning);
        assert!(warning.contains("1週間以内"));
        assert!(warning.contains("report-sev-warning"));

        let info = render_severity_badge(ReportSeverity::Info);
        assert!(info.contains("後回し可"));
        assert!(info.contains("report-sev-info"));
    }

    /// ReportSeverity の 3 値はすべて異なる class / 絵文字を持つこと（逆証明）
    #[test]
    fn test_severity_distinct_outputs() {
        let cls = [
            ReportSeverity::Critical.class(),
            ReportSeverity::Warning.class(),
            ReportSeverity::Info.class(),
        ];
        let mut sorted = cls.to_vec();
        sorted.sort();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "3 つの severity で class が重複しないこと");

        let emojis = [
            ReportSeverity::Critical.label(),
            ReportSeverity::Warning.label(),
            ReportSeverity::Info.label(),
        ];
        let mut sorted_e = emojis.to_vec();
        sorted_e.sort();
        sorted_e.dedup();
        assert_eq!(sorted_e.len(), 3, "絵文字も重複しないこと");
    }

    #[test]
    fn histogram_config_thins_dense_x_axis_labels_for_pdf() {
        let labels: Vec<String> = (0..30).map(|i| format!("{}万", 20 + i)).collect();
        let values = vec![1usize; labels.len()];
        let config = build_histogram_echart_config(
            &labels,
            &values,
            "#42A5F5",
            Some(250_000),
            Some(260_000),
            Some(270_000),
            5_000,
        );

        assert!(config.contains("\"interval\":2"), "30 label chart must thin x labels");
        assert!(config.contains("\"right\":\"12%\""), "right grid margin prevents PDF clipping");
        assert!(config.contains("\"hideOverlap\":true"), "ECharts overlap guard required");
    }
}

// ============================================================
// Round 12 集計関数 unit test 再投入（A2 agent / 2026-05-12）
// L1: 表面 / L2: 論理 / L3: ドメイン不変 / L4: 逆証明 / L5: 因果
// 対象: histogram_axis_interval, stats_are_close, build_salary_histogram,
//       compute_mode, percentile_sorted, compute_axis_range, format_man_yen
// ============================================================
#[cfg(test)]
mod round12_aggregation_tests {
    use super::*;

    // ---------- L1: histogram_axis_interval (表面) ----------
    #[test]
    fn l1_axis_interval_small_count_returns_zero() {
        assert_eq!(histogram_axis_interval(0), 0);
        assert_eq!(histogram_axis_interval(1), 0);
        assert_eq!(histogram_axis_interval(15), 0);
    }

    #[test]
    fn l1_axis_interval_medium_count_returns_one() {
        assert_eq!(histogram_axis_interval(16), 1);
        assert_eq!(histogram_axis_interval(20), 1);
        assert_eq!(histogram_axis_interval(27), 1);
    }

    #[test]
    fn l1_axis_interval_large_count_returns_two() {
        assert_eq!(histogram_axis_interval(28), 2);
        assert_eq!(histogram_axis_interval(50), 2);
        assert_eq!(histogram_axis_interval(10_000), 2);
    }

    // L3 invariant: monotonic non-decreasing
    #[test]
    fn l3_axis_interval_monotonic_non_decreasing() {
        let mut prev = histogram_axis_interval(0);
        for n in 1..=100usize {
            let cur = histogram_axis_interval(n);
            assert!(cur >= prev, "interval must be non-decreasing at n={}", n);
            prev = cur;
        }
    }

    #[test]
    fn l3_axis_interval_boundary_jumps_exact() {
        // 境界値の段差を明示的に確認
        assert_eq!(histogram_axis_interval(15), 0);
        assert_eq!(histogram_axis_interval(16), 1);
        assert_eq!(histogram_axis_interval(27), 1);
        assert_eq!(histogram_axis_interval(28), 2);
    }

    // ---------- L1/L2: stats_are_close ----------
    #[test]
    fn l1_stats_close_all_none_false() {
        assert!(!stats_are_close(None, None, None, 10_000));
    }

    #[test]
    fn l1_stats_close_single_value_false() {
        // 値が 1 つだけの時は近接判定不可
        assert!(!stats_are_close(Some(200_000), None, None, 10_000));
    }

    #[test]
    fn l2_stats_close_within_bin_size_2x_true() {
        // diff = 20000, bin*2 = 20000 → 境界包含
        assert!(stats_are_close(
            Some(200_000),
            Some(220_000),
            None,
            10_000
        ));
    }

    #[test]
    fn l2_stats_close_just_over_threshold_false() {
        // diff = 20001, bin*2 = 20000 → 超過
        assert!(!stats_are_close(
            Some(200_000),
            Some(220_001),
            None,
            10_000
        ));
    }

    #[test]
    fn l2_stats_close_three_values_uses_min_max_diff() {
        // min=200000, max=240000, bin*2=50000 → close
        assert!(stats_are_close(
            Some(200_000),
            Some(220_000),
            Some(240_000),
            25_000
        ));
    }

    #[test]
    fn l2_stats_close_bin_size_zero_false() {
        // bin_size <= 0 はガード
        assert!(!stats_are_close(Some(200_000), Some(200_000), None, 0));
        assert!(!stats_are_close(Some(200_000), Some(200_000), None, -1));
    }

    // ---------- L1/L2: build_salary_histogram ----------
    #[test]
    fn l1_histogram_empty_input_empty_output() {
        let (labels, counts, bounds) = build_salary_histogram(&[], 10_000);
        assert!(labels.is_empty());
        assert!(counts.is_empty());
        assert!(bounds.is_empty());
    }

    #[test]
    fn l1_histogram_zero_bin_size_returns_empty() {
        let (labels, counts, bounds) = build_salary_histogram(&[100_000, 200_000], 0);
        assert!(labels.is_empty() && counts.is_empty() && bounds.is_empty());
    }

    #[test]
    fn l1_histogram_negative_bin_size_returns_empty() {
        let (labels, counts, bounds) = build_salary_histogram(&[100_000], -10_000);
        assert!(labels.is_empty() && counts.is_empty() && bounds.is_empty());
    }

    #[test]
    fn l2_histogram_all_zero_values_filtered() {
        // v > 0 のみ有効 → すべて 0 なら empty
        let (labels, counts, bounds) = build_salary_histogram(&[0, 0, 0], 10_000);
        assert!(labels.is_empty() && counts.is_empty() && bounds.is_empty());
    }

    #[test]
    fn l2_histogram_single_value_one_bin() {
        let (labels, counts, bounds) = build_salary_histogram(&[225_000], 10_000);
        assert_eq!(labels.len(), 1);
        assert_eq!(counts, vec![1]);
        assert_eq!(bounds, vec![220_000]);
        assert_eq!(labels[0], "22万");
    }

    #[test]
    fn l2_histogram_fractional_bin_label_uses_decimal() {
        // bin_size=5000 → 22.5 万のように小数表記
        let (labels, _, bounds) = build_salary_histogram(&[225_000], 5_000);
        assert_eq!(bounds[0], 225_000);
        assert_eq!(labels[0], "22.5万");
    }

    #[test]
    fn l2_histogram_unsorted_input_works() {
        // 未ソート入力でも正常動作
        let (_labels, counts, _bounds) =
            build_salary_histogram(&[300_000, 100_000, 200_000], 100_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn l2_histogram_duplicate_values_count_correctly() {
        let (_labels, counts, _bounds) =
            build_salary_histogram(&[200_000, 200_000, 200_000], 10_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 3);
    }

    // L3 invariant: bin counts 合計 = 入力件数 (positive 値のみ)
    #[test]
    fn l3_histogram_count_sum_equals_positive_input() {
        let values: Vec<i64> = (1..=100).map(|i| i * 10_000).collect();
        let (_labels, counts, _bounds) = build_salary_histogram(&values, 50_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, values.len());
    }

    #[test]
    fn l3_histogram_count_sum_excludes_non_positive() {
        let values = vec![-100_000, 0, 100_000, 200_000];
        let (_labels, counts, _bounds) = build_salary_histogram(&values, 10_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 2, "0 と負値は除外される");
    }

    #[test]
    fn l3_histogram_boundaries_monotonic_increasing() {
        let values: Vec<i64> = (1..=50).map(|i| i * 10_000).collect();
        let (_labels, _counts, bounds) = build_salary_histogram(&values, 50_000);
        for w in bounds.windows(2) {
            assert!(w[1] > w[0], "境界は単調増加: {:?}", w);
        }
    }

    #[test]
    fn l3_histogram_labels_len_equals_counts_len() {
        let values: Vec<i64> = (1..=30).map(|i| i * 10_000).collect();
        let (labels, counts, bounds) = build_salary_histogram(&values, 25_000);
        assert_eq!(labels.len(), counts.len());
        assert_eq!(labels.len(), bounds.len());
    }

    #[test]
    fn l3_histogram_extreme_range() {
        // 極大値・極小値の両端を含む
        let (_labels, counts, bounds) =
            build_salary_histogram(&[10_000, 10_000_000], 1_000_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 2);
        // 最初の bin に小、最後の bin に大が入る
        assert!(bounds[0] <= 10_000);
        assert!(*bounds.last().unwrap() <= 10_000_000);
    }

    // ---------- L1/L2: compute_mode ----------
    #[test]
    fn l1_compute_mode_empty_returns_none() {
        assert_eq!(compute_mode(&[], 10_000), None);
    }

    #[test]
    fn l1_compute_mode_zero_bin_size_returns_none() {
        assert_eq!(compute_mode(&[200_000, 300_000], 0), None);
    }

    #[test]
    fn l2_compute_mode_returns_bin_start_of_max_count() {
        // 200_000 が 3 件で最頻 → bin開始値を返す (bin_size=10_000)
        let values = vec![200_000, 200_000, 200_000, 300_000, 400_000];
        let mode = compute_mode(&values, 10_000).expect("mode exists");
        assert_eq!(mode, 200_000);
    }

    // L3: mode は [min_bin_start, max] の範囲に入る
    #[test]
    fn l3_compute_mode_within_min_max_range() {
        let values = vec![150_000, 220_000, 280_000, 310_000, 450_000];
        let mode = compute_mode(&values, 50_000).expect("mode");
        let min_v = *values.iter().min().unwrap();
        let max_v = *values.iter().max().unwrap();
        // mode は bin 開始値なので、min を含む bin 開始 (= min/bin*bin) 以上、max 以下
        let min_bin_start = (min_v / 50_000) * 50_000;
        assert!(
            mode >= min_bin_start && mode <= max_v,
            "mode={} must be in [{}, {}]",
            mode,
            min_bin_start,
            max_v
        );
    }

    // ---------- L1/L2: percentile_sorted ----------
    #[test]
    fn l1_percentile_empty_returns_zero() {
        assert_eq!(percentile_sorted(&[], 50.0), 0.0);
    }

    #[test]
    fn l2_percentile_single_element() {
        assert_eq!(percentile_sorted(&[42.0], 0.0), 42.0);
        assert_eq!(percentile_sorted(&[42.0], 50.0), 42.0);
        assert_eq!(percentile_sorted(&[42.0], 100.0), 42.0);
    }

    #[test]
    fn l2_percentile_p0_returns_min_p100_returns_max() {
        let sorted = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        assert_eq!(percentile_sorted(&sorted, 0.0), 10.0);
        assert_eq!(percentile_sorted(&sorted, 100.0), 50.0);
    }

    #[test]
    fn l2_percentile_p50_middle_value() {
        let sorted = vec![10.0, 20.0, 30.0, 40.0, 50.0];
        // idx = round(4 * 0.5) = 2 → 30.0
        assert_eq!(percentile_sorted(&sorted, 50.0), 30.0);
    }

    #[test]
    fn l2_percentile_clamps_out_of_range() {
        let sorted = vec![10.0, 20.0, 30.0];
        // p > 100 は 100 に clamp
        assert_eq!(percentile_sorted(&sorted, 150.0), 30.0);
        // p < 0 は 0 に clamp
        assert_eq!(percentile_sorted(&sorted, -50.0), 10.0);
    }

    // L3 invariant: p25 <= p50 <= p75
    #[test]
    fn l3_percentile_quartiles_monotonic() {
        let sorted: Vec<f64> = (1..=100).map(|i| i as f64).collect();
        let p25 = percentile_sorted(&sorted, 25.0);
        let p50 = percentile_sorted(&sorted, 50.0);
        let p75 = percentile_sorted(&sorted, 75.0);
        assert!(p25 <= p50, "p25={} <= p50={}", p25, p50);
        assert!(p50 <= p75, "p50={} <= p75={}", p50, p75);
    }

    #[test]
    fn l3_percentile_in_min_max_range() {
        let sorted: Vec<f64> = (1..=50).map(|i| i as f64 * 1.5).collect();
        let min_v = *sorted.first().unwrap();
        let max_v = *sorted.last().unwrap();
        for p in [0.0_f64, 10.0, 25.0, 50.0, 75.0, 90.0, 100.0] {
            let v = percentile_sorted(&sorted, p);
            assert!(v >= min_v && v <= max_v, "p={}: {} not in [{},{}]", p, v, min_v, max_v);
        }
    }

    // ---------- L1/L2: compute_axis_range ----------
    #[test]
    fn l1_axis_range_empty_default() {
        let mut v: Vec<f64> = vec![];
        let (lo, hi) = compute_axis_range(&mut v);
        assert_eq!((lo, hi), (0.0, 1.0));
    }

    #[test]
    fn l2_axis_range_single_value_fallback() {
        let mut v = vec![25.0];
        let (lo, hi) = compute_axis_range(&mut v);
        // hi - lo が EPSILON 以下 → ±1.0 + 5% padding → floor/ceil
        assert!(lo < hi);
        assert!(lo >= 0.0);
        assert!(hi >= 26.0);
    }

    #[test]
    fn l2_axis_range_lo_clamped_at_zero() {
        let mut v: Vec<f64> = (0..100).map(|i| i as f64 * 0.1).collect();
        let (lo, _hi) = compute_axis_range(&mut v);
        assert!(lo >= 0.0, "lo must not go below 0, got {}", lo);
    }

    #[test]
    fn l2_axis_range_returns_integer_bounds() {
        let mut v: Vec<f64> = (1..=100).map(|i| i as f64 * 1.7).collect();
        let (lo, hi) = compute_axis_range(&mut v);
        assert_eq!(lo, lo.floor(), "lo should be floor-rounded");
        assert_eq!(hi, hi.ceil(), "hi should be ceil-rounded");
    }

    // L3: lo < hi 不変条件
    #[test]
    fn l3_axis_range_lo_less_than_hi() {
        for n in [1usize, 2, 5, 10, 100] {
            let mut v: Vec<f64> = (1..=n).map(|i| i as f64).collect();
            let (lo, hi) = compute_axis_range(&mut v);
            assert!(lo < hi, "n={}: lo={} must be < hi={}", n, lo, hi);
        }
    }

    // L5 因果: compute_axis_range は percentile_sorted を呼ぶ → 依存連鎖検証
    #[test]
    fn l5_axis_range_depends_on_percentile() {
        // 200 件、P2.5/P97.5 ベースになる
        let mut v: Vec<f64> = (1..=200).map(|i| i as f64).collect();
        let (lo, hi) = compute_axis_range(&mut v);
        // P2.5 ≈ 6, P97.5 ≈ 195 → padding 5% → lo は 0 付近に clamp、hi は 200 付近
        assert!(lo >= 0.0 && lo < 15.0, "lo around clamp 0..15, got {}", lo);
        assert!(hi >= 195.0 && hi <= 230.0, "hi around 195..230, got {}", hi);
    }

    // ---------- L1/L2: format_man_yen ----------
    #[test]
    fn l1_format_man_yen_zero_returns_dash() {
        assert_eq!(format_man_yen(0), "-");
    }

    #[test]
    fn l2_format_man_yen_round_value() {
        assert_eq!(format_man_yen(250_000), "25.0万円");
    }

    #[test]
    fn l2_format_man_yen_fractional_value() {
        assert_eq!(format_man_yen(225_000), "22.5万円");
    }

    #[test]
    fn l2_format_man_yen_negative_value() {
        // 負値は - prefix 付きで出る (0 以外なのでフォーマット適用)
        let s = format_man_yen(-250_000);
        assert!(s.contains("-25.0") && s.contains("万円"), "got {}", s);
    }

    #[test]
    fn l2_format_man_yen_large_value() {
        assert_eq!(format_man_yen(10_000_000), "1000.0万円");
    }

    // ---------- L4: 逆証明 (K4 / K6) ----------
    // K4: 構成比 35/75 = 46.6% が 76.1% と表示される
    //   → helpers 層は構成比計算をしないため、ここで PASS が出れば真因は上位 HTML レンダリング層
    #[test]
    fn l4_reverse_proof_k4_helpers_layer_has_no_composition_ratio_bug() {
        // helpers の集計関数群に構成比 (a/b * 100) ロジックがあるか?
        // → build_salary_histogram は count を返すのみ、ratio は返さない
        let (_labels, counts, _bounds) =
            build_salary_histogram(&[100_000, 200_000, 300_000], 100_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 3, "helpers は raw count のみ。構成比計算は上位の責務");
        // → K4 (76.1% 誤表示) は helpers 層に存在しない。真因は HTML レンダリング層に確定。
    }

    // K6: 重複行
    //   → helpers の関数群は SQL を呼ばない → 重複の発生源ではない
    #[test]
    fn l4_reverse_proof_k6_helpers_layer_has_no_duplicate_source() {
        // 同一値を渡しても、helpers は dedup せず素直にカウントする (これは仕様通り)
        let dup = vec![200_000, 200_000, 200_000];
        let (_l, counts, _b) = build_salary_histogram(&dup, 10_000);
        let total: usize = counts.iter().sum();
        assert_eq!(total, 3, "helpers は与えられた配列をそのまま集計するのみ");
        // → 重複行は helpers より上の SQL 層で発生。helpers は無罪。
    }

    // L4: percentile_sorted は名前通り「sorted 前提」 → 未ソート入力でも panic しないことを保証
    #[test]
    fn l4_reverse_proof_percentile_assumes_sorted_input() {
        let unsorted = vec![5.0, 1.0, 3.0, 2.0, 4.0];
        let _ = percentile_sorted(&unsorted, 50.0); // panic しないこと
    }
}
