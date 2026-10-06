//! BPO のレコード単位の制限 (`GET /api/crm/{contacts|companies|deals}/{id}` の本文を返す前の関門)。
//!
//! 決定 (2026-10-01): BPO は「架電キューに出たレコードだけ」読める。
//! - **Deal**: 自分 (本人のメールに対応する HubSpot owner) が担当で、キューの条件 ([`deal_in_queue`]) に合う Deal だけ。
//! - **Contact / Company**: 上の条件に合う Deal に紐づくものだけ (Contact → deals / Company → deals の関連を引き、
//!   その Deal を 1 回の batch で読んで同じ条件を当てる)。
//! - admin / consultant は関門を通らない (全レコード)。
//!
//! 外れたら 403 `forbidden_record`。HubSpot の本文 (レコードの値) は一切返さず、**存在しない id も同じ 403**
//! (404 だと id の存在を探れるため)。関門の読み取りが HubSpot の障害で失敗したときはそのエラー (固定文言) を返し、
//! 権限が確かめられないまま通すことは無い。
//!
//! HubSpot 呼び出し (BPO のみ。本体の読み取りより前): 本人の owner 対応 (キャッシュ済みなら 0 回) +
//! Deal は 1 回 / Contact・Company は 2 回 (関連 + 関連 Deal の batch)。電話番号の有無は見ない
//! (`deal_in_queue` の注記)。関連 Deal が [`MAX_GATE_DEALS`] を超える Contact / Company は、先頭
//! [`MAX_GATE_DEALS`] 件だけを見る (それ以降の Deal だけがキューに出ているなら拒否側に倒れる)。

use axum::http::StatusCode;
use axum::response::Response;

use super::call_queue::{deal_in_queue, jst_today_ms, DEAL_PROPERTIES};
use super::routes::{error_json, hubspot_error_response, CrmCtx};
use crate::hubspot::{HubSpotClient, HubSpotError, RecordType};

/// Contact / Company の関連 Deal のうち、キューの条件を確かめる最大件数
pub const MAX_GATE_DEALS: usize = 100;

fn forbidden_record() -> Response {
    error_json(StatusCode::FORBIDDEN, "forbidden_record")
}

/// BPO が `rt` の `id` を読んでよいか。通れば `Ok(())`、通らなければそのまま返せる応答。
pub(super) async fn bpo_may_read(
    client: &HubSpotClient,
    ctx: &CrmCtx,
    rt: RecordType,
    id: &str,
    email: &str,
) -> Result<(), Response> {
    let owner = match ctx.queue.owner_for(client, email).await {
        Ok(Some(o)) => o,
        // owner を引けない BPO は何も読めない (全員分に倒さない。キューと同じ扱い)
        Ok(None) => return Err(error_json(StatusCode::FORBIDDEN, "owner_not_found")),
        Err(e) => return Err(hubspot_error_response(&e)),
    };
    let today_ms = jst_today_ms(ctx.queue.now());

    let deal_ids: Vec<String> = match rt {
        RecordType::Deal => {
            return match client.get_object("deals", id, DEAL_PROPERTIES).await {
                Ok(deal) if deal_in_queue(&deal, &owner, today_ms) => Ok(()),
                Ok(_) | Err(HubSpotError::NotFound) => Err(forbidden_record()),
                Err(e) => Err(hubspot_error_response(&e)),
            };
        }
        RecordType::Contact | RecordType::Company => {
            match client
                .get_object_with_associations(rt.api_name(), id, &[], &["deals"])
                .await
            {
                Ok((rec, mut assocs)) => {
                    if rec.archived {
                        return Err(forbidden_record());
                    }
                    assocs
                        .remove("deals")
                        .map(|(refs, _more)| refs.into_iter().map(|r| r.id).collect())
                        .unwrap_or_default()
                }
                Err(HubSpotError::NotFound) => return Err(forbidden_record()),
                Err(e) => return Err(hubspot_error_response(&e)),
            }
        }
    };
    if deal_ids.is_empty() {
        return Err(forbidden_record());
    }
    let deal_ids: Vec<String> = deal_ids.into_iter().take(MAX_GATE_DEALS).collect();
    match client.batch_read("deals", &deal_ids, DEAL_PROPERTIES).await {
        Ok(deals) if deals.iter().any(|d| deal_in_queue(d, &owner, today_ms)) => Ok(()),
        Ok(_) => Err(forbidden_record()),
        Err(e) => Err(hubspot_error_response(&e)),
    }
}
