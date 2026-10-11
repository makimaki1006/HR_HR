//! 認証だけを再起動後に復元する。CSV・フィルタ等は MemoryStore に残す。
use std::sync::Arc;

use axum::{
    extract::{Request, State},
    http::header,
    middleware::Next,
    response::{IntoResponse, Response},
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
pub(crate) const COOKIE_ISSUED_KEY: &str = "login_cookie_issued";
pub(crate) const EXTERNAL_EXPIRY_KEY: &str = "external_password_expires";

#[derive(Clone)]
pub(crate) struct CookieState {
    pub app: Arc<AppState>,
    pub key: Option<Key>,
    pub secure: bool,
}

impl CookieState {
    pub fn from_env(app: Arc<AppState>) -> Self {
        let key = configured_key(std::env::var("SESSION_COOKIE_KEY").ok().as_deref());
        if key.is_some() && app.audit.is_none() {
            tracing::warn!("監査 DB 未接続のため認証 Cookie は発行しません。ログインはメモリだけに保持されます");
        }
        Self {
            app,
            key,
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
    session_id: String,
    expires: i64,
    external_expires: Option<String>,
}

impl Login {
    fn valid(&self) -> bool {
        !self.email.trim().is_empty()
            && !self.session_id.is_empty()
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

/// DB の障害中は Cookie の復元もログアウト完了扱いも行わない。
pub(crate) fn unavailable() -> Response {
    let mut response = (
        axum::http::StatusCode::SERVICE_UNAVAILABLE,
        "ログイン状態を確認できません。時間をおいて再度お試しください。",
    )
        .into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, "no-store".parse().unwrap());
    response
}

async fn verified_account(state: &AppState, login: &Login) -> Result<Option<String>, ()> {
    let audit = state.audit.clone().ok_or(())?;
    let sid = login.session_id.clone();
    let email = login.email.clone();
    let method = login.method.clone();
    tokio::task::spawn_blocking(move || {
        crate::audit::dao::active_login_account(audit.turso(), &sid, &email, &method)
    })
    .await
    .map_err(|_| ())?
    .map_err(|_| ())
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
    if path == "/logout" && had_cookie && state.key.is_none() {
        // 発行済み Cookie の鍵を失っている状態では、失効できたことを確認できない。
        return unavailable();
    }
    let mut restored = false;
    let mut previous_expiry = None;
    let mut previous_sid = None;
    if let Some(key) = &state.key {
        // ログアウトでは、Cookie が一部欠けても既存メモリの参照を失効処理まで残す。
        if path != "/logout"
            && !had_cookie
            && session.get::<bool>(COOKIE_ISSUED_KEY).await.unwrap_or(None) == Some(true)
        {
            let _ = session.flush().await;
        }
        if had_cookie {
            if let Some(login) = decode(&jar, key) {
                let account_id = match verified_account(&state.app, &login).await {
                    Ok(id) => id,
                    Err(()) => return unavailable(),
                };
                if let Some(account_id) = account_id {
                    previous_expiry = Some(login.expires);
                    previous_sid = Some(login.session_id.clone());
                    let email: Option<String> = session.get(SESSION_USER_KEY).await.unwrap_or(None);
                    let method: Option<String> =
                        session.get(SESSION_LOGIN_METHOD_KEY).await.unwrap_or(None);
                    let sid: Option<String> = session
                        .get(crate::SESSION_LOGIN_SESSION_ID_KEY)
                        .await
                        .unwrap_or(None);
                    if email.as_deref() != Some(&login.email)
                        || method.as_deref() != Some(&login.method)
                        || sid.as_deref() != Some(&login.session_id)
                    {
                        let _ = session.flush().await;
                        let _ = session.insert(SESSION_USER_KEY, &login.email).await;
                        let _ = session
                            .insert(SESSION_LOGIN_METHOD_KEY, &login.method)
                            .await;
                        let _ = session
                            .insert(crate::SESSION_LOGIN_SESSION_ID_KEY, &login.session_id)
                            .await;
                        let _ = session
                            .insert(crate::SESSION_ACCOUNT_ID_KEY, account_id)
                            .await;
                        if let Some(expiry) = &login.external_expires {
                            let _ = session.insert(EXTERNAL_EXPIRY_KEY, expiry).await;
                        }
                        restored = true;
                    }
                    let _ = session.insert(COOKIE_ISSUED_KEY, true).await;
                } else if path != "/logout" {
                    let _ = session.flush().await;
                }
            } else if path != "/logout" {
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
            if path != "/logout" {
                let _ = session.flush().await;
            }
        }
    }
    let rejected = super::unauthenticated_response(&request);
    let mut response = next.run(request).await;
    // 失効確認・失効書き込みが失敗した応答では、Cookie を更新・削除しない。
    if response.status().is_server_error() {
        return response;
    }
    if let Some(key) = &state.key {
        let email = session
            .get::<String>(SESSION_USER_KEY)
            .await
            .unwrap_or(None);
        let method = session
            .get::<String>(SESSION_LOGIN_METHOD_KEY)
            .await
            .unwrap_or(None);
        let sid = session
            .get::<String>(crate::SESSION_LOGIN_SESSION_ID_KEY)
            .await
            .unwrap_or(None);
        if email.is_some() && method.is_some() {
            // 再ログインでも以前の Cookie が復活しないよう、元のログインを終了する。
            if let Some(old_sid) = previous_sid.filter(|old| sid.as_deref() != Some(old.as_str())) {
                let Some(audit) = state.app.audit.clone() else {
                    return unavailable();
                };
                if !matches!(
                    tokio::task::spawn_blocking(move || crate::audit::dao::mark_session_ended(
                        &audit, &old_sid
                    ))
                    .await,
                    Ok(Ok(()))
                ) {
                    let _ = session.flush().await;
                    return unavailable();
                }
            }
        }
        if let (Some(email), Some(method), Some(session_id)) = (email, method, sid) {
            if session.is_modified() || !had_cookie {
                let expires = if restored && !login_request {
                    previous_expiry.unwrap()
                } else {
                    session.expiry_date().unix_timestamp()
                };
                let login = Login {
                    email,
                    method,
                    session_id,
                    expires,
                    external_expires: session.get(EXTERNAL_EXPIRY_KEY).await.unwrap_or(None),
                };
                if login.valid() {
                    match verified_account(&state.app, &login).await {
                        Ok(Some(_)) => {
                            let mut c = cookie(
                                serde_json::to_string(&login).expect("login serialization"),
                                state.secure,
                            );
                            c.set_expires(
                                time::OffsetDateTime::from_unix_timestamp(expires).unwrap(),
                            );
                            let _ = session.insert(COOKIE_ISSUED_KEY, true).await;
                            jar.private_mut(key).add(c);
                        }
                        Ok(None) => {
                            let _ = session.flush().await;
                            return rejected;
                        }
                        Err(()) => return unavailable(),
                    }
                }
            }
        } else {
            if login_request
                && session
                    .get::<String>(SESSION_USER_KEY)
                    .await
                    .unwrap_or(None)
                    .is_some()
            {
                tracing::warn!("ログイン履歴を保存できなかったため認証 Cookie は発行しません。ログインはメモリだけに保持されます");
            }
            if had_cookie {
                jar.remove(cookie(String::new(), state.secure));
            }
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
                session_id: "test-login".into(),
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
        // tracing の callsite 判定はプロセス共通。ほかのテストの subscriber と競合させない。
        const TEST: &str =
            "auth::login_cookie::tests::未設定と不正な鍵は値を出さず警告してメモリに戻る";
        if !std::env::args().any(|arg| arg == "--exact") {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", TEST, "--test-threads=1", "--nocapture"])
                .output()
                .unwrap();
            assert!(
                output.status.success()
                    && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
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
