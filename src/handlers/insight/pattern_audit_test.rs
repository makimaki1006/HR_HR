//! Team β: Insight Engine 全 38 パターンの発火条件 × severity 判定の徹底監査
//!
//! 監査原則（MEMORY `feedback_reverse_proof_tests` 準拠）:
//! - 「要素存在」ではなく「具体値でのトリガ×severity」を逆証明
//! - 閾値は source 定数を参照し、ハードコードしない
//! - 各 pattern につき最低 3 ケース（境界+ε / 境界-ε / 欠損で None）
//! - 矛盾発火 (cross-pattern interaction) も検証
//!
//! 対象:
//! - 既存 22: HS-1〜HS-6, FC-1〜FC-4, RC-1〜RC-3, AP-1〜AP-3, CZ-1〜CZ-3, CF-1〜CF-3
//! - 構造 6:  LS-1, LS-2, HH-1, MF-1, IN-1, GE-1 (SSDSE-A Phase A)
//! - Flow 10: SW-F01〜SW-F10
//!
//! 計 38 pattern。engine.rs / engine_flow.rs / helpers.rs は**無変更**。

#![cfg(test)]

use super::engine::generate_insights;
use super::engine_flow::analyze_flow_insights;
use super::fetch::InsightContext;
use super::flow_context::FlowIndicators;
use super::helpers::*;
use serde_json::Value;
use std::collections::HashMap;

// ======== Row ビルダー ========

type Row = HashMap<String, Value>;

fn row(pairs: &[(&str, Value)]) -> Row {
    let mut r = Row::new();
    for (k, v) in pairs {
        r.insert((*k).to_string(), v.clone());
    }
    r
}

fn v_f(x: f64) -> Value {
    Value::from(x)
}
fn v_i(x: i64) -> Value {
    Value::from(x)
}
fn v_s(x: &str) -> Value {
    Value::from(x.to_string())
}

// ======== InsightContextBuilder ========

struct Ctx {
    inner: InsightContext,
}

impl Ctx {
    fn new() -> Self {
        Self {
            inner: InsightContext {
                vacancy: vec![],
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
                ext_job_ratio: vec![],
                ext_labor_stats: vec![],
                ext_min_wage: vec![],
                ext_turnover: vec![],
                ext_population: vec![],
                ext_pyramid: vec![],
                ext_migration: vec![],
                ext_daytime_pop: vec![],
                ext_establishments: vec![],
                ext_business_dynamics: vec![],
                ext_care_demand: vec![],
                ext_household_spending: vec![],
                ext_climate: vec![],
                ext_households: vec![],
                ext_vital: vec![],
                ext_labor_force: vec![],
                ext_medical_welfare: vec![],
                ext_education_facilities: vec![],
                ext_geography: vec![],
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
            },
        }
    }

    fn build(self) -> InsightContext {
        self.inner
    }

    fn vacancy_seishain(mut self, vacancy_rate: f64, total_count: f64) -> Self {
        self.inner.vacancy.push(row(&[
            ("emp_group", v_s("正社員")),
            ("vacancy_rate", v_f(vacancy_rate)),
            ("total_count", v_f(total_count)),
        ]));
        self
    }

    fn ts_vacancy_seishain(mut self, rates: &[f64]) -> Self {
        for r in rates {
            self.inner.ts_vacancy.push(row(&[
                ("emp_group", v_s("正社員")),
                ("vacancy_rate", v_f(*r)),
            ]));
        }
        self
    }

    fn salary_comp_seishain(
        mut self,
        comp_index: f64,
        local_mean: f64,
        national_mean: f64,
        national_median: f64,
    ) -> Self {
        self.inner.salary_comp.push(row(&[
            ("emp_group", v_s("正社員")),
            ("competitiveness_index", v_f(comp_index)),
            ("local_mean_min", v_f(local_mean)),
            ("national_mean_min", v_f(national_mean)),
            ("national_median_min", v_f(national_median)),
        ]));
        self
    }

    #[allow(dead_code)]
    fn wage_compliance_below(mut self, below_count: i64) -> Self {
        self.inner.wage_compliance.push(row(&[
            ("emp_group", v_s("正社員")),
            ("below_count", v_i(below_count)),
        ]));
        self
    }

    fn transparency(mut self, avg: f64, worst_col: &str, worst_rate: f64) -> Self {
        let cols = [
            "disclosure_annual_holidays",
            "disclosure_bonus_months",
            "disclosure_employee_count",
            "disclosure_overtime",
            "disclosure_female_ratio",
            "disclosure_parttime_ratio",
        ];
        let mut pairs: Vec<(&str, Value)> =
            vec![("emp_group", v_s("正社員")), ("avg_transparency", v_f(avg))];
        for c in cols.iter() {
            let v = if *c == worst_col { worst_rate } else { 0.8 };
            pairs.push((c, v_f(v)));
        }
        self.inner.transparency.push(row(&pairs));
        self
    }

    fn temperature(mut self, temp: f64, urgency: f64) -> Self {
        self.inner.temperature.push(row(&[
            ("emp_group", v_s("正社員")),
            ("temperature", v_f(temp)),
            ("urgency_density", v_f(urgency)),
        ]));
        self
    }

    fn monopsony(mut self, hhi: f64, top1: f64) -> Self {
        self.inner.monopsony.push(row(&[
            ("emp_group", v_s("正社員")),
            ("hhi", v_f(hhi)),
            ("top1_share", v_f(top1)),
        ]));
        self
    }

    fn spatial_mismatch(mut self, isolation: f64, accessible_salary: f64) -> Self {
        self.inner.spatial_mismatch.push(row(&[
            ("emp_group", v_s("正社員")),
            ("isolation_score", v_f(isolation)),
            ("accessible_avg_salary_30km", v_f(accessible_salary)),
        ]));
        self
    }

    fn ext_daytime_ratio(mut self, ratio: f64) -> Self {
        self.inner
            .ext_daytime_pop
            .push(row(&[("daytime_ratio", v_f(ratio))]));
        self
    }

    fn ts_counts_seishain(mut self, values: &[f64]) -> Self {
        for v in values {
            self.inner.ts_counts.push(row(&[
                ("emp_group", v_s("正社員")),
                ("posting_count", v_f(*v)),
            ]));
        }
        self
    }

    fn ts_salary_seishain(mut self, values: &[f64]) -> Self {
        for v in values {
            self.inner
                .ts_salary
                .push(row(&[("emp_group", v_s("正社員")), ("mean_min", v_f(*v))]));
        }
        self
    }

    fn ext_min_wage(mut self, values: &[f64]) -> Self {
        for v in values {
            self.inner
                .ext_min_wage
                .push(row(&[("hourly_min_wage", v_f(*v))]));
        }
        self
    }

    fn pyramid(mut self, age: &str, male: f64, female: f64) -> Self {
        self.inner.ext_pyramid.push(row(&[
            ("age_group", v_s(age)),
            ("male_count", v_f(male)),
            ("female_count", v_f(female)),
        ]));
        self
    }

    fn migration(mut self, inflow: f64, outflow: f64) -> Self {
        self.inner
            .ext_migration
            .push(row(&[("inflow", v_f(inflow)), ("outflow", v_f(outflow))]));
        self
    }

    fn ts_fulfillment_seishain(mut self, values: &[f64]) -> Self {
        for v in values {
            self.inner.ts_fulfillment.push(row(&[
                ("emp_group", v_s("正社員")),
                ("avg_listing_days", v_f(*v)),
            ]));
        }
        self
    }

    fn ts_tracking_seishain(mut self, values: &[f64]) -> Self {
        for v in values {
            self.inner.ts_tracking.push(row(&[
                ("emp_group", v_s("正社員")),
                ("churn_rate", v_f(*v)),
            ]));
        }
        self
    }

    fn region_benchmark(mut self, composite: f64) -> Self {
        self.inner
            .region_benchmark
            .push(row(&[("composite_benchmark", v_f(composite))]));
        self
    }

    fn cascade_seishain(mut self, salary: f64, holidays: f64) -> Self {
        self.inner.cascade.push(row(&[
            ("emp_group", v_s("正社員")),
            ("avg_salary_min", v_f(salary)),
            ("avg_annual_holidays", v_f(holidays)),
        ]));
        self
    }

    fn ext_population(mut self, total_pop: f64) -> Self {
        self.inner
            .ext_population
            .push(row(&[("total_population", v_f(total_pop))]));
        self
    }

    fn ext_labor_force(
        mut self,
        unemployment_rate: f64,
        unemployed: f64,
        employed: f64,
        tertiary: f64,
        primary: f64,
    ) -> Self {
        self.inner.ext_labor_force.push(row(&[
            ("unemployment_rate", v_f(unemployment_rate)),
            ("unemployed", v_f(unemployed)),
            ("employed", v_f(employed)),
            ("tertiary_industry_employed", v_f(tertiary)),
            ("primary_industry_employed", v_f(primary)),
        ]));
        self
    }

