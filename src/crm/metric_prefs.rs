//! 架電画面の成績の帯に出す項目の個人設定 (ADR-028)。
//!
//! - `GET /api/crm/metrics/prefs`: 本人の設定。保存していなければ既定 (今の帯と同じ 4 項目)。
//!   監査 DB が使えないときも 200 で既定を返し、`available: false` と 1 行の案内を付ける (帯は既定で出る)。
//! - `PUT /api/crm/metrics/prefs`: 本人の設定を 1 行で保存する (上書き)。監査 DB が使えなければ 503。
//!
//! 設定は既存の監査 Turso の `crm_metric_prefs` (キー = メール、値 = JSON 文字列) に 1 人 1 行。
//! 書き込みは保存のときだけ。読み取りは 60 秒キャッシュし、保存したら本人の分を捨てる。
//! 自分の項目の定義 (条件・数え方・「誰の」) は保存の前に取引の項目の一覧で確かめる。
//! 条件の検証は詳細条件と同じ `queue_property_filter::resolve` (規則を 2 か所に書かない)。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

use axum::{
    body::Bytes,
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use tokio::sync::Mutex;
use tower_sessions::Session;
use ts_rs::TS;

use super::property_catalog::CatalogEntry;
use super::queue_property_filter::{self, PropertyCondition, ValueScope};
use super::rbac;
use super::routes::{error_json, hubspot_error_response, CrmCtx, CRM_REQUEST_DEADLINE};
use crate::audit::AuditDb;
use crate::AppState;

/// 帯にもともとある項目 (画面の並びの既定はこの順ではなく `DEFAULT_ITEMS`)
pub const BUILTIN_KEYS: &[&str] = &[
    "outbound",
    "inbound",
    "over_30s",
    "over_60s",
    "over_90s",
    "over_360s",
    "duration",
    "avg_duration",
    "appointments",
];
/// 設定が無い人の帯 (今までの帯と同じ)
pub const DEFAULT_ITEMS: &[&str] = &["outbound", "over_90s", "duration", "appointments"];
/// 自分の項目は 1 人 6 個まで
pub const MAX_CUSTOM: usize = 6;
/// 自分の項目の条件の数。HubSpot の Search は 1 つのまとまりに 6 条件までで、
/// 「誰の」(担当者 IN 比べる相手) に 1 つ使うので、自分で足せるのは 5 つ
pub const MAX_CUSTOM_CONDITIONS: usize = 5;
/// 帯に出す名前の長さ
pub const MAX_NAME_CHARS: usize = 12;
/// 保存する JSON の大きさ (1 人 1 行)
pub const MAX_PREFS_BYTES: usize = 16_000;
/// 「誰の」の既定 = 取引の担当者
pub const DEFAULT_OWNER_PROPERTY: &str = "hubspot_owner_id";
/// 読み取りのキャッシュ
pub const PREFS_TTL: Duration = Duration::from_secs(60);

/// 自分の項目の数え方
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum MetricKind {
    /// 条件に合う取引の数
    Count,
    /// 条件に合う取引の、選んだ数値の項目の合計
    Sum,
    /// 同じく平均 (値が空の取引は数えない)
    Avg,
}

/// 自分の項目 1 つの定義
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct CustomMetricDef {
    /// 画面が作る識別子 (`c-` + 英小文字・数字)。帯の並び (`items`) で使う
    pub id: String,
    /// 帯に出す短い名前
    pub name: String,
    pub kind: MetricKind,
    /// 合計・平均にする数値の項目 (件数のときは無し)
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[ts(optional)]
    pub property: Option<String>,
    /// 取引の項目の条件 (すべてに合うもの)。詳細条件と同じ形
    #[serde(default)]
    pub conditions: Vec<PropertyCondition>,
    /// 「誰の」取引か (担当者を選ぶ型の項目)。既定は取引の担当者
    #[serde(default = "default_owner_property")]
    pub owner_property: String,
}

fn default_owner_property() -> String {
    DEFAULT_OWNER_PROPERTY.to_string()
}

/// 帯の設定 (1 人分)
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(deny_unknown_fields)]
pub struct MetricPrefs {
    /// 帯に出す項目の並び。もともとの項目のキーか、自分の項目の `id`。ここに無い項目は出さない
    pub items: Vec<String>,
    #[serde(default)]
    pub custom: Vec<CustomMetricDef>,
}

