pub mod google_oidc;
#[cfg(test)]
mod login_flow_tests;
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
/// ログイン方式 (`LOGIN_METHOD_*` のいずれか)。監査の login_sessions.login_method と同じ値。
/// CRM の API はこの値が `google_oidc` のセッションだけを通す予定 (計画書 C-3)。
pub const SESSION_LOGIN_METHOD_KEY: &str = "login_method";

/// Google Workspace OIDC (本人確認済み)
pub const LOGIN_METHOD_GOOGLE_OIDC: &str = "google_oidc";
/// 社内パスワード (AUTH_PASSWORD / AUTH_PASSWORD_HASH、共有・無期限)
pub const LOGIN_METHOD_PASSWORD_INTERNAL: &str = "password_internal";
/// 外部の期限付きパスワード (AUTH_PASSWORDS_EXTRA)
pub const LOGIN_METHOD_PASSWORD_EXTERNAL: &str = "password_external";
/// パスワードログインの失敗記録用 (どちらのパスワードを狙ったかは分からない)
pub const LOGIN_METHOD_PASSWORD: &str = "password";

/// 未ログインの `/api/*` を 401 JSON にする条件 (2026-09-30、F-5)。
///
/// 3 つ全部を満たすときだけ 401 JSON、それ以外は従来どおり 303 `/login`:
/// 1. パスが `/api/` で始まる
/// 2. `HX-Request` ヘッダーが無い (HTMX の既存呼び出しは 303 のまま。HTMX は必ず付ける)
/// 3. `Accept` に `application/json` を含む (React の client.ts は付ける。`*/*` だけなら 303)
///
/// `/api/v1/*` (認証不要) や `/scout/*` は `auth_middleware` に入らないので影響しない。
pub fn wants_json_401(request: &Request) -> bool {
    if !request.uri().path().starts_with("/api/") {
        return false;
    }
    let headers = request.headers();
    if headers.contains_key("hx-request") {
        return false;
    }
    headers
        .get_all(axum::http::header::ACCEPT)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .any(|v| {
            v.split(',')
                .any(|part| part.split(';').next().unwrap_or("").trim().eq_ignore_ascii_case("application/json"))
        })
}

/// 401 の JSON ボディ。React の `client.ts` はこの形 (`error: auth_required`) を `AuthRequiredError` にする。
pub const AUTH_REQUIRED_JSON: &str = r#"{"error":"auth_required","login_url":"/login"}"#;

/// 未ログイン時の応答。`require_auth` と `auth_middleware` (lib.rs) の両経路で共通。
pub fn unauthenticated_response(request: &Request) -> Response {
    if wants_json_401(request) {
        (
            axum::http::StatusCode::UNAUTHORIZED,
            [(
                axum::http::header::CONTENT_TYPE,
                axum::http::HeaderValue::from_static("application/json"),
            )],
            AUTH_REQUIRED_JSON,
        )
            .into_response()
    } else {
        Redirect::to("/login").into_response()
    }
}

/// 認証ミドルウェア: ログイン済みでなければ /login へリダイレクト
/// (`/api/*` + `Accept: application/json` + `HX-Request` 無しのときだけ 401 JSON、`unauthenticated_response`)
pub async fn require_auth(session: Session, request: Request, next: Next) -> Response {
    let user: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
    if user.is_some() {
        next.run(request).await
    } else {
        unauthenticated_response(&request)
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
                return (
                    false,
                    Some(format!(
                    "このパスワードの利用期間は {} で終了しました。管理者にお問い合わせください。",
                    ext.expires
                )),
                );
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

    // ---- /api/* の 401 JSON 条件 (2026-09-30、F-5) ----

    fn req(uri: &str, headers: &[(&str, &str)]) -> Request {
        let mut b = axum::http::Request::builder().uri(uri);
        for (k, v) in headers {
            b = b.header(*k, *v);
        }
        b.body(axum::body::Body::empty()).unwrap()
    }

    #[test]
    fn 三条件がそろったときだけ401json() {
        // /api/ + Accept json + HX-Request 無し → 401
        assert!(wants_json_401(&req("/api/nav", &[("accept", "application/json")])));
        assert!(wants_json_401(&req(
            "/api/nav?x=1",
            &[("accept", "text/html, application/json;q=0.9")]
        )));
        assert!(wants_json_401(&req("/api/nav", &[("accept", "Application/JSON")])));
        // HX-Request があれば 303 (HTMX の既存呼び出し)
        assert!(!wants_json_401(&req(
            "/api/nav",
            &[("accept", "application/json"), ("hx-request", "true")]
        )));
        // Accept 無し / */* だけ / text/html → 303
        assert!(!wants_json_401(&req("/api/nav", &[])));
        assert!(!wants_json_401(&req("/api/nav", &[("accept", "*/*")])));
        assert!(!wants_json_401(&req("/api/nav", &[("accept", "text/html")])));
        // /api/ 以外 → 303 (Accept json でも)
        assert!(!wants_json_401(&req("/tab/market", &[("accept", "application/json")])));
        assert!(!wants_json_401(&req("/", &[("accept", "application/json")])));
        assert!(!wants_json_401(&req("/app/dummy", &[("accept", "application/json")])));
        assert!(!wants_json_401(&req("/apix", &[("accept", "application/json")])));
    }

    #[tokio::test]
    async fn 未ログイン応答は401jsonか303login() {
        use axum::http::{header, StatusCode};
        let r = unauthenticated_response(&req("/api/nav", &[("accept", "application/json")]));
        assert_eq!(r.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(r.headers()[header::CONTENT_TYPE], "application/json");
        let body = axum::body::to_bytes(r.into_body(), 1024).await.unwrap();
        let v: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            v,
            serde_json::json!({"error": "auth_required", "login_url": "/login"})
        );

        let r = unauthenticated_response(&req(
            "/api/nav",
            &[("accept", "application/json"), ("hx-request", "true")],
        ));
        assert_eq!(r.status(), StatusCode::SEE_OTHER);
        assert_eq!(r.headers()[header::LOCATION], "/login");
        let r = unauthenticated_response(&req("/tab/market", &[]));
        assert_eq!(r.status(), StatusCode::SEE_OTHER);
        assert_eq!(r.headers()[header::LOCATION], "/login");
    }
}
