//! Read-only job management. Existing HubSpot associations remain the source of truth.
use crate::{
    auth::{LOGIN_METHOD_GOOGLE_OIDC, SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY},
    geo::applicant_area,
    AppState,
};
use axum::{
    extract::{rejection::QueryRejection, Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::get,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};
use tower_sessions::Session;
mod applicant_extensions;
pub mod applicant_reasons;

#[derive(Debug)]
pub struct ReadError(pub(super) StatusCode, pub(super) &'static str);
impl ReadError {
    pub(crate) fn status(&self) -> StatusCode {
        self.0
    }
    pub(crate) fn code(&self) -> &'static str {
        self.1
    }
}
impl IntoResponse for ReadError {
    fn into_response(self) -> Response {
        (
            self.0,
            [(header::CACHE_CONTROL, "no-store")],
            Json(json!({"code":self.1})),
        )
            .into_response()
    }
}
fn fail(code: &'static str) -> ReadError {
    ReadError(StatusCode::BAD_GATEWAY, code)
}
fn valid_id(id: &str) -> Result<(), ReadError> {
    if id.is_empty() || id.len() > 30 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(ReadError(StatusCode::BAD_REQUEST, "invalid_record_id"));
    }
    Ok(())
}

const OPTION_LABELS_TTL: Duration = Duration::from_secs(6 * 60 * 60);
const OPTION_LABELS_RETRY: Duration = Duration::from_secs(10 * 60);
/// How long the application response waits for a running definition read after every other read
/// is done. A read slower than this fills the labels for later requests only.
const LABEL_GRACE: Duration = Duration::from_millis(300);

/// Reads `{results:[{name, options:[{value,label}]}]}` (HubSpot property batch read). Only the
/// category selects are kept; None when the reply holds none of them.
fn option_labels(data: &Value) -> Option<applicant_reasons::OptionLabels> {
    let mut labels = applicant_reasons::OptionLabels::new();
    for property in data["results"].as_array()? {
        let Some(name) = property["name"]
            .as_str()
            .filter(|name| applicant_reasons::CATEGORY_PROPERTIES.contains(name))
        else {
            continue;
        };
        let options: BTreeMap<String, String> = property["options"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|option| {
                let value = option["value"].as_str()?.trim();
                let label = option["label"].as_str()?.trim();
                (!value.is_empty() && !label.is_empty())
                    .then(|| (value.to_owned(), label.to_owned()))
            })
            .collect();
        labels.insert(name.to_owned(), options);
    }
    (!labels.is_empty()).then_some(labels)
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub id: String,
    pub properties: BTreeMap<String, Option<String>>,
}
impl Record {
    fn value(&self, key: &str) -> Option<&str> {
        self.properties
            .get(key)
            .and_then(|v| v.as_deref())
            .filter(|v| !v.trim().is_empty())
    }
}
#[derive(Clone, Debug, Serialize)]
pub struct Job {
    pub record: Record,
    pub deal_ids: Vec<String>,
}
#[derive(Deserialize)]
struct Page {
    results: Vec<Record>,
    #[serde(default)]
    paging: Option<Paging>,
}
#[derive(Deserialize)]
struct Paging {
    next: Option<Next>,
}
#[derive(Deserialize)]
struct Next {
    after: String,
}

/// The option labels of the reason category selects, kept per service (one HubSpot base).
#[derive(Default)]
struct LabelCache {
    /// When the last definition read finished, and what it gave (None: it failed).
    read: Option<(Instant, Option<applicant_reasons::OptionLabels>)>,
    /// A definition read is running (started at this time); no second one is started meanwhile.
    running_since: Option<Instant>,
}

enum PendingLabels {
    Ready(Option<applicant_reasons::OptionLabels>),
    Reading(tokio::task::JoinHandle<Option<applicant_reasons::OptionLabels>>),
}

