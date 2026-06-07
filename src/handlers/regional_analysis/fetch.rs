//! 地域×業界分析タブ: postings を 都道府県 / 市区町村 / 業界(job_type) で集計する取得層。
//!
//! ## 設計方針
//! - レポート (顧客 CSV 軸) と異なり postings DB を直接集計する常設タブ。
//! - 中央値・分位は SQL で salary_min を集めて Rust 側で算出
//!   (competitive::fetch::fetch_salary_stats_sql の OFFSET 方式を踏襲しつつ、
//!    ヒストグラム用に生値が必要なため取得後に計算する)。
//! - 給与は HW 掲載求人由来。月給 (salary_type='月給') かつ salary_min>=50000 の
//!   ノイズ除去後に集計 (competitive と同条件)。
//! - DISPLAY_SPEC §2 遵守: 求人「件数」は表示。求職者「人数」推定は生成しない。

use crate::handlers::competitive::escape_html;
use crate::handlers::helpers::{normalize_muni_for_external, strip_county_prefix};
use crate::AppState;

/// 地域×業界フィルタ。
///
/// - `prefecture`: 必須 (空文字なら呼び出し側でデータなし扱い)。
/// - `municipality`: 任意 (空文字なら都道府県全体)。
/// - `job_type`: 業界フィルタ (任意。空文字なら全業界)。postings の job_type 列を使う。
#[derive(Clone, Debug, Default)]
pub(crate) struct RegionalFilter {
    pub prefecture: String,
    pub municipality: String,
    pub job_type: String,
}

impl RegionalFilter {
    /// SQL の WHERE 句 (prefecture 前提) とバインドパラメータを構築する。
    ///
    /// 先頭は必ず `WHERE prefecture = ?`。muni / job_type が非空なら AND 追加。
    /// 文字列はすべてバインド変数 (SQL インジェクション対策)。
    fn where_clause(&self) -> (String, Vec<String>) {
        let mut sql = String::from(" WHERE prefecture = ?");
        let mut params: Vec<String> = vec![self.prefecture.clone()];
        if !self.municipality.is_empty() {
            sql.push_str(" AND municipality = ?");
            params.push(self.municipality.clone());
        }
        if !self.job_type.is_empty() {
            sql.push_str(" AND job_type = ?");
            params.push(self.job_type.clone());
        }
        (sql, params)
    }

    /// スコープ表示ラベル (HTML escape 済み)。
    pub(crate) fn scope_label(&self) -> String {
        let area = if !self.municipality.is_empty() {
            format!("{} {}", self.prefecture, self.municipality)
        } else if !self.prefecture.is_empty() {
            self.prefecture.clone()
        } else {
            "未選択".to_string()
        };
        let job = if self.job_type.is_empty() {
            "全業界".to_string()
        } else {
            self.job_type.clone()
        };
        escape_html(&format!("{} / {}", area, job))
    }
}

// --- 内部データ型 ---

/// 給与分布ヒストグラム + 代表値。
pub(crate) struct SalaryHistogram {
    /// 件数 (月給・有効レンジ内)。
    pub count: i64,
    /// バケット下限 (円) のラベル。
    pub bucket_labels: Vec<String>,
    /// バケット件数。
    pub bucket_counts: Vec<i64>,
    /// 平均 (円)。データなし時 0。
    pub mean: i64,
    /// 中央値 (円)。データなし時 0。
    pub median: i64,
    pub has_data: bool,
}

/// 市区町村別ランキング 1 行。
pub(crate) struct MuniRankRow {
    pub municipality: String,
    pub count: i64,
    /// 給与中央値 (円)。算出不能時 None。
    pub median_salary: Option<i64>,
}

/// 雇用形態別給与統計 1 行。
pub(crate) struct EmpSalaryRow {
    pub employment_type: String,
    pub count: i64,
    pub median_salary: Option<i64>,
}

// --- Phase2 データ型 ---

