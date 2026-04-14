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

    // SalesNow拡張フィールド（44カラム対応）
    pub employee_delta_1m: f64,
    pub employee_delta_3m: f64,
    pub employee_delta_6m: f64,
    pub employee_delta_2y: f64,
    pub capital_stock: i64,
    pub capital_stock_range: String,
    pub salesnow_score: f64,
    pub business_tags: String,
    pub established_date: String,
    pub listing_category: String,
    pub sales_amount: i64,
    pub tob_toc: String,
    pub company_url: String,
    pub group_employee_count: i64,

    pub nearby_companies: Vec<NearbyCompany>,
    pub hw_matched_postings: Vec<Row>,
    pub hw_matched_total_count: i64,

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

    // 地域×業種の人材フロー（SalesNow集計）
    pub region_industry_total_employees: i64,
    pub region_industry_net_change: i64,
    pub region_industry_avg_delta: f64,
    pub region_industry_company_count: i64,

    // 個社 vs 地域の比較
    pub company_vs_region_gap: f64,

    // 自社の給与 vs 市場（月給のみ）
    pub company_avg_salary_min: f64,
    pub company_salary_count: i64,
    pub salary_percentile: f64,

    // 成長シグナル
    pub growth_signal: String,
    pub growth_postings_count: i64,
    pub replacement_postings_count: i64,

    // 採用リスクスコア
    pub hiring_risk_score: f64,
    pub hiring_risk_grade: String,

    // 提案ポイント
    pub sales_pitches: Vec<(String, String)>,
}

/// Turso: 企業名で検索（タイプアヘッド）
pub fn search_companies(turso: &TursoDb, query: &str) -> Vec<Row> {
    if query.trim().is_empty() {
        return vec![];
    }
    let like_pattern = format!("%{}%", query.trim());
    let sql = r#"
        SELECT corporate_number, company_name, prefecture, sn_industry, sn_industry2,
               employee_count, employee_range, sales_range, credit_score,
               salesnow_score, listing_category
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
               prefecture, credit_score, address, postal_code, hubspot_id,
               employee_delta_1m, employee_delta_3m, employee_delta_6m, employee_delta_2y,
               capital_stock, capital_stock_range, salesnow_score,
               business_tags, established_date, listing_category,
               sales_amount, tob_toc, company_url, group_employee_count
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
        employee_delta_1m: get_f64(&row, "employee_delta_1m"),
        employee_delta_3m: get_f64(&row, "employee_delta_3m"),
        employee_delta_6m: get_f64(&row, "employee_delta_6m"),
        employee_delta_2y: get_f64(&row, "employee_delta_2y"),
        capital_stock: get_i64(&row, "capital_stock"),
        capital_stock_range: get_str(&row, "capital_stock_range"),
        salesnow_score: get_f64(&row, "salesnow_score"),
        business_tags: get_str(&row, "business_tags"),
        established_date: get_str(&row, "established_date"),
        listing_category: get_str(&row, "listing_category"),
        sales_amount: get_i64(&row, "sales_amount"),
        tob_toc: get_str(&row, "tob_toc"),
        company_url: get_str(&row, "company_url"),
        group_employee_count: get_i64(&row, "group_employee_count"),
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
    ctx.hw_matched_total_count = count_hw_postings(db, &ctx.company_name, &ctx.prefecture);
    ctx.hw_matched_postings = fetch_hw_postings_for_company(db, &ctx.company_name, &ctx.prefecture);

    // 近隣企業検索（郵便番号上3桁マッチ）
    if !ctx.postal_code.is_empty() {
        ctx.nearby_companies = fetch_nearby_companies(
            sn_db,
            db,
            &ctx.postal_code,
            &ctx.corporate_number,
            &ctx.prefecture,
        );
    }

    // --- クロス分析機能 ---

    // 地域×業種の人材フロー（SalesNow集計）
    if !ctx.prefecture.is_empty() && !ctx.sn_industry.is_empty() {
        fetch_region_industry_flow(sn_db, &mut ctx);
    }

    // 自社の給与 vs 市場
    if !ctx.company_name.is_empty() && !ctx.prefecture.is_empty() {
        fetch_company_salary_analysis(db, &mut ctx);
    }

    // 成長シグナル
    fetch_growth_signal_data(db, &mut ctx);
    ctx.growth_signal = compute_growth_signal(
        ctx.employee_delta_1y,
        ctx.growth_postings_count,
        ctx.replacement_postings_count,
    );

    // 採用リスクスコア
    let (score, grade) = compute_hiring_risk(
        ctx.aging_rate,
        ctx.market_vacancy_rate,
        ctx.salary_percentile,
        ctx.credit_score,
    );
    ctx.hiring_risk_score = score;
    ctx.hiring_risk_grade = grade;

    // 提案ポイント生成
    ctx.sales_pitches = generate_sales_pitches(&ctx);

    Some(ctx)
}

