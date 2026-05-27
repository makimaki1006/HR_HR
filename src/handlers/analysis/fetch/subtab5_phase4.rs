//! サブタブ5 Phase 4: 異常値 + 外部統計データ統合（最賃・違反・地域ベンチマーク・都道府県統計・人口/社会動態/昼夜間人口・求人倍率・労働・事業所・入離職・家計消費・業況・気象・介護需要）

use serde_json::Value;
use std::collections::HashMap;

use super::super::super::helpers::table_exists;
use super::super::super::helpers::normalize_muni_for_external;
use super::{query_3level, query_turso_or_local, EXTERNAL_CLEAN_FILTER};

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;
type Row = HashMap<String, Value>;

pub(crate) fn fetch_anomaly_data(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, metric_name, total_count, anomaly_count, anomaly_rate, \
        avg_value, stddev_value, anomaly_high_count, anomaly_low_count";
    let nat = "emp_group, metric_name, SUM(total_count) as total_count, \
        SUM(anomaly_count) as anomaly_count, \
        CAST(SUM(anomaly_count) AS REAL) / SUM(total_count) as anomaly_rate, \
        AVG(avg_value) as avg_value, AVG(stddev_value) as stddev_value, \
        SUM(anomaly_high_count) as anomaly_high_count, SUM(anomaly_low_count) as anomaly_low_count";
    query_3level(
        db,
        "v2_anomaly_stats",
        pref,
        muni,
        cols,
        "ORDER BY emp_group, metric_name",
        nat,
        "GROUP BY emp_group, metric_name ORDER BY emp_group, metric_name",
    )
}

pub(crate) fn fetch_minimum_wage(db: &Db, pref: &str) -> Vec<Row> {
    if !table_exists(db, "v2_external_minimum_wage") {
        return vec![];
    }

    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, hourly_min_wage \
          FROM v2_external_minimum_wage ORDER BY hourly_min_wage DESC"
                .to_string(),
            vec![],
        )
    };
    let p: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    db.query(&sql, &p).unwrap_or_default()
}

pub(crate) fn fetch_wage_compliance(db: &Db, pref: &str, muni: &str) -> Vec<Row> {
    let cols = "emp_group, total_hourly_postings, min_wage, below_min_count, below_min_rate, \
        avg_hourly_wage, median_hourly_wage";
    let nat = "emp_group, SUM(total_hourly_postings) as total_hourly_postings, \
        AVG(min_wage) as min_wage, SUM(below_min_count) as below_min_count, \
        CAST(SUM(below_min_count) AS REAL) / SUM(total_hourly_postings) as below_min_rate, \
        AVG(avg_hourly_wage) as avg_hourly_wage, AVG(median_hourly_wage) as median_hourly_wage";
    query_3level(
        db,
        "v2_wage_compliance",
        pref,
        muni,
        cols,
        "AND industry_raw = '' ORDER BY emp_group",
        nat,
        "AND industry_raw = '' GROUP BY emp_group ORDER BY emp_group",
    )
}

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
    query_3level(
        db,
        "v2_region_benchmark",
        pref,
        muni,
        cols,
        "ORDER BY emp_group",
        nat,
        "GROUP BY emp_group ORDER BY emp_group",
    )
}

/// 複数都道府県分の region_benchmark を一括取得（survey 媒体分析の主要 3 地域比較用）
///
/// 戻り値は (pref → 正社員行) の Vec。各行は emp_group / *_score 列を持つ。
/// 該当データが無い県は結果に含まれない。
///
/// # 注意
/// - 取得は muni='' (都道府県粒度) のみ。市区町村レベル比較は仕様外。
/// - スコアは 0-1 正規化値の前提（DB 仕様）。表示側で 0-100 に変換すること。
///
/// # 性能 (2026-05-24 監査 C P1 N+1 解消)
/// - 旧実装: prefs.len() N に対し v2_region_benchmark へ N クエリ (`fetch_region_benchmark` を直列呼び出し)
/// - 新実装: `prefecture IN (?, ?, ...)` で 1 クエリにまとめ、Rust 側で pref ごとに group + 正社員行優先選択
/// - 後方互換: 単数版 `fetch_region_benchmark(db, pref, "")` の挙動と等価
///   (空 prefs / table 無し / DB エラー時は空 Vec を返す)
pub(crate) fn fetch_region_benchmarks_for_prefs(db: &Db, prefs: &[String]) -> Vec<(String, Row)> {
    // 空入力 → DB クエリ発火させず空返却 (単数版 ループの空入力等価動作)
    if prefs.is_empty() {
        return Vec::new();
    }
    // テーブル不在 → 空返却 (単数版 query_3level の table_exists ガードと等価)
    if !table_exists(db, "v2_region_benchmark") {
        return Vec::new();
    }

    // 空文字列除外 + 重複除去 (DB クエリにはユニーク値のみ渡す)
    let mut seen = std::collections::HashSet::new();
    let unique_prefs: Vec<&String> = prefs
        .iter()
        .filter(|p| !p.is_empty())
        .filter(|p| seen.insert(p.as_str()))
        .collect();
    if unique_prefs.is_empty() {
        return Vec::new();
    }

    // SELECT 列は単数版 fetch_region_benchmark の cols + prefecture (group 用)
    let placeholders: Vec<String> = (0..unique_prefs.len())
        .map(|i| format!("?{}", i + 1))
        .collect();
    let sql = format!(
        "SELECT prefecture, emp_group, salary_competitiveness, job_market_tightness, wage_compliance, \
         industry_diversity, info_transparency, text_urgency, posting_freshness, \
         real_wage_power, labor_fluidity, working_age_ratio, population_growth, foreign_workforce, \
         composite_benchmark \
         FROM v2_region_benchmark \
         WHERE prefecture IN ({}) AND municipality = '' \
         ORDER BY prefecture, emp_group",
        placeholders.join(",")
    );

    let owned_params: Vec<String> = unique_prefs.iter().map(|p| (**p).clone()).collect();
    let params: Vec<&dyn rusqlite::types::ToSql> = owned_params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let rows = db.query(&sql, &params).unwrap_or_default();
    if rows.is_empty() {
        return Vec::new();
    }

    // prefecture でグループ化 (順序維持のため Vec<(pref, Vec<Row>)>)
    let mut grouped: HashMap<String, Vec<Row>> = HashMap::new();
    for row in rows {
        let pref = row
            .get("prefecture")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if pref.is_empty() {
            continue;
        }
        grouped.entry(pref).or_default().push(row);
    }

    // 入力順を維持しつつ、各 pref について「正社員」行を優先、無ければ先頭行を選択
    // (単数版ループのロジックと等価)
    let mut out: Vec<(String, Row)> = Vec::new();
    for pref in prefs {
        if pref.is_empty() {
            continue;
        }
        // 重複した pref が prefs に複数ある場合、最初の出現に対してのみ出力
        // (単数版ループでは同じ pref に対し毎回 fetch して同じ結果が複数出ていたが、
        //  そもそも呼び出し側は重複 pref を渡さない前提 — 後方互換のため最初のみ採用)
        if let Some(rows_for_pref) = grouped.remove(pref) {
            let chosen = rows_for_pref
                .iter()
                .find(|r| {
                    r.get("emp_group")
                        .and_then(|v| v.as_str())
                        .map(|s| s == "正社員")
                        .unwrap_or(false)
                })
                .cloned()
                .or_else(|| rows_for_pref.first().cloned());
            if let Some(row) = chosen {
                out.push((pref.clone(), row));
            }
        }
    }
    out
}

