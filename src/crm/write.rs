//! Headless CRM の書き込み (`PATCH /api/crm/deals/{id}`、`GET /api/crm/edit-schema`、`GET /api/crm/operations/{id}`)。
//!
//! 設計: `docs/architecture/headless-crm-design.md` §10〜§13、ADR-018。契約: 実装計画の「CRM write API contract」。
//!
//! ## 書けるもの
//! 「プロパティ」パネルに出る項目のうち HubSpot が API で書かせる項目 (`property_catalog` の `writable`)。
//! 読み取り専用・計算・`hs_*` / `hubspot_*`・「※編集不可」・BPO の取引 ID と HubSpot URL・`dealstage` / `pipeline`
//! (ステージは `stage` で移す) は書けない。**許可リストはサーバ側で毎回カタログに照らして確かめる**。
//!
//! ## 処理の順番 (順番に意味がある)
//! 1. CSRF (`X-Requested-With`) → 認可 (`rbac`) → 書き込みの栓 (`CRM_WRITES_ENABLED` / `CRM_WRITE_DEAL_ALLOWLIST`。
//!    閉じていれば 403 `writes_disabled` で **HubSpot を 1 回も呼ばない**)
//! 2. 同じ `operation_id` が台帳にあれば保存された結果を返す (二重に書かない)
//! 3. 項目名・値の型と選択肢・`base` の有無・ステージの検証 → 422
//! 4. 現在値の読み取り (1 オブジェクトにつき 1 回) → `base` と違えば 409 (**上書きしない**)、
//!    ステージ移動なら移動先の必須項目 (`stage_rules.json`) が空でないか → 422 `missing_required`
//! 5. 台帳 (監査 Turso) に `in_progress` で記録 → HubSpot に PATCH (オブジェクトごとに 1 回、案件はステージと同時)
//! 6. 結果: 成功 200 / 一時障害 202 (台帳を `pending` にして worker が再送) / 恒久エラー 422 など。
//!    成功したら案件のキャッシュ (ワークスペース・キュー) を捨て、誰が何を変えたかを `activity_logs` に残す。
//!
//! 台帳に記録できないとき (監査 DB 未接続・Turso 障害) は**書かずに** 503 `queue_unavailable` を返す
//! (監査できない書き込みは許さない)。

use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use axum::{
    body::Bytes,
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Notify;
use tower_sessions::Session;
use ts_rs::TS;

use super::pending::{self, Payload, StageMove, Step};
use super::property_catalog::{valid_property_name, CatalogEntry, CrmCatalogProperty};
use super::queue_pipelines::{find_pipeline, QUEUE_PIPELINES};
use super::rbac;
use super::routes::{
    hubspot_error_response, is_valid_id, timeout_response, CrmCtx, CRM_REQUEST_DEADLINE,
};
use super::stage_rules;
use crate::audit::AuditDb;
use crate::handlers::crm_metadata::CrmPropertyOption;
use crate::hubspot::{HubSpotClient, HubSpotError, RecordType};
use crate::AppState;

/// 1 回の書き込みで 1 オブジェクトに書ける項目数の上限
pub const MAX_SET_PROPS: usize = 100;
/// 文字列の最大長 (複数行 / 1 行)
pub const MAX_TEXT_LEN: usize = 65_536;
pub const MAX_LINE_LEN: usize = 1_024;

// ---------------------------------------------------------------------------
// 設定 (環境変数。ルーターを作るときに 1 回読む。テストでは差し替える)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct WriteConfig {
    /// `CRM_WRITES_ENABLED=1` で全案件の書き込みを開く
    pub enabled: bool,
    /// `CRM_WRITE_DEAL_ALLOWLIST` (カンマ区切りの案件 ID)。栓が閉じていても、ここにある案件だけ書ける
    pub allowlist: HashSet<String>,
    /// 再送待ちの上限 (`CRM_PENDING_MAX`)
    pub pending_max: i64,
}

impl WriteConfig {
    pub fn from_env() -> Self {
        let enabled = std::env::var("CRM_WRITES_ENABLED")
            .map(|v| matches!(v.trim(), "1" | "true" | "TRUE"))
            .unwrap_or(false);
        let allowlist = std::env::var("CRM_WRITE_DEAL_ALLOWLIST")
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();
        let pending_max = std::env::var("CRM_PENDING_MAX")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .filter(|n| *n > 0)
            .unwrap_or(pending::DEFAULT_PENDING_MAX);
        Self {
            enabled,
            allowlist,
            pending_max,
        }
    }

    /// 書き込みを許す案件か
    pub fn allows(&self, deal_id: &str) -> bool {
        self.enabled || self.allowlist.contains(deal_id)
    }
}

impl Default for WriteConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            allowlist: HashSet::new(),
            pending_max: pending::DEFAULT_PENDING_MAX,
        }
    }
}

