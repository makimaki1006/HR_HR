//! 地域分析タブ: 外部統計のみで構成する取得層。
//!
//! ## 設計方針
//! - postings (HW 掲載求人) への依存を完全に排除。
//! - 都道府県/市区町村フィルタは `municipality_code_master` を使用。
//! - 外部統計 (e-Stat / 国勢調査 / 公的統計) を Turso 優先で取得。

use crate::handlers::helpers::{normalize_muni_for_external, strip_county_prefix};
use crate::handlers::competitive::escape_html;
use crate::AppState;

/// 地域フィルタ。
///
/// - `prefecture`: 必須 (空文字なら呼び出し側でデータなし扱い)。
/// - `municipality`: 任意 (空文字なら都道府県全体)。
/// - `job_type`: 業界フィルタ (任意。空文字なら全産業)。
#[derive(Clone, Debug, Default)]
pub(crate) struct RegionalFilter {
    pub prefecture: String,
    pub municipality: String,
    pub job_type: String,
}

impl RegionalFilter {
    /// スコープ表示ラベル (HTML escape 済み)。
    pub(crate) fn scope_label(&self) -> String {
        let area = if !self.municipality.is_empty() {
            format!("{} {}", self.prefecture, self.municipality)
        } else if !self.prefecture.is_empty() {
            self.prefecture.clone()
        } else {
            "未選択".to_string()
        };
        escape_html(&area)
    }
}

// --- 人口ピラミッド ---

/// 人口ピラミッド 1 階級 (5 歳階級別 男女別人口)。
///
/// DISPLAY_SPEC §2: これは実統計 (国勢調査) の「人口」であり表示可。
/// 求人側の求職者人数推定ではない。
pub(crate) struct PyramidBand {
    pub age_group: String,
    pub male_count: i64,
    pub female_count: i64,
}

/// 人口ピラミッド集計結果 (粒度メタ情報付き)。
pub(crate) struct PopulationPyramid {
    pub bands: Vec<PyramidBand>,
    /// 集計粒度ラベル ("市区町村" or "都道府県")。
    pub granularity: String,
    /// 集計対象エリア名 (escape 前)。
    #[allow(dead_code)]
    pub area_name: String,
    pub has_data: bool,
}

/// 最低賃金 (都道府県粒度・時給)。
pub(crate) struct WageComparison {
    /// 最低賃金 (時給, 円)。取得不能時 None。
    pub hourly_min_wage: Option<f64>,
    pub has_data: bool,
}

/// 企業成長マトリックス 1 点 (外部企業データ 1 社)。
///
/// UI には "SalesNow" 固有名を出さない (「外部企業データ」表記)。
pub(crate) struct CompanyPoint {
    pub company_name: String,
    /// 従業員数。
    pub employee_count: i64,
    /// 過去1年の人員増減率 (%)。
    pub growth_rate_1y: f64,
    /// 業種 (外部企業データの業種区分)。
    pub industry: String,
}

// --- 在留外国人 ---

/// 在留外国人 1 行 (在留資格 × 人数)。
#[derive(Clone, Debug)]
pub(crate) struct ForeignResidentRow {
    pub visa_status: String,
    pub count: i64,
}

/// 在留外国人 (都道府県粒度、在留資格別)。
#[derive(Clone, Debug, Default)]
pub(crate) struct ForeignResidents {
    pub rows: Vec<ForeignResidentRow>,
    pub total: i64,
    pub survey_period: String,
    pub has_data: bool,
}

/// インターネット利用 (都道府県粒度)。
#[derive(Clone, Debug, Default)]
pub(crate) struct InternetUsage {
    pub usage_rate: Option<f64>,
    pub smartphone_rate: Option<f64>,
    pub year: Option<i64>,
    pub has_data: bool,
}

/// 職業別就業者 1 行 (職業 × 就業者数)。
#[derive(Clone, Debug)]
pub(crate) struct OccupationRow {
    pub occupation: String,
    pub population: i64,
}

/// 職業別就業者 (市区町村 or 都道府県粒度、従業地ベース実測)。
#[derive(Clone, Debug, Default)]
pub(crate) struct OccupationDist {
    pub rows: Vec<OccupationRow>,
    pub total: i64,
    pub granularity: String,
    pub area_name: String,
    pub has_data: bool,
}

