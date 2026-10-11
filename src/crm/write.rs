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
use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
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

use super::op_status::{self, OpState, OpStatusCache};
use super::pending::{self, Payload, StageMove, Step};
use super::property_catalog::{valid_property_name, CatalogEntry, CrmCatalogProperty};
use super::queue_pipelines::{find_pipeline, QUEUE_PIPELINES};
use super::rbac;
use super::record_lock::{self, RecordLocks};
use super::routes::{
    hubspot_error_response, is_valid_id, timeout_response, CrmCtx, CRM_REQUEST_DEADLINE,
};
use super::stage_rules;
use crate::audit::AuditDb;
use crate::handlers::crm_metadata::CrmPropertyOption;
use crate::hubspot::gateway::{cache_hit, cache_miss, Lane};
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
    /// 1 人あたりの保存受付の上限 (1 分あたり。`CRM_WRITE_RATE_PER_MIN`、最小 1。0・不正は既定 200)
    pub rate_per_min: u32,
    /// レコード単位の直列化の表 (本番は要求側と再送 worker で同じもの。`Default` は個別)
    pub locks: Arc<RecordLocks>,
    /// 同じレコードの先行する書き込みを待つ最長 (超えたら 503 `record_busy`)
    pub lock_wait: Duration,
    /// 操作の状態の表 (本番は要求側・再送 worker・照会で同じもの。`Default` は個別)
    pub statuses: Arc<OpStatusCache>,
    /// 1 回の PATCH 要求を待つ最長 (超えたとき、台帳に記録済みなら 202、未記録なら 504)
    pub deadline: Duration,
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
        let rate_per_min =
            parse_rate_per_min(std::env::var("CRM_WRITE_RATE_PER_MIN").ok().as_deref());
        Self {
            enabled,
            allowlist,
            pending_max,
            rate_per_min,
            locks: record_lock::shared(),
            lock_wait: record_lock::DEFAULT_LOCK_WAIT,
            statuses: op_status::shared(),
            deadline: CRM_REQUEST_DEADLINE,
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
            rate_per_min: DEFAULT_WRITE_RATE_PER_MIN,
            locks: Arc::new(RecordLocks::default()),
            lock_wait: record_lock::DEFAULT_LOCK_WAIT,
            statuses: Arc::new(OpStatusCache::default()),
            deadline: CRM_REQUEST_DEADLINE,
        }
    }
}

/// 1 人あたりの書き込み受付の速さの既定の上限 (1 分あたり。`CRM_WRITE_RATE_PER_MIN` で変える)
pub const DEFAULT_WRITE_RATE_PER_MIN: u32 = 200;

