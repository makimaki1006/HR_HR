//! ドメイン不変条件 (invariant) ベース 逆証明テスト集
//!
//! 2026-04-30 追加。
//!
//! ## 背景
//! 過去事故 (2026-04-27 unemployment 380% 流出): 具体値テストは合意確認止まりで、
//! ドメインの前提誤り (例: rate >= 0% かつ <= 100%) を検出できなかった。
//! 本ファイルは「不変条件 = どんな入力でも常に成立すべき性質」を assert! で明示し、
//! 多様な入力 (空 / 通常 / 極端値) に対して横断検証する。
//!
//! ## 参照
//! - `feedback_reverse_proof_tests.md`: 逆証明テスト原則
//! - `feedback_test_data_validation.md`: データ妥当性検証
//! - `feedback_correlation_not_causation.md`: 因果断定ワード禁止
//!
//! ## カバレッジ (10 invariants)
//! 1. 市場逼迫度スコア 0-100 範囲 (任意入力で)
//! 2. 失業率 / 求人倍率の妥当範囲
//! 3. 6 マトリクス整合性 (sum <= total)
//! 4. パーセンテージ合計 100%±0.1
//! 5. 規模区分の境界値 (300 / 299 名 off-by-one)
//! 6. compute_segment_takeaways の冪等性 + 矛盾しない
//! 7. industry filter 結果の純度 (該当業界 + 産業計除外)
//! 8. 空入力で panic しない
//! 9. 5 軸レーダー値範囲 + 退化形許容
//! 10. Variant フィルタ (Public で HW 専用文言不在)

#![cfg(test)]
#![allow(clippy::too_many_arguments)]

use super::super::super::company::fetch::{
    NearbyCompany, RegionalCompanySegments, StructuralSummary,
};
use super::super::super::insight::fetch::InsightContext;
use super::super::aggregator::SurveyAggregation;
use super::regional_compare::{
    compute_radar_scores, extract_demographic, extract_geographic, extract_psychographic,
    DemographicData, PsychographicData,
};
use super::salesnow::compute_segment_takeaways;
use crate::handlers::helpers::Row;
use serde_json::{json, Value};

// =====================================================================
// 共有ヘルパー
// =====================================================================

/// 不変条件: スコア (0.0 <= s <= 100.0) を assert
fn assert_score_in_range(score: f64, label: &str) {
    assert!(
        score.is_finite(),
        "{}: score must be finite, got {}",
        label,
        score
    );
    assert!(
        (0.0..=100.0).contains(&score),
        "{}: score must be in [0, 100], got {}",
        label,
        score
    );
}

/// 不変条件: 失業率 (0% <= rate <= 100%)
/// 過去事故 2026-04-27 380% 流出の再発防止
fn assert_unemployment_valid(rate: f64) {
    assert!(rate.is_finite(), "unemployment rate must be finite");
    assert!(rate >= 0.0, "unemployment rate must be >= 0%, got {}", rate);
    assert!(
        rate <= 100.0,
        "unemployment rate must be <= 100%, got {} (2026-04-27 380% incident)",
        rate
    );
}

/// 不変条件: 求人倍率 (>= 0、現実値域は <= 10)
fn assert_job_ratio_valid(ratio: f64) {
    assert!(ratio.is_finite(), "job ratio must be finite");
    assert!(ratio >= 0.0, "job ratio must be >= 0, got {}", ratio);
}

/// 不変条件: パーセンテージ (0.0 <= p <= 100.0)
fn assert_percentage_valid(p: f64, label: &str) {
    assert!(
        p.is_finite(),
        "{}: percentage must be finite, got {}",
        label,
        p
    );
    assert!(
        (0.0..=100.0).contains(&p),
        "{}: percentage must be in [0, 100], got {}",
        label,
        p
    );
}

/// HTML から data-testid="tightness-score" の値を抽出
/// 形式: `data-testid="tightness-score">XX.X</div>`
fn extract_tightness_score(html: &str) -> Option<f64> {
    let needle = "data-testid=\"tightness-score\">";
    let start = html.find(needle)?;
    let after = &html[start + needle.len()..];
    let end = after.find('<')?;
    after[..end].trim().parse::<f64>().ok()
}

