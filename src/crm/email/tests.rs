use super::*;

fn reply_message(from: &str) -> Value {
    json!({"id":"reply-301","threadId":"thread-201","labelIds":["INBOX"],"internalDate":(chrono::Utc::now()+chrono::Duration::minutes(1)).timestamp_millis().to_string(),"payload":{"mimeType":"text/html","headers":[{"name":"Message-ID","value":"<reply-301@customer.invalid>"},{"name":"From","value":from},{"name":"To","value":"Taro <taro@example.com>"},{"name":"Cc","value":"cc@example.invalid"},{"name":"Subject","value":"Re: 採用のご相談"},{"name":"Content-Type","value":"text/html; charset=UTF-8"}],"body":{"data":URL_SAFE_NO_PAD.encode("<p>火曜日14時でお願いします。</p>")}}})
}
async fn reply_env() -> Env {
    let e = env(true).await;
    assert_eq!(send(&e, &request()).await.0, 200);
    e.fake.lock().unwrap().replies = vec![reply_message("Customer <contact@example.invalid>")];
    e
}
async fn scan(e: &Env) {
    replies::run_round(
        &e.audit,
        &e.client,
        &e.service,
        &["example.com".into()],
        &crate::crm::rbac::CrmAccess::from_list(""),
    )
    .await
    .unwrap();
}
async fn incoming_row(e: &Env) -> OpRow {
    pending::blocking(&e.audit, |t| {
        let ids=t.query("SELECT operation_id FROM crm_pending_operations WHERE json_extract(payload,'$.email.reply') IS NOT NULL",&[])?;
        pending::get_op(t,&crate::handlers::helpers::get_str(&ids[0],"operation_id"))
    }).await.unwrap().unwrap().unwrap()
}
#[tokio::test]
async fn reply_import_one_and_second_round_has_zero_hubspot_calls() {
    let e = reply_env().await;
    scan(&e).await;
    assert_eq!(incoming_row(&e).await.status, "saved");
    let searches = e.fake.lock().unwrap().searches.len();
    scan(&e).await;
    let f = e.fake.lock().unwrap();
    assert_eq!(f.create_calls, 2); // one outgoing + exactly one incoming
    assert_eq!(f.searches.len(), searches);
    assert_eq!(f.reply_reads.len(), 1);
    assert_eq!(
        f.thread_reads,
        vec!["/gmail/users/me/threads/thread-201"; 2]
    );
    let p = &f.created[1]["properties"];
    assert_eq!(p["hs_email_direction"], "INCOMING_EMAIL");
    assert_eq!(p["hs_email_message_id"], "<reply-301@customer.invalid>");
    assert_eq!(p["hs_email_thread_id"], "thread-201");
    assert_eq!(p["hs_email_html"], "<p>火曜日14時でお願いします。</p>");
    assert_eq!(p["hs_email_subject"], "Re: 採用のご相談");
    let h: Value = serde_json::from_str(p["hs_email_headers"].as_str().unwrap()).unwrap();
    assert_eq!(h["from"]["email"], "contact@example.invalid");
    assert_eq!(h["to"][0]["email"], "taro@example.com");
    assert!(f
        .associations
        .contains(&"/crm/v3/objects/emails/903/associations/deals/700/210".into()));
    assert!(f
        .associations
        .contains(&"/crm/v3/objects/emails/903/associations/contacts/800/198".into()));
}
#[tokio::test]
async fn reply_existing_hubspot_record_skips_create_and_retains_local_receipt() {
    let e = reply_env().await;
    e.fake
        .lock()
        .unwrap()
        .created
        .push(json!({"properties":{"hs_email_message_id":"<reply-301@customer.invalid>"}}));
    scan(&e).await;
    scan(&e).await;
    assert_eq!(e.fake.lock().unwrap().create_calls, 1);
    assert_eq!(
        incoming_row(&e)
            .await
            .payload
            .email
            .unwrap()
            .hubspot_email_id
            .as_deref(),
        Some("903")
    );
}
#[tokio::test]
async fn reply_internal_and_self_are_ignored_with_no_hubspot_calls() {
    let e = reply_env().await;
    e.fake.lock().unwrap().replies = vec![
        reply_message("Staff <STAFF@EXAMPLE.COM>"),
        reply_message("taro@example.com"),
    ];
    let before = e.fake.lock().unwrap().searches.len();
    scan(&e).await;
    let f = e.fake.lock().unwrap();
    assert_eq!(f.searches.len(), before);
    assert_eq!(f.create_calls, 1);
    assert!(f.reply_reads.is_empty());
}
#[tokio::test]
async fn reply_old_thread_is_not_read() {
    let e = reply_env().await;
    let mut r = row(&e).await;
    r.payload.email.as_mut().unwrap().sent_at =
        pending::iso(chrono::Utc::now() - chrono::Duration::days(31));
    persist(&e.audit, &r).await.unwrap();
    scan(&e).await;
    assert!(e.fake.lock().unwrap().thread_reads.is_empty());
    assert_eq!(e.fake.lock().unwrap().create_calls, 1);
}
#[tokio::test]
async fn reply_expired_grant_is_skipped_and_exposed_until_renewed() {
    let e = reply_env().await;
    e.fake.lock().unwrap().expired = true;
    scan(&e).await;
    let calls = e.fake.lock().unwrap().token_requests.len();
    scan(&e).await;
    let (_, s) = call(
        &e,
        Method::GET,
        "/api/crm/gmail/replies/status",
        Value::Null,
    )
    .await;
    assert_eq!(s["grant_expired"], true);
    assert_eq!(e.fake.lock().unwrap().token_requests.len(), calls);
    assert!(e.fake.lock().unwrap().thread_reads.is_empty());
    let enc = grants::encrypt(&[7; 32], "taro@example.com", "renewed-refresh").unwrap();
    pending::blocking(&e.audit, move |t| {
        grants::save(t, "taro@example.com", &enc, &SCOPES.join(" "))
    })
    .await
    .unwrap()
    .unwrap();
    e.fake.lock().unwrap().expired = false;
    scan(&e).await;
    assert_eq!(e.fake.lock().unwrap().create_calls, 2);
}
#[tokio::test]
async fn reply_hubspot_failure_queues_then_retry_creates_once() {
    let e = reply_env().await;
    e.fake.lock().unwrap().fail_search = 1;
    scan(&e).await;
    let r = incoming_row(&e).await;
    assert_eq!(r.status, "pending");
    assert_eq!(r.attempts, 1);
    assert_eq!(e.fake.lock().unwrap().create_calls, 1);
    assert!(process_pending(&e.audit, &e.client, &e.write, &r).await);
    assert_eq!(incoming_row(&e).await.status, "saved");
    scan(&e).await;
    assert_eq!(e.fake.lock().unwrap().create_calls, 2);
    assert_eq!(e.fake.lock().unwrap().sends, 1);
}
#[tokio::test]
async fn reply_two_scanners_and_stale_pending_row_do_not_create_twice() {
    let e = reply_env().await;
    tokio::join!(scan(&e), scan(&e));
    let mut r = incoming_row(&e).await;
    let saved = r.clone();
    r.status = "pending".into();
    r.payload.email.as_mut().unwrap().hubspot_email_id = None;
    // A stale in-memory operation is reloaded after the database lease.
    replies::record(&e.audit, &e.client, &mut r).await.unwrap();
    assert_eq!(r.status, saved.status);
    assert_eq!(e.fake.lock().unwrap().create_calls, 2);
}
#[tokio::test]
async fn reply_lost_create_response_reconciles_without_second_create() {
    let e = reply_env().await;
    e.fake.lock().unwrap().lose_create = true;
    scan(&e).await;
    let r = incoming_row(&e).await;
    assert_eq!(r.status, "pending");
    assert!(r.payload.email.as_ref().unwrap().create_uncertain);
    e.fake.lock().unwrap().hidden_search = 1;
    process_pending(&e.audit, &e.client, &e.write, &r).await;
    assert_eq!(incoming_row(&e).await.status, "pending");
    let r = incoming_row(&e).await;
    process_pending(&e.audit, &e.client, &e.write, &r).await;
    assert_eq!(incoming_row(&e).await.status, "saved");
    assert_eq!(e.fake.lock().unwrap().create_calls, 2);
}
#[tokio::test]
async fn reply_plain_text_is_escaped_and_autoreply_is_imported() {
    let e = reply_env().await;
    {
        let mut f = e.fake.lock().unwrap();
        let m = &mut f.replies[0];
        m["payload"]["mimeType"] = json!("text/plain");
        m["payload"]["body"]["data"] =
            json!(URL_SAFE_NO_PAD.encode("<script>alert(1)</script> &\n不在です"));
        m["payload"]["headers"]
            .as_array_mut()
            .unwrap()
            .push(json!({"name":"Auto-Submitted","value":"auto-replied"}));
    }
    scan(&e).await;
    let f = e.fake.lock().unwrap();
    assert_eq!(
        f.created[1]["properties"]["hs_email_html"],
        "<p>&lt;script&gt;alert(1)&lt;/script&gt; &amp;<br>不在です</p>"
    );
    assert_eq!(
        f.created[1]["properties"]["hs_email_subject"],
        "【自動返信】Re: 採用のご相談"
    );
}
#[tokio::test]
async fn reply_revoked_or_disabled_accounts_are_never_read() {
    let e = reply_env().await;
    pending::blocking(&e.audit, |t| grants::revoke(t, "taro@example.com"))
        .await
        .unwrap()
        .unwrap();
    scan(&e).await;
    assert!(e.fake.lock().unwrap().thread_reads.is_empty());
    assert_eq!(
        call(
            &e,
            Method::GET,
            "/api/crm/gmail/replies/status",
            Value::Null
        )
        .await
        .1["grant_expired"],
        true
    );
    let enc = grants::encrypt(&[7; 32], "taro@example.com", "renewed-refresh").unwrap();
    pending::blocking(&e.audit,move |t| {
        grants::save(t,"taro@example.com",&enc,&SCOPES.join(" "))?;
        t.execute("INSERT INTO accounts(id,email,first_seen_at,disabled_at) VALUES ('disabled-1','taro@example.com','2026-10-11T00:00:00Z','2026-10-11T00:01:00Z')",&[])
    }).await.unwrap().unwrap();
    scan(&e).await;
    assert!(e.fake.lock().unwrap().thread_reads.is_empty());
}

