//! Background-only reply import. Gmail is read only for IDs from our send ledger.
mod parser;
use super::{EmailError, EmailService};
use crate::{
    audit::AuditDb,
    crm::{
        pending::{self, OpRow},
        rbac,
        routes::{error_json, CrmCtx},
    },
    db::turso_http::TursoDb,
    handlers::helpers::get_str,
    hubspot::{HubSpotClient, HubSpotError},
    AppState,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{collections::HashSet, sync::Arc, time::Duration};
use tower_sessions::Session;
use ts_rs::TS;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct ReplyHeaders {
    pub from: String,
    pub to: Vec<String>,
    pub cc: Vec<String>,
}
#[derive(Clone, Serialize, TS)]
pub struct ReplySyncStatus {
    pub last_checked_at: Option<String>,
    pub last_imported_at: Option<String>,
    pub grant_expired: bool,
    #[ts(type = "number")]
    pub interval_seconds: u64,
}
pub fn interval_seconds() -> u64 {
    std::env::var("CRM_EMAIL_REPLY_INTERVAL_SECS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(600)
        .clamp(60, 86_400)
}
pub(super) fn sync_status(t: &TursoDb, who: &str) -> Result<ReplySyncStatus, String> {
    let rows = t.query("SELECT s.last_checked_at, s.last_imported_at, s.expired_grant_at, g.granted_at, g.revoked_at FROM crm_gmail_grants g LEFT JOIN crm_gmail_reply_status s USING(operator_email) WHERE g.operator_email = ?1", &[&who])?;
    let optional = |name| {
        rows.first()
            .map(|r| get_str(r, name))
            .filter(|s| !s.is_empty())
    };
    Ok(ReplySyncStatus {
        last_checked_at: optional("last_checked_at"),
        last_imported_at: optional("last_imported_at"),
        grant_expired: optional("revoked_at").is_some()
            || optional("expired_grant_at").is_some_and(|at| Some(at) == optional("granted_at")),
        interval_seconds: interval_seconds(),
    })
}
pub(in crate::crm) async fn status(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let p = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let Some(audit) = &state.audit else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let who = p.email.unwrap_or_default();
    match pending::blocking(audit, move |t| sync_status(t, &who)).await {
        Ok(Ok(s)) => Json(s).into_response(),
        _ => error_json(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable"),
    }
}
pub fn spawn_worker(state: Arc<AppState>) {
    let (Some(audit), Some(client), Some(service)) = (
        state.audit.clone(),
        state.hubspot.clone(),
        EmailService::from_env(),
    ) else {
        return;
    };
    let access = rbac::CrmAccess::from_env();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(30)).await;
        let mut timer = tokio::time::interval(Duration::from_secs(interval_seconds()));
        timer.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            timer.tick().await;
            if run_round(
                &audit,
                &client,
                &service,
                &state.config.allowed_domains,
                &access,
            )
            .await
            .is_err()
            {
                tracing::warn!("Gmail reply check incomplete");
            }
        }
    });
}
impl EmailService {
    async fn reply_get(
        &self,
        access: &str,
        resource: &str,
        id: &str,
        format: &str,
    ) -> Result<Value, EmailError> {
        if id.is_empty()
            || !id
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        {
            return Err(EmailError::Invalid);
        }
        let response = self
            .http
            .get(format!("{}/users/me/{resource}/{id}", self.cfg.gmail_url))
            .bearer_auth(access)
            .query(&[("format", format)])
            .send()
            .await
            .map_err(|_| EmailError::GmailUnavailable)?;
        match response.status().as_u16() {
            401 => Err(EmailError::GrantRequired),
            403 => {
                let body: Value = response.json().await.unwrap_or_default();
                let quota = body["error"]["errors"].as_array().is_some_and(|errors| {
                    errors.iter().any(|e| {
                        matches!(
                            e["reason"].as_str(),
                            Some(
                                "rateLimitExceeded"
                                    | "userRateLimitExceeded"
                                    | "quotaExceeded"
                                    | "dailyLimitExceeded"
                            )
                        )
                    })
                });
                Err(if quota {
                    EmailError::GmailUnavailable
                } else {
                    EmailError::GrantRequired
                })
            }
            404 => Ok(Value::Null), // A deleted thread/message must not block other replies.
            200..=299 => response
                .json()
                .await
                .map_err(|_| EmailError::GmailUnavailable),
            _ => Err(EmailError::GmailUnavailable),
        }
    }
}
async fn db<T: Send + 'static>(
    audit: &AuditDb,
    f: impl FnOnce(&TursoDb) -> Result<T, String> + Send + 'static,
) -> Result<T, EmailError> {
    pending::blocking(audit, f)
        .await
        .map_err(|_| EmailError::Storage)?
        .map_err(|_| EmailError::Storage)
}
fn operation_id(rfc: &str) -> String {
    format!("gmail-reply-{:x}", Sha256::digest(rfc.as_bytes()))
}
/// At most 50 new replies per round; all HubSpot work shares its background gateway.
pub async fn run_round(
    audit: &AuditDb,
    client: &HubSpotClient,
    service: &EmailService,
    domains: &[String],
    access: &rbac::CrmAccess,
) -> Result<(), EmailError> {
    let operators = db(audit, |t| t.query("SELECT g.operator_email FROM crm_gmail_grants g LEFT JOIN accounts a ON a.email = g.operator_email WHERE g.revoked_at IS NULL AND a.disabled_at IS NULL ORDER BY g.operator_email", &[])).await?;
    let cutoff = pending::iso(chrono::Utc::now() - chrono::Duration::days(30));
    let mut imports = 0;
    for operator in operators {
        let who = get_str(&operator, "operator_email");
        if !access.is_allowed(&who)
            || !who.rsplit_once('@').is_some_and(|(_, d)| {
                domains
                    .iter()
                    .any(|allowed| d.eq_ignore_ascii_case(allowed.trim()))
            })
        {
            continue;
        }
        let name = who.clone();
        if db(audit, move |t| sync_status(t, &name))
            .await?
            .grant_expired
        {
            continue;
        }
        let name = who.clone();
        let since = cutoff.clone();
        let threads = db(audit, move |t| pending::reply_threads(t, &name, &since)).await?;
        let result = async {
            if !threads.is_empty() {
                let refresh = service.refresh_token(audit, &who).await?;
                let token = service.access_token(&refresh).await?;
                let mut checked = HashSet::new();
                for original in threads {
                    let Some(email) = original.payload.email.as_ref() else {
                        continue;
                    };
                    let Some(gmail) = &email.gmail else {
                        continue;
                    };
                    if !checked.insert(gmail.thread_id.clone()) {
                        continue;
                    }
                    let thread = match service
                        .reply_get(&token, "threads", &gmail.thread_id, "metadata")
                        .await
                    {
                        Ok(thread) => thread,
                        Err(EmailError::GrantRequired) => return Err(EmailError::GrantRequired),
                        Err(e) => {
                            tracing::warn!(error_kind = e.code(), "Gmail reply thread unavailable");
                            continue;
                        }
                    };
                    let Some(messages) = thread["messages"].as_array() else {
                        continue;
                    };
                    for metadata in messages {
                        if imports >= 50 {
                            break;
                        }
                        if !parser::external(metadata, &who, domains, &email.sent_at) {
                            continue;
                        }
                        let rfc = parser::header(metadata, "Message-ID");
                        if rfc.is_empty() {
                            continue;
                        }
                        let op = operation_id(&rfc);
                        let key = op.clone();
                        if db(audit, move |t| pending::get_op(t, &key))
                            .await?
                            .is_some()
                        {
                            continue;
                        }
                        let Some(id) = metadata["id"].as_str() else {
                            continue;
                        };
                        let message = match service.reply_get(&token, "messages", id, "full").await
                        {
                            Ok(message) => message,
                            Err(EmailError::GrantRequired) => {
                                return Err(EmailError::GrantRequired)
                            }
                            Err(e) => {
                                tracing::warn!(
                                    error_kind = e.code(),
                                    "Gmail reply message unavailable"
                                );
                                continue;
                            }
                        };
                        if message["threadId"].as_str() != Some(&gmail.thread_id)
                            || message["id"].as_str() != Some(id)
                            || parser::header(&message, "Message-ID") != rfc
                            || !parser::external(&message, &who, domains, &email.sent_at)
                        {
                            continue;
                        }
                        let incoming = match parser::parse(&message, email) {
                            Ok(incoming) => incoming,
                            Err(e) => {
                                tracing::warn!(
                                    error_kind = e.code(),
                                    "Gmail reply content unavailable"
                                );
                                continue;
                            }
                        };
                        let key = op.clone();
                        let name = who.clone();
                        let deal = original.deal_id.clone();
                        let payload = pending::Payload {
                            steps: vec![],
                            email: Some(incoming),
                        };
                        let inserted = db(audit, move |t| {
                            if pending::get_op(t, &key)?.is_some() {
                                return Ok(false);
                            }
                            match pending::insert_op(t, &key, &name, &deal, &payload, "emails") {
                                Ok(()) => Ok(true),
                                Err(e) => {
                                    if pending::get_op(t, &key)?.is_some() {
                                        Ok(false)
                                    } else {
                                        Err(e)
                                    }
                                }
                            }
                        })
                        .await?;
                        if !inserted {
                            continue;
                        }
                        imports += 1;
                        let mut incoming = db(audit, move |t| pending::get_op(t, &op))
                            .await?
                            .ok_or(EmailError::Storage)?;
                        record(audit, client, &mut incoming).await?;
                        super::write_wake();
                    }
                }
            }
            Ok::<_, EmailError>(())
        }
        .await;
        let name = who.clone();
        match result {
            Ok(()) => db(audit, move |t| { let now = pending::now_iso(); t.execute("INSERT INTO crm_gmail_reply_status(operator_email,last_checked_at) VALUES (?1,?2) ON CONFLICT(operator_email) DO UPDATE SET last_checked_at=excluded.last_checked_at", &[&name,&now]) }).await?,
            Err(EmailError::GrantRequired) => db(audit, move |t| t.execute("INSERT INTO crm_gmail_reply_status(operator_email,expired_grant_at) SELECT operator_email,granted_at FROM crm_gmail_grants WHERE operator_email=?1 ON CONFLICT(operator_email) DO UPDATE SET expired_grant_at=excluded.expired_grant_at", &[&name])).await?,
            Err(e) => { tracing::warn!(error_kind=e.code(), "Gmail reply check failed"); }
        }
    }
    Ok(())
}

