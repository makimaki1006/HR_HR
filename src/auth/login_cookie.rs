//! 認証だけを再起動後に復元する。CSV・フィルタ等は MemoryStore に残す。
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::Response,
};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde::{Deserialize, Serialize};
use tower_sessions::{
    cookie::{Cookie, CookieJar, Key, SameSite},
    Session,
};

use super::{
    LOGIN_METHOD_GOOGLE_OIDC, LOGIN_METHOD_PASSWORD_EXTERNAL, LOGIN_METHOD_PASSWORD_INTERNAL,
    SESSION_LOGIN_METHOD_KEY, SESSION_USER_KEY,
};
use crate::AppState;

pub(crate) const COOKIE_NAME: &str = "hr_login";
const COOKIE_ISSUED_KEY: &str = "login_cookie_issued";
pub(crate) const EXTERNAL_EXPIRY_KEY: &str = "external_password_expires";

#[derive(Clone)]
pub(crate) struct CookieState {
    pub app: Arc<AppState>,
    pub key: Option<Key>,
    pub secure: bool,
}

impl CookieState {
    pub fn from_env(app: Arc<AppState>) -> Self {
        Self {
            app,
            key: configured_key(std::env::var("SESSION_COOKIE_KEY").ok().as_deref()),
            secure: crate::config::is_production_env(),
        }
    }
}

fn configured_key(value: Option<&str>) -> Option<Key> {
    let key = value
        .and_then(|v| STANDARD.decode(v.trim()).ok())
        .and_then(|bytes| Key::try_from(bytes.as_slice()).ok());
    if key.is_none() {
        tracing::warn!("SESSION_COOKIE_KEY が未設定または不正です。ログインはメモリだけに保持され、再起動で失われます");
    }
    key
}

#[derive(Serialize, Deserialize)]
struct Login {
    email: String,
    method: String,
    expires: i64,
    external_expires: Option<String>,
}

impl Login {
    fn valid(&self) -> bool {
        !self.email.trim().is_empty()
            && self.expires > time::OffsetDateTime::now_utc().unix_timestamp()
            && matches!(
                self.method.as_str(),
                LOGIN_METHOD_GOOGLE_OIDC
                    | LOGIN_METHOD_PASSWORD_INTERNAL
                    | LOGIN_METHOD_PASSWORD_EXTERNAL
            )
            && (self.method != LOGIN_METHOD_PASSWORD_EXTERNAL
                || external_valid(self.external_expires.as_deref()))
    }
}

fn external_valid(expiry: Option<&str>) -> bool {
    expiry
        .and_then(|s| chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d").ok())
        .is_some_and(|date| date >= chrono::Local::now().date_naive())
}

fn cookie(value: String, secure: bool) -> Cookie<'static> {
    Cookie::build((COOKIE_NAME, value))
        .path("/")
        .http_only(true)
        .secure(secure)
        .same_site(if secure {
            SameSite::Strict
        } else {
            SameSite::Lax
        })
        .build()
}

fn decode(jar: &CookieJar, key: &Key) -> Option<Login> {
    let c = jar.private(key).get(COOKIE_NAME)?;
    let login: Login = serde_json::from_str(c.value()).ok()?;
    login.valid().then_some(login)
}

