//! 求人ごとの記録欄 3 つ (検証記録・要因メモ・外部文面) の保存 (ADR-032)。
//! HubSpot には書かない。既存の監査 Turso の表 `job_copy_listing_notes` に 1 欄 = 1 行で持つ。
//! 書き込みは保存ボタンのときだけ (1 行の UPSERT)、読み取りはプロセス内に短時間だけ覚える。
use super::{authorized_user, Access, ReadError};
use crate::{audit::AuditDb, crm::pending, AppState};
use axum::{
    extract::{Path, State},
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Extension, Json, Router,
};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Mutex;
use tower_sessions::Session;

/// 欄の種類と値の上限 (文字数)。画面の入力欄の上限に合わせる。
/// review: 検証記録 (4 欄 x 2,000 字 + 進行状況・期間の JSON)、factor: 要因メモ、incoming: 外部文面 (本文 100,000 字 + 資料名 200 字の JSON)。
const FIELDS: [(&str, usize); 3] = [
    ("review", 10_000),
    ("factor", 20_000),
    ("incoming", 101_000),
];
const CACHE_TTL: Duration = Duration::from_secs(30);
const CACHE_MAX: usize = 256;

#[derive(Clone, Serialize)]
struct Note {
    field: String,
    value: String,
    updated_by: String,
    updated_at: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PutBody {
    value: String,
}
#[derive(Default)]
pub(super) struct NotesStore {
    cache: Mutex<HashMap<String, (Instant, Vec<Note>)>>,
}

fn err(status: StatusCode, code: &'static str) -> ReadError {
    ReadError(status, code)
}
fn unavailable() -> ReadError {
    err(StatusCode::SERVICE_UNAVAILABLE, "notes_unavailable")
}
fn audit_of(state: &AppState) -> Result<AuditDb, ReadError> {
    state.audit.clone().ok_or_else(unavailable)
}
fn limit_of(field: &str) -> Option<usize> {
    FIELDS.iter().find(|(f, _)| *f == field).map(|(_, n)| *n)
}
fn json_no_store(value: serde_json::Value) -> Response {
    (
        StatusCode::OK,
        [(header::CACHE_CONTROL, "private, no-store")],
        Json(value),
    )
        .into_response()
}

pub(super) fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/api/job-copy/listings/{id}/notes", get(read))
        .route(
            "/api/job-copy/listings/{id}/notes/{field}",
            axum::routing::put(write),
        )
        .layer(Extension(Arc::new(NotesStore::default())))
}

async fn load(audit: &AuditDb, listing: &str) -> Result<Vec<Note>, ReadError> {
    let listing = listing.to_owned();
    let rows = pending::blocking(audit, move |db| {
        db.query(
            "SELECT field,value,updated_by,updated_at FROM job_copy_listing_notes WHERE listing_id=?1",
            &[&listing],
        )
    })
    .await
    .map_err(|_| unavailable())?
    .map_err(|_| unavailable())?;
    let text = |row: &HashMap<String, serde_json::Value>, key: &str| {
        row.get(key)
            .and_then(|v| v.as_str())
            .map(str::to_owned)
            .ok_or_else(unavailable)
    };
    let mut notes = rows
        .iter()
        .map(|row| {
            Ok(Note {
                field: text(row, "field")?,
                value: text(row, "value")?,
                updated_by: text(row, "updated_by")?,
                updated_at: text(row, "updated_at")?,
            })
        })
        .collect::<Result<Vec<_>, ReadError>>()?;
    notes.sort_by(|a, b| a.field.cmp(&b.field));
    Ok(notes)
}

fn notes_json(notes: &[Note]) -> serde_json::Value {
    let map: serde_json::Map<String, serde_json::Value> = notes
        .iter()
        .map(|n| {
            (
                n.field.clone(),
                serde_json::json!({"value": n.value, "updated_by": n.updated_by, "updated_at": n.updated_at}),
            )
        })
        .collect();
    serde_json::json!({ "notes": map })
}

