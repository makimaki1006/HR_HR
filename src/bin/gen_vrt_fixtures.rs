//! VRT (Visual Regression Test) 用 決定論的 HTML fixture ジェネレータ (2026-07 追加)
//!
//! # 目的
//!
//! `navy_report` の各セクション (01-09 + 07.5 + 07.6) を、**DB / サーバ /
//! ネットワークなし**でレンダリングし、バイト単位で再現可能な HTML fixture を
//! `vrt/fixtures/*.html` に出力する。CI (ubuntu-latest) で baseline を生成・
//! 比較することで、レポート描画の意図しない差分を機械的に検出する。
//!
//! # 決定性の担保
//!
//! - 入力は本ファイル内で構築した**合成データのみ** (実企業名・実スクレイピング
//!   データは一切含まない。公開リポのため)。
//! - 生成日時は `REPORT_FIXED_TIMESTAMP` 環境変数で固定 (main 冒頭でセット)。
//!   これにより §08 等の「YYYY年MM月DD日 HH:MM 時点」がラン毎に変化しない。
//! - HashMap のイテレーション順に依存する箇所を避けるため、可変長データは
//!   すべて `Vec` (順序保持) で与える。`hw_enrichment_map` は空 (未使用)。
//!
//! # 使い方
//!
//! ```text
//! cargo run --bin gen_vrt_fixtures
//! # → vrt/fixtures/report_mi.html    (MarketIntelligence variant フル)
//! # → vrt/fixtures/report_basic.html (Public variant, 07.5/07.6/09 なし)
//! ```
//!
//! # サンセット基準
//!
//! VRT 専用。8 週間 (2026-09 目安) で VRT による回帰検出実績がゼロなら、
//! 本 bin・fixture・`render_survey_report_page_for_vrt` フックごと縮小を検討する。

use std::collections::HashMap;
use std::path::Path;

use serde_json::Value;

use rust_dashboard::handlers::insight::fetch::InsightContext;
use rust_dashboard::handlers::survey::aggregator::{
    CompanyAgg, EmpTypeSalary, JobBoxRecord, JobboxAnalysis, PopularityAnalysis, ScatterPoint,
    SurveyAggregation,
};
use rust_dashboard::handlers::survey::job_seeker::{JobSeekerAnalysis, SalaryRangePerception};
use rust_dashboard::handlers::survey::report_html::{
    render_survey_report_page_for_vrt, ReportVariant,
};
use rust_dashboard::handlers::survey::statistics::{EnhancedStats, QuartileStats};

// ── Row ビルダー (InsightContext の Vec<Row> 用) ────────────────────────
// Row = HashMap<String, serde_json::Value> (helpers::Row の実体)。
type Row = HashMap<String, Value>;

fn row(pairs: &[(&str, Value)]) -> Row {
    pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), v.clone()))
        .collect()
}
fn vs(x: &str) -> Value {
    Value::from(x.to_string())
}
fn vi(x: i64) -> Value {
    Value::from(x)
}
fn vf(x: f64) -> Value {
    Value::from(x)
}

// ── 合成 SurveyAggregation ─────────────────────────────────────────────

/// 25 件の求人ボックスレコードを決定論的に生成 (Section 07.5 用)。
fn build_jobbox_records() -> Vec<JobBoxRecord> {
    let names = [
        "サンプル運輸株式会社",
        "架空ロジスティクス株式会社",
        "テスト物流サービス",
        "モデル配送センター",
        "ダミー陸運株式会社",
    ];
    let munis = ["高崎市", "前橋市", "太田市", "伊勢崎市", "桐生市"];
    let emps = [
        "正社員",
        "契約社員",
        "パート・アルバイト",
        "正社員",
        "派遣社員",
    ];
    (0..25)
        .map(|i| {
            let holidays = 85 + (i % 11) * 5; // 85..135 の範囲を巡回
            let base = 210_000 + (i % 8) * 12_000;
            JobBoxRecord {
                company_name: names[i as usize % names.len()].to_string(),
                job_title: format!("ドライバー職 (No.{:02})", i + 1),
                location: format!("群馬県 {}", munis[i as usize % munis.len()]),
                employment_type: emps[i as usize % emps.len()].to_string(),
                annual_holidays: holidays,
                salary_min: Some(base),
                salary_max: Some(base + 90_000),
            }
        })
        .collect()
}

