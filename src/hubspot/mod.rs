//! HubSpot CRM API の読み取りアダプタ (Headless CRM PR2)。
//!
//! - `client`: async reqwest クライアント (Bearer、タイムアウト、429/5xx の限定 retry、
//!   `X-HubSpot-RateLimit-*` の記録、Search 用の流量制限)
//! - `deep_link`: HubSpot 画面へのリンク (`record/0-1|0-2|0-3/{id}/`) と portal ID
//! - `types`: レコード・エラーの型 (`crm` モジュールとの契約)
//!
//! 書き込み API はこのモジュールに置かない (PR4 以降)。

pub mod client;
pub mod deep_link;
pub mod types;

pub use client::{ClientOptions, HubSpotClient, OwnerRef, DEFAULT_BASE_URL};
pub use types::{
    AssociationLabelDef, AssociationRef, EngagementType, HubSpotError, HubSpotRecord,
    RateLimitSnapshot, RecordType, TokenInfo,
};
