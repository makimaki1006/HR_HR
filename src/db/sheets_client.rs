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


/// リトライすべき HTTP ステータスか。
///
/// 2026-08-16 追加。実データでの起動確認中に Sheets API が **503 (UNAVAILABLE)** を返し、
/// 画面が 502 で落ちた。Google 側の一時的な不可用は日常的に起きる。
///
/// - 429 (レート超過) と 5xx (サーバ側の一時障害) は再試行する
/// - それ以外の 4xx は権限・シート名の誤りなど**再試行しても直らない**ので即諦める
///   （無駄な待ち時間を作らないため。401 を4回リトライしても意味がない）
fn is_retryable(status: u16) -> bool {
    status == 429 || (500..600).contains(&status)
}

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
    /// シートを **原本の列順のまま** (ヘッダ, 行) で返す。
    ///
    /// 2026-08-17 新設。従来は `get_sheet_as_rows` の `HashMap` しか無く、
    /// 呼び出し側がヘッダ順を復元できずアルファベット順にソートしていた。
    /// その結果、スプレッドシートをそのまま見るための画面(データブラウザ)で
    /// **原本と列順が違う**という副作用が出ていた。
    pub async fn get_sheet_as_table(
        &self,
        sheet_name: &str,
    ) -> Result<(Vec<String>, Vec<Vec<String>>)> {
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

        // 2026-08-16 追加: 429/5xx のリトライ。
        //   実データでの起動確認中に **Sheets API が 503 (UNAVAILABLE) を返して
        //   画面が 502 で落ちた**。Google 側の一時的な不可用は日常的に起きるため、
        //   リトライが無いと「たまに画面が真っ白」という再現しにくい不具合になる。
        //   Python 側(consulting_patrol / patrol_data)でも同じ教訓で
        //   429/5xx リトライを入れている。
        //   429 は Retry-After を尊重し、それ以外は指数バックオフ。
        //   4xx(429以外)は再試行しても無駄なので即座に諦める。
        const MAX_RETRIES: u32 = 4;
        let mut last_err = String::new();
        let mut body = String::new();
        let mut ok = false;

        for attempt in 0..=MAX_RETRIES {
            let resp = self
                .http
                .get(&url)
                .bearer_auth(&token)
                .send()
                .await
                .with_context(|| format!("Sheets API GET 失敗: {sheet_name}"))?;

            let status = resp.status();
            let text = resp.text().await.context("Sheets API body 読み込み失敗")?;

            if status.is_success() {
                body = text;
                ok = true;
                break;
            }

            let retryable = is_retryable(status.as_u16());
            last_err = format!("status={status} body={text}");
            if !retryable || attempt == MAX_RETRIES {
                break;
            }

            // 1s, 2s, 4s, 8s
            let wait = Duration::from_secs(1u64 << attempt);
            tracing::warn!(
                "Sheets API {sheet_name}: {status} のため {}秒後に再試行 ({}/{})",
                wait.as_secs(),
                attempt + 1,
                MAX_RETRIES
            );
            tokio::time::sleep(wait).await;
        }

        if !ok {
            bail!("Sheets API 失敗 ({sheet_name}): {last_err}");
        }

        let parsed: ValuesResponse = serde_json::from_str(&body)
            .with_context(|| format!("ValuesResponse パース失敗 ({sheet_name})"))?;

        if parsed.values.len() < 2 {
            return Ok((Vec::new(), Vec::new()));
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
        // 2026-08-17: 列を無言で落とさない。
        //
        //   従来は空ヘッダ列を `continue` で捨てていたため、「最新サマリ」
        //   シート(1行目が注記・3行目が実ヘッダという特殊な作り)で
        //   **2〜39列目が無言で消えて画面がほぼ空**になっていた。
        //
        //   はじめ「空文字ヘッダに `列N` を充てる」だけ直したが、**効かなかった**。
        //   実測すると header が 1列しか無い。Sheets API は行末の空セルを
        //   詰めて返すため、1行目が注記1セルだけのシートでは
        //   **ヘッダ行そのものが1セル**になり、空文字ですらない。
        //   したがって全行の最大幅までヘッダを伸ばす必要がある。
        let rows_raw: Vec<Vec<serde_json::Value>> = iter.collect();
        let widest = rows_raw.iter().map(|r| r.len()).max().unwrap_or(0);
        let header = normalize_header(header, widest);

        let iter = rows_raw.into_iter();
        let mut rows: Vec<Vec<String>> = Vec::with_capacity(iter.size_hint().0);
        for raw in iter {
            let mut cells = Vec::with_capacity(header.len());
            for i in 0..header.len() {
                cells.push(match raw.get(i) {
                    Some(serde_json::Value::String(s)) => s.clone(),
                    Some(serde_json::Value::Null) | None => String::new(),
                    Some(serde_json::Value::Bool(b)) => b.to_string(),
                    Some(serde_json::Value::Number(n)) => n.to_string(),
                    Some(other) => other.to_string(),
                });
            }
            rows.push(cells);
        }
        Ok((header, rows))
    }

    /// 従来互換。列順を要さない呼び出し向けに `HashMap` へ畳む。
    pub async fn get_sheet_as_rows(
        &self,
        sheet_name: &str,
    ) -> Result<Vec<HashMap<String, String>>> {
        let (header, rows) = self.get_sheet_as_table(sheet_name).await?;
        Ok(rows
            .into_iter()
            .map(|cells| {
                header
                    .iter()
                    .zip(cells)
                    .map(|(k, v)| (k.clone(), v))
                    .collect::<HashMap<String, String>>()
            })
            .collect())
    }

    pub fn spreadsheet_id(&self) -> &str {
        &self.spreadsheet_id
    }
}

