//! Read-only Drive transport. The calling handler owns Company/Listing authorization.
//! No browser-provided URL, OAuth credentials, upstream body, or token is logged.
use base64::{engine::general_purpose::STANDARD, Engine};
use jsonwebtoken::{Algorithm, EncodingKey, Header};
use reqwest::{redirect::Policy, Client, Response, StatusCode};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    sync::Arc,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
use tokio::sync::Mutex;

const TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const FILES_URL: &str = "https://www.googleapis.com/drive/v3/files";
const SCOPE: &str = "https://www.googleapis.com/auth/drive.readonly";
const MANIFEST_LIMIT: usize = 2 * 1024 * 1024;
const IMAGE_LIMIT: usize = 5 * 1024 * 1024;
const TOKEN_LIMIT: usize = 64 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DriveError(pub &'static str);

impl std::fmt::Display for DriveError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.0)
    }
}
impl std::error::Error for DriveError {}

#[derive(Deserialize)]
struct ServiceKey {
    client_email: String,
    private_key: String,
}
#[derive(Serialize)]
struct Claims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    iat: u64,
    exp: u64,
}
#[derive(Deserialize)]
struct TokenReply {
    access_token: String,
    expires_in: u64,
    token_type: String,
}
struct CachedToken {
    value: String,
    expires: Instant,
}
struct Inner {
    client: Client,
    email: String,
    key: EncodingKey,
    token_url: String,
    files_url: String,
    token: Mutex<Option<CachedToken>>,
}

#[derive(Clone)]
pub struct DriveReader {
    inner: Arc<Inner>,
}

impl DriveReader {
    #[cfg(test)]
    pub(crate) fn for_test(base: &str) -> Self {
        let key = serde_json::json!({
            "client_email": "synthetic@fixture.iam.gserviceaccount.com",
            "private_key": include_str!("../../tests/fixtures/oidc/test_key_1.pem")
        });
        Self::build(
            &key.to_string(),
            format!("{base}/token"),
            format!("{base}/drive/v3/files"),
        )
        .expect("synthetic test key")
    }

    pub fn from_env() -> Result<Self, DriveError> {
        let encoded = std::env::var("GOOGLE_SA_KEY_B64")
            .map_err(|_| DriveError("drive_service_key_missing"))?;
        if encoded.len() > 100_000 {
            return Err(DriveError("drive_service_key_invalid"));
        }
        let decoded = STANDARD
            .decode(encoded.trim())
            .map_err(|_| DriveError("drive_service_key_invalid"))?;
        let key =
            std::str::from_utf8(&decoded).map_err(|_| DriveError("drive_service_key_invalid"))?;
        Self::new_with_key(key)
    }

    /// SA JSON only; its token_uri cannot override the fixed Google endpoint.
    pub fn new_with_key(key_json: &str) -> Result<Self, DriveError> {
        Self::build(key_json, TOKEN_URL.to_owned(), FILES_URL.to_owned())
    }