#[tokio::test]
async fn reply_one_hundred_operators_without_new_mail_make_zero_hubspot_calls() {
    let e = reply_env().await;
    e.fake.lock().unwrap().replies.clear();
    let original = row(&e).await;
    pending::blocking(&e.audit, move |t| {
        for n in 1..100 {
            let who = format!("operator-{n}@example.com");
            let enc = grants::encrypt(&[7; 32], &who, "fake-refresh").unwrap();
            grants::save(t, &who, &enc, &SCOPES.join(" "))?;
            pending::insert_op(
                t,
                &format!("seed-outgoing-{n}"),
                &who,
                "700",
                &original.payload,
                "emails",
            )?;
        }
        Ok::<_, String>(())
    })
    .await
    .unwrap()
    .unwrap();
    let searches = e.fake.lock().unwrap().searches.len();
    scan(&e).await;
    let f = e.fake.lock().unwrap();
    assert_eq!(f.thread_reads.len(), 100);
    assert_eq!(f.searches.len(), searches);
    assert_eq!(f.create_calls, 1);
    assert!(f.reply_reads.is_empty());
}

#[tokio::test]
async fn reply_multipart_prefers_html_and_decodes_subject() {
    let e = reply_env().await;
    {
        let mut f = e.fake.lock().unwrap();
        let m = &mut f.replies[0];
        m["payload"]["mimeType"] = json!("multipart/alternative");
        m["payload"]["body"] = json!({});
        let encoded = STANDARD.encode("Re: 採用のご相談");
        m["payload"]["headers"][4]["value"] = json!(format!("=?UTF-8?B?{encoded}?="));
        m["payload"]["parts"] = json!([
            {"mimeType":"text/plain","body":{"data":URL_SAFE_NO_PAD.encode("plain version")}},
            {"mimeType":"text/html","headers":[{"name":"Content-Type","value":"text/html; charset=UTF-8"}],"body":{"data":URL_SAFE_NO_PAD.encode("<p>HTML の返信</p>")}}
        ]);
    }
    scan(&e).await;
    let f = e.fake.lock().unwrap();
    assert_eq!(
        f.created[1]["properties"]["hs_email_html"],
        "<p>HTML の返信</p>"
    );
    assert_eq!(
        f.created[1]["properties"]["hs_email_subject"],
        "Re: 採用のご相談"
    );
}