/// 年間休日値を 6 カテゴリに分類 (fixture 用の簡易バケッタ)。
fn holiday_category(h: i64) -> &'static str {
    match h {
        i if i <= 89 => "～89日",
        i if i <= 104 => "90～104日",
        i if i <= 119 => "105～119日",
        i if i <= 124 => "120～124日",
        i if i <= 129 => "125～129日",
        _ => "130日～",
    }
}

fn build_jobbox() -> JobboxAnalysis {
    let records = build_jobbox_records();
    let holidays: Vec<i64> = records.iter().map(|r| r.annual_holidays).collect();

    // カテゴリ分布 (順序固定)
    let cats = [
        "～89日",
        "90～104日",
        "105～119日",
        "120～124日",
        "125～129日",
        "130日～",
    ];
    let distribution: Vec<(String, usize)> = cats
        .iter()
        .map(|c| {
            let n = holidays
                .iter()
                .filter(|h| holiday_category(**h) == *c)
                .count();
            ((*c).to_string(), n)
        })
        .collect();

    // 給与×年間休日 散布図 (月給換算: min と max の中央値近似で x を作る)
    let scatter: Vec<ScatterPoint> = records
        .iter()
        .map(|r| ScatterPoint {
            x: r.salary_min.unwrap_or(0),
            y: r.annual_holidays,
        })
        .collect();
    let scatter_emp: Vec<(i64, i64, String)> = records
        .iter()
        .map(|r| {
            (
                r.salary_min.unwrap_or(0),
                r.annual_holidays,
                r.employment_type.clone(),
            )
        })
        .collect();

    let n = holidays.len() as f64;
    let ge120 = holidays.iter().filter(|h| **h >= 120).count() as f64 / n;
    let ge125 = holidays.iter().filter(|h| **h >= 125).count() as f64 / n;

    JobboxAnalysis {
        annual_holidays_values: holidays,
        annual_holidays_category_distribution: distribution,
        salary_vs_holidays_scatter: scatter,
        jobbox_records: records,
        holiday_pct_ge_120: ge120,
        holiday_pct_ge_125: ge125,
        holiday_stddev: 15.2,
        holiday_q3: 125,
        salary_vs_holidays_scatter_emp: scatter_emp,
        salary_holidays_correlation: Some(0.32),
        salary_holidays_regression: Some((0.00008, 95.0)),
        // セグメント別給与統計は fixture では省略 (default)。
        ..Default::default()
    }
}

fn build_popularity() -> PopularityAnalysis {
    PopularityAnalysis {
        popular_count: 6,
        super_popular_count: 2,
        none_count: 17,
        popular_ratio: 8.0 / 25.0,
        indeed_sp_total: 25,
        popular_salary_median: Some(268_000),
        non_popular_salary_median: Some(242_000),
        popular_holidays_median: Some(120),
        non_popular_holidays_median: Some(110),
        popular_n_salary: 8,
        non_popular_n_salary: 17,
        popular_n_holidays: 8,
        non_popular_n_holidays: 17,
        ..Default::default()
    }
}

