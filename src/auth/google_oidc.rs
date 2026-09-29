//! Google Workspace OIDC ログイン (ADR-017 / 実装計画 PR1)
//!
//! 流れ:
//! 1. `GET /auth/google/login`: state / nonce / PKCE verifier を作り、専用の短命 Cookie
//!    (HttpOnly, SameSite=Lax, Path=/auth/google, 5 分, 本番は Secure) に入れて Google へ 303。
//! 2. Google → `GET /auth/google/callback?code=..&state=..`: Cookie の state と照合 →
//!    code を token endpoint で交換 (client secret と PKCE verifier はサーバだけが持つ) →
//!    ID token を検証 (署名 / iss / aud / exp / nonce / hd / email_verified / email ドメイン) →
//!    `accounts.disabled_at` を確認 → セッション確立 → **200 の HTML** から `/` へ遷移。
//!
//! なぜ state 等をセッションに入れないか (計画書 C-1):
//! 本番のセッション Cookie は `SameSite=Strict`。Google からのコールバックはクロスサイトの
//! トップレベル遷移なので Strict の Cookie は送られない。Lax の専用 Cookie なら送られる。
//! コールバックで 302 `/` を返すと、リダイレクト連鎖がクロスサイト起点とみなされて
//! 新しいセッション Cookie が付かないおそれがあるため、200 の HTML から同一サイト起点で遷移する。
//!
//! なぜ開始を GET リンクにするか (C-2): CSP `form-action 'self'` は form 送信後の
//! リダイレクト先にも適用されるブラウザがあり、POST → 302 accounts.google.com が止まりうる。
//!
//! 4 つの `GOOGLE_OIDC_*` のどれかが未設定なら `AppState.google_oidc = None` で、
//! ここのハンドラは 404 を返す。ログイン画面にもボタンを出さない。

use std::sync::Arc;

use axum::{
    extract::{Query, State},
    http::{header, HeaderMap, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Router,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use base64::Engine;
use jsonwebtoken::{jwk::JwkSet, Algorithm, DecodingKey, Validation};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::sync::RwLock;
use tower_sessions::cookie::{Cookie, SameSite};
use tower_sessions::Session;

use crate::config::GoogleOidcConfig;
use crate::AppState;

/// Google の Discovery document
pub const GOOGLE_DISCOVERY_URL: &str =
    "https://accounts.google.com/.well-known/openid-configuration";
pub const LOGIN_PATH: &str = "/auth/google/login";
pub const CALLBACK_PATH: &str = "/auth/google/callback";
/// state / nonce / PKCE verifier を運ぶ短命 Cookie
pub const TX_COOKIE_NAME: &str = "hrhr_oidc_tx";
pub const TX_COOKIE_PATH: &str = "/auth/google";
pub const TX_COOKIE_MAX_AGE_SECS: i64 = 300;

// ============================================================================
// ID token 検証 (純粋関数。単体テストの対象)
// ============================================================================

/// ID token 検証の失敗理由。監査ログの failure_reason にも使う (`as_reason`)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IdTokenError {
    /// JWT として読めない / 必須クレーム欠落 / alg が RS256 でない
    Malformed(String),
    /// ヘッダに kid が無い
    MissingKid,
    /// JWKS に kid が無い (再取得しても無い)
    UnknownKey(String),
    Signature,
    Expired,
    Audience,
    Issuer,
    Nonce,
    /// `hd` クレームが無い、または GOOGLE_OIDC_HOSTED_DOMAIN と違う
    HostedDomain,
    EmailNotVerified,
    /// email が無い、または email のドメインが GOOGLE_OIDC_HOSTED_DOMAIN と違う
    EmailDomain,
}

impl IdTokenError {
    pub fn as_reason(&self) -> &'static str {
        match self {
            Self::Malformed(_) => "oidc_malformed",
            Self::MissingKid => "oidc_missing_kid",
            Self::UnknownKey(_) => "oidc_unknown_key",
            Self::Signature => "oidc_signature",
            Self::Expired => "oidc_expired",
            Self::Audience => "oidc_audience",
            Self::Issuer => "oidc_issuer",
            Self::Nonce => "oidc_nonce",
            Self::HostedDomain => "oidc_hosted_domain",
            Self::EmailNotVerified => "oidc_email_not_verified",
            Self::EmailDomain => "oidc_email_domain",
        }
    }
}

