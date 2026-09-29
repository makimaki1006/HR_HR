//! 地域分析 partial の HTML を金型 (golden) と突き合わせる (React 移行 W4, 2026-09-29)。
//!
//! # なぜ要るか
//! JSON 化 (計画 §2.8 手順 3) で render 側を触っても、旧画面の HTML が 1 バイトも
//! 変わらないことを固定する。金型は **リファクタ前のコード** で作ったもの。
//!
//! # 金型の作り直し
//! ```text
//! UPDATE_GOLDEN=1 cargo test --lib regional_analysis::golden_tests
//! ```
//! 旧画面の見た目を意図して変えたときだけ作り直す。通らないまま金型だけ更新するのは、
//! ずれを見えなくするだけなので禁止。

use super::fetch::{
    CompanyPoint, ForeignResidentRow, ForeignResidents, IndustryStructure, IndustryStructureRow,
    InternetUsage, JobOpeningsRatioData, JobOpeningsRatioPoint, LaborStatsRow, OccupationDist,
    OccupationRow, PopulationPyramid, PyramidBand, RegionalFilter, WageComparison,
};
use super::render::*;

const GOLDEN_DIR: &str = "tests/fixtures/w4/regional";

fn check(name: &str, html: &str) {
    let path = format!("{}/{GOLDEN_DIR}/{name}.html", env!("CARGO_MANIFEST_DIR"));
    if std::env::var("UPDATE_GOLDEN").as_deref() == Ok("1") {
        std::fs::create_dir_all(format!("{}/{GOLDEN_DIR}", env!("CARGO_MANIFEST_DIR"))).unwrap();
        std::fs::write(&path, html).unwrap();
        return;
    }
    let want = std::fs::read_to_string(&path)
        .unwrap_or_else(|_| panic!("{path} が無い。UPDATE_GOLDEN=1 で作る"));
    assert_eq!(html, want, "{name}: 金型と HTML が一致しない");
}

pub(super) fn pref() -> RegionalFilter {
    RegionalFilter {
        prefecture: "東京都".into(),
        municipality: String::new(),
        job_type: String::new(),
    }
}

pub(super) fn muni() -> RegionalFilter {
    RegionalFilter {
        prefecture: "東京都".into(),
        municipality: "新宿区<b>".into(),
        job_type: String::new(),
    }
}

pub(super) fn jor() -> JobOpeningsRatioData {
    JobOpeningsRatioData {
        points: vec![
            JobOpeningsRatioPoint {
                year: 2021,
                ratio: 1.234,
            },
            JobOpeningsRatioPoint {
                year: 2022,
                ratio: 0.987,
            },
        ],
        has_data: true,
    }
}

pub(super) fn labor() -> LaborStatsRow {
    LaborStatsRow {
        fiscal_year: 2022,
        unemployment_rate: Some(2.567),
        separation_rate: Some(14.2),
        monthly_salary_male: Some(390.0),
        monthly_salary_female: Some(287.3),
        working_hours_male: Some(165.0),
        working_hours_female: None,
        part_time_wage_male: Some(2084.4),
        part_time_wage_female: Some(1350.6),
    }
}

pub(super) fn industry() -> IndustryStructure {
    IndustryStructure {
        rows: vec![
            IndustryStructureRow {
                industry: "医療,福祉".into(),
                employees: 12345,
            },
            IndustryStructureRow {
                industry: "卸売業<script>".into(),
                employees: 6789,
            },
        ],
        total: 19134,
        granularity: "都道府県".into(),
        has_data: true,
    }
}

pub(super) fn pyramid() -> PopulationPyramid {
    PopulationPyramid {
        bands: vec![
            PyramidBand {
                age_group: "85歳以上".into(),
                male_count: 100,
                female_count: 250,
            },
            PyramidBand {
                age_group: "0〜4歳".into(),
                male_count: 500,
                female_count: 480,
            },
            PyramidBand {
                age_group: "10〜14歳".into(),
                male_count: 520,
                female_count: 505,
            },
        ],
        granularity: "市区町村".into(),
        area_name: "新宿区".into(),
        has_data: true,
    }
}

