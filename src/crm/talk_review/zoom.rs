//! Zoom Phone の録音・文字起こし。Server-to-Server OAuth (`grant_type=account_credentials`) で読み取るだけ。

use std::sync::Arc;
use std::time::{Duration, Instant};

use serde_json::Value;

use super::transcript::turns_from;
use super::types::TalkTurn;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ZoomError {
    /// 録音・文字起こしがまだできていない (後で再試行)
    NotReady,
    /// 録音が残っていない (古くて消えた)
    Gone,
    /// 通信・混雑・認証の一時的な失敗 (後で再試行)
    Transient(String),
    /// Zoom の設定 (環境変数) が無い
    NotConfigured,
}

/// HTTP の状態から分類する (純粋。テスト用に分離)
pub fn classify_status(status: u16) -> ZoomError {
    match status {
        400 => ZoomError::NotReady,
        404 | 410 => ZoomError::Gone,
        s => ZoomError::Transient(format!("http {s}")),
    }
}

/// 文字起こしの取得元 (テストで偽物に差し替える)
pub trait TranscriptSource: Send + Sync {
    fn fetch(
        &self,
        call_log_id: &str,
    ) -> impl std::future::Future<Output = Result<Vec<TalkTurn>, ZoomError>> + Send;
}

struct Token {
    value: String,
    until: Instant,
}

pub struct ZoomClient {
    account_id: String,
    client_id: String,
    client_secret: String,
    http: reqwest::Client,
    token: tokio::sync::Mutex<Option<Token>>,
}

impl ZoomClient {
    pub fn from_env() -> Option<Arc<Self>> {
        let get = |k: &str| {
            std::env::var(k)
                .ok()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
        };
        Some(Arc::new(Self {
            account_id: get("ZOOM_ACCOUNT_ID")?,
            client_id: get("ZOOM_CLIENT_ID")?,
            client_secret: get("ZOOM_CLIENT_SECRET")?,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(60))
                .build()
                .ok()?,
            token: tokio::sync::Mutex::new(None),
        }))
    }

    async fn bearer(&self) -> Result<String, ZoomError> {
        let mut g = self.token.lock().await;
        if let Some(t) = g.as_ref() {
            if Instant::now() < t.until {
                return Ok(t.value.clone());
            }
        }
        let r = self
            .http
            .post("https://zoom.us/oauth/token")
            .query(&[
                ("grant_type", "account_credentials"),
                ("account_id", self.account_id.as_str()),
            ])
            .basic_auth(&self.client_id, Some(&self.client_secret))
            .send()
            .await
            .map_err(|e| ZoomError::Transient(format!("token: {}", e.without_url())))?;
        if !r.status().is_success() {
            return Err(ZoomError::Transient(format!(
                "token http {}",
                r.status().as_u16()
            )));
        }
        let v: Value = r
            .json()
            .await
            .map_err(|_| ZoomError::Transient("token body".into()))?;
        let value = v
            .get("access_token")
            .and_then(Value::as_str)
            .ok_or_else(|| ZoomError::Transient("token missing".into()))?
            .to_string();
        let ttl = v.get("expires_in").and_then(Value::as_u64).unwrap_or(3600);
        *g = Some(Token {
            value: value.clone(),
            until: Instant::now() + Duration::from_secs(ttl.saturating_sub(60)),
        });
        Ok(value)
    }

    async fn get_json(&self, url: &str) -> Result<Value, ZoomError> {
        let tok = self.bearer().await?;
        let r = self
            .http
            .get(url)
            .bearer_auth(&tok)
            .send()
            .await
            .map_err(|e| ZoomError::Transient(format!("get: {}", e.without_url())))?;
        let st = r.status();
        if st.as_u16() == 401 {
            // トークンが失効していた。捨てて次の試行でやり直す
            *self.token.lock().await = None;
        }
        if !st.is_success() {
            return Err(classify_status(st.as_u16()));
        }
        r.json()
            .await
            .map_err(|_| ZoomError::Transient("body".into()))
    }
}

impl TranscriptSource for ZoomClient {
    async fn fetch(&self, call_log_id: &str) -> Result<Vec<TalkTurn>, ZoomError> {
        let rec = self
            .get_json(&format!(
                "https://api.zoom.us/v2/phone/call_logs/{call_log_id}/recordings"
            ))
            .await?;
        // 応答は録音 1 件のオブジェクト (試作は rec["id"])。配列で返る場合は先頭
        let rec = rec.as_array().and_then(|a| a.first()).unwrap_or(&rec);
        let id = rec
            .get("id")
            .and_then(Value::as_str)
            .ok_or(ZoomError::NotReady)?;
        let tx = self
            .get_json(&format!(
                "https://api.zoom.us/v2/phone/recording_transcript/download/{id}"
            ))
            .await?;
        let turns = turns_from(&tx);
        // 200 でも発言が 1 件も無ければ、文字起こしがまだ空 (後で取り直す)
        if turns.is_empty() {
            return Err(ZoomError::NotReady);
        }
        Ok(turns)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_classification() {
        assert_eq!(classify_status(400), ZoomError::NotReady);
        assert_eq!(classify_status(404), ZoomError::Gone);
        assert_eq!(
            classify_status(429),
            ZoomError::Transient("http 429".into())
        );
        assert_eq!(
            classify_status(503),
            ZoomError::Transient("http 503".into())
        );
    }
}
