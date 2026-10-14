//! Read-only: the application dates of every listing in one deal (案件の画面の週ごとの応募)。
//! Counts and dates only; no applicant attribute leaves the server. The relation listing -> deal
//! is checked against HubSpot for each listing, so a browser-provided id is not trusted.
use super::listings::{
    airwork_weekly_perf, indeed_weekly_perf, AIRWORK_WEEKLY_PERF, INDEED_WEEKLY_PERF,
};
use super::{authorized_user, valid_id, Access, ReadError, Record};
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, FixedOffset, NaiveDate};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
use tower_sessions::Session;

const MAX_LISTINGS: usize = 30;

#[derive(Deserialize)]
pub(super) struct DealSummaryQuery {
    deal: String,
    ids: String,
}

/// The calendar day (JST) of an application date: a plain `YYYY-MM-DD`, or a timestamp
/// (RFC 3339) converted to Japan time. None when unreadable.
fn jst_day(value: &str) -> Option<String> {
    let value = value.trim();
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return Some(date.format("%Y-%m-%d").to_string());
    }
    let jst = FixedOffset::east_opt(9 * 3600)?;
    DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.with_timezone(&jst).format("%Y-%m-%d").to_string())
}

/// Counts of dated applications by day, and the number without a usable date.
fn count_by_date(days: &[Option<String>]) -> (BTreeMap<String, usize>, usize) {
    let mut by_date = BTreeMap::new();
    let mut missing = 0;
    for day in days {
        match day {
            Some(day) => *by_date.entry(day.clone()).or_insert(0) += 1,
            None => missing += 1,
        }
    }
    (by_date, missing)
}

/// Applications linked to more than one of the requested listings. Each is counted once per
/// listing in the listing's own series, so the deal total must subtract the extra copies:
/// (number of such applications, extra copies by day, extra copies without a date).
fn shared_overlap(
    per_listing: &[Vec<(String, Option<String>)>],
) -> (usize, BTreeMap<String, usize>, usize) {
    let mut seen = BTreeMap::<&str, (usize, Option<&str>)>::new();
    for listing in per_listing {
        for (id, day) in listing {
            let entry = seen.entry(id.as_str()).or_insert((0, day.as_deref()));
            entry.0 += 1;
        }
    }
    let mut by_day = BTreeMap::new();
    let mut undated = 0;
    let mut shared = 0;
    for (times, day) in seen.values().filter(|(times, _)| *times > 1) {
        shared += 1;
        match day {
            Some(day) => *by_day.entry((*day).to_owned()).or_insert(0) += times - 1,
            None => undated += times - 1,
        }
    }
    (shared, by_day, undated)
}

/// The weekly results (Indeed for HRハッカー, AirWork for AirWork) of each listing, read for all of
/// them in one batch call. A listing with no usable value is simply absent (the screen shows
/// "no billing data").
fn perf_by_listing(records: &[Record]) -> BTreeMap<String, (&'static str, Value)> {
    records
        .iter()
        .filter_map(|record| {
            if let Some(v) = indeed_weekly_perf(record) {
                return Some((record.id.clone(), ("indeed_weekly_perf", v)));
            }
            Some((
                record.id.clone(),
                ("airwork_weekly_perf", airwork_weekly_perf(record)?),
            ))
        })
        .collect()
}

/// The one company related to the deal (id, name). None when there are none or several.
async fn deal_company(
    service: &super::JobReadService,
    deal: &str,
) -> Result<Option<(String, String)>, ReadError> {
    let companies = service.all_associations("deals", deal, "companies").await?;
    let [company] = companies.as_slice() else {
        return Ok(None);
    };
    let records = service
        .batch("companies", std::slice::from_ref(company), &["name"])
        .await?;
    Ok(records
        .first()
        .and_then(|record| record.value("name"))
        .map(|name| (company.clone(), name.to_owned())))
}

#[derive(Deserialize)]
pub(super) struct CompanyDealsQuery {
    company: String,
}

