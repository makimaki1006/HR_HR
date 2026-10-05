//! `/api/crm/*` の認可 (RBAC 骨格)。レコード読み取りと定義 (metadata) で同じ基準を使う。
//!
//! 現在の許可条件 (すべて満たすこと):
//! 1. ログイン済み (セッションに email がある)。無ければ **401** (`login_required`、JSON)
//! 2. ログイン方式が Google Workspace OIDC。パスワードログイン (社内共通 / 外部期限付き) は
//!    個人を特定できないので **403** (`google_login_required`)
//! 3. email が `CRM_METADATA_ALLOWED_EMAILS` (カンマ区切り、大文字小文字を区別しない完全一致) にある。
//!    空なら全員拒否。無ければ **403** (`forbidden`)
//! 4. 監査 DB で無効化されたアカウントでない。無効なら **403** (`account_disabled`)
//!    (監査 DB 未接続・照会失敗のときは止めない = `crate::account_is_disabled` の方針)
//!
//! 未ログインでも HTML の /login へ 303 せず JSON の 401 を返す (fetch から呼ばれるため)。
//! このため `/api/crm/*` は共有の auth_middleware (リダイレクト) の外に置く。
//!
//! 次の段階 (役割の本実装) の差し込み口:
//! - 役割 (admin / consultant / bpo / user) の保持先は audit Turso の `accounts.role` と決定済み。
//!   [`Principal::role`] はそのための欄で、**現在は読み込まない** (常に None。許可判定にも使わない)。
//! - BPO が読めるのは架電キューに出たレコードだけ、というレコード単位の絞り込みは
//!   [`can_read`] の `record` 引数 (どの種類のレコードか) に id を足して入れる想定。
//!
//! 差し替えはこのファイルだけで済むようにしてある。

use std::collections::HashSet;
use std::sync::Arc;

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

/// 許可メールの一覧 (暫定。役割の本実装まで)。
#[derive(Debug, Clone, Default)]
pub struct CrmAccess {
    allowed_emails: HashSet<String>,
}

impl CrmAccess {
    /// 環境変数 `CRM_METADATA_ALLOWED_EMAILS` から作る (未設定・空なら全員拒否)。
    pub fn from_env() -> Self {
        Self::from_list(&std::env::var("CRM_METADATA_ALLOWED_EMAILS").unwrap_or_default())
    }

    /// カンマ区切りの文字列から作る。前後の空白と空要素は捨て、小文字にそろえる。
    pub fn from_list(list: &str) -> Self {
        Self {
            allowed_emails: list
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
        }
    }

    pub fn is_allowed(&self, email: &str) -> bool {
        self.allowed_emails.contains(&email.trim().to_lowercase())
    }

    pub fn is_empty(&self) -> bool {
        self.allowed_emails.is_empty()
    }
}

/// 認可判定に使うログイン中の人の情報。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Principal {
    pub email: Option<String>,
    pub login_method: Option<String>,
    /// 役割の本実装 (次の段階) 用。現在は読み込まず、判定にも使わない。
    pub role: Option<String>,
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

/// 読み取りを許可するか (セッションの内容だけで決まる部分。無効化アカウントの照会は [`authorize`])。
///
/// `record` は今は使わない (どの種類でも同じ扱い)。レコード単位の絞り込みを入れるときの差し込み口。
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

/// 架電キューなどで使う役割。**暫定**: 役割の本実装 (audit Turso の `accounts.role`) までの間、
/// `ADMIN_EMAILS` に載っている人だけ admin、それ以外はすべて bpo として扱う (安全側。
/// 判定を間違えても「見える範囲が狭い方」に倒れる)。本実装後はこの関数だけ差し替える。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmRole {
    Admin,
    Bpo,
}

impl CrmRole {
    pub fn as_str(self) -> &'static str {
        match self {
            CrmRole::Admin => "admin",
            CrmRole::Bpo => "bpo",
        }
    }
}

