//! ヘルパー関数・定数定義（trend モジュール内部用）

use std::collections::HashMap;
use serde_json::json;

/// サブタブ定義
pub(crate) const TREND_SUBTABS: [(u8, &str); 5] = [
    (1, "量の変化"),
    (2, "質の変化"),
    (3, "構造の変化"),
    (4, "シグナル"),
    (5, "外部比較"),
];

/// 雇用形態グループの色定義
pub(crate) fn emp_group_color(group: &str) -> &'static str {
    match group {
        "正社員" => "#3b82f6",
        "パート" => "#f97316",
        _ => "#8b5cf6",
    }
}

/// snapshot_id（YYYYMM整数）→ "YYYY/MM" ラベル変換
pub(crate) fn snapshot_label(id: i64) -> String {
    let year = id / 100;
    let month = id % 100;
    format!("{}/{:02}", year, month)
}

/// snapshot_id文字列をi64に変換
/// "202501" → 202501（そのままパース）
/// "2025-01" → 202501（ハイフン除去してパース）
pub(crate) fn parse_snapshot_id(row: &std::collections::HashMap<String, serde_json::Value>, key: &str) -> i64 {
    row.get(key)
        .and_then(|v| {
            // まずi64として試行
            v.as_i64().or_else(|| {
                v.as_str().and_then(|s| {
                    // "202501" 形式
                    s.parse::<i64>().ok().or_else(|| {
                        // "2025-01" 形式 → ハイフン除去
                        s.replace('-', "").parse::<i64>().ok()
                    })
                })
            })
        })
        .unwrap_or(0)
}

/// EChartsチャートの共通ラッパーHTML生成
pub(crate) fn echart_div(config_json: &str, height: &str) -> String {
    format!(
        "<div class=\"echart\" data-chart-config='{}' style=\"width:100%;height:{};\"></div>",
        config_json, height
    )
}

/// ECharts line chart 共通オプション生成（dark theme）
pub(crate) fn line_chart_config(
    title: &str,
    x_labels: &[String],
    series: &[(String, String, Vec<f64>)],
    y_format: &str,
) -> String {
    let series_arr: Vec<serde_json::Value> = series.iter().map(|(name, color, data)| {
        let data_vals: Vec<serde_json::Value> = data.iter().map(|v| {
            if v.is_nan() || v.is_infinite() { serde_json::Value::Null } else { json!((*v * 100.0).round() / 100.0) }
        }).collect();
        json!({
            "name": name,
            "type": "line",
            "smooth": true,
            "symbol": "circle",
            "symbolSize": 4,
            "lineStyle": {"width": 2, "color": color},
            "itemStyle": {"color": color},
            "data": data_vals
        })
    }).collect();

    let legend_data: Vec<&str> = series.iter().map(|(n, _, _)| n.as_str()).collect();

    let y_axis_label = match y_format {
        "percent" => json!({"formatter": "{value}%"}),
        "yen" => json!({"formatter": "¥{value}"}),
        "days" => json!({"formatter": "{value}日"}),
        _ => json!({"color": "#94a3b8"}),
    };

    let mut y_axis = json!({
        "type": "value",
        "splitLine": {"lineStyle": {"color": "#1e293b"}},
        "axisLabel": {"color": "#94a3b8"}
    });
    if y_format == "percent" || y_format == "yen" || y_format == "days" {
        y_axis["axisLabel"] = y_axis_label;
    }

    let config = json!({
        "title": {"text": title, "left": "center", "textStyle": {"color": "#e2e8f0", "fontSize": 14}},
        "tooltip": {"trigger": "axis", "backgroundColor": "rgba(15,23,42,0.95)", "borderColor": "#334155", "textStyle": {"color": "#e2e8f0"}},
        "legend": {"data": legend_data, "bottom": 0, "textStyle": {"color": "#94a3b8"}},
        "grid": {"left": "10%", "right": "5%", "top": "15%", "bottom": "15%"},
        "xAxis": {"type": "category", "data": x_labels, "axisLabel": {"color": "#94a3b8", "rotate": 45, "fontSize": 10}, "axisLine": {"lineStyle": {"color": "#334155"}}},
        "yAxis": y_axis,
        "series": series_arr
    });

    config.to_string()
}

