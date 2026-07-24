//! 環境設定の読み込み(Phase 2b)。
//!
//! Google Ads / SerpApi の資格情報を環境変数から読む。std::env を最優先し、
//! 無ければリポジトリ直下 `.env`(クレートから見て `../.env`)を素朴にパースして
//! 補う。Python 版 `GoogleAdsRestConfig.from_env` / `serpapi_client.api_key` と一致。

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// 既定 API バージョン(Python `DEFAULT_API_VERSION` と一致)。
pub const DEFAULT_API_VERSION: &str = "v22";

/// 既定 `.env` の場所。
///
/// 2026-07-24 HR_HR 統合: 移植元は `../.env` 固定 (引き継ぎ資料の「要改修」項目)。
/// HR_HR では起動時に dotenvy がカレントの `.env` を std::env へ読み込むため
/// env 優先パスで通常は足り、ファイルフォールバックもカレント直下を見る。
fn default_dotenv_path() -> PathBuf {
    Path::new(".").join(".env")
}

/// `.env` を素朴にパースする(KEY=VALUE、`#` 行・空行・`=` 無し行は無視)。
/// 存在しなければ空マップ。値の前後空白は trim する(Python `load_dotenv` と同等)。
pub fn parse_dotenv(path: &Path) -> HashMap<String, String> {
    let mut map = HashMap::new();
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(_) => return map,
    };
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || !line.contains('=') {
            continue;
        }
        let (key, value) = match line.split_once('=') {
            Some(kv) => kv,
            None => continue,
        };
        map.insert(key.trim().to_string(), value.trim().to_string());
    }
    map
}

/// std::env を優先し、無ければ dotenv マップから引く。空文字列は未設定扱い。
fn env_or_dotenv(key: &str, dotenv: &HashMap<String, String>) -> String {
    if let Ok(v) = std::env::var(key) {
        if !v.is_empty() {
            return v;
        }
    }
    dotenv.get(key).cloned().unwrap_or_default()
}

/// Google Ads REST の資格情報(Python `GoogleAdsRestConfig` と一致)。
#[derive(Debug, Clone)]
pub struct GoogleAdsConfig {
    pub developer_token: String,
    pub client_id: String,
    pub client_secret: String,
    pub refresh_token: String,
    pub login_customer_id: String,
    /// env `GOOGLE_ADS_CUSTOMER_ID`(任意)。未設定なら空。
    /// 実照会に使う customer_id は [`GoogleAdsConfig::customer_id`] 経由で得る。
    pub customer_id_override: String,
    pub api_version: String,
}

impl GoogleAdsConfig {
    /// 環境変数(＋`../.env`)から読み込む。
    pub fn from_env() -> Self {
        let dotenv = parse_dotenv(&default_dotenv_path());
        Self::from_maps(&dotenv)
    }

    /// テスト用: 与えた dotenv マップ(＋std::env)から読み込む。
    pub fn from_maps(dotenv: &HashMap<String, String>) -> Self {
        let api_version = {
            let v = env_or_dotenv("GOOGLE_ADS_API_VERSION", dotenv);
            if v.is_empty() {
                DEFAULT_API_VERSION.to_string()
            } else {
                v
            }
        };
        GoogleAdsConfig {
            developer_token: env_or_dotenv("GOOGLE_ADS_DEVELOPER_TOKEN", dotenv),
            client_id: env_or_dotenv("GOOGLE_ADS_CLIENT_ID", dotenv),
            client_secret: env_or_dotenv("GOOGLE_ADS_CLIENT_SECRET", dotenv),
            refresh_token: env_or_dotenv("GOOGLE_ADS_REFRESH_TOKEN", dotenv),
            login_customer_id: env_or_dotenv("GOOGLE_ADS_LOGIN_CUSTOMER_ID", dotenv),
            customer_id_override: env_or_dotenv("GOOGLE_ADS_CUSTOMER_ID", dotenv),
            api_version,
        }
    }

    /// 実照会に使う customer_id。Python(`GOOGLE_ADS_CUSTOMER_ID or login_customer_id`、
    /// google_ads_keyword_volume.py:343 / case_conditions.py:530-534)と一致。
    /// env `GOOGLE_ADS_CUSTOMER_ID` があればそれ、無ければ login_customer_id。
    pub fn customer_id(&self) -> String {
        if self.customer_id_override.is_empty() {
            self.login_customer_id.clone()
        } else {
            self.customer_id_override.clone()
        }
    }

