//! Google Sheets API REST クライアント (gspread の Rust 版が無いため手書き)
//!
//! - Service Account JSON (base64) を環境変数 `GOOGLE_SA_KEY_B64` から読込
//! - jsonwebtoken で RS256 JWT 生成 → OAuth2 token endpoint で access_token 取得
//! - access_token は tokio::sync::RwLock で 1 時間キャッシュ (失効 5 分前にリフレッシュ)
//! - Spreadsheets API `values.get` で各シートの全レンジを取得
//!
//! 既存 HR_HR の reqwest は再利用。features = ["json"] を仮定 (rustls-tls は明示)。

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::STANDARD as B64, Engine};
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use tokio::sync::RwLock;

const TOKEN_URI: &str = "https://oauth2.googleapis.com/token";
const SHEETS_API: &str = "https://sheets.googleapis.com/v4/spreadsheets";
const SCOPES: &str = "https://www.googleapis.com/auth/spreadsheets.readonly";
/// access_token 失効までこれより短いタイミングでリフレッシュ
const REFRESH_BEFORE_EXPIRY: u64 = 300;

// ---- Service Account JSON ----------------------------------------------

#[derive(Debug, Deserialize)]
struct ServiceAccountKey {
    client_email: String,
    private_key: String,
    token_uri: Option<String>,
    #[allow(dead_code)]
    project_id: Option<String>,
}

// ---- JWT Claims --------------------------------------------------------

#[derive(Debug, Serialize)]
struct JwtClaims {
    iss: String,
    scope: String,
    aud: String,
    exp: u64,
    iat: u64,
}

// ---- OAuth token レスポンス --------------------------------------------

#[derive(Debug, Deserialize)]
struct TokenResponse {
    access_token: String,
    expires_in: u64,
    #[allow(dead_code)]
    token_type: String,
}

// ---- Sheets API レスポンス ---------------------------------------------

#[derive(Debug, Deserialize)]
struct ValuesResponse {
    #[serde(default)]
    values: Vec<Vec<serde_json::Value>>,
    #[serde(default)]
    #[allow(dead_code)]
    range: String,
}

// ---- キャッシュ済み access_token ---------------------------------------

#[derive(Clone)]
struct CachedToken {
    token: String,
    expires_at: u64, // epoch sec
}

// ---- 公開クライアント --------------------------------------------------

pub struct SheetsClient {
    http: reqwest::Client,
    sa_key: ServiceAccountKey,
    spreadsheet_id: String,
    token_cache: Arc<RwLock<Option<CachedToken>>>,
}

impl SheetsClient {
    /// 環境変数から初期化:
    ///   - GOOGLE_SA_KEY_B64 : base64 化された SA JSON
    ///   - SPREADSHEET_ID    : 対象スプシ ID
    pub fn from_env() -> Result<Self> {
        let b64 = std::env::var("GOOGLE_SA_KEY_B64")
            .context("環境変数 GOOGLE_SA_KEY_B64 が未設定")?;
        let json_bytes = B64
            .decode(b64.trim())
            .context("GOOGLE_SA_KEY_B64 の base64 デコードに失敗")?;
        let sa_key: ServiceAccountKey =
            serde_json::from_slice(&json_bytes).context("SA JSON のパースに失敗")?;

        let spreadsheet_id =
            std::env::var("SPREADSHEET_ID").context("環境変数 SPREADSHEET_ID が未設定")?;

        let http = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("reqwest クライアント初期化失敗")?;

        Ok(Self {
            http,
            sa_key,
            spreadsheet_id,
            token_cache: Arc::new(RwLock::new(None)),
        })
    }

    /// access_token を取得 (キャッシュ越し)
    async fn get_access_token(&self) -> Result<String> {
        let now = epoch_now()?;

        // ---- 1) 既存 token が valid なら即返す ----
        {
            let guard = self.token_cache.read().await;
            if let Some(cached) = guard.as_ref() {
                if cached.expires_at > now + REFRESH_BEFORE_EXPIRY {
                    return Ok(cached.token.clone());
                }
            }
        }

        // ---- 2) write lock を取って二重発行を防ぐ ----
        let mut guard = self.token_cache.write().await;
        if let Some(cached) = guard.as_ref() {
            if cached.expires_at > now + REFRESH_BEFORE_EXPIRY {
                return Ok(cached.token.clone());
            }
        }

        // ---- 3) JWT 生成 → token endpoint ----
        let jwt = self.build_jwt(now)?;
        let token_uri = self.sa_key.token_uri.as_deref().unwrap_or(TOKEN_URI);

        let params = [
            ("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer"),
            ("assertion", &jwt),
        ];

        let resp = self
            .http
            .post(token_uri)
            .form(&params)
            .send()
            .await
            .context("OAuth token endpoint POST 失敗")?;

        let status = resp.status();
        let body = resp.text().await.context("token endpoint body 読み込み失敗")?;
        if !status.is_success() {
            bail!("OAuth token 取得失敗: status={} body={}", status, body);
        }
        let token_resp: TokenResponse =
            serde_json::from_str(&body).context("token レスポンスのパース失敗")?;

        let cached = CachedToken {
            token: token_resp.access_token.clone(),
            expires_at: now + token_resp.expires_in,
        };
        *guard = Some(cached);

        Ok(token_resp.access_token)
    }

