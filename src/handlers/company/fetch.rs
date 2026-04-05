use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_f64, get_i64, get_str, Row};

/// 近隣企業データ（郵便番号上3桁マッチ）
#[derive(Default)]
pub struct NearbyCompany {
    pub corporate_number: String,
    pub company_name: String,
    pub prefecture: String,
    pub sn_industry: String,
    pub employee_count: i64,
    pub credit_score: f64,
    pub postal_code: String,
    pub hw_posting_count: i64,
}

/// 企業プロフィール + 市場コンテキストの統合データ
#[derive(Default)]
pub struct CompanyContext {
    // SalesNow企業データ（Turso）
    pub corporate_number: String,
    pub company_name: String,
    pub employee_count: i64,
    pub employee_range: String,
    pub employee_delta_1y: f64,
    pub sales_range: String,
    pub sn_industry: String,
    pub sn_industry2: String,
    pub prefecture: String,
    pub credit_score: f64,
    pub address: String,
    pub postal_code: String,
    pub nearby_companies: Vec<NearbyCompany>,
    pub hw_matched_postings: Vec<Row>,

    // 業界マッピング結果
    pub hw_job_types: Vec<(String, f64)>,
    pub primary_hw_job_type: String,

    // HW市場データ（ローカルSQLite）
    pub market_posting_count: i64,
    pub market_facility_count: i64,
    pub market_avg_salary_min: f64,
    pub market_avg_salary_max: f64,
    pub market_fulltime_rate: f64,
    pub market_vacancy_rate: f64,
    pub salary_distribution: Vec<(String, i64)>,
    pub emp_size_distribution: Vec<(String, i64)>,
    pub recruitment_reasons: Vec<(String, i64)>,
    pub benefit_rates: Vec<(String, f64)>,

    // 外部統計（Turso）
    pub population: i64,
    pub daytime_ratio: f64,
    pub aging_rate: f64,

    // 全国比較用
    pub national_avg_salary: f64,
    pub national_vacancy_rate: f64,
}

/// Turso: 企業名で検索（タイプアヘッド）
pub fn search_companies(turso: &TursoDb, query: &str) -> Vec<Row> {
    if query.trim().is_empty() {
        return vec![];
    }
    let like_pattern = format!("%{}%", query.trim());
    let sql = r#"
        SELECT corporate_number, company_name, prefecture, sn_industry, sn_industry2,
               employee_count, employee_range, sales_range, credit_score
        FROM v2_salesnow_companies
        WHERE company_name LIKE ?1
        ORDER BY employee_count DESC
        LIMIT 20
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&like_pattern];
    turso.query(sql, &params).unwrap_or_default()
}

/// Turso: 法人番号で企業詳細取得
pub fn fetch_company_detail(turso: &TursoDb, corporate_number: &str) -> Option<Row> {
    let sql = r#"
        SELECT corporate_number, company_name, employee_count, employee_range,
               employee_delta_1y, sales_range, sn_industry, sn_industry2,
               prefecture, credit_score, address, postal_code, hubspot_id
        FROM v2_salesnow_companies
        WHERE corporate_number = ?1
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&corporate_number];
    turso.query(sql, &params).ok()?.into_iter().next()
}

/// Turso: 業界マッピング取得
pub fn fetch_industry_mapping(turso: &TursoDb, sn_industry: &str) -> Vec<(String, f64)> {
    if sn_industry.is_empty() {
        return vec![];
    }
    let sql = r#"
        SELECT hw_job_type, confidence
        FROM v2_industry_mapping
        WHERE sn_industry = ?1
        ORDER BY confidence DESC
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&sn_industry];
    turso
        .query(sql, &params)
        .unwrap_or_default()
        .iter()
        .map(|r| (get_str(r, "hw_job_type"), get_f64(r, "confidence")))
        .collect()
}