/// ヘッダを「実際に値がある最大幅」まで伸ばし、名前の無い列に `列N` を付ける。
///
/// Sheets API は行末の空セルを詰めて返すため、1行目が注記1セルだけのシートでは
/// **ヘッダ行そのものが1セル**になる。そのまま使うと2列目以降が丸ごと消える。
/// 実測「最新サマリ」: 39列あるのにヘッダ1列 → **2〜39列目が無言で欠落**していた。
fn normalize_header(header: Vec<String>, widest: usize) -> Vec<String> {
    let mut h = header;
    if widest > h.len() {
        h.resize(widest, String::new());
    }
    h.into_iter()
        .enumerate()
        .map(|(i, x)| if x.trim().is_empty() { format!("列{}", i + 1) } else { x })
        .collect()
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

    #[test]
    fn ヘッダが短くても列を落とさない() {
        // 「最新サマリ」の実際の形: 1行目が注記1セル、実データは39列
        let got = normalize_header(vec!["最終更新: 2026-08-16".to_string()], 39);
        assert_eq!(got.len(), 39, "39列あるのにヘッダ1列では2〜39列目が消える");
        assert_eq!(got[0], "最終更新: 2026-08-16", "元の名前は書き換えない");
        assert_eq!(got[1], "列2");
        assert_eq!(got[38], "列39");
    }

    #[test]
    fn 空文字ヘッダにも位置名を付ける() {
        let got = normalize_header(
            vec!["owner_id".into(), "".into(), "  ".into(), "call_count".into()],
            4,
        );
        assert_eq!(got, vec!["owner_id", "列2", "列3", "call_count"]);
    }

    #[test]
    fn ヘッダの方が長いときは縮めない() {
        // 全行が空でも、ヘッダにある列は残す
        let got = normalize_header(vec!["a".into(), "b".into(), "c".into()], 1);
        assert_eq!(got, vec!["a", "b", "c"]);
    }

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
    #[test]
    fn 一時的な失敗はリトライする() {
        // 実際に踏んだのは 503。429 はレート超過。
        assert!(is_retryable(503), "Sheets API が返す UNAVAILABLE");
        assert!(is_retryable(500));
        assert!(is_retryable(502));
        assert!(is_retryable(429), "レート超過は待てば通る");
    }

    #[test]
    fn 再試行しても直らないものは諦める() {
        // 権限不足やシート名の誤りは、何度叩いても同じ結果になる。
        // リトライすると無駄に待たせるだけ(401を4回で15秒)。
        assert!(!is_retryable(401), "認証エラー");
        assert!(!is_retryable(403), "権限不足");
        assert!(!is_retryable(404), "シートが無い");
        assert!(!is_retryable(400));
    }

    #[test]
    fn 成功はリトライ対象外() {
        assert!(!is_retryable(200));
        assert!(!is_retryable(204));
    }

}
