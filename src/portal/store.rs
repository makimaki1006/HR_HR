//! 顧客向け応募者画面のアカウント・招待・覚えた端末・ログインの保存先。
//!
//! 計画 (`docs/architecture/customer-applicant-portal-plan.html` 9 章) では社内用のスプレッドシートに置く。
//! そのシートの読み書き (計画の PR1) はまだ無いので、ここでは差し替えられる [`PortalStore`] と、
//! プロセス内のメモリに置く [`MemoryStore`] だけを持つ。招待・端末・ログインはハッシュだけを置く。
use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Utc};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountStatus {
    /// 招待済みでパスワードをまだ決めていない
    Invited,
    Active,
    Disabled,
}

impl AccountStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            AccountStatus::Invited => "invited",
            AccountStatus::Active => "active",
            AccountStatus::Disabled => "disabled",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Account {
    pub id: String,
    /// 小文字
    pub email: String,
    pub customer_id: String,
    pub password_hash: Option<String>,
    /// 確認コードの追加の送り先 (小文字)
    pub extra_code_email: Option<String>,
    pub status: AccountStatus,
    pub password_changed_at: Option<DateTime<Utc>>,
    pub last_login_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InvitePurpose {
    /// 初回のパスワード設定
    Initial,
    /// パスワードの再設定
    Reset,
}

impl InvitePurpose {
    pub fn as_str(self) -> &'static str {
        match self {
            InvitePurpose::Initial => "initial",
            InvitePurpose::Reset => "reset",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Invite {
    pub token_hash: String,
    pub account_id: String,
    pub purpose: InvitePurpose,
    pub expires_at: DateTime<Utc>,
    pub used_at: Option<DateTime<Utc>>,
    /// 新しい招待を出した・アカウントを止めたときに無効にする
    pub revoked: bool,
}

#[derive(Debug, Clone)]
pub struct Device {
    pub token_hash: String,
    pub account_id: String,
    pub created_at: DateTime<Utc>,
    pub last_used_at: DateTime<Utc>,
    pub revoked: bool,
}

#[derive(Debug, Clone)]
pub struct Session {
    pub token_hash: String,
    pub account_id: String,
    pub created_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoreError {
    /// 同じメールアドレスのアカウントがある
    Duplicate,
    NotFound,
}

/// 保存先。社内用シートの実装に替えるときはこの trait を実装する。
pub trait PortalStore: Send + Sync {
    fn accounts(&self) -> Vec<Account>;
    fn account(&self, id: &str) -> Option<Account>;
    fn account_by_email(&self, email: &str) -> Option<Account>;
    fn insert_account(&self, account: Account) -> Result<(), StoreError>;
    fn update_account(&self, account: Account) -> Result<(), StoreError>;

    fn insert_invite(&self, invite: Invite);
    fn invite(&self, token_hash: &str) -> Option<Invite>;
    /// 未使用・未失効のときだけ使用済みにして true (同じリンクの同時使用で 2 回通さない)
    fn use_invite(&self, token_hash: &str, now: DateTime<Utc>) -> bool;
    fn revoke_invites(&self, account_id: &str);

    fn insert_device(&self, device: Device);
    fn device(&self, token_hash: &str) -> Option<Device>;
    fn touch_device(&self, token_hash: &str, now: DateTime<Utc>);
    fn revoke_devices(&self, account_id: &str);

    fn insert_session(&self, session: Session);
    fn session(&self, token_hash: &str) -> Option<Session>;
    fn end_session(&self, token_hash: &str, now: DateTime<Utc>);
    fn end_sessions(&self, account_id: &str, now: DateTime<Utc>);
}

#[derive(Default)]
struct Inner {
    accounts: HashMap<String, Account>,
    invites: HashMap<String, Invite>,
    devices: HashMap<String, Device>,
    sessions: HashMap<String, Session>,
}

/// プロセス内のメモリだけに置く保存先 (テストと手元の確認用。再起動で消える)。
#[derive(Default)]
pub struct MemoryStore {
    inner: Mutex<Inner>,
}

impl MemoryStore {
    fn lock(&self) -> std::sync::MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl PortalStore for MemoryStore {
    fn accounts(&self) -> Vec<Account> {
        let mut out: Vec<_> = self.lock().accounts.values().cloned().collect();
        out.sort_by(|a, b| (&a.customer_id, &a.email).cmp(&(&b.customer_id, &b.email)));
        out
    }
    fn account(&self, id: &str) -> Option<Account> {
        self.lock().accounts.get(id).cloned()
    }
    fn account_by_email(&self, email: &str) -> Option<Account> {
        self.lock()
            .accounts
            .values()
            .find(|a| a.email == email)
            .cloned()
    }
    fn insert_account(&self, account: Account) -> Result<(), StoreError> {
        let mut g = self.lock();
        if g.accounts.values().any(|a| a.email == account.email) {
            return Err(StoreError::Duplicate);
        }
        g.accounts.insert(account.id.clone(), account);
        Ok(())
    }
    fn update_account(&self, account: Account) -> Result<(), StoreError> {
        let mut g = self.lock();
        match g.accounts.get_mut(&account.id) {
            Some(a) => {
                *a = account;
                Ok(())
            }
            None => Err(StoreError::NotFound),
        }
    }

    fn insert_invite(&self, invite: Invite) {
        self.lock()
            .invites
            .insert(invite.token_hash.clone(), invite);
    }
    fn invite(&self, token_hash: &str) -> Option<Invite> {
        self.lock().invites.get(token_hash).cloned()
    }
    fn use_invite(&self, token_hash: &str, now: DateTime<Utc>) -> bool {
        match self.lock().invites.get_mut(token_hash) {
            Some(i) if i.used_at.is_none() && !i.revoked && now < i.expires_at => {
                i.used_at = Some(now);
                true
            }
            _ => false,
        }
    }
    fn revoke_invites(&self, account_id: &str) {
        for i in self.lock().invites.values_mut() {
            if i.account_id == account_id {
                i.revoked = true;
            }
        }
    }

    fn insert_device(&self, device: Device) {
        self.lock()
            .devices
            .insert(device.token_hash.clone(), device);
    }
    fn device(&self, token_hash: &str) -> Option<Device> {
        self.lock().devices.get(token_hash).cloned()
    }
    fn touch_device(&self, token_hash: &str, now: DateTime<Utc>) {
        if let Some(d) = self.lock().devices.get_mut(token_hash) {
            d.last_used_at = now;
        }
    }
    fn revoke_devices(&self, account_id: &str) {
        for d in self.lock().devices.values_mut() {
            if d.account_id == account_id {
                d.revoked = true;
            }
        }
    }

    fn insert_session(&self, session: Session) {
        self.lock()
            .sessions
            .insert(session.token_hash.clone(), session);
    }
    fn session(&self, token_hash: &str) -> Option<Session> {
        self.lock().sessions.get(token_hash).cloned()
    }
    fn end_session(&self, token_hash: &str, now: DateTime<Utc>) {
        if let Some(s) = self.lock().sessions.get_mut(token_hash) {
            s.ended_at.get_or_insert(now);
        }
    }
    fn end_sessions(&self, account_id: &str, now: DateTime<Utc>) {
        for s in self.lock().sessions.values_mut() {
            if s.account_id == account_id {
                s.ended_at.get_or_insert(now);
            }
        }
    }
}
