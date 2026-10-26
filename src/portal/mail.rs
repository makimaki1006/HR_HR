//! 確認コードのメール。計画 8 章: 会社の Google Workspace の Gmail から送る (別のサービスは使わない)。
//!
//! 本番の送信は [`GmailMailer`]: 社員本人 (`PORTAL_MAIL_SENDER`) の Gmail から送る。
//! 許可は CRM のメール送信の許可 (ADR-022、監査 Turso の `crm_gmail_grants`) をそのまま使う。
//! 新しい鍵・DB・表は足さない。
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use crate::audit::AuditDb;
use crate::crm::email::gmail::{plain_mime, RawSendError};
use crate::crm::email::{valid_address, EmailError, EmailService};

pub type MailFuture<'a> = Pin<Box<dyn Future<Output = Result<(), MailError>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailError;

/// 送信元の社員のアドレスを入れる環境変数 (公開リポなので値はコードに書かない)
pub const ENV_SENDER: &str = "PORTAL_MAIL_SENDER";

pub trait Mailer: Send + Sync {
    /// 件名・本文 (文字だけ) を、宛先 1 つにつき 1 通ずつ送る (宛先どうしは見えない)。
    /// 確認コードのほか、応募の通知 (計画 14 章) にも使う
    fn send<'a>(&'a self, to: &'a [String], subject: &'a str, body: &'a str) -> MailFuture<'a>;

    /// 件名にもコードを入れ、本文は短い文字だけ (リンク・画像は入れない。計画 5.2)
    fn send_code<'a>(&'a self, to: &'a [String], code: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            let (subject, body) = code_message(code);
            self.send(to, &subject, &body).await
        })
    }

    /// いま送れるか。送れないときは顧客の API を 503 にする
    fn ready(&self) -> bool {
        true
    }
}

/// 件名と本文。送信の実装はこれを使う
pub fn code_message(code: &str) -> (String, String) {
    (
        format!("確認コード {code}（応募者の管理）"),
        format!(
            "応募者の管理にログインするための確認コードです。\n\n{code}\n\n\
このコードは 10 分間有効です。\n\
お心当たりがない場合は、このメールを破棄し、弊社の担当者にご連絡ください。\n"
        ),
    )
}

/// 手元の確認用 (debug ビルドだけ)。送らずに、サーバーのログに件名を出す (件名にコードが入る)。
#[cfg(debug_assertions)]
pub struct DevLogMailer;

#[cfg(debug_assertions)]
impl Mailer for DevLogMailer {
    fn send<'a>(&'a self, to: &'a [String], subject: &'a str, _body: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            tracing::warn!(
                "[portal dev] {subject} → {} (手元の確認用。送信していない)",
                to.join(", ")
            );
            Ok(())
        })
    }
}

// ---- 社員本人の Gmail から送る ----

/// 許可が無い・切れたあと、次に確かめ直すまでの間 (要求が来たときだけ確かめる)
const RECHECK_AFTER: Duration = Duration::from_secs(60);
/// access token の期限より前に取り直す余裕
const TOKEN_MARGIN: Duration = Duration::from_secs(60);
/// Google が期限を返さないときの見込み
const DEFAULT_TOKEN_LIFE: Duration = Duration::from_secs(3600);

/// 送れない理由。ログには理由だけを出す (アドレス・トークンは出さない)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// 送信元の人が CRM 画面で Gmail の許可をしていない
    NoGrant,
    /// Google が許可切れ・取り消しを返した
    Expired,
    /// 許可を読めない (監査 DB・鍵)
    Storage,
    /// Google に繋がらない
    Network,
}

impl Problem {
    fn reason(self) -> &'static str {
        match self {
            Self::NoGrant => "送信元の許可がない（送信元の人が CRM 画面で Gmail の許可をしてください）",
            Self::Expired => "送信元の許可が切れた・取り消された（送信元の人が CRM 画面で Gmail の許可をし直してください）",
            Self::Storage => "送信元の許可を読めない（監査 DB・CRM_EMAIL_TOKEN_KEY）",
            Self::Network => "Google に繋がらない",
        }
    }
    /// 許可そのものが無い (確かめ直すまで送れない)
    fn blocks(self) -> bool {
        matches!(self, Self::NoGrant | Self::Expired)
    }
}