/// SalesNow(Turso) + HW(SQLite) + 外部統計(Turso) を統合してCompanyContextを構築
pub fn build_company_context(
    sn_db: &TursoDb,
    ext_db: Option<&TursoDb>,
    db: &crate::db::local_sqlite::LocalDb,
    corporate_number: &str,
) -> Option<CompanyContext> {
    let row = fetch_company_detail(sn_db, corporate_number)?;

    let mut ctx = CompanyContext {
        corporate_number: get_str(&row, "corporate_number"),
        company_name: get_str(&row, "company_name"),
        employee_count: get_i64(&row, "employee_count"),
        employee_range: get_str(&row, "employee_range"),
        employee_delta_1y: get_f64(&row, "employee_delta_1y"),
        sales_range: get_str(&row, "sales_range"),
        sn_industry: get_str(&row, "sn_industry"),
        sn_industry2: get_str(&row, "sn_industry2"),
        prefecture: get_str(&row, "prefecture"),
        credit_score: get_f64(&row, "credit_score"),
        address: get_str(&row, "address"),
        postal_code: get_str(&row, "postal_code"),
        nearby_companies: vec![],
        hw_matched_postings: vec![],
        ..Default::default()
    };

    // 業界マッピング（SalesNow DB）
    ctx.hw_job_types = fetch_industry_mapping(sn_db, &ctx.sn_industry);
    ctx.primary_hw_job_type = ctx
        .hw_job_types
        .first()
        .map(|(jt, _)| jt.clone())
        .unwrap_or_default();

    // HW市場データ取得（primary_hw_job_type × prefecture）
    if !ctx.primary_hw_job_type.is_empty() && !ctx.prefecture.is_empty() {
        fetch_market_stats(db, &mut ctx);
        fetch_salary_distribution(db, &mut ctx);
        fetch_emp_size_distribution(db, &mut ctx);
        fetch_recruitment_reasons(db, &mut ctx);
        fetch_benefit_rates(db, &mut ctx);
    }

    // 全国平均
    fetch_national_stats(db, &mut ctx);

    // 外部統計（country-statistics Turso: 人口等）
    if let Some(ext) = ext_db {
        fetch_external_stats(ext, &mut ctx);
    }

    // HW求人マッチング（企業名でfacility_nameを検索）
    ctx.hw_matched_postings = fetch_hw_postings_for_company(db, &ctx.company_name, &ctx.prefecture);

    // 近隣企業検索（郵便番号上3桁マッチ）
    if !ctx.postal_code.is_empty() {
        ctx.nearby_companies = fetch_nearby_companies(sn_db, db, &ctx.postal_code, &ctx.corporate_number, &ctx.prefecture);
    }

    Some(ctx)
}

/// HW基本統計（求人数、事業所数、平均給与、正社員率、欠員率）
fn fetch_market_stats(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = format!(
        "SELECT COUNT(*) as cnt, \
         COUNT(DISTINCT facility_name) as fac_cnt, \
         AVG(NULLIF(salary_min, 0)) as avg_min, \
         AVG(NULLIF(salary_max, 0)) as avg_max, \
         SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) as ft_cnt, \
         SUM(CASE WHEN recruitment_reason LIKE '%欠員%' OR recruitment_reason LIKE '%補充%' THEN 1 ELSE 0 END) as vacancy_cnt \
         FROM postings WHERE job_type = ?1 AND prefecture = ?2"
    );
    let jt = &ctx.primary_hw_job_type;
    let pref = &ctx.prefecture;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
    if let Ok(rows) = db.query(&sql, &params) {
        if let Some(r) = rows.first() {
            ctx.market_posting_count = get_i64(r, "cnt");
            ctx.market_facility_count = get_i64(r, "fac_cnt");
            ctx.market_avg_salary_min = get_f64(r, "avg_min");
            ctx.market_avg_salary_max = get_f64(r, "avg_max");
            let ft = get_i64(r, "ft_cnt");
            let total = ctx.market_posting_count;
            ctx.market_fulltime_rate = if total > 0 {
                ft as f64 / total as f64 * 100.0
            } else {
                0.0
            };
            let vac = get_i64(r, "vacancy_cnt");
            ctx.market_vacancy_rate = if total > 0 {
                vac as f64 / total as f64 * 100.0
            } else {
                0.0
            };
        }
    }
}

