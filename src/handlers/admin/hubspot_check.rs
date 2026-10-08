//! `GET /api/admin/hubspot-check` : 営業KPI の HubSpot 直読みを有効にする前の鍵の診断 (管理者専用)。
//!
//! 鍵 (`HUBSPOT_ACCESS_TOKEN`) が入っているか、必要な scope を持つかを返す。
//! **鍵の値・一部・ハッシュは返さず、ログにも出さない**。HubSpot への呼び出しは 1 回
//! (`HubSpotClient::access_token_info`、30 秒タイムアウト、成功のみ 5 分キャッシュ)。
//! `require_admin_mw` の内側に置くので、非管理者は 403・未ログインは 401 で HubSpot には届かない。

use std::sync::Arc;
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Serialize;

use crate::hubspot::{HubSpotClient, HubSpotError};
use crate::AppState;

/// 営業KPI 直読み (#66) と架電 CRM が使う読み取り scope。
pub const REQUIRED_SCOPES: [&str; 3] = [
    "crm.objects.deals.read",
    "crm.objects.owners.read",
    "crm.schemas.deals.read",
];

/// HubSpot への 1 回の呼び出しに掛ける上限。
pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Serialize)]
pub struct RequiredScope {
    pub scope: &'static str,
    /// scope を取得できたときだけ true / false。取得できなければ null
    pub present: Option<bool>,
}

#[derive(Debug, Serialize)]
pub struct HubSpotCheckResponse {
    /// `HUBSPOT_ACCESS_TOKEN` が設定されているか
    pub configured: bool,
    pub checked_at: Option<String>,
    /// 5 分キャッシュから返したか
    pub cached: bool,
    pub portal_id: Option<String>,
    pub scopes: Option<Vec<String>>,
    pub required: Vec<RequiredScope>,
    pub all_required_present: Option<bool>,
    /// `HubSpotError::error_kind()` の値。成功なら null
    pub error_kind: Option<String>,
    pub message: Option<String>,
}

fn required(scopes: Option<&[String]>) -> Vec<RequiredScope> {
    REQUIRED_SCOPES
        .iter()
        .map(|s| RequiredScope {
            scope: s,
            present: scopes.map(|have| have.iter().any(|h| h == s)),
        })
        .collect()
}

fn failure(configured: bool, e: &HubSpotError) -> (StatusCode, HubSpotCheckResponse) {
    let message = match e {
        HubSpotError::NotConfigured => {
            "HUBSPOT_ACCESS_TOKEN が設定されていません (Render の env を確認)".to_string()
        }
        HubSpotError::Auth { status } => format!(
            "HubSpot が鍵を受け付けませんでした ({status})。鍵が無効・失効、または作り直し後に Render の env が古いままの可能性"
        ),
        HubSpotError::NotFound => {
            "トークン情報 API が見つかりません。この種類の鍵では使えない可能性があります (手順書の代替確認へ)".to_string()
        }
        HubSpotError::Timeout => "HubSpot の応答が 30 秒以内に返りませんでした".to_string(),
        HubSpotError::RateLimited => "HubSpot のレート制限に達しました。少し待って再実行".to_string(),
        HubSpotError::Upstream { status } => format!(
            "HubSpot がエラーを返しました ({status})。400 系ならこの種類の鍵でトークン情報 API が使えない可能性があります"
        ),
        HubSpotError::Transport(_) => "HubSpot に接続できませんでした".to_string(),
        HubSpotError::Decode(_) => "HubSpot の応答の形が想定と違いました".to_string(),
        HubSpotError::Busy => {
            "HubSpot が混み合っています。少し待ってから再試行してください".to_string()
        }
    };
    (
        StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_GATEWAY),
        HubSpotCheckResponse {
            configured,
            checked_at: None,
            cached: false,
            portal_id: None,
            scopes: None,
            required: required(None),
            all_required_present: None,
            error_kind: Some(e.error_kind().to_string()),
            message: Some(message),
        },
    )
}

/// 診断の本体 (テストでは短いタイムアウトを渡す)。
pub async fn run_check(
    client: Option<&HubSpotClient>,
    timeout: Duration,
) -> (StatusCode, HubSpotCheckResponse) {
    let Some(client) = client else {
        return failure(false, &HubSpotError::NotConfigured);
    };
    // 外側の timeout は保険 (reqwest 側にも同じ値を掛けている)
    let result = match tokio::time::timeout(
        timeout + Duration::from_secs(1),
        client.access_token_info(timeout),
    )
    .await
    {
        Ok(r) => r,
        Err(_) => Err(HubSpotError::Timeout),
    };
    match result {
        Ok((info, cached)) => {
            let req = required(Some(&info.scopes));
            let all = req.iter().all(|r| r.present == Some(true));
            let mut scopes = info.scopes;
            scopes.sort();
            (
                StatusCode::OK,
                HubSpotCheckResponse {
                    configured: true,
                    checked_at: Some(info.checked_at),
                    cached,
                    portal_id: info.portal_id,
                    scopes: Some(scopes),
                    required: req,
                    all_required_present: Some(all),
                    error_kind: None,
                    message: None,
                },
            )
        }
        Err(e) => {
            // ログには error_kind だけ (鍵・応答本文は出さない)
            tracing::warn!(error_kind = e.error_kind(), "admin hubspot-check failed");
            failure(true, &e)
        }
    }
}

/// GET /api/admin/hubspot-check
pub async fn api_hubspot_check(State(state): State<Arc<AppState>>) -> Response {
    let (status, body) = run_check(state.hubspot.as_deref(), CHECK_TIMEOUT).await;
    (status, Json(body)).into_response()
}

#[cfg(test)]
#[path = "hubspot_check_tests.rs"]
mod hubspot_check_tests;
