//! `GET /api/crm/{contacts|companies|deals}/{id}` (HubSpot レコードの読み取り)。
//!
//! 処理順 (順番に意味がある):
//! 1. 認可 (`rbac`)。**HubSpot の設定有無より先**。未認可の人に設定状況 (503) を見せない → 403
//! 2. id の形式 (ASCII 数字 1〜20 桁)。不正なら HubSpot を呼ばずに 400
//! 3. HubSpot クライアント未設定 → 503 `not_configured`
//! 4. レコード本体 + 関連 + 直近アクティビティを HubSpot から読む
//!
//! HubSpot 呼び出し回数の上限 (1 リクエストあたり、直列):
//! - 本体 1 + 関連 (自分以外の 2 型) 2 + Engagement 関連 5 + Engagement batch read 最大 5
//!   (1 型 100 件ごとに 1 回。関連は 1 型 500 件で打ち切り)
//! - Deal はさらに `deal → contacts → calls`: 関連 Contact 最大 [`MAX_DEAL_CONTACTS_FOR_CALLS`] 件 ×
//!   関連 1 回 + calls batch read (100 件ごと)
//!
//! 応答には `Cache-Control: no-store` を付ける (個人情報を中間キャッシュに残さない)。

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use axum::{
    extract::{Path, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde::Serialize;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::Session;

use super::rbac;
use crate::hubspot::deep_link::{hubspot_portal_id, record_url};
use crate::hubspot::{EngagementType, HubSpotClient, HubSpotError, HubSpotRecord, RecordType};
use crate::AppState;

/// Contact で取るプロパティ
pub const CONTACT_PROPERTIES: &[&str] = &[
    "firstname",
    "lastname",
    "email",
    "phone",
    "company",
    "hubspot_owner_id",
    "lifecyclestage",
];

/// Company で取るプロパティ
pub const COMPANY_PROPERTIES: &[&str] = &[
    "name",
    "domain",
    "phone",
    "city",
    "state",
    "hubspot_owner_id",
];

/// Deal で取るプロパティ。
/// `dealstage` / `pipeline` は **ID のまま返す**。表示名はパイプライン定義ごとの独自ラベルで、
/// 別途パイプライン API (`/crm/v3/pipelines/deals`) で引く必要がある。ここでは推測でラベルを付けない。
pub const DEAL_PROPERTIES: &[&str] = &[
    "dealname",
    "dealstage",
    "pipeline",
    "amount",
    "closedate",
    "hubspot_owner_id",
];

/// Call のプロパティ。`hs_call_source` で 3 系統 (Zoom 純正連携 `INTEGRATIONS_PLATFORM` /
/// 手入力 / API) を区別できる。
pub const CALL_PROPERTIES: &[&str] = &[
    "hs_timestamp",
    "hs_call_title",
    "hs_call_direction",
    "hs_call_status",
    "hs_call_duration",
    "hs_call_source",
    "hs_call_disposition",
    "hubspot_owner_id",
];
pub const NOTE_PROPERTIES: &[&str] = &["hs_timestamp", "hs_note_body", "hubspot_owner_id"];
pub const TASK_PROPERTIES: &[&str] = &[
    "hs_timestamp",
    "hs_task_subject",
    "hs_task_status",
    "hubspot_owner_id",
];
pub const MEETING_PROPERTIES: &[&str] = &["hs_timestamp", "hs_meeting_title"];
pub const EMAIL_PROPERTIES: &[&str] = &["hs_timestamp", "hs_email_subject"];

/// 直近アクティビティの最大件数
pub const MAX_RECENT_ACTIVITIES: usize = 10;

/// Deal の直近アクティビティで `deal → contacts → calls` を辿る Contact 数の上限。
/// 1 Contact ごとに HubSpot 呼び出しが 1 回増えるため (直列)。超えた分は辿らず
/// `meta.activities_truncated = true` にする。
pub const MAX_DEAL_CONTACTS_FOR_CALLS: usize = 20;

/// id の最大桁数 (HubSpot の ID は数字。u64 に収まる桁数)
const MAX_ID_DIGITS: usize = 20;

const DATA_SCOPE: &str = "HubSpot の読み取り結果。書き込みはしない";

pub fn record_properties(rt: RecordType) -> &'static [&'static str] {
    match rt {
        RecordType::Contact => CONTACT_PROPERTIES,
        RecordType::Company => COMPANY_PROPERTIES,
        RecordType::Deal => DEAL_PROPERTIES,
    }
}

pub fn engagement_properties(et: EngagementType) -> &'static [&'static str] {
    match et {
        EngagementType::Call => CALL_PROPERTIES,
        EngagementType::Note => NOTE_PROPERTIES,
        EngagementType::Task => TASK_PROPERTIES,
        EngagementType::Meeting => MEETING_PROPERTIES,
        EngagementType::Email => EMAIL_PROPERTIES,
    }
}

