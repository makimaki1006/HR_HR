//! 「この案でCSVを出す」を押したときに、リクロジ向けの新しい求人 (0-420) をHubSpotに1件だけ作る。
//! 元のHRハッカー求人が関連している取引すべてに付ける。リクロジの求人IDを人が入れる口もここ。
//!
//! 二重に作らない仕組み (新しい表・項目は使わない):
//! 1. 作る処理は全体で1本ずつ (プロセス内のロック)。
//! 2. 作る前に、同じ元HR求人ID (`reclogi_moto_hr_kyuujin_id`) で `id_reclogi` が空の求人をHubSpotで検索し、
//!    あればそれを使う (作らない)。
//! 3. 作った直後は検索の反映が遅れるので、作った求人のIDを10分だけプロセス内に覚えて同じ扱いにする。
//!    (単一インスタンス前提。複数台になる場合は検索だけが頼りになる。)
use super::{authorized_user, drafts, valid_id, Access, JobReadService, ReadError, Record};
use crate::{hubspot::HubSpotError, job_gen::rikurozi::GENERATED, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use serde::Deserialize;
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_sessions::Session;

/// 求人票管理パイプラインの「募集準備中」(夜間の取込の `listing_stage` と同じ値)。
const PIPELINE_ID: &str = "t_5f0228538e8b269ecd3b553b104c008c";
const STAGE_PREPARING: &str = "1343099546";
const STATUS_PREPARING: &str = "公開前";
/// 元のHR求人から写す項目 (reclogi_applicants.py の HR_CONTENT_PROPS と COPY_FROM_HR に合わせる)。
const COPY_FROM_HR: [&str; 12] = [
    "zhizhong",
    "qinwude",
    "kinmujikan",
    "todoufuken",
    "shikuchouson",
    "guyongxingtai",
    "kyuuyokeitai",
    "nianshouxiaxian",
    "nianshoushangxian",
    "hubspot_owner_id",
    "ichijitaiounoumu_deforuto",
    "customer_sheet_url",
];
/// AIが書いた列 → 求人の項目。ほかのAI列はCSVだけに使う。
const GENERATED_TO_PROPERTY: [(&str, &str); 3] = [
    ("案件名", "hs_name"),
    ("仕事内容", "shigotonaiyou"),
    ("応募資格", "hissushikaku_shousai"),
];
const REMEMBER: Duration = Duration::from_secs(10 * 60);
const MAX_VALUE_CHARS: usize = 20_000;

fn err(status: StatusCode, code: &'static str) -> ReadError {
    ReadError(status, code)
}
fn hubspot_error(e: HubSpotError) -> ReadError {
    match e {
        HubSpotError::Busy => err(StatusCode::SERVICE_UNAVAILABLE, "hubspot_busy"),
        HubSpotError::NotFound => err(StatusCode::NOT_FOUND, "listing_not_found"),
        HubSpotError::Auth { .. } => err(StatusCode::FORBIDDEN, "hubspot_scope_denied"),
        HubSpotError::RateLimited => err(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
        _ => err(StatusCode::BAD_GATEWAY, "hubspot_write_failed"),
    }
}
fn require_csrf(headers: &HeaderMap) -> Result<(), ReadError> {
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("fetch")
    {
        return Err(err(StatusCode::FORBIDDEN, "csrf_required"));
    }
    Ok(())
}

/// 元の求人ID → (作った求人のID, 作った時刻)。検索の反映待ちの間だけ使う。
fn created_lock() -> &'static Mutex<BTreeMap<String, (String, Instant)>> {
    static LOCK: OnceLock<Mutex<BTreeMap<String, (String, Instant)>>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(BTreeMap::new()))
}
fn id_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// 作る求人の項目。AIが書いた文があればそれを、なければ原本を使わず空のままにする (原本の文を無審査で出さない)。
pub(super) fn build_properties(
    hr: &Record,
    hr_job_id: &str,
    generated: &BTreeMap<String, String>,
) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    for key in COPY_FROM_HR {
        if let Some(value) = hr.value(key) {
            props.insert(key.to_owned(), value.to_owned());
        }
    }
    for (column, property) in GENERATED_TO_PROPERTY {
        if let Some(value) = generated
            .get(column)
            .map(|v| v.trim())
            .filter(|v| !v.is_empty())
        {
            props.insert(property.to_owned(), value.to_owned());
        }
    }
    props.insert("reclogi_moto_hr_kyuujin_id".into(), hr_job_id.to_owned());
    props.insert("kyuujin_status".into(), STATUS_PREPARING.into());
    props.insert("kanri_kubun".into(), "弊社管理".into());
    props.insert("hs_pipeline".into(), PIPELINE_ID.into());
    props.insert("hs_pipeline_stage".into(), STAGE_PREPARING.into());
    props
}

