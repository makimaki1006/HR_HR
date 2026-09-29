//! Phase 2-B (2026-05-29): 時給モード H1/H3 QA テスト
//!
//! 対象:
//!   - H1: 扶養範囲到達時給 (Section 03 表 3-H) → `build_navy_fuyou_table`
//!   - H3: 最賃プレミアム率分布 (Section 09 図 9-3) → `build_navy_minwage_premium_histogram_svg`
//!
//! 2026-06-01: H4 (時給帯別 求人件数, 表 6-J) は HW postings 求人側集計ブロック
//! (図 6-3 + 表 6-G/H/I/J) の削除に伴い、`build_hourly_band_distribution` も
//! 撤去。本ファイルから H4 系 10 テスト + H-INT-03 (月給モードで表 6-J 非表示)
//! を削除。
//!
//! 設計方針:
//!   - 各 H に対し、空 / 単一値 / 通常 / 境界値 / 異常値 を網羅
//!   - 不変条件 (invariant) を assert! で検証 (`feedback_reverse_proof_tests.md`)
//!   - silent fallback 監査: 空入力で空文字 OR 明示的「該当データなし」を確認
//!   - 月給モード (is_hourly=false) で表 3-H / 図 9-3 が出力されないことを確認
//!
//! 既存テスト (`report_html_qa_test.rs` / `invariant_tests.rs`) との重複は避け、
//! 本ファイルは時給特有指標に限定する。

#![cfg(test)]

use super::navy_report::{build_navy_fuyou_table, build_navy_minwage_premium_histogram_svg};

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
    // 2026-07-28: colgroup 追加で table タグに style 属性が付いたため、
    // 閉じ > までの完全一致ではなく class 属性の存在で判定する。
    assert!(
        html.contains("<table class=\"table-navy\""),
        "table-navy class 必須"
    );
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
    assert!(
        dash_count >= 5,
        "median<=0 で 自社中央値行に 5 個の '—' 必要"
    );
    // 異常値 -100 が表示されてはいけない
    assert!(
        !html.contains("-100 円/時"),
        "負の median を数値表示すべきでない"
    );
}

/// H1-10 (不変条件): 全ライン値が非負整数 — 最小値 (週35h の各ライン) を検証
#[test]
fn h1_fuyou_all_line_values_positive_integer() {
    let html = build_navy_fuyou_table(0);
    // 103万 (週35h) = ceil(1030000/1820) = 566 — 最小値でも > 0
    // 130万 (週35h) = ceil(1300000/1820) = 715 — 最小値でも > 0
    assert!(
        html.contains("566 円/時"),
        "週35h 103万ライン 566 (最小値) > 0"
    );
    assert!(
        html.contains("715 円/時"),
        "週35h 130万ライン 715 (最小値) > 0"
    );
}

// ============================================================
// H3: 最賃プレミアム率分布 (10 ケース)
// ============================================================

