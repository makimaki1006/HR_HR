//! 応募 (0-421) の読み取り・保存。正本と業務履歴は HubSpot。
#[path = "applicant_write.rs"]
mod editing;
use super::{
    queue_cache::TtlCache,
    rbac,
    routes::{error_json, hubspot_error_response, CrmCtx},
};
use crate::{
    hubspot::{HubSpotClient, HubSpotError},
    AppState,
};
use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
pub use editing::ApplicantEditField;
pub(super) use editing::{build_write_step, WRITE_SCOPE_MESSAGE};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{sync::Arc, time::Duration};
use tower_sessions::Session;
use ts_rs::TS;

const OBJECT: &str = "0-421";
pub const PIPELINE: &str = "t_3c8f8179ebedb62c890f1cf2c73ff9c8";
pub const OPEN_STAGES: [&str; 8] = [
    "1343099551",
    "1431582290",
    "1431574804",
    "1431574805",
    "1431580509",
    "1431576654",
    "1431589173",
    "1431589174",
];
const PAGE: usize = 30;
const QUEUE_TTL: Duration = Duration::from_secs(30);
const DETAIL_TTL: Duration = Duration::from_secs(60);
const DEFINITIONS_TTL: Duration = Duration::from_secs(6 * 3600);
const SCOPE_MESSAGE: &str = "HubSpot の鍵に応募を読む権限がありません";
// 項目名の出どころ: handlers/job_copy_live/phone_hearing.rs の FIELDS (相手の画面とは独立)。
const HEARING: &[&str] = &[
    "hoyuushikaku_hiaringu",
    "kanrengyoumukeiken",
    "shuugyoujoukyou",
    "ichijitaiou_shuugyou_joukyou",
    "genshokumaeshokukaranotenshokuriyuu",
    "oubodouki",
    "kinmujoukennoyuusenjuni",
    "oubonoheiganjoukyou",
    "kyuujinkensakujouken",
    "shigotosagashinohanikoutsuushudan",
    "nyuushakiboujiki",
    "ichijitaiou_nyuusha_kibou_jiki",
    "ichijitaiou_azukatta_shitsumon",
    "ichijitaiou_jitai_riyuu",
    "ichijitaiou_shokan_memo",
    "tokukijikou",
    "bikou_hiaringu",
    "ichijitaioujisshinaiyou",
    "taioushubetsu",
    "yakusokukadennichiji",
    "renrakukakuninmokuteki",
    "kokyakushiitotenkijoukyou",
    "bposagyousuteetasu",
];
const RULES: &[&str] = &[
    "anmokuchi_shodoutaiou",
    "anmokuchi_taiouhouhou",
    "anmokuchi_taiouhouhousonotashousai",
    "anmokuchi_ashigirijouken",
    "anmokuchi_shikakuhoyuukakunin",
    "anmokuchi_keikenumukakunin",
    "anmokuchi_mensetsujijisanbutsu",
    "anmokuchi_oubohoukokusaki",
];
const QUEUE_PROPERTIES: &[&str] = &[
    "denwabangou",
    "ouboshashimei",
    "ouboshashimei_kana",
    "oubosaki_torihiki_name",
    "yingmuri",
    "hs_createdate",
    "hs_pipeline_stage",
    "yakusokukadennichiji",
    "oubobaitaimei",
];

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantField {
    pub label: String,
    pub value: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantSection {
    pub title: String,
    pub fields: Vec<ApplicantField>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantLink {
    pub label: String,
    pub url: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantRow {
    pub id: String,
    pub name: String,
    pub phone: String,
    pub deal: String,
    pub job: String,
    pub date: String,
    pub stage: String,
    pub stage_id: String,
    pub appointment: String,
    pub media: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantStage {
    pub id: String,
    pub label: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantPipeline {
    pub id: String,
    pub label: String,
    pub stages: Vec<ApplicantStage>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantQueue {
    pub items: Vec<ApplicantRow>,
    pub total: u32,
    pub next_cursor: Option<String>,
    pub pipelines: Vec<ApplicantPipeline>,
    pub media_options: Vec<ApplicantStage>,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantNote {
    pub date: String,
    pub html: String,
}
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct ApplicantDetail {
    #[serde(default)]
    pub values: std::collections::BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub editable: Vec<ApplicantEditField>,
    #[serde(default)]
    pub write_pipeline: Option<ApplicantPipeline>,
    #[serde(default)]
    pub writes_enabled: bool,
    pub row: ApplicantRow,
    pub sections: Vec<ApplicantSection>,
    pub phone: String,
    pub stop_reason: Option<String>,
    pub deep_link: String,
    pub jobs: Vec<ApplicantLink>,
    pub notes: Vec<ApplicantNote>,
    pub posting_url: Option<String>,
    pub jobcopy_url: Option<String>,
    pub sheet_url: Option<String>,
    pub sheet_embed: bool,
}

pub(super) struct ApplicantsState {
    definitions: TtlCache<(), Value>,
    definition_lock: tokio::sync::Mutex<()>,
    queues: TtlCache<String, ApplicantQueue>,
    details: TtlCache<String, ApplicantDetail>,
}
impl Default for ApplicantsState {
    fn default() -> Self {
        Self {
            definitions: TtlCache::new(DEFINITIONS_TTL, 1),
            definition_lock: tokio::sync::Mutex::new(()),
            queues: TtlCache::new(QUEUE_TTL, 500),
            details: TtlCache::new(DETAIL_TTL, 500),
        }
    }
}
async fn raw(client: &HubSpotClient, path: &str) -> Result<Value, HubSpotError> {
    let r = client.read_raw(false, path, &[], None).await?;
    match r.status {
        200..=299 => r
            .body
            .ok_or_else(|| HubSpotError::Decode("empty applicant response".into())),
        401 | 403 => Err(HubSpotError::Auth { status: r.status }),
        404 => Err(HubSpotError::NotFound),
        429 => Err(HubSpotError::RateLimited),
        s => Err(HubSpotError::Upstream { status: s }),
    }
}
impl ApplicantsState {
    pub(super) fn invalidate(&self, _id: &str) {
        self.details.clear();
        self.queues.clear();
    }
    async fn definitions(&self, client: &HubSpotClient) -> Result<Value, HubSpotError> {
        if let Some(v) = self.definitions.get(&()) {
            return Ok(v);
        }
        let _lock = self.definition_lock.lock().await;
        if let Some(v) = self.definitions.get(&()) {
            return Ok(v);
        }
        let (mut props, pipelines) = tokio::try_join!(
            raw(client, "/crm/v3/properties/0-421"),
            raw(client, "/crm/v3/pipelines/0-421")
        )?;
        if !props["results"].is_array() || !pipelines["results"].is_array() {
            return Err(HubSpotError::Decode("applicant definitions".into()));
        }
        for def in props["results"].as_array_mut().unwrap() {
            let name = text(def, "name");
            if let Some(label) = super::label_ja::property_label(OBJECT, &name, def) {
                def["label"] = json!(label);
            }
        }
        let value = json!({"properties":props["results"], "pipelines":pipelines["results"]});
        self.definitions.insert((), value.clone());
        Ok(value)
    }
}
fn values(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}
fn text(v: &Value, name: &str) -> String {
    v[name].as_str().unwrap_or("").trim().to_owned()
}
fn display(defs: &Value, name: &str, raw: &str) -> String {
    let prop = values(&defs["properties"]).find(|p| p["name"] == name);
    if name == "hs_pipeline_stage" {
        return values(&defs["pipelines"])
            .flat_map(|p| values(&p["stages"]))
            .find(|s| s["id"] == raw)
            .map(|s| text(s, "label"))
            .unwrap_or_else(|| "ステージを確認できません".into());
    }
    if let Some(p) = prop.filter(|p| p["type"] == "enumeration") {
        return raw
            .split(';')
            .map(|s| {
                values(&p["options"])
                    .find(|o| o["value"] == s)
                    .map(|o| text(o, "label"))
                    .unwrap_or_else(|| "選択内容を確認できません".into())
            })
            .collect::<Vec<_>>()
            .join("・");
    }
    raw.into()
}
fn row(v: &Value, defs: &Value) -> ApplicantRow {
    let p = &v["properties"];
    let get = |n: &str| text(p, n);
    let fallback = |a, b| {
        let s = get(a);
        if s.is_empty() {
            get(b)
        } else {
            s
        }
    };
    ApplicantRow {
        id: text(v, "id"),
        name: fallback("ouboshashimei", "ouboshashimei_kana"),
        phone: get("denwabangou"),
        deal: get("oubosaki_torihiki_name"),
        job: values(&defs["properties"])
            .filter(|d| {
                text(d, "name").starts_with("oubosaki_")
                    && (text(d, "label").contains("タイトル")
                        || text(d, "label").contains("求人名"))
            })
            .map(|d| get(&text(d, "name")))
            .find(|s| !s.is_empty())
            .unwrap_or_default(),
        date: fallback("yingmuri", "hs_createdate"),
        stage: display(defs, "hs_pipeline_stage", &get("hs_pipeline_stage")),
        stage_id: get("hs_pipeline_stage"),
        appointment: get("yakusokukadennichiji"),
        media: display(defs, "oubobaitaimei", &get("oubobaitaimei")),
    }
}
fn pipelines(defs: &Value) -> Vec<ApplicantPipeline> {
    values(&defs["pipelines"])
        .filter_map(|p| {
            let stages: Vec<_> = values(&p["stages"])
                .filter(|s| OPEN_STAGES.contains(&text(s, "id").as_str()))
                .map(|s| ApplicantStage {
                    id: text(s, "id"),
                    label: text(s, "label"),
                })
                .collect();
            (!stages.is_empty()).then(|| ApplicantPipeline {
                id: text(p, "id"),
                label: text(p, "label"),
                stages,
            })
        })
        .collect()
}
fn options(defs: &Value) -> Vec<ApplicantStage> {
    values(&defs["properties"])
        .find(|p| p["name"] == "oubobaitaimei")
        .into_iter()
        .flat_map(|p| values(&p["options"]))
        .map(|o| ApplicantStage {
            id: text(o, "value"),
            label: text(o, "label"),
        })
        .collect()
}
#[derive(Default, Deserialize)]
pub(super) struct QueueQuery {
    stages: Option<String>,
    media: Option<String>,
    cursor: Option<String>,
    fresh: Option<u8>,
}
#[derive(Default, Deserialize)]
pub(super) struct DetailQuery {
    fresh: Option<u8>,
}
#[derive(Clone, Serialize, Deserialize)]
struct Position {
    now: i64,
    phase: u32,
    due: u64,
    dated: u64,
    undated: u64,
}
fn base_filters(stages: &[String], media: &Option<String>) -> Vec<Value> {
    let mut f = vec![
        json!({"propertyName":"hs_pipeline_stage","operator":"IN","values":stages}),
        json!({"propertyName":"ichijitaiounoumu","operator":"EQ","value":"必要"}),
    ];
    if let Some(m) = media {
        f.push(json!({"propertyName":"oubobaitaimei","operator":"EQ","value":m}));
    }
    f
}
fn search_body(base: &[Value], pos: &Position, kind: &str, properties: &[String]) -> Value {
    let due = |op| json!({"propertyName":"yakusokukadennichiji","operator":op,"value":pos.now.to_string()});
    let mut groups = Vec::new();
    let mut branches: Vec<Vec<Value>> = if kind == "due" {
        vec![vec![due("LTE")]]
    } else if kind == "total" {
        vec![vec![]]
    } else {
        vec![
            vec![due("GT")],
            vec![json!({"propertyName":"yakusokukadennichiji","operator":"NOT_HAS_PROPERTY"})],
        ]
    };
    for extra in &mut branches {
        if kind == "dated" || kind == "undated" {
            extra.push(json!({"propertyName":"yingmuri","operator":if kind=="dated" {"HAS_PROPERTY"} else {"NOT_HAS_PROPERTY"}}));
        }
        let mut filters = base.to_vec();
        filters.extend(extra.clone());
        groups.push(json!({"filters":filters}));
    }
    let (sort, direction, after) = match kind {
        "due" => ("yakusokukadennichiji", "ASCENDING", pos.due),
        "dated" => ("yingmuri", "DESCENDING", pos.dated),
        _ => ("hs_createdate", "DESCENDING", pos.undated),
    };
    let mut body = json!({"filterGroups":groups,"sorts":[{"propertyName":sort,"direction":direction}],"properties":properties,"limit":if kind=="total" {1} else {PAGE}});
    if after > 0 {
        body["after"] = json!(after.to_string());
    }
    body
}
fn applicant_error(e: &HubSpotError) -> Response {
    if matches!(e, HubSpotError::Auth { status: 403 }) {
        return (
            StatusCode::FORBIDDEN,
            Json(json!({"error_kind":"applicants_scope","message":SCOPE_MESSAGE})),
        )
            .into_response();
    }
    hubspot_error_response(e)
}
fn link(object: &str, id: &str) -> String {
    format!(
        "https://app.hubspot.com/contacts/{}/record/{object}/{id}/",
        crate::hubspot::deep_link::hubspot_portal_id()
    )
}

pub(super) async fn queue(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    session: Session,
    Query(q): Query<QueueQuery>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let mut stages: Vec<String> = q
        .stages
        .as_deref()
        .map(|s| s.split(',').map(str::to_owned).collect())
        .unwrap_or_else(|| OPEN_STAGES.iter().map(|s| s.to_string()).collect());
    stages.sort();
    stages.dedup();
    if q.cursor.as_ref().is_some_and(|c| c.len() > 4096)
        || stages.is_empty()
        || stages.iter().any(|s| !OPEN_STAGES.contains(&s.as_str()))
        || q.media.as_ref().is_some_and(|m| m.len() > 300)
    {
        return error_json(StatusCode::BAD_REQUEST, "invalid_filter");
    }
    let binding = json!(["applicants", principal.email, stages, q.media]).to_string();
    let mut pos = Position {
        now: chrono::Utc::now().timestamp_millis(),
        phase: 0,
        due: 0,
        dated: 0,
        undated: 0,
    };
    if let Some(cursor) = &q.cursor {
        let parsed = cursor.split_once('~').and_then(|(payload, sig)| {
            ctx.queue
                .verify_cursor(sig, &format!("{binding}:{payload}"))?;
            serde_json::from_slice::<Position>(&URL_SAFE_NO_PAD.decode(payload).ok()?).ok()
        });
        match parsed {
            Some(p) if p.phase <= 1 && p.due < 10000 && p.dated < 10000 && p.undated < 10000 => {
                pos = p
            }
            _ => return error_json(StatusCode::BAD_REQUEST, "cursor_mismatch"),
        }
    }
    let key = if q.cursor.is_none() {
        format!("{binding}:first")
    } else {
        json!([binding, pos]).to_string()
    };
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let key = format!("{}:{key}", client.applicant_write_revision());
    if q.fresh != Some(1) {
        if let Some(body) = ctx.applicants.queues.get(&key) {
            return Json(body).into_response();
        }
    }
    let work = async {
        let _slot = tokio::time::timeout(Duration::from_secs(5), ctx.queue_slots.acquire())
            .await
            .map_err(|_| HubSpotError::Busy)?
            .map_err(|_| HubSpotError::Busy)?;
        let defs = ctx.applicants.definitions(&client).await?;
        let mut properties: Vec<String> = QUEUE_PROPERTIES.iter().map(|s| s.to_string()).collect();
        properties.extend(
            values(&defs["properties"])
                .filter(|p| {
                    text(p, "name").starts_with("oubosaki_")
                        && (text(p, "label").contains("タイトル")
                            || text(p, "label").contains("求人名"))
                })
                .map(|p| text(p, "name")),
        );
        let filters = base_filters(&stages, &q.media);
        let total = client
            .search(OBJECT, search_body(&filters, &pos, "total", &properties))
            .await?["total"]
            .as_u64()
            .ok_or_else(|| HubSpotError::Decode("applicant total".into()))?
            as u32;
        let mut items = Vec::new();
        let mut more = false;
        if pos.phase == 0 {
            let page = client
                .search(OBJECT, search_body(&filters, &pos, "due", &properties))
                .await?;
            items.extend(values(&page["results"]).map(|r| row(r, &defs)));
            if let Some(after) = page["paging"]["next"]["after"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .or_else(|| page["paging"]["next"]["after"].as_u64())
            {
                pos.due = after;
            } else {
                pos.phase = 1;
            }
            more = pos.phase == 0 || total > page["total"].as_u64().unwrap_or(0) as u32;
        }
        if items.is_empty() && pos.phase == 1 && total > 0 {
            // 約束済みが 0 件なら同じ要求で後半へ進む（空の一覧を先に見せない）。
            // 応募日の無い行は作成日で並べる。二つの Search をマージしてページ境界でも順を保つ。
            let dated = client
                .search(OBJECT, search_body(&filters, &pos, "dated", &properties))
                .await?;
            let undated = client
                .search(OBJECT, search_body(&filters, &pos, "undated", &properties))
                .await?;
            let a: Vec<_> = values(&dated["results"]).collect();
            let b: Vec<_> = values(&undated["results"]).collect();
            let (mut i, mut j) = (0, 0);
            while items.len() < PAGE && (i < a.len() || j < b.len()) {
                let take_a = j == b.len()
                    || (i < a.len()
                        && date_ms(&row(a[i], &defs).date) >= date_ms(&row(b[j], &defs).date));
                let v = if take_a {
                    let v = a[i];
                    i += 1;
                    v
                } else {
                    let v = b[j];
                    j += 1;
                    v
                };
                items.push(row(v, &defs));
            }
            pos.dated += i as u64;
            pos.undated += j as u64;
            more = i < a.len()
                || j < b.len()
                || !dated["paging"]["next"].is_null()
                || !undated["paging"]["next"].is_null();
        }
        let next_cursor = if more {
            let p = URL_SAFE_NO_PAD.encode(serde_json::to_vec(&pos).unwrap());
            Some(format!(
                "{p}~{}",
                ctx.queue.sign_cursor(&format!("{binding}:{p}"), 0, None)
            ))
        } else {
            None
        };
        Ok::<_, HubSpotError>(ApplicantQueue {
            items,
            total,
            next_cursor,
            pipelines: pipelines(&defs),
            media_options: options(&defs),
        })
    };
    match tokio::time::timeout(Duration::from_secs(20), work).await {
        Ok(Ok(body)) => {
            ctx.applicants.queues.insert(key, body.clone());
            Json(body).into_response()
        }
        Ok(Err(e)) => applicant_error(&e),
        Err(_) => super::routes::timeout_response(),
    }
}
fn date_ms(s: &str) -> i64 {
    s.parse()
        .ok()
        .or_else(|| {
            chrono::DateTime::parse_from_rfc3339(s)
                .ok()
                .map(|d| d.timestamp_millis())
        })
        .or_else(|| {
            chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                .ok()
                .and_then(|d| d.and_hms_opt(0, 0, 0))
                .map(|d| d.and_utc().timestamp_millis())
        })
        .unwrap_or(0)
}
fn sheet_url(p: &Value, jobs: &[crate::hubspot::HubSpotRecord]) -> Option<String> {
    let url = text(p, "customer_sheet_url_synced");
    if !url.is_empty() {
        Some(url)
    } else {
        jobs.iter()
            .filter_map(|j| {
                j.properties
                    .get("customer_sheet_url")
                    .and_then(|v| v.as_deref())
            })
            .find(|s| !s.trim().is_empty())
            .map(|s| s.trim().to_string())
    }
}
fn http_url(raw: &str) -> Option<String> {
    let raw = raw.trim();
    let url = reqwest::Url::parse(raw).ok()?;
    (matches!(url.scheme(), "http" | "https") && url.host_str().is_some()).then(|| raw.to_owned())
}
fn job_url(jobs: &[crate::hubspot::HubSpotRecord], name: &str) -> Option<String> {
    jobs.iter().find_map(|j| {
        j.properties
            .get(name)
            .and_then(|v| v.as_deref())
            .and_then(http_url)
    })
}
fn posting_url(p: &Value, jobs: &[crate::hubspot::HubSpotRecord]) -> Option<String> {
    http_url(&text(p, "oubosaki_kyuujin_url"))
        .or_else(|| job_url(jobs, "url_hrhakkaa"))
        .or_else(|| job_url(jobs, "url_airwork"))
}
fn sheet_embeddable(url: Option<&str>) -> bool {
    url.is_some_and(|s| s.starts_with("https://docs.google.com/spreadsheets/"))
}
fn sections(p: &Value, defs: &Value) -> Vec<ApplicantSection> {
    let mut groups: Vec<ApplicantSection> = [
        "応募者の情報",
        "この求人の対応ルール",
        "応募先",
        "応募の内容",
        "ヒアリングと結果",
    ]
    .iter()
    .map(|s| ApplicantSection {
        title: s.to_string(),
        fields: vec![],
    })
    .collect();
    for def in values(&defs["properties"]) {
        let name = text(def, "name");
        let value = text(p, &name);
        let label = text(def, "label");
        // 見出しに出す値は項目カードへ重ねて出さない（内部名で判定する）。
        if value.is_empty()
            || label.is_empty()
            || [
                "customer_sheet_url_synced",
                "ouboshashimei",
                "denwabangou",
                "hs_pipeline",
                "hs_pipeline_stage",
                "oubobaitaimei",
                "yingmuri",
                "hs_createdate",
            ]
            .contains(&name.as_str())
        {
            continue;
        }
        let idx = if RULES.contains(&name.as_str()) {
            1
        } else if HEARING.contains(&name.as_str()) {
            4
        } else if name.starts_with("oubosaki_") {
            2
        } else if name.starts_with("hr_")
            || [
                "mensetsukanounichiji",
                "renrakukanouyoubijikantai",
                "oubokyuujinmemo",
            ]
            .contains(&name.as_str())
        {
            3
        } else if [
            "ouboshashimei_kana",
            "nenrei",
            "seibetsu",
            "denwabangou",
            "meeruadoresu",
            "email",
            "yingmuri",
            "oubobaitaimei",
            "hs_pipeline_stage",
        ]
        .contains(&name.as_str())
            || label.contains("メール")
        {
            0
        } else {
            continue;
        };
        groups[idx].fields.push(ApplicantField {
            label: match name.as_str() {
                "shuugyoujoukyou" => "以前の就業状況".into(),
                "nyuushakiboujiki" => "以前の入社希望時期".into(),
                _ => label,
            },
            value: display(defs, &name, &value),
        });
    }
    groups
}
pub(super) async fn detail(
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    session: Session,
    Path(id): Path<String>,
    Query(q): Query<DetailQuery>,
) -> Response {
    if let Err(e) = rbac::authorize(&session, &state, &ctx.access, None).await {
        return e.into_response();
    }
    if !super::routes::is_valid_id(&id) {
        return error_json(StatusCode::BAD_REQUEST, "invalid_id");
    }
    let Some(client) = state.crm_hubspot() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let cache_key = format!("{}:{id}", client.applicant_write_revision());
    if q.fresh != Some(1) {
        if let Some(v) = ctx.applicants.details.get(&cache_key) {
            return Json(v).into_response();
        }
    }
    let work = async {
        let _slot = tokio::time::timeout(Duration::from_secs(5), ctx.read_slots.acquire())
            .await
            .map_err(|_| HubSpotError::Busy)?
            .map_err(|_| HubSpotError::Busy)?;
        let defs = ctx.applicants.definitions(&client).await?;
        let mut props: Vec<String> = values(&defs["properties"])
            .map(|d| text(d, "name"))
            .filter(|s| !s.is_empty())
            .collect();
        if !props.iter().any(|p| p == "oubosaki_kyuujin_url") {
            props.push("oubosaki_kyuujin_url".into());
        }
        let names: Vec<&str> = props.iter().map(String::as_str).collect();
        // 本体、求人票、メモは独立。関連 ID → batch read の依存だけを保つ。
        let (rows, jobs, notes) = tokio::try_join!(
            client.batch_read(OBJECT, std::slice::from_ref(&id), &names),
            async {
                let ids = all_associations(&client, &id, "0-420").await?;
                client
                    .batch_read(
                        "0-420",
                        &ids,
                        &[
                            "hs_name",
                            "customer_sheet_url",
                            "url_hrhakkaa",
                            "url_airwork",
                            "jobcopy_app_url",
                        ],
                    )
                    .await
            },
            async {
                let ids = all_associations(&client, &id, "notes").await?;
                client
                    .batch_read("notes", &ids, &["hs_timestamp", "hs_note_body"])
                    .await
            }
        )?;
        let record = rows.first().ok_or(HubSpotError::NotFound)?;
        let v = serde_json::to_value(record)
            .map_err(|_| HubSpotError::Decode("applicant record".into()))?;
        let p = &v["properties"];
        let sheet_url = sheet_url(p, &jobs);
        let mut notes: Vec<_> = notes
            .into_iter()
            .map(|n| ApplicantNote {
                date: text(&json!(n.properties), "hs_timestamp"),
                html: text(&json!(n.properties), "hs_note_body"),
            })
            .collect();
        notes.sort_by_key(|n| std::cmp::Reverse(date_ms(&n.date)));
        notes.truncate(5);
        let stop_reason = values(&defs["properties"])
            .filter(|d| {
                let l = text(d, "label");
                l.contains("架電禁止") || l.contains("ブロック理由")
            })
            .find_map(|d| {
                let name = text(d, "name");
                let value = text(p, &name);
                (!value.is_empty() && value != "false").then(|| display(&defs, &name, &value))
            });
        let editable = editing::editable(&defs);
        Ok::<_, HubSpotError>(ApplicantDetail {
            values: record
                .properties
                .iter()
                .filter(|(name, _)| {
                    editable.iter().any(|f| &f.name == *name)
                        || ["hs_pipeline", "hs_pipeline_stage"].contains(&name.as_str())
                })
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            editable,
            write_pipeline: editing::write_pipeline(&defs).map(|p| ApplicantPipeline {
                id: text(p, "id"),
                label: text(p, "label"),
                stages: values(&p["stages"])
                    .map(|s| ApplicantStage {
                        id: text(s, "id"),
                        label: text(s, "label"),
                    })
                    .collect(),
            }),
            writes_enabled: ctx.write.enabled,
            row: row(&v, &defs),
            sections: sections(p, &defs),
            phone: text(p, "denwabangou"),
            stop_reason,
            deep_link: link(OBJECT, &id),
            jobs: jobs
                .iter()
                .map(|j| ApplicantLink {
                    label: j
                        .properties
                        .get("hs_name")
                        .or_else(|| j.properties.get("name"))
                        .and_then(|v| v.as_deref())
                        .unwrap_or("応募先求人")
                        .into(),
                    url: link("0-420", &j.id),
                })
                .collect(),
            posting_url: posting_url(p, &jobs),
            jobcopy_url: job_url(&jobs, "jobcopy_app_url"),
            notes,
            sheet_embed: sheet_embeddable(sheet_url.as_deref()),
            sheet_url,
        })
    };
    match tokio::time::timeout(Duration::from_secs(20), work).await {
        Ok(Ok(v)) => {
            ctx.applicants.details.insert(cache_key, v.clone());
            Json(v).into_response()
        }
        Ok(Err(e)) => applicant_error(&e),
        Err(_) => super::routes::timeout_response(),
    }
}
async fn all_associations(
    client: &HubSpotClient,
    id: &str,
    to: &str,
) -> Result<Vec<String>, HubSpotError> {
    let mut ids = Vec::new();
    let mut after: Option<String> = None;
    loop {
        let mut query = vec![("limit", "500".to_string())];
        if let Some(a) = &after {
            query.push(("after", a.clone()));
        }
        let reply = client
            .read_raw(
                false,
                &format!("/crm/v4/objects/{OBJECT}/{id}/associations/{to}"),
                &query,
                None,
            )
            .await?;
        if reply.status != 200 {
            return Err(match reply.status {
                401 | 403 => HubSpotError::Auth {
                    status: reply.status,
                },
                _ => HubSpotError::Upstream {
                    status: reply.status,
                },
            });
        }
        let page = reply
            .body
            .ok_or_else(|| HubSpotError::Decode("applicant associations".into()))?;
        ids.extend(values(&page["results"]).filter_map(|r| {
            r["toObjectId"]
                .as_u64()
                .map(|n| n.to_string())
                .or_else(|| r["toObjectId"].as_str().map(str::to_owned))
        }));
        let next = page["paging"]["next"]["after"].as_str().map(str::to_owned);
        if next.is_none() {
            break;
        }
        if next == after || ids.len() > 10000 {
            return Err(HubSpotError::Decode(
                "too many applicant associations".into(),
            ));
        }
        after = next;
    }
    Ok(ids)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn posting_and_jobcopy_urls_only_use_http_and_respect_priority() {
        let jobs = vec![crate::hubspot::HubSpotRecord {
            id: "80001".into(),
            created_at: None,
            updated_at: None,
            archived: false,
            properties: [
                (
                    "url_hrhakkaa".into(),
                    Some("https://www.example.com/hrhacker".into()),
                ),
                (
                    "url_airwork".into(),
                    Some("https://www.example.com/airwork".into()),
                ),
                (
                    "jobcopy_app_url".into(),
                    Some("https://www.example.com/app/job-copy/fictional".into()),
                ),
            ]
            .into_iter()
            .collect(),
        }];
        assert_eq!(
            posting_url(
                &json!({"oubosaki_kyuujin_url":" https://www.example.com/applicant "}),
                &jobs
            )
            .as_deref(),
            Some("https://www.example.com/applicant")
        );
        assert_eq!(
            posting_url(
                &json!({"oubosaki_kyuujin_url":"javascript:alert(1)"}),
                &jobs
            )
            .as_deref(),
            Some("https://www.example.com/hrhacker")
        );
        let mut fallback = jobs.clone();
        fallback[0]
            .properties
            .insert("url_hrhakkaa".into(), Some("data:text/html,no".into()));
        assert_eq!(
            posting_url(&json!({}), &fallback).as_deref(),
            Some("https://www.example.com/airwork")
        );
        assert_eq!(posting_url(&json!({}), &[]), None);
        assert_eq!(
            posting_url(&json!({"oubosaki_kyuujin_url":"ftp://example.com/a"}), &[]),
            None
        );
        assert_eq!(
            posting_url(
                &json!({"oubosaki_kyuujin_url":"http://www.example.com/a"}),
                &[]
            )
            .as_deref(),
            Some("http://www.example.com/a")
        );
        assert_eq!(
            job_url(&jobs, "jobcopy_app_url").as_deref(),
            Some("https://www.example.com/app/job-copy/fictional")
        );
        fallback[0]
            .properties
            .insert("jobcopy_app_url".into(), Some("javascript:alert(1)".into()));
        assert_eq!(job_url(&fallback, "jobcopy_app_url"), None);
        assert_eq!(job_url(&[], "jobcopy_app_url"), None);
    }
    #[test]
    fn search_body_omits_initial_after_and_sends_string_offsets() {
        let mut pos = Position {
            now: 1792281600000,
            phase: 0,
            due: 0,
            dated: 0,
            undated: 0,
        };
        let filters = base_filters(&["1343099551".into()], &None);
        let properties = vec!["ouboshashimei".to_owned()];
        let mut expected = json!({
            "filterGroups": [{"filters": [
                {"propertyName":"hs_pipeline_stage","operator":"IN","values":["1343099551"]},
                {"propertyName":"ichijitaiounoumu","operator":"EQ","value":"必要"},
                {"propertyName":"yakusokukadennichiji","operator":"LTE","value":"1792281600000"}
            ]}],
            "sorts": [{"propertyName":"yakusokukadennichiji","direction":"ASCENDING"}],
            "properties": ["ouboshashimei"], "limit":30
        });
        assert_eq!(search_body(&filters, &pos, "due", &properties), expected);
        pos.due = 30;
        expected["after"] = json!("30");
        assert_eq!(search_body(&filters, &pos, "due", &properties), expected);
        for kind in ["dated", "undated", "total"] {
            assert!(search_body(&filters, &pos, kind, &properties)
                .get("after")
                .is_none());
        }
        pos.dated = 15;
        pos.undated = 45;
        assert_eq!(
            search_body(&filters, &pos, "dated", &properties)["after"],
            "15"
        );
        assert_eq!(
            search_body(&filters, &pos, "undated", &properties)["after"],
            "45"
        );
    }

    #[test]
    fn empty_rules_and_sheet_selection_do_not_invent_values() {
        let definitions = json!({"properties":[{"name":"anmokuchi_shodoutaiou","label":"初動対応","type":"string"}]});
        assert!(
            sections(&json!({"anmokuchi_shodoutaiou":" "}), &definitions)[1]
                .fields
                .is_empty()
        );
        assert_eq!(sheet_url(&json!({}), &[]), None);
        assert!(!sheet_embeddable(Some(
            "https://docs.google.com.example.invalid/spreadsheets/a"
        )));
        assert!(!sheet_embeddable(Some(
            "http://docs.google.com/spreadsheets/a"
        )));
        assert!(sheet_embeddable(Some(
            "https://docs.google.com/spreadsheets/d/fake/edit"
        )));
    }
    #[test]
    fn applicant_caches_expire_and_have_the_requested_production_ttls() {
        assert_eq!(QUEUE_TTL.as_secs(), 30);
        assert_eq!(DETAIL_TTL.as_secs(), 60);
        assert_eq!(DEFINITIONS_TTL.as_secs(), 21600);
        let state = ApplicantsState::default();
        // 共通キャッシュの期限切れを短い時間で確かめる。
        let cache = TtlCache::new(Duration::from_millis(10), 500);
        cache.insert("100".to_owned(), "架空 はるか".to_owned());
        assert_eq!(cache.get(&"100".to_owned()).as_deref(), Some("架空 はるか"));
        std::thread::sleep(Duration::from_millis(20));
        assert_eq!(cache.get(&"100".to_owned()), None);
        assert!(state.queues.is_empty());
        assert!(state.details.is_empty());
    }
}
