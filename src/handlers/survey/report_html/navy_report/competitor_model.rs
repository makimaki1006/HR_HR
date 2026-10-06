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

use super::competitor_consultation::{self, ConsultationSection};
use super::competitor_keywords::{self, KeywordComparison};
use super::competitor_population::{self, PopulationShares};
use super::section_05b_competitor::head_tag_counts;
use crate::handlers::survey::aggregator::{BoundStats, SurveyAggregation};

/// 表の最大行数。HTML の表は上位 10 語、グラフは上位 20 語を使うので、25 語あれば足りる。
const KEYWORD_ROWS: usize = 25;

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct CompetitorReport {
    pub meta: ReportMeta,
    pub excel: ExcelSection,
    pub google: GoogleSection,
    pub indeed: IndeedSection,
    pub population: PopulationSection,
    /// 「採用のヒント」タブ。給与の比較・訴求の確認候補・外部データの取得状況。
    pub consultation: ConsultationSection,
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
    /// 先頭 N 件と全体の占有率比較 (上位 20 語)。語ごとに同じ語の全体件数を引いてある。
    pub keyword_comparison: KeywordComparison,
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
    pub upper: HistogramSeries,
    pub lower: HistogramSeries,
}

/// 給与分布。空の給与区間も 0 件の階級として残す (階級が連続した軸になる)。
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct HistogramSeries {
    pub bins: Vec<HistogramBin>,
    /// 分布に使った求人の件数 (給与が読めた件数)。
    pub n: u32,
    /// 階級の幅 (表示単位)。階級が 81 個を超えないように基準幅の整数倍に広げることがある。
    pub step: f64,
    /// 「最多の給与帯：…」の一文。データが無い・全階級が 0 件のときは null。
    pub summary: Option<String>,
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
    Ok {
        /// 関連語を取得した地域名 (Google が解決した名前)。地域指定なし・解決できなかったときは null (全国)。
        region_name: Option<String>,
        suggestions: Vec<GoogleSuggestion>,
    },
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
        /// 集計地域 (全国のときは「全国」。不明なら空)。
        region: String,
        message: String,
    },
    Ok {
        region: String,
        /// 都道府県未指定 (全国の市区町村を合算) か。true のとき最低賃金・労働統計は出さない。
        is_national: bool,
        /// 人口の基準日。全市区町村で一致しないときは null。
        reference_date: Option<String>,
        /// 年齢の若い順に並べ替え済み。欠測は null のまま (0 にしない)。
        bands: Vec<PopulationBand>,
        /// 総人口を分母にした構成比。総人口と男女合計が合わないなど、成立しないときは null。
        shares: Option<Box<PopulationShares>>,
        minimum_wage: Option<f64>,
        minimum_wage_fiscal_year: Option<u32>,
        /// "YYYY-MM-DD" の文字列のまま (整形しない)。
        minimum_wage_effective_date: String,
        minimum_wage_as_of: String,
        /// "official_csv" | "database" | その他。
        minimum_wage_source: String,
        /// 厚生労働省の公式資料の URL (https://www.mhlw.go.jp/ だけ通す)。
        minimum_wage_source_url: Option<String>,
        labor: Option<LaborStats>,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub struct PopulationBand {
    pub age_group: String,
    /// 欠測・負の値は null。
    #[ts(type = "number | null")]
    pub male: Option<i64>,
    #[ts(type = "number | null")]
    pub female: Option<i64>,
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
    let google = google_section(google);
    let indeed = indeed_section(indeed);
    let population = population_section(population);
    let external = competitor_consultation::ExternalAvailability {
        google_demand: matches!(
            &google,
            GoogleSection::Ok {
                demand: GoogleDemand::Ok { .. },
                ..
            }
        ),
        google_suggestions: matches!(
            &google,
            GoogleSection::Ok {
                suggestions: GoogleSuggestions::Ok { .. },
                ..
            }
        ),
        indeed: matches!(&indeed, IndeedSection::Ok { .. }),
        population: matches!(&population, PopulationSection::Ok { .. }),
    };
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
    let base = if agg.is_hourly { 50 } else { 10000 };
    let histogram = |values: &[i64]| histogram_series(values, base, scale);

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
            keyword_comparison: competitor_keywords::build_comparison(
                &head,
                denom,
                &comp.tag_counts_all,
                agg.total_count,
            ),
            histograms: Histograms {
                upper: histogram(hi),
                lower: histogram(lo),
            },
        },
        google,
        indeed,
        population,
        consultation: competitor_consultation::build(agg, all, pop, top_n, use_fallback, &external),
    }
}

