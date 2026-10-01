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

/// 自社条件 (クエリ値。未入力・無効値 (非有限・負数) は null。0 は入力値 0 として扱う)
#[derive(Debug, Clone, Serialize, TS)]
pub struct RdConditionGapCompany {
    /// 自社推定年収 (円) = 月給 × (12 + 賞与月数)。月給と賞与の両方が入力されたときだけ算出し、
    /// どちらかが未入力なら null。月給 0 は入力値 0 として 0.0
    pub annual_income_estimated: Option<f64>,
    /// 自社年間休日 (日)。未入力は null
    pub annual_holidays: Option<f64>,
    /// 自社賞与月数 (ヶ月)。未入力は null
    pub bonus_months: Option<f64>,
    /// 自社月給下限 (円)。未入力は null
    pub salary_min: Option<f64>,
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
    company_salary_min: Option<f64>,
    company_bonus: Option<f64>,
    company_holidays: Option<f64>,
) -> RdConditionGapResponse {
    let company_salary_min = sanitize_input(company_salary_min);
    let company_bonus = sanitize_input(company_bonus);
    let company_holidays = sanitize_input(company_holidays);
    let company_annual_income = match (company_salary_min, company_bonus) {
        (Some(sal), Some(b)) => Some(compute_annual_income(sal, b)),
        _ => None,
    };

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

    let company = RdConditionGapCompany {
        annual_income_estimated: company_annual_income,
        annual_holidays: company_holidays,
        bonus_months: company_bonus,
        salary_min: company_salary_min,
    };
    let interpretation = build_interpretation(
        &gap_industry,
        &company,
        &industry_median,
        &job_type,
        &prefecture,
    );

    RdConditionGapResponse {
        prefecture,
        municipality,
        job_type,
        emp_type,
        industry_median,
        all_industry_median,
        company,
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

    Json(RdConditionGapResult::Ok(build_response(
        prefecture,
        q.municipality,
        q.job_type,
        q.emp_type,
        industry_median,
        all_industry_median,
        q.company_salary_min,
        q.company_bonus_months,
        q.company_annual_holidays,
    )))
}

/// 自社条件の入力値の検証。非有限値 (NaN / inf) と負数は無効入力として None。
/// 0 は入力値 0 として Some(0.0) のまま通す (未入力と区別する)。
pub(crate) fn sanitize_input(v: Option<f64>) -> Option<f64> {
    // -0.0 は 0.0 に正規化 (JSON に -0.0 を出さない)
    v.filter(|x| x.is_finite() && *x >= 0.0)
        .map(|x| if x == 0.0 { 0.0 } else { x })
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
    /// 年収差 (円)。自社の推定年収が算出できないときは null
    pub annual_income_diff: Option<f64>,
    /// 年収差の中央値比 (%)。年収差が null なら null。中央値 0 以下は 0.0
    pub annual_income_pct: Option<f64>,
    /// 年間休日差 (日)。自社の年間休日が未入力なら null
    pub annual_holidays_diff: Option<f64>,
    /// 賞与月数差 (ヶ月)。自社の賞与月数が未入力なら null
    pub bonus_months_diff: Option<f64>,
}

fn compute_gap(
    company_annual_income: Option<f64>,
    company_holidays: Option<f64>,
    company_bonus: Option<f64>,
    median: &MedianStats,
) -> Gap {
    let ai_diff = company_annual_income.map(|v| v - median.annual_income);
    let ai_pct = ai_diff.map(|d| {
        if median.annual_income > 0.0 {
            d / median.annual_income * 100.0
        } else {
            0.0
        }
    });
    Gap {
        annual_income_diff: ai_diff,
        annual_income_pct: ai_pct,
        annual_holidays_diff: company_holidays.map(|v| v - median.annual_holidays),
        bonus_months_diff: company_bonus.map(|v| v - median.bonus_months),
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
///
/// 年収の文は年収差があるときだけ、休日の文は休日差があるときだけ出す。
/// どちらも無いときは「未入力のため差は算出していません」とする。
fn build_interpretation(
    gap: &Gap,
    company: &RdConditionGapCompany,
    median: &MedianStats,
    job_type: &str,
    pref: &str,
) -> String {
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

    let income_label = gap.annual_income_diff.map(|diff| {
        let pct = gap.annual_income_pct.unwrap_or(0.0);
        if diff > 0.0 {
            format!(
                "御社推定年収は業界中央値より {:.0}円 ({:.1}%) 上回る傾向",
                diff, pct
            )
        } else if diff < 0.0 {
            format!(
                "御社推定年収は業界中央値より {:.0}円 ({:.1}%) 下回る傾向",
                diff.abs(),
                pct.abs()
            )
        } else {
            "御社推定年収は業界中央値と同水準".to_string()
        }
    });

    let holiday_label = gap.annual_holidays_diff.map(|diff| {
        if diff.abs() < 1.0 {
            "年間休日は業界中央値とほぼ同水準".to_string()
        } else if diff > 0.0 {
            format!("年間休日は業界中央値より {:.0}日多い傾向", diff)
        } else {
            format!("年間休日は業界中央値より {:.0}日少ない傾向", diff.abs())
        }
    });

    let (has_salary, has_bonus, has_holidays) = (
        company.salary_min.is_some(),
        company.bonus_months.is_some(),
        company.annual_holidays.is_some(),
    );
    let mut body = String::new();
    if !has_salary && !has_bonus && !has_holidays {
        body.push_str("自社条件 (月給・賞与・年間休日) が未入力のため、差は算出していません。");
    } else {
        // 年収の差は月給と賞与の両方が必要。足りない項目だけを挙げる
        match income_label {
            Some(label) => {
                body.push_str(&label);
                body.push('。');
            }
            None => {
                let missing: Vec<&str> = [(!has_salary, "月給"), (!has_bonus, "賞与")]
                    .into_iter()
                    .filter(|(m, _)| *m)
                    .map(|(_, n)| n)
                    .collect();
                body.push_str(&format!(
                    "{}が未入力のため、推定年収の差は算出していません (月給と賞与の両方が必要です)。",
                    missing.join("・")
                ));
            }
        }
        match holiday_label {
            Some(label) => {
                body.push_str(&label);
                body.push('。');
            }
            None => body.push_str("年間休日が未入力のため、年間休日の差は算出していません。"),
        }
    }

    format!(
        "【{region}・{industry}】{body}\
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
        let g = compute_gap(Some(4_000_000.0), Some(120.0), Some(2.5), &median);
        assert!((g.annual_income_diff.unwrap() - 500_000.0).abs() < 0.01);
        assert!((g.annual_income_pct.unwrap() - 14.285_714).abs() < 0.01);
        // 年休 120 vs 108 → 差 +12
        assert!((g.annual_holidays_diff.unwrap() - 12.0).abs() < 0.001);
        // 賞与 2.5 vs 2.8 → 差 -0.3
        assert!((g.bonus_months_diff.unwrap() - (-0.3)).abs() < 0.001);
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
        let g = compute_gap(Some(3_000_000.0), Some(105.0), Some(2.0), &median);
        assert!((g.annual_income_diff.unwrap() - (-500_000.0)).abs() < 0.01);
        assert!((g.annual_income_pct.unwrap() - (-14.285_714)).abs() < 0.01);
    }

    #[test]
    fn gap_zero_median() {
        // median=0 (データ無し) では pct = 0 で NaN/Inf を避ける
        let median = MedianStats::default();
        let g = compute_gap(Some(3_000_000.0), Some(100.0), Some(2.0), &median);
        assert_eq!(g.annual_income_pct, Some(0.0));
    }

    fn company_none() -> RdConditionGapCompany {
        RdConditionGapCompany {
            annual_income_estimated: None,
            annual_holidays: None,
            bonus_months: None,
            salary_min: None,
        }
    }

    #[test]
    fn interpretation_no_sample() {
        let gap = Gap::default();
        let median = MedianStats::default();
        let msg = build_interpretation(&gap, &company_none(), &median, "医療", "東京都");
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
            annual_income_diff: Some(500_000.0),
            annual_income_pct: Some(14.3),
            annual_holidays_diff: Some(12.0),
            bonus_months_diff: Some(0.0),
        };
        let company = RdConditionGapCompany {
            annual_income_estimated: Some(4_000_000.0),
            annual_holidays: Some(120.0),
            bonus_months: Some(2.5),
            salary_min: Some(280_000.0),
        };
        let msg = build_interpretation(&gap, &company, &median, "医療", "東京都");
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
            Some(company_annual_income),
            Some(company_holidays),
            Some(company_bonus),
            &industry_median,
        );
        let gap_all = compute_gap(
            Some(company_annual_income),
            Some(company_holidays),
            Some(company_bonus),
            &all_industry_median,
        );
        let company = RdConditionGapCompany {
            annual_income_estimated: Some(company_annual_income),
            annual_holidays: Some(company_holidays),
            bonus_months: Some(company_bonus),
            salary_min: Some(company_salary_min),
        };
        let interpretation = build_interpretation(
            &gap_industry,
            &company,
            &industry_median,
            &job_type,
            &prefecture,
        );
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
                0.5,
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
                Some(sal),
                Some(bonus),
                Some(hol),
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

    // ============================================================
    // 自社条件の未入力 (null) / 0 入力 / 一部入力
    // ============================================================

    fn median_a() -> MedianStats {
        MedianStats {
            annual_income: 3_045_000.0,
            annual_holidays: 110.0,
            bonus_months: 2.5,
            sample_size: 10,
        }
    }

    fn gap_json(sal: Option<f64>, bonus: Option<f64>, hol: Option<f64>) -> Value {
        let r = build_response(
            "岩手県".to_string(),
            "盛岡市".to_string(),
            "飲食業".to_string(),
            "正社員".to_string(),
            median_a(),
            median_a(),
            sal,
            bonus,
            hol,
        );
        serde_json::to_value(RdConditionGapResult::Ok(r)).unwrap()
    }

    fn interp(v: &Value) -> String {
        v["interpretation"].as_str().unwrap().to_string()
    }

    const TAIL: &str =
        "サンプル数 10件。※中央値は HW 掲載求人のみから算出。市場全体の実勢ではない。";
    const MISSING: &str = "自社条件 (月給・賞与・年間休日) が未入力のため、差は算出していません。";
    const INC: &str = "御社推定年収は業界中央値より 255000円 (8.4%) 上回る傾向。";
    const HOL_MISS: &str = "年間休日が未入力のため、年間休日の差は算出していません。";
    const INC_MISS_B: &str =
        "賞与が未入力のため、推定年収の差は算出していません (月給と賞与の両方が必要です)。";
    const INC_MISS_S: &str =
        "月給が未入力のため、推定年収の差は算出していません (月給と賞与の両方が必要です)。";
    const INC_MISS_SB: &str =
        "月給・賞与が未入力のため、推定年収の差は算出していません (月給と賞与の両方が必要です)。";

    #[test]
    fn gap_all_missing_is_null_and_message() {
        let v = gap_json(None, None, None);
        for k in [
            "annual_income_estimated",
            "annual_holidays",
            "bonus_months",
            "salary_min",
        ] {
            assert!(v["company"].get(k).is_some(), "key {k} must exist");
            assert!(v["company"][k].is_null(), "company.{k} must be null: {v}");
        }
        for g in ["gap_industry", "gap_all"] {
            for k in [
                "annual_income_diff",
                "annual_income_pct",
                "annual_holidays_diff",
                "bonus_months_diff",
            ] {
                assert!(v[g].get(k).is_some(), "key {g}.{k} must exist");
                assert!(v[g][k].is_null(), "{g}.{k} must be null: {v}");
            }
        }
        assert_eq!(interp(&v), format!("【岩手県・飲食業】{MISSING}{TAIL}"));
    }

    #[test]
    fn gap_full_input_values() {
        let v = gap_json(Some(220_000.0), Some(3.0), Some(115.0));
        // 220000 × 15 = 3,300,000。中央値 3,045,000 との差 255,000 (8.37...%)
        assert_eq!(v["company"]["annual_income_estimated"], 3_300_000.0);
        assert_eq!(v["gap_industry"]["annual_income_diff"], 255_000.0);
        assert_eq!(v["gap_industry"]["annual_holidays_diff"], 5.0);
        assert_eq!(v["gap_industry"]["bonus_months_diff"], 0.5);
        assert_eq!(
            interp(&v),
            format!(
                "【岩手県・飲食業】御社推定年収は業界中央値より 255000円 (8.4%) 上回る傾向。\
                 年間休日は業界中央値より 5日多い傾向。{TAIL}"
            )
        );
    }

    #[test]
    fn gap_zero_bonus_is_input_not_missing() {
        // 賞与 0 は入力値 0: 年収 200000×12 = 2,400,000、賞与差 0-2.5
        let v = gap_json(Some(200_000.0), Some(0.0), Some(100.0));
        assert_eq!(v["company"]["bonus_months"], 0.0);
        assert_eq!(v["company"]["annual_income_estimated"], 2_400_000.0);
        assert_eq!(v["gap_industry"]["bonus_months_diff"], -2.5);
        assert_eq!(v["gap_industry"]["annual_income_diff"], -645_000.0);
        assert_eq!(v["gap_industry"]["annual_holidays_diff"], -10.0);
    }

    #[test]
    fn gap_zero_salary_is_input_not_missing() {
        // 月給 0 は入力値 0: 年収 0、差は -3,045,000
        let v = gap_json(Some(0.0), Some(2.0), None);
        assert_eq!(v["company"]["salary_min"], 0.0);
        assert_eq!(v["company"]["annual_income_estimated"], 0.0);
        assert_eq!(v["gap_industry"]["annual_income_diff"], -3_045_000.0);
        assert_eq!(v["gap_industry"]["annual_income_pct"], -100.0);
        assert!(v["company"]["annual_holidays"].is_null());
        assert!(v["gap_industry"]["annual_holidays_diff"].is_null());
        assert_eq!(v["gap_industry"]["bonus_months_diff"], -0.5);
    }

    #[test]
    fn gap_salary_and_bonus_only() {
        // S+B: 220000 × 15 = 3,300,000。休日は null で文も出さない
        let v = gap_json(Some(220_000.0), Some(3.0), None);
        assert_eq!(v["gap_industry"]["annual_income_diff"], 255_000.0);
        assert!(v["gap_industry"]["annual_holidays_diff"].is_null());
        assert!(v["company"]["annual_holidays"].is_null());
        assert_eq!(
            interp(&v),
            format!("【岩手県・飲食業】{INC}{HOL_MISS}{TAIL}")
        );
    }

    #[test]
    fn gap_salary_and_holidays_only_has_no_income() {
        // S+H: 賞与が無いので推定年収は null (賞与 0 とみなさない)
        let v = gap_json(Some(220_000.0), None, Some(100.0));
        assert!(v["company"]["annual_income_estimated"].is_null());
        assert_eq!(v["company"]["salary_min"], 220_000.0);
        assert!(v["gap_industry"]["annual_income_diff"].is_null());
        assert!(v["gap_industry"]["annual_income_pct"].is_null());
        assert!(v["gap_industry"]["bonus_months_diff"].is_null());
        assert_eq!(v["gap_industry"]["annual_holidays_diff"], -10.0);
        assert_eq!(
            interp(&v),
            format!(
                "【岩手県・飲食業】{INC_MISS_B}年間休日は業界中央値より 10日少ない傾向。{TAIL}"
            )
        );
    }

    #[test]
    fn gap_salary_only_has_no_gap() {
        let v = gap_json(Some(220_000.0), None, None);
        assert!(v["company"]["annual_income_estimated"].is_null());
        assert!(v["gap_industry"]["annual_income_diff"].is_null());
        assert_eq!(
            interp(&v),
            format!("【岩手県・飲食業】{INC_MISS_B}{HOL_MISS}{TAIL}")
        );
    }

    #[test]
    fn gap_bonus_and_holidays_only() {
        // B+H: 月給が無いので年収は null。賞与差・休日差は出る
        let v = gap_json(None, Some(3.0), Some(115.0));
        assert!(v["company"]["annual_income_estimated"].is_null());
        assert!(v["gap_industry"]["annual_income_diff"].is_null());
        assert_eq!(v["gap_industry"]["bonus_months_diff"], 0.5);
        assert_eq!(v["gap_all"]["annual_holidays_diff"], 5.0);
        assert_eq!(
            interp(&v),
            format!("【岩手県・飲食業】{INC_MISS_S}年間休日は業界中央値より 5日多い傾向。{TAIL}")
        );
    }

    #[test]
    fn gap_bonus_only() {
        let v = gap_json(None, Some(3.0), None);
        assert_eq!(v["gap_industry"]["bonus_months_diff"], 0.5);
        assert!(v["gap_industry"]["annual_income_diff"].is_null());
        assert_eq!(
            interp(&v),
            format!("【岩手県・飲食業】{INC_MISS_S}{HOL_MISS}{TAIL}")
        );
    }

    #[test]
    fn gap_holidays_only() {
        let v = gap_json(None, None, Some(110.0));
        assert_eq!(v["gap_industry"]["annual_holidays_diff"], 0.0);
        assert!(v["gap_industry"]["bonus_months_diff"].is_null());
        assert_eq!(
            interp(&v),
            format!("【岩手県・飲食業】{INC_MISS_SB}年間休日は業界中央値とほぼ同水準。{TAIL}")
        );
    }

    #[test]
    fn gap_invalid_inputs_are_null() {
        // NaN / inf / 負数は無効入力: 未入力と同じく null
        let v = gap_json(Some(f64::NAN), Some(-1.0), Some(f64::INFINITY));
        assert!(v["company"]["salary_min"].is_null());
        assert!(v["company"]["bonus_months"].is_null());
        assert!(v["company"]["annual_holidays"].is_null());
        assert!(v["company"]["annual_income_estimated"].is_null());
        assert_eq!(interp(&v), format!("【岩手県・飲食業】{MISSING}{TAIL}"));
        assert_eq!(sanitize_input(Some(0.0)), Some(0.0));
        assert_eq!(sanitize_input(Some(-0.5)), None);
        assert_eq!(sanitize_input(Some(f64::NEG_INFINITY)), None);
        assert_eq!(sanitize_input(None), None);
    }

    /// 3 項目 x 入力あり・なしの 8 通り全部で、表 (company / gap_industry / gap_all) と文が食い違わない
    #[test]
    fn table_and_sentence_agree_for_all_8_combinations() {
        for mask in 0..8u8 {
            let (s, b, h) = (mask & 1 != 0, mask & 2 != 0, mask & 4 != 0);
            let v = gap_json(s.then_some(220_000.0), b.then_some(3.0), h.then_some(115.0));
            let text = interp(&v);
            let label = format!("S={s} B={b} H={h}");
            // company: 入力した項目だけ値、推定年収は月給と賞与の両方があるときだけ
            assert_eq!(!v["company"]["salary_min"].is_null(), s, "{label}");
            assert_eq!(!v["company"]["bonus_months"].is_null(), b, "{label}");
            assert_eq!(!v["company"]["annual_holidays"].is_null(), h, "{label}");
            assert_eq!(
                !v["company"]["annual_income_estimated"].is_null(),
                s && b,
                "{label}"
            );
            for g in ["gap_industry", "gap_all"] {
                assert_eq!(!v[g]["annual_income_diff"].is_null(), s && b, "{label} {g}");
                assert_eq!(!v[g]["annual_income_pct"].is_null(), s && b, "{label} {g}");
                assert_eq!(!v[g]["annual_holidays_diff"].is_null(), h, "{label} {g}");
                assert_eq!(!v[g]["bonus_months_diff"].is_null(), b, "{label} {g}");
            }
            // 文: 値のある差だけ述べ、無い差は「未入力」と述べる
            assert_eq!(
                text.contains("御社推定年収は業界中央値より"),
                s && b,
                "{label}: {text}"
            );
            assert_eq!(
                text.contains("年間休日は業界中央値より"),
                h,
                "{label}: {text}"
            );
            assert_eq!(text.contains(HOL_MISS), !h && (s || b), "{label}: {text}");
            let all_missing = !s && !b && !h;
            assert_eq!(text.contains(MISSING), all_missing, "{label}: {text}");
            // 期待する全文 (入力した項目を「未入力」と言わない)
            let income_part = if s && b {
                INC
            } else if all_missing {
                ""
            } else if s {
                INC_MISS_B
            } else if b {
                INC_MISS_S
            } else {
                INC_MISS_SB
            };
            let hol_part = if h {
                "年間休日は業界中央値より 5日多い傾向。"
            } else if all_missing {
                ""
            } else {
                HOL_MISS
            };
            let body = if all_missing {
                MISSING.to_string()
            } else {
                format!("{income_part}{hol_part}")
            };
            assert_eq!(text, format!("【岩手県・飲食業】{body}{TAIL}"), "{label}");
        }
    }

    #[test]
    fn negative_zero_is_normalized_to_zero() {
        let z = sanitize_input(Some(-0.0)).unwrap();
        assert_eq!(z, 0.0);
        assert!(!z.is_sign_negative(), "-0.0 must become +0.0");
        let v = gap_json(Some(-0.0), Some(-0.0), Some(-0.0));
        assert_eq!(v["company"]["salary_min"].to_string(), "0.0");
        assert_eq!(v["company"]["annual_holidays"].to_string(), "0.0");
        assert_eq!(v["company"]["bonus_months"].to_string(), "0.0");
        assert_eq!(v["company"]["annual_income_estimated"].to_string(), "0.0");
    }

    /// 禁止語 (断定表現) を含まないこと。validate_insight_phrase は「傾向」等の必須表現も要求するが、
    /// 「未入力のため算出していません」「同水準」の文は事実の記述で断定ではないので、
    /// 必須表現の欠如だけは許し、Forbidden の検出だけを失敗扱いにする。
    #[test]
    fn interpretation_all_patterns_pass_phrase_validator() {
        let check = |label: &str, text: &str| {
            if let Err(e) =
                crate::handlers::insight::phrase_validator::validate_insight_phrase(text)
            {
                assert!(
                    e.starts_with("Missing required hedging phrase"),
                    "{label}: {e} / {text}"
                );
            }
            assert!(!text.contains("。。"), "{label}: {text}");
        };
        let vals_s = [None, Some(0.0), Some(220_000.0), Some(150_000.0)];
        let vals_b = [None, Some(0.0), Some(3.0)];
        let vals_h = [None, Some(0.0), Some(110.0), Some(125.0)];
        let mut n = 0;
        let mut hedged = 0;
        for s in vals_s {
            for b in vals_b {
                for h in vals_h {
                    let v = gap_json(s, b, h);
                    let text = interp(&v);
                    check(&format!("{s:?}/{b:?}/{h:?}"), &text);
                    if crate::handlers::insight::phrase_validator::validate_insight_phrase(&text)
                        .is_ok()
                    {
                        hedged += 1;
                    }
                    n += 1;
                }
            }
        }
        assert_eq!(n, 48);
        // 年収または休日の差の文が付くパターンは「傾向」を含み、検証を完全に通る
        assert!(hedged > 0);
        check(
            "no sample",
            &build_interpretation(
                &Gap::default(),
                &company_none(),
                &MedianStats::default(),
                "",
                "",
            ),
        );
    }
}
