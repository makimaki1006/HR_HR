//! Excel競合調査の独立した入口。CSV・Indeed・Googleをそれぞれの単位で表示する。
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum_extra::extract::Multipart;
use serde_json::{json, Value};

use crate::handlers::helpers::escape_html;
use crate::handlers::survey::aggregator::aggregate_records_with_mode;
use crate::handlers::survey::report_html::{parse_top_n, render_competitor_report};
use crate::handlers::survey::upload::{parse_csv_bytes_with_hints, UserSourceHint, WageMode};
use crate::indeed::data::{snapshot, Series, Snapshot};
use crate::AppState;

#[path = "competitor_pdf.rs"]
mod pdf;

#[path = "competitor_api.rs"]
mod api;
pub use api::{
    api_options, api_pdf, api_report, CompetitorError, CompetitorErrorCode, CompetitorOptions,
    CompetitorReportResponse,
};

#[cfg(test)]
#[path = "competitor_api_tests.rs"]
mod api_tests;

// 旧 HTML の golden(PR-1)。非公開関数を直接使うため子モジュールにしている。テスト専用。
#[cfg(test)]
#[path = "competitor_golden_tests.rs"]
mod golden_tests;

pub async fn page(State(state): State<Arc<AppState>>) -> Html<String> {
    let market = tokio::task::spawn_blocking(move || {
        state.indeed_db.as_ref().and_then(|db| snapshot(db).ok())
    })
    .await
    .ok()
    .flatten();
    let titles = market
        .map(|s| {
            let mut titles: Vec<_> = s.titles.iter().map(|t| t.name.as_str()).collect();
            titles.sort_unstable();
            titles.into_iter().map(option).collect::<String>()
        })
        .unwrap_or_default();
    let prefs = crate::models::job_seeker::PREFECTURE_ORDER
        .iter()
        .map(|p| option(p))
        .collect::<String>();
    Html(
        include_str!("../../templates/competitor.html")
            .replace("{{TITLE_OPTIONS}}", &titles)
            .replace("{{PREF_OPTIONS}}", &prefs)
            .replace(
                "{{MARKET_STATUS}}",
                if market.is_some() {
                    "Indeed採用市場データを利用できます。"
                } else {
                    "Indeed採用市場データは取得できません。CSVの競合調査は利用できます。"
                },
            ),
    )
}

fn option(value: &str) -> String {
    let value = escape_html(value);
    format!("<option value=\"{value}\">{value}</option>")
}

fn error(message: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Html(format!(
            "<!doctype html><html lang=\"ja\"><meta charset=\"utf-8\"><title>競合調査</title><h1>競合調査</h1><p role=\"alert\">{}</p><a href=\"/competitor\">入力画面へ戻る</a></html>",
            escape_html(message)
        )),
    )
        .into_response()
}

// ---------------------------------------------------------------- 旧画面 (/report/competitor) と JSON API の共通部分

/// 入力フォームの読み取り・検証で起きる失敗。`message` は旧画面の文言 (HTML エラーページ) をそのまま持つ。
/// JSON API は `code` と `status` を使い、文言は `api::form_error` が固定文にする。
struct FormError {
    code: api::CompetitorErrorCode,
    status: StatusCode,
    message: &'static str,
}

impl FormError {
    fn bad(code: api::CompetitorErrorCode, message: &'static str) -> Self {
        Self {
            code,
            status: StatusCode::BAD_REQUEST,
            message,
        }
    }
}

struct RawForm {
    csv: Vec<u8>,
    fields: std::collections::HashMap<String, String>,
}

fn read_error(
    e: &axum_extra::extract::multipart::MultipartError,
    message: &'static str,
) -> FormError {
    if e.status() == StatusCode::PAYLOAD_TOO_LARGE {
        FormError {
            code: api::CompetitorErrorCode::CsvTooLarge,
            status: StatusCode::PAYLOAD_TOO_LARGE,
            message,
        }
    } else {
        FormError::bad(api::CompetitorErrorCode::CsvUnreadable, message)
    }
}

