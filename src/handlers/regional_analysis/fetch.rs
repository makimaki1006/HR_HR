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
