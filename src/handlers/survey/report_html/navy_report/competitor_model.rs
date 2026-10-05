//! 競合調査レポートの中間表現 (`CompetitorReport`)。
//!
//! 旧実装は `SurveyAggregation` と 3 本の `serde_json::Value` から直接 HTML 文字列を組んでいた。
//! ここで「画面に出す値そのもの」を型にし、HTML (`render_html`) と JSON API (`/api/competitor/report`) が
//! 同じ構造体から出るようにする。数値の加工 (万円換算・最頻値の同数ルール・フォールバック・年齢帯の並べ替え)
//! は **この構築関数だけ** が行い、表示側 (HTML / React) は整形 (桁区切り) だけをする。
//!
//! 外部由来の文字列 (Google の生エラー本文など) はここで落とす。型に載せないので JSON にも出ない。
//!
//! i64 は ts-rs が bigint にするので、TS に出す整数は `#[ts(type = "number")]` か f64 / u32 にする。
use serde::Serialize;
use serde_json::Value;
use ts_rs::TS;

use super::section_05b_competitor::head_tag_counts;
use crate::handlers::survey::aggregator::{BoundStats, SurveyAggregation};

/// 表の最大行数。HTML は上位 10 語、グラフは上位 25 語を使うので、25 語あれば足りる。
const KEYWORD_ROWS: usize = 25;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorReport {
    pub meta: ReportMeta,
    pub excel: ExcelSection,
    pub google: GoogleSection,
    pub indeed: IndeedSection,
    pub population: PopulationSection,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ReportMeta {
    /// 調査名 (空入力のときは既定名「Indeed競合調査」)。
    pub title: String,
    pub employment_type: Option<String>,
    pub prefecture: Option<String>,
    pub municipality: Option<String>,
    /// 給与の単位。"万円" または "円/時"。
    pub unit: String,
    pub is_hourly: bool,
    pub total_count: u32,
    /// 実際に使った上位 N (1〜200)。
    pub top_n_effective: u32,
    /// 利用者が送った値そのまま。API 層だけが埋める (HTML 生成では使わない)。
    pub top_n_requested: Option<String>,
    /// 給与の下限が読めた件数。0 に近いときは給与列を読めていない可能性がある。
    pub salary_parsed_count: u32,
    /// 重複排除後の件数のうち、給与の下限が読めなかった件数。
    pub salary_missing_count: u32,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct ExcelSection {
    /// 給与の小数桁数 (月給 = 2、時給 = 0)。値は 表示単位に直した数値。
    pub decimals: u8,
    /// 平均値・中央値・最頻値。各行の `values` は [総合下限, 総合上限, 人気下限, 人気上限]。
    pub salary_table: Vec<SalaryRow>,
    /// 集計件数 [総合下限, 総合上限, 人気下限, 人気上限]。
    pub salary_counts: [u32; 4],
    /// 差異 (総合 − 人気求人)。各行の `values` は [下限, 上限]。
    pub salary_diff: Vec<SalaryRow>,
    /// 求人票ワード調査 (全体)。最大 25 語、件数の多い順。
    pub keyword_all: Vec<KeywordRow>,
    /// 求人票ワード調査 (上位 N 件)。最大 25 語。
    pub keyword_head: Vec<KeywordRow>,
    pub histograms: Histograms,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct SalaryRow {
    pub label: String,
    /// 未取得は null (0 ではない)。
    pub values: Vec<Option<f64>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct KeywordRow {
    pub word: String,
    pub count: u32,
    /// 占有率の分母 (求人数)。
    pub jobs: u32,
    /// 件数 / 求人数 × 100。分母 0 のときは 0。
    pub share_pct: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct Histograms {
    pub upper: Vec<HistogramBin>,
    pub lower: Vec<HistogramBin>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HistogramBin {
    /// 階級の下端 (表示単位、整数文字列)。
    pub label: String,
    pub count: u32,
}

/// Google 広告 API の検索需要タブ。生のエラー本文は持たない。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GoogleSection {
    NotRequested {
        message: String,
    },
    Error {
        message: String,
    },
    Ok {
        keyword: String,
        region: String,
        demand: GoogleDemand,
        suggestions: GoogleSuggestions,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GoogleDemand {
    Ok {
        /// Google が解決した地域名。解決できなかったときは null。
        region_name: Option<String>,
        keywords: Vec<GoogleKeyword>,
    },
    MissingCredentials,
    Timeout,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct GoogleKeyword {
    pub keyword: String,
    pub avg_monthly: Option<f64>,
    pub competition: String,
    pub monthly_12m: Vec<GoogleMonth>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct GoogleMonth {
    pub month: String,
    pub search_volume: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum GoogleSuggestions {
    Ok { suggestions: Vec<GoogleSuggestion> },
    MissingCredentials,
    Timeout,
    Error,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct GoogleSuggestion {
    pub keyword: String,
    pub avg_monthly: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum IndeedSection {
    Unavailable {
        message: String,
    },
    Ok {
        title: String,
        region: String,
        source: String,
        caveat: String,
        built_at: String,
        rows: Vec<IndeedRow>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct IndeedRow {
    pub month: String,
    pub job: Option<f64>,
    pub ctk: Option<f64>,
    pub emp: Option<f64>,
    pub spp: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum PopulationSection {
    Unavailable {
        message: String,
    },
    Ok {
        region: String,
        /// 年齢の若い順に並べ替え済み。
        bands: Vec<PopulationBand>,
        minimum_wage: Option<f64>,
        minimum_wage_fiscal_year: Option<u32>,
        /// "YYYY-MM-DD" の文字列のまま (整形しない)。
        minimum_wage_effective_date: String,
        minimum_wage_as_of: String,
        /// "official_csv" | "database" | その他。
        minimum_wage_source: String,
        labor: Option<LaborStats>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PopulationBand {
    pub age_group: String,
    #[ts(type = "number")]
    pub male: i64,
    #[ts(type = "number")]
    pub female: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct LaborStats {
    /// 0 以下は null (旧 HTML は 0 を「—」で出していた)。
    pub fiscal_year: Option<u32>,
    pub unemployment_rate: Option<f64>,
    pub separation_rate: Option<f64>,
}

fn s(v: &Value, key: &str) -> String {
    v.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

fn n(v: &Value, key: &str) -> Option<f64> {
    v.get(key).and_then(Value::as_f64)
}

/// 集計結果と 3 つの外部コンテキスト (Indeed / Google / 人口) からレポートを作る。
/// 外部コンテキストの `Value` は `competitor.rs` が組んだものか、テストの固定 JSON。
pub(crate) fn build_competitor_report(
    agg: &SurveyAggregation,
    top_n: usize,
    title: &str,
    indeed: &Value,
    google: &Value,
    population: &Value,
) -> CompetitorReport {
    let (head, denom) = head_tag_counts(agg, top_n);
    let comp = &agg.competitor;
    let scale = if agg.is_hourly { 1.0 } else { 10000.0 };
    let disp = |v: i64| v as f64 / scale;

    let (lo, hi) = if agg.is_hourly {
        (&agg.salary_min_values_native, &agg.salary_max_values_native)
    } else {
        (&agg.salary_min_values, &agg.salary_max_values)
    };
    let fallback = BoundStats::from_values(lo, hi);
    let use_fallback = comp.pop_all.min_n == 0 && comp.pop_all.max_n == 0;
    let all = if use_fallback {
        &fallback
    } else {
        &comp.pop_all
    };
    let mut modes = comp.salary_modes;
    if use_fallback {
        for (index, values) in [lo, hi].into_iter().enumerate() {
            let mut counts = std::collections::BTreeMap::new();
            for value in values {
                *counts.entry(*value).or_insert(0usize) += 1;
            }
            // 同数なら低い額 (BTreeMap は昇順、`>` で厳密に増えたときだけ更新)。
            modes[index] = counts
                .into_iter()
                .fold(None, |best: Option<(i64, usize)>, item| {
                    if best.is_none_or(|(_, count)| item.1 > count) {
                        Some(item)
                    } else {
                        best
                    }
                })
                .map(|(v, _)| v);
        }
    }
    let pop = &comp.pop_popular;
    let rows = [
        (
            "平均値",
            [all.min_mean, all.max_mean, pop.min_mean, pop.max_mean],
        ),
        (
            "中央値",
            [
                all.min_median,
                all.max_median,
                pop.min_median,
                pop.max_median,
            ],
        ),
        ("最頻値", modes),
    ];
    let salary_table = rows
        .iter()
        .map(|(label, values)| SalaryRow {
            label: (*label).to_owned(),
            values: values.iter().map(|v| v.map(disp)).collect(),
        })
        .collect();
    // 差は整数のまま引いてから表示単位に直す (旧実装と同じ順序。浮動小数の丸め差を出さない)。
    let salary_diff = rows
        .iter()
        .map(|(label, v)| SalaryRow {
            label: (*label).to_owned(),
            values: vec![
                v[0].zip(v[2]).map(|(a, b)| disp(a - b)),
                v[1].zip(v[3]).map(|(a, b)| disp(a - b)),
            ],
        })
        .collect();

    let keyword_rows = |data: &[(String, usize)], denom: usize| -> Vec<KeywordRow> {
        data.iter()
            .take(KEYWORD_ROWS)
            .map(|(word, count)| KeywordRow {
                word: word.clone(),
                count: *count as u32,
                jobs: denom as u32,
                share_pct: if denom > 0 {
                    *count as f64 / denom as f64 * 100.0
                } else {
                    0.0
                },
            })
            .collect()
    };
    let step = if agg.is_hourly { 50 } else { 10000 };
    let histogram = |values: &[i64]| -> Vec<HistogramBin> {
        let mut bins = std::collections::BTreeMap::new();
        for value in values {
            *bins.entry(value / step * step).or_insert(0usize) += 1;
        }
        bins.into_iter()
            .map(|(v, count)| HistogramBin {
                label: format!("{:.0}", v as f64 / scale),
                count: count as u32,
            })
            .collect()
    };

    let top_n_effective = top_n as u32;
    CompetitorReport {
        meta: ReportMeta {
            title: if title.is_empty() {
                "Indeed競合調査".to_owned()
            } else {
                title.to_owned()
            },
            employment_type: agg.by_employment_type.first().map(|x| x.0.clone()),
            prefecture: agg.dominant_prefecture.clone(),
            municipality: agg.dominant_municipality.clone(),
            unit: if agg.is_hourly { "円/時" } else { "万円" }.to_owned(),
            is_hourly: agg.is_hourly,
            total_count: agg.total_count as u32,
            top_n_effective,
            top_n_requested: None,
            salary_parsed_count: lo.len() as u32,
            salary_missing_count: agg.total_count.saturating_sub(lo.len()) as u32,
            warnings: Vec::new(),
        },
        excel: ExcelSection {
            decimals: if agg.is_hourly { 0 } else { 2 },
            salary_table,
            salary_counts: [
                all.min_n as u32,
                all.max_n as u32,
                pop.min_n as u32,
                pop.max_n as u32,
            ],
            salary_diff,
            keyword_all: keyword_rows(&comp.tag_counts_all, agg.total_count),
            keyword_head: keyword_rows(&head, denom),
            histograms: Histograms {
                upper: histogram(hi),
                lower: histogram(lo),
            },
        },
        google: google_section(google),
        indeed: indeed_section(indeed),
        population: population_section(population),
    }
}

pub(super) fn google_section(data: &Value) -> GoogleSection {
    match data["status"].as_str() {
        Some("ok") => GoogleSection::Ok {
            keyword: s(data, "keyword"),
            region: s(data, "region"),
            demand: google_demand(&data["demand"]),
            suggestions: google_suggestions(&data["suggestions"]),
        },
        Some("not_requested") => GoogleSection::NotRequested {
            message: s(data, "message"),
        },
        // 自前の検索条件エラーなど。message は自前の固定文だけが入る経路。
        _ => GoogleSection::Error {
            message: s(data, "message"),
        },
    }
}

/// 外部 API の失敗は種類だけ残し、本文 (資格情報を含みうる) は捨てる。
fn google_demand(data: &Value) -> GoogleDemand {
    match data["status"].as_str() {
        Some("ok") => GoogleDemand::Ok {
            region_name: if data["region"].is_null() {
                None
            } else {
                Some(s(&data["region"], "canonical_name"))
            },
            keywords: data["keywords"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| GoogleKeyword {
                    keyword: s(row, "keyword"),
                    avg_monthly: n(row, "avg_monthly"),
                    competition: s(row, "competition"),
                    monthly_12m: row["monthly_12m"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .map(|m| GoogleMonth {
                            month: s(m, "month"),
                            search_volume: n(m, "search_volume"),
                        })
                        .collect(),
                })
                .collect(),
        },
        Some("missing_credentials") => GoogleDemand::MissingCredentials,
        Some("timeout") => GoogleDemand::Timeout,
        _ => GoogleDemand::Error,
    }
}

fn google_suggestions(data: &Value) -> GoogleSuggestions {
    match data["status"].as_str() {
        Some("ok") => GoogleSuggestions::Ok {
            suggestions: data["suggestions"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|row| GoogleSuggestion {
                    keyword: s(row, "keyword"),
                    avg_monthly: n(row, "avg_monthly"),
                })
                .collect(),
        },
        Some("missing_credentials") => GoogleSuggestions::MissingCredentials,
        Some("timeout") => GoogleSuggestions::Timeout,
        _ => GoogleSuggestions::Error,
    }
}

fn indeed_section(data: &Value) -> IndeedSection {
    if data["status"] != "ok" {
        return IndeedSection::Unavailable {
            message: s(data, "message"),
        };
    }
    IndeedSection::Ok {
        title: s(data, "title"),
        region: s(data, "region"),
        source: s(data, "source"),
        caveat: s(data, "caveat"),
        built_at: s(data, "built_at"),
        rows: data["rows"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|row| IndeedRow {
                month: s(row, "month"),
                job: n(row, "job"),
                ctk: n(row, "ctk"),
                emp: n(row, "emp"),
                spp: n(row, "spp"),
            })
            .collect(),
    }
}

pub(super) fn population_section(data: &Value) -> PopulationSection {
    if data["status"] != "ok" {
        return PopulationSection::Unavailable {
            message: s(data, "message"),
        };
    }
    let mut bands: Vec<PopulationBand> = data["bands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(PopulationBand {
                age_group: row["age_group"].as_str()?.to_owned(),
                male: row["male_count"].as_i64()?,
                female: row["female_count"].as_i64()?,
            })
        })
        .collect();
    // 先頭の数字で並べる (安定ソート。数字が無い階級は末尾)。
    bands.sort_by_key(|b| {
        b.age_group
            .chars()
            .take_while(|c| c.is_ascii_digit())
            .collect::<String>()
            .parse::<u32>()
            .unwrap_or(u32::MAX)
    });
    let labor = &data["labor"];
    PopulationSection::Ok {
        region: s(data, "region"),
        bands,
        minimum_wage: n(data, "minimum_wage"),
        minimum_wage_fiscal_year: data["minimum_wage_fiscal_year"]
            .as_i64()
            .and_then(|y| u32::try_from(y).ok()),
        minimum_wage_effective_date: s(data, "minimum_wage_effective_date"),
        minimum_wage_as_of: s(data, "minimum_wage_as_of"),
        minimum_wage_source: s(data, "minimum_wage_source"),
        labor: (!labor.is_null()).then(|| LaborStats {
            fiscal_year: labor["fiscal_year"]
                .as_i64()
                .filter(|y| *y > 0)
                .and_then(|y| u32::try_from(y).ok()),
            unemployment_rate: n(labor, "unemployment_rate"),
            separation_rate: n(labor, "separation_rate"),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handlers::survey::aggregator::{aggregate_records_with_mode, salary_fixture};
    use crate::handlers::survey::upload::WageMode;
    use serde_json::json;

    fn agg() -> SurveyAggregation {
        let records = salary_fixture::records(
            "月給 25万円 ~ 30万円\n月給 35万円 ~ 40万円",
            "大阪府 大阪市",
        );
        aggregate_records_with_mode(&records, WageMode::Monthly)
    }

    #[test]
    fn google_failures_keep_only_the_kind_not_the_raw_text() {
        let google = json!({"status":"ok","keyword":"k","region":"大阪府",
            "demand":{"status":"error","message":"secret-token 認証失敗","missing":["GOOGLE_ADS_DEVELOPER_TOKEN"]},
            "suggestions":{"status":"missing_credentials","message":"secret-token"}});
        let report = build_competitor_report(
            &agg(),
            10,
            "t",
            &json!({"status":"unavailable","message":"i"}),
            &google,
            &json!({"status":"unavailable","message":"p"}),
        );
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("secret-token"), "{json}");
        assert!(!json.contains("GOOGLE_ADS_DEVELOPER_TOKEN"), "{json}");
        let v = serde_json::to_value(&report).unwrap();
        assert_eq!(v["google"]["status"], "ok");
        assert_eq!(v["google"]["demand"]["status"], "error");
        assert_eq!(v["google"]["suggestions"]["status"], "missing_credentials");
    }

    #[test]
    fn google_timeout_and_not_requested_are_distinct_states() {
        let timeout = json!({"status":"ok","keyword":"k","region":"","demand":{"status":"timeout"},"suggestions":{"status":"timeout"}});
        let r = build_competitor_report(&agg(), 10, "", &Value::Null, &timeout, &Value::Null);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["google"]["demand"]["status"], "timeout");
        assert_eq!(v["google"]["suggestions"]["status"], "timeout");
        let r = build_competitor_report(
            &agg(),
            10,
            "",
            &Value::Null,
            &json!({"status":"not_requested","message":"m"}),
            &Value::Null,
        );
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["google"]["status"], "not_requested");
        assert_eq!(v["google"]["message"], "m");
        // 既定の調査名
        assert_eq!(v["meta"]["title"], "Indeed競合調査");
    }

    #[test]
    fn population_bands_are_sorted_by_age_and_missing_values_stay_null() {
        let pop = json!({"status":"ok","region":"大阪府",
            "bands":[
              {"age_group":"10〜14歳","male_count":3,"female_count":4},
              {"age_group":"0〜4歳","male_count":1,"female_count":2},
              {"age_group":"5〜9歳","male_count":5,"female_count":6},
              {"age_group":"不明","male_count":7,"female_count":8},
              {"age_group":"壊れた行","male_count":"x","female_count":1}],
            "minimum_wage":null,"minimum_wage_fiscal_year":2025,
            "minimum_wage_effective_date":"2025-10-16","minimum_wage_as_of":"2026-01-01",
            "minimum_wage_source":"official_csv",
            "labor":{"fiscal_year":0,"unemployment_rate":null,"separation_rate":0.0}});
        let r = build_competitor_report(&agg(), 10, "", &Value::Null, &Value::Null, &pop);
        let v = serde_json::to_value(&r).unwrap();
        let ages: Vec<&str> = v["population"]["bands"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["age_group"].as_str().unwrap())
            .collect();
        // 数字の無い階級は末尾、数値でない行は捨てる
        assert_eq!(ages, ["0〜4歳", "5〜9歳", "10〜14歳", "不明"]);
        assert_eq!(v["population"]["bands"][0]["male"], 1);
        assert!(
            v["population"]["minimum_wage"].is_null(),
            "未取得は 0 ではなく null"
        );
        assert_eq!(v["population"]["minimum_wage_effective_date"], "2025-10-16");
        assert!(
            v["population"]["labor"]["fiscal_year"].is_null(),
            "年度 0 は null"
        );
        assert!(v["population"]["labor"]["unemployment_rate"].is_null());
        assert_eq!(
            v["population"]["labor"]["separation_rate"], 0.0,
            "0 は 0 のまま"
        );
    }

    #[test]
    fn indeed_missing_months_stay_null_not_zero() {
        let indeed = json!({"status":"ok","title":"t","region":"大阪府","source":"s","caveat":"c","built_at":"2026-09-01",
            "rows":[{"month":"2026-07","job":100,"ctk":null,"emp":0,"spp":null}]});
        let r = build_competitor_report(&agg(), 10, "", &indeed, &Value::Null, &Value::Null);
        let v = serde_json::to_value(&r).unwrap();
        let row = &v["indeed"]["rows"][0];
        assert_eq!(row["job"], 100.0);
        assert!(row["ctk"].is_null());
        assert_eq!(row["emp"], 0.0);
        assert!(row["spp"].is_null());
    }
}