/// 業界別給与比較 1 行 (postings job_type 別)。
pub(crate) struct JobTypeSalaryRow {
    pub job_type: String,
    pub count: i64,
    pub median_salary: Option<i64>,
    /// 当該フィルタで選択中の業界か (ハイライト用)。
    pub highlighted: bool,
}

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
    /// 集計対象エリア名 (escape 前)。render は scope_label を使うため未使用だが
    /// デバッグ・将来拡張用に保持。
    #[allow(dead_code)]
    pub area_name: String,
    pub has_data: bool,
}

/// 最低賃金 vs 給与中央値の比較結果 (都道府県粒度)。
pub(crate) struct WageComparison {
    /// 最低賃金 (時給, 円)。取得不能時 None。
    pub hourly_min_wage: Option<f64>,
    /// 給与中央値 (月給, 円)。算出不能時 None。
    pub median_monthly: Option<i64>,
    /// 月給中央値の時給換算 (円)。月 173.8h 換算 (法定労働時間ベース)。
    pub median_hourly: Option<f64>,
    /// 集計件数 (月給・有効レンジ)。
    pub count: i64,
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

// --- カスケードフィルタ用一覧取得 ---

/// 都道府県一覧 (postings に出現するもの)。
pub(crate) fn fetch_prefectures(state: &AppState) -> Vec<String> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };
    let rows = db
        .query(
            "SELECT DISTINCT prefecture FROM postings \
             WHERE prefecture IS NOT NULL AND prefecture != '' ORDER BY prefecture",
            &[],
        )
        .unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            r.get("prefecture")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// 指定都道府県の市区町村一覧。
pub(crate) fn fetch_municipalities(state: &AppState, pref: &str) -> Vec<String> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };
    if pref.is_empty() {
        return Vec::new();
    }
    let rows = db
        .query(
            "SELECT DISTINCT municipality FROM postings \
             WHERE prefecture = ? AND municipality IS NOT NULL AND municipality != '' \
             ORDER BY municipality",
            &[&pref as &dyn rusqlite::types::ToSql],
        )
        .unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            r.get("municipality")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        })
        .filter(|s| !s.is_empty())
        .collect()
}

/// 業界(job_type)一覧 (都道府県・市区町村で絞り込み、件数付き降順)。
pub(crate) fn fetch_job_types(state: &AppState, pref: &str, muni: &str) -> Vec<(String, i64)> {
    let db = match &state.hw_db {
        Some(db) => db,
        None => return Vec::new(),
    };
    let mut sql = String::from(
        "SELECT job_type, COUNT(*) as cnt FROM postings \
         WHERE job_type IS NOT NULL AND job_type != ''",
    );
    let mut params: Vec<String> = Vec::new();
    if !pref.is_empty() {
        sql.push_str(" AND prefecture = ?");
        params.push(pref.to_string());
    }
    if !muni.is_empty() {
        sql.push_str(" AND municipality = ?");
        params.push(muni.to_string());
    }
    sql.push_str(" GROUP BY job_type ORDER BY cnt DESC");

    let bind: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = db.query(&sql, &bind).unwrap_or_default();
    rows.iter()
        .filter_map(|r| {
            let jt = r.get("job_type").and_then(|v| v.as_str())?.to_string();
            let cnt = r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);
            if jt.is_empty() {
                None
            } else {
                Some((jt, cnt))
            }
        })
        .collect()
}

// --- 集計 ---

/// 月給かつ salary_min が有効な値だけを集めるための追加条件。
const SALARY_MIN_COND: &str = " AND salary_type = '月給' AND salary_min >= 50000";

/// salary_min の生値リストを WHERE 句に従って取得 (昇順)。
fn fetch_salary_min_values(
    db: &crate::db::local_sqlite::LocalDb,
    where_clause: &str,
    params: &[String],
) -> Vec<i64> {
    let sql = format!(
        "SELECT salary_min FROM postings{}{} ORDER BY salary_min ASC",
        where_clause, SALARY_MIN_COND
    );
    let bind: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = match db.query(&sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("regional_analysis salary fetch failed: {e}");
            return Vec::new();
        }
    };
    rows.iter()
        .filter_map(|r| r.get("salary_min").and_then(|v| v.as_i64()))
        .filter(|&v| v >= 50000)
        .collect()
}