/// HW基本統計（求人数、事業所数、平均給与、正社員率、欠員率）
fn fetch_market_stats(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT COUNT(*) as cnt, \
         COUNT(DISTINCT facility_name) as fac_cnt, \
         AVG(CASE WHEN salary_type = '月給' AND salary_min > 0 THEN salary_min END) as avg_min, \
         AVG(CASE WHEN salary_type = '月給' AND salary_max > 0 THEN salary_max END) as avg_max, \
         SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) as ft_cnt, \
         SUM(CASE WHEN recruitment_reason LIKE '%欠員%' OR recruitment_reason LIKE '%補充%' THEN 1 ELSE 0 END) as vacancy_cnt \
         FROM postings WHERE job_type = ?1 AND prefecture = ?2".to_string();
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
        WHERE job_type = ?1 AND prefecture = ?2 AND salary_min > 0 AND salary_type = '月給' \
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
                    (
                        "退職金".into(),
                        get_i64(r, "b_retirement") as f64 / total * 100.0,
                    ),
                    ("賞与".into(), get_i64(r, "b_bonus") as f64 / total * 100.0),
                    ("昇給".into(), get_i64(r, "b_raise") as f64 / total * 100.0),
                    (
                        "育児休業".into(),
                        get_i64(r, "b_childcare") as f64 / total * 100.0,
                    ),
                    (
                        "社会保険".into(),
                        get_i64(r, "b_insurance") as f64 / total * 100.0,
                    ),
                ];
            }
        }
    }
}

/// 全国平均統計
fn fetch_national_stats(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let sql = "SELECT AVG(salary_min) as avg_min FROM postings WHERE salary_min > 0 AND salary_type = '月給'";
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
    let sql = "SELECT rowid, facility_name, job_type, employment_type, salary_type, \
               salary_min, salary_max, headline, municipality, industry_raw, \
               job_number, working_hours, holidays, benefits, recruitment_reason \
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
        let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&prefecture, &lim];
        sn_db.query(sql, &params).unwrap_or_default()
    };

    // まず企業リストを構築（HWカウントなし）
    let mut companies: Vec<NearbyCompany> = rows
        .iter()
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
        let sql =
            "SELECT COUNT(*) as cnt FROM postings WHERE facility_name LIKE ?1 AND prefecture = ?2";
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
    _prefecture: &str,
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

    rows.iter()
        .map(|r| {
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
        })
        .collect()
}

// ===== クロス分析用の新規fetch関数 =====

