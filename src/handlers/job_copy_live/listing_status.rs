//! 媒体の公開状況 (latest) of the snapshot's listings, read from HubSpot (read only).
//!
//! The values are what the HRハッカー CSV import last wrote on the listing: the publication start,
//! the end date set on the media (a planned end, often far in the future, never the day it really
//! stopped), the raw status and the last day the listing was in the CSV. Only the latest values
//! exist; past publication periods are not guessed. AirWork has no such dates.
use super::{authorized_user, fail, raw_moc, valid_id, Access, JobReadService, ReadError};
use crate::AppState;
use axum::{
    extract::State,
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::{DateTime, FixedOffset, NaiveDate, Timelike, Utc};
use serde_json::{json, Map, Value};
use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tower_sessions::Session;

const START: &str = "koukai_kaishi_nichiji";
const PLANNED_END: &str = "koukai_shuuryou_nichiji";
const STATUS: &str = "baitai_genjoukyou_hrhakkaa";
const LAST_IN_CSV: &str = "saishuu_csv_kenshutsu_bi";
const HRH_ID: &str = "id_hrhakkaa";
/// The key is shared with other batches: one batch read per listing set per this long.
pub(super) const CACHE_TTL: Duration = Duration::from_secs(15 * 60);
/// A failed read is kept this long, so waiting or repeated requests do not call HubSpot again
/// (the key is shared; a 429 must not turn into a burst).
pub(super) const FAILURE_TTL: Duration = Duration::from_secs(60);
/// The whole read gives up after this (the snapshot has at most 59 listings: two chunks).
const READ_TIMEOUT: Duration = Duration::from_secs(25);
/// HubSpot's batch read takes fewer inputs when history is asked for.
const CHUNK: usize = 50;

pub(super) struct Cached {
    at: Instant,
    listings: Vec<String>,
    result: Result<Value, (StatusCode, &'static str)>,
}
pub(super) type Cache = Arc<tokio::sync::Mutex<Option<Cached>>>;

fn jst() -> FixedOffset {
    FixedOffset::east_opt(9 * 3600).expect("valid offset")
}

/// A HubSpot date or datetime value as a calendar day. A date-only value is stored as midnight
/// UTC and is read as that day; any other time is read as the JST day.
fn day(raw: Option<&str>) -> Option<String> {
    let raw = raw?.trim();
    if raw.is_empty() {
        return None;
    }
    if let Ok(date) = NaiveDate::parse_from_str(raw, "%Y-%m-%d") {
        return (date.to_string() == raw).then(|| date.to_string());
    }
    let at: DateTime<Utc> = if raw.bytes().all(|b| b.is_ascii_digit()) {
        DateTime::from_timestamp_millis(raw.parse().ok()?)?
    } else {
        DateTime::parse_from_rfc3339(raw).ok()?.with_timezone(&Utc)
    };
    let date = if at.num_seconds_from_midnight() == 0 && at.nanosecond() == 0 {
        at.date_naive()
    } else {
        at.with_timezone(&jst()).date_naive()
    };
    Some(date.to_string())
}

fn status(raw: Option<&str>) -> Option<&'static str> {
    match raw?.trim() {
        "" => None,
        "公開" => Some("public"),
        "非公開" => Some("private"),
        _ => Some("unknown"),
    }
}

/// The day (JST) the change from 公開 to 非公開 was recorded, when the latest status is 非公開 and
/// the history shows that change. The listing stopped on that day or before it.
fn private_recorded_on(history: &Value) -> Option<String> {
    let mut entries: Vec<(DateTime<Utc>, &str)> = history
        .as_array()?
        .iter()
        .filter_map(|entry| {
            let at = DateTime::parse_from_rfc3339(entry["timestamp"].as_str()?)
                .ok()?
                .with_timezone(&Utc);
            Some((at, entry["value"].as_str().unwrap_or("").trim()))
        })
        .collect();
    entries.sort_by_key(|entry| std::cmp::Reverse(entry.0));
    let run = entries
        .iter()
        .take_while(|(_, value)| *value == "非公開")
        .count();
    match (run, entries.get(run)) {
        (1.., Some((_, "公開"))) => entries
            .get(run - 1)
            .map(|(at, _)| at.with_timezone(&jst()).date_naive().to_string()),
        _ => None,
    }
}

/// One listing of a batch read result, in plain field names (no HubSpot property names leave).
pub fn summarize(record: &Value) -> Value {
    let value = |key: &str| record["properties"][key].as_str();
    let hrhacker = value(HRH_ID).is_some_and(|id| !id.trim().is_empty());
    if !hrhacker {
        return json!({"hrhacker": false});
    }
    let status = status(value(STATUS));
    json!({
        "hrhacker": true,
        "start": day(value(START)),
        "planned_end": day(value(PLANNED_END)),
        "status": status,
        "last_in_csv": day(value(LAST_IN_CSV)),
        "private_recorded_on": if status == Some("private") {
            private_recorded_on(&record["propertiesWithHistory"][STATUS])
        } else {
            None
        },
    })
}

/// The listing IDs of the snapshot (capture_bundle.jobs[].hubspotListingId), sorted.
fn snapshot_listings(snapshot: &Value) -> Result<Vec<String>, ReadError> {
    let jobs = snapshot["capture_bundle"]["jobs"]
        .as_array()
        .ok_or_else(|| fail("moc_invalid"))?;
    let mut ids = BTreeSet::new();
    for job in jobs {
        let id = job["hubspotListingId"]
            .as_str()
            .ok_or_else(|| fail("moc_invalid"))?;
        valid_id(id).map_err(|_| fail("moc_invalid"))?;
        ids.insert(id.to_owned());
    }
    Ok(ids.into_iter().collect())
}

impl JobReadService {
    async fn read_publication(&self, listings: &[String]) -> Result<Value, ReadError> {
        let mut out = Map::new();
        for chunk in listings.chunks(CHUNK) {
            let data = self
                .request(
                    "/crm/v3/objects/0-420/batch/read",
                    &[],
                    Some(json!({
                        "properties": [START, PLANNED_END, STATUS, LAST_IN_CSV, HRH_ID],
                        "propertiesWithHistory": [STATUS],
                        "inputs": chunk.iter().map(|id| json!({"id": id})).collect::<Vec<_>>(),
                    })),
                )
                .await?;
            // A listing HubSpot does not return (archived or deleted: a 207 reply with errors) is
            // left out; the screen shows it as not read. Rows that were not asked for are dropped.
            let rows = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            let expected: BTreeSet<&str> = chunk.iter().map(String::as_str).collect();
            for row in rows {
                if let Some(id) = row["id"].as_str().filter(|id| expected.contains(id)) {
                    out.insert(id.to_owned(), summarize(row));
                }
            }
        }
        Ok(json!({"fetched_at": Utc::now().to_rfc3339(), "listings": out}))
    }

    /// Cached for CACHE_TTL per listing set (a failure for FAILURE_TTL); one read at a time (the
    /// lock is held while reading, so requests waiting on it reuse its result).
    pub(super) async fn listing_publication(
        &self,
        listings: &[String],
    ) -> Result<Value, ReadError> {
        let mut cache = self.publication.lock().await;
        if let Some(cached) = cache.as_ref().filter(|c| {
            c.listings == listings
                && c.at.elapsed()
                    < if c.result.is_ok() {
                        CACHE_TTL
                    } else {
                        FAILURE_TTL
                    }
        }) {
            return cached
                .result
                .clone()
                .map_err(|(status, code)| ReadError(status, code));
        }
        let result = tokio::time::timeout(READ_TIMEOUT, self.read_publication(listings))
            .await
            .unwrap_or_else(|_| Err(ReadError(StatusCode::GATEWAY_TIMEOUT, "hubspot_timeout")));
        *cache = Some(Cached {
            at: Instant::now(),
            listings: listings.to_vec(),
            result: result
                .as_ref()
                .map(Value::clone)
                .map_err(|e| (e.status(), e.code())),
        });
        result
    }
}

/// GET /api/job-copy/listing-status: the 媒体の公開状況 of every listing in the snapshot.
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let listings = snapshot_listings(&*raw_moc(&access).await?)?;
    let data = service.listing_publication(&listings).await?;
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{routing::post, Router};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn listing(properties: Value, history: Value) -> Value {
        json!({"id": "1", "properties": properties, "propertiesWithHistory": {STATUS: history}})
    }

    #[test]
    fn public_listing_keeps_the_dates_as_days_and_the_planned_end_apart() {
        let row = listing(
            json!({HRH_ID: "H1", START: "2026-08-24T00:00:00Z", PLANNED_END: "2027-07-06T00:00:00Z", STATUS: "公開", LAST_IN_CSV: "2026-10-08"}),
            json!([{"value": "公開", "timestamp": "2026-08-25T01:00:00Z"}]),
        );
        assert_eq!(
            summarize(&row),
            json!({"hrhacker": true, "start": "2026-08-24", "planned_end": "2027-07-06", "status": "public", "last_in_csv": "2026-10-08", "private_recorded_on": null})
        );
    }

    #[test]
    fn private_listing_names_the_day_the_change_was_recorded_only_when_history_shows_it() {
        let props = json!({HRH_ID: "H1", START: "2026-04-08T00:00:00Z", PLANNED_END: "2030-01-01T00:00:00Z", STATUS: "非公開", LAST_IN_CSV: "2026-10-07"});
        // 公開 -> 非公開 recorded at 2026-09-30 16:00 UTC = 2026-10-01 JST; a later 非公開 write
        // does not move the day.
        let changed = listing(
            props.clone(),
            json!([
                {"value": "非公開", "timestamp": "2026-10-05T00:00:00Z"},
                {"value": "非公開", "timestamp": "2026-09-30T16:00:00Z"},
                {"value": "公開", "timestamp": "2026-08-25T01:00:00Z"},
            ]),
        );
        let summary = summarize(&changed);
        assert_eq!(summary["status"], "private");
        assert_eq!(summary["planned_end"], "2030-01-01");
        assert_eq!(summary["private_recorded_on"], "2026-10-01");
        // Only 非公開 in the history (the sync started after it stopped): the day is unknown.
        let unknown = listing(
            props,
            json!([{"value": "非公開", "timestamp": "2026-07-09T00:00:00Z"}]),
        );
        assert_eq!(summarize(&unknown)["private_recorded_on"], Value::Null);
    }

    #[test]
    fn missing_or_odd_values_stay_unknown_and_airwork_gets_nothing() {
        let row = listing(
            json!({HRH_ID: "H1", START: "not a date", STATUS: "停止中", LAST_IN_CSV: ""}),
            json!([]),
        );
        assert_eq!(
            summarize(&row),
            json!({"hrhacker": true, "start": null, "planned_end": null, "status": "unknown", "last_in_csv": null, "private_recorded_on": null})
        );
        let airwork =
            json!({"id": "2", "properties": {"id_airwork": "A1", START: "2026-01-01T00:00:00Z"}});
        assert_eq!(summarize(&airwork), json!({"hrhacker": false}));
        // A time other than midnight UTC is read as the JST day; epoch milliseconds are accepted.
        assert_eq!(
            day(Some("2026-08-24T20:00:00Z")).as_deref(),
            Some("2026-08-25")
        );
        assert_eq!(day(Some("1724457600000")).as_deref(), Some("2024-08-24"));
        assert_eq!(day(Some("2026-02-30")), None);
    }

    #[test]
    fn snapshot_listings_are_sorted_unique_and_validated() {
        let data = json!({"capture_bundle": {"jobs": [{"hubspotListingId": "30"}, {"hubspotListingId": "4"}, {"hubspotListingId": "30"}]}});
        assert_eq!(
            snapshot_listings(&data).unwrap(),
            vec!["30".to_owned(), "4".to_owned()]
        );
        let bad = json!({"capture_bundle": {"jobs": [{"hubspotListingId": "../x"}]}});
        assert!(snapshot_listings(&bad).is_err());
    }

    async fn upstream(reply: Value) -> (String, Arc<AtomicUsize>, Arc<std::sync::Mutex<Value>>) {
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = Arc::new(std::sync::Mutex::new(Value::Null));
        let (c, s) = (calls.clone(), seen.clone());
        let app = Router::new().route(
            "/crm/v3/objects/0-420/batch/read",
            post(move |Json(body): Json<Value>| {
                let (c, s, reply) = (c.clone(), s.clone(), reply.clone());
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    *s.lock().unwrap() = body;
                    Json(reply)
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        (base, calls, seen)
    }

    #[tokio::test]
    async fn one_batch_read_is_cached_and_a_missing_listing_is_left_out() {
        let reply = json!({"results": [
            {"id": "4", "properties": {HRH_ID: "H4", START: "2026-09-01T00:00:00Z", STATUS: "公開", LAST_IN_CSV: "2026-10-08"}},
            {"id": "30", "properties": {"id_airwork": "A30"}},
            {"id": "77", "properties": {HRH_ID: "H77"}},
        ], "errors": [{"status": "error", "category": "OBJECT_NOT_FOUND"}]});
        let (base, calls, seen) = upstream(reply).await;
        let service = JobReadService::for_test(base);
        let ids = vec!["30".to_owned(), "4".to_owned(), "5".to_owned()];
        let first = service.listing_publication(&ids).await.unwrap();
        assert_eq!(first["listings"]["4"]["start"], "2026-09-01");
        assert_eq!(first["listings"]["4"]["status"], "public");
        assert_eq!(first["listings"]["30"], json!({"hrhacker": false}));
        // Not returned (archived): left out, not filled in. Not asked for: dropped.
        assert!(first["listings"].get("5").is_none());
        assert!(first["listings"].get("77").is_none());
        assert_eq!(
            seen.lock().unwrap()["propertiesWithHistory"],
            json!([STATUS])
        );
        let second = service.listing_publication(&ids).await.unwrap();
        assert_eq!(first, second);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn a_failed_read_is_kept_briefly_so_requests_do_not_call_again() {
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        let app = Router::new().route(
            "/crm/v3/objects/0-420/batch/read",
            post(move || {
                let c = c.clone();
                async move {
                    c.fetch_add(1, Ordering::SeqCst);
                    (StatusCode::TOO_MANY_REQUESTS, [("retry-after", "30")])
                }
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        let service = JobReadService::for_test(base);
        let ids = vec!["4".to_owned()];
        for _ in 0..3 {
            let error = service.listing_publication(&ids).await.unwrap_err();
            assert_eq!(error.code(), "hubspot_rate_limited");
            assert_eq!(error.status(), StatusCode::TOO_MANY_REQUESTS);
        }
        // Retry-After 30 s is longer than the request waits: one call, no retry, then cached.
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }
}
