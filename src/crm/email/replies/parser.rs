//! Gmail parsed MIME payloads: use HTML when present, escape plain text otherwise.
use super::super::{EmailError, EmailOperation};
use super::ReplyHeaders;
use base64::{engine::general_purpose::URL_SAFE, Engine};
use serde_json::Value;

pub(super) fn header(message: &Value, name: &str) -> String {
    message["payload"]["headers"]
        .as_array()
        .and_then(|h| {
            h.iter().find(|h| {
                h["name"]
                    .as_str()
                    .is_some_and(|n| n.eq_ignore_ascii_case(name))
            })
        })
        .and_then(|h| h["value"].as_str())
        .unwrap_or_default()
        .trim()
        .to_string()
}
pub(super) fn addresses(value: &str) -> Vec<String> {
    let raw = format!("Address: {value}");
    let Ok((h, _)) = mailparse::parse_header(raw.as_bytes()) else {
        return vec![];
    };
    let Ok(list) = mailparse::addrparse_header(&h) else {
        return vec![];
    };
    list.iter()
        .flat_map(|a| match a {
            mailparse::MailAddr::Single(s) => vec![s.addr.to_lowercase()],
            mailparse::MailAddr::Group(g) => {
                g.addrs.iter().map(|s| s.addr.to_lowercase()).collect()
            }
        })
        .collect()
}
pub(super) fn external(message: &Value, operator: &str, domains: &[String], sent_at: &str) -> bool {
    let from = addresses(&header(message, "From"));
    if from.len() != 1
        || !super::super::valid_address(&from[0])
        || from[0].eq_ignore_ascii_case(operator)
        || from[0].rsplit_once('@').is_some_and(|(_, d)| {
            domains
                .iter()
                .any(|allowed| d.eq_ignore_ascii_case(allowed.trim()))
        })
    {
        return false;
    }
    if message["labelIds"].as_array().is_some_and(|labels| {
        labels
            .iter()
            .any(|l| matches!(l.as_str(), Some("SENT" | "DRAFT")))
    }) {
        return false;
    }
    received_at(message).is_some_and(|at| at.as_str() >= sent_at)
}
fn received_at(message: &Value) -> Option<String> {
    let ms = message["internalDate"].as_str()?.parse::<i64>().ok()?;
    chrono::DateTime::from_timestamp_millis(ms).map(crate::crm::pending::iso)
}
fn bodies(
    part: &Value,
    depth: usize,
    html: &mut Option<String>,
    text: &mut Option<String>,
) -> Result<(), EmailError> {
    if depth > 30 {
        return Err(EmailError::Invalid);
    }
    if part["filename"].as_str().is_some_and(|s| !s.is_empty()) {
        return Ok(());
    }
    let mime = part["mimeType"].as_str().unwrap_or_default();
    if matches!(mime, "text/html" | "text/plain") {
        if let Some(data) = part["body"]["data"].as_str() {
            if data.len() > 1_400_000 {
                return Err(EmailError::Invalid);
            }
            let padded = format!("{data}{}", "=".repeat((4 - data.len() % 4) % 4));
            let bytes = URL_SAFE.decode(padded).map_err(|_| EmailError::Invalid)?;
            let content = part["headers"]
                .as_array()
                .and_then(|hs| {
                    hs.iter().find(|h| {
                        h["name"]
                            .as_str()
                            .is_some_and(|n| n.eq_ignore_ascii_case("Content-Type"))
                    })
                })
                .and_then(|h| h["value"].as_str())
                .unwrap_or(mime);
            let charset = mailparse::parse_content_type(content).charset;
            let encoding =
                encoding_rs::Encoding::for_label(charset.as_bytes()).unwrap_or(encoding_rs::UTF_8);
            let body = encoding.decode(&bytes).0.into_owned();
            if mime == "text/html" {
                html.get_or_insert(body);
            } else {
                text.get_or_insert(body);
            }
        }
    }
    // Do not read attached forwarded messages.
    if mime != "message/rfc822" {
        if let Some(parts) = part["parts"].as_array() {
            for p in parts {
                bodies(p, depth + 1, html, text)?;
            }
        }
    }
    Ok(())
}
pub(super) fn parse(
    message: &Value,
    original: &EmailOperation,
) -> Result<EmailOperation, EmailError> {
    let id = header(message, "Message-ID");
    if id.is_empty() || id.len() > 998 || id.contains(['\r', '\n']) {
        return Err(EmailError::Invalid);
    }
    let mut html = None;
    let mut text = None;
    bodies(&message["payload"], 0, &mut html, &mut text)?;
    let html = html.unwrap_or_else(|| {
        let text = text
            .unwrap_or_default()
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;")
            .replace('"', "&quot;")
            .replace('\'', "&#39;");
        format!(
            "<p>{}</p>",
            text.replace("\r\n", "\n").replace('\n', "<br>")
        )
    });
    let subject = header(message, "Subject");
    let raw = format!("Subject: {subject}");
    let mut subject = mailparse::parse_header(raw.as_bytes())
        .map(|(h, _)| h.get_value())
        .unwrap_or(subject);
    if subject.is_empty() {
        subject = "(件名なし)".into();
    }
    let auto = header(message, "Auto-Submitted");
    if (!auto.is_empty() && !auto.eq_ignore_ascii_case("no"))
        || !header(message, "X-Autoreply").is_empty()
        || !header(message, "X-Autorespond").is_empty()
    {
        subject = format!("【自動返信】{subject}");
    }
    let headers = ReplyHeaders {
        from: addresses(&header(message, "From"))
            .into_iter()
            .next()
            .ok_or(EmailError::Invalid)?,
        to: addresses(&header(message, "To")),
        cc: addresses(&header(message, "Cc")),
    };
    let mut email = original.clone();
    email.to = headers.to.join(", ");
    email.cc = headers.cc.clone();
    email.reply = Some(headers);
    email.subject = subject;
    email.html = html;
    email.rfc_message_id = Some(id.clone());
    email.requested_message_id = id;
    email.gmail = Some(serde_json::from_value(message.clone()).map_err(|_| EmailError::Invalid)?);
    email.hubspot_email_id = None;
    email.sent_at = received_at(message).ok_or(EmailError::Invalid)?;
    email.send_state = "received".into();
    email.tracking_id.clear();
    email.create_uncertain = false;
    Ok(email)
}