/// H3-01: min_wage <= 0 → "" (空文字) を返す
#[test]
fn h3_premium_min_wage_zero_returns_empty() {
    let svg = build_navy_minwage_premium_histogram_svg(&[1200, 1300], 0);
    assert_eq!(
        svg, "",
        "min_wage <= 0 で空文字を返すこと (呼出側で caption 表示)"
    );
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

// 2026-06-01: H4 (時給帯別 求人件数, 表 6-J) 10 ケースは
// HW postings 求人側集計ブロック (図 6-3 + 表 6-G/H/I/J) の削除に伴い
// `build_hourly_band_distribution` を撤去したため全削除。

// ============================================================
// 統合: Section render-level 制御 (月給モードで表 3-H / 図 9-3 非表示)
// ============================================================
//
// navy_report::render_navy_section_03_salary は HW context や salary_min_values 等
// 多くの引数を要求するため、ここでは render_survey_report_page 経由で is_hourly の
// 表示制御を検証する (build_navy_fuyou_table /
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

/// H-INT-02: 月給モード — 図 9-3 ヘッダが出ない
#[test]
fn integration_monthly_mode_no_premium_histogram_7_3() {
    let agg = SurveyAggregation::default();
    let seeker = JobSeekerAnalysis::default();
    let html = render_survey_report_page(&agg, &seeker, &[], &[], &[], &[], None, &[]);
    assert!(
        !html.contains("図 9-3"),
        "月給モードでは図 9-3 (最賃プレミアム率分布) を出力すべきでない"
    );
}

// H-INT-03 (月給モードで表 6-J 非表示) は 2026-06-01 削除。
// 表 6-J 自体が section_06_demographics.rs から撤去されたため、
// 「月給モードで出力されない」の検証対象が存在しない。

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

// ============================================================
// 2026-09-29 時給/月給混在: 実データ fixture を通した HTML の逆証明
//   - 表紙 / §03 冒頭の「時給中央値」が月給換算値 (1250×167=208750) に
//     「円/時」を付けて出ていた (追加A)
//   - 表2-E が月給換算の平均を「円/時」列に出していた (追加B)
//   - §03 冒頭の n= が分布の実件数と一致しなかった (疑い3)
// ============================================================

fn render_fixture_report(salary_lines: &str, location: &str) -> String {
    use super::super::aggregator::{aggregate_records, salary_fixture};
    use super::super::job_seeker::analyze_job_seeker;
    let recs = salary_fixture::records(salary_lines, location);
    let agg = aggregate_records(&recs);
    let seeker = analyze_job_seeker(&recs);
    super::render_survey_report_page_for_vrt(
        &agg,
        &seeker,
        &agg.by_company,
        &agg.by_emp_type_salary,
        &agg.salary_min_values,
        &agg.salary_max_values,
        None,
        super::ReportVariant::Full,
    )
}

/// f1 (時給 751 + 月給 1): 表紙・§03 冒頭の時給中央値は時給求人の下限中央値 1,250 円/時
#[test]
fn mix_f1_cover_and_lede_show_native_hourly_median() {
    use super::super::aggregator::salary_fixture;
    let html = render_fixture_report(salary_fixture::F1_HOURLY_MOSTLY, "東京都 板橋区");
    assert!(
        !html.contains("208750"),
        "月給換算値 208,750 (=1,250×167) が円/時として出ている"
    );
    assert!(
        html.contains("1250円/時"),
        "§03 冒頭の代表値に時給中央値 1250円/時 が必要"
    );
}

/// f1: 表2-E / 表3-B の円/時の列に月給換算値 (平均 216,518 / 中央値 208,750) が出ないこと
#[test]
fn mix_f1_prefecture_table_uses_hourly_values() {
    use super::super::aggregator::salary_fixture;
    let html = render_fixture_report(salary_fixture::F1_HOURLY_MOSTLY, "東京都 板橋区");
    assert!(
        !html.contains("216,518"),
        "月給換算平均 216,518 が円/時として出ている"
    );
    assert!(
        !html.contains("208,750"),
        "表3-B に月給換算中央値 208,750 が円/時として出ている"
    );
    let start = html
        .find("平均給与 (円/時)")
        .expect("表2-E (時給モード) が必要");
    let table = &html[start..start + html[start..].find("</table>").unwrap()];
    // 時給求人 751 件の代表時給 (下限・上限の中間) の平均。月給 1 件は含めない
    assert!(
        table.contains("<td class=\"num\">751</td>"),
        "表2-E の n は時給求人数 751: {table}"
    );
    assert!(
        table.contains("<td class=\"num bold\">1,296</td>"),
        "表2-E の東京都 平均時給 1,296 円/時: {table}"
    );
}

/// f2 (月給 567 + 年俸/年収 24): §03 冒頭の n は分布に使った 567 件、除外 24 件を注記
#[test]
fn mix_f2_lede_n_matches_distribution_count() {
    use super::super::aggregator::salary_fixture;
    let html = render_fixture_report(salary_fixture::F2_MONTHLY_WITH_ANNUAL, "大阪府 大阪市");
    // §02 のサンプル件数 (CSV 全行 591) は別指標なので、§03 冒頭の書式で判定する
    assert!(
        !html.contains("サンプル <strong>n=591</strong> ("),
        "n=591 (給与解析件数) が §03 の分布件数として表示されている"
    );
    assert!(
        html.contains("サンプル <strong>n=567</strong> (月給換算できた求人の下限給与)"),
        "§03 冒頭 n は月給分布の実件数 567"
    );
    assert!(html.contains("24 件"), "除外 24 件の注記が必要");
}

// ============================================================
// 2026-09-29 (続き) 時給/月給混在の横展開: §03 表3-E/3-F/3-G、§07 表9-E/F/H/I、
//   §09 KPI、本編 (SP) 結論帯・表3-SP の逆証明。
//   f1 (時給 751 + 月給 1) を東京都の公的統計 (最低賃金 1,163 円/時) と組み合わせる。
// ============================================================

/// 東京都の最低賃金 (2024 年度 1,163 円/時)・家計支出・家賃 m² 単価を持つ最小 ctx
fn tokyo_ctx() -> crate::handlers::insight::fetch::InsightContext {
    use serde_json::Value;
    let row = |pairs: &[(&str, Value)]| -> crate::handlers::helpers::Row {
        pairs
            .iter()
            .map(|(k, v)| ((*k).to_string(), v.clone()))
            .collect()
    };
    let rent = |pref: &str, muni: &str, rate: i64| {
        row(&[
            ("prefecture", Value::from(pref)),
            ("municipality", Value::from(muni)),
            ("structure", Value::from("総数")),
            ("area_class", Value::from("総数")),
            ("rental_total_units", Value::Null),
            ("median_rent_jpy", Value::from(rate)),
            ("as_of", Value::from("2023")),
        ])
    };
    crate::handlers::insight::fetch::InsightContext {
        pref: "東京都".to_string(),
        muni: "板橋区".to_string(),
        ext_min_wage: vec![
            row(&[
                ("fiscal_year", Value::from(2023)),
                ("hourly_min_wage", Value::from(1113)),
            ]),
            row(&[
                ("fiscal_year", Value::from(2024)),
                ("hourly_min_wage", Value::from(1163)),
            ]),
        ],
        ext_household_spending: vec![row(&[
            ("category", Value::from("消費支出")),
            ("monthly_amount", Value::from(300_000)),
        ])],
        ext_rental_housing: vec![rent("全国", "", 1_500), rent("東京都", "板橋区", 2_500)],
        ..Default::default()
    }
}

fn render_fixture_report_ctx(
    salary_lines: &str,
    location: &str,
    ctx: Option<&crate::handlers::insight::fetch::InsightContext>,
    variant: super::ReportVariant,
) -> String {
    use super::super::aggregator::{aggregate_records, salary_fixture};
    use super::super::job_seeker::analyze_job_seeker;
    let recs = salary_fixture::records(salary_lines, location);
    let agg = aggregate_records(&recs);
    let seeker = analyze_job_seeker(&recs);
    super::render_survey_report_page_for_vrt(
        &agg,
        &seeker,
        &agg.by_company,
        &agg.by_emp_type_salary,
        &agg.salary_min_values,
        &agg.salary_max_values,
        ctx,
        variant,
    )
}

/// `marker` の出現位置から次の block-title (無ければ 6,000 byte) までを切り出す
fn block_after<'a>(html: &'a str, marker: &str) -> &'a str {
    let start = html
        .find(marker)
        .unwrap_or_else(|| panic!("{marker} が見つからない"));
    let rest = &html[start + marker.len()..];
    let mut end = rest
        .find("class=\"block-title")
        .unwrap_or_else(|| rest.len().min(6_000));
    while !rest.is_char_boundary(end) {
        end -= 1;
    }
    &html[start..start + marker.len() + end]
}

