use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::{get_f64, get_i64, get_str, Row};

/// 近隣企業データ（郵便番号上3桁マッチ）
///
/// 2026-04-24 拡張: 媒体分析タブ/PDF の表示項目刷新のため以下を追加:
/// - sales_amount (f64): 推定売上金額
/// - sales_range (String): 売上レンジ分類ラベル
/// - employee_delta_1y (f64): 過去1年の人員増減率 (%)
/// - employee_delta_3m (f64): 過去3ヶ月の人員増減率 (%)
/// (credit_score は struct には保持するが UI 表示から除外)
#[derive(Debug, Default, Clone)]
pub struct NearbyCompany {
    pub corporate_number: String,
    pub company_name: String,
    pub prefecture: String,
    pub sn_industry: String,
    pub employee_count: i64,
    pub credit_score: f64,
    pub postal_code: String,
    pub hw_posting_count: i64,
    pub sales_amount: f64,
    pub sales_range: String,
    pub employee_delta_1y: f64,
    pub employee_delta_3m: f64,
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

    // 提案%ポイント
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

/// 2026-05-15: HW 大分類 → SalesNow sn_industry の逆マッピング。
///
/// 例: hw_industry="運輸業，郵便業" → ["運送業", "物流業", "陸運業", "倉庫業", ...]
///
/// 既存 `fetch_industry_mapping` (sn_industry → [hw_job_type]) の逆引き。
/// `v2_industry_mapping` テーブルに confidence 付きで両者の対応が格納されているため、
/// 信頼度 0.3 以上のものを採用する (低信頼度の混入を防ぎつつ網羅性を確保)。
///
/// LIKE で部分一致もサポート: hw_industry="運輸業" でも "運輸業，郵便業" の行を引く。
pub fn fetch_sn_industries_for_hw_industry(
    turso: &TursoDb,
    hw_industry: &str,
) -> Vec<String> {
    if hw_industry.is_empty() {
        return vec![];
    }
    // hw_job_type には「運輸業，郵便業」「運輸業」等の揺れがあり得るため、
    // ヘッド (「，」で切る前の部分) で LIKE マッチして広めに取る。
    let head: String = hw_industry
        .chars()
        .take_while(|c| *c != ',' && *c != '，')
        .collect();
    let pattern = format!("%{}%", if head.is_empty() { hw_industry } else { &head });
    let sql = r#"
        SELECT DISTINCT sn_industry
        FROM v2_industry_mapping
        WHERE hw_job_type LIKE ?1 AND confidence >= 0.3
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&pattern];
    turso
        .query(sql, &params)
        .unwrap_or_default()
        .iter()
        .map(|r| get_str(r, "sn_industry"))
        .filter(|s| !s.is_empty())
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

    // 業界マッピング（Turso 1 query）
    ctx.hw_job_types = fetch_industry_mapping(sn_db, &ctx.sn_industry);
    ctx.primary_hw_job_type = ctx
        .hw_job_types
        .first()
        .map(|(jt, _)| jt.clone())
        .unwrap_or_default();

    // === Phase 3: Turso 依存クエリを std::thread::scope で並列実行 ===
    // Why: Turso HTTP API は Render (US) ↔ Turso (日本) の往復で 2〜5秒/クエリ。
    //      並列化前は 3 つの独立 Turso クエリ（external_stats / region_flow / nearby）
    //      + nearby 内部の多数 HW count クエリが直列で 30〜100秒かかっていた。
    //      3 スレッドに分割することで実測 3 倍近くの短縮が見込める。
    let pref_snap = ctx.prefecture.clone();
    let sn_industry_snap = ctx.sn_industry.clone();
    let postal_snap = ctx.postal_code.clone();
    let corp_snap = ctx.corporate_number.clone();