/// ECharts stacked area chart 生成
pub(crate) fn stacked_area_config(
    title: &str,
    x_labels: &[String],
    series: &[(String, String, Vec<f64>)],
) -> String {
    let series_arr: Vec<serde_json::Value> = series.iter().map(|(name, color, data)| {
        let data_vals: Vec<serde_json::Value> = data.iter().map(|v| {
            if v.is_nan() || v.is_infinite() { serde_json::Value::Null } else { json!(v.round()) }
        }).collect();
        json!({
            "name": name,
            "type": "line",
            "stack": "total",
            "areaStyle": {"opacity": 0.4},
            "smooth": true,
            "symbol": "none",
            "lineStyle": {"width": 1.5, "color": color},
            "itemStyle": {"color": color},
            "data": data_vals
        })
    }).collect();

    let legend_data: Vec<&str> = series.iter().map(|(n, _, _)| n.as_str()).collect();

    let config = json!({
        "title": {"text": title, "left": "center", "textStyle": {"color": "#e2e8f0", "fontSize": 14}},
        "tooltip": {"trigger": "axis", "backgroundColor": "rgba(15,23,42,0.95)", "borderColor": "#334155", "textStyle": {"color": "#e2e8f0"}},
        "legend": {"data": legend_data, "bottom": 0, "textStyle": {"color": "#94a3b8"}},
        "grid": {"left": "10%", "right": "5%", "top": "15%", "bottom": "15%"},
        "xAxis": {"type": "category", "data": x_labels, "axisLabel": {"color": "#94a3b8", "rotate": 45, "fontSize": 10}, "axisLine": {"lineStyle": {"color": "#334155"}}},
        "yAxis": {"type": "value", "splitLine": {"lineStyle": {"color": "#1e293b"}}, "axisLabel": {"color": "#94a3b8"}},
        "series": series_arr
    });

    config.to_string()
}

/// ECharts stacked bar chart 生成
pub(crate) fn stacked_bar_config(
    title: &str,
    x_labels: &[String],
    series: &[(String, String, Vec<f64>)],
) -> String {
    let series_arr: Vec<serde_json::Value> = series.iter().map(|(name, color, data)| {
        let data_vals: Vec<serde_json::Value> = data.iter().map(|v| {
            if v.is_nan() || v.is_infinite() { serde_json::Value::Null } else { json!(v.round()) }
        }).collect();
        json!({
            "name": name,
            "type": "bar",
            "stack": "total",
            "itemStyle": {"color": color},
            "data": data_vals
        })
    }).collect();

    let legend_data: Vec<&str> = series.iter().map(|(n, _, _)| n.as_str()).collect();

    let config = json!({
        "title": {"text": title, "left": "center", "textStyle": {"color": "#e2e8f0", "fontSize": 14}},
        "tooltip": {"trigger": "axis", "backgroundColor": "rgba(15,23,42,0.95)", "borderColor": "#334155", "textStyle": {"color": "#e2e8f0"}},
        "legend": {"data": legend_data, "bottom": 0, "textStyle": {"color": "#94a3b8"}},
        "grid": {"left": "10%", "right": "5%", "top": "15%", "bottom": "15%"},
        "xAxis": {"type": "category", "data": x_labels, "axisLabel": {"color": "#94a3b8", "rotate": 45, "fontSize": 10}, "axisLine": {"lineStyle": {"color": "#334155"}}},
        "yAxis": {"type": "value", "splitLine": {"lineStyle": {"color": "#1e293b"}}, "axisLabel": {"color": "#94a3b8"}},
        "series": series_arr
    });

    config.to_string()
}

