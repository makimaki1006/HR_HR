//! HubSpot アダプタと `/api/crm/*` の間の契約となる型。
//!
//! ナレッジ上の罠 (hubspot-zoom-slack スキル):
//! - Engagement (Call/Note/Task 等) は Deal と**多対多**。`to_list[0]` のように 1 件に潰さない
//! - 接触は Deal より Contact に付くことが多い。Deal の直近アクティビティは
//!   `deal → contacts → calls` も辿る (Deal 直だけだと取りこぼす)
//! - Call は 3 系統 (Zoom 純正連携 `hs_call_source=INTEGRATIONS_PLATFORM` / 手入力 / API)。
//!   系統を区別できるよう `hs_call_source` を必ず取る
//! - Deal のステージは ID (`dealstage`) で返り、表示名はパイプライン定義ごとの独自ラベル。
//!   ID を表示名とみなさない

use std::collections::BTreeMap;

use serde::Serialize;

/// Deep Link と `/api/crm/{kind}/{id}` の対象になる 3 種類のレコード。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RecordType {
    Contact,
    Company,
    Deal,
}

impl RecordType {
    pub const ALL: [RecordType; 3] = [RecordType::Contact, RecordType::Company, RecordType::Deal];

    /// HubSpot の objectTypeId (Deep Link の `record/{type_id}/` 部分)
    pub fn type_id(self) -> &'static str {
        match self {
            RecordType::Contact => "0-1",
            RecordType::Company => "0-2",
            RecordType::Deal => "0-3",
        }
    }

    /// CRM API のパス上の名前 (`/crm/v3/objects/{api_name}`)
    pub fn api_name(self) -> &'static str {
        match self {
            RecordType::Contact => "contacts",
            RecordType::Company => "companies",
            RecordType::Deal => "deals",
        }
    }

    /// JSON の `object_type` に出す単数形
    pub fn as_str(self) -> &'static str {
        match self {
            RecordType::Contact => "contact",
            RecordType::Company => "company",
            RecordType::Deal => "deal",
        }
    }
}

/// 直近アクティビティとして読む Engagement の種類。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum EngagementType {
    Call,
    Note,
    Task,
    Meeting,
    Email,
}

impl EngagementType {
    pub const ALL: [EngagementType; 5] = [
        EngagementType::Call,
        EngagementType::Note,
        EngagementType::Task,
        EngagementType::Meeting,
        EngagementType::Email,
    ];

    pub fn api_name(self) -> &'static str {
        match self {
            EngagementType::Call => "calls",
            EngagementType::Note => "notes",
            EngagementType::Task => "tasks",
            EngagementType::Meeting => "meetings",
            EngagementType::Email => "emails",
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            EngagementType::Call => "call",
            EngagementType::Note => "note",
            EngagementType::Task => "task",
            EngagementType::Meeting => "meeting",
            EngagementType::Email => "email",
        }
    }
}

/// `GET /crm/v3/objects/{type}/{id}` / batch read の 1 件。
/// properties の値は HubSpot の JSON のまま文字列か null (数値も文字列で返る)。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HubSpotRecord {
    pub id: String,
    pub properties: BTreeMap<String, Option<String>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub archived: bool,
}

/// v4 associations の 1 件 (`toObjectId` と関連ラベル)。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AssociationRef {
    pub id: String,
    /// `associationTypes[].label` のうち null でないもの (無ラベルなら空)
    pub labels: Vec<String>,
    /// v3 の本体 GET (`?associations=`) の `results[].type` (例 `deal_to_company` / `deal_to_company_unlabeled`)。
    /// 同じ id に複数の型が付けば全部 (重複なし、HubSpot の返した順)。v4 の関連では常に空。
    /// v3 はラベル名を返さないので、ラベルは呼び出し側が関連ラベルの定義から引く
    #[serde(skip)]
    pub type_names: Vec<String>,
}

/// 関連ラベルの定義 1 件 (`GET /crm/v4/associations/{from}/{to}/labels`)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssociationLabelDef {
    /// `HUBSPOT_DEFINED` / `USER_DEFINED` 等 (HubSpot の値のまま)
    pub category: String,
    pub type_id: u64,
    /// 無ラベル (既定の関連) は None
    pub label: Option<String>,
}

/// 最後に観測した `X-HubSpot-RateLimit-*` ヘッダ (Search の応答には付かない)。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct RateLimitSnapshot {
    pub max: Option<u64>,
    pub remaining: Option<u64>,
    pub daily_remaining: Option<u64>,
}

/// アクセストークン情報 (scope・ポータル ID だけ。鍵は持たない)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenInfo {
    pub portal_id: Option<String>,
    pub scopes: Vec<String>,
    /// 確認した時刻 (RFC 3339)
    pub checked_at: String,
}

/// HubSpot 呼び出しの失敗。**Display / Debug にトークンを含めない**
/// (HubSpot の応答本文もそのまま載せない。本文に入力値が反映される場合があるため)。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HubSpotError {
    /// `HUBSPOT_ACCESS_TOKEN` 未設定
    NotConfigured,
    /// 401 / 403。retry しない
    Auth { status: u16 },
    /// 404
    NotFound,
    /// 429 を retry し尽くした
    RateLimited,
    /// 5xx を retry し尽くした、または想定外の 4xx
    Upstream { status: u16 },
    /// タイムアウト (retry し尽くした)
    Timeout,
    /// 接続失敗など (メッセージは reqwest のエラー種別のみ。URL は含めない)
    Transport(String),
    /// 応答 JSON が想定の形でない
    Decode(String),
}

impl HubSpotError {
    /// API 応答の `error_kind`
    pub fn error_kind(&self) -> &'static str {
        match self {
            HubSpotError::NotConfigured => "not_configured",
            HubSpotError::Auth { .. } => "hubspot_auth",
            HubSpotError::NotFound => "not_found",
            HubSpotError::RateLimited => "hubspot_rate_limited",
            HubSpotError::Upstream { .. } => "hubspot_upstream",
            HubSpotError::Timeout => "hubspot_timeout",
            HubSpotError::Transport(_) => "hubspot_transport",
            HubSpotError::Decode(_) => "hubspot_decode",
        }
    }

    /// `/api/crm/*` が返す HTTP ステータス
    pub fn http_status(&self) -> u16 {
        match self {
            HubSpotError::NotConfigured => 503,
            HubSpotError::NotFound => 404,
            HubSpotError::RateLimited => 503,
            HubSpotError::Auth { .. }
            | HubSpotError::Upstream { .. }
            | HubSpotError::Timeout
            | HubSpotError::Transport(_)
            | HubSpotError::Decode(_) => 502,
        }
    }
}

impl std::fmt::Display for HubSpotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HubSpotError::NotConfigured => write!(f, "HubSpot API が未設定です"),
            HubSpotError::Auth { status } => write!(f, "HubSpot の認証に失敗しました ({status})"),
            HubSpotError::NotFound => write!(f, "HubSpot にレコードがありません"),
            HubSpotError::RateLimited => write!(f, "HubSpot のレート制限に達しました"),
            HubSpotError::Upstream { status } => {
                write!(f, "HubSpot がエラーを返しました ({status})")
            }
            HubSpotError::Timeout => write!(f, "HubSpot への接続がタイムアウトしました"),
            HubSpotError::Transport(m) => write!(f, "HubSpot に接続できませんでした: {m}"),
            HubSpotError::Decode(m) => write!(f, "HubSpot の応答を解釈できませんでした: {m}"),
        }
    }
}

impl std::error::Error for HubSpotError {}
