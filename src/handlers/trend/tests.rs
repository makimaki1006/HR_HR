//! trend モジュールのユニットテスト

use super::helpers::*;
use super::render::*;

// ========================================
// snapshot_label() テスト
// ========================================

#[test]
fn snapshot_label_normal() {
    // 通常ケース: 2024年1月
    assert_eq!(snapshot_label(202401), "2024/01");
}

#[test]
fn snapshot_label_december() {
    // 12月（2桁月の最大値）
    assert_eq!(snapshot_label(202412), "2024/12");
}

#[test]
fn snapshot_label_zero() {
    // ゼロ入力（エッジケース）
    assert_eq!(snapshot_label(0), "0/00");
}

#[test]
fn snapshot_label_single_digit_month() {
    // 1桁月が0埋めされるか
    assert_eq!(snapshot_label(202503), "2025/03");
}

#[test]
fn snapshot_label_large_year() {
    // 大きな年（将来の日付）
    assert_eq!(snapshot_label(203001), "2030/01");
}

// ========================================
// emp_group_color() テスト
// ========================================

#[test]
fn emp_group_color_seishain() {
    assert_eq!(emp_group_color("正社員"), "#3b82f6");
}

#[test]
fn emp_group_color_part() {
    assert_eq!(emp_group_color("パート"), "#f97316");
}

#[test]
fn emp_group_color_sonota() {
    // 「その他」はデフォルトブランチに該当
    assert_eq!(emp_group_color("その他"), "#8b5cf6");
}

#[test]
fn emp_group_color_unknown() {
    // 不明な文字列もデフォルトカラーを返す
    assert_eq!(emp_group_color("unknown"), "#8b5cf6");
}

#[test]
fn emp_group_color_empty() {
    // 空文字列もデフォルトカラー
    assert_eq!(emp_group_color(""), "#8b5cf6");
}

// ========================================
// echart_div() テスト
// ========================================

#[test]
fn echart_div_contains_data_chart_config() {
    let result = echart_div("{}", "300px");
    assert!(
        result.contains("data-chart-config"),
        "data-chart-config属性が含まれるべき"
    );
}

#[test]
fn echart_div_contains_height() {
    let result = echart_div("{}", "320px");
    assert!(result.contains("320px"), "高さの値が含まれるべき");
}

#[test]
fn echart_div_contains_echart_class() {
    let result = echart_div("{}", "300px");
    assert!(
        result.contains(r#"class="echart""#),
        "echartクラスが含まれるべき"
    );
}

#[test]
fn echart_div_json_with_double_quotes() {
    // ダブルクォートを含むJSONがシングルクォートのラッパー属性を壊さないか
    let config = r#"{"title":"テスト","value":42}"#;
    let result = echart_div(config, "300px");
    // シングルクォートでラップされているので、data-chart-config='...'の形式
    assert!(
        result.contains("data-chart-config='"),
        "シングルクォートラッパーが存在するべき"
    );
    assert!(result.contains(config), "JSON設定が含まれるべき");
}

#[test]
fn echart_div_width_100pct() {
    let result = echart_div("{}", "300px");
    assert!(result.contains("width:100%"), "幅が100%に設定されるべき");
}

// ========================================
// line_chart_config() テスト
// ========================================

#[test]
fn line_chart_config_valid_json() {
    let labels = vec!["2024/01".to_string(), "2024/02".to_string()];
    let series = vec![(
        "正社員".to_string(),
        "#3b82f6".to_string(),
        vec![100.0, 110.0],
    )];
    let result = line_chart_config("テスト", &labels, &series, "");
    // JSONとして有効であることを検証
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "出力が有効なJSONであるべき: {}", result);
}

#[test]
fn line_chart_config_contains_series_name() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![
        ("正社員".to_string(), "#3b82f6".to_string(), vec![100.0]),
        ("パート".to_string(), "#f97316".to_string(), vec![80.0]),
    ];
    let result = line_chart_config("テスト", &labels, &series, "");
    assert!(
        result.contains("正社員"),
        "シリーズ名「正社員」が含まれるべき"
    );
    assert!(
        result.contains("パート"),
        "シリーズ名「パート」が含まれるべき"
    );
}

#[test]
fn line_chart_config_nan_becomes_null() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![f64::NAN])];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    // NaNはnullに変換されるべき（"NaN"文字列ではない）
    let data = &parsed["series"][0]["data"][0];
    assert!(
        data.is_null(),
        "NaN入力はnullになるべき。実際の値: {}",
        data
    );
    assert!(
        !result.contains("\"NaN\""),
        "\"NaN\"文字列が含まれてはならない"
    );
}

#[test]
fn line_chart_config_infinity_becomes_null() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![(
        "テスト".to_string(),
        "#000".to_string(),
        vec![f64::INFINITY],
    )];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let data = &parsed["series"][0]["data"][0];
    assert!(data.is_null(), "Infinity入力はnullになるべき");
}

