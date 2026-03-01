use std::env;

/// アプリケーション設定
#[derive(Debug, Clone)]
pub struct AppConfig {
    /// サーバーポート
    pub port: u16,
    /// ログインパスワード（平文）
    pub auth_password: String,
    /// ログインパスワード（bcryptハッシュ）
    pub auth_password_hash: String,
    /// 許可ドメインリスト
    pub allowed_domains: Vec<String>,
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
            allowed_domains: env::var("ALLOWED_DOMAINS")
                .unwrap_or_else(|_| "f-a-c.co.jp,cyxen.co.jp".to_string())
                .split(',')
                .map(|s| s.trim().to_lowercase())
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
                .unwrap_or(2000),
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
            "PORT", "AUTH_PASSWORD", "AUTH_PASSWORD_HASH", "ALLOWED_DOMAINS",
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