/// ECharts dual y-axis chart 生成（左軸: bar/line, 右軸: dashed line）
/// 外部統計データとHW時系列データを重ね合わせるためのチャート
pub(crate) fn dual_axis_chart_config(
    title: &str,
    x_labels: &[String],
    left_series: &[(String, String, Vec<f64>)],
    right_series: &[(String, String, Vec<f64>)],
    left_label: &str,
    right_label: &str,
) -> String {
    // 左軸シリーズ（実線、yAxisIndex: 0）
    let left_arr: Vec<serde_json::Value> = left_series.iter().map(|(name, color, data)| {
        let data_vals: Vec<serde_json::Value> = data.iter().map(|v| {
            if v.is_nan() || v.is_infinite() { serde_json::Value::Null } else { json!((*v * 100.0).round() / 100.0) }
        }).collect();
        json!({
            "name": name,
            "type": "line",
            "smooth": true,
            "symbol": "circle",
            "symbolSize": 4,
            "yAxisIndex": 0,
            "lineStyle": {"width": 2, "color": color},
            "itemStyle": {"color": color},
            "data": data_vals
        })
    }).collect();

    // 右軸シリーズ（破線、yAxisIndex: 1）
    let right_arr: Vec<serde_json::Value> = right_series.iter().map(|(name, color, data)| {
        let data_vals: Vec<serde_json::Value> = data.iter().map(|v| {
            if v.is_nan() || v.is_infinite() { serde_json::Value::Null } else { json!((*v * 100.0).round() / 100.0) }
        }).collect();
        json!({
            "name": name,
            "type": "line",
            "smooth": true,
            "symbol": "diamond",
            "symbolSize": 6,
            "yAxisIndex": 1,
            "lineStyle": {"width": 2, "type": "dashed", "color": color},
            "itemStyle": {"color": color},
            "data": data_vals
        })
    }).collect();

    let mut all_series = left_arr;
    all_series.extend(right_arr);

    // 凡例データ（左軸 + 右軸）
    let mut legend_data: Vec<&str> = left_series.iter().map(|(n, _, _)| n.as_str()).collect();
    legend_data.extend(right_series.iter().map(|(n, _, _)| n.as_str()));

    let config = json!({
        "title": {"text": title, "left": "center", "textStyle": {"color": "#e2e8f0", "fontSize": 14}},
        "tooltip": {
            "trigger": "axis",
            "backgroundColor": "rgba(15,23,42,0.95)",
            "borderColor": "#334155",
            "textStyle": {"color": "#e2e8f0"}
        },
        "legend": {"data": legend_data, "bottom": 0, "textStyle": {"color": "#94a3b8"}},
        "grid": {"left": "10%", "right": "10%", "top": "15%", "bottom": "15%"},
        "xAxis": {
            "type": "category",
            "data": x_labels,
            "axisLabel": {"color": "#94a3b8", "rotate": 45, "fontSize": 10},
            "axisLine": {"lineStyle": {"color": "#334155"}}
        },
        "yAxis": [
            {
                "type": "value",
                "name": left_label,
                "nameTextStyle": {"color": "#94a3b8"},
                "splitLine": {"lineStyle": {"color": "#1e293b"}},
                "axisLabel": {"color": "#94a3b8"}
            },
            {
                "type": "value",
                "name": right_label,
                "nameTextStyle": {"color": "#94a3b8"},
                "splitLine": {"show": false},
                "axisLabel": {"color": "#94a3b8"}
            }
        ],
        "series": all_series
    });

    config.to_string()
}

/// 年度データを月次スナップショットのX軸に合わせる（ステップ関数）
/// fiscal_years: ["2024", "2025", "2026"] のような年度文字列
/// values: 各年度に対応する値
/// monthly_snapshots: [202407, 202408, ..., 202603] のようなYYYYMM整数
/// 戻り値: 月次スナップショットに合わせた値ベクタ（年度の値をその年度の各月に繰り返す）
pub(crate) fn align_yearly_to_monthly(
    fiscal_years: &[String],
    values: &[f64],
    monthly_snapshots: &[i64],
) -> Vec<f64> {
    // 年度→値のマップを構築
    let mut fy_map: HashMap<i64, f64> = HashMap::new();
    for (fy_str, &val) in fiscal_years.iter().zip(values.iter()) {
        if let Ok(fy) = fy_str.parse::<i64>() {
            fy_map.insert(fy, val);
        }
    }

    // 各月次スナップショットに対応する年度を決定
    // 日本の会計年度: 4月始まり → YYYYMM が ????04〜????03 の場合
    // 例: 202404〜202503 → 2024年度
    monthly_snapshots.iter().map(|&snap| {
        let year = snap / 100;
        let month = snap % 100;
        // 4月以降はその年が年度、1-3月は前年が年度
        let fiscal_year = if month >= 4 { year } else { year - 1 };
        fy_map.get(&fiscal_year).copied().unwrap_or(f64::NAN)
    }).collect()
}
