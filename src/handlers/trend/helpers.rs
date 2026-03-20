//! ヘルパー関数・定数定義（trend モジュール内部用）

use serde_json::json;

/// サブタブ定義
pub(crate) const TREND_SUBTABS: [(u8, &str); 4] = [
    (1, "量の変化"),
    (2, "質の変化"),
    (3, "構造の変化"),
    (4, "シグナル"),
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
