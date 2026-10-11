//! Email engagement measurements owned by HR_HR (HubSpot counters are read-only).
//! Public handlers accept only opaque IDs; destinations always come from the store.
use std::{
    collections::HashMap,
    future::Future,
    pin::Pin,
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Utc};
use lol_html::{element, html_content::ContentType, rewrite_str, RewriteStrSettings};
use rand::RngCore;
use serde::{Deserialize, Serialize};
use tower_sessions::Session;
use ts_rs::TS;

use crate::{
    audit::AuditDb,
    crm::rbac::{self, CrmAccess},
    handlers::helpers::{get_i64, get_str},
    AppState,
};

pub type TrackingFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, TrackingError>> + Send + 'a>>;
#[derive(Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackingError {
    InvalidInput,
    Html,
    Storage,
    BindConflict,
    Busy,
}
impl std::fmt::Display for TrackingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "email tracking: {self:?}")
    }
}
impl std::error::Error for TrackingError {}
#[derive(Clone, Serialize, Deserialize)]
pub struct TrackingLink {
    pub link_no: i64,
    pub url: String,
}
#[derive(Clone)]
pub struct SentEmail {
    pub tracking_id: String,
    pub meta: TrackingMeta,
    pub created_at: i64,
    pub sent_at: Option<i64>,
}
#[derive(Clone)]
pub struct TrackingEvent {
    pub tracking_id: String,
    pub kind: &'static str,
    pub link_no: i64,
    pub at: i64,
    pub automatic: bool,
}

/// Object-safe interface shared with the sending PR. Implementations must not log metadata/URLs.
pub trait TrackingStore: Send + Sync {
    fn save<'a>(&'a self, email: SentEmail, links: Vec<TrackingLink>) -> TrackingFuture<'a, ()>;
    fn bind<'a>(
        &'a self,
        tracking_id: &'a str,
        hubspot_email_id: Option<&'a str>,
        gmail_message_id: &'a str,
    ) -> TrackingFuture<'a, ()>;
    fn lookup<'a>(&'a self, tracking_id: &'a str) -> TrackingFuture<'a, Option<SentEmail>>;
    fn destination<'a>(
        &'a self,
        tracking_id: &'a str,
        link_no: i64,
    ) -> TrackingFuture<'a, Option<String>>;
    fn record<'a>(&'a self, event: TrackingEvent) -> TrackingFuture<'a, ()>;
    fn summary<'a>(
        &'a self,
        hubspot_email_id: &'a str,
    ) -> TrackingFuture<'a, Option<EmailTrackingSummary>>;
}

fn valid_id(id: &str) -> bool {
    id.len() == 32 && id.bytes().all(|c| c.is_ascii_hexdigit())
}
fn valid_url(raw: &str) -> Option<reqwest::Url> {
    if raw.len() > 8192 || raw.chars().any(char::is_control) {
        return None;
    }
    let u = reqwest::Url::parse(raw).ok()?;
    (matches!(u.scheme(), "http" | "https")
        && u.host_str().is_some()
        && u.username().is_empty()
        && u.password().is_none())
    .then_some(u)
}

