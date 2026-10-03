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

pub async fn report(State(state): State<Arc<AppState>>, mut multipart: Multipart) -> Response {
    let mut csv = Vec::new();
    let mut fields = std::collections::HashMap::new();
    loop {
        let field = match multipart.next_field().await {
            Ok(Some(field)) => field,
            Ok(None) => break,
            Err(_) => {
                return error("CSVを読み込めませんでした。ファイルサイズと形式を確認してください。")
            }
        };
        let name = field.name().unwrap_or_default().to_owned();
        if name == "csv_file" {
            csv = match field.bytes().await {
                Ok(bytes) => bytes.to_vec(),
                Err(_) => return error("CSVを読み込めませんでした。"),
            };
        } else {
            match field.text().await {
                Ok(value) if value.len() <= 1000 => {
                    fields.insert(name, value);
                }
                _ => return error("調査条件が長すぎるか、読み取れませんでした。"),
            }
        }
    }
    if csv.is_empty() {
        return error("求人一覧CSVを選択してください。ExcelブックはCSVに書き出してください。");
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
        _ => return error("IndeedまたはIndeed (SP)を選択してください。"),
    };
    let mode = match get("wage_mode") {
        "monthly" => WageMode::Monthly,
        "hourly" => WageMode::Hourly,
        _ => return error("月給または時給を選択してください。"),
    };
    let top_n = parse_top_n(Some(get("top_n")));
    let pref = get("prefecture").to_owned();
    if !pref.is_empty() && !crate::models::job_seeker::PREFECTURE_ORDER.contains(&pref.as_str()) {
        return error("対象都道府県を選択肢から選んでください。");
    }
    let context_pref = (!pref.is_empty()).then(|| pref.clone());
    let agg = match tokio::task::spawn_blocking(move || {
        let records = parse_csv_bytes_with_hints(&csv, context_pref.as_deref(), source)?;
        Ok::<_, String>(aggregate_records_with_mode(&records, mode))
    })
    .await
    {
        Ok(Ok(agg)) if agg.total_count > 0 && agg.competitor.indeed_count > 0 => agg,
        Ok(Err(message)) => return error(&message),
        _ => return error("分析できるIndeed求人がありません。CSVの列と内容を確認してください。"),
    };
    let title = get("market_title").to_owned();
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
    let keyword = if get("search_keyword").is_empty() && !title.is_empty() {
        format!("{title} 求人")
    } else {
        get("search_keyword").to_owned()
    };
    let google = if get("include_google") == "1" && !keyword.is_empty() {
        google_context(&keyword, &pref).await
    } else {
        json!({"status":"not_requested", "message":"検索需要を取得するには検索語を指定し、Google広告APIの取得を選択してください。"})
    };
    let population = region_task.await.unwrap_or_else(
        |_| json!({"status":"unavailable","message":"人口・地域データを取得できませんでした。"}),
    );
    let html = render_competitor_report(
        &agg,
        top_n,
        get("survey_title"),
        &indeed,
        &google,
        &population,
    );
    if get("output_format") == "pdf" {
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
        fetch_labor_stats, fetch_population_pyramid, fetch_wage_comparison, RegionalFilter,
    };
    if pref.is_empty() {
        return json!({"status":"unavailable","message":"人口・地域データを表示するには、入力画面で対象都道府県を選択してください。"});
    }
    let filter = RegionalFilter {
        prefecture: pref.to_owned(),
        ..Default::default()
    };
    let pyramid = fetch_population_pyramid(state, &filter);
    let wage = fetch_wage_comparison(state, &filter);
    let labor = fetch_labor_stats(state, &filter);
    json!({"status":"ok","region":pref,"bands":pyramid.bands.iter().map(|b| json!({"age_group":b.age_group,"male_count":b.male_count,"female_count":b.female_count})).collect::<Vec<_>>(),"minimum_wage":wage.hourly_min_wage,"minimum_wage_fiscal_year":wage.fiscal_year,"minimum_wage_effective_date":wage.effective_date,"minimum_wage_source_url":wage.source_url,"minimum_wage_source":wage.source,"minimum_wage_as_of":wage.as_of,"labor":labor.map(|l|json!({"fiscal_year":l.fiscal_year,"unemployment_rate":l.unemployment_rate,"separation_rate":l.separation_rate}))})
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