/// `/api/crm/*` のルート。`protected_routes` に merge する (認証はそちらの route_layer)。
pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/crm/contacts/{id}", get(get_contact))
        .route("/api/crm/companies/{id}", get(get_company))
        .route("/api/crm/deals/{id}", get(get_deal))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

async fn get_contact(
    session: Session,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Contact, session, state, id).await
}

async fn get_company(
    session: Session,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Company, session, state, id).await
}

async fn get_deal(
    session: Session,
    State(state): State<Arc<AppState>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Deal, session, state, id).await
}

// ---------------------------------------------------------------------------
// 応答の型 (JSON の形はここで決まる)。React 画面から ts-rs で型生成する予定のため、
// serde_json::json! ではなく struct で組み立てる (ts-rs の derive はリーダーが後で付ける)。
// ---------------------------------------------------------------------------

/// エラー応答。`message` は HubSpot 由来のエラーのときだけ付く。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmErrorResponse {
    /// `forbidden` / `invalid_id` / `not_configured` / `HubSpotError::error_kind()` の値
    pub error_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

/// 関連レコード 1 件
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmAssociation {
    pub id: String,
    /// 関連ラベル (無ラベルなら空)
    pub labels: Vec<String>,
    pub deep_link: String,
}

/// 型ごとに「関連が 500 件を超えて打ち切ったか」。自分自身の型のキーは出さない。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CrmAssociationsTruncated {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contacts: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companies: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deals: Option<bool>,
}

/// 関連レコード。自分自身の型のキーは出さない (contact なら `contacts` が無い)。
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct CrmAssociations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contacts: Option<Vec<CrmAssociation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub companies: Option<Vec<CrmAssociation>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deals: Option<Vec<CrmAssociation>>,
    pub truncated: CrmAssociationsTruncated,
}

impl CrmAssociations {
    fn set(&mut self, rt: RecordType, items: Vec<CrmAssociation>, truncated: bool) {
        match rt {
            RecordType::Contact => {
                self.contacts = Some(items);
                self.truncated.contacts = Some(truncated);
            }
            RecordType::Company => {
                self.companies = Some(items);
                self.truncated.companies = Some(truncated);
            }
            RecordType::Deal => {
                self.deals = Some(items);
                self.truncated.deals = Some(truncated);
            }
        }
    }
}

/// アクティビティをどのレコード経由で見つけたか (直付きなら対象レコード自身)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmActivityVia {
    /// `contact` / `company` / `deal`
    pub object_type: RecordType,
    pub id: String,
}