    fn build_jwt(&self, now: u64) -> Result<String> {
        let claims = JwtClaims {
            iss: self.sa_key.client_email.clone(),
            scope: SCOPES.to_string(),
            aud: self
                .sa_key
                .token_uri
                .clone()
                .unwrap_or_else(|| TOKEN_URI.to_string()),
            iat: now,
            exp: now + 3600,
        };

        let key = EncodingKey::from_rsa_pem(self.sa_key.private_key.as_bytes())
            .context("private_key のパース失敗 (PEM 形式である必要あり)")?;

        let header = Header::new(Algorithm::RS256);
        let token = encode(&header, &claims, &key).context("JWT エンコード失敗")?;
        Ok(token)
    }

    /// 単一シートを取得し header をキーとした `Vec<HashMap<String,String>>` で返す
    pub async fn get_sheet_as_rows(
        &self,
        sheet_name: &str,
    ) -> Result<Vec<HashMap<String, String>>> {
        let token = self.get_access_token().await?;

        // シート名に '/' や日本語が含まれるので URL encode
        // single quote escape は Google Sheets A1 notation 仕様
        let range = format!("'{}'", sheet_name.replace('\'', "''"));
        let encoded_range = urlencoding::encode(&range);

        let url = format!(
            "{base}/{ss_id}/values/{range}?majorDimension=ROWS&valueRenderOption=UNFORMATTED_VALUE&dateTimeRenderOption=FORMATTED_STRING",
            base = SHEETS_API,
            ss_id = self.spreadsheet_id,
            range = encoded_range,
        );

        let resp = self
            .http
            .get(&url)
            .bearer_auth(&token)
            .send()
            .await
            .with_context(|| format!("Sheets API GET 失敗: {sheet_name}"))?;

        let status = resp.status();
        let body = resp.text().await.context("Sheets API body 読み込み失敗")?;
        if !status.is_success() {
            bail!("Sheets API 失敗 ({sheet_name}): status={status} body={body}");
        }

        let parsed: ValuesResponse = serde_json::from_str(&body)
            .with_context(|| format!("ValuesResponse パース失敗 ({sheet_name})"))?;

        if parsed.values.len() < 2 {
            return Ok(vec![]);
        }

        let mut iter = parsed.values.into_iter();
        let header_raw = iter.next().ok_or_else(|| anyhow!("header row 不在"))?;
        // ヘッダは文字列のみを許容
        let header: Vec<String> = header_raw
            .into_iter()
            .map(|v| match v {
                serde_json::Value::String(s) => s,
                serde_json::Value::Null => String::new(),
                other => other.to_string(),
            })
            .collect();
        let mut rows = Vec::with_capacity(iter.size_hint().0);
        for raw in iter {
            let mut obj = HashMap::with_capacity(header.len());
            for (i, key) in header.iter().enumerate() {
                if key.is_empty() {
                    continue;
                }
                let val_str = match raw.get(i) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Null) | None => String::new(),
                    Some(serde_json::Value::Bool(b)) => b.to_string(),
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(other) => other.to_string(),
                };
                obj.insert(key.clone(), val_str);
            }
            rows.push(obj);
        }
        Ok(rows)
    }

    pub fn spreadsheet_id(&self) -> &str {
        &self.spreadsheet_id
    }
}

fn epoch_now() -> Result<u64> {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .context("システム時刻取得失敗")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// JWT claims 構造の sanity check (実 SA 鍵不要)
    #[test]
    fn jwt_claims_serialize() {
        let claims = JwtClaims {
            iss: "test@example.iam.gserviceaccount.com".into(),
            scope: SCOPES.into(),
            aud: TOKEN_URI.into(),
            iat: 1700000000,
            exp: 1700003600,
        };
        let s = serde_json::to_string(&claims).unwrap();
        assert!(s.contains("\"iss\":\"test@example.iam.gserviceaccount.com\""));
        assert!(s.contains("\"exp\":1700003600"));
    }
}