/// Turso: 地域×業種の人材フロー集計
fn fetch_region_industry_flow(turso: &TursoDb, ctx: &mut CompanyContext) {
    let sql = r#"
        SELECT COUNT(*) as companies,
               COALESCE(SUM(employee_count), 0) as total_employees,
               COALESCE(SUM(CAST(employee_count * employee_delta_1y / (100.0 + employee_delta_1y) AS INTEGER)), 0) as net_change,
               COALESCE(AVG(employee_delta_1y), 0.0) as avg_delta
        FROM v2_salesnow_companies
        WHERE prefecture = ?1 AND sn_industry = ?2
          AND employee_count > 0 AND employee_delta_1y IS NOT NULL
    "#;
    let pref = &ctx.prefecture;
    let ind = &ctx.sn_industry;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![pref, ind];
    if let Ok(rows) = turso.query(sql, &params) {
        if let Some(r) = rows.first() {
            ctx.region_industry_company_count = get_i64(r, "companies");
            ctx.region_industry_total_employees = get_i64(r, "total_employees");
            ctx.region_industry_net_change = get_i64(r, "net_change");
            ctx.region_industry_avg_delta = get_f64(r, "avg_delta");
            ctx.company_vs_region_gap = ctx.employee_delta_1y - ctx.region_industry_avg_delta;
        }
    }
}

/// SQLite: 自社求人の給与分析（月給のみ）
fn fetch_company_salary_analysis(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let normalized = normalize_company_name(&ctx.company_name);
    if normalized.len() < 2 {
        return;
    }
    let like_pattern = format!("%{}%", normalized);
    let pref = &ctx.prefecture;

    // 自社の平均月給
    let sql = "SELECT AVG(salary_min) as avg_sal, COUNT(*) as cnt \
               FROM postings \
               WHERE facility_name LIKE ?1 AND prefecture = ?2 \
                 AND salary_type = '月給' AND salary_min > 0";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, pref];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            ctx.company_avg_salary_min = get_f64(r, "avg_sal");
            ctx.company_salary_count = get_i64(r, "cnt");
        }
    }

    // 市場内パーセンタイル（primary_hw_job_type × prefecture）
    if ctx.company_avg_salary_min > 0.0 && !ctx.primary_hw_job_type.is_empty() {
        let jt = &ctx.primary_hw_job_type;
        let sal = ctx.company_avg_salary_min;
        let sql_pct = "SELECT COUNT(*) as below \
                       FROM postings \
                       WHERE job_type = ?1 AND prefecture = ?2 \
                         AND salary_type = '月給' AND salary_min > 0 AND salary_min < ?3";
        let params_pct: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref, &sal];
        if let Ok(rows) = db.query(sql_pct, &params_pct) {
            if let Some(r) = rows.first() {
                let below = get_i64(r, "below") as f64;
                // 総数も取得
                let sql_total = "SELECT COUNT(*) as total FROM postings \
                                 WHERE job_type = ?1 AND prefecture = ?2 \
                                   AND salary_type = '月給' AND salary_min > 0";
                let params_total: Vec<&dyn rusqlite::types::ToSql> = vec![jt, pref];
                if let Ok(rows2) = db.query(sql_total, &params_total) {
                    if let Some(r2) = rows2.first() {
                        let total = get_i64(r2, "total") as f64;
                        if total > 0.0 {
                            ctx.salary_percentile = below / total * 100.0;
                        }
                    }
                }
            }
        }
    }
}

/// SQLite: 成長シグナル用の求人理由データ取得
fn fetch_growth_signal_data(db: &crate::db::local_sqlite::LocalDb, ctx: &mut CompanyContext) {
    let normalized = normalize_company_name(&ctx.company_name);
    if normalized.len() < 2 {
        return;
    }
    let like_pattern = format!("%{}%", normalized);
    let pref = &ctx.prefecture;
    let sql = "SELECT \
        SUM(CASE WHEN recruitment_reason LIKE '%増員%' THEN 1 ELSE 0 END) as growth_cnt, \
        SUM(CASE WHEN recruitment_reason LIKE '%欠員%' OR recruitment_reason LIKE '%補充%' THEN 1 ELSE 0 END) as replace_cnt \
        FROM postings \
        WHERE facility_name LIKE ?1 AND prefecture = ?2";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, pref];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            ctx.growth_postings_count = get_i64(r, "growth_cnt");
            ctx.replacement_postings_count = get_i64(r, "replace_cnt");
        }
    }
}