pub async fn instrument_html(
    store: &dyn TrackingStore,
    base_url: &str,
    html: &str,
    meta: TrackingMeta,
) -> Result<TrackedEmail, TrackingError> {
    let base = valid_url(base_url).ok_or(TrackingError::InvalidInput)?;
    if base.query().is_some()
        || base.fragment().is_some()
        || base.path() != "/"
        || html.len() > 1_000_000
        || meta.operator_email.is_empty()
        || meta.subject.len() > 2000
    {
        return Err(TrackingError::InvalidInput);
    }
    let base = base.as_str().trim_end_matches('/');
    let mut bytes = [0u8; 16];
    rand::rngs::OsRng.fill_bytes(&mut bytes);
    let tracking_id: String = bytes.iter().map(|v| format!("{v:02x}")).collect();
    let mut links = Vec::new();
    let pixel = format!(
        r#"<img src="{base}/t/o/{tracking_id}.gif" width="1" height="1" alt="" style="display:none" data-hr-email-open="1">"#
    );
    let mut inserted = false;
    let mut output = rewrite_str(
        html,
        RewriteStrSettings {
            element_content_handlers: vec![
                element!("a[href]", |el| {
                    let href = el
                        .get_attribute("data-hr-email-url")
                        .or_else(|| el.get_attribute("href"))
                        .unwrap_or_default();
                    if let Some(url) =
                        valid_url(html_escape::decode_html_entities(href.trim()).as_ref())
                    {
                        if links.len() >= 100 {
                            return Err("too many email links".into());
                        }
                        let no = links.len() as i64 + 1;
                        el.set_attribute("data-hr-email-url", url.as_str())?;
                        el.set_attribute("href", &format!("{base}/t/c/{tracking_id}/{no}"))?;
                        links.push(TrackingLink {
                            link_no: no,
                            url: url.to_string(),
                        });
                    }
                    Ok(())
                }),
                // Re-instrumented/quoted messages must never keep an old measurement pixel.
                element!("body", |el| {
                    el.append(&pixel, ContentType::Html);
                    inserted = true;
                    Ok(())
                }),
                element!("img[data-hr-email-open]", |el| {
                    el.remove();
                    Ok(())
                }),
            ],
            ..Default::default()
        },
    )
    .map_err(|_| TrackingError::Html)?;
    if !inserted {
        output.push_str(&pixel);
    }
    store
        .save(
            SentEmail {
                tracking_id: tracking_id.clone(),
                meta,
                created_at: Utc::now().timestamp_millis(),
                sent_at: None,
            },
            links,
        )
        .await?;
    Ok(TrackedEmail {
        tracking_id,
        html: output,
    })
}

pub async fn bind_sent(
    store: &dyn TrackingStore,
    tracking_id: &str,
    hubspot_email_id: Option<&str>,
    gmail_message_id: &str,
) -> Result<(), TrackingError> {
    if !valid_id(tracking_id)
        || gmail_message_id.is_empty()
        || gmail_message_id.len() > 256
        || hubspot_email_id.is_some_and(|id| {
            id.is_empty() || id.len() > 20 || !id.bytes().all(|c| c.is_ascii_digit())
        })
    {
        return Err(TrackingError::InvalidInput);
    }
    store
        .bind(tracking_id, hubspot_email_id, gmail_message_id)
        .await
}

#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq, Default)]
pub struct TrackingCounts {
    #[ts(type = "number")]
    pub count: i64,
    pub first_at: Option<String>,
    pub last_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct LinkTrackingSummary {
    #[ts(type = "number")]
    pub link_no: i64,
    pub url: String,
    pub clicks: TrackingCounts,
    pub automatic_clicks: TrackingCounts,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS, PartialEq, Eq)]
pub struct EmailTrackingSummary {
    pub opens: TrackingCounts,
    pub automatic_opens: TrackingCounts,
    pub clicks: TrackingCounts,
    pub automatic_clicks: TrackingCounts,
    pub links: Vec<LinkTrackingSummary>,
}

pub const TRACKING_DDL: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS crm_email_tracking (tracking_id TEXT PRIMARY KEY, operator_email TEXT NOT NULL, hubspot_deal_id TEXT, hubspot_contact_id TEXT, subject TEXT NOT NULL, hubspot_email_id TEXT UNIQUE, gmail_message_id TEXT, created_at INTEGER NOT NULL, sent_at INTEGER)",
    "CREATE TABLE IF NOT EXISTS crm_email_tracking_links (tracking_id TEXT NOT NULL, link_no INTEGER NOT NULL, url TEXT NOT NULL, PRIMARY KEY(tracking_id,link_no))",
    // One measured event per minute, per link and automatic/non-automatic category.
    "CREATE TABLE IF NOT EXISTS crm_email_tracking_events (tracking_id TEXT NOT NULL, kind TEXT NOT NULL CHECK(kind IN ('open','click')), link_no INTEGER NOT NULL DEFAULT 0, at INTEGER NOT NULL, automatic INTEGER NOT NULL CHECK(automatic IN (0,1)), minute INTEGER NOT NULL, PRIMARY KEY(tracking_id,kind,link_no,automatic,minute))",
];
pub fn ensure_tables(db: &crate::db::turso_http::TursoDb) -> Result<(), String> {
    for sql in TRACKING_DDL {
        db.execute(sql, &[])?;
    }
    Ok(())
}