impl std::fmt::Display for IdTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Malformed(m) => write!(f, "{}: {m}", self.as_reason()),
            Self::UnknownKey(k) => write!(f, "{}: kid={k}", self.as_reason()),
            _ => f.write_str(self.as_reason()),
        }
    }
}

/// 検証に通った本人情報
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerifiedIdentity {
    /// 小文字に正規化した email
    pub email: String,
    pub sub: String,
}

/// 照合に使う期待値
pub struct IdTokenCheck<'a> {
    pub client_id: &'a str,
    pub issuers: &'a [String],
    pub nonce: &'a str,
    /// 小文字 (例 `f-a-c.co.jp`)
    pub hosted_domain: &'a str,
}

#[derive(Debug, Deserialize)]
struct GoogleIdClaims {
    #[serde(default)]
    sub: String,
    #[serde(default)]
    email: Option<String>,
    /// Google は bool で返すが、古い実装で文字列 "true" の例があるので両方受ける
    #[serde(default)]
    email_verified: Option<serde_json::Value>,
    #[serde(default)]
    hd: Option<String>,
    #[serde(default)]
    nonce: Option<String>,
}

/// ID token を検証する。
///
/// 署名は `jwks` の kid 一致の鍵 (RS256) で、iss / aud / exp は jsonwebtoken に任せ
/// (exp の許容誤差は既定の 60 秒)、nonce / hd / email_verified / email ドメインをここで見る。
///
/// `hd` は **ID token のクレーム** を見る。認可 URL の `hd` パラメータは画面上の絞り込みでしかなく
/// 改ざんできるため、アクセス制御には使わない (Google 公式の注意書き)。
pub fn verify_id_token(
    id_token: &str,
    jwks: &JwkSet,
    check: &IdTokenCheck<'_>,
) -> Result<VerifiedIdentity, IdTokenError> {
    let header = jsonwebtoken::decode_header(id_token)
        .map_err(|e| IdTokenError::Malformed(e.to_string()))?;
    if header.alg != Algorithm::RS256 {
        return Err(IdTokenError::Malformed(format!("alg={:?}", header.alg)));
    }
    let kid = header.kid.ok_or(IdTokenError::MissingKid)?;
    let jwk = jwks
        .find(&kid)
        .ok_or_else(|| IdTokenError::UnknownKey(kid.clone()))?;
    let key = DecodingKey::from_jwk(jwk).map_err(|e| IdTokenError::Malformed(e.to_string()))?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_audience(&[check.client_id]);
    validation.set_issuer(check.issuers);
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);

    let claims = jsonwebtoken::decode::<GoogleIdClaims>(id_token, &key, &validation)
        .map_err(|e| {
            use jsonwebtoken::errors::ErrorKind;
            match e.kind() {
                ErrorKind::InvalidSignature => IdTokenError::Signature,
                ErrorKind::ExpiredSignature => IdTokenError::Expired,
                ErrorKind::InvalidAudience => IdTokenError::Audience,
                ErrorKind::InvalidIssuer => IdTokenError::Issuer,
                _ => IdTokenError::Malformed(e.to_string()),
            }
        })?
        .claims;

    if check.nonce.is_empty() || claims.nonce.as_deref() != Some(check.nonce) {
        return Err(IdTokenError::Nonce);
    }
    let hosted_domain = check.hosted_domain.to_lowercase();
    match claims.hd.as_deref() {
        Some(hd) if hd.eq_ignore_ascii_case(&hosted_domain) => {}
        _ => return Err(IdTokenError::HostedDomain),
    }
    let verified = match &claims.email_verified {
        Some(serde_json::Value::Bool(b)) => *b,
        Some(serde_json::Value::String(s)) => s == "true",
        _ => false,
    };
    if !verified {
        return Err(IdTokenError::EmailNotVerified);
    }
    let email = claims
        .email
        .as_deref()
        .map(|e| e.trim().to_lowercase())
        .unwrap_or_default();
    match email.rsplit_once('@') {
        Some((local, domain)) if !local.is_empty() && domain == hosted_domain => {}
        _ => return Err(IdTokenError::EmailDomain),
    }
    Ok(VerifiedIdentity {
        email,
        sub: claims.sub,
    })
}