const READ_PROPS: [&str; 17] = [
    "id_hrhakkaa",
    "reclogi_moto_hr_kyuujin_id",
    "id_reclogi",
    "zhizhong",
    "qinwude",
    "kinmujikan",
    "todoufuken",
    "shikuchouson",
    "guyongxingtai",
    "kyuuyokeitai",
    "nianshouxiaxian",
    "nianshoushangxian",
    "hubspot_owner_id",
    "ichijitaiounoumu_deforuto",
    "customer_sheet_url",
    "hs_name",
    "id_airwork",
];

impl JobReadService {
    /// 同じ元HR求人IDのリクロジ向け求人を探す。`only_without_id` なら `id_reclogi` が空のものだけ。
    async fn rikurozi_listings_for(
        &self,
        hr_job_id: &str,
        only_without_id: bool,
    ) -> Result<Vec<Record>, ReadError> {
        let mut filters = vec![
            json!({"propertyName":"reclogi_moto_hr_kyuujin_id","operator":"EQ","value":hr_job_id}),
        ];
        if only_without_id {
            filters.push(json!({"propertyName":"id_reclogi","operator":"NOT_HAS_PROPERTY"}));
        }
        let body = json!({
            "filterGroups":[{"filters":filters}],
            "properties":["hs_name","id_reclogi","id_hrhakkaa"],
            "sorts":[{"propertyName":"createdate","direction":"DESCENDING"}],
            "limit":10
        });
        let data = self
            .hubspot()
            .search("0-420", body)
            .await
            .map_err(hubspot_error)?;
        let rows = data["results"]
            .as_array()
            .ok_or_else(|| err(StatusCode::BAD_GATEWAY, "hubspot_invalid_response"))?;
        Ok(rows
            .iter()
            .filter_map(|row| serde_json::from_value::<Record>(row.clone()).ok())
            .filter(|r| r.value("id_hrhakkaa").is_none())
            .collect())
    }

