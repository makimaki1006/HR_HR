//! データ取得関数（全 fetch_* 関数）

use std::collections::HashMap;
use serde_json::Value;

use super::super::helpers::table_exists;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

/// Turso外部DBクエリ実行ヘルパー
/// Turso接続がある場合はTursoを使い、なければローカルDBにフォールバック
fn query_turso_or_local(
    turso: Option<&TursoDb>,
    local_db: &Db,
    sql: &str,
    params: &[String],
    local_table_check: &str,
) -> Vec<Row> {
    // Turso優先
    if let Some(tdb) = turso {
        let p: Vec<&dyn crate::db::turso_http::ToSqlTurso> =
            params.iter().map(|s| s as &dyn crate::db::turso_http::ToSqlTurso).collect();
        match tdb.query(sql, &p) {
            Ok(rows) if !rows.is_empty() => return rows,
            Ok(_) => {} // 空結果 → ローカルにフォールバック
            Err(e) => {
                tracing::warn!("Turso query failed, falling back to local: {e}");
            }
        }
    }

    // ローカルDBフォールバック
    if !local_table_check.is_empty() && !table_exists(local_db, local_table_check) {
        return vec![];
    }
    let p: Vec<&dyn rusqlite::types::ToSql> =
        params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    local_db.query(sql, &p).unwrap_or_default()
}

