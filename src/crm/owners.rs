//! `GET /api/crm/owners` (管理者のみ。架電キューの担当者の絞り込みを名前で選ぶための一覧)。
//!
//! 処理順: 認可 (`rbac::authorize`、未ログイン 401 / 許可外 403) → 役割 (admin 以外は 403 `forbidden`)
//! → HubSpot 未設定 503 → 一覧 (10 分キャッシュ)。**認可で落ちる限り HubSpot は 1 回も呼ばない**。
//!
//! HubSpot Owners API は有効な人 (`archived=false`) と退職者 (`archived=true`) を別の呼び出しで返し、
//! 1 ページ最大 100 件。両方をページの終わりまで読み、1 つの一覧にして返す (退職者は `archived: true`)。
//! 片方でも失敗したら一覧を返さず HubSpot のエラー (`error_kind`) をそのまま返す (半端な一覧は出さない)。
//! 失敗はキャッシュしない。同時に冷えた要求は 1 回の取得にまとめる。
//!
//! 名前は姓名を空白で結合。空なら email の @ より前、それも無ければ `(名前なし)`。
//! email は管理者向けの同名の人の見分けに付ける。

use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    extract::{Extension, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Json,
};
use serde::Serialize;
use serde_json::Value;
use tower_sessions::Session;
use ts_rs::TS;

use super::rbac::{self, CrmRole};
use super::routes::{
    error_json, hubspot_error_response, timeout_response, CrmCtx, CRM_REQUEST_DEADLINE,
};
use crate::hubspot::{HubSpotClient, HubSpotError};
use crate::AppState;

/// 一覧のキャッシュ有効期間
pub const OWNERS_TTL: Duration = Duration::from_secs(10 * 60);
/// 有効 / 退職者それぞれで読むページ数の上限 (100 件 x 30 = 3,000 人)。超えたら `truncated`
pub const MAX_OWNER_PAGES: usize = 30;

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmOwner {
    /// HubSpot owner ID (架電キューの `owner` パラメータにそのまま渡す)
    pub id: String,
    /// 姓名 (空なら email の @ より前、それも無ければ「(名前なし)」)
    pub name: String,
    pub email: Option<String>,
    /// 退職者など (HubSpot 上でアーカイブ済み)
    pub archived: bool,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmOwnersResponse {
    pub owners: Vec<CrmOwner>,
    /// ページ数の上限で一覧が途中で切れた
    pub truncated: bool,
    pub generated_at: String,
}

/// 一覧のキャッシュ (`CallQueueState` が持つ)
pub struct OwnerListCache {
    ttl: Duration,
    slot: tokio::sync::Mutex<Option<(Instant, Arc<CrmOwnersResponse>)>>,
}

impl OwnerListCache {
    pub fn new() -> Self {
        Self::with_ttl(OWNERS_TTL)
    }

    pub fn with_ttl(ttl: Duration) -> Self {
        Self {
            ttl,
            slot: tokio::sync::Mutex::new(None),
        }
    }

    /// 有効期間内ならキャッシュを返し、そうでなければ取得する (取得中は他の要求を待たせて 1 回にまとめる)
    async fn get(&self, client: &HubSpotClient) -> Result<Arc<CrmOwnersResponse>, HubSpotError> {
        let mut slot = self.slot.lock().await;
        if let Some((at, v)) = slot.as_ref() {
            if at.elapsed() < self.ttl {
                return Ok(v.clone());
            }
        }
        let fresh = Arc::new(fetch_all(client).await?);
        *slot = Some((Instant::now(), fresh.clone()));
        Ok(fresh)
    }
}

impl Default for OwnerListCache {
    fn default() -> Self {
        Self::new()
    }
}

fn s(v: &Value, key: &str) -> Option<String> {
    v.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|t| !t.is_empty())
        .map(str::to_string)
}

/// owner 1 件を表示用にする。ID が無い行は捨てる (None)
fn parse_owner(v: &Value, archived: bool) -> Option<CrmOwner> {
    let id = match v.get("id")? {
        Value::String(t) => t.trim().to_string(),
        Value::Number(n) => n.to_string(),
        _ => return None,
    };
    if id.is_empty() {
        return None;
    }
    let email = s(v, "email");
    let full = [s(v, "firstName"), s(v, "lastName")]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ");
    let name = if !full.is_empty() {
        full
    } else if let Some(local) = email
        .as_deref()
        .and_then(|e| e.split('@').next())
        .filter(|l| !l.is_empty())
    {
        local.to_string()
    } else {
        "(名前なし)".to_string()
    };
    Some(CrmOwner {
        id,
        name,
        email,
        archived,
    })
}

/// 1 種類 (有効 or 退職者) をページの終わりまで読む。上限で打ち切ったら true
async fn fetch_kind(
    client: &HubSpotClient,
    archived: bool,
    out: &mut Vec<CrmOwner>,
) -> Result<bool, HubSpotError> {
    let mut after: Option<String> = None;
    for _ in 0..MAX_OWNER_PAGES {
        let page = client.owners_page(archived, after.as_deref()).await?;
        let results = page
            .get("results")
            .and_then(Value::as_array)
            .ok_or_else(|| HubSpotError::Decode("owners without results".into()))?;
        out.extend(results.iter().filter_map(|r| parse_owner(r, archived)));
        match page
            .pointer("/paging/next/after")
            .and_then(Value::as_str)
            .filter(|a| !a.is_empty())
        {
            Some(a) => after = Some(a.to_string()),
            None => return Ok(false),
        }
    }
    Ok(true)
}

async fn fetch_all(client: &HubSpotClient) -> Result<CrmOwnersResponse, HubSpotError> {
    let mut owners = Vec::new();
    let t1 = fetch_kind(client, false, &mut owners).await?;
    let t2 = fetch_kind(client, true, &mut owners).await?;
    // 同じ ID は有効な方 (先に入れた方) を残す
    let mut seen = std::collections::HashSet::new();
    owners.retain(|o| seen.insert(o.id.clone()));
    owners.sort_by(|a, b| {
        a.archived
            .cmp(&b.archived)
            .then_with(|| a.name.cmp(&b.name))
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(CrmOwnersResponse {
        owners,
        truncated: t1 || t2,
        generated_at: chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
    })
}

pub(super) async fn get_owners(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(denied) => return denied.into_response(),
    };
    if rbac::resolve_role(&principal) != CrmRole::Admin {
        return error_json(StatusCode::FORBIDDEN, "forbidden");
    }
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.queue.owner_list.get(&client)).await {
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "crm owners timed out");
            timeout_response()
        }
        Ok(Err(e)) => {
            tracing::warn!(error_kind = e.error_kind(), "crm owners read failed");
            hubspot_error_response(&e)
        }
        Ok(Ok(list)) => Json((*list).clone()).into_response(),
    }
}
