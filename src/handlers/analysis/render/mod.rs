//! HTML描画関数 公開 API + dispatcher
//!
//! ## サブモジュール構成（リファクタ C-2: ファイル肥大化解消）
//! - `subtab1_recruit_trend`: 求人動向 (vacancy / resilience / transparency)
//! - `subtab2_salary`: 給与構造 (structure / competitiveness / compensation)
//! - `subtab3_text`: テキスト分析 (text_quality / keyword_profile / temperature)
//! - `subtab4_market_structure`: 市場構造 (employer_strategy / monopsony / spatial / competition / cascade)
//! - `subtab5_anomaly`: 異常値・外部 (anomaly + Phase 4-7 全 22 セクション + region_benchmark)
//! - `subtab6_forecast`: 予測・推定 (fulfillment / mobility / shadow_wage)
//! - `subtab7`: 通勤圏分析 (F2 で分離済)

#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::collections::HashMap;

#[cfg(test)]
type Row = HashMap<String, Value>;

// ======== サブモジュール宣言 (大規模ファイル分割) ========
mod subtab1_recruit_trend;
mod subtab2_salary;
mod subtab3_text;
mod subtab4_market_structure;
mod subtab5_anomaly;
mod subtab6_forecast;
mod subtab7;

// 公開 API: render_subtab_1..7 (handlers.rs から呼出)
pub(crate) use subtab1_recruit_trend::render_subtab_1;
pub(crate) use subtab2_salary::render_subtab_2;
pub(crate) use subtab3_text::render_subtab_3;
pub(crate) use subtab4_market_structure::render_subtab_4;
pub(crate) use subtab5_anomaly::render_subtab_5;
pub(crate) use subtab6_forecast::render_subtab_6;
pub(crate) use subtab7::render_subtab_7;

// テストモジュールから section 関数を呼び出すための再エクスポート (private)
// 現状テストは subtab5 系の Phase 4-7 関数のみ参照するため subtab5 のみ全公開
#[cfg(test)]
use subtab5_anomaly::*;

#[cfg(test)]
mod new_section_tests {
    use super::*;
    use serde_json::Value;
    use std::collections::HashMap;

    // -------- テストヘルパー --------

    /// 文字列ペアから Row (HashMap<String, Value>) を生成する
    fn make_row(pairs: &[(&str, &str)]) -> Row {
        let mut map = HashMap::new();
        for &(k, v) in pairs {
            map.insert(k.to_string(), Value::String(v.to_string()));
        }
        map
    }

    /// 整数値を持つキーを追加した Row を生成する
    fn make_row_with_int(pairs: &[(&str, &str)], int_pairs: &[(&str, i64)]) -> Row {
        let mut map = make_row(pairs);
        for &(k, v) in int_pairs {
            map.insert(k.to_string(), Value::from(v));
        }
        map
    }

    /// f64 値を持つキーを追加した Row を生成する
    fn make_row_with_float(pairs: &[(&str, &str)], float_pairs: &[(&str, f64)]) -> Row {
        let mut map = make_row(pairs);
        for &(k, v) in float_pairs {
            map.insert(
                k.to_string(),
                Value::Number(serde_json::Number::from_f64(v).unwrap()),
            );
        }
        map
    }

    // ======================================================
    // 1. render_education_section
    // ======================================================