#[test]
fn line_chart_config_empty_series() {
    let labels = vec!["2024/01".to_string()];
    let series: Vec<(String, String, Vec<f64>)> = vec![];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(parsed["series"].is_array(), "seriesは配列であるべき");
    assert_eq!(
        parsed["series"].as_array().unwrap().len(),
        0,
        "空のseriesはサイズ0の配列"
    );
}

#[test]
fn line_chart_config_percent_format() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![50.0])];
    let result = line_chart_config("テスト", &labels, &series, "percent");
    assert!(result.contains('%'), "percentフォーマットは%を含むべき");
}

#[test]
fn line_chart_config_yen_format() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![200000.0])];
    let result = line_chart_config("テスト", &labels, &series, "yen");
    // ¥記号が含まれるか（エスケープされている可能性あり）
    assert!(
        result.contains('¥') || result.contains("\\u00a5"),
        "yenフォーマットは¥を含むべき"
    );
}

#[test]
fn line_chart_config_no_format() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![100.0])];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "フォーマットなしでも有効なJSON");
}

#[test]
fn line_chart_config_days_format() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![30.0])];
    let result = line_chart_config("テスト", &labels, &series, "days");
    assert!(result.contains('日'), "daysフォーマットは「日」を含むべき");
}

#[test]
#[allow(clippy::approx_constant)]
fn line_chart_config_rounding() {
    // 値が小数点2桁に丸められるか（PI近似ではなく小数丸めテスト）
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![3.14159])];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let val = parsed["series"][0]["data"][0].as_f64().unwrap();
    // *100 -> round -> /100 なので 3.14 になるべき
    assert!(
        (val - 3.14).abs() < 0.001,
        "値が小数2桁に丸められるべき。実際: {}",
        val
    );
}

#[test]
fn line_chart_config_title_present() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![1.0])];
    let result = line_chart_config("チャートタイトル", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(
        parsed["title"]["text"].as_str().unwrap(),
        "チャートタイトル"
    );
}

// ========================================
// stacked_area_config() テスト
// ========================================

#[test]
fn stacked_area_config_valid_json() {
    let labels = vec!["2024/01".to_string(), "2024/02".to_string()];
    let series = vec![
        (
            "正社員".to_string(),
            "#3b82f6".to_string(),
            vec![100.0, 120.0],
        ),
        (
            "パート".to_string(),
            "#f97316".to_string(),
            vec![50.0, 60.0],
        ),
    ];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "有効なJSONであるべき");
}

#[test]
fn stacked_area_config_has_stack_property() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![100.0])];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    // stackプロパティが設定されていることを確認
    assert_eq!(parsed["series"][0]["stack"].as_str().unwrap(), "total");
}

#[test]
fn stacked_area_config_has_area_style() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![100.0])];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        parsed["series"][0]["areaStyle"].is_object(),
        "areaStyleが存在するべき"
    );
}

#[test]
fn stacked_area_config_nan_handling() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![f64::NAN])];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        parsed["series"][0]["data"][0].is_null(),
        "NaNはnullになるべき"
    );
}

#[test]
fn stacked_area_config_rounds_values() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![123.456])];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let val = parsed["series"][0]["data"][0].as_f64().unwrap();
    // round() で整数に丸められるべき
    assert!(
        (val - 123.0).abs() < 0.001,
        "stacked_areaはround()で丸めるべき。実際: {}",
        val
    );
}

// ========================================
// stacked_bar_config() テスト
// ========================================

#[test]
fn stacked_bar_config_valid_json() {
    let labels = vec!["2024/01".to_string(), "2024/02".to_string()];
    let series = vec![
        ("新規".to_string(), "#22c55e".to_string(), vec![50.0, 60.0]),
        (
            "継続".to_string(),
            "#3b82f6".to_string(),
            vec![200.0, 210.0],
        ),
        ("終了".to_string(), "#ef4444".to_string(), vec![30.0, 25.0]),
    ];
    let result = stacked_bar_config("テスト", &labels, &series);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "有効なJSONであるべき");
}

#[test]
fn stacked_bar_config_type_is_bar() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![100.0])];
    let result = stacked_bar_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["series"][0]["type"].as_str().unwrap(), "bar");
}

#[test]
fn stacked_bar_config_nan_handling() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![f64::NAN])];
    let result = stacked_bar_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        parsed["series"][0]["data"][0].is_null(),
        "NaNはnullになるべき"
    );
}

#[test]
fn stacked_bar_config_has_stack_total() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![100.0])];
    let result = stacked_bar_config("テスト", &labels, &series);
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert_eq!(parsed["series"][0]["stack"].as_str().unwrap(), "total");
}

// ========================================
// TREND_SUBTABS 定数テスト
// ========================================