/// 昇順ソート済みスライスの中央値。空なら None。
pub(crate) fn median_sorted(sorted: &[i64]) -> Option<i64> {
    let n = sorted.len();
    if n == 0 {
        return None;
    }
    if n % 2 == 1 {
        Some(sorted[n / 2])
    } else {
        // 偶数件は中央 2 値の平均 (整数丸め)
        Some((sorted[n / 2 - 1] + sorted[n / 2]) / 2)
    }
}

/// 1) 給与分布ヒストグラム。
///
/// バケット幅は 5 万円固定。50,000〜上限までを 5 万円刻みで集計。
/// 平均・中央値も同じ母集団 (月給・有効レンジ) から算出する。
pub(crate) fn fetch_salary_histogram(
    db: &crate::db::local_sqlite::LocalDb,
    filter: &RegionalFilter,
) -> SalaryHistogram {
    let (where_clause, params) = filter.where_clause();
    let values = fetch_salary_min_values(db, &where_clause, &params);

    if values.is_empty() {
        return SalaryHistogram {
            count: 0,
            bucket_labels: Vec::new(),
            bucket_counts: Vec::new(),
            mean: 0,
            median: 0,
            has_data: false,
        };
    }

    let count = values.len() as i64;
    let sum: i128 = values.iter().map(|&v| v as i128).sum();
    let mean = (sum / count as i128) as i64;
    // values は ASC 取得済みだが念のためソート不変条件を担保しない (取得時 ORDER BY)。
    let median = median_sorted(&values).unwrap_or(0);

    // バケット: 5 万円刻み。最大値を含む帯まで生成。
    const BUCKET_WIDTH: i64 = 50_000;
    let min_v = *values.first().unwrap_or(&0);
    let max_v = *values.last().unwrap_or(&0);
    let start_band = (min_v / BUCKET_WIDTH) * BUCKET_WIDTH;
    let end_band = (max_v / BUCKET_WIDTH) * BUCKET_WIDTH;
    let n_buckets = ((end_band - start_band) / BUCKET_WIDTH + 1).max(1) as usize;

    let mut bucket_counts = vec![0i64; n_buckets];
    for &v in &values {
        let idx = ((v - start_band) / BUCKET_WIDTH) as usize;
        let idx = idx.min(n_buckets - 1);
        bucket_counts[idx] += 1;
    }
    let bucket_labels: Vec<String> = (0..n_buckets)
        .map(|i| {
            let lo = start_band + i as i64 * BUCKET_WIDTH;
            format!("{}万", lo / 10_000)
        })
        .collect();

    SalaryHistogram {
        count,
        bucket_labels,
        bucket_counts,
        mean,
        median,
        has_data: true,
    }
}

/// 2) 市区町村別 求人数・給与中央値ランキング。
///
/// 業界フィルタ連動 (filter.job_type)。都道府県内の市区町村ごとに件数と
/// 給与中央値を算出。中央値は市区町村ごとに salary_min を集めて Rust で計算。
/// 件数降順、上限 50 件。
pub(crate) fn fetch_muni_ranking(
    db: &crate::db::local_sqlite::LocalDb,
    filter: &RegionalFilter,
    limit: usize,
) -> Vec<MuniRankRow> {
    if filter.prefecture.is_empty() {
        return Vec::new();
    }

    // 市区町村フィルタはランキングでは無視 (常に都道府県内の全市区町村を比較)。
    // job_type のみ連動させる。
    let mut count_sql = String::from(
        "SELECT municipality, COUNT(*) as cnt FROM postings \
         WHERE prefecture = ? AND municipality IS NOT NULL AND municipality != ''",
    );
    let mut params: Vec<String> = vec![filter.prefecture.clone()];
    if !filter.job_type.is_empty() {
        count_sql.push_str(" AND job_type = ?");
        params.push(filter.job_type.clone());
    }
    count_sql.push_str(" GROUP BY municipality ORDER BY cnt DESC");
    count_sql.push_str(&format!(" LIMIT {}", limit.max(1)));

    let bind: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = match db.query(&count_sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("regional_analysis muni ranking failed: {e}");
            return Vec::new();
        }
    };

    let mut out: Vec<MuniRankRow> = Vec::with_capacity(rows.len());
    for r in &rows {
        let muni = match r.get("municipality").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let count = r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);

        // 当該市区町村の給与中央値を算出
        let muni_filter = RegionalFilter {
            prefecture: filter.prefecture.clone(),
            municipality: muni.clone(),
            job_type: filter.job_type.clone(),
        };
        let (wc, wparams) = muni_filter.where_clause();
        let salaries = fetch_salary_min_values(db, &wc, &wparams);
        let median_salary = median_sorted(&salaries);

        out.push(MuniRankRow {
            municipality: muni,
            count,
            median_salary,
        });
    }
    out
}