pub(crate) fn fetch_prefecture_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats WHERE prefecture = ?1"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, unemployment_rate, job_change_desire_rate, non_regular_rate, \
          avg_monthly_wage, price_index, fulfillment_rate, real_wage_index \
          FROM v2_external_prefecture_stats ORDER BY prefecture"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_prefecture_stats")
}

pub(crate) fn fetch_population_data(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    // ヘッダー混入レコード (prefecture='都道府県' / municipality='市区町村') を除外する共通ガード。
    // 詳細: docs/SURVEY_MARKET_INTELLIGENCE_PHASE3_HEADER_FILTER.md
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (format!("SELECT prefecture, municipality, total_population, male_population, female_population, \
          age_0_14, age_15_64, age_65_over, aging_rate, working_age_rate, youth_rate \
          FROM v2_external_population WHERE prefecture = ?1 AND municipality = ?2 AND {}", EXTERNAL_CLEAN_FILTER),
         // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
         vec![pref.to_string(), normalize_muni_for_external(pref, muni)])
    } else if !pref.is_empty() {
        (format!("SELECT ?1 as prefecture, '全体' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population WHERE prefecture = ?1 AND {}", EXTERNAL_CLEAN_FILTER),
         vec![pref.to_string()])
    } else {
        (format!("SELECT '全国' as prefecture, '' as municipality, SUM(total_population) as total_population, \
          SUM(male_population) as male_population, SUM(female_population) as female_population, \
          SUM(age_0_14) as age_0_14, SUM(age_15_64) as age_15_64, SUM(age_65_over) as age_65_over, \
          CAST(SUM(age_65_over) AS REAL) / SUM(total_population) * 100 as aging_rate, \
          CAST(SUM(age_15_64) AS REAL) / SUM(total_population) * 100 as working_age_rate, \
          CAST(SUM(age_0_14) AS REAL) / SUM(total_population) * 100 as youth_rate \
          FROM v2_external_population WHERE {}", EXTERNAL_CLEAN_FILTER), vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population")
}

/// 指定都道府県内で `postings` 件数上位の市区町村名を取得（最大 `limit` 件）。
///
/// 用途: P1-5 Section 06 拡張 (市区町村別ピラミッド 上位 3)。
///
/// 粒度:
/// - `pref` が空: 空 `Vec` を返す（全国集計の場合は muni 別ピラミッドを表示しない）
/// - `pref` 指定: 当該都道府県内 postings を `municipality` で集計し件数降順で `limit` 件返す
///
/// メモリルール準拠:
/// - `feedback_silent_fallback_audit`: クエリ失敗時は警告を出してから空 `Vec` 返却
/// - `feedback_hw_data_scope`: HW 掲載求人 (postings) を母集団とする
pub(crate) fn fetch_top_muni_names(db: &Db, pref: &str, limit: usize) -> Vec<String> {
    if pref.is_empty() || limit == 0 {
        return Vec::new();
    }
    let sql = "SELECT municipality, COUNT(*) as cnt FROM postings \
         WHERE prefecture = ?1 \
           AND municipality IS NOT NULL AND municipality != '' \
         GROUP BY municipality \
         ORDER BY cnt DESC \
         LIMIT ?2";
    let limit_i64 = limit as i64;
    let params: Vec<&dyn rusqlite::types::ToSql> = vec![
        &pref as &dyn rusqlite::types::ToSql,
        &limit_i64 as &dyn rusqlite::types::ToSql,
    ];
    let rows = match db.query(sql, &params) {
        Ok(r) => r,
        Err(e) => {
            // R2-P1-7 (ultrathink Round 2, 2026-05-28): eprintln! → tracing::warn! 統一。
            tracing::warn!(
                "fetch_top_muni_names: postings query failed (pref={}, limit={}): {}",
                pref, limit, e
            );
            return Vec::new();
        }
    };
    use super::super::super::helpers::get_str_ref;
    rows.iter()
        .map(|r| get_str_ref(r, "municipality").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}

pub(crate) fn fetch_population_pyramid(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let order_clause = "ORDER BY CASE age_group \
          WHEN '0-9' THEN 0 WHEN '10-19' THEN 10 WHEN '20-29' THEN 20 \
          WHEN '30-39' THEN 30 WHEN '40-49' THEN 40 WHEN '50-59' THEN 50 \
          WHEN '60-69' THEN 60 WHEN '70-79' THEN 70 WHEN '80+' THEN 80 \
          WHEN '0-14' THEN 0 WHEN '15-64' THEN 15 WHEN '65-74' THEN 65 WHEN '75+' THEN 75 \
          ELSE 999 END";
    // ヘッダー混入レコード除外 (EXTERNAL_CLEAN_FILTER)
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            format!(
                "SELECT age_group, male_count, female_count \
          FROM v2_external_population_pyramid \
          WHERE prefecture = ?1 AND municipality = ?2 AND {EXTERNAL_CLEAN_FILTER} \
          {order_clause}"
            ),
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
    } else if !pref.is_empty() {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid \
          WHERE prefecture = ?1 AND {EXTERNAL_CLEAN_FILTER} \
          GROUP BY age_group \
          {order_clause}"),
         vec![pref.to_string()])
    } else {
        (format!("SELECT age_group, SUM(male_count) as male_count, SUM(female_count) as female_count \
          FROM v2_external_population_pyramid \
          WHERE {EXTERNAL_CLEAN_FILTER} \
          GROUP BY age_group \
          {order_clause}"),
         vec![])
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_population_pyramid")
}

pub(crate) fn fetch_migration_data(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT inflow, outflow, net_migration, net_migration_rate \
          FROM v2_external_migration WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
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

pub(crate) fn fetch_daytime_population(
    db: &Db,
    turso: Option<&TursoDb>,
    pref: &str,
    muni: &str,
) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !muni.is_empty() {
        (
            "SELECT nighttime_pop, daytime_pop, day_night_ratio, inflow_pop, outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1 AND municipality = ?2"
                .to_string(),
            // postings (郡名込み) と v2_external_* (郡名なし) の不一致吸収
            vec![pref.to_string(), normalize_muni_for_external(pref, muni)],
        )
    } else if !pref.is_empty() {
        // 2026-05-24 audit_B P0-2: EXTERNAL_CLEAN_FILTER 適用
        (
            format!(
                "SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population WHERE prefecture = ?1 AND {}",
                EXTERNAL_CLEAN_FILTER
            ),
            vec![pref.to_string()],
        )
    } else {
        (
            format!(
                "SELECT SUM(nighttime_pop) as nighttime_pop, SUM(daytime_pop) as daytime_pop, \
          CAST(SUM(daytime_pop) AS REAL) / NULLIF(SUM(nighttime_pop), 0) * 100 as day_night_ratio, \
          SUM(inflow_pop) as inflow_pop, SUM(outflow_pop) as outflow_pop \
          FROM v2_external_daytime_population WHERE {}",
                EXTERNAL_CLEAN_FILTER
            ),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_daytime_population")
}

pub(crate) fn fetch_job_openings_ratio(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, ratio_total, ratio_excl_part \
          FROM v2_external_job_openings_ratio \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_job_openings_ratio")
}

pub(crate) fn fetch_labor_stats(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture IN ('全国', ?1) \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, unemployment_rate, \
          separation_rate, monthly_salary_male, monthly_salary_female, \
          working_hours_male, working_hours_female, \
          part_time_wage_male, part_time_wage_female \
          FROM v2_external_labor_stats \
          WHERE prefecture = '全国' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_labor_stats")
}

pub(crate) fn fetch_establishments(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, industry_code as industry, industry_name, \
          SUM(establishments) as establishment_count, SUM(employees) as employees, \
          MAX(reference_year) as reference_year \
          FROM v2_external_establishments \
          WHERE prefecture = ?1 AND industry_code <> 'ALL' \
          GROUP BY prefecture, industry_code, industry_name \
          ORDER BY establishment_count DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, industry_code as industry, industry_name, \
          SUM(establishments) as establishment_count, SUM(employees) as employees, \
          MAX(reference_year) as reference_year \
          FROM v2_external_establishments \
          WHERE industry_code <> 'ALL' \
          GROUP BY industry_code, industry_name \
          ORDER BY establishment_count DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_establishments")
}

pub(crate) fn fetch_turnover(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture IN ('全国', ?1) AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT prefecture, fiscal_year, industry, entry_rate, separation_rate, net_rate \
          FROM v2_external_turnover \
          WHERE prefecture = '全国' AND industry = '医療，福祉' \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_turnover")
}

pub(crate) fn fetch_household_spending(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, category, monthly_amount, reference_year \
          FROM v2_external_household_spending \
          WHERE prefecture = ?1 \
          ORDER BY monthly_amount DESC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国選択時: 全47県の平均を計算
        (
            "SELECT '全国' as prefecture, category, \
          AVG(monthly_amount) as monthly_amount, MAX(reference_year) as reference_year \
          FROM v2_external_household_spending \
          GROUP BY category \
          ORDER BY monthly_amount DESC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_household_spending")
}

pub(crate) fn fetch_business_dynamics(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, opening_rate, closure_rate, \
          new_establishments, closed_establishments, net_change \
          FROM v2_external_business_dynamics \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        // 全国: 全都道府県の合計から算出
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          AVG(opening_rate) as opening_rate, AVG(closure_rate) as closure_rate, \
          SUM(new_establishments) as new_establishments, \
          SUM(closed_establishments) as closed_establishments, \
          SUM(net_change) as net_change \
          FROM v2_external_business_dynamics \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_business_dynamics")
}

pub(crate) fn fetch_climate(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, avg_temperature, max_temperature, \
          min_temperature, snow_days, sunshine_hours, precipitation \
          FROM v2_external_climate \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          AVG(avg_temperature) as avg_temperature, \
          MAX(max_temperature) as max_temperature, \
          MIN(min_temperature) as min_temperature, \
          AVG(snow_days) as snow_days, \
          AVG(sunshine_hours) as sunshine_hours, \
          AVG(precipitation) as precipitation \
          FROM v2_external_climate \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_climate")
}

pub(crate) fn fetch_care_demand(db: &Db, turso: Option<&TursoDb>, pref: &str) -> Vec<Row> {
    let (sql, params): (String, Vec<String>) = if !pref.is_empty() {
        (
            "SELECT prefecture, fiscal_year, insurance_benefit_cases, \
          nursing_home_count, health_facility_count, \
          home_care_offices, day_service_offices, \
          pop_65_over, pop_75_over, pop_65_over_rate \
          FROM v2_external_care_demand \
          WHERE prefecture = ?1 \
          ORDER BY fiscal_year ASC"
                .to_string(),
            vec![pref.to_string()],
        )
    } else {
        (
            "SELECT '全国' as prefecture, fiscal_year, \
          SUM(insurance_benefit_cases) as insurance_benefit_cases, \
          SUM(nursing_home_count) as nursing_home_count, \
          SUM(health_facility_count) as health_facility_count, \
          SUM(home_care_offices) as home_care_offices, \
          SUM(day_service_offices) as day_service_offices, \
          SUM(pop_65_over) as pop_65_over, SUM(pop_75_over) as pop_75_over, \
          AVG(pop_65_over_rate) as pop_65_over_rate \
          FROM v2_external_care_demand \
          GROUP BY fiscal_year ORDER BY fiscal_year ASC"
                .to_string(),
            vec![],
        )
    };
    query_turso_or_local(turso, db, &sql, &params, "v2_external_care_demand")
}

/// 散布図用: postings から (下限給与, 上限給与) ペアを最大 `limit` 件取得 (P2-1)
///
/// 用途: navy_report.rs Section 03 図 3-6 散布図 (各点 1 求人)。
///
/// フィルタ条件:
/// - `salary_type = '月給'` (時給/日給/年俸は除外、月給換算は既存パターン踏襲)
/// - `salary_min > 0 AND salary_max > 0` (NULL / 0 を明示除外)
/// - `salary_max >= salary_min` (異常データ除外)
/// - pref/muni が指定されていれば一致条件を AND で追加
///
/// メモリルール準拠:
/// - `feedback_silent_fallback_audit`: クエリ失敗時は警告ログを出して空 `Vec` 返却
/// - `feedback_hw_data_scope`: HW 掲載求人 (postings) を母集団とする
///
/// 戻り値: `Vec<(salary_min_yen, salary_max_yen)>` (円単位、最大 `limit` 件)
pub(crate) fn fetch_salary_scatter_pairs(
    db: &Db,
    pref: &str,
    muni: &str,
    limit: i64,
) -> Vec<(f64, f64)> {
    use super::super::super::helpers::get_f64;

    if limit <= 0 {
        return Vec::new();
    }

    // SQL 構築 (pref/muni を動的に AND 結合)。LIMIT は i64 を直接埋め込み。
    let mut sql = String::from(
        "SELECT salary_min, salary_max FROM postings \
         WHERE salary_type = '月給' \
           AND salary_min IS NOT NULL AND salary_max IS NOT NULL \
           AND salary_min > 0 AND salary_max > 0 \
           AND salary_max >= salary_min",
    );
    let mut params_owned: Vec<String> = Vec::new();
    if !pref.is_empty() {
        params_owned.push(pref.to_string());
        sql.push_str(&format!(" AND prefecture = ?{}", params_owned.len()));
    }
    if !muni.is_empty() {
        params_owned.push(muni.to_string());
        sql.push_str(&format!(" AND municipality = ?{}", params_owned.len()));
    }
    sql.push_str(&format!(" LIMIT {limit}"));

    let params: Vec<&dyn rusqlite::types::ToSql> = params_owned
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            // R2-P1-7 (ultrathink Round 2, 2026-05-28): eprintln! → tracing::warn! 統一。
            tracing::warn!(
                "fetch_salary_scatter_pairs: postings query failed (pref={}, muni={}, limit={}): {}",
                pref, muni, limit, e
            );
            return Vec::new();
        }
    };

    rows.iter()
        .map(|r| (get_f64(r, "salary_min"), get_f64(r, "salary_max")))
        // 二重防衛: SQL 側で除外済みだが get_f64 が NULL→0.0 を返す可能性あり
        .filter(|(lo, hi)| *lo > 0.0 && *hi > 0.0 && *hi >= *lo)
        .collect()
}

// ============================================================
// P2-2 (2026-05-28): CSV 企業別給与ランキング (Section 05 拡張)
// ============================================================

/// CSV (HW 求人) 由来の facility_name 別 給与中央値レコード。
///
/// **データソース**: ローカル SQLite `postings` テーブル (HW 求人スクレイピング結果)。
/// SalesNow API 経由の企業データとは別物。出典は「CSV 求人データ集計」と明記すること。
///
/// 単位: 万円 (postings.salary_min は円単位なので /10000 換算済)。
#[derive(Debug, Clone)]
pub struct CsvCompanySalary {
    /// 施設名 (CSV 実値、匿名化なし。HW 公開情報のため OK)
    pub facility_name: String,
    /// 同施設の求人件数 (代表性確保のため 2 件以上のみ採用)
    pub posting_count: i64,
    /// 下限給与の中央値 (万円)
    pub salary_lower_median: f64,
    /// 上限給与の中央値 (万円)
    pub salary_upper_median: f64,
}

/// f64 列の中央値を計算。`values` は呼び出し側で正値フィルタ済の前提。
///
/// - n=0 → 0.0 (silent fallback ではなく、呼び出し側で空チェックする想定)
/// - n=1 → 値そのまま
/// - n 奇数 → 中央 1 値
/// - n 偶数 → 中央 2 値の平均
fn median_f64(values: &mut [f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    // NaN は除外していないが、呼び出し側で SQL filter 済の前提。
    // NaN が混入していたら sort 結果が未定義になるため `partial_cmp` で防御。
    values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = values.len();
    if n % 2 == 1 {
        values[n / 2]
    } else {
        (values[n / 2 - 1] + values[n / 2]) / 2.0
    }
}

/// CSV (HW 求人) postings から facility_name 別 求人数 + 給与中央値ランキングを取得。
///
/// 用途: navy_report.rs Section 05 表 5-G (企業別給与ランキング)、表 5-H (注目企業リスト)。
///
/// 仕様:
/// - `salary_type = '月給'` (時給/日給/年俸は除外、月給換算は既存 fetch_salary_scatter_pairs 踏襲)
/// - `salary_min > 0 AND salary_max > 0 AND salary_max >= salary_min` (異常データ除外)
/// - `facility_name IS NOT NULL AND facility_name != ''`
/// - pref/muni 指定時のみ一致条件を AND で追加 (空文字列ならフィルタなし)
/// - Rust 側で facility_name 別 GROUP BY + 中央値計算
/// - 求人数 >= 2 の企業のみ採用 (代表性確保、単一求人ノイズ排除)
/// - **上限給与中央値 (`salary_upper_median`) 降順**でソート、上位 `limit` 件を返却
///
/// 単位変換: postings.salary_min/max は円単位 → 戻り値は **万円単位** (/10000)
///
/// メモリルール準拠:
/// - `feedback_silent_fallback_audit`: クエリ失敗時は警告ログを出して空 `Vec` 返却
/// - `feedback_hw_data_scope`: HW 掲載求人 (postings) を母集団とする
/// - `feedback_no_salesnow_mention`: SalesNow 表記は使わず「CSV 求人データ集計」と明記
pub(crate) fn fetch_csv_company_salary_ranking(
    db: &Db,
    pref: &str,
    muni: &str,
    limit: i64,
) -> Vec<CsvCompanySalary> {
    use super::super::super::helpers::{get_f64, get_str};

    if limit <= 0 {
        return Vec::new();
    }

    // SQL 構築: pref/muni を動的に AND 結合 (fetch_salary_scatter_pairs と同パターン)。
    // GROUP BY は Rust 側で行うため、生の facility_name + salary_min/max のみ SELECT。
    let mut sql = String::from(
        "SELECT facility_name, salary_min, salary_max FROM postings \
         WHERE facility_name IS NOT NULL AND facility_name != '' \
           AND salary_type = '月給' \
           AND salary_min IS NOT NULL AND salary_max IS NOT NULL \
           AND salary_min > 0 AND salary_max > 0 \
           AND salary_max >= salary_min",
    );
    let mut params_owned: Vec<String> = Vec::new();
    if !pref.is_empty() {
        params_owned.push(pref.to_string());
        sql.push_str(&format!(" AND prefecture = ?{}", params_owned.len()));
    }
    if !muni.is_empty() {
        params_owned.push(muni.to_string());
        sql.push_str(&format!(" AND municipality = ?{}", params_owned.len()));
    }
    // P0-2 fix (ultrathink Round 1 視点 3): facility_name 別 GROUP BY を Rust 側で行う制約で
    // 全行を Vec<Row> に展開する。月給掲載求人だけでも数万行になりうるため LIMIT 5000 で
    // サンプリング。caption で「直近 N 件サンプル」と明示すること。
    // 注: SQL の LIMIT 5000 は元データ件数の上限、引数 limit は返却企業数の上限 (別概念)。
    sql.push_str(" LIMIT 5000");

    let params: Vec<&dyn rusqlite::types::ToSql> = params_owned
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            // R2-P1-7 (ultrathink Round 2, 2026-05-28): eprintln! → tracing::warn! 統一。
            tracing::warn!(
                "fetch_csv_company_salary_ranking: postings query failed (pref={}, muni={}, limit={}): {}",
                pref, muni, limit, e
            );
            return Vec::new();
        }
    };

    // Rust 側で facility_name 別 GROUP BY。
    // Vec<(salary_min_yen, salary_max_yen)> を facility ごとに集約。
    let mut grouped: HashMap<String, (Vec<f64>, Vec<f64>)> = HashMap::new();
    for r in &rows {
        let name = get_str(r, "facility_name");
        if name.is_empty() {
            continue;
        }
        let lo = get_f64(r, "salary_min");
        let hi = get_f64(r, "salary_max");
        // 二重防衛: SQL 側で除外済みだが get_f64 が NULL→0.0 を返す可能性あり
        if lo <= 0.0 || hi <= 0.0 || hi < lo {
            continue;
        }
        let entry = grouped.entry(name).or_insert_with(|| (Vec::new(), Vec::new()));
        entry.0.push(lo);
        entry.1.push(hi);
    }

    // 求人数 >= 2 の企業のみ採用 + 中央値計算 + 万円換算
    let mut result: Vec<CsvCompanySalary> = grouped
        .into_iter()
        .filter(|(_, (lows, _))| lows.len() >= 2)
        .map(|(name, (mut lows, mut highs))| {
            let posting_count = lows.len() as i64;
            let lower_median_yen = median_f64(&mut lows);
            let upper_median_yen = median_f64(&mut highs);
            CsvCompanySalary {
                facility_name: name,
                posting_count,
                // 円 → 万円 換算
                salary_lower_median: lower_median_yen / 10_000.0,
                salary_upper_median: upper_median_yen / 10_000.0,
            }
        })
        .collect();

    // 上限給与中央値 降順 (同値時は posting_count 降順で安定化)
    result.sort_by(|a, b| {
        b.salary_upper_median
            .partial_cmp(&a.salary_upper_median)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(b.posting_count.cmp(&a.posting_count))
    });
    result.truncate(limit as usize);
    result
}