#[derive(Clone)]
pub struct JobReadService {
    client: reqwest::Client,
    token: String,
    base: String,
    labels: Arc<std::sync::Mutex<LabelCache>>,
}
impl JobReadService {
    #[cfg(test)]
    pub(crate) fn for_test(base: String) -> Self {
        Self::with_base("test-token".into(), base).unwrap()
    }
    pub fn new(token: String) -> Result<Self, ReadError> {
        Self::with_base(token, "https://api.hubapi.com".into())
    }
    fn with_base(token: String, base: String) -> Result<Self, ReadError> {
        if token.trim().is_empty() {
            return Err(fail("hubspot_not_configured"));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| fail("client_unavailable"))?;
        Ok(Self {
            client,
            token,
            base,
            labels: Arc::default(),
        })
    }
    async fn request(
        &self,
        path: &str,
        query: &[(&str, String)],
        body: Option<Value>,
    ) -> Result<Value, ReadError> {
        // POST is used only for HubSpot's batch/read endpoints. No create/update/delete.
        for attempt in 0..2 {
            let builder = match &body {
                Some(body) => self.client.post(format!("{}{path}", self.base)).json(body),
                None => self.client.get(format!("{}{path}", self.base)),
            };
            let response = builder
                .query(query)
                .bearer_auth(&self.token)
                .send()
                .await
                .map_err(|_| fail("hubspot_connection_failed"))?;
            let status = response.status();
            if attempt == 0 && (status.as_u16() == 429 || status.is_server_error()) {
                let wait = response
                    .headers()
                    .get("retry-after")
                    .and_then(|v| v.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1);
                if wait <= 2 {
                    tokio::time::sleep(Duration::from_secs(wait)).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(match status.as_u16() {
                    403 => fail("hubspot_scope_denied"),
                    401 => fail("hubspot_auth_failed"),
                    429 => ReadError(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
                    _ => fail("hubspot_read_failed"),
                });
            }
            return response
                .json()
                .await
                .map_err(|_| fail("hubspot_invalid_response"));
        }
        Err(fail("hubspot_read_failed"))
    }
    async fn associations(
        &self,
        from: &str,
        id: &str,
        to: &str,
        after: Option<&str>,
    ) -> Result<(Vec<String>, Option<String>), ReadError> {
        valid_id(id)?;
        let mut query = vec![("limit", "100".into())];
        if let Some(after) = after {
            valid_id(after)?;
            query.push(("after", after.into()));
        }
        let data = self
            .request(
                &format!("/crm/v4/objects/{from}/{id}/associations/{to}"),
                &query,
                None,
            )
            .await?;
        let rows = data["results"]
            .as_array()
            .ok_or_else(|| fail("hubspot_invalid_response"))?;
        let mut ids = BTreeSet::new();
        for row in rows {
            let id = row["toObjectId"]
                .as_str()
                .map(str::to_owned)
                .or_else(|| row["toObjectId"].as_u64().map(|id| id.to_string()))
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            valid_id(&id).map_err(|_| fail("hubspot_invalid_response"))?;
            ids.insert(id);
        }
        let next = match data.pointer("/paging/next") {
            None => None,
            Some(next) => Some(
                next.get("after")
                    .and_then(|v| {
                        v.as_str()
                            .map(str::to_owned)
                            .or_else(|| v.as_u64().map(|n| n.to_string()))
                    })
                    .ok_or_else(|| fail("hubspot_invalid_response"))?,
            ),
        };
        Ok((ids.into_iter().collect(), next))
    }
    async fn all_associations(
        &self,
        from: &str,
        id: &str,
        to: &str,
    ) -> Result<Vec<String>, ReadError> {
        let mut ids = BTreeSet::new();
        let mut after = None;
        let mut seen = BTreeSet::new();
        for _ in 0..20 {
            let (page, next) = self.associations(from, id, to, after.as_deref()).await?;
            ids.extend(page);
            match next {
                None => return Ok(ids.into_iter().collect()),
                Some(next) if seen.insert(next.clone()) => after = Some(next),
                _ => return Err(fail("association_paging_invalid")),
            }
        }
        Err(ReadError(
            StatusCode::UNPROCESSABLE_ENTITY,
            "association_scope_too_large",
        ))
    }
    async fn batch(
        &self,
        object: &str,
        ids: &[String],
        properties: &[&str],
    ) -> Result<Vec<Record>, ReadError> {
        if ids.is_empty() {
            return Ok(vec![]);
        }
        let mut records = Vec::new();
        for chunk in ids.chunks(100) {
            let data=self.request(&format!("/crm/v3/objects/{object}/batch/read"), &[], Some(json!({"properties":properties,"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}))).await?;
            if data
                .get("errors")
                .and_then(Value::as_array)
                .is_some_and(|v| !v.is_empty())
            {
                return Err(fail("hubspot_partial_read"));
            }
            let rows: Vec<Record> = serde_json::from_value(data["results"].clone())
                .map_err(|_| fail("hubspot_invalid_response"))?;
            let expected: BTreeSet<_> = chunk.iter().cloned().collect();
            let actual: BTreeSet<_> = rows.iter().map(|r| r.id.clone()).collect();
            if expected != actual || rows.len() != expected.len() {
                return Err(fail("hubspot_partial_read"));
            }
            records.extend(rows);
        }
        Ok(records)
    }
    async fn association_map(
        &self,
        from: &str,
        ids: &[String],
        to: &str,
    ) -> Result<BTreeMap<String, Vec<String>>, ReadError> {
        let mut mapping = BTreeMap::new();
        for chunk in ids.chunks(100) {
            let data=self.request(&format!("/crm/v4/associations/{from}/{to}/batch/read"),&[],Some(json!({"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}))).await?;
            let results = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            let expected: BTreeSet<_> = chunk.iter().cloned().collect();
            let mut seen = BTreeSet::new();
            let mut fallback = BTreeSet::new();
            for error in data
                .get("errors")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
            {
                if error["category"] != "OBJECT_NOT_FOUND" {
                    return Err(fail("hubspot_partial_read"));
                }
                let sources = error
                    .pointer("/context/fromObjectId")
                    .and_then(Value::as_array)
                    .filter(|ids| ids.len() == 1)
                    .ok_or_else(|| fail("hubspot_partial_read"))?;
                let id = sources[0]
                    .as_str()
                    .ok_or_else(|| fail("hubspot_partial_read"))?;
                if !expected.contains(id) || !fallback.insert(id.to_owned()) {
                    return Err(fail("hubspot_partial_read"));
                }
            }
            for result in results {
                let id = result
                    .pointer("/from/id")
                    .and_then(Value::as_str)
                    .ok_or_else(|| fail("hubspot_invalid_response"))?;
                if !expected.contains(id) || fallback.contains(id) || !seen.insert(id.to_owned()) {
                    return Err(fail("hubspot_partial_read"));
                }
                if result.get("paging").is_some() {
                    // Batch output may contain a partial association list. Re-read
                    // that source record through the complete paged GET contract.
                    mapping.insert(id.to_owned(), self.all_associations(from, id, to).await?);
                    continue;
                }
                let mut targets = BTreeSet::new();
                for target in result["to"]
                    .as_array()
                    .ok_or_else(|| fail("hubspot_invalid_response"))?
                {
                    let target = target["toObjectId"]
                        .as_str()
                        .map(str::to_owned)
                        .or_else(|| target["toObjectId"].as_u64().map(|n| n.to_string()))
                        .ok_or_else(|| fail("hubspot_invalid_response"))?;
                    valid_id(&target).map_err(|_| fail("hubspot_invalid_response"))?;
                    targets.insert(target);
                }
                mapping.insert(id.to_owned(), targets.into_iter().collect());
            }
            if expected != seen.union(&fallback).cloned().collect() {
                return Err(fail("hubspot_partial_read"));
            }
            // Live HubSpot returns 207 OBJECT_NOT_FOUND for some empty lists.
            // Verify each affected source via GET; never equate that error to zero.
            // Bound parallelism to three GETs, retaining all historical contracts.
            let fallback: Vec<_> = fallback.into_iter().collect();
            for sources in fallback.chunks(3) {
                let fetch = |index: usize| async move {
                    match sources.get(index) {
                        Some(id) => Ok(Some((
                            id.clone(),
                            self.all_associations(from, id, to).await?,
                        ))),
                        None => Ok::<_, ReadError>(None),
                    }
                };
                let (first, second, third) = tokio::join!(fetch(0), fetch(1), fetch(2));
                for result in [first, second, third] {
                    if let Some((id, targets)) = result? {
                        mapping.insert(id, targets);
                    }
                }
            }
        }
        Ok(mapping)
    }
    pub async fn customers(&self, after: Option<&str>) -> Result<Value, ReadError> {
        let mut query = vec![("limit", "50".into()), ("properties", "name".into())];
        if let Some(after) = after {
            valid_id(after)?;
            query.push(("after", after.into()));
        }
        let page: Page = serde_json::from_value(
            self.request("/crm/v3/objects/companies", &query, None)
                .await?,
        )
        .map_err(|_| fail("hubspot_invalid_response"))?;
        Ok(
            json!({"customers":page.results,"next_after":page.paging.and_then(|p|p.next).map(|p|p.after)}),
        )
    }
    pub async fn jobs(&self, company: &str, offset: usize) -> Result<Value, ReadError> {
        valid_id(company)?;
        let deals = self.all_associations("companies", company, "deals").await?;
        if deals.len() > 100 {
            return Err(ReadError(
                StatusCode::UNPROCESSABLE_ENTITY,
                "customer_contract_scope_too_large",
            ));
        }
        let contracts = self
            .batch(
                "deals",
                &deals,
                &["dealname", "code_of_customer", "pipeline", "dealstage"],
            )
            .await?;
        let mut mapping: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (deal, listings) in self.association_map("deals", &deals, "0-420").await? {
            for listing in listings {
                mapping.entry(listing).or_default().push(deal.clone());
            }
        }
        let ids: Vec<_> = mapping.keys().skip(offset).take(20).cloned().collect();
        let rows = self
            .batch(
                "0-420",
                &ids,
                &[
                    "hs_name",
                    "id_hrhakkaa",
                    "id_airwork",
                    "id_shop_hrhakkaa",
                    "airwork_account_login_id",
                    "shigotonaiyou",
                    "qinwude",
                    "zhizhong",
                    "kyuujin_status",
                    "hs_pipeline",
                    "hs_pipeline_stage",
                    "url_airwork",
                ],
            )
            .await?;
        let jobs: Vec<_> = rows
            .into_iter()
            .map(|record| Job {
                deal_ids: mapping.get(&record.id).cloned().unwrap_or_default(),
                record,
            })
            .collect();
        let portal = std::env::var("HUBSPOT_PORTAL_ID")
            .ok()
            .filter(|id| valid_id(id).is_ok());
        Ok(
            json!({"company_id":company,"portal_id":portal,"contracts":contracts,"jobs":jobs,"total":mapping.len(),"next_offset":if offset+ids.len()<mapping.len(){Some(offset+ids.len())}else{None}}),
        )
    }
    pub async fn validate_customer_listing(
        &self,
        company: &str,
        listing: &str,
    ) -> Result<(), ReadError> {
        valid_id(company)?;
        valid_id(listing)?;
        // Revalidate the Company -> Deal -> Listing path; a browser-provided ID is insufficient.
        let allowed_deals: BTreeSet<_> = self
            .all_associations("companies", company, "deals")
            .await?
            .into_iter()
            .collect();
        let listing_deals = self.all_associations("0-420", listing, "deals").await?;
        if !listing_deals.iter().any(|id| allowed_deals.contains(id)) {
            return Err(ReadError(
                StatusCode::FORBIDDEN,
                "job_not_related_to_customer",
            ));
        }
        Ok(())
    }
    /// (applications linked to this listing only, applications linked to more than one listing).
    /// An application missing from the reply is in neither set.
    async fn listing_links(
        &self,
        ids: &[String],
        listing: &str,
    ) -> Result<(BTreeSet<String>, BTreeSet<String>), ReadError> {
        let mut unambiguous = BTreeSet::new();
        let mut multi = BTreeSet::new();
        for chunk in ids.chunks(100) {
            let data = self
                .request(
                    "/crm/v4/associations/0-421/0-420/batch/read",
                    &[],
                    Some(json!({"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()})),
                )
                .await?;
            let results = data["results"]
                .as_array()
                .ok_or_else(|| fail("hubspot_invalid_response"))?;
            for result in results {
                let targets = result["to"]
                    .as_array()
                    .ok_or_else(|| fail("hubspot_invalid_response"))?;
                let target_ids: BTreeSet<_> = targets
                    .iter()
                    .filter_map(|t| {
                        t["toObjectId"]
                            .as_str()
                            .map(str::to_owned)
                            .or_else(|| t["toObjectId"].as_u64().map(|n| n.to_string()))
                    })
                    .collect();
                let Some(id) = result.pointer("/from/id").and_then(Value::as_str) else {
                    continue;
                };
                if target_ids.len() == 1
                    && target_ids.contains(listing)
                    && result.get("paging").is_none()
                {
                    unambiguous.insert(id.to_owned());
                } else if target_ids.len() > 1 || result.get("paging").is_some() {
                    multi.insert(id.to_owned());
                }
            }
        }
        Ok((unambiguous, multi))
    }
    /// Option labels (internal value -> label) of the reason category selects, from the
    /// property definitions. Returns the kept labels while they are fresh (OPTION_LABELS_TTL, or
    /// OPTION_LABELS_RETRY after a failed read). Otherwise it starts one definition read in the
    /// background (never two at once) and returns its handle; the application read does not wait
    /// for it beyond LABEL_GRACE (see applicants()). A failed read is not fatal: the screen then
    /// cannot name the chosen category and says so.
    fn reason_option_labels(&self) -> PendingLabels {
        let Ok(mut cache) = self.labels.lock() else {
            return PendingLabels::Ready(None);
        };
        if let Some((at, labels)) = &cache.read {
            let ttl = if labels.is_some() {
                OPTION_LABELS_TTL
            } else {
                OPTION_LABELS_RETRY
            };
            if at.elapsed() < ttl {
                return PendingLabels::Ready(labels.clone());
            }
        }
        if cache
            .running_since
            .is_some_and(|since| since.elapsed() < Duration::from_secs(60))
        {
            // Another request started the read; use the last labels kept, if any.
            return PendingLabels::Ready(
                cache.read.as_ref().and_then(|(_, labels)| labels.clone()),
            );
        }
        cache.running_since = Some(Instant::now());
        drop(cache);
        let service = self.clone();
        PendingLabels::Reading(tokio::spawn(async move {
            let read = service
                .request(
                    "/crm/v3/properties/0-421/batch/read",
                    &[],
                    Some(json!({"archived":false,"inputs":applicant_reasons::CATEGORY_PROPERTIES.iter().map(|name|json!({"name":name})).collect::<Vec<_>>()})),
                )
                .await
                .ok()
                .and_then(|data| option_labels(&data));
            if let Ok(mut cache) = service.labels.lock() {
                cache.read = Some((Instant::now(), read.clone()));
                cache.running_since = None;
            }
            read
        }))
    }
    pub async fn applicants(&self, company: &str, listing: &str) -> Result<Value, ReadError> {
        self.validate_customer_listing(company, listing).await?;
        let ids = self.all_associations("0-420", listing, "0-421").await?;
        // Every reason source is asked for in the same batch read as the other application
        // fields (no extra read per source).
        let mut properties = vec![
            "yingmuri",
            "seibetsu",
            "nenrei",
            "todoufuken",
            "shikuchouson",
        ];
        properties.extend(applicant_reasons::PROPERTIES);
        // The option labels of the two category selects come from a separate definition read,
        // kept for OPTION_LABELS_TTL. When none are kept, that read runs in the background while
        // the applications are read, and the response waits for it at most LABEL_GRACE after
        // everything else is done; a slower read only fills the labels for later requests.
        let pending_labels = self.reason_option_labels();
        let rows = self.batch("0-421", &ids, &properties).await?;
        // Missing/undefined properties remain unknown; do not turn an existing
        // appointment into zero applications. hs_appointment_start is synthesized
        // by the importer from yingmuri and does not prove the real event time.
        // Which applications HubSpot also links to another job. They are counted apart and never
        // put into a version's period (one application must not count for two jobs).
        // A failed association read does not fail the whole response: the totals and the dates
        // are still sent, the multi-job counts are left out (unknown, not 0), and no application
        // is put into a version's period (which needs to know it belongs to this job only).
        let links = self.listing_links(&ids, listing).await;
        let labels = match pending_labels {
            PendingLabels::Ready(labels) => labels,
            PendingLabels::Reading(mut handle) => tokio::time::timeout(LABEL_GRACE, &mut handle)
                .await
                .ok()
                .and_then(Result::ok)
                .flatten(),
        };
        let reasons = applicant_reasons::extract_with_labels(
            listing,
            &rows,
            chrono::Utc::now().to_rfc3339(),
            labels.as_ref(),
        );
        let mut response = json!({"listing_id":listing,"metric":"HubSpot応募レコード数","summary":null,"version_attribution":"日次観測との対応は別途必要。現在の関連による集計。","attribute_basis":"現在取得できる属性","billing":null,"capture_bundle":null,"dated_comparison":null,"capture_status":"not_configured_or_not_matched"});
        response["applicant_reasons"] =
            serde_json::to_value(reasons).map_err(|_| fail("reason_serialization_failed"))?;
        let record = self
            .batch(
                "0-420",
                &[listing.to_owned()],
                &["id_hrhakkaa", "id_shop_hrhakkaa"],
            )
            .await?;
        // Application ID -> version, when the response also carries per-version attributes.
        let mut groups = BTreeMap::new();
        match super::job_copy_capture::capture_for_hr_job(
            record.first().and_then(|r| r.value("id_hrhakkaa")),
            listing,
            record.first().and_then(|r| r.value("id_shop_hrhakkaa")),
        )
        .await
        {
            Ok(Some(_)) if links.is_err() => {
                response["capture_status"] = json!("listing_links_unavailable");
            }
            Ok(Some(bundle)) => {
                let unambiguous = links.as_ref().map(|(unambiguous, _)| unambiguous).ok();
                let applications: Vec<_> = rows
                    .iter()
                    .map(|row| crate::job_copy_date::ApplicantRecord {
                        application_id: row.id.clone(),
                        application_date: row
                            .value("yingmuri")
                            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
                            .map(|date| crate::job_copy_date::ApplicationDate::DateOnly { date })
                            .unwrap_or(crate::job_copy_date::ApplicationDate::Unknown),
                        listing_unambiguous: unambiguous
                            .is_some_and(|unambiguous| unambiguous.contains(&row.id)),
                        attributes: {
                            // Only 都道府県 + 市区町村 from the master leave the server; the raw
                            // address (番地・建物名) is dropped here.
                            let area = rounded_area(row);
                            crate::job_copy_date::ApplicantAttributes {
                                gender: row.value("seibetsu").map(str::to_owned),
                                age: row
                                    .value("nenrei")
                                    .and_then(|v| v.parse::<u8>().ok())
                                    .filter(|age| *age <= 120),
                                prefecture: area.prefecture,
                                municipality: area.municipality,
                            }
                        },
                    })
                    .collect();
                match super::job_copy_capture::dated_comparison_with_groups(&bundle, &applications)
                {
                    Ok((comparison, version_of)) => {
                        groups = version_of;
                        response["capture_bundle"] = bundle;
                        response["dated_comparison"] = comparison;
                        response["capture_status"] = json!("matched_private_evaluation_capture");
                        response["version_attribution"] = json!("変更検知日を基準に日次代表版へ対応。日付欠損・欠測日・複数求人関連は版対応不明として集計。");
                    }
                    Err(code) => response["capture_status"] = json!(code),
                }
            }
            Ok(None) => {}
            Err(code) => response["capture_status"] = json!(code),
        }
        let mut summary = summarize_grouped(&rows, &groups);
        match &links {
            Ok((_, multi)) => add_multi_listing(&mut summary, &rows, multi),
            Err(ReadError(_, code)) => response["listing_links_status"] = json!(code),
        }
        response["summary"] = summary;
        // No small group of applicants (fewer than 3 in an area or a gender × age × area cell)
        // leaves the server.
        protect_applicant_areas(&mut response);
        Ok(response)
    }
}

/// Protects a stored snapshot (/api/job-copy/moc) before it is sent, the same way as the live
/// read: every applicant area is rounded and small groups are hidden (protect_applicant_areas),
/// and every applicant reason text is masked (applicant_reasons::mask_personal_details). The
/// snapshot is written by a batch that may have kept the raw HubSpot strings (番地・建物名・電話番号・
/// 名前); those must not reach the browser, DevTools or HAR files.
fn round_snapshot_areas(data: &mut Value) {
    let Some(results) = data["results"].as_array_mut() else {
        return;
    };
    for result in results {
        protect_applicant_areas(result);
        if let Some(selections) = result["applicant_reasons"]["selections"].as_array_mut() {
            for selection in selections {
                for key in ["value", "label"] {
                    if let Some(text) = selection[key].as_str() {
                        let masked: String = applicant_reasons::mask_personal_details(text)
                            .chars()
                            .take(applicant_reasons::MAX_VALUE_CHARS)
                            .collect();
                        selection[key] = json!(masked);
                    }
                }
            }
        }
        if let Some(items) = result["applicant_reasons"]["items"].as_array_mut() {
            for item in items {
                if let Some(text) = item["text"].as_str() {
                    // Cut after masking, as extract() does (a mask can make the text longer).
                    let masked: String = applicant_reasons::mask_personal_details(text)
                        .chars()
                        .take(applicant_reasons::MAX_TEXT_CHARS)
                        .collect();
                    item["text"] = json!(masked);
                }
            }
        }
    }
}

/// Marks a `dated_comparison` whose per-version areas were hidden per version × gender × age
/// (job_copy_capture::dated_comparison). A stored snapshot without it was written before that
/// rule, so its per-version attributes are not sent.
pub const VERSION_AREA_RULE: &str = "version_gender_age_area_min3";

/// Applied to every response that carries applicant counts (the stored snapshot and the live
/// HubSpot read) before it leaves the server, so DevTools, HAR files and proxy logs never hold
/// a raw address or a small group of applicants:
/// - area labels are rounded to 都道府県 + 市区町村;
/// - gender × age × area cells with fewer than 3 applicants lose the area
///   (applicant_area::protect_applicant_keys), and the area totals are counted again from the
///   protected cells. Totals counted from the raw areas would let a reader subtract the named
///   cells and find the hidden applicant's area;
/// - without the cells, no area is named (the gender and age totals alone could then be read
///   as one area's applicants);
/// - per-version areas are sent only when they were hidden per version (VERSION_AREA_RULE);
///   otherwise the per-version attributes are left out.
///
/// The overall totals and the per-version counts are kept.
fn protect_applicant_areas(result: &mut Value) {
    let summary = &mut result["summary"];
    let cells = summary["joint_demographics"]["cells"]
        .as_array()
        .map(|cells| {
            let label = |cell: &Value, key: &str| cell[key].as_str().unwrap_or("").to_owned();
            applicant_area::protect_joint_cells(
                cells
                    .iter()
                    .map(|cell| applicant_area::JointCell {
                        gender: label(cell, "gender"),
                        age: label(cell, "age"),
                        prefecture: label(cell, "prefecture"),
                        municipality: label(cell, "municipality"),
                        count: cell["count"].as_u64().unwrap_or(0),
                    })
                    .collect(),
            )
        });
    for (key, municipality) in [("prefecture", false), ("municipality", true)] {
        let Some(counts) = summary["dimensions"][key].as_object() else {
            continue;
        };
        let total: u64 = counts
            .values()
            .map(|count| count.as_u64().unwrap_or(0))
            .sum();
        let totals: Vec<(String, u64)> = match &cells {
            Some(cells) if cells.iter().map(|cell| cell.count).sum::<u64>() == total => {
                applicant_area::area_totals(cells, municipality)
            }
            _ => {
                // No cells to check against: name no area.
                let mut hidden: Vec<(String, u64)> = Vec::new();
                for (label, count) in counts {
                    let label = applicant_area::round_area_label(municipality, label, None);
                    let label = if applicant_area::is_named_area(&label) {
                        applicant_area::AREA_OTHER.to_owned()
                    } else {
                        label
                    };
                    let count = count.as_u64().unwrap_or(0);
                    match hidden.iter_mut().find(|(existing, _)| *existing == label) {
                        Some((_, sum)) => *sum += count,
                        None => hidden.push((label, count)),
                    }
                }
                hidden
            }
        };
        let merged: BTreeMap<String, u64> = applicant_area::merge_small_areas(totals)
            .into_iter()
            .collect();
        summary["dimensions"][key] = json!(merged);
    }
    if let Some(cells) = cells {
        summary["joint_demographics"]["cells"] = json!(cells
            .into_iter()
            .map(|cell| json!({"gender":cell.gender,"age":cell.age,"prefecture":cell.prefecture,"municipality":cell.municipality,"count":cell.count}))
            .collect::<Vec<_>>());
    }
    let protected = result["dated_comparison"]["area_rule"] == VERSION_AREA_RULE;
    if let Some(versions) = result
        .get_mut("dated_comparison")
        .and_then(|comparison| comparison.get_mut("by_version"))
        .and_then(Value::as_object_mut)
    {
        for version in versions.values_mut() {
            if !protected {
                if let Some(dimensions) = version["dimensions"].as_object_mut() {
                    for value in dimensions.values_mut() {
                        *value = Value::Null;
                    }
                }
                continue;
            }
            for (key, municipality) in [("prefecture", false), ("municipality", true)] {
                round_distribution(&mut version["dimensions"][key], municipality);
            }
        }
    }
}

/// {denominator, categories: [{category, count, percentage}]} with the area categories rounded,
/// merged, and areas with fewer than 3 applicants put into 「その他」. The denominator stays the same.
fn round_distribution(distribution: &mut Value, municipality: bool) {
    let Some(categories) = distribution["categories"].as_array() else {
        return;
    };
    let denominator = distribution["denominator"].as_u64().unwrap_or(0);
    let with_percentage = categories.iter().any(|row| !row["percentage"].is_null());
    let mut merged: Vec<(String, u64)> = Vec::new();
    for row in categories {
        let label = applicant_area::round_area_label(
            municipality,
            row["category"].as_str().unwrap_or(""),
            None,
        );
        let count = row["count"].as_u64().unwrap_or(0);
        match merged.iter_mut().find(|(existing, _)| *existing == label) {
            Some((_, total)) => *total += count,
            None => merged.push((label, count)),
        }
    }
    distribution["categories"] = json!(applicant_area::merge_small_areas(merged)
        .into_iter()
        .map(|(category, count)| {
            let percentage = (with_percentage && denominator > 0)
                .then(|| count as f64 / denominator as f64 * 100.0);
            json!({"category":category,"count":count,"percentage":percentage})
        })
        .collect::<Vec<_>>());
}

fn rounded_area(row: &Record) -> applicant_area::RoundedArea {
    applicant_area::round_area(row.value("todoufuken"), row.value("shikuchouson"))
}

/// The age band used for the gender × age × area protection (the finest band any response uses).
pub fn age_band(age: Option<u32>) -> String {
    age.filter(|age| *age <= 120)
        .map(|age| {
            if age < 20 {
                "20歳未満".into()
            } else if age >= 70 {
                "70歳以上".into()
            } else {
                format!("{}代", age / 10 * 10)
            }
        })
        .unwrap_or("不明".into())
}

pub fn summarize(rows: &[Record]) -> Value {
    summarize_grouped(rows, &BTreeMap::new())
}

/// `groups`: application ID → the posting period (version) it is counted in, when the response
/// also carries per-version attributes. The areas are then hidden per version as well, so the
/// job-wide cells cannot be combined with a version's gender and age to name one applicant's city.
pub fn summarize_grouped(rows: &[Record], groups: &BTreeMap<String, String>) -> Value {
    let unique: BTreeMap<_, _> = rows.iter().map(|row| (&row.id, row)).collect();
    let mut dates: BTreeMap<String, usize> = BTreeMap::new();
    let mut dimensions: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    let mut missing_date = 0;
    let mut keys = Vec::with_capacity(unique.len());
    for (id, row) in &unique {
        if let Some(date) = row
            .value("yingmuri")
            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        {
            *dates.entry(date.to_string()).or_default() += 1;
        } else {
            missing_date += 1;
        }
        // Addresses are rounded to 都道府県 + 市区町村 before they are counted, so no label in
        // the response carries a street number or a building name.
        let area = rounded_area(row);
        let gender: String = row.value("seibetsu").unwrap_or("不明").into();
        let age = age_band(row.value("nenrei").and_then(|v| v.parse::<u32>().ok()));
        *dimensions
            .entry("gender")
            .or_default()
            .entry(gender.clone())
            .or_default() += 1;
        *dimensions
            .entry("age")
            .or_default()
            .entry(age.clone())
            .or_default() += 1;
        keys.push(applicant_area::ApplicantKey {
            group: groups.get(id.as_str()).cloned().unwrap_or_default(),
            gender,
            age,
            prefecture: applicant_area::prefecture_label(&area),
            municipality: applicant_area::municipality_label(&area),
            count: 1,
        });
    }
    // The area totals are counted from the hidden areas, never from the raw ones (see
    // applicant_area::protect_applicant_keys).
    let areas = applicant_area::protect_applicant_keys(&keys);
    let mut joint: BTreeMap<(String, String, String, String), usize> = BTreeMap::new();
    for (key, (prefecture, municipality)) in keys.iter().zip(areas) {
        *dimensions
            .entry("prefecture")
            .or_default()
            .entry(prefecture.clone())
            .or_default() += 1;
        *dimensions
            .entry("municipality")
            .or_default()
            .entry(municipality.clone())
            .or_default() += 1;
        *joint
            .entry((
                key.gender.clone(),
                key.age.clone(),
                prefecture,
                municipality,
            ))
            .or_default() += 1;
    }
    let cells:Vec<_> = joint.into_iter().map(|((gender,age,prefecture,municipality),count)|json!({"gender":gender,"age":age,"prefecture":prefecture,"municipality":municipality,"count":count})).collect();
    json!({"total":unique.len(),"duplicate_ids":rows.len()-unique.len(),"by_date":dates,"missing_date":missing_date,"dimensions":dimensions,"joint_demographics":{"total":unique.len(),"cells":cells}})
}

/// Adds `multi_listing_by_date` / `multi_listing_missing_date`: the applications (by
/// application date, as in `by_date`) that HubSpot also links to another job. Counts only.
pub fn add_multi_listing(summary: &mut Value, rows: &[Record], multi: &BTreeSet<String>) {
    let unique: BTreeMap<_, _> = rows.iter().map(|row| (&row.id, row)).collect();
    let mut dates: BTreeMap<String, usize> = BTreeMap::new();
    let mut missing = 0;
    for (id, row) in unique {
        if !multi.contains(id) {
            continue;
        }
        match row
            .value("yingmuri")
            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        {
            Some(date) => *dates.entry(date.to_string()).or_default() += 1,
            None => missing += 1,
        }
    }
    summary["multi_listing_by_date"] = json!(dates);
    summary["multi_listing_missing_date"] = json!(missing);
}

struct CachedSnapshot {
    file_id: String,
    sha256: String,
    data: Arc<Value>,
}
#[derive(Clone)]
pub(super) struct Access {
    allowed: BTreeSet<String>,
    service: Option<Arc<JobReadService>>,
    moc_path: Option<PathBuf>,
    moc_drive: Result<Option<SnapshotPointer>, &'static str>,
    snapshot_reader: Option<Arc<super::job_copy_drive::DriveReader>>,
    snapshot_cache: Arc<tokio::sync::Mutex<Option<CachedSnapshot>>>,
    resolved_jobs: Arc<tokio::sync::Mutex<BTreeMap<String, (Instant, Value)>>>,
    images: Option<Arc<super::job_copy_image_bridge::ImageBridge>>,
    drive_listings: BTreeSet<String>,
    drive_config_error: Option<&'static str>,
}
fn parse_drive_listings(raw: &str) -> Result<BTreeSet<String>, &'static str> {
    if raw.trim().is_empty() {
        return Ok(BTreeSet::new());
    }
    raw.split(',')
        .map(|id| {
            let id = id.trim();
            valid_id(id).map_err(|_| "drive_listing_configuration_invalid")?;
            Ok(id.to_owned())
        })
        .collect()
}
fn require_cloud_image_coverage(
    data: &Value,
    listings: &BTreeSet<String>,
) -> Result<(), ReadError> {
    let jobs = data
        .pointer("/capture_bundle/jobs")
        .and_then(Value::as_array)
        .ok_or_else(moc_invalid)?;
    for job in jobs {
        if job["images"]
            .as_array()
            .is_some_and(|images| !images.is_empty())
            && !job["hubspotListingId"]
                .as_str()
                .is_some_and(|id| listings.contains(id))
        {
            return Err(ReadError(
                StatusCode::SERVICE_UNAVAILABLE,
                "drive_images_not_configured",
            ));
        }
    }
    Ok(())
}
#[derive(Clone)]
struct SnapshotPointer {
    file_id: String,
    sha256: String,
}
fn snapshot_pointer(
    file: Option<&str>,
    hash: Option<&str>,
) -> Result<Option<SnapshotPointer>, &'static str> {
    let file = file.filter(|s| !s.is_empty());
    let hash = hash.filter(|s| !s.is_empty());
    match (file, hash) {
        (None, None) => Ok(None),
        (Some(file), Some(hash))
            if (10..=200).contains(&file.len())
                && file
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-'))
                && hash.len() == 64
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)) =>
        {
            Ok(Some(SnapshotPointer {
                file_id: file.into(),
                sha256: hash.into(),
            }))
        }
        _ => Err("moc_drive_configuration_invalid"),
    }
}
#[derive(Deserialize, Default)]
struct ReadQuery {
    company: Option<String>,
    listing: Option<String>,
    after: Option<String>,
    #[serde(default)]
    offset: usize,
}
fn authorize<'a>(
    email: Option<&'a str>,
    method: Option<&str>,
    allowed: &BTreeSet<String>,
) -> Result<&'a str, ReadError> {
    let email = email
        .filter(|email| !email.is_empty())
        .ok_or(ReadError(StatusCode::UNAUTHORIZED, "login_required"))?;
    if method != Some(LOGIN_METHOD_GOOGLE_OIDC) || !allowed.contains(&email.to_lowercase()) {
        return Err(ReadError(StatusCode::FORBIDDEN, "job_copy_access_denied"));
    }
    Ok(email)
}
pub(super) async fn authorized_user(
    state: &AppState,
    access: &Access,
    session: &Session,
) -> Result<String, ReadError> {
    let email: Option<String> = session
        .get(SESSION_USER_KEY)
        .await
        .map_err(|_| fail("session_unavailable"))?;
    let method: Option<String> = session
        .get(SESSION_LOGIN_METHOD_KEY)
        .await
        .map_err(|_| fail("session_unavailable"))?;
    let email = authorize(email.as_deref(), method.as_deref(), &access.allowed)?;
    if crate::account_is_disabled(state, email).await {
        return Err(ReadError(StatusCode::FORBIDDEN, "account_disabled"));
    }
    Ok(email.to_owned())
}
async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ReadQuery>,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let service = access.service.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let started = Instant::now();
    let mut data = match (&query.company, &query.listing) {
        (None, None) => service.customers(query.after.as_deref()).await?,
        (Some(company), None) => service.jobs(company, query.offset).await?,
        (Some(company), Some(listing)) => service.applicants(company, listing).await?,
        _ => return Err(ReadError(StatusCode::BAD_REQUEST, "company_required")),
    };
    data["fetched_at"] = json!(chrono::Utc::now().to_rfc3339());
    data["total_ms"] = json!(started.elapsed().as_secs_f64() * 1000.0);
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
}

const MOC_MAX_BYTES: usize = 32 * 1024 * 1024;
fn moc_invalid() -> ReadError {
    ReadError(StatusCode::SERVICE_UNAVAILABLE, "moc_schema_invalid")
}
fn keys_only(value: &Value, keys: &[&str]) -> bool {
    value
        .as_object()
        .is_some_and(|object| object.keys().all(|key| keys.contains(&key.as_str())))
}
fn scalars_except(value: &Value, compound: &[&str]) -> bool {
    value.as_object().is_some_and(|map| {
        map.iter().all(|(key, value)| {
            compound.contains(&key.as_str()) || (!value.is_object() && !value.is_array())
        })
    })
}
fn valid_timestamp(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|v| chrono::DateTime::parse_from_rfc3339(v).is_ok())
}
fn bounded_string(value: &Value, max: usize) -> bool {
    value.as_str().is_some_and(|v| v.len() <= max)
}
fn counts(value: &Value) -> bool {
    value
        .as_object()
        .is_some_and(|map| map.len() <= 10000 && map.values().all(|count| count.as_u64().is_some()))
}
fn aggregate_dimensions(value: &Value) -> bool {
    keys_only(value, &["gender", "age", "prefecture", "municipality"])
        && value
            .as_object()
            .is_some_and(|map| map.values().all(counts))
}
fn valid_distribution(value: &Value) -> bool {
    if value.is_null() {
        return true;
    }
    keys_only(value, &["denominator", "categories"])
        && value["denominator"].as_u64().is_some()
        && value["categories"].as_array().is_some_and(|rows| {
            rows.len() <= 10000
                && rows.iter().all(|row| {
                    keys_only(row, &["category", "count", "percentage"])
                        && bounded_string(&row["category"], 500)
                        && row["count"].as_u64().is_some()
                        && (row["percentage"].is_null()
                            || row["percentage"]
                                .as_f64()
                                .is_some_and(|n| (0.0..=100.0).contains(&n)))
                })
        })
}
fn valid_image(image: &Value) -> bool {
    keys_only(
        image,
        &[
            "id",
            "url",
            "caption",
            "contentHash",
            "sourceReferenceHash",
            "sourceSlot",
        ],
    ) && bounded_string(&image["id"], 500)
        && image
            .get("caption")
            .is_none_or(|caption| bounded_string(caption, 500))
        && image
            .as_object()
            .is_some_and(|map| map.values().all(|v| !v.is_object() && !v.is_array()))
        && image["url"].as_str().is_some_and(|url| {
            url.len() <= 2 * 1024 * 1024
                && [
                    "data:image/jpeg;base64,",
                    "data:image/png;base64,",
                    "data:image/webp;base64,",
                ]
                .iter()
                .any(|prefix| {
                    url.strip_prefix(prefix).is_some_and(|bytes| {
                        !bytes.is_empty()
                            && bytes
                                .bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b"+/=".contains(&b))
                    })
                })
        })
}
fn valid_copy(entry: &Value) -> bool {
    bounded_string(&entry["id"], 500)
        && bounded_string(&entry["body"], 100000)
        && entry.as_object().is_some_and(|map| {
            map.iter().all(|(key, value)| {
                if [
                    "images",
                    "imageReferences",
                    "history",
                    "companyIds",
                    "imageAcquisition",
                ]
                .contains(&key.as_str())
                {
                    true
                } else {
                    !value.is_array() && !value.is_object()
                }
            })
        })
        && entry.get("companyIds").is_none_or(|ids| {
            ids.as_array().is_some_and(|ids| {
                ids.len() <= 100
                    && ids
                        .iter()
                        .all(|id| id.as_str().is_some_and(|id| valid_id(id).is_ok()))
            })
        })
        && entry.get("imageAcquisition").is_none_or(|acquisition| {
            keys_only(acquisition, &["expected", "downloaded", "failed"])
                && ["expected", "downloaded", "failed"]
                    .iter()
                    .all(|key| acquisition[*key].as_u64().is_some_and(|n| n <= 3))
        })
        && entry.get("images").is_none_or(|images| {
            images
                .as_array()
                .is_some_and(|images| images.len() <= 3 && images.iter().all(valid_image))
        })
        && entry.get("imageReferences").is_none_or(|references| {
            references.as_array().is_some_and(|references| {
                let mut slots = BTreeSet::new();
                references.len() <= 3
                    && references.iter().all(|reference| {
                        keys_only(reference, &["referenceHash", "slot"])
                            && reference["referenceHash"].as_str().is_some_and(|sha| {
                                sha.len() == 64 && sha.bytes().all(|b| b.is_ascii_hexdigit())
                            })
                            && reference["slot"]
                                .as_u64()
                                .is_some_and(|slot| (1..=3).contains(&slot) && slots.insert(slot))
                    })
            })
        })
}
fn validate_moc(value: &Value) -> Result<(), ReadError> {
    if !keys_only(
        value,
        &["schemaVersion", "capturedAt", "capture_bundle", "results"],
    ) || value["schemaVersion"] != 1
        || !valid_timestamp(&value["capturedAt"])
    {
        return Err(moc_invalid());
    }
    let bundle = &value["capture_bundle"];
    let jobs = bundle["jobs"].as_array().ok_or_else(moc_invalid)?;
    if !keys_only(
        bundle,
        &["schemaVersion", "capturedAt", "sourceGeneratedDate", "jobs"],
    ) || !scalars_except(bundle, &["jobs"])
        || bundle["schemaVersion"] != 1
        || !valid_timestamp(&bundle["capturedAt"])
        || jobs.is_empty()
        || jobs.len() > 59
    {
        return Err(moc_invalid());
    }
    let mut listing_ids = BTreeSet::new();
    let mut copy_ids = BTreeSet::new();
    for job in jobs {
        let listing = job["hubspotListingId"].as_str().ok_or_else(moc_invalid)?;
        if !keys_only(
            job,
            &[
                "id",
                "hubspotListingId",
                "shopId",
                "companyIds",
                "title",
                "company",
                "media",
                "mediaJobId",
                "location",
                "body",
                "images",
                "observedRawStatus",
                "sourceGeneratedDate",
                "observationTimeBasis",
                "imageAcquiredAt",
                "imageAcquisitionStartedAt",
                "imageAcquisition",
                "history",
                "imageReferences",
            ],
        ) || valid_id(listing).is_err()
            || !listing_ids.insert(listing)
            || !valid_copy(job)
            || !copy_ids.insert(job["id"].as_str().ok_or_else(moc_invalid)?)
            || !bounded_string(&job["title"], 500)
        {
            return Err(moc_invalid());
        }
        if let Some(history) = job.get("history") {
            let history = history.as_array().ok_or_else(moc_invalid)?;
            if history.len() > 10
                || history.iter().any(|entry| {
                    !keys_only(
                        entry,
                        &[
                            "id",
                            "capturedAt",
                            "body",
                            "images",
                            "historicalImageBytesAvailable",
                            "observedRawStatus",
                            "sourceGeneratedDate",
                            "imageAcquisition",
                            "publicationBoundaryKnown",
                            "provenance",
                            "observationTimeBasis",
                            "imageReferences",
                            "imageAcquiredAt",
                            "imageAcquisitionStartedAt",
                        ],
                    ) || !valid_copy(entry)
                        || !valid_timestamp(&entry["capturedAt"])
                        || !copy_ids.insert(entry["id"].as_str().unwrap_or(""))
                })
            {
                return Err(moc_invalid());
            }
        }
    }
    let results = value["results"].as_array().ok_or_else(moc_invalid)?;
    let mut result_ids = BTreeSet::new();
    if results.len() != jobs.len() {
        return Err(moc_invalid());
    }
    for result in results {
        let id = result["listing_id"].as_str().ok_or_else(moc_invalid)?;
        let summary = &result["summary"];
        if !listing_ids.contains(id)
            || !result_ids.insert(id)
            || !scalars_except(
                result,
                &[
                    "summary",
                    "dated_comparison",
                    "applicant_reasons",
                    "hrh_performance",
                ],
            )
            || !keys_only(
                result,
                &[
                    "listing_id",
                    "metric",
                    "summary",
                    "version_attribution",
                    "attribute_basis",
                    "billing",
                    "capture_bundle",
                    "dated_comparison",
                    "capture_status",
                    "fetched_at",
                    "total_ms",
                    "applicant_reasons",
                    "hrh_performance",
                ],
            )
            || !keys_only(
                summary,
                &[
                    "total",
                    "duplicate_ids",
                    "by_date",
                    "missing_date",
                    "dimensions",
                    "joint_demographics",
                    "multi_listing_by_date",
                    "multi_listing_missing_date",
                ],
            )
            || ["total", "duplicate_ids", "missing_date"]
                .iter()
                .any(|key| summary[*key].as_u64().is_none())
            || !counts(&summary["by_date"])
            || summary
                .get("multi_listing_by_date")
                .is_some_and(|value| !counts(value))
            || summary
                .get("multi_listing_missing_date")
                .is_some_and(|value| value.as_u64().is_none())
            || !aggregate_dimensions(&summary["dimensions"])
            || !result["billing"].is_null()
            || !result["capture_bundle"].is_null()
        {
            return Err(moc_invalid());
        }
        if !applicant_extensions::validate(
            result,
            jobs.iter()
                .find(|job| job["hubspotListingId"].as_str() == Some(id))
                .ok_or_else(moc_invalid)?,
        ) {
            return Err(moc_invalid());
        }
        let comparison = &result["dated_comparison"];
        if !comparison.is_null()
            && (!keys_only(
                comparison,
                &[
                    "rule_version",
                    "total",
                    "unknown",
                    "by_version",
                    "daily_representatives",
                    "excluded_observations",
                    "basis",
                ],
            ) || !scalars_except(
                comparison,
                &[
                    "by_version",
                    "daily_representatives",
                    "excluded_observations",
                ],
            ) || comparison["total"].as_u64().is_none()
                || comparison["unknown"].as_u64().is_none()
                || !comparison["daily_representatives"].as_object().is_some_and(
                    |representatives| {
                        representatives.len() <= 11
                            && representatives.values().all(|representative| {
                                keys_only(
                                    representative,
                                    &[
                                        "date",
                                        "observation_id",
                                        "version_id",
                                        "source_freshness",
                                        "provisional",
                                    ],
                                ) && serde_json::from_value::<
                                    crate::job_copy_date::DailyRepresentative,
                                >(representative.clone())
                                .is_ok()
                            })
                    },
                )
                || !comparison["excluded_observations"]
                    .as_array()
                    .is_some_and(|rows| {
                        rows.len() <= 11
                            && rows.iter().all(|row| {
                                keys_only(row, &["observation_id", "reason"])
                                    && serde_json::from_value::<
                                        crate::job_copy_date::ExcludedObservation,
                                    >(row.clone())
                                    .is_ok()
                            })
                    })
                || !comparison["by_version"]
                    .as_object()
                    .is_some_and(|versions| {
                        versions.len() <= 11
                            && versions.values().all(|version| {
                                keys_only(version, &["count", "dimensions"])
                                    && version["count"].as_u64().is_some()
                                    && keys_only(
                                        &version["dimensions"],
                                        &["gender", "age", "prefecture", "municipality"],
                                    )
                                    && version["dimensions"].as_object().is_some_and(|dimensions| {
                                        dimensions.values().all(valid_distribution)
                                    })
                            })
                    }))
        {
            return Err(moc_invalid());
        }
    }
    Ok(())
}
async fn load_moc(path: Option<&Path>) -> Result<Value, ReadError> {
    use tokio::io::AsyncReadExt;
    let path = path.ok_or(ReadError(StatusCode::NOT_FOUND, "moc_not_configured"))?;
    let file = tokio::fs::File::open(path)
        .await
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "moc_file_unavailable"))?;
    let metadata = file
        .metadata()
        .await
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "moc_file_unavailable"))?;
    if !metadata.is_file() || metadata.len() > MOC_MAX_BYTES as u64 {
        return Err(ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "moc_file_too_large_or_invalid",
        ));
    }
    let mut raw = Vec::new();
    file.take(MOC_MAX_BYTES as u64 + 1)
        .read_to_end(&mut raw)
        .await
        .map_err(|_| ReadError(StatusCode::SERVICE_UNAVAILABLE, "moc_file_unavailable"))?;
    if raw.len() > MOC_MAX_BYTES {
        return Err(ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "moc_file_too_large_or_invalid",
        ));
    }
    let data: Value = serde_json::from_slice(&raw).map_err(|_| moc_invalid())?;
    validate_moc(&data)?;
    Ok(data)
}
// Access is constructed once with an immutable file ID/hash. Only verified raw
// evidence is cached; authorization and listing configuration are checked per request.
async fn raw_moc(access: &Access) -> Result<Arc<Value>, ReadError> {
    if let Some(code) = access.drive_config_error {
        return Err(ReadError(StatusCode::SERVICE_UNAVAILABLE, code));
    }
    match access
        .moc_drive
        .as_ref()
        .map_err(|code| ReadError(StatusCode::SERVICE_UNAVAILABLE, code))?
    {
        Some(pointer) => {
            let mut cached = access.snapshot_cache.lock().await;
            if let Some(snapshot) = cached.as_ref().filter(|snapshot| {
                snapshot.file_id == pointer.file_id && snapshot.sha256 == pointer.sha256
            }) {
                require_cloud_image_coverage(&snapshot.data, &access.drive_listings)?;
                return Ok(snapshot.data.clone());
            }
            let reader = access.snapshot_reader.as_ref().ok_or(ReadError(
                StatusCode::SERVICE_UNAVAILABLE,
                "drive_not_configured",
            ))?;
            let data = reader
                .read_snapshot_verified(&pointer.file_id, &pointer.sha256)
                .await
                .map_err(|_| {
                    ReadError(StatusCode::BAD_GATEWAY, "moc_drive_snapshot_unavailable")
                })?;
            validate_moc(&data)?;
            require_cloud_image_coverage(&data, &access.drive_listings)?;
            let data = Arc::new(data);
            *cached = Some(CachedSnapshot {
                file_id: pointer.file_id.clone(),
                sha256: pointer.sha256.clone(),
                data: data.clone(),
            });
            Ok(data)
        }
        None => Ok(Arc::new(load_moc(access.moc_path.as_deref()).await?)),
    }
}