async fn read_form(multipart: &mut Multipart) -> Result<RawForm, FormError> {
    let mut csv = Vec::new();
    let mut fields = std::collections::HashMap::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(e) => {
                return Err(read_error(
                    &e,
                    "CSVを読み込めませんでした。ファイルサイズと形式を確認してください。",
                ))
            }
        };
        let name = field.name().unwrap_or_default().to_owned();
        if name == "csv_file" {
            csv = match field.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(e) => return Err(read_error(&e, "CSVを読み込めませんでした。")),
            };
        } else {
            match field.text().await {
                Ok(value) if value.len() <= 1000 => {
                    fields.insert(name, value);
                }
                _ => {
                    return Err(FormError::bad(
                        api::CompetitorErrorCode::FieldTooLong,
                        "調査条件が長すぎるか、読み取れませんでした。",
                    ))
                }
            }
        }
    }
    Ok(RawForm { csv, fields })
}

/// 検証済みの入力。
struct ReportRequest {
    csv: Vec<u8>,
    source: UserSourceHint,
    mode: WageMode,
    top_n: usize,
    /// 利用者が送った `top_n` (空なら空文字)。
    top_n_raw: String,
    pref: String,
    market_title: String,
    survey_title: String,
    search_keyword: String,
    include_google: bool,
    output_format: String,
}

fn parse_request(raw: RawForm) -> Result<ReportRequest, FormError> {
    use api::CompetitorErrorCode as Code;
    let RawForm { csv, fields } = raw;
    if csv.is_empty() {
        return Err(FormError::bad(
            Code::CsvMissing,
            "求人一覧CSVを選択してください。ExcelブックはCSVに書き出してください。",
        ));
    }
    let get = |name: &str| {
        fields
            .get(name)
            .map(String::as_str)
            .unwrap_or_default()
            .trim()
    };
    let source = match get("source_type") {
        "indeed" => UserSourceHint::Indeed,
        "indeed_sp" => UserSourceHint::IndeedSp,
        _ => {
            return Err(FormError::bad(
                Code::InvalidSourceType,
                "IndeedまたはIndeed (SP)を選択してください。",
            ))
        }
    };
    let mode = match get("wage_mode") {
        "monthly" => WageMode::Monthly,
        "hourly" => WageMode::Hourly,
        _ => {
            return Err(FormError::bad(
                Code::InvalidWageMode,
                "月給または時給を選択してください。",
            ))
        }
    };
    let top_n = parse_top_n(Some(get("top_n")));
    let pref = get("prefecture").to_owned();
    if !pref.is_empty() && !crate::models::job_seeker::PREFECTURE_ORDER.contains(&pref.as_str()) {
        return Err(FormError::bad(
            Code::InvalidPrefecture,
            "対象都道府県を選択肢から選んでください。",
        ));
    }
    Ok(ReportRequest {
        source,
        mode,
        top_n,
        top_n_raw: get("top_n").to_owned(),
        market_title: get("market_title").to_owned(),
        survey_title: get("survey_title").to_owned(),
        search_keyword: get("search_keyword").to_owned(),
        include_google: get("include_google") == "1",
        output_format: get("output_format").to_owned(),
        pref,
        csv,
    })
}

#[derive(Debug)]
enum AnalyzeError {
    /// CSV の解析エラー。内部ライブラリの文字列を含むので、旧画面以外は画面に出さない。
    Parse(String),
    /// 行数の上限を超えた (件数)。
    TooManyRows(usize),
    /// 分析できる Indeed 求人が 0 件。
    NoIndeed,
}

/// 行数の上限判定 (純関数)。`max` ちょうどは通し、`max` + 1 から止める。`None` は上限なし。
fn check_row_limit(rows: usize, max: Option<usize>) -> Result<(), AnalyzeError> {
    match max {
        Some(max) if rows > max => Err(AnalyzeError::TooManyRows(rows)),
        _ => Ok(()),
    }
}

/// CSV の解析と集計。`max_rows` を超える行数なら集計せずに止める (旧画面は上限なし = `None`)。
async fn analyze(
    csv: Vec<u8>,
    source: UserSourceHint,
    mode: WageMode,
    pref: &str,
    max_rows: Option<usize>,
) -> Result<crate::handlers::survey::aggregator::SurveyAggregation, AnalyzeError> {
    let context_pref = (!pref.is_empty()).then(|| pref.to_owned());
    match tokio::task::spawn_blocking(move || {
        let records = parse_csv_bytes_with_hints(&csv, context_pref.as_deref(), source)
            .map_err(AnalyzeError::Parse)?;
        check_row_limit(records.len(), max_rows)?;
        Ok(aggregate_records_with_mode(&records, mode))
    })
    .await
    {
        Ok(Ok(agg)) if agg.total_count > 0 && agg.competitor.indeed_count > 0 => Ok(agg),
        Ok(Err(e)) => Err(e),
        _ => Err(AnalyzeError::NoIndeed),
    }
}