// ============================================================================
// Google とのやり取り (Discovery / token 交換 / JWKS)
// ============================================================================

/// Discovery document のうち使う項目
#[derive(Debug, Clone, Deserialize)]
pub struct Endpoints {
    pub issuer: String,
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub jwks_uri: String,
}

impl Endpoints {
    /// Google の iss は `https://accounts.google.com` と `accounts.google.com` の 2 通り
    pub fn accepted_issuers(&self) -> Vec<String> {
        let mut v = vec![self.issuer.clone()];
        if self.issuer == "https://accounts.google.com" {
            v.push("accounts.google.com".to_string());
        }
        v
    }
}

/// フロー全体の失敗
#[derive(Debug)]
pub enum OidcError {
    /// Discovery / JWKS が取れない (Google 側または通信の障害)
    Upstream(String),
    /// code → token 交換の失敗
    TokenExchange(String),
    IdToken(IdTokenError),
}

impl std::fmt::Display for OidcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Upstream(m) => write!(f, "oidc upstream: {m}"),
            Self::TokenExchange(m) => write!(f, "oidc token exchange: {m}"),
            Self::IdToken(e) => write!(f, "oidc id token: {e}"),
        }
    }
}

/// OIDC の実行時状態。`AppState.google_oidc` に `Arc` で持つ。
///
/// Discovery と JWKS は最初のログイン時に取りに行き、メモリにキャッシュする
/// (起動時には通信しない。Google 側の障害で起動が止まらないように)。
/// JWKS は kid が見つからないときだけ取り直す (Google の鍵ローテーション対応)。
pub struct GoogleOidc {
    cfg: GoogleOidcConfig,
    discovery_url: String,
    http: reqwest::Client,
    endpoints: RwLock<Option<Endpoints>>,
    jwks: RwLock<Option<JwkSet>>,
}

impl GoogleOidc {
    pub fn new(cfg: GoogleOidcConfig) -> Self {
        Self::build(cfg, GOOGLE_DISCOVERY_URL.to_string(), None)
    }

    /// Discovery を使わず、エンドポイントを直接与える (テストの偽 Google 用)
    pub fn with_endpoints(cfg: GoogleOidcConfig, endpoints: Endpoints) -> Self {
        Self::build(cfg, String::new(), Some(endpoints))
    }

    fn build(cfg: GoogleOidcConfig, discovery_url: String, endpoints: Option<Endpoints>) -> Self {
        let http = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .unwrap_or_default();
        Self {
            cfg,
            discovery_url,
            http,
            endpoints: RwLock::new(endpoints),
            jwks: RwLock::new(None),
        }
    }

    pub fn config(&self) -> &GoogleOidcConfig {
        &self.cfg
    }