    fn build(key_json: &str, token_url: String, files_url: String) -> Result<Self, DriveError> {
        if key_json.len() > TOKEN_LIMIT {
            return Err(DriveError("drive_service_key_invalid"));
        }
        let service: ServiceKey =
            serde_json::from_str(key_json).map_err(|_| DriveError("drive_service_key_invalid"))?;
        if service.client_email.len() > 254
            || !service.client_email.ends_with(".gserviceaccount.com")
            || !service.client_email.contains('@')
            || service.client_email.chars().any(char::is_whitespace)
        {
            return Err(DriveError("drive_service_key_invalid"));
        }
        let key = EncodingKey::from_rsa_pem(service.private_key.as_bytes())
            .map_err(|_| DriveError("drive_service_key_invalid"))?;
        let client = Client::builder()
            .redirect(Policy::none())
            .no_proxy()
            .connect_timeout(Duration::from_secs(5))
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|_| DriveError("drive_client_invalid"))?;
        Ok(Self {
            inner: Arc::new(Inner {
                client,
                email: service.client_email,
                key,
                token_url,
                files_url,
                token: Mutex::new(None),
            }),
        })
    }

    async fn token(&self) -> Result<String, DriveError> {
        let mut cached = self.inner.token.lock().await;
        if let Some(token) = cached
            .as_ref()
            .filter(|token| token.expires > Instant::now())
        {
            return Ok(token.value.clone());
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| DriveError("drive_clock_invalid"))?
            .as_secs();
        let assertion = jsonwebtoken::encode(
            &Header::new(Algorithm::RS256),
            &Claims {
                iss: &self.inner.email,
                scope: SCOPE,
                aud: TOKEN_URL,
                iat: now,
                exp: now + 3600,
            },
            &self.inner.key,
        )
        .map_err(|_| DriveError("drive_assertion_failed"))?;
        let response = self
            .inner
            .client
            .post(&self.inner.token_url)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", assertion.as_str()),
            ])
            .send()
            .await
            .map_err(|_| DriveError("drive_token_unavailable"))?;
        if !response.status().is_success() {
            return Err(DriveError("drive_token_rejected"));
        }
        let (_, raw) = bounded(response, TOKEN_LIMIT).await?;
        let reply: TokenReply =
            serde_json::from_slice(&raw).map_err(|_| DriveError("drive_token_invalid"))?;
        if reply.token_type != "Bearer"
            || reply.access_token.is_empty()
            || reply.access_token.len() > 8192
            || reply
                .access_token
                .chars()
                .any(|c| c.is_control() || c.is_whitespace())
            || reply.expires_in <= 30
            || reply.expires_in > 86_400
        {
            return Err(DriveError("drive_token_invalid"));
        }
        let value = reply.access_token;
        *cached = Some(CachedToken {
            value: value.clone(),
            expires: Instant::now() + Duration::from_secs(reply.expires_in.min(3600) - 30),
        });
        Ok(value)
    }

    async fn read_bytes(
        &self,
        file_id: &str,
        limit: usize,
    ) -> Result<(String, Vec<u8>), DriveError> {
        validate_id(file_id)?;
        let token = self.token().await?;
        let response = self
            .inner
            .client
            .get(format!("{}/{}", self.inner.files_url, file_id))
            .query(&[("alt", "media"), ("supportsAllDrives", "true")])
            .bearer_auth(token)
            .send()
            .await
            .map_err(|_| DriveError("drive_file_unavailable"))?;
        match response.status() {
            StatusCode::OK => bounded(response, limit).await,
            StatusCode::UNAUTHORIZED => {
                *self.inner.token.lock().await = None;
                Err(DriveError("drive_file_denied"))
            }
            StatusCode::FORBIDDEN => Err(DriveError("drive_file_denied")),
            StatusCode::NOT_FOUND => Err(DriveError("drive_file_missing")),
            _ => Err(DriveError("drive_file_failed")),
        }
    }

    pub async fn read_manifest(&self, file_id: &str) -> Result<Value, DriveError> {
        let (_, raw) = self.read_bytes(file_id, MANIFEST_LIMIT).await?;
        parse_manifest(&raw)
    }

    pub async fn read_manifest_verified(
        &self,
        file_id: &str,
        expected_sha256: &str,
    ) -> Result<Value, DriveError> {
        validate_hash(expected_sha256)?;
        let (_, raw) = self.read_bytes(file_id, MANIFEST_LIMIT).await?;
        verify_hash(&raw, expected_sha256)?;
        parse_manifest(&raw)
    }

    /// Configured immutable review snapshot, never an arbitrary browser file ID.
    pub async fn read_snapshot_verified(
        &self,
        file_id: &str,
        expected_sha256: &str,
    ) -> Result<Value, DriveError> {
        validate_hash(expected_sha256)?;
        let (mime, raw) = self.read_bytes(file_id, 32 * 1024 * 1024).await?;
        if mime != "application/json" {
            return Err(DriveError("drive_snapshot_mime_invalid"));
        }
        verify_hash(&raw, expected_sha256)?;
        parse_manifest(&raw)
    }

    pub async fn read_image(
        &self,
        file_id: &str,
        expected_sha256: &str,
    ) -> Result<(String, Vec<u8>), DriveError> {
        validate_hash(expected_sha256)?;
        let (mime, raw) = self.read_bytes(file_id, IMAGE_LIMIT).await?;
        if !matches!(mime.as_str(), "image/jpeg" | "image/png" | "image/webp") {
            return Err(DriveError("drive_image_mime_invalid"));
        }
        verify_hash(&raw, expected_sha256)?;
        Ok((mime, raw))
    }
}

fn validate_id(id: &str) -> Result<(), DriveError> {
    if !(10..=200).contains(&id.len())
        || !id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        return Err(DriveError("drive_file_id_invalid"));
    }
    Ok(())
}
fn validate_hash(hash: &str) -> Result<(), DriveError> {
    if hash.len() != 64 || !hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(DriveError("drive_hash_invalid"));
    }
    Ok(())
}
fn verify_hash(raw: &[u8], expected: &str) -> Result<(), DriveError> {
    if format!("{:x}", Sha256::digest(raw)) != expected.to_ascii_lowercase() {
        return Err(DriveError("drive_hash_mismatch"));
    }
    Ok(())
}
fn parse_manifest(raw: &[u8]) -> Result<Value, DriveError> {
    let value: Value =
        serde_json::from_slice(raw).map_err(|_| DriveError("drive_manifest_invalid"))?;
    if !value.is_object() {
        return Err(DriveError("drive_manifest_invalid"));
    }
    Ok(value)
}
async fn bounded(mut response: Response, limit: usize) -> Result<(String, Vec<u8>), DriveError> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err(DriveError("drive_file_too_large"));
    }
    let mime = response
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|header| header.to_str().ok())
        .and_then(|header| header.split(';').next())
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    let mut raw = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| DriveError("drive_file_unavailable"))?
    {
        if raw.len().saturating_add(chunk.len()) > limit {
            return Err(DriveError("drive_file_too_large"));
        }
        raw.extend_from_slice(&chunk);
    }
    Ok((mime, raw))
}

#[cfg(test)]
#[path = "job_copy_drive/tests.rs"]
mod tests;
