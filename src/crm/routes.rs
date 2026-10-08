//! `GET /api/crm/{contacts|companies|deals}/{id}` (HubSpot レコードの読み取り)。
//!
//! 処理順 (順番に意味がある):
//! 1. 認可 (`rbac`)。**HubSpot の設定有無より先**。未ログインは JSON 401、未認可は 403
//!    (未認可の人に設定状況 (503) を見せない)
//! 2. id の形式 (ASCII 数字 1〜20 桁)。不正なら HubSpot を呼ばずに 400
//! 3. HubSpot クライアント未設定 → 503 `not_configured`
//! 4. レコード本体 + 関連 + 直近アクティビティを HubSpot から読む
//!
//! HubSpot 呼び出し回数 (1 リクエストあたり。鍵を既存の営業自動化バッチと共有しており、
//! 100 req/10 秒・Search 5 req/秒の枠をアカウントで共有するため、1 画面で食い尽くさない):
//! 1. `get_object_with_associations` 1 回 = 本体 + 関連 ID
//!    (自分以外の contacts/companies/deals + calls/notes/tasks/meetings)。
//!    **emails は v1 では取らない** (共有鍵に email 読み取りスコープがあるか未確認で、
//!    無いとレコード全体が 403 になりうるため)
//! 2. Deal のみ: 関連 Contact (先頭 [`MAX_DEAL_CONTACTS_FOR_CALLS`] 件) の calls を
//!    `batch_associations` 1 回 (`deal → contacts → calls`)。
//!    注意: 同じ Contact の**別 Deal** の通話も混ざりうる (絞り込みは PR4 以降)
//! 3. Engagement の型ごと (4 型) に ID を集め (直付き優先で重複除去)、型ごとに
//!    [`MAX_ENGAGEMENTS_PER_TYPE`] 件まで `batch_read` 1 回
//!
//! 最大 1 + 1 + 4 = 6 回。さらに安全装置として [`MAX_HUBSPOT_CALLS_PER_REQUEST`] を超える呼び出しはしない
//! (超えた分は `meta.partial` に `call_budget` として出す)。client 内部の retry は数えない
//! (429 / 5xx 時のみ。回数は `max_retries` で別に抑えている)。
//! ハンドラ全体は [`CRM_REQUEST_DEADLINE`] で打ち切り、504 `crm_timeout` を返す。
//!
//! 取得の一部 (Engagement の batch_read、contact 経由の calls) が失敗しても本体は 200 で返し、
//! 失敗した部分を `meta.partial` に出す。本体レコード (手順 1) の失敗だけがエラー応答になる。
//!
//! 応答には `Cache-Control: no-store` を付ける (個人情報を中間キャッシュに残さない)。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