#[cfg(test)]
mod csv_company_salary_tests {
    use super::*;

    // ---------------- median_f64 ----------------
    #[test]
    fn median_f64_empty_returns_zero() {
        let mut v: Vec<f64> = vec![];
        assert_eq!(median_f64(&mut v), 0.0);
    }

    #[test]
    fn median_f64_single_returns_value() {
        let mut v = vec![250_000.0];
        assert_eq!(median_f64(&mut v), 250_000.0);
    }

    #[test]
    fn median_f64_two_returns_mean() {
        // n=2 偶数 → (a+b)/2
        let mut v = vec![200_000.0, 300_000.0];
        assert_eq!(median_f64(&mut v), 250_000.0);
    }

    #[test]
    fn median_f64_three_returns_middle() {
        // n=3 奇数 → 中央値
        let mut v = vec![100_000.0, 250_000.0, 400_000.0];
        assert_eq!(median_f64(&mut v), 250_000.0);
        // 順序非依存 (sort される)
        let mut v2 = vec![400_000.0, 100_000.0, 250_000.0];
        assert_eq!(median_f64(&mut v2), 250_000.0);
    }

    #[test]
    fn median_f64_four_returns_mean_of_middle_two() {
        // n=4 偶数 → 中央 2 値の平均
        let mut v = vec![100_000.0, 200_000.0, 300_000.0, 400_000.0];
        // 中央 2 値: 200000, 300000 → 平均 250000
        assert_eq!(median_f64(&mut v), 250_000.0);
    }