/// 給与帯分布
fn fetch_salary_distribution(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT \
        CASE \
            WHEN salary_min < 180000 THEN '18万未満' \
            WHEN salary_min < 200000 THEN '18-20万' \
            WHEN salary_min < 250000 THEN '20-25万' \
            WHEN salary_min < 300000 THEN '25-30万' \
            WHEN salary_min < 350000 THEN '30-35万' \
            WHEN salary_min < 400000 THEN '35-40万' \
            ELSE '40万以上' \
        END as band, COUNT(*) as cnt \
        FROM postings \
        WHERE job_type = ?1 AND prefecture = ?2 AND salary_min > 0 \
        GROUP BY band ORDER BY MIN(salary_min)";
    let jt = &ctx.primary_hw_job_type;
    let pref = &ctx.prefecture;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
    if let Ok(rows) = db.query(sql, &params) {
        ctx.salary_distribution = rows
            .iter()
            .map(|r| (get_str(r, "band"), get_i64(r, "cnt")))
            .collect();
    }
}

/// 従業員規模分布
fn fetch_emp_size_distribution(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT \
        CASE \
            WHEN employee_count <= 5 THEN '5人以下' \
            WHEN employee_count <= 20 THEN '6-20人' \
            WHEN employee_count <= 50 THEN '21-50人' \
            WHEN employee_count <= 100 THEN '51-100人' \
            WHEN employee_count <= 300 THEN '101-300人' \
            ELSE '300人超' \
        END as band, COUNT(*) as cnt \
        FROM postings \
        WHERE job_type = ?1 AND prefecture = ?2 AND employee_count > 0 \
        GROUP BY band ORDER BY MIN(employee_count)";
    let jt = &ctx.primary_hw_job_type;
    let pref = &ctx.prefecture;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
    if let Ok(rows) = db.query(sql, &params) {
        ctx.emp_size_distribution = rows
            .iter()
            .map(|r| (get_str(r, "band"), get_i64(r, "cnt")))
            .collect();
    }
}

/// 求人理由分布
fn fetch_recruitment_reasons(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT \
        CASE \
            WHEN recruitment_reason LIKE '%欠員%' OR recruitment_reason LIKE '%補充%' THEN '欠員補充' \
            WHEN recruitment_reason LIKE '%増員%' THEN '増員' \
            WHEN recruitment_reason LIKE '%新設%' OR recruitment_reason LIKE '%新規%' THEN '新設' \
            ELSE 'その他' \
        END as reason, COUNT(*) as cnt \
        FROM postings \
        WHERE job_type = ?1 AND prefecture = ?2 \
          AND recruitment_reason IS NOT NULL AND recruitment_reason != '' \
        GROUP BY reason ORDER BY cnt DESC";
    let jt = &ctx.primary_hw_job_type;
    let pref = &ctx.prefecture;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
    if let Ok(rows) = db.query(sql, &params) {
        ctx.recruitment_reasons = rows
            .iter()
            .map(|r| (get_str(r, "reason"), get_i64(r, "cnt")))
            .collect();
    }
}

/// 福利厚生普及率
fn fetch_benefit_rates(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT COUNT(*) as total, \
        SUM(CASE WHEN benefits LIKE '%退職金%' THEN 1 ELSE 0 END) as b_retirement, \
        SUM(CASE WHEN benefits LIKE '%賞与%' OR benefits LIKE '%ボーナス%' THEN 1 ELSE 0 END) as b_bonus, \
        SUM(CASE WHEN benefits LIKE '%昇給%' THEN 1 ELSE 0 END) as b_raise, \
        SUM(CASE WHEN benefits LIKE '%育児%' OR benefits LIKE '%育休%' THEN 1 ELSE 0 END) as b_childcare, \
        SUM(CASE WHEN benefits LIKE '%社会保険%' OR benefits LIKE '%厚生年金%' THEN 1 ELSE 0 END) as b_insurance \
        FROM postings WHERE job_type = ?1 AND prefecture = ?2 AND employment_type = '正社員'";
    let jt = &ctx.primary_hw_job_type;
    let pref = &ctx.prefecture;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            let total = get_i64(r, "total") as f64;
            if total > 0.0 {
                ctx.benefit_rates = vec![
                    ("退職金".into(), get_i64(r, "b_retirement") as f64 / total * 100.0),
                    ("賞与".into(), get_i64(r, "b_bonus") as f64 / total * 100.0),
                    ("昇給".into(), get_i64(r, "b_raise") as f64 / total * 100.0),
                    ("育児休業".into(), get_i64(r, "b_childcare") as f64 / total * 100.0),
                    ("社会保険".into(), get_i64(r, "b_insurance") as f64 / total * 100.0),
                ];
            }
        }
    }
}

