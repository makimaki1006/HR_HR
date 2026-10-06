//! HubSpot points to immutable Drive evidence. No public Drive URLs or arbitrary file proxy.
use super::{job_copy_drive::DriveReader, job_copy_live::JobReadService};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{collections::BTreeSet, sync::Arc, time::Duration};

pub const PROPERTY: &str = "job_copy_drive_manifest_v1";
pub type BridgeResult<T> = Result<T, &'static str>;

pub fn drive_id(value: &str) -> bool {
    (10..=200).contains(&value.len())
        && value
            .bytes()
            .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'-')
}
fn record_id(value: &str) -> bool {
    !value.is_empty() && value.len() <= 30 && value.bytes().all(|c| c.is_ascii_digit())
}
fn hash(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Pointer {
    pub file_id: String,
    pub sha256: String,
    pub observed_at: String,
}
impl Pointer {
    pub fn validate(&self) -> BridgeResult<()> {
        if !drive_id(&self.file_id)
            || !hash(&self.sha256)
            || chrono::DateTime::parse_from_rfc3339(&self.observed_at).is_err()
        {
            return Err("invalid_image_pointer");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ManifestImage {
    pub slot: usize,
    pub file_id: String,
    pub sha256: String,
    pub mime_type: String,
    pub size: usize,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: u32,
    pub listing_id: String,
    pub company_ids: Vec<String>,
    pub observed_at: String,
    pub images: Vec<ManifestImage>,
    pub operation_id: String,
    #[serde(default)]
    pub previous_manifest_file_id: Option<String>,
    #[serde(default)]
    pub previous_manifest_sha256: Option<String>,
}
impl Manifest {
    pub fn validate(&self) -> BridgeResult<()> {
        let mut slots = BTreeSet::new();
        if self.schema_version != 1
            || !record_id(&self.listing_id)
            || !hash(&self.operation_id)
            || self.company_ids.is_empty()
            || self.company_ids.len() > 100
            || self.company_ids.iter().any(|x| !record_id(x))
            || self.company_ids.iter().collect::<BTreeSet<_>>().len() != self.company_ids.len()
            || chrono::DateTime::parse_from_rfc3339(&self.observed_at).is_err()
            || self.images.len() > 100
        {
            return Err("invalid_image_manifest");
        }
        for image in &self.images {
            if image.slot == 0
                || image.slot > 100
                || !slots.insert(image.slot)
                || !drive_id(&image.file_id)
                || !hash(&image.sha256)
                || !["image/jpeg", "image/png", "image/webp"].contains(&image.mime_type.as_str())
                || image.size == 0
                || image.size > 5 * 1024 * 1024
            {
                return Err("invalid_image_manifest");
            }
        }
        match (
            &self.previous_manifest_file_id,
            &self.previous_manifest_sha256,
        ) {
            (None, None) => (),
            (Some(id), Some(sha)) if drive_id(id) && hash(sha) => (),
            _ => return Err("invalid_previous_manifest"),
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct ImageBridge {
    http: reqwest::Client,
    token: String,
    base: String,
    pub reader: Arc<DriveReader>,
    jobs: Arc<JobReadService>,
}
impl ImageBridge {
    pub fn new(
        token: String,
        reader: Arc<DriveReader>,
        jobs: Arc<JobReadService>,
    ) -> BridgeResult<Self> {
        if token.is_empty() {
            return Err("hubspot_not_configured");
        }
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(20))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "client_unavailable")?;
        Ok(Self {
            http,
            token,
            base: "https://api.hubapi.com".into(),
            reader,
            jobs,
        })
    }
    #[cfg(test)]
    pub(crate) fn for_test(base: &str, reader: Arc<DriveReader>) -> Self {
        let jobs = Arc::new(JobReadService::for_test(base.to_owned()));
        let mut bridge = Self::new("test-only".into(), reader, jobs).unwrap();
        bridge.base = base.to_owned();
        bridge
    }

    async fn get(&self, path: &str) -> BridgeResult<(StatusCode, Value)> {
        let response = self
            .http
            .get(format!("{}{path}", self.base))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| "hubspot_read_failed")?;
        let status = response.status();
        if !status.is_success() {
            return Ok((status, Value::Null));
        }
        let value = response
            .json()
            .await
            .map_err(|_| "hubspot_response_invalid")?;
        Ok((status, value))
    }
    pub async fn current(&self, listing: &str) -> BridgeResult<Option<Pointer>> {
        if !record_id(listing) {
            return Err("invalid_listing_id");
        }
        let (status, value) = self
            .get(&format!(
                "/crm/v3/objects/0-420/{listing}?properties={PROPERTY}"
            ))
            .await?;
        if !status.is_success() {
            return Err("hubspot_pointer_read_failed");
        }
        match value["properties"][PROPERTY].as_str() {
            None | Some("") => Ok(None),
            Some(raw) => {
                let pointer: Pointer =
                    serde_json::from_str(raw).map_err(|_| "invalid_image_pointer")?;
                pointer.validate()?;
                Ok(Some(pointer))
            }
        }
    }
    pub async fn manifest(&self, pointer: &Pointer) -> BridgeResult<Manifest> {
        pointer.validate()?;
        let raw = self
            .reader
            .read_manifest_verified(&pointer.file_id, &pointer.sha256)
            .await
            .map_err(|_| "drive_manifest_read_failed")?;
        let manifest: Manifest =
            serde_json::from_value(raw).map_err(|_| "invalid_image_manifest")?;
        manifest.validate()?;
        if manifest.observed_at != pointer.observed_at {
            return Err("manifest_date_mismatch");
        }
        Ok(manifest)
    }
    pub async fn authorize(&self, company: &str, listing: &str) -> BridgeResult<()> {
        self.jobs
            .validate_customer_listing(company, listing)
            .await
            .map_err(|error| {
                if error.status() == StatusCode::FORBIDDEN {
                    "customer_listing_not_authorized"
                } else {
                    error.code()
                }
            })
    }
    pub async fn image(
        &self,
        company: &str,
        listing: &str,
        manifest_id: &str,
        slot: usize,
    ) -> BridgeResult<(String, Vec<u8>)> {
        if !drive_id(manifest_id) || slot == 0 || slot > 100 {
            return Err("invalid_image_request");
        }
        let (_, manifest) = self.resolve_manifest(company, listing, manifest_id).await?;
        let image = manifest
            .images
            .iter()
            .find(|i| i.slot == slot)
            .ok_or("image_slot_missing")?;
        let (mime, bytes) = self
            .reader
            .read_image(&image.file_id, &image.sha256)
            .await
            .map_err(|_| "drive_image_read_failed")?;
        if mime != image.mime_type || bytes.len() != image.size {
            return Err("image_metadata_mismatch");
        }
        Ok((mime, bytes))
    }
    pub async fn resolve_manifest(
        &self,
        company: &str,
        listing: &str,
        manifest_id: &str,
    ) -> BridgeResult<(Pointer, Manifest)> {
        if !drive_id(manifest_id) {
            return Err("invalid_image_request");
        }
        self.authorize(company, listing).await?;
        let mut pointer = self.current(listing).await?.ok_or("image_not_linked")?;
        let mut seen = BTreeSet::new();
        for _ in 0..100 {
            if !seen.insert(pointer.file_id.clone()) {
                return Err("manifest_chain_cycle");
            }
            let manifest = self.manifest(&pointer).await?;
            if manifest.listing_id != listing
                || !manifest.company_ids.iter().any(|id| id == company)
            {
                return Err("manifest_customer_mismatch");
            }
            if pointer.file_id == manifest_id {
                return Ok((pointer, manifest));
            }
            let (Some(id), Some(sha)) = (
                manifest.previous_manifest_file_id,
                manifest.previous_manifest_sha256,
            ) else {
                return Err("manifest_not_linked_to_listing");
            };
            // Previous date comes from hash-verified immutable evidence, never a query parameter.
            let raw = self
                .reader
                .read_manifest_verified(&id, &sha)
                .await
                .map_err(|_| "drive_manifest_read_failed")?;
            let previous: Manifest =
                serde_json::from_value(raw).map_err(|_| "invalid_image_manifest")?;
            previous.validate()?;
            if chrono::DateTime::parse_from_rfc3339(&previous.observed_at)
                .map_err(|_| "invalid_manifest_date")?
                > chrono::DateTime::parse_from_rfc3339(&pointer.observed_at)
                    .map_err(|_| "invalid_manifest_date")?
            {
                return Err("manifest_chain_date_order");
            }
            pointer = Pointer {
                file_id: id,
                sha256: sha,
                observed_at: previous.observed_at,
            };
        }
        Err("manifest_chain_limit")
    }
    /// Explicit operator CLI only: provision one namespaced string property idempotently.
    pub async fn ensure_property(&self) -> BridgeResult<()> {
        let path = format!("/crm/v3/properties/0-420/{PROPERTY}");
        let (status, existing) = self.get(&path).await?;
        if status.is_success() {
            return validate_property(&existing);
        }
        if status != StatusCode::NOT_FOUND {
            return Err("property_schema_read_failed");
        }
        let _response = self.http.post(format!("{}/crm/v3/properties/0-420", self.base))
            .bearer_auth(&self.token).json(&json!({"name":PROPERTY,"label":"求人画像観測参照（Drive連携）",
                "type":"string","fieldType":"text","groupName":"tab_p_7_shisutemujouhou",
                "description":"求人画像の不変manifest参照。専用連携が管理。画像本体はGoogle Drive。"}))
            .send().await;
        // Deterministic property name also handles a successful CREATE with lost response.
        let (status, value) = self.get(&path).await?;
        if !status.is_success() {
            return Err("property_create_unconfirmed");
        }
        validate_property(&value)
    }
    /// Single-writer pilot. Immutable Drive manifest exists before this retryable PATCH.
    pub async fn publish(
        &self,
        company: &str,
        listing: &str,
        pointer: &Pointer,
    ) -> BridgeResult<&'static str> {
        self.authorize(company, listing).await?;
        let manifest = self.manifest(pointer).await?;
        if manifest.listing_id != listing || !manifest.company_ids.iter().any(|c| c == company) {
            return Err("manifest_customer_mismatch");
        }
        let current = self.current(listing).await?;
        // A pointer alone does not prove the originals are still available on retries.
        for image in &manifest.images {
            let (mime, raw) = self
                .reader
                .read_image(&image.file_id, &image.sha256)
                .await
                .map_err(|_| "original_not_verified")?;
            if mime != image.mime_type || raw.len() != image.size {
                return Err("original_metadata_mismatch");
            }
        }
        if current.as_ref() == Some(pointer) {
            return Ok("already_linked");
        }
        if current.is_some() {
            match self
                .resolve_manifest(company, listing, &pointer.file_id)
                .await
            {
                Ok((stored, _)) if stored == *pointer => return Ok("already_linked_historical"),
                Ok(_) => return Err("published_pointer_conflict"),
                Err("manifest_not_linked_to_listing") => (),
                Err(code) => return Err(code),
            }
        }
        validate_transition(current.as_ref(), &manifest)?;
        let payload = serde_json::to_string(pointer).map_err(|_| "pointer_serialization_failed")?;
        // No other properties or associations are mutated. Readback, not HTTP acknowledgement, completes the operation.
        let _response = self
            .http
            .patch(format!("{}/crm/v3/objects/0-420/{listing}", self.base))
            .bearer_auth(&self.token)
            .json(&json!({"properties":{PROPERTY:payload}}))
            .send()
            .await;
        if self.current(listing).await?.as_ref() == Some(pointer) {
            Ok("linked")
        } else {
            Err("hubspot_sync_pending")
        }
    }
}
fn validate_property(value: &Value) -> BridgeResult<()> {
    if value["name"] != PROPERTY
        || value["type"] != "string"
        || value["fieldType"] != "text"
        || value["calculated"] == true
        || value["archived"] == true
    {
        return Err("image_property_schema_conflict");
    }
    Ok(())
}
pub fn validate_transition(current: Option<&Pointer>, next: &Manifest) -> BridgeResult<()> {
    match current {
        None if next.previous_manifest_file_id.is_none() => Ok(()),
        Some(old)
            if next.previous_manifest_file_id.as_deref() == Some(&old.file_id)
                && next.previous_manifest_sha256.as_deref() == Some(&old.sha256)
                && chrono::DateTime::parse_from_rfc3339(&next.observed_at)
                    .map_err(|_| "invalid_manifest_date")?
                    >= chrono::DateTime::parse_from_rfc3339(&old.observed_at)
                        .map_err(|_| "invalid_manifest_date")? =>
        {
            Ok(())
        }
        _ => Err("manifest_previous_version_conflict"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn manifest() -> Manifest {
        serde_json::from_value(json!({"schemaVersion":1,"listingId":"30","companyIds":["10"],
            "observedAt":"2026-10-05T00:00:00Z","images":[{"slot":1,"fileId":"file_1234567890",
            "sha256":"a".repeat(64),"mimeType":"image/jpeg","size":100}],"operationId":"b".repeat(64)})).unwrap()
    }
    #[test]
    fn invalid_manifest_never_resolves_foreign_paths_or_unbounded_images() {
        let original = manifest();
        original.validate().unwrap();
        let mut m = original.clone();
        m.images[0].file_id = "../../secret".into();
        assert!(m.validate().is_err());
        let mut m = original.clone();
        m.images.push(m.images[0].clone());
        assert!(m.validate().is_err());
        let mut m = original.clone();
        m.images[0].mime_type = "image/svg+xml".into();
        assert!(m.validate().is_err());
        let mut m = original;
        m.images[0].size = 6 * 1024 * 1024;
        assert!(m.validate().is_err());
    }
    #[test]
    fn next_version_requires_exact_predecessor_and_never_rolls_back() {
        let mut m = manifest();
        assert!(validate_transition(None, &m).is_ok());
        let old = Pointer {
            file_id: "manifest_1234567890".into(),
            sha256: "c".repeat(64),
            observed_at: m.observed_at.clone(),
        };
        assert!(validate_transition(Some(&old), &m).is_err());
        m.previous_manifest_file_id = Some(old.file_id.clone());
        m.previous_manifest_sha256 = Some(old.sha256.clone());
        assert!(validate_transition(Some(&old), &m).is_ok());
        m.observed_at = "2026-10-04T00:00:00Z".into();
        assert!(validate_transition(Some(&old), &m).is_err());
        assert!(validate_transition(None, &m).is_err());
    }
    #[test]
    fn unknown_pointer_fields_and_existing_schema_conflicts_fail_closed() {
        assert!(serde_json::from_value::<Pointer>(json!({"fileId":"x","token":"secret"})).is_err());
        assert!(
            validate_property(&json!({"name":PROPERTY,"type":"number","fieldType":"text"}))
                .is_err()
        );
    }
}

#[cfg(test)]
#[path = "job_copy_image_bridge/tests.rs"]
mod integration_tests;