impl Default for MetricPrefs {
    fn default() -> Self {
        Self {
            items: DEFAULT_ITEMS.iter().map(|s| (*s).to_string()).collect(),
            custom: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
pub struct MetricPrefsResponse {
    pub prefs: MetricPrefs,
    /// 本人が保存した設定か (false = 既定)
    pub saved: bool,
    /// false のとき設定を読み書きできない (帯は既定で出す)。`message` に画面に出せる 1 行
    pub available: bool,
    pub message: Option<String>,
}

pub const UNAVAILABLE_MESSAGE: &str =
    "いまは項目の設定を読み書きできません。帯は標準の項目で表示しています。";

// ---------------------------------------------------------------------------
// 形の検証 (HubSpot を呼ばない)
// ---------------------------------------------------------------------------

fn valid_id(id: &str) -> bool {
    id.len() > 2
        && id.len() <= 34
        && id.starts_with("c-")
        && id[2..]
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
}

/// 画面に出せる理由つきの 400
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Invalid(pub String);

impl Invalid {
    fn new(m: impl Into<String>) -> Self {
        Self(m.into())
    }
}

/// 形だけを確かめる (項目の型は [`validate_against_catalog`])
pub fn validate_shape(p: &MetricPrefs) -> Result<(), Invalid> {
    if p.custom.len() > MAX_CUSTOM {
        return Err(Invalid::new("自分の項目は 6 個までです。"));
    }
    let mut ids = HashSet::new();
    for c in &p.custom {
        if !valid_id(&c.id) || !ids.insert(c.id.as_str()) {
            return Err(Invalid::new("項目の識別子が正しくありません。"));
        }
        let name = c.name.trim();
        if name.is_empty()
            || name.chars().count() > MAX_NAME_CHARS
            || name.chars().any(char::is_control)
        {
            return Err(Invalid::new("名前は 1〜12 文字で付けてください。"));
        }
        if c.conditions.len() > MAX_CUSTOM_CONDITIONS {
            return Err(Invalid::new("条件は 1 つの項目に 5 個までです。"));
        }
        // 1 つずつの条件の形 (長さ・選択肢の数) は詳細条件と同じ検証に任せる
        let raw = serde_json::to_string(&c.conditions)
            .map_err(|_| Invalid::new("条件の指定が正しくありません。"))?;
        queue_property_filter::parse(&raw).map_err(Invalid::new)?;
        match c.kind {
            MetricKind::Count => {
                if c.property.is_some() {
                    return Err(Invalid::new("件数には数値の項目を選びません。"));
                }
            }
            MetricKind::Sum | MetricKind::Avg => {
                if c.property.as_deref().is_none_or(str::is_empty) {
                    return Err(Invalid::new("合計・平均にする数値の項目を選んでください。"));
                }
            }
        }
        if c.owner_property.is_empty() || c.owner_property.len() > 200 {
            return Err(Invalid::new("「誰の」取引かを選んでください。"));
        }
    }
    if p.items.len() > BUILTIN_KEYS.len() + MAX_CUSTOM {
        return Err(Invalid::new("帯の項目が多すぎます。"));
    }
    let mut seen = HashSet::new();
    for it in &p.items {
        let known = BUILTIN_KEYS.contains(&it.as_str()) || ids.contains(it.as_str());
        if !known || !seen.insert(it.as_str()) {
            return Err(Invalid::new("帯の項目の並びが正しくありません。"));
        }
    }
    Ok(())
}

/// 詳細条件で選べる値の範囲 (取引ステージは架電キューの全パイプラインのもの)
pub fn value_scope() -> ValueScope {
    let mut stages: Vec<String> = super::queue_pipelines::QUEUE_PIPELINES
        .iter()
        .flat_map(|p| p.stages.iter().map(|(id, _)| (*id).to_string()))
        .collect();
    stages.sort();
    stages.dedup();
    ValueScope {
        stages,
        pipelines: super::queue_pipelines::QUEUE_PIPELINES
            .iter()
            .map(|p| p.id.to_string())
            .collect(),
    }
}

/// 項目 1 つを、HubSpot の Search の条件に直したもの (相対日付は `now` で解決済み)
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedMetric {
    pub filters: Vec<Value>,
    pub kind: MetricKind,
    pub property: Option<String>,
    pub owner_property: String,
}

/// 定義を取引の項目の一覧と照らして Search の条件に直す。不正なら画面に出せる理由
pub fn resolve_metric(
    c: &CustomMetricDef,
    catalog: &CatalogEntry,
    now: DateTime<Utc>,
    scope: &ValueScope,
) -> Result<ResolvedMetric, Invalid> {
    let owner = catalog
        .property("deals", &c.owner_property)
        .filter(|p| p.owner_ref)
        .ok_or_else(|| Invalid::new("「誰の」には担当者を選ぶ型の項目を選んでください。"))?;
    if let Some(name) = &c.property {
        let ok = catalog
            .property("deals", name)
            .is_some_and(|p| p.property_type == "number");
        if !ok {
            return Err(Invalid::new("合計・平均には数値の項目を選んでください。"));
        }
    }
    let mut filters = Vec::with_capacity(c.conditions.len());
    for cond in &c.conditions {
        let f = queue_property_filter::resolve(
            cond,
            catalog.property("deals", &cond.property),
            now,
            scope,
        )
        .map_err(Invalid::new)?;
        filters.push(f);
    }
    Ok(ResolvedMetric {
        filters,
        kind: c.kind,
        property: c.property.clone(),
        owner_property: owner.name.clone(),
    })
}

/// 形と項目の型の両方を確かめる
pub fn validate_against_catalog(
    p: &MetricPrefs,
    catalog: &CatalogEntry,
    now: DateTime<Utc>,
) -> Result<(), Invalid> {
    validate_shape(p)?;
    let scope = value_scope();
    for c in &p.custom {
        resolve_metric(c, catalog, now, &scope)
            .map_err(|Invalid(m)| Invalid(format!("「{}」: {m}", c.name.trim())))?;
    }
    Ok(())
}

/// 保存の前に名前の前後の空白を落とす
pub fn normalize(mut p: MetricPrefs) -> MetricPrefs {
    for c in &mut p.custom {
        c.name = c.name.trim().to_string();
    }
    p
}

// ---------------------------------------------------------------------------
// 保存 (監査 Turso)
// ---------------------------------------------------------------------------

#[derive(Debug)]
pub enum StoreError {
    Unavailable,
}

type Cache = HashMap<String, (Instant, Option<MetricPrefs>)>;

#[derive(Default)]
pub struct PrefsStore {
    cache: Arc<Mutex<Cache>>,
}

fn read_row(audit: &AuditDb, email: &str) -> Result<Option<MetricPrefs>, StoreError> {
    let rows = audit
        .turso()
        .query(
            "SELECT prefs FROM crm_metric_prefs WHERE email=?",
            &[&email],
        )
        .map_err(|_| StoreError::Unavailable)?;
    let Some(row) = rows.into_iter().next() else {
        return Ok(None);
    };
    let raw = row.get("prefs").and_then(Value::as_str).unwrap_or_default();
    // 読めない行 (手で壊した等) は既定に戻す。保存し直せば上書きされる
    Ok(serde_json::from_str::<MetricPrefs>(raw)
        .ok()
        .filter(|p| validate_shape(p).is_ok()))
}

impl PrefsStore {
    /// 本人の設定 (60 秒キャッシュ)。保存していなければ None
    pub async fn get(
        &self,
        audit: &AuditDb,
        email: &str,
    ) -> Result<Option<MetricPrefs>, StoreError> {
        {
            let cache = self.cache.lock().await;
            if let Some((at, v)) = cache.get(email) {
                if at.elapsed() < PREFS_TTL {
                    return Ok(v.clone());
                }
            }
        }
        let a = audit.clone();
        let e = email.to_string();
        let v = tokio::task::spawn_blocking(move || read_row(&a, &e))
            .await
            .map_err(|_| StoreError::Unavailable)??;
        let mut cache = self.cache.lock().await;
        if cache.len() >= 1024 {
            cache.retain(|_, (at, _)| at.elapsed() < PREFS_TTL);
            if cache.len() >= 1024 {
                cache.clear();
            }
        }
        cache.insert(email.to_string(), (Instant::now(), v.clone()));
        Ok(v)
    }

    /// 1 行で上書き保存し、本人のキャッシュを捨てる
    pub async fn put(
        &self,
        audit: &AuditDb,
        email: &str,
        prefs: &MetricPrefs,
    ) -> Result<(), StoreError> {
        let raw = serde_json::to_string(prefs).map_err(|_| StoreError::Unavailable)?;
        let a = audit.clone();
        let e = email.to_string();
        let now = Utc::now().to_rfc3339();
        let res = tokio::task::spawn_blocking(move || {
            a.turso()
                .execute(
                    "INSERT INTO crm_metric_prefs (email, prefs, updated_at) VALUES (?, ?, ?)
                     ON CONFLICT(email) DO UPDATE SET prefs=excluded.prefs, updated_at=excluded.updated_at",
                    &[&e, &raw, &now],
                )
                .map(|_| ())
                .map_err(|_| StoreError::Unavailable)
        })
        .await
        .map_err(|_| StoreError::Unavailable)?;
        // 書けたかどうか分からない (失敗) ときもキャッシュは捨てる (次の読み取りで確かめる)
        self.cache.lock().await.remove(email);
        res
    }
}

// ---------------------------------------------------------------------------
// ハンドラ
// ---------------------------------------------------------------------------

fn email_of(p: &rbac::Principal) -> Option<String> {
    let e = p.email.clone().unwrap_or_default().trim().to_lowercase();
    (!e.is_empty()).then_some(e)
}

/// 本人の設定を読む (帯と自分の項目の数字の両方が使う)。監査 DB が使えなければ Err
pub(super) async fn load(
    state: &AppState,
    ctx: &CrmCtx,
    email: &str,
) -> Result<Option<MetricPrefs>, StoreError> {
    let Some(audit) = state.audit.as_ref() else {
        return Err(StoreError::Unavailable);
    };
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.metric_prefs.get(audit, email)).await {
        Ok(r) => r,
        Err(_) => Err(StoreError::Unavailable),
    }
}

pub(super) async fn get_prefs(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(d) => return d.into_response(),
    };
    let Some(email) = email_of(&principal) else {
        return error_json(StatusCode::UNAUTHORIZED, "login_required");
    };
    let body = match load(&state, &ctx, &email).await {
        Ok(Some(prefs)) => MetricPrefsResponse {
            prefs,
            saved: true,
            available: true,
            message: None,
        },
        Ok(None) => MetricPrefsResponse {
            prefs: MetricPrefs::default(),
            saved: false,
            available: true,
            message: None,
        },
        Err(StoreError::Unavailable) => MetricPrefsResponse {
            prefs: MetricPrefs::default(),
            saved: false,
            available: false,
            message: Some(UNAVAILABLE_MESSAGE.to_string()),
        },
    };
    Json(body).into_response()
}

fn invalid_response(m: &str) -> Response {
    (
        StatusCode::BAD_REQUEST,
        Json(json!({"error_kind": "invalid_metric_prefs", "message": m})),
    )
        .into_response()
}

pub(super) async fn put_prefs(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    body: Bytes,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(d) => return d.into_response(),
    };
    let Some(email) = email_of(&principal) else {
        return error_json(StatusCode::UNAUTHORIZED, "login_required");
    };
    if body.len() > MAX_PREFS_BYTES {
        return invalid_response("設定が大きすぎます。条件か項目を減らしてください。");
    }
    let prefs: MetricPrefs = match serde_json::from_slice(&body) {
        Ok(p) => normalize(p),
        Err(_) => return invalid_response("設定の形が正しくありません。"),
    };
    if let Err(Invalid(m)) = validate_shape(&prefs) {
        return invalid_response(&m);
    }
    let Some(audit) = state.audit.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "prefs_unavailable");
    };
    // 自分の項目があれば、取引の項目の一覧 (6 時間キャッシュ) で型を確かめる
    if !prefs.custom.is_empty() {
        let Some(client) = state.crm_hubspot() else {
            return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
        };
        let entry = match tokio::time::timeout(CRM_REQUEST_DEADLINE, ctx.catalog.get(&client)).await
        {
            Ok(Ok((entry, _))) => entry,
            Ok(Err(e)) => return hubspot_error_response(&e),
            Err(_) => return super::routes::timeout_response(),
        };
        if let Err(Invalid(m)) = validate_against_catalog(&prefs, &entry, ctx.queue.now()) {
            return invalid_response(&m);
        }
    }
    match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        ctx.metric_prefs.put(&audit, &email, &prefs),
    )
    .await
    {
        Ok(Ok(())) => Json(MetricPrefsResponse {
            prefs,
            saved: true,
            available: true,
            message: None,
        })
        .into_response(),
        _ => error_json(StatusCode::SERVICE_UNAVAILABLE, "prefs_unavailable"),
    }
}
