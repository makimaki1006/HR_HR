use std::env;

/// Base64-encoded 32-byte AES-256 key used only by Scout credential APIs.
pub const SCOUT_CREDENTIALS_KEY_ENV: &str = "SCOUT_CREDENTIALS_KEY";

/// 外部パスワード（有効期限付き）
#[derive(Debug, Clone)]
pub struct ExternalPassword {
    pub password: String,
    /// 有効期限（YYYY-MM-DD形式）。この日を含む最終日まで有効
    pub expires: String,
}

/// 本番 (Render) で動いているか。Cookie の Secure / SameSite の切替に使う。
pub fn is_production_env() -> bool {
    env::var("RENDER").is_ok() || env::var("RENDER_SERVICE_NAME").is_ok()
}

/// Google Workspace OIDC ログインの設定 (ADR-017)。
///
/// 4 つの環境変数が **すべて** 空でないときだけ `Some`。1 つでも欠けたら OIDC は無効で、
/// ログイン画面にボタンを出さず `/auth/google/*` は 404 を返す (既存のパスワードログインは従来どおり)。
///
/// `hosted_domain` は ID token の `hd` クレームと email のドメインの照合にだけ使う。
/// `ALLOWED_DOMAINS` は流用しない (`*` を設定すると hd 検証が骨抜きになるため)。
#[derive(Clone)]
pub struct GoogleOidcConfig {
    pub client_id: String,
    pub client_secret: String,
    /// Google Cloud に登録した承認済みリダイレクト URI と完全一致させる
    /// (例 `https://hr-hw.onrender.com/auth/google/callback`)
    pub redirect_url: String,
    /// 小文字に正規化済み (例 `f-a-c.co.jp`)
    pub hosted_domain: String,
}

impl std::fmt::Debug for GoogleOidcConfig {
    // client_secret をログに出さない
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("GoogleOidcConfig")
            .field("client_id", &self.client_id)
            .field("client_secret", &"***")
            .field("redirect_url", &self.redirect_url)
            .field("hosted_domain", &self.hosted_domain)
            .finish()
    }
}

impl GoogleOidcConfig {
    /// 環境変数から読む。1 つでも未設定・空なら None (OIDC 無効)。
    pub fn from_env() -> Option<Self> {
        let raw = [
            env::var("GOOGLE_OIDC_CLIENT_ID").ok(),
            env::var("GOOGLE_OIDC_CLIENT_SECRET").ok(),
            env::var("GOOGLE_OIDC_REDIRECT_URL").ok(),
            env::var("GOOGLE_OIDC_HOSTED_DOMAIN").ok(),
        ];
        let any_set = raw.iter().flatten().any(|v| !v.trim().is_empty());
        let [id, secret, redirect, hd] = raw;
        let cfg = Self::from_values(id, secret, redirect, hd);
        if cfg.is_none() && any_set {
            tracing::warn!(
                "GOOGLE_OIDC_* が一部だけ設定されています。4 つ全部揃うまで Google ログインは無効です"
            );
        }
        cfg
    }

    /// 値から組み立てる (テストでも使う)。前後の空白は落とし、空文字は未設定扱い。
    pub fn from_values(
        client_id: Option<String>,
        client_secret: Option<String>,
        redirect_url: Option<String>,
        hosted_domain: Option<String>,
    ) -> Option<Self> {
        let norm = |v: Option<String>| v.map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
        Some(Self {
            client_id: norm(client_id)?,
            client_secret: norm(client_secret)?,
            redirect_url: norm(redirect_url)?,
            hosted_domain: norm(hosted_domain)?.to_lowercase(),
        })
    }
}

/// アプリケーション設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// サーバーポート
    pub port: u16,
    /// ログインパスワード（平文・社内用・無期限）
    pub auth_password: String,
    /// ログインパスワード（bcryptハッシュ・社内用・無期限）
    pub auth_password_hash: String,
    /// 外部パスワードリスト（有効期限付き）
    /// 環境変数: AUTH_PASSWORDS_EXTRA=pass1:2026-06-30,pass2:2026-12-31
    pub external_passwords: Vec<ExternalPassword>,
    /// 許可ドメインリスト
    pub allowed_domains: Vec<String>,
    /// 外部用追加許可ドメインリスト
    /// 環境変数: ALLOWED_DOMAINS_EXTRA=gmail.com,client.co.jp
    pub allowed_domains_extra: Vec<String>,
    /// ハローワークDBパス
    pub hellowork_db_path: String,
    /// Indeed 採用市場データ（分析層）。無くてもアプリは起動する
    pub indeed_db_path: String,
    /// キャッシュTTL（秒）
    pub cache_ttl_secs: u64,
    /// キャッシュ最大エントリ数
    pub cache_max_entries: usize,
    /// レート制限: 最大試行回数
    pub rate_limit_max_attempts: u32,
    /// レート制限: ロックアウト秒数
    pub rate_limit_lockout_secs: u64,
    /// 監査DB URL (Turso。空なら監査機能OFF)
    pub audit_turso_url: String,
    /// 監査DB 認証トークン
    pub audit_turso_token: String,
    /// IP ハッシュ化ソルト
    pub audit_ip_salt: String,
    /// 管理者メールアドレス（カンマ区切り）。ログイン時に role=admin 付与
    pub admin_emails: Vec<String>,
    /// 外部統計 Turso DB URL (空なら未設定扱い、監査と同パターン)
    pub turso_external_url: String,
    /// 外部統計 Turso DB 認証トークン
    pub turso_external_token: String,
    /// SalesNow Turso DB URL (空なら未設定扱い)
    pub salesnow_turso_url: String,
    /// SalesNow Turso DB 認証トークン
    pub salesnow_turso_token: String,
    /// Scout(スカウト自動化) 用 Turso DB URL (空なら scout 機能OFF)
    pub scout_turso_url: String,
    /// Scout 用 Turso DB 認証トークン
    pub scout_turso_token: String,
}

