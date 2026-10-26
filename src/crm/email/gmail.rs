//! Gmail HTTP adapter. Sends once; transport ambiguity never causes a resend.
use super::{EmailError, EmailOperation, EmailService};
use base64::{
    engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD},
    Engine,
};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq, Eq)]
pub struct GmailMessage {
    pub id: String,
    #[serde(rename = "threadId")]
    pub thread_id: String,
}
#[derive(Deserialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub id_token: Option<String>,
    pub scope: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}
/// Gmail へ送った 1 通の失敗 (顧客向け画面の確認コード用。CRM の send_gmail とは別の分け方)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RawSendError {
    Network,
    Status(u16),
}
impl EmailService {
    pub async fn tokens(&self, fields: &[(&str, &str)]) -> Result<Tokens, EmailError> {
        let mut fields = fields.to_vec();
        fields.extend([
            ("client_id", self.cfg.google.client_id.as_str()),
            ("client_secret", self.cfg.google.client_secret.as_str()),
        ]);
        let body = fields
            .iter()
            .map(|(k, v)| format!("{}={}", urlencoding::encode(k), urlencoding::encode(v)))
            .collect::<Vec<_>>()
            .join("&");
        let r = self
            .http
            .post(&self.cfg.token_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(body)
            .send()
            .await
            .map_err(|_| EmailError::GmailUnavailable)?;
        if r.status().as_u16() == 400 {
            return Err(EmailError::GrantRequired);
        }
        if !r.status().is_success() {
            return Err(EmailError::GmailUnavailable);
        }
        r.json().await.map_err(|_| EmailError::GmailUnavailable)
    }
    pub async fn access_token(&self, refresh: &str) -> Result<String, EmailError> {
        self.access_token_with_expiry(refresh).await.map(|(t, _)| t)
    }
    /// access token と有効秒数 (Google が返したとき)
    pub async fn access_token_with_expiry(
        &self,
        refresh: &str,
    ) -> Result<(String, Option<u64>), EmailError> {
        let t = self
            .tokens(&[("grant_type", "refresh_token"), ("refresh_token", refresh)])
            .await?;
        if t.access_token.is_empty() {
            return Err(EmailError::GrantRequired);
        }
        Ok((t.access_token, t.expires_in))
    }
    /// 組み立て済みの RFC 822 (base64url) を 1 通送る。再送はしない
    pub async fn send_raw(&self, access: &str, raw: &str) -> Result<(), RawSendError> {
        let r = self
            .http
            .post(format!("{}/users/me/messages/send", self.cfg.gmail_url))
            .bearer_auth(access)
            .json(&json!({ "raw": raw }))
            .send()
            .await
            .map_err(|_| RawSendError::Network)?;
        if r.status().is_success() {
            Ok(())
        } else {
            Err(RawSendError::Status(r.status().as_u16()))
        }
    }
    pub async fn revoke_google(&self, refresh: &str) -> Result<(), EmailError> {
        let r = self
            .http
            .post(&self.cfg.revoke_url)
            .header("Content-Type", "application/x-www-form-urlencoded")
            .body(format!("token={}", urlencoding::encode(refresh)))
            .send()
            .await
            .map_err(|_| EmailError::GmailUnavailable)?;
        // Already invalid tokens also count as revoked. Local credentials are cleared first.
        if r.status().is_success() || r.status().as_u16() == 400 {
            Ok(())
        } else {
            Err(EmailError::GmailUnavailable)
        }
    }
    pub async fn send_gmail(
        &self,
        access: &str,
        operator: &str,
        email: &EmailOperation,
    ) -> Result<GmailMessage, EmailError> {
        let r = self
            .http
            .post(format!("{}/users/me/messages/send", self.cfg.gmail_url))
            .bearer_auth(access)
            .json(&json!({"raw": mime(operator, email)}))
            .send()
            .await
            .map_err(|_| EmailError::SendUncertain)?;
        match r.status().as_u16() {
            200..=299 => r.json().await.map_err(|_| EmailError::SendUncertain),
            400..=499 => Err(EmailError::GmailRejected),
            _ => Err(EmailError::SendUncertain),
        }
    }
    pub async fn message_id(&self, access: &str, gmail_id: &str) -> Result<String, EmailError> {
        if !gmail_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
        {
            return Err(EmailError::GmailUnavailable);
        }
        let r = self
            .http
            .get(format!(
                "{}/users/me/messages/{}",
                self.cfg.gmail_url, gmail_id
            ))
            .bearer_auth(access)
            .query(&[("format", "metadata"), ("metadataHeaders", "Message-ID")])
            .send()
            .await
            .map_err(|_| EmailError::GmailUnavailable)?;
        if !r.status().is_success() {
            return Err(EmailError::GmailUnavailable);
        }
        let v: Value = r.json().await.map_err(|_| EmailError::GmailUnavailable)?;
        v["payload"]["headers"]
            .as_array()
            .and_then(|headers| {
                headers.iter().find(|h| {
                    h["name"]
                        .as_str()
                        .is_some_and(|n| n.eq_ignore_ascii_case("message-id"))
                })
            })
            .and_then(|h| h["value"].as_str())
            .filter(|s| !s.is_empty() && !s.contains(['\r', '\n']))
            .map(str::to_string)
            .ok_or(EmailError::GmailUnavailable)
    }
    pub async fn find_sent(
        &self,
        access: &str,
        rfc_id: &str,
    ) -> Result<Option<GmailMessage>, EmailError> {
        let r = self
            .http
            .get(format!("{}/users/me/messages", self.cfg.gmail_url))
            .bearer_auth(access)
            .query(&[
                ("q", format!("in:sent rfc822msgid:{rfc_id}")),
                ("maxResults", "2".to_string()),
            ])
            .send()
            .await
            .map_err(|_| EmailError::GmailUnavailable)?;
        if !r.status().is_success() {
            return Err(EmailError::GmailUnavailable);
        }
        let v: Value = r.json().await.map_err(|_| EmailError::GmailUnavailable)?;
        let messages = v["messages"].as_array();
        if messages.is_some_and(|m| m.len() > 1) {
            return Err(EmailError::SendUncertain);
        }
        messages
            .and_then(|m| m.first())
            .map(|m| serde_json::from_value(m.clone()).map_err(|_| EmailError::GmailUnavailable))
            .transpose()
    }
}
/// 件名を RFC 2047 の encoded-word にする。1 語は 75 文字まで (UTF-8 の 45 バイトずつ、文字の途中で切らない)。
/// 語どうしは折り返し (CRLF + 空白) でつなぐ
pub fn encode_subject(subject: &str) -> String {
    const MAX_BYTES: usize = 45; // base64 で 60 文字 + "=?UTF-8?B?" と "?=" の 12 文字 = 72 文字
    let mut words = Vec::new();
    let mut chunk = String::new();
    for c in subject.chars() {
        if chunk.len() + c.len_utf8() > MAX_BYTES {
            words.push(std::mem::take(&mut chunk));
        }
        chunk.push(c);
    }
    if !chunk.is_empty() || words.is_empty() {
        words.push(chunk);
    }
    words
        .iter()
        .map(|w| format!("=?UTF-8?B?{}?=", STANDARD.encode(w)))
        .collect::<Vec<_>>()
        .join("\r\n ")
}
/// 1 宛先・本文は文字だけのメール。宛先・送信元は呼ぶ側で valid_address を通してあること
pub fn plain_mime(from: &str, to: &str, subject: &str, body: &str) -> String {
    let subject = encode_subject(subject);
    let body = STANDARD.encode(body.replace("\r\n", "\n").replace('\n', "\r\n"));
    let wrapped = body
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\r\n");
    URL_SAFE_NO_PAD.encode(format!("From: {from}\r\nTo: {to}\r\nSubject: {subject}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/plain; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n", chrono::Utc::now().to_rfc2822()))
}
pub fn mime(operator: &str, email: &EmailOperation) -> String {
    // Every address/subject is validated before this; UTF-8 encoded-word avoids header injection.
    let subject = format!("=?UTF-8?B?{}?=", STANDARD.encode(&email.subject));
    let body = STANDARD.encode(&email.html);
    let wrapped = body
        .as_bytes()
        .chunks(76)
        .map(|c| std::str::from_utf8(c).unwrap_or_default())
        .collect::<Vec<_>>()
        .join("\r\n");
    URL_SAFE_NO_PAD.encode(format!("From: {operator}\r\nTo: {}\r\nCc: {}\r\nSubject: {subject}\r\nMessage-ID: {}\r\nDate: {}\r\nMIME-Version: 1.0\r\nContent-Type: text/html; charset=UTF-8\r\nContent-Transfer-Encoding: base64\r\n\r\n{wrapped}\r\n", email.to, email.cc.join(", "), email.requested_message_id, chrono::Utc::now().to_rfc2822()))
}
