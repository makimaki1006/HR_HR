//! 市場と比べる Indeed 職種を、人が求人ごとに指定して HubSpot の求人の項目に書く。
//! 文面から職種を推測しない。値は職種名、または「市場と比べない」を表す `__none__`。
use super::{authorized_user, listings, valid_id, Access, ReadError};
use crate::{hubspot::HubSpotError, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    Extension, Json,
};
use listings::INDEED_TITLE_NONE;
use serde::Deserialize;
use serde_json::{json, Value};
use std::sync::Arc;
use tower_sessions::Session;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Request {
    title: Option<String>,
    compare: bool,
}

fn err(status: StatusCode, code: &'static str) -> ReadError {
    ReadError(status, code)
}

fn write_error(e: HubSpotError) -> ReadError {
    match e {
        HubSpotError::Decode(m) if m == crate::hubspot::client::INDEED_TITLE_PROPERTY_MISSING => {
            err(StatusCode::CONFLICT, "indeed_title_property_missing")
        }
        HubSpotError::Busy => err(StatusCode::SERVICE_UNAVAILABLE, "hubspot_busy"),
        HubSpotError::NotFound => err(StatusCode::NOT_FOUND, "listing_not_found"),
        HubSpotError::Auth { .. } => err(StatusCode::FORBIDDEN, "hubspot_scope_denied"),
        HubSpotError::RateLimited => err(StatusCode::TOO_MANY_REQUESTS, "hubspot_rate_limited"),
        _ => err(StatusCode::BAD_GATEWAY, "hubspot_write_failed"),
    }
}

