//! `/api/crm/*` の認可 (役割 = RBAC)。レコード読み取りと定義 (metadata) で同じ基準を使う。
//!
//! 許可条件 (すべて満たすこと):
//! 1. ログイン済み (セッションに email がある)。無ければ **401** (`login_required`、JSON)
//! 2. ログイン方式が Google Workspace OIDC (会社ドメインの本人確認済み)。パスワードログイン (社内共通 / 外部期限付き) は
//!    個人を特定できないので **403** (`google_login_required`)
//! 3. `CRM_METADATA_ALLOWED_EMAILS` (カンマ区切り、大文字小文字を区別しない完全一致) が**設定されているときだけ**、
//!    その中にあること (追加の絞り込み。非常時に全員を一括で止める栓として残す)。
//!    空・未設定なら絞り込まない。外れたら **403** (`forbidden`)
//! 4. 監査 DB で無効化されたアカウントでない。無効なら **403** (`account_disabled`)
//!    (監査 DB 未接続・照会失敗のときは止めない = `crate::account_is_disabled` の方針)
//! 5. メールのドメインが会社ドメイン (`ALLOWED_DOMAINS`、外部追加ドメインは含めない)。外れたら **403** (`forbidden`)
//!
//! ## 見られる範囲 (決定 2026-10-07 を同日に更新: 全員が全件を見られる)
//! 会社の Google アカウントでログインした人は全員 CRM の「ユーザー」で、HubSpot と同じく全件を読める
//! (キュー・担当者の一覧・個別の Deal / Contact / Company・ワークスペース)。違いは**キューの既定の担当者**だけ。
//! | 区分 | 判定 | キューの既定 | 管理者だけの機能 (管理画面・役割変更・hubspot-check) |
//! |---|---|---|---|
//! | 管理者 | `ADMIN_EMAILS` または `accounts.role = admin` | 全員分 | 使える |
//! | 上記以外の全員 | (既定) | 自分 (`me`)。自分の owner が見つからなければ 409 `owner_not_resolved` で画面が選択を促す | 使えない |
//!
//! `owner=all` / `unassigned` / 任意の owner id は全員が指定できる。
//! - `accounts.role` の consultant / bpo / user は**判定に使わない** (admin だけ使う)。
//! - HubSpot の所属チーム (owner の `teams`) は範囲の判定に使わない。
//! - CRM を使えない人 (パスワードログイン・社外ドメイン・無効アカウント・未ログイン) は、このモジュールの `authorize` で
//!   HubSpot を呼ぶ前に拒否する。
//!
//! ## 管理者の読み取りと失敗時の方針 (権限を広げない)
//! - `accounts.role` はリクエストごとには引かず、メールごとに 5 分キャッシュする ([`ROLE_CACHE_TTL`])。管理画面で変えたときは
//!   同じプロセスのキャッシュを即時に捨てる ([`invalidate_role`])。別プロセスには最大 5 分かけて反映される。
//! - 監査 DB が未接続・照会に失敗したときは `ADMIN_EMAILS` の人だけ admin (非常口)、それ以外は管理者でない (= 自分の分だけ)。
//!   **キャッシュに残っている古い役割は使わない**。失敗は 30 秒だけ覚えて、監査 DB を叩き続けない。
//! - `ADMIN_EMAILS` の人は accounts.role に関わらず admin (従来どおり次のログインで admin に戻るため)。
//!
//! 未ログインでも HTML の /login へ 303 せず JSON の 401 を返す (fetch から呼ばれるため)。
//! このため `/api/crm/*` は共有の auth_middleware (リダイレクト) の外に置く。
//!
//! レコード単位の制限 (管理者以外) は `record_gate.rs`。

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use axum::{
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use tower_sessions::Session;

use crate::auth::{LOGIN_METHOD_GOOGLE_OIDC, SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY};
use crate::hubspot::RecordType;
use crate::AppState;

/// CRM を読めるログイン方式。
pub const READ_ALLOWED_LOGIN_METHODS: &[&str] = &[LOGIN_METHOD_GOOGLE_OIDC];

/// 役割を読んだ結果を覚えておく時間
pub const ROLE_CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// 役割を読めなかった (照会の失敗) ことを覚えておく時間。監査 DB を叩き続けないため短くする
pub const ROLE_FAILURE_TTL: Duration = Duration::from_secs(30);
/// キャッシュの最大件数 (超えたら全部捨てる。ユーザー数より十分大きい)
const ROLE_CACHE_MAX: usize = 10_000;

/// CRM の役割 (`accounts.role`)。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CrmRole {
    Admin,
    Consultant,
    Bpo,
    /// 既定。CRM は使えない
    User,
}

