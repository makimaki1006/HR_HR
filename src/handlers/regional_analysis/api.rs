//! 地域分析タブの JSON API (React 移行 W4, 2026-09-29)。
//!
//! 旧 HTML partial (`/api/regional/*`) と同じ取得関数 (`fetch.rs`) と同じ文言・書式
//! (`render.rs` の `*_TITLE` / `*_NOTE` / `fmt_*`) から組み立てる。旧 partial は並走期間中そのまま残す。
//!
//! | route                                      | 旧 partial                              |
//! |--------------------------------------------|-----------------------------------------|
//! | GET /api/app/regional/init                 | /tab/regional_analysis (都道府県一覧)   |
//! | GET /api/app/regional/municipalities       | /api/regional/municipalities            |
//! | GET /api/app/regional/job_openings_ratio   | /api/regional/job_openings_ratio        |
//! | GET /api/app/regional/labor_stats          | /api/regional/labor_stats               |
//! | GET /api/app/regional/industry_structure   | /api/regional/industry_structure        |
//! | GET /api/app/regional/population_pyramid   | /api/regional/population_pyramid        |
//! | GET /api/app/regional/wage_comparison      | /api/regional/wage_comparison           |
//! | GET /api/app/regional/company_matrix       | /api/regional/company_matrix            |
//! | GET /api/app/regional/foreign_residents    | /api/regional/foreign_residents         |
//! | GET /api/app/regional/internet_usage       | /api/regional/internet_usage            |
//! | GET /api/app/regional/occupation           | /api/regional/occupation                |
//!
//! - クエリは旧 partial と同じ `prefecture` / `municipality`。この画面は session のヘッダーフィルタを
//!   読まない (旧画面も独自のフィルタバーを持つ)。
//! - 文字列は **エスケープしない生の値** を返す (React が既定でエスケープする)。
//! - 「都道府県未選択」「DB 未接続」「データなし」は HTTP 200 + `status` で返す。
//!   `{"error": ...}` にすると client.ts が例外にしてしまい、注記として表示できないため。

use std::sync::Arc;

use axum::extract::{Query, State};
use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use ts_rs::TS;

use super::fetch::{
    fetch_company_matrix, fetch_foreign_residents, fetch_industry_structure, fetch_internet_usage,
    fetch_job_openings_ratio, fetch_labor_stats, fetch_municipalities,
    fetch_occupation_distribution, fetch_population_pyramid, fetch_prefectures,
    fetch_wage_comparison, RegionalFilter,
};
use super::handlers::{RegionalParams, COMPANY_MATRIX_LIMIT, INDUSTRY_STRUCTURE_LIMIT};
use super::render as r;
use crate::AppState;

/// パネルの状態。旧 partial のどの分岐に当たるかを表す。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum RegionalPanelStatus {
    /// データあり。`data` が入る。
    Ok,
    /// データなし (旧: 「◯◯ に該当するデータがありません。」)。
    NoData,
    /// 都道府県未選択 (旧: 「都道府県を選択してください。」)。
    PrefRequired,
    /// 外部統計 DB 未接続 (旧: 「外部統計データベースに接続できません。」)。
    DbUnavailable,
    /// 外部企業データ未接続 (企業成長マトリックスのみ)。
    CompanyDataUnavailable,
    /// 集計処理の失敗 (旧: 「集計処理に失敗しました。」)。
    AggregationFailed,
}

/// 1 パネルぶんの応答。`data` は `status == ok` のときだけ入る。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalPanel<T: TS> {
    pub status: RegionalPanelStatus,
    /// パネル見出し (旧 HTML の h3)。
    pub title: String,
    /// 対象地域 (「東京都」「東京都 新宿区」「未選択」)。エスケープ前。
    pub scope_label: String,
    /// 出典・スコープ注記 (旧 HTML の脚注)。status が ok / no_data のときに出す。
    pub note: String,
    /// ok 以外のときに出す文言 (旧 HTML の文言そのまま)。ok のときは null。
    pub message: Option<String>,
    pub data: Option<T>,
}

/// 構成比つきの 1 行 (産業構造・在留外国人・職業別就業者の表)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalShareRow {
    pub label: String,
    #[ts(type = "number")]
    pub value: i64,
    /// 表示用 (例 `12,345`)。
    pub value_display: String,
    /// 構成比 (%)。total が 0 のときは 0。
    pub share_pct: f64,
    /// 表示用 (例 `64.5%`)。
    pub share_display: String,
}

