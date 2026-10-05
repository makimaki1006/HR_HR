//! End-to-end bridge HTTP fixtures; no Google/HubSpot API or real credentials.
use super::*;
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, Response},
    routing::any,
    Router,
};
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, sync::Mutex};

const CURRENT_MANIFEST: &str = "synthetic_manifest_current_123";
const OLD_MANIFEST: &str = "synthetic_manifest_old_123";
const CURRENT_IMAGE: &str = "synthetic_image_current_123";
const OLD_IMAGE: &str = "synthetic_image_old_123";
const NEW_BYTES: &[u8] = b"synthetic-current-original";
const OLD_BYTES: &[u8] = b"synthetic-historical-original";

#[derive(Clone)]
struct Seen {
    method: String,
    path: String,
    authorization: Option<String>,
    body: Value,
}
#[derive(Clone, Copy)]
enum PatchMode {
    Success,
    LostAcknowledgement,
    Forbidden,
}
struct Fixture {
    files: Mutex<BTreeMap<String, (String, Vec<u8>)>>,
    current: Mutex<Option<Pointer>>,
    requests: Mutex<Vec<Seen>>,
    patch: PatchMode,
}
struct Upstream {
    base: String,
    fixture: Arc<Fixture>,
    pointer: Pointer,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
fn digest(raw: &[u8]) -> String {
    format!("{:x}", Sha256::digest(raw))
}
fn manifest(id: &str, bytes: &[u8], date: &str, previous: Option<&Pointer>) -> Manifest {
    Manifest {
        schema_version: 1,
        listing_id: "30".into(),
        company_ids: vec!["10".into()],
        observed_at: date.into(),
        images: vec![ManifestImage {
            slot: 1,
            file_id: id.into(),
            sha256: digest(bytes),
            mime_type: "image/png".into(),
            size: bytes.len(),
        }],
        operation_id: "1".repeat(64),
        previous_manifest_file_id: previous.map(|p| p.file_id.clone()),
        previous_manifest_sha256: previous.map(|p| p.sha256.clone()),
    }
}
fn evidence(file_id: &str, manifest: &Manifest) -> (Pointer, Vec<u8>) {
    let raw = serde_json::to_vec(manifest).unwrap();
    (
        Pointer {
            file_id: file_id.into(),
            sha256: digest(&raw),
            observed_at: manifest.observed_at.clone(),
        },
        raw,
    )
}
impl Upstream {
    async fn start(mode: PatchMode, historical: bool) -> Self {
        let mut files = BTreeMap::new();
        let previous = if historical {
            let old = manifest(OLD_IMAGE, OLD_BYTES, "2026-10-04T00:00:00Z", None);
            let (pointer, raw) = evidence(OLD_MANIFEST, &old);
            files.insert(OLD_MANIFEST.into(), ("application/json".into(), raw));
            files.insert(OLD_IMAGE.into(), ("image/png".into(), OLD_BYTES.to_vec()));
            Some(pointer)
        } else {
            None
        };
        let current = manifest(
            CURRENT_IMAGE,
            NEW_BYTES,
            "2026-10-05T00:00:00Z",
            previous.as_ref(),
        );
        let (pointer, raw) = evidence(CURRENT_MANIFEST, &current);
        files.insert(CURRENT_MANIFEST.into(), ("application/json".into(), raw));
        files.insert(
            CURRENT_IMAGE.into(),
            ("image/png".into(), NEW_BYTES.to_vec()),
        );
        let fixture = Arc::new(Fixture {
            files: Mutex::new(files),
            current: Mutex::new(if historical {
                Some(pointer.clone())
            } else {
                None
            }),
            requests: Mutex::new(vec![]),
            patch: mode,
        });
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let base = format!("http://{}", listener.local_addr().unwrap());
        let app = Router::new()
            .fallback(any(reply))
            .with_state(fixture.clone());
        let task = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        Self {
            base,
            fixture,
            pointer,
            task,
        }
    }
    fn bridge(&self) -> ImageBridge {
        let mut bridge = ImageBridge::new(
            "synthetic-hubspot-token".into(),
            Arc::new(DriveReader::for_test(&self.base)),
            Arc::new(JobReadService::for_test(self.base.clone())),
        )
        .unwrap();
        bridge.base = self.base.clone();
        bridge
    }
    fn calls(&self) -> Vec<Seen> {
        self.fixture.requests.lock().unwrap().clone()
    }
}
fn response(status: u16, mime: &str, raw: Vec<u8>) -> Response<Body> {
    Response::builder()
        .status(status)
        .header("content-type", mime)
        .body(Body::from(raw))
        .unwrap()
}
fn json_response(status: u16, value: Value) -> Response<Body> {
    response(
        status,
        "application/json",
        serde_json::to_vec(&value).unwrap(),
    )
}
async fn reply(State(fixture): State<Arc<Fixture>>, request: Request<Body>) -> Response<Body> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_owned();
    let method = parts.method.to_string();
    let raw = to_bytes(body, 64 * 1024).await.unwrap();
    let body: Value = serde_json::from_slice(&raw).unwrap_or(Value::Null);
    fixture.requests.lock().unwrap().push(Seen {
        method: method.clone(),
        path: path.clone(),
        authorization: parts
            .headers
            .get("authorization")
            .and_then(|header| header.to_str().ok())
            .map(str::to_owned),
        body: body.clone(),
    });
    if path == "/token" {
        return json_response(
            200,
            json!({"access_token":"synthetic-drive-token","token_type":"Bearer","expires_in":3600}),
        );
    }
    if let Some(file) = path.strip_prefix("/drive/v3/files/") {
        return match fixture.files.lock().unwrap().get(file) {
            Some((mime, raw)) => response(200, mime, raw.clone()),
            None => json_response(404, json!({"error":"synthetic-file-unavailable"})),
        };
    }
    if path.starts_with("/crm/v4/objects/companies/") && path.ends_with("/associations/deals") {
        let related = path == "/crm/v4/objects/companies/10/associations/deals";
        return json_response(
            200,
            json!({"results":[{"toObjectId":if related {"20"} else {"99"}}]}),
        );
    }
    if path == "/crm/v4/objects/0-420/30/associations/deals" {
        return json_response(200, json!({"results":[{"toObjectId":"20"}]}));
    }
    if path == "/crm/v3/objects/0-420/30" {
        if method == "PATCH" {
            if matches!(fixture.patch, PatchMode::Forbidden) {
                return json_response(403, json!({"error":"synthetic-write-denied"}));
            }
            let pointer: Pointer =
                serde_json::from_str(body["properties"][PROPERTY].as_str().unwrap()).unwrap();
            *fixture.current.lock().unwrap() = Some(pointer);
            // The state is committed even though acknowledgement reports failure.
            if matches!(fixture.patch, PatchMode::LostAcknowledgement) {
                return json_response(502, json!({"error":"synthetic-acknowledgement-lost"}));
            }
        }
        let raw = fixture
            .current
            .lock()
            .unwrap()
            .as_ref()
            .map(|p| serde_json::to_string(p).unwrap())
            .unwrap_or_default();
        return json_response(200, json!({"properties":{PROPERTY:raw}}));
    }
    json_response(404, json!({"error":"unexpected-local-fixture-path"}))
}

