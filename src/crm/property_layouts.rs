//! HR_HR-only role presentation settings in the existing audit store (ADR-024).
use super::{
    rbac::{self, CrmRole, RoleLookup},
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
    collections::{BTreeMap, HashSet},
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_sessions::Session;

#[derive(Default)]
pub struct LayoutCache(Mutex<BTreeMap<String, (Instant, Layouts)>>);
type Layouts = BTreeMap<String, Vec<String>>;
impl LayoutCache {
    async fn read(
        &self,
        audit: AuditDb,
        role: String,
    ) -> Result<BTreeMap<String, Vec<String>>, String> {
        // Serialize misses with updates: an older read cannot repopulate after invalidation.
        let mut slots = self.0.lock().await;
        if let Some((at, value)) = slots.get(&role) {
            if at.elapsed() < Duration::from_secs(180) {
                return Ok(value.clone());
            }
        }
        let key = role.clone();
        let value = tokio::task::spawn_blocking(move || {
            let rows = audit.turso().query("SELECT object_type, properties FROM crm_role_property_layouts WHERE role = ? LIMIT 3", &[&role])?;
            let mut out = BTreeMap::new();
            for row in rows { if let (Some(obj), Some(raw)) = (row["object_type"].as_str(), row["properties"].as_str()) {
                let props: Vec<String> = serde_json::from_str(raw).map_err(|_| "invalid layout")?;
                out.insert(obj.to_string(), props);
            }}
            Ok::<_, String>(out)
        }).await.map_err(|_| "layout task failed")??;
        slots.insert(key, (Instant::now(), value.clone()));
        Ok(value)
    }
    async fn save(
        &self,
        audit: AuditDb,
        role: String,
        object: String,
        properties: Vec<String>,
        email: String,
    ) -> Result<(), String> {
        let mut slots = self.0.lock().await;
        let key = role.clone();
        tokio::task::spawn_blocking(move || {
            let raw = serde_json::to_string(&properties).map_err(|_| "invalid layout")?;
            audit.turso().execute("INSERT INTO crm_role_property_layouts(role, object_type, properties, updated_by, updated_at) VALUES (?, ?, ?, ?, ?) ON CONFLICT(role, object_type) DO UPDATE SET properties=excluded.properties, updated_by=excluded.updated_by, updated_at=excluded.updated_at", &[&role, &object, &raw, &email, &chrono::Utc::now().to_rfc3339()])
        }).await.map_err(|_| "layout task failed")??;
        slots.remove(&key);
        Ok(())
    }
}
#[derive(Serialize)]
struct LayoutResponse {
    role: String,
    can_edit: bool,
    layouts: BTreeMap<String, Vec<String>>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LayoutRequest {
    properties: Vec<String>,
}
fn object_key(raw: &str) -> Option<&'static str> {
    match raw {
        "deal" => Some("deals"),
        "contact" => Some("contacts"),
        "company" => Some("companies"),
        _ => None,
    }
}
fn valid_names(properties: &[String]) -> bool {
    properties.len() <= super::property_catalog::MAX_SELECTED_PER_OBJECT
        && properties.iter().all(|p| {
            !p.is_empty()
                && p.len() <= 100
                && p.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        && properties.iter().collect::<HashSet<_>>().len() == properties.len()
}
pub(super) async fn get(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    let admin = rbac::resolve_role(&principal).is_admin();
    let email = principal.email.unwrap_or_default();
    // Existing CRM authorization deliberately normalizes non-admin roles. Presentation uses the account's actual role.
    let role = if admin {
        CrmRole::Admin
    } else {
        match rbac::lookup_role(&state, ctx.access.role_cache(), &email, Instant::now()).await {
            RoleLookup::Found(r) => r,
            _ => CrmRole::User,
        }
    };
    let Some(audit) = state.audit.clone() else {
        return Json(LayoutResponse {
            role: role.as_str().into(),
            can_edit: admin,
            layouts: BTreeMap::new(),
        })
        .into_response();
    };
    match tokio::time::timeout(
        CRM_REQUEST_DEADLINE,
        ctx.layouts.read(audit, role.as_str().into()),
    )
    .await
    {
        Ok(Ok(layouts)) => Json(LayoutResponse {
            role: role.as_str().into(),
            can_edit: admin,
            layouts,
        })
        .into_response(),
        _ => error_json(StatusCode::SERVICE_UNAVAILABLE, "layout_unavailable"),
    }
}
pub(super) async fn put(
    session: Session,
    State(state): State<Arc<AppState>>,
    Extension(ctx): Extension<Arc<CrmCtx>>,
    Path((role, object)): Path<(String, String)>,
    Json(body): Json<LayoutRequest>,
) -> Response {
    let principal = match rbac::authorize(&session, &state, &ctx.access, None).await {
        Ok(p) => p,
        Err(e) => return e.into_response(),
    };
    if !rbac::resolve_role(&principal).is_admin() {
        return error_json(StatusCode::FORBIDDEN, "forbidden");
    }
    if !CrmRole::ALL.iter().any(|r| r.as_str() == role)
        || object_key(&object).is_none()
        || !valid_names(&body.properties)
    {
        return error_json(StatusCode::BAD_REQUEST, "invalid_layout");
    }
    let Some(client) = state.hubspot.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "not_configured");
    };
    let Some(audit) = state.audit.clone() else {
        return error_json(StatusCode::SERVICE_UNAVAILABLE, "layout_unavailable");
    };
    let task = async {
        let (catalog, _) = ctx
            .catalog
            .get(&client)
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)?;
        if !catalog
            .unknown(object_key(&object).unwrap(), &body.properties)
            .is_empty()
        {
            return Err(StatusCode::BAD_REQUEST);
        }
        ctx.layouts
            .save(
                audit,
                role,
                object,
                body.properties,
                principal.email.unwrap_or_default(),
            )
            .await
            .map_err(|_| StatusCode::SERVICE_UNAVAILABLE)
    };
    match tokio::time::timeout(CRM_REQUEST_DEADLINE, task).await {
        Ok(Ok(())) => Json(serde_json::json!({"saved": true})).into_response(),
        Ok(Err(status)) => error_json(status, "invalid_layout"),
        Err(_) => error_json(StatusCode::SERVICE_UNAVAILABLE, "layout_unavailable"),
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounded_unique_names() {
        assert!(valid_names(&["dealname".into()]));
        assert!(!valid_names(&["bad-name".into()]));
        assert!(!valid_names(&["x".into(), "x".into()]));
        assert!(!valid_names(&vec!["x".into(); 101]));
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn cache_is_invalidated_after_save() {
        let (audit, _, queries) = crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
        let cache = LayoutCache::default();
        assert!(cache
            .read(audit.clone(), "bpo".into())
            .await
            .unwrap()
            .is_empty());
        assert!(cache
            .read(audit.clone(), "bpo".into())
            .await
            .unwrap()
            .is_empty());
        assert_eq!(
            queries
                .lock()
                .unwrap()
                .iter()
                .filter(|q| q.starts_with("SELECT object_type"))
                .count(),
            1
        );
        cache
            .save(
                audit.clone(),
                "bpo".into(),
                "deal".into(),
                vec!["bpo_10".into(), "dealname".into()],
                "fake@example.com".into(),
            )
            .await
            .unwrap();
        assert_eq!(
            cache.read(audit, "bpo".into()).await.unwrap()["deal"],
            vec!["bpo_10", "dealname"]
        );
        assert_eq!(
            queries
                .lock()
                .unwrap()
                .iter()
                .filter(|q| q.starts_with("SELECT object_type"))
                .count(),
            2
        );
    }
    #[tokio::test(flavor = "multi_thread")]
    async fn routes_enforce_admin_catalog_csrf_and_return_actual_account_role() {
        use axum::{
            body::{to_bytes, Body},
            http::{header, Request},
            routing::{get as get_route, post},
            Router,
        };
        use tower::ServiceExt;
        use tower_sessions::{MemoryStore, SessionManagerLayer};
        let (audit, conn, queries) =
            crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
        conn.lock().unwrap().execute("INSERT INTO accounts(id,email,role,first_seen_at) VALUES('fake','operator@f-a-c.co.jp','consultant','2026-10-12')", []).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        async fn catalog(Path(path): Path<String>) -> Json<serde_json::Value> {
            if path.ends_with("/groups") {
                Json(
                    serde_json::json!({"results":[{"name":"info","label":"情報","displayOrder":0}]}),
                )
            } else {
                Json(
                    serde_json::json!({"results":[{"name":"dealname","label":"案件名","type":"string","fieldType":"text","groupName":"info","displayOrder":0}]}),
                )
            }
        }
        tokio::spawn(async move {
            axum::serve(
                listener,
                Router::new().route("/crm/v3/properties/{*path}", get_route(catalog)),
            )
            .await
            .unwrap();
        });
        let mut config = crate::config::AppConfig::from_env();
        config.allowed_domains = vec!["f-a-c.co.jp".into()];
        config.admin_emails.clear();
        let state = Arc::new(AppState {
            config,
            hw_db: None,
            indeed_db: None,
            turso_db: None,
            salesnow_db: None,
            scout_db: None,
            cache: crate::db::cache::AppCache::new(60, 10),
            rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
            company_geo_cache: None,
            audit: Some(audit),
            google_oidc: None,
            hubspot: Some(Arc::new(
                crate::hubspot::HubSpotClient::new(
                    "fake-token".into(),
                    &base,
                    crate::hubspot::ClientOptions::default(),
                )
                .unwrap(),
            )),
        });
        async fn login(session: Session) -> StatusCode {
            session
                .insert(crate::auth::SESSION_USER_KEY, "operator@f-a-c.co.jp")
                .await
                .unwrap();
            session
                .insert(
                    crate::auth::SESSION_LOGIN_METHOD_KEY,
                    crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
                )
                .await
                .unwrap();
            StatusCode::OK
        }
        for admin in [false, true] {
            let access = rbac::CrmAccess::default().with_test_role(
                "operator@f-a-c.co.jp",
                if admin { CrmRole::Admin } else { CrmRole::Bpo },
            );
            let app = Router::new()
                .merge(super::super::routes::router(access))
                .route("/__login", post(login))
                .with_state(state.clone())
                .layer(SessionManagerLayer::new(MemoryStore::default()).with_secure(false));
            let anonymous = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/crm/property-layouts")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(anonymous.status(), StatusCode::UNAUTHORIZED);
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri("/__login")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let cookie = response.headers()[header::SET_COOKIE]
                .to_str()
                .unwrap()
                .split(';')
                .next()
                .unwrap()
                .to_owned();
            for (name, expected) in [
                ("dealname", if admin { 200 } else { 403 }),
                ("not_in_catalog", if admin { 400 } else { 403 }),
            ] {
                let response = app
                    .clone()
                    .oneshot(
                        Request::builder()
                            .method("PUT")
                            .uri("/api/crm/property-layouts/consultant/deal")
                            .header(header::COOKIE, &cookie)
                            .header(header::CONTENT_TYPE, "application/json")
                            .header("X-Requested-With", "fetch")
                            .body(Body::from(
                                serde_json::json!({"properties":[name]}).to_string(),
                            ))
                            .unwrap(),
                    )
                    .await
                    .unwrap();
                assert_eq!(response.status().as_u16(), expected);
            }
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method("PUT")
                        .uri("/api/crm/property-layouts/bpo/deal")
                        .header(header::COOKIE, &cookie)
                        .header(header::CONTENT_TYPE, "application/json")
                        .body(Body::from("{\"properties\":[]}"))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .uri("/api/crm/property-layouts")
                        .header(header::COOKIE, &cookie)
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            let value: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 10240).await.unwrap())
                    .unwrap();
            assert_eq!(value["role"], if admin { "admin" } else { "consultant" });
        }
        assert!(queries
            .lock()
            .unwrap()
            .iter()
            .all(|sql| !sql.contains("not_in_catalog")));
    }
}
