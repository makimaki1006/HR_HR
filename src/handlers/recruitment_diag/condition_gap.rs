//! Panel 5: 条件ギャップ診断
//!
//! HW postings から指定業界 (job_type) 内と全業界の中央値
//! (年収・年休・賞与月数) を算出し、自社条件との差分を計算する。
//!
//! 年収計算式: annual_income = salary_min × (12 + bonus_months)
//! - 月給換算 12ヶ月分 + 賞与 (月数) 分を加算
//!
//! データ範囲制約 (feedback_hw_data_scope):
//! - HW 掲載求人は全求人市場ではない。
//! - HW 慣習として市場実勢より給与を低めに出すケースあり。
//! - 中央値 vs 自社の差分は相関指標。因果 (給与を上げれば応募増) は保証しない。

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::get_f64;
use crate::handlers::recruitment_diag::competitors::{hw_data_scope_warning, prefcode_to_name};
use crate::AppState;
use axum::extract::{Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use ts_rs::TS;

#[derive(Deserialize)]
pub struct ConditionGapQuery {
    #[serde(default)]
    pub job_type: String,
    #[serde(default)]
    pub emp_type: String, // 例: "正社員"
    pub prefcode: Option<i32>,
    #[serde(default)]
    pub municipality: String,
    /// 自社条件
    pub company_salary_min: Option<f64>,
    pub company_bonus_months: Option<f64>,
    pub company_annual_holidays: Option<f64>,
}

/// 中央値統計
#[derive(Debug, Serialize, Default, Clone, TS)]
#[ts(rename = "RdMedianStats")]
pub struct MedianStats {
    /// 推定年収中央値 (円) = 月給中央値 × (12 + 賞与月数中央値)。データ無しは 0.0
    pub annual_income: f64,
    /// 年間休日中央値 (日)。データ無しは 0.0
    pub annual_holidays: f64,
    /// 賞与月数中央値 (ヶ月)。データ無しは 0.0
    pub bonus_months: f64,
    /// 母集団の求人件数 (月給・salary_min > 0 の HW 求人)
    pub sample_size: i64,
}

/// 自社条件 (クエリ値。未指定は 0.0)
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdConditionGapCompany {
    /// 自社推定年収 (円) = 月給 × (12 + 賞与月数)。月給 0 以下は 0.0
    pub annual_income_estimated: f64,
    /// 自社年間休日 (日)
    pub annual_holidays: f64,
    /// 自社賞与月数 (ヶ月)
    pub bonus_months: f64,
    /// 自社月給下限 (円)
    pub salary_min: f64,
}

/// Panel 5 成功時の本体
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdConditionGapResponse {
    /// 都道府県名 (prefcode 不正・未指定は空文字)
    pub prefecture: String,
    /// 市区町村名 (クエリの値そのまま)
    pub municipality: String,
    /// HW 職種名 (クエリの値そのまま)
    pub job_type: String,
    /// 雇用形態 (クエリの値そのまま。例: "正社員")
    pub emp_type: String,
    /// 指定業界の中央値
    pub industry_median: MedianStats,
    /// 全業界の中央値
    pub all_industry_median: MedianStats,
    /// 自社条件
    pub company: RdConditionGapCompany,
    /// 自社 − 業界中央値
    pub gap_industry: Gap,
    /// 自社 − 全業界中央値
    pub gap_all: Gap,
    /// 解釈テキスト
    pub interpretation: String,
    /// HW データ範囲の注意書き
    pub warning: String,
}

/// Panel 5 エラー時の本体 (HW DB 未接続)
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdConditionGapError {
    /// エラーメッセージ
    pub error: String,
    /// 常に全項目 0
    pub industry_median: MedianStats,
    /// 常に全項目 0
    pub all_industry_median: MedianStats,
}

/// Panel 5 の応答 (TS では `RdConditionGapResponse | RdConditionGapError`)
#[derive(Debug, Clone, Serialize, TS)]
#[serde(untagged)]
pub enum RdConditionGapResult {
    Ok(RdConditionGapResponse),
    Err(RdConditionGapError),
}

/// 成功本体を組み立てる (ハンドラから切り出した純関数)
#[allow(clippy::too_many_arguments)]
pub(crate) fn build_response(
    prefecture: String,
    municipality: String,
    job_type: String,
    emp_type: String,
    industry_median: MedianStats,
    all_industry_median: MedianStats,
    company_salary_min: f64,
    company_bonus: f64,
    company_holidays: f64,
) -> RdConditionGapResponse {
    let company_annual_income = compute_annual_income(company_salary_min, company_bonus);

    let gap_industry = compute_gap(
        company_annual_income,
        company_holidays,
        company_bonus,
        &industry_median,
    );
    let gap_all = compute_gap(
        company_annual_income,
        company_holidays,
        company_bonus,
        &all_industry_median,
    );

    let interpretation =
        build_interpretation(&gap_industry, &industry_median, &job_type, &prefecture);

    RdConditionGapResponse {
        prefecture,
        municipality,
        job_type,
        emp_type,
        industry_median,
        all_industry_median,
        company: RdConditionGapCompany {
            annual_income_estimated: company_annual_income,
            annual_holidays: company_holidays,
            bonus_months: company_bonus,
            salary_min: company_salary_min,
        },
        gap_industry,
        gap_all,
        interpretation,
        warning: hw_data_scope_warning(),
    }
}

/// GET /api/recruitment_diag/condition_gap
pub async fn condition_gap(
    State(state): State<Arc<AppState>>,
    Query(q): Query<ConditionGapQuery>,
) -> Json<RdConditionGapResult> {
    let prefecture = prefcode_to_name(q.prefcode).unwrap_or_default();

    let db = match &state.hw_db {
        Some(db) => db.clone(),
        None => return Json(error_response("HW DB 未接続")),
    };

    let job_type = q.job_type.clone();
    let emp_type = q.emp_type.clone();
    let muni = q.municipality.clone();
    let pref_snap = prefecture.clone();

    let (industry_median, all_industry_median) = tokio::task::spawn_blocking(move || {
        let ind = compute_median(&db, &job_type, &emp_type, &pref_snap, &muni);
        let all = compute_median(&db, "", &emp_type, &pref_snap, &muni);
        (ind, all)
    })
    .await
    .unwrap_or_default();

    // 自社条件 (未指定は 0.0)
    let company_salary_min = q.company_salary_min.unwrap_or(0.0);
    let company_bonus = q.company_bonus_months.unwrap_or(0.0);
    let company_holidays = q.company_annual_holidays.unwrap_or(0.0);

    Json(RdConditionGapResult::Ok(build_response(
        prefecture,
        q.municipality,
        q.job_type,
        q.emp_type,
        industry_median,
        all_industry_median,
        company_salary_min,
        company_bonus,
        company_holidays,
    )))
}

/// 年収 = 月給 × (12 + 賞与月数)
pub(crate) fn compute_annual_income(salary_min: f64, bonus_months: f64) -> f64 {
    if salary_min <= 0.0 {
        return 0.0;
    }
    let bonus = if bonus_months.is_finite() && bonus_months >= 0.0 {
        bonus_months
    } else {
        0.0
    };
    salary_min * (12.0 + bonus)
}

/// ギャップ計算構造 (自社 − 中央値)
#[derive(Debug, Serialize, Default, Clone, TS)]
#[ts(rename = "RdConditionGapDiff")]
pub struct Gap {
    /// 年収差 (円)
    pub annual_income_diff: f64,
    /// 年収差の中央値比 (%)。中央値 0 以下は 0.0
    pub annual_income_pct: f64,
    /// 年間休日差 (日)
    pub annual_holidays_diff: f64,
    /// 賞与月数差 (ヶ月)
    pub bonus_months_diff: f64,
}

fn compute_gap(
    company_annual_income: f64,
    company_holidays: f64,
    company_bonus: f64,
    median: &MedianStats,
) -> Gap {
    let ai_diff = company_annual_income - median.annual_income;
    let ai_pct = if median.annual_income > 0.0 {
        ai_diff / median.annual_income * 100.0
    } else {
        0.0
    };
    Gap {
        annual_income_diff: ai_diff,
        annual_income_pct: ai_pct,
        annual_holidays_diff: company_holidays - median.annual_holidays,
        bonus_months_diff: company_bonus - median.bonus_months,
    }
}

/// postings から中央値を算出
/// job_type 空文字なら全業界対象
pub(crate) fn compute_median(
    db: &LocalDb,
    job_type: &str,
    emp_type: &str,
    pref: &str,
    muni: &str,
) -> MedianStats {
    // where 構築
    let mut wc: Vec<String> = vec![
        "salary_min > 0".to_string(),
        "salary_type = '月給'".to_string(),
    ];
    let mut params_own: Vec<String> = Vec::new();
    let mut idx: usize = 1;

    if !job_type.is_empty() {
        wc.push(format!("job_type = ?{}", idx));
        params_own.push(job_type.to_string());
        idx += 1;
    }
    // Panel 5 修正 (2026-04-26 / P2 #9): UI 値「パート」「その他」を DB の実値リストに展開する
    // 修正前: emp_type="パート" → "employment_type = 'パート'" → ヒット 0 件
    // 修正後: emp_type="パート" → IN ('パート労働者', '有期雇用派遣パート', '無期雇用派遣パート')
    if !emp_type.is_empty() {
        let expanded = crate::handlers::emp_classifier::from_ui_value(emp_type)
            .map(crate::handlers::emp_classifier::expand_to_db_values)
            .unwrap_or_default();
        if expanded.is_empty() {
            // 既知の UI 3 値以外 (空文字含まず) はそのままマッチ (後方互換)
            wc.push(format!("employment_type = ?{}", idx));
            params_own.push(emp_type.to_string());
            idx += 1;
        } else if expanded.len() == 1 {
            wc.push(format!("employment_type = ?{}", idx));
            params_own.push(expanded[0].to_string());
            idx += 1;
        } else {
            let placeholders: Vec<String> = (0..expanded.len())
                .map(|i| format!("?{}", idx + i))
                .collect();
            wc.push(format!("employment_type IN ({})", placeholders.join(", ")));
            for v in expanded {
                params_own.push(v.to_string());
                idx += 1;
            }
        }
    }
    if !pref.is_empty() {
        wc.push(format!("prefecture = ?{}", idx));
        params_own.push(pref.to_string());
        idx += 1;
    }
    if !muni.is_empty() {
        wc.push(format!("municipality = ?{}", idx));
        params_own.push(muni.to_string());
        #[allow(unused_assignments)]
        {
            idx += 1;
        }
    }

    let where_sql = wc.join(" AND ");

    // 統計値: AVG を中央値の代替として使う (SQLite に median 関数が無いため)
    // より正確な中央値は ORDER BY LIMIT 1 OFFSET N/2 で別取得できるが、
    // リクエストあたりクエリ数削減のため AVG で近似する (大規模母集団では近似誤差は小)
    // 正確な中央値取得は別ヘルパで実装
    let sql = format!(
        "SELECT \
         COUNT(*) as cnt, \
         AVG(salary_min) as avg_salary, \
         AVG(CASE WHEN bonus_months > 0 THEN bonus_months END) as avg_bonus, \
         AVG(CASE WHEN annual_holidays > 0 THEN annual_holidays END) as avg_holidays \
         FROM postings WHERE {where_sql}"
    );

    // 平均から中央値近似を取得し、さらにより正確な中央値計算
    let params: Vec<&dyn rusqlite::types::ToSql> = params_own
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let avg_row = db
        .query(&sql, &params)
        .ok()
        .and_then(|r| r.into_iter().next());
    let (cnt, avg_salary, avg_bonus, avg_holidays) = if let Some(r) = avg_row {
        (
            crate::handlers::helpers::get_i64(&r, "cnt"),
            get_f64(&r, "avg_salary"),
            get_f64(&r, "avg_bonus"),
            get_f64(&r, "avg_holidays"),
        )
    } else {
        (0, 0.0, 0.0, 0.0)
    };

    if cnt == 0 {
        return MedianStats::default();
    }

    // より正確な中央値 (salary_min のみ、負荷を考慮)
    let median_salary =
        median_via_offset(db, &where_sql, &params_own, "salary_min").unwrap_or(avg_salary);
    let median_bonus = median_via_offset(
        db,
        &format!("{} AND bonus_months > 0", where_sql),
        &params_own,
        "bonus_months",
    )
    .unwrap_or(avg_bonus);
    let median_holidays = median_via_offset(
        db,
        &format!("{} AND annual_holidays > 0", where_sql),
        &params_own,
        "annual_holidays",
    )
    .unwrap_or(avg_holidays);

    // 年収中央値 = 月給中央値 × (12 + 賞与中央値)
    let annual_income = compute_annual_income(median_salary, median_bonus);

    MedianStats {
        annual_income,
        annual_holidays: median_holidays,
        bonus_months: median_bonus,
        sample_size: cnt,
    }
}

/// ORDER BY LIMIT 1 OFFSET N/2 で中央値取得
fn median_via_offset(
    db: &LocalDb,
    where_sql: &str,
    params_own: &[String],
    column: &str,
) -> Option<f64> {
    // column は内部指定のみなので SQL インジェクション対象外 (ホワイトリスト制御)
    if !["salary_min", "bonus_months", "annual_holidays"].contains(&column) {
        return None;
    }

    // 件数取得
    let count_sql = format!("SELECT COUNT(*) FROM postings WHERE {where_sql}");
    let params: Vec<&dyn rusqlite::types::ToSql> = params_own
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();
    let cnt: i64 = db.query_scalar(&count_sql, &params).ok()?;
    if cnt == 0 {
        return None;
    }
    let offset = cnt / 2;
    let median_sql = format!(
        "SELECT {column} as v FROM postings WHERE {where_sql} ORDER BY {column} LIMIT 1 OFFSET {offset}"
    );
    let rows = db.query(&median_sql, &params).ok()?;
    let first = rows.first()?;
    Some(get_f64(first, "v"))
}

/// 解釈テキスト生成
fn build_interpretation(gap: &Gap, median: &MedianStats, job_type: &str, pref: &str) -> String {
    if median.sample_size == 0 {
        return "該当条件での HW 求人データが不足しており、比較できませんでした。".to_string();
    }

    let region = if pref.is_empty() {
        "全国".to_string()
    } else {
        pref.to_string()
    };
    let industry = if job_type.is_empty() {
        "全業界".to_string()
    } else {
        job_type.to_string()
    };

    let income_label = if gap.annual_income_diff > 0.0 {
        format!(
            "御社推定年収は業界中央値より {:.0}円 ({:.1}%) 上回る傾向",
            gap.annual_income_diff, gap.annual_income_pct
        )
    } else if gap.annual_income_diff < 0.0 {
        format!(
            "御社推定年収は業界中央値より {:.0}円 ({:.1}%) 下回る傾向",
            gap.annual_income_diff.abs(),
            gap.annual_income_pct.abs()
        )
    } else {
        "御社推定年収は業界中央値と同水準".to_string()
    };

    let holiday_label = if gap.annual_holidays_diff.abs() < 1.0 {
        "年間休日は業界中央値とほぼ同水準".to_string()
    } else if gap.annual_holidays_diff > 0.0 {
        format!(
            "年間休日は業界中央値より {:.0}日多い傾向",
            gap.annual_holidays_diff
        )
    } else {
        format!(
            "年間休日は業界中央値より {:.0}日少ない傾向",
            gap.annual_holidays_diff.abs()
        )
    };

    format!(
        "【{region}・{industry}】{income_label}。{holiday_label}。\
        サンプル数 {sample}件。\
        ※中央値は HW 掲載求人のみから算出。市場全体の実勢ではない。",
        sample = median.sample_size
    )
}

fn error_response(msg: &str) -> RdConditionGapResult {
    RdConditionGapResult::Err(RdConditionGapError {
        error: msg.to_string(),
        industry_median: MedianStats::default(),
        all_industry_median: MedianStats::default(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // 逆証明: 具体的な自社条件/HW分布で期待年収差を手計算検証
    #[test]
    fn annual_income_basic() {
        // 月給 25万, 賞与 2.5ヶ月
        // 期待: 250000 × (12 + 2.5) = 250000 × 14.5 = 3,625,000
        let ai = compute_annual_income(250_000.0, 2.5);
        assert!((ai - 3_625_000.0).abs() < 0.01, "ai={}", ai);
    }

    #[test]
    fn annual_income_no_bonus() {
        // 月給 20万, 賞与 0ヶ月
        // 期待: 200000 × 12 = 2,400,000
        let ai = compute_annual_income(200_000.0, 0.0);
        assert!((ai - 2_400_000.0).abs() < 0.01);
    }

    #[test]
    fn annual_income_zero_salary() {
        assert_eq!(compute_annual_income(0.0, 2.0), 0.0);
    }

    #[test]
    fn annual_income_negative_bonus_clamped() {
        // 負の賞与月数は 0 扱い
        // 期待: 200000 × 12 = 2,400,000
        assert_eq!(compute_annual_income(200_000.0, -1.0), 2_400_000.0);
    }

    #[test]
    fn gap_positive() {
        // 自社年収 4,000,000 vs 中央値 3,500,000
        // 期待: diff=500,000, pct = 500,000/3,500,000 × 100 ≈ 14.285%
        let median = MedianStats {
            annual_income: 3_500_000.0,
            annual_holidays: 108.0,
            bonus_months: 2.8,
            sample_size: 1000,
        };
        let g = compute_gap(4_000_000.0, 120.0, 2.5, &median);
        assert!((g.annual_income_diff - 500_000.0).abs() < 0.01);
        assert!((g.annual_income_pct - 14.285_714).abs() < 0.01);
        // 年休 120 vs 108 → 差 +12
        assert!((g.annual_holidays_diff - 12.0).abs() < 0.001);
        // 賞与 2.5 vs 2.8 → 差 -0.3
        assert!((g.bonus_months_diff - (-0.3)).abs() < 0.001);
    }

    #[test]
    fn gap_negative() {
        // 自社 3,000,000 vs 中央値 3,500,000
        // 期待: diff=-500,000, pct = -14.285%
        let median = MedianStats {
            annual_income: 3_500_000.0,
            annual_holidays: 108.0,
            bonus_months: 2.8,
            sample_size: 1000,
        };
        let g = compute_gap(3_000_000.0, 105.0, 2.0, &median);
        assert!((g.annual_income_diff - (-500_000.0)).abs() < 0.01);
        assert!((g.annual_income_pct - (-14.285_714)).abs() < 0.01);
    }

    #[test]
    fn gap_zero_median() {
        // median=0 (データ無し) では pct = 0 で NaN/Inf を避ける
        let median = MedianStats::default();
        let g = compute_gap(3_000_000.0, 100.0, 2.0, &median);
        assert_eq!(g.annual_income_pct, 0.0);
    }

    #[test]
    fn interpretation_no_sample() {
        let gap = Gap::default();
        let median = MedianStats::default();
        let msg = build_interpretation(&gap, &median, "医療", "東京都");
        assert!(msg.contains("不足"));
    }

    #[test]
    fn interpretation_with_positive_gap() {
        // 年収 +500,000 差、+5.0%、年休 +12 日
        let median = MedianStats {
            annual_income: 3_500_000.0,
            annual_holidays: 108.0,
            bonus_months: 2.5,
            sample_size: 1234,
        };
        let gap = Gap {
            annual_income_diff: 500_000.0,
            annual_income_pct: 14.3,
            annual_holidays_diff: 12.0,
            bonus_months_diff: 0.0,
        };
        let msg = build_interpretation(&gap, &median, "医療", "東京都");
        assert!(msg.contains("上回る"));
        assert!(msg.contains("東京都"));
        assert!(msg.contains("医療"));
        assert!(msg.contains("1234"));
    }

    // ============================================================
    // Phase 1A-1: struct 置き換え前の json!() との等価テスト
    // ============================================================

    use serde_json::{json, Value};

    /// 置き換え前の `condition_gap` 成功時の組み立て (json!() 式はそのまま残す)
    #[allow(clippy::too_many_arguments)]
    fn legacy_condition_gap_json(
        prefecture: String,
        municipality: String,
        job_type: String,
        emp_type: String,
        industry_median: MedianStats,
        all_industry_median: MedianStats,
        company_salary_min: f64,
        company_bonus: f64,
        company_holidays: f64,
    ) -> Value {
        let company_annual_income = compute_annual_income(company_salary_min, company_bonus);
        let gap_industry = compute_gap(
            company_annual_income,
            company_holidays,
            company_bonus,
            &industry_median,
        );
        let gap_all = compute_gap(
            company_annual_income,
            company_holidays,
            company_bonus,
            &all_industry_median,
        );
        let interpretation =
            build_interpretation(&gap_industry, &industry_median, &job_type, &prefecture);
        json!({
            "prefecture": prefecture,
            "municipality": municipality,
            "job_type": job_type,
            "emp_type": emp_type,
            "industry_median": industry_median,
            "all_industry_median": all_industry_median,
            "company": {
                "annual_income_estimated": company_annual_income,
                "annual_holidays": company_holidays,
                "bonus_months": company_bonus,
                "salary_min": company_salary_min,
            },
            "gap_industry": gap_industry,
            "gap_all": gap_all,
            "interpretation": interpretation,
            "warning": hw_data_scope_warning(),
        })
    }

    /// 置き換え前の `error_response` (式はそのまま残す)
    fn legacy_error_response(msg: &str) -> Value {
        json!({
            "error": msg,
            "industry_median": MedianStats::default(),
            "all_industry_median": MedianStats::default(),
        })
    }

    #[test]
    fn condition_gap_response_matches_legacy_json() {
        let m1 = MedianStats {
            annual_income: 3_045_000.0,
            annual_holidays: 110.0,
            bonus_months: 2.5,
            sample_size: 10,
        };
        let m2 = MedianStats {
            annual_income: 2_999_999.75,
            annual_holidays: 104.5,
            bonus_months: 0.3,
            sample_size: 1,
        };
        let cases: Vec<(MedianStats, MedianStats, f64, f64, f64, &str, &str)> = vec![
            (
                m1.clone(),
                m2.clone(),
                220_000.0,
                3.0,
                115.0,
                "岩手県",
                "飲食業",
            ),
            (m1.clone(), m1.clone(), 180_000.5, 0.25, 96.0, "", ""),
            (
                MedianStats::default(),
                MedianStats::default(),
                0.0,
                0.0,
                0.0,
                "",
                "",
            ),
            (
                m2.clone(),
                MedianStats::default(),
                250_000.0,
                -1.0,
                0.1,
                "東京都",
                "医療",
            ),
        ];
        for (ind, all, sal, bonus, hol, pref, jt) in cases {
            let new = RdConditionGapResult::Ok(build_response(
                pref.to_string(),
                "盛岡市".to_string(),
                jt.to_string(),
                String::new(),
                ind.clone(),
                all.clone(),
                sal,
                bonus,
                hol,
            ));
            let legacy = legacy_condition_gap_json(
                pref.to_string(),
                "盛岡市".to_string(),
                jt.to_string(),
                String::new(),
                ind,
                all,
                sal,
                bonus,
                hol,
            );
            assert_eq!(
                serde_json::to_string(&new).unwrap(),
                serde_json::to_string(&legacy).unwrap()
            );
        }
    }

    #[test]
    fn condition_gap_error_matches_legacy_json() {
        for msg in ["HW DB 未接続", ""] {
            assert_eq!(
                serde_json::to_string(&error_response(msg)).unwrap(),
                serde_json::to_string(&legacy_error_response(msg)).unwrap()
            );
        }
    }

    #[test]
    fn condition_gap_ts_decl_has_main_fields() {
        let cfg = crate::handlers::recruitment_diag::types::ts_config();
        let result = RdConditionGapResult::decl(&cfg);
        assert!(
            result.contains("RdConditionGapResponse | RdConditionGapError"),
            "{result}"
        );
        let ok = RdConditionGapResponse::decl(&cfg);
        assert!(ok.contains("industry_median: RdMedianStats"), "{ok}");
        assert!(ok.contains("company: RdConditionGapCompany"), "{ok}");
        assert!(ok.contains("gap_industry: RdConditionGapDiff"), "{ok}");
        assert!(ok.contains("interpretation: string"), "{ok}");
        let median = MedianStats::decl(&cfg);
        assert!(median.contains("type RdMedianStats"), "{median}");
        assert!(median.contains("sample_size: number"), "{median}");
        let err = RdConditionGapError::decl(&cfg);
        assert!(err.contains("error: string"), "{err}");
        assert!(err.contains("all_industry_median: RdMedianStats"), "{err}");
    }
}