/// 成長シグナル判定（純粋ロジック）
fn compute_growth_signal(delta: f64, growth_cnt: i64, replace_cnt: i64) -> String {
    if delta > 5.0 && growth_cnt > 0 {
        "StrongGrowth".to_string()
    } else if delta > 0.0 && replace_cnt > growth_cnt {
        "Contradictory".to_string()
    } else if delta < -0.5 && (growth_cnt + replace_cnt) == 0 {
        "SilentDecline".to_string()
    } else if delta < -0.5 && replace_cnt > 0 {
        "Declining".to_string()
    } else if delta.abs() <= 0.5 {
        "Stagnant".to_string()
    } else if delta > 0.0 {
        "ModerateGrowth".to_string()
    } else {
        "Stagnant".to_string()
    }
}

/// 採用リスクスコア計算（0-100）
fn compute_hiring_risk(
    aging_rate: f64,
    vacancy_rate: f64,
    salary_percentile: f64,
    credit_score: f64,
) -> (f64, String) {
    // 各要素を0-25のスコアに変換して合算（最大100）
    // 高齢化率: 25%以下=0, 35%以上=25
    let aging_score = ((aging_rate - 25.0).max(0.0) / 10.0 * 25.0).min(25.0);

    // 欠員率: 10%以下=0, 50%以上=25
    let vacancy_score = ((vacancy_rate - 10.0).max(0.0) / 40.0 * 25.0).min(25.0);

    // 給与パーセンタイル: 低いほどリスク高い（下位30%=25, 上位50%=0）
    let salary_score = if salary_percentile > 0.0 {
        ((50.0 - salary_percentile).max(0.0) / 50.0 * 25.0).min(25.0)
    } else {
        12.5 // データなしは中間
    };

    // 与信スコア: 低いほどリスク高い（100=0, 0=25）
    let credit_risk = if credit_score > 0.0 {
        ((100.0 - credit_score) / 100.0 * 25.0).min(25.0)
    } else {
        12.5 // データなしは中間
    };

    let score = (aging_score + vacancy_score + salary_score + credit_risk).min(100.0);
    let grade = match score as i64 {
        0..=20 => "A",
        21..=40 => "B",
        41..=60 => "C",
        61..=80 => "D",
        _ => "F",
    };
    (score, grade.to_string())
}