/// 「円/時」の直前にある数値 (タグ・空白・カンマを除去) をすべて返す
fn yen_per_hour_values(html: &str) -> Vec<i64> {
    let mut out = Vec::new();
    for (pos, _) in html.match_indices("円/時") {
        let mut pre = html[..pos].trim_end();
        // `1250<span class="kpi-unit">円/時` のようにタグを挟む書式に対応
        while pre.ends_with('>') {
            match pre.rfind('<') {
                Some(i) => pre = pre[..i].trim_end(),
                None => break,
            }
        }
        let tail: Vec<char> = pre
            .chars()
            .rev()
            .take_while(|c| c.is_ascii_digit() || *c == ',')
            .collect();
        let digits: String = tail.iter().rev().filter(|c| c.is_ascii_digit()).collect();
        if let Ok(v) = digits.parse::<i64>() {
            out.push(v);
        }
    }
    out
}

/// f1 全 variant: 「円/時」を付けた数値に月給換算の桁 (1 万以上) が 1 つも無いこと
#[test]
fn mix_f1_no_monthly_value_labelled_yen_per_hour_in_any_variant() {
    use super::super::aggregator::salary_fixture;
    let ctx = tokyo_ctx();
    for variant in [
        super::ReportVariant::Full,
        super::ReportVariant::MarketIntelligence,
        super::ReportVariant::Sp,
    ] {
        let html = render_fixture_report_ctx(
            salary_fixture::F1_HOURLY_MOSTLY,
            "東京都 板橋区",
            Some(&ctx),
            variant,
        );
        let bad: Vec<i64> = yen_per_hour_values(&html)
            .into_iter()
            .filter(|v| *v >= 10_000)
            .collect();
        assert!(
            bad.is_empty(),
            "{:?}: 円/時 に月給換算の桁の値が出ている: {:?}",
            variant,
            bad
        );
    }
}

/// NG4: 表9-E は時給求人の下限中央値 1,250 円/時、東京都最賃 1,163 との比は 1.07 倍
#[test]
fn mix_f1_minwage_table_uses_hourly_lower_median() {
    use super::super::aggregator::salary_fixture;
    let ctx = tokyo_ctx();
    let html = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        Some(&ctx),
        super::ReportVariant::Full,
    );
    let block = block_after(&html, "表 9-E");
    assert!(block.contains("1,163 円/時"), "最賃 1,163: {block}");
    assert!(
        block.contains("1,250 円/時"),
        "求人下限中央値 1,250 円/時: {block}"
    );
    // 1250 / 1163 = 1.0748
    assert!(block.contains("1.07 倍"), "最賃比 1.07 倍: {block}");
    assert!(block.contains("差額 +87 円"), "差額 1250-1163=+87: {block}");
}