    async fn endpoints(&self) -> Result<Endpoints, OidcError> {
        if let Some(e) = self.endpoints.read().await.clone() {
            return Ok(e);
        }
        let fetched: Endpoints = self
            .http
            .get(&self.discovery_url)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| OidcError::Upstream(format!("discovery: {e}")))?
            .json()
            .await
            .map_err(|e| OidcError::Upstream(format!("discovery json: {e}")))?;
        *self.endpoints.write().await = Some(fetched.clone());
        Ok(fetched)
    }

    /// Google の認可画面の URL
    pub async fn authorization_url(&self, tx: &LoginTx) -> Result<String, OidcError> {
        let ep = self.endpoints().await?;
        let challenge = tx.code_challenge();
        let url = reqwest::Url::parse_with_params(
            &ep.authorization_endpoint,
            &[
                ("response_type", "code"),
                ("client_id", self.cfg.client_id.as_str()),
                ("redirect_uri", self.cfg.redirect_url.as_str()),
                ("scope", "openid email"),
                ("state", tx.state.as_str()),
                ("nonce", tx.nonce.as_str()),
                ("code_challenge", challenge.as_str()),
                ("code_challenge_method", "S256"),
                // 画面上の絞り込みのみ。アクセス制御は ID token の hd クレームで行う
                ("hd", self.cfg.hosted_domain.as_str()),
                ("prompt", "select_account"),
            ],
        )
        .map_err(|e| OidcError::Upstream(format!("authorization_endpoint: {e}")))?;
        Ok(url.to_string())
    }

    /// code を ID token に交換する (client secret と PKCE verifier を送る)
    pub async fn exchange_code(&self, code: &str, verifier: &str) -> Result<String, OidcError> {
        let ep = self.endpoints().await?;
        let body = [
            ("grant_type", "authorization_code"),
            ("code", code),
            ("client_id", self.cfg.client_id.as_str()),
            ("client_secret", self.cfg.client_secret.as_str()),
            ("redirect_uri", self.cfg.redirect_url.as_str()),
            ("code_verifier", verifier),
        ]
        .iter()
        .map(|(k, v)| format!("{k}={}", urlencoding::encode(v)))
        .collect::<Vec<_>>()
        .join("&");
        let resp = self
            .http
            .post(&ep.token_endpoint)
            .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
            .header(header::ACCEPT, "application/json")
            .body(body)
            .send()
            .await
            .map_err(|e| OidcError::TokenExchange(format!("request: {e}")))?;
        let status = resp.status();
        if !status.is_success() {
            // 応答本文には秘密は含まれないが、長さだけ抑える
            let text = resp.text().await.unwrap_or_default();
            return Err(OidcError::TokenExchange(format!(
                "status {status}: {}",
                text.chars().take(200).collect::<String>()
            )));
        }
        #[derive(Deserialize)]
        struct TokenResponse {
            id_token: Option<String>,
        }
        let tr: TokenResponse = resp
            .json()
            .await
            .map_err(|e| OidcError::TokenExchange(format!("json: {e}")))?;
        tr.id_token
            .filter(|t| !t.is_empty())
            .ok_or_else(|| OidcError::TokenExchange("id_token がない".to_string()))
    }

    async fn fetch_jwks(&self, jwks_uri: &str) -> Result<JwkSet, OidcError> {
        let set: JwkSet = self
            .http
            .get(jwks_uri)
            .send()
            .await
            .and_then(|r| r.error_for_status())
            .map_err(|e| OidcError::Upstream(format!("jwks: {e}")))?
            .json()
            .await
            .map_err(|e| OidcError::Upstream(format!("jwks json: {e}")))?;
        *self.jwks.write().await = Some(set.clone());
        Ok(set)
    }

    /// ID token を検証する。kid がキャッシュに無ければ JWKS を 1 回だけ取り直す。
    pub async fn verify(&self, id_token: &str, nonce: &str) -> Result<VerifiedIdentity, OidcError> {
        let ep = self.endpoints().await?;
        let issuers = ep.accepted_issuers();
        let check = IdTokenCheck {
            client_id: &self.cfg.client_id,
            issuers: &issuers,
            nonce,
            hosted_domain: &self.cfg.hosted_domain,
        };
        let cached = self.jwks.read().await.clone();
        let jwks = match cached {
            Some(set) => set,
            None => self.fetch_jwks(&ep.jwks_uri).await?,
        };
        match verify_id_token(id_token, &jwks, &check) {
            Err(IdTokenError::UnknownKey(_)) => {
                let fresh = self.fetch_jwks(&ep.jwks_uri).await?;
                verify_id_token(id_token, &fresh, &check).map_err(OidcError::IdToken)
            }
            other => other.map_err(OidcError::IdToken),
        }
    }
}