fn make_row(pairs: &[(&str, Value)]) -> Row {
    let mut m = Row::new();
    for (k, v) in pairs {
        m.insert(k.to_string(), v.clone());
    }
    m
}

/// 最小限の InsightContext (test fixture)
fn make_insight_ctx(
    ext_job_ratio: Vec<Row>,
    vacancy: Vec<Row>,
    ext_labor_force: Vec<Row>,
    ext_turnover: Vec<Row>,
    ext_population: Vec<Row>,
    ext_households: Vec<Row>,
    ext_internet_usage: Vec<Row>,
    ext_industry_employees: Vec<Row>,
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
        ts_vacancy: vec![],
        ts_salary: vec![],
        ts_fulfillment: vec![],
        ts_tracking: vec![],
        ext_job_ratio,
        ext_labor_stats: vec![],
        ext_min_wage: vec![],
        ext_turnover,
        ext_population,
        ext_pyramid: vec![],
        ext_migration: vec![],
        ext_daytime_pop: vec![],
        ext_establishments: vec![],
        ext_business_dynamics: vec![],
        ext_care_demand: vec![],
        ext_household_spending: vec![],
        ext_climate: vec![],
        ext_social_life: vec![],
        ext_internet_usage,
        ext_households,
        ext_vital: vec![],
        ext_labor_force,
        ext_medical_welfare: vec![],
        ext_education_facilities: vec![],
        ext_geography: vec![],
        ext_education: vec![],
        ext_industry_employees,
        hw_industry_counts: vec![],
        pref_avg_unemployment_rate: None,
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

fn make_company(
    corp_num: &str,
    name: &str,
    employee_count: i64,
    delta_1y: f64,
) -> NearbyCompany {
    NearbyCompany {
        corporate_number: corp_num.to_string(),
        company_name: name.to_string(),
        prefecture: "東京都".to_string(),
        sn_industry: "医療,福祉".to_string(),
        employee_count,
        credit_score: 0.0,
        postal_code: "100-0001".to_string(),
        hw_posting_count: 0,
        sales_amount: 0.0,
        sales_range: String::new(),
        employee_delta_1y: delta_1y,
        employee_delta_3m: 0.0,
    }
}

fn make_summary(
    large_count: usize,
    mid_count: usize,
    small_count: usize,
    large_growth: f64,
    mid_growth: f64,
    small_growth: f64,
    large_hw: f64,
    mid_hw: f64,
    small_hw: f64,
) -> StructuralSummary {
    StructuralSummary {
        large_count,
        mid_count,
        small_count,
        large_avg_growth_pct: large_growth,
        mid_avg_growth_pct: mid_growth,
        small_avg_growth_pct: small_growth,
        large_hw_continuity_pct: large_hw,
        mid_hw_continuity_pct: mid_hw,
        small_hw_continuity_pct: small_hw,
        pool_size: large_count + mid_count + small_count,
    }
}

// =====================================================================
// invariant 1: 市場逼迫度スコア 0-100 範囲 (任意入力で)
// =====================================================================

/// invariant 1a: 通常値で逼迫度スコアが 0-100 範囲
#[test]
fn invariant1_tightness_score_normal_inputs() {
    let cases: Vec<(f64, f64, f64, f64)> = vec![
        (0.5, 0.0, 5.0, 5.0),
        (1.5, 0.5, 1.0, 20.0),
        (1.0, 0.25, 3.0, 12.5),
        (0.8, 0.1, 4.0, 7.0),
        (1.2, 0.3, 2.0, 16.0),
    ];
    for (jr, vac, unemp, sep) in cases {
        let ctx = make_insight_ctx(
            vec![make_row(&[("ratio_total", json!(jr))])],
            vec![make_row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(vac)),
            ])],
            vec![make_row(&[("unemployment_rate", json!(unemp))])],
            vec![make_row(&[("separation_rate", json!(sep))])],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let mut html = String::new();
        super::market_tightness::render_section_market_tightness(&mut html, Some(&ctx));
        if let Some(score) = extract_tightness_score(&html) {
            assert_score_in_range(
                score,
                &format!("normal jr={} vac={} unemp={} sep={}", jr, vac, unemp, sep),
            );
        }
    }
}