use axum::{
    extract::{Path, Query, State},
    http::{header, HeaderValue, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use serde::{Deserialize, Serialize};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_sessions::Session;

use super::rbac::{self, CrmAccess};
use crate::handlers::crm_metadata::MetadataCache;
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
/// Task の `hs_timestamp` は期日 (未来になりうる)。並べ替えには作成日時 `hs_createdate` を使う。
pub const TASK_PROPERTIES: &[&str] = &[
    "hs_timestamp",
    "hs_createdate",
    "hs_task_subject",
    "hs_task_status",
    "hubspot_owner_id",
];
pub const MEETING_PROPERTIES: &[&str] = &["hs_timestamp", "hs_meeting_title"];
pub const EMAIL_PROPERTIES: &[&str] = &["hs_timestamp", "hs_email_subject"];

/// 直近アクティビティの最大件数
pub const MAX_RECENT_ACTIVITIES: usize = 10;

/// Deal の直近アクティビティで `deal → contacts → calls` を辿る Contact 数の上限
/// (1 回の `batch_associations` に渡す件数)。超えた分は辿らず `meta.activities_truncated = true`。
pub const MAX_DEAL_CONTACTS_FOR_CALLS: usize = 20;

/// Engagement 1 型あたりに `batch_read` で読む ID の上限。超えた分は読まず
/// `meta.activities_truncated = true`。HubSpot の返す順は時刻順とは限らないため、
/// 上限を超えると最新 10 件を取りこぼす可能性がある (truncated はそれを示す)。
pub const MAX_ENGAGEMENTS_PER_TYPE: usize = 100;

/// 1 リクエストで HubSpot を呼ぶ回数の上限 (安全装置)。通常は最大 6 回。
/// client 内部の retry は数えない。
pub const MAX_HUBSPOT_CALLS_PER_REQUEST: usize = 10;

/// ハンドラ全体の締め切り。超えたら 504 `crm_timeout`。
pub const CRM_REQUEST_DEADLINE: Duration = Duration::from_secs(20);

/// 直近アクティビティとして取る Engagement (email は共有鍵のスコープ未確認のため v1 では取らない)
const READ_ENGAGEMENTS: [EngagementType; 4] = [
    EngagementType::Call,
    EngagementType::Note,
    EngagementType::Task,
    EngagementType::Meeting,
];

/// HubSpot 呼び出し回数のカウンタ。呼ぶ前に `take` し、false なら呼ばない。
struct CallBudget {
    used: usize,
    max: usize,
}

impl CallBudget {
    fn new(max: usize) -> Self {
        Self { used: 0, max }
    }
    fn take(&mut self) -> bool {
        if self.used >= self.max {
            false
        } else {
            self.used += 1;
            true
        }
    }
}

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

/// ルートが共有するもの (許可メールと定義のキャッシュ)。
pub(super) struct CrmCtx {
    pub(super) access: CrmAccess,
    metadata_cache: MetadataCache,
    /// 架電キュー (`call_queue`) の状態 (owner / ステージ名のキャッシュ、cursor の署名鍵)
    pub(super) queue: super::call_queue::CallQueueState,
    /// レコード読み取りの同時実行数の上限。HubSpot の鍵は既存の営業自動化バッチと共有で
    /// (100 req/10 秒をアカウントで共有)、1 回の読み取りが最大 6 呼び出しになるため、
    /// 連打・多タブで枠を食い尽くさないよう絞る。待ちも締め切りに含める。
    pub(super) read_slots: tokio::sync::Semaphore,
}

/// レコード読み取りの同時実行数
pub const MAX_CONCURRENT_RECORD_READS: usize = 4;

/// `?refresh=true` で定義を取り直せる最短間隔 (これより新しいキャッシュは返す)
pub const METADATA_REFRESH_FLOOR: Duration = Duration::from_secs(5);

/// `/api/crm/*` のルート (metadata + レコード 3 種)。
///
/// **`protected_routes` (auth_middleware) の外に merge する**。未ログインを /login への 303 ではなく
/// JSON の 401 で返すため。認可は各ハンドラの先頭で `rbac::authorize` が行う。GET のみ。
pub fn router(access: CrmAccess) -> Router<Arc<AppState>> {
    router_with_queue(access, super::call_queue::CallQueueState::new())
}

/// [`router`] の架電キュー状態を差し替えられる版 (テストで時刻・署名鍵を固定する)。
pub(super) fn router_with_queue(
    access: CrmAccess,
    queue: super::call_queue::CallQueueState,
) -> Router<Arc<AppState>> {
    let ctx = Arc::new(CrmCtx {
        access,
        metadata_cache: MetadataCache::with_refresh_floor(METADATA_REFRESH_FLOOR),
        queue,
        read_slots: tokio::sync::Semaphore::new(MAX_CONCURRENT_RECORD_READS),
    });
    Router::new()
        .route("/api/crm/metadata", get(get_metadata))
        .route(
            "/api/crm/call-queue",
            get(super::call_queue::get_call_queue),
        )
        .route(
            "/api/crm/call-queue/pipelines",
            get(super::call_queue::get_call_queue_pipelines),
        )
        .route("/api/crm/owners", get(super::owners::get_owners))
        .route(
            "/api/crm/workspace/deals/{id}",
            get(super::workspace::get_workspace_deal),
        )
        .route("/api/crm/contacts/{id}", get(get_contact))
        .route("/api/crm/companies/{id}", get(get_company))
        .route("/api/crm/deals/{id}", get(get_deal))
        .layer(Extension(ctx))
        .layer(SetResponseHeaderLayer::overriding(
            header::CACHE_CONTROL,
            HeaderValue::from_static("no-store"),
        ))
}

async fn get_contact(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Contact, session, state, &ctx, id).await
}

async fn get_company(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Company, session, state, &ctx, id).await
}