/// 3レベルフィルタクエリ実行（市区町村→都道府県→全国）
fn query_3level(
    db: &Db,
    table: &str,
    pref: &str,
    muni: &str,
    select_cols: &str,
    filter_suffix: &str,
    national_select: &str,
    national_suffix: &str,
) -> Vec<Row> {
    if !table_exists(db, table) { return vec![]; }
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT {} FROM {} WHERE prefecture = ?1 AND municipality = ?2 {}",
            select_cols, table, filter_suffix),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        (format!("SELECT {} FROM {} WHERE prefecture = ?1 AND municipality = '' {}",
            select_cols, table, filter_suffix),
         vec![pref.to_string()])
    } else {
        (format!("SELECT {} FROM {} WHERE municipality = '' {}",
            national_select, table, national_suffix),
         vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 1 ========

pub(crate) fn fetch_vacancy_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, vacancy_count, growth_count, new_facility_count, vacancy_rate, growth_rate \
          FROM v2_vacancy_rate WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, SUM(vacancy_count) as vacancy_count, \
          SUM(growth_count) as growth_count, SUM(new_facility_count) as new_facility_count, \
          CAST(SUM(vacancy_count) AS REAL) / SUM(total_count) as vacancy_rate, \
          CAST(SUM(growth_count) AS REAL) / SUM(total_count) as growth_rate \
          FROM v2_vacancy_rate WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// C-4 業種別: 正社員とパートの欠員補充率上位10業種
pub(crate) fn fetch_vacancy_by_industry(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (filter, params): (String, Vec<String>) = if !muni.is_empty() {
        ("prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 業種集計
        ("municipality = '' AND length(industry_raw) > 0".to_string(), vec![])
    };

    let sql = format!(
        "SELECT industry_raw, emp_group, total_count, vacancy_rate, growth_rate \
         FROM v2_vacancy_rate WHERE {filter} AND total_count >= 30 \
         ORDER BY vacancy_rate DESC LIMIT 30"
    );
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_resilience_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE prefecture = ?1 AND municipality = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT prefecture as emp_group, total_count, industry_count, shannon_index, evenness, \
          top_industry, top_industry_share, hhi \
          FROM v2_regional_resilience WHERE municipality = '' AND emp_group = '正社員' \
          ORDER BY shannon_index DESC LIMIT 10".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_transparency_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_count, avg_transparency, median_transparency, \
          disclosure_annual_holidays, disclosure_bonus_months, disclosure_employee_count, \
          disclosure_capital, disclosure_overtime, disclosure_female_ratio, \
          disclosure_parttime_ratio, disclosure_founding_year \
          FROM v2_transparency_score WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_count) as total_count, \
          AVG(avg_transparency) as avg_transparency, AVG(median_transparency) as median_transparency, \
          AVG(disclosure_annual_holidays) as disclosure_annual_holidays, \
          AVG(disclosure_bonus_months) as disclosure_bonus_months, \
          AVG(disclosure_employee_count) as disclosure_employee_count, \
          AVG(disclosure_capital) as disclosure_capital, \
          AVG(disclosure_overtime) as disclosure_overtime, \
          AVG(disclosure_female_ratio) as disclosure_female_ratio, \
          AVG(disclosure_parttime_ratio) as disclosure_parttime_ratio, \
          AVG(disclosure_founding_year) as disclosure_founding_year \
          FROM v2_transparency_score WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 2 ========

pub(crate) fn fetch_temperature_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, sample_count, temperature, \
        urgency_density, selectivity_density, urgency_hit_rate, selectivity_hit_rate";
    // H-2: 全国集計はsample_countで加重平均（離島と東京が同じ重みにならないように）
    let nat = "emp_group, SUM(sample_count) as sample_count, \
        SUM(temperature * sample_count) / SUM(sample_count) as temperature, \
        SUM(urgency_density * sample_count) / SUM(sample_count) as urgency_density, \
        SUM(selectivity_density * sample_count) / SUM(sample_count) as selectivity_density, \
        SUM(urgency_hit_rate * sample_count) / SUM(sample_count) as urgency_hit_rate, \
        SUM(selectivity_hit_rate * sample_count) / SUM(sample_count) as selectivity_hit_rate";
    query_3level(db, "v2_text_temperature", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group",
        nat, "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group")
}

pub(crate) fn fetch_competition_data(db: &Db, pref: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cross_industry_competition'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT salary_band, education_group, emp_group, total_postings, industry_count, top_industries \
          FROM v2_cross_industry_competition WHERE prefecture = ?1 AND total_postings >= 10 \
          ORDER BY industry_count DESC LIMIT 30".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT salary_band, education_group, emp_group, \
          SUM(total_postings) as total_postings, AVG(industry_count) as industry_count, '' as top_industries \
          FROM v2_cross_industry_competition WHERE total_postings >= 10 \
          GROUP BY salary_band, education_group, emp_group \
          ORDER BY AVG(industry_count) DESC LIMIT 30".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_anomaly_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, metric_name, total_count, anomaly_count, anomaly_rate, \
        avg_value, stddev_value, anomaly_high_count, anomaly_low_count";
    let nat = "emp_group, metric_name, SUM(total_count) as total_count, \
        SUM(anomaly_count) as anomaly_count, \
        CAST(SUM(anomaly_count) AS REAL) / SUM(total_count) as anomaly_rate, \
        AVG(avg_value) as avg_value, AVG(stddev_value) as stddev_value, \
        SUM(anomaly_high_count) as anomaly_high_count, SUM(anomaly_low_count) as anomaly_low_count";
    query_3level(db, "v2_anomaly_stats", pref, muni,
        cols, "ORDER BY emp_group, metric_name",
        nat, "GROUP BY emp_group, metric_name ORDER BY emp_group, metric_name")
}

pub(crate) fn fetch_cascade_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if db.query_scalar::<i64>(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='v2_cascade_summary'", &[]
    ).unwrap_or(0) == 0 { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = ?2 AND length(industry_raw) > 0 \
          AND posting_count >= 20 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, industry_raw, posting_count, facility_count, \
          avg_salary_min, median_salary_min, avg_employee_count, avg_annual_holidays, vacancy_rate \
          FROM v2_cascade_summary WHERE prefecture = ?1 AND municipality = '' AND length(industry_raw) > 0 \
          AND posting_count >= 30 ORDER BY posting_count DESC LIMIT 20".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 業種別サマリー（各雇用形態の上位）
        ("SELECT emp_group, industry_raw, SUM(posting_count) as posting_count, \
          SUM(facility_count) as facility_count, \
          AVG(avg_salary_min) as avg_salary_min, AVG(median_salary_min) as median_salary_min, \
          AVG(avg_employee_count) as avg_employee_count, AVG(avg_annual_holidays) as avg_annual_holidays, \
          CAST(SUM(vacancy_rate * posting_count) AS REAL) / SUM(posting_count) as vacancy_rate \
          FROM v2_cascade_summary WHERE municipality = '' AND length(industry_raw) > 0 \
          GROUP BY emp_group, industry_raw HAVING SUM(posting_count) >= 100 \
          ORDER BY SUM(posting_count) DESC LIMIT 20".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 1B（給与分析） ========

pub(crate) fn fetch_salary_structure(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_type, total_count, avg_salary_min, avg_salary_max, \
        median_salary_min, p25_salary_min, p75_salary_min, p90_salary_min, \
        salary_spread, avg_bonus_months, estimated_annual_min";
    let nat = "emp_group, salary_type, SUM(total_count) as total_count, \
        AVG(avg_salary_min) as avg_salary_min, AVG(avg_salary_max) as avg_salary_max, \
        AVG(median_salary_min) as median_salary_min, AVG(p25_salary_min) as p25_salary_min, \
        AVG(p75_salary_min) as p75_salary_min, AVG(p90_salary_min) as p90_salary_min, \
        AVG(salary_spread) as salary_spread, AVG(avg_bonus_months) as avg_bonus_months, \
        AVG(estimated_annual_min) as estimated_annual_min";
    query_3level(db, "v2_salary_structure", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group, salary_type",
        nat, "AND industry_raw = '' GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type")
}

pub(crate) fn fetch_salary_competitiveness(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, local_avg_salary, national_avg_salary, competitiveness_index, \
        percentile_rank, sample_count";
    let nat = "emp_group, AVG(local_avg_salary) as local_avg_salary, \
        AVG(national_avg_salary) as national_avg_salary, \
        AVG(competitiveness_index) as competitiveness_index, \
        AVG(percentile_rank) as percentile_rank, SUM(sample_count) as sample_count";
    query_3level(db, "v2_salary_competitiveness", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group",
        nat, "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group")
}

pub(crate) fn fetch_compensation_package(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_count, avg_salary_min, avg_annual_holidays, avg_bonus_months, \
        salary_pctile, holidays_pctile, bonus_pctile, composite_score, rank_label";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_salary_min) as avg_salary_min, AVG(avg_annual_holidays) as avg_annual_holidays, \
        AVG(avg_bonus_months) as avg_bonus_months, \
        AVG(salary_pctile) as salary_pctile, AVG(holidays_pctile) as holidays_pctile, \
        AVG(bonus_pctile) as bonus_pctile, AVG(composite_score) as composite_score, \
        '' as rank_label";
    query_3level(db, "v2_compensation_package", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group",
        nat, "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group")
}

// ======== データ取得: Phase 2B（テキスト分析） ========

pub(crate) fn fetch_text_quality(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_count, avg_char_count, avg_unique_char_ratio, \
        avg_kanji_ratio, avg_numeric_ratio, avg_punctuation_density, information_score";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_char_count) as avg_char_count, AVG(avg_unique_char_ratio) as avg_unique_char_ratio, \
        AVG(avg_kanji_ratio) as avg_kanji_ratio, AVG(avg_numeric_ratio) as avg_numeric_ratio, \
        AVG(avg_punctuation_density) as avg_punctuation_density, AVG(information_score) as information_score";
    query_3level(db, "v2_text_quality", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group",
        nat, "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group")
}

pub(crate) fn fetch_keyword_profile(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, keyword_category, density, avg_count_per_posting";
    let nat = "emp_group, keyword_category, AVG(density) as density, AVG(avg_count_per_posting) as avg_count_per_posting";
    query_3level(db, "v2_keyword_profile", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group, keyword_category",
        nat, "AND industry_raw = '' GROUP BY emp_group, keyword_category ORDER BY emp_group, keyword_category")
}

// ======== データ取得: Phase 3（市場構造） ========

pub(crate) fn fetch_employer_strategy(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_employer_strategy_summary") { return vec![]; }

    // Python側テーブルはピボット形式（premium_count/premium_pct/salary_focus_count/...）
    // Rust側はrow形式（strategy_type/count/pct）で表示するため、UNION ALLで変換
    let base_filter = if !muni.is_empty() {
        format!("WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = ''")
    } else if !pref.is_empty() {
        format!("WHERE prefecture = ?1 AND municipality = '' AND industry_raw = ''")
    } else {
        format!("WHERE municipality = '' AND industry_raw = ''")
    };

    let agg = if muni.is_empty() && pref.is_empty() { true } else { false };

    let (sql, params): (String, Vec<String>) = if agg {
        // 全国集計: SUMでピボットカラムを集計後、UNION ALL
        (format!(
            "SELECT emp_group, 'プレミアム型' as strategy_type, SUM(premium_count) as count, \
               CAST(SUM(premium_count) AS REAL) / SUM(total_count) * 100.0 as pct \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, '給与一本勝負型', SUM(salary_focus_count), \
               CAST(SUM(salary_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, '福利厚生重視型', SUM(benefits_focus_count), \
               CAST(SUM(benefits_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             UNION ALL \
             SELECT emp_group, 'コスト優先型', SUM(cost_focus_count), \
               CAST(SUM(cost_focus_count) AS REAL) / SUM(total_count) * 100.0 \
             FROM v2_employer_strategy_summary {f} GROUP BY emp_group \
             ORDER BY emp_group, strategy_type", f = base_filter), vec![])
    } else {
        let params = if !muni.is_empty() {
            vec![pref.to_string(), muni.to_string()]
        } else {
            vec![pref.to_string()]
        };
        (format!(
            "SELECT emp_group, 'プレミアム型' as strategy_type, premium_count as count, premium_pct as pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, '給与一本勝負型', salary_focus_count, salary_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, '福利厚生重視型', benefits_focus_count, benefits_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             UNION ALL \
             SELECT emp_group, 'コスト優先型', cost_focus_count, cost_focus_pct \
             FROM v2_employer_strategy_summary {f} \
             ORDER BY emp_group, strategy_type", f = base_filter), params)
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_monopsony_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_monopsony_index") { return vec![]; }

    // Python側テーブルにtop1_nameカラムは存在しない
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = ?2 AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT emp_group, total_postings, unique_facilities, hhi, concentration_level, \
          top1_share, top3_share, top5_share, gini \
          FROM v2_monopsony_index WHERE prefecture = ?1 AND municipality = '' AND industry_raw = '' \
          ORDER BY emp_group".to_string(), vec![pref.to_string()])
    } else {
        ("SELECT emp_group, SUM(total_postings) as total_postings, \
          SUM(unique_facilities) as unique_facilities, \
          AVG(hhi) as hhi, '' as concentration_level, \
          AVG(top1_share) as top1_share, \
          AVG(top3_share) as top3_share, AVG(top5_share) as top5_share, AVG(gini) as gini \
          FROM v2_monopsony_index WHERE municipality = '' AND industry_raw = '' \
          GROUP BY emp_group ORDER BY emp_group".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_spatial_mismatch(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if !table_exists(db, "v2_spatial_mismatch") { return vec![]; }

    // 空間ミスマッチは市区町村レベルのみ（industry_rawフィルタなし）
    let sql = "SELECT emp_group, posting_count, avg_salary_min, \
          accessible_postings_30km, accessible_avg_salary_30km, \
          accessible_postings_60km, salary_gap_vs_accessible, isolation_score \
          FROM v2_spatial_mismatch WHERE prefecture = ?1 AND municipality = ?2 \
          ORDER BY emp_group".to_string();
    let params = vec![pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

// ======== データ取得: Phase 4（外部データ統合） ========

/// Phase 4-1: 最低賃金マスタ
pub(crate) fn fetch_minimum_wage(db: &Db, pref: &str) -> Vec<Row> {
    if !table_exists(db, "v2_external_minimum_wage") { return vec![]; }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC".to_string(), vec![])
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(&sql, &p).unwrap_or_default()
}

/// Phase 4-2: 最低賃金違反チェック
pub(crate) fn fetch_wage_compliance(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_hourly_postings, min_wage, below_min_count, below_min_rate, \
        avg_hourly_wage, median_hourly_wage";
    let nat = "emp_group, SUM(total_hourly_postings) as total_hourly_postings, \
        AVG(min_wage) as min_wage, SUM(below_min_count) as below_min_count, \
        CAST(SUM(below_min_count) AS REAL) / SUM(total_hourly_postings) as below_min_rate, \
        AVG(avg_hourly_wage) as avg_hourly_wage, AVG(median_hourly_wage) as median_hourly_wage";
    query_3level(db, "v2_wage_compliance", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group",
        nat, "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group")
}

/// Phase 4-3: 地域ベンチマーク（12軸レーダー用）
pub(crate) fn fetch_region_benchmark(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_competitiveness, job_market_tightness, wage_compliance, \
        industry_diversity, info_transparency, text_urgency, posting_freshness, \
        real_wage_power, labor_fluidity, working_age_ratio, population_growth, foreign_workforce, \
        composite_benchmark";
    let nat = "emp_group, \
        AVG(salary_competitiveness) as salary_competitiveness, \
        AVG(job_market_tightness) as job_market_tightness, \
        AVG(wage_compliance) as wage_compliance, \
        AVG(industry_diversity) as industry_diversity, \
        AVG(info_transparency) as info_transparency, \
        AVG(text_urgency) as text_urgency, \
        AVG(posting_freshness) as posting_freshness, \
        AVG(real_wage_power) as real_wage_power, \
        AVG(labor_fluidity) as labor_fluidity, \
        AVG(working_age_ratio) as working_age_ratio, \
        AVG(population_growth) as population_growth, \
        AVG(foreign_workforce) as foreign_workforce, \
        AVG(composite_benchmark) as composite_benchmark";
    query_3level(db, "v2_region_benchmark", pref, muni,
        cols, "ORDER BY emp_group",
        nat, "GROUP BY emp_group ORDER BY emp_group")
}

/// Phase 4-4: 都道府県別外部指標マスタ（Turso優先）
pub(crate) fn fetch_prefecture_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats ORDER BY prefecture".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_prefecture_stats")
}

/// Phase B: 人口ピラミッドデータ（市区町村レベル、Turso優先）
pub(crate) fn fetch_population_data(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT prefecture, municipality, total_population, male_population, female_population, \
          age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate \
          FROM v2_external_population WHERE prefecture = ?1 AND municipality = ?2".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT ?1 as prefecture, '全体' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT '全国' as prefecture, '' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population")
}

/// Phase B: 人口ピラミッド詳細（5歳階級×男女、Turso優先）
pub(crate) fn fetch_population_pyramid(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> Vec<Row> {
    let order_clause = "ORDER BY CASE age_group \
          WHEN '0-9' THEN 0 WHEN '10-19' THEN 10 WHEN '20-29' THEN 20 \
          WHEN '30-39' THEN 30 WHEN '40-49' THEN 40 WHEN '50-59' THEN 50 \
          WHEN '60-69' THEN 60 WHEN '70-79' THEN 70 WHEN '80+' THEN 80 \
          WHEN '0-14' THEN 0 WHEN '15-64' THEN 15 WHEN '65-74' THEN 65 WHEN '75+' THEN 75 \
          ELSE 999 END";
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT age_group, male_count, female_count \
          FROM v2_external_population_pyramid WHERE prefecture = ?1 AND municipality = ?2 \
          {order_clause}"),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid WHERE prefecture = ?1 \
          GROUP BY age_group \
          {order_clause}"),
         vec![pref.to_string()])
    } else {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid \
          GROUP BY age_group \
          {order_clause}"),
         vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population_pyramid")
}

/// Phase B: 社会動態（転入転出、Turso優先）
pub(crate) fn fetch_migration_data(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT inflow, outflow, net_migration, net_migration_rate \
          FROM v2_external_migration WHERE prefecture = ?1 AND municipality = ?2".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT SUM(inflow) as inflow, SUM(outflow) as outflow, \
          SUM(net_migration) as net_migration, \
          CAST(SUM(net_migration) AS REAL) / NULLIF(SUM(inflow + outflow), 0) * 1000 as net_migration_rate \
          FROM v2_external_migration WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT SUM(inflow) as inflow, SUM(outflow) as outflow, \
          SUM(net_migration) as net_migration, \
          CAST(SUM(net_migration) AS REAL) / NULLIF(SUM(inflow + outflow), 0) * 1000 as net_migration_rate \
          FROM v2_external_migration".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_migration")
}

/// Phase B: 昼夜間人口（Turso優先）
pub(crate) fn fetch_daytime_population(db: &Db, turso: Option<&TursoDb>, pref: &str, muni: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        ("SELECT nighttime_pop, daytime_pop, day_night_ratio, inflow_pop, outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1 AND municipality = ?2".to_string(),
         vec![pref.to_string(), muni.to_string()])
    } else if !pref.is_empty() {
        ("SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_daytime_population")
}

/// Phase 4-5: 有効求人倍率の年度次推移（Turso優先）
/// 全国 + 選択都道府県のデータを取得し、時系列チャートで比較表示する
pub(crate) fn fetch_job_openings_ratio(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_job_openings_ratio")
}

/// 労働市場指標の年度次推移（Turso優先）
pub(crate) fn fetch_labor_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_labor_stats")
}

/// 事業所数データ（都道府県別×産業分類、Turso優先）
pub(crate) fn fetch_establishments(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, industry, establishment_count, reference_year \
          FROM v2_external_establishments \
          WHERE prefecture = ?1 \
          ORDER BY establishment_count DESC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT '全国' as prefecture, industry, SUM(establishment_count) as establishment_count, \
          MAX(reference_year) as reference_year \
          FROM v2_external_establishments \
          GROUP BY industry \
          ORDER BY establishment_count DESC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_establishments")
}

/// 入職率・離職率データ（都道府県別×産業、Turso優先）
pub(crate) fn fetch_turnover(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture IN ('全国', ?1) AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture = '全国' AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_turnover")
}

/// 家計消費支出データ（都道府県別×カテゴリ、Turso優先）
pub(crate) fn fetch_household_spending(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, category, monthly_amount, reference_year \
          FROM v2_external_household_spending \
          WHERE prefecture = ?1 \
          ORDER BY monthly_amount DESC".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国選択時: 全47県の平均を計算
        ("SELECT '全国' as prefecture, category, \
          AVG(monthly_amount) as monthly_amount, MAX(reference_year) as reference_year \
          FROM v2_external_household_spending \
          GROUP BY category \
          ORDER BY monthly_amount DESC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_household_spending")
}

// ======== データ取得: Phase 4-6（事業所動態・気象・介護需要） ========

/// 事業所動態データ（開業率・廃業率、Turso優先）
pub(crate) fn fetch_business_dynamics(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, opening_rate, closure_rate, \
          new_establishments, closed_establishments, net_change \
          FROM v2_external_business_dynamics \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        // 全国: 全都道府県の合計から算出
        ("SELECT '全国' as prefecture, fiscal_year, \
          AVG(opening_rate) as opening_rate, AVG(closure_rate) as closure_rate, \
          SUM(new_establishments) as new_establishments, \
          SUM(closed_establishments) as closed_establishments, \
          SUM(net_change) as net_change \
          FROM v2_external_business_dynamics \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_business_dynamics")
}

/// 気象データ（都道府県別、Turso優先）
pub(crate) fn fetch_climate(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, avg_temperature, max_temperature, \
          min_temperature, snow_days, sunshine_hours, precipitation \
          FROM v2_external_climate \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT '全国' as prefecture, fiscal_year, \
          AVG(avg_temperature) as avg_temperature, \
          MAX(max_temperature) as max_temperature, \
          MIN(min_temperature) as min_temperature, \
          AVG(snow_days) as snow_days, \
          AVG(sunshine_hours) as sunshine_hours, \
          AVG(precipitation) as precipitation \
          FROM v2_external_climate \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_climate")
}

/// 介護需要データ（給付件数・施設数等、Turso優先）
pub(crate) fn fetch_care_demand(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        ("SELECT prefecture, fiscal_year, insurance_benefit_cases, \
          nursing_home_count, health_facility_count, \
          home_care_offices, day_service_offices, \
          pop_65_over, pop_75_over, pop_65_over_rate \
          FROM v2_external_care_demand \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC".to_string(),
         vec![pref.to_string()])
    } else {
        ("SELECT '全国' as prefecture, fiscal_year, \
          SUM(insurance_benefit_cases) as insurance_benefit_cases, \
          SUM(nursing_home_count) as nursing_home_count, \
          SUM(health_facility_count) as health_facility_count, \
          SUM(home_care_offices) as home_care_offices, \
          SUM(day_service_offices) as day_service_offices, \
          SUM(pop_65_over) as pop_65_over, SUM(pop_75_over) as pop_75_over, \
          AVG(pop_65_over_rate) as pop_65_over_rate \
          FROM v2_external_care_demand \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC".to_string(), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_care_demand")
}

// ======== データ取得: Phase 5（予測・推定） ========

/// Phase 5-1: 充足困難度予測
pub(crate) fn fetch_fulfillment_summary(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_count, avg_score, grade_a_pct, grade_b_pct, grade_c_pct, grade_d_pct";
    let nat = "emp_group, SUM(total_count) as total_count, \
        AVG(avg_score) as avg_score, AVG(grade_a_pct) as grade_a_pct, \
        AVG(grade_b_pct) as grade_b_pct, AVG(grade_c_pct) as grade_c_pct, \
        AVG(grade_d_pct) as grade_d_pct";
    query_3level(db, "v2_fulfillment_summary", pref, muni,
        cols, "ORDER BY emp_group",
        nat, "GROUP BY emp_group ORDER BY emp_group")
}

/// Phase 5-2: 地域間流動性推定（市区町村選択時のみ）
pub(crate) fn fetch_mobility_estimate(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    if muni.is_empty() { return vec![]; }
    if !table_exists(db, "v2_mobility_estimate") { return vec![]; }

    let sql = "SELECT emp_group, local_postings, local_avg_salary, gravity_attractiveness, \
               gravity_outflow, net_gravity, top3_destinations \
               FROM v2_mobility_estimate WHERE prefecture = ?1 AND municipality = ?2 \
               ORDER BY emp_group";
    let params = vec![pref.to_string(), muni.to_string()];
    let p: Vec<&dyn rusqlite::types::ToSql> = params.iter().map(|s| s as &dyn rusqlite::types::ToSql).collect();
    db.query(sql, &p).unwrap_or_default()
}

/// Phase 5-3: 給与分位テーブル
pub(crate) fn fetch_shadow_wage(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, salary_type, total_count, p10, p25, p50, p75, p90, mean, stddev, iqr";
    let nat = "emp_group, salary_type, SUM(total_count) as total_count, \
        AVG(p10) as p10, AVG(p25) as p25, AVG(p50) as p50, AVG(p75) as p75, AVG(p90) as p90, \
        AVG(mean) as mean, AVG(stddev) as stddev, AVG(iqr) as iqr";
    query_3level(db, "v2_shadow_wage", pref, muni,
        cols, "AND industry_raw = '' ORDER BY emp_group, salary_type",
        nat, "AND industry_raw = '' GROUP BY emp_group, salary_type ORDER BY emp_group, salary_type")
}