#[tokio::test]
async fn reply_two_independent_record_workers_use_database_claim() {
    let e = reply_env().await;
    e.fake.lock().unwrap().fail_search = 1;
    scan(&e).await;
    let mut a = incoming_row(&e).await;
    let mut b = a.clone();
    let searches = e.fake.lock().unwrap().searches.len();
    let (a_result, b_result) = tokio::join!(
        replies::record(&e.audit, &e.client, &mut a),
        replies::record(&e.audit, &e.client, &mut b)
    );
    assert!(a_result.is_ok() && b_result.is_ok());
    assert_eq!(incoming_row(&e).await.status, "saved");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.create_calls, 2);
    assert_eq!(f.searches.len(), searches + 1);
}

#[test]
fn token_cipher_is_random_and_bound_to_operator() {
    let key = [7; 32];
    let a = grants::encrypt(&key, "taro@example.com", "refresh-secret-123").unwrap();
    let b = grants::encrypt(&key, "taro@example.com", "refresh-secret-123").unwrap();
    assert_ne!(a, b);
    assert!(!a.contains("refresh-secret"));
    assert_eq!(
        grants::decrypt(&key, "taro@example.com", &a).unwrap(),
        "refresh-secret-123"
    );
    assert!(grants::decrypt(&key, "hanako@example.com", &a).is_err());
    assert!(grants::decrypt(&[8; 32], "taro@example.com", &a).is_err());
}
#[test]
fn requires_both_exact_scopes_and_rejects_header_injection() {
    assert!(grants::has_scopes(&SCOPES.join(" ")));
    assert!(!grants::has_scopes(SCOPES[0]));
    for addr in [
        "a@example.com\r\nBcc:evil@example.com",
        "a@example.com,evil@example.com",
        "a@b@c.com",
        "<a@example.com>",
    ] {
        assert!(!valid_address(addr));
    }
}
#[test]
fn export_ts_bindings() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("frontend/src/generated");
    let cfg = ts_rs::Config::new().with_out_dir(dir);
    SendEmailRequest::export_all(&cfg).unwrap();
    EmailResult::export_all(&cfg).unwrap();
    GmailGrantStatus::export_all(&cfg).unwrap();
    PendingEmailActivity::export_all(&cfg).unwrap();
    replies::ReplySyncStatus::export_all(&cfg).unwrap();
}

fn test_config() -> crate::config::AppConfig {
    crate::config::AppConfig {
        port: 0,
        auth_password: "internal-pass".to_string(),
        auth_password_hash: String::new(),
        external_passwords: Vec::new(),
        allowed_domains: vec!["example.com".to_string()],
        allowed_domains_extra: Vec::new(),
        hellowork_db_path: String::new(),
        indeed_db_path: String::new(),
        cache_ttl_secs: 60,
        cache_max_entries: 10,
        rate_limit_max_attempts: 50,
        rate_limit_lockout_secs: 60,
        audit_turso_url: String::new(),
        audit_turso_token: String::new(),
        audit_ip_salt: String::new(),
        admin_emails: vec!["admin@example.com".to_string()],
        turso_external_url: String::new(),
        turso_external_token: String::new(),
        salesnow_turso_url: String::new(),
        salesnow_turso_token: String::new(),
        scout_turso_url: String::new(),
        scout_turso_token: String::new(),
    }
}