    fn pref_unemp(mut self, v: f64) -> Self {
        self.inner.pref_avg_unemployment_rate = Some(v);
        self
    }

    fn pref_single(mut self, v: f64) -> Self {
        self.inner.pref_avg_single_rate = Some(v);
        self
    }

    fn households(mut self, single_rate: f64, single_count: f64) -> Self {
        self.inner.ext_households.push(row(&[
            ("single_rate", v_f(single_rate)),
            ("single_households", v_f(single_count)),
        ]));
        self
    }

    fn medical_welfare(mut self, physicians: f64) -> Self {
        self.inner
            .ext_medical_welfare
            .push(row(&[("physicians", v_f(physicians))]));
        self
    }

    fn establishments(mut self, industry: &str, count: f64) -> Self {
        self.inner.ext_establishments.push(row(&[
            ("industry", v_s(industry)),
            ("establishment_count", v_f(count)),
        ]));
        self
    }

    fn geography(mut self, habitable_km2: f64) -> Self {
        self.inner
            .ext_geography
            .push(row(&[("habitable_area_km2", v_f(habitable_km2))]));
        self
    }

    fn commute_zone(
        mut self,
        count: usize,
        pref_count: usize,
        total_pop: i64,
        working_age: i64,
        elderly: i64,
    ) -> Self {
        self.inner.commute_zone_count = count;
        self.inner.commute_zone_pref_count = pref_count;
        self.inner.commute_zone_total_pop = total_pop;
        self.inner.commute_zone_working_age = working_age;
        self.inner.commute_zone_elderly = elderly;
        self
    }

    fn commute_flow(
        mut self,
        inflow: i64,
        outflow: i64,
        self_rate: f64,
        top3: Vec<(String, String, i64)>,
    ) -> Self {
        self.inner.commute_inflow_total = inflow;
        self.inner.commute_outflow_total = outflow;
        self.inner.commute_self_rate = self_rate;
        self.inner.commute_inflow_top3 = top3;
        self
    }

    #[allow(dead_code)]
    fn flow(mut self, f: FlowIndicators) -> Self {
        self.inner.flow = Some(f);
        self
    }
}

fn mock_flow() -> FlowIndicators {
    FlowIndicators {
        midnight_ratio: None,
        holiday_day_ratio: None,
        daynight_ratio: None,
        day_night_diff_ratio: None,
        covid_recovery_ratio: None,
        monthly_amplitude: None,
        diff_region_inflow_ratio: None,
        inflow_breakdown: vec![],
        monthly_trend: vec![],
        citycode: 13101,
        year: 2019,
    }
}

/// 所有権版: temporary value borrow 問題回避用
fn find_owned(insights: Vec<Insight>, id: &str) -> Option<Insight> {
    insights.into_iter().find(|i| i.id == id)
}

/// generate_insights + find をワンショットで実行
fn gen_find(ctx: &InsightContext, id: &str) -> Option<Insight> {
    find_owned(generate_insights(ctx), id)
}

fn flow_find(ctx: &InsightContext, f: &FlowIndicators, id: &str) -> Option<Insight> {
    find_owned(analyze_flow_insights(ctx, f), id)
}

// ========================================================================
// HS-1: 慢性的人材不足
// ========================================================================

#[test]
fn hs1_no_fire_below_warning() {
    let ctx = Ctx::new()
        .vacancy_seishain(VACANCY_WARNING - 0.01, 100.0)
        .build();
    assert!(gen_find(&ctx, "HS-1").is_none());
}

#[test]
fn hs1_warning_at_threshold() {
    let ctx = Ctx::new().vacancy_seishain(0.25, 100.0).build();
    let i = gen_find(&ctx, "HS-1").expect("HS-1 should fire at 0.25");
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn hs1_critical_chronic() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.35, 200.0)
        .ts_vacancy_seishain(&[0.26, 0.28, 0.32])
        .build();
    let i = gen_find(&ctx, "HS-1").unwrap();
    assert_eq!(
        i.severity,
        Severity::Critical,
        "chronic + >=0.30 => Critical"
    );
}

#[test]
fn hs1_warning_critical_not_chronic() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.35, 200.0)
        .ts_vacancy_seishain(&[0.20, 0.22, 0.20])
        .build();
    let i = gen_find(&ctx, "HS-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

// ========================================================================
// HS-2: 給与競争力
// ========================================================================

#[test]
fn hs2_no_fire_above_warning() {
    let ctx = Ctx::new()
        .salary_comp_seishain(SALARY_COMP_WARNING, 250_000.0, 270_000.0, 265_000.0)
        .build();
    assert!(gen_find(&ctx, "HS-2").is_none());
}

#[test]
fn hs2_warning_at_0_85() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.85, 230_000.0, 270_000.0, 265_000.0)
        .build();
    let i = gen_find(&ctx, "HS-2").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn hs2_critical_below_0_80() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.70, 180_000.0, 270_000.0, 265_000.0)
        .build();
    let i = gen_find(&ctx, "HS-2").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

#[test]
fn hs2_no_fire_on_zero_comp() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.0, 0.0, 270_000.0, 265_000.0)
        .build();
    assert!(gen_find(&ctx, "HS-2").is_none());
}

// ========================================================================
// HS-3: 情報開示不足
// ========================================================================

#[test]
fn hs3_no_fire_at_threshold() {
    let ctx = Ctx::new()
        .transparency(TRANSPARENCY_WARNING, "disclosure_overtime", 0.3)
        .build();
    assert!(gen_find(&ctx, "HS-3").is_none());
}

#[test]
fn hs3_warning_below_warning_threshold() {
    let ctx = Ctx::new()
        .transparency(0.45, "disclosure_overtime", 0.2)
        .build();
    let i = gen_find(&ctx, "HS-3").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.body.contains("残業時間"));
}

#[test]
fn hs3_critical_below_critical_threshold() {
    let ctx = Ctx::new()
        .transparency(0.30, "disclosure_female_ratio", 0.1)
        .build();
    let i = gen_find(&ctx, "HS-3").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

// ========================================================================
// HS-4: テキスト温度
// 注意: get_f64 で 0.0 フォールバック → temperature 未設定では発火不能
// ========================================================================

#[test]
fn hs4_no_fire_when_vacancy_low() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.25, 100.0)
        .temperature(-0.5, 0.1)
        .build();
    assert!(gen_find(&ctx, "HS-4").is_none());
}

#[test]
fn hs4_no_fire_when_temp_zero() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.40, 100.0)
        .temperature(0.0, 0.0)
        .build();
    assert!(gen_find(&ctx, "HS-4").is_none());
}

#[test]
fn hs4_warning_high_vacancy_negative_temp() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.40, 100.0)
        .temperature(-0.2, 0.05)
        .build();
    let i = gen_find(&ctx, "HS-4").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

// ========================================================================
// HS-5: 雇用者集中
// ========================================================================

#[test]
fn hs5_no_fire_below_all_thresholds() {
    let ctx = Ctx::new().monopsony(0.10, 0.20).build();
    assert!(gen_find(&ctx, "HS-5").is_none());
}

#[test]
fn hs5_critical_on_hhi_only() {
    let ctx = Ctx::new().monopsony(0.30, 0.10).build();
    let i = gen_find(&ctx, "HS-5").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

#[test]
fn hs5_warning_on_top1_only() {
    let ctx = Ctx::new().monopsony(0.10, 0.35).build();
    let i = gen_find(&ctx, "HS-5").unwrap();
    assert_eq!(
        i.severity,
        Severity::Warning,
        "top1-only trigger should be Warning (hhi<CRITICAL)"
    );
}

// ========================================================================
// HS-6: 空間ミスマッチ
// ========================================================================

#[test]
fn hs6_no_fire_below_threshold() {
    let ctx = Ctx::new().spatial_mismatch(0.45, 250_000.0).build();
    assert!(gen_find(&ctx, "HS-6").is_none());
}

#[test]
fn hs6_warning_and_bedtown_cross() {
    let ctx = Ctx::new()
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(0.85)
        .build();
    let i = gen_find(&ctx, "HS-6").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.body.contains("ベッドタウン"));
}

#[test]
fn hs6_warning_no_daytime_data() {
    let ctx = Ctx::new().spatial_mismatch(0.60, 250_000.0).build();
    let i = gen_find(&ctx, "HS-6").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(!i.body.contains("ベッドタウン"));
}

// ========================================================================
// FC-1: 求人量トレンド
// ========================================================================