/// `CRM_WRITE_RATE_PER_MIN` の読み取り。未設定・0・数でない値は既定 (200)。最小は 1
pub fn parse_rate_per_min(v: Option<&str>) -> u32 {
    v.and_then(|s| s.trim().parse::<u32>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(DEFAULT_WRITE_RATE_PER_MIN)
}

/// 操作者ごとの固定窓の受付制限 (プロセス内。ルーターごとに 1 つ)
pub struct WriteRateLimiter {
    limit: u32,
    windows: std::sync::Mutex<std::collections::HashMap<String, (std::time::Instant, u32)>>,
}

impl Default for WriteRateLimiter {
    fn default() -> Self {
        Self::new(DEFAULT_WRITE_RATE_PER_MIN)
    }
}

impl WriteRateLimiter {
    pub fn new(limit: u32) -> Self {
        Self {
            limit: limit.max(1),
            windows: Default::default(),
        }
    }

    /// 受け付けてよければ true (数える)
    pub fn allow(&self, who: &str) -> bool {
        let now = std::time::Instant::now();
        let Ok(mut m) = self.windows.lock() else {
            return true;
        };
        if m.len() > 10_000 {
            m.retain(|_, (t, _)| now.duration_since(*t) < Duration::from_secs(60));
        }
        let e = m.entry(who.to_string()).or_insert((now, 0));
        if now.duration_since(e.0) >= Duration::from_secs(60) {
            *e = (now, 0);
        }
        e.1 += 1;
        e.1 <= self.limit
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
    /// 途中の段までは HubSpot に書けていた場合の、書けた分の保存後の値。何も書けていなければ無い
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partial: Option<CrmPatchPartial>,
}

/// 複数オブジェクトの書き込みが途中で止まったとき、すでに HubSpot に書けた分 (エラー応答に付く)
#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct CrmPatchPartial {
    /// 案件の項目の保存後の値 (案件の段が済んでいなければ空)
    pub values: BTreeMap<String, Option<String>>,
    /// 担当者・会社の保存後の値 (`contact` / `company`)
    #[serde(default)]
    pub objects_values: BTreeMap<String, BTreeMap<String, Option<String>>>,
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

/// 同じレコードへの先行する書き込みが終わらない (何も保存されていない。少し待って再度保存する)
fn record_busy() -> Response {
    count_rejection("record_busy");
    err(StatusCode::SERVICE_UNAVAILABLE, "record_busy")
}

fn now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// 断った要求の集計 (監査 Turso に 1 行ずつ書かない)
// ---------------------------------------------------------------------------

static REJECTIONS: OnceLock<Mutex<BTreeMap<String, u64>>> = OnceLock::new();

/// 受け付けなかった (何も保存していない) 要求を種類別に数える。`/api/admin/hubspot-usage` が読む。
///
/// 監査 (`activity_logs`) に残すのは**受け付けた結果** (保存した・再送待ちにした・競合・送ったあとの失敗) だけ。
/// 混雑 (`hubspot_busy`)・速さの上限 (`rate_limited`)・入力の誤り (`validation`) などを 1 件ずつ Turso に書くと、
/// 負荷が高いほど書き込みが増える (2026-10-09 の負荷試験: 拒否だけで監査の書き込みが支配的)。
pub fn count_rejection(kind: &str) {
    if let Ok(mut m) = REJECTIONS.get_or_init(Default::default).lock() {
        // 種類は固定の `error_kind` の語彙だけ。念のため際限なく増えないようにする
        if m.len() < 64 || m.contains_key(kind) {
            *m.entry(kind.to_string()).or_default() += 1;
        }
    }
}

/// 種類 → 起動してからの回数
pub fn rejections_snapshot() -> BTreeMap<String, u64> {
    REJECTIONS
        .get_or_init(Default::default)
        .lock()
        .map(|m| m.clone())
        .unwrap_or_default()
}

/// 台帳の `status` と試行回数 → API の状態名
pub fn api_status_of(status: &str, attempts: i64) -> &'static str {
    match status {
        "saved" => "saved",
        "failed" | "discarded" => "failed",
        _ if attempts > 1 => "retrying",
        _ => "pending",
    }
}

/// 台帳を更新した直後に、同じ内容を状態の表へ写す (照会をメモリから返すため。`op_status`)
fn note_status(
    write: &WriteConfig,
    operation_id: &str,
    operator: &str,
    status: &str,
    attempts: i64,
    last_error_code: &str,
    next_retry_at: &str,
) {
    write.statuses.put(
        operation_id,
        OpState {
            operator_email: operator.to_string(),
            status: api_status_of(status, attempts).to_string(),
            attempts,
            last_error_code: last_error_code.to_string(),
            next_retry_at: next_retry_at.to_string(),
        },
    );
}

/// 台帳に記録する瞬間 (= 以後は取り消せない) を、要求のタイムアウトと取り合うための印。
///
/// 要求は別のタスクで走り、ハンドラは締め切りまで待つだけ。締め切りが来たとき:
/// - まだ台帳に記録していない (`PRE`) → 取り下げて (`ABANDONED`) 504。タスクは記録の直前でこれを見て、何も送らずに終わる
/// - もう記録した (`COMMITTED`) → タスクは最後まで走り (HubSpot への送信・台帳の更新)、ハンドラは 202 queued を返す。
///   画面は操作の状態を見に行く。中断された要求が台帳に `in_progress` のまま取り残されない
#[derive(Default)]
struct CommitGate(AtomicU8);

const GATE_PRE: u8 = 0;
const GATE_COMMITTED: u8 = 1;
const GATE_ABANDONED: u8 = 2;

impl CommitGate {
    /// 記録に進む。すでに取り下げられていたら false (何もせず終わる)
    fn commit(&self) -> bool {
        self.0
            .compare_exchange(GATE_PRE, GATE_COMMITTED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
    /// 取り下げる。すでに記録に進んでいたら false
    fn abandon(&self) -> bool {
        self.0
            .compare_exchange(GATE_PRE, GATE_ABANDONED, Ordering::SeqCst, Ordering::SeqCst)
            .is_ok()
    }
}

fn invalid(errors: BTreeMap<String, String>, missing: Vec<String>) -> (StatusCode, Value) {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        json!(CrmPatchInvalid {
            status: "invalid".into(),
            errors,
            missing_required: missing,
            partial: None,
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
    // 競合確認の読み取りは相乗り・キャッシュをしない (直前の書き込み前の値と比べると、更新を取りこぼす)
    let rec = client
        .get_object_fresh(&step.object, &step.id, &names)
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
) -> bool {
    // 担当者・会社の値は個人情報なので監査に写さない (どの項目が変わったかだけ残す。値は HubSpot の変更履歴で見る)
    let redacted: Vec<Change> = changes
        .iter()
        .map(|c| {
            if c.object == "deal" {
                c.clone()
            } else {
                Change {
                    before: c.before.as_ref().map(|_| REDACTED.to_string()),
                    after: c.after.as_ref().map(|_| REDACTED.to_string()),
                    ..c.clone()
                }
            }
        })
        .collect();
    let meta = json!({
        "operation_id": operation_id,
        "operator": operator,
        "outcome": outcome,
        "at": now_rfc3339(),
        "changes": redacted,
    })
    .to_string();
    let (a, acc, sid, did) = (
        audit.clone(),
        account_id.to_string(),
        session_id.to_string(),
        deal_id.to_string(),
    );
    let res = tokio::task::spawn_blocking(move || {
        crate::audit::dao::insert_activity(&a, &acc, &sid, "crm_write", "hubspot_deal", &did, &meta)
    })
    .await;
    match res {
        Ok(true) => true,
        Ok(false) => {
            // HubSpot にはすでに書けている。利用者の応答は変えないが、監査の欠落は error で残す
            tracing::error!(
                operation_id = %operation_id,
                outcome = %outcome,
                "crm write audit row was NOT recorded"
            );
            false
        }
        Err(e) => {
            tracing::error!("crm write audit join failed: {e}");
            false
        }
    }
}

/// 監査に写さない値の代わりに置く印
pub const REDACTED: &str = "(非表示)";

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
        // 管理者が破棄した操作。同じ operation_id では二度と送らない (クライアントは鍵を捨てて入力し直す)
        "discarded" => err(StatusCode::GONE, "discarded"),
        // `failed` はここに来ない (呼び出し側が受付し直す)。万一来ても失敗の本文を返す
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
            Ok((refs, has_more)) => {
                if !refs.iter().any(|r| r.id == s.id) {
                    if has_more {
                        // 先頭ページに無いだけかもしれない。「紐づいていない」とは言えないので確認できなかったことにする
                        return Err((
                            StatusCode::SERVICE_UNAVAILABLE,
                            json!({"error": "association_check_incomplete", "error_kind": "association_check_incomplete"}),
                        ));
                    }
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
    // 1 人あたりの受付の速さ (拒否される要求も監査 Turso に書くので、連打で書き込みが膨らまないよう絞る)
    if !ctx.write_rate.allow(&operator) {
        count_rejection("rate_limited");
        return err(StatusCode::TOO_MANY_REQUESTS, "rate_limited");
    }

    // 混んでいて、しかもこの operation_id を知らない (= 新しい保存) なら、台帳を読む前に断る。
    // 過負荷のとき、断る要求ごとに Turso の SELECT (台帳の照会) を払わない。
    // 状態の表に載っている操作 (= 受け付け済みの再送) は、混んでいても保存された結果を返せるので通す
    if ctx.write.statuses.get(&req.operation_id).is_none()
        && client
            .gateway()
            .admit(Lane::General, client.priority(), 2)
            .is_err()
    {
        count_rejection("hubspot_busy");
        return hubspot_error_response(&HubSpotError::Busy);
    }

    // 冪等性: 同じ operation_id は保存された結果を返す
    let op_id = req.operation_id.clone();
    let mut reuse_failed = false;
    match pending::blocking(&audit, {
        let op_id = op_id.clone();
        move |t| pending::get_op(t, &op_id)
    })
    .await
    {
        Ok(Ok(Some(row))) => {
            // 持ち主の確認が先 (他人の operation_id の存在や案件を、違いのある応答で探らせない)
            if row.operator_email != operator && !is_admin {
                return err(StatusCode::FORBIDDEN, "forbidden");
            }
            if row.deal_id != deal_id {
                return one_error("operation_id", "別の案件で使われた operation_id です");
            }
            if row.status == "failed" {
                // 失敗は返し続けない。同じ operation_id の再送は新しい受付として最初からやり直す
                // (HubSpot の現在値を読み直して比べるので、途中まで書けた分は二重に書かない)
                reuse_failed = true;
            } else {
                return replay_response(&row);
            }
        }
        Ok(Ok(None)) => {}
        Ok(Err(e)) | Err(e) => {
            tracing::warn!("crm write: ledger lookup failed: {e}");
            count_rejection("queue_unavailable");
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

    // 別のタスクで走らせる: 締め切りでハンドラの future を捨てても、台帳に記録済みの要求は最後まで走って
    // 台帳を確定させる (送ったのに `in_progress` のまま取り残される・送った結果が分からないままになるのを防ぐ)
    let gate = Arc::new(CommitGate::default());
    let deadline = ctx.write.deadline;
    let mut task = tokio::spawn({
        let (state, ctx, gate) = (state.clone(), ctx.clone(), gate.clone());
        async move {
            run_patch(
                &state,
                &ctx,
                &client,
                &audit,
                &deal_id,
                &req,
                &operator,
                &account_id,
                &session_id,
                reuse_failed,
                &gate,
            )
            .await
        }
    });
    match tokio::time::timeout(deadline, &mut task).await {
        Ok(Ok(r)) => r,
        Ok(Err(e)) => {
            tracing::error!("crm write task failed: {e}");
            count_rejection("internal_error");
            err(StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
        }
        Err(_) => {
            tracing::warn!(error_kind = "crm_timeout", "crm write timed out");
            if gate.abandon() {
                // まだ何も記録も送信もしていない。タスクは記録の直前で気づいて終わる
                count_rejection("crm_timeout");
                timeout_response()
            } else {
                // 台帳に記録済み: タスクは最後まで走る。送れたか・再送待ちかは操作の状態を見に行けば分かる
                (
                    StatusCode::ACCEPTED,
                    Json(CrmPatchQueued {
                        status: "queued".into(),
                        operation_id: op_id,
                    }),
                )
                    .into_response()
            }
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
    reuse_failed: bool,
    gate: &CommitGate,
) -> Response {
    // 同じ案件への書き込みは 1 つずつ (読む → 比べる → 書く の間に他の要求が割り込めない)
    let Ok(mut held) = ctx
        .write
        .locks
        .acquire([record_lock::key("deals", deal_id)], ctx.write.lock_wait)
        .await
    else {
        return record_busy();
    };
    let (catalog, _) = match ctx.catalog.get(client).await {
        Ok(c) => c,
        Err(e) => {
            count_rejection(&format!("catalog:{}", e.error_kind()));
            return hubspot_error_response(&e);
        }
    };
    let mut steps = match build_steps(client, &catalog, deal_id, req).await {
        Ok(s) => s,
        Err((status, v)) => {
            count_rejection(v["error_kind"].as_str().unwrap_or("validation"));
            return (status, Json(v)).into_response();
        }
    };
    // HubSpot の呼び出し枠 (読み取り + PATCH をオブジェクトごとに) が今の混み具合で足りるか、読む前に確かめる。
    // 足りないのに読むと、読み取りだけ HubSpot に飛んで PATCH が断られる (負荷試験: PATCH 1 回に読み取り 4.6 回)
    if client
        .gateway()
        .admit(
            Lane::General,
            client.priority(),
            (steps.len() as u32).saturating_mul(2),
        )
        .is_err()
    {
        count_rejection("hubspot_busy");
        return hubspot_error_response(&HubSpotError::Busy);
    }
    // 担当者・会社も同じ要求の中で書くので、その分も (整列した順に) 取る
    if held
        .extend(
            steps.iter().map(|s| record_lock::key(&s.object, &s.id)),
            ctx.write.lock_wait,
        )
        .await
        .is_err()
    {
        return record_busy();
    }

    // 事前確認 (現在値の読み取り → 競合・必須項目)
    let mut prepared = Vec::new();
    for s in &steps {
        match prepare(client, s).await {
            Ok(p) => prepared.push(p),
            Err(stop) => {
                // 何も書かない終わり方 (競合など)。監査の書き込みを待たせないよう、先に鍵を放す
                drop(held);
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
                .await;
            }
        }
    }

    // 受付の上限: 未完了の操作が多すぎるときは、台帳にも HubSpot にも触らずに断る (何も保存されていない)
    if !reuse_failed {
        let active = pending::blocking(audit, pending::count_active)
            .await
            .ok()
            .and_then(Result::ok);
        if active.is_some_and(|n| n >= ctx.write.pending_max) {
            count_rejection("queue_full");
            return err(StatusCode::SERVICE_UNAVAILABLE, "queue_full");
        }
    }

    // ここから先は取り消せない (台帳に記録して送る)。締め切りで取り下げられていたら、何もせずに終わる
    if !gate.commit() {
        return timeout_response();
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
        move |t| {
            if reuse_failed {
                pending::reset_failed_op(t, &op, &pl, &refs)
            } else {
                pending::insert_op(t, &op, &who, &did, &pl, &refs)
            }
        }
    })
    .await;
    match ins {
        Ok(Ok(())) => {
            note_status(
                &ctx.write,
                &req.operation_id,
                operator,
                "in_progress",
                0,
                "",
                "",
            );
        }
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
            count_rejection("queue_unavailable");
            // 締め切りで 202 を返した後でも、状態の照会が「失敗」と答えられるようにする
            note_status(
                &ctx.write,
                &req.operation_id,
                operator,
                "failed",
                0,
                "queue_unavailable",
                "",
            );
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

    // 送ったあとの曖昧な失敗 (タイムアウト・5xx・通信断・応答が読めない) は、書けたかどうか分からない。
    // 再送待ちが満杯のときに「失敗」と断定すると、書けていた場合に利用者の画面と HubSpot が食い違う
    // (負荷試験: 失敗と答えたのに入っていた「幻」)。HubSpot を読み直して確かめ、確かめられなければ上限を超えても積む
    if matches!(outcome, Err(Stop::Transient(_))) {
        let full = pending::blocking(audit, pending::count_pending)
            .await
            .ok()
            .and_then(Result::ok)
            .is_some_and(|n| n >= ctx.write.pending_max);
        if full {
            match verify_after_send(client, &mut steps, &mut run).await {
                Verified::Saved => outcome = Ok(()),
                Verified::NotApplied => {
                    let payload = Payload { steps };
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
                    let body =
                        json!({"error": "queue_full", "error_kind": "queue_full"}).to_string();
                    let res = pending::blocking(audit, {
                        let (op, pl) = (req.operation_id.clone(), payload.clone());
                        move |t| {
                            pending::update_op(
                                t,
                                &op,
                                "failed",
                                1,
                                "queue_full",
                                503,
                                &body,
                                "",
                                &pl,
                            )
                        }
                    })
                    .await;
                    if matches!(res, Ok(Ok(()))) {
                        note_status(
                            &ctx.write,
                            &req.operation_id,
                            operator,
                            "failed",
                            1,
                            "queue_full",
                            "",
                        );
                    }
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
                Verified::Unknown => {
                    tracing::warn!(
                        operation_id = %req.operation_id,
                        "crm write: could not verify an ambiguous send while the queue is full; queueing it anyway"
                    );
                }
            }
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
            if matches!(res, Ok(Ok(()))) {
                note_status(&ctx.write, &req.operation_id, operator, "saved", 1, "", "");
            } else {
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
            let next = pending::iso(Utc::now() + pending::backoff_after(1));
            let code = e.error_kind().to_string();
            let res = pending::blocking(audit, {
                let (op, pl, code, next) = (
                    req.operation_id.clone(),
                    payload.clone(),
                    code.clone(),
                    next.clone(),
                );
                move |t| pending::update_op(t, &op, "pending", 1, &code, 0, "", &next, &pl)
            })
            .await;
            if !matches!(res, Ok(Ok(()))) {
                // 再送待ちに積めなかった。台帳の行は送信中のまま残り、止まった送信中として worker が後で
                // 拾って再送しうる。「何も保存されていない」とは言えない (途中まで書けた分もある) ので、
                // 確認できない扱いにする (同じ operation_id で再送すると、台帳の行の結果が返る)
                tracing::warn!("crm write: ledger update (pending) failed: {res:?}");
                return err(StatusCode::SERVICE_UNAVAILABLE, "queue_uncertain");
            }
            note_status(
                &ctx.write,
                &req.operation_id,
                operator,
                "pending",
                1,
                &code,
                &next,
            );
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
            // 先の段が書けていたら、書けた分を本文に付ける (画面が「何も保存されていない」と誤らない)
            let partial =
                (!run.values.is_empty() || !run.other.is_empty()).then(|| CrmPatchPartial {
                    values: run.values.clone(),
                    objects_values: run.other.clone(),
                });
            stop_response_recorded(
                stop,
                &ctx.write,
                audit,
                account_id,
                session_id,
                operator,
                &req.operation_id,
                deal_id,
                &payload,
                1,
                partial,
            )
            .await
        }
    }
}

/// 送ったあとの曖昧な失敗のあと、HubSpot を読み直した結果
enum Verified {
    /// 残りの段は全部すでに書けていた (保存済み)
    Saved,
    /// 書けていない (競合・必須の不足も「書けていない」)
    NotApplied,
    /// 読み直せなかった (確かめられない)
    Unknown,
}

/// まだ終わっていない段を読み直し、書く値がすでに HubSpot にあるか確かめる (PATCH はしない)。
/// 全部あれば段を完了にして `run` に積む
async fn verify_after_send(client: &HubSpotClient, steps: &mut [Step], run: &mut Run) -> Verified {
    let mut found: Vec<(usize, Prepared)> = Vec::new();
    for (i, s) in steps.iter().enumerate() {
        if s.done {
            continue;
        }
        match prepare(client, s).await {
            Ok(p) if p.already => found.push((i, p)),
            Ok(_) | Err(Stop::Conflict { .. }) | Err(Stop::Missing(_)) => {
                return Verified::NotApplied
            }
            Err(Stop::Transient(_)) | Err(Stop::Permanent(_)) => return Verified::Unknown,
        }
    }
    for (i, p) in found {
        let intended = intended_changes(std::slice::from_ref(&steps[i]), None);
        if send(client, &mut steps[i], &p, run).await.is_err() {
            return Verified::Unknown;
        }
        run.changes.extend(intended);
    }
    Verified::Saved
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
        // 読み取りに失敗した・必須項目が足りない: 何も送っていない「断り」。監査には書かず、種類別に数えるだけ
        Stop::Transient(e) | Stop::Permanent(e) => {
            count_rejection(&format!("read_failed:{}", e.error_kind()));
            hubspot_error_response(&e)
        }
        Stop::Missing(_) => {
            count_rejection("missing_required");
            (status, Json(body)).into_response()
        }
        // 競合 (409): 誰がどの値を上書きしようとして止められたかは残す
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
    write: &WriteConfig,
    audit: &AuditDb,
    account_id: &str,
    session_id: &str,
    operator: &str,
    operation_id: &str,
    deal_id: &str,
    payload: &Payload,
    attempts: i64,
    partial: Option<CrmPatchPartial>,
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
    let mut body = body;
    if let (Some(p), Some(obj)) = (&partial, body.as_object_mut()) {
        obj.insert("partial".into(), json!(p));
    }
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
    if matches!(res, Ok(Ok(()))) {
        note_status(write, operation_id, operator, "failed", attempts, &code, "");
    } else {
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
    api_status_of(&row.status, row.attempts)
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
    // まずプロセス内の表から返す (202 のあとの照会が Turso の SELECT にならない)。
    // 表に無い (再起動した・期限が切れた・表から押し出された) ときだけ台帳を 1 回読み、表に入れ直す
    let st = match ctx.write.statuses.get(&operation_id) {
        Some(st) => {
            cache_hit("op_status");
            st
        }
        None => {
            cache_miss("op_status");
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
            let st = OpState {
                operator_email: row.operator_email.clone(),
                status: api_status(&row).to_string(),
                attempts: row.attempts,
                last_error_code: row.last_error_code.clone(),
                next_retry_at: row.next_retry_at.clone(),
            };
            ctx.write.statuses.put(&operation_id, st.clone());
            st
        }
    };
    let is_admin = principal.role.is_some_and(|r| r.is_admin());
    if st.operator_email != principal.email.clone().unwrap_or_default() && !is_admin {
        return err(StatusCode::FORBIDDEN, "forbidden");
    }
    let opt = |s: &str| (!s.is_empty()).then(|| s.to_string());
    Json(CrmOperationStatus {
        operation_id,
        status: st.status.clone(),
        attempts: st.attempts,
        last_error_code: opt(&st.last_error_code),
        next_retry_at: opt(&st.next_retry_at),
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
    // 管理者の操作も状態の表に写す (本番の表は要求側・worker と共有のもの)
    op_status::shared().put(
        &row.operation_id,
        OpState {
            operator_email: row.operator_email.clone(),
            status: if retry { "pending" } else { "failed" }.to_string(),
            attempts: if retry { 0 } else { row.attempts },
            last_error_code: if retry {
                String::new()
            } else {
                row.last_error_code.clone()
            },
            next_retry_at: String::new(),
        },
    );
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

/// 期限が来た操作を 1 件処理する。結果は台帳と監査に残す。
/// 実際に送ろうとした (または栓で失敗にした) なら true、見送った (鍵が取れない・もう終わっている) なら false
pub async fn process_op(
    audit: &AuditDb,
    client: &HubSpotClient,
    write: &WriteConfig,
    row: &pending::OpRow,
) -> bool {
    // 栓: 受付のあとで書き込みを閉じたら、積んである操作も送らない (管理者の再試行も同じ。開け直してから再試行する)
    if !write.allows(&row.deal_id) {
        let body = json!({"error": "writes_disabled"}).to_string();
        let (a, op, pl) = (audit.clone(), row.operation_id.clone(), row.payload.clone());
        let r = pending::blocking(&a, move |t| {
            pending::update_op(t, &op, "failed", 1, "writes_disabled", 403, &body, "", &pl)
        })
        .await;
        if matches!(r, Ok(Ok(()))) {
            note_status(
                write,
                &row.operation_id,
                &row.operator_email,
                "failed",
                1,
                "writes_disabled",
                "",
            );
        } else {
            tracing::warn!("crm write worker: ledger update (writes_disabled) failed: {r:?}");
        }
        let account = row.operator_email.clone();
        audit_write(
            audit,
            &account,
            "",
            &row.operator_email,
            &row.operation_id,
            &row.deal_id,
            "failed:writes_disabled",
            &intended_changes(&row.payload.steps, None),
        )
        .await;
        return true;
    }
    // 受付側と同じ鍵 (案件 + 担当者・会社) を取ってから送る。取れなければ今回は見送り、次の周回でやり直す
    let _held = match write
        .locks
        .acquire(
            std::iter::once(record_lock::key("deals", &row.deal_id)).chain(
                row.payload
                    .steps
                    .iter()
                    .map(|s| record_lock::key(&s.object, &s.id)),
            ),
            write.lock_wait,
        )
        .await
    {
        Ok(h) => h,
        Err(_) => {
            tracing::warn!(
                operation_id = %row.operation_id,
                "crm write worker: record busy, retry on the next round"
            );
            return false;
        }
    };
    // 鍵を待っている間に台帳の行が変わった (管理者が破棄した・受付側が保存した) かもしれない。最新を読み直す
    let fresh_row = match pending::blocking(audit, {
        let op = row.operation_id.clone();
        move |t| pending::get_op(t, &op)
    })
    .await
    {
        Ok(Ok(Some(r))) => r,
        _ => row.clone(),
    };
    if !matches!(fresh_row.status.as_str(), "pending" | "in_progress") {
        return false;
    }
    let row = &fresh_row;
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
    // 最後の試行でも曖昧な失敗なら、「失敗」と断定する前に HubSpot を読み直して確かめる
    // (書けていれば保存済み。確かめられなければ失敗にせず、次の周期でもう一度)
    let mut unverified = false;
    if attempts >= pending::MAX_ATTEMPTS && matches!(result, Err(Stop::Transient(_))) {
        match verify_after_send(client, &mut payload.steps, &mut run).await {
            Verified::Saved => result = Ok(()),
            Verified::NotApplied => {}
            Verified::Unknown => unverified = true,
        }
    }
    // 監査の account_id (メールで引く) は、監査を書くときだけ引く
    let log = |outcome: String, changes: Vec<Change>| {
        let (a, who, op, did) = (
            audit.clone(),
            row.operator_email.clone(),
            row.operation_id.clone(),
            row.deal_id.clone(),
        );
        async move {
            let account = {
                let (a2, email) = (a.clone(), who.clone());
                pending::blocking(&a2, move |t| {
                    crate::audit::dao::find_account_by_email(t, &email).map(|a| a.id)
                })
                .await
                .ok()
                .flatten()
                .unwrap_or_else(|| who.clone())
            };
            audit_write(&a, &account, "", &who, &op, &did, &outcome, &changes).await
        }
    };
    let update = |status: &'static str,
                  code: String,
                  http: i64,
                  body: String,
                  next: String,
                  pl: Payload,
                  att: i64| {
        let op = row.operation_id.clone();
        let (who, code2, next2) = (row.operator_email.clone(), code.clone(), next.clone());
        async move {
            let r = pending::blocking(audit, {
                let op = op.clone();
                move |t| pending::update_op(t, &op, status, att, &code, http, &body, &next, &pl)
            })
            .await;
            if matches!(r, Ok(Ok(()))) {
                note_status(write, &op, &who, status, att, &code2, &next2);
            } else {
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
            if attempts >= pending::MAX_ATTEMPTS && !unverified {
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
    true
}

/// 再送 worker が同時に進める案件の数 (同じ案件の操作は古い順に 1 つずつ)
pub const WORKER_CONCURRENCY: usize = 4;
/// 1 周で取る期限切れの操作の数
pub const DUE_BATCH: i64 = 40;

/// 待っている案件の束を 1 つ取る (ロックは返す前に放す。await をまたがない)
fn next_group(
    queue: &Mutex<std::collections::VecDeque<Vec<pending::OpRow>>>,
) -> Option<Vec<pending::OpRow>> {
    queue.lock().ok().and_then(|mut q| q.pop_front())
}

/// 1 周の結果
pub struct Round {
    /// 次に期限が来る時刻 (再送待ちが無ければ None)
    pub next: Option<chrono::DateTime<Utc>>,
    /// 取った件数が上限いっぱい、かつ実際に処理した = すぐ次の周回をしてよい
    pub more: bool,
}

/// 1 周: 期限が来た操作を処理する。案件ごとに束ね (同じ案件は古い順に直列)、案件どうしは最大
/// [`WORKER_CONCURRENCY`] 件を同時に進める (直列だと 1 件あたり HubSpot 2 回 + Turso 数回で、約 0.8 件/秒しか減らなかった)。
/// HubSpot への呼び出しは背景の優先度のままなので、関所では画面の操作が先に通る
pub async fn run_due_round(audit: &AuditDb, client: &HubSpotClient, write: &WriteConfig) -> Round {
    let now = pending::now_iso();
    let rows = pending::blocking(audit, {
        let now = now.clone();
        move |t| pending::due_ops(t, &now, DUE_BATCH)
    })
    .await
    .ok()
    .and_then(Result::ok)
    .unwrap_or_default();
    let batch_full = rows.len() as i64 >= DUE_BATCH;
    let mut groups: Vec<Vec<pending::OpRow>> = Vec::new();
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for r in rows {
        match index.get(&r.deal_id) {
            Some(i) => groups[*i].push(r),
            None => {
                index.insert(r.deal_id.clone(), groups.len());
                groups.push(vec![r]);
            }
        }
    }
    let queue = Arc::new(Mutex::new(std::collections::VecDeque::from(groups)));
    let handled = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..WORKER_CONCURRENCY {
        let (queue, handled) = (queue.clone(), handled.clone());
        let (audit, client, write) = (audit.clone(), client.clone(), write.clone());
        set.spawn(async move {
            while let Some(group) = next_group(&queue) {
                for r in &group {
                    if process_op(&audit, &client, &write, r).await {
                        handled.fetch_add(1, Ordering::Relaxed);
                    }
                }
            }
        });
    }
    while set.join_next().await.is_some() {}
    let next = pending::blocking(audit, pending::next_due)
        .await
        .ok()
        .and_then(Result::ok)
        .flatten()
        .and_then(|t| chrono::DateTime::parse_from_rfc3339(&t).ok())
        .map(|d| d.with_timezone(&Utc));
    Round {
        next,
        more: batch_full && handled.load(Ordering::Relaxed) > 0,
    }
}

/// 1 周: 期限が来た操作を処理して、次に期限が来る時刻を返す (再送待ちが無ければ None)
pub async fn run_due(
    audit: &AuditDb,
    client: &HubSpotClient,
    write: &WriteConfig,
) -> Option<chrono::DateTime<Utc>> {
    run_due_round(audit, client, write).await.next
}

/// 再送 worker を始める (背景の優先度で HubSpot を呼ぶ。書き込みでポーリングしない)
pub fn spawn_worker(state: Arc<AppState>) {
    let (Some(audit), Some(client)) = (state.audit.clone(), state.hubspot.clone()) else {
        return;
    };
    let client = client.background();
    let write = WriteConfig::from_env();
    let wake = wake_handle();
    tokio::spawn(async move {
        // 起動直後は他の初期化を邪魔しない (E2E は debug ビルドだけ `CRM_WORKER_START_DELAY_SECS_DEBUG` で縮められる)
        let start_delay =
            pending::debug_override_secs("CRM_WORKER_START_DELAY_SECS_DEBUG").unwrap_or(30);
        tokio::time::sleep(Duration::from_secs(start_delay as u64)).await;
        let mut last_purge = std::time::Instant::now() - Duration::from_secs(86_400);
        loop {
            let round = run_due_round(&audit, &client, &write).await;
            if last_purge.elapsed() >= Duration::from_secs(86_400) {
                last_purge = std::time::Instant::now();
                let _ = pending::blocking(&audit, pending::purge_old).await;
            }
            // 期限切れがまだ残っていて前進している間は待たずに次の周回へ。そうでなければ次の期限まで眠る
            // (鍵が取れずに見送った行で空回りしないよう、最短 2 秒)
            if round.more {
                continue;
            }
            let wait = match round.next {
                Some(t) => (t - Utc::now())
                    .to_std()
                    .unwrap_or(Duration::from_secs(2))
                    .clamp(Duration::from_secs(2), Duration::from_secs(300)),
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