/// 提案ポイント生成（最大3つ）
fn generate_sales_pitches(ctx: &CompanyContext) -> Vec<(String, String)> {
    let mut pitches = Vec::new();

    // 1. 地域比較（自社 vs 地域平均の従業員変化率）
    if ctx.region_industry_company_count > 0 && ctx.employee_delta_1y != 0.0 {
        if ctx.employee_delta_1y < ctx.region_industry_avg_delta {
            let gap = ctx.region_industry_avg_delta - ctx.employee_delta_1y;
            pitches.push((
                format!(
                    "地域の{}業界は年間{:+}人の変動に対し、御社は{:.1}%の成長率です",
                    ctx.sn_industry, ctx.region_industry_net_change, ctx.employee_delta_1y
                ),
                format!(
                    "{}の{}業界{}社の平均成長率は{:.1}%です。御社は地域平均より{:.1}ポイント下回っています。人材確保の強化が競争力維持に重要です。",
                    ctx.prefecture, ctx.sn_industry, ctx.region_industry_company_count,
                    ctx.region_industry_avg_delta, gap
                ),
            ));
        } else if ctx.employee_delta_1y > ctx.region_industry_avg_delta + 2.0 {
            pitches.push((
                format!(
                    "御社は地域の{}業界平均を{:.1}ポイント上回る成長率です",
                    ctx.sn_industry,
                    ctx.employee_delta_1y - ctx.region_industry_avg_delta
                ),
                format!(
                    "成長に伴う採用ニーズの増加が見込まれます。{}の{}業界全体で{}人が従事しており、質の高い人材の早期確保が重要です。",
                    ctx.prefecture, ctx.sn_industry,
                    ctx.region_industry_total_employees
                ),
            ));
        }
    }

    // 2. 給与ギャップ
    if ctx.company_avg_salary_min > 0.0 && ctx.market_avg_salary_min > 0.0 {
        let gap = ctx.company_avg_salary_min - ctx.market_avg_salary_min;
        if gap < -5000.0 {
            pitches.push((
                format!(
                    "御社の求人給与は市場の下位{:.0}%に位置しています",
                    ctx.salary_percentile
                ),
                format!(
                    "御社の平均月給（下限）は{:.0}円で、市場平均{:.0}円を{:.0}円下回っています。給与改善により応募数増加が見込めます。",
                    ctx.company_avg_salary_min, ctx.market_avg_salary_min, gap.abs()
                ),
            ));
        } else if gap > 10000.0 {
            pitches.push((
                "御社の給与水準は市場上位に位置しています".to_string(),
                format!(
                    "御社の平均月給（下限）{:.0}円は市場平均{:.0}円を{:.0}円上回っており、給与面での競争力は高い状態です。",
                    ctx.company_avg_salary_min, ctx.market_avg_salary_min, gap
                ),
            ));
        }
    }

    // 3. 成長シグナル別の提案
    match ctx.growth_signal.as_str() {
        "Contradictory" => {
            pitches.push((
                "従業員は増加していますが、求人は欠員補充が中心です".to_string(),
                format!(
                    "増員求人{}件に対し欠員補充{}件。従業員数は増えていますが離職が発生しています。定着率改善の提案が有効です。",
                    ctx.growth_postings_count, ctx.replacement_postings_count
                ),
            ));
        }
        "SilentDecline" => {
            pitches.push((
                "従業員数が減少していますが、求人が出ていません".to_string(),
                "人員減少にもかかわらず採用活動が見られません。潜在的な採用ニーズを喚起する提案が有効です。".to_string(),
            ));
        }
        "Declining" => {
            pitches.push((
                format!("従業員数が減少中（{:.1}%）で、欠員補充求人が出ています", ctx.employee_delta_1y),
                format!(
                    "欠員補充求人が{}件出ています。人材定着と早期補充の両面からの支援提案が有効です。",
                    ctx.replacement_postings_count
                ),
            ));
        }
        _ => {}
    }

    // 4. HW未掲載の場合
    if ctx.hw_matched_total_count == 0 && ctx.market_posting_count > 0 {
        pitches.push((
            format!(
                "この地域の{}にはHW求人が{}件ありますが、御社は未掲載です",
                ctx.primary_hw_job_type, ctx.market_posting_count
            ),
            format!(
                "{}の{}業界には{}事業所から求人が出ています。ハローワークへの求人掲載で、新たな求職者層にリーチできます。",
                ctx.prefecture, ctx.primary_hw_job_type, ctx.market_facility_count
            ),
        ));
    }

    // 最大3つに制限
    pitches.truncate(3);
    pitches
}

/// HW求人数カウント（近隣企業用）
pub fn count_hw_postings(
    db: &crate::db::local_sqlite::LocalDb,
    company_name: &str,
    prefecture: &str,
) -> i64 {
    let normalized = normalize_company_name(company_name);
    if normalized.len() < 2 {
        return 0;
    }
    let like_pattern = format!("%{}%", normalized);
    let sql =
        "SELECT COUNT(*) as cnt FROM postings WHERE facility_name LIKE ?1 AND prefecture = ?2";
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![&like_pattern, &prefecture];
    if let Ok(rows) = db.query(sql, &params) {
        if let Some(r) = rows.first() {
            return get_i64(r, "cnt");
        }
    }
    0
}