    /// 未設定の必須キー(Python `missing()` と一致)。
    pub fn missing(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.developer_token.is_empty() {
            out.push("GOOGLE_ADS_DEVELOPER_TOKEN");
        }
        if self.client_id.is_empty() {
            out.push("GOOGLE_ADS_CLIENT_ID");
        }
        if self.refresh_token.is_empty() {
            out.push("GOOGLE_ADS_REFRESH_TOKEN");
        }
        if self.login_customer_id.is_empty() {
            out.push("GOOGLE_ADS_LOGIN_CUSTOMER_ID");
        }
        out
    }
}

/// SerpApi のキー(std::env 優先、無ければ `../.env`)。
pub fn serpapi_key() -> String {
    let dotenv = parse_dotenv(&default_dotenv_path());
    env_or_dotenv("SERPAPI_API_KEY", &dotenv)
}

/// 既定 Gemini モデル。env `GEMINI_MODEL` で上書き可(Python llm_gateway と揃える)。
/// models.list 実在確認済(2026-07-22)。
pub const DEFAULT_GEMINI_MODEL: &str = "gemini-3.5-flash-lite";

/// Gemini API キー(std::env 優先、無ければ `../.env` の GEMINI_API_KEY)。
pub fn gemini_api_key() -> String {
    let dotenv = parse_dotenv(&default_dotenv_path());
    env_or_dotenv("GEMINI_API_KEY", &dotenv)
}

/// Gemini モデル(env `GEMINI_MODEL`、無ければ既定 [`DEFAULT_GEMINI_MODEL`])。
/// コード変更なしで env でモデルを切り替えられる。
pub fn gemini_model() -> String {
    let dotenv = parse_dotenv(&default_dotenv_path());
    let m = env_or_dotenv("GEMINI_MODEL", &dotenv);
    if m.is_empty() {
        DEFAULT_GEMINI_MODEL.to_string()
    } else {
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_dotenv_reads_pairs_and_skips_comments() {
        let dir = std::env::temp_dir().join(format!("jme_cfg_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".env");
        std::fs::write(
            &path,
            "# comment\nGOOGLE_ADS_CLIENT_ID = abc123 \n\nNO_EQUALS_LINE\nSERPAPI_API_KEY=zzz\n",
        )
        .unwrap();
        let map = parse_dotenv(&path);
        assert_eq!(map.get("GOOGLE_ADS_CLIENT_ID").unwrap(), "abc123");
        assert_eq!(map.get("SERPAPI_API_KEY").unwrap(), "zzz");
        assert!(!map.contains_key("NO_EQUALS_LINE"));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn from_maps_defaults_api_version() {
        let dotenv = HashMap::from([("GOOGLE_ADS_CLIENT_ID".to_string(), "cid".to_string())]);
        let cfg = GoogleAdsConfig::from_maps(&dotenv);
        assert_eq!(cfg.api_version, "v22");
        assert_eq!(cfg.client_id, "cid");
        // 未設定必須キーの検出
        assert!(cfg.missing().contains(&"GOOGLE_ADS_DEVELOPER_TOKEN"));
    }

    #[test]
    fn customer_id_falls_back_to_login() {
        // GOOGLE_ADS_CUSTOMER_ID 無し → login_customer_id を使う。
        let dotenv = HashMap::from([(
            "GOOGLE_ADS_LOGIN_CUSTOMER_ID".to_string(),
            "1112223333".to_string(),
        )]);
        let cfg = GoogleAdsConfig::from_maps(&dotenv);
        assert_eq!(cfg.customer_id(), "1112223333");
    }

    #[test]
    fn customer_id_prefers_explicit_override() {
        // GOOGLE_ADS_CUSTOMER_ID 指定時はそちらを優先。
        let dotenv = HashMap::from([
            ("GOOGLE_ADS_LOGIN_CUSTOMER_ID".to_string(), "1112223333".to_string()),
            ("GOOGLE_ADS_CUSTOMER_ID".to_string(), "9998887777".to_string()),
        ]);
        let cfg = GoogleAdsConfig::from_maps(&dotenv);
        assert_eq!(cfg.customer_id(), "9998887777");
    }
}