#[test]
fn trend_subtabs_has_5_entries() {
    assert_eq!(TREND_SUBTABS.len(), 5);
}

#[test]
fn trend_subtabs_ids_sequential() {
    for (i, (id, _)) in TREND_SUBTABS.iter().enumerate() {
        assert_eq!(*id as usize, i + 1, "サブタブIDは1から連番であるべき");
    }
}

// ========================================
// render_subtab フォールバックテスト (turso=None)
// ========================================

#[test]
fn render_subtab_1_no_turso_fallback() {
    let result = render_subtab_1(None, "");
    assert!(
        result.contains("Tursoデータベースに接続されていない"),
        "Turso未接続時にフォールバックメッセージが含まれるべき。実際: {}",
        result
    );
}

#[test]
fn render_subtab_2_no_turso_fallback() {
    let result = render_subtab_2(None, "");
    assert!(
        result.contains("Tursoデータベースに接続されていない"),
        "Turso未接続時にフォールバックメッセージが含まれるべき"
    );
}

#[test]
fn render_subtab_3_no_turso_fallback() {
    let result = render_subtab_3(None, "");
    assert!(
        result.contains("Tursoデータベースに接続されていない"),
        "Turso未接続時にフォールバックメッセージが含まれるべき"
    );
}

#[test]
fn render_subtab_4_no_turso_fallback() {
    let result = render_subtab_4(None, "");
    assert!(
        result.contains("Tursoデータベースに接続されていない"),
        "Turso未接続時にフォールバックメッセージが含まれるべき"
    );
}

#[test]
fn render_subtab_fallback_contains_warning_icon() {
    let result = render_subtab_1(None, "");
    assert!(
        result.contains("\u{26a0}\u{fe0f}"),
        "フォールバック表示に警告アイコンが含まれるべき"
    );
}

#[test]
fn render_subtab_fallback_is_html() {
    let result = render_subtab_1(None, "");
    assert!(
        result.contains("<div"),
        "フォールバック表示はHTML要素を含むべき"
    );
    assert!(result.contains("</div>"), "HTMLが閉じられているべき");
}

// ========================================
// 複合テスト: echart_div + line_chart_config
// ========================================

#[test]
fn echart_div_with_real_config() {
    // 実際のline_chart_configの出力をechart_divに渡した場合
    let labels = vec!["2024/01".to_string(), "2024/02".to_string()];
    let series = vec![(
        "正社員".to_string(),
        "#3b82f6".to_string(),
        vec![100.0, 110.0],
    )];
    let config = line_chart_config("テスト", &labels, &series, "");
    let div = echart_div(&config, "320px");

    assert!(
        div.contains("data-chart-config"),
        "data-chart-config属性が存在するべき"
    );
    assert!(div.contains("320px"), "高さが設定されるべき");
    // HTMLとして壊れていないか（シングルクォートが閉じているか）
    let count = div.matches("data-chart-config='").count();
    assert_eq!(count, 1, "data-chart-config属性は1つだけであるべき");
}

// ========================================
// エッジケース: 空ラベル・大量データ
// ========================================

#[test]
fn line_chart_config_empty_labels() {
    let labels: Vec<String> = vec![];
    let series = vec![("テスト".to_string(), "#000".to_string(), vec![])];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "空ラベルでも有効なJSON");
}

#[test]
fn line_chart_config_multiple_series() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![
        ("正社員".to_string(), "#3b82f6".to_string(), vec![100.0]),
        ("パート".to_string(), "#f97316".to_string(), vec![80.0]),
        ("その他".to_string(), "#8b5cf6".to_string(), vec![20.0]),
    ];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    let series_arr = parsed["series"].as_array().unwrap();
    assert_eq!(series_arr.len(), 3, "3つのシリーズが含まれるべき");
}

#[test]
fn line_chart_config_negative_infinity_becomes_null() {
    let labels = vec!["2024/01".to_string()];
    let series = vec![(
        "テスト".to_string(),
        "#000".to_string(),
        vec![f64::NEG_INFINITY],
    )];
    let result = line_chart_config("テスト", &labels, &series, "");
    let parsed: serde_json::Value = serde_json::from_str(&result).unwrap();
    assert!(
        parsed["series"][0]["data"][0].is_null(),
        "負の無限大もnullになるべき"
    );
}

#[test]
fn stacked_area_config_empty_series() {
    let labels = vec!["2024/01".to_string()];
    let series: Vec<(String, String, Vec<f64>)> = vec![];
    let result = stacked_area_config("テスト", &labels, &series);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "空シリーズでも有効なJSON");
}

#[test]
fn stacked_bar_config_empty_series() {
    let labels = vec!["2024/01".to_string()];
    let series: Vec<(String, String, Vec<f64>)> = vec![];
    let result = stacked_bar_config("テスト", &labels, &series);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&result);
    assert!(parsed.is_ok(), "空シリーズでも有効なJSON");
}