/// Uses the existing audit DB. A bounded blocking pool also limits work after HTTP cancellation.
#[derive(Clone)]
pub struct AuditTrackingStore {
    audit: AuditDb,
    slots: Arc<tokio::sync::Semaphore>,
}
static STORE_SLOTS: OnceLock<Arc<tokio::sync::Semaphore>> = OnceLock::new();
impl AuditTrackingStore {
    pub fn new(audit: AuditDb) -> Self {
        Self {
            audit,
            slots: STORE_SLOTS
                .get_or_init(|| Arc::new(tokio::sync::Semaphore::new(8)))
                .clone(),
        }
    }
    async fn sql<T: Send + 'static>(
        &self,
        f: impl FnOnce(&crate::db::turso_http::TursoDb) -> Result<T, String> + Send + 'static,
    ) -> Result<T, TrackingError> {
        let permit = self
            .slots
            .clone()
            .try_acquire_owned()
            .map_err(|_| TrackingError::Busy)?;
        let audit = self.audit.clone();
        tokio::task::spawn_blocking(move || {
            let _permit = permit;
            f(audit.turso())
        })
        .await
        .map_err(|_| TrackingError::Storage)?
        .map_err(|_| TrackingError::Storage)
    }
}
fn sent_from_row(r: &HashMap<String, serde_json::Value>) -> SentEmail {
    SentEmail {
        tracking_id: get_str(r, "tracking_id"),
        created_at: get_i64(r, "created_at"),
        sent_at: r.get("sent_at").and_then(|v| v.as_i64()),
        meta: TrackingMeta {
            operator_email: get_str(r, "operator_email"),
            hubspot_deal_id: get_str(r, "hubspot_deal_id"),
            hubspot_contact_id: get_str(r, "hubspot_contact_id"),
            subject: get_str(r, "subject"),
        },
    }
}
fn iso_ms(ms: i64) -> Option<String> {
    DateTime::from_timestamp_millis(ms)
        .map(|t| t.to_rfc3339_opts(chrono::SecondsFormat::Millis, true))
}
fn counts(
    rows: &[HashMap<String, serde_json::Value>],
    kind: &str,
    link_no: Option<i64>,
    automatic: bool,
) -> TrackingCounts {
    let selected: Vec<_> = rows
        .iter()
        .filter(|r| {
            get_str(r, "kind") == kind
                && get_i64(r, "automatic") == i64::from(automatic)
                && link_no.is_none_or(|n| get_i64(r, "link_no") == n)
        })
        .collect();
    TrackingCounts {
        count: selected.iter().map(|r| get_i64(r, "count")).sum(),
        first_at: selected
            .iter()
            .map(|r| get_i64(r, "first_at"))
            .min()
            .and_then(iso_ms),
        last_at: selected
            .iter()
            .map(|r| get_i64(r, "last_at"))
            .max()
            .and_then(iso_ms),
    }
}
impl TrackingStore for AuditTrackingStore {
    fn save<'a>(&'a self, e: SentEmail, links: Vec<TrackingLink>) -> TrackingFuture<'a, ()> {
        Box::pin(async move {
            self.sql(move |db| {
            let deal = e.meta.hubspot_deal_id; let contact = e.meta.hubspot_contact_id;
            db.execute("INSERT INTO crm_email_tracking(tracking_id,operator_email,hubspot_deal_id,hubspot_contact_id,subject,created_at) VALUES(?,?,NULLIF(?,''),NULLIF(?,''),?,?)", &[&e.tracking_id,&e.meta.operator_email,&deal,&contact,&e.meta.subject,&e.created_at])?;
            let json = serde_json::to_string(&links).map_err(|_| "links serialization".to_string())?;
            db.execute("INSERT INTO crm_email_tracking_links(tracking_id,link_no,url) SELECT ?,json_extract(value,'$.link_no'),json_extract(value,'$.url') FROM json_each(?)", &[&e.tracking_id,&json])
        }).await
        })
    }
    fn bind<'a>(
        &'a self,
        id: &'a str,
        hs: Option<&'a str>,
        gmail: &'a str,
    ) -> TrackingFuture<'a, ()> {
        Box::pin(async move {
            let (id, hs, gmail) = (
                id.to_string(),
                hs.unwrap_or_default().to_string(),
                gmail.to_string(),
            );
            let matches = self.sql(move |db| {
                db.execute("UPDATE crm_email_tracking SET hubspot_email_id=COALESCE(NULLIF(?,''),hubspot_email_id),gmail_message_id=?,sent_at=COALESCE(sent_at,?) WHERE tracking_id=? AND (gmail_message_id IS NULL OR gmail_message_id=?) AND (hubspot_email_id IS NULL OR ?='' OR hubspot_email_id=?)", &[&hs,&gmail,&Utc::now().timestamp_millis(),&id,&gmail,&hs,&hs])?;
                let rows = db.query("SELECT gmail_message_id,hubspot_email_id FROM crm_email_tracking WHERE tracking_id=?", &[&id])?;
                Ok(rows.first().is_some_and(|r| get_str(r,"gmail_message_id") == gmail && (hs.is_empty() || get_str(r,"hubspot_email_id") == hs)))
            }).await?;
            if matches {
                Ok(())
            } else {
                Err(TrackingError::BindConflict)
            }
        })
    }
    fn lookup<'a>(&'a self, id: &'a str) -> TrackingFuture<'a, Option<SentEmail>> {
        Box::pin(async move {
            let id = id.to_string();
            self.sql(move |db| {
                Ok(db
                    .query(
                        "SELECT * FROM crm_email_tracking WHERE tracking_id=?",
                        &[&id],
                    )?
                    .first()
                    .map(sent_from_row))
            })
            .await
        })
    }
    fn destination<'a>(&'a self, id: &'a str, no: i64) -> TrackingFuture<'a, Option<String>> {
        Box::pin(async move {
            let id = id.to_string();
            self.sql(move |db| Ok(db.query("SELECT url FROM crm_email_tracking_links WHERE tracking_id=? AND link_no=?", &[&id,&no])?.first().map(|r| get_str(r,"url")))).await
        })
    }
    fn record<'a>(&'a self, e: TrackingEvent) -> TrackingFuture<'a, ()> {
        Box::pin(async move {
            self.sql(move |db| db.execute("INSERT OR IGNORE INTO crm_email_tracking_events(tracking_id,kind,link_no,at,automatic,minute) SELECT ?,?,?,?,?,? WHERE EXISTS(SELECT 1 FROM crm_email_tracking WHERE tracking_id=?) AND (?='open' OR EXISTS(SELECT 1 FROM crm_email_tracking_links WHERE tracking_id=? AND link_no=?))", &[&e.tracking_id,&e.kind,&e.link_no,&e.at,&i64::from(e.automatic),&(e.at/60_000),&e.tracking_id,&e.kind,&e.tracking_id,&e.link_no])).await
        })
    }
    fn summary<'a>(&'a self, hs: &'a str) -> TrackingFuture<'a, Option<EmailTrackingSummary>> {
        Box::pin(async move {
            let hs = hs.to_string();
            self.sql(move |db| {
            let Some(email) = db.query("SELECT tracking_id FROM crm_email_tracking WHERE hubspot_email_id=?", &[&hs])?.into_iter().next() else { return Ok(None); };
            let id=get_str(&email,"tracking_id");
            let rows=db.query("SELECT kind,link_no,automatic,COUNT(*) AS count,MIN(at) AS first_at,MAX(at) AS last_at FROM crm_email_tracking_events WHERE tracking_id=? GROUP BY kind,link_no,automatic", &[&id])?;
            let links=db.query("SELECT link_no,url FROM crm_email_tracking_links WHERE tracking_id=? ORDER BY link_no", &[&id])?.iter().map(|r| { let n=get_i64(r,"link_no"); LinkTrackingSummary { link_no:n,url:get_str(r,"url"),clicks:counts(&rows,"click",Some(n),false),automatic_clicks:counts(&rows,"click",Some(n),true) } }).collect();
            Ok(Some(EmailTrackingSummary { opens:counts(&rows,"open",None,false),automatic_opens:counts(&rows,"open",None,true),clicks:counts(&rows,"click",None,false),automatic_clicks:counts(&rows,"click",None,true),links }))
        }).await
        })
    }
}

