//! 顧客のログインの決まり (計画 5 章・5.1)。時刻は引数で受け取り、テストで進められるようにする。
//!
//! - パスワードは bcrypt。登録の無いメールでも同じだけ計算し、登録の有無を時間で明かさない。
//! - 失敗が続いたらメールアドレスごと・接続元ごとに一時ロック (既存の `RATE_LIMIT_*` と同じ回数・時間)。
//! - 新しい端末 (覚えた端末の札が無い・切れた) では 6 桁のコードをメールで送る。コードはハッシュだけを持ち、
//!   そのログインのやり取り (`challenge`) に結び付ける。プロセス内のメモリだけ (再起動したら送り直し)。
//! - 招待・端末・ログインの値は推測できない乱数を渡し、保存先にはハッシュだけを置く。
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use base64::Engine;
use chrono::{DateTime, Duration, Utc};
use rand::{Rng, RngCore};
use sha2::{Digest, Sha256};

use super::store::{
    Account, AccountStatus, Device, Invite, InvitePurpose, PortalStore, Session, StoreError,
};

#[derive(Debug, Clone)]
pub struct AuthConfig {
    pub bcrypt_cost: u32,
    pub max_failures: u32,
    pub lockout: Duration,
    pub session_ttl: Duration,
    pub invite_ttl: Duration,
    pub device_ttl: Duration,
    pub code_ttl: Duration,
    pub code_attempts: u32,
    pub resend_wait: Duration,
    pub max_resends: u32,
}

impl AuthConfig {
    pub fn new(max_failures: u32, lockout_secs: u64) -> Self {
        Self {
            bcrypt_cost: bcrypt::DEFAULT_COST,
            max_failures: max_failures.max(1),
            lockout: Duration::seconds(lockout_secs.max(1) as i64),
            session_ttl: Duration::hours(12),
            invite_ttl: Duration::days(7),
            device_ttl: Duration::days(90),
            code_ttl: Duration::minutes(10),
            code_attempts: 5,
            resend_wait: Duration::seconds(60),
            max_resends: 3,
        }
    }
}

pub const MIN_PASSWORD_CHARS: usize = 12;
/// bcrypt は 72 バイトより後ろを見ないので、それを超えるパスワードは受け付けない
pub const MAX_PASSWORD_BYTES: usize = 72;
const MAX_CHALLENGES: usize = 10_000;
const MAX_LOCK_KEYS: usize = 50_000;
/// 送り直しを含めて、1 回のログインのやり取りを続けられる時間
const CHALLENGE_LIFETIME_MIN: i64 = 30;

#[derive(Debug)]
pub enum LoginResult {
    SignedIn {
        session: String,
    },
    CodeRequired {
        challenge: String,
        code: String,
        to: Vec<String>,
    },
    Invalid,
    Locked,
}

#[derive(Debug)]
pub enum CodeResult {
    SignedIn {
        session: String,
        device: Option<String>,
    },
    Wrong {
        left: u32,
    },
    /// 期限切れ・間違いの上限・やり取りが無い。ログインからやり直す
    Expired,
}