async fn get_deal(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    handle(RecordType::Deal, session, state, &ctx, id).await
}

#[derive(Deserialize, Default)]
struct MetadataQuery {
    #[serde(default)]
    refresh: bool,
}

/// `GET /api/crm/metadata[?refresh=true]`。顧客の値は返さない。
async fn get_metadata(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Query(query): Query<MetadataQuery>,
) -> Response {
    // 1) 認可 (設定有無より先)
    if let Err(denied) = rbac::authorize(&session, &state, &ctx.access, None).await {
        return denied.into_response();
    }
    // 2) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 3) 取得 (全体に締め切りを付ける。ロックを持ったまま待たないよう timeout は外側)
    match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        ctx.metadata_cache.get(&client, query.refresh),
    )
    .await
    {
        Err(_elapsed) => {
            tracing::warn!(error_kind = "crm_timeout", "crm metadata timed out");
            timeout_response()
        }
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => {
            tracing::warn!(error_kind = e.error_kind(), "crm metadata read failed");
            hubspot_error_response(&e)
        }
    }
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

/// 型ごとに「関連の一覧が HubSpot 側で打ち切られたか」(v3 の応答に paging.next があった。上限件数は公式に記載なし)。自分自身の型のキーは出さない。
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
    /// 並べ替えに使った時刻 (HubSpot の文字列のまま)。Task は作成日時 `hs_createdate`、それ以外は `hs_timestamp`。
    /// Task の期日 `hs_timestamp` は `properties` に残る
    pub timestamp: Option<String>,
    pub properties: BTreeMap<String, Option<String>>,
    pub via: CrmActivityVia,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmMeta {
    pub hubspot_portal_id: String,
    pub data_scope: String,
    /// 関連の打ち切り / 辿る Contact 数・型ごとの件数上限超えで、直近アクティビティが完全でない可能性がある
    pub activities_truncated: bool,
    /// 取得できなかった部分 (無ければ空配列)
    pub partial: Vec<CrmPartial>,
}

/// 取得できなかった部分 1 件
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CrmPartial {
    /// `calls` / `notes` / `tasks` / `meetings` / `calls_via_contacts`
    pub part: String,
    /// `HubSpotError::error_kind()` の値、または `call_budget` (呼び出し回数の上限で取得しなかった)
    pub error_kind: String,
}

impl CrmPartial {
    fn new(part: &str, error_kind: &str) -> Self {
        Self {
            part: part.to_string(),
            error_kind: error_kind.to_string(),
        }
    }
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
    /// 上記 timestamp の降順、最大 [`MAX_RECENT_ACTIVITIES`] 件
    pub recent_activities: Vec<CrmActivity>,
    pub meta: CrmMeta,
}

pub(super) fn error_json(status: StatusCode, kind: &str) -> Response {
    (
        status,
        Json(CrmErrorResponse {
            error_kind: kind.to_string(),
            message: None,
        }),
    )
        .into_response()
}

pub(super) fn timeout_response() -> Response {
    (
        StatusCode::GATEWAY_TIMEOUT,
        Json(CrmErrorResponse {
            error_kind: "crm_timeout".to_string(),
            message: Some("HubSpot からの取得に時間がかかりすぎたため中断しました".to_string()),
        }),
    )
        .into_response()
}

/// HubSpot の失敗を応答にする。`message` は `HubSpotError` の固定文言 (上流の応答本文は含まない)。
pub(super) fn hubspot_error_response(e: &HubSpotError) -> Response {
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

/// HubSpot の ID として受け付ける形か (ASCII 数字 1〜20 桁)
pub fn is_valid_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= MAX_ID_DIGITS && id.bytes().all(|b| b.is_ascii_digit())
}