/// リッチな合成 `SurveyAggregation`。
///
/// `include_jobbox=false` の場合は Section 07.5 / 07.6 が描画されないよう
/// `jobbox` / `popularity` を default (空) にする (Public/basic fixture 用)。
fn build_agg(include_jobbox: bool) -> SurveyAggregation {
    SurveyAggregation {
        total_count: 250,
        new_count: 60,
        salary_parse_rate: 0.86,
        location_parse_rate: 0.93,
        dominant_prefecture: Some("群馬県".to_string()),
        dominant_municipality: Some("高崎市".to_string()),
        by_prefecture: vec![
            ("群馬県".to_string(), 130),
            ("埼玉県".to_string(), 45),
            ("栃木県".to_string(), 35),
            ("東京都".to_string(), 25),
            ("長野県".to_string(), 15),
        ],
        by_salary_range: vec![
            ("〜20万".to_string(), 28),
            ("20〜25万".to_string(), 82),
            ("25〜30万".to_string(), 90),
            ("30〜35万".to_string(), 35),
            ("35万〜".to_string(), 15),
        ],
        by_employment_type: vec![
            ("正社員".to_string(), 175),
            ("契約社員".to_string(), 30),
            ("パート・アルバイト".to_string(), 30),
            ("派遣社員".to_string(), 15),
        ],
        by_tags: vec![
            ("未経験可".to_string(), 70),
            ("資格不問".to_string(), 45),
            ("週休2日".to_string(), 60),
        ],
        salary_values: vec![
            200_000, 240_000, 260_000, 280_000, 300_000, 320_000, 350_000,
        ],
        enhanced_stats: Some(EnhancedStats {
            count: 7,
            mean: 278_000,
            median: 280_000,
            min: 200_000,
            max: 350_000,
            std_dev: 48_000,
            bootstrap_ci: None,
            trimmed_mean: None,
            quartiles: Some(QuartileStats {
                q1: 250_000,
                q2: 280_000,
                q3: 310_000,
                iqr: 60_000,
                lower_bound: 160_000,
                upper_bound: 400_000,
                outlier_count: 0,
                inlier_count: 7,
            }),
            reliability: "高".to_string(),
        }),
        by_company: vec![
            CompanyAgg {
                name: "サンプル運輸株式会社".to_string(),
                count: 42,
                avg_salary: 285_000,
                median_salary: 280_000,
            },
            CompanyAgg {
                name: "架空ロジスティクス株式会社".to_string(),
                count: 31,
                avg_salary: 272_000,
                median_salary: 265_000,
            },
            CompanyAgg {
                name: "テスト物流サービス".to_string(),
                count: 24,
                avg_salary: 258_000,
                median_salary: 255_000,
            },
        ],
        by_emp_type_salary: vec![
            EmpTypeSalary {
                emp_type: "正社員".to_string(),
                count: 175,
                avg_salary: 288_000,
                median_salary: 285_000,
            },
            EmpTypeSalary {
                emp_type: "契約社員".to_string(),
                count: 30,
                avg_salary: 252_000,
                median_salary: 250_000,
            },
            EmpTypeSalary {
                emp_type: "パート・アルバイト".to_string(),
                count: 30,
                avg_salary: 0,
                median_salary: 0,
            },
        ],
        salary_min_values: vec![
            200_000, 220_000, 240_000, 250_000, 260_000, 280_000, 300_000,
        ],
        salary_max_values: vec![
            260_000, 290_000, 310_000, 330_000, 350_000, 370_000, 400_000,
        ],
        by_tag_salary: Vec::new(),
        by_municipality_salary: Vec::new(),
        scatter_min_max: vec![
            ScatterPoint {
                x: 200_000,
                y: 260_000,
            },
            ScatterPoint {
                x: 240_000,
                y: 310_000,
            },
            ScatterPoint {
                x: 280_000,
                y: 350_000,
            },
        ],
        regression_min_max: None,
        by_prefecture_salary: Vec::new(),
        is_hourly: false,
        by_emp_group_native: Vec::new(),
        outliers_removed_total: 8,
        salary_values_raw_count: 258,
        salary_min_values_native: Vec::new(),
        salary_max_values_native: Vec::new(),
        scatter_min_max_native: Vec::new(),
        jobbox: if include_jobbox {
            build_jobbox()
        } else {
            JobboxAnalysis::default()
        },
        popularity: if include_jobbox {
            build_popularity()
        } else {
            PopularityAnalysis::default()
        },
    }
}