async fn read(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    Extension(store): Extension<Arc<NotesStore>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Response, ReadError> {
    authorized_user(&state, &access, &session).await?;
    super::valid_id(&id)?;
    let audit = audit_of(&state)?;
    // 読み込み中は鍵を持ち、保存側の破棄が必ず読み込みのあとに入るようにする (古い内容を残さない)。
    let mut cache = store.cache.lock().await;
    if let Some((at, notes)) = cache.get(&id) {
        if at.elapsed() < CACHE_TTL {
            return Ok(json_no_store(notes_json(notes)));
        }
    }
    let notes = load(&audit, &id).await?;
    if cache.len() >= CACHE_MAX {
        cache.retain(|_, (at, _)| at.elapsed() < CACHE_TTL);
        if cache.len() >= CACHE_MAX {
            cache.clear();
        }
    }
    cache.insert(id, (Instant::now(), notes.clone()));
    Ok(json_no_store(notes_json(&notes)))
}

async fn write(
    State(state): State<Arc<AppState>>,
    Extension(access): Extension<Access>,
    Extension(store): Extension<Arc<NotesStore>>,
    session: Session,
    Path((id, field)): Path<(String, String)>,
    headers: HeaderMap,
    Json(body): Json<PutBody>,
) -> Result<Response, ReadError> {
    let actor = authorized_user(&state, &access, &session)
        .await?
        .to_lowercase();
    super::valid_id(&id)?;
    if headers
        .get("x-requested-with")
        .and_then(|v| v.to_str().ok())
        != Some("fetch")
    {
        return Err(err(StatusCode::FORBIDDEN, "csrf_required"));
    }
    let limit =
        limit_of(&field).ok_or_else(|| err(StatusCode::BAD_REQUEST, "invalid_note_field"))?;
    if body.value.chars().count() > limit || body.value.contains('\0') {
        return Err(err(StatusCode::PAYLOAD_TOO_LARGE, "note_too_large"));
    }
    let audit = audit_of(&state)?;
    let now = Utc::now().to_rfc3339();
    let note = Note {
        field: field.clone(),
        value: body.value,
        updated_by: actor,
        updated_at: now,
    };
    let row = note.clone();
    let listing = id.clone();
    let saved = pending::blocking(&audit, move |db| {
        db.execute(
            "INSERT INTO job_copy_listing_notes(listing_id,field,value,updated_by,updated_at) VALUES(?1,?2,?3,?4,?5) \
             ON CONFLICT(listing_id,field) DO UPDATE SET value=excluded.value,updated_by=excluded.updated_by,updated_at=excluded.updated_at",
            &[&listing, &row.field, &row.value, &row.updated_by, &row.updated_at],
        )
    })
    .await;
    // 書けたかどうかが分からない場合も、覚えている内容は捨てる。
    store.cache.lock().await.remove(&id);
    saved
        .map_err(|_| unavailable())?
        .map_err(|_| unavailable())?;
    Ok(json_no_store(serde_json::json!({
        "field": note.field, "value": note.value,
        "updated_by": note.updated_by, "updated_at": note.updated_at,
    })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeMap, BTreeSet};

    struct Fixture {
        state: Arc<AppState>,
        access: Access,
        store: Arc<NotesStore>,
        conn: crate::audit::fake_turso::SharedConn,
        queries: Arc<std::sync::Mutex<Vec<String>>>,
    }
    async fn setup() -> Fixture {
        let (audit, conn, queries) =
            crate::audit::fake_turso::start_sqlite_audit_with_queries().await;
        let mut state = super::super::integration_tests::moc_state();
        Arc::get_mut(&mut state).unwrap().audit = Some(audit);
        let access = Access {
            service: None,
            images: None,
            drive_listings: BTreeSet::new(),
            drive_config_error: None,
            snapshot_reader: None,
            snapshot_cache: Arc::new(tokio::sync::Mutex::new(None)),
            resolved_jobs: Arc::new(tokio::sync::Mutex::new(BTreeMap::new())),
            moc_drive: Ok(None),
            moc_path: None,
        };
        Fixture {
            state,
            access,
            store: Arc::new(NotesStore::default()),
            conn,
            queries,
        }
    }
    async fn session_for(email: &str, method: &str) -> Session {
        let session = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        session
            .insert(crate::auth::SESSION_USER_KEY, email)
            .await
            .unwrap();
        session
            .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, method)
            .await
            .unwrap();
        session
    }
    async fn member(email: &str) -> Session {
        session_for(email, crate::auth::LOGIN_METHOD_GOOGLE_OIDC).await
    }
    fn csrf() -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert("x-requested-with", "fetch".parse().unwrap());
        h
    }
    async fn put(
        f: &Fixture,
        session: Session,
        id: &str,
        field: &str,
        value: &str,
        headers: HeaderMap,
    ) -> Result<serde_json::Value, ReadError> {
        let response = write(
            State(f.state.clone()),
            Extension(f.access.clone()),
            Extension(f.store.clone()),
            session,
            Path((id.into(), field.into())),
            headers,
            Json(PutBody {
                value: value.into(),
            }),
        )
        .await?;
        Ok(body_json(response).await)
    }
    async fn get_notes(
        f: &Fixture,
        session: Session,
        id: &str,
    ) -> Result<serde_json::Value, ReadError> {
        let response = read(
            State(f.state.clone()),
            Extension(f.access.clone()),
            Extension(f.store.clone()),
            session,
            Path(id.into()),
        )
        .await?;
        Ok(body_json(response).await)
    }
    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    fn code(e: ReadError) -> (StatusCode, &'static str) {
        (e.0, e.1)
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn saves_reads_back_and_a_second_user_overwrites_with_their_own_name() {
        let f = setup().await;
        assert_eq!(
            get_notes(&f, member("a@example.test").await, "30")
                .await
                .unwrap()["notes"],
            serde_json::json!({})
        );
        let saved = put(
            &f,
            member("A@example.test").await,
            "30",
            "factor",
            "ブランド力は広告の出稿量を見て確認",
            csrf(),
        )
        .await
        .unwrap();
        assert_eq!(saved["updated_by"], "a@example.test");
        assert_eq!(saved["value"], "ブランド力は広告の出稿量を見て確認");
        let read_back = get_notes(&f, member("b@example.test").await, "30")
            .await
            .unwrap();
        assert_eq!(
            read_back["notes"]["factor"]["value"],
            "ブランド力は広告の出稿量を見て確認"
        );
        assert_eq!(read_back["notes"]["factor"]["updated_by"], "a@example.test");
        // 別の利用者が上書き: 内容と更新者が入れ替わり、行は 1 つのまま。
        put(
            &f,
            member("b@example.test").await,
            "30",
            "factor",
            "二人目の記録",
            csrf(),
        )
        .await
        .unwrap();
        let after = get_notes(&f, member("a@example.test").await, "30")
            .await
            .unwrap();
        assert_eq!(after["notes"]["factor"]["value"], "二人目の記録");
        assert_eq!(after["notes"]["factor"]["updated_by"], "b@example.test");
        let rows: i64 = f
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM job_copy_listing_notes", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rows, 1);
        // 3 つの欄は別々の行、別の求人には出ない。
        put(
            &f,
            member("a@example.test").await,
            "30",
            "review",
            "{\"stage\":\"do\"}",
            csrf(),
        )
        .await
        .unwrap();
        put(
            &f,
            member("a@example.test").await,
            "30",
            "incoming",
            "{\"source\":\"資料\",\"text\":\"本文\"}",
            csrf(),
        )
        .await
        .unwrap();
        let all = get_notes(&f, member("a@example.test").await, "30")
            .await
            .unwrap();
        let keys: Vec<&String> = all["notes"].as_object().unwrap().keys().collect();
        assert_eq!(keys, ["factor", "incoming", "review"]);
        assert_eq!(
            get_notes(&f, member("a@example.test").await, "31")
                .await
                .unwrap()["notes"],
            serde_json::json!({})
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn a_save_is_one_upsert_and_reads_are_cached_until_a_save() {
        let f = setup().await;
        let before = f.queries.lock().unwrap().len();
        put(
            &f,
            member("a@example.test").await,
            "30",
            "review",
            "x",
            csrf(),
        )
        .await
        .unwrap();
        // 書き込みは UPSERT の 1 文だけ (ほかは既存の無効化アカウントの確認)。
        let sent: Vec<String> = f.queries.lock().unwrap()[before..]
            .iter()
            .filter(|q| !q.starts_with("SELECT disabled_at FROM accounts"))
            .cloned()
            .collect();
        assert_eq!(sent.len(), 1, "{sent:?}");
        assert!(sent[0].starts_with("INSERT INTO job_copy_listing_notes"));
        let count = |f: &Fixture| {
            f.queries
                .lock()
                .unwrap()
                .iter()
                .filter(|q| q.starts_with("SELECT field,value"))
                .count()
        };
        let s = member("a@example.test").await;
        get_notes(&f, s.clone(), "30").await.unwrap();
        get_notes(&f, s.clone(), "30").await.unwrap();
        assert_eq!(count(&f), 1, "2 回目はキャッシュ");
        put(&f, s.clone(), "30", "review", "y", csrf())
            .await
            .unwrap();
        let again = get_notes(&f, s, "30").await.unwrap();
        assert_eq!(again["notes"]["review"]["value"], "y");
        assert_eq!(count(&f), 2);
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn audit_failure_gives_503_and_a_failed_save_stores_nothing() {
        let f = setup().await;
        put(
            &f,
            member("a@example.test").await,
            "30",
            "factor",
            "前の内容",
            csrf(),
        )
        .await
        .unwrap();
        f.conn
            .lock()
            .unwrap()
            .execute("DROP TABLE job_copy_listing_notes", [])
            .unwrap();
        assert_eq!(
            code(
                put(
                    &f,
                    member("a@example.test").await,
                    "30",
                    "factor",
                    "新しい内容",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::SERVICE_UNAVAILABLE, "notes_unavailable")
        );
        assert_eq!(
            code(
                get_notes(&f, member("a@example.test").await, "30")
                    .await
                    .unwrap_err()
            ),
            (StatusCode::SERVICE_UNAVAILABLE, "notes_unavailable")
        );
        // 監査 DB が無い構成でも同じ
        let mut g = setup().await;
        Arc::get_mut(&mut g.state).unwrap().audit = None;
        assert_eq!(
            code(
                put(
                    &g,
                    member("a@example.test").await,
                    "30",
                    "factor",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::SERVICE_UNAVAILABLE, "notes_unavailable")
        );
        assert_eq!(
            code(
                get_notes(&g, member("a@example.test").await, "30")
                    .await
                    .unwrap_err()
            ),
            (StatusCode::SERVICE_UNAVAILABLE, "notes_unavailable")
        );
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn access_rules_outsider_non_google_disabled_csrf_and_input_checks() {
        let f = setup().await;
        let denied = (StatusCode::FORBIDDEN, "job_copy_access_denied");
        assert_eq!(
            code(
                put(
                    &f,
                    member("out@other.test").await,
                    "30",
                    "factor",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            denied
        );
        assert_eq!(
            code(
                get_notes(&f, member("out@other.test").await, "30")
                    .await
                    .unwrap_err()
            ),
            denied
        );
        assert_eq!(
            code(
                put(
                    &f,
                    session_for("a@example.test", crate::auth::LOGIN_METHOD_PASSWORD).await,
                    "30",
                    "factor",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            denied
        );
        let anonymous = Session::new(None, Arc::new(tower_sessions::MemoryStore::default()), None);
        assert_eq!(
            code(get_notes(&f, anonymous, "30").await.unwrap_err()),
            (StatusCode::UNAUTHORIZED, "login_required")
        );
        pending::blocking(f.state.audit.as_ref().unwrap(), |db| db.execute("INSERT INTO accounts(id,email,display_name,role,first_seen_at,disabled_at) VALUES('d','gone@example.test','停止','user','2026-10-01','2026-10-02')", &[])).await.unwrap().unwrap();
        assert_eq!(
            code(
                put(
                    &f,
                    member("gone@example.test").await,
                    "30",
                    "factor",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::FORBIDDEN, "account_disabled")
        );
        assert_eq!(
            code(
                put(
                    &f,
                    member("a@example.test").await,
                    "30",
                    "factor",
                    "x",
                    HeaderMap::new()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::FORBIDDEN, "csrf_required")
        );
        assert_eq!(
            code(
                put(
                    &f,
                    member("a@example.test").await,
                    "30",
                    "nope",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::BAD_REQUEST, "invalid_note_field")
        );
        assert_eq!(
            code(
                put(
                    &f,
                    member("a@example.test").await,
                    "abc",
                    "factor",
                    "x",
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::BAD_REQUEST, "invalid_record_id")
        );
        let too_long = "あ".repeat(20_001);
        assert_eq!(
            code(
                put(
                    &f,
                    member("a@example.test").await,
                    "30",
                    "factor",
                    &too_long,
                    csrf()
                )
                .await
                .unwrap_err()
            ),
            (StatusCode::PAYLOAD_TOO_LARGE, "note_too_large")
        );
        put(
            &f,
            member("a@example.test").await,
            "30",
            "factor",
            &"あ".repeat(20_000),
            csrf(),
        )
        .await
        .unwrap();
        let rows: i64 = f
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM job_copy_listing_notes", [], |r| {
                r.get(0)
            })
            .unwrap();
        assert_eq!(rows, 1, "拒否した要求は何も書かない");
    }
}
