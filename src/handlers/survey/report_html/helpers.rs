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
    let escaped = config_json.replace('\'', "&#39;");
    format!(
        "<div class=\"echart\" style=\"height:{}px;width:100%;\" data-chart-config='{}'></div>\n",
        height, escaped
    )
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

    let mut mark_lines = vec![];
    if let Some(m) = mean {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "平均",
            "lineStyle": {"color": "#e74c3c", "type": "dashed", "width": 2},
            "label": {"formatter": "平均", "fontSize": 10}
        }));
    }
    if let Some(m) = median {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "中央値",
            "lineStyle": {"color": "#27ae60", "type": "dashed", "width": 2},
            "label": {"formatter": "中央値", "fontSize": 10}
        }));
    }
    if let Some(m) = mode {
        mark_lines.push(json!({
            "xAxis": to_label(m),
            "name": "最頻値",
            "lineStyle": {"color": "#9b59b6", "type": "dashed", "width": 2},
            "label": {"formatter": "最頻値", "fontSize": 10}
        }));
    }

    let config = json!({
        "tooltip": {"trigger": "axis"},
        "xAxis": {
            "type": "category",
            "data": labels,
            "axisLabel": {"rotate": 30, "fontSize": 9}
        },
        "yAxis": {
            "type": "value",
            "axisLabel": {"fontSize": 9}
        },
        "grid": {"left": "10%", "right": "5%", "bottom": "20%", "top": "10%"},
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
pub(super) fn render_read_hint(html: &mut String, body: &str) {
    html.push_str(&format!(
        "<div class=\"read-hint\"><span class=\"read-hint-label\">\u{1F4D6} 読み方</span>{}</div>\n",
        escape_html(body)
    ));
}

/// 読み方ヒント（HTML 直挿し版。`<strong>` 等の埋め込み用）
pub(super) fn render_read_hint_html(html: &mut String, body_html: &str) {
    html.push_str(&format!(
        "<div class=\"read-hint\"><span class=\"read-hint-label\">\u{1F4D6} 読み方</span>{}</div>\n",
        body_html
    ));
}

/// 「このページの読み方」ガイド（セクション冒頭の 3 行ガイド）
pub(super) fn render_section_howto(html: &mut String, lines: &[&str]) {
    html.push_str("<div class=\"section-howto\">\n");
    html.push_str("<div class=\"howto-title\">\u{1F4DD} このページの読み方</div>\n");
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
  window.addEventListener('beforeprint', function() { charts.forEach(function(c) { c.resize(); }); });
  window.addEventListener('resize', function() { charts.forEach(function(c) { c.resize(); }); });
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
}
