//! Google カレンダーへの書き込みと、管理用カレンダーのアカウントの許可 (ADR-037)。
//!
//! - 許可は監査 Turso の表 `applicant_calendar_grants` に 1 行。範囲は `calendar.events` だけ。
//! - refresh token は CRM と同じ鍵 (`CRM_EMAIL_TOKEN_KEY`) の AES-256-GCM で暗号化し、AAD を
//!   「用途の文字 + アドレス」にする (CRM・顧客画面の送信元の行と取り違えて復号できない)。
//! - OAuth の手続き (state・PKCE・nonce・token の交換) は CRM の `EmailService` を使い回す。
//!   OAuth クライアントは CRM と同じ選び方 (`CRM_EMAIL_GOOGLE_CLIENT_ID` / `_SECRET` があればそれ)。
//! - Turso への書き込みは許可の保存・取り消しのときだけ。同期の状態はプロセス内だけ。
use std::future::Future;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use chrono::{DateTime, SecondsFormat, Utc};
use serde_json::Value;

use super::{CalendarConfig, MARK_KEY, MARK_VALUE};
use crate::audit::AuditDb;
use crate::crm::email::{grants, EmailConfig, EmailError, EmailService};
use crate::crm::pending;
use crate::db::turso_http::TursoDb;
use crate::handlers::helpers::get_str;

/// 許可の範囲 (予定の読み書きだけ。カレンダーの設定・共有は触らない)
pub const SCOPE: &str = "https://www.googleapis.com/auth/calendar.events";
/// 許可の戻り先 (Google の OAuth クライアントにこの URL を登録する)
pub const CALLBACK: &str = "/api/admin/applicant-calendar/callback";
const CALENDAR_BASE: &str = "https://www.googleapis.com/calendar/v3";
/// access token を期限より早めに取り直す余裕
const TOKEN_MARGIN: Duration = Duration::from_secs(120);
const DEFAULT_TOKEN_LIFE: Duration = Duration::from_secs(3000);

/// 暗号化の AAD。CRM の行 (AAD = アドレスだけ)・顧客画面の送信元の行と取り違えないよう用途の文字を足す
pub fn aad(account: &str) -> String {
    format!("applicant-calendar:{account}")
}

// ---- 保存 (監査 Turso) ----

pub struct GrantRow {
    pub encrypted: String,
    pub granted_at: String,
}

/// 取り消されていない許可。範囲が足りない行は無いものとして扱う
pub fn get(t: &TursoDb, account: &str) -> Result<Option<GrantRow>, String> {
    let rows = t.query(
        "SELECT encrypted_refresh_token, scopes, granted_at FROM applicant_calendar_grants WHERE account_email = ?1 AND revoked_at IS NULL",
        &[&account],
    )?;
    Ok(rows
        .first()
        .filter(|r| get_str(r, "scopes").split_whitespace().any(|s| s == SCOPE))
        .map(|r| GrantRow {
            encrypted: get_str(r, "encrypted_refresh_token"),
            granted_at: get_str(r, "granted_at"),
        })
        .filter(|g| !g.encrypted.is_empty()))
}

pub fn save(t: &TursoDb, account: &str, encrypted: &str, scopes: &str) -> Result<(), String> {
    let now = pending::now_iso();
    t.execute(
        "INSERT INTO applicant_calendar_grants (account_email, encrypted_refresh_token, scopes, granted_at, revoked_at) VALUES (?1, ?2, ?3, ?4, NULL) ON CONFLICT(account_email) DO UPDATE SET encrypted_refresh_token = excluded.encrypted_refresh_token, scopes = excluded.scopes, granted_at = excluded.granted_at, revoked_at = NULL",
        &[&account, &encrypted, &scopes, &now],
    )
}

pub fn revoke(t: &TursoDb, account: &str) -> Result<(), String> {
    let now = pending::now_iso();
    t.execute(
        "UPDATE applicant_calendar_grants SET revoked_at = ?1, encrypted_refresh_token = '' WHERE account_email = ?2",
        &[&now, &account],
    )
}

