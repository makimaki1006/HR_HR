//! Phase 2-B (2026-05-29): 時給モード H1/H3/H4 QA テスト
//!
//! 対象:
//!   - H1: 扶養範囲到達時給 (Section 03 表 3-H) → `build_navy_fuyou_table`
//!   - H3: 最賃プレミアム率分布 (Section 07 図 7-3) → `build_navy_minwage_premium_histogram_svg`
//!   - H4: 時給帯別 求人件数 (Section 06 表 6-J) → `build_hourly_band_distribution`
//!
//! 設計方針:
//!   - 各 H に対し、空 / 単一値 / 通常 / 境界値 / 異常値 を網羅
//!   - 不変条件 (invariant) を assert! で検証 (`feedback_reverse_proof_tests.md`)
//!   - silent fallback 監査: 空入力で空文字 OR 明示的「該当データなし」を確認
//!   - 月給モード (is_hourly=false) で表 3-H / 図 7-3 / 表 6-J が出力されないことを確認
//!
//! 既存テスト (`report_html_qa_test.rs` / `invariant_tests.rs`) との重複は避け、
//! 本ファイルは時給特有指標に限定する。

#![cfg(test)]

use super::navy_report::{
    build_hourly_band_distribution, build_navy_fuyou_table,
    build_navy_minwage_premium_histogram_svg,
};

// ============================================================
// H1: 扶養範囲到達時給 (10 ケース)
// ============================================================

/// H1-01: median = 0 → 自社中央値の 5 セルすべて "—" 表示
#[test]
fn h1_fuyou_median_zero_shows_dash_in_self_row() {
    let html = build_navy_fuyou_table(0);
    // "—" は最低 5 個 (週時間 5 列 × 自社中央値行 1 行)
    let dash_count = html.matches("—").count();
    assert!(
        dash_count >= 5,
        "median=0 のとき自社中央値行に最低 5 個の '—' が必要: {}",
        dash_count
    );
    // 103/130 ライン行は数値が入る (—ではない)
    assert!(html.contains("103 万円ライン"));
    assert!(html.contains("130 万円ライン"));
}

/// H1-02: median = 1200 → 自社中央値 5 セルに "1,200" 表示
#[test]
fn h1_fuyou_median_1200_shows_value_in_self_row() {
    let html = build_navy_fuyou_table(1200);
    // format_number(1200) = "1,200"
    let occurrences = html.matches("1,200 円/時").count();
    assert!(
        occurrences >= 5,
        "median=1200 のとき自社中央値行に 5 セル分の '1,200 円/時' が必要: {}",
        occurrences
    );
}

/// H1-03 (不変条件): 130 万円ラインの必要時給 > 103 万円ラインの必要時給 (同一週時間)
/// 計算式: 必要時給 = (annual_yen + denom - 1) / denom (切上)
/// 例: 週 20h → denom=1040, 103万=ceil(1030000/1040)=991, 130万=ceil(1300000/1040)=1250
///   - 1300000/1040 = 1250.0 ちょうど → ceil = 1250
#[test]
fn h1_fuyou_130man_always_higher_than_103man() {
    let html = build_navy_fuyou_table(0);
    // 週20h × 103万ライン = 991 (= ceil(1030000/1040))
    // 週20h × 130万ライン = 1,250 (= ceil(1300000/1040))
    assert!(
        html.contains("991 円/時"),
        "週20h × 103万ライン必要時給 991 が必要"
    );
    assert!(
        html.contains("1,250 円/時"),
        "週20h × 130万ライン必要時給 1,250 が必要"
    );
}

/// H1-04 (不変条件): 週時間昇順 → 必要時給降順 (反転) — 103万ラインで検証
/// 週15h: ceil(1030000/780) = 1321
/// 週20h: ceil(1030000/1040) = 991
/// 週25h: ceil(1030000/1300) = 793
/// 週30h: ceil(1030000/1560) = 661
/// 週35h: ceil(1030000/1820) = 566
#[test]
fn h1_fuyou_103man_descending_with_more_hours() {
    let html = build_navy_fuyou_table(0);
    // 5 値を順に検索 (出現位置で順序を間接確認)
    let pos_15h = html.find("1,321 円/時");
    let pos_20h = html.find("991 円/時");
    let pos_25h = html.find("793 円/時");
    let pos_30h = html.find("661 円/時");
    let pos_35h = html.find("566 円/時");
    assert!(pos_15h.is_some(), "週15h 必要時給 1,321 が含まれること");
    assert!(pos_20h.is_some(), "週20h 必要時給 991 が含まれること");
    assert!(pos_25h.is_some(), "週25h 必要時給 793 が含まれること");
    assert!(pos_30h.is_some(), "週30h 必要時給 661 が含まれること");
    assert!(pos_35h.is_some(), "週35h 必要時給 566 が含まれること");
    // 順序: 15h < 20h < 25h < 30h < 35h (HTML 出現位置で確認)
    let p15 = pos_15h.unwrap();
    let p20 = pos_20h.unwrap();
    let p25 = pos_25h.unwrap();
    let p30 = pos_30h.unwrap();
    let p35 = pos_35h.unwrap();
    assert!(p15 < p20, "15h セルは 20h より前に出現すべき");
    assert!(p20 < p25);
    assert!(p25 < p30);
    assert!(p30 < p35);
}