/// AUDIT_IP_SALT のデフォルト値（本番未設定時に warn 警告を出す対象）
pub(crate) const DEFAULT_AUDIT_IP_SALT: &str = "hellowork-default-salt";

impl AppConfig {
    /// 環境変数から設定を読み込む
    pub fn from_env() -> Self {
        let cfg = Self {
            port: env::var("PORT")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(9216),
            auth_password: env::var("AUTH_PASSWORD").unwrap_or_default(),
            auth_password_hash: env::var("AUTH_PASSWORD_HASH").unwrap_or_default(),
            external_passwords: env::var("AUTH_PASSWORDS_EXTRA")
                .unwrap_or_default()
                .split(',')
                .filter(|s| !s.trim().is_empty())
                .filter_map(|entry| {
                    let parts: Vec<&str> = entry.trim().splitn(2, ':').collect();
                    if parts.len() == 2 && !parts[0].is_empty() && !parts[1].is_empty() {
                        Some(ExternalPassword {
                            password: parts[0].to_string(),
                            expires: parts[1].to_string(),
                        })
                    } else {
                        tracing::warn!("AUTH_PASSWORDS_EXTRA の形式不正（無視）: {}", entry);
                        None
                    }
                })
                .collect(),
            // 2026-08-10: cyxen.co.jp を許可ドメインから削除（ユーザー指示）。
            // このドメインのアカウントはログインできなくなる。
            allowed_domains: env::var("ALLOWED_DOMAINS")
                .unwrap_or_else(|_| "f-a-c.co.jp".to_string())
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect(),
            allowed_domains_extra: env::var("ALLOWED_DOMAINS_EXTRA")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            indeed_db_path: env::var("INDEED_DB_PATH")
                .unwrap_or_else(|_| "data/indeed_insights.db".to_string()),
            hellowork_db_path: env::var("HELLOWORK_DB_PATH")
                .unwrap_or_else(|_| "data/hellowork.db".to_string()),
            cache_ttl_secs: env::var("CACHE_TTL_SECS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(1800),
            cache_max_entries: env::var("CACHE_MAX_ENTRIES")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(3000),
            rate_limit_max_attempts: env::var("RATE_LIMIT_MAX_ATTEMPTS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(5),
            rate_limit_lockout_secs: env::var("RATE_LIMIT_LOCKOUT_SECONDS")
                .ok()
                .and_then(|v| v.parse().ok())
                .unwrap_or(300),
            audit_turso_url: env::var("AUDIT_TURSO_URL").unwrap_or_default(),
            audit_turso_token: env::var("AUDIT_TURSO_TOKEN").unwrap_or_default(),
            audit_ip_salt: env::var("AUDIT_IP_SALT")
                .unwrap_or_else(|_| DEFAULT_AUDIT_IP_SALT.to_string()),
            admin_emails: env::var("ADMIN_EMAILS")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
            turso_external_url: env::var("TURSO_EXTERNAL_URL").unwrap_or_default(),
            turso_external_token: env::var("TURSO_EXTERNAL_TOKEN").unwrap_or_default(),
            salesnow_turso_url: env::var("SALESNOW_TURSO_URL").unwrap_or_default(),
            salesnow_turso_token: env::var("SALESNOW_TURSO_TOKEN").unwrap_or_default(),
            scout_turso_url: env::var("SCOUT_TURSO_URL").unwrap_or_default(),
            scout_turso_token: env::var("SCOUT_TURSO_TOKEN").unwrap_or_default(),
        };

        // 起動時セキュリティ警告: AUDIT_IP_SALT がデフォルト値のまま本番運用されると
        // レインボーテーブル攻撃で IP ハッシュが復元可能になるため、運用者に通知する
        if cfg.audit_ip_salt == DEFAULT_AUDIT_IP_SALT {
            tracing::warn!(
                "AUDIT_IP_SALT がデフォルト値です。本番では必ず固有の salt を環境変数に設定してください（IP ハッシュのレインボーテーブル攻撃対策）"
            );
        }

        // 2026-05-22 セキュリティ修正 (Agent A3 H6): 本番環境で AUTH_PASSWORD (平文)
        // が設定されている場合、強い警告を発する。AUTH_PASSWORD_HASH (bcrypt) のみ
        // 使うべき。本番判定は RENDER env で。
        let is_production = env::var("RENDER").is_ok() || env::var("RENDER_SERVICE_NAME").is_ok();
        if is_production && !cfg.auth_password.is_empty() {
            tracing::error!(
                "[SECURITY] 本番環境で AUTH_PASSWORD (平文) が設定されています。\
                 AUTH_PASSWORD_HASH (bcrypt) のみを使用してください。\
                 平文パスワードは認証ログ・メモリダンプ・config 漏洩で即漏洩します。"
            );
        }

        cfg
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn clear_env() {
        for key in &[
            "PORT",
            "AUTH_PASSWORD",
            "AUTH_PASSWORD_HASH",
            "AUTH_PASSWORDS_EXTRA",
            "ALLOWED_DOMAINS",
            "ALLOWED_DOMAINS_EXTRA",
            "HELLOWORK_DB_PATH",
            "CACHE_TTL_SECS",
            "CACHE_MAX_ENTRIES",
            "RATE_LIMIT_MAX_ATTEMPTS",
            "RATE_LIMIT_LOCKOUT_SECONDS",
            "AUDIT_TURSO_URL",
            "AUDIT_TURSO_TOKEN",
            "AUDIT_IP_SALT",
            "ADMIN_EMAILS",
            "TURSO_EXTERNAL_URL",
            "TURSO_EXTERNAL_TOKEN",
            "SALESNOW_TURSO_URL",
            "SALESNOW_TURSO_TOKEN",
        ] {
            env::remove_var(key);
        }
    }

    #[test]
    fn test_default_port() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let config = AppConfig::from_env();
        assert_eq!(config.port, 9216);
    }

    #[test]
    fn test_hellowork_db_default() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let config = AppConfig::from_env();
        assert_eq!(config.hellowork_db_path, "data/hellowork.db");
    }

    #[test]
    fn test_turso_external_default_empty() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let config = AppConfig::from_env();
        assert_eq!(config.turso_external_url, "");
        assert_eq!(config.turso_external_token, "");
    }