// ---- Calendar API ----

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CalError {
    Network,
    /// 401 (access token が切れた)
    Auth,
    /// 429 / 403 の上限
    RateLimited,
    Status(u16),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Inserted {
    Created,
    /// 同じ ID の予定がすでにある (削除済みでも 409)
    Exists,
}

/// Google カレンダー (テストで偽物に差し替える)。作成・上書き・削除は招待メールを送らない (`sendUpdates=none`)
pub trait CalendarApi: Send + Sync {
    /// この機能が作った予定 (拡張プロパティの印つき) のうち、`time_min` より後に終わるもの (削除済みは含めない)
    fn list_ours(
        &self,
        time_min: DateTime<Utc>,
    ) -> impl Future<Output = Result<Vec<Value>, CalError>> + Send;
    /// 無ければ None (404 / 410)
    fn get(&self, id: &str) -> impl Future<Output = Result<Option<Value>, CalError>> + Send;
    fn insert(&self, body: &Value) -> impl Future<Output = Result<Inserted, CalError>> + Send;
    fn patch(&self, id: &str, body: &Value) -> impl Future<Output = Result<(), CalError>> + Send;
    /// 無ければ何もしない (404 / 410 も成功)
    fn delete(&self, id: &str) -> impl Future<Output = Result<(), CalError>> + Send;
}

pub struct GoogleCalendar {
    http: reqwest::Client,
    base: String,
    calendar_id: String,
    token: String,
}

impl std::fmt::Debug for GoogleCalendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GoogleCalendar { token: [redacted] }")
    }
}

async fn status_error(r: reqwest::Response) -> CalError {
    let status = r.status().as_u16();
    match status {
        401 => CalError::Auth,
        429 => CalError::RateLimited,
        403 => {
            // 上限 (rateLimitExceeded など) かどうかだけ見る。本文はログに出さない
            let body = r.text().await.unwrap_or_default();
            if body.contains("ateLimitExceeded") || body.contains("quotaExceeded") {
                CalError::RateLimited
            } else {
                CalError::Status(403)
            }
        }
        s => CalError::Status(s),
    }
}

impl GoogleCalendar {
    fn events_url(&self) -> String {
        format!(
            "{}/calendars/{}/events",
            self.base,
            urlencoding::encode(&self.calendar_id)
        )
    }
}

impl CalendarApi for GoogleCalendar {
    async fn list_ours(&self, time_min: DateTime<Utc>) -> Result<Vec<Value>, CalError> {
        let mut out = Vec::new();
        let mut page: Option<String> = None;
        for _ in 0..40 {
            let mut q: Vec<(&str, String)> = vec![
                (
                    "privateExtendedProperty",
                    format!("{MARK_KEY}={MARK_VALUE}"),
                ),
                (
                    "timeMin",
                    time_min.to_rfc3339_opts(SecondsFormat::Secs, true),
                ),
                ("maxResults", "2500".into()),
                ("showDeleted", "false".into()),
            ];
            if let Some(p) = &page {
                q.push(("pageToken", p.clone()));
            }
            let r = self
                .http
                .get(self.events_url())
                .bearer_auth(&self.token)
                .query(&q)
                .send()
                .await
                .map_err(|_| CalError::Network)?;
            if !r.status().is_success() {
                return Err(status_error(r).await);
            }
            let v: Value = r.json().await.map_err(|_| CalError::Network)?;
            out.extend(v["items"].as_array().into_iter().flatten().cloned());
            page = v["nextPageToken"].as_str().map(str::to_string);
            if page.is_none() {
                return Ok(out);
            }
        }
        // 予定が多すぎる (10 万件超)。途中までで消す判断をしないよう失敗にする
        Err(CalError::Status(0))
    }

    async fn get(&self, id: &str) -> Result<Option<Value>, CalError> {
        let r = self
            .http
            .get(format!("{}/{id}", self.events_url()))
            .bearer_auth(&self.token)
            .send()
            .await
            .map_err(|_| CalError::Network)?;
        match r.status().as_u16() {
            404 | 410 => Ok(None),
            s if (200..300).contains(&s) => r.json().await.map(Some).map_err(|_| CalError::Network),
            _ => Err(status_error(r).await),
        }
    }

    async fn insert(&self, body: &Value) -> Result<Inserted, CalError> {
        let r = self
            .http
            .post(self.events_url())
            .bearer_auth(&self.token)
            .query(&[("sendUpdates", "none")])
            .json(body)
            .send()
            .await
            .map_err(|_| CalError::Network)?;
        match r.status().as_u16() {
            409 => Ok(Inserted::Exists),
            s if (200..300).contains(&s) => Ok(Inserted::Created),
            _ => Err(status_error(r).await),
        }
    }