// ---------------------------------------------------------------------------
// 契約の型 (ts-rs で frontend/src/generated/ に書き出す)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmEditableProperty {
    /// `deal` / `contact` / `company`
    pub object: String,
    pub name: String,
    pub label: String,
    #[serde(rename = "type")]
    #[ts(rename = "type")]
    pub property_type: String,
    pub field_type: String,
    pub options: Vec<CrmPropertyOption>,
    pub max_length: Option<u32>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmEditStage {
    pub id: String,
    pub label: String,
    /// 移動先で必須の項目
    pub required: Vec<String>,
    /// 移動先で表示される項目 (必須を含む)
    pub shown: Vec<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmEditPipelineStage {
    pub id: String,
    pub label: String,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmEditPipeline {
    pub id: String,
    pub label: String,
    pub stages: Vec<CrmEditPipelineStage>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmEditSchemaResponse {
    pub deal_id: String,
    pub pipeline_id: Option<String>,
    pub stage_id: Option<String>,
    pub editable: Vec<CrmEditableProperty>,
    pub stages: Vec<CrmEditStage>,
    pub pipelines: Vec<CrmEditPipeline>,
    pub writes_enabled: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmStageMove {
    pub pipeline_id: String,
    pub stage_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchObject {
    pub id: String,
    #[serde(default)]
    pub base: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub set: BTreeMap<String, Option<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchObjects {
    pub contact: Option<CrmPatchObject>,
    pub company: Option<CrmPatchObject>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmDealPatchRequest {
    pub operation_id: String,
    #[serde(default)]
    pub base: BTreeMap<String, Option<String>>,
    #[serde(default)]
    pub set: BTreeMap<String, Option<String>>,
    pub stage: Option<CrmStageMove>,
    pub objects: Option<CrmPatchObjects>,
}

/// 200 / 保存した
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchSaved {
    /// `saved`
    pub status: String,
    /// 案件の項目の保存後の値 (`dealstage` / `pipeline` も、移動したときは入る)
    pub values: BTreeMap<String, Option<String>>,
    /// 担当者・会社を同時に変えたときの保存後の値 (`contact` / `company`)。変えていなければ空
    #[serde(default)]
    pub objects_values: BTreeMap<String, BTreeMap<String, Option<String>>>,
    pub fetched_at: String,
}

/// 202 / HubSpot の一時障害で再送待ち
#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmPatchQueued {
    /// `queued`
    pub status: String,
    pub operation_id: String,
}

/// 409 / HubSpot の現在値が `base` と違う (上書きしない)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchConflict {
    /// `conflict`
    pub status: String,
    /// 競合したオブジェクト (`deal` / `contact` / `company`)
    pub object: String,
    /// 書こうとした項目の HubSpot の現在値
    pub current: BTreeMap<String, Option<String>>,
    /// 現在値が `base` とも書く値とも違う項目
    pub changed_by_hubspot: Vec<String>,
}

/// 422 / 検証エラー・必須項目の不足
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchInvalid {
    /// `invalid`
    pub status: String,
    pub errors: BTreeMap<String, String>,
    pub missing_required: Vec<String>,
}

/// `GET /api/crm/operations/{id}`
#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmOperationStatus {
    pub operation_id: String,
    /// `pending` / `retrying` / `saved` / `failed`
    pub status: String,
    #[ts(type = "number")]
    pub attempts: i64,
    pub last_error_code: Option<String>,
    pub next_retry_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmAdminOperation {
    pub operation_id: String,
    pub deal_id: String,
    pub operator_email: String,
    /// 変えようとした項目 (`dealstage` はステージ移動)
    pub props: Vec<String>,
    pub status: String,
    pub error: Option<String>,
    #[ts(type = "number")]
    pub attempts: i64,
    pub created_at: String,
    pub updated_at: String,
    pub next_retry_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct CrmAdminOperationsResponse {
    pub operations: Vec<CrmAdminOperation>,
}

// ---------------------------------------------------------------------------
// 応答の補助
// ---------------------------------------------------------------------------

fn err(status: StatusCode, kind: &str) -> Response {
    (status, Json(json!({ "error": kind, "error_kind": kind }))).into_response()
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

fn invalid(errors: BTreeMap<String, String>, missing: Vec<String>) -> (StatusCode, Value) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        json!(CrmPatchInvalid {
            status: "invalid".into(),
            errors,
            missing_required: missing,
        }),
    )
}

fn one_error(key: &str, msg: &str) -> Response {
    let (s, v) = invalid(BTreeMap::from([(key.to_string(), msg.to_string())]), vec![]);
    (s, Json(v)).into_response()
}

/// 書き込みルートにだけ掛ける CSRF 検査 (`/api/crm/*` は共有の auth_middleware の外なので、ここで同じ規則を適用する)
pub(super) async fn csrf_guard(
    request: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    if let Err(msg) = crate::check_csrf(&request) {
        return (StatusCode::FORBIDDEN, format!("Forbidden: {msg}")).into_response();
    }
    next.run(request).await
}

fn object_label(api: &str) -> &'static str {
    match api {
        "contacts" => "contact",
        "companies" => "company",
        _ => "deal",
    }
}

// ---------------------------------------------------------------------------
// 値の正規化と検証
// ---------------------------------------------------------------------------

/// 比較用に値をそろえる (空・空白だけ = 空。型ごとに表記ゆれを吸収)
pub fn norm(v: Option<&str>, ptype: &str) -> String {
    let t = v.unwrap_or("").trim();
    if t.is_empty() {
        return String::new();
    }
    match ptype {
        "number" => match t.parse::<f64>() {
            Ok(f) if f.is_finite() => format!("{f}"),
            _ => t.to_string(),
        },
        "date" => {
            if let Ok(d) = NaiveDate::parse_from_str(t, "%Y-%m-%d") {
                return d.format("%Y-%m-%d").to_string();
            }
            if let Some(ms) = digits_ms(t) {
                if let Some(dt) = chrono::DateTime::from_timestamp_millis(ms) {
                    return dt.format("%Y-%m-%d").to_string();
                }
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
                return dt.with_timezone(&Utc).format("%Y-%m-%d").to_string();
            }
            t.to_string()
        }
        "datetime" => {
            if let Some(ms) = digits_ms(t) {
                return ms.to_string();
            }
            if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(t) {
                return dt.timestamp_millis().to_string();
            }
            t.to_string()
        }
        "bool" => t.to_lowercase(),
        _ => t.to_string(),
    }
}

fn digits_ms(t: &str) -> Option<i64> {
    (t.len() >= 10 && t.bytes().all(|b| b.is_ascii_digit()))
        .then(|| t.parse::<i64>().ok())
        .flatten()
}

/// 値が項目の型・選択肢に合うか。`None` と空文字は消去として常に許す
pub fn validate_value(def: &CrmCatalogProperty, v: &Option<String>) -> Result<(), String> {
    let Some(s) = v.as_deref().map(str::trim).filter(|s| !s.is_empty()) else {
        return Ok(());
    };
    match def.property_type.as_str() {
        "enumeration" => {
            let allowed: HashSet<&str> = def.options.iter().map(|o| o.value.as_str()).collect();
            let parts: Vec<&str> = if def.field_type == "checkbox" {
                s.split(';').map(str::trim).collect()
            } else {
                vec![s]
            };
            for p in parts {
                if !allowed.contains(p) {
                    return Err(format!("選択肢にない値です: {p}"));
                }
            }
            Ok(())
        }
        "bool" => matches!(s, "true" | "false")
            .then_some(())
            .ok_or_else(|| "true か false で指定してください".to_string()),
        "number" => s
            .parse::<f64>()
            .ok()
            .filter(|f| f.is_finite())
            .map(|_| ())
            .ok_or_else(|| "数値で指定してください".to_string()),
        "date" => NaiveDate::parse_from_str(s, "%Y-%m-%d")
            .map(|_| ())
            .map_err(|_| "YYYY-MM-DD の日付で指定してください".to_string()),
        "datetime" => (chrono::DateTime::parse_from_rfc3339(s).is_ok() || digits_ms(s).is_some())
            .then_some(())
            .ok_or_else(|| "日時 (ISO 8601) で指定してください".to_string()),
        _ => {
            let max = max_length(def) as usize;
            if s.chars().count() > max {
                Err(format!("{max} 文字以内で入力してください"))
            } else {
                Ok(())
            }
        }
    }
}

fn max_length(def: &CrmCatalogProperty) -> u32 {
    if def.field_type == "textarea" {
        MAX_TEXT_LEN as u32
    } else {
        MAX_LINE_LEN as u32
    }
}

fn valid_operation_id(s: &str) -> bool {
    (8..=64).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

// ---------------------------------------------------------------------------
// GET /api/crm/edit-schema
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub(super) struct EditSchemaQuery {
    deal_id: Option<String>,
}

pub(super) async fn get_edit_schema(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Query(q): Query<EditSchemaQuery>,
) -> Response {
    if let Err(denied) =
        rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await
    {
        return denied.into_response();
    }
    let deal_id = q.deal_id.unwrap_or_default();
    if !is_valid_id(&deal_id) {
        return err(StatusCode::BAD_REQUEST, "invalid_id");
    }
    let Some(client) = state.hubspot.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let work = async {
        let (catalog, _) = ctx.catalog.get(&client).await?;
        let defs = ctx.queue.pipeline_defs(&client).await?;
        let deal = client
            .get_object("deals", &deal_id, &["pipeline", "dealstage"])
            .await?;
        Ok::<_, HubSpotError>((catalog, defs, deal))
    };
    let (catalog, defs, deal) = match tokio::time::timeout(CRM_REQUEST_DEADLINE, work).await {
        Err(_) => return timeout_response(),
        Ok(Err(e)) => return hubspot_error_response(&e),
        Ok(Ok(v)) => v,
    };
    let pipeline_id = deal.properties.get("pipeline").cloned().flatten();
    let stage_id = deal.properties.get("dealstage").cloned().flatten();
    let rules = stage_rules::rules();
    let mut editable = Vec::new();
    for api in ["deals", "contacts", "companies"] {
        for p in catalog.writable(api) {
            editable.push(CrmEditableProperty {
                object: object_label(api).to_string(),
                name: p.name.clone(),
                label: p.label.clone(),
                property_type: p.property_type.clone(),
                field_type: p.field_type.clone(),
                options: p.options.clone(),
                max_length: matches!(p.property_type.as_str(), "string" | "phone_number")
                    .then(|| max_length(p)),
            });
        }
    }
    let stages = defs
        .iter()
        .filter(|p| Some(&p.id) == pipeline_id.as_ref())
        .flat_map(|p| p.stages.iter())
        .map(|s| CrmEditStage {
            id: s.id.clone(),
            label: s.label.clone(),
            required: rules.required(&s.id),
            shown: rules.shown(&s.id),
        })
        .collect();
    let pipelines = QUEUE_PIPELINES
        .iter()
        .filter_map(|qp| defs.iter().find(|p| p.id == qp.id))
        .map(|p| CrmEditPipeline {
            id: p.id.clone(),
            label: p.label.clone(),
            stages: p
                .stages
                .iter()
                .map(|s| CrmEditPipelineStage {
                    id: s.id.clone(),
                    label: s.label.clone(),
                })
                .collect(),
        })
        .collect();
    Json(CrmEditSchemaResponse {
        writes_enabled: ctx.write.allows(&deal_id),
        deal_id,
        pipeline_id,
        stage_id,
        editable,
        stages,
        pipelines,
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// 実行エンジン (ハンドラと worker が共有)
// ---------------------------------------------------------------------------

/// 1 つの変更の記録 (監査用)
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub object: String,
    pub id: String,
    pub prop: String,
    pub before: Option<String>,
    pub after: Option<String>,
}

/// 実行を止めた理由
#[derive(Debug)]
pub enum Stop {
    Conflict {
        object: String,
        current: BTreeMap<String, Option<String>>,
        changed: Vec<String>,
    },
    Missing(Vec<String>),
    Transient(HubSpotError),
    Permanent(HubSpotError),
}

fn classify(e: HubSpotError) -> Stop {
    if e.is_transient() {
        Stop::Transient(e)
    } else {
        Stop::Permanent(e)
    }
}

/// 1 段の事前確認の結果
pub struct Prepared {
    cur: BTreeMap<String, Option<String>>,
    /// 書く値がすでに HubSpot にある (保存済みとして飛ばす)
    already: bool,
    /// 実際に動かすステージ (現在地と同じなら None)
    stage: Option<StageMove>,
}

fn step_ptype<'a>(step: &'a Step, prop: &str) -> &'a str {
    step.types.get(prop).map(String::as_str).unwrap_or("string")
}

/// 現在値を読み、競合 (`base` と違う) と移動先の必須項目の不足を確かめる。HubSpot の書き込みはしない
pub async fn prepare(client: &HubSpotClient, step: &Step) -> Result<Prepared, Stop> {
    let mut props: BTreeSet<String> = step.set.keys().cloned().collect();
    props.extend(step.base.keys().cloned());
    let required = step
        .stage
        .as_ref()
        .map(|s| stage_rules::rules().required(&s.stage_id))
        .unwrap_or_default();
    if step.stage.is_some() {
        props.insert("dealstage".into());
        props.insert("pipeline".into());
        props.extend(required.iter().cloned());
    }
    let names: Vec<&str> = props.iter().map(String::as_str).collect();
    let rec = client
        .get_object(&step.object, &step.id, &names)
        .await
        .map_err(classify)?;
    let cur: BTreeMap<String, Option<String>> = props
        .iter()
        .map(|k| (k.clone(), rec.properties.get(k).cloned().flatten()))
        .collect();
    let cur_of = |k: &str| cur.get(k).cloned().flatten();
    // 競合: 現在値が base とも書く値とも違う
    let mut changed = Vec::new();
    for (k, v) in &step.set {
        let t = step_ptype(step, k);
        let c = norm(cur_of(k).as_deref(), t);
        let b = norm(step.base.get(k).cloned().flatten().as_deref(), t);
        if c != b && c != norm(v.as_deref(), t) {
            changed.push(k.clone());
        }
    }
    if !changed.is_empty() {
        let current = step
            .set
            .keys()
            .map(|k| (k.clone(), cur_of(k)))
            .collect::<BTreeMap<_, _>>();
        return Err(Stop::Conflict {
            object: object_label(&step.object).to_string(),
            current,
            changed,
        });
    }
    let moving = step.stage.clone().filter(|s| {
        cur_of("dealstage").as_deref() != Some(s.stage_id.as_str())
            || cur_of("pipeline").as_deref() != Some(s.pipeline_id.as_str())
    });
    if moving.is_some() {
        let missing: Vec<String> = required
            .iter()
            .filter(|r| {
                let t = step_ptype(step, r);
                let eff = match step.set.get(r.as_str()) {
                    Some(v) => norm(v.as_deref(), t),
                    None => norm(cur_of(r).as_deref(), t),
                };
                eff.is_empty()
            })
            .cloned()
            .collect();
        if !missing.is_empty() {
            return Err(Stop::Missing(missing));
        }
    }
    let props_equal = step.set.iter().all(|(k, v)| {
        norm(cur_of(k).as_deref(), step_ptype(step, k)) == norm(v.as_deref(), step_ptype(step, k))
    });
    Ok(Prepared {
        already: props_equal && moving.is_none(),
        stage: moving,
        cur,
    })
}

/// 実行の途中経過 (成功した分)
#[derive(Default)]
pub struct Run {
    pub changes: Vec<Change>,
    pub values: BTreeMap<String, Option<String>>,
    pub other: BTreeMap<String, BTreeMap<String, Option<String>>>,
}

/// 事前確認を通った 1 段を HubSpot に書く (すでに同じ値なら書かずに完了扱い)
pub async fn send(
    client: &HubSpotClient,
    step: &mut Step,
    prep: &Prepared,
    run: &mut Run,
) -> Result<(), Stop> {
    let label = object_label(&step.object).to_string();
    let mut slot: BTreeMap<String, Option<String>> = BTreeMap::new();
    if prep.already {
        for k in step.set.keys() {
            slot.insert(k.clone(), prep.cur.get(k).cloned().flatten());
        }
    } else {
        let mut body: BTreeMap<String, Option<String>> = step.set.clone();
        if let Some(m) = &prep.stage {
            body.insert("dealstage".into(), Some(m.stage_id.clone()));
            body.insert("pipeline".into(), Some(m.pipeline_id.clone()));
        }
        let rec = client
            .patch_object(&step.object, &step.id, &body)
            .await
            .map_err(classify)?;
        for k in body.keys() {
            let v = rec
                .properties
                .get(k)
                .cloned()
                .unwrap_or_else(|| body[k].clone());
            slot.insert(k.clone(), v);
        }
        for (k, v) in &step.set {
            let t = step_ptype(step, k);
            let before = prep.cur.get(k).cloned().flatten();
            if norm(before.as_deref(), t) != norm(v.as_deref(), t) {
                run.changes.push(Change {
                    object: label.clone(),
                    id: step.id.clone(),
                    prop: k.clone(),
                    before,
                    after: v.clone(),
                });
            }
        }
        if let Some(m) = &prep.stage {
            run.changes.push(Change {
                object: label.clone(),
                id: step.id.clone(),
                prop: "dealstage".into(),
                before: prep.cur.get("dealstage").cloned().flatten(),
                after: Some(m.stage_id.clone()),
            });
            if prep.cur.get("pipeline").cloned().flatten().as_deref()
                != Some(m.pipeline_id.as_str())
            {
                run.changes.push(Change {
                    object: label.clone(),
                    id: step.id.clone(),
                    prop: "pipeline".into(),
                    before: prep.cur.get("pipeline").cloned().flatten(),
                    after: Some(m.pipeline_id.clone()),
                });
            }
        }
    }
    step.done = true;
    if label == "deal" {
        run.values.extend(slot);
    } else {
        run.other.insert(label, slot);
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// 監査
// ---------------------------------------------------------------------------

/// 誰が何を変えたかを `activity_logs` に 1 行で残す (時刻・操作者・案件・項目・変更前 → 後・結果)。
/// `account_id` が分からなければメールで代用する
#[allow(clippy::too_many_arguments)]
pub async fn audit_write(
    audit: &AuditDb,
    account_id: &str,
    session_id: &str,
    operator: &str,
    operation_id: &str,
    deal_id: &str,
    outcome: &str,
    changes: &[Change],
) {
    let meta = json!({
        "operation_id": operation_id,
        "operator": operator,
        "outcome": outcome,
        "at": now_rfc3339(),
        "changes": changes,
    })
    .to_string();
    let (a, acc, sid, did) = (
        audit.clone(),
        account_id.to_string(),
        session_id.to_string(),
        deal_id.to_string(),
    );
    let res = tokio::task::spawn_blocking(move || {
        crate::audit::dao::insert_activity(
            &a,
            &acc,
            &sid,
            "crm_write",
            "hubspot_deal",
            &did,
            &meta,
        );
    })
    .await;
    if let Err(e) = res {
        tracing::warn!("crm write audit join failed: {e}");
    }
}

/// 意図した変更 (競合・失敗の記録用。`before` は分かる範囲)
fn intended_changes(steps: &[Step], cur: Option<&BTreeMap<String, Option<String>>>) -> Vec<Change> {
    let mut v = Vec::new();
    for s in steps {
        for (k, a) in &s.set {
            let before = cur
                .and_then(|c| c.get(k).cloned().flatten())
                .or_else(|| s.base.get(k).cloned().flatten());
            v.push(Change {
                object: object_label(&s.object).to_string(),
                id: s.id.clone(),
                prop: k.clone(),
                before,
                after: a.clone(),
            });
        }
        if let Some(m) = &s.stage {
            v.push(Change {
                object: object_label(&s.object).to_string(),
                id: s.id.clone(),
                prop: "dealstage".into(),
                before: None,
                after: Some(m.stage_id.clone()),
            });
        }
    }
    v
}

fn invalidate_caches(ctx: &CrmCtx, deal_id: &str) {
    ctx.workspace_cache.invalidate_deal(deal_id);
    ctx.queue.invalidate_pages();
}

// ---------------------------------------------------------------------------
// PATCH /api/crm/deals/{id}
// ---------------------------------------------------------------------------

/// 台帳の行から、同じ `operation_id` の再送への返答を作る
fn replay_response(row: &pending::OpRow) -> Response {
    match row.status.as_str() {
        "saved" => match serde_json::from_str::<Value>(&row.result_json) {
            Ok(v) => (StatusCode::OK, Json(v)).into_response(),
            Err(_) => (StatusCode::OK, Json(json!({"status": "saved"}))).into_response(),
        },
        "pending" | "in_progress" => (
            StatusCode::ACCEPTED,
            Json(CrmPatchQueued {
                status: "queued".into(),
                operation_id: row.operation_id.clone(),
            }),
        )
            .into_response(),
        _ => {
            let status = StatusCode::from_u16(row.http_status as u16)
                .unwrap_or(StatusCode::UNPROCESSABLE_ENTITY);
            match serde_json::from_str::<Value>(&row.result_json) {
                Ok(v) => (status, Json(v)).into_response(),
                Err(_) => (
                    status,
                    Json(json!({"status": "failed", "error": row.last_error_code})),
                )
                    .into_response(),
            }
        }
    }
}

/// 検証して実行の段にする。失敗は (status, 本文)
async fn build_steps(
    client: &HubSpotClient,
    catalog: &CatalogEntry,
    deal_id: &str,
    req: &CrmDealPatchRequest,
) -> Result<Vec<Step>, (StatusCode, Value)> {
    let mut errors: BTreeMap<String, String> = BTreeMap::new();
    let mut steps: Vec<Step> = Vec::new();

    let mut add_step = |api: &str,
                        id: &str,
                        base: &BTreeMap<String, Option<String>>,
                        set: &BTreeMap<String, Option<String>>,
                        stage: Option<StageMove>,
                        errors: &mut BTreeMap<String, String>| {
        let label = object_label(api);
        let key = |p: &str| {
            if api == "deals" {
                p.to_string()
            } else {
                format!("{label}.{p}")
            }
        };
        if set.len() > MAX_SET_PROPS {
            errors.insert(
                key("_"),
                format!("一度に書ける項目は {MAX_SET_PROPS} 件までです"),
            );
            return;
        }
        let mut types = BTreeMap::new();
        for (name, value) in set {
            if !valid_property_name(name) {
                errors.insert(key(name), "項目名が不正です".into());
                continue;
            }
            let Some(def) = catalog.property(api, name).filter(|d| d.writable) else {
                errors.insert(key(name), "この項目は書き込めません".into());
                continue;
            };
            if let Err(m) = validate_value(def, value) {
                errors.insert(key(name), m);
                continue;
            }
            if !base.contains_key(name) {
                errors.insert(key(name), "変更前の値 (base) が必要です".into());
                continue;
            }
            types.insert(name.clone(), def.property_type.clone());
        }
        // base の型も比較に使う (set に無い base は無視)
        steps.push(Step {
            object: api.to_string(),
            id: id.to_string(),
            base: base
                .iter()
                .filter(|(k, _)| set.contains_key(*k))
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect(),
            set: set.clone(),
            types,
            stage,
            done: false,
        });
    };

    // ステージ
    let mut stage_move = None;
    if let Some(s) = &req.stage {
        match find_pipeline(&s.pipeline_id) {
            Some(p) if p.rule(&s.stage_id).is_some() => {
                stage_move = Some(StageMove {
                    pipeline_id: s.pipeline_id.clone(),
                    stage_id: s.stage_id.clone(),
                });
            }
            _ => {
                errors.insert(
                    "stage".into(),
                    "移動先のステージが許可されていません".into(),
                );
            }
        }
    }
    add_step(
        "deals",
        deal_id,
        &req.base,
        &req.set,
        stage_move,
        &mut errors,
    );

    if let Some(objs) = &req.objects {
        for (api, obj) in [("contacts", &objs.contact), ("companies", &objs.company)] {
            let Some(o) = obj else { continue };
            if !is_valid_id(&o.id) {
                errors.insert(format!("{}.id", object_label(api)), "ID が不正です".into());
                continue;
            }
            add_step(api, &o.id, &o.base, &o.set, None, &mut errors);
        }
    }
    if steps.iter().all(|s| s.set.is_empty() && s.stage.is_none()) && errors.is_empty() {
        errors.insert("_".into(), "変更がありません".into());
    }
    if !errors.is_empty() {
        return Err(invalid(errors, vec![]));
    }
    // 担当者・会社は、その案件に紐づくものだけ
    for (api, s) in [("contacts", "contact"), ("companies", "company")]
        .iter()
        .filter_map(|(api, _)| steps.iter().find(|s| s.object == *api).map(|s| (*api, s)))
    {
        match client.list_associations("deals", deal_id, api).await {
            Ok((refs, _)) => {
                if !refs.iter().any(|r| r.id == s.id) {
                    return Err((
                        StatusCode::FORBIDDEN,
                        json!({"error": "forbidden", "error_kind": "forbidden", "detail": format!("{} は案件に紐づいていません", object_label(api))}),
                    ));
                }
            }
            Err(e) => {
                return Err((
                    StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_GATEWAY),
                    json!({"error": e.error_kind(), "error_kind": e.error_kind()}),
                ))
            }
        }
    }
    Ok(steps)
}

pub(super) async fn patch_deal(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(deal_id): Path<String>,
    body: Bytes,
) -> Response {
    let principal =
        match rbac::authorize(&session, &state, &ctx.access, Some(RecordType::Deal)).await {
            Ok(p) => p,
            Err(denied) => return denied.into_response(),
        };
    if !is_valid_id(&deal_id) {
        return err(StatusCode::BAD_REQUEST, "invalid_id");
    }
    // 栓: 閉じていれば HubSpot も台帳も触らない
    if !ctx.write.allows(&deal_id) {
        return err(StatusCode::FORBIDDEN, "writes_disabled");
    }
    let Some(client) = state.hubspot.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let Some(audit) = state.audit.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let req: CrmDealPatchRequest = match serde_json::from_slice(&body) {
        Ok(r) => r,
        Err(_) => return err(StatusCode::BAD_REQUEST, "invalid_json"),
    };
    if !valid_operation_id(&req.operation_id) {
        return one_error("operation_id", "operation_id が不正です");
    }
    let operator = principal.email.clone().unwrap_or_default();
    let is_admin = principal.role.is_some_and(|r| r.is_admin());

    // 冪等性: 同じ operation_id は保存された結果を返す
    let op_id = req.operation_id.clone();
    match pending::blocking(&audit, {
        let op_id = op_id.clone();
        move |t| pending::get_op(t, &op_id)
    })
    .await
    {
        Ok(Ok(Some(row))) => {
            if row.deal_id != deal_id {
                return one_error("operation_id", "別の案件で使われた operation_id です");
            }
            if row.operator_email != operator && !is_admin {
                return err(StatusCode::FORBIDDEN, "forbidden");
            }
            return replay_response(&row);
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) | Err(e) => {
            tracing::warn!("crm write: ledger lookup failed: {e}");
            return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
        }
    }

    let account_id: String = session
        .get::<String>(crate::SESSION_ACCOUNT_ID_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| operator.clone());
    let session_id: String = session
        .get::<String>(crate::SESSION_LOGIN_SESSION_ID_KEY)
        .await
        .ok()
        .flatten()
        .unwrap_or_default();

    let flow = run_patch(
        &state,
        &ctx,
        &client,
        &audit,
        &deal_id,
        &req,
        &operator,
        &account_id,
        &session_id,
    );
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, flow).await {
        Ok(r) => r,
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "crm write timed out");
            timeout_response()
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_patch(
    _state: &Arc<AppState>,
    ctx: &Arc<CrmCtx>,
    client: &HubSpotClient,
    audit: &AuditDb,
    deal_id: &str,
    req: &CrmDealPatchRequest,
    operator: &str,
    account_id: &str,
    session_id: &str,
) -> Response {
    let (catalog, _) = match ctx.catalog.get(client).await {
        Ok(c) => c,
        Err(e) => return hubspot_error_response(&e),
    };
    let mut steps = match build_steps(client, &catalog, deal_id, req).await {
        Ok(s) => s,
        Err((status, v)) => return (status, Json(v)).into_response(),
    };

    // 事前確認 (現在値の読み取り → 競合・必須項目)
    let mut prepared = Vec::new();
    for s in &steps {
        match prepare(client, s).await {
            Ok(p) => prepared.push(p),
            Err(stop) => {
                return stop_response(
                    stop,
                    audit,
                    account_id,
                    session_id,
                    operator,
                    &req.operation_id,
                    deal_id,
                    &steps,
                )
                .await
            }
        }
    }

    // 台帳 (送る前に記録)
    let refs = json!({
        "contact": steps.iter().find(|s| s.object == "contacts").map(|s| s.id.clone()),
        "company": steps.iter().find(|s| s.object == "companies").map(|s| s.id.clone()),
    })
    .to_string();
    let payload = Payload {
        steps: steps.clone(),
    };
    let ins = pending::blocking(audit, {
        let (op, who, did, pl) = (
            req.operation_id.clone(),
            operator.to_string(),
            deal_id.to_string(),
            payload.clone(),
        );
        move |t| pending::insert_op(t, &op, &who, &did, &pl, &refs)
    })
    .await;
    match ins {
        Ok(Ok(())) => {}
        Ok(Err(e)) | Err(e) => {
            // 同時に同じ operation_id が来た場合は、先に入った行の結果を返す
            if let Ok(Ok(Some(row))) = pending::blocking(audit, {
                let op = req.operation_id.clone();
                move |t| pending::get_op(t, &op)
            })
            .await
            {
                return replay_response(&row);
            }
            tracing::warn!("crm write: ledger insert failed: {e}");
            return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
        }
    }

    // 送信
    let mut run = Run::default();
    let mut outcome: Result<(), Stop> = Ok(());
    for (s, p) in steps.iter_mut().zip(prepared.iter()) {
        if let Err(stop) = send(client, s, p, &mut run).await {
            outcome = Err(stop);
            break;
        }
    }
    let payload = Payload { steps };
    match outcome {
        Ok(()) => {
            let body = CrmPatchSaved {
                status: "saved".into(),
                values: run.values.clone(),
                objects_values: run.other.clone(),
                fetched_at: now_rfc3339(),
            };
            let body_json = serde_json::to_string(&body).unwrap_or_default();
            let res = pending::blocking(audit, {
                let (op, pl) = (req.operation_id.clone(), payload.clone());
                move |t| pending::update_op(t, &op, "saved", 1, "", 200, &body_json, "", &pl)
            })
            .await;
            if !matches!(res, Ok(Ok(()))) {
                tracing::warn!("crm write: ledger update (saved) failed: {res:?}");
            }
            audit_write(
                audit,
                account_id,
                session_id,
                operator,
                &req.operation_id,
                deal_id,
                "saved",
                &run.changes,
            )
            .await;
            invalidate_caches(ctx, deal_id);
            (StatusCode::OK, Json(body)).into_response()
        }
        Err(Stop::Transient(e)) => {
            // 途中まで書けた分は監査に残す
            if !run.changes.is_empty() {
                audit_write(
                    audit,
                    account_id,
                    session_id,
                    operator,
                    &req.operation_id,
                    deal_id,
                    "partial_saved",
                    &run.changes,
                )
                .await;
                invalidate_caches(ctx, deal_id);
            }
            let full = pending::blocking(audit, pending::count_pending)
                .await
                .ok()
                .and_then(Result::ok)
                .is_some_and(|n| n >= ctx.write.pending_max);
            if full {
                let body = json!({"error": "queue_full", "error_kind": "queue_full"}).to_string();
                let _ = pending::blocking(audit, {
                    let (op, pl) = (req.operation_id.clone(), payload.clone());
                    move |t| {
                        pending::update_op(t, &op, "failed", 1, "queue_full", 503, &body, "", &pl)
                    }
                })
                .await;
                audit_write(
                    audit,
                    account_id,
                    session_id,
                    operator,
                    &req.operation_id,
                    deal_id,
                    "queue_full",
                    &intended_changes(&payload.steps, None),
                )
                .await;
                return err(StatusCode::SERVICE_UNAVAILABLE, "queue_full");
            }
            let next = pending::iso(Utc::now() + pending::backoff_after(1));
            let code = e.error_kind().to_string();
            let res = pending::blocking(audit, {
                let (op, pl, code, next) = (req.operation_id.clone(), payload.clone(), code, next);
                move |t| pending::update_op(t, &op, "pending", 1, &code, 0, "", &next, &pl)
            })
            .await;
            if !matches!(res, Ok(Ok(()))) {
                // 再送待ちにも積めない: どこにも保存できていない
                tracing::warn!("crm write: ledger update (pending) failed: {res:?}");
                return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
            }
            audit_write(
                audit,
                account_id,
                session_id,
                operator,
                &req.operation_id,
                deal_id,
                "queued",
                &intended_changes(&payload.steps, None),
            )
            .await;
            wake_worker();
            (
                StatusCode::ACCEPTED,
                Json(CrmPatchQueued {
                    status: "queued".into(),
                    operation_id: req.operation_id.clone(),
                }),
            )
                .into_response()
        }
        Err(stop) => {
            if !run.changes.is_empty() {
                audit_write(
                    audit,
                    account_id,
                    session_id,
                    operator,
                    &req.operation_id,
                    deal_id,
                    "partial_saved",
                    &run.changes,
                )
                .await;
                invalidate_caches(ctx, deal_id);
            }
            let resp = stop_response_recorded(
                stop,
                audit,
                account_id,
                session_id,
                operator,
                &req.operation_id,
                deal_id,
                &payload,
                1,
            )
            .await;
            resp
        }
    }
}

/// 事前確認の段階で止まった (台帳には入れていない) 場合の応答 + 監査
#[allow(clippy::too_many_arguments)]
async fn stop_response(
    stop: Stop,
    audit: &AuditDb,
    account_id: &str,
    session_id: &str,
    operator: &str,
    operation_id: &str,
    deal_id: &str,
    steps: &[Step],
) -> Response {
    let (status, body, outcome, cur) = stop_body(&stop);
    match stop {
        Stop::Transient(e) => {
            audit_write(
                audit,
                account_id,
                session_id,
                operator,
                operation_id,
                deal_id,
                &format!("read_failed:{}", e.error_kind()),
                &[],
            )
            .await;
            hubspot_error_response(&e)
        }
        Stop::Permanent(e) => {
            audit_write(
                audit,
                account_id,
                session_id,
                operator,
                operation_id,
                deal_id,
                &format!("failed:{}", e.error_kind()),
                &[],
            )
            .await;
            hubspot_error_response(&e)
        }
        _ => {
            audit_write(
                audit,
                account_id,
                session_id,
                operator,
                operation_id,
                deal_id,
                &outcome,
                &intended_changes(steps, cur.as_ref()),
            )
            .await;
            (status, Json(body)).into_response()
        }
    }
}

/// 台帳に入れた後で止まった (恒久エラー・競合など) 場合: 台帳を `failed` にして応答する
#[allow(clippy::too_many_arguments)]
async fn stop_response_recorded(
    stop: Stop,
    audit: &AuditDb,
    account_id: &str,
    session_id: &str,
    operator: &str,
    operation_id: &str,
    deal_id: &str,
    payload: &Payload,
    attempts: i64,
) -> Response {
    let (status, body, outcome, cur) = stop_body(&stop);
    let (status, body, code) = match &stop {
        Stop::Permanent(e) => match e {
            HubSpotError::Upstream { .. } => {
                let (s, v) = invalid(
                    BTreeMap::from([(
                        "_".to_string(),
                        "HubSpot が値を受け付けませんでした".to_string(),
                    )]),
                    vec![],
                );
                (s, v, e.error_kind().to_string())
            }
            other => (
                StatusCode::from_u16(other.http_status()).unwrap_or(StatusCode::BAD_GATEWAY),
                json!({"error": other.error_kind(), "error_kind": other.error_kind()}),
                other.error_kind().to_string(),
            ),
        },
        _ => (status, body, outcome.clone()),
    };
    let body_s = body.to_string();
    let res = pending::blocking(audit, {
        let (op, pl, code, st) = (
            operation_id.to_string(),
            payload.clone(),
            code.clone(),
            i64::from(status.as_u16()),
        );
        move |t| pending::update_op(t, &op, "failed", attempts, &code, st, &body_s, "", &pl)
    })
    .await;
    if !matches!(res, Ok(Ok(()))) {
        tracing::warn!("crm write: ledger update (failed) failed: {res:?}");
    }
    audit_write(
        audit,
        account_id,
        session_id,
        operator,
        operation_id,
        deal_id,
        &format!("failed:{code}"),
        &intended_changes(&payload.steps, cur.as_ref()),
    )
    .await;
    (status, Json(body)).into_response()
}

/// 止めた理由 → (HTTP status, 本文, 監査の結果名, 現在値)
fn stop_body(
    stop: &Stop,
) -> (
    StatusCode,
    Value,
    String,
    Option<BTreeMap<String, Option<String>>>,
) {
    match stop {
        Stop::Conflict {
            object,
            current,
            changed,
        } => (
            StatusCode::CONFLICT,
            json!(CrmPatchConflict {
                status: "conflict".into(),
                object: object.clone(),
                current: current.clone(),
                changed_by_hubspot: changed.clone(),
            }),
            "conflict".into(),
            Some(current.clone()),
        ),
        Stop::Missing(m) => {
            let errors = m
                .iter()
                .map(|k| (k.clone(), "移動先のステージで必須です".to_string()))
                .collect();
            let (s, v) = invalid(errors, m.clone());
            (s, v, "missing_required".into(), None)
        }
        Stop::Transient(e) | Stop::Permanent(e) => (
            StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_GATEWAY),
            json!({"error": e.error_kind(), "error_kind": e.error_kind()}),
            e.error_kind().to_string(),
            None,
        ),
    }
}

// ---------------------------------------------------------------------------
// GET /api/crm/operations/{id}
// ---------------------------------------------------------------------------

pub fn api_status(row: &pending::OpRow) -> &'static str {
    match row.status.as_str() {
        "saved" => "saved",
        "failed" | "discarded" => "failed",
        _ if row.attempts > 1 => "retrying",
        _ => "pending",
    }
}

pub(super) async fn get_operation(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(operation_id): Path<String>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(denied) => return denied.into_response(),
    };
    if !valid_operation_id(&operation_id) {
        return err(StatusCode::BAD_REQUEST, "invalid_id");
    }
    let Some(audit) = state.audit.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let row = match pending::blocking(&audit, {
        let id = operation_id.clone();
        move |t| pending::get_op(t, &id)
    })
    .await
    {
        Ok(Ok(Some(r))) => r,
        Ok(Ok(None)) => return err(StatusCode::NOT_FOUND, "not_found"),
        _ => return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable"),
    };
    let is_admin = principal.role.is_some_and(|r| r.is_admin());
    if row.operator_email != principal.email.clone().unwrap_or_default() && !is_admin {
        return err(StatusCode::FORBIDDEN, "forbidden");
    }
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Json(CrmOperationStatus {
        operation_id: row.operation_id.clone(),
        status: api_status(&row).to_string(),
        attempts: row.attempts,
        last_error_code: opt(&row.last_error_code),
        next_retry_at: opt(&row.next_retry_at),
    })
    .into_response()
}

// ---------------------------------------------------------------------------
// 管理者: 一覧・再試行・破棄 (require_admin_mw の内側)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct AdminListQuery {
    status: Option<String>,
}

fn admin_row(r: &pending::OpRow) -> CrmAdminOperation {
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    CrmAdminOperation {
        operation_id: r.operation_id.clone(),
        deal_id: r.deal_id.clone(),
        operator_email: r.operator_email.clone(),
        props: r.changed_props(),
        status: api_status(r).to_string(),
        error: opt(&r.last_error_code),
        attempts: r.attempts,
        created_at: r.created_at.clone(),
        updated_at: r.updated_at.clone(),
        next_retry_at: opt(&r.next_retry_at),
    }
}

pub async fn api_admin_list(
    State(state): State<Arc<AppState>>,
    Query(q): Query<AdminListQuery>,
) -> Response {
    let Some(audit) = state.audit.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    let status = match q.status.as_deref() {
        Some("pending") => "pending",
        Some("failed") | None => "failed",
        Some(_) => return err(StatusCode::BAD_REQUEST, "invalid_param"),
    };
    match pending::blocking(&audit, move |t| pending::list_ops(t, status, 200)).await {
        Ok(Ok(rows)) => Json(CrmAdminOperationsResponse {
            operations: rows.iter().map(admin_row).collect(),
        })
        .into_response(),
        _ => err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable"),
    }
}

/// 失敗した操作をもう一度再送待ちに戻す (回数を数え直す)
pub async fn api_admin_retry(
    State(state): State<Arc<AppState>>,
    Path(operation_id): Path<String>,
) -> Response {
    admin_transition(state, operation_id, true).await
}

/// 失敗・再送待ちの操作を破棄する (再送しない)
pub async fn api_admin_discard(
    State(state): State<Arc<AppState>>,
    Path(operation_id): Path<String>,
) -> Response {
    admin_transition(state, operation_id, false).await
}

async fn admin_transition(state: Arc<AppState>, operation_id: String, retry: bool) -> Response {
    let Some(audit) = state.audit.clone() else {
        return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    };
    if !valid_operation_id(&operation_id) {
        return err(StatusCode::BAD_REQUEST, "invalid_id");
    }
    let row = match pending::blocking(&audit, {
        let id = operation_id.clone();
        move |t| pending::get_op(t, &id)
    })
    .await
    {
        Ok(Ok(Some(r))) => r,
        Ok(Ok(None)) => return err(StatusCode::NOT_FOUND, "not_found"),
        _ => return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable"),
    };
    let allowed = if retry {
        row.status == "failed"
    } else {
        matches!(row.status.as_str(), "failed" | "pending")
    };
    if !allowed {
        return err(StatusCode::CONFLICT, "invalid_state");
    }
    let res = pending::blocking(&audit, {
        let r = row.clone();
        move |t| {
            if retry {
                pending::update_op(
                    t,
                    &r.operation_id,
                    "pending",
                    0,
                    "",
                    0,
                    "",
                    &pending::now_iso(),
                    &r.payload,
                )
            } else {
                pending::update_op(
                    t,
                    &r.operation_id,
                    "discarded",
                    r.attempts,
                    &r.last_error_code,
                    r.http_status,
                    &r.result_json,
                    "",
                    &r.payload,
                )
            }
        }
    })
    .await;
    if !matches!(res, Ok(Ok(()))) {
        return err(StatusCode::SERVICE_UNAVAILABLE, "queue_unavailable");
    }
    if retry {
        wake_worker();
    }
    Json(json!({"operation_id": operation_id, "status": if retry {"pending"} else {"failed"}}))
        .into_response()
}

// ---------------------------------------------------------------------------
// 再送 worker
// ---------------------------------------------------------------------------

static WAKE: OnceLock<Arc<Notify>> = OnceLock::new();

fn wake_handle() -> Arc<Notify> {
    WAKE.get_or_init(|| Arc::new(Notify::new())).clone()
}

/// 再送 worker を起こす (受付・管理者の再試行の後)
pub fn wake_worker() {
    wake_handle().notify_one();
}

/// 期限が来た操作を 1 件処理する。結果は台帳と監査に残す
pub async fn process_op(audit: &AuditDb, client: &HubSpotClient, row: &pending::OpRow) {
    let mut payload = row.payload.clone();
    let mut run = Run::default();
    let attempts = row.attempts.max(1) + i64::from(row.status == "pending");
    let mut result: Result<(), Stop> = Ok(());
    for i in 0..payload.steps.len() {
        if payload.steps[i].done {
            continue;
        }
        let prep = match prepare(client, &payload.steps[i]).await {
            Ok(p) => p,
            Err(s) => {
                result = Err(s);
                break;
            }
        };
        let mut step = payload.steps[i].clone();
        let r = send(client, &mut step, &prep, &mut run).await;
        payload.steps[i] = step;
        if let Err(s) = r {
            result = Err(s);
            break;
        }
    }
    let account = {
        let email = row.operator_email.clone();
        pending::blocking(audit, move |t| {
            crate::audit::dao::find_account_by_email(t, &email).map(|a| a.id)
        })
        .await
        .ok()
        .flatten()
        .unwrap_or_else(|| row.operator_email.clone())
    };
    let log = |outcome: String, changes: Vec<Change>| {
        let (a, acc, who, op, did) = (
            audit.clone(),
            account.clone(),
            row.operator_email.clone(),
            row.operation_id.clone(),
            row.deal_id.clone(),
        );
        async move { audit_write(&a, &acc, "", &who, &op, &did, &outcome, &changes).await }
    };
    let update = |status: &'static str,
                  code: String,
                  http: i64,
                  body: String,
                  next: String,
                  pl: Payload,
                  att: i64| {
        let op = row.operation_id.clone();
        async move {
            let r = pending::blocking(audit, move |t| {
                pending::update_op(t, &op, status, att, &code, http, &body, &next, &pl)
            })
            .await;
            if !matches!(r, Ok(Ok(()))) {
                tracing::warn!("crm write worker: ledger update failed: {r:?}");
            }
        }
    };
    match result {
        Ok(()) => {
            let body = serde_json::to_string(&CrmPatchSaved {
                status: "saved".into(),
                values: run.values.clone(),
                objects_values: run.other.clone(),
                fetched_at: now_rfc3339(),
            })
            .unwrap_or_default();
            update(
                "saved",
                String::new(),
                200,
                body,
                String::new(),
                payload,
                attempts,
            )
            .await;
            log("saved_by_retry".into(), run.changes).await;
        }
        Err(Stop::Transient(e)) => {
            if !run.changes.is_empty() {
                log("partial_saved".into(), run.changes.clone()).await;
            }
            if attempts >= pending::MAX_ATTEMPTS {
                update(
                    "failed",
                    "max_attempts".into(),
                    503,
                    json!({"error":"max_attempts"}).to_string(),
                    String::new(),
                    payload.clone(),
                    attempts,
                )
                .await;
                log(
                    "failed:max_attempts".into(),
                    intended_changes(&payload.steps, None),
                )
                .await;
            } else {
                let next = pending::iso(Utc::now() + pending::backoff_after(attempts));
                update(
                    "pending",
                    e.error_kind().into(),
                    0,
                    String::new(),
                    next,
                    payload,
                    attempts,
                )
                .await;
            }
        }
        Err(stop) => {
            if !run.changes.is_empty() {
                log("partial_saved".into(), run.changes.clone()).await;
            }
            let (status, body, outcome, cur) = stop_body(&stop);
            let (status, body, code) = match &stop {
                Stop::Permanent(e) => (
                    StatusCode::from_u16(e.http_status()).unwrap_or(StatusCode::BAD_GATEWAY),
                    json!({"error": e.error_kind()}),
                    e.error_kind().to_string(),
                ),
                _ => (status, body, outcome),
            };
            update(
                "failed",
                code.clone(),
                i64::from(status.as_u16()),
                body.to_string(),
                String::new(),
                payload.clone(),
                attempts,
            )
            .await;
            log(
                format!("failed:{code}"),
                intended_changes(&payload.steps, cur.as_ref()),
            )
            .await;
        }
    }
}

/// 1 周: 期限が来た操作を処理して、次に期限が来る時刻を返す (再送待ちが無ければ None)
pub async fn run_due(audit: &AuditDb, client: &HubSpotClient) -> Option<chrono::DateTime<Utc>> {
    let now = pending::now_iso();
    let rows = pending::blocking(audit, {
        let now = now.clone();
        move |t| pending::due_ops(t, &now, 20)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    for r in &rows {
        process_op(audit, client, r).await;
    }
    let next = pending::blocking(audit, pending::next_due)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()?;
    chrono::DateTime::parse_from_rfc3339(&next)
        .ok()
        .map(|d| d.with_timezone(&Utc))
}

/// 再送 worker を始める (背景の優先度で HubSpot を呼ぶ。書き込みでポーリングしない)
pub fn spawn_worker(state: Arc<AppState>) {
    let (Some(audit), Some(client)) = (state.audit.clone(), state.hubspot.clone()) else {
        return;
    };
    let client = client.background();
    let wake = wake_handle();
    tokio::spawn(async move {
        // 起動直後は他の初期化を邪魔しない (E2E は debug ビルドだけ `CRM_WORKER_START_DELAY_SECS_DEBUG` で縮められる)
        let start_delay =
            pending::debug_override_secs("CRM_WORKER_START_DELAY_SECS_DEBUG").unwrap_or(30);
        tokio::time::sleep(Duration::from_secs(start_delay as u64)).await;
        let mut last_purge = std::time::Instant::now() - Duration::from_secs(86_400);
        loop {
            let next = run_due(&audit, &client).await;
            if last_purge.elapsed() >= Duration::from_secs(86_400) {
                last_purge = std::time::Instant::now();
                let _ = pending::blocking(&audit, pending::purge_old).await;
            }
            let wait = match next {
                Some(t) => (t - Utc::now())
                    .to_std()
                    .unwrap_or(Duration::from_secs(5))
                    .clamp(Duration::from_secs(5), Duration::from_secs(300)),
                None => Duration::from_secs(600),
            };
            tokio::select! {
                _ = tokio::time::sleep(wait) => {}
                _ = wake.notified() => {}
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def(t: &str, ft: &str, opts: &[&str]) -> CrmCatalogProperty {
        CrmCatalogProperty {
            name: "x".into(),
            label: "x".into(),
            property_type: t.into(),
            field_type: ft.into(),
            options: opts
                .iter()
                .map(|v| CrmPropertyOption {
                    label: (*v).into(),
                    value: (*v).into(),
                    hidden: false,
                })
                .collect(),
            writable: true,
        }
    }

    #[test]
    fn 値の検証() {
        let e = def("enumeration", "select", &["a", "b"]);
        assert!(validate_value(&e, &Some("a".into())).is_ok());
        assert!(validate_value(&e, &Some("zzz".into())).is_err());
        assert!(validate_value(&e, &None).is_ok(), "消去は常に許す");
        assert!(validate_value(&e, &Some("".into())).is_ok());
        let c = def("enumeration", "checkbox", &["a", "b"]);
        assert!(validate_value(&c, &Some("a;b".into())).is_ok());
        assert!(validate_value(&c, &Some("a;q".into())).is_err());
        let d = def("date", "date", &[]);
        assert!(validate_value(&d, &Some("2026-10-12".into())).is_ok());
        assert!(validate_value(&d, &Some("2026-13-40".into())).is_err());
        assert!(validate_value(&d, &Some("10/12".into())).is_err());
        let n = def("number", "number", &[]);
        assert!(validate_value(&n, &Some("1200.5".into())).is_ok());
        assert!(validate_value(&n, &Some("abc".into())).is_err());
        let s = def("string", "text", &[]);
        assert!(validate_value(&s, &Some("x".repeat(MAX_LINE_LEN))).is_ok());
        assert!(validate_value(&s, &Some("x".repeat(MAX_LINE_LEN + 1))).is_err());
        let t = def("string", "textarea", &[]);
        assert!(validate_value(&t, &Some("x".repeat(MAX_LINE_LEN + 1))).is_ok());
    }

    #[test]
    fn 比較用の正規化() {
        assert_eq!(norm(None, "string"), "");
        assert_eq!(norm(Some("  "), "string"), "");
        assert_eq!(
            norm(Some("120000.0"), "number"),
            norm(Some("120000"), "number")
        );
        // 日付: 文字列 / midnight UTC の ms / ISO が同じ日に
        assert_eq!(norm(Some("2026-10-12"), "date"), "2026-10-12");
        assert_eq!(norm(Some("1791763200000"), "date"), "2026-10-12");
        assert_eq!(norm(Some("2026-10-12T00:00:00Z"), "date"), "2026-10-12");
        assert_eq!(
            norm(Some("2026-10-12T00:00:00.000Z"), "datetime"),
            norm(Some("1791763200000"), "datetime")
        );
        assert_eq!(norm(Some("TRUE"), "bool"), "true");
    }

    #[test]
    fn 書き込みの栓() {
        let mut c = WriteConfig::default();
        assert!(!c.allows("1"));
        c.allowlist.insert("1".into());
        assert!(c.allows("1"));
        assert!(!c.allows("2"));
        c.enabled = true;
        assert!(c.allows("2"));
    }

    #[test]
    fn 操作の状態名() {
        let mk = |status: &str, attempts: i64| pending::OpRow {
            operation_id: "x".into(),
            operator_email: "u@example.com".into(),
            deal_id: "1".into(),
            payload: Payload { steps: vec![] },
            status: status.into(),
            attempts,
            last_error_code: String::new(),
            http_status: 0,
            result_json: String::new(),
            next_retry_at: String::new(),
            created_at: String::new(),
            updated_at: String::new(),
        };
        assert_eq!(api_status(&mk("in_progress", 0)), "pending");
        assert_eq!(api_status(&mk("pending", 1)), "pending");
        assert_eq!(api_status(&mk("pending", 3)), "retrying");
        assert_eq!(api_status(&mk("saved", 1)), "saved");
        assert_eq!(api_status(&mk("failed", 1)), "failed");
        assert_eq!(api_status(&mk("discarded", 1)), "failed");
    }
}