/// NG5: 表9-F / 9-H は 1,250×167 = 208,750 円/月 を 1 回だけ換算して使う (二重換算しない)
#[test]
fn mix_f1_monthly_tables_do_not_double_convert() {
    use super::super::aggregator::salary_fixture;
    let ctx = tokyo_ctx();
    let html = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        Some(&ctx),
        super::ReportVariant::Full,
    );
    // 二重換算値 208,750 × 167 = 34,861,250
    assert!(
        !html.contains("34,861,250"),
        "月給換算値をさらに ×167 している"
    );
    let f = block_after(&html, "表 9-F");
    assert!(f.contains("208,750"), "表9-F 月給換算 208,750: {f}");
    // 消費支出 300,000 / 208,750 = 143.7%
    assert!(f.contains("143.7%"), "表9-F カバー率 143.7%: {f}");
    let h = block_after(&html, "表 9-H");
    // 想定 50m² 家賃 2,500×50 = 125,000、208,750 / 125,000 = 167%
    assert!(h.contains("125,000"), "表9-H 想定家賃 125,000: {h}");
    assert!(h.contains("167%"), "表9-H 月給カバー率 167%: {h}");
}

/// NG6: §09 KPI「求人給与」は 1250 円/時、最低賃金比の給与水準は 1250/1163×100 = 107
#[test]
fn mix_f1_market_intelligence_kpi_uses_hourly_median() {
    use super::super::aggregator::salary_fixture;
    let ctx = tokyo_ctx();
    let html = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        Some(&ctx),
        super::ReportVariant::MarketIntelligence,
    );
    let start = html.find("求人給与</div>").expect("§09 KPI 求人給与が必要");
    let kpi = &html[start..start + 120];
    assert!(
        kpi.contains("<div class=\"kpi-value\">1250<span class=\"kpi-unit\">円/時</span>"),
        "KPI 求人給与 1250 円/時: {kpi}"
    );
    let start = html.find("最低賃金比の給与水準</div>").expect("KPI 必要");
    let kpi = &html[start..start + 120];
    assert!(
        kpi.contains("<div class=\"kpi-value\">107<span"),
        "最低賃金比 107 (上限 200 張り付きではない): {kpi}"
    );
}

/// NG7: 本編 (SP) 表3-SP の中央値は 1,250 円/時 (時給求人の下限)
#[test]
fn mix_f1_sp_quartiles_use_hourly_values() {
    use super::super::aggregator::salary_fixture;
    let ctx = tokyo_ctx();
    let html = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        Some(&ctx),
        super::ReportVariant::Sp,
    );
    // 結論帯: 時給求人の下限 P25/P50/P75 (円/時)
    assert!(
        html.contains("時給求人の下限時給は中央値 1,250円/時"),
        "§03 結論帯は時給求人の下限時給"
    );
    let block = block_after(&html, "表 3-SP");
    assert!(block.contains("n=751"), "時給求人 751 件: {block}");
    assert!(
        block.contains(">1,250 円/時</td>"),
        "中央値 1,250 円/時: {block}"
    );
}

/// NG3 / NG2: 表3-E・図3-5・表3-F は月給換算 (万円) と明記し、円/時 を付けない
#[test]
fn mix_f1_cluster_tables_are_labelled_monthly_equivalent() {
    use super::super::aggregator::salary_fixture;
    let html = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        None,
        super::ReportVariant::Full,
    );
    assert!(
        !html.contains("時給換算表示 (月給/167h)"),
        "値は万円のままなのに見出しが時給換算表示"
    );
    let e = block_after(&html, "表 3-E");
    assert!(e.contains("月給換算"), "表3-E 見出しに月給換算: {e}");
    let f = block_after(&html, "表 3-F");
    assert!(!f.contains("円/時"), "表3-F に円/時: {f}");
    assert!(f.contains("万円"), "表3-F は万円表示: {f}");
}

/// 要確認10: §02 市区町村表の中央値は月給換算。時給モードでは注記を付ける (月給モードは不変)
#[test]
fn mix_f1_region_table_notes_monthly_equivalent_in_hourly_mode() {
    use super::super::aggregator::salary_fixture;
    let note = "時給モードでも本表の中央値は月給換算";
    let f1 = render_fixture_report_ctx(
        salary_fixture::F1_HOURLY_MOSTLY,
        "東京都 板橋区",
        None,
        super::ReportVariant::Full,
    );
    assert!(f1.contains(note), "時給モードの市区町村表に月給換算の注記");
    let f2 = render_fixture_report_ctx(
        salary_fixture::F2_MONTHLY_WITH_ANNUAL,
        "大阪府 大阪市",
        None,
        super::ReportVariant::Full,
    );
    assert!(!f2.contains(note), "月給モードには出さない");
}