/// H1-05: 構造的不変条件 — table-navy class / thead / tbody / 6 列 (区分 + 5 週時間)
#[test]
fn h1_fuyou_table_structure_has_table_navy_class_and_6_columns() {
    let html = build_navy_fuyou_table(1500);
    assert!(html.contains("<table class=\"table-navy\">"), "table-navy class 必須");
    assert!(html.contains("<thead>"), "thead 必須");
    assert!(html.contains("<tbody>"), "tbody 必須");
    // 列ヘッダ: 区分 + 5 週時間 (15/20/25/30/35h)
    assert!(html.contains("週 15h"));
    assert!(html.contains("週 20h"));
    assert!(html.contains("週 25h"));
    assert!(html.contains("週 30h"));
    assert!(html.contains("週 35h"));
}

/// H1-06: median = 1500 (両ライン上回る) → ライン値 (991, 1250) も median (1500) も
///         全て表示される。1500 > 1250 (週20h 130万ライン上回り) を確認。
#[test]
fn h1_fuyou_median_above_both_lines() {
    let html = build_navy_fuyou_table(1500);
    assert!(html.contains("991 円/時"), "週20h 103万ライン 991 表示");
    assert!(html.contains("1,250 円/時"), "週20h 130万ライン 1,250 表示");
    assert!(html.contains("1,500 円/時"), "median 1,500 表示");
}

/// H1-07: a11y 不変条件 — すべての <th> に scope="col" 属性付与
#[test]
fn h1_fuyou_th_has_scope_col() {
    let html = build_navy_fuyou_table(1200);
    let th_count = html.matches("<th ").count();
    let scoped_count = html.matches("scope=\"col\"").count();
    assert!(th_count > 0, "<th> が少なくとも 1 つ必要");
    assert_eq!(
        th_count, scoped_count,
        "すべての <th> に scope=\"col\" が必要 (a11y)"
    );
}

/// H1-08: 区分行ラベル — 「103 万円ライン」「130 万円ライン」「自社 下限給与 中央値」を含む
#[test]
fn h1_fuyou_row_labels_present() {
    let html = build_navy_fuyou_table(1200);
    assert!(html.contains("103 万円ライン"));
    assert!(html.contains("130 万円ライン"));
    assert!(html.contains("自社 下限給与 中央値"));
}

/// H1-09: 負の median (異常値) も "—" 行になる (panic しない)
#[test]
fn h1_fuyou_negative_median_treated_as_no_data() {
    let html = build_navy_fuyou_table(-100);
    // median <= 0 → 自社中央値行は "—"
    let dash_count = html.matches("—").count();
    assert!(dash_count >= 5, "median<=0 で 自社中央値行に 5 個の '—' 必要");
    // 異常値 -100 が表示されてはいけない
    assert!(!html.contains("-100 円/時"), "負の median を数値表示すべきでない");
}

/// H1-10 (不変条件): 全ライン値が非負整数 — 最小値 (週35h の各ライン) を検証
#[test]
fn h1_fuyou_all_line_values_positive_integer() {
    let html = build_navy_fuyou_table(0);
    // 103万 (週35h) = ceil(1030000/1820) = 566 — 最小値でも > 0
    // 130万 (週35h) = ceil(1300000/1820) = 715 — 最小値でも > 0
    assert!(html.contains("566 円/時"), "週35h 103万ライン 566 (最小値) > 0");
    assert!(html.contains("715 円/時"), "週35h 130万ライン 715 (最小値) > 0");
}

// ============================================================
// H3: 最賃プレミアム率分布 (10 ケース)
// ============================================================

/// H3-01: min_wage <= 0 → "" (空文字) を返す
#[test]
fn h3_premium_min_wage_zero_returns_empty() {
    let svg = build_navy_minwage_premium_histogram_svg(&[1200, 1300], 0);
    assert_eq!(svg, "", "min_wage <= 0 で空文字を返すこと (呼出側で caption 表示)");
    let svg_neg = build_navy_minwage_premium_histogram_svg(&[1200], -50);
    assert_eq!(svg_neg, "", "min_wage 負値でも空文字");
}

