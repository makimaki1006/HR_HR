//! 媒体分析タブ 市区町村粒度ヘルパー (2026-04-26)
//!
//! ## 背景 (ユーザー指摘)
//! 「都道府県単位の集計はあまり参考にならない」
//! → 媒体分析タブの主役は CSV に登場する **市区町村** であり、47 都道府県ではない。
//!
//! ## 責務
//! - CSV 集計から件数 Top N の (prefecture, municipality) を抽出
//! - 主要市区町村ごとに既存 fetch 関数を呼び出して市区町村粒度のデータを準備
//! - 市区町村データが欠損している地域は都道府県値で fallback (注記を呼出側で付与)
//!
//! ## 既存テーブル可否マトリクス (確認済 schema)
//!
//! | テーブル | 市区町村粒度 | 関数 |
//! |---|---|---|
//! | v2_external_population | OK | fetch_population_data |
//! | v2_external_population_pyramid | OK | fetch_population_pyramid |
//! | v2_external_education | NG (prefecture のみ) | fetch_education |
//! | v2_external_education_facilities | OK | fetch_education_facilities |
//! | v2_external_labor_force | OK | fetch_labor_force |
//! | v2_external_geography | OK (citycode あり) | fetch_geography |
//! | v2_region_benchmark | OK (muni 列) | fetch_region_benchmark |
//! | v2_external_industry_structure | NG (prefecture_code のみ) | fetch_industry_structure |
//! | v2_external_household_spending | NG (prefecture+政令市) | fetch_household_spending |
//! | v2_external_social_life | NG (prefecture+政令市) | fetch_social_life |
//! | v2_external_internet_usage | NG (prefecture のみ) | fetch_internet_usage |
//! | v2_external_minimum_wage | NG (47 県のみ) | fetch_minimum_wage |
//!
//! NG のテーブルは表示側で「都道府県粒度参考値」と注記する。

use super::super::analysis::fetch as af;
use super::super::helpers::{get_f64, get_i64, get_str_ref, Row};
use super::aggregator::SurveyAggregation;

type Db = crate::db::local_sqlite::LocalDb;
type TursoDb = crate::db::turso_http::TursoDb;

/// CSV 件数 上位 N 市区町村を抽出
///
/// 入力: `agg.by_municipality_salary` (件数降順、最大 15 件)
/// 戻り値: `Vec<(prefecture, municipality, count)>` で件数 Top `n` 件
///
/// `feedback_test_data_validation.md` 準拠:
/// 戻り値の (pref, muni) は `Some` 値を持つレコードのみで、空文字は除外する。
pub(crate) fn top_municipalities(
    agg: &SurveyAggregation,
    n: usize,
) -> Vec<(String, String, usize)> {
    agg.by_municipality_salary
        .iter()
        .filter(|m| !m.prefecture.is_empty() && !m.name.is_empty())
        .take(n)
        .map(|m| (m.prefecture.clone(), m.name.clone(), m.count))
        .collect()
}

/// 市区町村別 デモグラフィック指標
///
/// 各市区町村について以下を集約:
/// - 人口ピラミッド (年齢階級別 男女)
/// - 学歴分布 (※ 市区町村 schema 不存在のため都道府県値 fallback)
/// - 労働力 (失業者・労働力人口)
/// - 教育施設数 (幼〜高)
/// - 高齢化率・生産年齢人口比率
#[derive(Debug, Clone, Default)]
pub(crate) struct MunicipalityDemographics {
    pub prefecture: String,
    pub municipality: String,
    pub csv_count: usize,
    pub pyramid: Vec<Row>,
    /// 学歴分布。市区町村粒度データなしのため都道府県値で fallback (`is_education_pref_fallback=true`)
    pub education: Vec<Row>,
    pub is_education_pref_fallback: bool,
    pub labor_force: Vec<Row>,
    pub education_facilities: Vec<Row>,
    pub population: Vec<Row>,
    pub geography: Vec<Row>,
}