/// Indeed 採用市場・Google・人口の 3 つの外部コンテキスト (この順)。Google は選択されたときだけ呼ぶ。
async fn collect_context(state: Arc<AppState>, req: &ReportRequest) -> (Value, Value, Value) {
    let pref = req.pref.clone();
    let title = req.market_title.clone();
    let region_state = state.clone();
    let region_pref = pref.clone();
    let region_task =
        tokio::task::spawn_blocking(move || population_context(&region_state, &region_pref));
    let market = tokio::task::spawn_blocking(move || {
        state.indeed_db.as_ref().and_then(|db| snapshot(db).ok())
    })
    .await
    .ok()
    .flatten();
    let indeed = indeed_context(market, &title, &pref);
    let keyword = if req.search_keyword.is_empty() && !title.is_empty() {
        format!("{title} 求人")
    } else {
        req.search_keyword.clone()
    };
    let google = if req.include_google && !keyword.is_empty() {
        google_context(&keyword, &pref).await
    } else {
        json!({"status":"not_requested", "message":"検索需要を取得するには検索語を指定し、Google広告APIの取得を選択してください。"})
    };
    let population = region_task.await.unwrap_or_else(
        |_| json!({"status":"unavailable","message":"人口・地域データを取得できませんでした。"}),
    );
    (indeed, google, population)
}

pub async fn report(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let mut req = match read_form(&mut multipart).await.and_then(parse_request) {
        Ok(req) => req,
        Err(e) => return error(e.message),
    };
    let csv = std::mem::take(&mut req.csv);
    let agg = match analyze(csv, req.source, req.mode, &req.pref, None).await {
        Ok(agg) => agg,
        Err(AnalyzeError::Parse(message)) => return error(&message),
        Err(_) => {
            return error("分析できるIndeed求人がありません。CSVの列と内容を確認してください。")
        }
    };
    let (indeed, google, population) = collect_context(state, &req).await;
    let html = render_competitor_report(
        &agg,
        req.top_n,
        &req.survey_title,
        &indeed,
        &google,
        &population,
    );
    if req.output_format == "pdf" {
        match pdf::generate(&html).await {
            Ok(bytes) => (
                [
                    ("content-type", "application/pdf"),
                    (
                        "content-disposition",
                        "attachment; filename=\"competitor-report.pdf\"",
                    ),
                    ("cache-control", "no-store"),
                ],
                bytes,
            )
                .into_response(),
            Err(message) => (StatusCode::SERVICE_UNAVAILABLE, message).into_response(),
        }
    } else {
        Html(html).into_response()
    }
}

pub(super) fn population_context(state: &AppState, pref: &str) -> Value {
    use crate::handlers::regional_analysis::fetch::{
        fetch_labor_stats, fetch_population_report_rows, fetch_population_report_totals,
        fetch_wage_comparison, RegionalFilter,
    };

    let filter = RegionalFilter {
        prefecture: pref.to_owned(),
        ..Default::default()
    };
    let bands = fetch_population_report_rows(state, pref);
    let totals = fetch_population_report_totals(state, pref);
    if pref.is_empty() {
        return if bands.is_empty() && totals.is_null() {
            json!({"status":"unavailable","region":"全国","message":"全国の人口データを取得できませんでした。"})
        } else {
            json!({"status":"ok","region":"全国","bands":bands,"reference_date":totals["reference_date"],"totals":totals})
        };
    }
    let wage = fetch_wage_comparison(state, &filter);
    let labor = fetch_labor_stats(state, &filter);
    json!({"status":"ok","region":pref,"bands":bands,"reference_date":totals["reference_date"],"totals":totals,"minimum_wage":wage.hourly_min_wage,"minimum_wage_fiscal_year":wage.fiscal_year,"minimum_wage_effective_date":wage.effective_date,"minimum_wage_source_url":wage.source_url,"minimum_wage_source":wage.source,"minimum_wage_as_of":wage.as_of,"labor":labor.map(|l|json!({"fiscal_year":l.fiscal_year,"unemployment_rate":l.unemployment_rate,"separation_rate":l.separation_rate}))})
}