pub(super) async fn put(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    session: Session,
    Path(id): Path<String>,
    headers: HeaderMap,
    Json(req): Json<Request>,
) -> Result<([(header::HeaderName, &'static str); 1], Json<Value>), ReadError> {
    authorized_user(&state, &access, &session).await?;
    valid_id(&id)?;
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("fetch")
    {
        return Err(err(StatusCode::FORBIDDEN, "csrf_required"));
    }
    let value = if req.compare {
        let title = req
            .title
            .as_deref()
            .map(str::trim)
            .filter(|t| !t.is_empty())
            .ok_or_else(|| err(StatusCode::BAD_REQUEST, "indeed_title_required"))?;
        // 市場データにある職種名だけ。職種名が印と同じ文字列でも通さない。
        let db = state.indeed_db.as_ref().ok_or(ReadError(
            StatusCode::SERVICE_UNAVAILABLE,
            "market_data_unavailable",
        ))?;
        let titles = listings::market_titles(db)?;
        if title == INDEED_TITLE_NONE || !titles.iter().any(|t| t == title) {
            return Err(err(StatusCode::BAD_REQUEST, "indeed_title_unknown"));
        }
        title.to_owned()
    } else {
        INDEED_TITLE_NONE.to_owned()
    };
    let service = access.service.as_deref().ok_or(ReadError(
        StatusCode::SERVICE_UNAVAILABLE,
        "hubspot_not_configured",
    ))?;
    service
        .hubspot()
        .patch_listing_indeed_title(&id, &value)
        .await
        .map_err(write_error)?;
    service.detail_cache.forget_listing(&id);
    listings::update_indeed_title(service, &id, &value).await;
    tracing::info!(listing = %id, none = !req.compare, "job_copy_indeed_title_saved");
    let (title, state_name) = if req.compare {
        (Some(value), "set")
    } else {
        (None, "none")
    };
    Ok((
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(json!({"listing_id": id, "indeed_title": title, "indeed_title_state": state_name})),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{extract::State as MockState, response::IntoResponse, routing::patch, Router};
    use std::collections::BTreeSet;
    use std::sync::Mutex;

    #[derive(Default)]
    struct Remote {
        patches: Vec<Value>,
        property_exists: bool,
    }
    async fn write_remote(
        MockState(remote): MockState<Arc<Mutex<Remote>>>,
        Json(payload): Json<Value>,
    ) -> axum::response::Response {
        let mut remote = remote.lock().unwrap();
        remote.patches.push(payload.clone());
        if !remote.property_exists {
            return (
                StatusCode::BAD_REQUEST,
                Json(json!({"status":"error","category":"VALIDATION_ERROR","errors":[{"code":"PROPERTY_DOESNT_EXIST"}]})),
            )
                .into_response();
        }
        Json(json!({"id":"30","properties":payload["properties"]})).into_response()
    }
    async fn property_definition(
        MockState(remote): MockState<Arc<Mutex<Remote>>>,
    ) -> axum::response::Response {
        if remote.lock().unwrap().property_exists {
            Json(json!({"name":"jobcopy_indeed_title"})).into_response()
        } else {
            (StatusCode::NOT_FOUND, Json(json!({}))).into_response()
        }
    }
    struct Fixture {
        state: Arc<AppState>,
        access: Access,
        session: Session,
        remote: Arc<Mutex<Remote>>,
        _db: tempfile::NamedTempFile,
    }
    async fn session_for(email: Option<&str>) -> Session {
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        if let Some(email) = email {
            session
                .insert(crate::auth::SESSION_USER_KEY, email)
                .await
                .unwrap();
            session
                .insert(
                    crate::auth::SESSION_LOGIN_METHOD_KEY,
                    crate::auth::LOGIN_METHOD_GOOGLE_OIDC,
                )
                .await
                .unwrap();
        }
        session
    }
    async fn fixture(property_exists: bool) -> Fixture {
        let remote = Arc::new(Mutex::new(Remote {
            property_exists,
            ..Default::default()
        }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .route("/crm/v3/objects/0-420/30", patch(write_remote))
            .route(
                "/crm/v3/properties/0-420/jobcopy_indeed_title",
                axum::routing::get(property_definition),
            )
            .with_state(remote.clone());
        tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let db = tempfile::NamedTempFile::new().unwrap();
        rusqlite::Connection::open(db.path())
            .unwrap()
            .execute_batch(
                "CREATE TABLE insight_title_pref (norm_title TEXT);
                 INSERT INTO insight_title_pref VALUES ('配送ドライバー'),('看護師');",
            )
            .unwrap();
        let mut state = super::super::integration_tests::moc_state();
        Arc::get_mut(&mut state).unwrap().indeed_db =
            Some(crate::db::local_sqlite::LocalDb::new(db.path().to_str().unwrap()).unwrap());
        let access = Access {
            service: Some(Arc::new(super::super::JobReadService::for_test(base))),
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
            snapshot_reader: None,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(std::collections::BTreeMap::new())),
            moc_drive: Ok(None),
            moc_path: None,
        };
        Fixture {
            state,
            access,
            session: session_for(Some("writer@example.test")).await,
            remote,
            _db: db,
        }
    }
    fn headers(csrf: bool) -> HeaderMap {
        let mut h = HeaderMap::new();
        if csrf {
            h.insert("x-requested-with", "fetch".parse().unwrap());
        }
        h
    }
    async fn call(
        f: &Fixture,
        session: Session,
        csrf: bool,
        title: Option<&str>,
        compare: bool,
    ) -> Result<Value, (StatusCode, &'static str)> {
        put(
            State(f.state.clone()),
            Extension(f.access.clone()),
            session,
            Path("30".into()),
            headers(csrf),
            Json(Request {
                title: title.map(str::to_owned),
                compare,
            }),
        )
        .await
        .map(|(_, Json(v))| v)
        .map_err(|ReadError(status, code)| (status, code))
    }

    #[tokio::test]
    async fn a_known_title_is_saved_to_the_listing_property() {
        let f = fixture(true).await;
        let v = call(&f, f.session.clone(), true, Some("配送ドライバー"), true)
            .await
            .unwrap();
        assert_eq!(v["indeed_title"], "配送ドライバー");
        assert_eq!(v["indeed_title_state"], "set");
        let patches = f.remote.lock().unwrap().patches.clone();
        assert_eq!(
            patches,
            vec![json!({"properties":{"jobcopy_indeed_title":"配送ドライバー"}})]
        );
    }

    #[tokio::test]
    async fn not_comparing_writes_the_sentinel_and_ignores_any_title() {
        let f = fixture(true).await;
        let v = call(&f, f.session.clone(), true, Some("看護師"), false)
            .await
            .unwrap();
        assert_eq!(v["indeed_title"], Value::Null);
        assert_eq!(v["indeed_title_state"], "none");
        assert_eq!(
            f.remote.lock().unwrap().patches,
            vec![json!({"properties":{"jobcopy_indeed_title":"__none__"}})]
        );
    }

    #[tokio::test]
    async fn unknown_missing_or_sentinel_titles_are_rejected_before_any_write() {
        let f = fixture(true).await;
        for title in [Some("運転手"), Some("__none__"), Some("  "), None] {
            let e = call(&f, f.session.clone(), true, title, true)
                .await
                .unwrap_err();
            assert_eq!(e.0, StatusCode::BAD_REQUEST, "{title:?}");
        }
        assert!(f.remote.lock().unwrap().patches.is_empty());
    }

    #[tokio::test]
    async fn a_missing_hubspot_property_gives_the_friendly_error() {
        let f = fixture(false).await;
        let e = call(&f, f.session.clone(), true, Some("看護師"), true)
            .await
            .unwrap_err();
        assert_eq!(e, (StatusCode::CONFLICT, "indeed_title_property_missing"));
    }

    #[tokio::test]
    async fn anonymous_other_users_and_missing_csrf_never_write() {
        let f = fixture(true).await;
        let e = call(&f, session_for(None).await, true, Some("看護師"), true)
            .await
            .unwrap_err();
        assert_eq!(e.0, StatusCode::UNAUTHORIZED);
        let e = call(
            &f,
            session_for(Some("other@other.test")).await,
            true,
            Some("看護師"),
            true,
        )
        .await
        .unwrap_err();
        assert_eq!(e.0, StatusCode::FORBIDDEN);
        let e = call(&f, f.session.clone(), false, Some("看護師"), true)
            .await
            .unwrap_err();
        assert_eq!(e, (StatusCode::FORBIDDEN, "csrf_required"));
        assert!(f.remote.lock().unwrap().patches.is_empty());
    }

    #[test]
    fn listing_rows_report_the_three_states_without_guessing() {
        let make = |value: Option<&str>| -> super::super::Record {
            let mut props = json!({"id_hrhakkaa":"HR-1","hrh_kyuujinhyou_honbun":"Indeed表示職種名：配送ドライバー"});
            if let Some(v) = value {
                props["jobcopy_indeed_title"] = json!(v);
            }
            serde_json::from_value(json!({"id":"1","properties":props})).unwrap()
        };
        let row = |v| listings::tests_listing(&make(v));
        // 本文に「Indeed表示職種名」があっても、指定が無ければ未指定のまま。
        assert_eq!(row(None), (None, "unset"));
        assert_eq!(row(Some("")), (None, "unset"));
        assert_eq!(row(Some("__none__")), (None, "none"));
        assert_eq!(row(Some("看護師")), (Some("看護師".into()), "set"));
    }
}