impl MunicipalityDemographics {
    /// 高齢化率 (65+ 比率) を pyramid から計算。0 if no data.
    pub fn aging_rate(&self) -> f64 {
        let mut total = 0_i64;
        let mut elderly = 0_i64;
        for r in &self.pyramid {
            let m = get_i64(r, "male_count");
            let f = get_i64(r, "female_count");
            let t = m + f;
            total += t;
            let g = get_str_ref(r, "age_group");
            if matches!(
                g,
                "65-69"
                    | "70-74"
                    | "75-79"
                    | "80-84"
                    | "85+"
                    | "85-"
                    | "70-79"
                    | "80+"
                    | "65-74"
                    | "75+"
            ) {
                elderly += t;
            }
        }
        if total > 0 {
            (elderly as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// 生産年齢人口 (15-64) 比率を pyramid から計算。0 if no data.
    pub fn working_age_rate(&self) -> f64 {
        let mut total = 0_i64;
        let mut wa = 0_i64;
        for r in &self.pyramid {
            let m = get_i64(r, "male_count");
            let f = get_i64(r, "female_count");
            let t = m + f;
            total += t;
            let g = get_str_ref(r, "age_group");
            if matches!(
                g,
                "15-19"
                    | "20-24"
                    | "25-29"
                    | "30-34"
                    | "35-39"
                    | "40-44"
                    | "45-49"
                    | "50-54"
                    | "55-59"
                    | "60-64"
                    | "10-19"
                    | "20-29"
                    | "30-39"
                    | "40-49"
                    | "50-59"
                    | "60-69"
                    | "15-64"
            ) {
                wa += t;
            }
        }
        if total > 0 {
            (wa as f64 / total as f64) * 100.0
        } else {
            0.0
        }
    }

    /// 失業者数 (推定値)。direct unemployed > 0 ならそれ、無ければ rate × labor_force から計算。
    pub fn estimated_unemployed(&self) -> Option<i64> {
        let r = self.labor_force.first()?;
        let direct = get_i64(r, "unemployed");
        if direct > 0 {
            return Some(direct);
        }
        let rate = get_f64(r, "unemployment_rate");
        let employed = get_i64(r, "employed");
        let unemp_calc = get_i64(r, "unemployed");
        let lf = employed + unemp_calc;
        if rate > 0.0 && lf > 0 {
            Some(((lf as f64) * rate / 100.0).round() as i64)
        } else {
            None
        }
    }

    /// 教育施設合計 (幼〜高)
    pub fn total_facilities(&self) -> i64 {
        self.education_facilities
            .first()
            .map(|r| {
                get_i64(r, "kindergartens")
                    + get_i64(r, "elementary_schools")
                    + get_i64(r, "junior_high_schools")
                    + get_i64(r, "high_schools")
            })
            .unwrap_or(0)
    }
}

/// 上位 N 市区町村のデモグラフィックデータをまとめて取得。
///
/// 各市区町村について既存 fetch を呼び出し:
/// - `fetch_population_pyramid(db, turso, pref, muni)` (市区町村粒度 OK)
/// - `fetch_labor_force(db, turso, pref, muni)` (市区町村粒度 OK)
/// - `fetch_education_facilities(db, turso, pref, muni)` (市区町村粒度 OK)
/// - `fetch_population_data(db, turso, pref, muni)` (市区町村粒度 OK)
/// - `fetch_geography(db, turso, pref, muni)` (市区町村粒度 OK)
/// - `fetch_education(db, turso, pref)` (※都道府県粒度のみ → fallback フラグ ON)
///
/// 失敗 (テーブルなし / Turso 接続エラー) は空 Vec で fail-soft。
/// 表示側で空の指標は非表示 / 注記でフォールバック明示する。
pub(crate) fn fetch_municipality_demographics(
    db: &Db,
    turso: Option<&TursoDb>,
    top_munis: &[(String, String, usize)],
) -> Vec<MunicipalityDemographics> {
    top_munis
        .iter()
        .map(|(pref, muni, count)| MunicipalityDemographics {
            prefecture: pref.clone(),
            municipality: muni.clone(),
            csv_count: *count,
            pyramid: af::fetch_population_pyramid(db, turso, pref, muni),
            // education は市区町村粒度なし → 都道府県値で fallback
            education: af::fetch_education(db, turso, pref),
            is_education_pref_fallback: true,
            labor_force: af::fetch_labor_force(db, turso, pref, muni),
            education_facilities: af::fetch_education_facilities(db, turso, pref, muni),
            population: af::fetch_population_data(db, turso, pref, muni),
            geography: af::fetch_geography(db, turso, pref, muni),
        })
        .collect()
}

/// 上位 3 市区町村の region_benchmark 6 軸スコア。
///
/// `fetch_region_benchmark(db, pref, muni)` は muni 指定で市区町村粒度を返す。
/// 該当データが無い市区町村は都道府県値で fallback し、表示側で注記する。
///
/// 戻り値: Vec<(label, row, is_pref_fallback)>。label は "{pref} {muni}" 形式。
pub(crate) fn fetch_region_benchmarks_for_municipalities(
    db: &Db,
    top_munis: &[(String, String, usize)],
) -> Vec<(String, Row, bool)> {
    let mut out = Vec::new();
    for (pref, muni, _) in top_munis {
        if pref.is_empty() || muni.is_empty() {
            continue;
        }
        // 市区町村粒度を試行
        let muni_rows = af::fetch_region_benchmark(db, pref, muni);
        let chosen_muni = muni_rows
            .iter()
            .find(|r| get_str_ref(r, "emp_group") == "正社員")
            .cloned()
            .or_else(|| muni_rows.first().cloned());

        if let Some(row) = chosen_muni {
            // 市区町村行が見つかった
            out.push((format!("{} {}", pref, muni), row, false));
            continue;
        }

        // fallback: 都道府県値
        let pref_rows = af::fetch_region_benchmark(db, pref, "");
        let chosen_pref = pref_rows
            .iter()
            .find(|r| get_str_ref(r, "emp_group") == "正社員")
            .cloned()
            .or_else(|| pref_rows.first().cloned());

        if let Some(row) = chosen_pref {
            out.push((format!("{} {} (県値参考)", pref, muni), row, true));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::aggregator::MunicipalitySalaryAgg;
    use super::*;

    fn agg_with_munis(munis: &[(&str, &str, usize)]) -> SurveyAggregation {
        let mut agg = SurveyAggregation::default();
        agg.by_municipality_salary = munis
            .iter()
            .map(|(p, m, c)| MunicipalitySalaryAgg {
                name: m.to_string(),
                prefecture: p.to_string(),
                count: *c,
                avg_salary: 250_000,
                median_salary: 250_000,
            })
            .collect();
        agg
    }

    /// 逆証明: top_municipalities が件数 Top N を返すこと
    #[test]
    fn top_municipalities_returns_n_items() {
        let agg = agg_with_munis(&[
            ("東京都", "千代田区", 100),
            ("東京都", "新宿区", 80),
            ("神奈川県", "横浜市", 60),
            ("北海道", "札幌市", 40),
        ]);
        let top = top_municipalities(&agg, 3);
        assert_eq!(top.len(), 3, "Top 3 で 3 件返る");
        assert_eq!(top[0].0, "東京都");
        assert_eq!(top[0].1, "千代田区");
        assert_eq!(top[0].2, 100);
        assert_eq!(top[2].1, "横浜市");
    }

    /// 逆証明: 空 prefecture / municipality は除外
    #[test]
    fn top_municipalities_skips_empty_fields() {
        let agg = agg_with_munis(&[
            ("", "市区町村なし", 100),
            ("東京都", "", 80),
            ("東京都", "千代田区", 50),
        ]);
        let top = top_municipalities(&agg, 5);
        assert_eq!(top.len(), 1, "空 prefecture/muni は除外");
        assert_eq!(top[0].1, "千代田区");
    }

    /// 逆証明: 件数より少ない要求でも全件返す
    #[test]
    fn top_municipalities_handles_n_larger_than_data() {
        let agg = agg_with_munis(&[("東京都", "千代田区", 100), ("東京都", "新宿区", 80)]);
        let top = top_municipalities(&agg, 10);
        assert_eq!(top.len(), 2);
    }

    /// 逆証明: 空 agg では空 Vec
    #[test]
    fn top_municipalities_empty() {
        let agg = SurveyAggregation::default();
        let top = top_municipalities(&agg, 3);
        assert!(top.is_empty());
    }

    /// 逆証明: aging_rate 計算 (65+ 60% / 全体 100% → 60.0%)
    #[test]
    fn aging_rate_calculates_from_pyramid() {
        use serde_json::json;

        fn row(group: &str, m: i64, f: i64) -> Row {
            let mut r = Row::new();
            r.insert("age_group".to_string(), json!(group));
            r.insert("male_count".to_string(), json!(m));
            r.insert("female_count".to_string(), json!(f));
            r
        }

        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![
                row("20-29", 1000, 1000), // 生産年齢
                row("30-39", 1000, 1000), // 生産年齢
                row("65-69", 2000, 2000), // 高齢
                row("70-79", 1000, 1000), // 高齢
            ],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![],
            education_facilities: vec![],
            population: vec![],
            geography: vec![],
        };
        let rate = demo.aging_rate();
        // 高齢 = 6000, total = 10000, 60.0%
        assert!(
            (rate - 60.0).abs() < 0.01,
            "aging_rate = 60.0% (got {})",
            rate
        );
    }

    /// 逆証明: working_age_rate 計算 (生産年齢 4000 / total 10000 → 40.0%)
    #[test]
    fn working_age_rate_calculates_from_pyramid() {
        use serde_json::json;

        fn row(group: &str, m: i64, f: i64) -> Row {
            let mut r = Row::new();
            r.insert("age_group".to_string(), json!(group));
            r.insert("male_count".to_string(), json!(m));
            r.insert("female_count".to_string(), json!(f));
            r
        }

        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![
                row("0-9", 1000, 1000),   // 年少 2000
                row("20-29", 1000, 1000), // 生産年齢 2000
                row("30-39", 1000, 1000), // 生産年齢 2000
                row("65-69", 1000, 1000), // 高齢 2000
                row("70-79", 1000, 1000), // 高齢 2000
            ],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![],
            education_facilities: vec![],
            population: vec![],
            geography: vec![],
        };
        let rate = demo.working_age_rate();
        // 生産年齢 = 4000, total = 10000, 40.0%
        assert!(
            (rate - 40.0).abs() < 0.01,
            "working_age_rate = 40.0% (got {})",
            rate
        );
    }

    /// 逆証明: estimated_unemployed: direct value 取得
    #[test]
    fn estimated_unemployed_uses_direct_value() {
        use serde_json::json;
        let mut r = Row::new();
        r.insert("unemployed".to_string(), json!(25_000));
        r.insert("employed".to_string(), json!(975_000));
        r.insert("unemployment_rate".to_string(), json!(2.5));

        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![r],
            education_facilities: vec![],
            population: vec![],
            geography: vec![],
        };
        assert_eq!(demo.estimated_unemployed(), Some(25_000));
    }

    /// 逆証明: estimated_unemployed: rate 経由の計算
    #[test]
    fn estimated_unemployed_calculates_from_rate() {
        use serde_json::json;
        let mut r = Row::new();
        r.insert("unemployed".to_string(), json!(0)); // direct なし
        r.insert("employed".to_string(), json!(400_000));
        r.insert("unemployment_rate".to_string(), json!(4.0));

        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![r],
            education_facilities: vec![],
            population: vec![],
            geography: vec![],
        };
        // 400_000 × 4% = 16_000
        assert_eq!(demo.estimated_unemployed(), Some(16_000));
    }

    /// 逆証明: total_facilities が 4 区分の合計を返す
    #[test]
    fn total_facilities_sums_4_categories() {
        use serde_json::json;
        let mut r = Row::new();
        r.insert("kindergartens".to_string(), json!(20));
        r.insert("elementary_schools".to_string(), json!(50));
        r.insert("junior_high_schools".to_string(), json!(25));
        r.insert("high_schools".to_string(), json!(15));

        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![],
            education_facilities: vec![r],
            population: vec![],
            geography: vec![],
        };
        assert_eq!(demo.total_facilities(), 110); // 20+50+25+15
    }

    /// 逆証明: education は常に都道府県粒度 fallback (現状 schema)
    #[test]
    fn education_is_always_pref_fallback() {
        let demo = MunicipalityDemographics {
            prefecture: "東京都".to_string(),
            municipality: "千代田区".to_string(),
            csv_count: 50,
            pyramid: vec![],
            education: vec![],
            is_education_pref_fallback: true,
            labor_force: vec![],
            education_facilities: vec![],
            population: vec![],
            geography: vec![],
        };
        // 学歴データは v2_external_education に市区町村列なし → fallback フラグは常時 true
        assert!(
            demo.is_education_pref_fallback,
            "学歴データは市区町村粒度未対応のため pref_fallback フラグ true 必須"
        );
    }
}
