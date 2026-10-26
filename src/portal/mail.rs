//! 確認コードのメール。計画 8 章: 会社の Google Workspace の Gmail から送る (別のサービスは使わない)。
//!
//! 本番の送信は [`GmailMailer`]: 社員本人 (`PORTAL_MAIL_SENDER`) の Gmail から送る。
//! 許可は顧客画面のために別に取る (ADR-036、監査 Turso の `portal_mail_grants`、gmail.send だけ。
//! 取り方は [`super::mail_grant`])。CRM のメール送信の許可 (`crm_gmail_grants`) は使わない。
use std::collections::{HashMap, VecDeque};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::mail_grant::{self, GrantRow};
use crate::audit::AuditDb;
use crate::crm::email::gmail::{plain_mime, RawSendError};
use crate::crm::email::{grants, valid_address, EmailConfig, EmailError, EmailService};
use crate::crm::pending;

/// 送信の結果。成功なら届けた (Gmail が受け付けた) 宛先
pub type MailFuture<'a> = Pin<Box<dyn Future<Output = Result<Vec<String>, MailError>> + Send + 'a>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MailError;

/// 送信元の社員のアドレスを入れる環境変数 (公開リポなので値はコードに書かない)
pub const ENV_SENDER: &str = "PORTAL_MAIL_SENDER";

pub trait Mailer: Send + Sync {
    /// 件名・本文 (文字だけ) を、宛先 1 つにつき 1 通ずつ送る (宛先どうしは見えない)。
    /// 確認コードのほか、応募の通知 (計画 14 章) にも使う。1 通も届かなければ Err、
    /// 一部だけ届いたら届いた宛先だけを返す
    fn send<'a>(&'a self, to: &'a [String], subject: &'a str, body: &'a str) -> MailFuture<'a>;

    /// 件名にもコードを入れ、本文は短い文字だけ (リンク・画像は入れない。計画 5.2)
    fn send_code<'a>(&'a self, to: &'a [String], code: &'a str) -> MailFuture<'a> {
        Box::pin(async move {
            let (subject, body) = code_message(code);
            self.send(to, &subject, &body).await
        })
    }

    /// いま送れるか。送れないときは確認コードを送る経路 (ログインのコード・送り直し) だけを断る
    fn ready(&self) -> bool {
        true
    }

    /// 社員本人の Gmail から送る実装なら、それ (管理画面で許可の状態を見る・取り消すのに使う)
    fn gmail(&self) -> Option<&GmailMailer> {
        None
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
            Ok(to.to_vec())
        })
    }
}

// ---- 送信の回数の上限 (プロセス内、単一インスタンス前提) ----

/// アカウントごと: 確認コードの送信は 1 時間に 5 回まで
pub const PER_ACCOUNT_PER_HOUR: usize = 5;
/// 全体: 1 時間に 200 通まで (送信元の社員の Gmail の 1 日の上限を使い切られないように)
pub const TOTAL_PER_HOUR: usize = 200;

/// 確認コードの送信の回数の上限
pub struct SendLimits {
    per_account: usize,
    total: usize,
    window: Duration,
    state: Mutex<LimitState>,
}

#[derive(Default)]
struct LimitState {
    /// アカウントごとの送信 (1 回の送信 = 1)
    accounts: HashMap<String, VecDeque<Instant>>,
    /// 全体の通数 (宛先ごとに 1)
    mails: VecDeque<Instant>,
}

impl Default for SendLimits {
    fn default() -> Self {
        Self::new(
            PER_ACCOUNT_PER_HOUR,
            TOTAL_PER_HOUR,
            Duration::from_secs(3600),
        )
    }
}

impl SendLimits {
    pub fn new(per_account: usize, total: usize, window: Duration) -> Self {
        Self {
            per_account,
            total,
            window,
            state: Mutex::default(),
        }
    }

    fn prune(&self, st: &mut LimitState, now: Instant) {
        let window = self.window;
        let old = |t: &Instant| now.saturating_duration_since(*t) >= window;
        while st.mails.front().is_some_and(old) {
            st.mails.pop_front();
        }
        st.accounts.retain(|_, q| {
            while q.front().is_some_and(old) {
                q.pop_front();
            }
            !q.is_empty()
        });
    }

    fn fits(&self, st: &LimitState, key: &str, mails: usize) -> bool {
        st.accounts.get(key).map_or(0, VecDeque::len) < self.per_account
            && st.mails.len() + mails <= self.total
    }

    /// 送ってよいかだけを見る (数えない)
    pub fn allows(&self, key: &str, mails: usize, now: Instant) -> bool {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.prune(&mut st, now);
        self.fits(&st, key, mails)
    }

    /// 送ってよければ数えて true。超えるなら数えずに false
    pub fn take(&self, key: &str, mails: usize, now: Instant) -> bool {
        let mut st = self.state.lock().unwrap_or_else(|e| e.into_inner());
        self.prune(&mut st, now);
        if !self.fits(&st, key, mails) {
            return false;
        }
        st.accounts
            .entry(key.to_string())
            .or_default()
            .push_back(now);
        st.mails.extend(std::iter::repeat_n(now, mails));
        true
    }
}