/// 給与分布を階級に分ける。階級数が 81 を超えないよう、基準幅 (月給 1 万円・時給 50 円) の
/// 整数倍に幅を広げる。最小〜最大の間の空の階級は 0 件のまま残す (飛ばすと軸が歪む)。
fn histogram_series(values: &[i64], base: i64, scale: f64) -> HistogramSeries {
    let (Some(lo), Some(hi)) = (values.iter().min(), values.iter().max()) else {
        return HistogramSeries {
            bins: Vec::new(),
            n: 0,
            step: base as f64 / scale,
            summary: None,
        };
    };
    let span = (hi - lo) / base + 1;
    let step = base * ((span + 79) / 80).max(1);
    let start = lo / step;
    let end = hi / step;
    let mut counts = vec![0u32; (end - start + 1) as usize];
    for v in values {
        counts[(v / step - start) as usize] += 1;
    }
    let bins: Vec<HistogramBin> = counts
        .into_iter()
        .enumerate()
        .map(|(i, count)| HistogramBin {
            label: format!("{:.0}", ((start + i as i64) * step) as f64 / scale),
            count,
        })
        .collect();
    series_from_bins(bins, values.len() as u32, step as f64 / scale)
}

pub(super) fn series_from_bins(bins: Vec<HistogramBin>, n: u32, step: f64) -> HistogramSeries {
    let summary = peak_summary(&bins);
    HistogramSeries {
        bins,
        n,
        step,
        summary,
    }
}