    /// 取引への関連のうち足りないものを作る。付いている取引と付けられなかった数を返す。
    async fn link_listing_to_deals(
        &self,
        listing_id: &str,
        deals: &[String],
        already: &BTreeSet<String>,
    ) -> (Vec<String>, usize) {
        let (mut ok, mut failed) = (Vec::new(), 0);
        for deal in deals {
            let linked = already.contains(deal)
                || self
                    .hubspot()
                    .associate_listing_deal(listing_id, deal)
                    .await
                    .is_ok();
            if linked {
                ok.push(deal.clone());
            } else {
                failed += 1;
            }
        }
        (ok, failed)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CreateRequest {
    /// 案の作成結果の「列名 → AIが書いた文」。
    generated: BTreeMap<String, String>,
}

pub(super) async fn create(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<CreateRequest>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    require_csrf(&headers)?;
    let service = access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    // 案の保存と同じ「書き込み」の設定に従う。止まっているときは何も作らない。
    let allowed = drafts::Writes::from_env().allows(&id);
    create_inner(service, &id, req, allowed).await
}

async fn create_inner(
    service: &JobReadService,
    id: &str,
    req: CreateRequest,
    allowed: bool,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    if !allowed {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"status":"skipped","listing_id":null,"linked_deals":0,"unlinked_deals":0})),
        ));
    }
    let known: BTreeSet<&str> = GENERATED.iter().map(|(_, column, _)| *column).collect();
    if req.generated.len() > GENERATED.len()
        || req
            .generated
            .iter()
            .any(|(k, v)| !known.contains(k.as_str()) || v.chars().count() > MAX_VALUE_CHARS)
    {
        return Err(err(StatusCode::BAD_REQUEST, "invalid_generated_values"));
    }
    if req
        .generated
        .get("案件名")
        .is_none_or(|v| v.trim().is_empty())
    {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "listing_title_missing",
        ));
    }
    let mut remembered = created_lock().lock().await;
    remembered.retain(|_, (_, at)| at.elapsed() < REMEMBER);
    let hr = service
        .batch("0-420", &[id.to_owned()], &READ_PROPS)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "listing_not_found"))?;
    let hr_job_id = hr
        .value("id_hrhakkaa")
        .ok_or_else(|| err(StatusCode::UNPROCESSABLE_ENTITY, "hrh_source_required"))?
        .to_owned();
    let deals = service.all_associations("0-420", id, "deals").await?;
    if deals.is_empty() {
        return Err(err(
            StatusCode::UNPROCESSABLE_ENTITY,
            "deal_relation_missing",
        ));
    }
    // 1. 最近作った求人 / HubSpot にすでにある求人を使う。
    let existing = match remembered.get(&hr_job_id) {
        Some((listing, _)) => Some(listing.clone()),
        None => service
            .rikurozi_listings_for(&hr_job_id, true)
            .await?
            .into_iter()
            .next()
            .map(|r| r.id),
    };
    let (listing_id, status, already) = match existing {
        Some(listing) => {
            let (linked, _) = service
                .hubspot()
                .list_associations("0-420", &listing, "deals")
                .await
                .map_err(hubspot_error)?;
            let already = linked.into_iter().map(|a| a.id).collect::<BTreeSet<_>>();
            (listing, "existing", already)
        }
        None => {
            let props = build_properties(&hr, &hr_job_id, &req.generated);
            let created = service
                .hubspot()
                .create_rikurozi_listing(&props)
                .await
                .map_err(hubspot_error)?;
            remembered.insert(hr_job_id.clone(), (created.clone(), Instant::now()));
            (created, "created", BTreeSet::new())
        }
    };
    let (linked_deals, unlinked) = service
        .link_listing_to_deals(&listing_id, &deals, &already)
        .await;
    let linked = linked_deals.len();
    drop(remembered);
    // 一覧の作り直し (6 時間ごと) を待たずに、案件管理の一覧へ「公開前」の求人として出す。
    // 失敗しても作った求人は HubSpot にあり、次の作り直しで出るので、応答は変えない。
    if let Err(e) = super::listings::refresh_listing(service, &listing_id, &linked_deals).await {
        tracing::warn!(listing = %listing_id, code = e.code(), "job_copy_rikurozi_listing_index_update_failed");
    }
    tracing::info!(listing = %id, created = status == "created", linked, unlinked, "job_copy_rikurozi_listing");
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({
            "status": status,
            "listing_id": listing_id,
            "linked_deals": linked,
            "unlinked_deals": unlinked,
        })),
    ))
}