#[tokio::test]
async fn publish_verifies_originals_before_patch_and_only_changes_pointer_property() {
    let server = Upstream::start(PatchMode::Success, false).await;
    assert_eq!(
        server
            .bridge()
            .publish("10", "30", &server.pointer)
            .await
            .unwrap(),
        "linked"
    );
    let calls = server.calls();
    let patch = calls
        .iter()
        .position(|call| call.method == "PATCH")
        .unwrap();
    let original = calls
        .iter()
        .position(|call| call.path == format!("/drive/v3/files/{CURRENT_IMAGE}"))
        .unwrap();
    assert!(original < patch);
    assert_eq!(
        calls[patch].body["properties"].as_object().unwrap().len(),
        1
    );
    assert!(calls[patch].body["properties"].get(PROPERTY).is_some());
    assert_eq!(
        calls[patch].authorization.as_deref(),
        Some("Bearer synthetic-hubspot-token")
    );
    assert_eq!(
        calls[original].authorization.as_deref(),
        Some("Bearer synthetic-drive-token")
    );
    assert!(calls
        .iter()
        .filter(|call| call.path.starts_with("/drive/"))
        .all(|call| call.method == "GET"));
}

#[tokio::test]
async fn lost_patch_acknowledgement_is_completed_by_authoritative_readback() {
    let server = Upstream::start(PatchMode::LostAcknowledgement, false).await;
    let bridge = server.bridge();
    assert_eq!(
        bridge.publish("10", "30", &server.pointer).await.unwrap(),
        "linked"
    );
    assert_eq!(
        bridge.publish("10", "30", &server.pointer).await.unwrap(),
        "already_linked"
    );
    assert_eq!(
        server
            .calls()
            .iter()
            .filter(|call| call.method == "PATCH")
            .count(),
        1
    );
}