impl CrmRole {
    pub const ALL: [CrmRole; 4] = [
        CrmRole::Admin,
        CrmRole::Consultant,
        CrmRole::Bpo,
        CrmRole::User,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            CrmRole::Admin => "admin",
            CrmRole::Consultant => "consultant",
            CrmRole::Bpo => "bpo",
            CrmRole::User => "user",
        }
    }

    /// 前後の空白を除き小文字にして完全一致。4 つ以外は `None`
    pub fn parse_known(raw: &str) -> Option<CrmRole> {
        let t = raw.trim().to_lowercase();
        CrmRole::ALL.into_iter().find(|r| r.as_str() == t)
    }

    /// 未知の値・空は最小権限の user
    pub fn parse(raw: &str) -> CrmRole {
        CrmRole::parse_known(raw).unwrap_or(CrmRole::User)
    }

    /// 権限の強さ (同じメールの行が複数あるとき小さい方を採る)
    fn rank(self) -> u8 {
        match self {
            CrmRole::Admin => 3,
            CrmRole::Consultant => 2,
            CrmRole::Bpo => 1,
            CrmRole::User => 0,
        }
    }

    /// 管理者か (キューの既定が全員分になる。管理画面・hubspot-check など管理者だけの機能の判定にも使う)
    pub fn is_admin(self) -> bool {
        self == CrmRole::Admin
    }

    /// 全レコードを読めるか。CRM の利用者 (authorize を通った人) は全員読める (決定 2026-10-07)。
    /// 役割が決まっていない (`User` = 最小権限) ときだけ読めない (レコード単位の関門 `record_gate` に回る = 安全側)
    pub fn reads_all_records(self) -> bool {
        self != CrmRole::User
    }
}

/// 監査 DB から役割を引いた結果
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoleLookup {
    /// accounts の行がある (複数なら最小権限)
    Found(CrmRole),
    /// 読めたが行が無い
    NoRow,
    /// 監査 DB 未接続 / 照会の失敗 / 読み取りタスクの異常
    Unavailable,
}

/// 役割の最終判定。管理者 (`ADMIN_EMAILS` の人、または accounts.role = admin) は [`CrmRole::Admin`]。
/// それ以外の全員は **[`CrmRole::Bpo`] = 自分の担当分だけ** (名前は旧来のもので、BPO かどうかは見ていない)。
/// consultant / user などの値は見ない。
pub fn finalize_role(lookup: RoleLookup, email: &str, admin_emails: &[String]) -> CrmRole {
    let email = email.trim();
    let in_env = !email.is_empty()
        && admin_emails
            .iter()
            .any(|a| a.trim().eq_ignore_ascii_case(email));
    if in_env || lookup == RoleLookup::Found(CrmRole::Admin) {
        CrmRole::Admin
    } else {
        CrmRole::Bpo
    }
}

/// メール (小文字) → 役割の読み取り結果のキャッシュ。時刻は呼び出し側から渡す (テストで進められる)。
#[derive(Debug, Default)]
pub struct RoleCache {
    inner: Mutex<HashMap<String, (Instant, RoleLookup)>>,
}

impl RoleCache {
    pub fn new() -> Self {
        Self::default()
    }

    fn key(email: &str) -> String {
        email.trim().to_lowercase()
    }

