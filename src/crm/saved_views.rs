//! Opaque queue URL queries, owned by HR_HR rather than HubSpot (ADR-026).
use super::{
    rbac,
    routes::{error_json, CrmCtx, CRM_REQUEST_DEADLINE},
};
use crate::{audit::AuditDb, AppState};
use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    Extension, Json,
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_sessions::Session;

#[derive(Clone, Serialize, Deserialize, Debug)]
struct View {
    id: String,
    #[serde(skip_serializing)]
    creator_email: String,
    name: String,
    scope: String,
    query: String,
    position: f64,
    created_at: String,
    updated_at: String,
}
impl View {
    fn editable(&self, email: &str, admin: bool) -> bool {
        self.creator_email == email || (self.scope == "shared" && admin)
    }
}
#[derive(Serialize)]
struct Item {
    #[serde(flatten)]
    view: View,
    can_edit: bool,
}
impl Item {
    fn new(view: View, email: &str, admin: bool) -> Self {
        let can_edit = view.editable(email, admin);
        Self { view, can_edit }
    }
}
struct Position(Option<f64>);
impl crate::db::turso_http::ToSqlTurso for Position {
    fn to_turso_arg(&self) -> crate::db::turso_http::TursoArg {
        self.0.map_or(
            crate::db::turso_http::TursoArg::Null,
            crate::db::turso_http::TursoArg::Real,
        )
    }
}
type Cache = HashMap<String, (Instant, Vec<View>)>;
#[derive(Default)]
pub struct ViewsStore {
    cache: Arc<Mutex<Cache>>,
    writer: Arc<Mutex<()>>,
}
#[derive(Debug)]
enum Error {
    Missing,
    Forbidden,
    Limit,
    Conflict,
    Invalid,
    Unavailable,
}
impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let (status, kind) = match self {
            Self::Missing => (StatusCode::NOT_FOUND, "view_not_found"),
            Self::Forbidden => (StatusCode::FORBIDDEN, "forbidden"),
            Self::Limit => (StatusCode::CONFLICT, "views_limit"),
            Self::Conflict => (StatusCode::CONFLICT, "view_conflict"),
            Self::Invalid => (StatusCode::BAD_REQUEST, "invalid_view"),
            Self::Unavailable => (StatusCode::SERVICE_UNAVAILABLE, "views_unavailable"),
        };
        error_json(status, kind)
    }
}
fn read_rows(
    audit: &AuditDb,
    sql: &str,
    args: &[&dyn crate::db::turso_http::ToSqlTurso],
) -> Result<Vec<View>, Error> {
    audit
        .turso()
        .query(sql, args)
        .map_err(|_| Error::Unavailable)?
        .into_iter()
        .map(|row| {
            serde_json::from_value(serde_json::to_value(row).map_err(|_| Error::Unavailable)?)
                .map_err(|_| Error::Unavailable)
        })
        .collect()
}
fn valid(name: &str, scope: &str, query: &str, position: Option<f64>) -> Result<(), Error> {
    if name.trim().is_empty()
        || name.chars().count() > 80
        || name.chars().any(char::is_control)
        || !matches!(scope, "private" | "shared")
        || query.chars().count() > 2000
        || query.chars().any(char::is_control)
        || query.contains('#')
        || position.is_some_and(|n| !n.is_finite() || n.abs() > 1e9)
    {
        return Err(Error::Invalid);
    }
    super::call_queue::validate_saved_query(query).map_err(|_| Error::Invalid)
}
fn valid_id(id: &str) -> bool {
    id.len() == 36 && uuid::Uuid::parse_str(id).is_ok_and(|u| u.to_string() == id)
}
impl ViewsStore {
    async fn list(&self, audit: AuditDb, email: String) -> Result<Vec<View>, Error> {
        let mut cache = self.cache.lock().await;
        if let Some((at, rows)) = cache.get(&email) {
            if at.elapsed() < Duration::from_secs(60) {
                return Ok(rows.clone());
            }
        }
        let owner = email.clone();
        let rows=tokio::task::spawn_blocking(move||read_rows(&audit,"SELECT * FROM crm_saved_views WHERE (creator_email=? AND scope='private') OR scope='shared' ORDER BY position,created_at,id LIMIT 150",&[&owner])).await.map_err(|_|Error::Unavailable)??;
        if cache.len() >= 512 {
            cache.retain(|_, (at, _)| at.elapsed() < Duration::from_secs(60));
            if cache.len() >= 512 {
                cache.clear();
            }
        }
        cache.insert(email, (Instant::now(), rows.clone()));
        Ok(rows)
    }
    async fn mutate(
        &self,
        audit: AuditDb,
        email: String,
        admin: bool,
        mutation: Mutation,
    ) -> Result<Option<View>, Error> {
        // The worker retains the lock and invalidates even if the request times out after committing.
        let guard = self.writer.clone().lock_owned().await;
        let cache = self.cache.clone();
        tokio::task::spawn_blocking(move || {
            let _guard = guard;
            let result = write(&audit, &email, admin, mutation);
            cache.blocking_lock().clear();
            result
        })
        .await
        .map_err(|_| Error::Unavailable)?
    }
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Create {
    id: String,
    name: String,
    scope: String,
    query: String,
    position: Option<f64>,
}
#[derive(Deserialize, Default)]
#[serde(deny_unknown_fields)]
pub struct Patch {
    name: Option<String>,
    scope: Option<String>,
    query: Option<String>,
    position: Option<f64>,
}
enum Mutation {
    Create(Create),
    Update(String, Patch),
    Delete(String),
}
fn write(
    audit: &AuditDb,
    email: &str,
    admin: bool,
    mutation: Mutation,
) -> Result<Option<View>, Error> {
    let id = match &mutation {
        Mutation::Create(c) => c.id.as_str(),
        Mutation::Update(id, _) | Mutation::Delete(id) => id.as_str(),
    };
    if !valid_id(id) {
        return Err(Error::Invalid);
    }
    let old = read_rows(audit, "SELECT * FROM crm_saved_views WHERE id=?", &[&id])?.pop();
    if let Mutation::Create(c) = mutation {
        valid(&c.name, &c.scope, &c.query, c.position)?;
        if let Some(v) = old {
            if v.creator_email == email
                && v.name == c.name.trim()
                && v.scope == c.scope
                && v.query == c.query
                && c.position.is_none_or(|p| p == v.position)
            {
                return Ok(Some(v));
            }
            return Err(Error::Conflict);
        }
        let now = chrono::Utc::now().to_rfc3339();
        let name = c.name.trim().to_owned();
        let rows=audit.turso().query("INSERT INTO crm_saved_views(id,creator_email,name,scope,query,position,created_at,updated_at) SELECT ?,?,?,?,?,COALESCE(?,(SELECT COALESCE(MAX(position),0)+1 FROM crm_saved_views)),?,? WHERE (?='private' AND (SELECT COUNT(*) FROM crm_saved_views WHERE creator_email=? AND scope='private')<50) OR (?='shared' AND (SELECT COUNT(*) FROM crm_saved_views WHERE scope='shared')<100) RETURNING *",&[&c.id,&email,&name,&c.scope,&c.query,&Position(c.position),&now,&now,&c.scope,&email,&c.scope]).map_err(|_|Error::Unavailable)?;
        return result_view(rows).map(Some);
    }
    let old = old.ok_or(Error::Missing)?;
    if old.scope == "private" && old.creator_email != email {
        return Err(Error::Missing);
    }
    if !old.editable(email, admin) {
        return Err(Error::Forbidden);
    }
    let allowed = if admin { 1i64 } else { 0 };
    if let Mutation::Delete(id) = mutation {
        audit.turso().execute("DELETE FROM crm_saved_views WHERE id=? AND (creator_email=? OR (scope='shared' AND ?=1))",&[&id,&email,&allowed]).map_err(|_|Error::Unavailable)?;
        return Ok(None);
    }
    let Mutation::Update(id, p) = mutation else {
        unreachable!()
    };
    let name = p.name.unwrap_or(old.name).trim().to_owned();
    let scope = p.scope.unwrap_or(old.scope);
    let query = p.query.unwrap_or(old.query);
    let position = p.position.unwrap_or(old.position);
    valid(&name, &scope, &query, Some(position))?;
    let now = chrono::Utc::now().to_rfc3339();
    let rows=audit.turso().query("UPDATE crm_saved_views SET name=?,scope=?,query=?,position=?,updated_at=? WHERE id=? AND (creator_email=? OR (scope='shared' AND ?=1)) AND ((?='private' AND (SELECT COUNT(*) FROM crm_saved_views WHERE creator_email=? AND scope='private' AND id<>?)<50) OR (?='shared' AND (SELECT COUNT(*) FROM crm_saved_views WHERE scope='shared' AND id<>?)<100)) RETURNING *",&[&name,&scope,&query,&position,&now,&id,&email,&allowed,&scope,&old.creator_email,&id,&scope,&id]).map_err(|_|Error::Unavailable)?;
    result_view(rows).map(Some)
}
fn result_view(mut rows: Vec<HashMap<String, serde_json::Value>>) -> Result<View, Error> {
    let row = rows.pop().ok_or(Error::Limit)?;
    serde_json::from_value(serde_json::to_value(row).map_err(|_| Error::Unavailable)?)
        .map_err(|_| Error::Unavailable)
}
async fn principal(
    session: &Session,
    state: &Arc<AppState>,
    ctx: &CrmCtx,
) -> Result<(String, bool, AuditDb), Response> {
    let p = rbac::authorize(session, state, &ctx.access, None)
        .await
        .map_err(IntoResponse::into_response)?;
    let admin = rbac::resolve_role(&p).is_admin();
    let email = p.email.unwrap_or_default().trim().to_lowercase();
    let audit = state
        .audit
        .clone()
        .ok_or_else(|| Error::Unavailable.into_response())?;
    Ok((email, admin, audit))
}
pub(super) async fn list(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let (email, admin, audit) = match principal(&session, &state, &ctx).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    match tokio::time::timeout(CRM_REQUEST_DEADLINE,ctx.views.list(audit,email.clone())).await {Ok(Ok(rows))=>Json(serde_json::json!({"views":rows.into_iter().map(|v|Item::new(v,&email,admin)).collect::<Vec<_>>()})).into_response(),Ok(Err(e))=>e.into_response(),Err(_)=>Error::Unavailable.into_response()}
}
async fn mutate(
    session: Session,
    state: Arc<AppState>,
    ctx: Arc<CrmCtx>,
    mutation: Mutation,
) -> Response {
    let (email, admin, audit) = match principal(&session, &state, &ctx).await {
        Ok(p) => p,
        Err(r) => return r,
    };
    match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        ctx.views.mutate(audit, email.clone(), admin, mutation),
    )
    .await
    {
        Ok(Ok(Some(view))) => Json(Item::new(view, &email, admin)).into_response(),
        Ok(Ok(None)) => Json(serde_json::json!({"deleted":true})).into_response(),
        Ok(Err(e)) => e.into_response(),
        Err(_) => Error::Unavailable.into_response(),
    }
}
pub(super) async fn create(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Json(body): Json<Create>,
) -> Response {
    mutate(session, state, ctx, Mutation::Create(body)).await
}
pub(super) async fn update(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
    Json(body): Json<Patch>,
) -> Response {
    mutate(session, state, ctx, Mutation::Update(id, body)).await
}
pub(super) async fn delete(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path(id): Path<String>,
) -> Response {
    mutate(session, state, ctx, Mutation::Delete(id)).await
}
#[cfg(test)]
mod tests;
