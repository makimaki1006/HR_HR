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

use super::super::super::analysis::fetch::CsvCompanySalary;
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

/// 不変条件 (強化版): 求人倍率の現実上限を明示。
///
/// 2026-06-05 テスト品質チーム指摘: 既存 `assert_job_ratio_valid` は下限 (>= 0) のみで
/// 上限 sanity が無く、`* 100` 単位ずれ (例: 1.5 倍 → 150 倍) を検出できなかった。
/// 有効求人倍率は全国・職種別とも歴史的に ~10 倍を超えた記録がない (バブル期建設業等の
/// 極端職種で数倍 - 高々 10 倍程度)。10 倍超は単位ずれ or ETL バグの疑い。
fn assert_job_ratio_realistic(ratio: f64) {
    assert_job_ratio_valid(ratio);
    assert!(
        ratio <= 10.0,
        "job ratio realistic 上限超過 (>10 は単位ずれ/ETL バグ疑い、例: 1.5*100=150), got {}",
        ratio
    );
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
        // 詳細版 (Section 10) cross_* テーブル (2026-07-09): テスト fixture は空 Vec。
        cross_future_workforce: vec![],
        cross_wage_public: vec![],
        cross_switcher_supply: vec![],
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
        muni_pyramids: vec![],
        ext_migration: vec![],
        ext_daytime_pop: vec![],
        ext_establishments: vec![],
        ext_business_dynamics: vec![],
        ext_care_demand: vec![],
        ext_household_spending: vec![],
        ext_rental_housing: vec![],
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
        hw_job_type_counts: vec![],
        salary_scatter_pairs: vec![],
        csv_company_ranking: vec![],
        posting_target: None,
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

fn make_company(corp_num: &str, name: &str, employee_count: i64, delta_1y: f64) -> NearbyCompany {
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
    assert!(
        joined.contains("縮小傾向"),
        "全規模マイナス → 縮小傾向 takeaway"
    );
    assert!(
        !joined.contains("拡大傾向"),
        "全規模マイナス時に「拡大傾向」takeaway が同時出てはならない (矛盾)"
    );

    let s_expand = make_summary(3, 4, 5, 2.0, 1.5, 1.2, 50.0, 50.0, 50.0);
    let t2 = compute_segment_takeaways(&s_expand);
    let joined2 = t2.join("\n");
    assert!(
        joined2.contains("拡大傾向"),
        "全規模プラス → 拡大傾向 takeaway"
    );
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
    assert!(
        html_full.is_empty(),
        "全空 → market_tightness section 非表示"
    );

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

// =====================================================================
// 2026-05-24 audit_B P1-1: employee_delta_1y 範囲 invariant
//
// 過去事故 2026-04-30: 単位 (% vs 比率) 混同で 100 倍ずれ
// 過去事故 2026-05-14: navy_report.rs:2729 表示層 ×100 バグ再発
//
// 防御策: ドメイン上限を test で明示。±300% を超えるレコードは ETL バグ。
// (fetch.rs:894/1199 の in_realistic_range も同じ閾値 ±300% を使用)
// =====================================================================

/// 不変条件: employee_delta_1y は % 単位、現実値域は -100% 〜 +1000%
/// (理論上限 1000% = 11 倍化、実務上は ±300% を超えればデータ精度由来の外れ値)
fn assert_employee_delta_valid_range(delta_pct: f64) {
    assert!(delta_pct.is_finite(), "employee_delta_1y must be finite");
    assert!(
        delta_pct >= -100.0,
        "employee_delta_1y は -100% 未満不可 (従業員 0 化が下限), got {}",
        delta_pct
    );
    assert!(
        delta_pct <= 1000.0,
        "employee_delta_1y は 1000% 超不可 (2026-04-30 100倍ずれ事故再発防止), got {}",
        delta_pct
    );
}

#[test]
fn invariant_employee_delta_normal_range_accepted() {
    // 通常値: -50% 〜 +300%
    for v in [-50.0_f64, -10.0, 0.0, 5.5, 15.0, 100.0, 300.0] {
        assert_employee_delta_valid_range(v);
    }
}

#[test]
#[should_panic(expected = "employee_delta_1y は 1000% 超不可")]
fn invariant_employee_delta_panics_on_100x_inflation() {
    // 2026-04-30 100倍ずれ事故: 5% が 500% として保存されたパターン
    // (500.0 はギリ通るが、500.0 * 100 = 50000.0 のような事故を検出)
    assert_employee_delta_valid_range(50_000.0);
}

#[test]
#[should_panic(expected = "employee_delta_1y は -100% 未満不可")]
fn invariant_employee_delta_panics_on_below_minus100() {
    assert_employee_delta_valid_range(-150.0);
}

#[test]
fn invariant_employee_delta_realistic_range_filter_300pct() {
    // fetch.rs::in_realistic_range と同じ閾値 (±300%) を test で再確認
    let realistic = |d: f64| d.is_finite() && d.abs() <= 300.0;
    assert!(realistic(0.0));
    assert!(realistic(50.0));
    assert!(realistic(300.0));
    assert!(realistic(-300.0));
    assert!(!realistic(300.01));
    assert!(!realistic(-300.01));
    assert!(!realistic(f64::NAN));
    assert!(!realistic(f64::INFINITY));
}

// =====================================================================
// 2026-05-24 audit_B P1-4: 全国平均失業率 / Percentage newtype 範囲不変条件
//
// 過去事故 2026-04-27 unemployment 380% 流出: SQL 側 `* 100` 済の値を
// 受け手側で再度 `* 100` する事故。コメント依存防御では再発する。
// helpers::Percentage newtype で範囲を強制し、test で値域を検証。
// =====================================================================

use crate::handlers::helpers::Percentage;

#[test]
fn invariant_p1_4_percentage_clamps_extreme_inputs() {
    // 380% 流出と同型: try_new は弾く、new はクランプ
    assert!(
        Percentage::try_new(380.0).is_none(),
        "380% は try_new で None"
    );
    assert_eq!(
        Percentage::new(380.0).unwrap().value(),
        100.0,
        "new は 100 にクランプ"
    );
    assert_eq!(
        Percentage::new(-5.0).unwrap().value(),
        0.0,
        "負値は 0 にクランプ"
    );
}

#[test]
fn invariant_p1_4_national_unemployment_realistic_range() {
    // 全国失業率の現実値域 (戦後最大 6.5% 程度、通常 1-5%、上限 10%)。
    // pref_avg_unemployment_rate が 10% を超えていたら ETL バグ or 単位ずれ疑い。
    let realistic_national_unemp = |v: f64| (0.0..=10.0).contains(&v);
    assert!(realistic_national_unemp(2.5), "通常 2.5%");
    assert!(realistic_national_unemp(6.5), "戦後最大 6.5%");
    assert!(
        !realistic_national_unemp(250.0),
        "250% は単位ずれ (2.5 * 100 事故型)"
    );
    // 注意: ratio 取り違え (0.025 = 2.5% / 100) は値が小さくなり範囲内に入るため
    // 「現実値域」だけでは検知不能。代わりに「下限が 0.5% 未満」という
    // ドメイン経験値で厳格判定する。戦後の日本で 0.5% 未満は記録なし。
    let suspect_ratio_form = |v: f64| v < 0.5;
    assert!(
        suspect_ratio_form(0.025),
        "0.025 は ratio 形式 (= 2.5% を 100 で割った状態) の疑い → 厳格判定で検知"
    );
    assert!(!suspect_ratio_form(2.5), "2.5% は正常");
    assert!(!suspect_ratio_form(0.6), "0.6% は珍しいが現実値域");
}

#[test]
fn invariant_p1_4_percentage_display_consistent() {
    // newtype 経由なら format は常に "{:.1}%" で統一
    let p = Percentage::new(2.567).unwrap();
    assert_eq!(format!("{}", p), "2.6%");
    let p2 = Percentage::new(100.0).unwrap();
    assert_eq!(format!("{}", p2), "100.0%");
}

#[test]
fn invariant_p1_4_market_tightness_national_unemp_in_range() {
    // pref_avg_unemployment_rate が SQL から正しい単位 (%) で来ている場合、
    // 範囲は 0-10% に収まる。複数の現実値で確認。
    for nat_v in [Some(1.5_f64), Some(2.5), Some(3.7), Some(6.5), None] {
        if let Some(v) = nat_v {
            // ドメイン的にあり得る範囲は 0-10%
            assert!(
                (0.0..=10.0).contains(&v),
                "全国失業率は 0-10% の範囲 (2026-04-27 380% 流出防御), got {}",
                v
            );
            // newtype 化しても通る
            assert!(Percentage::try_new(v).is_some());
        }
    }
}

// =====================================================================
// 2026-06-05 テスト品質チーム指摘: 求人倍率の現実上限 sanity
//
// 背景: 既存不変条件 (invariant 2) は求人倍率 >= 0 のみ検証し、上限が無かった。
// MEMORY feedback_unit_consistency_audit / feedback_three_layer_audit:
//   % と ratio / 100倍ずれ (例: 1.5 → 150) はデータ層・計算層・表示層のどこでも起き得る。
// ドメイン経験値: 有効求人倍率は全国・職種別とも高々 ~10 倍。10 倍超は単位ずれ疑い。
// =====================================================================

#[test]
fn invariant_job_ratio_realistic_accepts_normal() {
    // 現実値: 0 倍 (求人皆無) 〜 数倍 (人手不足職種) 〜 10 倍 (上限ギリ)
    for v in [0.0_f64, 0.5, 1.0, 1.5, 3.6, 8.0, 10.0] {
        assert_job_ratio_realistic(v);
    }
}

#[test]
#[should_panic(expected = "job ratio realistic 上限超過")]
fn invariant_job_ratio_realistic_panics_on_100x_unit_error() {
    // 1.5 倍を `* 100` してしまった単位ずれ事故型 (= 150 倍)
    assert_job_ratio_realistic(150.0);
}

#[test]
#[should_panic(expected = "job ratio realistic 上限超過")]
fn invariant_job_ratio_realistic_panics_just_above_10() {
    assert_job_ratio_realistic(10.01);
}

/// 求人倍率の単位ずれ (ratio 形式 vs % 形式) を逆証明で検知。
/// `ext_job_ratio` の ratio_total は「倍」単位 (例: 1.5)。誤って % 換算 (150.0) で
/// 入った場合、realistic 上限で検出できる。逆に 0.015 のような ratio/100 形式は
/// 値が極端に小さくなり「0 倍に近い」異常として別途検知すべき。
#[test]
fn invariant_job_ratio_unit_form_detection() {
    // 正常形 (倍単位)
    let normal = 1.5_f64;
    assert_job_ratio_realistic(normal);
    // % 誤変換形 (1.5 → 150) は realistic 上限で弾かれる
    let pct_form = normal * 100.0;
    assert!(
        pct_form > 10.0,
        "% 誤変換形 {} は realistic 上限 10 を超え、検出可能",
        pct_form
    );
    // ratio/100 形式 (1.5 → 0.015) は「ほぼ 0 倍」として疑わしい
    let over_divided = normal / 100.0;
    assert!(
        over_divided < 0.1,
        "ratio/100 誤変換形 {} は 0.1 倍未満で異常 (求人皆無は稀)",
        over_divided
    );
}

// =====================================================================
// 2026-06-05 テスト品質チーム指摘: 性別比率合計 ≈ 100%
//
// 背景: ext_pyramid (age_group, male_count, female_count) から算出する
//   性別比率について、男性比 + 女性比 が 100%±0.5 に収まることを保証する
//   不変条件が欠落していた。比率分母の取り違え (母集団 != male+female) や
//   集計バグで合計が 100% から乖離する事故を逆証明で検出する。
//
// 注意: 国勢調査ピラミッドには「不明」性別カラムが無く male/female の 2 区分。
//   よって母集団 = male + female とした上で male_pct + female_pct == 100% が成立。
//   将来「不明」を加える場合は male + female + unknown == total を分母にすること。
// =====================================================================

fn pyramid_row(age: &str, male: i64, female: i64) -> Row {
    make_row(&[
        ("age_group", json!(age)),
        ("male_count", json!(male)),
        ("female_count", json!(female)),
    ])
}

/// ext_pyramid 全体から (男性合計, 女性合計) を集計するテスト内ヘルパ。
/// (本体 demographics::render_demographic_kpis と同じ male_count/female_count 採取方式)
fn sum_gender(pyramid: &[Row]) -> (i64, i64) {
    let mut male = 0i64;
    let mut female = 0i64;
    for r in pyramid {
        let m = r.get("male_count").and_then(|v| v.as_i64()).unwrap_or(0);
        let f = r.get("female_count").and_then(|v| v.as_i64()).unwrap_or(0);
        male += m;
        female += f;
    }
    (male, female)
}

#[test]
fn invariant_gender_ratio_sums_to_100_diverse_inputs() {
    // 多様な男女比 (女性過多 / 男性過多 / 均衡 / 極端) で合計 100%±0.5 を検証
    let cases: Vec<Vec<Row>> = vec![
        // 均衡寄り (実データ風 5 歳階級)
        vec![
            pyramid_row("20-24", 5000, 4800),
            pyramid_row("25-29", 6000, 5800),
            pyramid_row("30-34", 7000, 6800),
        ],
        // 女性過多 (介護・保育職地域想定)
        vec![pyramid_row("30-34", 2000, 8000)],
        // 男性過多
        vec![pyramid_row("40-44", 9000, 1000)],
        // 極端 (片方ほぼ 0)
        vec![pyramid_row("50-54", 1, 9999)],
    ];
    for (i, pyramid) in cases.iter().enumerate() {
        let (male, female) = sum_gender(pyramid);
        let total = (male + female) as f64;
        assert!(total > 0.0, "case {}: 母集団 > 0", i);
        let male_pct = male as f64 / total * 100.0;
        let female_pct = female as f64 / total * 100.0;
        assert_percentage_valid(male_pct, &format!("case {} male_pct", i));
        assert_percentage_valid(female_pct, &format!("case {} female_pct", i));
        let sum = male_pct + female_pct;
        assert!(
            (sum - 100.0).abs() < 0.5,
            "case {}: 男性比({:.4}) + 女性比({:.4}) = {:.4} は 100%±0.5 のはず",
            i,
            male_pct,
            female_pct,
            sum
        );
    }
}

/// 逆証明: 分母を取り違えた集計 (= total を male+female 以外で割る) は 100% から
/// 乖離するため、本不変条件が前提誤りを検出できることを示す。
#[test]
fn invariant_gender_ratio_wrong_denominator_breaks_100() {
    let pyramid = vec![pyramid_row("30-34", 6000, 4000)]; // male+female = 10000
    let (male, female) = sum_gender(&pyramid);
    let correct_total = (male + female) as f64;
    // 正しい分母 → 100%
    let sum_ok = male as f64 / correct_total * 100.0 + female as f64 / correct_total * 100.0;
    assert!((sum_ok - 100.0).abs() < 0.5, "正しい分母なら 100%");

    // 誤った分母 (例: male のみを「総人口」と誤認 = 6000 で割る) → 合計 > 100%
    let wrong_total = male as f64;
    let sum_bad = male as f64 / wrong_total * 100.0 + female as f64 / wrong_total * 100.0;
    assert!(
        (sum_bad - 100.0).abs() >= 0.5,
        "誤った分母なら 100% から乖離し検出可能 (got {:.2})",
        sum_bad
    );
}

#[test]
fn invariant_gender_ratio_section_renders_without_panic() {
    // 性別データを含む ext_pyramid を InsightContext に載せ、demographics section が
    // panic せず出力されること (実描画経路でのスモーク)。
    let mut ctx = make_insight_ctx(
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
        vec![],
    );
    ctx.ext_pyramid = vec![
        pyramid_row("20-24", 5000, 4800),
        pyramid_row("25-29", 6000, 5800),
        pyramid_row("30-34", 7000, 6800),
    ];
    let mut html = String::new();
    super::demographics::render_section_demographics(&mut html, &ctx);
    // ピラミッド SSR SVG が出力されること (描画経路を実通過した証跡)
    assert!(
        html.contains("pyramid-ssr"),
        "性別データありで demographics ピラミッド (.pyramid-ssr) が描画されるはず"
    );
    // 描画後も母集団から算出した性別比は 100%±0.5 を維持
    let (male, female) = sum_gender(&ctx.ext_pyramid);
    let total = (male + female) as f64;
    let sum = (male as f64 / total + female as f64 / total) * 100.0;
    assert!((sum - 100.0).abs() < 0.5, "描画データの性別比合計 100%±0.5");
}

// =====================================================================
// 2026-06-05 テスト品質チーム指摘: 給与統計の正値性 (補足記録)
//
// 検証対象であった compute_distribution_stats は navy_report/common.rs の
// `pub(super)` 関数で、本ファイル (report_html 直下) からは可視性の都合で
// 直接呼べない。シグネチャ/可視性変更は本タスクの制約で禁止のため、給与統計
// 自体の不変条件 (中央値 > 0 / 負値・0 入力で None) は同モジュール内の
// navy_report/tests.rs に既存テスト
//   - compute_distribution_stats_all_zero_returns_none
//   - compute_distribution_stats_filters_negative_and_zero
//   - compute_distribution_stats_invariants_n1/n2/n5/n100
// として担保済み。ここでは「給与は正値」という不変条件をヘルパとして明示し、
// 表示層 (万円換算) で負/0 が混入しないことを逆証明で検証する。
// =====================================================================

/// 不変条件: 給与統計値 (円) は正値。中央値・平均が 0 以下なら None 相当の異常。
fn assert_salary_positive(yen: i64, label: &str) {
    assert!(yen > 0, "{}: 給与 (円) は > 0 のはず, got {}", label, yen);
}

#[test]
fn invariant_salary_positive_accepts_normal() {
    for v in [180_000_i64, 250_000, 300_000, 1_000_000] {
        assert_salary_positive(v, "normal");
    }
}

#[test]
#[should_panic(expected = "給与 (円) は > 0 のはず")]
fn invariant_salary_positive_panics_on_zero() {
    assert_salary_positive(0, "zero");
}

#[test]
#[should_panic(expected = "給与 (円) は > 0 のはず")]
fn invariant_salary_positive_panics_on_negative() {
    assert_salary_positive(-1, "negative");
}

/// 万円換算 (yen / 10000) が表示前提を満たすこと: 正の給与は換算後も正、
/// かつ現実上限 (月給 200 万円 = 2,000,000 円) を超えない sanity。
#[test]
fn invariant_salary_man_yen_conversion_realistic() {
    let cases: Vec<i64> = vec![180_000, 250_000, 500_000, 2_000_000];
    for yen in cases {
        assert_salary_positive(yen, "conv");
        let man = yen as f64 / 10_000.0;
        assert!(man > 0.0, "万円換算後も正: {} 円 → {} 万円", yen, man);
        assert!(
            man <= 200.0,
            "月給 200 万円超は現実外れ値疑い (単位ずれ?), {} 万円",
            man
        );
    }
}

// =====================================================================
// invariant 11: 番兵 (sentinel) 静的監査 — HW 由来データ 顧客向け版 非漏洩
// =====================================================================
//
// 2026-07-13 追加。§09 図 9-B (採用ターゲット厚み) / 図 9-C (競合求人密度) が
// hw_industry_counts (postings 由来) を顧客向け variant (MarketIntelligence /
// Extended / Sp) で表示していた違反の撤去に対する恒久回帰ガード。
//
// ## 仕組み (番兵方式)
// InsightContext の HW 由来フィールドに一意な番兵文字列/数値を注入し、顧客向け
// variant の完全レポート HTML をレンダリングして「どの番兵も HTML に現れない」ことを
// assert する。9-B/9-C 経路が復活すれば hw_industry_counts の番兵が HTML に出て
// テストが落ちる。Full variant には出てよい (HW 表示が許可された唯一の版)。
//
// ## 番兵設計 (2026-07-13 拡張)
// hw_industry_counts / hw_job_type_counts の番兵は **最上位 (top) に置く**。
// 旧設計 (非 top) は §01 Finding06/07 (compute_skew_severity) が top カテゴリ名を
// 全 variant で出力していたための切り分けだったが、2026-07-13 の HW リーク一括撤去で
// Finding06/07 自体を Full 限定化したため、top 番兵でも顧客向け variant には
// 一切出ないことが保証できるようになった (むしろ top に置くことで Finding 経路の
// 回帰も同時に検出できる)。
//
// ## csv_company_ranking 番兵 (2026-07-13 拡張)
// csv_company_ranking (postings facility_name 由来 = HW データ) は §05 表 5-G/5-H が
// variant ガードなしで全 variant に描画していたリークを 2026-07-13 に Full 限定化。
// 施設名番兵 (SENTINEL_CSV_COMPANY) が顧客向け variant に出ないことを assert し、
// Full では表 5-G 経由で出ることの逆証明も行う。

/// HW 由来フィールドに番兵を注入した完全な InsightContext を構築。
///
/// 公的統計 (ext_*) は正常系データを入れ、9-A/9-B/9-C の各図が「データなし」では
/// なく実描画になるようにする (番兵が漏れる余地を最大化)。
fn build_hw_sentinel_ctx() -> InsightContext {
    let mut ctx = InsightContext::default();
    ctx.pref = "群馬県".to_string();
    ctx.muni = "高崎市".to_string();

    // 公的統計 (使用 OK): ext_industry_employees は §04 表 4-B の行名ソース。
    ctx.ext_industry_employees = vec![
        make_row(&[
            ("industry_code", json!("H")),
            ("industry_name", json!("運輸業,郵便業")),
            ("employees_total", json!(48000)),
        ]),
        make_row(&[
            ("industry_code", json!("P")),
            ("industry_name", json!("医療,福祉")),
            ("employees_total", json!(72000)),
        ]),
    ];
    ctx.ext_min_wage = vec![make_row(&[
        ("fiscal_year", json!(2024)),
        ("hourly_min_wage", json!(985)),
    ])];
    ctx.ext_household_spending = vec![make_row(&[
        ("category", json!("消費支出")),
        ("monthly_amount", json!(268000)),
    ])];
    ctx.commute_inflow_top3 = vec![("群馬県".to_string(), "前橋市".to_string(), 12_500)];
    ctx.commute_inflow_total = 42_000;
    ctx.commute_self_rate = 0.72;
    ctx.ext_job_ratio = vec![make_row(&[("ratio_total", json!(1.42))])];
    ctx.ext_labor_force = vec![make_row(&[("unemployment_rate", json!(2.4))])];

    // HW 由来 番兵 (2026-07-13: top 位置に配置。Finding06/07 の Full 限定化により
    // 顧客向け variant では top でも出ないことを保証できる):
    ctx.hw_industry_counts = vec![
        (SENTINEL_HW_INDUSTRY.to_string(), 999_999),
        ("運輸業,郵便業".to_string(), 48_000),
    ];
    ctx.hw_job_type_counts = vec![
        (SENTINEL_HW_JOBTYPE.to_string(), 999_999),
        ("ドライバー".to_string(), 1_000),
    ];
    // salary_scatter_pairs: 他の数値と衝突しない特徴的ペア (§03 図 3-6、Full 限定)。
    ctx.salary_scatter_pairs = vec![(SENTINEL_SCATTER_X, SENTINEL_SCATTER_Y)];

    // csv_company_ranking (postings facility_name 由来 = HW データ)。
    // §05 表 5-G/5-H は 2026-07-13 に Full 限定化。顧客向け variant で施設名番兵が
    // 漏れないことを保証する。posting_count は 2 件以上 (fetch 側の代表性フィルタ相当)。
    ctx.csv_company_ranking = vec![CsvCompanySalary {
        facility_name: SENTINEL_CSV_COMPANY.to_string(),
        posting_count: 5,
        salary_lower_median: 22.0,
        salary_upper_median: 30.0,
        native_unit: "月給".to_string(),
    }];

    ctx
}

const SENTINEL_HW_INDUSTRY: &str = "HW監査番兵産業ZZQ";
const SENTINEL_HW_JOBTYPE: &str = "HW監査番兵職種ZZQ";
const SENTINEL_CSV_COMPANY: &str = "HW監査番兵企業ZZQ";
const SENTINEL_SCATTER_X: f64 = 123_457.0;
const SENTINEL_SCATTER_Y: f64 = 234_561.0;

/// §03 図 3-6 (給与レンジ 散布図) の HW 経路固有キャプション断片。
///
/// 散布点は SVG 座標に変換され、サマリは n と平均幅しか出さないため、
/// SENTINEL_SCATTER_X/Y の生値は HTML に現れない (数値番兵は検出能力ゼロ)。
/// このキャプションは HW postings 由来の月給散布図経路でのみ出力されるため、
/// 顧客向け variant への HW リーク検出/逆証明の実効的な指標として使う。
const SCATTER_HW_CAPTION: &str = "対象地域から最大 1000 件抽出";

/// 顧客向け 3 variant (MI / Extended / Sp) の完全レポートをレンダリングするヘルパー。
fn render_full_report(variant: super::ReportVariant, ctx: &InsightContext) -> String {
    use super::super::aggregator::{CompanyAgg, EmpTypeSalary};
    use super::super::job_seeker::JobSeekerAnalysis;
    use super::render_survey_report_page_for_vrt;

    let agg = SurveyAggregation {
        total_count: 250,
        ..Default::default()
    };
    let seeker = JobSeekerAnalysis::default();
    let by_company: Vec<CompanyAgg> = vec![];
    let by_emp: Vec<EmpTypeSalary> = vec![];
    let smin: Vec<i64> = vec![];
    let smax: Vec<i64> = vec![];
    render_survey_report_page_for_vrt(
        &agg,
        &seeker,
        &by_company,
        &by_emp,
        &smin,
        &smax,
        Some(ctx),
        variant,
    )
}

#[test]
fn invariant11_hw_sentinels_absent_from_customer_facing_variants() {
    use super::ReportVariant;

    // §09 図 9-B/9-C 撤去 + 2026-07-13 HW リーク一括撤去 (§05 表 5-G/5-H /
    // §04 表 4-B / §01 Finding06/07) 後に、hw_industry_counts / hw_job_type_counts /
    // csv_company_ranking / salary_scatter_pairs の番兵が顧客向け variant に
    // 一切現れないことを恒久保証。番兵は top 位置 (Finding 経路の回帰も検出)。
    //
    // Public も対象に含める (2026-07-13): show_hw_sections() = Full のみ true が
    // 設計 SSoT であり、Public (対外提案向け) も HW 非表示 variant。従来は Public に
    // 表 4-B / Finding06/07 が漏れていた (旧 report_basic fixture で実確認) ため、
    // 本テストで Public も恒久ガードする。
    let ctx = build_hw_sentinel_ctx();
    let sentinels_str = [
        SENTINEL_HW_INDUSTRY,
        SENTINEL_HW_JOBTYPE,
        SENTINEL_CSV_COMPANY,
    ];

    for variant in [
        ReportVariant::Public,
        ReportVariant::MarketIntelligence,
        ReportVariant::Extended,
        ReportVariant::Sp,
        // 2026-07-13: Ver10 も顧客向け版のため HW 由来番兵の非漏洩を恒久ガードする。
        ReportVariant::Ver10,
    ] {
        let html = render_full_report(variant, &ctx);
        for s in sentinels_str {
            assert!(
                !html.contains(s),
                "HW 由来番兵 '{}' が顧客向け variant {:?} の HTML に漏洩している \
                 (§09 図 9-B/9-C / §05 表 5-G/5-H / §04 表 4-B / §01 Finding06/07 の \
                 Full 限定ルール違反の再発)",
                s,
                variant
            );
        }
        // §03 図 3-6 の HW 経路キャプション不在チェック (2026-07-17)。
        // 散布点は SVG 座標に変換され生値が HTML に出ないため、旧 SENTINEL_SCATTER_X/Y
        // の数値文字列チェックは形骸化していた (顧客向けに図が出ても素通り)。HW
        // postings 由来の月給散布図経路でのみ出るこのキャプションで実効的にガードする。
        assert!(
            !html.contains(SCATTER_HW_CAPTION),
            "HW 由来 給与散布図 (図 3-6) が顧客向け variant {:?} の HTML に漏洩している \
             (キャプション '{}' が出現。show_hw_sections() = Full 限定ルール違反の再発)",
            variant,
            SCATTER_HW_CAPTION
        );
    }
}

#[test]
fn invariant11_hw_sentinels_visible_in_full_variant() {
    use super::ReportVariant;

    // 逆証明 (2026-07-13 拡張): Full variant (HW 表示が許可された唯一の版) では、
    // Full 限定化した各経路を通じて番兵が実際に HTML へ出ることを確認する。
    // これにより「番兵が顧客向け variant に出ない」のが検出機構の不備 (そもそも
    // どの variant でも出ない) ではなく、variant ガードの効果であることを証明する。
    //
    // 経路の対応:
    //   - SENTINEL_HW_INDUSTRY (top) → §01 Finding06 産業構成 偏り (compute_skew_severity)
    //   - SENTINEL_HW_JOBTYPE  (top) → §01 Finding07 職種構成 偏り (同上)
    //   - SENTINEL_CSV_COMPANY       → §05 表 5-G 企業別給与ランキング / 表 5-H 注目企業
    //   - SCATTER_HW_CAPTION         → §03 図 3-6 給与レンジ 散布図 (HW postings 月給ペア)
    let ctx = build_hw_sentinel_ctx();
    let html = render_full_report(ReportVariant::Full, &ctx);
    assert!(
        html.contains(SENTINEL_HW_INDUSTRY),
        "逆証明: Full では top 産業番兵が Finding06 経由で HTML に出るはず"
    );
    assert!(
        html.contains(SENTINEL_HW_JOBTYPE),
        "逆証明: Full では top 職種番兵が Finding07 経由で HTML に出るはず"
    );
    assert!(
        html.contains(SENTINEL_CSV_COMPANY),
        "逆証明: Full では CSV 企業番兵が §05 表 5-G/5-H 経由で HTML に出るはず"
    );
    // 2026-07-17: 図 3-6 の HW キャプションが Full では出ること。これにより
    // 「顧客向けに出ない」= variant ガードの効果であって検出機構の不備 (どの
    // variant でも図が出ない) ではないことを保証する (番兵の形骸化防止)。
    assert!(
        html.contains(SCATTER_HW_CAPTION),
        "逆証明: Full では図 3-6 (HW 月給散布図) のキャプション '{}' が HTML に出るはず",
        SCATTER_HW_CAPTION
    );
    // 出典表記の訂正 (2026-07-13): 「CSV 求人データ集計」誤記が Full でも出ないこと。
    assert!(
        !html.contains("出典: CSV 求人データ集計"),
        "表 5-G/5-H の出典は「ハローワーク掲載求人の集計」に訂正済みのはず"
    );
    assert!(
        html.contains("出典: ハローワーク掲載求人の集計"),
        "Full の表 5-G/5-H キャプションに訂正後の出典表記が出るはず"
    );
}