/// The deals of one company (事業所), with their names. Read-only; ids only come from HubSpot.
pub(super) async fn company_deals(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<CompanyDealsQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    valid_id(&query.company)?;
    let ids = service
        .all_associations("companies", &query.company, "deals")
        .await?;
    let records = service.batch("deals", &ids, &["dealname"]).await?;
    let deals: Vec<Value> = records
        .iter()
        .map(|r| json!({"id": r.id, "name": r.value("dealname")}))
        .collect();
    let name = service
        .batch("companies", std::slice::from_ref(&query.company), &["name"])
        .await?
        .first()
        .and_then(|record| record.value("name").map(str::to_owned));
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"company": {"id": query.company, "name": name}, "deals": deals})),
    ))
}

pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<DealSummaryQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    valid_id(&query.deal)?;
    let ids: Vec<String> = query
        .ids
        .split(',')
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    if ids.is_empty() || ids.len() > MAX_LISTINGS {
        return Err(ReadError(StatusCode::BAD_REQUEST, "invalid_listing_ids"));
    }
    for id in &ids {
        valid_id(id)?;
    }
    let mut listings = Vec::new();
    let mut linked = Vec::new();
    for id in &ids {
        let read: Result<(Value, Vec<(String, Option<String>)>), ReadError> = async {
            let deals = service.all_associations("0-420", id, "deals").await?;
            if !deals.contains(&query.deal) {
                return Err(ReadError(StatusCode::FORBIDDEN, "listing_not_in_deal"));
            }
            let applications = service.all_associations("0-420", id, "0-421").await?;
            let rows = service.batch("0-421", &applications, &["yingmuri"]).await?;
            let days: Vec<Option<String>> = rows
                .iter()
                .map(|r| r.value("yingmuri").and_then(jst_day))
                .collect();
            let (by_date, missing) = count_by_date(&days);
            let pairs = rows.iter().map(|r| r.id.clone()).zip(days).collect();
            Ok((
                json!({"listing_id":id,"total":rows.len(),"missing_date":missing,"by_date":by_date,
                    "fetched_at":chrono::Utc::now().to_rfc3339()}),
                pairs,
            ))
        }
        .await;
        match read {
            Ok((value, pairs)) => {
                listings.push(value);
                linked.push(pairs);
            }
            // The deal relation is a permission check: fail the whole request.
            Err(error) if error.0 == StatusCode::FORBIDDEN => return Err(error),
            Err(error) => listings.push(json!({"listing_id":id,"error":error.1})),
        }
    }
    let (shared, shared_by_date, shared_missing_date) = shared_overlap(&linked);
    // One batch call for every listing's weekly results; a failure only hides billing.
    let perf = match service
        .batch(
            "0-420",
            &ids,
            &[INDEED_WEEKLY_PERF, AIRWORK_WEEKLY_PERF, "id_airwork"],
        )
        .await
    {
        Ok(records) => perf_by_listing(&records),
        Err(error) => {
            tracing::warn!(code = error.1, "job-copy deal billing read failed");
            BTreeMap::new()
        }
    };
    for entry in &mut listings {
        let found = entry["listing_id"].as_str().and_then(|id| perf.get(id));
        entry["indeed_weekly_perf"] = Value::Null;
        entry["airwork_weekly_perf"] = Value::Null;
        if let Some((key, value)) = found {
            entry[*key] = value.clone();
        }
    }
    let company = deal_company(service, &query.deal).await.ok().flatten();
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "deal_id": query.deal,
            "company": company.map(|(id, name)| json!({"id": id, "name": name})),
            "fetched_at": chrono::Utc::now().to_rfc3339(),
            "listings": listings,
            "shared_applications": shared,
            "shared_by_date": shared_by_date,
            "shared_missing_date": shared_missing_date,
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn airwork_rows_are_read_with_their_own_id() {
        let aw = json!({"schema_version":1,"source":"airwork","job_id":"555","captured_at":"2026-10-05T00:00:00Z",
            "rows":[{"period_start":"2026-09-28","period_end":"2026-10-04","impressions":100,"clicks":5,"cost_yen":8000,"applications":2}]})
        .to_string();
        let rec = |id: &str, own: &str| -> Record {
            serde_json::from_value(
                json!({"id":id,"properties":{"id_airwork":own,"airwork_weekly_perf":aw}}),
            )
            .unwrap()
        };
        let map = perf_by_listing(&[rec("7", "555"), rec("8", "556")]);
        assert_eq!(map.len(), 1);
        assert_eq!(map["7"].0, "airwork_weekly_perf");
        assert_eq!(map["7"].1["rows"][0]["cost_yen"], 8000);
    }

    #[test]
    fn perf_is_taken_from_valid_json_and_bad_values_are_ignored() {
        let rec = |id: &str, v: Value| -> Record {
            serde_json::from_value(json!({"id":id,"properties":{"indeed_weekly_perf":v}})).unwrap()
        };
        let good = json!({"schema_version":1,"source":"hrhacker","job_id":"1234567","captured_at":"2026-10-05T00:00:00Z",
            "rows":[{"period_start":"2026-09-28","period_end":"2026-10-04","impressions":100,"clicks":5,"cost_yen":12345.5,"applications":2}]})
        .to_string();
        let records = vec![
            rec("1", json!(good)),
            rec("2", json!("{not json")),
            rec("3", json!(null)),
            rec(
                "4",
                json!(r#"{"schema_version":2,"source":"hrhacker","rows":[]}"#),
            ),
        ];
        let map = perf_by_listing(&records);
        assert_eq!(map.len(), 1);
        assert_eq!(map["1"].0, "indeed_weekly_perf");
        assert_eq!(map["1"].1["rows"][0]["cost_yen"], 12345.5);
        assert!(!map.contains_key("2") && !map.contains_key("3") && !map.contains_key("4"));
    }

    #[test]
    fn counts_dated_and_undated_applications() {
        let days: Vec<Option<String>> = [
            Some("2026-10-01"),
            Some("2026-10-01"),
            Some("2026-10-03"),
            None,
            Some("bad"),
        ]
        .iter()
        .map(|v| v.and_then(jst_day))
        .collect();
        let (by_date, missing) = count_by_date(&days);
        assert_eq!(by_date.get("2026-10-01"), Some(&2));
        assert_eq!(by_date.get("2026-10-03"), Some(&1));
        assert_eq!(missing, 2);
    }

    #[test]
    fn timestamps_are_counted_on_their_japan_time_day() {
        // 2026-09-30 16:30 UTC is 2026-10-01 01:30 in Japan.
        assert_eq!(
            jst_day("2026-09-30T16:30:00Z").as_deref(),
            Some("2026-10-01")
        );
        assert_eq!(
            jst_day("2026-10-01T08:00:00+09:00").as_deref(),
            Some("2026-10-01")
        );
        assert_eq!(jst_day("2026-10-01").as_deref(), Some("2026-10-01"));
        assert_eq!(jst_day("not a date"), None);
    }

    #[test]
    fn shared_applications_are_subtracted_by_day() {
        let app = |id: &str, day: Option<&str>| (id.to_owned(), day.map(str::to_owned));
        let a = vec![
            app("1", Some("2026-10-01")),
            app("2", Some("2026-10-02")),
            app("3", None),
        ];
        let b = vec![app("3", None), app("4", Some("2026-10-01"))];
        let c = vec![app("2", Some("2026-10-02")), app("3", None)];
        let (shared, by_day, undated) = shared_overlap(&[a, b, c]);
        // 2 is in a and c (1 extra on 10-02); 3 is in a, b and c (2 extra, undated).
        assert_eq!(shared, 2);
        assert_eq!(by_day.get("2026-10-02"), Some(&1));
        assert_eq!(by_day.len(), 1);
        assert_eq!(undated, 2);
    }
}