#[tokio::test]
async fn write_denial_is_pending_and_already_linked_missing_original_is_failure() {
    let denied = Upstream::start(PatchMode::Forbidden, false).await;
    assert_eq!(
        denied
            .bridge()
            .publish("10", "30", &denied.pointer)
            .await
            .unwrap_err(),
        "hubspot_sync_pending"
    );
    assert!(denied.fixture.current.lock().unwrap().is_none());
    let missing = Upstream::start(PatchMode::Success, false).await;
    *missing.fixture.current.lock().unwrap() = Some(missing.pointer.clone());
    missing.fixture.files.lock().unwrap().remove(CURRENT_IMAGE);
    assert_eq!(
        missing
            .bridge()
            .publish("10", "30", &missing.pointer)
            .await
            .unwrap_err(),
        "original_not_verified"
    );
    assert!(!missing.calls().iter().any(|call| call.method == "PATCH"));
}

#[tokio::test]
async fn foreign_company_and_unlinked_manifest_never_fetch_requested_drive_evidence() {
    let server = Upstream::start(PatchMode::Success, false).await;
    *server.fixture.current.lock().unwrap() = Some(server.pointer.clone());
    let bridge = server.bridge();
    assert_eq!(
        bridge
            .image("99", "30", CURRENT_MANIFEST, 1)
            .await
            .unwrap_err(),
        "customer_listing_not_authorized"
    );
    assert!(!server
        .calls()
        .iter()
        .any(|call| call.path.starts_with("/drive/")));
    let unlinked = "synthetic_unrelated_manifest_123";
    assert_eq!(
        bridge.image("10", "30", unlinked, 1).await.unwrap_err(),
        "manifest_not_linked_to_listing"
    );
    assert!(!server
        .calls()
        .iter()
        .any(|call| call.path == format!("/drive/v3/files/{unlinked}")
            || call.path == format!("/drive/v3/files/{CURRENT_IMAGE}")));
}

#[tokio::test]
async fn historical_chain_uses_verified_predecessor_and_returns_old_original() {
    let server = Upstream::start(PatchMode::Success, true).await;
    assert_eq!(
        server
            .bridge()
            .image("10", "30", OLD_MANIFEST, 1)
            .await
            .unwrap(),
        ("image/png".into(), OLD_BYTES.to_vec())
    );
    let calls = server.calls();
    assert!(calls
        .iter()
        .any(|call| call.path == format!("/drive/v3/files/{OLD_MANIFEST}")));
    assert!(calls
        .iter()
        .any(|call| call.path == format!("/drive/v3/files/{OLD_IMAGE}")));
    assert!(!calls
        .iter()
        .any(|call| call.path == format!("/drive/v3/files/{CURRENT_IMAGE}")));
}