    let (ext_result, flow_result, nearby_result) = std::thread::scope(|s| {
        // Thread A: 外部統計（人口 / 昼夜比 / 高齢化率）
        let h_ext = s.spawn(|| {
            if let Some(ext) = ext_db {
                if pref_snap.is_empty() {
                    return (0i64, 0.0f64, 0.0f64);
                }
                let sql = "SELECT total_population, daytime_population_ratio, aging_rate \
                           FROM v2_external_prefecture_stats WHERE prefecture = ?1";
                let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&pref_snap];
                if let Ok(rows) = ext.query(sql, &params) {
                    if let Some(r) = rows.first() {
                        return (
                            get_i64(r, "total_population"),
                            get_f64(r, "daytime_population_ratio"),
                            get_f64(r, "aging_rate"),
                        );
                    }
                }
            }
            (0, 0.0, 0.0)
        });

        // Thread B: 地域×業種 人材フロー
        let h_flow = s.spawn(|| {
            if pref_snap.is_empty() || sn_industry_snap.is_empty() {
                return (0i64, 0i64, 0i64, 0.0f64);
            }
            let sql = r#"
                SELECT COUNT(*) as companies,
                       COALESCE(SUM(employee_count), 0) as total_employees,
                       COALESCE(SUM(CAST(employee_count * employee_delta_1y / (100.0 + employee_delta_1y) AS INTEGER)), 0) as net_change,
                       COALESCE(AVG(employee_delta_1y), 0.0) as avg_delta
                FROM v2_salesnow_companies
                WHERE prefecture = ?1 AND sn_industry = ?2
                  AND employee_count > 0 AND employee_delta_1y IS NOT NULL
            "#;
            let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
                vec![&pref_snap, &sn_industry_snap];
            if let Ok(rows) = sn_db.query(sql, &params) {
                if let Some(r) = rows.first() {
                    return (
                        get_i64(r, "companies"),
                        get_i64(r, "total_employees"),
                        get_i64(r, "net_change"),
                        get_f64(r, "avg_delta"),
                    );
                }
            }
            (0, 0, 0, 0.0)
        });

        // Thread C: 近隣企業（郵便番号prefixマッチ、最も重い）
        let h_nearby = s.spawn(|| {
            if postal_snap.is_empty() {
                return Vec::<NearbyCompany>::new();
            }
            fetch_nearby_companies(sn_db, db, &postal_snap, &corp_snap, &pref_snap)
        });

        (
            h_ext.join().unwrap_or((0, 0.0, 0.0)),
            h_flow.join().unwrap_or((0, 0, 0, 0.0)),
            h_nearby.join().unwrap_or_default(),
        )
    });

    // Thread A 結果を ctx に反映
    ctx.population = ext_result.0;
    ctx.daytime_ratio = ext_result.1;
    ctx.aging_rate = ext_result.2;

    // Thread B 結果
    ctx.region_industry_company_count = flow_result.0;
    ctx.region_industry_total_employees = flow_result.1;
    ctx.region_industry_net_change = flow_result.2;
    ctx.region_industry_avg_delta = flow_result.3;
    ctx.company_vs_region_gap = ctx.employee_delta_1y - ctx.region_industry_avg_delta;

    // Thread C 結果
    ctx.nearby_companies = nearby_result;

    // === Phase 4: HW ローカル SQLite クエリ（高速なので直列でよい）===
    if !ctx.primary_hw_job_type.is_empty() && !ctx.prefecture.is_empty() {
        fetch_market_stats(db, &mut ctx);
        fetch_salary_distribution(db, &mut ctx);
        fetch_emp_size_distribution(db, &mut ctx);
        fetch_recruitment_reasons(db, &mut ctx);
        fetch_benefit_rates(db, &mut ctx);
    }

    fetch_national_stats(db, &mut ctx);

    // HW求人マッチング（企業名でfacility_nameを検索）
    ctx.hw_matched_total_count = count_hw_postings(db, &ctx.company_name, &ctx.prefecture);
    ctx.hw_matched_postings = fetch_hw_postings_for_company(db, &ctx.company_name, &ctx.prefecture);

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

    // 提案%ポイント生成
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
        // 2026-04-24 BUG FIX:
        //   旧実装は SELECT 列に sales_amount / sales_range / employee_delta_1y /
        //   employee_delta_3m が含まれておらず、survey report の
        //   「地域注目企業」セクションで売上・人員推移が 0 / "" になっていた。
        //   都道府県のみ版 (下ブランチ) と同じ 11 列構成に揃える。
        let muni_pattern = format!("%{}%", municipality);
        let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                   employee_count, credit_score, postal_code, \
                   sales_amount, sales_range, \
                   employee_delta_1y, employee_delta_3m \
                   FROM v2_salesnow_companies \
                   WHERE prefecture = ?1 AND address LIKE ?2 \
                   ORDER BY employee_count DESC LIMIT ?3";
        let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            vec![&prefecture, &muni_pattern, &lim];
        sn_db.query(sql, &params).unwrap_or_default()
    } else {
        // 都道府県のみ
        let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                   employee_count, credit_score, postal_code, \
                   sales_amount, sales_range, \
                   employee_delta_1y, employee_delta_3m \
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
            sales_amount: get_f64(r, "sales_amount"),
            sales_range: get_str(r, "sales_range"),
            employee_delta_1y: get_f64(r, "employee_delta_1y"),
            employee_delta_3m: get_f64(r, "employee_delta_3m"),
        })
        .collect();

    // HW求人数を一括取得（N+1回避: 1クエリで全企業分をカウント）
    batch_count_hw_postings(db, &mut companies, prefecture);

    companies
}

/// 4 セグメント (大手 / 中堅 / 急成長 / 採用活発) で企業を返す
///
/// 2026-04-29 追加: ユーザー指摘「今は地元の大手しか表示されてない」に対応。
/// 単一 ORDER BY employee_count DESC では多様な競合が見えないため、
/// 規模・成長率・HW 採用件数の 3 軸でセグメント抽出する。
///
/// # 戻り値
/// `RegionalCompanySegments`:
/// - `large` : 大手 (employee_count Top 10)
/// - `mid` : 中堅 (50-300 名、employee_count Top 10)
/// - `growth` : 急成長 (employee_delta_1y > +0.10、降順 Top 10)
/// - `hiring` : 採用活発 (HW 求人 ≥ 5、降順 Top 10)
pub fn fetch_company_segments_by_region(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    prefecture: &str,
    municipality: &str,
) -> RegionalCompanySegments {
    fetch_company_segments_by_region_with_industry(sn_db, db, prefecture, municipality, None)
}