    #[test]
    fn median_f64_invariant_min_le_median_le_max() {
        // 不変条件: min <= median <= max
        let mut v = vec![123.0, 456.0, 789.0, 1011.0, 1213.0];
        let min = 123.0_f64;
        let max = 1213.0_f64;
        let m = median_f64(&mut v);
        assert!(m >= min && m <= max, "median {m} out of [{min}, {max}]");
    }
}

// ============================================================
// P2-3 (2026-05-28): 求人ターゲット プロファイル (Section 06 拡張)
// ============================================================
//
// 背景: hellowork.db には「求職者個人」テーブルが存在しないため、
//   postings (HW 求人) 側に記載された募集対象条件 (年齢制限 / 給与レンジ / 経験 /
//   雇用形態) を集計し「求人側から見たターゲット プロファイル」として提示する。
//
// 注意 (DISPLAY_SPEC v1.0 §2):
//   求職者「人数」推定は禁止。本構造体が保持するのは **求人件数** の集計のみで、
//   推定母集団人数は一切含まない。Hard NG 用語 (target_count / estimated_population /
//   推定人数 / 想定人数 / 母集団人数) は使わない。

/// 求人側から見たターゲット プロファイル (各分布は (ラベル, 件数) ペア。
/// 件数はすべて **求人件数** であり、求職者人数の推定値ではない)。
#[derive(Debug, Clone, Default)]
pub struct PostingTargetProfile {
    /// 集計対象の総求人件数 (年齢/給与/経験/雇用形態いずれかの集計に含まれる件数の合計ではなく、
    /// 「pref/muni スコープ内の有効な postings 件数」)
    pub total_postings: i64,
    /// 年齢制限の分布。ラベルは age_min/age_max から導出した範囲表記
    /// (例: "〜29歳" / "30〜44歳" / "45〜64歳" / "65歳〜" / "制限なし")。
    pub age_range_distribution: Vec<(String, i64)>,
    /// 月給 (salary_min) の分布。万円単位の区間ラベル (例: "〜20万" / "20〜25万" / ... / "40万〜")。
    pub salary_target_distribution: Vec<(String, i64)>,
    /// 経験要件の分布。"経験不問 (実質)" / "経験記載あり" の 2 値分類
    /// (experience_required が NULL/空文字なら「経験不問 (実質)」)。
    pub experience_required_distribution: Vec<(String, i64)>,
    /// 雇用形態の分布 (postings.employment_type そのまま、空文字は "未記載" に置換)。
    pub employment_type_distribution: Vec<(String, i64)>,
}

