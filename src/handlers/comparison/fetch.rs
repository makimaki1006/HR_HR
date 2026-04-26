//! 47 都道府県の集計 KPI を取得する
//!
//! `postings` テーブルから 1 回の SQL で全指標をまとめて取得する。
//! 全 47 県を必ず返却する（postings に行がない県は 0 件で埋める）。

use crate::db::local_sqlite::LocalDb;
use crate::handlers::helpers::{get_f64, get_i64, get_str};
use crate::models::job_seeker::PREFECTURE_ORDER;
use std::collections::HashMap;

/// 都道府県別 KPI（全指標を一括保持）
#[derive(Debug, Clone, Default)]
pub struct PrefectureKpi {
    pub prefecture: String,
    /// 求人件数
    pub posting_count: i64,
    /// 月給下限の平均（円）
    pub salary_min_avg: f64,
    /// 正社員求人比率（0.0-1.0）
    pub seishain_ratio: f64,
    /// 事業所数
    pub facility_count: i64,
    /// 給与開示率（salary_min > 0 の比率、0.0-1.0）
    pub salary_disclosure_rate: f64,
}

/// 利用可能な指標一覧
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComparisonMetric {
    PostingCount,
    SalaryMinAvg,
    SeishainRatio,
    FacilityCount,
    SalaryDisclosureRate,
}

impl ComparisonMetric {
    pub fn from_str(s: &str) -> Self {
        match s {
            "salary_min_avg" => Self::SalaryMinAvg,
            "seishain_ratio" => Self::SeishainRatio,
            "facility_count" => Self::FacilityCount,
            "salary_disclosure_rate" => Self::SalaryDisclosureRate,
            _ => Self::PostingCount,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PostingCount => "posting_count",
            Self::SalaryMinAvg => "salary_min_avg",
            Self::SeishainRatio => "seishain_ratio",
            Self::FacilityCount => "facility_count",
            Self::SalaryDisclosureRate => "salary_disclosure_rate",
        }
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::PostingCount => "求人件数",
            Self::SalaryMinAvg => "月給下限の平均",
            Self::SeishainRatio => "正社員求人比率",
            Self::FacilityCount => "事業所数",
            Self::SalaryDisclosureRate => "給与開示率",
        }
    }

    pub fn unit(&self) -> &'static str {
        match self {
            Self::PostingCount => "件",
            Self::SalaryMinAvg => "円",
            Self::SeishainRatio => "%",
            Self::FacilityCount => "事業所",
            Self::SalaryDisclosureRate => "%",
        }
    }

    /// 値を表示用文字列に変換
    pub fn format_value(&self, kpi: &PrefectureKpi) -> String {
        match self {
            Self::PostingCount => crate::handlers::helpers::format_number(kpi.posting_count),
            Self::SalaryMinAvg => {
                if kpi.salary_min_avg > 0.0 {
                    crate::handlers::helpers::format_number(kpi.salary_min_avg.round() as i64)
                } else {
                    "-".to_string()
                }
            }
            Self::SeishainRatio => format!("{:.1}", kpi.seishain_ratio * 100.0),
            Self::FacilityCount => crate::handlers::helpers::format_number(kpi.facility_count),
            Self::SalaryDisclosureRate => format!("{:.1}", kpi.salary_disclosure_rate * 100.0),
        }
    }

    /// 数値ソート用の値抽出
    pub fn numeric_value(&self, kpi: &PrefectureKpi) -> f64 {
        match self {
            Self::PostingCount => kpi.posting_count as f64,
            Self::SalaryMinAvg => kpi.salary_min_avg,
            Self::SeishainRatio => kpi.seishain_ratio * 100.0,
            Self::FacilityCount => kpi.facility_count as f64,
            Self::SalaryDisclosureRate => kpi.salary_disclosure_rate * 100.0,
        }
    }
}

/// 全 47 都道府県の KPI を取得（産業フィルタ対応）
///
/// `industry_raws`: 産業大分類フィルタ（空ならば全産業）
///
/// 返り値: 47 件 (PREFECTURE_ORDER 順)。postings にない県は 0 件で埋める。
pub fn fetch_all_prefecture_kpi(
    db: &LocalDb,
    industry_raws: &[String],
) -> Vec<PrefectureKpi> {
    // 産業フィルタ句を構築（プレースホルダ ?1, ?2... をバインド）
    let (filter_clause, params): (String, Vec<String>) = if industry_raws.is_empty() {
        (String::new(), Vec::new())
    } else {
        let placeholders: Vec<String> = (1..=industry_raws.len()).map(|i| format!("?{i}")).collect();
        (
            format!(" AND job_type IN ({})", placeholders.join(",")),
            industry_raws.to_vec(),
        )
    };

    let sql = format!(
        "SELECT prefecture, \
                COUNT(*) AS posting_count, \
                AVG(CASE WHEN salary_type = '月給' AND salary_min > 0 THEN salary_min END) AS salary_min_avg, \
                CAST(SUM(CASE WHEN employment_type = '正社員' THEN 1 ELSE 0 END) AS REAL) / \
                  NULLIF(COUNT(*), 0) AS seishain_ratio, \
                COUNT(DISTINCT facility_name) AS facility_count, \
                CAST(SUM(CASE WHEN salary_min > 0 THEN 1 ELSE 0 END) AS REAL) / \
                  NULLIF(COUNT(*), 0) AS salary_disclosure_rate \
         FROM postings \
         WHERE prefecture IS NOT NULL AND prefecture != ''{filter_clause} \
         GROUP BY prefecture"
    );

    let bind_refs: Vec<&dyn rusqlite::types::ToSql> = params
        .iter()
        .map(|s| s as &dyn rusqlite::types::ToSql)
        .collect();

    let rows = db.query(&sql, &bind_refs).unwrap_or_default();

    // prefecture → KPI のマップを構築
    let mut map: HashMap<String, PrefectureKpi> = HashMap::new();
    for row in &rows {
        let pref = get_str(row, "prefecture");
        if pref.is_empty() {
            continue;
        }
        map.insert(
            pref.clone(),
            PrefectureKpi {
                prefecture: pref,
                posting_count: get_i64(row, "posting_count"),
                salary_min_avg: get_f64(row, "salary_min_avg"),
                seishain_ratio: get_f64(row, "seishain_ratio"),
                facility_count: get_i64(row, "facility_count"),
                salary_disclosure_rate: get_f64(row, "salary_disclosure_rate"),
            },
        );
    }

    // PREFECTURE_ORDER で 47 件を必ず返す（欠損は 0 件で埋める）
    PREFECTURE_ORDER
        .iter()
        .map(|&pref| {
            map.remove(pref).unwrap_or_else(|| PrefectureKpi {
                prefecture: pref.to_string(),
                ..Default::default()
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metric_from_str_round_trip() {
        for s in [
            "posting_count",
            "salary_min_avg",
            "seishain_ratio",
            "facility_count",
            "salary_disclosure_rate",
        ] {
            assert_eq!(ComparisonMetric::from_str(s).as_str(), s);
        }
    }

    #[test]
    fn metric_from_str_unknown_falls_back_to_posting_count() {
        // 未知の値は PostingCount にフォールバック（XSS 対策）
        assert_eq!(
            ComparisonMetric::from_str("../../../etc/passwd").as_str(),
            "posting_count"
        );
        assert_eq!(
            ComparisonMetric::from_str("").as_str(),
            "posting_count"
        );
    }
}
