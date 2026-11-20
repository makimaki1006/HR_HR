//! 応募者の電話番号への、社内からの発信 (Zoom Phone の通話履歴。読み取りだけ)。
//!
//! CS の通話は HubSpot の応募には付かないので、番号で Zoom を引く。Zoom は 1 回に 1 か月までしか
//! 返さないため、30 日ずつ 2 回に分けて直近 60 日を読む。結果は番号ごとにプロセス内に 3 分持つ。
use super::*;
use crate::crm::talk_review::zoom::{normalize_number, NumberDial, ZoomClient};
use std::sync::{LazyLock, OnceLock};

/// さかのぼる日数 (30 日の窓 2 つ)
const WINDOW_DAYS: i64 = 30;
const WINDOWS: i64 = 2;
const CALLS_TTL: Duration = Duration::from_secs(180);
const ZOOM_DEADLINE: Duration = Duration::from_secs(15);

#[derive(Debug, Clone, Serialize, TS)]
pub struct ApplicantCall {
    /// かけた時刻 (RFC 3339)
    pub at: String,
    /// かけた人 (分からなければ空)
    pub caller: String,
    /// 相手が出たか
    pub answered: bool,
    /// 通話の長さ (秒)
    pub seconds: u32,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct ApplicantCalls {
    /// さかのぼった日数
    pub days: u32,
    /// 新しい順
    pub calls: Vec<ApplicantCall>,
}

static ZOOM: OnceLock<Option<Arc<ZoomClient>>> = OnceLock::new();
/// 鍵は番号の下 9 桁 (同じ人の別の応募でも使い回す)
static CACHE: LazyLock<TtlCache<String, ApplicantCalls>> =
    LazyLock::new(|| TtlCache::new(CALLS_TTL, 500));

async fn phone_of(ctx: &CrmCtx, client: &HubSpotClient, id: &str) -> Result<String, HubSpotError> {
    // 画面は先に詳細を読むので、たいていここで見つかる (HubSpot を呼ばない)
    let key = format!("{}:{id}", client.applicant_write_revision());
    if let Some(d) = ctx.applicants.details.get(&key) {
        return Ok(d.phone);
    }
    let rows = client
        .batch_read(OBJECT, &[id.to_string()], &["denwabangou"])
        .await?;
    let record = rows.first().ok_or(HubSpotError::NotFound)?;
    let v = serde_json::to_value(record)
        .map_err(|_| HubSpotError::Decode("applicant record".into()))?;
    Ok(text(&v["properties"], "denwabangou"))
}

fn merge(mut dials: Vec<NumberDial>) -> Vec<ApplicantCall> {
    dials.sort_by(|a, b| b.start_ms.cmp(&a.start_ms));
    dials.dedup_by(|a, b| a.start_ms == b.start_ms && a.caller == b.caller);
    dials
        .into_iter()
        .map(|d| ApplicantCall {
            at: d.start_time,
            caller: d.caller,
            answered: d.answered,
            seconds: d.seconds,
        })
        .collect()
}

pub(in crate::crm) async fn calls(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    session: Session,
    Path(id): Path<String>,
) -> Response {
    if let Err(e) = rbac::authorize(&session, &state, &ctx.access, None).await {
        return e.into_response();
    }
    if !super::super::routes::is_valid_id(&id) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_id");
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let phone = match phone_of(&ctx, &client, &id).await {
        Ok(p) => p,
        Err(e) => return hubspot_error_response(&e),
    };
    let days = (WINDOW_DAYS * WINDOWS) as u32;
    let Some(number9) = normalize_number(&phone) else {
        return Json(ApplicantCalls {
            days,
            calls: Vec::new(),
        })
        .into_response();
    };
    if let Some(v) = CACHE.get(&number9) {
        return Json(v).into_response();
    }
    let Some(zoom) = ZOOM.get_or_init(ZoomClient::from_env).clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "calls_unavailable");
    };
    let today = crate::crm::caller_metrics::jst_day(chrono::Utc::now()).0;
    let back = |n: i64| today - chrono::Duration::days(n);
    let read = async {
        tokio::try_join!(
            zoom.dials_to_number(&phone, back(WINDOW_DAYS), today),
            zoom.dials_to_number(&phone, back(WINDOW_DAYS * 2), back(WINDOW_DAYS + 1)),
        )
    };
    match tokio::time::timeout(ZOOM_DEADLINE, read).await {
        Ok(Ok((mut recent, older))) => {
            recent.extend(older);
            let out = ApplicantCalls {
                days,
                calls: merge(recent),
            };
            CACHE.insert(number9, out.clone());
            Json(out).into_response()
        }
        Ok(Err(e)) => {
            tracing::warn!("applicant calls: zoom read failed: {e:?}");
            error_json(StatusCode::BAD_GATEWAY, "calls_unavailable")
        }
        Err(_) => error_json(StatusCode::GATEWAY_TIMEOUT, "calls_unavailable"),
    }
}