struct Inner {
    sender: String,
    email: Arc<EmailService>,
    audit: AuditDb,
    /// access token と期限 (プロセス内だけ)。取り直しは 1 本ずつ
    token: tokio::sync::Mutex<Option<(String, Instant)>>,
    ready: AtomicBool,
    checking: AtomicBool,
    last_check: Mutex<Option<Instant>>,
}

/// 社員本人 (`PORTAL_MAIL_SENDER`) の Gmail から送る [`Mailer`]
pub struct GmailMailer {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for GmailMailer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GmailMailer { sender: [redacted], token: [redacted] }")
    }
}

impl GmailMailer {
    /// 環境変数から作る。CRM のメール送信が有効になる条件 (CRM_EMAIL_ENABLED・鍵・Google の設定・監査 DB)
    /// がそろわないときは作らない。許可の有無は作ったあとに確かめ、無ければ送れない (503) まま
    pub fn from_env(audit: Option<AuditDb>) -> Option<Arc<Self>> {
        let me = Self::from_config(
            std::env::var(ENV_SENDER).ok(),
            EmailService::from_env().map(Arc::new),
            audit,
        )?;
        // 起動したらすぐ背景で許可を確かめる (確かめ終わるまでは 503)
        if tokio::runtime::Handle::try_current().is_ok() {
            me.spawn_check();
        }
        Some(me)
    }

    /// [`Self::from_env`] の中身 (環境変数を読んだあと)
    pub fn from_config(
        sender: Option<String>,
        email: Option<Arc<EmailService>>,
        audit: Option<AuditDb>,
    ) -> Option<Arc<Self>> {
        let sender = sender.unwrap_or_default();
        let sender = sender.trim();
        if sender.is_empty() {
            tracing::info!("[portal] {ENV_SENDER} が未設定なので、確認コードのメールは送れない");
            return None;
        }
        let Some(email) = email else {
            tracing::warn!(
                "[portal] CRM のメール送信が無効（CRM_EMAIL_ENABLED・CRM_EMAIL_TOKEN_KEY・Google の設定）なので、確認コードのメールは送れない"
            );
            return None;
        };
        let Some(audit) = audit else {
            tracing::warn!("[portal] 監査 DB が無いので、確認コードのメールは送れない");
            return None;
        };
        Self::new(sender, email, audit)
    }

    /// 送信元のアドレスが正しい形のときだけ作る。許可は [`Self::check_now`] か、最初の要求で確かめる
    pub fn new(sender: &str, email: Arc<EmailService>, audit: AuditDb) -> Option<Arc<Self>> {
        let sender = sender.trim().to_ascii_lowercase();
        if !valid_address(&sender) {
            tracing::warn!(
                "[portal] {ENV_SENDER} の形が正しくないので、確認コードのメールは送れない"
            );
            return None;
        }
        let me = Arc::new(Self {
            inner: Arc::new(Inner {
                sender,
                email,
                audit,
                token: tokio::sync::Mutex::new(None),
                ready: AtomicBool::new(false),
                checking: AtomicBool::new(false),
                last_check: Mutex::new(None),
            }),
        });
        Some(me)
    }

    /// 許可を確かめ (access token を取り直す)、送れるかを返す
    pub async fn check_now(&self) -> bool {
        self.inner.check().await
    }

    fn spawn_check(&self) {
        if self
            .inner
            .checking
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .is_err()
        {
            return;
        }
        let inner = self.inner.clone();
        tokio::spawn(async move {
            inner.check().await;
        });
    }
}