/// H3-02: values 空 → "該当データなし" を明示表示
#[test]
fn h3_premium_empty_values_shows_no_data_caption() {
    let svg = build_navy_minwage_premium_histogram_svg(&[], 1000);
    assert!(
        svg.contains("該当データなし"),
        "空 values でも min_wage>0 なら 'データなし' 明示。silent fallback 禁止: {}",
        svg
    );
}

/// H3-03: 全件 0 以下 (フィルタで全除外) → "該当データなし"
#[test]
fn h3_premium_all_zero_values_shows_no_data() {
    let svg = build_navy_minwage_premium_histogram_svg(&[0, 0, -100], 1000);
    assert!(
        svg.contains("該当データなし"),
        "全 values が 0 以下 → 'データなし': {}",
        svg
    );
}

/// H3-04 (不変条件): bucket 合計件数 == values の有効件数 (>0 のみ)
///   検証方法: SVG 内の "<rect" 描画と bar 上数値テキストの合計を間接確認。
///   ここでは bar count ラベル (各 bar 上に数値が出る) の合計を検証。
#[test]
fn h3_premium_bucket_total_equals_valid_count() {
    // min_wage=1000、values: 1100 (10%) / 1200 (20%) / 1500 (50%) → 各 bucket 1 件ずつ、計 3 件
    let values = vec![1100, 1200, 1500];
    let svg = build_navy_minwage_premium_histogram_svg(&values, 1000);
    assert!(svg.contains("<svg"), "SVG 出力");
    // 件数 1 のラベルが少なくとも 3 回出現すること
    let label_count = svg.matches(">1</text>").count();
    assert!(
        label_count >= 3,
        "3 つの bucket に 1 件ずつ → ラベル '>1</text>' 3 回以上: {}",
        label_count
    );
}

/// H3-05: 全件 min_wage 未満 → premium_pct < 0 → <0% bucket に集中
///   values: [800, 900, 950], min_wage: 1000 → 全て <0%
#[test]
fn h3_premium_all_below_minwage_concentrate_negative_bucket() {
    let values = vec![800, 900, 950];
    let svg = build_navy_minwage_premium_histogram_svg(&values, 1000);
    // <0% ラベルが SVG 内に出現
    assert!(svg.contains("&lt;0%"), "<0% ラベルが x 軸に表示");
    // bar 上に "3" が出る (<0% bucket に 3 件集中)
    assert!(svg.contains(">3</text>"), "<0% bucket に 3 件集中");
}

/// H3-06: 通常分布 — 10% / 20% / 30% / 45% を含み、それぞれ別 bucket に振分
#[test]
fn h3_premium_typical_distribution_buckets_distinct() {
    // min_wage=1000、values: 1099 (9.9% → 5-10), 1199 (19.9% → 15-20),
    //                          1299 (29.9% → 25-30), 1449 (44.9% → 40-45)
    let values = vec![1099, 1199, 1299, 1449];
    let svg = build_navy_minwage_premium_histogram_svg(&values, 1000);
    // 各 bucket label が表示
    assert!(svg.contains("5-10%"));
    assert!(svg.contains("15-20%"));
    assert!(svg.contains("25-30%"));
    assert!(svg.contains("40-45%"));
    // 4 つの異なる bucket に 1 件ずつ → ">1</text>" が 4 回以上
    assert!(svg.matches(">1</text>").count() >= 4);
}

/// H3-07: 高プレミアム (>= 45%) は 45%+ bucket に分類
#[test]
fn h3_premium_high_value_goes_to_overflow_bucket() {
    // 2000 vs min_wage 1000 → premium 100% → 45%+
    let values = vec![2000, 3000];
    let svg = build_navy_minwage_premium_histogram_svg(&values, 1000);
    assert!(svg.contains("45%+"), "45%+ overflow bucket ラベル");
    assert!(svg.matches(">2</text>").count() >= 1, "45%+ に 2 件");
}

/// H3-08: 構造的不変条件 — SVG タグ / role="img" / aria-label / x 軸タイトル
#[test]
fn h3_premium_svg_structure_and_a11y() {
    let svg = build_navy_minwage_premium_histogram_svg(&[1100], 1000);
    assert!(svg.contains("<svg "), "SVG タグ");
    assert!(svg.contains("</svg>"), "SVG 閉じタグ");
    assert!(svg.contains("role=\"img\""), "a11y role=img");
    assert!(svg.contains("aria-label="), "a11y aria-label");
    assert!(svg.contains("最賃プレミアム率"), "x 軸タイトル");
}