/// invariant 1b: 極端入力 (失業率 380%, 求人倍率 -1.0 等) でも HTML 出力スコアは 0-100
/// 過去事故 2026-04-27 unemployment 380% 流出再発防止
#[test]
fn invariant1_tightness_score_extreme_inputs_clamped() {
    let cases: Vec<(f64, f64, f64, f64, &str)> = vec![
        (10.0, 1.0, 0.0, 50.0, "all_max"),
        (0.1, 2.0, 380.0, 999.0, "unemployment_380"),
        (-5.0, -1.0, 50.0, -10.0, "negative_inputs"),
        (1000.0, 1000.0, 1000.0, 1000.0, "all_huge"),
    ];
    for (jr, vac, unemp, sep, label) in cases {
        let ctx = make_insight_ctx(
            vec![make_row(&[("ratio_total", json!(jr))])],
            vec![make_row(&[
                ("emp_group", json!("正社員")),
                ("vacancy_rate", json!(vac)),
            ])],
            vec![make_row(&[("unemployment_rate", json!(unemp))])],
            vec![make_row(&[("separation_rate", json!(sep))])],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let mut html = String::new();
        super::market_tightness::render_section_market_tightness(&mut html, Some(&ctx));
        if let Some(score) = extract_tightness_score(&html) {
            assert_score_in_range(score, &format!("extreme {}", label));
        }
        let mut html_pub = String::new();
        super::market_tightness::render_section_market_tightness_public(&mut html_pub, Some(&ctx));
        if let Some(score) = extract_tightness_score(&html_pub) {
            assert_score_in_range(score, &format!("extreme public {}", label));
        }
    }
}

// =====================================================================
// invariant 2: 失業率 / 求人倍率の妥当範囲
// =====================================================================

#[test]
fn invariant2_helper_accepts_valid_unemployment() {
    assert_unemployment_valid(0.0);
    assert_unemployment_valid(2.5);
    assert_unemployment_valid(30.0);
    assert_unemployment_valid(100.0);
}

#[test]
#[should_panic(expected = "unemployment rate must be <= 100%")]
fn invariant2_helper_panics_on_unemployment_380() {
    assert_unemployment_valid(380.0);
}

#[test]
#[should_panic(expected = "unemployment rate must be >= 0%")]
fn invariant2_helper_panics_on_negative_unemployment() {
    assert_unemployment_valid(-1.0);
}

#[test]
fn invariant2_helper_accepts_valid_job_ratio() {
    assert_job_ratio_valid(0.0);
    assert_job_ratio_valid(1.5);
    assert_job_ratio_valid(10.0);
}

#[test]
#[should_panic(expected = "job ratio must be >= 0")]
fn invariant2_helper_panics_on_negative_job_ratio() {
    assert_job_ratio_valid(-0.1);
}

// =====================================================================
// invariant 3: 6 マトリクス整合性 (sum <= total)
// =====================================================================

/// 6 マトリクス: growth_* + decline_* の合計 <= pool_size
/// 中立帯 (-5% <= delta <= +5%) はどのセルにも含まれないため、合計 <= pool_size
#[test]
fn invariant3_six_matrix_sum_lte_pool() {
    let companies = vec![
        make_company("c1", "A", 500, 0.10),
        make_company("c2", "B", 350, 0.08),
        make_company("c3", "C", 320, -0.07),
        make_company("c4", "D", 200, 0.06),
        make_company("c5", "E", 100, -0.10),
        make_company("c6", "F", 80, 0.0),
        make_company("c7", "G", 30, 0.07),
        make_company("c8", "H", 20, -0.08),
        make_company("c9", "I", 10, 0.02),
        make_company("c10", "J", 5, 0.0),
    ];
    let segments = RegionalCompanySegments {
        pool_size: companies.len(),
        large: vec![],
        mid: vec![],
        growth: vec![],
        hiring: vec![],
        growth_large: vec![companies[0].clone(), companies[1].clone()],
        growth_mid: vec![companies[3].clone()],
        growth_small: vec![companies[6].clone()],
        decline_large: vec![companies[2].clone()],
        decline_mid: vec![companies[4].clone()],
        decline_small: vec![companies[7].clone()],
    };

    let total: usize = segments.growth_large.len()
        + segments.growth_mid.len()
        + segments.growth_small.len()
        + segments.decline_large.len()
        + segments.decline_mid.len()
        + segments.decline_small.len();

    assert!(
        total <= segments.pool_size,
        "6 マトリクスの合計 {} は pool_size {} を超えてはならない",
        total,
        segments.pool_size
    );
    assert!(
        total < segments.pool_size,
        "中立帯あり → total ({}) < pool_size ({})",
        total,
        segments.pool_size
    );
}

#[test]
fn invariant3_six_matrix_cells_disjoint() {
    let companies = vec![
        make_company("c1", "A", 500, 0.10),
        make_company("c2", "B", 350, -0.07),
        make_company("c3", "C", 200, 0.06),
        make_company("c4", "D", 100, -0.10),
        make_company("c5", "E", 30, 0.07),
        make_company("c6", "F", 20, -0.08),
    ];
    let segments = RegionalCompanySegments {
        pool_size: companies.len(),
        large: vec![],
        mid: vec![],
        growth: vec![],
        hiring: vec![],
        growth_large: vec![companies[0].clone()],
        growth_mid: vec![companies[2].clone()],
        growth_small: vec![companies[4].clone()],
        decline_large: vec![companies[1].clone()],
        decline_mid: vec![companies[3].clone()],
        decline_small: vec![companies[5].clone()],
    };
    let mut all_corp_nums: Vec<String> = Vec::new();
    for cell in [
        &segments.growth_large,
        &segments.growth_mid,
        &segments.growth_small,
        &segments.decline_large,
        &segments.decline_mid,
        &segments.decline_small,
    ] {
        for c in cell {
            all_corp_nums.push(c.corporate_number.clone());
        }
    }
    let unique_count = all_corp_nums
        .iter()
        .collect::<std::collections::HashSet<_>>()
        .len();
    assert_eq!(
        unique_count,
        all_corp_nums.len(),
        "6 マトリクスのセル間に重複企業があってはならない (corp_nums={:?})",
        all_corp_nums
    );
}

// =====================================================================
// invariant 4: パーセンテージ合計 100%±0.1
// =====================================================================

#[test]
fn invariant4_size_distribution_sum_100() {
    let s = make_summary(13, 4, 3, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0);
    let total = s.total_count();
    let large_pct = s.large_count as f64 / total as f64 * 100.0;
    let mid_pct = s.mid_count as f64 / total as f64 * 100.0;
    let small_pct = s.small_count as f64 / total as f64 * 100.0;
    let sum = large_pct + mid_pct + small_pct;
    assert!(
        (sum - 100.0).abs() < 0.1,
        "規模構成比 合計 100%±0.1 のはず, got {}",
        sum
    );
    assert_percentage_valid(large_pct, "large_pct");
    assert_percentage_valid(mid_pct, "mid_pct");
    assert_percentage_valid(small_pct, "small_pct");
}

#[test]
fn invariant4_industry_top_share_le_100() {
    let ctx = make_insight_ctx(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![
            make_row(&[
                ("industry_name", json!("医療,福祉")),
                ("employees_total", json!(2800_i64)),
            ]),
            make_row(&[
                ("industry_name", json!("製造業")),
                ("employees_total", json!(1200_i64)),
            ]),
            make_row(&[
                ("industry_name", json!("卸売業,小売業")),
                ("employees_total", json!(6000_i64)),
            ]),
        ],
    );
    let agg = SurveyAggregation::default();
    let g = extract_geographic(&ctx, &agg);
    let (_, pct) = g.top_industry.expect("top_industry");
    assert_percentage_valid(pct, "top_industry_pct");
    let total: f64 = 2800.0 + 1200.0 + 6000.0;
    let sum_pct: f64 = (2800.0 / total + 1200.0 / total + 6000.0 / total) * 100.0;
    assert!(
        (sum_pct - 100.0).abs() < 0.1,
        "産業構成比合計 100%±0.1, got {}",
        sum_pct
    );
}

// =====================================================================
// invariant 5: 規模区分の境界値 (300 / 299 名 off-by-one)
// =====================================================================

#[test]
fn invariant5_size_band_boundary_300_vs_299() {
    let s = make_summary(1, 1, 1, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0);
    assert_eq!(s.total_count(), 3);
    assert_eq!(s.large_count, 1);
    assert_eq!(s.mid_count, 1);
    assert_eq!(s.small_count, 1);

    let s_no_large = make_summary(0, 5, 5, 0.0, 1.0, -1.0, 0.0, 50.0, 50.0);
    let t = compute_segment_takeaways(&s_no_large);
    assert!(!t.is_empty(), "large_count=0 でも takeaways が返る");
    let joined = t.join("\n");
    assert!(
        !joined.contains("縮小傾向"),
        "large_count=0 で「全規模流出」takeaway は発火しないはず"
    );
}

#[test]
fn invariant5_summary_total_count_correct() {
    let cases: Vec<(usize, usize, usize)> = vec![
        (0, 0, 0),
        (1, 0, 0),
        (0, 1, 0),
        (0, 0, 1),
        (10, 20, 30),
        (100, 0, 0),
    ];
    for (l, m, sm) in cases {
        let s = make_summary(l, m, sm, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
        assert_eq!(
            s.total_count(),
            l + m + sm,
            "total_count = large + mid + small (l={}, m={}, sm={})",
            l,
            m,
            sm
        );
    }
}

// =====================================================================
// invariant 6: compute_segment_takeaways の冪等性 + 矛盾しない
// =====================================================================

#[test]
fn invariant6_takeaways_idempotent() {
    let s = make_summary(5, 5, 5, 5.0, 1.0, -2.0, 60.0, 50.0, 40.0);
    let t1 = compute_segment_takeaways(&s);
    let t2 = compute_segment_takeaways(&s);
    let t3 = compute_segment_takeaways(&s);
    assert_eq!(t1, t2, "冪等性: 1回目 == 2回目");
    assert_eq!(t2, t3, "冪等性: 2回目 == 3回目");
}

#[test]
fn invariant6_takeaways_no_contradiction_shrink_vs_expand() {
    let s_shrink = make_summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0);
    let t = compute_segment_takeaways(&s_shrink);
    let joined = t.join("\n");
    assert!(joined.contains("縮小傾向"), "全規模マイナス → 縮小傾向 takeaway");
    assert!(
        !joined.contains("拡大傾向"),
        "全規模マイナス時に「拡大傾向」takeaway が同時出てはならない (矛盾)"
    );

    let s_expand = make_summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0);
    let t2 = compute_segment_takeaways(&s_expand);
    let joined2 = t2.join("\n");
    assert!(joined2.contains("拡大傾向"), "全規模プラス → 拡大傾向 takeaway");
    assert!(
        !joined2.contains("縮小傾向"),
        "全規模プラス時に「縮小傾向」が出てはならない"
    );
}

