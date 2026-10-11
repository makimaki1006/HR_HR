//! Gmail change IDs only. Read headers/content solely for our eligible sent threads.
use super::{db, gmail_response, EmailError, EmailService, OpRow};
use crate::{audit::AuditDb, handlers::helpers::get_str};
use serde_json::Value;
use std::collections::{HashMap, HashSet};

pub(super) async fn cursor(audit: &AuditDb, operator: &str) -> Result<Option<String>, EmailError> {
    let who = operator.to_string();
    db(audit, move |t| {
        Ok(t.query(
            "SELECT history_id FROM crm_gmail_reply_cursors WHERE operator_email=?1",
            &[&who],
        )?
        .first()
        .map(|r| get_str(r, "history_id")))
    })
    .await
}
pub(super) async fn save(
    audit: &AuditDb,
    operator: &str,
    old: Option<String>,
    new: String,
) -> Result<(), EmailError> {
    let who = operator.to_string();
    let old = old.unwrap_or_default();
    // A concurrent round cannot overwrite a cursor which another round already advanced.
    db(audit, move |t| t.execute("INSERT INTO crm_gmail_reply_cursors(operator_email,history_id) VALUES (?1,?2) ON CONFLICT(operator_email) DO UPDATE SET history_id=excluded.history_id WHERE history_id=?3", &[&who,&new,&old])).await
}
fn history_id(value: &Value) -> Result<String, EmailError> {
    value["historyId"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 20 && s.bytes().all(|b| b.is_ascii_digit()))
        .map(str::to_string)
        .ok_or(EmailError::GmailUnavailable)
}
async fn get(
    service: &EmailService,
    token: &str,
    path: &str,
    query: &[(&str, &str)],
) -> Result<Value, EmailError> {
    let response = service
        .http
        .get(format!("{}/users/me/{path}", service.cfg.gmail_url))
        .bearer_auth(token)
        .query(query)
        .send()
        .await
        .map_err(|_| EmailError::GmailUnavailable)?;
    gmail_response(response).await
}
pub(super) async fn candidates(
    service: &EmailService,
    token: &str,
    cursor: Option<&str>,
    originals: Vec<OpRow>,
) -> Result<(String, Vec<(Value, OpRow)>), EmailError> {
    let mut targets = HashMap::new();
    for row in originals {
        if let Some(thread) = row
            .payload
            .email
            .as_ref()
            .and_then(|e| e.gmail.as_ref())
            .map(|g| g.thread_id.clone())
        {
            targets.entry(thread).or_insert(row);
        }
    }
    if let Some(start) = cursor {
        let mut page = String::new();
        let mut seen_pages = HashSet::new();
        let mut seen_messages = HashSet::new();
        let mut messages = vec![];
        loop {
            let mut query = vec![
                ("startHistoryId", start),
                ("historyTypes", "messageAdded"),
                ("maxResults", "500"),
            ];
            if !page.is_empty() {
                query.push(("pageToken", &page));
            }
            let value = get(service, token, "history", &query).await?;
            if value.is_null() {
                break;
            } // History expired (404): one targeted bootstrap below.
            let next_id = history_id(&value)?;
            if let Some(history) = value["history"].as_array() {
                for entry in history {
                    if let Some(added) = entry["messagesAdded"].as_array() {
                        for added in added {
                            let msg = &added["message"];
                            let Some(original) =
                                msg["threadId"].as_str().and_then(|id| targets.get(id))
                            else {
                                continue;
                            };
                            let id = msg["id"].as_str().ok_or(EmailError::GmailUnavailable)?;
                            if !seen_messages.insert(id.to_string()) {
                                continue;
                            }
                            let metadata =
                                service.reply_get(token, "messages", id, "metadata").await?;
                            if metadata.is_null() {
                                continue;
                            } // Deleted in the meantime.
                            if metadata["id"] != msg["id"]
                                || metadata["threadId"] != msg["threadId"]
                            {
                                return Err(EmailError::GmailUnavailable);
                            }
                            messages.push((metadata, original.clone()));
                        }
                    }
                }
            }
            match value["nextPageToken"].as_str().filter(|s| !s.is_empty()) {
                None => return Ok((next_id, messages)),
                Some(next) => {
                    if !seen_pages.insert(next.to_string()) {
                        return Err(EmailError::GmailUnavailable);
                    }
                    page = next.to_string();
                }
            }
        }
    }
    // Snapshot BEFORE fetching threads: additions during bootstrap are replayed next round.
    let snapshot = history_id(&get(service, token, "profile", &[]).await?)?;
    let mut messages = vec![];
    for (thread, original) in targets {
        let value = service
            .reply_get(token, "threads", &thread, "metadata")
            .await?;
        if let Some(entries) = value["messages"].as_array() {
            for metadata in entries {
                messages.push((metadata.clone(), original.clone()));
            }
        }
    }
    Ok((snapshot, messages))
}