async fn handle(
    rt: RecordType,
    session: Session,
    state: Arc<AppState>,
    ctx: &CrmCtx,
    id: String,
) -> Response {
    // 1) 認可 (設定有無より先。役割もここで決まる)
    let principal = match rbac::authorize(&session, &state, &ctx.access, Some(rt)).await {
        Ok(p) => p,
        Err(denied) => return denied.into_response(),
    };
    let role = rbac::resolve_role(&principal);
    // 2) id
    if !is_valid_id(&id) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_id");
    }
    // 3) HubSpot 設定
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    // 4) 読み取り。同時実行の枠待ちも含めて全体に締め切りを付ける
    let started = std::time::Instant::now();
    let _slot = match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.read_slots.acquire()).await {
        Ok(Ok(permit)) => permit,
        _ => {
            tracing::warn!(
                error_kind = "crm_timeout",
                "crm read waited too long for a slot"
            );
            return timeout_response();
        }
    };
    // 4b) BPO は「自分が担当で架電キューの条件に合う Deal」と、それに紐づく Contact / Company だけ。
    //     本文を読む前に確かめる (外れたら本文は一切返さない)。admin / consultant は通らない
    if !role.reads_all_records() {
        let email = principal.email.as_deref().unwrap_or_default();
        let remaining = CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed());
        match tokio::time::timeout(
            remaining,
            super::record_gate::bpo_may_read(&client, ctx, rt, &id, email),
        )
        .await
        {
            Err(_elapsed) => {
                tracing::warn!(error_kind = "crm_timeout", "crm record gate timed out");
                return timeout_response();
            }
            Ok(Err(resp)) => return resp,
            Ok(Ok(())) => {}
        }
    }
    let portal = hubspot_portal_id();
    read_response(
        &client,
        rt,
        &id,
        &portal,
        MAX_HUBSPOT_CALLS_PER_REQUEST,
        CRM_REQUEST_DEADLINE.saturating_sub(started.elapsed()),
    )
    .await
}

/// HubSpot から読んで応答にする。`max_calls` / `deadline` はテストで差し替えるため引数にしている。
pub async fn read_response(
    client: &HubSpotClient,
    rt: RecordType,
    id: &str,
    portal: &str,
    max_calls: usize,
    deadline: Duration,
) -> Response {
    let result = tokio::time::timeout(
        deadline,
        build_record_view_with_budget(client, rt, id, portal, max_calls),
    )
    .await;
    match result {
        Err(_elapsed) => {
            tracing::warn!(
                error_kind = "crm_timeout",
                object_type = rt.as_str(),
                id = %id,
                "crm read timed out"
            );
            timeout_response()
        }
        Ok(Ok(v)) => Json(v).into_response(),
        Ok(Err(e)) => {
            tracing::warn!(
                error_kind = e.error_kind(),
                object_type = rt.as_str(),
                id = %id,
                "crm read failed"
            );
            hubspot_error_response(&e)
        }
    }
}

/// 直近アクティビティの候補 1 件
struct Activity {
    et: EngagementType,
    record: HubSpotRecord,
    via_type: RecordType,
    via_id: String,
    /// 並べ替えに使った時刻 (Task は作成日時 `hs_createdate`、それ以外は `hs_timestamp`)
    sort_ts: Option<String>,
}

/// 時刻文字列を並べ替え用の epoch ミリ秒にする。
/// HubSpot v3 は ISO 8601 (`2026-09-01T10:00:00Z` / `...00.123Z`) で返すが、
/// 数字 (epoch ms) の場合も受ける。解釈できなければ None (末尾に並ぶ)。
pub(super) fn timestamp_millis(v: Option<&str>) -> Option<i64> {
    let s = v?.trim();
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(s) {
        return Some(dt.timestamp_millis());
    }
    s.parse::<i64>().ok()
}