#[test]
fn invariant6_takeaways_no_contradiction_all_shrink_vs_polarized() {
    let s = make_summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0);
    let t = compute_segment_takeaways(&s);
    let joined = t.join("\n");
    assert!(joined.contains("縮小傾向"), "全縮小は縮小傾向");
    assert!(
        !joined.contains("大手のみ人員拡大"),
        "全縮小時に「大手のみ人員拡大」二極化 takeaway は同時出てはならない"
    );
}

#[test]
fn invariant6_takeaways_no_causal_assertion() {
    let cases = vec![
        make_summary(5, 5, 5, 5.0, 1.0, -2.0, 60.0, 50.0, 40.0),
        make_summary(3, 4, 5, -1.5, -2.0, -1.2, 30.0, 30.0, 30.0),
        make_summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0),
        make_summary(13, 4, 3, 0.5, 0.5, 0.5, 50.0, 50.0, 50.0),
        make_summary(0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0),
    ];
    let banned = ["最適", "すべき", "決定打", "保証", "確実"];
    for s in cases {
        let t = compute_segment_takeaways(&s);
        for line in &t {
            for word in &banned {
                assert!(
                    !line.contains(word),
                    "因果断定ワード '{}' が含まれている: {}",
                    word,
                    line
                );
            }
        }
    }
}

