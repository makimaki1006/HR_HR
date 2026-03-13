use std::env;

/// 外部パスワード（有効期限付き）
#[derive(Debug, Clone)]
pub struct ExternalPassword {
    pub password: String,
    /// 有効期限（YYYY-MM-DD形式）。この日を含む最終日まで有効
    pub expires: String,
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
    /// キャッシュTTL（秒）
    pub cache_ttl_secs: u64,
    /// キャッシュ最大エントリ数
    pub cache_max_entries: usize,
    /// レート制限: 最大試行回数
    pub rate_limit_max_attempts: u32,
    /// レート制限: ロックアウト秒数
    pub rate_limit_lockout_secs: u64,
}

impl AppConfig {
    /// 環境変数から設定を読み込む
    pub fn from_env() -> Self {
        Self {
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
            allowed_domains: env::var("ALLOWED_DOMAINS")
                .unwrap_or_else(|_| "f-a-c.co.jp,cyxen.co.jp".to_string())
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .collect(),
            allowed_domains_extra: env::var("ALLOWED_DOMAINS_EXTRA")
                .unwrap_or_default()
                .split(',')
                .map(|s| s.trim().to_lowercase())
                .filter(|s| !s.is_empty())
                .collect(),
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
        }
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
            "PORT", "AUTH_PASSWORD", "AUTH_PASSWORD_HASH", "AUTH_PASSWORDS_EXTRA",
            "ALLOWED_DOMAINS", "ALLOWED_DOMAINS_EXTRA",
            "HELLOWORK_DB_PATH", "CACHE_TTL_SECS",
            "CACHE_MAX_ENTRIES", "RATE_LIMIT_MAX_ATTEMPTS", "RATE_LIMIT_LOCKOUT_SECONDS",
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
}