/// 3) 雇用形態別 給与統計 (中央値 / 件数)。
///
/// 給与中央値は雇用形態問わず月給・有効レンジで算出するため、ここでは
/// SALARY_MIN_COND を外し全レコード件数 + 月給ベース中央値の両方を出す。
/// 件数は全雇用形態の求人数、中央値は月給・有効レンジの salary_min。
pub(crate) fn fetch_emp_salary_stats(
    db: &crate::db::local_sqlite::LocalDb,
    filter: &RegionalFilter,
) -> Vec<EmpSalaryRow> {
    let (where_clause, params) = filter.where_clause();

    // 雇用形態の一覧 (件数降順)
    let list_sql = format!(
        "SELECT employment_type, COUNT(*) as cnt FROM postings{} \
         AND employment_type IS NOT NULL AND employment_type != '' \
         GROUP BY employment_type ORDER BY cnt DESC",
        where_clause
    );
    let bind: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = match db.query(&list_sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("regional_analysis emp stats failed: {e}");
            return Vec::new();
        }
    };

    let mut out: Vec<EmpSalaryRow> = Vec::with_capacity(rows.len());
    for r in &rows {
        let emp = match r.get("employment_type").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let count = r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);

        // 当該雇用形態の月給中央値
        let emp_where = format!("{} AND employment_type = ?", where_clause);
        let mut emp_params = params.clone();
        emp_params.push(emp.clone());
        let salaries = fetch_salary_min_values(db, &emp_where, &emp_params);
        let median_salary = median_sorted(&salaries);

        out.push(EmpSalaryRow {
            employment_type: emp,
            count,
            median_salary,
        });
    }
    out
}

// --- Phase2 集計 ---

/// 4) 業界別給与比較 (postings job_type 別 中央値・件数)。
///
/// 都道府県 (+市区町村) 内の業界ごとに件数と給与中央値を算出。
/// filter.job_type が非空のとき当該業界に `highlighted=true` を立てる。
/// 件数降順、上限 `limit` 件。hw_db (postings) を集計。
pub(crate) fn fetch_job_type_salary(
    db: &crate::db::local_sqlite::LocalDb,
    filter: &RegionalFilter,
    limit: usize,
) -> Vec<JobTypeSalaryRow> {
    if filter.prefecture.is_empty() {
        return Vec::new();
    }

    // 業界比較では filter.job_type は「ハイライト対象」としてのみ使い、
    // 集計母集団からは除外しない (全業界を並べて比較するため)。
    let mut count_sql = String::from(
        "SELECT job_type, COUNT(*) as cnt FROM postings \
         WHERE prefecture = ? AND job_type IS NOT NULL AND job_type != ''",
    );
    let mut params: Vec<String> = vec![filter.prefecture.clone()];
    if !filter.municipality.is_empty() {
        count_sql.push_str(" AND municipality = ?");
        params.push(filter.municipality.clone());
    }
    count_sql.push_str(" GROUP BY job_type ORDER BY cnt DESC");
    count_sql.push_str(&format!(" LIMIT {}", limit.max(1)));

    let bind: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = match db.query(&count_sql, &bind) {
        Ok(r) => r,
        Err(e) => {
            tracing::error!("regional_analysis job_type salary failed: {e}");
            return Vec::new();
        }
    };

    let mut out: Vec<JobTypeSalaryRow> = Vec::with_capacity(rows.len());
    for r in &rows {
        let jt = match r.get("job_type").and_then(|v| v.as_str()) {
            Some(s) if !s.is_empty() => s.to_string(),
            _ => continue,
        };
        let count = r.get("cnt").and_then(|v| v.as_i64()).unwrap_or(0);

        // 当該業界の月給中央値
        let jt_filter = RegionalFilter {
            prefecture: filter.prefecture.clone(),
            municipality: filter.municipality.clone(),
            job_type: jt.clone(),
        };
        let (wc, wparams) = jt_filter.where_clause();
        let salaries = fetch_salary_min_values(db, &wc, &wparams);
        let median_salary = median_sorted(&salaries);

        let highlighted = !filter.job_type.is_empty() && filter.job_type == jt;
        out.push(JobTypeSalaryRow {
            job_type: jt,
            count,
            median_salary,
            highlighted,
        });
    }
    out
}

