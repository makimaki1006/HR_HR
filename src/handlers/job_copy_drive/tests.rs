//! Local HTTP fixtures only; no environment changes or Google requests.
use super::*;

#[tokio::test]
async fn snapshot_reader_verifies_json_mime_hash_and_independent_size_limit() {
    let raw = serde_json::to_vec(
        &serde_json::json!({"schemaVersion":1,"padding":"x".repeat(3 * 1024 * 1024)}),
    )
    .unwrap();
    let server = Upstream::start("application/json", raw.clone(), 200, false, 3600).await;
    let reader = server.reader();
    assert_eq!(
        reader
            .read_snapshot_verified(FILE_ID, &digest(&raw))
            .await
            .unwrap()["schemaVersion"],
        1
    );
    assert_eq!(
        reader
            .read_snapshot_verified(FILE_ID, &"0".repeat(64))
            .await
            .unwrap_err(),
        DriveError("drive_hash_mismatch")
    );
    let wrong = Upstream::start("text/html", raw.clone(), 200, false, 3600).await;
    assert_eq!(
        wrong
            .reader()
            .read_snapshot_verified(FILE_ID, &digest(&raw))
            .await
            .unwrap_err(),
        DriveError("drive_snapshot_mime_invalid")
    );
    let oversized = Upstream::start(
        "application/json",
        vec![b'x'; 32 * 1024 * 1024 + 1],
        200,
        false,
        3600,
    )
    .await;
    assert!(oversized
        .reader()
        .read_snapshot_verified(FILE_ID, &"0".repeat(64))
        .await
        .is_err());
}
use axum::{
    body::{to_bytes, Body},
    extract::State,
    http::{Request, Response as HttpResponse},
    routing::any,
    Router,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use std::sync::Mutex as StdMutex;

const FILE_ID: &str = "synthetic_drive_file_123";
const IMAGE: &[u8] = b"synthetic-image-original";

#[derive(Clone)]
struct Seen {
    path: String,
    query: String,
    method: String,
    authorization: Option<String>,
    body: Vec<u8>,
}
struct Fixture {
    requests: StdMutex<Vec<Seen>>,
    mime: &'static str,
    bytes: Vec<u8>,
    file_status: u16,
    token_redirect: bool,
    token_ttl: u64,
}
struct Upstream {
    base: String,
    fixture: Arc<Fixture>,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Upstream {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Upstream {
    async fn start(
        mime: &'static str,
        bytes: Vec<u8>,
        file_status: u16,
        token_redirect: bool,
        token_ttl: u64,
    ) -> Self {
        let fixture = Arc::new(Fixture {
            requests: StdMutex::new(vec![]),
            mime,
            bytes,
            file_status,
            token_redirect,
            token_ttl,
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
            task,
        }
    }
    fn reader(&self) -> DriveReader {
        let key = serde_json::json!({"client_email":"synthetic@fixture.iam.gserviceaccount.com",
            "private_key":include_str!("../../../tests/fixtures/oidc/test_key_1.pem"),
            "token_uri":"https://untrusted.example.test/token"});
        DriveReader::build(
            &key.to_string(),
            format!("{}/token", self.base),
            format!("{}/drive/v3/files", self.base),
        )
        .unwrap()
    }
    fn calls(&self) -> Vec<Seen> {
        self.fixture.requests.lock().unwrap().clone()
    }
}

async fn reply(State(fixture): State<Arc<Fixture>>, request: Request<Body>) -> HttpResponse<Body> {
    let (parts, body) = request.into_parts();
    let path = parts.uri.path().to_owned();
    let body = to_bytes(body, 64 * 1024).await.unwrap().to_vec();
    fixture.requests.lock().unwrap().push(Seen {
        path: path.clone(),
        query: parts.uri.query().unwrap_or("").to_owned(),
        method: parts.method.to_string(),
        authorization: parts
            .headers
            .get("authorization")
            .map(|value| value.to_str().unwrap().to_owned()),
        body,
    });
    if path == "/token" {
        if fixture.token_redirect {
            return HttpResponse::builder()
                .status(307)
                .header("Location", "/redirect-target")
                .body(Body::empty())
                .unwrap();
        }
        return HttpResponse::builder().header("Content-Type", "application/json").body(Body::from(serde_json::json!({
            "access_token":"synthetic-local-token", "token_type":"Bearer", "expires_in":fixture.token_ttl,
        }).to_string())).unwrap();
    }
    HttpResponse::builder()
        .status(fixture.file_status)
        .header("Content-Type", fixture.mime)
        .header("Location", "/redirect-target")
        .body(Body::from(fixture.bytes.clone()))
        .unwrap()
}
fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

#[tokio::test]
async fn readonly_jwt_fixed_audience_and_cached_bearer_are_used() {
    let server = Upstream::start("image/png", IMAGE.to_vec(), 200, false, 3600).await;
    let reader = server.reader();
    assert_eq!(
        reader.read_image(FILE_ID, &digest(IMAGE)).await.unwrap(),
        ("image/png".to_owned(), IMAGE.to_vec())
    );
    reader.read_image(FILE_ID, &digest(IMAGE)).await.unwrap();
    let calls = server.calls();
    assert_eq!(calls.iter().filter(|call| call.path == "/token").count(), 1);
    let token = &calls[0];
    assert_eq!(token.method, "POST");
    assert!(token.authorization.is_none());
    let form = std::str::from_utf8(&token.body).unwrap();
    let assertion = form
        .split('&')
        .find_map(|entry| entry.strip_prefix("assertion="))
        .unwrap();
    let payload = assertion.split('.').nth(1).unwrap();
    let claims: Value = serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload).unwrap()).unwrap();
    assert_eq!(claims["scope"], SCOPE);
    assert_eq!(claims["aud"], TOKEN_URL);
    assert_eq!(
        claims["exp"].as_u64().unwrap() - claims["iat"].as_u64().unwrap(),
        3600
    );
    for call in &calls[1..] {
        assert_eq!(call.method, "GET");
        assert_eq!(call.path, format!("/drive/v3/files/{FILE_ID}"));
        assert!(call.query.contains("alt=media"));
        assert!(call.query.contains("supportsAllDrives=true"));
        assert_eq!(
            call.authorization.as_deref(),
            Some("Bearer synthetic-local-token")
        );
    }
}

#[tokio::test]
async fn concurrent_reads_share_one_token_refresh() {
    let server = Upstream::start("image/webp", IMAGE.to_vec(), 200, false, 3600).await;
    let reader = server.reader();
    let hash = digest(IMAGE);
    let (one, two) = tokio::join!(
        reader.read_image(FILE_ID, &hash),
        reader.read_image(FILE_ID, &hash)
    );
    assert!(one.is_ok() && two.is_ok());
    assert_eq!(
        server
            .calls()
            .iter()
            .filter(|call| call.path == "/token")
            .count(),
        1
    );
}

#[tokio::test]
async fn expired_cache_refreshes_without_exposing_old_token() {
    let server = Upstream::start("image/jpeg", IMAGE.to_vec(), 200, false, 3600).await;
    let reader = server.reader();
    reader.read_image(FILE_ID, &digest(IMAGE)).await.unwrap();
    reader.inner.token.lock().await.as_mut().unwrap().expires =
        Instant::now() - Duration::from_secs(1);
    reader.read_image(FILE_ID, &digest(IMAGE)).await.unwrap();
    assert_eq!(
        server
            .calls()
            .iter()
            .filter(|call| call.path == "/token")
            .count(),
        2
    );
}

#[tokio::test]
async fn file_id_and_hash_injection_are_rejected_before_any_http() {
    let server = Upstream::start("image/png", IMAGE.to_vec(), 200, false, 3600).await;
    let reader = server.reader();
    for id in [
        "../token",
        "https://evil.example/x",
        "id123456789?alt=media",
        "file#fragment",
    ] {
        assert_eq!(
            reader.read_image(id, &digest(IMAGE)).await.unwrap_err(),
            DriveError("drive_file_id_invalid")
        );
    }
    assert_eq!(
        reader
            .read_manifest_verified(FILE_ID, "invalid")
            .await
            .unwrap_err(),
        DriveError("drive_hash_invalid")
    );
    assert!(server.calls().is_empty());
}

#[tokio::test]
async fn manifest_hash_is_checked_before_json_and_only_objects_are_accepted() {
    let raw = br#"{"schemaVersion":1,"images":[]}"#.to_vec();
    let server = Upstream::start("application/json", raw.clone(), 200, false, 3600).await;
    assert_eq!(
        server
            .reader()
            .read_manifest_verified(FILE_ID, &digest(&raw))
            .await
            .unwrap()["schemaVersion"],
        1
    );
    assert_eq!(
        server
            .reader()
            .read_manifest_verified(FILE_ID, &"0".repeat(64))
            .await
            .unwrap_err(),
        DriveError("drive_hash_mismatch")
    );
    let invalid = Upstream::start("application/json", b"[1,2]".to_vec(), 200, false, 3600).await;
    assert_eq!(
        invalid.reader().read_manifest(FILE_ID).await.unwrap_err(),
        DriveError("drive_manifest_invalid")
    );
}

#[tokio::test]
async fn manifest_and_image_size_limits_reject_large_responses() {
    let manifest = Upstream::start(
        "application/json",
        vec![b' '; MANIFEST_LIMIT + 1],
        200,
        false,
        3600,
    )
    .await;
    assert_eq!(
        manifest.reader().read_manifest(FILE_ID).await.unwrap_err(),
        DriveError("drive_file_too_large")
    );
    let image = Upstream::start("image/png", vec![b'x'; IMAGE_LIMIT + 1], 200, false, 3600).await;
    assert_eq!(
        image
            .reader()
            .read_image(FILE_ID, &"0".repeat(64))
            .await
            .unwrap_err(),
        DriveError("drive_file_too_large")
    );
}

#[tokio::test]
async fn unsafe_image_mime_and_content_mismatch_are_rejected() {
    let wrong = Upstream::start("image/svg+xml", IMAGE.to_vec(), 200, false, 3600).await;
    assert_eq!(
        wrong
            .reader()
            .read_image(FILE_ID, &digest(IMAGE))
            .await
            .unwrap_err(),
        DriveError("drive_image_mime_invalid")
    );
    let png = Upstream::start("image/png", IMAGE.to_vec(), 200, false, 3600).await;
    assert_eq!(
        png.reader()
            .read_image(FILE_ID, &"0".repeat(64))
            .await
            .unwrap_err(),
        DriveError("drive_hash_mismatch")
    );
}

#[tokio::test]
async fn token_and_file_redirects_are_not_followed() {
    let file = Upstream::start("image/png", IMAGE.to_vec(), 307, false, 3600).await;
    assert_eq!(
        file.reader()
            .read_image(FILE_ID, &digest(IMAGE))
            .await
            .unwrap_err(),
        DriveError("drive_file_failed")
    );
    assert!(!file
        .calls()
        .iter()
        .any(|call| call.path == "/redirect-target"));
    let token = Upstream::start("image/png", IMAGE.to_vec(), 200, true, 3600).await;
    assert_eq!(
        token
            .reader()
            .read_image(FILE_ID, &digest(IMAGE))
            .await
            .unwrap_err(),
        DriveError("drive_token_rejected")
    );
    assert_eq!(token.calls().len(), 1);
}

#[tokio::test]
async fn permissions_missing_and_invalid_token_ttl_have_static_errors() {
    for (status, error) in [
        (403, "drive_file_denied"),
        (404, "drive_file_missing"),
        (500, "drive_file_failed"),
    ] {
        let server = Upstream::start(
            "image/png",
            b"secret upstream message".to_vec(),
            status,
            false,
            3600,
        )
        .await;
        assert_eq!(
            server
                .reader()
                .read_image(FILE_ID, &digest(IMAGE))
                .await
                .unwrap_err(),
            DriveError(error)
        );
    }
    let short = Upstream::start("image/png", IMAGE.to_vec(), 200, false, 5).await;
    assert_eq!(
        short
            .reader()
            .read_image(FILE_ID, &digest(IMAGE))
            .await
            .unwrap_err(),
        DriveError("drive_token_invalid")
    );
    assert_eq!(short.calls().len(), 1);
}

#[test]
fn invalid_service_key_is_not_echoed_in_errors() {
    let error = DriveReader::new_with_key(r#"{"private_key":"synthetic-secret"}"#)
        .err()
        .unwrap();
    assert_eq!(error.to_string(), "drive_service_key_invalid");
    assert_eq!(
        format!("{error:?}"),
        "DriveError(\"drive_service_key_invalid\")"
    );
}