    #[test]
    fn test_salesnow_turso_default_empty() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        let config = AppConfig::from_env();
        assert_eq!(config.salesnow_turso_url, "");
        assert_eq!(config.salesnow_turso_token, "");
    }

    #[test]
    fn test_turso_external_from_env() {
        let _lock = ENV_LOCK.lock().unwrap();
        clear_env();
        env::set_var("TURSO_EXTERNAL_URL", "libsql://example.turso.io");
        env::set_var("TURSO_EXTERNAL_TOKEN", "tok123");
        let config = AppConfig::from_env();
        assert_eq!(config.turso_external_url, "libsql://example.turso.io");
        assert_eq!(config.turso_external_token, "tok123");
        env::remove_var("TURSO_EXTERNAL_URL");
        env::remove_var("TURSO_EXTERNAL_TOKEN");
    }

    fn s(v: &str) -> Option<String> {
        Some(v.to_string())
    }

    #[test]
    fn google_oidc_is_enabled_only_when_all_four_values_are_set() {
        let cfg = GoogleOidcConfig::from_values(
            s("cid"),
            s("secret"),
            s("https://hr-hw.onrender.com/auth/google/callback"),
            s(" F-A-C.co.jp "),
        )
        .expect("4 つ揃えば有効");
        assert_eq!(cfg.hosted_domain, "f-a-c.co.jp");
        assert_eq!(
            cfg.redirect_url,
            "https://hr-hw.onrender.com/auth/google/callback"
        );
        // Debug に secret が出ない
        assert!(!format!("{cfg:?}").contains("secret\""));
        assert!(format!("{cfg:?}").contains("***"));

        // 1 つでも欠けたら無効
        assert!(GoogleOidcConfig::from_values(None, s("x"), s("x"), s("x")).is_none());
        assert!(GoogleOidcConfig::from_values(s("x"), None, s("x"), s("x")).is_none());
        assert!(GoogleOidcConfig::from_values(s("x"), s("x"), None, s("x")).is_none());
        assert!(GoogleOidcConfig::from_values(s("x"), s("x"), s("x"), None).is_none());
        // 空白だけも未設定扱い
        assert!(GoogleOidcConfig::from_values(s("x"), s("  "), s("x"), s("x")).is_none());
    }
}
