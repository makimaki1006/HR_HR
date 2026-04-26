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