/// 年齢制限の単一バケット ラベルを返す。age_min/age_max のいずれかが None でも動作する。
///
/// 分類規則 (上位優先):
/// - age_min/max 両方 None → "制限なし"
/// - age_max が指定 + age_max <= 29 → "〜29歳"
/// - age_min が指定 + age_min >= 65 → "65歳〜"
/// - age_min が指定 + age_min >= 45 → "45〜64歳"
/// - age_max が指定 + age_max <= 44 → "30〜44歳"
/// - それ以外 (15-64 の広域指定など) → "ミドル中心 (30〜44歳含む)"
fn age_range_bucket(age_min: Option<i64>, age_max: Option<i64>) -> &'static str {
    match (age_min, age_max) {
        (None, None) => "制限なし",
        (_, Some(hi)) if hi <= 29 => "〜29歳",
        (Some(lo), _) if lo >= 65 => "65歳〜",
        (Some(lo), _) if lo >= 45 => "45〜64歳",
        (_, Some(hi)) if hi <= 44 => "30〜44歳",
        _ => "ミドル中心 (30〜44歳含む)",
    }
}

/// 月給 (円) を万円単位の区間ラベルへ変換。salary_min を基準にする。
///
/// 区間: "〜20万" / "20〜25万" / "25〜30万" / "30〜35万" / "35〜40万" / "40万〜"
fn salary_bucket(salary_min_yen: f64) -> &'static str {
    let m = salary_min_yen / 10_000.0;
    if m < 20.0 {
        "〜20万"
    } else if m < 25.0 {
        "20〜25万"
    } else if m < 30.0 {
        "25〜30万"
    } else if m < 35.0 {
        "30〜35万"
    } else if m < 40.0 {
        "35〜40万"
    } else {
        "40万〜"
    }
}