    /// 有効期間内のものだけ返す。`Unavailable` は短い期間 ([`ROLE_FAILURE_TTL`])
    pub fn get(&self, email: &str, now: Instant) -> Option<RoleLookup> {
        let g = self.inner.lock().ok()?;
        let (at, v) = g.get(&Self::key(email))?;
        let ttl = if *v == RoleLookup::Unavailable {
            ROLE_FAILURE_TTL
        } else {
            ROLE_CACHE_TTL
        };
        (now.saturating_duration_since(*at) < ttl).then_some(*v)
    }

    pub fn put(&self, email: &str, now: Instant, v: RoleLookup) {
        if let Ok(mut g) = self.inner.lock() {
            if g.len() >= ROLE_CACHE_MAX {
                g.clear();
            }
            g.insert(Self::key(email), (now, v));
        }
    }

    pub fn invalidate(&self, email: &str) {
        if let Ok(mut g) = self.inner.lock() {
            g.remove(&Self::key(email));
        }
    }
}

static GLOBAL_ROLE_CACHE: OnceLock<Arc<RoleCache>> = OnceLock::new();

/// 本番で使う、プロセス全体で 1 つのキャッシュ (管理画面の役割変更から捨てられるように共有する)
pub fn global_role_cache() -> Arc<RoleCache> {
    GLOBAL_ROLE_CACHE
        .get_or_init(|| Arc::new(RoleCache::new()))
        .clone()
}

/// 管理画面で役割を変えたとき、そのメールのキャッシュを (このプロセスで) 即時に捨てる
pub fn invalidate_role(email: &str) {
    global_role_cache().invalidate(email);
}

/// 監査 DB から役割を引く (キャッシュ付き)。**読めなかったときは古い値を使わない**。
pub async fn lookup_role(
    state: &AppState,
    cache: &RoleCache,
    email: &str,
    now: Instant,
) -> RoleLookup {
    if let Some(v) = cache.get(email, now) {
        return v;
    }
    let Some(audit) = &state.audit else {
        // 未接続は覚えない (DB を叩かないので負荷にならない)
        return RoleLookup::Unavailable;
    };
    let audit = audit.clone();
    let email_owned = email.trim().to_string();
    let result = tokio::task::spawn_blocking(move || {
        crate::audit::dao::find_roles_by_email(audit.turso(), &email_owned)
    })
    .await;
    let lookup = match result {
        Ok(Ok(rows)) => rows
            .iter()
            .map(|r| CrmRole::parse(r))
            .min_by_key(|r| r.rank())
            .map_or(RoleLookup::NoRow, RoleLookup::Found),
        Ok(Err(e)) => {
            tracing::warn!("crm role lookup failed (権限は広げない): {e}");
            RoleLookup::Unavailable
        }
        Err(e) => {
            tracing::warn!("crm role lookup join failed (権限は広げない): {e}");
            RoleLookup::Unavailable
        }
    };
    cache.put(email, now, lookup);
    lookup
}

/// 許可メールの絞り込み (任意) と役割のキャッシュ。
#[derive(Debug, Clone)]
pub struct CrmAccess {
    allowed_emails: HashSet<String>,
    roles: Arc<RoleCache>,
    /// テスト専用: 監査 DB を引かずに役割を固定する (本番ビルドには存在しない)
    #[cfg(test)]
    test_roles: HashMap<String, CrmRole>,
}

impl Default for CrmAccess {
    fn default() -> Self {
        Self::from_list("")
    }
}

impl CrmAccess {
    /// 環境変数 `CRM_METADATA_ALLOWED_EMAILS` から作る。役割のキャッシュはプロセス共通のものを使う。
    pub fn from_env() -> Self {
        let mut a =
            Self::from_list(&std::env::var("CRM_METADATA_ALLOWED_EMAILS").unwrap_or_default());
        a.roles = global_role_cache();
        a
    }

    /// カンマ区切りの文字列から作る。前後の空白と空要素は捨て、小文字にそろえる。
    /// 役割のキャッシュは新しく作る (テストで互いに影響しない)。
    pub fn from_list(list: &str) -> Self {
        Self {
            allowed_emails: list
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            roles: Arc::new(RoleCache::new()),
            #[cfg(test)]
            test_roles: HashMap::new(),
        }
    }