    async fn patch(&self, id: &str, body: &Value) -> Result<(), CalError> {
        let r = self
            .http
            .patch(format!("{}/{id}", self.events_url()))
            .bearer_auth(&self.token)
            .query(&[("sendUpdates", "none")])
            .json(body)
            .send()
            .await
            .map_err(|_| CalError::Network)?;
        if r.status().is_success() {
            Ok(())
        } else {
            Err(status_error(r).await)
        }
    }

    async fn delete(&self, id: &str) -> Result<(), CalError> {
        let r = self
            .http
            .delete(format!("{}/{id}", self.events_url()))
            .bearer_auth(&self.token)
            .query(&[("sendUpdates", "none")])
            .send()
            .await
            .map_err(|_| CalError::Network)?;
        match r.status().as_u16() {
            404 | 410 => Ok(()),
            s if (200..300).contains(&s) => Ok(()),
            _ => Err(status_error(r).await),
        }
    }
}

// ---- 許可と状態 ----

/// 同期できない理由 (管理画面に平易な言葉で出す)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Problem {
    NoGrant,
    Expired,
    Storage,
    Google,
    HubSpot,
}

impl Problem {
    pub fn message(self) -> &'static str {
        match self {
            Self::NoGrant => "まだ許可されていないので、カレンダーに載せていません。",
            Self::Expired => "許可が切れています。管理用カレンダーの人がもう一度許可してください。",
            Self::Storage => "許可の情報を読めませんでした。少し待つと戻ることがあります。",
            Self::Google => "Google カレンダーに書き込めませんでした。次の周回でやり直します。",
            Self::HubSpot => "HubSpot の応募を読めませんでした。次の周回でやり直します。",
        }
    }
}

/// 周回の結果 (プロセス内だけ)
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Counts {
    pub created: u32,
    pub updated: u32,
    pub deleted: u32,
    pub unchanged: u32,
    pub errors: u32,
}

#[derive(Debug, Clone, Default)]
pub struct SyncStatus {
    pub last_sync_at: Option<DateTime<Utc>>,
    pub last: Counts,
    pub errors_since_start: u64,
    pub problem: Option<Problem>,
}

pub struct ApplicantCalendar {
    pub cfg: CalendarConfig,
    account: String,
    email: Arc<EmailService>,
    audit: AuditDb,
    http: reqwest::Client,
    calendar_base: String,
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    expired: AtomicBool,
    status: Mutex<SyncStatus>,
}

impl std::fmt::Debug for ApplicantCalendar {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApplicantCalendar { account: [redacted], token: [redacted] }")
    }
}

static GLOBAL: OnceLock<Option<Arc<ApplicantCalendar>>> = OnceLock::new();

impl ApplicantCalendar {
    /// プロセスで 1 つ (背景の周回と管理画面で共有する)。設定がそろわなければ None
    pub fn global(audit: Option<AuditDb>) -> Option<Arc<Self>> {
        GLOBAL.get_or_init(|| Self::from_env(audit)).clone()
    }

    fn from_env(audit: Option<AuditDb>) -> Option<Arc<Self>> {
        let cfg = CalendarConfig::from_env();
        let email = EmailConfig::from_env_with_redirect(cfg.redirect_url.clone(), CALLBACK)
            .map(|c| Arc::new(EmailService::new(c)));
        Self::from_config(cfg, email, audit)
    }

    pub fn from_config(
        cfg: CalendarConfig,
        email: Option<Arc<EmailService>>,
        audit: Option<AuditDb>,
    ) -> Option<Arc<Self>> {
        let Some(account) = cfg.account.clone() else {
            if cfg.enabled {
                tracing::warn!("[calendar] APPLICANT_CALENDAR_ACCOUNT が未設定か形が違うので、応募の日付をカレンダーに載せない");
            }
            return None;
        };
        let Some(email) = email else {
            tracing::warn!("[calendar] 許可の設定 (CRM_EMAIL_TOKEN_KEY・Google の設定・APPLICANT_CALENDAR_REDIRECT_URL) がそろわないので、応募の日付をカレンダーに載せない");
            return None;
        };
        let Some(audit) = audit else {
            tracing::warn!("[calendar] 監査 DB が無いので、応募の日付をカレンダーに載せない");
            return None;
        };
        Some(Arc::new(Self {
            cfg,
            account,
            email,
            audit,
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .build()
                .unwrap_or_default(),
            calendar_base: CALENDAR_BASE.into(),
            token: tokio::sync::Mutex::new(None),
            expired: AtomicBool::new(false),
            status: Mutex::new(SyncStatus::default()),
        }))
    }