fn defer_snapshot_images(data: &mut Value, listings: &BTreeSet<String>) -> Result<(), ReadError> {
    let jobs = data["capture_bundle"]["jobs"]
        .as_array_mut()
        .ok_or_else(moc_invalid)?;
    for job in jobs {
        let listing = job["hubspotListingId"]
            .as_str()
            .ok_or_else(moc_invalid)?
            .to_owned();
        if !listings.contains(&listing) {
            continue;
        }
        defer_image_urls(job.get_mut("images"), &listing, 0)?;
        if let Some(history) = job["history"].as_array_mut() {
            for (index, version) in history.iter_mut().enumerate() {
                if version["historicalImageBytesAvailable"] == true {
                    defer_image_urls(version.get_mut("images"), &listing, index + 1)?;
                }
            }
        }
    }
    Ok(())
}
fn valid_image_hash(hash: &str) -> bool {
    hash.len() == 64
        && hash
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}
fn defer_image_urls(
    images: Option<&mut Value>,
    listing: &str,
    version: usize,
) -> Result<(), ReadError> {
    if let Some(images) = images.and_then(Value::as_array_mut) {
        for (index, image) in images.iter_mut().enumerate() {
            let hash = image["contentHash"]
                .as_str()
                .filter(|hash| valid_image_hash(hash))
                .ok_or_else(moc_invalid)?;
            image["url"] = json!(format!("/api/job-copy/snapshot-image?listing_id={listing}&version={version}&slot={}&image_hash={hash}",index+1));
        }
    }
    Ok(())
}
async fn moc(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    let mut data = (*raw_moc(&access).await?).clone();
    defer_snapshot_images(&mut data, &access.drive_listings)?;
    round_snapshot_areas(&mut data);
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SnapshotImageQuery {
    listing_id: String,
    version: usize,
    slot: usize,
    image_hash: String,
}
fn snapshot_image_entry(job: &Value, version: usize, slot: usize) -> Result<&Value, ReadError> {
    if slot == 0 || slot > 3 || version > 10 {
        return Err(ReadError(
            StatusCode::BAD_REQUEST,
            "invalid_snapshot_image_request",
        ));
    }
    let source = if version == 0 {
        job
    } else {
        let history = job["history"]
            .as_array()
            .and_then(|rows| rows.get(version - 1))
            .ok_or(ReadError(StatusCode::NOT_FOUND, "snapshot_image_not_found"))?;
        if history["historicalImageBytesAvailable"] != true {
            return Err(ReadError(StatusCode::NOT_FOUND, "snapshot_image_not_found"));
        }
        history
    };
    source["images"]
        .as_array()
        .and_then(|images| images.get(slot - 1))
        .ok_or(ReadError(StatusCode::NOT_FOUND, "snapshot_image_not_found"))
}
async fn resolved_snapshot_job(
    access: &Access,
    raw_job: &Value,
    listing: &str,
) -> Result<Value, ReadError> {
    // A bounded short cache prevents simultaneous images of the same selected job
    // repeating upstream resolution. The original image endpoint still rechecks live
    // relations, manifest ancestry and image hashes for every image response.
    let mut cached = access.resolved_jobs.lock().await;
    let cloud = matches!(&access.moc_drive, Ok(Some(_)));
    let cache_key = match &access.moc_drive {
        Ok(Some(pointer)) => format!("{}:{}:{listing}", pointer.file_id, pointer.sha256),
        _ => listing.to_owned(),
    };
    if cloud {
        if let Some((_, job)) = cached
            .get(&cache_key)
            .filter(|(expires, _)| *expires > Instant::now())
        {
            return Ok(job.clone());
        }
    }
    let bridge = access.images.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "drive_not_configured",
    ))?;
    let mut single = json!({"capture_bundle":{"jobs":[raw_job.clone()]}});
    hydrate_drive_images(&mut single, bridge, &BTreeSet::from([listing.to_owned()])).await?;
    let job = single["capture_bundle"]["jobs"][0].clone();
    if cloud {
        cached.retain(|_, (expires, _)| *expires > Instant::now());
        if cached.len() >= 59 {
            cached.clear();
        }
        cached.insert(
            cache_key,
            (Instant::now() + Duration::from_secs(30), job.clone()),
        );
    }
    Ok(job)
}
async fn snapshot_image(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    query: Result<Query<SnapshotImageQuery>, QueryRejection>,
) -> Result<Response, ReadError> {
    authorized_user(&state, &access, &session).await?;
    if let Some(code) = access.drive_config_error {
        return Err(ReadError(StatusCode::SERVICE_UNAVAILABLE, code));
    }
    let Query(query) =
        query.map_err(|_| ReadError(StatusCode::BAD_REQUEST, "invalid_snapshot_image_request"))?;
    valid_id(&query.listing_id)?;
    if !valid_image_hash(&query.image_hash)
        || query.slot == 0
        || query.slot > 3
        || query.version > 10
    {
        return Err(ReadError(
            StatusCode::BAD_REQUEST,
            "invalid_snapshot_image_request",
        ));
    }
    if !access.drive_listings.contains(&query.listing_id) {
        return Err(ReadError(
            StatusCode::FORBIDDEN,
            "image_listing_not_enabled",
        ));
    }
    let data = raw_moc(&access).await?;
    let job = data["capture_bundle"]["jobs"]
        .as_array()
        .and_then(|jobs| {
            jobs.iter()
                .find(|job| job["hubspotListingId"].as_str() == Some(query.listing_id.as_str()))
        })
        .ok_or(ReadError(
            StatusCode::NOT_FOUND,
            "snapshot_listing_not_found",
        ))?;
    let original = snapshot_image_entry(job, query.version, query.slot)?;
    if original["contentHash"].as_str() != Some(query.image_hash.as_str()) {
        return Err(ReadError(StatusCode::CONFLICT, "snapshot_image_changed"));
    }
    let resolved = resolved_snapshot_job(&access, job, &query.listing_id).await?;
    let image = snapshot_image_entry(&resolved, query.version, query.slot)?;
    if image["contentHash"].as_str() != Some(query.image_hash.as_str()) {
        return Err(ReadError(StatusCode::CONFLICT, "snapshot_image_changed"));
    }
    let url = image["url"]
        .as_str()
        .filter(|url| url.starts_with("/api/job-copy/image?"))
        .ok_or(ReadError(
            StatusCode::BAD_GATEWAY,
            "moc_drive_image_mapping_incomplete",
        ))?;
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Redirect::temporary(url),
    )
        .into_response())
}