/// H3-09: x 軸ラベル — 11 段 bucket すべてのラベルが出力
#[test]
fn h3_premium_all_11_bucket_labels_present() {
    let svg = build_navy_minwage_premium_histogram_svg(&[1100], 1000);
    let labels = [
        "&lt;0%", "0-5%", "5-10%", "10-15%", "15-20%", "20-25%", "25-30%", "30-35%", "35-40%",
        "40-45%", "45%+",
    ];
    for label in labels.iter() {
        assert!(
            svg.contains(label),
            "bucket label '{}' が x 軸に必要: {}",
            label,
            &svg[..svg.len().min(800)]
        );
    }
}

/// H3-10 (不変条件): 大規模入力 (n=1000) で panic せず SVG を返す
#[test]
fn h3_premium_large_input_no_panic() {
    let values: Vec<i64> = (0..1000).map(|i| 1000 + (i % 500)).collect();
    let svg = build_navy_minwage_premium_histogram_svg(&values, 1000);
    assert!(svg.contains("<svg "), "大規模入力でも SVG 描画完了");
}

// ============================================================
// H4: 時給帯別 求人件数 (10 ケース)
// ============================================================

/// H4-01: empty values → 全 bucket 0 件 (sum == 0)
#[test]
fn h4_band_empty_returns_all_zero_buckets() {
    let dist = build_hourly_band_distribution(&[]);
    assert_eq!(dist.len(), 13, "bucket 数は 13 段固定");
    let total: i64 = dist.iter().map(|(_, c)| *c).sum();
    assert_eq!(total, 0, "empty → 合計 0");
}

/// H4-02 (不変条件): bucket 合計 == values の有効件数 (>0 のみ)
#[test]
fn h4_band_total_equals_valid_count() {
    let values = vec![800, 1000, 1500, 2500, 0, -10]; // valid: 4 件 (0, -10 除外)
    let dist = build_hourly_band_distribution(&values);
    let total: i64 = dist.iter().map(|(_, c)| *c).sum();
    assert_eq!(total, 4, "0/-10 除外で 4 件");
}

/// H4-03: 単一値 [1200, 1200, 1200] → "1200-1300円" bucket に 3 件
#[test]
fn h4_band_single_value_concentrated_in_correct_bucket() {
    let dist = build_hourly_band_distribution(&[1200, 1200, 1200]);
    let target = dist.iter().find(|(l, _)| l == "1200-1300円");
    assert!(target.is_some(), "1200-1300円 bucket が存在");
    assert_eq!(target.unwrap().1, 3, "1200-1300円 bucket に 3 件");
    // 他 bucket は全 0
    let other_total: i64 = dist
        .iter()
        .filter(|(l, _)| l != "1200-1300円")
        .map(|(_, c)| *c)
        .sum();
    assert_eq!(other_total, 0, "他 bucket は全 0");
}

/// H4-04: 境界値 — 1000 ちょうど → "1000-1100円" bucket (lo 包含)
#[test]
fn h4_band_boundary_1000_goes_to_1000_1100() {
    let dist = build_hourly_band_distribution(&[1000]);
    let target = dist.iter().find(|(l, _)| l == "1000-1100円");
    assert_eq!(target.unwrap().1, 1, "1000 ちょうどは 1000-1100 bucket");
    let lower = dist.iter().find(|(l, _)| l == "900-1000円");
    assert_eq!(lower.unwrap().1, 0, "1000 は 900-1000 bucket には入らない (hi 排他)");
}

/// H4-05: 境界値 — 899 → "<900円" bucket
#[test]
fn h4_band_boundary_below_900() {
    let dist = build_hourly_band_distribution(&[899, 850, 100]);
    let target = dist.iter().find(|(l, _)| l == "<900円");
    assert_eq!(target.unwrap().1, 3, "899, 850, 100 は全て <900円 bucket");
}

/// H4-06: 境界値 — 2000 → "2000円+" bucket (overflow)
#[test]
fn h4_band_boundary_2000_goes_to_overflow() {
    let dist = build_hourly_band_distribution(&[2000, 3000, 5000]);
    let target = dist.iter().find(|(l, _)| l == "2000円+");
    assert_eq!(target.unwrap().1, 3, "2000 以上は overflow bucket");
}