    /// 許可メールの絞り込みを通るか。**絞り込みが設定されていない (空) ときは誰でも通る** (役割で決まる)。
    pub fn is_allowed(&self, email: &str) -> bool {
        self.allowed_emails.is_empty() || self.allowed_emails.contains(&email.trim().to_lowercase())
    }

    /// 絞り込みが設定されていないか
    pub fn is_empty(&self) -> bool {
        self.allowed_emails.is_empty()
    }

    /// 役割のキャッシュ (テストで有効期限を確かめる)
    pub fn role_cache(&self) -> &RoleCache {
        &self.roles
    }

    /// 本人の役割 (キャッシュ → 監査 DB → 非常口)。
    pub async fn role_of(&self, state: &AppState, email: &str) -> CrmRole {
        #[cfg(test)]
        if let Some(r) = self.test_roles.get(&email.trim().to_lowercase()) {
            return *r;
        }
        let lookup = lookup_role(state, &self.roles, email, Instant::now()).await;
        finalize_role(lookup, email, &state.config.admin_emails)
    }

    /// テスト専用: 本番と同じプロセス共通のキャッシュを使う (役割変更 API のキャッシュ破棄の配線を確かめる)
    #[cfg(test)]
    pub fn with_global_role_cache(mut self) -> Self {
        self.roles = global_role_cache();
        self
    }

    /// テスト専用: このメールの役割を固定する
    #[cfg(test)]
    pub fn with_test_role(mut self, email: &str, role: CrmRole) -> Self {
        self.test_roles.insert(email.trim().to_lowercase(), role);
        self
    }
}

/// 認可判定に使うログイン中の人の情報。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Principal {
    pub email: Option<String>,
    pub login_method: Option<String>,
    /// 役割。[`authorize`] が監査 DB から引いて入れる (セッションからは読まない)。
    pub role: Option<CrmRole>,
}

/// 拒否の理由。`error_kind` が API 応答に出る。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    LoginRequired,
    GoogleLoginRequired,
    NotAllowed,
    AccountDisabled,
}

impl Denied {
    pub fn error_kind(self) -> &'static str {
        match self {
            Denied::LoginRequired => "login_required",
            Denied::GoogleLoginRequired => "google_login_required",
            Denied::NotAllowed => "forbidden",
            Denied::AccountDisabled => "account_disabled",
        }
    }

    pub fn status(self) -> StatusCode {
        match self {
            Denied::LoginRequired => StatusCode::UNAUTHORIZED,
            _ => StatusCode::FORBIDDEN,
        }
    }
}

#[derive(Serialize)]
struct DeniedBody {
    error_kind: &'static str,
}

impl IntoResponse for Denied {
    fn into_response(self) -> Response {
        let mut resp = (
            self.status(),
            Json(DeniedBody {
                error_kind: self.error_kind(),
            }),
        )
            .into_response();
        resp.headers_mut()
            .insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
        resp
    }
}

/// 読み取りを許可するか (セッションの内容だけで決まる部分。無効化アカウントの照会と役割は [`authorize`])。
///
/// `record` は今は使わない (どの種類でも同じ扱い)。レコード単位の絞り込みは `record_gate.rs`。
pub fn can_read(
    principal: &Principal,
    _record: Option<RecordType>,
    access: &CrmAccess,
) -> Result<(), Denied> {
    let email = principal
        .email
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .ok_or(Denied::LoginRequired)?;
    let method_ok = principal
        .login_method
        .as_deref()
        .is_some_and(|m| READ_ALLOWED_LOGIN_METHODS.contains(&m));
    if !method_ok {
        return Err(Denied::GoogleLoginRequired);
    }
    if !access.is_allowed(email) {
        return Err(Denied::NotAllowed);
    }
    Ok(())
}

/// 本人の役割。[`authorize`] を通った人は必ず入っている。入っていなければ最小権限 (user)。
pub fn resolve_role(principal: &Principal) -> CrmRole {
    principal.role.unwrap_or(CrmRole::User)
}