#[derive(Debug)]
pub enum ResendResult {
    Sent { code: String, to: Vec<String> },
    Wait { secs: i64 },
    Limit,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PasswordProblem {
    TooShort,
    TooLong,
    SameAsEmail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InviteError {
    /// 無い・期限切れ・使用済み・取り消し済み (どれかは明かさない)
    Invalid,
    Password(PasswordProblem),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminError {
    BadEmail,
    BadCustomer,
    Duplicate,
    NotFound,
}

struct Challenge {
    account_id: String,
    code_hash: String,
    created_at: DateTime<Utc>,
    expires_at: DateTime<Utc>,
    attempts: u32,
    resends: u32,
    last_sent_at: DateTime<Utc>,
}

#[derive(Default)]
struct LockInfo {
    failures: u32,
    locked_until: Option<DateTime<Utc>>,
}

pub struct PortalAuth {
    store: Arc<dyn PortalStore>,
    cfg: AuthConfig,
    /// 登録の無いメール・パスワード未設定のときに比べる値 (計算時間をそろえる)
    dummy_hash: String,
    challenges: Mutex<HashMap<String, Challenge>>,
    locks: Mutex<HashMap<String, LockInfo>>,
}

pub fn random_token() -> String {
    let mut b = [0u8; 32];
    rand::rngs::OsRng.fill_bytes(&mut b);
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(b)
}

pub fn hash_token(token: &str) -> String {
    hex_digest(token.as_bytes())
}

fn hex_digest(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn code_hash(challenge_hash: &str, code: &str) -> String {
    hex_digest(format!("{challenge_hash}:{code}").as_bytes())
}

fn new_code() -> String {
    format!("{:06}", rand::rngs::OsRng.gen_range(0..1_000_000u32))
}

pub fn normalize_email(raw: &str) -> String {
    raw.trim().to_lowercase()
}

/// 形だけの確認 (1 つの @、前後が空でない、ドメインに . がある、空白が無い、254 文字以内)
pub fn plausible_email(email: &str) -> bool {
    let Some((local, domain)) = email.split_once('@') else {
        return false;
    };
    !local.is_empty()
        && domain.contains('.')
        && !domain.starts_with('.')
        && !domain.ends_with('.')
        && !domain.contains('@')
        && email.len() <= 254
        && !email.chars().any(|c| c.is_whitespace() || c.is_control())
}

/// `taro@example.com` → `t***@example.com`。画面に送り先を出すときに使う
pub fn mask_email(email: &str) -> String {
    match email.split_once('@') {
        Some((local, domain)) => {
            let first: String = local.chars().take(1).collect();
            format!("{first}***@{domain}")
        }
        None => "***".to_string(),
    }
}

pub fn check_password(password: &str, email: &str) -> Result<(), PasswordProblem> {
    if password.len() > MAX_PASSWORD_BYTES {
        return Err(PasswordProblem::TooLong);
    }
    if password.chars().count() < MIN_PASSWORD_CHARS {
        return Err(PasswordProblem::TooShort);
    }
    if password.trim().to_lowercase() == email {
        return Err(PasswordProblem::SameAsEmail);
    }
    Ok(())
}

impl PortalAuth {
    pub fn new(store: Arc<dyn PortalStore>, cfg: AuthConfig) -> Self {
        let dummy_hash =
            bcrypt::hash(random_token(), cfg.bcrypt_cost).expect("bcrypt のダミーを作れない");
        Self {
            store,
            cfg,
            dummy_hash,
            challenges: Mutex::new(HashMap::new()),
            locks: Mutex::new(HashMap::new()),
        }
    }

    pub fn store(&self) -> &Arc<dyn PortalStore> {
        &self.store
    }

    // ---- ロック ----

    fn locked(&self, keys: &[String], now: DateTime<Utc>) -> bool {
        let g = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        keys.iter().any(|k| {
            g.get(k)
                .and_then(|i| i.locked_until)
                .is_some_and(|until| now < until)
        })
    }

    fn record_failure(&self, keys: &[String], now: DateTime<Utc>) {
        let mut g = self.locks.lock().unwrap_or_else(|e| e.into_inner());
        if g.len() >= MAX_LOCK_KEYS {
            g.retain(|_, i| i.locked_until.is_some_and(|u| now < u));
        }
        for k in keys {
            let info = g.entry(k.clone()).or_default();
            if info.locked_until.is_some_and(|u| now >= u) {
                *info = LockInfo::default();
            }
            info.failures += 1;
            if info.failures >= self.cfg.max_failures {
                info.locked_until = Some(now + self.cfg.lockout);
            }
        }
    }

    fn clear_failures(&self, key: &str) {
        self.locks
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(key);
    }

    // ---- ログイン ----

    /// メールアドレスとパスワード。覚えた端末の札 (`device`) が有効ならそのままログイン、無ければコードを送る。
    /// bcrypt を計算するので `spawn_blocking` の中で呼ぶ。
    pub fn login(
        &self,
        email: &str,
        password: &str,
        device: Option<&str>,
        ip: &str,
        now: DateTime<Utc>,
    ) -> LoginResult {
        let email = normalize_email(email);
        let keys = [format!("email:{email}"), format!("ip:{ip}")];
        if self.locked(&keys, now) {
            return LoginResult::Locked;
        }
        let account = self.store.account_by_email(&email);
        let hash = account
            .as_ref()
            .and_then(|a| a.password_hash.as_deref())
            .unwrap_or(&self.dummy_hash);
        let password_ok =
            password.len() <= MAX_PASSWORD_BYTES && bcrypt::verify(password, hash).unwrap_or(false);
        let account = match account {
            Some(a)
                if password_ok
                    && a.status == AccountStatus::Active
                    && a.password_hash.is_some() =>
            {
                a
            }
            _ => {
                self.record_failure(&keys, now);
                return LoginResult::Invalid;
            }
        };
        self.clear_failures(&keys[0]);

        if let Some(token) = device {
            let h = hash_token(token);
            if let Some(d) = self.store.device(&h) {
                if self.device_valid(&d, &account, now) {
                    self.store.touch_device(&h, now);
                    return LoginResult::SignedIn {
                        session: self.start_session(account, now),
                    };
                }
            }
        }

        let challenge = random_token();
        let ch = hash_token(&challenge);
        let code = new_code();
        let mut g = self.challenges.lock().unwrap_or_else(|e| e.into_inner());
        g.retain(|_, c| now < c.expires_at);
        if g.len() >= MAX_CHALLENGES {
            // 異常に多い。古いものから捨てずに全部捨てる (正しい人はログインし直せばよい)
            g.clear();
        }
        g.insert(
            ch.clone(),
            Challenge {
                account_id: account.id.clone(),
                code_hash: code_hash(&ch, &code),
                created_at: now,
                expires_at: now + self.cfg.code_ttl,
                attempts: 0,
                resends: 0,
                last_sent_at: now,
            },
        );
        LoginResult::CodeRequired {
            challenge,
            code,
            to: code_recipients(&account),
        }
    }

    fn device_valid(&self, d: &Device, account: &Account, now: DateTime<Utc>) -> bool {
        !d.revoked
            && d.account_id == account.id
            && now < d.last_used_at + self.cfg.device_ttl
            && account
                .password_changed_at
                .is_none_or(|t| d.created_at >= t)
    }

    fn start_session(&self, mut account: Account, now: DateTime<Utc>) -> String {
        let token = random_token();
        self.store.insert_session(Session {
            token_hash: hash_token(&token),
            account_id: account.id.clone(),
            created_at: now,
            ended_at: None,
        });
        account.last_login_at = Some(now);
        let _ = self.store.update_account(account);
        token
    }

    /// コードを送れなかったとき、そのやり取りを捨てる
    pub fn drop_challenge(&self, challenge: &str) {
        self.challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&hash_token(challenge));
    }

    pub fn verify_code(
        &self,
        challenge: &str,
        code: &str,
        remember: bool,
        now: DateTime<Utc>,
    ) -> CodeResult {
        let ch = hash_token(challenge);
        let account_id = {
            let mut g = self.challenges.lock().unwrap_or_else(|e| e.into_inner());
            let Some(c) = g.get_mut(&ch) else {
                return CodeResult::Expired;
            };
            if now >= c.expires_at {
                g.remove(&ch);
                return CodeResult::Expired;
            }
            let digits = code.trim();
            if digits.len() != 6
                || !digits.bytes().all(|b| b.is_ascii_digit())
                || code_hash(&ch, digits) != c.code_hash
            {
                c.attempts += 1;
                let left = self.cfg.code_attempts.saturating_sub(c.attempts);
                if left == 0 {
                    g.remove(&ch);
                    return CodeResult::Expired;
                }
                return CodeResult::Wrong { left };
            }
            let c = g.remove(&ch).expect("直前に見つけた");
            c.account_id
        };
        let Some(account) = self
            .store
            .account(&account_id)
            .filter(|a| a.status == AccountStatus::Active)
        else {
            return CodeResult::Expired;
        };
        let device = remember.then(|| {
            let token = random_token();
            self.store.insert_device(Device {
                token_hash: hash_token(&token),
                account_id: account.id.clone(),
                created_at: now,
                last_used_at: now,
                revoked: false,
            });
            token
        });
        CodeResult::SignedIn {
            session: self.start_session(account, now),
            device,
        }
    }

    pub fn resend(&self, challenge: &str, now: DateTime<Utc>) -> ResendResult {
        let ch = hash_token(challenge);
        let mut g = self.challenges.lock().unwrap_or_else(|e| e.into_inner());
        let Some(c) = g.get_mut(&ch) else {
            return ResendResult::Expired;
        };
        if now >= c.created_at + Duration::minutes(CHALLENGE_LIFETIME_MIN) {
            g.remove(&ch);
            return ResendResult::Expired;
        }
        if c.resends >= self.cfg.max_resends {
            return ResendResult::Limit;
        }
        let ready_at = c.last_sent_at + self.cfg.resend_wait;
        if now < ready_at {
            return ResendResult::Wait {
                secs: (ready_at - now).num_seconds().max(1),
            };
        }
        let Some(account) = self
            .store
            .account(&c.account_id)
            .filter(|a| a.status == AccountStatus::Active)
        else {
            g.remove(&ch);
            return ResendResult::Expired;
        };
        let code = new_code();
        c.code_hash = code_hash(&ch, &code);
        c.expires_at = now + self.cfg.code_ttl;
        c.attempts = 0;
        c.resends += 1;
        c.last_sent_at = now;
        ResendResult::Sent {
            code,
            to: code_recipients(&account),
        }
    }

    /// ログイン中の人。ログアウト済み・期限切れ・止めたアカウントなら None
    pub fn current(&self, session: &str, now: DateTime<Utc>) -> Option<Account> {
        let s = self.store.session(&hash_token(session))?;
        if s.ended_at.is_some() || now >= s.created_at + self.cfg.session_ttl {
            return None;
        }
        self.store
            .account(&s.account_id)
            .filter(|a| a.status == AccountStatus::Active)
            .filter(|a| a.password_changed_at.is_none_or(|t| s.created_at >= t))
    }

    pub fn logout(&self, session: &str, now: DateTime<Utc>) {
        self.store.end_session(&hash_token(session), now);
    }

    // ---- 招待 ----

    pub fn invite_info(
        &self,
        token: &str,
        now: DateTime<Utc>,
    ) -> Result<(Account, InvitePurpose), InviteError> {
        let i = self
            .store
            .invite(&hash_token(token))
            .filter(|i| i.used_at.is_none() && !i.revoked && now < i.expires_at)
            .ok_or(InviteError::Invalid)?;
        let a = self
            .store
            .account(&i.account_id)
            .filter(|a| a.status != AccountStatus::Disabled)
            .ok_or(InviteError::Invalid)?;
        Ok((a, i.purpose))
    }

    /// 招待のリンクでパスワードを決める。覚えた端末とログイン中の画面は全部無効にする。
    /// 決めたあとは普通にログインしてもらう (新しい端末なので確認コードを聞く)。bcrypt を計算する。
    pub fn accept_invite(
        &self,
        token: &str,
        password: &str,
        now: DateTime<Utc>,
    ) -> Result<String, InviteError> {
        let (mut account, _) = self.invite_info(token, now)?;
        check_password(password, &account.email).map_err(InviteError::Password)?;
        let hash =
            bcrypt::hash(password, self.cfg.bcrypt_cost).map_err(|_| InviteError::Invalid)?;
        if !self.store.use_invite(&hash_token(token), now) {
            return Err(InviteError::Invalid);
        }
        account.password_hash = Some(hash);
        account.password_changed_at = Some(now);
        account.status = AccountStatus::Active;
        self.store.revoke_devices(&account.id);
        self.store.end_sessions(&account.id, now);
        self.drop_challenges_of(&account.id);
        let email = account.email.clone();
        self.store
            .update_account(account)
            .map_err(|_| InviteError::Invalid)?;
        Ok(email)
    }

    fn drop_challenges_of(&self, account_id: &str) {
        self.challenges
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .retain(|_, c| c.account_id != account_id);
    }

    // ---- 社内の操作 ----

    /// アカウントを作り、初回の招待のリンクの値を返す (生の値はここで 1 回だけ)
    pub fn create_account(
        &self,
        email: &str,
        customer_id: &str,
        now: DateTime<Utc>,
    ) -> Result<(Account, String), AdminError> {
        let email = normalize_email(email);
        if !plausible_email(&email) {
            return Err(AdminError::BadEmail);
        }
        let customer_id = customer_id.trim();
        if customer_id.is_empty() || customer_id.chars().count() > 64 {
            return Err(AdminError::BadCustomer);
        }
        let account = Account {
            id: uuid::Uuid::new_v4().to_string(),
            email,
            customer_id: customer_id.to_string(),
            password_hash: None,
            extra_code_email: None,
            status: AccountStatus::Invited,
            password_changed_at: None,
            last_login_at: None,
            created_at: now,
        };
        self.store
            .insert_account(account.clone())
            .map_err(|e| match e {
                StoreError::Duplicate => AdminError::Duplicate,
                StoreError::NotFound => AdminError::NotFound,
            })?;
        let token = self.issue_invite(&account.id, now)?;
        Ok((account, token))
    }

    /// 招待のリンクを出し直す (前のリンクは使えなくなる)。パスワードがあれば再設定のリンク
    pub fn issue_invite(&self, account_id: &str, now: DateTime<Utc>) -> Result<String, AdminError> {
        let a = self.store.account(account_id).ok_or(AdminError::NotFound)?;
        self.store.revoke_invites(&a.id);
        let token = random_token();
        self.store.insert_invite(Invite {
            token_hash: hash_token(&token),
            account_id: a.id.clone(),
            purpose: if a.password_hash.is_some() {
                InvitePurpose::Reset
            } else {
                InvitePurpose::Initial
            },
            expires_at: now + self.cfg.invite_ttl,
            used_at: None,
            revoked: false,
        });
        Ok(token)
    }

    /// 止める。覚えた端末・ログイン中の画面・招待・送ったコードを全部無効にする
    pub fn disable(&self, account_id: &str, now: DateTime<Utc>) -> Result<Account, AdminError> {
        let mut a = self.store.account(account_id).ok_or(AdminError::NotFound)?;
        a.status = AccountStatus::Disabled;
        self.store.revoke_devices(&a.id);
        self.store.end_sessions(&a.id, now);
        self.store.revoke_invites(&a.id);
        self.drop_challenges_of(&a.id);
        self.store
            .update_account(a.clone())
            .map_err(|_| AdminError::NotFound)?;
        Ok(a)
    }

    /// 止めたのを戻す。パスワードが無ければ招待済みに戻る (端末は戻らない)
    pub fn enable(&self, account_id: &str) -> Result<Account, AdminError> {
        let mut a = self.store.account(account_id).ok_or(AdminError::NotFound)?;
        a.status = if a.password_hash.is_some() {
            AccountStatus::Active
        } else {
            AccountStatus::Invited
        };
        self.store
            .update_account(a.clone())
            .map_err(|_| AdminError::NotFound)?;
        Ok(a)
    }

    /// 確認コードの追加の送り先 (空なら外す)
    pub fn set_extra_email(&self, account_id: &str, email: &str) -> Result<Account, AdminError> {
        let mut a = self.store.account(account_id).ok_or(AdminError::NotFound)?;
        let e = normalize_email(email);
        a.extra_code_email = if e.is_empty() {
            None
        } else if plausible_email(&e) && e != a.email {
            Some(e)
        } else {
            return Err(AdminError::BadEmail);
        };
        self.store
            .update_account(a.clone())
            .map_err(|_| AdminError::NotFound)?;
        Ok(a)
    }
}

fn code_recipients(a: &Account) -> Vec<String> {
    let mut to = vec![a.email.clone()];
    if let Some(e) = &a.extra_code_email {
        to.push(e.clone());
    }
    to
}
