//! One-to-one email: Gmail sends, HubSpot owns history. Pending retries only record.
mod gmail;
mod grants;
mod oauth;
mod routes;
#[cfg(test)]
mod tests;
pub mod tracking;

use super::{
    pending::{self, OpRow},
    record_lock,
    write::WriteConfig,
};
use crate::{
    audit::AuditDb,
    config::GoogleOidcConfig,
    hubspot::{HubSpotClient, HubSpotError},
};
use base64::{engine::general_purpose::STANDARD, Engine};
use gmail::GmailMessage;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::Duration,
};
use ts_rs::TS;

pub const SCOPES: [&str; 2] = [
    "https://www.googleapis.com/auth/gmail.send",
    "https://www.googleapis.com/auth/gmail.readonly",
];
pub const CALLBACK: &str = "/api/crm/gmail/callback";

#[derive(Clone)]
pub struct EmailConfig {
    pub key: [u8; 32],
    pub google: GoogleOidcConfig,
    pub redirect_url: String,
    pub base_url: String,
    pub authorization_url: String,
    pub token_url: String,
    pub revoke_url: String,
    pub gmail_url: String,
}
impl EmailConfig {
    pub fn from_env() -> Option<Self> {
        if std::env::var("CRM_EMAIL_ENABLED").ok().as_deref() != Some("1") {
            return None;
        }
        let key: [u8; 32] = STANDARD
            .decode(std::env::var("CRM_EMAIL_TOKEN_KEY").ok()?)
            .ok()?
            .try_into()
            .ok()?;
        let redirect_url = std::env::var("CRM_EMAIL_REDIRECT_URL").ok()?;
        let url = reqwest::Url::parse(&redirect_url).ok()?;
        if url.path() != CALLBACK
            || url.query().is_some()
            || url.fragment().is_some()
            || (url.scheme() != "https"
                && !(url.scheme() == "http"
                    && matches!(url.host_str(), Some("localhost" | "127.0.0.1"))))
        {
            return None;
        }
        let base_url = url.origin().ascii_serialization();
        Some(Self {
            key,
            google: GoogleOidcConfig::from_env()?,
            redirect_url,
            base_url,
            authorization_url: "https://accounts.google.com/o/oauth2/v2/auth".into(),
            token_url: "https://oauth2.googleapis.com/token".into(),
            revoke_url: "https://oauth2.googleapis.com/revoke".into(),
            gmail_url: "https://gmail.googleapis.com/gmail/v1".into(),
        })
    }
}
pub struct EmailService {
    pub cfg: EmailConfig,
    http: reqwest::Client,
    transactions: Mutex<HashMap<String, oauth::GrantTx>>,
    pub locks: Arc<record_lock::RecordLocks>,
}
impl std::fmt::Debug for EmailService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("EmailService { credentials: [redacted] }")
    }
}
impl EmailService {
    pub fn new(cfg: EmailConfig) -> Self {
        Self {
            cfg,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(15))
                .build()
                .unwrap_or_default(),
            transactions: Mutex::new(HashMap::new()),
            locks: record_lock::shared(),
        }
    }
    pub fn from_env() -> Option<Self> {
        EmailConfig::from_env().map(Self::new)
    }
    pub async fn refresh_token(
        &self,
        audit: &AuditDb,
        operator: &str,
    ) -> Result<String, EmailError> {
        let who = operator.to_string();
        let encrypted = pending::blocking(audit, move |t| grants::get(t, &who))
            .await
            .map_err(|_| EmailError::Storage)?
            .map_err(|_| EmailError::Storage)?
            .ok_or(EmailError::GrantRequired)?;
        grants::decrypt(&self.cfg.key, operator, &encrypted)
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EmailError {
    Storage,
    GrantRequired,
    Invalid,
    Recipient,
    GmailUnavailable,
    GmailRejected,
    SendUncertain,
    Tracking,
}
impl EmailError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::Storage => "queue_unavailable",
            Self::GrantRequired => "gmail_grant_required",
            Self::Invalid => "invalid_email",
            Self::Recipient => "recipient_not_related",
            Self::GmailUnavailable => "gmail_unavailable",
            Self::GmailRejected => "gmail_send_failed",
            Self::SendUncertain => "gmail_send_uncertain",
            Self::Tracking => "tracking_unavailable",
        }
    }
    pub fn status(&self) -> u16 {
        match self {
            Self::GrantRequired | Self::Recipient => 403,
            Self::Invalid => 422,
            Self::GmailRejected => 502,
            Self::SendUncertain => 202,
            _ => 503,
        }
    }
}
#[derive(Clone, Deserialize, Serialize, TS)]
pub struct SendEmailRequest {
    pub operation_id: String,
    pub hubspot_deal_id: String,
    pub contact_id: String,
    pub to: String,
    #[serde(default)]
    pub cc: Vec<String>,
    pub subject: String,
    pub html: String,
}
/// Persist only until recorded. Terminal rows are reduced to durable deduplication receipts.
#[derive(Clone, Deserialize, Serialize, Debug, PartialEq)]
pub struct EmailOperation {
    pub contact_id: String,
    pub contact_ids: Vec<String>,
    pub to: String,
    pub cc: Vec<String>,
    pub subject: String,
    pub html: String,
    pub owner_id: String,
    pub tracking_id: String,
    pub requested_message_id: String,
    pub rfc_message_id: Option<String>,
    pub gmail: Option<GmailMessage>,
    pub hubspot_email_id: Option<String>,
    pub sent_at: String,
    /// sending / sent / rejected. Worker never sends, including interrupted `sending`.
    pub send_state: String,
    /// A lost CREATE response must be reconciled; a negative search is never permission to create again.
    #[serde(default)]
    pub create_uncertain: bool,
}
#[derive(Clone, Serialize, Deserialize, TS, Debug)]
pub struct EmailResult {
    pub operation_id: String,
    // saved (green) / pending (yellow) / failed (red)
    pub status: String,
    pub sent: bool,
    pub recorded: bool,
    pub error_kind: Option<String>,
    pub hubspot_email_id: Option<String>,
}
impl EmailResult {
    pub fn from_row(row: &OpRow) -> Self {
        let e = row.payload.email.as_ref();
        Self {
            operation_id: row.operation_id.clone(),
            status: if row.status == "in_progress" {
                "pending".into()
            } else {
                row.status.clone()
            },
            sent: e.is_some_and(|e| e.send_state == "sent"),
            recorded: row.status == "saved",
            error_kind: (!row.last_error_code.is_empty()).then(|| row.last_error_code.clone()),
            hubspot_email_id: e.and_then(|e| e.hubspot_email_id.clone()),
        }
    }
}
#[derive(Clone, Serialize, TS)]
pub struct GmailGrantStatus {
    pub enabled: bool,
    pub granted: bool,
}

