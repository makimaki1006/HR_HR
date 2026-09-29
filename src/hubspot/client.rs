//! HubSpot CRM API の async クライアント (読み取りのみ)。
//! 契約用のスタブ。実装は担当 A。

use std::time::Duration;

use super::types::{AssociationRef, HubSpotError, HubSpotRecord, RateLimitSnapshot};

pub const DEFAULT_BASE_URL: &str = "https://api.hubapi.com";

/// retry・タイムアウトの設定 (テストでは待ち時間を短くする)。
#[derive(Debug, Clone)]
pub struct ClientOptions {
    pub timeout: Duration,
    /// 初回を除く retry 回数 (429 / 5xx / タイムアウトのみ)
    pub max_retries: u32,
    /// retry の待ち時間の基準 (指数的に増やす。429 の Retry-After があればそちらを優先し上限で切る)
    pub retry_base_delay: Duration,
    /// Search API の間隔 (5 req/s = 200ms)
    pub search_min_interval: Duration,
}

impl Default for ClientOptions {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(10),
            max_retries: 2,
            retry_base_delay: Duration::from_millis(500),
            search_min_interval: Duration::from_millis(200),
        }
    }
}

/// Debug にトークンを出さない。
pub struct HubSpotClient {
    _private: (),
}

impl std::fmt::Debug for HubSpotClient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HubSpotClient").finish_non_exhaustive()
    }
}

impl HubSpotClient {
    /// `access_token` が空白だけなら `Err(NotConfigured)`。
    pub fn new(
        _access_token: String,
        _base_url: &str,
        _opts: ClientOptions,
    ) -> Result<Self, HubSpotError> {
        todo!("A")
    }

    /// `GET /crm/v3/objects/{object}/{id}?properties=...`
    /// `object` は `RecordType::api_name()` / `EngagementType::api_name()`。
    pub async fn get_object(
        &self,
        _object: &str,
        _id: &str,
        _properties: &[&str],
    ) -> Result<HubSpotRecord, HubSpotError> {
        todo!("A")
    }

    /// `POST /crm/v3/objects/{object}/batch/read` (読み取り。最大 100 件ずつに分割)。
    /// 見つからない ID は結果に含まれないだけでエラーにしない。
    pub async fn batch_read(
        &self,
        _object: &str,
        _ids: &[String],
        _properties: &[&str],
    ) -> Result<Vec<HubSpotRecord>, HubSpotError> {
        todo!("A")
    }

    /// `GET /crm/v4/objects/{from}/{id}/associations/{to}?limit=500`。
    /// 多対多をそのまま返す (1 件に潰さない)。戻り値の bool は次ページが残っていたか。
    pub async fn list_associations(
        &self,
        _from: &str,
        _id: &str,
        _to: &str,
    ) -> Result<(Vec<AssociationRef>, bool), HubSpotError> {
        todo!("A")
    }

    /// `POST /crm/v3/objects/{object}/search` (読み取り)。5 req/s 以下に絞る。
    pub async fn search(
        &self,
        _object: &str,
        _body: serde_json::Value,
    ) -> Result<serde_json::Value, HubSpotError> {
        todo!("A")
    }

    /// 最後に観測したレート制限ヘッダ
    pub fn last_rate_limit(&self) -> Option<RateLimitSnapshot> {
        todo!("A")
    }
}