fn indeed_context(market: Option<&Snapshot>, title: &str, pref: &str) -> Value {
    let Some(snap) = market else {
        return json!({"status":"unavailable", "message":"Indeed採用市場データを取得できませんでした。"});
    };
    let series: Option<&Series> = if pref.is_empty() {
        snap.by_title.get(title)
    } else {
        snap.by_pref
            .iter()
            .find(|r| r.title == title && r.prefecture == pref)
            .map(|r| &r.series)
    };
    let Some(series) = series else {
        return json!({"status":"unavailable", "message":"選択した職種・地域のIndeedデータがありません。職種を指定しているか確認してください。"});
    };
    let spp = series.seekers_per_posting();
    json!({
        "status":"ok", "title":title, "region":if pref.is_empty() {"全国"} else {pref},
        "source":snap.meta.source, "caveat":snap.meta.caveat, "built_at":snap.meta.built_at,
        "rows":snap.meta.months.iter().enumerate().map(|(i, month)| json!({
            "month":month, "job":series.job.get(i).copied().flatten(),
            "ctk":series.ctk.get(i).copied().flatten(), "emp":series.emp.get(i).copied().flatten(),
            "spp":spp.get(i).copied().flatten()
        })).collect::<Vec<_>>()
    })
}

async fn google_context(keyword: &str, pref: &str) -> Value {
    use crate::media_engine::handlers::{
        keywords_endpoint, suggest_endpoint, KeywordsQuery, SuggestQuery,
    };
    let region = (!pref.is_empty()).then_some(pref);
    // ハンドラの入力・資格情報・地域解決・API実装を共有。ブラウザから実測値は受け取らない。
    let demand: KeywordsQuery = match serde_json::from_value(json!({"kw":keyword,"region":region}))
    {
        Ok(query) => query,
        Err(_) => return json!({"status":"error", "message":"検索条件を読み取れませんでした。"}),
    };
    let suggestions: SuggestQuery = match serde_json::from_value(
        json!({"seed":keyword,"region":region,"limit":20}),
    ) {
        Ok(query) => query,
        Err(_) => {
            return json!({"status":"error", "message":"関連語の検索条件を読み取れませんでした。"})
        }
    };
    let timeout = std::time::Duration::from_secs(45);
    let (demand, suggestions) = tokio::join!(
        tokio::time::timeout(timeout, keywords_endpoint(Query(demand))),
        tokio::time::timeout(timeout, suggest_endpoint(Query(suggestions)))
    );
    let timeout_result = || json!({"status":"timeout"});
    json!({
        "status":"ok", "keyword":keyword,"region":pref,
        "demand":demand.map(|response| response.0).unwrap_or_else(|_| timeout_result()),
        "suggestions":suggestions.map(|response| response.0).unwrap_or_else(|_| timeout_result())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::indeed::data::PrefSeries;

    #[test]
    fn indeed_region_match_preserves_missing_values_and_does_not_fall_back_to_nation() {
        let series = Series {
            job: vec![Some(100.0), None],
            ctk: vec![Some(250.0), Some(9.0)],
            emp: vec![Some(20.0), None],
        };
        let mut snap = Snapshot::default();
        snap.meta.months = vec!["2026-07".into(), "2026-08".into()];
        snap.by_title.insert("施設長".into(), series.clone());
        snap.by_pref.push(PrefSeries {
            prefecture: "大阪府".into(),
            title: "施設長".into(),
            series,
            mobile_pct: None,
        });
        let data = indeed_context(Some(&snap), "施設長", "大阪府");
        assert_eq!(data["rows"][0]["spp"], 2.5);
        assert!(data["rows"][1]["job"].is_null());
        assert!(data["rows"][1]["spp"].is_null());
        assert_eq!(
            indeed_context(Some(&snap), "施設長", "東京都")["status"],
            "unavailable"
        );
    }
}