// --- 追加: e-Stat 3 系 ---

/// 有効求人倍率 推移 1 点。
pub(crate) struct JobOpeningsRatioPoint {
    pub year: i64,
    pub ratio: f64,
}

/// 有効求人倍率推移 (都道府県粒度、年度別)。
pub(crate) struct JobOpeningsRatioData {
    pub points: Vec<JobOpeningsRatioPoint>,
    pub has_data: bool,
}

/// 労働統計主要指標 (最新年度 1 行)。
pub(crate) struct LaborStatsRow {
    pub fiscal_year: i64,
    pub unemployment_rate: Option<f64>,
    pub separation_rate: Option<f64>,
    pub monthly_salary_male: Option<f64>,
    pub monthly_salary_female: Option<f64>,
    pub working_hours_male: Option<f64>,
    pub working_hours_female: Option<f64>,
    pub part_time_wage_male: Option<f64>,
    pub part_time_wage_female: Option<f64>,
}

/// 産業構造 1 行 (産業 × 従業者)。
pub(crate) struct IndustryStructureRow {
    pub industry: String,
    pub employees: i64,
}

/// 産業構造集計結果。
pub(crate) struct IndustryStructure {
    pub rows: Vec<IndustryStructureRow>,
    pub total: i64,
    pub granularity: String,
    pub has_data: bool,
}

// --- カスケードフィルタ用一覧取得 ---