/// 最多の階級の一文。同数のピークは全部残す (3 つまで列挙、それ以上は区間数だけ)。
fn peak_summary(bins: &[HistogramBin]) -> Option<String> {
    let peak = bins.iter().map(|b| b.count).max().filter(|p| *p > 0)?;
    let total: u32 = bins.iter().map(|b| b.count).sum();
    let peaks: Vec<&HistogramBin> = bins.iter().filter(|b| b.count == peak).collect();
    let interval = bins.first().zip(bins.get(1)).and_then(|(first, second)| {
        Some(second.label.parse::<f64>().ok()? - first.label.parse::<f64>().ok()?)
    });
    let band = |label: &str| match label.parse::<f64>().ok().zip(interval) {
        Some((start, step)) => format!("{start:.0}〜{:.0}", start + step),
        None => format!("{label}〜"),
    };
    Some(if peaks.len() == 1 {
        format!(
            "最多の給与帯：{} / {}件・{:.1}%",
            band(&peaks[0].label),
            peak,
            peak as f64 / total.max(1) as f64 * 100.0
        )
    } else if peaks.len() <= 3 {
        format!(
            "最多の給与帯：{} / 各{}件",
            peaks
                .iter()
                .map(|b| band(&b.label))
                .collect::<Vec<_>>()
                .join("・"),
            peak
        )
    } else {
        format!("最多の給与帯：同数{}区間 / 各{}件", peaks.len(), peak)
    })
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
            region_name: if data["region"].is_null() {
                None
            } else {
                Some(s(&data["region"], "canonical_name"))
            },
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

/// 厚生労働省 (mhlw.go.jp とそのサブドメイン) の https URL だけをリンクにする。
/// 利用者の入力や外部 API 由来の URL をそのまま <a href> に出さないための許可リスト。
fn is_mhlw_url(url: &str) -> bool {
    let Some(rest) = url.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or_default();
    // ユーザー情報 (user@host) やポート指定は許さない。
    !host.contains(['@', ':', '\\']) && (host == "mhlw.go.jp" || host.ends_with(".mhlw.go.jp"))
}

pub(super) fn population_section(data: &Value) -> PopulationSection {
    if data["status"] != "ok" {
        return PopulationSection::Unavailable {
            region: s(data, "region"),
            message: s(data, "message"),
        };
    }
    // 年齢階級の名前が読めない行だけ捨てる。人数が欠測・負の行は残し、人数を null にする (0 にしない)。
    let mut bands: Vec<PopulationBand> = data["bands"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|row| {
            Some(PopulationBand {
                age_group: row["age_group"].as_str()?.to_owned(),
                male: row["male_count"].as_i64().filter(|n| *n >= 0),
                female: row["female_count"].as_i64().filter(|n| *n >= 0),
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
    let region = s(data, "region");
    PopulationSection::Ok {
        is_national: region == "全国",
        region,
        reference_date: data["reference_date"].as_str().map(str::to_owned),
        bands,
        shares: competitor_population::build_shares(data).map(Box::new),
        minimum_wage: n(data, "minimum_wage"),
        minimum_wage_fiscal_year: data["minimum_wage_fiscal_year"]
            .as_i64()
            .and_then(|y| u32::try_from(y).ok()),
        minimum_wage_effective_date: s(data, "minimum_wage_effective_date"),
        minimum_wage_as_of: s(data, "minimum_wage_as_of"),
        minimum_wage_source: s(data, "minimum_wage_source"),
        minimum_wage_source_url: data["minimum_wage_source_url"]
            .as_str()
            .filter(|url| is_mhlw_url(url))
            .map(str::to_owned),
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
        // 数字の無い階級は末尾。人数が数値でない行は捨てずに、人数を null にして残す (0 にしない)
        assert_eq!(ages, ["0〜4歳", "5〜9歳", "10〜14歳", "不明", "壊れた行"]);
        assert_eq!(v["population"]["bands"][0]["male"], 1);
        assert!(v["population"]["bands"][4]["male"].is_null());
        assert_eq!(v["population"]["bands"][4]["female"], 1);
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
    fn histogram_is_bounded_and_keeps_every_value() {
        let wide = histogram_series(&[1000, 1_000_000], 50, 1.0);
        assert!(wide.bins.len() <= 81);
        assert_eq!(wide.bins.iter().map(|b| b.count).sum::<u32>(), 2);
        assert!(wide.step > 50.0, "幅は基準幅の整数倍に広がる");
        let none = histogram_series(&[], 10_000, 10_000.0);
        assert!(none.bins.is_empty() && none.summary.is_none() && none.n == 0);
    }

    #[test]
    fn population_shares_and_national_flag_come_from_the_context() {
        let pop = json!({"status":"ok","region":"全国","reference_date":"2020-10-01",
            "bands":[{"age_group":"20-29","male_count":100,"female_count":200}],
            "totals":{"total_population":1000,"male_population":400,"female_population":600}});
        let r = build_competitor_report(&agg(), 10, "", &Value::Null, &Value::Null, &pop);
        let v = serde_json::to_value(&r).unwrap();
        assert_eq!(v["population"]["is_national"], true);
        assert_eq!(v["population"]["reference_date"], "2020-10-01");
        assert_eq!(v["population"]["shares"]["total"], 1000);
        assert_eq!(v["population"]["shares"]["unrecorded"]["male"], 300);
        // 総人口と男女合計が矛盾するときは割合を作らない (null)
        let mut bad = pop;
        bad["totals"]["total_population"] = json!(999);
        let v = serde_json::to_value(build_competitor_report(
            &agg(),
            10,
            "",
            &Value::Null,
            &Value::Null,
            &bad,
        ))
        .unwrap();
        assert!(v["population"]["shares"].is_null());
        // 人数が欠測の年齢帯は null のまま残る (0 にしない)
        let missing = json!({"status":"ok","region":"大阪府",
            "bands":[{"age_group":"20-24","male_count":null,"female_count":100}]});
        let v = serde_json::to_value(build_competitor_report(
            &agg(),
            10,
            "",
            &Value::Null,
            &Value::Null,
            &missing,
        ))
        .unwrap();
        assert!(v["population"]["bands"][0]["male"].is_null());
        assert_eq!(v["population"]["bands"][0]["female"], 100);
        assert_eq!(v["population"]["is_national"], false);
    }

    #[test]
    fn minimum_wage_link_only_passes_the_ministry_domain() {
        let pop = |url: &str| json!({"status":"ok","region":"大阪府","bands":[],"minimum_wage_source_url":url});
        let get = |url: &str| {
            serde_json::to_value(build_competitor_report(
                &agg(),
                10,
                "",
                &Value::Null,
                &Value::Null,
                &pop(url),
            ))
            .unwrap()["population"]["minimum_wage_source_url"]
                .clone()
        };
        assert_eq!(
            get("https://www.mhlw.go.jp/a.pdf"),
            json!("https://www.mhlw.go.jp/a.pdf")
        );
        // 公式の最低賃金サイト (サブドメイン) は通す
        assert_eq!(
            get("https://saiteichingin.mhlw.go.jp/table/page_list_nationallist.php"),
            json!("https://saiteichingin.mhlw.go.jp/table/page_list_nationallist.php")
        );
        assert!(get("https://evil.example/a.pdf").is_null());
        assert!(get("https://www.mhlw.go.jp.evil.example/a.pdf").is_null());
        assert!(get("https://www.mhlw.go.jp@evil.example/a.pdf").is_null());
        assert!(get("https://evilmhlw.go.jp/a.pdf").is_null());
        assert!(get("http://www.mhlw.go.jp/a.pdf").is_null());
        assert!(get("javascript:alert(1)").is_null());
    }

    #[test]
    fn consultation_and_comparison_are_in_the_model() {
        let r = build_competitor_report(
            &agg(),
            10,
            "",
            &json!({"status":"ok","title":"t","region":"r","rows":[]}),
            &json!({"status":"ok","keyword":"k","region":"","demand":{"status":"ok","region":null,"keywords":[]},"suggestions":{"status":"error"}}),
            &Value::Null,
        );
        let v = serde_json::to_value(&r).unwrap();
        let fetched: Vec<(&str, bool)> = v["consultation"]["external"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| {
                (
                    e["label"].as_str().unwrap(),
                    e["fetched"].as_bool().unwrap(),
                )
            })
            .collect();
        assert_eq!(
            fetched,
            [
                ("Google検索需要", true),
                ("Google関連語", false),
                ("Indeed採用市場", true),
                ("人口・地域", false)
            ]
        );
        // 月給の中央値は万円。2 件 (25, 35) の下限の中央値 30、人気求人は無いので差は null (0 にしない)
        let row = &v["consultation"]["salary"][0];
        assert_eq!(row["label"], "下限");
        assert_eq!(row["all_median"].as_f64(), Some(30.0));
        assert_eq!(row["popular_n"], 0);
        assert!(row["popular_median"].is_null());
        assert!(row["delta"].is_null());
        assert_eq!(v["excel"]["keyword_comparison"]["all_n"], 2);
        assert_eq!(v["excel"]["histograms"]["lower"]["n"], 2);
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