// ---- 社員本人の Gmail から送る ----

/// 許可が無い・切れたあと、次に確かめ直すまでの間 (要求が来たときだけ確かめる)
const RECHECK_AFTER: Duration = Duration::from_secs(60);
/// Gmail に断られた (403・429) あと、送らない間
const PAUSE_AFTER_REFUSAL: Duration = Duration::from_secs(600);
/// access token の期限より前に取り直す余裕
const TOKEN_MARGIN: Duration = Duration::from_secs(60);
/// Google が期限を返さないときの見込み
const DEFAULT_TOKEN_LIFE: Duration = Duration::from_secs(3600);

/// 送れない理由。ログには理由だけを出す (アドレス・トークンは出さない)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Problem {
    /// 送信元の人が管理画面で Gmail の許可をしていない (または取り消した)
    NoGrant,
    /// 送信元のアカウントが HR_HR で無効化されている
    Disabled,
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
            Self::NoGrant => "送信元の許可がない（送信元の人が応募者画面の管理画面で Gmail の許可をしてください）",
            Self::Disabled => "送信元のアカウントが HR_HR で無効になっている",
            Self::Expired => "送信元の許可が切れた・取り消された（送信元の人が応募者画面の管理画面で Gmail の許可をし直してください）",
            Self::Storage => "送信元の許可を読めない（監査 DB・CRM_EMAIL_TOKEN_KEY）",
            Self::Network => "Google に繋がらない",
        }
    }
    /// 許可そのものが無い (確かめ直すまで送れない)
    fn blocks(self) -> bool {
        matches!(self, Self::NoGrant | Self::Disabled | Self::Expired)
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
    /// Gmail に断られて送らない期限
    paused_until: Mutex<Option<Instant>>,
    /// 最後に送れなかった理由 (管理画面の「切れ」の表示に使う)
    last_problem: Mutex<Option<Problem>>,
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
    /// がそろわないときは作らない。許可の有無は作ったあとに確かめ、無ければ確認コードを送れない
    pub fn from_env(audit: Option<AuditDb>) -> Option<Arc<Self>> {
        let me = Self::from_config(
            std::env::var("PORTAL_MAIL_SENDER").ok(),
            EmailConfig::from_env_with_redirect(
                std::env::var("PORTAL_MAIL_REDIRECT_URL").ok(),
                mail_grant::CALLBACK,
            )
            .map(|c| Arc::new(EmailService::new(c))),
            audit,
        )?;
        // 起動したらすぐ背景で許可を確かめる
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
                "[portal] 送信の設定（CRM_EMAIL_TOKEN_KEY・Google の設定・PORTAL_MAIL_REDIRECT_URL）がそろわないので、確認コードのメールは送れない"
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
        Some(Arc::new(Self {
            inner: Arc::new(Inner {
                sender,
                email,
                audit,
                token: tokio::sync::Mutex::new(None),
                ready: AtomicBool::new(false),
                checking: AtomicBool::new(false),
                last_check: Mutex::new(None),
                paused_until: Mutex::new(None),
                last_problem: Mutex::new(None),
            }),
        }))
    }

    /// 許可を確かめ (access token を取り直す)、送れるかを返す
    pub async fn check_now(&self) -> bool {
        self.inner.check().await
    }

    /// 送信元のアドレス (管理画面にだけ出す。ログには出さない)
    pub fn sender(&self) -> &str {
        &self.inner.sender
    }

    /// 許可を取る手続き (顧客画面用の callback を持つ)
    pub(crate) fn service(&self) -> &EmailService {
        &self.inner.email
    }

    /// 最後に Google が許可切れ・取り消しを返したか
    pub fn expired(&self) -> bool {
        self.inner
            .last_problem
            .lock()
            .map(|p| *p == Some(Problem::Expired))
            .unwrap_or(false)
    }

    /// Gmail に断られて、しばらく送らない間か
    pub fn is_paused(&self) -> bool {
        self.inner.paused()
    }

    /// 保存してある許可 (取り消されていないもの)。読めなければ Err
    pub async fn grant_row(&self) -> Result<Option<GrantRow>, MailError> {
        let who = self.inner.sender.clone();
        pending::blocking(&self.inner.audit, move |t| mail_grant::get(t, &who))
            .await
            .map_err(|_| MailError)?
            .map_err(|_| MailError)
    }

    /// 許可を取り消す。保存した許可を消してから Google にも取り消しを頼む。
    /// Ok(true) は「こちらでは消したが Google の取り消しは失敗した」
    pub async fn revoke(&self) -> Result<bool, MailError> {
        let inner = &self.inner;
        let refresh = self.grant_row().await.ok().flatten().and_then(|row| {
            grants::decrypt(
                &inner.email.cfg.key,
                &mail_grant::aad(&inner.sender),
                &row.encrypted,
            )
            .ok()
        });
        let who = inner.sender.clone();
        let done = pending::blocking(&inner.audit, move |t| mail_grant::revoke(t, &who)).await;
        if !matches!(done, Ok(Ok(()))) {
            return Err(MailError);
        }
        inner.forget_token().await;
        inner.blocked(Problem::NoGrant);
        inner.email.forget_transactions(&inner.sender);
        tracing::info!("[portal] 確認コードのメールの送信元の Gmail の許可を取り消した");
        Ok(match refresh {
            Some(token) => inner.email.revoke_google(&token).await.is_err(),
            None => false,
        })
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
                self.set_problem(None);
                true
            }
            Err(p) => {
                tracing::warn!("[portal] 確認コードのメールを送れない: {}", p.reason());
                self.ready.store(false, Ordering::Release);
                self.set_problem(Some(p));
                false
            }
        };
        if let Ok(mut t) = self.last_check.lock() {
            *t = Some(Instant::now());
        }
        self.checking.store(false, Ordering::Release);
        ok
    }

    /// 送るたびに監査 DB を読み直し、許可がいまもあること・送信元が HR_HR で無効になっていないことを
    /// 確かめる (CRM 画面での取り消しを access token の期限まで見逃さない)。返すのは refresh token
    async fn confirm(&self) -> Result<String, Problem> {
        let who = self.sender.clone();
        let (disabled, grant) = pending::blocking(&self.audit, move |t| {
            (
                crate::audit::dao::is_email_disabled(t, &who),
                mail_grant::get(t, &who),
            )
        })
        .await
        .map_err(|_| Problem::Storage)?;
        match disabled {
            Ok(false) => {}
            Ok(true) => return Err(Problem::Disabled),
            Err(_) => return Err(Problem::Storage),
        }
        let row = grant
            .map_err(|_| Problem::Storage)?
            .ok_or(Problem::NoGrant)?;
        grants::decrypt(
            &self.email.cfg.key,
            &mail_grant::aad(&self.sender),
            &row.encrypted,
        )
        .map_err(|_| Problem::Storage)
    }

    /// 許可を確かめ、期限内の access token を使い回す (切れていれば取り直す)
    async fn access(&self) -> Result<String, Problem> {
        let refresh = self.confirm().await?;
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

    fn set_problem(&self, p: Option<Problem>) {
        if let Ok(mut slot) = self.last_problem.lock() {
            *slot = p;
        }
    }

    fn blocked(&self, p: Problem) {
        self.set_problem(Some(p));
        if p.blocks() {
            self.ready.store(false, Ordering::Release);
            if let Ok(mut t) = self.last_check.lock() {
                *t = Some(Instant::now());
            }
        }
    }

    fn paused(&self) -> bool {
        self.paused_until
            .lock()
            .map(|t| t.is_some_and(|t| Instant::now() < t))
            .unwrap_or(false)
    }

    fn pause(&self) {
        if let Ok(mut t) = self.paused_until.lock() {
            *t = Some(Instant::now() + PAUSE_AFTER_REFUSAL);
        }
    }
}