/// Durable lease covers the entire search / CREATE / association operation across processes.
/// The pre-CREATE uncertainty marker additionally protects a crash or an expired lease.
pub(super) async fn record(
    audit: &AuditDb,
    client: &HubSpotClient,
    row: &mut OpRow,
) -> Result<(), EmailError> {
    let email = row.payload.email.as_ref().ok_or(EmailError::Invalid)?;
    let rfc = email.rfc_message_id.clone().ok_or(EmailError::Invalid)?;
    let claimant = uuid::Uuid::new_v4().to_string();
    let (key, id, owner) = (row.operation_id.clone(), rfc.clone(), claimant.clone());
    let claimed = db(audit, move |t| {
        t.execute("INSERT OR IGNORE INTO crm_gmail_reply_receipts(message_id,operation_id) VALUES (?1,?2)", &[&id,&key])?;
        let now = pending::now_iso(); let until = pending::iso(chrono::Utc::now() + chrono::Duration::minutes(10));
        t.execute("UPDATE crm_gmail_reply_receipts SET claimed_by=?1,claim_until=?2 WHERE message_id=?3 AND (claimed_by='' OR claim_until<=?4)", &[&owner,&until,&id,&now])?;
        Ok(t.query("SELECT claimed_by FROM crm_gmail_reply_receipts WHERE message_id=?1", &[&id])?.first().is_some_and(|r| get_str(r,"claimed_by") == owner))
    }).await?;
    if !claimed {
        return Ok(());
    }
    // Reload AFTER acquiring the durable lease: another process may have completed a stale row.
    let id = row.operation_id.clone();
    let fresh = db(audit, move |t| pending::get_op(t, &id)).await;
    let result = async {
        *row = fresh?.ok_or(EmailError::Storage)?;
        if !matches!(row.status.as_str(), "pending" | "in_progress") { return Ok(()); }
        let mut email = row.payload.email.clone().ok_or(EmailError::Invalid)?;
        let headers = email.reply.clone().ok_or(EmailError::Invalid)?;
        row.attempts += 1;
        let client = client.background();
        let outcome = async {
            if email.hubspot_email_id.is_none() {
                let found = client.search("emails", json!({"filterGroups":[{"filters":[{"propertyName":"hs_email_message_id","operator":"EQ","value":rfc}]}],"properties":["hs_email_message_id"],"limit":2})).await?;
                if let Some(id) = found["results"].as_array().and_then(|v| v.first()).and_then(|v| v["id"].as_str()) { email.hubspot_email_id = Some(id.into()); }
                else if email.create_uncertain { return Err(HubSpotError::Transport("create_unconfirmed".into())); }
                else {
                    email.create_uncertain = true;
                    row.payload.email = Some(email.clone());
                    super::persist(audit,row).await.map_err(|_| HubSpotError::Transport("ledger_unavailable".into()))?;
                    let properties = json!({"hs_email_direction":"INCOMING_EMAIL","hs_timestamp":email.sent_at,"hs_email_subject":email.subject,"hs_email_html":email.html,"hs_email_message_id":rfc,"hs_email_thread_id":email.gmail.as_ref().map(|g| &g.thread_id),"hubspot_owner_id":email.owner_id,"hs_email_headers":json!({"from":{"email":headers.from},"to":headers.to.iter().map(|s|json!({"email":s})).collect::<Vec<_>>(),"cc":headers.cc.iter().map(|s|json!({"email":s})).collect::<Vec<_>>(),"bcc":[]}).to_string()});
                    match client.create_email(properties, &row.deal_id, &email.contact_ids).await {
                        Ok(id) => { email.hubspot_email_id = Some(id); email.create_uncertain = false; }
                        Err(e) => { if matches!(e, HubSpotError::RateLimited | HubSpotError::Busy | HubSpotError::Auth {..} | HubSpotError::Upstream { status:400..=499 }) { email.create_uncertain = false; } return Err(e); }
                    }
                    row.payload.email = Some(email.clone());
                    super::persist(audit,row).await.map_err(|_| HubSpotError::Transport("ledger_unavailable".into()))?;
                }
            }
            let id = email.hubspot_email_id.as_deref().ok_or(HubSpotError::Decode("missing email id".into()))?;
            client.associate_email(id,"deals",&row.deal_id,210).await?;
            for contact in &email.contact_ids { client.associate_email(id,"contacts",contact,198).await?; }
            Ok::<_,HubSpotError>(())
        }.await;
        row.payload.email = Some(email);
        match outcome {
            Ok(()) => super::finish(row,"saved",""),
            Err(e) => super::finish(row, if row.attempts >= pending::MAX_ATTEMPTS || matches!(e,HubSpotError::Auth{..}|HubSpotError::Upstream{status:400..=499}) {"failed"} else {"pending"},e.error_kind()),
        }
        super::persist(audit,row).await?;
        if row.status == "saved" {
            let name = row.operator_email.clone();
            db(audit,move |t| { let now = pending::now_iso(); t.execute("INSERT INTO crm_gmail_reply_status(operator_email,last_imported_at) VALUES (?1,?2) ON CONFLICT(operator_email) DO UPDATE SET last_imported_at=excluded.last_imported_at", &[&name,&now]) }).await?;
        }
        Ok(())
    }.await;
    let release = db(audit, move |t| t.execute("UPDATE crm_gmail_reply_receipts SET claimed_by='',claim_until='' WHERE message_id=?1 AND claimed_by=?2", &[&rfc,&claimant])).await;
    result.and(release)
}