/// 5) 人口ピラミッド (v2_external_population_pyramid)。
///
/// 市区町村指定時はその市区町村粒度、未指定時は都道府県集計。
/// turso_db を優先し無ければローカル hw_db にフォールバック。
/// postings (郡名込み) と外部統計 (郡名なし) の不一致は
/// normalize_muni_for_external で吸収。
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

    // 粒度・SQL・パラメータの決定。
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

    // Turso 優先 → ローカルフォールバック。
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

/// 月給中央値の時給換算で使う月間労働時間 (h)。
/// 法定労働時間 40h/週 × 52週 ÷ 12月 ≒ 173.3h。丸めて 173.8 を使用。
const MONTHLY_WORK_HOURS: f64 = 173.8;

/// 6) 最低賃金 vs 給与中央値 (都道府県粒度)。
///
/// 最低賃金は v2_external_minimum_wage の都道府県値 (時給)。
/// 給与中央値は postings の月給・有効レンジから算出し、時給換算も併記。
/// 最低賃金は常に都道府県値である旨を render 側で明記する。
pub(crate) fn fetch_wage_comparison(state: &AppState, filter: &RegionalFilter) -> WageComparison {
    let empty = WageComparison {
        hourly_min_wage: None,
        median_monthly: None,
        median_hourly: None,
        count: 0,
        has_data: false,
    };
    if filter.prefecture.is_empty() {
        return empty;
    }

    // 最低賃金 (都道府県値, 時給) — turso_db 優先。
    let wage_sql = "SELECT hourly_min_wage FROM v2_external_minimum_wage WHERE prefecture = ?";
    let wage_rows = query_external(state, wage_sql, &[filter.prefecture.clone()]);
    let hourly_min_wage = wage_rows
        .first()
        .and_then(|r| r.get("hourly_min_wage"))
        .and_then(|v| v.as_f64());

    // 給与中央値 (月給) — postings (hw_db) から。市区町村/業界フィルタ連動。
    let (median_monthly, count): (Option<i64>, i64) = match &state.hw_db {
        Some(db) => {
            let (wc, wparams) = filter.where_clause();
            let salaries = fetch_salary_min_values(db, &wc, &wparams);
            (median_sorted(&salaries), salaries.len() as i64)
        }
        None => (None, 0),
    };

    let median_hourly = median_monthly.map(|m| m as f64 / MONTHLY_WORK_HOURS);

    let has_data = hourly_min_wage.is_some() || median_monthly.is_some();
    if !has_data {
        return empty;
    }
    WageComparison {
        hourly_min_wage,
        median_monthly,
        median_hourly,
        count,
        has_data: true,
    }
}

/// 7) 企業成長マトリックス (外部企業データ = v2_salesnow_companies)。
///
/// UI には "SalesNow" 固有名を出さない (「外部企業データ」)。
/// salesnow_db を使用。市区町村指定時は address LIKE で絞り込み。
/// 成長率 (employee_delta_1y, %) × 従業員数 の散布図用ポイントを返す。
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
        // 市区町村は address LIKE で絞り込む (郡名込みでも部分一致するよう
        // strip 後の muni 名を使用)。
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