/// セッションから [`Principal`] を作る。役割は [`authorize`] が入れる。
pub async fn load_principal(session: &Session) -> Principal {
    Principal {
        email: session.get(SESSION_USER_KEY).await.unwrap_or(None),
        login_method: session.get(SESSION_LOGIN_METHOD_KEY).await.unwrap_or(None),
        role: None,
    }
}

/// 認可の入口。通れば役割入りの [`Principal`]、通らなければ [`Denied`] (そのまま応答にできる)。
/// **HubSpot の設定有無を見る前に呼ぶ** (未認可の人に 503 など設定状況を見せない)。
/// 役割は監査 DB (Turso) だけを読む。**HubSpot は呼ばない**。
pub async fn authorize(
    session: &Session,
    state: &Arc<AppState>,
    access: &CrmAccess,
    record: Option<RecordType>,
) -> Result<Principal, Denied> {
    let mut principal = load_principal(session).await;
    can_read(&principal, record, access)?;
    let email = principal.email.clone().unwrap_or_default();
    // 会社ドメイン (ALLOWED_DOMAINS。外部追加ドメインは含めない)。OIDC のログイン時にも見ているが、ここでも確かめる
    if !crate::auth::validate_email_domain(&email, &state.config.allowed_domains) {
        return Err(Denied::NotAllowed);
    }
    if crate::account_is_disabled(state, &email).await {
        return Err(Denied::AccountDisabled);
    }
    let role = access.role_of(state, &email).await;
    principal.role = Some(role);
    Ok(principal)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        LOGIN_METHOD_PASSWORD, LOGIN_METHOD_PASSWORD_EXTERNAL, LOGIN_METHOD_PASSWORD_INTERNAL,
    };

    fn p(email: Option<&str>, method: Option<&str>) -> Principal {
        Principal {
            email: email.map(str::to_string),
            login_method: method.map(str::to_string),
            role: None,
        }
    }

    #[test]
    fn 許可リストの解釈() {
        let a = CrmAccess::from_list(" A@f-a-c.co.jp , ,b@f-a-c.co.jp,");
        assert!(a.is_allowed("a@f-a-c.co.jp"));
        assert!(a.is_allowed("A@F-A-C.CO.JP"));
        assert!(a.is_allowed(" b@f-a-c.co.jp "));
        assert!(!a.is_allowed("c@f-a-c.co.jp"));
        // 部分一致・ドメイン一致では通らない
        assert!(!a.is_allowed("a@f-a-c.co.jp.evil.com"));
        assert!(!a.is_allowed("f-a-c.co.jp"));
        assert!(!a.is_allowed(""));
        assert!(!a.is_empty());
        assert!(CrmAccess::from_list("").is_empty());
        assert!(CrmAccess::from_list(" , ").is_empty());
    }

    /// ログイン方式 × 許可リスト × email の表
    #[test]
    fn can_read_の判定表() {
        let access = CrmAccess::from_list("taro@f-a-c.co.jp");
        let oidc = Some(LOGIN_METHOD_GOOGLE_OIDC);
        let me = Some("taro@f-a-c.co.jp");
        type Case<'a> = (Option<&'a str>, Option<&'a str>, Result<(), Denied>);
        let cases: &[Case] = &[
            (me, oidc, Ok(())),
            (Some("TARO@f-a-c.co.jp"), oidc, Ok(())),
            (Some("hanako@f-a-c.co.jp"), oidc, Err(Denied::NotAllowed)),
            (None, oidc, Err(Denied::LoginRequired)),
            (Some(""), oidc, Err(Denied::LoginRequired)),
            (Some("  "), oidc, Err(Denied::LoginRequired)),
            (
                me,
                Some(LOGIN_METHOD_PASSWORD_INTERNAL),
                Err(Denied::GoogleLoginRequired),
            ),
            (
                me,
                Some(LOGIN_METHOD_PASSWORD_EXTERNAL),
                Err(Denied::GoogleLoginRequired),
            ),
            (
                me,
                Some(LOGIN_METHOD_PASSWORD),
                Err(Denied::GoogleLoginRequired),
            ),
            (me, None, Err(Denied::GoogleLoginRequired)),
            (me, Some("google_oidc "), Err(Denied::GoogleLoginRequired)),
        ];
        for (email, method, want) in cases {
            for rt in [None, Some(RecordType::Contact), Some(RecordType::Deal)] {
                assert_eq!(
                    can_read(&p(*email, *method), rt, &access),
                    *want,
                    "email={email:?} method={method:?} record={rt:?}"
                );
            }
        }
    }

    /// 許可リストが空なら絞り込まない (役割だけで決まる)。未ログイン・パスワードログインは従来どおり落ちる
    #[test]
    fn 空の許可リストは絞り込まない() {
        let none = CrmAccess::default();
        let oidc = Some(LOGIN_METHOD_GOOGLE_OIDC);
        assert_eq!(
            can_read(&p(Some("taro@f-a-c.co.jp"), oidc), None, &none),
            Ok(())
        );
        assert_eq!(
            can_read(&p(None, oidc), None, &none),
            Err(Denied::LoginRequired)
        );
        assert_eq!(
            can_read(
                &p(
                    Some("taro@f-a-c.co.jp"),
                    Some(LOGIN_METHOD_PASSWORD_INTERNAL)
                ),
                None,
                &none
            ),
            Err(Denied::GoogleLoginRequired)
        );
    }

    /// 役割の文字列の解釈: 4 つだけ。大文字小文字・前後の空白は吸収。未知・空・全角・部分一致は user
    #[test]
    fn 役割の文字列は_4_つだけ_それ以外は_user() {
        let cases: &[(&str, CrmRole)] = &[
            ("admin", CrmRole::Admin),
            ("Admin", CrmRole::Admin),
            (" ADMIN\t", CrmRole::Admin),
            ("consultant", CrmRole::Consultant),
            ("Consultant ", CrmRole::Consultant),
            ("bpo", CrmRole::Bpo),
            (" BPO ", CrmRole::Bpo),
            ("user", CrmRole::User),
            ("", CrmRole::User),
            ("   ", CrmRole::User),
            ("superadmin", CrmRole::User),
            ("admin,bpo", CrmRole::User),
            ("administrator", CrmRole::User),
            ("ａｄｍｉｎ", CrmRole::User),
            ("owner", CrmRole::User),
            ("null", CrmRole::User),
        ];
        for (raw, want) in cases {
            assert_eq!(CrmRole::parse(raw), *want, "{raw:?}");
        }
        // parse_known は未知を None で返す (変更 API が 400 にするため)
        assert_eq!(CrmRole::parse_known("bpo"), Some(CrmRole::Bpo));
        assert_eq!(CrmRole::parse_known("boss"), None);
        assert_eq!(CrmRole::parse_known(""), None);
        // 全レコードを読めるのは CRM の利用者全員 (役割が決まっていない user だけ読めない = 安全側)。管理者は admin だけ
        assert!(CrmRole::Admin.reads_all_records());
        assert!(CrmRole::Consultant.reads_all_records());
        assert!(CrmRole::Bpo.reads_all_records());
        assert!(!CrmRole::User.reads_all_records());
        assert!(CrmRole::Admin.is_admin());
        assert!(!CrmRole::Consultant.is_admin());
        assert!(!CrmRole::Bpo.is_admin());
        assert!(!CrmRole::User.is_admin());
    }

    /// 読み取り結果 × ADMIN_EMAILS の表。管理者 = ADMIN_EMAILS か accounts.role=admin。それ以外は全員「自分の分だけ」(Bpo)
    #[test]
    fn 最終判定の表_管理者以外は全員自分の分だけ() {
        let admins = vec![" Boss@f-a-c.co.jp ".to_string()];
        let boss = "boss@f-a-c.co.jp";
        let staff = "staff@f-a-c.co.jp";
        type Case<'a> = (RoleLookup, &'a str, CrmRole);
        let own = CrmRole::Bpo;
        let cases: &[Case] = &[
            (RoleLookup::Found(CrmRole::Admin), staff, CrmRole::Admin),
            // ADMIN_EMAILS は accounts の値に関わらず admin
            (RoleLookup::Found(CrmRole::Bpo), boss, CrmRole::Admin),
            (RoleLookup::Found(CrmRole::User), boss, CrmRole::Admin),
            (RoleLookup::NoRow, boss, CrmRole::Admin),
            (RoleLookup::Unavailable, boss, CrmRole::Admin),
            (RoleLookup::Unavailable, "BOSS@F-A-C.CO.JP", CrmRole::Admin),
            // consultant / bpo / user の値は判定に使わない (全員 自分の分だけ)
            (RoleLookup::Found(CrmRole::Consultant), staff, own),
            (RoleLookup::Found(CrmRole::Bpo), staff, own),
            (RoleLookup::Found(CrmRole::User), staff, own),
            (RoleLookup::NoRow, staff, own),
            (RoleLookup::Unavailable, staff, own),
            (RoleLookup::Unavailable, "", own),
            // 部分一致は昇格しない
            (RoleLookup::Unavailable, "boss@f-a-c.co.jp.evil.com", own),
            (RoleLookup::Unavailable, "xboss@f-a-c.co.jp", own),
        ];
        for (lookup, email, want) in cases {
            assert_eq!(
                finalize_role(*lookup, email, &admins),
                *want,
                "{lookup:?} {email:?}"
            );
        }
        // ADMIN_EMAILS が空なら非常口は無い
        assert_eq!(finalize_role(RoleLookup::Unavailable, boss, &[]), own);
    }

    /// キャッシュの有効期限 (成功 5 分 / 失敗 30 秒)、大文字小文字・空白の吸収、invalidate
    #[test]
    fn 役割キャッシュの有効期限() {
        let c = RoleCache::new();
        let t0 = Instant::now();
        c.put("Taro@F-A-C.co.jp ", t0, RoleLookup::Found(CrmRole::Bpo));
        let at = |secs: u64| t0 + Duration::from_secs(secs);
        assert_eq!(
            c.get("taro@f-a-c.co.jp", t0),
            Some(RoleLookup::Found(CrmRole::Bpo))
        );
        assert_eq!(
            c.get(" TARO@f-a-c.co.jp", at(299)),
            Some(RoleLookup::Found(CrmRole::Bpo))
        );
        assert_eq!(c.get("taro@f-a-c.co.jp", at(300)), None);
        assert_eq!(c.get("taro@f-a-c.co.jp", at(3600)), None);
        // 失敗は 30 秒
        c.put("fail@f-a-c.co.jp", t0, RoleLookup::Unavailable);
        assert_eq!(
            c.get("fail@f-a-c.co.jp", at(29)),
            Some(RoleLookup::Unavailable)
        );
        assert_eq!(c.get("fail@f-a-c.co.jp", at(30)), None);
        // 行なしは成功扱いで 5 分
        c.put("none@f-a-c.co.jp", t0, RoleLookup::NoRow);
        assert_eq!(c.get("none@f-a-c.co.jp", at(120)), Some(RoleLookup::NoRow));
        // 別のメールは別
        assert_eq!(c.get("other@f-a-c.co.jp", t0), None);
        // invalidate
        c.invalidate("TARO@f-a-c.co.jp");
        assert_eq!(c.get("taro@f-a-c.co.jp", t0), None);
        assert_eq!(c.get("none@f-a-c.co.jp", t0), Some(RoleLookup::NoRow));
    }

    #[test]
    fn 拒否の応答は_json_で_no_store() {
        for (d, status, kind) in [
            (Denied::LoginRequired, 401, "login_required"),
            (Denied::GoogleLoginRequired, 403, "google_login_required"),
            (Denied::NotAllowed, 403, "forbidden"),
            (Denied::AccountDisabled, 403, "account_disabled"),
        ] {
            assert_eq!(d.status().as_u16(), status);
            assert_eq!(d.error_kind(), kind);
            let r = d.into_response();
            assert_eq!(r.headers()[header::CACHE_CONTROL], "no-store");
            assert!(r.headers().get(header::LOCATION).is_none());
        }
    }
}
