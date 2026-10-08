//! HubSpot CRM API の読み取りアダプタ (Headless CRM PR2)。
//!
//! - `client`: async reqwest クライアント (Bearer、タイムアウト、429/5xx の限定 retry、
//!   `X-HubSpot-RateLimit-*` の記録、Search 用の流量制限)
//! - `deep_link`: HubSpot 画面へのリンク (`record/0-1|0-2|0-3/{id}/`) と portal ID
//! - `gateway`: プロセス共有の関所 (流量制限・優先度・待ちの上限・429 の全員停止・観測)
//! - `types`: レコード・エラーの型 (`crm` モジュールとの契約)
//!
//! 書き込み API はこのモジュールに置かない (PR4 以降)。

pub mod client;
pub mod deep_link;
pub mod gateway;
pub mod types;

pub use client::{
    base_url_from_env, ClientOptions, HubSpotClient, OwnerRef, RawReply, DEFAULT_BASE_URL,
};
pub use gateway::{Gateway, GatewayConfig, Priority};
pub use types::{
    AssociationLabelDef, AssociationRef, EngagementType, HubSpotError, HubSpotRecord,
    RateLimitSnapshot, RecordType, TokenInfo,
};