/// 年齢制限ラベルを表示順 (若年→高齢) で並べた配列を返す。
/// 表示順固定のため `Vec<(label, count)>` 構築時に使う。
fn age_range_display_order() -> &'static [&'static str] {
    &[
        "〜29歳",
        "30〜44歳",
        "ミドル中心 (30〜44歳含む)",
        "45〜64歳",
        "65歳〜",
        "制限なし",
    ]
}

/// 給与ラベルを表示順 (低→高) で並べた配列を返す。
fn salary_display_order() -> &'static [&'static str] {
    &[
        "〜20万",
        "20〜25万",
        "25〜30万",
        "30〜35万",
        "35〜40万",
        "40万〜",
    ]
}

/// postings から「求人側ターゲット プロファイル」を集計。
///
/// 仕様:
/// - 集計対象は **pref/muni スコープ内の全 postings** (給与/年齢の SQL 制約はかけず、
///   分布側で NULL を「制限なし」「給与未記載」として明示扱いする)。
/// - 給与分布は `salary_type = '月給' AND salary_min > 0` の求人のみカウント
///   (時給/年俸は月給換算せず母集団から除外、salary_target_distribution の合計は
///   `total_postings` と一致しない)。
/// - 経験/雇用形態は NULL/空文字を明示ラベル ("経験不問 (実質)" / "未記載") に置換し、
///   silent fallback (キー消失) を避ける。
/// - 年齢分布は表示順固定 (`age_range_display_order`) で並び、件数 0 のバケットも省略しない
///   (構成比合計 100% を保証するため)。
///
/// メモリルール準拠:
/// - `feedback_silent_fallback_audit`: クエリ失敗時は警告ログを出して default を返却。
/// - `feedback_hw_data_scope`: HW 掲載求人 (postings) を母集団とすることを明示。
/// - `feedback_never_guess_data`: 求職者人数の推定は行わず、求人件数のみを集計。
pub(crate) fn fetch_posting_target_profile(
    db: &Db,
    pref: &str,
    muni: &str,
) -> PostingTargetProfile {
    use super::super::super::helpers::{get_f64, get_str};

    // ---- SQL: postings から age_min/age_max/salary_min/salary_type/experience_required/employment_type を SELECT
    let mut sql = String::from(
        "SELECT age_min, age_max, salary_min, salary_type, \
                experience_required, employment_type \
         FROM postings WHERE 1=1",
    );
    let mut params_owned: Vec<String> = Vec::new();
    if !pref.is_empty() {
        params_owned.push(pref.to_string());
        sql.push_str(&format!(" AND prefecture = ?{}", params_owned.len()));
    }
    if !muni.is_empty() {
        params_owned.push(muni.to_string());
        sql.push_str(&format!(" AND municipality = ?{}", params_owned.len()));
    }
    // P0-1 fix (ultrathink Round 1 視点 3): pref フィルタなしの全国モードや大都市圏で
    // 数十万行を Rust Vec<Row> に展開し Render 512MB を圧迫するリスクがあるため
    // 直近 10,000 件にサンプリング。caption で「直近 N 件サンプル」と明示すること。
    sql.push_str(" LIMIT 10000");

    let params: Vec<&dyn rusqlite::types::ToSql> = params_owned
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = match db.query(&sql, &params) {
        Ok(r) => r,
        Err(e) => {
            // R2-P1-7 (ultrathink Round 2, 2026-05-28): eprintln! → tracing::warn! 統一。
            tracing::warn!(
                "fetch_posting_target_profile: postings query failed (pref={}, muni={}): {}",
                pref, muni, e
            );
            return PostingTargetProfile::default();
        }
    };

    // 集計バッファ (順序保持の Vec ではなく HashMap で集計し、最後に表示順で並べ替える)
    let mut age_counts: HashMap<&'static str, i64> = HashMap::new();
    for label in age_range_display_order() {
        age_counts.insert(*label, 0);
    }
    let mut salary_counts: HashMap<&'static str, i64> = HashMap::new();
    for label in salary_display_order() {
        salary_counts.insert(*label, 0);
    }
    // 経験要件は 2 値固定 (未記載=「経験不問 (実質)」/ 記載あり=「経験記載あり」)
    let mut exp_unspec: i64 = 0;
    let mut exp_specified: i64 = 0;
    // 雇用形態は動的 (postings.employment_type の値域に依存)
    let mut emp_counts: HashMap<String, i64> = HashMap::new();

    for r in &rows {
        // 年齢
        let age_min_opt: Option<i64> = r.get("age_min").and_then(|v| v.as_i64());
        let age_max_opt: Option<i64> = r.get("age_max").and_then(|v| v.as_i64());
        let age_label = age_range_bucket(age_min_opt, age_max_opt);
        *age_counts.entry(age_label).or_insert(0) += 1;

        // 給与 (salary_type='月給' AND salary_min>0 のみ)
        let salary_type = get_str(r, "salary_type");
        let salary_min = get_f64(r, "salary_min");
        if salary_type == "月給" && salary_min > 0.0 {
            let label = salary_bucket(salary_min);
            *salary_counts.entry(label).or_insert(0) += 1;
        }

        // 経験要件
        let exp = get_str(r, "experience_required");
        if exp.trim().is_empty() {
            exp_unspec += 1;
        } else {
            exp_specified += 1;
        }

        // 雇用形態
        let emp = get_str(r, "employment_type");
        let emp_label = if emp.trim().is_empty() {
            "未記載".to_string()
        } else {
            emp
        };
        *emp_counts.entry(emp_label).or_insert(0) += 1;
    }

    // 表示順固定の Vec を組み立て
    let age_distribution: Vec<(String, i64)> = age_range_display_order()
        .iter()
        .map(|l| ((*l).to_string(), *age_counts.get(*l).unwrap_or(&0)))
        .collect();
    let salary_distribution: Vec<(String, i64)> = salary_display_order()
        .iter()
        .map(|l| ((*l).to_string(), *salary_counts.get(*l).unwrap_or(&0)))
        .collect();
    let exp_distribution: Vec<(String, i64)> = vec![
        ("経験不問 (実質)".to_string(), exp_unspec),
        ("経験記載あり".to_string(), exp_specified),
    ];

    // 雇用形態は件数降順
    let mut emp_distribution: Vec<(String, i64)> = emp_counts.into_iter().collect();
    emp_distribution.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

    PostingTargetProfile {
        total_postings: rows.len() as i64,
        age_range_distribution: age_distribution,
        salary_target_distribution: salary_distribution,
        experience_required_distribution: exp_distribution,
        employment_type_distribution: emp_distribution,
    }
}