/// 元のHR求人から作ったリクロジ向け求人の一覧 (リクロジの求人IDの入力欄用)。
pub(super) async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    let service = access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let hr = service
        .batch("0-420", std::slice::from_ref(&id), &["id_hrhakkaa"])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "listing_not_found"))?;
    let Some(hr_job_id) = hr.value("id_hrhakkaa") else {
        return Ok((
            [(header::CACHE_CONTROL, "private, no-store")],
            Json(json!({"listings": []})),
        ));
    };
    let mut rows = service.rikurozi_listings_for(hr_job_id, false).await?;
    if let Some((listing, at)) = created_lock().lock().await.get(hr_job_id) {
        if at.elapsed() < REMEMBER && !rows.iter().any(|r| &r.id == listing) {
            rows.insert(
                0,
                Record {
                    id: listing.clone(),
                    properties: BTreeMap::new(),
                },
            );
        }
    }
    let listings: Vec<Value> = rows
        .iter()
        .map(|r| json!({"listing_id": r.id, "name": r.value("hs_name"), "reclogi_id": r.value("id_reclogi")}))
        .collect();
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listings": listings})),
    ))
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct IdRequest {
    reclogi_id: String,
}

/// リクロジの求人IDを保存する。ほかの求人がすでに同じIDなら保存しない。
pub(super) async fn put_reclogi_id(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<IdRequest>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    require_csrf(&headers)?;
    let value = req.reclogi_id.trim();
    if value.is_empty() || value.len() > 15 || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(err(StatusCode::BAD_REQUEST, "reclogi_id_invalid"));
    }
    let service = access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    let _guard = id_lock().lock().await;
    // 対象は「リクロジ向けに作った求人」だけ (HR・AirWork の求人には書かない)。
    let record = service
        .hubspot()
        .get_object_fresh(
            "0-420",
            &id,
            &[
                "id_hrhakkaa",
                "id_airwork",
                "reclogi_moto_hr_kyuujin_id",
                "id_reclogi",
            ],
        )
        .await
        .map_err(hubspot_error)?;
    let has = |key: &str| {
        record
            .properties
            .get(key)
            .and_then(|v| v.as_deref())
            .is_some_and(|v| !v.trim().is_empty())
    };
    if !has("reclogi_moto_hr_kyuujin_id") || has("id_hrhakkaa") || has("id_airwork") {
        return Err(err(StatusCode::CONFLICT, "not_a_rikurozi_listing"));
    }
    if has("id_reclogi") {
        return Err(err(StatusCode::CONFLICT, "reclogi_id_already_set"));
    }
    let found = service
        .hubspot()
        .search(
            "0-420",
            json!({
                "filterGroups":[{"filters":[{"propertyName":"id_reclogi","operator":"EQ","value":value}]}],
                "properties":["hs_name"],
                "limit":5
            }),
        )
        .await
        .map_err(hubspot_error)?;
    let used = found["results"]
        .as_array()
        .is_some_and(|rows| rows.iter().any(|r| r["id"].as_str() != Some(id.as_str())));
    if used {
        return Err(err(StatusCode::CONFLICT, "reclogi_id_in_use"));
    }
    service
        .hubspot()
        .patch_listing_reclogi_id(&id, value)
        .await
        .map_err(hubspot_error)?;
    service.detail_cache.forget_listing(&id);
    // 一覧の行も差し替える (公開前で ID が空の行 → リクロジの求人IDの行)。
    if let Err(e) = super::listings::refresh_listing(service, &id, &[]).await {
        tracing::warn!(listing = %id, code = e.code(), "job_copy_reclogi_id_index_update_failed");
    }
    tracing::info!(listing = %id, "job_copy_reclogi_id_saved");
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listing_id": id, "reclogi_id": value})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::{to_bytes, Body},
        http::{Method, Request},
        routing::any,
        Router,
    };

    /// 偽の HubSpot。受けた要求を (メソッド, パス, 本文) で記録する。
    #[derive(Default)]
    struct Remote {
        calls: Vec<(String, String, Value)>,
        /// 作った求人 (id, properties)
        created: Vec<(String, Value)>,
        /// true なら検索が作った求人をすぐ返す (反映が済んだ状態)
        search_sees_created: bool,
        /// id_reclogi の検索で返す別の求人 ID
        id_reclogi_holder: Option<String>,
        listing900_reclogi: Option<String>,
    }
    type Shared = Arc<std::sync::Mutex<Remote>>;

    fn hr_props() -> Value {
        json!({
            "id_hrhakkaa":"1234567","hs_name":"HRの求人","zhizhong":"介護職","qinwude":"東京都千代田区",
            "kinmujikan":"9:00-18:00","todoufuken":"東京都","shikuchouson":"千代田区",
            "guyongxingtai":"part_time","kyuuyokeitai":"HOUR","hubspot_owner_id":"777",
            "customer_sheet_url":"https://docs.google.com/spreadsheets/d/x","shigotonaiyou":"元の仕事内容"
        })
    }
    async fn serve(remote: Shared) -> String {
        let app = Router::new().fallback(any(move |req: Request<Body>| {
            let remote = remote.clone();
            async move {
                let method = req.method().clone();
                let path = req.uri().path().to_owned();
                let bytes = to_bytes(req.into_body(), 1_000_000).await.unwrap();
                let body: Value = if bytes.is_empty() {
                    Value::Null
                } else {
                    serde_json::from_slice(&bytes).unwrap()
                };
                let mut r = remote.lock().unwrap();
                r.calls
                    .push((method.to_string(), path.clone(), body.clone()));
                let reply = match (method, path.as_str()) {
                    (Method::POST, "/crm/v3/objects/0-420/batch/read")
                        if body["inputs"][0]["id"] == "900" =>
                    {
                        // 一覧の索引に入れるための読み直し。作った求人 (公開前) を返す。
                        let mut props = r.created.first().map(|(_, p)| p.clone()).unwrap_or_else(
                            || json!({"reclogi_moto_hr_kyuujin_id":"1234567","kyuujin_status":"公開前"}),
                        );
                        if let Some(v) = &r.listing900_reclogi {
                            props["id_reclogi"] = json!(v);
                        }
                        json!({"results":[{"id":"900","properties":props}]})
                    }
                    (Method::POST, "/crm/v3/objects/0-420/batch/read") => {
                        json!({"results":[{"id":"1","properties":hr_props()}]})
                    }
                    (Method::POST, "/crm/v4/associations/0-420/deals/batch/read") => {
                        json!({"results":[{"from":{"id":"900"},"to":[{"toObjectId":10}]}]})
                    }
                    (Method::POST, "/crm/v4/associations/0-420/0-421/batch/read") => {
                        json!({"results":[],"errors":[{"category":"OBJECT_NOT_FOUND",
                            "subCategory":"crm.associations.NO_ASSOCIATIONS_FOUND",
                            "context":{"fromObjectId":["900"]}}]})
                    }
                    (Method::POST, "/crm/v3/objects/deals/batch/read") => {
                        let rows: Vec<Value> = body["inputs"]
                            .as_array()
                            .unwrap()
                            .iter()
                            .map(|i| json!({"id":i["id"],"properties":{
                                "dealname":"取引","pipeline":"21596025","dealstage":"s"}}))
                            .collect();
                        json!({"results":rows})
                    }
                    (Method::GET, "/crm/v4/objects/0-420/1/associations/deals") => {
                        json!({"results":[{"toObjectId":10},{"toObjectId":20}]})
                    }
                    (Method::POST, "/crm/v3/objects/0-420/search") => {
                        let filters = body["filterGroups"][0]["filters"].as_array().unwrap();
                        if filters[0]["propertyName"] == "id_reclogi" {
                            let rows: Vec<Value> = r
                                .id_reclogi_holder
                                .iter()
                                .map(|id| json!({"id":id,"properties":{}}))
                                .collect();
                            json!({"results":rows})
                        } else {
                            let rows: Vec<Value> = if r.search_sees_created {
                                r.created
                                    .iter()
                                    .map(|(id, p)| json!({"id":id,"properties":p}))
                                    .collect()
                            } else {
                                vec![]
                            };
                            json!({"results":rows})
                        }
                    }
                    (Method::POST, "/crm/v3/objects/0-420") => {
                        let id = format!("{}", 900 + r.created.len());
                        r.created.push((id.clone(), body["properties"].clone()));
                        json!({"id":id,"properties":body["properties"]})
                    }
                    (Method::GET, "/crm/v4/objects/0-420/900/associations/deals") => {
                        json!({"results":[{"toObjectId":10}]})
                    }
                    (Method::PUT, p)
                        if p.starts_with(
                            "/crm/v4/objects/0-420/900/associations/default/deals/",
                        ) =>
                    {
                        json!({})
                    }
                    (Method::GET, "/crm/v3/objects/0-420/900") => {
                        let mut props = json!({"reclogi_moto_hr_kyuujin_id":"1234567"});
                        if let Some(v) = &r.listing900_reclogi {
                            props["id_reclogi"] = json!(v);
                        }
                        json!({"id":"900","properties":props})
                    }
                    (Method::GET, "/crm/v3/objects/0-420/1") => {
                        json!({"id":"1","properties":{"id_hrhakkaa":"1234567"}})
                    }
                    (Method::PATCH, "/crm/v3/objects/0-420/900") => {
                        r.listing900_reclogi =
                            body["properties"]["id_reclogi"].as_str().map(str::to_owned);
                        json!({"id":"900","properties":body["properties"]})
                    }
                    other => panic!("unexpected request {other:?}"),
                };
                axum::Json(reply)
            }
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        base
    }
    fn request() -> CreateRequest {
        CreateRequest {
            generated: BTreeMap::from([
                ("案件名".to_owned(), "新しい案件名".to_owned()),
                ("仕事内容".to_owned(), "AIが書いた仕事内容".to_owned()),
                ("応募資格".to_owned(), "AIが書いた応募資格".to_owned()),
                ("キャッチコピー".to_owned(), "CSVだけに使う".to_owned()),
            ]),
        }
    }
    fn count(remote: &Shared, method: &str, path: &str) -> usize {
        remote
            .lock()
            .unwrap()
            .calls
            .iter()
            .filter(|(m, p, _)| m == method && p == path)
            .count()
    }
    /// 同じ元HR求人IDを使うテストが並列で動いても、覚えている表が混ざらないよう直列にする。
    fn test_guard() -> &'static tokio::sync::Mutex<()> {
        static G: OnceLock<tokio::sync::Mutex<()>> = OnceLock::new();
        G.get_or_init(|| tokio::sync::Mutex::new(()))
    }

    #[tokio::test]
    async fn creates_once_with_the_expected_values_and_links_every_deal() {
        let _g = test_guard().lock().await;
        created_lock().lock().await.clear();
        let remote: Shared = Default::default();
        let service = JobReadService::for_test(serve(remote.clone()).await);
        let index = super::super::listings::upsert_test_support::row;
        super::super::listings::upsert_test_support::empty_index(&service).await;
        let (_, Json(first)) = create_inner(&service, "1", request(), true).await.unwrap();
        // 一覧の作り直しを待たずに、案件管理の一覧へ「公開前」のリクロジ求人として出る。
        assert_eq!(
            index(&service, "900").await,
            Some((
                "reclogi",
                String::new(),
                Some("掲載前".to_owned()),
                vec!["10".to_owned(), "20".to_owned()]
            ))
        );
        assert_eq!(first["status"], "created");
        assert_eq!(first["listing_id"], "900");
        assert_eq!(first["linked_deals"], 2);
        assert_eq!(first["unlinked_deals"], 0);
        let created = remote.lock().unwrap().created.clone();
        assert_eq!(created.len(), 1);
        let p = &created[0].1;
        // 入れる値
        assert_eq!(p["hs_name"], "新しい案件名");
        assert_eq!(p["shigotonaiyou"], "AIが書いた仕事内容");
        assert_eq!(p["hissushikaku_shousai"], "AIが書いた応募資格");
        assert_eq!(p["reclogi_moto_hr_kyuujin_id"], "1234567");
        assert_eq!(p["kyuujin_status"], "公開前");
        assert_eq!(p["hs_pipeline_stage"], "1343099546");
        // 原本から写す値
        assert_eq!(p["zhizhong"], "介護職");
        assert_eq!(p["kinmujikan"], "9:00-18:00");
        assert_eq!(p["hubspot_owner_id"], "777");
        assert_eq!(
            p["customer_sheet_url"],
            "https://docs.google.com/spreadsheets/d/x"
        );
        // 入れないもの
        for key in [
            "id_hrhakkaa",
            "id_shop_hrhakkaa",
            "id_reclogi",
            "hrh_kyuujinhyou_honbun",
        ] {
            assert!(p.get(key).is_none(), "{key} must not be set");
        }
        // 取引すべてに関連付け
        for deal in ["10", "20"] {
            assert_eq!(
                count(
                    &remote,
                    "PUT",
                    &format!("/crm/v4/objects/0-420/900/associations/default/deals/{deal}")
                ),
                1
            );
        }
    }

    #[tokio::test]
    async fn pressing_twice_creates_one_listing_even_while_search_has_not_caught_up() {
        let _g = test_guard().lock().await;
        created_lock().lock().await.clear();
        let remote: Shared = Default::default();
        let service = JobReadService::for_test(serve(remote.clone()).await);
        create_inner(&service, "1", request(), true).await.unwrap();
        let (_, Json(second)) = create_inner(&service, "1", request(), true).await.unwrap();
        assert_eq!(second["status"], "existing");
        assert_eq!(second["listing_id"], "900");
        assert_eq!(remote.lock().unwrap().created.len(), 1);
        assert_eq!(count(&remote, "POST", "/crm/v3/objects/0-420"), 1);
        // 足りない関連 (取引20) だけを付け直す。取引10はすでにある。
        assert_eq!(
            count(
                &remote,
                "PUT",
                "/crm/v4/objects/0-420/900/associations/default/deals/10"
            ),
            1
        );
        assert_eq!(
            count(
                &remote,
                "PUT",
                "/crm/v4/objects/0-420/900/associations/default/deals/20"
            ),
            2
        );
    }

    #[tokio::test]
    async fn two_simultaneous_presses_create_one_listing() {
        let _g = test_guard().lock().await;
        created_lock().lock().await.clear();
        let remote: Shared = Default::default();
        let service = JobReadService::for_test(serve(remote.clone()).await);
        let (a, b) = tokio::join!(
            create_inner(&service, "1", request(), true),
            create_inner(&service, "1", request(), true)
        );
        let mut statuses = vec![
            a.unwrap().1 .0["status"].as_str().unwrap().to_owned(),
            b.unwrap().1 .0["status"].as_str().unwrap().to_owned(),
        ];
        statuses.sort();
        assert_eq!(statuses, ["created", "existing"]);
        assert_eq!(remote.lock().unwrap().created.len(), 1);
    }

    #[tokio::test]
    async fn after_a_restart_the_search_finds_the_listing_and_nothing_new_is_created() {
        let _g = test_guard().lock().await;
        created_lock().lock().await.clear();
        let remote: Shared = Default::default();
        {
            let mut r = remote.lock().unwrap();
            r.created.push((
                "900".into(),
                json!({"reclogi_moto_hr_kyuujin_id":"1234567"}),
            ));
            r.search_sees_created = true;
        }
        let service = JobReadService::for_test(serve(remote.clone()).await);
        let (_, Json(v)) = create_inner(&service, "1", request(), true).await.unwrap();
        assert_eq!(v["status"], "existing");
        assert_eq!(count(&remote, "POST", "/crm/v3/objects/0-420"), 0);
    }

    #[tokio::test]
    async fn writes_disabled_or_bad_input_create_nothing() {
        let _g = test_guard().lock().await;
        created_lock().lock().await.clear();
        let remote: Shared = Default::default();
        let service = JobReadService::for_test(serve(remote.clone()).await);
        let (_, Json(v)) = create_inner(&service, "1", request(), false).await.unwrap();
        assert_eq!(v["status"], "skipped");
        assert!(remote.lock().unwrap().calls.is_empty());
        let mut no_title = request();
        no_title.generated.insert("案件名".into(), "  ".into());
        let e = create_inner(&service, "1", no_title, true)
            .await
            .unwrap_err();
        assert_eq!(e.1, "listing_title_missing");
        let mut unknown = request();
        unknown.generated.insert("勝手な列".into(), "x".into());
        let e = create_inner(&service, "1", unknown, true)
            .await
            .unwrap_err();
        assert_eq!(e.1, "invalid_generated_values");
        assert_eq!(remote.lock().unwrap().created.len(), 0);
    }

    #[tokio::test]
    async fn reclogi_id_is_saved_unless_another_listing_has_it() {
        let _g = test_guard().lock().await;
        let remote: Shared = Default::default();
        let base = serve(remote.clone()).await;
        let service = Arc::new(JobReadService::for_test(base));
        super::super::listings::upsert_test_support::empty_index(&service).await;
        let state = super::super::integration_tests::moc_state();
        let access = Access {
            service: Some(service.clone()),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
            snapshot_reader: None,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            moc_drive: Ok(None),
            moc_path: None,
        };
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        session
            .insert(crate::auth::SESSION_USER_KEY, "writer@example.test")
            .await
            .unwrap();
        session
            .insert(
                crate::auth::SESSION_LOGIN_METHOD_KEY,
                crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
            )
            .await
            .unwrap();
        let mut csrf = HeaderMap::new();
        csrf.insert("x-requested-with", "fetch".parse().unwrap());
        let call = |value: &'static str, headers: HeaderMap| {
            let (state, access, session) = (state.clone(), access.clone(), session.clone());
            async move {
                put_reclogi_id(
                    State(state),
                    Extension(access),
                    session,
                    Path("900".into()),
                    headers,
                    Json(IdRequest {
                        reclogi_id: value.into(),
                    }),
                )
                .await
                .map(|(_, Json(v))| v)
                .map_err(|ReadError(s, c)| (s, c))
            }
        };
        // 入力の誤り・CSRFなしは何も書かない
        assert_eq!(
            call("abc", csrf.clone()).await.unwrap_err().1,
            "reclogi_id_invalid"
        );
        assert_eq!(
            call("123", HeaderMap::new()).await.unwrap_err().1,
            "csrf_required"
        );
        assert_eq!(count(&remote, "PATCH", "/crm/v3/objects/0-420/900"), 0);
        // 別の求人が同じIDを持っている
        remote.lock().unwrap().id_reclogi_holder = Some("555".into());
        assert_eq!(
            call("424242", csrf.clone()).await.unwrap_err(),
            (StatusCode::CONFLICT, "reclogi_id_in_use")
        );
        assert_eq!(count(&remote, "PATCH", "/crm/v3/objects/0-420/900"), 0);
        // 空いていれば保存できる
        remote.lock().unwrap().id_reclogi_holder = None;
        let saved = call("424242", csrf.clone()).await.unwrap();
        assert_eq!(saved["reclogi_id"], "424242");
        let patch = remote
            .lock()
            .unwrap()
            .calls
            .iter()
            .find(|(m, _, _)| m == "PATCH")
            .map(|(_, _, b)| b.clone())
            .unwrap();
        assert_eq!(patch, json!({"properties":{"id_reclogi":"424242"}}));
        // 一覧の行もリクロジの求人IDの行になる。
        let row = super::super::listings::upsert_test_support::row(&service, "900")
            .await
            .unwrap();
        assert_eq!((row.0, row.1.as_str()), ("reclogi", "424242"));
        // 入力済みの求人は上書きしない
        assert_eq!(
            call("999", csrf).await.unwrap_err(),
            (StatusCode::CONFLICT, "reclogi_id_already_set")
        );
        assert_eq!(count(&remote, "PATCH", "/crm/v3/objects/0-420/900"), 1);
    }
}