#[test]
fn fc1_no_fire_below_3_samples() {
    let ctx = Ctx::new().ts_counts_seishain(&[100.0, 110.0]).build();
    assert!(gen_find(&ctx, "FC-1").is_none());
}

#[test]
fn fc1_warning_on_decreasing() {
    let ctx = Ctx::new().ts_counts_seishain(&[100.0, 80.0, 60.0]).build();
    let i = gen_find(&ctx, "FC-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.body.contains("減少"));
}

#[test]
fn fc1_info_on_increasing() {
    let ctx = Ctx::new()
        .ts_counts_seishain(&[100.0, 120.0, 150.0, 180.0])
        .build();
    let i = gen_find(&ctx, "FC-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
    assert!(i.body.contains("増加"));
}

#[test]
fn fc1_positive_on_stable() {
    let ctx = Ctx::new()
        .ts_counts_seishain(&[100.0, 100.0, 101.0, 100.0])
        .build();
    let i = gen_find(&ctx, "FC-1").unwrap();
    assert_eq!(
        i.severity,
        Severity::Positive,
        "stable trend is labeled Positive (counterintuitive)"
    );
}

// ========================================================================
// FC-2: 給与上昇圧力
// ========================================================================

#[test]
fn fc2_no_fire_insufficient_samples() {
    let ctx = Ctx::new()
        .ts_salary_seishain(&[250_000.0, 260_000.0])
        .build();
    assert!(gen_find(&ctx, "FC-2").is_none());
}

#[test]
fn fc2_positive_when_salary_outpaces_wage() {
    let ctx = Ctx::new()
        .ts_salary_seishain(&[200_000.0, 210_000.0, 230_000.0, 250_000.0])
        .ext_min_wage(&[1000.0, 1001.0, 1002.0, 1003.0])
        .build();
    let i = gen_find(&ctx, "FC-2").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

#[test]
fn fc2_warning_when_salary_lags() {
    let ctx = Ctx::new()
        .ts_salary_seishain(&[250_000.0, 250_100.0, 250_200.0])
        .ext_min_wage(&[900.0, 990.0, 1089.0])
        .build();
    let i = gen_find(&ctx, "FC-2").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

// ========================================================================
// FC-3: 人口動態による労働力予測
// ========================================================================

#[test]
fn fc3_no_fire_empty_pyramid() {
    let ctx = Ctx::new().build();
    assert!(gen_find(&ctx, "FC-3").is_none());
}

#[test]
fn fc3_info_low_decline() {
    let ctx = Ctx::new()
        .pyramid("20-24", 100.0, 100.0)
        .pyramid("25-29", 100.0, 100.0)
        .pyramid("30-34", 100.0, 100.0)
        .pyramid("35-39", 100.0, 100.0)
        .pyramid("40-44", 50.0, 50.0)
        .pyramid("55-59", 40.0, 35.0)
        .pyramid("60-64", 40.0, 35.0)
        .migration(100.0, 80.0)
        .build();
    let i = gen_find(&ctx, "FC-3").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn fc3_critical_high_decline_with_outmigration() {
    let ctx = Ctx::new()
        .pyramid("20-24", 20.0, 20.0)
        .pyramid("30-34", 30.0, 30.0)
        .pyramid("40-44", 30.0, 30.0)
        .pyramid("55-59", 50.0, 50.0)
        .pyramid("60-64", 40.0, 40.0)
        .migration(50.0, 100.0)
        .build();
    let i = gen_find(&ctx, "FC-3").unwrap();
    assert_eq!(i.severity, Severity::Critical);
    assert!(i.body.contains("転出超過"));
}

// ========================================================================
// FC-4: 充足困難度の悪化予兆
// ========================================================================

#[test]
fn fc4_no_fire_all_improving() {
    let ctx = Ctx::new()
        .ts_fulfillment_seishain(&[60.0, 55.0, 50.0])
        .ts_tracking_seishain(&[0.20, 0.18, 0.15])
        .build();
    assert!(gen_find(&ctx, "FC-4").is_none());
}

#[test]
fn fc4_info_on_mild_worsening() {
    let ctx = Ctx::new()
        .ts_fulfillment_seishain(&[50.0, 51.0, 52.0])
        .ts_tracking_seishain(&[0.20, 0.19, 0.18])
        .build();
    let i = gen_find(&ctx, "FC-4").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn fc4_warning_on_dual_worsening() {
    let ctx = Ctx::new()
        .ts_fulfillment_seishain(&[30.0, 40.0, 50.0, 60.0])
        .ts_tracking_seishain(&[0.10, 0.13, 0.17, 0.22])
        .build();
    let i = gen_find(&ctx, "FC-4").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

// ========================================================================
// RC-1: 総合ベンチマーク
// ========================================================================

#[test]
fn rc1_no_fire_on_zero_composite() {
    let ctx = Ctx::new().region_benchmark(0.0).build();
    assert!(gen_find(&ctx, "RC-1").is_none());
}

#[test]
fn rc1_warning_below_30() {
    let ctx = Ctx::new().region_benchmark(20.0).build();
    let i = gen_find(&ctx, "RC-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn rc1_positive_above_70() {
    let ctx = Ctx::new().region_benchmark(85.0).build();
    let i = gen_find(&ctx, "RC-1").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

#[test]
fn rc1_info_mid() {
    let ctx = Ctx::new().region_benchmark(50.0).build();
    let i = gen_find(&ctx, "RC-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

// ========================================================================
// RC-2: 給与地域差
// ========================================================================

#[test]
fn rc2_no_fire_when_national_missing() {
    let ctx = Ctx::new().cascade_seishain(250_000.0, 120.0).build();
    assert!(gen_find(&ctx, "RC-2").is_none());
}

#[test]
fn rc2_warning_large_negative() {
    let ctx = Ctx::new()
        .cascade_seishain(240_000.0, 120.0)
        .salary_comp_seishain(0.85, 240_000.0, 280_000.0, 275_000.0)
        .build();
    let i = gen_find(&ctx, "RC-2").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn rc2_positive_large_positive() {
    let ctx = Ctx::new()
        .cascade_seishain(290_000.0, 120.0)
        .salary_comp_seishain(1.10, 290_000.0, 270_000.0, 270_000.0)
        .build();
    let i = gen_find(&ctx, "RC-2").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

// ========================================================================
// RC-3: 人口×求人密度クロス
// ========================================================================

#[test]
fn rc3_no_fire_small_population() {
    let ctx = Ctx::new()
        .ext_population(50.0)
        .vacancy_seishain(0.25, 20.0)
        .build();
    assert!(gen_find(&ctx, "RC-3").is_none());
}

#[test]
fn rc3_warning_high_density() {
    let ctx = Ctx::new()
        .ext_population(1000.0)
        .vacancy_seishain(0.25, 100.0)
        .build();
    let i = gen_find(&ctx, "RC-3").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn rc3_positive_low_density_with_cross_ref_to_ge1() {
    let ctx = Ctx::new()
        .ext_population(1_000_000.0)
        .vacancy_seishain(0.25, 100.0)
        .build();
    let i = gen_find(&ctx, "RC-3").unwrap();
    assert_eq!(i.severity, Severity::Positive);
    assert!(
        i.body.contains("可住地") && i.body.contains("構造分析"),
        "Positive RC-3 must cross-reference GE-1 (habitable density) caveat"
    );
}

#[test]
fn rc3_info_mid_density() {
    let ctx = Ctx::new()
        .ext_population(10_000.0)
        .vacancy_seishain(0.25, 200.0)
        .build();
    let i = gen_find(&ctx, "RC-3").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

// ========================================================================
// AP-1: 給与改善提案
// ========================================================================

#[test]
fn ap1_no_fire_without_hs2() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.95, 265_000.0, 280_000.0, 290_000.0)
        .build();
    assert!(gen_find(&ctx, "AP-1").is_none());
}

#[test]
fn ap1_fires_when_hs2_fires() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.70, 180_000.0, 270_000.0, 265_000.0)
        .build();
    let out = generate_insights(&ctx);
    assert!(out.iter().any(|i| i.id == "HS-2"));
    let ap = out
        .into_iter()
        .find(|i| i.id == "AP-1")
        .expect("AP-1 should fire with HS-2");
    assert_eq!(ap.severity, Severity::Info);
}

// ========================================================================
// AP-2: 求人原稿改善
// ========================================================================

#[test]
fn ap2_fires_when_hs3_fires() {
    let ctx = Ctx::new()
        .transparency(0.30, "disclosure_female_ratio", 0.05)
        .build();
    let out = generate_insights(&ctx);
    assert!(out.iter().any(|i| i.id == "HS-3"));
    let ap = out
        .into_iter()
        .find(|i| i.id == "AP-2")
        .expect("AP-2 should fire when HS-3 fires and missing items");
    assert_eq!(ap.severity, Severity::Info);
    assert!(ap.body.contains("女性比率"));
}

// ========================================================================
// AP-3: エリア拡大
// ========================================================================

#[test]
fn ap3_fires_bedtown() {
    let ctx = Ctx::new()
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(0.80)
        .build();
    let out = generate_insights(&ctx);
    assert!(out.iter().any(|i| i.id == "HS-6"));
    let ap = out
        .into_iter()
        .find(|i| i.id == "AP-3")
        .expect("AP-3 should fire");
    assert_eq!(ap.severity, Severity::Info);
}

#[test]
fn ap3_no_fire_in_urban_core() {
    let ctx = Ctx::new()
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(1.20)
        .build();
    assert!(gen_find(&ctx, "AP-3").is_none());
}

// ========================================================================
// CZ-1: 通勤圏人口ポテンシャル
// ========================================================================

#[test]
fn cz1_info_when_local_share_high() {
    let ctx = Ctx::new()
        .pyramid("20-24", 500.0, 500.0)
        .commute_zone(5, 1, 10_000, 7_000, 2_000)
        .build();
    let i = gen_find(&ctx, "CZ-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn cz1_positive_when_local_small_fraction() {
    let ctx = Ctx::new()
        .pyramid("20-24", 100.0, 100.0)
        .commute_zone(20, 3, 100_000, 70_000, 20_000)
        .build();
    let i = gen_find(&ctx, "CZ-1").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

// ========================================================================
// CZ-2: 通勤圏給与格差
// ========================================================================

#[test]
fn cz2_warning_large_local_discount() {
    let ctx = Ctx::new()
        .cascade_seishain(220_000.0, 120.0)
        .spatial_mismatch(0.30, 270_000.0)
        .commute_zone(1, 1, 100, 80, 20)
        .build();
    let i = gen_find(&ctx, "CZ-2").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn cz2_positive_local_premium() {
    let ctx = Ctx::new()
        .cascade_seishain(290_000.0, 120.0)
        .spatial_mismatch(0.30, 250_000.0)
        .commute_zone(1, 1, 100, 80, 20)
        .build();
    let i = gen_find(&ctx, "CZ-2").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

// ========================================================================
// CZ-3: 通勤圏高齢化
// ========================================================================

#[test]
fn cz3_no_fire_low_elderly() {
    let ctx = Ctx::new().commute_zone(5, 1, 10_000, 7_000, 1_500).build();
    assert!(gen_find(&ctx, "CZ-3").is_none());
}

#[test]
fn cz3_info_moderate_aging() {
    let ctx = Ctx::new().commute_zone(5, 1, 10_000, 7_000, 2_500).build();
    let i = gen_find(&ctx, "CZ-3").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn cz3_warning_severe_aging() {
    let ctx = Ctx::new().commute_zone(5, 1, 10_000, 5_000, 3_500).build();
    let i = gen_find(&ctx, "CZ-3").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

// ========================================================================
// CF-1: 実通勤圏発見
// ========================================================================

#[test]
fn cf1_no_fire_zero_inflow() {
    let ctx = Ctx::new().commute_zone(5, 1, 10_000, 7_000, 2_000).build();
    assert!(gen_find(&ctx, "CF-1").is_none());
}

#[test]
fn cf1_warning_very_low_actual_ratio() {
    let ctx = Ctx::new()
        .commute_zone(5, 1, 100_000, 70_000, 20_000)
        .commute_flow(50, 30, 0.5, vec![("千葉県".into(), "船橋市".into(), 20)])
        .build();
    let i = gen_find(&ctx, "CF-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn cf1_info_typical_ratio() {
    let ctx = Ctx::new()
        .commute_zone(5, 1, 10_000, 7_000, 2_000)
        .commute_flow(500, 300, 0.5, vec![("千葉県".into(), "船橋市".into(), 200)])
        .build();
    let i = gen_find(&ctx, "CF-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

// ========================================================================
// CF-2: 流入元ターゲティング
// ========================================================================

#[test]
fn cf2_no_fire_empty_top3() {
    let ctx = Ctx::new()
        .commute_zone(5, 1, 10_000, 7_000, 2_000)
        .commute_flow(500, 0, 0.5, vec![])
        .build();
    assert!(gen_find(&ctx, "CF-2").is_none());
}

#[test]
fn cf2_info_cross_pref() {
    let ctx = Ctx::new()
        .commute_zone(5, 1, 10_000, 7_000, 2_000)
        .commute_flow(
            500,
            300,
            0.5,
            vec![("神奈川県".into(), "横浜市".into(), 200)],
        )
        .build();
    let i = gen_find(&ctx, "CF-2").unwrap();
    assert_eq!(i.severity, Severity::Info);
    assert!(i.body.contains("都道府県をまたぐ"));
}

// ========================================================================
// CF-3: 地元就業率
// ========================================================================

#[test]
fn cf3_no_fire_zero_rate() {
    let ctx = Ctx::new()
        .commute_flow(100, 100, 0.0, vec![("千葉県".into(), "船橋市".into(), 50)])
        .build();
    assert!(gen_find(&ctx, "CF-3").is_none());
}

#[test]
fn cf3_warning_low_self() {
    let ctx = Ctx::new()
        .commute_flow(
            1000,
            1000,
            0.25,
            vec![("神奈川県".into(), "川崎市".into(), 500)],
        )
        .build();
    let i = gen_find(&ctx, "CF-3").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn cf3_positive_high_self() {
    let ctx = Ctx::new()
        .commute_flow(
            1000,
            100,
            0.85,
            vec![("東京都".into(), "新宿区".into(), 500)],
        )
        .build();
    let i = gen_find(&ctx, "CF-3").unwrap();
    assert_eq!(i.severity, Severity::Positive);
}

// ========================================================================
// LS-1: 採用余力シグナル
// ========================================================================

#[test]
fn ls1_no_fire_below_warning_ratio() {
    let ctx = Ctx::new()
        .ext_labor_force(2.5, 100.0, 3900.0, 3500.0, 100.0)
        .pref_unemp(2.2)
        .build();
    assert!(gen_find(&ctx, "LS-1").is_none());
}

#[test]
fn ls1_warning_at_1_3_ratio() {
    let ctx = Ctx::new()
        .ext_labor_force(3.0, 200.0, 6500.0, 5500.0, 200.0)
        .pref_unemp(2.2)
        .build();
    let i = gen_find(&ctx, "LS-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn ls1_critical_at_1_6_ratio() {
    let ctx = Ctx::new()
        .ext_labor_force(4.0, 400.0, 9600.0, 8000.0, 300.0)
        .pref_unemp(2.5)
        .build();
    let i = gen_find(&ctx, "LS-1").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

#[test]
fn ls1_no_fire_without_pref_avg() {
    let ctx = Ctx::new()
        .ext_labor_force(4.0, 400.0, 9600.0, 8000.0, 300.0)
        .build();
    assert!(gen_find(&ctx, "LS-1").is_none());
}

// ========================================================================
// LS-2: 産業偏在
// ========================================================================

#[test]
fn ls2_no_fire_balanced() {
    let ctx = Ctx::new()
        .ext_labor_force(2.0, 100.0, 10_000.0, 7_000.0, 1_000.0)
        .build();
    assert!(gen_find(&ctx, "LS-2").is_none());
}

#[test]
fn ls2_info_tertiary_just_over() {
    let ctx = Ctx::new()
        .ext_labor_force(2.0, 100.0, 10_000.0, 8_700.0, 500.0)
        .build();
    let i = gen_find(&ctx, "LS-2").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn ls2_warning_tertiary_extreme() {
    let ctx = Ctx::new()
        .ext_labor_force(2.0, 100.0, 10_000.0, 9_700.0, 100.0)
        .build();
    let i = gen_find(&ctx, "LS-2").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn ls2_info_primary_heavy() {
    let ctx = Ctx::new()
        .ext_labor_force(3.0, 100.0, 10_000.0, 5_000.0, 2_500.0)
        .build();
    let i = gen_find(&ctx, "LS-2").unwrap();
    assert_eq!(i.severity, Severity::Info);
    assert!(i.body.contains("第一次産業"));
}

// ========================================================================
// HH-1: 単独世帯
// ========================================================================

#[test]
fn hh1_no_fire_below_40() {
    let ctx = Ctx::new().households(35.0, 1000.0).build();
    assert!(gen_find(&ctx, "HH-1").is_none());
}

#[test]
fn hh1_info_moderate_single() {
    let ctx = Ctx::new().households(45.0, 1500.0).build();
    let i = gen_find(&ctx, "HH-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn hh1_warning_very_high_single() {
    let ctx = Ctx::new()
        .households(60.0, 2500.0)
        .pref_single(45.0)
        .build();
    let i = gen_find(&ctx, "HH-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.body.contains("県平均"));
}

// ========================================================================
// MF-1: 医療供給密度
// ========================================================================

#[test]
fn mf1_no_fire_sufficient_physicians() {
    let ctx = Ctx::new()
        .ext_population(100_000.0)
        .medical_welfare(300.0)
        .build();
    assert!(gen_find(&ctx, "MF-1").is_none());
}

#[test]
fn mf1_warning_moderate_shortage() {
    let ctx = Ctx::new()
        .ext_population(100_000.0)
        .medical_welfare(200.0)
        .build();
    let i = gen_find(&ctx, "MF-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn mf1_critical_severe_shortage() {
    let ctx = Ctx::new()
        .ext_population(100_000.0)
        .medical_welfare(100.0)
        .build();
    let i = gen_find(&ctx, "MF-1").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

// ========================================================================
// IN-1: 産業構造ミスマッチ（現実装の仕様を検証）
// ========================================================================

#[test]
fn in1_no_fire_mw_in_normal_range() {
    let ctx = Ctx::new()
        .establishments("850", 200.0)
        .establishments("800", 800.0)
        .build();
    assert!(gen_find(&ctx, "IN-1").is_none());
}

#[test]
fn in1_info_mw_extremely_low() {
    let ctx = Ctx::new()
        .establishments("850", 20.0)
        .establishments("800", 980.0)
        .build();
    let i = gen_find(&ctx, "IN-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn in1_info_mw_extremely_high() {
    let ctx = Ctx::new()
        .establishments("850", 500.0)
        .establishments("800", 500.0)
        .build();
    let i = gen_find(&ctx, "IN-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn in1_no_fire_without_medical_welfare_code() {
    let ctx = Ctx::new().establishments("800", 1000.0).build();
    assert!(gen_find(&ctx, "IN-1").is_none());
}

// ========================================================================
// GE-1: 可住地密度（2026-04-23 緩和済）
// ========================================================================

#[test]
fn ge1_no_fire_normal_density() {
    let ctx = Ctx::new().ext_population(50_000.0).geography(10.0).build();
    assert!(gen_find(&ctx, "GE-1").is_none());
}

#[test]
fn ge1_warning_extreme_urban() {
    let ctx = Ctx::new().ext_population(250_000.0).geography(10.0).build();
    let i = gen_find(&ctx, "GE-1").unwrap();
    assert_eq!(i.severity, Severity::Warning);
    assert!(i.body.contains("過密"));
}

#[test]
fn ge1_info_extreme_sparse_2026_04_23() {
    let ctx = Ctx::new().ext_population(100.0).geography(10.0).build();
    let i = gen_find(&ctx, "GE-1").unwrap();
    assert_eq!(
        i.severity,
        Severity::Info,
        "2026-04-23: extreme sparse should be Info (not Warning)"
    );
    assert!(i.body.contains("極端な過疎"));
}

#[test]
fn ge1_info_mild_urban() {
    let ctx = Ctx::new().ext_population(150_000.0).geography(10.0).build();
    let i = gen_find(&ctx, "GE-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
    assert!(i.body.contains("過密傾向"));
}

#[test]
fn ge1_info_mild_sparse() {
    let ctx = Ctx::new().ext_population(300.0).geography(10.0).build();
    let i = gen_find(&ctx, "GE-1").unwrap();
    assert_eq!(i.severity, Severity::Info);
    assert!(i.body.contains("過疎傾向"));
}

// ========================================================================
// Flow パターン
// ========================================================================

#[test]
fn swf01_no_fire_below_1_2() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.midnight_ratio = Some(1.1);
    assert!(flow_find(&ctx, &f, "SW-F01").is_none());
}

#[test]
fn swf01_warning_at_1_3() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.midnight_ratio = Some(1.3);
    let i = flow_find(&ctx, &f, "SW-F01").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn swf01_critical_at_1_6() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.midnight_ratio = Some(1.6);
    let i = flow_find(&ctx, &f, "SW-F01").unwrap();
    assert_eq!(i.severity, Severity::Critical);
}

#[test]
fn swf02_no_fire_below_threshold() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.holiday_day_ratio = Some(1.2);
    assert!(flow_find(&ctx, &f, "SW-F02").is_none());
}

#[test]
fn swf02_warning_at_1_4() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.holiday_day_ratio = Some(1.4);
    let i = flow_find(&ctx, &f, "SW-F02").unwrap();
    assert_eq!(i.severity, Severity::Warning);
}

#[test]
fn swf03_no_fire_balanced() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.daynight_ratio = Some(0.85);
    assert!(flow_find(&ctx, &f, "SW-F03").is_none());
}

#[test]
fn swf03_info_typical_bedtown() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.daynight_ratio = Some(0.7);
    let i = flow_find(&ctx, &f, "SW-F03").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn swf04_always_none_placeholder() {
    let ctx = Ctx::new().vacancy_seishain(0.25, 100.0).build();
    let mut f = mock_flow();
    f.daynight_ratio = Some(1.5);
    assert!(flow_find(&ctx, &f, "SW-F04").is_none());
}

#[test]
fn swf05_no_fire_below_1_5() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.holiday_day_ratio = Some(1.4);
    assert!(flow_find(&ctx, &f, "SW-F05").is_none());
}

#[test]
fn swf05_info_at_1_6_excludes_f02() {
    // M-2 修正 (2026-04-26): 1.5 以上では SW-F05 のみ発火、SW-F02 は抑制 (矛盾排他)
    // 修正前: ratio=1.6 で F02/F05 両方発火 (人材不足 vs 機会あり が矛盾)
    // 修正後: ratio=1.6 で F05 のみ発火 (観光ポテンシャル領域)
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.holiday_day_ratio = Some(1.6);
    let out = analyze_flow_insights(&ctx, &f);
    let f05 = out.iter().find(|i| i.id == "SW-F05").cloned().unwrap();
    assert_eq!(f05.severity, Severity::Info);
    assert!(
        !out.iter().any(|i| i.id == "SW-F02"),
        "M-2 mutual exclusion: SW-F05 fires (>=1.5) implies SW-F02 must NOT fire"
    );
}

#[test]
fn swf06_no_fire_below_0_9() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.covid_recovery_ratio = Some(0.85);
    assert!(flow_find(&ctx, &f, "SW-F06").is_none());
}

#[test]
fn swf06_info_at_full_recovery() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.covid_recovery_ratio = Some(1.0);
    let i = flow_find(&ctx, &f, "SW-F06").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn swf07_no_fire_below_0_15() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.diff_region_inflow_ratio = Some(0.10);
    assert!(flow_find(&ctx, &f, "SW-F07").is_none());
}

#[test]
fn swf07_info_at_0_20() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.diff_region_inflow_ratio = Some(0.20);
    let i = flow_find(&ctx, &f, "SW-F07").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn swf08_no_fire_below_1_3() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.daynight_ratio = Some(1.2);
    assert!(flow_find(&ctx, &f, "SW-F08").is_none());
}

#[test]
fn swf08_info_office_district() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.daynight_ratio = Some(1.8);
    let i = flow_find(&ctx, &f, "SW-F08").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn swf09_no_fire_below_0_3() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.monthly_amplitude = Some(0.2);
    assert!(flow_find(&ctx, &f, "SW-F09").is_none());
}

#[test]
fn swf09_info_at_0_4() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.monthly_amplitude = Some(0.4);
    let i = flow_find(&ctx, &f, "SW-F09").unwrap();
    assert_eq!(i.severity, Severity::Info);
}

#[test]
fn swf10_always_none_phase_c_pending() {
    let ctx = Ctx::new().build();
    let f = mock_flow();
    assert!(flow_find(&ctx, &f, "SW-F10").is_none());
}

// ========================================================================
// Cross-pattern interaction
// ========================================================================

#[test]
fn cross_rc3_positive_with_ge1_info_has_reference() {
    // RC-3: postings 50 * 1000 / 1_000_000 = 0.05 件/千人 < 5 → Positive
    // GE-1: density 1_000_000 / 100_000 = 10 < 20 → 極端過疎 Info
    let ctx = Ctx::new()
        .ext_population(1_000_000.0)
        .vacancy_seishain(0.20, 50.0)
        .geography(100_000.0)
        .build();
    let out = generate_insights(&ctx);
    let rc3 = out
        .iter()
        .find(|i| i.id == "RC-3")
        .expect("RC-3 should fire")
        .clone();
    let ge1 = out
        .iter()
        .find(|i| i.id == "GE-1")
        .expect("GE-1 should fire")
        .clone();
    assert_eq!(rc3.severity, Severity::Positive);
    assert_eq!(ge1.severity, Severity::Info);
    assert!(
        rc3.body.contains("可住地") && rc3.body.contains("構造分析"),
        "RC-3 Positive body must cross-reference GE-1 (habitable density)"
    );
}

#[test]
fn cross_hs2_triggers_ap1() {
    let ctx = Ctx::new()
        .salary_comp_seishain(0.70, 180_000.0, 270_000.0, 265_000.0)
        .build();
    let out = generate_insights(&ctx);
    let hs2 = out.iter().find(|i| i.id == "HS-2").unwrap().clone();
    let ap1 = out.iter().find(|i| i.id == "AP-1").unwrap().clone();
    assert_eq!(hs2.severity, Severity::Critical);
    assert_eq!(ap1.severity, Severity::Info);
}

#[test]
fn cross_hs6_ap3_require_bedtown() {
    let ctx_bedtown = Ctx::new()
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(0.80)
        .build();
    let out_b = generate_insights(&ctx_bedtown);
    assert!(out_b.iter().any(|i| i.id == "HS-6"));
    assert!(out_b.iter().any(|i| i.id == "AP-3"));

    let ctx_urban = Ctx::new()
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(1.20)
        .build();
    let out_u = generate_insights(&ctx_urban);
    assert!(out_u.iter().any(|i| i.id == "HS-6"));
    assert!(
        !out_u.iter().any(|i| i.id == "AP-3"),
        "AP-3 must skip in urban core even when HS-6 fires"
    );
}

#[test]
fn cross_cz2_warning_with_hs2() {
    let ctx = Ctx::new()
        .cascade_seishain(220_000.0, 120.0)
        .spatial_mismatch(0.30, 270_000.0)
        .salary_comp_seishain(0.78, 220_000.0, 280_000.0, 275_000.0)
        .commute_zone(1, 1, 100, 80, 20)
        .build();
    let out = generate_insights(&ctx);
    let cz2 = out.iter().find(|i| i.id == "CZ-2").unwrap().clone();
    let hs2 = out.iter().find(|i| i.id == "HS-2").unwrap().clone();
    assert_eq!(cz2.severity, Severity::Warning);
    assert_eq!(hs2.severity, Severity::Critical);
    assert!(cz2.body.contains("地元月給"));
}

#[test]
fn cross_fc1_decline_cz3_aging_consistent() {
    let ctx = Ctx::new()
        .ts_counts_seishain(&[200.0, 150.0, 100.0, 70.0])
        .commute_zone(5, 1, 10_000, 5_500, 3_500)
        .build();
    let out = generate_insights(&ctx);
    assert_eq!(
        out.iter().find(|i| i.id == "FC-1").unwrap().severity,
        Severity::Warning
    );
    assert_eq!(
        out.iter().find(|i| i.id == "CZ-3").unwrap().severity,
        Severity::Warning
    );
}

#[test]
fn cross_ls1_hs1_simultaneous_mismatch() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.35, 100.0)
        .ts_vacancy_seishain(&[0.32, 0.33, 0.35])
        .ext_labor_force(4.0, 400.0, 9600.0, 8000.0, 300.0)
        .pref_unemp(2.5)
        .build();
    let out = generate_insights(&ctx);
    assert_eq!(
        out.iter().find(|i| i.id == "HS-1").unwrap().severity,
        Severity::Critical
    );
    assert_eq!(
        out.iter().find(|i| i.id == "LS-1").unwrap().severity,
        Severity::Critical
    );
}

#[test]
fn meta_cz3_category_is_forecast() {
    let ctx = Ctx::new().commute_zone(5, 1, 10_000, 5_000, 3_500).build();
    let i = gen_find(&ctx, "CZ-3").unwrap();
    assert_eq!(
        i.category,
        InsightCategory::Forecast,
        "CZ-3 is categorized as Forecast (not RegionalCompare) despite being commute-zone based"
    );
}

#[test]
fn meta_cf3_category_is_hiring_structure() {
    let ctx = Ctx::new()
        .commute_flow(
            500,
            200,
            0.85,
            vec![("東京都".into(), "新宿区".into(), 200)],
        )
        .build();
    let i = gen_find(&ctx, "CF-3").unwrap();
    assert_eq!(
        i.category,
        InsightCategory::HiringStructure,
        "CF-3 is categorized as HiringStructure (not commute)"
    );
}

#[test]
fn meta_insights_sorted_by_severity() {
    let ctx = Ctx::new()
        .vacancy_seishain(0.35, 100.0)
        .ts_vacancy_seishain(&[0.32, 0.33, 0.35])
        .ts_counts_seishain(&[100.0, 100.0, 100.0])
        .region_benchmark(50.0)
        .build();
    let out = generate_insights(&ctx);
    for pair in out.windows(2) {
        assert!(
            pair[0].severity <= pair[1].severity,
            "insights must be sorted Critical→Positive. got {:?} before {:?}",
            pair[0].severity,
            pair[1].severity
        );
    }
}

#[test]
fn meta_empty_context_no_panic_no_insights() {
    let ctx = Ctx::new().build();
    let out = generate_insights(&ctx);
    assert!(out.is_empty(), "empty ctx should yield no insights");
}

#[test]
fn anomaly_fc1_stable_labeled_positive() {
    let ctx = Ctx::new()
        .ts_counts_seishain(&[100.0, 100.0, 100.0, 100.0])
        .build();
    let i = gen_find(&ctx, "FC-1").unwrap();
    assert_eq!(i.severity, Severity::Positive);
    assert!(i.body.contains("横ばい"));
}

#[test]
fn anomaly_in1_severity_only_info_despite_comment() {
    let ctx = Ctx::new()
        .establishments("850", 350.0)
        .establishments("800", 650.0)
        .build();
    let i = gen_find(&ctx, "IN-1").unwrap();
    assert_eq!(
        i.severity,
        Severity::Info,
        "IN-1 severity is always Info regardless of mw_share magnitude (contradicts source comment)"
    );
}

#[test]
fn anomaly_hs4_null_temperature_is_treated_as_zero() {
    let ctx = Ctx::new().vacancy_seishain(0.40, 100.0).build();
    assert!(
        gen_find(&ctx, "HS-4").is_none(),
        "HS-4 cannot fire without explicitly negative temperature; null is silently treated as 0.0"
    );
}

// ============================================================================
// P2 修正の逆証明テスト群 (2026-04-26 / E2 チーム実装)
// memory `feedback_reverse_proof_tests.md` に基づき、修正前/修正後の具体値を assert する。
// ============================================================================

use super::phrase_validator::validate_insight_phrase;

/// 全 38 patterns の body が phrase_validator を通過することを保証する回帰テスト。
/// 修正前: HS-1 (「維持しています」) / HS-2 (「不足しています」) / GE-1 (「限定的なため」)
///         / SW-F06 (「100%」) などが検証 NG だった。
/// 修正後: 全 patterns の body にヘッジ語 (「傾向」「可能性」「みられ」「うかがえ」) を含む。
#[test]
fn p2_all_patterns_pass_phrase_validator() {
    // HS/FC/RC/AP/CZ/CF/LS/HH/MF/IN/GE 系を maximally に発火させる
    let ctx = Ctx::new()
        .vacancy_seishain(0.35, 200.0)
        .ts_vacancy_seishain(&[0.32, 0.33, 0.35])
        .salary_comp_seishain(0.70, 180_000.0, 270_000.0, 265_000.0)
        .transparency(0.30, "disclosure_overtime", 0.10)
        .temperature(-0.2, 0.05)
        .monopsony(0.30, 0.40)
        .spatial_mismatch(0.70, 250_000.0)
        .ext_daytime_ratio(0.85)
        .ts_counts_seishain(&[100.0, 80.0, 60.0])
        .ts_salary_seishain(&[200_000.0, 210_000.0, 215_000.0])
        .ext_min_wage(&[1000.0, 1010.0, 1020.0])
        .pyramid("55-59", 100.0, 100.0)
        .pyramid("60-64", 100.0, 100.0)
        .pyramid("30-34", 50.0, 50.0)
        .migration(50.0, 100.0)
        .ts_fulfillment_seishain(&[20.0, 30.0, 40.0])
        .ts_tracking_seishain(&[0.05, 0.07, 0.10])
        .region_benchmark(20.0)
        .cascade_seishain(220_000.0, 110.0)
        .ext_population(50_000.0)
        .ext_labor_force(4.5, 200.0, 4_500.0, 4_000.0, 100.0)
        .pref_unemp(2.5)
        .households(45.0, 8_000.0)
        .medical_welfare(5.0)
        .establishments("850", 35.0)
        .establishments("800", 65.0)
        .geography(10.0)
        .commute_zone(5, 2, 100_000, 60_000, 25_000)
        .commute_flow(
            500,
            300,
            0.85,
            vec![("東京都".into(), "新宿区".into(), 200)],
        )
        .build();

    let core = generate_insights(&ctx);

    // Flow 系も別 ctx で網羅検証
    let mut f = mock_flow();
    f.midnight_ratio = Some(1.6);
    f.holiday_day_ratio = Some(1.4); // F02 fires (1.3..1.5)
    f.daynight_ratio = Some(0.7); // F03 fires
    f.covid_recovery_ratio = Some(0.95); // F06 fires
    f.diff_region_inflow_ratio = Some(0.20);
    f.monthly_amplitude = Some(0.5);
    let flow_insights = analyze_flow_insights(&Ctx::new().build(), &f);

    // 観光地用に F05 を別途確認
    let mut f2 = mock_flow();
    f2.holiday_day_ratio = Some(1.6); // F05 fires (>=1.5)
    let flow_insights2 = analyze_flow_insights(&Ctx::new().build(), &f2);

    let mut all = core;
    all.extend(flow_insights);
    all.extend(flow_insights2);

    let mut failures: Vec<(String, String, String)> = vec![];
    for ins in &all {
        if let Err(e) = validate_insight_phrase(&ins.body) {
            failures.push((ins.id.clone(), e, ins.body.clone()));
        }
    }
    assert!(
        failures.is_empty(),
        "P2 phrase validator regression: {} patterns failed.\n{:#?}",
        failures.len(),
        failures
    );
    assert!(
        all.len() >= 20,
        "Maximally-loaded ctx must fire >= 20 patterns (got {})",
        all.len()
    );
}

/// 修正前 (engine.rs:1426): body に「未マッチ層」が含まれていた
/// 修正後: 「未マッチ層」を排除し、HW 媒体スコープの限界を明記する
#[test]
fn p2_ls1_body_excludes_unmatched_layer_terminology() {
    let ctx = Ctx::new()
        .ext_labor_force(5.0, 600.0, 10_000.0, 8_000.0, 200.0)
        .pref_unemp(3.0)
        .build();
    let i = gen_find(&ctx, "LS-1").expect("LS-1 must fire");
    assert!(
        !i.body.contains("未マッチ層"),
        "Forbidden term '未マッチ層' must be removed (P2 #7)"
    );
    assert!(
        i.body.contains("失業者数"),
        "Body must use neutral term '失業者数'"
    );
    assert!(
        i.body.contains("HW") || i.body.contains("媒体"),
        "Body must clarify HW media scope limitation"
    );
}

/// M-2 修正の逆証明: SW-F02 と SW-F05 は holiday_day_ratio=1.6 で同時発火しない
/// 修正前: ratio=1.6 で F02/F05 両方発火 (人材不足 vs 機会あり が矛盾)
/// 修正後: ratio=1.6 で F05 のみ、ratio=1.4 で F02 のみ
#[test]
fn p2_swf02_swf05_mutually_exclusive_at_high_ratio() {
    let ctx = Ctx::new().build();
    let mut f_high = mock_flow();
    f_high.holiday_day_ratio = Some(1.6);
    let out_high = analyze_flow_insights(&ctx, &f_high);
    let f02_count_high = out_high.iter().filter(|i| i.id == "SW-F02").count();
    let f05_count_high = out_high.iter().filter(|i| i.id == "SW-F05").count();
    assert_eq!(
        f02_count_high, 0,
        "M-2: F02 must NOT fire at ratio=1.6 (F05 territory)"
    );
    assert_eq!(f05_count_high, 1, "M-2: F05 fires at ratio=1.6");

    let mut f_mid = mock_flow();
    f_mid.holiday_day_ratio = Some(1.4);
    let out_mid = analyze_flow_insights(&ctx, &f_mid);
    let f02_count_mid = out_mid.iter().filter(|i| i.id == "SW-F02").count();
    let f05_count_mid = out_mid.iter().filter(|i| i.id == "SW-F05").count();
    assert_eq!(
        f02_count_mid, 1,
        "M-2: F02 fires at ratio=1.4 (intermediate)"
    );
    assert_eq!(f05_count_mid, 0, "M-2: F05 does NOT fire at ratio=1.4");
}

/// M-8 修正の逆証明: SW-F06 が ts_counts に 2019/9 vs 2021/9 のサンプルがある場合、
/// 求人側回復済 (>= 0.8) なら抑制される。
/// 修正前: 人流 recovery >= 0.9 のみで発火 (求人側未参照)
/// 修正後: 人流 >= 0.9 AND 求人 < 0.8 が必要
#[test]
fn p2_swf06_suppressed_when_posting_also_recovered() {
    // posting_count: 2019-09 = 100、2021-09 = 90 → recovery = 0.9 (>= 0.8 → 抑制)
    let mut ctx_inner = Ctx::new().build();
    ctx_inner.ts_counts.push(row(&[
        ("emp_group", v_s("正社員")),
        ("year_month", v_s("2019-09")),
        ("posting_count", v_f(100.0)),
    ]));
    ctx_inner.ts_counts.push(row(&[
        ("emp_group", v_s("正社員")),
        ("year_month", v_s("2021-09")),
        ("posting_count", v_f(90.0)),
    ]));

    let mut f = mock_flow();
    f.covid_recovery_ratio = Some(0.95);
    let out = analyze_flow_insights(&ctx_inner, &f);
    assert!(
        !out.iter().any(|i| i.id == "SW-F06"),
        "M-8: F06 must NOT fire when posting_recovery >= 0.8"
    );

    // posting_count: 2019-09 = 100、2021-09 = 70 → recovery = 0.7 (< 0.8 → 発火)
    let mut ctx_inner2 = Ctx::new().build();
    ctx_inner2.ts_counts.push(row(&[
        ("emp_group", v_s("正社員")),
        ("year_month", v_s("2019-09")),
        ("posting_count", v_f(100.0)),
    ]));
    ctx_inner2.ts_counts.push(row(&[
        ("emp_group", v_s("正社員")),
        ("year_month", v_s("2021-09")),
        ("posting_count", v_f(70.0)),
    ]));
    let out2 = analyze_flow_insights(&ctx_inner2, &f);
    let f06 = out2
        .iter()
        .find(|i| i.id == "SW-F06")
        .expect("M-8: F06 must fire when flow recovered AND posting lags");
    assert!(
        f06.body.contains("0.95倍") || f06.body.contains("0.7"),
        "F06 body must reference both ratios, got: {}",
        f06.body
    );
}

/// M-13 修正の逆証明: AP-1 年間人件費が「賞与4ヶ月+法定福利16%」を含む
/// 修正前: increase × 12
/// 修正後: increase × (12 + 4) × 1.16 ≈ increase × 18.56
#[test]
fn p2_ap1_annual_cost_includes_bonus_and_legal_welfare() {
    // increase = 270000 - 250000 = 20000
    // 修正前: 20000 × 12 = 240000
    // 修正後: 20000 × 16 × 1.16 = 371200
    let ctx = Ctx::new()
        .salary_comp_seishain(0.78, 250_000.0, 280_000.0, 270_000.0)
        .build();
    let ap1 = gen_find(&ctx, "AP-1").expect("AP-1 must fire (HS-2 prerequisite)");
    let cost_evidence = ap1
        .evidence
        .iter()
        .find(|e| e.metric == "年間コスト増")
        .expect("year cost evidence missing");
    // 期待値: 20000 × (12 + 4) × 1.16 = 371200
    let expected = 20000.0 * 16.0 * 1.16;
    assert!(
        (cost_evidence.value - expected).abs() < 1.0,
        "Annual cost mismatch. Expected {}, got {}",
        expected,
        cost_evidence.value
    );
    // 旧値 (12ヶ月単純換算) を上回ることを確認
    assert!(
        cost_evidence.value > 20000.0 * 12.0,
        "M-13: bonus/welfare補正後の年間コストが補正前を上回ること"
    );
    assert!(ap1.body.contains("賞与"), "Body must mention 賞与");
    assert!(ap1.body.contains("法定福利"), "Body must mention 法定福利");
}

/// M-10 修正の逆証明: RC-2 給与差が相対閾値 (-10%/+5%) で判定
/// 修正前: 固定 -20000円 で Warning → 高給与職種では発火不足、低給与職種では誤発火
/// 修正後: 全国平均比 -10% で Warning
#[test]
fn p2_rc2_uses_relative_threshold() {
    // 介護想定: local=230k vs national=240k → diff_pct = -4.2% (-10%閾値以上) → Info
    let ctx_low = Ctx::new()
        .cascade_seishain(230_000.0, 120.0)
        .salary_comp_seishain(0.96, 230_000.0, 240_000.0, 235_000.0)
        .build();
    let rc2_low = gen_find(&ctx_low, "RC-2").expect("RC-2 must fire");
    assert_eq!(
        rc2_low.severity,
        Severity::Info,
        "Low-salary -4.2% gap stays Info (was Info even before)"
    );

    // IT 想定: local=380k vs national=400k → diff_pct = -5.0% → Info (旧 Warning だった)
    let ctx_it_mid = Ctx::new()
        .cascade_seishain(380_000.0, 120.0)
        .salary_comp_seishain(0.95, 380_000.0, 400_000.0, 390_000.0)
        .build();
    let rc2_it_mid = gen_find(&ctx_it_mid, "RC-2").expect("RC-2 must fire");
    assert_eq!(
        rc2_it_mid.severity,
        Severity::Info,
        "M-10: IT -5% gap is Info (修正前は Warning だった誤発火)"
    );

    // IT 想定: local=350k vs national=400k → diff_pct = -12.5% → Warning
    let ctx_it_low = Ctx::new()
        .cascade_seishain(350_000.0, 120.0)
        .salary_comp_seishain(0.875, 350_000.0, 400_000.0, 390_000.0)
        .build();
    let rc2_it_low = gen_find(&ctx_it_low, "RC-2").expect("RC-2 must fire");
    assert_eq!(
        rc2_it_low.severity,
        Severity::Warning,
        "M-10: IT -12.5% gap is Warning (修正後の動的閾値)"
    );
}

/// emp_classifier 統一の逆証明: 契約社員/業務委託 が Other に分類される
/// 修正前 (survey/aggregator.rs): 契約/業務委託 → Regular
/// 修正後 (emp_classifier::classify): 契約社員/業務委託 → Other
#[test]
fn p2_emp_classifier_contract_and_gyomu_itaku_are_other() {
    use crate::handlers::emp_classifier::{classify, EmpGroup};
    assert_eq!(classify("契約社員"), EmpGroup::Other);
    assert_eq!(classify("業務委託"), EmpGroup::Other);
    assert_eq!(classify("正社員以外"), EmpGroup::Other);
    // 「正社員」「正職員」は Regular のまま
    assert_eq!(classify("正社員"), EmpGroup::Regular);
    assert_eq!(classify("正職員"), EmpGroup::Regular);
    // 「派遣」は Other
    assert_eq!(classify("派遣"), EmpGroup::Other);
}

/// emp_classifier expand_to_db_values: その他 = 4 件 (旧3件)
/// 修正前 (recruitment_diag/mod.rs:78): その他 = [正社員以外, 派遣, 契約社員] (3件)
/// 修正後: [正社員以外, 派遣, 契約社員, 業務委託] (4件)
#[test]
fn p2_emp_classifier_expand_other_includes_gyomu_itaku() {
    use crate::handlers::emp_classifier::{expand_to_db_values, EmpGroup};
    let v = expand_to_db_values(EmpGroup::Other);
    assert_eq!(v.len(), 4, "Other group must include 4 db values now");
    assert!(v.contains(&"業務委託"), "Must include 業務委託");
    assert!(v.contains(&"契約社員"));
    assert!(v.contains(&"派遣"));
    assert!(v.contains(&"正社員以外"));
}

/// GE-1 body の逆証明: 修正前 = 「限定的なため」(ヘッジなし)、修正後 = 「限定的な傾向がみられ」
/// 3 件の pre-existing test failures のうち 2 件 (GE-1 関連) を解決した修正の証跡。
#[test]
fn p2_ge1_extreme_sparse_body_has_hedge_phrase() {
    let ctx = Ctx::new().ext_population(100.0).geography(10.0).build();
    let ge1 = gen_find(&ctx, "GE-1").expect("GE-1 must fire");
    assert_eq!(ge1.severity, Severity::Info);
    assert!(
        ge1.body.contains("極端な過疎"),
        "Body must mention 極端な過疎"
    );
    // ヘッジ phrase が含まれること (「傾向」or「うかがえ」)
    assert!(
        ge1.body.contains("傾向") || ge1.body.contains("うかがえ"),
        "P2 fix: hedge word must be present (was 限定的なため = NG)"
    );
    assert!(
        validate_insight_phrase(&ge1.body).is_ok(),
        "Body must pass phrase_validator"
    );
}

/// SW-F06 body の逆証明: 修正前 = 「100%」(禁止語)、修正後 = 「1.00 倍」表記
/// 3 件の pre-existing test failures のうち 1 件を解決した修正の証跡。
#[test]
fn p2_swf06_full_recovery_body_no_100_percent() {
    let ctx = Ctx::new().build();
    let mut f = mock_flow();
    f.covid_recovery_ratio = Some(1.0);
    let out = analyze_flow_insights(&ctx, &f);
    let f06 = out
        .iter()
        .find(|i| i.id == "SW-F06")
        .expect("SW-F06 must fire at recovery=1.0");
    assert_eq!(f06.severity, Severity::Info);
    assert!(
        !f06.body.contains("100%"),
        "P2 fix: 100% (forbidden) must be replaced with 倍率表記"
    );
    assert!(
        f06.body.contains("1.00倍") || f06.body.contains("1.00 倍"),
        "Body must use 倍率表記, got: {}",
        f06.body
    );
    assert!(
        validate_insight_phrase(&f06.body).is_ok(),
        "Body must pass phrase_validator"
    );
}

// ============================================================================
// F1 修正の逆証明テスト群 (2026-04-26 / F1 チーム実装)
// memory `feedback_reverse_proof_tests.md` に基づき、修正前/修正後の具体値を assert する。
// ============================================================================

/// HS-4 TEMP_LOW_THRESHOLD 相対閾値化 (F1 #1) の逆証明。
///
/// **修正前**: TEMP_LOW_THRESHOLD = 0.0 (中立)
/// **修正後**: TEMP_LOW_THRESHOLD = -0.15 (市区町村 P25 -0.1417 の保守的丸め)
///
/// 実データ分布 (hellowork.db, 2026-04-26 直接照会):
/// - 都道府県(47件): min=-0.4063, P25=-0.0377, P50=0.1331, max=0.6063
/// - 市区町村(1004件): min=-2.7286, P25=-0.1417, P50=0.1020, P75=0.4515, max=3.3639
/// - 負値割合: 県27.7% / 市37.8%
///
/// 旧閾値 0.0 では低温区域の半数近くで発火 → 過剰検出。
/// 新閾値 -0.15 で「真に下位四分位」のみ発火に変更。
#[test]
fn f1_hs4_threshold_negative_015_no_fire_for_temp_minus_010() {
    // temp=-0.10: 旧閾値0.0なら発火、新閾値-0.15なら発火しないこと
    let ctx = Ctx::new()
        .vacancy_seishain(0.40, 100.0)
        .temperature(-0.10, 0.05)
        .build();
    assert!(
        gen_find(&ctx, "HS-4").is_none(),
        "F1 fix: HS-4 must not fire at temp=-0.10 (>= -0.15 new threshold).          Was firing under old 0.0 threshold."
    );
}

#[test]
fn f1_hs4_threshold_negative_015_fires_for_temp_minus_020() {
    // temp=-0.20: 新閾値 -0.15 を下回り、依然として発火する
    let ctx = Ctx::new()
        .vacancy_seishain(0.40, 100.0)
        .temperature(-0.20, 0.05)
        .build();
    let i = gen_find(&ctx, "HS-4").expect("HS-4 must fire at temp=-0.20 < -0.15");
    assert_eq!(i.severity, Severity::Warning);
    // body に温度値表示があること
    assert!(i.body.contains("-0.20") || i.body.contains("-0.2"));
}

#[test]
fn f1_hs4_threshold_boundary_at_minus_015() {
    // temp=-0.15 (境界): >= -0.15 で発火しないため、ちょうど境界では発火しない
    let ctx = Ctx::new()
        .vacancy_seishain(0.40, 100.0)
        .temperature(-0.15, 0.05)
        .build();
    assert!(
        gen_find(&ctx, "HS-4").is_none(),
        "F1 fix: HS-4 boundary check (>= TEMP_LOW_THRESHOLD は不発火)"
    );
}

#[test]
fn f1_hs4_threshold_constant_value_is_negative_015() {
    // helpers.rs の定数値そのものを逆証明
    use super::helpers::TEMP_LOW_THRESHOLD;
    assert_eq!(
        TEMP_LOW_THRESHOLD, -0.15,
        "F1 #1: TEMP_LOW_THRESHOLD must be -0.15 (mun-P25 -0.1417 を保守側丸め)"
    );
}