/// 外部統計クエリ (Turso 優先 → ローカル hw_db フォールバック)。
/// competitive::external::query_external と同等の挙動。
fn query_external(
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

// ============================================================
// Phase 3: 外部統計 3 パネル (在留外国人 / インターネット利用 / 職業別就業者)
// ============================================================

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

/// 8) 在留外国人を取得 (都道府県値・在留資格別降順)。総数行は除外。
///
/// 出典: SSDSE-A (住民基本台帳)。外国人材の雇用可能性・多文化対応ニーズの把握用。
pub(crate) fn fetch_foreign_residents(
    state: &AppState,
    filter: &RegionalFilter,
) -> ForeignResidents {
    if filter.prefecture.is_empty() {
        return ForeignResidents::default();
    }
    // prefecture 完全一致でヘッダ混入行は除外される。総数系ラベルは LIKE で除外。
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

/// インターネット利用 (都道府県粒度)。
#[derive(Clone, Debug, Default)]
pub(crate) struct InternetUsage {
    pub usage_rate: Option<f64>,
    pub smartphone_rate: Option<f64>,
    pub year: Option<i64>,
    pub has_data: bool,
}

/// 9) インターネット利用率・スマートフォン保有率を取得 (都道府県値)。
///
/// 出典: 通信利用動向。採用チャネル (SNS/WEB) の有効性を判断する参考指標。
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

/// 10) 職業別就業者を取得。
///
/// `data_label='measured' AND basis='workplace'` (国勢調査・従業地ベース実測) のみ集計。
/// 男女合算 (gender 値を SUM)。市区町村未選択時は都道府県集計。
/// estimated_beta (population NULL の推定行) は `population IS NOT NULL` で除外。
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn where_clause_pref_only() {
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "".into(),
        };
        let (sql, params) = f.where_clause();
        assert_eq!(sql, " WHERE prefecture = ?");
        assert_eq!(params, vec!["東京都".to_string()]);
    }

    #[test]
    fn where_clause_full() {
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "新宿区".into(),
            job_type: "医療，福祉".into(),
        };
        let (sql, params) = f.where_clause();
        assert_eq!(
            sql,
            " WHERE prefecture = ? AND municipality = ? AND job_type = ?"
        );
        assert_eq!(
            params,
            vec![
                "東京都".to_string(),
                "新宿区".to_string(),
                "医療，福祉".to_string()
            ]
        );
    }

    #[test]
    fn where_clause_uses_bind_not_interpolation() {
        // XSS/SQLi 監査: 悪意ある job_type 値が SQL 文字列に直接埋め込まれないこと。
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "'; DROP TABLE postings; --".into(),
        };
        let (sql, params) = f.where_clause();
        assert!(!sql.contains("DROP TABLE"));
        assert!(params.iter().any(|p| p.contains("DROP TABLE")));
    }

    #[test]
    fn median_sorted_odd() {
        assert_eq!(median_sorted(&[100, 200, 300]), Some(200));
    }

    #[test]
    fn median_sorted_even() {
        // 偶数件は中央 2 値の平均
        assert_eq!(median_sorted(&[100, 200, 300, 400]), Some(250));
    }

    #[test]
    fn median_sorted_empty_is_none() {
        // 空入力で silent に 0 を返さず None (データなしを明示できる)
        assert_eq!(median_sorted(&[]), None);
    }

    #[test]
    fn median_sorted_single() {
        assert_eq!(median_sorted(&[180_000]), Some(180_000));
    }

    #[test]
    fn scope_label_neutral_and_escaped() {
        let f = RegionalFilter {
            prefecture: "東京都".into(),
            municipality: "".into(),
            job_type: "".into(),
        };
        // 全業界・都道府県のみのときのラベル
        assert_eq!(f.scope_label(), "東京都 / 全業界");
    }
}