#[cfg(test)]
mod posting_target_profile_tests {
    use super::*;

    // ---------------- age_range_bucket ----------------
    #[test]
    fn age_bucket_both_none_returns_unrestricted() {
        assert_eq!(age_range_bucket(None, None), "制限なし");
    }

    #[test]
    fn age_bucket_max_le_29_returns_young() {
        assert_eq!(age_range_bucket(None, Some(29)), "〜29歳");
        assert_eq!(age_range_bucket(Some(18), Some(25)), "〜29歳");
    }

    #[test]
    fn age_bucket_min_ge_65_returns_senior() {
        assert_eq!(age_range_bucket(Some(65), None), "65歳〜");
        assert_eq!(age_range_bucket(Some(70), Some(75)), "65歳〜");
    }

    #[test]
    fn age_bucket_min_ge_45_returns_late_career() {
        assert_eq!(age_range_bucket(Some(45), None), "45〜64歳");
        assert_eq!(age_range_bucket(Some(50), Some(60)), "45〜64歳");
    }

    #[test]
    fn age_bucket_max_le_44_returns_target() {
        assert_eq!(age_range_bucket(None, Some(44)), "30〜44歳");
        assert_eq!(age_range_bucket(Some(30), Some(40)), "30〜44歳");
    }

    #[test]
    fn age_bucket_18_59_returns_middle_focus() {
        // age_min=18, age_max=59 → どの上位条件にもマッチせず middle にフォール
        assert_eq!(
            age_range_bucket(Some(18), Some(59)),
            "ミドル中心 (30〜44歳含む)"
        );
    }

