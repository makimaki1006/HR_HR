//! 全データソースからの一括取得 → InsightContext構築
//! 既存の analysis/fetch.rs と trend/fetch.rs の関数を再利用

use super::super::analysis::fetch as af;
use super::super::helpers::Row;
use super::super::trend::fetch as tf;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;

/// 示唆エンジンへの統一入力
pub struct InsightContext {
    // === ローカルSQLite ===
    pub vacancy: Vec<Row>,
    pub resilience: Vec<Row>,
    pub transparency: Vec<Row>,
    pub temperature: Vec<Row>,
    pub competition: Vec<Row>,
    pub cascade: Vec<Row>,
    pub salary_comp: Vec<Row>,
    pub monopsony: Vec<Row>,
    pub spatial_mismatch: Vec<Row>,
    pub wage_compliance: Vec<Row>,
    pub region_benchmark: Vec<Row>,
    pub text_quality: Vec<Row>,
    // === Turso時系列 ===
    pub ts_counts: Vec<Row>,
    pub ts_vacancy: Vec<Row>,
    pub ts_salary: Vec<Row>,
    pub ts_fulfillment: Vec<Row>,
    pub ts_tracking: Vec<Row>,
    // === Turso外部統計（使用中） ===
    pub ext_job_ratio: Vec<Row>,
    pub ext_labor_stats: Vec<Row>,
    pub ext_min_wage: Vec<Row>,
    pub ext_turnover: Vec<Row>,
    // === Turso外部統計（新規活用） ===
    pub ext_population: Vec<Row>,
    pub ext_pyramid: Vec<Row>,
    pub ext_migration: Vec<Row>,
    pub ext_daytime_pop: Vec<Row>,
    pub ext_establishments: Vec<Row>,
    pub ext_business_dynamics: Vec<Row>,
    // ext_foreign: 未実装のため省略
    pub ext_care_demand: Vec<Row>,
    pub ext_household_spending: Vec<Row>,
    pub ext_climate: Vec<Row>,
    // === Phase A: SSDSE-A 新規6テーブル ===
    pub ext_households: Vec<Row>,
    pub ext_vital: Vec<Row>,
    pub ext_labor_force: Vec<Row>,
    pub ext_medical_welfare: Vec<Row>,
    pub ext_education_facilities: Vec<Row>,
    pub ext_geography: Vec<Row>,
    // === Phase A: 県平均（SUM方式、LS/MF/GE等の比較基準） ===
    pub pref_avg_unemployment_rate: Option<f64>,
    pub pref_avg_single_rate: Option<f64>,
    pub pref_avg_physicians_per_10k: Option<f64>,
    pub pref_avg_daycare_per_1k_children: Option<f64>,
    pub pref_avg_habitable_density: Option<f64>,
    // === Phase B: Agoop 人流（v2_flow_* テーブル未投入時は None） ===
    pub flow: Option<super::flow_context::FlowIndicators>,
    // === 通勤圏（距離ベース） ===
    pub commute_zone_count: usize,
    pub commute_zone_pref_count: usize,
    pub commute_zone_total_pop: i64,
    pub commute_zone_working_age: i64,
    pub commute_zone_elderly: i64,
    // === 通勤フロー（実データ: 国勢調査OD） ===
    pub commute_inflow_total: i64,
    pub commute_outflow_total: i64,
    pub commute_self_rate: f64,
    pub commute_inflow_top3: Vec<(String, String, i64)>, // (pref, muni, count)
    // === メタ ===
    pub pref: String,
    pub muni: String,
}