// A uses &AuditDb at the seam. Forward through the same bounded adapter so merging B
// requires no edits to A's sending files or to the established function signatures.
impl TrackingStore for AuditDb {
    fn save<'a>(&'a self, e: SentEmail, links: Vec<TrackingLink>) -> TrackingFuture<'a, ()> {
        Box::pin(async move { AuditTrackingStore::new(self.clone()).save(e, links).await })
    }
    fn bind<'a>(
        &'a self,
        id: &'a str,
        hs: Option<&'a str>,
        gmail: &'a str,
    ) -> TrackingFuture<'a, ()> {
        Box::pin(async move {
            AuditTrackingStore::new(self.clone())
                .bind(id, hs, gmail)
                .await
        })
    }
    fn lookup<'a>(&'a self, id: &'a str) -> TrackingFuture<'a, Option<SentEmail>> {
        Box::pin(async move { AuditTrackingStore::new(self.clone()).lookup(id).await })
    }
    fn destination<'a>(&'a self, id: &'a str, no: i64) -> TrackingFuture<'a, Option<String>> {
        Box::pin(async move {
            AuditTrackingStore::new(self.clone())
                .destination(id, no)
                .await
        })
    }
    fn record<'a>(&'a self, event: TrackingEvent) -> TrackingFuture<'a, ()> {
        Box::pin(async move { AuditTrackingStore::new(self.clone()).record(event).await })
    }
    fn summary<'a>(&'a self, id: &'a str) -> TrackingFuture<'a, Option<EmailTrackingSummary>> {
        Box::pin(async move { AuditTrackingStore::new(self.clone()).summary(id).await })
    }
}