/// 並べ替えに使う時刻。Task の `hs_timestamp` は期日 (未来になりうる) なので作成日時を使う。
fn sort_timestamp(et: EngagementType, r: &HubSpotRecord) -> Option<String> {
    let prop = |k: &str| r.properties.get(k).and_then(|v| v.clone());
    match et {
        EngagementType::Task => prop("hs_createdate").or_else(|| r.created_at.clone()),
        _ => prop("hs_timestamp"),
    }
}

/// HubSpot から読んで応答を組み立てる (呼び出し回数の上限は既定値)。
pub async fn build_record_view(
    client: &HubSpotClient,
    rt: RecordType,
    id: &str,
    portal: &str,
) -> Result<CrmRecordResponse, HubSpotError> {
    build_record_view_with_budget(client, rt, id, portal, MAX_HUBSPOT_CALLS_PER_REQUEST).await
}

/// 打ち切り前に ID を数値の降順 (= 新しい順とみなす) に並べる。
/// HubSpot の ID が作成順に増えることは [推測] (公式の保証は確認していない)。
/// 数値にできない ID は末尾 (validate 済みなので通常は無い)。
pub(super) fn sort_ids_newest_first(ids: &mut [String]) {
    ids.sort_by(|a, b| {
        let na = a.parse::<u64>().ok();
        let nb = b.parse::<u64>().ok();
        nb.cmp(&na).then_with(|| a.cmp(b))
    });
}