impl Mailer for GmailMailer {
    fn gmail(&self) -> Option<&GmailMailer> {
        Some(self)
    }

    fn ready(&self) -> bool {
        if self.inner.paused() {
            return false;
        }
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
            if inner.paused() {
                tracing::warn!("[portal] Gmail に断られたあとの待ちの間なので送らない");
                return Err(MailError);
            }
            let access = match inner.access().await {
                Ok(t) => t,
                Err(p) => {
                    tracing::warn!("[portal] メールを送れない: {}", p.reason());
                    if p.blocks() {
                        inner.forget_token().await;
                    }
                    inner.blocked(p);
                    return Err(MailError);
                }
            };
            let mut delivered = Vec::new();
            for addr in to {
                let raw = plain_mime(&inner.sender, addr, subject, body);
                match inner.email.send_raw(&access, &raw).await {
                    Ok(()) => delivered.push(addr.clone()),
                    Err(RawSendError::Status(401)) => {
                        tracing::warn!(
                            "[portal] Gmail が許可切れを返した（送信元の人が応募者画面の管理画面で Gmail の許可をし直してください）"
                        );
                        inner.forget_token().await;
                        inner.blocked(Problem::Expired);
                        break;
                    }
                    Err(RawSendError::Status(s @ (403 | 429))) => {
                        tracing::warn!(
                            "[portal] Gmail が送信を断った（{s}。送信の上限・権限の可能性）。10 分は送らない"
                        );
                        inner.pause();
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
            if delivered.is_empty() {
                return Err(MailError);
            }
            if delivered.len() < to.len() {
                tracing::warn!(
                    "[portal] {} 通のうち {} 通だけ送れた",
                    to.len(),
                    delivered.len()
                );
            }
            Ok(delivered)
        })
    }
}