// ============================================================================
// state / nonce / PKCE verifier を運ぶ Cookie
// ============================================================================

/// 1 回のログイン試行の秘密値。いずれも 32 バイト乱数の base64url (43 文字、`.` を含まない)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LoginTx {
    pub state: String,
    pub nonce: String,
    pub verifier: String,
}

fn random_token() -> String {
    use rand::RngCore;
    let mut buf = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut buf);
    URL_SAFE_NO_PAD.encode(buf)
}

impl LoginTx {
    pub fn generate() -> Self {
        Self {
            state: random_token(),
            nonce: random_token(),
            verifier: random_token(),
        }
    }

    /// PKCE S256: base64url(SHA-256(verifier))
    pub fn code_challenge(&self) -> String {
        URL_SAFE_NO_PAD.encode(Sha256::digest(self.verifier.as_bytes()))
    }

    fn to_cookie_value(&self) -> String {
        format!("{}.{}.{}", self.state, self.nonce, self.verifier)
    }

    fn from_cookie_value(v: &str) -> Option<Self> {
        let mut it = v.split('.');
        let (state, nonce, verifier) = (it.next()?, it.next()?, it.next()?);
        if it.next().is_some() || [state, nonce, verifier].iter().any(|s| s.len() < 32) {
            return None;
        }
        Some(Self {
            state: state.to_string(),
            nonce: nonce.to_string(),
            verifier: verifier.to_string(),
        })
    }
}

fn tx_cookie(value: String, max_age_secs: i64) -> HeaderValue {
    let cookie = Cookie::build((TX_COOKIE_NAME, value))
        .http_only(true)
        .secure(crate::config::is_production_env())
        .same_site(SameSite::Lax)
        .path(TX_COOKIE_PATH)
        .max_age(time::Duration::seconds(max_age_secs))
        .build();
    HeaderValue::from_str(&cookie.to_string()).unwrap_or_else(|_| HeaderValue::from_static(""))
}

fn read_cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .flat_map(|s| s.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v.to_string())
}

// ============================================================================
// ハンドラ
// ============================================================================

/// `/auth/google/login` と `/auth/google/callback`。常に登録し、未設定時はハンドラが 404 を返す
/// (設定の有無でルートが変わると、ルート重複の検査が片方の構成でしか効かなくなるため)。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route(LOGIN_PATH, get(login_start))
        .route(CALLBACK_PATH, get(callback))
}

fn no_store(resp: &mut Response) {
    resp.headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
}

