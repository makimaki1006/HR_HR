//! 社内向けテスト版の Google 呼び出し (Sheets の読み書きと Drive の更新時刻)。
//!
//! 鍵は既存と同じ `GOOGLE_SA_KEY_B64`。既存の `db::sheets_client::SheetsClient` は読み取り専用の
//! スコープで固定しているので変えず、ここで書き込みのスコープ (spreadsheets と
//! drive.metadata.readonly) のトークンを別に取る。鍵の中身・トークン・シートの値はログに出さない。

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::STANDARD as B64, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::sync::RwLock;

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
pub const SHEETS_API: &str = "https://sheets.googleapis.com/v4/spreadsheets";
pub const DRIVE_API: &str = "https://www.googleapis.com/drive/v3/files";
pub const SCOPES: &str =
    "https://www.googleapis.com/auth/spreadsheets https://www.googleapis.com/auth/drive.metadata.readonly";
const REFRESH_BEFORE_EXPIRY: u64 = 300;

#[derive(Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
}

#[derive(Serialize)]
struct JwtClaims<'a> {
    iss: &'a str,
    scope: &'a str,
    aud: &'a str,
    exp: u64,
    iat: u64,
}

#[derive(Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
}

/// Google 呼び出しの失敗。値や本文は持たない (ログ・応答に出さないため)。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GoogleError {
    /// Google が返した HTTP の状態
    Status(u16),
    /// 送れなかった・応答が読めなかった
    Network,
    /// トークンが取れなかった
    Token,
}

pub struct GoogleSa {
    http: reqwest::Client,
    client_email: String,
    private_key: String,
    token_uri: String,
    token: RwLock<Option<(String, u64)>>,
}

fn epoch_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

impl GoogleSa {
    /// `GOOGLE_SA_KEY_B64` から作る。無い・読めないときは None (理由はログに出さない)。
    pub fn from_env() -> Option<Self> {
        let b64 = std::env::var("GOOGLE_SA_KEY_B64").ok()?;
        let bytes = B64.decode(b64.trim()).ok()?;
        let key: ServiceAccountKey = serde_json::from_slice(&bytes).ok()?;
        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .ok()?;
        Some(Self {
            http,
            client_email: key.client_email,
            private_key: key.private_key,
            token_uri: key.token_uri.unwrap_or_else(|| TOKEN_URI.to_string()),
            token: RwLock::new(None),
        })
    }

    pub fn client_email(&self) -> &str {
        &self.client_email
    }

    async fn access_token(&self) -> Result<String, GoogleError> {
        let now = epoch_now();
        if let Some((t, exp)) = self.token.read().await.as_ref() {
            if *exp > now + REFRESH_BEFORE_EXPIRY {
                return Ok(t.clone());
            }
        }
        let mut guard = self.token.write().await;
        if let Some((t, exp)) = guard.as_ref() {
            if *exp > now + REFRESH_BEFORE_EXPIRY {
                return Ok(t.clone());
            }
        }
        let claims = JwtClaims {
            iss: &self.client_email,
            scope: SCOPES,
            aud: &self.token_uri,
            iat: now,
            exp: now + 3600,
        };
        let key = EncodingKey::from_rsa_pem(self.private_key.as_bytes())
            .map_err(|_| GoogleError::Token)?;
        let jwt = encode(&Header::new(Algorithm::RS256), &claims, &key)
            .map_err(|_| GoogleError::Token)?;
        let resp = self
            .http
            .post(&self.token_uri)
            .form(&[
                ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
                ("assertion", jwt.as_str()),
            ])
            .send()
            .await
            .map_err(|_| GoogleError::Token)?;
        if !resp.status().is_success() {
            return Err(GoogleError::Token);
        }
        let t: TokenResponse = resp.json().await.map_err(|_| GoogleError::Token)?;
        *guard = Some((t.access_token.clone(), now + t.expires_in));
        Ok(t.access_token)
    }

    async fn finish(resp: Result<reqwest::Response, reqwest::Error>) -> Result<Value, GoogleError> {
        let resp = resp.map_err(|_| GoogleError::Network)?;
        let status = resp.status();
        if !status.is_success() {
            return Err(GoogleError::Status(status.as_u16()));
        }
        resp.json().await.map_err(|_| GoogleError::Network)
    }

    pub async fn get(&self, url: &str, query: &[(&str, &str)]) -> Result<Value, GoogleError> {
        let token = self.access_token().await?;
        Self::finish(
            self.http
                .get(url)
                .query(query)
                .bearer_auth(token)
                .send()
                .await,
        )
        .await
    }

    pub async fn put(
        &self,
        url: &str,
        query: &[(&str, &str)],
        body: &Value,
    ) -> Result<Value, GoogleError> {
        let token = self.access_token().await?;
        Self::finish(
            self.http
                .put(url)
                .query(query)
                .bearer_auth(token)
                .json(body)
                .send()
                .await,
        )
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn 書き込みとドライブの更新時刻のスコープで取る() {
        let scopes: Vec<&str> = SCOPES.split(' ').collect();
        assert_eq!(
            scopes,
            vec![
                "https://www.googleapis.com/auth/spreadsheets",
                "https://www.googleapis.com/auth/drive.metadata.readonly"
            ]
        );
    }
}
