//! Temporary A/B seam. Replace this module with Codex B's tracking implementation.
//! This PR intentionally sends unchanged HTML until B is merged.
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;

pub trait TrackingStore: Send + Sync {}
impl TrackingStore for crate::audit::AuditDb {}

pub struct TrackingMeta {
    pub operator_email: String,
    pub hubspot_deal_id: String,
    pub hubspot_contact_id: String,
    pub subject: String,
}
pub struct TrackedEmail {
    pub tracking_id: String,
    pub html: String,
}
#[derive(Debug)]
pub struct TrackingError;

pub async fn instrument_html(
    _store: &dyn TrackingStore,
    _base_url: &str,
    html: &str,
    _meta: TrackingMeta,
) -> Result<TrackedEmail, TrackingError> {
    let mut id = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut id);
    Ok(TrackedEmail {
        tracking_id: URL_SAFE_NO_PAD.encode(id),
        html: html.into(),
    })
}
pub async fn bind_sent(
    _store: &dyn TrackingStore,
    _tracking_id: &str,
    _hubspot_email_id: Option<&str>,
    _gmail_message_id: &str,
) -> Result<(), TrackingError> {
    Ok(())
}