pub(crate) async fn restore_login(
    State(state): State<CookieState>,
    session: Session,
    request: Request,
    next: Next,
) -> Response {
    let path = request.uri().path();
    let login_request = path == "/login" || path == "/auth/google/callback";
    if path.starts_with("/static") || path.starts_with("/scout") || path == "/health" {
        return next.run(request).await;
    }
    let mut jar = CookieJar::new();
    for h in request.headers().get_all(header::COOKIE) {
        if let Ok(h) = h.to_str() {
            for c in Cookie::split_parse(h).flatten() {
                jar.add_original(c.into_owned());
            }
        }
    }
    let had_cookie = jar.get(COOKIE_NAME).is_some();
    let mut restored = false;
    let mut previous_expiry = None;
    if let Some(key) = &state.key {
        if !had_cookie && session.get::<bool>(COOKIE_ISSUED_KEY).await.unwrap_or(None) == Some(true)
        {
            // ブラウザが期限切れ Cookie を送らなくなっても、メモリの ID だけで認証を延長しない。
            let _ = session.flush().await;
        }
        if had_cookie {
            if let Some(login) = decode(&jar, key) {
                previous_expiry = Some(login.expires);
                let email: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
                let method: Option<String> =
                    session.get(SESSION_LOGIN_METHOD_KEY).await.unwrap_or(None);
                if email.as_deref() != Some(&login.email)
                    || method.as_deref() != Some(&login.method)
                {
                    // Cookie が認証の根拠。別のメモリセッションの一時データは引き継がない。
                    let _ = session.flush().await;
                    let _ = session.insert(SESSION_USER_KEY, &login.email).await;
                    let _ = session
                        .insert(SESSION_LOGIN_METHOD_KEY, &login.method)
                        .await;
                    if let Some(expiry) = &login.external_expires {
                        let _ = session.insert(EXTERNAL_EXPIRY_KEY, expiry).await;
                    }
                    // 役割や監査 ID は Cookie に持たず、既存監査 DB から読み直す。
                    if let Some(audit) = &state.app.audit {
                        let audit = audit.clone();
                        let email = login.email.clone();
                        if let Ok(Some(account)) = tokio::task::spawn_blocking(move || {
                            crate::audit::dao::find_account_by_email(audit.turso(), &email)
                        })
                        .await
                        {
                            let _ = session
                                .insert(crate::SESSION_ACCOUNT_ID_KEY, account.id)
                                .await;
                        }
                    }
                    let _ = session.insert(COOKIE_ISSUED_KEY, true).await;
                    restored = true;
                }
            } else {
                let _ = session.flush().await;
            }
        }
    }
    if let Some(email) = session
        .get::<String>(SESSION_USER_KEY)
        .await
        .unwrap_or(None)
    {
        let method = session
            .get::<String>(SESSION_LOGIN_METHOD_KEY)
            .await
            .unwrap_or(None);
        let expiry = session
            .get::<String>(EXTERNAL_EXPIRY_KEY)
            .await
            .unwrap_or(None);
        if crate::account_is_disabled(&state.app, &email).await
            || (method.as_deref() == Some(LOGIN_METHOD_PASSWORD_EXTERNAL)
                && !external_valid(expiry.as_deref()))
        {
            let _ = session.flush().await;
        }
    }
    let mut response = next.run(request).await;
    if let Some(key) = &state.key {
        let email = session
            .get::<String>(SESSION_USER_KEY)
            .await
            .unwrap_or(None);
        let method = session
            .get::<String>(SESSION_LOGIN_METHOD_KEY)
            .await
            .unwrap_or(None);
        if let (Some(email), Some(method)) = (email, method) {
            // MemoryStore と同じ更新条件で24時間の期限を延長する。
            if session.is_modified() || !had_cookie {
                let expires = if restored && !login_request {
                    previous_expiry.unwrap()
                } else {
                    session.expiry_date().unix_timestamp()
                };
                let login = Login {
                    email,
                    method,
                    expires,
                    external_expires: session.get(EXTERNAL_EXPIRY_KEY).await.unwrap_or(None),
                };
                if login.valid() {
                    let mut c = cookie(
                        serde_json::to_string(&login).expect("login serialization"),
                        state.secure,
                    );
                    c.set_expires(time::OffsetDateTime::from_unix_timestamp(expires).unwrap());
                    let _ = session.insert(COOKIE_ISSUED_KEY, true).await;
                    jar.private_mut(key).add(c);
                }
            }
        } else if had_cookie {
            jar.remove(cookie(String::new(), state.secure));
        }
        for c in jar.delta() {
            if let Ok(value) = c.to_string().parse() {
                response.headers_mut().append(header::SET_COOKIE, value);
            }
        }
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use std::sync::Mutex;

    #[test]
    fn 期限切れと不明な方式と社外期限を拒否し本番属性を保持する() {
        let key = Key::from(&[42; 64]);
        let now = time::OffsetDateTime::now_utc().unix_timestamp();
        for (method, expires, external, valid) in [
            (LOGIN_METHOD_GOOGLE_OIDC, now + 86400, None, true),
            (LOGIN_METHOD_PASSWORD_INTERNAL, now + 86400, None, true),
            (LOGIN_METHOD_GOOGLE_OIDC, now - 1, None, false),
            ("unknown", now + 86400, None, false),
            (
                LOGIN_METHOD_PASSWORD_EXTERNAL,
                now + 86400,
                Some("2020-01-01"),
                false,
            ),
            (
                LOGIN_METHOD_PASSWORD_EXTERNAL,
                now + 86400,
                Some("2099-12-31"),
                true,
            ),
            (LOGIN_METHOD_PASSWORD_EXTERNAL, now + 86400, None, false),
            (
                LOGIN_METHOD_PASSWORD_EXTERNAL,
                now + 86400,
                Some("invalid"),
                false,
            ),
        ] {
            let login = Login {
                email: "taro@f-a-c.co.jp".into(),
                method: method.into(),
                expires,
                external_expires: external.map(str::to_string),
            };
            let mut jar = CookieJar::new();
            jar.private_mut(&key)
                .add(cookie(serde_json::to_string(&login).unwrap(), true));
            assert_eq!(decode(&jar, &key).is_some(), valid);
            let header = jar.delta().next().unwrap().to_string();
            assert!(
                header.contains("Secure")
                    && header.contains("HttpOnly")
                    && header.contains("SameSite=Strict")
                    && header.contains("Path=/")
            );
        }
    }

    #[test]
    fn 未設定と不正な鍵は値を出さず警告してメモリに戻る() {
        #[derive(Clone)]
        struct Writer(Arc<Mutex<Vec<u8>>>);
        impl Write for Writer {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                self.0.lock().unwrap().extend_from_slice(buf);
                Ok(buf.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let buffer = Arc::new(Mutex::new(Vec::new()));
        let writer = Writer(buffer.clone());
        let subscriber = tracing_subscriber::fmt()
            .without_time()
            .with_ansi(false)
            .with_writer(move || writer.clone())
            .finish();
        tracing::subscriber::with_default(subscriber, || {
            assert!(configured_key(None).is_none());
            assert!(configured_key(Some("secret-invalid-value")).is_none());
            assert!(configured_key(Some(&STANDARD.encode([0; 63]))).is_none());
            assert!(configured_key(Some(&STANDARD.encode([42; 64]))).is_some());
        });
        let logged = String::from_utf8(buffer.lock().unwrap().clone()).unwrap();
        assert_eq!(logged.matches("WARN").count(), 3);
        assert!(logged.contains("SESSION_COOKIE_KEY"));
        assert!(!logged.contains("secret-invalid-value"));
    }
}