async fn login_start(State(state): State<Arc<AppState>>) -> Response {
    let Some(oidc) = state.google_oidc.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let tx = LoginTx::generate();
    match oidc.authorization_url(&tx).await {
        Ok(url) => {
            let mut resp = StatusCode::SEE_OTHER.into_response();
            match HeaderValue::from_str(&url) {
                Ok(loc) => {
                    resp.headers_mut().insert(header::LOCATION, loc);
                }
                Err(_) => {
                    return crate::render_login_status(
                        &state,
                        StatusCode::SERVICE_UNAVAILABLE,
                        "Google ログインを開始できませんでした。時間をおいて再度お試しください。",
                    );
                }
            }
            resp.headers_mut().append(
                header::SET_COOKIE,
                tx_cookie(tx.to_cookie_value(), TX_COOKIE_MAX_AGE_SECS),
            );
            no_store(&mut resp);
            resp
        }
        Err(e) => {
            tracing::warn!("google oidc login start failed: {e}");
            crate::render_login_status(
                &state,
                StatusCode::SERVICE_UNAVAILABLE,
                "Google ログインを開始できませんでした。時間をおいて再度お試しください。",
            )
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub struct CallbackQuery {
    code: Option<String>,
    state: Option<String>,
    error: Option<String>,
}

/// コールバック失敗時の応答内容
struct Fail {
    status: StatusCode,
    message: &'static str,
    /// Some なら監査の失敗ログに残す (attempted_email, reason)
    audit: Option<(String, String)>,
}

impl Fail {
    fn bad_request(message: &'static str) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            message,
            audit: None,
        }
    }
}

async fn callback(
    State(state): State<Arc<AppState>>,
    session: Session,
    req: axum::extract::Request,
) -> Response {
    let Some(oidc) = state.google_oidc.clone() else {
        return StatusCode::NOT_FOUND.into_response();
    };
    let client_ip = crate::request_client_ip(&req);
    let ua = crate::request_user_agent(&req);
    let query = Query::<CallbackQuery>::try_from_uri(req.uri())
        .map(|q| q.0)
        .unwrap_or_default();
    let tx = read_cookie(req.headers(), TX_COOKIE_NAME)
        .as_deref()
        .and_then(LoginTx::from_cookie_value);

    let mut resp = match run_callback(&state, &oidc, &session, query, tx, &client_ip, &ua).await {
        Ok(()) => login_success_page(),
        Err(fail) => {
            if let Some((email, reason)) = &fail.audit {
                tracing::warn!("GOOGLE_LOGIN_FAILED: email={email}, reason={reason}");
                crate::record_failed_login(
                    &state,
                    email,
                    &client_ip,
                    &ua,
                    crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
                    reason,
                )
                .await;
            }
            crate::render_login_status(&state, fail.status, fail.message)
        }
    };
    // 成否にかかわらず、使い終わった state/nonce/verifier の Cookie は消す (再利用させない)
    resp.headers_mut()
        .append(header::SET_COOKIE, tx_cookie(String::new(), 0));
    no_store(&mut resp);
    resp
}

async fn run_callback(
    state: &AppState,
    oidc: &GoogleOidc,
    session: &Session,
    query: CallbackQuery,
    tx: Option<LoginTx>,
    client_ip: &str,
    ua: &str,
) -> Result<(), Fail> {
    if query.error.is_some() {
        return Err(Fail::bad_request(
            "Google ログインがキャンセルされたか、許可されませんでした。",
        ));
    }
    let Some(tx) = tx else {
        return Err(Fail::bad_request(
            "ログインの有効期限 (5 分) が切れたか、Cookie が無効です。もう一度お試しください。",
        ));
    };
    if query.state.as_deref() != Some(tx.state.as_str()) {
        return Err(Fail::bad_request(
            "ログイン要求を確認できませんでした (state 不一致)。もう一度お試しください。",
        ));
    }
    let Some(code) = query.code.filter(|c| !c.is_empty()) else {
        return Err(Fail::bad_request(
            "Google から認可コードが返りませんでした。もう一度お試しください。",
        ));
    };

    let upstream_fail = |e: OidcError| {
        tracing::warn!("google oidc callback failed: {e}");
        Fail {
            status: match e {
                OidcError::TokenExchange(_) => StatusCode::BAD_GATEWAY,
                _ => StatusCode::SERVICE_UNAVAILABLE,
            },
            message: "Google との通信に失敗しました。時間をおいて再度お試しください。",
            audit: None,
        }
    };
    let id_token = oidc
        .exchange_code(&code, &tx.verifier)
        .await
        .map_err(upstream_fail)?;
    let identity = match oidc.verify(&id_token, &tx.nonce).await {
        Ok(id) => id,
        Err(OidcError::IdToken(e)) => {
            return Err(Fail {
                status: StatusCode::FORBIDDEN,
                message: "このアカウントではログインできません (会社の Google Workspace アカウントでログインしてください)。",
                audit: Some((unverified_email(&id_token), e.as_reason().to_string())),
            });
        }
        Err(e) => return Err(upstream_fail(e)),
    };

    if crate::account_is_disabled(state, &identity.email).await {
        return Err(Fail {
            status: StatusCode::FORBIDDEN,
            message: "このアカウントは無効化されています。管理者にお問い合わせください。",
            audit: Some((identity.email, "account_disabled".to_string())),
        });
    }

    tracing::info!("GOOGLE_LOGIN_SUCCESS: email={}", identity.email);
    crate::complete_login(
        state,
        session,
        &identity.email,
        crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
        client_ip,
        ua,
    )
    .await;
    Ok(())
}

/// 監査の attempted_email 用。検証に失敗した token の email を「未検証の値」として読む。
/// 認可判断には使わない。
fn unverified_email(id_token: &str) -> String {
    id_token
        .split('.')
        .nth(1)
        .and_then(|p| URL_SAFE_NO_PAD.decode(p).ok())
        .and_then(|b| serde_json::from_slice::<serde_json::Value>(&b).ok())
        .and_then(|v| v.get("email").and_then(|e| e.as_str()).map(str::to_string))
        .unwrap_or_default()
}

/// セッション確立後に返す 200 の HTML。
/// 同一サイトの文書から `/` へ遷移させ、SameSite=Strict のセッション Cookie が付くようにする (C-1)。
fn login_success_page() -> Response {
    let html = r#"<!DOCTYPE html>
<html lang="ja">
<head>
<meta charset="UTF-8">
<meta http-equiv="refresh" content="0;url=/">
<title>ログインしています</title>
</head>
<body>
<p>ログインしました。自動で移動しない場合は <a href="/">こちら</a> を押してください。</p>
<script>location.replace("/");</script>
</body>
</html>"#;
    (StatusCode::OK, axum::response::Html(html)).into_response()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use jsonwebtoken::{EncodingKey, Header};

    pub(crate) const KEY_1_PEM: &str = include_str!("../../tests/fixtures/oidc/test_key_1.pem");
    pub(crate) const KEY_2_PEM: &str = include_str!("../../tests/fixtures/oidc/test_key_2.pem");
    /// 偽 Google が公開する JWKS。test-key-1 の公開鍵だけが入っている
    pub(crate) const JWKS_JSON: &str = include_str!("../../tests/fixtures/oidc/jwks.json");

    pub(crate) const CLIENT_ID: &str = "test-client.apps.googleusercontent.com";
    const NONCE: &str = "nonce-0123456789abcdef0123456789abcdef";

    fn now() -> i64 {
        chrono::Utc::now().timestamp()
    }

    fn good_claims() -> serde_json::Value {
        serde_json::json!({
            "iss": "https://accounts.google.com",
            "aud": CLIENT_ID,
            "sub": "1234567890",
            "email": "taro@f-a-c.co.jp",
            "email_verified": true,
            "hd": "f-a-c.co.jp",
            "nonce": NONCE,
            "iat": now(),
            "exp": now() + 3600,
        })
    }

    pub(crate) fn sign(claims: &serde_json::Value, pem: &str, kid: &str) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some(kid.to_string());
        let key = EncodingKey::from_rsa_pem(pem.as_bytes()).expect("test key");
        jsonwebtoken::encode(&header, claims, &key).expect("sign")
    }

    fn verify(token: &str) -> Result<VerifiedIdentity, IdTokenError> {
        let jwks: JwkSet = serde_json::from_str(JWKS_JSON).unwrap();
        let issuers = vec![
            "https://accounts.google.com".to_string(),
            "accounts.google.com".to_string(),
        ];
        verify_id_token(
            token,
            &jwks,
            &IdTokenCheck {
                client_id: CLIENT_ID,
                issuers: &issuers,
                nonce: NONCE,
                hosted_domain: "f-a-c.co.jp",
            },
        )
    }

    fn with(
        mut claims: serde_json::Value,
        key: &str,
        value: serde_json::Value,
    ) -> serde_json::Value {
        claims[key] = value;
        claims
    }

    #[test]
    fn 正常なid_tokenでemailが返る() {
        let token = sign(&good_claims(), KEY_1_PEM, "test-key-1");
        let id = verify(&token).expect("正常系は通る");
        assert_eq!(id.email, "taro@f-a-c.co.jp");
        assert_eq!(id.sub, "1234567890");
    }

    #[test]
    fn iss_が短い形でも通る() {
        let claims = with(good_claims(), "iss", "accounts.google.com".into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token).unwrap().email, "taro@f-a-c.co.jp");
    }

    #[test]
    fn hd_欠落は拒否() {
        let mut claims = good_claims();
        claims.as_object_mut().unwrap().remove("hd");
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::HostedDomain));
    }

    #[test]
    fn hd_が_gmail_は拒否() {
        let claims = with(good_claims(), "hd", "gmail.com".into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::HostedDomain));
    }

    #[test]
    fn aud_不一致は拒否() {
        let claims = with(
            good_claims(),
            "aud",
            "other-client.apps.googleusercontent.com".into(),
        );
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::Audience));
    }

    #[test]
    fn exp_過去は拒否() {
        // 既定の許容誤差 60 秒より十分前
        let claims = with(good_claims(), "exp", (now() - 3600).into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::Expired));
    }

    #[test]
    fn email_verified_false_は拒否() {
        let claims = with(good_claims(), "email_verified", false.into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::EmailNotVerified));
    }

    #[test]
    fn nonce_不一致は拒否() {
        let claims = with(good_claims(), "nonce", "another-nonce".into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::Nonce));
    }

    #[test]
    fn 別の鍵で署名したものは拒否() {
        // kid は正規の鍵を名乗るが、実際は JWKS に無い鍵 (test-key-2) で署名
        let token = sign(&good_claims(), KEY_2_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::Signature));
    }

    #[test]
    fn iss_不正は拒否() {
        let claims = with(good_claims(), "iss", "https://evil.example".into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::Issuer));
    }

    #[test]
    fn email_のドメインが_hd_と違えば拒否() {
        let claims = with(good_claims(), "email", "taro@gmail.com".into());
        let token = sign(&claims, KEY_1_PEM, "test-key-1");
        assert_eq!(verify(&token), Err(IdTokenError::EmailDomain));
    }

    #[test]
    fn jwks_に無い_kid_は_unknown_key() {
        let token = sign(&good_claims(), KEY_2_PEM, "test-key-2");
        assert_eq!(
            verify(&token),
            Err(IdTokenError::UnknownKey("test-key-2".to_string()))
        );
    }

    #[test]
    fn pkce_challenge_は_sha256_の_base64url() {
        // 期待値は Python で別途計算:
        // base64.urlsafe_b64encode(hashlib.sha256(v).digest()).rstrip(b'=')
        let tx = LoginTx {
            state: String::new(),
            nonce: String::new(),
            verifier: "test-verifier-0123456789-abcdefghijklmnopqrstuv".to_string(),
        };
        assert_eq!(
            tx.code_challenge(),
            "680IdP-aUA00b-bdVxY-BHXXHFEgT6Uuiw8M7OiG3RQ"
        );
    }

    #[test]
    fn tx_cookie_の往復と不正値() {
        let tx = LoginTx::generate();
        assert_eq!(tx.state.len(), 43);
        assert_ne!(tx.state, tx.nonce);
        let v = tx.to_cookie_value();
        assert_eq!(LoginTx::from_cookie_value(&v), Some(tx));
        assert_eq!(LoginTx::from_cookie_value(""), None);
        assert_eq!(LoginTx::from_cookie_value("a.b.c"), None);
        assert_eq!(LoginTx::from_cookie_value(&format!("{v}.extra")), None);
    }

    #[test]
    fn tx_cookie_の属性() {
        let v = tx_cookie("abc".to_string(), TX_COOKIE_MAX_AGE_SECS);
        let s = v.to_str().unwrap();
        assert!(s.starts_with("hrhr_oidc_tx=abc"), "{s}");
        assert!(s.contains("HttpOnly"), "{s}");
        assert!(s.contains("SameSite=Lax"), "{s}");
        assert!(s.contains("Path=/auth/google"), "{s}");
        assert!(s.contains("Max-Age=300"), "{s}");
    }
}