pub fn valid_address(s: &str) -> bool {
    if s.len() > 254 || !s.is_ascii() || s.contains(['\r', '\n']) {
        return false;
    }
    let Some((local, domain)) = s.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && local.len() <= 64
        && !local.starts_with('.')
        && !local.ends_with('.')
        && !local.contains("..")
        && local
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".!#$%&'*+-/=?^_`{|}~".contains(c))
        && domain.contains('.')
        && domain.split('.').all(|part| {
            !part.is_empty()
                && !part.starts_with('-')
                && !part.ends_with('-')
                && part.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
}
pub fn validate(req: &SendEmailRequest) -> Result<(), EmailError> {
    if req.operation_id.len() < 8
        || req.operation_id.len() > 128
        || !req
            .operation_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        || !super::routes::is_valid_id(&req.hubspot_deal_id)
        || !super::routes::is_valid_id(&req.contact_id)
        || !valid_address(&req.to)
        || req.cc.len() > 20
        || req.cc.iter().any(|c| !valid_address(c))
        || req.subject.trim().is_empty()
        || req.subject.len() > 512
        || req.subject.contains(['\r', '\n'])
        || req.html.trim().is_empty()
        || req.html.len() > 131_072
    {
        return Err(EmailError::Invalid);
    }
    Ok(())
}
pub async fn persist(audit: &AuditDb, row: &OpRow) -> Result<(), EmailError> {
    let row = row.clone();
    pending::blocking(audit, move |t| {
        pending::update_op(
            t,
            &row.operation_id,
            &row.status,
            row.attempts,
            &row.last_error_code,
            row.http_status,
            &serde_json::to_string(&EmailResult::from_row(&row)).unwrap_or_default(),
            &row.next_retry_at,
            &row.payload,
        )
    })
    .await
    .map_err(|_| EmailError::Storage)?
    .map_err(|_| EmailError::Storage)
}
fn finish(row: &mut OpRow, status: &str, code: &str) {
    row.status = status.into();
    row.last_error_code = code.into();
    row.next_retry_at = if status == "pending" {
        pending::iso(chrono::Utc::now() + pending::backoff_after(row.attempts))
    } else {
        String::new()
    };
    row.http_status = match status {
        "saved" => 200,
        "pending" => 202,
        _ => 502,
    };
    if status == "saved"
        || row
            .payload
            .email
            .as_ref()
            .is_some_and(|e| e.send_state == "rejected")
    {
        // Keep IDs only: the receipt must outlive the normal seven-day PATCH retention.
        if let Some(e) = row.payload.email.as_mut() {
            e.html.clear();
            e.subject.clear();
            e.to.clear();
            e.cc.clear();
        }
    }
}
/// HubSpot record/association retry; no call to send_gmail exists in this path.
pub async fn record(
    audit: &AuditDb,
    client: &HubSpotClient,
    service: &EmailService,
    row: &mut OpRow,
) -> Result<(), EmailError> {
    let mut email = row.payload.email.clone().ok_or(EmailError::Invalid)?;
    if email.send_state == "rejected" {
        finish(row, "failed", "gmail_send_failed");
        return persist(audit, row).await;
    }
    row.attempts += 1;
    if email.gmail.is_none() || email.rfc_message_id.is_none() {
        let resolution = async {
            let token = service.refresh_token(audit, &row.operator_email).await?;
            let access = service.access_token(&token).await?;
            if email.gmail.is_none() {
                email.gmail = service
                    .find_sent(&access, &email.requested_message_id)
                    .await?;
            }
            let msg = email.gmail.as_ref().ok_or(EmailError::SendUncertain)?;
            email.rfc_message_id = Some(service.message_id(&access, &msg.id).await?);
            email.send_state = "sent".into();
            Ok::<_, EmailError>(())
        }
        .await;
        row.payload.email = Some(email.clone());
        if let Err(e) = resolution {
            finish(
                row,
                if row.attempts >= pending::MAX_ATTEMPTS {
                    "failed"
                } else {
                    "pending"
                },
                e.code(),
            );
            return persist(audit, row).await;
        }
        // Persist Gmail identity before any HubSpot CREATE.
        persist(audit, row).await?;
    }
    let outcome = async {
        if email.hubspot_email_id.is_none() {
            let rfc = email.rfc_message_id.clone().ok_or(HubSpotError::Decode("missing message id".into()))?;
            let found = client.search("emails", json!({"filterGroups":[{"filters":[{"propertyName":"hs_email_message_id","operator":"EQ","value":rfc}]}],"properties":["hs_email_message_id"],"limit":2})).await?;
            if let Some(id) = found["results"].as_array().and_then(|v| v.first()).and_then(|v| v["id"].as_str()) { email.hubspot_email_id = Some(id.into()); }
            else if email.create_uncertain { return Err(HubSpotError::Transport("create_unconfirmed".into())); }
            else {
                // Mark before CREATE. Crash/lost response may only search, never blindly create a second record.
                email.create_uncertain = true;
                row.payload.email = Some(email.clone());
                persist(audit, row).await.map_err(|_| HubSpotError::Transport("ledger_unavailable".into()))?;
                let properties = json!({"hs_email_direction":"EMAIL","hs_email_status":"SENT","hs_timestamp":email.sent_at,"hs_email_headers":json!({"from":{"email":row.operator_email},"to":[{"email":email.to}],"cc":email.cc.iter().map(|e|json!({"email":e})).collect::<Vec<_>>(),"bcc":[]}).to_string(),"hs_email_subject":email.subject,"hs_email_html":email.html,"hs_email_message_id":rfc,"hs_email_thread_id":email.gmail.as_ref().map(|m|&m.thread_id),"hubspot_owner_id":email.owner_id});
                match client.create_email(properties, &row.deal_id, &email.contact_ids).await {
                    Ok(id) => { email.hubspot_email_id = Some(id); email.create_uncertain = false; }
                    Err(e) => {
                        // A HTTP rejection did not create a record. Network/decode/5xx remains uncertain.
                        if matches!(e, HubSpotError::RateLimited | HubSpotError::Busy | HubSpotError::Auth {..} | HubSpotError::Upstream { status:400..=499 }) { email.create_uncertain = false; }
                        return Err(e);
                    }
                }
                row.payload.email = Some(email.clone());
                persist(audit, row).await.map_err(|_| HubSpotError::Transport("ledger_unavailable".into()))?;
            }
        }
        let id = email.hubspot_email_id.as_deref().ok_or(HubSpotError::Decode("missing email id".into()))?;
        // Idempotent PUT also repairs an interrupted association after a reconciled CREATE.
        client.associate_email(id, "deals", &row.deal_id, 210).await?;
        for contact in &email.contact_ids { client.associate_email(id, "contacts", contact, 198).await?; }
        tracking::bind_sent(audit, &email.tracking_id, Some(id), &email.gmail.as_ref().ok_or(HubSpotError::Decode("missing gmail id".into()))?.id).await.map_err(|_| HubSpotError::Transport("tracking_unavailable".into()))?;
        Ok::<_, HubSpotError>(())
    }.await;
    row.payload.email = Some(email);
    match outcome {
        Ok(()) => finish(row, "saved", ""),
        Err(e) => finish(
            row,
            if row.attempts >= pending::MAX_ATTEMPTS
                || matches!(
                    e,
                    HubSpotError::Auth { .. } | HubSpotError::Upstream { status: 400..=499 }
                )
            {
                "failed"
            } else {
                "pending"
            },
            e.error_kind(),
        ),
    }
    persist(audit, row).await?;
    Ok(())
}
pub async fn process_pending(
    audit: &AuditDb,
    client: &HubSpotClient,
    write: &WriteConfig,
    row: &OpRow,
) -> bool {
    let Some(service) = &write.email else {
        let mut stopped = row.clone();
        finish(&mut stopped, "failed", "email_disabled");
        return persist(audit, &stopped).await.is_ok();
    };
    let Ok(_guard) = service
        .locks
        .acquire(
            [record_lock::key("emails", &row.operation_id)],
            write.lock_wait,
        )
        .await
    else {
        return false;
    };
    let id = row.operation_id.clone();
    let Ok(Ok(Some(mut fresh))) = pending::blocking(audit, move |t| pending::get_op(t, &id)).await
    else {
        return false;
    };
    if !matches!(fresh.status.as_str(), "pending" | "in_progress") {
        return false;
    }
    let _ = record(audit, client, service, &mut fresh).await;
    true
}

pub(super) use oauth::{authorize, callback, revoke};
pub(super) use routes::{get_email, grant_status, list_emails, send_email};

fn write_wake() {
    super::write::wake_worker();
}

#[derive(Clone, Serialize, TS)]
pub struct PendingEmailActivity {
    pub result: EmailResult,
    pub subject: String,
    pub to: String,
    pub cc: Vec<String>,
    pub sent_at: String,
}
