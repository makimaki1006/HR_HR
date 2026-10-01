//! Read-only HubSpot metadata. Credentials remain on the server; no customer values or writes.
use crate::{
    auth::{LOGIN_METHOD_GOOGLE_OIDC, SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY},
    AppState,
};
use axum::{
    extract::{Query, State},
    http::{header, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_sessions::Session;
use ts_rs::TS;

#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct CrmPropertyOption {
    pub label: String,
    pub value: String,
    #[serde(default)]
    pub hidden: bool,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPropertyDefinition {
    pub object_type: String,
    pub name: String,
    pub label: String,
    pub property_type: String,
    pub field_type: String,
    pub options: Vec<CrmPropertyOption>,
}
#[derive(Clone, Debug, Deserialize, Serialize, TS)]
pub struct CrmStage {
    pub id: String,
    pub label: String,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmPipeline {
    pub id: String,
    pub label: String,
    pub stages: Vec<CrmStage>,
}
#[derive(Clone, Debug, Serialize, TS)]
pub struct CrmMetadataResponse {
    pub properties: Vec<CrmPropertyDefinition>,
    pub pipelines: Vec<CrmPipeline>,
    pub fetched_at: String,
    pub hubspot_ms: f64,
    pub total_ms: f64,
    pub cache_hit: bool,
}
#[derive(Deserialize)]
struct Results<T> {
    results: Vec<T>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubProperty {
    name: String,
    label: String,
    #[serde(rename = "type")]
    property_type: String,
    field_type: String,
    #[serde(default)]
    options: Vec<CrmPropertyOption>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubStage {
    id: String,
    label: String,
    display_order: i32,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct HubPipeline {
    id: String,
    label: String,
    display_order: i32,
    stages: Vec<HubStage>,
}

/// Sanitized errors: never forward upstream bodies, request headers or token-bearing URLs.
#[derive(Debug)]
pub struct MetadataError {
    pub status: StatusCode,
    pub code: &'static str,
}
impl IntoResponse for MetadataError {
    fn into_response(self) -> Response {
        (
            self.status,
            [(header::CACHE_CONTROL, "no-store")],
            Json(serde_json::json!({"code":self.code})),
        )
            .into_response()
    }
}
fn error(status: StatusCode, code: &'static str) -> MetadataError {
    MetadataError { status, code }
}

pub struct MetadataService {
    client: reqwest::Client,
    token: String,
    base_url: String,
    cache: Mutex<Option<(Instant, CrmMetadataResponse)>>,
}
impl MetadataService {
    pub fn new(token: String) -> Result<Self, MetadataError> {
        Self::with_base_url(token, "https://api.hubapi.com".to_owned())
    }
    fn with_base_url(token: String, base_url: String) -> Result<Self, MetadataError> {
        if token.trim().is_empty() {
            return Err(error(
                StatusCode::SERVICE_UNAVAILABLE,
                "hubspot_not_configured",
            ));
        }
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(15))
            .connect_timeout(Duration::from_secs(5))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| {
                error(
                    StatusCode::SERVICE_UNAVAILABLE,
                    "hubspot_client_unavailable",
                )
            })?;
        Ok(Self {
            client,
            token,
            base_url,
            cache: Mutex::new(None),
        })
    }
    async fn read<T: serde::de::DeserializeOwned>(&self, path: &str) -> Result<T, MetadataError> {
        // Only fixed GET paths are used. At most one short retry, with bounded Retry-After.
        for attempt in 0..2 {
            let response = self
                .client
                .get(format!("{}{path}", self.base_url))
                .bearer_auth(&self.token)
                .send()
                .await
                .map_err(|e| {
                    if e.is_timeout() {
                        error(StatusCode::GATEWAY_TIMEOUT, "hubspot_timeout")
                    } else {
                        error(StatusCode::BAD_GATEWAY, "hubspot_connection_failed")
                    }
                })?;
            let status = response.status();
            if attempt == 0 && (status.as_u16() == 429 || status.is_server_error()) {
                let delay = response
                    .headers()
                    .get("retry-after")
                    .and_then(|h| h.to_str().ok())
                    .and_then(|s| s.parse::<u64>().ok())
                    .unwrap_or(1);
                if delay <= 2 {
                    tokio::time::sleep(Duration::from_secs(delay)).await;
                    continue;
                }
            }
            if !status.is_success() {
                return Err(match status.as_u16() {
                    401 => error(StatusCode::BAD_GATEWAY, "hubspot_auth_failed"),
                    403 => error(StatusCode::BAD_GATEWAY, "hubspot_scope_denied"),
                    404 => error(StatusCode::BAD_GATEWAY, "hubspot_endpoint_missing"),
                    429 => error(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
                    _ => error(StatusCode::BAD_GATEWAY, "hubspot_unavailable"),
                });
            }
            return response
                .json()
                .await
                .map_err(|_| error(StatusCode::BAD_GATEWAY, "hubspot_invalid_response"));
        }
        Err(error(StatusCode::BAD_GATEWAY, "hubspot_unavailable"))
    }
    pub async fn metadata(&self, refresh: bool) -> Result<CrmMetadataResponse, MetadataError> {
        let started = Instant::now();
        // Serialize misses so multiple tabs do not fan out into duplicate upstream loads.
        let mut cache = self.cache.lock().await;
        if !refresh {
            if let Some((stored, response)) = cache.as_ref() {
                if stored.elapsed() < Duration::from_secs(60) {
                    let mut response = response.clone();
                    response.cache_hit = true;
                    response.hubspot_ms = 0.0;
                    response.total_ms = started.elapsed().as_secs_f64() * 1000.0;
                    return Ok(response);
                }
            }
        }
        let upstream = Instant::now();
        let (contacts, companies, deals, pipelines) = tokio::try_join!(
            self.read::<Results<HubProperty>>("/crm/v3/properties/contacts"),
            self.read::<Results<HubProperty>>("/crm/v3/properties/companies"),
            self.read::<Results<HubProperty>>("/crm/v3/properties/deals"),
            self.read::<Results<HubPipeline>>("/crm/v3/pipelines/deals"),
        )?;
        let hubspot_ms = upstream.elapsed().as_secs_f64() * 1000.0;
        let mut properties = Vec::new();
        for (object_type, definitions) in [
            ("contacts", contacts.results),
            ("companies", companies.results),
            ("deals", deals.results),
        ] {
            // Expose the current MOC's reviewed field set, not all account metadata.
            properties.extend(
                definitions
                    .into_iter()
                    .filter(|p| allowed_property(object_type, &p.name))
                    .map(|p| CrmPropertyDefinition {
                        object_type: object_type.to_owned(),
                        name: p.name,
                        label: p.label,
                        property_type: p.property_type,
                        field_type: p.field_type,
                        options: p.options,
                    }),
            );
        }
        let mut pipelines = pipelines.results;
        pipelines.sort_by_key(|p| p.display_order);
        let pipelines = pipelines
            .into_iter()
            .map(|mut p| {
                p.stages.sort_by_key(|s| s.display_order);
                CrmPipeline {
                    id: p.id,
                    label: p.label,
                    stages: p
                        .stages
                        .into_iter()
                        .map(|s| CrmStage {
                            id: s.id,
                            label: s.label,
                        })
                        .collect(),
                }
            })
            .collect();
        let response = CrmMetadataResponse {
            properties,
            pipelines,
            fetched_at: chrono::Utc::now().to_rfc3339(),
            hubspot_ms,
            total_ms: started.elapsed().as_secs_f64() * 1000.0,
            cache_hit: false,
        };
        *cache = Some((Instant::now(), response.clone()));
        Ok(response)
    }
}
fn allowed_property(object_type: &str, name: &str) -> bool {
    let names: &[&str] = match object_type {
        "contacts" => &[
            "firstname",
            "lastname",
            "email",
            "phone",
            "jobtitle",
            "hubspot_owner_id",
            "hs_lead_status",
            "lifecyclestage",
            "notes_last_contacted",
        ],
        "companies" => &[
            "name",
            "domain",
            "phone",
            "industry",
            "city",
            "numberofemployees",
            "hubspot_owner_id",
            "lifecyclestage",
        ],
        "deals" => &[
            "pipeline",
            "dealstage",
            "bpo_10",
            "bpo_14",
            "bpo_13",
            "bpo_16",
            "bpo_40",
            "bpo_42",
            "bpo_45",
            "bpo_21",
            "bpo_22",
            "bpo_50",
            "bpo_24",
            "bpo_49",
            "bpo_34",
            "bpo_25",
            "bpo_8",
            "bpo_23",
            "bpo__",
            "bpo_33",
            "bpo_3",
            "bpo_4",
            "bpo_18",
            "bpo_19",
            "bpo_32",
        ],
        _ => &[],
    };
    names.contains(&name)
}
fn authorize(
    email: Option<&str>,
    method: Option<&str>,
    allowed: &HashSet<String>,
) -> Result<(), MetadataError> {
    let email = email
        .filter(|s| !s.is_empty())
        .ok_or_else(|| error(StatusCode::UNAUTHORIZED, "login_required"))?;
    if method != Some(LOGIN_METHOD_GOOGLE_OIDC) {
        return Err(error(StatusCode::FORBIDDEN, "google_login_required"));
    }
    if !allowed.contains(&email.to_lowercase()) {
        return Err(error(StatusCode::FORBIDDEN, "crm_metadata_access_denied"));
    }
    Ok(())
}
#[derive(Deserialize, Default)]
struct MetadataQuery {
    #[serde(default)]
    refresh: bool,
}

/// Merge outside the shared redirecting auth layer: this API emits JSON 401/403 itself.
pub fn router() -> Router<Arc<AppState>> {
    let allowed: Arc<HashSet<String>> = Arc::new(
        std::env::var("CRM_METADATA_ALLOWED_EMAILS")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_lowercase())
            .filter(|s| !s.is_empty())
            .collect(),
    );
    let service = Arc::new(
        std::env::var("HUBSPOT_ACCESS_TOKEN")
            .ok()
            .and_then(|token| MetadataService::new(token).ok()),
    );
    Router::new().route(
        "/api/crm/metadata",
        get(
            move |State(state): State<Arc<AppState>>,
                  session: Session,
                  Query(query): Query<MetadataQuery>| {
                let allowed = allowed.clone();
                let service = service.clone();
                async move {
                    let email: Option<String> =
                        session.get(SESSION_USER_KEY).await.map_err(|_| {
                            error(StatusCode::INTERNAL_SERVER_ERROR, "session_unavailable")
                        })?;
                    let method: Option<String> =
                        session.get(SESSION_LOGIN_METHOD_KEY).await.map_err(|_| {
                            error(StatusCode::INTERNAL_SERVER_ERROR, "session_unavailable")
                        })?;
                    authorize(email.as_deref(), method.as_deref(), &allowed)?;
                    if crate::account_is_disabled(&state, email.as_deref().unwrap_or_default())
                        .await
                    {
                        return Err(error(StatusCode::FORBIDDEN, "account_disabled"));
                    }
                    let service = service.as_ref().as_ref().ok_or_else(|| {
                        error(StatusCode::SERVICE_UNAVAILABLE, "hubspot_not_configured")
                    })?;
                    let response = service.metadata(query.refresh).await?;
                    Ok::<_, MetadataError>(([(header::CACHE_CONTROL, "no-store")], Json(response)))
                }
            },
        ),
    )
}

#[cfg(test)]
mod tests;