/// 2026-04-29: 業界フィルタ対応版
///
/// `industry` (HW 大分類名) を指定すると `WHERE sn_industry LIKE %industry%` で絞る。
/// `None` または空 → 絞らない (異業種ベンチマーク用途)。
///
/// **注意**: SalesNow `sn_industry` の値は HW 大分類と完全一致しない場合があるため
/// LIKE マッチで部分一致する。例: industry="医療,福祉" → "医療" を含む sn_industry にマッチ。
/// 2026-05-15: HW 大分類「運輸業，郵便業」を直接 `sn_industry LIKE '%運輸業%'` で
///   検索すると、SalesNow 実値が「運送業」「陸運」等 別表記のため 0 件になる問題に対応。
///   呼出側 (handlers.rs) で v2_industry_mapping から HW → SalesNow 逆引きしたリストを
///   `sn_industries_override` として渡せば、`sn_industry IN (...)` 厳密一致で query する。
///   None / 空リストの場合は従来の LIKE 部分一致経路にフォールバック。
pub fn fetch_company_segments_by_region_with_industry(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    prefecture: &str,
    municipality: &str,
    industry: Option<&str>,
) -> RegionalCompanySegments {
    fetch_company_segments_by_region_with_industry_internal(
        sn_db, db, prefecture, municipality, industry, &[],
    )
}

pub fn fetch_company_segments_by_region_with_sn_industries(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    prefecture: &str,
    municipality: &str,
    sn_industries: &[String],
) -> RegionalCompanySegments {
    fetch_company_segments_by_region_with_industry_internal(
        sn_db, db, prefecture, municipality, None, sn_industries,
    )
}