pub(super) fn wage() -> WageComparison {
    WageComparison {
        hourly_min_wage: Some(1113.0),
        has_data: true,
    }
}

pub(super) fn companies() -> Vec<CompanyPoint> {
    (0..22)
        .map(|i| CompanyPoint {
            company_name: format!("会社{i}&Co"),
            employee_count: 1000 - i * 10,
            growth_rate_1y: -3.26 + i as f64 * 0.51,
            industry: "製造業".into(),
        })
        .collect()
}

pub(super) fn foreign() -> ForeignResidents {
    ForeignResidents {
        rows: (0..14)
            .map(|i| ForeignResidentRow {
                visa_status: format!("資格{i}"),
                count: 1400 - i * 100,
            })
            .collect(),
        total: 10500,
        survey_period: "2023".into(),
        has_data: true,
    }
}

pub(super) fn internet() -> InternetUsage {
    InternetUsage {
        usage_rate: Some(85.55),
        smartphone_rate: None,
        year: Some(2023),
        has_data: true,
    }
}

pub(super) fn occupation() -> OccupationDist {
    OccupationDist {
        rows: vec![
            OccupationRow {
                occupation: "事務従事者".into(),
                population: 3000,
            },
            OccupationRow {
                occupation: "販売従事者".into(),
                population: 1000,
            },
        ],
        total: 4000,
        granularity: "市区町村".into(),
        area_name: "新宿区".into(),
        has_data: true,
    }
}

#[test]
fn job_openings_ratio_matches_golden() {
    check("jor_data", &render_job_openings_ratio(&pref(), &jor()));
    check(
        "jor_empty",
        &render_job_openings_ratio(
            &muni(),
            &JobOpeningsRatioData {
                points: vec![],
                has_data: false,
            },
        ),
    );
}

#[test]
fn labor_stats_matches_golden() {
    check("labor_data", &render_labor_stats(&pref(), Some(&labor())));
    check("labor_none", &render_labor_stats(&muni(), None));
}

#[test]
fn industry_structure_matches_golden() {
    check("industry_data", &render_industry_structure(&muni(), &industry()));
    check(
        "industry_empty",
        &render_industry_structure(
            &pref(),
            &IndustryStructure {
                rows: vec![],
                total: 0,
                granularity: "都道府県".into(),
                has_data: false,
            },
        ),
    );
}

#[test]
fn population_pyramid_matches_golden() {
    check("pyramid_data", &render_population_pyramid(&muni(), &pyramid()));
    check(
        "pyramid_empty",
        &render_population_pyramid(
            &pref(),
            &PopulationPyramid {
                bands: vec![],
                granularity: "都道府県".into(),
                area_name: String::new(),
                has_data: false,
            },
        ),
    );
}

#[test]
fn wage_comparison_matches_golden() {
    check("wage_data", &render_wage_comparison(&pref(), &wage()));
    check(
        "wage_empty",
        &render_wage_comparison(
            &pref(),
            &WageComparison {
                hourly_min_wage: None,
                has_data: false,
            },
        ),
    );
}

#[test]
fn company_matrix_matches_golden() {
    check("company_data", &render_company_matrix(&pref(), &companies()));
    check("company_empty", &render_company_matrix(&muni(), &[]));
}

#[test]
fn foreign_residents_matches_golden() {
    check("foreign_data", &render_foreign_residents(&pref(), &foreign()));
    check(
        "foreign_empty",
        &render_foreign_residents(&pref(), &ForeignResidents::default()),
    );
}

#[test]
fn internet_usage_matches_golden() {
    check("internet_data", &render_internet_usage(&pref(), &internet()));
    check(
        "internet_empty",
        &render_internet_usage(&pref(), &InternetUsage::default()),
    );
}

#[test]
fn occupation_matches_golden() {
    check("occupation_data", &render_occupation(&muni(), &occupation()));
    check(
        "occupation_empty",
        &render_occupation(&pref(), &OccupationDist::default()),
    );
}