fn build_seeker() -> JobSeekerAnalysis {
    JobSeekerAnalysis {
        expected_salary: Some(255_000),
        salary_range_perception: Some(SalaryRangePerception {
            avg_range_width: 65_000,
            avg_lower: 235_000,
            avg_upper: 300_000,
            expected_point: 256_000,
            narrow_count: 35,
            medium_count: 110,
            wide_count: 25,
        }),
        inexperience_analysis: None,
        new_listings_premium: Some(12_000),
        total_analyzed: 250,
    }
}

// ── 合成 InsightContext ────────────────────────────────────────────────
//
// 列名は db_columns.rs (ext_* 列名 SSoT) と section_0X_*.rs の get_* キーに合わせる。
// 各テーブル 3-5 行の代表データ。全表を網羅はせず、Section 02/04/05/06/07/09 が
// 「no data」ではなく実データ描画になる主要テーブルを埋める。
fn build_ctx() -> InsightContext {
    let mut c = InsightContext::default();
    c.pref = "群馬県".to_string();
    c.muni = "高崎市".to_string();

    // Section 01/04/05/09: HW 産業別 / 職種別 件数
    c.hw_industry_counts = vec![
        ("運輸業,郵便業".to_string(), 420),
        ("医療,福祉".to_string(), 380),
        ("製造業".to_string(), 350),
        ("卸売業,小売業".to_string(), 210),
        ("建設業".to_string(), 140),
    ];
    c.hw_job_type_counts = vec![
        ("ドライバー".to_string(), 320),
        ("介護職".to_string(), 260),
        ("製造オペレーター".to_string(), 180),
        ("販売スタッフ".to_string(), 120),
        ("倉庫作業".to_string(), 90),
    ];

    // Section 03: 給与レンジ散布図 (下限, 上限)
    c.salary_scatter_pairs = vec![
        (200_000.0, 260_000.0),
        (220_000.0, 290_000.0),
        (240_000.0, 310_000.0),
        (260_000.0, 330_000.0),
        (280_000.0, 360_000.0),
    ];

    // Section 04 / 07: 最低賃金履歴 (fiscal_year, hourly_min_wage)
    c.ext_min_wage = vec![
        row(&[("fiscal_year", vi(2022)), ("hourly_min_wage", vi(895))]),
        row(&[("fiscal_year", vi(2023)), ("hourly_min_wage", vi(935))]),
        row(&[("fiscal_year", vi(2024)), ("hourly_min_wage", vi(985))]),
    ];
    // 有効求人倍率 (fiscal_year, ratio_total)
    c.ext_job_ratio = vec![
        row(&[("fiscal_year", vi(2022)), ("ratio_total", vf(1.28))]),
        row(&[("fiscal_year", vi(2023)), ("ratio_total", vf(1.35))]),
        row(&[("fiscal_year", vi(2024)), ("ratio_total", vf(1.42))]),
    ];
    // 離職率/入職率 (fiscal_year, separation_rate, entry_rate)
    c.ext_turnover = vec![
        row(&[
            ("fiscal_year", vi(2022)),
            ("separation_rate", vf(14.2)),
            ("entry_rate", vf(15.1)),
        ]),
        row(&[
            ("fiscal_year", vi(2023)),
            ("separation_rate", vf(13.8)),
            ("entry_rate", vf(14.6)),
        ]),
    ];
    // 失業率/労働力率 (unemployment_rate, labor_force_participation_rate, reference_date)
    c.ext_labor_force = vec![row(&[
        ("unemployment_rate", vf(2.4)),
        ("labor_force_participation_rate", vf(61.8)),
        ("reference_date", vs("2024")),
    ])];

    // Section 04: 産業構造 (industry_name, employees_total, industry_code)
    c.ext_industry_employees = vec![
        row(&[
            ("industry_code", vs("H")),
            ("industry_name", vs("運輸業,郵便業")),
            ("employees_total", vi(48_000)),
        ]),
        row(&[
            ("industry_code", vs("P")),
            ("industry_name", vs("医療,福祉")),
            ("employees_total", vi(72_000)),
        ]),
        row(&[
            ("industry_code", vs("E")),
            ("industry_name", vs("製造業")),
            ("employees_total", vi(95_000)),
        ]),
        row(&[
            ("industry_code", vs("I")),
            ("industry_name", vs("卸売業,小売業")),
            ("employees_total", vi(66_000)),
        ]),
    ];
    // 事業所数 / 事業所ダイナミクス (件数系)
    c.ext_establishments = vec![
        row(&[
            ("industry_name", vs("運輸業,郵便業")),
            ("establishments", vi(1_200)),
        ]),
        row(&[
            ("industry_name", vs("医療,福祉")),
            ("establishments", vi(2_100)),
        ]),
        row(&[
            ("industry_name", vs("製造業")),
            ("establishments", vi(1_800)),
        ]),
    ];
    c.ext_business_dynamics = vec![
        row(&[
            ("reference_year", vi(2023)),
            ("entry_rate", vf(4.1)),
            ("exit_rate", vf(3.6)),
        ]),
        row(&[
            ("reference_year", vi(2024)),
            ("entry_rate", vf(4.3)),
            ("exit_rate", vf(3.5)),
        ]),
    ];

    // Section 06: 人口ピラミッド (age_group, male_count, female_count)
    c.ext_pyramid = vec![
        row(&[
            ("age_group", vs("0-14")),
            ("male_count", vi(52_000)),
            ("female_count", vi(49_000)),
        ]),
        row(&[
            ("age_group", vs("15-29")),
            ("male_count", vi(61_000)),
            ("female_count", vi(58_000)),
        ]),
        row(&[
            ("age_group", vs("30-44")),
            ("male_count", vi(78_000)),
            ("female_count", vi(75_000)),
        ]),
        row(&[
            ("age_group", vs("45-64")),
            ("male_count", vi(95_000)),
            ("female_count", vi(94_000)),
        ]),
        row(&[
            ("age_group", vs("65-")),
            ("male_count", vi(88_000)),
            ("female_count", vi(110_000)),
        ]),
    ];
    // Section 06: 教育施設密度
    c.ext_education_facilities = vec![
        row(&[("facility_type", vs("小学校")), ("count", vi(120))]),
        row(&[("facility_type", vs("中学校")), ("count", vi(62))]),
        row(&[("facility_type", vs("高等学校")), ("count", vi(38))]),
    ];

    // Section 07: 家計支出 (prefecture, category, monthly_amount, reference_year)
    c.ext_household_spending = vec![
        row(&[
            ("prefecture", vs("群馬県")),
            ("category", vs("消費支出")),
            ("monthly_amount", vi(268_000)),
            ("reference_year", vi(2024)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("category", vs("住居")),
            ("monthly_amount", vi(18_500)),
            ("reference_year", vi(2024)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("category", vs("食料")),
            ("monthly_amount", vi(72_000)),
            ("reference_year", vi(2024)),
        ]),
    ];
    // Section 07: ライフスタイル (社会生活 / インターネット利用)
    c.ext_social_life = vec![
        row(&[
            ("category", vs("スポーツ")),
            ("participation_rate", vf(68.2)),
        ]),
        row(&[
            ("category", vs("趣味・娯楽")),
            ("participation_rate", vf(84.1)),
        ]),
        row(&[
            ("category", vs("学習・自己啓発")),
            ("participation_rate", vf(32.5)),
        ]),
    ];
    c.ext_internet_usage = vec![row(&[
        ("internet_usage_rate", vf(83.4)),
        ("smartphone_ownership_rate", vf(88.7)),
    ])];
    // Section 07: 昼間人口
    c.ext_daytime_pop = vec![row(&[
        ("daytime_population", vi(340_000)),
        ("nighttime_population", vi(370_000)),
        ("daytime_ratio", vf(91.9)),
    ])];

    // Section 02: 地理指標 (total_area_km2, habitable_area_km2, population_density_per_km2 ...)
    c.ext_geography = vec![row(&[
        ("total_area_km2", vf(6_362.3)),
        ("habitable_area_km2", vf(2_285.0)),
        ("population_density_per_km2", vf(300.5)),
        ("habitable_density_per_km2", vf(836.2)),
        ("reference_year", vi(2024)),
    ])];

    // Section 02: 通勤流入 top3 (pref, muni, count)
    c.commute_inflow_top3 = vec![
        ("群馬県".to_string(), "前橋市".to_string(), 12_500),
        ("埼玉県".to_string(), "本庄市".to_string(), 4_200),
        ("群馬県".to_string(), "藤岡市".to_string(), 3_800),
    ];
    c.commute_inflow_total = 42_000;
    c.commute_outflow_total = 31_000;
    c.commute_self_rate = 0.72;

    // Section 10 (詳細版): cross_future_workforce (図1 働き手の将来マップ)
    // 列名は db_columns.rs の const と一致。介護・HW は含まない (国の将来人口推計 由来)。
    c.cross_future_workforce = vec![
        row(&[
            ("prefecture", vs("群馬県")),
            ("muni_code", vs("10202")),
            ("municipality", vs("高崎市")),
            ("wa_2020", vi(230_000)),
            ("working_age_ratio_2020", vf(59.5)),
            ("wa_decline_rate", vf(-18.2)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("muni_code", vs("10201")),
            ("municipality", vs("前橋市")),
            ("wa_2020", vi(200_000)),
            ("working_age_ratio_2020", vf(58.0)),
            ("wa_decline_rate", vf(-22.5)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("muni_code", vs("10205")),
            ("municipality", vs("太田市")),
            ("wa_2020", vi(140_000)),
            ("working_age_ratio_2020", vf(61.2)),
            ("wa_decline_rate", vf(-15.8)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("muni_code", vs("10204")),
            ("municipality", vs("伊勢崎市")),
            ("wa_2020", vi(125_000)),
            ("working_age_ratio_2020", vf(60.1)),
            ("wa_decline_rate", vf(-19.4)),
        ]),
        row(&[
            ("prefecture", vs("群馬県")),
            ("muni_code", vs("10203")),
            ("municipality", vs("桐生市")),
            ("wa_2020", vi(60_000)),
            ("working_age_ratio_2020", vf(54.3)),
            ("wa_decline_rate", vf(-33.6)),
        ]),
    ];

    // Section 10 (詳細版): cross_wage_public (図2 給与の相場比較)
    //   scheduled_earnings=所定内給与 / minwage_fulltime_monthly=最低賃金×160時間 (10月改定の階段)
    c.cross_wage_public = (1..=12)
        .map(|mo: i64| {
            let scheduled = 248_000 + (mo - 1) * 700; // 248,000 → 255,700 へ漸増
            let hourly = if mo >= 10 { 1_050 } else { 985 };
            row(&[
                ("prefecture", vs("群馬県")),
                ("year_month", vs(&format!("2025-{:02}", mo))),
                ("scheduled_earnings", vi(scheduled)),
                ("min_wage_monthly_160h", vi(hourly * 160)),
                ("min_wage_hourly", vi(hourly)),
            ])
        })
        .collect();

    // Section 10 (詳細版): cross_switcher_supply (図3 転職を考えている人 / 図4 採用ネック診断)
    //   region_code "00000"=全国 / "10000"=群馬県 / "10202"=高崎市
    c.cross_switcher_supply = vec![
        row(&[
            ("region_code", vs("00000")),
            ("region_name", vs("全国")),
            ("job_change_desire_rate", vf(8.5)),
            ("side_job_holders", vi(3_000_000)),
            ("additional_job_seekers", vi(4_200_000)),
            ("job_change_seekers", vi(6_800_000)),
            ("pref_job_openings_ratio", vf(1.30)),
        ]),
        row(&[
            ("region_code", vs("10000")),
            ("region_name", vs("群馬県")),
            ("job_change_desire_rate", vf(7.8)),
            ("side_job_holders", vi(42_000)),
            ("additional_job_seekers", vi(55_000)),
            ("job_change_seekers", vi(88_000)),
            ("pref_job_openings_ratio", vf(1.42)),
        ]),
        row(&[
            ("region_code", vs("10202")),
            ("region_name", vs("高崎市")),
            ("job_change_desire_rate", vf(7.9)),
            ("side_job_holders", vi(9_500)),
            ("additional_job_seekers", vi(12_000)),
            ("job_change_seekers", vi(24_000)),
            ("pref_job_openings_ratio", vf(1.40)),
        ]),
    ];

    c
}

fn write_fixture(out_dir: &Path, name: &str, html: &str) {
    let path = out_dir.join(name);
    std::fs::write(&path, html).unwrap_or_else(|e| panic!("write {} 失敗: {e}", path.display()));
    println!("  生成: {} ({} bytes)", path.display(), html.len());
}

fn main() {
    // 決定化: 生成日時を固定 (呼出側が未設定の場合のみ)。
    if std::env::var("REPORT_FIXED_TIMESTAMP").is_err() {
        std::env::set_var("REPORT_FIXED_TIMESTAMP", "2026-01-15 09:00");
    }

    // 出力先: <crate_root>/vrt/fixtures/
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let out_dir = Path::new(manifest_dir).join("vrt").join("fixtures");
    std::fs::create_dir_all(&out_dir).expect("vrt/fixtures ディレクトリ作成失敗");

    println!(
        "VRT fixture 生成開始 (REPORT_FIXED_TIMESTAMP={})",
        std::env::var("REPORT_FIXED_TIMESTAMP").unwrap_or_default()
    );

    let ctx = build_ctx();
    let seeker = build_seeker();

    // 1) MarketIntelligence variant: 全セクション (01-09 + 07.5 + 07.6)
    let agg_full = build_agg(true);
    let html_mi = render_survey_report_page_for_vrt(
        &agg_full,
        &seeker,
        &agg_full.by_company,
        &agg_full.by_emp_type_salary,
        &agg_full.salary_min_values,
        &agg_full.salary_max_values,
        Some(&ctx),
        ReportVariant::MarketIntelligence,
    );
    write_fixture(&out_dir, "report_mi.html", &html_mi);

    // 2) Public variant: 07.5 / 07.6 / 09 なし (jobbox/popularity 空 + Public)
    let agg_basic = build_agg(false);
    let html_basic = render_survey_report_page_for_vrt(
        &agg_basic,
        &seeker,
        &agg_basic.by_company,
        &agg_basic.by_emp_type_salary,
        &agg_basic.salary_min_values,
        &agg_basic.salary_max_values,
        Some(&ctx),
        ReportVariant::Public,
    );
    write_fixture(&out_dir, "report_basic.html", &html_basic);

    // 3) Extended variant: MarketIntelligence の全セクション + Section 10 (追加 4 図)
    //    report_mi.html は Extended とは別 variant のため sha は不変 (標準版不変の証明)。
    let html_extended = render_survey_report_page_for_vrt(
        &agg_full,
        &seeker,
        &agg_full.by_company,
        &agg_full.by_emp_type_salary,
        &agg_full.salary_min_values,
        &agg_full.salary_max_values,
        Some(&ctx),
        ReportVariant::Extended,
    );
    write_fixture(&out_dir, "report_extended.html", &html_extended);

    // 4) SP版 (仮) variant: Extended の全セクション + SP 専用ブロック
    //    (経営サマリー1ページ / 各ページ結論バンド / 給与四分位 / 優先アクション表)。
    //    既存 3 fixture (basic/mi/extended) の sha は不変 (SP は別 variant のため)。
    let html_sp = render_survey_report_page_for_vrt(
        &agg_full,
        &seeker,
        &agg_full.by_company,
        &agg_full.by_emp_type_salary,
        &agg_full.salary_min_values,
        &agg_full.salary_max_values,
        Some(&ctx),
        ReportVariant::Sp,
    );
    write_fixture(&out_dir, "report_sp.html", &html_sp);

    println!("VRT fixture 生成完了");
}