/// 都道府県一覧 (municipality_code_master から取得)。
pub(crate) fn fetch_prefectures(state: &AppState) -> Vec<String> {
    // Turso 優先 → ローカル hw_db フォールバック。
    let sql = "SELECT DISTINCT prefecture FROM municipality_code_master \
               WHERE prefecture IS NOT NULL AND prefecture != '' ORDER BY prefecture";
    let rows = query_external(state, sql, &[]);
    rows.iter()
        .filter_map(|r| {
            r.get("prefecture")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// 指定都道府県の市区町村一覧 (municipality_code_master から取得)。
pub(crate) fn fetch_municipalities(state: &AppState, pref: &str) -> Vec<String> {
    if pref.is_empty() {
        return Vec::new();
    }
    let sql = "SELECT DISTINCT municipality_name FROM municipality_code_master \
               WHERE prefecture = ? AND municipality_name IS NOT NULL AND municipality_name != '' \
               ORDER BY municipality_name";
    let rows = query_external(state, sql, &[pref.to_string()]);
    rows.iter()
        .filter_map(|r| {
            r.get("municipality_name")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

// --- 外部統計クエリ共通ヘルパ ---

/// 外部統計クエリ (Turso 優先 → ローカル hw_db フォールバック)。
pub(crate) fn query_external(
    state: &AppState,
    sql: &str,
    params: &[String],
) -> Vec<std::collections::HashMap<String, serde_json::Value>> {
    if let Some(tdb) = state.turso_db.as_ref() {
        let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params
            .iter()
            .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
            .collect();
        match tdb.query(sql, &p) {
            Ok(rows) if !rows.is_empty() => return rows,
            Ok(_) => {}
            Err(e) => {
                tracing::warn!(
                    "regional_analysis Turso external query failed, fallback local: {e}"
                );
            }
        }
    }
    if let Some(db) = state.hw_db.as_ref() {
        let p: Vec<&dyn rusqlite::types::ToSql> = params
            .iter()
            .map(|s| s as &dyn rusqlite::types::ToSql)
            .collect();
        return db.query(sql, &p).unwrap_or_default();
    }
    Vec::new()
}

// --- 既存維持: 人口ピラミッド (国勢調査) ---

/// 人口ピラミッド (v2_external_population_pyramid)。
///
/// 市区町村指定時はその市区町村粒度、未指定時は都道府県集計。
/// turso_db を優先し無ければローカル hw_db にフォールバック。
pub(crate) fn fetch_population_pyramid(
    state: &AppState,
    filter: &RegionalFilter,
) -> PopulationPyramid {
    let empty = |granularity: &str, area: &str| PopulationPyramid {
        bands: Vec::new(),
        granularity: granularity.to_string(),
        area_name: area.to_string(),
        has_data: false,
    };

    if filter.prefecture.is_empty() {
        return empty("都道府県", "");
    }

    let (granularity, area_name, sql, params): (String, String, String, Vec<String>) =
        if !filter.municipality.is_empty() {
            let ext_muni = normalize_muni_for_external(&filter.prefecture, &filter.municipality);
            (
                "市区町村".to_string(),
                format!("{} {}", filter.prefecture, filter.municipality),
                "SELECT age_group, male_count, female_count \
                 FROM v2_external_population_pyramid \
                 WHERE prefecture = ? AND municipality = ?"
                    .to_string(),
                vec![filter.prefecture.clone(), ext_muni],
            )
        } else {
            (
                "都道府県".to_string(),
                filter.prefecture.clone(),
                "SELECT age_group, SUM(male_count) as male_count, \
                 SUM(female_count) as female_count \
                 FROM v2_external_population_pyramid \
                 WHERE prefecture = ? GROUP BY age_group"
                    .to_string(),
                vec![filter.prefecture.clone()],
            )
        };

    let rows = query_external(state, &sql, &params);

    let bands: Vec<PyramidBand> = rows
        .iter()
        .filter_map(|r| {
            let age = r.get("age_group").and_then(|v| v.as_str())?.to_string();
            if age.is_empty() {
                return None;
            }
            let male = r.get("male_count").and_then(|v| v.as_i64()).unwrap_or(0);
            let female = r.get("female_count").and_then(|v| v.as_i64()).unwrap_or(0);
            Some(PyramidBand {
                age_group: age,
                male_count: male,
                female_count: female,
            })
        })
        .filter(|b| b.male_count > 0 || b.female_count > 0)
        .collect();

    if bands.is_empty() {
        return empty(&granularity, &area_name);
    }
    PopulationPyramid {
        bands,
        granularity,
        area_name,
        has_data: true,
    }
}

// --- 既存維持: 最低賃金 (外部統計のみ) ---

/// 最低賃金 (都道府県粒度・時給)。
///
/// postings 依存を削除。最低賃金のみ返す。
pub(crate) fn fetch_wage_comparison(state: &AppState, filter: &RegionalFilter) -> WageComparison {
    if filter.prefecture.is_empty() {
        return WageComparison {
            hourly_min_wage: None,
            has_data: false,
        };
    }

    let wage_sql = "SELECT hourly_min_wage FROM v2_external_minimum_wage WHERE prefecture = ?";
    let wage_rows = query_external(state, wage_sql, &[filter.prefecture.clone()]);
    let hourly_min_wage = wage_rows
        .first()
        .and_then(|r| r.get("hourly_min_wage"))
        .and_then(|v| v.as_f64());

    let has_data = hourly_min_wage.is_some();
    WageComparison {
        hourly_min_wage,
        has_data,
    }
}

// --- 既存維持: 企業成長マトリックス (SalesNow) ---

/// 企業成長マトリックス (外部企業データ = v2_salesnow_companies)。
///
/// UI には "SalesNow" 固有名を出さない (「外部企業データ」)。
pub(crate) fn fetch_company_matrix(
    state: &AppState,
    filter: &RegionalFilter,
    limit: usize,
) -> Vec<CompanyPoint> {
    let sn_db = match state.salesnow_db.as_ref() {
        Some(db) => db,
        None => return Vec::new(),
    };
    if filter.prefecture.is_empty() {
        return Vec::new();
    }

    let (sql, params): (String, Vec<String>) = if !filter.municipality.is_empty() {
        let muni_key = strip_county_prefix(&filter.municipality);
        let muni_pattern = format!("%{}%", muni_key);
        (
            format!(
                "SELECT company_name, employee_count, employee_delta_1y, sn_industry \
                 FROM v2_salesnow_companies \
                 WHERE prefecture = ? AND address LIKE ? \
                   AND employee_count > 0 AND employee_delta_1y IS NOT NULL \
                 ORDER BY employee_count DESC LIMIT {}",
                limit.max(1)
            ),
            vec![filter.prefecture.clone(), muni_pattern],
        )
    } else {
        (
            format!(
                "SELECT company_name, employee_count, employee_delta_1y, sn_industry \
                 FROM v2_salesnow_companies \
                 WHERE prefecture = ? \
                   AND employee_count > 0 AND employee_delta_1y IS NOT NULL \
                 ORDER BY employee_count DESC LIMIT {}",
                limit.max(1)
            ),
            vec![filter.prefecture.clone()],
        )
    };

    let bind: Vec<&dyn crate::db::turso_http::ToSqlTurso> = params
        .iter()
        .map(|s| s as &dyn crate::db::turso_http::ToSqlTurso)
        .collect();
    let rows = match sn_db.query(&sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::warn!("regional_analysis company matrix query failed: {e}");
            return Vec::new();
        }
    };

    rows.iter()
        .filter_map(|r| {
            let name = r.get("company_name").and_then(|v| v.as_str())?.to_string();
            let emp = r
                .get("employee_count")
                .and_then(|v| v.as_i64())
                .unwrap_or(0);
            if emp <= 0 {
                return None;
            }
            let growth = r.get("employee_delta_1y").and_then(|v| v.as_f64())?;
            let industry = r
                .get("sn_industry")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            Some(CompanyPoint {
                company_name: name,
                employee_count: emp,
                growth_rate_1y: growth,
                industry,
            })
        })
        .collect()
}

// --- 既存維持: 在留外国人 ---

/// 在留外国人を取得 (都道府県値・在留資格別降順)。
///
/// 出典: SSDSE-A (住民基本台帳)。
pub(crate) fn fetch_foreign_residents(
    state: &AppState,
    filter: &RegionalFilter,
) -> ForeignResidents {
    if filter.prefecture.is_empty() {
        return ForeignResidents::default();
    }
    let sql = "SELECT visa_status, count, survey_period \
               FROM v2_external_foreign_residents \
               WHERE prefecture = ? AND count > 0 \
                 AND visa_status IS NOT NULL AND visa_status <> '' \
                 AND visa_status NOT LIKE '%総数%' AND visa_status NOT LIKE '%総計%' \
                 AND visa_status NOT LIKE '%合計%' \
               ORDER BY count DESC";
    let rows = query_external(state, sql, &[filter.prefecture.clone()]);
    let mut out = Vec::new();
    let mut period = String::new();
    for r in &rows {
        let vs = r
            .get("visa_status")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let cnt = r.get("count").and_then(|v| v.as_i64()).unwrap_or(0);
        if vs.is_empty() || cnt <= 0 {
            continue;
        }
        if period.is_empty() {
            if let Some(p) = r.get("survey_period").and_then(|v| v.as_str()) {
                period = p.to_string();
            }
        }
        out.push(ForeignResidentRow {
            visa_status: vs,
            count: cnt,
        });
    }
    let total: i64 = out.iter().map(|r| r.count).sum();
    let has_data = !out.is_empty();
    ForeignResidents {
        rows: out,
        total,
        survey_period: period,
        has_data,
    }
}

/// インターネット利用率・スマートフォン保有率を取得 (都道府県値)。
///
/// 出典: 通信利用動向。
pub(crate) fn fetch_internet_usage(state: &AppState, filter: &RegionalFilter) -> InternetUsage {
    if filter.prefecture.is_empty() {
        return InternetUsage::default();
    }
    let sql = "SELECT internet_usage_rate, smartphone_ownership_rate, year \
               FROM v2_external_internet_usage WHERE prefecture = ?";
    let rows = query_external(state, sql, &[filter.prefecture.clone()]);
    match rows.first() {
        Some(r) => {
            let usage_rate = r.get("internet_usage_rate").and_then(|v| v.as_f64());
            let smartphone_rate = r.get("smartphone_ownership_rate").and_then(|v| v.as_f64());
            let year = r.get("year").and_then(|v| v.as_i64());
            let has_data = usage_rate.is_some() || smartphone_rate.is_some();
            InternetUsage {
                usage_rate,
                smartphone_rate,
                year,
                has_data,
            }
        }
        None => InternetUsage::default(),
    }
}

/// 職業別就業者を取得。
///
/// `data_label='measured' AND basis='workplace'` (国勢調査・従業地ベース実測) のみ集計。
pub(crate) fn fetch_occupation_distribution(
    state: &AppState,
    filter: &RegionalFilter,
) -> OccupationDist {
    if filter.prefecture.is_empty() {
        return OccupationDist::default();
    }
    let (granularity, area_name, sql, params): (String, String, String, Vec<String>) =
        if !filter.municipality.is_empty() {
            let ext_muni = normalize_muni_for_external(&filter.prefecture, &filter.municipality);
            (
                "市区町村".to_string(),
                format!("{} {}", filter.prefecture, filter.municipality),
                "SELECT occupation_name, SUM(population) AS pop \
                 FROM municipality_occupation_population \
                 WHERE prefecture = ? AND municipality_name = ? \
                   AND data_label = 'measured' AND basis = 'workplace' \
                   AND population IS NOT NULL \
                 GROUP BY occupation_name ORDER BY pop DESC"
                    .to_string(),
                vec![filter.prefecture.clone(), ext_muni],
            )
        } else {
            (
                "都道府県".to_string(),
                filter.prefecture.clone(),
                "SELECT occupation_name, SUM(population) AS pop \
                 FROM municipality_occupation_population \
                 WHERE prefecture = ? \
                   AND data_label = 'measured' AND basis = 'workplace' \
                   AND population IS NOT NULL \
                 GROUP BY occupation_name ORDER BY pop DESC"
                    .to_string(),
                vec![filter.prefecture.clone()],
            )
        };
    let rows = query_external(state, &sql, &params);
    let mut out = Vec::new();
    for r in &rows {
        let occ = r
            .get("occupation_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let pop = r.get("pop").and_then(|v| v.as_i64()).unwrap_or(0);
        if occ.is_empty() || pop <= 0 {
            continue;
        }
        out.push(OccupationRow {
            occupation: occ,
            population: pop,
        });
    }
    let total: i64 = out.iter().map(|r| r.population).sum();
    let has_data = !out.is_empty();
    OccupationDist {
        rows: out,
        total,
        granularity,
        area_name,
        has_data,
    }
}

// ============================================================
// 追加: e-Stat 3 系
// ============================================================

/// 有効求人倍率推移 (v2_external_job_openings_ratio)。
///
/// 出典: e-Stat 政府統計コード 00450091 (一般職業紹介状況)。
/// 都道府県未選択時は空を返す。
pub(crate) fn fetch_job_openings_ratio(
    state: &AppState,
    filter: &RegionalFilter,
) -> JobOpeningsRatioData {
    if filter.prefecture.is_empty() {
        return JobOpeningsRatioData {
            points: Vec::new(),
            has_data: false,
        };
    }
    let sql = "SELECT fiscal_year, ratio_total FROM v2_external_job_openings_ratio \
               WHERE prefecture = ? ORDER BY fiscal_year";
    let rows = query_external(state, sql, &[filter.prefecture.clone()]);
    let points: Vec<JobOpeningsRatioPoint> = rows
        .iter()
        .filter_map(|r| {
            let year = r.get("fiscal_year").and_then(|v| v.as_i64())?;
            let ratio = r.get("ratio_total").and_then(|v| v.as_f64())?;
            Some(JobOpeningsRatioPoint { year, ratio })
        })
        .collect();
    let has_data = !points.is_empty();
    JobOpeningsRatioData { points, has_data }
}

/// 労働統計主要指標 (v2_external_labor_stats、最新年度 1 行)。
///
/// 出典: e-Stat 社会人口統計体系 / 労働政策研究・研修機構。
/// 取得カラム: 完全失業率・離職率・月収(男女)・所定内労働時間(男女)・パート時給(男女)。
pub(crate) fn fetch_labor_stats(
    state: &AppState,
    filter: &RegionalFilter,
) -> Option<LaborStatsRow> {
    if filter.prefecture.is_empty() {
        return None;
    }
    let sql = "SELECT fiscal_year, unemployment_rate, separation_rate, \
               monthly_salary_male, monthly_salary_female, \
               working_hours_male, working_hours_female, \
               part_time_wage_male, part_time_wage_female \
               FROM v2_external_labor_stats WHERE prefecture = ? \
               ORDER BY fiscal_year DESC LIMIT 1";
    let rows = query_external(state, sql, &[filter.prefecture.clone()]);
    rows.first().map(|r| {
        let gf64 = |key: &str| r.get(key).and_then(|v| v.as_f64());
        let gi64 = |key: &str| r.get(key).and_then(|v| v.as_i64()).unwrap_or(0);
        LaborStatsRow {
            fiscal_year: gi64("fiscal_year"),
            unemployment_rate: gf64("unemployment_rate"),
            separation_rate: gf64("separation_rate"),
            monthly_salary_male: gf64("monthly_salary_male"),
            monthly_salary_female: gf64("monthly_salary_female"),
            working_hours_male: gf64("working_hours_male"),
            working_hours_female: gf64("working_hours_female"),
            part_time_wage_male: gf64("part_time_wage_male"),
            part_time_wage_female: gf64("part_time_wage_female"),
        }
    })
}

/// 産業構造 (v2_external_industry_structure)。
///
/// 市区町村指定時は city_name 完全一致、未指定時は都道府県集計。
/// 集計不能コード (AS/AR/CR/AB/D) は除外。
/// 出典: 総務省統計局 国勢調査 (経済センサス)。
pub(crate) fn fetch_industry_structure(
    state: &AppState,
    filter: &RegionalFilter,
    limit: usize,
) -> IndustryStructure {
    if filter.prefecture.is_empty() {
        return IndustryStructure {
            rows: Vec::new(),
            total: 0,
            granularity: "都道府県".to_string(),
            has_data: false,
        };
    }

    let (granularity, sql, params): (String, String, Vec<String>) =
        if !filter.municipality.is_empty() {
            let ext_muni = normalize_muni_for_external(&filter.prefecture, &filter.municipality);
            (
                "市区町村".to_string(),
                format!(
                    "SELECT industry_name, SUM(employees_total) AS employees \
                     FROM v2_external_industry_structure \
                     WHERE city_name = ? \
                       AND industry_code NOT IN ('AS','AR','CR','AB','D') \
                     GROUP BY industry_name ORDER BY employees DESC LIMIT {}",
                    limit.max(1)
                ),
                vec![ext_muni],
            )
        } else {
            // pref_name_to_code を使用して prefecture_code で集計。
            let map = crate::geo::pref_name_to_code();
            match map.get(filter.prefecture.as_str()) {
                Some(code) => (
                    "都道府県".to_string(),
                    format!(
                        "SELECT industry_name, SUM(employees_total) AS employees \
                         FROM v2_external_industry_structure \
                         WHERE prefecture_code = ? \
                           AND industry_code NOT IN ('AS','AR','CR','AB','D') \
                         GROUP BY industry_name ORDER BY employees DESC LIMIT {}",
                        limit.max(1)
                    ),
                    vec![code.to_string()],
                ),
                None => {
                    tracing::warn!(
                        "regional_analysis industry_structure: unknown prefecture '{}'",
                        filter.prefecture
                    );
                    return IndustryStructure {
                        rows: Vec::new(),
                        total: 0,
                        granularity: "都道府県".to_string(),
                        has_data: false,
                    };
                }
            }
        };

    let rows = query_external(state, &sql, &params);
    let mut out: Vec<IndustryStructureRow> = Vec::new();
    for r in &rows {
        let industry = r
            .get("industry_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let emp = r.get("employees").and_then(|v| v.as_i64()).unwrap_or(0);
        if industry.is_empty() || emp <= 0 {
            continue;
        }
        out.push(IndustryStructureRow {
            industry,
            employees: emp,
        });
    }
    let total: i64 = out.iter().map(|r| r.employees).sum();
    let has_data = !out.is_empty();
    IndustryStructure {
        rows: out,
        total,
        granularity,
        has_data,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scope_label_pref_only() {
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "".into(),
        };
        assert_eq!(f.scope_label(), "東京都");
    }

    #[test]
    fn scope_label_with_muni() {
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "新宿区".into(),
            job_type: "".into(),
        };
        assert_eq!(f.scope_label(), "東京都 新宿区");
    }

    #[test]
    fn scope_label_empty_is_unselected() {
        let f = RegionalFilter::default();
        assert_eq!(f.scope_label(), "未選択");
    }

    #[test]
    fn scope_label_escapes_xss() {
        let f = RegionalFilter {
            prefecture: "<script>x</script>".into(),
            municipality: "".into(),
            job_type: "".into(),
        };
        assert!(!f.scope_label().contains("<script>"));
    }
}
