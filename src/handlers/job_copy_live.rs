//! Read-only job management. Existing HubSpot associations remain the source of truth.
use crate::{
    auth::{LOGIN_METHOD_GOOGLE_OIDC, SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY},
    AppState,
};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
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

#[derive(Debug)]
pub struct ReadError(StatusCode, &'static str);
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

pub struct JobReadService {
    client: reqwest::Client,
    token: String,
    base: String,
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
    pub async fn applicants(&self, company: &str, listing: &str) -> Result<Value, ReadError> {
        self.validate_customer_listing(company, listing).await?;
        let ids = self.all_associations("0-420", listing, "0-421").await?;
        let rows = self
            .batch(
                "0-421",
                &ids,
                &[
                    "yingmuri",
                    "seibetsu",
                    "nenrei",
                    "todoufuken",
                    "shikuchouson",
                ],
            )
            .await?;
        // Missing/undefined properties remain unknown; do not turn an existing
        // appointment into zero applications. hs_appointment_start is synthesized
        // by the importer from yingmuri and does not prove the real event time.
        let summary = summarize(&rows);
        let mut response = json!({"listing_id":listing,"metric":"HubSpot応募レコード数","summary":summary,"version_attribution":"日次観測との対応は別途必要。現在の関連による集計。","attribute_basis":"現在取得できる属性","billing":null,"capture_bundle":null,"dated_comparison":null,"capture_status":"not_configured_or_not_matched"});
        let record = self
            .batch(
                "0-420",
                &[listing.to_owned()],
                &["id_hrhakkaa", "id_shop_hrhakkaa"],
            )
            .await?;
        match super::job_copy_capture::capture_for_hr_job(
            record.first().and_then(|r| r.value("id_hrhakkaa")),
            listing,
            record.first().and_then(|r| r.value("id_shop_hrhakkaa")),
        )
        .await
        {
            Ok(Some(bundle)) => {
                let mut unambiguous = BTreeSet::new();
                for chunk in ids.chunks(100) {
                    let data=self.request("/crm/v4/associations/0-421/0-420/batch/read",&[],Some(json!({"inputs":chunk.iter().map(|id|json!({"id":id})).collect::<Vec<_>>()}))).await?;
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
                        if target_ids.len() == 1
                            && target_ids.contains(listing)
                            && result.get("paging").is_none()
                        {
                            if let Some(id) = result.pointer("/from/id").and_then(Value::as_str) {
                                unambiguous.insert(id.to_owned());
                            }
                        }
                    }
                }
                let applications: Vec<_> = rows
                    .iter()
                    .map(|row| crate::job_copy_date::ApplicantRecord {
                        application_id: row.id.clone(),
                        application_date: row
                            .value("yingmuri")
                            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
                            .map(|date| crate::job_copy_date::ApplicationDate::DateOnly { date })
                            .unwrap_or(crate::job_copy_date::ApplicationDate::Unknown),
                        listing_unambiguous: unambiguous.contains(&row.id),
                        attributes: crate::job_copy_date::ApplicantAttributes {
                            gender: row.value("seibetsu").map(str::to_owned),
                            age: row
                                .value("nenrei")
                                .and_then(|v| v.parse::<u8>().ok())
                                .filter(|age| *age <= 120),
                            prefecture: row.value("todoufuken").map(str::to_owned),
                            municipality: row.value("shikuchouson").map(str::to_owned),
                        },
                    })
                    .collect();
                match super::job_copy_capture::dated_comparison(&bundle, &applications) {
                    Ok(comparison) => {
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
        Ok(response)
    }
}

pub fn summarize(rows: &[Record]) -> Value {
    let unique: BTreeMap<_, _> = rows.iter().map(|row| (&row.id, row)).collect();
    let mut dates: BTreeMap<String, usize> = BTreeMap::new();
    let mut dimensions: BTreeMap<&str, BTreeMap<String, usize>> = BTreeMap::new();
    let mut missing_date = 0;
    for row in unique.values() {
        if let Some(date) = row
            .value("yingmuri")
            .and_then(|v| chrono::NaiveDate::parse_from_str(v, "%Y-%m-%d").ok())
        {
            *dates.entry(date.to_string()).or_default() += 1;
        } else {
            missing_date += 1;
        }
        for (dimension, property) in [
            ("gender", "seibetsu"),
            ("prefecture", "todoufuken"),
            ("municipality", "shikuchouson"),
        ] {
            let label = if dimension == "municipality" {
                match (row.value("todoufuken"), row.value(property)) {
                    (Some(pref), Some(city)) => format!("{pref} / {city}"),
                    (None, Some(city)) => format!("都道府県不明 / {city}"),
                    _ => "不明".into(),
                }
            } else {
                row.value(property).unwrap_or("不明").into()
            };
            *dimensions
                .entry(dimension)
                .or_default()
                .entry(label)
                .or_default() += 1;
        }
        let label = row
            .value("nenrei")
            .and_then(|v| v.parse::<u32>().ok())
            .filter(|age| *age <= 120)
            .map(|age| {
                if age < 20 {
                    "20歳未満".into()
                } else if age >= 70 {
                    "70歳以上".into()
                } else {
                    format!("{}代", age / 10 * 10)
                }
            })
            .unwrap_or("不明".into());
        *dimensions
            .entry("age")
            .or_default()
            .entry(label)
            .or_default() += 1;
    }
    json!({"total":unique.len(),"duplicate_ids":rows.len()-unique.len(),"by_date":dates,"missing_date":missing_date,"dimensions":dimensions})
}

#[derive(Clone)]
struct Access {
    allowed: BTreeSet<String>,
    service: Option<Arc<JobReadService>>,
    moc_path: Option<PathBuf>,
    moc_drive: Result<Option<SnapshotPointer>, &'static str>,
    snapshot_reader: Option<Arc<super::job_copy_drive::DriveReader>>,
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
async fn authorized_user(
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
            || !scalars_except(result, &["summary", "dated_comparison"])
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
                ],
            )
            || ["total", "duplicate_ids", "missing_date"]
                .iter()
                .any(|key| summary[*key].as_u64().is_none())
            || !counts(&summary["by_date"])
            || !aggregate_dimensions(&summary["dimensions"])
            || !result["billing"].is_null()
            || !result["capture_bundle"].is_null()
        {
            return Err(moc_invalid());
        }
        let comparison = &result["dated_comparison"];
        if !comparison.is_null() {
            if !keys_only(
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
                || !comparison["daily_representatives"]
                    .as_object()
                    .is_some_and(|representatives| {
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
                    })
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
                    })
            {
                return Err(moc_invalid());
            }
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
async fn moc(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
) -> Result<impl IntoResponse, ReadError> {
    authorized_user(&state, &access, &session).await?;
    if let Some(code) = access.drive_config_error {
        return Err(ReadError(StatusCode::SERVICE_UNAVAILABLE, code));
    }
    let mut data = match access
        .moc_drive
        .as_ref()
        .map_err(|code| ReadError(StatusCode::SERVICE_UNAVAILABLE, code))?
    {
        Some(pointer) => {
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
            data
        }
        None => load_moc(access.moc_path.as_deref()).await?,
    };
    if !access.drive_listings.is_empty() {
        let bridge = access.images.as_ref().ok_or(ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "drive_not_configured",
        ))?;
        hydrate_drive_images(&mut data, bridge, &access.drive_listings).await?;
    }
    Ok(([(header::CACHE_CONTROL, "no-store")], Json(data)))
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
        .layer(Extension(Access {
            allowed,
            service,
            images,
            drive_listings,
            drive_config_error,
            snapshot_reader,
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
}

#[cfg(test)]
#[path = "job_copy_live/tests.rs"]
mod integration_tests;