    // ---------------- salary_bucket ----------------
    #[test]
    fn salary_bucket_under_20man() {
        assert_eq!(salary_bucket(180_000.0), "〜20万");
        assert_eq!(salary_bucket(0.0), "〜20万"); // 境界 (呼び出し側でフィルタ済の想定)
    }

    #[test]
    fn salary_bucket_20_25_range() {
        assert_eq!(salary_bucket(200_000.0), "20〜25万");
        assert_eq!(salary_bucket(249_000.0), "20〜25万");
    }

    #[test]
    fn salary_bucket_25_30_range() {
        assert_eq!(salary_bucket(250_000.0), "25〜30万");
        assert_eq!(salary_bucket(299_999.0), "25〜30万");
    }

    #[test]
    fn salary_bucket_40man_plus() {
        assert_eq!(salary_bucket(400_000.0), "40万〜");
        assert_eq!(salary_bucket(800_000.0), "40万〜");
    }

    // ---------------- display order coverage ----------------
    #[test]
    fn age_display_order_covers_all_buckets() {
        // 想定し得る (age_min, age_max) 組合せのいずれもが表示順 Vec のラベルにマッチすること
        let cases: Vec<(Option<i64>, Option<i64>)> = vec![
            (None, None),
            (None, Some(29)),
            (Some(18), Some(25)),
            (Some(65), None),
            (Some(70), Some(75)),
            (Some(45), None),
            (Some(50), Some(60)),
            (None, Some(44)),
            (Some(30), Some(40)),
            (Some(18), Some(59)),
            (Some(18), Some(64)),
        ];
        let order: Vec<&'static str> = age_range_display_order().to_vec();
        for c in cases {
            let label = age_range_bucket(c.0, c.1);
            assert!(
                order.contains(&label),
                "{:?} → label {label} not in display order",
                c
            );
        }
    }

    // ---------------- 構成比合計 100% 不変条件 ----------------
    #[test]
    fn empty_profile_default_yields_zero_total() {
        let p = PostingTargetProfile::default();
        assert_eq!(p.total_postings, 0);
        assert_eq!(p.age_range_distribution.len(), 0);
        assert_eq!(p.salary_target_distribution.len(), 0);
        assert_eq!(p.experience_required_distribution.len(), 0);
        assert_eq!(p.employment_type_distribution.len(), 0);
    }

    /// 年齢分布の和が total_postings と一致することの確認 (シミュレーション)。
    /// fetch_posting_target_profile は DB が要るため、ここでは age_range_bucket の
    /// 全網羅性 (どんな入力でも必ずいずれかのラベルに分類される) を検証することで
    /// 「構成比合計 100%」不変条件を間接的に保証する。
    #[test]
    fn age_bucket_total_invariant_over_synthetic_inputs() {
        let inputs: Vec<(Option<i64>, Option<i64>)> = vec![
            (None, None),
            (None, Some(20)),
            (None, Some(30)),
            (None, Some(44)),
            (None, Some(50)),
            (None, Some(64)),
            (Some(18), None),
            (Some(18), Some(40)),
            (Some(18), Some(59)),
            (Some(18), Some(64)),
            (Some(40), Some(60)),
            (Some(45), Some(60)),
            (Some(50), Some(70)),
            (Some(65), None),
            (Some(70), None),
        ];
        let n = inputs.len() as i64;
        let order: Vec<&'static str> = age_range_display_order().to_vec();
        let mut counts: HashMap<&'static str, i64> = HashMap::new();
        for l in &order {
            counts.insert(*l, 0);
        }
        for (lo, hi) in inputs {
            let label = age_range_bucket(lo, hi);
            *counts.entry(label).or_insert(0) += 1;
        }
        let sum: i64 = counts.values().sum();
        assert_eq!(sum, n, "age bucket total must equal input count");
    }

    /// salary_bucket の全網羅性: 任意の正の f64 が必ず 6 区間のいずれかに分類される。
    #[test]
    fn salary_bucket_total_invariant_over_synthetic_inputs() {
        let inputs: Vec<f64> = vec![
            150_000.0, 200_000.0, 220_000.0, 250_000.0, 280_000.0, 300_000.0, 320_000.0, 350_000.0,
            380_000.0, 400_000.0, 500_000.0, 1_000_000.0,
        ];
        let order: Vec<&'static str> = salary_display_order().to_vec();
        let mut counts: HashMap<&'static str, i64> = HashMap::new();
        for l in &order {
            counts.insert(*l, 0);
        }
        for v in &inputs {
            let label = salary_bucket(*v);
            *counts.entry(label).or_insert(0) += 1;
        }
        let sum: i64 = counts.values().sum();
        assert_eq!(sum, inputs.len() as i64);
    }
}