pub fn enabled_from_env() -> bool {
    std::env::var("CRM_EMAIL_ENABLED").ok().as_deref() == Some("1")
        && std::env::var("CRM_EMAIL_TOKEN_KEY")
            .ok()
            .and_then(|key| STANDARD.decode(key).ok())
            .is_some_and(|key| key.len() == 32)
}

/// Heuristic only: do not claim that a person opened an image or clicked a URL.
/// Raw IP, User-Agent and Referer are never persisted.
pub fn automatic_hint(headers: &HeaderMap, at: i64, sent_at: i64) -> bool {
    let ua = headers
        .get(header::USER_AGENT)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_ascii_lowercase();
    at.saturating_sub(sent_at) < 10_000
        || [
            "proofpoint",
            "mimecast",
            "barracuda",
            "safelinks",
            "microsoft office existence discovery",
            "applemail",
            "mailprivacy",
            "prefetch",
            "headless",
            "bot",
            "crawler",
            "spider",
        ]
        .iter()
        .any(|s| ua.contains(s))
        || ["purpose", "sec-purpose", "x-purpose"].iter().any(|k| {
            headers
                .get(*k)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.to_ascii_lowercase().contains("prefetch"))
        })
}

struct Window {
    at: Instant,
    used: usize,
}
impl Window {
    fn take(&mut self, now: Instant, max: usize) -> bool {
        if now.saturating_duration_since(self.at) >= Duration::from_secs(60) {
            self.at = now;
            self.used = 0;
        }
        if self.used >= max {
            return false;
        }
        self.used += 1;
        true
    }
}
struct Limits {
    events: HashMap<(String, &'static str, i64, bool), Instant>,
    writes: Window,
    reads: Window,
}
type DestinationCache = HashMap<(String, i64), (Instant, Option<String>)>;
pub struct TrackingContext {
    store: Option<Arc<dyn TrackingStore>>,
    access: CrmAccess,
    limits: Mutex<Limits>,
    cache: Mutex<HashMap<String, (Instant, Option<SentEmail>)>>,
    destinations: Mutex<DestinationCache>,
}
impl TrackingContext {
    pub fn new(store: Option<Arc<dyn TrackingStore>>, access: CrmAccess) -> Self {
        let now = Instant::now();
        Self {
            store,
            access,
            limits: Mutex::new(Limits {
                events: HashMap::new(),
                writes: Window { at: now, used: 0 },
                reads: Window { at: now, used: 0 },
            }),
            cache: Mutex::new(HashMap::new()),
            destinations: Mutex::new(HashMap::new()),
        }
    }
    async fn lookup(&self, id: &str) -> Option<SentEmail> {
        if !valid_id(id) {
            return None;
        }
        let now = Instant::now();
        if let Some((at, value)) = self.cache.lock().ok()?.get(id) {
            if now.saturating_duration_since(*at) < Duration::from_secs(60) {
                return value.clone();
            }
        }
        if !self.limits.lock().ok()?.reads.take(now, 600) {
            return None;
        }
        let value = self.store.as_ref()?.lookup(id).await.ok()?;
        if let Ok(mut cache) = self.cache.lock() {
            cache.retain(|_, (at, _)| now.saturating_duration_since(*at) < Duration::from_secs(60));
            if cache.len() < 2000 && value.as_ref().is_none_or(|e| e.sent_at.is_some()) {
                cache.insert(id.to_string(), (now, value.clone()));
            }
        }
        value
    }
    async fn destination(&self, id: &str, no: i64) -> Option<String> {
        let now = Instant::now();
        let key = (id.to_string(), no);
        if let Some((at, value)) = self.destinations.lock().ok()?.get(&key) {
            if now.saturating_duration_since(*at) < Duration::from_secs(60) {
                return value.clone();
            }
        }
        if !self.limits.lock().ok()?.reads.take(now, 600) {
            return None;
        }
        let value = self.store.as_ref()?.destination(id, no).await.ok()?;
        if let Ok(mut cache) = self.destinations.lock() {
            cache.retain(|_, (at, _)| now.saturating_duration_since(*at) < Duration::from_secs(60));
            if cache.len() < 2000 {
                cache.insert(key, (now, value.clone()));
            }
        }
        value
    }
    async fn record(&self, e: TrackingEvent) {
        let Some(store) = &self.store else {
            return;
        };
        let now = Instant::now();
        let allowed = if let Ok(mut limits) = self.limits.lock() {
            limits
                .events
                .retain(|_, at| now.saturating_duration_since(*at) < Duration::from_secs(60));
            let key = (e.tracking_id.clone(), e.kind, e.link_no, e.automatic);
            if limits.events.contains_key(&key) || !limits.writes.take(now, 120) {
                false
            } else {
                limits.events.insert(key, now);
                true
            }
        } else {
            false
        };
        if allowed && store.record(e).await.is_err() {
            tracing::warn!("email tracking event could not be saved");
        }
    }
}

/// Sending PR owns encryption-key validation; missing keys disable measurements as well.
pub fn configured_store(state: &AppState) -> Option<Arc<dyn TrackingStore>> {
    if !enabled_from_env() {
        return None;
    }
    state
        .audit
        .clone()
        .map(|audit| Arc::new(AuditTrackingStore::new(audit)) as Arc<dyn TrackingStore>)
}
pub fn router_for_state(state: &AppState) -> Router<Arc<AppState>> {
    router(Arc::new(TrackingContext::new(
        configured_store(state),
        CrmAccess::from_env(),
    )))
}
pub fn router(ctx: Arc<TrackingContext>) -> Router<Arc<AppState>> {
    Router::new()
        .route("/t/o/{file}", get(open))
        .route("/t/c/{tracking_id}/{link_no}", get(click))
        .route("/api/crm/emails/{hubspot_email_id}/tracking", get(summary))
        .layer(Extension(ctx))
}
const GIF: &[u8] = &[
    71, 73, 70, 56, 57, 97, 1, 0, 1, 0, 128, 0, 0, 0, 0, 0, 255, 255, 255, 33, 249, 4, 1, 0, 0, 0,
    0, 44, 0, 0, 0, 0, 1, 0, 1, 0, 0, 2, 2, 68, 1, 0, 59,
];
fn no_store(mut r: Response) -> Response {
    r.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    r.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    r
}
async fn open(
    Extension(ctx): Extension<Arc<TrackingContext>>,
    Path(file): Path<String>,
    headers: HeaderMap,
) -> Response {
    if let Some(id) = file.strip_suffix(".gif") {
        if let Some(e) = ctx.lookup(id).await {
            let at = Utc::now().timestamp_millis();
            let automatic = automatic_hint(&headers, at, e.sent_at.unwrap_or(e.created_at));
            ctx.record(TrackingEvent {
                tracking_id: e.tracking_id,
                kind: "open",
                link_no: 0,
                at,
                automatic,
            })
            .await;
        }
    }
    no_store(([(header::CONTENT_TYPE, "image/gif")], GIF).into_response())
}
async fn click(
    Extension(ctx): Extension<Arc<TrackingContext>>,
    Path((id, no)): Path<(String, i64)>,
    headers: HeaderMap,
) -> Response {
    let Some(e) = ctx.lookup(&id).await else {
        return no_store(StatusCode::NOT_FOUND.into_response());
    };
    if !(1..=100).contains(&no) {
        return no_store(StatusCode::NOT_FOUND.into_response());
    }
    let Some(url) = ctx.destination(&id, no).await else {
        return no_store(StatusCode::NOT_FOUND.into_response());
    };
    let Some(url) = valid_url(&url) else {
        return no_store(StatusCode::NOT_FOUND.into_response());
    };
    let Ok(location) = HeaderValue::from_str(url.as_str()) else {
        return no_store(StatusCode::NOT_FOUND.into_response());
    };
    let at = Utc::now().timestamp_millis();
    let automatic = automatic_hint(&headers, at, e.sent_at.unwrap_or(e.created_at));
    ctx.record(TrackingEvent {
        tracking_id: id,
        kind: "click",
        link_no: no,
        at,
        automatic,
    })
    .await;
    no_store((StatusCode::FOUND, [(header::LOCATION, location)]).into_response())
}
async fn summary(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<TrackingContext>>,
    Path(id): Path<String>,
) -> Response {
    if let Err(denied) = rbac::authorize(&session, &state, &ctx.access, None).await {
        return denied.into_response();
    }
    if id.is_empty() || id.len() > 20 || !id.bytes().all(|v| v.is_ascii_digit()) {
        return no_store(StatusCode::BAD_REQUEST.into_response());
    }
    let Some(store) = &ctx.store else {
        return no_store(StatusCode::NO_CONTENT.into_response());
    };
    match store.summary(&id).await {
        Ok(Some(v)) => no_store(Json(v).into_response()),
        Ok(None) => no_store(StatusCode::NO_CONTENT.into_response()),
        Err(_) => no_store(StatusCode::SERVICE_UNAVAILABLE.into_response()),
    }
}

#[cfg(test)]
mod tests;