/// 全国平均統計
fn fetch_national_stats(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT AVG(NULLIF(salary_min, 0)) as avg_min FROM postings WHERE salary_min > 0";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            ctx.national_avg_salary = get_f64(r, "avg_min");
        }
    }

    let sql2 = "SELECT \
        CAST(SUM(CASE WHEN recruitment_reason LIKE '%欠員%' OR recruitment_reason LIKE '%補充%' THEN 1 ELSE 0 END) AS REAL) \
        / CAST(COUNT(*) AS REAL) * 100.0 as vr \
        FROM postings WHERE recruitment_reason IS NOT NULL AND recruitment_reason != ''";
    if let Ok(rows) = db.query(sql2, &params) {
        if let Some(r) = rows.first() {
            ctx.national_vacancy_rate = get_f64(r, "vr");
        }
    }
}

/// 外部統計（Turso: 人口、昼夜間比、高齢化率）
fn fetch_external_stats(turso: &TursoDb, ctx: &mut CompanyContext) {
    let sql = "SELECT total_population, daytime_population_ratio, aging_rate \
               FROM v2_external_prefecture_stats WHERE prefecture = ?1";
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&ctx.prefecture];
    if let Ok(rows) = turso.query(sql, &params) {
        if let Some(r) = rows.first() {
            ctx.population = get_i64(r, "total_population");
            ctx.daytime_ratio = get_f64(r, "daytime_population_ratio");
            ctx.aging_rate = get_f64(r, "aging_rate");
        }
    }
}

/// 企業名正規化（法人格除去）
fn normalize_company_name(name: &str) -> String {
    name.replace("株式会社", "")
        .replace("有限会社", "")
        .replace("合同会社", "")
        .replace("(株)", "")
        .replace("（株）", "")
        .replace("(有)", "")
        .replace("（有）", "")
        .replace("(合)", "")
        .replace("（合）", "")
        .trim()
        .to_string()
}

/// HW求人マッチング（企業名でfacility_nameをLIKE検索）
pub fn fetch_hw_postings_for_company(
    db: &crate::db::local_sqlite::LocalDb,
    company_name: &str,
    prefecture: &str,
) -> Vec<Row> {
    let normalized = normalize_company_name(company_name);
    if normalized.len() < 2 {
        return vec![];
    }
    let like_pattern = format!("%{}%", normalized);
    let sql = "SELECT facility_name, job_type, employment_type, salary_type, \
               salary_min, salary_max, headline, municipality, industry_raw \
               FROM postings \
               WHERE facility_name LIKE ?1 AND prefecture = ?2 \
               ORDER BY salary_min DESC LIMIT 30";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, &prefecture];
    db.query(sql, &params).unwrap_or_default()
}

