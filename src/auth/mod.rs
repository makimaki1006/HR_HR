pub mod session;

use axum::{
    extract::Request,
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use tower_sessions::Session;

/// セッションキー
pub const SESSION_USER_KEY: &str = "user_email";
pub const SESSION_JOB_TYPE_KEY: &str = "current_job_type";
pub const SESSION_PREFECTURE_KEY: &str = "current_prefecture";
pub const SESSION_MUNICIPALITY_KEY: &str = "current_municipality";
/// 複数選択対応セッションキー（JSON配列文字列）
pub const SESSION_JOB_TYPES_KEY: &str = "current_job_types";
pub const SESSION_INDUSTRY_RAWS_KEY: &str = "current_industry_raws";

/// 認証ミドルウェア: ログイン済みでなければ /login へリダイレクト
pub async fn require_auth(session: Session, request: Request, next: Next) -> Response {
    let user: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    if user.is_some() {
        next.run(request).await
    } else {
        Redirect::to("/login").into_response()
    }
}

/// メールアドレスのドメインが許可リストに含まれるか検証
/// "*" が含まれていれば全ドメイン許可（@を含むメール形式チェックのみ）
pub fn validate_email_domain(email: &str, allowed_domains: &[String]) -> bool {
    let email_lower = email.to_lowercase();
    if let Some(domain) = email_lower.split('@').nth(1) {
        if allowed_domains.iter().any(|d| d == "*") {
            !domain.is_empty()
        } else {
            allowed_domains.iter().any(|d| d == domain)
        }
    } else {
        false
    }
}

/// パスワード検証（bcryptハッシュまたは平文）
/// 社内パスワードのみチェック。外部パスワードは verify_password_with_externals を使う
pub fn verify_password(input: &str, plain: &str, hash: &str) -> bool {
    if !hash.is_empty() {
        bcrypt::verify(input, hash).unwrap_or(false)
    } else if !plain.is_empty() {
        input == plain
    } else {
        false
    }
}

/// 社内パスワード + 外部パスワード（有効期限付き）を統合チェック
/// 戻り値: (認証OK, 期限切れメッセージ)
pub fn verify_password_with_externals(
    input: &str,
    plain: &str,
    hash: &str,
    external_passwords: &[crate::config::ExternalPassword],
) -> (bool, Option<String>) {
    // 社内パスワード: 無期限
    if verify_password(input, plain, hash) {
        return (true, None);
    }

    // 外部パスワード: 有効期限チェック
    let today = chrono::Local::now().format("%Y-%m-%d").to_string();
    for ext in external_passwords {
        if input == ext.password {
            if today.as_str() <= ext.expires.as_str() {
                return (true, None);
            } else {
                // パスワード一致だが期限切れ
                return (false, Some(format!(
                    "このパスワードの利用期間は {} で終了しました。管理者にお問い合わせください。",
                    ext.expires
                )));
            }
        }
    }

    (false, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    // テスト41: 正しいドメイン → 許可
    #[test]
    fn test_valid_domain_allowed() {
        let domains = vec!["example.com".to_string(), "test.co.jp".to_string()];
        assert!(validate_email_domain("user@example.com", &domains));
        assert!(validate_email_domain("user@test.co.jp", &domains));
    }

    // テスト41逆証明: 不正ドメイン → 拒否
    #[test]
    fn test_invalid_domain_rejected() {
        let domains = vec!["example.com".to_string()];
        assert!(!validate_email_domain("user@evil.com", &domains));
    }

    // ドメインなしメール → 拒否
    #[test]
    fn test_no_at_sign_rejected() {
        let domains = vec!["example.com".to_string()];
        assert!(!validate_email_domain("invalid-email", &domains));
    }

    // 大文字小文字の区別なし
    #[test]
    fn test_case_insensitive_domain() {
        let domains = vec!["example.com".to_string()];
        assert!(validate_email_domain("User@EXAMPLE.COM", &domains));
    }

    // ワイルドカード: * で全ドメイン許可
    #[test]
    fn test_wildcard_domain() {
        let domains = vec!["*".to_string()];
        assert!(validate_email_domain("anyone@anything.com", &domains));
        assert!(validate_email_domain("user@gmail.com", &domains));
        assert!(!validate_email_domain("no-at-sign", &domains));
    }

    // テスト42: 平文パスワード一致 → 認証OK
    #[test]
    fn test_plain_password_match() {
        assert!(verify_password("secret", "secret", ""));
    }

    // テスト42逆証明: 不一致 → 拒否
    #[test]
    fn test_plain_password_mismatch() {
        assert!(!verify_password("wrong", "secret", ""));
    }

    // テスト43: bcryptハッシュ一致 → 認証OK
    #[test]
    fn test_bcrypt_password_match() {
        let hash = bcrypt::hash("mypassword", 4).unwrap();
        assert!(verify_password("mypassword", "", &hash));
    }

    // テスト43逆証明: bcrypt不一致 → 拒否
    #[test]
    fn test_bcrypt_password_mismatch() {
        let hash = bcrypt::hash("mypassword", 4).unwrap();
        assert!(!verify_password("wrongpassword", "", &hash));
    }

    // 両方空 → 拒否
    #[test]
    fn test_no_password_configured() {
        assert!(!verify_password("anything", "", ""));
    }

    // 外部パスワード: 有効期限内 → 認証OK
    #[test]
    fn test_external_password_valid() {
        let externals = vec![crate::config::ExternalPassword {
            password: "ext_pass".to_string(),
            expires: "2099-12-31".to_string(),
        }];
        let (ok, msg) = verify_password_with_externals("ext_pass", "", "", &externals);
        assert!(ok);
        assert!(msg.is_none());
    }

    // 外部パスワード: 期限切れ → 認証NG + メッセージ
    #[test]
    fn test_external_password_expired() {
        let externals = vec![crate::config::ExternalPassword {
            password: "old_pass".to_string(),
            expires: "2020-01-01".to_string(),
        }];
        let (ok, msg) = verify_password_with_externals("old_pass", "", "", &externals);
        assert!(!ok);
        assert!(msg.is_some());
        assert!(msg.unwrap().contains("2020-01-01"));
    }

    // 社内パスワードは外部チェックでも無期限で通る
    #[test]
    fn test_internal_password_via_externals() {
        let externals = vec![];
        let (ok, _) = verify_password_with_externals("secret", "secret", "", &externals);
        assert!(ok);
    }

    // 外部パスワード不一致 → 認証NG、メッセージなし
    #[test]
    fn test_external_password_wrong() {
        let externals = vec![crate::config::ExternalPassword {
            password: "ext_pass".to_string(),
            expires: "2099-12-31".to_string(),
        }];
        let (ok, msg) = verify_password_with_externals("wrong", "", "", &externals);
        assert!(!ok);
        assert!(msg.is_none());
    }
}