pub async fn hydrate_drive_images(
    data: &mut Value,
    bridge: &super::job_copy_image_bridge::ImageBridge,
    listings: &BTreeSet<String>,
) -> Result<(), ReadError> {
    let jobs = data["capture_bundle"]["jobs"]
        .as_array_mut()
        .ok_or_else(moc_invalid)?;
    let mut work = tokio::task::JoinSet::new();
    let mut completed = Vec::new();
    for (index, job) in jobs.iter().enumerate() {
        let listing = job["hubspotListingId"]
            .as_str()
            .ok_or_else(moc_invalid)?
            .to_owned();
        if !listings.contains(&listing) {
            continue;
        }
        if work.len() >= 4 {
            completed.push(
                work.join_next()
                    .await
                    .ok_or_else(moc_invalid)?
                    .map_err(|_| fail("image_resolution_failed"))??,
            );
        }
        let bridge = bridge.clone();
        let mut job = job.clone();
        work.spawn(async move {
            let pointer = bridge
                .current(&listing)
                .await
                .map_err(fail)?
                .ok_or_else(|| fail("image_not_linked"))?;
            let manifest = bridge.manifest(&pointer).await.map_err(fail)?;
            if manifest.listing_id != listing {
                return Err(fail("manifest_listing_mismatch"));
            }
            let company = manifest
                .company_ids
                .iter()
                .find(|id| {
                    job["companyIds"]
                        .as_array()
                        .is_some_and(|ids| ids.iter().any(|x| x.as_str() == Some(id.as_str())))
                })
                .ok_or_else(|| fail("manifest_customer_mismatch"))?
                .clone();
            bridge.authorize(&company, &listing).await.map_err(fail)?;
            let expected_images = job["images"].as_array().ok_or_else(moc_invalid)?.len();
            if replace_image_urls(job.get_mut("images"), &manifest, &company, &pointer.file_id)
                != expected_images
            {
                return Err(fail("moc_drive_image_mapping_incomplete"));
            }
            if let Some(history) = job["history"].as_array_mut() {
                for version in history {
                    // Only replace originals already supported by captured bytes; don't fill unobserved historical images.
                    if version["historicalImageBytesAvailable"] == true {
                        replace_image_urls(
                            version.get_mut("images"),
                            &manifest,
                            &company,
                            &pointer.file_id,
                        );
                    }
                }
            }
            Ok::<_, ReadError>((index, job))
        });
    }
    while let Some(result) = work.join_next().await {
        completed.push(result.map_err(|_| fail("image_resolution_failed"))??);
    }
    // Publish replacements only after every selected job has passed all checks.
    for (index, job) in completed {
        jobs[index] = job;
    }
    Ok(())
}
fn replace_image_urls(
    images: Option<&mut Value>,
    manifest: &super::job_copy_image_bridge::Manifest,
    company: &str,
    manifest_id: &str,
) -> usize {
    let mut replaced = 0;
    if let Some(images) = images.and_then(Value::as_array_mut) {
        for image in images {
            if let Some(reference) = manifest.images.iter().find(|candidate| {
                image["contentHash"].as_str() == Some(candidate.sha256.as_str())
                    && image["sourceSlot"].as_u64() == Some(candidate.slot as u64)
            }) {
                image["url"] = json!(format!("/api/job-copy/image?company_id={company}&listing_id={}&manifest_id={manifest_id}&slot={}",manifest.listing_id,reference.slot));
                replaced += 1;
            }
        }
    }
    replaced
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ImageQuery {
    company_id: String,
    listing_id: String,
    manifest_id: String,
    slot: usize,
}
async fn image(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Query(query): Query<ImageQuery>,
) -> Result<Response, ReadError> {
    authorized_user(&state, &access, &session).await?;
    if let Some(code) = access.drive_config_error {
        return Err(ReadError(StatusCode::SERVICE_UNAVAILABLE, code));
    }
    valid_id(&query.company_id)?;
    valid_id(&query.listing_id)?;
    if !access.drive_listings.contains(&query.listing_id) {
        return Err(ReadError(
            StatusCode::FORBIDDEN,
            "image_listing_not_enabled",
        ));
    }
    let bridge = access.images.as_ref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "drive_not_configured",
    ))?;
    let (mime, bytes) = bridge
        .image(
            &query.company_id,
            &query.listing_id,
            &query.manifest_id,
            query.slot,
        )
        .await
        .map_err(|code| {
            ReadError(
                if code == "customer_listing_not_authorized" || code == "manifest_customer_mismatch"
                {
                    StatusCode::FORBIDDEN
                } else {
                    StatusCode::BAD_GATEWAY
                },
                code,
            )
        })?;
    Ok((
        [
            (header::CONTENT_TYPE, mime.as_str()),
            (header::CACHE_CONTROL, "private, no-store"),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
            (header::REFERRER_POLICY, "no-referrer"),
        ],
        bytes,
    )
        .into_response())
}
pub fn router() -> Router<Arc<AppState>> {
    let allowed = std::env::var("JOB_COPY_ALLOWED_EMAILS")
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().to_lowercase())
        .filter(|s| !s.is_empty())
        .collect();
    let service = std::env::var("HUBSPOT_ACCESS_TOKEN")
        .ok()
        .and_then(|t| JobReadService::new(t).ok())
        .map(Arc::new);
    let snapshot_reader = super::job_copy_drive::DriveReader::from_env()
        .ok()
        .map(Arc::new);
    let images = service.as_ref().and_then(|jobs| {
        let reader = snapshot_reader.clone()?;
        super::job_copy_image_bridge::ImageBridge::new(
            std::env::var("HUBSPOT_ACCESS_TOKEN").ok()?,
            reader,
            jobs.clone(),
        )
        .ok()
        .map(Arc::new)
    });
    let parsed_listings =
        parse_drive_listings(&std::env::var("JOB_COPY_DRIVE_LISTINGS").unwrap_or_default());
    let drive_config_error = parsed_listings.as_ref().err().copied();
    let drive_listings = parsed_listings.unwrap_or_default();
    Router::new()
        .route("/api/job-copy/live", get(read))
        .route("/api/job-copy/moc", get(moc))
        .route("/api/job-copy/image", get(image))
        .route("/api/job-copy/snapshot-image", get(snapshot_image))
        .route("/api/job-copy/market", get(super::job_copy_market::read))
        .layer(Extension(Access {
            allowed,
            service,
            images,
            drive_listings,
            drive_config_error,
            snapshot_reader,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            moc_drive: snapshot_pointer(
                std::env::var("JOB_COPY_MOC_DRIVE_FILE_ID").ok().as_deref(),
                std::env::var("JOB_COPY_MOC_DRIVE_SHA256").ok().as_deref(),
            ),
            moc_path: std::env::var_os("JOB_COPY_MOC_PATH")
                .filter(|path| !path.is_empty())
                .map(PathBuf::from),
        }))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn drive_listing_configuration_never_silently_drops_invalid_ids() {
        assert!(parse_drive_listings("").unwrap().is_empty());
        assert_eq!(
            parse_drive_listings(" 30,31,30 ").unwrap(),
            BTreeSet::from(["30".into(), "31".into()])
        );
        for raw in ["oops", "30,oops", "30,", ",30", "30,,31", "30/31"] {
            assert_eq!(
                parse_drive_listings(raw).unwrap_err(),
                "drive_listing_configuration_invalid"
            );
        }
    }
    #[test]
    fn cloud_snapshot_requires_every_current_image_listing_to_be_enabled() {
        let data = moc_fixture();
        assert_eq!(
            require_cloud_image_coverage(&data, &BTreeSet::new())
                .unwrap_err()
                .1,
            "drive_images_not_configured"
        );
        assert!(require_cloud_image_coverage(&data, &BTreeSet::from(["30".into()])).is_ok());
        let mut without_images = data.clone();
        without_images["capture_bundle"]["jobs"][0]["images"] = json!([]);
        assert!(require_cloud_image_coverage(&without_images, &BTreeSet::new()).is_ok());
        let mut partial = data;
        let mut second = partial["capture_bundle"]["jobs"][0].clone();
        second["hubspotListingId"] = json!("31");
        partial["capture_bundle"]["jobs"]
            .as_array_mut()
            .unwrap()
            .push(second);
        assert!(require_cloud_image_coverage(&partial, &BTreeSet::from(["30".into()])).is_err());
    }
    fn moc_fixture() -> Value {
        json!({"schemaVersion":1,"capturedAt":"2026-10-04T01:00:00Z",
            "capture_bundle":{"schemaVersion":1,"capturedAt":"2026-10-03T01:00:00+09:00","jobs":[{
                "id":"capture-job-30","hubspotListingId":"30","title":"Synthetic job","body":"Salary 1200 yen",
                "images":[{"id":"image-1","url":"data:image/png;base64,YQ=="}]}]},
            "results":[{"listing_id":"30","summary":{"total":11,"duplicate_ids":0,"missing_date":2,
                "by_date":{"2026-10-01":9},"dimensions":{"gender":{"男性":7,"不明":4}}},
                "dated_comparison":{"rule_version":"synthetic","total":11,"unknown":11,
                    "by_version":{"version-1":{"count":0,"dimensions":{"gender":{"denominator":0,"categories":[]}}}},
                    "daily_representatives":{},"excluded_observations":[]}}]})
    }
    #[test]
    fn moc_preserves_real_counts_and_unknown_attribution_without_fabrication() {
        let data = moc_fixture();
        validate_moc(&data).unwrap();
        assert_eq!(data["results"][0]["summary"]["total"], 11);
        assert_eq!(data["results"][0]["dated_comparison"]["unknown"], 11);
        assert_eq!(
            data["results"][0]["dated_comparison"]["by_version"]["version-1"]["count"],
            0
        );
    }
    #[test]
    fn moc_rejects_duplicate_or_foreign_listings_and_row_level_applicant_data() {
        let original = moc_fixture();
        let mut duplicate = original.clone();
        let job = duplicate["capture_bundle"]["jobs"][0].clone();
        duplicate["capture_bundle"]["jobs"]
            .as_array_mut()
            .unwrap()
            .push(job);
        assert_eq!(
            validate_moc(&duplicate).unwrap_err().1,
            "moc_schema_invalid"
        );
        for (path, value) in [
            ("listing_id", json!("31")),
            ("applicants", json!([{"email":"private@example.test"}])),
        ] {
            let mut data = original.clone();
            data["results"][0][path] = value;
            assert!(validate_moc(&data).is_err());
        }
        let mut data = original;
        data["results"][0]["dated_comparison"]["by_version"]["version-1"]["properties"] =
            json!({"phone":"synthetic-private"});
        assert!(validate_moc(&data).is_err());
    }
    #[test]
    fn moc_rejects_bad_dates_versions_and_remote_or_script_image_urls() {
        for timestamp in ["2026-10-04", "invalid", "2026-99-99T00:00:00Z"] {
            let mut data = moc_fixture();
            data["capturedAt"] = json!(timestamp);
            assert!(validate_moc(&data).is_err());
        }
        for url in [
            "https://external.test/image.png",
            "javascript:alert(1)",
            "data:image/svg+xml;base64,YQ==",
        ] {
            let mut data = moc_fixture();
            data["capture_bundle"]["jobs"][0]["images"][0]["url"] = json!(url);
            assert!(validate_moc(&data).is_err());
        }
        let mut data = moc_fixture();
        data["schemaVersion"] = json!(2);
        assert!(validate_moc(&data).is_err());
    }
    #[tokio::test]
    async fn moc_reads_only_configured_file_and_bounds_file_size() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("synthetic-moc.json");
        assert_eq!(load_moc(None).await.unwrap_err().0, StatusCode::NOT_FOUND);
        assert_eq!(
            load_moc(Some(&path)).await.unwrap_err().1,
            "moc_file_unavailable"
        );
        tokio::fs::write(&path, serde_json::to_vec(&moc_fixture()).unwrap())
            .await
            .unwrap();
        assert_eq!(
            load_moc(Some(&path)).await.unwrap()["results"][0]["summary"]["total"],
            11
        );
        tokio::fs::write(&path, b"not JSON with private@example.test")
            .await
            .unwrap();
        assert_eq!(
            load_moc(Some(&path)).await.unwrap_err().1,
            "moc_schema_invalid"
        );
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MOC_MAX_BYTES as u64 + 1).unwrap();
        assert_eq!(
            load_moc(Some(&path)).await.unwrap_err().1,
            "moc_file_too_large_or_invalid"
        );
    }
    #[tokio::test]
    async fn moc_errors_have_no_store_and_static_codes_without_private_values() {
        use axum::body::to_bytes;
        let response = moc_invalid().into_response();
        assert_eq!(response.headers()[header::CACHE_CONTROL], "no-store");
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<Value>(&body).unwrap(),
            json!({"code":"moc_schema_invalid"})
        );
    }
    #[test]
    fn only_explicit_oidc_users_may_read() {
        let allowed = BTreeSet::from(["reader@example.test".into()]);
        assert_eq!(
            authorize(None, Some(LOGIN_METHOD_GOOGLE_OIDC), &allowed)
                .unwrap_err()
                .0,
            StatusCode::UNAUTHORIZED
        );
        assert_eq!(
            authorize(Some("reader@example.test"), Some("password"), &allowed)
                .unwrap_err()
                .0,
            StatusCode::FORBIDDEN
        );
        assert!(authorize(
            Some("reader@example.test"),
            Some(LOGIN_METHOD_GOOGLE_OIDC),
            &BTreeSet::new()
        )
        .is_err());
        assert!(authorize(
            Some("READER@example.test"),
            Some(LOGIN_METHOD_GOOGLE_OIDC),
            &allowed
        )
        .is_ok());
    }
    #[test]
    fn ids_cannot_change_paths() {
        for id in ["", "1/associations", "../", "1?token=bad"] {
            assert!(valid_id(id).is_err());
        }
        assert!(valid_id("123").is_ok());
    }
    #[test]
    fn aggregate_deduplicates_and_preserves_unknowns() {
        let a = Record {
            id: "1".into(),
            properties: BTreeMap::from([
                ("yingmuri".into(), Some("2026-10-03".into())),
                ("seibetsu".into(), Some("男性".into())),
                ("nenrei".into(), Some("35".into())),
            ]),
        };
        let b = Record {
            id: "2".into(),
            properties: BTreeMap::from([
                ("yingmuri".into(), Some("bad".into())),
                ("nenrei".into(), Some("999".into())),
            ]),
        };
        let summary = summarize(&[a.clone(), a, b]);
        assert_eq!(summary["total"], 2);
        assert_eq!(summary["duplicate_ids"], 1);
        assert_eq!(summary["missing_date"], 1);
        assert_eq!(summary["by_date"]["2026-10-03"], 1);
        assert_eq!(summary["dimensions"]["age"]["30代"], 1);
        assert_eq!(summary["dimensions"]["age"]["不明"], 1);
        assert_eq!(summary["dimensions"]["gender"]["男性"], 1);
        assert_eq!(summary["dimensions"]["gender"]["不明"], 1);
    }

    fn applicant(id: &str, prefecture: Option<&str>, city: Option<&str>) -> Record {
        Record {
            id: id.into(),
            properties: BTreeMap::from([
                ("yingmuri".into(), Some("2026-10-03".into())),
                ("seibetsu".into(), Some("女性".into())),
                ("nenrei".into(), Some("28".into())),
                ("todoufuken".into(), prefecture.map(str::to_owned)),
                ("shikuchouson".into(), city.map(str::to_owned)),
            ]),
        }
    }

    #[test]
    fn summary_rounds_applicant_addresses_before_they_leave_the_server() {
        let rows = [
            applicant(
                "1",
                Some("大分県"),
                Some("大分市府内町3丁目10-1 府内ビル201号室"),
            ),
            applicant("2", Some("大分県"), Some("大分市大手町2-31")),
            applicant("3", None, Some("別府市北浜2-9-1 コーポ北浜102")),
            applicant("4", Some("大分県"), Some("架空町1-2-3")),
            applicant("5", None, None),
        ];
        let summary = summarize(&rows);
        let text = summary.to_string();
        for raw in [
            "府内町",
            "201号室",
            "大手町",
            "北浜",
            "コーポ",
            "架空町",
            "1-2-3",
            " / ",
        ] {
            assert!(
                !text.contains(raw),
                "raw address part {raw:?} left in {text}"
            );
        }
        // summarize() already counts the areas from the protected cells.
        assert_eq!(
            summary["dimensions"]["municipality"],
            json!({"その他": 4, "不明": 1})
        );
        let mut response = json!({"summary": summary});
        protect_applicant_areas(&mut response);
        let sent = &response["summary"];
        // Every city has fewer than 3 applicants, so none of them is named in the response.
        assert_eq!(
            sent["dimensions"]["municipality"],
            json!({"その他": 4, "不明": 1})
        );
        assert_eq!(
            sent["dimensions"]["prefecture"],
            json!({"大分県": 4, "不明": 1})
        );
        assert_eq!(
            sent["joint_demographics"]["cells"],
            json!([
                {"gender": "女性", "age": "20代", "prefecture": "不明", "municipality": "不明", "count": 1},
                {"gender": "女性", "age": "20代", "prefecture": "大分県", "municipality": "その他", "count": 4}
            ])
        );
        assert_eq!(sent["joint_demographics"]["total"], 5);
        let text = response.to_string();
        for city in ["大分市", "別府市", "市区町村不明"] {
            assert!(
                !text.contains(city),
                "{city:?} with fewer than 3 applicants left in {text}"
            );
        }
    }

    #[test]
    fn live_response_never_sends_an_area_or_a_cell_with_fewer_than_three_applicants() {
        let row = |id: &str, gender: &str, age: &str, city: &str| Record {
            id: id.into(),
            properties: BTreeMap::from([
                ("yingmuri".into(), Some("2026-10-03".into())),
                ("seibetsu".into(), Some(gender.into())),
                ("nenrei".into(), Some(age.into())),
                ("todoufuken".into(), Some("大分県".into())),
                ("shikuchouson".into(), Some(city.into())),
            ]),
        };
        let rows = [
            row("1", "女性", "63", "由布市湯布院町1234-5"),
            row("2", "男性", "34", "由布市挾間町"),
            row("3", "男性", "35", "由布市挾間町"),
            row("4", "男性", "24", "大分市府内町"),
            row("5", "男性", "25", "大分市府内町"),
            row("6", "男性", "26", "大分市府内町"),
        ];
        let mut response = json!({"summary": summarize(&rows), "dated_comparison": {"area_rule": VERSION_AREA_RULE, "by_version": {"v1": {"count": 3, "dimensions": {
            "municipality": {"denominator": 3, "categories": [
                {"category": "大分県 / 由布市", "count": 1, "percentage": 33.3},
                {"category": "大分県 / 大分市", "count": 2, "percentage": 66.7}
            ]}
        }}}}});
        protect_applicant_areas(&mut response);
        let summary = &response["summary"];
        // 由布市 has 3 applicants, but 女性・60代・由布市 (1 person) and 男性・30代・由布市 (2) are
        // hidden in the cells, so 由布市 is not named in the totals either: with 「由布市 3」 next to
        // 「男性・20代・大分市 3」, the two hidden cells could be read as 由布市 by subtraction.
        assert_eq!(
            summary["dimensions"]["municipality"],
            json!({"大分県大分市": 3, "その他": 3})
        );
        assert_eq!(
            summary["dimensions"]["prefecture"],
            json!({"大分県": 3, "その他": 3})
        );
        assert!(!response.to_string().contains("由布市"));
        assert_no_hidden_remainder(summary);
        for cell in summary["joint_demographics"]["cells"].as_array().unwrap() {
            let count = cell["count"].as_u64().unwrap();
            assert!(
                count >= 3 || (cell["municipality"] == "その他" || cell["municipality"] == "不明"),
                "small cell names a city: {cell}"
            );
            if count < 3 {
                assert!(
                    cell["prefecture"] == "その他" || cell["prefecture"] == "不明",
                    "{cell}"
                );
            }
        }
        assert_eq!(
            summary["joint_demographics"]["cells"]
                .as_array()
                .unwrap()
                .iter()
                .map(|cell| cell["count"].as_u64().unwrap())
                .sum::<u64>(),
            6
        );
        // Each version is protected on its own: 1 and 2 applicants become その他.
        assert_eq!(
            response["dated_comparison"]["by_version"]["v1"]["dimensions"]["municipality"]
                ["categories"],
            json!([{"category": "その他", "count": 3, "percentage": 100.0}])
        );
    }

    /// Each named area's total equals the sum of the cells that name it, so nothing is left over
    /// to be found by subtracting the named cells.
    fn assert_no_hidden_remainder(summary: &Value) {
        for (key, field) in [
            ("municipality", "municipality"),
            ("prefecture", "prefecture"),
        ] {
            for (area, total) in summary["dimensions"][key].as_object().unwrap() {
                if area == "その他" || area == "不明" {
                    continue;
                }
                let named: u64 = summary["joint_demographics"]["cells"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .filter(|cell| cell[field] == area.as_str())
                    .map(|cell| cell["count"].as_u64().unwrap())
                    .sum();
                assert_eq!(named, total.as_u64().unwrap(), "{key} {area} in {summary}");
            }
        }
    }

    fn applicant_row(id: &str, gender: &str, age: &str, city: &str, date: &str) -> Record {
        Record {
            id: id.into(),
            properties: BTreeMap::from([
                ("yingmuri".into(), Some(date.into())),
                ("seibetsu".into(), Some(gender.into())),
                ("nenrei".into(), Some(age.into())),
                ("todoufuken".into(), Some("大分県".into())),
                ("shikuchouson".into(), Some(city.into())),
            ]),
        }
    }

    #[test]
    fn one_city_with_every_applicant_is_not_named_next_to_single_gender_and_age_counts() {
        // All 3 applications are in 大分市; each gender × age has 1. If 大分市 3 were sent, the
        // gender and age totals would all be 大分市's, and 女性・20代・大分市 = 1件 could be read.
        let rows = [
            applicant_row("1", "女性", "24", "大分市府内町", "2026-10-01"),
            applicant_row("2", "男性", "35", "大分市大手町", "2026-10-02"),
            applicant_row("3", "女性", "47", "大分市中央町", "2026-10-03"),
        ];
        let mut response = json!({"summary": summarize(&rows)});
        protect_applicant_areas(&mut response);
        let summary = &response["summary"];
        assert_eq!(summary["dimensions"]["municipality"], json!({"その他": 3}));
        assert_eq!(summary["dimensions"]["prefecture"], json!({"その他": 3}));
        assert_eq!(
            summary["dimensions"]["gender"],
            json!({"女性": 2, "男性": 1})
        );
        assert!(!response.to_string().contains("大分市"));
        assert!(!response.to_string().contains("大分県"));
        // Three applicants with the same gender and age keep their city.
        let rows = [
            applicant_row("1", "女性", "24", "大分市府内町", "2026-10-01"),
            applicant_row("2", "女性", "25", "大分市大手町", "2026-10-02"),
            applicant_row("3", "女性", "27", "大分市中央町", "2026-10-03"),
        ];
        let mut response = json!({"summary": summarize(&rows)});
        protect_applicant_areas(&mut response);
        assert_eq!(
            response["summary"]["dimensions"]["municipality"],
            json!({"大分県大分市": 3})
        );
        assert_no_hidden_remainder(&response["summary"]);
    }

    #[test]
    fn areas_are_hidden_per_version_when_versions_carry_gender_and_age() {
        // 男性・20代・大分市 is 3 in the job, but 2 in version A and 1 in version B. With the
        // version's gender and age next to it, the job-wide 大分市 cell would place the one
        // version-B applicant in 大分市, so the city is hidden.
        let rows = [
            applicant_row("1", "男性", "24", "大分市府内町", "2026-10-01"),
            applicant_row("2", "男性", "25", "大分市府内町", "2026-10-01"),
            applicant_row("3", "男性", "26", "大分市府内町", "2026-10-05"),
        ];
        let groups = BTreeMap::from([
            ("1".to_owned(), "A".to_owned()),
            ("2".to_owned(), "A".to_owned()),
            ("3".to_owned(), "B".to_owned()),
        ]);
        let summary = summarize_grouped(&rows, &groups);
        assert_eq!(summary["dimensions"]["municipality"], json!({"その他": 3}));
        assert!(!summary.to_string().contains("大分市"));
        let summary = summarize(&rows);
        assert_eq!(
            summary["dimensions"]["municipality"],
            json!({"大分県大分市": 3})
        );
    }

    #[test]
    fn stored_per_version_attributes_without_the_version_rule_are_not_sent() {
        let mut response = json!({"summary": {"total": 3, "dimensions": {}}, "dated_comparison": {"by_version": {"v1": {"count": 3, "dimensions": {
            "gender": {"denominator": 3, "categories": [{"category": "女性", "count": 3, "percentage": 100.0}]},
            "municipality": {"denominator": 3, "categories": [{"category": "大分県 / 大分市", "count": 3, "percentage": 100.0}]}
        }}}}});
        protect_applicant_areas(&mut response);
        let version = &response["dated_comparison"]["by_version"]["v1"];
        assert_eq!(version["count"], 3);
        assert!(version["dimensions"]["gender"].is_null());
        assert!(version["dimensions"]["municipality"].is_null());
    }

    #[test]
    fn area_totals_without_cells_name_no_area() {
        let mut response = json!({"summary": {"total": 4, "dimensions": {
            "gender": {"女性": 4},
            "municipality": {"大分県 / 大分市": 3, "不明": 1},
            "prefecture": {"大分県": 3, "不明": 1}
        }}});
        protect_applicant_areas(&mut response);
        assert_eq!(
            response["summary"]["dimensions"]["municipality"],
            json!({"その他": 3, "不明": 1})
        );
        assert_eq!(
            response["summary"]["dimensions"]["prefecture"],
            json!({"その他": 3, "不明": 1})
        );
    }

    #[test]
    fn stored_reason_texts_are_masked_before_sending() {
        let mut data = json!({"results": [{
            "listing_id": "30",
            "summary": {"total": 1, "dimensions": {}},
            "applicant_reasons": {"items": [
                {"text": "大分市府内町3丁目10-1 山田さん 090-1234-5678"},
                {"text": "週3日から働けるため"}
            ]}
        }]});
        round_snapshot_areas(&mut data);
        let items = &data["results"][0]["applicant_reasons"]["items"];
        assert_eq!(items[0]["text"], "＊＊ ＊＊さん ＊＊");
        assert_eq!(items[1]["text"], "週3日から働けるため");
        let text = data.to_string();
        for raw in ["府内町", "山田", "090", "1234", "5678"] {
            assert!(!text.contains(raw), "{raw:?} left in {text}");
        }
    }

    #[test]
    fn stored_selection_values_are_masked_before_sending() {
        let mut data = json!({"results": [{
            "listing_id": "30",
            "summary": {"total": 1, "dimensions": {}},
            "applicant_reasons": {"selections": [
                {"value": "給与", "label": "給与"},
                {"value": "090-1234-5678", "label": null}
            ]}
        }]});
        round_snapshot_areas(&mut data);
        let selections = &data["results"][0]["applicant_reasons"]["selections"];
        assert_eq!(selections[0]["value"], "給与");
        assert_eq!(selections[0]["label"], "給与");
        assert_eq!(selections[1]["value"], "＊＊");
        assert!(selections[1]["label"].is_null());
    }

    #[test]
    fn option_labels_keep_only_the_category_selects() {
        let labels = option_labels(&json!({"results":[
            {"name":"ouboriyuukategori_hiaringu","options":[{"value":"a","label":"給与"},{"value":"","label":"x"}]},
            {"name":"other","options":[{"value":"b","label":"y"}]}
        ]}))
        .unwrap();
        assert_eq!(labels.len(), 1);
        assert_eq!(labels["ouboriyuukategori_hiaringu"].len(), 1);
        assert_eq!(labels["ouboriyuukategori_hiaringu"]["a"], "給与");
        assert!(option_labels(&json!({"results":[]})).is_none());
        assert!(option_labels(&json!({"message":"x"})).is_none());
    }

    #[test]
    fn rounded_attributes_keep_only_master_names() {
        let area = rounded_area(&applicant(
            "1",
            Some("東京都千代田区丸の内1-1-1"),
            Some("○○マンション305"),
        ));
        // The city field holds only a building name, so the city is not guessed from the
        // prefecture field and the building name is dropped.
        assert_eq!(area.prefecture.as_deref(), Some("東京都"));
        assert_eq!(area.municipality, None);
        let area = rounded_area(&applicant(
            "2",
            Some("東京都"),
            Some("千代田区丸の内1-1-1 ○○マンション305"),
        ));
        assert_eq!(area.municipality.as_deref(), Some("千代田区"));
    }

    #[test]
    fn snapshot_areas_are_rounded_and_merged_before_sending() {
        let mut data = json!({"results": [{
            "listing_id": "30",
            "summary": {
                "total": 4,
                "dimensions": {
                    "gender": {"女性": 4},
                    "prefecture": {"大分県": 3, "大分県大分市府内町3-10-1": 1},
                    "municipality": {"大分県 / 大分市府内町3-10-1 201号室": 1, "大分県 / 大分市大手町2-31": 2, "都道府県不明 / 別府市北浜2-9-1": 1}
                },
                "joint_demographics": {"total": 4, "cells": [
                    {"gender": "女性", "age": "20代", "prefecture": "大分県", "municipality": "大分県 / 大分市府内町3-10-1", "count": 1},
                    {"gender": "女性", "age": "20代", "prefecture": "大分県", "municipality": "大分県 / 大分市大手町2-31", "count": 2},
                    {"gender": "女性", "age": "20代", "prefecture": "不明", "municipality": "都道府県不明 / 別府市北浜2-9-1", "count": 1}
                ]}
            },
            "dated_comparison": {"by_version": {"v1": {"count": 4, "dimensions": {
                "municipality": {"denominator": 4, "categories": [
                    {"category": "大分県 / 大分市府内町3-10-1", "count": 1, "percentage": 25.0},
                    {"category": "大分県 / 大分市大手町2-31", "count": 2, "percentage": 50.0},
                    {"category": "不明", "count": 1, "percentage": 25.0}
                ]},
                "prefecture": null
            }}}}
        }]});
        round_snapshot_areas(&mut data);
        let result = &data["results"][0];
        // The 別府市 applicant's cell has 1 applicant, so 大分県 is hidden for it in the cells and
        // the 大分県 total is counted from the cells (3), not taken from the stored total (4).
        assert_eq!(
            result["summary"]["dimensions"]["prefecture"],
            json!({"大分県": 3, "その他": 1})
        );
        assert_eq!(
            result["summary"]["dimensions"]["municipality"],
            json!({"大分県大分市": 3, "その他": 1})
        );
        assert_eq!(
            result["summary"]["dimensions"]["gender"],
            json!({"女性": 4})
        );
        assert_eq!(
            result["summary"]["joint_demographics"]["cells"],
            json!([
                {"gender": "女性", "age": "20代", "prefecture": "大分県", "municipality": "大分県大分市", "count": 3},
                {"gender": "女性", "age": "20代", "prefecture": "その他", "municipality": "その他", "count": 1}
            ])
        );
        // Written without the per-version rule, so the per-version attributes are not sent.
        assert!(
            result["dated_comparison"]["by_version"]["v1"]["dimensions"]["municipality"].is_null()
        );
        assert_eq!(result["dated_comparison"]["by_version"]["v1"]["count"], 4);
        assert!(
            result["dated_comparison"]["by_version"]["v1"]["dimensions"]["prefecture"].is_null()
        );
        // With the rule, the per-version labels are rounded.
        let mut data = json!({"results": [{"summary": {"total": 4}, "dated_comparison": {"area_rule": VERSION_AREA_RULE, "by_version": {"v1": {"count": 4, "dimensions": {
            "municipality": {"denominator": 4, "categories": [
                {"category": "大分県 / 大分市府内町3-10-1", "count": 1, "percentage": 25.0},
                {"category": "大分県 / 大分市大手町2-31", "count": 2, "percentage": 50.0},
                {"category": "不明", "count": 1, "percentage": 25.0}
            ]}
        }}}}}]});
        round_snapshot_areas(&mut data);
        assert_eq!(
            data["results"][0]["dated_comparison"]["by_version"]["v1"]["dimensions"]
                ["municipality"],
            json!({"denominator": 4, "categories": [
                {"category": "大分県大分市", "count": 3, "percentage": 75.0},
                {"category": "不明", "count": 1, "percentage": 25.0}
            ]})
        );
        let text = data.to_string();
        for raw in ["府内町", "201号室", "大手町", "北浜", " / "] {
            assert!(
                !text.contains(raw),
                "raw address part {raw:?} left in {text}"
            );
        }
    }
}

#[cfg(test)]
#[path = "job_copy_live/tests.rs"]
mod integration_tests;