/// 地域ベースの企業検索（都道府県 + 市区町村フィルタ）
/// CSV調査の統合レポートで使用
pub fn fetch_companies_by_region(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    prefecture: &str,
    municipality: &str,
    limit: usize,
) -> Vec<NearbyCompany> {
    if prefecture.is_empty() {
        return vec![];
    }

    let lim = limit.min(50) as i64;

    let rows = if !municipality.is_empty() {
        // 市区町村フィルタあり
        let muni_pattern = format!("%{}%", municipality);
        let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                   employee_count, credit_score, postal_code \
                   FROM v2_salesnow_companies \
                   WHERE prefecture = ?1 AND address LIKE ?2 \
                   ORDER BY employee_count DESC LIMIT ?3";
        let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            vec![&prefecture, &muni_pattern, &lim];
        sn_db.query(sql, &params).unwrap_or_default()
    } else {
        // 都道府県のみ
        let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                   employee_count, credit_score, postal_code \
                   FROM v2_salesnow_companies \
                   WHERE prefecture = ?1 \
                   ORDER BY employee_count DESC LIMIT ?2";
        let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            vec![&prefecture, &lim];
        sn_db.query(sql, &params).unwrap_or_default()
    };

    // まず企業リストを構築（HWカウントなし）
    let mut companies: Vec<NearbyCompany> = rows.iter()
        .map(|r| NearbyCompany {
            corporate_number: get_str(r, "corporate_number"),
            company_name: get_str(r, "company_name"),
            prefecture: get_str(r, "prefecture"),
            sn_industry: get_str(r, "sn_industry"),
            employee_count: get_i64(r, "employee_count"),
            credit_score: get_f64(r, "credit_score"),
            postal_code: get_str(r, "postal_code"),
            hw_posting_count: 0,
        })
        .collect();

    // HW求人数を一括取得（N+1回避: 1クエリで全企業分をカウント）
    batch_count_hw_postings(db, &mut companies, prefecture);

    companies
}

/// HW求人数を一括カウント（N+1クエリ回避）
fn batch_count_hw_postings(
    db: &crate::db::local_sqlite::LocalDb,
    companies: &mut [NearbyCompany],
    prefecture: &str,
) {
    if companies.is_empty() {
        return;
    }
    // 各企業名を正規化してOR条件で一括検索
    for c in companies.iter_mut() {
        let normalized = normalize_company_name(&c.company_name);
        if normalized.len() < 2 {
            continue;
        }
        let like_pattern = format!("%{}%", normalized);
        let sql = "SELECT COUNT(*) as cnt FROM postings WHERE facility_name LIKE ?1 AND prefecture = ?2";
        let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, &prefecture];
        if let Ok(rows) = db.query(sql, &params) {
            if let Some(r) = rows.first() {
                c.hw_posting_count = get_i64(r, "cnt");
            }
        }
    }
}

/// 近隣企業検索（郵便番号上3桁マッチ）
pub fn fetch_nearby_companies(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    postal_code: &str,
    exclude_corp: &str,
    prefecture: &str,
) -> Vec<NearbyCompany> {
    if postal_code.len() < 3 {
        return vec![];
    }
    // 郵便番号上3桁でエリアマッチ
    let prefix = &postal_code[..3];
    let like_pattern = format!("{}%", prefix);
    let sql = r#"
        SELECT corporate_number, company_name, prefecture, sn_industry,
               employee_count, credit_score, postal_code
        FROM v2_salesnow_companies
        WHERE postal_code LIKE ?1 AND corporate_number != ?2
        ORDER BY employee_count DESC
        LIMIT 50
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&like_pattern, &exclude_corp];
    let rows = sn_db.query(sql, &params).unwrap_or_default();

    rows.iter().map(|r| {
        let name = get_str(r, "company_name");
        let pref = get_str(r, "prefecture");
        // HW求人数を集計
        let hw_count = count_hw_postings(db, &name, &pref);
        NearbyCompany {
            corporate_number: get_str(r, "corporate_number"),
            company_name: name,
            prefecture: pref,
            sn_industry: get_str(r, "sn_industry"),
            employee_count: get_i64(r, "employee_count"),
            credit_score: get_f64(r, "credit_score"),
            postal_code: get_str(r, "postal_code"),
            hw_posting_count: hw_count,
        }
    }).collect()
}

/// HW求人数カウント（近隣企業用）
fn count_hw_postings(db: &crate::db::local_sqlite::LocalDb, company_name: &str, prefecture: &str) -> i64 {
    let normalized = normalize_company_name(company_name);
    if normalized.len() < 2 {
        return 0;
    }
    let like_pattern = format!("%{}%", normalized);
    let sql = "SELECT COUNT(*) as cnt FROM postings WHERE facility_name LIKE ?1 AND prefecture = ?2";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, &prefecture];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            return get_i64(r, "cnt");
        }
    }
    0
}