/// 2026-05-15: 通勤圏 (近隣市町村) + 業界 で SalesNow 企業を取得。
///
/// `neighborhood`: (prefecture, municipality) のペアリスト (自市町村 + 通勤圏 30km 圏)
/// `sn_industries`: v2_industry_mapping 逆引きで取得した SalesNow 実値リスト
///
/// SQL: WHERE sn_industry IN (...) AND ((pref=? AND address LIKE ?) OR (pref=? AND address LIKE ?) ...)
///
/// 県境を越えた通勤圏 (例: 藤岡市 + 埼玉県本庄市) にも対応。
/// 近隣自治体まで含めることで 1 自治体単独で 0 件になる問題を緩和し、
/// 「藤岡市だけでなく近隣も含めた業界動向」を見せる。
pub fn fetch_company_segments_by_neighborhood_sn_industries(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    neighborhood: &[(String, String)],
    sn_industries: &[String],
) -> RegionalCompanySegments {
    if neighborhood.is_empty() || sn_industries.is_empty() {
        return RegionalCompanySegments::default();
    }
    let band_limit: i64 = 30;
    let band_ranges: [(i64, i64, &str); 3] = [
        (300, 9_999_999, "large"),
        (50, 299, "mid"),
        (1, 49, "small"),
    ];

    // SQL placeholder index map:
    //   ?1..?N : sn_industries
    //   ?(N+1)..?(N+1+2M) : neighborhood (pref/muni alternating)
    //   ?(N+1+2M)..  : lo, hi, band_limit
    let n_ind = sn_industries.len();
    let n_nbr = neighborhood.len();
    let in_placeholders: String = (0..n_ind)
        .map(|i| format!("?{}", i + 1))
        .collect::<Vec<_>>()
        .join(",");
    let or_pairs: String = (0..n_nbr)
        .map(|i| {
            let pref_idx = n_ind + 1 + i * 2;
            let muni_idx = pref_idx + 1;
            format!("(prefecture = ?{} AND address LIKE ?{})", pref_idx, muni_idx)
        })
        .collect::<Vec<_>>()
        .join(" OR ");
    let lo_idx = n_ind + 1 + n_nbr * 2;
    let hi_idx = lo_idx + 1;
    let limit_idx = hi_idx + 1;
    let sql = format!(
        "SELECT corporate_number, company_name, prefecture, sn_industry, \
         employee_count, credit_score, postal_code, \
         sales_amount, sales_range, \
         employee_delta_1y, employee_delta_3m \
         FROM v2_salesnow_companies \
         WHERE sn_industry IN ({}) AND ({}) \
           AND employee_count >= ?{} AND employee_count <= ?{} \
         ORDER BY employee_count DESC LIMIT ?{}",
        in_placeholders, or_pairs, lo_idx, hi_idx, limit_idx
    );

    let mut all_rows: Vec<crate::handlers::helpers::Row> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    // muni を LIKE 用に %X% で wrap した文字列を保持
    let muni_patterns: Vec<String> = neighborhood
        .iter()
        .map(|(_, m)| format!("%{}%", m))
        .collect();

    for (lo, hi, _band) in band_ranges.iter() {
        let mut params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = Vec::new();
        for s in sn_industries {
            params.push(s);
        }
        for (i, (p, _m)) in neighborhood.iter().enumerate() {
            params.push(p);
            params.push(&muni_patterns[i]);
        }
        params.push(lo);
        params.push(hi);
        params.push(&band_limit);
        let rows = sn_db.query(&sql, &params).unwrap_or_default();
        for r in rows {
            let cn = get_str(&r, "corporate_number");
            if seen.insert(cn) {
                all_rows.push(r);
            }
        }
    }

    // ここから pool 構築 + セグメント分けは既存ロジックと同等。
    // build_segments_from_rows() に切り出してあれば良かったが今は inline 再構築する。
    let mut pool: Vec<NearbyCompany> = all_rows
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
            sales_amount: get_f64(r, "sales_amount"),
            sales_range: get_str(r, "sales_range"),
            employee_delta_1y: get_f64(r, "employee_delta_1y"),
            employee_delta_3m: get_f64(r, "employee_delta_3m"),
        })
        .collect();

    // HW 求人数を一括取得 (代表 prefecture を中心点として — 通勤圏全体ではないが
    //  hw_posting_count は急成長/採用活発の判定に使うのみで地域影響は限定的)
    let primary_pref = neighborhood.first().map(|(p, _)| p.as_str()).unwrap_or("");
    batch_count_hw_postings(db, &mut pool, primary_pref);

    let mut large = pool.clone();
    large.sort_by_key(|c| std::cmp::Reverse(c.employee_count));
    large.truncate(10);

    let mut mid: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| (50..=300).contains(&c.employee_count))
        .cloned()
        .collect();
    mid.sort_by_key(|c| std::cmp::Reverse(c.employee_count));
    mid.truncate(10);

    let mut growth: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| {
            c.employee_delta_1y > 10.0
                && c.employee_delta_1y <= 300.0
                && c.employee_count >= 10
        })
        .cloned()
        .collect();
    growth.sort_by(|a, b| {
        b.employee_delta_1y
            .partial_cmp(&a.employee_delta_1y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    growth.truncate(10);

    let mut hiring: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| c.hw_posting_count >= 5)
        .cloned()
        .collect();
    hiring.sort_by_key(|c| std::cmp::Reverse(c.hw_posting_count));
    hiring.truncate(10);

    let cell_limit: usize = 5;
    let pick_band = |min_emp: i64, max_emp: i64, growth_pos: bool| -> Vec<NearbyCompany> {
        let in_realistic_range = |d: f64| d.is_finite() && d.abs() <= 300.0;
        let mut v: Vec<NearbyCompany> = pool
            .iter()
            .filter(|c| {
                (min_emp..=max_emp).contains(&c.employee_count)
                    && in_realistic_range(c.employee_delta_1y)
                    && (if growth_pos {
                        c.employee_delta_1y > 5.0
                    } else {
                        c.employee_delta_1y < -5.0
                    })
            })
            .cloned()
            .collect();
        if growth_pos {
            v.sort_by(|a, b| {
                b.employee_delta_1y
                    .partial_cmp(&a.employee_delta_1y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            v.sort_by(|a, b| {
                a.employee_delta_1y
                    .partial_cmp(&b.employee_delta_1y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        v.truncate(cell_limit);
        v
    };

    RegionalCompanySegments {
        pool_size: pool.len(),
        large,
        mid,
        growth,
        hiring,
        growth_large: pick_band(300, 9_999_999, true),
        growth_mid: pick_band(50, 299, true),
        growth_small: pick_band(1, 49, true),
        decline_large: pick_band(300, 9_999_999, false),
        decline_mid: pick_band(50, 299, false),
        decline_small: pick_band(1, 49, false),
    }
}

fn fetch_company_segments_by_region_with_industry_internal(
    sn_db: &TursoDb,
    db: &crate::db::local_sqlite::LocalDb,
    prefecture: &str,
    municipality: &str,
    industry: Option<&str>,
    sn_industries_override: &[String],
) -> RegionalCompanySegments {
    if prefecture.is_empty() {
        return RegionalCompanySegments::default();
    }

    // 2026-04-30 修正: 単一の `ORDER BY employee_count DESC LIMIT 100` だと
    // 小規模企業 (<50 名) がほぼ pool に含まれず、構造サマリの「小規模」帯が
    // 常に過少になる問題があった。3 つの規模帯から **個別に上位 30 社ずつ** 取得して
    // 多様な pool (最大 90 社) を構築する。
    let band_limit: i64 = 30;
    let industry_keyword = industry.filter(|s| !s.is_empty()).map(|s| {
        let head: String = s.chars().take_while(|c| *c != ',' && *c != '，').collect();
        if head.is_empty() {
            s.to_string()
        } else {
            head
        }
    });
    // 2026-05-15: sn_industries_override 指定時は LIKE でなく IN 句で厳密一致。
    //   v2_industry_mapping から取得した SalesNow 実値リストを使う。
    let use_sn_in_clause = !sn_industries_override.is_empty();

    // 各規模帯のクエリ
    // 大手: employee_count >= 300
    // 中規模: 50 ≤ employee_count ≤ 299
    // 小規模: 1 ≤ employee_count ≤ 49
    let band_ranges: [(i64, i64, &str); 3] = [
        (300, 9_999_999, "large"),
        (50, 299, "mid"),
        (1, 49, "small"),
    ];

    let mut all_rows: Vec<crate::handlers::helpers::Row> = Vec::new();
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();

    for (lo, hi, _band) in band_ranges.iter() {
        // 2026-05-15: sn_industries_override 指定時は動的 SQL を生成して IN 句で query。
        //   LIKE 部分一致経路 (旧) は override 不在時のみフォールバックで使う。
        let rows = if use_sn_in_clause {
            let in_placeholders: String = (0..sn_industries_override.len())
                .map(|i| format!("?{}", i + 2))
                .collect::<Vec<_>>()
                .join(",");
            // パラメータインデックス順:
            //   ?1 = prefecture
            //   ?2..?N+1 = sn_industries_override の各値
            //   ?N+2 = (muni 指定時のみ) %muni%
            //   ?N+3 .. = lo / hi / band_limit (省略可: ToSql は順)
            let muni_clause: String;
            let lo_idx: usize;
            let hi_idx: usize;
            let limit_idx: usize;
            let muni_pat: String;
            let sql: String;
            if municipality.is_empty() {
                muni_clause = String::new();
                lo_idx = sn_industries_override.len() + 2;
                hi_idx = lo_idx + 1;
                limit_idx = hi_idx + 1;
                muni_pat = String::new();
                sql = format!(
                    "SELECT corporate_number, company_name, prefecture, sn_industry, \
                     employee_count, credit_score, postal_code, \
                     sales_amount, sales_range, \
                     employee_delta_1y, employee_delta_3m \
                     FROM v2_salesnow_companies \
                     WHERE prefecture = ?1 AND sn_industry IN ({}) \
                       AND employee_count >= ?{} AND employee_count <= ?{} \
                     ORDER BY employee_count DESC LIMIT ?{}",
                    in_placeholders, lo_idx, hi_idx, limit_idx
                );
            } else {
                let muni_idx = sn_industries_override.len() + 2;
                muni_clause = format!(" AND address LIKE ?{}", muni_idx);
                lo_idx = muni_idx + 1;
                hi_idx = lo_idx + 1;
                limit_idx = hi_idx + 1;
                muni_pat = format!("%{}%", municipality);
                sql = format!(
                    "SELECT corporate_number, company_name, prefecture, sn_industry, \
                     employee_count, credit_score, postal_code, \
                     sales_amount, sales_range, \
                     employee_delta_1y, employee_delta_3m \
                     FROM v2_salesnow_companies \
                     WHERE prefecture = ?1 AND sn_industry IN ({}){} \
                       AND employee_count >= ?{} AND employee_count <= ?{} \
                     ORDER BY employee_count DESC LIMIT ?{}",
                    in_placeholders, muni_clause, lo_idx, hi_idx, limit_idx
                );
            }
            let mut params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = Vec::new();
            params.push(&prefecture);
            for s in sn_industries_override {
                params.push(s);
            }
            if !municipality.is_empty() {
                params.push(&muni_pat);
            }
            params.push(lo);
            params.push(hi);
            params.push(&band_limit);
            sn_db.query(&sql, &params).unwrap_or_default()
        } else { match (municipality.is_empty(), &industry_keyword) {
            (false, Some(kw)) => {
                let muni_pattern = format!("%{}%", municipality);
                let ind_pattern = format!("%{}%", kw);
                let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                           employee_count, credit_score, postal_code, \
                           sales_amount, sales_range, \
                           employee_delta_1y, employee_delta_3m \
                           FROM v2_salesnow_companies \
                           WHERE prefecture = ?1 AND address LIKE ?2 AND sn_industry LIKE ?3 \
                             AND employee_count >= ?4 AND employee_count <= ?5 \
                           ORDER BY employee_count DESC LIMIT ?6";
                let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![
                    &prefecture,
                    &muni_pattern,
                    &ind_pattern,
                    lo,
                    hi,
                    &band_limit,
                ];
                sn_db.query(sql, &params).unwrap_or_default()
            }
            (true, Some(kw)) => {
                let ind_pattern = format!("%{}%", kw);
                let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                           employee_count, credit_score, postal_code, \
                           sales_amount, sales_range, \
                           employee_delta_1y, employee_delta_3m \
                           FROM v2_salesnow_companies \
                           WHERE prefecture = ?1 AND sn_industry LIKE ?2 \
                             AND employee_count >= ?3 AND employee_count <= ?4 \
                           ORDER BY employee_count DESC LIMIT ?5";
                let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
                    vec![&prefecture, &ind_pattern, lo, hi, &band_limit];
                sn_db.query(sql, &params).unwrap_or_default()
            }
            (false, None) => {
                let muni_pattern = format!("%{}%", municipality);
                let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                           employee_count, credit_score, postal_code, \
                           sales_amount, sales_range, \
                           employee_delta_1y, employee_delta_3m \
                           FROM v2_salesnow_companies \
                           WHERE prefecture = ?1 AND address LIKE ?2 \
                             AND employee_count >= ?3 AND employee_count <= ?4 \
                           ORDER BY employee_count DESC LIMIT ?5";
                let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
                    vec![&prefecture, &muni_pattern, lo, hi, &band_limit];
                sn_db.query(sql, &params).unwrap_or_default()
            }
            (true, None) => {
                let sql = "SELECT corporate_number, company_name, prefecture, sn_industry, \
                           employee_count, credit_score, postal_code, \
                           sales_amount, sales_range, \
                           employee_delta_1y, employee_delta_3m \
                           FROM v2_salesnow_companies \
                           WHERE prefecture = ?1 \
                             AND employee_count >= ?2 AND employee_count <= ?3 \
                           ORDER BY employee_count DESC LIMIT ?4";
                let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
                    vec![&prefecture, lo, hi, &band_limit];
                sn_db.query(sql, &params).unwrap_or_default()
            }
        } };  // ← match ... else { match { ... } } の閉じ
        for r in rows {
            let cn = get_str(&r, "corporate_number");
            if seen.insert(cn) {
                all_rows.push(r);
            }
        }
    }
    let rows = all_rows;

    let mut pool: Vec<NearbyCompany> = rows
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
            sales_amount: get_f64(r, "sales_amount"),
            sales_range: get_str(r, "sales_range"),
            employee_delta_1y: get_f64(r, "employee_delta_1y"),
            employee_delta_3m: get_f64(r, "employee_delta_3m"),
        })
        .collect();

    // HW 求人数を一括取得 (4 セグメントすべての判定に必要)
    batch_count_hw_postings(db, &mut pool, prefecture);

    // セグメント分け
    // 大手: employee_count 降順 Top 10
    let mut large = pool.clone();
    large.sort_by_key(|c| std::cmp::Reverse(c.employee_count));
    large.truncate(10);

    // 中堅: 50 ≤ employee_count ≤ 300、Top 10 (employee_count 降順)
    let mut mid: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| (50..=300).contains(&c.employee_count))
        .cloned()
        .collect();
    mid.sort_by_key(|c| std::cmp::Reverse(c.employee_count));
    mid.truncate(10);

    // 急成長: employee_delta_1y > +10.0% (10% 増以上)、降順 Top 10
    // 2026-04-30: DB は %単位 (10.0 = +10%) で格納 (render.rs/company_markers.rs と整合)。
    // 旧コード `> 0.10` は「+0.1% 超」しかフィルタしないバグだった。
    // 2026-05-14: 一部レコードに +1000% を超える非現実値 (SalesNow データ精度
    //             由来) があり、PDF に「+3321%」等が表示されてしまう問題があった
    //             (feedback_unit_consistency_audit / 表 5-B 急成長企業の信頼性
    //             指摘 2026-05-14)。+300% 超は外れ値として除外し、レポートには
    //             現実的な急成長企業のみを掲載する。
    let mut growth: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| {
            c.employee_delta_1y > 10.0
                && c.employee_delta_1y <= 300.0
                && c.employee_count >= 10
        })
        .cloned()
        .collect();
    growth.sort_by(|a, b| {
        b.employee_delta_1y
            .partial_cmp(&a.employee_delta_1y)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    growth.truncate(10);

    // 採用活発: HW 求人 ≥ 5、降順 Top 10
    let mut hiring: Vec<NearbyCompany> = pool
        .iter()
        .filter(|c| c.hw_posting_count >= 5)
        .cloned()
        .collect();
    hiring.sort_by_key(|c| std::cmp::Reverse(c.hw_posting_count));
    hiring.truncate(10);

    // 2026-04-30: 規模 × 動向 6 マトリクス
    // 規模帯: 大企業 (300+) / 中小企業 (50-299) / 零細企業 (<50)
    // 動向: 増員傾向 (+5% 超) / 人員減少傾向 (-5% 未満)
    // 各セル Top 5 (10 だと表が横長になりすぎるため)
    let cell_limit: usize = 5;
    let pick_band = |min_emp: i64, max_emp: i64, growth_pos: bool| -> Vec<NearbyCompany> {
        // 2026-05-14: ±300% を超えるレコードはデータ精度由来の外れ値として除外
        //             (表 5-B / 5-F の信頼性確保)。
        let in_realistic_range = |d: f64| d.is_finite() && d.abs() <= 300.0;
        let mut v: Vec<NearbyCompany> = pool
            .iter()
            .filter(|c| {
                (min_emp..=max_emp).contains(&c.employee_count)
                    && in_realistic_range(c.employee_delta_1y)
                    && (if growth_pos {
                        // 2026-04-30: ±5% (±5.0) growth threshold (DB は %単位)
                        c.employee_delta_1y > 5.0
                    } else {
                        c.employee_delta_1y < -5.0
                    })
            })
            .cloned()
            .collect();
        if growth_pos {
            v.sort_by(|a, b| {
                b.employee_delta_1y
                    .partial_cmp(&a.employee_delta_1y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        } else {
            v.sort_by(|a, b| {
                a.employee_delta_1y
                    .partial_cmp(&b.employee_delta_1y)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        }
        v.truncate(cell_limit);
        v
    };

    let growth_large = pick_band(300, 9_999_999, true);
    let growth_mid = pick_band(50, 299, true);
    let growth_small = pick_band(1, 49, true);
    let decline_large = pick_band(300, 9_999_999, false);
    let decline_mid = pick_band(50, 299, false);
    let decline_small = pick_band(1, 49, false);

    RegionalCompanySegments {
        pool_size: pool.len(),
        large,
        mid,
        growth,
        hiring,
        growth_large,
        growth_mid,
        growth_small,
        decline_large,
        decline_mid,
        decline_small,
    }
}

/// 4 セグメント結果のコンテナ
///
/// 2026-04-30 追加: ユーザー指摘「増員できている / 離職が多い」 × 「大企業 / 中小企業 / 零細企業」
/// の 6 マトリクスを `growth_*` / `decline_*` フィールドで提供。既存 4 フィールドは互換性のため維持。
#[derive(Debug, Clone, Default)]
pub struct RegionalCompanySegments {
    /// 取得した母集団のサイズ (デバッグ・注記用)
    pub pool_size: usize,
    /// 大手 (employee_count Top)
    pub large: Vec<NearbyCompany>,
    /// 中堅 (50-300 名)
    pub mid: Vec<NearbyCompany>,
    /// 急成長 (employee_delta_1y > +0.10)
    pub growth: Vec<NearbyCompany>,
    /// 採用活発 (HW 求人 ≥ 5)
    pub hiring: Vec<NearbyCompany>,
    // === 2026-04-30 追加: 規模 × 動向 6 マトリクス ===
    /// 大企業 (300+ 名) × 増員傾向 (employee_delta_1y > +5%)
    pub growth_large: Vec<NearbyCompany>,
    /// 中小企業 (50-299 名) × 増員傾向
    pub growth_mid: Vec<NearbyCompany>,
    /// 零細企業 (<50 名) × 増員傾向
    pub growth_small: Vec<NearbyCompany>,
    /// 大企業 × 人員減少傾向 (employee_delta_1y < -5%)
    /// 注: 「離職が多い」だけでなく組織改編・自然減・配置転換等も含む観測
    pub decline_large: Vec<NearbyCompany>,
    /// 中小企業 × 人員減少傾向
    pub decline_mid: Vec<NearbyCompany>,
    /// 零細企業 × 人員減少傾向
    pub decline_small: Vec<NearbyCompany>,
}

/// 規模帯別の集約サマリ (バイネームではなく傾向値ベース)
///
/// 2026-04-29 追加: ユーザー指摘
/// > 大手だけ出してもしょうがない / 中小顧客が多い
/// > エンタープライズ顧客は大手のベンチマーク動向が気になる
/// > 両方羅列するとメッセージが希薄化する
///
/// 個別バイネーム表に頼らず、規模帯ごとの集約値で「規模帯別の傾向」と
/// 「規模を横断する共通点 / 乖離点」をルールベース提示するためのデータ構造。
#[derive(Debug, Default, Clone)]
pub struct StructuralSummary {
    /// 大手 (300+ 名) の社数
    pub large_count: usize,
    /// 中規模 (50-299 名) の社数
    pub mid_count: usize,
    /// 小規模 (<50 名) の社数
    pub small_count: usize,
    /// 大手の平均人員推移 (%、1y)
    pub large_avg_growth_pct: f64,
    /// 中規模の平均人員推移 (%、1y)
    pub mid_avg_growth_pct: f64,
    /// 小規模の平均人員推移 (%、1y)
    pub small_avg_growth_pct: f64,
    /// 大手の HW 求人継続率 (求人 1 件以上を出している企業の比率)
    pub large_hw_continuity_pct: f64,
    /// 中規模の HW 求人継続率
    pub mid_hw_continuity_pct: f64,
    /// 小規模の HW 求人継続率
    pub small_hw_continuity_pct: f64,
    /// pool サイズ (caveat 用)
    pub pool_size: usize,
}

impl StructuralSummary {
    pub fn total_count(&self) -> usize {
        self.large_count + self.mid_count + self.small_count
    }

    /// 規模間の人員推移格差 (max - min)、規模間で乖離が大きいか判断する指標
    pub fn growth_spread_pct(&self) -> f64 {
        let vals = [
            self.large_avg_growth_pct,
            self.mid_avg_growth_pct,
            self.small_avg_growth_pct,
        ];
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        if max.is_finite() && min.is_finite() {
            max - min
        } else {
            0.0
        }
    }

    /// 規模帯別の HW 求人継続率の最大 - 最小
    pub fn hw_continuity_spread_pct(&self) -> f64 {
        let vals = [
            self.large_hw_continuity_pct,
            self.mid_hw_continuity_pct,
            self.small_hw_continuity_pct,
        ];
        let max = vals.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let min = vals.iter().cloned().fold(f64::INFINITY, f64::min);
        if max.is_finite() && min.is_finite() {
            max - min
        } else {
            0.0
        }
    }
}

impl RegionalCompanySegments {
    /// すべて空か (fail-soft 用)
    pub fn is_empty(&self) -> bool {
        self.large.is_empty()
            && self.mid.is_empty()
            && self.growth.is_empty()
            && self.hiring.is_empty()
    }

    /// 規模帯別の集約サマリを計算 (バイネーム不要のベンチマーク用)
    ///
    /// pool の重複除去後の集約値を返す。pool は employee_count DESC で
    /// 取得しているため、極小規模 (<10 名) のサンプルは少ない可能性がある。
    pub fn structural_summary(&self) -> StructuralSummary {
        // 4 セグメントを和集合で重複除去 (corporate_number 基準)
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        let mut all: Vec<&NearbyCompany> = Vec::new();
        for seg in [&self.large, &self.mid, &self.growth, &self.hiring] {
            for c in seg {
                if seen.insert(c.corporate_number.clone()) {
                    all.push(c);
                }
            }
        }

        // 規模帯ごとに集計
        let mut large: Vec<&NearbyCompany> = Vec::new();
        let mut mid: Vec<&NearbyCompany> = Vec::new();
        let mut small: Vec<&NearbyCompany> = Vec::new();
        for c in &all {
            if c.employee_count >= 300 {
                large.push(c);
            } else if c.employee_count >= 50 {
                mid.push(c);
            } else if c.employee_count > 0 {
                small.push(c);
            }
        }

        let avg_growth = |v: &[&NearbyCompany]| -> f64 {
            // 2026-04-30: DB は %単位 (10.0 = +10%) で格納。
            // 異常値除去は ±200% まで許容 (短期合併等で 100% 超は実在)。
            let valid: Vec<f64> = v
                .iter()
                .map(|c| c.employee_delta_1y)
                .filter(|x| x.is_finite() && x.abs() < 200.0)
                .collect();
            if valid.is_empty() {
                0.0
            } else {
                valid.iter().sum::<f64>() / valid.len() as f64
            }
        };

        let hw_continuity = |v: &[&NearbyCompany]| -> f64 {
            if v.is_empty() {
                0.0
            } else {
                let with_hw = v.iter().filter(|c| c.hw_posting_count > 0).count();
                with_hw as f64 / v.len() as f64 * 100.0
            }
        };

        StructuralSummary {
            large_count: large.len(),
            mid_count: mid.len(),
            small_count: small.len(),
            large_avg_growth_pct: avg_growth(&large),
            mid_avg_growth_pct: avg_growth(&mid),
            small_avg_growth_pct: avg_growth(&small),
            large_hw_continuity_pct: hw_continuity(&large),
            mid_hw_continuity_pct: hw_continuity(&mid),
            small_hw_continuity_pct: hw_continuity(&small),
            pool_size: self.pool_size,
        }
    }

    /// 規模分布ヒストグラム (5 階級: <10 / 10-49 / 50-299 / 300-999 / 1000+)
    /// pool 全体のサイズ分布を返す
    pub fn size_histogram(&self) -> [(&'static str, usize); 5] {
        // pool は 4 セグメントの和ではないため、large が代替母集団 (employee_count 降順 Top 10)
        // 厳密な分布のため pool を再構築 (large が降順 Top 10 なので近似値)
        let mut bands = [
            ("<10 名", 0usize),
            ("10-49 名", 0),
            ("50-299 名", 0),
            ("300-999 名", 0),
            ("1000+ 名", 0),
        ];
        // 4 セグメント連結で重複除去用の corporate_number セット
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for seg in [&self.large, &self.mid, &self.growth, &self.hiring] {
            for c in seg {
                if !seen.insert(c.corporate_number.clone()) {
                    continue;
                }
                let n = c.employee_count;
                let idx = if n < 10 {
                    0
                } else if n < 50 {
                    1
                } else if n < 300 {
                    2
                } else if n < 1000 {
                    3
                } else {
                    4
                };
                bands[idx].1 += 1;
            }
        }
        bands
    }
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
    // 郵便番号上3桁でエリアマッチ (char 単位で取り出し、非 ASCII 混入時の UTF-8 panic を回避)
    let prefix: String = postal_code.chars().take(3).collect();
    if prefix.chars().count() < 3 {
        return vec![];
    }
    let like_pattern = format!("{}%", prefix);
    let sql = r#"
        SELECT corporate_number, company_name, prefecture, sn_industry,
               employee_count, credit_score, postal_code,
               sales_amount, sales_range,
               employee_delta_1y, employee_delta_3m
        FROM v2_salesnow_companies
        WHERE postal_code LIKE ?1 AND corporate_number != ?2
        ORDER BY employee_count DESC
        LIMIT 50
    "#;
    let params: Vec<&dyn crate::db::turso_http::ToSqlTurso> = vec![&like_pattern, &exclude_corp];
    let rows = sn_db.query(sql, &params).unwrap_or_default();

    // 企業リストを先に構築（hw_posting_count=0 初期値）
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
            sales_amount: get_f64(r, "sales_amount"),
            sales_range: get_str(r, "sales_range"),
            employee_delta_1y: get_f64(r, "employee_delta_1y"),
            employee_delta_3m: get_f64(r, "employee_delta_3m"),
        })
        .collect();

    // N+1 回避: batch_count_hw_postings を使って企業ごとの HW 求人数を一括取得
    // Why: 旧実装は50社 × count_hw_postings 直列 = N+1 パターンで、
    //      各クエリ 5〜30ms の積み重ねで 数百ms〜1s のオーバーヘッドが発生
    // 注: 企業ごとに prefecture が異なりうるが、batch_count_hw_postings は
    //     共通 prefecture を要求するため、prefecture ごとにグループ化する
    use std::collections::HashMap;
    let mut by_pref: HashMap<String, Vec<usize>> = HashMap::new();
    for (i, c) in companies.iter().enumerate() {
        by_pref.entry(c.prefecture.clone()).or_default().push(i);
    }
    for (pref, indices) in by_pref.iter() {
        // 同一 prefecture の企業だけ抽出して batch_count に渡す
        let mut sub: Vec<NearbyCompany> = indices.iter().map(|&i| companies[i].clone()).collect();
        batch_count_hw_postings(db, &mut sub, pref);
        // 結果を元の位置に書き戻す
        for (j, &idx) in indices.iter().enumerate() {
            companies[idx].hw_posting_count = sub[j].hw_posting_count;
        }
    }

    companies
}

// ===== クロス分析用の新規fetch関数 =====

/// Turso: 地域×業種の人材フロー集計
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

/// 提案%ポイント生成（最大3つ）
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
                    "{}の{}業界{}社の平均成長率は{:.1}%です。御社は地域平均より{:.1}%ポイント下回っています。人材確保の強化が競争力維持に重要です。",
                    ctx.prefecture, ctx.sn_industry, ctx.region_industry_company_count,
                    ctx.region_industry_avg_delta, gap
                ),
            ));
        } else if ctx.employee_delta_1y > ctx.region_industry_avg_delta + 2.0 {
            pitches.push((
                format!(
                    "御社は地域の{}業界平均を{:.1}%ポイント上回る成長率です",
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
                    "御社の平均月給（下限）は{:.0}円で、市場平均{:.0}円を{:.0}円下回っています。給与水準の見直しにより応募数が増加する可能性があります（相関であり因果は別途検証要）。",
                    ctx.company_avg_salary_min, ctx.market_avg_salary_min, gap.abs()
                ),
            ));
        } else if gap > 10000.0 {
            pitches.push((
                "御社の給与水準は相対的に市場上位の傾向がみられます".to_string(),
                format!(
                    "御社の平均月給（下限）{:.0}円は市場平均{:.0}円を{:.0}円上回っており、給与面での競争力が相対的に高い可能性があります（HW掲載求人ベースの観測値）。",
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