/// 全データを一括取得してInsightContextを構築
pub(crate) fn build_insight_context(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> InsightContext {
    // Turso時系列データ（Turso必須）
    let (ts_counts, ts_vacancy, ts_salary, ts_fulfillment, ts_tracking) = if let Some(tdb) = turso {
        (
            tf::fetch_ts_counts(tdb, pref),
            tf::fetch_ts_vacancy(tdb, pref),
            tf::fetch_ts_salary(tdb, pref),
            tf::fetch_ts_fulfillment(tdb, pref),
            tf::fetch_ts_tracking(tdb, pref),
        )
    } else {
        (vec![], vec![], vec![], vec![], vec![])
    };

    // Turso外部統計（trend/fetch.rsの関数）
    let (ext_job_ratio, ext_labor_stats, ext_min_wage_ts, ext_turnover_ts) =
        if let Some(tdb) = turso {
            (
                tf::fetch_ext_job_openings_ratio(tdb, pref),
                tf::fetch_ext_labor_stats(tdb, pref),
                tf::fetch_ext_minimum_wage_history(tdb, pref),
                tf::fetch_ext_turnover(tdb, pref),
            )
        } else {
            (vec![], vec![], vec![], vec![])
        };

    let mut ctx = InsightContext {
        // ローカルSQLite（analysis/fetch.rsの関数を再利用）
        vacancy: af::fetch_vacancy_data(db, pref, muni),
        resilience: af::fetch_resilience_data(db, pref, muni),
        transparency: af::fetch_transparency_data(db, pref, muni),
        temperature: af::fetch_temperature_data(db, pref, muni),
        competition: af::fetch_competition_data(db, pref),
        cascade: af::fetch_cascade_data(db, pref, muni),
        salary_comp: af::fetch_salary_competitiveness(db, pref, muni),
        monopsony: af::fetch_monopsony_data(db, pref, muni),
        spatial_mismatch: af::fetch_spatial_mismatch(db, pref, muni),
        wage_compliance: af::fetch_wage_compliance(db, pref, muni),
        region_benchmark: af::fetch_region_benchmark(db, pref, muni),
        text_quality: af::fetch_text_quality(db, pref, muni),
        // Turso時系列
        ts_counts,
        ts_vacancy,
        ts_salary,
        ts_fulfillment,
        ts_tracking,
        // Turso外部統計（使用中）
        ext_job_ratio,
        ext_labor_stats,
        ext_min_wage: ext_min_wage_ts,
        ext_turnover: ext_turnover_ts,
        // Turso外部統計（新規活用 - analysis/fetch.rsの関数を再利用）
        ext_population: af::fetch_population_data(db, turso, pref, muni),
        ext_pyramid: af::fetch_population_pyramid(db, turso, pref, muni),
        ext_migration: af::fetch_migration_data(db, turso, pref, muni),
        ext_daytime_pop: af::fetch_daytime_population(db, turso, pref, muni),
        ext_establishments: af::fetch_establishments(db, turso, pref),
        ext_business_dynamics: af::fetch_business_dynamics(db, turso, pref),
        ext_care_demand: af::fetch_care_demand(db, turso, pref),
        ext_household_spending: af::fetch_household_spending(db, turso, pref),
        ext_climate: af::fetch_climate(db, turso, pref),
        // Phase A: SSDSE-A 新規6テーブル
        ext_households: af::fetch_households(db, turso, pref, muni),
        ext_vital: af::fetch_vital_statistics(db, turso, pref, muni),
        ext_labor_force: af::fetch_labor_force(db, turso, pref, muni),
        ext_medical_welfare: af::fetch_medical_welfare(db, turso, pref, muni),
        ext_education_facilities: af::fetch_education_facilities(db, turso, pref, muni),
        ext_geography: af::fetch_geography(db, turso, pref, muni),
        // Phase A: 県平均（SUM方式、market-level benchmark）
        pref_avg_unemployment_rate: af::fetch_prefecture_mean(
            db,
            turso,
            pref,
            "SUM(unemployed)",
            "SUM(employed) + SUM(unemployed)",
            "v2_external_labor_force",
        ),
        pref_avg_single_rate: af::fetch_prefecture_mean(
            db,
            turso,
            pref,
            "SUM(single_households)",
            "SUM(total_households)",
            "v2_external_households",
        ),
        pref_avg_physicians_per_10k: None, // ctx作成後に人口で計算（相互依存回避）
        pref_avg_daycare_per_1k_children: None, // 同上
        pref_avg_habitable_density: None,  // 同上
        // Phase B: Agoop 人流（デフォルトyear=2019、コロナバイアス最小）
        flow: super::flow_context::build_flow_context(db, turso, pref, muni, 2019),
        // 通勤圏（距離ベース）
        commute_zone_count: 0,
        commute_zone_pref_count: 0,
        commute_zone_total_pop: 0,
        commute_zone_working_age: 0,
        commute_zone_elderly: 0,
        // 通勤フロー（実データ）
        commute_inflow_total: 0,
        commute_outflow_total: 0,
        commute_self_rate: 0.0,
        commute_inflow_top3: vec![],
        // メタ
        pref: pref.to_string(),
        muni: muni.to_string(),
    };

    // 通勤圏データ計算（市区町村選択時のみ）
    if !muni.is_empty() {
        let zone = af::fetch_commute_zone(db, pref, muni, 30.0);
        if !zone.is_empty() {
            let mut pref_set = std::collections::HashSet::new();
            for m in &zone {
                pref_set.insert(m.prefecture.clone());
            }
            ctx.commute_zone_count = zone.len();
            ctx.commute_zone_pref_count = pref_set.len();

            let pyramid = af::fetch_commute_zone_pyramid(db, turso, &zone);
            for row in &pyramid {
                let male = super::super::helpers::get_i64(row, "male_count");
                let female = super::super::helpers::get_i64(row, "female_count");
                let total = male + female;
                ctx.commute_zone_total_pop += total;
                let age = super::super::helpers::get_str_ref(row, "age_group");
                match age {
                    "15-19" | "20-24" | "25-29" | "30-34" | "35-39" | "40-44" | "45-49"
                    | "50-54" | "55-59" | "60-64" | "10-19" | "20-29" | "30-39" | "40-49"
                    | "50-59" | "60-69" => ctx.commute_zone_working_age += total,
                    _ => {}
                }
                match age {
                    "65-69" | "70-74" | "75-79" | "80-84" | "85+" | "70-79" | "80+" => {
                        ctx.commute_zone_elderly += total
                    }
                    _ => {}
                }
            }
        }
    }

    // 通勤フロー（実データ）
    if !muni.is_empty() {
        let inflow = af::fetch_commute_inflow(db, pref, muni);
        ctx.commute_inflow_total = inflow.iter().map(|f| f.total_commuters).sum();
        ctx.commute_inflow_top3 = inflow
            .iter()
            .take(3)
            .map(|f| {
                (
                    f.partner_pref.clone(),
                    f.partner_muni.clone(),
                    f.total_commuters,
                )
            })
            .collect();

        let outflow = af::fetch_commute_outflow(db, pref, muni);
        ctx.commute_outflow_total = outflow.iter().map(|f| f.total_commuters).sum();
        ctx.commute_self_rate = af::fetch_self_commute_rate(db, pref, muni);
    }

    ctx
}
