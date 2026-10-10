//! 明示操作だけが書く。HubSpotが正本、監査DBには再送・冪等性の台帳のみ置く。
use super::{authorized_user, Access, JobReadService, ReadError};
use crate::{
    audit::AuditDb,
    crm::{pending, record_lock, write::WriteRateLimiter},
    job_gen::drafts::{self, DraftSnapshot, ReviewDraftRequest, SaveDraftRequest},
    AppState,
};
use axum::{
    extract::{DefaultBodyLimit, Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Extension, Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::{Arc, OnceLock},
    time::Duration,
};
use tower_sessions::Session;

fn wake_worker() -> &'static tokio::sync::Notify {
    static WAKE: OnceLock<tokio::sync::Notify> = OnceLock::new();
    WAKE.get_or_init(tokio::sync::Notify::new)
}

#[derive(Clone)]
pub(super) struct Writes {
    enabled: bool,
    allowlist: BTreeSet<String>,
    limiter: Arc<WriteRateLimiter>,
}
impl Writes {
    pub(super) fn from_env() -> Self {
        Self {
            enabled: std::env::var("JOB_COPY_DRAFT_WRITES_ENABLED").is_ok_and(|v| v == "true"),
            allowlist: std::env::var("JOB_COPY_DRAFT_WRITE_ALLOWLIST")
                .unwrap_or_default()
                .split(',')
                .map(str::trim)
                .filter(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit()))
                .map(str::to_owned)
                .collect(),
            limiter: Arc::new(WriteRateLimiter::new(200)),
        }
    }
    pub(super) fn allows(&self, id: &str) -> bool {
        self.enabled || self.allowlist.contains(id)
    }
}
#[derive(Clone, Serialize, Deserialize)]
struct Operation {
    id: String,
    actor: String,
    listing: String,
    fingerprint: String,
    base: String,
    snapshot: DraftSnapshot,
    status: String,
    attempts: i64,
    code: String,
}
fn err(status: StatusCode, code: &'static str) -> ReadError {
    ReadError(status, code)
}
fn unavailable() -> ReadError {
    err(StatusCode::SERVICE_UNAVAILABLE, "draft_queue_unavailable")
}
fn operation_response(op: &Operation) -> Response {
    let status = match op.status.as_str() {
        "saved" => StatusCode::OK,
        "failed" => StatusCode::CONFLICT,
        _ => StatusCode::ACCEPTED,
    };
    let body = drafts::DraftOperationResponse {
        status: match op.status.as_str() {
            "saved" => drafts::DraftOperationStatus::Saved,
            "failed" => drafts::DraftOperationStatus::Failed,
            _ => drafts::DraftOperationStatus::Pending,
        },
        code: op.code.clone(),
        operation_id: op.id.clone(),
        draft: (op.status == "saved").then(|| op.snapshot.clone()),
        revision: (op.status == "saved")
            .then(|| drafts::revision(&op.snapshot.properties().expect("validated snapshot"))),
    };
    (
        status,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(body),
    )
        .into_response()
}
async fn find(audit: &AuditDb, id: String) -> Result<Option<Operation>, ReadError> {
    pending::blocking(audit, move |db| {
        let rows = db.query(
            "SELECT payload FROM job_copy_draft_operations WHERE operation_id=?1",
            &[&id],
        )?;
        rows.first()
            .map(|row| {
                serde_json::from_str::<Operation>(row["payload"].as_str().ok_or("invalid payload")?)
                    .map_err(|_| "invalid payload".to_owned())
            })
            .transpose()
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())
}
async fn persist(audit: &AuditDb, op: &Operation, insert: bool) -> Result<(), ReadError> {
    let op = op.clone();
    let payload = serde_json::to_string(&op).map_err(|_| unavailable())?;
    pending::blocking(audit,move |db| {
        let now = pending::now_iso();
        let next = pending::iso(Utc::now() + if op.status == "in_progress" { chrono::Duration::seconds(600) } else { pending::backoff_after(op.attempts) });
        if insert {
            let count = db.query("SELECT COUNT(*) AS n FROM job_copy_draft_operations WHERE status IN ('pending','in_progress')", &[])?;
            if count.first().and_then(|r|r["n"].as_i64()).unwrap_or(50_000) >= 50_000 { return Err("queue full".into()); }
            db.execute("INSERT INTO job_copy_draft_operations(operation_id,operator_email,listing_id,payload,status,next_retry_at,updated_at) VALUES(?1,?2,?3,?4,?5,?6,?7)", &[&op.id,&op.actor,&op.listing,&payload,&op.status,&next,&now])
        } else { db.execute("UPDATE job_copy_draft_operations SET payload=?1,status=?2,next_retry_at=?3,updated_at=?4 WHERE operation_id=?5", &[&payload,&op.status,&next,&now,&op.id]) }
    }).await.map_err(|_|unavailable())?.map_err(|_|unavailable())
}
async fn current(
    service: &JobReadService,
    id: &str,
) -> Result<BTreeMap<String, Option<String>>, crate::hubspot::HubSpotError> {
    let mut properties = drafts::PROPERTIES.to_vec();
    properties.extend(["id_hrhakkaa", "id_airwork", "airwork_account_login_id"]);
    Ok(service
        .hubspot()
        .get_listing_fresh(id, &properties)
        .await?
        .properties)
}
fn known_listing(properties: &BTreeMap<String, Option<String>>) -> bool {
    let has = |p: &str| {
        properties
            .get(p)
            .and_then(|v| v.as_deref())
            .is_some_and(|v| !v.trim().is_empty())
    };
    (has("id_hrhakkaa") && !has("id_airwork"))
        || (!has("id_hrhakkaa") && has("id_airwork") && has("airwork_account_login_id"))
}
async fn disabled(state: &AppState, actor: &str) -> Result<bool, ReadError> {
    let actor = actor.to_owned();
    pending::blocking(state.audit.as_ref().ok_or_else(unavailable)?, move |db| {
        crate::audit::dao::is_email_disabled(db, &actor)
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())
}
async fn process(
    state: &AppState,
    access: &Access,
    writes: &Writes,
    op: &mut Operation,
) -> Result<(), ReadError> {
    let audit = state.audit.as_ref().ok_or_else(unavailable)?;
    op.attempts += 1;
    let allowed = access.allowed.contains(&op.actor)
        && writes.allows(&op.listing)
        && !disabled(state, &op.actor).await?;
    let result = if !allowed {
        Err((false, "draft_forbidden"))
    } else {
        let service = access.service.as_ref().ok_or_else(unavailable)?;
        match current(service, &op.listing).await {
            Ok(properties) if !known_listing(&properties) => Err((false, "listing_media_unknown")),
            Ok(properties) => {
                let desired = op
                    .snapshot
                    .properties()
                    .map_err(|c| err(StatusCode::BAD_REQUEST, c))?;
                if drafts::revision(&properties) == drafts::revision(&desired) {
                    Ok(())
                } else if drafts::revision(&properties) != op.base {
                    Err((false, "draft_conflict"))
                } else {
                    service
                        .hubspot()
                        .patch_listing_draft(&op.listing, &desired)
                        .await
                        .map(|_| ())
                        .map_err(|e| (e.is_transient(), "draft_write_failed"))
                }
            }
            Err(e) => Err((e.is_transient(), "draft_read_failed")),
        }
    };
    match result {
        Ok(()) => {
            op.status = "saved".into();
            op.code.clear();
        }
        Err((transient, code)) => {
            op.status = if transient && op.attempts < 8 {
                "pending"
            } else {
                "failed"
            }
            .into();
            op.code = code.into();
        }
    }
    persist(audit, op, false).await?;
    wake_worker().notify_one();
    tracing::info!(operator_email=%op.actor, listing_id=%op.listing, operation_id=%op.id, status=%op.status, "job_copy_draft_write");
    Ok(())
}
async fn authenticate(
    state: &AppState,
    access: &Access,
    writes: &Writes,
    session: &Session,
    headers: &HeaderMap,
    id: &str,
) -> Result<String, ReadError> {
    let actor = authorized_user(state, access, session)
        .await?
        .to_lowercase();
    super::valid_id(id)?;
    if id.len() > 20 {
        return Err(err(StatusCode::BAD_REQUEST, "invalid_record_id"));
    }
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("fetch")
    {
        return Err(err(StatusCode::FORBIDDEN, "csrf_required"));
    }
    if !writes.allows(id) {
        return Err(err(StatusCode::FORBIDDEN, "draft_writes_disabled"));
    }
    if !writes.limiter.allow(&actor) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "draft_rate_limited"));
    }
    if disabled(state, &actor).await? {
        return Err(err(StatusCode::FORBIDDEN, "account_disabled"));
    }
    access
        .service
        .as_ref()
        .ok_or_else(|| err(StatusCode::SERVICE_UNAVAILABLE, "hubspot_not_configured"))?;
    Ok(actor)
}
async fn save(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    Extension(writes): Extension<Writes>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<SaveDraftRequest>,
) -> Result<Response, ReadError> {
    let actor = authenticate(&state, &access, &writes, &session, &headers, &id).await?;
    let snapshot = DraftSnapshot::from_request(&req, Utc::now())
        .map_err(|c| err(StatusCode::BAD_REQUEST, c))?;
    accept(
        &state,
        &access,
        &writes,
        actor,
        id,
        req.operation_id.clone(),
        req.base_revision.clone(),
        drafts::hash(&serde_json::to_string(&req).expect("request serializes")),
        Some(snapshot),
        None,
    )
    .await
}
async fn review(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    Extension(writes): Extension<Writes>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<ReviewDraftRequest>,
) -> Result<Response, ReadError> {
    let actor = authenticate(&state, &access, &writes, &session, &headers, &id).await?;
    if !drafts::valid_operation(&req.operation_id, &req.base_revision) {
        return Err(err(StatusCode::BAD_REQUEST, "draft_invalid_input"));
    }
    accept(
        &state,
        &access,
        &writes,
        actor,
        id,
        req.operation_id.clone(),
        req.base_revision.clone(),
        drafts::hash(&serde_json::to_string(&req).expect("request serializes")),
        None,
        Some(req),
    )
    .await
}
#[allow(clippy::too_many_arguments)]
async fn accept(
    state: &AppState,
    access: &Access,
    writes: &Writes,
    actor: String,
    listing: String,
    id: String,
    base: String,
    fingerprint: String,
    snapshot: Option<DraftSnapshot>,
    review: Option<ReviewDraftRequest>,
) -> Result<Response, ReadError> {
    let _lock = record_lock::shared()
        .acquire(
            [
                record_lock::key("0-420", &listing),
                record_lock::key("job-copy-operation", &id),
            ],
            Duration::from_secs(10),
        )
        .await
        .map_err(|_| err(StatusCode::SERVICE_UNAVAILABLE, "record_busy"))?;
    let audit = state.audit.as_ref().ok_or_else(unavailable)?;
    if let Some(op) = find(audit, id.clone()).await? {
        if op.actor != actor || op.listing != listing || op.fingerprint != fingerprint {
            return Err(err(StatusCode::CONFLICT, "operation_id_conflict"));
        }
        return Ok(operation_response(&op));
    }
    let snapshot = match snapshot {
        Some(s) => s,
        None => {
            let properties = current(access.service.as_ref().ok_or_else(unavailable)?, &listing)
                .await
                .map_err(super::client_error)?;
            if drafts::revision(&properties) != base {
                return Err(err(StatusCode::CONFLICT, "draft_conflict"));
            }
            let mut s: DraftSnapshot = serde_json::from_str(
                properties
                    .get(drafts::FACTS)
                    .and_then(|v| v.as_deref())
                    .unwrap_or(""),
            )
            .map_err(|_| err(StatusCode::CONFLICT, "draft_not_found"))?;
            let req = review.ok_or_else(|| err(StatusCode::BAD_REQUEST, "draft_invalid_input"))?;
            if !s.valid() || s.draft_id != req.draft_id {
                return Err(err(StatusCode::CONFLICT, "draft_conflict"));
            }
            s.review_status = req.status;
            s.properties()
                .map_err(|c| err(StatusCode::BAD_REQUEST, c))?;
            s
        }
    };
    let mut op = Operation {
        id,
        actor,
        listing,
        fingerprint,
        base,
        snapshot,
        status: "in_progress".into(),
        attempts: 0,
        code: String::new(),
    };
    persist(audit, &op, true).await?;
    process(state, access, writes, &mut op).await?;
    Ok(operation_response(&op))
}
async fn operation(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Response, ReadError> {
    let actor = authorized_user(&state, &access, &session)
        .await?
        .to_lowercase();
    let op = find(state.audit.as_ref().ok_or_else(unavailable)?, id)
        .await?
        .filter(|op| op.actor == actor)
        .ok_or_else(|| err(StatusCode::NOT_FOUND, "draft_operation_not_found"))?;
    Ok(operation_response(&op))
}
pub(super) fn router(state: Option<&Arc<AppState>>, access: Access) -> Router<Arc<AppState>> {
    let writes = Writes::from_env();
    if let Some(state) = state.filter(|s| s.audit.is_some() && access.service.is_some()) {
        let state = Arc::downgrade(state);
        let access = access.clone();
        let writes = writes.clone();
        tokio::spawn(async move {
            let mut last_purge: Option<tokio::time::Instant> = None;
            let mut wait = Duration::ZERO;
            loop {
                tokio::select! {
                    _ = tokio::time::sleep(wait) => {},
                    _ = wake_worker().notified() => {},
                }
                let Some(state) = state.upgrade() else { break };
                let Some(audit) = state.audit.as_ref() else {
                    break;
                };
                if last_purge.is_none_or(|at| at.elapsed() >= Duration::from_secs(86_400)) {
                    let purged = pending::blocking(audit, |db| {
                        db.execute("DELETE FROM job_copy_draft_operations WHERE status IN ('saved','failed') AND updated_at < ?1", &[&pending::iso(Utc::now()-chrono::Duration::days(7))])
                    }).await;
                    if matches!(purged, Ok(Ok(()))) {
                        last_purge = Some(tokio::time::Instant::now());
                    }
                }
                let rows = pending::blocking(audit, |db| {
                    db.query("SELECT payload FROM job_copy_draft_operations WHERE status IN ('pending','in_progress') AND next_retry_at <= ?1 ORDER BY next_retry_at LIMIT 10", &[&pending::now_iso()])
                }).await;
                wait = Duration::from_secs(30);
                if let Ok(Ok(rows)) = rows {
                    if rows.is_empty() {
                        let active = pending::blocking(audit, |db| db.query("SELECT COUNT(*) AS n FROM job_copy_draft_operations WHERE status IN ('pending','in_progress')", &[])).await;
                        if matches!(active, Ok(Ok(ref entries)) if entries.first().and_then(|entry| entry["n"].as_i64()) == Some(0))
                        {
                            wait = Duration::from_secs(600);
                        }
                    }
                    for row in rows {
                        if let Some(mut op) = row
                            .get("payload")
                            .and_then(Value::as_str)
                            .and_then(|raw| serde_json::from_str::<Operation>(raw).ok())
                        {
                            if let Ok(_lock) = record_lock::shared()
                                .acquire(
                                    [
                                        record_lock::key("0-420", &op.listing),
                                        record_lock::key("job-copy-operation", &op.id),
                                    ],
                                    Duration::from_secs(10),
                                )
                                .await
                            {
                                let Ok(Some(current)) = find(audit, op.id.clone()).await else {
                                    continue;
                                };
                                if !matches!(current.status.as_str(), "pending" | "in_progress") {
                                    continue;
                                }
                                op = current;
                                let _ = process(&state, &access, &writes, &mut op).await;
                            }
                        }
                    }
                }
            }
        });
    }
    Router::new()
        .route(
            "/api/job-copy/listings/{id}/draft",
            post(save).patch(review),
        )
        .route("/api/job-copy/draft-operations/{id}", get(operation))
        .layer(DefaultBodyLimit::max(2 * 1024 * 1024))
        .layer(Extension(writes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::job_gen::drafts::tests::request;
    use axum::{extract::State as MockState, routing::get};
    use std::sync::Mutex;
    #[derive(Default)]
    struct Remote {
        properties: BTreeMap<String, Option<String>>,
        patches: Vec<Value>,
        transient: bool,
        permanent: bool,
    }
    async fn read_remote(MockState(remote): MockState<Arc<Mutex<Remote>>>) -> Json<Value> {
        Json(json!({"id":"30","properties":remote.lock().unwrap().properties}))
    }
    async fn write_remote(
        MockState(remote): MockState<Arc<Mutex<Remote>>>,
        Json(payload): Json<Value>,
    ) -> Response {
        let mut remote = remote.lock().unwrap();
        remote.patches.push(payload.clone());
        if remote.permanent {
            return (StatusCode::BAD_REQUEST, Json(json!({"status":"error"}))).into_response();
        }
        if remote.transient {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(json!({"status":"error"})),
            )
                .into_response();
        }
        for (k, v) in payload["properties"].as_object().unwrap() {
            remote
                .properties
                .insert(k.clone(), v.as_str().map(str::to_owned));
        }
        Json(json!({"id":"30","properties":remote.properties})).into_response()
    }
    async fn setup() -> (Arc<AppState>, Access, Writes, Session, Arc<Mutex<Remote>>) {
        let remote = Arc::new(Mutex::new(Remote {
            properties: BTreeMap::from([
                ("id_hrhakkaa".into(), Some("synthetic-job".into())),
                (
                    "hrh_kyuujinhyou_honbun".into(),
                    Some("掲載本文は変えない".into()),
                ),
            ]),
            ..Default::default()
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route(
                "/crm/v3/objects/0-420/30",
                get(read_remote).patch(write_remote),
            )
            .with_state(remote.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let (audit, _) = crate::audit::fake_turso::start_sqlite_audit().await;
        let mut state = super::super::integration_tests::moc_state();
        Arc::get_mut(&mut state).unwrap().audit = Some(audit);
        let access = Access {
            allowed: BTreeSet::from(["writer@example.test".into()]),
            service: Some(Arc::new(JobReadService::for_test(base))),
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
        let writes = Writes {
            enabled: true,
            allowlist: BTreeSet::new(),
            limiter: Arc::new(WriteRateLimiter::new(200)),
        };
        (state, access, writes, session, remote)
    }
    fn headers() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-requested-with", "fetch".parse().unwrap());
        h
    }
    async fn save_fixture(
        state: Arc<AppState>,
        access: Access,
        writes: Writes,
        session: Session,
        req: SaveDraftRequest,
    ) -> Result<Response, ReadError> {
        save(
            State(state),
            Extension(access),
            Extension(writes),
            session,
            Path("30".into()),
            headers(),
            Json(req),
        )
        .await
    }
    #[tokio::test]
    async fn save_payload_author_audit_idempotency_and_review_only_touch_draft() {
        let (state, access, writes, session, remote) = setup().await;
        let r = save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session.clone(),
            request(),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let p = remote.lock().unwrap().patches[0]["properties"].clone();
        assert_eq!(p.as_object().unwrap().len(), 5);
        assert_eq!(p[drafts::STATUS], "pending");
        assert!(p[drafts::BODY]
            .as_str()
            .unwrap()
            .contains("基本給与 最小：270000"));
        let op = find(state.audit.as_ref().unwrap(), request().operation_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(op.actor, "writer@example.test");
        assert_eq!(op.status, "saved");
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                session.clone(),
                request()
            )
            .await
            .unwrap()
            .status(),
            StatusCode::OK
        );
        assert_eq!(remote.lock().unwrap().patches.len(), 1);
        let revision = drafts::revision(&remote.lock().unwrap().properties);
        let req = ReviewDraftRequest {
            operation_id: uuid::Uuid::new_v4().to_string(),
            base_revision: revision,
            draft_id: op.snapshot.draft_id.clone(),
            status: crate::job_gen::drafts::DraftStatus::Adopted,
        };
        let r = review(
            State(state),
            Extension(access),
            Extension(writes),
            session,
            Path("30".into()),
            headers(),
            Json(req),
        )
        .await
        .unwrap();
        assert_eq!(r.status(), StatusCode::OK);
        let remote = remote.lock().unwrap();
        assert_eq!(remote.patches.len(), 2);
        assert_eq!(
            remote.properties[drafts::STATUS].as_deref(),
            Some("adopted")
        );
        assert_eq!(
            remote.properties["hrh_kyuujinhyou_honbun"].as_deref(),
            Some("掲載本文は変えない")
        );
        let stored: DraftSnapshot =
            serde_json::from_str(remote.properties[drafts::FACTS].as_ref().unwrap()).unwrap();
        assert_eq!(stored.draft_id, op.snapshot.draft_id);
        assert_eq!(stored.created_at, op.snapshot.created_at);
    }
    #[tokio::test]
    async fn anonymous_password_disallowed_disabled_write_gate_and_csrf_never_patch() {
        let (state, access, writes, session, remote) = setup().await;
        let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                anonymous,
                request()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::UNAUTHORIZED
        );
        session
            .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, "password")
            .await
            .unwrap();
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                session.clone(),
                request()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        session
            .insert(
                crate::auth::SESSION_LOGIN_METHOD_KEY,
                crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
            )
            .await
            .unwrap();
        session
            .insert(crate::auth::SESSION_USER_KEY, "outsider@example.test")
            .await
            .unwrap();
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                session.clone(),
                request()
            )
            .await
            .unwrap_err()
            .0,
            StatusCode::FORBIDDEN
        );
        session
            .insert(crate::auth::SESSION_USER_KEY, "writer@example.test")
            .await
            .unwrap();
        let mut off = writes.clone();
        off.enabled = false;
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                off,
                session.clone(),
                request()
            )
            .await
            .unwrap_err()
            .1,
            "draft_writes_disabled"
        );
        assert_eq!(
            save(
                State(state),
                Extension(access),
                Extension(writes),
                session,
                Path("30".into()),
                HeaderMap::new(),
                Json(request())
            )
            .await
            .unwrap_err()
            .1,
            "csrf_required"
        );
        assert!(remote.lock().unwrap().patches.is_empty());
    }
    #[tokio::test]
    async fn disabled_user_missing_audit_and_permanent_failure_never_retry() {
        let (state, access, writes, session, remote) = setup().await;
        pending::blocking(state.audit.as_ref().unwrap(), |db| db.execute("INSERT INTO accounts(id,email,display_name,role,first_seen_at,disabled_at) VALUES('disabled','writer@example.test','合成利用者','consultant','2026-10-10','2026-10-10')", &[])).await.unwrap().unwrap();
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                session.clone(),
                request()
            )
            .await
            .unwrap_err()
            .1,
            "account_disabled"
        );
        assert!(remote.lock().unwrap().patches.is_empty());
        let (state, access, writes, session, remote) = setup().await;
        remote.lock().unwrap().permanent = true;
        let response = save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session.clone(),
            request(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::CONFLICT);
        let op = find(state.audit.as_ref().unwrap(), request().operation_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(op.status, "failed");
        assert_eq!(op.attempts, 1);
        let raw = axum::body::to_bytes(response.into_body(), 100_000)
            .await
            .unwrap();
        let result: drafts::DraftOperationResponse = serde_json::from_slice(&raw).unwrap();
        assert!(result.draft.is_none());
        assert!(result.revision.is_none());
        save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session.clone(),
            request(),
        )
        .await
        .unwrap();
        assert_eq!(remote.lock().unwrap().patches.len(), 1);
        let no_audit = super::super::integration_tests::moc_state();
        assert_eq!(
            save_fixture(no_audit, access, writes, session, request())
                .await
                .unwrap_err()
                .1,
            "draft_queue_unavailable"
        );
        assert_eq!(remote.lock().unwrap().patches.len(), 1);
    }
    #[tokio::test]
    async fn retry_detects_a_newer_draft_instead_of_overwriting_it() {
        let (state, access, writes, session, remote) = setup().await;
        remote.lock().unwrap().transient = true;
        save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session,
            request(),
        )
        .await
        .unwrap();
        let mut op = find(state.audit.as_ref().unwrap(), request().operation_id)
            .await
            .unwrap()
            .unwrap();
        remote
            .lock()
            .unwrap()
            .properties
            .insert(drafts::BODY.into(), Some("別の利用者が保存した案".into()));
        remote.lock().unwrap().transient = false;
        process(&state, &access, &writes, &mut op).await.unwrap();
        assert_eq!(op.status, "failed");
        assert_eq!(op.code, "draft_conflict");
        assert_eq!(remote.lock().unwrap().patches.len(), 1);
        assert_eq!(
            remote.lock().unwrap().properties[drafts::BODY].as_deref(),
            Some("別の利用者が保存した案")
        );
    }
    #[tokio::test]
    async fn conflicts_and_reused_operation_ids_do_not_overwrite() {
        let (state, access, writes, session, remote) = setup().await;
        save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session.clone(),
            request(),
        )
        .await
        .unwrap();
        let mut changed = request();
        changed.row.insert("仕事内容".into(), "別の案".into());
        assert_eq!(
            save_fixture(
                state.clone(),
                access.clone(),
                writes.clone(),
                session.clone(),
                changed
            )
            .await
            .unwrap_err()
            .1,
            "operation_id_conflict"
        );
        let mut stale = request();
        stale.operation_id = uuid::Uuid::new_v4().to_string();
        let r = save_fixture(state, access, writes, session, stale)
            .await
            .unwrap();
        assert_eq!(r.status(), StatusCode::CONFLICT);
        assert_eq!(remote.lock().unwrap().patches.len(), 1);
    }
    #[tokio::test]
    async fn temporary_failure_is_durable_then_rechecks_before_retry_and_lost_reply_is_idempotent()
    {
        let (state, access, writes, session, remote) = setup().await;
        remote.lock().unwrap().transient = true;
        let response = save_fixture(
            state.clone(),
            access.clone(),
            writes.clone(),
            session.clone(),
            request(),
        )
        .await
        .unwrap();
        assert_eq!(response.status(), StatusCode::ACCEPTED);
        let mut op = find(state.audit.as_ref().unwrap(), request().operation_id)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(op.status, "pending");
        assert_eq!(op.attempts, 1);
        remote.lock().unwrap().transient = false;
        process(&state, &access, &writes, &mut op).await.unwrap();
        assert_eq!(op.status, "saved");
        assert_eq!(remote.lock().unwrap().patches.len(), 2);
        op.status = "in_progress".into();
        persist(state.audit.as_ref().unwrap(), &op, false)
            .await
            .unwrap();
        process(&state, &access, &writes, &mut op).await.unwrap();
        assert_eq!(op.status, "saved");
        assert_eq!(
            remote.lock().unwrap().patches.len(),
            2,
            "送信成功後の応答喪失でも再PATCHしない"
        );
        let mut pending_op = op.clone();
        pending_op.base = drafts::revision(&remote.lock().unwrap().properties);
        pending_op.snapshot.draft_id = uuid::Uuid::new_v4().to_string();
        let mut off = writes;
        off.enabled = false;
        process(&state, &access, &off, &mut pending_op)
            .await
            .unwrap();
        assert_eq!(pending_op.code, "draft_forbidden");
        assert_eq!(remote.lock().unwrap().patches.len(), 2);
    }
}