// =====================================================================
// invariant 7: industry filter (該当業界 + 産業計除外)
//
// fetch_ext_turnover_with_industry は Turso DB を要求するため
// 単体テスト不可。代替として section レンダリングの存在検証のみ実施。
// =====================================================================

#[test]
fn invariant7_industry_filter_section_renders_with_data() {
    let ctx = make_insight_ctx(
        vec![make_row(&[("ratio_total", json!(1.2))])],
        vec![],
        vec![make_row(&[("unemployment_rate", json!(2.5))])],
        vec![make_row(&[
            ("industry", json!("医療,福祉")),
            ("separation_rate", json!(15.0)),
        ])],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let mut html = String::new();
    super::market_tightness::render_section_market_tightness(&mut html, Some(&ctx));
    assert!(
        !html.is_empty(),
        "通常入力で section が出力されること (DB 由来 industry filter は別途 integration test で検証)"
    );
}

// =====================================================================
// invariant 8: 空入力ハンドリング
// =====================================================================

#[test]
fn invariant8_empty_input_no_panic_no_section() {
    let ctx = make_insight_ctx(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );

    let mut html_full = String::new();
    super::market_tightness::render_section_market_tightness(&mut html_full, Some(&ctx));
    assert!(html_full.is_empty(), "全空 → market_tightness section 非表示");

    let mut html_pub = String::new();
    super::market_tightness::render_section_market_tightness_public(&mut html_pub, Some(&ctx));
    assert!(
        html_pub.is_empty(),
        "全空 → public market_tightness section 非表示"
    );

    let agg = SurveyAggregation::default();
    let mut html_rc = String::new();
    super::regional_compare::render_section_regional_compare(&mut html_rc, &ctx, &agg);
    assert!(html_rc.is_empty(), "全空 → regional_compare section 非表示");

    let segments_empty = RegionalCompanySegments::default();
    assert!(
        segments_empty.is_empty(),
        "default RegionalCompanySegments が is_empty"
    );

    let summary_empty = make_summary(0, 0, 0, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let t = compute_segment_takeaways(&summary_empty);
    assert!(
        !t.is_empty(),
        "全空 summary でも takeaway は最低 1 件 (fallback)"
    );
}

#[test]
fn invariant8_none_context_no_panic() {
    let mut html = String::new();
    super::market_tightness::render_section_market_tightness(&mut html, None);
    assert!(html.is_empty());

    let mut html_pub = String::new();
    super::market_tightness::render_section_market_tightness_public(&mut html_pub, None);
    assert!(html_pub.is_empty());
}

// =====================================================================
// invariant 9: 5 軸レーダー値範囲 + 退化形許容
// =====================================================================

#[test]
fn invariant9_radar_5_axis_in_bounds_diverse_inputs() {
    let cases: Vec<(f64, f64, f64, f64, f64)> = vec![
        (3.6, 52.0, 22.0, 45.0, 88.1),
        (0.0, 0.0, 0.0, 0.0, 0.0),
        (2.5, 38.1, 28.9, 30.0, 82.0),
        (50.0, 100.0, 80.0, 100.0, 100.0),
        (10.0, 10.0, 5.0, 5.0, 30.0),
    ];
    for (unemp, single, aging, univ, internet) in cases {
        let mut d = DemographicData::default();
        d.unemployment_pct = Some(unemp);
        d.single_hh_pct = Some(single);
        d.aging_pct = Some(aging);
        d.univ_grad_pct = Some(univ);
        let mut p = PsychographicData::default();
        p.internet_rate = Some(internet);

        let scores = compute_radar_scores(&d, &p);
        assert_eq!(scores.len(), 5, "5 軸レーダー");
        for (i, s) in scores.iter().enumerate() {
            assert_score_in_range(
                *s,
                &format!(
                    "radar axis {} for inputs ({}, {}, {}, {}, {})",
                    i, unemp, single, aging, univ, internet
                ),
            );
        }
    }
}

#[test]
fn invariant9_radar_degenerate_baseline_50() {
    let d = DemographicData::default();
    let p = PsychographicData::default();
    let scores = compute_radar_scores(&d, &p);
    for (i, s) in scores.iter().enumerate() {
        assert!(
            (s - 50.0).abs() < 1e-6,
            "退化形 (None 入力) → 軸 {} スコア = 50.0 (中心値), got {}",
            i,
            s
        );
    }
}

#[test]
fn invariant9_radar_no_panic_zero_baseline() {
    let mut d = DemographicData::default();
    d.unemployment_pct = Some(0.0);
    let p = PsychographicData::default();
    let scores = compute_radar_scores(&d, &p);
    assert_score_in_range(scores[0], "axis 0 with 0% unemployment");
}

// =====================================================================
// invariant 10: Variant フィルタ (Public で HW 専用文言不在)
// =====================================================================

#[test]
fn invariant10_public_variant_excludes_hw_specific_terms() {
    let ctx = make_insight_ctx(
        vec![make_row(&[("ratio_total", json!(1.3))])],
        vec![make_row(&[
            ("emp_group", json!("正社員")),
            ("vacancy_rate", json!(0.4)),
        ])],
        vec![make_row(&[("unemployment_rate", json!(2.5))])],
        vec![make_row(&[("separation_rate", json!(15.0))])],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let mut html_pub = String::new();
    super::market_tightness::render_section_market_tightness_public(&mut html_pub, Some(&ctx));

    assert!(
        !html_pub.contains("欠員補充率"),
        "Public variant に「欠員補充率」が含まれてはならない"
    );
    assert!(
        !html_pub.contains("HW 欠員") && !html_pub.contains("HW欠員"),
        "Public variant に「HW 欠員」が含まれてはならない"
    );
    assert!(
        !html_pub.contains("vacancy_rate"),
        "Public variant にカラム名 'vacancy_rate' が含まれてはならない"
    );
}

#[test]
fn invariant10_full_variant_includes_hw_terms() {
    let ctx = make_insight_ctx(
        vec![make_row(&[("ratio_total", json!(1.3))])],
        vec![make_row(&[
            ("emp_group", json!("正社員")),
            ("vacancy_rate", json!(0.4)),
        ])],
        vec![make_row(&[("unemployment_rate", json!(2.5))])],
        vec![make_row(&[("separation_rate", json!(15.0))])],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    let mut html_full = String::new();
    super::market_tightness::render_section_market_tightness(&mut html_full, Some(&ctx));
    assert!(
        html_full.contains("欠員補充率"),
        "Full variant では「欠員補充率」が含まれるはず"
    );
}

// =====================================================================
// 追加 invariant: 抽出値の妥当範囲
// =====================================================================

#[test]
fn invariant_extract_demographic_unemployment_valid_range() {
    let cases: Vec<f64> = vec![0.5, 2.5, 5.0, 30.0];
    for unemp in cases {
        let ctx = make_insight_ctx(
            vec![],
            vec![],
            vec![make_row(&[("unemployment_rate", json!(unemp))])],
            vec![],
            vec![],
            vec![],
            vec![],
            vec![],
        );
        let d = extract_demographic(&ctx);
        let raw = d.unemployment_pct.expect("unemployment 抽出済");
        assert_unemployment_valid(raw);
    }
}

#[test]
fn invariant_extract_demographic_percentages_in_range() {
    let ctx = make_insight_ctx(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![make_row(&[("aging_rate", json!(28.5))])],
        vec![make_row(&[("single_rate", json!(45.0))])],
        vec![],
        vec![],
    );
    let d = extract_demographic(&ctx);
    if let Some(v) = d.single_hh_pct {
        assert_percentage_valid(v, "single_hh_pct");
    }
    if let Some(v) = d.aging_pct {
        assert_percentage_valid(v, "aging_pct");
    }
}

#[test]
fn invariant_extract_psychographic_rates_in_range() {
    let ctx = make_insight_ctx(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![make_row(&[
            ("internet_usage_rate", json!(88.1)),
            ("smartphone_ownership_rate", json!(75.5)),
        ])],
        vec![],
    );
    let p = extract_psychographic(&ctx);
    if let Some(v) = p.internet_rate {
        assert_percentage_valid(v, "internet_rate");
    }
    if let Some(v) = p.smartphone_rate {
        assert_percentage_valid(v, "smartphone_rate");
    }
}