impl Inner {
    async fn check(&self) -> bool {
        self.checking.store(true, Ordering::Release);
        *self.token.lock().await = None;
        let ok = match self.access().await {
            Ok(_) => {
                tracing::info!("[portal] 確認コードのメールの送信元の許可を確かめた");
                self.ready.store(true, Ordering::Release);
                true
            }
            Err(p) => {
                tracing::warn!("[portal] 確認コードのメールを送れない: {}", p.reason());
                self.ready.store(false, Ordering::Release);
                false
            }
        };
        if let Ok(mut t) = self.last_check.lock() {
            *t = Some(Instant::now());
        }
        self.checking.store(false, Ordering::Release);
        ok
    }

    /// 期限内の access token を使い回し、切れていれば許可を読んで取り直す
    async fn access(&self) -> Result<String, Problem> {
        let mut slot = self.token.lock().await;
        if let Some((token, until)) = slot.as_ref() {
            if Instant::now() + TOKEN_MARGIN < *until {
                return Ok(token.clone());
            }
        }
        *slot = None;
        let refresh = self
            .email
            .refresh_token(&self.audit, &self.sender)
            .await
            .map_err(|e| match e {
                EmailError::GrantRequired => Problem::NoGrant,
                _ => Problem::Storage,
            })?;
        let (token, secs) = self
            .email
            .access_token_with_expiry(&refresh)
            .await
            .map_err(|e| match e {
                EmailError::GrantRequired => Problem::Expired,
                _ => Problem::Network,
            })?;
        let life = secs.map(Duration::from_secs).unwrap_or(DEFAULT_TOKEN_LIFE);
        *slot = Some((token.clone(), Instant::now() + life));
        Ok(token)
    }

    async fn forget_token(&self) {
        *self.token.lock().await = None;
    }

    fn blocked(&self, p: Problem) {
        if p.blocks() {
            self.ready.store(false, Ordering::Release);
            if let Ok(mut t) = self.last_check.lock() {
                *t = Some(Instant::now());
            }
        }
    }
}

impl Mailer for GmailMailer {
    fn ready(&self) -> bool {
        if self.inner.ready.load(Ordering::Acquire) {
            return true;
        }
        // 許可が無い間は、要求が来たときだけ (60 秒に 1 回まで) 確かめ直す
        let due = self
            .inner
            .last_check
            .lock()
            .map(|t| t.is_none_or(|t| t.elapsed() >= RECHECK_AFTER))
            .unwrap_or(false);
        if due && tokio::runtime::Handle::try_current().is_ok() {
            self.spawn_check();
        }
        false
    }

    fn send<'a>(&'a self, to: &'a [String], subject: &'a str, body: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            let inner = &self.inner;
            if to.is_empty() || subject.contains(['\r', '\n']) {
                return Err(MailError);
            }
            if !to.iter().all(|a| valid_address(a)) {
                tracing::warn!("[portal] 宛先の形が正しくないので送らない");
                return Err(MailError);
            }
            let access = match inner.access().await {
                Ok(t) => t,
                Err(p) => {
                    tracing::warn!("[portal] メールを送れない: {}", p.reason());
                    inner.blocked(p);
                    return Err(MailError);
                }
            };
            let mut sent = 0usize;
            for addr in to {
                let raw = plain_mime(&inner.sender, addr, subject, body);
                match inner.email.send_raw(&access, &raw).await {
                    Ok(()) => sent += 1,
                    Err(RawSendError::Status(401)) => {
                        tracing::warn!(
                            "[portal] Gmail が許可切れを返した（送信元の人が CRM 画面で Gmail の許可をし直してください）"
                        );
                        inner.forget_token().await;
                        inner.blocked(Problem::Expired);
                        break;
                    }
                    Err(RawSendError::Status(s)) => {
                        tracing::warn!("[portal] Gmail の送信の失敗（{s}）");
                    }
                    Err(RawSendError::Network) => {
                        tracing::warn!("[portal] Gmail の送信の失敗（通信）");
                    }
                }
            }
            if sent == 0 {
                return Err(MailError);
            }
            if sent < to.len() {
                tracing::warn!("[portal] {} 通のうち {sent} 通だけ送れた", to.len());
            }
            Ok(())
        })
    }
}