/// 構成比つきの表 (合計行つき)。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalShareTable {
    /// 値の大きい順 (旧 HTML の表の並び)。チャートは旧画面ではこの逆順 (下から大) で描く。
    pub rows: Vec<RegionalShareRow>,
    #[ts(type = "number")]
    pub total: i64,
    pub total_display: String,
    /// 旧画面のチャートに載せる行数 (先頭から)。null なら全行。
    pub chart_limit: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalInitResponse {
    /// 都道府県一覧 (municipality_code_master、名前順)。
    pub prefectures: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalMunicipalitiesResponse {
    pub prefecture: String,
    /// 市区町村一覧 (名前順)。都道府県が空なら空配列。先頭の「全て」は含めない。
    pub municipalities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct JobOpeningsRatioPointJson {
    #[ts(type = "number")]
    pub fiscal_year: i64,
    /// 軸ラベル (例 `2022年度`)。
    pub label: String,
    /// 有効求人倍率 (旧チャートと同じく小数 2 桁に丸めた値)。
    pub ratio: f64,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct JobOpeningsRatioJson {
    pub points: Vec<JobOpeningsRatioPointJson>,
    /// 均衡水準の目安線 (1.0 倍)。
    pub reference_line: f64,
}

/// 指標カード 1 枚。
#[derive(Debug, Clone, Serialize, TS)]
pub struct RegionalStatCard {
    pub key: String,
    pub label: String,
    /// 元の値 (DB の単位のまま)。null は値なし。
    pub value: Option<f64>,
    /// 表示用 (旧 HTML と同じ書式・単位換算済み。値なしは `-`)。
    pub display: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct LaborStatsJson {
    #[ts(type = "number")]
    pub fiscal_year: i64,
    /// 旧 HTML の並び順 (完全失業率 → パート時給(女))。
    pub cards: Vec<RegionalStatCard>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct IndustryStructureJson {
    /// 集計粒度 (「市区町村」「都道府県」)。
    pub granularity: String,
    pub table: RegionalShareTable,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct PyramidBandJson {
    pub age_group: String,
    #[ts(type = "number")]
    pub male: i64,
    #[ts(type = "number")]
    pub female: i64,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct PopulationPyramidJson {
    pub granularity: String,
    /// 年齢の若い順 (旧チャートの y 軸の並び)。
    pub bands: Vec<PyramidBandJson>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct WageComparisonJson {
    pub card: RegionalStatCard,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CompanyPointJson {
    pub company_name: String,
    pub industry: String,
    #[ts(type = "number")]
    pub employee_count: i64,
    pub employee_count_display: String,
    /// 過去 1 年の従業員増減率 (%)。元の値。
    pub growth_rate_1y: f64,
    /// 散布図に載せる値 (旧チャートと同じく小数 1 桁に丸めた値)。
    pub growth_rate_chart: f64,
    /// 表の表示用 (例 `+1.2`)。
    pub growth_rate_display: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CompanyMatrixJson {
    /// 従業員数の多い順 (取得上限あり)。散布図は全点。
    pub points: Vec<CompanyPointJson>,
    /// 表に出す行数 (先頭から)。
    pub table_limit: u32,
    /// 散布図の下の件数注記 (旧 HTML の文言そのまま)。
    pub count_note: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct ForeignResidentsJson {
    pub survey_period: String,
    pub table: RegionalShareTable,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct InternetUsageJson {
    #[ts(type = "number | null")]
    pub year: Option<i64>,
    /// インターネット利用率 → スマートフォン保有率 の順。label に年が入る (旧 HTML と同じ)。
    pub cards: Vec<RegionalStatCard>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct OccupationJson {
    pub granularity: String,
    pub table: RegionalShareTable,
}

// ------------------------------------------------------------------
// 組み立て (純関数。テストは fetch 結果の struct から直接呼ぶ)
// ------------------------------------------------------------------

fn panel<T: TS>(
    status: RegionalPanelStatus,
    title: &str,
    filter: &RegionalFilter,
    note: String,
    message: Option<String>,
    data: Option<T>,
) -> RegionalPanel<T> {
    RegionalPanel {
        status,
        title: title.to_string(),
        scope_label: filter.scope_label_raw(),
        note,
        message,
        data,
    }
}

fn ok<T: TS>(title: &str, filter: &RegionalFilter, note: String, data: T) -> RegionalPanel<T> {
    panel(
        RegionalPanelStatus::Ok,
        title,
        filter,
        note,
        None,
        Some(data),
    )
}

fn no_data<T: TS>(
    title: &str,
    label: &str,
    filter: &RegionalFilter,
    note: String,
) -> RegionalPanel<T> {
    panel(
        RegionalPanelStatus::NoData,
        title,
        filter,
        note,
        Some(r::no_data_message(label)),
        None,
    )
}

/// 取得前の分岐 (都道府県未選択 / DB 未接続)。旧ハンドラと同じ順で判定する。
fn precheck<T: TS>(
    state: &AppState,
    title: &str,
    filter: &RegionalFilter,
    needs_company_db: bool,
) -> Option<RegionalPanel<T>> {
    if filter.prefecture.is_empty() {
        return Some(panel(
            RegionalPanelStatus::PrefRequired,
            title,
            filter,
            String::new(),
            Some(r::PREF_REQUIRED_MESSAGE.to_string()),
            None,
        ));
    }
    if needs_company_db {
        if state.salesnow_db.is_none() {
            return Some(panel(
                RegionalPanelStatus::CompanyDataUnavailable,
                title,
                filter,
                String::new(),
                Some(r::COMPANY_DATA_UNAVAILABLE_MESSAGE.to_string()),
                None,
            ));
        }
    } else if state.turso_db.is_none() && state.hw_db.is_none() {
        return Some(panel(
            RegionalPanelStatus::DbUnavailable,
            title,
            filter,
            String::new(),
            Some(r::DB_UNAVAILABLE_MESSAGE.to_string()),
            None,
        ));
    }
    None
}

fn failed<T: TS>(title: &str, filter: &RegionalFilter) -> RegionalPanel<T> {
    panel(
        RegionalPanelStatus::AggregationFailed,
        title,
        filter,
        String::new(),
        Some(r::AGGREGATION_FAILED_MESSAGE.to_string()),
        None,
    )
}

fn share_table<'a>(
    rows: impl Iterator<Item = (&'a str, i64)>,
    total: i64,
    chart_limit: Option<u32>,
) -> RegionalShareTable {
    let rows = rows
        .map(|(label, value)| {
            let pct = r::share_pct(value, total);
            RegionalShareRow {
                label: label.to_string(),
                value,
                value_display: crate::handlers::overview::format_number(value),
                share_pct: pct,
                share_display: format!("{pct:.1}%"),
            }
        })
        .collect();
    RegionalShareTable {
        rows,
        total,
        total_display: crate::handlers::overview::format_number(total),
        chart_limit,
    }
}

fn stat_card(key: &str, label: String, value: Option<f64>, display: String) -> RegionalStatCard {
    RegionalStatCard {
        key: key.to_string(),
        label,
        value,
        display,
    }
}

pub(crate) fn build_job_openings_ratio(
    filter: &RegionalFilter,
    d: &super::fetch::JobOpeningsRatioData,
) -> RegionalPanel<JobOpeningsRatioJson> {
    let note = r::JOB_OPENINGS_RATIO_NOTE.to_string();
    if !d.has_data || d.points.is_empty() {
        return no_data(r::JOB_OPENINGS_RATIO_TITLE, "有効求人倍率", filter, note);
    }
    let points = d
        .points
        .iter()
        .map(|p| JobOpeningsRatioPointJson {
            fiscal_year: p.year,
            label: format!("{}年度", p.year),
            ratio: r::round_ratio(p.ratio),
        })
        .collect();
    ok(
        r::JOB_OPENINGS_RATIO_TITLE,
        filter,
        note,
        JobOpeningsRatioJson {
            points,
            reference_line: 1.0,
        },
    )
}

pub(crate) fn build_labor_stats(
    filter: &RegionalFilter,
    row: Option<&super::fetch::LaborStatsRow>,
) -> RegionalPanel<LaborStatsJson> {
    let note = r::LABOR_STATS_NOTE.to_string();
    let Some(row) = row else {
        return no_data(r::LABOR_STATS_TITLE, "労働統計", filter, note);
    };
    let pct = |v| r::fmt_opt_f64(v, "%");
    let hours = |v| r::fmt_opt_f64(v, "h");
    let cards = vec![
        stat_card(
            "unemployment_rate",
            "完全失業率".into(),
            row.unemployment_rate,
            pct(row.unemployment_rate),
        ),
        stat_card(
            "separation_rate",
            "離職率".into(),
            row.separation_rate,
            pct(row.separation_rate),
        ),
        stat_card(
            "monthly_salary_male",
            "月収(男性)".into(),
            row.monthly_salary_male,
            r::fmt_monthly_salary(row.monthly_salary_male),
        ),
        stat_card(
            "monthly_salary_female",
            "月収(女性)".into(),
            row.monthly_salary_female,
            r::fmt_monthly_salary(row.monthly_salary_female),
        ),
        stat_card(
            "working_hours_male",
            "所定内労働時間(男)".into(),
            row.working_hours_male,
            hours(row.working_hours_male),
        ),
        stat_card(
            "working_hours_female",
            "所定内労働時間(女)".into(),
            row.working_hours_female,
            hours(row.working_hours_female),
        ),
        stat_card(
            "part_time_wage_male",
            "パート時給(男)".into(),
            row.part_time_wage_male,
            r::fmt_hourly_wage(row.part_time_wage_male),
        ),
        stat_card(
            "part_time_wage_female",
            "パート時給(女)".into(),
            row.part_time_wage_female,
            r::fmt_hourly_wage(row.part_time_wage_female),
        ),
    ];
    ok(
        r::LABOR_STATS_TITLE,
        filter,
        note,
        LaborStatsJson {
            fiscal_year: row.fiscal_year,
            cards,
        },
    )
}

pub(crate) fn build_industry_structure(
    filter: &RegionalFilter,
    d: &super::fetch::IndustryStructure,
) -> RegionalPanel<IndustryStructureJson> {
    let note = r::industry_structure_note(&d.granularity);
    if !d.has_data || d.rows.is_empty() {
        return no_data(r::INDUSTRY_STRUCTURE_TITLE, "産業構造", filter, note);
    }
    let table = share_table(
        d.rows.iter().map(|x| (x.industry.as_str(), x.employees)),
        d.total,
        None,
    );
    ok(
        r::INDUSTRY_STRUCTURE_TITLE,
        filter,
        note,
        IndustryStructureJson {
            granularity: d.granularity.clone(),
            table,
        },
    )
}

pub(crate) fn build_population_pyramid(
    filter: &RegionalFilter,
    p: &super::fetch::PopulationPyramid,
) -> RegionalPanel<PopulationPyramidJson> {
    let note = r::population_pyramid_note(&p.granularity);
    if !p.has_data || p.bands.is_empty() {
        return no_data(r::POPULATION_PYRAMID_TITLE, "人口ピラミッド", filter, note);
    }
    let bands = r::sorted_bands(&p.bands)
        .into_iter()
        .map(|b| PyramidBandJson {
            age_group: b.age_group.clone(),
            male: b.male_count,
            female: b.female_count,
        })
        .collect();
    ok(
        r::POPULATION_PYRAMID_TITLE,
        filter,
        note,
        PopulationPyramidJson {
            granularity: p.granularity.clone(),
            bands,
        },
    )
}

pub(crate) fn build_wage_comparison(
    filter: &RegionalFilter,
    c: &super::fetch::WageComparison,
) -> RegionalPanel<WageComparisonJson> {
    let note = r::WAGE_COMPARISON_NOTE.to_string();
    if !c.has_data {
        return no_data(r::WAGE_COMPARISON_TITLE, "最低賃金", filter, note);
    }
    let card = stat_card(
        "hourly_min_wage",
        "最低賃金 (都道府県値・時給)".into(),
        c.hourly_min_wage,
        r::fmt_hourly_wage(c.hourly_min_wage),
    );
    ok(
        r::WAGE_COMPARISON_TITLE,
        filter,
        note,
        WageComparisonJson { card },
    )
}

pub(crate) fn build_company_matrix(
    filter: &RegionalFilter,
    points: &[super::fetch::CompanyPoint],
) -> RegionalPanel<CompanyMatrixJson> {
    let note = r::COMPANY_MATRIX_NOTE.to_string();
    if points.is_empty() {
        return no_data(
            r::COMPANY_MATRIX_TITLE,
            "企業成長マトリックス",
            filter,
            note,
        );
    }
    let pts = points
        .iter()
        .map(|p| CompanyPointJson {
            company_name: p.company_name.clone(),
            industry: p.industry.clone(),
            employee_count: p.employee_count,
            employee_count_display: crate::handlers::overview::format_number(p.employee_count),
            growth_rate_1y: p.growth_rate_1y,
            growth_rate_chart: r::round_growth(p.growth_rate_1y),
            growth_rate_display: format!("{:+.1}", p.growth_rate_1y),
        })
        .collect();
    ok(
        r::COMPANY_MATRIX_TITLE,
        filter,
        note,
        CompanyMatrixJson {
            points: pts,
            table_limit: r::COMPANY_MATRIX_TABLE_LIMIT as u32,
            count_note: r::company_matrix_count_note(points.len()),
        },
    )
}

pub(crate) fn build_foreign_residents(
    filter: &RegionalFilter,
    fr: &super::fetch::ForeignResidents,
) -> RegionalPanel<ForeignResidentsJson> {
    let note = r::FOREIGN_RESIDENTS_NOTE.to_string();
    if !fr.has_data || fr.rows.is_empty() {
        return no_data(r::FOREIGN_RESIDENTS_TITLE, "在留外国人", filter, note);
    }
    let table = share_table(
        fr.rows.iter().map(|x| (x.visa_status.as_str(), x.count)),
        fr.total,
        Some(r::FOREIGN_RESIDENTS_CHART_LIMIT as u32),
    );
    ok(
        r::FOREIGN_RESIDENTS_TITLE,
        filter,
        note,
        ForeignResidentsJson {
            survey_period: fr.survey_period.clone(),
            table,
        },
    )
}

pub(crate) fn build_internet_usage(
    filter: &RegionalFilter,
    iu: &super::fetch::InternetUsage,
) -> RegionalPanel<InternetUsageJson> {
    let note = r::INTERNET_USAGE_NOTE.to_string();
    if !iu.has_data {
        return no_data(r::INTERNET_USAGE_TITLE, "インターネット利用", filter, note);
    }
    let year = r::internet_year_label(iu.year);
    let cards = vec![
        stat_card(
            "usage_rate",
            format!("インターネット利用率 {year}"),
            iu.usage_rate,
            r::fmt_pct1(iu.usage_rate),
        ),
        stat_card(
            "smartphone_rate",
            format!("スマートフォン保有率 {year}"),
            iu.smartphone_rate,
            r::fmt_pct1(iu.smartphone_rate),
        ),
    ];
    ok(
        r::INTERNET_USAGE_TITLE,
        filter,
        note,
        InternetUsageJson {
            year: iu.year,
            cards,
        },
    )
}

pub(crate) fn build_occupation(
    filter: &RegionalFilter,
    occ: &super::fetch::OccupationDist,
) -> RegionalPanel<OccupationJson> {
    let note = r::occupation_note(&occ.granularity);
    if !occ.has_data || occ.rows.is_empty() {
        return no_data(r::OCCUPATION_TITLE, "職業別就業者", filter, note);
    }
    let table = share_table(
        occ.rows
            .iter()
            .map(|x| (x.occupation.as_str(), x.population)),
        occ.total,
        None,
    );
    ok(
        r::OCCUPATION_TITLE,
        filter,
        note,
        OccupationJson {
            granularity: occ.granularity.clone(),
            table,
        },
    )
}

// ------------------------------------------------------------------
// ハンドラ
// ------------------------------------------------------------------

/// 取得 (spawn_blocking) → 組み立て。旧ハンドラと同じ分岐順。
async fn run_panel<D, T>(
    state: Arc<AppState>,
    params: RegionalParams,
    title: &'static str,
    needs_company_db: bool,
    fetch: impl FnOnce(&AppState, &RegionalFilter) -> D + Send + 'static,
    build: impl FnOnce(&RegionalFilter, &D) -> RegionalPanel<T>,
) -> Json<RegionalPanel<T>>
where
    D: Send + 'static,
    T: TS,
{
    let filter = params.to_filter();
    if let Some(p) = precheck(&state, title, &filter, needs_company_db) {
        return Json(p);
    }
    let f = filter.clone();
    let data = tokio::task::spawn_blocking(move || fetch(&state, &f))
        .await
        .ok();
    Json(match data {
        Some(d) => build(&filter, &d),
        None => failed(title, &filter),
    })
}

async fn init(State(state): State<Arc<AppState>>) -> Json<RegionalInitResponse> {
    let st = state.clone();
    let prefectures = tokio::task::spawn_blocking(move || fetch_prefectures(&st))
        .await
        .unwrap_or_default();
    Json(RegionalInitResponse { prefectures })
}

async fn municipalities(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalMunicipalitiesResponse> {
    let pref = params.prefecture.unwrap_or_default();
    let st = state.clone();
    let p = pref.clone();
    let municipalities = if pref.is_empty() {
        Vec::new()
    } else {
        tokio::task::spawn_blocking(move || fetch_municipalities(&st, &p))
            .await
            .unwrap_or_default()
    };
    Json(RegionalMunicipalitiesResponse {
        prefecture: pref,
        municipalities,
    })
}

async fn job_openings_ratio(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<JobOpeningsRatioJson>> {
    run_panel(
        state,
        params,
        r::JOB_OPENINGS_RATIO_TITLE,
        false,
        fetch_job_openings_ratio,
        build_job_openings_ratio,
    )
    .await
}

async fn labor_stats(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<LaborStatsJson>> {
    run_panel(
        state,
        params,
        r::LABOR_STATS_TITLE,
        false,
        fetch_labor_stats,
        |f, d| build_labor_stats(f, d.as_ref()),
    )
    .await
}

async fn industry_structure(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<IndustryStructureJson>> {
    run_panel(
        state,
        params,
        r::INDUSTRY_STRUCTURE_TITLE,
        false,
        |s, f| fetch_industry_structure(s, f, INDUSTRY_STRUCTURE_LIMIT),
        build_industry_structure,
    )
    .await
}

async fn population_pyramid(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<PopulationPyramidJson>> {
    run_panel(
        state,
        params,
        r::POPULATION_PYRAMID_TITLE,
        false,
        fetch_population_pyramid,
        build_population_pyramid,
    )
    .await
}

async fn wage_comparison(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<WageComparisonJson>> {
    run_panel(
        state,
        params,
        r::WAGE_COMPARISON_TITLE,
        false,
        fetch_wage_comparison,
        build_wage_comparison,
    )
    .await
}

async fn company_matrix(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<CompanyMatrixJson>> {
    run_panel(
        state,
        params,
        r::COMPANY_MATRIX_TITLE,
        true,
        |s, f| fetch_company_matrix(s, f, COMPANY_MATRIX_LIMIT),
        |f, d| build_company_matrix(f, d),
    )
    .await
}

async fn foreign_residents(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<ForeignResidentsJson>> {
    run_panel(
        state,
        params,
        r::FOREIGN_RESIDENTS_TITLE,
        false,
        fetch_foreign_residents,
        build_foreign_residents,
    )
    .await
}

async fn internet_usage(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<InternetUsageJson>> {
    run_panel(
        state,
        params,
        r::INTERNET_USAGE_TITLE,
        false,
        fetch_internet_usage,
        build_internet_usage,
    )
    .await
}

async fn occupation(
    State(state): State<Arc<AppState>>,
    Query(params): Query<RegionalParams>,
) -> Json<RegionalPanel<OccupationJson>> {
    run_panel(
        state,
        params,
        r::OCCUPATION_TITLE,
        false,
        fetch_occupation_distribution,
        build_occupation,
    )
    .await
}

/// `/api/app/regional/*` のルータ。`protected_routes` に merge する (認証は外側の route_layer)。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/app/regional/init", get(init))
        .route("/api/app/regional/municipalities", get(municipalities))
        .route(
            "/api/app/regional/job_openings_ratio",
            get(job_openings_ratio),
        )
        .route("/api/app/regional/labor_stats", get(labor_stats))
        .route(
            "/api/app/regional/industry_structure",
            get(industry_structure),
        )
        .route(
            "/api/app/regional/population_pyramid",
            get(population_pyramid),
        )
        .route("/api/app/regional/wage_comparison", get(wage_comparison))
        .route("/api/app/regional/company_matrix", get(company_matrix))
        .route(
            "/api/app/regional/foreign_residents",
            get(foreign_residents),
        )
        .route("/api/app/regional/internet_usage", get(internet_usage))
        .route("/api/app/regional/occupation", get(occupation))
}