    /// テスト用: 偽の Calendar API に向ける
    #[cfg(test)]
    pub fn with_calendar_base(mut self: Arc<Self>, base: &str) -> Arc<Self> {
        Arc::get_mut(&mut self).expect("共有前").calendar_base = base.into();
        self
    }

    pub fn account(&self) -> &str {
        &self.account
    }

    pub(crate) fn service(&self) -> &EmailService {
        &self.email
    }

    pub(crate) fn audit(&self) -> &AuditDb {
        &self.audit
    }

    pub fn expired(&self) -> bool {
        self.expired.load(Ordering::Acquire)
    }

    pub fn status(&self) -> SyncStatus {
        self.status.lock().map(|s| s.clone()).unwrap_or_default()
    }

    pub(crate) fn record(&self, counts: Counts, problem: Option<Problem>, finished: bool) {
        if let Ok(mut s) = self.status.lock() {
            s.errors_since_start += u64::from(counts.errors);
            s.last = counts;
            s.problem = problem;
            if finished {
                s.last_sync_at = Some(Utc::now());
            }
        }
    }

    /// 保存してある許可 (取り消されていないもの)
    pub async fn grant_row(&self) -> Result<Option<GrantRow>, String> {
        let who = self.account.clone();
        pending::blocking(&self.audit, move |t| get(t, &who)).await?
    }

    /// 許可を保存したあと (古い access token と「切れ」の印を捨てる)
    pub async fn granted(&self) {
        *self.token.lock().await = None;
        self.expired.store(false, Ordering::Release);
        if let Ok(mut s) = self.status.lock() {
            if matches!(s.problem, Some(Problem::NoGrant | Problem::Expired)) {
                s.problem = None;
            }
        }
    }

    /// access token が 401 で断られたとき (次は取り直す)
    pub async fn forget_token(&self) {
        *self.token.lock().await = None;
    }

    /// 許可を確かめ、期限内の access token を返す (切れていれば取り直す)
    pub async fn access(&self) -> Result<String, Problem> {
        let row = self
            .grant_row()
            .await
            .map_err(|_| Problem::Storage)?
            .ok_or(Problem::NoGrant)?;
        let refresh = grants::decrypt(&self.email.cfg.key, &aad(&self.account), &row.encrypted)
            .map_err(|_| Problem::Storage)?;
        let mut slot = self.token.lock().await;
        if let Some((token, until)) = slot.as_ref() {
            if Instant::now() + TOKEN_MARGIN < *until {
                return Ok(token.clone());
            }
        }
        *slot = None;
        let (token, secs) = self
            .email
            .access_token_with_expiry(&refresh)
            .await
            .map_err(|e| match e {
                EmailError::GrantRequired => {
                    self.expired.store(true, Ordering::Release);
                    Problem::Expired
                }
                _ => Problem::Google,
            })?;
        self.expired.store(false, Ordering::Release);
        let life = secs.map(Duration::from_secs).unwrap_or(DEFAULT_TOKEN_LIFE);
        *slot = Some((token.clone(), Instant::now() + life));
        Ok(token)
    }

    pub fn calendar(&self, token: String) -> GoogleCalendar {
        GoogleCalendar {
            http: self.http.clone(),
            base: self.calendar_base.clone(),
            calendar_id: self.cfg.calendar_id.clone(),
            token,
        }
    }

    /// 許可を取り消す。保存した許可を消してから Google にも取り消しを頼む。
    /// Ok(true) は「こちらでは消したが Google の取り消しは失敗した」
    pub async fn revoke(&self) -> Result<bool, String> {
        let refresh = self.grant_row().await.ok().flatten().and_then(|row| {
            grants::decrypt(&self.email.cfg.key, &aad(&self.account), &row.encrypted).ok()
        });
        let who = self.account.clone();
        pending::blocking(&self.audit, move |t| revoke(t, &who)).await??;
        *self.token.lock().await = None;
        self.email.forget_transactions(&self.account);
        if let Ok(mut s) = self.status.lock() {
            s.problem = Some(Problem::NoGrant);
        }
        tracing::info!("[calendar] 管理用カレンダーの許可を取り消した");
        Ok(match refresh {
            Some(token) => self.email.revoke_google(&token).await.is_err(),
            None => false,
        })
    }
}