/// 直近アクティビティ 1 件
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmActivity {
    /// `call` / `note` / `task` / `meeting` / `email`
    #[serde(rename = "type")]
    pub activity_type: EngagementType,
    pub id: String,
    /// `hs_timestamp` の値 (HubSpot の文字列のまま)
    pub timestamp: Option<String>,
    pub properties: BTreeMap<String, Option<String>>,
    pub via: CrmActivityVia,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmMeta {
    pub hubspot_portal_id: String,
    pub data_scope: String,
    /// 関連 500 件超 / 辿る Contact 数の上限超えで、直近アクティビティが完全でない可能性がある
    pub activities_truncated: bool,
}

/// 成功応答 (200)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmRecordResponse {
    /// `contact` / `company` / `deal`
    pub object_type: RecordType,
    pub id: String,
    /// 値は HubSpot の文字列のまま (数値も文字列) か null
    pub properties: BTreeMap<String, Option<String>>,
    pub created_at: Option<String>,
    pub updated_at: Option<String>,
    pub deep_link: String,
    pub associations: CrmAssociations,
    /// `hs_timestamp` 降順、最大 [`MAX_RECENT_ACTIVITIES`] 件
    pub recent_activities: Vec<CrmActivity>,
    pub meta: CrmMeta,
}

fn error_json(status: StatusCode, kind: &str) -> Response {
    (
        status,
        Json(CrmErrorResponse {
            error_kind: kind.to_string(),
            message: None,
        }),
    )
        .into_response()
}

/// HubSpot の ID として受け付ける形か (ASCII 数字 1〜20 桁)
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_DIGITS && id.bytes().all(|b| b.is_ascii_digit())
}

async fn handle(rt: RecordType, session: Session, state: Arc<AppState>, id: String) -> Response {
    // 1) 認可 (設定有無より先)
    let principal = rbac::load_principal(&session, &state).await;
    if !rbac::can_read(&principal, rt, rbac::READ_ALLOWED_ROLES) {
        return error_json(StatusCode::FORBIDDEN, "forbidden");
    }
    // 2) id
    if !is_valid_id(&id) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_id");
    }
    // 3) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 4) 読み取り
    let portal = hubspot_portal_id();
    match build_record_view(&client, rt, &id, &portal).await {
        Ok(v) => Json(v).into_response(),
        Err(e) => {
            tracing::warn!(
                error_kind = e.error_kind(),
                object_type = rt.as_str(),
                id = %id,
                "crm read failed"
            );
            let status = StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_GATEWAY);
            (
                status,
                Json(CrmErrorResponse {
                    error_kind: e.error_kind().to_string(),
                    message: Some(e.to_string()),
                }),
            )
                .into_response()
        }
    }
}

/// 直近アクティビティの候補 1 件
struct Activity {
    et: EngagementType,
    record: HubSpotRecord,
    via_type: RecordType,
    via_id: String,
}

/// `hs_timestamp` を並べ替え用の epoch ミリ秒にする。
/// HubSpot v3 は ISO 8601 (`2026-09-01T10:00:00Z` / `...00.123Z`) で返すが、
/// 数字 (epoch ms) の場合も受ける。解釈できなければ None (末尾に並ぶ)。
fn timestamp_millis(v: Option<&str>) -> Option<i64> {
    let s = v?.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    s.parse::<i64>().ok()
}

fn hs_timestamp(r: &HubSpotRecord) -> Option<&str> {
    r.properties.get("hs_timestamp").and_then(|v| v.as_deref())
}