/// H4-07: 順序保持 — bucket は宣言順 (<900 → 2000+) で返る
#[test]
fn h4_band_order_preserved() {
    let dist = build_hourly_band_distribution(&[]);
    let labels: Vec<&str> = dist.iter().map(|(l, _)| l.as_str()).collect();
    assert_eq!(labels[0], "<900円");
    assert_eq!(labels[1], "900-1000円");
    assert_eq!(labels[2], "1000-1100円");
    assert_eq!(labels[12], "2000円+");
    assert_eq!(dist.len(), 13, "総数 13 段");
}

/// H4-08: スケール — n=1000 でも sum 整合性 (panic なし)
#[test]
fn h4_band_scale_1000_values() {
    let values: Vec<i64> = (0..1000).map(|i| 900 + (i % 1500)).collect();
    let dist = build_hourly_band_distribution(&values);
    let total: i64 = dist.iter().map(|(_, c)| *c).sum();
    assert_eq!(total, 1000, "n=1000 → 合計 1000");
}

/// H4-09 (不変条件): 各 bucket count ∈ [0, total]
#[test]
fn h4_band_each_count_within_total_range() {
    let values = vec![800, 950, 1050, 1150, 1250, 1350, 1450, 1550, 1650, 1750, 1850, 1950, 2050];
    let dist = build_hourly_band_distribution(&values);
    let total: i64 = dist.iter().map(|(_, c)| *c).sum();
    for (label, c) in dist.iter() {
        assert!(*c >= 0, "{} count >= 0 (negative is impossible)", label);
        assert!(*c <= total, "{} count <= total ({} <= {})", label, c, total);
    }
}

/// H4-10: n=1 → 該当する 1 bucket だけ count=1、他は 0
#[test]
fn h4_band_n1_single_bucket_only() {
    let dist = build_hourly_band_distribution(&[1450]);
    let non_zero: Vec<&(String, i64)> = dist.iter().filter(|(_, c)| *c > 0).collect();
    assert_eq!(non_zero.len(), 1, "n=1 → 1 bucket のみ count > 0");
    assert_eq!(non_zero[0].0, "1400-1500円");
    assert_eq!(non_zero[0].1, 1);
}

// ============================================================
// 統合: Section render-level 制御 (月給モードで表 3-H / 図 7-3 / 表 6-J 非表示)
// ============================================================
//
// navy_report::render_navy_section_03_salary は HW context や salary_min_values 等
// 多くの引数を要求するため、ここでは render_survey_report_page 経由で is_hourly の
// 表示制御を検証する (build_hourly_band_distribution / build_navy_fuyou_table /
// build_navy_minwage_premium_histogram_svg 単体は上で完了)。

use super::super::aggregator::SurveyAggregation;
use super::super::job_seeker::JobSeekerAnalysis;
use super::render_survey_report_page;

/// H-INT-01: 月給モード (is_hourly=false) — 表 3-H ヘッダが出ない
#[test]
fn integration_monthly_mode_no_fuyou_table_3h() {
    let agg = SurveyAggregation::default(); // is_hourly=false (default)
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        !html.contains("表 3-H"),
        "月給モードでは表 3-H (扶養範囲到達時給) を出力すべきでない"
    );
}

/// H-INT-02: 月給モード — 図 7-3 ヘッダが出ない
#[test]
fn integration_monthly_mode_no_premium_histogram_7_3() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        !html.contains("図 7-3"),
        "月給モードでは図 7-3 (最賃プレミアム率分布) を出力すべきでない"
    );
}

/// H-INT-03: 月給モード — 表 6-J ヘッダが出ない
#[test]
fn integration_monthly_mode_no_hourly_band_6j() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        !html.contains("表 6-J"),
        "月給モードでは表 6-J (時給帯別 求人件数) を出力すべきでない"
    );
}

/// H-INT-04: 時給モード (is_hourly=true) かつ Section 03 が描画される最小条件で
///           表 3-H ヘッダが出力されること
#[test]
fn integration_hourly_mode_renders_fuyou_table_3h() {
    let mut agg = SurveyAggregation::default();
    agg.is_hourly = true;
    agg.total_count = 10;
    agg.salary_parse_rate = 1.0;
    agg.salary_min_values_native = vec![1200, 1300, 1400];
    agg.salary_max_values_native = vec![1500, 1600, 1700];
    // 月給フィールドも空でないと SO WHAT の lo/hi 算出に影響するが Section 03 は通る
    agg.salary_min_values = vec![1200, 1300, 1400];
    agg.salary_max_values = vec![1500, 1600, 1700];
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        html.contains("表 3-H"),
        "時給モードかつデータあり → 表 3-H が出力されるべき"
    );
    assert!(
        html.contains("扶養範囲到達時給"),
        "表 3-H のタイトル '扶養範囲到達時給' が含まれる"
    );
}
