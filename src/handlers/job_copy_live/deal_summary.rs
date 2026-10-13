//! Read-only: the application dates of every listing in one deal (案件の画面の週ごとの応募)。
//! Counts and dates only; no applicant attribute leaves the server. The relation listing -> deal
//! is checked against HubSpot for each listing, so a browser-provided id is not trusted.
use super::{authorized_user, valid_id, Access, ReadError};
use crate::AppState;
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::IntoResponse,
    Extension, Json,
};
use chrono::NaiveDate;
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

/// Counts of dated applications by day, and the number without a usable date.
fn count_by_date(dates: &[Option<&str>]) -> (BTreeMap<String, usize>, usize) {
    let mut by_date = BTreeMap::new();
    let mut missing = 0;
    for value in dates {
        match value.and_then(|v| NaiveDate::parse_from_str(v, "%Y-%m-%d").ok()) {
            Some(date) => {
                *by_date
                    .entry(date.format("%Y-%m-%d").to_string())
                    .or_insert(0) += 1
            }
            None => missing += 1,
        }
    }
    (by_date, missing)
}

/// Applications linked to more than one of the requested listings (counted once per listing in
/// each listing's own series, so a sum of the series can exceed the number of applications).
fn shared_count(per_listing: &[BTreeSet<String>]) -> usize {
    let mut seen = BTreeMap::<&str, usize>::new();
    for set in per_listing {
        for id in set {
            *seen.entry(id.as_str()).or_insert(0) += 1;
        }
    }
    seen.values().filter(|n| **n > 1).count()
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
    let mut sets = Vec::new();
    for id in &ids {
        let read: Result<(Value, BTreeSet<String>), ReadError> = async {
            let deals = service.all_associations("0-420", id, "deals").await?;
            if !deals.contains(&query.deal) {
                return Err(ReadError(StatusCode::FORBIDDEN, "listing_not_in_deal"));
            }
            let applications = service.all_associations("0-420", id, "0-421").await?;
            let rows = service.batch("0-421", &applications, &["yingmuri"]).await?;
            let dates: Vec<Option<&str>> = rows.iter().map(|r| r.value("yingmuri")).collect();
            let (by_date, missing) = count_by_date(&dates);
            let set: BTreeSet<String> = applications.into_iter().collect();
            Ok((
                json!({"listing_id":id,"total":rows.len(),"missing_date":missing,"by_date":by_date}),
                set,
            ))
        }
        .await;
        match read {
            Ok((value, set)) => {
                listings.push(value);
                sets.push(set);
            }
            // The deal relation is a permission check: fail the whole request.
            Err(error) if error.0 == StatusCode::FORBIDDEN => return Err(error),
            Err(error) => listings.push(json!({"listing_id":id,"error":error.1})),
        }
    }
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "deal_id": query.deal,
            "fetched_at": chrono::Utc::now().to_rfc3339(),
            "listings": listings,
            "shared_applications": shared_count(&sets),
        })),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_dated_and_undated_applications() {
        let (by_date, missing) = count_by_date(&[
            Some("2026-10-01"),
            Some("2026-10-01"),
            Some("2026-10-03"),
            None,
            Some("bad"),
        ]);
        assert_eq!(by_date.get("2026-10-01"), Some(&2));
        assert_eq!(by_date.get("2026-10-03"), Some(&1));
        assert_eq!(missing, 2);
    }

    #[test]
    fn shared_applications_are_those_in_two_or_more_listings() {
        let set = |v: &[&str]| {
            v.iter()
                .map(|s| s.to_string())
                .collect::<BTreeSet<String>>()
        };
        assert_eq!(
            shared_count(&[set(&["1", "2", "3"]), set(&["3", "4"]), set(&["3", "2"])]),
            2
        );
    }
}