use crate::{
    audit::{
        fake_turso::{start_sqlite_audit, SharedConn},
        AuditDb,
    },
    hubspot::{ClientOptions, Gateway, GatewayConfig},
    AppState,
};
use axum::{
    body::{to_bytes, Body, Bytes},
    extract::{OriginalUri, State},
    http::{header, Method, Request, StatusCode},
    response::{IntoResponse, Response},
    routing::post,
    Json, Router,
};
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use serde_json::Value;
use tower::ServiceExt;
use tower_sessions::{MemoryStore, Session, SessionManagerLayer};

#[derive(Default)]
struct Fake {
    replies: Vec<Value>,
    thread_reads: Vec<String>,
    reply_reads: Vec<String>,
    expired: bool,
    sends: usize,
    send_attempts: usize,
    reject_gmail: bool,
    lose_send: bool,
    metadata_id: Option<String>,
    searches: Vec<Value>,
    raw: String,
    create_calls: usize,
    created: Vec<Value>,
    associations: Vec<String>,
    fail_search: usize,
    lose_create: bool,
    hidden_search: usize,
    token_requests: Vec<String>,
    consent_email: Option<String>,
    consent_scopes: Option<String>,
    nonce: String,
    revoke_calls: usize,
}
type SharedFake = Arc<Mutex<Fake>>;
async fn fake(
    State(f): State<SharedFake>,
    OriginalUri(uri): OriginalUri,
    method: Method,
    bytes: Bytes,
) -> Response {
    let mut f = f.lock().unwrap();
    let body: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    let path = uri.path();
    let mut status = StatusCode::OK;
    let value = match path {
        "/token" => {
            if f.expired {
                status = StatusCode::BAD_REQUEST;
            }
            let form = String::from_utf8_lossy(&bytes).to_string();
            f.token_requests.push(form);
            let mut h = jsonwebtoken::Header::new(jsonwebtoken::Algorithm::RS256);
            h.kid = Some("test-key-1".into());
            let key = jsonwebtoken::EncodingKey::from_rsa_pem(
                include_str!("../../../tests/fixtures/oidc/test_key_1.pem").as_bytes(),
            )
            .unwrap();
            let token = jsonwebtoken::encode(&h,&json!({"iss":"https://accounts.google.com","aud":"test-client","sub":"operator-1","email":f.consent_email.clone().unwrap_or("taro@example.com".into()),"hd":"example.com","email_verified":true,"nonce":f.nonce,"exp":chrono::Utc::now().timestamp()+3600}),&key).unwrap();
            json!({"access_token":"access-test","refresh_token":"refresh-test-123","id_token":token,"scope":f.consent_scopes.clone().unwrap_or(SCOPES.join(" "))})
        }
        "/jwks" => {
            serde_json::from_str(include_str!("../../../tests/fixtures/oidc/jwks.json")).unwrap()
        }
        "/revoke" => {
            f.revoke_calls += 1;
            json!({})
        }
        "/gmail/users/me/messages/send" => {
            f.send_attempts += 1;
            f.raw = body["raw"].as_str().unwrap_or_default().into();
            if f.reject_gmail {
                status = StatusCode::FORBIDDEN;
                json!({"error":"rejected"})
            } else {
                f.sends += 1;
                if f.lose_send {
                    status = StatusCode::INTERNAL_SERVER_ERROR;
                }
                json!({"id":"gmail-101","threadId":"thread-201"})
            }
        }
        "/gmail/users/me/messages/gmail-101" => {
            let raw = URL_SAFE_NO_PAD.decode(&f.raw).unwrap_or_default();
            let mime = String::from_utf8_lossy(&raw);
            let generated = mime
                .lines()
                .find_map(|l| l.strip_prefix("Message-ID: "))
                .unwrap_or("<recovered@example.com>");
            json!({"id":"gmail-101","threadId":"thread-201","payload":{"headers":[{"name":"Message-ID","value":f.metadata_id.as_deref().unwrap_or(generated)}]}})
        }
        "/gmail/users/me/messages" => {
            if f.sends > 0 {
                json!({"messages":[{"id":"gmail-101","threadId":"thread-201"}]})
            } else {
                json!({"messages":[]})
            }
        }
        p if p.starts_with("/gmail/users/me/threads/") => {
            f.thread_reads.push(p.into());
            assert!(uri.query().unwrap_or_default().contains("format=metadata"));
            json!({"messages":f.replies})
        }
        p if p.starts_with("/gmail/users/me/messages/reply-") => {
            f.reply_reads.push(p.into());
            assert!(uri.query().unwrap_or_default().contains("format=full"));
            f.replies
                .iter()
                .find(|m| Some(p.rsplit('/').next().unwrap()) == m["id"].as_str())
                .cloned()
                .unwrap_or(Value::Null)
        }
        "/crm/v3/objects/deals/700" => {
            json!({"id":"700","properties":{},"archived":false,"associations":{"contacts":{"results":[{"id":"800","type":"deal_to_contact"},{"id":"801","type":"deal_to_contact"}]}}})
        }
        "/crm/v3/objects/contacts/batch/read" => {
            json!({"results":[{"id":"800","properties":{"email":"contact@example.invalid"},"archived":false},{"id":"801","properties":{"email":"cc@example.invalid"},"archived":false}]})
        }
        "/crm/v3/owners" => {
            json!({"results":[{"id":"900","email":"taro@example.com","archived":false}]})
        }
        "/crm/v3/objects/emails/search" => {
            f.searches.push(body.clone());
            if f.fail_search > 0 {
                f.fail_search -= 1;
                status = StatusCode::SERVICE_UNAVAILABLE;
                json!({})
            } else if f.hidden_search > 0 {
                f.hidden_search -= 1;
                json!({"results":[]})
            } else {
                json!({"results":f.created.iter().enumerate().filter(|(_,e)| e["properties"]["hs_email_message_id"] == body["filterGroups"][0]["filters"][0]["value"]).map(|(i,e)|json!({"id":(902+i).to_string(),"properties":e["properties"]})).collect::<Vec<_>>()})
            }
        }
        "/crm/v3/objects/emails" if method == Method::POST => {
            f.create_calls += 1;
            f.created.push(body.clone());
            if f.lose_create {
                f.lose_create = false;
                status = StatusCode::INTERNAL_SERVER_ERROR;
                json!({})
            } else {
                json!({"id":(901+f.created.len()).to_string(),"properties":body["properties"]})
            }
        }
        p if p.starts_with("/crm/v3/objects/emails/") && p.contains("/associations/") => {
            assert_eq!(method, Method::PUT);
            f.associations.push(p.into());
            json!({})
        }
        _ => {
            status = StatusCode::NOT_FOUND;
            json!({"path":path})
        }
    };
    (status, Json(value)).into_response()
}
async fn inject(session: Session) -> StatusCode {
    session
        .insert(crate::auth::SESSION_USER_KEY, "taro@example.com")
        .await
        .unwrap();
    session
        .insert(crate::auth::SESSION_LOGIN_METHOD_KEY, "google_oidc")
        .await
        .unwrap();
    StatusCode::NO_CONTENT
}
struct Env {
    app: Router,
    fake: SharedFake,
    service: Arc<EmailService>,
    client: Arc<HubSpotClient>,
    audit: AuditDb,
    conn: SharedConn,
    cookie: String,
    write: WriteConfig,
}
async fn env(granted: bool) -> Env {
    let fake_state = Arc::new(Mutex::new(Fake::default()));
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let fake_app = Router::new().fallback(fake).with_state(fake_state.clone());
    tokio::spawn(async move {
        axum::serve(listener, fake_app).await.unwrap();
    });
    let google = GoogleOidcConfig {
        client_id: "test-client".into(),
        client_secret: "test-secret".into(),
        redirect_url: "http://localhost/auth/google/callback".into(),
        hosted_domain: "example.com".into(),
    };
    let service = Arc::new(EmailService::new(EmailConfig {
        key: [7; 32],
        google: google.clone(),
        redirect_url: format!("http://localhost{CALLBACK}"),
        base_url: "http://localhost".into(),
        authorization_url: format!("{base}/authorize"),
        token_url: format!("{base}/token"),
        revoke_url: format!("{base}/revoke"),
        gmail_url: format!("{base}/gmail"),
    }));
    let client = Arc::new(
        HubSpotClient::with_gateway(
            "test-hubspot".into(),
            &base,
            ClientOptions {
                max_retries: 0,
                ..ClientOptions::default()
            },
            Arc::new(Gateway::new(GatewayConfig {
                per_second: 100,
                per_10s: 1000,
                search_interval: Duration::ZERO,
                ..GatewayConfig::default()
            })),
        )
        .unwrap(),
    );
    let (audit, conn) = start_sqlite_audit().await;
    if granted {
        let enc = grants::encrypt(&[7; 32], "taro@example.com", "refresh-test-123").unwrap();
        pending::blocking(&audit, move |t| {
            grants::save(t, "taro@example.com", &enc, &SCOPES.join(" "))
        })
        .await
        .unwrap()
        .unwrap();
    }
    let state = Arc::new(AppState {
        config: test_config(),
        hw_db: None,
        indeed_db: None,
        turso_db: None,
        salesnow_db: None,
        scout_db: None,
        cache: crate::db::cache::AppCache::new(60, 10),
        rate_limiter: crate::auth::session::RateLimiter::new(50, 60),
        company_geo_cache: None,
        audit: Some(audit.clone()),
        hubspot: Some(client.clone()),
        google_oidc: Some(Arc::new(
            crate::auth::google_oidc::GoogleOidc::with_endpoints(
                google,
                crate::auth::google_oidc::Endpoints {
                    issuer: "https://accounts.google.com".into(),
                    authorization_endpoint: format!("{base}/authorize"),
                    token_endpoint: format!("{base}/token"),
                    jwks_uri: format!("{base}/jwks"),
                },
            ),
        )),
    });
    let write = WriteConfig {
        email: Some(service.clone()),
        ..WriteConfig::default()
    };
    let app = Router::new()
        .merge(crate::crm::routes::router_with_write(
            crate::crm::rbac::CrmAccess::from_list(""),
            crate::crm::call_queue::CallQueueState::new(),
            crate::crm::workspace_cache::WorkspaceCache::default(),
            write.clone(),
        ))
        .route("/test/login", post(inject))
        .with_state(state)
        .layer(SessionManagerLayer::new(MemoryStore::default()));
    let r = app
        .clone()
        .oneshot(Request::post("/test/login").body(Body::empty()).unwrap())
        .await
        .unwrap();
    let cookie = r.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    Env {
        app,
        fake: fake_state,
        service,
        client,
        audit,
        conn,
        cookie,
        write,
    }
}
fn request() -> SendEmailRequest {
    SendEmailRequest {
        operation_id: "email-op-001".into(),
        hubspot_deal_id: "700".into(),
        contact_id: "800".into(),
        to: "contact@example.invalid".into(),
        cc: vec!["cc@example.invalid".into()],
        subject: "採用のご相談".into(),
        html: "<p>山田様<br>採用人数は3名です。</p>".into(),
    }
}
async fn call(e: &Env, method: Method, path: &str, body: Value) -> (u16, Value) {
    let r = e
        .app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(path)
                .header(header::COOKIE, &e.cookie)
                .header(header::CONTENT_TYPE, "application/json")
                .header("X-Requested-With", "fetch")
                .body(Body::from(body.to_string()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = r.status().as_u16();
    let bytes = to_bytes(r.into_body(), 1024 * 1024).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}
async fn send(e: &Env, r: &SendEmailRequest) -> (u16, Value) {
    call(e, Method::POST, "/api/crm/emails", json!(r)).await
}
async fn row(e: &Env) -> OpRow {
    pending::blocking(&e.audit, |t| pending::get_op(t, "email-op-001"))
        .await
        .unwrap()
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn normal_send_records_headers_ids_owner_and_all_associations() {
    let e = env(true).await;
    let (status, v) = send(&e, &request()).await;
    assert_eq!(status, 200);
    assert_eq!(v["status"], "saved");
    assert_eq!(v["sent"], true);
    assert_eq!(v["recorded"], true);
    assert_eq!(v["hubspot_email_id"], "902");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.sends, 1);
    assert_eq!(f.create_calls, 1);
    let p = &f.created[0]["properties"];
    assert_eq!(p["hs_email_direction"], "EMAIL");
    assert_eq!(p["hs_email_status"], "SENT");
    assert_eq!(p["hubspot_owner_id"], "900");
    assert_eq!(p["hs_email_thread_id"], "thread-201");
    assert_eq!(p["hs_email_subject"], "採用のご相談");
    assert_eq!(p["hs_email_html"], "<p>山田様<br>採用人数は3名です。</p>");
    let headers: Value = serde_json::from_str(p["hs_email_headers"].as_str().unwrap()).unwrap();
    assert_eq!(headers["from"]["email"], "taro@example.com");
    assert_eq!(headers["to"][0]["email"], "contact@example.invalid");
    assert_eq!(headers["cc"][0]["email"], "cc@example.invalid");
    let mime = String::from_utf8(URL_SAFE_NO_PAD.decode(&f.raw).unwrap()).unwrap();
    assert!(mime.contains(&format!(
        "Message-ID: {}",
        p["hs_email_message_id"].as_str().unwrap()
    )));
    assert_eq!(
        f.created[0]["associations"][0]["types"][0]["associationTypeId"],
        210
    );
    assert_eq!(f.created[0]["associations"][1]["to"]["id"], "800");
    assert_eq!(f.associations.len(), 3);
}
#[tokio::test]
async fn gmail_rejection_is_red_no_send_and_no_hubspot_creation() {
    let e = env(true).await;
    e.fake.lock().unwrap().reject_gmail = true;
    let (status, v) = send(&e, &request()).await;
    assert_eq!(status, 502);
    assert_eq!(v["status"], "failed");
    assert_eq!(v["sent"], false);
    assert_eq!(v["recorded"], false);
    let (_, again) = send(&e, &request()).await;
    assert_eq!(again["status"], "failed");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.sends, 0);
    assert_eq!(f.send_attempts, 1);
    assert_eq!(f.create_calls, 0);
}
#[tokio::test]
async fn hubspot_failure_is_yellow_retry_creates_exactly_one_without_resending() {
    let e = env(true).await;
    e.fake.lock().unwrap().fail_search = 1;
    let (status, v) = send(&e, &request()).await;
    assert_eq!(status, 202);
    assert_eq!(v["status"], "pending");
    assert_eq!(v["sent"], true);
    assert_eq!(v["recorded"], false);
    let r = row(&e).await;
    assert!(crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await);
    let r = row(&e).await;
    assert_eq!(r.status, "saved");
    assert_eq!(r.payload.email.as_ref().unwrap().html, "");
    assert!(!crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await);
    let f = e.fake.lock().unwrap();
    assert_eq!(f.sends, 1);
    assert_eq!(f.create_calls, 1);
    assert_eq!(f.created.len(), 1);
}
#[tokio::test]
async fn same_operation_parallel_and_after_old_retention_never_sends_twice() {
    let e = env(true).await;
    let req = request();
    let (a, b) = tokio::join!(send(&e, &req), send(&e, &req));
    assert_eq!(a.0, 200);
    assert_eq!(b.0, 200);
    e.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE crm_pending_operations SET updated_at = '2020-01-01T00:00:00Z'",
            [],
        )
        .unwrap();
    pending::blocking(&e.audit, pending::purge_old)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(send(&e, &req).await.0, 200);
    let f = e.fake.lock().unwrap();
    assert_eq!(f.sends, 1);
    assert_eq!(f.create_calls, 1);
}
#[tokio::test]
async fn rejects_unrelated_to_cc_contact_and_header_injection() {
    let e = env(true).await;
    for (field, val, code) in [
        ("to", "stranger@example.invalid", 403),
        ("cc", "stranger@example.invalid", 403),
        ("contact_id", "899", 403),
        (
            "to",
            "contact@example.invalid\r\nBcc: evil@example.invalid",
            422,
        ),
    ] {
        let mut r = request();
        match field {
            "to" => r.to = val.into(),
            "cc" => r.cc = vec![val.into()],
            _ => r.contact_id = val.into(),
        };
        let (status, _) = send(&e, &r).await;
        assert_eq!(status, code, "{field}");
    }
    let f = e.fake.lock().unwrap();
    assert_eq!(f.send_attempts, 0);
    assert_eq!(f.create_calls, 0);
}
#[tokio::test]
async fn missing_grant_returns_403_without_gmail_or_hubspot_write() {
    let e = env(false).await;
    let (status, v) = send(&e, &request()).await;
    assert_eq!(status, 403);
    assert_eq!(v["error_kind"], "gmail_grant_required");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.send_attempts, 0);
    assert_eq!(f.create_calls, 0);
    assert!(f.token_requests.is_empty());
}
#[tokio::test]
async fn lost_create_with_delayed_search_only_reconciles_never_creates_again() {
    let e = env(true).await;
    e.fake.lock().unwrap().lose_create = true;
    assert_eq!(send(&e, &request()).await.0, 202);
    e.fake.lock().unwrap().hidden_search = 1;
    let r = row(&e).await;
    crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await;
    assert_eq!(row(&e).await.status, "pending");
    let r = row(&e).await;
    crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await;
    assert_eq!(row(&e).await.status, "saved");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.create_calls, 1);
    assert_eq!(f.created.len(), 1);
    assert_eq!(f.sends, 1);
}
#[tokio::test]
async fn consent_state_scope_identity_encryption_replay_and_revocation() {
    let e = env(false).await;
    let start = e
        .app
        .clone()
        .oneshot(
            Request::get("/api/crm/gmail/authorize")
                .header(header::COOKIE, &e.cookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(start.status(), StatusCode::SEE_OTHER);
    let url = reqwest::Url::parse(start.headers()[header::LOCATION].to_str().unwrap()).unwrap();
    let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
    assert_eq!(query["scope"], format!("openid email {}", SCOPES.join(" ")));
    assert_eq!(query["access_type"], "offline");
    assert_eq!(query["prompt"], "consent");
    assert_eq!(query["include_granted_scopes"], "true");
    assert_eq!(query["code_challenge_method"], "S256");
    let txcookie = start.headers()[header::SET_COOKIE]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_string();
    e.fake.lock().unwrap().nonce = query["nonce"].clone();
    let callback = format!("{CALLBACK}?code=fake-code&state={}", query["state"]);
    let r = e
        .app
        .clone()
        .oneshot(
            Request::get(&callback)
                .header(header::COOKIE, &txcookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::OK);
    let enc: String=e.conn.lock().unwrap().query_row("SELECT encrypted_refresh_token FROM crm_gmail_grants WHERE operator_email='taro@example.com'",[],|r|r.get(0)).unwrap();
    assert!(!enc.contains("refresh-test"));
    assert_eq!(
        grants::decrypt(&[7; 32], "taro@example.com", &enc).unwrap(),
        "refresh-test-123"
    );
    let r = e
        .app
        .clone()
        .oneshot(
            Request::get(callback)
                .header(header::COOKIE, txcookie)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(r.status(), StatusCode::BAD_REQUEST);
    assert_eq!(
        call(&e, Method::POST, "/api/crm/gmail/revoke", json!({}))
            .await
            .0,
        200
    );
    assert_eq!(
        call(&e, Method::GET, "/api/crm/gmail/status", json!({}))
            .await
            .1["granted"],
        false
    );
    assert_eq!(send(&e, &request()).await.0, 403);
    assert_eq!(e.fake.lock().unwrap().revoke_calls, 1);
}

#[tokio::test]
async fn invalid_state_missing_cookie_other_google_user_and_partial_consent_are_rejected() {
    for scenario in ["state", "cookie", "user", "scopes"] {
        let e = env(false).await;
        let (url, state) = e.service.authorization("taro@example.com".into()).unwrap();
        let q: HashMap<String, String> = reqwest::Url::parse(&url)
            .unwrap()
            .query_pairs()
            .into_owned()
            .collect();
        {
            let mut f = e.fake.lock().unwrap();
            f.nonce = q["nonce"].clone();
            if scenario == "user" {
                f.consent_email = Some("other@example.com".into());
            }
            if scenario == "scopes" {
                f.consent_scopes = Some(SCOPES[0].into());
            }
        }
        let actual_state = if scenario == "state" { "wrong" } else { &state };
        let actual_cookie = if scenario == "cookie" {
            String::new()
        } else {
            format!("hrhr_gmail_tx={state}")
        };
        let r = e
            .app
            .clone()
            .oneshot(
                Request::get(format!("{CALLBACK}?code=fake-code&state={actual_state}"))
                    .header(header::COOKIE, actual_cookie)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            r.status().as_u16(),
            if matches!(scenario, "state" | "cookie") {
                400
            } else {
                403
            },
            "{scenario}"
        );
        let n: i64 = e
            .conn
            .lock()
            .unwrap()
            .query_row("SELECT COUNT(*) FROM crm_gmail_grants", [], |r| r.get(0))
            .unwrap();
        assert_eq!(n, 0, "{scenario}");
        if matches!(scenario, "state" | "cookie") {
            assert!(e.fake.lock().unwrap().token_requests.is_empty());
        }
    }
}
#[tokio::test]
async fn unauthenticated_and_missing_csrf_requests_are_rejected_without_send() {
    let e = env(true).await;
    for (csrf, status) in [(true, 401), (false, 403)] {
        let mut builder =
            Request::post("/api/crm/emails").header(header::CONTENT_TYPE, "application/json");
        if csrf {
            builder = builder.header("X-Requested-With", "fetch");
        }
        let r = e
            .app
            .clone()
            .oneshot(
                builder
                    .body(Body::from(json!(request()).to_string()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(r.status().as_u16(), status);
    }
    assert_eq!(e.fake.lock().unwrap().send_attempts, 0);
}
#[tokio::test]
async fn pending_activity_list_restores_own_subject_and_state_before_recording() {
    let e = env(true).await;
    e.fake.lock().unwrap().fail_search = 1;
    assert_eq!(send(&e, &request()).await.0, 202);
    let (s, v) = call(&e, Method::GET, "/api/crm/emails?deal_id=700", json!({})).await;
    assert_eq!(s, 200);
    assert_eq!(v.as_array().unwrap().len(), 1);
    assert_eq!(v[0]["subject"], "採用のご相談");
    assert_eq!(v[0]["to"], "contact@example.invalid");
    assert_eq!(v[0]["result"]["status"], "pending");
    assert_eq!(v[0]["result"]["sent"], true);
    assert_eq!(
        call(&e, Method::GET, "/api/crm/emails?deal_id=701", json!({}))
            .await
            .1,
        json!([])
    );
}

#[tokio::test]
async fn actual_gmail_rfc_message_id_is_used_for_hubspot_not_the_gmail_resource_id() {
    let e = env(true).await;
    e.fake.lock().unwrap().metadata_id = Some("<google-actual-101@example.com>".into());
    assert_eq!(send(&e, &request()).await.0, 200);
    let f = e.fake.lock().unwrap();
    assert_eq!(
        f.created[0]["properties"]["hs_email_message_id"],
        "<google-actual-101@example.com>"
    );
    assert_eq!(
        f.searches[0]["filterGroups"][0]["filters"][0]["propertyName"],
        "hs_email_message_id"
    );
    assert_eq!(
        f.searches[0]["filterGroups"][0]["filters"][0]["value"],
        "<google-actual-101@example.com>"
    );
}
#[tokio::test]
async fn lost_send_response_is_reconciled_via_readonly_never_resent() {
    let e = env(true).await;
    e.fake.lock().unwrap().lose_send = true;
    let (s, v) = send(&e, &request()).await;
    assert_eq!(s, 202);
    assert_eq!(v["status"], "pending");
    assert_eq!(v["sent"], false);
    let r = row(&e).await;
    assert!(crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await);
    assert_eq!(row(&e).await.status, "saved");
    assert_eq!(send(&e, &request()).await.0, 200);
    let f = e.fake.lock().unwrap();
    assert_eq!(f.send_attempts, 1);
    assert_eq!(f.sends, 1);
    assert_eq!(f.created.len(), 1);
}
#[tokio::test]
async fn ledger_failure_before_acceptance_sends_nothing_and_after_send_recovers_without_resending()
{
    let e = env(true).await;
    e.conn.lock().unwrap().execute_batch("CREATE TRIGGER fail_accept BEFORE INSERT ON crm_pending_operations BEGIN SELECT RAISE(FAIL, 'test-unavailable'); END;").unwrap();
    assert_eq!(send(&e, &request()).await.0, 503);
    assert_eq!(e.fake.lock().unwrap().send_attempts, 0);
    e.conn.lock().unwrap().execute_batch("DROP TRIGGER fail_accept; CREATE TRIGGER fail_update BEFORE UPDATE ON crm_pending_operations BEGIN SELECT RAISE(FAIL, 'test-unavailable'); END;").unwrap();
    assert_eq!(send(&e, &request()).await.0, 503);
    assert_eq!(e.fake.lock().unwrap().sends, 1);
    e.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER fail_update;")
        .unwrap();
    assert_eq!(send(&e, &request()).await.0, 202);
    let r = row(&e).await;
    crate::crm::write::process_op(&e.audit, &e.client, &e.write, &r).await;
    assert_eq!(row(&e).await.status, "saved");
    let f = e.fake.lock().unwrap();
    assert_eq!(f.send_attempts, 1);
    assert_eq!(f.created.len(), 1);
}
