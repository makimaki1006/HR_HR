//! `/api/crm/*` の認可 (RBAC 骨格)。
//!
//! 暫定の許可条件: **ログイン方式が Google Workspace OIDC かつ 監査 DB の `accounts.role` が
//! [`READ_ALLOWED_ROLES`] に含まれる** 人だけが CRM レコードを読める。
//!
//! 未決事項 (差し替えはこのファイルだけで済むようにしてある):
//! - D-1 / D-2: 役割の種類 (admin / consultant / manager ...) と保持先 (audit の `accounts.role`
//!   か、別テーブルか、HubSpot の owner か)。決まったら [`READ_ALLOWED_ROLES`] と
//!   [`load_principal`] の役割の取り方を差し替える。レコード単位の絞り込み (自分の担当だけ等) は
//!   [`can_read`] の `record` 引数で足す想定。
//! - D-10: 監査 DB 未接続時の扱い。**いまは fail closed (403)**。監査 DB が落ちている間は
//!   CRM を誰も読めない。
//!
//! 役割が取れない場合 (監査 DB 未接続 / セッションに account_id が無い / 照会失敗 /
//! アカウントが無効化済み) はすべて「役割なし」= 拒否。

use std::sync::Arc;

use tower_sessions::Session;

use crate::auth::{LOGIN_METHOD_GOOGLE_OIDC, SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY};
use crate::hubspot::RecordType;
use crate::{AppState, SESSION_ACCOUNT_ID_KEY};

/// CRM レコードを読めるログイン方式。パスワードログイン (社内共通 / 外部期限付き) は
/// 個人を特定できないため許可しない。
pub const READ_ALLOWED_LOGIN_METHODS: &[&str] = &[LOGIN_METHOD_GOOGLE_OIDC];

/// CRM レコードを読める役割 (暫定。D-1/D-2 で決まるまで admin のみ)。
pub const READ_ALLOWED_ROLES: &[&str] = &["admin"];

/// 認可判定に使うログイン中の人の情報。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Principal {
    pub email: Option<String>,
    pub login_method: Option<String>,
    /// 監査 DB の `accounts.role`。取れなかった場合は None (= どの役割にも当たらない)
    pub role: Option<String>,
}

/// 読み取りを許可するか。許可条件はここ 1 箇所に集める。
///
/// `record` は今は使わない (全レコード同じ扱い)。レコード単位の絞り込みを入れるときの差し込み口。
pub fn can_read(principal: &Principal, _record: RecordType, allowed_roles: &[&str]) -> bool {
    let method_ok = principal
        .login_method
        .as_deref()
        .is_some_and(|m| READ_ALLOWED_LOGIN_METHODS.contains(&m));
    let role_ok = principal
        .role
        .as_deref()
        .is_some_and(|r| allowed_roles.contains(&r));
    method_ok && role_ok
}

/// セッションと監査 DB から [`Principal`] を作る。
///
/// ログイン方式が許可対象でなければ監査 DB は照会しない (無駄な Turso 呼び出しを避ける)。
pub async fn load_principal(session: &Session, state: &Arc<AppState>) -> Principal {
    let email: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    let login_method: Option<String> = session.get(SESSION_LOGIN_METHOD_KEY).await.unwrap_or(None);
    let mut principal = Principal {
        email,
        login_method,
        role: None,
    };
    let method_ok = principal
        .login_method
        .as_deref()
        .is_some_and(|m| READ_ALLOWED_LOGIN_METHODS.contains(&m));
    if !method_ok {
        return principal;
    }
    // D-10 未決: 監査 DB 未接続なら役割なし (fail closed)
    let Some(audit) = state.audit.clone() else {
        return principal;
    };
    let account_id: Option<String> = session.get(SESSION_ACCOUNT_ID_KEY).await.unwrap_or(None);
    let Some(account_id) = account_id else {
        return principal;
    };
    // require_admin_mw と同じく reqwest::blocking の Turso を spawn_blocking で呼ぶ
    let role = tokio::task::spawn_blocking(move || {
        crate::audit::dao::find_account_by_id(audit.turso(), &account_id)
            // 無効化済みアカウントは役割なし扱い
            .filter(|a| a.disabled_at.trim().is_empty())
            .map(|a| a.role)
            .filter(|r| !r.is_empty())
    })
    .await
    .unwrap_or_else(|e| {
        tracing::warn!("crm rbac spawn_blocking join failed: {e}");
        None
    });
    principal.role = role;
    principal
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::{
        LOGIN_METHOD_PASSWORD, LOGIN_METHOD_PASSWORD_EXTERNAL, LOGIN_METHOD_PASSWORD_INTERNAL,
    };

    fn p(method: Option<&str>, role: Option<&str>) -> Principal {
        Principal {
            email: Some("taro@f-a-c.co.jp".to_string()),
            login_method: method.map(str::to_string),
            role: role.map(str::to_string),
        }
    }

    /// login_method × 役割 × 監査未接続 (role=None) の表
    #[test]
    fn can_read_の判定表() {
        let oidc = Some(LOGIN_METHOD_GOOGLE_OIDC);
        let cases: &[(Option<&str>, Option<&str>, bool)] = &[
            (oidc, Some("admin"), true),
            (oidc, Some("user"), false),
            (oidc, Some(""), false),
            (oidc, Some("Admin"), false), // 大文字小文字は区別する (DB の値そのまま)
            (oidc, None, false),          // 監査未接続 / account_id 無し / 照会失敗
            (Some(LOGIN_METHOD_PASSWORD_INTERNAL), Some("admin"), false),
            (Some(LOGIN_METHOD_PASSWORD_EXTERNAL), Some("admin"), false),
            (Some(LOGIN_METHOD_PASSWORD), Some("admin"), false),
            (None, Some("admin"), false), // login_method 未記録の古いセッション
            (Some("google_oidc "), Some("admin"), false),
        ];
        for (method, role, want) in cases {
            for rt in RecordType::ALL {
                assert_eq!(
                    can_read(&p(*method, *role), rt, READ_ALLOWED_ROLES),
                    *want,
                    "method={method:?} role={role:?} record={rt:?}"
                );
            }
        }
    }

    /// 逆証明: 許可リストを変えると結果が変わる = 判定が READ_ALLOWED_ROLES に依存している
    #[test]
    fn 許可リストに_user_を渡すと_user_が通る() {
        let user = p(Some(LOGIN_METHOD_GOOGLE_OIDC), Some("user"));
        assert!(!can_read(&user, RecordType::Deal, READ_ALLOWED_ROLES));
        assert!(can_read(&user, RecordType::Deal, &["admin", "user"]));
        // 許可リストを空にすると admin も通らない
        let admin = p(Some(LOGIN_METHOD_GOOGLE_OIDC), Some("admin"));
        assert!(!can_read(&admin, RecordType::Deal, &[]));
        // 許可リストを広げてもパスワードログインは通らない
        let pw = p(Some(LOGIN_METHOD_PASSWORD_INTERNAL), Some("user"));
        assert!(!can_read(&pw, RecordType::Deal, &["admin", "user"]));
    }

    #[test]
    fn 既定の許可リストは_admin_だけ() {
        assert_eq!(READ_ALLOWED_ROLES, &["admin"]);
        assert_eq!(READ_ALLOWED_LOGIN_METHODS, &["google_oidc"]);
    }
}