/// 役割の暫定判定 (上の [`CrmRole`] を参照)。`principal.role` は本実装まで読まない。
pub fn resolve_role(config: &crate::config::AppConfig, principal: &Principal) -> CrmRole {
    let email = principal.email.as_deref().unwrap_or_default().trim();
    if !email.is_empty()
        && config
            .admin_emails
            .iter()
            .any(|a| a.trim().eq_ignore_ascii_case(email))
    {
        CrmRole::Admin
    } else {
        CrmRole::Bpo
    }
}

/// セッションから [`Principal`] を作る。役割は読み込まない (次の段階)。
pub async fn load_principal(session: &Session) -> Principal {
    Principal {
        email: session.get(SESSION_USER_KEY).await.unwrap_or(None),
        login_method: session.get(SESSION_LOGIN_METHOD_KEY).await.unwrap_or(None),
        role: None,
    }
}

/// 認可の入口。通れば [`Principal`]、通らなければ [`Denied`] (そのまま応答にできる)。
/// **HubSpot の設定有無を見る前に呼ぶ** (未認可の人に 503 など設定状況を見せない)。
pub async fn authorize(
    session: &Session,
    state: &Arc<AppState>,
    access: &CrmAccess,
    record: Option<RecordType>,
) -> Result<Principal, Denied> {
    let principal = load_principal(session).await;
    can_read(&principal, record, access)?;
    if crate::account_is_disabled(state, principal.email.as_deref().unwrap_or_default()).await {
        return Err(Denied::AccountDisabled);
    }
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

    /// 役割の暫定判定: ADMIN_EMAILS にある人だけ admin、それ以外は (未知の人・空・役割欄があっても) すべて bpo
    #[test]
    fn 役割は_admin_emails_だけが_admin_でそれ以外は_bpo() {
        let mut config = crate::config::AppConfig::from_env();
        config.admin_emails = vec![
            "Boss@f-a-c.co.jp".to_string(),
            " cto@f-a-c.co.jp ".to_string(),
        ];
        let role = |email: Option<&str>, r: Option<&str>| {
            let mut who = p(email, Some(LOGIN_METHOD_GOOGLE_OIDC));
            who.role = r.map(str::to_string);
            resolve_role(&config, &who)
        };
        assert_eq!(role(Some("boss@f-a-c.co.jp"), None), CrmRole::Admin);
        assert_eq!(role(Some("BOSS@F-A-C.CO.JP"), None), CrmRole::Admin);
        assert_eq!(role(Some("cto@f-a-c.co.jp"), None), CrmRole::Admin);
        assert_eq!(role(Some("staff@f-a-c.co.jp"), None), CrmRole::Bpo);
        // 部分一致・別ドメインは admin にならない
        assert_eq!(role(Some("boss@f-a-c.co.jp.evil.com"), None), CrmRole::Bpo);
        assert_eq!(role(Some("xboss@f-a-c.co.jp"), None), CrmRole::Bpo);
        // 未ログイン・空は bpo (安全側)
        assert_eq!(role(None, None), CrmRole::Bpo);
        assert_eq!(role(Some(" "), None), CrmRole::Bpo);
        // principal.role (役割の本実装まで読まない) に "admin" とあっても昇格しない
        assert_eq!(role(Some("staff@f-a-c.co.jp"), Some("admin")), CrmRole::Bpo);
        // ADMIN_EMAILS が空なら全員 bpo
        config.admin_emails.clear();
        assert_eq!(
            resolve_role(&config, &p(Some("boss@f-a-c.co.jp"), None)),
            CrmRole::Bpo
        );
    }

    /// 逆証明: 空の許可リストでは Google ログインの本人でも通らない
    #[test]
    fn 空の許可リストは全員拒否() {
        let who = p(Some("taro@f-a-c.co.jp"), Some(LOGIN_METHOD_GOOGLE_OIDC));
        assert_eq!(
            can_read(&who, None, &CrmAccess::default()),
            Err(Denied::NotAllowed)
        );
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