/// HubSpot から読んで応答を組み立てる。
pub async fn build_record_view(
    client: &HubSpotClient,
    rt: RecordType,
    id: &str,
    portal: &str,
) -> Result<CrmRecordResponse, HubSpotError> {
    let record = client
        .get_object(rt.api_name(), id, record_properties(rt))
        .await?;

    // --- 関連レコード (自分と同じ型は除く) ---
    let mut associations = CrmAssociations::default();
    let mut deal_contact_ids: Vec<String> = Vec::new();
    for other in RecordType::ALL {
        if other == rt {
            continue;
        }
        let (refs, more) = client
            .list_associations(rt.api_name(), id, other.api_name())
            .await?;
        if rt == RecordType::Deal && other == RecordType::Contact {
            deal_contact_ids = refs.iter().map(|r| r.id.clone()).collect();
        }
        let items = refs
            .into_iter()
            .map(|r| CrmAssociation {
                deep_link: record_url(portal, other, &r.id),
                id: r.id,
                labels: r.labels,
            })
            .collect();
        associations.set(other, items, more);
    }

    // --- 直近アクティビティ ---
    // Engagement は多対多なので同じ id が複数経路で現れる。先に見つけた方 (= 直付き) を残す。
    let mut seen: HashSet<(EngagementType, String)> = HashSet::new();
    let mut activities: Vec<Activity> = Vec::new();
    let mut activities_truncated = false;

    for et in EngagementType::ALL {
        let (refs, more) = client
            .list_associations(rt.api_name(), id, et.api_name())
            .await?;
        activities_truncated |= more;
        let ids: Vec<String> = refs
            .into_iter()
            .map(|r| r.id)
            .filter(|eid| seen.insert((et, eid.clone())))
            .collect();
        if ids.is_empty() {
            continue;
        }
        for rec in client
            .batch_read(et.api_name(), &ids, engagement_properties(et))
            .await?
        {
            activities.push(Activity {
                et,
                record: rec,
                via_type: rt,
                via_id: id.to_string(),
            });
        }
    }

    // Deal: 接触 (Call) は Deal より Contact に付くことが多い。deal → contacts → calls も辿る。
    if rt == RecordType::Deal {
        if deal_contact_ids.len() > MAX_DEAL_CONTACTS_FOR_CALLS {
            activities_truncated = true;
        }
        let mut call_ids: Vec<String> = Vec::new();
        let mut call_via: BTreeMap<String, String> = BTreeMap::new();
        for cid in deal_contact_ids.iter().take(MAX_DEAL_CONTACTS_FOR_CALLS) {
            let (refs, more) = client
                .list_associations(
                    RecordType::Contact.api_name(),
                    cid,
                    EngagementType::Call.api_name(),
                )
                .await?;
            activities_truncated |= more;
            for r in refs {
                if seen.insert((EngagementType::Call, r.id.clone())) {
                    call_via.insert(r.id.clone(), cid.clone());
                    call_ids.push(r.id);
                }
            }
        }
        if !call_ids.is_empty() {
            for rec in client
                .batch_read(
                    EngagementType::Call.api_name(),
                    &call_ids,
                    engagement_properties(EngagementType::Call),
                )
                .await?
            {
                let via_id = call_via.get(&rec.id).cloned().unwrap_or_default();
                activities.push(Activity {
                    et: EngagementType::Call,
                    record: rec,
                    via_type: RecordType::Contact,
                    via_id,
                });
            }
        }
    }

    // hs_timestamp 降順 (無いものは末尾)。同時刻は id で安定化。
    activities.sort_by(|a, b| {
        let ta = timestamp_millis(hs_timestamp(&a.record));
        let tb = timestamp_millis(hs_timestamp(&b.record));
        tb.cmp(&ta).then_with(|| a.record.id.cmp(&b.record.id))
    });
    let recent_activities: Vec<CrmActivity> = activities
        .into_iter()
        .take(MAX_RECENT_ACTIVITIES)
        .map(|a| CrmActivity {
            activity_type: a.et,
            timestamp: hs_timestamp(&a.record).map(str::to_string),
            id: a.record.id,
            properties: a.record.properties,
            via: CrmActivityVia {
                object_type: a.via_type,
                id: a.via_id,
            },
        })
        .collect();

    Ok(CrmRecordResponse {
        object_type: rt,
        deep_link: record_url(portal, rt, &record.id),
        id: record.id,
        properties: record.properties,
        created_at: record.created_at,
        updated_at: record.updated_at,
        associations,
        recent_activities,
        meta: CrmMeta {
            hubspot_portal_id: portal.to_string(),
            data_scope: DATA_SCOPE.to_string(),
            activities_truncated,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn id_の形式() {
        for ok in ["1", "123", "12345678901234567890"] {
            assert!(is_valid_id(ok), "{ok}");
        }
        for ng in [
            "",
            "abc",
            "12a",
            "123456789012345678901", // 21 桁
            "-1",
            " 12",
            "１２", // 全角数字
            "1.0",
        ] {
            assert!(!is_valid_id(ng), "{ng:?}");
        }
    }

    #[test]
    fn hs_timestamp_の解釈() {
        assert_eq!(
            timestamp_millis(Some("2026-09-01T10:00:00Z")),
            Some(1_788_256_800_000)
        );
        assert_eq!(
            timestamp_millis(Some("2026-09-01T10:00:00.500Z")),
            Some(1_788_256_800_500)
        );
        assert_eq!(
            timestamp_millis(Some("1788256800000")),
            Some(1_788_256_800_000)
        );
        assert_eq!(timestamp_millis(Some("not a date")), None);
        assert_eq!(timestamp_millis(None), None);
    }

    /// struct の JSON の形が仕様どおりか (自分自身の型のキーが無い、type / via / null の出方)
    #[test]
    fn 応答_struct_の_json_の形() {
        let mut assoc = CrmAssociations::default();
        assoc.set(
            RecordType::Deal,
            vec![CrmAssociation {
                id: "900".into(),
                labels: vec![],
                deep_link: "https://app.hubspot.com/contacts/1/record/0-3/900/".into(),
            }],
            false,
        );
        assoc.set(RecordType::Company, vec![], true);
        let mut props = BTreeMap::new();
        props.insert("email".to_string(), Some("a@example.com".to_string()));
        props.insert("phone".to_string(), None);
        let resp = CrmRecordResponse {
            object_type: RecordType::Contact,
            id: "55".into(),
            properties: props.clone(),
            created_at: None,
            updated_at: Some("2026-09-01T00:00:00Z".into()),
            deep_link: "https://app.hubspot.com/contacts/1/record/0-1/55/".into(),
            associations: assoc,
            recent_activities: vec![CrmActivity {
                activity_type: EngagementType::Call,
                id: "1001".into(),
                timestamp: Some("2026-09-10T01:00:00Z".into()),
                properties: props,
                via: CrmActivityVia {
                    object_type: RecordType::Contact,
                    id: "55".into(),
                },
            }],
            meta: CrmMeta {
                hubspot_portal_id: "1".into(),
                data_scope: DATA_SCOPE.into(),
                activities_truncated: false,
            },
        };
        let v = serde_json::to_value(&resp).unwrap();
        assert_eq!(
            v,
            serde_json::json!({
                "object_type": "contact",
                "id": "55",
                "properties": {"email": "a@example.com", "phone": null},
                "created_at": null,
                "updated_at": "2026-09-01T00:00:00Z",
                "deep_link": "https://app.hubspot.com/contacts/1/record/0-1/55/",
                "associations": {
                    "companies": [],
                    "deals": [{"id": "900", "labels": [],
                               "deep_link": "https://app.hubspot.com/contacts/1/record/0-3/900/"}],
                    "truncated": {"companies": true, "deals": false}
                },
                "recent_activities": [{
                    "type": "call", "id": "1001", "timestamp": "2026-09-10T01:00:00Z",
                    "properties": {"email": "a@example.com", "phone": null},
                    "via": {"object_type": "contact", "id": "55"}
                }],
                "meta": {"hubspot_portal_id": "1",
                         "data_scope": "HubSpot の読み取り結果。書き込みはしない",
                         "activities_truncated": false}
            })
        );
        // エラー: message が無いときはキーごと出さない
        let e = CrmErrorResponse {
            error_kind: "forbidden".into(),
            message: None,
        };
        assert_eq!(
            serde_json::to_value(&e).unwrap(),
            serde_json::json!({"error_kind": "forbidden"})
        );
    }
}