/// 呼び出し回数の上限 `max_calls` を指定して組み立てる。本体の取得 (1 回目) は上限に関わらず行う。
pub async fn build_record_view_with_budget(
    client: &HubSpotClient,
    rt: RecordType,
    id: &str,
    portal: &str,
    max_calls: usize,
) -> Result<CrmRecordResponse, HubSpotError> {
    let mut budget = CallBudget::new(max_calls);
    let mut partial: Vec<CrmPartial> = Vec::new();
    let mut activities_truncated = false;

    // 1) 本体 + 関連 ID を 1 回で (本体の失敗だけが全体のエラー)
    let to_types: Vec<&str> = RecordType::ALL
        .iter()
        .filter(|o| **o != rt)
        .map(|o| o.api_name())
        .chain(READ_ENGAGEMENTS.iter().map(|e| e.api_name()))
        .collect();
    budget.used += 1;
    let (record, mut assocs) = match client
        .get_object_with_associations(rt.api_name(), id, record_properties(rt), &to_types)
        .await
    {
        Ok(v) => v,
        // 関連の型 (calls / notes / tasks / meetings 等) のどれかのスコープが共有鍵に無いと、
        // 本体ごと 401/403 になる可能性がある [推測: 公式に記載なし、実データ未確認]。
        // その場合は関連なしで本体だけ取り直し、関連とアクティビティは partial に出す。
        Err(HubSpotError::Auth { .. }) if budget.take() => {
            let rec = client
                .get_object(rt.api_name(), id, record_properties(rt))
                .await?;
            partial.push(CrmPartial::new("associations", "hubspot_auth"));
            (rec, Default::default())
        }
        Err(e) => return Err(e),
    };

    // アーカイブ済み (削除済み) は生きているものとして見せない。通常 GET は 404 を返すが、
    // archived: true で返ってきた場合に備える。
    if record.archived {
        return Err(HubSpotError::NotFound);
    }

    // --- 関連レコード (自分と同じ型は除く)。v3 の応答は関連ラベルを返さないので labels は空 ---
    let mut associations = CrmAssociations::default();
    let mut deal_contact_ids: Vec<String> = Vec::new();
    for other in RecordType::ALL {
        if other == rt {
            continue;
        }
        let (refs, more) = assocs.remove(other.api_name()).unwrap_or_default();
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

    // 2) Deal: 接触 (Call) は Deal より Contact に付くことが多い。deal → contacts → calls も辿る。
    //    (同じ Contact の別 Deal の通話も混ざりうる。絞り込みは PR4 以降)
    //    1 回の batch_associations で (call id, 経由した contact id) を得る。失敗しても本体は返す。
    let mut contact_calls: Vec<(String, String)> = Vec::new();
    if rt == RecordType::Deal && !deal_contact_ids.is_empty() {
        if deal_contact_ids.len() > MAX_DEAL_CONTACTS_FOR_CALLS {
            activities_truncated = true;
            // 打ち切るときだけ新しい contact を残す (打ち切らないときは HubSpot の返した順のまま)
            sort_ids_newest_first(&mut deal_contact_ids);
            deal_contact_ids.truncate(MAX_DEAL_CONTACTS_FOR_CALLS);
        }
        if budget.take() {
            match client
                .batch_associations(
                    RecordType::Contact.api_name(),
                    EngagementType::Call.api_name(),
                    &deal_contact_ids,
                )
                .await
            {
                Ok(map) => {
                    for cid in &deal_contact_ids {
                        for r in map.get(cid).into_iter().flatten() {
                            contact_calls.push((r.id.clone(), cid.clone()));
                        }
                    }
                }
                Err(e) => partial.push(CrmPartial::new("calls_via_contacts", e.error_kind())),
            }
        } else {
            partial.push(CrmPartial::new("calls_via_contacts", "call_budget"));
        }
    }

    // 3) Engagement を型ごとに batch read。Engagement は多対多なので同じ id が複数経路で現れる。
    //    直付きを先に入れ、後から来た contact 経由は重複なら捨てる (= 直付き優先)。
    let mut activities: Vec<Activity> = Vec::new();
    for et in READ_ENGAGEMENTS {
        let (refs, more) = assocs.remove(et.api_name()).unwrap_or_default();
        activities_truncated |= more;
        let mut seen: HashSet<String> = HashSet::new();
        let mut ids: Vec<String> = Vec::new();
        let mut via: HashMap<String, (RecordType, String)> = HashMap::new();
        for r in refs {
            if seen.insert(r.id.clone()) {
                via.insert(r.id.clone(), (rt, id.to_string()));
                ids.push(r.id);
            }
        }
        if et == EngagementType::Call {
            for (call_id, cid) in &contact_calls {
                if seen.insert(call_id.clone()) {
                    via.insert(call_id.clone(), (RecordType::Contact, cid.clone()));
                    ids.push(call_id.clone());
                }
            }
        }
        if ids.len() > MAX_ENGAGEMENTS_PER_TYPE {
            activities_truncated = true;
            // 直付き・contact 経由を区別せず新しいものを残す (直付きが多いと経由の新しい通話が全部落ちるため)
            sort_ids_newest_first(&mut ids);
            ids.truncate(MAX_ENGAGEMENTS_PER_TYPE);
        }
        if ids.is_empty() {
            continue;
        }
        if !budget.take() {
            partial.push(CrmPartial::new(et.api_name(), "call_budget"));
            continue;
        }
        match client
            .batch_read(et.api_name(), &ids, engagement_properties(et))
            .await
        {
            Ok(recs) => {
                for rec in recs.into_iter().filter(|r| !r.archived) {
                    let (via_type, via_id) =
                        via.get(&rec.id).cloned().unwrap_or((rt, id.to_string()));
                    activities.push(Activity {
                        et,
                        sort_ts: sort_timestamp(et, &rec),
                        record: rec,
                        via_type,
                        via_id,
                    });
                }
            }
            Err(e) => partial.push(CrmPartial::new(et.api_name(), e.error_kind())),
        }
    }

    // 時刻降順 (無いものは末尾)。同時刻は id で安定化。
    activities.sort_by(|a, b| {
        let ta = timestamp_millis(a.sort_ts.as_deref());
        let tb = timestamp_millis(b.sort_ts.as_deref());
        tb.cmp(&ta).then_with(|| a.record.id.cmp(&b.record.id))
    });
    let recent_activities: Vec<CrmActivity> = activities
        .into_iter()
        .take(MAX_RECENT_ACTIVITIES)
        .map(|a| CrmActivity {
            activity_type: a.et,
            timestamp: a.sort_ts,
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
            partial,
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
                partial: vec![CrmPartial::new("notes", "hubspot_auth")],
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
                         "activities_truncated": false,
                         "partial": [{"part": "notes", "error_kind": "hubspot_auth"}]}
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