    /// 空データ → 空文字列を返す（境界条件）
    #[test]
    fn test_education_empty_returns_empty() {
        let result = render_education_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// total_count が全て 0 → 空文字列を返す
    #[test]
    fn test_education_all_zero_count_returns_empty() {
        let row = make_row_with_int(
            &[("education_level", "大学")],
            &[("total_count", 0), ("male_count", 0), ("female_count", 0)],
        );
        let result = render_education_section(&[row], "東京都");
        assert!(result.is_empty(), "total_count=0では空文字列を返すべき");
    }

    /// モックデータ → 学歴レベル文字列がHTMLに含まれる（逆証明: 含まれない場合は失敗）
    #[test]
    fn test_education_contains_level_value() {
        let row = make_row_with_int(
            &[("education_level", "大学院")],
            &[
                ("total_count", 500000),
                ("male_count", 300000),
                ("female_count", 200000),
            ],
        );
        let html = render_education_section(&[row], "東京都");
        assert!(
            html.contains("大学院"),
            "学歴レベルがHTMLに含まれるべき: {}",
            &html[..html.len().min(500)]
        );
    }

    /// 都道府県ラベルがHTMLに含まれる
    #[test]
    fn test_education_contains_pref_label() {
        let row = make_row_with_int(
            &[("education_level", "高校")],
            &[
                ("total_count", 1000000),
                ("male_count", 500000),
                ("female_count", 500000),
            ],
        );
        let html = render_education_section(&[row], "大阪府");
        assert!(
            html.contains("大阪府"),
            "都道府県ラベルがHTMLに含まれるべき"
        );
    }

    /// ECharts クラスとデータ属性がHTMLに含まれる
    #[test]
    fn test_education_contains_echart_class() {
        let row = make_row_with_int(
            &[("education_level", "専門学校")],
            &[
                ("total_count", 200000),
                ("male_count", 100000),
                ("female_count", 100000),
            ],
        );
        let html = render_education_section(&[row], "愛知県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsの class='echart' と data-chart-config 属性が含まれるべき"
        );
    }

    /// 3桁区切りフォーマットで数値が表示される
    #[test]
    fn test_education_number_formatting() {
        let row = make_row_with_int(
            &[("education_level", "大学")],
            &[
                ("total_count", 3102649),
                ("male_count", 1600000),
                ("female_count", 1502649),
            ],
        );
        let html = render_education_section(&[row], "東京都");
        // format_number により「3,102,649」形式になる
        assert!(
            html.contains("3,102,649"),
            "total_countが3桁区切りでフォーマットされるべき: actual html snippets = {}",
            &html[..html.len().min(1000)]
        );
    }

    // ======================================================
    // 2. render_household_type_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_household_empty_returns_empty() {
        let result = render_household_type_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 世帯類型名がHTMLに含まれる
    #[test]
    fn test_household_contains_type_name() {
        let row = make_row_with_float(&[("household_type", "単独世帯")], &[("ratio", 35.2)]);
        let row = {
            let mut r = row;
            r.insert("count".to_string(), Value::from(5000000_i64));
            r
        };
        let html = render_household_type_section(&[row], "東京都");
        assert!(html.contains("単独世帯"), "世帯類型名がHTMLに含まれるべき");
    }

    /// EChartsドーナツチャートのマーカーが含まれる
    #[test]
    fn test_household_contains_pie_chart() {
        let mut row = make_row(&[("household_type", "核家族世帯")]);
        row.insert("count".to_string(), Value::from(3000000_i64));
        row.insert(
            "ratio".to_string(),
            Value::Number(serde_json::Number::from_f64(50.0).unwrap()),
        );
        let html = render_household_type_section(&[row], "神奈川県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsドーナツチャートが含まれるべき"
        );
    }

    /// 世帯数が3桁区切りでHTMLに含まれる
    #[test]
    fn test_household_count_formatted() {
        let mut row = make_row(&[("household_type", "単独世帯")]);
        row.insert("count".to_string(), Value::from(1234567_i64));
        row.insert(
            "ratio".to_string(),
            Value::Number(serde_json::Number::from_f64(25.0).unwrap()),
        );
        let html = render_household_type_section(&[row], "大阪府");
        assert!(
            html.contains("1,234,567"),
            "世帯数が3桁区切りでフォーマットされるべき"
        );
    }

    // ======================================================
    // 3. render_foreign_residents_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_foreign_empty_returns_empty() {
        let result = render_foreign_residents_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 在留資格名がHTMLに含まれる
    #[test]
    fn test_foreign_contains_visa_status() {
        let row = make_row_with_int(
            &[
                ("visa_status", "技術・人文知識・国際業務"),
                ("survey_period", "2023年"),
            ],
            &[("count", 987654)],
        );
        let html = render_foreign_residents_section(&[row], "東京都");
        assert!(
            html.contains("技術・人文知識・国際業務"),
            "在留資格名がHTMLに含まれるべき"
        );
    }

    /// 調査時点がHTMLに含まれる
    #[test]
    fn test_foreign_contains_survey_period() {
        let row = make_row_with_int(
            &[("visa_status", "永住者"), ("survey_period", "2022年6月末")],
            &[("count", 100000)],
        );
        let html = render_foreign_residents_section(&[row], "愛知県");
        assert!(html.contains("2022年6月末"), "調査時点がHTMLに含まれるべき");
    }

    /// 人数が3桁区切りでHTMLに含まれる
    #[test]
    fn test_foreign_count_formatted() {
        let row = make_row_with_int(
            &[("visa_status", "留学"), ("survey_period", "2023年")],
            &[("count", 1234567)],
        );
        let html = render_foreign_residents_section(&[row], "大阪府");
        assert!(
            html.contains("1,234,567"),
            "人数が3桁区切りでフォーマットされるべき"
        );
    }

    // ======================================================
    // 4. render_land_price_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_land_price_empty_returns_empty() {
        let result = render_land_price_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 住宅地データ → 住宅地ラベルがHTMLに含まれる
    #[test]
    fn test_land_price_contains_residential_label() {
        let row = make_row_with_float(
            &[("land_use", "住宅地")],
            &[("avg_price_per_sqm", 250000.0), ("yoy_change_pct", 2.5)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(500_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        assert!(html.contains("住宅地"), "住宅地ラベルがHTMLに含まれるべき");
    }

    /// 前年比がプラスのとき緑色コードが含まれる
    #[test]
    fn test_land_price_positive_yoy_green_color() {
        let row = make_row_with_float(
            &[("land_use", "商業地")],
            &[("avg_price_per_sqm", 3000000.0), ("yoy_change_pct", 5.0)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(200_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        // プラスYoY → 緑色 (#22c55e)
        assert!(
            html.contains("#22c55e"),
            "前年比プラス時は緑色コードが含まれるべき"
        );
    }

    /// 前年比がマイナスのとき赤色コードが含まれる
    #[test]
    fn test_land_price_negative_yoy_red_color() {
        let row = make_row_with_float(
            &[("land_use", "工業地")],
            &[("avg_price_per_sqm", 50000.0), ("yoy_change_pct", -1.5)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(100_i64));
            r
        };
        let html = render_land_price_section(&[row], "北海道");
        // マイナスYoY → 赤色 (#ef4444)
        assert!(
            html.contains("#ef4444"),
            "前年比マイナス時は赤色コードが含まれるべき"
        );
    }

    /// マッチしない用途名 → 「データなし」が含まれる
    #[test]
    fn test_land_price_no_matching_land_use_shows_no_data() {
        // 「その他」という用途名は住宅地・商業地・工業地のいずれにも一致しない
        let row = make_row_with_float(
            &[("land_use", "その他")],
            &[("avg_price_per_sqm", 10000.0), ("yoy_change_pct", 0.0)],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2024_i64));
            r.insert("point_count".to_string(), Value::from(10_i64));
            r
        };
        let html = render_land_price_section(&[row], "東京都");
        assert!(
            html.contains("データなし"),
            "マッチしない用途名の場合「データなし」が表示されるべき"
        );
    }

    // ======================================================
    // 5. render_regional_infra_section
    // ======================================================

    /// 両方空 → 空文字列を返す
    #[test]
    fn test_regional_infra_both_empty_returns_empty() {
        let result = render_regional_infra_section(&[], &[], "東京都");
        assert!(
            result.is_empty(),
            "car_data・net_data両方空では空文字列を返すべき"
        );
    }

    /// car_dataのみあり → 自動車保有率KPIが含まれる
    #[test]
    fn test_regional_infra_car_data_only_renders() {
        let row = make_row_with_float(&[], &[("cars_per_100people", 75.5)]);
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[row], &[], "群馬県");
        assert!(
            html.contains("自動車保有率"),
            "自動車保有率KPIが含まれるべき"
        );
        assert!(html.contains("群馬県"), "都道府県ラベルが含まれるべき");
    }

    /// 高い自動車保有率（≥70）→ 緑色コードが含まれる
    #[test]
    fn test_regional_infra_car_high_rate_green() {
        let mut row = HashMap::new();
        row.insert(
            "cars_per_100people".to_string(),
            Value::Number(serde_json::Number::from_f64(80.0).unwrap()),
        );
        row.insert("year".to_string(), Value::from(2022_i64));
        let html = render_regional_infra_section(&[row], &[], "栃木県");
        assert!(
            html.contains("#22c55e"),
            "保有率≥70の場合は緑色コードが含まれるべき"
        );
    }

    /// net_dataのみあり → インターネット利用率KPIが含まれる
    #[test]
    fn test_regional_infra_net_data_only_renders() {
        let row = make_row_with_float(
            &[],
            &[
                ("internet_usage_rate", 85.0),
                ("smartphone_ownership_rate", 78.0),
            ],
        );
        let row = {
            let mut r = row;
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[], &[row], "神奈川県");
        assert!(
            html.contains("インターネット利用率"),
            "インターネット利用率KPIが含まれるべき"
        );
    }

    /// 両方あり → 両方のKPIが含まれる
    #[test]
    fn test_regional_infra_both_data_renders_both_kpis() {
        let car_row = {
            let mut r = make_row_with_float(&[], &[("cars_per_100people", 65.0)]);
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let net_row = {
            let mut r = make_row_with_float(
                &[],
                &[
                    ("internet_usage_rate", 82.0),
                    ("smartphone_ownership_rate", 75.0),
                ],
            );
            r.insert("year".to_string(), Value::from(2022_i64));
            r
        };
        let html = render_regional_infra_section(&[car_row], &[net_row], "埼玉県");
        assert!(
            html.contains("自動車保有率"),
            "自動車保有率KPIが含まれるべき"
        );
        assert!(
            html.contains("インターネット利用率"),
            "インターネット利用率KPIが含まれるべき"
        );
    }

    // ======================================================
    // 6. render_social_life_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_social_life_empty_returns_empty() {
        let result = render_social_life_section(&[], "東京都");
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// カテゴリ名がHTMLに含まれる
    #[test]
    fn test_social_life_contains_category() {
        let row = make_row_with_float(
            &[
                ("category", "スポーツ"),
                ("subcategory", "ジョギング・マラソン"),
            ],
            &[("participation_rate", 42.5)],
        );
        let html = render_social_life_section(&[row], "東京都");
        assert!(html.contains("スポーツ"), "カテゴリ名がHTMLに含まれるべき");
    }

    /// サブカテゴリ名がHTMLに含まれる
    #[test]
    fn test_social_life_contains_subcategory() {
        let row = make_row_with_float(
            &[("category", "趣味・娯楽"), ("subcategory", "読書")],
            &[("participation_rate", 60.0)],
        );
        let html = render_social_life_section(&[row], "京都府");
        assert!(html.contains("読書"), "サブカテゴリ名がHTMLに含まれるべき");
    }

    /// EChartsレーダーチャートのマーカーが含まれる
    #[test]
    fn test_social_life_contains_radar_chart() {
        let row = make_row_with_float(
            &[("category", "ボランティア"), ("subcategory", "地域行事")],
            &[("participation_rate", 25.3)],
        );
        let html = render_social_life_section(&[row], "兵庫県");
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsレーダーチャートが含まれるべき"
        );
    }

    /// 行動者率の具体値がHTMLに含まれる（逆証明: 存在チェックではなく値検証）
    #[test]
    fn test_social_life_participation_rate_in_html() {
        let row = make_row_with_float(
            &[("category", "学習・自己啓発"), ("subcategory", "外国語")],
            &[("participation_rate", 12.3)],
        );
        let html = render_social_life_section(&[row], "福岡県");
        assert!(
            html.contains("12.3"),
            "行動者率の具体値がHTMLに含まれるべき"
        );
    }

    // ======================================================
    // 7. render_boj_tankan_section
    // ======================================================

    /// 空データ → 空文字列を返す
    #[test]
    fn test_boj_tankan_empty_returns_empty() {
        let result = render_boj_tankan_section(&[]);
        assert!(result.is_empty(), "空データでは空文字列を返すべき");
    }

    /// 対象外産業・対象外DI種別のみ → series_map空 → 空文字列を返す
    #[test]
    fn test_boj_tankan_non_target_industry_returns_empty() {
        // 「その他産業」はtarget_industriesに含まれないのでseries_mapは空になる
        let row = make_row_with_float(
            &[
                ("industry_j", "その他産業"),
                ("di_type", "business"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", 10.0)],
        );
        let result = render_boj_tankan_section(&[row]);
        assert!(result.is_empty(), "対象外産業のみでは空文字列を返すべき");
    }

    /// 製造業 業況DI → EChartsチャートとタイトルが含まれる
    #[test]
    fn test_boj_tankan_manufacturing_business_condition_renders() {
        let row = make_row_with_float(
            &[
                ("industry_j", "製造業"),
                ("di_type", "business"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", 15.0)],
        );
        let html = render_boj_tankan_section(&[row]);
        assert!(
            html.contains("echart") && html.contains("data-chart-config"),
            "EChartsチャートが含まれるべき"
        );
        assert!(
            html.contains("業況判断DI"),
            "タイトル「業況判断DI」が含まれるべき"
        );
    }

    /// 非製造業 雇用人員DI → シリーズ名がchart_configに含まれる
    #[test]
    fn test_boj_tankan_non_manufacturing_employment_excess_renders() {
        let row = make_row_with_float(
            &[
                ("industry_j", "非製造業"),
                ("di_type", "employment"),
                ("survey_date", "2024Q1"),
            ],
            &[("di_value", -5.0)],
        );
        let html = render_boj_tankan_section(&[row]);
        // chart_config内に「非製造業 雇用人員DI」シリーズ名が含まれる
        assert!(
            html.contains("非製造業 雇用人員DI"),
            "非製造業 雇用人員DIのシリーズ名がHTMLに含まれるべき"
        );
    }

    /// 複数日付 → 時系列が昇順ソートされてchart_configに含まれる
    #[test]
    fn test_boj_tankan_multiple_dates_sorted() {
        let rows = vec![
            make_row_with_float(
                &[
                    ("industry_j", "製造業"),
                    ("di_type", "business"),
                    ("survey_date", "2024Q3"),
                ],
                &[("di_value", 12.0)],
            ),
            make_row_with_float(
                &[
                    ("industry_j", "製造業"),
                    ("di_type", "business"),
                    ("survey_date", "2024Q1"),
                ],
                &[("di_value", 8.0)],
            ),
        ];
        let html = render_boj_tankan_section(&rows);
        // 昇順ソート後は 2024Q1 が 2024Q3 より前に出現するはず
        let pos_q1 = html.find("2024Q1").expect("2024Q1がHTMLに含まれるべき");
        let pos_q3 = html.find("2024Q3").expect("2024Q3がHTMLに含まれるべき");
        assert!(
            pos_q1 < pos_q3,
            "調査日付は昇順ソートされて出力されるべき (2024Q1 < 2024Q3)"
        );
    }
}
